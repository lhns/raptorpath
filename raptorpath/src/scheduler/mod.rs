//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.
//!
//! Congestion control is BBR-inspired and delay-based: it tracks the minimum
//! RTT (propagation baseline) and maximum delivery rate in sliding windows,
//! then sets cwnd = BDP (bandwidth × delay product).  Loss alone does NOT
//! reduce the window — only rising RTT does.  This prevents wireless random
//! loss from collapsing throughput, unlike traditional loss-based AIMD.

pub mod clock;
pub use clock::*;

use crate::control::LossEstimator;
use crate::fec::{FecBackend, WireSymbol};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

/// BBR phase state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BbrPhase {
    /// Probing for bandwidth (aggressive 2x gain).
    Startup,
    /// Steady-state bandwidth probing (1x gain).
    ProbeBw,
    /// Periodic min_rtt re-measurement (reduced cwnd).
    ProbeRtt,
}

/// How often to enter ProbeRTT to refresh min_rtt (BBRv1: 10s).
const PROBE_RTT_INTERVAL: Duration = Duration::from_secs(10);
/// How long to hold in ProbeRTT phase (BBRv1: 200ms).
const PROBE_RTT_DURATION: Duration = Duration::from_millis(200);
/// Cwnd during ProbeRTT — minimal to drain the pipe.
const PROBE_RTT_CWND: u32 = 4;

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

/// BBR-inspired congestion control state.
///
/// Instead of reacting to loss (AIMD), we model the pipe:
///   - `min_rtt`: propagation delay baseline (10s sliding window)
///   - `max_bw`: maximum observed delivery rate (10s sliding window)
///   - `bdp = max_bw × min_rtt`: the bandwidth-delay product
///   - `cwnd = gain × bdp` (gain > 1 during probe, = 1 steady state)
///
/// Loss handling is RTT-aware:
///   - Loss + stable RTT → wireless/random → no cwnd reduction
///   - Loss + rising RTT → real congestion → reduce to BDP
///   - Decode failure + rising RTT → aggressive drain to 0.75 × BDP
#[derive(Debug)]
pub struct BbrState {
    /// Whether ProbeRTT phase is enabled (can be disabled for benchmarking)
    enable_probe_rtt: bool,
    /// Sliding window of bandwidth samples (symbols/sec).
    bw_samples: VecDeque<BwSample>,
    /// Sliding window of RTT samples.
    rtt_samples: VecDeque<RttSample>,
    /// How long to keep samples in sliding windows.
    window_duration: Duration,
    /// Minimum RTT seen in the current window (propagation baseline).
    min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    max_bw: f64,
    /// Current gain factor applied to BDP for cwnd.
    /// > 1.0 during startup/probing, 1.0 in steady state.
    cwnd_gain: f64,
    /// Current BBR phase (Startup, ProbeBw, or ProbeRtt).
    phase: BbrPhase,
    /// When min_rtt was last refreshed (for ProbeRTT entry decision).
    min_rtt_stamp: Instant,
    /// When ProbeRTT hold period ends (None if not in ProbeRTT).
    probe_rtt_done_stamp: Option<Instant>,
    /// Cwnd to restore after ProbeRTT exits.
    prior_cwnd: Option<u32>,
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
    /// Injectable clock for time queries.
    clock: Arc<dyn Clock>,
}

impl BbrState {
    fn new(clock: Arc<dyn Clock>, enable_probe_rtt: bool) -> Self {
        let now = clock.now();
        Self {
            enable_probe_rtt,
            bw_samples: VecDeque::new(),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            cwnd_gain: 2.0, // start with 2x gain for startup probing
            phase: BbrPhase::Startup,
            min_rtt_stamp: now,
            probe_rtt_done_stamp: None,
            prior_cwnd: None,
            prev_rtt: None,
            rtt_increases: 0,
            congestion_threshold: 3,
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
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
            // RTT increased by more than 10% → possible congestion
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

        let old_min = self.min_rtt;
        self.min_rtt = self.rtt_samples.iter().map(|s| s.rtt).min();

        // Refresh min_rtt_stamp only when a genuinely new low is observed.
        // This ensures ProbeRTT fires after PROBE_RTT_INTERVAL when RTT
        // is trending up and no fresh minimum has been seen.
        if old_min.is_none() || self.min_rtt < old_min {
            self.min_rtt_stamp = now;
        }
    }

    /// Whether RTT is trending upward (congestion detected).
    fn is_congested(&self) -> bool {
        self.rtt_increases >= self.congestion_threshold
    }

    /// Compute the BDP-based cwnd target.
    fn bdp_cwnd(&self) -> u32 {
        // During ProbeRTT, use minimal cwnd to drain the pipe
        if self.phase == BbrPhase::ProbeRtt {
            return PROBE_RTT_CWND;
        }

        let min_rtt_secs = self
            .min_rtt
            .unwrap_or(Duration::from_millis(50))
            .as_secs_f64();

        let bdp = self.max_bw * min_rtt_secs;

        // Apply gain and clamp
        let target = (bdp * self.cwnd_gain) as u32;
        target.clamp(PathState::MIN_CWND, PathState::MAX_CWND)
    }

    /// Expire samples older than the sliding window.
    fn expire_old_samples(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        while self
            .bw_samples
            .front()
            .is_some_and(|s| s.timestamp < cutoff)
        {
            self.bw_samples.pop_front();
        }
        while self
            .rtt_samples
            .front()
            .is_some_and(|s| s.timestamp < cutoff)
        {
            self.rtt_samples.pop_front();
        }
    }

    /// Exit startup phase: drop gain to steady-state.
    fn exit_startup(&mut self) {
        if self.phase == BbrPhase::Startup {
            self.phase = BbrPhase::ProbeBw;
            self.cwnd_gain = 1.0;
        }
    }

    /// Whether we're still in startup phase.
    fn in_startup(&self) -> bool {
        self.phase == BbrPhase::Startup
    }

    /// Check if we should enter ProbeRTT to refresh min_rtt.
    /// Enters ProbeRTT if min_rtt hasn't been refreshed in PROBE_RTT_INTERVAL.
    /// `current_cwnd` is needed to save/restore after ProbeRTT.
    fn maybe_enter_probe_rtt(&mut self, current_cwnd: u32) {
        if !self.enable_probe_rtt || self.phase == BbrPhase::ProbeRtt {
            return;
        }
        let now = self.clock.now();
        if now.duration_since(self.min_rtt_stamp) > PROBE_RTT_INTERVAL {
            self.prior_cwnd = Some(current_cwnd);
            self.phase = BbrPhase::ProbeRtt;
            self.probe_rtt_done_stamp = Some(now + PROBE_RTT_DURATION);
        }
    }

    /// Check if ProbeRTT hold period is complete, and exit if so.
    /// Returns the prior cwnd to restore, if exiting.
    fn maybe_exit_probe_rtt(&mut self) -> Option<u32> {
        if self.phase != BbrPhase::ProbeRtt {
            return None;
        }
        let now = self.clock.now();
        if let Some(done) = self.probe_rtt_done_stamp {
            if now >= done {
                self.min_rtt_stamp = now;
                self.phase = BbrPhase::ProbeBw;
                self.probe_rtt_done_stamp = None;
                return self.prior_cwnd.take();
            }
        }
        None
    }

    fn reset(&mut self) {
        let clock = self.clock.clone();
        let enable_probe_rtt = self.enable_probe_rtt;
        *self = Self::new(clock, enable_probe_rtt);
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
    /// Slow-start threshold (kept for compatibility with tests, but BBR
    /// uses BDP-based cwnd instead of ssthresh-driven phase changes)
    pub ssthresh: u32,
    /// Whether we are in slow-start phase (maps to BBR startup)
    pub in_slow_start: bool,
    /// Last time we received an RTCP-style report or any data from this path
    pub last_report: Instant,
    /// Maximum datagram size discovered for this path
    pub max_datagram_size: Option<usize>,
    /// BBR congestion control state
    bbr: BbrState,
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
    pub fn new(id: PathId, clock: Arc<dyn Clock>, enable_probe_rtt: bool) -> Self {
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
            bbr: BbrState::new(clock.clone(), enable_probe_rtt),
            clock,
        }
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

    /// BBR-style congestion control: handle acknowledgements.
    ///
    /// Records delivery, computes BDP, and adjusts cwnd toward the
    /// bandwidth-delay product.  During startup, cwnd grows aggressively
    /// (2× BDP gain).  Once the pipe is full (RTT starts rising or BDP
    /// stabilizes), transitions to steady state (1× gain).
    pub fn on_ack(&mut self, acked: u32) {
        let _rate = self.bbr.record_delivery(acked);

        // Startup exit: if delivery rate stopped growing or RTT is rising
        if self.bbr.in_startup() {
            if self.bbr.is_congested() {
                self.bbr.exit_startup();
            }
            // Also exit startup if we've had enough samples and BDP is meaningful
            if self.bbr.bw_samples.len() >= 4 && self.bbr.min_rtt.is_some() {
                let bdp = self.bbr.bdp_cwnd();
                if self.cwnd >= bdp {
                    self.bbr.exit_startup();
                }
            }
        }

        // Check ProbeRTT entry/exit
        self.bbr.maybe_enter_probe_rtt(self.cwnd);
        if let Some(prior) = self.bbr.maybe_exit_probe_rtt() {
            self.cwnd = prior;
        }

        // Set cwnd based on BDP
        let bdp_target = self.bbr.bdp_cwnd();

        if self.bbr.phase == BbrPhase::ProbeRtt {
            // During ProbeRTT, force cwnd to PROBE_RTT_CWND
            self.cwnd = PROBE_RTT_CWND;
        } else if self.bbr.in_startup() {
            // During startup, grow toward BDP target but also allow
            // traditional slow-start growth if BDP estimate is too low
            self.cwnd = std::cmp::max(self.cwnd + acked, bdp_target);
        } else {
            // Steady state: converge toward BDP
            // Smooth transition: move 25% toward target per ACK
            if bdp_target > self.cwnd {
                let step = std::cmp::max(1, (bdp_target - self.cwnd) / 4);
                self.cwnd += step;
            } else if bdp_target < self.cwnd {
                let step = std::cmp::max(1, (self.cwnd - bdp_target) / 4);
                self.cwnd = self.cwnd.saturating_sub(step);
            }
        }

        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);

        // Sync legacy fields
        self.in_slow_start = self.bbr.in_startup();
        if !self.in_slow_start && self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// BBR-style congestion control: handle loss events.
    ///
    /// Unlike AIMD, loss alone does NOT reduce cwnd.  The key insight:
    ///   - Loss + stable RTT → wireless/random loss → ignore (FEC handles it)
    ///   - Loss + rising RTT → real congestion → drain to BDP
    ///   - Decode failure + rising RTT → severe congestion → drain to 0.75 × BDP
    pub fn on_loss(&mut self, fec_recovered: bool) {
        // During ProbeRTT, don't further reduce cwnd
        if self.bbr.phase == BbrPhase::ProbeRtt {
            return;
        }

        if self.bbr.is_congested() {
            // RTT is rising → real congestion
            self.bbr.exit_startup();
            let bdp = self.bbr.bdp_cwnd();

            if fec_recovered {
                // Congestion but FEC saved us — drain to BDP
                self.cwnd = std::cmp::max(bdp, Self::MIN_CWND);
            } else {
                // Decode failure + congestion — aggressive drain to 75% BDP
                let target = (bdp as f64 * 0.75) as u32;
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

    /// Feed an RTT measurement into BBR state.
    /// Call this when processing ACKs/reports that include RTT.
    pub fn record_rtt_sample(&mut self, rtt: Duration) {
        self.bbr.record_rtt(rtt);
    }

    /// Read BBR's current min_rtt estimate (for diagnostics/benchmarking).
    pub fn bbr_min_rtt(&self) -> Option<Duration> {
        self.bbr.min_rtt()
    }
}

/// The multipath scheduler.
pub struct Scheduler {
    paths: HashMap<PathId, PathState>,
    clock: Arc<dyn Clock>,
    enable_probe_rtt: bool,
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            enable_probe_rtt: true,
        }
    }

    pub fn new_with_config(clock: Arc<dyn Clock>, enable_probe_rtt: bool) -> Self {
        Self {
            paths: HashMap::new(),
            clock,
            enable_probe_rtt,
        }
    }

    pub fn add_path(&mut self, id: PathId) {
        self.paths.insert(id, PathState::new(id, self.clock.clone(), self.enable_probe_rtt));
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

    /// Schedule symbols across paths.
    ///
    /// Strategy:
    /// - Source symbols go to the LOWEST LATENCY paths first (minimize time to first byte)
    /// - Repair symbols go to the HIGHEST GOODPUT paths (maximize decode probability)
    /// - Within each category, distribute proportional to available capacity
    ///
    /// Returns: Vec<(PathId, Vec<WireSymbol>)>
    pub fn schedule(
        &mut self,
        source_symbols: Vec<WireSymbol>,
        repair_symbols: Vec<WireSymbol>,
    ) -> Vec<(PathId, Vec<WireSymbol>)> {
        let mut assignments: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        // Sort paths by RTT for source symbol scheduling
        let mut paths_by_rtt: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();
        paths_by_rtt.sort_by(|a, b| a.estimator.rtt().cmp(&b.estimator.rtt()));

        // Distribute source symbols to lowest-latency paths first
        let mut source_iter = source_symbols.into_iter();
        for path in &paths_by_rtt {
            let available = path.available() as usize;
            let batch: Vec<_> = source_iter.by_ref().take(available).collect();
            if batch.is_empty() {
                break;
            }
            assignments
                .entry(path.id)
                .or_default()
                .extend(batch);
        }
        // If source symbols remain, distribute to any available path
        for sym in source_iter {
            if let Some(path) = paths_by_rtt.first() {
                assignments.entry(path.id).or_default().push(sym);
            }
        }

        // Sort paths by goodput for repair symbol scheduling
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

        // Distribute repair symbols proportional to goodput
        if !paths_by_goodput.is_empty() {
            let total_goodput: f64 = paths_by_goodput.iter().map(|p| p.effective_goodput()).sum();
            let mut repair_iter = repair_symbols.into_iter().peekable();

            if total_goodput > 0.0 {
                for path in &paths_by_goodput {
                    let fraction = path.effective_goodput() / total_goodput;
                    // Proportional share (at least 1 if there are symbols left)
                    let count = (fraction * repair_iter.len() as f64).ceil() as usize;
                    let batch: Vec<_> = repair_iter.by_ref().take(count).collect();
                    if !batch.is_empty() {
                        assignments.entry(path.id).or_default().extend(batch);
                    }
                }
            }
            // Remaining repair symbols to best path
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
                path.bbr.reset();
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

    /// Pick the best path for a source symbol: lowest RTT with available capacity.
    pub fn best_source_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .min_by(|a, b| a.estimator.rtt().cmp(&b.estimator.rtt()))
            .map(|p| p.id)
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
            .min_by(|a, b| a.estimator.rtt().cmp(&b.estimator.rtt()))
            .map(|p| p.id)
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
    /// Set enable_probe_rtt for new paths (for benchmarking)
    pub fn set_enable_probe_rtt(&mut self, enable: bool) {
        self.enable_probe_rtt = enable;
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
}
