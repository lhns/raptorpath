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

use crate::control::LossEstimator;
use crate::fec::WireSymbol;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

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
    /// Whether we're in startup phase (probe for bandwidth).
    in_startup: bool,
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
}

impl BbrState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            bw_samples: VecDeque::new(),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            cwnd_gain: 2.0, // start with 2x gain for startup probing
            in_startup: true,
            prev_rtt: None,
            rtt_increases: 0,
            congestion_threshold: 3,
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
        }
    }

    /// Record delivery of `count` symbols.  Returns the computed delivery rate.
    fn record_delivery(&mut self, count: u32) -> f64 {
        self.delivered += count as u64;
        let now = Instant::now();
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
        let now = Instant::now();

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

        // Update min RTT
        self.min_rtt = self.rtt_samples.iter().map(|s| s.rtt).min();
    }

    /// Whether RTT is trending upward (congestion detected).
    fn is_congested(&self) -> bool {
        self.rtt_increases >= self.congestion_threshold
    }

    /// Compute the BDP-based cwnd target.
    fn bdp_cwnd(&self) -> u32 {
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
        if self.in_startup {
            self.in_startup = false;
            self.cwnd_gain = 1.0;
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
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
    pub fn new(id: PathId) -> Self {
        Self {
            id,
            estimator: LossEstimator::new(),
            cwnd: Self::INITIAL_CWND,
            in_flight: 0,
            active: true,
            ssthresh: 64,
            in_slow_start: true,
            last_report: Instant::now(),
            max_datagram_size: None,
            bbr: BbrState::new(),
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
        let rate = self.bbr.record_delivery(acked);

        // Startup exit: if delivery rate stopped growing or RTT is rising
        if self.bbr.in_startup {
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

        // Set cwnd based on BDP
        let bdp_target = self.bbr.bdp_cwnd();

        if self.bbr.in_startup {
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
        self.in_slow_start = self.bbr.in_startup;
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
}

/// The multipath scheduler.
pub struct Scheduler {
    paths: HashMap<PathId, PathState>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }

    pub fn add_path(&mut self, id: PathId) {
        self.paths.insert(id, PathState::new(id));
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
            path.last_report = Instant::now();
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
        let now = Instant::now();
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
        Self::new()
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
        }
    }

    #[test]
    fn test_schedule_prefers_low_rtt_for_source() {
        let mut sched = Scheduler::new();
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
}
