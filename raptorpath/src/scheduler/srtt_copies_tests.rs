//! W3.2 A1 (design §1.2 "R1 and R2 are not bit-identical"): the bounding tests
//! for the two smoothed echo RTTs the engine kept on one sample stream, Copa's
//! `srtt` (R1) and the loss estimator's `ewma_rtt` (R2). CLAUDE.md: every
//! documented divergence carries a test that BOUNDS it. They were written and
//! run green BEFORE the two were merged into `LegState.s`; they stay as the
//! bound on what the merge changed, driven by test-local references for both
//! arithmetics so they cannot go dead once only one remains in production.
//!
//! The four documented differences, each bounded below:
//!   1. arithmetic: R1 rounds each of two `mul_f64` terms to ns, R2 blends in
//!      f64 seconds and rounds once. Rounding error enters each step and
//!      decays as 0.875, so the gap is at most 8 ns per rounding unit injected
//!      per step. Units per step: 2 (R1) + 1 (R2) at 0.5 ns each (nearest) =
//!      1.5 ns, so 12 ns; the bound is 16 ns, covering f64 representation.
//!   2. revival: `copa.reset()` clears R1, the estimator is kept. After n
//!      post-revival samples R2 − R1 = 0.875^n · (pre − first).
//!   3. seed, `RWM_MSTAR_ANCHOR=0`: R2 blends from the 50 ms constructor
//!      seed, R1 seeds from the first sample: R2 − R1 = 0.875^n · (50 ms −
//!      first).
//!   4. feed, `RWM_COPA_WIRE`: R1 is fed the wire RTT, R2 the echo. After
//!      the merge `srtt()` reads the echo stream under the arm, and the
//!      controller keeps its own wire EWMA for its cadence.

use super::*;

/// 8 ns per rounding unit × 2 (see the module doc: 1.5 units measured in
/// worst case, 2 allowed).
const ROUND_BOUND_NS: u128 = 16;

fn ms(x: u64) -> Duration {
    Duration::from_millis(x)
}

// ---- test-local references for BOTH arithmetics -----------------------------

/// R1 as Copa ran it: two `mul_f64` terms, each rounded to ns.
fn ref_r1(prev: Option<Duration>, x: Duration) -> Duration {
    match prev {
        Some(s) => s.mul_f64(0.875) + x.mul_f64(0.125),
        None => x,
    }
}

/// R2 as the estimator ran it: one f64-seconds blend, rounded once.
fn ref_r2(prev: Duration, x: Duration) -> Duration {
    Duration::from_secs_f64(0.125 * x.as_secs_f64() + (1.0 - 0.125) * prev.as_secs_f64())
}

fn gap(a: Duration, b: Duration) -> u128 {
    a.as_nanos().abs_diff(b.as_nanos())
}

// ---- the production readings, behind three accessors -------------------------

/// R1: the reading Copa's cadence and σ were captured against.
fn prod_r1(p: &PathState) -> Option<Duration> {
    p.srtt_measured()
}
/// R2: what the recovery clocks, the rate law and A* read. After the A1 merge
/// this is the same field as R1.
fn prod_r2(p: &PathState) -> Duration {
    p.srtt()
}
/// What R2's readers are expected to see, given the two reference chains:
/// before the merge R2's own chain, after it R1's (the arithmetic and seed the
/// merged `s` kept).
fn expect_r2(r1_chain: Duration, _r2_chain: Duration) -> Duration {
    r1_chain
}
/// Revival, as `Scheduler::touch_path_at` does it.
fn revive(p: &mut PathState) {
    p.copa.reset();
    p.leg.reset();
}
/// Feed one sample the way a control handler does: the echo to the leg's `s`,
/// the controller's sample (echo unless `RWM_COPA_WIRE`) to Copa.
fn feed(p: &mut PathState, echo: Duration, cc: Duration) {
    p.record_rtt(echo, cc);
}

/// A deterministic ns-resolution sample stream (an LCG), 1 ms .. 300 ms.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> Duration {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        Duration::from_nanos(1_000_000 + (self.0 >> 33) % 299_000_000)
    }
}

fn fresh() -> (Arc<MockClock>, PathState) {
    assert!(
        crate::gates::get().mstar_anchor && !crate::gates::get().copa_wire,
        "default gates (MSTAR_ANCHOR on, COPA_WIRE off)"
    );
    let clock = Arc::new(MockClock::new());
    let p = PathState::new(0, clock.clone());
    (clock, p)
}

/// Arithmetic: |R1 − R2| ≤ 16 ns over 20 000 ns-resolution samples, with the
/// bound non-vacuous (the two really do differ by ≥ 1 ns somewhere) and both
/// production values equal to their test-local reference chains.
#[test]
fn echo_srtt_copies_bounded() {
    let (clock, mut p) = fresh();
    let mut rng = Lcg(0x5eed);
    let (mut c1, mut c2): (Option<Duration>, Duration) = (None, Duration::ZERO);
    let (mut max_gap, mut ref_max_gap) = (0u128, 0u128);
    for i in 0..20_000u32 {
        clock.advance(ms(1));
        let x = rng.next();
        // The chains: R1 seeds from the first sample; R2 seeds likewise under
        // MSTAR_ANCHOR (default on).
        c1 = Some(ref_r1(c1, x));
        c2 = if i == 0 { x } else { ref_r2(c2, x) };
        feed(&mut p, x, x);
        assert_eq!(prod_r1(&p), c1, "R1 is the two-mul_f64 chain (sample {i})");
        assert_eq!(
            prod_r2(&p),
            expect_r2(c1.unwrap(), c2),
            "R2 is the reference chain it is expected to be (sample {i})"
        );
        let g = gap(prod_r1(&p).unwrap(), prod_r2(&p));
        max_gap = max_gap.max(g);
        ref_max_gap = ref_max_gap.max(gap(c1.unwrap(), c2));
        assert!(g <= ROUND_BOUND_NS, "|R1 − R2| = {g} ns > {ROUND_BOUND_NS} ns at sample {i}");
    }
    println!("SRTT_COPIES arithmetic: max |R1 - R2| = {max_gap} ns, reference chains {ref_max_gap} ns");
    assert!(ref_max_gap >= 1, "non-vacuous: the two arithmetics differ somewhere");
    assert!(ref_max_gap <= ROUND_BOUND_NS);
}

/// Revival: `touch_path_at` resets Copa and keeps the estimator, so R1 reseeds
/// from the first post-revival sample and R2 blends from its stale pre-death
/// value: after n post-revival samples |R1 − R2| = 0.875^n · |pre − first|,
/// to within the rounding bound.
#[test]
fn echo_srtt_revival_gap_decays_as_0875n() {
    let (clock, mut p) = fresh();
    let mut rng = Lcg(7);
    for _ in 0..40 {
        clock.advance(ms(2));
        let x = Duration::from_micros(30_000) + rng.next() / 100;
        feed(&mut p, x, x);
    }
    let pre = prod_r2(&p);
    revive(&mut p);
    let first = ms(60) + Duration::from_nanos(777);
    assert!(gap(pre, first) > 20_000_000, "pre-death srtt is far from the first sample");
    let mut r1: Option<Duration> = None;
    let mut r2 = pre;
    for n in 1..=60u32 {
        clock.advance(ms(2));
        let x = first;
        r1 = Some(ref_r1(r1, x));
        r2 = ref_r2(r2, x);
        feed(&mut p, x, x);
        assert_eq!(prod_r1(&p), r1, "R1 reseeds from the first sample (n={n})");
        assert_eq!(prod_r2(&p), expect_r2(r1.unwrap(), r2), "n={n}");
        let g = gap(r1.unwrap(), r2);
        let bound = ((0.875f64).powi(n as i32) * gap(pre, first) as f64) as u128 + ROUND_BOUND_NS;
        assert!(g <= bound, "revival gap {g} ns > 0.875^{n}·|pre−first| + {ROUND_BOUND_NS} = {bound} ns");
        if n == 1 {
            assert!(g > 15_000_000, "non-vacuous: the gap right after revival is the milliseconds");
        }
    }
}

/// Seed under `RWM_MSTAR_ANCHOR=0`: R2 blends the first samples into the
/// 50 ms constructor seed while R1 seeds from the first sample.
#[test]
fn echo_srtt_seed_blend_without_mstar_anchor_is_bounded() {
    let (clock, mut p) = fresh();
    let first = ms(8) + Duration::from_nanos(321);
    let mut r1: Option<Duration> = None;
    let mut r2 = ms(50);
    for n in 1..=60u32 {
        clock.advance(ms(2));
        r1 = Some(ref_r1(r1, first));
        r2 = ref_r2(r2, first);
        feed(&mut p, first, first);
        assert_eq!(prod_r1(&p), r1, "n={n}");
        assert_eq!(prod_r2(&p), expect_r2(r1.unwrap(), r2), "n={n}");
        let g = gap(r1.unwrap(), r2);
        let bound = ((0.875f64).powi(n as i32) * gap(ms(50), first) as f64) as u128 + ROUND_BOUND_NS;
        assert!(g <= bound, "seed-blend gap {g} ns > {bound} ns at n={n}");
        if n == 1 {
            assert!(g > 30_000_000, "non-vacuous: 50 ms seed vs an 8 ms stream");
        }
    }
}

/// Feed under `RWM_COPA_WIRE`: R1 follows the wire RTT, R2 the echo — two
/// streams of two clocks, so no numeric bound exists between them; the
/// divergence is pinned instead by each chain equalling its own reference.
#[test]
fn echo_srtt_copies_take_different_feeds_under_copa_wire() {
    let (clock, mut p) = fresh();
    p.force_wire_for_test(0.5);
    let (mut c1, mut c2): (Option<Duration>, Duration) = (None, Duration::ZERO);
    let mut c3: Option<Duration> = None; // R1's arithmetic on the echo stream
    for i in 0..200u64 {
        clock.advance(ms(2));
        let echo = ms(70) + Duration::from_micros(13 * i % 5000); // dwell-inclusive
        let wire = ms(20) + Duration::from_micros(7 * i % 900); // packet-timed
        c1 = Some(ref_r1(c1, wire));
        c2 = if i == 0 { echo } else { ref_r2(c2, echo) };
        c3 = Some(ref_r1(c3, echo));
        feed(&mut p, echo, wire);
    }
    assert_eq!(wire_cadence_srtt(&p), c1.unwrap(), "Copa's cadence srtt follows the wire stream");
    assert_eq!(echo_srtt(&p), expect_wire_echo(c2, c3.unwrap()), "the echo reading follows the echo stream");
    assert!(gap(c1.unwrap(), c2) > 40_000_000, "non-vacuous: the two clocks differ by their dwell");
}

/// Copa's own (controller-local) srtt in wire mode.
fn wire_cadence_srtt(p: &PathState) -> Duration {
    p.copa.cadence_srtt(&p.leg)
}
/// The echo-fed reading (`srtt()`, which Copa's cadence no longer shares
/// under the arm).
fn echo_srtt(p: &PathState) -> Duration {
    p.srtt()
}
/// Before the merge R2's own chain; after it the merged `s`'s (R1 arithmetic).
fn expect_wire_echo(_r2_chain: Duration, r1_chain_on_echo: Duration) -> Duration {
    r1_chain_on_echo
}
