//! Shared loopback harness for the perf-mode integration tests.
//!
//! Two shapes of test use this module:
//!
//! * **Spawned** (`*_reachability.rs`): the real `raptorpath` binary runs as a
//!   perf server child ([`spawn_perf_server`]) and a perf client child
//!   ([`run_perf_client`] / [`lossy_run`]). Each child's environment is its
//!   own, so a gate set for one arm cannot leak into another test.
//! * **In-process** (`*_loopback.rs`, `*_l0.rs`): `perf::server` and
//!   `perf::client` run as tokio tasks in the test process
//!   ([`in_process_loopback`]).
//!
//! Kept OUT of `common/mod.rs` on purpose: that module is the simulation
//! harness, and the sim-only binaries must not compile process-spawning code.
//! Include it with `#[path = "common/loopback.rs"] mod loopback;`.
//!
//! Harness rules this module enforces (each prevents a real flake):
//!
//! * **Ports come from the OS, over UDP** ([`free_port`]) — the server binds
//!   UDP, so probing a TCP port proves nothing, and hard-coded shared ports
//!   collide across test binaries run in parallel.
//! * **The server log is one merged, line-buffered stream** ([`ServerLog`]):
//!   stdout and stderr are read line by line into one `Vec<String>` in
//!   arrival order. Chunked readers could split a gauge line between the two
//!   streams' chunks.
//! * **Every wait has a real deadline.** Readiness is polled (log line, then
//!   the UDP port actually bound), never a blocking `read` that can outlive
//!   its deadline, and never a fixed sleep.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::net::{SocketAddr, UdpSocket};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle as ThreadHandle;
use std::time::{Duration, Instant};

/// How long a spawned or in-process server has to become ready.
pub const READY_DEADLINE: Duration = Duration::from_secs(60);

/// The bytes the perf server prints once its engine task is spawned.
pub const READY_BANNER: &str = "perf server ready";

const POLL: Duration = Duration::from_millis(20);

// ── ports ────────────────────────────────────────────────────────────────

/// A free loopback UDP port, chosen by the OS. The server binds UDP (QUIC),
/// so the probe binds UDP too.
pub fn free_port() -> u16 {
    let s = UdpSocket::bind("127.0.0.1:0").expect("probe bind");
    s.local_addr().expect("probe addr").port()
}

/// `127.0.0.1:<free_port()>`.
pub fn free_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], free_port()))
}

/// `n` distinct free loopback addresses (one per path).
pub fn free_addrs(n: usize) -> Vec<SocketAddr> {
    let mut v: Vec<SocketAddr> = Vec::with_capacity(n);
    while v.len() < n {
        let a = free_addr();
        if !v.contains(&a) {
            v.push(a);
        }
    }
    v
}

/// The comma-joined form `--bind` / `--peer` take.
pub fn join(addrs: &[SocketAddr]) -> String {
    addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",")
}

/// Is something bound on `addr` (UDP)? Sends one 1-byte datagram from a
/// connected socket: an unbound port answers with ICMP port-unreachable,
/// which surfaces as `ConnectionRefused` (Linux) / `ConnectionReset`
/// (Windows) on the next `recv`. Silence means bound (QUIC drops a 1-byte
/// datagram as unparseable). The probe never binds `addr` itself, so it
/// cannot race the server for the port.
///
/// One-sided by construction: `false` is proof of "not bound"; `true` could
/// in principle be a suppressed ICMP, which only degrades to the QUIC
/// handshake's own Initial retransmission — never to a wrong result.
pub fn udp_bound(addr: SocketAddr) -> bool {
    let Ok(s) = UdpSocket::bind("127.0.0.1:0") else {
        return false;
    };
    if s.connect(addr).is_err() || s.send(&[0u8]).is_err() {
        return false;
    }
    let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
    let mut b = [0u8; 64];
    match s.recv(&mut b) {
        Ok(_) => true,
        Err(e) => !matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset),
    }
}

/// Poll `cond` every 20 ms until it holds or `timeout` elapses.
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}

// ── child processes ──────────────────────────────────────────────────────

/// Apply an arm's env to a spawned endpoint: EVERY inherited `RWM_*` var is
/// removed first, then `env` is set. A control arm must be ABSENT, not
/// "whatever the developer's shell exported" — inheritance defeats an
/// allowlist (the `holddown_reachability` rule, made universal). `RUST_LOG`
/// and everything else non-`RWM_` is inherited as before.
fn apply_env(cmd: &mut Command, env: &[(&str, &str)]) {
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("RWM_") {
            cmd.env_remove(k);
        }
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
}

/// Kills (and reaps) the child on drop, so a failing assertion never leaks
/// a perf server.
pub struct Reaper(pub Child);

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The server's merged stdout+stderr, one entry per line, in arrival order.
#[derive(Clone, Default)]
pub struct ServerLog(Arc<Mutex<Vec<String>>>);

impl ServerLog {
    fn push(&self, line: String) {
        self.0.lock().expect("server log").push(line);
    }

    /// Number of lines received so far — a mark for [`Self::wait_line_after`].
    pub fn len(&self) -> usize {
        self.0.lock().expect("server log").len()
    }

    /// The whole log as text (lines joined with `\n`).
    pub fn text(&self) -> String {
        self.0.lock().expect("server log").join("\n")
    }

    /// Is there a line at index ≥ `mark` containing `pat`?
    pub fn has_line_after(&self, mark: usize, pat: &str) -> bool {
        self.0
            .lock()
            .expect("server log")
            .iter()
            .skip(mark)
            .any(|l| l.contains(pat))
    }

    /// Wait (≤ `timeout`) for a line containing `pat` to arrive at index
    /// ≥ `mark`. Returns whether it arrived; the caller's own assertion on
    /// the log names the failure.
    pub fn wait_line_after(&self, mark: usize, pat: &str, timeout: Duration) -> bool {
        wait_until(timeout, || self.has_line_after(mark, pat))
    }

    /// Wait (≤ `timeout`) until no line arrived for `quiet`.
    pub fn wait_quiet(&self, quiet: Duration, timeout: Duration) -> bool {
        let mut last_len = self.len();
        let mut last_change = Instant::now();
        wait_until(timeout, || {
            let n = self.len();
            if n != last_len {
                last_len = n;
                last_change = Instant::now();
            }
            last_change.elapsed() >= quiet
        })
    }
}

fn pump<R: Read + Send + 'static>(src: R, log: ServerLog) -> ThreadHandle<()> {
    std::thread::spawn(move || {
        let mut r = BufReader::new(src);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    while matches!(buf.last(), Some(b'\n' | b'\r')) {
                        buf.pop();
                    }
                    log.push(String::from_utf8_lossy(&buf).into_owned());
                }
            }
        }
    })
}

/// A running perf server child.
pub struct PerfServer {
    pub addrs: Vec<SocketAddr>,
    pub log: ServerLog,
    reaper: Reaper,
    readers: Vec<ThreadHandle<()>>,
}

impl PerfServer {
    /// The single bind address (panics on a multi-path server).
    pub fn addr(&self) -> SocketAddr {
        assert_eq!(self.addrs.len(), 1, "multi-path server: use .addrs");
        self.addrs[0]
    }

    pub fn pid(&self) -> u32 {
        self.reaper.0.id()
    }

    /// The server log after the client finished: waits (≤ 10 s) for a
    /// `pat` line emitted after `mark` — one gauge emission that post-dates
    /// the transfer — and then for the log to go quiet for 100 ms (≤ 2 s), so
    /// the rest of that cadence block (its sibling lines are printed back to
    /// back) is in too. Replaces the old fixed 1.5 s grace sleep with the
    /// condition it was waiting for.
    pub fn log_after(&self, mark: usize, pat: &str) -> String {
        if self.log.wait_line_after(mark, pat, Duration::from_secs(10)) {
            self.log.wait_quiet(Duration::from_millis(100), Duration::from_secs(2));
        }
        self.log.text()
    }

    /// Stop the server with `SIG<sig>` (unix; `"INT"` = ctrl_c, `"TERM"` =
    /// what `pkill -x raptorpath` sends) and wait ≤ 15 s for it to exit, then
    /// join the readers so the log holds everything written on the way out
    /// (the engine's `final=1` exit flush). Non-unix has no signal to send:
    /// the server is killed once a `fresh_tag` cadence line post-dating this
    /// call arrived (≤ 10 s), and only the cadence lines are observable.
    // `sig` is read on unix only, `fresh_tag` elsewhere only.
    #[allow(unused_variables)]
    pub fn stop_with(mut self, sig: &str, fresh_tag: &str) -> String {
        #[cfg(unix)]
        {
            let pid = self.reaper.0.id().to_string();
            let sent = Command::new("kill")
                .args([&format!("-{sig}"), &pid])
                .status()
                .is_ok_and(|s| s.success());
            assert!(sent, "could not send SIG{sig} to the perf server (pid {pid})");
            let child = &mut self.reaper.0;
            let exited = wait_until(Duration::from_secs(15), || {
                matches!(child.try_wait(), Ok(Some(_)))
            });
            assert!(
                exited,
                "perf server did not exit within 15 s of SIG{sig} — the shutdown \
                 broadcast never reached its tasks"
            );
        }
        #[cfg(not(unix))]
        {
            let mark = self.log.len();
            self.log.wait_line_after(mark, fresh_tag, Duration::from_secs(10));
            let _ = self.reaper.0.kill();
            let _ = self.reaper.0.wait();
        }
        for r in self.readers.drain(..) {
            let _ = r.join();
        }
        self.log.text()
    }
}

/// Spawn `raptorpath perf --server --bind <binds> <args…>` with exactly the
/// `RWM_*` env in `env` (inherited `RWM_*` vars are cleared, so the server
/// never inherits a netem spec: the client's shapes the transfer). Returns once the server printed
/// its ready banner AND every bind address is observably bound, within
/// [`READY_DEADLINE`]; panics with the server's own output otherwise.
pub fn spawn_perf_server(binds: &[SocketAddr], env: &[(&str, &str)], args: &[&str]) -> PerfServer {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_raptorpath"));
    cmd.args(["perf", "--server", "--bind", &join(binds)]);
    cmd.args(args);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    apply_env(&mut cmd, env);
    let mut child = Reaper(cmd.spawn().expect("spawn perf server"));
    let log = ServerLog::default();
    let readers = vec![
        pump(child.0.stdout.take().expect("server stdout"), log.clone()),
        pump(child.0.stderr.take().expect("server stderr"), log.clone()),
    ];

    let deadline = Instant::now() + READY_DEADLINE;
    let mut exited = None;
    let ready = wait_until(READY_DEADLINE, || {
        if let Ok(Some(st)) = child.0.try_wait() {
            exited = Some(st);
            return true;
        }
        log.has_line_after(0, READY_BANNER)
    });
    assert!(
        ready && exited.is_none(),
        "perf server never became ready (exit: {exited:?}); it said:\n{}",
        log.text()
    );
    let remaining = deadline.saturating_duration_since(Instant::now());
    let bound = wait_until(remaining, || binds.iter().all(|a| udp_bound(*a)));
    assert!(
        bound,
        "perf server printed ready but never bound {}; it said:\n{}",
        join(binds),
        log.text()
    );
    PerfServer { addrs: binds.to_vec(), log, reaper: child, readers }
}

/// Run `raptorpath perf --client --peer <peers> <args…>` to completion with
/// exactly the `RWM_*` env in `env`. Returns the raw output.
pub fn perf_client_output(peers: &[SocketAddr], env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cli = Command::new(env!("CARGO_BIN_EXE_raptorpath"));
    cli.args(["perf", "--client", "--peer", &join(peers)]);
    cli.args(args);
    cli.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    apply_env(&mut cli, env);
    cli.output().expect("run perf client")
}

/// [`perf_client_output`], asserting success; returns `stdout\nstderr`.
pub fn run_perf_client(peers: &[SocketAddr], env: &[(&str, &str)], args: &[&str]) -> String {
    let out = perf_client_output(peers, env, args);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "perf client failed ({:?}; env {env:?}; args {args:?})\n--- stdout ---\n{stdout}\n\
         --- stderr ---\n{stderr}",
        out.status
    );
    format!("{stdout}\n{stderr}")
}

/// The standard perf-mode argument tail: `--bytes <b> --runs <n>
/// --protocol-hint <hint> --window-reliable`.
pub fn perf_args<'a>(hint: &'a str, bytes: &'a str, runs: &'a str) -> [&'a str; 7] {
    [
        "--bytes",
        bytes,
        "--runs",
        runs,
        "--protocol-hint",
        hint,
        "--window-reliable",
    ]
}

/// The netem cell most reachability tests shape the client with: the L1
/// `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms jitter, GE p = 2 % /
/// q = 40 % ⇒ ε ≈ 4.8 %), seeded.
pub const C3: [(&str, &str); 2] = [("RWM_L0_NETEM", "c3"), ("RWM_L0_SEED", "42")];

/// The bottleneck rate of [`CLEAN_RATE`], in Mbit/s. Kept literally in sync
/// with the `custom:` spec below; tests derive their run-duration floor from
/// it (`bytes·8 / rate` is the shaper's lower bound on a run's wall time).
pub const CLEAN_RATE_MBIT: f64 = 50.0;

/// Rate-only client shaping for the tick-sampled `[DIAG]` reachability tests
/// (σ, candidates, tlag): `custom:<rate_mbit>;<ow_ms>;<jit_ms>;<ge_p>;<ge_q>`
/// = 50 Mbit, 0 ms one-way, 0 ms jitter, `ge_p = 0` (the GE loss branch is
/// skipped), seeded.
///
/// **The fields are `;`-separated, not `,`.** `L0Netem::from_env` first
/// splits the WHOLE spec on `,` into per-path cells (`c2,c3` = path 0 / path
/// 1), then `l0_scenario` splits one cell on `;`. A comma-separated
/// `custom:` spec parses to no scenario, the shim logs `shim OFF` and the run
/// is silently unshaped — so every user asserts `shim ACTIVE` and the absence
/// of `shim OFF` (the shim is not in the `[GATES]` echo).
///
/// Why: `[DIAG]` is printed only on 250 ms ticks and samples cwnd saturation
/// at that one instant. Unshaped loopback is CPU-bound, so its wall time —
/// and with it the tick count — shrinks with every sender speed-up (two 8 MB
/// objects fell to 0.3–0.8 s, i.e. 1–3 ticks, and the clauses became coin
/// flips). A fixed bottleneck bounds the wall time below by `bytes·8/R`
/// regardless of CPU speed, and makes a full cwnd the steady state instead of
/// a scheduler coincidence. Only the client is shaped; the server clears
/// inherited `RWM_*`, so the ack direction stays the host's own loopback.
/// R = 50 Mbit was chosen by a VM calibration (10 runs per test).
pub const CLEAN_RATE: [(&str, &str); 2] = [
    ("RWM_L0_NETEM", "custom:50;0;0;0;100"),
    ("RWM_L0_SEED", "42"),
];

/// Prove [`CLEAN_RATE`] executed on a client run (MEASUREMENT DISCIPLINE
/// rule 1): the shim's own ACTIVE echo is present, its `shim OFF` warning is
/// absent, exactly `runs` timed (non-warm-up, non-DNF) perf runs were
/// reported, and each took at least 0.8 × the shaper's `bytes·8/R` floor.
/// Returns the per-run seconds.
pub fn assert_clean_rate_executed(log: &str, bytes: u64, runs: usize) -> Vec<f64> {
    assert!(
        log.contains("shim ACTIVE"),
        "no `L0 netem shim ACTIVE` echo — the CLEAN_RATE shaper did not run:\n{log}"
    );
    assert!(
        !log.contains("shim OFF"),
        "the L0 netem shim logged `shim OFF` — the CLEAN_RATE spec did not parse \
         (comma instead of `;`?):\n{log}"
    );
    let mut secs = Vec::new();
    for line in log.lines().filter(|l| l.trim_start().starts_with('{')) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if v.get("run").is_none() {
            continue; // the warm-up line, or the closing summary
        }
        assert!(
            v.get("dnf").is_none(),
            "a perf run did not finish under the CLEAN_RATE shaper: {line}"
        );
        let s = v
            .get("seconds")
            .and_then(|s| s.as_f64())
            .unwrap_or_else(|| panic!("a perf run line carries no `seconds`: {line}"));
        secs.push(s);
    }
    assert_eq!(
        secs.len(),
        runs,
        "expected {runs} timed perf runs in the client output, parsed {secs:?}:\n{log}"
    );
    let floor = 0.8 * bytes as f64 * 8.0 / (CLEAN_RATE_MBIT * 1e6);
    for s in &secs {
        assert!(
            *s >= floor,
            "a {bytes}-byte run took {s:.3} s, under 0.8 × the {CLEAN_RATE_MBIT} Mbit \
             shaper's floor ({floor:.3} s) — the rate shaping did not execute"
        );
    }
    secs
}

/// Client env for an optional netem spec: `Some(spec)` ⇒ `RWM_L0_NETEM=spec`
/// seeded with 42; `None` ⇒ nothing (the shim is OFF, the wire is the host's
/// own loopback).
pub fn shaped(netem: Option<&str>) -> Vec<(&str, &str)> {
    match netem {
        Some(spec) => vec![("RWM_L0_NETEM", spec), ("RWM_L0_SEED", "42")],
        None => Vec::new(),
    }
}

/// One reliable-window perf transfer, spawned: see [`transfer`].
#[derive(Clone, Copy)]
pub struct Transfer<'a> {
    /// Loopback paths (one fresh port each).
    pub paths: usize,
    /// Env for BOTH endpoints (the arm).
    pub env: &'a [(&'a str, &'a str)],
    /// Extra env for the client only (netem shapes the client's egress; the
    /// server's ack direction stays clean).
    pub client_env: &'a [(&'a str, &'a str)],
    /// Extra env for the server only.
    pub server_env: &'a [(&'a str, &'a str)],
    pub hint: &'a str,
    pub bytes: &'a str,
    pub runs: &'a str,
    /// Extra flags for BOTH endpoints (e.g. `--window-generation-coding`).
    pub extra_args: &'a [&'a str],
    /// `Some(tag)`: take the server log only after a `tag` line post-dating
    /// the transfer arrived (≤ 10 s) — one fresh cadence emission, the
    /// condition the old fixed 1.5 s sleep stood in for. `None`: snapshot the
    /// server log immediately (for tests that only read the client).
    pub srv_tag: Option<&'a str>,
}

impl Default for Transfer<'_> {
    fn default() -> Self {
        Transfer {
            paths: 1,
            env: &[],
            client_env: &C3,
            server_env: &[],
            hint: "bulk",
            bytes: "4000000",
            runs: "2",
            extra_args: &[],
            srv_tag: None,
        }
    }
}

/// Run one [`Transfer`]: spawn the server, run the client to success, and
/// return `(client log, server log)`.
pub fn transfer(t: Transfer<'_>) -> (String, String) {
    let binds = free_addrs(t.paths);
    let mut sargs = vec!["--protocol-hint", t.hint, "--window-reliable"];
    sargs.extend_from_slice(t.extra_args);
    let mut senv: Vec<(&str, &str)> = t.env.to_vec();
    senv.extend_from_slice(t.server_env);
    let srv = spawn_perf_server(&binds, &senv, &sargs);
    let mut cenv: Vec<(&str, &str)> = t.env.to_vec();
    cenv.extend_from_slice(t.client_env);
    let mut cargs = perf_args(t.hint, t.bytes, t.runs).to_vec();
    cargs.extend_from_slice(t.extra_args);
    let cli = run_perf_client(&binds, &cenv, &cargs);
    let srv_log = match t.srv_tag {
        Some(tag) => srv.log_after(srv.log.len(), tag),
        None => srv.log.text(),
    };
    (cli, srv_log)
}

/// The shape most reachability tests share: one path, `c3` seeded netem on
/// the client, two runs of `bytes`, `env` on both endpoints. Returns
/// `(client log, server log)` (see [`Transfer::srv_tag`]).
pub fn lossy_run(
    env: &[(&str, &str)],
    hint: &str,
    bytes: &str,
    srv_tag: Option<&str>,
) -> (String, String) {
    transfer(Transfer { env, hint, bytes, srv_tag, ..Transfer::default() })
}

// ── in-process ───────────────────────────────────────────────────────────

/// `perf::server` / `perf::client` as tokio tasks in the test process.
pub mod in_process {
    use super::*;
    use raptorpath::config::{self, RaptorpathConfig};
    use raptorpath::net::PeerConfig;
    use raptorpath::perf;
    use tokio::task::JoinHandle;

    /// Server + client configs for one loopback pair, one path per entry of
    /// `ports` (fresh ports from [`free_port`]). Callers that need more
    /// fields (`window_out_of_order`, …) set them on BOTH returned structs
    /// before [`resolve`]. `window_reliable` is set explicitly (the ρ dial);
    /// clear it to `None` to take the named point's preset.
    pub fn cfgs(ports: &[u16], hint: &str, window_reliable: bool) -> (RaptorpathConfig, RaptorpathConfig) {
        let addrs: Vec<String> = ports.iter().map(|p| format!("127.0.0.1:{p}")).collect();
        let srv = RaptorpathConfig {
            server: Some(true),
            bind: Some(addrs.clone()),
            protocol_hint: Some(hint.into()),
            window_reliable: Some(window_reliable),
            ..Default::default()
        };
        let cli = RaptorpathConfig {
            bind: Some(vec!["127.0.0.1:0".to_string(); ports.len()]),
            peer: Some(addrs),
            protocol_hint: Some(hint.into()),
            window_reliable: Some(window_reliable),
            ..Default::default()
        };
        (srv, cli)
    }

    /// `n` distinct fresh ports.
    pub fn ports(n: usize) -> Vec<u16> {
        free_addrs(n).iter().map(|a| a.port()).collect()
    }

    pub fn resolve(cfg: &RaptorpathConfig) -> PeerConfig {
        config::resolve(cfg).expect("resolve config").0
    }

    /// Wait (≤ [`READY_DEADLINE`]) until every address in `binds` is
    /// observably bound ([`udp_bound`] — no fixed sleep). `task` is the
    /// server's task: if it ends first, its result is the failure message.
    pub async fn wait_bound<T: std::fmt::Debug>(binds: &[SocketAddr], task: &mut JoinHandle<T>, what: &str) {
        let deadline = Instant::now() + READY_DEADLINE;
        loop {
            if task.is_finished() {
                let r = task.await;
                panic!("{what}: server task exited before it was ready: {r:?}");
            }
            let probe = binds.to_vec();
            let bound = tokio::task::spawn_blocking(move || probe.iter().all(|a| udp_bound(*a)))
                .await
                .expect("bind probe");
            if bound {
                return;
            }
            assert!(Instant::now() < deadline, "{what}: server never bound {binds:?}");
            tokio::time::sleep(POLL).await;
        }
    }

    /// Spawn `perf::server(srv)` and [`wait_bound`] on its bind addresses.
    /// Returns the server task; the caller aborts it.
    pub async fn start_server(srv: PeerConfig, what: &str) -> JoinHandle<anyhow::Result<()>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let binds: Vec<SocketAddr> = srv.bind_addrs.clone();
        let mut task = tokio::spawn(perf::server(srv));
        wait_bound(&binds, &mut task, what).await;
        task
    }

    /// [`start_server`], then `perf::client(cli, bytes, runs)` inside
    /// `timeout`, then abort the server. The client only returns `Ok` after
    /// the warm-up and every run's object was acked, and the perf server acks
    /// only when every chunk is present — so `Ok` IS the all-bytes round
    /// trip. `what` names the test in failure messages.
    pub async fn run(
        srv: PeerConfig,
        cli: PeerConfig,
        bytes: usize,
        runs: u32,
        timeout: Duration,
        what: &str,
    ) {
        let task = start_server(srv, what).await;
        tokio::time::timeout(timeout, perf::client(cli, bytes, runs))
            .await
            .unwrap_or_else(|_| panic!("{what}: perf loopback timed out"))
            .unwrap_or_else(|e| panic!("{what}: perf client failed: {e:#}"));
        task.abort();
    }
}

/// The common in-process shape: one path on a fresh port, `hint`, optional
/// reliable window, `bytes` × `runs`, 60 s. Returns once the round trip
/// completed (see [`in_process::run`]).
pub async fn in_process_loopback(hint: &str, window_reliable: bool, bytes: usize, runs: u32, what: &str) {
    let (srv, cli) = in_process::cfgs(&[free_port()], hint, window_reliable);
    let (srv, cli) = (in_process::resolve(&srv), in_process::resolve(&cli));
    assert_eq!(srv.window_reliable, window_reliable, "{what}: server window_reliable");
    in_process::run(srv, cli, bytes, runs, Duration::from_secs(60), what).await;
}
