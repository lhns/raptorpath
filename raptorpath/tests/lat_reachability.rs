//! DELIVERED LATENCY IS DECOMPOSED — `[LAT]` — AND THE CROSS-PATH CLASS IS
//! STRUCTURALLY ZERO AT ONE PATH.
//!
//! **The measurand, and why it is owed.** Seed S7: what a user of this
//! transport experiences is delivered latency and goodput, and the record has
//! never decomposed the first one. It has measured holes, repairs, false
//! repairs, stalls and shares. The recovery-clock prize is already BOUNDED
//! and small (§16.80: < 1.5–3.4 % at the duals, ≈ 0 at singles), so the law
//! worth finding first is the one governing the LARGEST term — and nobody
//! knows which term that is.
//!
//! `[LAT]` splits every delivered source symbol's wait into `A_x` (queue +
//! sender dwell above the path's floor), `R` (the reorder wait, CLASSED by
//! the `[SUCC]` record of the hole that released it) and `P` (the repair
//! wait). This binary is the gate that must pass before the pre-registered
//! CTL readout — PLACEMENT-INDICTED / QUEUE-DOMINATED / REPAIR-DOMINATED /
//! MIXED — is worth taking.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **THE LINE FIRES** from the receiver with `n > 0`. THE DEAD-GAUGE
//!      READING this test exists to fail on: no `[LAT]` exists on the
//!      shipped-before engine, `ReorderBuffer` did not return the instant it
//!      buffered anything, and `SuccGauge::resolve` returned nothing at all,
//!      so the reorder wait had neither a value nor a class.
//!   2. **THE ACCOUNTING IDENTITY, ON THE WIRE**: per path,
//!      `n = rwxp_n + rwsp_n + rwrep_n + nowait`. Every delivery either
//!      waited in exactly one class or did not. A gauge whose classes do not
//!      partition its own denominator is caught here.
//!   3. **`rwxp_n ≡ 0` ON ONE PATH.** The cross-path class means *the arrival
//!      that released this wait landed on a different path from the one that
//!      exposed the hole*, which is STRUCTURALLY IMPOSSIBLE with one path.
//!      This is the CONTROL: a nonzero reading here would mean the class is
//!      measuring something other than what it is named for, and every
//!      dual-cell number taken through it would be uninterpretable.
//!   4. **`rwxp_n > 0` ON TWO PATHS** (`RWM_L0_NETEM=c2,c3`, the c8 shape) —
//!      the scheduler's own inversions, which D0 measured at 92–96 % of
//!      dual-cell holes. The pair (3) + (4) is what makes this an instrument
//!      rather than a counter.
//!   5. **THE SHARES ARE A PARTITION**: `sh_ax + sh_rwxp + sh_rwsp +
//!      sh_rwrep + sh_rep = 1` to rounding, so the pre-registered readout can
//!      be taken off one line.
//!   6. **THE QUANTILES ARE ORDERED** (`p50 ≤ p90 ≤ p95 ≤ p99`) and `-` iff
//!      the class has no sample — never 0, which a parser would read as a
//!      measured zero microseconds.
//!
//! **What this deliberately does NOT assert.** Any VALUE of any share or
//! quantile, and in particular NOT which term dominates. Loopback's queueing
//! is the host scheduler's and its loss is the shim's Gilbert-Elliott
//! process; the readout that routes Track A comes off an L1 run scored
//! against a pre-registration. This is the INSTRUMENT gate.
//!
//! **No new gate.** The readout rides the EXISTING `RWM_DIAG` surface beside
//! `[SUCC]` and `[ETA]`, on their cadence, so a missing line can only be read
//! as an unreached emission site.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

fn join(a: &[SocketAddr]) -> String {
    a.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
}

fn spawn_perf_server(binds: &[SocketAddr]) -> (Reaper, Arc<Mutex<String>>) {
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
    for (k, v) in ARM {
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

/// One loopback transfer. Returns the SERVER (receiver) log.
fn run(paths: usize, netem: Option<&str>, bytes: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let binds: Vec<SocketAddr> = (0..paths)
        .map(|_| format!("127.0.0.1:{}", free_port()).parse().unwrap())
        .collect();
    let (_srv, srv_log) = spawn_perf_server(&binds);

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
    for (k, v) in ARM {
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
        "perf client failed (paths={paths}, netem={netem:?}, {:?})\n\
         --- stdout ---\n{cli_stdout}\n--- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let out = srv_log.lock().expect("stderr sink").clone();
    out
}

// ── READERS ─────────────────────────────────────────────────────────────

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn f64_field(line: &str, key: &str) -> f64 {
    let v = field(line, key);
    v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

/// `-`-or-number. `None` is the ABSENT reading and is never 0.
fn opt_field(line: &str, key: &str) -> Option<u64> {
    let v = field(line, key);
    (v != "-").then(|| v.parse().expect("numeric slot"))
}

/// The per-path slots, each rendered so `field()` can read it.
fn slots(line: &str) -> Vec<String> {
    line.split_whitespace().fold(Vec::new(), |mut acc: Vec<String>, t| {
        let head = t.split_once(':').is_some_and(|(h, _)| {
            h.len() > 1 && h.starts_with('p') && h[1..].chars().all(|c| c.is_ascii_digit())
        });
        if head {
            acc.push(t.replacen(':', " ", 1));
        } else if let Some(last) = acc.last_mut() {
            last.push(' ');
            last.push_str(t);
        }
        acc
    })
}

fn last_lat(log: &str) -> &str {
    log.lines()
        .rev()
        .find(|l| l.contains("[LAT] site=receiver"))
        .unwrap_or_else(|| {
            panic!(
                "no [LAT] line from the RECEIVER — delivered latency has no \
                 decomposition, which is the DEAD-GAUGE reading this test \
                 exists to fail on:\n{log}"
            )
        })
}

/// 1, 2, 5, 6 on any topology.
fn assert_lat_line(l: &str) -> Vec<String> {
    assert!(u64_field(l, "n=") > 0, "[LAT] n=0 — nothing was ever delivered: {l}");
    let sl = slots(l);
    assert!(!sl.is_empty(), "[LAT] fired with no path slot: {l}");
    for s in &sl {
        // 2. THE ACCOUNTING IDENTITY.
        let n = u64_field(s, "n=");
        let parts = ["rwxp_n=", "rwsp_n=", "rwrep_n="]
            .iter()
            .map(|k| u64_field(s, k))
            .sum::<u64>()
            + u64_field(s, "nowait=");
        assert_eq!(
            n, parts,
            "[LAT] n is not the sum of its wait classes and its no-wait count \
             — the classes do not partition the deliveries: {s}"
        );
        // 5. THE SHARES ARE A PARTITION.
        let sh: f64 = ["sh_ax=", "sh_rwxp=", "sh_rwsp=", "sh_rwrep=", "sh_rep="]
            .iter()
            .map(|k| f64_field(s, k))
            .sum();
        assert!(
            (sh - 1.0).abs() < 1e-3,
            "[LAT] shares sum to {sh}, not 1 — they are not a partition of the \
             accumulated wait: {s}"
        );
        // 6. ORDERED QUANTILES, `-` IFF NO SAMPLE.
        let q: Vec<Option<u64>> = ["ax_p50=", "ax_p90=", "ax_p95=", "ax_p99="]
            .iter()
            .map(|k| opt_field(s, k))
            .collect();
        for w in q.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                assert!(a <= b, "A_x quantiles out of order in {s}");
            }
        }
        for name in ["rwxp", "rwsp", "rwrep", "rep"] {
            let cn = u64_field(s, &format!("{name}_n="));
            for p in ["p50", "p95"] {
                let v = opt_field(s, &format!("{name}_{p}="));
                assert_eq!(
                    v.is_none(),
                    cn == 0,
                    "`{name}_{p}` must render `-` IFF `{name}_n = 0` — an absent \
                     reading is never a measured zero: {s}"
                );
            }
            if cn == 0 {
                assert_eq!(u64_field(s, &format!("{name}_sum=")), 0);
            }
        }
    }
    sl
}

// ── THE TWO TOPOLOGIES ──────────────────────────────────────────────────

/// 1, 2, 3, 5, 6: THE GAUGE FIRES AND THE CROSS-PATH CLASS IS ZERO ON ONE
/// PATH. The control — a nonzero reading here voids every dual-cell number.
#[test]
fn the_decomposition_fires_and_cross_path_is_structurally_zero_on_one_path() {
    let log = run(1, None, "8000000");
    assert!(log.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{log}");
    let l = last_lat(&log);
    println!("[lat-reach] N=1: {l}");
    let sl = assert_lat_line(l);
    assert_eq!(sl.len(), 1, "a single-path run must report exactly one path slot: {l}");
    assert_eq!(
        u64_field(&sl[0], "rwxp_n="),
        0,
        "[LAT] rwxp_n > 0 at ONE PATH — a cross-path reorder wait is \
         STRUCTURALLY IMPOSSIBLE there, so the class is measuring something \
         other than what it is named for and every dual-cell reading taken \
         through it is void:\n{l}"
    );
    assert_eq!(u64_field(&sl[0], "rwxp_sum="), 0, "{l}");
}

/// 4: THE CROSS-PATH CLASS IS POPULATED ON TWO PATHS — the scheduler's own
/// inversions, which the whole placement track is about.
#[test]
fn the_cross_path_wait_is_populated_on_two_paths() {
    let log = run(2, Some("c2,c3"), "24000000");
    let l = last_lat(&log);
    println!("[lat-reach] N=2 c2,c3: {l}");
    let sl = assert_lat_line(l);
    assert!(sl.len() >= 2, "the dual topology must report both paths: {l}");
    let xp: u64 = sl.iter().map(|s| u64_field(s, "rwxp_n=")).sum();
    let sp: u64 = sl.iter().map(|s| u64_field(s, "rwsp_n=")).sum();
    let rep: u64 = sl.iter().map(|s| u64_field(s, "rwrep_n=")).sum();
    println!("[lat-reach] rwxp_n={xp} rwsp_n={sp} rwrep_n={rep}");
    assert!(
        xp > 0,
        "[LAT] rwxp_n = 0 on the dual c2,c3 topology — the cross-path class is \
         unreachable, so the reorder wait cannot be attributed to the \
         scheduler at all:\n{l}"
    );
}
