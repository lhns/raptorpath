//! The composed cap law (`RWM_COMPOSED_CAP`, an off-by-default arm; paper
//! §10) routes end to end; its arithmetic is pinned by the `three_term_*`
//! law tests. The test proves: the gate resolves the pool law (the same
//! `net::three_term_store_cap` seat `RWM_THREE_TERM` reaches) while the echo
//! still reads `RWM_THREE_TERM=0`, so the two arms are separately
//! scrapeable; no new constant reaches the late-stage brake (it caps at the
//! path's own cwnd; `RWM_INFL_CAP`/`RWM_INFL_BDP` stay 0/unset); and a
//! window-reliable loopback completes with the whole composition live — the
//! perf protocol acks only when every chunk is present, and a brake that
//! never reopened would hang. Loopback has one path, no loss and no
//! bottleneck, so the cap's interiority at c7/c8 is a bench and VM question.
//! One test function: the gates are process-global env.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composed_cap_composes_its_three_pieces_and_the_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_COMPOSED_CAP", "1");
    // The honest-anchor composition the law is linear in: the law consumes
    // a rate anchor, so the arm runs on honest inputs (default ON) plus
    // RWM_PLAIN_RS.
    std::env::set_var("RWM_PLAIN_RS", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();

    // ── 1. The gate composes what it claims ──────────────────────────────
    assert!(g.composed_cap, "RWM_COMPOSED_CAP must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_COMPOSED_CAP=1"),
        "the arm's [GATES] echo must NAME the gate with its ON value: {}",
        g.echo_line()
    );
    // Separately scrapeable from the three-term arm: the composed arm reaches
    // the same pool seat but adds the brake, and a log must tell them apart.
    assert!(
        g.echo_line().contains("RWM_THREE_TERM=0"),
        "the composed arm must not masquerade as the three-term arm: {}",
        g.echo_line()
    );
    assert!(
        g.plain_rs,
        "the honest-anchor composition must resolve ON too"
    );

    // ── 2. No new constant reached the brake ─────────────────────────────
    // The late-stage brake's per-path cap is the path's own cwnd — the
    // congestion controller's own window. Neither of the two knobs that could
    // have supplied a number instead is touched, and the echo proves it.
    assert_eq!(g.infl_cap, 0, "the composed arm must not set RWM_INFL_CAP");
    assert!(
        g.infl_bdp.is_none(),
        "the composed arm must not set RWM_INFL_BDP: {:?}",
        g.infl_bdp
    );
    assert!(
        g.echo_line().contains("RWM_INFL_CAP=0")
            && g.echo_line().contains("RWM_INFL_BDP=unset"),
        "the arm's echo must show the brake's legacy knobs at their shipped \
         values — the derived cwnd cap is not a number: {}",
        g.echo_line()
    );
    // And the pool keeps its own bounds: no pool knob moved.
    assert_eq!(g.store_path_pool, 2048, "the composed arm must not re-fit the knee");
    assert!(
        (g.store_gain - 2.0).abs() < 1e-12,
        "the composed arm must not re-fit the gain"
    );

    // ── 3. Routing, executed ─────────────────────────────────────────────
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // A brake that closed and never reopened deadlocks here rather than
    // reading oddly — `cwnd_full` gates admission and is non-exempt for
    // recovery emission, so this timeout is the composition's liveness check.
    run(
        srv,
        cli,
        2_000_000,
        2,
        Duration::from_secs(120),
        "composed-cap loopback timed out — the late-stage cwnd brake closed and did not reopen, or the pool law deadlocked the admission gate",
    )
    .await;
}
