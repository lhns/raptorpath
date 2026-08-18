//! THE `×N` DELETION and THE EXTRACTED LATE-STAGE BRAKE, routed
//! (`RWM_SUM_CAP` + `RWM_LATE_BRAKE`, paper §16.60 / §16.60.1, ADR-0070
//! findings 2 and 7).
//!
//! The arithmetic of both is pinned elsewhere and is not repeated here:
//! `sum_store_cap_value_is_linear_in_n_the_template_applied` (the law's shape
//! over N = 1…8, clamps neutralised and asserted inert),
//! `the_correction_makes_the_pin_threshold_per_path_instead_of_path_count_free`
//! (each clamp on its own), `sum_cap_and_the_unified_set_are_independent_axes_
//! of_one_law` (the composition as a factorisation), and
//! `published_pooled_cap_equals_the_engine_pooled_cap_on_both_arms` (agreement
//! with the paper). What none of them can show is that the gates ROUTE — and
//! ADR-0070's whole postmortem is about instruments that could not see the
//! property under test. MEASUREMENT DISCIPLINE rule 1: prove the mechanism
//! under test executes.
//!
//! Asserted in the order they can fail:
//!
//!   1. **THE GATES RESOLVE AND ARE SEPARATELY SCRAPEABLE.** `RWM_SUM_CAP=1`
//!      and `RWM_LATE_BRAKE=1` must appear with their ON values in the
//!      `[GATES]` echo, and — the part that matters — `RWM_COMPOSED_CAP` and
//!      `RWM_THREE_TERM` must both still read `0`. The extraction exists
//!      precisely so the brake can be armed WITHOUT the composed pool law that
//!      §16.57 refuted on magnitude; if this arm could not be told from the
//!      composed arm in a log, the extraction would have bought nothing.
//!   2. **NO NEW CONSTANT REACHED EITHER MECHANISM.** The brake's per-path cap
//!      is the path's OWN cwnd and the corrected law is the shipped expression
//!      minus one factor, so `RWM_INFL_CAP`, `RWM_INFL_BDP`, the knee, the gain
//!      and the boot cap must all read their shipped values in this arm's own
//!      echo. If either mechanism had needed a number, it would be visible
//!      here.
//!   3. **ROUTING, executed.** A window-reliable loopback completes with both
//!      live. This is a real liveness check rather than a formality: the brake
//!      gates admission AND is non-exempt for recovery emission, so a brake
//!      that closed and never reopened HANGS rather than merely reading oddly;
//!      and the corrected law returns a strictly SMALLER cap than the shipped
//!      one wherever it is interior, so a deletion that under-provisioned the
//!      pool would stall the sender here rather than in a battery.
//!
//! **What only the VM can validate**, stated so this binary is not mistaken
//! for the arm's evidence: loopback has one path, so `n_live < 2` and the
//! pooled law is not even engaged — the single-path chain runs, which is
//! exactly the "bit-identical at N = 1 by construction" claim §16.60 makes,
//! and asserting the loopback still completes is the check that the claim did
//! not break the seat it does not touch. The multipath INTERIORITY the arm
//! exists for (c7 4096 → 3271, c8 4096 → 3020) is a bench and VM question.
//!
//! Own test binary and ONE test function: the gates are process-global env,
//! resolved once at engine start.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sum_cap_and_late_brake_route_without_the_composed_law_and_the_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_SUM_CAP", "1");
    std::env::set_var("RWM_LATE_BRAKE", "1");
    // The honest-anchor composition the pooled law consumes (§16.51, default
    // ON) plus the send-interval sampler, matching the composed arm's setup so
    // the two loopbacks differ only in the mechanism under test.
    std::env::set_var("RWM_PLAIN_RS", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();

    // ── 1. THE GATES RESOLVE, AND ARE SEPARATELY SCRAPEABLE ──────────────
    assert!(g.sum_cap, "RWM_SUM_CAP must resolve ON for this test");
    assert!(g.late_brake, "RWM_LATE_BRAKE must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_SUM_CAP=1") && g.echo_line().contains("RWM_LATE_BRAKE=1"),
        "the arm's [GATES] echo must NAME both gates with their ON values: {}",
        g.echo_line()
    );
    // THE EXTRACTION'S WHOLE POINT: the brake is armed and the composed pool
    // law is NOT. Before `RWM_LATE_BRAKE` this combination was not expressible
    // — the only door to the cwnd brake also forced `three_term_on`.
    assert!(
        !g.composed_cap && !g.three_term,
        "the brake must be armed WITHOUT the composed/three-term pool law — \
         that combination is the reason the gate exists"
    );
    assert!(
        g.echo_line().contains("RWM_COMPOSED_CAP=0")
            && g.echo_line().contains("RWM_THREE_TERM=0"),
        "this arm must not masquerade as the composed or three-term arm: {}",
        g.echo_line()
    );
    // The Σ-set stays the shipped one: `RWM_SUM_CAP` changes the MULTIPLIER,
    // and the SET is an independent dial (`RWM_STORE_CAP_UNIFIED`). An arm that
    // moved both without saying so is the confound §16.53 fired a STOP RULE on.
    assert!(
        !g.store_cap_unified && g.echo_line().contains("RWM_STORE_CAP_UNIFIED=0"),
        "the ×N deletion must not silently carry the live set too: {}",
        g.echo_line()
    );

    // ── 2. NO NEW CONSTANT REACHED EITHER MECHANISM ──────────────────────
    // The brake's per-path cap is the path's own cwnd; neither knob that could
    // have supplied a number instead is touched.
    assert_eq!(g.infl_cap, 0, "this arm must not set RWM_INFL_CAP");
    assert!(
        g.infl_bdp.is_none(),
        "this arm must not set RWM_INFL_BDP: {:?}",
        g.infl_bdp
    );
    assert!(
        g.echo_line().contains("RWM_INFL_CAP=0") && g.echo_line().contains("RWM_INFL_BDP=unset"),
        "the echo must show the brake's legacy knobs at their shipped values — \
         the derived cwnd cap is not a number: {}",
        g.echo_line()
    );
    // The corrected law is the shipped expression MINUS ONE FACTOR: every other
    // symbol must be untouched, so that gain's fossil status and the knee's
    // staleness are identical on both arms and cancel out of the comparison
    // rather than confounding it (§16.60's provenance table).
    assert_eq!(g.store_path_pool, 2048, "the ×N deletion must not re-fit the knee");
    assert!(
        (g.store_gain - 2.0).abs() < 1e-12,
        "the ×N deletion must not re-fit the gain"
    );
    assert_eq!(
        g.store_boot, 128,
        "the ×N deletion must not carry the boot cap's derived value — §16.61 \
         records that as DERIVED and NOT SHIPPED, blocked on the cliff"
    );
    assert!(
        g.echo_line().contains("RWM_STORE_GAIN=2")
            && g.echo_line().contains("RWM_STORE_PATH_POOL=2048")
            && g.echo_line().contains("RWM_STORE_BOOT=128"),
        "the echo must carry the untouched pool constants: {}",
        g.echo_line()
    );

    // ── 3. ROUTING, executed ─────────────────────────────────────────────
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47893".into()]),
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
        peer: Some(vec!["127.0.0.1:47893".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // A brake that closed and never reopened deadlocks here rather than reading
    // oddly — `cwnd_full` gates admission and is NON-EXEMPT for recovery
    // emission — and a pool the deletion under-provisioned stalls the sender.
    // So this timeout is the composition's liveness check, not a formality.
    tokio::time::timeout(
        Duration::from_secs(120),
        perf::client(cli_pc, 2_000_000, 2),
    )
    .await
    .expect(
        "sum-cap/late-brake loopback timed out — the extracted cwnd brake closed \
         and did not reopen, or the corrected pool law starved the admission gate",
    )
    .expect("sum-cap perf client failed");

    srv.abort();
}
