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

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

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

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    run(
        srv,
        cli,
        200_000,
        2,
        Duration::from_secs(60),
        "three-term loopback timed out",
    )
    .await;
}
