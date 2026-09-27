//! Recovery-plane laws: derived patience and stall definition, multipath
//! recovery suppression, the laws extracted from the sender loop, and the
//! derived recovery round. Re-exported from `net`.

use super::*;

// ── Derived patience + derived stall definition ───────────────────────────
//
// Two fixed literals — the recovery plane's patience and the gauge that
// measures its stalls — re-expressed as laws over measured inputs. Both
// reproduce their literal exactly at the operating point where the literal's
// own assumption holds. The patience floor survives as the floor of the
// no-ceiling derived round (`derived_recovery_round_us`,
// `RWM_DERIVED_SWEEP`), and the stall threshold as the `RWM_SIDLE_DERIVED`
// gauge.
//
// Neither is a dial: neither selects a law, a code path or a constructor
// argument on (δ, ρ, r), and nothing keys on a threshold in the triangle.

/// The engine's timer granularity (µs) — RFC 9002 §6.1.2 kGranularity,
/// derived rather than borrowed.
///
/// RFC 9002 calls kGranularity system-dependent and recommends 1 ms. Here
/// the finest interval at which any recovery clock can be evaluated is the
/// sender loop's wake period: both timer arms of the send `select!` sleep
/// exactly 1 ms (the pacing refill and the backpressure poll), so a threshold
/// below that cannot be observed. The two coincide at 1 ms.
pub const TIMER_GRANULARITY_US: u64 = 1_000;

/// The sender loop's wake period (µs) as the emission-gap gauge's
/// observation granularity. Same 1 ms, named separately because it plays a
/// different role: `TIMER_GRANULARITY_US` floors a recovery clock,
/// `LOOP_WAKE_US` bounds what the gauge can resolve.
pub(crate) const LOOP_WAKE_US: u64 = 1_000;

/// The derived recovery-patience floor (µs): timer granularity + the path's
/// own measured RTT jitter.
///
/// This replaces `NACK_RETX_COOLDOWN_FLOOR_US`'s 10 ms (10× RFC 9002's
/// kGranularity) where that literal is behavioural: the kGranularity analog
/// inside `mp_time_threshold_split`, and the per-seq retransmit cooldown. At
/// RTprop ≈ 8–10 ms the literal sits at or above the 9/8·srtt term it was
/// meant to floor, so patience was a constant rather than a property of the
/// path.
///
/// Both terms are measured in-tree: `TIMER_GRANULARITY_US` above, and
/// `jitter_us` = the path's consecutive-difference RTT jitter
/// (`PathState::rtt_jitter_us()`, Copa's RFC 3550-style EWMA widened by its
/// window-level twin as the Copa backoff threshold does, with the loss
/// estimator's interarrival jitter as the pre-Copa-sample fallback). The
/// jitter term is clamped at one srtt so a pathological estimate cannot make
/// patience unbounded.
///
/// With no clock at all (`srtt_us == 0`) the fixed floor is kept — an
/// information-availability fallback, not a mode.
///
/// RFC 9002's kTimeThreshold (9/8, its empirical recommendation; RACK uses
/// 5/4) and kPacketThreshold (3) are cited, not derived. Only the floor is
/// derived.
pub fn patience_floor_us(jitter_us: u64, srtt_us: u64) -> u64 {
    if srtt_us == 0 {
        return NACK_RETX_COOLDOWN_FLOOR_US;
    }
    TIMER_GRANULARITY_US.saturating_add(jitter_us.min(srtt_us))
}

/// The derived stall threshold (µs) for the emission-gap gauges — the
/// definition `sidle`/`widle` count against.
///
/// The fixed `SIDLE_GAP_MIN_US` / `WIDLE_GAP_MIN_US` = 3 ms is 3 ×
/// `LOOP_WAKE_US`, "three times the nominal inter-emission interval" with the
/// loop wake standing in for that interval. That holds only while emission
/// events are at least as frequent as loop wakes. Emission batching
/// (`RWM_EMIT_BATCH`) makes them rarer — one counter change covers a whole
/// batch — so a fixed 3 ms would count ordinary pacing intervals as stalls.
///
/// The law keeps the form and replaces the assumed interval with the
/// measured one (`evt_us`, the mean inter-event interval over the previous
/// diagnostic window):
///
/// ```text
///   3 · max(evt_us, LOOP_WAKE_US)   clamped to [3 ms, HOLE_NACK_REFRESH_MIN]
/// ```
///
/// No new constant: the multiplier 3 is `SIDLE_GAP_MIN_US / LOOP_WAKE_US`;
/// the floor is the fixed constant; the ceiling is the hole-refresh cadence
/// (a wire gap longer than the interval at which the receiver re-advertises a
/// stalled hole is a stall at any operating point, and the ceiling keeps the
/// gauge from going blind at very slow cells).
///
/// Coincidence property (unit-tested): whenever `evt_us ≤ LOOP_WAKE_US` the
/// law returns exactly 3 000 µs. The derived stall total can never exceed the
/// fixed-threshold one.
pub(crate) fn stall_threshold_us(evt_us: u64) -> u64 {
    const STALL_GAP_MIN_US: u64 = 3_000;
    let nominal = evt_us.max(LOOP_WAKE_US);
    nominal
        .saturating_mul(STALL_GAP_MIN_US / LOOP_WAKE_US)
        .clamp(STALL_GAP_MIN_US, HOLE_NACK_REFRESH_MIN.as_micros() as u64)
}

// ── Multipath recovery suppression (`RWM_RECOV_MP`, default ON) ────────────
//
// Under dual-path striping, global recovery clocks inflate the retransmit
// and repair share far above the single-path run of the same config. The
// root defect: recovery clocks are global where multipath demands they be
// per-path (ADR-0059, paper §7.1).
//
// The hole law. A SACK gap is evidence of loss on a single path (FIFO within
// the path); across paths a seq gap is normal — the scheduler created it
// (striping + inter-path delay skew). The age gate (age ≥ max-path-SRTT/2
// since the original send) fires while the symbol's own flight is still in
// the air, and after a retransmit the clock is never reset to the new
// flight, so a scheduler-created gap re-fires every cooldown while copies
// are still flying. The law here is RFC 9002 §6.1.2 time-threshold loss
// detection generalized per path (the packet-threshold channel is not used
// across paths — cross-path seq gaps are the RFC 4737 reordering caveat;
// multipath QUIC solves the same problem with per-path packet-number
// spaces): a reported gap seq is a candidate hole only once its live flight
// (the last (re)send) is older than kTimeThreshold = 9/8 of its own path's
// smoothed RTT, with the per-seq cooldown floor as the kGranularity analog.
// Suppression-only: the receiver's hole-refresh keeps re-advertising, so a
// real hole fires the moment its flight clock expires. N = 1 live path keeps
// the single-path gates bit-exactly (single-path gaps are FIFO-real).
//
// The loss serials stay global. `batch_seq` is a global counter, while the
// receiver's per-path `PathBatchTracker` estimates expected symbols from
// batch_seq gaps, so under striping every path switch reads the other path's
// run as loss. Per-path serial namespaces were refuted at runtime and
// removed. Under `RWM_LOSS_SENT_TRUTH` (default OFF) the sender instead feeds
// its estimator `1 − Δcum_received / Δsymbols_sent`, both operands per-path
// measurements it already holds (`PathStats::symbols_sent`), with no wire
// change; the law and its named residual are on
// `PathState::sender_truth_loss_delta`. `cum_expected` is the sum of the same
// gap estimate, so differencing the merged-ack counters cannot remove the
// contamination. It stays off because the SRTT/loss-scaled recovery cadences
// were tuned against the contaminated values; re-deriving them is the open
// follow-up.
//
// Sub-gate for trace attribution: `RWM_RECOV_MP_LAW` (default ON under the
// umbrella) gates the hole law.

/// RFC 9002 §6.1.2 time threshold for the flight path: kTimeThreshold (9/8)
/// × max of the two smoothed RTT clocks available for the path (Copa EWMA
/// srtt and the estimator's EWMA app-echo RTT — the analog of
/// `max(smoothed_rtt, latest_rtt)`), floored at `floor_us`, the kGranularity
/// analog (the engine passes `NACK_RETX_COOLDOWN_FLOOR_US`). kTimeThreshold
/// is RFC 9002's empirical recommendation (RACK uses 5/4); only the floor is
/// derived.
///
/// Returns the threshold and whether the floor term won (the `pf=` mechanism
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
/// path: the fast loss channel. A seq's original flight on path j is declared
/// lost as soon as ≥3 later path-j symbols are known delivered — same-path
/// FIFO evidence (3 absorbs rare in-path reordering, per the RFC).
/// Scheduler-created cross-path gaps can never trigger it: their same-path
/// successors are exactly as un-arrived as they are. This keeps real-loss
/// recovery latency at about one skew, not a full RTT, under the
/// time-threshold suppression. Applies to original flights only — a
/// retransmit's wire order is not its seq order, so retransmits are governed
/// by the time threshold alone.
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
/// * `n_live_paths <= 1`: the law is inert — single-path gaps are FIFO-real
///   and the single-path gates own the decision.
/// * Unknown flight (no send record): never suppress a seq we cannot clock —
///   the reliability backstop.
/// * Otherwise: a hole only once the live flight is at least `threshold_us`
///   old — a gap on path A while the seq's flight is still inside path B's
///   expected-arrival clock is a gap the scheduler created, not a hole.
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

// ── Laws extracted from the sender loop ────────────────────────────────────
//
// Each function below is an expression from the sender's gap-report handler,
// its tail-sweep arm, or the receiver's hole-refresh arm, pulled out so the
// recovery plane can be driven without a transport (`tests/recovery_bench.rs`)
// and unit-tested.

/// RFC 9002 §6.1.2 time-threshold ripeness for one flight, path-count
/// agnostic: the live flight (last (re)send) must be at least `threshold_us`
/// old. An unknown flight is ripe (never suppress a seq we cannot clock).
/// This is the body `mp_hole_ripe` applies past its N ≤ 1 bypass, and the
/// `RWM_RECOV_SP` arm's test.
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

/// The age gate (the channel used when neither `RWM_RECOV_MP` nor
/// `RWM_RECOV_SP` is armed): a gap seq whose original send is younger than
/// half the pooled smoothed clock is late, not lost. Note the asymmetry —
/// this clock is `srtt/2` where the two RFC channels use `9/8·srtt`, and the
/// `srtt` fed to it is the pooled app-echo RTT (see
/// `pooled_recovery_srtt_us`).
pub fn legacy_age_ripe(now_us: u64, send_time_us: u64, srtt_us: u64) -> bool {
    now_us.saturating_sub(send_time_us) >= srtt_us / 2
}

/// The pooled recovery clock (µs): the max smoothed RTT over the live paths,
/// falling back to the cooldown floor when no path has a sample yet.
///
/// The samples are the estimator's app-echo RTT, which includes store dwell
/// (`QuicTransport::wire_rtt` is the dwell-free twin, ADR-0062) — so the age
/// gate, the per-seq cooldown and the tail sweep all inherit the dwell
/// through this one reduction.
pub fn pooled_recovery_srtt_us(path_rtt_us: &[u64]) -> u64 {
    path_rtt_us.iter().copied().max().unwrap_or(NACK_RETX_COOLDOWN_FLOOR_US)
}

/// The path set the sender's recovery clocks pool over: the tail-sweep
/// clock, the per-seq retransmit cooldown's pooled SRTT/jitter, and the
/// repair margin's loss rate. Live paths, as the receiver's recovery timer
/// uses (`receiver.rs`, `live_paths()`): the saturation-filtered
/// `active_paths()` (`available() > 0`) is empty exactly when every path is
/// cwnd-full, which would collapse the clocks to the 10 ms floor / zero margin.
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

/// Tail-sweep timeout (µs): 2×SRTT clamped to
/// [`TAIL_SWEEP_MIN_US`, `TAIL_SWEEP_MAX_US`]. The last symbols of a burst
/// have no successors, so the receiver can never SACK a gap behind them —
/// this is the sender's own stall detector.
pub fn tail_sweep_timeout_us(srtt_us: u64) -> u64 {
    (srtt_us.saturating_mul(2)).clamp(TAIL_SWEEP_MIN_US, TAIL_SWEEP_MAX_US)
}

/// The shipped clamp's aspect ratio,
/// `HOLE_NACK_REFRESH_MAX / HOLE_NACK_REFRESH_MIN` = 100 ms / 25 ms = 4
/// (paper §7.4). Computed from the two literals rather than written down, so
/// [`hole_nack_refresh_floored`] at `floor = HOLE_NACK_REFRESH_MIN`
/// reproduces the shipped law for every input (unit-tested).
pub const HOLE_NACK_REFRESH_BAND: u32 = (HOLE_NACK_REFRESH_MAX.as_micros()
    / HOLE_NACK_REFRESH_MIN.as_micros()) as u32;

/// Receiver hole-refresh cadence with the clamp band's floor supplied — the
/// one law, with one of its two constants resolved from a value:
///
/// ```text
///   refresh(srtt)  =  (2·srtt).clamp( floor , HOLE_NACK_REFRESH_BAND · floor )
/// ```
///
/// The whole band scales, not the lower rail alone, because the binding rail
/// differs by operating point: at a 2 ms-RTT cell `2·srtt ≈ 4 ms` and the
/// lower rail binds; at a loaded 100 Mbit cell `2·srtt ≥ 100 ms` and the
/// upper rail binds. Moving the lower rail alone is inert wherever the upper
/// rail binds; one parameter scaling the band moves whichever rail binds.
///
/// Not a mode: no branch on `floor`, continuous in `floor` over the whole
/// domain, and `floor = HOLE_NACK_REFRESH_MIN` is the shipped machine. The
/// no-clock fallback is the band's ceiling, which at the shipped floor is
/// `HOLE_NACK_REFRESH_MAX`.
pub fn hole_nack_refresh_floored(srtt: Option<Duration>, floor: Duration) -> Duration {
    let ceil = floor * HOLE_NACK_REFRESH_BAND;
    srtt.map(|s| (s * 2).clamp(floor, ceil)).unwrap_or(ceil)
}

/// Receiver hole-refresh cadence: 2×SRTT clamped to
/// [`HOLE_NACK_REFRESH_MIN`, `HOLE_NACK_REFRESH_MAX`], falling back to the
/// max when no path clock exists yet. In reliable window mode this cadence —
/// not any sender timer — re-presents a stalled hole to the sender, so it
/// bounds every recovery channel's observable latency.
///
/// The shipped law: the `floor = HOLE_NACK_REFRESH_MIN` instance of
/// [`hole_nack_refresh_floored`].
pub fn hole_nack_refresh(srtt: Option<Duration>) -> Duration {
    hole_nack_refresh_floored(srtt, HOLE_NACK_REFRESH_MIN)
}

// ── The derived recovery round (`RWM_DERIVED_SWEEP`, default OFF) ─────────
//
// Both recovery clocks above are `2·SRTT` clamped to [25 ms, 100 ms], and
// neither literal has a derivation. The stated justification (on
// `TAIL_SWEEP_MIN_US`) is that the clock must sit above the ack arrival time
// (~1×SRTT + jitter) and below the receiver's reorder hold plus an inner-TCP
// RTO. Both halves are re-derived here:
//
//   * The floor is redundant given the multiplier. Its job is
//     `2·srtt ≥ srtt + jitter`, which holds whenever `jitter ≤ srtt` — true
//     for jitter as a consecutive-difference statistic. The only case it
//     covers is "no clock yet", which `patience_floor_us` (timer granularity
//     + the path's measured jitter) already handles.
//
//   * The ceiling's two referents do not hold. The reorder hold belongs to
//     the EVICT path; the reliable window (ρ = 1) receiver never
//     force-delivers past a hole (`recv_window_reliable`, net/receiver.rs).
//     And the L1 vehicle `perf.rs::run_object` carries no inner TCP stack.
//     A ceiling with no referent is removed rather than re-fitted.
//
// What remains is the shipped form with the derived floor and no ceiling —
// continuous in its argument, zero new constants (the `2` is the shipped
// multiplier):
//
//     round(srtt, jitter) = max(2·srtt, patience_floor_us(jitter, srtt))
//
// An env A/B arm, never a dial: nothing here keys on δ, ρ or a hint.

/// The derived recovery round (µs): `2·SRTT` floored by the derived patience
/// floor, with no ceiling. See the block comment above.
///
/// Coincidence property (unit-tested): wherever `2·srtt` already lies inside
/// the clamp and the derived floor is below it, this returns exactly
/// `tail_sweep_timeout_us(srtt)`.
pub fn derived_recovery_round_us(srtt_us: u64, jitter_us: u64) -> u64 {
    srtt_us.saturating_mul(2).max(patience_floor_us(jitter_us, srtt_us))
}
