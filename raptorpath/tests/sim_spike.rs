//! SIM SPIKE (throwaway): both perf peers in one process on a current-thread
//! tokio runtime with paused time, quinn on the in-memory `sim` network.
//! One run per process (process-global statics, hash seeds): the driver
//! script runs this test binary repeatedly and diffs the trace files.
//!
//! Env: SIM_SEED (link + engine RNG + tokio select seed), SIM_OUT (trace
//! file), SIM_SCEN = basic | tail | tailctl, SIM_BYTES, SIM_LIMIT_S (virtual
//! limit), SIM_STD_CLOCK=1 (leave engine reads on the std clock),
//! SIM_WALL_S (watchdog, real seconds).

use raptorpath::config::{self, RaptorpathConfig};
use raptorpath::perf;
use raptorpath::sim::{Blackout, LinkCfg, SimNet};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

fn env_u64(k: &str, d: u64) -> u64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

#[test]
fn sim_run() {
    let seed = env_u64("SIM_SEED", 1);
    let scen = std::env::var("SIM_SCEN").unwrap_or_else(|_| "basic".into());
    let out = std::env::var("SIM_OUT").unwrap_or_else(|_| "sim_trace.txt".into());
    let limit = Duration::from_secs(env_u64("SIM_LIMIT_S", 30));
    let wall_limit = env_u64("SIM_WALL_S", 300);
    let std_clock = std::env::var("SIM_STD_CLOCK").is_ok_and(|v| v == "1");
    let tail = scen.starts_with("tail");
    let bytes = env_u64("SIM_BYTES", if tail { 6_000_000 } else { 2_000_000 }) as usize;
    let hint = if tail { "realtime" } else { "auto" };

    raptorpath::clock::SIM_CLOCK.store(!std_clock, Relaxed);
    raptorpath::clock::SIM_RNG.store(std::env::var("SIM_NO_RNG").is_err(), Relaxed);
    raptorpath::clock::SIM_RNG_STATE.store(seed.wrapping_mul(0x2545_F491_4F6C_DD1D), Relaxed);
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut sb = [0u8; 8];
    sb.copy_from_slice(&seed.to_le_bytes());
    let mut b = tokio::runtime::Builder::new_current_thread();
    b.enable_all().start_paused(true);
    if std::env::var("SIM_NO_TOKIO_SEED").is_err() {
        b.rng_seed(tokio::runtime::RngSeed::from_bytes(&sb));
    }
    let rt = b.build().unwrap();

    let wall0 = std::time::Instant::now();
    let srv_addr: std::net::SocketAddr = "127.0.0.1:47011".parse().unwrap();

    let (outcome, virt_s, net) = rt.block_on(async move {
        let link = |loss: f64| LinkCfg {
            rate_bps: 40e6,
            delay: Duration::from_millis(10),
            loss,
            queue_bytes: 500_000,
        };
        let (c2s_loss, blackout) = match scen.as_str() {
            "basic" => (std::env::var("SIM_LOSS").ok().and_then(|v| v.parse().ok()).unwrap_or(0.01), None),
            // k = 11800 of 12 146 source chunks: the blackout starts at about k's
            // send (counted in data-sized datagrams) and lasts 1.5 s.
            "tail" => (
                0.025,
                Some(Blackout { start_dgram: env_u64("SIM_BLACK_AT", 11_800), min_len: 400, dur: Duration::from_millis(1500) }),
            ),
            _ => (0.0, None), // tailctl: same config, clean link, no blackout
        };
        let net = SimNet::install(seed, vec![srv_addr], link(c2s_loss), link(0.0), blackout);
        {
            let net = net.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(5));
                let w = wall0.elapsed().as_secs();
                eprintln!(
                    "[SIMWD] wall_s={w} virt_us={} events={}",
                    net.last_virt_us.load(Relaxed),
                    net.events.load(Relaxed)
                );
                if w >= wall_limit {
                    eprintln!("[SIMWD] wall limit hit: aborting");
                    std::process::abort();
                }
            });
        }

        let srv = RaptorpathConfig {
            server: Some(true),
            bind: Some(vec![srv_addr.to_string()]),
            protocol_hint: Some(hint.into()),
            window_reliable: if tail { Some(false) } else { None },
            ..Default::default()
        };
        let cli = RaptorpathConfig {
            bind: Some(vec!["127.0.0.1:0".to_string()]),
            peer: Some(vec![srv_addr.to_string()]),
            protocol_hint: Some(hint.into()),
            window_reliable: if tail { Some(false) } else { None },
            ..Default::default()
        };
        let srv = config::resolve(&srv).unwrap().0;
        let cli = config::resolve(&cli).unwrap().0;
        let t0 = tokio::time::Instant::now();
        let server = tokio::spawn(perf::server(srv));
        tokio::time::sleep(Duration::from_millis(20)).await;
        let r = tokio::time::timeout(limit, perf::client(cli, bytes, 1)).await;
        let virt = t0.elapsed().as_secs_f64();
        server.abort();
        let outcome = match r {
            Err(_) => "virtual_limit".to_string(),
            Ok(Ok(())) => "client_ok".to_string(),
            Ok(Err(e)) => format!("client_err:{e:#}"),
        };
        (outcome, virt, net)
    });
    let wall = wall0.elapsed().as_secs_f64();
    let trace = net.trace.lock().unwrap().join("\n");
    std::fs::write(&out, trace + "\n").unwrap();
    println!(
        "[SIM] seed={seed} outcome={outcome} virt_s={virt_s:.6} wall_s={wall:.3} events={} std_clock={std_clock}",
        net.events.load(Relaxed)
    );
}
