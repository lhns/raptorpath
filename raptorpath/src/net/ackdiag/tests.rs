use super::*;

fn snap(id: u32) -> PathSnapshot {
    PathSnapshot {
        path_id: id,
        anchor_syms: 0.0,
        rtprop_s: 0.0,
        sent_total: 0,
        src_ack: 0,
    }
}

/// **Readout 1, ABSOLUTE.** A known arrival train produces exactly its own
/// spacing quantiles — nearest-rank, in µs, with n = arrivals − 1 (the
/// first arrival opens the series, it does not close a gap).
#[test]
fn arrival_spacing_quantiles_are_the_injected_train() {
    let g = AckCadenceGauge::new();
    // Ten arrivals 1 ms apart, then one 50 ms late: gaps are
    // [1000 ×9, 50000], so p50 = 1000 and the max/p99 = 50000.
    let mut t = 0u64;
    g.note_ack_at(t, 0, 1, 1);
    for _ in 0..9 {
        t += 1_000;
        g.note_ack_at(t, 0, 1, 1);
    }
    t += 50_000;
    g.note_ack_at(t, 0, 1, 1);
    let line = g.report_line(snap(0), t).expect("path 0 reported");
    assert!(
        line.contains("gap_us[p50=1000 p90=1000 p99=50000 n=10]"),
        "spacing quantiles are not the injected train: {line}"
    );
    assert!(line.contains("acks=11/z=0(0.0%)"), "{line}");
}

/// **Readout 2, ABSOLUTE.** The zero-delta class is counted, NOT sampled:
/// a `(0, 0)` ack raises `z=` and must not enter the `drecv` series (it
/// would drag every quantile toward zero and make a stale-ack storm look
/// like a low-delivery cell).
#[test]
fn zero_delta_acks_are_counted_and_excluded_from_the_delta_series() {
    let g = AckCadenceGauge::new();
    let mut t = 0u64;
    for i in 0..10u32 {
        t += 1_000;
        // Five real acks of 4 symbols, five sentinels.
        if i % 2 == 0 {
            g.note_ack_at(t, 0, 4, 4);
        } else {
            g.note_ack_at(t, 0, 0, 0);
        }
    }
    let line = g.report_line(snap(0), t).expect("path 0 reported");
    assert!(line.contains("acks=10/z=5(50.0%)"), "{line}");
    assert!(
        line.contains("drecv[p50=4 p90=4 max=4 n=5 sum=20]"),
        "the delta series must hold the five NON-zero acks only: {line}"
    );
}

/// **Readout 3, ABSOLUTE.** The over-read is the sample rate over the
/// window's own long-run rate, and nothing else. Feed 1000 sym over a
/// 1 s window (⇒ `rate_lr` = 1000 sym/s) with accepted samples at 1000,
/// 5000 and 10000 sym/s: x must read exactly 1.00 / 5.00 / 10.00.
#[test]
fn realized_overread_is_the_sample_rate_over_the_windows_own_long_run_rate() {
    let g = AckCadenceGauge::new();
    // Three calls of 200 sym each are accepted; 400 more arrive on
    // rejected (sub-1 ms) calls, so `count` sums to 1000 either way —
    // the denominator must see the delivery the SAMPLER saw, accepted or
    // not, or the ratio is inflated by the rejection rate.
    g.note_rate_sample(0, 200, 1_000.0, true);
    g.note_rate_sample(0, 200, 5_000.0, true);
    g.note_rate_sample(0, 200, 10_000.0, true);
    g.note_rate_sample(0, 400, 0.0, false);
    // Report at exactly 1 s of gauge-epoch window.
    let mut s = snap(0);
    {
        let mut m = g.paths.lock();
        m.get_mut(&0).unwrap().win_start_us = 0;
    }
    s.anchor_syms = 500.0;
    s.rtprop_s = 0.100; // ⇒ xanchor = 500 / (1000 · 0.1) = 5.00
    let line = g.report_line(s, 1_000_000).expect("path 0 reported");
    assert!(line.contains("rate_lr=1000sym/s"), "{line}");
    assert!(
        line.contains("x[p50=5.00 p90=10.00 p99=10.00]"),
        "per-sample over-read must be rate/rate_lr exactly: {line}"
    );
    assert!(
        line.contains("xanchor=5.00"),
        "xanchor must be the ledger's anchor/(rate·RTprop): {line}"
    );
    assert!(line.contains("rd[acc=3 rej=1 cnt=1000]"), "{line}");
}

/// **Readout 3's guard.** With no delivery in the window there is no
/// denominator, and the gauge must print `-` rather than a divide — a
/// fabricated over-read is exactly the failure this instrument exists to
/// end.
#[test]
fn overread_is_undefined_not_invented_when_the_window_delivered_nothing() {
    let g = AckCadenceGauge::new();
    g.note_ack_at(0, 0, 0, 0);
    g.note_ack_at(1_000, 0, 0, 0);
    let line = g.report_line(snap(0), 1_000_000).expect("path 0 reported");
    assert!(line.contains("rate_lr=0sym/s"), "{line}");
    assert!(line.contains("x[p50=- p90=- p99=-]"), "{line}");
    assert!(line.contains("xanchor=-"), "{line}");
}

/// **Readout 4, ABSOLUTE.** The reconciliation ratios are arithmetic over
/// cumulative counters, with no constant anywhere: `cr/s` = Σd_received /
/// symbols_sent and `ce/cr` = Σd_expected / Σd_received.
#[test]
fn reconciliation_ratios_are_arithmetic_over_the_cumulative_counters() {
    let g = AckCadenceGauge::new();
    // 100 acks × (expected 11, received 10) = 1100 / 1000.
    let mut t = 0u64;
    for _ in 0..100 {
        t += 1_000;
        g.note_ack_at(t, 0, 11, 10);
    }
    let mut s = snap(0);
    s.sent_total = 1_000; // every wire symbol acked ⇒ cr/s = 1.000
    // 800 delivered SOURCE symbols against 1000 counted arrivals ⇒
    // cr/sa = 1.250: the counters hold 200 symbols the source frontier
    // does not, which is exactly the repair/retransmit signature.
    s.src_ack = 800;
    let line = g.report_line(s, t).expect("path 0 reported");
    assert!(
        line.contains(
            "recon[sent=1000 crecv=1000 cexp=1100 srcack=800 cr/s=1.000 \
             ce/cr=1.100 cr/sa=1.250]"
        ),
        "{line}"
    );
}

/// The cumulative totals SURVIVE a window boundary and the window series
/// do not — the property that makes the last line of a run the accounting
/// read while each line is still a distribution over its own window.
#[test]
fn windows_reset_the_series_and_carry_the_totals() {
    let g = AckCadenceGauge::new();
    let mut t = 0u64;
    for _ in 0..5 {
        t += 1_000;
        g.note_ack_at(t, 0, 2, 2);
    }
    let first = g.report_line(snap(0), t).expect("first window");
    assert!(first.contains("acks=5/") && first.contains("crecv=10"), "{first}");
    for _ in 0..5 {
        t += 1_000;
        g.note_ack_at(t, 0, 2, 2);
    }
    let second = g.report_line(snap(0), t).expect("second window");
    assert!(
        second.contains("acks=5/") && second.contains("crecv=20"),
        "the window count must reset and the total must not: {second}"
    );
}

/// A path with nothing to report prints nothing, so a `0` on an
/// `[ACKDIAG]` line is always a MEASURED zero (the `dgq` discipline:
/// a gauge that reads 0 for two different reasons is not a gauge).
#[test]
fn a_silent_path_emits_no_line() {
    let g = AckCadenceGauge::new();
    assert!(g.report_line(snap(7), 1_000).is_none());
    g.note_ack_at(1_000, 7, 1, 1);
    assert!(g.report_line(snap(7), 2_000).is_some());
    // Reported and then silent again ⇒ silent again.
    assert!(g.report_line(snap(7), 3_000).is_none());
}

/// The per-window cap is REPORTED, never silent: a truncated window must
/// be distinguishable from a complete one.
#[test]
fn the_sample_cap_is_reported_not_hidden() {
    let g = AckCadenceGauge::new();
    let mut t = 0u64;
    for _ in 0..(SAMPLE_CAP + 10) {
        t += 10;
        g.note_ack_at(t, 0, 1, 1);
    }
    let line = g.report_line(snap(0), t).expect("path 0 reported");
    assert!(!line.contains(" ov=0"), "the cap must be visible: {line}");
}

/// **THE BEHAVIOUR-NEUTRALITY PIN.** The gauge is observation-only, and
/// this is asserted STRUCTURALLY rather than promised in prose: no method
/// on this module may reach back into the engine's mutable state. The
/// gauge's own source must contain no `&mut Scheduler` / `path_mut` /
/// `set_` call and no write to any engine handle — its only writes are to
/// its OWN fields, and its only engine reads are the four immutable
/// snapshot values in [`PathSnapshot`].
///
/// Why a source scrape and not a runtime assertion: the failure mode is
/// someone LATER adding a convenient write here (the `[SF]` gauge's own
/// history), and that omission has no runtime symptom to assert on — the
/// same reasoning `gates::forwarding_audit` and the wait-bucket audit
/// already use in this crate.
#[test]
fn ackdiag_is_observation_only() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/ackdiag.rs"),
    )
    .expect("read src/net/ackdiag.rs");
    // The GAUGE, not this test module — the forbidden list below is
    // itself a set of those literals.
    let src = &src[..src.find("#[cfg(test)]").expect("the test module marker")];
    // Strip comments and doc comments: the module header NAMES these
    // things to explain why it does not do them.
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "path_mut",
        "&mut Scheduler",
        "set_cc_window_bytes",
        "release_in_flight",
        "charge_in_flight",
        "record_delivery(",
        "on_delivery_signal",
    ] {
        assert!(
            !code.contains(forbidden),
            "the ack-cadence gauge must not touch engine state: found `{forbidden}`"
        );
    }
    // And the ONE mutable engine borrow it could plausibly acquire — the
    // scheduler lock — must be read-only: `sched.path(` , never
    // `scheduler.lock()` results used mutably.
    assert!(
        code.contains(".path(*id)"),
        "the snapshot must read paths through the IMMUTABLE `Scheduler::path` \
         accessor — `path_mut` is on the forbidden list above"
    );
}

/// **THE WINDOW OVERRIDE, ABSOLUTE.** The default is the SHIPPED 2 s and
/// every path into it is pinned to a number — an ordinal "smaller than
/// the default" test would pass on a resolver that returned garbage.
///
/// The default arm is the load-bearing one: every committed `[ACKDIAG]`
/// ledger was captured at 2 s and the window is the unit of every series
/// read off them, so an override that shifted the DEFAULT would silently
/// re-unit the whole era's record.
#[test]
fn the_window_override_resolves_to_absolute_values_and_defaults_unchanged() {
    // ERA COMPARABILITY: unset is 2 s, exactly as before the override.
    assert_eq!(resolve_window_us(None), 2_000_000);
    assert_eq!(resolve_window_us(None), ACKDIAG_WINDOW_US);
    // The c9 arm's value, which is the `[DIAG]` line's own cadence and
    // the blocking dependency C9-1..4 are written against.
    assert_eq!(resolve_window_us(Some("250000")), 250_000);
    // Whitespace is trimmed (an `env` prefix can carry it).
    assert_eq!(resolve_window_us(Some("  250000 ")), 250_000);
    // GARBAGE FALLS BACK TO THE DEFAULT, never to 0 and never to a panic.
    // A 0 window would fire a report on every sender-loop iteration; an
    // unparseable one is a driver typo, and both must be VISIBLE in the
    // echo as "your override did not take" rather than as a dead or
    // screaming gauge.
    for bad in ["", "0", "abc", "-1", "250_000", "2e5", "250000ms"] {
        assert_eq!(
            resolve_window_us(Some(bad)),
            ACKDIAG_WINDOW_US,
            "{bad:?} must fall back to the shipped default"
        );
    }
    // THE CLAMP, at both ends and on both sides of each edge.
    assert_eq!(resolve_window_us(Some("1")), ACKDIAG_WINDOW_US_MIN);
    assert_eq!(resolve_window_us(Some("49999")), ACKDIAG_WINDOW_US_MIN);
    assert_eq!(resolve_window_us(Some("50000")), ACKDIAG_WINDOW_US_MIN);
    assert_eq!(resolve_window_us(Some("60000000")), ACKDIAG_WINDOW_US_MAX);
    assert_eq!(
        resolve_window_us(Some("999999999")),
        ACKDIAG_WINDOW_US_MAX
    );
    // And the clamp does not touch anything inside its own range.
    assert_eq!(resolve_window_us(Some("2000000")), 2_000_000);
}

/// The window is what `report_due` actually gates on — the wiring, not
/// just the resolver (MEASUREMENT DISCIPLINE rule 1: prove the mechanism
/// under test executes). A resolver that no caller reads is a constant.
#[test]
fn report_due_gates_on_the_active_window() {
    let g = AckCadenceGauge::new();
    let w = window_us();
    // First call only stamps the epoch — it never reports.
    assert!(!g.report_due(1_000));
    // One µs short of the window: still closed.
    assert!(!g.report_due(1_000 + w - 1));
    // Exactly one window: open.
    assert!(g.report_due(1_000 + w));
    // And the window re-arms from the new stamp, not from the old one.
    assert!(!g.report_due(1_000 + w + 1));
    assert!(g.report_due(1_000 + 2 * w));
}

/// The gate ships OFF, so the gauge is absent and every feed site is a
/// null check. (Set-env semantics are `config::env_flag`'s.)
#[test]
fn the_gauge_is_absent_on_the_shipped_default() {
    // NOTE: relies on the test env not exporting RWM_ACKDIAG — the same
    // assumption every engine-default test in this crate makes.
    if std::env::var("RWM_ACKDIAG").is_ok() {
        return;
    }
    assert!(
        gauge().is_none(),
        "RWM_ACKDIAG ships default OFF: the gauge must not exist"
    );
}
