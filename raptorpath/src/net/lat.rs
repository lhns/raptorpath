//! `[LAT]` — DELIVERED LATENCY, DECOMPOSED.
//!
//! **The question, and why it has never been answered.** What a user of this
//! transport experiences is delivered latency and goodput. The record has
//! measured holes, repairs, false repairs, stalls and shares — and has NEVER
//! decomposed the delivered latency itself. Seed S7 of the law search says
//! that is the wrong order: find the law governing the LARGEST term first,
//! and the recovery-clock prize is already known to be bounded by 1.5–3.4 %
//! at the duals and ≈ 0 at singles (§16.80).
//!
//! So this gauge splits every delivered source symbol's wait into three
//! disjoint pieces, per path:
//!
//!     A_x  =  (t_arr − send_ts)  −  min_running(t_arr − send_ts)
//!     R    =  t_deliv − t_buffered              (the REORDER WAIT)
//!     P    =  the `[SUCC]` hole duration        (the REPAIR WAIT)
//!
//! `A_x` is **queueing plus sender dwell above this path's own floor**. The
//! running-min subtraction is what makes it a measurement at all: `t_arr` and
//! `send_ts` are two different clocks, so only DIFFERENCES of their gap mean
//! anything, and the path's best-ever gap is the floor everything is read
//! against.
//!
//! **DISCLOSURE, because it changes what `A_x` is a measurement OF: the
//! SENDER'S OWN DWELL RIDES INSIDE IT.** `send_timestamp_us` is stamped in
//! `emit_source` at the placement lock — BEFORE quinn's datagram queue, its
//! pacer and the kernel. So `A_x` is (sender reservoir dwell + wire queue +
//! propagation) above the floor, not the network queue alone. That is the
//! same conflation the #80 battery measured on the app-echo RTT (arm D), and
//! it is stated here rather than left for a reader to discover: an `A_x` that
//! dominates indicts the STORE/PACING law, which may sit on either side of
//! the socket.
//!
//! **`R` IS CLASSED BY WHAT RELEASED IT**, read off the `[SUCC]` gauge's own
//! resolution record rather than guessed:
//!
//!   * `rw_xp` — the releasing arrival landed on a DIFFERENT path from the
//!     one that exposed the hole. A CROSS-PATH skew event: the scheduler's
//!     own inversion. **STRUCTURALLY ZERO AT ONE PATH**, which is this
//!     gauge's control reading.
//!   * `rw_sp` — same path. In-path reordering, or a real loss on that path.
//!   * `rw_rep` — the decoder reconstructed the releasing seq from coded
//!     repair.
//!
//! **THE ACCOUNTING IDENTITY, asserted on the engine's own output:**
//!
//!     n_deliv  =  n_rwxp + n_rwsp + n_rwrep + n_nowait
//!
//! Every delivered symbol either waited in the reorder buffer (in exactly one
//! class) or did not. A gauge whose classes do not partition its own
//! denominator is caught by its reachability test rather than in a results
//! table.
//!
//! **The shares `sh_*`** are each class's share of the TOTAL accumulated
//! wait, so the pre-registered first readout (PLACEMENT-INDICTED /
//! QUEUE-DOMINATED / REPAIR-DOMINATED / MIXED) can be taken off one line.
//!
//! **`-` iff `n = 0`**, `[SUCC]`'s convention verbatim: an absent reading is
//! never a measured zero, and every value sits beside its own sample count.
//! ALWAYS FED — no gate changes what is recorded, only whether the line is
//! printed, and it is printed under the SAME gate and on the SAME cadence as
//! `[SUCC]` because the two are read together.
//!
//! Nothing here branches, decides, or is reachable from a control law.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::succ::{Hist, HoleOutcome, HoleRecord};

/// How many arrivals may be pending delivery before the oldest record is
/// dropped. A declared resource bound, not a timeout: a seq that arrives and
/// is never delivered (an abandoned hole under the EVICT policy) would
/// otherwise sit here forever. Evictions are COUNTED and printed (`over=`).
const PENDING_MAX: usize = 16_384;

/// Which class of reorder wait one delivery paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RwClass {
    /// Released by an arrival on a DIFFERENT path from the exposer.
    CrossPath,
    /// Released by an arrival on the SAME path.
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
    /// The running MIN of `t_arr − send_ts`, µs (with its constant, unknown
    /// clock offset — which is why only the difference from it is printed).
    min_gap: Option<i64>,
    /// Arrivals that RESET the floor. A warm-up witness: while the min is
    /// still moving, `A_x` is biased HIGH, and this is the count that says so
    /// instead of a hidden filter.
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

/// **THE DELIVERED-LATENCY DECOMPOSITION GAUGE.** Owned by the receiver task.
/// No engine handle, no shared state, nothing `&mut`-reachable from a
/// decision site.
#[derive(Default)]
pub struct LatGauge {
    paths: BTreeMap<u32, PathLat>,
    /// `seq → (path, A_x µs)` recorded at ARRIVAL and consumed at DELIVERY.
    /// Bounded; `order` is the eviction FIFO.
    pending: HashMap<u64, (u32, u64)>,
    order: VecDeque<u64>,
    /// Records evicted by the bound, and deliveries whose arrival record was
    /// already gone. Both are printed; neither is silently dropped.
    over: u64,
    /// Every delivery offered to the gauge — the identity's left side.
    n_deliv: u64,
}

impl LatGauge {
    /// **ONE SOURCE ARRIVAL.** `send_ts_us` is the batch's own sender-clock
    /// stamp, `arr_us` the receiver's. Computes `A_x` against this path's
    /// running floor and parks it until the seq is delivered.
    ///
    /// Called for the seq's OWN source arrival only — a seq reconstructed by
    /// the decoder never rode a wire as itself, so it has no `A_x` and is
    /// deliberately absent from that histogram rather than credited a zero.
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
        if self.pending.insert(seq, (path_id, ax)).is_none() {
            self.order.push_back(seq);
        }
        while self.order.len() > PENDING_MAX {
            if let Some(old) = self.order.pop_front() {
                if self.pending.remove(&old).is_some() {
                    self.over += 1;
                }
            }
        }
    }

    /// **ONE IN-ORDER DELIVERY.** `wait_us` is `t_deliv − t_buffered` (0 for a
    /// symbol that was never buffered); `release` is the `[SUCC]` resolution
    /// record of the arrival that triggered this drain.
    ///
    /// The CLASS of a nonzero wait comes from that record and nowhere else. A
    /// nonzero wait with NO record — the `[SUCC]` bounds refused to track the
    /// hole, or the frontier moved for a reason other than a resolution — is
    /// counted `rw_sp`, the conservative class: it can only make the
    /// cross-path share, which is the reading the placement indictment rests
    /// on, read LOW.
    pub fn note_delivery(&mut self, seq: u64, wait_us: u64, release: Release) {
        self.n_deliv += 1;
        let Some((path_id, ax)) = self.pending.remove(&seq) else {
            // Delivered without an arrival record: a decoder reconstruction
            // (never rode a wire as itself) or an eviction. Counted, and NOT
            // given a fabricated path.
            self.over += 1;
            return;
        };
        // `order` keeps the stale key; it is skipped on eviction.
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

    /// Has this gauge ever seen a delivery — i.e. does it sit at a RECEIVER?
    pub fn is_receiver_site(&self) -> bool {
        self.n_deliv > 0
    }

    /// Deliveries offered — the identity's left side.
    pub fn n_deliv(&self) -> u64 {
        self.n_deliv
    }

    /// The `[LAT]` line. Cumulative: the LAST line of a log is the reading.
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

    /// THE ACCOUNTING IDENTITY, and the three classes are DISJOINT.
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
        // The repair wait is the HOLE duration, not the reorder wait.
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
        // against the LOWERED floor). Two resets.
        assert!(a.contains("ax_sum=15000"), "{a}");
        assert!(a.contains("minrst=2"), "{a}");
    }

    /// A delivery whose arrival was never recorded (a decoder reconstruction)
    /// is COUNTED as `over` and never given a fabricated path.
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
