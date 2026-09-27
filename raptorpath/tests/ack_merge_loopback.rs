//! The window-mode ack merge (`RWM_ACK_MERGE`, paper §9.5) carries the whole
//! delivery accounting, and block mode is untouched by it. Under the merge
//! the receiver stops sending the per-batch `ControlMessage::Ack` in window
//! mode and the sender re-homes every consumer of it — including the
//! in-flight release — onto the diff of the cumulative
//! `cum_expected`/`cum_received` counters. The perf object protocol acks only
//! when every chunk is present, so completion is both the delivered-set check
//! and the re-homing's liveness proof: a merge that lost counts would stall.
//! The second transfer is the scope guard: block mode keeps the per-batch
//! `Ack` (its `block_arq` dup-ack channel is built on it) and must run
//! unchanged with the gate on. One test function: the gate is process-global
//! and the two transfers must not race for ports.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ack_merge_window_loopback_and_block_mode_scope() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_ACK_MERGE", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ack_merge, "gate must resolve ON for this test");
    assert!(
        raptorpath::scheduler::ack_merge_active(),
        "the cached resolution the receiver and sender arms both read must agree"
    );

    // ── 1. Window mode: the merged path carries the whole accounting ─────
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

    // ── 2. Block mode: out of scope, must be untouched ───────────────────
    // A fresh port pair, so the first pair's teardown is not waited on.
    let (s, c) = cfgs(&ports(1), "bulk", false);
    let (bsrv, bcli) = (resolve(&s), resolve(&c));
    assert!(
        !bsrv.window_reliable,
        "this transfer must take the BLOCK path — the scope guard is vacuous otherwise"
    );

    // Block mode still runs its per-batch Ack → BlockArq loss channel. With
    // the gate on this must be exactly as it is with the gate off.
    run(
        bsrv,
        bcli,
        200_000,
        2,
        Duration::from_secs(60),
        "block-mode transfer under RWM_ACK_MERGE (scope defect)",
    )
    .await;

}
