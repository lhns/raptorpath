//! The hole-attribution classes reach the engine. `[SUCC] orig_frac` cannot
//! separate a late original from a resent one (the wire carries no
//! retransmit bit), so it only bounds the false-repair fraction; the sender,
//! which knows whether and when it put a copy on the wire, decomposes it.
//! Clauses, in the order they can fail:
//!
//! 1. The class fields exist on both gauges: `[HOLD]` carries
//!    `hn_n`/`hy_n`/`cx_n` (heal_noretx / heal_retx_young / closed_retx),
//!    `sp_n`/`xp_n`/`up_n` and the ripeness slots `age_n`/`age_ripe`/
//!    `ripe_frac`/`thr_p50_us`; `[SUCC]` carries `sp_n`/`xp_n`/`xp_frac`.
//! 2. The sites executed (measurement-discipline rule 1): `[HOLD] evals > 0`
//!    and `[SUCC] det > 0`.
//! 3. The sum identities close: on `[HOLD]`, `hn_n + hy_n + cx_n = fed` and
//!    `sp_n + xp_n + up_n = fed`; on `[SUCC]`,
//!    `det = orig+rep+aban+open+over` and `sp_n + xp_n = res`.
//! 4. `xp_n ≡ 0` on a single path, a property of the wire.
//! 5. `xp_n > 0` on two paths (`c2,c3`) — the same field moved by topology
//!    alone, so clause 4 is a measured zero, not an unreached path.
//! 6. The proactive taper copy (a correction slot spent on a copy of the
//!    oldest un-acked seq, landing as `[RFA] dup_src`) is counted: the
//!    ungated `[DIAG] taper=` equals the DIAG-gated `plost=` on every line.
//!    Its value is printed, not asserted: over the `c3` loopback it is zero.
//! 7. The audit suppresses nothing: with `RWM_HOLDDOWN_Q` absent every
//!    `[HOLD]` line reads `sup=0`.
//!
//! No class fraction, ripeness fraction or goodput value is asserted. The
//! audit adds no gate and edits no law.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{numeric_prefix, str_field, u64_field};

/// The base arm. `RWM_DIAG` carries `[DIAG] … taper=` and the periodic
/// `[SUCC]` readout; `RWM_FDIAG` the receiver-side decode trace. No gate here
/// changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One loopback transfer. `netem` is the `RWM_L0_NETEM` spec (`None` ⇒ the
/// shim is off and the wire is the host's own loopback). Returns
/// `(client/sender log, server/receiver log)`; the server log is taken once a
/// `[SUCC]` readout post-dating the transfer arrived.
fn run(paths: usize, netem: Option<&str>, bytes: &str, runs: &str) -> (String, String) {
    loopback::transfer(loopback::Transfer {
        paths,
        env: &ARM,
        client_env: &loopback::shaped(netem),
        bytes,
        runs,
        srv_tag: Some("[SUCC] "),
        ..Default::default()
    })
}

// ── Readers ─────────────────────────────────────────────────────────────

fn hold_lines(log: &str) -> Vec<&str> {
    log.lines().filter(|l| l.contains("[HOLD] site=sender")).collect()
}

fn last_succ(log: &str) -> &str {
    log.lines()
        .rev()
        .find(|l| l.contains("[SUCC] "))
        .unwrap_or_else(|| panic!("no `[SUCC]` line — the receiver gauge never ran:\n{log}"))
}

/// The maximum `taper=<n>` printed, read as a max over lines: the last
/// `[DIAG]` line may be truncated by the SIGKILL.
fn max_field(log: &str, key: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or_else(|| panic!("`{key}` never appeared in the log"))
}

/// Every slot the audit adds, asserted present by name — a missing field
/// must read as an unreached emission site, never as a measured zero.
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
        // The inherited identity.
        let (evals, sup, emit) =
            (u64_field(l, "evals="), u64_field(l, "sup="), u64_field(l, "emit="));
        assert_eq!(evals, sup + emit, "evals must equal sup + emit: {l}");
        // Clause 7: the shipped arm holds nothing.
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
        // A ripeness reading is taken at every first report, of which there
        // are at least as many as resolutions.
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

// ── The gates ───────────────────────────────────────────────────────────

/// Clauses 1–4, 6, 7 on one path over a lossy wire.
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

    // Clause 4: the control reading.
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

    // Clause 6: the taper counter is present, ungated, and agrees with the
    // DIAG-gated counter of the same branch.
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

/// Clause 5: the same field moves when the topology does.
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
        str_field(succ, "xp_frac="),
    );
    assert!(
        u64_field(succ, "xp_n=") > 0,
        "TWO paths and not one hole was closed by an arrival on the other — \
         either the split is wired to a constant or the second path never \
         carried data: {succ}"
    );
}

/// The loopback floor: one path, shim off. The wire is lossless and FIFO, so
/// every `[SUCC] det` here is an artifact (the 2 ms gap-ack sampler,
/// receive-buffer eviction, or the gauge itself). The number is printed; the
/// identities and the empty cross-path class are asserted.
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
