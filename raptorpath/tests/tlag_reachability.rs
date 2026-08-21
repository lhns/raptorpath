//! THE RATE-INVARIANT DISPERSION GAUGE IS REPORTED — the `[DIAG]` line's
//! `tlag_us=` field, beside the four gauges the scored battery measured.
//!
//! **The defect this repairs.** The scored VM battery (goal-gate, "THE SIGMA
//! ESTIMATOR — THE SCORED RESULT") closed goal #101 item 2 `NEEDS-MORE` with
//! all four estimators failing clause `S`, and named the nearest miss and its
//! cause together: `msd_us` reaches `R_total = 8.667` on the bar's own most
//! generous domain against an accept bar of `6.0`, **and every one of its
//! failures is a sparse leg** — `rho = −0.548` between `R_total` and sample
//! rate across the eight sender legs, with the two thinnest legs (581 and
//! 1 762 samples/s) the two worst readings in the whole battery. The closing
//! line names the successor in as many words: *"a fixed-TIME lag rather than a
//! fixed-SAMPLE lag is the obvious candidate and is not built here."*
//!
//! Paper §16.75 builds it, formula first:
//!
//! ```text
//!     σ̂_Δ(τ) = median { |rtt(tᵢ) − rtt(tⱼ)| : (i, j) ∈ P(τ) }
//!
//!     P(τ) = { (i, j(i)) : j(i) = the most recent sample with tᵢ − tⱼ ≥ τ,
//!                          admitted iff tᵢ − t_{j(i)} ≤ c·τ }
//!
//!     τ = RTprop (MEASURED),   c = 2
//! ```
//!
//! **Why a spawned binary and not a unit test.** `[DIAG]` is an `eprintln!`
//! from inside the sender loop on a surface gated by `RWM_DIAG`. A unit test
//! can pin the accessor; only a run of the shipped binary shows the field
//! exists in a log an L1 parser will scrape. MEASUREMENT DISCIPLINE rule 1 —
//! prove the mechanism under test executes.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires, with per-path blocks.
//!   3. **Every per-path block carries `tlag_us=`** — the EXISTENCE clause,
//!      which is what fails on the pre-change engine, where the token occurs
//!      nowhere in the tree.
//!   4. **All FOUR older gauges are STILL THERE**, on every block, exactly
//!      once each. The successor is added BESIDE its controls, and the re-run
//!      battery scores it against them from one sample stream in one run; a
//!      change that replaced any of them would pass clause 3 and must not pass.
//!   5. **The `-`-iff-`n == 0` convention as a BICONDITIONAL**, on every
//!      reading of every block. It holds by construction — value and count come
//!      from the one `tlag_diffs()` pair set — and this asserts that.
//!   6. **THE DECIMATION ACTUALLY EXECUTED**, and this is the clause that makes
//!      the test a routing gate rather than a spelling check. This runs on
//!      loopback, where the sender takes RTT samples at tens of kHz. **Without
//!      the `τ/m` admission spacing, a 256-entry ring would span well under one
//!      `RTprop` at that rate, the band `[τ, 2τ]` would contain no pairs at
//!      all, and `n` would be 0 on every block.** So `n ≥ 32` here is direct
//!      evidence that the time-decimation ran and that the τ-band found
//!      partners — not merely that a field was formatted. 32 is `L/8`, the
//!      `UNSCOREABLE-THIN` floor §16.75.6 F1 pre-registers for the parser.
//!   7. **THE RING BOUND HOLDS**: `n ≤ L − 1`. One anchor contributes at most
//!      one pair, so the pair count can never exceed the ring depth minus one.
//!      A count above it means the pair set is not the pair set the formula
//!      names.
//!   8. **τ WAS ESTABLISHED WHERE THE GAUGE READ.** Every block carrying a
//!      positive `tlag_us` also carries a positive `rtp…ms` — the very RTprop
//!      the band is built on. This separates a real reading from §16.75.6 F2's
//!      "τ unavailable" path, which is the one other way the gauge can be
//!      silent.
//!   9. **SCALE.** A dispersion of a loopback RTT cannot plausibly exceed a
//!      second — the µs/s unit error, caught at the instrument rather than in a
//!      results table.
//!
//! **What this binary deliberately does NOT assert, and it is the important
//! half.** Any ORDERING between `tlag_us` and any other gauge, and any VALUE.
//! Loopback's dispersion is the host scheduler's, not a network's; §16.74.5
//! requirement 3 binds, and loopback is neither of the two seats. **Rate
//! invariance itself is not testable here** — one host at one sample rate
//! cannot show a quantity is invariant across rates. That is what the re-run
//! VM battery is for, and §16.75.7's prediction `P4` is pre-registered against
//! it. This binary prints a characterization block for the record and asserts
//! nothing about its contents beyond reachability, feeding and scale.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The arm: the DIAG surface on, window-reliable — the same composition every
/// L1 battery arm runs. The gauge has no gate of its own to set.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// `SIGMA_CAND_WINDOW` from `scheduler/mod.rs`, restated here because a test
/// binary cannot see a private constant. If the engine's `L` moves, these
/// assertions fail loudly rather than silently weakening.
const WINDOW: u64 = 256;

/// The `UNSCOREABLE-THIN` floor `K = L/8` that paper §16.75.6 F1 pre-registers
/// as a PARSER rule. It is not a threshold in the engine and this test is the
/// only place in the tree it appears as a number.
const K_THIN: u64 = WINDOW / 8;

/// The four gauges the scored battery measured. They stay, unchanged, as the
/// re-run's controls and as its regression check.
const CONTROLS: [&str; 4] = ["sig_us=", "rvar_us=", "qsp_us=", "msd_us="];

/// The successor under test.
const TLAG: &str = "tlag_us=";

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

/// Nearest-rank quantile, the tree's own convention (`Path::cand_quantile`).
fn quantile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// The per-path block's own RTprop, the `rtp<floor>ms` TAIL of the block's
/// clock token `rtt=<app>/wrtt=<wire>/rtp<floor>ms`. This is the τ the gauge's
/// band is built on.
fn parse_rtp(toks: &[&str]) -> Option<f64> {
    toks.iter()
        .find(|t| t.starts_with("rtt=") && t.contains("/rtp"))
        .and_then(|t| t.rsplit_once("/rtp"))
        .and_then(|(_, r)| r.strip_suffix("ms"))
        .and_then(|r| r.parse::<f64>().ok())
}

#[test]
fn the_diag_line_reports_the_fixed_time_lag_dispersion_beside_its_four_controls() {
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

    // 3 + 4 + 5 + 8. EXISTENCE on every block; the four controls unchanged
    //    beside it; the `-` convention as a biconditional; and τ established
    //    wherever the gauge read a value.
    let mut readings: Vec<(Option<u64>, u64)> = Vec::new();
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

        // 4. THE CONTROLS SURVIVE. The re-run battery scores six estimators
        //    side by side and reads NO verdict from the successor's column if
        //    the four controls do not reproduce their committed verdicts
        //    (§16.75.7). That is only possible if they are all still emitted.
        for field in CONTROLS {
            let hits = toks.iter().filter(|t| t.starts_with(field)).count();
            assert_eq!(
                hits, n_rtp,
                "[DIAG] carries {n_rtp} per-path RTT blocks but {hits} `{field}` \
                 fields — a control gauge was dropped or replaced rather than \
                 kept beside the successor: {line}"
            );
        }

        // 3. THE SUCCESSOR EXISTS, on every block. THIS IS THE CLAUSE THAT
        //    FAILS ON THE PRE-CHANGE ENGINE: `tlag_us=` occurs nowhere in the
        //    tree at `6cf2328`. A gauge present on some paths and not others is
        //    worse than absent — a parser would average over a biased subset.
        let hits: Vec<&&str> = toks.iter().filter(|t| t.starts_with(TLAG)).collect();
        assert_eq!(
            hits.len(),
            n_rtp,
            "[DIAG] carries {n_rtp} per-path RTT blocks but {} `{TLAG}` fields \
             — the gauge is missing from at least one path: {line}",
            hits.len()
        );

        let rtp_ms = parse_rtp(&toks);
        for t in hits {
            let (v, n) = parse_gauge(TLAG, t);
            // 5. THE CONVENTION, BOTH WAYS. Value and count come from one pair
            //    set, so this holds by construction; asserting it is what makes
            //    "by construction" checkable from outside the crate.
            assert_eq!(
                v.is_none(),
                n == 0,
                "`{TLAG}` broke the `-`-iff-no-pair convention: read {v:?} at \
                 n={n} — a parser cannot tell a suppressed gauge from a leg too \
                 thin to hold a τ-lag pair: {line}"
            );
            // 7. THE RING BOUND. One anchor contributes at most one pair.
            assert!(
                n < WINDOW,
                "`{TLAG}` reports n={n} pairs from a ring of at most {WINDOW} \
                 entries — one anchor may contribute at most one pair, so a \
                 count of {n} is not the pair set |P(τ)| the formula names: {line}"
            );
            // 8. τ WAS ESTABLISHED WHERE THE GAUGE READ. Separates a real
            //    reading from §16.75.6 F2's "RTprop unavailable" silence.
            if v.is_some() {
                let r = rtp_ms.unwrap_or_else(|| {
                    panic!(
                        "`{TLAG}` read a value on a block with no parseable \
                         `rtp<floor>ms` token — the band's τ has no witness: {line}"
                    )
                });
                assert!(
                    r > 0.0,
                    "`{TLAG}` read a value at rtp={r}ms — the band [τ, 2τ] is \
                     degenerate at τ = 0 and cannot have admitted a pair: {line}"
                );
            }
            readings.push((v, n));
        }
    }
    assert!(
        blocks > 0,
        "no per-path [DIAG] block in the whole log — nothing to read a gauge off:\n{log}"
    );

    // 6. AND IT IS FED — AND THE DECIMATION EXECUTED.
    //
    //    This is the routing clause. On loopback the sender takes RTT samples
    //    at tens of kHz. A 256-entry ring holding EVERY sample would span well
    //    under one RTprop at that rate, so the band [τ, 2τ] would contain no
    //    admissible partner anywhere and every reading would be `-` at n = 0.
    //    A positive count here is therefore direct evidence that the `τ/m`
    //    admission spacing ran and that the τ-band found partners.
    let best = readings
        .iter()
        .filter_map(|(v, n)| v.map(|v| (v, *n)))
        .max_by_key(|(_, n)| *n)
        .unwrap_or_else(|| {
            panic!(
                "every [DIAG] `{TLAG}` read `-` over {} readings — either the \
                 feed site is unreached, or the ring is not time-decimated and \
                 spans less than one RTprop at the loopback sample rate, which \
                 is exactly the failure the decimation exists to prevent:\n{}",
                readings.len(),
                diag.join("\n")
            )
        });
    let (v, n) = best;
    assert!(
        n >= K_THIN,
        "`{TLAG}` never rested on more than n={n} pairs (UNSCOREABLE-THIN floor \
         K = L/8 = {K_THIN}). Over a multi-megabyte loopback transfer the ring \
         should span 32·RTprop and hold a partner for nearly every anchor; a \
         count this low means the decimation is not spacing admissions at τ/m"
    );
    assert!(
        v > 0,
        "`{TLAG}` read 0 µs at n={n} — an RTT series with literally zero \
         dispersion at a lag of one RTprop over a whole transfer is not a \
         measurement, it is an unfed gauge"
    );
    // 9. SCALE — the µs/s unit error, the most likely mistake in this change.
    assert!(
        v < 1_000_000,
        "`{TLAG}` = {v} µs on loopback is not a dispersion of a loopback RTT \
         — suspect a unit error in the gauge"
    );

    // ------------------------------------------------------------------
    // THE CHARACTERIZATION BLOCK — printed for the record, ASSERTED ON
    // NOWHERE. `R_local` is the acceptance bar's own functional (p95/p05 over
    // pooled post-warm-up readings) evaluated over this run's [DIAG] time
    // series. It is NOT `R_total`: the bar's statistic pools REPS at a shaped
    // cell, and this pools intervals of one loopback run. AND IT ESTABLISHES
    // NOTHING ABOUT RATE INVARIANCE — one host at one sample rate cannot.
    // ------------------------------------------------------------------
    let mut kept: Vec<u64> = readings
        .iter()
        .filter(|(_, n)| *n >= K_THIN)
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
    println!("\n[tlag] {blocks} per-path [DIAG] blocks, loopback, bulk, window-reliable");
    println!(
        "[tlag] {:<9} {:>12} {:>10} {:>10} {:>10} {:>8} {:>8}",
        "field", "best(µs)/n", "p05", "p50", "p95", "R_local", "n_kept"
    );
    println!(
        "[tlag] {:<9} {:>7}/n{:<4} {p05:>10} {p50:>10} {p95:>10} {r:>8} {:>8}",
        "tlag_us",
        v,
        n,
        kept.len()
    );
    println!(
        "[tlag] readings kept at n >= K_THIN = {K_THIN}; NOTHING HERE IS SCORED \
         — the bar is scored on the VM or it is not scored"
    );
}
