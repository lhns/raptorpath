//! The `[DIAG]` line reports the fixed-time-lag RTT dispersion gauge
//! `tlag_us=` on every per-path block, beside the four older gauges it is
//! scored against (paper §7.4):
//!
//! ```text
//!     σ̂_Δ(τ) = median { |rtt(tᵢ) − rtt(tⱼ)| : (i, j) ∈ P(τ) }
//!
//!     P(τ) = { (i, j(i)) : j(i) = the most recent sample with tᵢ − tⱼ ≥ τ,
//!                          admitted iff tᵢ − t_{j(i)} ≤ c·τ }
//!
//!     τ = RTprop (measured),   c = 2
//! ```
//!
//! `[DIAG]` is an `eprintln!` inside the sender loop, so only a run of the
//! shipped binary shows the field reaches a log. Clauses, in the order they
//! can fail:
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires, with per-path blocks.
//!   3. Every per-path block carries `tlag_us=`.
//!   4. The four older gauges are still there, exactly once per block.
//!   5. `-` iff `n == 0`, on every reading (value and count come from one
//!      `tlag_diffs()` pair set).
//!   6. The time decimation executed: on loopback the sender samples RTT at
//!      kHz rates (thousands of packets per second even through the 50 Mbit
//!      shaper), so without the `τ/m` admission spacing a 256-entry ring
//!      would span under one RTprop and `n` would be 0 everywhere. `n ≥ L/8`
//!      is the parser's thin floor.
//!   7. The ring bound: `n ≤ L − 1` (one anchor contributes at most one pair).
//!   8. τ was established where the gauge read: every valued block carries a
//!      parseable `rtp…ms`, positive wherever the whole-ms token can resolve
//!      it (see `parse_rtp`).
//!   9. Scale: a shaped-loopback RTT dispersion is under one second (µs/s
//!      unit error).
//!
//! The client runs on a RATE-SHAPED loopback (`loopback::CLEAN_RATE`: the L0
//! netem shim at 50 Mbit, no delay, no jitter, no loss), so the number of
//! 250 ms `[DIAG]` ticks — and the ring's fill at a printed tick — does not
//! shrink with sender speed-ups. The harness clauses prove the shaper ran:
//! its ACTIVE echo, a per-run duration floor and a tick-count floor.
//!
//! No ordering between gauges and no value is asserted: a rate-shaped
//! loopback's dispersion is the host scheduler's plus the shaper's queueing,
//! and one host at one sample rate cannot show rate invariance. The
//! characterization block is printed only.

#[path = "common/loopback.rs"]
mod loopback;

/// The tick-count floor: two 8 MB runs at 50 Mbit are ≥ 2.56 s of shaped
/// transfer, ≥ 10 ticks at the 250 ms `[DIAG]` cadence.
const MIN_TICKS: usize = 8;

/// The arm: the DIAG surface on, as every L1 battery arm runs it. The gauge
/// has no gate of its own.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// `SIGMA_CAND_WINDOW` from `scheduler/mod.rs`, restated here because a test
/// binary cannot see a private constant. If the engine's `L` moves, these
/// assertions fail loudly rather than silently weakening.
const WINDOW: u64 = 256;

/// The thin floor `K = L/8` the battery parser applies. Not an engine
/// threshold.
const K_THIN: u64 = WINDOW / 8;

/// The four older gauges, kept unchanged as controls.
const CONTROLS: [&str; 4] = ["sig_us=", "rvar_us=", "qsp_us=", "msd_us="];

/// The successor under test.
const TLAG: &str = "tlag_us=";

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
/// band is built on. Returns the printed text and its value.
///
/// The print is whole milliseconds (`rtp{:.0}ms`, `net/diag.rs`), and an
/// unset RTprop prints `0` too. Release-build loopback RTprop is routinely
/// below 0.5 ms, so `rtp0ms` means "τ < 0.5 ms", not "τ = 0".
fn parse_rtp<'a>(toks: &[&'a str]) -> Option<(&'a str, f64)> {
    toks.iter()
        .find(|t| t.starts_with("rtt=") && t.contains("/rtp"))
        .and_then(|t| t.rsplit_once("/rtp"))
        .and_then(|(_, r)| r.strip_suffix("ms"))
        .and_then(|r| r.parse::<f64>().ok().map(|v| (r, v)))
}

/// Is `txt` a whole-ms `0` — the one rendering whose τ the token cannot
/// decide (τ ∈ [0, 0.5) ms)? A print with a fractional part (a finer engine
/// format) is decidable, and then clause 8 is asserted on it in full.
fn rtp_rounds_away(txt: &str) -> bool {
    !txt.contains('.') && txt.parse::<u64>() == Ok(0)
}

#[test]
fn the_diag_line_reports_the_fixed_time_lag_dispersion_beside_its_four_controls() {
    let srv = loopback::spawn_perf_server(
        &[loopback::free_addr()],
        &ARM,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let cli_env: Vec<(&str, &str)> = ARM.iter().chain(loopback::CLEAN_RATE.iter()).copied().collect();
    let log = loopback::run_perf_client(&srv.addrs, &cli_env, &loopback::perf_args("bulk", "8000000", "2"));

    // 1. The gate, two-sided: a missing `[DIAG]` must read as an unreached
    //    emission site, never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the [GATES] echo does not carry RWM_DIAG=1 — the arm did not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );

    // 2. The line fires, with per-path blocks.
    let diag: Vec<&str> = log.lines().filter(|l| l.contains("[DIAG] ")).collect();
    assert!(
        !diag.is_empty(),
        "no [DIAG] line in a run with RWM_DIAG=1 — the report is unreachable:\n{log}"
    );

    // 2a. The harness ran as designed: the shaper executed (shim ACTIVE, each
    //     run at least 0.8 × bytes·8/R) and the run spanned enough 250 ms
    //     ticks that the tick-sampled clauses below are not a coin flip.
    let secs = loopback::assert_clean_rate_executed(&log, 8_000_000, 2);
    println!("[tlag] shaped run seconds: {secs:?}; {} [DIAG] ticks", diag.len());
    assert!(
        diag.len() >= MIN_TICKS,
        "only {} [DIAG] ticks over two shaped 8 MB runs (floor {MIN_TICKS}) — the \
         tick-sampled clauses are back to a handful of samples:\n{}",
        diag.len(),
        diag.join("\n")
    );

    // 3 + 4 + 5 + 8: existence on every block; the four controls beside it;
    //    the `-` convention as a biconditional; τ established wherever the
    //    gauge read a value.
    let mut readings: Vec<(Option<u64>, u64)> = Vec::new();
    let mut blocks = 0usize;
    // Clause-8 readings whose τ the whole-ms token cannot decide (see
    // `parse_rtp`). Counted and printed, never silently passed.
    let mut tau_undecidable = 0usize;
    for line in &diag {
        let toks: Vec<&str> = line.split_whitespace().collect();
        // A per-path block is identified by its own clock token,
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

        // 4. The controls survive: the battery reads no verdict from the
        //    successor's column unless they are all still emitted.
        for field in CONTROLS {
            let hits = toks.iter().filter(|t| t.starts_with(field)).count();
            assert_eq!(
                hits, n_rtp,
                "[DIAG] carries {n_rtp} per-path RTT blocks but {hits} `{field}` \
                 fields — a control gauge was dropped or replaced rather than \
                 kept beside the successor: {line}"
            );
        }

        // 3. The successor exists on every block. A gauge present on some
        //    paths and not others is worse than absent: a parser would average
        //    over a biased subset.
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
            // 5. The convention, both ways — by construction, checked from
            //    outside the crate.
            assert_eq!(
                v.is_none(),
                n == 0,
                "`{TLAG}` broke the `-`-iff-no-pair convention: read {v:?} at \
                 n={n} — a parser cannot tell a suppressed gauge from a leg too \
                 thin to hold a τ-lag pair: {line}"
            );
            // 7. The ring bound: one anchor contributes at most one pair.
            assert!(
                n < WINDOW,
                "`{TLAG}` reports n={n} pairs from a ring of at most {WINDOW} \
                 entries — one anchor may contribute at most one pair, so a \
                 count of {n} is not the pair set |P(τ)| the formula names: {line}"
            );
            // 8. τ was established where the gauge read, separating a real
            //    reading from the "RTprop unavailable" silence. Decidable only
            //    where the print is not a rounded-away `0`
            //    (`rtp_rounds_away`); at `rtp0ms` the τ > 0 half is the
            //    engine's own construction (`tlag_diffs` returns no pair when
            //    `min_rtt` is unset or zero, covered by clause 5).
            if v.is_some() {
                let (txt, r) = rtp_ms.unwrap_or_else(|| {
                    panic!(
                        "`{TLAG}` read a value on a block with no parseable \
                         `rtp<floor>ms` token — the band's τ has no witness: {line}"
                    )
                });
                assert!(r.is_finite() && r >= 0.0, "rtp={txt}ms is not an RTprop: {line}");
                if rtp_rounds_away(txt) {
                    tau_undecidable += 1;
                } else {
                    assert!(
                        r > 0.0,
                        "`{TLAG}` read a value at rtp={txt}ms — the band [τ, 2τ] is \
                         degenerate at τ = 0 and cannot have admitted a pair: {line}"
                    );
                }
            }
            readings.push((v, n));
        }
    }
    assert!(
        blocks > 0,
        "no per-path [DIAG] block in the whole log — nothing to read a gauge off:\n{log}"
    );

    // 6. The gauge is fed and the decimation executed: a positive count is
    //    direct evidence that the `τ/m` admission spacing ran and the τ-band
    //    found partners.
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
    // 9. Scale — the µs/s unit error.
    assert!(
        v < 1_000_000,
        "`{TLAG}` = {v} µs on a shaped loopback is not a dispersion of its RTT \
         — suspect a unit error in the gauge"
    );

    // ------------------------------------------------------------------
    // Characterization block — printed, asserted nowhere. `R_local` is the
    // bar's functional (p95/p05 over post-warm-up readings) over this run's
    // [DIAG] series; it is not `R_total` (which pools reps at a shaped cell)
    // and says nothing about rate invariance.
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
    println!("\n[tlag] {blocks} per-path [DIAG] blocks, 50 Mbit rate-shaped loopback, bulk, window-reliable");
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
    println!(
        "[tlag] clause 8: {tau_undecidable} valued readings at a whole-ms rtp0ms \
         (τ < 0.5 ms, undecidable from the token; see `parse_rtp`)"
    );
}
