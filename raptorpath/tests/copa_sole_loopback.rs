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

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_reliable_window_copa_sole_passthrough() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Engine-owned substrate window (per path) + the plain-mode Copa feed it
    // implies. Read at transport creation / engine start below.
    std::env::set_var("RWM_QUIC_CC", "passthrough");

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    run(
        srv,
        cli,
        200_000,
        2,
        Duration::from_secs(60),
        "copa-sole passthrough loopback timed out",
    )
    .await;
}
