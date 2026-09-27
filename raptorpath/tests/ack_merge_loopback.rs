//! ack-merge loopback (goal-gate "Unlock The Default 1: ack-merge",
//! `RWM_ACK_MERGE`): the reliable-window perf loopback with the WINDOW-mode
//! control-datagram merge ON, plus the BLOCK-mode scope guard in the same
//! process.
//!
//! Why this is the right gate for this build. Under the merge the receiver
//! stops sending the legacy per-batch `ControlMessage::Ack` in window mode
//! and the sender re-homes EVERY consumer of that arm onto the diff of the
//! v6 cumulative `cum_expected`/`cum_received` counters — including the
//! in-flight release, without which the Copa/flow-control gate simply jams
//! and the transfer never completes. The perf object protocol acks only when
//! every chunk is present (`st.got.len() == total`), so COMPLETION IS the
//! delivered-set check and, because the store releases only on that
//! accounting, it is simultaneously the re-homing's liveness proof. A merge
//! that lost counts would stall here, not merely run slower.
//!
//! The second transfer is the SCOPE guard: block mode keeps the legacy `Ack`
//! in full (it has no `WindowAck` to merge into, and `block_arq` — whose
//! dup-ack `LATER_ACK_LOSS_THRESHOLD` channel is built on that message — is
//! live only there). It must be unaffected by the gate. Asserted rather than
//! reasoned about, per the pre-registration.
//!
//! Own test binary and ONE test function: the gate is process-global env
//! resolved once at engine start, and the two transfers must not race for
//! ports.

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

    // ── 1. WINDOW mode: the merged path carries the whole accounting ─────
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // Completion == every chunk delivered, reassembled and acked with ONE
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

    // ── 2. BLOCK mode: out of scope, must be untouched ───────────────────
    // A fresh port pair, so the first pair's teardown is not waited on.
    let (s, c) = cfgs(&ports(1), "bulk", false);
    let (bsrv, bcli) = (resolve(&s), resolve(&c));
    assert!(
        !bsrv.window_reliable,
        "this transfer must take the BLOCK path — the scope guard is vacuous otherwise"
    );

    // Block mode still runs its per-batch Ack → BlockArq loss channel. With
    // the gate ON this must be exactly as it is with the gate OFF.
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
