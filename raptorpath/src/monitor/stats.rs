//! Shared statistics for runtime monitoring.
//!
//! Uses atomics for hot-path updates (no locking on the data path).

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU64, Ordering};
use std::sync::Arc;

/// Global stats shared between data path and monitoring endpoint.
pub struct SharedStats {
    pub paths: parking_lot::RwLock<Vec<Arc<PathStats>>>,
    pub fec: FecStats,
    pub blocks: BlockStats,
    pub uptime_start_us: AtomicU64,
}

impl SharedStats {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            paths: parking_lot::RwLock::new(Vec::new()),
            fec: FecStats::default(),
            blocks: BlockStats::default(),
            uptime_start_us: AtomicU64::new(now),
        }
    }

    /// Add a path to track.
    pub fn add_path(&self, id: u32) {
        let mut paths = self.paths.write();
        paths.push(Arc::new(PathStats::new(id)));
    }

    /// Get path stats by ID.
    pub fn path(&self, id: u32) -> Option<Arc<PathStats>> {
        let paths = self.paths.read();
        paths.iter().find(|p| p.id == id).cloned()
    }

    /// Take a serializable snapshot of all stats.
    pub fn snapshot(&self) -> StatsSnapshot {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        let uptime_secs =
            (now - self.uptime_start_us.load(Ordering::Relaxed)) as f64 / 1_000_000.0;

        let paths = self.paths.read();
        let path_snapshots: Vec<PathSnapshot> = paths.iter().map(|p| p.snapshot()).collect();

        let total_source = self.fec.total_source_symbols.load(Ordering::Relaxed);
        let total_repair = self.fec.total_repair_symbols.load(Ordering::Relaxed);
        let overhead_ratio = if total_source > 0 {
            total_repair as f64 / total_source as f64
        } else {
            0.0
        };

        StatsSnapshot {
            uptime_secs,
            paths: path_snapshots,
            fec: FecSnapshot {
                target_tail_loss: f64::from_bits(
                    self.fec.target_tail_loss_bits.load(Ordering::Relaxed),
                ),
                actual_failure_rate: f64::from_bits(
                    self.fec.actual_failure_rate_bits.load(Ordering::Relaxed),
                ),
                pi_correction: i64_to_f64(self.fec.pi_correction_e3.load(Ordering::Relaxed)),
                overhead_ratio,
                total_source_symbols: total_source,
                total_repair_symbols: total_repair,
            },
            blocks: BlockSnapshot {
                encoded: self.blocks.encoded.load(Ordering::Relaxed),
                decoded_ok: self.blocks.decoded_ok.load(Ordering::Relaxed),
                decoded_fail: self.blocks.decoded_fail.load(Ordering::Relaxed),
                pending: self.blocks.pending.load(Ordering::Relaxed),
            },
        }
    }
}

impl Default for SharedStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-path statistics.
pub struct PathStats {
    pub id: u32,
    pub active: AtomicBool,
    /// Loss rate * 1e6, stored as u64 for atomic access.
    pub loss_rate_e6: AtomicU64,
    pub rtt_us: AtomicU64,
    pub throughput_bps: AtomicU64,
    pub symbols_sent: AtomicU64,
    pub symbols_received: AtomicU64,
    pub cwnd: AtomicU64,
    pub in_flight: AtomicU64,
    pub in_slow_start: AtomicBool,
    /// Interarrival jitter in microseconds (RFC 3550 style)
    pub jitter_us: AtomicU64,
}

impl PathStats {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            active: AtomicBool::new(true),
            loss_rate_e6: AtomicU64::new(0),
            rtt_us: AtomicU64::new(50_000), // 50ms default
            throughput_bps: AtomicU64::new(0),
            symbols_sent: AtomicU64::new(0),
            symbols_received: AtomicU64::new(0),
            cwnd: AtomicU64::new(10),
            in_flight: AtomicU64::new(0),
            in_slow_start: AtomicBool::new(true),
            jitter_us: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> PathSnapshot {
        PathSnapshot {
            id: self.id,
            active: self.active.load(Ordering::Relaxed),
            loss_rate: self.loss_rate_e6.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            rtt_ms: self.rtt_us.load(Ordering::Relaxed) as f64 / 1_000.0,
            throughput_mbps: self.throughput_bps.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            symbols_sent: self.symbols_sent.load(Ordering::Relaxed),
            symbols_received: self.symbols_received.load(Ordering::Relaxed),
            cwnd: self.cwnd.load(Ordering::Relaxed),
            in_flight: self.in_flight.load(Ordering::Relaxed),
            in_slow_start: self.in_slow_start.load(Ordering::Relaxed),
            jitter_us: self.jitter_us.load(Ordering::Relaxed),
        }
    }
}

/// FEC controller statistics.
#[derive(Default)]
pub struct FecStats {
    /// Stored as f64 bits for atomic access.
    pub actual_failure_rate_bits: AtomicU64,
    /// PI correction * 1000 (signed).
    pub pi_correction_e3: AtomicI64,
    /// Target tail loss stored as f64 bits.
    pub target_tail_loss_bits: AtomicU64,
    pub total_source_symbols: AtomicU64,
    pub total_repair_symbols: AtomicU64,
    /// Number of runtime FEC backend switches.
    pub backend_switches: AtomicU64,
    /// Current FEC backend (FecBackend variant mapped to u8).
    pub current_backend: AtomicU8,
}

/// Block decode statistics.
#[derive(Default)]
pub struct BlockStats {
    pub encoded: AtomicU64,
    pub decoded_ok: AtomicU64,
    pub decoded_fail: AtomicU64,
    pub pending: AtomicU64,
}

// --- Serializable snapshots ---

#[derive(Debug, Serialize)]
pub struct StatsSnapshot {
    pub uptime_secs: f64,
    pub paths: Vec<PathSnapshot>,
    pub fec: FecSnapshot,
    pub blocks: BlockSnapshot,
}

#[derive(Debug, Serialize)]
pub struct PathSnapshot {
    pub id: u32,
    pub active: bool,
    pub loss_rate: f64,
    pub rtt_ms: f64,
    pub throughput_mbps: f64,
    pub symbols_sent: u64,
    pub symbols_received: u64,
    pub cwnd: u64,
    pub in_flight: u64,
    pub in_slow_start: bool,
    pub jitter_us: u64,
}

#[derive(Debug, Serialize)]
pub struct FecSnapshot {
    pub target_tail_loss: f64,
    pub actual_failure_rate: f64,
    pub pi_correction: f64,
    pub overhead_ratio: f64,
    pub total_source_symbols: u64,
    pub total_repair_symbols: u64,
}

#[derive(Debug, Serialize)]
pub struct BlockSnapshot {
    pub encoded: u64,
    pub decoded_ok: u64,
    pub decoded_fail: u64,
    pub pending: u64,
}

fn i64_to_f64(v: i64) -> f64 {
    v as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_stats_new() {
        let stats = SharedStats::new();
        assert_eq!(stats.blocks.encoded.load(Ordering::Relaxed), 0);
        assert!(stats.paths.read().is_empty());
    }

    #[test]
    fn test_add_and_get_path() {
        let stats = SharedStats::new();
        stats.add_path(0);
        stats.add_path(1);

        assert!(stats.path(0).is_some());
        assert!(stats.path(1).is_some());
        assert!(stats.path(2).is_none());
    }

    #[test]
    fn test_path_stats_update() {
        let stats = SharedStats::new();
        stats.add_path(0);

        let path = stats.path(0).unwrap();
        path.loss_rate_e6.store(50_000, Ordering::Relaxed); // 5% loss
        path.rtt_us.store(15_000, Ordering::Relaxed); // 15ms

        let snap = path.snapshot();
        assert!((snap.loss_rate - 0.05).abs() < 1e-6);
        assert!((snap.rtt_ms - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_snapshot_serialization() {
        let stats = SharedStats::new();
        stats.add_path(0);
        stats.blocks.encoded.store(100, Ordering::Relaxed);
        stats.blocks.decoded_ok.store(99, Ordering::Relaxed);
        stats.blocks.decoded_fail.store(1, Ordering::Relaxed);
        stats.fec.total_source_symbols.store(5000, Ordering::Relaxed);
        stats.fec.total_repair_symbols.store(500, Ordering::Relaxed);

        let snap = stats.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"encoded\":100"));
        assert!(json.contains("\"decoded_ok\":99"));

        // Overhead ratio should be 0.1 (500/5000)
        assert!((snap.fec.overhead_ratio - 0.1).abs() < 1e-6);
    }
}
