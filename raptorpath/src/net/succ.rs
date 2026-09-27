//! `[SUCC]` — the same-flow successor-arrival distribution, measured at the
//! receiver.
//!
//! Almost all recovery fires are `gap_data` — the receiver's SACK report,
//! emitted when a higher seq arrives while a hole is outstanding (paper
//! §7.4). This gauge characterizes that measurand: per hole, how long the
//! hole lives and what closes it. It derives no law, positions no waiting
//! time, and hands nothing to any consumer.
//!
//! ## The origin event: hole detection, not hole creation
//!
//!   * Creation — the instant the sender put the symbol on the wire, or the
//!     instant the network dropped it — is not receiver-observable: the
//!     receiver never sees the lost symbol, so it holds no send-timestamp for
//!     it. Measuring from creation would need the sender's clock and a wire
//!     change, and would measure a quantity the fire site cannot condition on.
//!   * Detection — the first arrival of a strictly higher seq while this seq
//!     is unresolved — is what this gauge uses.
//!
//! Detection is the same event the majority cause fires on. `gap_data`'s
//! producer in `receiver.rs` is
//!
//! ```text
//!     gap_report_due = highest_seen_seq > highest_delivered_seq
//!                   && highest_seen_seq > last_gap_ack_seen
//!                   && last_gap_ack_time.elapsed() >= GAP_ACK_MIN_INTERVAL
//! ```
//!
//! — a higher seq arrived while a hole was outstanding, i.e. detection. A
//! waiting time on a clock that starts at detection is positioned on the same
//! origin as the fires it governs. The gauge's own high-water mark is fed by
//! exactly the arrivals that feed `highest_seen_seq`, so the two advance
//! together by construction.
//!
//! Every duration reported here excludes the interval from creation to
//! detection: a lower bound on hole age, and an exact measure of the
//! conditional the fire site sees.
//!
//! ## The three outcomes — disjoint, first terminal event wins
//!
//!   * `orig` — the seq's own source symbol arrived. A late reorder, or
//!     the sender's copy (retransmit / taper copy). The wire carries no
//!     "this is a retransmit" bit, but every batch carries its sender stamp:
//!     originals are stamped in seq order, so a closing copy stamped later
//!     than the arrival that exposed the hole cannot be the original. Those
//!     holes are the `HoleOutcome::Retransmit` subset, printed as `rtx_n=`
//!     (still inside `orig_n`, whose meaning is own-source arrival) and
//!     excluded from `[LATE]`'s self-heal estimate π̂0. The
//!     split is a lower bound on copies: a copy stamped before its hole's
//!     exposer was stamped reads `orig`.
//!   * `rep` — the seq came out of the decoder, reconstructed from coded
//!     repair rather than from its own source arrival. The same test the
//!     `[RFA]` site already uses for `fill_coded`: `symbol.is_repair ||
//!     seq != symbol.block_id`.
//!   * `aban` — the in-order delivery frontier moved past the hole while it
//!     was still open: force-delivery / give-up. Under the reliable window
//!     (ρ = 1) `ReorderBuffer::new_reliable` never expires a hole, so this
//!     class is structurally empty there and a nonzero reading is a finding
//!     about the engine.
//!
//! A hole still outstanding when the line is emitted is in none of the three:
//! it is counted in `open`, a census and not an outcome. The gauge is read
//! cumulatively (last line wins) at a site the harness SIGKILLs, so "still
//! open at the last line" is reported as its own slot rather than folded into
//! `aban`.
//!
//! The accounting identity, asserted by test:
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
//! ## What is reported: quantiles, not a mean
//!
//! A waiting time is a quantile decision — "wait until the successor has
//! probably arrived" — so the mean is not reported as a headline. Per
//! outcome: `n`, `p50`, `p90`, `p99`, `mx`, all in µs. Plus two derived
//! readings:
//!
//!   * `orig_frac` = `orig_n / (orig_n + rep_n)` — of the holes that
//!     resolved, the fraction the original closed. A repair emitted for a
//!     hole whose original was coming was unnecessary.
//!   * `cross` — the false-repair boundary in time: the smallest
//!     histogram-bucket lower edge `t` at which the count of holes closed by
//!     a repair within `t` strictly exceeds the count closed by their
//!     original within `t`. Below `cross`, waiting pays — most holes
//!     that close, close by themselves. Above it, waiting does not. It renders
//!     `-` when no such `t` exists, which reads "the original is ahead at
//!     every horizon" and is a legal outcome, not a missing value.
//!
//! ## The histogram and its declared error
//!
//! Counts land in log-spaced buckets, 8 sub-buckets per octave, exact below
//! 8 µs. A bucket's relative width above that is `2^(1/8) − 1 ≈ 9.05 %`, and a
//! reported quantile is the lower edge of the bucket the rank falls in — so
//! every quantile here is an underestimate by at most 9.05 % (pinned by
//! test). Memory is [`BUCKETS`] × 8 B × 3 outcomes ≈ 12 kB
//! for the whole run, independent of hole count: this gauge cannot grow with
//! the transfer.
//!
//! ## The raw dump (default off)
//!
//! `RWM_SUCC_DUMP=1` additionally emits `[SUCCDUMP]` batches of raw
//! `(outcome, µs)` records, so any functional can be computed offline over
//! the exact samples rather than over this gauge's buckets. It is capped
//! ([`dump_max`], `RWM_SUCC_DUMP_MAX`) and announces its own truncation with
//! one `[SUCCDUMP-CAP]` line. The quantile line is always emitted; only the
//! dump is gated.
//!
//! ## One window per invocation
//!
//! The gauge's high-water mark and its open-hole map are per receiver task,
//! and the receiver task outlives an individual perf run. A multi-run
//! invocation (`--runs N`, N > 1) restarts the window's seq space while the
//! gauge's mark stays at the previous run's maximum, so the second run's
//! arrivals expose no hole, and the previous run's trailing holes can look
//! abandoned when the delivery frontier resets (`aban_n = 1` under a reliable
//! window). `[SUCC]` is therefore a one-run-per-invocation instrument, as the
//! L1 batteries invoke the engine (`perf_rwm_c.sh … 1`).
//!
//! ## Read-only
//!
//! Every counter is fed from events the receiver already produces. The gauge
//! holds no engine handle, returns nothing any engine site reads, and no
//! branch anywhere in the tree tests any value it computes. Pinned by
//! [`tests::succ_is_observation_only`].

use std::collections::BTreeMap;
use std::time::Instant;

// ── Declared resource bounds ────────────────────────────────────────────

/// Maximum simultaneously-tracked open holes. Beyond this a detection is
/// counted in `det` and `over` and never tracked, so the accounting identity
/// holds and the truncation is read off the line rather than inferred.
///
/// The in-flight span is bounded by the sender's outstanding cap, so this is
/// ~2 orders above any reachable simultaneous-hole count. It exists so that a
/// pathological run cannot make an observation-only gauge the reason for an
/// OOM.
pub const MAX_OPEN: usize = 65_536;

/// Maximum seq span one arrival may expose as holes in a single step. A jump
/// wider than this is counted whole into `det` and `over` without enumeration,
/// which bounds the per-arrival cost of the gauge at O(MAX_SPAN) and its
/// typical cost at O(1) — the engine's seqs are dense, so the realized span is
/// 1 on every ordinary arrival.
pub const MAX_SPAN: u64 = 4_096;

/// Sub-buckets per octave, as a power of two. 8 ⇒ ≤ 9.05 % relative bucket
/// width, and the reported quantile is the bucket's lower edge.
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
    crate::gates::get().succ_dump_max
}

/// The resolve-time read behind [`dump_max`].
pub(crate) fn resolve_dump_max() -> u64 {
    std::env::var("RWM_SUCC_DUMP_MAX")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DUMP_MAX_DEFAULT)
}

// ── The bucket map ──────────────────────────────────────────────────────

/// Bucket index for `v` µs. Monotone non-decreasing in `v`, and exact (index
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

/// Classify a hole closed by an arrival of the seq's own source symbol.
///
/// Originals are stamped in seq order by one sender clock, so the original
/// of a hole was stamped no later than any original of a higher seq —
/// including the arrival that exposed it. A closing copy stamped strictly
/// later than the exposer is therefore the sender's copy, not the original.
/// Both stamps are the sender's clock (a same-clock comparison). A copy sent
/// before the exposer was sent is not caught (it reads `Original`), so
/// `Retransmit` is a lower bound. A stamp of 0 means "unknown".
pub fn classify_source_close(closer_ts_us: u64, exposer_ts_us: u64) -> HoleOutcome {
    if exposer_ts_us > 0 && closer_ts_us > exposer_ts_us {
        HoleOutcome::Retransmit
    } else {
        HoleOutcome::Original
    }
}

// ── One outcome's distribution ──────────────────────────────────────────

/// Which terminal event closed a hole. A label: nothing in the engine
/// branches on it, only counters read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoleOutcome {
    /// The seq's own source symbol arrived and it was the original send — a
    /// late reorder, i.e. a genuine self-heal.
    Original,
    /// The seq's own source symbol arrived as the sender's copy
    /// (retransmit / taper copy): its sender stamp is later than the stamp of
    /// the arrival that exposed the hole, which no original can be (see
    /// [`classify_source_close`]). Printed inside `orig_*` (an own-source
    /// arrival) and separately as `rtx_n=`; not a self-heal for `[LATE]`'s
    /// pi0.
    Retransmit,
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
            HoleOutcome::Retransmit => 'x',
            HoleOutcome::Repair => 'r',
            HoleOutcome::Abandoned => 'a',
        }
    }
}

/// One hole's terminal record, handed back by
/// [`SuccGauge::resolve`] and [`SuccGauge::abandon_below`].
///
/// `us` is the lower end of the lateness bracket (from the arrival that
/// exposed the hole) and `hi_us` the upper end (from the previous advance of
/// the high-water mark, the earliest instant the seq could have been due).
/// `us <= hi_us` always. Both are printed by `[LATE]`; `[LAT]` uses `us` and
/// the class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoleRecord {
    /// The sequence number this record is about. `[RFA] late_after_aban`
    /// needs it: a copy that arrives after the frontier gave up is only
    /// identifiable against the set of seqs the frontier actually skipped.
    pub seq: u64,
    /// Which terminal event closed the hole.
    pub outcome: HoleOutcome,
    /// The closing arrival landed on a different path from the exposer.
    pub cross: bool,
    /// Time from exposure to close, µs — the bracket's lower end, and the
    /// number `[SUCC]`'s own histograms carry.
    pub us: u64,
    /// Time from the previous high-water advance to close, µs — the bracket's
    /// upper end.
    pub hi_us: u64,
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
    /// The exact accumulated total, us. `[LAT]`'s shares are sums and not
    /// quantiles: a share must add up across classes, and bucket lower edges
    /// do not.
    pub fn sum_us(&self) -> u64 {
        self.sum_us
    }

    /// The `p`-quantile as the lower edge of the bucket the rank falls in —
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

// ── The gauge ───────────────────────────────────────────────────────────

/// The receiver-site successor-arrival gauge. Owned by the receiver task; no
/// engine handle, no shared state, no `&mut` reachable from any decision site.
pub struct SuccGauge {
    /// Open holes: seq → (the instant it was detected, the path whose
    /// arrival exposed it, ...). `BTreeMap` because the abandonment sweep is a
    /// frontier range, which a hash map cannot do without scanning the whole
    /// map on every frontier advance.
    ///
    /// The exposer path is recorded at detection and nowhere else. A hole is
    /// exposed by the arrival of a strictly higher seq on exactly one path,
    /// and the seq that eventually closes it lands on exactly one path too.
    /// Same ⇒ a same-path ordering event (in-path reordering, or a real loss
    /// on that path). Different ⇒ a cross-path skew event, structurally
    /// impossible at a single-path cell. Observation only.
    ///
    /// The fourth element is the exposing arrival's sender stamp
    /// (`send_timestamp_us`, 0 = unknown), for [`classify_source_close`].
    open: BTreeMap<u64, (Instant, u32, Instant, u64)>,
    /// The gauge's own high-water seq mark. `None` until the first arrival —
    /// the flow's first symbol exposes no hole, it establishes the baseline.
    hi: Option<u64>,
    /// The instant `hi` last advanced (`[LATE]`, `net/late.rs`).
    ///
    /// A hole's true lateness is not observable: the receiver learns a seq is
    /// missing only when a higher one arrives, and the seq was actually due
    /// some time between the previous advance of this mark and that arrival.
    /// So every hole carries a bracket — the third element of the `open`
    /// tuple is this instant, captured at the moment the hole was exposed —
    /// and `[LATE]` prints both ends. A law positioned on a bracketed
    /// quantity must say which end it used.
    ///
    /// Observation only; `[SUCC]` itself still times from the exposure.
    hi_at: Instant,
    /// Per-outcome distributions, indexed by `HoleOutcome`.
    orig: Hist,
    rep: Hist,
    aban: Hist,
    /// The same/cross exposure split: resolution times of holes whose
    /// closing arrival came on the same path that exposed them (`sp`) and on
    /// a different one (`xp`). Disjoint, and their `n`s sum to `res` by
    /// construction: every resolution carries exactly one arrival path.
    sp: Hist,
    xp: Hist,
    /// Of the `orig` class, the holes closed by the sender's copy
    /// ([`HoleOutcome::Retransmit`]). Printed as `rtx_n=`.
    rtx_n: u64,
    /// Every hole ever detected, tracked or not — the identity's left side.
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
            hi_at: Instant::now(),
            orig: Hist::default(),
            rep: Hist::default(),
            aban: Hist::default(),
            sp: Hist::default(),
            xp: Hist::default(),
            rtx_n: 0,
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

    /// Resolution. One seq has just been resolved — by its own source
    /// arrival (`by_repair = false`) or by the decoder (`by_repair = true`).
    /// A no-op unless the seq is an open, tracked hole, which is what makes
    /// the three outcomes disjoint: the first terminal event wins and every
    /// later observation of the same seq falls through.
    ///
    /// Call this for every seq of an arrival before [`Self::observe_high`]
    /// for any of them. One `add_symbol` can emit several seqs in arbitrary
    /// order; resolving the whole batch first is what stops a batch that
    /// decodes `[10, 8]` from opening a hole for 8 and closing it at 0 µs.
    ///
    /// `path_id` is the path the closing arrival landed on — compared against
    /// the path that exposed the hole. Classification only: the comparison
    /// feeds two histograms and no decision.
    ///
    /// Returns the resolution record ([`HoleRecord`]), or `None` when the seq
    /// was not an open, tracked hole (the ordinary in-order case).
    ///
    /// `[LAT]` (`net/lat.rs`) is the consumer: the class of a reorder wait is
    /// the class of the hole whose resolution released it, and this gauge is
    /// the only place that record exists. Handing it back beats recomputing
    /// it, which would be a second classification able to disagree with this
    /// one.
    pub fn resolve(
        &mut self,
        seq: u64,
        by_repair: bool,
        now: Instant,
        path_id: u32,
    ) -> Option<HoleRecord> {
        self.resolve_at(seq, by_repair, now, path_id, 0)
    }

    /// [`Self::resolve`] with the closing arrival's sender stamp
    /// (`send_timestamp_us`), which separates the sender's copy from the
    /// original ([`classify_source_close`]). The receiver calls this one.
    pub fn resolve_at(
        &mut self,
        seq: u64,
        by_repair: bool,
        now: Instant,
        path_id: u32,
        send_ts_us: u64,
    ) -> Option<HoleRecord> {
        let (t0, exposer, hi_at, exposer_ts) = self.open.remove(&seq)?;
        let us = now.saturating_duration_since(t0).as_micros() as u64;
        let outcome = if by_repair {
            HoleOutcome::Repair
        } else {
            classify_source_close(send_ts_us, exposer_ts)
        };
        let cross = path_id != exposer;
        if cross {
            self.xp.add(us);
        } else {
            self.sp.add(us);
        }
        self.record(outcome, us);
        Some(HoleRecord {
            seq,
            outcome,
            cross,
            us,
            hi_us: now.saturating_duration_since(hi_at).as_micros() as u64,
        })
    }

    /// Detection. One seq has arrived. Every seq strictly between the
    /// current high-water mark and `seq` has, by definition of a high-water
    /// mark, never been seen — so each is a hole this arrival has just
    /// exposed, and each is stamped now.
    pub fn observe_high(&mut self, seq: u64, now: Instant, path_id: u32) {
        self.observe_high_at(seq, now, path_id, 0)
    }

    /// [`Self::observe_high`] with the arrival's sender stamp, recorded on
    /// every hole it exposes. The receiver calls this one.
    pub fn observe_high_at(&mut self, seq: u64, now: Instant, path_id: u32, send_ts_us: u64) {
        let Some(hi) = self.hi else {
            // The flow's first arrival establishes the baseline and exposes
            // nothing: there is no "outstanding hole" below a mark that does
            // not exist yet.
            self.hi = Some(seq);
            self.hi_at = now;
            return;
        };
        if seq <= hi {
            return;
        }
        let span = seq - hi - 1;
        // `[LATE]`: the previous advance is the earliest instant the holes
        // this arrival exposes could have been due, so it is stamped on each
        // of them before the mark moves.
        let hi_prev = self.hi_at;
        self.hi = Some(seq);
        self.hi_at = now;
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
            self.open.insert(s, (now, path_id, hi_prev, send_ts_us));
        }
    }

    /// Abandonment. The in-order delivery frontier has advanced to
    /// `frontier` (the next seq that will be delivered). Every open hole
    /// strictly below it was passed over undelivered — given up.
    ///
    /// Returns the abandoned holes' records so `[LATE]` sees the third
    /// outcome class too. Empty — and allocating nothing — in the common case
    /// where the frontier passed no open hole, which is every advance under
    /// the reliable window.
    pub fn abandon_below(&mut self, frontier: u64, now: Instant) -> Vec<HoleRecord> {
        if self.open.is_empty() {
            return Vec::new();
        }
        // `split_off` leaves the below-frontier prefix behind and returns the
        // rest, so the sweep costs O(k log n) in the number abandoned rather
        // than O(n) in the number open.
        let keep = self.open.split_off(&frontier);
        let gone = std::mem::replace(&mut self.open, keep);
        let mut out = Vec::with_capacity(gone.len());
        for (seq, (t0, exposer, hi_at, _)) in gone {
            let us = now.saturating_duration_since(t0).as_micros() as u64;
            self.record(HoleOutcome::Abandoned, us);
            out.push(HoleRecord {
                seq,
                outcome: HoleOutcome::Abandoned,
                // An abandoned hole was closed by no arrival at all, so it has
                // no closing path: it is reported same-path, the conservative
                // class, which can only make the cross-path share read low.
                cross: false,
                us,
                hi_us: now.saturating_duration_since(hi_at).as_micros() as u64,
            });
            let _ = exposer;
        }
        out
    }

    fn record(&mut self, outcome: HoleOutcome, us: u64) {
        match outcome {
            HoleOutcome::Original => self.orig.add(us),
            HoleOutcome::Retransmit => {
                self.orig.add(us);
                self.rtx_n += 1;
            }
            HoleOutcome::Repair => self.rep.add(us),
            HoleOutcome::Abandoned => self.aban.add(us),
        }
        if self.dump_on && self.dumped < self.dump_cap {
            self.dumped += 1;
            self.dump_pending.push((outcome.tag(), us));
        }
    }

    /// Has this gauge ever seen an arrival — i.e. does it sit at a receiver?
    /// A sender-role site never calls it and must stay silent.
    pub fn is_receiver_site(&self) -> bool {
        self.hi.is_some()
    }

    /// The open holes, ascending, that are at least `min_age_us` late — the
    /// request law's `REQUEST <=> l >= l*` predicate (paper §7.6), evaluated
    /// on the one set that already exists. At most `max` of them (the
    /// caller's declared wire bound), and the age is the lower end of the
    /// lateness bracket — the
    /// conservative one, and the same clock `[SUCC]` times and `[LATE]` fits
    /// its density on.
    ///
    /// Read-only: this gauge decides nothing. The caller that reads it is the
    /// arm, and with the arm absent nothing calls it.
    pub fn holes_at_least(&self, now: Instant, min_age_us: u64, max: usize) -> Vec<u64> {
        let mut out = Vec::new();
        for (&seq, &(t0, _, _, _)) in self.open.iter() {
            if out.len() >= max {
                break;
            }
            if now.saturating_duration_since(t0).as_micros() as u64 >= min_age_us {
                out.push(seq);
            }
        }
        out
    }

    /// The earliest `A_hat`: the detection instant of the oldest open
    /// hole. `None` when nothing is outstanding. This is the term the request
    /// law's deadline is built on: a hole with no further arrivals still has
    /// to become requestable, and `earliest A_hat + l*` is when it does.
    pub fn oldest_open_at(&self) -> Option<Instant> {
        self.open.values().map(|&(t0, _, _, _)| t0).min()
    }

    /// Holes currently outstanding — a census, not an outcome.
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

    /// Holes closed by an arrival on the same path that exposed them.
    pub fn sp_n(&self) -> u64 {
        self.sp.n()
    }

    /// Holes closed by an arrival on a different path from the one that
    /// exposed them — the wire-reorder (scheduler-skew) class. At a
    /// single-path cell this is structurally zero.
    pub fn xp_n(&self) -> u64 {
        self.xp.n()
    }

    /// `xp_n / (sp_n + xp_n)`. `None` — rendered `-` — when nothing resolved.
    pub fn xp_frac(&self) -> Option<f64> {
        let d = self.sp.n() + self.xp.n();
        (d > 0).then(|| self.xp.n() as f64 / d as f64)
    }

    /// `Original` and `Retransmit` share the `orig` histogram (own-source
    /// arrivals); `rtx_n` is the retransmit share of it.
    pub fn hist(&self, outcome: HoleOutcome) -> &Hist {
        match outcome {
            HoleOutcome::Original | HoleOutcome::Retransmit => &self.orig,
            HoleOutcome::Repair => &self.rep,
            HoleOutcome::Abandoned => &self.aban,
        }
    }

    /// The false-repair boundary in time (see the module header): the
    /// smallest bucket lower edge `t` at which strictly more holes have been
    /// closed by a repair within `t` than by their own original within `t`. `None` — rendered `-` — when no such `t` exists,
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

    /// Of the holes that resolved, the fraction closed by the original.
    /// `None` — rendered `-` — when none resolved.
    pub fn orig_frac(&self) -> Option<f64> {
        let res = self.orig.n + self.rep.n;
        (res > 0).then(|| self.orig.n as f64 / res as f64)
    }

    /// Holes closed by the sender's copy ([`HoleOutcome::Retransmit`]).
    pub fn rtx_n(&self) -> u64 {
        self.rtx_n
    }

    /// The `[SUCC]` line this gauge would emit right now. Cumulative: the last
    /// line of a log is the reading — the `[RACK]` / `[RFA]` / `[FCAUSE]`
    /// convention.
    pub fn line(&self) -> String {
        // `rtx_n=` appended (the additive-column rule): the subset of
        // `orig_n` closed by the sender's copy rather than the original.
        let mut l = succ_report_line(
            self.gen,
            self.det,
            &self.orig,
            &self.rep,
            &self.aban,
            &self.sp,
            &self.xp,
            self.open_n(),
            self.over,
            self.crossing_us(),
            self.dump_on,
            self.dumped,
        );
        l.push_str(&format!(" rtx_n={}", self.rtx_n));
        l
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

// ── The line ────────────────────────────────────────────────────────────

/// Render one outcome's five slots. `-` iff `n == 0`, so an absent reading is
/// never confusable with a measured zero — and `n` sits beside every value, so
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
    sp: &Hist,
    xp: &Hist,
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
    // The same/cross exposure split, appended so every prior reader of
    // this line keeps its offsets — the additive-column rule.
    let xpd = sp.n() + xp.n();
    let xf = if xpd == 0 {
        "-".to_string()
    } else {
        format!("{:.4}", xp.n() as f64 / xpd as f64)
    };
    let qq =
        |h: &Hist, p: f64| h.quantile(p).map_or_else(|| "-".to_string(), |v| v.to_string());
    format!(
        "[SUCC] gen={} det={} res={} {} {} {} open={} over={} \
         orig_frac={} cross_us={} dump={}/{} \
         sp_n={} xp_n={} xp_frac={} sp_p50_us={} sp_p90_us={} \
         xp_p50_us={} xp_p90_us={}",
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
        sp.n(),
        xp.n(),
        xf,
        qq(sp, 0.50),
        qq(sp, 0.90),
        qq(xp, 0.50),
        qq(xp, 0.90),
    )
}

#[cfg(test)]
mod tests;
