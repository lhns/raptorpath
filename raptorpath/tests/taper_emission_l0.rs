//! #85 taper-emission L0 battery: the local rung for "the wire consumes r*".
//!
//! Replays the #46 L1 c3-realtime cell IN PROCESS: real engine (perf server +
//! client over memory TUNs, real QUIC on 127.0.0.1), plain window-reliable
//! mode, realtime hint, with the transport L0 netem shim (`RWM_L0_NETEM`,
//! src/transport/quic.rs) shaping the datagram path. The `c3heavy` scenario
//! carries the #46 ARM-3 heavy-tail loss (semi-Markov, Weibull k = 0.5,
//! theta = 0.55, onset 2.3% → eps ≈ 7% — rstar_tail_validation.rs), which is
//! the burst-tail structure netem `gemodel` (GE) CANNOT express — so this L0
//! shim, not the netem VM, is the correct local rung for the §8.4.1
//! heavy-tail claim.
//!
//! Delivered-reliability observable (same as tools/l1/rstar_battery.sh):
//! realtime's reorder horizon is far below the c3 ARQ round, so a loss not
//! recovered IN-WINDOW is force-delivered as an app hole and the 100 KB perf
//! object can never complete → per-object DNF fraction IS app-level delivered
//! reliability. DNFs are an EXPECTED datum, cut short by RWM_PERF_TIMEOUT_S.
//! Emitted overhead is read from the sender DIAG cod/src rates (RWM_DIAG=1;
//! scrape lines with src-rate >> 0 — the reverse direction places ~none).
//!
//! `#[ignore]` — measurement instrument, not a CI gate. One arm per process
//! (env is process-global). The #85 2×2 (r* arm × emission arm):
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

use std::time::Duration;

use raptorpath::{config, perf};

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

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47921".into()]),
        protocol_hint: Some(hint.clone()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47921".into()]),
        protocol_hint: Some(hint),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    // Generous overall bound; each run has the perf RUN_TIMEOUT (5 s here).
    tokio::time::timeout(Duration::from_secs(900), perf::client(cli_pc, bytes, runs))
        .await
        .expect("taper_emission_l0 battery timed out")
        .expect("taper_emission_l0 perf client failed");

    srv.abort();
}
