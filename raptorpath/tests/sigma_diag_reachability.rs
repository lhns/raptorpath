//! σ IS REPORTED — the `[DIAG]` line's `sig_us=<µs>/n<count>` field.
//!
//! **The defect this repairs.** `Path::rtt_sigma_us()` — §16.69's second
//! moment, `√(EWMA[(rtt − srtt)²])` at RFC 6298's own β = 1/4 — has been
//! computed on every arm since it was written, and the engine's own comment
//! next to the feed site says what became of it: *"Fed unconditionally; read
//! by nothing on the default arm."* Its only consumers sit behind
//! `RWM_QUANTILE_CLOCKS`, default OFF and REFUTED-STANDING.
//!
//! The cost of that is on the record. §16.69 derived `W(α) = srtt + k(α)·σ`
//! against a **working value** `σ ≈ 10 ms` at c8, because no measured σ
//! existed to use. The cost-ratio memo's §2.3 then had to *invert Cantelli*
//! against the shipped `fa_frac` to estimate σ — pairing a receiver-site
//! clock with a sender-site statistic, inverting an inequality, and reporting
//! a lower bound as a point value — and got ≈ 18.1 ms at c8, about 1.8× the
//! assumed figure. Every option in that memo takes σ as an input. The repair
//! is one print statement, and this binary asserts that the print statement
//! is REACHED and FED.
//!
//! **Why a spawned binary and not a unit test.** `[DIAG]` is an `eprintln!`
//! from inside the sender loop, on a surface gated by `RWM_DIAG`. A unit test
//! can pin the accessor and does (`scheduler`'s own tests); only a run of the
//! shipped binary can show that the field exists in a log an L1 parser will
//! scrape. That is MEASUREMENT DISCIPLINE rule 1 — prove the mechanism under
//! test executes — and it is the same lesson `gauge_reachability.rs` records
//! one layer up: every `[CCAP]` pin asserted the format and none asked whether
//! the line fired.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. The two-sided gate echo: `RWM_DIAG=1` present, `RWM_DIAG=0` absent.
//!   2. `[DIAG]` fires at all, with a per-path block.
//!   3. Every per-path block carries `sig_us=<µs|->/n<count>` — the field's
//!      EXISTENCE, which is what fails on the shipped-before engine.
//!   4. At least one late block reports a σ that is **parsed, positive, and
//!      finite**, fed by a sample count that is not the EWMA's seed. This is
//!      the "nonzero on a loopback transfer with jitter" clause: loopback has
//!      no netem, but an app-echo RTT over a real scheduler, a real store and
//!      a real ack path is not constant, and `rtt_var_sq` is the variance of
//!      exactly that series. A σ that read 0 over thousands of samples would
//!      mean the EWMA is not being fed, which is the failure this asserts
//!      against.
//!   5. σ is BOUNDED by the RTT it is a dispersion of — a gauge printing a
//!      wildly out-of-scale number (a unit error between µs and s, the most
//!      likely mistake in this change) is caught here rather than in a
//!      battery's results table.
//!
//! **What this binary deliberately does NOT assert.** Any particular VALUE of
//! σ. Loopback's dispersion is the host scheduler's, not a network's, and no
//! claim about c8's σ can be made from it. The measurement that supersedes
//! §16.69's assumed 10 ms is an L1 run; this is the instrument gate that must
//! pass before that run is worth making.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The arm: the DIAG surface on, window-reliable, honest anchors — the same
/// composition every L1 battery arm runs. No gate here changes a law.
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

fn spawn_perf_server() -> (SocketAddr, Reaper) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let mut cmd = Command::new(bin);
    cmd.args([
        "perf",
        "--server",
        "--bind",
        &addr.to_string(),
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    let mut srv = Reaper(cmd.spawn().expect("spawn perf server"));

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
    assert!(
        seen.contains("perf server ready"),
        "perf server never became ready; it said: {seen}"
    );
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = out.read_to_end(&mut sink);
    });
    (addr, srv)
}

/// Parse ONE per-path `sig_us=<µs|->/n<count>` token into (σ µs, n).
/// `None` for the `-` (no sample yet) case, which is a legitimate reading and
/// not a parse failure.
fn parse_sig(tok: &str) -> (Option<u64>, u64) {
    let v = tok.strip_prefix("sig_us=").expect("caller filters on the prefix");
    let (sig, n) = v.split_once("/n").unwrap_or_else(|| {
        panic!("sig_us= must render as `<µs|->/n<count>`, got `{tok}`")
    });
    let n: u64 = n
        .parse()
        .unwrap_or_else(|e| panic!("sig_us= sample count `{n}` does not parse: {e}"));
    if sig == "-" {
        return (None, n);
    }
    let sig: u64 = sig
        .parse()
        .unwrap_or_else(|e| panic!("sig_us= value `{sig}` does not parse: {e}"));
    (Some(sig), n)
}

#[test]
fn the_diag_line_reports_the_rtt_sigma_the_recovery_clock_needs() {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv) = spawn_perf_server();

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "8000000",
        "--runs",
        "2",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    let out = cli.output().expect("run perf client");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let log = format!("{stdout}\n{stderr}");
    assert!(
        out.status.success(),
        "perf client failed ({:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status
    );

    // 1. THE GATE, TWO-SIDED. A missing `[DIAG]` must be readable as an
    //    unreached emission site and never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the [GATES] echo does not carry RWM_DIAG=1 — the arm did not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );

    // 2. THE LINE FIRES, with per-path blocks.
    let diag: Vec<&str> = log.lines().filter(|l| l.contains("[DIAG] ")).collect();
    assert!(
        !diag.is_empty(),
        "no [DIAG] line in a run with RWM_DIAG=1 — the report is unreachable:\n{log}"
    );

    // 3. THE FIELD EXISTS on every per-path block. THIS IS THE ASSERTION THAT
    //    FAILS ON THE SHIPPED-BEFORE ENGINE: σ was computed and discarded.
    let mut sigs: Vec<(Option<u64>, u64)> = Vec::new();
    let mut blocks = 0usize;
    for line in &diag {
        let toks: Vec<&str> = line.split_whitespace().collect();
        // A per-path block is identified by its OWN clock token,
        // `rtt=<app>/wrtt=<wire>/rtp<floor>ms`. The `/wrtt=` is what
        // distinguishes it from the line's AGGREGATE `rtt=<ms>ms` field —
        // a `starts_with("rtt=")` test matches both, and a `[DIAG]` line
        // emitted with `np=0` (no live path yet) has the aggregate and no
        // block at all, which is a legitimate reading and not a missing gauge.
        let n_sig = toks.iter().filter(|t| t.starts_with("sig_us=")).count();
        let n_rtp = toks
            .iter()
            .filter(|t| t.starts_with("rtt=") && t.contains("/wrtt="))
            .count();
        if n_rtp == 0 {
            continue;
        }
        blocks += n_rtp;
        // Every per-path block must carry the field. A gauge present on some
        // paths and not others is worse than absent: a parser would average
        // over a biased subset without knowing it.
        assert_eq!(
            n_sig, n_rtp,
            "[DIAG] carries {n_rtp} per-path RTT blocks but {n_sig} sig_us= \
             fields — σ is missing from at least one path: {line}"
        );
        for t in toks.iter().filter(|t| t.starts_with("sig_us=")) {
            sigs.push(parse_sig(t));
        }
    }
    assert!(
        blocks > 0,
        "no per-path [DIAG] block in the whole log — nothing to read σ off:\n{log}"
    );

    // 4. AND IT IS FED. Over a multi-megabyte transfer the sender takes
    //    thousands of RTT samples; a σ that never became positive would mean
    //    the EWMA is not reached, which is exactly the defect being repaired.
    let best = sigs
        .iter()
        .filter_map(|(s, n)| s.map(|s| (s, *n)))
        .max_by_key(|(_, n)| *n);
    let (sigma_us, n) = best.unwrap_or_else(|| {
        panic!(
            "every [DIAG] sig_us= read `-` over {} samples of the field — the \
             σ EWMA was never fed:\n{}",
            sigs.len(),
            diag.join("\n")
        )
    });
    println!("[sigma-diag] best σ reading: sig_us={sigma_us} n={n} over {blocks} path-blocks");
    assert!(
        sigma_us > 0,
        "σ read 0 µs at n={n} — an RTT series with literally zero dispersion \
         over a whole transfer is not a measurement, it is an unfed gauge"
    );
    assert!(
        n > 8,
        "the σ EWMA folded only {n} samples over the whole transfer — the \
         count is the gauge's own warm-up evidence and it says the feed site \
         is barely reached"
    );

    // 5. SCALE. σ is a dispersion of the RTT, so it cannot plausibly exceed a
    //    second on loopback; this catches the µs/s unit error the change is
    //    most likely to make, at the instrument rather than in a results table.
    assert!(
        sigma_us < 1_000_000,
        "σ = {sigma_us} µs on loopback is not a dispersion of a loopback RTT — \
         suspect a unit error in the gauge"
    );
}
