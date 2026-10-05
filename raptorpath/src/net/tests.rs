use super::*;

// ── The hold-down gauge (paper §7.4) — disarmed is inert, armed suppresses ──

/// Disarmed is inert, asserted at the gate itself. With `RWM_HOLDDOWN_Q`
/// absent, `should_hold` is `false` on every call at every age, so every
/// fire reaches `record_fire_cause`. No suppression, no `T`, no law.
///
/// The estimator nevertheless observes: the control reports
/// `obs_p50/p90/p99` over its own window while commanding nothing, so the
/// unforced outstanding-time distribution is measured on the arm that
/// defines it.
#[test]
fn disarmed_the_holddown_gate_is_inert_at_every_age() {
    let mut g = HoldDownGauge::new("sender", None);
    assert!(!g.armed(), "an absent level must not arm");
    for seq in 0..64u64 {
        g.on_reported(seq, 0, 1_000_000, true, None);
        for age in [0u64, 1, 1_000, 1_000_000, u64::MAX / 2] {
            assert!(
                !g.should_hold(seq, 1_000_000 + age),
                "disarmed must never hold (seq={seq} age={age})"
            );
        }
    }
    // The estimator observes: 64 holes resolve, at 1..=64 ms each.
    for seq in 0..64u64 {
        g.on_report(&[(seq + 1, 1_000)], 1_000_000 + (seq + 1) * 1_000, &Default::default(), 0, &|_| 0);
    }
    let l = g.line(0);
    // The law is absent and the line says so.
    assert!(l.contains("q=unset"), "the disarmed line must say so: {l}");
    assert!(l.contains("n_req=-"), "no window law is in force: {l}");
    assert!(l.contains("t_us=-"), "and therefore no T: {l}");
    assert!(l.contains("sup=0"), "disarmed must suppress nothing: {l}");
    assert!(l.contains("law_n=0"), "disarmed runs no law: {l}");
    assert!(l.contains("hd_n=0"), "and holds nothing, so no realized delay: {l}");
    // The observation is live.
    assert!(l.contains("n_obs=1000"), "the control's declared window: {l}");
    assert_eq!(g.fed.get(&0).copied().unwrap_or(0), 64, "the control must OBSERVE");
    assert!(!l.contains("obs_p50_us=-"), "the control must report a distribution: {l}");
    assert_eq!(g.obs_q(0, 0.50), Some(32_000), "and it must be the right one");
    // Two-sided: the site counted every evaluation it saw.
    assert!(
        l.contains("evals=320"),
        "the disarmed arm still counts its evaluations: {l}"
    );
    assert!(g.t_us.is_empty(), "no path may ever carry a T on the control");
}


// ── The request law's arms (paper §7.6) — disarmed is inert, the seam is one `&&` ──

/// Disarmed, nothing in the request law exists, asserted at the seam
/// itself — the twin of [`disarmed_the_holddown_gate_is_inert_at_every_age`].
///
/// Three things are pinned, in the order they could break:
///
///   1. The seam. With the gate absent `request_law_armed` is `false`
///      at every configuration, so `recv_nack_tx` keeps its shipped
///      arming and the per-seq SACK->gap producer is untouched.
///   2. The vocabulary. With arm (B) absent `m = 1` at every `pi0`
///      and every resource bound — the copy, so arm (A) alone cannot
///      change the bytes as well as the timing.
///   3. The echo is two-sided. `[REQS] on=0` renders every counter at
///      zero and its fraction as `-`, never `0`, so "the arm never
///      reached the wire" is a reading and not an inference from a
///      missing line.
#[test]
fn disarmed_the_request_law_is_inert_at_every_configuration() {
    // 1. The seam. The gate is the last conjunct, so with it absent no
    //    combination of the other two can arm anything.
    for rel in [false, true] {
        for gen in [false, true] {
            assert!(
                !request_law_armed(rel, gen, false),
                "an absent gate armed the seam at ({rel},{gen})"
            );
        }
    }
    // And armed it is the plain reliable window and nothing else.
    assert!(request_law_armed(true, false, true));
    assert!(!request_law_armed(true, true, true), "generation has no per-seq layer");
    assert!(!request_law_armed(false, false, true), "the EVICT seat is out of scope");

    // 2. The vocabulary. `m = 1` at every input while (B) is absent.
    for p in [None, Some(0.0), Some(0.006), Some(0.5), Some(0.96), Some(1.0)] {
        for a in [0u64, 1, 2, 64, 100_000] {
            assert_eq!(
                crate::net::late::request_m(p, false, a),
                1,
                "disarmed (B) must give the COPY at pi0={p:?} a_star={a}"
            );
        }
    }
    // Armed, the same law reaches m = 1 by itself at single-path inputs
    // (pi0 = 0.0077 / 0.0054): the copy is the law's own limit, not a
    // special case.
    assert_eq!(crate::net::late::request_m(Some(0.0077), true, 4096), 1);
    assert_eq!(crate::net::late::request_m(Some(0.0054), true, 4096), 1);
    // ... and rises continuously with pi0 at the duals (0.9606 / 0.9233).
    assert_eq!(crate::net::late::request_m(Some(0.9606), true, 4096), 18);
    assert_eq!(crate::net::late::request_m(Some(0.9233), true, 4096), 9);
    // The clamp is a resource bound and it binds visibly.
    assert_eq!(crate::net::late::request_m(Some(0.9606), true, 4), 4);

    // 3. The two-sided echo.
    let ctl = reqs_report_line(false, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, FireCause::Other);
    assert!(ctl.starts_with("[REQS] on=0 "), "{ctl}");
    assert!(ctl.contains("served=0 copy=0 coded=0"), "{ctl}");
    assert!(
        ctl.contains("wa1_none_frac=-"),
        "an absent fraction is `-`, never 0: {ctl}"
    );
    assert!(ctl.contains("cause=other"), "{ctl}");
    // Armed and serving, the same line reads its counts and its WA1 split.
    let arm = reqs_report_line(true, 9, 40, 18, 30, 7, 7, 3, 2, 1, 5, FireCause::GapData);
    assert!(arm.starts_with("[REQS] on=1 reports=9 spans=40 m_max=18 "), "{arm}");
    assert!(arm.contains("served=37 copy=30 coded=7"), "{arm}");
    assert!(arm.contains("wa1_some=7 wa1_none=3 wa1_none_frac=0.3000"), "{arm}");
    assert!(arm.contains("stale=2 budget_bound=1 open_wants=5 cause=gap_data"), "{arm}");

    // The wire form is total in both directions, so an unknown cause byte
    // from a future peer reads as `other` rather than panicking.
    for c in [FireCause::Timer, FireCause::GapData, FireCause::GapRefresh, FireCause::Other] {
        assert_eq!(FireCause::from_u8(c.as_u8()), c, "{c:?}");
    }
    assert_eq!(FireCause::from_u8(200), FireCause::Other);
}
/// Armed, the gate suppresses exactly the holes younger than `T`, and
/// the accounting closes: `evals = sup + emit` at every path, always —
/// without which `[HOLD] sup=` is a number nobody can place.
#[test]
fn armed_the_holddown_gate_holds_below_t_and_the_accounting_closes() {
    const Q: f64 = 0.5; // N = 2K = 20, the derived floor arm
    let n = holddown_window_n(Q).expect("H500 resolves");
    assert_eq!(n, 20);
    let mut g = HoldDownGauge::new("sender", Some(Q));
    assert!(g.armed());

    // Fill the estimator on path 0 with 20 known resolutions: 1..=20 ms.
    // Every one retires without a repair having flown, so every one is an
    // original resolution and the window is exactly 1_000..=20_000 µs.
    let empty_shed = std::collections::BTreeSet::new();
    for i in 1..=20u64 {
        g.on_reported(i, 0, 0, true, None);
        g.on_report(&[(i + 1, i + 1)], i * 1_000, &empty_shed, 0, &|_| 0);
    }
    // `T` is the K-th largest of the freshest 20 = the 11th smallest.
    let t = g.t_us.get(&0).copied().expect("the law ran once the window filled");
    assert_eq!(t, 11_000, "T must be the K-th largest of the window: {t}");

    // A hole younger than `T` is held; one older is emitted. No threshold on
    // any dial enters — the only number is `T`.
    g.on_reported(100, 0, 0, true, None);
    assert!(g.should_hold(100, t - 1), "younger than T must be held");
    g.on_reported(101, 0, 0, true, None);
    assert!(!g.should_hold(101, t), "at T exactly, the hold-down releases");
    g.on_reported(102, 0, 0, true, None);
    assert!(!g.should_hold(102, t + 1), "older than T must be emitted");

    let c = g.ctr.get(&0).copied().expect("path 0 saw fires");
    assert_eq!(c[0], 3, "evals");
    assert_eq!(c[1], 3, "law_n — the window was full at every evaluation");
    assert_eq!(c[2], 1, "sup");
    assert_eq!(c[3], 2, "emit");
    assert_eq!(c[0], c[2] + c[3], "evals must equal sup + emit, always");
    let l = g.line(0);
    assert!(l.contains("q=0.500000") && l.contains("n_req=20") && l.contains("t_us=11000"), "{l}");
    assert!(l.contains("samp_n=20") && l.contains("fed=20"), "{l}");
}

/// A shed hole was abandoned, not resolved, and must not feed the
/// estimator. It is the one exclusion: feeding a shed hole would push `T`
/// upward exactly where the contract had already given up. A repaired hole
/// is fed — see `on_retired` for why "no repair flew" is a fixed point at
/// zero on this machine, and for the sign of the bias that buys.
#[test]
fn the_holddown_estimator_excludes_only_the_shed() {
    let mut g = HoldDownGauge::new("sender", Some(0.5));
    let mut shed: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    for i in 1..=30u64 {
        g.on_reported(i, 0, 0, true, None);
    }
    shed.insert(9);
    shed.insert(11);
    g.on_report(&[(31, 31)], 5_000, &shed, 0, &|_| 0);
    assert_eq!(
        g.fed.get(&0).copied().unwrap_or(0),
        28,
        "30 retired minus two shed - and NOTHING else is excluded"
    );
    // And the frontier is exclusive above: a hole the ack has not passed is
    // still outstanding and is not retired.
    let mut g2 = HoldDownGauge::new("sender", Some(0.5));
    for i in 1..=30u64 {
        g2.on_reported(i, 0, 0, true, None);
    }
    g2.on_report(&[(11, 11)], 5_000, &std::collections::BTreeSet::new(), 0, &|_| 0);
    assert_eq!(g2.fed.get(&0).copied().unwrap_or(0), 10, "seqs 1..=10 only");
    assert_eq!(g2.first.len(), 20, "11..=30 are still outstanding");
}

/// The estimator bootstraps from a cold start with no warm-up branch. Fed
/// only holes for which no repair had flown, the window would never fill:
/// with `T` unavailable the sender answers every report immediately, so
/// that set is empty and `T` is a fixed point at zero. One rule, applied
/// identically at every window occupancy, must reach an armed `T` from a
/// completely cold gauge.
#[test]
fn the_holddown_estimator_bootstraps_from_cold_with_no_warmup_branch() {
    let mut g = HoldDownGauge::new("sender", Some(0.5));
    let n = holddown_window_n(0.5).expect("H500");
    let empty = std::collections::BTreeSet::new();
    // Cold: no T, so the gate falls through and every fire is emitted.
    g.on_reported(1, 0, 0, true, None);
    assert!(!g.should_hold(1, 10_000_000), "cold, the gate must fall through");
    assert!(g.t_us.get(&0).is_none());
    // Feed exactly N retirements, all of which would have been repaired on
    // the shipped machine. The window fills and the law arms itself.
    for i in 2..=(n as u64 + 1) {
        g.on_reported(i, 0, 0, true, None);
        g.on_report(&[(i + 1, i + 1)], i * 1_000, &empty, 0, &|_| 0);
    }
    assert!(
        g.t_us.get(&0).is_some(),
        "the estimator must arm from a cold start on repaired holes alone"
    );
}

/// The head-of-line defect, bounded by a test. The estimator must never
/// take a sample whose length is another hole's timing:
///
/// 1. `on_retired` — the cumulative-frontier sweep — feeds nothing, ever,
///    at any ack, on any path. It is a prune.
/// 2. `on_report` resolves a hole per hole, even while an earlier hole
///    is still open and therefore still pinning the frontier far below it.
///
/// Without (2) the sample for a late hole is a max-statistic over the whole
/// outstanding set, and `T` inflates by orders of magnitude over the RTT.
#[test]
fn the_holddown_estimator_never_takes_a_head_of_line_gated_sample() {
    let empty = std::collections::BTreeSet::new();
    // (1) The frontier sweep feeds nothing.
    let mut g = HoldDownGauge::new("sender", Some(0.5));
    for i in 1..=50u64 {
        g.on_reported(i, 0, 0, true, None);
    }
    g.on_retired(50);
    assert_eq!(g.fed.get(&0).copied().unwrap_or(0), 0, "on_retired must PRUNE, never feed");
    assert!(g.t_us.get(&0).is_none(), "and therefore must never arm a T");
    assert!(g.first.is_empty(), "but it must still bound the map");

    // (2) A hole resolves while an earlier one is still open. seq 1 stays
    // missing for the whole test — the cumulative frontier can never pass
    // seq 2 — and seq 2..=21 must nevertheless be timed on their own.
    let mut g2 = HoldDownGauge::new("sender", Some(0.5));
    for i in 1..=21u64 {
        g2.on_reported(i, 0, 0, true, None);
    }
    for i in 2..=21u64 {
        // seq 1 is still reported missing; seq `i` is not.
        g2.on_report(&[(1, 1), (i + 1, 30)], (i - 1) * 1_000, &empty, 0, &|_| 0);
    }
    assert_eq!(
        g2.fed.get(&0).copied().unwrap_or(0),
        20,
        "seqs 2..=21 must resolve on their own timing with seq 1 still open"
    );
    assert!(g2.first.contains_key(&1), "the still-missing hole must stay outstanding");
    // Samples are 1_000..=20_000 us; the K-th largest of 20 is 11_000.
    assert_eq!(
        g2.t_us.get(&0).copied(),
        Some(11_000),
        "and T must be the order statistic of THOSE samples, not of the frontier's lag"
    );
}

/// The window is per path and the paths do not pool. Two paths with
/// different reordering distributions must command different `T`, or the
/// estimator is measuring a mixture nobody named.
#[test]
fn the_holddown_estimator_is_per_path_and_paths_do_not_pool() {
    let mut g = HoldDownGauge::new("sender", Some(0.5));
    let empty_shed = std::collections::BTreeSet::new();
    // Path 0: 1..=20 ms. Path 1: 100..=2000 ms, 100x slower.
    for i in 1..=20u64 {
        g.on_reported(i, 0, 0, true, None);
        g.on_report(&[(i + 1, i + 1)], i * 1_000, &empty_shed, 0, &|_| 0);
    }
    for i in 101..=120u64 {
        g.on_reported(i, 1, 0, true, None);
        g.on_report(&[(i + 1, i + 1)], (i - 100) * 100_000, &empty_shed, 0, &|_| 0);
    }
    assert_eq!(g.t_us.get(&0).copied(), Some(11_000));
    assert_eq!(g.t_us.get(&1).copied(), Some(1_100_000));
    assert_eq!(g.win.get(&0).map(|w| w.len()), Some(20));
    assert_eq!(g.win.get(&1).map(|w| w.len()), Some(20));
}

/// An unfilled window falls through to the shipped behaviour and says so
/// in `law_n`. Information availability, never a mode: the arm's row is
/// then read as partial rather than pooled with a full one.
#[test]
fn an_unfilled_holddown_window_falls_through_and_law_n_says_so() {
    let mut g = HoldDownGauge::new("sender", Some(0.5));
    let empty_shed = std::collections::BTreeSet::new();
    for i in 1..=19u64 {
        g.on_reported(i, 0, 0, true, None);
        g.on_report(&[(i + 1, i + 1)], i * 1_000, &empty_shed, 0, &|_| 0);
    }
    assert_eq!(g.win.get(&0).map(|w| w.len()), Some(19), "one short of N");
    assert!(g.t_us.get(&0).is_none(), "the law must not have run");
    g.on_reported(100, 0, 0, true, None);
    assert!(
        !g.should_hold(100, 0),
        "with no T the gate falls through to the shipped behaviour"
    );
    let c = g.ctr.get(&0).copied().expect("path 0 saw the fire");
    assert_eq!((c[0], c[1], c[2], c[3]), (1, 0, 0, 1), "evals=1 law_n=0 sup=0 emit=1");
    assert!(g.line(0).contains("t_us=-"), "an unavailable T renders `-`");
}
// The reorder buffer lives in `net/reorder.rs`.
use super::reorder::ReorderBuffer;

// ── δ-honest overload shedding (paper §5.6) ──

/// Invariant 1: shed only past-deadline and within the
/// ρ budget. Fresh data is never shed however large the budget; stale
/// data is never shed past the budget; a cold (0) deadline sheds
/// nothing.
#[test]
fn shed_only_past_deadline_and_within_rho_budget() {
    let d = 25_000u64; // D(δ) = 25 ms
    // Past deadline, budget open (1% of 1000 sources = 10 allowed).
    assert!(shed_allowed(30_000, d, 0, 1_000, 0.01));
    assert!(shed_allowed(30_000, d, 9, 1_000, 0.01));
    // Budget exactly spent: the 11th shed is refused (serialize).
    assert!(!shed_allowed(30_000, d, 10, 1_000, 0.01));
    // Fresh (within deadline): never shed, however large the budget.
    assert!(!shed_allowed(10_000, d, 0, 1_000, 1.0));
    assert!(!shed_allowed(d, d, 0, 1_000, 1.0), "age == D is not past it");
    // Cold start: no derived deadline or zero budget ⇒ nothing sheds.
    assert!(!shed_allowed(30_000, 0, 0, 1_000, 1.0));
    assert!(!shed_allowed(30_000, d, 0, 1_000, 0.0));
    assert!(!shed_allowed(30_000, d, 0, 0, 0.5), "no sources ⇒ no budget");
}

/// Invariant 2: the reliable-transfer contract (ρ = 1,
/// retain-until-acked) is never shed — the law is compiled out on the
/// reliable path by construction, and it never arms outside the
/// unified machine or against the explicit =0 opt-out.
#[test]
fn shed_never_arms_on_reliable_contract() {
    // The only armed combination: unified + EVICT + gate on.
    assert!(shed_armed(true, false, true));
    // Reliable (bulk/auto window_reliable) never sheds.
    assert!(!shed_armed(true, true, true));
    // Unified off never sheds.
    assert!(!shed_armed(false, false, true));
    // RWM_UNIFIED_SHED=0 = the serializing control arm.
    assert!(!shed_armed(true, false, false));
}

/// The shed deadline is the span law's D (paper §5.3): b·RTprop, capped
/// at the 2·RTprop deficit-round limit — no new constants.
#[test]
fn shed_deadline_is_the_span_law_d() {
    // Realtime b = ½: D = RTprop/2.
    assert_eq!(shed_deadline_us(0.5, 40_000), 20_000);
    // Auto b = 1: D = RTprop.
    assert_eq!(shed_deadline_us(1.0, 40_000), 40_000);
    // Bulk b = 2 caps at the 2·RTprop limit.
    assert_eq!(shed_deadline_us(2.0, 40_000), 80_000);
    assert_eq!(shed_deadline_us(4.0, 40_000), 80_000);
}

/// Receiver arm: the in-order hold is the δ dial (b·SRTT, b = ½ on the
/// realtime-only EVICT path) while the give-up budget is open, and
/// reverts to the 4×SRTT ∈ [60, 300] ms clamp when the law is off or the
/// budget is spent.
#[test]
fn shed_recv_hold_delta_dial_and_legacy_fallback() {
    let srtt = Duration::from_millis(80);
    assert_eq!(shed_recv_hold(srtt, true, true), Duration::from_millis(40));
    let legacy = (srtt * 4).clamp(REORDER_MIN_HOLD, REORDER_MAX_HOLD);
    assert_eq!(shed_recv_hold(srtt, true, false), legacy, "budget spent ⇒ serialize");
    assert_eq!(shed_recv_hold(srtt, false, true), legacy, "law off ⇒ legacy");
    // The clamps still bind in the fallback (60 ms floor / 300 ms cap).
    assert_eq!(
        shed_recv_hold(Duration::from_millis(10), false, false),
        Duration::from_millis(60)
    );
    assert_eq!(
        shed_recv_hold(Duration::from_millis(200), false, false),
        Duration::from_millis(300)
    );
}

/// `[SHEDH]` partitions every evaluation — "every clamp gets a
/// bind-fraction gauge", asserted as arithmetic. A gauge whose classes do
/// not sum to its own denominator is not a bind fraction.
///
/// The counters are process-global and every test in this module that
/// calls `shed_recv_hold` feeds them, so this reads the DELTAS it causes
/// itself rather than absolute values — the only form that survives a
/// parallel runner.
#[test]
fn shedh_partitions_every_evaluation() {
    use std::sync::atomic::Ordering::Relaxed;
    let snap = || {
        (
            SHEDH.evals.load(Relaxed),
            SHEDH.dial.load(Relaxed),
            SHEDH.legacy.load(Relaxed),
            SHEDH.at_floor.load(Relaxed),
            SHEDH.at_cap.load(Relaxed),
            SHEDH.interior.load(Relaxed),
        )
    };
    let a = snap();
    // One of each class, by construction: dial; floor-bound (4·10 = 40 ms
    // < 60); cap-bound (4·200 = 800 ms > 300); interior (4·20 = 80 ms).
    shed_recv_hold(Duration::from_millis(80), true, true);
    shed_recv_hold(Duration::from_millis(10), false, false);
    shed_recv_hold(Duration::from_millis(200), false, false);
    shed_recv_hold(Duration::from_millis(20), false, false);
    let b = snap();
    let d = |i: usize| match i {
        0 => b.0 - a.0,
        1 => b.1 - a.1,
        2 => b.2 - a.2,
        3 => b.3 - a.3,
        4 => b.4 - a.4,
        _ => b.5 - a.5,
    };
    assert_eq!(d(0), 4, "every call must be one evaluation");
    assert_eq!(d(1) + d(2), d(0), "dial + legacy must be evals");
    assert_eq!(d(3) + d(4) + d(5), d(2), "the three clamp classes must be legacy");
    assert_eq!((d(1), d(3), d(4), d(5)), (1, 1, 1, 1), "one of each class");
    // The line renders, carries the constants it is auditing, and ends on
    // a field a concurrent stderr writer can corrupt without losing a
    // datum (the `fa_class` convention — here the two clamp rails).
    let l = shedh_report_line();
    assert!(l.starts_with("[SHEDH] evals="), "{l}");
    assert!(l.contains("floor_ms=60") && l.contains("cap_ms=300"), "{l}");
    assert!(l.contains("mean_us="), "{l}");
}

/// Receiver give-up budget: the loss-class bound — holes given up may
/// never exceed ε̂_recv × frontier; a clean channel (ε̂ = 0) never opens
/// the budget (nothing to shed on a clean channel anyway).
#[test]
fn shed_recv_budget_is_loss_class() {
    assert!(shed_recv_budget_ok(0, 1_000, 0.05));
    assert!(shed_recv_budget_ok(49, 1_000, 0.05));
    assert!(!shed_recv_budget_ok(50, 1_000, 0.05));
    assert!(!shed_recv_budget_ok(0, 0, 0.05), "no frontier yet ⇒ closed");
    assert!(!shed_recv_budget_ok(0, 1_000, 0.0), "clean channel ⇒ closed");
}

/// Receiver-tail parallelization. With a bound of 6 a lossy bulk transfer
/// reports only the first 6 outstanding generations' deficits per round,
/// so holes are repaired frontier-first — one round-trip per ~6
/// generations (serial tail). Lifting `report_gens` to cover the whole
/// in-flight range reports every outstanding generation's deficit in one
/// report, so the sender repairs all holes in a single round-trip.
#[test]
fn receiver_tail_reports_all_deficits_in_one_round() {
    // 50 outstanding generations, each K=384, each 3 DoF short (rank 381).
    let mut gen_widths: BTreeMap<u64, u16> = BTreeMap::new();
    for g in 0..50u64 {
        gen_widths.insert(g * 384, 384);
    }
    let rank_of = |_anchor: u64, k: u64| k - 3; // deficit 3 in every gen

    // Bound 6: only the frontier-first 6 generations are reported —
    // the tail is serialized (the remaining 44 wait for future rounds).
    let d6 = collect_gen_deficits(&gen_widths, 6, rank_of);
    assert_eq!(d6.len(), 6, "legacy bound reports only 6 generations");

    // Parallel tail flush: all 50 holes reported in a single round.
    let all = collect_gen_deficits(&gen_widths, 256, rank_of);
    assert_eq!(all.len(), 50, "every outstanding generation reported at once");
    assert!(all.iter().all(|&(_, d)| d == 3));
    let total: u32 = all.iter().map(|(_, d)| d).sum();
    assert_eq!(total, 150, "the full residual deficit is requested in one round");

    // Fully-decoded generations (deficit 0) are omitted regardless of cap.
    let none = collect_gen_deficits(&gen_widths, 256, |_a, k| k);
    assert!(none.is_empty(), "decoded generations report no deficit");
}

/// Repair-coverage horizon: a hole covered by the in-flight proactive
/// repair within the horizon fires no reactive NACK; a hole still uncovered
/// when the horizon expires falls back to the NACK.
#[test]
fn horizon_withholds_nack_until_repair_window_then_falls_back() {
    use std::time::{Duration, Instant};
    let horizon = Duration::from_millis(5);
    let mut armed: BTreeMap<u64, Instant> = BTreeMap::new();
    let t0 = Instant::now();

    // A frontier generation just went deficient. First sight → armed and
    // withheld: no reactive NACK yet (give the proactive repair its horizon).
    let d = vec![(0u64, 3u32)];
    let ready = horizon_gate_deficits(&d, &mut armed, horizon, t0);
    assert!(ready.is_empty(), "a fresh hole is withheld, not NACKed immediately");
    assert_eq!(armed.len(), 1, "the fresh hole is armed");

    // The proactive repair decodes it within the horizon → it drops out of
    // the deficit set → disarmed, and no NACK ever fired (the proactive win).
    let none: Vec<(u64, u32)> = vec![];
    let ready = horizon_gate_deficits(&none, &mut armed, horizon, t0 + Duration::from_millis(2));
    assert!(ready.is_empty());
    assert!(armed.is_empty(), "decoded-within-horizon hole is disarmed with no NACK");

    // A hole that proactive repair does not cover: still deficient after the
    // horizon expires → the reactive NACK fires (the reliability fallback).
    let d2 = vec![(384u64, 2u32)];
    let ready = horizon_gate_deficits(&d2, &mut armed, horizon, t0);
    assert!(ready.is_empty(), "still withheld before the horizon");
    let ready = horizon_gate_deficits(&d2, &mut armed, horizon, t0 + Duration::from_millis(6));
    assert_eq!(ready, vec![(384, 2)], "horizon expired uncovered → reactive fallback fires");

    // horizon == 0 restores the immediate shipped path.
    let mut armed0: BTreeMap<u64, Instant> = BTreeMap::new();
    let ready = horizon_gate_deficits(&d, &mut armed0, Duration::ZERO, t0);
    assert_eq!(ready, d, "horizon 0 reports immediately (shipped path)");
}

#[test]
fn test_parse_cidr() {
    let (ip, prefix) = parse_cidr("10.99.0.1/24").unwrap();
    assert_eq!(ip, "10.99.0.1".parse::<IpAddr>().unwrap());
    assert_eq!(prefix, 24);
}

// ── The honest-anchor (DH) arm's paused-sender residual at c1 ──
//
// Two hypotheses for why the honest-input arm (`RWM_PLAIN_RS` +
// `RWM_HONEST_ANCHOR`, "DH") pauses more than the no-feed arm ("A"),
// adjudicated at component level:
//   1. the bench measures sender blocking on the scheduler lock directly
//      (two threads, production lock, production attribution seam);
//   2. `dh_store_cap_keeps_its_warm_cap_at_cwnd_saturation` bounds the
//      rival mechanism: the DH arm's honest-cap law fell out to the
//      128-symbol boot cap whenever `active_paths()` returns empty — the
//      `sf=` zero-tick cliff, removed by plan 2b (the Σ's set is now
//      `channel_paths` = `live_paths()`; the test now pins its absence);
//   3. `honest_anchor_floor_sits_at_true_bdp…` pins why the DH sender
//      hits that cliff harder than A: the honest send-interval anchor
//      floors cwnd at the true BDP class while the ack-interval feed
//      floors it at the burst-peak over-read, so the same outstanding
//      level saturates (`available() == 0`) only the honest arm.

/// Lock-blocking measurement (run explicitly, --release):
///
///   cargo test --release -p raptorpath --lib -- --ignored --nocapture c1_attribution_lock
///
/// Two OS threads share the production `Arc<crate::scheduler::SchedMutex>`
/// at c1-class rate (24 000 delivered seqs/s, RTprop 2 ms):
///   - SENDER thread, 1 ms ticks: the sender loop's per-iteration lock
///     work (per-seq `on_src_sent` + `charge_src`/`charge_in_flight` at
///     placement, then the backpressure poll: `expire_in_flight` +
///     in_flight/cwnd read — `run_block_sender`/`run_window_sender`'s
///     poll body), measuring every lock acquisition wait.
///   - ACK thread at the swept cadence: arm A = the no-feed arm
///     (`sched.ack` + RTT sample under one acquisition); arm DH = the
///     `RWM_PLAIN_RS`+`RWM_HONEST_ANCHOR` arm — the Ack-arm section
///     (release_in_flight + on_delivery_signal + RTT sample), drop,
///     then `newly_delivered` + the production `copa_attribute_newly`
///     seam under a second acquisition, exactly `handle_control_message`'s
///     shape.
/// A third config replays the c1 recovery shape (85 ms ack stall, then
/// the SACK catch-up burst) to bound the worst-case hold.
///
/// Verdict rule: a DH−A sender lock-wait share under 5 points of wall at
/// the realistic cadences refutes "sender blocks on lock acquisition" as
/// the mechanism (the numbers are printed either way, `[P3-LOCK]` lines).
#[test]
#[ignore = "measurement: run explicitly with --release --ignored --nocapture"]
fn c1_attribution_lock_blocking_bench() {
    use std::sync::atomic::{AtomicBool, AtomicU64 as AU64};
    use std::time::Instant;

    const RATE: u64 = 24_000; // c1 class, sym/s
    const TICK_US: u64 = 1_000; // sender iteration ≈ 1 ms
    const BURST: u64 = RATE * TICK_US / 1_000_000; // 24 seqs/tick
    const LAG: u64 = 72; // ≈3 ms of seqs between send and ack
    const RUN_S: f64 = 4.0;
    const WARM_S: f64 = 0.5;

    fn pct(sorted: &[u64], p: f64) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let i = ((sorted.len() as f64 - 1.0) * p) as usize;
        sorted[i]
    }

    // One config+arm run. Returns (sender wait share %, ack hold duty %,
    // sender wait [p50, p99, max] µs, ack hold [p50, p99, max] µs).
    #[allow(clippy::too_many_arguments)]
    fn run(
        dh: bool,
        ack_cad_us: u64,
        stall: bool,
        label: &str,
    ) -> (f64, f64, [f64; 3], [f64; 3]) {
        let scheduler = Arc::new(crate::scheduler::SchedMutex::new(Scheduler::new(Arc::new(
            WallClock,
        ))));
        {
            let mut s = scheduler.lock();
            s.add_path(0);
            let p = s.path_mut(0).unwrap();
            p.record_rtt_sample(Duration::from_millis(2));
            p.force_honest_anchor_for_test(); // the DH arm's O(1) deque
        }
        let feed = Arc::new(CopaFeed::new_sampling_only(true));
        let frontier = AU64::new(0); // seqs sent so far
        let stop = AtomicBool::new(false);

        let mut snd_waits: Vec<u64> = Vec::with_capacity(8192);
        let mut ack_waits: Vec<u64> = Vec::with_capacity(8192);
        let mut ack_holds: Vec<u64> = Vec::with_capacity(8192);
        let mut attributed: u64 = 0;

        std::thread::scope(|sc| {
            // ── SENDER thread ────────────────────────────────────────
            let snd = sc.spawn(|| {
                let mut waits = Vec::with_capacity(8192);
                let t0 = Instant::now();
                let mut next = t0;
                let mut seq: u64 = 0;
                while t0.elapsed().as_secs_f64() < RUN_S {
                    // production emit path: the send-record DashMap
                    // write rides outside the scheduler guard.
                    for s in seq..seq + BURST {
                        feed.on_sent(s, 0);
                    }
                    let tq = Instant::now();
                    let mut sched = scheduler.lock();
                    let wait = tq.elapsed().as_nanos() as u64;
                    if let Some(p) = sched.path_mut(0) {
                        for s in seq..seq + BURST {
                            p.on_src_sent(s, false);
                        }
                        p.charge_src(BURST as u32);
                        p.charge_in_flight(BURST as u32);
                    }
                    // the backpressure poll (run_block_sender's body)
                    let mut fl = 0u64;
                    let mut cw = 0u64;
                    for id in sched.live_paths() {
                        if let Some(p) = sched.path_mut(id) {
                            p.expire_in_flight();
                            fl += p.in_flight as u64;
                            cw += p.cwnd as u64;
                        }
                    }
                    let _ = fl >= cw.max(4);
                    drop(sched);
                    if t0.elapsed().as_secs_f64() > WARM_S {
                        waits.push(wait);
                    }
                    seq += BURST;
                    frontier.store(seq, Ordering::Release);
                    next += Duration::from_micros(TICK_US);
                    while Instant::now() < next {
                        std::hint::spin_loop();
                    }
                }
                stop.store(true, Ordering::Release);
                waits
            });
            // ── ACK thread ───────────────────────────────────────────
            let ack = sc.spawn(|| {
                let mut waits = Vec::with_capacity(8192);
                let mut holds = Vec::with_capacity(8192);
                let mut attr: u64 = 0;
                let t0 = Instant::now();
                let mut next = t0;
                let mut acked: u64 = 0;
                let mut last_stall = t0;
                while !stop.load(Ordering::Acquire) {
                    if stall && last_stall.elapsed().as_millis() >= 1_000 {
                        // c1 recovery shape: the ack stream stalls one
                        // sweep-class round, then catches up in one
                        // frontier jump (the biggest real batch).
                        std::thread::sleep(Duration::from_millis(85));
                        last_stall = Instant::now();
                    }
                    let target = frontier.load(Ordering::Acquire).saturating_sub(LAG);
                    if target > acked {
                        let d = (target - acked) as u32;
                        // Ack-arm acquisition (control_msg PART 1+2).
                        let tq = Instant::now();
                        let mut sched = scheduler.lock();
                        let w1 = tq.elapsed().as_nanos() as u64;
                        let th = Instant::now();
                        if dh {
                            if let Some(p) = sched.path_mut(0) {
                                p.release_in_flight(d);
                                p.on_delivery_signal(); // !owns_cc arm
                                p.record_rtt_sample(Duration::from_millis(2));
                            }
                        } else {
                            if let Some(p) = sched.path_mut(0) {
                                p.record_rtt_sample(Duration::from_millis(2));
                            }
                            sched.ack(0, d); // legacy no-feed arm
                        }
                        drop(sched);
                        let h1 = th.elapsed().as_nanos() as u64;
                        let (w2, h2) = if dh {
                            // The RWM_PLAIN_RS attribution: cursor diff
                            // (no scheduler lock), then the production
                            // seam under its own acquisition.
                            let newly = feed.newly_delivered(target, &[]);
                            attr += newly.len() as u64;
                            let tq = Instant::now();
                            let mut sched = scheduler.lock();
                            let w = tq.elapsed().as_nanos() as u64;
                            let th = Instant::now();
                            copa_attribute_newly(&feed, 0, now_us(), &newly, &mut sched);
                            drop(sched);
                            (w, th.elapsed().as_nanos() as u64)
                        } else {
                            (0, 0)
                        };
                        if t0.elapsed().as_secs_f64() > WARM_S {
                            waits.push(w1 + w2);
                            holds.push(h1 + h2);
                        }
                        acked = target;
                    }
                    next += Duration::from_micros(ack_cad_us);
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    }
                }
                (waits, holds, attr)
            });
            snd_waits = snd.join().unwrap();
            let (w, h, a) = ack.join().unwrap();
            ack_waits = w;
            ack_holds = h;
            attributed = a;
        });

        // Liveness (measurement-discipline rule 1): the mechanism under test ran.
        let sent = frontier.load(Ordering::Acquire);
        if dh {
            assert!(
                attributed as f64 >= 0.8 * (sent.saturating_sub(LAG)) as f64,
                "attribution must cover the acked stream: {attributed} of {sent}"
            );
            let sched = scheduler.lock();
            assert!(
                sched.path(0).unwrap().copa_bdp_anchor().is_some(),
                "the send-interval sampler must establish (samples ACCEPTED)"
            );
        }

        let wall_ns = (RUN_S - WARM_S) * 1e9;
        let mut sw = snd_waits.clone();
        sw.sort_unstable();
        let mut ah = ack_holds.clone();
        ah.sort_unstable();
        let snd_share = 100.0 * snd_waits.iter().sum::<u64>() as f64 / wall_ns;
        let hold_duty = 100.0 * ack_holds.iter().sum::<u64>() as f64 / wall_ns;
        let sw_p = [
            pct(&sw, 0.5) as f64 / 1e3,
            pct(&sw, 0.99) as f64 / 1e3,
            *sw.last().unwrap_or(&0) as f64 / 1e3,
        ];
        let ah_p = [
            pct(&ah, 0.5) as f64 / 1e3,
            pct(&ah, 0.99) as f64 / 1e3,
            *ah.last().unwrap_or(&0) as f64 / 1e3,
        ];
        println!(
            "[P3-LOCK] {label:<26} arm={} sender: wait-share {snd_share:.3}% \
             p50/p99/max {:.1}/{:.1}/{:.1} µs (n={}) | ack: hold-duty {hold_duty:.3}% \
             hold p50/p99/max {:.1}/{:.1}/{:.1} µs wait-sum {:.2} ms attr={attributed}",
            if dh { "DH" } else { "A " },
            sw_p[0],
            sw_p[1],
            sw_p[2],
            sw.len(),
            ah_p[0],
            ah_p[1],
            ah_p[2],
            ack_waits.iter().sum::<u64>() as f64 / 1e6,
        );
        (snd_share, hold_duty, sw_p, ah_p)
    }

    let mut deltas = Vec::new();
    for (cad, name) in [(1_000u64, "per-msg acks (1 ms)"), (5_000, "bunched acks (5 ms)")] {
        let (a_share, _, _, _) = run(false, cad, false, name);
        let (dh_share, dh_duty, _, dh_hold) = run(true, cad, false, name);
        println!(
            "[P3-LOCK] {name:<26} Δ(DH−A) sender wait-share = {:.3} points \
             (hypothesis needs ~13; ack-side lock duty {dh_duty:.3}%, worst hold {:.1} µs)",
            dh_share - a_share,
            dh_hold[2],
        );
        deltas.push(dh_share - a_share);
    }
    // Worst-case bound: the recovery-stall catch-up batch (reported,
    // not scored — c1's steady state has no such stall each tick).
    let _ = run(false, 1_000, true, "recovery catch-up (85 ms)");
    let _ = run(true, 1_000, true, "recovery catch-up (85 ms)");

    for (i, d) in deltas.iter().enumerate() {
        assert!(
            *d < 5.0,
            "config {i}: DH−A sender lock-wait share = {d:.3} points — the \
             named lock-blocking mechanism would need ~13; investigate before \
             concluding (one green run is not evidence; re-run per discipline)"
        );
    }
}

/// Mechanism side (always-on, deterministic): the DH arm's own store-cap
/// law chain — `RWM_PLAIN_RS=1 RWM_HONEST_CAP=1` — evaluated over the
/// sender's own pool inputs (`store_cap_pool_inputs`, the channel's
/// membership set) at the same warm path, unsaturated and then
/// cwnd-saturated.
///
/// The chain mirrored here is `run_window_sender`'s dyn-cap block (the
/// `honest_cap_on` branch and its fallbacks). Before plan 2b the Σ ranged
/// over `active_paths()`, which a cwnd-full path leaves: the honest law's
/// inputs vanished and the cap fell to the 128 boot value (the cliff). The
/// invariant now: the saturated cap EQUALS the warm cap.
#[test]
fn dh_store_cap_keeps_its_warm_cap_at_cwnd_saturation() {
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    // Warm the honest (send-interval) anchor at the c1 shape: 24 k
    // sym/s, RTprop 2 ms, deliveries lagging sends by ~3 ms.
    {
        let p = sched.path_mut(0).unwrap();
        p.record_rtt_sample(Duration::from_millis(2));
        let step = Duration::from_micros(41);
        for seq in 0..4000u64 {
            p.on_src_sent(seq, false);
            if seq >= 72 {
                p.on_src_delivered_seq(seq - 72);
            }
            clock.advance(step);
        }
        assert!(
            p.copa_bdp_anchor().is_some(),
            "honest anchor must be warm (samples accepted)"
        );
    }
    // The DH law chain over the sender's pool inputs, exactly as the
    // sender computes it (RWM_STORE_GAIN default 2.0; floor 64;
    // RELIABLE_STORE_MAX latch at N = 1; store_boot_cap 128 — gates.rs
    // defaults).
    let now = now_us();
    let mut ks: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let mut cap_over = |sched: &Scheduler| -> usize {
        let (bdp, slots) = store_cap_pool_inputs(sched, true);
        let terms = honest_cap_terms(&mut ks, &slots, now, 2.0);
        let hsum: f64 = terms.iter().flatten().sum();
        if hsum > 0.0 {
            (hsum.ceil() as usize).clamp(64, RELIABLE_STORE_MAX)
        } else if bdp > 0.0 {
            ((2.0 * bdp).ceil() as usize).clamp(64, RELIABLE_STORE_MAX)
        } else {
            128 // store_boot_cap fallback — the (former) cliff
        }
    };

    // Unsaturated: the honest law computes its warm cap (runway term ≈
    // rate·0.1 ⇒ the 1024 latch at c1 rates).
    let warm_cap = cap_over(&sched);
    assert_eq!(
        warm_cap, RELIABLE_STORE_MAX,
        "c1-class honest cap latches the store max"
    );

    // Saturated (the wire-bound sender state: in_flight ≥ cwnd): the
    // spare-capacity filter empties the placement set while the path is
    // alive and its anchor is warm.
    {
        let p = sched.path_mut(0).unwrap();
        let cw = p.cwnd;
        p.charge_in_flight(cw);
        assert_eq!(p.available(), 0);
    }
    assert!(sched.active_paths().is_empty(), "cwnd-saturated ⇒ active_paths() EMPTY");
    assert_eq!(sched.live_paths(), vec![0], "…while the path is fully live");
    assert_eq!(
        cap_over(&sched),
        warm_cap,
        "no cliff: the saturated path keeps its warm honest cap (was 128 on active_paths())"
    );
}

/// Coupling side (always-on, deterministic): why the DH sender falls off
/// the cliff harder than A. One delivery process, two feeds:
///   - honest (`RWM_PLAIN_RS` send-interval sampler): the windowed-max
///     anchor reads ≈ the true rate, so the cwnd anchor floor sits at
///     the true-BDP class;
///   - ack-interval (no-feed `sched.ack` sampler) under c1-class ack
///     bunching: the windowed-max latches the bunch peak, so the floor
///     sits far above.
/// `available() = cwnd − in_flight` with cwnd ≥ floor: an outstanding
/// level between the two floors can saturate only the honest arm. The
/// honest floor makes the saturated state the resting state of a
/// wire-bound sender; where the sender has intake headroom, in_flight
/// sits below even the honest floor and no cliff fires.
#[test]
fn honest_anchor_floor_sits_at_true_bdp_where_the_legacy_ack_feed_floors_high() {
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0); // honest feed
    sched.add_path(1); // legacy ack-interval feed
    for id in [0u32, 1] {
        sched
            .path_mut(id)
            .unwrap()
            .record_rtt_sample(Duration::from_millis(2));
    }
    // One true process: 24 k sym/s for 2 s. Path 0 sees it per
    // delivered seq (send-interval Δt); path 1 sees the same totals as
    // c1-class bunched acks: per 10 ms cycle, a straggler ack after
    // 8.9 ms then the 216-symbol bunch 1.1 ms later — Δdelivered/Δt
    // ≈ 196 k sym/s at the same 24 k carried rate.
    for cycle in 0..200u64 {
        // path 0: continuous per-seq attribution
        {
            let p = sched.path_mut(0).unwrap();
            let base = cycle * 240;
            for i in 0..240u64 {
                let seq = base + i;
                p.on_src_sent(seq, false);
                if seq >= 72 {
                    p.on_src_delivered_seq(seq - 72);
                }
                clock.advance(Duration::from_micros(41));
            }
        }
        // path 1: same 240 symbols, bunched (same wall interval)
        {
            clock.advance(Duration::from_micros(160)); // pad to 10 ms
            let p = sched.path_mut(1).unwrap();
            p.record_rtt_sample(Duration::from_millis(2));
            p.on_ack(24); // straggler re-arms last_delivered_time
            clock.advance(Duration::from_micros(1_100));
            let p = sched.path_mut(1).unwrap();
            p.on_ack(216); // the bunch: 216 / 1.1 ms ≈ 196 k sym/s
        }
    }
    let a_honest = sched.path(0).unwrap().copa_bdp_anchor().expect("warm");
    let a_legacy = sched.path(1).unwrap().copa_bdp_anchor().expect("warm");
    let true_bdp = 24_000.0 * 0.002; // rate × RTprop = 48 symbols
    assert!(
        a_honest < 3.0 * true_bdp,
        "honest anchor reads the true-BDP class: {a_honest:.0} vs {true_bdp:.0}"
    );
    assert!(
        a_legacy > 3.0 * a_honest,
        "legacy ack-interval anchor must floor high (the over-read): \
         {a_legacy:.0} vs honest {a_honest:.0}"
    );
    let f_honest = sched.path(0).unwrap().anchor_floor_for_test().expect("floor");
    let f_legacy = sched.path(1).unwrap().anchor_floor_for_test().expect("floor");
    assert!(
        f_legacy > 3 * f_honest,
        "the cwnd floors order the same way: honest {f_honest} vs legacy {f_legacy}"
    );
    // The consequence, as arithmetic on the real predicate: outstanding
    // between the floors saturates only the honest arm. cwnd ≥ floor
    // always (the floor only ratchets up), so the ack-interval arm cannot
    // read available() == 0 at this level; the honest arm at its
    // wire-bound lower bound (cwnd == floor) reads exactly 0.
    let mid = 2 * f_honest;
    assert!(mid < f_legacy);
    {
        let p = sched.path_mut(0).unwrap();
        p.cwnd = f_honest; // the floor IS the resting cwnd lower bound
        p.charge_in_flight(mid);
        assert_eq!(p.available(), 0, "honest arm: saturated at mid outstanding");
    }
    {
        let p = sched.path_mut(1).unwrap();
        p.cwnd = p.cwnd.max(f_legacy);
        p.charge_in_flight(mid);
        assert!(
            p.available() > 0,
            "legacy arm keeps spare capacity at the same outstanding"
        );
    }
}

#[test]
fn test_parse_cidr_32() {
    let (ip, prefix) = parse_cidr("192.168.1.1/32").unwrap();
    assert_eq!(ip, "192.168.1.1".parse::<IpAddr>().unwrap());
    assert_eq!(prefix, 32);
}

#[test]
fn test_parse_cidr_invalid() {
    assert!(parse_cidr("10.0.0.1").is_err());
    assert!(parse_cidr("not/valid").is_err());
}

#[test]
fn test_prefix_to_netmask() {
    let mask = prefix_to_netmask(24);
    assert_eq!(mask, "255.255.255.0".parse::<IpAddr>().unwrap());

    let mask = prefix_to_netmask(16);
    assert_eq!(mask, "255.255.0.0".parse::<IpAddr>().unwrap());

    let mask = prefix_to_netmask(32);
    assert_eq!(mask, "255.255.255.255".parse::<IpAddr>().unwrap());
}

#[test]
fn test_path_batch_tracker_no_loss() {
    let mut tracker = PathBatchTracker::new();
    let (expected, received) = tracker.record_batch(0, 10);
    assert_eq!(expected, 10); // first batch
    assert_eq!(received, 10);

    let (expected, received) = tracker.record_batch(1, 10);
    assert_eq!(expected, 10); // sequential, no gap
    assert_eq!(received, 10);
}

#[test]
fn test_path_batch_tracker_with_gap() {
    let mut tracker = PathBatchTracker::new();
    tracker.record_batch(0, 10);

    // Skip batch 1 (lost)
    let (expected, received) = tracker.record_batch(2, 10);
    assert_eq!(expected, 20); // gap of 2, estimates 2*10 expected
    assert_eq!(received, 10);
}

/// S10 / F1 — reorder is not loss. Arrivals 6, 8, 7, 9 with nothing lost:
/// the tracker must read expected == received. Through 949e06b it set
/// `last_seq` backwards on the late 7 and charged the 7→9 step as a gap
/// again: 2 phantom losses on 4 arrivals.
#[test]
fn s10_tracker_reorder_without_loss_reads_zero_loss() {
    let mut t = PathBatchTracker::new();
    for s in [6u64, 8, 7, 9] {
        t.record_batch(s, 1);
    }
    assert_eq!(
        (t.total_expected, t.total_received),
        (4, 4),
        "6,8,7,9 lost nothing: expected must equal received"
    );
}

/// S10 / F1 — a true loss plus reorders reads exactly the true loss. Seqs
/// 0..=29 with 12 dropped on the wire and three displaced arrivals
/// (d = 1, 2, 3): the cumulative loss is exactly 1.
#[test]
fn s10_tracker_true_loss_plus_reorder_reads_exactly_the_true_loss() {
    let mut order: Vec<u64> = (0..30).filter(|&s| s != 12).collect();
    // Displace 5 behind 6, 16 behind 18, 23 behind 26.
    let mv = |o: &mut Vec<u64>, s: u64, after: u64| {
        let i = o.iter().position(|&x| x == s).unwrap();
        o.remove(i);
        let j = o.iter().position(|&x| x == after).unwrap();
        o.insert(j + 1, s);
    };
    mv(&mut order, 5, 6);
    mv(&mut order, 16, 18);
    mv(&mut order, 23, 26);
    let mut t = PathBatchTracker::new();
    let mut late = Vec::new();
    for s in order {
        let (e, r) = t.record_batch(s, 1);
        if e == 0 {
            late.push((s, r));
        }
    }
    assert_eq!(t.total_received, 29);
    assert_eq!(
        t.total_expected - t.total_received,
        1,
        "exactly the one wire loss (seq 12), no reorder phantoms"
    );
    // A late arrival credits received without adding expected.
    assert_eq!(late, vec![(5, 1), (16, 1), (23, 1)]);
}

/// S10 / F1 — QUIC datagrams are never retransmitted and packet numbers are
/// deduplicated, so a duplicate `path_seq` cannot arrive. The tracker
/// asserts it (debug builds) instead of silently double-crediting.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "duplicate path_seq")]
fn s10_tracker_debug_asserts_no_duplicate_path_seq() {
    let mut t = PathBatchTracker::new();
    t.record_batch(3, 1);
    t.record_batch(4, 1);
    t.record_batch(3, 1);
}

// ----- CopaFeed attribution cursor -----

/// Frontier advance attributes each seq exactly once, in order (wire v9:
/// the frontier is `next_expected`, the delivered-prefix count).
#[test]
fn copa_feed_frontier_attributes_once() {
    let feed = CopaFeed::new();
    assert_eq!(feed.newly_delivered(3, &[]), vec![0, 1, 2]);
    // Duplicate/stale ack → empty diff, never a re-attribution.
    assert!(feed.newly_delivered(3, &[]).is_empty());
    assert!(feed.newly_delivered(2, &[]).is_empty());
    assert_eq!(feed.newly_delivered(5, &[]), vec![3, 4]);
}

/// Wire v9, seq 0: `next_expected = 0` attributes nothing (v8's
/// `received_up_to = 0` attributed seq 0 before it was delivered), and
/// `next_expected = 1` attributes exactly seq 0.
#[test]
fn copa_feed_attributes_seq_zero_only_once_delivered() {
    let feed = CopaFeed::new();
    assert!(feed.newly_delivered(0, &[]).is_empty(), "nothing delivered, nothing attributed");
    assert_eq!(feed.newly_delivered(1, &[]), vec![0], "seq 0 delivered alone");
    assert!(feed.newly_delivered(1, &[]).is_empty());
}

/// SACKed seqs above the frontier are attributed immediately and not
/// re-attributed when the frontier later passes them.
#[test]
fn copa_feed_sack_dedupes_against_frontier() {
    let feed = CopaFeed::new();
    // 0..=1 delivered (`next_expected = 2`), receiver also has 5..=6
    // (hole 2..=4).
    assert_eq!(feed.newly_delivered(2, &[(5, 6)]), vec![0, 1, 5, 6]);
    // Same SACK re-advertised → nothing new.
    assert!(feed.newly_delivered(2, &[(5, 6)]).is_empty());
    // Hole repaired: 0..=7 delivered — only the gap seqs (2..=4) and 7
    // are new; 5..=6 were consumed from the sacked set.
    assert_eq!(feed.newly_delivered(8, &[]), vec![2, 3, 4, 7]);
}

/// seq→path attribution: the seq is charged to the path it was (last)
/// sent on; unknown seqs fall back to the ack path. A cross-path
/// retransmit keeps the previous commitment as the flight-witness
/// fallback.
#[test]
fn copa_feed_seq_path_last_send_wins() {
    let feed = CopaFeed::new();
    feed.on_sent(10, 0);
    feed.on_sent(10, 1); // retransmit on the other path
    let commit = feed.seq_path.remove(&10).map(|(_, c)| c).unwrap();
    assert_eq!(commit.last.0, 1);
    assert_eq!(commit.prev.map(|(p, _)| p), Some(0));
    assert!(feed.seq_path.remove(&10).is_none());
}

/// The flight-time witness. An ack younger than the
/// retransmit path's RTprop proves the original flight delivered the
/// seq — the retransmit path's delivered counter must not advance. An
/// ack older than RTprop credits the retransmit path (a genuine
/// retransmit delivery). Unknown RTprop / single-path history keep
/// last-sent attribution.
#[test]
fn flight_witness_credits_original_path_for_spurious_retransmit() {
    // Original on fast (path 0) at t=0, retransmitted on slow (path 1,
    // RTprop 60 ms) at t=1_000_000.
    let commit = SendCommit {
        last: (1, 1_000_000),
        prev: Some((0, 0)),
    };
    let rtprop = |pid: u32| if pid == 1 { Some(60_000u64) } else { Some(8_000u64) };
    // Ack 5 ms after the retransmit: the slow flight cannot have
    // completed — the fast original delivered it.
    assert_eq!(resolve_flight_path(&commit, 1_005_000, rtprop), 0);
    // Ack 80 ms after the retransmit: the slow flight qualifies.
    assert_eq!(resolve_flight_path(&commit, 1_080_000, rtprop), 1);
    // Exactly RTprop old: qualifies (>=).
    assert_eq!(resolve_flight_path(&commit, 1_060_000, rtprop), 1);
    // Warm-up (no RTprop yet): last-sent attribution.
    assert_eq!(resolve_flight_path(&commit, 1_005_000, |_| None), 1);
    // Single-path history: always the last (= only) commitment.
    let single = SendCommit {
        last: (1, 1_000_000),
        prev: None,
    };
    assert_eq!(resolve_flight_path(&single, 1_000_001, rtprop), 1);
    // A→B→A bounce: prev is the previous distinct path (B), and a
    // young ack after the same-path resend credits B's older flight.
    let feed = CopaFeed::new();
    feed.on_sent(7, 0);
    feed.on_sent(7, 1);
    feed.on_sent(7, 0);
    let c = feed.seq_path.remove(&7).map(|(_, c)| c).unwrap();
    assert_eq!(c.last.0, 0);
    assert_eq!(c.prev.map(|(p, _)| p), Some(1));
}

// ----- The skew-aware hole law (paper §7.1) -----

/// The per-flight time threshold is the RFC 9002 §6.1.2 shape: 9/8 of
/// the larger smoothed clock, floored at the per-seq cooldown floor
/// (the kGranularity analog). No new constants.
#[test]
fn mp_time_threshold_is_nine_eighths_of_max_clock_with_floor() {
    const F: u64 = NACK_RETX_COOLDOWN_FLOOR_US;
    // 40 ms srtt, 32 ms ewma → 9/8 × 40 ms = 45 ms.
    assert_eq!(mp_time_threshold_split(40_000, 32_000, F).0, 45_000);
    // The larger clock wins regardless of which estimator it is.
    assert_eq!(mp_time_threshold_split(32_000, 40_000, F).0, 45_000);
    // Tiny clocks floor at NACK_RETX_COOLDOWN_FLOOR_US.
    assert_eq!(mp_time_threshold_split(1_000, 500, F).0, F);
    assert_eq!(mp_time_threshold_split(0, 0, F).0, F);
}

// ----- Derived patience -----

/// The gate-off contract: passing the 10 ms literal reproduces the
/// fixed-floor function exactly, and RFC 9002's kTimeThreshold (9/8)
/// and kPacketThreshold (3) are untouched by any of this.
#[test]
fn derived_patience_off_is_bit_identical_to_the_legacy_threshold() {
    const F: u64 = NACK_RETX_COOLDOWN_FLOOR_US;
    for srtt in [0u64, 1, 500, 1_000, 8_000, 8_888, 8_889, 9_000, 40_000, 250_000] {
        for ewma in [0u64, 700, 9_500, 40_000] {
            let legacy = (srtt.max(ewma).saturating_mul(9) / 8).max(F);
            assert_eq!(
                mp_time_threshold_split(srtt, ewma, F).0,
                legacy,
                "srtt={srtt} ewma={ewma}"
            );
        }
    }
    // The cited constants are still the cited constants.
    assert_eq!(MP_PACKET_THRESHOLD, 3);
    assert_eq!(mp_time_threshold_split(80_000, 0, 0).0, 90_000); // 9/8 exactly
}

/// The law: timer granularity + the path's own measured jitter,
/// clamped at one srtt, with the fixed floor kept when there is no
/// clock at all to derive from.
#[test]
fn patience_floor_is_granularity_plus_measured_jitter() {
    // No clock yet ⇒ nothing to derive ⇒ the fixed floor.
    assert_eq!(patience_floor_us(0, 0), NACK_RETX_COOLDOWN_FLOOR_US);
    assert_eq!(patience_floor_us(5_000, 0), NACK_RETX_COOLDOWN_FLOOR_US);
    // Zero measured jitter ⇒ pure timer granularity (RFC 9002's
    // recommended kGranularity, and this engine's own 1 ms loop wake).
    assert_eq!(patience_floor_us(0, 9_000), TIMER_GRANULARITY_US);
    // The jitter term is additive and measured — it scales with the
    // link.
    assert_eq!(patience_floor_us(300, 9_000), 1_300);
    assert_eq!(patience_floor_us(2_500, 9_000), 3_500);
    // …and is clamped at one srtt so a pathological estimate cannot
    // make patience unbounded.
    assert_eq!(patience_floor_us(10_000_000, 9_000), 10_000);
    // Monotone non-decreasing in jitter, at fixed srtt.
    let mut prev = 0;
    for j in (0..20_000).step_by(250) {
        let f = patience_floor_us(j, 40_000);
        assert!(f >= prev, "jitter={j}");
        prev = f;
    }
}

/// The tail-sweep SRTT fallback is inert with respect to this constant:
/// every fallback value ≤ 12.5 ms — the fixed 10 ms and any derived floor
/// alike — yields exactly `TAIL_SWEEP_MIN_US` after the
/// `(srtt·2).clamp(25 ms, 100 ms)` the site applies.
#[test]
fn tail_sweep_srtt_fallback_is_inert_to_the_patience_floor() {
    let sweep = tail_sweep_timeout_us;
    assert_eq!(sweep(NACK_RETX_COOLDOWN_FLOOR_US), TAIL_SWEEP_MIN_US);
    assert_eq!(sweep(TIMER_GRANULARITY_US), TAIL_SWEEP_MIN_US);
    for f in [0u64, 1, 1_000, 1_400, 5_000, 10_000, 12_500] {
        assert_eq!(sweep(f), TAIL_SWEEP_MIN_US, "fallback {f} must be inert");
    }
    // The first value that is not inert, recorded so the bound is exact.
    assert!(sweep(12_501) > TAIL_SWEEP_MIN_US);
}

// ----- The extracted recovery laws.
// Each test below re-evaluates the inline form of an extracted law and
// asserts the extracted function is identical to it over a dense grid.

#[test]
fn extracted_laws_are_identical_to_the_inline_expressions_they_replaced() {
    let times = [0u64, 1, 999, 1_000, 5_000, 9_999, 10_000, 11_250, 79_000, 177_750, 1 << 40];
    let clocks = [0u64, 1_000, 8_000, 10_000, 12_500, 40_000, 158_000, 200_000];

    for &now in &times {
        for &t in &times {
            for &thr in &clocks {
                // §6.1.2 ripeness (both the RECOV_SP arm and mp_hole_ripe's body).
                assert_eq!(
                    time_threshold_ripe(now, Some(t), thr),
                    now.saturating_sub(t) >= thr,
                    "time_threshold_ripe({now},{t},{thr})"
                );
                // Per-seq cooldown (the `< cooldown ⇒ suppress` inline test).
                assert_eq!(
                    cooldown_elapsed(now, t, thr),
                    !(now.saturating_sub(t) < thr),
                    "cooldown_elapsed({now},{t},{thr})"
                );
            }
            for &srtt in &clocks {
                // Age gate: `now - send < srtt/2 ⇒ suppress`.
                assert_eq!(
                    legacy_age_ripe(now, t, srtt),
                    !(now.saturating_sub(t) < srtt / 2),
                    "legacy_age_ripe({now},{t},{srtt})"
                );
            }
        }
    }
    // An unknown flight is ripe — the reliability backstop.
    assert!(time_threshold_ripe(0, None, u64::MAX));
    // mp_hole_ripe still bypasses at N ≤ 1 and delegates above it.
    for n in 0..4usize {
        for &thr in &clocks {
            let expect = n <= 1 || time_threshold_ripe(50_000, Some(0), thr);
            assert_eq!(mp_hole_ripe(n, 50_000, Some(0), thr), expect, "n={n} thr={thr}");
        }
    }

    // Pooled clock reduction + cooldown + floor.
    assert_eq!(pooled_recovery_srtt_us(&[]), NACK_RETX_COOLDOWN_FLOOR_US);
    assert_eq!(pooled_recovery_srtt_us(&[8_000, 158_000, 12_000]), 158_000);
    for &s in &clocks {
        for &f in &clocks {
            assert_eq!(retx_cooldown_us(s, f), s.max(f));
        }
    }

    // Receiver hole-refresh cadence.
    assert_eq!(hole_nack_refresh(None), HOLE_NACK_REFRESH_MAX);
    for ms in [0u64, 5, 10, 12, 13, 20, 50, 51, 200] {
        let s = Duration::from_millis(ms);
        assert_eq!(
            hole_nack_refresh(Some(s)),
            (s * 2).clamp(HOLE_NACK_REFRESH_MIN, HOLE_NACK_REFRESH_MAX)
        );
    }
}

/// At a c7-like operating point (RTprop ≈ 10 ms) the recovery clock's
/// argument — not its constants — sets patience. Fed the
/// store-dwell-inclusive app-echo RTT (~158 ms) every channel's patience
/// is ~×16–18 RTprop; fed the dwell-free wire clock it is ~×1.1–1.4.
#[test]
fn patience_is_set_by_the_clock_argument_not_by_the_constants() {
    const RTPROP_US: u64 = 10_000;
    const APP_ECHO_US: u64 = 158_000; // measured, c7
    const WIRE_US: u64 = 14_000; // rtp 10 + measured wireQ 4, c7 p0

    let f = NACK_RETX_COOLDOWN_FLOOR_US;
    // §6.1.2 time threshold (the RECOV_MP / RECOV_SP channel).
    assert_eq!(mp_time_threshold_split(0, APP_ECHO_US, f).0, 177_750);
    assert_eq!(mp_time_threshold_split(0, WIRE_US, f).0, 15_750);
    // Age gate (the shipped default channel) = srtt/2.
    assert_eq!(APP_ECHO_US / 2, 79_000);
    assert_eq!(WIRE_US / 2, 7_000);
    // Per-seq cooldown.
    assert_eq!(retx_cooldown_us(APP_ECHO_US, f), APP_ECHO_US);
    assert_eq!(retx_cooldown_us(WIRE_US, f), WIRE_US);
    // Tail sweep saturates its 100 ms clamp under app-echo and sits just
    // above its 25 ms floor under the wire clock.
    assert_eq!(tail_sweep_timeout_us(APP_ECHO_US), TAIL_SWEEP_MAX_US);
    assert_eq!(tail_sweep_timeout_us(WIRE_US), 28_000);
    // The ratios, stated as the claim: ×17.8 vs ×1.6 RTprop.
    assert_eq!(mp_time_threshold_split(0, APP_ECHO_US, f).0 / RTPROP_US, 17);
    assert_eq!(mp_time_threshold_split(0, WIRE_US, f).0 / RTPROP_US, 1);
}

/// The coincidence property. Wherever the fixed gauge's own stated
/// assumption holds (emission events at least as frequent as the 1 ms
/// loop wake), the derived threshold reproduces 3 000 µs to the
/// microsecond.
#[test]
fn derived_stall_threshold_reproduces_the_legacy_3ms_where_they_coincide() {
    for evt in [0u64, 1, 10, 100, 500, 999, 1_000] {
        assert_eq!(
            stall_threshold_us(evt),
            3_000,
            "evt={evt} µs must reproduce the legacy constant exactly"
        );
    }
}

/// The derived round's mechanism-liveness echo separates "the site ran"
/// from "the law bound", and it must, because the coincidence property
/// makes those different claims: an arm that only ever evaluates inside
/// the `[25, 100] ms` band is bit-identical to its control. Both echoes
/// are one-shot — the two call sites sit in per-iteration hot loops, so a
/// re-arming echo would flood the log.
#[test]
fn the_derived_round_echo_fires_once_per_claim_and_separates_ran_from_bound() {
    // Inside the band: the site ran, the law did not bind.
    let mut e = DerivedRoundEcho::default();
    e.observe("t", 20_000, 100, tail_sweep_timeout_us(20_000), tail_sweep_timeout_us(20_000));
    assert!(e.ran, "an evaluation inside the band must still prove execution");
    assert!(!e.diverged, "identical values are NOT a divergence");

    // Now the same site above the ceiling: the law binds, and only then.
    let srtt = 376_000;
    let derived = derived_recovery_round_us(srtt, 100);
    let legacy = tail_sweep_timeout_us(srtt);
    assert_ne!(derived, legacy, "the fixture must actually diverge");
    e.observe("t", srtt, 100, derived, legacy);
    assert!(e.diverged, "a departure from the clamped law must be echoed");

    // Both claims are latched: further evaluations re-emit nothing.
    let before = (e.ran, e.diverged);
    for _ in 0..1_000 {
        e.observe("t", srtt, 100, derived, legacy);
    }
    assert_eq!((e.ran, e.diverged), before, "both echoes are one-shot");

    // A site that diverges on its first evaluation latches both at once.
    let mut f = DerivedRoundEcho::default();
    f.observe("t", srtt, 100, derived, legacy);
    assert!(f.ran && f.diverged);
}

/// Neither echo's prose may contain the phrase the other echo is counted
/// on, and neither may contain a bare `RWM_DERIVED_SWEEP=<n>` that a
/// `[GATES]`-scoped grep could pick up: an echo whose own explanatory text
/// matches the pattern a driver counts corrupts the separation of "the
/// site ran" from "the law bound" with no other symptom.
#[test]
fn the_derived_round_echoes_do_not_match_each_others_grep_patterns() {
    let ran = DerivedRoundEcho::ran_msg("s", 1, 2, 3, 4);
    let div = DerivedRoundEcho::diverged_msg("s", 1, 2, 3, 4);

    assert!(ran.starts_with(DS_ECHO_RAN));
    assert!(div.starts_with(DS_ECHO_DIVERGED));
    assert!(
        !div.contains(DS_ECHO_RAN),
        "the DIVERGED echo must not match the execution grep: {div}"
    );
    assert!(
        !ran.contains(DS_ECHO_DIVERGED),
        "the ACTIVE echo must not match the binding grep: {ran}"
    );
    // The gate's own name may appear, but never with a resolved value:
    // that is the `[GATES]` line's job.
    for m in [&ran, &div] {
        assert!(
            !m.contains("RWM_DERIVED_SWEEP=1"),
            "echo prose must not carry a resolved gate value: {m}"
        );
    }
    // Both carry the five fields the parser reads, in the same dialect.
    for m in [&ran, &div] {
        for f in ["site=", "srtt_us=", "jitter_us=", "derived_us=", "legacy_us="] {
            assert!(m.contains(f), "{m} is missing {f}");
        }
    }
}

/// The law: monotone in the measured interval, both clamps, and the
/// departure only where the fixed gauge's assumption fails.
#[test]
fn derived_stall_threshold_scales_with_the_measured_event_interval() {
    // Above the loop wake it tracks 3 × the measured interval — this is
    // the batched-emitter regime the 3 ms constant mis-reads.
    assert_eq!(stall_threshold_us(2_000), 6_000);
    assert_eq!(stall_threshold_us(4_000), 12_000);
    // …up to the engine's own hole-refresh cadence, then it stops.
    assert_eq!(
        stall_threshold_us(100_000),
        HOLE_NACK_REFRESH_MIN.as_micros() as u64
    );
    assert_eq!(stall_threshold_us(u64::MAX), HOLE_NACK_REFRESH_MIN.as_micros() as u64);
    // Monotone non-decreasing, and never below the 3 ms constant.
    let mut prev = 0;
    for evt in (0..60_000).step_by(97) {
        let t = stall_threshold_us(evt);
        assert!(t >= prev && t >= 3_000, "evt={evt}");
        prev = t;
    }
}

/// One-directionality: over any gap trace, the derived stall total can
/// never exceed the fixed one, because the derived threshold is never
/// below the 3 ms constant.
/// So a shrink in `sidle2` is evidence of over-counting and can never be
/// an artifact of the new gauge itself.
#[test]
fn derived_stall_gauge_can_only_ever_report_less_than_the_legacy_one() {
    let mut state = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for evt in [200u64, 1_000, 2_500, 6_000, 40_000] {
        let thr = stall_threshold_us(evt);
        assert!(thr >= 3_000);
        let (mut legacy_us, mut legacy_n) = (0u64, 0u64);
        let (mut derived_us, mut derived_n) = (0u64, 0u64);
        for _ in 0..20_000 {
            let gap = next() % 30_000;
            if gap >= 3_000 {
                legacy_us += gap;
                legacy_n += 1;
            }
            if gap >= thr {
                derived_us += gap;
                derived_n += 1;
            }
        }
        assert!(derived_us <= legacy_us, "evt={evt}");
        assert!(derived_n <= legacy_n, "evt={evt}");
    }
}

/// The skew-aware hole law: a gap on path A while the seq's flight is
/// still inside path B's expected-arrival clock is not a hole; once
/// B's clock expires it is. Single path (N=1): the law never
/// suppresses, and an unknown flight is
/// never suppressed (reliability backstop).
#[test]
fn mp_hole_law_suppresses_young_cross_path_flights_only() {
    let thr = 45_000u64; // path B's 9/8×srtt clock
    // Dual path, flight sent at t=1_000_000 on B.
    // t = +10 ms: inside B's clock → not a hole.
    assert!(!mp_hole_ripe(2, 1_010_000, Some(1_000_000), thr));
    // t = +45 ms: B's clock expired → a hole (retransmit eligible).
    assert!(mp_hole_ripe(2, 1_045_000, Some(1_000_000), thr));
    // t = +44.999 ms: still inside (strict).
    assert!(!mp_hole_ripe(2, 1_044_999, Some(1_000_000), thr));
    // N=1: the law is inert — always ripe regardless of age (the
    // single-path gates own the decision).
    assert!(mp_hole_ripe(1, 1_010_000, Some(1_000_000), thr));
    assert!(mp_hole_ripe(0, 1_010_000, Some(1_000_000), thr));
    // Unknown flight: never suppress (a seq we cannot clock must stay
    // recoverable — the single-path gates decide).
    assert!(mp_hole_ripe(2, 1_010_000, None, thr));
}

/// The law composes with retransmit flight inheritance: after a
/// retransmit the live flight is the retransmit on its path, so the
/// seq is suppressed until the new flight's clock expires (closes the
/// re-NACK-while-flying feedback), then ripe again.
#[test]
fn mp_hole_law_clocks_the_live_flight_after_retransmit() {
    let thr_b = 45_000u64;
    // Original flight expired → fired at t=1_045_000; the retransmit
    // becomes the live flight at that instant.
    let retx_at = 1_045_000u64;
    // Immediately after: suppressed (the retransmit is still flying).
    assert!(!mp_hole_ripe(2, retx_at + 1_000, Some(retx_at), thr_b));
    // After the retransmit path's clock: ripe again (escalation if the
    // retransmit itself died).
    assert!(mp_hole_ripe(2, retx_at + thr_b, Some(retx_at), thr_b));
}

/// The packet-threshold fast channel (RFC 9002 §6.1.1 per path): a gap
/// report's implied delivered intervals, and the ≥3-same-path-successors
/// decision. Cross-path skew gaps cannot trigger it (their same-path
/// successors are equally un-arrived); real same-path losses fire in
/// ~one skew instead of a full RTT.
#[test]
fn mp_packet_threshold_evidence_and_decision() {
    // Report: missing 5..=6 and 9..=9 → delivered 7..=8 (between gaps)
    // and 10 (the seq that bounded the last gap).
    assert_eq!(
        mp_delivered_intervals(&[(5, 6), (9, 9)]),
        vec![(7, 8), (10, 10)]
    );
    // Single gap: only the bounding seq is provable.
    assert_eq!(mp_delivered_intervals(&[(5, 6)]), vec![(7, 7)]);
    // Adjacent gaps produce no between-interval.
    assert_eq!(mp_delivered_intervals(&[(5, 6), (7, 8)]), vec![(9, 9)]);
    assert!(mp_delivered_intervals(&[]).is_empty());

    // Decision: 3 delivered path-j successors above s = lost.
    let ev = vec![10, 12, 14, 16];
    assert!(mp_fast_lost(&ev, 9), ">=3 successors above 9");
    assert!(mp_fast_lost(&ev, 10), "3 above 10 (12,14,16)");
    assert!(!mp_fast_lost(&ev, 12), "only 2 above 12");
    assert!(!mp_fast_lost(&ev, 20), "none above 20");
    assert!(!mp_fast_lost(&[], 0), "no evidence, never lost-fast");
    // The cross-path skew shape: path A delivered 100..102 while s=50
    // flies on B with no delivered B successors — B's evidence list is
    // empty, so the fast channel never fires for B's flight.
    let b_ev: Vec<u64> = vec![];
    assert!(!mp_fast_lost(&b_ev, 50));
}

// ----- Per-path batch serial namespaces -----

/// The loss-serial defect, reproduced at the unit: a global batch_seq
/// striped across two paths makes each path's tracker read the other
/// path's run as loss (expected ≈ 2×received at round-robin — ~50%
/// phantom loss with zero real loss). Per-path serial namespaces
/// (each path's stream sequential) read exactly 0% loss on the same
/// arrival pattern.
#[test]
fn per_path_batch_serials_kill_striping_phantom_loss() {
    // Global counter, round-robin striping, no loss: path 0 gets
    // even serials, path 1 odd.
    let mut t0 = PathBatchTracker::new();
    let mut t1 = PathBatchTracker::new();
    for s in 0..200u64 {
        if s % 2 == 0 {
            t0.record_batch(s, 1);
        } else {
            t1.record_batch(s, 1);
        }
    }
    // Phantom loss: each path expected ~2× what it received.
    assert!(t0.total_expected >= t0.total_received * 2 - 2);
    assert!(t1.total_expected >= t1.total_received * 2 - 2);

    // Per-path serials, same striping, no loss: sequential per path.
    let mut p0 = PathBatchTracker::new();
    let mut p1 = PathBatchTracker::new();
    for s in 0..100u64 {
        p0.record_batch(s, 1);
        p1.record_batch(s, 1);
    }
    assert_eq!(p0.total_expected, p0.total_received, "no phantom loss");
    assert_eq!(p1.total_expected, p1.total_received, "no phantom loss");

    // Per-path serials still see real loss: drop serials 10..=14.
    let mut pl = PathBatchTracker::new();
    for s in 0..100u64 {
        if (10..=14).contains(&s) {
            continue; // lost on the wire
        }
        pl.record_batch(s, 1);
    }
    assert_eq!(pl.total_expected - pl.total_received, 5, "real loss still counted");
}

/// A hostile/corrupt ack cannot trap the diff in a huge loop.
#[test]
fn copa_feed_per_ack_work_is_bounded() {
    let feed = CopaFeed::new();
    let newly = feed.newly_delivered(u64::MAX - 1, &[]);
    assert!(newly.len() <= 65_536);
}

/// The recovery clocks must not lose a path because its cwnd is full.
/// `active_paths()` drops every path with `available() == 0`, so on a
/// sender whose paths are all cwnd-saturated (the normal state of a
/// wire-bound bulk transfer) the pooled clock must not fall to the 10 ms
/// floor instead of the paths' measured RTT.
#[test]
fn recovery_clocks_keep_cwnd_saturated_paths() {
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);
    sched.add_path(1);
    for (id, ms) in [(0u32, 40u64), (1, 80)] {
        let p = sched.path_mut(id).unwrap();
        for _ in 0..32 {
            p.estimator.record_rtt(Duration::from_millis(ms));
        }
        let cw = p.cwnd;
        p.charge_in_flight(cw);
        assert_eq!(p.available(), 0, "path {id} must be cwnd-saturated");
    }
    assert!(sched.active_paths().is_empty(), "precondition: both paths saturated");
    let want = [0u32, 1]
        .iter()
        .map(|id| sched.path(*id).unwrap().estimator.rtt().as_micros() as u64)
        .max()
        .unwrap();
    assert!(want > NACK_RETX_COOLDOWN_FLOOR_US, "precondition: measured RTT above the floor");
    let mut ids = recovery_clock_paths(&sched);
    ids.sort_unstable();
    assert_eq!(ids, vec![0, 1], "recovery clocks must pool over every live path");
    assert_eq!(
        pooled_recovery_srtt_of(&sched),
        want,
        "pooled recovery SRTT must be the saturated paths' measured RTT, not the floor"
    );
}

/// The engine clock is monotonic, non-zero and never panics. (A wall-
/// clock step cannot be injected from a unit test; the guarantee against
/// it is structural — the value is `base + Instant::elapsed()`.)
#[test]
fn now_us_is_monotonic_and_nonzero() {
    let mut prev = now_us();
    assert!(prev >= NOW_US_MIN_BASE, "stamps start at or above the base");
    for _ in 0..10_000 {
        let t = now_us();
        assert!(t >= prev, "now_us went backwards: {t} < {prev}");
        prev = t;
    }
    std::thread::sleep(Duration::from_millis(2));
    assert!(now_us() > prev, "now_us advances with real time");
}

// ----- sack_to_gaps (SACK-driven reactive repair) -----

#[test]
fn test_sack_to_gaps_single_hole() {
    // Delivered 0..=4 (`next_expected = 5`); receiver has 7..=9 → 5..=6 missing.
    assert_eq!(sack_to_gaps(5, &[(7, 9)]), vec![(5, 6)]);
}

#[test]
fn test_sack_to_gaps_multiple_holes() {
    // Delivered seq 0 (`next_expected = 1`); has 2..=3 and 6..=6 → 1 and
    // 4..=5 missing.
    assert_eq!(sack_to_gaps(1, &[(2, 3), (6, 6)]), vec![(1, 1), (4, 5)]);
}

#[test]
fn test_sack_to_gaps_adjacent_range_no_gap() {
    // Sack range starts right after the cumulative point → nothing
    // missing below it, and seqs above it are not reported (may be
    // in flight).
    assert!(sack_to_gaps(5, &[(5, 9)]).is_empty());
}

#[test]
fn test_sack_to_gaps_round_trips_receiver_encoding() {
    // The receiver converts its missing-gap view into received
    // (SACK) ranges; sack_to_gaps must invert that exactly.
    let mut received = BTreeSet::new();
    for seq in [11u64, 12, 15, 18, 19, 20] {
        received.insert(seq);
    }
    let highest_delivered = 10u64; // 0..=10 contiguous
    let highest_seen = 20u64;
    let gaps = compute_gap_ranges(&received, highest_delivered, highest_seen);
    // Receiver-side conversion (as in the WindowAck send path)
    let mut sack_ranges = Vec::new();
    let mut cursor = highest_delivered + 1;
    for &(gap_start, gap_end) in &gaps {
        if cursor < gap_start {
            sack_ranges.push((cursor, gap_start - 1));
        }
        cursor = gap_end + 1;
    }
    if cursor <= highest_seen {
        sack_ranges.push((cursor, highest_seen));
    }
    assert_eq!(sack_ranges, vec![(11, 12), (15, 15), (18, 20)]);
    // Sender-side inversion recovers the missing seqs 13..=14, 16..=17
    assert_eq!(sack_to_gaps(highest_delivered + 1, &sack_ranges), vec![(13, 14), (16, 17)]);
}

/// Seq 0 lost. Through wire v8 a receiver that had delivered nothing
/// advertised `received_up_to = 0` — the same value as "seq 0 delivered".
/// v9 advertises `next_expected = 0`. Driven through the receiver's own
/// encoding (`received_sack_ranges`) and the sender's inversion: seq 0
/// must be reported as a gap, and when seq 0 was delivered it must not.
#[test]
fn a_lost_seq_zero_is_sack_reported() {
    // Seq 0 dropped; 1..=10 received, none deliverable in order.
    let received: BTreeSet<u64> = (1..=10).collect();
    let ranges = received_sack_ranges(&received, 0, 10);
    let gaps = sack_to_gaps(0, &ranges);
    assert!(
        gaps.iter().any(|&(a, _)| a == 0),
        "seq 0 lost must be SACK-reported: ranges={ranges:?} gaps={gaps:?}"
    );
    assert_eq!(gaps, vec![(0, 0)]);
    // Control: seq 0 delivered (`next_expected = 1`), seq 1 lost, 2..=10
    // received.
    let received: BTreeSet<u64> = (2..=10).collect();
    let ranges = received_sack_ranges(&received, 1, 10);
    assert_eq!(sack_to_gaps(1, &ranges), vec![(1, 1)], "delivered seq 0 is not re-reported");
}

/// Wire v9: seqs 0 AND 1 lost, 2..=10 received, nothing delivered. The
/// cumulative point is `next_expected = 0`, so the sender's inversion must
/// name the whole leading hole `(0, 1)` at once. v8's `received_up_to = 0`
/// could not say "nothing delivered", and its seq-0 heuristic only fired
/// when the first SACK range started at 1, so this report named `(1, 1)`
/// and seq 0 waited a round for the tail sweep.
#[test]
fn lost_seqs_zero_and_one_are_both_sack_reported() {
    let received: BTreeSet<u64> = (2..=10).collect();
    let ranges = received_sack_ranges(&received, 0, 10);
    assert_eq!(ranges, vec![(2, 10)]);
    assert_eq!(sack_to_gaps(0, &ranges), vec![(0, 1)], "both leading holes, one report");
}

#[test]
fn test_sack_to_gaps_caps_at_max_gaps() {
    // 2×MAX_NACK_GAPS isolated received seqs → gap list is capped.
    let sack: Vec<(u64, u64)> = (0..(MAX_NACK_GAPS as u64 * 2))
        .map(|i| (2 + i * 2, 2 + i * 2))
        .collect();
    let gaps = sack_to_gaps(0, &sack);
    assert_eq!(gaps.len(), MAX_NACK_GAPS);
}

#[test]
fn test_received_sack_ranges_inverts_to_gaps() {
    // The extracted helper must produce exactly the data-arm WindowAck's
    // ranges, and round-trip via sack_to_gaps.
    let mut received = BTreeSet::new();
    for seq in [3u64, 4, 7] {
        received.insert(seq);
    }
    // 0..=2 delivered: `next_expected = 3`.
    let ranges = received_sack_ranges(&received, 3, 7);
    assert_eq!(ranges, vec![(3, 4), (7, 7)]);
    assert_eq!(sack_to_gaps(3, &ranges), vec![(5, 6)]);
}

// ----- SACK truncation (plan 2a): a capped report must be a PREFIX --------

/// The true received runs in `(delivered, seen]`, uncapped — the oracle the
/// truncation tests compare the wire encoding against.
fn true_received_runs(received: &BTreeSet<u64>, delivered: u64, seen: u64) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::new();
    for &s in received.range(delivered + 1..=seen) {
        match out.last_mut() {
            Some((_, e)) if *e + 1 == s => *e = s,
            _ => out.push((s, s)),
        }
    }
    out
}

/// A receiver with far more holes than one report can name: `delivered =
/// 10`, then every other seq received for `5·MAX_NACK_GAPS` gaps. Returns
/// `(received, delivered, seen, missing)`.
fn many_gap_fixture() -> (BTreeSet<u64>, u64, u64, BTreeSet<u64>) {
    let delivered = 10u64;
    let n_gaps = 5 * MAX_NACK_GAPS as u64;
    // Received: 11, 13, 15, ... ; missing: 12, 14, 16, ...
    // The receiver's set holds the delivered frontier seq itself (its prune
    // keeps everything at or above `highest_delivered_seq`).
    let mut received: BTreeSet<u64> = (0..=n_gaps).map(|i| delivered + 1 + 2 * i).collect();
    received.insert(delivered);
    let seen = *received.iter().next_back().unwrap();
    let missing: BTreeSet<u64> =
        (delivered + 1..=seen).filter(|s| !received.contains(s)).collect();
    assert_eq!(missing.len() as u64, n_gaps);
    (received, delivered, seen, missing)
}

/// Encoder side: no emitted SACK range may cover a seq the receiver does not
/// have, and the emitted list must be a prefix of the true received runs
/// (the only truncation the wire can express honestly: `sack_to_gaps`
/// reads everything above the last range as "not reported").
#[test]
fn sack_truncated_report_never_claims_a_missing_seq() {
    let (received, delivered, seen, missing) = many_gap_fixture();
    let ranges = received_sack_ranges(&received, delivered + 1, seen);
    for &(a, b) in &ranges {
        for s in a..=b {
            assert!(
                !missing.contains(&s),
                "SACK range ({a},{b}) claims missing seq {s} as received \
                 (ranges.len()={}, last={:?})",
                ranges.len(),
                ranges.last()
            );
        }
    }
    let truth = true_received_runs(&received, delivered, seen);
    assert!(!ranges.is_empty() && ranges.len() <= truth.len());
    assert_eq!(
        ranges[..],
        truth[..ranges.len()],
        "the capped report must be a prefix of the true received runs"
    );
}

/// Sender side: feed the (possibly capped) report through the sender's own
/// inversion and SACK-clocked release. No missing seq may be released, and
/// every derived gap must be a true hole.
#[test]
fn sack_truncated_report_releases_no_missing_seq_at_the_sender() {
    let (received, delivered, seen, missing) = many_gap_fixture();
    let ranges = received_sack_ranges(&received, delivered + 1, seen);
    // The sender retains every seq above the cumulative point.
    let sent_store: BTreeMap<u64, ()> = (delivered + 1..=seen).map(|s| (s, ())).collect();
    let mut released = BTreeSet::new();
    for &(a, b) in &ranges {
        sack_release_mark(&sent_store, &mut released, a, b);
    }
    let wrongly: Vec<u64> = released.intersection(&missing).copied().collect();
    assert!(
        wrongly.is_empty(),
        "sender released {} never-received seqs (first {:?}) off a truncated SACK",
        wrongly.len(),
        wrongly.first()
    );
    for (a, b) in sack_to_gaps(delivered + 1, &ranges) {
        for s in a..=b {
            assert!(missing.contains(&s), "derived gap seq {s} was in fact received");
        }
    }
}

/// Byte-identity at the old cap: exactly `MAX_NACK_GAPS` gaps with a
/// received run above the last one — the final `(cursor, seen)` range is
/// emitted, exactly as before the truncation fix.
#[test]
fn sack_report_at_the_gap_cap_keeps_its_tail_range() {
    let delivered = 10u64;
    let n_gaps = MAX_NACK_GAPS as u64;
    let mut received: BTreeSet<u64> = (0..n_gaps).map(|i| delivered + 1 + 2 * i).collect();
    received.insert(delivered); // as the receiver's set holds it
    let tail_start = delivered + 1 + 2 * n_gaps;
    received.extend(tail_start..tail_start + 5);
    let seen = tail_start + 4;
    let ranges = received_sack_ranges(&received, delivered + 1, seen);
    assert_eq!(ranges, true_received_runs(&received, delivered, seen));
    assert_eq!(ranges.len(), MAX_NACK_GAPS + 1);
    assert_eq!(*ranges.last().unwrap(), (tail_start, seen));
    assert_eq!(sack_to_gaps(delivered + 1, &ranges).len(), MAX_NACK_GAPS);
}

/// Exactly `MAX_SACK_RANGES` true runs are all reported; one more run is cut
/// at the cap and the cut list is still an honest prefix (the next run is
/// simply unreported, so its leading hole is not released either).
#[test]
fn sack_report_is_capped_at_max_sack_ranges_as_a_prefix() {
    let delivered = 100u64;
    for n_runs in [MAX_SACK_RANGES, MAX_SACK_RANGES + 1] {
        let mut received: BTreeSet<u64> =
            (0..n_runs as u64).map(|i| delivered + 2 + 3 * i).collect();
        received.insert(delivered);
        let seen = *received.iter().next_back().unwrap();
        let truth = true_received_runs(&received, delivered, seen);
        assert_eq!(truth.len(), n_runs);
        let ranges = received_sack_ranges(&received, delivered + 1, seen);
        assert_eq!(ranges.len(), n_runs.min(MAX_SACK_RANGES));
        assert_eq!(ranges[..], truth[..ranges.len()]);
        // Every gap the sender derives is a true hole.
        for (a, b) in sack_to_gaps(delivered + 1, &ranges) {
            for s in a..=b {
                assert!(!received.contains(&s), "derived gap seq {s} was received");
            }
        }
    }
    // Degenerate frontiers: nothing above the cumulative point (v9 count
    // form: 0..=7 delivered is `next_expected = 8`).
    let received: BTreeSet<u64> = [5u64, 6, 7].into_iter().collect();
    assert!(received_sack_ranges(&received, 8, 7).is_empty());
    assert!(received_sack_ranges(&received, 10, 7).is_empty());
    // And nothing seen at all: `next_expected = 0`, `seen = 0`.
    assert!(received_sack_ranges(&BTreeSet::new(), 0, 0).is_empty());
}

/// The SACK cap is what one control datagram can carry: a `WindowAck` with
/// `n` ranges serializes to exactly `WINDOW_ACK_BASE_BYTES +
/// WINDOW_ACK_BYTES_PER_RANGE·n` bytes, the cap fits the datagram budget
/// with the declared margin, and it is the LARGEST such `n`.
#[test]
fn window_ack_sack_size_fits_the_control_datagram() {
    use crate::transport::{ControlMessage, WireMessage};
    let size = |n: usize| {
        WireMessage::Control(ControlMessage::WindowAck {
            next_expected: u64::MAX,
            received_above: u32::MAX,
            sack_ranges: (0..n as u64).map(|i| (u64::MAX - i, u64::MAX)).collect(),
            echo_send_timestamp_us: u64::MAX,
            jitter_us: u32::MAX,
            cumulative_received: u64::MAX,
            cum_expected: u64::MAX,
            cum_received: u64::MAX,
        })
        .serialize()
        .expect("serialize")
        .len()
    };
    assert_eq!(size(0), WINDOW_ACK_BASE_BYTES, "WindowAck base size moved");
    assert_eq!(size(1) - size(0), WINDOW_ACK_BYTES_PER_RANGE, "per-range stride moved");
    let at_cap = size(MAX_SACK_RANGES);
    assert_eq!(at_cap, WINDOW_ACK_BASE_BYTES + WINDOW_ACK_BYTES_PER_RANGE * MAX_SACK_RANGES);
    assert!(
        at_cap + SACK_RANGE_MARGIN_BYTES <= SACK_DATAGRAM_BUDGET,
        "WindowAck at MAX_SACK_RANGES={MAX_SACK_RANGES} is {at_cap} B, over the \
         {SACK_DATAGRAM_BUDGET} B control-datagram budget less margin"
    );
    assert!(
        size(MAX_SACK_RANGES + 1) + SACK_RANGE_MARGIN_BYTES > SACK_DATAGRAM_BUDGET,
        "MAX_SACK_RANGES is not the largest count that fits"
    );
    // Never below the old per-report gap cap's worth of ranges: a report
    // with MAX_NACK_GAPS gaps (MAX_NACK_GAPS + 1 runs) is still uncut.
    assert!(MAX_SACK_RANGES > MAX_NACK_GAPS);
    eprintln!("WindowAck: {} B + {} B/range; cap {MAX_SACK_RANGES} -> {at_cap} B of {SACK_DATAGRAM_BUDGET}",
        WINDOW_ACK_BASE_BYTES, WINDOW_ACK_BYTES_PER_RANGE);
}

/// Wire v9 (plan 2c) — the store gate CONVERGES at a stalled frontier past
/// the SACK cap. The SACK report is an honest prefix capped at
/// `MAX_SACK_RANGES`, so with the frontier stalled and more received runs
/// above it than one report can name, every report is the SAME prefix and
/// the mark set never gets past the first `MAX_SACK_RANGES` runs (v8 pinned
/// that shortfall here: 2·(n_runs − 64) received seqs over-counted forever).
/// v9 carries `received_above` (distinct received seqs above the frontier)
/// on every WindowAck, and the gate's released count is ONE always-computed
/// expression `max(|marks|, received_above − not_retained)`, so the operand
/// lands exactly on the unreceived count.
#[test]
fn stalled_frontier_store_gate_converges_past_the_sack_cap() {
    let delivered = 1_000u64;
    let n_runs = 3 * MAX_SACK_RANGES as u64;
    // Frontier hole at delivered+1; runs of 2 received seqs, 1-seq holes.
    let mut received: BTreeSet<u64> = BTreeSet::new();
    received.insert(delivered);
    for i in 0..n_runs {
        let a = delivered + 2 + 3 * i;
        received.insert(a);
        received.insert(a + 1);
    }
    let seen = *received.iter().next_back().unwrap();
    let sent_store: BTreeMap<u64, ()> = (delivered + 1..=seen).map(|s| (s, ())).collect();
    let received_above = received.range(delivered + 1..).count();
    let mut released = BTreeSet::new();
    for _report in 0..50 {
        for (a, b) in received_sack_ranges(&received, delivered + 1, seen) {
            sack_release_mark(&sent_store, &mut released, a, b);
        }
    }
    // The per-seq mark set still stops at the prefix (it is exact per seq
    // and keeps its per-seq consumers: Copa attribution, per-path release).
    assert_eq!(released.len(), 2 * MAX_SACK_RANGES, "the mark set is the prefix");
    assert!(released.iter().all(|s| received.contains(s)), "and never lies");
    // The GATE converges: what it uncounts is every received seq above the
    // frontier, so the outstanding it reads is exactly the unreceived count.
    // The report is what the receiver's WindowAck carries: `next_expected
    // = delivered + 1` and `received_above` (the receiver's set holds the
    // frontier seq itself, which is below `next_expected` and not counted).
    let mut report = AboveReport::default();
    report.fold(delivered + 1, received_above as u32);
    let gate_released = store_gate_released(&sent_store, released.len(), report);
    assert_eq!(
        gate_released, received_above,
        "the store gate must release every received seq above the stalled frontier, not just the SACK prefix"
    );
    assert_eq!(
        sent_store.len() - gate_released,
        sent_store.keys().filter(|s| !received.contains(s)).count(),
        "outstanding = exactly the holes"
    );
}

/// Wire v9: the store gate's released count is a LOWER bound on the
/// retained seqs the receiver actually has -- for any report, stale or
/// fresh, from any path, with the sender's store pruned past the report's
/// frontier or not. A randomized sweep against the brute-force truth
/// `|S ∩ received|`; also asserts that a FRESH report is exact on a dense
/// store (the convergence the pinned test above needs).
#[test]
fn store_gate_released_never_exceeds_the_retained_received_truth() {
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move |m: u64| {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng >> 33) % m.max(1)
    };
    for _case in 0..2_000 {
        // The sender sent 0..sent; the receiver received a random subset.
        let sent = 1 + next(400);
        let received: BTreeSet<u64> = (0..sent).filter(|_| next(10) < 7).collect();
        // Receiver frontier at report time: its delivered prefix.
        let mut f_r = 0u64;
        while received.contains(&f_r) {
            f_r += 1;
        }
        let above = received.range(f_r..).count() as u32;
        let mut report = AboveReport::default();
        report.fold(f_r, above);
        // The sender's store: pruned at a cumulative point that may be
        // behind the report (ack not yet processed) or ahead of it (the
        // report is stale: a later ack already moved the frontier).
        let f_s = if next(2) == 0 { f_r.saturating_sub(next(5)) } else {
            let mut f = f_r;
            // A later frontier the receiver really reached.
            while f < sent && next(3) != 0 {
                f += 1;
            }
            f
        };
        let store: BTreeMap<u64, ()> = (f_s..sent).map(|s| (s, ())).collect();
        // Marks: an honest subset of the retained received seqs.
        let marks: usize = store.keys().filter(|s| received.contains(s) && next(3) == 0).count();
        let truth = store.keys().filter(|s| received.contains(s) || **s < f_r).count();
        let gate = store_gate_released(&store, marks, report);
        assert!(
            gate <= truth,
            "gate released {gate} > truth {truth} (sent {sent}, f_r {f_r}, f_s {f_s}, above {above})"
        );
        if f_s == f_r {
            // Fresh report on a dense store: exact.
            assert_eq!(gate, truth, "a fresh report must converge exactly");
        }
    }
}

/// The receiver's incremental `received_above` counter equals the
/// brute-force `|received ∩ [next_expected, ∞)|` under inserts (new and
/// duplicate, above and below the frontier), frontier advances, and prunes
/// below the frontier -- the contract the receiver loop keeps.
#[test]
fn received_above_counter_matches_the_brute_force_count() {
    let mut rng: u64 = 0xD1B5_4A32_D192_ED03;
    let mut next = move |m: u64| {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng >> 33) % m.max(1)
    };
    let mut set: BTreeSet<u64> = BTreeSet::new();
    let mut ctr = ReceivedAbove::new();
    let mut frontier = 0u64;
    for step in 0..20_000u64 {
        match next(10) {
            0..=6 => {
                let seq = frontier.saturating_sub(5) + next(200);
                let newly = set.insert(seq);
                ctr.on_insert(seq, newly);
            }
            7 | 8 => {
                while set.contains(&frontier) {
                    frontier += 1;
                }
                if next(4) == 0 {
                    frontier += next(3); // a hold-expiry jump past a hole
                }
            }
            _ => {
                ctr.sync(&set, frontier);
                let prune_before = frontier.saturating_sub(1 + next(20));
                set = set.split_off(&prune_before);
            }
        }
        if step % 7 == 0 {
            let want = set.range(frontier..).count() as u32;
            assert_eq!(ctr.sync(&set, frontier), want, "step {step}");
        }
    }
}

/// Wire v9 seq-0 cases at the sender: a delivered seq 0 alone
/// (`next_expected = 1`) is a cumulative advance that prunes seq 0 from the
/// store, the marks and the per-path accounts; `next_expected = 0` advances
/// nothing. Through v8 both read `received_up_to = 0` and seq 0 was never
/// pruned.
#[test]
fn a_delivered_seq_zero_alone_is_acked_and_pruned() {
    assert_eq!(sender_phases::cumulative_advance(0, 0), None, "nothing delivered");
    let adv = sender_phases::cumulative_advance(0, 1);
    assert_eq!(adv, Some((0, 0)), "seq 0 delivered alone is an advance");
    assert_eq!(sender_phases::cumulative_advance(1, 1), None, "a duplicate ack");
    assert_eq!(sender_phases::cumulative_advance(1, 4), Some((1, 3)));
    // The prunes on_ack_advance runs, at `next_expected = 1`.
    let mut store: BTreeMap<u64, ()> = (0..5).map(|s| (s, ())).collect();
    let mut marks: BTreeSet<u64> = [0u64, 2].into_iter().collect();
    let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
    let mut out: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for s in 0..5 {
        percap_charge(&mut acct, &mut out, s, 0);
    }
    let (_, ack) = adv.unwrap();
    store = store.split_off(&(ack + 1));
    sack_release_prune(&mut marks, 1);
    percap_release_cumulative(&mut acct, &mut out, 1);
    assert!(!store.contains_key(&0) && store.len() == 4, "seq 0 pruned, 1..=4 kept");
    assert_eq!(marks.into_iter().collect::<Vec<_>>(), vec![2]);
    assert_eq!(out[&0], 4);
}

// ----- Path-scaled outstanding pool (RWM_STORE_PATHS, paper §6.1) --------

#[test]
fn path_scaled_store_cap_is_legacy_for_singles_and_off() {
    // Flag off: always the single-path law, regardless of path count.
    assert_eq!(path_scaled_store_cap(false, 2, 1000.0, 2.0, 64, 2048), None);
    // Flag on but a single live path: the single-path law bit-exactly.
    assert_eq!(path_scaled_store_cap(true, 1, 1000.0, 2.0, 64, 2048), None);
    // No dynamic base yet (anchor cold): the boot cap decides.
    assert_eq!(path_scaled_store_cap(true, 2, 0.0, 2.0, 64, 2048), None);
}

#[test]
fn path_scaled_store_cap_scales_value_and_ceiling_with_paths() {
    // C7-shaped: Σ anchor-BDP ≈ 1076, gain 2, N = 2 → 2·2·1076 = 4304,
    // clamped at the N×2048 = 4096 ceiling (the per-path knee).
    assert_eq!(
        path_scaled_store_cap(true, 2, 1076.0, 2.0, 64, 2048),
        Some(4096)
    );
    // Below the ceiling the dynamic value rules (transient anchor sag).
    assert_eq!(
        path_scaled_store_cap(true, 2, 500.0, 2.0, 64, 2048),
        Some(2000)
    );
    // Floor guards a transiently-tiny estimate.
    assert_eq!(path_scaled_store_cap(true, 2, 1.0, 2.0, 64, 2048), Some(64));
    // Three paths: ceiling 3×2048.
    assert_eq!(
        path_scaled_store_cap(true, 3, 4000.0, 2.0, 64, 2048),
        Some(3 * 2048)
    );
}

// ----- Law-shape tests (measurement-discipline rule 17) ------------------

/// The law-shape template, so the next law can be covered by copying a
/// test.
///
/// `path_scaled_store_cap`'s value is `gain·N·Σᵢ anchorᵢ`, and at
/// symmetric inputs `Σ` is itself ∝ N — so the value is quadratic in the
/// path count while the pool is a sum over paths, i.e. linear (paper
/// §6.1). Three test habits hide such a defect:
///
///  1. The clamp eats the evidence. `clamp(·, floor, N·knee)` is pinned at
///     the ceiling for every Σ ≥ knee/gain = 1024. A test that reads the
///     law through its clamp measures the ceiling, not the law. ⇒ Test the
///     unclamped value and the clamp separately: pick inputs that make the
///     clamp provably inert (huge pool, floor ≈ 0) for the value, and
///     inputs that make it provably binding for the ceiling.
///  2. Axes the cells never exercise. With N ∈ {1, 2}, N² and N are
///     indistinguishable as a ratio unless asserted against an absolute
///     form. ⇒ Sweep the structural axis synthetically (here N = 1..8).
///  3. Points instead of shapes. A point (`cap(2, 1076) == 4096`) is
///     satisfied by any law that passes through it. ⇒ Assert the
///     exponent/closed form itself, on synthetic inputs chosen so the
///     closed form is hand-computable.
///
/// The template, applied to any new law `f(N, x…)`:
///
/// ```text
///   a. synthetic SYMMETRIC inputs (equal per-path term), round numbers;
///   b. neutralise every clamp, then assert the closed form over N = 1..8;
///   c. re-engage each clamp on its own and assert ITS shape (a clamp may
///      never be the only thing making a law sane);
///   d. state the DERIVED shape in the test name, so a change of shape is
///      a change of a test name and therefore a reviewed decision.
/// ```
///
/// The two `path_scaled_store_cap` tests below pin the quadratic law
/// (the `RWM_SUM_CAP=0` arm) rather than fix it (CLAUDE.md: "every
/// documented divergence must carry a test that bounds it"). The third
/// applies the same template to [`three_term_store_cap`], which is linear
/// in N as derived.
/// The full arm, resolved: does the composition actually compose?
///
/// Compositions whose members do not compose are the failure mode: e.g.
/// the retired `RWM_STORE_CAPW` made the (also retired, plan 2b)
/// `RWM_STORE_CAP_UNIFIED` bit a no-op wherever capw engaged, because
/// `capw_store_cap` sat above `path_scaled_store_cap` in the chain and read
/// `live_paths()` unconditionally. The Σ's set is now membership everywhere.
///
/// This test asserts the full arm's gate set resolves to the machine it is
/// supposed to be, at the policy layer where the collapses happen:
///
/// * every one of the five bits survives resolution (none is silently
///   ANDed away by a scope it does not satisfy);
/// * the pool law reached is the pooled one, not the three-term law — i.e.
///   `three_term_on` and `composed_cap` stay off (the composed pool law's
///   magnitude is refuted, paper §10);
/// * the brake is armed without it;
/// * and the control arm — every bit off, `RWM_SUM_CAP` explicitly off
///   (it defaults on) — still resolves to the quadratic chain and its 4096
///   pin, which makes the pair an A/B and keeps the displaced arm
///   re-runnable.
///
/// `RWM_DELTA_CAP` (default on) is a fixed control here, off on both arms:
/// this pair's values are the `gain = 2.0` pool's, and the value
/// multiplier is not the factor under test. The shipped default is
/// asserted first.
#[test]
fn the_full_arm_gate_set_resolves_to_the_intended_machine() {
    use crate::control::fec_rate::ProtocolHint;
    use crate::gates::RuntimeGates;
    use crate::net::sender_policy::SenderPolicy;

    // The plain-reliable window sender: the seat the cap law governs.
    let resolve = |g: &RuntimeGates| {
        SenderPolicy::resolve(g, 1200, ProtocolHint::Auto, true, false, false, false)
    };

    // ── The control arm ───────────────────────────────────────────────
    // Arm A is the quadratic law, which is not the default (`RWM_SUM_CAP`
    // resolves on), so the control is constructed explicitly as the `=0`
    // arm. Set by field, not through the environment (see the full arm's
    // note below).
    let mut base = RuntimeGates::resolve();
    assert!(
        base.sum_cap,
        "the shipped default must carry the ×N deletion — \
         if this fails the flip drifted back (gates.rs pins it too)"
    );
    assert!(
        base.delta_cap,
        "the shipped default must carry the δ-cap (paper §6.1) \
         — if this fails the flip drifted back (gates.rs pins it too)"
    );
    base.sum_cap = false; // the DISPLACED quadratic, still re-runnable
    // The value multiplier is fixed off on both arms of this pair. This
    // test varies the count multiplier against the `gain = 2.0` pool, and
    // its two c8 values (4096 and 3020) are that law's. The δ-cap is pinned
    // to `=0` on both arms so it cancels out of the contrast.
    base.delta_cap = false;
    let ctl = resolve(&base);
    assert!(!ctl.delta_cap, "control: the δ-cap must be fixed OFF for this pair");
    assert!(ctl.plain_dyn_cap, "the control arm is not on the dyn-cap seat");
    assert!(!ctl.sum_cap, "control: the ×N deletion must be OFF");
    assert!(!ctl.late_brake, "control: the brake must be OFF");
    assert!(!ctl.three_term_on && !ctl.composed_cap, "control: pooled law only");

    // ── The full arm ──────────────────────────────────────────────────
    // Set by field rather than through the environment on purpose: an
    // env-mutating test is process-global state in a parallel runner.
    let mut full = RuntimeGates::resolve();
    full.sum_cap = true; // the ×N deletion            (paper §6.1)
    full.delta_cap = false; // the value multiplier, FIXED OFF — see the
                            // control arm's note: this pair varies the
                            // count multiplier and nothing else.
    full.late_brake = true; // the late-stage brake
    full.loss_sent_truth = true; // ── the ledger/loss trio ──
    full.release_1to1 = true;
    full.charge_recovery = true;
    let arm = resolve(&full);
    assert!(
        !arm.delta_cap,
        "FULL: the δ-cap must be fixed OFF on this arm too — the pair \
         varies the COUNT multiplier, and a value multiplier that moved \
         with it would make the two published c8 values a different law's"
    );

    // 1. Every bit survives resolution. A gate that resolves off because
    //    of a scope it does not satisfy is a null effect, and would be
    //    scored as a null result by anything that reads only the
    //    `[GATES]` echo.
    assert!(arm.sum_cap, "FULL: RWM_SUM_CAP was ANDed away by resolution");
    assert!(arm.late_brake, "FULL: the brake was ANDed away");
    // The ledger/loss trio is not resolved through `SenderPolicy`: those
    // three bits are read by the scheduler (`scheduler::loss_sent_truth_active`
    // and siblings), because they govern the per-path estimator and the
    // in-flight ledger rather than the sender's policy. Having no shared
    // resolution step with the cap gates, they cannot be ANDed away by
    // them. Asserted at the gate surface, the layer they live on.
    assert!(full.loss_sent_truth, "FULL: the loss truth bit is not set");
    assert!(full.release_1to1, "FULL: the 1:1 release bit is not set");
    assert!(full.charge_recovery, "FULL: the recovery charge bit is not set");
    assert!(
        full.echo_line().contains("RWM_LOSS_SENT_TRUTH=1")
            && full.echo_line().contains("RWM_RELEASE_1TO1=1")
            && full.echo_line().contains("RWM_CHARGE_RECOVERY=1"),
        "FULL: the trio must be visible in the echo a battery parses: {}",
        full.echo_line()
    );

    // 2. The pool law is the pooled one. The composed pool law's magnitude
    //    is refuted (WIN_STORE_MAX becomes the law at every dual), so the
    //    full arm carries the ×N deletion instead of it, not as well.
    assert!(
        !arm.three_term_on && !arm.composed_cap,
        "FULL: the composed/three-term pool law engaged — the arm is not \
         the ×N deletion any more"
    );

    // 3. The brake is armed without the composed law: `RWM_LATE_BRAKE`
    //    arms the cwnd brake without forcing `three_term_on`.
    assert!(
        arm.late_brake && !arm.composed_cap,
        "FULL: the brake is only reachable through the composed law — the \
         extraction did not take"
    );

    // 4. The cap law under this policy is the corrected formula, evaluated
    //    through the same function the engine calls, at the wire's own c8 Σ.
    const SIGMA_C8: f64 = 1_509.677;
    let corrected = pooled_store_cap(
        arm.store_paths_on,
        arm.sum_cap,
        arm.delta_cap,
        arm.delta_b,
        2,
        SIGMA_C8,
        arm.store_bdp_gain,
        arm.store_cap_floor,
        arm.store_path_pool,
    )
    .expect("the pooled law is engaged at a warm dual");
    let shipped = pooled_store_cap(
        ctl.store_paths_on,
        ctl.sum_cap,
        ctl.delta_cap,
        ctl.delta_b,
        2,
        SIGMA_C8,
        ctl.store_bdp_gain,
        ctl.store_cap_floor,
        ctl.store_path_pool,
    )
    .expect("engaged");
    assert_eq!(
        shipped, 4_096,
        "the `=0` arm is not the pinned quadratic law — the displaced arm \
         must stay re-runnable and must still reproduce the 4096 pin"
    );
    assert_eq!(corrected, 3_020, "the FULL arm is not the published c8 value");
    assert!(corrected < shipped, "the FULL arm did not free the law from its ceiling");

    // 5. The one real cross-layer interaction. The trio is not inert with
    //    respect to the brake: `RWM_RELEASE_1TO1` changes how the in-flight
    //    ledger is released, and the brake's predicate is
    //    `in_flightᵢ ≥ cwndᵢ` — so the trio moves the brake's operand while
    //    the brake moves nothing the trio reads (a one-way dependency).
    //    Asserted structurally: both bits live in the same resolved policy
    //    and neither disables the other.
    assert!(
        full.release_1to1 && arm.late_brake,
        "the trio and the brake must coexist — the brake reads the ledger \
         the trio fixes, so a composition that drops either is not the arm"
    );
}

/// The routing gate for the derived-setpoint law — paper §6.1, gate
/// `RWM_DELTA_CAP`.
///
/// Measurement-discipline rule 1: a gate that resolves is not a gate that
/// routes. This asserts, on the policy the engine actually runs, that each
/// law reaches its seat, that its siblings stay where they were, and that
/// no new constant reached any mechanism: gain, knee, floor and boot are
/// all at their shipped values on every arm, so an arm differs from its
/// control in exactly one factor.
///
/// Set by field rather than through the environment: an env-mutating test
/// is process-global state in a parallel runner.
#[test]
fn the_derived_setpoint_gates_route_and_introduce_no_new_constant() {
    use crate::gates::RuntimeGates;
    use crate::net::sender_policy::SenderPolicy;
    use crate::net::{
        codel_setpoint_q, pooled_store_cap,
    };

    // The anti-drift pin, read before anything is neutralised:
    // `RWM_DELTA_CAP` resolves on by default, and this test's arms are
    // explicit on both sides so the default cannot silently turn its
    // control into its arm.
    assert!(
        RuntimeGates::resolve().delta_cap,
        "RWM_DELTA_CAP must resolve ON by default — the flip drifted back"
    );
    let base = || {
        let mut g = RuntimeGates::resolve();
        // Neutralise whatever the ambient environment carries, so this
        // asserts the law's routing and not the machine it runs on. The
        // control is the `=0` arm explicitly, not the default.
        g.delta_cap = false;
        g.derived_sweep = false;
        g.store_env_set = false;
        g.store_override = None;
        g
    };
    let resolve = |g: &RuntimeGates, h: ProtocolHint| {
        SenderPolicy::resolve(g, 1200, h, true, false, false, false)
    };

    // ── The δ-cap reaches the pooled seat at every dial point ──
    for hint in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let ctl = resolve(&base(), hint);
        let mut on = base();
        on.delta_cap = true;
        let arm = resolve(&on, hint);

        assert!(!ctl.delta_cap, "{hint:?}: the control resolved the gate ON");
        assert!(arm.delta_cap, "{hint:?}: RWM_DELTA_CAP did not reach the seat");

        // The siblings are untouched: this gate picks the value multiplier
        // and nothing else. The count multiplier (`RWM_SUM_CAP`) and the
        // brake are independent axes (the Σ's set is membership, not a dial).
        assert_eq!(arm.sum_cap, ctl.sum_cap, "{hint:?}: the count multiplier moved");
        assert_eq!(arm.late_brake, ctl.late_brake, "{hint:?}: the brake armed itself");
        assert_eq!(arm.three_term_on, ctl.three_term_on, "{hint:?}: the pool law changed");

        // No new constant reached the mechanism.
        assert!((arm.store_bdp_gain - 2.0).abs() < 1e-12, "{hint:?}: the gain moved");
        assert_eq!(arm.store_path_pool, 2048, "{hint:?}: the knee moved");
        assert_eq!(arm.store_cap_floor, ctl.store_cap_floor, "{hint:?}: the floor moved");
        assert_eq!(arm.store_boot_cap, ctl.store_boot_cap, "{hint:?}: the boot cap moved");

        // The dial reached the law, and the seat computes (1+q)·Σ.
        let q = codel_setpoint_q(arm.delta_b);
        assert!((0.05..=0.10).contains(&q), "{hint:?}: q left the derived band");
        let sigma = 1_200.0f64;
        let a = pooled_store_cap(
            true, true, arm.delta_cap, arm.delta_b, 2, sigma, arm.store_bdp_gain,
            arm.store_cap_floor, arm.store_path_pool,
        )
        .expect("engaged at a warm dual");
        let c = pooled_store_cap(
            true, true, ctl.delta_cap, ctl.delta_b, 2, sigma, ctl.store_bdp_gain,
            ctl.store_cap_floor, ctl.store_path_pool,
        )
        .expect("engaged");
        assert_eq!(a, ((1.0 + q) * sigma).ceil() as usize, "{hint:?}: not (1+q)·Σ");
        assert!(a < c, "{hint:?}: the derived multiplier did not shrink the pool");
    }

    // Bit-identical at N = 1, by construction.
    assert_eq!(
        pooled_store_cap(true, true, true, 1.0, 1, 1_200.0, 2.0, 10, 2048),
        None,
        "the pooled law engaged at N = 1"
    );

    // The δ-cap is a CAP law, scoped to `plain_dyn_cap`: it must not arm
    // on a coded seat.
    let mut d = base();
    d.delta_cap = true;
    let coded_cap =
        SenderPolicy::resolve(&d, 1200, ProtocolHint::Auto, true, true, false, false);
    assert!(!coded_cap.delta_cap, "the δ-cap escaped the plain dyn-cap scope");
}

/// The echo format pins. `[DCAP]`, `[RACK]` and `[LCW]` are what an L1
/// driver greps and what a report parser regexes; a silent format change
/// makes a battery read zeros and call them a null result. Pinned as whole
/// strings, the way `[SUMCAP]`'s own contract is.
#[test]
fn the_new_echo_lines_are_format_pinned_for_the_parsers() {
    use crate::net::{dcap_report_line, rack_report_line};
    use crate::scheduler::lcw_report_line;

    // [DCAP] — engagement, both clamp bind fractions, the counterfactual
    // change fraction, realized vs asked, and the resolved dial.
    assert_eq!(
        dcap_report_line(10, 8, 8, 0, 0, 1_213.0 * 8.0, 1_212.0 * 8.0, 0.05, 0.5, true),
        "[DCAP] on=1 eng=8/10 chg=8/8 chg_frac=1.0000 pin=0.0000 floor=0.0000 cap=1213.0 ask=1212.0 q=0.050000 b=0.5000"
    );
    // Never armed reads as eng=0/0 and must stay distinguishable from
    // armed-and-inert (eng=0/N) — both numerator and denominator print.
    assert!(dcap_report_line(0, 0, 0, 0, 0, 0.0, 0.0, 0.05, 0.5, true).contains("eng=0/0"));
    assert!(dcap_report_line(9, 0, 0, 0, 0, 0.0, 0.0, 0.05, 0.5, true).contains("eng=0/9"));

    // [RACK] — the false-alarm validation, with RACK's own class
    // bar printed beside it.
    assert_eq!(
        rack_report_line(40, 39),
        "[RACK] fa=39/40 fa_frac=0.9750 fa_class=0.0625"
    );
    // A gauge that never fired reads an explicit zero fraction.
    assert!(rack_report_line(0, 0).contains("fa=0/0 fa_frac=0.0000"));

    // [FCAUSE] — the per-cause breakdown of `fa=`'s numerator population.
    // Full contract in `tests/fcause_reachability.rs`; pinned here too
    // because this is the one test an L1 parser change is checked against.
    assert_eq!(
        crate::net::fcause_report_line(12, 430, 58, 0, 494, false),
        "[FCAUSE] gen=0 n=500 timer=12 gap_data=430 gap_refresh=58 other=0 \
         timer_frac=0.0240 gap_frac=0.9760 fired=494 unattr=6 fa_class=0.0625"
    );
    // Never fired renders `-`, so an absent reading is never poolable with
    // a measured zero.
    assert!(
        crate::net::fcause_report_line(0, 0, 0, 0, 0, false).contains("timer_frac=-"),
        "a fraction with no denominator must render `-`"
    );

    // [LCW] — the one-sided-clamp witness, and its scoreable ratio.
    crate::scheduler::lcw_reset();
    assert_eq!(
        crate::scheduler::lcw_report_line(),
        "[LCW] over_n=0 over_mass=0 loss_mass=0 rect_frac=0.0000"
    );
    assert!(lcw_report_line().starts_with("[LCW] "));
}

mod law_shape {
    use crate::net::{
        contract_stall_s, path_scaled_store_cap, three_term_store_cap, ThreeTermTerm,
        WIN_STORE_MAX,
    };

    /// Per-path anchor for the synthetic symmetric cell, in symbols. Round
    /// so every expected value below is hand-computable.
    const A: f64 = 100.0;
    /// The shipped gain (`sender_policy::resolve`).
    const GAIN: f64 = 2.0;
    /// A pool so large that `N·pool` cannot bind for any N ≤ 8 at these
    /// inputs — this is what makes the assertion below a statement about
    /// the law rather than about its ceiling.
    const POOL_INERT: usize = 1 << 20;
    /// Floor ≈ 0 (the law clamps to `[floor, ceiling]`, and `floor = 0`
    /// would still be a clamp; 1 is the smallest value that cannot bind).
    const FLOOR_INERT: usize = 1;

    /// The quadratic arm, pinned: the unclamped value is quadratic in the
    /// live-path count at symmetric inputs.
    ///
    /// `cap = gain·N·Σᵢ anchorᵢ`, and at a symmetric cell `Σ = N·A`, so
    /// `cap = gain·A·N²`. The derivation the law generalises
    /// (`Σᵢ gain·anchorᵢ`) is linear in N; the multiplier is applied to an
    /// already-summed quantity.
    ///
    /// This pins the `RWM_SUM_CAP=0` arm, not the default. It reads the law
    /// through `path_scaled_store_cap`, which is the `sum_cap = false` face
    /// of `pooled_store_cap`, so the arm under test is explicit in the call.
    /// The arm stays re-runnable, so its shape stays pinned. The shipped
    /// shape is pinned by its sibling
    /// (`sum_store_cap_value_is_linear_in_n_the_template_applied`).
    #[test]
    fn path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect() {
        let cap = |n: usize| {
            path_scaled_store_cap(true, n, n as f64 * A, GAIN, FLOOR_INERT, POOL_INERT)
                .expect("the law is engaged at N >= 2 with a positive base")
        };

        // (b) The closed form, over the whole synthetic axis. `200·N²`.
        for n in 2..=8usize {
            let expected = (GAIN * A * (n * n) as f64) as usize;
            assert_eq!(cap(n), expected, "N={n}: the law is not gain·A·N²");
            // The clamp is provably inert here — otherwise this test would
            // be measuring the ceiling (hole 1).
            assert!(cap(n) < n * POOL_INERT, "N={n}: the ceiling bound");
            assert!(cap(n) > FLOOR_INERT, "N={n}: the floor bound");
        }

        // (a/c) The ratio that names the exponent. A law linear in N would
        // read 2 at every doubling; the `=0` arm's law reads 4.
        for n in [2usize, 3, 4] {
            let r = cap(2 * n) as f64 / cap(n) as f64;
            assert!(
                (r - 4.0).abs() < 1e-9,
                "cap({}) / cap({n}) = {r}, i.e. not quadratic",
                2 * n
            );
        }

        // The absolute numbers, spelled out: the pool the law hands a
        // symmetric 8-path cell is 16× the pool it hands a symmetric dual,
        // where the summed derivation asks for 4×.
        assert_eq!(cap(2), 800);
        assert_eq!(cap(4), 3_200);
        assert_eq!(cap(8), 12_800);
        assert_eq!(cap(8) / cap(2), 16, "linear would be 4");
    }

    /// The template applied to the shipped default: under `RWM_SUM_CAP`
    /// (default on) the unclamped value is linear in the live-path count at
    /// symmetric inputs — `gain·Σ` with `Σ = N·A`, i.e. `gain·A·N`.
    ///
    /// With the quadratic sibling above, this asserts that the gate selects
    /// between a quadratic and a linear law and nothing else.
    ///
    /// Same three holes closed as the template requires: the clamps are
    /// neutralised and their inertness asserted rather than assumed, N is
    /// swept 1..8 (the axis no cell reaches, and the only place the two
    /// exponents are distinguishable), and the closed form is asserted
    /// absolutely rather than as a point.
    #[test]
    fn sum_store_cap_value_is_linear_in_n_the_template_applied() {
        use crate::net::pooled_store_cap;
        let cap = |n: usize| {
            pooled_store_cap(true, true, false, 1.0, n, n as f64 * A, GAIN, FLOOR_INERT, POOL_INERT)
                .expect("the law is engaged at N >= 2 with a positive base")
        };

        // (b) The closed form over the whole synthetic axis: `gain·A·N` =
        // `200·N`, against the shipped arm's `200·N²`.
        for n in 2..=8usize {
            let expected = (GAIN * A * n as f64) as usize;
            assert_eq!(cap(n), expected, "N={n}: the corrected law is not gain·A·N");
            assert!(cap(n) < n * POOL_INERT, "N={n}: the ceiling bound");
            assert!(cap(n) > FLOOR_INERT, "N={n}: the floor bound");
        }

        // (a/c) The ratio that names the exponent: the quadratic law reads 4
        // at a doubling and the linear law reads 2; at N ∈ {1, 2} alone the
        // two are indistinguishable.
        for n in [2usize, 3, 4] {
            let r = cap(2 * n) as f64 / cap(n) as f64;
            assert!(
                (r - 2.0).abs() < 1e-9,
                "cap({}) / cap({n}) = {r}, i.e. not linear",
                2 * n
            );
        }

        // The absolute numbers, spelled out beside the defect pin's, so the
        // two shapes are readable side by side in one place.
        assert_eq!(cap(2), 400); // shipped: 800
        assert_eq!(cap(4), 800); // shipped: 3_200
        assert_eq!(cap(8), 1_600); // shipped: 12_800
        assert_eq!(cap(8) / cap(2), 4, "quadratic would be 16");

        // The exact relationship between the arms: the deleted factor is
        // exactly N, at every N, on the
        // unclamped value. If this ever fails the gate is doing something
        // other than deleting one multiplication.
        for n in 2..=8usize {
            let shipped =
                path_scaled_store_cap(true, n, n as f64 * A, GAIN, FLOOR_INERT, POOL_INERT)
                    .expect("engaged");
            assert_eq!(
                shipped,
                cap(n) * n,
                "N={n}: the two arms do not differ by exactly the count multiplier"
            );
        }

        // N = 1 is not a case — the guard returns before the multiplier is
        // read, so both arms are `None` and the caller keeps the
        // single-path law bit-exactly. Asserted, because a by-construction
        // claim rots silently.
        for on in [false, true] {
            assert_eq!(
                pooled_store_cap(true, on, false, 1.0, 1, A, GAIN, FLOOR_INERT, POOL_INERT),
                None,
                "sum_cap={on}: the N = 1 guard must fire before the multiplier"
            );
        }
    }

    /// The composition, as four formulas: `RWM_SUM_CAP` (the count
    /// multiplier) and the Σ's path set are independent axes of one law,
    /// and their four combinations are four distinct expressions. The set
    /// decides `pipe_sum`, the gate decides the multiplier, and neither
    /// reads the other. Since plan 2b the set is not a dial (the retired
    /// `RWM_STORE_CAP_UNIFIED`): the sender always passes the membership Σ
    /// (`unified = true` below). The `unified = false` column is kept as the
    /// bound on what the retired saturation-filtered Σ cost.
    ///
    /// The test states independence as a factorisation rather than as four
    /// inequalities: the multiplier ratio is `N` whichever set is used, and
    /// the set ratio is `Σ_live/Σ_active` whichever multiplier is used. Two
    /// ratios that hold across the other axis is what "independent dials"
    /// means, and it is stronger than "the four numbers differ".
    ///
    /// Note the asymmetry the composition inherits and does not fix:
    /// `N` is `live_paths().len()` in both columns, including the retired
    /// one where the Σ ranged over `active_paths()` only (the count and the
    /// sum then ranged over different sets; plan 2b removed that mismatch
    /// by making the Σ's set membership too).
    #[test]
    fn sum_cap_and_the_unified_set_are_independent_axes_of_one_law() {
        use crate::net::pooled_store_cap;
        // A cwnd-saturated leg: live counts it, active drops it. Round
        // numbers so all four values are hand-computable.
        const N_LIVE: usize = 2;
        const SIGMA_LIVE: f64 = 1_000.0; // both legs warm
        const SIGMA_ACTIVE: f64 = 400.0; // the saturated leg omitted

        let f = |sum_cap: bool, unified: bool| {
            let sigma = if unified { SIGMA_LIVE } else { SIGMA_ACTIVE };
            pooled_store_cap(true, sum_cap, false, 1.0, N_LIVE, sigma, GAIN, FLOOR_INERT, POOL_INERT)
                .expect("engaged")
        };

        // The four formulas, absolutely. `N` is live in every one.
        assert_eq!(f(false, false), 1_600); // gain·N·Σ_active = 2·2·400
        assert_eq!(f(false, true), 4_000); // gain·N·Σ_live   = 2·2·1000
        assert_eq!(f(true, false), 800); // gain·Σ_active   = 2·400
        assert_eq!(f(true, true), 2_000); // gain·Σ_live     = 2·1000

        // All four distinct — the weak statement, kept because a
        // composition that collapses two arms into one number is how a
        // gate becomes a silent no-op.
        let vals = [f(false, false), f(false, true), f(true, false), f(true, true)];
        for i in 0..vals.len() {
            for j in (i + 1)..vals.len() {
                assert_ne!(vals[i], vals[j], "combination {i} and {j} collapse");
            }
        }

        // The factorisation — the actual independence claim.
        // 1. The multiplier's effect is ×N, whichever set is selected.
        for unified in [false, true] {
            assert_eq!(
                f(false, unified),
                f(true, unified) * N_LIVE,
                "unified={unified}: the multiplier's effect depends on the set"
            );
        }
        // 2. The set's effect is ×(Σ_live/Σ_active), whichever multiplier.
        let set_ratio = SIGMA_LIVE / SIGMA_ACTIVE;
        for sum_cap in [false, true] {
            let r = f(sum_cap, true) as f64 / f(sum_cap, false) as f64;
            assert!(
                (r - set_ratio).abs() < 1e-9,
                "sum_cap={sum_cap}: the set's effect {r} depends on the multiplier"
            );
        }
    }

    /// (c) The clamp, tested on its own: the ceiling is `N·knee`, i.e.
    /// linear in N — which is why the quadratic value is invisible on every
    /// measured cell. Once `Σ ≥ knee/gain` the realized cap is the
    /// ceiling and carries no information about the value at all, so the
    /// two must never be asserted through one another.
    #[test]
    fn path_scaled_store_cap_ceiling_is_linear_in_n() {
        const KNEE: usize = 2048; // RWM_STORE_PATH_POOL, the shipped pool
        const FLOOR: usize = 64;
        // A pool base so large that the value cannot possibly be interior.
        let cap = |n: usize| {
            path_scaled_store_cap(true, n, n as f64 * 1.0e9, GAIN, FLOOR, KNEE)
                .expect("engaged")
        };
        for n in 2..=8usize {
            assert_eq!(cap(n), n * KNEE, "N={n}: the ceiling is not N·knee");
        }
        // Linear, so a doubling reads exactly 2 — and this is the only
        // ratio a measurement of a pinned cap can report, whatever the
        // value underneath is doing.
        for n in [2usize, 3, 4] {
            assert_eq!(cap(2 * n) as f64 / cap(n) as f64, 2.0);
        }
        // The floor is the other clamp, and it is a constant in N.
        for n in 2..=8usize {
            assert_eq!(
                path_scaled_store_cap(true, n, 1e-6, GAIN, FLOOR, KNEE),
                Some(FLOOR),
                "N={n}"
            );
        }
    }

    /// (c) The clamp under the correction: the pin threshold stops being
    /// path-count-free and becomes per path — which gives the law back an
    /// operating range.
    ///
    /// ```text
    ///   quadratic  gain·N·Σ ≥ N·knee  ⟺  Σ ≥ knee/gain        (N cancels)
    ///   linear     gain·Σ   ≥ N·knee  ⟺  Σ ≥ N·knee/gain      (per path)
    /// ```
    ///
    /// The quadratic threshold's path-count-freeness is why that law
    /// degenerates: `Σ` grows with N (it is a sum over paths) while the
    /// threshold does not, so past two paths the ceiling is guaranteed and
    /// the measured, derived, per-path term never appears in any output.
    /// Pinned here as an absolute pair of crossovers rather than as a
    /// ratio, and swept over N = 2..8 so the "N cancels / N does not
    /// cancel" difference is asserted on the axis it lives on.
    ///
    /// Companion to `the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free`
    /// in `store_cap_sf_bench`, which pins the quadratic half on the bench's
    /// own constants; this asserts both halves against each other.
    #[test]
    fn the_correction_makes_the_pin_threshold_per_path_instead_of_path_count_free() {
        use crate::net::pooled_store_cap;
        const KNEE: usize = 2048;
        const FLOOR: usize = 10; // the derived floor (paper §6.1); inert here

        for n in 2..=8usize {
            let ceiling = n * KNEE;
            // The quadratic threshold: knee/gain, identical at every N.
            let shipped_pin = KNEE as f64 / GAIN;
            assert!(
                (shipped_pin - 1024.0).abs() < 1e-9,
                "N={n}: the shipped threshold moved off the path-count-free 1024"
            );
            // Just below it the quadratic law is interior; at it, pinned.
            let below = path_scaled_store_cap(true, n, shipped_pin - 1.0, GAIN, FLOOR, KNEE);
            let at = path_scaled_store_cap(true, n, shipped_pin, GAIN, FLOOR, KNEE);
            assert!(below.unwrap() < ceiling, "N={n}: shipped not interior below");
            assert_eq!(at, Some(ceiling), "N={n}: shipped not pinned at knee/gain");

            // The linear threshold: N·knee/gain, i.e. 1024 per path.
            let corrected_pin = n as f64 * KNEE as f64 / GAIN;
            assert!(
                (corrected_pin - 1024.0 * n as f64).abs() < 1e-9,
                "N={n}: the corrected threshold is not per-path"
            );
            let below = pooled_store_cap(true, true, false, 1.0, n, corrected_pin - 1.0, GAIN, FLOOR, KNEE);
            let at = pooled_store_cap(true, true, false, 1.0, n, corrected_pin, GAIN, FLOOR, KNEE);
            assert!(below.unwrap() < ceiling, "N={n}: corrected not interior below");
            assert_eq!(at, Some(ceiling), "N={n}: corrected not pinned at N·knee/gain");

            // The consequence: at the quadratic threshold — a Σ every
            // measured dual exceeds — the quadratic law is pinned while the
            // linear law is still interior, for every path count above one.
            assert_eq!(
                path_scaled_store_cap(true, n, shipped_pin, GAIN, FLOOR, KNEE),
                Some(ceiling)
            );
            assert!(
                pooled_store_cap(true, true, false, 1.0, n, shipped_pin, GAIN, FLOOR, KNEE).unwrap()
                    < ceiling,
                "N={n}: the correction does not free the law at the shipped pin threshold"
            );
        }
    }

    /// The template applied to `three_term_store_cap`: its value is linear
    /// in N at symmetric inputs — a Σ over paths with no count multiplier,
    /// and its term 3 (`2·rate_fast·skew`) vanishes
    /// identically over a symmetric set because `rtp_max == rtp_min`.
    ///
    /// Same three holes closed: the `[floor, WIN_STORE_MAX]` clamp is kept
    /// provably inert (asserted, not assumed), N is swept 1..8, and the
    /// closed form — not a point — is what is asserted.
    #[test]
    fn three_term_store_cap_value_is_linear_in_n_the_template_applied() {
        const RATE: f64 = 1_000.0; // symbols/s
        const RTPROP_S: f64 = 0.05;
        const K: f64 = 1.0; // honest clock, no standing queue
        const RHO: f64 = 1.0;
        const B: f64 = 0.5; // Realtime's δ budget; any point on the dial
        const FLOOR: usize = 64;

        let term = ThreeTermTerm { rate: RATE, rtprop_s: RTPROP_S, k: K };
        let cap = |n: usize| {
            let terms = vec![Some(term); n];
            three_term_store_cap(true, &terms, RHO, B, FLOOR)
                .expect("every synthetic path is warm")
        };

        // The per-path term, from the law's own pieces (window + slack;
        // span = 0 at a symmetric set). Absolute, hand-computable:
        // window = 1000·1·0.05 = 50; stall(ρ=1) = (9/8 + 1)·srtt =
        // 2.125·0.05 = 0.10625 s; slack = 106.25.
        let srtt_s = K * RTPROP_S;
        let single = RATE * srtt_s + RATE * contract_stall_s(RHO, B, RTPROP_S, srtt_s);
        assert!((single - 156.25).abs() < 1e-9, "per-path term drifted: {single}");

        for n in 1..=8usize {
            let (limit, window, slack, span) = cap(n);
            // (b) the closed form: Σ over paths, no count multiplier.
            assert_eq!(
                limit,
                (n as f64 * single).ceil() as usize,
                "N={n}: the three-term law is not Σ-linear in N"
            );
            // Every term individually linear, and term 3 identically 0 at
            // a symmetric set (no path-count predicate anywhere).
            assert!((window - n as f64 * RATE * srtt_s).abs() < 1e-9, "N={n} window");
            assert!((slack - n as f64 * (single - RATE * srtt_s)).abs() < 1e-9, "N={n} slack");
            assert_eq!(span, 0.0, "N={n}: skew is 0 over a symmetric set");
            // (c) the clamp is inert — asserted, so this can never silently
            // become a measurement of `WIN_STORE_MAX`.
            assert!(limit > FLOOR && limit < WIN_STORE_MAX, "N={n}: a clamp bound at {limit}");
        }

        // The ratio that names the exponent, against the quadratic law's 16.
        assert_eq!(cap(8).0 as f64 / cap(1).0 as f64, 1_250.0 / 157.0);
        let r = cap(8).0 as f64 / cap(1).0 as f64;
        assert!((r - 8.0).abs() < 0.1, "N=8 vs N=1 reads {r}, not linear-8");
    }

    /// The composed law's unclamped value, separated from its memory
    /// bound — step (c) of the template. `WIN_STORE_MAX` is the only bound
    /// left above this law, and a clamp that always binds converts a law
    /// into a constant and hides its shape from every measurement taken
    /// through it (measurement-discipline rule 18).
    ///
    /// So: the bound is shown to be reachable (it is not decorative), and
    /// shown to be a constant in N once reached (it is a resource limit,
    /// not a term — a term would scale with the Σ). Both directions,
    /// because "never binds" and "always binds" are both defects here.
    #[test]
    fn three_term_memory_bound_is_a_resource_limit_and_not_a_term_of_the_law() {
        const RTPROP_S: f64 = 0.05;
        const K: f64 = 1.0;
        const RHO: f64 = 1.0;
        const B: f64 = 0.5;
        const FLOOR: usize = 64;
        let cap_at = |rate: f64, n: usize| {
            let terms = vec![Some(ThreeTermTerm { rate, rtprop_s: RTPROP_S, k: K }); n];
            three_term_store_cap(true, &terms, RHO, B, FLOOR).expect("warm").0
        };

        // The per-path term at rate 1000 is 156.25 (the test above).
        // Interior: at N = 1..8 the law is nowhere near the memory bound,
        // so the shape assertions above are statements about the law.
        for n in 1..=8usize {
            assert!(
                cap_at(1_000.0, n) < WIN_STORE_MAX,
                "N={n}: the memory bound is binding where the law should be interior"
            );
        }

        // Reachable: drive the rate up and the bound engages. A bound that
        // could never bind would be decorative, and stating it as a
        // resource limit would be a fiction.
        assert_eq!(
            cap_at(1_000_000.0, 1),
            WIN_STORE_MAX,
            "the memory bound is unreachable — it is not the resource limit it claims to be"
        );

        // A constant in N once reached. This is the whole distinction
        // between a resource limit and a term: the law's own value is
        // Σ-linear in N (asserted above), so if this bound scaled with N
        // it would be part of the law. It does not.
        for n in 1..=8usize {
            assert_eq!(
                cap_at(1_000_000.0, n),
                WIN_STORE_MAX,
                "N={n}: the memory bound scaled with the path count — that makes it a TERM"
            );
        }

        // And the paroled floor, from the other side: it is the law's
        // lower bound, also a constant in N, and it can bind, so it is
        // pinned here rather than assumed unreachable.
        for n in 1..=8usize {
            assert_eq!(
                cap_at(1e-9, n),
                FLOOR,
                "N={n}: the paroled floor is not the law's lower bound"
            );
        }
    }
}

// ----- The composed cap law's report line --------------------------------

/// The `[CCAP]` line's shape, pinned absolutely. An L1 parser is written
/// against these keys, and the two that carry the argument are `eng=`
/// (mechanism liveness — measurement-discipline rule 1) and `mem=` (the
/// bind fraction of the only bound left above the law). A silent rename
/// would leave a battery reading zeros and calling a warm-up failure a
/// null result.
#[test]
fn the_ccap_line_reports_engagement_and_both_bind_fractions() {
    // Engaged everywhere, nothing bound, brake closed a quarter of the
    // time: the reading the composed arm is predicted to produce.
    // A symmetric cell: span 0 at every refresh, so the span field reads
    // exactly 0 and the ratio is the well-defined 0.000 that says
    // "undefined — read `span=` first", never a NaN.
    let line = ccap_report_line(
        200,
        200,
        0,
        0,
        200.0 * 3020.0,
        1_000,
        250,
        64,
        0.0,
        0.0,
        200.0 * 9_400.0,
        0.0,
    );
    assert_eq!(
        line,
        "[CCAP] eng=200/200 cap=3020.0 mem=0.0000 floor=0.0000 floor_val=64 \
         brake=250/1000 brake_frac=0.2500 span=0.0 span_sigma=0.0 \
         span_ratio=0.000 rate_fast=9400.0 spread_us=0.0"
    );

    // Configured but never engaged — a warm-up failure, and it must be
    // distinguishable from a null result. `eng=0/200` is that signature;
    // the bind fractions are 0/0 = 0.0 rather than NaN, so a parser reads
    // "undefined" from `eng` and never from a poisoned float.
    let cold = ccap_report_line(200, 0, 0, 0, 200.0 * 128.0, 1_000, 0, 64, 0.0, 0.0, 0.0, 0.0);
    assert!(cold.contains("eng=0/200"), "{cold}");
    assert!(cold.contains("mem=0.0000") && cold.contains("floor=0.0000"), "{cold}");
    // The span block on a never-engaged run: zeros, not NaNs.
    assert!(
        cold.contains("span=0.0 span_sigma=0.0 span_ratio=0.000")
            && cold.contains("rate_fast=0.0 spread_us=0.0"),
        "{cold}"
    );

    // The stop condition: the memory bound has become the law.
    let pinned = ccap_report_line(
        100,
        100,
        100,
        0,
        100.0 * 4096.0,
        500,
        500,
        64,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    assert!(pinned.contains("mem=1.0000"), "{pinned}");
    assert!(pinned.contains("cap=4096.0"), "{pinned}");

    // Zero refreshes must not divide by zero.
    assert!(ccap_report_line(0, 0, 0, 0, 0.0, 0, 0, 64, 0.0, 0.0, 0.0, 0.0).contains("cap=0.0"));
}

/// The c9h reading (two fast legs, two slow), rendered and pinned as a
/// string so the L1 parser and the renderer agree. The numbers are the
/// cell's anchors (`rate_fast` 9 400 sym/s, spread 29.88 ms ⇒ shipped
/// ≈ 281 sym), and the Σ form at c9h's two min-RTprop legs is twice that.
///
/// This is why the field set is four numbers and not one: from this line
/// alone a reader can (a) read the anchor-free ratio 2.000, (b) check the
/// 281 against the `[265, 315]` band, and (c) if it lands outside both
/// bands, attribute that to the anchors, because `rate_fast=` and
/// `spread_us=` are right there.
#[test]
fn the_ccap_span_block_renders_the_c9h_discriminator() {
    let n = 40u64;
    let (rate_fast, spread_s) = (9_400.0, 0.029_88);
    let shipped = rate_fast * spread_s; // ~= 280.9 sym
    let line = ccap_report_line(
        n,
        n,
        0,
        0,
        n as f64 * 3020.0,
        100,
        0,
        64,
        n as f64 * shipped,
        n as f64 * 2.0 * shipped,
        n as f64 * rate_fast,
        n as f64 * spread_s,
    );
    assert!(line.contains("span=280.9"), "{line}");
    assert!(line.contains("span_sigma=561.7"), "{line}");
    // The ratio — anchor-free, predicted exactly 2.000.
    assert!(line.contains("span_ratio=2.000"), "{line}");
    // The anchors, so a both-bands miss is attributable.
    assert!(line.contains("rate_fast=9400.0"), "{line}");
    assert!(line.contains("spread_us=29880.0"), "{line}");
    // And the absolute band is readable off `span=` directly.
    let span: f64 = line
        .split_whitespace()
        .find_map(|t| t.strip_prefix("span="))
        .and_then(|v| v.parse().ok())
        .expect("span= must parse as f64");
    assert!(
        (265.0..=315.0).contains(&span),
        "the anchors no longer land in C9-L3's own band: {line}"
    );
}

/// The two span forms, computed by one function, over the geometries that
/// distinguish them. This is the arithmetic half; the gauge half is the
/// rendered line above and the reachability half is
/// `tests/gauge_reachability.rs`.
#[test]
fn span_forms_separate_the_shipped_and_sigma_laws_only_where_the_contract_says() {
    let tt = |rate: f64, rtprop_ms: f64| {
        Some(ThreeTermTerm {
            rate,
            rtprop_s: rtprop_ms / 1000.0,
            k: 1.0,
        })
    };

    // 1. One path — both forms identically 0 by arithmetic, no predicate.
    let one = span_forms(&[tt(9_400.0, 8.45)]).expect("warm");
    assert_eq!(one.shipped, 0.0);
    assert_eq!(one.sigma, 0.0);
    assert_eq!(one.spread_s, 0.0);
    assert_eq!(one.rate_fast, 9_400.0);

    // 2. Symmetric N = 4 (the c9 cell) — exactly 0.
    let sym = span_forms(&[
        tt(9_400.0, 8.45),
        tt(9_400.0, 8.45),
        tt(9_400.0, 8.45),
        tt(9_400.0, 8.45),
    ])
    .expect("warm");
    assert_eq!(sym.shipped, 0.0, "C9-L1: span must be 0 at a symmetric cell");
    assert_eq!(sym.sigma, 0.0);

    // 3. The dual (N = 2, one fast leg) — the two forms agree, so a dual
    //    cannot tell them apart.
    let dual = span_forms(&[tt(9_400.0, 8.45), tt(9_400.0, 38.33)]).expect("warm");
    assert!((dual.sigma / dual.shipped - 1.0).abs() < 1e-9, "{dual:?}");

    // 4. c9h — two fast legs, two slow. The ratio is the count of
    //    min-RTprop legs: exactly 2.000, and anchor-free.
    let c9h = span_forms(&[
        tt(9_400.0, 8.45),
        tt(9_400.0, 8.45),
        tt(1_880.0, 38.33),
        tt(1_880.0, 38.33),
    ])
    .expect("warm");
    assert!(
        (c9h.sigma / c9h.shipped - 2.000).abs() < 1e-9,
        "C9-L3's anchor-free ratio: {c9h:?}"
    );
    // The absolute anchor, in its band.
    assert!(
        (265.0..=315.0).contains(&c9h.shipped),
        "C9-L3's absolute band [265, 315]: {c9h:?}"
    );
    assert_eq!(c9h.rate_fast, 9_400.0);
    assert!((c9h.spread_s - 0.029_88).abs() < 1e-9, "{c9h:?}");

    // 5. No tie predicate: nudge one "fast" leg by a microsecond — the
    //    kind of inexactness every wire measurement has — and the ratio
    //    still reads 2.000 to three decimals. A leg-counting gauge would
    //    have collapsed to 1.000 here.
    let jittered = span_forms(&[
        tt(9_400.0, 8.450),
        tt(9_400.0, 8.451),
        tt(1_880.0, 38.33),
        tt(1_880.0, 38.33),
    ])
    .expect("warm");
    assert!(
        (jittered.sigma / jittered.shipped - 2.000).abs() < 5e-4,
        "a near-tie must not collapse the discriminator: {jittered:?}"
    );

    // 6. Cold / empty sets return None on exactly the ticks the law does.
    assert_eq!(span_forms(&[]), None);
    assert_eq!(span_forms(&[tt(9_400.0, 8.45), None]), None);
}

/// The law and its gauge are one computation. `three_term_store_cap`'s
/// term 3 and `span_forms().shipped` are the same number at every
/// geometry.
#[test]
fn the_span_gauge_is_the_laws_own_term_3() {
    let tt = |rate: f64, rtprop_ms: f64| {
        Some(ThreeTermTerm {
            rate,
            rtprop_s: rtprop_ms / 1000.0,
            k: 1.2,
        })
    };
    for terms in [
        vec![tt(9_400.0, 8.45)],
        vec![tt(9_400.0, 8.45), tt(9_400.0, 38.33)],
        vec![
            tt(9_400.0, 8.45),
            tt(9_400.0, 8.45),
            tt(1_880.0, 38.33),
            tt(1_880.0, 38.33),
        ],
    ] {
        let (_, _, _, span) =
            three_term_store_cap(true, &terms, 1.0, 0.5, 64).expect("warm");
        let forms = span_forms(&terms).expect("warm");
        assert_eq!(
            span, forms.shipped,
            "the law's TERM 3 and the [CCAP] span gauge diverged"
        );
    }
}

/// The carrier and the renderer cannot drift apart.
///
/// `[CCAP]`'s tally lives in `SenderTeardownGauges` so that the counters
/// and their emission site are one object. This pins that the carrier
/// renders exactly what the pinned renderer renders for the same tally,
/// field for field and in the same order.
///
/// Reachability — that the destructor actually fires under the harness's
/// exit path — is asserted by `tests/gauge_reachability.rs`, which is the
/// half no unit test can see.
#[test]
fn the_teardown_carrier_renders_exactly_the_pinned_ccap_line() {
    let mut g = SenderTeardownGauges::new(true, 64);
    g.refreshes = 200;
    g.engaged = 190;
    g.at_mem = 3;
    g.at_floor = 7;
    g.cap_sum = 200.0 * 3020.0;
    g.brake_ticks = 1_000;
    g.brake_closed = 250;
    // The span block travels through the carrier too — and it is fed here
    // by `record_span`, the same call the refresh site makes, so a drift
    // between the accumulator and the renderer is visible.
    g.record_span(SpanForms {
        shipped: 190.0 * 280.9,
        sigma: 190.0 * 561.8,
        rate_fast: 190.0 * 9_400.0,
        spread_s: 190.0 * 0.029_88,
    });
    assert_eq!(
        g.ccap_line(),
        ccap_report_line(
            200,
            190,
            3,
            7,
            200.0 * 3020.0,
            1_000,
            250,
            64,
            190.0 * 280.9,
            190.0 * 561.8,
            190.0 * 9_400.0,
            190.0 * 0.029_88,
        ),
        "the carrier and the renderer disagree — a positional-argument drift"
    );
    assert!(
        g.ccap_line().contains("span_ratio=2.000"),
        "the carrier must render the scored ratio: {}",
        g.ccap_line()
    );
    // And the fresh carrier is the well-defined zero, not a NaN: a sender
    // that ended before its first refresh must still print a readable line.
    assert!(SenderTeardownGauges::new(true, 64)
        .ccap_line()
        .contains("eng=0/0"));
}

/// The brake's set is load-bearing (CLAUDE.md: every documented
/// divergence carries a test that bounds it).
///
/// With the composed arm's derived per-path cap — the path's own cwnd —
/// "path i is full" is `in_flight_i >= cwnd_i`, which is exactly
/// `available()_i == 0`. `active_paths()` is *active AND available() > 0*,
/// so every member of that set has `in_flight < cwnd` by construction and
/// `infl_percap_full` over it can only ever return false. A brake wired to
/// that set would resolve on, take a lock every iteration, and never
/// brake.
#[test]
fn the_composed_brake_over_the_active_set_would_be_false_by_construction() {
    // What `active_paths()` can yield under the derived cap: membership
    // requires available() > 0, i.e. in_flight < cwnd, for every member.
    // Any such vector is un-full, whatever the values are.
    for &(infl, cwnd) in &[(0u64, 100u64), (99, 100), (1, 2), (500, 501)] {
        assert!(
            !infl_percap_full(&[(infl, cwnd)]),
            "a path in active_paths() has available() > 0, so it cannot be full"
        );
    }
    assert!(
        !infl_percap_full(&[(99, 100), (0, 40), (5, 6)]),
        "no member of the active set can be full under cap_i = cwnd_i"
    );

    // Over `live_paths()` the same predicate is a real question, because
    // a live path may be saturated (available() == 0) and still live.
    assert!(
        infl_percap_full(&[(100, 100), (40, 40)]),
        "every live path at its own cwnd ⇒ the brake closes"
    );
    assert!(
        !infl_percap_full(&[(100, 100), (39, 40)]),
        "one live path below its own cwnd ⇒ the brake stays open"
    );
    // Saturated beyond the window (a retransmit can overshoot) still
    // reads full — the predicate is `>=`, not `==`.
    assert!(infl_percap_full(&[(120, 100), (41, 40)]));
}

// ----- Capacity-weighted pool (RWM_STORE_CAPW) -----------------------------

#[test]
fn capw_store_cap_not_engaged_off_single_or_unwarm() {
    // Flag off: never engaged.
    assert_eq!(
        capw_store_cap(false, &[Some(1000.0), Some(400.0)], 64, 2048),
        None
    );
    // N = 1: not engaged — the caller keeps the single-path law
    // bit-exactly (the same singles contract as RWM_STORE_PATHS).
    assert_eq!(capw_store_cap(true, &[Some(1000.0)], 64, 2048), None);
    // Anchors-not-warm fallback: any unwarm live path → None → the
    // caller keeps the configured pooled law (path-scaled / single-path)
    // until every anchor is live — a partial sum would under-provision
    // the unwarm path's share of the shared pool.
    assert_eq!(capw_store_cap(true, &[Some(1000.0), None], 64, 2048), None);
    assert_eq!(capw_store_cap(true, &[None, None], 64, 2048), None);
    assert_eq!(
        capw_store_cap(true, &[Some(1000.0), Some(0.0)], 64, 2048),
        None
    );
    assert_eq!(capw_store_cap(true, &[], 64, 2048), None);
}

#[test]
fn capw_store_cap_symmetric_is_n_times_single() {
    // The c7 degenerate: N identical paths → pool = N × the single-path
    // honest term (≈ N×(single pool) — symmetric cells preserved).
    let single = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 2.0).unwrap();
    // 83.2·(1.5+1) + 10 400·1·0.1 = 208 + 1040 = 1248.
    assert!((single - 1248.0).abs() < 1e-6);
    assert_eq!(
        capw_store_cap(true, &[Some(single), Some(single)], 64, 2048),
        Some(2496)
    );
    // Three symmetric paths: 3× (ceiling 3×2048 not binding).
    assert_eq!(
        capw_store_cap(true, &[Some(single); 3].to_vec(), 64, 2048),
        Some(3744)
    );
}

#[test]
fn capw_store_cap_asymmetric_weights_by_capacity_not_path_count() {
    // The c8 shape (the law's target cell): a c2-class fast path
    // (10 400 sym/s, RTprop 8 ms → anchor 83.2) + a c3-class slow path
    // (2000 sym/s, RTprop 40 ms → anchor 80). Each earns its own pipe +
    // recovery round — the slow path's 1/5 rate earns ~1/3 of the fast
    // term (its longer RTprop partially offsets), not the equal ×knee
    // share the path-count law grants.
    let fast = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 2.0).unwrap(); // 1248
    let slow = honest_store_cap(Some(80.0), Some(2_000.0), 1.5, 2.0).unwrap(); // 200+200=400
    assert!((slow - 400.0).abs() < 1e-6);
    let pool = capw_store_cap(true, &[Some(fast), Some(slow)], 64, 2048).unwrap();
    assert_eq!(pool, 1648);
    // Strictly between the single-path 1024 latch (fast path
    // under-provisioned) and the
    // path-scaled N×2048 = 4096 (slow path over-provisioned).
    assert!(pool > 1024 && pool < 4096);
    // Capacity weighting: the slow path's contribution is its own term,
    // ~24% of the pool — not the 50% the count-scaled ceiling implies.
    assert!((slow / (fast + slow) - 0.243).abs() < 0.01);
}

#[test]
fn capw_store_cap_overread_anchors_clamp_to_the_path_scaled_ceiling() {
    // The ack-interval anchor over-reads several-fold: inflated terms
    // clamp at the N×knee ceiling — the path-scaled degenerate (without
    // honest anchors, `RWM_PLAIN_RS=1`, the law cannot differentiate).
    // Floor guards transiently-tiny terms.
    assert_eq!(
        capw_store_cap(true, &[Some(6.0 * 1248.0), Some(6.0 * 400.0)], 64, 2048),
        Some(4096)
    );
    assert_eq!(
        capw_store_cap(true, &[Some(1.0), Some(2.0)], 64, 2048),
        Some(64)
    );
}

// ----- Pool-anchor honest dual-store law (RWM_POOL_ANCHOR) -----------------

/// At the law level: with a c7-class true send rate (≈ 8.9k sym/s/path,
/// RTprop 8 ms, K ≈ 2) the honest send-anchor pool sizes to the ~2.2k
/// residence+runway class, while an inflated ack-interval anchor (the
/// burst-peak over-read) drives the path-scaled law to its 4096 clamp —
/// standing-queue headroom. Same pure functions the engine branch calls
/// (honest_store_cap terms → capw_store_cap pool).
#[test]
fn pool_anchor_honest_terms_bound_the_dual_pool_where_the_legacy_law_clamps() {
    let sr = 8_900.0; // true per-path send rate, sym/s (c7 ≈ 85 Mbit @1200B)
    let rtp = 0.008; // c2-class RTprop
    let term = honest_store_cap(Some(sr * rtp), Some(sr), 2.0, 2.0).unwrap();
    // cap_i = 71.2·(2+1) + 8900·1·0.1 ≈ 1104 — the 1024-per-path class.
    assert!((1000.0..1300.0).contains(&term), "cap_i class, got {term}");
    let pool = capw_store_cap(true, &[Some(term), Some(term)], 64, 2048).unwrap();
    assert!((1500..3000).contains(&pool), "Σ pool class, got {pool}");
    // An inflated ack-interval anchor: Σ bdp ≈ 2 × 330k × 8 ms ⇒ the
    // path-scaled law rails at the N×knee ceiling.
    let inflated_bdp_sum = 2.0 * 330_000.0 * rtp;
    assert_eq!(
        path_scaled_store_cap(true, 2, inflated_bdp_sum, 2.0, 64, 2048),
        Some(4096)
    );
    assert!(pool < 4096, "the honest pool removes the clamp headroom");
    // N = 1 bit-exactness: one term never engages the pooled law — the
    // caller's single-path law runs verbatim.
    assert_eq!(capw_store_cap(true, &[Some(term)], 64, 2048), None);
    // Warm-up: any unwarm live path defers to the configured fallback.
    assert_eq!(capw_store_cap(true, &[Some(term), None], 64, 2048), None);
}

// ----- Honest floor-clock caps --------------------------------------------

#[test]
fn echo_ratio_min_is_self_queue_proof_and_window_expires() {
    let mut e = EchoRatioMin::new(5_000_000);
    // Before any sample: K = 1 (the floor-law degenerate).
    assert_eq!(e.k(), 1.0);
    // Early unloaded samples set the honest drain-clock ratio.
    assert!((e.observe(1.5, 1_000_000) - 1.5).abs() < 1e-9);
    // Self-queue inflation (dwell → echo → ratio) cannot raise the min —
    // the c8 parking spiral has no handle on this statistic.
    assert!((e.observe(4.0, 2_000_000) - 1.5).abs() < 1e-9);
    assert!((e.observe(8.0, 3_000_000) - 1.5).abs() < 1e-9);
    // Degenerate samples are inert (NaN) or clamped (≥ 1: a smoothed
    // echo transiently under the windowed-min floor is clock noise).
    assert!((e.observe(f64::NAN, 3_500_000) - 1.5).abs() < 1e-9);
    let mut e2 = EchoRatioMin::new(5_000_000);
    assert_eq!(e2.observe(0.8, 1_000_000), 1.0);
    // Anchor-hygiene rule 3 (ADR-0061) — the window expires: a stale unloaded read
    // rolls out after two half-windows and the ratio re-measures.
    assert!((e.observe(3.0, 6_500_000) - 1.5).abs() < 1e-9); // prev bucket holds 1.5
    assert!((e.observe(3.5, 11_600_000) - 3.0).abs() < 1e-9); // 1.5 expired
}

#[test]
fn echo_ratio_seed_identity_sample_is_discarded_not_latched() {
    // At the estimator seed instant srtt ≡ min_rtt (shared seeding), so
    // ratio ≡ 1.0 — feeding it would latch the windowed min at 1.0 for a
    // whole window. The seed-identity sample must be discarded.
    let mut e = EchoRatioMin::new(5_000_000);
    let ms = |m: u64| std::time::Duration::from_millis(m);
    // Seed instant: srtt == RTprop bit-equal → no sample, K stays 1.0
    // as the default (not as a latched measurement).
    assert_eq!(e.observe_srtt_over_rtprop(ms(8), Some(ms(8)), 1_000_000), 1.0);
    // A real measurement then sets the min — it was not latched at 1.0.
    assert!(
        (e.observe_srtt_over_rtprop(ms(16), Some(ms(8)), 2_000_000) - 2.0).abs()
            < 1e-9
    );
    // Warm-up (no RTprop) observes nothing.
    assert!(
        (e.observe_srtt_over_rtprop(ms(16), None, 3_000_000) - 2.0).abs() < 1e-9
    );
}

#[test]
fn honest_store_cap_is_residence_plus_recovery_runway() {
    // Derived, not tuned: cap_i = anchor_i·(K_i + gain − 1) +
    // rate_i·(gain−1)·R — residence (Little's law on the unloaded
    // drain clock) + (gain−1) recovery rounds on the recovery engine's
    // clock (R = 100 ms, the hole-refresh/tail-sweep cadence bound)
    // plus the retransmit flight (the anchor term's second round).
    // c2-like at the measured cell clocks (rate 10 400 sym/s, RTprop
    // 8 ms → anchor 83.2; K = 2): 83.2×3 + 10 400×0.1 = 1289.6.
    let c = honest_store_cap(Some(10_400.0 * 0.008), Some(10_400.0), 2.0, 2.0)
        .unwrap();
    assert!((c - 1289.6).abs() < 1e-6);
    // K clamps at 1 from below (K < 1 is clock noise).
    assert_eq!(
        honest_store_cap(Some(83.2), Some(10_400.0), 0.5, 2.0),
        honest_store_cap(Some(83.2), Some(10_400.0), 1.0, 2.0)
    );
    // gain = 1: pure residence, zero runway — the R term vanishes with
    // (gain−1), never a negative runway.
    let g1 = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 1.0).unwrap();
    assert!((g1 - 83.2 * 1.5).abs() < 1e-9);
    let g05 = honest_store_cap(Some(83.2), Some(10_400.0), 1.5, 0.5).unwrap();
    assert!((g05 - 83.2 * 1.5).abs() < 1e-9);
    // Warm-up (no anchor / no rate): None — the caller keeps its
    // warm-up share.
    assert_eq!(honest_store_cap(None, Some(10_400.0), 2.0, 2.0), None);
    assert_eq!(honest_store_cap(Some(83.2), None, 2.0, 2.0), None);
    assert_eq!(honest_store_cap(Some(0.0), Some(10_400.0), 2.0, 2.0), None);
}

// ── The three-term law ────────────────────────────────────────────────

/// A warm term at the bench's own axes: `k = srtt/RTprop` exactly, so
/// these numbers are the ones `tests/slack_bench.rs` computes.
fn tt(rate: f64, rtprop_ms: f64, srtt_ms: f64) -> Option<ThreeTermTerm> {
    Some(ThreeTermTerm {
        rate,
        rtprop_s: rtprop_ms / 1e3,
        k: srtt_ms / rtprop_ms,
    })
}

/// The δ dial's named points — a dial, read once, in one place (paper
/// §5.4).
///
/// Bit-exact, not approximate: `b` is `span_horizon_b(delta_price(hint))`,
/// so the three shipped numbers are the output of `2^(−½·log₁₀(δ/0.5))` on
/// a libm. If that composition misses ½ / 1 / 2 by one ULP on some target,
/// this assertion fails the build rather than letting a step ship; the
/// fallback is the exact-by-construction `b = exp2(½·log₁₀ ζ)` over the
/// enum's own ζ literals. The `log₁₀(δ/δ_Auto)` form is exact here because
/// δ ∈ {50, 0.5, 0.005} are themselves the exact f64 quotients `0.5/ζ`,
/// `δ/0.5` is an exact power-of-two scaling, and `log₁₀` of {100, 1, 0.01}
/// returns exactly {2, 0, −2}.
#[test]
fn delta_budget_b_is_the_dial_not_a_mode() {
    assert_eq!(delta_budget_b(ProtocolHint::Realtime), 0.5);
    assert_eq!(delta_budget_b(ProtocolHint::Auto), 1.0);
    assert_eq!(delta_budget_b(ProtocolHint::Bulk), 2.0);
    // The hint names a δ, and it names the same δ the Copa mapping does —
    // one map, one seat. The dial's three named points, bit-exactly.
    assert_eq!(delta_price(ProtocolHint::Realtime), 50.0);
    assert_eq!(delta_price(ProtocolHint::Auto), 0.5);
    assert_eq!(delta_price(ProtocolHint::Bulk), 0.005);
    // `b(hint)` is `b_of(δ(hint))` — the composition, not a coincidence.
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        assert_eq!(delta_budget_b(h), delta_budget_b_of(delta_price(h)));
    }

    // The only law b enters is continuous and monotone in it, through
    // every named point — no step at a preset (CLAUDE.md).
    let mut prev = 0u64;
    for i in 0..=200 {
        let b = i as f64 / 100.0; // sweeps 0 → 2, hitting ½, 1, 2 exactly
        let d = shed_deadline_us(b, 20_000);
        assert!(d >= prev, "D(b) stepped down at b={b}");
        prev = d;
    }
    assert_eq!(shed_deadline_us(0.5, 20_000), 10_000);
    assert_eq!(shed_deadline_us(2.0, 20_000), 40_000);

    // ── The continuity gate on δ itself ──────────────────────────────
    // Sweep the dial log-uniformly over its own span [δ_Bulk, δ_Realtime]
    // and assert the two shape properties the paper's b(δ) claims.
    let (lo, hi) = (0.005f64, 50.0f64);
    const N: usize = 500;
    let mut prev_b = f64::INFINITY;
    let mut prev_d = u64::MAX;
    for i in 0..=N {
        let d = lo * (hi / lo).powf(i as f64 / N as f64);
        let b = delta_budget_b_of(d);
        assert!(
            (0.5..=2.0).contains(&b),
            "δ={d}: b={b} left the dial's own range"
        );
        assert!(
            b < prev_b,
            "b(δ) must be STRICTLY decreasing: b({d}) = {b} did not fall below {prev_b}"
        );
        // D(δ) = min(b·RTprop, 2·RTprop) at the c2 RTprop: non-increasing.
        let dl = shed_deadline_us(b, 8_000);
        assert!(dl <= prev_d, "D(δ) stepped UP at δ={d}");
        prev_b = b;
        prev_d = dl;
    }
    // ±2 % nudges at every preset. A behaviour step across a preset is a
    // defect even if each side is individually correct (CLAUDE.md), and
    // the bound is absolute in b, not a ratio that a small b could hide
    // inside: a 2 % move in δ is a 0.0043-decade move in log δ, so
    // |Δb| < 0.01 for every b on this dial.
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let d0 = delta_price(h);
        let b0 = delta_budget_b_of(d0);
        for f in [0.98f64, 1.02] {
            let b1 = delta_budget_b_of(d0 * f);
            assert!(
                (b1 - b0).abs() < 0.01,
                "{h:?}: a {f}× nudge of δ stepped b from {b0} to {b1}"
            );
        }
    }
    // And the shed deadline at every preset, over the bench's own RTprop
    // grid — the c2/c3 legs plus the range between and around them —
    // bit-exact against b ∈ {½, 1, 2}.
    for (h, b_old) in [
        (ProtocolHint::Realtime, 0.5f64),
        (ProtocolHint::Auto, 1.0),
        (ProtocolHint::Bulk, 2.0),
    ] {
        for rtprop_us in [
            1_000u64, 4_000, 8_000, /* c2 */ 20_000, 40_000, 60_000, /* c3 */
            100_000, 250_000,
        ] {
            assert_eq!(
                shed_deadline_us(delta_budget_b(h), rtprop_us),
                shed_deadline_us(b_old, rtprop_us),
                "{h:?} at RTprop {rtprop_us} µs: D moved when b's SHAPE changed"
            );
        }
    }
}

/// ζ and δ are one involution, bit-exactly. The rate controller's
/// effective tail target reads `ζ(δ(hint))`, which equals
/// `hint.tail_loss_scale()` only if the round trip
/// `ζ → δ = 0.5/ζ → ζ = 0.5/δ` is exact at all three preset ζ. It is
/// (0.01, 1, 100 are all exactly representable ratios of 0.5).
#[test]
fn zeta_of_the_hints_delta_is_the_hints_own_scale() {
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        assert_eq!(
            raptorpath_math::zeta_of_delta(delta_price(h)),
            h.tail_loss_scale(),
            "{h:?}: ζ(δ(hint)) is not the hint's own declared price ratio"
        );
    }
    // β, the rate mix's weight: 0 at both the Realtime and Auto ends
    // (the clamp) and 1 at Bulk (x/x) — all three exactly, which is what
    // makes the mix byte-identical at the presets.
    assert_eq!(
        raptorpath_math::bulkness_of_delta(delta_price(ProtocolHint::Realtime)),
        0.0
    );
    assert_eq!(
        raptorpath_math::bulkness_of_delta(delta_price(ProtocolHint::Auto)),
        0.0
    );
    assert_eq!(
        raptorpath_math::bulkness_of_delta(delta_price(ProtocolHint::Bulk)),
        1.0
    );
}

/// Absolute arithmetic on the composed law — every number hand-computable
/// from a rate and a time, and continuous in ρ with both stall terms
/// always evaluated (CLAUDE.md: no mode bit, no threshold that selects a
/// formula).
#[test]
fn three_term_law_is_arithmetic_and_continuous() {
    // ── The c2 single, ρ = 1, b = ½: RTprop 8 ms, wireQ 4 ms ⇒ K = 1.5.
    //   window = 10 400 × 12 ms                     = 124.8
    //   slack  = 10 400 × 17/8 × 12 ms              = 265.2
    //   span   = 2 × 10 400 × 0                     =   0
    let (cap, w, sl, sp) =
        three_term_store_cap(true, &[tt(10_400.0, 8.0, 12.0)], 1.0, 0.5, 64).unwrap();
    assert!((w - 124.8).abs() < 1e-9, "window {w}");
    assert!((sl - 265.2).abs() < 1e-9, "slack {sl}");
    assert_eq!(sp, 0.0, "ONE path ⇒ the span term is identically zero");
    // 124.8 + 265.2 = 390.000000000000057 in f64, and a CAP ceils
    // (it must cover), so the shipped integer is 391. The ±1-symbol
    // ceil quantum is pinned here rather than hidden by a tolerance.
    assert_eq!(cap, 391, "ceil(124.8 + 265.2 + 0)");

    // ρ = 0 (fully sheddable): the stall collapses to the span law's own
    // D(δ) = b·RTprop = 4 ms — `shed_deadline_us`, not a second constant.
    let (_, w0, sl0, _) =
        three_term_store_cap(true, &[tt(10_400.0, 8.0, 12.0)], 0.0, 0.5, 64).unwrap();
    assert!((w0 - 124.8).abs() < 1e-9, "the window term does not move with ρ");
    assert!((sl0 - 41.6).abs() < 1e-9, "10 400 × 4 ms = 41.6, got {sl0}");
    // A straight line in ρ through 21 points — both terms always
    // computed, nothing switches at any value of the dial.
    let mid =
        three_term_store_cap(true, &[tt(10_400.0, 8.0, 12.0)], 0.5, 0.5, 64).unwrap().2;
    assert!((mid - (sl0 + 265.2) / 2.0).abs() < 1e-9, "midpoint {mid}");
    let mut prev = -1.0;
    for i in 0..=20 {
        let rho = i as f64 / 20.0;
        let s =
            three_term_store_cap(true, &[tt(10_400.0, 8.0, 12.0)], rho, 0.5, 64).unwrap().2;
        let want = (1.0 - rho) * 41.6 + rho * 265.2;
        assert!((s - want).abs() < 1e-9, "ρ={rho}: {s} vs {want}");
        assert!(s >= prev, "slack(ρ) stepped down at ρ={rho}");
        prev = s;
    }

    // ── The c8 geometry, both paths: c2 (10 400 sym/s, RTprop 8 ms,
    // srtt 12 ms) + c3 (2 000 sym/s, RTprop 60 ms, srtt 64 ms).
    //   window = 10 400×12 ms + 2 000×64 ms          = 124.8 + 128.0
    //   slack  = 10 400×25.5 ms + 2 000×136 ms       = 265.2 + 272.0
    //   span   = 2 × 10 400 × (60−8)/2 ms            = 540.8
    let c8 = [tt(10_400.0, 8.0, 12.0), tt(2_000.0, 60.0, 64.0)];
    let (cap8, w8, sl8, sp8) = three_term_store_cap(true, &c8, 1.0, 0.5, 64).unwrap();
    assert!((w8 - 252.8).abs() < 1e-9, "window {w8}");
    assert!((sl8 - 537.2).abs() < 1e-9, "slack {sl8}");
    // The sender-retention span: 541, against an independently measured
    // good pin of 508 (+6.5 %), and ×7.57 below the 4096 path-scaled cap.
    assert!((sp8 - 540.8).abs() < 1e-9, "span {sp8} must be PS6's 540.8");
    assert_eq!(cap8, 1331, "252.8 + 537.2 + 540.8 = 1330.8 ⇒ 1331");
    // Path order is not a parameter: the law is a sum plus a spread.
    let rev = [c8[1], c8[0]];
    assert_eq!(three_term_store_cap(true, &rev, 1.0, 0.5, 64).unwrap().0, cap8);

    // Off, and warm-up, both return None — the caller's existing chain
    // then runs verbatim (the gate's off-value property, in the law).
    assert_eq!(three_term_store_cap(false, &c8, 1.0, 0.5, 64), None);
    assert_eq!(three_term_store_cap(true, &[], 1.0, 0.5, 64), None);
    assert_eq!(
        three_term_store_cap(true, &[c8[0], None], 1.0, 0.5, 64),
        None,
        "one cold path ⇒ no partial sum (the capw rule)"
    );
    // The clamp is memory, not law: an absurd pipe stops at WIN_STORE_MAX.
    assert_eq!(
        three_term_store_cap(true, &[tt(10_000_000.0, 8.0, 12.0)], 1.0, 0.5, 64).unwrap().0,
        WIN_STORE_MAX
    );
}

/// No topology branch. Sweep the path count and the skew, and assert
/// there is no step anywhere: the span term is identically 0 at one path
/// and at any number of paths with equal RTprop, and it approaches 0
/// continuously
/// as the skew shrinks. No `if n == 1` produces this — the arithmetic
/// does, because `max − min` over one element is zero.
#[test]
fn three_term_span_vanishes_continuously_as_skew_goes_to_zero() {
    let base_ms = 8.0;
    let k = 1.5;
    let mk = |n: usize, skew_ms: f64| -> (usize, f64, f64, f64) {
        // n paths, all at the same rate; one of them lagging by the
        // skew. n = 1 ⇒ the lagging path is the only path.
        let terms: Vec<Option<ThreeTermTerm>> = (0..n)
            .map(|i| {
                let rtp = if i + 1 == n { base_ms + 2.0 * skew_ms } else { base_ms };
                Some(ThreeTermTerm { rate: 10_400.0, rtprop_s: rtp / 1e3, k })
            })
            .collect();
        three_term_store_cap(true, &terms, 1.0, 0.5, 64).unwrap()
    };

    // (a) Path-count sweep at zero skew: the span term is 0 at every
    // path count, so nothing about the limit keys on topology.
    for n in 1..=6 {
        let (_, _, _, span) = mk(n, 0.0);
        assert_eq!(span, 0.0, "n={n}: zero skew must give a zero span term");
    }
    // A single path is the n = 1 case of the same expression — not a
    // special case, and not reachable by any branch.
    assert_eq!(mk(1, 40.0).3, 0.0, "one path has no skew to be skewed BY");

    // (b) Skew → 0 at N = 2: linear, zero intercept, no step. The span
    // is `2 · rate_fast · skew` exactly at every point, and the limit's
    // difference from the zero-skew limit vanishes with the skew.
    let (_, w0, sl0, _) = mk(2, 0.0);
    let zero_limit = (w0 + sl0).ceil() as usize;
    assert_eq!(mk(2, 0.0).0, zero_limit);
    // 20 ms of skew down to 0 in 50 µs steps. A step at any of these is
    // a defect even if both sides are individually correct, so the test
    // walks the whole sweep and bounds every adjacent difference. (The
    // lagging path's own window and slack terms move with its RTprop
    // too — that is a real dependence on a real signal, and it is
    // included in the bound rather than excluded from the sweep.)
    let mut prev_span = -1.0;
    let mut prev_limit: Option<usize> = None;
    for i in (0..=400).rev() {
        let skew_ms = i as f64 / 20.0;
        let (limit, _, _, span) = mk(2, skew_ms);
        assert!(
            (span - 2.0 * 10_400.0 * skew_ms / 1e3).abs() < 1e-9,
            "skew {skew_ms} ms: span {span} is not 2·rate_fast·skew"
        );
        assert!(span >= 0.0 && (prev_span < 0.0 || span <= prev_span + 1e-9));
        prev_span = span;
        // No step: 50 µs of skew is 1.04 symbols of span plus 4.9 of
        // window+slack on the lagging path's own clock — under 8, at
        // every one of the 400 positions, including the last one into
        // zero skew where a topology branch would have shown up.
        if let Some(p) = prev_limit {
            assert!(
                limit.abs_diff(p) <= 8,
                "skew {skew_ms} ms: limit stepped {p} → {limit}"
            );
        }
        prev_limit = Some(limit);
    }
    // The sweep ended AT zero skew, and it arrived there continuously.
    assert_eq!(prev_limit, Some(zero_limit));
    assert_eq!(mk(2, 0.0).3, 0.0);
    // And the first nudge off zero moves the limit by a handful of
    // symbols, not by a cliff: 50 µs of skew is 1.04 symbols of span
    // plus 4.9 of the lagging path's own window + slack.
    assert!(mk(2, 0.05).0.abs_diff(zero_limit) <= 8);
    assert!((mk(2, 0.05).3 - 1.04).abs() < 1e-9, "the span's own first step");
}

/// The store dwell cannot walk back into the law's own argument, so the
/// closed loop's fixed point is reached in one evaluation. Bounded, not
/// described: an inflated echo sample can only raise a windowed min's
/// members, never the min itself, while one honest sample is in window —
/// and the bound on "in window" is stated.
#[test]
fn three_term_law_closes_the_dwell_loop_in_one_evaluation() {
    let mut ks: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let honest = ThreeTermPath {
        id: 1,
        rate: Some(10_400.0),
        srtt: Duration::from_millis(12), // RTprop 8 ms + a 4 ms wire queue
        rtprop: Some(Duration::from_millis(8)),
        k_raw: None,
    };
    let t0 = 1_000_000u64;
    let first = three_term_terms(&mut ks, &[Some(honest)], t0);
    let cap0 = three_term_store_cap(true, &first, 1.0, 0.5, 64).unwrap();
    assert!((first[0].unwrap().k - 1.5).abs() < 1e-12, "K = 12/8");
    assert_eq!(cap0.0, 391);

    // Now the store fills and the app-echo RTT balloons to 200 ms (the
    // open-loop argument). The law does not move, at any point over the
    // window.
    let dwelled = ThreeTermPath { srtt: Duration::from_millis(200), ..honest };
    for t in 1..=9u64 {
        let now = t0 + t * 1_000_000;
        let terms = three_term_terms(&mut ks, &[Some(dwelled)], now);
        let cap = three_term_store_cap(true, &terms, 1.0, 0.5, 64).unwrap();
        assert_eq!(cap, cap0, "the dwell re-entered the law at t+{t}s");
    }
    // The residual, bounded: K's memory is two `PERCAP_K_HALF_WINDOW_US`
    // half-buckets. A dwell sustained past that does move the law — and
    // the bound on how far is the dwell ratio itself, so this is the
    // law's stated limitation and not an invariant.
    let far = t0 + 3 * 2 * PERCAP_K_HALF_WINDOW_US;
    let terms = three_term_terms(&mut ks, &[Some(dwelled)], far);
    let k_far = terms[0].unwrap().k;
    assert!((k_far - 25.0).abs() < 1e-9, "200/8 = 25, got {k_far}");
    // …and even then the memory clamp bounds the damage, which is the
    // second reason the loop cannot run away in the engine.
    assert_eq!(
        three_term_store_cap(true, &terms, 1.0, 0.5, 64).unwrap().0,
        WIN_STORE_MAX
    );
}

/// `RWM_HONEST_K`: the K override is one formula —
/// `k_raw.unwrap_or(smoothed)` — so (a) `k_raw = None` (the shipped
/// default: the gate resolves off and `PathState::k_raw()` returns None)
/// is byte-identical to the smoothed law, (b) `Some(k)` substitutes the
/// raw-fed floor into the unchanged law, and (c) the smoothed tracker's
/// window state is observed identically either way (the A/B isolates the
/// K source, nothing else).
#[test]
fn honest_inputs_k_raw_override_is_one_formula_and_off_is_byte_identical() {
    let mk = |k_raw: Option<f64>| HonestCapPath {
        id: 1,
        anchor: Some(83.2),
        rate: Some(10_400.0),
        srtt: Duration::from_millis(12),
        rtprop: Some(Duration::from_millis(8)),
        k_raw,
    };
    // (a) Off ⇒ byte-identical to the smoothed law.
    let mut ks_off: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let off = honest_cap_terms(&mut ks_off, &[Some(mk(None))], 1_000_000, 2.0);
    let legacy_k = 12.0 / 8.0;
    let expect_legacy = honest_store_cap(Some(83.2), Some(10_400.0), legacy_k, 2.0);
    assert_eq!(off[0], expect_legacy, "None ⇒ the legacy K, bit-exactly");

    // (b) On ⇒ the same law at the raw floor.
    let mut ks_on: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let on = honest_cap_terms(&mut ks_on, &[Some(mk(Some(1.0)))], 1_000_000, 2.0);
    let expect_raw = honest_store_cap(Some(83.2), Some(10_400.0), 1.0, 2.0);
    assert_eq!(on[0], expect_raw, "Some(k) ⇒ the same law at the raw K");
    assert!(on[0].unwrap() < off[0].unwrap(), "the floor is below the smoothed read");

    // (c) The smoothed tracker was fed identically on both arms.
    assert_eq!(ks_off.get(&1).map(|e| e.k()), ks_on.get(&1).map(|e| e.k()));

    // Same law through the three-term collector: the window term's K.
    let tt = |k_raw: Option<f64>| ThreeTermPath {
        id: 2,
        rate: Some(10_400.0),
        srtt: Duration::from_millis(12),
        rtprop: Some(Duration::from_millis(8)),
        k_raw,
    };
    let mut ks: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let t_off = three_term_terms(&mut ks, &[Some(tt(None))], 1_000_000);
    assert!((t_off[0].unwrap().k - legacy_k).abs() < 1e-12);
    let t_on = three_term_terms(&mut ks, &[Some(tt(Some(1.0)))], 1_000_000);
    assert!((t_on[0].unwrap().k - 1.0).abs() < 1e-12);
}

#[test]
fn honest_caps_shallow_account_sits_at_recovery_budget_not_knee() {
    // On honest anchors. Deep c2-like path (rate 10 400, RTprop 8 ms,
    // K 1.5): cap = 1248 — differentiated, inside the knee (an over-read
    // anchor knee-clamps to 2048).
    let fast = (honest_store_cap(Some(10_400.0 * 0.008), Some(10_400.0), 1.5, 2.0)
        .unwrap()
        .ceil() as usize)
        .clamp(64, 2048);
    assert_eq!(fast, 1248);
    assert!(fast < 2048, "fast cap must be derived, not knee-clamped");
    // Shallow c8-slow-class path (rate 1 954, RTprop 60 ms → anchor
    // 117.2; K 1.3): cap = 466 ≈ a measured good pin (508 outstanding,
    // 0.26 s dwell) — its recovery budget, not the 2048 knee (≈1 s parked
    // dwell). The own-pick parking channel is closed by construction.
    let slow = (honest_store_cap(Some(1_954.0 * 0.060), Some(1_954.0), 1.3, 2.0)
        .unwrap()
        .ceil() as usize)
        .clamp(64, 2048);
    assert_eq!(slow, 466);
    assert!(slow < 2048 / 4, "slow cap must sit at its recovery budget, not the knee");
    // Per-path independence still holds: deepening the fast pipe does
    // not move the slow cap.
    let fast2 = (honest_store_cap(Some(20_800.0 * 0.008), Some(20_800.0), 1.5, 2.0)
        .unwrap()
        .ceil() as usize)
        .clamp(64, 2048);
    assert!(fast2 > fast);
    assert_eq!(
        (honest_store_cap(Some(1_954.0 * 0.060), Some(1_954.0), 1.3, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, 2048),
        slow
    );
}

#[test]
fn honest_anchor_sum_cap_preserves_sc2_throughput_headroom() {
    // At the law level: on honest anchors the floor law's cap falls from
    // the 1024 latch to gain·anchor_honest ≈ 150–170 at a true 8-ms
    // RTprop — a 100-Mbit pipe whose recovery round is ~12× its wire
    // round trip.
    let rate: f64 = 10_400.0;
    let anchor: f64 = rate * 0.008;
    let store_max = 1024usize;
    // The floor law on honest anchors (the RWM_HONEST_CAP=0 control
    // arm): gain·anchor = 167 ≪ 1024.
    let floor_law = ((2.0 * anchor).ceil() as usize).clamp(64, store_max);
    assert_eq!(floor_law, 167);
    // The honest law at measured drain clocks (K ≈ 2): 1290 → latches
    // the 1024 store — headroom supplied by the engine's own recovery
    // cadence.
    let honest = (honest_store_cap(Some(anchor), Some(rate), 2.0, 2.0)
        .unwrap()
        .ceil() as usize)
        .clamp(64, store_max);
    assert_eq!(honest, store_max);
    // Monotone: for any measured K ≥ 1 the honest cap strictly exceeds
    // the floor law — honest anchors can widen but never shrink
    // the single-path window relative to the control.
    for k in [1.0, 1.2, 1.7, 2.5, 4.0] {
        let c = (honest_store_cap(Some(anchor), Some(rate), k, 2.0)
            .unwrap()
            .ceil() as usize)
            .clamp(64, store_max);
        assert!(c > floor_law);
    }
}

/// The derived pipeline depth M* = ceil(rate·2·SRTT/G)+1 — A* =
/// clamp(D·rate, 1, W) quantized to generations (paper §5.3) — covers
/// BDP + one deficit round, clamps to the fixed 2 on cold start, and to
/// GEN_PIPE_MAX_GENS at the top.
#[test]
fn gen_pipe_depth_covers_bdp_plus_one_deficit_round() {
    // Cold start (no rate / no srtt sample) → the fixed depth 2.
    assert_eq!(gen_pipe_depth(0.0, 0.016, 384), 2);
    assert_eq!(gen_pipe_depth(1500.0, 0.0, 384), 2);
    // c2-class: rate 1500 sym/s, SRTT 16 ms → D·rate = 48 sym ≪ G ⇒
    // ceil(48/384)+1 = 2 — a small-BDP link needs no extra depth.
    assert_eq!(gen_pipe_depth(1500.0, 0.016, 384), 2);
    // Link-class c2: rate 10 000 sym/s, SRTT 40 ms (queue/jitter-inflated)
    // → D·rate = 800 ⇒ ceil(800/384)+1 = 4 generations in flight.
    assert_eq!(gen_pipe_depth(10_000.0, 0.040, 384), 4);
    // High-BDP (RTT200 @ 100 Mbit): 10 000 sym/s × 0.4 s = 4000 sym ⇒
    // ceil(4000/384)+1 = 12.
    assert_eq!(gen_pipe_depth(10_000.0, 0.200, 384), 12);
    // Monotone in rate·srtt, and hard-capped at GEN_PIPE_MAX_GENS.
    assert_eq!(gen_pipe_depth(1e9, 1.0, 384), GEN_PIPE_MAX_GENS);
}

// `RWM_PLAIN_RS`: the sampling-only feed must declare that it does not
// own the CC operating point — everything the
// Copa-sole feed switches (store-cap law, percap pipes, cwnd-dynamics
// call site, pass-through window writes) keys on `owns_cc()`.
#[test]
fn sampling_only_feed_does_not_own_cc() {
    assert!(CopaFeed::new().owns_cc());
    assert!(!CopaFeed::new_sampling_only(true).owns_cc());
}

/// Per-path BDP in-flight cap. The sender is "full" only when no path is
/// below its own cap (gain·BtlBw_i·RTprop_i). The slow path's RTT-inflated
/// cap bounds only the slow path; the fast path with room keeps the pipe
/// moving — a summed global budget would stall it behind the slow path.
#[test]
fn infl_percap_bounds_each_path_independently() {
    // Fast path (cap 100) has room at 40; slow path (RTT-inflated cap 60) is
    // at its cap. Not full — the fast path keeps pulling source.
    assert!(
        !infl_percap_full(&[(40, 100), (60, 60)]),
        "fast path with room ⇒ not full even when the slow path is at its cap"
    );
    // Every path at/above its own cap ⇒ full (total in-flight ≈ Σ per-path BDP).
    assert!(
        infl_percap_full(&[(100, 100), (60, 60)]),
        "all paths at their per-path cap ⇒ full"
    );
    assert!(
        infl_percap_full(&[(120, 100), (80, 60)]),
        "all paths over their per-path cap ⇒ full"
    );
    // Degenerate zero-cap path never blocks (cap.max(1)); a fresh path with
    // room keeps the sender open.
    assert!(!infl_percap_full(&[(0, 0), (10, 100)]));
    // Single fast path with room ⇒ not full (single-path parity control).
    assert!(!infl_percap_full(&[(50, 145)]));
}

#[test]
fn test_sent_store_retention_survives_window_eviction() {
    // The sender-loop invariant: the coding window slides freely (cap
    // eviction), but the sent-data store still serves the exact bytes
    // of any un-acked symbol for targeted retransmit — and entries
    // leave the store by ack only (the same split_off the loop runs).
    use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
    let mut encoder = RlcWindowEncoder::new(64);
    let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();

    for i in 0..(MAX_WINDOW_SIZE as u64 + 100) {
        let framed = vec![i as u8; 32];
        let sym = encoder.add_source(&framed);
        sent_store.insert(sym.block_id, sym.clone());
        // The loop's cap eviction (identical arithmetic).
        if encoder.window_size() > MAX_WINDOW_SIZE {
            let (oldest, _) = encoder.window_span();
            encoder.advance(oldest + (encoder.window_size() - MAX_WINDOW_SIZE) as u64);
        }
    }

    // Seq 10 slid out of the coding window (EVICT would have lost it)…
    assert!(encoder.get_source(10).is_none(), "seq 10 must be past the FEC horizon");
    // …but the store still holds the exact sent bytes for targeted ARQ.
    let held = sent_store.get(&10).expect("store retains un-acked symbol bytes");
    assert_eq!(held.block_id, 10);
    assert!(!held.is_repair);
    assert_eq!(&held.data[..32], &[10u8; 32]);

    // Removal by ack only: pruning at ack=49 drops exactly seqs 0..=49.
    let ack = 49u64;
    sent_store = sent_store.split_off(&(ack + 1));
    assert!(sent_store.get(&ack).is_none());
    assert!(sent_store.get(&(ack + 1)).is_some());
    assert_eq!(sent_store.len(), (MAX_WINDOW_SIZE + 100) - 50);
}

// ----- SACK-clocked store release (RWM_STORE_SACK_RELEASE, ADR-0060) -----
// Invariants: SACKed → released → retransmit-still-possible → cumulative-ack →
// fully freed; window opens on SACK; no double-release; released slots
// return to the pool; released seqs keep their per-flight loss clocks.

#[test]
fn test_sack_release_every_unacked_symbol_stays_recoverable() {
    // The chain that must not break: a SACKed symbol leaves the
    // outstanding count but its payload and
    // ARQ state survive until the cumulative frontier passes it.
    use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
    let n = 100u64;
    let mut encoder = RlcWindowEncoder::new(64);
    let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();
    let mut released: BTreeSet<u64> = BTreeSet::new();
    for i in 0..n {
        let sym = encoder.add_source(&vec![i as u8; 32]);
        sent_store.insert(sym.block_id, sym);
    }
    // Frontier at 9; hole at 10; receiver SACKed 11..=99.
    let ack = 9u64;
    sent_store = sent_store.split_off(&(ack + 1));
    sack_release_prune(&mut released, ack + 1);
    let newly = sack_release_mark(&sent_store, &mut released, 11, 99);
    assert_eq!(newly.len(), 89, "11..=99 newly released");
    // Released, not removed: outstanding drops to the hole + frontier
    // successor set, but every entry is still in the store.
    assert_eq!(sent_store.len(), 90, "nothing was removed from the store");
    assert_eq!(sack_release_outstanding(sent_store.len(), released.len()), 1);
    // Retransmit still possible for a released symbol (the NACK path
    // serves from sent_store.get — e.g. after a receiver eviction).
    let held = sent_store.get(&50).expect("released symbol still retransmittable");
    assert_eq!(&held.data[..32], &[50u8; 32]);
    // The hole itself was never SACKed → still counted.
    assert!(!released.contains(&10));
    // Cumulative frontier passes everything → fully freed, both maps.
    let ack2 = 99u64;
    sent_store = sent_store.split_off(&(ack2 + 1));
    sack_release_prune(&mut released, ack2 + 1);
    assert!(sent_store.is_empty());
    assert!(released.is_empty(), "released marks freed with the store (subset invariant)");
}

#[test]
fn test_sack_release_opens_window_and_returns_slots_to_pool() {
    // The mechanism: outstanding = retained − released re-opens the
    // flow-control gate (RWM_STORE_PATHS' pooled cap composes through
    // the same count) while a hole holds the cumulative frontier.
    let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
    let mut released: BTreeSet<u64> = BTreeSet::new();
    for i in 0..1024u64 {
        sent_store.insert(i, 0);
    }
    let cap = 1024usize; // RELIABLE_STORE_MAX-class pooled cap
    // Store pegged at cap across a frontier stall: gate closed.
    assert!(sack_release_outstanding(sent_store.len(), released.len()) >= cap);
    // Hole at 0; receiver SACKs 1..=1023 → slots return to the pool.
    let newly = sack_release_mark(&sent_store, &mut released, 1, 1023);
    assert_eq!(newly.len(), 1023);
    let outstanding = sack_release_outstanding(sent_store.len(), released.len());
    assert_eq!(outstanding, 1, "window opens: only the hole still counts");
    assert!(outstanding < cap, "gate re-opens while the frontier is frozen");
    // The frontier is not advanced — retention intact (reliability).
    assert_eq!(sent_store.len(), 1024);
}

#[test]
fn test_sack_release_no_double_release_and_percap_composes() {
    // A re-advertised SACK snapshot (gap reports are state snapshots,
    // re-sent every cadence) must not double-release: neither the
    // released set nor the per-path accounts move twice.
    let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
    let mut released: BTreeSet<u64> = BTreeSet::new();
    let mut acct: BTreeMap<u64, u32> = BTreeMap::new();
    let mut out: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for i in 0..10u64 {
        sent_store.insert(i, 0);
        percap_charge(&mut acct, &mut out, i, (i % 2) as u32);
    }
    assert_eq!(out[&0], 5);
    assert_eq!(out[&1], 5);
    // First snapshot: release 2..=5 (accounts freed on the newly list —
    // the sender-loop arm's exact arithmetic).
    let newly = sack_release_mark(&sent_store, &mut released, 2, 5);
    assert_eq!(newly, vec![2, 3, 4, 5]);
    for &k in &newly {
        percap_release_seq(&mut acct, &mut out, k);
    }
    assert_eq!(out[&0], 3);
    assert_eq!(out[&1], 3);
    // Same snapshot again (plus overlap): nothing newly released.
    let again = sack_release_mark(&mut sent_store, &mut released, 2, 5);
    assert!(again.is_empty(), "idempotent under re-advertised snapshots");
    assert_eq!(released.len(), 4);
    // Cumulative release later cannot double-free the accounts either
    // (percap_release_cumulative finds SACK-released seqs already gone).
    percap_release_cumulative(&mut acct, &mut out, 10);
    assert_eq!(out[&0], 0);
    assert_eq!(out[&1], 0);
}

#[test]
fn test_sack_release_keeps_arq_state_and_flight_clocks() {
    // RWM_RECOV_MP interaction: releasing a slot must not touch the
    // per-flight state — nack_retx_at (the live-flight clock the
    // per-path law times), retransmit_buffer (tail-sweep metadata),
    // source_path_map (seq→path evidence for the packet-threshold
    // channel). The release law takes none of them as inputs.
    let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
    let mut released: BTreeSet<u64> = BTreeSet::new();
    let mut nack_retx_at: std::collections::HashMap<u64, (u64, u32)> =
        std::collections::HashMap::new();
    let mut retransmit_buffer: BTreeMap<u64, (u64, f64, u32)> = BTreeMap::new();
    let mut source_path_map: BTreeMap<u64, u32> = BTreeMap::new();
    for i in 0..20u64 {
        sent_store.insert(i, 0);
        nack_retx_at.insert(i, (1_000 + i, (i % 2) as u32));
        retransmit_buffer.insert(i, (2_000 + i, 0.01, (i % 2) as u32));
        source_path_map.insert(i, (i % 2) as u32);
    }
    let newly = sack_release_mark(&sent_store, &mut released, 5, 19);
    assert_eq!(newly.len(), 15);
    // Released seqs keep their flight clocks + ARQ metadata intact.
    assert_eq!(nack_retx_at.len(), 20);
    assert_eq!(nack_retx_at[&10], (1_010, 0));
    assert_eq!(retransmit_buffer.len(), 20);
    assert_eq!(source_path_map.len(), 20);
    // And the store itself (the payload copy) is untouched.
    assert_eq!(sent_store.len(), 20);
}

#[test]
fn test_sack_release_mark_skips_seqs_not_retained() {
    // Ranges can race the cumulative frontier (the atomic may already
    // be ahead): seqs no longer in the store are never marked, so the
    // released set stays a subset of sent_store keys.
    let mut sent_store: BTreeMap<u64, u8> = BTreeMap::new();
    let mut released: BTreeSet<u64> = BTreeSet::new();
    for i in 50..60u64 {
        sent_store.insert(i, 0);
    }
    let newly = sack_release_mark(&sent_store, &mut released, 0, 100);
    assert_eq!(newly.len(), 10, "only retained seqs are markable");
    assert!(released.iter().all(|k| sent_store.contains_key(k)));
    assert_eq!(sack_release_outstanding(sent_store.len(), released.len()), 0);
}

/// SACK + BDP reassembly end-to-end reliability invariant: the sender
/// advances past a hole (SACK-prunes every out-of-order-received symbol
/// from its store), the receiver holds the
/// out-of-order symbols in its non-evicting reassembly (bounded by the BDP
/// the sender's outstanding cap enforces), the receiver prune never evicts a
/// received-but-undelivered symbol, the hole recovers by retransmit from the
/// sender's retained store, and every byte is delivered in order. Evicting
/// a pruned-but-unconsumed symbol at the receiver violates it.
#[test]
fn test_sack_bdp_reassembly_delivers_every_byte_past_a_hole() {
    use crate::fec::{RlcWindowEncoder, WindowEncoder, WireSymbol};
    // ---- Sender: send 300 source symbols, all retained in the store. ----
    let n = 300u64;
    let mut encoder = RlcWindowEncoder::new(64);
    let mut sent_store: BTreeMap<u64, WireSymbol> = BTreeMap::new();
    for i in 0..n {
        let sym = encoder.add_source(&vec![(i % 251) as u8; 32]);
        sent_store.insert(sym.block_id, sym);
    }

    // ---- Receiver: reliable, non-evicting reassembly (the BDP buffer). ----
    let mut reorder = ReorderBuffer::new_reliable();
    let mut received_seqs: BTreeSet<u64> = BTreeSet::new();
    let mut highest_delivered_seq: u64 = 0; // -1 sentinel via next_deliver_seq
    let mut highest_seen_seq: u64 = 0;
    let recv_win_cap: u64 = MAX_WINDOW_SIZE as u64;
    let mut delivered: Vec<u64> = Vec::new();

    // The receiver gets seq 0..=9 in order, then a hole at seq 10, then
    // everything above (11..=299) out of order. Deliver in wire arrival order.
    let hole = 10u64;
    let arrival: Vec<u64> = (0..=9).chain(11..n).collect();
    for &seq in &arrival {
        received_seqs.insert(seq);
        highest_seen_seq = highest_seen_seq.max(seq);
        for (dseq, _, _) in reorder.push(seq, Bytes::from(vec![(seq % 251) as u8; 32])) {
            delivered.push(dseq);
            highest_delivered_seq = highest_delivered_seq.max(dseq);
        }
        // The receiver periodically prunes its decoder/received-seq state.
        // Invariant (RWM_REASM_BDP clamp): prune_before never exceeds the
        // delivered frontier, so no received-above-hole symbol is evicted.
        let prune_before = highest_delivered_seq
            .saturating_sub(recv_win_cap * 2)
            .min(highest_delivered_seq);
        received_seqs = received_seqs.split_off(&prune_before);
    }

    // Delivery is frozen at the hole: only 0..=9 delivered so far.
    assert_eq!(delivered, (0..=9).collect::<Vec<_>>(), "in-order stalls at the hole");
    // 11..=299 are held (received but not delivered) — the reassembly holds
    // them all, none evicted (non-evicting reliable buffer + clamped prune).
    assert_eq!(reorder.pending_count(), (n - 11) as usize, "all OOO symbols held");
    for seq in 11..n {
        assert!(received_seqs.contains(&seq), "seq {seq} must survive prune until delivered");
    }

    // ---- Sender: SACK-prune the received (out-of-order) symbols. ----
    // The cumulative ack is 9; everything 11..=299 was SACKed.
    let ack = 9u64;
    sent_store = sent_store.split_off(&(ack + 1));
    for (start, end) in received_sack_ranges(&received_seqs, ack + 1, highest_seen_seq) {
        let acked: Vec<u64> = sent_store.range(start..=end).map(|(&k, _)| k).collect();
        for k in acked {
            sent_store.remove(&k);
        }
    }
    // Only the hole stays retained — the sender is free to inject fresh source.
    assert_eq!(sent_store.len(), 1, "sender retains ONLY the unfilled hole");
    assert!(sent_store.contains_key(&hole), "the hole survives for ARQ retransmit");

    // ---- Recovery: the hole is retransmitted from the retained store. ----
    let hole_sym = sent_store.get(&hole).expect("hole retained").clone();
    for (dseq, _, _) in reorder.push(hole, Bytes::copy_from_slice(&hole_sym.data[..32])) {
        delivered.push(dseq);
        highest_delivered_seq = highest_delivered_seq.max(dseq);
    }

    // Every byte delivered, in order, exactly once — the reliability invariant.
    assert_eq!(delivered, (0..n).collect::<Vec<_>>(), "every symbol delivered in order");
    assert_eq!(reorder.pending_count(), 0, "reassembly fully drained — nothing stranded");
}

/// Idle-triggered recovery. The congestion multiplier may fully suppress
/// NACK repairs (correct on a congested straggler), but must never stay
/// suppressed when the sender is idle
/// except for a confirmed hole — that wedges a reliable transfer.
#[test]
fn test_idle_triggered_recovery_floor() {
    let mut st = NackCongestionState::new();
    // Drive congestion: both loss and RTT rising for >= threshold periods.
    let mut rtt = Duration::from_millis(20);
    let mut loss = 0.02;
    for _ in 0..8 {
        loss += 0.02;
        rtt += Duration::from_millis(5);
        st.update(loss, Some(rtt));
    }
    // Multiplier has collapsed toward 0 (full suppression).
    assert!(st.repair_multiplier < 0.05, "congestion must suppress: {}", st.repair_multiplier);

    // Active sender: suppression stands — congestion safety wins, so a
    // retransmit would not be forced onto the straggler.
    let active = st.effective_multiplier(false);
    assert_eq!(active, st.repair_multiplier, "active transfer keeps raw multiplier");
    assert_eq!((MAX_NACK_REPAIRS_PER_NACK as f64 * active).round() as u64, 0,
        "active + suppressed => 0 forced repairs");

    // Idle sender (no new source in flight): recovery is never fully
    // suppressed — the floor yields >= 1 targeted retransmit per round so
    // the confirmed hole is recovered and the transfer un-wedges.
    let idle = st.effective_multiplier(true);
    assert!(idle >= IDLE_RECOVERY_FLOOR, "idle floor lifts the multiplier: {idle}");
    assert!((MAX_NACK_REPAIRS_PER_NACK as f64 * idle).round() as u64 >= 1,
        "idle floor must permit >= 1 retransmit/round");

    // Continuity: on a clean/uncongested channel the idle floor is a no-op
    // (raw multiplier already >= floor), so behavior is unchanged.
    let mut clean = NackCongestionState::new();
    for _ in 0..5 { clean.update(0.0, Some(Duration::from_millis(20))); }
    assert_eq!(clean.effective_multiplier(true), clean.repair_multiplier,
        "idle floor is a no-op when not suppressed");
    assert!((clean.repair_multiplier - 1.0).abs() < 1e-9);
}


// ── Ack-merge emission decision (`RWM_ACK_MERGE`, default on) ──

/// With `RWM_ACK_MERGE` off, a datagram goes out on exactly the unmerged
/// predicate and never otherwise.
#[test]
fn ack_merge_off_emits_on_exactly_the_shipped_predicate() {
    for &adv in &[false, true] {
        for &gap in &[false, true] {
            let (emit, advertise) = window_ack_emission(adv, gap, false);
            assert_eq!(
                emit,
                adv || gap,
                "gate OFF must emit iff (cumulative_advanced || gap_report_due)"
            );
            assert_eq!(emit, advertise, "gate OFF: emit and advertise are one decision");
        }
    }
}

/// With the gate on the ack is unconditional — it carries the suppressed
/// per-batch `Ack`'s payload, so it must keep that message's
/// once-per-data-message cadence. Two control datagrams become one; zero
/// is never correct.
#[test]
fn ack_merge_on_emits_once_per_data_message() {
    for &adv in &[false, true] {
        for &gap in &[false, true] {
            let (emit, _) = window_ack_emission(adv, gap, true);
            assert!(emit, "the merged ack carries the Ack payload and must always go out");
        }
    }
}

/// The safety law of the merge: the gate changes only whether a datagram
/// is sent, never what it advertises. `advertise` is invariant under the
/// gate, so `GAP_ACK_MIN_INTERVAL` still rate-limits gap reports at its
/// shipped cadence and the depth-16 nack/sack `try_send` channels see no
/// new pressure — a merge-only ack carries counters and an echo, never a
/// gap report. (Getting this wrong would turn every stalled-frontier
/// batch into a NACK storm, which is the failure the rate limit exists
/// to prevent.)
#[test]
fn ack_merge_never_changes_what_the_ack_advertises() {
    for &adv in &[false, true] {
        for &gap in &[false, true] {
            let (_, off) = window_ack_emission(adv, gap, false);
            let (_, on) = window_ack_emission(adv, gap, true);
            assert_eq!(
                off, on,
                "gap advertisement (and therefore the gap rate limit) is                      invariant under RWM_ACK_MERGE"
            );
        }
    }
    // Concretely: frontier stalled on a hole, gap report not yet due.
    let (emit, advertise) = window_ack_emission(false, false, true);
    assert!(emit, "the merged ack still goes out (it carries the counters)");
    assert!(!advertise, "but it advertises no gap — the rate limit holds");
}

// ── One pipeline for every hint (ADR-0069, executed) ──

/// The default routes EVERY hint to the window pipeline: the block pipeline
/// is deleted (ADR-0069, discharged by `docs/status.md` §5's
/// `WINDOW-NOT-WORSE`). Asserts the routing consequence, not just the
/// fields (measurement-discipline rule 1): the resolved default config of
/// each named point carries the streaming RLC codec, and the retention
/// contract ρ is the named point's preset — ρ = 1 (retain-until-acked) at
/// Bulk/Auto, ρ < 1 (EVICT) at Realtime. ρ is an independent dial: it never
/// selects a pipeline.
#[test]
fn default_config_routes_every_hint_to_the_window_pipeline() {
    for (hint, h) in [
        (ProtocolHint::Realtime, "realtime"),
        (ProtocolHint::Auto, "auto"),
        (ProtocolHint::Bulk, "bulk"),
    ] {
        let cfg = crate::config::RaptorpathConfig {
            protocol_hint: Some(h.into()),
            ..Default::default()
        };
        let (pc, _) = crate::config::resolve(&cfg).expect("the default config resolves");
        assert_eq!(pc.protocol_hint, hint);
        assert_eq!(
            pc.fec_backend,
            FecBackend::Rlc,
            "{hint:?}: the default codec is the window pipeline's RLC"
        );
        assert_eq!(
            pipeline_backend(&pc).expect("the default routes to the window pipeline"),
            FecBackend::Rlc,
            "{hint:?}: run_impl's pipeline selection accepts the default"
        );
        assert_eq!(
            pc.window_reliable,
            hint != ProtocolHint::Realtime,
            "{hint:?}: ρ preset — retain-until-acked at Bulk/Auto, EVICT at Realtime"
        );
    }
    // The empty config is the Auto named point.
    let (pc, _) = crate::config::resolve(&crate::config::RaptorpathConfig::default())
        .expect("the empty default config resolves");
    assert_eq!(pc.protocol_hint, ProtocolHint::Auto, "shipped default hint is Auto");
    assert_eq!(pc.fec_backend, FecBackend::Rlc);
    assert!(pc.window_reliable, "Auto's ρ preset is retain-until-acked");

    // A hand-built library PeerConfig naming a block-only codec is a
    // startup error naming ADR-0069 — there is no block fallback.
    let (mut lib, _) = crate::config::resolve(&crate::config::RaptorpathConfig::default()).unwrap();
    for b in [FecBackend::RaptorQ, FecBackend::ReedSolomon] {
        lib.fec_backend = b;
        let err = pipeline_backend(&lib).expect_err("block-only codec must not route");
        assert!(err.to_string().contains("ADR-0069"), "{b:?}: {err}");
    }
}

// ── The cross-path loss contamination, removed by wire v9, bounded ──
//
// A deterministic two-path model of the wire geometry: the shipped
// sender sequencer (`BatchCounter::next`, which stamps the global
// `batch_seq` and the v9 per-path `path_seq`), batches striped across
// two paths, per-path datagram loss injected at a known rate, and the
// real `PathBatchTracker` on the receiver side fed through its real entry
// point (`record(&SymbolBatch)`). Both estimator feeds are driven from it
// and read back through the real `LossEstimator`, so the assertions bound
// the shipped mechanism, not a re-implementation of it
// (measurement-discipline rule 1). Through wire v8 the tracker read the
// global sequence and every other path's batch counted as loss.

/// A batch of `n` empty symbols, as the receiver's tracker sees it (it
/// reads only the sequence and the symbol count).
fn model_batch(seqs: (u64, u64), n: u32, path: u32) -> SymbolBatch {
    let syms = (0..n)
        .map(|i| crate::fec::WireSymbol {
            block_id: seqs.0,
            payload_id: i,
            is_repair: false,
            data: Vec::new().into(),
            backend: FecBackend::Rlc,
        })
        .collect();
    SymbolBatch::new(syms, 0, seqs, path)
}

/// What one path read, per arm, in one run of the two-path model.
#[derive(Debug, Clone, Copy)]
struct XpathRead {
    /// The realized per-path datagram loss actually injected.
    eps_true: f64,
    /// The law itself, count-weighted: `1 − Σreceived / Σexpected` over
    /// everything the arm fed the estimator. Everything below is an
    /// estimator read of it.
    fed_old: f64,
    fed_new: f64,
    /// `LossEstimator::loss_rate()` — `tx_ewma_loss`, the EWMA of the
    /// per-call ratio. The shipped consumer read (NACK margin, placement
    /// costs, …). Sampled as the run mean, because at a low-loss cell the
    /// instantaneous EWMA sits near 0 between rare loss events.
    ewma_old: f64,
    ewma_new: f64,
    /// `LossEstimator::loss_rate_mean()` — the Beta posterior mean, which
    /// is count-weighted (a shipped consumer: the scheduler reads
    /// `loss_rate().max(loss_rate_mean())`).
    beta_old: f64,
    beta_new: f64,
}

/// One deterministic run of the two-path model.
///
/// `share` is the striping period: 1 => single path (the N = 1 control),
/// 2 => 50/50 alternation (c7), 6 => 5:1 (c8's split).
fn xpath_loss_model(
    batches: u32,
    syms: u32,
    share: u32,
    eps: [f64; 2],
    lag: usize,
) -> [XpathRead; 2] {
    use crate::control::estimator::LossEstimator;
    use crate::scheduler::PathState;
    use crate::scheduler::MockClock;

    // Deterministic LCG — no rand, no clock, no seed drift.
    let mut rng: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((rng >> 11) as f64) / ((1u64 << 53) as f64)
    };

    let clock = Arc::new(MockClock::new());
    let mut tracker = [PathBatchTracker::new(), PathBatchTracker::new()];
    let mut legacy = [LossEstimator::new(), LossEstimator::new()];
    let mut truth = [LossEstimator::new(), LossEstimator::new()];
    let mut path_state = [
        PathState::new(0, clock.clone()),
        PathState::new(1, clock.clone()),
    ];
    // The sender's own per-path wire-handoff counter (`PathStats::
    // symbols_sent`), and its lagged view: an ack is processed when the
    // sender has already dispatched `lag` further batches on that path.
    // That lag is the in-flight offset the law's doc comment names.
    let mut sent_cum = [0u64; 2];
    let mut sent_hist: [Vec<u64>; 2] = [Vec::new(), Vec::new()];
    let mut dispatched = [0u64; 2];
    let mut dropped = [0u64; 2];
    // Arrivals, in wire order, as (path, (batch_seq, path_seq), symbols,
    // dispatch index on that path — the cursor into `sent_hist`, which must
    // be keyed by dispatches, not by arrivals: a dropped batch still
    // advanced the sender's counter, and that is the whole signal).
    let mut arrivals: Vec<(usize, (u64, u64), u32, usize)> = Vec::new();
    // The shipped sequencer: one call per dispatched batch, lost or not.
    let counter = BatchCounter::new();

    for seq in 0..batches as u64 {
        let p = if share == 1 {
            0
        } else if seq % share as u64 == 0 {
            1
        } else {
            0
        };
        sent_cum[p] += syms as u64;
        sent_hist[p].push(sent_cum[p]);
        let disp_idx = dispatched[p] as usize;
        dispatched[p] += 1;
        let seqs = counter.next(p as u32);
        if next() < eps[p] {
            dropped[p] += 1;
            continue;
        }
        arrivals.push((p, seqs, syms, disp_idx));
    }

    let mut seen = [0usize; 2];
    // Σ(expected, received) actually fed, per arm — the law's own operands.
    let mut fed_old = [(0u64, 0u64); 2];
    let mut fed_new = [(0u64, 0u64); 2];
    // Running mean of the shipped `loss_rate()` read, per arm.
    let mut ewma_old = [(0.0f64, 0u64); 2];
    let mut ewma_new = [(0.0f64, 0u64); 2];

    for (p, seqs, n, disp_idx) in arrivals {
        // Receiver: the real tracker through its real entry point, on the
        // batch exactly as the wire carries it.
        let (expected, received) = tracker[p].record(&model_batch(seqs, n, p as u32));
        // Arm A (shipped): the receiver's gap estimate.
        legacy[p].record_batch(expected, received);
        fed_old[p].0 += expected as u64;
        fed_old[p].1 += received as u64;
        ewma_old[p].0 += legacy[p].loss_rate();
        ewma_old[p].1 += 1;
        // Arm B (`RWM_LOSS_SENT_TRUTH`): the sender's own count against
        // the receiver's clean cumulative arrival count, paired through
        // the real cursor law.
        let idx = (disp_idx + lag).min(sent_hist[p].len().saturating_sub(1));
        let (le, lr) = path_state[p]
            .sender_truth_loss_delta(sent_hist[p][idx], tracker[p].total_received);
        if le > 0 {
            truth[p].record_batch(le, lr);
            fed_new[p].0 += le as u64;
            fed_new[p].1 += lr as u64;
        }
        ewma_new[p].0 += truth[p].loss_rate();
        ewma_new[p].1 += 1;
        seen[p] += 1;
    }

    let ratio = |(e, r): (u64, u64)| if e == 0 { 0.0 } else { 1.0 - r as f64 / e as f64 };
    let mean = |(s, n): (f64, u64)| if n == 0 { 0.0 } else { s / n as f64 };
    [0usize, 1].map(|p| XpathRead {
        eps_true: dropped[p] as f64 / dispatched[p].max(1) as f64,
        fed_old: ratio(fed_old[p]),
        fed_new: ratio(fed_new[p]),
        ewma_old: mean(ewma_old[p]),
        ewma_new: mean(ewma_new[p]),
        beta_old: legacy[p].loss_rate_mean(),
        beta_new: truth[p].loss_rate_mean(),
    })
}

/// Direction 1 (wire v9) — the DEFAULT gap estimate reads each path's own
/// ε, independent of the striping share. Before v9 the receiver's
/// `PathBatchTracker` read gaps in the one global `batch_seq`, so the other
/// path's batches counted as loss (ε̂ ≈ 0.5 at 50/50, 0.17/0.84 at 5:1). v9
/// stamps a per-path batch sequence and the tracker keys on it, so the gap
/// pair is honest at every share. Absolute bounds at every share, including
/// the N = 1 control and asymmetric per-leg loss (attribution, not just
/// magnitude).
#[test]
fn default_gap_estimate_reads_each_path_own_epsilon_independent_of_share() {
    for (cell, share, eps) in [
        ("n1", 1u32, [0.0055f64, 0.0055]),
        ("c7", 2, [0.0055, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
        ("s10", 10, [0.0196, 0.0055]),
    ] {
        let r = xpath_loss_model(60_000, 8, share, eps, 4);
        for (p, x) in r.iter().enumerate() {
            if share == 1 && p == 1 {
                continue; // the N = 1 control dispatches nothing on p1
            }
            assert!(
                (x.fed_old - x.eps_true).abs() <= 5e-4,
                "{cell} p{p}: the default gap estimate must read this path's own ε independent of share — {:.5} vs realized {:.5}",
                x.fed_old,
                x.eps_true
            );
            // The Beta posterior decays at 0.995/call: at the end of a run it
            // is one ~200-call window (~9 loss events at these ε), bounded at
            // the cells the sender-truth test bounds it (c7/c8, and the N = 1
            // control); at s10 the law above is the gate and the window read
            // is sampling noise.
            if share <= 6 {
                assert!(
                    (0.5 * x.eps_true..2.0 * x.eps_true).contains(&x.beta_old),
                    "{cell} p{p}: loss_rate_mean must read this path's own ε — {:.5} vs realized {:.5}",
                    x.beta_old,
                    x.eps_true
                );
            }
        }
        if share > 1 {
            let (hi, lo) = if eps[1] > eps[0] { (1, 0) } else { (0, 1) };
            if eps[hi] > eps[lo] {
                assert!(
                    r[hi].fed_old / r[lo].fed_old > 2.5,
                    "{cell}: the lossier leg's ε must be the larger one ({:.5} vs {:.5})",
                    r[hi].fed_old,
                    r[lo].fed_old
                );
            }
        }
    }
}

/// Direction 2 — under the gate each path reads its own realized loss.
/// Absolute bounds, not ordinal ones, at both the symmetric (c7) and the
/// 5:1 asymmetric (c8) geometry, with the c8 legs carrying different loss
/// (0.55% / 1.96%) so the test bounds attribution, not just magnitude.
///
/// Three quantities are bounded, because they answer different questions:
/// the law (`Σreceived/Σexpected`, exact), the count-weighted estimator
/// read (`loss_rate_mean`, the Beta posterior), and the shipped
/// `loss_rate()` EWMA. The EWMA carries a known ≈½ under-read at a
/// rare-loss cell that is a property of the estimator, not of this gate:
/// a dropped datagram folds its loss into the next ack's doubled delta,
/// so the per-call ratio it averages reads `0.5` on ~ε of the calls
/// instead of `ε` on all of them. It is bounded here (`[0.3ε, 1.5ε]`).
#[test]
fn sender_truth_loss_reads_each_path_own_epsilon_at_n2() {
    // c7 geometry: symmetric 50/50, both legs 0.55%.
    // c8 geometry: 5:1 split, asymmetric loss — p0 = fast leg 0.55%,
    // p1 = slow leg 1.96%.
    for (cell, share, eps) in [
        ("c7", 2u32, [0.0055f64, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
    ] {
        let r = xpath_loss_model(60_000, 8, share, eps, 4);
        for (p, x) in r.iter().enumerate() {
            // What this model reads (60 000 batches, 8 sym/batch, lag 4):
            //   c7 p0/p1  ε_true 0.0055  fed_old 0.503/0.503
            //                            fed_new 0.0056/0.0055
            //   c8 p0     ε_true 0.0054  fed_old 0.171
            //                            fed_new 0.0055
            //   c8 p1     ε_true 0.0195  fed_old 0.837
            //                            fed_new 0.0199
            assert!(
                (x.fed_new - x.eps_true).abs() <= 5e-4,
                "{cell} p{p}: the LAW must read this path's own ε — \
                 {:.5} vs realized {:.5}",
                x.fed_new,
                x.eps_true
            );
            // The Beta posterior decays at 0.995/call, so it is a
            // ~200-call window mean, not the run mean — bounded
            // multiplicatively (its sampling spread at these loss rates).
            assert!(
                (0.5 * x.eps_true..2.0 * x.eps_true).contains(&x.beta_new),
                "{cell} p{p}: loss_rate_mean must read this path's own ε — \
                 {:.5} vs realized {:.5}",
                x.beta_new,
                x.eps_true
            );
            assert!(
                (0.3 * x.eps_true..1.5 * x.eps_true).contains(&x.ewma_new),
                "{cell} p{p}: loss_rate() must land in the ε class — \
                 {:.5} vs realized {:.5}",
                x.ewma_new,
                x.eps_true
            );
            // Wire v9: the default (gap) arm is no longer the inflated
            // one -- it reads the same per-path law as the gate's pair.
            assert!(
                (x.fed_old - x.fed_new).abs() <= 1e-3,
                "{cell} p{p}: under v9 the default gap arm and the \
                 sender-truth arm must read the same law ({:.5} vs {:.5})",
                x.fed_old,
                x.fed_new
            );
            assert!(
                (0.3 * x.eps_true..1.5 * x.eps_true).contains(&x.ewma_old),
                "{cell} p{p}: the default loss_rate() must land in the ε \
                 class too — {:.5} vs realized {:.5}",
                x.ewma_old,
                x.eps_true
            );
        }
        // Attribution: at c8 the slow leg's honest ε must be the larger
        // of the two by ≈ the injected ratio.
        if cell == "c8" {
            assert!(
                r[1].fed_new / r[0].fed_new > 2.5,
                "c8: the slow leg's honest ε must be the larger one \
                 ({:.5} vs {:.5})",
                r[1].fed_new,
                r[0].fed_new
            );
            assert!(
                r[1].fed_old / r[0].fed_old > 2.5,
                "c8: the default arm attributes too — the slow leg's ε is \
                 the larger one ({:.5} vs {:.5}); v8 read 0.84 here",
                r[1].fed_old,
                r[0].fed_old
            );
        }
    }
}

/// The law's named residual, bounded: the sent cursor leads the received
/// cursor by ≈ in_flight, so a run's opening over-reads by about one BDP
/// and thereafter the deltas are unbiased.
/// Sweep the lag over two orders and require the settled read to stay
/// inside the same absolute tolerance.
#[test]
fn sender_truth_loss_delta_is_unbiased_under_a_constant_in_flight_lag() {
    for lag in [0usize, 1, 4, 16, 64, 256] {
        let r = xpath_loss_model(60_000, 8, 2, [0.0055, 0.0055], lag);
        let per_path = 60_000.0 / 2.0;
        // The bound, as a formula rather than a tuned number: a constant
        // lag biases nothing in steady state (the offset cancels in the
        // delta), and the only residual is the tail — the last `lag`
        // batches on each path are charged as expected with no arrival
        // left to match them. That is `lag / batches_per_path`, plus the
        // 5e-4 sampling floor the zero-lag run already carries.
        let bound = 5e-4 + lag as f64 / per_path;
        for (p, x) in r.iter().enumerate() {
            assert!(
                (x.fed_new - x.eps_true).abs() <= bound,
                "lag {lag} p{p}: a CONSTANT in-flight lag must not bias the \
                 delta pair beyond its tail term {bound:.5} — read {:.5} vs \
                 realized {:.5}",
                x.fed_new,
                x.eps_true
            );
        }
    }
}

/// The cursor law's own invariants (the `ack_merge_counter_delta`
/// contract, re-asserted for the sender-truth pair): stale/reordered acks
/// are no-ops, cursors only move forward, received never exceeds
/// expected, and the zero-payload sentinel is inert.
#[test]
fn sender_truth_loss_delta_is_idempotent_and_never_underflows() {
    use crate::scheduler::PathState;
    use crate::scheduler::MockClock;
    let clock = Arc::new(MockClock::new());
    let mut p = PathState::new(0, clock.clone());
    assert_eq!(p.sender_truth_loss_delta(500, 0), (0, 0), "sentinel is inert");
    assert_eq!(p.sender_truth_loss_delta(500, 480), (500, 480));
    assert_eq!(
        p.sender_truth_loss_delta(500, 480),
        (0, 0),
        "a duplicate ack is a no-op"
    );
    assert_eq!(
        p.sender_truth_loss_delta(300, 200),
        (0, 0),
        "a reordered/stale ack is a no-op"
    );
    assert_eq!(p.sender_truth_loss_delta(700, 660), (200, 180));
    // Lag running the other way (receiver ahead of the sampled sender
    // counter) must clamp, never underflow the derived loss count.
    let mut q = PathState::new(0, clock);
    assert_eq!(q.sender_truth_loss_delta(100, 400), (100, 100));
}

/// The release law's own invariants, the mirror of the pair above.
#[test]
fn sender_truth_release_delta_is_idempotent_and_never_underflows() {
    use crate::scheduler::MockClock;
    use crate::scheduler::PathState;
    let clock = Arc::new(MockClock::new());
    let mut p = PathState::new(0, clock.clone());
    assert_eq!(p.sender_truth_release_delta(500, 0), 0, "sentinel is inert");
    assert_eq!(p.sender_truth_release_delta(500, 480), 20);
    assert_eq!(
        p.sender_truth_release_delta(500, 480),
        0,
        "a duplicate ack releases nothing"
    );
    assert_eq!(
        p.sender_truth_release_delta(300, 200),
        0,
        "a reordered/stale ack releases nothing"
    );
    assert_eq!(p.sender_truth_release_delta(700, 660), 20);
    // Receiver ahead of the sampled sender counter (the tail direction):
    // the honest lost count is zero, never a negative that would wrap into
    // a huge release. This is the direction that decides whether the law
    // can leak the gauge open, and it cannot.
    let mut q = PathState::new(0, clock.clone());
    assert_eq!(q.sender_truth_release_delta(100, 400), 0);
    // The two gates' cursors are independent — flipping one must not
    // consume the other's delta. Same path, both laws, same operands:
    // each sees the full delta.
    let mut r = PathState::new(0, clock);
    assert_eq!(r.sender_truth_loss_delta(1000, 900), (1000, 900));
    assert_eq!(
        r.sender_truth_release_delta(1000, 900),
        100,
        "the release cursor must not have been advanced by the loss law"
    );
}

// ── The in-flight accounting ledger, deterministically ───────────────
//
// The sibling of `xpath_loss_model`, and built the same way: the shipped
// sequencer (`BatchCounter`, v9 per-path `path_seq`), the real
// `PathBatchTracker` on the receiver, and the real
// `PathState` in-flight ledger driven through the real `charge_in_flight`
// / `release_in_flight` / cursor laws on the sender. The two un-metered
// recovery channels are modelled as what they are — a wire handoff with
// no charge — so the charge defect (`RWM_CHARGE_RECOVERY`) and the release
// defect (`RWM_RELEASE_1TO1`) can be moved one at a time.

/// What one path's ledger read, in one run of the two-path model.
#[derive(Debug, Clone, Copy)]
struct LedgerRead {
    /// Symbols this path actually handed to the wire (source + recovery).
    wire: u64,
    /// Symbols `charge_in_flight` was called for.
    charges: u64,
    /// Σ`n` the release law asked for.
    releases_req: u64,
    /// Of that, what `release_in_flight`'s saturating subtraction threw
    /// away because `in_flight` was already zero — budget the path can
    /// never get back.
    releases_wasted: u64,
    /// Symbols the receiver actually took delivery of on this path.
    delivered: u64,
    /// `(releases_req − charges) / delivered` — the leak: extra budget
    /// slots released per delivered symbol.
    leak_per_delivered: f64,
    /// Fraction of post-ack samples at which the gauge read `in_flight ==
    /// 0` while the path genuinely had symbols outstanding. This is the
    /// defect's behavioural face: `available() = cwnd − in_flight` is then
    /// wide open on evidence the path does not have.
    false_empty_frac: f64,
    /// Mean gauge reading, and the mean truth beside it.
    infl_mean: f64,
    outstanding_mean: f64,
    /// The gauge after the last ack — zero iff the ledger balanced.
    infl_final: u32,
}

/// Which lost-symbol release law the run drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Release {
    /// Shipped: `expected − received` off the global-`batch_seq` gap
    /// estimate, plus the 250 ms-floored expiry as a backstop.
    Legacy,
    /// The refuted candidate: `d(symbols_sent) − d(cum_received)`.
    SentTruth,
    /// `RWM_RELEASE_1TO1`: no ack-arm term at all — the RFC 9002
    /// time-threshold sweep of the charge log is the whole answer.
    OneToOne,
}

/// One deterministic run of the two-path ledger model.
///
/// `share`: 1 = single path (the N = 1 control), 2 = 50/50 (c7), 6 = 5:1
/// (c8's split). `lag` = how many of this path's own dispatches
/// fit in one RTT, so `lag × syms` is the true outstanding and `rtt_us /
/// lag` is the dispatch interval — the model's clock, which
/// `expire_in_flight` reads.
///
/// `charge_recovery` = `RWM_CHARGE_RECOVERY`; `recov_per_drop` = symbols
/// the recovery plane puts on the wire per drop (the SACK-gap retransmit
/// plus the NACK repair margin), which arrive and are counted by the
/// receiver either way.
#[allow(clippy::too_many_arguments)]
fn xpath_ledger_model(
    batches: u32,
    syms: u32,
    share: u32,
    eps: [f64; 2],
    lag: usize,
    rtt_us: u64,
    release: Release,
    charge_recovery: bool,
    recov_per_drop: u32,
) -> [LedgerRead; 2] {
    use crate::scheduler::MockClock;
    use crate::scheduler::PathState;

    let mut rng: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((rng >> 11) as f64) / ((1u64 << 53) as f64)
    };

    // One clock per path. The two paths' ledgers are independent in this
    // model, and a per-path clock lets each one's in-flight window be
    // stated physically: `lag` dispatches is one RTT, so the true
    // outstanding is `lag x syms` and the dispatch interval is
    // `RTT / lag`. Time is what `expire_in_flight` reads, so it cannot be
    // left implicit.
    let rtt = Duration::from_micros(rtt_us);
    let dt = Duration::from_micros(rtt_us / lag.max(1) as u64);
    let clocks = [Arc::new(MockClock::new()), Arc::new(MockClock::new())];
    let mut tracker = [PathBatchTracker::new(), PathBatchTracker::new()];
    let mut path_state = [
        PathState::new(0, clocks[0].clone()),
        PathState::new(1, clocks[1].clone()),
    ];
    for p in 0..2usize {
        path_state[p].force_release_1to1(release == Release::OneToOne);
        // The path's own smoothed RTT — the operand of the RFC 9002
        // horizon. Fed through the real sampler, not written.
        for _ in 0..64 {
            path_state[p].record_rtt_sample(rtt);
        }
    }

    // ── Dispatch phase: build each path's wire history ────────────────
    // Per dispatch: ((batch_seq, path_seq), symbols, arrived, symbols
    // charged). The recovery channel's entries carry `charged = 0` unless
    // the gate is on — that is the whole of defect 1, expressed as data.
    let mut disp: [Vec<((u64, u64), u32, bool, u32)>; 2] = [Vec::new(), Vec::new()];
    // Parallel per-path cumulative `PathStats::symbols_sent`. Under the
    // charge gate the recovery channel increments it too; with the gate
    // off it does not — the counter is one of the three meters the two
    // channels bypass.
    let mut sent_hist: [Vec<u64>; 2] = [Vec::new(), Vec::new()];
    let mut sent_cum = [0u64; 2];
    let counter = BatchCounter::new();
    let mut dropped = [0u64; 2];

    for i in 0..batches as u64 {
        let p = if share == 1 {
            0
        } else if i % share as u64 == 0 {
            1
        } else {
            0
        };
        // The source batch: charged and counted at the handoff, always.
        let seq = counter.next(p as u32);
        sent_cum[p] += syms as u64;
        let arrived = next() >= eps[p];
        disp[p].push((seq, syms, arrived, syms));
        sent_hist[p].push(sent_cum[p]);
        if arrived {
            continue;
        }
        dropped[p] += 1;
        // The recovery plane answers the drop on the same path. It takes
        // its own sequences (`batch_counter.next`) and it reaches the link
        // — it is only the sender's books it is missing from.
        for _ in 0..recov_per_drop {
            let rseq = counter.next(p as u32);
            if charge_recovery {
                sent_cum[p] += 1;
            }
            disp[p].push((rseq, 1, true, u32::from(charge_recovery)));
            sent_hist[p].push(sent_cum[p]);
        }
    }

    // ── Ack phase ─────────────────────────────────────────────────────
    let mut charge_cursor = [0usize; 2];
    let mut wire = [0u64; 2];
    let mut charges = [0u64; 2];
    let mut releases_req = [0u64; 2];
    let mut releases_wasted = [0u64; 2];
    let mut delivered = [0u64; 2];
    let mut false_empty = [0u64; 2];
    let mut samples = [0u64; 2];
    let mut infl_sum = [0f64; 2];
    let mut outstanding_sum = [0f64; 2];

    for p in 0..2usize {
        for d in 0..disp[p].len() {
            // Everything up to `d + lag` has left the wire by the time
            // this ack is processed.
            let upto = (d + lag).min(disp[p].len() - 1);
            while charge_cursor[p] <= upto {
                // A dispatch takes `dt` of wall time, and the charge is
                // stamped with the clock `expire_in_flight` reads.
                clocks[p].advance(dt);
                let (_, n, _, charged) = disp[p][charge_cursor[p]];
                wire[p] += n as u64;
                if charged > 0 {
                    charges[p] += charged as u64;
                    path_state[p].charge_in_flight(charged);
                }
                charge_cursor[p] += 1;
            }
            // The engine sweeps the charge log on the sender loop's own
            // cadence (the backpressure poll, the report task,
            // `block_sender`). Driven at every ack here, in every arm —
            // it is an always-on mechanism, and leaving it out of the
            // shipped arm would flatter the gate.
            let before_exp = path_state[p].in_flight;
            path_state[p].expire_in_flight();
            releases_req[p] += (before_exp - path_state[p].in_flight) as u64;
            let (seq, n, arrived, _) = disp[p][d];
            if !arrived {
                continue;
            }
            // Receiver: the real tracker through its real entry point.
            let (expected, received) = tracker[p].record(&model_batch(seq, n, p as u32));
            delivered[p] += received as u64;
            // Sender, delivery arm (identical in both arms).
            let mut rel = |ps: &mut PathState, want: u32| {
                let before = ps.in_flight;
                ps.release_in_flight(want);
                releases_req[p] += want as u64;
                releases_wasted[p] += (want as u64).saturating_sub((before - ps.in_flight) as u64);
            };
            rel(&mut path_state[p], received);
            // Sender, lost arm — the shipped term, the refuted candidate,
            // and the 1:1 gate.
            let lost = match release {
                Release::Legacy => expected.saturating_sub(received),
                Release::SentTruth => {
                    let sent = sent_hist[p][upto];
                    path_state[p]
                        .sender_truth_release_delta(sent, tracker[p].total_received)
                }
                // No ack-arm term: `expire_in_flight` above is the release.
                Release::OneToOne => 0,
            };
            rel(&mut path_state[p], lost);

            // The gauge, against the truth it is supposed to be.
            let truth: u64 = disp[p][(d + 1)..=upto].iter().map(|(_, n, _, _)| *n as u64).sum();
            samples[p] += 1;
            infl_sum[p] += path_state[p].in_flight as f64;
            outstanding_sum[p] += truth as f64;
            if path_state[p].in_flight == 0 && truth > 0 {
                false_empty[p] += 1;
            }
        }
    }

    // Quiesce: the flow stops, and the sender loop keeps sweeping. This is
    // where "does the ledger close?" is actually asked — a gauge that only
    // reads zero because it saturated is not a closed ledger, which is why
    // `releases_wasted` is reported beside `infl_final`.
    for p in 0..2usize {
        clocks[p].advance(rtt * 8);
        let before_exp = path_state[p].in_flight;
        path_state[p].expire_in_flight();
        releases_req[p] += (before_exp - path_state[p].in_flight) as u64;
    }

    let _ = dropped;
    [0usize, 1].map(|p| LedgerRead {
        wire: wire[p],
        charges: charges[p],
        releases_req: releases_req[p],
        releases_wasted: releases_wasted[p],
        delivered: delivered[p],
        leak_per_delivered: (releases_req[p] as f64 - charges[p] as f64)
            / delivered[p].max(1) as f64,
        false_empty_frac: false_empty[p] as f64 / samples[p].max(1) as f64,
        infl_mean: infl_sum[p] / samples[p].max(1) as f64,
        outstanding_mean: outstanding_sum[p] / samples[p].max(1) as f64,
        infl_final: path_state[p].in_flight,
    })
}

/// Direction 1 (wire v9) — the shipped counter-delta release no longer
/// leaks at N = 2.
///
/// Through wire v8 the gap was read in the global `batch_seq`: at 50/50
/// striping every path's gap was exactly 2, so the lost arm released
/// `received` on top of the delivery arm's `received` (≈1 extra slot per
/// delivered symbol at c7, ≈5 on c8's slow leg) and the gauge sat on the
/// floor (`in_flight == 0` on > 90% of acks with a full window outstanding).
/// v9 keys the gap on the per-path `path_seq`, so the lost arm releases
/// exactly the batches lost on THIS path: every charge is released once
/// (the trailing lost batches by the quiesce sweep), the leak is zero, and
/// the gauge reads the window. Absolute bounds at both multipath cells and
/// the N = 1 control.
#[test]
fn v9_counter_delta_release_closes_the_ledger_at_n2() {
    let leg = |share, eps| {
        xpath_ledger_model(60_000, 8, share, eps, 16, 40_000, Release::Legacy, false, 0)
    };
    for (cell, share, eps) in [
        ("c7", 2u32, [0.0055, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
        ("n1", 1, [0.0055, 0.0]),
    ] {
        let r = leg(share, eps);
        let legs: &[usize] = if share == 1 { &[0] } else { &[0, 1] };
        for &p in legs {
            let x = r[p];
            eprintln!("[v9-ledger] {cell} p{p}: {x:?}");
            assert!(x.delivered > 10_000, "{cell} p{p}: the model must run");
            assert!(
                x.outstanding_mean > 100.0,
                "{cell} p{p}: the model must hold a real in-flight window ({:.1})",
                x.outstanding_mean
            );
            assert_eq!(
                x.releases_req, x.charges,
                "{cell} p{p}: every charge released exactly once (v8 over-released \
                 ≈1 slot/delivered at c7, ≈5 at c8's slow leg)"
            );
            assert_eq!(x.infl_final, 0, "{cell} p{p}: the ledger closes at quiesce");
            assert!(
                x.false_empty_frac < 0.05,
                "{cell} p{p}: the gauge must not sit on the floor (v8: > 0.9), read {:.3}",
                x.false_empty_frac
            );
        }
    }
}

/// The ledger readout. Not a gate — every number it prints is bounded by
/// one of the four tests around it. `cargo test -p raptorpath --lib -- --ignored ledger_readout
/// --nocapture`.
#[test]
#[ignore = "readout, not a gate"]
fn ledger_readout() {
    println!(
        "{:<6} {:<3} {:<10} {:>10} {:>10} {:>9} {:>9} {:>8} {:>8}",
        "cell", "p", "release", "charges", "releases", "leak/dl", "infl", "truth", "empty%"
    );
    for (cell, share, eps) in [
        ("c7", 2u32, [0.0055, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
        ("n1", 1, [0.0055, 0.0]),
    ] {
        for (name, rel) in [
            ("legacy", Release::Legacy),
            ("senttruth", Release::SentTruth),
            ("1to1", Release::OneToOne),
        ] {
            let r = xpath_ledger_model(60_000, 8, share, eps, 16, 40_000, rel, false, 0);
            for p in 0..(if share == 1 { 1 } else { 2 }) {
                let x = r[p];
                println!(
                    "{cell:<6} {p:<3} {name:<10} {:>10} {:>10} {:>9.3} {:>9.1} {:>8.1} {:>7.1}%",
                    x.charges,
                    x.releases_req,
                    x.leak_per_delivered,
                    x.infl_mean,
                    x.outstanding_mean,
                    x.false_empty_frac * 100.0
                );
            }
        }
    }
    for (name, rel, chg) in [
        ("legacy/off", Release::Legacy, false),
        ("legacy/chg", Release::Legacy, true),
        ("1to1/off", Release::OneToOne, false),
        ("1to1/chg", Release::OneToOne, true),
    ] {
        let r =
            xpath_ledger_model(60_000, 8, 2, [0.0055, 0.0055], 16, 40_000, rel, chg, 5);
        println!(
            "c7-recov p0 {name:<12} wire {:>7} charges {:>7} releases {:>7} wasted {:>6} infl {:>6.1}",
            r[0].wire, r[0].charges, r[0].releases_req, r[0].releases_wasted, r[0].infl_mean
        );
    }
}

/// The refuted candidate, reproduced.
///
/// Sourcing the lost-symbol release from the same clean pair
/// `RWM_LOSS_SENT_TRUTH` gives the loss estimator does not work, and the
/// refutation is arithmetic: with
/// every send charged, releasing `d_received + (d_sent − d_received)`
/// telescopes to `in_flight == outstanding_at_cursor_init`, a constant —
/// zero, for cursors that start at zero. `d_sent − d_received` is
/// `loss + Δ(outstanding)`, so releasing on it releases the in-flight
/// window itself.
///
/// This test asserts the refutation absolutely: the candidate reproduces
/// the shipped defect's own signature (gauge pinned on the floor while the
/// path is loaded), at both multipath geometries and at the N = 1 control
/// — where the shipped term is honest, so the candidate would regress a
/// cell that has no defect. The clean pair works for a ratio and does not
/// transfer to a ledger.
#[test]
fn sender_truth_release_pins_the_gauge_on_the_floor() {
    for (cell, share, eps) in [
        ("c7", 2u32, [0.0055, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
        ("n1", 1, [0.0055, 0.0]),
    ] {
        let r = xpath_ledger_model(
            60_000, 8, share, eps, 16, 40_000, Release::SentTruth, false, 0,
        );
        let legs: &[usize] = if share == 1 { &[0] } else { &[0, 1] };
        for &p in legs {
            let x = r[p];
            assert!(x.delivered > 10_000, "{cell} p{p}: the model must run");
            assert!(
                x.outstanding_mean > 100.0,
                "{cell} p{p}: the model must hold a real in-flight window"
            );
            assert!(
                x.infl_mean < 0.05 * x.outstanding_mean,
                "{cell} p{p}: the sender-truth release must PIN the gauge on \
                 the floor — that is the refutation; read {:.2} against a \
                 true outstanding mean of {:.1}",
                x.infl_mean,
                x.outstanding_mean
            );
            assert!(
                x.false_empty_frac > 0.9,
                "{cell} p{p}: the candidate must reproduce the legacy \
                 defect's own signature, read {:.3}",
                x.false_empty_frac
            );
        }
    }
}

/// Direction 2 — under `RWM_RELEASE_1TO1` the gauge reads the in-flight
/// window, and the ledger closes at quiesce.
///
/// The contaminated ack-arm term is deleted and the whole lost-symbol
/// release is `expire_in_flight`'s sweep of the charge log at RFC 9002
/// §6.1.2's kTimeThreshold, `9/8 × SRTT`. Because that sweep pops the very
/// entries `charge_in_flight` pushed, the release is 1:1 with the charge by
/// construction however the paths are striped — and because the horizon is
/// the RTT scale rather than `max(4×SRTT, 250 ms)`, the gauge
/// settles at the in-flight window instead of a quarter-second of send
/// rate.
///
/// The bound is the law's own shape, not a tuned number. The equilibrium
/// occupancy is `horizon × rate`, the truth is `RTT × rate`, so the gauge
/// must read `9/8 ×` the truth — asserted as `[1.0, 1.3]×`, one-sided in
/// the conservative direction (it may over-read the wire's occupancy, which
/// narrows `available()`; it may never under-read it, which is the leak).
/// Asserted at both multipath geometries and at the N = 1 control.
#[test]
fn release_1to1_makes_the_gauge_read_the_in_flight_window() {
    for (cell, share, eps) in [
        ("c7", 2u32, [0.0055, 0.0055]),
        ("c8", 6, [0.0055, 0.0196]),
        ("n1", 1, [0.0055, 0.0]),
    ] {
        let r = xpath_ledger_model(
            60_000, 8, share, eps, 16, 40_000, Release::OneToOne, false, 0,
        );
        let legs: &[usize] = if share == 1 { &[0] } else { &[0, 1] };
        for &p in legs {
            let x = r[p];
            assert!(x.delivered > 10_000, "{cell} p{p}: the model must run");
            // 1:1 — an equality, and it is what the other two arms fail.
            assert_eq!(
                x.releases_req, x.charges,
                "{cell} p{p}: every slot charged must be released exactly \
                 once (charges {} releases {})",
                x.charges, x.releases_req
            );
            assert_eq!(
                x.releases_wasted, 0,
                "{cell} p{p}: a 1:1 ledger can never waste a release"
            );
            assert_eq!(
                x.infl_final, 0,
                "{cell} p{p}: in_flight must return to ZERO at quiesce"
            );
            // The gauge reads the window, and reads it high, never low.
            let ratio = x.infl_mean / x.outstanding_mean;
            assert!(
                (1.0..1.3).contains(&ratio),
                "{cell} p{p}: the gauge must read 9/8 × the in-flight window \
                 — read {:.1} against a true outstanding mean of {:.1} \
                 ({ratio:.3}×)",
                x.infl_mean,
                x.outstanding_mean
            );
            assert!(
                x.false_empty_frac < 0.01,
                "{cell} p{p}: the gauge must not sit on the floor, read {:.3}",
                x.false_empty_frac
            );
        }
    }
}

/// The charge defect, both directions — the two recovery channels reach
/// the wire un-metered, and `RWM_CHARGE_RECOVERY` is exactly the
/// difference.
///
/// Asserted as separate facts so a flip of either gate alone is bounded
/// rather than guessed:
///
///   * charge off: `charges < wire`, and the deficit is the recovery
///     channel — an equality by construction of the model;
///   * charge on: `charges == wire`, so the gauge counts what flew;
///   * release 1:1 + charge off: the ledger is still 1:1 against what was
///     charged, and under-counts the wire by exactly the un-metered
///     recovery — bounded, and in the conservative direction, where the
///     shipped release's error is an over-release unbounded in the path
///     count;
///   * both on: `charges == wire == releases`, and `in_flight` returns to
///     zero at quiesce. That is the SF bench's `Acct::Traffic`
///     configuration.
#[test]
fn charge_recovery_closes_the_un_metered_wire_and_composes_with_the_release() {
    // 5 recovery symbols per drop: the SACK-gap retransmit plus a NACK
    // repair margin of 4, which is the margin the contaminated loss
    // estimate buys (`ceil(retransmitted × ε̂)` at ε̂ ≈ 0.5).
    let recov = 5u32;
    let run = |release: Release, charge: bool| {
        xpath_ledger_model(
            60_000, 8, 2, [0.0055, 0.0055], 16, 40_000, release, charge, recov,
        )
    };
    let off = run(Release::Legacy, false);
    let charged = run(Release::Legacy, true);
    let rel_only = run(Release::OneToOne, false);
    let both = run(Release::OneToOne, true);

    for p in 0..2usize {
        // Measurement-discipline rule 1: the channel must have fired.
        assert!(
            off[p].wire > off[p].charges,
            "p{p}: the recovery channel never reached the wire"
        );
        let recov_syms = off[p].wire - off[p].charges;
        assert!(
            recov_syms > 500,
            "p{p}: too little recovery traffic to bound ({recov_syms})"
        );
        // Direction 2: with the gate on, every wire symbol is charged.
        assert_eq!(
            charged[p].charges, charged[p].wire,
            "p{p}: under RWM_CHARGE_RECOVERY the charge must equal the wire \
             (charges {} wire {})",
            charged[p].charges, charged[p].wire
        );
        // Release alone: the ack arm's contaminated term is gone, but the
        // delivery arm still releases the arrival of every recovery symbol
        // the sender never charged for — so the residual imbalance is
        // bounded by the un-metered recovery and by nothing else. That
        // residual is the charge defect's, not the release law's, and it is why the
        // two gates are stated as a composition rather than a choice.
        assert!(
            rel_only[p].releases_req >= rel_only[p].charges
                && rel_only[p].releases_req - rel_only[p].charges <= recov_syms,
            "p{p}: with the charge gate off the residual must be bounded by \
             the un-metered recovery ({recov_syms}) — charges {} releases {}",
            rel_only[p].charges,
            rel_only[p].releases_req
        );
        assert!(
            rel_only[p].wire - rel_only[p].releases_req <= recov_syms,
            "p{p}: release-only must under-count the wire by at most the \
             un-metered recovery"
        );
        // Both: charges == wire == releases, and the ledger closes.
        assert_eq!(both[p].charges, both[p].wire);
        assert_eq!(
            both[p].releases_req, both[p].wire,
            "p{p}: with both gates the ledger must balance against the WIRE"
        );
        assert_eq!(both[p].releases_wasted, 0);
        assert_eq!(
            both[p].infl_final, 0,
            "p{p}: in_flight must return to ZERO at quiesce"
        );
        // And the shipped arm, for contrast. Through wire v8 it leaked past
        // the source-only ≈1 slot per delivered symbol. Under v9 the
        // per-path gap no longer reads the other path, so it never
        // over-releases: the recovery batch that follows a lost source batch
        // prices the gap at ITS size (1, not 8) — an under-release — the
        // un-charged recovery deliveries release theirs, and the RFC 9002
        // sweep releases whatever the log still holds. Nothing is released
        // into an empty gauge, so the requested release equals the charge
        // exactly. (The un-metered recovery's own error is the gauge
        // under-read bounded by the release-only rows above.)
        assert_eq!(
            off[p].releases_wasted, 0,
            "p{p}: under v9 the legacy arm never releases into an empty gauge"
        );
        assert_eq!(
            off[p].releases_req, off[p].charges,
            "p{p}: under v9 the legacy arm's requested release equals its \
             charge (v8: a leak > 1 slot per delivered symbol)"
        );
    }
}

/// The `[PIPE]` echo names the one route the engine takes, at every named
/// point, with the backend `pipeline_backend` pins for the resolved default
/// config. The battery greps exactly these tokens, so the format is pinned.
#[test]
fn pipe_echo_names_the_route_the_engine_takes() {
    for (hint, h) in [
        (ProtocolHint::Realtime, "realtime"),
        (ProtocolHint::Auto, "auto"),
        (ProtocolHint::Bulk, "bulk"),
    ] {
        let cfg = crate::config::RaptorpathConfig { protocol_hint: Some(h.into()), ..Default::default() };
        let (pc, _) = crate::config::resolve(&cfg).expect("resolves");
        let backend = pipeline_backend(&pc).expect("the default config has a window codec");
        assert_eq!(
            pipe_echo_line(hint, backend),
            format!("[PIPE] pipeline=window backend=Rlc hint={h}")
        );
    }
}

// ── Plan 2b: the channel's path set is membership, not "can send now" ──
//
// Every "what is the channel / how big is the pool" reader must return the
// SAME value whether or not the paths are cwnd-full. `active_paths()` (the
// placement filter: `available() > 0`) is empty exactly while a wire-bound
// sender's paths carry the transfer, so a reader on it jumps to its
// empty-set fallback (Σ = 0 → boot cap 128, ε = 0, P_lost constants
// (0.05, 0.005, 0.0), NACK inputs (0.0, None), react-cap 50 ms) at the
// moment the channel is busiest. Each test below: a warm two-path channel,
// the quantity read unsaturated, both paths driven to `in_flight = cwnd`,
// the quantity read again — ABSOLUTE equality, plus "not the fallback".

/// A warm, heterogeneous two-path channel: path 0 = SRTT 20 ms / ε≈0.02,
/// path 1 = SRTT 60 ms / ε≈0.10 (the worst-ε path). Both BDP anchors warm
/// (the c1-shape send-interval warm-up of the DH store-cap test: RTprop
/// 2 ms, deliveries lagging sends by ~3 ms).
fn channel_fixture() -> Scheduler {
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    sched.add_path(1);
    for id in [0u32, 1] {
        sched.path_mut(id).unwrap().record_rtt_sample(Duration::from_millis(2));
    }
    let step = Duration::from_micros(41);
    for seq in 0..4000u64 {
        for id in [0u32, 1] {
            let p = sched.path_mut(id).unwrap();
            p.on_src_sent(seq, false);
            if seq >= 72 {
                p.on_src_delivered_seq(seq - 72);
            }
        }
        clock.advance(step);
    }
    for (id, ms, lost) in [(0u32, 20u64, 2u32), (1, 60, 10)] {
        let p = sched.path_mut(id).unwrap();
        for _ in 0..32 {
            p.record_rtt_sample(Duration::from_millis(ms));
            p.estimator.record_rtt(Duration::from_millis(ms));
            p.estimator.record_batch(100, 100 - lost);
        }
    }
    for id in [0u32, 1] {
        let p = sched.path(id).unwrap();
        assert!(p.copa_bdp_anchor().is_some(), "path {id}: BDP anchor must be warm");
        assert!(p.available() > 0, "path {id}: fixture starts unsaturated");
    }
    sched
}

/// Drive every path to `in_flight = cwnd` (the wire-bound resting state).
fn saturate_channel(sched: &mut Scheduler) {
    for id in [0u32, 1] {
        let p = sched.path_mut(id).unwrap();
        let cw = p.cwnd;
        p.charge_in_flight(cw);
        assert_eq!(p.available(), 0, "path {id} must be cwnd-full");
    }
    assert!(sched.active_paths().is_empty(), "precondition: placement set empty");
    let mut live = sched.live_paths();
    live.sort_unstable();
    assert_eq!(live, vec![0, 1], "precondition: both paths still members");
}

/// Read `f` on the fixture unsaturated, then saturated.
fn read_both<T>(f: impl Fn(&Scheduler) -> T) -> (T, T) {
    let mut sched = channel_fixture();
    let before = f(&sched);
    saturate_channel(&mut sched);
    (before, f(&sched))
}

#[test]
fn saturated_channel_store_cap_sigma_has_no_boot_cliff() {
    // (Σ anchor, number of honest-cap slots) — the plain dyn-cap's inputs
    // (the sender's own collector).
    let (un, sat) = read_both(|s| {
        let (bdp, slots) = store_cap_pool_inputs(s, true);
        (bdp, slots.len())
    });
    assert!(un.0 > 0.0 && un.1 == 2, "unsaturated: both anchors in the Σ ({un:?})");
    assert_eq!(
        sat, un,
        "saturated: the store-cap Σ must not lose the cwnd-full paths (Σ = 0 is the boot-cap-128 cliff)"
    );
}

#[test]
fn saturated_channel_keeps_the_worst_eps_pick() {
    let (un, sat) = read_both(|s| {
        worst_eps_channel_path(s).map(|(id, p)| (id, p.estimator.loss_rate()))
    });
    assert_eq!(un.map(|x| x.0), Some(1), "path 1 is the worst-ε path");
    assert_eq!(sat, un, "saturated: the worst-ε pick must not vanish");
    let (un, sat) = read_both(channel_worst_loss_rate);
    assert!(un > 0.05, "worst ε ≈ 0.10 ({un})");
    assert_eq!(sat, un, "saturated: the channel's worst ε must not fall to 0");
    // The emit/rate reader's own entry point routes through the same pick.
    let (un, sat) = read_both(|s| super::emit_source::worst_eps_path(s).map(|(id, _)| id));
    assert_eq!((un, sat), (Some(1), Some(1)), "worst_eps_path must survive saturation");
}

#[test]
fn saturated_channel_keeps_the_p_lost_inputs() {
    let (un, sat) = read_both(p_lost_inputs);
    assert_ne!(un, (0.05, 0.005, 0.0), "unsaturated: measured, not the fallback");
    assert_eq!(
        sat, un,
        "saturated: P_lost inputs must be the measured worst path, not (0.05, 0.005, 0.0)"
    );
}

#[test]
fn saturated_channel_keeps_the_nack_budget_inputs() {
    let (un, sat) = read_both(nack_congestion_inputs);
    assert!(un.0 > 0.0 && un.1.is_some(), "unsaturated: measured ({un:?})");
    assert_eq!(sat, un, "saturated: NACK congestion inputs must not fall to (0.0, None)");
    // The ADR-0050 budget's estimator inputs.
    let (un, sat) = read_both(|s| {
        worst_eps_estimator(s).map(|e| (e.predictive_loss_upper(0.99), e.nack_effectiveness()))
    });
    assert!(un.is_some(), "unsaturated: an estimator is picked");
    assert_eq!(sat, un, "saturated: the NACK budget must still see the worst estimator");
}

#[test]
fn saturated_channel_keeps_the_react_cap_srtt_and_pool_sums() {
    let (un, sat) = read_both(channel_max_srtt_us);
    assert!(un.is_some_and(|v| v >= 50_000), "max SRTT = the 60 ms path ({un:?})");
    assert_eq!(sat, un, "saturated: react-cap SRTT must not fall to the 50 ms default");
    let (un, sat) = read_both(channel_max_rtprop_s);
    assert!(un > 0.0, "max RTprop is measured ({un})");
    assert_eq!(sat, un, "saturated: generation RTprop must not fall to 0");
    let (un, sat) = read_both(channel_bdp_anchor_sum);
    assert!(un > 0.0);
    assert_eq!(sat, un, "saturated: the infl-BDP Σ must not fall to 0");
}

#[test]
fn shutdown_and_window_start_reach_a_saturated_path() {
    let (mut un, mut sat) = read_both(control_broadcast_paths);
    un.sort_unstable();
    sat.sort_unstable();
    assert_eq!(un, vec![0, 1]);
    assert_eq!(sat, vec![0, 1], "WindowStart / Shutdown must go out on cwnd-full paths");
    let (mut un, mut sat) = read_both(channel_paths);
    un.sort_unstable();
    sat.sort_unstable();
    assert_eq!(sat, un, "the channel set is membership");
}

// ── S10: the loss-estimator feed graph (feeds A, C, D) ───────────────────
//
// Driven through the real control-message dispatch (`handle_control_message`
// with a real `ControlCtx`) and the receiver's real feed seam, so each test
// proves the wiring, not a re-implementation of it.

/// The TX estimator's observable state, compared whole: EWMA, Beta mean,
/// BOCD predictive upper, cumulative fed loss, and the GE HMM (Debug).
fn s10_tx_state(e: &crate::control::estimator::LossEstimator) -> (f64, f64, f64, f64, String) {
    (
        e.loss_rate(),
        e.loss_rate_mean(),
        e.predictive_loss_upper(0.95),
        e.cumulative_loss(),
        format!("{:?}", e.ge_estimator()),
    )
}

/// Run `f` with a real `ControlCtx` over a one-path scheduler (path 0).
async fn s10_with_ctx<R>(f: impl FnOnce(&super::control_msg::ControlCtx<'_>) -> R) -> R {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let transport = Arc::new(
        QuicTransport::new(&[addr], false, None)
            .await
            .expect("loopback endpoint binds"),
    );
    let scheduler = Arc::new(crate::scheduler::SchedMutex::new(Scheduler::new(Arc::new(WallClock))));
    scheduler.lock().add_path(0);
    let stats = Arc::new(SharedStats::new());
    stats.add_path(0);
    let ctx = super::control_msg::ControlCtx {
        scheduler: &scheduler,
        transport: &transport,
        stats: &stats,
        nack_tx: None,
        peer_window_ack: None,
        ack_wake: None,
        deficit_tx: None,
        request_tx: None,
        sack_tx: None,
        copa_feed: None,
        mstar_anchor: true,
    };
    f(&ctx)
}

/// S10 / F3 — the peer's `PathReport.loss_rate` never enters the local
/// estimator. It is the peer's own estimator value (a duplicate of the
/// ack feed, and a loop: each side's report fed the other's estimator),
/// injected as a truncated 100-trial pseudo-batch. Through 949e06b a
/// report with loss 0.01 fed (100, 99).
#[tokio::test]
async fn s10_path_report_loss_leaves_the_estimator_unchanged() {
    s10_with_ctx(|ctx| {
        let before = s10_tx_state(&ctx.scheduler.lock().path(0).unwrap().estimator);
        super::control_msg::handle_control_message(
            0,
            ControlMessage::PathReport {
                path_id: 0,
                loss_rate: 0.01,
                avg_rtt_us: 20_000,
                throughput_bps: 0.0,
                jitter_us: 0,
                symbols_sent: 0,
                symbols_received: 0,
            },
            ctx,
        );
        let after = s10_tx_state(&ctx.scheduler.lock().path(0).unwrap().estimator);
        assert_eq!(before, after, "a peer report must not feed the estimator");
        assert_eq!(
            ctx.stats.path(0).unwrap().peer_loss_rate_e6.load(Ordering::Relaxed),
            10_000,
            "the peer's reported loss is kept as a monitoring value"
        );
    })
    .await;
}

/// S10 / F2 (feed A) — the per-batch `Ack` arm carries the late-arrival
/// credit too. After F1 a late arrival's Ack reads `(expected 0,
/// received N)`; the phantom gap its absence charged arrives on the next
/// Ack as `(2N, N)`. The pair sums to (2N, 2N): no loss. Through 949e06b
/// the `(0, N)` Ack was skipped (`if le > 0`) and the next fed 50 % loss.
#[tokio::test]
async fn s10_per_batch_ack_carries_the_late_arrival_credit() {
    s10_with_ctx(|ctx| {
        for (e, r) in [(4u32, 4u32), (0, 4), (8, 4), (4, 4)] {
            super::control_msg::handle_control_message(
                0,
                ControlMessage::Ack {
                    block_id: 0,
                    batch_seq: 0,
                    received_ids: (0..r).collect(),
                    echo_send_timestamp_us: 0,
                    expected_count: e,
                    received_count: r,
                },
                ctx,
            );
        }
        let s = ctx.scheduler.lock();
        let est = &s.path(0).unwrap().estimator;
        assert_eq!(est.cumulative_loss(), 0.0, "Σe = Σr = 16: no loss was fed");
    })
    .await;
}

/// S10 / F4 — the receiver's own feed (D) is INCOMING-direction loss. It
/// goes to the RX slot only; the TX estimator (EWMA, Beta, BOCD, GE,
/// cumulative), which this endpoint's sender role reads for the OUTGOING
/// direction, is untouched. Through 949e06b the receiver wrote the TX
/// fields and `rx_loss_rate()` had no production feed.
#[test]
fn s10_receiver_feed_goes_to_rx_and_leaves_tx_untouched() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    let before = s10_tx_state(&sched.path(0).unwrap().estimator);
    for _ in 0..20 {
        super::receiver::record_incoming_loss(&mut sched, 0, 10, 8);
    }
    let est = &sched.path(0).unwrap().estimator;
    assert_eq!(s10_tx_state(est), before, "incoming loss must not reach the TX estimator");
    assert!(
        est.rx_loss_rate() > 0.1,
        "incoming loss reads on the RX slot ({})",
        est.rx_loss_rate()
    );
}

/// Threading P1, D1 — an inbound `WindowAck` wakes the local window sender.
/// Through 8d7d8c1 the sender, paused on the store/cwnd brake or on an empty
/// pacing bucket, learned of an ack only from a 1 ms `sleep_until` poll
/// (rounded up by tokio to 1–2 ms); `on_window_ack` signalled nothing. The
/// absolute invariant: after one `WindowAck` the sender's `Notify` holds a
/// permit (`notified()` resolves without any timer advance); after a
/// `PathReport` it does not (the wake is the ack, not any control message).
#[tokio::test]
async fn p1_d1_a_window_ack_wakes_the_sender() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let transport = Arc::new(
        QuicTransport::new(&[addr], false, None)
            .await
            .expect("loopback endpoint binds"),
    );
    let scheduler = Arc::new(crate::scheduler::SchedMutex::new(Scheduler::new(Arc::new(WallClock))));
    scheduler.lock().add_path(0);
    let stats = Arc::new(SharedStats::new());
    stats.add_path(0);
    let ack = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let wake = Arc::new(super::control_msg::AckWake::new());
    let ctx = super::control_msg::ControlCtx {
        scheduler: &scheduler,
        transport: &transport,
        stats: &stats,
        nack_tx: None,
        peer_window_ack: Some(&ack),
        ack_wake: Some(&wake),
        deficit_tx: None,
        request_tx: None,
        sack_tx: None,
        copa_feed: None,
        mstar_anchor: true,
    };
    // A stored permit resolves on the first poll: a zero timeout polls the
    // inner future once before it looks at the (paused) clock.
    async fn permit(w: &super::control_msg::AckWake) -> bool {
        tokio::time::timeout(std::time::Duration::ZERO, w.notified()).await.is_ok()
    }
    super::control_msg::handle_control_message(
        0,
        ControlMessage::PathReport {
            path_id: 0,
            loss_rate: 0.0,
            avg_rtt_us: 20_000,
            throughput_bps: 0.0,
            jitter_us: 0,
            symbols_sent: 0,
            symbols_received: 0,
        },
        &ctx,
    );
    assert!(!permit(&wake).await, "a PathReport must not wake the sender");
    super::control_msg::handle_control_message(
        0,
        ControlMessage::WindowAck {
            next_expected: 7,
            received_above: 0,
            sack_ranges: Vec::new(),
            echo_send_timestamp_us: 0,
            jitter_us: 0,
            cumulative_received: 0,
            cum_expected: 0,
            cum_received: 0,
        },
        &ctx,
    );
    assert_eq!(ack.load(Ordering::Relaxed), 7, "the ack point is published");
    assert!(
        permit(&wake).await,
        "a WindowAck must leave a wake permit for the sender (read through a \
         zero timeout: no timer can have fired)"
    );
    assert!(!permit(&wake).await, "one ack, one permit");
    assert_eq!(
        wake.acks.load(Ordering::Relaxed),
        1,
        "the ack counter `wake[timer_acked]` is read against counts exactly the WindowAcks"
    );
}

/// Threading P1, D1 — the routing half (rule 1: the wiring between layers
/// actually routes there). The receiver task hands its `ControlCtx` the
/// sender's `Notify` (not `None`), the sender's `select!` awaits that same
/// `Notify` under exactly the paused / pacing-dry guards of the two 1 ms
/// polls it shortcuts, and its wake is charged to its own bucket (8).
#[test]
fn p1_d1_the_receiver_routes_the_wake_and_the_sender_awaits_it() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net");
    let recv = std::fs::read_to_string(dir.join("receiver.rs")).expect("receiver.rs");
    let send = std::fs::read_to_string(dir.join("mod.rs")).expect("mod.rs");
    assert!(
        recv.contains("ack_wake: Some(&recv_ack_wake),"),
        "the receiver's data-loop ControlCtx must carry the sender's Notify"
    );
    assert!(
        send.contains("let recv_ack_wake = ack_wake.clone();")
            && send.contains("let sender_ack_wake = ack_wake.clone();"),
        "one Notify, cloned to both tasks"
    );
    let arm = send
        .find("ack_wake.notified(),")
        .expect("the sender's select! must await the ack wake");
    let tail = &send[arm..arm + 200];
    assert!(
        tail.contains("if tx_paused || (pol.cc_pace && st.src_tokens < 1.0) => { wait_arm = 8;"),
        "the ack arm must be guarded exactly like the two polls it shortcuts \
         and charge bucket 8: {tail}"
    );
}

/// The source text of `path` relative to `src/`, with every `#[cfg(test)]`
/// module after the first marker cut off and all whitespace removed.
fn p1_squashed_src(path: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(path);
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
    let body = match src.find("#[cfg(test)]") {
        Some(i) => &src[..i],
        None => &src[..],
    };
    body.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Threading P1, D11 — the sender's tail-sweep deadline is ONE pinned
/// `Sleep`, `reset()` when the deadline moves, not a fresh `sleep_until`
/// future built (and, about half the time, registered in and removed from
/// tokio's global timer wheel) on every loop iteration.
#[test]
fn p1_d11_the_tail_deadline_is_one_persistent_sleep() {
    let s = p1_squashed_src("net/mod.rs");
    assert!(
        !s.contains("Some(d)=>tokio::time::sleep_until(d).await,None=>std::future::pending().await,}}=>{wait_arm=6;"),
        "the per-iteration tail sleep future is back"
    );
    let pin = s.find("tokio::pin!(tail_sleep);").expect("one pinned tail Sleep");
    let lp = s[pin..].find("loop{").expect("the sender loop follows") + pin;
    let reset = s[lp..].find("tail_sleep.as_mut().reset(").expect("reset inside the loop") + lp;
    let arm = s[reset..]
        .find("_=&muttail_sleep,iftail_deadline.is_some()=>{wait_arm=6;")
        .expect("the tail arm awaits the persistent Sleep under the deadline guard");
    assert!(arm > 0);
}

/// Threading P1, D17 — `BatchCounter` takes no mutex on the emission path:
/// per-path sequences are atomics (a bounded dense table; ids past it are
/// the cold fallback).
#[test]
fn p1_d17_batch_counter_has_no_hot_mutex() {
    let s = p1_squashed_src("net/mod.rs");
    let a = s.find("pub(crate)structBatchCounter{").expect("BatchCounter");
    let b = s[a..].find('}').expect("struct end") + a;
    let fields = &s[a..b];
    assert!(
        !fields.contains("per_path:"),
        "per-path numbering must not be the Mutex<HashMap> `per_path`: {fields}"
    );
    assert!(
        fields.contains("dense:[AtomicU64;BATCH_DENSE_PATHS],"),
        "the dense per-path atomic table must exist: {fields}"
    );
}

/// D17's semantics: identical numbering to the `Mutex<HashMap>` it replaces
/// — one global sequence, and per path 0, 1, 2, … in call order — for ids
/// inside and outside the dense table, interleaved.
#[test]
fn p1_d17_batch_counter_numbering_is_per_path_and_global() {
    let c = BatchCounter::new();
    let ids = [0u32, 1, 63, 64, 1000, u32::MAX, 0, 64, 1, u32::MAX, 63, 0];
    let mut want: std::collections::HashMap<u32, u64> = Default::default();
    for (g, &id) in ids.iter().enumerate() {
        let w = want.entry(id).or_insert(0);
        assert_eq!(c.next(id), (g as u64, *w), "call {g} on path {id}");
        *w += 1;
    }
}

/// Threading P1, D16 — the hot tasks reach a path's monitoring stats
/// without the `SharedStats` RwLock, the linear scan and the `Arc` clone:
/// `SharedStats::path_ref` (a dense `OnceLock` table filled at `add_path`).
/// Zero `stats.path(` calls remain in the per-datagram / per-ack files.
#[test]
fn p1_d16_hot_files_use_the_lock_free_stats_ref() {
    for f in ["net/control_msg.rs", "net/emit_source.rs", "net/receiver.rs", "net/sender_phases.rs", "net/mod.rs"] {
        let s = p1_squashed_src(f);
        assert_eq!(s.matches("stats.path(").count(), 0, "{f}: a locking SharedStats::path call remains");
    }
    assert!(
        p1_squashed_src("monitor/stats.rs").contains("pubfnpath_ref(&self,id:u32)->Option<PathStatsRef<'_>>"),
        "SharedStats::path_ref must exist"
    );
}

// ── T2 (emission-batching scope, status §8): a same-path burst leaves the
// v9 per-path loss feed exact, in any interleaving ─────────────────────────
//
// The old `live_paths == 1` scope was added for the wire-v8 striping-gap
// misread, "amplified by longer same-path arrival runs". With the v9
// per-path `path_seq` a run of 64 symbols on A advances A's sequence
// contiguously and leaves B's untouched: every delivered symbol's pair is
// (1, 1) on its own path, whatever the run structure. The v8 contrast (one
// global counter for both paths) shows the artefact this guards against.

/// Feed `order` (a path id per emitted symbol) through the sender's
/// `BatchCounter` and the receiver's per-path trackers (lossless, in order),
/// returning per path (Σexpected, Σreceived) through `credit_ack_pair` and
/// every per-batch pair. `v8` reads the gap in the global counter instead.
fn t2_feed(order: &[u32], v8: bool) -> std::collections::HashMap<u32, (u64, u64, Vec<(u32, u32)>)> {
    use crate::scheduler::{MockClock, PathState};
    let clock = Arc::new(MockClock::new());
    let counter = BatchCounter::new();
    let mut trackers: std::collections::HashMap<u32, PathBatchTracker> = Default::default();
    let mut paths: std::collections::HashMap<u32, PathState> = Default::default();
    let mut out: std::collections::HashMap<u32, (u64, u64, Vec<(u32, u32)>)> = Default::default();
    for &p in order {
        let (global, path_seq) = counter.next(p);
        let seq = if v8 { global } else { path_seq };
        let (e, r) = trackers.entry(p).or_insert_with(PathBatchTracker::new).record_batch(seq, 1);
        let (e, r) = paths
            .entry(p)
            .or_insert_with(|| PathState::new(p, clock.clone()))
            .credit_ack_pair(e, r);
        let o = out.entry(p).or_insert((0, 0, Vec::new()));
        o.0 += e as u64;
        o.1 += r as u64;
        o.2.push((e, r));
    }
    for (p, t) in &trackers {
        let o = &out[p];
        assert_eq!((t.total_expected, t.total_received), (o.0, o.1), "credit is identity without reorder");
    }
    out
}

#[test]
fn t2_same_path_runs_are_exact_on_both_paths_in_any_interleaving() {
    // 64 on A then 4 on B; B first; alternating; long runs both ways; and a
    // pseudo-random interleaving (fixed LCG) of 1000 symbols.
    let mut orders: Vec<Vec<u32>> = vec![
        [vec![0; 64], vec![1; 4]].concat(),
        [vec![1; 4], vec![0; 64]].concat(),
        (0..128).map(|i| (i % 2) as u32).collect(),
        [vec![0; 64], vec![1; 64], vec![0; 3], vec![1; 1], vec![0; 64]].concat(),
    ];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    orders.push(
        (0..1000)
            .map(|_| {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                // Runs of random length: flip the path with p = 1/8.
                ((x >> 33) % 8 == 0) as u32
            })
            .scan(0u32, |cur, flip| {
                *cur ^= flip;
                Some(*cur)
            })
            .collect(),
    );
    for order in &orders {
        let out = t2_feed(order, false);
        for p in [0u32, 1] {
            let n = order.iter().filter(|&&q| q == p).count() as u64;
            let (e, r, pairs) = &out[&p];
            assert_eq!((*e, *r), (n, n), "path {p}: expected == received == sent");
            assert!(pairs.iter().all(|&pr| pr == (1, 1)), "path {p}: every pair (1, 1)");
        }
    }
}

#[test]
fn t2_v8_global_numbering_is_the_artefact_the_scope_guarded_against() {
    // The same 64-then-4-then-64 run read in the ONE global counter (wire
    // v8): A's second run lands after B's 4, so A is charged a gap of 5
    // (4 phantom losses) — expected > received on A with nothing lost.
    let order = [vec![0u32; 64], vec![1; 4], vec![0; 64]].concat();
    let out = t2_feed(&order, true);
    let (e, r, pairs) = &out[&0];
    assert_eq!(*r, 128);
    assert_eq!(*e, 132, "v8: 4 phantom losses charged to A in one lump");
    assert!(pairs.contains(&(5, 1)), "the lumpy (gap+1, 1) pair");
    // v9 on the identical order: exact.
    let out = t2_feed(&order, false);
    assert_eq!((out[&0].0, out[&0].1), (128, 128));
    assert_eq!((out[&1].0, out[&1].1), (4, 4));
}
