//! The `[DIAG]` line reports the RTT dispersion σ as `sig_us=<µs>/n<count>`.
//! `Path::rtt_sigma_us()` is the second moment `√(EWMA[(rtt − srtt)²])` at
//! RFC 6298's β = 1/4, the σ in the recovery clock `W(α) = srtt + k(α)·σ`
//! (paper §7.4). `[DIAG]` is an `eprintln!` inside the sender loop, so only a
//! run of the shipped binary shows the field reaches a log. Clauses, in the
//! order they can fail:
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires, with a per-path block.
//!   3. Every per-path block carries `sig_us=<µs|->/n<count>`.
//!   3b. `np=` counts registered paths, and the per-path block prints on a
//!       cwnd-full path (`np_act=` is the saturation-filtered count).
//!   4. At least one block reports a parsed, positive σ fed by more than the
//!      EWMA's seed: an app-echo RTT over a real scheduler, store and ack path
//!      is not constant even without netem.
//!   5. σ is under one second on loopback (µs/s unit error).
//!
//! No value of σ is asserted: loopback's dispersion is the host scheduler's.

#[path = "common/loopback.rs"]
mod loopback;

/// The arm: the DIAG surface on, as every L1 battery arm runs it. No gate
/// here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// Parse one per-path `sig_us=<µs|->/n<count>` token into (σ µs, n). `None`
/// is the `-` (no sample yet) reading, not a parse failure.
fn parse_sig(tok: &str) -> (Option<u64>, u64) {
    let v = tok.strip_prefix("sig_us=").expect("caller filters on the prefix");
    let (sig, n) = v.split_once("/n").unwrap_or_else(|| {
        panic!("sig_us= must render as `<µs|->/n<count>`, got `{tok}`")
    });
    let n: u64 = n
        .parse()
        .unwrap_or_else(|e| panic!("sig_us= sample count `{n}` does not parse: {e}"));
    if sig == "-" {
        return (None, n);
    }
    let sig: u64 = sig
        .parse()
        .unwrap_or_else(|e| panic!("sig_us= value `{sig}` does not parse: {e}"));
    (Some(sig), n)
}

#[test]
fn the_diag_line_reports_the_rtt_sigma_the_recovery_clock_needs() {
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

    // 3. The field exists on every per-path block.
    let mut sigs: Vec<(Option<u64>, u64)> = Vec::new();
    let mut blocks = 0usize;
    for line in &diag {
        let toks: Vec<&str> = line.split_whitespace().collect();
        // A per-path block is identified by its own clock token,
        // `rtt=<app>/wrtt=<wire>/rtp<floor>ms`; the `/wrtt=` distinguishes it
        // from the line's aggregate `rtt=<ms>ms`. A line with `np=0` (no path
        // registered) has the aggregate and no block, a legitimate reading;
        // clause 3b pins that this never happens on a single-path run.
        let n_sig = toks.iter().filter(|t| t.starts_with("sig_us=")).count();
        let n_rtp = toks
            .iter()
            .filter(|t| t.starts_with("rtt=") && t.contains("/wrtt="))
            .count();
        if n_rtp == 0 {
            continue;
        }
        blocks += n_rtp;
        // A gauge present on some paths and not others is worse than absent:
        // a parser would average over a biased subset.
        assert_eq!(
            n_sig, n_rtp,
            "[DIAG] carries {n_rtp} per-path RTT blocks but {n_sig} sig_us= \
             fields — σ is missing from at least one path: {line}"
        );
        for t in toks.iter().filter(|t| t.starts_with("sig_us=")) {
            sigs.push(parse_sig(t));
        }
    }
    assert!(
        blocks > 0,
        "no per-path [DIAG] block in the whole log — nothing to read σ off:\n{log}"
    );

    // 3b. `np=` counts registered paths and the block prints on a saturated
    //     one. The path is registered before the sender loop starts
    //     (`run_impl` adds every configured path up front), so on a
    //     single-path run every `[DIAG]` line owes `np=1`, `np_act <= np`, and
    //     exactly `np` per-path blocks.
    let mut saturated_ticks = 0usize;
    for line in &diag {
        let toks: Vec<&str> = line.split_whitespace().collect();
        let num = |key: &str| -> u64 {
            toks.iter()
                .find_map(|t| t.strip_prefix(key))
                .unwrap_or_else(|| panic!("`{key}` missing from [DIAG]: {line}"))
                .parse()
                .unwrap_or_else(|e| panic!("`{key}` does not parse: {e} in {line}"))
        };
        let np = num("np=");
        let np_act = num("np_act=");
        assert_eq!(
            np, 1,
            "one configured path must read np=1 on every tick: {line}"
        );
        assert!(
            np_act <= np,
            "the saturation-filtered count exceeds the registered one: {line}"
        );
        let n_sig = toks.iter().filter(|t| t.starts_with("sig_us=")).count() as u64;
        assert_eq!(
            n_sig, np,
            "np={np} registered paths but {n_sig} sig_us= blocks — the block's \
             print condition is still keyed on the saturation-filtered set: {line}"
        );
        if np_act < np {
            saturated_ticks += 1;
        }
    }
    println!(
        "[sigma-diag] {saturated_ticks} of {} [DIAG] ticks had a cwnd-full path (np_act < np) — \
         each still carried its sig_us= block",
        diag.len()
    );
    assert!(
        saturated_ticks > 0,
        "no [DIAG] tick saw the single loopback path cwnd-full over two 8 MB bulk \
         objects — the saturated case this clause exists to cover was not \
         exercised:\n{}",
        diag.join("\n")
    );

    // 4. It is fed: over thousands of RTT samples a σ that never became
    //    positive would mean the EWMA is not reached.
    let best = sigs
        .iter()
        .filter_map(|(s, n)| s.map(|s| (s, *n)))
        .max_by_key(|(_, n)| *n);
    let (sigma_us, n) = best.unwrap_or_else(|| {
        panic!(
            "every [DIAG] sig_us= read `-` over {} samples of the field — the \
             σ EWMA was never fed:\n{}",
            sigs.len(),
            diag.join("\n")
        )
    });
    println!("[sigma-diag] best σ reading: sig_us={sigma_us} n={n} over {blocks} path-blocks");
    assert!(
        sigma_us > 0,
        "σ read 0 µs at n={n} — an RTT series with literally zero dispersion \
         over a whole transfer is not a measurement, it is an unfed gauge"
    );
    assert!(
        n > 8,
        "the σ EWMA folded only {n} samples over the whole transfer — the \
         count is the gauge's own warm-up evidence and it says the feed site \
         is barely reached"
    );

    // 5. Scale: σ cannot plausibly exceed a second on loopback (µs/s unit
    //    error).
    assert!(
        sigma_us < 1_000_000,
        "σ = {sigma_us} µs on loopback is not a dispersion of a loopback RTT — \
         suspect a unit error in the gauge"
    );
}
