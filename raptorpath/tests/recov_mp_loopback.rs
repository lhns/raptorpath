//! Multipath recovery suppression (`RWM_RECOV_MP`) — in-process loopback
//! guard (branch `feat/recovery-suppression`).
//!
//! Runs the REAL engine end to end over a DUAL loopback path with the gate
//! set: the per-flight RFC-9002-style time-threshold hole law + per-path
//! batch serial namespaces are LIVE on the plain window-reliable pipeline.
//! Completion — the perf server acks only when EVERY byte is present and
//! reassembled — is the end-to-end proof that suppression-only recovery
//! gating never wedges a transfer (a suppressed gap is re-advertised by the
//! receiver's hole-refresh until its flight clock expires, so real holes
//! still recover; dnf 0, every byte).  In-proc loopback is lossless and
//! skew-free, so this guards the PLUMBING (law + serials live, no
//! delivered-set change); the over-emission suppression is the L1 netem
//! measurement.
//!
//! Own test binary so `RWM_RECOV_MP` (process-global env) cannot leak into
//! the other window-mode loopback tests running in parallel.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recov_mp_dual_path_reliable_completion() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_RECOV_MP", "1");

    let (s, c) = cfgs(&ports(2), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(
        srv.window_reliable,
        "recovery suppression targets the plain reliable window"
    );

    run(
        srv,
        cli,
        2_000_000,
        1,
        Duration::from_secs(90),
        "RWM_RECOV_MP dual-path loopback timed out",
    )
    .await;
    std::env::remove_var("RWM_RECOV_MP");
}
