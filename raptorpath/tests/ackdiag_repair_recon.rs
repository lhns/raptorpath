//! THE REPAIR-COUNTING RECONCILIATION (goal-gate "Ack-Cadence Gauge",
//! readout 4) — measured on a LOSSY loopback, not read off the source.
//!
//! The question the "SF Accounting Axis" worker flagged: do repair and
//! retransmit symbols enter the RECEIVER's expected/received counters — the
//! very counters whose diff (`PathState::ack_merge_counter_delta`) is fed to
//! the rate sampler as `count` and to the loss estimator as a batch?
//!
//! The READ says yes: the counters come from
//! `PathBatchTracker::record_batch(batch_seq, batch.symbols.len())`
//! (`net/mod.rs`, fed at `receiver.rs`), which counts the symbols of an
//! arriving batch and never looks at `symbol.is_repair`. But this repo's
//! standing rule is that a documented model-vs-engine claim carries a test
//! that BOUNDS it rather than prose that describes it, so this binary
//! MEASURES it, using the gauge's own discriminator:
//!
//!   * `crecv` = Σ`d_received` — what the receiver's tracker counted arriving.
//!   * `srcack` = the cumulative WindowAck frontier — DELIVERED SOURCE
//!     symbols, and source symbols only.
//!
//! Under loss the sender must put strictly more symbols on the wire than the
//! source stream contains (retransmits, and the taper's repair). So:
//!
//!   * repairs COUNTED  ⇒ `crecv > srcack` — the counters hold arrivals the
//!     source frontier does not.
//!   * repairs EXCLUDED ⇒ `crecv ≤ srcack` — the counters would track the
//!     source frontier, short by every recovery symbol.
//!
//! The two hypotheses are separated by the SIGN of one measured inequality,
//! which is why this can be decided at loopback at all. The MAGNITUDE (how
//! far above 1 `cr/sa` sits at a real cell) is a wire question, because the
//! recovery volume is set by the channel — see the ledger section's "what only
//! the VM can answer".
//!
//! Own test binary: `RWM_ACKDIAG` and `RWM_L0_NETEM` are process-global and
//! the gauge must not be contaminated by the clean-loopback arm.

use std::time::Duration;

use raptorpath::{config, perf};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repairs_enter_the_receivers_expected_received_counters() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::env::set_var("RWM_ACKDIAG", "1");
    // The L0 netem shim's `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms
    // jitter, GE p = 2%/q = 40% ⇒ ε ≈ 4.8%) applied to CLIENT EGRESS — the
    // bulk-data direction. Loss is what forces the recovery symbols this
    // reconciliation needs to exist at all.
    std::env::set_var("RWM_L0_NETEM", "c3");
    std::env::set_var("RWM_L0_SEED", "42");

    let g = raptorpath::gates::RuntimeGates::resolve();
    assert!(g.ackdiag, "RWM_ACKDIAG must resolve ON for this test");
    let gauge = raptorpath::net::ackdiag::gauge()
        .expect("RWM_ACKDIAG=1 must construct the process-global gauge");

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47873".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47873".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(Duration::from_secs(180), perf::client(cli_pc, 4_000_000, 2))
        .await
        .expect("lossy ackdiag loopback timed out")
        .expect("lossy ackdiag perf client failed");

    srv.abort();
    // Let the last acks land before reading the totals.
    tokio::time::sleep(Duration::from_millis(300)).await;

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

    // ── THE DISCRIMINATOR ────────────────────────────────────────────────
    // The CONTEMPORANEOUS pair, both sampled at the last report: `srcack` is
    // the delivered SOURCE frontier (the same `window_ack_seq` the `[DIAG]`
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
    // Repairs COUNTED ⇒ arrivals exceed the source frontier. Repairs EXCLUDED
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
    // And the sender-side estimate of what it PUT on the path must in turn
    // exceed what arrived, once anything is lost.
    assert!(
        cexp_sum > crecv_sum,
        "on a lossy cell the tracker's expected count ({cexp_sum}) must exceed \
         its received count ({crecv_sum}) — either the shim dropped nothing \
         (the test proves nothing) or the counters are not the wire's"
    );
    // And the loss the counters imply must be in the right ORDER for the cell
    // (c3: ε ≈ 4.8%), not an artifact: a counter stream that had lost a whole
    // symbol CLASS (e.g. every repair) would read far higher.
    let implied_loss = 1.0 - crecv_sum as f64 / cexp_sum as f64;
    println!("[ackdiag-recon] implied loss from the counters = {implied_loss:.4}");
    assert!(
        implied_loss > 0.0 && implied_loss < 0.40,
        "implied loss {implied_loss:.4} is not a loss rate — the counters are \
         not counting the same population on both sides"
    );
}
