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
/// Controls the latency vs bandwidth trade-off in the interpolated objective.
/// See paper Section 13.8.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingWeights {
    /// Weight for latency cost: SUM(x_i × E_i)
    pub w_lat: f64,
    /// Weight for bandwidth overhead cost: SUM(x_i × r_i)
    pub w_bw: f64,
}

impl SchedulingWeights {
    pub fn from_hint(hint: ProtocolHint) -> Self {
        match hint {
            ProtocolHint::Realtime => Self { w_lat: 1.0, w_bw: 0.0 },
            ProtocolHint::Bulk => Self { w_lat: 0.0, w_bw: 1.0 },
            ProtocolHint::Auto => Self { w_lat: 0.5, w_bw: 0.5 },
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

    /// Whether the standing-queue signal is above the hint-coupled target:
    /// windowed-min RTT − floor (= dq, clamped ≥ 0.1ms) exceeds
    /// (queue_mult − 1) × floor (also clamped ≥ 0.1ms).
    ///
    /// Equivalent to the gate driver's `min_rtt_win > floor × queue_mult`
    /// except for the dq clamp, which keeps sub-millisecond-RTT links from
    /// backing off on jitter (see DQ_FLOOR_SECS).
    fn queue_above_target(&self) -> bool {
        let (Some(win_min), Some(floor)) = (self.min_rtt_since_update, self.min_rtt) else {
            return false;
        };
        let floor_s = floor.as_secs_f64();
        let dq = (win_min.as_secs_f64() - floor_s).max(DQ_FLOOR_SECS);
        let dq_target = ((self.queue_mult - 1.0) * floor_s).max(DQ_FLOOR_SECS);
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
        self.last_cwnd_update = self.clock.now();
        // No RTT samples since the last update → no signal, hold.
        if self.min_rtt_since_update.is_none() {
            return cwnd;
        }
        let above = self.queue_above_target();
        self.min_rtt_since_update = None;
        let c = cwnd as f64;
        let next = if above {
            self.ramping = false;
            c * BACKOFF_MULT
        } else if self.ramping {
            c * RAMP_GAIN + 1.0
        } else {
            c + ADDITIVE_STEP
        };
        next.round() as u32
    }

    /// Immediate backoff (ramp fast-exit or decode-failure congestion):
    /// ×0.92, end the ramp, restart the update window.
    fn backoff(&mut self, cwnd: u32) -> u32 {
        self.ramping = false;
        self.min_rtt_since_update = None;
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

        if self.copa.ramping && self.copa.queue_above_target() {
            // Fast ramp exit: gentle ×0.92, NOT a collapse to a
            // rate-formula target (the pre-P7 bug: the initial burst
            // inflated its own RTT samples, dq exploded, and the target
            // dropped to the floor on the very first burst).
            self.cwnd = self.copa.backoff(self.cwnd);
        } else if self.copa.should_update(now) {
            self.cwnd = self.copa.update_cwnd(self.cwnd);
        }

        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);

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
        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);
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
}

/// The multipath scheduler.
///
/// Uses the interpolated objective function from paper Section 13.8:
///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
/// where E_i is effective delivery time and r_i is correction rate per path.
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
        }
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

        // Distribute source symbols to lowest-cost paths first
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

        // Update in_flight counters
        for (path_id, syms) in &assignments {
            if let Some(path) = self.paths.get_mut(path_id) {
                path.in_flight += syms.len() as u32;
            }
        }

        assignments.into_iter().collect()
    }

    /// Acknowledge received symbols on a path.
    pub fn ack(&mut self, path_id: PathId, count: u32) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.in_flight = path.in_flight.saturating_sub(count);
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
                // queue target; pacing restarts at the initial burst).
                path.cwnd = PathState::INITIAL_CWND;
                path.ssthresh = 64;
                path.in_slow_start = true;
                path.copa.reset();
                path.pace_tokens = PathState::INITIAL_CWND as f64;
                path.last_pace_refill = path.last_report;
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
        // floor = 100ms, windowed min = 115ms → dq = 15ms:
        //   Realtime target  8ms → backoff
        //   Auto target   12.5ms → backoff
        //   Bulk target     25ms → keep growing
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
                sched.path_mut(0).unwrap().record_rtt_sample(millis(115));
            }
            clock.advance(millis(150));
            sched.ack(0, 8);
            (pre, sched.path(0).unwrap().cwnd)
        }

        let (rt_pre, rt_post) = run(ProtocolHint::Realtime);
        let (auto_pre, auto_post) = run(ProtocolHint::Auto);
        let (bulk_pre, bulk_post) = run(ProtocolHint::Bulk);

        assert!(rt_post < rt_pre, "Realtime backs off at dq=15ms: {rt_pre}->{rt_post}");
        assert!(auto_post < auto_pre, "Auto backs off at dq=15ms: {auto_pre}->{auto_post}");
        assert!(bulk_post > bulk_pre, "Bulk tolerates dq=15ms: {bulk_pre}->{bulk_post}");
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
}
