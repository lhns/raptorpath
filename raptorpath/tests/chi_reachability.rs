//! **χ IS REACHABLE, AND WITHOUT THE ARM THE RATE IS BYTE-IDENTICAL.**
//!
//! Paper §14.26 / §16.82 (in flight).
//!
//! **The thing this binary exists to prevent has already happened once.**
//! `FecRateController::set_completion_exposure` shipped with P6 and had
//! **zero engine callers** — the production tunnel is an endless stream, so
//! χ was left at 0 "for now" and nothing ever set it. The consequence was not
//! a missing feature, it was a silently degenerate law: with χ ≡ 0 the Bulk
//! end of the dial has `δ_eff = ε̂`, `z_for_tail_target` returns `−∞`, and
//! `controller_rate` returns **exactly 0**. Every scored L1 battery in this
//! tree ran the bulk hint. **The r leg — the FEC/ARQ trade-off this project is
//! named for — has therefore only ever been measured at its corner, and no
//! test said so.** The function carried an `#[allow(dead_code)]` instead.
//!
//! `RWM_COMPLETION_EXPOSURE` is the wire that makes the interior reachable,
//! and this is the gate that proves the wire conducts:
//!
//!   1. **The gate echo, two-sided.** `RWM_COMPLETION_EXPOSURE=1` on the armed
//!      arm and `=0` on the control, on the run's own `[GATES]` line.
//!   2. **THE MECHANISM EXECUTED** (MEASUREMENT DISCIPLINE rule 1): `[CHI]`
//!      reaches `max > 0.5` on the armed arm — χ is a survival function of
//!      `(T_rem − 1.5·srtt)/σ_ARQ`, so `max > ½` means the glide entered the
//!      region where `δ_eff` has genuinely left `ε̂`, not merely that a
//!      counter moved.
//!   3. **THE LAW ACTUALLY MOVED**: the transfer's coded output `cod > 0` on
//!      the armed arm. χ that never reaches the wire is a gauge, not an arm.
//!   4. **THE CONTROL IS INERT AND SAYS SO**: `[CHI] n=0 max=0.0000` with the
//!      gate off — the feed is not merely unset, it is never READ. `n=0` and
//!      `max=0` are different failures and both are asserted.
//!   5. **BYTE-IDENTITY, disarmed.** The rate site's arithmetic with the gate
//!      off is the arithmetic without this commit, asserted directly against
//!      `controller_rate` rather than inferred from a passing battery.
//!
//! **It fails on the old engine** at clause 2: `[CHI]` does not exist there.
//!
//! **Nothing here flips a default.** `RWM_COMPLETION_EXPOSURE` is ABSENT on
//! every shipped arm and the tunnel path has no feed to give it.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The base arm. `RWM_DIAG` carries `[CHI]`, `[SHEDH]` and `[DIAG] cod=`.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

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

/// Keep the leading numeric prefix — stderr has two interleaving writers and
/// a concurrent write can land inside a gauge line's LAST field (the
/// `alpha_override_reachability` lesson, inherited here).
fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(v.len());
    &v[..end]
}

fn f64_field(line: &str, key: &str) -> f64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn last_line<'a>(log: &'a str, tag: &str) -> Option<&'a str> {
    log.lines().rev().find(|l| l.contains(tag))
}

fn require<'a>(log: &'a str, tag: &str, what: &str) -> &'a str {
    last_line(log, tag).unwrap_or_else(|| panic!("no `{tag}` line — {what}\n--- log ---\n{log}"))
}

/// The MAXIMUM `cod=<n>` the sender printed — read as a max over lines, never
/// off the last one: `[DIAG]`'s counters are INTERVAL counters and reading
/// them off the tail is the harness defect that mis-scored `W4`.
fn max_u64_token(log: &str, key: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// One object loopback run.
///
/// **THE SIZE IS SET BY THE GAUGE'S CADENCE, NOT BY THE PHYSICS.** `[CHI]` is
/// emitted on the engine's 1 s gauge cadence with LAST LINE WINS, so a run
/// that finishes inside one second produces exactly ONE line — the one emitted
/// at `t = 0` before a single ack has arrived, which reads `n=0` whatever the
/// arm did. That is a HARNESS artefact and it is recorded here because it read
/// exactly like the failure the binary exists to detect. Three 3 MB objects at
/// the `c3` cell's 20 Mbit/s run ≈ 1.2 s each, so every arm gets several
/// emissions and the last one carries the transfer's own totals.
fn run(extra: &[(&str, &str)]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv, srv_log) = spawn_perf_server(extra);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "3000000",
        "--runs",
        "3",
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
    // A lossy cell, so the rate law has something to price at all.
    // `c3heavy` and not `c3`, for a reason that IS the arm's own finding: the
    // glide's fully-exposed target is `BULK_TAIL_BUDGET = 0.05`, so on any
    // channel cleaner than 5 % the corner survives full exposure and `r*` is
    // 0 whatever χ does (asserted directly in clause 5 below). `c3`'s ε ≈ 4.8 %
    // sits just BELOW that line; `c3heavy` (ε ≈ 5.8 %) sits just above it. A
    // reachability gate must run where the mechanism can act.
    cli.env("RWM_L0_NETEM", "c3heavy");
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

// ── 1 — THE ARMED ARM: χ is fed, reaches the glide, and reaches the wire ──

#[test]
fn the_completion_exposure_arm_feeds_chi_and_the_rate_reaches_the_wire() {
    let (cli, _srv) = run(&[("RWM_COMPLETION_EXPOSURE", "1")]);

    // (1) THE GATE ECHO. A missing gauge below can then only be read as an
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

    // (2) THE MECHANISM EXECUTED. χ > ½ means the glide entered the region
    // where δ_eff has genuinely left ε̂ — not merely that a counter moved.
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

    // (3) THE LAW REACHED THE WIRE. A χ that moves the controller but emits
    // no coded symbol is a gauge, not an arm.
    assert!(
        max_u64_token(&cli, "cod=") > 0,
        "no coded symbol went on the wire, so r* never left the corner"
    );
}

// ── 2 — THE CONTROL: absent means never READ, and it says so ─────────────

#[test]
fn without_the_arm_chi_is_never_evaluated_and_the_gauge_says_so() {
    let (cli, _srv) = run(&[]);

    let gates = require(&cli, "[GATES]", "the engine never echoed its gates");
    assert!(
        gates.contains("RWM_COMPLETION_EXPOSURE=0"),
        "the control arm must be readable as OFF: {gates}"
    );
    assert!(
        !cli.contains("completion-exposure feed ACTIVE"),
        "the control arm must publish NO feed"
    );

    // The gauge fires on EVERY arm (MEASUREMENT DISCIPLINE 15), and on this
    // one it must read zero on BOTH fields: `n=0` says the site was never
    // evaluated, `max=0` says no value was ever produced. They are different
    // failures and a control that only pinned one would miss the other.
    let chi = require(
        &cli,
        "[CHI] ",
        "the χ gauge must fire on the control arm too, or gate-off is not readable",
    );
    assert_eq!(f64_field(chi, "n="), 0.0, "χ was evaluated with the gate off: {chi}");
    assert_eq!(f64_field(chi, "max="), 0.0, "χ took a value with the gate off: {chi}");
    assert_eq!(f64_field(chi, "frac_gt_half="), 0.0, "{chi}");
}

// ── 3 — DISARMED BYTE-IDENTITY, asserted directly ────────────────────────

/// With χ = 0 the rate is EXACTLY what it was before this commit existed —
/// asserted against `controller_rate` itself rather than inferred from a
/// battery that happened not to move. `assert_eq!`, not a tolerance.
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
    // The corner, exactly: δ_eff = p ⇒ z = −∞ ⇒ r* = 0 identically. This is
    // the number every scored bulk battery in this tree actually ran at.
    assert_eq!(controller_rate(&base), 0.0);
    // And it is the corner across the whole estimator range, not at one point.
    for p in [0.001f64, 0.01, 0.05, 0.2, 0.5] {
        let mut i = RateInputs { p_upper: p, ..base };
        i.completion_exposure = 0.0;
        assert_eq!(controller_rate(&i), 0.0, "p={p}: the χ = 0 corner moved");
    }
    // χ > 0 LEAVES the corner — the property the arm exists to reach. Stated
    // here so "the gate is inert" and "the gate does nothing" stay distinct.
    //
    // **AND IT ONLY LEAVES IT WHERE `ε̂ > BULK_TAIL_BUDGET`.** The glide's
    // fully-exposed target is `δ_eff = 0.05`, so at any channel cleaner than
    // 5 % the ratio `δ_eff/p ≥ 1`, `z = −∞`, and `r* = 0` — **the corner
    // survives full exposure**. This is not a defect of the arm; it is
    // §16.82's own finding about `BULK_TAIL_BUDGET` (register: *"an `e.g.`
    // promoted to a `const`"*) reproduced as arithmetic, and it is asserted in
    // BOTH directions here so an `R-INERT` battery verdict can be attributed
    // to the budget rather than to the wiring.
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
    // The crossing is the budget itself and nothing else — a THRESHOLD in the
    // arithmetic of one continuous law, not a branch in the code.
    assert_eq!(raptorpath_math::BULK_TAIL_BUDGET, 0.05);
}
