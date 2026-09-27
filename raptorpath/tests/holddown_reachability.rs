//! The hold-down clock `RWM_HOLDDOWN_Q` (paper §7.4) reaches the sender's
//! gap-report response path on the real engine over a lossy wire. Recovery
//! fires are overwhelmingly gap-driven, not timer-driven; this arm holds a
//! reported hole for `T(q)` before repairing it. Clauses, in the order they
//! can fail:
//!
//! 1. The gate is echoed at both endpoints: `RWM_HOLDDOWN_Q=<resolved>` on
//!    the armed arm, `unset` on the control.
//! 2. The gauge exists and its window law is the one asked for:
//!    `[HOLD] site=sender q=0.500000 n_req=20`.
//! 3. The site executed: `evals > 0` (measurement-discipline rule 1).
//! 4. The estimator was fed and its law ran: `fed > 0`, `samp_n > 0`,
//!    `law_n > 0`, and `t_us` is a number.
//! 5. The gate suppressed a fire: `sup > 0` — a knob that decides, not one
//!    that is merely read.
//! 6. The accounting closes: `evals = sup + emit` on every line.
//! 7. The control is inert and says so: `q=unset`, `n_req=-`, `sup=0`,
//!    `law_n=0`, while it still observes the unforced outstanding-time
//!    distribution and the fires still reach the wire.
//! 8. Garbage (unparseable, ≥ 1, ≤ 0) resolves back to absent and prints
//!    `unset`.
//!
//! No value of `T`, the suppression fraction or goodput is asserted.
//! `RWM_HOLDDOWN_Q` is absent by default and nothing shipped reads it.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{require, str_field, u64_field};

/// The base arm. `RWM_DIAG` carries `[DIAG] retx=` and the receiver's periodic
/// `[QCLK]` readouts. No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The armed level. `q = 0.5` is the derived floor arm: the window law is
/// flat at `N = 2K = 20`, the fastest-filling level the construction can
/// express and so the one a loopback run can reach. The window law and the
/// order statistic are pinned in `recovery_bench.rs`; this binary pins that
/// the engine routes to them and that the gate suppresses a fire.
const HQ: &str = "0.5";
const HQ_ECHO: &str = "q=0.500000";
const N_REQ: &str = "n_req=20";

/// One lossy loopback run in the given gate configuration.
/// Returns `(client/sender log, server/receiver log)`.
///
/// The harness clears inherited `RWM_*` vars, so the control's
/// `RWM_HOLDDOWN_Q` is unset. The `c3` cell shapes client egress, seeded;
/// loss drives the recovery clock. The server log is read only for its
/// startup `[GATES]` echo.
fn lossy_run(extra: &[(&str, &str)]) -> (String, String) {
    let mut env = ARM.to_vec();
    env.extend_from_slice(extra);
    loopback::lossy_run(&env, "bulk", "4000000", None)
}

/// The maximum `retx=<n>` the sender printed, read as a max over lines:
/// `retx=` in the `[DIAG]` tail is an interval counter.
fn max_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| {
            v.trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

/// Every `[HOLD]` line the sender printed, newest last: one per path that saw
/// a fire, plus the unattributed bucket `path=-` (timer fires this arm does
/// not touch).
fn hold_lines(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|l| l.contains("[HOLD] site=sender"))
        .collect()
}

/// The `[HOLD]` line for a real path (`path=` is a number, not `-`). The
/// unattributed bucket carries no window and no law, so pooling it with a
/// real path's row would report a law that never ran as one that ran and did
/// nothing.
fn hold_pathline<'a>(lines: &[&'a str]) -> &'a str {
    lines
        .iter()
        .copied()
        .find(|l| !l.contains("path=-"))
        .unwrap_or_else(|| {
            panic!("no per-path `[HOLD]` line — the gap-report site never ran:\n{lines:#?}")
        })
}

/// The accounting identity on every line of every arm: a fire is either
/// held or emitted.
fn assert_accounting_closes(lines: &[&str]) {
    assert!(!lines.is_empty(), "the sender printed no `[HOLD]` line at all");
    for l in lines {
        let evals = u64_field(l, "evals=");
        let sup = u64_field(l, "sup=");
        let emit = u64_field(l, "emit=");
        assert_eq!(evals, sup + emit, "evals must equal sup + emit: {l}");
        let law_n = u64_field(l, "law_n=");
        assert!(law_n <= evals, "law_n cannot exceed evals: {l}");
        assert!(sup <= law_n, "a fire cannot be held by a law that did not run: {l}");
    }
}

// ── 1 — The armed arm: set, echoed, routed, fed, and it suppresses ───────

#[test]
fn the_holddown_arms_echoes_routes_feeds_its_estimator_and_suppresses_a_fire() {
    let (cli, srv) = lossy_run(&[("RWM_HOLDDOWN_Q", HQ)]);

    // (1) The gate echo, both endpoints.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains(&format!("RWM_HOLDDOWN_Q={HQ}")),
            "{site}: the RESOLVED level must be on the [GATES] line: {gates}"
        );
    }

    let lines = hold_lines(&cli);
    assert_accounting_closes(&lines);
    let l = hold_pathline(&lines);

    // (2) The gauge, with the window law the arm asked for.
    assert!(
        l.contains(HQ_ECHO),
        "the gauge must print the RESOLVED level: {l}"
    );
    assert!(
        l.contains(N_REQ),
        "N(1-q) must be the window law's own answer at this level: {l}"
    );

    // (3) The site executed.
    let evals = u64_field(l, "evals=");
    assert!(evals > 0, "the gap-report response site never ran: {l}");

    // (4) The estimator was fed and its own law ran.
    let fed = u64_field(l, "fed=");
    let samp_n = u64_field(l, "samp_n=");
    let law_n = u64_field(l, "law_n=");
    assert!(fed > 0, "no hole ever retired by its own original: {l}");
    assert!(samp_n > 0, "the per-path window is empty: {l}");
    assert!(
        law_n > 0,
        "the window never filled, so the arm's own law never ran: {l}"
    );
    let t = u64_field(l, "t_us=");
    assert!(t > 0, "a law that ran must have produced a T: {l}");

    // (5) The gate decided something.
    let sup = u64_field(l, "sup=");
    assert!(
        sup > 0,
        "the hold-down never suppressed a single fire — the knob is inert: {l}"
    );

    // The realized hold-down delay is reported as a distribution: a mean
    // would hide the tail the level commands.
    for k in ["hd_p50_us=", "hd_p90_us=", "hd_p99_us=", "hd_mx_us=", "hd_n="] {
        let _ = str_field(l, k);
    }

    // The wire still worked: holes that cleared the hold-down were repaired.
    assert!(
        max_retx(&cli) > 0,
        "the sender retransmitted nothing at all — the hold-down starved the plane"
    );
}

// ── 2 — The control: absent is inert, and it says so ─────────────────────

#[test]
fn without_the_level_the_gate_is_inert_and_the_echo_says_unset() {
    let (cli, srv) = lossy_run(&[]);

    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_HOLDDOWN_Q=unset"),
            "{site}: an absent level must print `unset`, two-sided: {gates}"
        );
    }

    let lines = hold_lines(&cli);
    assert_accounting_closes(&lines);
    // The control's gauge is still emitted (measurement-discipline rule 15),
    // so an absent `[HOLD]` line can only be read as an unreached site.
    for l in &lines {
        assert!(l.contains("q=unset"), "the control must say `q=unset`: {l}");
        assert!(l.contains("n_req=-"), "no window law is in force: {l}");
        assert_eq!(u64_field(l, "sup="), 0, "the control must hold NOTHING: {l}");
        assert_eq!(u64_field(l, "law_n="), 0, "the control runs no law: {l}");
        assert!(l.contains("t_us=-"), "the control commands no T: {l}");
        // The control observes: it commands no level and holds nothing, but
        // reports the unforced outstanding-time distribution, without which
        // "the distribution is long at this cell" cannot be told from "the
        // hold-down made it long". Nothing in the engine reads it.
        assert!(l.contains("n_obs="), "the control must declare its window: {l}");
    }
    // The observation is asserted on the per-path line only: the
    // unattributed bucket (`path=-`) has no window by construction.
    let pl = hold_pathline(&lines);
    assert!(u64_field(pl, "fed=") > 0, "the control must OBSERVE: {pl}");
    assert!(u64_field(pl, "samp_n=") > 0, "its window must be non-empty: {pl}");
    assert!(
        !pl.contains("obs_p50_us=-"),
        "the control must report a distribution to be a baseline: {pl}"
    );
    let o50 = u64_field(pl, "obs_p50_us=");
    let o90 = u64_field(pl, "obs_p90_us=");
    assert!(o50 > 0 && o90 >= o50, "the quantiles must be ordered: {pl}");

    let evals: u64 = lines.iter().map(|l| u64_field(l, "evals=")).sum();
    assert!(
        evals > 0,
        "the control's own site must still have run — otherwise clause 3 of the \
         armed arm proves nothing about a difference"
    );
    assert!(max_retx(&cli) > 0, "the control retransmitted nothing");
}

// ── 3 — Garbage resolves back to absent, visibly ─────────────────────────

#[test]
fn a_garbage_holddown_level_resolves_back_to_absent_and_prints_unset() {
    // Unparseable; at the top of the domain, where the window law diverges;
    // and at the bottom, where the hold-down is zero and the shipped machine
    // is expressed by absence rather than by an armed arm.
    for bad in ["banana", "1.5", "1.0", "0", "-0.5", "", "0.5,0.9"] {
        let (cli, srv) = lossy_run(&[("RWM_HOLDDOWN_Q", bad)]);
        for (site, log) in [("sender", &cli), ("receiver", &srv)] {
            let gates = require(log, "[GATES]", "the engine never echoed its gates");
            assert!(
                gates.contains("RWM_HOLDDOWN_Q=unset"),
                "{site}: `{bad}` must resolve back to ABSENT and print it: {gates}"
            );
        }
        let lines = hold_lines(&cli);
        assert_accounting_closes(&lines);
        for l in &lines {
            assert!(l.contains("q=unset"), "`{bad}`: {l}");
            assert_eq!(u64_field(l, "sup="), 0, "`{bad}` must hold nothing: {l}");
        }
    }
}

// ── 4 — The two arms are two arms ────────────────────────────────────────

#[test]
fn the_armed_and_disarmed_arms_realize_different_gap_report_behaviour() {
    let (armed, _) = lossy_run(&[("RWM_HOLDDOWN_Q", HQ)]);
    let (ctl, _) = lossy_run(&[]);

    let a: u64 = hold_lines(&armed).iter().map(|l| u64_field(l, "sup=")).sum();
    let c: u64 = hold_lines(&ctl).iter().map(|l| u64_field(l, "sup=")).sum();
    assert!(a > 0, "the armed arm suppressed nothing");
    assert_eq!(c, 0, "the control suppressed something — it is not a control");

    // The cross-gauge identity that proves where the gate sits:
    // `should_hold` is consulted once per fire that reaches
    // `record_fire_cause`, and the fire is then either held or classified by
    // `[FCAUSE]` (which counts only fires that reached the wire). So
    //
    //     sum([HOLD] evals)  ==  sum([HOLD] sup)  +  [FCAUSE] n
    //
    // holds exactly on both arms — an assertion a gauge agreeing only with
    // itself could not pass.
    for (name, log) in [("armed", &armed), ("control", &ctl)] {
        let f = require(log, "[FCAUSE]", "the sender never classified a fire");
        let n = u64_field(f, "n=");
        let ev: u64 = hold_lines(log).iter().map(|l| u64_field(l, "evals=")).sum();
        let sp: u64 = hold_lines(log).iter().map(|l| u64_field(l, "sup=")).sum();
        assert!(n > 0, "{name}: no fire reached the wire at all: {f}");
        assert_eq!(
            ev,
            sp + n,
            "{name}: the hold-down gate is not where it claims to be —              evals={ev} sup={sp} [FCAUSE] n={n}"
        );
    }
}
