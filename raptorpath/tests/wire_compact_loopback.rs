//! Compact-wire-framing loopback (goal-gate "Window Decoupling + MTU
//! Scaling" part 2, `RWM_WIRE_COMPACT`): the reliable-window perf loopback
//! with compact DATA framing ON — every window-mode symbol datagram rides
//! the v5 tag+varint frame end-to-end. The perf object protocol acks only
//! when every chunk is present, so completion IS the codec-correctness
//! check on live traffic (source + repair + retransmits). Own test binary:
//! the gate is process-global env, resolved once.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire_compact_reliable_window_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_WIRE_COMPACT", "1");
    assert!(
        raptorpath::transport::wire_compact_active(),
        "gate must resolve ON for this test"
    );

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));

    run(
        srv,
        cli,
        200_000,
        2,
        Duration::from_secs(60),
        "wire-compact loopback timed out",
    )
    .await;
}
