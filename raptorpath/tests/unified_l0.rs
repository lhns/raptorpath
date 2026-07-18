//! #61 unified-machine L0 battery arm: the local rung for the RWM_UNIFIED A/B
//! (paper §16.20; goal-gate "Unified Decoder").
//!
//! Runs the real engine (perf server + client over memory TUNs, real QUIC on
//! 127.0.0.1) under the transport L0 netem shim (`RWM_L0_NETEM`,
//! src/transport/quic.rs) — one measurement ARM per process (env is
//! process-global). The battery driver sets the arm via env and scrapes the
//! per-run `"seconds"`/`"dnf"` lines (per-object completion time distribution
//! = the local tail proxy) plus the sender DIAG cod/src rates (mechanism
//! liveness) and the RWM_UNIFIED / backend echoes (MEASUREMENT DISCIPLINE).
//!
//! Env knobs (beyond every engine RWM_* knob — RWM_UNIFIED, RWM_TAPER_R,
//! RWM_L0_SEED, RWM_DIAG, RWM_PERF_TIMEOUT_S, ...):
//!   RWM_L0_NETEM    shim scenario (default c3heavy — the #85 heavy-tail cell)
//!   RWM_L0_HINT     protocol hint (default realtime) — the δ dial
//!   RWM_L0_BYTES    object size (default 100_000)
//!   RWM_L0_RUNS     objects per arm (default 40)
//!   RWM_L0_BACKEND  explicit fec_backend (e.g. "rlc" to pin the RLC family
//!                   under the realtime hint in the LEGACY arms; unset = the
//!                   shipped auto-selection)
//!   RWM_L0_SYSREP   "1" = generation systematic-repair mode (the bulk
//!                   machine; window_systematic_repair)
//!
//! `#[ignore]` — measurement instrument, not a CI gate.

use std::time::Duration;

use raptorpath::{config, perf};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "measurement instrument (#61 unified-machine L0 battery), not a CI gate"]
async fn unified_l0_arm() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // MEASUREMENT DISCIPLINE: surface the engine's mechanism-liveness echoes
    // (backend selection, "unified global decoder", "span law ACTIVE", gen
    // pipe) on stderr. RUST_LOG=info (default here) is enough.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    if std::env::var("RWM_L0_NETEM").is_err() {
        std::env::set_var("RWM_L0_NETEM", "c3heavy");
    }
    if std::env::var("RWM_PERF_TIMEOUT_S").is_err() {
        std::env::set_var("RWM_PERF_TIMEOUT_S", "5");
    }
    let bytes = env_usize("RWM_L0_BYTES", 100_000);
    let runs = env_usize("RWM_L0_RUNS", 40) as u32;
    let hint = std::env::var("RWM_L0_HINT").unwrap_or_else(|_| "realtime".into());
    let backend = std::env::var("RWM_L0_BACKEND").ok();
    let sysrep = std::env::var("RWM_L0_SYSREP").map(|v| v == "1").unwrap_or(false);

    eprintln!(
        "--- unified_l0 arm: netem={:?} seed={:?} hint={hint} backend={backend:?} sysrep={sysrep} \
         bytes={bytes} runs={runs} RWM_UNIFIED={:?} RWM_TAPER_R={:?} RWM_GEN_PIPE={:?} \
         RWM_PERF_TIMEOUT_S={:?}",
        std::env::var("RWM_L0_NETEM").ok(),
        std::env::var("RWM_L0_SEED").ok(),
        std::env::var("RWM_UNIFIED").ok(),
        std::env::var("RWM_TAPER_R").ok(),
        std::env::var("RWM_GEN_PIPE").ok(),
        std::env::var("RWM_PERF_TIMEOUT_S").ok(),
    );

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47923".into()]),
        protocol_hint: Some(hint.clone()),
        window_reliable: Some(true),
        window_systematic_repair: if sysrep { Some(true) } else { None },
        fec_backend: backend.clone(),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47923".into()]),
        protocol_hint: Some(hint),
        window_reliable: Some(true),
        window_systematic_repair: if sysrep { Some(true) } else { None },
        fec_backend: backend,
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    tokio::time::timeout(Duration::from_secs(1800), perf::client(cli_pc, bytes, runs))
        .await
        .expect("unified_l0 arm timed out")
        .expect("unified_l0 perf client failed");

    srv.abort();
}
