//! Compact-wire-framing loopback (goal-gate "Window Decoupling + MTU
//! Scaling" part 2, `RWM_WIRE_COMPACT`): the reliable-window perf loopback
//! with compact DATA framing ON — every window-mode symbol datagram rides
//! the v5 tag+varint frame end-to-end. The perf object protocol acks only
//! when every chunk is present, so completion IS the codec-correctness
//! check on live traffic (source + repair + retransmits). Own test binary:
//! the gate is process-global env, resolved once.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire_compact_reliable_window_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_WIRE_COMPACT", "1");
    assert!(
        raptorpath::transport::wire_compact_active(),
        "gate must resolve ON for this test"
    );

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47858".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47858".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("wire-compact loopback timed out")
        .expect("wire-compact perf client failed");

    srv.abort();
}
