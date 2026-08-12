//! THE TEARDOWN GAUGES ARE REACHABLE UNDER A `perf`-SHAPED EXIT.
//!
//! This binary exists because of a measured defect, and it asserts the one
//! thing the defect's own test suite never asserted.
//!
//! **The defect.** `[CCAP]` (`RWM_COMPOSED_CAP`) and `[WALL]`
//! (`RWM_WALLDIAG`) shipped emitting from exactly two arms of
//! `run_window_sender`'s `select!`: the `shutdown_rx` arm and the
//! `packet == None` "TUN closed" arm. The `perf` harness — `crate::perf`, the
//! object benchmark **every L1 battery runs** — is not guaranteed to reach
//! either. `perf::client` finishes its objects, prints its summary and
//! returns; nothing signals shutdown, and the memory TUN closes only as a
//! side effect of the `MemTun` handle dropping, which RACES the process
//! teardown that is already under way. Whoever wins decides whether the run
//! has a gauge. The composed-cap battery's pre-launch smoke measured the
//! outcome on the VM: `window sender shut down gracefully` 0/4, `TUN closed`
//! 1/4, and therefore `[CCAP]` on 0 of 2 logs and `[WALL]` on 1 of 4
//! (goal-gate, "PRE-BATTERY SMOKE"). ~192 invocations were about to be spent
//! producing five UNSCORED predictions for want of a gauge.
//!
//! **Why every existing test passed.** The unit pins assert
//! `ccap_report_line` / `walldiag::report_line`'s FORMAT — the string an L1
//! parser scrapes. `composed_cap_loopback` and `walldiag_loopback` drive a
//! real `perf::client` transfer and assert the gate echo, the in-process
//! gauge state and behaviour neutrality. Not one of them asked whether the
//! LINE FIRES. That is ADR-0070's postmortem shape one layer down: every pin
//! asserting that the code computes the model, none asking whether the model
//! is reached.
//!
//! **The fix under test.** Both gauges are now emitted by the destructor of
//! `net::SenderTeardownGauges`, a local of `run_window_sender`. A destructor
//! is on EVERY exit of a scope — both teardown arms, any early return, an
//! unwind, and the case the harness actually exercises, the task future being
//! dropped at runtime shutdown — and it runs exactly once.
//!
//! **The two tests, and why both are needed.**
//!
//! * `the_teardown_gauges_fire_exactly_once_under_the_shipped_perf_harness`
//!   runs the SHIPPED binary through the SHIPPED `perf` subcommand — literally
//!   what the L1 driver invokes — and asserts exactly one `[CCAP]` and one
//!   `[WALL]`, fed with real numbers. It cannot by itself discriminate the
//!   fix, because it cannot choose which side of the teardown race it lands
//!   on; it prints the side it took.
//!
//! * `the_teardown_gauges_fire_when_the_sender_task_is_dropped_at_runtime_shutdown`
//!   removes the race and IS the discriminator. A child process holds the
//!   `MemTun` alive across the runtime's drop, so the sender reaches NEITHER
//!   teardown arm and ends only by having its future dropped — the losing
//!   side of the race, made deterministic. On the shipped-before code this
//!   run emits nothing at all.
//!
//! **Residual gap, stated rather than papered over.** The graceful-shutdown
//! arm is driven by Ctrl+C in the shipped binary, which is not portably
//! deliverable to a child on both Linux and Windows, and the real TUN-closed
//! arm needs a TUN device (root). Neither is asserted directly here. Both are
//! covered by construction: they are exits of `run_window_sender`, and a
//! destructor is on every exit of a scope — so the paths not asserted are a
//! SUBSET of the paths asserted. That is the exact inversion of the shipped
//! bug, where the two asserted paths were DISJOINT from the one the harness
//! took.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Env var by which this binary re-executes itself as the deterministic
/// runtime-drop child (see the second test).
const CHILD_PEER: &str = "RWM_GAUGE_CHILD_PEER";

/// The composed arm exactly as the battery configures it: the composed pool
/// law + its late-stage brake, on honest anchors, with both gauges' gates ON.
/// `RWM_THREE_TERM` stays OFF — the composed gate reaches the pool seat on its
/// own (see `composed_cap_loopback`).
const ARM: [(&str, &str); 4] = [
    ("RWM_COMPOSED_CAP", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RWM_WALLDIAG", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// A port nothing else in the suite binds. Taken from the OS so parallel test
/// binaries cannot collide, then released — the engine binds UDP and the probe
/// binds TCP, so the reservation is advisory but the number is unique.
fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    l.local_addr().expect("probe addr").port()
}

/// Kill a child and reap it — a leaked perf server would hold its port.
struct Reaper(Child);
impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn the shipped binary as a `perf` server on a fresh port and block until
/// it reports ready. Returns the address and the reaper.
fn spawn_perf_server() -> (SocketAddr, Reaper) {
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
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    // The child must NOT inherit the re-exec marker.
    cmd.env_remove(CHILD_PEER);
    let mut srv = Reaper(cmd.spawn().expect("spawn perf server"));

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
    // Drain the rest so a full pipe buffer cannot wedge the server mid-run.
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = out.read_to_end(&mut sink);
    });
    (addr, srv)
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

/// Pull `key=<f64>` out of a gauge line.
fn field(line: &str, key: &str) -> Option<f64> {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .and_then(|v| v.parse::<f64>().ok())
}

/// THE REACHABILITY ASSERTION, shared by both tests: in a log produced by ONE
/// window sender, `[CCAP]` and `[WALL]` each appear EXACTLY once — not "at
/// least once", because the battery parsers read a per-run scalar and two
/// lines is as wrong as none — and each carries a real measurement rather
/// than an empty struct rendered at the right moment.
fn assert_one_fed_gauge_of_each(log: &str, what: &str) {
    // Mechanism liveness first: a missing line must be readable as an
    // unreachable emission site and never as an unset gate.
    assert!(
        log.contains("RWM_COMPOSED_CAP=1"),
        "{what}: the [GATES] echo does not carry RWM_COMPOSED_CAP=1:\n{log}"
    );
    assert!(
        log.contains("RWM_WALLDIAG=1"),
        "{what}: the [GATES] echo does not carry RWM_WALLDIAG=1:\n{log}"
    );

    let ccap: Vec<&str> = log.lines().filter(|l| l.contains("[CCAP]")).collect();
    let wall: Vec<&str> = log.lines().filter(|l| l.contains("[WALL]")).collect();
    assert_eq!(
        ccap.len(),
        1,
        "{what}: expected the run's ONE [CCAP] line, got {}: {ccap:?}\n\
         --- full log ---\n{log}",
        ccap.len()
    );
    assert_eq!(
        wall.len(),
        1,
        "{what}: expected the run's ONE [WALL] line, got {}: {wall:?}\n\
         --- full log ---\n{log}",
        wall.len()
    );
    let ccap = ccap[0];
    let wall = wall[0];
    println!("[gauge-reachability/{what}] {ccap}");
    println!("[gauge-reachability/{what}] {wall}");

    let eng = ccap
        .split_whitespace()
        .find_map(|t| t.strip_prefix("eng="))
        .unwrap_or_else(|| panic!("{what}: [CCAP] must carry eng=<engaged>/<refreshes> — {ccap}"));
    let (engaged, refreshes) = eng.split_once('/').expect("eng=<a>/<b>");
    let refreshes: u64 = refreshes.parse().expect("eng= denominator");
    let engaged: u64 = engaged.parse().expect("eng= numerator");
    assert!(
        refreshes > 0,
        "{what}: [CCAP] reports {refreshes} dyn-cap refreshes — the tally was \
         never fed, so the line is a rendered empty struct: {ccap}"
    );
    assert!(
        engaged <= refreshes,
        "{what}: [CCAP] engagement exceeds its own denominator: {ccap}"
    );
    for key in ["cap=", "mem=", "floor=", "floor_val=", "brake_frac="] {
        assert!(
            ccap.contains(key),
            "{what}: [CCAP] is missing the scrapeable field `{key}`: {ccap}"
        );
    }

    let total_ms = field(wall, "total_ms=")
        .unwrap_or_else(|| panic!("{what}: [WALL] must carry total_ms= — {wall}"));
    let it_ms = field(wall, "it_ms=")
        .unwrap_or_else(|| panic!("{what}: [WALL] must carry it_ms= — {wall}"));
    let onset = field(wall, "onset=")
        .unwrap_or_else(|| panic!("{what}: [WALL] must carry onset= — {wall}"));
    assert!(
        total_ms > 0.0,
        "{what}: [WALL] reports a {total_ms} ms run — the gauge was never fed: {wall}"
    );
    assert!(
        it_ms > 0.0 && it_ms < 1000.0,
        "{what}: [WALL] reports a sender-loop period of {it_ms} ms, which is not \
         a loop: {wall}"
    );
    assert!(
        (0.0..=1.0).contains(&onset),
        "{what}: [WALL] onset must be a fraction of the transfer wall: {wall}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 1. THE SHIPPED HARNESS, END TO END
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn the_teardown_gauges_fire_exactly_once_under_the_shipped_perf_harness() {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv) = spawn_perf_server();

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
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    cli.env_remove(CHILD_PEER);
    let out = cli.output().expect("run perf client");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let log = format!("{stdout}\n{stderr}");
    assert!(
        out.status.success(),
        "perf client failed ({:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains("\"summary\""),
        "perf client produced no summary line — the transfer did not complete:\n{stdout}"
    );

    // Which side of the teardown race this run landed on. NOT asserted: it is
    // genuinely nondeterministic (the VM smoke saw `TUN closed` on 1 run of 4),
    // and that nondeterminism is the whole defect. It is PRINTED so a reader of
    // a failing run can tell the two cases apart, and so a future change that
    // makes the harness deterministic is visible here.
    println!(
        "[gauge-reachability/shipped-harness] exit arms taken: \
         graceful={} tun_closed={}",
        count(&log, "window sender shut down gracefully"),
        count(&log, "TUN closed"),
    );

    assert_one_fed_gauge_of_each(&log, "shipped-harness");
}

// ─────────────────────────────────────────────────────────────────────────
// 2. THE DISCRIMINATOR: NEITHER TEARDOWN ARM IS TAKEN
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn the_teardown_gauges_fire_when_the_sender_task_is_dropped_at_runtime_shutdown() {
    let (addr, _srv) = spawn_perf_server();

    // Re-execute THIS test binary as the child fixture below. A child process
    // is needed because the emission is an `eprintln!` from an engine task and
    // the whole point is that it happens AFTER the runtime is dropped — i.e.
    // after any in-process test body has already returned.
    let mut child = Command::new(std::env::current_exe().expect("current_exe"));
    child
        .args([
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads",
            "1",
            "runtime_drop_child_fixture",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in ARM {
        child.env(k, v);
    }
    child.env(CHILD_PEER, addr.to_string());
    let out = child.output().expect("re-exec the runtime-drop child fixture");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let log = format!("{stdout}\n{stderr}");
    assert!(
        out.status.success(),
        "the runtime-drop child fixture failed ({:?})\n--- stdout ---\n{stdout}\n\
         --- stderr ---\n{stderr}",
        out.status
    );
    assert!(
        log.contains("CHILD-FED"),
        "the child never got a transfer going, so it proves nothing about \
         reachability:\n{log}"
    );

    // THE DEFECT, ASSERTED. The child holds its `MemTun` alive across the
    // runtime's drop, so the sender's `select!` never sees a closed TUN and
    // nothing ever signals shutdown: NEITHER of the two arms the gauges used
    // to live on is taken. On the shipped-before code this run emitted
    // nothing; the assertions below then fail on a count of 0.
    assert_eq!(
        count(&log, "window sender shut down gracefully"),
        0,
        "the child took the GRACEFUL-SHUTDOWN arm, so this run no longer \
         isolates the runtime-drop path — re-derive the fixture instead of \
         letting the discriminator go vacuous:\n{log}"
    );
    assert_eq!(
        count(&log, "TUN closed"),
        0,
        "the child took the TUN-CLOSED arm, so this run no longer isolates the \
         runtime-drop path — the fixture must hold its MemTun alive across the \
         runtime's drop:\n{log}"
    );

    assert_one_fed_gauge_of_each(&log, "runtime-drop");
}

/// THE CHILD FIXTURE for the test above. `#[ignore]`d so the ordinary suite
/// skips it; the parent runs it by name with `--ignored`, and without
/// `RWM_GAUGE_CHILD_PEER` it is a no-op even then.
///
/// It is deliberately NOT a `#[tokio::test]`: it builds the runtime by hand so
/// that it can DROP it while holding the `MemTun` — which is the whole point.
/// `block_on` returns the `MemTun` into this scope, so the engine's TUN read
/// side stays open past `drop(rt)`; the sender task therefore ends by having
/// its future dropped at runtime shutdown and by nothing else. That is exactly
/// how the sender ends under `perf` whenever it loses the teardown race, and
/// exactly how it ends under `#[tokio::main]` returning from `main`.
///
/// The payload is raw filler, not the perf object protocol: the peer will
/// discard it, and the sender loop — the only thing under test — runs
/// identically either way.
#[test]
#[ignore = "child fixture, re-executed by the runtime-drop reachability test"]
fn runtime_drop_child_fixture() {
    let Ok(peer) = std::env::var(CHILD_PEER) else {
        return;
    };
    let _ = rustls::crypto::ring::default_provider().install_default();

    // The `[GATES]` echo. The engine emits it through `tracing`, and this
    // fixture installs no subscriber, so it prints the SAME string from the
    // SAME process-global resolution — mechanism liveness for the parent's
    // "the arm was armed" check, which is what separates an unreachable
    // emission site from an unset gate.
    let g = raptorpath::gates::RuntimeGates::resolve();
    println!("{}", g.echo_line());
    assert!(g.composed_cap && g.walldiag, "the child's arm must be armed");

    let cfg = raptorpath::config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec![peer]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (pc, _) = raptorpath::config::resolve(&cfg).expect("resolve child config");
    assert!(pc.window_reliable, "the child must run the WINDOW sender");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("child runtime");

    // NOTE the binding: `mem` OUTLIVES `rt`.
    let mem = rt.block_on(async {
        let (tun, mem) = raptorpath::tun::TunInterface::memory(1500);
        let _engine = tokio::spawn(raptorpath::net::run_with_tun(pc, tun));
        // Feed long enough for the dyn-cap refresh to tick and the wall gauge
        // to accumulate a span — the reachability assertion also checks the
        // gauges carry real numbers.
        let payload = bytes::Bytes::from(vec![0xA5u8; 1100]);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut fed = 0u64;
        while tokio::time::Instant::now() < deadline {
            if mem.feed.send(payload.clone()).await.is_err() {
                break;
            }
            fed += 1;
            if fed % 256 == 0 {
                tokio::task::yield_now().await;
            }
        }
        println!("CHILD-FED {fed}");
        mem
    });

    // THE EXIT UNDER TEST: the runtime goes away and takes the sender task's
    // future with it, while the TUN is still open and nothing has signalled
    // shutdown.
    drop(rt);
    drop(mem);
}
