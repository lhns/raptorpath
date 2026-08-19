//! `[CPUPROF]` IS REACHED AND FED — the sender CPU decomposition's gate.
//!
//! **The defect class this exists to prevent.** `gauge_reachability.rs`
//! records the lesson in this tree's own words: `[CCAP]` and `[WALL]` carried
//! always-on FORMAT pins for a month while being emitted from two `select!`
//! arms the `perf` harness is not guaranteed to reach — *"every `[CCAP]` pin
//! asserted the format and none asked whether the line fired."* `cpuprof.rs`
//! has the same shape of pins, so it needs the same shape of gate, and it
//! needs it BEFORE a battery is pre-registered against the instrument rather
//! than after the battery comes back empty.
//!
//! MEASUREMENT DISCIPLINE rule 1: prove the mechanism under test executes.
//!
//! **Why a spawned binary and not a unit test.** `[CPUPROF]` is an
//! `eprintln!` from the destructor of a local of `run_window_sender`. No unit
//! test can construct that sender, and no unit test can show that the five
//! seams are on the path a real transfer takes. Only a run of the shipped
//! binary can, and the run must be a `perf --client`, because `perf::client`
//! is the exit shape that defeated the two `select!` arms.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. The gate is TWO-SIDED in the `[GATES]` echo: `RWM_CPUPROF=1` present,
//!      `RWM_CPUPROF=0` absent. A missing `[CPUPROF]` must be readable as an
//!      unreached emission site and never as an unset gate.
//!   2. The line fires AT ALL, exactly once per sender.
//!   3. Its token set is the one `net::cpuprof::report_line` renders and an
//!      L1 parser is written against — asserted by PARSING, not by substring.
//!   4. **Every one of the five seams is FED** (`n > 0`). This is the
//!      assertion with teeth: a seam wired into a code path the window sender
//!      does not take would report a clean `0.0000` share, and a results
//!      table would print it as "this cost nothing" rather than as "this was
//!      never measured". A whole-program-LTO build gives no other warning.
//!   5. The shares are ARITHMETICALLY COHERENT: each in `[0, 1]`, `attr` the
//!      sum of them, `unattr = 1 − attr`, and `attr ≤ 1`. An `attr` above 1
//!      would mean the seams are NOT disjoint — the one structural
//!      assumption the decomposition rests on — and it is caught here rather
//!      than in a battery's results table.
//!   6. **The gauge ships OFF**: the same run without the gate prints no
//!      `[CPUPROF]` at all. Two-sided, on the line as well as on the echo.
//!
//! **What this binary deliberately does NOT assert.** Any particular VALUE of
//! any seam's share. Loopback has no netem, no shaped bottleneck and no
//! network MTU; its decomposition is the host's, not a cell's, and no claim
//! about c9's 68.5 ms/MB can be made from it. The measurement that decomposes
//! the ceiling is an L1 run under `tools/l1/cpuprof_battery.sh`; this is the
//! instrument gate that must pass before that run is worth making.
//!
//! It also does not assert that `cpu_ms` is available: `CLOCK_PROCESS_CPUTIME_ID`
//! is read under `cfg(target_os = "linux")` and renders `-` elsewhere, so the
//! share clauses are asserted CONDITIONALLY on a numeric reading and the
//! `-` case is asserted to be consistently `-` across every derived field.
//! The VM is Linux; a developer host may not be, and a gate that only passes
//! on one of them is a gate nobody runs.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The arm under test: the CPU decomposition on, nothing else changed. No
/// gate here changes a law, and `RWM_DIAG` is deliberately ABSENT — the
/// instrument is independent of the `[DIAG]` surface by construction and this
/// run proves it does not need it.
const ARM: [(&str, &str); 2] = [("RWM_CPUPROF", "1"), ("RUST_LOG", "raptorpath=info")];

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

fn spawn_perf_server(env: &[(&str, &str)]) -> (SocketAddr, Reaper) {
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
    for (k, v) in env {
        cmd.env(k, v);
    }
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
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = out.read_to_end(&mut sink);
    });
    (addr, srv)
}

/// Run one `perf --client` transfer against a fresh server and return the
/// merged stdout+stderr the L1 drivers scrape.
fn run_transfer(env: &[(&str, &str)]) -> String {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv) = spawn_perf_server(env);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "8000000",
        "--runs",
        "1",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
        // Generation coding ON: the `enc` seam is the coded path, and a run
        // without it would leave the decomposition's headline column unfed
        // for a reason that is the HARNESS's and not the engine's. This is
        // the same flag `perf_rwm_c.sh` passes on every L1 battery arm.
        "--window-generation-coding",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    for (k, v) in env {
        cli.env(k, v);
    }
    let out = cli.output().expect("run perf client");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed ({:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status
    );
    format!("{stdout}\n{stderr}")
}

/// One parsed seam token: `<name>=<ms>/n<count>/<share|->`.
#[derive(Debug)]
struct SeamTok {
    name: String,
    ms: f64,
    n: u64,
    share: Option<f64>,
}

fn parse_seam(tok: &str) -> SeamTok {
    let (name, rest) = tok.split_once('=').expect("a seam token is name=value");
    let mut parts = rest.split('/');
    let ms = parts.next().expect("seam ms");
    let n = parts.next().expect("seam count");
    let share = parts.next().expect("seam share");
    assert!(
        parts.next().is_none(),
        "a seam token has exactly three fields: {tok}"
    );
    let n = n
        .strip_prefix('n')
        .unwrap_or_else(|| panic!("the seam count must render as `/n<count>`: {tok}"));
    SeamTok {
        name: name.to_string(),
        ms: ms
            .parse()
            .unwrap_or_else(|e| panic!("seam ms `{ms}` does not parse ({e}): {tok}")),
        n: n
            .parse()
            .unwrap_or_else(|e| panic!("seam count `{n}` does not parse ({e}): {tok}")),
        share: if share == "-" {
            None
        } else {
            Some(
                share
                    .parse()
                    .unwrap_or_else(|e| panic!("seam share `{share}` does not parse ({e}): {tok}")),
            )
        },
    }
}

fn parse_scalar(line: &str, key: &str) -> Option<f64> {
    let tok = line
        .split_whitespace()
        .find(|t| t.starts_with(&format!("{key}=")))
        .unwrap_or_else(|| panic!("[CPUPROF] carries no `{key}=` field: {line}"));
    let v = tok.split_once('=').unwrap().1;
    if v == "-" {
        None
    } else {
        Some(
            v.parse()
                .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse ({e})")),
        )
    }
}

#[test]
fn the_cpuprof_line_fires_and_every_seam_is_fed() {
    let log = run_transfer(&ARM);

    // 1. THE GATE, TWO-SIDED.
    assert!(
        log.contains("RWM_CPUPROF=1"),
        "the [GATES] echo does not carry RWM_CPUPROF=1 — the arm did not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_CPUPROF=0"),
        "the [GATES] echo carries BOTH sides of RWM_CPUPROF:\n{log}"
    );

    // 2. THE LINE FIRES — and exactly once per sender. THIS IS THE ASSERTION
    //    THAT FAILS IF THE DESTRUCTOR IS THE WRONG SITE, which is precisely
    //    how `[CCAP]` and `[WALL]` were emitted on 0-1 of 4 runs.
    let lines: Vec<&str> = log.lines().filter(|l| l.contains("[CPUPROF] ")).collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly ONE [CPUPROF] line from one sender, got {}:\n{}",
        lines.len(),
        log
    );
    let line = lines[0];
    println!("[cpuprof-reach] {line}");

    // 3. THE SCALAR FIELDS PARSE.
    let run_ms = parse_scalar(line, "run_ms").expect("run_ms is never `-`");
    assert!(run_ms > 0.0, "a transfer has a positive wall span: {run_ms}");
    let cpu_ms = parse_scalar(line, "cpu_ms");
    let cores = parse_scalar(line, "cores");
    assert_eq!(
        cpu_ms.is_some(),
        cores.is_some(),
        "`cores` is derived from `cpu_ms`: the two must be available together: {line}"
    );

    // 4. EVERY SEAM IS FED. The teeth of this file.
    let seams: Vec<SeamTok> = line
        .split_whitespace()
        .filter(|t| t.contains("/n"))
        .map(|t| parse_seam(t))
        .collect();
    let names: Vec<&str> = seams.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["enc", "src", "frm", "ser", "hand"],
        "the seam set and its ORDER are what an L1 parser indexes by: {line}"
    );
    for s in &seams {
        assert!(
            s.n > 0,
            "seam `{}` was never entered over a whole 8 MB transfer — it is \
             wired into a path the window sender does not take, and a results \
             table would print its 0 share as a measurement: {line}",
            s.name
        );
        assert!(
            s.ms >= 0.0,
            "seam `{}` reports negative time: {line}",
            s.name
        );
    }

    // 5. ARITHMETIC COHERENCE — and in particular `attr <= 1`, which is the
    //    DISJOINTNESS assumption the whole decomposition rests on. A nested
    //    seam added later shows up here as an attribution above 100 %.
    let attr = parse_scalar(line, "attr");
    let unattr = parse_scalar(line, "unattr");
    match cpu_ms {
        None => {
            // The `-` case must be CONSISTENT: no derived field may quietly
            // acquire a number when its denominator has none.
            assert!(attr.is_none() && unattr.is_none(), "inconsistent `-`: {line}");
            for s in &seams {
                assert!(
                    s.share.is_none(),
                    "seam `{}` has a share with no CPU denominator: {line}",
                    s.name
                );
            }
            println!(
                "[cpuprof-reach] no process-CPU clock on this platform; \
                 share clauses skipped, `-` consistency asserted"
            );
        }
        Some(cpu) => {
            assert!(cpu > 0.0, "a transfer consumes CPU: {line}");
            let attr = attr.expect("attr accompanies cpu_ms");
            let unattr = unattr.expect("unattr accompanies cpu_ms");
            let mut sum = 0.0;
            for s in &seams {
                let sh = s.share.unwrap_or_else(|| {
                    panic!("seam `{}` has no share but cpu_ms is numeric: {line}", s.name)
                });
                assert!(
                    (0.0..=1.0).contains(&sh),
                    "seam `{}` share {sh} is outside [0, 1]: {line}",
                    s.name
                );
                sum += sh;
            }
            assert!(
                (sum - attr).abs() < 5e-4,
                "`attr` ({attr}) is not the sum of the printed shares ({sum}): {line}"
            );
            assert!(
                (attr + unattr - 1.0).abs() < 5e-4,
                "`unattr` must be 1 - `attr`: {line}"
            );
            assert!(
                attr <= 1.0 + 5e-4,
                "the seams attribute {attr} of process CPU — above 1.0 means they \
                 are NOT DISJOINT, which is the one structural assumption the \
                 decomposition rests on: {line}"
            );
            println!("[cpuprof-reach] attr={attr:.4} unattr={unattr:.4} cores={cores:?}");
        }
    }
}

/// **THE OFF SIDE.** The same transfer without the gate prints no
/// `[CPUPROF]` at all. Asserted on the LINE and not only on the echo,
/// because the whole claim that this instrument is free on every shipped arm
/// rests on the gauge not existing.
#[test]
fn the_gauge_is_silent_on_the_shipped_default() {
    let log = run_transfer(&[("RUST_LOG", "raptorpath=info")]);
    assert!(
        log.contains("RWM_CPUPROF=0"),
        "the [GATES] echo must NAME the gate with its 0 value on the default arm:\n{log}"
    );
    assert!(
        !log.contains("[CPUPROF]"),
        "the CPU-decomposition gauge ships OFF and must print nothing:\n{log}"
    );
}
