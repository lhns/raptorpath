//! FMTCP-class pure decode-on-total aggregation — in-process loopback guard.
//!
//! Runs the REAL engine end to end with the composite `RWM_FMTCP` gate set,
//! over a DUAL loopback path. RWM_FMTCP self-selects the systematic-repair
//! generation submode and forces the four FMTCP levers (total-in-flight flow
//! control, per-path BDP in-flight cap, fungible cross-path fountain repair with
//! NO per-hole ARQ, decode-on-total out-of-order delivery). Completion — the
//! perf server acks only when EVERY byte is present and reassembled — is the
//! end-to-end proof that the composite mode never wedges and delivers reliably
//! (dnf 0, every byte). In-proc loopback is lossless, so it guards the PLUMBING
//! (total-in-flight FC pipelines multiple generations, the deficit-frontier
//! advances out of order, no per-seq ARQ path is taken); the LOSSY cross-path
//! aggregation win (C8 > 15.7 Mbit/s) is the decisive L1 netem measurement.
//!
//! This lives in its OWN test binary so `RWM_FMTCP=1` (a process-global env var)
//! cannot leak into the other window-mode loopback tests running in parallel.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fmtcp_loopback_dual_path_reliable_completion() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // The composite gate. Isolated to this test binary's process (edition 2021,
    // set_var is safe); it activates only with window_reliable, so it is inert
    // for any non-window code paths in this process.
    std::env::set_var("RWM_FMTCP", "1");

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47861".into(), "127.0.0.1:47862".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    // RWM_FMTCP self-selects the systematic-repair generation submode.
    assert!(
        srv_pc.window_reliable,
        "FMTCP composes on the reliable window"
    );
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into(), "127.0.0.1:0".into()]),
        peer: Some(vec![
            "127.0.0.1:47861".into(),
            "127.0.0.1:47862".into(),
        ]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // 2 MB → several generations at the default G=384. Completion is the
    // reliability invariant: every byte decoded-on-total, reassembled by offset,
    // acked — proving the total-in-flight FC + fungible fountain + OOO delivery
    // compose without wedging or losing data.
    tokio::time::timeout(Duration::from_secs(90), perf::client(cli_pc, 2_000_000, 1))
        .await
        .expect("FMTCP dual-path loopback timed out")
        .expect("FMTCP dual-path perf client failed");

    srv.abort();
    std::env::remove_var("RWM_FMTCP");
}
