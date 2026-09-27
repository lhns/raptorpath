//! The recovery-plane report lines and teardown gauges: `[RACK]`, `[RFA]`,
//! `[FCAUSE]`, `[REQS]`, the RACK clock gauge and the sender teardown set.
//! Moved verbatim out of `net/mod.rs` (cleanup Stage 3).

use super::*;

/// The `[RACK]` false-alarm echo — paper §16.68.1's validation of the
/// recovery plane against RFC 8985 §6.2 Step 4's own published spurious
/// budget, run on the SHIPPED clamp.
///
/// `fa=<spurious>/<fired>` — recovery rounds that fired, and those whose
/// target's live flight was YOUNGER than its own per-path law threshold (the
/// data was going to arrive anyway); `fa_frac` their ratio; `fa_class` the
/// RFC's own 1/16 bar, printed so a parser never has to know it. (The line
/// also carried the removed `RWM_RACK_CLOCKS` arm's bind fractions; those
/// fields left with the arm.)
pub fn rack_report_line(fired: u64, spurious: u64) -> String {
    let frac = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    format!(
        "[RACK] fa={}/{} fa_frac={:.4} fa_class={:.4}",
        spurious,
        fired,
        frac(spurious, fired),
        RACK_SPURIOUS_BUDGET,
    )
}

/// RFC 8985 §6.2 Step 4's own published spurious budget — *"approximately once
/// every 16 recoveries (less than 7%)"*. The CLASS BAR every arm of the
/// recovery-clock family is scored against (§16.68.1), printed beside the
/// measurement so a parser never has to know it.
pub const RACK_SPURIOUS_BUDGET: f64 = 1.0 / 16.0;

// ── `[RFA]`: THE REALIZED FALSE-REPAIR GAUGE, AT THE RECEIVER ────────────
//
// **WHAT WAS AND WAS NOT MISSING — the premise, corrected before this was
// written.** `RackClockGauge::record_fire` has exactly ONE call site, the
// sender's gap-driven retransmit loop fed by `recv_nack_tx`. It is NOT dead.
// `recv_nack_tx` is `None` under GENERATION CODING and only there (the
// suppression a few hundred lines above: generation recovers a short
// generation with more coded symbols, never by resending a seq), so the
// primitives pass's `fired = 0` at 15/15 was structural to the configuration
// it ran — generation ON — and not to the instrument. MEASURED here on a
// `c3`-lossy loopback: plain window gives `[RACK] fa=278/1332` with
// `[DIAG] retx=1327`; the same run with `--window-generation-coding` gives
// `retx=0`. The α-sweep runs plain window, so the SENDER's `fa=` already
// works there and this gauge does not replace it.
//
// **WHAT IS GENUINELY MISSING, AND IS WHAT THIS ADDS.** The sender's `fa=` is
// a PREDICTION: at fire time it asks whether the target's live flight is
// younger than its own per-path law threshold, i.e. whether the data *was
// going to* arrive anyway. That is the COMMANDED false-alarm fraction. The
// α-sweep (goal #100 item 2) scores REALIZED against commanded, and
// "realized" — the repair was emitted AND the original arrived anyway — is
// only observable where both copies land, at the RECEIVER, which built this
// same gauge and never called `record_fire` at all. The two numbers are not
// close: the run above reads a commanded `fa_frac = 0.2087` against a
// realized `false_frac = 0.7660`, 3.7×. That gap is the measurand, and
// nothing in the tree reported it before.
//
// **THE EVENT CLASS, STATED BECAUSE THE FIRE SITE IS GENUINELY AMBIGUOUS.**
// A FALSE REPAIR is *a repair emitted whose original arrived anyway.* The
// wire carries `is_repair` but no "this is a retransmit" bit, so the receiver
// cannot label an arriving source symbol a retransmit directly — it can only
// observe REDUNDANCY, which is the same fact and is ground truth rather than
// a prediction. Four disjoint classes are separated, each with its own
// counter, rather than one ambiguous total:
//
//   * `fill_coded`   — a seq the DECODER reconstructed from coded repair.
//                      The repair WORKED (the source had not arrived).
//                      TRUE repair. This is `[FDIAG]`'s DECODE class.
//   * `fill_src`     — a source arrival that first-resolved a seq the
//                      receiver was already OVERDUE on (a higher seq had
//                      already arrived). TRUE repair — or plain reordering;
//                      see the contamination note below. `[FDIAG]`'s SOURCE
//                      class, measured EMPTY on this engine at five cells.
//   * `dup_src`      — a SECOND source copy of a seq whose source copy had
//                      already arrived. The engine transmits each source
//                      symbol once; a second copy exists only because a
//                      repair mechanism resent it. **FALSE**: the repair was
//                      emitted and the original arrived anyway. ARQ class.
//   * `preempt_src`  — a source arrival for a seq the decoder had ALREADY
//                      reconstructed from coded repair. **FALSE**: the coded
//                      repair was unnecessary; the original arrived anyway.
//                      FEC class.
//
//   fired  = fill_coded + fill_src + dup_src + preempt_src
//   false  = dup_src + preempt_src
//
// so `[RACK]`'s `fa_frac` at a receiver-role gauge reads the REALIZED false-
// repair fraction, on the same denominator shape as the sender's predicted
// one, and is scored against the same [`RACK_SPURIOUS_BUDGET`] class bar.
//
// **THE DENOMINATOR, AND THE ONE CLASS THAT IS NOT RECEIVER-OBSERVABLE.**
// `src_n` (every source arrival) is carried beside the four classes so a
// reader can form `ν_recv = fired / src_n` — fires per delivered source
// symbol, the receiver-site analogue of the ledger's `fired / dgq_hand`. What
// is NOT here: REDUNDANT CODED RANK (a repair symbol that arrives and
// contributes no new degree of freedom). Coded repair is fungible, so a
// repair that recovers nothing may still have carried rank the decoder banked
// for later; separating "carried no rank" from "carried rank nobody needed"
// requires decoder-internal accounting this gauge deliberately does not do.
// That class is the FEC OVERHEAD commanded by `r`, not a false alarm of the
// recovery clock, and it is left to `[PFRAC]`/`repairs_useful`.
//
// **CONTAMINATION, DISCLOSED RATHER THAN CORRECTED.** `fill_src` counts a
// reordered ORIGINAL that arrives late as a successful repair, because the
// receiver cannot tell it from a retransmit. This inflates `fired` and so
// DEFLATES `fa_frac` — the realized false fraction reported here is a LOWER
// BOUND. On this engine the bias is bounded by measurement: `[FDIAG] SOURCE
// n = 0` at all five cells of the primitives pass, i.e. `fill_src` is empty
// and the bias is nil there.
//
// **THE CONFIGURATION CONTRACT, ECHOED ON THE LINE ITSELF.** This is a
// PLAIN-WINDOW instrument, for the same reason the sender's `fa=` is. Under
// generation coding every arrival is coded: `src_n = 0`, both FALSE classes
// are structurally empty, and `fill_coded` counts the ORDINARY carrier rather
// than any repair. So the line carries `gen=` and `rep_n=` and a reader never
// has to infer which machine a row belongs to. MEASURED: the generation arm
// of the run above emits `gen=1 … src_n=0`.
//
// **READ-ONLY.** Every counter here is fed from an `&self` probe of state the
// decoder already keeps. No control flow, no law, no default, no gate.

/// One receiver-observed source arrival's repair class — see the `[RFA]`
/// commentary above. `NotRepair` is the ordinary case (a first, in-order
/// source arrival) and is NOT a fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvRepair {
    /// First resolution of this seq and it was not overdue — the ordinary
    /// forward progress of the stream. Not a repair event.
    NotRepair,
    /// First resolution of a seq the receiver was already overdue on.
    /// A repair that WORKED (or a reordered original — see the note).
    FillSource,
    /// A second SOURCE copy of a seq already seen as source. FALSE repair.
    DupSource,
    /// A source arrival for a seq the decoder had already reconstructed from
    /// coded repair. FALSE repair.
    PreemptedSource,
}

impl RecvRepair {
    /// Is this class a FIRE (any repair-class event at all)?
    pub fn is_fire(self) -> bool {
        !matches!(self, RecvRepair::NotRepair)
    }
    /// Is this class a FALSE repair — a repair emitted whose original
    /// arrived anyway?
    pub fn is_false(self) -> bool {
        matches!(self, RecvRepair::DupSource | RecvRepair::PreemptedSource)
    }
}

/// Classify ONE source-symbol arrival, from the decoder's own `seq_probe`
/// and the receiver's own frontier. Pure, total, and pinned by test.
///
/// * `seen_as_source` — the decoder's dup filter has already recorded a
///   SOURCE arrival of this seq.
/// * `recovered` — the decoder already holds reconstructed data for the seq.
/// * `overdue` — a strictly higher seq had already arrived when this one
///   landed, so this seq was a hole rather than forward progress.
///
/// The order of the arms is the semantics: `seen_as_source` DOMINATES,
/// because a second source copy is a wasted transmission regardless of what
/// the decoder had reconstructed in the meantime.
pub fn classify_recv_repair(seen_as_source: bool, recovered: bool, overdue: bool) -> RecvRepair {
    if seen_as_source {
        RecvRepair::DupSource
    } else if recovered {
        RecvRepair::PreemptedSource
    } else if overdue {
        RecvRepair::FillSource
    } else {
        RecvRepair::NotRepair
    }
}

/// The `[RFA]` line — the receiver-site class breakdown behind `[RACK]`'s
/// `fa=`. Cumulative counters: the LAST line of a log is the reading, the
/// same convention `[WIDLE]` and `[FDIAG]` use.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn rfa_report_line(
    fill_coded: u64,
    fill_src: u64,
    dup_src: u64,
    preempt_src: u64,
    src_n: u64,
    rep_n: u64,
    gen: bool,
    rep_redundant: u64,
    late_after_aban: u64,
) -> String {
    let fires = fill_coded + fill_src + dup_src + preempt_src;
    let falses = dup_src + preempt_src;
    let frac = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    format!(
        "[RFA] gen={} fires={} false={} false_frac={:.4} fill_coded={} \
         fill_src={} dup_src={} preempt_src={} src_n={} rep_n={} \
         nu_recv={:.5} fa_class={:.4} rep_redundant={} late_after_aban={}",
        gen as u8,
        fires,
        falses,
        frac(falses, fires),
        fill_coded,
        fill_src,
        dup_src,
        preempt_src,
        src_n,
        rep_n,
        frac(fires, src_n),
        RACK_SPURIOUS_BUDGET,
        rep_redundant,
        late_after_aban,
    )
}

// ── `[FCAUSE]`: WHY EACH RECOVERY FIRE FIRED ─────────────────────────────
//
// **THE QUESTION THIS ANSWERS, AND WHY IT IS OPEN.** The quantile-native
// sweep (goal-gate, "qnative sweep SCORED") moved the realized recovery
// clock `W` cleanly across arms — a 200× span in the contract α — and the
// commanded false-alarm fraction `[RACK] fa_frac` did not move at 4 of 5
// cells. `fa ⊥ W`. §16.69's measurand (the ack-arrival distribution the
// waiting time is positioned on) is therefore WRONG: a clock that the fires
// do not respond to cannot be the thing that decides them. Both routes'
// shared premise — that the recovery fires are TIMER-DRIVEN, so positioning
// the timer repositions the fires — is refuted by that independence.
//
// The only explanation the code leaves standing is that MOST FIRES ARE NOT
// TIMER-DRIVEN. This gauge classifies them and closes the question with a
// count instead of an argument.
//
// **THE CAUSAL STRUCTURE, READ OFF THE CODE.** `record_fire`'s ONE call site
// is the sender's gap loop, and that loop's `gaps` vector has exactly two
// producers:
//
//   * the sender's OWN tail-sweep deadline arm — `pending_gaps =
//     Some(vec![(seq, seq)])` — which is the arm the sender's recovery clock
//     actually clocks (`sweep_timeout_us` → `tail_deadline`). THIS, and
//     only this, is a timer-driven fire in the sense §16.69 assumed.
//   * the `nack_rx` channel, whose sole producer is the SACK→gap inversion
//     in the WindowAck handler. These fires are clocked by the RECEIVER, not
//     by the sender's `W` at all.
//
// **THE RECEIVER'S TWO ARMS ARE SEPARABLE ON THE WIRE, FOR FREE.** The
// receiver emits a SACK-bearing WindowAck from two places, and they are
// already distinguishable in the message the sender is holding:
//
//   * the DATA arm (`gap_report_due`, the dupack analog) carries the real
//     `echo_send_timestamp_us` of the batch that triggered it;
//   * the timer-driven HOLE RE-ADVERTISEMENT arm broadcasts one message to
//     every live path and so cannot carry a per-path echo — it sets
//     `echo_send_timestamp_us: 0`, the "no counter payload" SENTINEL that
//     site already documents and that `on_window_ack` already branches on
//     for its RTT update.
//
// So the split costs NO wire change and NO behaviour change: it reads a
// field the handler has in scope. That matters for the measurand question,
// because the refresh arm is clocked by `hole_refresh` — the RECEIVER's
// twin of the very law under test. A fire in `gap_refresh` is timer-driven
// by the receiver's clock; a fire in `gap_data` is driven by DATA ARRIVAL
// and by no clock at all.
//
//   n = timer + gap_data + gap_refresh + other
//
// **THE ALIASING, DISCLOSED.** `echo == 0` is a sentinel, not a proof: a
// data-arm ack whose batch genuinely carried `batch_send_ts == 0` would be
// misfiled as `gap_refresh`. The engine stamps `send_timestamp_us: now_us()`
// on every batch, so a zero there is a wall-clock impossibility rather than
// a rare event — but this is an inference from the producer, not from this
// gauge, so it is named here rather than asserted away.
//
// **WHAT CANNOT BE CLASSIFIED, NAMED RATHER THAN GUESSED.** `other` is not
// decoration. A gap batch that reaches the loop through neither tagged
// producer would land there, and the reachability test asserts it is EMPTY
// on the configurations measured rather than assuming it must be. No fire is
// attributed by inference; a fire whose cause tag is absent is counted as
// `other` and reported as such.
//
// **THE DENOMINATOR DISCREPANCY, REPORTED RATHER THAN REPAIRED.** `[RACK]`'s
// `fired` is bumped only inside `if let Some(mp_flight)` — a fire whose
// target has no live-flight record is EMITTED to the wire but never counted,
// so `fa=`'s denominator undercounts. This gauge counts at the emission
// itself, after every `continue`, so `n` is the true fire count. Both are
// printed and `unattr = n - fired` names the gap. `fired` is NOT changed:
// it is the number the sweep already scored, and moving it would silently
// re-base every prior reading.
//
// **READ-ONLY.** A tag rides the existing gap channel and a counter is
// bumped. No law, no threshold, no gate, no default, no control flow.

/// Why one recovery fire fired — see the `[FCAUSE]` commentary above.
///
/// This is a LABEL carried alongside a gap batch, never a selector: no code
/// path, law, or constant anywhere branches on it. Only counters read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FireCause {
    /// The sender's own tail-sweep deadline expired — the ONE cause the
    /// quantile/Cantelli recovery clock `W` actually clocks.
    Timer,
    /// A SACK-bearing WindowAck from the receiver's DATA arm (the dupack
    /// analog): driven by data arrival, by no clock.
    GapData,
    /// A SACK-bearing WindowAck from the receiver's timer-driven hole
    /// re-advertisement arm: clocked by `hole_refresh` at the RECEIVER.
    GapRefresh,
    /// A gap batch that reached the fire site carrying no cause tag. Counted,
    /// never guessed at.
    #[default]
    Other,
}

impl FireCause {
    /// **THE WIRE FORM** -- the v8 `RepairRequest`'s `cause` byte. A plain
    /// `u8` so a future cause never renumbers a wire variant, and a total
    /// function in BOTH directions so an unknown byte from a future peer
    /// reads as [`FireCause::Other`] (*a batch that reached the fire site
    /// carrying no cause this binary knows*) rather than panicking or being
    /// guessed at.
    pub fn as_u8(self) -> u8 {
        match self {
            FireCause::Timer => 0,
            FireCause::GapData => 1,
            FireCause::GapRefresh => 2,
            FireCause::Other => 3,
        }
    }

    /// The inverse of [`Self::as_u8`], total: anything else is `Other`.
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => FireCause::Timer,
            1 => FireCause::GapData,
            2 => FireCause::GapRefresh,
            _ => FireCause::Other,
        }
    }

    /// The class NAME, for the `[REQS]` echo. Same vocabulary as
    /// `[FCAUSE]`'s own columns, so a request's cause and a fire's cause are
    /// read off one dictionary.
    pub fn as_str(self) -> &'static str {
        match self {
            FireCause::Timer => "timer",
            FireCause::GapData => "gap_data",
            FireCause::GapRefresh => "gap_refresh",
            FireCause::Other => "other",
        }
    }
}

/// **THE `[REQS]` LINE -- WHAT THE SENDER DID WITH THE RECEIVER'S REQUESTS**
/// (paper 16.83 arms (A)/(B)). Cumulative; the LAST line of a log is the
/// reading, the `[RACK]`/`[RFA]`/`[FCAUSE]` convention.
///
/// **TWO-SIDED.** `on=0` with every count at 0 is the CONTROL's reading and
/// is emitted on every diagnosed run, so "the arm never reached the wire" is
/// a reading rather than an inference from a missing line.
///
/// `WA1` is the pair `wa1_some`/`wa1_none`: 16.83.3's soundness precondition,
/// COUNTED. `none` means `generate_repair_range` refused the span and the
/// answer fell back to a per-seq copy -- which is the shipped machine with
/// extra latency, and the refuter arm (B) is scored against.
///
/// Fractions render `-` on a zero denominator, never 0.
#[allow(clippy::too_many_arguments)]
pub fn reqs_report_line(
    on: bool,
    reports: u64,
    spans: u64,
    m_max: u64,
    copy: u64,
    coded: u64,
    wa1_some: u64,
    wa1_none: u64,
    stale: u64,
    budget_bound: u64,
    open_wants: u64,
    cause: FireCause,
) -> String {
    let served = copy + coded;
    let wa1_n = wa1_some + wa1_none;
    let frac = |n: u64, d: u64| {
        if d == 0 {
            "-".to_string()
        } else {
            format!("{:.4}", n as f64 / d as f64)
        }
    };
    format!(
        "[REQS] on={} reports={} spans={} m_max={} served={} copy={} coded={} \
         wa1_some={} wa1_none={} wa1_none_frac={} stale={} budget_bound={} \
         open_wants={} cause={}",
        u8::from(on),
        reports,
        spans,
        m_max,
        served,
        copy,
        coded,
        wa1_some,
        wa1_none,
        frac(wa1_none, wa1_n),
        stale,
        budget_bound,
        open_wants,
        cause.as_str(),
    )
}

/// The `[FCAUSE]` line — the per-cause breakdown of every recovery fire the
/// sender emitted. Cumulative counters; the LAST line of a log is the
/// reading, the same convention `[RACK]` and `[RFA]` use.
///
/// Fractions render as `-` when their denominator is zero, so an absent
/// reading is never confusable with a measured zero.
pub fn fcause_report_line(
    timer: u64,
    gap_data: u64,
    gap_refresh: u64,
    other: u64,
    fired: u64,
    gen: bool,
) -> String {
    let n = timer + gap_data + gap_refresh + other;
    let frac = |v: u64| {
        if n == 0 {
            "-".to_string()
        } else {
            format!("{:.4}", v as f64 / n as f64)
        }
    };
    format!(
        "[FCAUSE] gen={} n={} timer={} gap_data={} gap_refresh={} other={} \
         timer_frac={} gap_frac={} fired={} unattr={} fa_class={:.4}",
        gen as u8,
        n,
        timer,
        gap_data,
        gap_refresh,
        other,
        frac(timer),
        frac(gap_data + gap_refresh),
        fired,
        n.saturating_sub(fired),
        RACK_SPURIOUS_BUDGET,
    )
}

/// The `[RACK]` / `[RFA]` / `[FCAUSE]` tally: recovery-fire accounting at
/// the sender and the receiver.
#[derive(Default)]
pub(crate) struct RackClockGauge {
    /// §16.68.1's FALSE-ALARM VALIDATION, and it runs on EVERY arm including
    /// the shipped control — the `[25, 100] ms` clamp's own false-alarm rate
    /// has never been measured in this tree, and a cited empirical fraction
    /// that has not been validated against the thing it is empirical ABOUT
    /// does not clear this repository's bar.
    ///
    /// `fired` — recovery rounds that fired. `spurious` — those whose target's
    /// live flight was YOUNGER than its own per-path law threshold, i.e. the
    /// data was going to arrive anyway (the existing spurious-by-law class,
    /// read here ungated by `RWM_DIAG` so it is available to every arm).
    /// Scored against RFC 8985 §6.2 Step 4's own published budget,
    /// [`RACK_SPURIOUS_BUDGET`] = 1/16 = 6.25 %.
    fired: u64,
    spurious: u64,
    /// ── THE RECEIVER-SITE CLASSES (`[RFA]`) ──────────────────────────
    /// See the `[RFA]` commentary above [`RACK_SPURIOUS_BUDGET`] for the
    /// event-class definition each of these counts. They FEED `fired` /
    /// `spurious` above, so a receiver-role `[RACK]` line reports the
    /// REALIZED false-repair fraction on the same two slots the sender uses
    /// for its PREDICTED one.
    fill_coded: u64,
    fill_src: u64,
    dup_src: u64,
    preempt_src: u64,
    /// Every source-symbol arrival. The `ν_recv` denominator.
    src_n: u64,
    /// Every REPAIR-symbol arrival. Carried for the CONFIGURATION CONTRACT
    /// and for nothing else: under generation coding (§16.3) every arrival is
    /// coded, so `src_n = 0` and the two FALSE classes are structurally empty
    /// — the same configuration fact that empties the SENDER's `fa=`. Printing
    /// both makes that readable off one line instead of inferred.
    rep_n: u64,
    /// Is generation coding on at this receiver? Echoed as `[RFA] gen=` so
    /// the line says which machine it is a measurement of.
    recv_gen: bool,
    /// **`rep_redundant = repairs_fed - repairs_useful`** -- THE FALSE
    /// MEASURAND UNDER CODED ANSWERS (paper 16.83). `dup_src` / `preempt_src`
    /// count a wasted SOURCE copy; under a coded answer "the original arrived
    /// anyway" is inexpressible, and the waste instead shows up as an
    /// equation that added no rank. Mirrored from the decoder's own counters
    /// at the readout, so this gauge holds no second copy of them.
    rep_redundant: u64,
    /// **`late_after_aban`** -- a SOURCE arrival for a seq strictly below the
    /// in-order frontier: a copy that landed after the frontier had already
    /// moved past it. STRUCTURALLY ZERO under the reliable window (the buffer
    /// never delivers past a hole), so a nonzero reading there is a finding;
    /// under the EVICT (rho < 1) seat it is the repair waste that seat has
    /// never been scored on, and the counter `tools/l1/tail_matrix.sh`'s
    /// scrape reads. **The name is part of the `[RFA]` line's contract** --
    /// the Track B scrape greps for it.
    late_after_aban: u64,
    /// ── THE FIRE-CAUSE CLASSES (`[FCAUSE]`) ──────────────────────────
    /// One counter per [`FireCause`], bumped at the EMISSION of every
    /// recovery fire — after every suppression `continue`, so their sum is
    /// the true fire count rather than `fired`'s flight-attributed subset.
    /// See the `[FCAUSE]` commentary above [`FireCause`].
    cause_timer: u64,
    cause_gap_data: u64,
    cause_gap_refresh: u64,
    cause_other: u64,
    /// Is generation coding on at this SENDER? Echoed as `[FCAUSE] gen=`:
    /// under generation the SACK→gap producer is suppressed, so both `gap_`
    /// classes are STRUCTURALLY empty and the line must say so on its face.
    send_gen: bool,
}

impl RackClockGauge {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record one recovery-round FIRE and whether it was a false alarm —
    /// §16.68.1. Fed on every arm; observation only.
    pub(crate) fn record_fire(&mut self, spurious: bool) {
        self.fired += 1;
        if spurious {
            self.spurious += 1;
        }
    }

    /// Record ONE source-symbol arrival at the RECEIVER, in its repair class
    /// (`classify_recv_repair`). Every repair-class arrival is a FIRE and the
    /// two redundant classes are the FALSE ones, so this is the receiver's
    /// feed for the same `fa=<spurious>/<fired>` slots the sender feeds.
    /// Observation only.
    pub(crate) fn record_recv_source(&mut self, class: RecvRepair) {
        self.src_n += 1;
        self.record_fire_for(class);
    }

    fn record_fire_for(&mut self, class: RecvRepair) {
        if !class.is_fire() {
            return;
        }
        match class {
            RecvRepair::NotRepair => unreachable!("is_fire() excluded it"),
            RecvRepair::FillSource => self.fill_src += 1,
            RecvRepair::DupSource => self.dup_src += 1,
            RecvRepair::PreemptedSource => self.preempt_src += 1,
        }
        self.record_fire(class.is_false());
    }

    /// Record ONE repair-symbol arrival — the configuration-contract
    /// denominator, not a fire.
    pub(crate) fn record_recv_repair_arrival(&mut self) {
        self.rep_n += 1;
    }

    /// Record ONE seq reconstructed by the DECODER rather than by its own
    /// source arrival — a repair that WORKED. A fire, never false.
    pub(crate) fn record_recv_coded_fill(&mut self) {
        self.fill_coded += 1;
        self.record_fire(false);
    }

    /// Mirror the decoder's `repairs_fed - repairs_useful` onto the gauge at
    /// the readout. Observation only.
    pub(crate) fn set_rep_redundant(&mut self, n: u64) {
        self.rep_redundant = n;
    }

    /// Record ONE source arrival below the in-order frontier -- a copy that
    /// arrived after the give-up. Observation only.
    pub(crate) fn record_late_after_aban(&mut self) {
        self.late_after_aban += 1;
    }

    /// Echo which machine this receiver is: generation coding on or off.
    pub(crate) fn set_recv_generation(&mut self, gen: bool) {
        self.recv_gen = gen;
    }

    /// Echo which machine this SENDER is — the `[FCAUSE]` configuration
    /// contract. Observation only.
    pub(crate) fn set_send_generation(&mut self, gen: bool) {
        self.send_gen = gen;
    }

    /// Record the CAUSE of one recovery fire that reached the wire.
    ///
    /// Called at the emission itself, so `fcause_n()` counts every fire —
    /// including the ones `record_fire` drops for want of a live-flight
    /// record. Observation only; nothing branches on the cause.
    pub(crate) fn record_fire_cause(&mut self, cause: FireCause) {
        match cause {
            FireCause::Timer => self.cause_timer += 1,
            FireCause::GapData => self.cause_gap_data += 1,
            FireCause::GapRefresh => self.cause_gap_refresh += 1,
            FireCause::Other => self.cause_other += 1,
        }
    }

    /// The true fire count — the sum of the four cause classes.
    pub(crate) fn fcause_n(&self) -> u64 {
        self.cause_timer + self.cause_gap_data + self.cause_gap_refresh + self.cause_other
    }

    /// Has this gauge classified ANY fire, i.e. does it sit at a SENDER that
    /// ran the gap loop? A receiver-role gauge never does and stays silent.
    pub(crate) fn is_fire_cause_site(&self) -> bool {
        self.fcause_n() > 0
    }

    /// The `[FCAUSE]` line this gauge would emit right now.
    pub(crate) fn fcause_line(&self) -> String {
        fcause_report_line(
            self.cause_timer,
            self.cause_gap_data,
            self.cause_gap_refresh,
            self.cause_other,
            self.fired,
            self.send_gen,
        )
    }

    /// Has this gauge seen ANY symbol arrival, i.e. does it sit at a
    /// RECEIVER? A sender-role gauge sees none and never emits `[RFA]`.
    pub(crate) fn is_receiver_site(&self) -> bool {
        self.src_n > 0 || self.rep_n > 0
    }

    /// The `[RFA]` line this gauge would emit right now.
    pub(crate) fn rfa_line(&self) -> String {
        rfa_report_line(
            self.fill_coded,
            self.fill_src,
            self.dup_src,
            self.preempt_src,
            self.src_n,
            self.rep_n,
            self.recv_gen,
            self.rep_redundant,
            self.late_after_aban,
        )
    }

    /// The `[RACK]` line this gauge would emit right now.
    pub(crate) fn rack_line(&self) -> String {
        rack_report_line(self.fired, self.spurious)
    }
}

impl Drop for RackClockGauge {
    fn drop(&mut self) {
        // Emitted whenever a recovery round fired — §16.68.1's validation of
        // the shipped clamp's false-alarm rate. A run that fired nothing
        // stays silent.
        if self.fired > 0 {
            eprintln!("{}", self.rack_line());
        }
        // The receiver-site class breakdown behind that `fa=`. Emitted on the
        // SAME rule and with NO gate of its own: a gauge that saw source
        // arrivals sits at a receiver and owes the breakdown; a sender-role
        // gauge never sees one and stays silent. NOTE that the L1 harnesses
        // SIGKILL the server, so this `Drop` is not reachable there — the
        // receiver also emits `[RFA]` on a cadence under the EXISTING
        // `RWM_DIAG`/`RWM_FDIAG` gates, and its last line is the reading.
        if self.is_receiver_site() {
            eprintln!("{}", self.rfa_line());
        }
        // The SENDER-site cause breakdown behind that same `fa=`. Same rule,
        // no gate of its own: a gauge that classified a fire ran the gap loop
        // and owes the breakdown. Two-sided — it is emitted on the clock-OFF
        // arm too, because "what fires when the clock is DISARMED" is the
        // shipped machine's reading and the whole point of the question.
        if self.is_fire_cause_site() {
            eprintln!("{}", self.fcause_line());
        }
    }
}

/// ── THE SENDER-TEARDOWN GAUGE CARRIER (`[WALL]` + `[CCAP]`) ──────────────
///
/// The run's ONE emission of both teardown gauges, bound to the **LIFETIME of
/// the sender loop** rather than to any of its exit arms.
///
/// **Why this is a `Drop` and not two `eprintln!`s at a `return`.** The
/// gauges shipped emitted from exactly two arms of `run_window_sender`'s
/// `select!` — the `shutdown_rx` arm and the `packet == None` ("TUN closed")
/// arm. The `perf` harness (`crate::perf`, the object benchmark every L1
/// battery runs) takes NEITHER: `perf::client` finishes its objects, prints
/// its summary and returns, dropping the engine `JoinHandle` without
/// signalling shutdown and without closing the memory TUN; the sender task
/// then lives until the runtime is dropped at the end of `main`. The
/// pre-battery smoke measured the consequence — `window sender shut down
/// gracefully` 0/4 and `TUN closed` 1/4, so `[CCAP]` appeared on 0 of 2 logs
/// and `[WALL]` on 1 of 4 (goal-gate, "PRE-BATTERY SMOKE"). The renderers
/// were pinned; REACHABILITY never was — ADR-0070's own postmortem shape, one
/// layer down.
///
/// A destructor is the only site that is on EVERY exit path a sender can
/// take: the graceful-shutdown arm, the TUN-closed arm, an early `return`,
/// an unwind, and — the case the harness actually exercises — the task future
/// being dropped at runtime shutdown. It also emits exactly ONCE by
/// construction, which the two arms only achieved because each happened to
/// `return` immediately after.
///
/// It carries the `[CCAP]` tally as fields so that the counters and their
/// emission cannot drift apart again. `[WALL]`'s own state lives in
/// `net::walldiag`'s process-global gauge; only its emission is carried here.
///
/// OBSERVATION ONLY, and on the shipped default a no-op: `RWM_WALLDIAG` ships
/// OFF so `report_at_teardown` returns on a `None` gauge, and `composed_cap`
/// is `RWM_COMPOSED_CAP`, also OFF. Nothing here is read by the engine.
pub(crate) struct SenderTeardownGauges {
    /// `[CCAP]` tally — dyn-cap refresh ticks under the composed law.
    pub(crate) refreshes: u64,
    /// Refresh ticks at which the law actually produced a value (warm).
    pub(crate) engaged: u64,
    /// Engaged ticks clamped by the `WIN_STORE_MAX` memory bound.
    pub(crate) at_mem: u64,
    /// Engaged ticks clamped by the paroled `store_cap_floor`.
    pub(crate) at_floor: u64,
    /// Σ realized cap over refreshes (the mean is rendered).
    pub(crate) cap_sum: f64,
    /// Late-stage cwnd brake: ticks armed.
    pub(crate) brake_ticks: u64,
    /// Late-stage cwnd brake: ticks closed.
    pub(crate) brake_closed: u64,
    /// Σ over ENGAGED refreshes of [`SpanForms::shipped`] — C9-L1's field.
    pub(crate) span_sum: f64,
    /// Σ over engaged refreshes of [`SpanForms::sigma`] — the crosscheck form,
    /// reported so C9-L3's ratio is measured. Read by nothing.
    pub(crate) span_sigma_sum: f64,
    /// Σ over engaged refreshes of [`SpanForms::rate_fast`] — C9-L3's ±5 %
    /// anchor.
    pub(crate) rate_fast_sum: f64,
    /// Σ over engaged refreshes of [`SpanForms::spread_s`] — C9-L3's ±0.4 %
    /// anchor. Rendered as µs.
    pub(crate) spread_s_sum: f64,
    /// `RWM_COMPOSED_CAP` — whether the `[CCAP]` line is emitted at all.
    composed_cap: bool,
    /// `store_cap_floor`, rendered as `floor_val=` (provenance, ADR-0070/5).
    floor: usize,
}

impl SenderTeardownGauges {
    pub(crate) fn new(composed_cap: bool, floor: usize) -> Self {
        Self {
            refreshes: 0,
            engaged: 0,
            at_mem: 0,
            at_floor: 0,
            cap_sum: 0.0,
            brake_ticks: 0,
            brake_closed: 0,
            span_sum: 0.0,
            span_sigma_sum: 0.0,
            rate_fast_sum: 0.0,
            spread_s_sum: 0.0,
            composed_cap,
            floor,
        }
    }

    /// Fold ONE engaged refresh's span geometry into the tally. Called at the
    /// same refresh that fed `engaged`, so every span mean's denominator IS
    /// `eng=`'s numerator and a parser needs no second liveness field.
    pub(crate) fn record_span(&mut self, s: SpanForms) {
        self.span_sum += s.shipped;
        self.span_sigma_sum += s.sigma;
        self.rate_fast_sum += s.rate_fast;
        self.spread_s_sum += s.spread_s;
    }

    /// The `[CCAP]` line this carrier would emit right now. Split out so the
    /// reachability tests and the format pins share one renderer with the
    /// destructor.
    pub(crate) fn ccap_line(&self) -> String {
        ccap_report_line(
            self.refreshes,
            self.engaged,
            self.at_mem,
            self.at_floor,
            self.cap_sum,
            self.brake_ticks,
            self.brake_closed,
            self.floor,
            self.span_sum,
            self.span_sigma_sum,
            self.rate_fast_sum,
            self.spread_s_sum,
        )
    }
}

impl Drop for SenderTeardownGauges {
    fn drop(&mut self) {
        // The run's ONE `[WALL]` line (`RWM_WALLDIAG`), then the run's ONE
        // `[CCAP]` line (`RWM_COMPOSED_CAP`) — same order the two teardown
        // arms used, so an L1 parser written against the smoke's logs is
        // unaffected.
        walldiag::report_at_teardown(now_us());
        // The run's ONE `[CPUPROF]` line (`RWM_CPUPROF`) — the sender CPU
        // decomposition. Same site and same reason as `[WALL]`: `perf::client`
        // takes NEITHER teardown `select!` arm, so a destructor is the only
        // place on every exit path. Emitted unconditionally by the gauge's own
        // null check, so the shipped default prints nothing.
        cpuprof::report_at_teardown();
        if self.composed_cap {
            eprintln!("{}", self.ccap_line());
        }
    }
}
