//! The `×N` deletion (`RWM_SUM_CAP`, paper §6.1) and the late-stage cwnd
//! brake (`RWM_LATE_BRAKE`) route end to end (`docs/measurement-discipline.md`
//! rule 1; the law arithmetic is pinned in the store-cap benches and
//! `formula_agreement`). The test proves: both gates resolve and are echoed
//! while `RWM_COMPOSED_CAP`/`RWM_THREE_TERM` stay `0`, so the brake is
//! scrapeably armed without the composed pool law; no new constant reaches
//! either mechanism (the brake caps at the path's own cwnd, the pool keeps
//! its shipped knee/gain/boot); and a window-reliable loopback completes —
//! a real liveness check, since a brake that never reopened would hang.
//! Loopback has one path, so the pooled law is not engaged; this checks that
//! the N = 1 chain is untouched, and the multipath effect is a bench and VM
//! question. One test function: the gates are process-global env.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sum_cap_and_late_brake_route_without_the_composed_law_and_the_loopback_completes() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_SUM_CAP", "1");
    std::env::set_var("RWM_LATE_BRAKE", "1");
    // The honest-anchor composition the pooled law consumes (default ON)
    // plus the send-interval sampler, matching the composed arm's setup so
    // the two loopbacks differ only in the mechanism under test.
    std::env::set_var("RWM_PLAIN_RS", "1");
    let g = raptorpath::gates::RuntimeGates::resolve();

    // ── 1. The gates resolve, and are separately scrapeable ──────────────
    assert!(g.sum_cap, "RWM_SUM_CAP must resolve ON for this test");
    assert!(g.late_brake, "RWM_LATE_BRAKE must resolve ON for this test");
    assert!(
        g.echo_line().contains("RWM_SUM_CAP=1") && g.echo_line().contains("RWM_LATE_BRAKE=1"),
        "the arm's [GATES] echo must NAME both gates with their ON values: {}",
        g.echo_line()
    );
    // The brake is armed and the composed pool law is not — the combination
    // `RWM_LATE_BRAKE` exists to express.
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
    // and the SET is an independent dial (`RWM_STORE_CAP_UNIFIED`); an arm that
    // moved both without saying so would confound the two.
    assert!(
        !g.store_cap_unified && g.echo_line().contains("RWM_STORE_CAP_UNIFIED=0"),
        "the ×N deletion must not silently carry the live set too: {}",
        g.echo_line()
    );

    // ── 2. No new constant reached either mechanism ──────────────────────
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
    // The corrected law is the old expression minus one factor: every other
    // symbol must be untouched, so the gain and knee are identical on both
    // arms and cancel out of the comparison rather than confounding it.
    assert_eq!(g.store_path_pool, 2048, "the ×N deletion must not re-fit the knee");
    assert!(
        (g.store_gain - 2.0).abs() < 1e-12,
        "the ×N deletion must not re-fit the gain"
    );
    assert_eq!(
        g.store_boot, 128,
        "the ×N deletion must not carry the boot cap's derived value — paper §6.2 \
         records that as DERIVED and NOT SHIPPED, blocked on the cliff"
    );
    assert!(
        g.echo_line().contains("RWM_STORE_GAIN=2")
            && g.echo_line().contains("RWM_STORE_PATH_POOL=2048")
            && g.echo_line().contains("RWM_STORE_BOOT=128"),
        "the echo must carry the untouched pool constants: {}",
        g.echo_line()
    );

    // ── 3. Routing, executed ─────────────────────────────────────────────
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);

    // A brake that closed and never reopened deadlocks here rather than reading
    // oddly — `cwnd_full` gates admission and is non-exempt for recovery
    // emission — and a pool the deletion under-provisioned stalls the sender.
    // So this timeout is the composition's liveness check, not a formality.
    run(
        srv,
        cli,
        2_000_000,
        2,
        Duration::from_secs(120),
        "sum-cap/late-brake loopback timed out — the extracted cwnd brake closed and did not reopen, or the corrected pool law starved the admission gate",
    )
    .await;
}
