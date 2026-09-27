//! The receiver's `[LAT]` gauge decomposes delivered latency, and its
//! cross-path class is structurally zero at one path. Every delivered source
//! symbol's wait splits into `A_x` (queue and sender dwell above the path's
//! floor), `R` (the reorder wait, classed by the `[SUCC]` record of the hole
//! that released it: cross-path, same-path or repair) and `P` (the repair
//! wait). Clauses, in the order they can fail:
//!
//!   1. The line fires from the receiver with `n > 0`.
//!   2. Per path, `n = rwxp_n + rwsp_n + rwrep_n + nowait`.
//!   3. `rwxp_n ≡ 0` on one path: the release arrival cannot land on a
//!      different path from the one that exposed the hole. This is the
//!      control; a nonzero reading would void every dual-cell number.
//!   4. `rwxp_n > 0` on two paths (`c2,c3`), the scheduler's own inversions.
//!   5. The shares sum to 1 to rounding.
//!   6. Quantiles are ordered, and `-` iff the class has no sample.
//!   7. The receiver block also prints once at task exit, marked `final=1`.
//!
//! No share or quantile value is asserted, in particular not which term
//! dominates: loopback's queueing is the host scheduler's. The readout rides
//! `RWM_DIAG` beside `[SUCC]` and `[ETA]`.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

#[cfg_attr(not(unix), allow(unused_imports))]
use gauge::{f64_field, is_final, opt_field, slots, u64_field};

const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One loopback transfer. Returns the server (receiver) log.
///
/// The server is stopped with SIGINT so its receiver reaches the exit flush:
/// the perf server never ends a tunnel on its own and `Reaper` SIGKILLs
/// (no destructor). The server is waited for and its readers joined so the
/// log holds everything written on the way out. Without `kill` (non-unix)
/// the server is killed once a fresh cadence line arrived, so the `final=1`
/// tests are `cfg(unix)`.
fn run(paths: usize, netem: Option<&str>, bytes: &str) -> String {
    let binds = loopback::free_addrs(paths);
    let srv = loopback::spawn_perf_server(&binds, &ARM, &["--protocol-hint", "bulk", "--window-reliable"]);
    let mut env = ARM.to_vec();
    if let Some(spec) = netem {
        env.extend([("RWM_L0_NETEM", spec), ("RWM_L0_SEED", "42")]);
    }
    loopback::run_perf_client(&binds, &env, &loopback::perf_args("bulk", bytes, "2"));
    srv.stop_with("INT", "[LAT] site=receiver")
}

// ── Readers ─────────────────────────────────────────────────────────────

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

/// Clauses 1, 2, 5, 6 on any topology.
fn assert_lat_line(l: &str) -> Vec<String> {
    assert!(u64_field(l, "n=") > 0, "[LAT] n=0 — nothing was ever delivered: {l}");
    let sl = slots(l);
    assert!(!sl.is_empty(), "[LAT] fired with no path slot: {l}");
    for s in &sl {
        // 2. The accounting identity.
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
        // 5. The shares are a partition.
        let sh: f64 = ["sh_ax=", "sh_rwxp=", "sh_rwsp=", "sh_rwrep=", "sh_rep="]
            .iter()
            .map(|k| f64_field(s, k))
            .sum();
        assert!(
            (sh - 1.0).abs() < 1e-3,
            "[LAT] shares sum to {sh}, not 1 — they are not a partition of the \
             accumulated wait: {s}"
        );
        // 6. Ordered quantiles, `-` iff no sample.
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

// ── The two topologies ──────────────────────────────────────────────────

/// Clauses 1, 2, 3, 5, 6, 7 on one path and a short transfer.
///
/// The 1 MB object completes well under the receiver block's 1 s cadence, so
/// the only `[LAT]` line is the exit flush: it must carry `final=1`, be the
/// last of its kind (a scraper taking the last line reads complete counts)
/// and the only `final=1` line (exactly-once). `cfg(unix)`: see `run`.
#[cfg(unix)]
#[test]
fn the_decomposition_fires_and_cross_path_is_structurally_zero_on_one_path() {
    let log = run(1, None, "1000000");
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

    // 7. The last `[LAT]` line is the flush, and the only one; its siblings
    //    in the block flush with it.
    assert!(
        is_final(l),
        "the LAST [LAT] site=receiver line of a short transfer is not the exit \
         flush (`final=1`) — the receiver block was never flushed at exit, so a \
         transfer shorter than the 1 s cadence has no reading and every longer \
         one loses its final partial second:\n{l}"
    );
    let finals = log
        .lines()
        .filter(|l| l.contains("[LAT] site=receiver") && is_final(l))
        .count();
    assert_eq!(
        finals, 1,
        "exactly ONE final [LAT] block is owed per receiver task; the flush is \
         not idempotent:\n{log}"
    );
    for tag in ["[SUCC] ", "[ETA] site=receiver", "[REQ] ", "[RANK] "] {
        let last = log
            .lines()
            .rev()
            .find(|x| x.contains(tag))
            .unwrap_or_else(|| panic!("no `{tag}` line in the receiver log:\n{log}"));
        assert!(
            is_final(last),
            "`{tag}` was not flushed with the block: {last}"
        );
    }
}

/// Clause 4: the cross-path class is populated on two paths.
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
