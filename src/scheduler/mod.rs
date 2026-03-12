//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.

use crate::control::LossEstimator;
use crate::fec::WireSymbol;
use std::collections::HashMap;

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

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
    /// Slow-start threshold
    pub ssthresh: u32,
    /// Whether we are in slow-start phase
    pub in_slow_start: bool,
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

    /// AIMD congestion control: handle acknowledgements.
    ///
    /// In slow start the window grows by `acked` each call (roughly doubles
    /// per RTT). In congestion avoidance we do standard additive increase.
    pub fn on_ack(&mut self, acked: u32) {
        if self.in_slow_start {
            self.cwnd += acked;
            if self.cwnd >= self.ssthresh {
                self.in_slow_start = false;
            }
        } else {
            // Additive increase: +1 per full window acked
            self.cwnd += std::cmp::max(1, acked / self.cwnd);
        }
        self.cwnd = std::cmp::min(self.cwnd, Self::MAX_CWND);
    }

    /// AIMD congestion control: handle loss events.
    ///
    /// FEC-aware: if the block was recovered by the FEC decoder despite the
    /// lost packet, the loss is likely random (wireless) rather than
    /// congestion, so we barely reduce. If the block *failed* to decode,
    /// treat it as a real congestion signal and halve the window.
    pub fn on_loss(&mut self, fec_recovered: bool) {
        if fec_recovered {
            // Random / wireless loss – gentle reduction
            self.cwnd = std::cmp::max(self.cwnd.saturating_sub(1), Self::MIN_CWND);
        } else {
            // Congestion signal – multiplicative decrease
            self.ssthresh = std::cmp::max(self.cwnd / 2, Self::MIN_CWND);
            self.cwnd = self.ssthresh;
            self.in_slow_start = false;
        }
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
