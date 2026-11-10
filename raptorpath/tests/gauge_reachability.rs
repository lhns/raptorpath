//! The teardown gauges `[CCAP]` (`RWM_LATE_BRAKE`) and `[WALL]`
//! (`RWM_WALLDIAG`) fire exactly once, fed with real numbers, under a
//! `perf`-shaped exit. `perf::client` returns without signalling shutdown,
//! and the memory TUN closes only when its handle drops, racing process
//! teardown; so the gauges are emitted by the destructor of
//! `net::SenderTeardownGauges`, which runs once on every exit of
//! `run_window_sender`. The first test runs the shipped `perf` subcommand end
//! to end and prints which side of the race it took; the second removes the
//! race: a child process holds the `MemTun` alive across the runtime's drop,
//! so the sender ends only by having its future dropped. The graceful
//! (Ctrl+C) and real-TUN-closed arms are not driven here; they are exits of
//! the same scope, so the destructor covers them by construction.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use std::process::{Command, Stdio};
use std::time::Duration;

/// Env var by which this binary re-executes itself as the deterministic
/// runtime-drop child (see the second test).
const CHILD_PEER: &str = "RWM_GAUGE_CHILD_PEER";

/// The late-brake arm as the battery configures it, with both gauges' gates on.
const ARM: [(&str, &str); 3] = [
    ("RWM_LATE_BRAKE", "1"),
    ("RWM_WALLDIAG", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// Spawn the shipped binary as a `perf` server on a fresh port (ready and
/// bound). The re-exec marker is never inherited: it is only ever set on the
/// child-fixture process, which spawns no server — asserted, not assumed.
fn spawn_perf_server() -> loopback::PerfServer {
    assert!(
        std::env::var_os(CHILD_PEER).is_none(),
        "the re-exec marker must not reach a spawned perf endpoint"
    );
    loopback::spawn_perf_server(
        &[loopback::free_addr()],
        &ARM,
        &["--protocol-hint", "bulk", "--window-reliable"],
    )
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

/// Pull `key=<f64>` out of a gauge line (numeric prefix applied).
fn field(line: &str, key: &str) -> Option<f64> {
    gauge::raw_field(line, key).and_then(|v| gauge::numeric_prefix(v).parse::<f64>().ok())
}

/// The shared reachability assertion: in a log produced by one window
/// sender, `[CCAP]` and `[WALL]` each appear exactly once (the battery
/// parsers read a per-run scalar, so two lines is as wrong as none) and each
/// carries a real measurement rather than an empty struct.
fn assert_one_fed_gauge_of_each(log: &str, what: &str) {
    // Mechanism liveness first: a missing line must be readable as an
    // unreachable emission site and never as an unset gate.
    assert!(
        log.contains("RWM_LATE_BRAKE=1") && !log.contains("RWM_LATE_BRAKE=0"),
        "{what}: the [GATES] echo does not carry RWM_LATE_BRAKE=1 two-sidedly:\n{log}"
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

    let brake = ccap
        .split_whitespace()
        .find_map(|t| t.strip_prefix("brake="))
        .unwrap_or_else(|| panic!("{what}: [CCAP] must carry brake=<closed>/<ticks> - {ccap}"));
    let (closed, ticks) = brake.split_once('/').expect("brake=<a>/<b>");
    let ticks: u64 = ticks.parse().expect("brake= denominator");
    let closed: u64 = closed.parse().expect("brake= numerator");
    assert!(
        ticks > 0,
        "{what}: [CCAP] reports {ticks} brake ticks - the tally was never fed, \
         so the line is a rendered empty struct: {ccap}"
    );
    assert!(closed <= ticks, "{what}: [CCAP] closed exceeds its own denominator: {ccap}");
    assert!(
        ccap.contains("brake_frac="),
        "{what}: [CCAP] is missing the scrapeable field `brake_frac=`: {ccap}"
    );

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
// 1. The shipped harness, end to end
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn the_teardown_gauges_fire_exactly_once_under_the_shipped_perf_harness() {
    let srv = spawn_perf_server();
    let out = loopback::perf_client_output(
        &srv.addrs,
        &ARM,
        &loopback::perf_args("bulk", "4000000", "2"),
    );
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

    // Which side of the teardown race this run landed on. Not asserted — it
    // is nondeterministic, which is the defect — but printed so a failing run
    // can be read, and a future deterministic harness is visible here.
    println!(
        "[gauge-reachability/shipped-harness] exit arms taken: \
         graceful={} tun_closed={}",
        count(&log, "window sender shut down gracefully"),
        count(&log, "TUN closed"),
    );

    assert_one_fed_gauge_of_each(&log, "shipped-harness");
}

// ─────────────────────────────────────────────────────────────────────────
// 2. The discriminator: neither teardown arm is taken
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn the_teardown_gauges_fire_when_the_sender_task_is_dropped_at_runtime_shutdown() {
    let srv = spawn_perf_server();

    // Re-execute this test binary as the child fixture below. A child process
    // is needed because the emission is an `eprintln!` from an engine task
    // and must happen after the runtime is dropped, i.e. after any in-process
    // test body has returned.
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
    child.env(CHILD_PEER, srv.addr().to_string());
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

    // The child holds its `MemTun` alive across the runtime's drop, so the
    // sender's `select!` never sees a closed TUN and nothing signals shutdown:
    // neither teardown arm is taken, and only the destructor can emit.
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

/// The child fixture for the test above. `#[ignore]`d so the ordinary suite
/// skips it; the parent runs it by name with `--ignored`, and without
/// `RWM_GAUGE_CHILD_PEER` it is a no-op.
///
/// Not a `#[tokio::test]`: it builds the runtime by hand so it can drop it
/// while holding the `MemTun`. `block_on` returns the `MemTun` into this
/// scope, so the engine's TUN read side stays open past `drop(rt)` and the
/// sender task ends only by having its future dropped — how it ends under
/// `perf` when it loses the teardown race, and under `#[tokio::main]`
/// returning from `main`. The payload is raw filler, not the perf object
/// protocol; the sender loop runs identically either way.
#[test]
#[ignore = "child fixture, re-executed by the runtime-drop reachability test"]
fn runtime_drop_child_fixture() {
    let Ok(peer) = std::env::var(CHILD_PEER) else {
        return;
    };
    let _ = rustls::crypto::ring::default_provider().install_default();

    // The `[GATES]` echo: the engine emits it through `tracing` and this
    // fixture installs no subscriber, so it prints the same string from the
    // same resolution, giving the parent its mechanism-liveness check.
    let g = raptorpath::gates::RuntimeGates::resolve();
    println!("{}", g.echo_line());
    assert!(g.late_brake && g.walldiag, "the child's arm must be armed");

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

    // `mem` outlives `rt`.
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

    // The exit under test: the runtime drops the sender task's future while
    // the TUN is still open and nothing has signalled shutdown.
    drop(rt);
    drop(mem);
}
