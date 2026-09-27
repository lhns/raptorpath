//! The receiver's realized false-repair gauge `[RFA]` fires, and the
//! configuration contract under which the sender's `fa=` is a measurement.
//! The sender's `fa=` is a prediction (at fire time: is the target's flight
//! younger than the law threshold?); a realized false repair — emitted, and
//! the original arrived anyway — is only observable at the receiver, where
//! both copies land. `RackClockGauge::record_fire` has one call site, the
//! sender's gap loop fed by `recv_nack_tx`, which is `None` under generation
//! coding. Clauses, in the order they can fail:
//!
//!   1. `classify_recv_repair` is total and its truth table is the event-class
//!      definition.
//!   2. `rfa_report_line`'s format, so an L1 parser has a pin.
//!   3. The configuration contract, both sides, over a `c3`-lossy loopback:
//!      plain window ⇒ `[RACK] fired > 0` and `[DIAG] retx > 0`; generation
//!      ⇒ `retx = 0`.
//!   4. The receiver emits `[RFA]` with `fires > 0` and `src_n > 0` in plain
//!      window; under generation it reads `gen=1 src_n=0`, saying which
//!      machine a row belongs to.
//!   5. Internal consistency: the four classes sum to `fires`, the two
//!      redundant ones to `false`, and `false_frac` and `ν_recv = fires/src_n`
//!      are fractions.
//!
//! The periodic readout rides `RWM_DIAG`/`RWM_FDIAG` and the `Drop` emission
//! `[RACK]`'s ungated rule, so there is no new gate. No value of the false
//! fraction is asserted: loopback's redundancy is the shim's and the host
//! scheduler's. Own test binary: `RWM_L0_NETEM` is process-global in the
//! child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{f64_field, numeric_prefix, str_field, u64_field};

use raptorpath::net::{classify_recv_repair, rfa_report_line, RecvRepair};

// ── 1 + 2: the pure pins ────────────────────────────────────────────────

#[test]
fn the_event_class_is_exactly_what_the_alpha_sweep_needs() {
    // A false repair is a repair emitted whose original arrived anyway: at
    // the receiver, redundancy, with exactly two redundant observations.
    // `seen_as_source` dominates `recovered`: a second source copy is wasted
    // whatever the decoder did in the meantime.
    for overdue in [false, true] {
        for recovered in [false, true] {
            assert_eq!(
                classify_recv_repair(true, recovered, overdue),
                RecvRepair::DupSource,
                "a second SOURCE copy is a duplicate at every other state \
                 (recovered={recovered} overdue={overdue})"
            );
        }
    }
    for overdue in [false, true] {
        assert_eq!(
            classify_recv_repair(false, true, overdue),
            RecvRepair::PreemptedSource,
            "a source arrival for an already-DECODED seq preempts the coded \
             repair that decoded it (overdue={overdue})"
        );
    }
    assert_eq!(
        classify_recv_repair(false, false, true),
        RecvRepair::FillSource,
        "first resolution of an OVERDUE seq is a repair that worked"
    );
    assert_eq!(
        classify_recv_repair(false, false, false),
        RecvRepair::NotRepair,
        "first, in-order resolution is ordinary forward progress, NOT a fire"
    );

    // The fire/false partition the `[RACK]` slots are fed from.
    for c in [
        RecvRepair::NotRepair,
        RecvRepair::FillSource,
        RecvRepair::DupSource,
        RecvRepair::PreemptedSource,
    ] {
        assert_eq!(
            c.is_fire(),
            c != RecvRepair::NotRepair,
            "{c:?}: every repair class is a fire and only NotRepair is not"
        );
        if c.is_false() {
            assert!(c.is_fire(), "{c:?}: a FALSE repair must also be a FIRE");
        }
    }
    assert!(RecvRepair::DupSource.is_false());
    assert!(RecvRepair::PreemptedSource.is_false());
    assert!(
        !RecvRepair::FillSource.is_false(),
        "a repair that CLOSED a hole is not a false alarm"
    );
}

#[test]
fn the_rfa_line_format_is_pinned() {
    // `rep_redundant` (the false measurand under coded answers) and
    // `late_after_aban` (a copy that arrived after the give-up, grepped by
    // name by `tools/l1/tail_matrix.sh`) are appended at the end, so every
    // earlier field keeps its position.
    // fill_coded=9 fill_src=3 dup_src=4 preempt_src=1 over src_n=1000:
    //   fires = 17, false = 5, false_frac = 5/17, nu_recv = 17/1000.
    assert_eq!(
        rfa_report_line(9, 3, 4, 1, 1000, 200, false, 41, 7),
        "[RFA] gen=0 fires=17 false=5 false_frac=0.2941 fill_coded=9 \
         fill_src=3 dup_src=4 preempt_src=1 src_n=1000 rep_n=200 \
         nu_recv=0.01700 fa_class=0.0625 rep_redundant=41 late_after_aban=7"
    );
    // The generation row: `src_n = 0` is structural, so every ratio on it
    // reads 0 rather than divides, and `gen=1` says why.
    assert_eq!(
        rfa_report_line(0, 0, 0, 0, 0, 9730, true, 0, 0),
        "[RFA] gen=1 fires=0 false=0 false_frac=0.0000 fill_coded=0 \
         fill_src=0 dup_src=0 preempt_src=0 src_n=0 rep_n=9730 \
         nu_recv=0.00000 fa_class=0.0625 rep_redundant=0 late_after_aban=0"
    );
}

// ── 3-5: the reachability run ───────────────────────────────────────────

/// The arm. `RWM_DIAG` carries the periodic `[RFA]` readout (the L1
/// harnesses SIGKILL the server, so a `Drop`-only emission is unreachable
/// there). No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One lossy loopback: the perf server is the bulk-direction receiver whose
/// `[RFA]` is under test. The `c3` cell (20 Mbit, 20 ms one-way, 5 ms jitter,
/// GE p = 2 % / q = 40 % ⇒ ε ≈ 4.8 %) shapes client egress, seeded, so
/// repair-class events exist; the ack direction stays clean. The server log is
/// taken once an `[RFA]` readout post-dating the transfer landed. Returns
/// `(client log, server log)`.
fn lossy_run(generation: bool) -> (String, String) {
    let extra: &[&str] = if generation { &["--window-generation-coding"] } else { &[] };
    loopback::transfer(loopback::Transfer {
        env: &ARM,
        extra_args: extra,
        srv_tag: Some("[RFA] "),
        ..Default::default()
    })
}

/// The last `retx=<n>` the sender printed — `[DIAG]`'s cumulative retransmit
/// count, the independent witness for whether the gap loop ran at all.
fn last_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .next_back()
        .unwrap_or_else(|| panic!("no `retx=` in the sender log — [DIAG] never fired"))
}

/// Clause 3: the sender's `fa=` denominator is alive in plain window and
/// structurally dead under generation, where `recv_nack_tx` is `None` and no
/// gap reaches the loop `record_fire` lives in.
#[test]
fn the_senders_fa_is_alive_in_plain_window_and_dead_under_generation() {
    let (plain_cli, _plain_srv) = lossy_run(false);
    let plain_retx = last_retx(&plain_cli);
    let rack = plain_cli
        .lines()
        .rev()
        .find(|l| l.contains("[RACK] "))
        .unwrap_or_else(|| {
            panic!(
                "no [RACK] line from the PLAIN-WINDOW sender: its `Drop` emits \
                 whenever `on || fired > 0`, so absence with the gate off means \
                 fired == 0 exactly — the sweep has no fa= denominator:\n{plain_cli}"
            )
        });
    let fa = str_field(rack, "fa=");
    let (sp, fd) = fa
        .split_once('/')
        .unwrap_or_else(|| panic!("fa= must render `<spurious>/<fired>`: {rack}"));
    let (sp, fd): (u64, u64) = (sp.parse().unwrap(), numeric_prefix(fd).parse().unwrap());
    println!("[rfa-reach] PLAIN sender fa={sp}/{fd} retx={plain_retx}");
    assert!(
        fd > 0,
        "[RACK] fired=0 in PLAIN WINDOW over a c3-lossy transfer — the \
         α-sweep's commanded false-alarm fraction has no denominator:\n{rack}"
    );
    assert!(
        sp <= fd,
        "[RACK] spurious={sp} exceeds fired={fd} — the two slots are not a \
         fraction:\n{rack}"
    );
    assert!(
        plain_retx > 0,
        "[DIAG] retx=0 in PLAIN WINDOW while [RACK] fired={fd} — the \
         independent witness disagrees with the gauge"
    );

    // The other side: generation suppresses the SACK→gap producer
    // (`recv_nack_tx = None`), so `fired = 0` there is a configuration fact,
    // not a dead instrument.
    let (gen_cli, gen_srv) = lossy_run(true);
    let gen_retx = last_retx(&gen_cli);
    println!("[rfa-reach] GENERATION sender retx={gen_retx}");
    assert_eq!(
        gen_retx, 0,
        "[DIAG] retx={gen_retx} under GENERATION CODING — `recv_nack_tx` is \
         supposed to be None there, so the per-seq retransmit path must not \
         run and the primitives pass's fired=0 must stay explained:\n{gen_cli}"
    );

    // The receiver's line says which machine it measured.
    if let Some(l) = gen_srv.lines().rev().find(|l| l.contains("[RFA] ")) {
        println!("[rfa-reach] GENERATION receiver: {l}");
        assert!(
            l.contains("gen=1"),
            "[RFA] from a generation receiver must echo gen=1: {l}"
        );
        assert_eq!(
            u64_field(l, "src_n="),
            0,
            "[RFA] src_n must be 0 under generation — every arrival is coded, \
             so the FALSE classes are structurally empty: {l}"
        );
    }
}

#[test]
fn the_receiver_reports_the_realized_false_repair_fraction() {
    let (_cli, log) = lossy_run(false);

    // The gate: a missing `[RFA]` must read as an unreached emission site,
    // never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the server's [GATES] echo does not carry RWM_DIAG=1 — the arm did \
         not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the server's [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );

    // 4. The line fires, nonzero.
    let rfa: Vec<&str> = log.lines().filter(|l| l.contains("[RFA] ")).collect();
    assert!(
        !rfa.is_empty(),
        "no [RFA] line from the RECEIVER over a lossy transfer — the \
         false-repair gauge is unreachable:\n{log}"
    );
    // Cumulative counters: the last line is the reading.
    let last = *rfa.last().expect("non-empty");
    println!("[rfa-reach] {} lines; last: {last}", rfa.len());

    let fires = u64_field(last, "fires=");
    let falses = u64_field(last, "false=");
    let fill_coded = u64_field(last, "fill_coded=");
    let fill_src = u64_field(last, "fill_src=");
    let dup_src = u64_field(last, "dup_src=");
    let preempt_src = u64_field(last, "preempt_src=");
    let src_n = u64_field(last, "src_n=");
    let false_frac = f64_field(last, "false_frac=");
    let nu_recv = f64_field(last, "nu_recv=");

    assert!(
        last.contains("gen=0"),
        "this run is PLAIN WINDOW — the α-sweep's configuration — and the \
         line must say so: {last}"
    );
    assert!(
        fires > 0,
        "[RFA] fires=0 over a c3-lossy PLAIN-WINDOW transfer — the receiver \
         saw no repair-class event at all, which is the DEAD-GAUGE reading \
         this test exists to fail on:\n{last}"
    );

    // 5. Internal consistency: a class counted twice, or a fraction on the
    //    wrong denominator, is caught at the instrument.
    assert_eq!(
        fires,
        fill_coded + fill_src + dup_src + preempt_src,
        "[RFA] fires is not the sum of its four classes: {last}"
    );
    assert_eq!(
        falses,
        dup_src + preempt_src,
        "[RFA] false is not the sum of the two REDUNDANT classes: {last}"
    );
    assert!(
        (0.0..=1.0).contains(&false_frac),
        "[RFA] false_frac={false_frac} is not a fraction: {last}"
    );
    assert!(
        (false_frac - falses as f64 / fires as f64).abs() < 1e-3,
        "[RFA] false_frac={false_frac} disagrees with {falses}/{fires}: {last}"
    );

    // The denominator is fed, and ν_recv is formed on it.
    assert!(
        src_n > 0,
        "[RFA] src_n=0 — no source arrival was counted, so ν_recv has no \
         denominator: {last}"
    );
    assert!(
        (0.0..=1.0).contains(&nu_recv),
        "[RFA] nu_recv={nu_recv} is fires per SOURCE ARRIVAL and cannot \
         exceed 1: {last}"
    );
    assert!(
        (nu_recv - fires as f64 / src_n as f64).abs() < 1e-3,
        "[RFA] nu_recv={nu_recv} disagrees with {fires}/{src_n}: {last}"
    );

    // The `[RACK]` slots this feeds, if the receiver task reached its `Drop`
    // (not guaranteed — see `ARM`); where the line is present it must agree.
    if let Some(rack) = log.lines().rev().find(|l| l.contains("[RACK] ")) {
        let fa = str_field(rack, "fa=");
        let (sp, fd) = fa
            .split_once('/')
            .unwrap_or_else(|| panic!("fa= must render `<spurious>/<fired>`: {rack}"));
        let (sp, fd): (u64, u64) = (sp.parse().unwrap(), numeric_prefix(fd).parse().unwrap());
        println!("[rfa-reach] receiver [RACK] fa={sp}/{fd}");
        assert_eq!(
            fd, fires,
            "[RACK] fired={fd} disagrees with [RFA] fires={fires} — the two \
             slots are fed by the same events:\n{rack}\n{last}"
        );
        assert_eq!(
            sp, falses,
            "[RACK] spurious={sp} disagrees with [RFA] false={falses}:\n{rack}\n{last}"
        );
    }
}
