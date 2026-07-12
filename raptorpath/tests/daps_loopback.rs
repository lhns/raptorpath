//! DAPS delay-aware scheduling — in-process loopback guard.
//!
//! Runs the REAL engine end to end with the `RWM_DAPS` gate set, over a DUAL
//! loopback path.  RWM_DAPS composes on the FMTCP total-in-flight base and adds
//! delay-aware (DAPS) source placement — the slow path carries FUTURE-offset
//! stream data (Sarwar et al., WAINA 2013; Kuhn et al., IEEE ICC 2014) under an
//! ECF completion-time guard (Lim et al., CoNEXT 2017) — plus the right-sized
//! §8.4 r* and a deep read-ahead / reassembly window.  Completion — the perf
//! server acks only when EVERY byte is present and reassembled by offset — is
//! the end-to-end proof that the DAPS placement gate never wedges and delivers
//! reliably (dnf 0, every byte).  In-proc loopback is lossless (RTprop skew ≈ 0
//! ⇒ Δ_j ≈ 0 ⇒ DAPS reduces to the FMTCP base here), so it guards the PLUMBING;
//! the heterogeneous-skew aggregation win is the L1 netem measurement.
//!
//! Own test binary so `RWM_DAPS=1` (process-global) cannot leak into the other
//! window-mode loopback tests running in parallel.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daps_loopback_dual_path_reliable_completion() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_DAPS", "1");

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47871".into(), "127.0.0.1:47872".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable, "DAPS composes on the reliable window");
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into(), "127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47871".into(), "127.0.0.1:47872".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(Duration::from_secs(90), perf::client(cli_pc, 2_000_000, 1))
        .await
        .expect("DAPS dual-path loopback timed out")
        .expect("DAPS dual-path perf client failed");

    srv.abort();
    std::env::remove_var("RWM_DAPS");
}
