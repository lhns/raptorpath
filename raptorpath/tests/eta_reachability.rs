//! The sender's per-placement delivery prediction reaches the wire (v8
//! `SymbolBatch.eta_rel_us`) and both `[ETA]` gauges read it on one run. The
//! placement law computes each path's expected delivery time
//! (`expected_delivery_load`) to choose a path; the receiver otherwise
//! detects holes by sequence alone. Clauses, in the order they can fail:
//!
//!   1. v8 is on the wire: a v7 peer refuses at handshake, so a completed
//!      transfer is the version assertion.
//!   2. The sender line fires with `n > 0` stamped placements and a finite F̂.
//!   3. Window source batches carry a prediction: the sender's `zero=` is
//!      below 1 and some receiver path counts predicted arrivals.
//!   4. σ̂ is finite at both ends (`-` iff `n = 0`, by construction in `Tlag`).
//!   5. The witness `σ̂_sender ≥ σ̂_recv` (`net/eta.rs`: the sender's error
//!      rides a round trip, the receiver's only the forward leg) is printed,
//!      not asserted — loopback's return leg is the host scheduler's.
//!   6. The bind gauges (`zero=`, `cold_r=`, `cold_ge=`, `bind=`) are present
//!      and are fractions.
//!   7. The receiver block also prints once at task exit, marked `final=1`,
//!      so a transfer shorter than the cadence still has a reading.
//!
//! No value of σ̂, the lateness quantiles or the prediction error is
//! asserted. Both readouts ride `RWM_DIAG` (sender beside `[DIAG]` at 250 ms,
//! receiver beside `[SUCC]` at 1 s). Own test binary: `RWM_L0_NETEM` is
//! process-global in the child, and the spawned pair must not contend with
//! the in-process loopback tests.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

#[cfg_attr(not(unix), allow(unused_imports))]
use gauge::{is_final, opt_f64_field as f64_field, require, slots, str_field, u64_field};

const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// Stops the server so its receiver reaches an exit path. The perf server
/// never ends a tunnel on its own and `Reaper` SIGKILLs (no destructor), so
/// the server is signalled — the engine's shutdown broadcast then reaches the
/// receiver's exit flush — waited for, and its readers joined so the log
/// holds everything written on the way out. `"INT"` is `ctrl_c`; `"TERM"` is
/// what the tools/l1 harnesses send (`pkill -x raptorpath`). The engine treats
/// both as one shutdown trigger (`shutdown_signal`). Without `kill`
/// (non-unix) the server is killed once a fresh cadence line arrived, so the
/// `final=1` tests are `cfg(unix)`.
fn run_with_signal(paths: usize, netem: Option<&str>, bytes: &str, sig: &str) -> (String, String) {
    let binds = loopback::free_addrs(paths);
    let srv = loopback::spawn_perf_server(&binds, &ARM, &["--protocol-hint", "bulk", "--window-reliable"]);
    let mut env = ARM.to_vec();
    if let Some(spec) = netem {
        env.extend([("RWM_L0_NETEM", spec), ("RWM_L0_SEED", "42")]);
    }
    let cli = loopback::run_perf_client(&binds, &env, &loopback::perf_args("bulk", bytes, "2"));
    let srv = srv.stop_with(sig, "[ETA] site=receiver");
    (cli, srv)
}

/// One loopback transfer. Returns `(client/sender log, server/receiver log)`.
fn run(paths: usize, netem: Option<&str>, bytes: &str) -> (String, String) {
    run_with_signal(paths, netem, bytes, "INT")
}

// ── Readers ─────────────────────────────────────────────────────────────

/// `sig_us=<v|->/n<pairs>` — returns `(value, pairs)`; `None` iff pairs is 0.
fn sigma(line: &str) -> (Option<u64>, u64) {
    let raw = str_field(line, "sig_us=");
    let (v, n) = raw.split_once("/n").unwrap_or_else(|| panic!("malformed sig_us slot `{raw}`"));
    let pairs: u64 = n.parse().expect("pair count");
    let val = if v == "-" { None } else { Some(v.parse::<u64>().expect("sigma")) };
    assert_eq!(
        val.is_none(),
        pairs == 0,
        "`-` must hold IFF the pair count is 0 — the biconditional is by \
         construction in `Tlag` and this is where it is checked on the engine's \
         own output: {line}"
    );
    (val, pairs)
}

// ── The run ─────────────────────────────────────────────────────────────

/// Clauses 1-6: both gauges fire over a lossy dual-path transfer, and the
/// witness is makeable.
#[test]
fn the_senders_prediction_reaches_the_wire_and_both_gauges_read_it() {
    // Two shaped paths: the placement law must actually choose for the
    // prediction to mean anything; a single path collapses it to an identity.
    let (cli, srv) = run(2, Some("c2,c3"), "24000000");

    // The gate: a missing `[ETA]` must read as an unreached emission site,
    // never as an unset gate.
    assert!(cli.contains("RWM_DIAG=1"), "the sender's [GATES] echo lacks RWM_DIAG=1:\n{cli}");
    assert!(srv.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{srv}");

    // 2. The sender line fires.
    let s = require(&cli, "[ETA] site=sender", "the gauge is unreachable");
    println!("[eta-reach] sender: {s}");
    let stamped = u64_field(s, "n=");
    assert!(
        stamped > 0,
        "[ETA] site=sender n=0 — no placement was ever stamped, which is the \
         DEAD-GAUGE reading this test exists to fail on:\n{s}"
    );
    assert!(u64_field(s, "fhat_us=") > 0, "F̂ must be a real instant: {s}");

    // 6. The sender's bind gauges are present and are fractions.
    for k in ["zero=", "cold_r=", "cold_ge="] {
        let v = f64_field(s, k);
        assert!(
            v.is_none_or(|x| (0.0..=1.0).contains(&x)),
            "`{k}` must be a fraction or `-`: {s}"
        );
    }
    let zero = f64_field(s, "zero=").expect("stamped > 0 ⇒ the fraction exists");

    // 3. `zero` is the fraction of stamped placements with no prediction (a
    //    path with no cwnd yet); on a completed transfer it is a minority.
    assert!(
        zero < 1.0,
        "[ETA] site=sender zero=1.0 — every placement stamped the sentinel, so \
         the wire field is structurally dead:\n{s}"
    );

    // The receiver's side of the same claim, counted on arrivals.
    let r = require(&srv, "[ETA] site=receiver", "the gauge is unreachable");
    println!("[eta-reach] receiver: {r}");
    assert!(u64_field(r, "n=") > 0, "the receiver saw no arrival at all: {r}");
    let mut predicted_paths = 0usize;
    let mut recv_sigmas: Vec<u64> = Vec::new();
    let recv_slots = slots(r);
    for slot in &recv_slots {
        let n = u64_field(slot, "n=");
        let bind = f64_field(slot, "bind=");
        assert!(
            bind.is_none_or(|x| (0.0..=1.0).contains(&x)),
            "`bind=` must be a fraction or `-`: {slot}"
        );
        // The SRTT source is always named.
        let src = str_field(slot, "srtt_src=");
        assert!(
            ["wire", "echo", "-"].contains(&src),
            "unknown srtt source `{src}` in {slot}"
        );
        if n > 0 {
            predicted_paths += 1;
            // Quantiles are ordered; an unordered triple is a bucketing bug.
            let q: Vec<u64> = ["l_p50=", "l_p90=", "l_p95=", "l_p99=", "l_mx="]
                .iter()
                .map(|k| u64_field(slot, k))
                .collect();
            for w in q.windows(2) {
                assert!(w[0] <= w[1], "lateness quantiles out of order in {slot}");
            }
            if let (Some(v), _) = sigma(slot) {
                recv_sigmas.push(v);
            }
        }
    }
    assert!(
        predicted_paths > 0,
        "no path saw a single PREDICTED arrival — `eta_rel` never reached the \
         receiver, so the v8 field is not actually on the wire:\n{r}"
    );

    // 4. σ̂ is reachable at both ends.
    let mut send_sigmas: Vec<u64> = Vec::new();
    for slot in &slots(s) {
        if let (Some(v), pairs) = sigma(slot) {
            assert!(pairs > 0);
            send_sigmas.push(v);
        }
    }
    assert!(
        !send_sigmas.is_empty(),
        "the sender's τ-lag never found a pair on the ETA-error series — σ̂_e \
         has no producer and Track A's temperature derivation has no input:\n{s}"
    );
    assert!(
        !recv_sigmas.is_empty(),
        "the receiver's τ-lag never found a pair on the lateness series:\n{r}"
    );

    // 5. The witness, reported only: its direction on loopback is the host
    //    scheduler's.
    let sd = *send_sigmas.iter().max().expect("non-empty");
    let rc = *recv_sigmas.iter().max().expect("non-empty");
    println!(
        "[eta-reach] WITNESS σ̂_sender={sd}us σ̂_recv={rc}us — {} (pre-stated: \
         σ̂_sender ≥ σ̂_recv; loopback direction is NOT a pass/fail here)",
        if sd >= rc { "HOLDS" } else { "INVERTED" }
    );
}

/// The single-path control on a short transfer: the placement law collapses
/// to an identity but the prediction is still stamped and read, so a zero
/// reading on the dual cell cannot be blamed on the topology.
///
/// Clause 7, the exit flush: the last `[ETA] site=receiver` line carries
/// `final=1` (so a scraper taking the last line reads complete counts) and is
/// the only `final=1` line of its kind (exactly-once through whichever exit
/// reaches it). The object stays at 8 MB because the sender line rides the
/// 250 ms `[DIAG]` cadence with no exit flush; the under-cadence receiver
/// case is `the_exit_flush_fires_on_sigterm_too`. `cfg(unix)`: see
/// `run_with_signal`.
#[cfg(unix)]
#[test]
fn the_prediction_is_stamped_and_read_on_one_path_too() {
    let (cli, srv) = run(1, None, "8000000");
    let s = require(&cli, "[ETA] site=sender", "the gauge is unreachable");
    let r = require(&srv, "[ETA] site=receiver", "the gauge is unreachable");
    println!("[eta-reach] N=1 sender: {s}");
    println!("[eta-reach] N=1 receiver: {r}");
    assert!(u64_field(s, "n=") > 0, "no placement stamped on one path: {s}");
    assert_eq!(
        slots(s).len(),
        1,
        "a single-path run must report exactly one path slot: {s}"
    );
    assert_eq!(slots(r).len(), 1, "and the receiver must agree: {r}");
    assert!(u64_field(r, "n=") > 0, "no arrival observed on one path: {r}");
    assert!(
        f64_field(s, "zero=").is_some_and(|z| z < 1.0),
        "even the identity placement must carry a prediction: {s}"
    );

    // 7. The last receiver line is the flush, and the only one.
    assert!(
        is_final(r),
        "the LAST [ETA] site=receiver line of a short transfer is not the exit \
         flush (`final=1`) — the receiver block was never flushed at exit, so a \
         transfer shorter than the 1 s cadence has no reading and every longer \
         one loses its final partial second:\n{r}"
    );
    let finals = srv
        .lines()
        .filter(|l| l.contains("[ETA] site=receiver") && is_final(l))
        .count();
    assert_eq!(
        finals, 1,
        "exactly ONE final [ETA] site=receiver block is owed per receiver task; \
         the flush is not idempotent:\n{srv}"
    );
    // The whole block flushes together: its siblings carry the same marker
    // on their own last line.
    for tag in ["[SUCC] ", "[LAT] site=receiver", "[REQ] ", "[RANK] "] {
        let l = require(&srv, tag, "the gauge is unreachable");
        assert!(is_final(l), "`{tag}` was not flushed with the block: {l}");
    }
}

/// The exit flush on SIGTERM, the signal every tools/l1 harness sends
/// (`pkill -x raptorpath`). The mirror of the SIGINT case: the server exits
/// within the grace, the last `[ETA] site=receiver` line is the flush,
/// exactly once, and its siblings flush with it.
#[cfg(unix)]
#[test]
fn the_exit_flush_fires_on_sigterm_too() {
    let (_cli, srv) = run_with_signal(1, None, "1000000", "TERM");
    let r = require(&srv, "[ETA] site=receiver", "the gauge is unreachable");
    println!("[eta-reach] N=1 receiver after SIGTERM: {r}");
    assert!(u64_field(r, "n=") > 0, "no arrival observed on one path: {r}");
    assert!(
        is_final(r),
        "the LAST [ETA] site=receiver line after SIGTERM is not the exit flush \
         (`final=1`) — the harness's `pkill -x raptorpath` does not reach the \
         receiver's exit path, so no L1 ledger can carry complete counts:\n{r}"
    );
    let finals = srv
        .lines()
        .filter(|l| l.contains("[ETA] site=receiver") && is_final(l))
        .count();
    assert_eq!(
        finals, 1,
        "exactly ONE final [ETA] site=receiver block is owed per receiver task \
         on the SIGTERM path too:\n{srv}"
    );
    for tag in ["[SUCC] ", "[LAT] site=receiver", "[REQ] ", "[RANK] "] {
        let l = require(&srv, tag, "the gauge is unreachable");
        assert!(is_final(l), "`{tag}` was not flushed with the block on SIGTERM: {l}");
    }
}
