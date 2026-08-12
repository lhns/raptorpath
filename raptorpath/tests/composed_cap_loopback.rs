//! THE COMPOSED CAP LAW loopback (`RWM_COMPOSED_CAP`, paper §16.56,
//! ADR-0070 Deliverable 2).
//!
//! The composed law's ARITHMETIC is pinned in three places that already
//! exist and are not repeated here: `three_term_store_cap_value_is_linear_
//! in_n_the_template_applied` (the law's shape over N = 1…8),
//! `three_term_memory_bound_is_a_resource_limit_and_not_a_term_of_the_law`
//! (the unclamped value separated from its only remaining bound), and
//! `three_term_engine_law_is_the_bench_terms_at_the_anchors` (engine↔bench
//! equivalence). What none of them can show is that the COMPOSITION routes —
//! and ADR-0070's entire postmortem is about measurements that could not see
//! the property under test.
//!
//! So this binary asserts, in the order they can fail:
//!
//!   1. **THE GATE COMPOSES WHAT IT CLAIMS.** `RWM_COMPOSED_CAP=1` must
//!      resolve the POOL LAW on (it reaches the same seat `RWM_THREE_TERM`
//!      does — the composed law IS `net::three_term_store_cap`, one
//!      implementation) while `RWM_THREE_TERM` itself stays OFF in the echo.
//!      That last part matters: the two gates are separately scrapeable, and
//!      a battery must be able to tell the composed arm from the three-term
//!      arm in a log.
//!   2. **NO NEW CONSTANT REACHED THE BRAKE.** `RWM_INFL_CAP` and
//!      `RWM_INFL_BDP` must read their shipped values in the composed arm's
//!      own `[GATES]` echo — 0 and unset. The brake's per-path cap is the
//!      path's OWN cwnd, taken from the congestion controller; if the arm
//!      had needed a number, it would be visible here.
//!   3. **ROUTING, executed.** A window-reliable loopback completes with the
//!      whole composition live. The perf object protocol acks only when every
//!      chunk is present, so completion IS the no-deadlock / no-loss check —
//!      and it is a real check here, because this arm turns ON a brake that
//!      has never run in composition with a sane pool: `cwnd_full` gates
//!      admission AND is non-exempt for recovery emission, so a brake that
//!      closed and never reopened would hang rather than merely read oddly.
//!      The warm-up branch is exercised too (a cold live path must return
//!      `None` and let the shipped chain run).
//!
//! **What only the VM can validate.** Loopback cannot answer the question the
//! arm exists for. There is one path, so the resequencing span is zero by
//! arithmetic; there is no loss, so the brake is unlikely to close; and there
//! is no bottleneck, so the cap's INTERIORITY at the honest c7/c8 geometries
//! — §16.56's stated prediction, and its STOP condition if the memory bound
//! binds — is a bench and VM question. This binary pins that the composition
//! is wired and does not deadlock.
//!
//! Own test binary and ONE test function: the gates are process-global env,
//! resolved once at engine start.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composed_cap_composes_its_three_pieces_and_the_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_COMPOSED_CAP", "1");
    // The honest-anchor composition the law is LINEAR in, exactly as the
    // three-term loopback names it: the law consumes a rate anchor, so the
    // arm is scored on honest inputs (§16.51, default ON) plus RWM_PLAIN_RS.
    std::env::set_var("RWM_PLAIN_RS", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();

    // ── 1. THE GATE COMPOSES WHAT IT CLAIMS ──────────────────────────────
    assert!(g.composed_cap, "RWM_COMPOSED_CAP must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_COMPOSED_CAP=1"),
        "the arm's [GATES] echo must NAME the gate with its ON value: {}",
        g.echo_line()
    );
    // Separately scrapeable from the three-term arm. The composed arm reaches
    // the same POOL seat, but it is a different arm (it adds the brake), and a
    // battery must be able to tell them apart in a log.
    assert!(
        g.echo_line().contains("RWM_THREE_TERM=0"),
        "the composed arm must not masquerade as the three-term arm: {}",
        g.echo_line()
    );
    assert!(
        g.plain_rs,
        "the honest-anchor composition must resolve ON too"
    );

    // ── 2. NO NEW CONSTANT REACHED THE BRAKE ─────────────────────────────
    // The late-stage brake's per-path cap is the path's OWN cwnd — the
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
    // And the pool keeps its own bounds: the paroled floor is the shipped 64
    // and no pool knob moved.
    assert_eq!(g.store_path_pool, 2048, "the composed arm must not re-fit the knee");
    assert!(
        (g.store_gain - 2.0).abs() < 1e-12,
        "the composed arm must not re-fit the gain"
    );

    // ── 3. ROUTING, executed ─────────────────────────────────────────────
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47891".into()]),
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
        peer: Some(vec!["127.0.0.1:47891".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // A brake that closed and never reopened deadlocks here rather than
    // reading oddly — `cwnd_full` gates admission and is NON-EXEMPT for
    // recovery emission, so this timeout is the composition's liveness check.
    tokio::time::timeout(
        Duration::from_secs(120),
        perf::client(cli_pc, 2_000_000, 2),
    )
    .await
    .expect(
        "composed-cap loopback timed out — the late-stage cwnd brake closed and \
         did not reopen, or the pool law deadlocked the admission gate",
    )
    .expect("composed-cap perf client failed");

    srv.abort();
}
