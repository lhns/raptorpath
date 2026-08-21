//! **THE QUANTILE-NATIVE `W` IS REACHABLE, AND THE ABSENT ARM IS UNCHANGED.**
//!
//! Paper §16.76. Two batteries failed to find a `σ̂` this tree can put in
//! `W = mean + k(α)·σ̂`, and the second one measured why the search was
//! misdirected: the shipped estimator supplies a **conditional** spread at
//! 3–5 % of the **marginal** dispersion the Cantelli form requires (20–300× at
//! seven of eight sender legs), and the marginal quantity is itself
//! regime-dominated — one rep in eighty moved pooled `R_total` by 33×.
//! §16.76's answer is to remove the term rather than estimate it:
//!
//! ```text
//!   W_q(α) = X_(N(α)−K+1) ,   N(α) = max(⌈K/α⌉, 2K) ,   K = 10
//! ```
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **THE ARM ARMS AND ECHOES, TWO-SIDED.** `[GATES] RWM_W_FORM=quantile`
//!      at BOTH endpoints on the armed arm; `RWM_W_FORM=cantelli` on the
//!      absent arm. The RESOLVED token, never a flag — the
//!      `RWM_ALPHA_OVERRIDE` precedent, and the 31 Mbit/s anomaly's own
//!      failure mode is what it is for.
//!   2. **THE DIAL ROUTES.** `[QALPHA] form=quantile win_n=<N(α)>` at both
//!      sites, with `win_n` equal to the paper's published `N(α)` for the
//!      arm's own α. MEASUREMENT DISCIPLINE rule 1: prove the mechanism under
//!      test executes, and prove the DIAL routed to it.
//!   3. **THE LAW FIRES AND ITS PROVENANCE IS PER INVOCATION.**
//!      `[QCLK] form=quantile win_n=<N(α)> win_ok=<n> law_n>0`, with an
//!      ordered `W` distribution — so a row states which of the two rival laws
//!      produced its clock off its own log.
//!   4. **THE α-REACHABILITY GATE** the sweep's pre-registration inherits:
//!      `[RACK] fired > 0` and `[DIAG] retx > 0` over a lossy transfer. A row
//!      reading zero is VOID, not a small number.
//!   5. **THE ABSENT ARM IS BYTE-IDENTICAL.** With `RWM_W_FORM` unset the echo
//!      reads `cantelli`, the realized `W` still tracks `srtt + k(α)·σ`, and
//!      the existing Cantelli assertions hold unchanged.
//!   6. **GARBAGE RESOLVES BACK TO ABSENT, VISIBLY** — the echo reads
//!      `cantelli` and today's law is in force.
//!   7. **THE TWO FORMS REALIZE DIFFERENT CLOCKS** on the same cell, same
//!      seed, same binary. This is the clause that makes the form a treatment
//!      rather than a label, and it is asserted here rather than assumed in a
//!      results table.
//!
//! **THIS BINARY FAILS ON THE PRE-CHANGE ENGINE**: `RWM_W_FORM`, `form=` and
//! `win_n=` do not exist there, so every clause above reads a missing field.
//!
//! **What this binary deliberately does NOT assert.** Any FIELD value of `W`,
//! of the false-alarm fraction, or of goodput. Loopback's dispersion is the
//! host scheduler's and its loss is the shim's GE process; no claim about any
//! cell can be made from it. This is the instrument gate that must pass before
//! the L1 re-run is worth making.
//!
//! **Nothing here flips a default.** `RWM_QUANTILE_CLOCKS` stays OFF and
//! REFUTED-STANDING, `RWM_W_FORM` defaults to `cantelli`, and nothing shipped
//! reads either.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The base arm. `RWM_DIAG` carries `[DIAG] retx=` and the receiver's periodic
/// `[QCLK]` readouts. No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The swept arm's α. `N(0.05) = ⌈10/0.05⌉ = 200` — the paper's own grid row,
/// chosen here because 200 samples fill on a loopback transfer in milliseconds
/// while `Q002`'s 5 000 is the arm §16.76.5 predicts UNSCOREABLE at a sparse
/// leg. **The window law is pinned absolutely in `recovery_bench.rs`; this
/// binary pins that the ENGINE ROUTES to it.**
const ALPHA: &str = "0.05";
const ALPHA_F: f64 = 0.05;
const WIN_N: &str = "200";

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

/// Spawn the perf SERVER — the RECEIVER of the bulk direction, and one of the
/// two sites that owns a quantile clock.
fn spawn_perf_server(extra: &[(&str, &str)]) -> (SocketAddr, Reaper, Arc<Mutex<String>>) {
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
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.env_remove("RWM_L0_NETEM");
    // The absent arm must be ABSENT: inheritance defeats an allowlist, and the
    // whole point of a `cantelli` control is that nothing set it.
    if !extra.iter().any(|(k, _)| *k == "RWM_W_FORM") {
        cmd.env_remove("RWM_W_FORM");
    }
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
    assert!(
        seen.contains("perf server ready"),
        "perf server never became ready; it said: {seen}"
    );
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
    (addr, srv, log)
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

/// Keep the leading numeric prefix of a token — stderr has two writers and a
/// `tracing` write can land inside a gauge line's LAST field. Every gauge here
/// ends on the constant `fa_class=`, and this is the reader's half.
fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(v.len());
    &v[..end]
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn f64_field(line: &str, key: &str) -> f64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn require<'a>(log: &'a str, tag: &str, what: &str) -> &'a str {
    log.lines()
        .rev()
        .find(|l| l.contains(tag))
        .unwrap_or_else(|| panic!("no `{tag}` line — {what}\n--- log ---\n{log}"))
}

/// One lossy loopback run in the given gate configuration.
/// Returns `(client/sender log, server/receiver log)`.
fn lossy_run(extra: &[(&str, &str)]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv, srv_log) = spawn_perf_server(extra);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "4000000",
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
    for (k, v) in extra {
        cli.env(k, v);
    }
    if !extra.iter().any(|(k, _)| *k == "RWM_W_FORM") {
        cli.env_remove("RWM_W_FORM");
    }
    // The L1 `c3` cell (LTE-class) on client egress, seeded. Loss is what
    // drives the recovery clock this test is about at all.
    cli.env("RWM_L0_NETEM", "c3");
    cli.env("RWM_L0_SEED", "42");

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed ({extra:?}, {:?})\n--- stdout ---\n{cli_stdout}\n\
         --- stderr ---\n{cli_stderr}",
        out.status
    );
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

/// The MAXIMUM `retx=<n>` the sender printed. Read as a max over lines and
/// never off the last one — `retx=` in the `[DIAG]` tail is an INTERVAL
/// counter, and reading it off the last line made the plain-window pass report
/// `W4` failing at 5 of 15 reps whose `[RACK] fired` was 11–5 717.
fn max_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| {
            v.trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

/// The realized-`W` p50 at a site, in µs, plus the ARM'S OWN law's count.
/// `law_n`, never `evals`: only the arm's own law's realizations belong to it.
fn qclk(log: &str, site: &str) -> (u64, u64) {
    let l = require(
        log,
        &format!("[QCLK] site={site}"),
        "the recovery clock was never evaluated at this site, or the gauge is unreached",
    );
    (u64_field(l, "w_us_p50="), u64_field(l, "law_n="))
}

// ── 1 — THE ARMED ARM: set, echoed, routed, realized, provenanced ────────

#[test]
fn the_quantile_native_form_arms_echoes_routes_and_reports_its_own_window() {
    let (cli, srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_W_FORM", "quantile"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);

    // (1) THE GATE ECHO, both endpoints, two-sided. A missing gauge below can
    // then only be read as an unreached emission site, never as an unset gate.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_QUANTILE_CLOCKS=1"),
            "{site}: the outer quantile gate did not arm:\n{gates}"
        );
        assert!(
            gates.contains("RWM_W_FORM=quantile"),
            "{site}: `[GATES]` must echo the RESOLVED W law, not a flag — the \
             RWM_ALPHA_OVERRIDE precedent, and the 31 Mbit/s anomaly's own \
             failure mode:\n{gates}"
        );
    }

    // (2) THE DIAL ROUTED, AND `win_n` IS THE PAPER'S PUBLISHED N(α).
    // `[GATES]` says what was ASKED FOR; `[QALPHA]` says what the LAW
    // evaluates, at the site that evaluates it — and the window law's own
    // output is on that line because the arm's separability is derived from it
    // (§16.76.8) before the run.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let q = require(
            log,
            &format!("[QALPHA] site={site}"),
            "the resolved-alpha echo is unreached",
        );
        assert_eq!(field(q, "quantile="), "1", "{site}: {q}");
        assert_eq!(
            field(q, "form="),
            "quantile",
            "{site}: the W law must be readable at the seat that runs it: {q}"
        );
        assert_eq!(
            field(q, "win_n="),
            WIN_N,
            "{site}: N(alpha) must equal the paper's published window length — \
             the UNSEPARATED-BY-CONSTRUCTION set is derived from it: {q}"
        );
        assert!(
            (f64_field(q, "alpha=") - ALPHA_F).abs() < 1e-9,
            "{site}: the override did not reach the law: {q}"
        );
    }

    // (3) THE LAW FIRED, AND ITS PROVENANCE IS PER INVOCATION.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let l = require(
            log,
            &format!("[QCLK] site={site}"),
            "the clock gauge is unreached",
        );
        assert_eq!(field(l, "on="), "1", "{site}: {l}");
        assert_eq!(
            field(l, "form="),
            "quantile",
            "{site}: the realized-W line must name which of the two rival laws \
             produced it — provenance per invocation, never per driver table: {l}"
        );
        assert_eq!(field(l, "win_n="), WIN_N, "{site}: {l}");
        let evals = u64_field(l, "evals=");
        let law_n = u64_field(l, "law_n=");
        let win_ok = u64_field(l, "win_ok=");
        assert!(evals > 0, "{site}: the recovery clock was never evaluated: {l}");
        // THE UNSCOREABLE COUNTER (§16.76.5(1)). An evaluation whose window is
        // shorter than N(alpha) falls through to the law below — information
        // availability, never a mode — and that number belongs to a DIFFERENT
        // law. `win_ok/evals` is the bind fraction CLAUDE.md's FORMULA-FIRST
        // rule owes any clamp, and it is what will say whether c8-Q002 was
        // measurable at all.
        assert!(
            law_n > 0,
            "{site}: the quantile-native law never produced a single clock — \
             every evaluation fell through with a short window: {l}"
        );
        assert_eq!(
            win_ok, law_n,
            "{site}: on the quantile form `win_ok` and `law_n` count the same \
             event and must agree, or the bind-fraction gauge is lying: {l}"
        );
        assert!(win_ok <= evals, "{site}: a bind fraction above 1: {l}");
        let (p05, p50, p95) = (
            u64_field(l, "w_us_p05="),
            u64_field(l, "w_us_p50="),
            u64_field(l, "w_us_p95="),
        );
        let (wmin, wmax) = (u64_field(l, "w_us_min="), u64_field(l, "w_us_max="));
        assert!(p05 <= p50 && p50 <= p95, "{site}: W quantiles are not ordered: {l}");
        assert!(
            wmin <= p05 && p95 <= wmax,
            "{site}: W quantiles escape their own range: {l}"
        );
        assert!(
            wmin > 0,
            "{site}: W floors at the timer granularity, never at zero: {l}"
        );
    }

    // (4) THE α-REACHABILITY GATE (MEASUREMENT DISCIPLINE rule 1). The clock's
    // two consumers drive the machinery `fired` and `retx` count. If they read
    // zero the curve is mechanical and the rep is VOID, not a small number.
    let rack = require(&cli, "[RACK]", "the sender's recovery gauge never emitted");
    let fired: u64 = field(rack, "fa=")
        .split('/')
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("`fa=<spur>/<fired>` unreadable: {rack}"));
    assert!(
        fired > 0,
        "the recovery clock never fired — the quantile-native law is unreachable: {rack}"
    );
    assert!(
        max_retx(&cli) > 0,
        "no retransmit ran — the machinery this clock parameterises did not execute"
    );
}

// ── 2 — THE ABSENT ARM IS CANTELLI, AND IT SAYS SO ───────────────────────

#[test]
fn without_the_form_the_cantelli_law_stands_and_the_echo_says_cantelli() {
    // The outer gate armed, the FORM absent: this is the exact configuration
    // every previously-committed quantile-arm row ran in, and it must be
    // byte-identical to before `RWM_W_FORM` existed.
    let (cli, srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);

    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_W_FORM=cantelli"),
            "{site}: an ABSENT form must print `cantelli`, so `my arm did not \
             take` is READ and not inferred: {gates}"
        );
        let q = require(
            log,
            &format!("[QALPHA] site={site}"),
            "the resolved-alpha echo must fire on EVERY arm",
        );
        assert_eq!(field(q, "form="), "cantelli", "{site}: {q}");
        // `win_n` is still printed on the Cantelli arm — two-sided, so an
        // absent window is as checkable as a present one, and so the CONTROL's
        // own row can state what window the rival law would have wanted.
        assert_eq!(field(q, "win_n="), WIN_N, "{site}: {q}");
        // k(0.05) = sqrt(0.95/0.05) = 4.3589 — the Cantelli law is unchanged.
        assert!(
            (f64_field(q, "k=") - 4.3589).abs() < 1e-3,
            "{site}: the absent arm must still evaluate Cantelli's own k: {q}"
        );
    }

    // The absent arm still reports its realized clock, and it is Cantelli's.
    let l = require(&cli, "[QCLK] site=sender", "the clock gauge is unreached");
    assert_eq!(field(l, "form="), "cantelli", "{l}");
    assert!(u64_field(l, "law_n=") > 0, "the Cantelli law must still run: {l}");
    // `win_ok` counts window availability on EVERY arm — the control's own
    // window fill is what says whether the treatment arm COULD have run, and
    // it is therefore printed on the control too (MEASUREMENT DISCIPLINE 15).
    assert!(u64_field(l, "win_ok=") > 0, "{l}");
}

// ── 3 — GARBAGE RESOLVES TO ABSENT, VISIBLY ──────────────────────────────

#[test]
fn a_garbage_w_form_resolves_back_to_cantelli_and_prints_cantelli() {
    // ONE full lossy transfer on the representative garbage value: the whole
    // path — gate, resolution, law, gauge — has to hold, not just the parse.
    let (cli, _srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_W_FORM", "quantle"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);
    let gates = require(&cli, "[GATES]", "the engine never echoed its gates");
    assert!(
        gates.contains("RWM_W_FORM=cantelli"),
        "`quantle` must resolve back to ABSENT and print `cantelli` — a \
         mistyped arm that silently ran the other law is the 31 Mbit/s \
         anomaly's failure mode: {gates}"
    );
    let q = require(&cli, "[QALPHA] site=sender", "the resolved echo is unreached");
    assert_eq!(field(q, "form="), "cantelli", "{q}");
    let c = require(&cli, "[QCLK] site=sender", "the clock gauge is unreached");
    assert!(
        u64_field(c, "law_n=") > 0,
        "`quantle`: the fallen-back arm still runs a law and must report it: {c}"
    );

    // The rest of the domain, on a SERVER SPAWN ALONE. The gate resolves at
    // engine start and the echo is emitted there, so no transfer is needed to
    // read it — and five back-to-back lossy loopbacks in one test is five
    // chances at an unrelated harness flake for one fact each.
    for bad in ["", " ", "1", "true", "QUANTILE!", "cantelli-ish"] {
        let (_addr, _srv, log) = spawn_perf_server(&[
            ("RWM_QUANTILE_CLOCKS", "1"),
            ("RWM_W_FORM", bad),
        ]);
        std::thread::sleep(Duration::from_millis(300));
        let l = log.lock().expect("stderr sink").clone();
        let gates = require(&l, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_W_FORM=cantelli"),
            "`{bad}` must resolve back to ABSENT and print `cantelli`: {gates}"
        );
    }
}

// ── 4 — AT THE CONTRACT'S OWN α THE DIRECT ROUTE DECLINES, VISIBLY ───────

#[test]
fn at_the_contracts_own_alpha_the_window_is_unavailable_and_the_echo_says_so() {
    // §16.69 REASON 2, MADE VISIBLE RATHER THAN SILENTLY EXTRAPOLATED. The
    // contract's own α at `bulk` is 1e-3, so N(α) = 10 000 — above the declared
    // 8192 cap. The law must DECLINE and say `unavail`; it must never truncate
    // to the deepest window it happens to hold, because a quantile read off a
    // shorter window is a different law's output at a different level.
    let (cli, _srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_W_FORM", "quantile"),
    ]);
    let q = require(&cli, "[QALPHA] site=sender", "the resolved echo is unreached");
    assert_eq!(
        field(q, "win_n="),
        "unavail",
        "at the contract's own alpha the direct quantile needs more samples \
         than the declared cap holds, and the echo must SAY so — this is \
         16.69 reason 2, reproducible rather than asserted: {q}"
    );
    let c = require(&cli, "[QCLK] site=sender", "the clock gauge is unreached");
    assert_eq!(field(c, "win_n="), "unavail", "{c}");
    assert_eq!(
        u64_field(c, "win_ok="),
        0,
        "an unavailable window can never report a satisfied evaluation: {c}"
    );
    // AND THE ARM STILL RAN A CLOCK — it fell through to the law below, which
    // is information availability and not a mode. `law_n = 0` on this arm is
    // the VOID condition the battery pre-registers, and it is `evals` that
    // must be positive here: the site was reached, the arm's own law was not.
    assert!(
        u64_field(c, "evals=") > 0,
        "the evaluation site must still be reached: {c}"
    );
    assert_eq!(
        u64_field(c, "law_n="),
        0,
        "with its window unavailable the quantile-native law produced NOTHING, \
         and the gauge must say so rather than pooling another law's clocks \
         into this arm's distribution: {c}"
    );
}

// ── 5 — THE FORM IS A TREATMENT, NOT A LABEL ─────────────────────────────

#[test]
fn the_two_w_forms_realize_different_clocks_on_the_same_cell() {
    // Same α, same cell, same seed, same binary — only the LAW differs.
    // Cantelli at α = 0.05 is `srtt + 4.359·σ`; the quantile-native form is the
    // 10th largest of 200 raw RTT samples. If those cannot be told apart here,
    // no L1 arm list tells them apart either, and the re-run would be reading a
    // label. CLAUDE.md's testing-discipline rule: assert the WIRING routes, not
    // that A is ordinally more than B.
    let (cant, _) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);
    let (qnat, _) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_W_FORM", "quantile"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);

    let (w_cant, n_cant) = qclk(&cant, "sender");
    let (w_qnat, n_qnat) = qclk(&qnat, "sender");
    assert!(
        n_cant > 0 && n_qnat > 0,
        "one of the two forms never produced its own clock (cantelli law_n={n_cant}, \
         quantile law_n={n_qnat}) — the comparison would be between one law and \
         a fall-through"
    );
    assert_ne!(
        w_cant, w_qnat,
        "the two W laws realized the SAME p50 clock ({w_cant} us) at one alpha \
         on one cell. RWM_W_FORM would be a LABEL, not a treatment."
    );
}
