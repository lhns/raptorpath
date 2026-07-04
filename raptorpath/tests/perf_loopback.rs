//! In-process loopback test for the rp-native perf mode: a perf server
//! and perf client, each running the REAL engine over a memory TUN,
//! exchange a small object over 127.0.0.1 (real QUIC, no kernel TUN,
//! no routes/DNS). Guards the run_with_tun seam and the perf object
//! protocol end to end.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_small_object() {
    // quinn needs an installed crypto provider (main() does this for the
    // binary; tests must do it themselves).
    let _ = rustls::crypto::ring::default_provider().install_default();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47831".into()]),
        protocol_hint: Some("bulk".into()),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    // Let the server engine bind + start accepting before connecting.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47831".into()]),
        protocol_hint: Some("bulk".into()),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // The client bails if the warm-up object is never acked and only
    // returns Ok after every run completed or timed out; bounding the
    // whole thing well under the 300 s run timeout means Ok == the
    // object round-tripped (chunks delivered, reassembled, acked).
    tokio::time::timeout(
        Duration::from_secs(60),
        perf::client(cli_pc, 200_000, 2),
    )
    .await
    .expect("perf loopback timed out")
    .expect("perf client failed");

    srv.abort();
}
