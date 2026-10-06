//! Shared statistics for runtime monitoring.
//!
//! Uses atomics for hot-path updates (no locking on the data path).

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

/// Global stats shared between data path and monitoring endpoint.
pub struct SharedStats {
    pub paths: parking_lot::RwLock<Vec<Arc<PathStats>>>,
    /// Threading P1, D16: the same `Arc<PathStats>` as `paths`, in a dense
    /// write-once table indexed by path id, so the per-datagram / per-ack
    /// readers ([`Self::path_ref`]) take no lock, do no scan and clone no
    /// `Arc` (the shared refcount line the hot tasks used to bounce).
    dense: [std::sync::OnceLock<Arc<PathStats>>; STATS_DENSE_PATHS],
    pub fec: FecStats,
    pub blocks: BlockStats,
    pub uptime_start_us: AtomicU64,
}

/// Path ids `0..STATS_DENSE_PATHS` get a lock-free slot in [`SharedStats`].
/// A resource bound (64 pointers), not a law constant: paths are numbered
/// from 0 in bind order, so every shipped topology is dense; a larger id
/// takes the locking scan, with the same answer.
pub const STATS_DENSE_PATHS: usize = 64;

/// A path's stats as [`SharedStats::path_ref`] returns them: borrowed from
/// the dense table (the hot case), or an owned `Arc` from the locking scan
/// (an id past the table, or a path pushed into `paths` directly).
pub enum PathStatsRef<'a> {
    Dense(&'a PathStats),
    Scanned(Arc<PathStats>),
}

impl std::ops::Deref for PathStatsRef<'_> {
    type Target = PathStats;
    #[inline]
    fn deref(&self) -> &PathStats {
        match self {
            PathStatsRef::Dense(p) => p,
            PathStatsRef::Scanned(p) => p,
        }
    }
}

impl SharedStats {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            paths: parking_lot::RwLock::new(Vec::new()),
            dense: std::array::from_fn(|_| std::sync::OnceLock::new()),
            fec: FecStats::default(),
            blocks: BlockStats::default(),
            uptime_start_us: AtomicU64::new(now),
        }
    }

    /// Add a path to track.
    pub fn add_path(&self, id: u32) {
        let ps = Arc::new(PathStats::new(id));
        let mut paths = self.paths.write();
        // `path()` answers the FIRST entry with this id, so the dense slot
        // is write-once: a repeated id keeps the first, as the scan does.
        if let Some(slot) = self.dense.get(id as usize) {
            let _ = slot.set(ps.clone());
        }
        paths.push(ps);
    }

    /// Get path stats by ID.
    pub fn path(&self, id: u32) -> Option<Arc<PathStats>> {
        let paths = self.paths.read();
        paths.iter().find(|p| p.id == id).cloned()
    }

    /// [`Self::path`] for the hot tasks (threading P1, D16): the dense slot
    /// when the id has one — no lock, no scan, no `Arc` clone — and the
    /// locking scan otherwise. Same answer as `path(id)` in every case.
    #[inline]
    pub fn path_ref(&self, id: u32) -> Option<PathStatsRef<'_>> {
        match self.dense.get(id as usize).and_then(|s| s.get()) {
            Some(p) => Some(PathStatsRef::Dense(p)),
            None => self.path(id).map(PathStatsRef::Scanned),
        }
    }

    /// Every tracked path id, ascending, each once (the first entry wins, as
    /// [`Self::path`] answers). Threading Q2: the receiver's path set.
    pub fn path_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.paths.read().iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The ids whose published `active` flag is set, ascending — the
    /// receiver's `live_paths()` (threading Q2: liveness is the sender's
    /// state, read through the flag the sender publishes, see
    /// [`CrossDirection`]). Dense ids are read lock-free; an id past the
    /// dense table takes the locking scan.
    pub fn live_path_ids(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut overflow = false;
        for (i, slot) in self.dense.iter().enumerate() {
            if let Some(p) = slot.get() {
                if p.active.load(Ordering::Relaxed) {
                    out.push(i as u32);
                }
            }
        }
        // Ids past the dense table (none in any shipped topology).
        for p in self.paths.read().iter() {
            if p.id as usize >= STATS_DENSE_PATHS && p.active.load(Ordering::Relaxed) {
                out.push(p.id);
                overflow = true;
            }
        }
        if overflow {
            out.sort_unstable();
            out.dedup();
        }
        out
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
    /// The peer's `PathReport.loss_rate` × 1e6: the loss the peer observes
    /// on the direction it receives. Monitoring only — never an estimator
    /// input.
    pub peer_loss_rate_e6: AtomicU64,
    /// Threading Q2: what each direction's owner publishes for the other.
    pub xdir: CrossDirection,
}

/// Threading Q2 (plan rule 2: cross-direction reads use published state).
/// The scheduler is split by direction — the sender task owns the TX half
/// (`Scheduler`), the receiver task the RX half (`RxScheduler`) — and each
/// half publishes, per path, exactly the values the other direction reads:
///
/// * RX → TX (written by the receiver): the incoming loss EWMA and the
///   RFC 3550 arrival jitter (mirrored into the sender's estimator,
///   `LossEstimator::set_rx_mirror`), and the last-arrival stamp (liveness:
///   a dead path is revived and its dead-check clock kept by the sender);
/// * TX → RX (written by the sender, after every ack batch and control
///   message it processes): SRTT, RTprop (`min_rtt`), the RTT jitter and σ
///   that size the receiver's hold / deficit / refresh clocks and its
///   `[ETA]` reference — and the `active` flag of [`PathStats`].
///
/// Relaxed stores and loads of independent words: a reader may see one
/// value a publication older than another (no torn value; no reader
/// combines two of them into a law input that a mixed pair would bend
/// beyond the one-publication staleness, which is the declared effect).
#[derive(Default)]
pub struct CrossDirection {
    /// `f64` bits of the receiver's incoming loss EWMA.
    pub rx_loss_bits: AtomicU64,
    /// `f64` bits of the receiver's RFC 3550 arrival jitter, µs.
    pub rx_jitter_bits: AtomicU64,
    /// Wall µs (`net::now_us`) of the last arrival on this path (data or a
    /// liveness Ping); 0 = none yet.
    pub rx_seen_us: AtomicU64,
    /// The sender's SRTT, ns + 1 (0 = not yet published).
    srtt_ns_p1: AtomicU64,
    /// RTprop (`PathState::min_rtt`), ns + 1 (0 = none).
    min_rtt_ns_p1: AtomicU64,
    /// `PathState::rtt_jitter_us`.
    rtt_jitter_us: AtomicU64,
    /// `PathState::rtt_sigma_us` + 1 (0 = none).
    rtt_sigma_us_p1: AtomicU64,
}

/// The TX values the receiver reads, as one load set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TxPublished {
    pub srtt: std::time::Duration,
    pub min_rtt: Option<std::time::Duration>,
    pub rtt_jitter_us: u64,
    pub rtt_sigma_us: Option<u64>,
}

impl CrossDirection {
    /// The receiver's publication (RX → TX).
    #[inline]
    pub fn publish_rx(&self, rx_loss: f64, jitter_us: f64) {
        self.rx_loss_bits.store(rx_loss.to_bits(), Ordering::Relaxed);
        self.rx_jitter_bits.store(jitter_us.to_bits(), Ordering::Relaxed);
    }

    /// `(incoming loss EWMA, arrival jitter µs)` as the receiver last
    /// published them (`(0.0, 0.0)` before any arrival: the estimator's own
    /// initial values).
    #[inline]
    pub fn rx(&self) -> (f64, f64) {
        (
            f64::from_bits(self.rx_loss_bits.load(Ordering::Relaxed)),
            f64::from_bits(self.rx_jitter_bits.load(Ordering::Relaxed)),
        )
    }

    /// The sender's publication (TX → RX).
    #[inline]
    pub fn publish_tx(&self, v: TxPublished) {
        let p1 = |d: std::time::Duration| (d.as_nanos() as u64).saturating_add(1);
        self.srtt_ns_p1.store(p1(v.srtt), Ordering::Relaxed);
        self.min_rtt_ns_p1.store(v.min_rtt.map_or(0, p1), Ordering::Relaxed);
        self.rtt_jitter_us.store(v.rtt_jitter_us, Ordering::Relaxed);
        self.rtt_sigma_us_p1
            .store(v.rtt_sigma_us.map_or(0, |s| s.saturating_add(1)), Ordering::Relaxed);
    }

    /// The sender's last publication; `None` before the first.
    #[inline]
    pub fn tx(&self) -> Option<TxPublished> {
        let s = self.srtt_ns_p1.load(Ordering::Relaxed);
        if s == 0 {
            return None;
        }
        let d = |v: u64| std::time::Duration::from_nanos(v - 1);
        let m = self.min_rtt_ns_p1.load(Ordering::Relaxed);
        let g = self.rtt_sigma_us_p1.load(Ordering::Relaxed);
        Some(TxPublished {
            srtt: d(s),
            min_rtt: (m != 0).then(|| d(m)),
            rtt_jitter_us: self.rtt_jitter_us.load(Ordering::Relaxed),
            rtt_sigma_us: (g != 0).then(|| g - 1),
        })
    }
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
            peer_loss_rate_e6: AtomicU64::new(0),
            xdir: CrossDirection::default(),
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
            peer_loss_rate: self.peer_loss_rate_e6.load(Ordering::Relaxed) as f64 / 1_000_000.0,
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
    /// **The taper copy.** The proactive emission's `P_lost` branch
    /// (`net/emit_source.rs`) spends a correction slot on a copy of the oldest
    /// un-acked seq instead of on a fresh coded symbol. Those copies land at
    /// the receiver as `[RFA] dup_src` exactly as a gap-driven retransmit
    /// does, but they are not gap fires — so without this counter
    /// `dup_src / [FCAUSE] n` over-attributes realized waste to the reactive
    /// loop. Counted at the emission decision, on every arm (unlike the
    /// DIAG-gated `mpd_plost_retx` beside it), so the waste split
    /// {gap-fire copy, taper copy, margin} is readable from a shipped run.
    pub taper_copy: AtomicU64,
    /// Source copies put on the wire in a correction slot: SACK-gap
    /// retransmits, request-law copies and taper copies. Disjoint from
    /// `total_repair_symbols`, which counts genuinely coded repair only.
    pub total_copy_symbols: AtomicU64,
}

/// What one correction-slot handoff carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionKind {
    /// A coded repair symbol (a linear combination over the window).
    Coded,
    /// A copy of an already-sent source symbol (retransmit / taper copy).
    SourceCopy,
}

impl FecStats {
    /// Meter one correction-slot handoff: CODED symbols into
    /// `total_repair_symbols` (`[DIAG] cod=`), source COPIES into
    /// `total_copy_symbols`. `sent` is whether the transport accepted it; a
    /// refused handoff never reached the wire and is not counted.
    pub fn record_correction(&self, kind: CorrectionKind, sent: bool) {
        if !sent {
            return;
        }
        let ctr = match kind {
            CorrectionKind::Coded => &self.total_repair_symbols,
            CorrectionKind::SourceCopy => &self.total_copy_symbols,
        };
        ctr.fetch_add(1, Ordering::Relaxed);
    }
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
    /// The peer's reported incoming loss (monitoring only).
    pub peer_loss_rate: f64,
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

    /// Threading P1, D16: `path_ref` names the very `PathStats` `path` does
    /// — the same allocation, inside and outside the dense table, and for a
    /// repeated id the first one — and `None` exactly where `path` is.
    #[test]
    fn path_ref_is_the_same_stats_as_path() {
        let stats = SharedStats::new();
        for id in [0u32, 1, 63, 64, 1000, 1] {
            stats.add_path(id);
        }
        // A path pushed into `paths` directly (no dense slot) is still found.
        stats.paths.write().push(Arc::new(PathStats::new(7)));
        for id in [0u32, 1, 7, 63, 64, 1000, 2, 65, u32::MAX] {
            let a = stats.path(id);
            let b = stats.path_ref(id);
            assert_eq!(a.is_some(), b.is_some(), "id {id}: presence");
            if let (Some(a), Some(b)) = (a, b) {
                assert!(std::ptr::eq(&*a, &*b), "id {id}: not the same PathStats");
                if id < STATS_DENSE_PATHS as u32 && id != 7 {
                    assert!(matches!(b, PathStatsRef::Dense(_)), "id {id}: dense slot unused");
                }
            }
        }
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

    /// `[DIAG] cod=` reads `total_repair_symbols`: a source COPY must not
    /// land there, and a handoff the transport refused must land nowhere.
    #[test]
    fn corrections_are_metered_by_kind_and_only_when_sent() {
        let fec = FecStats::default();
        fec.record_correction(CorrectionKind::SourceCopy, true);
        assert_eq!(fec.total_repair_symbols.load(Ordering::Relaxed), 0, "a copy is not coded");
        assert_eq!(fec.total_copy_symbols.load(Ordering::Relaxed), 1);
        fec.record_correction(CorrectionKind::Coded, true);
        assert_eq!(fec.total_repair_symbols.load(Ordering::Relaxed), 1);
        fec.record_correction(CorrectionKind::Coded, false);
        fec.record_correction(CorrectionKind::SourceCopy, false);
        assert_eq!(fec.total_repair_symbols.load(Ordering::Relaxed), 1, "failed send not counted");
        assert_eq!(fec.total_copy_symbols.load(Ordering::Relaxed), 1, "failed send not counted");
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
