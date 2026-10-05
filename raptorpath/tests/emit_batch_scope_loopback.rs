//! Emission-batching scope (status §8; T1 + T6): with `RWM_EMIT_BATCH=1`
//! the burst intake is LIVE at every path count — N = 1, 2 and 4 — and the
//! transfer completes. Through `ddfc07a` the batching path was scoped on
//! `live_paths == 1`, a path-count step: at N ≥ 2 the emission path was
//! bit-identical to gate-off (mean burst depth ≤ 1, no burst gauge at all).
//!
//! Each N runs the shipped binary end to end (`perf` server + client over N
//! loopback binds, window-reliable bulk, `RWM_EMIT_BURST=8`, `RWM_DIAG=1`)
//! and reads the client sender's last `[DIAG]` line (cumulative,
//! last-line-wins):
//!
//!   1. the gate echo, two-sided (`RWM_EMIT_BATCH=1` present, `=0` absent)
//!      and the `emission batching ACTIVE` liveness echo;
//!   2. `np=N` on that line — the run really had N paths (rule 1 applied to
//!      the multipath run itself: a "dual" that silently ran single-path
//!      would pass trivially);
//!   3. `eb_bursts > 0` and mean depth `eb_syms / eb_bursts > 1`;
//!   4. no burst exceeds the bound: `eb_syms ≤ 8 · eb_bursts`, and the end
//!      tallies partition the bursts;
//!   5. at N ≥ 2 the bursts placed symbols on more than one path
//!      (`eb_maxrun` names ≥ 2 paths) — the burst is striped, not scoped
//!      away.
//!
//! Completion (the perf client exits 0 only when every object is acked
//! whole) is the no-loss-at-burst-boundaries check, as in
//! `emit_batch_loopback.rs`.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

const BURST: u64 = 8;

const ARM: [(&str, &str); 5] = [
    ("RWM_EMIT_BATCH", "1"),
    ("RWM_EMIT_BURST", "8"),
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// Run one N and return the client's last `[DIAG]` line (owned).
fn run_n(n: usize) -> (String, String) {
    let binds = loopback::free_addrs(n);
    let srv = loopback::spawn_perf_server(
        &binds,
        &ARM,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let log = loopback::run_perf_client(&srv.addrs, &ARM, &loopback::perf_args("bulk", "24000000", "2"));
    let last = log
        .lines()
        .filter(|l| l.contains("[DIAG] "))
        .last()
        .unwrap_or_else(|| panic!("N={n}: no [DIAG] line with RWM_DIAG=1:\n{log}"))
        .to_string();
    (log, last)
}

fn check(n: usize) -> f64 {
    let (log, line) = run_n(n);
    // 1. Two-sided gate echo + liveness echo.
    assert!(log.contains("RWM_EMIT_BATCH=1"), "N={n}: [GATES] lacks RWM_EMIT_BATCH=1:\n{log}");
    assert!(!log.contains("RWM_EMIT_BATCH=0"), "N={n}: [GATES] carries both sides:\n{log}");
    assert!(
        log.contains("emission batching ACTIVE"),
        "N={n}: the batching liveness echo is absent:\n{log}"
    );
    // 2. The run had N paths.
    assert_eq!(gauge::u64_field(&line, "np="), n as u64, "N={n}: np= on the [DIAG] line: {line}");
    // 3. Bursts happened and are deeper than one symbol.
    let bursts = gauge::u64_field(&line, "eb_bursts=");
    let syms = gauge::u64_field(&line, "eb_syms=");
    assert!(bursts > 0, "N={n}: no burst at all (the scope forbids batching here?): {line}");
    let depth = syms as f64 / bursts as f64;
    println!("N={n}: eb_bursts={bursts} eb_syms={syms} mean depth={depth:.2}  [{}] [{}]",
        gauge::str_field(&line, "eb_end="), gauge::str_field(&line, "eb_maxrun="));
    assert!(depth > 1.0, "N={n}: mean burst depth {depth:.2} ≤ 1 — batching is not live: {line}");
    // 4. The bound holds and the end tallies partition the bursts.
    assert!(syms <= BURST * bursts, "N={n}: a burst exceeded the bound {BURST}: {line}");
    let ends = gauge::str_field(&line, "eb_end=");
    let total: u64 = ends
        .split('/')
        .map(|kv| {
            let (_, v) = kv.split_once(':').unwrap_or_else(|| panic!("eb_end token `{kv}`"));
            gauge::numeric_prefix(v).parse::<u64>().unwrap()
        })
        .sum();
    assert_eq!(total, bursts, "N={n}: eb_end tallies do not partition the bursts: {line}");
    // 5. At N ≥ 2 the bursts were striped across paths.
    let runs = gauge::str_field(&line, "eb_maxrun=");
    let named = if runs == "-" { 0 } else { runs.split(',').count() };
    if n >= 2 {
        assert!(named >= 2, "N={n}: bursts touched {named} path(s), expected ≥ 2: {line}");
    } else {
        assert_eq!(named, 1, "N=1: bursts must name the one path: {line}");
    }
    depth
}

/// T1 (red through ddfc07a: the scope forbade a burst at N = 2) and T6 (the
/// batching path is live at N = 1, 2, 4). One test function: the perf
/// endpoints are child processes with their own env, but the runs are kept
/// sequential so loopback load does not mix N's.
#[test]
fn batching_is_live_at_every_path_count() {
    let mut depths = Vec::new();
    for n in [1usize, 2, 4] {
        depths.push((n, check(n)));
    }
    println!("mean burst depth by N: {depths:?}");
}
