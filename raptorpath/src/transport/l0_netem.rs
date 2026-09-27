//! The L0 netem shim: per-path rate/delay/jitter/Gilbert-Elliott shaping
//! inside the transport's datagram send path, for in-process loopback tests.
//! `transport/quic.rs` keeps the one call seam (`QuicTransport`'s datagram
//! send); the design note follows.

use crate::scheduler::PathId;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

// ───────────────────────────────────────────────────────────────────────────
// L0 netem shim (env `RWM_L0_NETEM`, default off ⇒ byte-identical shipped
// path). Emulates the L1 harness's per-path netem qdisc (rate + delay +
// jitter + Gilbert-Elliott loss) inside the transport's datagram send path so
// the in-process loopback tests (tests/perf_loopback.rs and the gen-substrate
// L0 bench) can reproduce the L1 window/RTT/loss dynamics locally.
//
//   RWM_L0_NETEM=c2        every path shaped like the L1 `c2` scenario
//   RWM_L0_NETEM=c2,c3     path 0 = c2, path 1 = c3 (the C8 topology)
//   RWM_L0_SEED=42         GE/jitter RNG seed (default 42)
//
// Semantics mirror tools/l1/topo_dual.sh: rate+delay+jitter shape both
// directions; GE loss applies only to the client egress (the bulk-data
// direction — topo_dual shapes loss on the cli qdiscs only). FIFO release
// (rate stage then delay stage, monotonic per path — netem with a rate does
// not reorder), tail-drop at the netem default 1000-packet limit.
//
// Fidelity boundary: drops/delay happen before quinn, so quinn's own
// congestion controller sees a clean sub-ms loopback. This shim reproduces
// the raptorpath-layer dynamics (flow windows, pacing, deficit rounds); it
// deliberately does not reproduce quinn-internal CC behaviour under loss —
// if L1 measures a wall the shim cannot, the residual is quinn-level.
#[derive(Clone, Copy, Debug)]
struct L0PathCfg {
    rate_bps: f64,
    delay_us: u64,
    jitter_us: u64,
    ge_p: f64, // P(good→bad) per packet (heavy-tail mode: burst-onset prob)
    ge_q: f64, // P(bad→good) per packet; bad state drops (h=1)
    // #85 heavy-tail loss (the semi-Markov synthetic of
    // raptorpath-math/tests/rstar_tail_validation.rs): geometric Good
    // sojourns (onset = ge_p), discrete-Weibull(theta, k) Bad sojourns by
    // inverse transform — the burst-tail structure netem `gemodel` (GE)
    // cannot express, which is why this shim is the local rung for the
    // heavy-tail claim (paper §4.3). wb_k = 0 ⇒ plain GE (byte-identical).
    wb_theta: f64,
    wb_k: f64,
}

fn l0_scenario(name: &str) -> Option<L0PathCfg> {
    // Mirrors tools/l1/lib.sh scenario_params: rate one_way jitter ge_p ge_q.
    let f = |rate_mbit: f64, ow_ms: u64, jit_ms: u64, p: f64, q: f64| L0PathCfg {
        rate_bps: rate_mbit * 1e6,
        delay_us: ow_ms * 1000,
        jitter_us: jit_ms * 1000,
        ge_p: p / 100.0,
        ge_q: q / 100.0,
        wb_theta: 0.0,
        wb_k: 0.0,
    };
    // Heavy-tail semi-Markov: p = burst-onset %, Weibull(theta, k) bursts.
    let h = |rate_mbit: f64, ow_ms: u64, jit_ms: u64, p: f64, theta: f64, k: f64| L0PathCfg {
        rate_bps: rate_mbit * 1e6,
        delay_us: ow_ms * 1000,
        jitter_us: jit_ms * 1000,
        ge_p: p / 100.0,
        ge_q: 0.0,
        wb_theta: theta,
        wb_k: k,
    };
    match name.trim() {
        "c2" | "wifi" => Some(f(100.0, 5, 3, 1.3, 50.0)),
        "c3" | "lte" => Some(f(20.0, 20, 5, 2.0, 40.0)),
        // #85: c3's rate/RTT/jitter shape with a heavy-tail burst law
        // (Weibull k = 0.5, theta = 0.55 ⇒ E[burst] = 6.2). Onset
        // 1.0% ⇒ eps ≈ 5.8% — LTE-class average like c3's 4.8% but with the
        // burst tail GE cannot represent. (Onset 2.3% ⇒ eps = 12.5% is
        // reachable via heavy:20;20;5;2.3;0.55;0.5 — too deep for a
        // per-object delivered-reliability observable: at 12.5% heavy-tail
        // every 100 KB realtime object dies in every arm.)
        "c3heavy" => Some(h(20.0, 20, 5, 1.0, 0.55, 0.5)),
        "clean" => Some(f(100.0, 5, 0, 0.0, 100.0)),
        other => {
            if let Some(spec) = other.strip_prefix("heavy:") {
                // heavy:rate_mbit;ow_ms;jit_ms;onset_pct;theta;k
                let v: Vec<f64> = spec.split(';').filter_map(|s| s.parse().ok()).collect();
                if v.len() == 6 {
                    return Some(h(v[0], v[1] as u64, v[2] as u64, v[3], v[4], v[5]));
                }
                return None;
            }
            // custom:rate_mbit,ow_ms,jit_ms,ge_p,ge_q
            let spec = other.strip_prefix("custom:")?;
            let v: Vec<f64> = spec.split(';').filter_map(|s| s.parse().ok()).collect();
            if v.len() == 5 {
                Some(f(v[0], v[1] as u64, v[2] as u64, v[3], v[4]))
            } else {
                None
            }
        }
    }
}

/// SplitMix64 — deterministic, dependency-free RNG for the shim.
fn l0_rand(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z = z ^ (z >> 31);
    (z >> 11) as f64 / (1u64 << 53) as f64
}

struct L0PathState {
    ge_bad: bool,
    /// #85 heavy-tail mode: packets left in the current Weibull burst.
    wb_bad_left: u64,
    rng: u64,
    link_free_at_us: u64,
    last_release_us: u64,
    queued: Arc<std::sync::atomic::AtomicUsize>,
    tx: Option<mpsc::UnboundedSender<(u64, bytes::Bytes)>>,
}

pub(super) struct L0Netem {
    cfgs: Vec<L0PathCfg>,
    states: DashMap<PathId, parking_lot::Mutex<L0PathState>>,
    epoch: std::time::Instant,
    seed: u64,
    // Transit counters (RWM_DIAG reads them; always-on
    // atomics, negligible cost): where do packets die during an outage?
    enq: std::sync::atomic::AtomicU64,
    ge_drops: std::sync::atomic::AtomicU64,
    tail_drops: std::sync::atomic::AtomicU64,
    sent_ok: std::sync::atomic::AtomicU64,
    send_errs: std::sync::atomic::AtomicU64,
}

impl L0Netem {
    pub(super) fn from_env() -> Option<Arc<Self>> {
        let spec = crate::gates::get().l0_netem.clone()?;
        if spec.trim().is_empty() {
            return None;
        }
        let cfgs: Vec<L0PathCfg> = spec.split(',').filter_map(l0_scenario).collect();
        if cfgs.is_empty() {
            warn!(%spec, "RWM_L0_NETEM set but no scenario parsed — shim OFF");
            return None;
        }
        let seed: u64 = crate::gates::get()
            .l0_seed_raw
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);
        info!(?cfgs, seed, "L0 netem shim ACTIVE on the datagram path");
        Some(Arc::new(Self {
            cfgs,
            states: DashMap::new(),
            epoch: std::time::Instant::now(),
            seed,
            enq: std::sync::atomic::AtomicU64::new(0),
            ge_drops: std::sync::atomic::AtomicU64::new(0),
            tail_drops: std::sync::atomic::AtomicU64::new(0),
            sent_ok: std::sync::atomic::AtomicU64::new(0),
            send_errs: std::sync::atomic::AtomicU64::new(0),
        }))
    }

    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    fn cfg(&self, path_id: PathId) -> L0PathCfg {
        let i = (path_id as usize).min(self.cfgs.len() - 1);
        self.cfgs[i]
    }

    /// Shape + (maybe) drop + schedule one datagram for delayed send.
    pub(super) fn send(self: &Arc<Self>, path_id: PathId, is_server: bool, conn: &quinn::Connection, data: bytes::Bytes) {
        let cfg = self.cfg(path_id);
        let now = self.now_us();
        let entry = self.states.entry(path_id).or_insert_with(|| {
            parking_lot::Mutex::new(L0PathState {
                ge_bad: false,
                wb_bad_left: 0,
                rng: self.seed ^ ((path_id as u64 + 1) * 0x9E37) ^ ((is_server as u64) << 32),
                link_free_at_us: 0,
                last_release_us: 0,
                queued: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                tx: None,
            })
        });
        let mut st = entry.lock();
        // GE loss on the bulk-data direction only (client egress), like the
        // L1 topo (loss on the cli qdiscs; the ack direction is clean).
        if !is_server && cfg.ge_p > 0.0 {
            let drop = if cfg.wb_k > 0.0 {
                // #85 heavy-tail semi-Markov (see L0PathCfg): geometric Good
                // sojourns, discrete-Weibull(theta, k) Bad sojourns drawn by
                // inverse transform B = ceil((ln U / ln theta)^(1/k)) — the
                // same generator as rstar_tail_validation.rs.
                if st.wb_bad_left > 0 {
                    st.wb_bad_left -= 1;
                    true
                } else {
                    let u = l0_rand(&mut st.rng);
                    if u < cfg.ge_p {
                        let uu = l0_rand(&mut st.rng).max(1e-300);
                        let b = (uu.ln() / cfg.wb_theta.ln())
                            .powf(1.0 / cfg.wb_k)
                            .ceil()
                            .max(1.0)
                            .min(10_000.0) as u64;
                        st.wb_bad_left = b - 1; // this packet is the burst's first
                        true
                    } else {
                        false
                    }
                }
            } else {
                let drop = st.ge_bad;
                let u = l0_rand(&mut st.rng);
                if st.ge_bad {
                    if u < cfg.ge_q {
                        st.ge_bad = false;
                    }
                } else if u < cfg.ge_p {
                    st.ge_bad = true;
                }
                drop
            };
            if drop {
                self.ge_drops.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        }
        // netem default packet limit: tail-drop beyond 1000 queued.
        if st.queued.load(std::sync::atomic::Ordering::Relaxed) >= 1000 {
            self.tail_drops.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        // Rate stage (serialization through the shaped link), then delay+jitter.
        let ser_us = (data.len() as f64 * 8.0 / cfg.rate_bps * 1e6) as u64;
        let start = now.max(st.link_free_at_us);
        st.link_free_at_us = start + ser_us;
        let jitter = if cfg.jitter_us > 0 {
            let u = l0_rand(&mut st.rng) * 2.0 - 1.0;
            (u * cfg.jitter_us as f64) as i64
        } else {
            0
        };
        let mut release =
            (st.link_free_at_us as i64 + cfg.delay_us as i64 + jitter).max(0) as u64;
        // FIFO (a netem rate stage does not reorder).
        release = release.max(st.last_release_us);
        st.last_release_us = release;
        // Lazily spawn the per-path forwarder that sleeps until each packet's
        // release time and performs the real quinn send.
        if st.tx.is_none() {
            let (tx, mut rx) = mpsc::unbounded_channel::<(u64, bytes::Bytes)>();
            let conn = conn.clone();
            let epoch = self.epoch;
            let queued = st.queued.clone();
            let shim = self.clone();
            tokio::spawn(async move {
                while let Some((rel_us, data)) = rx.recv().await {
                    let now = epoch.elapsed().as_micros() as u64;
                    if rel_us > now {
                        tokio::time::sleep(std::time::Duration::from_micros(rel_us - now)).await;
                    }
                    queued.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    match conn.send_datagram(data) {
                        Ok(()) => {
                            shim.sent_ok.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(_) => {
                            shim.send_errs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            });
            st.tx = Some(tx);
        }
        st.queued.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.enq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = st.tx.as_ref().unwrap().send((release, data));
    }

    /// Cumulative transit counters + current queue
    /// depth: (enq, ge_drops, tail_drops, sent_ok, send_errs, queued_now).
    pub(super) fn transit_stats(&self) -> (u64, u64, u64, u64, u64, usize) {
        use std::sync::atomic::Ordering::Relaxed;
        let q: usize = self
            .states
            .iter()
            .map(|e| e.value().lock().queued.load(Relaxed))
            .sum();
        (
            self.enq.load(Relaxed),
            self.ge_drops.load(Relaxed),
            self.tail_drops.load(Relaxed),
            self.sent_ok.load(Relaxed),
            self.send_errs.load(Relaxed),
            q,
        )
    }
}
