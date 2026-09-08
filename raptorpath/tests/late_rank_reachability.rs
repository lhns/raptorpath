//! THE RECEIVER'S OWN SEAT IS INSTRUMENTED — `[LATE]`, `[RANK]`, AND THE TWO
//! NEW `[RFA]` COUNTERS.
//!
//! **What is being made measurable, and why.** §16.83 puts the repair
//! decision at the RECEIVER, where the information is: the frontier, the
//! lateness distribution and the rank all live there, and every clock in the
//! record lives at the sender. Three quantities that decision needs have
//! never had a producer:
//!
//!   * **the hole's LATENESS**, which the receiver cannot observe exactly and
//!     must BRACKET (`[LATE] lo_*` / `hi_*`);
//!   * **the RANK DEFICIT** `holes − pivots` over the frontier span, which
//!     `frontier_probe` has always been able to compute and which NOTHING has
//!     ever read outside `RWM_FDIAG`'s block (`[RANK]`);
//!   * **the false measurand under CODED answers** — `repairs_fed −
//!     repairs_useful`, because "the original arrived anyway" is
//!     inexpressible once the answer is an equation (`[RFA] rep_redundant`),
//!     and the EVICT seat's own waste, a copy that landed after the frontier
//!     gave up (`[RFA] late_after_aban`).
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **THE FIELDS EXIST AND THE LINES FIRE.** THE DEAD-GAUGE READING this
//!      binary exists to fail on: none of these lines or fields exists on the
//!      shipped-before engine.
//!   2. **`[LATE] n > 0` OVER A LOSSY TRANSFER**, with `det > 0` on the
//!      independent `[SUCC]` witness — two gauges, different code, same
//!      holes.
//!   3. **THE IDENTITIES CLOSE**: `[LATE] n = orig + rep + aban` and
//!      `n = xp_n + sp_n`, on the engine's own output. A gauge whose classes
//!      do not partition its own denominator is caught here.
//!   4. **`xp_frac ≡ 0` ON ONE PATH, `> 0` ON TWO.** Cross-path is
//!      STRUCTURALLY impossible at a single path; the pair is what makes the
//!      class an instrument rather than a counter.
//!   5. **`knee_us` IS NON-NULL ON TWO PATHS.** `H` is observed as the
//!      arrival-stall onset during a frontier freeze, and the dual topology
//!      is where the frontier actually freezes. An absent `H` would make
//!      `ℓ*_recv`'s cap unmeasurable, which is the whole KNEE-BOUND question.
//!   6. **`[RANK]` REPORTS A DEFICIT** and its identity `deficit =
//!      holes − pivots` holds, with the tail over-count reported beside it
//!      rather than subtracted from it.
//!   7. **`[RFA]` CARRIES BOTH NEW COUNTERS BY NAME.** `late_after_aban`'s
//!      NAME is part of the line's contract — Track B's `tail_matrix.sh`
//!      scrape greps for it — and under the RELIABLE window it must read 0,
//!      because the reorder buffer never delivers past a hole. A nonzero
//!      reading there is a finding about the engine, and this is where it
//!      would be caught.
//!
//! **What this deliberately does NOT assert.** Any VALUE of `ℓ*_recv`, of the
//! knee, or of either bind fraction — and in particular NOT whether the knee
//! binds. That is the KNEE-BOUND ledger verdict, and it is read off an L1 run
//! against a pre-registration. This is the INSTRUMENT gate.
//!
//! **No new gate.** All three readouts ride the EXISTING `RWM_DIAG` surface
//! beside `[SUCC]`, so a missing line can only be read as an unreached
//! emission site.

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
    let log = srv_log.lock().expect("stderr sink").clone();
    log
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

/// A `-`-or-number slot. `None` is the ABSENT reading and is never 0.
fn opt_field(line: &str, key: &str) -> Option<u64> {
    let v = field(line, key);
    (v != "-").then(|| v.parse().expect("numeric slot"))
}

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    log.lines().rev().find(|l| l.contains(pat)).unwrap_or_else(|| {
        panic!(
            "no `{pat}` line from the RECEIVER — the gauge is unreachable, which \
             is the DEAD-GAUGE reading this test exists to fail on:\n{log}"
        )
    })
}

/// 1, 3, 6, 7 — everything that must hold on ANY topology.
fn assert_common(log: &str) -> (String, String, String) {
    let late = last_with(log, "[LATE] ").to_string();
    let rank = last_with(log, "[RANK] ").to_string();
    let rfa = last_with(log, "[RFA] ").to_string();

    // 3. THE `[LATE]` IDENTITIES, ON THE ENGINE'S OWN OUTPUT.
    let n = u64_field(&late, "n=");
    assert_eq!(
        n,
        u64_field(&late, "orig=") + u64_field(&late, "rep=") + u64_field(&late, "aban="),
        "[LATE] n is not the sum of its three outcome classes: {late}"
    );
    assert_eq!(
        n,
        u64_field(&late, "xp_n=") + u64_field(&late, "sp_n="),
        "[LATE] n is not the sum of the same/cross split: {late}"
    );
    // Both ends of the bracket exist, and the upper never sits below the
    // lower — a bracket that inverts is not a bracket.
    for p in ["p50", "p90", "p99"] {
        if let (Some(lo), Some(hi)) =
            (opt_field(&late, &format!("lo_{p}=")), opt_field(&late, &format!("hi_{p}=")))
        {
            assert!(hi >= lo, "[LATE] hi_{p} < lo_{p} — the bracket inverted: {late}");
        }
    }
    // The declared cost ratio is ON the line, so a different one is a rescale
    // of a printed number rather than a hidden constant.
    assert!(late.contains("w=1.00"), "[LATE] must print its declared ratio: {late}");
    // Both bind fractions are present and are fractions.
    for k in ["knee_bind=", "sampler_bind="] {
        let v = field(&late, k);
        assert!(
            v == "-" || (0.0..=1.0).contains(&v.parse::<f64>().expect("fraction")),
            "`{k}` must be a fraction or `-`: {late}"
        );
    }

    // 6. `[RANK]`'s identity.
    assert_eq!(
        u64_field(&rank, "deficit="),
        u64_field(&rank, "holes=").saturating_sub(u64_field(&rank, "pivots=")),
        "[RANK] deficit is not holes − pivots: {rank}"
    );
    assert!(u64_field(&rank, "reports=") > 0, "[RANK] never took a reading: {rank}");
    assert!(rank.contains("tail_overcount="), "[RANK] must report its tail correction: {rank}");

    // 7. BOTH NEW `[RFA]` COUNTERS, BY NAME.
    for k in ["rep_redundant=", "late_after_aban="] {
        assert!(
            rfa.contains(k),
            "`{k}` missing from the [RFA] line — its NAME is part of the \
             contract the Track B scrape greps for: {rfa}"
        );
    }
    // Under the RELIABLE window the reorder buffer never delivers past a
    // hole, so a copy can never land below the frontier.
    assert_eq!(
        u64_field(&rfa, "late_after_aban="),
        0,
        "[RFA] late_after_aban > 0 under the RELIABLE window — the reorder \
         buffer delivered past a hole, which it is built not to do. A FINDING \
         about the engine, caught here: {rfa}"
    );
    (late, rank, rfa)
}

// ── THE TWO TOPOLOGIES ──────────────────────────────────────────────────

/// 1-4, 6, 7 at ONE PATH: everything fires, and `xp_frac ≡ 0`.
#[test]
fn the_receiver_seat_gauges_fire_and_cross_path_is_zero_on_one_path() {
    let log = run(1, Some("c3"), "12000000");
    assert!(log.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{log}");
    let (late, rank, rfa) = assert_common(&log);
    println!("[late-reach] N=1 {late}");
    println!("[late-reach] N=1 {rank}");

    // 2. THE GAUGE FIRES, AND AN INDEPENDENT WITNESS AGREES THERE WERE HOLES.
    let n = u64_field(&late, "n=");
    assert!(
        n > 0,
        "[LATE] n=0 over a c3-lossy transfer — no hole was ever bracketed:\n{late}"
    );
    let succ = last_with(&log, "[SUCC] ");
    let det = u64_field(succ, "det=");
    println!("[late-reach] witness [SUCC] det={det} against [LATE] n={n}");
    assert!(
        det > 0,
        "[LATE] n={n} while the independent [SUCC] witness reports det=0 — the \
         two gauges disagree about whether this transfer had holes:\n{succ}"
    );

    // 4. THE CONTROL. Cross-path is structurally impossible at one path.
    assert_eq!(
        u64_field(&late, "xp_n="),
        0,
        "[LATE] xp_n > 0 at ONE PATH — a cross-path resolution is structurally \
         impossible there, so the class is measuring something other than what \
         it is named for:\n{late}"
    );
    assert_eq!(field(&late, "xp_frac="), "0.0000", "{late}");
    let _ = rfa;
}

/// 4, 5 at TWO PATHS: the cross-path class is populated and the knee is
/// observable.
#[test]
fn cross_path_and_the_knee_are_populated_on_two_paths() {
    let log = run(2, Some("c2,c3"), "24000000");
    let (late, rank, _rfa) = assert_common(&log);
    println!("[late-reach] N=2 {late}");
    println!("[late-reach] N=2 {rank}");

    assert!(u64_field(&late, "n=") > 0, "[LATE] n=0 on the dual topology: {late}");
    assert!(
        u64_field(&late, "xp_n=") > 0,
        "[LATE] xp_n = 0 on the dual c2,c3 topology — the cross-path class is \
         unreachable, so no hole can ever be attributed to the scheduler:\n{late}"
    );
    let xf: f64 = field(&late, "xp_frac=").parse().expect("fraction");
    assert!(xf > 0.0 && xf <= 1.0, "xp_frac out of range: {late}");

    // 5. THE KNEE IS OBSERVABLE — `H` is what caps `ℓ*_recv`, so an absent
    //    one makes the whole KNEE-BOUND question unmeasurable.
    let knee = opt_field(&late, "knee_us=");
    println!(
        "[late-reach] knee_us={knee:?} knee_n={} d_us={:?} lstar_us={:?} \
         knee_bind={} sampler_bind={}",
        u64_field(&late, "knee_n="),
        opt_field(&late, "d_us="),
        opt_field(&late, "lstar_us="),
        field(&late, "knee_bind="),
        field(&late, "sampler_bind="),
    );
    assert!(
        knee.is_some(),
        "[LATE] knee_us=- on the dual topology — the arrival-stall onset was \
         never observed, so `H` has no producer and `ℓ*_recv`'s cap cannot be \
         evaluated at all:\n{late}"
    );
    assert_eq!(
        knee.is_none(),
        u64_field(&late, "knee_n=") == 0,
        "`knee_us` must render `-` IFF `knee_n = 0`: {late}"
    );
    // `ℓ*_recv` exists once either term does — and is `-`, never 0, when
    // neither does (0 would read "request immediately", the shipped corner,
    // and be indistinguishable from a genuine π₀ → 0 finding).
    assert!(
        opt_field(&late, "lstar_us=").is_some(),
        "[LATE] lstar_us=- with a knee present — the hypothetical threshold is \
         not being computed: {late}"
    );
}
