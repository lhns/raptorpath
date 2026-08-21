//! WHY EACH RECOVERY FIRE FIRED — `[FCAUSE]` — AND THE CONFIGURATION
//! CONTRACT THE CAUSE MIX IS ONLY READABLE UNDER.
//!
//! **The question this instrument was built to close.** The quantile-native
//! α-sweep (goal-gate, "qnative sweep SCORED") moved the realized recovery
//! clock `W` cleanly across six arms — a 200× span in the contract α, with
//! `[QALPHA] win_n` tracking it arm for arm — and the commanded false-alarm
//! fraction `[RACK] fa_frac` DID NOT MOVE at 4 of 5 cells. `fa ⊥ W`.
//!
//! That independence refutes the shared premise of both §16.69 routes: that
//! the recovery fires are timer-driven, so that repositioning the waiting
//! time repositions the fires. A clock the fires do not respond to is not the
//! clock that decides them, and the measurand derived from it — the
//! ack-arrival distribution — is therefore the wrong quantity to position a
//! waiting time on.
//!
//! The only explanation the code leaves standing is that MOST FIRES ARE NOT
//! TIMER-DRIVEN. `[FCAUSE]` classifies them, so the successor measurand is
//! named from a count rather than from an argument.
//!
//! **The classification, read off the code and not invented.**
//! `RackClockGauge::record_fire` has ONE call site, the sender's gap loop, and
//! that loop's `gaps` vector has exactly two producers:
//!
//!   * `timer` — the sender's own tail-sweep deadline arm. This is the ONLY
//!     cause the quantile/Cantelli `W` clocks: `tail_deadline` is computed
//!     from `sweep_timeout_us_all`, and nothing else in the loop reads it.
//!   * the `nack_rx` channel, fed solely by the SACK→gap inversion in the
//!     WindowAck handler — clocked by the RECEIVER, never by the sender's `W`.
//!     Its two receiver arms are separable for free, because the timer-driven
//!     hole re-advertisement broadcasts ONE message to every live path and so
//!     cannot carry a per-path echo: it stamps `echo_send_timestamp_us: 0`,
//!     the sentinel the handler ALREADY branches on for its RTT update.
//!     `gap_refresh` is that arm; `gap_data` is the dupack analog, driven by
//!     data arrival and by no clock at all.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. `fcause_report_line`'s FORMAT, so an L1 parser has a pin — including
//!      the `-`-iff-no-denominator rule, so an absent reading can never be
//!      read as a measured zero.
//!   2. **THE LINE FIRES AND ITS CAUSES ARE POPULATED**, over a `c3`-lossy
//!      plain-window loopback with the quantile clock ARMED. THIS IS THE
//!      ASSERTION THAT FAILS ON THE OLD ENGINE: `[FCAUSE]` does not exist
//!      there and no fire was ever attributed to a cause.
//!   3. The line is INTERNALLY CONSISTENT: the four classes sum to `n`,
//!      `other` is EMPTY (an unclassifiable fire is counted, never guessed —
//!      this asserts the tag reaches every producer rather than assuming it),
//!      and the two fractions agree with the counts they are formed from.
//!   4. `n` AGREES WITH AN INDEPENDENT WITNESS. `[DIAG] retx=` is bumped at
//!      the same emission, by different code, so `n` and `retx` count the
//!      same events. A gauge that double-counts a cause is caught here.
//!   5. `n >= [RACK] fired`, and the difference is what `unattr=` reports.
//!      This is NOT a defect of this gauge: `record_fire` sits inside
//!      `if let Some(mp_flight)`, so `fa=`'s denominator has always dropped
//!      fires whose target had no live-flight record. `[FCAUSE]` counts at
//!      the emission, after every suppression `continue`. The discrepancy is
//!      PRINTED rather than repaired, because moving `fired` would silently
//!      re-base every reading the sweep already scored.
//!   6. **THE CONFIGURATION CONTRACT.** Under GENERATION coding the SACK→gap
//!      producer is suppressed (`recv_nack_tx = None`), so BOTH `gap_` classes
//!      are structurally empty and the cause mix is not a measurement of the
//!      shipped plain-window machine at all. The line echoes `gen=` so no row
//!      is ever read out of its configuration scope.
//!
//! **No new gate.** `[FCAUSE]` rides `[RACK]`'s own ungated `Drop` rule; there
//! is no new dial to echo two-sidedly. `RWM_DIAG=1` is asserted present in the
//! `[GATES]` echo, so a missing `[DIAG] retx=` can only be read as an
//! unreached site.
//!
//! **What this deliberately does NOT assert.** Any particular cause MIX.
//! Loopback's loss is the shim's GE process, not a network's; the ratio that
//! answers the measurand question comes off the L1 diagnostic pass. This is
//! the instrument gate that must pass before that pass is worth making.
//!
//! Own test binary: `RWM_L0_NETEM` is process-global in the child, and the
//! spawned pair must not contend with the in-process loopback tests.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use raptorpath::net::fcause_report_line;

// ── 1: THE PURE PIN ─────────────────────────────────────────────────────

#[test]
fn the_fcause_line_format_is_pinned() {
    // timer=12 gap_data=430 gap_refresh=58 other=0 ⇒ n = 500.
    //   timer_frac = 12/500 = 0.0240, gap_frac = 488/500 = 0.9760.
    //   fired = 494 ⇒ unattr = 6.
    assert_eq!(
        fcause_report_line(12, 430, 58, 0, 494, false),
        "[FCAUSE] gen=0 n=500 timer=12 gap_data=430 gap_refresh=58 other=0 \
         timer_frac=0.0240 gap_frac=0.9760 fired=494 unattr=6 fa_class=0.0625"
    );

    // NEVER FIRED reads `-`, never `0.0000`: a fraction with no denominator
    // is ABSENT, and an absent reading must not be poolable with a measured
    // zero. This is the rule the `[DIAG]` candidate estimators use.
    let empty = fcause_report_line(0, 0, 0, 0, 0, false);
    assert_eq!(
        empty,
        "[FCAUSE] gen=0 n=0 timer=0 gap_data=0 gap_refresh=0 other=0 \
         timer_frac=- gap_frac=- fired=0 unattr=0 fa_class=0.0625"
    );
    assert!(
        !empty.contains("timer_frac=0"),
        "an unfired gauge must not render a fraction: {empty}"
    );

    // THE GENERATION ROW. Both `gap_` classes are structurally empty there,
    // so a 1.0000 timer fraction is a CONFIGURATION fact, and `gen=1` is what
    // says so on the line's face.
    let g = fcause_report_line(37, 0, 0, 0, 37, true);
    assert!(g.contains("gen=1"), "{g}");
    assert!(g.contains("timer_frac=1.0000"), "{g}");
    assert!(g.contains("gap_frac=0.0000"), "{g}");
    assert!(g.contains("unattr=0"), "{g}");

    // The trailing sacrificial constant: a concurrent `tracing` write corrupts
    // the LAST field, so the last field is a constant every parser knows.
    for l in [&empty, &g] {
        assert!(
            l.trim_end().ends_with("fa_class=0.0625"),
            "every gauge line ends on the class bar: {l}"
        );
    }
}

// ── 2-6: THE REACHABILITY RUN ───────────────────────────────────────────

/// The arm. The quantile clock is ARMED at the sweep's `Q009` probe point, so
/// the timer whose fires this test is counting is the one the sweep measured.
/// `RWM_DIAG` carries `[DIAG] retx=`, the independent witness. No gate here
/// changes a law: `RWM_QUANTILE_CLOCKS` selects which recovery cadence the
/// sender computes, and the sweep already scored both sides of it.
const ARM: [(&str, &str); 5] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RWM_QUANTILE_CLOCKS", "1"),
    ("RWM_W_FORM", "quantile"),
    ("RWM_ALPHA_OVERRIDE", "0.009"),
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

fn spawn_perf_server(generation: bool) -> (SocketAddr, Reaper, Arc<Mutex<String>>) {
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
    if generation {
        cmd.arg("--window-generation-coding");
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    // The shim shapes the CLIENT's egress; the server's own datagram path
    // (the ack direction) is left clean so acks are not the thing under test.
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

/// Parse `key=<value>` out of one whitespace-tokenised gauge line.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn f64_field(line: &str, key: &str) -> f64 {
    let v = field(line, key);
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

/// Run ONE lossy loopback in the given configuration. Returns
/// `(client log, server log)`. The CLIENT is the bulk-direction SENDER and so
/// is the site whose `[FCAUSE]` this test is about.
fn lossy_run(generation: bool) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv, srv_log) = spawn_perf_server(generation);

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
    if generation {
        cli.arg("--window-generation-coding");
    }
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    // The L1 `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms jitter,
    // GE p = 2 % / q = 40 % ⇒ ε ≈ 4.8 %) on client egress, seeded. Loss is
    // what forces the recovery fires this gauge classifies to exist.
    cli.env("RWM_L0_NETEM", "c3");
    cli.env("RWM_L0_SEED", "42");

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed (generation={generation}, {:?})\n--- stdout ---\n\
         {cli_stdout}\n--- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

/// `[DIAG]`'s cumulative retransmit count, MAX over all lines (the W4'
/// convention: the periodic readout can be cut off mid-transfer, so the
/// largest reading is the one that saw the most).
fn max_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| v.parse::<u64>().ok())
        .max()
        .unwrap_or_else(|| panic!("no `retx=` in the sender log — [DIAG] never fired"))
}

/// 2-5. THE CAUSES ARE PRESENT, CONSISTENT, AND WITNESSED.
#[test]
fn every_recovery_fire_is_attributed_to_a_named_cause() {
    let (cli, _srv) = lossy_run(false);

    // THE GATE. A missing witness must be readable as an unreached site and
    // never as an unset gate.
    assert!(
        cli.contains("RWM_DIAG=1"),
        "the client's [GATES] echo does not carry RWM_DIAG=1 — the arm did \
         not arm:\n{cli}"
    );
    // The clock under test is ARMED, so the `timer` class is the sweep's own
    // timer and a zero there is a finding rather than a disarmed dial.
    assert!(
        cli.contains("RWM_QUANTILE_CLOCKS=1"),
        "the quantile clock did not arm — the `timer` class would not be the \
         clock the sweep measured:\n{cli}"
    );

    // 2. THE LINE FIRES. This is what fails on the old engine.
    let last = cli
        .lines()
        .rev()
        .find(|l| l.contains("[FCAUSE] "))
        .unwrap_or_else(|| {
            panic!(
                "no [FCAUSE] line from the PLAIN-WINDOW sender over a c3-lossy \
                 transfer — no recovery fire was attributed to any cause, which \
                 is the DEAD-INSTRUMENT reading this test exists to fail on:\n{cli}"
            )
        });
    println!("[fcause-reach] PLAIN: {last}");

    let n = u64_field(last, "n=");
    let timer = u64_field(last, "timer=");
    let gap_data = u64_field(last, "gap_data=");
    let gap_refresh = u64_field(last, "gap_refresh=");
    let other = u64_field(last, "other=");
    let fired = u64_field(last, "fired=");
    let unattr = u64_field(last, "unattr=");

    assert!(
        last.contains("gen=0"),
        "this run is PLAIN WINDOW — the sweep's configuration — and the line \
         must say so: {last}"
    );
    assert!(
        n > 0,
        "[FCAUSE] n=0 over a c3-lossy plain-window transfer: the gap loop \
         never fired, so there is no cause mix to read:\n{last}"
    );

    // 3. INTERNAL CONSISTENCY.
    assert_eq!(
        n,
        timer + gap_data + gap_refresh + other,
        "[FCAUSE] n is not the sum of its four causes: {last}"
    );
    // `other` is the NAMED unclassifiable class, and it must be EMPTY: every
    // producer of the gap loop's batches carries a tag. This asserts the
    // plumbing reaches all of them rather than assuming it.
    assert_eq!(
        other, 0,
        "[FCAUSE] other={other} — a gap batch reached the fire site with no \
         cause tag, so some producer is unplumbed and its fires are \
         unattributed:\n{last}"
    );
    let timer_frac = f64_field(last, "timer_frac=");
    let gap_frac = f64_field(last, "gap_frac=");
    assert!(
        (timer_frac - timer as f64 / n as f64).abs() < 1e-3,
        "[FCAUSE] timer_frac={timer_frac} disagrees with {timer}/{n}: {last}"
    );
    assert!(
        (gap_frac - (gap_data + gap_refresh) as f64 / n as f64).abs() < 1e-3,
        "[FCAUSE] gap_frac={gap_frac} disagrees with \
         ({gap_data}+{gap_refresh})/{n}: {last}"
    );
    assert!(
        (timer_frac + gap_frac - 1.0).abs() < 1e-3,
        "[FCAUSE] with other=0 the two fractions must partition the fires: \
         {timer_frac} + {gap_frac} != 1 in {last}"
    );

    // 4. THE INDEPENDENT WITNESS. `[DIAG] retx=` is bumped by different code
    //    at the same emission, so it counts the same events. `>=` rather than
    //    `==` because the periodic `[DIAG]` readout can be cut off before the
    //    final fires while `[FCAUSE]` emits at teardown.
    let retx = max_retx(&cli);
    println!("[fcause-reach] n={n} vs [DIAG] retx={retx} (unattr={unattr})");
    assert!(retx > 0, "[DIAG] retx=0 while [FCAUSE] n={n}: {last}");
    assert!(
        n >= retx,
        "[FCAUSE] n={n} is BELOW the independent witness [DIAG] retx={retx} — \
         the cause counters are missing fires the gap loop emitted:\n{last}"
    );

    // 5. THE DENOMINATOR DISCREPANCY IS REPORTED, NOT HIDDEN. `record_fire`
    //    sits inside `if let Some(mp_flight)`; `[FCAUSE]` does not. `fired`
    //    is therefore a SUBSET, and `unattr` names the difference.
    assert!(
        n >= fired,
        "[FCAUSE] n={n} < [RACK] fired={fired} — `fired` counts a strict \
         subset of the emissions and cannot exceed the true fire count:\n{last}"
    );
    assert_eq!(
        unattr,
        n - fired,
        "[FCAUSE] unattr must be exactly n - fired: {last}"
    );

    // And the `[RACK]` line it sits beside must agree about `fired`.
    if let Some(rack) = cli.lines().rev().find(|l| l.contains("[RACK] ")) {
        let fa = field(rack, "fa=");
        let (_sp, fd) = fa
            .split_once('/')
            .unwrap_or_else(|| panic!("fa= must render `<spurious>/<fired>`: {rack}"));
        assert_eq!(
            fd.parse::<u64>().expect("fired parses"),
            fired,
            "[FCAUSE] fired={fired} disagrees with [RACK] fa=.../{fd} — the \
             two lines read the same counter:\n{rack}\n{last}"
        );
    }
}

/// 6. THE CONFIGURATION CONTRACT. The cause mix is only a measurement of the
/// shipped machine in PLAIN WINDOW: generation coding suppresses the SACK→gap
/// producer outright, so both `gap_` classes are structurally empty there and
/// a row from that arm does not pool with a plain-window row.
#[test]
fn the_gap_causes_are_structurally_empty_under_generation() {
    let (gen_cli, _gen_srv) = lossy_run(true);

    // `recv_nack_tx = None` under generation, so the per-seq retransmit path
    // does not run at all — the same fact `rfa_reachability` measures.
    let gen_retx: u64 = gen_cli
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| v.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    println!("[fcause-reach] GENERATION retx={gen_retx}");

    match gen_cli.lines().rev().find(|l| l.contains("[FCAUSE] ")) {
        Some(l) => {
            println!("[fcause-reach] GENERATION: {l}");
            assert!(
                l.contains("gen=1"),
                "[FCAUSE] from a generation sender must echo gen=1 so no row \
                 is read out of its configuration scope: {l}"
            );
            assert_eq!(
                u64_field(l, "gap_data="),
                0,
                "[FCAUSE] gap_data must be 0 under generation — the SACK→gap \
                 producer is suppressed there: {l}"
            );
            assert_eq!(
                u64_field(l, "gap_refresh="),
                0,
                "[FCAUSE] gap_refresh must be 0 under generation — the \
                 SACK→gap producer is suppressed there: {l}"
            );
            assert_eq!(
                u64_field(l, "other="),
                0,
                "[FCAUSE] other must be 0 in every configuration: {l}"
            );
        }
        None => {
            // Legal and expected: with no gap producer AND no tail-sweep fire,
            // nothing was classified and the gauge stays silent by the same
            // rule `[RACK]` uses. What is NOT legal is a silent gauge beside a
            // retransmit — that would be an unattributed fire.
            assert_eq!(
                gen_retx, 0,
                "no [FCAUSE] line under generation while [DIAG] retx={gen_retx} \
                 — fires were emitted and none was classified:\n{gen_cli}"
            );
        }
    }
}
