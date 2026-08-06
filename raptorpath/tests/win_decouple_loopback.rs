//! Window/inflight-decoupling loopback (goal-gate "Window Decoupling + MTU
//! Scaling" part 1, `RWM_WIN_DECOUPLE`): the reliable-window perf loopback
//! with the decoupled gate ON. The perf object protocol acks only when
//! every chunk is present, so completion IS the no-deadlock / no-loss check
//! for the head-span gate + stall meter + retention backstop (and for the
//! N1-scoped sampling anchor underneath). Own test binary: the gate is
//! process-global env, resolved at engine start.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn win_decouple_reliable_window_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_WIN_DECOUPLE", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.win_decouple, "gate must resolve ON for this test");

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47857".into()]),
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
        peer: Some(vec!["127.0.0.1:47857".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // Completion == every chunk delivered through the decoupled admission
    // gate (2 runs + warm-up object; loopback rates keep the anchor warm
    // so the law engages, and warm-up covers the legacy fallback branch).
    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("win-decouple loopback timed out")
        .expect("win-decouple perf client failed");

    srv.abort();
}
