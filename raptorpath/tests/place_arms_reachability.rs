//! **TRACK A's THREE PLACEMENT ARMS REACH THE WIRE, AND SAY SO TWO-SIDED.**
//!
//! **What this binary is for.** `RWM_PLACE_T_DERIVED`, `RWM_PLACE_HOL` and
//! `RWM_PLACE_WDIV_DERIVED` (paper §16.81) are DEFAULT-ABSENT experiment arms
//! on the placement law. A battery that runs them is worth nothing unless
//! three things are true of every invocation, and none of them is provable
//! from a diff:
//!
//!   1. **THE ARM IS ECHOED TWO-SIDED.** `[GATES]` names each of the three
//!      with its resolved value on BOTH endpoints, so "the arm was absent" is
//!      as readable off a control log as "the arm was present" is off a
//!      challenger's. A run whose configuration axis has no echo is the
//!      failure mode that produced the 31 Mbit/s anomaly.
//!   2. **THE ARM EXECUTED.** MEASUREMENT DISCIPLINE rule 1: prove the
//!      mechanism under test runs. For arm 1 that is `T_eff` printed, finite
//!      and DIFFERENT from the shipped `0.15`; for arm 2 it is `hol_calls > 0`
//!      AND — the WB1-style execution witness — `hol_mv > 0`, the count of
//!      placements whose argmin the frontier term actually MOVED. A term that
//!      is computed but never decides has not been measured.
//!   3. **THE CONTROL IS SILENT.** With every arm absent the same gauges read
//!      `-` / `0` on the same run shape. That is what makes a difference
//!      between arms attributable to the arm.
//!
//! **IT FAILS ON THE OLD ENGINE, WHICH IS THE POINT.** Before this branch
//! there is no `RWM_PLACE_*` in `[GATES]` and no `t_eff=` / `hol_*=` on the
//! `[ETA]` line, so every assertion below is unsatisfiable — a reachability
//! test that passes on the engine it is supposed to gate is not a gate.
//!
//! **What it deliberately does NOT assert.** Any VALUE of `T_eff`, of the
//! `s_i > H` bind fraction, or of `W`. Loopback's queueing is the host's and
//! its loss is the shim's Gilbert–Elliott process; the numbers come off an L1
//! run scored against a pre-registration. This is the INSTRUMENT gate.
//!
//! Own test binary, for `eta_reachability.rs`'s reason: `RWM_L0_NETEM` is
//! process-global in the child and the spawned pair must not contend with the
//! in-process loopback tests.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The environment every arm shares. `RWM_DIAG=1` is what makes `[ETA]` fire
/// at all; it is asserted present in the `[GATES]` echo below rather than
/// assumed.
const BASE: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The three arm gates, in the order the `[GATES]` echo prints them.
const ARM_GATES: [&str; 3] =
    ["RWM_PLACE_T_DERIVED", "RWM_PLACE_HOL", "RWM_PLACE_WDIV_DERIVED"];

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    l.local_addr().expect("probe addr").port()
}

struct Reaper(Child);
impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn join(a: &[SocketAddr]) -> String {
    a.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
}

fn spawn_perf_server(binds: &[SocketAddr], arm: &[(&str, &str)]) -> (Reaper, Arc<Mutex<String>>) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let mut cmd = Command::new(bin);
    cmd.args([
        "perf",
        "--server",
        "--bind",
        &join(binds),
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in BASE {
        cmd.env(k, v);
    }
    // THE ARM RIDES BOTH ENDPOINTS. The receiver runs a scheduler too, and an
    // arm that is present at one end only is a configuration split nobody
    // could read off a log.
    for (k, v) in arm {
        cmd.env(k, v);
    }
    cmd.env_remove("RWM_L0_NETEM");
    let mut srv = Reaper(cmd.spawn().expect("spawn perf server"));

    let log = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&log);
    let mut err = srv.0.stderr.take().expect("server stderr");
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink
                    .lock()
                    .expect("stderr sink")
                    .push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    });

    let mut out = srv.0.stdout.take().expect("server stdout");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = String::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline && !seen.contains("perf server ready") {
        match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) => panic!("reading perf server stdout: {e}"),
        }
    }
    assert!(seen.contains("perf server ready"), "perf server never became ready: {seen}");
    {
        let sink = Arc::clone(&log);
        sink.lock().expect("stderr sink").push_str(&seen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match out.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink
                        .lock()
                        .expect("stderr sink")
                        .push_str(&String::from_utf8_lossy(&buf[..n])),
                }
            }
        });
    }
    (srv, log)
}

/// One loopback transfer under one arm. Returns `(sender log, receiver log)`.
fn run(paths: usize, netem: Option<&str>, bytes: &str, arm: &[(&str, &str)]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let binds: Vec<SocketAddr> = (0..paths)
        .map(|_| format!("127.0.0.1:{}", free_port()).parse().unwrap())
        .collect();
    let (_srv, srv_log) = spawn_perf_server(&binds, arm);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &join(&binds),
        "--bytes",
        bytes,
        "--runs",
        "2",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in BASE {
        cli.env(k, v);
    }
    for (k, v) in arm {
        cli.env(k, v);
    }
    match netem {
        Some(spec) => {
            cli.env("RWM_L0_NETEM", spec);
            cli.env("RWM_L0_SEED", "42");
        }
        None => {
            cli.env_remove("RWM_L0_NETEM");
        }
    }

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed (paths={paths}, netem={netem:?}, arm={arm:?}, {:?})\n\
         --- stdout ---\n{cli_stdout}\n--- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

// ── READERS ─────────────────────────────────────────────────────────────

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line — THIS IS THE OLD-ENGINE FAILURE: {line}"))
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

/// A gauge slot that is `-` iff its own denominator is zero.
fn f64_field(line: &str, key: &str) -> Option<f64> {
    let v = field(line, key);
    if v == "-" {
        return None;
    }
    Some(v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e}")))
}

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    log.lines()
        .rev()
        .find(|l| l.contains(pat))
        .unwrap_or_else(|| panic!("no line containing `{pat}`:\n{log}"))
}

/// **THE TWO-SIDED ECHO ASSERTION.** Every arm gate is NAMED on the `[GATES]`
/// line of BOTH endpoints, with the value this run was configured for. Scoped
/// to the `[GATES]` line on purpose: the resolve-time liveness echoes contain
/// the gate NAMES in their prose, and an unscoped grep would read the
/// documentation instead of the resolved value.
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

// ── THE RUNS ────────────────────────────────────────────────────────────

/// **CTL — every arm absent.** The gauges exist on the line (so their absence
/// on a challenger would be a wiring failure and not a missing field) and read
/// `-` / `0`: nothing armed, nothing counted. This is the row every other
/// reading is a difference from.
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

    // Arm 2 silent: the frontier term never ran, so it never moved an argmin.
    assert_eq!(u64_field(s, "hol_calls="), 0, "CTL ran the frontier term: {s}");
    assert_eq!(u64_field(s, "hol_n="), 0, "{s}");
    assert_eq!(field(s, "hol_mv="), "-", "{s}");
    assert_eq!(field(s, "hol_sh="), "-", "{s}");
    assert_eq!(field(s, "hol_w="), "-", "{s}");
}

/// **Tσ — `RWM_PLACE_T_DERIVED`.** The temperature is resolved from the
/// sender's own ETA-error dispersion on every placement: `t_n > 0`, `T_eff`
/// printed and FINITE, and — the reading the whole arm exists for — DIFFERENT
/// from the shipped `0.15`, which is the falsifiable claim
/// `σ̂_e = 0.19238·ref` being falsified or confirmed on this cell.
///
/// The cold count is REPORTED, not asserted: whether the τ-lag has found a
/// pair by the time a placement is priced is a property of the cell's own
/// sample rate, and the point of printing `t_cold` is that a reader never has
/// to guess.
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
    // The gauge must not be a constant echo of the shipped dial: with the
    // cold rule NOT firing everywhere, at least one resolution used a measured
    // dispersion, and reporting 0.15 back would mean the pooling never ran.
    if cold < 1.0 {
        assert!(
            (t - 0.15).abs() > 1e-9,
            "T_eff came back as the shipped 0.15 on a run where the cold rule \
             did not always fire — the derived pooling did not execute:\n{s}"
        );
    }
}

/// **HOL — `RWM_PLACE_HOL`.** The frontier term runs on every source
/// placement (`hol_n > 0`), its `κ` bind is a fraction, `W` prints at a live
/// value — and the EXECUTION WITNESS fires: `hol_mv > 0`, the term changed the
/// argmin on at least one placement of a lossy asymmetric dual. That last
/// assertion is the one that separates "the term was computed" from "the term
/// decided", and it is the reason this arm is scoreable at all.
#[test]
fn the_frontier_arm_runs_and_moves_the_argmin_on_the_dual() {
    let arm = [("RWM_PLACE_HOL", "1")];
    let (cli, srv) = run(2, Some("c2,c3"), "16000000", &arm);
    assert_arm_echo(&cli, &srv, &arm);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] HOL sender: {s}");
    let calls = u64_field(s, "hol_calls=");
    let evals = u64_field(s, "hol_n=");
    assert!(
        calls > 0 && evals > 0,
        "[ETA] hol_calls={calls} hol_n={evals} with RWM_PLACE_HOL=1 — the arm \
         is echoed but the term never reached `place_costs`:\n{s}"
    );
    let sh = f64_field(s, "hol_sh=").expect("hol_n > 0 ⇒ the fraction exists");
    assert!((0.0..=1.0).contains(&sh), "the `s_i > H` bind must be a fraction: {s}");
    let w = f64_field(s, "hol_w=").expect("hol_calls > 0 ⇒ the level exists");
    assert!(w.is_finite() && w >= 0.0, "W must be a finite non-negative price: {s}");
    let mv = f64_field(s, "hol_mv=").expect("hol_calls > 0 ⇒ the fraction exists");
    assert!(
        mv > 0.0,
        "[ETA] hol_mv=0 over {calls} placements on a lossy ASYMMETRIC dual — \
         the frontier term was computed and never changed a decision. That is \
         a real finding (INERT-AS-DERIVED), and it is asserted here rather \
         than reported because a Stage-2 battery scored on an inert term is \
         not a measurement:\n{s}"
    );
    println!(
        "[place-reach] frontier term: κ bound reachable on {:.1} % of source \
         evaluations, argmin moved on {:.1} % of {calls} placements, W={w:.3e}",
        sh * 100.0,
        mv * 100.0
    );
}

/// **THE `N = 1` CONTROL, ON THE WIRE.** With one path the softmax over a
/// singleton is 1 at every temperature and every cost, so NO arm can change a
/// placement — `hol_mv` must be exactly 0 even with the frontier term armed.
/// A movement here VOIDS a battery run, which is why it is asserted on the
/// engine's own output and not argued.
#[test]
fn no_arm_can_move_a_single_path_placement_on_the_wire() {
    let arm = [
        ("RWM_PLACE_T_DERIVED", "1"),
        ("RWM_PLACE_HOL", "1"),
        ("RWM_PLACE_WDIV_DERIVED", "1"),
    ];
    let (cli, srv) = run(1, None, "8000000", &arm);
    assert_arm_echo(&cli, &srv, &arm);

    let s = last_with(&cli, "[ETA] site=sender");
    println!("[place-reach] N=1 all-armed sender: {s}");
    assert!(u64_field(s, "hol_calls=") > 0, "the arm must still RUN at N = 1: {s}");
    assert_eq!(
        f64_field(s, "hol_mv="),
        Some(0.0),
        "a singleton candidate set has no argmin to move — a nonzero count \
         here means the witness is counting something other than a reversal:\n{s}"
    );
}
