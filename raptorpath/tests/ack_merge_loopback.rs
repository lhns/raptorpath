//! The ack merge (`RWM_ACK_MERGE`, paper §9.5) carries the whole delivery
//! accounting. Under the merge
//! the receiver stops sending the per-batch `ControlMessage::Ack` in window
//! mode and the sender re-homes every consumer of it — including the
//! in-flight release — onto the diff of the cumulative
//! `cum_expected`/`cum_received` counters. The perf object protocol acks only
//! when every chunk is present, so completion is both the delivered-set check
//! and the re-homing's liveness proof: a merge that lost counts would stall.
//! (The block-mode scope guard went with the block pipeline, ADR-0069.)

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ack_merge_window_loopback() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_ACK_MERGE", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ack_merge, "gate must resolve ON for this test");
    assert!(
        raptorpath::scheduler::ack_merge_active(),
        "the cached resolution the receiver and sender arms both read must agree"
    );

    // The merged path carries the whole accounting.
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // Completion == every chunk delivered, reassembled and acked with one
    // control datagram per data message instead of two (2 runs + warm-up).
    run(
        srv,
        cli,
        200_000,
        2,
        Duration::from_secs(60),
        "ack-merge window loopback (a stall = the re-homed accounting stalled)",
    )
    .await;
}
