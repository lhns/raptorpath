//! L0 gen-substrate throughput bench (feat/gen-substrate-ceiling, JOB 1).
//!
//! Runs the REAL engine (perf server + perf client over memory TUNs and real
//! QUIC on 127.0.0.1) with the transport-level L0 netem shim
//! (`RWM_L0_NETEM`, src/transport/quic.rs) shaping the datagram path like the
//! L1 harness scenarios — so the generation-mode pipeline dynamics
//! (window advance, pacing, deficit rounds) can be diagnosed locally with
//! induced RTT + rate + GE loss, before any VM time is spent.
//!
//! `#[ignore]` — this is a measurement instrument, not a CI gate. Run one
//! configuration per process (env is process-global):
//!
//! ```text
//! RWM_L0_NETEM=c2 RWM_L0_MODE=gen RWM_GEN_R=0.03 RWM_DIAG=1 \
//!   cargo test --test gen_substrate_l0 --release -- --ignored --nocapture
//! ```
//!
//! Env knobs:
//!   RWM_L0_NETEM  scenario per path (c2 / c3 / c2,c3 / clean / custom:…)
//!   RWM_L0_MODE   plain | gen (default) | sys  (window pipeline submode)
//!   RWM_L0_BYTES  object size (default 12_500_000 = 12.5 MB)
//!   RWM_L0_RUNS   timed runs (default 3)
//!   RWM_L0_DUAL   =1 → two loopback paths (C7/C8 shape via RWM_L0_NETEM)
//!   plus every RWM_* knob the engine itself reads (RWM_GEN_R, RWM_STORE, …)

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "measurement instrument (JOB 1 diagnosis), not a CI gate"]
async fn gen_substrate_l0_bench() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let bytes = env_usize("RWM_L0_BYTES", 12_500_000);
    let runs = env_usize("RWM_L0_RUNS", 3) as u32;
    let mode = std::env::var("RWM_L0_MODE").unwrap_or_else(|_| "gen".into());
    let dual = std::env::var("RWM_L0_DUAL").map(|v| v == "1").unwrap_or(false);

    let (gen_coding, sys_repair) = match mode.as_str() {
        "plain" => (false, false),
        "sys" => (false, true),
        _ => (true, false),
    };

    let n_paths = if dual { 2 } else { 1 };

    eprintln!(
        "--- gen_substrate_l0: mode={mode} dual={dual} bytes={bytes} runs={runs} \
         netem={:?}",
        std::env::var("RWM_L0_NETEM").ok()
    );

    let (mut s, mut c) = cfgs(&ports(n_paths), "bulk", true);
    for cfg in [&mut s, &mut c] {
        cfg.window_generation_coding = Some(gen_coding);
        cfg.window_systematic_repair = Some(sys_repair);
    }
    let (srv, cli) = (resolve(&s), resolve(&c));

    // Generous overall bound: the runs themselves have the perf RUN_TIMEOUT.
    run(srv, cli, bytes, runs, Duration::from_secs(1200), "gen_substrate_l0 bench").await;
}
