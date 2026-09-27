//! Four-path loopback: the engine carries `N = 4` end to end, the only
//! engine-level four-path check in the tree (the quad cell's plumbing). The
//! test proves: four binds and four peers complete a real window-reliable
//! transfer; the ackdiag gauge names all four paths and every leg carried
//! (a hard-coded `pid < 2` guard would leave p2/p3 at zero and silently
//! collapse the pairwise correlations); the `RWM_ACKDIAG_WINDOW_US=250000`
//! override is resolved, echoed and exercised on a live ack stream; and the
//! transfer still completes with the gauge on at the finer cadence
//! (behaviour neutrality). In-process loopback is lossless and skew-free, so
//! nothing here measures a correlation. One test function: `RWM_ACKDIAG`
//! and `RWM_ACKDIAG_WINDOW_US` are process-global, resolved once.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_engine_carries_four_paths_and_the_gauge_names_all_four() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Both knobs before first touch. The window is set to the c9 arm's value
    // rather than the default precisely so the override is exercised on a
    // real ack stream: a resolver that no transfer ever consults is a
    // constant with extra steps.
    std::env::set_var("RWM_ACKDIAG", "1");
    std::env::set_var("RWM_ACKDIAG_WINDOW_US", "250000");

    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ackdiag, "RWM_ACKDIAG must resolve ON for this test");
    // The echo, two-sided and numeric: the ledger's cadence has to be readable
    // off the run's own output, because a 2 s ledger and a 250 ms ledger are
    // different measurands and must never be pooled.
    assert!(
        g.echo_line().contains("RWM_ACKDIAG_WINDOW_US=250000"),
        "the [GATES] echo must carry the RESOLVED window, not a flag: {}",
        g.echo_line()
    );
    assert_eq!(
        raptorpath::net::ackdiag::window_us(),
        250_000,
        "the override must resolve to the c9 arm's cadence"
    );

    let gauge = raptorpath::net::ackdiag::gauge()
        .expect("RWM_ACKDIAG=1 must construct the process-global gauge");

    // ── four binds, four peers ───────────────────────────────────────────
    let (s, c) = cfgs(&ports(4), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable);
    assert_eq!(srv.bind_addrs.len(), 4, "the server must bind the quad");

    // Long enough to cross many 250 ms windows, so the per-window report path
    // is exercised repeatedly rather than once at the end.
    run(
        srv,
        cli,
        20_000_000,
        3,
        Duration::from_secs(180),
        "four-path loopback (a timeout = the engine did not carry the quad)",
    )
    .await;

    // ── the pid<2 gate ───────────────────────────────────────────────────
    let ids = gauge.known_paths();
    for id in &ids {
        let t = gauge.totals(*id).expect("known path has totals");
        println!(
            "[quad-loopback] p{id} acks={} zero={} d_recv={} d_exp={} \
             rd_acc={} rd_rej={}",
            t.acks, t.zero_acks, t.d_recv, t.d_exp, t.rd_accepted, t.rd_rejected
        );
    }
    assert_eq!(
        ids.len(),
        4,
        "the gauge named {} path(s), not 4: {ids:?}. A quad whose gauge only \
         names p0/p1 is the SF bench's `pid < 2` truncation reproduced on the \
         wire — six pre-registered pairwise correlations would silently \
         become one and the output would still look well-formed.",
        ids.len()
    );

    // Every leg must actually have carried — a path the gauge knows about but
    // that moved nothing is a leg the scheduler opened and never used, and it
    // would enter the quad's correlation matrix as a constant series (whose
    // Pearson correlation is undefined, i.e. a silently dropped pair).
    let mut live = 0usize;
    for id in &ids {
        let t = gauge.totals(*id).expect("known path has totals");
        assert!(
            t.d_exp >= t.d_recv,
            "p{id}: Σd_expected ({}) < Σd_received ({})",
            t.d_exp,
            t.d_recv
        );
        assert!(t.zero_acks <= t.acks, "p{id}: more zero-delta acks than acks");
        if t.acks > 0 && t.d_recv > 0 && t.rd_accepted > 0 {
            live += 1;
        }
    }
    assert_eq!(
        live, 4,
        "only {live} of 4 legs recorded acks AND deltas AND accepted rate \
         samples; a leg that carries nothing contributes a constant series to \
         the quad's correlation matrix, whose pairwise correlations are \
         UNDEFINED and would be dropped without appearing in any count"
    );

    std::env::remove_var("RWM_ACKDIAG");
    std::env::remove_var("RWM_ACKDIAG_WINDOW_US");
}
