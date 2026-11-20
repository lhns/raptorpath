//! Two-peer harness for the deterministic simulation (`src/sim.rs`).
//!
//! One scenario per process: the engine's statics (the clock's RNG, the
//! `now_us` base, the test seams' env) are process-global. A parent test
//! therefore re-runs its own test binary as a child (`child` test, gated on
//! `SIM_CHILD`), one child at a time, and reads back the child's trace file
//! and log. Each scenario gets its own server port: the real socket is
//! still bound (never polled) under the sim socket.
#![allow(dead_code, unexpected_cfgs)]

#[cfg(not(tokio_unstable))]
compile_error!("sim tests need RUSTFLAGS=\"--cfg tokio_unstable\" (seeded select! order, src/sim.rs)");

use raptorpath::config::{self, RaptorpathConfig};
use raptorpath::perf;
use raptorpath::sim::{Jitter, LinkCfg, SimNet};
use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// A scenario, passed to the child as env.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub seed: u64,
    pub paths: u16,
    pub bytes: usize,
    pub hint: &'static str,
    /// `window_reliable`: Some(true) is ρ = 1, Some(false) ρ < 1, None the hint's preset.
    pub reliable: Option<bool>,
    /// Client-to-server Bernoulli loss.
    pub loss: f64,
    /// Uniform jitter amplitude, ms (0: none).
    pub jitter_ms: u64,
    /// Virtual-time limit for the client, s.
    pub limit_s: u64,
    pub port: u16,
    /// Extra env for the child (seams, gates).
    pub env: Vec<(&'static str, String)>,
}

impl Scenario {
    pub fn basic(seed: u64, paths: u16, port: u16) -> Self {
        Scenario {
            seed,
            paths,
            bytes: 2_000_000,
            hint: "auto",
            reliable: None,
            loss: 0.01,
            jitter_ms: 4,
            limit_s: 30,
            port,
            env: vec![],
        }
    }
}

pub struct ChildRun {
    pub trace: String,
    /// stdout + stderr, minus lines that carry wall time or a real port.
    pub log: String,
    pub outcome: String,
    pub virt_s: f64,
    pub wall_s: f64,
}

impl ChildRun {
    /// The `[TSEAM]` lines whose `ev=` is `ev`.
    pub fn tseam(&self, ev: &str) -> Vec<&str> {
        let tag = format!("[TSEAM] ev={ev} ");
        self.log.lines().filter(|l| l.starts_with(&tag)).collect()
    }
}

/// The data sender's exit account (`[ACCT] ... final=1`, ADR-0081).
pub struct Acct(pub String);

impl Acct {
    pub fn f(&self, key: &str) -> f64 {
        let pat = format!(" {key}=");
        let i = self.0.find(&pat).unwrap_or_else(|| panic!("no {key}= in {}", self.0));
        self.0[i + pat.len()..].split(' ').next().and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("bad {key}= in {}", self.0))
    }

    pub fn u(&self, key: &str) -> u64 {
        self.f(key) as u64
    }

    /// The account's end-of-transfer checks; empty = clean. Same rules as
    /// `tests/common/loopback.rs` `AcctLine::violations`.
    pub fn violations(&self) -> Vec<String> {
        let (sent, a, h, e) = (self.u("S_sent"), self.u("A"), self.u("H"), self.u("E"));
        let (o_serv, o_strand) = (self.u("O_serv"), self.u("O_strand"));
        let mut v = Vec::new();
        if sent != a + h + e + o_serv + o_strand {
            v.push(format!("identity does not close: S_sent={sent} != {}", a + h + e + o_serv + o_strand));
        }
        if o_serv + o_strand != 0 {
            v.push(format!("O = {}", o_serv + o_strand));
        }
        if o_strand != 0 {
            v.push(format!("O_strand = {o_strand}"));
        }
        if self.u("inert_sweeps") != 0 {
            v.push(format!("inert_sweeps = {}", self.u("inert_sweeps")));
        }
        let budget = self.f("bud") * sent as f64;
        if (h + e) as f64 > budget + 1e-9 {
            v.push(format!("H+E = {} > budget {budget:.3}", h + e));
        }
        v
    }
}

impl ChildRun {
    /// The exit line of the sender that sent the most source seqs (the data
    /// sender; the peer's ack-only sender sends none and prints nothing).
    pub fn acct(&self) -> Acct {
        self.log
            .lines()
            .filter(|l| l.starts_with("[ACCT] ") && l.ends_with(" final=1"))
            .map(|l| Acct(format!(" {}", &l["[ACCT] ".len()..])))
            .max_by_key(|a| a.u("S_sent"))
            .unwrap_or_else(|| panic!("no `[ACCT] ... final=1` line (build without the recovery account?):\n{}", self.log))
    }
}

/// Run `sc` in a child process of this test binary.
pub fn run(sc: &Scenario) -> ChildRun {
    let dir = std::env::temp_dir().join(format!("rp-sim-{}-{}", std::process::id(), sc.port));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join(format!("trace-{}.txt", sc.seed));
    let _ = std::fs::remove_file(&out);
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["child", "--exact", "--nocapture", "--test-threads=1"])
        .env("SIM_CHILD", "1")
        .env("SIM_OUT", &out)
        .env("SIM_SEED", sc.seed.to_string())
        .env("SIM_PATHS", sc.paths.to_string())
        .env("SIM_BYTES", sc.bytes.to_string())
        .env("SIM_HINT", sc.hint)
        .env("SIM_RELIABLE", sc.reliable.map_or("-".into(), |r| r.to_string()))
        .env("SIM_LOSS", sc.loss.to_string())
        .env("SIM_JITTER_MS", sc.jitter_ms.to_string())
        .env("SIM_LIMIT_S", sc.limit_s.to_string())
        .env("SIM_PORT", sc.port.to_string())
        .env("RWM_DIAG", "1");
    for (k, v) in &sc.env {
        cmd.env(k, v);
    }
    let o = cmd.output().expect("spawn child");
    let raw = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "child failed ({:?}):\n{raw}", o.status);
    let sim = raw.lines().find(|l| l.starts_with("[SIM] ")).unwrap_or_else(|| panic!("no [SIM] line:\n{raw}"));
    let field = |k: &str| sim.split(' ').find_map(|f| f.strip_prefix(k)).unwrap_or("").to_string();
    let log = raw
        .lines()
        .filter(|l| !(l.contains("RCVBUF") || l.contains("finished in") || l.starts_with("[SIM] ") || l.starts_with("[SIMWD]")))
        .collect::<Vec<_>>()
        .join("\n");
    let run = ChildRun {
        trace: std::fs::read_to_string(&out).unwrap_or_default(),
        log,
        outcome: field("outcome="),
        virt_s: field("virt_s=").parse().unwrap_or(0.0),
        wall_s: field("wall_s=").parse().unwrap_or(0.0),
    };
    println!("{sim}");
    let _ = std::fs::remove_dir_all(&dir);
    run
}

fn env<T: std::str::FromStr>(k: &str) -> T {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("child env {k}"))
}

/// The child side: run the scenario from env, write the trace, print one
/// `[SIM]` line. A no-op unless `SIM_CHILD` is set (a plain `cargo test`
/// run passes it trivially).
pub fn child() {
    if std::env::var_os("SIM_CHILD").is_none() {
        return;
    }
    let seed: u64 = env("SIM_SEED");
    let npaths: u16 = env("SIM_PATHS");
    let bytes: usize = env("SIM_BYTES");
    let hint: String = env("SIM_HINT");
    let reliable: Option<bool> = std::env::var("SIM_RELIABLE").ok().and_then(|v| v.parse().ok());
    let loss: f64 = env("SIM_LOSS");
    let jitter_ms: u64 = env("SIM_JITTER_MS");
    let limit = Duration::from_secs(env("SIM_LIMIT_S"));
    let port: u16 = env("SIM_PORT");
    let out: String = env("SIM_OUT");

    let _ = rustls::crypto::ring::default_provider().install_default();
    let wall0 = std::time::Instant::now();
    // Runaway guard: a scenario that stops advancing virtual time would
    // otherwise hang CI.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        eprintln!("[SIMWD] wall limit (60 s) hit: aborting");
        std::process::abort();
    });

    let rt = raptorpath::sim::runtime(seed);
    let srv_addrs: Vec<std::net::SocketAddr> =
        (0..npaths).map(|i| std::net::SocketAddr::from(([127, 0, 0, 1], port + i))).collect();
    let (outcome, virt_s, net) = rt.block_on(async move {
        let jitter = if jitter_ms == 0 { Jitter::None } else { Jitter::Uniform { max: Duration::from_millis(jitter_ms) } };
        let link = |loss: f64| LinkCfg { jitter, ..LinkCfg::default_40m(loss) };
        let net = SimNet::install(seed, srv_addrs.clone(), link(loss), link(0.0), None);
        let peer = |server: bool| RaptorpathConfig {
            server: server.then_some(true),
            bind: Some(if server {
                srv_addrs.iter().map(|a| a.to_string()).collect()
            } else {
                vec!["127.0.0.1:0".to_string(); npaths as usize]
            }),
            peer: (!server).then(|| srv_addrs.iter().map(|a| a.to_string()).collect()),
            protocol_hint: Some(hint.clone()),
            window_reliable: reliable,
            ..Default::default()
        };
        let srv = config::resolve(&peer(true)).unwrap().0;
        let cli = config::resolve(&peer(false)).unwrap().0;
        let t0 = tokio::time::Instant::now();
        let server = tokio::spawn(perf::server(srv));
        tokio::time::sleep(Duration::from_millis(20)).await;
        let r = tokio::time::timeout(limit, perf::client(cli, bytes, 1)).await;
        let virt = t0.elapsed().as_secs_f64();
        server.abort();
        let outcome = match r {
            Err(_) => "virtual_limit".to_string(),
            Ok(Ok(())) => "client_ok".to_string(),
            Ok(Err(e)) => format!("client_err:{}", format!("{e:#}").replace(' ', "_")),
        };
        (outcome, virt, net)
    });
    drop(rt);
    std::fs::write(&out, net.trace()).unwrap();
    println!(
        "[SIM] seed={seed} outcome={outcome} virt_s={virt_s:.6} wall_s={:.3} events={}",
        wall0.elapsed().as_secs_f64(),
        net.events.load(Relaxed)
    );
}
