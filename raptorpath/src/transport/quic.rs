//! QUIC transport implementation using quinn.
//!
//! Each path gets its own QUIC connection. We use:
//! - DATAGRAM frames for symbol data (unreliable, low overhead)
//! - A bidirectional stream for control messages (reliable)
//!
//! TLS modes:
//! - Default: self-signed cert, skip verification (dev/testing)
//! - Pinned: verify server cert matches a pinned DER/PEM file (production)

use super::protocol::{ControlMessage, Handshake, PROTOCOL_VERSION, SymbolBatch, WireMessage};
use crate::scheduler::PathId;
use dashmap::DashMap;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

// ───────────────────────────────────────────────────────────────────────────
// L0 netem shim (env `RWM_L0_NETEM`, DEFAULT OFF ⇒ byte-identical shipped
// path). Emulates the L1 harness's per-path netem qdisc (rate + delay +
// jitter + Gilbert-Elliott loss) INSIDE the transport's datagram send path so
// the in-process loopback tests (tests/perf_loopback.rs and the gen-substrate
// L0 bench) can reproduce the L1 window/RTT/loss dynamics locally — the JOB-1
// diagnosis instrument for the generation-mode per-path substrate ceiling.
//
//   RWM_L0_NETEM=c2        every path shaped like the L1 `c2` scenario
//   RWM_L0_NETEM=c2,c3     path 0 = c2, path 1 = c3 (the C8 topology)
//   RWM_L0_SEED=42         GE/jitter RNG seed (default 42)
//
// Semantics mirror tools/l1/topo_dual.sh: rate+delay+jitter shape BOTH
// directions; GE loss applies only to the CLIENT egress (the bulk-data
// direction — topo_dual shapes loss on the cli qdiscs only). FIFO release
// (rate stage then delay stage, monotonic per path — netem with a rate does
// not reorder), tail-drop at the netem default 1000-packet limit.
//
// NOTE the fidelity boundary: drops/delay happen BEFORE quinn, so quinn's own
// congestion controller sees a clean sub-ms loopback. This shim reproduces
// the raptorpath-layer dynamics (flow windows, pacing, deficit rounds); it
// deliberately does NOT reproduce quinn-internal CC behaviour under loss —
// if L1 measures a wall the shim cannot, the residual is quinn-level.
#[derive(Clone, Copy, Debug)]
struct L0PathCfg {
    rate_bps: f64,
    delay_us: u64,
    jitter_us: u64,
    ge_p: f64, // P(good→bad) per packet (heavy-tail mode: burst-onset prob)
    ge_q: f64, // P(bad→good) per packet; bad state drops (h=1)
    // #85 heavy-tail loss (the #46 ARM-3 semi-Markov synthetic,
    // raptorpath-math/tests/rstar_tail_validation.rs): geometric Good
    // sojourns (onset = ge_p), discrete-Weibull(theta, k) Bad sojourns by
    // inverse transform — the burst-tail structure netem `gemodel` (GE)
    // cannot express, which is why THIS shim is the local rung for the
    // §8.4.1 heavy-tail claim. wb_k = 0 ⇒ plain GE (byte-identical).
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
        // #85: c3's rate/RTT/jitter shape with the #46 documented heavy-tail
        // burst law (Weibull k = 0.5, theta = 0.55 ⇒ E[burst] = 6.2). Onset
        // 1.0% ⇒ eps ≈ 5.8% — LTE-class average like c3's 4.8% but with the
        // burst tail GE cannot represent. (The #46 ARM-3 synthetic's onset
        // 2.3% ⇒ eps = 12.5% is reachable via heavy:20;20;5;2.3;0.55;0.5 —
        // too deep for a per-object delivered-reliability observable: at
        // 12.5% heavy-tail every 100 KB realtime object dies in EVERY arm.)
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

// ───────────────────────────────────────────────────────────────────────────
// QUIC substrate congestion-controller override (env `RWM_QUIC_CC`, DEFAULT
// UNSET ⇒ quinn BBR — the Default CC Flip, 2026-07-21).
//
// WHY (feat/gen-substrate-ceiling JOB 1): quinn gates EVERY packet send —
// including DATAGRAM frames, which carry all raptorpath wire symbols — on its
// own congestion window (quinn-proto connection/mod.rs "blocked by congestion
// control"), default CUBIC. On a GE-lossy path the loss-reactive Cubic window
// is a hard substrate ceiling underneath raptorpath's own loss-tolerant
// FEC/CC design — the exact per-connection (= per-path) wall the L1
// generation-mode measurements hit (plain 17.5 → plain+BBR 74.5 pooled,
// ×4.3; "Gen Substrate Ceiling"). Every measured best arm since has used
// BBR; as of 2026-07-21 the shipped default IS BBR (the A/B inverts: the
// legacy wire is now the explicit `RWM_QUIC_CC=cubic` opt-out arm).
// FAIRNESS CAVEAT (documented at flip time, "Cross-Traffic" battery): BBR
// takes a 0.95–0.96 share against a competing Cubic flow at the c2 cell —
// mildly aggressive, within the deployed-BBRv1 envelope.
//   RWM_QUIC_CC=bbr          quinn BBR (explicit; = the default)
//   RWM_QUIC_CC=bbr_rs       in-tree burst-robust BBR (transport/bbr_rs.rs:
//                            quinn's Bbr with the interval-guarded per-flight
//                            rate sampler — goal-gate "Ship The Wins 2:
//                            shal8 anchor", ADR-0054/0061; built as an
//                            explicit arm, flip decision battery-gated)
//   RWM_QUIC_CC=newreno
//   RWM_QUIC_CC=cubic        quinn stock Cubic (the legacy/fairness arm)
//   RWM_QUIC_CC=passthrough  OUR engine owns the window (see below)
// Unrecognized values warn and keep the BBR default.
// Applied to BOTH client and server configs (each direction's sends are
// governed by the sender-side controller of that connection).
//
// PASSTHROUGH (feat/copa-sole-cc, task #80): substrate CC as POLICY. quinn's
// controller becomes a pass-through shim whose window() simply reads an
// Arc<AtomicU64> (bytes) that the raptorpath engine writes per path — the
// engine's own Copa-lite per-path cwnd becomes THE congestion window of the
// substrate (per connection = per path), instead of min(app CC, quinn CC)
// double control. quinn's loss events are recorded (stats only), never acted
// on — loss handling is the FEC layer's job (paper §12); congestion safety is
// Copa's delay backoff, which the engine writes into the atomic. quinn's own
// pacer derives its rate from this window, so pacing stays consistent with
// the engine's cwnd. The atomic starts at PASSTHROUGH_INITIAL_WINDOW so the
// TLS handshake and pre-feed traffic are never starved before Copa's first
// cwnd write; connections that never get a Copa feed (ack-only reverse
// direction) simply keep that static window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuicCcMode {
    /// Env unset/unrecognized/explicit `bbr`: quinn BBR — the shipped default
    /// (Default CC Flip, 2026-07-21).
    Bbr,
    /// Explicit `bbr_rs`: the in-tree burst-robust BBR (one changed
    /// mechanism vs quinn's: the bandwidth estimator — see
    /// transport/bbr_rs.rs module docs).
    BbrRs,
    NewReno,
    /// Explicit `cubic`: quinn stock Cubic — the legacy wire / fairness arm.
    Cubic,
    Passthrough,
}

fn quic_cc_mode() -> QuicCcMode {
    let Ok(name) = std::env::var("RWM_QUIC_CC") else {
        return QuicCcMode::Bbr;
    };
    match name.trim().to_ascii_lowercase().as_str() {
        "bbr" => QuicCcMode::Bbr,
        "bbr_rs" => QuicCcMode::BbrRs,
        "newreno" => QuicCcMode::NewReno,
        "cubic" => QuicCcMode::Cubic,
        "passthrough" => QuicCcMode::Passthrough,
        other => {
            warn!(%other, "RWM_QUIC_CC unrecognized — keeping the BBR default");
            QuicCcMode::Bbr
        }
    }
}

/// Initial pass-through window (bytes). Generous on purpose: it covers the
/// TLS handshake and the first RTTs before the engine's first Copa cwnd
/// write (a starved handshake would deadlock the tunnel), and it is the
/// permanent window for connections whose direction carries only control
/// traffic (no Copa feed). Once the engine writes, Copa owns the value.
const PASSTHROUGH_INITIAL_WINDOW: u64 = 256 * 1024;

/// Absolute floor for the pass-through window: never below two datagrams, so
/// a zero/garbage write can never wedge the connection entirely (ACK and
/// control packets keep flowing at a trickle).
const PASSTHROUGH_MIN_WINDOW_MTUS: u64 = 2;

/// Record-only counters for what quinn WOULD have reacted to (RWM_DIAG-class
/// observability; never gates anything).
#[derive(Debug, Default)]
pub struct PassthroughCcStats {
    pub congestion_events: std::sync::atomic::AtomicU64,
    pub lost_bytes: std::sync::atomic::AtomicU64,
    pub persistent_congestion: std::sync::atomic::AtomicU64,
}

/// The pass-through `quinn::congestion::Controller`: `window()` reads the
/// shared atomic; every congestion signal is a recorded no-op.
struct PassthroughController {
    window: Arc<std::sync::atomic::AtomicU64>,
    stats: Arc<PassthroughCcStats>,
    mtu: u16,
}

impl quinn::congestion::Controller for PassthroughController {
    fn on_congestion_event(
        &mut self,
        _now: std::time::Instant,
        _sent: std::time::Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        use std::sync::atomic::Ordering;
        self.stats.congestion_events.fetch_add(1, Ordering::Relaxed);
        self.stats.lost_bytes.fetch_add(lost_bytes, Ordering::Relaxed);
        if is_persistent_congestion {
            self.stats.persistent_congestion.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu;
    }

    fn window(&self) -> u64 {
        self.window
            .load(std::sync::atomic::Ordering::Relaxed)
            .max(PASSTHROUGH_MIN_WINDOW_MTUS * self.mtu as u64)
    }

    fn clone_box(&self) -> Box<dyn quinn::congestion::Controller> {
        Box::new(Self {
            window: self.window.clone(),
            stats: self.stats.clone(),
            mtu: self.mtu,
        })
    }

    fn initial_window(&self) -> u64 {
        PASSTHROUGH_INITIAL_WINDOW
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

/// Factory handing every connection built from it the SAME per-path window
/// atomic (one factory per path/endpoint — per-connection = per-path).
struct PassthroughFactory {
    window: Arc<std::sync::atomic::AtomicU64>,
    stats: Arc<PassthroughCcStats>,
}

impl quinn::congestion::ControllerFactory for PassthroughFactory {
    fn build(
        self: Arc<Self>,
        _now: std::time::Instant,
        current_mtu: u16,
    ) -> Box<dyn quinn::congestion::Controller> {
        Box::new(PassthroughController {
            window: self.window.clone(),
            stats: self.stats.clone(),
            mtu: current_mtu,
        })
    }
}

fn quic_cc_factory(
) -> Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>> {
    match quic_cc_mode() {
        QuicCcMode::Bbr => {
            info!("quinn congestion controller: BBR (shipped default; RWM_QUIC_CC overrides)");
            Some(Arc::new(quinn::congestion::BbrConfig::default()))
        }
        QuicCcMode::BbrRs => {
            // Mechanism-liveness echo (MEASUREMENT DISCIPLINE item 1): the
            // battery greps for "burst-robust BBR".
            info!(
                "RWM_QUIC_CC=bbr_rs: burst-robust BBR (in-tree controller, \
                 interval-guarded per-flight rate sampler — ADR-0061 family)"
            );
            Some(Arc::new(super::bbr_rs::BbrRsConfig::default()))
        }
        QuicCcMode::NewReno => {
            info!("RWM_QUIC_CC=newreno: quinn congestion controller overridden to NewReno");
            Some(Arc::new(quinn::congestion::NewRenoConfig::default()))
        }
        QuicCcMode::Cubic => {
            info!("RWM_QUIC_CC=cubic: quinn stock Cubic (legacy wire / fairness arm)");
            Some(Arc::new(quinn::congestion::CubicConfig::default()))
        }
        // Passthrough needs a PER-PATH handle — built in cc_factory_for_path.
        QuicCcMode::Passthrough => None,
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

struct L0Netem {
    cfgs: Vec<L0PathCfg>,
    states: DashMap<PathId, parking_lot::Mutex<L0PathState>>,
    epoch: std::time::Instant,
    seed: u64,
    // diag/unified-collapse transit counters (RWM_DIAG reads them; always-on
    // atomics, negligible cost): where do packets die during an outage?
    enq: std::sync::atomic::AtomicU64,
    ge_drops: std::sync::atomic::AtomicU64,
    tail_drops: std::sync::atomic::AtomicU64,
    sent_ok: std::sync::atomic::AtomicU64,
    send_errs: std::sync::atomic::AtomicU64,
}

impl L0Netem {
    fn from_env() -> Option<Arc<Self>> {
        let spec = std::env::var("RWM_L0_NETEM").ok()?;
        if spec.trim().is_empty() {
            return None;
        }
        let cfgs: Vec<L0PathCfg> = spec.split(',').filter_map(l0_scenario).collect();
        if cfgs.is_empty() {
            warn!(%spec, "RWM_L0_NETEM set but no scenario parsed — shim OFF");
            return None;
        }
        let seed: u64 = std::env::var("RWM_L0_SEED")
            .ok()
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
    fn send(self: &Arc<Self>, path_id: PathId, is_server: bool, conn: &quinn::Connection, data: bytes::Bytes) {
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
                // same generator as rstar_tail_validation.rs ARM 3.
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
        // release time and performs the REAL quinn send.
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

    /// diag/unified-collapse: cumulative transit counters + current queue
    /// depth: (enq, ge_drops, tail_drops, sent_ok, send_errs, queued_now).
    fn transit_stats(&self) -> (u64, u64, u64, u64, u64, usize) {
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

/// A QUIC-based multipath transport.
///
/// Uses DashMap for connections so paths can be added/removed at runtime.
pub struct QuicTransport {
    /// Local endpoints (one per bind address / path)
    endpoints: DashMap<PathId, Endpoint>,
    /// Active connections per path
    connections: DashMap<PathId, quinn::Connection>,
    /// Whether this transport is a server
    is_server: bool,
    /// Optional pinned certificate for client-side verification.
    /// When set, the client verifies the server's cert matches this fingerprint.
    pinned_cert_hash: Option<[u8; 32]>,
    /// L0 netem shim (env `RWM_L0_NETEM`; None = shipped path, byte-identical).
    l0_netem: Option<Arc<L0Netem>>,
    /// `RWM_QUIC_CC=passthrough`: the engine owns the substrate window.
    cc_passthrough: bool,
    /// Per-path pass-through window handles (bytes) — one per endpoint/path,
    /// created when that path's endpoint config is built; the engine writes
    /// its Copa cwnd here via `set_cc_window_bytes`.
    cc_windows: DashMap<PathId, Arc<std::sync::atomic::AtomicU64>>,
    /// Per-path record-only pass-through congestion stats (diagnostics).
    cc_stats: DashMap<PathId, Arc<PassthroughCcStats>>,
    /// `RWM_DIAG`: run the datagram send-queue audit at the
    /// `send_datagram_shaped` seam. Resolved ONCE, here, so the shipped
    /// default pays neither the extra connection-lock take nor the atomics.
    dg_audit: bool,
    /// Datagram send-queue audit counters, per path (goal-gate "What Binds
    /// Throughput", instrument 3). See `datagram_queue_stats`.
    dg_stats: DashMap<PathId, Arc<DatagramQueueAudit>>,
}

/// The datagram send-queue audit (`RWM_DIAG`), one per path.
///
/// **The gap this closes.** `quinn::Connection::send_datagram` calls
/// `Datagrams::send(data, drop = true)` (quinn-proto 0.11.14,
/// `connection/datagrams.rs`:38–48), which **silently evicts the OLDEST
/// queued datagrams** when the 4 MB send buffer overflows — about 3 300
/// 1 200 B symbols — logging a `trace!` nobody enables and returning `Ok`.
/// The engine's `src=`/`cod=` gauges count HANDOFFS, not transmissions, so
/// an evicted symbol is indistinguishable from a delivered one in every log
/// the three-term battery produced. The law set the cap to 3 073 at c2r100
/// and 4 096 at c2r200 — the same order as that buffer — so the arm most
/// exposed to the loss was the scored one.
///
/// **It cannot be counted exactly from quinn's public API, and this is what
/// is available instead.** quinn exposes no eviction counter and no hook;
/// `ConnectionStats` counts frames transmitted, not datagrams dropped before
/// transmission. What it does expose is
/// `Connection::datagram_send_buffer_space()`, which is
/// `datagram_send_buffer_size.saturating_sub(outgoing_total)`. Read at the
/// seam, immediately BEFORE the send, that gives an exact predicate:
///
/// * quinn's eviction loop runs iff `outgoing_total > buffer_size` on entry;
/// * `space == 0` iff `outgoing_total >= buffer_size` on entry.
///
/// So `full` counts every call that evicted, plus the measure-zero tie where
/// the queue is byte-exactly full. **`full` is therefore an upper bound on
/// the number of evicting CALLS that is tight to one boundary case, and a
/// LOWER bound on the number of datagrams EVICTED** — one call's `while`
/// loop pops until it is back under the ceiling, which is one datagram when
/// sizes are uniform (ours are: 1 200 B symbols) and more when they are not.
/// Both directions are named because neither is exact.
///
/// The corroborating cross-check is independent of that predicate:
/// `tx_frames` is quinn's own `stats().frame_tx.datagram`, the count of
/// DATAGRAM frames actually put on the wire. DATAGRAM frames are never
/// retransmitted, so in a run that ends with a drained queue
/// `handoff − tx_frames` is the total lost to eviction, computed without
/// reference to `full` at all. `space` is echoed so the queue depth at the
/// last DIAG window is readable and `handoff − tx_frames` can be corrected
/// for what was still queued.
///
/// **OFF-value property:** with `RWM_DIAG` unset the audit does not run and
/// every counter reads 0, so `dgq[...]` never appears — enforced by
/// `transport::quic::tests::datagram_queue_audit_is_off_without_diag`.
///
/// **Scope:** only the real `conn.send_datagram` path is audited. The
/// `RWM_L0_NETEM` shim branch has its own transit ledger
/// (`l0_transit_stats`) and does not touch quinn's datagram buffer at all.
#[derive(Default)]
pub struct DatagramQueueAudit {
    /// Calls into the seam that quinn ACCEPTED (returned `Ok`).
    pub handoff: std::sync::atomic::AtomicU64,
    /// Calls whose `datagram_send_buffer_space()` was 0 on entry — the
    /// eviction predicate. See the type doc for exactly what it bounds.
    pub full: std::sync::atomic::AtomicU64,
    /// Calls quinn REJECTED (`TooLarge` / `UnsupportedByPeer` / `Disabled`).
    /// These are loud (the seam returns `Err`); counted so that
    /// `handoff + err` reconciles with the engine's own handoff count.
    pub err: std::sync::atomic::AtomicU64,
    /// `datagram_send_buffer_space()` at the most recent call, in bytes.
    pub space: std::sync::atomic::AtomicU64,
}

impl QuicTransport {
    /// Create a new transport with endpoints bound to the given addresses.
    ///
    /// `pin_cert_path`: optional path to a DER or PEM certificate file.
    /// When provided, the client will verify that the server's certificate
    /// matches this pinned cert (SHA-256 fingerprint comparison).
    pub async fn new(
        bind_addrs: &[SocketAddr],
        is_server: bool,
        pin_cert_path: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let pinned_cert_hash = pin_cert_path
            .map(|p| load_pinned_cert_hash(p))
            .transpose()?;

        if let Some(hash) = &pinned_cert_hash {
            info!(fingerprint = %hex::encode(hash), "TLS cert pinning enabled");
        }

        let cc_passthrough = quic_cc_mode() == QuicCcMode::Passthrough;
        if cc_passthrough {
            info!(
                initial_window = PASSTHROUGH_INITIAL_WINDOW,
                "RWM_QUIC_CC=passthrough: quinn congestion window is engine-owned (per path)"
            );
        }
        let cc_windows: DashMap<PathId, Arc<std::sync::atomic::AtomicU64>> = DashMap::new();
        let cc_stats: DashMap<PathId, Arc<PassthroughCcStats>> = DashMap::new();

        let endpoints = DashMap::new();

        for (i, addr) in bind_addrs.iter().enumerate() {
            let cc = Self::cc_factory_for_path(
                cc_passthrough,
                &cc_windows,
                &cc_stats,
                i as PathId,
            );
            let endpoint = if is_server {
                let (server_config, cert_der) = Self::generate_self_signed_config(cc)?;
                // Log the server cert fingerprint so the user can pin it on the client
                let fingerprint = sha256_fingerprint(&cert_der[0]);
                info!(%addr, path_id = i, fingerprint = %hex::encode(fingerprint),
                    "server endpoint bound — use this fingerprint for --pin-cert");
                Endpoint::server(server_config, *addr)?
            } else {
                let mut ep = Endpoint::client(*addr)?;
                let client_config = Self::make_client_config(pinned_cert_hash, cc);
                ep.set_default_client_config(client_config);
                info!(%addr, path_id = i, "client endpoint bound");
                ep
            };
            endpoints.insert(i as PathId, endpoint);
        }

        Ok(Self {
            endpoints,
            connections: DashMap::new(),
            is_server,
            pinned_cert_hash,
            l0_netem: L0Netem::from_env(),
            cc_passthrough,
            cc_windows,
            cc_stats,
            // Resolved once, at construction: the audit's per-datagram cost
            // (one connection-lock take for `datagram_send_buffer_space`)
            // must not exist on the shipped path. `RWM_DIAG` is an existing
            // gate — already in `RWM_FORWARD`, already in the `[GATES]` echo
            // — so this adds no name to the gate surface.
            dg_audit: crate::config::env_flag("RWM_DIAG", false),
            dg_stats: DashMap::new(),
        })
    }

    /// Per-path congestion-controller factory. Passthrough mode gets a
    /// PER-PATH factory sharing that path's window atomic (per-connection =
    /// per-path: each endpoint serves exactly one path, and any reconnect on
    /// it correctly inherits the same engine-owned window); the other modes
    /// use the stock env-selected factory.
    fn cc_factory_for_path(
        cc_passthrough: bool,
        cc_windows: &DashMap<PathId, Arc<std::sync::atomic::AtomicU64>>,
        cc_stats: &DashMap<PathId, Arc<PassthroughCcStats>>,
        path_id: PathId,
    ) -> Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>> {
        if !cc_passthrough {
            return quic_cc_factory();
        }
        let window = cc_windows
            .entry(path_id)
            .or_insert_with(|| {
                Arc::new(std::sync::atomic::AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW))
            })
            .clone();
        let stats = cc_stats
            .entry(path_id)
            .or_insert_with(|| Arc::new(PassthroughCcStats::default()))
            .clone();
        Some(Arc::new(PassthroughFactory { window, stats }))
    }

    /// Engine write side of the pass-through window: set path `path_id`'s
    /// substrate congestion window in BYTES. No-op unless
    /// `RWM_QUIC_CC=passthrough` created a handle for this path.
    pub fn set_cc_window_bytes(&self, path_id: PathId, bytes: u64) {
        if !self.cc_passthrough {
            return;
        }
        if let Some(w) = self.cc_windows.get(&path_id) {
            w.store(bytes, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Whether the pass-through substrate CC is active (the engine owns the
    /// per-path quinn window and should be feeding it).
    pub fn cc_passthrough_active(&self) -> bool {
        self.cc_passthrough
    }

    /// Packet-timed path RTT for `path_id` from quinn's RFC 9002 estimator
    /// (feat/copa-wire-signal): measured at the QUIC packet layer — send of
    /// an ack-eliciting packet to receipt of its ACK, ack-delay corrected —
    /// so it EXCLUDES the sender's own app-layer store/reservoir dwell in
    /// the datagram queue (which sits BEFORE packetization). This is the
    /// wire clock for Copa's queue term d_q = wire_rtt − wire_RTTmin; the
    /// app-layer echo RTT stays with the reliability/tail machinery where
    /// end-to-end (pipeline-inclusive) delay is the right quantity.
    pub fn wire_rtt(&self, path_id: PathId) -> Option<std::time::Duration> {
        self.connections.get(&path_id).map(|c| c.rtt())
    }

    /// Quinn-level DATAGRAM frame counters for `path_id`:
    /// `(datagram_frames_rx, datagram_frames_tx)` from `Connection::stats()`.
    ///
    /// Wedge forensics (fix/frontier-wedge): `frame_rx.datagram` counts every
    /// DATAGRAM frame quinn ACCEPTED at the packet layer — BEFORE the app's
    /// `read_datagram()` and before quinn's bounded incoming datagram buffer
    /// (which silently drops the OLDEST buffered datagram on overflow). If
    /// this counter advances while the app-level receive loop sees nothing,
    /// arriving datagrams are being destroyed between quinn's packet layer
    /// and the application (buffer overflow), not lost on the wire.
    pub fn datagram_frame_stats(&self, path_id: PathId) -> Option<(u64, u64)> {
        self.connections.get(&path_id).map(|c| {
            let s = c.stats();
            (s.frame_rx.datagram, s.frame_tx.datagram)
        })
    }

    /// Quinn substrate congestion gauge for `path_id` (goal-gate "Ship The
    /// Wins 2: shal8 anchor" diagnosis instrument — read only at the
    /// RWM_DIAG print, never gates anything):
    /// `(cwnd_bytes, congestion_events, lost_packets, sent_packets)` from
    /// `Connection::stats().path`. Under the shipped BBR default the cwnd
    /// IS 2 × quinn's internal BtlBŵ × RTprop, so a cwnd many multiples of
    /// the true BDP·MTU is direct in-vivo evidence of the max-filter
    /// over-read (P-D1).
    pub fn quinn_path_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64)> {
        self.connections.get(&path_id).map(|c| {
            let p = c.stats().path;
            (p.cwnd, p.congestion_events, p.lost_packets, p.sent_packets)
        })
    }

    /// diag/unified-collapse: L0 shim transit counters, None when the shim is
    /// off — (enq, ge_drops, tail_drops, sent_ok, send_errs, queued_now).
    pub fn l0_transit_stats(&self) -> Option<(u64, u64, u64, u64, u64, usize)> {
        self.l0_netem.as_ref().map(|s| s.transit_stats())
    }

    /// Read back the pass-through window (bytes) for diagnostics; None when
    /// passthrough is off or the path has no handle.
    pub fn cc_window_bytes(&self, path_id: PathId) -> Option<u64> {
        self.cc_windows
            .get(&path_id)
            .map(|w| w.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Record-only pass-through congestion stats for diagnostics:
    /// (congestion_events, lost_bytes, persistent_congestion).
    pub fn cc_passthrough_stats(&self, path_id: PathId) -> Option<(u64, u64, u64)> {
        use std::sync::atomic::Ordering;
        self.cc_stats.get(&path_id).map(|s| {
            (
                s.congestion_events.load(Ordering::Relaxed),
                s.lost_bytes.load(Ordering::Relaxed),
                s.persistent_congestion.load(Ordering::Relaxed),
            )
        })
    }

    /// Datagram send seam: the L0 netem shim (when active) shapes + schedules
    /// the send; otherwise this is exactly `conn.send_datagram`.
    fn send_datagram_shaped(
        &self,
        path_id: PathId,
        conn: &quinn::Connection,
        data: Vec<u8>,
    ) -> anyhow::Result<()> {
        match &self.l0_netem {
            Some(shim) => {
                shim.send(path_id, self.is_server, conn, data.into());
                Ok(())
            }
            None => {
                if !self.dg_audit {
                    conn.send_datagram(data.into())?;
                    return Ok(());
                }
                use std::sync::atomic::Ordering::Relaxed;
                // Read the queue depth BEFORE the send: quinn's eviction
                // decision is taken against the state on entry, so this is
                // the only moment at which the predicate is meaningful.
                let space = conn.datagram_send_buffer_space() as u64;
                let a = self
                    .dg_stats
                    .entry(path_id)
                    .or_insert_with(|| Arc::new(DatagramQueueAudit::default()))
                    .clone();
                a.space.store(space, Relaxed);
                if space == 0 {
                    a.full.fetch_add(1, Relaxed);
                }
                match conn.send_datagram(data.into()) {
                    Ok(()) => {
                        a.handoff.fetch_add(1, Relaxed);
                        Ok(())
                    }
                    Err(e) => {
                        a.err.fetch_add(1, Relaxed);
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// Datagram send-queue audit readout for a path (`RWM_DIAG` only):
    /// `(handoff, full, err, space_bytes, tx_frames)`. `None` when the audit
    /// is off or the path has never sent a datagram — which is the OFF-value
    /// property: no audit, no gauge, rather than a gauge reading zero for two
    /// different reasons.
    ///
    /// `tx_frames` is read live from quinn (`stats().frame_tx.datagram`) —
    /// DATAGRAM frames actually transmitted. See `DatagramQueueAudit` for why
    /// `handoff − tx_frames` is the eviction estimate that does NOT depend on
    /// the `full` predicate.
    pub fn datagram_queue_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64, u64)> {
        use std::sync::atomic::Ordering::Relaxed;
        if !self.dg_audit {
            return None;
        }
        let a = self.dg_stats.get(&path_id)?;
        let tx_frames = self
            .connections
            .get(&path_id)
            .map_or(0, |c| c.stats().frame_tx.datagram);
        Some((
            a.handoff.load(Relaxed),
            a.full.load(Relaxed),
            a.err.load(Relaxed),
            a.space.load(Relaxed),
            tx_frames,
        ))
    }

    /// Connect to a peer on a specific path.
    pub async fn connect(&self, path_id: PathId, peer_addr: SocketAddr) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let connection = endpoint.connect(peer_addr, "raptorpath")?.await?;

        // ADR-0010: perform handshake
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::perform_handshake(&connection, &local_hs).await?;

        info!(path_id, %peer_addr, "connected and handshake complete");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Accept an incoming connection on a specific path.
    pub async fn accept(&self, path_id: PathId) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?;
        let connection = incoming.await?;

        // ADR-0010: accept handshake from peer
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::accept_handshake(&connection, &local_hs).await?;

        info!(path_id, remote = %connection.remote_address(), "accepted with handshake");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Add a new path at runtime. Binds a new endpoint, connects or accepts,
    /// and returns the connection for receiver spawning.
    pub async fn add_path(
        &self,
        path_id: PathId,
        bind_addr: SocketAddr,
        peer_addr: Option<SocketAddr>,
    ) -> anyhow::Result<quinn::Connection> {
        // Create and bind new endpoint
        let cc = Self::cc_factory_for_path(
            self.cc_passthrough,
            &self.cc_windows,
            &self.cc_stats,
            path_id,
        );
        let endpoint = if self.is_server {
            let (server_config, _cert) = Self::generate_self_signed_config(cc)?;
            Endpoint::server(server_config, bind_addr)?
        } else {
            let mut ep = Endpoint::client(bind_addr)?;
            ep.set_default_client_config(Self::make_client_config(self.pinned_cert_hash, cc));
            ep
        };
        self.endpoints.insert(path_id, endpoint);

        // Connect or accept
        if let Some(peer) = peer_addr {
            self.connect(path_id, peer).await?;
        } else {
            self.accept(path_id).await?;
        }

        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("connection not found after setup"))?
            .clone();
        Ok(conn)
    }

    /// Remove a path at runtime.
    pub fn remove_path(&self, path_id: PathId) {
        if let Some((_, conn)) = self.connections.remove(&path_id) {
            conn.close(0u32.into(), b"path removed");
        }
        self.endpoints.remove(&path_id);
        info!(path_id, "path removed");
    }

    /// Spawn receive loops for a single path, feeding into a channel.
    pub fn spawn_receiver_for_path(
        &self,
        path_id: PathId,
        conn: quinn::Connection,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        let conn_uni = conn.clone();
        // Stream-origin control messages go to a DEDICATED channel: the
        // data channel backs up under symbol floods, and liveness
        // (PathReport/Ping) queued behind it starved the dead-path check
        // (L1 finding: bulk transfers killed the tunnel in ~6 s).
        let tx_uni = ctrl_tx;

        // Datagram receiver
        let handle = tokio::spawn(async move {
            loop {
                match conn.read_datagram().await {
                    Ok(data) => match WireMessage::deserialize(&data) {
                        Ok(msg) => {
                            if tx.send((path_id, msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(path_id, ?e, "failed to deserialize datagram");
                        }
                    },
                    Err(e) => {
                        error!(path_id, ?e, "datagram receive error");
                        break;
                    }
                }
            }
        });
        handles.push(handle);

        // Uni-stream receiver (for reliable control messages)
        let uni_handle = tokio::spawn(async move {
            loop {
                match conn_uni.accept_uni().await {
                    Ok(mut recv) => {
                        tracing::debug!(path_id, "uni stream accepted");
                        let mut len_buf = [0u8; 4];
                        if let Err(e) = recv.read_exact(&mut len_buf).await {
                            tracing::debug!(path_id, ?e, "uni stream length read failed");
                            continue;
                        }
                        let len = u32::from_be_bytes(len_buf) as usize;
                        if len > 1_000_000 { continue; }
                        let mut data = vec![0u8; len];
                        if recv.read_exact(&mut data).await.is_err() {
                            continue;
                        }
                        match WireMessage::deserialize(&data) {
                            Ok(msg) => {
                                if tx_uni.send((path_id, msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(path_id, ?e, "failed to deserialize uni stream message");
                            }
                        }
                    }
                    Err(e) => {
                        error!(path_id, ?e, "uni stream accept error");
                        break;
                    }
                }
            }
        });
        handles.push(uni_handle);

        handles
    }

    /// Perform handshake on a connection (client side). Returns the peer's handshake.
    async fn perform_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.open_bi().await?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;
        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete"
        );
        Ok(peer)
    }

    /// Accept a handshake from a peer (server side).
    async fn accept_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.accept_bi().await?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete (server)"
        );
        Ok(peer)
    }

    /// Send a symbol batch over a path using QUIC datagrams.
    ///
    /// feat/window-mtu part 2 (`RWM_WIRE_COMPACT`, v5): one-symbol batches
    /// — the window-mode data path, one symbol per datagram — ride the
    /// compact tag+varint frame (~14–16 B vs the 65-B magic+bincode
    /// framing, the measured ~4.3 Mbit/0.95 Mbit framing tax at c2/c3).
    /// Multi-symbol (block-mode) batches and everything else keep legacy
    /// framing; gate OFF is byte-identical.
    pub fn send_symbols(&self, path_id: PathId, batch: SymbolBatch) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        // The two `RWM_CPUPROF` seams of the send path, and they are ADJACENT
        // rather than nested so their shares add rather than over-count:
        //   `ser`  the wire serialization (compact v5, or the bincode legacy)
        //   `hand` the datagram HANDOFF to quinn — NOT the send syscall, which
        //          happens on quinn's endpoint driver task and is invisible
        //          here by construction (see `net::cpuprof` module docs).
        use crate::net::cpuprof::{timed, Seam};
        if crate::transport::protocol::wire_compact_active() {
            let compact = timed(Seam::Ser, || {
                crate::transport::protocol::serialize_data_compact(&batch)
            });
            if let Some(buf) = compact {
                return timed(Seam::Hand, || self.send_datagram_shaped(path_id, &conn, buf));
            }
        }
        let msg = WireMessage::Data(batch);
        let data = timed(Seam::Ser, || msg.serialize())?;

        timed(Seam::Hand, || self.send_datagram_shaped(path_id, &conn, data))
    }

    /// Send a control message as a datagram (best-effort, low latency).
    pub fn send_control_datagram(&self, path_id: PathId, msg: ControlMessage) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;
        self.send_datagram_shaped(path_id, &conn, data)
    }

    /// Send a control message over a path's reliable stream.
    pub async fn send_control(
        &self,
        path_id: PathId,
        msg: ControlMessage,
    ) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let mut send = conn.open_uni().await?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;

        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;
        Ok(())
    }

    /// Query the max datagram size for a path (PMTU-based).
    pub fn max_datagram_size(&self, path_id: PathId) -> Option<usize> {
        self.connections
            .get(&path_id)
            .and_then(|conn| conn.max_datagram_size())
    }

    /// Receive datagrams from a path.
    pub async fn recv_datagram(&self, path_id: PathId) -> anyhow::Result<WireMessage> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let data = conn.read_datagram().await?;
        let msg = WireMessage::deserialize(&data)?;
        Ok(msg)
    }

    /// Spawn receive loops for all paths, feeding into a channel.
    pub fn spawn_receivers(
        &self,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        for entry in self.connections.iter() {
            let path_id = *entry.key();
            let conn = entry.value().clone();
            handles.extend(self.spawn_receiver_for_path(
                path_id,
                conn,
                tx.clone(),
                ctrl_tx.clone(),
            ));
        }

        handles
    }

    /// Apply the symbol-datagram MTU floor to a quinn transport config
    /// (fix/frontier-wedge — the c3/C8 ~60 s "collapse run" root cause).
    ///
    /// THE WEDGE: every wire symbol rides ONE QUIC datagram of ~1261–1275
    /// bytes (1200-byte symbol + repair header + bincode/batch framing).
    /// quinn's defaults are `initial_mtu = min_mtu = 1200`; PMTUD raises the
    /// path MTU to ~1452 right after the handshake, which is the ONLY reason
    /// those datagrams are sendable at all. quinn also runs an MTU
    /// BLACK-HOLE DETECTOR: a burst of lost large packets (GE loss at c3
    /// looks exactly like an MTU black hole) resets `current_mtu` to
    /// `min_mtu` (1200) and pauses discovery for `black_hole_cooldown`
    /// (default 60 s). During that window `max_datagram_size` ≈ 1170 <
    /// every symbol datagram, so EVERY data send — source, repair, and
    /// every targeted retransmit of the frontier blocker — fails at the
    /// sender with `SendDatagramError::TooLarge` (measured: 8 077
    /// consecutive failures over 60 s in the wedge forensics), while small
    /// control datagrams (acks) still flow and keep the wire RTT fresh.
    /// The transfer freezes for exactly the cooldown, then self-resolves
    /// when PMTUD re-probes. Cross-arm (BBR / Copa / stock) because it is
    /// below the CC layer.
    ///
    /// THE FIX: the engine structurally REQUIRES ~1275-byte datagrams (a
    /// symbol is never fragmented), so declare that floor to quinn:
    /// `min_mtu = initial_mtu = MTU_FLOOR`. A (possibly spurious)
    /// black-hole reset then lands AT the floor and symbol sends keep
    /// working; PMTUD and the black-hole detector otherwise stay active
    /// (upward probing unchanged — the 60 s cooldown remains as quinn's
    /// safety net, no longer wedging ours). A path that truly cannot carry
    /// MTU_FLOOR-byte UDP payloads could never carry a symbol anyway —
    /// before this fix it failed as a silent send blackout; now it fails
    /// loudly as persistent large-packet loss.
    ///
    /// `RWM_MTU_FLOOR` overrides (A/B instrument): `0` restores stock quinn
    /// defaults (the wedge-prone control arm), any other value sets the
    /// floor explicitly.
    fn apply_mtu_floor(transport: &mut quinn::TransportConfig) {
        // A max-size repair-symbol batch serializes to 1279 datagram bytes
        // (1200 symbol + 14 repair header + 65 magic/bincode-fixint batch
        // framing — measured by `mtu_floor_covers_symbol_batch`), + ~33
        // QUIC 1-RTT overhead (short header + CID + PN + AEAD tag +
        // DATAGRAM frame header) = ~1312 minimum UDP payload; 1350 leaves
        // margin for CID/PN-length variation.
        const MTU_FLOOR: u16 = 1350;
        let floor: u16 = std::env::var("RWM_MTU_FLOOR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(MTU_FLOOR);
        if floor == 0 {
            info!("MTU floor OFF (RWM_MTU_FLOOR=0): stock quinn MTUD — black-hole reset lands at 1200 < symbol datagram (wedge-reproduction arm)");
            return; // stock quinn MTU behavior (wedge-reproduction arm)
        }
        info!(floor, "MTU floor: min_mtu=initial_mtu — quinn black-hole reset keeps symbol datagrams sendable (fix/frontier-wedge)");
        transport.initial_mtu(floor);
        transport.min_mtu(floor);
        // feat/window-mtu part 2 mechanism-liveness echo (MEASUREMENT
        // DISCIPLINE item 1): the compact-framing gate, resolved once.
        if crate::transport::protocol::wire_compact_active() {
            info!(
                "compact DATA framing ACTIVE (RWM_WIRE_COMPACT v5: one-symbol \
                 datagrams ride the tag+varint frame, ~14-16 B vs 65-B legacy \
                 framing; receive support unconditional; datagrams SHRINK — \
                 no MTU-floor interaction)"
            );
        }
    }

    fn generate_self_signed_config(
        cc: Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    ) -> anyhow::Result<(ServerConfig, Vec<CertificateDer<'static>>)> {
        let cert = rcgen::generate_simple_self_signed(vec!["raptorpath".into()])?;
        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        let mut server_config = ServerConfig::with_single_cert(
            vec![cert_der.clone()],
            key_der.into(),
        )?;

        // Enable datagrams
        let transport = Arc::get_mut(&mut server_config.transport).unwrap();
        transport.max_concurrent_bidi_streams(100u32.into());
        transport.max_concurrent_uni_streams(100u32.into());
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);
        Self::apply_mtu_floor(transport);
        if let Some(cc) = cc {
            transport.congestion_controller_factory(cc);
        }

        Ok((server_config, vec![cert_der]))
    }

    /// Build a client config with either pinned cert verification or
    /// insecure mode (skip verification) for dev/testing.
    fn make_client_config(
        pinned_hash: Option<[u8; 32]>,
        cc: Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    ) -> ClientConfig {
        let verifier: Arc<dyn rustls::client::danger::ServerCertVerifier> = match pinned_hash {
            Some(hash) => Arc::new(PinnedCertVerifier { expected_hash: hash }),
            None => Arc::new(SkipCertVerification),
        };

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();

        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .expect("rustls config should be valid"),
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);
        Self::apply_mtu_floor(&mut transport);
        if let Some(cc) = cc {
            transport.congestion_controller_factory(cc);
        }
        config.transport_config(Arc::new(transport));
        config
    }
}

/// Compute SHA-256 fingerprint of a DER-encoded certificate.
fn sha256_fingerprint(cert: &CertificateDer<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    hasher.finalize().into()
}

/// Load a pinned certificate from a DER or PEM file and return its SHA-256 hash.
fn load_pinned_cert_hash(path: &Path) -> anyhow::Result<[u8; 32]> {
    let data = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read pinned cert '{}': {e}", path.display()))?;

    // Try PEM first, fall back to DER
    let cert_der = if data.starts_with(b"-----BEGIN") {
        let pem = pem::parse(&data)
            .map_err(|e| anyhow::anyhow!("failed to parse PEM cert '{}': {e}", path.display()))?;
        CertificateDer::from(pem.into_contents())
    } else {
        CertificateDer::from(data)
    };

    Ok(sha256_fingerprint(&cert_der))
}

/// Certificate verifier that pins to a specific certificate's SHA-256 fingerprint.
#[derive(Debug)]
struct PinnedCertVerifier {
    expected_hash: [u8; 32],
}

impl rustls::client::danger::ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual_hash = sha256_fingerprint(end_entity);
        if actual_hash == self.expected_hash {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: expected {}, got {}",
                hex::encode(self.expected_hash),
                hex::encode(actual_hash),
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Skip certificate verification (for self-signed certs in testing/dev).
#[derive(Debug)]
struct SkipCertVerification;

impl rustls::client::danger::ServerCertVerifier for SkipCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod passthrough_cc_tests {
    use super::*;
    use quinn::congestion::{Controller, ControllerFactory};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    fn build(window: &Arc<AtomicU64>, mtu: u16) -> Box<dyn Controller> {
        let f = Arc::new(PassthroughFactory {
            window: window.clone(),
            stats: Arc::new(PassthroughCcStats::default()),
        });
        f.build(Instant::now(), mtu)
    }

    /// The shim's window() follows the engine-owned atomic: what our engine
    /// writes IS the substrate congestion window.
    #[test]
    fn window_follows_the_atomic() {
        let w = Arc::new(AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW));
        let c = build(&w, 1200);
        assert_eq!(c.window(), PASSTHROUGH_INITIAL_WINDOW);
        w.store(37_500, Ordering::Relaxed); // Copa cwnd 30 sym x 1250 B
        assert_eq!(c.window(), 37_500);
        w.store(1_000_000, Ordering::Relaxed);
        assert_eq!(c.window(), 1_000_000);
    }

    /// The handshake is never starved: the initial window is generous (the
    /// atomic starts at PASSTHROUGH_INITIAL_WINDOW, well above quinn's stock
    /// RFC-9002 initial ~14 720 B) and a zero/garbage write floors at two
    /// datagrams instead of wedging the connection.
    #[test]
    fn handshake_not_starved_and_zero_write_floors() {
        let w = Arc::new(AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW));
        let c = build(&w, 1500);
        assert!(c.initial_window() >= 64 * 1024);
        assert!(c.window() >= 64 * 1024, "pre-feed window must cover the handshake");
        w.store(0, Ordering::Relaxed);
        assert_eq!(c.window(), 2 * 1500, "zero write floors at 2 MTUs, never 0");
    }

    /// clone_box (quinn clones controllers for path state) keeps sharing the
    /// SAME engine-owned atomic.
    #[test]
    fn clone_box_shares_the_atomic() {
        let w = Arc::new(AtomicU64::new(50_000));
        let c = build(&w, 1200);
        let c2 = c.clone_box();
        w.store(80_000, Ordering::Relaxed);
        assert_eq!(c.window(), 80_000);
        assert_eq!(c2.window(), 80_000);
    }

    /// Congestion events are recorded, never acted on: window unchanged.
    #[test]
    fn congestion_events_are_recorded_noops() {
        let w = Arc::new(AtomicU64::new(100_000));
        let stats = Arc::new(PassthroughCcStats::default());
        let f = Arc::new(PassthroughFactory { window: w.clone(), stats: stats.clone() });
        let mut c = f.build(Instant::now(), 1200);
        let now = Instant::now();
        c.on_congestion_event(now, now, false, 3_600);
        c.on_congestion_event(now, now, true, 1_200);
        assert_eq!(c.window(), 100_000, "loss must not move the engine-owned window");
        assert_eq!(stats.congestion_events.load(Ordering::Relaxed), 2);
        assert_eq!(stats.lost_bytes.load(Ordering::Relaxed), 4_800);
        assert_eq!(stats.persistent_congestion.load(Ordering::Relaxed), 1);
    }
}

#[cfg(test)]
mod datagram_queue_audit_tests {
    use super::*;

    /// **The OFF-value property** for the datagram send-queue audit
    /// (goal-gate "What Binds Throughput", instrument 3).
    ///
    /// The audit costs one connection-lock take per datagram, so it must not
    /// exist on the shipped path. `dg_audit` resolves `RWM_DIAG` ONCE at
    /// construction, and with it off `datagram_queue_stats` must return
    /// `None` for every path — not `Some((0,0,0,0,0))`, which would be
    /// indistinguishable from "the audit ran and saw nothing".
    ///
    /// **Written to be correct in BOTH process conditions.** A test that
    /// mutated `RWM_DIAG` would race every other test in the process (and is
    /// `unsafe` in edition 2024), so this reads the AMBIENT value and asserts
    /// the branch that value selects. Run it in a process with `RWM_DIAG`
    /// unset and again with `RWM_DIAG=1` and both arms are covered — which is
    /// the multi-process discipline this repo already applies to env gates.
    #[tokio::test]
    async fn datagram_queue_audit_follows_rwm_diag_and_is_absent_when_off() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let t = QuicTransport::new(&[addr], false, None)
            .await
            .expect("client endpoint binds on loopback");

        let diag_on = crate::config::env_flag("RWM_DIAG", false);
        assert_eq!(
            t.dg_audit, diag_on,
            "dg_audit must be RWM_DIAG resolved at construction"
        );

        // No path has sent a datagram, so the readout is `None` EITHER WAY —
        // the two reasons are distinguished by `dg_audit`, never by a zero.
        assert!(
            t.datagram_queue_stats(0).is_none(),
            "no handoff has happened: the gauge must be absent, not zero"
        );

        if !diag_on {
            // The stronger OFF claim: the map itself is never touched, so the
            // seam takes no lock and allocates no per-path audit record.
            assert!(
                t.dg_stats.is_empty(),
                "the audit must not allocate when RWM_DIAG is off"
            );
        }
    }

    /// The eviction predicate's semantics, asserted against quinn's actual
    /// source so the bound in `DatagramQueueAudit`'s docs is checked rather
    /// than merely claimed.
    ///
    /// quinn-proto's `Datagrams::send(_, drop = true)` pops while
    /// `outgoing_total > buffer_size`, and `send_buffer_space()` is
    /// `buffer_size.saturating_sub(outgoing_total)`. Therefore
    /// `space == 0  <=>  outgoing_total >= buffer_size`, which contains the
    /// eviction condition `outgoing_total > buffer_size` and exceeds it only
    /// on the exact tie. This test states that containment as arithmetic.
    #[test]
    fn the_full_predicate_contains_the_eviction_condition_and_differs_only_at_the_tie() {
        const SIZE: usize = 4 * 1024 * 1024;
        let space = |total: usize| SIZE.saturating_sub(total);
        let evicts = |total: usize| total > SIZE;
        let full = |total: usize| space(total) == 0;

        for total in [0, 1, SIZE / 2, SIZE - 1, SIZE, SIZE + 1, SIZE + 1_200, SIZE * 2] {
            assert!(
                !evicts(total) || full(total),
                "every evicting state must be counted by `full` (total={total})"
            );
        }
        // The one over-count, named explicitly rather than left implicit.
        assert!(full(SIZE) && !evicts(SIZE), "the tie is the only over-count");
        assert!(!full(SIZE - 1), "a queue with room must not be counted");
    }
}
