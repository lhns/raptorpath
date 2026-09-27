//! The `[DIAG]` line reports three candidate RTT dispersion gauges —
//! `rvar_us=`, `qsp_us=`, `msd_us=` — beside the shipped `sig_us=`, on the
//! same sample stream in the same run, so every comparison is paired per path
//! per interval (paper §7.4). Each candidate moves one axis of the shipped
//! estimator:
//!
//! ```text
//!   axis              sig_us (shipped)   rvar      qsp       msd
//!   ----------------  -----------------  --------  --------  --------
//!   memory            7 samples          7         L = 256   L = 256
//!   deviation enters  squared            linear    rank      rank
//!   reference         lagging srtt       lagging   none      none
//! ```
//!
//! `rvar` vs `sig_us` isolates the square; `qsp` vs `rvar` the memory; `msd`
//! vs `qsp` the reference. Clauses, in the order they can fail:
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires, with per-path blocks.
//!   3. Every per-path block carries all three candidate fields.
//!   4. `sig_us=` is still there, exactly once per block.
//!   5. Each candidate reads `-` iff its own sample count is 0 (`sig_us`
//!      also renders `-` for a zero dispersion, so it is exempt).
//!   6. All three are fed and positive, and the window-class pair reaches a
//!      full window (`n` = 256).
//!   7. Scale: under one second on loopback (µs/s unit error).
//!
//! No ordering and no value is asserted: loopback's dispersion is the host
//! scheduler's, and the acceptance bar is scored on the VM. The
//! characterization block is printed only.

#[path = "common/loopback.rs"]
mod loopback;

/// The arm: the DIAG surface on, as every L1 battery arm runs it. The
/// candidate gauges have no gate of their own.
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
    let srv = loopback::spawn_perf_server(
        &[loopback::free_addr()],
        &ARM,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let log = loopback::run_perf_client(&srv.addrs, &ARM, &loopback::perf_args("bulk", "8000000", "2"));

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

    // 3 + 4 + 5: existence on every block for all four fields, and the `-`
    //    convention as a biconditional on every reading.
    let mut readings: [Vec<(Option<u64>, u64)>; 4] = Default::default();
    let mut blocks = 0usize;
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
        for (fi, field) in FIELDS.iter().enumerate() {
            let hits: Vec<&&str> = toks.iter().filter(|t| t.starts_with(field)).collect();
            // A gauge present on some paths and not others is worse than
            // absent: a parser would average over a biased subset. This also
            // fails if a change replaces `sig_us` rather than adding beside it.
            assert_eq!(
                hits.len(),
                n_rtp,
                "[DIAG] carries {n_rtp} per-path RTT blocks but {} `{field}` fields \
                 — the gauge is missing from at least one path: {line}",
                hits.len()
            );
            for t in hits {
                let (v, n) = parse_gauge(field, t);
                // 5. `-` iff the sample set is empty — candidates only; the
                //    shipped `sig_us` renders `-` for a zero dispersion too.
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

    // 6. They are fed: a candidate that never became positive over thousands
    //    of RTT samples has an unreached feed site.
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
        // 7. Scale — the µs/s unit error.
        assert!(
            v < 1_000_000,
            "`{field}` = {v} µs on loopback is not a dispersion of a loopback \
             RTT — suspect a unit error in the gauge"
        );
    }

    // 6b. The window-class pair reaches a full window; a window that never
    //     fills reports a quantile of fewer order statistics than it claims.
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
    // Characterization block — printed, asserted nowhere. `R_local` is the
    // bar's functional (p95/p05 over post-warm-up readings) over this run's
    // [DIAG] series; it is not `R_total`, which pools reps at a shaped cell.
    // ------------------------------------------------------------------
    println!("\n[sigma-cand] {blocks} per-path [DIAG] blocks, loopback, bulk, window-reliable");
    println!(
        "[sigma-cand] {:<9} {:>12} {:>10} {:>10} {:>10} {:>8} {:>8}",
        "field", "best(µs)/n", "p05", "p50", "p95", "R_local", "n_kept"
    );
    for (fi, field) in FIELDS.iter().enumerate() {
        // Warm-up exclusion: EWMA classes at n >= 16, window classes at a
        // full window.
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
