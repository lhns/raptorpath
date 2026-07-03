//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.
//!
//! Congestion control uses Copa (delay-based): it tracks the minimum RTT
//! (propagation baseline) and computes rate = 1/(d_copa × dq), where
//! dq = RTT - min_RTT is the queuing delay.  Loss alone does NOT reduce
//! the window — only rising RTT does.  This prevents wireless random loss
//! from collapsing throughput.  No ProbeRTT phase (natural oscillation).

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
const COPA_DELTA: f64 = 0.5;

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

/// Copa delay-based congestion control state.
///
/// Copa (Arun & Balakrishnan, NSDI 2018) computes the sending rate from
/// the queuing delay: rate = 1 / (d_copa × dq), where dq = RTT - min_RTT.
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
    /// Minimum RTT seen in the current window (propagation baseline).
    min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    max_bw: f64,
    /// Previous RTT for detecting trends.
    prev_rtt: Option<Duration>,
    /// Consecutive RTT increases (congestion signal).
    rtt_increases: u32,
    /// Number of RTT increases that count as congestion.
    congestion_threshold: u32,
    /// Delivered symbols counter for delivery rate calculation.
    delivered: u64,
    /// Timestamp of last delivery measurement.
    last_delivered_time: Instant,
    /// Delivered count at last measurement.
    last_delivered: u64,
    /// Whether we're still in initial ramp-up (first few RTTs).
    in_startup: bool,
    /// Injectable clock for time queries.
    clock: Arc<dyn Clock>,
}

impl CopaState {
    fn new(clock: Arc<dyn Clock>) -> Self {
        let now = clock.now();
        Self {
            bw_samples: VecDeque::new(),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            prev_rtt: None,
            rtt_increases: 0,
            congestion_threshold: 3,
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
            in_startup: true,
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

    /// Record an RTT sample.
    fn record_rtt(&mut self, rtt: Duration) {
        let now = self.clock.now();

        // Detect RTT trend
        if let Some(prev) = self.prev_rtt {
            if rtt > prev + prev / 10 {
                self.rtt_increases += 1;
            } else {
                self.rtt_increases = self.rtt_increases.saturating_sub(1);
            }
        }
        self.prev_rtt = Some(rtt);

        self.rtt_samples.push_back(RttSample {
            rtt,
            timestamp: now,
        });
        self.expire_old_samples(now);
        self.min_rtt = self.rtt_samples.iter().map(|s| s.rtt).min();
    }

    /// Whether RTT is trending upward (congestion detected).
    fn is_congested(&self) -> bool {
        self.rtt_increases >= self.congestion_threshold
    }

    /// Copa cwnd target: rate = 1/(d_copa × dq), cwnd = rate × min_rtt.
    ///
    /// dq = current_rtt - min_rtt (queuing delay).
    /// When dq is small (queue empty): rate is high → large cwnd.
    /// When dq is large (queue full): rate drops → small cwnd.
    fn copa_cwnd(&self) -> u32 {
        let min_rtt = self.min_rtt.unwrap_or(Duration::from_millis(50));
        let min_rtt_secs = min_rtt.as_secs_f64();
        let current_rtt_secs = self.prev_rtt
            .unwrap_or(min_rtt)
            .as_secs_f64();

        let dq = (current_rtt_secs - min_rtt_secs).max(0.0001); // avoid div by zero
        let rate = 1.0 / (COPA_DELTA * dq); // symbols per second

        // cwnd = rate × min_rtt (how many symbols fill the pipe)
        let cwnd = rate * min_rtt_secs;

        // During startup, allow aggressive growth (2x gain)
        let gain = if self.in_startup { 2.0 } else { 1.0 };
        let target = (cwnd * gain) as u32;
        target.clamp(PathState::MIN_CWND, PathState::MAX_CWND)
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

    fn in_startup(&self) -> bool {
        self.in_startup
    }

    fn exit_startup(&mut self) {
        self.in_startup = false;
    }

    fn reset(&mut self) {
        let clock = self.clock.clone();
        *self = Self::new(clock);
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
    /// Injectable clock
    clock: Arc<dyn Clock>,
}

impl PathState {
    /// Minimum congestion window (never go below this).
    pub const MIN_CWND: u32 = 2;
    /// Initial congestion window.
    pub const INITIAL_CWND: u32 = 10;
    /// Maximum congestion window.
    pub const MAX_CWND: u32 = 10_000;
}

impl PathState {
    pub fn new(id: PathId, clock: Arc<dyn Clock>) -> Self {
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
            copa: CopaState::new(clock.clone()),
            clock,
        }
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

    /// Copa congestion control: handle acknowledgements.
    ///
    /// Records delivery and adjusts cwnd via Copa's delay-based formula:
    /// rate = 1/(d_copa × dq), cwnd = rate × min_rtt.
    /// During startup, cwnd grows aggressively (2× gain).
    /// Once RTT starts rising, transitions to steady state.
    pub fn on_ack(&mut self, acked: u32) {
        let _rate = self.copa.record_delivery(acked);

        // Startup exit: if RTT is rising (queue building)
        if self.copa.in_startup() {
            if self.copa.is_congested() {
                self.copa.exit_startup();
            }
            // Also exit startup if we've had enough samples
            if self.copa.bw_samples.len() >= 4 && self.copa.min_rtt.is_some() {
                let copa_target = self.copa.copa_cwnd();
                if self.cwnd >= copa_target {
                    self.copa.exit_startup();
                }
            }
        }

        // Copa cwnd: purely delay-based, no phases
        let copa_target = self.copa.copa_cwnd();

        if self.copa.in_startup() {
            // During startup, grow toward Copa target but also allow
            // traditional slow-start growth if estimate is too low
            self.cwnd = std::cmp::max(self.cwnd + acked, copa_target);
        } else {
            // Steady state: converge toward Copa target
            // Smooth transition: move 25% toward target per ACK
            if copa_target > self.cwnd {
                let step = std::cmp::max(1, (copa_target - self.cwnd) / 4);
                self.cwnd += step;
            } else if copa_target < self.cwnd {
                let step = std::cmp::max(1, (self.cwnd - copa_target) / 4);
                self.cwnd = self.cwnd.saturating_sub(step);
            }
        }

        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);

        // Sync legacy fields
        self.in_slow_start = self.copa.in_startup();
        if !self.in_slow_start && self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// Copa congestion control: handle loss events.
    ///
    /// Unlike AIMD, loss alone does NOT reduce cwnd.  The key insight:
    ///   - Loss + stable RTT → wireless/random loss → ignore (FEC handles it)
    ///   - Loss + rising RTT → real congestion → drain toward Copa target
    ///   - Decode failure + rising RTT → severe congestion → drain to 0.75 × Copa target
    pub fn on_loss(&mut self, fec_recovered: bool) {
        if self.copa.is_congested() {
            // RTT is rising → real congestion
            self.copa.exit_startup();
            let copa_target = self.copa.copa_cwnd();

            if fec_recovered {
                // Congestion but FEC saved us — drain to Copa target
                self.cwnd = std::cmp::max(copa_target, Self::MIN_CWND);
            } else {
                // Decode failure + congestion — aggressive drain to 75% Copa target
                let target = (copa_target as f64 * 0.75) as u32;
                self.cwnd = std::cmp::max(target, Self::MIN_CWND);
                self.ssthresh = self.cwnd;
                self.in_slow_start = false;
            }
        } else {
            // RTT is stable → wireless/random loss, not congestion
            if fec_recovered {
                // FEC recovered, no congestion signal → do nothing
            } else {
                // Decode failure without congestion is unusual.
                // Gently reduce: this might be a borderline case where
                // we need slightly more FEC, not less bandwidth.
                self.cwnd = std::cmp::max(self.cwnd.saturating_sub(1), Self::MIN_CWND);
            }
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
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(ProtocolHint::Auto),
        }
    }

    /// Create scheduler with protocol hint for weight configuration.
    pub fn new_with_hint(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(hint),
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
        self.paths.insert(id, PathState::new(id, self.clock.clone()));
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
                // Reset to startup on recovery
                path.cwnd = PathState::INITIAL_CWND;
                path.ssthresh = 64;
                path.in_slow_start = true;
                path.copa.reset();
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
    /// Set protocol hint (updates scheduling weights).
    pub fn set_protocol_hint(&mut self, hint: ProtocolHint) {
        self.weights = SchedulingWeights::from_hint(hint);
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
}
