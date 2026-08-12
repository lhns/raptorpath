//! DEAD-WALL ONSET/DURATION loopback (`RWM_WALLDIAG`, ADR-0070 validation
//! path step 2).
//!
//! The unit tests in `net::walldiag` pin the gauge's ARITHMETIC against
//! injected instants — a clean run, a terminal wall, scattered gaps, and the
//! cap-paused case — so this binary pins the other half, MEASUREMENT
//! DISCIPLINE rule 1: **prove the mechanism under test executes.**
//!
//! Three things are asserted, in the order they can fail:
//!
//!   1. **ROUTING.** The gate resolves ON, its `[GATES]` echo carries the ON
//!      value, and the process-global gauge exists. After a loopback transfer
//!      the gauge has been FED — a non-zero iteration count and a real
//!      wall-clock span — which proves the single feed site in
//!      `run_window_sender` executed with the three scalars it consumes
//!      (`wait_arm`, `SenderState::last_source_send_us`, the retransmit
//!      counter).
//!   2. **THE CLEAN-RUN READING.** A loopback over 127.0.0.1 is the
//!      lossless, latency-free end of the instrument's range, so the sender
//!      is productive essentially up to teardown: the terminal window must be
//!      a small fraction of the transfer and its onset must sit near 1.0.
//!      This is the calibration the c8 arms are read against — a cell whose
//!      wall reads like loopback has no wall.
//!   3. **BEHAVIOUR NEUTRALITY.** The same transfer completes with the gauge
//!      ON. `net::walldiag::tests::walldiag_is_observation_only` pins the
//!      STRUCTURAL half (the gauge owns all its state and takes no engine
//!      handle at all); this is the executed half.
//!
//! **What only the VM can validate**, stated here so the loopback is not
//! over-read: loopback cannot produce a c3-class lossy tail, because there is
//! no loss, no propagation delay and no bottleneck queue — so the number this
//! binary pins is the ZERO end of the scale, not the instrument's ability to
//! resolve a real wall. The lossy end (a measurable terminal window with
//! retransmits inside it) needs netem, and the STABILITY claim that motivated
//! the instrument — that onset/duration does not invert between pools minutes
//! apart, where the tick-share statistic did — is a repeated-measures claim
//! that can only be scored on the VM battery.
//!
//! Own test binary and ONE test function: `RWM_WALLDIAG` is a process-global
//! `OnceLock`, resolved once at first touch and never re-read.

use std::time::Duration;

use raptorpath::{config, perf};

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
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47881".into()]),
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
        peer: Some(vec!["127.0.0.1:47881".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // BEHAVIOUR NEUTRALITY, executed: an observation-only instrument cannot
    // stall a transfer.
    tokio::time::timeout(
        Duration::from_secs(120),
        perf::client(cli_pc, 20_000_000, 3),
    )
    .await
    .expect("walldiag loopback timed out — the gauge is not observation-only")
    .expect("walldiag perf client failed");

    // ── ROUTING ──────────────────────────────────────────────────────────
    // Read the gauge WITHOUT the teardown clock: `report` takes the caller's
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

    // ── THE CLEAN-RUN READING ────────────────────────────────────────────
    // Loopback is the zero end of the scale: no loss, no propagation, no
    // bottleneck. The sender is productive essentially to the end, so the
    // terminal window is a small fraction of the run and the onset is late.
    // The bound is deliberately loose (10 %) — this pins the CLASS, and the
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
