//! `[LAT]` — delivered latency, decomposed.
//!
//! What a user of this transport experiences is delivered latency and
//! goodput. This gauge splits every delivered source symbol's wait into three
//! disjoint pieces, per path, so the law governing the largest term can be
//! found first:
//!
//! ```text
//!     A_x  =  (t_arr − send_ts)  −  min_running(t_arr − send_ts)
//!     R    =  t_deliv − t_buffered              (the REORDER WAIT)
//!     P    =  the `[SUCC]` hole duration        (the REPAIR WAIT)
//! ```
//!
//! `A_x` is queueing plus sender dwell above this path's own floor. `t_arr`
//! and `send_ts` are two different clocks, so only differences of their gap
//! mean anything, and the path's best-ever gap is the floor everything is
//! read against.
//!
//! The sender's own dwell rides inside `A_x`: `send_timestamp_us` is stamped
//! in `emit_source` at the placement lock — before quinn's datagram queue,
//! its pacer and the kernel. So `A_x` is (sender store dwell + wire queue +
//! propagation) above the floor, not the network queue alone, and an `A_x`
//! that dominates indicts the store/pacing law, which may sit on either side
//! of the socket.
//!
//! `R` is classed by what released it, read off the `[SUCC]` gauge's own
//! resolution record:
//!
//!   * `rw_xp` — the releasing arrival landed on a different path from the
//!     one that exposed the hole: a cross-path skew event. Structurally zero
//!     at one path, which is this gauge's control reading.
//!   * `rw_sp` — same path. In-path reordering, or a real loss on that path.
//!   * `rw_rep` — the decoder reconstructed the releasing seq from coded
//!     repair.
//!
//! The accounting identity, asserted on the engine's own output:
//!
//! ```text
//!     n_deliv  =  n_rwxp + n_rwsp + n_rwrep + n_nowait
//! ```
//!
//! Every delivered symbol either waited in the reorder buffer (in exactly one
//! class) or did not.
//!
//! The shares `sh_*` are each class's share of the total accumulated wait,
//! so the dominant term can be read off one line.
//!
//! `-` iff `n = 0`, as in `[SUCC]`: an absent reading is never a measured
//! zero, and every value sits beside its own sample count. Always fed — no
//! gate changes what is recorded, only whether the line is printed, and it is
//! printed under the same gate and on the same cadence as `[SUCC]` because
//! the two are read together.
//!
//! Nothing here branches, decides, or is reachable from a control law.

use std::collections::{BTreeMap, VecDeque};

use super::seq_ring::SeqRing;
use super::succ::{Hist, HoleOutcome, HoleRecord};

/// How many arrivals may be pending delivery before the oldest record is
/// dropped. A declared resource bound, not a timeout: a seq that arrives and
/// is never delivered (an abandoned hole under the EVICT policy) would
/// otherwise sit here forever. Evictions are counted and printed (`over=`).
const PENDING_MAX: usize = 16_384;

/// The widest key span (highest − lowest pending seq) the ring store keeps.
/// The seqs are dense (the sender numbers them 0, 1, 2, …), so a span past
/// this is a stray (a copy of a long-delivered seq, or a jump): the ring would
/// pad one slot per absent key, so the stray is refused instead and counted
/// in `over=`, exactly as an evicted record is. 4 × the pending bound, i.e.
/// at most ~1.5 MB of slots.
const PENDING_SPAN_MAX: u64 = 4 * PENDING_MAX as u64;

/// Which class of reorder wait one delivery paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RwClass {
    /// Released by an arrival on a different path from the exposer.
    CrossPath,
    /// Released by an arrival on the same path.
    SamePath,
    /// Released by a decoder reconstruction.
    Repair,
}

/// The `[SUCC]` resolution record of the hole this delivery was waiting
/// behind. `None` when the releasing arrival closed no tracked hole.
pub type Release = Option<HoleRecord>;

#[derive(Default)]
struct PathLat {
    /// Queue + sender dwell above this path's floor.
    ax: Hist,
    /// The three reorder-wait classes.
    rwxp: Hist,
    rwsp: Hist,
    rwrep: Hist,
    /// The repair wait — the `[SUCC]` hole duration, when the release was a
    /// coded reconstruction.
    rep: Hist,
    /// `A_x + R`, the per-symbol total above the floor.
    tot: Hist,
    /// Deliveries with no reorder wait at all.
    nowait: u64,
    /// Every delivery attributed to this path.
    deliv: u64,
    /// The running min of `t_arr − send_ts`, µs (with its constant, unknown
    /// clock offset — which is why only the difference from it is printed).
    min_gap: Option<i64>,
    /// Arrivals that reset the floor. A warm-up witness: while the min is
    /// still moving, `A_x` is biased high, and this count says so instead of
    /// a hidden filter.
    min_resets: u64,
}

impl PathLat {
    fn sum(&self) -> u64 {
        self.ax
            .sum_us()
            .saturating_add(self.rwxp.sum_us())
            .saturating_add(self.rwsp.sum_us())
            .saturating_add(self.rwrep.sum_us())
            .saturating_add(self.rep.sum_us())
    }
}

/// The arrival records awaiting delivery: a ring over the (dense) seq space
/// instead of a hash map — no hashing, no rehash growth, no per-record
/// allocation on the receiver's per-symbol path (threading R1). Bounded to
/// [`PENDING_MAX`] insertions by `order`, the eviction FIFO (it keeps the
/// stale key of a delivered record; the key is skipped when it comes up).
#[derive(Default)]
struct Pending {
    ring: SeqRing<(u32, u64)>,
    order: VecDeque<u64>,
    /// One past the last in-order delivery. Deliveries are in order, so a
    /// record below it can never be taken: without this floor such a dead
    /// record (a duplicate copy of a delivered seq) pins the ring's front for
    /// its whole FIFO lifetime, and every arrival delivered at once then pads
    /// and trims the ring across the whole gap — O(gap) per message (§20).
    floor: u64,
}

impl Pending {
    /// Record `seq`; returns how many records were lost to a bound (the
    /// eviction FIFO, or the span guard), for the caller's `over=`.
    fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64 {
        if seq < self.floor {
            return 1; // already delivered: dead on arrival, counted
        }
        let mut lost = 0;
        if let (Some((lo, _)), Some(hi)) = (self.ring.first(), self.ring.last_key()) {
            if seq < lo {
                if hi - seq >= PENDING_SPAN_MAX {
                    return 1; // a stray far below the live span
                }
            } else {
                // A jump above: shed the oldest records until the span fits.
                let mut lo = lo;
                while seq.saturating_sub(lo) >= PENDING_SPAN_MAX {
                    self.ring.remove(&lo);
                    lost += 1;
                    match self.ring.first() {
                        Some((l, _)) => lo = l,
                        None => break,
                    }
                }
            }
        }
        if self.ring.insert(seq, v).is_none() {
            self.order.push_back(seq);
        }
        while self.order.len() > PENDING_MAX {
            if let Some(old) = self.order.pop_front() {
                if self.ring.remove(&old).is_some() {
                    lost += 1;
                }
            }
        }
        lost
    }

    fn take(&mut self, seq: u64) -> Option<(u32, u64)> {
        self.ring.remove(&seq)
    }

    /// The in-order delivery of `seq`: its record (if any), and the dead
    /// records below it dropped (returned as lost, for `over=`).
    fn deliver(&mut self, seq: u64) -> (Option<(u32, u64)>, u64) {
        let got = self.take(seq);
        let mut lost = 0;
        if seq >= self.floor {
            self.floor = seq + 1;
            let before = self.ring.len();
            self.ring.prune_below(self.floor);
            lost = (before - self.ring.len()) as u64;
        }
        (got, lost)
    }
}

/// The delivered-latency decomposition gauge. Owned by the receiver task.
/// No engine handle, no shared state, nothing `&mut`-reachable from a
/// decision site.
#[derive(Default)]
pub struct LatGauge {
    paths: BTreeMap<u32, PathLat>,
    /// `seq → (path, A_x µs)` recorded at arrival and consumed at delivery.
    pending: Pending,
    /// Records evicted by the bound, and deliveries whose arrival record was
    /// already gone. Both are printed; neither is silently dropped.
    over: u64,
    /// Every delivery offered to the gauge — the identity's left side.
    n_deliv: u64,
}

impl LatGauge {
    /// One source arrival. `send_ts_us` is the batch's own sender-clock
    /// stamp, `arr_us` the receiver's. Computes `A_x` against this path's
    /// running floor and parks it until the seq is delivered.
    ///
    /// Called for the seq's own source arrival only — a seq reconstructed by
    /// the decoder never rode a wire as itself, so it has no `A_x` and is
    /// absent from that histogram rather than credited a zero.
    pub fn note_arrival(&mut self, seq: u64, path_id: u32, send_ts_us: u64, arr_us: u64) {
        let p = self.paths.entry(path_id).or_default();
        let gap = arr_us as i64 - send_ts_us as i64;
        let floor = match p.min_gap {
            Some(m) if m <= gap => m,
            _ => {
                p.min_resets += 1;
                p.min_gap = Some(gap);
                gap
            }
        };
        let ax = (gap - floor).max(0) as u64;
        self.over += self.pending.insert(seq, (path_id, ax));
    }

    /// One in-order delivery. `wait_us` is `t_deliv − t_buffered` (0 for a
    /// symbol that was never buffered); `release` is the `[SUCC]` resolution
    /// record of the arrival that triggered this drain.
    ///
    /// The class of a nonzero wait comes from that record and nowhere else. A
    /// nonzero wait with no record — the `[SUCC]` bounds refused to track the
    /// hole, or the frontier moved for a reason other than a resolution — is
    /// counted `rw_sp`, the conservative class: it can only make the
    /// cross-path share read low.
    pub fn note_delivery(&mut self, seq: u64, wait_us: u64, release: Release) {
        self.n_deliv += 1;
        // The floor's premise: deliveries are in order and non-decreasing.
        debug_assert!(seq >= self.pending.floor, "[LAT] delivery {seq} below the floor {}", self.pending.floor);
        let (got, dead) = self.pending.deliver(seq);
        self.over += dead;
        let Some((path_id, ax)) = got else {
            // Delivered without an arrival record: a decoder reconstruction
            // (never rode a wire as itself) or an eviction. Counted, and not
            // given a fabricated path.
            self.over += 1;
            return;
        };
        let p = self.paths.entry(path_id).or_default();
        p.deliv += 1;
        p.ax.add(ax);
        p.tot.add(ax.saturating_add(wait_us));
        if wait_us == 0 {
            p.nowait += 1;
        } else {
            match Self::class_of(release) {
                RwClass::CrossPath => p.rwxp.add(wait_us),
                RwClass::SamePath => p.rwsp.add(wait_us),
                RwClass::Repair => p.rwrep.add(wait_us),
            }
        }
        if let Some(r) = release {
            if r.outcome == HoleOutcome::Repair {
                p.rep.add(r.us);
            }
        }
    }

    fn class_of(release: Release) -> RwClass {
        match release {
            Some(r) if r.outcome == HoleOutcome::Repair => RwClass::Repair,
            Some(r) if r.cross => RwClass::CrossPath,
            _ => RwClass::SamePath,
        }
    }

    /// Has this gauge ever seen a delivery — i.e. does it sit at a receiver?
    pub fn is_receiver_site(&self) -> bool {
        self.n_deliv > 0
    }

    /// Deliveries offered — the identity's left side.
    pub fn n_deliv(&self) -> u64 {
        self.n_deliv
    }

    /// The `[LAT]` line. Cumulative: the last line of a log is the reading.
    /// The whole per-path body renders `-` when the gauge has nothing.
    pub fn line(&self) -> String {
        let q = |h: &Hist, p: f64| h.quantile(p).map_or_else(|| "-".to_string(), |v| v.to_string());
        let mut s = format!("[LAT] site=receiver n={} over={}", self.n_deliv, self.over);
        if self.paths.is_empty() {
            s.push_str(" -");
            return s;
        }
        for (id, p) in &self.paths {
            let tot = p.sum().max(1);
            let sh = |h: &Hist| format!("{:.4}", h.sum_us() as f64 / tot as f64);
            let cls = |name: &str, h: &Hist| {
                format!(
                    "{name}_n={} {name}_p50={} {name}_p95={} {name}_sum={}",
                    h.n(),
                    q(h, 0.50),
                    q(h, 0.95),
                    h.sum_us(),
                )
            };
            s.push_str(&format!(
                " p{}:n={} nowait={} minrst={} ax_n={} ax_p50={} ax_p90={} ax_p95={} \
                 ax_p99={} ax_sum={} {} {} {} {} tot_p50={} tot_p95={} tot_p99={} \
                 sh_ax={} sh_rwxp={} sh_rwsp={} sh_rwrep={} sh_rep={}",
                id,
                p.deliv,
                p.nowait,
                p.min_resets,
                p.ax.n(),
                q(&p.ax, 0.50),
                q(&p.ax, 0.90),
                q(&p.ax, 0.95),
                q(&p.ax, 0.99),
                p.ax.sum_us(),
                cls("rwxp", &p.rwxp),
                cls("rwsp", &p.rwsp),
                cls("rwrep", &p.rwrep),
                cls("rep", &p.rep),
                q(&p.tot, 0.50),
                q(&p.tot, 0.95),
                q(&p.tot, 0.99),
                sh(&p.ax),
                sh(&p.rwxp),
                sh(&p.rwsp),
                sh(&p.rwrep),
                sh(&p.rep),
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::{RngCore, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use std::collections::HashMap;

    /// The store this ring replaced (`HashMap` + eviction FIFO), kept as the
    /// reference the ring is differenced against.
    #[derive(Default)]
    struct RefPending {
        map: HashMap<u64, (u32, u64)>,
        order: VecDeque<u64>,
    }

    impl RefPending {
        fn insert(&mut self, seq: u64, v: (u32, u64)) -> u64 {
            let mut lost = 0;
            if self.map.insert(seq, v).is_none() {
                self.order.push_back(seq);
            }
            while self.order.len() > PENDING_MAX {
                if let Some(old) = self.order.pop_front() {
                    if self.map.remove(&old).is_some() {
                        lost += 1;
                    }
                }
            }
            lost
        }

        fn take(&mut self, seq: u64) -> Option<(u32, u64)> {
            self.map.remove(&seq)
        }
    }

    /// Within the span guard the ring is the map: the same record comes back
    /// for every delivery and the same number of records is lost to the
    /// bound, over random arrival orders with duplicates, holes (seqs that
    /// are never delivered, so the FIFO bound binds) and out-of-order
    /// deliveries.
    #[test]
    fn the_ring_store_matches_the_hash_map_it_replaced() {
        for seed in 0..12u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let (mut ring, mut map) = (Pending::default(), RefPending::default());
            let mut head = 0u64;
            let (mut lost_ring, mut lost_map) = (0u64, 0u64);
            for step in 0..60_000u32 {
                let r = rng.next_u32();
                if r % 3 != 0 {
                    // An arrival: usually the next seq, sometimes a repeat or
                    // one a little behind (a cross-path duplicate).
                    let seq = match r % 11 {
                        0 => head.saturating_sub((rng.next_u32() % 64) as u64),
                        _ => {
                            head += 1 + (rng.next_u32() % 2) as u64;
                            head
                        }
                    };
                    let v = ((rng.next_u32() % 3) as u32, rng.next_u32() as u64);
                    lost_ring += ring.insert(seq, v);
                    lost_map += map.insert(seq, v);
                } else {
                    // A delivery near the head (in order or not), or of a
                    // seq with no record.
                    let seq = head.saturating_sub((rng.next_u32() % 400) as u64);
                    assert_eq!(ring.take(seq), map.take(seq), "seed {seed} step {step} take {seq}");
                }
                // Absolute: the same set of records is held, so an evicted record
                // left behind (which no probe near the head would find) fails here.
                assert_eq!(ring.ring.len(), map.map.len(), "seed {seed} step {step}: records held");
            }
            assert_eq!(lost_ring, lost_map, "seed {seed}: records lost to the bound");
            assert!(lost_ring > 0, "seed {seed}: the eviction bound must have bound");
        }
    }

    /// Cost, not content (status 20): on an in-order stream delivered at once,
    /// a dead record (a duplicate copy of an already-delivered seq) must not
    /// pin the ring's front. Absolute bounds, not a comparison with the map:
    /// the ring spans one slot after each insert, and a delivery leaves no
    /// record below it. A pinned front makes every pair pad and trim across
    /// the whole gap, O(gap) per message.
    #[test]
    fn a_dead_duplicate_does_not_make_the_store_cost_o_gap() {
        let mut g = LatGauge::default();
        let span = |g: &LatGauge| match (g.pending.ring.first(), g.pending.ring.last_key()) {
            (Some((lo, _)), Some(hi)) => hi - lo + 1,
            _ => 0,
        };
        for s in 0..50_000u64 {
            g.note_arrival(s, 0, s * 10, s * 10 + 500);
            assert!(span(&g) <= 2, "seq {s}: ring slot span {} after an in-order insert", span(&g));
            g.note_delivery(s, 0, None);
            assert!(
                g.pending.ring.first().map_or(true, |(lo, _)| lo >= s),
                "seq {s}: a record below the delivered seq is held"
            );
            if s % 5_600 == 5_599 {
                // A duplicate source copy of a seq delivered 1 000 earlier.
                g.note_arrival(s - 1_000, 1, 0, 0);
            }
        }
        assert_eq!(g.n_deliv(), 50_000);
    }

    /// Past the span guard the ring refuses and counts; it never hands back a
    /// record for a seq it was not given, and the live window stays exact.
    #[test]
    fn a_stray_far_from_the_live_span_is_counted_not_padded() {
        let mut p = Pending::default();
        for s in 1_000_000..1_000_100u64 {
            assert_eq!(p.insert(s, (1, s)), 0);
        }
        // A copy of a long-delivered seq: refused, counted, no allocation of
        // a million empty slots.
        assert_eq!(p.insert(10, (9, 9)), 1);
        assert_eq!(p.take(10), None);
        // A jump far above sheds the old records (counted), keeps the new.
        let lost = p.insert(1_000_000 + 10 * PENDING_SPAN_MAX, (2, 7));
        assert_eq!(lost, 100);
        assert_eq!(p.take(1_000_050), None);
        assert_eq!(p.take(1_000_000 + 10 * PENDING_SPAN_MAX), Some((2, 7)));
        assert!(p.ring.iter().count() == 0);
    }

    /// The accounting identity, and the three classes are disjoint.
    #[test]
    fn the_classes_partition_the_deliveries() {
        let mut g = LatGauge::default();
        assert!(!g.is_receiver_site());
        assert_eq!(g.line(), "[LAT] site=receiver n=0 over=0 -");
        // Four deliveries on path 0: one of each wait class, one with none.
        let rec = |outcome, cross, us| Some(HoleRecord { seq: 0, outcome, cross, us, hi_us: us });
        for (i, (wait, rel)) in [
            (0u64, None),
            (1_000, rec(HoleOutcome::Original, true, 900)),
            (2_000, rec(HoleOutcome::Original, false, 1_800)),
            (3_000, rec(HoleOutcome::Repair, false, 2_700)),
        ]
        .into_iter()
        .enumerate()
        {
            let seq = i as u64;
            g.note_arrival(seq, 0, 0, 10_000 + 100 * seq);
            g.note_delivery(seq, wait, rel);
        }
        assert!(g.is_receiver_site());
        let l = g.line();
        assert!(l.starts_with("[LAT] site=receiver n=4 over=0"), "{l}");
        assert!(l.contains("nowait=1"), "{l}");
        assert!(l.contains("rwxp_n=1"), "{l}");
        assert!(l.contains("rwsp_n=1"), "{l}");
        assert!(l.contains("rwrep_n=1"), "{l}");
        // The repair wait is the hole duration, not the reorder wait.
        assert!(l.contains("rep_n=1 rep_p50=2560 rep_p95=2560 rep_sum=2700"), "{l}");
        // n_deliv = Σ_class n + n_nowait, exactly.
        assert_eq!(g.n_deliv(), 4);
    }

    /// `A_x` is offset-free and the floor is the path's own running min.
    #[test]
    fn ax_is_measured_against_the_paths_own_floor() {
        let read = |offset: u64| {
            let mut g = LatGauge::default();
            for (i, gap) in [30_000u64, 25_000, 40_000].into_iter().enumerate() {
                let seq = i as u64;
                g.note_arrival(seq, 1, 1_000, 1_000 + gap + offset);
                g.note_delivery(seq, 0, None);
            }
            g.line()
        };
        let a = read(0);
        assert_eq!(a, read(7_000_000), "a constant clock offset moved A_x");
        // Floor walks 30 000 → 25 000; A_x = 0, 0, 15 000 (the third is read
        // against the lowered floor). Two resets.
        assert!(a.contains("ax_sum=15000"), "{a}");
        assert!(a.contains("minrst=2"), "{a}");
    }

    /// A delivery whose arrival was never recorded (a decoder reconstruction)
    /// is counted as `over` and never given a fabricated path.
    #[test]
    fn a_delivery_without_an_arrival_record_is_counted_not_invented() {
        let mut g = LatGauge::default();
        g.note_delivery(
            99,
            5_000,
            Some(HoleRecord { seq: 99, outcome: HoleOutcome::Repair, cross: false, us: 1, hi_us: 1 }),
        );
        let l = g.line();
        assert!(l.starts_with("[LAT] site=receiver n=1 over=1"), "{l}");
        assert!(l.ends_with(" -"), "no path slot may be invented: {l}");
    }

    /// Every quantile of an empty class renders `-`, never 0.
    #[test]
    fn absent_classes_render_dash_and_never_zero() {
        let mut g = LatGauge::default();
        g.note_arrival(0, 3, 0, 100);
        g.note_delivery(0, 0, None);
        let l = g.line();
        for k in ["rwxp_p50=-", "rwxp_p95=-", "rwsp_p50=-", "rwrep_p50=-", "rep_p50=-"] {
            assert!(l.contains(k), "`{k}` missing from {l}");
        }
        assert!(l.contains("rwxp_n=0 rwxp_p50=- rwxp_p95=- rwxp_sum=0"), "{l}");
    }
}
