//! goal-gate "What Binds Throughput" — the L0 probe for the anchor tax.
//!
//! THE CLAIM UNDER TEST. In the three-term battery, `RWM_PLAIN_RS` alone
//! (arm D) costs 0.635/0.654 of arm A's symbol rate at c1 (23.5-24.1 k
//! sym/s) and 0.877/0.884 at c7 (19.1 k sym/s), and costs EXACTLY NOTHING —
//! D/A = 0.996 to 1.002 — at all six cell-seeds in the 5.0-9.9 k sym/s band,
//! which between them span RTT 10-525 ms, GE loss 0-2.5 %, jitter 0-25 ms
//! and link utilisation 51-100 %. The tax's only argument is arm A's own
//! SYMBOL RATE, and it is anti-correlated with how hard the store binds
//! (the zero-tax cells all sit at occupancy/cap 0.99-1.00 and paused
//! 5-25 %; c1, the maximum-tax cell, sits at 0.43 and 0.8 %).
//!
//! That shape says the tax is a per-symbol cost local to the SENDER, not a
//! store-sizing effect. This bench is the component-level discriminator
//! (discipline 14): loopback has no shaped bottleneck, no propagation delay
//! and no loss, so the only ceiling is the sender itself.
//!
//!   * if a loopback run reproduces D/A well below 1 at a comparable symbol
//!     rate, the cost is local to the sender and needs no network;
//!   * if it reproduces D/A ~ 1.00 at that rate, the c1/c7 loss REQUIRES the
//!     network and the "per-symbol cost" reading is refuted.
//!
//! IT ONLY DISCRIMINATES IF IT REACHES THE RATE. The tax is invisible below
//! ~10 k sym/s in the battery, so a substrate that cannot drive the sender
//! past that cannot settle anything, and the bench says so rather than
//! quoting a ratio measured where no ratio is expected. Run it and read the
//! printed `sym/s` FIRST.
//!
//! `RWM_PLAIN_RS` is resolved once per process, so the two conditions are
//! two separate invocations, not two cases in one:
//!
//! ```text
//!   cargo test --release -p raptorpath --test anchor_rate_bench -- --ignored --nocapture
//!   RWM_PLAIN_RS=1 cargo test --release -p raptorpath --test anchor_rate_bench -- --ignored --nocapture
//! ```
//!
//! `#[ignore]`d: it is a measurement, it takes seconds to tens of seconds,
//! and it asserts no threshold. It is in no gate and weakens none.

use std::time::{Duration, Instant};

use raptorpath::{config, perf};

/// Bytes pushed through the reliable window. Large enough that the
/// steady state dominates the connect/warm-up transient.
const NBYTES: usize = 64 * 1024 * 1024;
const SYMBOL_BYTES: f64 = 1200.0;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not a gate: run explicitly with --ignored --nocapture"]
async fn anchor_rate_loopback_ceiling() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let plain_rs = std::env::var("RWM_PLAIN_RS").unwrap_or_else(|_| "unset".into());

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47871".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47871".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    let t0 = Instant::now();
    let out = tokio::time::timeout(
        Duration::from_secs(300),
        perf::client(cli_pc, NBYTES, 1),
    )
    .await;
    let dt = t0.elapsed().as_secs_f64();
    srv.abort();

    let ok = matches!(&out, Ok(Ok(())));
    // The warm-up object and the connect handshake are inside `dt`, so the
    // rate below is a LOWER bound on the steady-state rate. That direction
    // is the safe one: it can only make the substrate look less capable
    // than it is, never more.
    let sym_s = NBYTES as f64 / SYMBOL_BYTES / dt;
    println!(
        "[ANCHOR-BENCH] RWM_PLAIN_RS={plain_rs} completed={ok} bytes={NBYTES} \
         wall={dt:.2}s rate={:.1}Mbit sym/s={sym_s:.0}",
        NBYTES as f64 * 8.0 / dt / 1e6,
    );
    println!(
        "[ANCHOR-BENCH] battery reference: the tax is 0.64 at 23.5-24.1k sym/s, \
         0.88 at 19.1k, and 1.00 (0.996-1.002) at 5.0-9.9k. A loopback below \
         ~10k sym/s DISCRIMINATES NOTHING."
    );
    assert!(ok, "loopback transfer did not complete: {out:?}");
}
