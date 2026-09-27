//! THE RECEIVER'S REALIZED FALSE-REPAIR GAUGE FIRES — `[RFA]` — AND THE
//! CONFIGURATION CONTRACT UNDER WHICH `fa=` IS A MEASUREMENT AT ALL.
//!
//! **The premise, corrected before this was written.** It is NOT true that
//! `RackClockGauge::record_fire` is unreachable. It has one call site — the
//! sender's gap-driven retransmit loop fed by `recv_nack_tx` — and that
//! channel is `None` under GENERATION CODING and only there (`net/mod.rs`, the
//! §16.3 suppression). The goal #100 primitives pass ran generation ON, so its
//! `fired = 0` at 15/15 was structural to THAT configuration. **The α-sweep
//! (goal #100 item 2) runs PLAIN WINDOW, where the sender's `fa=` already
//! works.** This binary measures that claim rather than assuming it either
//! way, in both configurations, which is the first thing it asserts.
//!
//! **What was genuinely missing.** The sender's `fa=` is a PREDICTION — at
//! fire time it asks whether the target's flight is younger than its own law
//! threshold, i.e. whether the data *was going to* arrive anyway. The α-sweep
//! scores REALIZED against commanded, and realized — the repair was emitted
//! AND the original arrived anyway — is only observable where both copies
//! land, at the RECEIVER, whose gauge (`receiver.rs`) called `record`
//! (evaluations) and never `record_fire` at all, so a receiver-role `[RACK]`
//! read `fa=0/0` on every invocation since it was written
//! (`tools/l1/ccand_battery.sh` instrument facts 5 and 7(b) say exactly this).
//! `[RFA]` is that second term.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. `classify_recv_repair` is TOTAL and its truth table is the event-class
//!      definition. Pure, so it fails instantly and locally when the class
//!      definition drifts from the doc comment it is written under.
//!   2. `rfa_report_line`'s FORMAT, so an L1 parser has a pin.
//!   3. **THE CONFIGURATION CONTRACT, BOTH SIDES.** Over a LOSSY loopback
//!      (`RWM_L0_NETEM=c3`, the L1 `c3` cell's Gilbert-Elliott ε ≈ 4.8 % on
//!      client egress — loss is what makes repairs exist at all):
//!      **PLAIN WINDOW** ⇒ the sender's `[RACK] fa=<n>/<fired>` has
//!      `fired > 0` and `[DIAG] retx > 0`; **GENERATION** ⇒ `retx = 0`. This
//!      is the fact the α-sweep's arm list depends on, and it is measured here
//!      rather than read off the source.
//!   4. The receiver emits `[RFA]` **with `fires > 0` and `src_n > 0`** in
//!      plain window. THIS IS THE ASSERTION THAT FAILS ON THE SHIPPED-BEFORE
//!      ENGINE: the line does not exist there and no receiver-site counter was
//!      ever fed. Under generation the same line reads `gen=1 src_n=0` —
//!      structurally empty, and SAYING so on the line rather than leaving a
//!      reader to infer which machine a row belongs to.
//!   5. The line is INTERNALLY CONSISTENT and in range: the four classes sum
//!      to `fires`, the two redundant ones sum to `false`, `false_frac` is a
//!      fraction, and `ν_recv = fires / src_n` is one too. A gauge that
//!      double-counts a class is caught here rather than in a results table.
//!
//! **No new gate.** The periodic readout rides the EXISTING `RWM_DIAG` /
//! `RWM_FDIAG` gates (both already in the two-sided `[GATES]` echo) and the
//! `Drop` emission rides `[RACK]`'s own ungated rule, so there is no new gate
//! to echo two-sidedly. `RWM_DIAG=1` IS asserted present in the `[GATES]` echo
//! below, so a missing `[RFA]` can only be read as an unreached emission site.
//!
//! **What this deliberately does NOT assert.** Any particular VALUE of the
//! realized false-repair fraction, or of the gap between it and the commanded
//! one. Loopback's redundancy is the shim's GE process and the host
//! scheduler's, not a network's; the number that scores against RFC 8985's
//! 6.25 % class bar comes off an L1 run. This is the instrument gate that must
//! pass before that run is worth making.
//!
//! Own test binary: `RWM_L0_NETEM` is process-global in the child, and the
//! spawned pair must not contend with the in-process loopback tests.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{f64_field, numeric_prefix, str_field, u64_field};

use raptorpath::net::{classify_recv_repair, rfa_report_line, RecvRepair};

// ── 1 + 2: THE PURE PINS ────────────────────────────────────────────────

#[test]
fn the_event_class_is_exactly_what_the_alpha_sweep_needs() {
    // A FALSE repair = a repair emitted whose original arrived anyway. At the
    // receiver that is REDUNDANCY, and there are exactly two redundant
    // observations. `seen_as_source` DOMINATES `recovered`: a second source
    // copy is a wasted transmission whatever the decoder did in the meantime.
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
    // v8 ADDITIVE: `rep_redundant` (the false measurand under coded answers)
    // and `late_after_aban` (a copy that arrived after the give-up -- the
    // EVICT-seat counter `tools/l1/tail_matrix.sh` greps for BY NAME) are
    // APPENDED to the END of the line, so every field a parser already reads
    // keeps its position and its meaning.
    // fill_coded=9 fill_src=3 dup_src=4 preempt_src=1 over src_n=1000:
    //   fires = 17, false = 5, false_frac = 5/17, nu_recv = 17/1000.
    assert_eq!(
        rfa_report_line(9, 3, 4, 1, 1000, 200, false, 41, 7),
        "[RFA] gen=0 fires=17 false=5 false_frac=0.2941 fill_coded=9 \
         fill_src=3 dup_src=4 preempt_src=1 src_n=1000 rep_n=200 \
         nu_recv=0.01700 fa_class=0.0625 rep_redundant=41 late_after_aban=7"
    );
    // THE GENERATION ROW. `src_n = 0` is structural there, so every ratio
    // formed on it must read 0 rather than divide — and `gen=1` says why.
    assert_eq!(
        rfa_report_line(0, 0, 0, 0, 0, 9730, true, 0, 0),
        "[RFA] gen=1 fires=0 false=0 false_frac=0.0000 fill_coded=0 \
         fill_src=0 dup_src=0 preempt_src=0 src_n=0 rep_n=9730 \
         nu_recv=0.00000 fa_class=0.0625 rep_redundant=0 late_after_aban=0"
    );
}

// ── 3-5: THE REACHABILITY RUN ───────────────────────────────────────────

/// The arm. `RWM_DIAG` carries the periodic `[RFA]` readout (the L1 harnesses
/// SIGKILL the server, so a `Drop`-only emission is unreachable there and this
/// test must not depend on one either). No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One lossy loopback: the perf SERVER is the receiver of the bulk direction,
/// and the site whose `[RFA]` this test is about. The L1 `c3` cell
/// (LTE-class: 20 Mbit, 20 ms one-way, 5 ms jitter, GE p = 2 % / q = 40 % ⇒
/// ε ≈ 4.8 %) shapes client egress, seeded — loss is what forces the
/// repair-class events this gauge is about to exist; the server's own
/// datagram path (the ack direction) is left clean. The server log is taken
/// once the receiver's periodic `[RFA]` readout post-dating the transfer
/// landed. Returns `(client log, server log)`.
fn lossy_run(generation: bool) -> (String, String) {
    let extra: &[&str] = if generation { &["--window-generation-coding"] } else { &[] };
    loopback::transfer(loopback::Transfer {
        env: &ARM,
        extra_args: extra,
        srv_tag: Some("[RFA] "),
        ..Default::default()
    })
}

/// The LAST `retx=<n>` the sender printed — `[DIAG]`'s cumulative retransmit
/// count, the independent witness for whether the gap loop ran at all.
fn last_retx(log: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix("retx="))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .next_back()
        .unwrap_or_else(|| panic!("no `retx=` in the sender log — [DIAG] never fired"))
}

/// 3. THE CONFIGURATION CONTRACT, MEASURED ON BOTH SIDES.
///
/// The α-sweep's arm list depends on this and nothing else: the sender's
/// `fa=` denominator is alive in PLAIN WINDOW and structurally dead under
/// GENERATION, because `recv_nack_tx` is `None` there and no gap ever reaches
/// the loop `record_fire` lives in.
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

    // THE OTHER SIDE. Generation suppresses the SACK→gap producer
    // (`recv_nack_tx = None`), so the per-seq retransmit path does not run —
    // which is why the primitives pass measured fired = 0 at 15/15 and why
    // that reading is a CONFIGURATION fact, not a dead instrument.
    let (gen_cli, gen_srv) = lossy_run(true);
    let gen_retx = last_retx(&gen_cli);
    println!("[rfa-reach] GENERATION sender retx={gen_retx}");
    assert_eq!(
        gen_retx, 0,
        "[DIAG] retx={gen_retx} under GENERATION CODING — `recv_nack_tx` is \
         supposed to be None there, so the per-seq retransmit path must not \
         run and the primitives pass's fired=0 must stay explained:\n{gen_cli}"
    );

    // And the receiver's line SAYS which machine it measured, so no α-sweep
    // row is ever read out of its configuration scope.
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

    // THE GATE. A missing `[RFA]` must be readable as an unreached emission
    // site and never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the server's [GATES] echo does not carry RWM_DIAG=1 — the arm did \
         not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the server's [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );

    // 4. THE LINE FIRES, AND IT FIRES NONZERO. This is what fails on the
    //    shipped-before engine: no `[RFA]` at all, and no receiver-site
    //    counter behind `[RACK]`'s `fa=` was ever fed.
    let rfa: Vec<&str> = log.lines().filter(|l| l.contains("[RFA] ")).collect();
    assert!(
        !rfa.is_empty(),
        "no [RFA] line from the RECEIVER over a lossy transfer — the \
         false-repair gauge is unreachable:\n{log}"
    );
    // Cumulative counters: the LAST line is the reading.
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

    // 5. INTERNAL CONSISTENCY. A class counted twice, or a fraction formed on
    //    the wrong denominator, is caught at the instrument.
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

    // THE DENOMINATOR IS FED, and ν_recv is formed on it.
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

    // The `[RACK]` slots this feeds, IF the receiver task reached its `Drop`.
    // Not asserted as a precondition — the whole reason for the periodic
    // readout above is that a receiver `Drop` is not reachable under the L1
    // harnesses' SIGKILL — but where the line IS present it must agree.
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
