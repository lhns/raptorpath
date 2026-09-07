//! **THE ATTRIBUTION AUDIT (D0) — THE REACHABILITY GATE.**
//!
//! The successor-arrival pass measured **`orig_frac` = 0.97858 pooled over
//! 374 120 resolved holes** and disclosed, in its own words, that the number
//! is a BOUND and not a fraction:
//!
//! > *"Reorder vs retransmit. `orig` cannot separate a late original from a
//! > resent one — the wire carries no retransmit bit. So `orig_frac` bounds the
//! > false-repair fraction from one side and does not decompose it."*
//!
//! D0 decomposes it, at the only site that CAN: the SENDER, which knows
//! whether it ever put a copy of the seq on the wire and when. This binary is
//! the gate that must pass before the audit pass is worth making, and it
//! asserts, in the order it can fail:
//!
//! 1. **THE CLASS FIELDS EXIST, ON BOTH GAUGES.** `[HOLD]` carries
//!    `hn_n`/`hy_n`/`cx_n` (heal_noretx / heal_retx_young / closed_retx),
//!    `sp_n`/`xp_n`/`up_n`, and the ripeness slots `age_n`/`age_ripe`/
//!    `ripe_frac`/`thr_p50_us`; `[SUCC]` carries `sp_n`/`xp_n`/`xp_frac`.
//! 2. **THE SITES EXECUTED** — MEASUREMENT DISCIPLINE rule 1: `[HOLD] evals>0`
//!    and `[SUCC] det>0` before any produced number is read.
//! 3. **THE SUM IDENTITIES CLOSE.** On every `[HOLD]` line
//!    `hn_n + hy_n + cx_n = fed` and `sp_n + xp_n + up_n = fed` — the classes
//!    partition exactly the resolutions the pre-audit line already counted, so
//!    no reading is re-based. On `[SUCC]`, `det = orig+rep+aban+open+over` (the
//!    inherited identity) AND `sp_n + xp_n = res` (the new one).
//! 4. **`xp_n ≡ 0` ON A SINGLE PATH.** A one-path flow cannot close a hole with
//!    an arrival on another path. This is the CONTROL READING the audit's `c1`
//!    row rests on, and it is a property of the wire rather than of the sample.
//! 5. **`xp_n > 0` ON TWO PATHS** (`RWM_L0_NETEM=c2,c3`, the C8 topology) — the
//!    same field, same binary, moved by the topology alone. Without clause 5,
//!    clause 4 could be an unreached code path rather than a measured zero.
//! 6. **THE TAPER COPY IS COUNTED, AND THE TWO COUNTERS OF ITS BRANCH
//!    AGREE.** The proactive emission's `P_lost` branch spends a correction
//!    slot on a COPY of the oldest un-acked seq; those copies land as
//!    `[RFA] dup_src` and are NOT gap fires, so realized waste cannot be
//!    attributed to the reactive loop without this number. `[DIAG] … taper=`
//!    (the new, UNGATED counter) must equal `plost=` (the pre-existing
//!    DIAG-gated one) on every line — two independently placed counters of one
//!    branch, which disagree the moment either is mis-wired.
//!
//!    **DISCLOSED IN ADVANCE, because it is a reading and not a defect:** over
//!    the `c3`-shaped loopback this branch reads **ZERO** — the oldest un-acked
//!    seq is never old enough, at the loss estimate carried on it, for
//!    `P_lost` to select a copy. The value is PRINTED by this binary rather
//!    than asserted positive, so a cell where the branch does fire is a
//!    measurement and not a gate failure. On this configuration realized
//!    repair waste has only two sources: the gap-fire copy and the margin.
//! 7. **THE AUDIT SUPPRESSES NOTHING.** With `RWM_HOLDDOWN_Q` absent — the
//!    shipped arm — every `[HOLD]` line still reads `sup=0` and
//!    `evals = sup + emit`. The audit is read-only: it adds fields to two
//!    gauge lines and takes no branch that reaches a wire byte.
//!
//! **THIS BINARY FAILS ON THE PRE-AUDIT ENGINE**: none of `hn_n=`, `cx_n=`,
//! `up_n=`, `ripe_frac=`, `taper=` or `[SUCC] xp_frac=` exists there, so every
//! clause above reads a missing field.
//!
//! **What this binary deliberately does NOT assert.** Any FIELD value of any
//! class fraction, of the ripeness fraction, or of goodput. Loopback's
//! reordering is the host scheduler's and its loss is the shim's GE process; no
//! claim about any L1 cell can be made from it.
//!
//! **Nothing here flips a default, adds a gate, or edits a law.**

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The base arm. `RWM_DIAG` carries `[DIAG] … taper=` and the periodic
/// `[SUCC]` readout; `RWM_FDIAG` the receiver-side decode trace. No gate here
/// changes a law.
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
    // The shim shapes the CLIENT's egress; the server must be unshaped, and
    // the absent arm must be ABSENT — inheritance defeats an allowlist.
    cmd.env_remove("RWM_L0_NETEM");
    cmd.env_remove("RWM_HOLDDOWN_Q");
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
    (srv, log)
}

/// One loopback transfer. `netem` is the `RWM_L0_NETEM` spec (`None` ⇒ the
/// shim is OFF and the wire is the host's own loopback — the A0.1 FLOOR).
/// Returns `(client/sender log, server/receiver log)`.
fn run(paths: usize, netem: Option<&str>, bytes: &str, runs: &str) -> (String, String) {
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
        runs,
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    cli.env_remove("RWM_HOLDDOWN_Q");
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

/// Keep the leading numeric prefix — stderr has two writers and a `tracing`
/// write can land inside a gauge line's LAST field.
fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(v.len());
    &v[..end]
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = numeric_prefix(field(line, key));
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

fn hold_lines(log: &str) -> Vec<&str> {
    log.lines().filter(|l| l.contains("[HOLD] site=sender")).collect()
}

fn last_succ(log: &str) -> &str {
    log.lines()
        .rev()
        .find(|l| l.contains("[SUCC] "))
        .unwrap_or_else(|| panic!("no `[SUCC]` line — the receiver gauge never ran:\n{log}"))
}

/// The MAXIMUM `taper=<n>` printed. Read as a max over lines and never off the
/// last one — the `[DIAG]` tail is emitted per interval, and the counter is
/// cumulative but the last line may be truncated by the SIGKILL.
fn max_field(log: &str, key: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or_else(|| panic!("`{key}` never appeared in the log"))
}

/// **THE FIELD CENSUS.** Every slot the audit adds, asserted PRESENT by name
/// rather than inferred from a value — a missing field must read as an
/// unreached emission site and never as a measured zero.
const HOLD_FIELDS: [&str; 19] = [
    "hn_n=", "hn_p50_us=", "hn_p90_us=", "hy_n=", "hy_p50_us=", "hy_p90_us=",
    "cx_n=", "cx_p50_us=", "cx_p90_us=", "sp_n=", "xp_n=", "up_n=", "xp_frac=",
    "age_n=", "age_ripe=", "ripe_frac=", "age_p50_us=", "age_p90_us=",
    "thr_p50_us=",
];

const SUCC_FIELDS: [&str; 7] = [
    "sp_n=", "xp_n=", "xp_frac=", "sp_p50_us=", "sp_p90_us=", "xp_p50_us=",
    "xp_p90_us=",
];

/// Clauses 1, 3 and 7 on every `[HOLD]` line of a run.
fn assert_hold_lines(lines: &[&str]) {
    assert!(!lines.is_empty(), "the sender printed no `[HOLD]` line at all");
    for l in lines {
        for k in HOLD_FIELDS {
            assert!(l.contains(k), "`{k}` missing — pre-audit engine? {l}");
        }
        // The inherited identity, unchanged by the audit.
        let (evals, sup, emit) =
            (u64_field(l, "evals="), u64_field(l, "sup="), u64_field(l, "emit="));
        assert_eq!(evals, sup + emit, "evals must equal sup + emit: {l}");
        // Clause 7: the shipped arm holds NOTHING. The audit is read-only.
        assert_eq!(sup, 0, "the audit must suppress no fire on the shipped arm: {l}");
        // Clause 3: the classes partition exactly the fed resolutions.
        let fed = u64_field(l, "fed=");
        let (hn, hy, cx) =
            (u64_field(l, "hn_n="), u64_field(l, "hy_n="), u64_field(l, "cx_n="));
        assert_eq!(
            hn + hy + cx,
            fed,
            "heal_noretx + heal_retx_young + closed_retx must equal fed: {l}"
        );
        let (sp, xp, up) =
            (u64_field(l, "sp_n="), u64_field(l, "xp_n="), u64_field(l, "up_n="));
        assert_eq!(sp + xp + up, fed, "the same/cross/unattributed split must close: {l}");
        // A ripeness reading is taken at every FIRST report, of which there are
        // at least as many as there are resolutions.
        assert!(
            u64_field(l, "age_n=") >= fed,
            "a resolution cannot precede its own first report: {l}"
        );
        assert!(u64_field(l, "age_ripe=") <= u64_field(l, "age_n="), "{l}");
    }
}

/// Clauses 1 and 3 on the `[SUCC]` line.
fn assert_succ_line(l: &str) {
    for k in SUCC_FIELDS {
        assert!(l.contains(k), "`{k}` missing from [SUCC] — pre-audit engine? {l}");
    }
    let det = u64_field(l, "det=");
    let sum = u64_field(l, "orig_n=")
        + u64_field(l, "rep_n=")
        + u64_field(l, "aban_n=")
        + u64_field(l, "open=")
        + u64_field(l, "over=");
    assert_eq!(det, sum, "[SUCC] det = orig+rep+aban+open+over must hold: {l}");
    let res = u64_field(l, "res=");
    assert_eq!(
        u64_field(l, "sp_n=") + u64_field(l, "xp_n="),
        res,
        "[SUCC] sp_n + xp_n must equal res — every resolution has exactly one \
         arrival path: {l}"
    );
}

// ── THE GATES ───────────────────────────────────────────────────────────

/// Clauses 1–4, 6, 7 on ONE path over a lossy wire.
#[test]
fn the_audit_classes_reach_the_engine_and_cross_path_is_zero_on_one_path() {
    let (cli, srv) = run(1, Some("c3"), "4000000", "2");

    // Clause 2: the sites executed, before any produced number is read.
    let hold = hold_lines(&cli);
    assert!(!hold.is_empty(), "no `[HOLD]` line from the sender:\n{cli}");
    let pathline = hold
        .iter()
        .copied()
        .find(|l| !l.contains("path=-"))
        .unwrap_or_else(|| panic!("no per-path `[HOLD]` — the gap loop never ran:\n{cli}"));
    assert!(u64_field(pathline, "evals=") > 0, "the gap-report site never ran: {pathline}");

    assert_hold_lines(&hold);

    let succ = last_succ(&srv);
    assert!(
        u64_field(succ, "det=") > 0,
        "[SUCC] det=0 over a c3-lossy transfer — the receiver saw no hole: {succ}"
    );
    assert_succ_line(succ);

    // Clause 4: THE CONTROL READING.
    assert_eq!(
        u64_field(succ, "xp_n="),
        0,
        "a ONE-PATH flow closed a hole with an arrival on another path: {succ}"
    );
    assert!(succ.contains("xp_frac=0.0000"), "one path ⇒ a measured zero, not `-`: {succ}");
    for l in &hold {
        assert_eq!(
            u64_field(l, "xp_n="),
            0,
            "a one-path sender saw a cross-path gap report: {l}"
        );
    }

    // Clause 6: the taper counter is present, ungated, and AGREES with the
    // DIAG-gated counter of the same branch on every line.
    let taper = max_field(&cli, "taper=");
    let plost = max_field(&cli, "plost=");
    println!("[holeclass] single-path c3: taper_copy={taper} plost={plost}");
    println!("[holeclass] single-path c3 [HOLD]: {pathline}");
    assert_eq!(
        taper, plost,
        "the ungated `taper=` and the DIAG-gated `plost=` count the SAME \
         branch and must agree — one of them is mis-wired:\n{cli}"
    );
    for l in cli.lines().filter(|l| l.contains(" mpr[")) {
        assert!(l.contains(" taper="), "a `[DIAG]` line without `taper=`: {l}");
    }
}

/// Clause 5: the SAME field moves when the topology does.
#[test]
fn the_cross_path_class_is_populated_on_two_paths() {
    let (cli, srv) = run(2, Some("c2,c3"), "24000000", "2");

    let hold = hold_lines(&cli);
    assert_hold_lines(&hold);
    let succ = last_succ(&srv);
    assert_succ_line(succ);
    assert!(
        u64_field(succ, "det=") > 0,
        "[SUCC] det=0 on the dual c2,c3 topology: {succ}"
    );
    println!(
        "[holeclass] dual c2,c3: det={} res={} sp_n={} xp_n={} xp_frac={}",
        u64_field(succ, "det="),
        u64_field(succ, "res="),
        u64_field(succ, "sp_n="),
        u64_field(succ, "xp_n="),
        field(succ, "xp_frac="),
    );
    assert!(
        u64_field(succ, "xp_n=") > 0,
        "TWO paths and not one hole was closed by an arrival on the other — \
         either the split is wired to a constant or the second path never \
         carried data: {succ}"
    );
}

/// **A0.1 — THE LOOPBACK FLOOR.** One path, shim OFF: the wire is the host's
/// own loopback, which is lossless and FIFO, so **every `[SUCC] det` here is an
/// artifact by construction** — the 2 ms gap-ack sampler, receive-buffer
/// eviction, or the gauge itself. The NUMBER is a measurement and is printed
/// rather than asserted; what is asserted is that the identities close and that
/// the cross-path class is structurally empty.
#[test]
fn the_lossless_single_path_floor_is_measured_and_its_identities_close() {
    let (cli, srv) = run(1, None, "50000000", "2");
    let succ = last_succ(&srv);
    assert_succ_line(succ);
    let det = u64_field(succ, "det=");
    println!(
        "[holeclass] A0.1 FLOOR (no netem, 1 path): det={det} res={} orig_n={} \
         rep_n={} open={} over={} sp_n={} xp_n={}",
        u64_field(succ, "res="),
        u64_field(succ, "orig_n="),
        u64_field(succ, "rep_n="),
        u64_field(succ, "open="),
        u64_field(succ, "over="),
        u64_field(succ, "sp_n="),
        u64_field(succ, "xp_n="),
    );
    assert_eq!(u64_field(succ, "xp_n="), 0, "one path cannot expose a cross-path hole: {succ}");
    for l in &hold_lines(&cli) {
        for k in HOLD_FIELDS {
            assert!(l.contains(k), "`{k}` missing — pre-audit engine? {l}");
        }
    }
}
