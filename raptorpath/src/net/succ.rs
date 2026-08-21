//! `[SUCC]` — THE SAME-FLOW SUCCESSOR-ARRIVAL DISTRIBUTION, MEASURED AT THE
//! RECEIVER. The quantity the fire-cause pass NAMED and did not characterize.
//!
//! ## WHY THIS EXISTS — the one reading the spine is owed
//!
//! The fire-cause pass (goal-gate, "THE FIRE-CAUSE PASS — THE SCORED RESULT",
//! 2026-08-21) classified 107 597 recovery fires and found **0.59 % of them
//! timer-driven and 98.99 % `gap_data`** — the receiver's SACK report, emitted
//! when *a higher seq arrives while a hole is outstanding*. It closed by naming
//! the successor measurand and by naming, explicitly, what it had NOT done:
//!
//! > *"the successor-arrival distribution has never been measured on this
//! > engine. §16.69's measurand was wrong; this pass names its replacement but
//! > does not characterize it. A derivation written against an uncharacterized
//! > distribution would repeat the exact defect just corrected."*
//!
//! This module is that characterization, and NOTHING else. **It derives no
//! law, positions no waiting time, and hands nothing to any consumer.** It
//! measures, per hole, how long the hole lives and what closes it.
//!
//! ## THE ORIGIN EVENT — hole DETECTION, and why not hole CREATION
//!
//! A hole has two candidate origins and the choice is stated rather than
//! defaulted:
//!
//!   * **CREATION** — the instant the sender put the symbol on the wire, or
//!     the instant the network dropped it. **NOT RECEIVER-OBSERVABLE.** The
//!     receiver never sees the lost symbol, so it holds no send-timestamp for
//!     it and no arrival to subtract one from. Measuring from creation would
//!     require the sender's clock and a wire change, and would measure a
//!     quantity the fire site cannot condition on.
//!   * **DETECTION** — the first arrival of a *strictly higher* seq while this
//!     seq is unresolved. This is what this gauge uses.
//!
//! **DETECTION IS NOT A CONVENIENCE CHOICE; IT IS THE SAME EVENT THE MAJORITY
//! CAUSE FIRES ON.** `gap_data`'s producer in `receiver.rs` is
//!
//! ```text
//!     gap_report_due = highest_seen_seq > highest_delivered_seq
//!                   && highest_seen_seq > last_gap_ack_seen
//!                   && last_gap_ack_time.elapsed() >= GAP_ACK_MIN_INTERVAL
//! ```
//!
//! — a higher seq arrived while a hole was outstanding, i.e. **detection**. A
//! waiting time positioned on a clock that starts at detection is positioned
//! on the same origin as 99 % of the fires it is supposed to govern. A clock
//! started at creation would be positioned on an origin the deciding site
//! cannot observe. The gauge's own high-water mark is fed by exactly the
//! arrivals that feed `highest_seen_seq`, so the two advance together by
//! construction.
//!
//! **The consequence, disclosed:** every duration reported here EXCLUDES the
//! interval from creation to detection. This is a **lower bound on hole age**
//! and an **exact measure of the conditional the fire site sees**. Those are
//! different quantities and the second is the one under study.
//!
//! ## THE THREE OUTCOMES — disjoint by construction, first terminal event wins
//!
//!   * `orig` — **the seq's own SOURCE symbol arrived.** A late reorder, or a
//!     retransmit; the receiver cannot tell them apart (the wire carries
//!     `is_repair` but no "this is a retransmit" bit — the same contamination
//!     `[RFA]`'s `fill_src` discloses, restated here rather than assumed
//!     inherited).
//!   * `rep` — **the seq came out of the DECODER**, reconstructed from coded
//!     repair rather than from its own source arrival. The same test the
//!     `[RFA]` site already uses for `fill_coded`: `symbol.is_repair ||
//!     seq != symbol.block_id`.
//!   * `aban` — **the in-order delivery frontier moved past the hole while it
//!     was still open.** Force-delivery / give-up. Under the reliable window
//!     (ρ = 1) `ReorderBuffer::new_reliable` never expires a hole, so this
//!     class is STRUCTURALLY EMPTY there and a nonzero reading is a finding
//!     about the engine, not about this gauge.
//!
//! A hole that is still outstanding when the line is emitted is in NONE of the
//! three: it is counted in `open`, a CENSUS and not an outcome. The gauge is
//! read cumulatively (last line wins) at a site the harness SIGKILLs, so
//! "still open at the last line" is the honest reading of *window close* and
//! is reported as its own slot rather than folded into `aban`.
//!
//! **THE ACCOUNTING IDENTITY, asserted by test rather than asserted in prose:**
//!
//! ```text
//!     det = orig_n + rep_n + aban_n + open + over
//! ```
//!
//! `over` is the declared resource bound made visible: holes detected while
//! the tracking map was at [`MAX_OPEN`], or exposed by a seq jump wider than
//! [`MAX_SPAN`]. They are counted in `det` and never tracked, so the identity
//! holds and a truncated measurement announces itself instead of quietly
//! shrinking its own denominator.
//!
//! ## WHAT IS REPORTED, AND WHY QUANTILES RATHER THAN A MEAN
//!
//! A waiting time is a QUANTILE decision — "wait until the successor has
//! probably arrived" — so the mean is the wrong summary and is not reported as
//! a headline. Per outcome: `n`, `p50`, `p90`, `p99`, `mx`, all in µs. Plus
//! two derived readings the next step's derivation is pre-registered to need:
//!
//!   * **`orig_frac`** = `orig_n / (orig_n + rep_n)` — of the holes that
//!     resolved, the fraction the ORIGINAL closed. The false-repair boundary
//!     in the large: a repair emitted for a hole whose original was coming was
//!     unnecessary.
//!   * **`cross`** — **the FALSE-REPAIR BOUNDARY IN TIME.** Defined here, in
//!     advance, so the number is not chosen after seeing the data: `cross` is
//!     the smallest histogram-bucket lower edge `t` at which the count of
//!     holes closed by a REPAIR within `t` strictly exceeds the count closed
//!     by their ORIGINAL within `t`. Below `cross`, waiting pays — most holes
//!     that close, close by themselves. Above it, waiting does not. It renders
//!     `-` when no such `t` exists, which reads "the original is ahead at
//!     every horizon" and is a legal outcome, not a missing value.
//!
//! ## THE HISTOGRAM, AND ITS DECLARED ERROR
//!
//! Counts land in log-spaced buckets, **8 sub-buckets per octave**, exact
//! below 8 µs. A bucket's relative width above that is `2^(1/8) − 1 ≈ 9.05 %`,
//! and a reported quantile is the **lower edge** of the bucket the rank falls
//! in — so every quantile here is an underestimate by at most 9.05 %. Bounded,
//! stated, and pinned by test. Memory is [`BUCKETS`] × 8 B × 3 outcomes ≈ 12 kB
//! for the whole run, independent of hole count: this gauge cannot grow with
//! the transfer.
//!
//! ## THE RAW DUMP — default OFF, for the offline derivation only
//!
//! `RWM_SUCC_DUMP=1` additionally emits `[SUCCDUMP]` batches of raw
//! `(outcome, µs)` records so the next step can compute any functional over
//! the exact samples rather than over this gauge's buckets — the `[RTTDUMP]`
//! lesson, applied before it is needed rather than after a battery is scored
//! against a bucket edge. It is capped ([`dump_max`], `RWM_SUCC_DUMP_MAX`) and
//! announces its own truncation with one `[SUCCDUMP-CAP]` line. **The quantile
//! line is emitted ALWAYS; only the dump is gated**, so no pass depends on the
//! dump being on and no pass pays for it unless it asked.
//!
//! ## ONE WINDOW PER INVOCATION — a disclosed scope, not an assumption
//!
//! The gauge's high-water mark and its open-hole map are per-RECEIVER-TASK, and
//! the receiver task outlives an individual perf RUN. A multi-run invocation
//! (`--runs N`, N > 1) restarts the window's seq space while the gauge's mark
//! stays at the previous run's maximum, so the second run's arrivals advance no
//! mark, expose no hole, and can make the previous run's trailing holes look
//! abandoned when the delivery frontier resets. **MEASURED, and it is why the
//! reachability run's `--runs 2` shows `aban_n = 1` under a reliable window
//! that cannot abandon a hole.**
//!
//! The consequence is stated rather than engineered around: **`[SUCC]` is a
//! ONE-RUN-PER-INVOCATION instrument**, exactly as the L1 batteries invoke the
//! engine (`perf_rwm_c.sh … 1`). A pass that runs more than one transfer per
//! invocation is reading a gauge across a discontinuity its own denominator
//! does not survive, and the pre-registration says so before the pass rather
//! than a results table saying so after it.
//!
//! ## READ-ONLY
//!
//! Every counter is fed from events the receiver already produces. The gauge
//! holds no engine handle, returns nothing any engine site reads, and no
//! branch anywhere in the tree tests any value it computes. Pinned by
//! [`tests::succ_is_observation_only`].

use std::collections::BTreeMap;
use std::time::Instant;

// ── DECLARED RESOURCE BOUNDS ────────────────────────────────────────────

/// Maximum simultaneously-tracked open holes. Beyond this a detection is
/// counted in `det` and `over` and never tracked, so the accounting identity
/// holds and the truncation is READ off the line rather than inferred.
///
/// At the widest cell of the pass (`c1`, 400 MB, 1400 B symbols) the in-flight
/// span is bounded by the sender's outstanding cap, so this is ~2 orders above
/// any reachable simultaneous-hole count. It exists so that a pathological run
/// cannot make an observation-only gauge the reason for an OOM.
pub const MAX_OPEN: usize = 65_536;

/// Maximum seq span one arrival may expose as holes in a single step. A jump
/// wider than this is counted whole into `det` and `over` without enumeration,
/// which bounds the per-arrival cost of the gauge at O(MAX_SPAN) and its
/// typical cost at O(1) — the engine's seqs are dense, so the realized span is
/// 1 on every ordinary arrival.
pub const MAX_SPAN: u64 = 4_096;

/// Sub-buckets per octave, as a power of two. 8 ⇒ ≤ 9.05 % relative bucket
/// width, and the reported quantile is the bucket's LOWER edge.
const SUB_BITS: u32 = 3;
const SUB: u64 = 1 << SUB_BITS;

/// Histogram bucket count. Covers every `u64` µs value: the largest index a
/// `u64` can produce is `((63 - SUB_BITS + 1) << SUB_BITS) + SUB - 1 = 495`.
pub const BUCKETS: usize = 512;

/// Raw records per emitted `[SUCCDUMP]` line.
const DUMP_BATCH: usize = 256;

/// Default cap on dumped records (`RWM_SUCC_DUMP_MAX`).
pub const DUMP_MAX_DEFAULT: u64 = 200_000;

/// The resolved raw-dump cap — echoed on the `[GATES]` line so a truncated
/// dump is readable off the run's own output rather than inferred.
pub fn dump_max() -> u64 {
    std::env::var("RWM_SUCC_DUMP_MAX")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DUMP_MAX_DEFAULT)
}

// ── THE BUCKET MAP ──────────────────────────────────────────────────────

/// Bucket index for `v` µs. Monotone non-decreasing in `v`, and EXACT (index
/// == value) below [`SUB`].
pub fn bucket_of(v: u64) -> usize {
    if v < SUB {
        return v as usize;
    }
    let e = 63 - v.leading_zeros(); // ≥ SUB_BITS
    let hi = ((e - SUB_BITS + 1) as u64) << SUB_BITS;
    let lo = (v >> (e - SUB_BITS)) & (SUB - 1);
    (hi + lo) as usize
}

/// The smallest µs value that lands in bucket `i` — what a quantile reports.
pub fn bucket_lower_edge(i: usize) -> u64 {
    let i = i as u64;
    if i < SUB {
        return i;
    }
    let e = (i >> SUB_BITS) + SUB_BITS as u64 - 1;
    let sub = i & (SUB - 1);
    (SUB + sub) << (e - SUB_BITS as u64)
}

// ── ONE OUTCOME'S DISTRIBUTION ──────────────────────────────────────────

/// Which terminal event closed a hole. A LABEL: nothing in the engine
/// branches on it, only counters read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoleOutcome {
    /// The seq's OWN source symbol arrived — a late reorder or a retransmit,
    /// indistinguishable at the receiver.
    Original,
    /// The decoder reconstructed the seq from coded repair.
    Repair,
    /// The in-order delivery frontier moved past the hole while it was still
    /// open — force-delivery / give-up.
    Abandoned,
}

impl HoleOutcome {
    /// The one-character tag used in the raw dump.
    pub fn tag(self) -> char {
        match self {
            HoleOutcome::Original => 'o',
            HoleOutcome::Repair => 'r',
            HoleOutcome::Abandoned => 'a',
        }
    }
}

/// A bounded log-bucket histogram over µs, plus exact `n`, `max` and `sum`.
#[derive(Clone)]
pub struct Hist {
    buckets: Box<[u64; BUCKETS]>,
    n: u64,
    max_us: u64,
    sum_us: u64,
}

impl Default for Hist {
    fn default() -> Self {
        Self { buckets: Box::new([0; BUCKETS]), n: 0, max_us: 0, sum_us: 0 }
    }
}

impl Hist {
    /// Record one sample.
    pub fn add(&mut self, us: u64) {
        self.buckets[bucket_of(us)] += 1;
        self.n += 1;
        self.max_us = self.max_us.max(us);
        self.sum_us = self.sum_us.saturating_add(us);
    }

    pub fn n(&self) -> u64 {
        self.n
    }
    pub fn max_us(&self) -> u64 {
        self.max_us
    }
    pub fn mean_us(&self) -> Option<u64> {
        (self.n > 0).then(|| self.sum_us / self.n)
    }

    /// The `p`-quantile as the LOWER EDGE of the bucket the rank falls in —
    /// an underestimate by at most one bucket width (≤ 9.05 %). `None` iff no
    /// sample was ever recorded, which the line renders `-`.
    pub fn quantile(&self, p: f64) -> Option<u64> {
        if self.n == 0 {
            return None;
        }
        // 1-based rank, at least 1, at most n.
        let rank = ((p * self.n as f64).ceil() as u64).clamp(1, self.n);
        let mut cum = 0u64;
        for (i, &c) in self.buckets.iter().enumerate() {
            cum += c;
            if cum >= rank {
                return Some(bucket_lower_edge(i));
            }
        }
        Some(self.max_us)
    }
}

// ── THE GAUGE ───────────────────────────────────────────────────────────

/// The receiver-site successor-arrival gauge. Owned by the receiver task; no
/// engine handle, no shared state, no `&mut` reachable from any decision site.
pub struct SuccGauge {
    /// Open holes: seq → the instant it was DETECTED. `BTreeMap` because the
    /// abandonment sweep is a frontier range, which a hash map cannot do
    /// without scanning the whole map on every frontier advance.
    open: BTreeMap<u64, Instant>,
    /// The gauge's own high-water seq mark. `None` until the first arrival —
    /// the flow's first symbol exposes no hole, it establishes the baseline.
    hi: Option<u64>,
    /// Per-outcome distributions, indexed by `HoleOutcome`.
    orig: Hist,
    rep: Hist,
    aban: Hist,
    /// Every hole ever DETECTED, tracked or not — the identity's left side.
    det: u64,
    /// Detections the bounds refused to track.
    over: u64,
    /// Is generation coding on at this receiver? Echoed as `[SUCC] gen=`: under
    /// generation every arrival is coded, the `orig` class is structurally
    /// empty, and the line must say which machine it measured on its face —
    /// the `[RFA]` / `[FCAUSE]` convention, not a new one.
    gen: bool,
    /// Raw dump state (`RWM_SUCC_DUMP`).
    dump_on: bool,
    dump_cap: u64,
    dumped: u64,
    dump_pending: Vec<(char, u64)>,
    dump_capped_announced: bool,
}

impl SuccGauge {
    /// `gen` — generation coding at this receiver, echoed on the line.
    /// `dump_on` / `dump_cap` — the raw-dump gate and its resolved cap.
    pub fn new(gen: bool, dump_on: bool, dump_cap: u64) -> Self {
        Self {
            open: BTreeMap::new(),
            hi: None,
            orig: Hist::default(),
            rep: Hist::default(),
            aban: Hist::default(),
            det: 0,
            over: 0,
            gen,
            dump_on,
            dump_cap,
            dumped: 0,
            dump_pending: Vec::new(),
            dump_capped_announced: false,
        }
    }

    /// **RESOLUTION.** One seq has just been resolved — by its own source
    /// arrival (`by_repair = false`) or by the decoder (`by_repair = true`).
    /// A no-op unless the seq is an open, tracked hole, which is what makes
    /// the three outcomes disjoint: the FIRST terminal event wins and every
    /// later observation of the same seq falls through.
    ///
    /// **Call this for every seq of an arrival BEFORE [`Self::observe_high`]
    /// for any of them.** One `add_symbol` can emit several seqs in arbitrary
    /// order; resolving the whole batch first is what stops a batch that
    /// decodes `[10, 8]` from opening a hole for 8 and closing it at 0 µs.
    pub fn resolve(&mut self, seq: u64, by_repair: bool, now: Instant) {
        let Some(t0) = self.open.remove(&seq) else {
            return;
        };
        let us = now.saturating_duration_since(t0).as_micros() as u64;
        let outcome =
            if by_repair { HoleOutcome::Repair } else { HoleOutcome::Original };
        self.record(outcome, us);
    }

    /// **DETECTION.** One seq has arrived. Every seq strictly between the
    /// current high-water mark and `seq` has, by definition of a high-water
    /// mark, never been seen — so each is a hole this arrival has just
    /// EXPOSED, and each is stamped now.
    pub fn observe_high(&mut self, seq: u64, now: Instant) {
        let Some(hi) = self.hi else {
            // The flow's first arrival establishes the baseline and exposes
            // nothing: there is no "outstanding hole" below a mark that does
            // not exist yet.
            self.hi = Some(seq);
            return;
        };
        if seq <= hi {
            return;
        }
        let span = seq - hi - 1;
        self.hi = Some(seq);
        if span == 0 {
            return;
        }
        self.det = self.det.saturating_add(span);
        if span > MAX_SPAN {
            // Bound the per-arrival cost. Counted whole, tracked not at all.
            self.over = self.over.saturating_add(span);
            return;
        }
        for s in (hi + 1)..seq {
            if self.open.len() >= MAX_OPEN {
                self.over = self.over.saturating_add(seq - s);
                return;
            }
            self.open.insert(s, now);
        }
    }

    /// **ABANDONMENT.** The in-order delivery frontier has advanced to
    /// `frontier` (the next seq that will be delivered). Every open hole
    /// strictly below it was passed over undelivered — given up.
    pub fn abandon_below(&mut self, frontier: u64, now: Instant) {
        if self.open.is_empty() {
            return;
        }
        // `split_off` leaves the below-frontier prefix behind and returns the
        // rest, so the sweep costs O(k log n) in the number ABANDONED rather
        // than O(n) in the number open.
        let keep = self.open.split_off(&frontier);
        let gone = std::mem::replace(&mut self.open, keep);
        for (_, t0) in gone {
            let us = now.saturating_duration_since(t0).as_micros() as u64;
            self.record(HoleOutcome::Abandoned, us);
        }
    }

    fn record(&mut self, outcome: HoleOutcome, us: u64) {
        match outcome {
            HoleOutcome::Original => self.orig.add(us),
            HoleOutcome::Repair => self.rep.add(us),
            HoleOutcome::Abandoned => self.aban.add(us),
        }
        if self.dump_on && self.dumped < self.dump_cap {
            self.dumped += 1;
            self.dump_pending.push((outcome.tag(), us));
        }
    }

    /// Has this gauge ever seen an arrival — i.e. does it sit at a RECEIVER?
    /// A sender-role site never calls it and must stay silent.
    pub fn is_receiver_site(&self) -> bool {
        self.hi.is_some()
    }

    /// Holes currently outstanding — a CENSUS, not an outcome.
    pub fn open_n(&self) -> u64 {
        self.open.len() as u64
    }

    /// Every hole ever detected. The accounting identity's left side.
    pub fn det_n(&self) -> u64 {
        self.det
    }

    /// Detections the declared bounds refused to track.
    pub fn over_n(&self) -> u64 {
        self.over
    }

    pub fn hist(&self, outcome: HoleOutcome) -> &Hist {
        match outcome {
            HoleOutcome::Original => &self.orig,
            HoleOutcome::Repair => &self.rep,
            HoleOutcome::Abandoned => &self.aban,
        }
    }

    /// **THE FALSE-REPAIR BOUNDARY IN TIME**, defined in this module's header
    /// BEFORE any pass ran: the smallest bucket lower edge `t` at which strictly
    /// more holes have been closed by a REPAIR within `t` than by their own
    /// ORIGINAL within `t`. `None` — rendered `-` — when no such `t` exists,
    /// which reads "the original leads at every horizon" and is a legal
    /// outcome rather than a missing value.
    pub fn crossing_us(&self) -> Option<u64> {
        let (mut co, mut cr) = (0u64, 0u64);
        for i in 0..BUCKETS {
            co += self.orig.buckets[i];
            cr += self.rep.buckets[i];
            if cr > co {
                return Some(bucket_lower_edge(i));
            }
        }
        None
    }

    /// Of the holes that RESOLVED, the fraction closed by the original.
    /// `None` — rendered `-` — when none resolved.
    pub fn orig_frac(&self) -> Option<f64> {
        let res = self.orig.n + self.rep.n;
        (res > 0).then(|| self.orig.n as f64 / res as f64)
    }

    /// The `[SUCC]` line this gauge would emit right now. Cumulative: the LAST
    /// line of a log is the reading — the `[RACK]` / `[RFA]` / `[FCAUSE]`
    /// convention.
    pub fn line(&self) -> String {
        succ_report_line(
            self.gen,
            self.det,
            &self.orig,
            &self.rep,
            &self.aban,
            self.open_n(),
            self.over,
            self.crossing_us(),
            self.dump_on,
            self.dumped,
        )
    }

    /// Drain whatever raw-dump lines are ready. `flush` also emits the partial
    /// tail batch, which is why this gauge has no "lost tail" caveat: the
    /// periodic readout flushes, so every recorded sample reaches the log.
    /// Returns `[SUCCDUMP-CAP]` exactly once, when the cap first binds.
    pub fn take_dump_lines(&mut self, flush: bool) -> Vec<String> {
        let mut out = Vec::new();
        if !self.dump_on {
            return out;
        }
        while self.dump_pending.len() >= DUMP_BATCH
            || (flush && !self.dump_pending.is_empty())
        {
            let take = self.dump_pending.len().min(DUMP_BATCH);
            let batch: Vec<(char, u64)> = self.dump_pending.drain(..take).collect();
            let mut s = format!("[SUCCDUMP] n={} d=", batch.len());
            for (i, (tag, us)) in batch.iter().enumerate() {
                if i > 0 {
                    s.push(';');
                }
                s.push(*tag);
                s.push(',');
                s.push_str(&us.to_string());
            }
            out.push(s);
        }
        if !self.dump_capped_announced && self.dumped >= self.dump_cap {
            self.dump_capped_announced = true;
            out.push(format!("[SUCCDUMP-CAP] dumped={}", self.dumped));
        }
        out
    }
}

// ── THE LINE ────────────────────────────────────────────────────────────

/// Render one outcome's five slots. `-` iff `n == 0`, so an absent reading is
/// never confusable with a measured zero — and `n` sits BESIDE every value, so
/// no quantile is ever read without its own sample count.
fn slots(name: &str, h: &Hist) -> String {
    let q = |p: f64| h.quantile(p).map_or_else(|| "-".to_string(), |v| v.to_string());
    format!(
        "{name}_n={} {name}_p50_us={} {name}_p90_us={} {name}_p99_us={} \
         {name}_mx_us={} {name}_mean_us={}",
        h.n(),
        q(0.50),
        q(0.90),
        q(0.99),
        if h.n() == 0 { "-".to_string() } else { h.max_us().to_string() },
        h.mean_us().map_or_else(|| "-".to_string(), |v| v.to_string()),
    )
}

/// The `[SUCC]` line — the per-outcome time-to-resolution distribution of the
/// same-flow successor-arrival measurand. See the module header.
#[allow(clippy::too_many_arguments)]
pub fn succ_report_line(
    gen: bool,
    det: u64,
    orig: &Hist,
    rep: &Hist,
    aban: &Hist,
    open: u64,
    over: u64,
    cross_us: Option<u64>,
    dump_on: bool,
    dumped: u64,
) -> String {
    let res = orig.n() + rep.n();
    let of = if res == 0 {
        "-".to_string()
    } else {
        format!("{:.4}", orig.n() as f64 / res as f64)
    };
    format!(
        "[SUCC] gen={} det={} res={} {} {} {} open={} over={} \
         orig_frac={} cross_us={} dump={}/{}",
        u8::from(gen),
        det,
        res,
        slots("orig", orig),
        slots("rep", rep),
        slots("aban", aban),
        open,
        over,
        of,
        cross_us.map_or_else(|| "-".to_string(), |v| v.to_string()),
        u8::from(dump_on),
        dumped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, us: u64) -> Instant {
        base + Duration::from_micros(us)
    }

    // ── THE BUCKET MAP ──────────────────────────────────────────────────

    #[test]
    fn buckets_are_monotone_contiguous_and_bounded_in_width() {
        // Exact below SUB.
        for v in 0..SUB {
            assert_eq!(bucket_of(v), v as usize, "sub-SUB buckets are exact");
            assert_eq!(bucket_lower_edge(v as usize), v);
        }
        // `bucket_lower_edge` is the inverse of `bucket_of` on edges, and
        // `bucket_of` is monotone. Both are what a quantile depends on.
        let mut prev = 0usize;
        let mut v = 1u64;
        while v < (1u64 << 40) {
            let b = bucket_of(v);
            assert!(b >= prev, "bucket_of must be monotone at {v}");
            prev = b;
            let lo = bucket_lower_edge(b);
            assert!(lo <= v, "bucket {b} lower edge {lo} exceeds its member {v}");
            assert_eq!(bucket_of(lo), b, "edge {lo} must map back to bucket {b}");
            // THE DECLARED ERROR: a reported quantile is the lower edge, so
            // the underestimate is bounded by the bucket's relative width.
            if v >= SUB {
                let hi_edge = bucket_lower_edge(b + 1);
                assert!(
                    (hi_edge - lo) as f64 / lo as f64 <= 0.1305,
                    "bucket {b} spans {lo}..{hi_edge}, wider than 2^(1/8) allows"
                );
            }
            v = v + 1 + v / 97;
        }
        // Every u64 lands inside the declared array.
        assert!(bucket_of(u64::MAX) < BUCKETS, "u64::MAX must be in range");
    }

    #[test]
    fn quantiles_are_lower_edges_and_absent_reads_as_none() {
        let mut h = Hist::default();
        assert_eq!(h.quantile(0.5), None, "no sample ⇒ no quantile, never 0");
        assert_eq!(h.mean_us(), None);
        for us in 1..=100u64 {
            h.add(us);
        }
        assert_eq!(h.n(), 100);
        assert_eq!(h.max_us(), 100);
        let p50 = h.quantile(0.5).expect("100 samples");
        // Rank 50 ⇒ the sample 50 ⇒ its bucket's lower edge, at most 9.05 %
        // below it and never above it.
        assert!(p50 <= 50, "a lower-edge quantile never overestimates: {p50}");
        assert!(p50 as f64 >= 50.0 / 1.1305, "and is bounded below: {p50}");
        assert_eq!(h.quantile(1.0), Some(bucket_lower_edge(bucket_of(100))));
        assert!(h.quantile(0.0).expect("nonempty") <= p50);
    }

    // ── THE EVENT SEMANTICS ─────────────────────────────────────────────

    #[test]
    fn the_first_arrival_exposes_nothing_and_the_second_exposes_the_gap() {
        let t = Instant::now();
        let mut g = SuccGauge::new(false, false, 0);
        assert!(!g.is_receiver_site(), "a gauge with no arrival is not a receiver");
        // The flow's first symbol is seq 7 — that is a BASELINE, not seven
        // holes. A gauge that opened holes below its first-ever arrival would
        // manufacture its own denominator.
        g.observe_high(7, t);
        assert!(g.is_receiver_site());
        assert_eq!(g.det_n(), 0, "the first arrival exposes no hole");
        assert_eq!(g.open_n(), 0);
        // seq 10 arrives: 8 and 9 have never been seen, so both are exposed.
        g.observe_high(10, t);
        assert_eq!(g.det_n(), 2);
        assert_eq!(g.open_n(), 2);
        // A seq at or below the mark exposes nothing.
        g.observe_high(9, t);
        g.observe_high(10, t);
        assert_eq!(g.det_n(), 2, "a non-advancing arrival exposes no hole");
    }

    #[test]
    fn the_three_outcomes_are_disjoint_and_the_first_terminal_event_wins() {
        let t = Instant::now();
        let mut g = SuccGauge::new(false, false, 0);
        g.observe_high(0, t);
        g.observe_high(4, t); // exposes 1, 2, 3
        assert_eq!(g.det_n(), 3);

        g.resolve(1, false, at(t, 500)); // original, 500 µs
        g.resolve(2, true, at(t, 1500)); // repair, 1500 µs
        g.abandon_below(4, at(t, 9000)); // 3 given up, 9000 µs

        assert_eq!(g.hist(HoleOutcome::Original).n(), 1);
        assert_eq!(g.hist(HoleOutcome::Repair).n(), 1);
        assert_eq!(g.hist(HoleOutcome::Abandoned).n(), 1);
        assert_eq!(g.hist(HoleOutcome::Original).max_us(), 500);
        assert_eq!(g.hist(HoleOutcome::Repair).max_us(), 1500);
        assert_eq!(g.hist(HoleOutcome::Abandoned).max_us(), 9000);

        // A LATE ARRIVAL OF AN ABANDONED SEQ IS NOT A SECOND OUTCOME. This is
        // the property that makes the three classes a partition rather than
        // three overlapping counts.
        g.resolve(3, false, at(t, 20_000));
        g.resolve(1, true, at(t, 20_000));
        assert_eq!(g.hist(HoleOutcome::Original).n(), 1, "no double-count");
        assert_eq!(g.hist(HoleOutcome::Repair).n(), 1);
        assert_eq!(g.hist(HoleOutcome::Abandoned).n(), 1);
    }

    #[test]
    fn the_accounting_identity_holds_including_at_the_declared_bounds() {
        let t = Instant::now();
        let identity = |g: &SuccGauge| {
            assert_eq!(
                g.det_n(),
                g.hist(HoleOutcome::Original).n()
                    + g.hist(HoleOutcome::Repair).n()
                    + g.hist(HoleOutcome::Abandoned).n()
                    + g.open_n()
                    + g.over_n(),
                "det = orig + rep + aban + open + over"
            );
        };

        let mut g = SuccGauge::new(false, false, 0);
        g.observe_high(0, t);
        for s in 1..200u64 {
            g.observe_high(s * 3, t); // exposes two holes per step
            identity(&g);
        }
        for s in 1..100u64 {
            g.resolve(s * 3 - 1, s % 2 == 0, at(t, 100 * s));
            identity(&g);
        }
        g.abandon_below(200, at(t, 999_999));
        identity(&g);

        // MAX_SPAN: a jump wider than the bound is counted WHOLE and tracked
        // not at all, so the identity survives the truncation that protects
        // the gauge's memory.
        let mut g2 = SuccGauge::new(false, false, 0);
        g2.observe_high(0, t);
        g2.observe_high(MAX_SPAN + 10, t);
        assert_eq!(g2.det_n(), MAX_SPAN + 9);
        assert_eq!(g2.over_n(), MAX_SPAN + 9, "a too-wide span is all `over`");
        assert_eq!(g2.open_n(), 0);
        identity(&g2);

        // MAX_OPEN: the map stops growing and the excess lands in `over`.
        let mut g3 = SuccGauge::new(false, false, 0);
        g3.observe_high(0, t);
        let mut next = 0u64;
        while g3.open_n() < MAX_OPEN as u64 {
            next += MAX_SPAN;
            g3.observe_high(next, t);
        }
        assert_eq!(g3.open_n(), MAX_OPEN as u64);
        let before = g3.over_n();
        g3.observe_high(next + 100, t);
        assert_eq!(g3.over_n(), before + 99, "past the cap, detections are `over`");
        identity(&g3);
    }

    // ── THE DERIVED READINGS ────────────────────────────────────────────

    #[test]
    fn the_crossing_point_is_where_repair_overtakes_original() {
        let t = Instant::now();
        let mut g = SuccGauge::new(false, false, 0);
        g.observe_high(0, t);
        for s in 1..=200u64 {
            g.observe_high(s * 2, t);
        }
        // Originals: fast (≈1 ms). Repairs: slow (≈50 ms), and MORE numerous,
        // so the repair CDF must overtake somewhere between the two clusters.
        for s in 1..=50u64 {
            g.resolve(s * 2 - 1, false, at(t, 1_000 + s));
        }
        for s in 51..=200u64 {
            g.resolve(s * 2 - 1, true, at(t, 50_000 + s));
        }
        let cross = g.crossing_us().expect("repairs outnumber originals");
        assert!(
            (1_000..=50_000).contains(&cross),
            "the crossing must sit between the two clusters, got {cross}"
        );
        assert!(
            (g.orig_frac().expect("resolved") - 0.25).abs() < 1e-9,
            "50 of 200 resolved holes closed by their original"
        );

        // NO CROSSING IS A LEGAL OUTCOME, not a missing value: when the
        // original leads at every horizon there is no `t` to report.
        let mut h = SuccGauge::new(false, false, 0);
        h.observe_high(0, t);
        for s in 1..=20u64 {
            h.observe_high(s * 2, t);
            h.resolve(s * 2 - 1, false, at(t, 1_000));
        }
        assert_eq!(h.crossing_us(), None, "originals only ⇒ no crossing");
        assert_eq!(h.orig_frac(), Some(1.0));

        // And an empty gauge reports neither.
        let e = SuccGauge::new(false, false, 0);
        assert_eq!(e.crossing_us(), None);
        assert_eq!(e.orig_frac(), None);
    }

    // ── THE LINE ────────────────────────────────────────────────────────

    #[test]
    fn the_succ_line_format_is_pinned() {
        let mut orig = Hist::default();
        orig.add(1_000);
        orig.add(2_000);
        let mut rep = Hist::default();
        rep.add(40_000);
        let aban = Hist::default();
        let l = succ_report_line(false, 7, &orig, &rep, &aban, 3, 1, Some(2048), false, 0);
        assert_eq!(
            l,
            // 1000 µs ⇒ bucket [960, 1024); 2000 ⇒ [1920, 2048); 40 000 ⇒
            // [36864, 40960). Every quantile is its bucket's LOWER edge, so
            // each reads at or below the sample it summarises and never above
            // it — and `mx` carries the EXACT maximum beside it, so the
            // bucketing's direction is checkable off the line itself.
            "[SUCC] gen=0 det=7 res=3 orig_n=2 orig_p50_us=960 orig_p90_us=1920 \
             orig_p99_us=1920 orig_mx_us=2000 orig_mean_us=1500 rep_n=1 \
             rep_p50_us=36864 rep_p90_us=36864 rep_p99_us=36864 rep_mx_us=40000 \
             rep_mean_us=40000 aban_n=0 aban_p50_us=- aban_p90_us=- aban_p99_us=- \
             aban_mx_us=- aban_mean_us=- open=3 over=1 orig_frac=0.6667 \
             cross_us=2048 dump=0/0"
        );
        // `-` IFF NONE, on every slot of an empty outcome, and never a 0 that
        // a parser would read as a measured zero.
        assert!(l.contains("aban_n=0 aban_p50_us=-"));
        // THE GENERATION ROW: the line says which machine it measured.
        let e = Hist::default();
        let g = succ_report_line(true, 0, &e, &e, &e, 0, 0, None, true, 12);
        assert!(g.starts_with("[SUCC] gen=1 det=0 res=0 "), "{g}");
        assert!(g.contains("orig_frac=- cross_us=- dump=1/12"), "{g}");
    }

    #[test]
    fn the_raw_dump_is_off_by_default_batched_and_announces_its_own_cap() {
        let t = Instant::now();
        // OFF: not one line, whatever happens.
        let mut off = SuccGauge::new(false, false, 0);
        off.observe_high(0, t);
        off.observe_high(100, t);
        for s in 1..100u64 {
            off.resolve(s, false, at(t, s));
        }
        assert!(off.take_dump_lines(true).is_empty(), "the dump ships OFF");

        // ON: full batches only until flushed, then the tail.
        let mut on = SuccGauge::new(false, true, 1_000);
        on.observe_high(0, t);
        on.observe_high(1_000, t);
        for s in 1..=(DUMP_BATCH as u64 + 5) {
            on.resolve(s, s % 3 == 0, at(t, s * 10));
        }
        let lines = on.take_dump_lines(false);
        assert_eq!(lines.len(), 1, "one FULL batch, tail withheld: {lines:?}");
        assert!(lines[0].starts_with(&format!("[SUCCDUMP] n={DUMP_BATCH} d=")));
        assert_eq!(lines[0].matches(';').count(), DUMP_BATCH - 1);
        assert!(lines[0].contains("o,10;"), "records are <tag>,<us>");
        let tail = on.take_dump_lines(true);
        assert_eq!(tail.len(), 1, "flush emits the partial tail: {tail:?}");
        assert!(tail[0].starts_with("[SUCCDUMP] n=5 d="));
        assert!(on.take_dump_lines(true).is_empty(), "nothing left after a flush");

        // THE CAP BINDS ONCE, LOUDLY, and stops recording raw records — while
        // the histograms keep counting, so a capped dump never truncates the
        // quantile line it rides beside.
        let mut cap = SuccGauge::new(false, true, 4);
        cap.observe_high(0, t);
        cap.observe_high(100, t);
        for s in 1..=20u64 {
            cap.resolve(s, false, at(t, s));
        }
        let out = cap.take_dump_lines(true);
        assert!(
            out.iter().any(|l| l == "[SUCCDUMP-CAP] dumped=4"),
            "the cap announces itself exactly once: {out:?}"
        );
        let again = cap.take_dump_lines(true);
        assert!(!again.iter().any(|l| l.contains("SUCCDUMP-CAP")), "once, not twice");
        assert_eq!(
            cap.hist(HoleOutcome::Original).n(),
            20,
            "the CAP is on the dump, not on the measurement"
        );
    }

    // ── THE DISCIPLINE ──────────────────────────────────────────────────

    /// MEASUREMENT DISCIPLINE: this gauge is READ-ONLY, and that is a property
    /// of the SOURCE, not of a comment. Nothing in the engine may branch on
    /// anything it computes, so it holds no engine handle and no site outside
    /// this module and the receiver's feed/readout may name its readers.
    #[test]
    fn succ_is_observation_only() {
        let src = include_str!("succ.rs");
        // Spelled in halves so this test's OWN source is not the match it is
        // looking for — the scraper reads the whole file, itself included.
        for forbidden in [
            concat!("Sched", "uler"),
            concat!("FecRate", "Controller"),
            concat!("Quic", "Transport"),
            concat!("Control", "Message"),
        ] {
            assert!(
                !src.contains(forbidden),
                "`{forbidden}` in an observation-only gauge — it has acquired \
                 an engine handle and can no longer be read as read-only"
            );
        }
        // The engine-wide half of the same claim: the receiver may FEED and
        // PRINT this gauge and may not TEST it. `crossing_us`, `orig_frac` and
        // `quantile` are the three readers a law could plausibly be built on,
        // so they are the three whose call sites are pinned to `line()`.
        let recv = include_str!("receiver.rs");
        for reader in ["crossing_us", "orig_frac", ".quantile("] {
            assert!(
                !recv.contains(reader),
                "receiver.rs calls `{reader}` — the successor gauge's readings \
                 have reached a decision site. NO LAW MAY READ THIS GAUGE; the \
                 derivation it feeds is FORMULA-FIRST, in the paper, and not a \
                 wire from an instrument to a branch."
            );
        }
    }
}
