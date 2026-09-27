use super::*;
use std::time::Duration;

/// A hole closed by the sender's copy is not a self-heal: `[LATE]`'s
/// pi0 must not count it, while a late original (stamped before the
/// arrival that exposed the hole) still is one.
#[test]
fn a_retransmit_resolved_hole_is_not_counted_as_self_heal() {
    let t = Instant::now();
    let mut g = SuccGauge::new(false, false, 0);
    let mut late = crate::net::late::LateGauge::default();
    g.observe_high_at(0, t, 0, 1_000);
    g.observe_high_at(2, t, 0, 1_020); // exposes seq 1
    // seq 1 closes with a copy the sender stamped after the exposer.
    let rec = g
        .resolve_at(1, false, t + Duration::from_millis(30), 0, 40_000)
        .expect("seq 1 is an open hole");
    assert_eq!(rec.outcome, HoleOutcome::Retransmit, "the copy is a retransmit");
    late.note_hole(rec.outcome, rec.cross, rec.us, rec.hi_us);
    assert_eq!(
        late.pi0(),
        Some(0.0),
        "a retransmit-resolved hole must not be counted in pi0"
    );
    // Control: seq 3's original (stamped before its exposer) heals it.
    g.observe_high_at(4, t, 0, 1_060); // exposes seq 3
    let rec = g
        .resolve_at(3, false, t + Duration::from_millis(5), 1, 1_040)
        .expect("seq 3 is an open hole");
    assert_eq!(rec.outcome, HoleOutcome::Original, "a late original is a self-heal");
    late.note_hole(rec.outcome, rec.cross, rec.us, rec.hi_us);
    assert_eq!(late.pi0(), Some(0.5), "one heal of two holes at risk");
    // Both stay in `orig_n` (own-source arrivals); `rtx_n` is the copy.
    assert_eq!(g.hist(HoleOutcome::Original).n(), 2);
    assert_eq!(g.rtx_n(), 1);
    assert!(g.line().ends_with(" rtx_n=1"), "{}", g.line());
    // Unknown stamps (0) read as `Original`.
    assert_eq!(classify_source_close(5, 0), HoleOutcome::Original);
    assert_eq!(classify_source_close(0, 5), HoleOutcome::Original);
}

fn at(base: Instant, us: u64) -> Instant {
    base + Duration::from_micros(us)
}

// ── The bucket map ──────────────────────────────────────────────────

#[test]
fn buckets_are_monotone_contiguous_and_bounded_in_width() {
    // Exact below SUB.
    for v in 0..SUB {
        assert_eq!(bucket_of(v), v as usize, "sub-SUB buckets are exact");
        assert_eq!(bucket_lower_edge(v as usize), v);
    }
    // `bucket_lower_edge` is the inverse of `bucket_of` on edges, and
    // `bucket_of` is monotone. Both are what a quantile depends on.
    let mut prev = 0usize;
    let mut v = 1u64;
    while v < (1u64 << 40) {
        let b = bucket_of(v);
        assert!(b >= prev, "bucket_of must be monotone at {v}");
        prev = b;
        let lo = bucket_lower_edge(b);
        assert!(lo <= v, "bucket {b} lower edge {lo} exceeds its member {v}");
        assert_eq!(bucket_of(lo), b, "edge {lo} must map back to bucket {b}");
        // The declared error: a reported quantile is the lower edge, so
        // the underestimate is bounded by the bucket's relative width.
        if v >= SUB {
            let hi_edge = bucket_lower_edge(b + 1);
            assert!(
                (hi_edge - lo) as f64 / lo as f64 <= 0.1305,
                "bucket {b} spans {lo}..{hi_edge}, wider than 2^(1/8) allows"
            );
        }
        v = v + 1 + v / 97;
    }
    // Every u64 lands inside the declared array.
    assert!(bucket_of(u64::MAX) < BUCKETS, "u64::MAX must be in range");
}

#[test]
fn quantiles_are_lower_edges_and_absent_reads_as_none() {
    let mut h = Hist::default();
    assert_eq!(h.quantile(0.5), None, "no sample ⇒ no quantile, never 0");
    assert_eq!(h.mean_us(), None);
    for us in 1..=100u64 {
        h.add(us);
    }
    assert_eq!(h.n(), 100);
    assert_eq!(h.max_us(), 100);
    let p50 = h.quantile(0.5).expect("100 samples");
    // Rank 50 ⇒ the sample 50 ⇒ its bucket's lower edge, at most 9.05 %
    // below it and never above it.
    assert!(p50 <= 50, "a lower-edge quantile never overestimates: {p50}");
    assert!(p50 as f64 >= 50.0 / 1.1305, "and is bounded below: {p50}");
    assert_eq!(h.quantile(1.0), Some(bucket_lower_edge(bucket_of(100))));
    assert!(h.quantile(0.0).expect("nonempty") <= p50);
}

// ── The event semantics ─────────────────────────────────────────────

#[test]
fn the_first_arrival_exposes_nothing_and_the_second_exposes_the_gap() {
    let t = Instant::now();
    let mut g = SuccGauge::new(false, false, 0);
    assert!(!g.is_receiver_site(), "a gauge with no arrival is not a receiver");
    // The flow's first symbol is seq 7 — that is a baseline, not seven
    // holes. A gauge that opened holes below its first-ever arrival would
    // manufacture its own denominator.
    g.observe_high(7, t, 0);
    assert!(g.is_receiver_site());
    assert_eq!(g.det_n(), 0, "the first arrival exposes no hole");
    assert_eq!(g.open_n(), 0);
    // seq 10 arrives: 8 and 9 have never been seen, so both are exposed.
    g.observe_high(10, t, 0);
    assert_eq!(g.det_n(), 2);
    assert_eq!(g.open_n(), 2);
    // A seq at or below the mark exposes nothing.
    g.observe_high(9, t, 0);
    g.observe_high(10, t, 0);
    assert_eq!(g.det_n(), 2, "a non-advancing arrival exposes no hole");
}

#[test]
fn the_three_outcomes_are_disjoint_and_the_first_terminal_event_wins() {
    let t = Instant::now();
    let mut g = SuccGauge::new(false, false, 0);
    g.observe_high(0, t, 0);
    g.observe_high(4, t, 0); // exposes 1, 2, 3
    assert_eq!(g.det_n(), 3);

    g.resolve(1, false, at(t, 500), 0); // original, 500 µs
    g.resolve(2, true, at(t, 1500), 0); // repair, 1500 µs
    g.abandon_below(4, at(t, 9000)); // 3 given up, 9000 µs

    assert_eq!(g.hist(HoleOutcome::Original).n(), 1);
    assert_eq!(g.hist(HoleOutcome::Repair).n(), 1);
    assert_eq!(g.hist(HoleOutcome::Abandoned).n(), 1);
    assert_eq!(g.hist(HoleOutcome::Original).max_us(), 500);
    assert_eq!(g.hist(HoleOutcome::Repair).max_us(), 1500);
    assert_eq!(g.hist(HoleOutcome::Abandoned).max_us(), 9000);

    // A late arrival of an abandoned seq is not a second outcome. This is
    // the property that makes the three classes a partition rather than
    // three overlapping counts.
    g.resolve(3, false, at(t, 20_000), 0);
    g.resolve(1, true, at(t, 20_000), 0);
    assert_eq!(g.hist(HoleOutcome::Original).n(), 1, "no double-count");
    assert_eq!(g.hist(HoleOutcome::Repair).n(), 1);
    assert_eq!(g.hist(HoleOutcome::Abandoned).n(), 1);
}

#[test]
fn the_accounting_identity_holds_including_at_the_declared_bounds() {
    let t = Instant::now();
    let identity = |g: &SuccGauge| {
        assert_eq!(
            g.det_n(),
            g.hist(HoleOutcome::Original).n()
                + g.hist(HoleOutcome::Repair).n()
                + g.hist(HoleOutcome::Abandoned).n()
                + g.open_n()
                + g.over_n(),
            "det = orig + rep + aban + open + over"
        );
    };

    let mut g = SuccGauge::new(false, false, 0);
    g.observe_high(0, t, 0);
    for s in 1..200u64 {
        g.observe_high(s * 3, t, 0); // exposes two holes per step
        identity(&g);
    }
    for s in 1..100u64 {
        g.resolve(s * 3 - 1, s % 2 == 0, at(t, 100 * s), 0);
        identity(&g);
    }
    g.abandon_below(200, at(t, 999_999));
    identity(&g);

    // MAX_SPAN: a jump wider than the bound is counted whole and tracked
    // not at all, so the identity survives the truncation that protects
    // the gauge's memory.
    let mut g2 = SuccGauge::new(false, false, 0);
    g2.observe_high(0, t, 0);
    g2.observe_high(MAX_SPAN + 10, t, 0);
    assert_eq!(g2.det_n(), MAX_SPAN + 9);
    assert_eq!(g2.over_n(), MAX_SPAN + 9, "a too-wide span is all `over`");
    assert_eq!(g2.open_n(), 0);
    identity(&g2);

    // MAX_OPEN: the map stops growing and the excess lands in `over`.
    let mut g3 = SuccGauge::new(false, false, 0);
    g3.observe_high(0, t, 0);
    let mut next = 0u64;
    while g3.open_n() < MAX_OPEN as u64 {
        next += MAX_SPAN;
        g3.observe_high(next, t, 0);
    }
    assert_eq!(g3.open_n(), MAX_OPEN as u64);
    let before = g3.over_n();
    g3.observe_high(next + 100, t, 0);
    assert_eq!(g3.over_n(), before + 99, "past the cap, detections are `over`");
    identity(&g3);
}

// ── The derived readings ────────────────────────────────────────────

#[test]
fn the_crossing_point_is_where_repair_overtakes_original() {
    let t = Instant::now();
    let mut g = SuccGauge::new(false, false, 0);
    g.observe_high(0, t, 0);
    for s in 1..=200u64 {
        g.observe_high(s * 2, t, 0);
    }
    // Originals: fast (≈1 ms). Repairs: slow (≈50 ms), and more numerous,
    // so the repair CDF must overtake somewhere between the two clusters.
    for s in 1..=50u64 {
        g.resolve(s * 2 - 1, false, at(t, 1_000 + s), 0);
    }
    for s in 51..=200u64 {
        g.resolve(s * 2 - 1, true, at(t, 50_000 + s), 0);
    }
    let cross = g.crossing_us().expect("repairs outnumber originals");
    assert!(
        (1_000..=50_000).contains(&cross),
        "the crossing must sit between the two clusters, got {cross}"
    );
    assert!(
        (g.orig_frac().expect("resolved") - 0.25).abs() < 1e-9,
        "50 of 200 resolved holes closed by their original"
    );

    // No crossing is a legal outcome, not a missing value: when the
    // original leads at every horizon there is no `t` to report.
    let mut h = SuccGauge::new(false, false, 0);
    h.observe_high(0, t, 0);
    for s in 1..=20u64 {
        h.observe_high(s * 2, t, 0);
        h.resolve(s * 2 - 1, false, at(t, 1_000), 0);
    }
    assert_eq!(h.crossing_us(), None, "originals only ⇒ no crossing");
    assert_eq!(h.orig_frac(), Some(1.0));

    // And an empty gauge reports neither.
    let e = SuccGauge::new(false, false, 0);
    assert_eq!(e.crossing_us(), None);
    assert_eq!(e.orig_frac(), None);
}

// ── The line ────────────────────────────────────────────────────────

#[test]
fn the_succ_line_format_is_pinned() {
    let mut orig = Hist::default();
    orig.add(1_000);
    orig.add(2_000);
    let mut rep = Hist::default();
    rep.add(40_000);
    let aban = Hist::default();
    let (mut sp, mut xp) = (Hist::default(), Hist::default());
    sp.add(700);
    sp.add(900);
    xp.add(30_000);
    let l = succ_report_line(
        false, 7, &orig, &rep, &aban, &sp, &xp, 3, 1, Some(2048), false, 0,
    );
    assert_eq!(
        l,
        // 1000 µs ⇒ bucket [960, 1024); 2000 ⇒ [1920, 2048); 40 000 ⇒
        // [36864, 40960). Every quantile is its bucket's lower edge, so
        // each reads at or below the sample it summarises and never above
        // it — and `mx` carries the exact maximum beside it, so the
        // bucketing's direction is checkable off the line itself.
        "[SUCC] gen=0 det=7 res=3 orig_n=2 orig_p50_us=960 orig_p90_us=1920 \
         orig_p99_us=1920 orig_mx_us=2000 orig_mean_us=1500 rep_n=1 \
         rep_p50_us=36864 rep_p90_us=36864 rep_p99_us=36864 rep_mx_us=40000 \
         rep_mean_us=40000 aban_n=0 aban_p50_us=- aban_p90_us=- aban_p99_us=- \
         aban_mx_us=- aban_mean_us=- open=3 over=1 orig_frac=0.6667 \
         cross_us=2048 dump=0/0 sp_n=2 xp_n=1 xp_frac=0.3333 \
         sp_p50_us=640 sp_p90_us=896 xp_p50_us=28672 xp_p90_us=28672"
    );
    // `-` iff none, on every slot of an empty outcome, and never a 0 that
    // a parser would read as a measured zero.
    assert!(l.contains("aban_n=0 aban_p50_us=-"));
    // The generation row: the line says which machine it measured.
    let e = Hist::default();
    let g = succ_report_line(true, 0, &e, &e, &e, &e, &e, 0, 0, None, true, 12);
    assert!(g.starts_with("[SUCC] gen=1 det=0 res=0 "), "{g}");
    assert!(g.contains("orig_frac=- cross_us=- dump=1/12"), "{g}");
    assert!(
        g.ends_with(
            "sp_n=0 xp_n=0 xp_frac=- sp_p50_us=- sp_p90_us=- xp_p50_us=- xp_p90_us=-"
        ),
        "the A0.3 split renders `-` iff none, and sits at the END of the \
         line so every prior reader keeps its offsets: {g}"
    );
}

#[test]
fn the_raw_dump_is_off_by_default_batched_and_announces_its_own_cap() {
    let t = Instant::now();
    // Off: not one line, whatever happens.
    let mut off = SuccGauge::new(false, false, 0);
    off.observe_high(0, t, 0);
    off.observe_high(100, t, 0);
    for s in 1..100u64 {
        off.resolve(s, false, at(t, s), 0);
    }
    assert!(off.take_dump_lines(true).is_empty(), "the dump ships OFF");

    // On: full batches only until flushed, then the tail.
    let mut on = SuccGauge::new(false, true, 1_000);
    on.observe_high(0, t, 0);
    on.observe_high(1_000, t, 0);
    for s in 1..=(DUMP_BATCH as u64 + 5) {
        on.resolve(s, s % 3 == 0, at(t, s * 10), 0);
    }
    let lines = on.take_dump_lines(false);
    assert_eq!(lines.len(), 1, "one FULL batch, tail withheld: {lines:?}");
    assert!(lines[0].starts_with(&format!("[SUCCDUMP] n={DUMP_BATCH} d=")));
    assert_eq!(lines[0].matches(';').count(), DUMP_BATCH - 1);
    assert!(lines[0].contains("o,10;"), "records are <tag>,<us>");
    let tail = on.take_dump_lines(true);
    assert_eq!(tail.len(), 1, "flush emits the partial tail: {tail:?}");
    assert!(tail[0].starts_with("[SUCCDUMP] n=5 d="));
    assert!(on.take_dump_lines(true).is_empty(), "nothing left after a flush");

    // The cap binds once, loudly, and stops recording raw records — while
    // the histograms keep counting, so a capped dump never truncates the
    // quantile line it rides beside.
    let mut cap = SuccGauge::new(false, true, 4);
    cap.observe_high(0, t, 0);
    cap.observe_high(100, t, 0);
    for s in 1..=20u64 {
        cap.resolve(s, false, at(t, s), 0);
    }
    let out = cap.take_dump_lines(true);
    assert!(
        out.iter().any(|l| l == "[SUCCDUMP-CAP] dumped=4"),
        "the cap announces itself exactly once: {out:?}"
    );
    let again = cap.take_dump_lines(true);
    assert!(!again.iter().any(|l| l.contains("SUCCDUMP-CAP")), "once, not twice");
    assert_eq!(
        cap.hist(HoleOutcome::Original).n(),
        20,
        "the CAP is on the dump, not on the measurement"
    );
}

// ── Observation only ────────────────────────────────────────────────

/// This gauge is read-only, and that is a property of the source, not of
/// a comment. Nothing in the engine may branch on
/// anything it computes, so it holds no engine handle and no site outside
/// this module and the receiver's feed/readout may name its readers.
#[test]
fn succ_is_observation_only() {
    let src = include_str!("../succ.rs");
    // Spelled in halves so this test's own source is not the match it is
    // looking for — the scraper reads the whole file, itself included.
    for forbidden in [
        concat!("Sched", "uler"),
        concat!("FecRate", "Controller"),
        concat!("Quic", "Transport"),
        concat!("Control", "Message"),
    ] {
        assert!(
            !src.contains(forbidden),
            "`{forbidden}` in an observation-only gauge — it has acquired \
             an engine handle and can no longer be read as read-only"
        );
    }
    // The engine-wide half of the same claim: the receiver may feed and
    // print this gauge and may not test it. `crossing_us`, `orig_frac` and
    // `quantile` are the three readers a law could plausibly be built on,
    // so they are the three whose call sites are pinned to `line()`.
    let recv = include_str!("../receiver.rs");
    for reader in ["crossing_us", "orig_frac", ".quantile("] {
        assert!(
            !recv.contains(reader),
            "receiver.rs calls `{reader}` — the successor gauge's readings \
             have reached a decision site. NO LAW MAY READ THIS GAUGE; the \
             derivation it feeds is FORMULA-FIRST, in the paper, and not a \
             wire from an instrument to a branch."
        );
    }
}
// ── The same/cross exposure split ───────────────────────────────────

/// The split is disjoint, closes against `res`, and is structurally zero
/// on a single path: at a one-path cell no hole can be closed by an
/// arrival on another path, so `xp_n = 0` is a property of the wire and
/// not a property of the sample.
#[test]
fn the_same_cross_exposure_split_is_disjoint_closes_and_is_zero_on_one_path() {
    let t = Instant::now();

    // Single path: every arrival on path 0.
    let mut one = SuccGauge::new(false, false, 0);
    one.observe_high(0, t, 0);
    one.observe_high(4, t, 0); // exposes 1, 2, 3
    one.resolve(1, false, at(t, 500), 0);
    one.resolve(2, true, at(t, 1_500), 0);
    one.resolve(3, false, at(t, 2_500), 0);
    assert_eq!(one.xp_n(), 0, "a one-path flow cannot expose a cross-path hole");
    assert_eq!(one.sp_n(), 3, "every resolution is same-path there");
    assert_eq!(
        one.sp_n() + one.xp_n(),
        one.hist(HoleOutcome::Original).n() + one.hist(HoleOutcome::Repair).n(),
        "the split must close against res"
    );
    assert_eq!(one.xp_frac(), Some(0.0), "an at-zero fraction is 0.0, not `-`");
    assert!(one.line().contains("xp_n=0"), "{}", one.line());

    // Two paths: a hole exposed by a path-1 arrival and closed by a
    // path-0 one is cross-path; closed by a path-1 one is same-path.
    let mut two = SuccGauge::new(false, false, 0);
    two.observe_high(0, t, 0);
    two.observe_high(3, t, 1); // path 1 exposes 1 and 2
    two.resolve(1, false, at(t, 400), 0); // cross
    two.resolve(2, false, at(t, 800), 1); // same
    assert_eq!((two.sp_n(), two.xp_n()), (1, 1));
    assert_eq!(two.xp_frac(), Some(0.5));
    let l = two.line();
    assert!(l.contains("sp_n=1 xp_n=1 xp_frac=0.5000"), "{l}");

    // Nothing resolved ⇒ `-`, never 0.
    let empty = SuccGauge::new(false, false, 0);
    assert_eq!(empty.xp_frac(), None);
    assert!(empty.line().contains("xp_frac=-"), "{}", empty.line());
}

