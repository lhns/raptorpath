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

/// RWM Phase A: the same loopback exchange over the RELIABLE sliding-window
/// pipeline (`window_reliable`, bulk hint → windowed RLC). Guards the
/// retention path end to end: the sent-data store fills and drains on real
/// peer WindowAcks (removal by ack only), the receiver's reliable reorder
/// buffer delivers in order, and completion still happens — i.e. the policy
/// plumbing itself never wedges a clean link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_reliable_window() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47833".into()]),
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
        peer: Some(vec!["127.0.0.1:47833".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(
        Duration::from_secs(60),
        perf::client(cli_pc, 200_000, 2),
    )
    .await
    .expect("reliable-window perf loopback timed out")
    .expect("reliable-window perf client failed");

    srv.abort();
}

/// RWM Phase C (paper §16.2, H→∞ corner): the reliable-window loopback with
/// OUT-OF-ORDER object delivery (`window_out_of_order`). The receiver hands
/// each decoded symbol to the consumer the instant it decodes (bypassing the
/// in-order frontier) and the sender's retention backpressure is relaxed;
/// the perf server reassembles by offset and acks on total-decoded. Guards
/// the Phase C plumbing end to end: it must complete (every chunk delivered
/// and reassembled — the object protocol only acks when st.got.len() ==
/// total, so completion IS the all-bytes-present check) without wedging.
/// The LOSSY exercise of the same path is the L1 C8 measurement (real GE
/// loss on the netem harness), where holes are recovered by NACK/retransmit
/// under retention and the object still completes with all bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_out_of_order_object() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47835".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        window_out_of_order: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable && srv_pc.window_out_of_order);
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47835".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        window_out_of_order: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // A larger object (spans many windows, so out-of-order delivery is
    // actually exercised across window boundaries) still completes.
    tokio::time::timeout(
        Duration::from_secs(60),
        perf::client(cli_pc, 1_000_000, 2),
    )
    .await
    .expect("out-of-order perf loopback timed out")
    .expect("out-of-order perf client failed");

    srv.abort();
}

/// Fungible frontier (paper §16.3 "empty quadrant"): the reliable-window
/// loopback in CODED-ONLY mode (`window_coded_only`). The sender emits ONLY
/// coded (random-linear-combination) symbols over the window — no raw
/// systematic source on the wire during normal flow — and the receiver
/// reconstructs every source seq by Gaussian elimination and delivers it
/// out-of-order (reassemble by offset). Guards the coded-object path end to
/// end: with NO systematic passthrough the object must still complete with
/// all bytes (the perf server only acks when st.got.len() == total, so
/// completion IS the all-bytes-present, decode-on-K check). The LOSSY
/// exercise is the L1 C8 measurement (real GE loss on the netem harness),
/// where the fungible window aggregates across the two paths.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_coded_object() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47837".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        window_coded_only: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable && srv_pc.window_coded_only);
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47837".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        window_coded_only: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // A multi-window object: coded-only must reconstruct every seq purely by
    // GE (no systematic passthrough) across window boundaries and complete.
    tokio::time::timeout(
        Duration::from_secs(60),
        perf::client(cli_pc, 1_000_000, 2),
    )
    .await
    .expect("coded-object perf loopback timed out")
    .expect("coded-object perf client failed");

    srv.abort();
}
