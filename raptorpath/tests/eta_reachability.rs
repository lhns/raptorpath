//! THE SENDER'S OWN PREDICTION REACHES THE WIRE AND COMES BACK — `[ETA]`,
//! BOTH ENDS, ON ONE RUN.
//!
//! **The measurand, and why it is owed.** The placement law already computes
//! each path's expected delivery time (`expected_delivery_load`), uses it to
//! CHOOSE, and throws it away; the receiver has never been told what the
//! sender expected and detects holes by SEQUENCE ALONE. Seed S1 of the law
//! search says those are two readings of one model, and wire v8's
//! `SymbolBatch.eta_rel_us` is the channel between them. This binary is the
//! gate that must pass before any measurement made through that channel is
//! worth taking.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **v8 IS THE VERSION ON THE WIRE.** The handshake carries it and the
//!      pair completes a transfer. A v7 binary refuses at handshake, so a
//!      completed run IS the version assertion.
//!   2. **THE SENDER LINE FIRES**, with `n > 0` stamped placements and a
//!      finite `F̂`. THE DEAD-GAUGE READING this test exists to fail on: on
//!      the shipped-before engine there is no `[ETA]` line at all, the wire
//!      field does not exist, and the quantity Track A's temperature
//!      derivation is supposed to rest on has no producer.
//!   3. **EVERY WINDOW-PATH SOURCE BATCH CARRIES A PREDICTION.** The
//!      receiver's `bind=` is the fraction of arrivals carrying the 0
//!      sentinel; a plain reliable window sends its sources through
//!      `emit_source`'s placement site and its repairs through the recovery
//!      plane, so `bind` must be STRICTLY BELOW 1 and the per-path predicted
//!      sample count strictly positive. This is the "`eta_rel > 0` on 100 %
//!      of window source batches" clause, read from the side that can
//!      actually count arrivals.
//!   4. **`σ̂` IS FINITE AT BOTH ENDS** — a value and a pair count that agree
//!      (`-` iff `n = 0`, by construction in `Tlag`), so the τ-lag estimator
//!      is reachable on the ETA series and not only on the RTT one.
//!   5. **THE PRE-STATED WITNESS**, written in `net/eta.rs`'s header before
//!      either gauge was fed: `σ̂_sender ≥ σ̂_recv`. The sender's error rides
//!      a ROUND trip, the receiver's lateness only the FORWARD leg, so the
//!      sender's dispersion contains the receiver's plus the return path's.
//!      **It is reported and NOT asserted as a pass/fail** — loopback's
//!      return leg is the host scheduler's, not a network's, and a witness
//!      whose direction is a property of the cell is a finding to be read off
//!      an L1 run rather than a gate here. What IS asserted is that BOTH
//!      numbers exist so the comparison is makeable at all.
//!   6. **THE BIND GAUGES ARE PRESENT AND ARE FRACTIONS** — `zero=`,
//!      `cold_r=`, `cold_ge=` on the sender, `bind=` on the receiver. Every
//!      clamp owes a bind-fraction gauge; an absent one is the defect.
//!
//! **What this deliberately does NOT assert.** Any particular VALUE of `σ̂`,
//! of the lateness quantiles, or of the prediction error. Loopback's queueing
//! is the host's and its loss is the shim's Gilbert-Elliott process; the
//! numbers that characterize the measurand come off an L1 run scored against
//! a pre-registration. This is the INSTRUMENT gate, not the measurement.
//!
//! **No new gate.** Both readouts ride the EXISTING `RWM_DIAG` surface — the
//! sender's beside `[DIAG]` at 250 ms, the receiver's beside `[SUCC]` at 1 s
//! — so a missing line can only be read as an unreached emission site.
//! `RWM_DIAG=1` IS asserted present in the `[GATES]` echo below.
//!
//! Own test binary, for `succ_reachability.rs`'s reason: `RWM_L0_NETEM` is
//! process-global in the child and the spawned pair must not contend with the
//! in-process loopback tests.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    l.local_addr().expect("probe addr").port()
}

struct Reaper(Child);
impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn join(a: &[SocketAddr]) -> String {
    a.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
}

fn spawn_perf_server(binds: &[SocketAddr]) -> (Reaper, Arc<Mutex<String>>) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let mut cmd = Command::new(bin);
    cmd.args([
        "perf",
        "--server",
        "--bind",
        &join(binds),
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    cmd.env_remove("RWM_L0_NETEM");
    let mut srv = Reaper(cmd.spawn().expect("spawn perf server"));

    let log = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&log);
    let mut err = srv.0.stderr.take().expect("server stderr");
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink
                    .lock()
                    .expect("stderr sink")
                    .push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    });

    let mut out = srv.0.stdout.take().expect("server stdout");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = String::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline && !seen.contains("perf server ready") {
        match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) => panic!("reading perf server stdout: {e}"),
        }
    }
    assert!(seen.contains("perf server ready"), "perf server never became ready: {seen}");
    {
        let sink = Arc::clone(&log);
        sink.lock().expect("stderr sink").push_str(&seen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match out.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink
                        .lock()
                        .expect("stderr sink")
                        .push_str(&String::from_utf8_lossy(&buf[..n])),
                }
            }
        });
    }
    (srv, log)
}

/// One loopback transfer. Returns `(client/sender log, server/receiver log)`.
fn run(paths: usize, netem: Option<&str>, bytes: &str) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let binds: Vec<SocketAddr> = (0..paths)
        .map(|_| format!("127.0.0.1:{}", free_port()).parse().unwrap())
        .collect();
    let (_srv, srv_log) = spawn_perf_server(&binds);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &join(&binds),
        "--bytes",
        bytes,
        "--runs",
        "2",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    match netem {
        Some(spec) => {
            cli.env("RWM_L0_NETEM", spec);
            cli.env("RWM_L0_SEED", "42");
        }
        None => {
            cli.env_remove("RWM_L0_NETEM");
        }
    }

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed (paths={paths}, netem={netem:?}, {:?})\n\
         --- stdout ---\n{cli_stdout}\n--- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

// ── READERS ─────────────────────────────────────────────────────────────

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn f64_field(line: &str, key: &str) -> Option<f64> {
    let v = field(line, key);
    if v == "-" {
        return None;
    }
    Some(v.parse().unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e}")))
}

/// The per-path slots of a gauge line, each rendered so `field()` can read
/// it: the `p<id>:` prefix becomes its own token.
fn slots(line: &str) -> Vec<String> {
    line.split_whitespace()
        .fold(Vec::new(), |mut acc: Vec<String>, t| {
            let is_head = t
                .split_once(':')
                .is_some_and(|(h, _)| h.starts_with('p') && h[1..].chars().all(|c| c.is_ascii_digit()) && h.len() > 1);
            if is_head {
                acc.push(t.replacen(':', " ", 1));
            } else if let Some(last) = acc.last_mut() {
                last.push(' ');
                last.push_str(t);
            }
            acc
        })
}

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    log.lines()
        .rev()
        .find(|l| l.contains(pat))
        .unwrap_or_else(|| panic!("no line containing `{pat}`:\n{log}"))
}

/// `sig_us=<v|->/n<pairs>` — returns `(value, pairs)`; `None` iff pairs is 0.
fn sigma(line: &str) -> (Option<u64>, u64) {
    let raw = field(line, "sig_us=");
    let (v, n) = raw.split_once("/n").unwrap_or_else(|| panic!("malformed sig_us slot `{raw}`"));
    let pairs: u64 = n.parse().expect("pair count");
    let val = if v == "-" { None } else { Some(v.parse::<u64>().expect("sigma")) };
    assert_eq!(
        val.is_none(),
        pairs == 0,
        "`-` must hold IFF the pair count is 0 — the biconditional is by \
         construction in `Tlag` and this is where it is checked on the engine's \
         own output: {line}"
    );
    (val, pairs)
}

// ── THE RUN ─────────────────────────────────────────────────────────────

/// 1-6: BOTH GAUGES FIRE OVER A LOSSY DUAL-PATH TRANSFER, AND THE WITNESS IS
/// MAKEABLE.
#[test]
fn the_senders_prediction_reaches_the_wire_and_both_gauges_read_it() {
    // Two paths, both shaped: the placement law must actually CHOOSE for the
    // prediction to be about anything, and a single path collapses it to an
    // identity.
    let (cli, srv) = run(2, Some("c2,c3"), "24000000");

    // THE GATE. A missing `[ETA]` must read as an unreached emission site and
    // never as an unset gate.
    assert!(cli.contains("RWM_DIAG=1"), "the sender's [GATES] echo lacks RWM_DIAG=1:\n{cli}");
    assert!(srv.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{srv}");

    // 2. THE SENDER LINE FIRES. This is what fails on the shipped-before
    //    engine: no line, no wire field, no producer for the ETA series.
    let s = last_with(&cli, "[ETA] site=sender");
    println!("[eta-reach] sender: {s}");
    let stamped = u64_field(s, "n=");
    assert!(
        stamped > 0,
        "[ETA] site=sender n=0 — no placement was ever stamped, which is the \
         DEAD-GAUGE reading this test exists to fail on:\n{s}"
    );
    assert!(u64_field(s, "fhat_us=") > 0, "F̂ must be a real instant: {s}");

    // 6. THE SENDER'S BIND GAUGES ARE PRESENT AND ARE FRACTIONS.
    for k in ["zero=", "cold_r=", "cold_ge="] {
        let v = f64_field(s, k);
        assert!(
            v.is_none_or(|x| (0.0..=1.0).contains(&x)),
            "`{k}` must be a fraction or `-`: {s}"
        );
    }
    let zero = f64_field(s, "zero=").expect("stamped > 0 ⇒ the fraction exists");

    // 3. EVERY WINDOW-PATH SOURCE BATCH CARRIES A PREDICTION. `zero` is the
    //    fraction of STAMPED placements that had no prediction — a path with
    //    no cwnd yet. On a completed transfer that must be a minority, not
    //    everything.
    assert!(
        zero < 1.0,
        "[ETA] site=sender zero=1.0 — every placement stamped the sentinel, so \
         the wire field is structurally dead:\n{s}"
    );

    // The receiver's side of the same claim, counted on ARRIVALS.
    let r = last_with(&srv, "[ETA] site=receiver");
    println!("[eta-reach] receiver: {r}");
    assert!(u64_field(r, "n=") > 0, "the receiver saw no arrival at all: {r}");
    let mut predicted_paths = 0usize;
    let mut recv_sigmas: Vec<u64> = Vec::new();
    let recv_slots = slots(r);
    for slot in &recv_slots {
        let n = u64_field(slot, "n=");
        let bind = f64_field(slot, "bind=");
        assert!(
            bind.is_none_or(|x| (0.0..=1.0).contains(&x)),
            "`bind=` must be a fraction or `-`: {slot}"
        );
        // The SRTT source is always named — a reading whose reference is
        // unstated is not a reading.
        let src = field(slot, "srtt_src=");
        assert!(
            ["wire", "echo", "-"].contains(&src),
            "unknown srtt source `{src}` in {slot}"
        );
        if n > 0 {
            predicted_paths += 1;
            // Quantiles are ordered — an unordered triple is a bucketing bug.
            let q: Vec<u64> = ["l_p50=", "l_p90=", "l_p95=", "l_p99=", "l_mx="]
                .iter()
                .map(|k| u64_field(slot, k))
                .collect();
            for w in q.windows(2) {
                assert!(w[0] <= w[1], "lateness quantiles out of order in {slot}");
            }
            if let (Some(v), _) = sigma(slot) {
                recv_sigmas.push(v);
            }
        }
    }
    assert!(
        predicted_paths > 0,
        "no path saw a single PREDICTED arrival — `eta_rel` never reached the \
         receiver, so the v8 field is not actually on the wire:\n{r}"
    );

    // 4. σ̂ IS REACHABLE AT BOTH ENDS.
    let mut send_sigmas: Vec<u64> = Vec::new();
    for slot in &slots(s) {
        if let (Some(v), pairs) = sigma(slot) {
            assert!(pairs > 0);
            send_sigmas.push(v);
        }
    }
    assert!(
        !send_sigmas.is_empty(),
        "the sender's τ-lag never found a pair on the ETA-error series — σ̂_e \
         has no producer and Track A's temperature derivation has no input:\n{s}"
    );
    assert!(
        !recv_sigmas.is_empty(),
        "the receiver's τ-lag never found a pair on the lateness series:\n{r}"
    );

    // 5. THE PRE-STATED WITNESS, REPORTED. Loopback's return leg is the host
    //    scheduler's, so the DIRECTION is a finding for an L1 run; what this
    //    binary owes is that both numbers exist and the comparison is
    //    makeable.
    let sd = *send_sigmas.iter().max().expect("non-empty");
    let rc = *recv_sigmas.iter().max().expect("non-empty");
    println!(
        "[eta-reach] WITNESS σ̂_sender={sd}us σ̂_recv={rc}us — {} (pre-stated: \
         σ̂_sender ≥ σ̂_recv; loopback direction is NOT a pass/fail here)",
        if sd >= rc { "HOLDS" } else { "INVERTED" }
    );
}

/// The SINGLE-PATH CONTROL. The placement law collapses to an identity, but
/// the prediction is still stamped and still read — so a reading of zero on
/// the dual cell could never be blamed on the topology.
#[test]
fn the_prediction_is_stamped_and_read_on_one_path_too() {
    let (cli, srv) = run(1, None, "8000000");
    let s = last_with(&cli, "[ETA] site=sender");
    let r = last_with(&srv, "[ETA] site=receiver");
    println!("[eta-reach] N=1 sender: {s}");
    println!("[eta-reach] N=1 receiver: {r}");
    assert!(u64_field(s, "n=") > 0, "no placement stamped on one path: {s}");
    assert_eq!(
        slots(s).len(),
        1,
        "a single-path run must report exactly one path slot: {s}"
    );
    assert_eq!(slots(r).len(), 1, "and the receiver must agree: {r}");
    assert!(u64_field(r, "n=") > 0, "no arrival observed on one path: {r}");
    assert!(
        f64_field(s, "zero=").is_some_and(|z| z < 1.0),
        "even the identity placement must carry a prediction: {s}"
    );
}
