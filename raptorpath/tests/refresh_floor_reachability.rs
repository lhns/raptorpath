//! **paper 16.78 THE REFRESH-FLOOR LIFT — THE REACHABILITY GATE.**
//!
//! 16.77.8d established as arithmetic that the sender learns a hole CLOSED
//! from the **absence** of that hole in a **later** receiver report, so the
//! finest gap response it can time is one hole-refresh interval — and at
//! **four of five measured cells that interval sits AT OR ABOVE the median of
//! the hole-self-heal distribution it is supposed to be a quantile of**
//! (`[SUCC]` `orig` p50: `c1` 24.6 ms against a 25 ms floor; `c7` 30.7 ms and
//! `sc2` 98.3 ms against a 100 ms one). Until that cadence moves, **no
//! hold-down level `q` is commandable below the self-heal median** and the
//! whole sub-floor region of the `(q, refresh)` surface is unreadable.
//!
//! `RWM_REFRESH_FLOOR_US` is the named precondition. This binary asserts, in
//! the order it can fail, that it REACHES the receiver's cadence on the real
//! engine over a real lossy wire:
//!
//! 1. **THE GATE IS ECHOED, TWO-SIDED, AT BOTH ENDPOINTS.** `[GATES]` prints
//!    `RWM_REFRESH_FLOOR_US=<resolved us>` — a number on the armed arm and
//!    `unset` on the control. A missing cadence change below can then only be
//!    read as an unreached site and never as an unset gate.
//! 2. **THE RECEIVER'S GAUGE EXISTS AND THE SITE EXECUTED.**
//!    `[QCLK] site=receiver` with `evals > 0`. MEASUREMENT DISCIPLINE rule 1:
//!    prove the mechanism under test runs before reading anything it produced.
//! 3. **THE DELIVERED CADENCE IS BELOW THE SHIPPED RAIL — THE WIRING WITNESS,
//!    AND AN ABSOLUTE LAW INVARIANT.** The armed floor is 6 150 us, so the
//!    band is `[6.150, 24.600] ms` and **every** sample the receiver realizes
//!    must be `< 25 000 us` at EVERY srtt — at the lower rail and at the
//!    upper one alike. On the shipped engine the cadence is
//!    `(2*srtt).clamp(25, 100) ms` and **no sample can be below 25 000 us**,
//!    so this clause is unsatisfiable there. This is 16.78's `F1`.
//! 4. **THE CONTROL IS INERT AND SAYS SO.** Absent ⇒ `unset` on both
//!    endpoints and **no** sample below the shipped 25 ms rail — the
//!    byte-identity claim, read off a real run rather than off a unit test.
//! 5. **GARBAGE AND OUT-OF-DOMAIN RESOLVE BACK TO ABSENT, VISIBLY**, so a
//!    mistyped arm is READ rather than inferred: an unparseable value, a value
//!    below the receiver loop's wake granularity, and a value above the
//!    shipped upper rail all print `unset` and all leave the cadence shipped.
//!
//! **THIS BINARY FAILS ON THE PRE-CHANGE ENGINE**: `RWM_REFRESH_FLOOR_US` and
//! its `[GATES]` token do not exist there, and clause 3's sub-25 ms cadence is
//! arithmetically unreachable through `(2*srtt).clamp(25, 100) ms`.
//!
//! **What this binary deliberately does NOT assert.** Any field value of the
//! repair volume, the false-repair fraction, the hold-down `T`, or goodput.
//! Loopback's reordering is the host scheduler's and its loss is the shim's GE
//! process; **no claim about any cell can be made from it**. This is the
//! instrument gate that must pass before the L1 `(q, refresh)` sweep is worth
//! making.
//!
//! **Nothing here flips a default.** `RWM_REFRESH_FLOOR_US` is ABSENT by
//! default and nothing shipped reads it.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{require, u64_field};

/// The base arm. `RWM_DIAG` carries the receiver's periodic `[QCLK]`
/// readouts, which is where the realized cadence is read. No gate here
/// changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The gate under test.
const GATE: &str = "RWM_REFRESH_FLOOR_US";

/// The armed floor, in microseconds. This is `c1`'s own `p50/4` arm from
/// 16.78.3's derived grid (`[SUCC] orig` p50 = 24.6 ms ⇒ 6.15 ms), chosen
/// here because its band ceiling `4 * 6150 = 24 600 us` is **strictly below
/// the shipped 25 000 us rail**, which makes clause 3 an absolute invariant
/// over every srtt the loopback can produce rather than a claim about one.
const FLOOR_US: u64 = 6_150;

/// The shipped lower rail, `HOLE_NACK_REFRESH_MIN`. Nothing the shipped
/// cadence law can return is below this, at any srtt.
const SHIPPED_MIN_US: u64 = 25_000;

/// The band ceiling the armed arm may not exceed: `HOLE_NACK_REFRESH_BAND`
/// (= the shipped clamp's own 100/25 aspect ratio) times the commanded floor.
const ARMED_CEIL_US: u64 = FLOOR_US * 4;

/// One lossy loopback run in the given gate configuration.
/// Returns `(client/sender log, server/receiver log)`.
///
/// The absent arm is ABSENT: inheritance defeats an allowlist, and the whole
/// point of the control is that NOTHING set the floor — the harness clears
/// every inherited `RWM_*` var. The L1 `c3` cell (LTE-class) shapes client
/// egress, seeded: loss is what creates the holes whose re-advertisement
/// cadence this test is about. The receiver log is taken once a `[QCLK]
/// site=receiver` readout post-dating the transfer landed.
fn lossy_run(extra: &[(&str, &str)]) -> (String, String) {
    let mut env = ARM.to_vec();
    env.extend_from_slice(extra);
    loopback::lossy_run(&env, "bulk", "4000000", Some("[QCLK] site=receiver"))
}

/// The RECEIVER's `[QCLK]` line — the realized hole-refresh cadence as a
/// DISTRIBUTION. `w_us_min` / `w_us_max` bound every cadence the site actually
/// used, which is the only reading that can witness a clamp band.
fn receiver_qclk(srv: &str) -> &str {
    require(
        srv,
        "[QCLK] site=receiver",
        "the receiver never printed its realized recovery clock — the \
         hole-refresh site did not run, so nothing below can be read",
    )
}

/// Clause 2, on every arm: the site executed before anything it produced is
/// read. MEASUREMENT DISCIPLINE rule 1.
fn assert_site_executed(l: &str) {
    let evals = u64_field(l, "evals=");
    assert!(
        evals > 0,
        "the receiver's hole-refresh site never evaluated a cadence — the \
         mechanism under test did not run: {l}"
    );
    let kept = u64_field(l, "kept=");
    assert!(
        kept > 0,
        "the receiver evaluated a cadence but kept no sample, so `w_us_*` \
         below would be a quantile over nothing: {l}"
    );
}

// ── 1 — THE ARMED ARM: set, echoed two-sided, and the cadence MOVES ───────

#[test]
fn the_refresh_floor_arms_echoes_two_sided_and_delivers_a_cadence_below_the_shipped_rail() {
    let (cli, srv) = lossy_run(&[(GATE, &FLOOR_US.to_string())]);

    // (1) THE GATE ECHO, both endpoints, two-sided. The floor is CONSUMED at
    // the receiver and ECHOED at both, so "the sender's arm did not take" and
    // "the receiver's arm did not take" are separate, readable facts.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains(&format!("{GATE}={FLOOR_US}")),
            "{site}: the RESOLVED floor must be on the [GATES] line: {gates}"
        );
    }

    // (2) THE SITE EXECUTED.
    let q = receiver_qclk(&srv);
    assert_site_executed(q);

    // (3) THE WIRING WITNESS, AS AN ABSOLUTE LAW INVARIANT (16.78 `F1`).
    // The commanded band is [6 150, 24 600] us, so EVERY realized sample must
    // be strictly below the shipped 25 000 us rail — at whichever rail binds.
    // The shipped law cannot return such a value at any srtt.
    let w_max = u64_field(q, "w_us_max=");
    let w_min = u64_field(q, "w_us_min=");
    assert!(
        w_max < SHIPPED_MIN_US,
        "the armed floor must put the DELIVERED cadence below the shipped \
         25 ms rail at every sample — `w_us_max={w_max}` is not below \
         {SHIPPED_MIN_US} us, so the lift did not reach the site that emits \
         the report (16.78 F1): {q}"
    );
    assert!(
        w_min >= FLOOR_US && w_max <= ARMED_CEIL_US,
        "every realized cadence must lie inside the COMMANDED band \
         [{FLOOR_US}, {ARMED_CEIL_US}] us — got [{w_min}, {w_max}]: {q}"
    );
}

// ── 2 — THE CONTROL: absent, inert, and the cadence is the shipped one ────

#[test]
fn the_absent_floor_is_visible_and_leaves_the_shipped_cadence_untouched() {
    let (cli, srv) = lossy_run(&[]);

    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains(&format!("{GATE}=unset")),
            "{site}: the ABSENT floor must echo `unset`, so a control is as \
             mechanically assertable as an arm: {gates}"
        );
    }

    let q = receiver_qclk(&srv);
    assert_site_executed(q);

    // BYTE-IDENTITY, READ OFF A REAL RUN. The shipped law is
    // `(2*srtt).clamp(25, 100) ms`; no sample it returns can be outside that.
    let w_min = u64_field(q, "w_us_min=");
    let w_max = u64_field(q, "w_us_max=");
    assert!(
        w_min >= SHIPPED_MIN_US && w_max <= 100_000,
        "with the floor ABSENT the realized cadence must stay inside the \
         SHIPPED band [25 000, 100 000] us — got [{w_min}, {w_max}], which \
         means the re-expression changed the default path: {q}"
    );
}

// ── 3 — GARBAGE AND OUT-OF-DOMAIN RESOLVE BACK TO ABSENT, VISIBLY ─────────

#[test]
fn garbage_and_out_of_domain_floors_resolve_back_to_absent_and_say_so() {
    // Unparseable; below the receiver loop's wake granularity (LOOP_WAKE_US =
    // 1 000 us), where the cadence cannot be expressed by the loop that has to
    // emit it; and above the shipped upper rail, where the band's LOWER rail
    // would leave the shipped band entirely. The domain is the law's own.
    for bad in ["banana", "0", "999", "-1", "100001", ""] {
        let (cli, srv) = lossy_run(&[(GATE, bad)]);
        for (site, log) in [("sender", &cli), ("receiver", &srv)] {
            let gates = require(log, "[GATES]", "the engine never echoed its gates");
            assert!(
                gates.contains(&format!("{GATE}=unset")),
                "{site}: `{GATE}={bad}` is outside the law's own domain and \
                 must resolve back to ABSENT and PRINT `unset`, so a mistyped \
                 arm is READ rather than inferred: {gates}"
            );
        }
        let q = receiver_qclk(&srv);
        assert_site_executed(q);
        let w_min = u64_field(q, "w_us_min=");
        assert!(
            w_min >= SHIPPED_MIN_US,
            "`{GATE}={bad}` resolved to absent on the echo but the cadence \
             moved anyway — got `w_us_min={w_min}`: {q}"
        );
    }
}
