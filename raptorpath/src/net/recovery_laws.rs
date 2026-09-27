//! Recovery-plane laws: derived patience and stall definition, multipath
//! recovery suppression, the laws extracted from the sender loop, and the
//! derived recovery round. Moved verbatim out of `net/mod.rs` (cleanup
//! Stage 3); re-exported from `net`, so every path is unchanged.

use super::*;

// ── Derived patience + derived stall definition ───────────────────────────
//
// Goal-gate "Unlock The Default 2: derived patience" (2026-08-07). Two fixed
// literals sat in the plane §16.37/§16.39 named as the c7 blocker's owner —
// the recovery plane's patience and the gauge that measures its stalls — in
// a project whose own rules say a clock must be DERIVED from the operating
// point. Both are re-expressed here as laws over measured inputs, and both
// reproduce their literal EXACTLY at the operating point where the literal's
// own assumption holds. The patience floor's own A/B arm
// (`RWM_PATIENCE_DERIVED`) was refuted and removed; the floor survives as
// the no-ceiling derived round's floor (`derived_recovery_round_us`,
// `RWM_DERIVED_SWEEP`), and the stall threshold as the `RWM_SIDLE_DERIVED`
// gauge.
//
// Neither is a dial: neither selects a law, a code path or a constructor
// argument on (δ, ρ, r), and nothing keys on a threshold in the triangle
// (CLAUDE.md's no-mode-switch invariant).

/// The engine's timer granularity (µs) — RFC 9002 §6.1.2 kGranularity,
/// DERIVED rather than borrowed.
///
/// RFC 9002 defines kGranularity as "the timer granularity … a
/// system-dependent value" and RECOMMENDS 1 ms. In this engine the finest
/// interval at which ANY recovery clock can be evaluated is the sender
/// loop's wake period: both timer arms of the send `select!` sleep exactly
/// 1 ms (the pacing refill and the backpressure poll), so a threshold below
/// that cannot be observed, let alone acted on. The engine's own granularity
/// and the RFC's recommendation coincide at 1 ms.
pub const TIMER_GRANULARITY_US: u64 = 1_000;

/// The sender loop's wake period (µs) as the emission-gap gauge's
/// OBSERVATION GRANULARITY. Same 1 ms, named separately because it plays a
/// different role: `TIMER_GRANULARITY_US` floors a recovery clock,
/// `LOOP_WAKE_US` bounds what the gauge can resolve.
pub(crate) const LOOP_WAKE_US: u64 = 1_000;

/// The derived recovery-patience floor (µs): timer granularity + the path's
/// OWN measured RTT jitter.
///
/// This replaces `NACK_RETX_COOLDOWN_FLOOR_US`'s 10 ms — 10× RFC 9002's
/// kGranularity — at the two sites where that literal is BEHAVIOURAL (the
/// kGranularity analog inside `mp_time_threshold_split`, and the per-seq
/// retransmit cooldown). At c2/c7 (RTprop ≈ 8–10 ms) the literal is at or
/// above the 9/8·srtt term it was meant to floor, so recovery patience was a
/// CONSTANT rather than a property of the path.
///
/// Both terms are already measured in-tree and neither is invented here:
/// `TIMER_GRANULARITY_US` above, and `jitter_us` = the path's
/// consecutive-difference RTT jitter (`PathState::rtt_jitter_us()`, Copa's
/// RFC 3550-style EWMA widened by its window-level twin exactly as the Copa
/// backoff threshold does, with the loss estimator's interarrival jitter as
/// the pre-Copa-sample fallback). The jitter term is clamped at one srtt so
/// a pathological estimate cannot make patience unbounded.
///
/// With NO clock at all (`srtt_us == 0`, before the first sample) there is
/// nothing to derive from and the legacy floor is kept verbatim — an
/// information-availability fallback, not a mode.
///
/// RFC 9002's kTimeThreshold (9/8) and kPacketThreshold (3) are UNTOUCHED:
/// they are cited — 9/8 as RFC 9002's EMPIRICAL recommendation ("Experience
/// with QUIC shows that 9/8 works well"; RACK uses 5/4), corrected 2026-08-19,
/// cross-check item 6(d). Only the floor is derived.
pub fn patience_floor_us(jitter_us: u64, srtt_us: u64) -> u64 {
    if srtt_us == 0 {
        return NACK_RETX_COOLDOWN_FLOOR_US;
    }
    TIMER_GRANULARITY_US.saturating_add(jitter_us.min(srtt_us))
}

/// The derived STALL threshold (µs) for the emission-gap gauges — the
/// definition `sidle`/`widle` count against.
///
/// The legacy `SIDLE_GAP_MIN_US` / `WIDLE_GAP_MIN_US` = 3 ms is 3 ×
/// `LOOP_WAKE_US`, i.e. "three times the nominal inter-emission interval",
/// with the loop wake STANDING IN for that interval. The substitution is
/// valid only while emission EVENTS are at least as frequent as loop wakes.
/// Emission batching (`RWM_EMIT_BATCH`) exists precisely to make them
/// rarer — one counter change now covers a whole batch — so the nominal
/// inter-EVENT interval rises above 1 ms by construction and a fixed 3 ms
/// begins counting ordinary pacing intervals as stalls.
///
/// The law keeps the legacy FORM and replaces the ASSUMED nominal interval
/// with the MEASURED one (`evt_us`, the mean inter-event interval over the
/// previous diagnostic window):
///
/// ```text
///   3 · max(evt_us, LOOP_WAKE_US)   clamped to [3 ms, HOLE_NACK_REFRESH_MIN]
/// ```
///
/// No new constant is introduced: the multiplier 3 IS the legacy
/// `SIDLE_GAP_MIN_US / LOOP_WAKE_US`; the floor IS the legacy constant; the
/// ceiling is the engine's own hole-refresh cadence (a wire gap longer than
/// the interval at which the receiver re-advertises a stalled hole is a
/// stall at any operating point — the ceiling stops the derived gauge going
/// blind at very slow cells).
///
/// COINCIDENCE PROPERTY (unit-tested): whenever `evt_us ≤ LOOP_WAKE_US` the
/// law returns exactly 3 000 µs — the legacy constant, to the microsecond.
/// The derived gauge is a strict generalization that reproduces the legacy
/// gauge wherever the legacy gauge's own stated assumption holds, and it is
/// one-directional by construction: the derived stall total can never exceed
/// the legacy one.
pub(crate) fn stall_threshold_us(evt_us: u64) -> u64 {
    const STALL_GAP_MIN_US: u64 = 3_000;
    let nominal = evt_us.max(LOOP_WAKE_US);
    nominal
        .saturating_mul(STALL_GAP_MIN_US / LOOP_WAKE_US)
        .clamp(STALL_GAP_MIN_US, HOLE_NACK_REFRESH_MIN.as_micros() as u64)
}

// ── Multipath recovery suppression (branch feat/recovery-suppression, env
//    `RWM_RECOV_MP`, default OFF ⇒ shipped path byte-identical) ─────────────
//
// The fifth control-plane wall (goal-gate "Engine Parallelization" STEP 1d):
// under dual-path striping the recovery plane roughly DOUBLES its per-source
// retransmit share and ×2.2–2.5s its repair share vs the same config run
// single-path; at dual-c1 (GE 0.1%, nothing real to recover) the sender
// retransmits 9.3% of source (single-path: 0.2%, ×46) and the dual sink
// aggregates BELOW one path alone. The root defects are two instances of ONE
// mistake: recovery clocks/serials are GLOBAL where multipath demands they be
// PER-PATH.
//
// (1) The hole law. A SACK gap is evidence of loss on a SINGLE path (FIFO
//     within the path); across paths a seq gap is NORMAL — the scheduler
//     CREATED it (striping + inter-path delay skew). The legacy age gate
//     (age ≥ max-path-SRTT/2 since the ORIGINAL send) fires while the
//     symbol's own flight is still in the air, and after a retransmit the
//     clock is never reset to the NEW flight, so an open (scheduler-created)
//     gap re-fires every cooldown while copies are still flying — the
//     feedback flood. The law here is RFC 9002 §6.1.2 time-threshold loss
//     detection generalized per path (the packet-threshold channel is
//     deliberately NOT used across paths — cross-path seq gaps are exactly
//     the RFC 4737 reordering caveat; multipath QUIC solves the same problem
//     with per-path packet-number spaces): a reported gap seq is a candidate
//     hole only once its LIVE flight (the last (re)send) is older than
//     kTimeThreshold = 9/8 of its OWN path's smoothed RTT, with the existing
//     per-seq cooldown floor as the kGranularity analog. Suppression-only:
//     the receiver's hole-refresh keeps re-advertising, so a real hole fires
//     the moment its flight clock expires. N = 1 live path keeps the legacy
//     gates bit-exactly (single-path gaps are FIFO-real; sc2/sc3 inert).
//
// (2) The loss serials, DIAGNOSED and left global. `batch_seq` is a GLOBAL
//     counter, but the receiver's per-path `PathBatchTracker` estimates
//     expected symbols from batch_seq GAPS — so under striping every
//     path-switch reads the other path's run as loss and the per-path loss
//     estimators saturate. The per-path serial-namespace fix
//     (`RWM_RECOV_MP_SERIAL`) was diagnostically TRUE but runtime-REFUTED on
//     the post-wall substrate (honest signal re-heats every SRTT/loss-scaled
//     recovery cadence the poisoned values were accidentally damping; sender
//     CPU ×2.4, dual-c1 181→134 — goal-gate "Multipath Recovery Suppression"
//     2026-07-21) and REMOVED 2026-07-27 per the DEPRECATION REGISTER (no
//     re-test owed: refuted ON the clean substrate). A cheaper serial-
//     namespace implementation is a NEW pre-registered build, not a revival.
//
//     ANSWERED WITHOUT A NAMESPACE, 2026-08-18 (`fix/loss-crosspath`,
//     MECHANICAL DEFECT SWEEP item 3). The poisoning is removable ON THE
//     SENDER with NO wire change and no serial rework, because the sender
//     already keeps the exact quantity the receiver was guessing:
//     `PathStats::symbols_sent`, one increment per wire handoff per path.
//     Under `RWM_LOSS_SENT_TRUTH` (**default OFF**) the estimator is fed
//     `1 − Δcum_received / Δsymbols_sent` instead of the gap estimate — both
//     operands per-path measurements, both already present. The law, its
//     provenance and its named residual are on
//     `PathState::sender_truth_loss_delta`. Ledger replay over the ackdiag
//     battery: expected/received **2.05 → 1.01** (c7) and **5.59 → 0.94**
//     (c8 slow leg); ε̂ **0.51 / 0.82 → 0.007 / 0.020** against realized
//     0.0055 / 0.0196. Note that this ALSO makes the merged-ack counter path
//     no cleaner than the legacy one: `cum_expected` is the SUM of the same
//     gap estimate, so differencing it cannot remove the contamination —
//     the "ride the counter deltas" idea is refuted structurally.
//     It ships OFF because the runtime refutation above is about the honest
//     SIGNAL, not about how it was obtained, and still stands: the
//     SRTT/loss-scaled recovery cadences were tuned against the poisoned
//     values. That cadence re-derivation is the named follow-up, unchanged.
//
// Sub-gate for trace attribution: `RWM_RECOV_MP_LAW` (default ON under the
// umbrella) gates (1).

/// RFC 9002 §6.1.2 time threshold for the flight path: kTimeThreshold (9/8)
/// × max of the two smoothed RTT clocks available for the path (Copa EWMA
/// srtt and the estimator's EWMA app-echo RTT — the analog of
/// `max(smoothed_rtt, latest_rtt)`), floored at the existing per-seq
/// retransmit cooldown floor (the kGranularity analog). No new constants.
///
/// `floor_us` is the kGranularity analog, supplied by the caller: the
/// engine passes `NACK_RETX_COOLDOWN_FLOOR_US`. kTimeThreshold (9/8) is
/// untouched — cited as RFC 9002's empirical recommendation ("works well";
/// RACK uses 5/4 — corrected 2026-08-19, cross-check item 6(d)); only the
/// floor is derived.
///
/// Returns the threshold and whether the FLOOR term won (the `pf=` mechanism
/// gauge: "patience is derived" means the floor term stops winning).
pub fn mp_time_threshold_split(
    srtt_us: u64,
    ewma_rtt_us: u64,
    floor_us: u64,
) -> (u64, bool) {
    let s = srtt_us.max(ewma_rtt_us);
    let clock = s.saturating_mul(9) / 8;
    if clock >= floor_us {
        (clock, false)
    } else {
        (floor_us, true)
    }
}

/// RFC 9002 §6.1.1 packet threshold (kPacketThreshold = 3), generalized per
/// path: the FAST honest loss channel. A seq's original flight on path j is
/// declared lost as soon as ≥3 LATER path-j symbols are known delivered —
/// same-path FIFO evidence (UDP within one 5-tuple does not reorder under
/// netem/typical paths; 3 absorbs rare in-path reordering per the RFC).
/// Scheduler-created cross-path gaps can never trigger it: their same-path
/// successors are exactly as un-arrived as they are. This restores legacy
/// real-loss recovery latency (≈ one skew, not a full RTT) under the
/// time-threshold suppression. Applies to ORIGINAL flights only — a
/// retransmit's wire order is not its seq order, so retransmits are
/// governed by the time threshold alone.
pub const MP_PACKET_THRESHOLD: usize = 3;

/// The delivered intervals a gap report implies: between consecutive maximal
/// missing runs everything was SACKed, and the seq just past the last gap is
/// the SACK range that bounded it (its extent is unknown — one seq is the
/// provable minimum). Pure; unit-tested.
pub fn mp_delivered_intervals(gaps: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut out = Vec::with_capacity(gaps.len());
    for i in 0..gaps.len() {
        let lo = gaps[i].1 + 1;
        let hi = if i + 1 < gaps.len() {
            gaps[i + 1].0.saturating_sub(1)
        } else {
            gaps[i].1 + 1
        };
        if lo <= hi {
            out.push((lo, hi));
        }
    }
    out
}

/// Fast-loss decision from per-path delivered evidence (sorted seq list):
/// ≥ MP_PACKET_THRESHOLD delivered path-j seqs strictly above `s`.
pub fn mp_fast_lost(delivered_on_path: &[u64], s: u64) -> bool {
    let above = delivered_on_path.len() - delivered_on_path.partition_point(|&x| x <= s);
    above >= MP_PACKET_THRESHOLD
}

/// The skew-aware hole law (pure, unit-tested): may a reported gap seq be
/// treated as a hole (targeted retransmit eligible) right now?
///
/// * `n_live_paths <= 1`: the law is INERT — single-path gaps are FIFO-real
///   and the legacy gates own the decision (bit-exact shipped behavior).
/// * Unknown flight (no send record): legacy behavior (never suppress a seq
///   we cannot clock — the reliability backstop stays intact).
/// * Otherwise: a hole only once the LIVE flight is at least
///   `threshold_us` old — a gap on path A while the seq's flight is still
///   inside path B's expected-arrival clock is a gap the scheduler created,
///   not a hole.
pub fn mp_hole_ripe(
    n_live_paths: usize,
    now_us: u64,
    flight_send_us: Option<u64>,
    threshold_us: u64,
) -> bool {
    if n_live_paths <= 1 {
        return true;
    }
    time_threshold_ripe(now_us, flight_send_us, threshold_us)
}

// ── Laws EXTRACTED from the sender loop (goal-gate "Component Benches",
//    2026-08-08). Pure refactor: each function below reproduces, verbatim,
//    an expression that was previously inline in `run_impl`'s gap-report
//    handler, its tail-sweep arm, or the receiver's hole-refresh arm. They
//    are extracted so the recovery plane can be driven WITHOUT a transport
//    (`tests/recovery_bench.rs`) and unit-tested for good. No new
//    constants; no behaviour change. ────────────────────────────────────

/// RFC 9002 §6.1.2 time-threshold ripeness for ONE flight, path-count
/// agnostic: the LIVE flight (last (re)send) must be at least
/// `threshold_us` old. An unknown flight is ripe (never suppress a seq we
/// cannot clock — the reliability backstop). This is the body `mp_hole_ripe`
/// applies past its N ≤ 1 bypass, and verbatim the `RWM_RECOV_SP` arm's
/// inline test.
pub fn time_threshold_ripe(
    now_us: u64,
    flight_send_us: Option<u64>,
    threshold_us: u64,
) -> bool {
    match flight_send_us {
        None => true,
        Some(t) => now_us.saturating_sub(t) >= threshold_us,
    }
}

/// The LEGACY age gate (the pre-RFC-9002 channel, still the default when
/// neither `RWM_RECOV_MP` nor `RWM_RECOV_SP` is armed): a gap seq whose
/// ORIGINAL send is younger than half the pooled smoothed clock is merely
/// late, not lost. Note the asymmetry the bench exists to expose — this
/// clock is `srtt/2` where the two RFC channels use `9/8·srtt`, and the
/// `srtt` fed to it is the pooled **app-echo** RTT (see
/// `pooled_recovery_srtt_us`).
pub fn legacy_age_ripe(now_us: u64, send_time_us: u64, srtt_us: u64) -> bool {
    now_us.saturating_sub(send_time_us) >= srtt_us / 2
}

/// The POOLED recovery clock (µs): the MAX smoothed RTT over the live
/// paths, falling back to the legacy floor when no path has a sample yet.
///
/// THIS is the argument the component bench interrogates. The samples fed
/// here are the ESTIMATOR's app-echo RTT, which is store-dwell inclusive
/// (ADR-0062 / §16.34: `QuicTransport::wire_rtt` is the dwell-free twin) —
/// so the legacy age gate, the per-seq cooldown and the tail sweep all
/// inherit the dwell through this one reduction.
pub fn pooled_recovery_srtt_us(path_rtt_us: &[u64]) -> u64 {
    path_rtt_us.iter().copied().max().unwrap_or(NACK_RETX_COOLDOWN_FLOOR_US)
}

/// The path set the sender's recovery clocks pool over: the tail-sweep
/// clock, the per-seq retransmit cooldown's pooled SRTT/jitter, and the
/// repair margin's loss rate. LIVE paths, as the receiver's recovery timer
/// uses (`receiver.rs`, `live_paths()`): the saturation-filtered
/// `active_paths()` (`available() > 0`) is empty exactly when every path is
/// cwnd-full, which collapsed the clocks to the 10 ms floor / zero margin.
pub fn recovery_clock_paths(sched: &Scheduler) -> Vec<crate::scheduler::PathId> {
    sched.live_paths()
}

/// [`pooled_recovery_srtt_us`] over [`recovery_clock_paths`]: the pooled
/// app-echo RTT the recovery clocks run on.
pub fn pooled_recovery_srtt_of(sched: &Scheduler) -> u64 {
    let rtts: Vec<u64> = recovery_clock_paths(sched)
        .iter()
        .filter_map(|id| sched.path(*id))
        .map(|p| p.estimator.rtt().as_micros() as u64)
        .collect();
    pooled_recovery_srtt_us(&rtts)
}

/// The per-seq retransmit cooldown clock (µs): the pooled smoothed RTT,
/// floored. The engine passes `NACK_RETX_COOLDOWN_FLOOR_US` as `floor_us`.
pub fn retx_cooldown_us(srtt_us: u64, floor_us: u64) -> u64 {
    srtt_us.max(floor_us)
}

/// Has a seq's per-seq retransmit cooldown elapsed? (`false` ⇒ the service
/// is suppressed by the cooldown channel.)
pub fn cooldown_elapsed(now_us: u64, last_retx_us: u64, cooldown_us: u64) -> bool {
    now_us.saturating_sub(last_retx_us) >= cooldown_us
}

/// P10b tail-sweep timeout (µs): 2×SRTT clamped to
/// [`TAIL_SWEEP_MIN_US`, `TAIL_SWEEP_MAX_US`]. The last symbols of a burst
/// have no successors, so the receiver can never SACK a gap behind them —
/// this is the sender's own stall detector.
pub fn tail_sweep_timeout_us(srtt_us: u64) -> u64 {
    (srtt_us.saturating_mul(2)).clamp(TAIL_SWEEP_MIN_US, TAIL_SWEEP_MAX_US)
}

/// The shipped clamp's own ASPECT RATIO,
/// `HOLE_NACK_REFRESH_MAX / HOLE_NACK_REFRESH_MIN` = 100 ms / 25 ms = **4**.
///
/// Paper §16.78.0. This is NOT a new constant: it is the ratio the two
/// existing literals already stand in, computed from them rather than
/// written down, so that [`hole_nack_refresh_floored`] at
/// `floor = HOLE_NACK_REFRESH_MIN` reproduces the shipped law's output for
/// EVERY input — identically, not approximately, asserted by a unit test.
pub const HOLE_NACK_REFRESH_BAND: u32 = (HOLE_NACK_REFRESH_MAX.as_micros()
    / HOLE_NACK_REFRESH_MIN.as_micros()) as u32;

/// Receiver hole-refresh cadence with the clamp band's FLOOR supplied
/// (paper §16.78) — the ONE law, with one of its two constants resolved from
/// a value instead of read from a literal:
///
/// ```text
///   refresh(srtt)  =  (2·srtt).clamp( floor , HOLE_NACK_REFRESH_BAND · floor )
/// ```
///
/// **Why the whole BAND scales and not the lower rail alone.** The rail that
/// bounds the cadence is whichever rail BINDS, and that differs by operating
/// point: at a 2 ms-RTT cell `2·srtt ≈ 4 ms` and the LOWER rail binds; at a
/// loaded 100 Mbit cell `2·srtt ≥ 100 ms` and the UPPER rail binds. Moving
/// the lower rail alone is INERT wherever the upper rail is the one doing the
/// bounding, which is most measured cells (§16.78.0's table). One parameter
/// scaling the band moves whichever rail binds, everywhere, with one formula.
///
/// **This is not a mode.** There is no branch on `floor`, no second law, and
/// no threshold that selects a code path — the cadence is continuous in
/// `floor` over the whole domain, and `floor = HOLE_NACK_REFRESH_MIN` IS the
/// shipped machine.
///
/// The no-clock fallback is the band's own ceiling, which at the shipped
/// floor is `HOLE_NACK_REFRESH_MAX` verbatim.
pub fn hole_nack_refresh_floored(srtt: Option<Duration>, floor: Duration) -> Duration {
    let ceil = floor * HOLE_NACK_REFRESH_BAND;
    srtt.map(|s| (s * 2).clamp(floor, ceil)).unwrap_or(ceil)
}

/// Receiver hole-refresh cadence: 2×SRTT clamped to
/// [`HOLE_NACK_REFRESH_MIN`, `HOLE_NACK_REFRESH_MAX`], falling back to the
/// MAX when no path clock exists yet. In reliable window mode this cadence
/// — not any sender timer — is what re-presents a stalled hole to the
/// sender, so it bounds every recovery channel's observable latency.
///
/// The SHIPPED law, and the `floor = HOLE_NACK_REFRESH_MIN` instance of
/// [`hole_nack_refresh_floored`].
pub fn hole_nack_refresh(srtt: Option<Duration>) -> Duration {
    hole_nack_refresh_floored(srtt, HOLE_NACK_REFRESH_MIN)
}

// ── The DERIVED recovery round (`RWM_DERIVED_SWEEP`, default OFF) ─────────
//
// Goal-gate "The Derived Recovery Clamp" (2026-08-12). Both recovery clocks
// above are `2·SRTT` CLAMPED to [25 ms, 100 ms], and both literals are
// undocumented: `TAIL_SWEEP_*` arrived at cb66b93 ("clamp [25,100]ms —
// block mode's P8 sweeper analog", no measurement), `HOLE_NACK_REFRESH_*`
// at 4c90153 with no mention at all. The only stated justification is the
// comment on `TAIL_SWEEP_MIN_US`: the clock "must sit above the ack arrival
// time (~1×SRTT + jitter) … and below the receiver's reorder hold (60 ms
// floor) plus the inner-TCP RTO (~200 ms)".
//
// Both halves of that sentence are re-derived here rather than asserted:
//
//   * THE FLOOR IS REDUNDANT GIVEN THE MULTIPLIER. Its job is
//     `2·srtt ≥ srtt + jitter`, which holds whenever `jitter ≤ srtt` — true
//     by the definition of jitter as a consecutive-DIFFERENCE statistic. The
//     only case it really covers is "no clock yet", and the engine ALREADY
//     has a derived law for exactly that: `patience_floor_us` (goal-gate
//     "Unlock The Default 2") = timer granularity + the path's own measured
//     jitter, with the legacy literal as the no-sample fallback. So the
//     floor here is not a new constant; it is the one already derived.
//
//   * THE CEILING'S TWO REFERENTS DO NOT HOLD ON THE MEASURED STACK. The
//     receiver's reorder hold is a property of the EVICT path; the reliable
//     window (ρ = 1) receiver never force-delivers past a hole
//     (`recv_window_reliable`, net/receiver.rs), so no hold bounds this
//     cadence there. And the "inner-TCP RTO" does not exist: goal-gate "The
//     Latency-Feedback Source" PROVED, name by name, that the L1 vehicle
//     `perf.rs::run_object` carries no inner stack at all. A ceiling whose
//     stated purpose is to stay under two absent quantities is a constant
//     with no derivation behind it, and it is removed rather than re-fitted.
//
// What remains is the shipped FORM with the clamp replaced by the derived
// floor and NO ceiling — one expression, continuous in its argument, with
// zero new constants (the `2` is the shipped multiplier, untouched):
//
//     round(srtt, jitter) = max(2·srtt, patience_floor_us(jitter, srtt))
//
// This is an ENV GATE (an A/B attribution arm), never a dial on the
// (δ, ρ, r) triangle: nothing here keys on δ, on ρ, or on a hint.

/// The DERIVED recovery round (µs): `2·SRTT` floored by the derived
/// patience floor, with NO ceiling. See the block comment above.
///
/// COINCIDENCE PROPERTY (unit-tested): wherever `2·srtt` already lies inside
/// the legacy clamp AND the derived floor is below it, this returns exactly
/// `tail_sweep_timeout_us(srtt)` — the derived law is a strict
/// generalization that reproduces the literal law over the whole band the
/// literal law's own stated assumption ("2×SRTT is inside [25,100] ms")
/// holds on.
pub fn derived_recovery_round_us(srtt_us: u64, jitter_us: u64) -> u64 {
    srtt_us.saturating_mul(2).max(patience_floor_us(jitter_us, srtt_us))
}

// (The rival recovery-clock arms that used to follow here — the RACK round,
// `RWM_RACK_CLOCKS`/`RWM_RACK_REO_MULT`, paper §16.68, and the Cantelli
// quantile round with its α seat, `RWM_QUANTILE_CLOCKS`/`RWM_ALPHA_OVERRIDE`,
// §16.69 — were refuted and removed. The shipped clamp and
// `RWM_DERIVED_SWEEP` above are the two recovery rounds the engine computes.)
