//! COMPONENT BENCH — the RECOVERY PLANE ALONE.
//!
//! Goal-gate "Component Benches" (2026-08-08). No CC, no scheduler, no
//! multipath placement, no transport, no tokio: a deterministic
//! discrete-event driver that feeds synthetic arrival/loss patterns to the
//! SHIPPED recovery laws (`raptorpath::net::*`, extracted in the commit
//! before this one) and records, per hole, WHEN and WHY it was served.
//!
//! The standing rule this instrument exists to serve: **no L1 battery until
//! the mechanism is characterized at component level and the component
//! result predicts a number L1 can confirm or refute.** Two consecutive
//! goal-gate attempts (ack-merge, derived patience) were falsified because
//! their PREMISE was mis-measured — facts that cost hours of L1 time and
//! that this file answers in seconds.
//!
//! ```text
//! cargo test --test recovery_bench --release -- --ignored --nocapture
//! ```
//!
//! Env knobs (axes are comma lists; every default is stated):
//!   RWM_RB_RTPROP_MS  5,10,20,50,100,200   RTprop per cell
//!   RWM_RB_LOSS       0.001,0.01,0.026,0.05
//!   RWM_RB_PATTERN    uniform,ge           iid vs Gilbert-Elliott bursty
//!   RWM_RB_PATHS      1,2                  path count (2 ⇒ skew applied)
//!   RWM_RB_CLOCK      app,wire             THE clock ARGUMENT (see below)
//!   RWM_RB_ARMS       shipped,legacy,sp,pd gate arms (see `ARMS`)
//!   RWM_RB_N          6000                 source symbols per cell
//!   RWM_RB_SEEDS      42,7
//!   RWM_RB_MBPS       100.0                source rate (c7 class)
//!   RWM_RB_SKEW_MS    5                    inter-path one-way delay skew
//!   RWM_RB_WIREQ_MS   4                    standing network queue (c7 p0)
//!   RWM_RB_DWELL_MS   144                  sender-store dwell the app-echo
//!                                          RTT includes; RTprop 10 + wireQ 4
//!                                          + 144 = the 158 ms measured at c7
//!   RWM_RB_JITTER_MS  1                    measured RTT jitter (pd arm)
//!   RWM_RB_SHED       0                    arm the δ-honest shed law
//!
//! THE CLOCK ARGUMENT (the first question, §16.40's named successor):
//! every recovery clock in the plane is fed by `pooled_recovery_srtt_us` and
//! by the per-path `(copa_srtt, estimator_rtt)` pair. The estimator's
//! app-echo RTT is STORE-DWELL INCLUSIVE — measured 158 ms at c7 against an
//! 8–10 ms RTprop — while `QuicTransport::wire_rtt` (ADR-0062 / §16.34)
//! excludes the dwell. `RWM_RB_CLOCK=app` feeds the dwell-inclusive clock,
//! `wire` the dwell-free one. NOTHING else differs between the arms.
//!
//! WHAT THIS BENCH CANNOT SEE — stated up front, because that boundary is
//! what keeps L1 honest:
//!   * congestion control. No cwnd, no pacing gate, no Copa backoff: a
//!     retransmit here is never rate-limited or queued behind source data,
//!     so measured hole→service latency is a LOWER BOUND on the real one.
//!   * the NACK budget's congestion modulation (`NackCongestionState`) —
//!     the per-report budget is the un-throttled `MAX_NACK_REPAIRS_PER_NACK`.
//!   * FEC. No proactive repair, no generation/window decode: every hole is
//!     served by ARQ alone, so retx counts are an UPPER bound wherever r > 0
//!     would have covered the hole first.
//!   * the store/reservoir itself — i.e. the very dwell that inflates the
//!     app-echo clock is an INPUT here, not an emergent quantity. The bench
//!     can say what a given dwell does to patience; it cannot say what
//!     changing the clock does to the dwell (that feedback loop is exactly
//!     what L1 must measure).
//!   * scheduler placement, per-path CC coupling, path failover.
//!   * control-plane loss and the real ack/emission interleave.

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

fn env_str(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}
fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|s| s != "0" && !s.eq_ignore_ascii_case("false"))
        .unwrap_or(default)
}
fn list_u64(name: &str, default: &str) -> Vec<u64> {
    env_str(name, default).split(',').filter_map(|s| s.trim().parse().ok()).collect()
}
fn list_f64(name: &str, default: &str) -> Vec<f64> {
    env_str(name, default).split(',').filter_map(|s| s.trim().parse().ok()).collect()
}
fn list_str(name: &str, default: &str) -> Vec<String> {
    env_str(name, default)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// SplitMix64 — the generator the codebase already uses for coefficients and
/// for `gen_decode_bench`'s wire trace.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        self.next() as f64 / u64::MAX as f64
    }
}

// ───────────────────────────── the axes ─────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pattern {
    Uniform,
    Ge,
}
impl Pattern {
    fn tag(self) -> &'static str {
        match self {
            Pattern::Uniform => "unif",
            Pattern::Ge => "ge",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clock {
    /// The estimator's app-echo RTT: RTprop + wire queue + STORE DWELL.
    App,
    /// `QuicTransport::wire_rtt`: RTprop + wire queue, dwell excluded.
    Wire,
}
impl Clock {
    fn tag(self) -> &'static str {
        match self {
            Clock::App => "app",
            Clock::Wire => "wire",
        }
    }
}

/// A gate arm. These are ENV GATES (A/B attribution arms), NOT dials on the
/// (δ, ρ, r) triangle — nothing here keys a law on δ or ρ.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Arm {
    name: &'static str,
    recov_mp: bool,
    recov_sp: bool,
    patience_derived: bool,
}
const ARMS: &[Arm] = &[
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
enum Chan {
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
const CHANS: [Chan; 5] = [Chan::Time, Chan::Fast, Chan::LegacyAge, Chan::Sweep, Chan::Shed];
impl Chan {
    fn tag(self) -> &'static str {
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
struct Cell {
    rtprop_us: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    arm: Arm,
    seed: u64,
}

/// Fixed inputs shared by every cell (the operating-point calibration).
#[derive(Clone, Copy)]
struct Calib {
    n_src: u64,
    mbps: f64,
    skew_us: u64,
    wireq_us: u64,
    dwell_us: u64,
    jitter_us: u64,
    shed: bool,
    budget: usize,
}
impl Calib {
    fn from_env() -> Self {
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
    fn fixture() -> Self {
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
const MEAN_BURST: f64 = 8.0;
struct LossChain {
    pattern: Pattern,
    p: f64,
    p_gb: f64,
    p_bg: f64,
    bad: bool,
}
impl LossChain {
    fn new(pattern: Pattern, p: f64) -> Self {
        let p_bg = 1.0 / MEAN_BURST;
        let p_gb = (p * p_bg) / (1.0 - p).max(1e-9);
        Self { pattern, p, p_gb, p_bg, bad: false }
    }
    fn drops(&mut self, rng: &mut Rng) -> bool {
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
enum Ev {
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
struct Hole {
    lost_at_us: u64,
    first_service_us: Option<u64>,
    delivered_us: Option<u64>,
    chan: Option<Chan>,
    services: u32,
}

#[derive(Default, Clone)]
struct Counts {
    retx: u64,
    sweeps: u64,
    reports: u64,
    supp_cool: u64,
    supp_law: u64,
    supp_age: u64,
    shed: u64,
}

struct Out {
    holes: Vec<Hole>,
    counts: Counts,
    /// The §6.1.2 threshold on path 0 (µs) — the arm's PATIENCE.
    patience_us: u64,
    /// The pooled clock the legacy gate / cooldown / sweep all read (µs).
    pooled_us: u64,
    cooldown_us: u64,
    refresh_us: u64,
}

/// The ordinary SACK-bearing WindowAck floor (`GAP_ACK_MIN_INTERVAL`, 2 ms).
const GAP_ACK_MIN_US: u64 = 2_000;

fn run_cell(cell: Cell, cal: Calib) -> Out {
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
                let next = if arrived_since_ack > 0 {
                    now + GAP_ACK_MIN_US
                } else {
                    // Nothing arrived: the stalled-hole refresh cadence owns
                    // the next advertisement.
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
                let acked: Vec<u64> = retransmit_buffer.range(..frontier).map(|(&s, _)| s).collect();
                for s in acked {
                    retransmit_buffer.remove(&s);
                    nack_retx_at.remove(&s);
                }
                if !sweep {
                    for (lo, hi) in mp_delivered_intervals(&gaps) {
                        let del: Vec<u64> =
                            retransmit_buffer.range(lo..=hi).map(|(&s, _)| s).collect();
                        for s in del {
                            retransmit_buffer.remove(&s);
                            nack_retx_at.remove(&s);
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
    Out { holes: hv, counts, patience_us, pooled_us, cooldown_us, refresh_us }
}

// ───────────────────────────── reporting ────────────────────────────────

fn pct(v: &[u64], q: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    let i = ((((v.len() - 1) as f64) * q).round() as usize).min(v.len() - 1);
    v[i]
}

/// A cell's pooled result across seeds.
struct Row {
    rtprop_us: u64,
    loss: f64,
    pattern: Pattern,
    n_paths: usize,
    clock: Clock,
    patience_us: u64,
    pooled_us: u64,
    cooldown_us: u64,
    refresh_us: u64,
    n_holes: u64,
    n_served: u64,
    n_delivered: u64,
    svc_ms: Vec<u64>, // hole → first service, µs, sorted
    del_ms: Vec<u64>, // hole → delivered, µs, sorted
    mix: [u64; 5],
    redundant: u64,
    counts: Counts,
}

fn pool(cells: &[(Cell, Out)]) -> Row {
    let c0 = cells[0].0;
    let o0 = &cells[0].1;
    let mut row = Row {
        rtprop_us: c0.rtprop_us,
        loss: c0.loss,
        pattern: c0.pattern,
        n_paths: c0.n_paths,
        clock: c0.clock,
        patience_us: o0.patience_us,
        pooled_us: o0.pooled_us,
        cooldown_us: o0.cooldown_us,
        refresh_us: o0.refresh_us,
        n_holes: 0,
        n_served: 0,
        n_delivered: 0,
        svc_ms: Vec::new(),
        del_ms: Vec::new(),
        mix: [0; 5],
        redundant: 0,
        counts: Counts::default(),
    };
    for (_, o) in cells {
        row.n_holes += o.holes.len() as u64;
        for h in &o.holes {
            if let Some(t) = h.first_service_us {
                row.n_served += 1;
                row.svc_ms.push(t.saturating_sub(h.lost_at_us));
            }
            if let Some(t) = h.delivered_us {
                row.n_delivered += 1;
                row.del_ms.push(t.saturating_sub(h.lost_at_us));
            }
            if let Some(c) = h.chan {
                let i = CHANS.iter().position(|&x| x == c).unwrap();
                row.mix[i] += 1;
            }
            row.redundant += h.services.saturating_sub(1) as u64;
        }
        row.counts.retx += o.counts.retx;
        row.counts.sweeps += o.counts.sweeps;
        row.counts.reports += o.counts.reports;
        row.counts.supp_cool += o.counts.supp_cool;
        row.counts.supp_law += o.counts.supp_law;
        row.counts.supp_age += o.counts.supp_age;
        row.counts.shed += o.counts.shed;
    }
    row.svc_ms.sort_unstable();
    row.del_ms.sort_unstable();
    row
}

fn mix_str(mix: &[u64; 5]) -> String {
    let tot: u64 = mix.iter().sum();
    if tot == 0 {
        return "-".into();
    }
    CHANS
        .iter()
        .enumerate()
        .filter(|(i, _)| mix[*i] > 0)
        .map(|(i, c)| format!("{}{:.0}%", c.tag(), 100.0 * mix[i] as f64 / tot as f64))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ms(us: u64) -> f64 {
    us as f64 / 1_000.0
}

#[test]
#[ignore]
fn recovery_bench() {
    let cal = Calib::from_env();
    let rtprops = list_u64("RWM_RB_RTPROP_MS", "5,10,20,50,100,200");
    let losses = list_f64("RWM_RB_LOSS", "0.001,0.01,0.026,0.05");
    let patterns: Vec<Pattern> = list_str("RWM_RB_PATTERN", "uniform,ge")
        .iter()
        .map(|s| if s == "ge" { Pattern::Ge } else { Pattern::Uniform })
        .collect();
    let paths = list_u64("RWM_RB_PATHS", "1,2");
    let clocks: Vec<Clock> = list_str("RWM_RB_CLOCK", "app,wire")
        .iter()
        .map(|s| if s == "wire" { Clock::Wire } else { Clock::App })
        .collect();
    let arm_names = list_str("RWM_RB_ARMS", "shipped,legacy,sp,pd");
    let arms: Vec<Arm> =
        ARMS.iter().copied().filter(|a| arm_names.iter().any(|n| n == a.name)).collect();
    let seeds = list_u64("RWM_RB_SEEDS", "42,7");

    println!("\n=== RECOVERY PLANE COMPONENT BENCH (goal-gate \"Component Benches\") ===");
    println!(
        "N={} src @ {:.1} Mbit/s ({} µs/sym) · skew {} ms · wireQ {} ms · dwell {} ms · jitter {} ms · shed {} · budget {}/report",
        cal.n_src,
        cal.mbps,
        (((1_200.0 * 8.0) / (cal.mbps * 1e6)) * 1e6) as u64,
        cal.skew_us / 1000,
        cal.wireq_us / 1000,
        cal.dwell_us / 1000,
        cal.jitter_us / 1000,
        if cal.shed { "ON" } else { "off" },
        cal.budget
    );
    println!("seeds {seeds:?} · kTimeThreshold 9/8 · kPacketThreshold 3 · GAP_ACK_MIN 2 ms");

    // rows[(arm, cellkey)] for the cross-arm summaries below.
    let mut all: Vec<(&'static str, Row)> = Vec::new();
    let t0 = std::time::Instant::now();

    for arm in &arms {
        println!("\n--- arm `{}` (recov_mp={} recov_sp={} patience_derived={}) ---",
            arm.name, arm.recov_mp, arm.recov_sp, arm.patience_derived);
        println!(
            "{:>4} {:>6} {:>5} {:>3} {:>5} | {:>9} {:>8} {:>8} {:>7} | {:>6} {:>5} | {:>7} {:>7} {:>7} {:>8} | {:<26} | {:>6} {:>6} {:>6} | {}",
            "rtp", "loss", "pat", "np", "clk",
            "patience", "pooled", "cooldwn", "refresh",
            "holes", "svc%",
            "p50 ms", "p90", "p99", "max",
            "channel mix (first service)",
            "retx", "sweep", "redun",
            "supp c/l/a"
        );
        for &rtprop_ms in &rtprops {
            for &loss in &losses {
                for &pattern in &patterns {
                    for &np in &paths {
                        for &clock in &clocks {
                            let cells: Vec<(Cell, Out)> = seeds
                                .iter()
                                .map(|&seed| {
                                    let c = Cell {
                                        rtprop_us: rtprop_ms * 1_000,
                                        loss,
                                        pattern,
                                        n_paths: np as usize,
                                        clock,
                                        arm: *arm,
                                        seed,
                                    };
                                    let o = run_cell(c, cal);
                                    (c, o)
                                })
                                .collect();
                            let r = pool(&cells);
                            println!(
                                "{:>4} {:>6.3} {:>5} {:>3} {:>5} | {:>9.1} {:>8.1} {:>8.1} {:>7.1} | {:>6} {:>4.0}% | {:>7.1} {:>7.1} {:>7.1} {:>8.1} | {:<26} | {:>6} {:>6} {:>6} | {}/{}/{}",
                                rtprop_ms, loss, pattern.tag(), np, clock.tag(),
                                ms(r.patience_us), ms(r.pooled_us), ms(r.cooldown_us), ms(r.refresh_us),
                                r.n_holes,
                                if r.n_holes > 0 { 100.0 * r.n_served as f64 / r.n_holes as f64 } else { 0.0 },
                                ms(pct(&r.svc_ms, 0.50)), ms(pct(&r.svc_ms, 0.90)),
                                ms(pct(&r.svc_ms, 0.99)), ms(*r.svc_ms.last().unwrap_or(&0)),
                                mix_str(&r.mix),
                                r.counts.retx, r.counts.sweeps, r.redundant,
                                r.counts.supp_cool, r.counts.supp_law, r.counts.supp_age
                            );
                            all.push((arm.name, r));
                        }
                    }
                }
            }
        }
    }

    // ── (a) the patience claim, per cell ──
    println!("\n=== (a) PATIENCE vs RTprop — app-echo clock vs wire clock ===");
    println!("{:>4} {:>3} | {:>12} {:>12} | {:>12} {:>12} | {:>10}",
        "rtp", "np", "patience app", "×RTprop", "patience wire", "×RTprop", "app/wire");
    for &rtprop_ms in &rtprops {
        for &np in &paths {
            let g = |clk: Clock| {
                all.iter()
                    .find(|(a, r)| {
                        *a == "shipped"
                            && r.rtprop_us == rtprop_ms * 1000
                            && r.n_paths == np as usize
                            && r.clock == clk
                    })
                    .map(|(_, r)| r.patience_us)
            };
            if let (Some(a), Some(w)) = (g(Clock::App), g(Clock::Wire)) {
                println!(
                    "{:>4} {:>3} | {:>12.1} {:>12.1} | {:>12.1} {:>12.1} | {:>10.2}",
                    rtprop_ms, np,
                    ms(a), a as f64 / (rtprop_ms * 1000) as f64,
                    ms(w), w as f64 / (rtprop_ms * 1000) as f64,
                    a as f64 / w as f64
                );
            }
        }
    }

    // ── (b) does the FAST (packet-threshold) channel get to fire? ──
    println!("\n=== (b) the FAST channel (RFC 9002 §6.1.1, kPacketThreshold=3) ===");
    println!("shipped arm, np=2 (the only configuration in which it is armed):");
    println!("{:>4} {:>6} {:>5} {:>5} | {:>6} {:>7} {:>7} | {:>9} {:>9}",
        "rtp", "loss", "pat", "clk", "holes", "fast", "time", "fast share", "svc p50 ms");
    for (a, r) in &all {
        if *a != "shipped" || r.n_paths != 2 {
            continue;
        }
        let fast = r.mix[1];
        let time = r.mix[0];
        let tot: u64 = r.mix.iter().sum();
        println!(
            "{:>4} {:>6.3} {:>5} {:>5} | {:>6} {:>7} {:>7} | {:>8.1}% {:>9.1}",
            r.rtprop_us / 1000, r.loss, r.pattern.tag(), r.clock.tag(),
            r.n_holes, fast, time,
            if tot > 0 { 100.0 * fast as f64 / tot as f64 } else { 0.0 },
            ms(pct(&r.svc_ms, 0.50))
        );
    }

    // ── (c) THE L1 PREDICTION ──
    println!("\n=== (c) PREDICTION FOR L1 (c7 cell: RTprop 10 ms, GE 2.6 %, np=2) ===");
    for arm in &arms {
        let g = |clk: Clock| {
            all.iter().find(|(a, r)| {
                *a == arm.name
                    && r.rtprop_us == 10_000
                    && (r.loss - 0.026).abs() < 1e-9
                    && r.pattern == Pattern::Ge
                    && r.n_paths == 2
                    && r.clock == clk
            })
        };
        if let (Some((_, a)), Some((_, w))) = (g(Clock::App), g(Clock::Wire)) {
            let rr = |x: u64, y: u64| if y == 0 { f64::NAN } else { x as f64 / y as f64 };
            println!(
                "arm {:<8}: retx {} → {} (×{:.2})  sweeps {} → {} (×{:.2})  redundant {} → {} (×{:.2})  svc p50 {:.1} → {:.1} ms (×{:.2})",
                arm.name,
                a.counts.retx, w.counts.retx, rr(w.counts.retx, a.counts.retx),
                a.counts.sweeps, w.counts.sweeps, rr(w.counts.sweeps, a.counts.sweeps),
                a.redundant, w.redundant, rr(w.redundant, a.redundant),
                ms(pct(&a.svc_ms, 0.50)), ms(pct(&w.svc_ms, 0.50)),
                pct(&w.svc_ms, 0.50) as f64 / pct(&a.svc_ms, 0.50).max(1) as f64
            );
        }
    }
    println!("\n{} cells in {:.2} s", all.len(), t0.elapsed().as_secs_f64());
}

// ────────────────────── regression fixtures (fast, CI) ──────────────────

/// Characteristic bench outputs pinned as fixtures so the recovery plane's
/// behaviour cannot drift silently. These are ABSOLUTE assertions on the
/// plane's own laws at named operating points (CLAUDE.md's testing
/// discipline: ordinal tests do not catch routing bugs), and they double as
/// the liveness proof that the mechanism under test actually executes —
/// every fixture asserts a non-zero service count through a NAMED channel.
///
/// If a law changes deliberately, re-run the bench and update the numbers in
/// the same commit, saying so.
#[test]
fn recovery_bench_fixtures_pin_the_plane() {
    let cal = Calib::fixture();
    let base = Cell {
        rtprop_us: 10_000,
        loss: 0.026,
        pattern: Pattern::Uniform,
        n_paths: 2,
        clock: Clock::App,
        arm: ARMS[0], // shipped
        seed: 42,
    };
    let p50 = |o: &Out| {
        let mut v: Vec<u64> =
            o.holes.iter().filter_map(|h| h.first_service_us.map(|t| t - h.lost_at_us)).collect();
        v.sort_unstable();
        pct(&v, 0.5)
    };
    let mix = |o: &Out| {
        let mut m = [0u64; 5];
        for h in &o.holes {
            if let Some(c) = h.chan {
                m[CHANS.iter().position(|&x| x == c).unwrap()] += 1;
            }
        }
        m
    };

    // ── FIXTURE 1: the clock LAWS at the c7 operating point (dual path).
    // Path 0's app-echo RTT is the c7-measured 158 ms (RTprop 10 + wireQ 4 +
    // dwell 144), so patience = 9/8 × 158 = 177.75 ms; the pooled clock is
    // the SKEWED path's app-echo (2×(5+5) + 4 + 144 = 168 ms) and the
    // per-seq cooldown equals it. The receiver's refresh reads the COPA
    // clock, so it is 2 × 24 = 48 ms in BOTH clock arms.
    let a = run_cell(base, cal);
    assert_eq!(a.patience_us, 177_750, "path-0 §6.1.2 threshold, app-echo");
    assert_eq!(a.pooled_us, 168_000, "pooled recovery clock = max app-echo");
    assert_eq!(a.cooldown_us, 168_000, "per-seq cooldown = pooled clock");
    assert_eq!(a.refresh_us, 48_000, "hole refresh reads COPA, not the estimator");

    let w = run_cell(Cell { clock: Clock::Wire, ..base }, cal);
    assert_eq!(w.patience_us, 15_750, "path-0 §6.1.2 threshold, wire");
    assert_eq!(w.pooled_us, 24_000, "pooled = skewed path's wire RTT");
    assert_eq!(w.cooldown_us, 24_000);
    assert_eq!(w.refresh_us, 48_000, "unchanged: the receiver reads Copa srtt");
    assert_eq!(a.holes.len(), w.holes.len(), "same seed ⇒ same wire losses");
    assert!(
        a.patience_us > 11 * w.patience_us,
        "the clock ARGUMENT, not the constants, sets patience: {} vs {}",
        a.patience_us,
        w.patience_us
    );

    // ── FIXTURE 2: the SHIPPED plane's measured behaviour at that point.
    // Every number below is a bench output, not a derivation.
    assert_eq!(a.holes.len(), FIX_HOLES);
    assert_eq!(mix(&a), FIX_APP_MIX, "app-echo channel mix (time/fast/age/sweep/shed)");
    assert_eq!(a.counts.retx, FIX_APP_RETX);
    assert_eq!(a.counts.sweeps, FIX_APP_SWEEPS);
    assert_eq!(p50(&a), FIX_APP_P50_US);
    assert_eq!(mix(&w), FIX_WIRE_MIX, "wire channel mix");
    assert_eq!(w.counts.retx, FIX_WIRE_RETX);
    assert_eq!(w.counts.sweeps, FIX_WIRE_SWEEPS);
    assert_eq!(p50(&w), FIX_WIRE_P50_US);
    // Liveness: every arm must actually serve holes through a named channel.
    for (tag, o) in [("app", &a), ("wire", &w)] {
        assert!(mix(o).iter().sum::<u64>() > 0, "{tag}: no hole reached a channel");
        assert!(o.counts.retx > 0, "{tag}: no retransmit fired");
    }

    // ── FIXTURE 3: the MP law's N ≤ 1 bypass is BIT-EXACT — at one path the
    // shipped arm must reproduce the legacy age gate exactly. (`mp_hole_ripe`
    // returns true unconditionally at N ≤ 1; this is that claim, measured.)
    let sp1 = run_cell(Cell { n_paths: 1, ..base }, cal);
    let lg1 = run_cell(Cell { n_paths: 1, arm: ARMS[1], ..base }, cal);
    assert_eq!(sp1.pooled_us, 158_000, "np=1: 2×5 + 4 + 144 = the c7 app-echo");
    assert_eq!(sp1.cooldown_us, 158_000);
    assert_eq!(sp1.refresh_us, 28_000, "2 × 14 ms Copa, inside the [25,100] clamp");
    assert_eq!(mix(&sp1), mix(&lg1), "N<=1 bypass must be bit-exact vs legacy");
    assert_eq!(sp1.counts.retx, lg1.counts.retx);
    assert_eq!(p50(&sp1), p50(&lg1));
    assert_eq!(mix(&lg1)[2], lg1.holes.len() as u64, "np=1 shipped ⇒ 100% legacy age gate");

    // ── FIXTURE 4: THE ASYMMETRY `RWM_RECOV_SP` inherits. At N = 1 the
    // §6.1.2 channel waits 9/8·srtt where the legacy gate waits srtt/2 — so
    // on the app-echo clock, arming SP makes recovery STRICTLY SLOWER, by
    // the ratio (9/8)/(1/2) = 2.25. That is a component fact about the
    // clock's ARGUMENT, not about the RFC channel.
    let sp_arm = run_cell(Cell { n_paths: 1, arm: ARMS[2], ..base }, cal);
    assert_eq!(mix(&sp_arm)[0], sp_arm.holes.len() as u64, "SP at np=1 ⇒ 100% time channel");
    assert!(
        p50(&sp_arm) > 2 * p50(&lg1),
        "SP on the app-echo clock must be >2x slower than the legacy gate: {} vs {}",
        p50(&sp_arm),
        p50(&lg1)
    );
    // On the WIRE clock the same arm is fast in absolute terms — the channel
    // was never the problem.
    let sp_wire = run_cell(Cell { n_paths: 1, arm: ARMS[2], clock: Clock::Wire, ..base }, cal);
    assert!(
        p50(&sp_wire) * 5 < p50(&sp_arm),
        "wire-clocked SP must be >5x faster than app-clocked SP: {} vs {}",
        p50(&sp_wire),
        p50(&sp_arm)
    );
}

// The measured fixture values (bench outputs, `Calib::fixture`, seed 42,
// RTprop 10 ms / uniform 2.6 % / np = 2 / arm `shipped`).
const FIX_HOLES: usize = 154;
/// 100 % FAST: on the app-echo clock the §6.1.2 channel (177.75 ms) never
/// ripens inside a hole's life, so the packet-threshold channel serves
/// EVERY hole. That is the answer to bench question (b), pinned.
const FIX_APP_MIX: [u64; 5] = [0, 154, 0, 0, 0];
const FIX_APP_RETX: u64 = 154;
const FIX_APP_SWEEPS: u64 = 1;
const FIX_APP_P50_US: u64 = 16_600;
/// On the wire clock the time channel ripens at 15.75 ms and takes half the
/// holes — the channel mix, not the retx total, is what the argument moves.
const FIX_WIRE_MIX: [u64; 5] = [75, 79, 0, 0, 0];
const FIX_WIRE_RETX: u64 = 157;
const FIX_WIRE_SWEEPS: u64 = 3;
const FIX_WIRE_P50_US: u64 = 16_600;
