//! **§16.77 THE HOLD-DOWN CLOCK — THE REACHABILITY GATE.**
//!
//! The fire-cause pass counted **0.59 % of 107 597 classified recovery fires
//! from a timer and 98.99 % from the sender answering a receiver gap report**.
//! Every recovery clock this tree has written sets the TIMER. `RWM_HOLDDOWN_Q`
//! is the first knob pointed at the other 99 %, and this binary asserts, in the
//! order it can fail, that it REACHES that path on the real engine over a real
//! lossy wire:
//!
//! 1. **THE GATE IS ECHOED, TWO-SIDED, AT BOTH ENDPOINTS.** `[GATES]` prints
//!    `RWM_HOLDDOWN_Q=<resolved>` — a number on the armed arm and `unset` on
//!    the control. A missing gauge below can then only be read as an unreached
//!    emission site and never as an unset gate.
//! 2. **THE GAUGE EXISTS AND ITS WINDOW LAW IS THE ONE THE ARM ASKED FOR.**
//!    `[HOLD] site=sender q=0.500000 n_req=20`.
//! 3. **THE SITE EXECUTED** — `evals > 0`. MEASUREMENT DISCIPLINE rule 1: prove
//!    the mechanism under test runs before reading anything it produced.
//! 4. **THE ESTIMATOR WAS FED AND ITS OWN LAW RAN** — `fed > 0`, `samp_n > 0`,
//!    `law_n > 0`, and `t_us` is a number rather than `-`.
//! 5. **THE GATE ACTUALLY SUPPRESSED A FIRE** — `sup > 0`. This is the clause
//!    that distinguishes a knob that is read from a knob that DECIDES, and it
//!    is the whole reason the arm exists.
//! 6. **THE ACCOUNTING CLOSES** — `evals = sup + emit` on every line, without
//!    which `sup=` is a number nobody can place.
//! 7. **THE CONTROL IS INERT AND SAYS SO** — absent ⇒ `q=unset`, `n_req=-`,
//!    `sup=0`, `law_n=0`, `fed=0`, and the fires still reach the wire.
//! 8. **GARBAGE RESOLVES BACK TO ABSENT, VISIBLY** — an unparseable level, a
//!    level at or above 1, and a level at or below 0 all print `unset`, so a
//!    mistyped arm is READ rather than inferred.
//!
//! **THIS BINARY FAILS ON THE PRE-CHANGE ENGINE**: `RWM_HOLDDOWN_Q`, the
//! `[HOLD]` line, `q=` and `n_req=` do not exist there, so every clause above
//! reads a missing field.
//!
//! **What this binary deliberately does NOT assert.** Any FIELD value of `T`,
//! of the suppression fraction, or of goodput. Loopback's reordering is the host
//! scheduler's and its loss is the shim's GE process; no claim about any cell
//! can be made from it. This is the instrument gate that must pass before the L1
//! sweep is worth making.
//!
//! **Nothing here flips a default.** `RWM_HOLDDOWN_Q` is ABSENT by default and
//! nothing shipped reads it.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The base arm. `RWM_DIAG` carries `[DIAG] retx=` and the receiver's periodic
/// `[QCLK]` readouts. No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The swept arm's level. `q = 0.5` is §16.77.8's DERIVED FLOOR arm — the
/// window law is flat at `N = 2K = 20` there, which is the fastest-filling
/// level the construction can express and therefore the one a loopback run can
/// actually reach. **The window law and the order statistic are pinned
/// ABSOLUTELY in `recovery_bench.rs`; this binary pins that the ENGINE ROUTES
/// TO THEM and that the gate it opens actually suppresses a fire.**
const HQ: &str = "0.5";
const HQ_ECHO: &str = "q=0.500000";
const N_REQ: &str = "n_req=20";

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

/// Spawn the perf SERVER — the RECEIVER of the bulk direction, and one of the
/// two sites that owns a quantile clock.
fn spawn_perf_server(extra: &[(&str, &str)]) -> (SocketAddr, Reaper, Arc<Mutex<String>>) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let mut cmd = Command::new(bin);
    cmd.args([
        "perf",
        "--server",
        "--bind",
        &addr.to_string(),
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.env_remove("RWM_L0_NETEM");
    // The absent arm must be ABSENT: inheritance defeats an allowlist, and the
    // whole point of the control arm is that NOTHING set the level.
    if !extra.iter().any(|(k, _)| *k == "RWM_HOLDDOWN_Q") {
        cmd.env_remove("RWM_HOLDDOWN_Q");
    }
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
    assert!(
        seen.contains("perf server ready"),
        "perf server never became ready; it said: {seen}"
    );
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
    (addr, srv, log)
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

/// Keep the leading numeric prefix of a token — stderr has two writers and a
/// `tracing` write can land inside a gauge line's LAST field. Every gauge here
/// ends on the constant `fa_class=`, and this is the reader's half.
fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(v.len());
    &v[..end]
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn f64_field(line: &str, key: &str) -> f64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn require<'a>(log: &'a str, tag: &str, what: &str) -> &'a str {
    log.lines()
        .rev()
        .find(|l| l.contains(tag))
        .unwrap_or_else(|| panic!("no `{tag}` line — {what}\n--- log ---\n{log}"))
}

/// One lossy loopback run in the given gate configuration.
/// Returns `(client/sender log, server/receiver log)`.
fn lossy_run(extra: &[(&str, &str)]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv, srv_log) = spawn_perf_server(extra);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "4000000",
        "--runs",
        "2",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    for (k, v) in extra {
        cli.env(k, v);
    }
    if !extra.iter().any(|(k, _)| *k == "RWM_HOLDDOWN_Q") {
        cli.env_remove("RWM_HOLDDOWN_Q");
    }
    // The L1 `c3` cell (LTE-class) on client egress, seeded. Loss is what
    // drives the recovery clock this test is about at all.
    cli.env("RWM_L0_NETEM", "c3");
    cli.env("RWM_L0_SEED", "42");

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed ({extra:?}, {:?})\n--- stdout ---\n{cli_stdout}\n\
         --- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

/// The MAXIMUM `retx=<n>` the sender printed. Read as a max over lines and
/// never off the last one — `retx=` in the `[DIAG]` tail is an INTERVAL
/// counter, and reading it off the last line made the plain-window pass report
/// `W4` failing at 5 of 15 reps whose `[RACK] fired` was 11-5 717.
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

/// Every `[HOLD]` line the sender printed, newest last. One per path that saw a
/// fire, plus the unattributed bucket — `path=-`, the timer fires this arm does
/// not touch.
fn hold_lines(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|l| l.contains("[HOLD] site=sender"))
        .collect()
}

/// The `[HOLD]` line for a real path (`path=` is a number, not `-`). The
/// unattributed bucket carries no window and no law, so pooling it with a real
/// path's row would report a law that never ran as a law that ran and did
/// nothing — the A7 pathology.
fn hold_pathline<'a>(lines: &[&'a str]) -> &'a str {
    lines
        .iter()
        .copied()
        .find(|l| !l.contains("path=-"))
        .unwrap_or_else(|| {
            panic!("no per-path `[HOLD]` line — the gap-report site never ran:\n{lines:#?}")
        })
}

/// The accounting identity, asserted on EVERY line of EVERY arm: a fire is
/// either held or emitted, and there is no third place for it to go.
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

// ── 1 — THE ARMED ARM: set, echoed, routed, fed, and it SUPPRESSES ───────

#[test]
fn the_holddown_arms_echoes_routes_feeds_its_estimator_and_suppresses_a_fire() {
    let (cli, srv) = lossy_run(&[("RWM_HOLDDOWN_Q", HQ)]);

    // (1) THE GATE ECHO, both endpoints, two-sided.
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

    // (2) THE GAUGE, AND ITS WINDOW LAW IS THE ONE THE ARM ASKED FOR.
    assert!(
        l.contains(HQ_ECHO),
        "the gauge must print the RESOLVED level: {l}"
    );
    assert!(
        l.contains(N_REQ),
        "N(1-q) must be the window law's own answer at this level: {l}"
    );

    // (3) THE SITE EXECUTED — MEASUREMENT DISCIPLINE rule 1.
    let evals = u64_field(l, "evals=");
    assert!(evals > 0, "the gap-report response site never ran: {l}");

    // (4) THE ESTIMATOR WAS FED AND ITS OWN LAW RAN.
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

    // (5) THE GATE DECIDED SOMETHING. This is the clause that separates a knob
    // that is READ from a knob that DECIDES.
    let sup = u64_field(l, "sup=");
    assert!(
        sup > 0,
        "the hold-down never suppressed a single fire — the knob is inert: {l}"
    );

    // And the realized hold-down delay is reported as a DISTRIBUTION, because a
    // mean would hide exactly the tail the level commands.
    for k in ["hd_p50_us=", "hd_p90_us=", "hd_p99_us=", "hd_mx_us=", "hd_n="] {
        let _ = field(l, k);
    }

    // The wire still worked: holes that cleared the hold-down were repaired.
    assert!(
        max_retx(&cli) > 0,
        "the sender retransmitted nothing at all — the hold-down starved the plane"
    );
}

// ── 2 — THE CONTROL: absent is INERT, and it SAYS SO ─────────────────────

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
    // The control's gauge is still emitted — MEASUREMENT DISCIPLINE 15 — so an
    // absent `[HOLD]` line can only be read as an unreached site.
    for l in &lines {
        assert!(l.contains("q=unset"), "the control must say `q=unset`: {l}");
        assert!(l.contains("n_req=-"), "no window law is in force: {l}");
        assert_eq!(u64_field(l, "sup="), 0, "the control must hold NOTHING: {l}");
        assert_eq!(u64_field(l, "law_n="), 0, "the control runs no law: {l}");
        assert_eq!(u64_field(l, "fed="), 0, "the control feeds no estimator: {l}");
        assert!(l.contains("t_us=-"), "the control has no T: {l}");
    }
    let evals: u64 = lines.iter().map(|l| u64_field(l, "evals=")).sum();
    assert!(
        evals > 0,
        "the control's own site must still have run — otherwise clause 3 of the \
         armed arm proves nothing about a difference"
    );
    assert!(max_retx(&cli) > 0, "the control retransmitted nothing");
}

// ── 3 — GARBAGE RESOLVES BACK TO ABSENT, VISIBLY ─────────────────────────

#[test]
fn a_garbage_holddown_level_resolves_back_to_absent_and_prints_unset() {
    // Unparseable; at the top of the domain, where the window law diverges;
    // and at the bottom, where the hold-down IS zero and the shipped machine is
    // expressed by ABSENCE rather than by an armed arm (§16.77.10).
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

// ── 4 — THE TWO ARMS ARE TWO ARMS ────────────────────────────────────────

#[test]
fn the_armed_and_disarmed_arms_realize_different_gap_report_behaviour() {
    let (armed, _) = lossy_run(&[("RWM_HOLDDOWN_Q", HQ)]);
    let (ctl, _) = lossy_run(&[]);

    let a: u64 = hold_lines(&armed).iter().map(|l| u64_field(l, "sup=")).sum();
    let c: u64 = hold_lines(&ctl).iter().map(|l| u64_field(l, "sup=")).sum();
    assert!(a > 0, "the armed arm suppressed nothing");
    assert_eq!(c, 0, "the control suppressed something — it is not a control");

    // The suppression is visible OUTSIDE the gauge that counts it: `[FCAUSE]`
    // counts only the fires that reached the wire, so a held fire is one the
    // classifier never saw. A gauge that agreed only with itself would be a
    // gauge that measured itself.
    // THE EXACT CROSS-GAUGE IDENTITY THAT PROVES WHERE THE GATE SITS.
    // `should_hold` is consulted exactly once per fire that reaches
    // `record_fire_cause`, and the fire is then either HELD or CLASSIFIED. So
    //
    //     sum([HOLD] evals)  ==  sum([HOLD] sup)  +  [FCAUSE] n
    //
    // holds on BOTH arms, exactly, and it is the one assertion here that a
    // gauge agreeing only with itself could not pass.
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
