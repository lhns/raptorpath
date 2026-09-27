//! Emission-batching loopback (goal-gate "Emission Batching",
//! `RWM_EMIT_BATCH`): the reliable-window perf loopback with pacer-quantum
//! burst intake ON and a deliberately SMALL burst quantum, so the transfer
//! crosses many burst boundaries. The perf object protocol acks only when
//! every chunk is present (`st.got.len() == total`), so completion IS the
//! no-symbol-loss-at-burst-boundaries check, and the reliable in-order
//! pipeline underneath is the ordering contract. Own test binary: the gate
//! is process-global env, resolved at engine start.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn emit_batch_reliable_window_loopback_small_burst() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Gate ON with a small burst quantum: 200 KB / ~1.2 KB symbols ≈ 170
    // symbols ≈ 20+ burst boundaries at burst=8. Both engines (client bulk
    // sender AND the server's reverse sender) resolve the gate.
    std::env::set_var("RWM_EMIT_BATCH", "1");
    std::env::set_var("RWM_EMIT_BURST", "8");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.emit_batch, "gate must resolve ON for this test");
    assert_eq!(g.emit_burst, 8);

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // Completion == every chunk delivered, reassembled and acked through
    // the batched emission path (2 runs + warm-up object).
    run(
        srv,
        cli,
        200_000,
        2,
        Duration::from_secs(60),
        "emit-batch loopback timed out",
    )
    .await;
}
