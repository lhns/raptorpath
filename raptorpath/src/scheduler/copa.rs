//! Copa-lite congestion control: the δ price, the competitive-mode
//! constants, the delay/jitter/anchor constants and `CopaState` with its
//! rate/RTT samplers. Moved verbatim out of `scheduler/mod.rs` (cleanup
//! Stage 3); re-exported from `scheduler`.

use super::*;

/// Copa congestion control parameter: target queue depth.
/// d_copa = 0.5 targets ~2 packets of queue. See paper Section 12.4.
/// Units: 1/symbols — rate = 1/(d_copa [1/sym] × dq [s]) is symbols/second.
pub(crate) const COPA_DELTA: f64 = 0.5;

/// Hint→δ mapping (paper §12.4, wire-signal addendum): Copa's utility is
/// U = log(throughput) − δ·log(delay), so δ IS the marginal latency price.
/// The protocol hint already declares exactly one price ratio — the
/// tail-loss-target scale ζ (`ProtocolHint::tail_loss_scale`, Realtime 0.01
/// / Auto 1 / Bulk 100: Realtime prices lateness 100× dearer, Bulk 100×
/// cheaper). Anchoring Auto at the Copa-paper default δ = 0.5 gives the
/// continuous, constant-free mapping
///
///   δ(hint) = COPA_DELTA / ζ(hint)   ∈ {50 (Realtime), 0.5 (Auto),
///                                        0.005 (Bulk)}
///
/// Equilibrium standing queue = 1/δ packets (rate = 1/(δ·d_q) at the
/// bottleneck rate μ ⇒ q = 1/δ), i.e. d_q* = 1/(δ·μ): Bulk tolerates 200
/// symbols of queue (≈19 ms at the c2 cell's 10.4 k sym/s — still ~3×
/// tighter than BBR-under's measured 65–87 ms), Realtime targets an
/// essentially empty queue (jitter headroom governs), Auto reproduces the
/// classic δ = 0.5 two-packet target. `over` = RWM_COPA_DELTA (the
/// δ-frontier measurement knob), which overrides the hint when set.
///
/// THE HINT→δ MAP ITSELF now lives ONCE, in [`hint_delta_price`], and is
/// read through [`crate::net::delta_price`] — the ONE place a hint names a
/// δ (§16.81). This function is the CC's own view of that number:
/// `RWM_COPA_DELTA` overrides it for the CC alone, so a battery can move the
/// contract's δ (`RWM_DELTA`) while pinning the congestion controller.
/// PRECEDENCE: `RWM_COPA_DELTA` ▸ `RWM_DELTA` ▸ the hint's map.
pub(crate) fn copa_delta(hint: ProtocolHint, over: Option<f64>) -> f64 {
    over.filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or_else(|| crate::net::delta_price(hint))
}

/// The hint→δ MAP, with no override of any kind applied: δ(hint) =
/// COPA_DELTA / ζ(hint) ∈ {50, 0.5, 0.005} (paper §12.4). The ONE
/// transcription of it in the engine; [`crate::net::delta_price`] is its
/// public seat and every δ-priced law reads THAT.
pub(crate) fn hint_delta_price(hint: ProtocolHint) -> f64 {
    COPA_DELTA / hint.tail_loss_scale()
}

/// `copa_delta` with the RWM_COPA_DELTA env override applied.
pub(crate) fn copa_delta_for_hint(hint: ProtocolHint) -> f64 {
    // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09), emitted
    // ONCE per process even though this function is re-entered on every hint
    // change — the fixed-δ probe of the "Copa Competitive Mode" battery is an
    // arm whose only distinguishing knob is this override, so it needs an
    // assertable echo; the OnceLock keeps it off the repeat path.
    {
        use std::sync::OnceLock;
        static ECHOED: OnceLock<()> = OnceLock::new();
        ECHOED.get_or_init(|| {
            tracing::info!(
                copa_delta_override =
                    std::env::var("RWM_COPA_DELTA").as_deref().unwrap_or("unset"),
                "Copa δ override (RWM_COPA_DELTA; unset = the hint→δ mapping)"
            );
        });
    }
    let over = std::env::var("RWM_COPA_DELTA")
        .ok()
        .and_then(|s| s.parse::<f64>().ok());
    copa_delta(hint, over)
}

// --- Copa TCP-competitive mode (feat/copa-compete, task: roadmap item 6) ---
//
// Copa §2.2 (Arun & Balakrishnan, "Copa: Practical Delay-Based Congestion
// Control for the Internet", NSDI 2018) defines TWO operating modes:
//
//   1. the DEFAULT mode (δ fixed — the paper's 0.5; here the hint-mapped
//      δ(hint), see `copa_delta`), and
//   2. a COMPETITIVE mode "where δ is adjusted dynamically to match the
//      aggressiveness of typical buffer-filling schemes".
//
// Detection (verbatim mechanism from the paper): Copa's own dynamics empty
// the bottleneck queue at least once every 5·RTT when only Copa flows share
// it (paper §3). A concurrent long-running buffer-filling flow (Cubic,
// NewReno) breaks that periodicity. "Hence if the sender sees a 'nearly
// empty' queue in the last 5 RTTs, it remains in the default mode;
// otherwise, it switches to competitive mode. We estimate 'nearly empty' as
// any queuing delay lower than 10% of the rate oscillations in the last
// four RTTs; i.e., d_q < 0.1·(RTTmax − RTTmin) where RTTmax is measured
// over the past four RTTs and RTTmin is our long-term minimum" — the
// RTTmax term self-calibrates the notion of "nearly empty" to the path's
// short-term RTT variance.
//
// Competitive law (paper §2.2): "In competitive mode the sender varies 1/δ
// according to whatever buffer-filling algorithm one wishes to emulate
// (e.g., NewReno, Cubic, etc.). In our implementation we perform AIMD on
// 1/δ based on packet success or loss" — NewReno-style: additive increase
// of 1/δ by 1 per RTT without loss, multiplicative decrease (halve 1/δ) on
// a loss event. "In competitive mode, δ ≤ 0.5. When Copa switches from
// competitive mode to default mode, it resets δ to 0.5."
//
// Composition with the hint→δ mapping (ours): the paper's 0.5 is its
// default-mode δ; ours is δ_base = δ(hint). The faithful generalization
// keeps the hint as the BASE price and lets competition adapt AROUND it:
// competitive mode enters at δ = δ_base, AIMD keeps 1/δ ≥ 1/δ_base (the
// paper's "δ ≤ 0.5" with 0.5 → δ_base), and switch-back resets δ = δ_base.
// The loss signal is quinn's wire-level loss detection (the pass-through
// shim's recorded `congestion_events` — the same packet-timed layer as the
// wire d_q clock); FEC recovery is irrelevant here because the AIMD term
// only prices AGGRESSIVENESS against a loss-based competitor, it never
// gates delivery (loss handling stays the FEC layer's job, §12.1).
//
// Hysteresis is the paper's own: the 5-RTT nearly-empty observation window
// on both edges (a competitive-mode Copa cohort still empties the queue
// every 5 RTT if no buffer-filler is present, so an erroneous or stale
// switch self-corrects within a few RTTs; the paper accepts brief flaps by
// design). Gated: `RWM_COPA_COMPETE` (default OFF) and only meaningful on
// top of the wire-clocked signal (the δ-mapped update law is what the
// adapted δ feeds). Env unset ⇒ every path byte-identical.

/// Nearly-empty threshold coefficient (Copa §2.2: d_q < 0.1·(RTTmax−RTTmin)).
pub(crate) const COMPETE_EMPTY_FRAC: f64 = 0.1;
/// Detection window: no nearly-empty queue in the last 5 RTTs ⇒ competitive.
pub(crate) const COMPETE_WINDOW_RTTS: f64 = 5.0;
/// RTTmax lookback for the nearly-empty calibration (paper: past 4 RTTs).
pub(crate) const COMPETE_RTTMAX_RTTS: f64 = 4.0;
/// Bound on 1/δ in competitive mode: 2/δ (the coupling cap's dither term)
/// may never exceed MAX_CWND, so the AIMD's additive growth cannot decouple
/// cwnd from the store the way the uncapped v1 law did (see the coupling-cap
/// note in `wire_update_cwnd`).
pub(crate) const COMPETE_INV_DELTA_MAX: f64 = PathState::MAX_CWND as f64 / 2.0;

/// Floor on the queuing-delay estimate dq, in seconds (0.1 ms).
///
/// Two jobs, both continuity guards (no branch cliffs):
///   - `copa_target_cwnd()` divides by dq; on a LAN where a sample can equal
///     the floor exactly, dq → 0 would explode the target to infinity.
///   - The backoff threshold (queue_mult − 1) × floor collapses toward 0 on
///     sub-millisecond-RTT links; flooring both dq and the threshold at the
///     same 0.1 ms means jitter at the clamp boundary cannot trigger a
///     spurious backoff (dq == threshold is not > threshold).
pub(crate) const DQ_FLOOR_SECS: f64 = 1e-4;

/// Jitter headroom multiplier k in the backoff threshold
/// (queue_mult − 1) × floor + k × jitter_est (paper Section 12.4,
/// jitter-adjusted queue target).
///
/// The P1 mapping assumed path jitter ≪ the queue target. Real links
/// violate that: at L1's C2 cell (10ms floor, ±3ms/direction netem
/// jitter) the Bulk threshold was 2.5ms while a typical RTT sample sat
/// ~6ms above the 10s floor — the windowed-min queue signal measured
/// JITTER, not queue, and every per-SRTT update bought a ×0.92 backoff
/// (cwnd pinned near the floor; measured L1 root cause of the 16x
/// rp-vs-quinn gap at C2). Widening the threshold by k×jitter makes the
/// comparison read "queue above target AND above what jitter alone
/// explains". k = 2 puts the false-backoff rate for a min-of-N window
/// at the few-percent level for the N ≈ 4-30 ACK batches an SRTT holds,
/// while a genuine standing queue (which shifts ALL samples, leaving
/// the consecutive-difference jitter estimate unchanged) still crosses
/// the widened threshold within a few updates. Continuity: jitter → 0
/// recovers the P1 threshold exactly.
pub(crate) const JITTER_HEADROOM: f64 = 2.0;
/// EWMA gain for the consecutive-difference jitter estimator (RFC
/// 3550-style interarrival jitter, gain 1/8 rather than 1/16: the ramp
/// fast-exit consults the threshold from the first ACKs on, so the
/// estimate must converge within tens of samples).
pub(crate) const JITTER_GAIN: f64 = 0.125;

/// EWMA gain for the `rvar_us=` CANDIDATE DISPERSION GAUGE — **RFC 6298 §2's
/// own `β`, and its provenance is the RFC.**
///
/// `RTTVAR ← (1 − β)·RTTVAR + β·|SRTT − R'|`, β = 1/4, verbatim. The same
/// constant RFC 8985 §6.2 inherits for RACK. **CITED, never fitted** — this is
/// the constant the CLAUDE.md FORMULA-FIRST rule asks for a reference for, and
/// the reference is the standard the shipped `rtt_var_sq` EWMA already cites
/// for the identical gain on the SECOND moment.
pub(crate) const SIGMA_CAND_RVAR_GAIN: f64 = 0.25;

/// Window length `L` for the two WINDOW-CLASS candidate dispersion gauges
/// (`qsp_us=`, `msd_us=`) — the count of most-recent raw RTT samples held.
///
/// **Why a window at all, and why 256.** The shipped `sig_us` EWMA at β = 1/4
/// has an effective memory of `N_eff = (2 − β)/β = 7 samples`. It is a
/// SEVEN-SAMPLE estimate no matter what its `n` reads, which is precisely why
/// "converged at `n` ≈ 18 000" did not mean converged: the plain-window
/// primitives measured `σ(c8)` at 0.191 / 3.140 / 54.836 ms across three reps
/// at that same `n` (goal-gate, plain-window scored result §4 — a 287× spread
/// that survived two sessions because the `n` column looked converged). `n`
/// counts how long the gauge has been FED; it does not count what is IN the
/// reading.
///
/// 256 is `L` for two reasons, both stated before any candidate was measured:
/// it is **36× the EWMA's memory**, so the memory axis is separated from the
/// functional axis by more than an order of magnitude; and the `P90` these
/// gauges take needs its tail to rest on real order statistics —
/// `L·(1 − 0.90) = 25.6` clears the standard ≥ 10 requirement by 2.6×. It is
/// also 1.4 % of `c8`'s per-rep sample budget, so a window-class `n_warm = L`
/// clears the pre-registered `C2` bar (`n_warm ≤ 883` at `c8`) by 3.4×.
///
/// **Resource bound, stated OUTSIDE the law** (FORMULA-FIRST): 256 × 4 B =
/// 1 KiB per path, and the sort that reads it is `O(L log L)` at the `[DIAG]`
/// cadence only — the feed site stays `O(1)`.
pub(crate) const SIGMA_CAND_WINDOW: usize = 256;

/// Band width `c` for the FIXED-TIME-LAG dispersion gauge `tlag_us=`
/// (**paper §16.75**): a pair is admitted iff its lag falls in `[τ, c·τ]`.
///
/// **DECLARED RESOURCE BOUND, stated as one** (FORMULA-FIRST), not a fitted
/// constant — and it does not change the estimand, which §16.75.2 pins to the
/// band's POSITION at `τ = RTprop`. One octave is the standard dyadic binning
/// of a structure function, and the arithmetic of what it buys is in the paper:
/// at `c8L`'s slow sender leg (581 samples/s, spacing 1.72 ms) against
/// `RTprop ≈ 38 ms` the band `[38, 76] ms` is 22 spacings wide, so a partner
/// exists for essentially every anchor; at the battery's sparsest leg (`c8L`
/// receiver, 23.9 samples/s ⇒ 41.8 ms spacing) exactly one spacing fits, which
/// is the boundary case the `n` column and the parser-side `UNSCOREABLE-THIN`
/// rule exist for.
pub(crate) const SIGMA_TLAG_BAND_C: u32 = 2;

/// Decimation factor `m` for `tlag_us=`: a sample enters the gauge's ring only
/// if `τ/m` has elapsed since the last admitted one (**paper §16.75.4**).
///
/// **DECLARED RESOURCE BOUND, stated as one, and it is a MEMORY measure rather
/// than a filter.** It caps the ring's sample rate at `m/τ` REGARDLESS of the
/// ack rate, so a `SIGMA_CAND_WINDOW`-deep ring spans `256·τ/8 = 32·τ` at every
/// leg. Without it the ring at a 20 kHz sender leg would span 12.8 ms — far
/// less than one `RTprop` — the band `[τ, 2τ]` would contain **no pairs at
/// all**, and the gauge would render `-` at exactly the legs it is built for.
///
/// **It does not change the estimand**: rate invariance comes from selecting
/// pairs by elapsed TIME (§16.75.2), and any fixed `m` yields it. `m` sets only
/// the resolution of the realized lag inside the band — `τ/8`, i.e. 12.5 % of
/// `τ`.
pub(crate) const SIGMA_TLAG_DECIM_M: u32 = 8;

/// Quantile of the per-update window-min history used as the QUEUE floor
/// (paper Section 12.4, jitter-robust queue floor).
///
/// The queuing-delay signal compares a min-of-N statistic (N ≈ the ACK
/// samples in one SRTT window) against the propagation floor, a
/// min-of-thousands over 10s. On a jittery link those are DIFFERENT
/// statistics: at L1's C2 cell the 10s floor found 7.0ms while a
/// typical window min sits at 12-13ms — a permanent apparent dq of
/// ~5ms with an empty queue (and netem's jitter FIFO correlates
/// consecutive samples, so the consecutive-difference jitter estimate
/// ~0.85ms cannot bridge the gap). Comparing the window min against a
/// low QUANTILE of its own recent distribution is self-calibrating
/// under any jitter correlation structure: queue-free windows sit near
/// their own P10 by construction, while a genuine standing queue
/// shifts every window min up within one SRTT and the 10s-window
/// quantile lags behind — the signal survives. On a clean link every
/// window min equals the floor, the quantile equals the floor, and
/// the P1 semantics are recovered exactly.
pub(crate) const QUEUE_FLOOR_QUANTILE: f64 = 0.10;

/// Startup ramp: multiplicative growth factor per window update, until the
/// first backoff (gate driver P1: cwnd = cwnd × 1.5 + 1).
pub(crate) const RAMP_GAIN: f64 = 1.5;
/// Steady state: additive increase per window update (symbols).
pub(crate) const ADDITIVE_STEP: f64 = 2.0;
/// Backoff: multiplicative decrease when the windowed min RTT exceeds the
/// hint-coupled queue target.
pub(crate) const BACKOFF_MULT: f64 = 0.92;
/// SRTT assumed before the first RTT sample arrives (update cadence only).
pub(crate) const DEFAULT_SRTT: Duration = Duration::from_millis(50);

// --- BtlBw-anchored recovery (paper Section 12.6) ---
//
// The additive +2/SRTT recovery after a delay backoff crawls: from a
// ×0.92 trough it takes dozens of SRTTs to re-fill the pipe, so cwnd sits
// well below BDP (measured L1 C2: p50 ~80-110 symbols vs BDP ~160). We
// already maintain a delivery-rate max-filter (`max_bw`) and a 10s min-RTT
// (`min_rtt`); their product is a BtlBw×RTprop = BDP estimate. Use it to
// (a) pull post-backoff recovery TOWARD BDP proportionally (decaying to
// the gentle +2 probe as cwnd → BDP) and (b) floor cwnd at the estimate so
// a backoff (or a jitter false-positive) cannot crawl cwnd below the pipe.
//
// CRITICAL: `max_bw` is a windowed MAX of COARSE ACK-batch delivery rates
// — no per-packet sampling and no app-limited detection (BBR discards
// app-limited samples precisely because they underestimate BtlBw). For a
// warm-up-limited transfer (the dominant 1.8MB regime) the estimate reads
// LOW exactly when we would want it high. So the anchor is used ONLY to
// RAISE cwnd — a recovery target and a floor, never a cap. A stale/under-
// estimated BtlBw can then only fail to help; it can never suppress cwnd.

/// Minimum delivery-rate samples in the 10s window before the BtlBw anchor
/// is trusted. A handful of coarse samples is too noisy to floor cwnd on.
///
/// PUBLIC because it is now LOAD-BEARING OUTSIDE the scheduler: the store-cap
/// bootstrap floor is DERIVED from it (`net::sender_policy::STORE_CAP_FLOOR`,
/// paper §16.59) rather than being the bare `64` ADR-0070 finding 5 recorded
/// as PROVENANCE ABSENT. The floor's job is to keep enough outstanding that
/// this gate can close; the number of samples the gate wants is this constant,
/// and the derivation cites it instead of restating it.
pub const ANCHOR_MIN_SAMPLES: usize = 8;
/// cwnd_gain on the BtlBw×RTprop BDP estimate for the post-backoff recovery
/// TARGET. 1.0 = aim to re-fill exactly the pipe; the gentle +2 probe (and
/// the hint-coupled queue target) still governs the standing queue ABOVE
/// BDP, so this is not BBR's cwnd_gain=2 (which deliberately buffers 1×BDP).
pub(crate) const ANCHOR_RECOVERY_GAIN: f64 = 1.0;
/// Proportional pull toward the recovery target per SRTT update: the
/// increment is max(ADDITIVE_STEP, α·(target − cwnd)). Continuous and
/// self-decaying — at α=0.25 a trough at 0.5×BDP closes ~90% of the gap in
/// ~8 SRTTs (vs ~40 SRTTs for +2), and the term vanishes into +2 as
/// cwnd → target (no discrete phase, no cliff).
pub(crate) const ANCHOR_PULL_ALPHA: f64 = 0.25;
/// cwnd floor as a multiple of the BtlBw×RTprop estimate. cwnd is never
/// driven below this once the anchor is established (floor, NOT cap).
///
/// 0.85, not 1.0: a floor AT the full BDP estimate pins cwnd there even
/// when the delay signal reports queue-above-target — the L1 C2 cwnd trace
/// showed `above=true` on nearly every update with cwnd held exactly at
/// bdp_anchor, i.e. the floor was maintaining a ~16 ms standing queue the
/// backoff could no longer drain. Flooring at 0.85×BDP keeps cwnd off the
/// 8-symbol collapse (the measured deficiency) while leaving the delay
/// backoff ~15% of authority around BDP to drain a genuine queue; the
/// recovery pull (gain 1.0) still re-fills toward full BDP each clean
/// update, so cwnd oscillates just under the pipe rather than sitting in
/// standing bufferbloat. Because `max_bw` also underestimates during
/// warm-up, the realized floor sits further below true BDP — the safety
/// (see the risk note above and Section 12.6).
pub(crate) const ANCHOR_FLOOR_GAIN: f64 = 0.85;

/// Floor on the in_flight expiry horizon (see `PathState::expire_in_flight`).
/// max(4×SRTT, this): stranded budget (lost best-effort ACK datagrams)
/// releases within ~a quarter second instead of jamming the TUN gate until
/// the 2s leak-guard decay.
pub(crate) const IN_FLIGHT_EXPIRY_MIN: Duration = Duration::from_millis(250);

/// Hint-coupled queue-target multiplier (P1, paper Section 12.4): the
/// standing queue is allowed to raise the windowed min RTT to
/// floor × mult before Copa-lite backs off. Realtime keeps the queue
/// near-empty; Bulk trades a deeper queue for utilization.
pub(crate) fn queue_target_mult(hint: ProtocolHint) -> f64 {
    match hint {
        ProtocolHint::Realtime => 1.08,
        ProtocolHint::Auto => 1.125,
        ProtocolHint::Bulk => 1.25,
    }
}

/// Sliding window entry for bandwidth/RTT tracking.
#[derive(Clone, Debug)]
pub(crate) struct BwSample {
    /// Delivery rate in symbols per second.
    pub(crate) delivery_rate: f64,
    /// Timestamp when this sample was taken.
    timestamp: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct RttSample {
    rtt: Duration,
    timestamp: Instant,
}

/// Cap on rate-sample send records tracked per path (bounds the map when
/// symbols are lost / attributed without a matching send record). ~a few
/// aggregate BDPs; oldest are dropped past this.
pub(crate) const RS_MAX_TRACKED: usize = 8192;

/// A sent SOURCE symbol's BBR delivery-rate-sample state
/// (draft-cheng-iccrg-delivery-rate-estimation), snapshotted at send time and
/// consumed when the symbol is acked to produce ONE rate sample whose Δt is the
/// SEND interval — robust to ack-aggregation and a standing queue.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RsPacket {
    /// `C.delivered` (this path's delivered counter) at the moment of send.
    delivered: u64,
    /// `C.delivered_time` (time `delivered` last advanced) at send.
    delivered_time: Instant,
    /// `C.first_sent_time` (start of the current in-flight send burst) at send.
    first_sent_time: Instant,
    /// When this symbol was sent.
    sent_time: Instant,
    /// The sender was app-limited (starved, not cwnd/pace-limited) at send —
    /// the sample may only RAISE the max-filter, never be read as bw dropping.
    app_limited: bool,
}

/// Copa-lite delay-based congestion control state.
///
/// Copa (Arun & Balakrishnan, NSDI 2018), simplified to the semantics that
/// won the L0 goal gate (tests/gate_suite.rs run_fec driver, P1+P2):
///
///   - Propagation floor: min RTT sample over a sliding ~10s window (P2's
///     estimated floor; windowed rather than lifetime so a route change
///     re-learns within one window).
///   - Queuing-delay signal: min RTT sample since the LAST cwnd update.
///   - Two-speed ramp: ×1.5+1 per update until first backoff, then +2/×0.92.
///   - Backoff when the windowed min exceeds the hint-coupled queue target
///     floor × queue_mult (P1).
///
/// Key properties:
///   - No phases (no Startup/ProbeBw/ProbeRtt state machine)
///   - Natural rate oscillation drains queues without explicit probe phase
///   - Compatible with taper function (no FEC protection gaps)
///   - Delay-based: loss + stable RTT = channel loss (ignore)
///
/// See paper Section 12 (Congestion Control Integration).
#[derive(Debug)]
pub struct CopaState {
    /// Sliding window of bandwidth samples (symbols/sec).
    pub(crate) bw_samples: VecDeque<BwSample>,
    /// goal-gate "Honest Inputs" (`RWM_HONEST_ANCHOR`): monotonic
    /// (non-increasing) MAX-deque maintained beside `bw_samples` — the exact
    /// mirror of the `rtt_samples` min-deque, on the max statistic. Fed and
    /// evicted in lockstep with `bw_samples` (`bw_push_sample` /
    /// `bw_evict_before`), so its front is ALWAYS the full-window fold's
    /// value (unit-pinned). `max_bw` reads it only under `bw_o1`; the deque
    /// itself is maintained unconditionally (O(1) amortized, ≤ the size of
    /// `bw_samples`) so the equality is testable without env plumbing.
    pub(crate) bw_mono: VecDeque<BwSample>,
    /// `RWM_HONEST_ANCHOR` resolved at construction: `max_bw` = the mono
    /// deque's front (O(1)) instead of the per-sample full-window fold
    /// (O(window) — the measured c1 CPU tax under `RWM_PLAIN_RS`).
    /// Value-identical either way.
    bw_o1: bool,
    /// Sliding window of RTT samples.
    rtt_samples: VecDeque<RttSample>,
    /// How long to keep samples in sliding windows (10s).
    window_duration: Duration,
    /// Minimum RTT in the sliding window = estimated propagation floor.
    pub(crate) min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    pub(crate) max_bw: f64,
    /// Smoothed RTT (EWMA 7/8 old + 1/8 new) — pacing-rate denominator and
    /// cwnd-update cadence.
    pub(crate) srtt: Option<Duration>,
    /// Minimum RTT sample since the last cwnd update — the queuing-delay
    /// signal (windowed min, NOT an EWMA; see module docs).
    min_rtt_since_update: Option<Duration>,
    /// Consecutive-difference jitter estimate (seconds): EWMA of
    /// |rtt_i − rtt_{i−1}| at gain 1/8 (RFC 3550-style). Shift-robust by
    /// construction — a standing queue shifts ALL samples and leaves the
    /// consecutive differences at jitter scale, so this measures jitter,
    /// never queue. Widens the backoff threshold (JITTER_HEADROOM).
    pub(crate) jitter_est: f64,
    /// EWMA of the SQUARED deviation from the smoothed RTT (seconds²) —
    /// `var = (1−β)·var + β·(rtt − srtt)²` at RFC 6298 §2's own smoothing
    /// gain `β = 1/4`, the gain that RFC uses for exactly this job on the
    /// FIRST absolute moment (`RTTVAR = (1−β)·RTTVAR + β·|SRTT − R'|`).
    ///
    /// Exists for paper §16.69's DERIVED recovery clock, which needs a genuine
    /// SECOND moment: Cantelli's distribution-free bound is stated in σ, and
    /// the tree's two existing dispersion signals are both mean-ABSOLUTE
    /// statistics (`jitter_est` on consecutive differences, the estimator's
    /// RFC 3550 interarrival jitter). Converting either to σ requires assuming
    /// a distribution, which would turn the derived clock's one
    /// distribution-free guarantee into a fitted coefficient — §16.69.
    ///
    /// Observation only: read by the `[DIAG] sig_us=` and `[QCLK]` gauges.
    pub(crate) rtt_var_sq: f64,
    /// How many samples have been folded into `rtt_var_sq` — the EWMA's own
    /// warm-up denominator, and the honest half of the `sig_us=` gauge.
    ///
    /// **This exists because "is σ valid yet?" has a different answer here
    /// than everywhere else in the engine.** `ANCHOR_MIN_SAMPLES` = 8 gates
    /// the DELIVERED-RATE anchor (`bw_samples`); it has nothing to do with
    /// this statistic, which is fed from the RTT sample stream and is
    /// available from the first sample that has an `srtt` to deviate from. But
    /// it is not TRUSTWORTHY from the first sample: the EWMA is seeded at 0
    /// and runs at RFC 6298's β = 1/4, so it carries ≈ (1−β)^n = 0.75^n of its
    /// seed after n samples — 24 % at n = 5, 10 % at n = 8, 1 % at n = 16.
    /// A σ read at n = 2 is biased LOW by roughly half, and a gauge that
    /// reported it as a bare number would hand an L1 parser a warm-up artefact
    /// wearing a measurement's clothes.
    ///
    /// So the count is reported ALONGSIDE σ rather than used to gate it (`n`
    /// in `sig_us=<µs>/n<count>`). Emitting only-when-valid would require a
    /// threshold on n, and a threshold that selects whether a gauge exists is
    /// the same defect as a threshold that selects a law: the reader could not
    /// tell "σ suppressed" from "path never sampled". The parser gets the
    /// number and the evidence about it, and decides.
    pub(crate) rtt_var_n: u64,
    /// **CANDIDATE 2 of 3 — the `rvar_us=` gauge.** RFC 6298 §2's `RTTVAR`,
    /// the MEAN-DEVIATION EWMA: `rvar ← (1−β)·rvar + β·|rtt − srtt|` at the
    /// RFC's own β = 1/4 (`SIGMA_CAND_RVAR_GAIN`).
    ///
    /// **It is the shipped `rtt_var_sq` with the SQUARE removed and nothing
    /// else changed** — same feed site, same β, same lagging `srtt` reference,
    /// same 7-sample memory. That is the entire point of building it: it is
    /// not a competitor, it is the CONTROLLED COMPARISON that isolates one of
    /// the three candidate causes of the 287×. If `rvar`'s dispersion lands
    /// near `√(dispersion of sig_us)`, the culprit is OUTLIER LEVERAGE — the
    /// square, which admits a single excursion as its square — and the memory
    /// is innocent. If `rvar`'s dispersion stays near `sig_us`'s, the square is
    /// innocent and the memory or the reference is the culprit. **Neither
    /// candidate alone can tell those apart; the pair can.**
    ///
    /// Provenance: CITED (RFC 6298 §2; RFC 8985 §6.2 inherits it for RACK).
    /// Observation only: read by nothing but `[DIAG]`.
    pub(crate) rtt_mdev: f64,
    /// Samples folded into [`CopaState::rtt_mdev`] — its warm-up denominator,
    /// on the line beside it. EWMA-class, so the pre-registered `n_warm` is 16
    /// (`0.75^16` = 1.00 % seed retention), the same as `rtt_var_n`'s.
    pub(crate) rtt_mdev_n: u64,
    /// **THE RAW RTT SERIES, last `SIGMA_CAND_WINDOW` samples, µs, FIFO.**
    /// Feeds candidates 1 (`qsp_us`) and 3 (`msd_us`).
    ///
    /// **This cannot reuse `rtt_samples`.** That deque is MONOTONIC — it pops
    /// from the back on every sample that is not larger than the incoming one,
    /// because it exists to serve a windowed MINIMUM in O(1). It therefore does
    /// not hold the series; it holds a lower envelope of it, and a dispersion
    /// statistic taken over an envelope would read the drift and not the
    /// spread. This is a plain FIFO and holds every sample in arrival order,
    /// which is also what makes the SUCCESSIVE differences of candidate 3
    /// meaningful.
    pub(crate) rtt_win: VecDeque<u32>,
    /// **THE TIMESTAMPED, TIME-DECIMATED RTT SERIES for candidate 4
    /// (`tlag_us=`, paper §16.75)**: `(arrival instant, rtt µs)`, ascending in
    /// time, at most `SIGMA_CAND_WINDOW` entries.
    ///
    /// **This cannot reuse `rtt_win`.** That FIFO carries no timestamps, so a
    /// lag can only be counted in SAMPLES there — which is precisely the defect
    /// §16.75 exists to remove: a lag-1 successive difference estimates the
    /// structure function `V` at whatever the inter-sample spacing happens to
    /// be, and that spacing is set by the ack rate rather than by the link.
    ///
    /// **And it cannot reuse `rtt_samples` either**, for the same reason
    /// `rtt_win` cannot: that deque is a MONOTONIC min-deque and holds a lower
    /// envelope of the series, not the series.
    ///
    /// Entries are admitted at most one per `RTprop / SIGMA_TLAG_DECIM_M`, so
    /// the ring spans `32 · RTprop` at every sample rate (see the const doc).
    /// Fed unconditionally; read by nothing but `[DIAG]`.
    pub(crate) rtt_tlag: VecDeque<(Instant, u32)>,
    /// Previous raw RTT sample (for the consecutive difference).
    prev_rtt_sample: Option<Duration>,
    /// Per-update window-min history over the sliding window: the queue
    /// floor is a low quantile of these (QUEUE_FLOOR_QUANTILE) — the same
    /// statistic as the queue signal itself, so jitter cannot open a
    /// permanent gap between signal and floor (see const docs).
    win_min_history: VecDeque<(Instant, Duration)>,
    /// RTT samples recorded since the last cwnd update — evidence count
    /// for the ramp fast-exit (a min over ≥3 samples; a min-of-1 is just
    /// one jittery sample and fired false ramp exits at L1's C2).
    pub(crate) samples_since_update: u32,
    /// Window-level jitter estimate (seconds): EWMA (gain 1/4) of
    /// |win_min_i − win_min_{i−1}| between consecutive cwnd updates.
    /// Under correlated jitter the raw-sample consecutive differences
    /// collapse (~0.85ms at C2) while the window min wanders 3-5ms per
    /// update; this estimator sees that amplitude and stays shift-robust
    /// (a standing queue is ONE transition sample).
    pub(crate) win_jitter_est: f64,
    /// Previous update's window min (for the window-level difference).
    prev_win_min: Option<Duration>,
    /// True until the first congestion backoff (multiplicative ramp phase).
    pub(crate) ramping: bool,
    /// Hint-coupled queue-target multiplier (P1): 1.08/1.125/1.25.
    pub(crate) queue_mult: f64,
    /// When the cwnd was last updated (updates run once per SRTT).
    last_cwnd_update: Instant,
    /// Delivered symbols counter for delivery rate calculation.
    delivered: u64,
    /// Timestamp of last delivery measurement.
    last_delivered_time: Instant,
    /// Delivered count at last measurement.
    last_delivered: u64,
    // --- BBR delivery-rate sampling (the send-interval anchor, ADR-0061) ---
    /// Total SOURCE symbols delivered on this path (BBR `C.delivered`). Separate
    /// from `delivered` so the legacy ack-interval anchor stays byte-exact.
    pub(crate) rs_delivered: u64,
    /// Time `rs_delivered` last advanced (BBR `C.delivered_time`).
    rs_delivered_time: Instant,
    /// Send time of the packet that started the current in-flight send burst
    /// (BBR `C.first_sent_time`); advances to each acked packet's send time.
    rs_first_sent_time: Instant,
    /// Outstanding rate-sample send records (seq → snapshot), consumed on ack.
    rs_sent: BTreeMap<u64, RsPacket>,
    // --- DIAG counters (diag/slow-path-anchor) -------------------------------
    // Pure observation — these NEVER affect a control decision (read only at the
    // RWM_DIAG print).  They trace WHY the per-path BtlBw anchor does or does not
    // warm: how many source seqs were snapshotted at send, how many acks were
    // attributed here, and how each attributed ack was classified by the BBR
    // GenerateRateSample guards (interval<MinRTT / zero-delivered / app-limited)
    // vs accepted into the windowed-max filter.
    rs_sent_count: u64,
    rs_applimited_sent: u64,
    rs_attributions: u64,
    rs_no_record: u64,
    rs_rej_interval: u64,
    rs_rej_zero: u64,
    rs_rej_applimited: u64,
    rs_generated: u64,
    /// RWM_RS_TRACE: eprintln each ACCEPTED rate sample above the given
    /// symbols/s threshold with its (delivered, interval, send_elapsed,
    /// ack_elapsed) decomposition — the over-read forensics instrument
    /// (feat/copa-sole-cc). 0 = off (default, no cost on the sample path).
    rs_trace_thresh: f64,
    /// DIAG label for the RSTRACE prints: the owning path's id (u32::MAX
    /// until the owner stamps it). Never read by any control decision.
    pub(crate) rs_trace_path: u32,
    // --- Wire-clocked δ-mapped update law (feat/copa-wire-signal) -----------
    /// Wire mode: the delay term is the packet-timed wire RTT (fed by the
    /// transport seam) and the cwnd update law is Copa's actual
    /// target-rate/velocity dynamics around rate = 1/(δ·d_q). False (default,
    /// env unset) ⇒ every legacy path byte-identical.
    wire_mode: bool,
    /// Copa δ (1/symbols): the hint-mapped latency price (`copa_delta`).
    /// In legacy mode this stays COPA_DELTA so the diagnostic
    /// `copa_target_cwnd` is unchanged.
    pub(crate) delta: f64,
    /// Copa velocity v: the per-SRTT step is v/δ symbols; v doubles once
    /// per update while the direction has persisted ≥ 3 consecutive
    /// updates, and resets to 1 on a direction flip (Copa §2.2's velocity
    /// parameter at per-SRTT granularity — the 3-window hysteresis is what
    /// bounds the overshoot of a pure every-window doubling, MEASURED at
    /// the L1 smoke: cwnd pinned MAX_CWND with 130 ms app-RTT spikes
    /// without it).
    velocity: f64,
    /// Consecutive same-direction update count (velocity hysteresis).
    dir_streak: u32,
    /// Direction of the previous wire-mode update (None = no update yet /
    /// after a backoff reset).
    last_dir_up: Option<bool>,
    // --- Copa §2.2 TCP-competitive mode (feat/copa-compete) -----------------
    /// Mode switching enabled (RWM_COPA_COMPETE && wire mode). False
    /// (default) ⇒ every field below is inert and the law byte-identical.
    pub(crate) compete_on: bool,
    /// The default-mode δ: the hint-mapped base price (`copa_delta`).
    /// `delta` diverges from it only while in competitive mode.
    pub(crate) delta_base: f64,
    /// Currently in competitive mode.
    pub(crate) in_compete: bool,
    /// DIAG: competitive-mode entries (mechanism liveness counter).
    pub(crate) compete_switches: u64,
    /// Last instant a "nearly empty" queue was observed
    /// (d_q < 0.1·(RTTmax−RTTmin), Copa §2.2). None = no sample yet
    /// (treated as recently-empty: default mode, false-positive safe).
    last_nearly_empty: Option<Instant>,
    /// Monotonic (non-increasing) deque of wire RTT samples over the past
    /// ~4 RTTs — front = RTTmax for the nearly-empty calibration. Mirror of
    /// the `rtt_samples` min-deque; O(1) amortized.
    compete_max_deque: VecDeque<(Instant, Duration)>,
    /// A wire-level loss event (quinn congestion event) was recorded since
    /// the last per-SRTT update — the competitive AIMD's MD trigger.
    loss_since_update: bool,
    /// Cumulative congestion-event counter last seen from the pass-through
    /// shim (diffed, not reset — the shim counter is monotone).
    last_cong_events: u64,
    // --- Raw-sample echo-ratio floor (goal-gate "Honest Inputs") ------------
    /// `RWM_HONEST_K` resolved at construction: feed the K tracker below the
    /// RAW per-sample ratio at the sample clock. False (default) ⇒ the
    /// tracker is never fed and `k_raw_ratio()` is `None` — every K consumer
    /// keeps the legacy smoothed-at-refresh feed byte-identically.
    k_raw_on: bool,
    /// The path's raw-fed windowed-min echo-ratio (`EchoRatioMin`, the SAME
    /// window/clamp/guard as the net-side refresh-clock trackers): fed
    /// rtt_raw/RTprop per sample in `record_rtt` under `k_raw_on`.
    k_raw: crate::net::EchoRatioMin,
    /// Monotonic µs epoch for `k_raw`'s window arithmetic.
    k_raw_epoch: Instant,
    /// Injectable clock for time queries.
    clock: Arc<dyn Clock>,
}

impl CopaState {
    pub(crate) fn new(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let wire_mode = copa_wire_active();
        let delta = if wire_mode {
            copa_delta_for_hint(hint)
        } else {
            COPA_DELTA
        };
        let now = clock.now();
        Self {
            wire_mode,
            delta,
            velocity: 1.0,
            dir_streak: 0,
            last_dir_up: None,
            compete_on: wire_mode && copa_compete_active(),
            delta_base: delta,
            in_compete: false,
            compete_switches: 0,
            last_nearly_empty: None,
            compete_max_deque: VecDeque::new(),
            loss_since_update: false,
            last_cong_events: 0,
            k_raw_on: honest_k_active(),
            k_raw: crate::net::EchoRatioMin::new(crate::net::PERCAP_K_HALF_WINDOW_US),
            k_raw_epoch: now,
            bw_samples: VecDeque::new(),
            bw_mono: VecDeque::new(),
            bw_o1: honest_anchor_active(),
            rtt_var_sq: 0.0,
            rtt_var_n: 0,
            rtt_mdev: 0.0,
            rtt_mdev_n: 0,
            rtt_win: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
            rtt_tlag: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            srtt: None,
            min_rtt_since_update: None,
            jitter_est: 0.0,
            prev_rtt_sample: None,
            win_min_history: VecDeque::new(),
            samples_since_update: 0,
            win_jitter_est: 0.0,
            prev_win_min: None,
            ramping: true,
            queue_mult: queue_target_mult(hint),
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
            rs_delivered: 0,
            rs_delivered_time: now,
            rs_first_sent_time: now,
            rs_sent: BTreeMap::new(),
            rs_sent_count: 0,
            rs_applimited_sent: 0,
            rs_attributions: 0,
            rs_no_record: 0,
            rs_rej_interval: 0,
            rs_rej_zero: 0,
            rs_rej_applimited: 0,
            rs_generated: 0,
            rs_trace_thresh: std::env::var("RWM_RS_TRACE")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0),
            rs_trace_path: u32::MAX,
            last_cwnd_update: now,
            clock,
        }
    }

    /// Admit one bandwidth sample into the sliding window: `bw_samples` gets
    /// it verbatim (its LENGTH is the anchor-establishment gate —
    /// `ANCHOR_MIN_SAMPLES` — and must not change meaning), and the
    /// monotonic max-deque `bw_mono` gets it with dominated-candidate
    /// eviction (goal-gate "Honest Inputs"): a sample that is older AND no
    /// larger than the new one can never again be the windowed max — any
    /// front-eviction cutoff that spares it spares the newer, larger sample
    /// too — so after eviction the mono deque is strictly decreasing
    /// front→back with increasing timestamps, and its FRONT equals the
    /// full-window fold over `bw_samples` at all times (unit-pinned by
    /// `bw_mono_front_equals_full_window_fold`).
    fn bw_push_sample(&mut self, now: Instant, rate: f64) {
        self.bw_samples.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
        while self
            .bw_mono
            .back()
            .is_some_and(|s| s.delivery_rate <= rate)
        {
            self.bw_mono.pop_back();
        }
        self.bw_mono.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
    }

    /// Evict bandwidth samples older than `cutoff` from BOTH windows (the
    /// two structures see identical push and eviction sequences — that
    /// lockstep is what makes front == fold an invariant rather than a
    /// coincidence). Called with the legacy 10 s cutoff from
    /// `expire_old_samples` and with the ≈10·RTprop [1 s, 10 s] cutoff from
    /// `rs_on_delivered`, exactly where `bw_samples` was already evicted.
    fn bw_evict_before(&mut self, cutoff: Instant) {
        while self.bw_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_samples.pop_front();
        }
        while self.bw_mono.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_mono.pop_front();
        }
    }

    /// Recompute `max_bw` after a push/evict. `RWM_HONEST_ANCHOR` selects the
    /// COST of the same value: the mono deque's front (O(1) amortized) or
    /// the legacy full-window fold (O(window) per accepted sample — the
    /// measured c1 sender-CPU tax once `RWM_PLAIN_RS` feeds this per
    /// delivered symbol instead of per ack).
    fn bw_refresh_max(&mut self) {
        self.max_bw = if self.bw_o1 {
            self.bw_mono.front().map_or(0.0, |s| s.delivery_rate)
        } else {
            self.bw_samples
                .iter()
                .map(|s| s.delivery_rate)
                .fold(0.0f64, f64::max)
        };
    }

    /// The legacy full-window fold, unconditionally — the equivalence
    /// oracle for `bw_mono` (test-only).
    #[cfg(test)]
    pub(crate) fn bw_fold(&self) -> f64 {
        self.bw_samples
            .iter()
            .map(|s| s.delivery_rate)
            .fold(0.0f64, f64::max)
    }

    /// Record delivery of `count` symbols.  Returns the computed delivery rate.
    pub(crate) fn record_delivery(&mut self, count: u32) -> f64 {
        self.delivered += count as u64;
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_delivered_time).as_secs_f64();

        // Need at least 1ms of elapsed time to compute a meaningful rate
        if elapsed < 0.001 {
            // ACK-CADENCE GAUGE (`RWM_ACKDIAG`, net/ackdiag.rs — readout 3):
            // a REJECTED sample. It carries no rate (the filter is untouched)
            // but its `count` did arrive, so it feeds the over-read
            // denominator: the normalizer must see every delivered symbol the
            // sampler saw, or x is inflated by the rejection rate.
            if let Some(g) = crate::net::ackdiag::gauge() {
                g.note_rate_sample(self.rs_trace_path, count, 0.0, false);
            }
            return self.max_bw;
        }

        let delta_delivered = self.delivered - self.last_delivered;
        let rate = delta_delivered as f64 / elapsed;

        self.last_delivered_time = now;
        self.last_delivered = self.delivered;

        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            eprintln!(
                "[RSTRACE-LEGACY] path={} rate={:.0} delta={} elapsed_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                rate,
                delta_delivered,
                elapsed * 1e3,
                self.max_bw,
            );
        }
        // ACK-CADENCE GAUGE (`RWM_ACKDIAG`, net/ackdiag.rs — readout 3): an
        // ACCEPTED ack-interval sample, i.e. exactly one of the values the
        // windowed max below folds. This is THE statistic matrix row 10 calls
        // "UNVERIFIED — and it is the one that is ALWAYS ON"; the gauge
        // normalizes it at print time by the window's own long-run delivered
        // rate to give the realized over-read x directly.
        if let Some(g) = crate::net::ackdiag::gauge() {
            g.note_rate_sample(self.rs_trace_path, count, rate, true);
        }
        // Add to sliding window
        self.bw_push_sample(now, rate);
        self.expire_old_samples(now);

        // Update max bandwidth
        self.bw_refresh_max();

        rate
    }

    // --- BBR delivery-rate sampling (feat/btlbw-rate-sample) ------------------
    //
    // The legacy `record_delivery` above computes rate = Δdelivered / Δt where
    // Δt is the ACK-ARRIVAL interval (`now − last_delivered_time`).  Under DAPS,
    // acks arrive BATCHED (ack-aggregation): a batch collapses Δt toward zero, so
    // Δdelivered/Δt spikes and the windowed-MAX locks onto the spike — the ~145×
    // over-read L1 DIAG measured (fast bdp 14509 / RTprop 12 ms ⇒ ≈1.2M sym/s vs
    // true ≈8.3k).  A rate anchor 145× too high makes EVERY per-path pace bucket
    // / BDP cap inert (the bucket never binds; outstanding bloats to the deep
    // read-ahead — see temporal_oracle PART 6g).
    //
    // The fix (Cardwell/Cheng, draft-cheng-iccrg-delivery-rate-estimation):
    // sample Δt over the SEND interval — max(send_elapsed, ack_elapsed) — so a
    // batched ack (tiny ack_elapsed) is overridden by the true send spacing and
    // the sample is a correct delivery-rate LOWER BOUND.  The max-filter then
    // maxes over CORRECT samples and converges to the true BtlBw (×1).

    /// BBR `SendPacket`: snapshot the rate-sample state for a sent SOURCE symbol.
    pub(crate) fn rs_on_sent(&mut self, seq: u64, app_limited: bool) {
        self.rs_sent_count += 1; // DIAG
        if app_limited {
            self.rs_applimited_sent += 1; // DIAG
        }
        let now = self.clock.now();
        // No packets in flight → (re)start the send burst window.
        if self.rs_sent.is_empty() {
            self.rs_first_sent_time = now;
            self.rs_delivered_time = now;
        }
        self.rs_sent.insert(
            seq,
            RsPacket {
                delivered: self.rs_delivered,
                delivered_time: self.rs_delivered_time,
                first_sent_time: self.rs_first_sent_time,
                sent_time: now,
                app_limited,
            },
        );
        // Bound the map: drop the oldest snapshots for symbols that were lost or
        // attributed cumulatively without ever matching a send record.
        while self.rs_sent.len() > RS_MAX_TRACKED {
            if let Some(&k) = self.rs_sent.keys().next() {
                self.rs_sent.remove(&k);
            } else {
                break;
            }
        }
    }

    /// BBR `UpdateRateSample` + `GenerateRateSample` for ONE acked SOURCE symbol.
    /// Feeds the windowed-max delivery-rate filter (`max_bw`) a sample whose Δt is
    /// the SEND interval, so batched acks / a standing queue cannot inflate it.
    pub(crate) fn rs_on_delivered(&mut self, seq: u64) {
        self.rs_attributions += 1; // DIAG
        let now = self.clock.now();
        let Some(p) = self.rs_sent.remove(&seq) else {
            // No send record (attributed cumulatively past a dropped record):
            // still advance the delivered cursor so later samples stay correct.
            self.rs_no_record += 1; // DIAG
            self.rs_delivered += 1;
            self.rs_delivered_time = now;
            return;
        };
        // Advance the connection delivered cursor (BBR: C.delivered += len).
        self.rs_delivered += 1;
        self.rs_delivered_time = now;
        // send_elapsed spans the send spacing of the packets from the burst start
        // to this packet; ack_elapsed spans the same deliveries in wall time.
        let send_elapsed = p.sent_time.saturating_duration_since(p.first_sent_time);
        let ack_elapsed = now.saturating_duration_since(p.delivered_time);
        // Advance the burst-window start (BBR: C.first_sent_time = P.sent_time).
        self.rs_first_sent_time = p.sent_time;
        // max() is what makes the sample ack-aggregation robust: a batched ack
        // shrinks ack_elapsed, but send_elapsed preserves the true spacing.
        let interval = send_elapsed.max(ack_elapsed).as_secs_f64();
        let delivered = self.rs_delivered.saturating_sub(p.delivered);
        // Reject samples spanning less than one RTprop (BBR GenerateRateSample:
        // `if interval < MinRTT: return`).  An interval below the propagation RTT
        // cannot reliably estimate the bottleneck rate: it is the classic
        // ack-aggregation / send-burst artefact (a batch of queued symbols acked
        // together over a tiny window), which otherwise reads many× the true link
        // (the DAPS slow-path over-read).  Requiring interval ≥ RTprop forces the
        // sample to average over ≥ one pipe, so a drain burst reads the true
        // bottleneck.  Falls back to a 1 ms absolute floor before an RTprop sample.
        let min_interval = self
            .min_rtt
            .map(|r| r.as_secs_f64())
            .unwrap_or(0.001)
            .max(0.001);
        // DIAG: classify the rejection (split from the combined guard below for
        // per-cause counting; behaviour identical — same early return).
        if interval < min_interval {
            self.rs_rej_interval += 1; // DIAG
            return;
        }
        if delivered == 0 {
            self.rs_rej_zero += 1; // DIAG
            return;
        }
        let rate = delivered as f64 / interval;
        // App-limited samples underestimate bw (the pipe was starved, not full),
        // so they may only RAISE the max-filter, never be read as bw dropping.
        // In a pure windowed-max a low app-limited sample is simply not the max;
        // admit one only when it exceeds the current max (BBR §app-limited).
        if p.app_limited && rate <= self.max_bw {
            self.rs_rej_applimited += 1; // DIAG
            return;
        }
        self.rs_generated += 1; // DIAG
        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            eprintln!(
                "[RSTRACE] path={} seq={} rate={:.0} delivered={} interval_ms={:.2} send_ms={:.2} ack_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                seq,
                rate,
                delivered,
                interval * 1e3,
                send_elapsed.as_secs_f64() * 1e3,
                ack_elapsed.as_secs_f64() * 1e3,
                self.max_bw,
            );
        }
        self.bw_push_sample(now, rate);
        // Max-filter window ≈ 10·RTprop (BBR's BtlBw filter), clamped to
        // [1s, 10s]: long enough to hold the true BtlBw between acks, short
        // enough that a genuine rate change is not pinned for the full 10s
        // sample window.  Falls back to 10s before a min-RTT sample exists.
        let win = self
            .min_rtt
            .map(|r| (r.as_secs_f64() * 10.0).clamp(1.0, 10.0))
            .unwrap_or(10.0);
        let cutoff = now
            .checked_sub(Duration::from_secs_f64(win))
            .unwrap_or(now);
        self.bw_evict_before(cutoff);
        // goal-gate "Honest Inputs": under RWM_PLAIN_RS this runs once per
        // DELIVERED SOURCE SYMBOL, so the legacy full-window fold here is
        // O(window·rate) per second — the measured c1 sender-CPU tax
        // (+61…64% CPU/byte, latlever CPU gauge). RWM_HONEST_ANCHOR reads
        // the same value off the mono deque in O(1).
        self.bw_refresh_max();
    }

    /// Record an RTT sample: SRTT EWMA, 10s floor window, and the
    /// since-last-update min (queuing-delay signal).
    pub(crate) fn record_rtt(&mut self, rtt: Duration) {
        let now = self.clock.now();

        // SRTT EWMA (RFC 6298 weights, same as the gate driver).
        self.srtt = Some(match self.srtt {
            Some(s) => s.mul_f64(0.875) + rtt.mul_f64(0.125),
            None => rtt,
        });

        // Windowed min for the queuing-delay signal.
        self.min_rtt_since_update = Some(match self.min_rtt_since_update {
            Some(m) => m.min(rtt),
            None => rtt,
        });

        // Consecutive-difference jitter EWMA (shift-robust; see field doc).
        if let Some(prev) = self.prev_rtt_sample {
            let diff = if rtt > prev { rtt - prev } else { prev - rtt };
            self.jitter_est += (diff.as_secs_f64() - self.jitter_est) * JITTER_GAIN;
        }
        self.prev_rtt_sample = Some(rtt);
        self.samples_since_update += 1;

        // Windowed minimum via a MONOTONIC (non-decreasing) deque — O(1)
        // amortised instead of the former O(n) rescan of the entire 10 s
        // sample history on every sample. At L1 (thousands of ACK-driven RTT
        // samples/s over a 10 s window ⇒ ~20k-element deque) that rescan was
        // the single largest sender CPU cost (~42% self, a hidden O(n²) over a
        // transfer — MEASURED by perf). A new sample evicts every pending
        // candidate whose RTT is >= its own: those can never be the window
        // minimum while this newer, smaller-or-equal sample is in the window
        // (and it expires strictly later), so after eviction the deque stays
        // non-decreasing front→back with strictly increasing timestamps. The
        // front is therefore always the current windowed min, and time-based
        // expiry still pops from the (oldest-timestamp) front. Exact same
        // `min_rtt` value as the rescan, just maintained incrementally.
        // §16.69's second moment, fed against the SMOOTHED mean the same way
        // RFC 6298 §2 feeds RTTVAR and at the same β = 1/4. Fed
        // unconditionally; read by nothing on the default arm.
        if let Some(sr) = self.srtt {
            let dev = rtt.as_secs_f64() - sr.as_secs_f64();
            self.rtt_var_sq = 0.75 * self.rtt_var_sq + 0.25 * dev * dev;
            // Counted at the SAME site that feeds the EWMA, so the `[DIAG]`
            // `sig_us=<µs>/n<count>` gauge's denominator can never describe a
            // different sample set than its numerator.
            self.rtt_var_n += 1;
            // CANDIDATE 2 (`rvar_us=`): RFC 6298 §2's mean deviation, fed at
            // the SAME site, from the SAME `dev`, at the SAME β. Identical in
            // every respect to the line above except that the deviation enters
            // LINEARLY instead of squared — which is what makes the pair a
            // decomposition rather than two guesses. Read by nothing.
            self.rtt_mdev += (dev.abs() - self.rtt_mdev) * SIGMA_CAND_RVAR_GAIN;
            self.rtt_mdev_n += 1;
        }
        // CANDIDATES 1 and 3 (`qsp_us=`, `msd_us=`): the raw series, FIFO,
        // last `SIGMA_CAND_WINDOW`. Fed unconditionally and WITHOUT an `srtt`
        // precondition — unlike the two EWMAs above, neither of these gauges
        // takes a deviation against a reference, so neither has to wait for
        // one. O(1): one push, at most one pop. Read by nothing.
        if self.rtt_win.len() == SIGMA_CAND_WINDOW {
            self.rtt_win.pop_front();
        }
        self.rtt_win
            .push_back((rtt.as_micros() as u64).min(u32::MAX as u64) as u32);
        while self.rtt_samples.back().is_some_and(|s| s.rtt >= rtt) {
            self.rtt_samples.pop_back();
        }
        self.rtt_samples.push_back(RttSample {
            rtt,
            timestamp: now,
        });
        self.expire_old_samples(now);
        self.min_rtt = self.rtt_samples.front().map(|s| s.rtt);

        // CANDIDATE 4 (`tlag_us=`, paper §16.75): the TIMESTAMPED, TIME-DECIMATED
        // series. Fed AFTER `min_rtt` is refreshed above, so the admission
        // spacing uses the freshest `τ = RTprop` this sample can see.
        //
        // Admission is a SPACING CAP applied identically at every rate — not a
        // branch on the rate, not a threshold that selects a code path. As the
        // arrival spacing rises through `τ/m` the admitted stream passes
        // continuously from decimated to pass-through (§16.75.8, continuity in
        // the sample rate). With `τ` unavailable nothing is admitted, the pair
        // set stays empty and the gauge renders `-`: there is NO fallback
        // constant, because a fallback constant is a free constant with no
        // provenance and a second code path (§16.75.6 F2).
        //
        // O(1): one `duration_since`, at most one push and one pop.
        if let Some(tau) = self.min_rtt {
            let spacing = tau / SIGMA_TLAG_DECIM_M;
            let admit = match self.rtt_tlag.back() {
                Some((t_last, _)) => now.duration_since(*t_last) >= spacing,
                None => true,
            };
            if admit {
                if self.rtt_tlag.len() == SIGMA_CAND_WINDOW {
                    self.rtt_tlag.pop_front();
                }
                self.rtt_tlag
                    .push_back((now, (rtt.as_micros() as u64).min(u32::MAX as u64) as u32));
            }
        }

        // goal-gate "Honest Inputs" (`RWM_HONEST_K`): feed the K tracker the
        // RAW sample's ratio at the SAMPLE clock, against the freshest floor.
        // The windowed MIN then reads the delay distribution's FLOOR — the
        // quantity the honest-cap/three-term derivations assume — instead of
        // the min of the SMOOTHED series, which sits near the distribution's
        // mean and reads ×1.34-class high under ±25 ms jitter (the measured
        // jit25 inversion). Same tracker type, same window, same ≥ 1 clamp,
        // same seed-identity guard (which here also discards the exact
        // floor-setting sample — the min then reads the second-lowest in
        // window, a negligible upward bias under any dense sample stream,
        // and the guard stays shared discipline rather than forking).
        // Gated: OFF ⇒ nothing is fed and `k_raw_ratio()` is None — every
        // consumer keeps the legacy smoothed feed byte-identically.
        if self.k_raw_on {
            let now_us = now.duration_since(self.k_raw_epoch).as_micros() as u64;
            self.k_raw
                .observe_srtt_over_rtprop(rtt, self.min_rtt, now_us);
        }

        // Copa §2.2 competitive-mode detector sampling (feat/copa-compete):
        // mark the instants at which the queue is "nearly empty". Gated so
        // the shipped/wire-only paths pay nothing.
        if self.compete_on {
            self.compete_note_sample(rtt, now);
        }
    }

    /// Copa §2.2 nearly-empty detector, per wire RTT sample: maintain RTTmax
    /// over the past ~4 RTTs (monotonic max-deque, the mirror of the min
    /// deque above) and mark `last_nearly_empty` whenever the current
    /// queuing delay d_q = sample − RTTmin(long-term) is below
    /// 0.1·(RTTmax − RTTmin). The RTTmax term calibrates "nearly empty" to
    /// the path's short-term RTT variance (paper §2.2); the DQ_FLOOR guard
    /// keeps a zero-variance clean/idle link (RTTmax == RTTmin ⇒ threshold
    /// 0) from reading as "never empty" — a d_q at the clamp floor IS an
    /// empty queue.
    fn compete_note_sample(&mut self, rtt: Duration, now: Instant) {
        while self.compete_max_deque.back().is_some_and(|&(_, r)| r <= rtt) {
            self.compete_max_deque.pop_back();
        }
        self.compete_max_deque.push_back((now, rtt));
        let lookback = self.srtt().mul_f64(COMPETE_RTTMAX_RTTS);
        let cutoff = now.checked_sub(lookback).unwrap_or(now);
        while self
            .compete_max_deque
            .front()
            .is_some_and(|&(t, _)| t < cutoff)
        {
            self.compete_max_deque.pop_front();
        }
        let Some(floor) = self.min_rtt else { return };
        let rtt_max = self
            .compete_max_deque
            .front()
            .map(|&(_, r)| r)
            .unwrap_or(rtt);
        let dq = (rtt.as_secs_f64() - floor.as_secs_f64()).max(0.0);
        let threshold = COMPETE_EMPTY_FRAC * (rtt_max.as_secs_f64() - floor.as_secs_f64());
        if dq <= threshold.max(DQ_FLOOR_SECS) {
            self.last_nearly_empty = Some(now);
        }
    }

    /// Wire-level loss evidence for the competitive AIMD (feat/copa-compete):
    /// fed the pass-through shim's CUMULATIVE `congestion_events` counter for
    /// this path; any advance since the last read marks a loss into the
    /// current update window. No-op unless competitive switching is enabled.
    pub(crate) fn note_congestion_events(&mut self, cumulative: u64) {
        if !self.compete_on {
            return;
        }
        if cumulative > self.last_cong_events {
            self.loss_since_update = true;
        }
        self.last_cong_events = cumulative;
    }

    /// Copa §2.2 mode switching + the competitive AIMD on 1/δ, evaluated once
    /// per SRTT update (the paper's per-RTT cadence). See the module-level
    /// mechanism note at `copa_compete_active`.
    ///
    ///   - default → competitive: no nearly-empty queue observed in the last
    ///     5 RTTs; enter at δ = δ_base (AIMD grows 1/δ from the base price).
    ///   - competitive AIMD (NewReno-emulating, per the paper's
    ///     implementation): loss in the window ⇒ 1/δ ← max(1/δ_base, 1/(2δ));
    ///     otherwise 1/δ ← 1/δ + 1. Invariant: δ ≤ δ_base (the paper's
    ///     "δ ≤ 0.5" generalized to the hint base), 1/δ bounded so the
    ///     coupling cap's 2/δ term stays ≤ MAX_CWND.
    ///   - competitive → default: a nearly-empty queue within the last
    ///     5 RTTs ⇒ reset δ = δ_base (the paper's reset-to-0.5), velocity
    ///     re-measures from 1.
    ///
    /// Skipped during the ramp: the startup burst's own queue is not
    /// competitor evidence, and the velocity law is not live yet.
    fn compete_update(&mut self, now: Instant) {
        if !self.compete_on || self.ramping {
            return;
        }
        let window = self.srtt().mul_f64(COMPETE_WINDOW_RTTS);
        let empty_recent = match self.last_nearly_empty {
            Some(t) => now.saturating_duration_since(t) <= window,
            // No detector evidence yet (no RTT floor/sample): stay default —
            // the false-positive-safe direction.
            None => true,
        };
        if self.in_compete {
            if empty_recent {
                self.in_compete = false;
                self.delta = self.delta_base;
                self.velocity = 1.0;
                self.dir_streak = 0;
                self.last_dir_up = None;
            } else {
                let inv = 1.0 / self.delta;
                let inv = if self.loss_since_update {
                    (inv * 0.5).max(1.0 / self.delta_base)
                } else {
                    (inv + 1.0).min(COMPETE_INV_DELTA_MAX)
                };
                self.delta = 1.0 / inv;
            }
        } else if !empty_recent {
            self.in_compete = true;
            self.compete_switches += 1;
            self.delta = self.delta_base;
        }
        self.loss_since_update = false;
    }

    /// Smoothed RTT, defaulting to 50ms before the first sample.
    fn srtt(&self) -> Duration {
        self.srtt.unwrap_or(DEFAULT_SRTT)
    }

    /// The queue floor: QUEUE_FLOOR_QUANTILE of the recent window-min
    /// history — the same min-of-N statistic as the queue signal itself
    /// (see const docs; falls back to the propagation floor before any
    /// history accumulates). Never below the propagation floor by
    /// construction (every window min is itself an RTT sample).
    fn queue_floor(&self) -> Option<Duration> {
        if self.win_min_history.is_empty() {
            return self.min_rtt;
        }
        let mut v: Vec<Duration> = self.win_min_history.iter().map(|&(_, d)| d).collect();
        let idx = (((v.len() - 1) as f64) * QUEUE_FLOOR_QUANTILE).round() as usize;
        let (_, nth, _) = v.select_nth_unstable(idx);
        Some(*nth)
    }

    /// Whether the standing-queue signal is above the hint-coupled target:
    /// windowed-min RTT − queue_floor (= dq, clamped ≥ 0.1ms) exceeds
    /// (queue_mult − 1) × queue_floor + k × jitter_est (also clamped
    /// ≥ 0.1ms).
    ///
    /// Equivalent to the gate driver's `min_rtt_win > floor × queue_mult`
    /// except for three continuity guards (all vanish on a clean link,
    /// where queue_floor == floor and jitter_est == 0):
    ///   - the dq clamp keeps sub-millisecond-RTT links from backing off
    ///     on sub-clamp noise (see DQ_FLOOR_SECS),
    ///   - the queue floor is a low quantile of the window-min history
    ///     rather than the extreme-value 10s min, so jitter cannot open a
    ///     permanent gap between signal and floor (QUEUE_FLOOR_QUANTILE —
    ///     measured L1 root cause of the C2 throughput collapse), and
    ///   - the k × jitter_est term covers the residual within-window
    ///     spread at small sample counts (JITTER_HEADROOM).
    /// Wire-mode queuing delay d_q (seconds): STANDING wire RTT (the most
    /// recent packet-timed sample — quinn's srtt, already an EWMA over many
    /// per-ack samples: Copa §2's RTTstanding) − propagation floor − jitter
    /// headroom, clamped ≥ DQ_FLOOR_SECS.
    ///
    /// Differences from the legacy signal, all consequences of the wire
    /// clock (feat/copa-wire-signal):
    ///   - The signal is the CURRENT standing estimate, NOT the per-window
    ///     min. MEASURED (L1 c2 smoke, v1 of this law): the δ-sawtooth's
    ///     drain trough falls inside every update window, so a windowed min
    ///     reads "queue empty" at every update — the direction stays up,
    ///     the velocity compounds, and cwnd pins MAX_CWND with 130 ms
    ///     app-RTT spikes. The smoothed standing sample tracks the queue
    ///     the law is actually steering.
    ///   - The floor is the RAW 10 s min (`min_rtt`), not the quantile
    ///     queue floor: wire samples are already smoothed (sample-level
    ///     jitter averaged out), and the law's dither drains the queue to
    ///     ~empty regularly, refreshing the raw min (the quantile floor was
    ///     an app-echo jitter fix, and under a deep Bulk standing queue it
    ///     would creep up to the queue itself within its 10 s window —
    ///     staleness by construction).
    ///   - The jitter headroom is SUBTRACTED from the measured d_q rather
    ///     than added to a threshold, so one adjusted quantity feeds both
    ///     the above-target test and the target-rate law (continuity:
    ///     jitter → 0 recovers plain Copa exactly).
    fn wire_dq_secs(&self) -> Option<f64> {
        let standing = self.prev_rtt_sample?;
        let floor = self.min_rtt?;
        let jitter = self.jitter_est.max(self.win_jitter_est);
        Some(
            (standing.as_secs_f64() - floor.as_secs_f64() - JITTER_HEADROOM * jitter)
                .max(DQ_FLOOR_SECS),
        )
    }

    /// Wire-mode congestion test: is the current rate above Copa's target
    /// rate 1/(δ·d_q)?  cwnd/srtt > 1/(δ·d_q)  ⇔  cwnd·δ·d_q > srtt.
    fn wire_above_target(&self, cwnd: u32) -> bool {
        match self.wire_dq_secs() {
            Some(dq) => cwnd as f64 * self.delta * dq > self.srtt().as_secs_f64(),
            None => false,
        }
    }

    pub(crate) fn queue_above_target(&self, cwnd: u32) -> bool {
        if self.wire_mode {
            return self.wire_above_target(cwnd);
        }
        let (Some(win_min), Some(floor)) = (self.min_rtt_since_update, self.queue_floor()) else {
            return false;
        };
        let floor_s = floor.as_secs_f64();
        let dq = (win_min.as_secs_f64() - floor_s).max(DQ_FLOOR_SECS);
        // Headroom covers whichever jitter evidence is larger: per-sample
        // (consecutive raw-sample differences) or per-window (consecutive
        // window-min differences) — under correlated jitter (a slow RTT
        // wave) only the window-level estimator sees the true amplitude:
        // measured at L1 C2, raw diffs ~0.85ms while window mins wander
        // ~3-5ms between updates. Both are consecutive-difference EWMAs,
        // hence shift-robust: a standing queue contributes ONE transition
        // sample, not a persistent inflation, so congestion detection
        // survives (unlike a quantile-spread term, which a level shift
        // would inflate for a full window).
        let jitter = self.jitter_est.max(self.win_jitter_est);
        let dq_target = ((self.queue_mult - 1.0) * floor_s + JITTER_HEADROOM * jitter)
            .max(DQ_FLOOR_SECS);
        dq > dq_target
    }

    /// Whether a cwnd window update is due (once per SRTT).
    pub(crate) fn should_update(&self, now: Instant) -> bool {
        now.duration_since(self.last_cwnd_update) >= self.srtt()
    }

    /// Per-SRTT window update (gate driver semantics):
    ///   - windowed min above the queue target → backoff ×0.92, end ramp
    ///   - ramping → ×1.5 + 1
    ///   - steady state → +2
    /// Resets the queuing-delay window. Returns the new cwnd (unclamped
    /// against MIN/MAX — the caller clamps).
    pub(crate) fn update_cwnd(&mut self, cwnd: u32) -> u32 {
        let now = self.clock.now();
        self.last_cwnd_update = now;
        // No RTT samples since the last update → no signal, hold.
        let Some(win_min) = self.min_rtt_since_update else {
            return cwnd;
        };
        // Copa §2.2 mode switching + competitive AIMD, per-SRTT cadence,
        // BEFORE the direction test so the adapted δ drives this update's
        // law (no-op unless RWM_COPA_COMPETE && wire mode).
        self.compete_update(now);
        let above = self.queue_above_target(cwnd);
        tracing::debug!(
            cwnd,
            above,
            win_min_us = win_min.as_micros() as u64,
            floor_us = self.min_rtt.map(|d| d.as_micros() as u64),
            qfloor_us = self.queue_floor().map(|d| d.as_micros() as u64),
            jitter_us = (self.jitter_est * 1e6) as u64,
            win_jitter_us = (self.win_jitter_est * 1e6) as u64,
            srtt_us = self.srtt().as_micros() as u64,
            n_samples = self.samples_since_update,
            max_bw = self.max_bw as u64,
            bdp_anchor = self.bdp_anchor().map(|b| b.round() as u64),
            anchor_floor = self.anchor_floor(),
            wire = self.wire_mode,
            delta = self.delta,
            velocity = self.velocity,
            compete = self.in_compete,
            compete_switches = self.compete_switches,
            "copa cwnd update"
        );
        // Record this window's min in the queue-floor history.
        self.win_min_history.push_back((now, win_min));
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        while self.win_min_history.front().is_some_and(|&(t, _)| t < cutoff) {
            self.win_min_history.pop_front();
        }
        // Window-level consecutive-difference jitter (see field doc).
        if let Some(prev) = self.prev_win_min {
            let diff = if win_min > prev { win_min - prev } else { prev - win_min };
            self.win_jitter_est += (diff.as_secs_f64() - self.win_jitter_est) * 0.25;
        }
        self.prev_win_min = Some(win_min);
        // Capture the wire-mode queue signal BEFORE the window reset below
        // (wire_dq_secs reads min_rtt_since_update).
        let wire_dq = if self.wire_mode { self.wire_dq_secs() } else { None };
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        let c = cwnd as f64;
        if self.wire_mode {
            return self.wire_update_cwnd(c, above, wire_dq).round() as u32;
        }
        let next = if above {
            self.ramping = false;
            c * BACKOFF_MULT
        } else if self.ramping {
            c * RAMP_GAIN + 1.0
        } else {
            // Steady state: gentle additive probe, but when a trusted BtlBw
            // anchor says cwnd is below the BDP target (post-backoff trough),
            // pull toward it proportionally — a fast catch-up that decays
            // into the +2 probe as cwnd → target (paper Section 12.6). Only
            // ever RAISES the step above +2 (the anchor never suppresses).
            match self.bdp_anchor() {
                Some(bdp) => {
                    let target = ANCHOR_RECOVERY_GAIN * bdp;
                    if c < target {
                        c + (ANCHOR_PULL_ALPHA * (target - c)).max(ADDITIVE_STEP)
                    } else {
                        c + ADDITIVE_STEP
                    }
                }
                None => c + ADDITIVE_STEP,
            }
        };
        next.round() as u32
    }

    /// Wire-mode per-SRTT update (feat/copa-wire-signal): Copa's actual
    /// dynamics (Arun & Balakrishnan, NSDI 2018 §2) at per-SRTT granularity.
    ///
    ///   target_rate = 1/(δ·d_q)  ⇒  target_cwnd = srtt/(δ·d_q)
    ///   direction   = up if cwnd ≤ target_cwnd, else down
    ///   step        = v/δ symbols per SRTT (Copa: cwnd ± v/(δ·cwnd) per
    ///                 ACK × cwnd ACKs/RTT = v/δ per RTT); v doubles once
    ///                 per update after the direction has persisted ≥ 3
    ///                 updates (Copa §2.2 hysteresis), resets to 1 on a
    ///                 flip.
    ///
    /// Equilibrium: rate = μ (the bottleneck) at a standing queue of 1/δ
    /// packets; the ±v/δ dither around it drains the queue to ~empty every
    /// few updates, which is what keeps the 10 s RTT floor fresh (no
    /// ProbeRTT needed). The legacy +2 additive probe IS this law's up-step
    /// at δ = 0.5, v = 1 — continuity with the P1 semantics.
    ///
    /// Two safety caps, both continuity-preserving:
    ///   - up-step ≤ cwnd (at most double per SRTT — Copa's slow-start
    ///     bound), and the ramp itself stays ×1.5+1 until first above;
    ///   - down-step ≤ max(measured queue μ̂·d_q, (1−0.92)·cwnd): draining
    ///     more than the standing queue would empty the PIPE (utilization
    ///     loss for nothing) — the queue cap lands the trough at ≈BDP; the
    ///     0.08·cwnd floor keeps drain progress alive before a BtlBw
    ///     estimate exists.
    fn wire_update_cwnd(&mut self, c: f64, above: bool, wire_dq: Option<f64>) -> f64 {
        let next = self.wire_update_cwnd_uncapped(c, above, wire_dq);
        // Coupling cap (MEASURED at the L1 c2 smoke, v1/v2 of this law):
        // Copa's fixed point is cwnd* = BDP + 1/δ. Once cwnd exceeds the
        // sender's outstanding store cap, it is DECOUPLED from the wire —
        // the delay signal cannot punish further growth (the queue no
        // longer grows with cwnd) and the jitter-clamped d_q keeps voting
        // "up", so cwnd ratchets to MAX_CWND and the burst tail-drops the
        // path qdisc (cwnd 4 000–7 800 observed vs fixed point ≈ 300).
        // Cap at BDP + 2/δ — the fixed point plus one dither amplitude
        // (the up phase still probes a full base step past equilibrium).
        // max_bw's windowed-MAX under-reads only app-limited flows, and an
        // under-read cap is still > BDP at Bulk's 1/δ (the pipe stays
        // fillable and the samples can read the true rate back up — not
        // the §12.11 circular-cap case, which capped AT the anchor).
        match self.bdp_anchor() {
            Some(bdp) => next.min(bdp + 2.0 / self.delta),
            None => next,
        }
    }

    fn wire_update_cwnd_uncapped(&mut self, c: f64, above: bool, wire_dq: Option<f64>) -> f64 {
        if self.ramping {
            if above {
                // Ramp exit: same gentle ×0.92 first step as the legacy /
                // per-ACK fast exit; the velocity law takes over next update.
                self.ramping = false;
                return c * BACKOFF_MULT;
            }
            return c * RAMP_GAIN + 1.0;
        }
        let Some(dq) = wire_dq else {
            return c; // no queue signal this window — hold
        };
        let up = !above;
        if self.last_dir_up == Some(up) {
            self.dir_streak = self.dir_streak.saturating_add(1);
            if self.dir_streak >= 3 {
                // Direction persisted ≥ 3 updates → double the velocity
                // (bounded so the step can never exceed the cwnd ceiling).
                self.velocity =
                    (self.velocity * 2.0).min(self.delta * PathState::MAX_CWND as f64);
            }
        } else {
            self.dir_streak = 1;
            self.velocity = 1.0;
        }
        self.last_dir_up = Some(up);
        let step = (self.velocity / self.delta).max(1.0);
        if up {
            c + step.min(c)
        } else {
            let queue_syms = if self.max_bw > 0.0 {
                self.max_bw * dq
            } else {
                f64::INFINITY
            };
            let drain = step.min(queue_syms.max(c * (1.0 - BACKOFF_MULT)));
            (c - drain).max(0.0)
        }
    }

    /// Immediate backoff (ramp fast-exit or decode-failure congestion):
    /// ×0.92, end the ramp, restart the update window.
    pub(crate) fn backoff(&mut self, cwnd: u32) -> u32 {
        self.ramping = false;
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        self.last_cwnd_update = self.clock.now();
        // Wire mode: a backoff is a down move — reset the velocity streak so
        // the next windowed update re-measures direction from v = 1.
        self.velocity = 1.0;
        self.dir_streak = 1;
        self.last_dir_up = Some(false);
        (cwnd as f64 * BACKOFF_MULT).round() as u32
    }

    /// Classic Copa rate target — DIAGNOSTIC ONLY (the cwnd dynamics above
    /// are the ramp/backoff scheme; this is the closed-form equilibrium).
    ///
    /// Units:
    ///   dq   [s]         = SRTT − floor, clamped ≥ DQ_FLOOR_SECS
    ///   rate [symbols/s] = 1 / (COPA_DELTA [1/symbols] × dq [s])
    ///   cwnd [symbols]   = rate [symbols/s] × SRTT [s]
    ///
    /// (The pre-P7 code multiplied rate by min_rtt and doubled it during
    /// startup; rate × SRTT is the pipe-plus-standing-queue the rate can
    /// keep full over one feedback delay.)
    pub(crate) fn copa_target_cwnd(&self) -> u32 {
        let floor = self.min_rtt.unwrap_or(DEFAULT_SRTT).as_secs_f64();
        let srtt = self.srtt().as_secs_f64();
        let dq = (srtt - floor).max(DQ_FLOOR_SECS);
        // `delta` == COPA_DELTA in legacy mode (byte-identical diagnostic);
        // in wire mode it is the hint-mapped δ the live law targets.
        let rate = 1.0 / (self.delta * dq); // symbols per second
        let cwnd = rate * srtt; // symbols
        (cwnd.round() as u32).clamp(PathState::MIN_CWND, PathState::MAX_CWND)
    }

    /// BtlBw×RTprop BDP estimate in symbols — the active recovery anchor
    /// (paper Section 12.6), or None until it is trustworthy.
    ///
    /// UNITS: max_bw [symbols/s] × min_rtt [s] = symbols (in-flight the
    /// bottleneck rate keeps outstanding over one propagation RTT).
    ///
    /// Gated on ANCHOR_MIN_SAMPLES delivery samples AND a min-RTT sample:
    /// `max_bw` is a windowed MAX of coarse ACK-batch rates with no
    /// per-packet/app-limited accounting, so a handful of samples (or no
    /// RTT floor yet) is not enough to steer cwnd. It STRUCTURALLY
    /// underestimates a warm-up/app-limited flow, which is exactly why it
    /// is only ever used to RAISE cwnd (recovery target + floor), never as
    /// a cap — an underestimate can only fail to help, never suppress.
    pub(crate) fn bdp_anchor(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES || self.max_bw <= 0.0 {
            return None;
        }
        let rtprop = self.min_rtt?.as_secs_f64();
        Some(self.max_bw * rtprop)
    }

    /// The per-path bottleneck rate (symbols/s) for scheduler consumers (the
    /// percap store-cap law reads it via `btlbw_sym_per_s`): the pure
    /// windowed-MAX `max_bw`, gated on ANCHOR_MIN_SAMPLES like `bdp_anchor`
    /// (byte-identical to `bdp_anchor()/RTprop`).
    ///
    /// Historical note (DEPRECATION REGISTER, removed 2026-07-27): the
    /// RWM_RATE_WIRE/RWM_RATE_Q robust-quantile de-noise branch was refuted by
    /// its own structural argument — decode-clocked samples are mostly-low, so
    /// the windowed-MAX is near-correct and ANY sub-max quantile UNDER-reads
    /// and throttles ("Slow-Path Anchor Diagnosis STEP 3", 2026-07-13). The
    /// rate-signal need was met by the honest-anchor family (ADR-0061).
    pub(crate) fn effective_btlbw(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES {
            return None;
        }
        if self.max_bw > 0.0 { Some(self.max_bw) } else { None }
    }

    /// The cwnd floor from the BtlBw anchor (symbols), or None if not yet
    /// established. A floor, NOT a cap — it only ratchets cwnd UP toward the
    /// pipe, so a stale/underestimated BtlBw cannot suppress the window
    /// (paper Section 12.6). Caller clamps against MAX_CWND.
    pub(crate) fn anchor_floor(&self) -> Option<u32> {
        self.bdp_anchor()
            .map(|bdp| (ANCHOR_FLOOR_GAIN * bdp).round() as u32)
    }

    /// Expire samples older than the sliding window.
    fn expire_old_samples(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        self.bw_evict_before(cutoff);
        while self.rtt_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.rtt_samples.pop_front();
        }
    }

    pub(crate) fn set_queue_mult(&mut self, mult: f64) {
        self.queue_mult = mult;
    }

    /// Wire mode: re-derive δ when the protocol hint changes (paired with
    /// `set_queue_mult` from `PathState::set_hint`). No-op in legacy mode —
    /// δ stays COPA_DELTA there.
    pub(crate) fn set_hint_delta(&mut self, hint: ProtocolHint) {
        if self.wire_mode {
            self.delta = copa_delta_for_hint(hint);
            // A hint change re-bases the competitive AIMD: drop to default
            // mode at the new base price; the detector re-enters competitive
            // within 5 RTTs if the buffer-filler evidence persists.
            self.delta_base = self.delta;
            self.in_compete = false;
        }
    }

    /// The raw-fed windowed-min echo ratio (`RWM_HONEST_K`), or None with
    /// the gate off (the legacy smoothed-at-refresh feed stays the only K
    /// source). 1.0 before the first raw sample — identical to a cold
    /// legacy tracker, so warm-up has no behavior cliff.
    pub(crate) fn k_raw_ratio(&self) -> Option<f64> {
        if self.k_raw_on {
            Some(self.k_raw.k())
        } else {
            None
        }
    }

    /// Test hook: force the raw-sample K feed on (bypasses the
    /// process-global env cache, which other tests' env vars could race).
    #[cfg(test)]
    pub(crate) fn force_k_raw(&mut self) {
        self.k_raw_on = true;
    }

    /// Test hook: force the O(1) max-filter read (bypasses the
    /// process-global env cache).
    #[cfg(test)]
    pub(crate) fn force_bw_o1(&mut self) {
        self.bw_o1 = true;
    }

    /// Test hook: force wire mode with an explicit δ. Unit tests must not
    /// depend on the process-global env cache (`copa_wire_active`), which
    /// other tests' env vars could race.
    #[cfg(test)]
    pub(crate) fn force_wire(&mut self, delta: f64) {
        self.wire_mode = true;
        self.delta = delta;
        self.delta_base = delta;
    }

    /// Test hook: enable the competitive mode switching on top of a forced
    /// wire mode (bypasses the process-global env caches, which other tests'
    /// env vars could race).
    #[cfg(test)]
    pub(crate) fn force_compete(&mut self) {
        debug_assert!(self.wire_mode, "compete rides the wire law");
        self.compete_on = true;
    }

    pub(crate) fn reset(&mut self) {
        let clock = self.clock.clone();
        let queue_mult = self.queue_mult;
        let delta_base = self.delta_base;
        *self = Self::new(clock, ProtocolHint::Auto);
        self.queue_mult = queue_mult; // hint survives a path reset
        // Wire mode: the hint-mapped BASE δ survives a path reset; a
        // competitive-mode δ does not (fresh path, fresh detection — the
        // detector re-enters competitive within 5 RTTs if warranted).
        self.delta = delta_base;
        self.delta_base = delta_base;
    }

    /// Read the current min_rtt estimate (for diagnostics/benchmarking).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.min_rtt
    }

    /// (diag/slow-path-anchor) Snapshot of the rate-sample anchor DIAG counters:
    /// (sent, applimited_sent, attributions, no_record, rej_interval, rej_zero,
    /// rej_applimited, generated, bw_fill).  Observation only.
    pub(crate) fn rs_diag(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, usize) {
        (
            self.rs_sent_count,
            self.rs_applimited_sent,
            self.rs_attributions,
            self.rs_no_record,
            self.rs_rej_interval,
            self.rs_rej_zero,
            self.rs_rej_applimited,
            self.rs_generated,
            self.bw_samples.len(),
        )
    }
}
