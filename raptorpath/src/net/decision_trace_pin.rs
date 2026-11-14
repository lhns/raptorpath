//! Behaviour pin for the decision laws that the shared-core design
//! (docs/derivations/shared-core-design.md, steps 1.2-2.5) moves between
//! crates. The `hotpath_pin` pattern: a scripted input sequence through each
//! decision, a SHA-256 digest of every output, captured on the untouched
//! tree (step 0.3, before any move). A pure move must keep every digest.
//!
//! Covered (one digest each):
//!   span      `gen_pipe_depth` (net/mod.rs, D13), `shed_deadline_us` (D12's
//!             deadline seat), `delta_budget_b_of` (b(delta));
//!   recovery  D6-D8 except `mp_hole_ripe`: time threshold, packet threshold
//!             (SeqRing evidence + fast-lost), age gate, pooled clocks,
//!             cooldown, tail sweep, hole refresh, stall threshold;
//!   shed      D11 `shed_allowed`, `residual_loss_after_fec`;
//!   taper     D21 `TaperFunction`, `TaperBudget::accrue`;
//!   store     D1/D2 `pooled_store_cap` chain, `codel_setpoint_q`,
//!             `pool_value_multiplier`, `delta_budget_b(hint)`;
//!   snapver   D5 `SnapOrder`, `snapver_*`, `judged_gaps`, `take_gap_batch`.
//!
//! Not driveable without I/O today (listed, not pinned): the inline A*/Delta
//! seat in `emit_source.rs` (inside the emit block, needs the scheduler,
//! encoder and transport); D9 per-seq verdicts and D10 margin (inside
//! `serve_gaps`); `mp_hole_ripe` (stays in the engine by design).
//!
//! Captured on main 766d9906 (untouched tree).
//!
//! Determinism: no clock, no RNG, no HashMap iteration (maps are read by
//! sorted key). f64 results of pure IEEE arithmetic (+ - * / min max ceil
//! clamp) are digested by `to_bits`. Results that pass through `powf`, `log10`
//! or `exp` use the platform libm, whose last ulp may differ between
//! Windows and Linux, so they are digested at 9 significant decimals
//! (`fq`) instead; a pure move keeps those bit-identical on one platform
//! anyway.
//!
//! A deliberate law change re-captures the digests
//! (`DECISION_TRACE_PIN_PRINT=1 cargo test decision_trace_pin -- --nocapture`
//! prints the live values) and must say so.

use sha2::{Digest, Sha256};
use std::time::Duration;

use super::*;

struct Tr(Sha256);

impl Tr {
    fn new(tag: &str) -> Self {
        let mut h = Sha256::new();
        h.update(tag.as_bytes());
        Tr(h)
    }
    fn u(&mut self, x: u64) {
        self.0.update(x.to_le_bytes());
    }
    fn b(&mut self, x: bool) {
        self.0.update([x as u8]);
    }
    /// Exact: pure IEEE arithmetic only.
    fn f(&mut self, x: f64) {
        self.0.update(x.to_bits().to_le_bytes());
    }
    /// Quantised: the value passed through platform libm.
    fn fq(&mut self, x: f64) {
        self.0.update(format!("{x:.9e}").as_bytes());
    }
    fn d(&mut self, x: Duration) {
        self.0.update(x.as_nanos().to_le_bytes());
    }
    fn s(&mut self, x: &str) {
        self.0.update(x.as_bytes());
        self.0.update([0xfe]);
    }
    fn sep(&mut self) {
        self.0.update([0xff]);
    }
    fn finish(self) -> String {
        hex::encode(self.0.finalize())
    }
}

fn check(name: &str, got: String, want: &str) {
    if std::env::var_os("DECISION_TRACE_PIN_PRINT").is_some() {
        println!("DECISION_TRACE_PIN {name} = {got}");
    }
    assert_eq!(got, want, "{name}: a decision law's output changed");
}

const F_GRID: [f64; 12] = [
    f64::NEG_INFINITY,
    -1.0,
    -0.0,
    0.0,
    1e-12,
    0.005,
    0.5,
    1.0,
    2.0,
    50.0,
    1e9,
    f64::INFINITY,
];
const NAN: f64 = f64::NAN;
const US: [u64; 9] = [0, 1, 999, 1_000, 10_000, 25_000, 50_000, 100_000, 1 << 40];

// ---- span forms at their engine seats ----------------------------------

fn span_digest() -> String {
    let mut t = Tr::new("span");
    // D13 generation pipeline depth: M* = ceil(rate*2*rtt/G)+1 clamped.
    // +inf is left out: `ceil() as usize + 1` overflows (panics in debug,
    // wraps to depth 2 in release), a latent profile-dependent edge reported
    // with step 0.3, not pinned.
    for &rate in F_GRID.iter().filter(|r| **r != f64::INFINITY).chain([NAN, 100.0, 12_345.678].iter()) {
        for &rtt in &[-1.0, 0.0, 0.001, 0.05, 0.25, 3.0, NAN] {
            for g in [0usize, 1, 16, 64, 1000] {
                t.u(gen_pipe_depth(rate, rtt, g) as u64);
            }
        }
    }
    t.sep();
    // D11/D12 deadline seat: D = min(b*rtprop, 2*rtprop) with b capped at 2.
    for &b in F_GRID.iter().chain([NAN, 0.75, 1.5].iter()) {
        for &rt in &US {
            t.u(shed_deadline_us(b, rt));
        }
    }
    t.sep();
    // b(delta): the horizon map the span seats read.
    for &p in F_GRID.iter().chain([NAN, 0.5e-2, 5.0e-3, 0.4, 0.6, 49.0, 51.0].iter()) {
        t.fq(delta_budget_b_of(p));
    }
    t.finish()
}

#[test]
fn pin_span_forms() {
    check(
        "span",
        span_digest(),
        "f502805bfb746561154d468920c9e3bf62a1fc182164d637d0a30a15b55665a3",
    );
}

// ---- D6-D8 recovery verdicts -------------------------------------------

fn recovery_digest() -> String {
    let mut t = Tr::new("recovery");
    // D6: time threshold with the floor-won cause, and the ripeness verdict.
    for &s in &US {
        for &e in &US {
            for &fl in &[0u64, 1_000, 25_000, 1 << 41] {
                let (thr, floor_won) = mp_time_threshold_split(s, e, fl);
                t.u(thr);
                t.b(floor_won);
            }
        }
    }
    t.sep();
    for &now in &US {
        for &flight in &[None, Some(0u64), Some(1), Some(now), Some(now / 2), Some(u64::MAX)] {
            for &thr in &US {
                t.b(time_threshold_ripe(now, flight, thr));
            }
        }
    }
    t.sep();
    // D7: packet threshold. Delivered-interval derivation from a SACK gap list.
    let gap_lists: [&[(u64, u64)]; 5] = [
        &[],
        &[(5, 5)],
        &[(3, 4), (9, 9), (12, 20)],
        &[(0, 0), (1, 1), (2, 2)],
        &[(10, 12), (12, 14), (30, 31)],
    ];
    for g in gap_lists {
        let iv = mp_delivered_intervals(g);
        t.u(iv.len() as u64);
        for (lo, hi) in iv {
            t.u(lo);
            t.u(hi);
        }
        t.sep();
    }
    // mp_fast_lost on sorted delivered vectors.
    let delivered: [&[u64]; 4] = [&[], &[7], &[3, 5, 8, 9, 10], &[1, 2, 3, 4, 5, 6]];
    for d in delivered {
        for s in 0..12u64 {
            t.b(mp_fast_lost(d, s));
        }
    }
    t.u(MP_PACKET_THRESHOLD as u64);
    t.sep();
    // Evidence ingest: SeqRing of seq -> path, three reports with growing
    // gap lists; the per-path vectors are read in sorted-key order.
    let mut ring: seq_ring::SeqRing<u32> = seq_ring::SeqRing::new();
    for q in 0..40u64 {
        ring.insert(q, (q % 3) as u32);
    }
    let mut delivered: std::collections::HashMap<u32, Vec<u64>> = std::collections::HashMap::new();
    let mut evid_max = 0u64;
    let reports: [&[(u64, u64)]; 4] = [&[(2, 3)], &[(2, 3), (8, 9), (15, 15)], &[(2, 3), (8, 9), (15, 15), (22, 30)], &[]];
    for gaps in reports {
        mp_ingest_evidence(gaps, &ring, &mut delivered, &mut evid_max);
        t.u(evid_max);
        let mut keys: Vec<u32> = delivered.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            t.u(k as u64);
            for &q in &delivered[&k] {
                t.u(q);
            }
            t.sep();
        }
        for seq in 0..40u64 {
            for orig in [None, Some(0u32), Some(1), Some(2), Some(9)] {
                t.b(mp_packet_threshold_fires(seq, orig, &delivered));
            }
        }
    }
    t.sep();
    // D8: age gate, pooled clocks, cooldown, tail sweep, hole refresh, stall.
    for &now in &US {
        for &sent in &US {
            for &srtt in &[0u64, 1, 2, 50_000] {
                t.b(legacy_age_ripe(now, sent, srtt));
            }
        }
    }
    t.u(pooled_recovery_srtt_us(&[]));
    t.u(pooled_recovery_srtt_us(&[7]));
    t.u(pooled_recovery_srtt_us(&[30_000, 80_000, 5_000]));
    for &s in &US {
        for &fl in &[0u64, 10_000, 1 << 41] {
            t.u(retx_cooldown_us(s, fl));
        }
        for &last in &US {
            for &cd in &[0u64, 10_000, 50_000] {
                t.b(cooldown_elapsed(s, last, cd));
            }
        }
        t.u(tail_sweep_timeout_us(s));
        t.u(stall_threshold_us(s));
    }
    t.d(hole_nack_refresh(None));
    for ms in [0u64, 1, 10, 12, 13, 25, 50, 51, 100, 5000] {
        t.d(hole_nack_refresh(Some(Duration::from_millis(ms))));
    }
    for c in [
        TIMER_GRANULARITY_US,
        NACK_RETX_COOLDOWN_FLOOR_US,
        TAIL_SWEEP_MIN_US,
        TAIL_SWEEP_MAX_US,
    ] {
        t.u(c);
    }
    t.d(HOLE_NACK_REFRESH_MIN);
    t.d(HOLE_NACK_REFRESH_MAX);
    t.finish()
}

#[test]
fn pin_recovery_verdicts() {
    check(
        "recovery",
        recovery_digest(),
        "c26e7112c87a95f0e7c8ccfc459bc29fa42a70a1ffba42dd68bed718ad2759c3",
    );
}

// ---- D11 shed ------------------------------------------------------------

fn shed_digest() -> String {
    let mut t = Tr::new("shed");
    for &age in &US {
        for &dl in &[0u64, 1, 999, 1_000, 50_000] {
            for &(shed, src) in &[(0u64, 0u64), (0, 1), (0, 100), (4, 100), (5, 100), (6, 100), (99, 100), (1000, 10)] {
                for &frac in &[-1.0, 0.0, 0.01, 0.05, 0.0500001, 0.5, 1.0, NAN] {
                    t.b(shed_allowed(age, dl, shed, src, frac));
                }
            }
        }
    }
    t.sep();
    // residual loss after FEC: epsilon, r, window, burst variance.
    for &eps in &[-0.1, 0.0, 1e-4, 0.01, 0.05, 0.2, 0.9, 1.0, 1.5, NAN] {
        for &r in &[0.0, 0.01, 0.1, 0.5, 1.0] {
            for &w in &[1.0, 8.0, 64.0, 256.0] {
                for &s2 in &[1.0, 2.5, 10.0] {
                    t.fq(crate::control::fec_rate::residual_loss_after_fec(eps, r, w, s2));
                }
            }
        }
    }
    t.finish()
}

#[test]
fn pin_shed_law() {
    check(
        "shed",
        shed_digest(),
        "b0af84978489e30ef75d926e946355e2bb452ade6963a41a59450b85d8b0d355",
    );
}

// ---- D21 taper -----------------------------------------------------------

fn taper_digest() -> String {
    use crate::control::{TaperBudget, TaperFunction};
    let mut t = Tr::new("taper");
    // Taper shapes: q = 0 (degenerate), small, default 0.5, and 1 (decay 0).
    let shapes: Vec<TaperFunction> = [0.0, 0.01, 0.2, 0.5, 1.0]
        .iter()
        .map(|&q| TaperFunction { amplitude: 0.1 * q, decay: 1.0 - q, total_rate: 0.1, q })
        .collect();
    for sh in &shapes {
        for off in [0.0, 1.0, 5.0, 31.0] {
            t.fq(sh.density(off));
        }
    }
    t.sep();
    // Scripted accrual: 3 budgets x (rate, spare, span) schedules x 3 shapes.
    for (si, sh) in shapes.iter().enumerate().filter(|(i, _)| [1, 3, 4].contains(i)) {
        for &(rate, spare, span) in &[
            (0.0, 1.0, 64usize),
            (0.05, 1.0, 64),
            (0.05, 0.01, 64),
            (0.3, 1.0, 8),
            (0.3, 0.0, 8),
            (1.5, 5.0, 1),
            (0.2, -1.0, 0),
            (-0.1, 1.0, 16),
        ] {
            let mut b = TaperBudget::new();
            t.u(si as u64);
            for step in 0..96u64 {
                let g = b.accrue(rate, step, sh, span, spare);
                t.fq(g);
                t.fq(b.owed());
            }
            t.sep();
        }
    }
    t.finish()
}

#[test]
fn pin_taper_budget() {
    check(
        "taper",
        taper_digest(),
        "1eff0e00f3df10d9da0b2c5787b3aca79834cfbcd53280f043bbf78ef46a2798",
    );
}

// ---- D1/D2 store-cap chain -------------------------------------------------

fn store_digest() -> String {
    let mut t = Tr::new("store");
    // The pin is on the shipped table: RWM_DELTA must not be set.
    assert!(
        crate::gates::delta_override().is_none(),
        "decision_trace_pin is captured with RWM_DELTA unset"
    );
    let hints = [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk];
    for h in hints {
        t.fq(delta_budget_b(h));
        t.f(delta_price(h)); // table lookup: COPA_DELTA / scale, no libm
        t.fq(codel_setpoint_q(delta_budget_b(h)));
    }
    t.sep();
    let b_lo = delta_budget_b(ProtocolHint::Realtime);
    let b_hi = delta_budget_b(ProtocolHint::Bulk);
    let bs = [0.0, 0.25, b_lo, 0.6, 0.75, 1.0, 1.3, 1.5, b_hi, 3.0, f64::INFINITY, NAN];
    for &b in &bs {
        t.fq(codel_setpoint_q(b));
        for dc in [false, true] {
            let m = pool_value_multiplier(dc, b, 2.0);
            if dc { t.fq(m) } else { t.f(m) }
        }
    }
    t.sep();
    for &sum_cap in &[false, true] {
        for &delta_cap in &[false, true] {
            for &b in &bs {
                for n in [0usize, 1, 2, 3, 8] {
                    for &ps in &[0.0, -1.0, 3.5, 100.0, 4096.0, 1e7] {
                        let u = pooled_store_cap_unclamped(sum_cap, delta_cap, b, n, ps, 2.0);
                        if delta_cap { t.fq(u) } else { t.f(u) }
                        for &on in &[false, true] {
                            // (floor, pool): the engine's N*pool ceiling and floor.
                            for &(floor, pool) in &[(0usize, 0usize), (64, 512), (64, 2048), (512, 2048)] {
                                match pooled_store_cap(on, sum_cap, delta_cap, b, n, ps, 2.0, floor, pool) {
                                    Some(v) => {
                                        t.b(true);
                                        t.u(v as u64);
                                    }
                                    None => t.b(false),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    t.finish()
}

#[test]
fn pin_store_cap_chain() {
    check(
        "store",
        store_digest(),
        "60ddf6275c2510002da518051b9ee06a5fcbfbd5b9a451d3dd160b5f3af015e0",
    );
}

// ---- D5 snapshot order ----------------------------------------------------

fn snap_digest() -> String {
    let mut t = Tr::new("snapver");
    let r = |n: u64, a: u32| Some(AboveReport { next_expected: n, received_above: a });
    // A scripted report stream: in order, stale, equal, regressing frontier,
    // regressing count, and no-report (None).
    let stream = [r(0, 0), r(5, 2), r(5, 1), None, r(5, 2), r(4, 9), r(9, 0), r(9, 3), None, r(8, 100), r(9, 3), r(10, 0)];
    let mut order = SnapOrder::default();
    for v in stream {
        t.b(snapver_judges(&order, v));
        t.b(order.is_stale(v));
        let gaps = [(1u64, 2u64), (7, 7)];
        t.u(judged_gaps(&order, v, &gaps).len() as u64);
        if let Some(v) = v {
            order.fold(v);
        }
        let n = order.newest();
        t.u(n.next_expected);
        t.u(n.received_above as u64);
    }
    t.sep();
    for kept in stream {
        for cand in stream {
            t.b(snapver_coalesce_keeps(kept, cand));
        }
    }
    t.sep();
    // take_gap_batch through a real mpsc channel (no runtime needed for
    // try_recv): pending first, then queue with and without coalescing.
    for coalesce in [false, true] {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<GapBatch>(32);
        for (i, v) in stream.iter().enumerate() {
            tx.try_send((FireCause::GapData, i as u32, vec![(i as u64, i as u64 + 1)], *v)).unwrap();
        }
        let mut order = SnapOrder::default();
        let mut pending: Option<GapBatch> = Some((FireCause::Timer, 99, vec![(0, 0)], r(2, 2)));
        loop {
            match take_gap_batch(&mut pending, &mut rx, coalesce, &mut order) {
                Some(((cause, leg, gaps, v), coalesced)) => {
                    t.s(&format!("{cause:?}"));
                    t.u(leg as u64);
                    t.u(gaps.len() as u64);
                    t.u(gaps[0].0);
                    t.u(v.map_or(u64::MAX, |v| v.next_expected));
                    t.u(coalesced);
                    t.u(order.newest().next_expected);
                    t.u(order.newest().received_above as u64);
                    t.sep();
                }
                None => break,
            }
        }
    }
    t.finish()
}

#[test]
fn pin_snapshot_order() {
    check(
        "snapver",
        snap_digest(),
        "66b8184de91857541e5d110c9637e1602df72d45ac42f7605794b515147ed4a8",
    );
}

/// The digests are a pure function of the code: a second evaluation in the
/// same process agrees (no hidden state, no clock, no iteration order).
#[test]
fn digests_are_reproducible_within_a_process() {
    assert_eq!(span_digest(), span_digest());
    assert_eq!(recovery_digest(), recovery_digest());
    assert_eq!(shed_digest(), shed_digest());
    assert_eq!(taper_digest(), taper_digest());
    assert_eq!(store_digest(), store_digest());
    assert_eq!(snap_digest(), snap_digest());
}
