//! The placement-law experiment arms `RWM_PLACE_T_DERIVED` and
//! `RWM_PLACE_WDIV_DERIVED` (paper §5.7, both default-absent) reach the wire
//! and echo two-sided. (`RWM_PLACE_HOL` is archived: tag
//! `archive/RWM_PLACE_HOL`.) Clauses:
//!
//!   1. `[GATES]` names each arm with its resolved value on both endpoints,
//!      so an absent arm is as readable off a control log as a present one.
//!   2. The arm executed (measurement-discipline rule 1): for the derived
//!      temperature, `T_eff` printed, finite and different from the shipped
//!      `0.15`.
//!   3. With every arm absent the same gauges read `-` / `0`.
//!
//! No value of `T_eff` is asserted; loopback's queueing is the host's.
//! Own test binary: `RWM_L0_NETEM` is
//! process-global in the child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{field, opt_f64_field as f64_field, u64_field};

/// The environment every arm shares. `RWM_DIAG=1` makes `[ETA]` fire; it is
/// asserted present in the `[GATES]` echo.
const BASE: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    // A fixed open store (status §49): the path then runs into its own
    // congestion window, the case these clauses need. `RWM_PLAIN_RS=1`
    // reached it through the deleted honest-cap arm.
    ("RWM_STORE", "4096"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The arm gates, in the order the `[GATES]` echo prints them.
const ARM_GATES: [&str; 2] = ["RWM_PLACE_T_DERIVED", "RWM_PLACE_WDIV_DERIVED"];

/// One loopback transfer under one arm. Returns `(sender log, receiver log)`.
///
/// The arm rides both endpoints: the receiver runs a scheduler too. The
/// receiver log is read only for its startup `[GATES]` echo, so it is
/// snapshotted without waiting.
fn run(paths: usize, netem: Option<&str>, bytes: &str, arm: &[(&str, &str)]) -> (String, String) {
    let mut env = BASE.to_vec();
    env.extend_from_slice(arm);
    loopback::transfer(loopback::Transfer {
        paths,
        env: &env,
        client_env: &loopback::shaped(netem),
        bytes,
        ..Default::default()
    })
}

// ── Readers ─────────────────────────────────────────────────────────────

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    gauge::require(log, pat, "the gauge is unreachable")
}

/// The two-sided echo: every arm gate is named on the `[GATES]` line of both
/// endpoints with this run's value. Scoped to the `[GATES]` line because the
/// resolve-time liveness echoes contain the gate names in their prose.
fn assert_arm_echo(cli: &str, srv: &str, want: &[(&str, &str)]) {
    for (log, side) in [(cli, "sender"), (srv, "receiver")] {
        let g = last_with(log, "[GATES]");
        assert!(g.contains("RWM_DIAG=1"), "the {side}'s [GATES] lacks RWM_DIAG=1: {g}");
        for gate in ARM_GATES {
            let expect = want
                .iter()
                .find(|(k, _)| *k == gate)
                .map(|(_, v)| if *v == "0" { "0" } else { "1" })
                .unwrap_or("0");
            let tok = format!("{gate}={expect}");
            assert!(
                g.contains(&tok),
                "the {side}'s [GATES] must name `{tok}` — TWO-SIDED, so an \
                 ABSENT arm is as readable as a present one (old engine: the \
                 token does not exist at all):\n{g}"
            );
        }
    }
}

// ── The runs ────────────────────────────────────────────────────────────

/// A run that hit the perf client's run timeout is a stall, not a gauge
/// reading: the DNF line must fail the transfer, with the server tail.
#[test]
fn a_stalled_run_fails_as_a_dnf_not_as_a_gauge_reading() {
    let run = r#"{"proto":"rp-native","hint":"bulk","run":1,"seconds":1.5,"mbps":53.3}"#;
    let dnf = r#"{"proto":"rp-native","hint":"bulk","run":2,"dnf":true,"timeout_s":300}"#;
    let cli = format!(
        "{}\n{run}\n{dnf}\n{}\n",
        r#"{"warmup":true,"seconds":0.01}"#,
        r#"{"summary":true,"runs":2,"dnf":1}"#,
    );
    let srv = "INFO raptorpath: server saw STALL_MARKER_7\n";
    let err = std::panic::catch_unwind(|| loopback::assert_no_dnf(&cli, Some(srv)))
        .expect_err("a DNF run line must fail the transfer");
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .expect("panic message");
    assert!(msg.contains("did not finish"), "{msg}");
    assert!(msg.contains("STALL_MARKER_7"), "server tail missing: {msg}");

    let clean = format!(
        "{}\n{run}\n{run}\n{}\n",
        r#"{"warmup":true,"seconds":0.01}"#,
        r#"{"summary":true,"runs":2,"dnf":0}"#,
    );
    loopback::assert_no_dnf(&clean, Some(srv));
}

/// Control — every arm absent. The gauges exist on the line and read
/// `-` / `0`; every other reading is a difference from this row.
#[test]
fn the_control_arm_echoes_absent_and_every_arm_gauge_is_silent() {
    let (cli, srv) = run(2, Some("c2,c3"), "16000000", &[]);
    assert_arm_echo(&cli, &srv, &[]);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] CTL sender: {s}");
    assert!(u64_field(s, "n=") > 0, "no placement stamped at all: {s}");

    // Arm 1 silent: no derived temperature was ever resolved.
    assert_eq!(u64_field(s, "t_n="), 0, "CTL resolved a DERIVED temperature: {s}");
    assert_eq!(field(s, "t_eff="), "-", "`-` iff n = 0, by construction: {s}");
    assert_eq!(field(s, "t_cold="), "-", "{s}");
}

/// `RWM_PLACE_T_DERIVED`: the temperature is resolved from the sender's own
/// ETA-error dispersion on every placement — `t_n > 0`, `T_eff` finite, and
/// different from the shipped `0.15` (the shipped constant's claim is
/// `σ̂_e = 0.19238·ref`). The cold count is printed, not asserted: whether
/// the τ-lag has a pair when a placement is priced depends on the cell's
/// sample rate.
#[test]
fn the_derived_temperature_arm_resolves_a_finite_t_eff_on_the_dual() {
    let arm = [("RWM_PLACE_T_DERIVED", "1")];
    let (cli, srv) = run(2, Some("c2,c3"), "16000000", &arm);
    assert_arm_echo(&cli, &srv, &arm);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] Tsigma sender: {s}");
    let n = u64_field(s, "t_n=");
    assert!(
        n > 0,
        "[ETA] t_n=0 with RWM_PLACE_T_DERIVED=1 — the arm is echoed but the \
         law never reached `place_temperature_eff`, which is a WIRING failure \
         and not a null result:\n{s}"
    );
    let t = f64_field(s, "t_eff=").expect("t_n > 0 ⇒ the level exists");
    assert!(t.is_finite() && t >= 0.0, "T_eff must be a finite non-negative scalar: {s}");
    let cold = f64_field(s, "t_cold=").expect("t_n > 0 ⇒ the fraction exists");
    assert!((0.0..=1.0).contains(&cold), "`t_cold=` must be a fraction: {s}");
    println!(
        "[place-reach] T_eff={t:.6} vs shipped 0.15 — σ̂_e/ref = {:.5} against \
         the shipped constant's own claim of 0.19238; cold rule fired on \
         {:.1} % of resolutions",
        t / (6.0_f64).sqrt() * std::f64::consts::PI,
        cold * 100.0
    );
    // Unless the cold rule fired everywhere, at least one resolution used a
    // measured dispersion; reporting 0.15 back would mean the pooling never
    // ran.
    if cold < 1.0 {
        assert!(
            (t - 0.15).abs() > 1e-9,
            "T_eff came back as the shipped 0.15 on a run where the cold rule \
             did not always fire — the derived pooling did not execute:\n{s}"
        );
    }
}

/// The `N = 1` control on the wire: a softmax over a singleton is 1 at every
/// temperature and cost, so no arm can change a placement. The derived
/// temperature must still RUN at N = 1 (`t_n > 0`) and decide nothing.
#[test]
fn no_arm_can_move_a_single_path_placement_on_the_wire() {
    let arm = [("RWM_PLACE_T_DERIVED", "1"), ("RWM_PLACE_WDIV_DERIVED", "1")];
    let (cli, srv) = run(1, None, "8000000", &arm);
    assert_arm_echo(&cli, &srv, &arm);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] N=1 all-armed sender: {s}");
    assert!(u64_field(s, "t_n=") > 0, "the arm must still RUN at N = 1: {s}");
}
