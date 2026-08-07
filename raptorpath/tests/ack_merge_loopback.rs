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

use std::time::Duration;

use raptorpath::{config, perf};

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
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47861".into()]),
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
        peer: Some(vec!["127.0.0.1:47861".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // Completion == every chunk delivered, reassembled and acked with ONE
    // control datagram per data message instead of two (2 runs + warm-up).
    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("ack-merge window loopback timed out — the re-homed accounting stalled")
        .expect("ack-merge window perf client failed");

    srv.abort();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // ── 2. BLOCK mode: out of scope, must be untouched ───────────────────
    let bsrv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47862".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(false),
        ..Default::default()
    };
    let (bsrv_pc, _) = config::resolve(&bsrv_cfg).unwrap();
    assert!(
        !bsrv_pc.window_reliable,
        "this transfer must take the BLOCK path — the scope guard is vacuous otherwise"
    );
    let bsrv = tokio::spawn(perf::server(bsrv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let bcli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47862".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(false),
        ..Default::default()
    };
    let (bcli_pc, _) = config::resolve(&bcli_cfg).unwrap();

    // Block mode still runs its per-batch Ack → BlockArq loss channel. With
    // the gate ON this must be exactly as it is with the gate OFF.
    tokio::time::timeout(Duration::from_secs(60), perf::client(bcli_pc, 200_000, 2))
        .await
        .expect("block-mode transfer timed out under RWM_ACK_MERGE — scope defect")
        .expect("block-mode perf client failed under RWM_ACK_MERGE — scope defect");

    bsrv.abort();
}
