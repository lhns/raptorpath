//! THE RECOVERY-PLANE MODEL, shared by the two component benches that need
//! it: `tests/recovery_bench.rs` (which characterizes the plane itself) and
//! `tests/slack_bench.rs` (which needs the plane's STALL DISTRIBUTION as an
//! input and must not invent one).
//!
//! Extracted VERBATIM from `recovery_bench.rs` on 2026-08-09 (goal-gate
//! "Emission-Slack Bench"). The only additions are OBSERVATIONS - per-seq
//! send/arrive/release timestamps and the receiver's resequencing-span
//! samples - which are recorded, never read, by the driver itself.
//! `recovery_bench_fixtures_pin_the_plane` is the proof that the move was
//! behaviour-identical: it is unchanged and still passes.
//!
//! A deterministic discrete-event driver: no CC, no scheduler, no multipath
//! placement, no transport, no tokio. It feeds synthetic arrival/loss
//! patterns to the SHIPPED recovery laws (`raptorpath::net::*`) and records,
//! per hole, WHEN and WHY it was served.
//!
//! See `recovery_bench.rs`'s header for the clock argument and for the
//! full "what this cannot see" boundary.
#![allow(dead_code)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};
use std::time::Duration;

use raptorpath::net::{
    cooldown_elapsed, hole_nack_refresh, legacy_age_ripe, mp_delivered_intervals, mp_fast_lost,
    mp_hole_ripe, mp_time_threshold_split, pooled_recovery_srtt_us, recovery_floor_us,
    retx_cooldown_us, shed_allowed, shed_deadline_us, tail_sweep_timeout_us, time_threshold_ripe,
    MAX_NACK_GAPS, MAX_NACK_REPAIRS_PER_NACK,
};

// ───────────────────────────── env plumbing ─────────────────────────────

pub fn env_str(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}
pub fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
pub fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
pub fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|s| s != "0" && !s.eq_ignore_ascii_case("false"))
        .unwrap_or(default)
}
pub fn list_u64(name: &str, default: &str) -> Vec<u64> {
    env_str(name, default).split(',').filter_map(|s| s.trim().parse().ok()).collect()
}
pub fn list_f64(name: &str, default: &str) -> Vec<f64> {
    env_str(name, default).split(',').filter_map(|s| s.trim().parse().ok()).collect()
}
pub fn list_str(name: &str, default: &str) -> Vec<String> {
    env_str(name, default)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// SplitMix64 — the generator the codebase already uses for coefficients and
/// for `gen_decode_bench`'s wire trace.
pub struct Rng(u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn unit(&mut self) -> f64 {
        self.next() as f64 / u64::MAX as f64
    }
}

// ───────────────────────────── the axes ─────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pattern {
    Uniform,
    Ge,
}
impl Pattern {
    pub fn tag(self) -> &'static str {
        match self {
            Pattern::Uniform => "unif",
            Pattern::Ge => "ge",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clock {
    /// The estimator's app-echo RTT: RTprop + wire queue + STORE DWELL.
    App,
    /// `QuicTransport::wire_rtt`: RTprop + wire queue, dwell excluded.
    Wire,
}
impl Clock {
    pub fn tag(self) -> &'static str {
        match self {
            Clock::App => "app",
            Clock::Wire => "wire",
        }
    }
}

/// A gate arm. These are ENV GATES (A/B attribution arms), NOT dials on the
/// (δ, ρ, r) triangle — nothing here keys a law on δ or ρ.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Arm {
    pub name: &'static str,
    pub recov_mp: bool,
    pub recov_sp: bool,
    pub patience_derived: bool,
}
pub const ARMS: &[Arm] = &[
    // `RWM_RECOV_MP` default ON since 2026-07-21 — the shipped stack.
    Arm { name: "shipped", recov_mp: true, recov_sp: false, patience_derived: false },
    // Neither RFC channel: the pre-2026-07 legacy `srtt/2` age gate.
    Arm { name: "legacy", recov_mp: false, recov_sp: false, patience_derived: false },
    // `RWM_RECOV_SP`: the §6.1.2 time threshold at N = 1 too.
    Arm { name: "sp", recov_mp: true, recov_sp: true, patience_derived: false },
    // `RWM_PATIENCE_DERIVED` on top of the shipped stack.
    Arm { name: "pd", recov_mp: true, recov_sp: false, patience_derived: true },
];

/// Which channel ADMITTED a hole's first service.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Chan {
    /// RFC 9002 §6.1.2 time threshold (the MP or SP arm).
    Time,
    /// RFC 9002 §6.1.1 packet threshold — the FAST channel (MP arm, N > 1).
    Fast,
    /// The legacy `srtt/2` age gate.
    LegacyAge,
    /// The sender's own tail sweep synthesized the report that served it.
    Sweep,
    /// The δ-honest shed law retired the hole instead of serving it.
    Shed,
}
pub const CHANS: [Chan; 5] = [Chan::Time, Chan::Fast, Chan::LegacyAge, Chan::Sweep, Chan::Shed];
impl Chan {
    pub fn tag(self) -> &'static str {
        match self {
            Chan::Time => "time",
            Chan::Fast => "fast",
            Chan::LegacyAge => "age",
            Chan::Sweep => "sweep",
            Chan::Shed => "shed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub rtprop_us: u64,
    pub loss: f64,
    pub pattern: Pattern,
    pub n_paths: usize,
    pub clock: Clock,
    pub arm: Arm,
    pub seed: u64,
}

/// Fixed inputs shared by every cell (the operating-point calibration).
#[derive(Clone, Copy)]
pub struct Calib {
    pub n_src: u64,
    pub mbps: f64,
    pub skew_us: u64,
    pub wireq_us: u64,
    pub dwell_us: u64,
    pub jitter_us: u64,
    pub shed: bool,
    pub budget: usize,
}
impl Calib {
    pub fn from_env() -> Self {
        Self {
            n_src: env_u64("RWM_RB_N", 6_000),
            mbps: env_f64("RWM_RB_MBPS", 100.0),
            skew_us: env_u64("RWM_RB_SKEW_MS", 5) * 1_000,
            wireq_us: env_u64("RWM_RB_WIREQ_MS", 4) * 1_000,
            dwell_us: env_u64("RWM_RB_DWELL_MS", 144) * 1_000,
            jitter_us: env_u64("RWM_RB_JITTER_MS", 1) * 1_000,
            shed: env_flag("RWM_RB_SHED", false),
            budget: MAX_NACK_REPAIRS_PER_NACK,
        }
    }
    /// The fixture calibration: small, fast, and pinned by `#[test]`.
    pub fn fixture() -> Self {
        Self {
            n_src: 6_000,
            mbps: 100.0,
            skew_us: 5_000,
            wireq_us: 4_000,
            dwell_us: 144_000,
            jitter_us: 1_000,
            shed: false,
            budget: MAX_NACK_REPAIRS_PER_NACK,
        }
    }
}

// ─────────────────────────── the loss model ─────────────────────────────

/// Per-path wire loss. Uniform = iid Bernoulli. GE = the two-state
/// Gilbert-Elliott chain the L1 profiles use (`netem loss gemodel`): the BAD
/// state drops everything, the mean bad run is `MEAN_BURST` symbols, and
/// `p_gb` is solved so the stationary bad fraction equals the cell's nominal
/// loss — the same loss RATE as the uniform arm, redistributed.
pub const MEAN_BURST: f64 = 8.0;
pub struct LossChain {
    pub pattern: Pattern,
    pub p: f64,
    pub p_gb: f64,
    pub p_bg: f64,
    pub bad: bool,
}
impl LossChain {
    pub fn new(pattern: Pattern, p: f64) -> Self {
        let p_bg = 1.0 / MEAN_BURST;
        let p_gb = (p * p_bg) / (1.0 - p).max(1e-9);
        Self { pattern, p, p_gb, p_bg, bad: false }
    }
    pub fn drops(&mut self, rng: &mut Rng) -> bool {
        match self.pattern {
            Pattern::Uniform => rng.unit() < self.p,
            Pattern::Ge => {
                if self.bad {
                    if rng.unit() < self.p_bg {
                        self.bad = false;
                    }
                } else if rng.unit() < self.p_gb {
                    self.bad = true;
                }
                self.bad
            }
        }
    }
}

// ───────────────────────────── the driver ───────────────────────────────

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Ev {
    /// A symbol leaves the sender on `path` (originals and retransmits).
    Send { seq: u64, path: u32, retx: bool },
    /// A symbol reaches the receiver.
    Arrive { seq: u64, retx: bool },
    /// The receiver's SACK-bearing ack timer.
    Ack,
    /// A gap report reaches the sender.
    Report { frontier: u64, highest: u64, gaps: Vec<(u64, u64)>, sweep: bool },
    /// The sender's tail-sweep timer.
    Sweep,
}

/// One hole's life: an ORIGINAL flight that died on the wire.
#[derive(Clone, Copy)]
pub struct Hole {
    pub lost_at_us: u64,
    pub first_service_us: Option<u64>,
    pub delivered_us: Option<u64>,
    pub chan: Option<Chan>,
    pub services: u32,
}

#[derive(Default, Clone)]
pub struct Counts {
    pub retx: u64,
    pub sweeps: u64,
    pub reports: u64,
    pub supp_cool: u64,
    pub supp_law: u64,
    pub supp_age: u64,
    pub shed: u64,
}

pub struct Out {
    pub holes: Vec<Hole>,
    pub counts: Counts,
    /// The §6.1.2 threshold on path 0 (µs) — the arm's PATIENCE.
    pub patience_us: u64,
    /// The pooled clock the legacy gate / cooldown / sweep all read (µs).
    pub pooled_us: u64,
    pub cooldown_us: u64,
    pub refresh_us: u64,
    /// The wire serialization time of one symbol at the cell's rate (µs).
    pub tx_gap_us: u64,
    /// Per-path one-way delay (µs); path i carries i × skew.
    pub owd_us: Vec<u64>,
    // ── OBSERVATIONS (recorded, never read, by this driver; added for
    //    `slack_bench`. Nothing below feeds a decision here.) ──
    /// Per-seq ORIGINAL send time (µs).
    pub send_us: Vec<u64>,
    /// Per-seq SENDER-STORE release time (µs): the instant the seq left
    /// `retransmit_buffer` — by the cumulative frontier, by a SACK-implied
    /// delivered interval, or by the shed law. `None` ⇒ never released
    /// inside the horizon.
    pub store_release_us: Vec<Option<u64>>,
    /// Per-seq RECEIVER arrival time (µs), original flight or repair.
    pub recv_us: Vec<Option<u64>>,
    /// `(time, frontier)` at every non-sweep gap report the sender
    /// processed — the CUMULATIVE release timeline.
    pub frontier_events: Vec<(u64, u64)>,
    /// The receiver's RESEQUENCING SPAN, sampled at every ack instant:
    /// `highest_seen + 1 − cumulative_frontier`, i.e. how many seqs the
    /// receiver is holding above its in-order frontier.
    pub span_samples: Vec<u64>,
}

/// The ordinary SACK-bearing WindowAck floor (`GAP_ACK_MIN_INTERVAL`, 2 ms).
pub const GAP_ACK_MIN_US: u64 = 2_000;

pub fn run_cell(cell: Cell, cal: Calib) -> Out {
    let tx_gap_us = (((1_200.0 * 8.0) / (cal.mbps * 1e6)) * 1e6).max(1.0) as u64;
    let n_paths = cell.n_paths as u32;

    // Per-path clocks. `owd` is the one-way delay; path 1 carries the skew.
    // `copa` is the wire-timed Copa clock (RWM_COPA_WIRE, the default);
    // `ewma` is the ESTIMATOR clock — the axis under test.
    let owd: Vec<u64> = (0..n_paths).map(|i| cell.rtprop_us / 2 + i as u64 * cal.skew_us).collect();
    let copa: Vec<u64> = owd.iter().map(|o| 2 * o + cal.wireq_us).collect();
    let ewma: Vec<u64> = copa
        .iter()
        .map(|w| match cell.clock {
            Clock::App => w + cal.dwell_us,
            Clock::Wire => *w,
        })
        .collect();

    // The pooled recovery clock — the SHIPPED reduction, verbatim.
    let pooled_us = pooled_recovery_srtt_us(&ewma);
    let pooled_floor = recovery_floor_us(cell.arm.patience_derived, cal.jitter_us, pooled_us);
    let cooldown_us = retx_cooldown_us(pooled_us, pooled_floor);
    let thr_of = |p: u32| -> u64 {
        let (c, e) = (copa[p as usize], ewma[p as usize]);
        let floor = recovery_floor_us(cell.arm.patience_derived, cal.jitter_us, c.max(e));
        mp_time_threshold_split(c, e, floor).0
    };
    let patience_us = thr_of(0);
    // The receiver's refresh cadence reads the COPA clock (`PathState::srtt`),
    // NOT the estimator — so the clock argument does not move it. That
    // asymmetry is a FINDING, not an accident: keep it faithful.
    let refresh_us =
        hole_nack_refresh(Some(Duration::from_micros(copa.iter().copied().max().unwrap_or(0))))
            .as_micros() as u64;
    let shed_deadline = shed_deadline_us(0.5, cell.rtprop_us);
    // ρ budget: the residual the (δ,ρ,r) design already concedes — modelled
    // as the cell's own loss class (there is no FEC in the recovery plane).
    let shed_budget_frac = cell.loss;

    // The ORIGINAL loss pattern is precomputed in seq order, so it is
    // IDENTICAL across every arm and clock of a cell — the A/B is over the
    // laws, never over the wire. Retransmit loss is drawn from an
    // independent stream for the same reason.
    let mut rng = Rng(cell.seed.wrapping_mul(0x9E37_79B9).wrapping_add(0xABCD));
    let mut chains: Vec<LossChain> =
        (0..n_paths).map(|_| LossChain::new(cell.pattern, cell.loss)).collect();
    let lost_orig: Vec<bool> = (0..cal.n_src)
        .map(|seq| chains[(seq % n_paths as u64) as usize].drops(&mut rng))
        .collect();
    let mut rng_retx = Rng(cell.seed.wrapping_mul(0xD1B5_4A32).wrapping_add(0x1234));
    let mut retx_chains: Vec<LossChain> =
        (0..n_paths).map(|_| LossChain::new(cell.pattern, cell.loss)).collect();

    // ── sender state (mirrors `run_impl`'s gap handler) ──
    let mut retransmit_buffer: BTreeMap<u64, (u64, u32)> = BTreeMap::new();
    let mut nack_retx_at: HashMap<u64, (u64, u32)> = HashMap::new();
    let mut source_path_map: BTreeMap<u64, u32> = BTreeMap::new();
    let mut mp_delivered: HashMap<u32, Vec<u64>> = HashMap::new();
    let mut mp_evid_max: u64 = 0;
    let mut shed_seqs: BTreeSet<u64> = BTreeSet::new();
    let mut shed_total: u64 = 0;
    let mut last_tail_sweep_us: u64 = 0;

    // ── receiver state ──
    let mut missing: BTreeSet<u64> = BTreeSet::new();
    let mut highest_seen: u64 = 0;
    let mut any_arrival = false;
    let mut arrived_since_ack: u64 = 0;
    let mut last_hole_nack_at: u64 = 0;

    let mut holes: HashMap<u64, Hole> = HashMap::new();
    let mut counts = Counts::default();

    // ── OBSERVATION ONLY (nothing below is read by a decision here) ──
    let n = cal.n_src as usize;
    let mut obs_send: Vec<u64> = vec![0; n];
    let mut obs_release: Vec<Option<u64>> = vec![None; n];
    let mut obs_recv: Vec<Option<u64>> = vec![None; n];
    let mut obs_frontier: Vec<(u64, u64)> = Vec::new();
    let mut obs_span: Vec<u64> = Vec::new();

    // Event queue: (time, monotone tiebreaker, event) ⇒ total determinism.
    let mut heap: BinaryHeap<Reverse<(u64, u64, Ev)>> = BinaryHeap::new();
    let mut ord: u64 = 0;
    macro_rules! push {
        ($t:expr, $e:expr) => {{
            ord += 1;
            heap.push(Reverse(($t, ord, $e)));
        }};
    }

    for seq in 0..cal.n_src {
        let path = (seq % n_paths as u64) as u32;
        push!(seq * tx_gap_us, Ev::Send { seq, path, retx: false });
    }
    let last_send_us = cal.n_src.saturating_sub(1) * tx_gap_us;
    push!(GAP_ACK_MIN_US, Ev::Ack);
    push!(tail_sweep_timeout_us(pooled_us), Ev::Sweep);

    // Virtual-time guard: the tail must drain, but a pathological cell must
    // not spin. 60 s of virtual time past the last original send.
    let horizon = last_send_us + 60_000_000;

    while let Some(Reverse((now, _, ev))) = heap.pop() {
        if now > horizon {
            break;
        }
        match ev {
            Ev::Send { seq, path, retx } => {
                if !retx {
                    source_path_map.insert(seq, path);
                    retransmit_buffer.insert(seq, (now, path));
                    obs_send[seq as usize] = now;
                }
                let lost = if retx {
                    retx_chains[path as usize].drops(&mut rng_retx)
                } else {
                    lost_orig[seq as usize]
                };
                if lost {
                    if !retx {
                        holes.insert(
                            seq,
                            Hole {
                                lost_at_us: now,
                                first_service_us: None,
                                delivered_us: None,
                                chan: None,
                                services: 0,
                            },
                        );
                    }
                } else {
                    push!(now + owd[path as usize], Ev::Arrive { seq, retx });
                }
            }
            Ev::Arrive { seq, retx } => {
                any_arrival = true;
                arrived_since_ack += 1;
                if obs_recv[seq as usize].is_none() {
                    obs_recv[seq as usize] = Some(now);
                }
                if seq > highest_seen {
                    for s in (highest_seen + 1)..seq {
                        missing.insert(s);
                    }
                    highest_seen = seq;
                    missing.remove(&seq);
                } else {
                    missing.remove(&seq);
                }
                if retx || holes.contains_key(&seq) {
                    if let Some(h) = holes.get_mut(&seq) {
                        if h.delivered_us.is_none() {
                            h.delivered_us = Some(now);
                        }
                    }
                }
            }
            Ev::Ack => {
                let frontier = missing.iter().next().copied().unwrap_or(highest_seen + 1);
                if any_arrival {
                    obs_span.push((highest_seen + 1).saturating_sub(frontier));
                }
                if any_arrival && (arrived_since_ack > 0 || !missing.is_empty()) {
                    // The first `MAX_NACK_GAPS` maximal missing runs — the
                    // WindowAck's SACK payload.
                    let mut gaps: Vec<(u64, u64)> = Vec::new();
                    let mut it = missing.iter().copied();
                    if let Some(first) = it.next() {
                        let (mut lo, mut hi) = (first, first);
                        for s in it {
                            if s == hi + 1 {
                                hi = s;
                            } else {
                                gaps.push((lo, hi));
                                if gaps.len() >= MAX_NACK_GAPS {
                                    break;
                                }
                                lo = s;
                                hi = s;
                            }
                        }
                        if gaps.len() < MAX_NACK_GAPS {
                            gaps.push((lo, hi));
                        }
                    }
                    if !gaps.is_empty() {
                        last_hole_nack_at = now;
                    }
                    push!(
                        now + owd[0],
                        Ev::Report { frontier, highest: highest_seen, gaps, sweep: false }
                    );
                }
                let next = if arrived_since_ack > 0 || last_hole_nack_at == 0 {
                    // COLD START (`last_hole_nack_at == 0`): the ack timer is
                    // armed at `GAP_ACK_MIN_US`, before the first symbol can
                    // possibly have arrived (one owd away). There is no
                    // stalled hole to refresh yet, so the ordinary gap-ack
                    // floor owns the cadence — the shipped receiver acks ON
                    // ARRIVAL subject to that floor. Deferring the first
                    // advertisement by the full refresh cadence instead would
                    // hold EVERY symbol emitted inside that window in the
                    // sender's store, a startup transient with no counterpart
                    // in the engine. Measured (goal-gate "Coverage: derivable
                    // or not"): at c1/20 ms that transient alone SET the
                    // required backlog — S(0.1 %) = 1521 against a
                    // 58 ms/38.46 µs = 1508-symbol cold-start hold — and made
                    // it independent of the loss rate, which is exactly the
                    // signature of an artifact rather than of a term.
                    now + GAP_ACK_MIN_US
                } else {
                    // Nothing arrived and a hole IS outstanding: the
                    // stalled-hole refresh cadence owns the next
                    // advertisement.
                    (last_hole_nack_at + refresh_us).max(now + GAP_ACK_MIN_US)
                };
                arrived_since_ack = 0;
                if now < horizon && (!retransmit_buffer.is_empty() || now < last_send_us) {
                    push!(next, Ev::Ack);
                }
            }
            Ev::Sweep => {
                // P10b tail sweep: arm at oldest-activity + 2×SRTT clamped.
                let timeout = tail_sweep_timeout_us(pooled_us);
                if let Some((&seq, &(send_us, _))) = retransmit_buffer.iter().next() {
                    let last_activity = nack_retx_at
                        .get(&seq)
                        .map_or(send_us, |&(r, _)| r.max(send_us))
                        .max(last_tail_sweep_us);
                    let deadline = last_activity + timeout;
                    if now >= deadline {
                        last_tail_sweep_us = now;
                        counts.sweeps += 1;
                        push!(
                            now,
                            Ev::Report {
                                frontier: seq,
                                highest: highest_seen,
                                gaps: vec![(seq, seq)],
                                sweep: true,
                            }
                        );
                        push!(now + timeout, Ev::Sweep);
                    } else {
                        push!(deadline, Ev::Sweep);
                    }
                } else if now < last_send_us {
                    push!(now + timeout, Ev::Sweep);
                }
            }
            Ev::Report { frontier, highest, gaps, sweep } => {
                counts.reports += 1;
                // Ack processing: everything below the frontier, plus the
                // delivered intervals the gap list implies, is gone.
                obs_frontier.push((now, frontier));
                let acked: Vec<u64> = retransmit_buffer.range(..frontier).map(|(&s, _)| s).collect();
                for s in acked {
                    retransmit_buffer.remove(&s);
                    nack_retx_at.remove(&s);
                    obs_release[s as usize] = Some(now);
                }
                if !sweep {
                    for (lo, hi) in mp_delivered_intervals(&gaps) {
                        let del: Vec<u64> =
                            retransmit_buffer.range(lo..=hi).map(|(&s, _)| s).collect();
                        for s in del {
                            retransmit_buffer.remove(&s);
                            nack_retx_at.remove(&s);
                            obs_release[s as usize] = Some(now);
                        }
                    }
                    // Packet-threshold evidence ingestion (MP arm, N > 1).
                    if cell.arm.recov_mp && cell.n_paths > 1 {
                        for (lo, hi) in mp_delivered_intervals(&gaps) {
                            let start = lo.max(mp_evid_max + 1);
                            if start > hi {
                                continue;
                            }
                            for (&q, &pj) in source_path_map.range(start..=hi) {
                                mp_delivered.entry(pj).or_default().push(q);
                            }
                            mp_evid_max = mp_evid_max.max(hi);
                        }
                    }
                }

                let mut served: usize = 0;
                'gaps: for &(gs, ge) in &gaps {
                    for seq in gs..=ge {
                        if served >= cal.budget {
                            break 'gaps;
                        }
                        // δ-honest shed (armed only under RWM_RB_SHED).
                        if cal.shed {
                            if shed_seqs.contains(&seq) {
                                continue;
                            }
                            if let Some(&(send_us, _)) = retransmit_buffer.get(&seq) {
                                if shed_allowed(
                                    now.saturating_sub(send_us),
                                    shed_deadline,
                                    shed_total,
                                    cal.n_src,
                                    shed_budget_frac,
                                ) {
                                    retransmit_buffer.remove(&seq);
                                    nack_retx_at.remove(&seq);
                                    obs_release[seq as usize] = Some(now);
                                    shed_seqs.insert(seq);
                                    shed_total += 1;
                                    counts.shed += 1;
                                    if let Some(h) = holes.get_mut(&seq) {
                                        if h.chan.is_none() {
                                            h.chan = Some(Chan::Shed);
                                            h.first_service_us = Some(now);
                                        }
                                    }
                                    continue;
                                }
                            }
                        }
                        // Per-seq cooldown.
                        if let Some(&(last, _)) = nack_retx_at.get(&seq) {
                            if !cooldown_elapsed(now, last, cooldown_us) {
                                counts.supp_cool += 1;
                                continue;
                            }
                        }
                        let mp_flight: Option<(u64, u32)> = nack_retx_at
                            .get(&seq)
                            .copied()
                            .or_else(|| retransmit_buffer.get(&seq).map(|&(t, p)| (t, p)));
                        let chan: Chan;
                        if cell.arm.recov_mp && cell.n_paths > 1 {
                            let time_ripe = match mp_flight {
                                Some((t, p)) => mp_hole_ripe(cell.n_paths, now, Some(t), thr_of(p)),
                                None => true,
                            };
                            let mut fast = false;
                            if !time_ripe && !nack_retx_at.contains_key(&seq) {
                                fast = source_path_map
                                    .get(&seq)
                                    .and_then(|j| mp_delivered.get(j))
                                    .is_some_and(|v| mp_fast_lost(v, seq));
                            }
                            if !time_ripe && !fast {
                                counts.supp_law += 1;
                                continue;
                            }
                            chan = if fast { Chan::Fast } else { Chan::Time };
                        } else if cell.arm.recov_sp && cell.n_paths <= 1 {
                            let time_ripe = time_threshold_ripe(
                                now,
                                mp_flight.map(|(t, _)| t),
                                mp_flight.map(|(_, p)| thr_of(p)).unwrap_or(0),
                            );
                            if !time_ripe {
                                counts.supp_law += 1;
                                continue;
                            }
                            chan = Chan::Time;
                        } else {
                            if let Some(&(send_us, _)) = retransmit_buffer.get(&seq) {
                                if !legacy_age_ripe(now, send_us, pooled_us) {
                                    counts.supp_age += 1;
                                    continue;
                                }
                            }
                            chan = Chan::LegacyAge;
                        }
                        // Nothing to serve (already acked) — the real sender
                        // skips a stale gap for an acked seq the same way.
                        if !retransmit_buffer.contains_key(&seq) {
                            continue;
                        }
                        served += 1;
                        counts.retx += 1;
                        let path = source_path_map.get(&seq).copied().unwrap_or(0);
                        let retx_path = (path + 1) % n_paths;
                        nack_retx_at.insert(seq, (now, retx_path));
                        if let Some(h) = holes.get_mut(&seq) {
                            h.services += 1;
                            if h.first_service_us.is_none() {
                                h.first_service_us = Some(now);
                                h.chan = Some(if sweep { Chan::Sweep } else { chan });
                            }
                        }
                        push!(now, Ev::Send { seq, path: retx_path, retx: true });
                    }
                }
                let _ = highest;
            }
        }
    }

    let mut hv: Vec<Hole> = holes.into_values().collect();
    hv.sort_by_key(|h| h.lost_at_us);
    Out {
        holes: hv,
        counts,
        patience_us,
        pooled_us,
        cooldown_us,
        refresh_us,
        tx_gap_us,
        owd_us: owd,
        send_us: obs_send,
        store_release_us: obs_release,
        recv_us: obs_recv,
        frontier_events: obs_frontier,
        span_samples: obs_span,
    }
}

pub fn pct(v: &[u64], q: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    let i = ((((v.len() - 1) as f64) * q).round() as usize).min(v.len() - 1);
    v[i]
}

pub fn ms(us: u64) -> f64 {
    us as f64 / 1_000.0
}
