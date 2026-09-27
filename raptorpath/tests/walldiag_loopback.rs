//! The dead-wall onset/duration gauge (`RWM_WALLDIAG`) executes on a real
//! transfer (`docs/measurement-discipline.md` rule 1; its arithmetic is
//! unit-tested in `net::walldiag`). The test proves: the gate resolves on and
//! is echoed, and after a loopback transfer the gauge has been fed (non-zero
//! iterations, a real wall-clock span), so the single feed site in
//! `run_window_sender` executed; the clean-run reading sits at the zero end
//! of the scale (terminal window a small fraction of the run, onset near
//! 1.0) — the calibration a lossy cell is read against; and the transfer
//! completes with the gauge on (behaviour neutrality, executed). Loopback has
//! no loss, delay or bottleneck, so resolving a real wall is a VM question.
//! One test function: `RWM_WALLDIAG` is process-global, resolved once.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, start_server};
use raptorpath::perf;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn walldiag_gauge_is_wired_reads_clean_at_loopback_and_is_behaviour_neutral() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_WALLDIAG", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.walldiag, "RWM_WALLDIAG must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_WALLDIAG=1"),
        "the gate's liveness echo must carry the ON value: {}",
        g.echo_line()
    );
    let gauge = raptorpath::net::walldiag::gauge()
        .expect("RWM_WALLDIAG=1 must construct the process-global gauge");

    // ── the transfer ─────────────────────────────────────────────────────
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv_pc, cli_pc) = (resolve(&s), resolve(&c));
    assert!(srv_pc.window_reliable);
    // The server stays up until the gauge has been read (it is aborted at
    // the end).
    let srv = start_server(srv_pc, "walldiag loopback").await;

    // Behaviour neutrality, executed: an observation-only instrument cannot
    // stall a transfer.
    tokio::time::timeout(
        Duration::from_secs(120),
        perf::client(cli_pc, 20_000_000, 3),
    )
    .await
    .expect("walldiag loopback timed out — the gauge is not observation-only")
    .expect("walldiag perf client failed");

    // ── routing ──────────────────────────────────────────────────────────
    // Read the gauge without the teardown clock: `report` takes the caller's
    // end stamp, and the sender's own teardown may not have run yet (the
    // server task is still alive). `max(last_us)` inside `report` makes the
    // reading well-defined from here.
    let r = gauge
        .report(0)
        .expect("the gauge must have been fed — the sender-loop feed site never ran");
    println!("[walldiag-loopback] {}", raptorpath::net::walldiag::report_line(r));

    assert!(
        r.total_ms > 100.0,
        "the run's span reads {} ms — the gauge was fed once and never again",
        r.total_ms
    );
    assert!(
        r.it_ms > 0.0 && r.it_ms < 100.0,
        "the sender-loop iteration period reads {} ms, which is not a loop",
        r.it_ms
    );
    assert!(
        (0.0..=1.0).contains(&r.onset),
        "onset must be a fraction of the transfer wall: {}",
        r.onset
    );

    // ── the clean-run reading ────────────────────────────────────────────
    // Loopback is the zero end of the scale: no loss, no propagation, no
    // bottleneck. The sender is productive essentially to the end, so the
    // terminal window is a small fraction of the run and the onset is late.
    // The bound is deliberately loose (10 %) — this pins the class, and the
    // class is what a c8 reading is compared against. A lossy cell reading
    // inside this bound would mean the cell has no wall.
    assert!(
        r.duration_ms <= 0.10 * r.total_ms,
        "a lossless loopback reported a terminal wall of {} ms in a {} ms run \
         ({:.1} %) — either the run really stalled or `productive(t)` is wrong",
        r.duration_ms,
        r.total_ms,
        100.0 * r.duration_ms / r.total_ms
    );
    assert!(
        r.onset >= 0.90,
        "a lossless loopback's last productive instant is at {:.4} of the run",
        r.onset
    );

    srv.abort();
}
