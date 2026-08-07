//! Derived-patience loopback (goal-gate "Unlock The Default 2: derived
//! patience", `RWM_PATIENCE_DERIVED` + `RWM_SIDLE_DERIVED`).
//!
//! MEASUREMENT DISCIPLINE item 1 applied to the LAWS rather than to prose:
//! the reliable-window perf loopback with BOTH gates on, so the derived
//! recovery-patience floor actually governs the RFC 9002 §6.1.2 threshold
//! and the per-seq retransmit cooldown on a live transfer, end to end. The
//! perf object protocol acks only when every chunk is present, so completion
//! IS the delivered-set contract: a patience floor that fires too eagerly
//! (spurious retransmit storms) or too late (holes never served) shows up
//! here as a failed or timed-out transfer, not as a silent statistic.
//!
//! Own test binary: both gates are process-global env, resolved once at
//! engine start via `OnceLock`.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn derived_patience_reliable_window_loopback_delivers_everything() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Both gates ON. `RWM_SIDLE_DERIVED` rides along because the battery
    // runs it on every arm and the two must be proven to compose; it is
    // DIAG-only, so with `RWM_DIAG` unset it stays inert here as well.
    std::env::set_var("RWM_PATIENCE_DERIVED", "1");
    std::env::set_var("RWM_SIDLE_DERIVED", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.patience_derived, "gate must resolve ON for this test");
    assert!(g.sidle_derived, "gauge gate must resolve ON for this test");

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

    // Completion == every chunk delivered, reassembled and acked with the
    // derived floor governing both recovery clocks (2 runs + warm-up).
    tokio::time::timeout(Duration::from_secs(60), perf::client(cli_pc, 200_000, 2))
        .await
        .expect("derived-patience loopback timed out")
        .expect("derived-patience perf client failed");

    srv.abort();
}
