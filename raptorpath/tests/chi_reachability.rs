//! The completion exposure χ (paper §4.6, §4.9) is reachable through
//! `RWM_COMPLETION_EXPOSURE`, and without the arm the rate is byte-identical.
//! The production tunnel is an endless stream, so nothing sets χ by default;
//! with χ ≡ 0 the Bulk end of the dial has `δ_eff = ε̂`, `z_for_tail_target`
//! returns `−∞` and `controller_rate` returns exactly 0 — the r leg sits at
//! its corner. Clauses:
//!
//!   1. The gate echo, two-sided: `RWM_COMPLETION_EXPOSURE=1` on the armed
//!      arm and `=0` on the control.
//!   2. The mechanism executed (measurement-discipline rule 1): `[CHI]`
//!      reaches `max > 0.5` — χ is a survival function of
//!      `(T_rem − 1.5·srtt)/σ_ARQ`, so `max > ½` means `δ_eff` has left `ε̂`.
//!   3. The law reached the wire: coded output `cod > 0` on the armed arm.
//!   4. The control is inert and says so: `[CHI] n=0 max=0.0000`.
//!   5. Byte-identity disarmed, asserted directly against `controller_rate`.
//!
//! `RWM_COMPLETION_EXPOSURE` is absent on every shipped arm.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{f64_field, max_u64_token, require};

/// The base arm. `RWM_DIAG` carries `[CHI]`, `[SHEDH]` and `[DIAG] cod=`.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One object loopback run.
///
/// The size is set by the gauge's cadence: `[CHI]` is emitted every 1 s,
/// last line wins, so a run shorter than a second yields only the `t = 0`
/// line (read `n=0` whatever the arm did). Three 3 MB objects at the `c3`
/// cell's 20 Mbit/s take ≈ 1.2 s each, so every arm gets several emissions.
fn run(extra: &[(&str, &str)]) -> String {
    let mut env = ARM.to_vec();
    env.extend_from_slice(extra);
    // A lossy cell, so the rate law has something to price. `c3heavy`
    // (ε ≈ 5.8 %), not `c3` (ε ≈ 4.8 %): the glide's fully-exposed target is
    // `BULK_TAIL_BUDGET = 0.05`, so below 5 % loss the corner survives full
    // exposure and `r* = 0` whatever χ does (clause 5).
    let netem = [("RWM_L0_NETEM", "c3heavy"), ("RWM_L0_SEED", "42")];
    // Every assertion reads the client (sender) log; the server log is unused.
    let (cli, _srv) = loopback::transfer(loopback::Transfer {
        env: &env,
        client_env: &netem,
        bytes: "3000000",
        runs: "3",
        ..Default::default()
    });
    cli
}

// ── 1 — The armed arm: χ is fed, reaches the glide, and reaches the wire ──

#[test]
fn the_completion_exposure_arm_feeds_chi_and_the_rate_reaches_the_wire() {
    let cli = run(&[("RWM_COMPLETION_EXPOSURE", "1")]);

    // (1) The gate echo: a missing gauge below can only be read as an
    // unreached emission site, never as an unset gate.
    let gates = require(&cli, "[GATES]", "the engine never echoed its gates");
    assert!(
        gates.contains("RWM_COMPLETION_EXPOSURE=1"),
        "the χ arm did not arm:\n{gates}"
    );
    // The feed's own mechanism-liveness echo (the perf client's half).
    assert!(
        cli.contains("completion-exposure feed ACTIVE"),
        "the perf client never published a feed, so the gate armed nothing"
    );

    // (2) The mechanism executed: χ > ½ means δ_eff has left ε̂.
    let chi = require(&cli, "[CHI] ", "the χ gauge is unreached — old engine?");
    assert!(
        f64_field(chi, "n=") > 0.0,
        "the rate site never evaluated χ: {chi}"
    );
    assert!(
        f64_field(chi, "max=") > 0.5,
        "χ never entered the glide's own region (max ≤ 0.5), so the arm ran \
         the shipped machine under a different label: {chi}"
    );
    assert!(
        f64_field(chi, "frac_gt_half=") > 0.0,
        "no evaluation reached χ > ½: {chi}"
    );

    // (3) The law reached the wire: a χ that moves the controller but emits
    // no coded symbol is a gauge, not an arm.
    assert!(
        max_u64_token(&cli, "cod=") > 0,
        "no coded symbol went on the wire, so r* never left the corner"
    );
}

// ── 2 — The control: absent means never read, and it says so ─────────────

#[test]
fn without_the_arm_chi_is_never_evaluated_and_the_gauge_says_so() {
    let cli = run(&[]);

    let gates = require(&cli, "[GATES]", "the engine never echoed its gates");
    assert!(
        gates.contains("RWM_COMPLETION_EXPOSURE=0"),
        "the control arm must be readable as OFF: {gates}"
    );
    assert!(
        !cli.contains("completion-exposure feed ACTIVE"),
        "the control arm must publish NO feed"
    );

    // The gauge fires on every arm (measurement-discipline rule 15); here
    // both fields must read zero: `n=0` (never evaluated) and `max=0` (no
    // value produced) are different failures.
    let chi = require(
        &cli,
        "[CHI] ",
        "the χ gauge must fire on the control arm too, or gate-off is not readable",
    );
    assert_eq!(f64_field(chi, "n="), 0.0, "χ was evaluated with the gate off: {chi}");
    assert_eq!(f64_field(chi, "max="), 0.0, "χ took a value with the gate off: {chi}");
    assert_eq!(f64_field(chi, "frac_gt_half="), 0.0, "{chi}");
}

// ── 3 — Disarmed byte-identity, asserted directly ────────────────────────

/// With χ = 0 the rate is exactly the corner, asserted against
/// `controller_rate` itself with `assert_eq!`, not a tolerance.
#[test]
fn with_chi_zero_the_bulk_rate_is_exactly_the_corner_it_always_was() {
    use raptorpath_math::{controller_rate, MassStats, RateInputs};
    let base = RateInputs {
        p_upper: 0.048,
        sigma2: 2.0,
        mean_burst: 2.5,
        mass: MassStats::default(),
        tail_provision: true,
        window: 64.0,
        t_symbols: 120.0,
        srtt: 0.04,
        t_sym: 1e-4,
        codec_overhead: 0.004,
        tail_target: 1e-3,
        bulk_late_is_fine: true,
        completion_exposure: 0.0,
        inner_feedback: 0.0,
        saturation_cap: true,
        max_overhead: 0.5,
    };
    // The corner, exactly: δ_eff = p ⇒ z = −∞ ⇒ r* = 0 identically.
    assert_eq!(controller_rate(&base), 0.0);
    // The corner holds across the estimator range, not at one point.
    for p in [0.001f64, 0.01, 0.05, 0.2, 0.5] {
        let mut i = RateInputs { p_upper: p, ..base };
        i.completion_exposure = 0.0;
        assert_eq!(controller_rate(&i), 0.0, "p={p}: the χ = 0 corner moved");
    }
    // χ > 0 leaves the corner, but only where `ε̂ > BULK_TAIL_BUDGET`: the
    // fully-exposed target is `δ_eff = 0.05`, so on a cleaner channel
    // `δ_eff/p ≥ 1`, `z = −∞` and `r* = 0` (paper §4.9). Asserted in both
    // directions so an inert battery verdict can be attributed to the budget
    // rather than to the wiring.
    for p in [0.001f64, 0.01, 0.048] {
        let armed = RateInputs { p_upper: p, completion_exposure: 1.0, ..base };
        assert_eq!(
            controller_rate(&armed),
            0.0,
            "p={p} < BULK_TAIL_BUDGET: full exposure must STILL be the corner \
             — if this moved, the 0.05 budget moved"
        );
    }
    for p in [0.08f64, 0.2, 0.4] {
        let armed = RateInputs { p_upper: p, completion_exposure: 1.0, ..base };
        assert!(
            controller_rate(&armed) > 0.0,
            "p={p} > BULK_TAIL_BUDGET: χ = 1 must leave the corner, or the arm \
             has nothing to measure"
        );
    }
    // The crossing is the budget itself — a threshold in the arithmetic of
    // one continuous law, not a branch in the code.
    assert_eq!(raptorpath_math::BULK_TAIL_BUDGET, 0.05);
}
