//! Emission-batching loopback (goal-gate "Emission Batching",
//! `RWM_EMIT_BATCH`): the reliable-window perf loopback with pacer-quantum
//! burst intake ON and a deliberately SMALL burst quantum, so the transfer
//! crosses many burst boundaries. The perf object protocol acks only when
//! every chunk is present (`st.got.len() == total`), so completion IS the
//! no-symbol-loss-at-burst-boundaries check, and the reliable in-order
//! pipeline underneath is the ordering contract. Own test binary: the gate
//! is process-global env, resolved at engine start.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn emit_batch_reliable_window_loopback_small_burst() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Gate ON with a small burst quantum: 200 KB / ~1.2 KB symbols ≈ 170
    // symbols ≈ 20+ burst boundaries at burst=8. Both engines (client bulk
    // sender AND the server's reverse sender) resolve the gate.
    std::env::set_var("RWM_EMIT_BATCH", "1");
    std::env::set_var("RWM_EMIT_BURST", "8");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.emit_batch, "gate must resolve ON for this test");
    assert_eq!(g.emit_burst, 8);

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47851".into()]),
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
        peer: Some(vec!["127.0.0.1:47851".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // Completion == every chunk delivered, reassembled and acked through
    // the batched emission path (2 runs + warm-up object).
    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("emit-batch loopback timed out")
        .expect("emit-batch perf client failed");

    srv.abort();
}
