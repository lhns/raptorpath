//! Repair and retransmit symbols enter the receiver's expected/received
//! counters — the counters whose diff (`PathState::ack_merge_counter_delta`)
//! feeds the rate sampler as `count` and the loss estimator as a batch.
//! `PathBatchTracker::record(&batch)` (keyed on the v9 per-path `path_seq`) counts an
//! arriving batch's symbols without looking at `symbol.is_repair`; this test
//! measures that on a lossy loopback using the gauge's own discriminator:
//! `crecv` = Σ`d_received` (what the tracker counted arriving) against
//! `srcack` = the cumulative WindowAck frontier (delivered source symbols
//! only). Under loss the sender puts more symbols on the wire than the source
//! contains, so repairs counted ⇒ `crecv > srcack`, repairs excluded ⇒
//! `crecv ≤ srcack`: the sign of one inequality decides it. How far above 1
//! the ratio sits at a real cell is a wire question.
//!
//! Own test binary: `RWM_ACKDIAG` and `RWM_L0_NETEM` are process-global and
//! the gauge must not be contaminated by the clean-loopback arm.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repairs_enter_the_receivers_expected_received_counters() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_ACKDIAG", "1");
    // The L0 netem shim's `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms
    // jitter, GE p = 2%/q = 40% ⇒ ε ≈ 4.8%) applied to client egress — the
    // bulk-data direction. Loss is what forces the recovery symbols this
    // reconciliation needs to exist at all.
    std::env::set_var("RWM_L0_NETEM", "c3");
    std::env::set_var("RWM_L0_SEED", "42");

    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ackdiag, "RWM_ACKDIAG must resolve ON for this test");
    let gauge = raptorpath::net::ackdiag::gauge()
        .expect("RWM_ACKDIAG=1 must construct the process-global gauge");

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    run(srv, cli, 4_000_000, 2, Duration::from_secs(180), "lossy ackdiag loopback").await;

    // Let the last acks land before reading the totals: wait (≤ 2 s) until
    // the gauge's ack count has been still for 100 ms.
    let acks = || -> u64 {
        gauge.known_paths().iter().filter_map(|id| gauge.totals(*id)).map(|t| t.acks).sum()
    };
    let mut last = acks();
    let mut still_since = std::time::Instant::now();
    loopback::wait_until(Duration::from_secs(2), || {
        let now = acks();
        if now != last {
            last = now;
            still_since = std::time::Instant::now();
        }
        still_since.elapsed() >= Duration::from_millis(100)
    });

    let mut crecv_sum = 0u64;
    let mut cexp_sum = 0u64;
    for id in gauge.known_paths() {
        let t = gauge.totals(id).expect("known path has totals");
        println!(
            "[ackdiag-recon] p{id} acks={} zero={} crecv={} cexp={} rd_acc={} rd_rej={}",
            t.acks, t.zero_acks, t.d_recv, t.d_exp, t.rd_accepted, t.rd_rejected
        );
        crecv_sum += t.d_recv;
        cexp_sum += t.d_exp;
    }
    assert!(crecv_sum > 0, "the gauge recorded no arrivals at all");

    // ── the discriminator ────────────────────────────────────────────────
    // The contemporaneous pair, both sampled at the last report: `srcack` is
    // the delivered source frontier (the same `window_ack_seq` the `[DIAG]`
    // line computes goodput from — source symbols and nothing else) and
    // `crecv_at` is Σ`d_received` at that same instant. Pairing an
    // end-of-transfer `crecv` with a mid-transfer frontier would inflate the
    // ratio and let this pass for the wrong reason.
    let (crecv_at, srcack) = gauge.last_recon();
    assert!(
        srcack > 0,
        "no [ACKDIAG] report fired — the transfer was shorter than one window \
         and the discriminator was never sampled"
    );
    println!(
        "[ackdiag-recon] contemporaneous crecv={crecv_at} srcack={srcack} \
         cr/sa={:.3} (end-of-run crecv={crecv_sum} cexp={cexp_sum})",
        crecv_at as f64 / srcack as f64
    );
    // Repairs counted ⇒ arrivals exceed the source frontier. Repairs excluded
    // ⇒ the counters would track the frontier and this fails.
    assert!(
        crecv_at > srcack,
        "REPAIRS ARE NOT IN THE COUNTERS: arrivals counted ({crecv_at}) do not \
         exceed the delivered SOURCE frontier ({srcack}) on a cell that lost \
         packets — the receiver's expected/received counters would then be a \
         source-symbol statistic, not a wire statistic, and every consumer of \
         `ack_merge_counter_delta` (the rate sampler's `count`, the loss \
         estimator's batch, the in-flight release) is reading the wrong \
         population"
    );
    // And the sender-side estimate of what it put on the path must in turn
    // exceed what arrived, once anything is lost.
    assert!(
        cexp_sum > crecv_sum,
        "on a lossy cell the tracker's expected count ({cexp_sum}) must exceed \
         its received count ({crecv_sum}) — either the shim dropped nothing \
         (the test proves nothing) or the counters are not the wire's"
    );
    // And the loss the counters imply must be in the right order for the cell
    // (c3: ε ≈ 4.8%), not an artifact: a counter stream that had lost a whole
    // symbol class (e.g. every repair) would read far higher.
    let implied_loss = 1.0 - crecv_sum as f64 / cexp_sum as f64;
    println!("[ackdiag-recon] implied loss from the counters = {implied_loss:.4}");
    assert!(
        implied_loss > 0.0 && implied_loss < 0.40,
        "implied loss {implied_loss:.4} is not a loss rate — the counters are \
         not counting the same population on both sides"
    );
}
