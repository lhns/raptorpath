//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.
//!
//! Congestion control is Copa-lite (delay-based, paper Sections 12.4-12.5),
//! ported from the L0-proven gate-suite driver (P1+P2 semantics):
//!
//!   - Propagation floor = min RTT sample in a sliding ~10s window.
//!   - Queuing-delay signal = min RTT sample since the last cwnd update
//!     (a windowed MIN, not an EWMA: the min sees through transient
//!     serialization bursts to the standing queue; an EWMA stays inflated
//!     long after the queue drains and causes a backoff spiral).
//!   - Hint-coupled queue target (P1): back off when the windowed min
//!     exceeds floor × {1.08 Realtime, 1.125 Auto, 1.25 Bulk}.
//!   - Two-speed ramp: multiplicative ×1.5+1 per RTT until the first
//!     backoff, then additive +2 / multiplicative ×0.92.
//!   - Token-bucket pacing at cwnd/SRTT with burst allowance max(10, cwnd/8)
//!     (state lives here; the drain in net/mod.rs consumes the tokens).
//!
//! Loss alone does NOT reduce the window — only a standing queue does.
//! This prevents wireless random loss from collapsing throughput.
//! No ProbeRTT phase (natural oscillation refreshes the floor).
//!
//! UNITS: `cwnd`, `in_flight`, and pacing tokens are all in SYMBOLS.
//! Pacing rate = cwnd [symbols] / SRTT [s] = symbols/second.

pub mod clock;
pub use clock::*;

use crate::control::fec_rate::ProtocolHint;
use crate::control::LossEstimator;
use crate::fec::{FecBackend, WireSymbol};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

/// Copa congestion control parameter: target queue depth.
/// d_copa = 0.5 targets ~2 packets of queue. See paper Section 12.4.
/// Units: 1/symbols — rate = 1/(d_copa [1/sym] × dq [s]) is symbols/second.
const COPA_DELTA: f64 = 0.5;

/// Floor on the queuing-delay estimate dq, in seconds (0.1 ms).
///
/// Two jobs, both continuity guards (no branch cliffs):
///   - `copa_target_cwnd()` divides by dq; on a LAN where a sample can equal
///     the floor exactly, dq → 0 would explode the target to infinity.
///   - The backoff threshold (queue_mult − 1) × floor collapses toward 0 on
///     sub-millisecond-RTT links; flooring both dq and the threshold at the
///     same 0.1 ms means jitter at the clamp boundary cannot trigger a
///     spurious backoff (dq == threshold is not > threshold).
const DQ_FLOOR_SECS: f64 = 1e-4;

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
const JITTER_HEADROOM: f64 = 2.0;
/// EWMA gain for the consecutive-difference jitter estimator (RFC
/// 3550-style interarrival jitter, gain 1/8 rather than 1/16: the ramp
/// fast-exit consults the threshold from the first ACKs on, so the
/// estimate must converge within tens of samples).
const JITTER_GAIN: f64 = 0.125;
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
const QUEUE_FLOOR_QUANTILE: f64 = 0.10;

/// Startup ramp: multiplicative growth factor per window update, until the
/// first backoff (gate driver P1: cwnd = cwnd × 1.5 + 1).
const RAMP_GAIN: f64 = 1.5;
/// Steady state: additive increase per window update (symbols).
const ADDITIVE_STEP: f64 = 2.0;
/// Backoff: multiplicative decrease when the windowed min RTT exceeds the
/// hint-coupled queue target.
const BACKOFF_MULT: f64 = 0.92;
/// SRTT assumed before the first RTT sample arrives (update cadence only).
const DEFAULT_SRTT: Duration = Duration::from_millis(50);

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
const ANCHOR_MIN_SAMPLES: usize = 8;
/// cwnd_gain on the BtlBw×RTprop BDP estimate for the post-backoff recovery
/// TARGET. 1.0 = aim to re-fill exactly the pipe; the gentle +2 probe (and
/// the hint-coupled queue target) still governs the standing queue ABOVE
/// BDP, so this is not BBR's cwnd_gain=2 (which deliberately buffers 1×BDP).
const ANCHOR_RECOVERY_GAIN: f64 = 1.0;
/// Proportional pull toward the recovery target per SRTT update: the
/// increment is max(ADDITIVE_STEP, α·(target − cwnd)). Continuous and
/// self-decaying — at α=0.25 a trough at 0.5×BDP closes ~90% of the gap in
/// ~8 SRTTs (vs ~40 SRTTs for +2), and the term vanishes into +2 as
/// cwnd → target (no discrete phase, no cliff).
const ANCHOR_PULL_ALPHA: f64 = 0.25;
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
const ANCHOR_FLOOR_GAIN: f64 = 0.85;

/// Floor on the in_flight expiry horizon (see `PathState::expire_in_flight`).
/// max(4×SRTT, this): stranded budget (lost best-effort ACK datagrams)
/// releases within ~a quarter second instead of jamming the TUN gate until
/// the 2s leak-guard decay.
const IN_FLIGHT_EXPIRY_MIN: Duration = Duration::from_millis(250);

/// Hint-coupled queue-target multiplier (P1, paper Section 12.4): the
/// standing queue is allowed to raise the windowed min RTT to
/// floor × mult before Copa-lite backs off. Realtime keeps the queue
/// near-empty; Bulk trades a deeper queue for utilization.
fn queue_target_mult(hint: ProtocolHint) -> f64 {
    match hint {
        ProtocolHint::Realtime => 1.08,
        ProtocolHint::Auto => 1.125,
        ProtocolHint::Bulk => 1.25,
    }
}

/// Scheduling weights derived from protocol hint.
/// RWM placement (paper §16.3) softmax temperature.
///
/// The placement cost is measured in units of the FASTEST path's SRTT (the
/// load term is `E_i(load)/ref_srtt`, ≈ 0.5 for the idle fast path). `T` is
/// therefore the softness of the water-filling transition in units of a fast
/// one-way delay: two paths whose costs differ by `T` place at odds e:1 ≈
/// 2.7:1. `T → 0` is the paper's strict best-path (argmin) limit; larger `T`
/// dithers and pulls more traffic onto a slower path (more aggregation, more
/// head-of-line risk on a reliable in-order stream). This is the one dial
/// §16.3 names as a documented constant; L1 measurement tunes it.
pub(crate) const PLACE_TEMPERATURE: f64 = 0.15;

/// The effective placement temperature: `PLACE_TEMPERATURE`, overridable once
/// per process via the `RWM_PLACE_T` env var (the §16.3 dial exposed for L1
/// tuning without a rebuild). Read once and cached.
fn place_temperature() -> f64 {
    use std::sync::OnceLock;
    static T: OnceLock<f64> = OnceLock::new();
    *T.get_or_init(|| {
        std::env::var("RWM_PLACE_T")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|t| *t > 0.0 && t.is_finite())
            .unwrap_or(PLACE_TEMPERATURE)
    })
}

/// Floor (seconds) for the SRTT reference that de-dimensionalises the
/// propagation-preference term — a div-by-zero guard for the pre-first-sample
/// window, NOT a tuning knob (any positive value cancels once real RTTs land).
pub(crate) const PLACE_REF_FLOOR_SECS: f64 = 0.001;

/// Controls the latency vs bandwidth trade-off in the interpolated objective.
/// See paper Section 13.8.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingWeights {
    /// Weight for latency cost: SUM(x_i × E_i)
    pub w_lat: f64,
    /// Weight for bandwidth overhead cost: SUM(x_i × r_i)
    pub w_bw: f64,
    /// Weight for the fate-diversity penalty ρ_fate (RWM per-symbol placement,
    /// paper Section 16.3). Applies to REPAIR symbols only: it is the
    /// continuous form of the old hard `best_repair_path_avoiding` rule — a
    /// repair placed on a path that already carried the window symbols it
    /// covers gains no diversity, so its marginal cost rises. Zero for source.
    pub w_div: f64,
}

impl SchedulingWeights {
    pub fn from_hint(hint: ProtocolHint) -> Self {
        // w_div is hint-independent: fate diversity for a repair is worth the
        // same across workloads (a repair correlated with its coverage is
        // wasted regardless of the (δ, ρ, r) triangle). See place_symbol.
        match hint {
            ProtocolHint::Realtime => Self { w_lat: 1.0, w_bw: 0.0, w_div: 1.0 },
            ProtocolHint::Bulk => Self { w_lat: 0.0, w_bw: 1.0, w_div: 1.0 },
            ProtocolHint::Auto => Self { w_lat: 0.5, w_bw: 0.5, w_div: 1.0 },
        }
    }
}

/// Global correction deficit tracker.
///
/// Tracks `deficit = SUM(epsilon_s for un-ACKed symbols)` — the total expected
/// corrections still needed across all paths. See paper Section 13.4.
///
/// Each sent symbol adds `epsilon_i` (loss rate of its path) to the deficit.
/// Each ACKed symbol removes its send-time `epsilon_s` (confirmed survived).
/// Lost corrections add to the deficit, creating the geometric chain that
/// produces `r = epsilon / (1 - epsilon)`.
#[derive(Debug)]
pub struct CorrectionDeficit {
    /// Per-symbol tracking: (seq, path_id, epsilon_at_send)
    pending: VecDeque<(u64, PathId, f64)>,
    /// Running sum of epsilon_s for all pending symbols.
    total: f64,
}

impl CorrectionDeficit {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            total: 0.0,
        }
    }

    /// Record a symbol sent on a path with loss rate epsilon.
    pub fn on_send(&mut self, seq: u64, path_id: PathId, epsilon: f64) {
        self.pending.push_back((seq, path_id, epsilon));
        self.total += epsilon;
    }

    /// Acknowledge a symbol (confirmed received). Removes its epsilon from deficit.
    /// Returns true if the symbol was found and removed.
    pub fn on_ack(&mut self, seq: u64) -> bool {
        if let Some(pos) = self.pending.iter().position(|(s, _, _)| *s == seq) {
            let (_, _, eps) = self.pending.remove(pos).unwrap();
            self.total -= eps;
            if self.total < 0.0 {
                self.total = 0.0; // floating point guard
            }
            true
        } else {
            false
        }
    }

    /// Acknowledge all symbols up to and including `up_to_seq` (cumulative ACK).
    pub fn on_ack_cumulative(&mut self, up_to_seq: u64) {
        while self.pending.front().is_some_and(|(s, _, _)| *s <= up_to_seq) {
            let (_, _, eps) = self.pending.pop_front().unwrap();
            self.total -= eps;
        }
        if self.total < 0.0 {
            self.total = 0.0;
        }
    }

    /// Current total correction deficit.
    pub fn deficit(&self) -> f64 {
        self.total
    }

    /// Number of un-ACKed symbols being tracked.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Per-path deficit: sum of epsilon_s for un-ACKed symbols on a specific path.
    pub fn path_deficit(&self, path_id: PathId) -> f64 {
        self.pending
            .iter()
            .filter(|(_, pid, _)| *pid == path_id)
            .map(|(_, _, eps)| eps)
            .sum()
    }
}

/// Sliding window entry for bandwidth/RTT tracking.
#[derive(Clone, Debug)]
struct BwSample {
    /// Delivery rate in symbols per second.
    delivery_rate: f64,
    /// Timestamp when this sample was taken.
    timestamp: Instant,
}

#[derive(Clone, Debug)]
struct RttSample {
    rtt: Duration,
    timestamp: Instant,
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
    bw_samples: VecDeque<BwSample>,
    /// Sliding window of RTT samples.
    rtt_samples: VecDeque<RttSample>,
    /// How long to keep samples in sliding windows (10s).
    window_duration: Duration,
    /// Minimum RTT in the sliding window = estimated propagation floor.
    min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    max_bw: f64,
    /// Smoothed RTT (EWMA 7/8 old + 1/8 new) — pacing-rate denominator and
    /// cwnd-update cadence.
    srtt: Option<Duration>,
    /// Minimum RTT sample since the last cwnd update — the queuing-delay
    /// signal (windowed min, NOT an EWMA; see module docs).
    min_rtt_since_update: Option<Duration>,
    /// Consecutive-difference jitter estimate (seconds): EWMA of
    /// |rtt_i − rtt_{i−1}| at gain 1/8 (RFC 3550-style). Shift-robust by
    /// construction — a standing queue shifts ALL samples and leaves the
    /// consecutive differences at jitter scale, so this measures jitter,
    /// never queue. Widens the backoff threshold (JITTER_HEADROOM).
    jitter_est: f64,
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
    samples_since_update: u32,
    /// Window-level jitter estimate (seconds): EWMA (gain 1/4) of
    /// |win_min_i − win_min_{i−1}| between consecutive cwnd updates.
    /// Under correlated jitter the raw-sample consecutive differences
    /// collapse (~0.85ms at C2) while the window min wanders 3-5ms per
    /// update; this estimator sees that amplitude and stays shift-robust
    /// (a standing queue is ONE transition sample).
    win_jitter_est: f64,
    /// Previous update's window min (for the window-level difference).
    prev_win_min: Option<Duration>,
    /// True until the first congestion backoff (multiplicative ramp phase).
    ramping: bool,
    /// Hint-coupled queue-target multiplier (P1): 1.08/1.125/1.25.
    queue_mult: f64,
    /// When the cwnd was last updated (updates run once per SRTT).
    last_cwnd_update: Instant,
    /// Delivered symbols counter for delivery rate calculation.
    delivered: u64,
    /// Timestamp of last delivery measurement.
    last_delivered_time: Instant,
    /// Delivered count at last measurement.
    last_delivered: u64,
    /// Injectable clock for time queries.
    clock: Arc<dyn Clock>,
}

impl CopaState {
    fn new(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let now = clock.now();
        Self {
            bw_samples: VecDeque::new(),
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
            last_cwnd_update: now,
            clock,
        }
    }

    /// Record delivery of `count` symbols.  Returns the computed delivery rate.
    fn record_delivery(&mut self, count: u32) -> f64 {
        self.delivered += count as u64;
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_delivered_time).as_secs_f64();

        // Need at least 1ms of elapsed time to compute a meaningful rate
        if elapsed < 0.001 {
            return self.max_bw;
        }

        let delta_delivered = self.delivered - self.last_delivered;
        let rate = delta_delivered as f64 / elapsed;

        self.last_delivered_time = now;
        self.last_delivered = self.delivered;

        // Add to sliding window
        self.bw_samples.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
        self.expire_old_samples(now);

        // Update max bandwidth
        self.max_bw = self
            .bw_samples
            .iter()
            .map(|s| s.delivery_rate)
            .fold(0.0f64, f64::max);

        rate
    }

    /// Record an RTT sample: SRTT EWMA, 10s floor window, and the
    /// since-last-update min (queuing-delay signal).
    fn record_rtt(&mut self, rtt: Duration) {
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

        self.rtt_samples.push_back(RttSample {
            rtt,
            timestamp: now,
        });
        self.expire_old_samples(now);
        self.min_rtt = self.rtt_samples.iter().map(|s| s.rtt).min();
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
    fn queue_above_target(&self) -> bool {
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
    fn should_update(&self, now: Instant) -> bool {
        now.duration_since(self.last_cwnd_update) >= self.srtt()
    }

    /// Per-SRTT window update (gate driver semantics):
    ///   - windowed min above the queue target → backoff ×0.92, end ramp
    ///   - ramping → ×1.5 + 1
    ///   - steady state → +2
    /// Resets the queuing-delay window. Returns the new cwnd (unclamped
    /// against MIN/MAX — the caller clamps).
    fn update_cwnd(&mut self, cwnd: u32) -> u32 {
        let now = self.clock.now();
        self.last_cwnd_update = now;
        // No RTT samples since the last update → no signal, hold.
        let Some(win_min) = self.min_rtt_since_update else {
            return cwnd;
        };
        let above = self.queue_above_target();
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
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        let c = cwnd as f64;
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

    /// Immediate backoff (ramp fast-exit or decode-failure congestion):
    /// ×0.92, end the ramp, restart the update window.
    fn backoff(&mut self, cwnd: u32) -> u32 {
        self.ramping = false;
        self.min_rtt_since_update = None;
        self.samples_since_update = 0;
        self.last_cwnd_update = self.clock.now();
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
    fn copa_target_cwnd(&self) -> u32 {
        let floor = self.min_rtt.unwrap_or(DEFAULT_SRTT).as_secs_f64();
        let srtt = self.srtt().as_secs_f64();
        let dq = (srtt - floor).max(DQ_FLOOR_SECS);
        let rate = 1.0 / (COPA_DELTA * dq); // symbols per second
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
    fn bdp_anchor(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES || self.max_bw <= 0.0 {
            return None;
        }
        let rtprop = self.min_rtt?.as_secs_f64();
        Some(self.max_bw * rtprop)
    }

    /// The cwnd floor from the BtlBw anchor (symbols), or None if not yet
    /// established. A floor, NOT a cap — it only ratchets cwnd UP toward the
    /// pipe, so a stale/underestimated BtlBw cannot suppress the window
    /// (paper Section 12.6). Caller clamps against MAX_CWND.
    fn anchor_floor(&self) -> Option<u32> {
        self.bdp_anchor()
            .map(|bdp| (ANCHOR_FLOOR_GAIN * bdp).round() as u32)
    }

    /// Expire samples older than the sliding window.
    fn expire_old_samples(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        while self.bw_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_samples.pop_front();
        }
        while self.rtt_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.rtt_samples.pop_front();
        }
    }

    fn set_queue_mult(&mut self, mult: f64) {
        self.queue_mult = mult;
    }

    fn reset(&mut self) {
        let clock = self.clock.clone();
        let queue_mult = self.queue_mult;
        *self = Self::new(clock, ProtocolHint::Auto);
        self.queue_mult = queue_mult; // hint survives a path reset
    }

    /// Read the current min_rtt estimate (for diagnostics/benchmarking).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.min_rtt
    }
}

/// Per-path state tracked by the scheduler.
pub struct PathState {
    pub id: PathId,
    pub estimator: LossEstimator,
    /// Congestion window in symbols
    pub cwnd: u32,
    /// Symbols currently in flight
    pub in_flight: u32,
    /// Whether the path is considered usable
    pub active: bool,
    /// Slow-start threshold (kept for legacy test compatibility)
    pub ssthresh: u32,
    /// Whether we are in slow-start phase (Copa startup)
    pub in_slow_start: bool,
    /// Last time we received an RTCP-style report or any data from this path
    pub last_report: Instant,
    /// Maximum datagram size discovered for this path
    pub max_datagram_size: Option<usize>,
    /// Copa delay-based congestion control state.
    copa: CopaState,
    /// Token-bucket pacing: symbols sendable right now. Replenished at
    /// cwnd/SRTT symbols per second, capped at the burst allowance
    /// max(10, cwnd/8). May go NEGATIVE: the drain in net/mod.rs is
    /// batch-granular and lets the final batch overdraft; the debt is
    /// repaid before the next drain, so the average rate stays cwnd/SRTT.
    pace_tokens: f64,
    /// Last time pacing tokens were replenished.
    last_pace_refill: Instant,
    /// FIFO log of in_flight charges (charge instant, symbols) backing the
    /// time-based release in `expire_in_flight`. Invariant (best-effort):
    /// sum of counts == in_flight; direct writes to `in_flight` (tests,
    /// the leak-guard backstop) break it temporarily and all helpers
    /// saturate rather than trust it.
    in_flight_log: VecDeque<(Instant, u32)>,
    /// Injectable clock
    clock: Arc<dyn Clock>,
}

impl PathState {
    /// Minimum congestion window in symbols (never go below this).
    /// 8 rather than the historical 2: an L1 run on a real emulated link
    /// showed the old collapse-to-target dynamics crawling at 2 symbols/RTT
    /// after the first burst; the floor guarantees a usable trickle that
    /// keeps RTT samples (and thus recovery) flowing.
    pub const MIN_CWND: u32 = 8;
    /// Initial congestion window.
    pub const INITIAL_CWND: u32 = 10;
    /// Maximum congestion window.
    pub const MAX_CWND: u32 = 10_000;
}

impl PathState {
    pub fn new(id: PathId, clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(id, clock, ProtocolHint::Auto)
    }

    /// Create path state with a protocol hint (sets Copa-lite's
    /// hint-coupled queue target, paper Section 12.4 / P1).
    pub fn new_with_hint(id: PathId, clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let now = clock.now();
        Self {
            id,
            estimator: LossEstimator::new(),
            cwnd: Self::INITIAL_CWND,
            in_flight: 0,
            active: true,
            ssthresh: 64,
            in_slow_start: true,
            last_report: now,
            max_datagram_size: None,
            copa: CopaState::new(clock.clone(), hint),
            pace_tokens: Self::INITIAL_CWND as f64,
            last_pace_refill: now,
            in_flight_log: VecDeque::new(),
            clock,
        }
    }

    /// Update the hint-coupled queue target when the protocol hint changes.
    pub fn set_hint(&mut self, hint: ProtocolHint) {
        self.copa.set_queue_mult(queue_target_mult(hint));
    }

    /// Correction rate r = epsilon / (1 - epsilon).
    /// The (1-epsilon) denominator accounts for corrections-of-corrections.
    /// See paper Section 13.4.
    pub fn correction_rate(&self) -> f64 {
        let eps = self.estimator.loss_rate();
        if eps >= 1.0 {
            return f64::INFINITY;
        }
        eps / (1.0 - eps)
    }

    /// Effective delivery time E_i = RTT_i/2 + epsilon_i × t_recovery_i.
    ///
    /// t_recovery is the expected time to recover a lost symbol. We approximate
    /// it as one RTT (ARQ round-trip) weighted by loss probability. When FEC
    /// is likely to recover (low loss), t_recovery is small. When ARQ is needed
    /// (high loss or aged symbol), t_recovery approaches one full RTT.
    ///
    /// See paper Section 13.5.
    pub fn effective_delivery_time(&self) -> f64 {
        let rtt_secs = self.estimator.rtt().as_secs_f64();
        let eps = self.estimator.loss_rate();
        // t_recovery ≈ RTT (one round-trip for ARQ recovery)
        let t_recovery = rtt_secs;
        rtt_secs / 2.0 + eps * t_recovery
    }

    /// Load-DEPENDENT expected frontier-completion-time `E_i(load)` (seconds) —
    /// the always-on load term of the RWM placement law (paper Section 16.3).
    /// The time a symbol handed to this path now takes to reach the receiver:
    ///
    ///   E_i(load) = in_flight_i / (cwnd_i/SRTT_i)   ← drain the current backlog
    ///             + SRTT_i / 2                        ← one-way propagation
    ///             + eps_i · RTT_i                     ← expected loss recovery
    ///
    /// The queue term uses the path's live PACING RATE (`cwnd/SRTT`), so a
    /// backlog on a low-capacity / high-RTT path costs proportionally MORE real
    /// time than the same backlog on the fast path — this is what makes the law
    /// water-fill by CAPACITY (arrival rate matches drain rate at equilibrium),
    /// not by equal window-fraction (which over-loads the slow path and, on a
    /// reliable in-order stream, collapses the frontier — MEASURED at C8:
    /// dimensionless fill gave 3.4 Mbit/s vs 15.4 fast-path-alone). It rises
    /// CONTINUOUSLY with `in_flight` (past cwnd under overdraft), so spillover
    /// is a smooth equilibrium, not a regime switch. Because it is the delivery
    /// latency of a reliable in-order stream (the completion cost itself), it
    /// carries UNIT weight independent of the protocol hint.
    pub fn expected_delivery_load(&self) -> f64 {
        let srtt = self.srtt().as_secs_f64();
        let eps = self.estimator.loss_rate();
        let cwnd = self.cwnd.max(1) as f64;
        let queue_wait = (self.in_flight as f64 / cwnd) * srtt;
        queue_wait + srtt / 2.0 + eps * srtt
    }

    /// Source-carrying capacity: B_eff = throughput / (1 + r).
    /// See paper Section 13.5.
    pub fn effective_bandwidth(&self) -> f64 {
        let throughput = self.estimator.throughput();
        let r = self.correction_rate();
        if r.is_infinite() {
            return 0.0;
        }
        throughput / (1.0 + r)
    }

    /// Effective goodput: throughput * (1 - loss_rate).
    /// This is what actually gets through to the receiver.
    pub fn effective_goodput(&self) -> f64 {
        let throughput = self.estimator.throughput();
        let loss = self.estimator.loss_rate();
        throughput * (1.0 - loss)
    }

    /// Available capacity: cwnd - in_flight.
    pub fn available(&self) -> u32 {
        self.cwnd.saturating_sub(self.in_flight)
    }

    /// Spare capacity as a fraction of in-flight traffic.
    ///
    /// Returns `(cwnd - in_flight) / in_flight` when in_flight > 0.
    /// Used by the FEC rate controller to ensure repairs don't exceed
    /// available link capacity (the "never hurts" guarantee).
    ///
    /// Returns f64::INFINITY when in_flight is 0 (unlimited spare capacity).
    pub fn spare_capacity(&self) -> f64 {
        if self.in_flight == 0 {
            return f64::INFINITY;
        }
        self.cwnd.saturating_sub(self.in_flight) as f64 / self.in_flight as f64
    }

    /// Copa-lite congestion control: handle acknowledgements.
    ///
    /// The cwnd update runs once per SRTT (gate driver cadence):
    ///   - windowed-min RTT above the queue target → ×0.92, end ramp
    ///   - ramping (before the first backoff) → ×1.5 + 1
    ///   - steady state → +2
    ///
    /// During the ramp the backoff check additionally runs per ACK, so the
    /// exponential phase ends within one feedback message of the first
    /// standing-queue evidence rather than waiting out the SRTT window.
    pub fn on_ack(&mut self, acked: u32) {
        let _rate = self.copa.record_delivery(acked);
        let now = self.clock.now();

        if self.copa.ramping
            && self.copa.samples_since_update >= 3
            && self.copa.queue_above_target()
        {
            // Fast ramp exit: gentle ×0.92, NOT a collapse to a
            // rate-formula target (the pre-P7 bug: the initial burst
            // inflated its own RTT samples, dq exploded, and the target
            // dropped to the floor on the very first burst). Requires
            // ≥3 samples of evidence: a partial window's min can be a
            // single jittery sample, and one draw from the jitter tail
            // must not end the exponential ramp (L1 C2 finding).
            self.cwnd = self.copa.backoff(self.cwnd);
        } else if self.copa.should_update(now) {
            self.cwnd = self.copa.update_cwnd(self.cwnd);
        }

        self.clamp_cwnd_with_anchor();

        // Sync legacy fields
        self.in_slow_start = self.copa.ramping;
        if !self.in_slow_start && self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// Copa-lite congestion control: handle loss events.
    ///
    /// Loss alone does NOT reduce cwnd — channel loss is FEC's job, not
    /// CC's (paper Section 12). The key insight:
    ///   - Loss + FEC recovered → wireless/random loss → ignore entirely
    ///   - Decode failure + standing queue above target → real congestion
    ///     → backoff ×0.92 (same speed as the delay backoff; a decode
    ///     failure adds no extra information beyond the delay signal)
    ///   - Decode failure + empty queue → borderline FEC under-provision,
    ///     not congestion → end the ramp and step down by 1
    pub fn on_loss(&mut self, fec_recovered: bool) {
        if fec_recovered {
            return;
        }
        if self.copa.queue_above_target() {
            self.cwnd = self.copa.backoff(self.cwnd);
        } else {
            self.copa.ramping = false;
            self.cwnd = self.cwnd.saturating_sub(1);
        }
        self.clamp_cwnd_with_anchor();
        self.in_slow_start = false;
        if self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// Feed an RTT measurement into Copa state.
    /// Call this when processing ACKs/reports that include RTT.
    pub fn record_rtt_sample(&mut self, rtt: Duration) {
        self.copa.record_rtt(rtt);
    }

    /// Read Copa's current min_rtt estimate (for diagnostics/benchmarking).
    pub fn copa_min_rtt(&self) -> Option<Duration> {
        self.copa.min_rtt()
    }

    /// Smoothed RTT estimate (Copa's EWMA; the loss estimator's EWMA as a
    /// fallback before the first Copa sample).
    pub fn srtt(&self) -> Duration {
        match self.copa.srtt {
            Some(s) => s,
            None => self.estimator.rtt(),
        }
    }

    /// Classic Copa equilibrium target, for diagnostics (see
    /// `CopaState::copa_target_cwnd` for the units derivation).
    pub fn copa_target_cwnd(&self) -> u32 {
        self.copa.copa_target_cwnd()
    }

    /// BtlBw×RTprop BDP anchor estimate in symbols, once established
    /// (paper Section 12.6). None during warm-up / before a min-RTT
    /// sample. Diagnostic/benchmarking accessor.
    pub fn copa_bdp_anchor(&self) -> Option<f64> {
        self.copa.bdp_anchor()
    }

    /// Clamp cwnd to [MIN_CWND, MAX_CWND] and then raise it to the BtlBw
    /// anchor floor if one is established (paper Section 12.6). The floor
    /// only ratchets cwnd UP (never a cap) and is itself bounded by
    /// MAX_CWND, so an over-read BtlBw cannot exceed the hard ceiling.
    fn clamp_cwnd_with_anchor(&mut self) {
        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);
        if let Some(floor) = self.copa.anchor_floor() {
            self.cwnd = self.cwnd.max(floor.min(Self::MAX_CWND));
        }
    }

    // --- Token-bucket pacing (paper Section 12.5, gate driver P1) ---
    //
    // UNITS: tokens are SYMBOLS. Refill rate = cwnd [symbols] / SRTT [s]
    // = symbols/second; burst allowance = max(10, cwnd/8) symbols.

    /// Replenish pacing tokens for elapsed wall time.
    pub fn pace_refill(&mut self) {
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_pace_refill).as_secs_f64();
        self.last_pace_refill = now;
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = self.cwnd as f64 / srtt; // symbols per second
        let burst = (self.cwnd as f64 / 8.0).max(10.0);
        self.pace_tokens = (self.pace_tokens + rate * elapsed).min(burst);
    }

    /// Current pacing token balance (symbols; may be negative — see field).
    pub fn pace_tokens(&self) -> f64 {
        self.pace_tokens
    }

    /// Consume tokens for `n` symbols just sent (may push balance negative).
    pub fn consume_pace_tokens(&mut self, n: u32) {
        self.pace_tokens -= n as f64;
    }

    /// Time until at least one pacing token is available at the current
    /// refill rate (zero if a token is already available).
    pub fn pace_delay(&self) -> Duration {
        if self.pace_tokens >= 1.0 {
            return Duration::ZERO;
        }
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = (self.cwnd as f64 / srtt).max(1.0); // symbols per second
        Duration::from_secs_f64((1.0 - self.pace_tokens) / rate)
    }

    // --- in_flight budget accounting (P7 follow-up 2) ---
    //
    // in_flight is a BUDGET GAUGE (symbols committed: interleaver + pacing
    // carry + wire), charged exactly once per symbol at SCHEDULE time and
    // released by ACK feedback. ACKs are best-effort datagrams: a lost ACK
    // strands its release forever, and stranded budget compounds until the
    // TUN gate jams (L1 finding: the gate cycled at the 2s leak-guard
    // cadence instead of the RTT). The FIFO charge log makes releases
    // robust: budget older than max(4×SRTT, 250ms) is delivered-or-lost
    // either way (RFC 9002-style time-threshold, at gauge granularity) and
    // expires. Pacing (cwnd/SRTT tokens) remains the actual rate limiter,
    // so an early expiry can only let the encoder run ahead, never the
    // wire.

    /// Charge `n` symbols against the in_flight budget (at schedule time).
    pub fn charge_in_flight(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        self.in_flight = self.in_flight.saturating_add(n);
        self.in_flight_log.push_back((self.clock.now(), n));
    }

    /// Release `n` symbols of budget (ACK feedback: received or
    /// gap-inferred lost). Pops the OLDEST charges first.
    pub fn release_in_flight(&mut self, n: u32) {
        self.in_flight = self.in_flight.saturating_sub(n);
        let mut remaining = n;
        while remaining > 0 {
            match self.in_flight_log.front_mut() {
                Some((_, c)) if *c > remaining => {
                    *c -= remaining;
                    remaining = 0;
                }
                Some((_, c)) => {
                    remaining -= *c;
                    self.in_flight_log.pop_front();
                }
                None => break,
            }
        }
    }

    /// Expire budget charged longer than max(4×SRTT, 250ms) ago: its ACK
    /// (or the loss evidence) would have arrived by now — the datagram was
    /// delivered with the ACK lost, or lost with no later batch to reveal
    /// the gap. Either way it is no longer on the wire.
    pub fn expire_in_flight(&mut self) {
        if self.in_flight_log.is_empty() {
            return;
        }
        let horizon = (self.srtt() * 4).max(IN_FLIGHT_EXPIRY_MIN);
        let now = self.clock.now();
        while let Some(&(t, c)) = self.in_flight_log.front() {
            if now.duration_since(t) < horizon {
                break;
            }
            self.in_flight = self.in_flight.saturating_sub(c);
            self.in_flight_log.pop_front();
        }
    }
}

/// The multipath scheduler.
///
/// Uses the interpolated objective function from paper Section 13.8:
///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
/// where E_i is effective delivery time and r_i is correction rate per path.
///
/// Source placement is BLOCK-granular (paper Section 13.8 in-order coupling
/// refinement, L2 ws1): one schedule() call = one FEC block = one delivery
/// unit, and under the cross-block in-order delivery contract a block's
/// delivery time is the MAX over the paths its source symbols touch — the
/// linear per-symbol objective silently assumed independent delivery.
/// Measured at L1 C8 (100mbit/10ms + 20mbit/40ms): blocks striped across
/// both paths completed at mean 189 ms vs 17.5 ms for fast-path-only blocks,
/// and 92% of in-order head-of-line waits were caused by blocks touching the
/// slow path. Whole-block affinity bounds the damage to the y_i fraction of
/// blocks actually assigned to the slow path (smooth WRR on B_eff_i).
pub struct Scheduler {
    paths: HashMap<PathId, PathState>,
    clock: Arc<dyn Clock>,
    /// Global correction deficit tracker (paper Section 13.4).
    pub deficit: CorrectionDeficit,
    /// Scheduling weights from protocol hint.
    weights: SchedulingWeights,
    /// Protocol hint — also sets Copa-lite's queue target on each path
    /// (paper Section 12.4 / P1).
    hint: ProtocolHint,
    /// Block-granular source affinity (see struct docs). On by default;
    /// `false` restores per-symbol greedy striping (ablation).
    block_affinity: bool,
    /// Smooth-WRR credit per path for the block-affinity pick.
    affinity_credit: HashMap<PathId, f64>,
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(clock, ProtocolHint::Auto)
    }

    /// Create scheduler with protocol hint for weight configuration and
    /// the per-path Copa-lite queue target.
    pub fn new_with_hint(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(hint),
            hint,
            block_affinity: true,
            affinity_credit: HashMap::new(),
        }
    }

    /// Enable/disable block-granular source affinity (ablation switch;
    /// `false` = legacy per-symbol greedy striping).
    pub fn set_block_affinity(&mut self, enabled: bool) {
        self.block_affinity = enabled;
    }

    /// Update scheduling weights (e.g., when protocol hint changes).
    pub fn set_weights(&mut self, weights: SchedulingWeights) {
        self.weights = weights;
    }

    /// Current scheduling weights.
    pub fn weights(&self) -> SchedulingWeights {
        self.weights
    }

    pub fn add_path(&mut self, id: PathId) {
        self.paths
            .insert(id, PathState::new_with_hint(id, self.clock.clone(), self.hint));
    }

    pub fn remove_path(&mut self, id: PathId) {
        self.paths.remove(&id);
    }

    pub fn path_mut(&mut self, id: PathId) -> Option<&mut PathState> {
        self.paths.get_mut(&id)
    }

    pub fn path(&self, id: PathId) -> Option<&PathState> {
        self.paths.get(&id)
    }

    pub fn active_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active && p.available() > 0)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Paths that are up, regardless of remaining cwnd budget.
    ///
    /// Use for CONTROL-PLANE traffic (reports, pings, BlockStart) and
    /// congestion bookkeeping. `active_paths()` filters by spare capacity
    /// (for scheduling DATA) — using it for liveness made a saturated path
    /// invisible: no pings were sent while in_flight >= cwnd, so the peer
    /// declared the path dead mid-transfer (L1 harness finding).
    pub fn live_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Schedule symbols across paths using the interpolated objective.
    ///
    /// Objective (paper Section 13.8):
    ///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
    ///
    /// Source symbols go to paths with lowest weighted cost.
    /// Repair symbols go to paths with highest effective goodput (maximize decode probability).
    ///
    /// Returns: Vec<(PathId, Vec<WireSymbol>)>
    pub fn schedule(
        &mut self,
        source_symbols: Vec<WireSymbol>,
        repair_symbols: Vec<WireSymbol>,
    ) -> Vec<(PathId, Vec<WireSymbol>)> {
        let mut assignments: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        let active_paths: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();

        if active_paths.is_empty() {
            return vec![];
        }

        // Compute per-path cost for source scheduling using interpolated objective.
        // cost_i = w_lat × E_i + w_bw × r_i
        // Lower cost = better path for source symbols.
        let mut path_costs: Vec<(PathId, f64, u32)> = active_paths
            .iter()
            .map(|p| {
                let e_i = p.effective_delivery_time();
                let r_i = p.correction_rate();
                let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
                let cost = self.weights.w_lat * e_i + self.weights.w_bw * r_clamped;
                (p.id, cost, p.available())
            })
            .collect();
        path_costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Distribute source symbols.
        //
        // Block-granular affinity (default; see struct docs): one call =
        // one block = one delivery unit — ALL source symbols ride one
        // path, picked by smooth WRR on source-carrying capacity, so a
        // block's completion time is a single path's delivery time rather
        // than the max over every path touched. The pick may exceed the
        // path's remaining cwnd budget: in_flight is charged anyway and
        // the aggregate TUN gate + token-bucket pacing provide the
        // backpressure (same contract as the old overflow-to-best-path).
        if self.block_affinity && !source_symbols.is_empty() {
            let k = source_symbols.len();
            if let Some(pid) = self.pick_affinity_path(k) {
                assignments.entry(pid).or_default().extend(source_symbols);
            }
        } else {
            // Legacy per-symbol striping: lowest-cost paths first, up to
            // each path's spare cwnd budget (ablation mode).
            let mut source_iter = source_symbols.into_iter();
            for &(pid, _, avail) in &path_costs {
                let batch: Vec<_> = source_iter.by_ref().take(avail as usize).collect();
                if batch.is_empty() {
                    break;
                }
                assignments.entry(pid).or_default().extend(batch);
            }
            // Overflow to best path
            for sym in source_iter {
                if let Some(&(pid, _, _)) = path_costs.first() {
                    assignments.entry(pid).or_default().push(sym);
                }
            }
        }

        // Repair symbols: distribute proportional to effective goodput
        let mut paths_by_goodput: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active)
            .collect();
        paths_by_goodput.sort_by(|a, b| {
            b.effective_goodput()
                .partial_cmp(&a.effective_goodput())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if !paths_by_goodput.is_empty() {
            let total_goodput: f64 = paths_by_goodput.iter().map(|p| p.effective_goodput()).sum();
            let mut repair_iter = repair_symbols.into_iter().peekable();

            if total_goodput > 0.0 {
                for path in &paths_by_goodput {
                    let fraction = path.effective_goodput() / total_goodput;
                    let count = (fraction * repair_iter.len() as f64).ceil() as usize;
                    let batch: Vec<_> = repair_iter.by_ref().take(count).collect();
                    if !batch.is_empty() {
                        assignments.entry(path.id).or_default().extend(batch);
                    }
                }
            }
            // Remaining repair symbols to best goodput path
            for sym in repair_iter {
                if let Some(path) = paths_by_goodput.first() {
                    assignments.entry(path.id).or_default().push(sym);
                }
            }
        }

        // Charge the in_flight budget at SCHEDULE time — the single charge
        // point for block-mode symbols (the paced drain in net/mod.rs must
        // NOT charge again at send time; double-charging leaked +1 per
        // symbol and jammed the TUN gate — L1 finding, P7 follow-up 2).
        for (path_id, syms) in &assignments {
            if let Some(path) = self.paths.get_mut(path_id) {
                path.charge_in_flight(syms.len() as u32);
            }
        }

        assignments.into_iter().collect()
    }

    /// Pick the path for a whole block's source symbols — the block-granular
    /// solution of the Section 13.8 objective (in-order coupling refinement):
    ///
    ///   - w_lat > 0 (Realtime/Auto): the LP solution is degenerate — the
    ///     minimum interpolated-cost path carries blocks until its cwnd
    ///     budget is exhausted, then spills to the next-cheapest (block-
    ///     granular spill; per-symbol spill is what striped blocks across
    ///     paths and made every block pay max_i D_i).
    ///   - w_lat == 0 (Bulk): demand saturates capacity, so the optimum is
    ///     y_i ∝ B_eff_i (Section 13.5, with C_i = the live Copa pacing
    ///     rate cwnd/SRTT — always defined, unlike the delivery-rate EWMA
    ///     which is cold at startup), realized by smooth WRR so consecutive
    ///     blocks alternate as evenly as the weights allow (minimal
    ///     in-order skew). Paths whose delivery time exceeds the fastest
    ///     path's by more than the in-order hold horizon are source-
    ///     ineligible (their blocks would be force-delivered as holes);
    ///     they keep serving corrections/retransmits.
    ///
    /// Paths with exhausted cwnd budget are skipped while any path has
    /// budget (WRR credit keeps accruing, so a briefly-full path gets its
    /// share back later); if ALL budgets are exhausted the pick falls back
    /// to every active path (the TUN gate is the real backpressure —
    /// schedule() must never drop a block).
    fn pick_affinity_path(&mut self, block_symbols: usize) -> Option<PathId> {
        /// In-order hold horizon (mirrors BLOCK_REORDER_MAX_HOLD in
        /// net/mod.rs): a block delivered later than this past its
        /// predecessors expires the receiver hold and surfaces as an
        /// inner-stream hole.
        const HOLD_HORIZON_SECS: f64 = 0.3;
        /// Source-eligibility threshold as a fraction of the horizon.
        /// Eligibility must gate on the block-delivery TAIL (an expiry is
        /// a tail event), but the estimate below is a median-ish model;
        /// ARQ rounds stack the tail to ~3-4x the median (measured C8:
        /// median 134 ms, expiries at 301+ ms), so a median skew above
        /// H/4 already pushes the tail past the horizon.
        const ELIGIBLE_SKEW: f64 = HOLD_HORIZON_SECS / 4.0;

        /// Expected delivery time of a WHOLE block of `k` source symbols
        /// on this path (paper 13.8 refinement, D_i): serialization at
        /// the Copa pacing rate + one-way propagation + an ARQ round at
        /// THIS path's RTT weighted by the per-BLOCK loss probability
        /// 1-(1-eps)^k. The per-symbol E_i (Section 13.5) undercounts by
        /// ~an order of magnitude here: k*eps expected losses make a
        /// recovery round nearly certain for realistic k (measured C8:
        /// eps=4.8%, k=56 -> P_blk = 0.94; B-blocks p50 94 ms vs
        /// E_B = 22 ms).
        fn block_delivery_time(p: &PathState, k: f64) -> f64 {
            let srtt = p.srtt().as_secs_f64().max(1e-3);
            let rate = (p.cwnd as f64 / srtt).max(1.0); // symbols/sec
            // Long-run loss, not the instantaneous EWMA: under GE bursts
            // the EWMA decays to ~0 between bursts and flip-flops the
            // eligibility gate open exactly long enough for the next
            // burst to catch a freshly admitted block (measured C8: B
            // still carried 12% of source, mixed-block p99 1.0 s). The
            // Beta-posterior mean spans bursts and gaps alike.
            let eps = p
                .estimator
                .loss_rate()
                .max(p.estimator.loss_rate_mean())
                .clamp(0.0, 0.99);
            let p_blk = 1.0 - (1.0 - eps).powf(k);
            k / rate + srtt / 2.0 + p_blk * 2.0 * srtt
        }

        let with_budget: Vec<&PathState> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();
        let cands: Vec<&PathState> = if with_budget.is_empty() {
            self.paths.values().filter(|p| p.active).collect()
        } else {
            with_budget
        };
        if cands.is_empty() {
            return None;
        }

        if self.weights.w_lat > 0.0 {
            // Latency-weighted: min interpolated cost, deterministic
            // tie-break by id.
            return cands
                .iter()
                .min_by(|a, b| {
                    let ca = self.path_cost(a);
                    let cb = self.path_cost(b);
                    ca.partial_cmp(&cb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.id.cmp(&b.id))
                })
                .map(|p| p.id);
        }

        // Bulk: capacity-share WRR over hold-feasible paths (HOL-cost
        // source eligibility: a path whose per-block delivery skew
        // threatens the in-order hold horizon carries NO source — it
        // keeps its repair/retransmit role, which has no ordering
        // deadline and keeps its estimators warm for re-admission).
        //
        // Eligibility is computed over ALL active paths, not just the
        // budget-filtered candidates: when the fast path's cwnd is
        // momentarily full, the slow path used to become the only
        // candidate and pass the skew test against itself (measured C8:
        // B still carried 12% of source through exactly this hole). An
        // ineligible path must not carry source even then — the pick
        // over-commits the eligible path instead (pacing keeps the wire
        // rate at cwnd/SRTT; the aggregate TUN gate closes as the
        // over-commit accumulates).
        let k = (block_symbols as f64).max(1.0);
        let active: Vec<&PathState> = self.paths.values().filter(|p| p.active).collect();
        let d_min = active
            .iter()
            .map(|p| block_delivery_time(p, k))
            .fold(f64::INFINITY, f64::min);
        let eligible: Vec<&&PathState> = active
            .iter()
            .filter(|p| block_delivery_time(p, k) - d_min <= ELIGIBLE_SKEW)
            .collect();
        let cands: Vec<&&PathState> = {
            let with_budget: Vec<&&PathState> = eligible
                .iter()
                .copied()
                .filter(|p| p.available() > 0)
                .collect();
            if with_budget.is_empty() { eligible } else { with_budget }
        };
        let mut weighted: Vec<(PathId, f64)> = cands
            .iter()
            .map(|p| {
                let srtt = p.srtt().as_secs_f64().max(1e-3);
                let rate = p.cwnd as f64 / srtt; // symbols/sec (Copa pacing rate)
                let r = p.correction_rate();
                let r = if r.is_infinite() { 10.0 } else { r };
                (p.id, rate / (1.0 + r)) // B_eff (Section 13.5)
            })
            .collect();
        weighted.sort_unstable_by(|a, b| a.0.cmp(&b.0)); // deterministic order
        let total: f64 = weighted.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            return weighted.first().map(|&(id, _)| id);
        }
        // Drop credit for removed paths so a re-added id starts fresh.
        let paths = &self.paths;
        self.affinity_credit.retain(|id, _| paths.contains_key(id));
        let mut pick: Option<(PathId, f64)> = None;
        for &(id, w) in &weighted {
            let credit = self.affinity_credit.entry(id).or_insert(0.0);
            *credit += w / total;
            if pick.is_none() || *credit > pick.unwrap().1 {
                pick = Some((id, *credit));
            }
        }
        let (id, _) = pick?;
        *self.affinity_credit.get_mut(&id).unwrap() -= 1.0;
        Some(id)
    }

    /// Acknowledge received symbols on a path.
    pub fn ack(&mut self, path_id: PathId, count: u32) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.release_in_flight(count);
            path.on_ack(count);
        }
    }

    /// Notify the scheduler of a loss event on a path.
    ///
    /// `fec_recovered`: true if the FEC decoder recovered the block despite
    /// the loss (random/wireless loss), false if the block failed to decode
    /// (congestion signal).
    pub fn on_loss(&mut self, path_id: PathId, fec_recovered: bool) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.on_loss(fec_recovered);
        }
    }

    /// Record that we received a report/data from a path (keepalive).
    pub fn touch_path(&mut self, path_id: PathId) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.last_report = self.clock.now();
            if !path.active {
                tracing::info!(path_id, "path recovered — marking active");
                path.active = true;
                // Reset to startup on recovery (Copa reset keeps the hint's
                // queue target; pacing restarts at the initial burst; the
                // dead path's in-flight budget is gone with it).
                path.cwnd = PathState::INITIAL_CWND;
                path.ssthresh = 64;
                path.in_slow_start = true;
                path.copa.reset();
                path.pace_tokens = PathState::INITIAL_CWND as f64;
                path.last_pace_refill = path.last_report;
                path.in_flight = 0;
                path.in_flight_log.clear();
            }
        }
    }

    /// Check all paths for staleness and deactivate dead ones.
    /// Returns list of path IDs that were deactivated.
    pub fn check_dead_paths(&mut self, timeout: Duration) -> Vec<PathId> {
        let now = self.clock.now();
        let mut deactivated = vec![];
        for path in self.paths.values_mut() {
            if path.active && now.duration_since(path.last_report) > timeout {
                tracing::warn!(path_id = path.id, "path timed out — marking inactive");
                path.active = false;
                deactivated.push(path.id);
            }
        }
        deactivated
    }

    /// Get all path IDs (including inactive).
    pub fn all_path_ids(&self) -> Vec<PathId> {
        self.paths.keys().copied().collect()
    }

    /// Pick the best path for a source symbol: lowest interpolated cost.
    ///
    /// cost_i = w_lat × E_i + w_bw × r_i (paper Section 13.8)
    pub fn best_source_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Compute the interpolated scheduling cost for a path.
    fn path_cost(&self, path: &PathState) -> f64 {
        let e_i = path.effective_delivery_time();
        let r_i = path.correction_rate();
        let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
        self.weights.w_lat * e_i + self.weights.w_bw * r_clamped
    }

    /// Pick the best path for a repair symbol: highest goodput with available capacity.
    pub fn best_repair_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Pick the best repair path, preferring to avoid `avoid` for cross-path diversity.
    /// Falls back to `best_repair_path()` if no alternative exists.
    pub fn best_repair_path_avoiding(&self, avoid: PathId) -> Option<PathId> {
        let alt = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != avoid)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id);
        alt.or_else(|| self.best_repair_path())
    }

    /// RWM per-symbol placement law (paper Section 16.3) — the ONE continuous
    /// marginal-cost rule that stripes source AND repair symbols across paths
    /// with no load regimes and no case splits. Replaces the single-path
    /// `best_source_path` / `best_repair_path` pair for the reliable window
    /// pipeline.
    ///
    /// For each active path `i`:
    ///
    ///   cost_i = Ê_i(load) / ref_srtt            ← frontier-completion-time
    ///          + w_bw · r_i                       ← correction/bandwidth burden
    ///          + w_div · ρ_fate(s, i)             ← repair diversity
    ///   P(i) ∝ exp(−cost_i / T)
    ///
    /// The paper (§16.3) writes `w_lat·E_i(load) + w_bw·r_i + w_div·ρ_fate`. Two
    /// implementation choices make it work for a reliable in-order stream:
    ///
    /// (1) `E_i(load)` is the expected frontier-completion-TIME
    ///     (`expected_delivery_load`): queue drain at the path's PACING RATE
    ///     `cwnd/SRTT`, plus propagation, plus loss recovery. Being in time, it
    ///     is capacity-aware — a backlog on the slow path costs more real time —
    ///     so the law water-fills by CAPACITY. A dimensionless `in_flight/cwnd`
    ///     fill instead fills both paths to equal FRACTION, over-loading the
    ///     low-capacity path; on an in-order stream that collapses the frontier
    ///     (MEASURED C8: 3.4 Mbit/s vs 15.4 fast-path-alone).
    ///
    /// (2) `E_i(load)` carries UNIT weight, not `w_lat`. The paper's `w_lat ≈ 0`
    ///     for Bulk is a lossy-throughput heuristic; on a RELIABLE in-order
    ///     stream latency-to-frontier is the completion cost itself, so it is
    ///     always weighted. `w_bw` still adds the wire-waste (loss) penalty that
    ///     is the Bulk-vs-Realtime dial. This also satisfies §16.3's requirement
    ///     that the queue signal drive water-filling ("token availability IS the
    ///     marginal-cost signal") — the queue term is never gated away.
    ///
    /// Terms:
    ///   - `Ê_i(load)/ref_srtt`: de-dimensionalised by the fastest path's SRTT,
    ///     O(1) and comparable across heterogeneous RTTs; rises continuously
    ///     with `in_flight`, equalised across paths at the water-filling point.
    ///   - `r_i`: correction rate / loss burden (clamped for dead paths).
    ///   - `ρ_fate(s,i)`: REPAIR symbols only — the fraction of the symbols this
    ///     repair covers that path `i` already carried (a repair riding its own
    ///     coverage adds no diversity). `covered_paths` holds one entry per
    ///     covered source symbol (with multiplicity); the continuous form of
    ///     `best_repair_path_avoiding`. Zero for source symbols.
    ///
    /// Temperature `T = PLACE_TEMPERATURE` is the one dial from strict best-path
    /// (T → 0 ⇒ argmin) to dithering. Single path ⇒ that path always (byte-
    /// identical to the pre-RWM single-path sender).
    ///
    /// Returns the sampled `PathId`, or `None` if no path is up at all.
    pub fn place_symbol(&self, is_repair: bool, covered_paths: &[PathId]) -> Option<PathId> {
        let probs = self.place_probs(is_repair, covered_paths);
        if probs.is_empty() {
            return None;
        }
        let u: f64 = rand::random();
        let mut acc = 0.0;
        for (pid, p) in &probs {
            acc += p;
            if u <= acc {
                return Some(*pid);
            }
        }
        // Floating-point slack: fall through to the last candidate.
        probs.last().map(|(pid, _)| *pid)
    }

    /// The softmax placement distribution over paths (paper §16.3). Exposed for
    /// unit-testing the placement law (concentration, continuous spillover,
    /// water-filling, fate steering, T → 0 argmin) without sampling noise.
    /// Returns `(PathId, probability)` summing to 1 over the candidate set.
    pub fn place_probs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        self.place_probs_with_temperature(is_repair, covered_paths, place_temperature())
    }

    /// `place_probs` with an explicit temperature — the T dial exposed for
    /// tests (T → 0 ⇒ argmin, the no-cutoffs strict-best-path limit).
    pub fn place_probs_with_temperature(
        &self,
        is_repair: bool,
        covered_paths: &[PathId],
        temperature: f64,
    ) -> Vec<(PathId, f64)> {
        let costs = self.place_costs(is_repair, covered_paths);
        if costs.is_empty() {
            return vec![];
        }
        // The costs from `place_costs` are already dimensionless (the latency
        // term is normalised by the fastest SRTT), so the temperature is a pure
        // dimensionless dial. Shift by the min cost for numerical stability
        // (softmax is shift-invariant).
        let t_eff = temperature.max(f64::MIN_POSITIVE);
        let min_cost = costs
            .iter()
            .map(|(_, c)| *c)
            .fold(f64::INFINITY, f64::min);
        let mut weights: Vec<(PathId, f64)> = costs
            .iter()
            .map(|(pid, c)| (*pid, (-(c - min_cost) / t_eff).exp()))
            .collect();
        let z: f64 = weights.iter().map(|(_, w)| w).sum();
        if z <= 0.0 || !z.is_finite() {
            // Degenerate (T → 0 with ties, or overflow): argmin gets all mass.
            let arg = costs
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(pid, _)| *pid);
            return costs
                .iter()
                .map(|(pid, _)| (*pid, if Some(*pid) == arg { 1.0 } else { 0.0 }))
                .collect();
        }
        for (_, w) in &mut weights {
            *w /= z;
        }
        weights
    }

    /// Per-path marginal placement cost (paper §16.3), over ALL active paths.
    ///
    /// We deliberately do NOT hard-filter on spare capacity. The paper phrases
    /// a full path as "skipped (∞ cost)", but its own no-cutoffs convention
    /// binds mechanisms ("no control law may case-split"), and a hard filter
    /// would make a path vanish discontinuously at `in_flight == cwnd` — the
    /// exact threshold jump the monotonic-spillover requirement forbids. The
    /// `in_flight/cwnd` congestion term IS the continuous form: it climbs past
    /// 1.0 under overdraft, driving a saturated path's softmax mass toward zero
    /// smoothly without ever removing it, so placement never drops a symbol
    /// (the send loop's pacing/backpressure remains the real capacity gate).
    fn place_costs(&self, is_repair: bool, covered_paths: &[PathId]) -> Vec<(PathId, f64)> {
        let ref_srtt = self
            .paths
            .values()
            .filter(|p| p.active)
            .map(|p| p.srtt().as_secs_f64().max(PLACE_REF_FLOOR_SECS))
            .fold(f64::INFINITY, f64::min);
        let ref_srtt = if ref_srtt.is_finite() {
            ref_srtt
        } else {
            PLACE_REF_FLOOR_SECS
        };

        let w_bw = self.weights.w_bw;
        let w_div = self.weights.w_div;

        let covered_total = covered_paths.len() as f64;

        let cost_of = |p: &PathState| -> f64 {
            // Frontier-completion-time — the always-on load term (unit weight),
            // de-dimensionalised by the fastest SRTT so it is O(1). This single
            // term carries BOTH the congestion signal (queue drain at the pacing
            // rate) and the propagation preference; because it is expressed in
            // TIME it is capacity-aware, so it water-fills by capacity rather
            // than over-loading the slow path.
            let load = p.expected_delivery_load() / ref_srtt;
            // Bandwidth/correction burden (loss/wire waste); the hint's w_bw
            // dial. w_lat does NOT gate placement: on a reliable in-order stream
            // latency-to-frontier is the completion cost itself, already carried
            // by `load` at unit weight, not a per-hint preference.
            let r = p.correction_rate();
            let r = if r.is_infinite() { 10.0 } else { r };
            // Fate diversity (repairs only): fraction of covered symbols on p.
            let fate = if is_repair && covered_total > 0.0 {
                covered_paths.iter().filter(|&&c| c == p.id).count() as f64 / covered_total
            } else {
                0.0
            };
            load + w_bw * r + w_div * fate
        };

        self.paths
            .values()
            .filter(|p| p.active)
            .map(|p| (p.id, cost_of(p)))
            .collect()
    }

    /// Pick a secondary path for redundant source scheduling (different from primary).
    /// Returns None if only one usable path is available.
    pub fn redundant_source_path(&self, primary: PathId) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != primary)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Aggregate spare capacity across all active paths.
    ///
    /// Returns the minimum spare_capacity fraction across active paths,
    /// representing the tightest bottleneck. Used to cap FEC repair rate.
    pub fn spare_capacity(&self) -> f64 {
        self.paths
            .values()
            .filter(|p| p.active)
            .map(|p| p.spare_capacity())
            .fold(f64::INFINITY, f64::min)
    }

    /// Get the minimum max_datagram_size across all active paths that have
    /// reported an MTU. Returns None if no active path has a known MTU.
    pub fn min_mtu(&self) -> Option<usize> {
        self.paths
            .values()
            .filter(|p| p.active)
            .filter_map(|p| p.max_datagram_size)
            .min()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(Arc::new(WallClock))
    }
}

impl Scheduler {
    /// Set protocol hint (updates scheduling weights and each path's
    /// Copa-lite queue target).
    pub fn set_protocol_hint(&mut self, hint: ProtocolHint) {
        self.weights = SchedulingWeights::from_hint(hint);
        self.hint = hint;
        for path in self.paths.values_mut() {
            path.set_hint(hint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(id: u32, repair: bool) -> WireSymbol {
        WireSymbol {
            block_id: 0,
            payload_id: id,
            is_repair: repair,
            data: vec![0u8; 64],
            backend: FecBackend::RaptorQ,
        }
    }

    #[test]
    fn test_best_source_path_picks_lowest_rtt() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(100));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(10));

        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_best_repair_path_picks_highest_goodput() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low throughput
        sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(0).unwrap().estimator.record_throughput(100.0);

        // Path 1: high throughput
        sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(1).unwrap().estimator.record_throughput(1000.0);

        assert_eq!(sched.best_repair_path(), Some(1));
    }

    #[test]
    fn test_redundant_source_path_picks_different_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);
        sched.add_path(2);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(5));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(20));
        sched
            .path_mut(2)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(50));

        // Primary is 0, redundant should be 1 (second-lowest RTT)
        let redundant = sched.redundant_source_path(0);
        assert_eq!(redundant, Some(1));
    }

    #[test]
    fn test_redundant_source_path_none_with_single_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        assert_eq!(sched.redundant_source_path(0), None);
    }

    #[test]
    fn test_best_source_path_skips_full_cwnd() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(5));
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(50));

        // Fill path 0's cwnd
        let cwnd = sched.path(0).unwrap().cwnd;
        sched.path_mut(0).unwrap().in_flight = cwnd;

        // Should pick path 1 since path 0 has no capacity
        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_schedule_prefers_low_rtt_for_source() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: high RTT
        sched
            .path_mut(0)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(100));
        // Path 1: low RTT
        sched
            .path_mut(1)
            .unwrap()
            .estimator
            .record_rtt(std::time::Duration::from_millis(10));

        let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
        let result = sched.schedule(source, vec![]);

        // Path 1 (lower RTT) should get symbols first
        let path1_count = result
            .iter()
            .find(|(id, _)| *id == 1)
            .map(|(_, s)| s.len())
            .unwrap_or(0);

        assert!(path1_count > 0, "Low-RTT path should receive source symbols");
    }

    #[test]
    fn test_best_repair_path_avoiding_picks_alternative() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: highest goodput
        sched.path_mut(0).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(0).unwrap().estimator.record_throughput(1000.0);

        // Path 1: lower goodput
        sched.path_mut(1).unwrap().estimator.record_batch(10, 9);
        sched.path_mut(1).unwrap().estimator.record_throughput(500.0);

        // Avoiding path 0 should pick path 1
        assert_eq!(sched.best_repair_path_avoiding(0), Some(1));
        // Avoiding path 1 should pick path 0
        assert_eq!(sched.best_repair_path_avoiding(1), Some(0));
    }

    #[test]
    fn test_best_repair_path_avoiding_falls_back_single_path() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        // With only one path, avoiding it should still return it
        assert_eq!(sched.best_repair_path_avoiding(0), Some(0));
    }

    // -----------------------------------------------------------------------
    // Correction deficit tests (paper Section 13.4)
    // -----------------------------------------------------------------------

    #[test]
    fn test_deficit_tracks_sends_and_acks() {
        let mut deficit = CorrectionDeficit::new();
        assert_eq!(deficit.deficit(), 0.0);

        deficit.on_send(0, 1, 0.10);
        deficit.on_send(1, 1, 0.10);
        deficit.on_send(2, 2, 0.05);
        assert!((deficit.deficit() - 0.25).abs() < 1e-10);
        assert_eq!(deficit.pending_count(), 3);

        // ACK symbol 1
        assert!(deficit.on_ack(1));
        assert!((deficit.deficit() - 0.15).abs() < 1e-10);
        assert_eq!(deficit.pending_count(), 2);

        // ACK unknown symbol → no change
        assert!(!deficit.on_ack(99));
        assert!((deficit.deficit() - 0.15).abs() < 1e-10);
    }

    #[test]
    fn test_deficit_cumulative_ack() {
        let mut deficit = CorrectionDeficit::new();
        for seq in 0..10 {
            deficit.on_send(seq, 1, 0.10);
        }
        assert!((deficit.deficit() - 1.0).abs() < 1e-10);

        deficit.on_ack_cumulative(4); // ACK 0..=4
        assert_eq!(deficit.pending_count(), 5);
        assert!((deficit.deficit() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_deficit_per_path() {
        let mut deficit = CorrectionDeficit::new();
        deficit.on_send(0, 1, 0.10);
        deficit.on_send(1, 2, 0.05);
        deficit.on_send(2, 1, 0.10);

        assert!((deficit.path_deficit(1) - 0.20).abs() < 1e-10);
        assert!((deficit.path_deficit(2) - 0.05).abs() < 1e-10);
        assert!((deficit.path_deficit(3) - 0.00).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // Effective delivery time tests (paper Section 13.5)
    // -----------------------------------------------------------------------

    #[test]
    fn test_effective_delivery_time() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // Record multiple RTT samples so EWMA converges
        for _ in 0..20 {
            path.estimator.record_rtt(Duration::from_millis(100));
        }
        // Record some loss: 10 sent, 9 received → ~10% loss
        for _ in 0..20 {
            path.estimator.record_batch(10, 9);
        }

        let e = path.effective_delivery_time();
        let rtt = path.estimator.rtt().as_secs_f64();
        let eps = path.estimator.loss_rate();
        let expected = rtt / 2.0 + eps * rtt;
        assert!((e - expected).abs() < 0.001, "E_i={e}, expected={expected}, rtt={rtt}, eps={eps}");
    }

    #[test]
    fn test_correction_rate() {
        let mut sched = Scheduler::new(Arc::new(WallClock));
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // Record loss to get ~10% loss rate
        for _ in 0..20 {
            path.estimator.record_batch(10, 9);
        }
        let eps = path.estimator.loss_rate();
        let r = path.correction_rate();
        let expected = eps / (1.0 - eps);
        assert!((r - expected).abs() < 0.001, "r={r}, expected={expected}");
    }

    // -----------------------------------------------------------------------
    // Interpolated objective tests (paper Section 13.8)
    // -----------------------------------------------------------------------

    #[test]
    fn test_realtime_prefers_low_latency_over_low_loss() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low RTT (10ms), high loss (20%)
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: high RTT (200ms), low loss (1%)
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        // Realtime (w_lat=1, w_bw=0): should prefer path 0 (lower E_i despite higher loss)
        assert_eq!(sched.best_source_path(), Some(0));
    }

    #[test]
    fn test_bulk_prefers_low_overhead() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: low RTT (10ms), high loss (20%) → high r
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: high RTT (200ms), low loss (1%) → low r
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        // Bulk (w_lat=0, w_bw=1): should prefer path 1 (lower correction rate)
        assert_eq!(sched.best_source_path(), Some(1));
    }

    #[test]
    fn test_schedule_uses_objective_weights() {
        // With Realtime hint, source should go to low-latency path even if it has more loss
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Realtime);
        sched.add_path(0);
        sched.add_path(1);

        // Path 0: fast, lossy
        sched.path_mut(0).unwrap().estimator.record_rtt(Duration::from_millis(10));
        for _ in 0..20 {
            sched.path_mut(0).unwrap().estimator.record_batch(10, 8);
        }

        // Path 1: slow, clean
        sched.path_mut(1).unwrap().estimator.record_rtt(Duration::from_millis(200));
        for _ in 0..20 {
            sched.path_mut(1).unwrap().estimator.record_batch(100, 99);
        }

        let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
        let result = sched.schedule(source, vec![]);

        let path0_count = result
            .iter()
            .find(|(id, _)| *id == 0)
            .map(|(_, s)| s.len())
            .unwrap_or(0);

        assert!(path0_count > 0, "Realtime should send source on fast path");
    }

    // -----------------------------------------------------------------------
    // P7: Copa-lite production port (paper Sections 12.4-12.5, gate P1+P2)
    // -----------------------------------------------------------------------

    fn millis(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn test_copa_lite_cwnd_never_below_floor() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Establish a 10ms propagation floor.
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
        }

        // Hammer with inflated-RTT windows (delay backoffs) ...
        for _ in 0..50 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
            clock.advance(millis(150));
            sched.ack(0, 4);
            assert!(
                sched.path(0).unwrap().cwnd >= PathState::MIN_CWND,
                "delay backoffs must never take cwnd below the floor"
            );
        }
        // ... and with decode failures (loss steps).
        for _ in 0..100 {
            sched.on_loss(0, false);
        }
        let cwnd = sched.path(0).unwrap().cwnd;
        assert_eq!(cwnd, PathState::MIN_CWND);
        assert!(cwnd >= 8, "floor is 8 symbols, never the historical 2");
    }

    #[test]
    fn test_burst_rtt_spike_does_not_collapse_cwnd() {
        // The pre-P7 failure mode: the initial burst inflates its own RTT
        // samples, dq explodes, and the rate-formula target collapses cwnd
        // to the floor. With the windowed-min filter remembering the
        // propagation floor, a burst costs one gentle ×0.92 backoff.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Learn the 10ms floor and ramp for a few clean RTTs.
        for _ in 0..6 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
            clock.advance(millis(15));
            sched.ack(0, 8);
        }
        let pre_burst = sched.path(0).unwrap().cwnd;
        assert!(
            pre_burst > PathState::INITIAL_CWND,
            "ramp should have grown cwnd, got {pre_burst}"
        );

        // A burst inflates a full update window of RTT samples 4x.
        for _ in 0..4 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(40));
        }
        clock.advance(millis(50));
        sched.ack(0, 8);

        let post_burst = sched.path(0).unwrap().cwnd;
        let one_backoff = (pre_burst as f64 * BACKOFF_MULT) as u32;
        assert!(
            post_burst + 1 >= one_backoff,
            "burst must cost at most one gentle backoff: pre={pre_burst}, post={post_burst}"
        );
        assert!(
            post_burst > 2 * PathState::MIN_CWND,
            "burst must not collapse cwnd toward the floor: post={post_burst}"
        );

        // After the burst drains, samples return to the floor and cwnd
        // recovers additively (+2 per update).
        sched.path_mut(0).unwrap().record_rtt_sample(millis(10));
        clock.advance(millis(50));
        sched.ack(0, 8);
        let recovered = sched.path(0).unwrap().cwnd;
        assert_eq!(
            recovered,
            post_burst + ADDITIVE_STEP as u32,
            "post-backoff growth is additive"
        );
    }

    #[test]
    fn test_ramp_multiplicative_until_backoff_then_additive() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        // Clean RTTs at the floor: each per-SRTT update multiplies ×1.5+1.
        let mut prev = sched.path(0).unwrap().cwnd;
        for _ in 0..4 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
            clock.advance(millis(30));
            sched.ack(0, prev);
            let cur = sched.path(0).unwrap().cwnd;
            assert_eq!(
                cur,
                (prev as f64 * RAMP_GAIN + 1.0).round() as u32,
                "ramp phase is multiplicative"
            );
            assert!(sched.path(0).unwrap().in_slow_start);
            prev = cur;
        }

        // First backoff: inflated window ends the ramp.
        sched.path_mut(0).unwrap().record_rtt_sample(millis(80));
        clock.advance(millis(50));
        sched.ack(0, prev);
        let after_backoff = sched.path(0).unwrap().cwnd;
        assert_eq!(after_backoff, (prev as f64 * BACKOFF_MULT).round() as u32);
        assert!(!sched.path(0).unwrap().in_slow_start);

        // Subsequent clean updates are additive +2 — never multiplicative.
        let mut prev = after_backoff;
        for _ in 0..3 {
            sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
            clock.advance(millis(50));
            sched.ack(0, prev);
            let cur = sched.path(0).unwrap().cwnd;
            assert_eq!(cur, prev + ADDITIVE_STEP as u32, "steady state is additive");
            prev = cur;
        }
    }

    #[test]
    fn test_hint_changes_backoff_threshold() {
        // P1 (paper 12.4): the protocol hint sets the queue target.
        // floor = 100ms, windowed min = 118ms → dq = 18ms. The 100→118
        // step also charges the jitter estimator (18/8 = 2.25ms decaying
        // to ~1.72ms over the three samples → ~3.4ms threshold widening):
        //   Realtime target  8ms + 3.4ms → backoff
        //   Auto target   12.5ms + 3.4ms → backoff
        //   Bulk target     25ms + 3.4ms → keep growing
        fn run(hint: ProtocolHint) -> (u32, u32) {
            let clock = Arc::new(MockClock::new());
            let mut sched = Scheduler::new_with_hint(clock.clone(), hint);
            sched.add_path(0);
            for _ in 0..3 {
                sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
                clock.advance(millis(150));
                sched.ack(0, 8);
            }
            let pre = sched.path(0).unwrap().cwnd;
            for _ in 0..3 {
                sched.path_mut(0).unwrap().record_rtt_sample(millis(118));
            }
            clock.advance(millis(150));
            sched.ack(0, 8);
            (pre, sched.path(0).unwrap().cwnd)
        }

        let (rt_pre, rt_post) = run(ProtocolHint::Realtime);
        let (auto_pre, auto_post) = run(ProtocolHint::Auto);
        let (bulk_pre, bulk_post) = run(ProtocolHint::Bulk);

        assert!(rt_post < rt_pre, "Realtime backs off at dq=18ms: {rt_pre}->{rt_post}");
        assert!(auto_post < auto_pre, "Auto backs off at dq=18ms: {auto_pre}->{auto_post}");
        assert!(bulk_post > bulk_pre, "Bulk tolerates dq=18ms: {bulk_pre}->{bulk_post}");
    }

    #[test]
    fn test_jitter_widens_backoff_threshold_c2() {
        // Jitter-adjusted queue target (paper 12.4). C2-like link: 10ms
        // floor with ±6ms RTT jitter (netem 3ms/direction). Bulk's raw P1
        // threshold is 2.5ms — smaller than the jitter — so the pre-fix
        // windowed-min signal read jitter as a standing queue and backed
        // off nearly every update (measured at L1: cwnd pinned at the
        // floor for 60% of ACKs, 16x throughput gap vs quinn). With the
        // k×jitter_est widening, a jittery-but-queue-free link must ramp.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
        sched.add_path(0);

        // Deterministic jitter pattern with min 10ms, spread 6ms — every
        // update window's min sample sits 2-4ms above the 10s floor once
        // the floor has seen a 10ms sample.
        let pattern_ms = [10u64, 14, 12, 16, 13, 15, 12, 14];
        let mut cwnd_track = Vec::new();
        for round in 0..40 {
            // 4 ACK batches per SRTT window, one RTT sample each; skip
            // the true-floor sample in most windows (the windowed min
            // usually does NOT reach the floor — that is the trap).
            for k in 0..4 {
                let idx = (round * 4 + k) % pattern_ms.len();
                let ms = if round == 0 && k == 0 { 10 } else { pattern_ms[idx].max(12) };
                sched.path_mut(0).unwrap().record_rtt_sample(millis(ms));
            }
            clock.advance(millis(15));
            sched.ack(0, 8);
            cwnd_track.push(sched.path(0).unwrap().cwnd);
        }
        let final_cwnd = *cwnd_track.last().unwrap();
        assert!(
            final_cwnd > 100,
            "jittery queue-free C2 link must ramp past 100 symbols, got {final_cwnd} (track: {cwnd_track:?})"
        );

        // Sanity: a genuine standing queue on the SAME jittery link still
        // triggers backoff within a few updates — the queue shifts every
        // sample up by 12ms, while the consecutive-difference jitter
        // estimate stays at jitter scale.
        let before = sched.path(0).unwrap().cwnd;
        let mut backed_off = false;
        for round in 0..6 {
            for k in 0..4 {
                let idx = (round * 4 + k) % pattern_ms.len();
                sched
                    .path_mut(0)
                    .unwrap()
                    .record_rtt_sample(millis(pattern_ms[idx] + 12));
            }
            clock.advance(millis(25));
            sched.ack(0, 8);
            if sched.path(0).unwrap().cwnd < before {
                backed_off = true;
                break;
            }
        }
        assert!(backed_off, "a genuine 12ms standing queue must still back off");
    }

    #[test]
    fn test_hint_plumbed_to_paths() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.25);

        sched.set_protocol_hint(ProtocolHint::Realtime);
        assert_eq!(sched.path(0).unwrap().copa.queue_mult, 1.08);

        // New paths pick up the current hint.
        sched.add_path(1);
        assert_eq!(sched.path(1).unwrap().copa.queue_mult, 1.08);
    }

    #[test]
    fn test_pacing_token_bucket_rate_and_burst() {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        // SRTT = 100ms exactly (EWMA of identical samples).
        for _ in 0..4 {
            path.record_rtt_sample(millis(100));
        }
        path.cwnd = 200; // rate = 200/0.1 = 2000 symbols/sec
        path.pace_tokens = 0.0;

        clock.advance(millis(5)); // 2000/s × 5ms = 10 tokens
        sched.path_mut(0).unwrap().pace_refill();
        let tokens = sched.path(0).unwrap().pace_tokens();
        assert!((tokens - 10.0).abs() < 1e-6, "refill rate is cwnd/SRTT, got {tokens}");

        clock.advance(millis(100)); // would add 200 → capped at burst
        sched.path_mut(0).unwrap().pace_refill();
        let tokens = sched.path(0).unwrap().pace_tokens();
        // burst allowance = max(10, cwnd/8) = max(10, 25) = 25
        assert!((tokens - 25.0).abs() < 1e-6, "burst cap is max(10, cwnd/8), got {tokens}");

        // Batch-granular overdraft: consumption may push the bucket negative.
        let path = sched.path_mut(0).unwrap();
        path.consume_pace_tokens(30);
        assert!(path.pace_tokens() < 0.0);
        assert!(path.pace_delay() > Duration::ZERO);

        // Small-cwnd burst floor: max(10, cwnd/8) = 10.
        let path = sched.path_mut(0).unwrap();
        path.cwnd = 16;
        path.pace_tokens = 0.0;
        clock.advance(millis(1000));
        path.pace_refill();
        assert!((path.pace_tokens() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_copa_target_cwnd_units() {
        // Units doc-test: floor = SRTT = 100ms → dq clamps at 0.1ms.
        // rate = 1/(0.5 [1/sym] × 1e-4 [s]) = 20000 symbols/s
        // cwnd = 20000 [sym/s] × 0.1 [s] = 2000 symbols
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock);
        sched.add_path(0);
        let path = sched.path_mut(0).unwrap();
        path.record_rtt_sample(millis(100));
        assert_eq!(path.copa_target_cwnd(), 2000);
    }

    #[test]
    fn test_paced_ramp_reaches_block_scale_without_spurious_backoff() {
        // P7 follow-up regression: with SYMBOL-paced sends the standing
        // queue stays near zero, so at C2-like parameters (10ms floor, no
        // competing traffic) the ramp must sail past one 64KB block
        // (~56 symbols) within 15 SRTTs and never back off. The first L1
        // run of batch-granular pacing failed exactly this: every block
        // burst self-queued ~5.4ms > Bulk's 2.5ms threshold and cwnd
        // pinned at ~34, just under one block.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone()); // Auto: 1.125 target
        sched.add_path(0);

        for round in 0..15 {
            // Token-paced send phase across one RTT: consume only what
            // the bucket allows, in 1ms steps (never a whole-block burst).
            for _ in 0..12 {
                let p = sched.path_mut(0).unwrap();
                p.pace_refill();
                let budget = p.pace_tokens().max(0.0) as u32;
                if budget > 0 {
                    p.consume_pace_tokens(budget);
                }
                clock.advance(millis(1));
            }
            // Paced sends leave only sub-threshold jitter over the floor
            // (alternating 10.0/10.5ms; Auto's backoff needs > 11.25ms).
            let sample = if round % 2 == 0 { 10_000 } else { 10_500 };
            let p = sched.path_mut(0).unwrap();
            p.record_rtt_sample(Duration::from_micros(sample));

            let before = sched.path(0).unwrap().cwnd;
            sched.ack(0, before.min(64));
            let after = sched.path(0).unwrap().cwnd;
            assert!(
                after >= before,
                "paced sends must not trigger a backoff (round {round}): {before} -> {after}"
            );
        }
        let cwnd = sched.path(0).unwrap().cwnd;
        assert!(
            cwnd > 100,
            "ramp must clear one 64KB block (~56 symbols) at C2, got {cwnd}"
        );
    }

    #[test]
    fn test_schedule_ack_roundtrip_conserves_in_flight() {
        // The in_flight budget is charged ONCE, at schedule time. The L1
        // stall (P7 follow-up 2) was a double charge: schedule() charged,
        // then the paced drain charged the same symbols again at send
        // time — +1 leak per symbol, TUN gate jammed shut, throughput
        // throttled to the 2s leak-guard decay (~30 KB/s at C2).
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock);
        sched.add_path(0);

        let source: Vec<_> = (0..8).map(|i| make_symbol(i, false)).collect();
        let assignments = sched.schedule(source, vec![]);
        let scheduled: u32 = assignments.iter().map(|(_, s)| s.len() as u32).sum();
        assert_eq!(scheduled, 8);
        assert_eq!(sched.path(0).unwrap().in_flight, 8);

        // The paced drain charges TOKENS only — in_flight must not move
        // between schedule and ack (this is what net/mod.rs does now).
        sched.path_mut(0).unwrap().consume_pace_tokens(8);
        assert_eq!(sched.path(0).unwrap().in_flight, 8);

        // ACK feedback releases everything: budget conserved, gate opens.
        sched.ack(0, 8);
        assert_eq!(
            sched.path(0).unwrap().in_flight,
            0,
            "schedule → send → ack must conserve the in_flight budget"
        );
    }

    #[test]
    fn test_in_flight_expiry_releases_stranded_budget() {
        // ACKs are best-effort datagrams: a lost ACK strands its release
        // forever. The time-based expiry (max(4×SRTT, 250ms)) must reopen
        // the gate at RTT timescale without any feedback at all.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        let path = sched.path_mut(0).unwrap();
        for _ in 0..4 {
            path.record_rtt_sample(millis(10)); // srtt 10ms → horizon 250ms
        }
        path.charge_in_flight(56);
        assert_eq!(path.in_flight, 56);

        // Well before the horizon: nothing expires.
        clock.advance(millis(100));
        let path = sched.path_mut(0).unwrap();
        path.expire_in_flight();
        assert_eq!(path.in_flight, 56);

        // A partial ACK releases FIFO; the stranded remainder expires
        // once the horizon passes.
        path.release_in_flight(50);
        assert_eq!(path.in_flight, 6);
        clock.advance(millis(200)); // total 300ms > 250ms horizon
        let path = sched.path_mut(0).unwrap();
        path.expire_in_flight();
        assert_eq!(
            path.in_flight, 0,
            "stranded budget must expire at RTT timescale, not the 2s guard"
        );
    }

    #[test]
    fn test_c2_loop_cwnd_grows_past_200_within_5s() {
        // Full C2 loop at the scheduler level (100 Mbit / 10ms RTT / Bulk),
        // mirroring the production wiring: schedule-time budget charge,
        // token-paced sends stamped at WIRE time (echo-timestamp RTT
        // therefore excludes pacing-queue delay — verified hypothesis:
        // batches are built at send time from the carry), per-datagram
        // ACKs with ~1.3% of them lost (stranding releases), and
        // time-based expiry. cwnd must ramp past 200 symbols within 5
        // simulated seconds and the sender must be ACK-clocked, not
        // leak-guard throttled.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new_with_hint(clock.clone(), ProtocolHint::Bulk);
        sched.add_path(0);

        const OWD: Duration = Duration::from_millis(5); // 10ms RTT
        let mut carry: u32 = 0; // interleaver + pacing carry (already charged)
        // (ack_arrival, symbols, wire_send_instant)
        let mut acks: VecDeque<(Instant, u32, Instant)> = VecDeque::new();
        let mut wire_counter: u64 = 0;
        let mut total_sent: u64 = 0;

        for _tick in 0..5000 {
            let now = clock.now();

            // Encoder + TUN gate: schedule one 56-symbol block (64KB /
            // 1200B) whenever the committed budget is under cwnd.
            {
                let p = sched.path_mut(0).unwrap();
                p.expire_in_flight();
                if p.in_flight < p.cwnd {
                    p.charge_in_flight(56);
                    carry += 56;
                }
            }

            // Pacer: send from the carry under tokens; the batch timestamp
            // is stamped HERE (wire time), as in send_interleaved_batches.
            {
                let p = sched.path_mut(0).unwrap();
                p.pace_refill();
                let budget = (p.pace_tokens().max(0.0) as u32).min(carry);
                if budget > 0 {
                    p.consume_pace_tokens(budget);
                    carry -= budget;
                    total_sent += budget as u64;
                    // Receiver ACKs each datagram after one RTT; ~1.3% of
                    // ACK datagrams are lost (their releases stranded).
                    let mut acked = 0;
                    for _ in 0..budget {
                        wire_counter += 1;
                        if wire_counter % 77 != 0 {
                            acked += 1;
                        }
                    }
                    if acked > 0 {
                        acks.push_back((now + OWD * 2, acked, now));
                    }
                }
            }

            // Deliver due ACKs: RTT = now − echoed wire timestamp.
            while acks.front().is_some_and(|(t, _, _)| *t <= now) {
                let (_, n, sent_at) = acks.pop_front().unwrap();
                let rtt = now.duration_since(sent_at);
                let p = sched.path_mut(0).unwrap();
                p.record_rtt_sample(rtt);
                p.release_in_flight(n);
                p.on_ack(n);
            }

            clock.advance(millis(1));
        }

        let cwnd = sched.path(0).unwrap().cwnd;
        assert!(
            cwnd > 200,
            "C2 loop must ramp cwnd past 200 symbols within 5s, got {cwnd}"
        );
        // Ack-clocked throughput, not the 2s leak-guard trickle (the L1
        // stall sent ~450 symbols in 15s; here 5s must move far more).
        assert!(
            total_sent > 20_000,
            "sender must be ack-clocked, not gate-starved: sent {total_sent}"
        );
    }

    #[test]
    fn test_low_floor_clamp_no_spurious_backoff() {
        // LAN-class floor (200us): the backoff threshold clamps at 0.1ms
        // and dq clamps at the SAME 0.1ms, so sub-clamp jitter (raw dq
        // 80us) can never back off — while a genuine standing queue
        // (raw dq 200us > clamp) still does.
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);

        for round in 0..10 {
            let sample = if round % 2 == 0 { 200 } else { 280 };
            let p = sched.path_mut(0).unwrap();
            p.record_rtt_sample(Duration::from_micros(sample));
            clock.advance(millis(1)); // >> sub-ms SRTT: update every round
            let before = sched.path(0).unwrap().cwnd;
            sched.ack(0, 8);
            let after = sched.path(0).unwrap().cwnd;
            assert!(
                after >= before,
                "jitter below the dq clamp must not back off (round {round}): {before} -> {after}"
            );
        }
        assert!(
            sched.path(0).unwrap().cwnd > PathState::INITIAL_CWND,
            "LAN ramp should have grown"
        );

        // Sanity: a real standing queue above the clamp DOES back off.
        let p = sched.path_mut(0).unwrap();
        p.record_rtt_sample(Duration::from_micros(400)); // raw dq 200us
        clock.advance(millis(1));
        let before = sched.path(0).unwrap().cwnd;
        sched.ack(0, 8);
        assert!(
            sched.path(0).unwrap().cwnd < before,
            "genuine LAN queue must still back off"
        );
    }

    // ===================================================================
    // RWM Phase B — per-symbol placement law (paper §16.3). The cost is
    //   in_flight/cwnd + w_lat·(E_prop/ref_srtt) + w_bw·r + w_div·fate,
    // sampled as P(i) ∝ exp(−cost/T).
    // ===================================================================

    /// Look up a path's probability in a place_probs distribution.
    fn prob_of(dist: &[(PathId, f64)], id: PathId) -> f64 {
        dist.iter().find(|(p, _)| *p == id).map(|(_, w)| *w).unwrap_or(0.0)
    }

    fn set_rtt(sched: &mut Scheduler, id: PathId, ms: u64) {
        // The estimator RTT is an EWMA (α = 0.125) seeded at 50 ms; feed enough
        // samples to converge so tests exercise the intended RTT, not a warm-up
        // blend of it and the seed.
        let p = sched.path_mut(id).unwrap();
        for _ in 0..60 {
            p.estimator.record_rtt(std::time::Duration::from_millis(ms));
        }
    }

    /// (a) Idle 2-path → placement concentrates on the cheapest (lowest-RTT)
    /// path. Softmax "concentrate" = the vast majority of the mass.
    #[test]
    fn place_idle_concentrates_on_cheapest() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 50); // path 1 is 5× slower
        // both idle (in_flight = 0)
        let dist = sched.place_probs(false, &[]);
        let p0 = prob_of(&dist, 0);
        let p1 = prob_of(&dist, 1);
        assert!(p0 > 0.95, "cheapest path must take the mass, got p0={p0}");
        assert!(p0 > p1);
    }

    /// (b) As the chosen path's in_flight rises, placement shifts CONTINUOUSLY
    /// to the other path — no threshold jump. Assert strict monotonic shift.
    #[test]
    fn place_shifts_monotonically_with_load() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10); // symmetric: isolate the load term
        let cwnd = sched.path(0).unwrap().cwnd; // 10

        let mut prev_p0 = f64::INFINITY;
        // Sweep in_flight from empty to 2× cwnd (into overdraft) — the path is
        // never removed from the distribution (no capacity filter), so the
        // shift is continuous THROUGH saturation, not a jump at cwnd.
        for infl in 0..=(2 * cwnd) {
            sched.path_mut(0).unwrap().in_flight = infl;
            let dist = sched.place_probs(false, &[]);
            let p0 = prob_of(&dist, 0);
            let p1 = prob_of(&dist, 1);
            assert!(
                p0 < prev_p0,
                "p0 must strictly decrease as path-0 load rises: infl={infl} p0={p0} prev={prev_p0}"
            );
            // p1 is its complement (two paths) → strictly increasing.
            assert!((p0 + p1 - 1.0).abs() < 1e-9);
            prev_p0 = p0;
        }
        // Ended favouring the unloaded path.
        assert!(prev_p0 < 0.1, "heavily loaded path should be largely abandoned");
    }

    /// (c) Water-filling equilibrium: the fixed point of marginal-cost
    /// equalisation is `in_flight/cwnd` equal across paths, i.e. in_flight ∝
    /// cwnd ∝ capacity. At that stock ratio placement is BALANCED (both paths
    /// used equally) — the signature that the law fills proportional to
    /// capacity rather than concentrating.
    #[test]
    fn place_backlog_waterfills_proportional_to_capacity() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10);
        // Path 0 has 2× the capacity of path 1.
        sched.path_mut(0).unwrap().cwnd = 20;
        sched.path_mut(1).unwrap().cwnd = 10;
        // Equilibrium stock: in_flight ∝ cwnd ⇒ equal fill fraction 0.4.
        sched.path_mut(0).unwrap().in_flight = 8;
        sched.path_mut(1).unwrap().in_flight = 4;
        let dist = sched.place_probs(false, &[]);
        let p0 = prob_of(&dist, 0);
        let p1 = prob_of(&dist, 1);
        assert!(p0 > 0.1 && p1 > 0.1, "both paths used at equilibrium: p0={p0} p1={p1}");
        assert!((p0 - p1).abs() < 0.05, "balanced at the capacity-proportional fixed point");

        // And off-equilibrium (equal stock, unequal capacity) the law pushes
        // MORE toward the higher-capacity (lower-fill) path.
        sched.path_mut(0).unwrap().in_flight = 6;
        sched.path_mut(1).unwrap().in_flight = 6;
        let dist2 = sched.place_probs(false, &[]);
        assert!(prob_of(&dist2, 0) > prob_of(&dist2, 1));
    }

    /// (d) Repair fate steers a repair OFF the path that carried the window
    /// symbols it covers; source placement ignores fate.
    #[test]
    fn place_repair_fate_steers_off_covered_path() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10);
        set_rtt(&mut sched, 1, 10); // identical paths — only fate differs

        // Source ignores fate → balanced even when all coverage is on path 0.
        let src = sched.place_probs(false, &[0, 0, 0, 0]);
        assert!((prob_of(&src, 0) - prob_of(&src, 1)).abs() < 0.05);

        // Repair whose coverage is entirely on path 0 → steered to path 1.
        let rep = sched.place_probs(true, &[0, 0, 0, 0]);
        assert!(
            prob_of(&rep, 1) > 0.95,
            "repair must avoid its own coverage: p1={}",
            prob_of(&rep, 1)
        );

        // Split coverage → fate equal → balanced again.
        let rep_split = sched.place_probs(true, &[0, 0, 1, 1]);
        assert!((prob_of(&rep_split, 0) - prob_of(&rep_split, 1)).abs() < 0.05);
    }

    /// (e) T → 0 collapses the softmax to argmin (strict best-path, the
    /// no-cutoffs limit).
    #[test]
    fn place_temperature_zero_is_argmin() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
        sched.add_path(0);
        sched.add_path(1);
        set_rtt(&mut sched, 0, 10); // cheaper
        set_rtt(&mut sched, 1, 50);
        let dist = sched.place_probs_with_temperature(false, &[], 1e-9);
        assert!(prob_of(&dist, 0) > 0.999, "T→0 → argmin all mass on path 0");
        assert!(prob_of(&dist, 1) < 1e-3);
    }

    /// Single path ⇒ that path always (byte-identical to the pre-RWM
    /// single-path sender — the law with N=1 is a no-op).
    #[test]
    fn place_single_path_is_identity() {
        let mut sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Bulk);
        sched.add_path(0);
        set_rtt(&mut sched, 0, 20);
        let dist = sched.place_probs(false, &[]);
        assert_eq!(dist.len(), 1);
        assert_eq!(dist[0].0, 0);
        assert!((dist[0].1 - 1.0).abs() < 1e-12);
        // Even heavily overdrafted, the lone path is still chosen.
        sched.path_mut(0).unwrap().in_flight = 10_000;
        assert_eq!(sched.place_symbol(false, &[]), Some(0));
    }
}
