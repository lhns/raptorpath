//! `[FCAUSE]` attributes every sender recovery fire to a named cause, and
//! echoes the configuration the cause mix is readable under.
//! `RackClockGauge::record_fire` has one call site, the sender's gap loop,
//! whose `gaps` vector has two producers:
//!
//!   * `timer` — the sender's tail-sweep deadline arm, the only cause the
//!     sender's recovery clock (`sweep_timeout_us`) times.
//!   * the `nack_rx` channel, fed by the SACK→gap inversion in the WindowAck
//!     handler and clocked by the receiver. The timer-driven hole
//!     re-advertisement broadcasts one message to every path and stamps
//!     `echo_send_timestamp_us: 0`, so it separates as `gap_refresh`;
//!     `gap_data` is the dupack analog, driven by data arrival alone.
//!
//! Clauses, in the order they can fail:
//!
//!   1. `fcause_report_line`'s format, including `-` iff no denominator.
//!   2. The line fires with populated causes over a `c3`-lossy plain-window
//!      loopback.
//!   3. The four classes sum to `n`, `other` is empty (an unclassifiable fire
//!      is counted, never guessed), and the fractions match the counts.
//!   4. `n` agrees with the independent witness `[DIAG] retx=`, bumped by
//!      different code at the same emission.
//!   5. `n >= [RACK] fired`, the difference printed as `unattr=`:
//!      `record_fire` sits inside `if let Some(mp_flight)`, so `fired` drops
//!      fires with no live-flight record; `[FCAUSE]` counts at the emission.
//!   6. Under generation coding the SACK→gap producer is suppressed
//!      (`recv_nack_tx = None`), so both `gap_` classes are structurally
//!      empty; the line echoes `gen=` so no row is read out of scope.
//!
//! `[FCAUSE]` rides `[RACK]`'s ungated `Drop` rule; no cause mix is asserted
//! (loopback loss is the shim's GE process). Own test binary: `RWM_L0_NETEM`
//! is process-global in the child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{f64_field, numeric_prefix, str_field, u64_field};
use raptorpath::net::fcause_report_line;

// ── 1: the pure pin ─────────────────────────────────────────────────────

#[test]
fn the_fcause_line_format_is_pinned() {
    // timer=12 gap_data=430 gap_refresh=58 other=0 ⇒ n = 500.
    //   timer_frac = 12/500 = 0.0240, gap_frac = 488/500 = 0.9760.
    //   fired = 494 ⇒ unattr = 6.
    assert_eq!(
        fcause_report_line(12, 430, 58, 0, 494, false),
        "[FCAUSE] gen=0 n=500 timer=12 gap_data=430 gap_refresh=58 other=0 \
         timer_frac=0.0240 gap_frac=0.9760 fired=494 unattr=6 fa_class=0.0625"
    );

    // Never fired reads `-`, never `0.0000`: a fraction with no denominator
    // is absent and must not pool with a measured zero.
    let empty = fcause_report_line(0, 0, 0, 0, 0, false);
    assert_eq!(
        empty,
        "[FCAUSE] gen=0 n=0 timer=0 gap_data=0 gap_refresh=0 other=0 \
         timer_frac=- gap_frac=- fired=0 unattr=0 fa_class=0.0625"
    );
    assert!(
        !empty.contains("timer_frac=0"),
        "an unfired gauge must not render a fraction: {empty}"
    );

    // The generation row: both `gap_` classes are structurally empty, so a
    // 1.0000 timer fraction is a configuration fact, stated by `gen=1`.
    let g = fcause_report_line(37, 0, 0, 0, 37, true);
    assert!(g.contains("gen=1"), "{g}");
    assert!(g.contains("timer_frac=1.0000"), "{g}");
    assert!(g.contains("gap_frac=0.0000"), "{g}");
    assert!(g.contains("unattr=0"), "{g}");

    // The trailing sacrificial constant: a concurrent `tracing` write corrupts
    // the last field, so the last field is a constant every parser knows.
    for l in [&empty, &g] {
        assert!(
            l.trim_end().ends_with("fa_class=0.0625"),
            "every gauge line ends on the class bar: {l}"
        );
    }
}

// ── 2-6: the reachability run ───────────────────────────────────────────

/// The arm. `RWM_DIAG` carries `[DIAG] retx=`, the independent witness. No
/// gate here changes a law: the sender runs the shipped recovery clock.
const ARM: [(&str, &str); 2] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
];

/// Run one lossy loopback in the given configuration. Returns
/// `(client log, server log)`; the client is the bulk-direction sender whose
/// `[FCAUSE]` is under test. The `c3` cell shapes the client's egress
/// ([`loopback::C3`]) so recovery fires exist; the ack direction stays clean.
fn lossy_run(generation: bool) -> (String, String) {
    let extra: &[&str] = if generation { &["--window-generation-coding"] } else { &[] };
    loopback::transfer(loopback::Transfer { env: &ARM, extra_args: extra, ..Default::default() })
}

/// `[DIAG]`'s cumulative retransmit count, max over all lines (the periodic
/// readout can be cut off mid-transfer, so the largest reading saw the most).
fn max_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or_else(|| panic!("no `retx=` in the sender log — [DIAG] never fired"))
}

/// Clauses 2-5: the causes are present, consistent, and witnessed.
#[test]
fn every_recovery_fire_is_attributed_to_a_named_cause() {
    let (cli, _srv) = lossy_run(false);

    // The gate: a missing witness must read as an unreached site, never as an
    // unset gate.
    assert!(
        cli.contains("RWM_DIAG=1"),
        "the client's [GATES] echo does not carry RWM_DIAG=1 — the arm did \
         not arm:\n{cli}"
    );

    // 2. The line fires.
    let last = cli
        .lines()
        .rev()
        .find(|l| l.contains("[FCAUSE] "))
        .unwrap_or_else(|| {
            panic!(
                "no [FCAUSE] line from the PLAIN-WINDOW sender over a c3-lossy \
                 transfer — no recovery fire was attributed to any cause, which \
                 is the DEAD-INSTRUMENT reading this test exists to fail on:\n{cli}"
            )
        });
    println!("[fcause-reach] PLAIN: {last}");

    let n = u64_field(last, "n=");
    let timer = u64_field(last, "timer=");
    let gap_data = u64_field(last, "gap_data=");
    let gap_refresh = u64_field(last, "gap_refresh=");
    let other = u64_field(last, "other=");
    let fired = u64_field(last, "fired=");
    let unattr = u64_field(last, "unattr=");

    assert!(
        last.contains("gen=0"),
        "this run is PLAIN WINDOW — the sweep's configuration — and the line \
         must say so: {last}"
    );
    assert!(
        n > 0,
        "[FCAUSE] n=0 over a c3-lossy plain-window transfer: the gap loop \
         never fired, so there is no cause mix to read:\n{last}"
    );

    // 3. Internal consistency.
    assert_eq!(
        n,
        timer + gap_data + gap_refresh + other,
        "[FCAUSE] n is not the sum of its four causes: {last}"
    );
    // `other` is the named unclassifiable class and must be empty: every
    // producer of the gap loop's batches carries a tag.
    assert_eq!(
        other, 0,
        "[FCAUSE] other={other} — a gap batch reached the fire site with no \
         cause tag, so some producer is unplumbed and its fires are \
         unattributed:\n{last}"
    );
    let timer_frac = f64_field(last, "timer_frac=");
    let gap_frac = f64_field(last, "gap_frac=");
    assert!(
        (timer_frac - timer as f64 / n as f64).abs() < 1e-3,
        "[FCAUSE] timer_frac={timer_frac} disagrees with {timer}/{n}: {last}"
    );
    assert!(
        (gap_frac - (gap_data + gap_refresh) as f64 / n as f64).abs() < 1e-3,
        "[FCAUSE] gap_frac={gap_frac} disagrees with \
         ({gap_data}+{gap_refresh})/{n}: {last}"
    );
    assert!(
        (timer_frac + gap_frac - 1.0).abs() < 1e-3,
        "[FCAUSE] with other=0 the two fractions must partition the fires: \
         {timer_frac} + {gap_frac} != 1 in {last}"
    );

    // 4. The independent witness. `>=` rather than `==` because the periodic
    //    `[DIAG]` readout can be cut off before the final fires while
    //    `[FCAUSE]` emits at teardown.
    let retx = max_retx(&cli);
    println!("[fcause-reach] n={n} vs [DIAG] retx={retx} (unattr={unattr})");
    assert!(retx > 0, "[DIAG] retx=0 while [FCAUSE] n={n}: {last}");
    assert!(
        n >= retx,
        "[FCAUSE] n={n} is BELOW the independent witness [DIAG] retx={retx} — \
         the cause counters are missing fires the gap loop emitted:\n{last}"
    );

    // 5. The denominator discrepancy is reported: `fired` is a subset, and
    //    `unattr` names the difference.
    assert!(
        n >= fired,
        "[FCAUSE] n={n} < [RACK] fired={fired} — `fired` counts a strict \
         subset of the emissions and cannot exceed the true fire count:\n{last}"
    );
    assert_eq!(
        unattr,
        n - fired,
        "[FCAUSE] unattr must be exactly n - fired: {last}"
    );

    // The sender's `[RACK]` line must agree about `fired`. The client also
    // runs a receiver task whose own `[RACK]` counts repair-class arrivals,
    // and teardown order is not fixed, so the sender's line is found by value.
    let racks: Vec<&str> = cli.lines().filter(|l| l.contains("[RACK] ")).collect();
    if !racks.is_empty() {
        let fired_of = |rack: &str| -> u64 {
            let fa = str_field(rack, "fa=");
            let (_sp, fd) = fa
                .split_once('/')
                .unwrap_or_else(|| panic!("fa= must render `<spurious>/<fired>`: {rack}"));
            numeric_prefix(fd).parse::<u64>().expect("fired parses")
        };
        assert!(
            racks.iter().any(|r| fired_of(r) == fired),
            "[FCAUSE] fired={fired} matches no [RACK] fa=.../<fired> line — the \
             sender's two lines read the same counter:\n{racks:#?}\n{last}"
        );
    }
}

/// Clause 6: the cause mix measures the shipped machine only in plain window;
/// generation coding suppresses the SACK→gap producer, so both `gap_` classes
/// are structurally empty there.
#[test]
fn the_gap_causes_are_structurally_empty_under_generation() {
    let (gen_cli, _gen_srv) = lossy_run(true);

    // `recv_nack_tx = None` under generation: the per-seq retransmit path does
    // not run (the same fact `rfa_reachability` measures).
    let gen_retx: u64 = gen_cli
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| v.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    println!("[fcause-reach] GENERATION retx={gen_retx}");

    match gen_cli.lines().rev().find(|l| l.contains("[FCAUSE] ")) {
        Some(l) => {
            println!("[fcause-reach] GENERATION: {l}");
            assert!(
                l.contains("gen=1"),
                "[FCAUSE] from a generation sender must echo gen=1 so no row \
                 is read out of its configuration scope: {l}"
            );
            assert_eq!(
                u64_field(l, "gap_data="),
                0,
                "[FCAUSE] gap_data must be 0 under generation — the SACK→gap \
                 producer is suppressed there: {l}"
            );
            assert_eq!(
                u64_field(l, "gap_refresh="),
                0,
                "[FCAUSE] gap_refresh must be 0 under generation — the \
                 SACK→gap producer is suppressed there: {l}"
            );
            assert_eq!(
                u64_field(l, "other="),
                0,
                "[FCAUSE] other must be 0 in every configuration: {l}"
            );
        }
        None => {
            // Legal: with no gap producer and no tail-sweep fire nothing was
            // classified and the gauge stays silent (`[RACK]`'s rule). A silent
            // gauge beside a retransmit would be an unattributed fire.
            assert_eq!(
                gen_retx, 0,
                "no [FCAUSE] line under generation while [DIAG] retx={gen_retx} \
                 — fires were emitted and none was classified:\n{gen_cli}"
            );
        }
    }
}
