//! Threading P1, D2 → Q2: no quinn call is made while the scheduler is held.
//!
//! Through Q1 this was a debug-build lock-order witness: the scheduler's
//! mutex guard counted itself per thread and every quinn seam checked the
//! count. Threading Q2 removed the mutex — the scheduler's TX half is owned
//! by the window sender (plain `&mut`, `net::tx_inputs::TxCore`), its RX
//! half by the receiver — so "held across a quinn call" has no lock to be
//! about; the rule now holds by construction twice over: (1) no task but
//! the path's I/O owner calls quinn (the owner identity check at every seam,
//! `transport::io_owner::OwnedConn`), and (2) the scheduler sits behind no
//! lock at all (the source scan
//! `net::tests::q2_the_scheduler_halves_sit_behind_no_mutex`).
//!
//! This test keeps the whole-engine drive the witness used — client and
//! server in-process, two lossy paths, `RWM_DIAG=1`, a reliable bulk
//! transfer long enough for the 2 s report tick, the hole re-advertisement
//! timer and the DIAG cadence — and reads the surviving witness, the owner
//! identity check:
//!
//!   * `checks > 0`: the check ran (MEASUREMENT DISCIPLINE rule 1 — a zero
//!     violation count from a dead instrument proves nothing);
//!   * `violations == 0`: every quinn call ran inside its path's owner.
//!
//! Debug and release alike (the identity check is not compiled away).
//! Own test binary: `RWM_L0_NETEM` / `RWM_DIAG` are process-global.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, start_server};
use raptorpath::transport::io_owner::{first_identity_violation, identity_counts};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_quinn_call_is_made_outside_its_owner_under_load() {
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

    let (checks, violations) = identity_counts();
    println!("[lockorder] identity checks={checks} violations={violations}");
    assert!(checks > 0, "no quinn seam was checked — the witness is not wired");
    assert_eq!(
        violations,
        0,
        "a quinn call was made outside its path's owner: {}",
        first_identity_violation().unwrap_or_default()
    );
    match res {
        Err(_) => panic!("lock-order loopback: perf transfer timed out"),
        Ok(Err(e)) => panic!("lock-order loopback: perf client failed: {e:#}"),
        Ok(Ok(_)) => {}
    }
}
