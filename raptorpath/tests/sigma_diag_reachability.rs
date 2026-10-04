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
//!      is not constant even without netem jitter or loss.
//!   5. σ is under one second on the shaped loopback (µs/s unit error).
//!
//! The client runs on a RATE-SHAPED loopback (`loopback::CLEAN_RATE`: the L0
//! netem shim at 40 Mbit, no delay, no jitter, no loss). `[DIAG]` samples
//! saturation only on 250 ms ticks, so an unshaped, CPU-bound transfer gives
//! a tick count that shrinks with every sender speed-up; the shaper bounds
//! the wall time below by `bytes·8/R`. The harness clauses prove it ran: the
//! shim's ACTIVE echo, a per-run duration floor and a tick-count floor.
//!
//! No value of σ is asserted: a rate-shaped loopback's dispersion is the host
//! scheduler's plus the shaper's queueing.

#[path = "common/loopback.rs"]
mod loopback;

/// The tick-count floor: two 8 MB runs at 40 Mbit are ≥ 3.2 s of shaped
/// transfer, ≥ 12 ticks at the 250 ms `[DIAG]` cadence (the cadence drifts
/// late by each tick's loop latency, so the floor keeps a margin of 4).
const MIN_TICKS: usize = 8;

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
    println!("[sigma-diag] shaped run seconds: {secs:?}; {} [DIAG] ticks", diag.len());
    assert!(
        diag.len() >= MIN_TICKS,
        "only {} [DIAG] ticks over two shaped 8 MB runs (floor {MIN_TICKS}) — the \
         tick-sampled clauses are back to a handful of samples:\n{}",
        diag.len(),
        diag.join("\n")
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
        "no [DIAG] tick saw the single path cwnd-full over two 8 MB bulk \
         objects through a 40 Mbit shaper — the saturated case this clause exists to cover was not \
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

    // 5. Scale: σ cannot plausibly exceed a second on a rate-shaped loopback
    //    (µs/s unit error).
    assert!(
        sigma_us < 1_000_000,
        "σ = {sigma_us} µs on a shaped loopback is not a dispersion of its RTT — \
         suspect a unit error in the gauge"
    );
}
