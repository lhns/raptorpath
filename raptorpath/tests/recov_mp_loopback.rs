//! Multipath recovery suppression (`RWM_RECOV_MP`) — in-process loopback
//! guard (branch `feat/recovery-suppression`).
//!
//! Runs the REAL engine end to end over a DUAL loopback path with the gate
//! set: the per-flight RFC-9002-style time-threshold hole law + per-path
//! batch serial namespaces are LIVE on the plain window-reliable pipeline.
//! Completion — the perf server acks only when EVERY byte is present and
//! reassembled — is the end-to-end proof that suppression-only recovery
//! gating never wedges a transfer (a suppressed gap is re-advertised by the
//! receiver's hole-refresh until its flight clock expires, so real holes
//! still recover; dnf 0, every byte).  In-proc loopback is lossless and
//! skew-free, so this guards the PLUMBING (law + serials live, no
//! delivered-set change); the over-emission suppression is the L1 netem
//! measurement.
//!
//! Own test binary so `RWM_RECOV_MP` (process-global env) cannot leak into
//! the other window-mode loopback tests running in parallel.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recov_mp_dual_path_reliable_completion() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_RECOV_MP", "1");

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47881".into(), "127.0.0.1:47882".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(
        srv_pc.window_reliable,
        "recovery suppression targets the plain reliable window"
    );
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into(), "127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47881".into(), "127.0.0.1:47882".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(Duration::from_secs(90), perf::client(cli_pc, 2_000_000, 1))
        .await
        .expect("RWM_RECOV_MP dual-path loopback timed out")
        .expect("RWM_RECOV_MP dual-path perf client failed");

    srv.abort();
    std::env::remove_var("RWM_RECOV_MP");
}
