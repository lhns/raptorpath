//! **α IS REACHABLE, AND THE CLOCK IT PRODUCES IS REPORTED.**
//!
//! Goal #100 item 2 is an ISOLATION EXPERIMENT: hold everything fixed and
//! sweep the recovery clock's false-alarm rate α, the one variable, so the
//! cost curve `latency(α)` vs `goodput(α)` can be read directly rather than
//! argued. Three things had to exist before that run was worth making, and
//! none of them did:
//!
//!   1. **A way to SET α.** `pol.contract_alpha` is `target_tail_loss × ζ(hint)`
//!      and nothing else — three points, 1e-7 / 1e-5 / 1e-3, all of them in the
//!      region paper §16.69's reasons 1 and 2 refute. The cost-ratio memo
//!      (`docs/research/cost-ratio-memo.md`) evaluates four candidate mappings
//!      between 0.008 and 0.99, **recommends none**, and shows that three of
//!      them are three points on the fourth's curve. The measurement that
//!      adjudicates them is a sweep of α — so α needs an input.
//!   2. **A way to READ the α a row actually ran at.** `[GATES]` can only say
//!      what was ASKED FOR. The 31 Mbit/s anomaly is on this tree's record as
//!      what happens when a configuration axis has no echo: two scored passes
//!      measured a different machine from the one they compared against, for
//!      months, and `[GATES] RWM_GEN` looked exactly like the witness it was
//!      not.
//!   3. **A way to read the clock α actually PRODUCED.** `W(α) = srtt + k(α)·σ`
//!      is commanded by α and realized through σ, and σ is not a constant:
//!      the plain-window primitives pass measured `σ(c8)` at 0.191 / 3.140 /
//!      54.836 ms across three reps at converged `n` — **287×** — and recorded
//!      that spread as its largest open item. **W is therefore a DISTRIBUTION,
//!      two arms commanded at different α can realize overlapping W, and a
//!      sweep scored on commanded α alone is scored on a label.** Before this
//!      binary, the quantile arm fed no gauge at all: `[RACK]`'s `round=` is a
//!      mean over evaluations and is written only under `RWM_RACK_CLOCKS`.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. The two-sided gate echo: `RWM_QUANTILE_CLOCKS=1` and
//!      `RWM_ALPHA_OVERRIDE=<value>` present on the armed arm;
//!      `RWM_QUANTILE_CLOCKS=0` and `RWM_ALPHA_OVERRIDE=unset` on the control.
//!   2. `[QALPHA]` at BOTH sites, with the RESOLVED α equal to the override on
//!      the armed arm and to the contract's own 1e-3 (bulk) on the control —
//!      the routing gate. MEASUREMENT DISCIPLINE rule 1: prove the mechanism
//!      under test executes, and prove the DIAL routed to it.
//!   3. `[QCLK]` present with `evals > 0` and a `W` distribution that is
//!      populated and ordered (`p05 ≤ p50 ≤ p95`, `min ≤ mean ≤ max`).
//!   4. The α-reachability gate the sweep's own pre-registration inherits:
//!      the machinery α parameterises actually ran — `[RACK] fired > 0` and
//!      `[DIAG] retx > 0` over a lossy transfer.
//!   5. **The law moved the clock.** A LARGE α and a SMALL α on the same cell,
//!      same seed, same binary must not produce the same realized `W`. This is
//!      the clause that makes the sweep's independent variable a treatment
//!      rather than a label, and it is asserted here rather than assumed in a
//!      results table.
//!   6. Garbage in the override resolves back to ABSENT **visibly** — the
//!      echo reads `unset` and the contract's α is in force.
//!
//! **What this binary deliberately does NOT assert.** Any FIELD value of α, of
//! `W`, of the false-alarm fraction, or of goodput. Loopback's dispersion is
//! the host scheduler's and its loss is the shim's GE process; no claim about
//! any cell can be made from it. This is the instrument gate that must pass
//! before the L1 sweep is worth making — the `sigma_diag_reachability` /
//! `rfa_reachability` pattern, one layer on.
//!
//! **Nothing here flips a default.** `RWM_QUANTILE_CLOCKS` stays OFF and
//! REFUTED-STANDING; `RWM_ALPHA_OVERRIDE` is ABSENT unless a battery arms it
//! per invocation, and nothing shipped reads it.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The base arm. `RWM_DIAG` carries `[DIAG] retx=`, `sig_us=` and the
/// receiver's periodic `[QCLK]`/`[RFA]` readouts. No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// The contract's own α at the `bulk` hint — `target_tail_loss × ζ` =
/// 1e-5 × 100. What the SENDER's control arm MUST resolve to, so "the override
/// did not take" and "the override took" are separate readings.
const CONTRACT_ALPHA_BULK: f64 = 1e-3;

/// The contract's own α at the RECEIVER, which is the **Auto** point at every
/// hint: the protocol hint is not plumbed to the receiver task, a stated
/// limitation of the refuted arm (`receiver.rs`). **The two sites therefore
/// DISAGREE about the contract's α — and an OVERRIDE, being a number rather
/// than a hint mapping, reaches both sites identically and removes that
/// disagreement.** Asserted here rather than described, because it is the
/// reason a SWEPT arm is better defined across sites than the contract arm it
/// is compared against, and a reader of the sweep needs to know it.
const CONTRACT_ALPHA_RECV: f64 = 1e-5;

/// The contract's α at the site under test.
fn contract_alpha_at(site: &str) -> f64 {
    if site == "sender" {
        CONTRACT_ALPHA_BULK
    } else {
        CONTRACT_ALPHA_RECV
    }
}

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
/// two sites that owns a quantile clock. Its stderr is drained on a thread so
/// the periodic gauge lines are captured while it runs.
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
    // The shim shapes the CLIENT's egress; the ack direction is left clean so
    // acks are not the thing under test.
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
    assert!(
        seen.contains("perf server ready"),
        "perf server never became ready; it said: {seen}"
    );
    // The `[GATES]` echo and the gauges do not share a stream, so BOTH streams
    // land in the one sink — otherwise a missing `[QCLK]` could not be told
    // apart from an unset gate.
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

/// Parse `key=<value>` out of one whitespace-tokenised gauge line.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

/// Keep the leading numeric prefix of a token.
///
/// **stderr HAS TWO WRITERS AND THEY INTERLEAVE.** The gauges are `eprintln!`
/// and the mechanism echoes are `tracing::info!`, and a concurrent write can
/// land inside a gauge line's LAST field — observed here as
/// `k=2.1059<ansi-escape><timestamp>` on one run. The engine's answer is to
/// end every gauge line on a CONSTANT the parser already knows (`fa_class=`,
/// the convention `[RACK]` and `[RFA]` already follow); this is the reader's
/// half of the same defence, and any L1 parser of these lines needs it too.
fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
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

/// The LAST line carrying `tag`, per the cumulative-counter convention every
/// gauge in this tree shares (`[RFA]`, `[WIDLE]`, `[FDIAG]`).
fn last_line<'a>(log: &'a str, tag: &str) -> Option<&'a str> {
    log.lines().rev().find(|l| l.contains(tag))
}

fn require<'a>(log: &'a str, tag: &str, what: &str) -> &'a str {
    last_line(log, tag).unwrap_or_else(|| panic!("no `{tag}` line — {what}\n--- log ---\n{log}"))
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
    // The L1 `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms jitter,
    // GE p = 2 % / q = 40 % ⇒ ε ≈ 4.8 %) on client egress, seeded. Loss is
    // what drives the recovery clock this test is about at all.
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

/// The MAXIMUM `retx=<n>` the sender printed. **Read as a max over lines and
/// never off the last one**: `retx=` in the `[DIAG]` tail is an INTERVAL
/// counter, and reading it off the last line made the plain-window primitives
/// pass report `W4` as failing at 5 of 15 reps whose `[RACK] fired` on the
/// same run was 11–5 717 (goal-gate, "PLAIN WINDOW, THE SCORED RESULT" §2).
/// That harness defect is repaired HERE so no battery inherits it.
fn max_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| v.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// The realized-`W` p50 at a site, in µs, plus the evaluation count.
fn qclk(log: &str, site: &str) -> (u64, u64) {
    let l = require(
        log,
        &format!("[QCLK] site={site}"),
        "the recovery clock was never evaluated at this site, or the gauge is unreached",
    );
    // `law_n`, never `evals`: only the ARM'S OWN law's realizations belong to
    // the arm. See the gauge's declaration for what pooling them cost.
    (u64_field(l, "w_us_p50="), u64_field(l, "law_n="))
}

// ── 1 — THE ARMED ARM: the override is set, echoed, routed and realized ──

#[test]
fn the_alpha_override_arms_echoes_routes_and_moves_the_realized_clock() {
    const ALPHA: &str = "0.184";

    let (cli, srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_ALPHA_OVERRIDE", ALPHA),
    ]);

    // (1) THE GATE ECHO, both endpoints, two-sided. A missing gauge below can
    // then only be read as an unreached emission site, never as an unset gate.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_QUANTILE_CLOCKS=1"),
            "{site}: the quantile arm did not arm:\n{gates}"
        );
        assert!(
            gates.contains(&format!("RWM_ALPHA_OVERRIDE={ALPHA}")),
            "{site}: `[GATES]` must echo the RESOLVED alpha, not a flag \
             (the RWM_ACKDIAG_WINDOW_US precedent):\n{gates}"
        );
    }

    // (2) THE DIAL ROUTED. `[GATES]` says what was asked for; `[QALPHA]` says
    // what the LAW evaluates, at the site that evaluates it.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let q = require(log, &format!("[QALPHA] site={site}"), "the resolved-alpha echo is unreached");
        assert_eq!(field(q, "quantile="), "1", "{site}: {q}");
        assert!(
            (f64_field(q, "alpha=") - 0.184).abs() < 1e-9,
            "{site}: the override did not reach the law: {q}"
        );
        assert!(
            (f64_field(q, "contract_alpha=") - contract_alpha_at(site)).abs() < 1e-12,
            "{site}: the CONTRACT's own alpha must still be printed beside the \
             override, or a swept row cannot be placed against the shipped one: {q}"
        );
        assert_eq!(field(q, "override="), "1.840000e-1", "{site}: {q}");
        // k(0.184) = sqrt(0.816/0.184) = 2.1058...
        assert!(
            (f64_field(q, "k=") - 2.1058).abs() < 1e-3,
            "{site}: Cantelli's k must be the one the paper publishes for this \
             alpha - the construction is not what 16.69 refuted: {q}"
        );
    }

    // (3) THE REALIZED CLOCK IS REPORTED, AS A DISTRIBUTION.
    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let l = require(log, &format!("[QCLK] site={site}"), "the clock gauge is unreached");
        assert_eq!(field(l, "on="), "1", "{site}: {l}");
        let evals = u64_field(l, "evals=");
        let law_n = u64_field(l, "law_n=");
        let kept = u64_field(l, "kept=");
        assert!(evals > 0, "{site}: the recovery clock was never evaluated: {l}");
        // THE BIND-FRACTION CLAUSE, and it is here because its absence was
        // CAUGHT rather than foreseen. An evaluation with no sigma sample yet
        // falls through to the law below the armed one, and pooling those
        // fall-throughs into the distribution made a run at alpha = 0.002
        // report a `W` p50 of exactly 25 000 us - `TAIL_SWEEP_MIN_US`, the
        // LEGACY floor - against 128 ms at alpha = 0.9 on the same cell, i.e.
        // the sweep's own independent variable read INVERTED because the two
        // medians came from two different laws. `law_n` is what makes that
        // readable instead of silently wrong.
        assert!(
            law_n > 0,
            "{site}: the quantile law never produced a single clock - every \
             evaluation fell through to the law below it: {l}"
        );
        assert!(kept > 0, "{site}: the W distribution is empty: {l}");
        let (p05, p50, p95) = (
            u64_field(l, "w_us_p05="),
            u64_field(l, "w_us_p50="),
            u64_field(l, "w_us_p95="),
        );
        let (wmin, wmax) = (u64_field(l, "w_us_min="), u64_field(l, "w_us_max="));
        let mean = f64_field(l, "w_us_mean=");
        assert!(p05 <= p50 && p50 <= p95, "{site}: W quantiles are not ordered: {l}");
        assert!(wmin <= p05 && p95 <= wmax, "{site}: W quantiles escape their own range: {l}");
        assert!(
            mean >= wmin as f64 && mean <= wmax as f64,
            "{site}: the W mean is outside its own min/max - a unit error: {l}"
        );
        assert!(wmin > 0, "{site}: W floors at the timer granularity, never at zero: {l}");
    }

    // (4) THE α-REACHABILITY GATE (MEASUREMENT DISCIPLINE rule 1). α's two
    // consumers drive the machinery `fired` and `retx` count. If they read
    // zero the curve is mechanical and the rep is VOID, not a small number.
    let rack = require(&cli, "[RACK]", "the sender's recovery gauge never emitted");
    let fired: u64 = field(rack, "fa=")
        .split('/')
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("`fa=<spur>/<fired>` unreadable: {rack}"));
    assert!(fired > 0, "the recovery clock never fired - alpha is unreachable: {rack}");
    assert!(
        max_retx(&cli) > 0,
        "no retransmit ran - the machinery alpha parameterises did not execute"
    );
}

// ── 2 — THE CONTROL ARM: absent means absent, and it says so ─────────────

#[test]
fn without_the_override_the_contract_alpha_stands_and_the_echo_says_unset() {
    let (cli, srv) = lossy_run(&[("RWM_QUANTILE_CLOCKS", "0")]);

    for (site, log) in [("sender", &cli), ("receiver", &srv)] {
        let gates = require(log, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_QUANTILE_CLOCKS=0"),
            "{site}: the control arm must be readable as OFF: {gates}"
        );
        assert!(
            gates.contains("RWM_ALPHA_OVERRIDE=unset"),
            "{site}: an ABSENT override must print `unset`, so `my arm did not \
             take` is READ and not inferred: {gates}"
        );
        let q = require(log, &format!("[QALPHA] site={site}"), "the resolved-alpha echo must fire on EVERY arm");
        assert_eq!(field(q, "quantile="), "0", "{site}: {q}");
        assert_eq!(field(q, "override="), "unset", "{site}: {q}");
        assert!(
            (f64_field(q, "alpha=") - contract_alpha_at(site)).abs() < 1e-12,
            "{site}: with no override the CONTRACT's own alpha must stand - and \
             the two sites DISAGREE about what that is, which is exactly why a \
             swept arm (a number, not a hint mapping) is better defined across \
             sites than the contract arm it is read against: {q}"
        );
    }

    // The control still reports its realized clock — the shipped clamp's own
    // cadence, which is what every treatment arm is read against.
    let (_, evals) = qclk(&cli, "sender");
    assert!(evals > 0, "the control arm must report the clock it ran");
}

// ── 3 — GARBAGE RESOLVES TO ABSENT, VISIBLY ─────────────────────────────

#[test]
fn a_garbage_override_resolves_back_to_the_contract_and_prints_unset() {
    // ONE full lossy transfer, on the representative garbage value: the whole
    // path — gate, resolution, law, gauge — has to hold, not just the parse.
    let (cli, _srv) = lossy_run(&[
        ("RWM_QUANTILE_CLOCKS", "1"),
        ("RWM_ALPHA_OVERRIDE", "banana"),
    ]);
    let gates = require(&cli, "[GATES]", "the engine never echoed its gates");
    assert!(
        gates.contains("RWM_ALPHA_OVERRIDE=unset"),
        "`banana` must resolve back to ABSENT and print `unset` - a mistyped \
         arm that silently ran the contract's alpha is the 31 Mbit/s anomaly's \
         failure mode: {gates}"
    );
    let q = require(&cli, "[QALPHA] site=sender", "the resolved-alpha echo is unreached");
    assert!(
        (f64_field(q, "alpha=") - CONTRACT_ALPHA_BULK).abs() < 1e-12,
        "`banana`: the contract's alpha must stand: {q}"
    );
    let c = require(&cli, "[QCLK] site=sender", "the clock gauge is unreached");
    assert!(
        u64_field(c, "law_n=") > 0,
        "`banana`: the fallen-back arm still runs a law and must report it: {c}"
    );

    // The rest of the domain, on a SERVER SPAWN ALONE. The gate resolves at
    // engine start and the echo is emitted there, so no transfer is needed to
    // read it — and six back-to-back lossy loopbacks in one test is six
    // chances at an unrelated harness flake for one fact each. `k(α)` is
    // undefined at α ≤ 0 and has a negative radicand above 1, so the domain
    // filter is the LAW's own and not a taste.
    for bad in ["0", "-0.5", "1.5", "nan", "1e309", "0.5 "] {
        let (_addr, _srv, log) = spawn_perf_server(&[
            ("RWM_QUANTILE_CLOCKS", "1"),
            ("RWM_ALPHA_OVERRIDE", bad),
        ]);
        std::thread::sleep(Duration::from_millis(300));
        let l = log.lock().expect("stderr sink").clone();
        let gates = require(&l, "[GATES]", "the engine never echoed its gates");
        assert!(
            gates.contains("RWM_ALPHA_OVERRIDE=unset"),
            "`{bad}` must resolve back to ABSENT and print `unset`: {gates}"
        );
        // `[QALPHA]` is NOT asserted here: the receiver task starts on the
        // first connection, so a server that nobody dialled has not reached
        // that seat. The gate echo IS emitted at engine start, and the law
        // path is covered by the full transfer above. Asserting an unreached
        // site would be a clause satisfied by nothing.
    }
}

// ── 4 — THE SWEEP'S INDEPENDENT VARIABLE IS A TREATMENT, NOT A LABEL ────

#[test]
fn a_large_and_a_small_alpha_realize_different_clocks_on_the_same_cell() {
    // k(0.9) = 0.333, k(0.002) = 22.3 - a 67x spread in the margin term. If
    // the realized W does not separate here, on one binary, one cell and one
    // seed, then no L1 arm list separates either, and the sweep would be
    // reading a label. This is the clause CLAUDE.md's testing-discipline rule
    // demands: assert the WIRING routes, not that A is ordinally more than B.
    let (fast, _) = lossy_run(&[("RWM_QUANTILE_CLOCKS", "1"), ("RWM_ALPHA_OVERRIDE", "0.9")]);
    let (slow, _) = lossy_run(&[("RWM_QUANTILE_CLOCKS", "1"), ("RWM_ALPHA_OVERRIDE", "0.002")]);

    let (w_fast, n_fast) = qclk(&fast, "sender");
    let (w_slow, n_slow) = qclk(&slow, "sender");
    assert!(n_fast > 0 && n_slow > 0, "one of the arms never evaluated its clock");
    assert!(
        w_slow > w_fast,
        "alpha did not move the realized clock: p50 W was {w_slow} us at \
         alpha=0.002 (k=22.3) and {w_fast} us at alpha=0.9 (k=0.33). The \
         sweep's independent variable would be a LABEL, not a treatment."
    );
}
