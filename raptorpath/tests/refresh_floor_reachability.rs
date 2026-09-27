//! The refresh-floor lift `RWM_REFRESH_FLOOR_US` (paper §7.4) reaches the
//! receiver's hole-refresh cadence on the real engine over a lossy wire. The
//! sender learns a hole closed from its absence in a later receiver report,
//! so the finest gap response it can time is one refresh interval, and the
//! shipped `(2·srtt).clamp(25, 100) ms` puts that interval at or above the
//! self-heal median at most cells; no hold-down level is commandable below
//! it until the cadence moves. Clauses, in the order they can fail:
//!
//! 1. The gate is echoed at both endpoints: `RWM_REFRESH_FLOOR_US=<us>` on
//!    the armed arm, `unset` on the control.
//! 2. The site executed: `[QCLK] site=receiver` with `evals > 0`
//!    (measurement-discipline rule 1).
//! 3. The delivered cadence is below the shipped rail, an absolute invariant:
//!    the armed floor is 6 150 µs, so the band is `[6.150, 24.600] ms` and
//!    every realized sample is `< 25 000 µs` at every srtt — unsatisfiable
//!    through the shipped clamp.
//! 4. The control is inert: `unset` on both endpoints and no sample below the
//!    shipped 25 ms rail.
//! 5. Garbage and out-of-domain values (unparseable, below the receiver
//!    loop's wake granularity, above the shipped upper rail) resolve back to
//!    absent, print `unset`, and leave the cadence shipped.
//!
//! No repair volume, false-repair fraction, hold-down `T` or goodput is
//! asserted. `RWM_REFRESH_FLOOR_US` is absent by default and nothing shipped
//! reads it.

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

/// The armed floor, in microseconds: `c1`'s `p50/4` arm (`[SUCC] orig` p50 =
/// 24.6 ms ⇒ 6.15 ms). Its band ceiling `4 · 6150 = 24 600 µs` is strictly
/// below the shipped 25 000 µs rail, which makes clause 3 an absolute
/// invariant over every srtt the loopback can produce.
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
/// The harness clears inherited `RWM_*` vars, so the control's floor is
/// unset. The `c3` cell shapes client egress, seeded; loss creates the holes
/// whose re-advertisement cadence is under test. The receiver log is taken
/// once a `[QCLK] site=receiver` readout post-dating the transfer landed.
fn lossy_run(extra: &[(&str, &str)]) -> (String, String) {
    let mut env = ARM.to_vec();
    env.extend_from_slice(extra);
    loopback::lossy_run(&env, "bulk", "4000000", Some("[QCLK] site=receiver"))
}

/// The receiver's `[QCLK]` line — the realized hole-refresh cadence as a
/// distribution. `w_us_min`/`w_us_max` bound every cadence the site used,
/// the only reading that can witness a clamp band.
fn receiver_qclk(srv: &str) -> &str {
    require(
        srv,
        "[QCLK] site=receiver",
        "the receiver never printed its realized recovery clock — the \
         hole-refresh site did not run, so nothing below can be read",
    )
}

/// Clause 2, on every arm: the site executed before anything it produced is
/// read.
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

// ── 1 — The armed arm: set, echoed two-sided, and the cadence moves ───────

#[test]
fn the_refresh_floor_arms_echoes_two_sided_and_delivers_a_cadence_below_the_shipped_rail() {
    let (cli, srv) = lossy_run(&[(GATE, &FLOOR_US.to_string())]);

    // (1) The gate echo, both endpoints. The floor is consumed at the
    // receiver and echoed at both, so each side's failure to take is a
    // separate, readable fact.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains(&format!("{GATE}={FLOOR_US}")),
            "{site}: the RESOLVED floor must be on the [GATES] line: {gates}"
        );
    }

    // (2) The site executed.
    let q = receiver_qclk(&srv);
    assert_site_executed(q);

    // (3) The wiring witness as an absolute invariant: the commanded band is
    // [6 150, 24 600] µs, so every realized sample is strictly below the
    // shipped 25 000 µs rail, which the shipped law cannot return at any srtt.
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

// ── 2 — The control: absent, inert, and the cadence is the shipped one ────

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

    // Byte-identity, read off a real run: the shipped law is
    // `(2·srtt).clamp(25, 100) ms`.
    let w_min = u64_field(q, "w_us_min=");
    let w_max = u64_field(q, "w_us_max=");
    assert!(
        w_min >= SHIPPED_MIN_US && w_max <= 100_000,
        "with the floor ABSENT the realized cadence must stay inside the \
         SHIPPED band [25 000, 100 000] us — got [{w_min}, {w_max}], which \
         means the re-expression changed the default path: {q}"
    );
}

// ── 3 — Garbage and out-of-domain resolve back to absent, visibly ─────────

#[test]
fn garbage_and_out_of_domain_floors_resolve_back_to_absent_and_say_so() {
    // Unparseable; below the receiver loop's wake granularity
    // (`LOOP_WAKE_US` = 1 000 µs), where the loop cannot emit the cadence;
    // and above the shipped upper rail, where the band's lower rail would
    // leave the shipped band entirely.
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
