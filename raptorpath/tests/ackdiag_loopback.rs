//! The ack-cadence gauge (`RWM_ACKDIAG`) is wired into the engine's ack path
//! (`docs/measurement-discipline.md` rule 1; its arithmetic is unit-tested in
//! `net::ackdiag`). The test proves: after a window-reliable loopback the
//! gauge has recorded WindowAck arrivals, delivered-count deltas and accepted
//! `record_delivery` samples, so all three feed sites (`on_window_ack` in
//! `net/control_msg.rs` and both arms of `CopaState::record_delivery`) are
//! live; its numbers are self-consistent (`Σd_received ≤ Σd_expected`, since
//! the tracker charges `gap × received` across a batch-seq gap, and the
//! acceptance fraction lies in [0, 1]); and the transfer completes with the
//! gauge on (behaviour neutrality, executed). One test function:
//! `RWM_ACKDIAG` is process-global, resolved once.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

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
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // Behaviour neutrality, executed: an observation-only instrument cannot
    // stall a transfer. A gauge that took a lock in the wrong order, or that
    // dropped an ack, would time out here rather than merely print oddly.
    run(
        srv,
        cli,
        20_000_000,
        3,
        Duration::from_secs(120),
        "ackdiag loopback (a timeout = the gauge is not observation-only)",
    )
    .await;

    // ── routing + self-consistency ───────────────────────────────────────
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
