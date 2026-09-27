//! THE SENDER'S OWN PREDICTION REACHES THE WIRE AND COMES BACK — `[ETA]`,
//! BOTH ENDS, ON ONE RUN.
//!
//! **The measurand, and why it is owed.** The placement law already computes
//! each path's expected delivery time (`expected_delivery_load`), uses it to
//! CHOOSE, and throws it away; the receiver has never been told what the
//! sender expected and detects holes by SEQUENCE ALONE. Seed S1 of the law
//! search says those are two readings of one model, and wire v8's
//! `SymbolBatch.eta_rel_us` is the channel between them. This binary is the
//! gate that must pass before any measurement made through that channel is
//! worth taking.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **v8 IS THE VERSION ON THE WIRE.** The handshake carries it and the
//!      pair completes a transfer. A v7 binary refuses at handshake, so a
//!      completed run IS the version assertion.
//!   2. **THE SENDER LINE FIRES**, with `n > 0` stamped placements and a
//!      finite `F̂`. THE DEAD-GAUGE READING this test exists to fail on: on
//!      the shipped-before engine there is no `[ETA]` line at all, the wire
//!      field does not exist, and the quantity Track A's temperature
//!      derivation is supposed to rest on has no producer.
//!   3. **EVERY WINDOW-PATH SOURCE BATCH CARRIES A PREDICTION.** The
//!      receiver's `bind=` is the fraction of arrivals carrying the 0
//!      sentinel; a plain reliable window sends its sources through
//!      `emit_source`'s placement site and its repairs through the recovery
//!      plane, so `bind` must be STRICTLY BELOW 1 and the per-path predicted
//!      sample count strictly positive. This is the "`eta_rel > 0` on 100 %
//!      of window source batches" clause, read from the side that can
//!      actually count arrivals.
//!   4. **`σ̂` IS FINITE AT BOTH ENDS** — a value and a pair count that agree
//!      (`-` iff `n = 0`, by construction in `Tlag`), so the τ-lag estimator
//!      is reachable on the ETA series and not only on the RTT one.
//!   5. **THE PRE-STATED WITNESS**, written in `net/eta.rs`'s header before
//!      either gauge was fed: `σ̂_sender ≥ σ̂_recv`. The sender's error rides
//!      a ROUND trip, the receiver's lateness only the FORWARD leg, so the
//!      sender's dispersion contains the receiver's plus the return path's.
//!      **It is reported and NOT asserted as a pass/fail** — loopback's
//!      return leg is the host scheduler's, not a network's, and a witness
//!      whose direction is a property of the cell is a finding to be read off
//!      an L1 run rather than a gate here. What IS asserted is that BOTH
//!      numbers exist so the comparison is makeable at all.
//!   6. **THE BIND GAUGES ARE PRESENT AND ARE FRACTIONS** — `zero=`,
//!      `cold_r=`, `cold_ge=` on the sender, `bind=` on the receiver. Every
//!      clamp owes a bind-fraction gauge; an absent one is the defect.
//!
//! **What this deliberately does NOT assert.** Any particular VALUE of `σ̂`,
//! of the lateness quantiles, or of the prediction error. Loopback's queueing
//! is the host's and its loss is the shim's Gilbert-Elliott process; the
//! numbers that characterize the measurand come off an L1 run scored against
//! a pre-registration. This is the INSTRUMENT gate, not the measurement.
//!
//! **No new gate.** Both readouts ride the EXISTING `RWM_DIAG` surface — the
//! sender's beside `[DIAG]` at 250 ms, the receiver's beside `[SUCC]` at 1 s
//! — so a missing line can only be read as an unreached emission site.
//! `RWM_DIAG=1` IS asserted present in the `[GATES]` echo below.
//!
//! **The exit flush (2026-09-08).** The receiver block ALSO prints once at
//! the end of the receiver task, marked `final=1` (`net/recv_block.rs`), so a
//! transfer shorter than the cadence still has a reading. The N = 1 test
//! below is that case; it stops the server with SIGINT so the task actually
//! reaches an exit (see `run_with_signal`).
//!
//! Own test binary, for `succ_reachability.rs`'s reason: `RWM_L0_NETEM` is
//! process-global in the child and the spawned pair must not contend with the
//! in-process loopback tests.

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
///
/// The signal is named: `"INT"` is `ctrl_c`, `"TERM"` is what `pkill -x
/// raptorpath` sends from every tools/l1 harness. The engine treats the two
/// as ONE shutdown trigger (`net/mod.rs`, `shutdown_signal`), and the SIGTERM
/// test below is what proves the L1 path reaches the flush
/// (`loopback::PerfServer::stop_with`).
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

// ── READERS ─────────────────────────────────────────────────────────────

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

// ── THE RUN ─────────────────────────────────────────────────────────────

/// 1-6: BOTH GAUGES FIRE OVER A LOSSY DUAL-PATH TRANSFER, AND THE WITNESS IS
/// MAKEABLE.
#[test]
fn the_senders_prediction_reaches_the_wire_and_both_gauges_read_it() {
    // Two paths, both shaped: the placement law must actually CHOOSE for the
    // prediction to be about anything, and a single path collapses it to an
    // identity.
    let (cli, srv) = run(2, Some("c2,c3"), "24000000");

    // THE GATE. A missing `[ETA]` must read as an unreached emission site and
    // never as an unset gate.
    assert!(cli.contains("RWM_DIAG=1"), "the sender's [GATES] echo lacks RWM_DIAG=1:\n{cli}");
    assert!(srv.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{srv}");

    // 2. THE SENDER LINE FIRES. This is what fails on the shipped-before
    //    engine: no line, no wire field, no producer for the ETA series.
    let s = require(&cli, "[ETA] site=sender", "the gauge is unreachable");
    println!("[eta-reach] sender: {s}");
    let stamped = u64_field(s, "n=");
    assert!(
        stamped > 0,
        "[ETA] site=sender n=0 — no placement was ever stamped, which is the \
         DEAD-GAUGE reading this test exists to fail on:\n{s}"
    );
    assert!(u64_field(s, "fhat_us=") > 0, "F̂ must be a real instant: {s}");

    // 6. THE SENDER'S BIND GAUGES ARE PRESENT AND ARE FRACTIONS.
    for k in ["zero=", "cold_r=", "cold_ge="] {
        let v = f64_field(s, k);
        assert!(
            v.is_none_or(|x| (0.0..=1.0).contains(&x)),
            "`{k}` must be a fraction or `-`: {s}"
        );
    }
    let zero = f64_field(s, "zero=").expect("stamped > 0 ⇒ the fraction exists");

    // 3. EVERY WINDOW-PATH SOURCE BATCH CARRIES A PREDICTION. `zero` is the
    //    fraction of STAMPED placements that had no prediction — a path with
    //    no cwnd yet. On a completed transfer that must be a minority, not
    //    everything.
    assert!(
        zero < 1.0,
        "[ETA] site=sender zero=1.0 — every placement stamped the sentinel, so \
         the wire field is structurally dead:\n{s}"
    );

    // The receiver's side of the same claim, counted on ARRIVALS.
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
        // The SRTT source is always named — a reading whose reference is
        // unstated is not a reading.
        let src = str_field(slot, "srtt_src=");
        assert!(
            ["wire", "echo", "-"].contains(&src),
            "unknown srtt source `{src}` in {slot}"
        );
        if n > 0 {
            predicted_paths += 1;
            // Quantiles are ordered — an unordered triple is a bucketing bug.
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

    // 4. σ̂ IS REACHABLE AT BOTH ENDS.
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

    // 5. THE PRE-STATED WITNESS, REPORTED. Loopback's return leg is the host
    //    scheduler's, so the DIRECTION is a finding for an L1 run; what this
    //    binary owes is that both numbers exist and the comparison is
    //    makeable.
    let sd = *send_sigmas.iter().max().expect("non-empty");
    let rc = *recv_sigmas.iter().max().expect("non-empty");
    println!(
        "[eta-reach] WITNESS σ̂_sender={sd}us σ̂_recv={rc}us — {} (pre-stated: \
         σ̂_sender ≥ σ̂_recv; loopback direction is NOT a pass/fail here)",
        if sd >= rc { "HOLDS" } else { "INVERTED" }
    );
}

/// The SINGLE-PATH CONTROL, ON A SHORT TRANSFER. The placement law collapses
/// to an identity, but the prediction is still stamped and still read — so a
/// reading of zero on the dual cell could never be blamed on the topology.
///
/// 7. **THE EXIT FLUSH** (goal-gate "OPERATOR SANCTION (2026-09-08
///    ~14:00Z)"). The 8 MB object finished in ~0.4 s on the VM, the
///    receiver block's 1 s cadence never fired, and there was no
///    `[ETA] site=receiver` line at all. The receiver line must now be
///    present, carry `final=1`, be the LAST of its kind (so a scraper that
///    takes the last line reads the complete counts), and be the ONLY
///    `final=1` line of its kind (the flush is exactly-once through
///    whichever exit reaches it). Fails on the shipped-before engine twice
///    over: no line on a fast host, and no marker on any host.
///
///    THE OBJECT STAYS AT 8 MB (2026-09-08, VM verification of the merged
///    fix): the SENDER's `[ETA] site=sender` line rides the `[DIAG]` 250 ms
///    cadence (`net/diag.rs`) and has NO exit flush, so a 1 MB object
///    (0.066 s per run on the VM) ends before the sender's first tick and
///    the sender assertions above are unreachable — that variant went red
///    with `no line containing [ETA] site=sender` while every receiver
///    sibling carried `final=1`. The under-cadence RECEIVER case is pinned
///    at 1 MB by `the_exit_flush_fires_on_sigterm_too` below, which reads
///    the receiver only.
///
/// `cfg(unix)`: the server has to be stopped with SIGINT for its receiver to
/// reach an exit path at all — see `run_with_signal`.
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

    // 7. THE EXIT FLUSH. The last receiver line IS the flush, and it is the
    //    only one.
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

/// **THE EXIT FLUSH ON SIGTERM — THE L1 PATH.** Every tools/l1 harness stops
/// the server with `pkill -x raptorpath`, which is SIGTERM; the SIGINT test
/// above proves the flush exists but not that L1 can see it. Before the
/// engine handled SIGTERM this was an abrupt kill: no shutdown broadcast, no
/// Drop, no `final=1` — the flush landed on main and STILL never reached a
/// ledger. The mirror of the SIGINT case, stopped with `kill -TERM`: the
/// server must exit within the grace, the last `[ETA] site=receiver` line
/// must be the flush, exactly once, and its siblings flush with it.
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
