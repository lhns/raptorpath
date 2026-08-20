//! THE THREE CANDIDATE DISPERSION GAUGES ARE REPORTED — the `[DIAG]` line's
//! `rvar_us=` / `qsp_us=` / `msd_us=` fields, beside the shipped `sig_us=`.
//!
//! **The defect this repairs.** Goal #100 closed NEEDS-MORE with exactly one
//! instrument named: `rtt_sigma_us()`'s own rep-to-rep dispersion (287× at
//! `c8` at converged `n` ≈ 18 000) exceeds the dynamic range of the `k(α)` it
//! multiplies (18.24 over the swept α range), so **50 of 50 α-sweep treatment
//! pairs realized overlapping clocks** — the law worked and the estimator
//! consumed the contrast. Paper §16.74.5 turned that into a REQUIREMENT OF THE
//! MODEL and named windowed quantile dispersion and RACK-style `rttvar` as
//! obvious candidates, while deliberately preferring neither.
//!
//! A successor cannot be chosen by argument, and it cannot be chosen by
//! comparing numbers taken in different sessions on different binaries — which
//! is how a 287× spread survived two sessions unnoticed. So all three
//! candidates are built as READ-ONLY GAUGES that run SIMULTANEOUSLY, on the
//! same RTT sample stream, in the same run, on the same `[DIAG]` line, beside
//! the shipped estimator they are competing with. **That layout is the whole
//! design**: every comparison is paired per path per interval.
//!
//! **They are a DECOMPOSITION and not three guesses.** The shipped `sig_us`
//! carries three suspect properties at once and the measured spread cannot say
//! which produced it. Each candidate moves exactly ONE axis:
//!
//! ```text
//!   axis              sig_us (shipped)   rvar      qsp       msd
//!   ----------------  -----------------  --------  --------  --------
//!   memory            7 samples          7         L = 256   L = 256
//!   deviation enters  SQUARED            linear    rank      rank
//!   reference         lagging srtt       lagging   none      none
//! ```
//!
//! `rvar` vs `sig_us` isolates the SQUARE; `qsp` vs `rvar` isolates the
//! MEMORY; `msd` vs `qsp` isolates the REFERENCE.
//!
//! **Why a spawned binary and not a unit test.** `[DIAG]` is an `eprintln!`
//! from inside the sender loop on a surface gated by `RWM_DIAG`. A unit test
//! can pin the accessors; only a run of the shipped binary shows that the
//! fields exist in a log an L1 parser will scrape. MEASUREMENT DISCIPLINE
//! rule 1 — prove the mechanism under test executes — and the same lesson
//! `sigma_diag_reachability.rs` records one layer down.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires, with per-path blocks.
//!   3. **Every per-path block carries all three candidate fields** — the
//!      EXISTENCE clause, which is what fails on the pre-change engine, where
//!      none of the three tokens occurs anywhere in the tree.
//!   4. **`sig_us=` is STILL THERE, on every block, exactly once.** The
//!      candidates are added BESIDE the shipped gauge and it is unchanged;
//!      a change that replaced it would pass clause 3 and must not pass.
//!   5. **The `-`-before-first-sample convention holds as a BICONDITIONAL**:
//!      a field reads `-` if and only if its own sample count is 0. The
//!      shipped `sig_us` cannot satisfy this (it also renders `-` for a
//!      dispersion of exactly zero, which a parser cannot tell from "no
//!      sample"); the candidates are built so that it can be asserted, and it
//!      is asserted on every reading of every block.
//!   6. **All three are FED and become positive after their own warm-up**, and
//!      the window-class pair reaches a FULL window (`n` = 256) — the
//!      pre-registered window-class `n_warm`. A gauge that stayed at `-`, or
//!      stayed at a partly-full window over a multi-megabyte transfer, is an
//!      unfed gauge and that is the failure this asserts against.
//!   7. **SCALE.** Each is a dispersion of an RTT and cannot plausibly exceed
//!      a second on loopback — the µs/s unit error, caught at the instrument
//!      rather than in a results table.
//!
//! **What this binary deliberately does NOT assert, and it is the important
//! half.** Any ORDERING between the four gauges, and any VALUE. Loopback's
//! dispersion is the host scheduler's, not a network's; §16.74.5 requirement 3
//! binds — *"an estimator qualified at one seat is not qualified at the
//! other"* — and loopback is neither of the two seats the primitives were
//! measured at. **The acceptance bar (goal-gate "THE SIGMA ESTIMATOR — THE
//! ACCEPTANCE BAR") is scored by a VM battery and by nothing here.** This
//! binary prints a characterization block for the record and asserts nothing
//! about its contents beyond reachability, feeding and scale.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The arm: the DIAG surface on, window-reliable, honest anchors — the same
/// composition every L1 battery arm runs. No gate here changes a law, and the
/// candidate gauges have no gate of their own to set.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// `SIGMA_CAND_WINDOW` from `scheduler/mod.rs`, restated here because a test
/// binary cannot see a private constant. If the engine's `L` moves, this
/// assertion fails loudly rather than silently weakening.
const WINDOW: u64 = 256;

/// Every dispersion field on the per-path block, shipped first.
const FIELDS: [&str; 4] = ["sig_us=", "rvar_us=", "qsp_us=", "msd_us="];

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

/// Parse one `<name>=<µs|->/n<count>` token into (value µs, n). `None` is the
/// `-` reading — a legitimate value and not a parse failure.
fn parse_gauge(field: &str, tok: &str) -> (Option<u64>, u64) {
    let v = tok
        .strip_prefix(field)
        .expect("caller filters on the prefix");
    let (val, n) = v
        .split_once("/n")
        .unwrap_or_else(|| panic!("{field} must render as `<µs|->/n<count>`, got `{tok}`"));
    let n: u64 = n
        .parse()
        .unwrap_or_else(|e| panic!("{field} sample count `{n}` does not parse: {e}"));
    if val == "-" {
        return (None, n);
    }
    let val: u64 = val
        .parse()
        .unwrap_or_else(|e| panic!("{field} value `{val}` does not parse: {e}"));
    (Some(val), n)
}

/// Nearest-rank quantile, the tree's own convention
/// (`net::QuantileClockGauge::quantile`, `Path::cand_quantile`).
fn quantile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[test]
fn the_diag_line_reports_all_three_candidate_dispersion_gauges_beside_the_shipped_one() {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv) = spawn_perf_server();

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "8000000",
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
    let out = cli.output().expect("run perf client");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let log = format!("{stdout}\n{stderr}");
    assert!(
        out.status.success(),
        "perf client failed ({:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status
    );

    // 1. THE GATE, TWO-SIDED. A missing `[DIAG]` must be readable as an
    //    unreached emission site and never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the [GATES] echo does not carry RWM_DIAG=1 — the arm did not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );

    // 2. THE LINE FIRES, with per-path blocks.
    let diag: Vec<&str> = log.lines().filter(|l| l.contains("[DIAG] ")).collect();
    assert!(
        !diag.is_empty(),
        "no [DIAG] line in a run with RWM_DIAG=1 — the report is unreachable:\n{log}"
    );

    // 3 + 4 + 5. EXISTENCE on every block, for all four fields; and the
    //    `-` convention as a biconditional on every reading.
    //
    //    Readings are collected per field for the characterization block and
    //    for the feed assertions below.
    let mut readings: [Vec<(Option<u64>, u64)>; 4] = Default::default();
    let mut blocks = 0usize;
    for line in &diag {
        let toks: Vec<&str> = line.split_whitespace().collect();
        // A per-path block is identified by its OWN clock token,
        // `rtt=<app>/wrtt=<wire>/rtp<floor>ms` — the aggregate `rtt=<ms>ms`
        // matches a bare `starts_with("rtt=")` and must not be counted.
        let n_rtp = toks
            .iter()
            .filter(|t| t.starts_with("rtt=") && t.contains("/wrtt="))
            .count();
        if n_rtp == 0 {
            continue;
        }
        blocks += n_rtp;
        for (fi, field) in FIELDS.iter().enumerate() {
            let hits: Vec<&&str> = toks.iter().filter(|t| t.starts_with(field)).collect();
            // A gauge present on some paths and not others is worse than
            // absent: a parser would average over a biased subset. THIS IS THE
            // CLAUSE THAT FAILS ON THE PRE-CHANGE ENGINE for the three
            // candidates — and it is the clause that fails if a change ever
            // REPLACES `sig_us` rather than adding beside it.
            assert_eq!(
                hits.len(),
                n_rtp,
                "[DIAG] carries {n_rtp} per-path RTT blocks but {} `{field}` fields \
                 — the gauge is missing from at least one path: {line}",
                hits.len()
            );
            for t in hits {
                let (v, n) = parse_gauge(field, t);
                // 5. THE CONVENTION, BOTH WAYS. `-` iff the sample set is
                //    empty. Asserted only for the candidates: the shipped
                //    `sig_us` renders `-` for a zero dispersion too, which is
                //    the ambiguity the candidates were built not to have, and
                //    pinning the shipped gauge to a rule it does not follow
                //    would be a test asserting a change nobody made.
                if fi > 0 {
                    assert_eq!(
                        v.is_none(),
                        n == 0,
                        "`{field}` broke the `-`-iff-no-sample convention: read \
                         {v:?} at n={n} — a parser cannot tell a suppressed \
                         gauge from an unsampled path: {line}"
                    );
                }
                readings[fi].push((v, n));
            }
        }
    }
    assert!(
        blocks > 0,
        "no per-path [DIAG] block in the whole log — nothing to read a gauge off:\n{log}"
    );

    // 6. AND THEY ARE FED. Over a multi-megabyte transfer the sender takes
    //    thousands of RTT samples; a candidate that never became positive
    //    would mean its feed site is not reached, which is the defect this
    //    binary exists to catch.
    let mut best: Vec<(u64, u64)> = Vec::new();
    for (fi, field) in FIELDS.iter().enumerate() {
        let b = readings[fi]
            .iter()
            .filter_map(|(v, n)| v.map(|v| (v, *n)))
            .max_by_key(|(_, n)| *n)
            .unwrap_or_else(|| {
                panic!(
                    "every [DIAG] `{field}` read `-` over {} samples of the field \
                     — the gauge was never fed:\n{}",
                    readings[fi].len(),
                    diag.join("\n")
                )
            });
        best.push(b);
        let (v, n) = b;
        assert!(
            v > 0,
            "`{field}` read 0 µs at n={n} — an RTT series with literally zero \
             dispersion over a whole transfer is not a measurement, it is an \
             unfed gauge"
        );
        // 7. SCALE — the µs/s unit error, the most likely mistake in this
        //    change, caught here rather than in a battery's results table.
        assert!(
            v < 1_000_000,
            "`{field}` = {v} µs on loopback is not a dispersion of a loopback \
             RTT — suspect a unit error in the gauge"
        );
    }

    // 6b. THE WINDOW-CLASS PAIR REACHES A FULL WINDOW — the pre-registered
    //     window-class `n_warm` (goal-gate "THE SIGMA ESTIMATOR — THE
    //     ACCEPTANCE BAR" clause C1). A window that never fills means the
    //     gauge reports a quantile of fewer order statistics than it claims.
    let qsp_max_n = readings[2].iter().map(|(_, n)| *n).max().unwrap_or(0);
    let msd_max_n = readings[3].iter().map(|(_, n)| *n).max().unwrap_or(0);
    assert_eq!(
        qsp_max_n, WINDOW,
        "`qsp_us=` never reached a full window (best n={qsp_max_n}, L={WINDOW}) — \
         either the window never filled over a whole transfer, or the engine's \
         SIGMA_CAND_WINDOW no longer matches this test's WINDOW"
    );
    assert_eq!(
        msd_max_n,
        WINDOW - 1,
        "`msd_us=` never reached a full difference set (best n={msd_max_n}, \
         expected L−1 = {}) — the successive-difference count must be exactly \
         one below the window fill",
        WINDOW - 1
    );

    // ------------------------------------------------------------------
    // THE CHARACTERIZATION BLOCK — printed for the record, ASSERTED ON
    // NOWHERE. `R_local` is the acceptance bar's own functional (p95/p05 over
    // pooled post-warm-up readings) evaluated over this run's [DIAG] time
    // series. It is NOT `R_total`: the bar's statistic pools REPS at a shaped
    // cell, and this pools intervals of one loopback run. It is reported so
    // the local half of goal #101 item 2 has numbers rather than adjectives.
    // ------------------------------------------------------------------
    println!("\n[sigma-cand] {blocks} per-path [DIAG] blocks, loopback, bulk, window-reliable");
    println!(
        "[sigma-cand] {:<9} {:>12} {:>10} {:>10} {:>10} {:>8} {:>8}",
        "field", "best(µs)/n", "p05", "p50", "p95", "R_local", "n_kept"
    );
    for (fi, field) in FIELDS.iter().enumerate() {
        // Warm-up exclusion, exactly as pre-registered in clause C1: EWMA
        // classes at n >= 16, window classes at a FULL window.
        let n_warm: u64 = match fi {
            0 | 1 => 16,
            2 => WINDOW,
            _ => WINDOW - 1,
        };
        let mut kept: Vec<u64> = readings[fi]
            .iter()
            .filter(|(_, n)| *n >= n_warm)
            .filter_map(|(v, _)| *v)
            .collect();
        kept.sort_unstable();
        let (p05, p50, p95) = (
            quantile(&kept, 0.05),
            quantile(&kept, 0.50),
            quantile(&kept, 0.95),
        );
        let r = if p05 > 0 {
            format!("{:.2}", p95 as f64 / p05 as f64)
        } else {
            "n/a".to_string()
        };
        println!(
            "[sigma-cand] {:<9} {:>7}/n{:<4} {p05:>10} {p50:>10} {p95:>10} {r:>8} {:>8}",
            field.trim_end_matches('='),
            best[fi].0,
            best[fi].1,
            kept.len()
        );
    }
    println!(
        "[sigma-cand] warm-up exclusions applied: sig/rvar n>={}, qsp n>={WINDOW}, msd n>={}",
        16,
        WINDOW - 1
    );
}
