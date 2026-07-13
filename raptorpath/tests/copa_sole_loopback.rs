//! feat/copa-sole-cc end-to-end loopback guard: the PLAIN reliable-window
//! perf exchange with `RWM_QUIC_CC=passthrough` — quinn's congestion window
//! is the pass-through shim fed by OUR per-path Copa-lite cwnd (which the
//! plain-mode WindowAck delivery feed drives). Guards, over real QUIC on
//! 127.0.0.1:
//!   - the handshake is not starved by the shim (connection establishes),
//!   - the plain-mode Copa feed + cwnd writes never wedge the transfer
//!     (objects complete), i.e. Copa-sole substrate ownership is live end to
//!     end.
//!
//! This file contains exactly ONE test on purpose: it sets a process-global
//! env var, and integration-test files compile to their own binary, so a
//! single test here cannot race other tests' env reads.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_reliable_window_copa_sole_passthrough() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Engine-owned substrate window (per path) + the plain-mode Copa feed it
    // implies. Read at transport creation / engine start below.
    std::env::set_var("RWM_QUIC_CC", "passthrough");

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

    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("copa-sole passthrough loopback timed out")
        .expect("copa-sole passthrough perf client failed");

    srv.abort();
}
