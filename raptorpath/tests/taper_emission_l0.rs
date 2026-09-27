//! Taper-emission L0 measurement: the local rung for "the wire consumes r*".
//!
//! Runs the c3-realtime cell in process: real engine (perf server +
//! client over memory TUNs, real QUIC on 127.0.0.1), plain window-reliable
//! mode, realtime hint, with the transport L0 netem shim (`RWM_L0_NETEM`,
//! src/transport/quic.rs) shaping the datagram path. The `c3heavy` scenario
//! carries a heavy-tail loss (semi-Markov, Weibull k = 0.5, theta = 0.55,
//! onset 2.3% → eps ≈ 7% — rstar_tail_validation.rs), a burst-tail
//! structure netem `gemodel` (GE) cannot express — so this L0 shim, not the
//! netem VM, is the local rung for the burst-tail provisioning claim
//! (paper §4.3).
//!
//! Delivered-reliability observable: realtime's reorder horizon is far below
//! the c3 ARQ round, so a loss not recovered in-window is force-delivered as
//! an app hole and the 100 KB perf object can never complete → per-object DNF
//! fraction is app-level delivered reliability. DNFs are an expected datum,
//! cut short by RWM_PERF_TIMEOUT_S.
//! Emitted overhead is read from the sender DIAG cod/src rates (RWM_DIAG=1;
//! scrape lines with src-rate >> 0 — the reverse direction places ~none).
//!
//! `#[ignore]` — measurement instrument, not a CI gate. One arm per process
//! (env is process-global). The 2×2 (r* arm × emission arm):
//!
//! ```text
//! for TAIL in 0 1; do for TAPER in 0 1; do
//!   RWM_RSTAR_TAIL=$TAIL RWM_TAPER_R=$TAPER RWM_L0_NETEM=c3heavy \
//!   RWM_L0_SEED=42 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5 \
//!   cargo test --test taper_emission_l0 --release -- --ignored --nocapture
//! done; done
//! ```
//!
//! Env knobs: RWM_L0_NETEM (default c3heavy), RWM_L0_BYTES (default 100_000),
//! RWM_L0_RUNS (default 20), RWM_L0_HINT (default realtime), plus every
//! engine RWM_* knob (RWM_RSTAR_TAIL, RWM_TAPER_R, RWM_L0_SEED, ...).

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "measurement instrument (#85 taper-emission 2x2), not a CI gate"]
async fn taper_emission_l0_battery() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Default the cell (overridable): heavy-tail c3, realtime, 100 KB
    // objects, DNF cut at 5 s. set_var before any engine thread spawns.
    if std::env::var("RWM_L0_NETEM").is_err() {
        std::env::set_var("RWM_L0_NETEM", "c3heavy");
    }
    if std::env::var("RWM_PERF_TIMEOUT_S").is_err() {
        std::env::set_var("RWM_PERF_TIMEOUT_S", "5");
    }
    let bytes = env_usize("RWM_L0_BYTES", 100_000);
    let runs = env_usize("RWM_L0_RUNS", 20) as u32;
    let hint = std::env::var("RWM_L0_HINT").unwrap_or_else(|_| "realtime".into());

    eprintln!(
        "--- taper_emission_l0: netem={:?} seed={:?} hint={hint} bytes={bytes} runs={runs} \
         RWM_RSTAR_TAIL={:?} RWM_TAPER_R={:?} RWM_PERF_TIMEOUT_S={:?}",
        std::env::var("RWM_L0_NETEM").ok(),
        std::env::var("RWM_L0_SEED").ok(),
        std::env::var("RWM_RSTAR_TAIL").ok(),
        std::env::var("RWM_TAPER_R").ok(),
        std::env::var("RWM_PERF_TIMEOUT_S").ok(),
    );

    let (s, c) = cfgs(&ports(1), &hint, true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    run(srv, cli, bytes, runs, Duration::from_secs(900), "taper_emission_l0 battery").await;
}
