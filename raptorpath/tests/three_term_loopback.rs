//! Three-term outstanding-limit loopback (goal-gate "Three-Term Law",
//! `RWM_THREE_TERM`): the reliable-window perf loopback with the composed
//! law ON, in the arm the pre-registration names — `RWM_THREE_TERM=1`
//! composed with `RWM_PLAIN_RS=1`, because the law is LINEAR in the rate
//! anchor and the shipped default anchor over-reads ×4.6–7.4.
//!
//! What this proves is ROUTING, not throughput (MEASUREMENT DISCIPLINE
//! rule 1: prove the mechanism under test executes). The perf object
//! protocol acks only when every chunk is present, so completion IS the
//! no-deadlock / no-loss check for a limit computed by a law that has never
//! run inside `run_window_sender` before — including its warm-up branch,
//! where a cold anchor must return `None` and let the shipped chain run.
//! Own test binary: the gate is process-global env, resolved at engine
//! start.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_term_reliable_window_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_THREE_TERM", "1");
    std::env::set_var("RWM_PLAIN_RS", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.three_term, "gate must resolve ON for this test");
    assert!(g.plain_rs, "the honest-anchor composition must resolve ON too");
    // The gate's two-sided echo, on the ARM side (the default-OFF side is
    // asserted in `gates::tests`).
    assert!(
        g.echo_line().contains("RWM_THREE_TERM=1"),
        "the arm's [GATES] echo must NAME the gate: {}",
        g.echo_line()
    );

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
        .expect("three-term loopback timed out")
        .expect("three-term perf client failed");

    srv.abort();
}
