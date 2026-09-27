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
//!
//! **The exit flush (2026-09-08).** The receiver block ALSO prints once at
//! the end of the receiver task, marked `final=1` (`net/recv_block.rs`), so a
//! transfer shorter than the cadence still has a reading. The N = 1 test
//! below is that case; it stops the server with SIGINT so the task actually
//! reaches an exit (see `run`).

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

/// One loopback transfer. Returns the SERVER (receiver) log.
///
/// STOP THE SERVER THE WAY ITS RECEIVER CAN SEE (the exit flush, 2026-09-08).
///
/// The perf server never ends a tunnel on its own: the client leaving is not
/// a terminal event for the engine (dead-path detection is 6 s away and does
/// not shut the tunnel down), and `Reaper` SIGKILLs, which runs no
/// destructor — so under the old "sleep 1.5 s and read" the receiver task
/// never reached ANY of its exit paths and its `final=1` block could never
/// be observed. Here the server is sent SIGINT — the engine's `ctrl_c`
/// handler → the shutdown broadcast → the receiver's shutdown arm → its
/// exit flush — then waited for, and its readers are joined so the log holds
/// everything the process wrote on the way out. This is the terminal path the
/// L1 harnesses would take with `pkill -INT`; a harness that SIGKILLs still
/// reads the last cadence line.
///
/// Without `kill` (non-unix) the server is killed once one fresh cadence line
/// arrived: the cadence lines are there, the `final=1` block is not, and the
/// tests that need it are `cfg(unix)`.
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

// ── READERS ─────────────────────────────────────────────────────────────

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
/// PATH, ON A SHORT TRANSFER. The control — a nonzero reading here voids
/// every dual-cell number.
///
/// 7. **THE EXIT FLUSH** (goal-gate "OPERATOR SANCTION (2026-09-08
///    ~14:00Z)"). The object is 1 MB — it completes in well under the
///    receiver block's 1 s cadence on any host, which is EXACTLY the case
///    that went red on the VM: the 8 MB object this test used to send
///    finished in ~0.4 s there, the cadence never fired, and there was no
///    `[LAT]` line at all. The receiver line must now be present, carry
///    `final=1`, be the LAST of its kind (a scraper that takes the last line
///    reads the complete counts) and be the ONLY `final=1` line of its kind
///    (the flush is exactly-once). Fails on the shipped-before engine twice
///    over: no line on a fast host, and no marker on any host.
///
/// `cfg(unix)`: the server has to be stopped with SIGINT for its receiver to
/// reach an exit path at all — see `run`.
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

    // 7. THE EXIT FLUSH. The last `[LAT]` line IS the flush, and it is the
    //    only one; its siblings in the block are flushed with it.
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
