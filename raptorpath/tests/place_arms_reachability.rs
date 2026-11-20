//! The placement-law experiment arm `RWM_PLACE_WDIV_DERIVED` (paper §5.7,
//! default-absent) reaches the wire and echoes two-sided. (`RWM_PLACE_HOL`
//! and `RWM_PLACE_T_DERIVED` are archived: tags `archive/RWM_PLACE_HOL`,
//! `archive/RWM_PLACE_T_DERIVED`.) Clauses:
//!
//!   1. `[GATES]` names the arm with its resolved value on both endpoints,
//!      so an absent arm is as readable off a control log as a present one.
//!   2. The sender stamps placements with the arm absent and armed.
//!
//! Own test binary: `RWM_L0_NETEM` is
//! process-global in the child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::u64_field;

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
const ARM_GATES: [&str; 1] = ["RWM_PLACE_WDIV_DERIVED"];

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
        "{}\n{run}\n{run}\n{}\n{CLEAN_ACCT}\n",
        r#"{"warmup":true,"seconds":0.01}"#,
        r#"{"summary":true,"runs":2,"dnf":0}"#,
    );
    loopback::assert_no_dnf(&clean, Some(srv));
}

/// A clean sender exit line (ADR-0081): the identity closes, nothing is outstanding.
const CLEAN_ACCT: &str = "[ACCT] reliable=0 S_sent=100 A=95 H=3 E=2 O_serv=0 O_strand=0 O_max=0 closes=1 inert_sweeps=0 sweeps=0 \
    oldest_unchanged=0 oldest_unchanged_max=0 O_hwm=40 store_cap=64 bud=0.050000 shed_slot=1 shed_nack=2 \
    shed_visit_slot=5 shed_visit_nack=9 clamp_evals=30 clamp_excl=4 final=1";

fn panic_msg(f: impl FnOnce() + std::panic::UnwindSafe) -> Option<String> {
    let err = std::panic::catch_unwind(f).err()?;
    err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
}

/// The account check (ADR-0081) fails each way the account can be wrong, and a
/// missing exit line fails too (the check must execute). Builds without the
/// account have nothing to read and skip.
#[test]
fn the_account_check_fails_each_defect_and_a_missing_line() {
    if !loopback::ACCOUNT_CARRIED {
        return;
    }
    let ok = r#"{"proto":"rp-native","hint":"bulk","run":1,"seconds":1.5,"mbps":53.3}"#;
    let fails = |what: &str, acct: String, expect: &str| {
        let log = format!("{ok}\n{acct}\n");
        let msg = panic_msg(move || loopback::assert_no_dnf(&log, None)).unwrap_or_else(|| panic!("{what}: did not fail"));
        assert!(msg.contains(expect), "{what}: {msg}");
    };
    fails("O_strand", CLEAN_ACCT.replace("O_strand=0", "O_strand=3").replace("S_sent=100", "S_sent=103"), "O_strand = 3");
    // Outstanding seqs: judged against the receiver's frontier (ack in flight vs stuck).
    let o2 = CLEAN_ACCT.replace("O_serv=0", "O_serv=2").replace("S_sent=100", "S_sent=102").replace("O_max=0", "O_max=101");
    let log = format!("{ok}\n{o2}\n");
    let msg = panic_msg(|| loopback::assert_no_dnf(&log, Some("[ACCT-RX] frontier=100"))).expect("O above the frontier must fail");
    assert!(msg.contains("not below the receiver's frontier 100"), "{msg}");
    loopback::assert_no_dnf(&log, Some("[ACCT-RX] frontier=50\n[ACCT-RX] frontier=102 final=1"));
    fails("identity", CLEAN_ACCT.replace("S_sent=100", "S_sent=101"), "does not close");
    fails("inert", CLEAN_ACCT.replace("inert_sweeps=0", "inert_sweeps=4"), "inert_sweeps = 4");
    fails("shed without budget", CLEAN_ACCT.replace("bud=0.050000", "bud=0.000000"), "no shed budget ever open");
    // E over the budget is reported, not gated: E is an upper bound on abandonment.
    loopback::assert_no_dnf(&format!("{ok}\n{}\n", CLEAN_ACCT.replace("E=2", "E=40").replace("A=95", "A=57")), None);
    let none = panic_msg(|| loopback::assert_no_dnf(&format!("{ok}\n"), None)).expect("a missing exit line must fail");
    assert!(none.contains("no `[ACCT]"), "{none}");
}

/// Control — every arm absent; every other reading is a difference from this row.
#[test]
fn the_control_arm_echoes_absent_and_stamps_placements() {
    let (cli, srv) = run(2, Some("c2,c3"), "16000000", &[]);
    assert_arm_echo(&cli, &srv, &[]);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] CTL sender: {s}");
    assert!(u64_field(s, "n=") > 0, "no placement stamped at all: {s}");
}

/// The `N = 1` control on the wire: a softmax over a singleton is 1 at every
/// temperature and cost, so no arm can change a placement. The arm must
/// still RUN at N = 1 (placements stamped) and decide nothing.
#[test]
fn no_arm_can_move_a_single_path_placement_on_the_wire() {
    let arm = [("RWM_PLACE_WDIV_DERIVED", "1")];
    let (cli, srv) = run(1, None, "8000000", &arm);
    assert_arm_echo(&cli, &srv, &arm);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] N=1 all-armed sender: {s}");
    assert!(u64_field(s, "n=") > 0, "the arm must still RUN at N = 1: {s}");
}
