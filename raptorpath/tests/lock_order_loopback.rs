//! Threading P1, D2: no quinn call is made while the scheduler is held.
//!
//! `scheduler::sched_lock` counts live scheduler guards per thread (debug
//! builds) and every quinn-touching seam of `transport::quic` checks the
//! count first (`assert_not_held`). This test drives the whole engine
//! in-process — client and server, two lossy paths, `RWM_DIAG=1`, a reliable
//! bulk transfer long enough for the 2 s report tick, the hole
//! re-advertisement timer and the DIAG cadence — and then reads the witness:
//!
//!   * `checks > 0` and `acquires > 0`: the witness ran (MEASUREMENT
//!     DISCIPLINE rule 1 — a zero violation count from a dead instrument
//!     proves nothing);
//!   * `violations == 0`: no seam was reached with a guard alive.
//!
//! Red on the tree before the D2 fix: the sender's WindowStart broadcast
//! (`for pid in control_broadcast_paths(&sched) { send_control_datagram }`)
//! is the first of several sites that trip it.
//!
//! Debug only: release builds compile the witness away (every count is 0).
//! Own test binary: `RWM_L0_NETEM` / `RWM_DIAG` are process-global.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, start_server};
use raptorpath::scheduler::sched_lock::{first_violation, witness_counts};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg_attr(not(debug_assertions), ignore = "the lock-order witness exists in debug builds only")]
async fn no_quinn_call_is_made_while_the_scheduler_is_held() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Two c3 legs (20 Mbit, 20 ms, GE ε ≈ 4.8 %) on the client's egress:
    // loss makes holes, holes arm the receiver's re-advertisement timer
    // (the broadcast WindowAck sites) and the sender's repair paths.
    std::env::set_var("RWM_L0_NETEM", "c3,c3");
    std::env::set_var("RWM_L0_SEED", "42");
    std::env::set_var("RWM_DIAG", "1");

    let (s, c) = cfgs(&ports(2), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    let task = start_server(srv, "lock-order loopback").await;
    // 3 MB × 2 runs over 2 × 20 Mbit: several seconds, past one 2 s report
    // tick. A violation panics an engine task, so the transfer may not
    // finish: the witness is read first, the transfer result after.
    let res = tokio::time::timeout(
        Duration::from_secs(90),
        raptorpath::perf::client(cli, 3_000_000, 2),
    )
    .await;
    task.abort();

    let (checks, violations, acquires) = witness_counts();
    println!("[lockorder] checks={checks} violations={violations} acquires={acquires}");
    assert!(acquires > 0, "the scheduler guard counter never ran");
    assert!(checks > 0, "no quinn seam was checked — the witness is not wired");
    assert_eq!(
        violations,
        0,
        "a quinn call was made with the scheduler held: {}",
        first_violation().unwrap_or_default()
    );
    match res {
        Err(_) => panic!("lock-order loopback: perf transfer timed out"),
        Ok(Err(e)) => panic!("lock-order loopback: perf client failed: {e:#}"),
        Ok(Ok(_)) => {}
    }
}
