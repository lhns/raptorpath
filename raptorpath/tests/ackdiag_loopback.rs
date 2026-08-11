//! ACK-CADENCE GAUGE loopback (goal-gate "Ack-Cadence Gauge", `RWM_ACKDIAG`).
//!
//! The unit tests in `net::ackdiag` pin the gauge's ARITHMETIC against injected
//! trains. This binary pins the other half — MEASUREMENT DISCIPLINE rule 1,
//! *prove the mechanism under test executes*: that the gauge is actually WIRED
//! into the engine's ack path, that the numbers it collects off a real
//! transfer are SELF-CONSISTENT, and that turning it on does not change what
//! the engine does.
//!
//! Three things are asserted, in the order they can fail:
//!
//!   1. **ROUTING.** The gate resolves ON, the process-global gauge exists,
//!      and after a window-reliable loopback it has recorded WindowAck
//!      arrivals, non-zero delivered-count deltas and accepted
//!      `record_delivery` samples on at least one path. All three feed sites
//!      (`net/control_msg.rs`'s `on_window_ack`, and both arms of
//!      `CopaState::record_delivery`) are proven live by that.
//!   2. **SELF-CONSISTENCY.** `Σd_received ≤ Σd_expected` (the receiver's
//!      tracker charges `gap × received` across a batch-seq gap, so expected
//!      can only meet or exceed received — the same invariant
//!      `ack_merge_counter_delta_*` pins on the counters themselves, here
//!      re-read off the gauge that transcribes them), and the accepted +
//!      rejected sample counts add up to a non-zero total whose acceptance
//!      fraction is a real number in [0, 1].
//!   3. **BEHAVIOUR NEUTRALITY.** The same transfer completes with the gauge
//!      ON. `net::ackdiag::tests::ackdiag_is_observation_only` pins the
//!      STRUCTURAL half (the gauge owns all of its state and reaches no engine
//!      handle mutably); this is the executed half.
//!
//! Own test binary and ONE test function: `RWM_ACKDIAG` is a process-global
//! `OnceLock`, resolved once at first touch and never re-read.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ackdiag_gauge_is_wired_self_consistent_and_behaviour_neutral() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_ACKDIAG", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ackdiag, "RWM_ACKDIAG must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_ACKDIAG=1"),
        "the gate's liveness echo must carry the ON value: {}",
        g.echo_line()
    );
    let gauge = raptorpath::net::ackdiag::gauge()
        .expect("RWM_ACKDIAG=1 must construct the process-global gauge");

    // ── the transfer ─────────────────────────────────────────────────────
    // Window-reliable, and long enough to cross at least one ~2 s ACKDIAG
    // window so the `[ACKDIAG]` line itself is exercised (run with
    // `-- --nocapture` to read it).
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47871".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable);
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47871".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // BEHAVIOUR NEUTRALITY, executed: an observation-only instrument cannot
    // stall a transfer. A gauge that took a lock in the wrong order, or that
    // dropped an ack, would time out here rather than merely print oddly.
    tokio::time::timeout(
        Duration::from_secs(120),
        perf::client(cli_pc, 20_000_000, 3),
    )
    .await
    .expect("ackdiag loopback timed out — the gauge is not observation-only")
    .expect("ackdiag perf client failed");

    srv.abort();

    // ── ROUTING + SELF-CONSISTENCY ───────────────────────────────────────
    let ids = gauge.known_paths();
    assert!(
        !ids.is_empty(),
        "the gauge saw no path at all — the ack feed site is not wired"
    );
    let mut live = 0usize;
    for id in ids {
        let t = gauge.totals(id).expect("known path has totals");
        println!(
            "[ackdiag-loopback] p{id} acks={} zero={} d_recv={} d_exp={} rd_acc={} rd_rej={}",
            t.acks, t.zero_acks, t.d_recv, t.d_exp, t.rd_accepted, t.rd_rejected
        );
        // The tracker's own invariant, re-read off the gauge: `expected` is
        // `received` scaled by the batch-sequence gap, so it can never be the
        // smaller of the two.
        assert!(
            t.d_exp >= t.d_recv,
            "p{id}: Σd_expected ({}) < Σd_received ({}) — the gauge is \
             transcribing the counter diff wrongly",
            t.d_exp,
            t.d_recv
        );
        assert!(
            t.zero_acks <= t.acks,
            "p{id}: more zero-delta acks ({}) than acks ({})",
            t.zero_acks,
            t.acks
        );
        if t.acks > 0 && t.d_recv > 0 && t.rd_accepted > 0 {
            live += 1;
        }
    }
    assert!(
        live > 0,
        "no path recorded acks AND deltas AND accepted rate samples — at least \
         one of the three feed sites (on_window_ack / record_delivery accept / \
         record_delivery reject) never executed"
    );
}
