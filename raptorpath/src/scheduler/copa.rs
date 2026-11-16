//! Copa-lite congestion control: the δ price, the competitive-mode
//! constants, the delay/jitter/anchor constants and `CopaState` with its
//! rate/RTT samplers; re-exported from `scheduler`.

use super::*;

/// Copa congestion control parameter: target queue depth.
/// d_copa = 0.5 targets ~2 packets of queue (paper §8.2).
/// Units: 1/symbols — rate = 1/(d_copa [1/sym] × dq [s]) is symbols/second.
pub(crate) const COPA_DELTA: f64 = 0.5;

/// Hint→δ mapping (paper §8.2): Copa's utility is
/// U = log(throughput) − δ·log(delay), so δ is the marginal latency price.
/// The protocol hint already declares exactly one price ratio — the
/// tail-loss-target scale ζ (`ProtocolHint::tail_loss_scale`, Realtime 0.01
/// / Auto 1 / Bulk 100: Realtime prices lateness 100× dearer, Bulk 100×
/// cheaper). Anchoring Auto at the Copa-paper default δ = 0.5 gives the
/// continuous, constant-free mapping
///
/// ```text
///   δ(hint) = COPA_DELTA / ζ(hint)   ∈ {50 (Realtime), 0.5 (Auto),
///                                        0.005 (Bulk)}
/// ```
///
/// Equilibrium standing queue = 1/δ packets (rate = 1/(δ·d_q) at the
/// bottleneck rate μ ⇒ q = 1/δ), i.e. d_q* = 1/(δ·μ): Bulk tolerates 200
/// symbols of queue (≈19 ms at 10.4 k sym/s), Realtime targets an
/// essentially empty queue (jitter headroom governs), Auto reproduces the
/// classic δ = 0.5 two-packet target. `over` = RWM_COPA_DELTA (the
/// δ-frontier measurement knob), which overrides the hint when set.
///
/// The hint→δ map itself lives once, in [`hint_delta_price`], and is read
/// through [`crate::net::delta_price`] — the one place a hint names a δ.
/// This function is the CC's own view of that number: `RWM_COPA_DELTA`
/// overrides it for the CC alone, so a run can move the contract's δ
/// (`RWM_DELTA`) while pinning the congestion controller.
/// Precedence: `RWM_COPA_DELTA` ▸ `RWM_DELTA` ▸ the hint's map.
pub(crate) fn copa_delta(hint: ProtocolHint, over: Option<f64>) -> f64 {
    over.filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or_else(|| crate::net::delta_price(hint))
}

/// The hint→δ map, with no override of any kind applied: δ(hint) =
/// COPA_DELTA / ζ(hint) ∈ {50, 0.5, 0.005} (paper §4.1). The one
/// transcription of it in the engine; [`crate::net::delta_price`] is its
/// public seat and every δ-priced law reads that.
pub(crate) fn hint_delta_price(hint: ProtocolHint) -> f64 {
    COPA_DELTA / hint.tail_loss_scale()
}

/// `copa_delta` with the RWM_COPA_DELTA env override applied.
pub(crate) fn copa_delta_for_hint(hint: ProtocolHint) -> f64 {
    // Liveness echo, emitted once per process even though this function is
    // re-entered on every hint change: a fixed-δ arm's only distinguishing
    // knob is this override, so it needs an assertable echo
    // (docs/measurement-discipline.md rule 15); the OnceLock keeps it off the
    // repeat path.
    {
        use std::sync::OnceLock;
        static ECHOED: OnceLock<()> = OnceLock::new();
        ECHOED.get_or_init(|| {
            tracing::info!(
                copa_delta_override = %crate::gates::get()
                    .copa_delta
                    .map_or("unset".to_string(), |d| d.to_string()),
                "Copa δ override (RWM_COPA_DELTA; unset = the hint→δ mapping)"
            );
        });
    }
    // The value `[GATES]` echoes as `RWM_COPA_DELTA=`: one resolution, so the
    // echo and the behaviour cannot disagree.
    copa_delta(hint, crate::gates::get().copa_delta)
}

// --- Copa TCP-competitive mode (ADR-0062) ---
//
// Copa §2.2 (Arun & Balakrishnan, "Copa: Practical Delay-Based Congestion
// Control for the Internet", NSDI 2018) defines two operating modes:
//
//   1. the default mode (δ fixed — the paper's 0.5; here the hint-mapped
//      δ(hint), see `copa_delta`), and
//   2. a competitive mode "where δ is adjusted dynamically to match the
//      aggressiveness of typical buffer-filling schemes".
//
// Detection (verbatim mechanism from the Copa paper): Copa's own dynamics
// empty the bottleneck queue at least once every 5·RTT when only Copa flows
// share it (Copa §3). A concurrent long-running buffer-filling flow (Cubic,
// NewReno) breaks that periodicity. "Hence if the sender sees a 'nearly
// empty' queue in the last 5 RTTs, it remains in the default mode;
// otherwise, it switches to competitive mode. We estimate 'nearly empty' as
// any queuing delay lower than 10% of the rate oscillations in the last
// four RTTs; i.e., d_q < 0.1·(RTTmax − RTTmin) where RTTmax is measured
// over the past four RTTs and RTTmin is our long-term minimum" — the
// RTTmax term self-calibrates the notion of "nearly empty" to the path's
// short-term RTT variance.
//
// Competitive law (Copa §2.2): "In competitive mode the sender varies 1/δ
// according to whatever buffer-filling algorithm one wishes to emulate
// (e.g., NewReno, Cubic, etc.). In our implementation we perform AIMD on
// 1/δ based on packet success or loss" — NewReno-style: additive increase
// of 1/δ by 1 per RTT without loss, multiplicative decrease (halve 1/δ) on
// a loss event. "In competitive mode, δ ≤ 0.5. When Copa switches from
// competitive mode to default mode, it resets δ to 0.5."
//
// Composition with the hint→δ mapping (ours): the paper's 0.5 is its
// default-mode δ; ours is δ_base = δ(hint). The faithful generalization
// keeps the hint as the base price and lets competition adapt around it:
// competitive mode enters at δ = δ_base, AIMD keeps 1/δ ≥ 1/δ_base (the
// paper's "δ ≤ 0.5" with 0.5 → δ_base), and switch-back resets δ = δ_base.
// The loss signal is quinn's wire-level loss detection (the pass-through
// shim's recorded `congestion_events` — the same packet-timed layer as the
// wire d_q clock); FEC recovery is irrelevant here because the AIMD term
// only prices aggressiveness against a loss-based competitor, it never
// gates delivery (loss handling stays the FEC layer's job).
//
// Hysteresis is the paper's own: the 5-RTT nearly-empty observation window
// on both edges (a competitive-mode Copa cohort still empties the queue
// every 5 RTT if no buffer-filler is present, so an erroneous or stale
// switch self-corrects within a few RTTs; the paper accepts brief flaps by
// design). Gated: `RWM_COPA_COMPETE` (default off) and only meaningful on
// top of the wire-clocked signal (the δ-mapped update law is what the
// adapted δ feeds).

/// Nearly-empty threshold coefficient (Copa §2.2: d_q < 0.1·(RTTmax−RTTmin)).
pub(crate) const COMPETE_EMPTY_FRAC: f64 = 0.1;
/// Detection window: no nearly-empty queue in the last 5 RTTs ⇒ competitive.
pub(crate) const COMPETE_WINDOW_RTTS: f64 = 5.0;
/// RTTmax lookback for the nearly-empty calibration (paper: past 4 RTTs).
pub(crate) const COMPETE_RTTMAX_RTTS: f64 = 4.0;
/// Bound on 1/δ in competitive mode: 2/δ (the coupling cap's dither term)
/// may never exceed MAX_CWND, so the AIMD's additive growth cannot decouple
/// cwnd from the store (see the coupling-cap note in `wire_update_cwnd`).
pub(crate) const COMPETE_INV_DELTA_MAX: f64 = PathState::MAX_CWND as f64 / 2.0;

/// Floor on the queuing-delay estimate dq, in seconds (0.1 ms).
///
/// Two jobs, both continuity guards (no branch cliffs):
///   - The Copa target `srtt/(δ·dq)` divides by dq; on a LAN where a sample
///     can equal the floor exactly, dq → 0 would explode the target to infinity.
///   - The backoff threshold (queue_mult − 1) × floor collapses toward 0 on
///     sub-millisecond-RTT links; flooring both dq and the threshold at the
///     same 0.1 ms means jitter at the clamp boundary cannot trigger a
///     spurious backoff (dq == threshold is not > threshold).
pub(crate) const DQ_FLOOR_SECS: f64 = 1e-4;

/// Jitter headroom multiplier k in the backoff threshold
/// (queue_mult − 1) × floor + k × jitter_est (paper §8.2, jitter-adjusted
/// queue target).
///
/// A queue target alone assumes path jitter ≪ the target. Real links
/// violate that: with a 10 ms floor and ±3 ms/direction jitter the Bulk
/// threshold is 2.5 ms while a typical RTT sample sits ~6 ms above the
/// floor, so the windowed-min queue signal measures jitter, not queue, and
/// every per-SRTT update buys a ×0.92 backoff (cwnd pinned near the floor).
/// Widening the threshold by k×jitter makes the comparison read "queue
/// above target and above what jitter alone explains". k = 2 puts the
/// false-backoff rate for a min-of-N window
/// at the few-percent level for the N ≈ 4-30 ACK batches an SRTT holds,
/// while a genuine standing queue (which shifts all samples, leaving
/// the consecutive-difference jitter estimate unchanged) still crosses
/// the widened threshold within a few updates. Continuity: jitter → 0
/// recovers the plain queue-target threshold exactly.
pub(crate) const JITTER_HEADROOM: f64 = 2.0;
/// EWMA gain for the consecutive-difference jitter estimator (RFC
/// 3550-style interarrival jitter, gain 1/8 rather than 1/16: the ramp
/// fast-exit consults the threshold from the first ACKs on, so the
/// estimate must converge within tens of samples).
pub(crate) const JITTER_GAIN: f64 = 0.125;

/// EWMA gain for the `rvar_us=` candidate dispersion gauge — RFC 6298 §2's
/// own `β`: `RTTVAR ← (1 − β)·RTTVAR + β·|SRTT − R'|`, β = 1/4, verbatim
/// (inherited by RFC 8985 §6.2 for RACK). Cited, never fitted; the shipped
/// `rtt_var_sq` EWMA uses the identical gain on the second moment.
pub(crate) const SIGMA_CAND_RVAR_GAIN: f64 = 0.25;

/// Window length `L` for the two window-class candidate dispersion gauges
/// (`qsp_us=`, `msd_us=`) — the count of most-recent raw RTT samples held.
///
/// The shipped `sig_us` EWMA at β = 1/4 has an effective memory of
/// `N_eff = (2 − β)/β = 7 samples` no matter what its `n` reads: `n` counts
/// how long the gauge has been fed, not what is in the reading. 256 is 36×
/// that memory, separating the memory axis from the functional axis by more
/// than an order of magnitude, and the `P90` these gauges take rests on
/// `L·(1 − 0.90) = 25.6` order statistics, clearing the standard ≥ 10 by
/// 2.6×.
///
/// Resource bound, stated outside the law: 256 × 4 B = 1 KiB per path, and
/// the sort that reads it is `O(L log L)` at the `[DIAG]` cadence only — the
/// feed site stays `O(1)`.
pub(crate) const SIGMA_CAND_WINDOW: usize = 256;

/// Band width `c` for the fixed-time-lag dispersion gauge `tlag_us=`
/// (paper §7.4): a pair is admitted iff its lag falls in `[τ, c·τ]`.
///
/// A declared resource bound, not a fitted constant, and it does not change
/// the estimand, which is pinned by the band's position at `τ = RTprop`. One
/// octave is the standard dyadic binning of a structure function: at 581
/// samples/s against `RTprop ≈ 38 ms` the band `[38, 76] ms` is 22 spacings
/// wide, so a partner exists for essentially every anchor; at 23.9 samples/s
/// (41.8 ms spacing) exactly one spacing fits, the boundary case the `n`
/// column exists for.
pub(crate) const SIGMA_TLAG_BAND_C: u32 = 2;

/// Decimation factor `m` for `tlag_us=`: a sample enters the gauge's ring only
/// if `τ/m` has elapsed since the last admitted one.
///
/// A declared resource bound, and a memory measure rather than a filter: it
/// caps the ring's sample rate at `m/τ` regardless of the ack rate, so a
/// `SIGMA_CAND_WINDOW`-deep ring spans `256·τ/8 = 32·τ` at every leg.
/// Without it the ring at a 20 kHz sender leg would span 12.8 ms — far less
/// than one `RTprop` — the band `[τ, 2τ]` would contain no pairs at all, and
/// the gauge would render `-` at exactly the legs it is built for.
///
/// It does not change the estimand: rate invariance comes from selecting
/// pairs by elapsed time, and any fixed `m` yields it. `m` sets only
/// the resolution of the realized lag inside the band — `τ/8`, i.e. 12.5 % of
/// `τ`.
pub(crate) const SIGMA_TLAG_DECIM_M: u32 = 8;

/// Quantile of the per-update window-min history used as the queue floor
/// (jitter-robust queue floor).
///
/// The queuing-delay signal compares a min-of-N statistic (N ≈ the ACK
/// samples in one SRTT window) against the propagation floor, a
/// min-of-thousands over 10s. On a jittery link those are different
/// statistics: the 10s floor can sit ~5 ms below a typical window min,
/// a permanent apparent dq with an empty queue (and correlated jitter
/// defeats the consecutive-difference jitter estimate). Comparing the
/// window min against a low quantile of its own recent distribution is
/// self-calibrating
/// under any jitter correlation structure: queue-free windows sit near
/// their own P10 by construction, while a genuine standing queue
/// shifts every window min up within one SRTT and the 10s-window
/// quantile lags behind — the signal survives. On a clean link every
/// window min equals the floor, the quantile equals the floor, and
/// the plain queue-target semantics are recovered exactly.
pub(crate) const QUEUE_FLOOR_QUANTILE: f64 = 0.10;

/// Startup ramp: multiplicative growth factor per window update, until the
/// first backoff (cwnd = cwnd × 1.5 + 1).
pub(crate) const RAMP_GAIN: f64 = 1.5;
/// Steady state: additive increase per window update (symbols).
pub(crate) const ADDITIVE_STEP: f64 = 2.0;
/// Backoff: multiplicative decrease when the windowed min RTT exceeds the
/// hint-coupled queue target.
pub(crate) const BACKOFF_MULT: f64 = 0.92;
/// SRTT assumed before the first RTT sample arrives (update cadence only).
pub(crate) const DEFAULT_SRTT: Duration = Duration::from_millis(50);

// --- BtlBw-anchored recovery (paper §8.3) ---
//
// The additive +2/SRTT recovery after a delay backoff crawls: from a
// ×0.92 trough it takes dozens of SRTTs to re-fill the pipe, so cwnd sits
// well below BDP. We
// already maintain a delivery-rate max-filter (`max_bw`) and a 10s min-RTT
// (`min_rtt`); their product is a BtlBw×RTprop = BDP estimate. Use it to
// (a) pull post-backoff recovery toward BDP proportionally (decaying to
// the gentle +2 probe as cwnd → BDP) and (b) floor cwnd at the estimate so
// a backoff (or a jitter false-positive) cannot crawl cwnd below the pipe.
//
// `max_bw` is a windowed max of coarse ACK-batch delivery rates — no
// per-packet sampling and no app-limited detection (BBR discards
// app-limited samples precisely because they underestimate BtlBw). For a
// warm-up-limited transfer the estimate reads low exactly when we would
// want it high. So the anchor is used only to raise cwnd — a recovery
// target and a floor, never a cap. A stale/under-
// estimated BtlBw can then only fail to help; it can never suppress cwnd.

/// Minimum delivery-rate samples in the 10s window before the BtlBw anchor
/// is trusted. A handful of coarse samples is too noisy to floor cwnd on.
///
/// Public because the store-cap bootstrap floor is derived from it
/// (`net::sender_policy::STORE_CAP_FLOOR`, paper §6.2): the floor's job is to
/// keep enough outstanding that this gate can close, and the number of
/// samples the gate wants is this constant.
pub const ANCHOR_MIN_SAMPLES: usize = 8;
/// cwnd_gain on the BtlBw×RTprop BDP estimate for the post-backoff recovery
/// target. 1.0 = aim to re-fill exactly the pipe; the gentle +2 probe (and
/// the hint-coupled queue target) still governs the standing queue above
/// BDP, so this is not BBR's cwnd_gain=2 (which deliberately buffers 1×BDP).
pub(crate) const ANCHOR_RECOVERY_GAIN: f64 = 1.0;
/// Proportional pull toward the recovery target per SRTT update: the
/// increment is max(ADDITIVE_STEP, α·(target − cwnd)). Continuous and
/// self-decaying — at α=0.25 a trough at 0.5×BDP closes ~90% of the gap in
/// ~8 SRTTs (vs ~40 SRTTs for +2), and the term vanishes into +2 as
/// cwnd → target (no discrete phase, no cliff).
pub(crate) const ANCHOR_PULL_ALPHA: f64 = 0.25;
/// cwnd floor as a multiple of the BtlBw×RTprop estimate. cwnd is never
/// driven below this once the anchor is established (floor, not cap).
///
/// 0.85, not 1.0: a floor at the full BDP estimate pins cwnd there even
/// when the delay signal reports queue-above-target, so the floor maintains
/// a standing queue the backoff can no longer drain. Flooring at 0.85×BDP
/// keeps cwnd off the 8-symbol collapse while leaving the delay
/// backoff ~15% of authority around BDP to drain a genuine queue; the
/// recovery pull (gain 1.0) still re-fills toward full BDP each clean
/// update, so cwnd oscillates just under the pipe rather than sitting in
/// standing bufferbloat. Because `max_bw` also underestimates during
/// warm-up, the realized floor sits further below true BDP — the safety
/// (see the risk note above).
pub(crate) const ANCHOR_FLOOR_GAIN: f64 = 0.85;

/// Floor on the in_flight expiry horizon (see `PathState::expire_in_flight`).
/// max(4×SRTT, this): stranded budget (lost best-effort ACK datagrams)
/// releases within ~a quarter second instead of jamming the TUN gate until
/// the leak-guard decay.
pub(crate) const IN_FLIGHT_EXPIRY_MIN: Duration = Duration::from_millis(250);

/// Hint-coupled queue-target multiplier (paper §8.2): the standing queue is
/// allowed to raise the windowed min RTT to floor × mult before Copa-lite
/// backs off. Realtime keeps the queue near-empty; Bulk trades a deeper
/// queue for utilization. A three-arm hint table on the application-echo
/// law, a declared corner (paper §11.2).
pub(crate) fn queue_target_mult(hint: ProtocolHint) -> f64 {
    match hint {
        ProtocolHint::Realtime => 1.08,
        ProtocolHint::Auto => 1.125,
        ProtocolHint::Bulk => 1.25,
    }
}

/// Copa-lite delay-based congestion control state.
///
/// Copa (Arun & Balakrishnan, NSDI 2018), simplified to the semantics the
/// L0 gate suite pins (`tests/gate_suite.rs`):
///
///   - Propagation floor: min RTT sample over a sliding ~10s window
///     (windowed rather than lifetime so a route change re-learns within one
///     window).
///   - Queuing-delay signal: min RTT sample since the last cwnd update.
///   - Two-speed ramp: ×1.5+1 per update until first backoff, then +2/×0.92.
///   - Backoff when the windowed min exceeds the hint-coupled queue target
///     floor × queue_mult.
///
/// Key properties:
///   - No phases (no Startup/ProbeBw/ProbeRtt state machine)
///   - Natural rate oscillation drains queues without explicit probe phase
///   - Compatible with taper function (no FEC protection gaps)
///   - Delay-based: loss + stable RTT = channel loss (ignore)
///
/// See paper §8.
#[derive(Debug)]
pub struct CopaState {
    /// How long to keep samples in sliding windows (10s).
    window_duration: Duration,
    /// `RWM_COPA_WIRE` only: the controller's own smoothed RTT on the wire
    /// sample stream (EWMA 7/8 + 1/8, seeded from the first sample). In the
    /// default arm the cadence reads the leg's `s` (`LegState`, W3.2 A1), the
    /// same stream, so this stays `None`. Under the arm the two streams
    /// differ (wire vs echo), so the controller keeps its own.
    wire_srtt: Option<Duration>,
    /// Minimum RTT sample since the last cwnd update — the queuing-delay
    /// signal (windowed min, not an EWMA; see module docs).
    min_rtt_since_update: Option<Duration>,
    /// Per-update window-min history over the sliding window: the queue
    /// floor is a low quantile of these (QUEUE_FLOOR_QUANTILE) — the same
    /// statistic as the queue signal itself, so jitter cannot open a
    /// permanent gap between signal and floor (see const docs).
    win_min_history: VecDeque<(Instant, Duration)>,
    /// RTT samples recorded since the last cwnd update — evidence count
    /// for the ramp fast-exit (a min over ≥3 samples; a min-of-1 is just
    /// one jittery sample and fires false ramp exits).
    pub(crate) samples_since_update: u32,
    /// Window-level jitter estimate (seconds): EWMA (gain 1/4) of
    /// |win_min_i − win_min_{i−1}| between consecutive cwnd updates.
    /// Under correlated jitter the raw-sample consecutive differences
    /// collapse while the window min wanders by several ms per update; this
    /// estimator sees that amplitude and stays shift-robust (a standing queue
    /// is one transition sample).
    pub(crate) win_jitter_est: f64,
    /// Previous update's window min (for the window-level difference).
    prev_win_min: Option<Duration>,
    /// True until the first congestion backoff (multiplicative ramp phase).
    pub(crate) ramping: bool,
    /// Hint-coupled queue-target multiplier: 1.08/1.125/1.25.
    pub(crate) queue_mult: f64,
    /// When the cwnd was last updated (updates run once per SRTT).
    last_cwnd_update: Instant,
    // --- Wire-clocked δ-mapped update law (ADR-0062) ----------------------
    /// Wire mode: the delay term is the packet-timed wire RTT (fed by the
    /// transport seam) and the cwnd update law is Copa's actual
    /// target-rate/velocity dynamics around rate = 1/(δ·d_q). False (default)
    /// ⇒ the application-echo law.
    wire_mode: bool,
    /// Copa δ (1/symbols): the hint-mapped latency price (`copa_delta`).
    /// Outside wire mode this stays COPA_DELTA.
    pub(crate) delta: f64,
    /// Copa velocity v: the per-SRTT step is v/δ symbols; v doubles once
    /// per update while the direction has persisted ≥ 3 consecutive
    /// updates, and resets to 1 on a direction flip (Copa §2.2's velocity
    /// parameter at per-SRTT granularity — the 3-window hysteresis bounds the
    /// overshoot of a pure every-window doubling, which otherwise pins cwnd at
    /// MAX_CWND with large app-RTT spikes).
    velocity: f64,
    /// Consecutive same-direction update count (velocity hysteresis).
    dir_streak: u32,
    /// Direction of the previous wire-mode update (None = no update yet /
    /// after a backoff reset).
    last_dir_up: Option<bool>,
    // --- Copa §2.2 TCP-competitive mode ----------------------------------------
    /// Mode switching enabled (RWM_COPA_COMPETE && wire mode). False
    /// (default) ⇒ every field below is inert.
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
            window_duration: Duration::from_secs(10),
            wire_srtt: None,
            min_rtt_since_update: None,
            win_min_history: VecDeque::new(),
            samples_since_update: 0,
            win_jitter_est: 0.0,
            prev_win_min: None,
            ramping: true,
            queue_mult: queue_target_mult(hint),
            last_cwnd_update: now,
            clock,
        }
    }

    /// Record an RTT sample: the controller's share of it — the wire arm's own
    /// srtt, the queuing-delay window min and the update-window sample count.
    /// The leg's estimators (`s`, RTprop, jitter, σ and the dispersion
    /// candidates) take the sample in [`LegState::record_sample`], between
    /// this and [`Self::note_competition`] (`PathState::record_rtt`).
    pub(crate) fn note_sample(&mut self, rtt: Duration) {
        // SRTT EWMA (RFC 6298 weights) — the wire arm's controller-local one.
        if self.wire_mode {
            self.wire_srtt = Some(match self.wire_srtt {
                Some(s) => s.mul_f64(0.875) + rtt.mul_f64(0.125),
                None => rtt,
            });
        }

        // Windowed min for the queuing-delay signal.
        self.min_rtt_since_update = Some(match self.min_rtt_since_update {
            Some(m) => m.min(rtt),
            None => rtt,
        });
        self.samples_since_update += 1;
    }

    /// Copa §2.2 competitive-mode detector sampling (after the leg's RTprop
    /// is refreshed with this sample): mark the instants at which the queue is
    /// "nearly empty". Gated so the shipped/wire-only paths pay nothing.
    pub(crate) fn note_competition(&mut self, rtt: Duration, leg: &LegState) {
        if self.compete_on {
            let now = self.clock.now();
            self.compete_note_sample(rtt, now, leg);
        }
    }

    /// Copa §2.2 nearly-empty detector, per wire RTT sample: maintain RTTmax
    /// over the past ~4 RTTs (monotonic max-deque, the mirror of the min
    /// deque above) and mark `last_nearly_empty` whenever the current
    /// queuing delay d_q = sample − RTTmin(long-term) is below
    /// 0.1·(RTTmax − RTTmin). The RTTmax term calibrates "nearly empty" to
    /// the path's short-term RTT variance (Copa §2.2); the DQ_FLOOR guard
    /// keeps a zero-variance clean/idle link (RTTmax == RTTmin ⇒ threshold
    /// 0) from reading as "never empty" — a d_q at the clamp floor is an
    /// empty queue.
    fn compete_note_sample(&mut self, rtt: Duration, now: Instant, leg: &LegState) {
        while self.compete_max_deque.back().is_some_and(|&(_, r)| r <= rtt) {
            self.compete_max_deque.pop_back();
        }
        self.compete_max_deque.push_back((now, rtt));
        let lookback = self.cadence_srtt(leg).mul_f64(COMPETE_RTTMAX_RTTS);
        let cutoff = now.checked_sub(lookback).unwrap_or(now);
        while self
            .compete_max_deque
            .front()
            .is_some_and(|&(t, _)| t < cutoff)
        {
            self.compete_max_deque.pop_front();
        }
        let Some(floor) = leg.min_rtt else { return };
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

    /// Wire-level loss evidence for the competitive AIMD: fed the
    /// pass-through shim's cumulative `congestion_events` counter for
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
    fn compete_update(&mut self, now: Instant, leg: &LegState) {
        if !self.compete_on || self.ramping {
            return;
        }
        let window = self.cadence_srtt(leg).mul_f64(COMPETE_WINDOW_RTTS);
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

    /// The smoothed RTT the controller's cadence and targets read: the leg's
    /// `s` (default arm), or the controller-local wire EWMA (`RWM_COPA_WIRE`);
    /// 50 ms before the first sample and until a revived leg reseeds.
    pub(crate) fn cadence_srtt(&self, leg: &LegState) -> Duration {
        if self.wire_mode {
            self.wire_srtt.unwrap_or(DEFAULT_SRTT)
        } else {
            leg.s.cadence()
        }
    }

    /// The measured smoothed RTT σ's deviation is taken against (`None`
    /// before a sample).
    pub(crate) fn reference_srtt(&self, leg: &LegState) -> Option<Duration> {
        if self.wire_mode { self.wire_srtt } else { leg.s.measured() }
    }

    /// The queue floor: QUEUE_FLOOR_QUANTILE of the recent window-min
    /// history — the same min-of-N statistic as the queue signal itself
    /// (see const docs; falls back to the propagation floor before any
    /// history accumulates). Never below the propagation floor by
    /// construction (every window min is itself an RTT sample).
    fn queue_floor(&self, leg: &LegState) -> Option<Duration> {
        if self.win_min_history.is_empty() {
            return leg.min_rtt;
        }
        let mut v: Vec<Duration> = self.win_min_history.iter().map(|&(_, d)| d).collect();
        let idx = (((v.len() - 1) as f64) * QUEUE_FLOOR_QUANTILE).round() as usize;
        let (_, nth, _) = v.select_nth_unstable(idx);
        Some(*nth)
    }

    /// Wire-mode queuing delay d_q (seconds): standing wire RTT (the most
    /// recent packet-timed sample — quinn's srtt, already an EWMA over many
    /// per-ack samples: Copa §2's RTTstanding) − propagation floor − jitter
    /// headroom, clamped ≥ DQ_FLOOR_SECS.
    ///
    /// Differences from the application-echo signal, all consequences of the
    /// wire clock:
    ///   - The signal is the current standing estimate, not the per-window
    ///     min: the δ-sawtooth's drain trough falls inside every update
    ///     window, so a windowed min reads "queue empty" at every update —
    ///     the direction stays up, the velocity compounds, and cwnd pins
    ///     MAX_CWND. The smoothed standing sample tracks the queue the law is
    ///     actually steering.
    ///   - The floor is the raw 10 s min (`min_rtt`), not the quantile
    ///     queue floor: wire samples are already smoothed (sample-level
    ///     jitter averaged out), and the law's dither drains the queue to
    ///     ~empty regularly, refreshing the raw min (the quantile floor was
    ///     an app-echo jitter fix, and under a deep Bulk standing queue it
    ///     would creep up to the queue itself within its 10 s window —
    ///     staleness by construction).
    ///   - The jitter headroom is subtracted from the measured d_q rather
    ///     than added to a threshold, so one adjusted quantity feeds both
    ///     the above-target test and the target-rate law (continuity:
    ///     jitter → 0 recovers plain Copa exactly).
    fn wire_dq_secs(&self, leg: &LegState) -> Option<f64> {
        let standing = leg.prev_rtt_sample?;
        let floor = leg.min_rtt?;
        let jitter = leg.jitter_est.max(self.win_jitter_est);
        Some(
            (standing.as_secs_f64() - floor.as_secs_f64() - JITTER_HEADROOM * jitter)
                .max(DQ_FLOOR_SECS),
        )
    }

    /// Wire-mode congestion test: is the current rate above Copa's target
    /// rate 1/(δ·d_q)?  cwnd/srtt > 1/(δ·d_q)  ⇔  cwnd·δ·d_q > srtt.
    fn wire_above_target(&self, cwnd: u32, leg: &LegState) -> bool {
        match self.wire_dq_secs(leg) {
            Some(dq) => cwnd as f64 * self.delta * dq > self.cadence_srtt(leg).as_secs_f64(),
            None => false,
        }
    }

    /// Whether the standing-queue signal is above the hint-coupled target:
    /// windowed-min RTT − queue_floor (= dq, clamped ≥ 0.1ms) exceeds
    /// (queue_mult − 1) × queue_floor + k × jitter_est (also clamped
    /// ≥ 0.1ms).
    ///
    /// Equivalent to `min_rtt_win > floor × queue_mult` except for three
    /// continuity guards (all vanish on a clean link, where
    /// queue_floor == floor and jitter_est == 0):
    ///   - the dq clamp keeps sub-millisecond-RTT links from backing off
    ///     on sub-clamp noise (see DQ_FLOOR_SECS),
    ///   - the queue floor is a low quantile of the window-min history
    ///     rather than the extreme-value 10s min, so jitter cannot open a
    ///     permanent gap between signal and floor (QUEUE_FLOOR_QUANTILE), and
    ///   - the k × jitter_est term covers the residual within-window
    ///     spread at small sample counts (JITTER_HEADROOM).
    ///
    /// In wire mode the test is `wire_above_target` instead.
    pub(crate) fn queue_above_target(&self, cwnd: u32, leg: &LegState) -> bool {
        if self.wire_mode {
            return self.wire_above_target(cwnd, leg);
        }
        let (Some(win_min), Some(floor)) = (self.min_rtt_since_update, self.queue_floor(leg)) else {
            return false;
        };
        let floor_s = floor.as_secs_f64();
        let dq = (win_min.as_secs_f64() - floor_s).max(DQ_FLOOR_SECS);
        // Headroom covers whichever jitter evidence is larger: per-sample
        // (consecutive raw-sample differences) or per-window (consecutive
        // window-min differences) — under correlated jitter (a slow RTT
        // wave) only the window-level estimator sees the true amplitude
        // (raw diffs stay sub-ms while window mins wander several ms between
        // updates). Both are consecutive-difference EWMAs,
        // hence shift-robust: a standing queue contributes one transition
        // sample, not a persistent inflation, so congestion detection
        // survives (unlike a quantile-spread term, which a level shift
        // would inflate for a full window).
        let jitter = leg.jitter_est.max(self.win_jitter_est);
        let dq_target = ((self.queue_mult - 1.0) * floor_s + JITTER_HEADROOM * jitter)
            .max(DQ_FLOOR_SECS);
        dq > dq_target
    }

    /// Whether a cwnd window update is due (once per SRTT).
    pub(crate) fn should_update(&self, now: Instant, leg: &LegState) -> bool {
        now.duration_since(self.last_cwnd_update) >= self.cadence_srtt(leg)
    }

    /// Per-SRTT window update:
    ///   - windowed min above the queue target → backoff ×0.92, end ramp
    ///   - ramping → ×1.5 + 1
    ///   - steady state → +2
    /// Resets the queuing-delay window. Returns the new cwnd (unclamped
    /// against MIN/MAX — the caller clamps).
    pub(crate) fn update_cwnd(&mut self, cwnd: u32, leg: &LegState) -> u32 {
        let now = self.clock.now();
        self.last_cwnd_update = now;
        // No RTT samples since the last update → no signal, hold.
        let Some(win_min) = self.min_rtt_since_update else {
            return cwnd;
        };
        // Copa §2.2 mode switching + competitive AIMD, per-SRTT cadence,
        // before the direction test so the adapted δ drives this update's
        // law (no-op unless RWM_COPA_COMPETE && wire mode).
        self.compete_update(now, leg);
        let above = self.queue_above_target(cwnd, leg);
        tracing::debug!(
            cwnd,
            above,
            win_min_us = win_min.as_micros() as u64,
            floor_us = leg.min_rtt.map(|d| d.as_micros() as u64),
            qfloor_us = self.queue_floor(leg).map(|d| d.as_micros() as u64),
            jitter_us = (leg.jitter_est * 1e6) as u64,
            win_jitter_us = (self.win_jitter_est * 1e6) as u64,
            srtt_us = self.cadence_srtt(leg).as_micros() as u64,
            n_samples = self.samples_since_update,
            max_bw = leg.max_bw as u64,
            bdp_anchor = leg.bdp_anchor().map(|b| b.round() as u64),
            anchor_floor = leg.anchor_floor(),
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
        // Capture the wire-mode queue signal before the window reset below
        // (wire_dq_secs reads min_rtt_since_update).
        let wire_dq = if self.wire_mode { self.wire_dq_secs(leg) } else { None };
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        let c = cwnd as f64;
        if self.wire_mode {
            return self.wire_update_cwnd(c, above, wire_dq, leg).round() as u32;
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
            // into the +2 probe as cwnd → target (paper §8.3). Only ever
            // raises the step above +2 (the anchor never suppresses).
            match leg.bdp_anchor() {
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

    /// Wire-mode per-SRTT update: Copa's actual dynamics (Arun &
    /// Balakrishnan, NSDI 2018 §2) at per-SRTT granularity.
    ///
    /// ```text
    ///   target_rate = 1/(δ·d_q)  ⇒  target_cwnd = srtt/(δ·d_q)
    ///   direction   = up if cwnd ≤ target_cwnd, else down
    ///   step        = v/δ symbols per SRTT (Copa: cwnd ± v/(δ·cwnd) per
    ///                 ACK × cwnd ACKs/RTT = v/δ per RTT); v doubles once
    ///                 per update after the direction has persisted ≥ 3
    ///                 updates (Copa §2.2 hysteresis), resets to 1 on a
    ///                 flip.
    /// ```
    ///
    /// Equilibrium: rate = μ (the bottleneck) at a standing queue of 1/δ
    /// packets; the ±v/δ dither around it drains the queue to ~empty every
    /// few updates, which is what keeps the 10 s RTT floor fresh (no
    /// ProbeRTT needed). The application-echo law's +2 additive probe is this
    /// law's up-step at δ = 0.5, v = 1.
    ///
    /// Two safety caps, both continuity-preserving:
    ///   - up-step ≤ cwnd (at most double per SRTT — Copa's slow-start
    ///     bound), and the ramp itself stays ×1.5+1 until first above;
    ///   - down-step ≤ max(measured queue μ̂·d_q, (1−0.92)·cwnd): draining
    ///     more than the standing queue would empty the pipe (utilization
    ///     loss for nothing) — the queue cap lands the trough at ≈BDP; the
    ///     0.08·cwnd floor keeps drain progress alive before a BtlBw
    ///     estimate exists.
    fn wire_update_cwnd(&mut self, c: f64, above: bool, wire_dq: Option<f64>, leg: &LegState) -> f64 {
        let next = self.wire_update_cwnd_uncapped(c, above, wire_dq, leg);
        // Coupling cap: Copa's fixed point is cwnd* = BDP + 1/δ. Once cwnd
        // exceeds the sender's outstanding store cap, it is decoupled from
        // the wire — the delay signal cannot punish further growth (the
        // queue no longer grows with cwnd) and the jitter-clamped d_q keeps
        // voting "up", so cwnd ratchets to MAX_CWND and the burst tail-drops
        // the path qdisc. Cap at BDP + 2/δ — the fixed point plus one dither
        // amplitude (the up phase still probes a full base step past
        // equilibrium). max_bw's windowed max under-reads only app-limited
        // flows, and an under-read cap is still > BDP at Bulk's 1/δ (the pipe
        // stays fillable and the samples can read the true rate back up,
        // unlike a cap at the anchor itself, which is circular).
        match leg.bdp_anchor() {
            Some(bdp) => next.min(bdp + 2.0 / self.delta),
            None => next,
        }
    }

    fn wire_update_cwnd_uncapped(&mut self, c: f64, above: bool, wire_dq: Option<f64>, leg: &LegState) -> f64 {
        if self.ramping {
            if above {
                // Ramp exit: same gentle ×0.92 first step as the
                // application-echo / per-ACK fast exit; the velocity law takes
                // over next update.
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
            let queue_syms = if leg.max_bw > 0.0 {
                leg.max_bw * dq
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

    pub(crate) fn set_queue_mult(&mut self, mult: f64) {
        self.queue_mult = mult;
    }

    /// Wire mode: re-derive δ when the protocol hint changes (paired with
    /// `set_queue_mult` from `PathState::set_hint`). No-op outside wire
    /// mode — δ stays COPA_DELTA there.
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
        // Wire mode: the hint-mapped base δ survives a path reset; a
        // competitive-mode δ does not (fresh path, fresh detection — the
        // detector re-enters competitive within 5 RTTs if warranted).
        self.delta = delta_base;
        self.delta_base = delta_base;
    }

}
