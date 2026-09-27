//! L0 probe for the `RWM_PLAIN_RS` anchor tax: a symbol-rate cost that
//! appears only above ~10 k sym/s and grows with the sender's own rate, not
//! with how hard the store binds. This bench is the component-level
//! discriminator (`docs/measurement-discipline.md` rule 14): loopback has no
//! shaped bottleneck, no propagation delay and no loss, so the only ceiling
//! is the sender itself.
//!
//!   * if a loopback run reproduces a plain-rs/baseline rate ratio well below
//!     1 at a comparable symbol rate, the cost is local to the sender and
//!     needs no network;
//!   * if it reproduces a ratio ~ 1.00 at that rate, the loss requires the
//!     network and the per-symbol-cost reading is refuted.
//!
//! It only discriminates if it reaches the rate: a substrate that cannot
//! drive the sender past ~10 k sym/s cannot settle anything. Read the
//! printed `sym/s` first.
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

#[path = "common/loopback.rs"]
mod loopback;

use std::time::{Duration, Instant};

use loopback::in_process::{cfgs, ports, resolve, start_server};
use raptorpath::perf;

/// Bytes pushed through the reliable window. Large enough that the
/// steady state dominates the connect/warm-up transient.
const NBYTES: usize = 64 * 1024 * 1024;
const SYMBOL_BYTES: f64 = 1200.0;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not a gate: run explicitly with --ignored --nocapture"]
async fn anchor_rate_loopback_ceiling() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let plain_rs = std::env::var("RWM_PLAIN_RS").unwrap_or_else(|_| "unset".into());

    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv_pc, cli_pc) = (resolve(&s), resolve(&c));
    let srv = start_server(srv_pc, "anchor-rate bench").await;

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
    // rate below is a lower bound on the steady-state rate. That direction
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
