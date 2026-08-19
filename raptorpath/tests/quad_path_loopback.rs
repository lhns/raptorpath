//! FOUR-PATH loopback — the local half of the c9 quad cell's validation
//! (goal-gate "Eppen's Condition at c8" §4; ERA LEDGER item 5).
//!
//! WHY THIS BINARY EXISTS. The c9 pre-registration is written against an
//! `N = 4` geometry, and before this file **no four-path measurement of any
//! kind existed in the tree**. The only `N = 4` coverage was
//! `store_cap_sf_bench.rs`'s `c7x4` — a SIMULATED quad with no kernel, no
//! QUIC and no real scheduler — and every engine-level loopback in the repo
//! ran one path or two. So the c9 harness would otherwise have reached the
//! VM never having run the engine at four paths at all, and a failure there
//! would be indistinguishable from a failure of the cell.
//!
//! WHAT IT PINS, and each one is a prerequisite the c9 battery's verdicts
//! rest on rather than a property worth knowing for its own sake:
//!
//!   1. **THE ENGINE CARRIES FOUR PATHS END TO END.** Four binds, four peers,
//!      a real window-reliable transfer that completes. `config::resolve`
//!      takes vectors, but "takes a vector" and "works at four" are different
//!      claims and only the second one is load-bearing here.
//!   2. **THE GAUGE NAMES FOUR PATHS.** `known_paths().len() == 4`. This is
//!      THE `pid < 2` GATE, on the wire instead of in the bench. The SF bench
//!      raised `MAX_PATHS` from 2 to 4 for `c7x4` and left three per-path
//!      gauge writes guarded by a hard-coded `pid < 2`, so `delivered_p[2..4]`
//!      read 0 no matter what placement did and the assertion built on them
//!      COULD NOT FAIL. A quad ledger whose gauge only ever names p0 and p1
//!      is that defect exactly: six pre-registered pairwise correlations
//!      would silently become one, and the output would look well-formed.
//!   3. **THE 250 ms WINDOW OVERRIDE IS LIVE ON A REAL TRANSFER.**
//!      `RWM_ACKDIAG_WINDOW_US=250000` is a BLOCKING dependency of C9-1..4 —
//!      six correlations cannot be carried by the four windows per rep the
//!      shipped 2 s cadence yields. Here the override is resolved, echoed and
//!      exercised against an actual ack stream, not just unit-tested.
//!   4. **BEHAVIOUR NEUTRALITY SURVIVES THE WIDENING.** The transfer
//!      completes with the gauge on at four paths and an 8x finer report
//!      cadence — i.e. ~32x the report volume of the arm the neutrality claim
//!      was originally measured on.
//!
//! WHAT IT DOES NOT PIN, stated so no verdict is read off it. In-process
//! loopback is lossless, skew-free and shares one host clock: there is no
//! netem, no Gilbert-Elliott chain, no per-leg seed and no bandwidth
//! asymmetry. **Nothing here measures a correlation, and no C9 clause is
//! scored by this file.** It proves the plumbing the wire measurement needs;
//! the measurement itself is the VM launch step.
//!
//! Own test binary and ONE test function: `RWM_ACKDIAG` and
//! `RWM_ACKDIAG_WINDOW_US` are process-global `OnceLock`s, resolved once at
//! first touch and never re-read.

use std::time::Duration;

use raptorpath::{config, perf};

/// The quad's loopback ports. Distinct from every other loopback binary's
/// range (47831-47931, 47991-47992 are taken) so the suite stays parallel-safe.
const PORTS: [u16; 4] = [47951, 47952, 47953, 47954];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_engine_carries_four_paths_and_the_gauge_names_all_four() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // BOTH knobs before first touch. The window is set to the c9 arm's value
    // rather than the default precisely so the override is exercised on a
    // real ack stream: a resolver that no transfer ever consults is a
    // constant with extra steps.
    std::env::set_var("RWM_ACKDIAG", "1");
    std::env::set_var("RWM_ACKDIAG_WINDOW_US", "250000");

    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ackdiag, "RWM_ACKDIAG must resolve ON for this test");
    // THE ECHO, two-sided and NUMERIC: the ledger's cadence has to be readable
    // off the run's own output, because a 2 s ledger and a 250 ms ledger are
    // different measurands and must never be pooled.
    assert!(
        g.echo_line().contains("RWM_ACKDIAG_WINDOW_US=250000"),
        "the [GATES] echo must carry the RESOLVED window, not a flag: {}",
        g.echo_line()
    );
    assert_eq!(
        raptorpath::net::ackdiag::window_us(),
        250_000,
        "the override must resolve to the c9 arm's cadence"
    );

    let gauge = raptorpath::net::ackdiag::gauge()
        .expect("RWM_ACKDIAG=1 must construct the process-global gauge");

    // ── four binds, four peers ───────────────────────────────────────────
    let binds: Vec<String> = PORTS.iter().map(|p| format!("127.0.0.1:{p}")).collect();
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(binds.clone()),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable);
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into(); 4]),
        peer: Some(binds),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // Long enough to cross MANY 250 ms windows, so the per-window report path
    // is exercised repeatedly rather than once at the end.
    tokio::time::timeout(
        Duration::from_secs(180),
        perf::client(cli_pc, 20_000_000, 3),
    )
    .await
    .expect("four-path loopback timed out — the engine did not carry the quad")
    .expect("four-path perf client failed");

    srv.abort();

    // ── THE pid<2 GATE ───────────────────────────────────────────────────
    let ids = gauge.known_paths();
    for id in &ids {
        let t = gauge.totals(*id).expect("known path has totals");
        println!(
            "[quad-loopback] p{id} acks={} zero={} d_recv={} d_exp={} \
             rd_acc={} rd_rej={}",
            t.acks, t.zero_acks, t.d_recv, t.d_exp, t.rd_accepted, t.rd_rejected
        );
    }
    assert_eq!(
        ids.len(),
        4,
        "the gauge named {} path(s), not 4: {ids:?}. A quad whose gauge only \
         names p0/p1 is the SF bench's `pid < 2` truncation reproduced on the \
         wire — six pre-registered pairwise correlations would silently \
         become one and the output would still look well-formed.",
        ids.len()
    );

    // Every leg must actually have CARRIED — a path the gauge knows about but
    // that moved nothing is a leg the scheduler opened and never used, and it
    // would enter the quad's correlation matrix as a constant series (whose
    // Pearson correlation is undefined, i.e. a silently dropped pair).
    let mut live = 0usize;
    for id in &ids {
        let t = gauge.totals(*id).expect("known path has totals");
        assert!(
            t.d_exp >= t.d_recv,
            "p{id}: Σd_expected ({}) < Σd_received ({})",
            t.d_exp,
            t.d_recv
        );
        assert!(t.zero_acks <= t.acks, "p{id}: more zero-delta acks than acks");
        if t.acks > 0 && t.d_recv > 0 && t.rd_accepted > 0 {
            live += 1;
        }
    }
    assert_eq!(
        live, 4,
        "only {live} of 4 legs recorded acks AND deltas AND accepted rate \
         samples; a leg that carries nothing contributes a constant series to \
         the quad's correlation matrix, whose pairwise correlations are \
         UNDEFINED and would be dropped without appearing in any count"
    );

    std::env::remove_var("RWM_ACKDIAG");
    std::env::remove_var("RWM_ACKDIAG_WINDOW_US");
}
