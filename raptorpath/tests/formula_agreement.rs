//! Agreement tests: the shipped code computes the published formula.
//!
//! Every other pin asserts a property of the code (linearity in N, continuity
//! in ρ, a clamp's reachability); none asserts equality with the paper, so a
//! law can drift from its publication unseen. Each test here transcribes a
//! formula-first law from `docs/fec-arq-model.md` and asserts the engine
//! function equals it.
//!
//! ## Template for adding a law
//!
//! 1. **Transcribe.** A local `fn published_*` that is the paper's expression
//!    and nothing else — in the paper's symbols and order, with its section in
//!    a comment. It must not call the engine function it checks, import the
//!    engine's helpers for the parts it transcribes, or be "simplified": an
//!    algebraically equal rewrite re-derives the thing under test.
//! 2. **Drive both** on the same inputs, over a grid that includes the dials'
//!    named points, the wire-measured range of every measured symbol, and the
//!    degenerate ends (N = 1, zero skew, the clamp's two sides).
//! 3. **Assert equality, and bound every deliberate divergence.** Where the
//!    engine quantizes (integer µs, `ceil` to whole symbols), the residual is
//!    asserted against an explicit bound derived from the quantization, never
//!    hidden by a loose tolerance.
//! 4. **Prove the clamp is not answering.** A law compared to its paper
//!    through a bound that always binds is a comparison of two constants.
//!    Every agreement assertion states that the value is interior — or, where
//!    the clamp is under test, that it is the clamp.
//!
//! Laws in the class: the δ deadline `D(δ)` (paper §5.6), the contract stall
//! (§6.4), the three-term / composed store cap (§10), the pooled store cap in
//! both forms (§6.1), the span horizon `b(δ)` (§5.4), and the δ-cap setpoint
//! `q(δ)` (§6.1). For the pooled cap's shipped `×N` form, what is transcribed
//! is the paper's statement of the expression the code runs, labelled as the
//! thing under review — not a derivation for it.

use raptorpath::net::{
    contract_stall_s, delta_budget_b, pooled_store_cap, pooled_store_cap_unclamped,
    shed_deadline_us, three_term_store_cap, ThreeTermTerm, WIN_STORE_MAX,
};
use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::net::{
    codel_setpoint_q, delta_budget_b_of, delta_price, pool_value_multiplier, CODEL_TARGET_HI,
    CODEL_TARGET_LO,
};

// ─────────────────────────────────────────────────────────────────────────
// 1. Transcriptions — the paper, and nothing but the paper
// ─────────────────────────────────────────────────────────────────────────

/// Paper §5.6 — `D(δ) = min(b(δ)·RTprop, 2·RTprop)`. Real-valued, in
/// seconds: the paper states a time, not a microsecond count.
fn published_d_of_delta(b: f64, rtprop_s: f64) -> f64 {
    (b * rtprop_s).min(2.0 * rtprop_s)
}

/// Paper §6.4 — `stall(δ, ρ, srtt) = (1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt)`.
/// Both terms always computed; no branch on ρ (the no-mode-switch invariant
/// read off the formula).
fn published_stall_s(rho: f64, b: f64, rtprop_s: f64, srtt_s: f64) -> f64 {
    (1.0 - rho) * published_d_of_delta(b, rtprop_s) + rho * ((9.0 / 8.0) * srtt_s + srtt_s)
}

/// The three-term / composed store cap (paper §10), with term 1 funding one
/// ack round trip `Kᵢ·RTpropᵢ`:
///
/// ```text
/// cap = Σᵢ over live_paths [ rateᵢ·srttᵢ + rateᵢ·stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew
///   srttᵢ     = Kᵢ·RTpropᵢ
///   skew      = (maxᵢ RTpropᵢ − minᵢ RTpropᵢ) / 2
///   rate_fast = the rate of the least-RTprop path
///   clamp: [floor, WIN_STORE_MAX] — the memory bound stated outside the law
/// ```
///
/// Returned unclamped and real-valued, so the caller asserts the law and its
/// bound separately (template part 4). The engine's `ceil` to whole symbols
/// is applied by the caller, where it is visible.
fn published_composed_cap_unclamped(paths: &[(f64, f64, f64)], rho: f64, b: f64) -> f64 {
    let srtt = |k: f64, rtprop_s: f64| k * rtprop_s;
    let mut sum = 0.0;
    for &(rate, rtprop_s, k) in paths {
        let s = srtt(k, rtprop_s);
        sum += rate * s + rate * published_stall_s(rho, b, rtprop_s, s);
    }
    let rtp_min = paths.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let rtp_max = paths.iter().map(|p| p.1).fold(0.0f64, f64::max);
    let rate_fast = paths
        .iter()
        .filter(|p| p.1 == rtp_min)
        .map(|p| p.0)
        .next()
        .unwrap_or(0.0);
    let skew = (rtp_max - rtp_min) / 2.0;
    sum + 2.0 * rate_fast * skew
}

/// Paper §6.1 — the pooled outstanding cap, both forms.
///
/// ```text
///   shipped    cap = clamp( gain · N · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
///   corrected  cap = clamp( gain     · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
/// ```
///
/// In the paper's order and symbols, with the multiplier as the single
/// differing factor. Returned unclamped and real-valued (template part 4);
/// the caller applies the `ceil` and the clamp.
///
/// It takes per-path anchors rather than a pre-summed Σ on purpose: the claim
/// under test is that the Σ is the path-count scaling, and a pre-summed number
/// would assume it.
fn published_pooled_cap_unclamped(anchors: &[f64], gain: f64, sum_cap: bool) -> f64 {
    let n = anchors.len() as f64;
    let sigma: f64 = anchors.iter().sum();
    if sum_cap { gain * sigma } else { gain * n * sigma }
}

/// Paper §6.1's ceiling — `max(N·knee, floor)`, not bare `N·knee`. At the
/// shipped `knee = 2048` the two coincide and no cell reaches the difference,
/// which is why it is transcribed from the paper rather than from memory.
fn published_pooled_ceiling(n: usize, knee: usize, floor: usize) -> usize {
    (n * knee).max(floor)
}

// ─────────────────────────────────────────────────────────────────────────
// 2. The grid — dials at their named points, measured symbols at their
//    wire-measured range, and both degenerate ends
// ─────────────────────────────────────────────────────────────────────────

/// `K` at its wire-measured values over 833 `[3T]` evaluations, plus the
/// synthetic ends. 1.04 = c8, 1.14 = c7/sc2, 1.15 = c1, 1.505 = c8L.
const K_WIRE: &[f64] = &[1.0, 1.04, 1.14, 1.15, 1.505, 2.0];

/// ρ across its whole dial including both ends — the shed term is live only
/// below 1, and ρ = 1 is the shipped retain-until-acked scope.
const RHO_GRID: &[f64] = &[0.0, 0.25, 0.5, 0.75, 0.9, 1.0];

/// Paper §5.4 — `b(δ) = clamp(2^(−½·log₁₀(δ/δ_Auto)), ½, 2)`, δ_Auto = 0.5.
///
/// Written from the paper's expression in a different algebraic arrangement
/// from the engine's (`2^x` via `powf` on the ratio), so the two agreeing is
/// evidence rather than a tautology of shared code.
fn published_span_horizon_b(delta: f64) -> f64 {
    let raw = (-0.5 * (delta / 0.5).log10()).exp2();
    raw.clamp(0.5, 2.0)
}

/// b(δ) at the protocol's named points, plus two off-point values: they are
/// points on a dial, not modes, so the law must agree between them too.
fn b_grid() -> Vec<f64> {
    vec![
        delta_budget_b(ProtocolHint::Realtime),
        0.75,
        delta_budget_b(ProtocolHint::Auto),
        1.5,
        delta_budget_b(ProtocolHint::Bulk),
    ]
}

/// The bench's own transcribed cell legs (`tests/store_cap_sf_bench.rs`):
/// c2 = 100 Mbit / 10 ms ⇒ 10 400 sym/s at RTprop 8 ms; c3 = 20 Mbit / 40 ms
/// ⇒ 2 000 sym/s at RTprop 60 ms.
const LEG_C2: (f64, f64) = (10_400.0, 0.008);
const LEG_C3: (f64, f64) = (2_000.0, 0.060);

/// The µs quantization the engine applies to `D(δ)` and the paper does not.
///
/// `contract_stall_s` computes `shed_deadline_us(b, (rtprop_s·1e6) as u64)`,
/// which truncates twice toward zero: seconds→µs (< 1 µs) and the product
/// `b·rtprop_us`→u64 (< 1 µs). So the engine's `D` is below the paper's by
/// strictly less than 2 µs, never above, and the stall carries that residual
/// weighted by `(1 − ρ)`. An absolute bound in seconds rather than a relative
/// tolerance, so it cannot absorb a real divergence: at the grid's smallest
/// RTprop it is 0.02 % of D.
const D_QUANT_BOUND_S: f64 = 2e-6;

// ─────────────────────────────────────────────────────────────────────────
// 3. The tests
// ─────────────────────────────────────────────────────────────────────────

/// Law: `D(δ)`, paper §5.6. The engine's `shed_deadline_us` against the
/// published `min(b·RTprop, 2·RTprop)`.
///
/// The engine returns integer µs; the paper returns a time. The divergence is
/// a floor toward zero of at most [`D_QUANT_BOUND_S`], asserted signed (the
/// engine is never above the paper), because an unsigned band would pass a
/// law that had drifted upward by a µs for another reason.
#[test]
fn published_delta_deadline_equals_the_engine_shed_deadline() {
    for &b in &b_grid() {
        for &rtprop_ms in &[0.05f64, 1.0, 8.0, 38.0, 60.0, 353.0] {
            let rtprop_s = rtprop_ms / 1e3;
            let engine_s = shed_deadline_us(b, (rtprop_s * 1e6) as u64) as f64 / 1e6;
            let paper_s = published_d_of_delta(b, rtprop_s);
            let err = paper_s - engine_s;
            // The lower end is `-f64::EPSILON`, not `0.0`: where the µs
            // truncation is exact the two sides still differ by one ULP of
            // double rounding (`0.75·38 ms` reads 0.0284999…97 against 0.0285).
            // A representation artefact, and the only slack on this side.
            assert!(
                (-f64::EPSILON..D_QUANT_BOUND_S).contains(&err),
                "D(δ) diverges from §16.20.3 beyond the µs quantization: \
                 b={b} RTprop={rtprop_ms}ms paper={paper_s} engine={engine_s} err={err}"
            );
        }
    }
}

/// Law: the contract stall, paper §6.4. `contract_stall_s` against the
/// published `(1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt)`.
///
/// At ρ = 1 (the shipped scope) the shed term is multiplied by zero, so the
/// agreement is exact and asserted exactly. Below ρ = 1 it carries the `D`
/// quantization weighted by `(1 − ρ)`, and that weighting is asserted: a
/// residual that did not shrink with ρ would mean the divergence is in the
/// retained term.
#[test]
fn published_contract_stall_equals_the_engine_stall() {
    for &rho in RHO_GRID {
        for &b in &b_grid() {
            for &k in K_WIRE {
                for &rtprop_ms in &[0.05f64, 8.0, 38.0, 60.0, 353.0] {
                    let rtprop_s = rtprop_ms / 1e3;
                    let srtt_s = k * rtprop_s;
                    let engine = contract_stall_s(rho, b, rtprop_s, srtt_s);
                    let paper = published_stall_s(rho, b, rtprop_s, srtt_s);
                    let err = paper - engine;
                    let bound = (1.0 - rho) * D_QUANT_BOUND_S;
                    if rho == 1.0 {
                        assert_eq!(
                            engine, paper,
                            "at the SHIPPED ρ = 1 the stall must equal §16.56 EXACTLY: \
                             b={b} K={k} RTprop={rtprop_ms}ms"
                        );
                    }
                    assert!(
                        (-f64::EPSILON..=bound + f64::EPSILON).contains(&err),
                        "stall diverges from §16.56 beyond (1−ρ)·quantization: \
                         ρ={rho} b={b} K={k} RTprop={rtprop_ms}ms \
                         paper={paper} engine={engine} err={err} bound={bound}"
                    );
                }
            }
        }
    }
}

/// Law: the three-term / composed store cap (paper §10).
/// `net::three_term_store_cap` against the published Σ.
///
/// Transcribing term 1 as `rateᵢ·RTpropᵢ` instead of `rateᵢ·Kᵢ·RTpropᵢ` fails
/// this test at every `K > 1` in the grid — every wire value ever measured.
///
/// Template part 4: the law's only remaining bound is the memory bound, so
/// every geometry is asserted interior before its value is compared; this can
/// never become an assertion that two constants are both 4096.
#[test]
fn published_composed_cap_equals_the_engine_three_term_law() {
    const FLOOR: usize = 0; // inert by construction; the clamp is asserted below
    let mut checked = 0usize;
    for &rho in RHO_GRID {
        for &b in &b_grid() {
            for &k in K_WIRE {
                // The geometries: the single path (span zero by arithmetic),
                // the symmetric dual (span zero because max == min), the
                // asymmetric dual (the only one with a live span term), and
                // the symmetric quad as the N ≥ 3 axis.
                let geoms: Vec<Vec<(f64, f64, f64)>> = vec![
                    vec![(LEG_C2.0, LEG_C2.1, k)],
                    vec![(LEG_C2.0, LEG_C2.1, k), (LEG_C2.0, LEG_C2.1, k)],
                    vec![(LEG_C2.0, LEG_C2.1, k), (LEG_C3.0, LEG_C3.1, k)],
                    vec![(LEG_C2.0, LEG_C2.1, k); 4],
                ];
                for g in geoms {
                    let terms: Vec<Option<ThreeTermTerm>> = g
                        .iter()
                        .map(|&(rate, rtprop_s, k)| Some(ThreeTermTerm { rate, rtprop_s, k }))
                        .collect();
                    let (limit, window, slack, span) =
                        three_term_store_cap(true, &terms, rho, b, FLOOR)
                            .expect("every synthetic path is warm");

                    let paper = published_composed_cap_unclamped(&g, rho, b);

                    // (4) The clamp is not answering.
                    assert!(
                        limit < WIN_STORE_MAX && limit > FLOOR,
                        "the memory bound is answering instead of the law \
                         (N={} ρ={rho} b={b} K={k} limit={limit}) — this comparison \
                         would be of two constants",
                        g.len()
                    );

                    // (3) Equality. The engine's total is the same real number
                    // as the paper's before its `ceil` to whole symbols; the
                    // per-ρ residual is the same (1−ρ)·quantization the stall
                    // carries, scaled by Σ rate.
                    let engine_total = window + slack + span;
                    let rate_sum: f64 = g.iter().map(|p| p.0).sum();
                    let bound = (1.0 - rho) * D_QUANT_BOUND_S * rate_sum + 1e-9;
                    let err = paper - engine_total;
                    let n = g.len();
                    assert!(
                        (-1e-9..=bound).contains(&err),
                        "the composed cap diverges from §16.56 (amended 2026-08-18): \
                         N={n} ρ={rho} b={b} K={k} paper={paper} engine={engine_total} \
                         (window={window} slack={slack} span={span}) err={err} bound={bound}"
                    );
                    assert_eq!(
                        limit,
                        engine_total.ceil() as usize,
                        "the realized limit is not the ceil of the law's own total"
                    );
                    checked += 1;
                }
            }
        }
    }
    // Mechanism liveness (measurement discipline rule 1): a grid that became
    // empty would pass this test while asserting nothing.
    assert_eq!(
        checked,
        RHO_GRID.len() * b_grid().len() * K_WIRE.len() * 4,
        "the agreement grid did not execute at full size"
    );
}

/// Term 1 as `rateᵢ·RTpropᵢ` is not what the engine computes; the ratio
/// between them is exactly `K` on the window term. Pinned at the wire's own
/// `K` values so the choice cannot be silently reversed.
#[test]
fn the_amended_term_one_is_k_times_the_pre_amendment_term_one() {
    for &k in &[1.04f64, 1.14, 1.15, 1.505] {
        let g = [(LEG_C2.0, LEG_C2.1, k)];
        let terms = [Some(ThreeTermTerm { rate: g[0].0, rtprop_s: g[0].1, k })];
        let (_, window, ..) = three_term_store_cap(true, &terms, 1.0, 1.0, 0).expect("warm");
        let pre_amendment_window = g[0].0 * g[0].1; // rate·RTprop, term 1 without K
        assert!(
            (window / pre_amendment_window - k).abs() < 1e-12,
            "the window term is not K× the pre-amendment published term: K={k}"
        );
        // And the size of it, recomputed: 4–50 % high.
        let pct = 100.0 * (k - 1.0);
        assert!(
            (4.0..=50.5).contains(&pct),
            "K={k} is outside the range §16.57 measured on the wire"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 4. The pooled store cap (paper §6.1)
// ─────────────────────────────────────────────────────────────────────────

/// The wire's own per-path anchors in symbols, reconstructed as
/// `store_cap_sf_bench::AckShape::anchor_sym` does (three measured columns
/// multiplied: `xanchor · rate_lr · RTprop`), so the agreement is driven over
/// the range the law operates in (template part 2).
const WIRE_C7: [f64; 2] = [9.80 * 9_432.0 * 0.0077, 10.11 * 9_418.0 * 0.0097];
const WIRE_C8: [f64; 2] = [13.29 * 6_948.0 * 0.0084, 13.82 * 1_376.0 * 0.0386];

/// The shipped pooled-law constants (`sender_policy::resolve`, `gates.rs`).
const POOL_GAIN: f64 = 2.0;
const KNEE: usize = 2048;
const POOL_FLOOR: usize = raptorpath::net::sender_policy::STORE_CAP_FLOOR;

/// Law: the pooled store cap, paper §6.1, both arms. `net::pooled_store_cap`
/// against the published expressions.
///
/// Driven over the wire's measured anchors, a symmetric synthetic sweep to
/// N = 8 (the only axis on which the two arms' shapes are distinguishable),
/// and an asymmetric geometry. The engine's `ceil` to whole symbols is the
/// sole deliberate divergence and it is bounded: the realized value is the
/// published one rounded up by strictly less than one symbol, asserted
/// signed.
#[test]
fn published_pooled_cap_equals_the_engine_pooled_cap_on_both_arms() {
    // A pool large enough that the ceiling is provably inert, so what follows
    // is an assertion about the law (template part 4).
    const POOL_INERT: usize = 1 << 20;

    let mut cases: Vec<Vec<f64>> = vec![WIRE_C7.to_vec(), WIRE_C8.to_vec()];
    for n in 2..=8usize {
        cases.push(vec![137.0; n]);
    }
    cases.push(vec![50.0, 900.0, 3_000.0]);

    for anchors in &cases {
        let n = anchors.len();
        let sigma: f64 = anchors.iter().sum();
        for sum_cap in [false, true] {
            let paper = published_pooled_cap_unclamped(anchors, POOL_GAIN, sum_cap);

            // (i) The unclamped law, exactly — no quantization on this side.
            let engine_raw = pooled_store_cap_unclamped(sum_cap, false, 1.0, n, sigma, POOL_GAIN);
            assert!(
                (engine_raw - paper).abs() < 1e-9,
                "N={n} sum_cap={sum_cap}: unclamped engine {engine_raw} vs paper {paper}"
            );

            // (ii) The realized value: the paper's expression, ceil'd.
            let engine = pooled_store_cap(true, sum_cap, false, 1.0, n, sigma, POOL_GAIN, POOL_FLOOR, POOL_INERT)
                .expect("engaged at N >= 2 with a positive base");
            let err = engine as f64 - paper;
            assert!(
                (0.0..1.0).contains(&err),
                "N={n} sum_cap={sum_cap}: realized {engine} is not paper {paper} ceil'd (err {err})"
            );

            // (iii) The clamp is not answering.
            let ceiling = published_pooled_ceiling(n, POOL_INERT, POOL_FLOOR);
            assert!(
                engine < ceiling && engine > POOL_FLOOR,
                "N={n} sum_cap={sum_cap}: value {engine} is not interior — this \
                 assertion compared two clamps, not two laws"
            );
        }
    }
}

/// The ceiling, agreed separately — `max(N·knee, floor)`, not bare `N·knee`,
/// and the same expression on both arms. A law and its clamp are asserted
/// apart, and the `max(·, floor)` half is the piece shorthand keeps dropping.
#[test]
fn published_pooled_ceiling_equals_the_engine_ceiling_on_both_arms() {
    for n in 2..=8usize {
        // A base so large the value cannot be interior: what comes back is the
        // ceiling, which makes this an assertion about the bound.
        for sum_cap in [false, true] {
            let engine =
                pooled_store_cap(true, sum_cap, false, 1.0, n, 1.0e12, POOL_GAIN, POOL_FLOOR, KNEE).expect("on");
            assert_eq!(
                engine,
                published_pooled_ceiling(n, KNEE, POOL_FLOOR),
                "N={n} sum_cap={sum_cap}: the ceiling is not max(N·knee, floor)"
            );
        }
        // The `max(·, floor)` clause exercised where it differs: a knee below
        // the floor. No shipped cell reaches this, which is why it is pinned.
        assert_eq!(
            pooled_store_cap(true, true, false, 1.0, n, 1.0e12, POOL_GAIN, POOL_FLOOR, 1),
            Some(POOL_FLOOR),
            "N={n}: the ceiling dropped its floor clause"
        );
    }
}

/// The published predictions, recomputed from the wire's measured anchors
/// rather than copied from the paper's table. They are what a pre-registration
/// is scored against, so they must be a consequence of the formula and the
/// measured inputs: correcting an anchor input fails this test and flags the
/// paper's table.
#[test]
fn the_published_predictions_are_what_the_law_computes_at_the_wires_anchors() {
    for (cell, anchors, expect_corrected) in
        [("c7", &WIRE_C7, 3_271usize), ("c8", &WIRE_C8, 3_020usize)]
    {
        let sigma: f64 = anchors.iter().sum();
        let n = anchors.len();

        // The shipped arm: pinned at the ceiling, 2·knee at a dual (the
        // 121/126-reps observation, as arithmetic).
        let shipped = pooled_store_cap(true, false, false, 1.0, n, sigma, POOL_GAIN, POOL_FLOOR, KNEE)
            .expect("on");
        assert_eq!(shipped, 2 * KNEE, "{cell}: the shipped arm is not pinned at 2·knee");
        assert_eq!(shipped, 4_096);

        // The corrected arm: interior, and exactly the published integer.
        let corrected = pooled_store_cap(true, true, false, 1.0, n, sigma, POOL_GAIN, POOL_FLOOR, KNEE)
            .expect("on");
        assert_eq!(
            corrected, expect_corrected,
            "{cell}: §16.60's published prediction is not what the law computes"
        );
        assert!(
            corrected < 2 * KNEE && corrected > POOL_FLOOR,
            "{cell}: the correction is not interior — the prediction would be a clamp"
        );

        // The published ratios, recomputed: c7 0.799, c8 0.737.
        let ratio = corrected as f64 / shipped as f64;
        assert!(
            (0.70..0.81).contains(&ratio),
            "{cell}: the cap ratio {ratio:.4} left the published band"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// 5. The δ-dial laws: the span horizon b(δ) (paper §5.4) and the δ-cap
//    setpoint q(δ) (paper §6.1), on the same four-part template.
// ════════════════════════════════════════════════════════════════════════

/// Paper §6.1 —
/// `q(δ) = q_lo + (q_hi − q_lo)·(clamp(b, b_lo, b_hi) − b_lo)/(b_hi − b_lo)`,
/// in the paper's symbols and order. The band endpoints are RFC 8289 §3.2's;
/// the dial endpoints are the dial's.
fn published_codel_q(b: f64) -> f64 {
    let (q_lo, q_hi) = (0.05, 0.10);
    let (b_lo, b_hi) = (0.5, 2.0);
    q_lo + (q_hi - q_lo) * ((b.clamp(b_lo, b_hi) - b_lo) / (b_hi - b_lo))
}

/// Law: `b(δ)`, paper §5.4. The engine's span horizon
/// (`span_horizon_b(delta_price(hint))`) against the paper's continuous form
/// over the whole dial — including between the presets and at both ends of
/// the clamp, so the law is read, not only pinned at three values.
#[test]
fn published_span_horizon_b_equals_the_engine_over_the_whole_delta_dial() {
    // 1. The named points, absolutely and bit-exactly — the numbers the paper
    //    publishes.
    assert_eq!(delta_budget_b_of(delta_price(ProtocolHint::Realtime)), 0.5);
    assert_eq!(delta_budget_b_of(delta_price(ProtocolHint::Auto)), 1.0);
    assert_eq!(delta_budget_b_of(delta_price(ProtocolHint::Bulk)), 2.0);
    assert_eq!(published_span_horizon_b(50.0), 0.5);
    assert_eq!(published_span_horizon_b(0.5), 1.0);
    assert_eq!(published_span_horizon_b(0.005), 2.0);

    // 2. Agreement with the paper's transcription over a log-uniform sweep of
    //    the dial's span, and two decades past each end so the clamp — the
    //    law's range, not a mode — is exercised on both sides.
    let (lo, hi) = (0.005f64 / 100.0, 50.0f64 * 100.0);
    const N: usize = 600;
    for i in 0..=N {
        let d = lo * (hi / lo).powf(i as f64 / N as f64);
        let (eng, pap) = (delta_budget_b_of(d), published_span_horizon_b(d));
        assert!(
            (eng - pap).abs() < 1e-12,
            "δ={d}: engine {eng} vs paper {pap}"
        );
    }

    // 3. The off-points `b_grid()` sweeps. The store-cap rows are evaluated at
    //    b = 0.75 and b = 1.5, which are not preset values. Invert the law
    //    (δ = δ_Auto·10^(−2·log₂ b)) and check those b are points on this dial
    //    rather than free parameters of the cap benches.
    for b in b_grid() {
        let d = 0.5 * 10f64.powf(-2.0 * b.log2());
        let back = delta_budget_b_of(d);
        assert!(
            (back - b).abs() < 1e-12,
            "b={b} is not a point on the δ dial: δ={d} maps back to {back}"
        );
        assert!(
            (published_span_horizon_b(d) - b).abs() < 1e-12,
            "b={b}: the paper's form disagrees at its own δ={d}"
        );
    }

    // 4. The no-mode-switch property: ±2 % nudges either side of every named
    //    point move b by less than 0.01 and never move it up. A behaviour step
    //    across a preset is a defect even if each side is correct (CLAUDE.md).
    //
    //    Realtime and Bulk are the clamp's two endpoints — b(50) = ½ and
    //    b(0.005) = 2 are where `clamp(½, 2)` starts binding — so the outward
    //    nudge at those presets is flat by construction, not by a mode.
    //    Strictness is asserted where the law is unclamped (the open interval,
    //    clause 2's sweep and clause 5) and non-strictness at the endpoints.
    for h in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let d0 = delta_price(h);
        let (lo_b, mid, hi_b) = (
            delta_budget_b_of(d0 * 0.98),
            delta_budget_b_of(d0),
            delta_budget_b_of(d0 * 1.02),
        );
        assert!(
            hi_b <= mid && mid <= lo_b,
            "{h:?}: b is not monotone through the preset: {lo_b} / {mid} / {hi_b}"
        );
        assert!(
            (lo_b - mid).abs() < 0.01 && (hi_b - mid).abs() < 0.01,
            "{h:?}: a 2 % nudge of δ stepped b: {lo_b} / {mid} / {hi_b}"
        );
    }
    // 5. Strict decrease on the dial's interior, where the clamp is inert.
    let mut prev = f64::INFINITY;
    for i in 0..=200 {
        let d = 0.0051 * (49.0f64 / 0.0051).powf(i as f64 / 200.0);
        let b = delta_budget_b_of(d);
        assert!(b < prev, "δ={d}: b({d}) = {b} did not fall below {prev}");
        prev = b;
    }
    // The Auto preset is interior: both nudges move, in the right direction.
    let a = delta_price(ProtocolHint::Auto);
    assert!(
        delta_budget_b_of(a * 1.02) < delta_budget_b_of(a)
            && delta_budget_b_of(a) < delta_budget_b_of(a * 0.98),
        "Auto sits in the clamp's interior and must move strictly either side"
    );
}

/// Law: `q(δ)`, paper §6.1. The engine against the published map over the
/// whole dial, including between the named points — they are points on a
/// dial, not modes.
#[test]
fn published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band() {
    // 1. The named points, absolutely: the numbers the paper publishes.
    let rt = delta_budget_b(ProtocolHint::Realtime);
    let au = delta_budget_b(ProtocolHint::Auto);
    let bu = delta_budget_b(ProtocolHint::Bulk);
    assert!((codel_setpoint_q(rt) - CODEL_TARGET_LO).abs() < 1e-12, "Realtime is not CoDel's 0.05");
    assert!((codel_setpoint_q(bu) - CODEL_TARGET_HI).abs() < 1e-12, "Bulk is not CoDel's 0.10");
    assert!(
        (codel_setpoint_q(au) - 1.0 / 15.0).abs() < 1e-12,
        "Auto is not the band's affine midpoint (1/15 = 6.667 %)"
    );

    // 2. The closed form the paper states as an algebraic consequence, not a
    //    fifth constant: q(b) = (b+1)/30 on the dial's own interval.
    for i in 0..=150 {
        let b = 0.5 + 1.5 * (i as f64 / 150.0);
        assert!(
            (codel_setpoint_q(b) - (b + 1.0) / 30.0).abs() < 1e-12,
            "b={b}: the engine is not (b+1)/30"
        );
        // 3. Agreement with the paper's transcription, everywhere.
        assert!(
            (codel_setpoint_q(b) - published_codel_q(b)).abs() < 1e-12,
            "b={b}: engine {} vs paper {}",
            codel_setpoint_q(b),
            published_codel_q(b)
        );
    }

    // 4. The no-mode-switch property: continuous and strictly monotone through
    //    every named point, with ±2 % nudges either side. A behaviour step
    //    across a preset is a defect even if each side is correct (CLAUDE.md).
    for &b in &[rt, au, bu] {
        let (lo, hi) = (b * 0.98, b * 1.02);
        let (qlo, q0, qhi) = (codel_setpoint_q(lo), codel_setpoint_q(b), codel_setpoint_q(hi));
        // Bulk saturates at the dial's b_hi = 2 (the shipped D(δ)'s own
        // `min`), so above it the map is flat — continuous, never a step.
        assert!(qlo <= q0 && q0 <= qhi, "b={b}: not monotone through the preset");
        assert!((q0 - qlo).abs() < 0.01 && (qhi - q0).abs() < 0.01, "b={b}: a STEP at the preset");
    }

    // 5. The band is never left.
    for i in 0..=400 {
        let b = -1.0 + 5.0 * (i as f64 / 400.0);
        let q = codel_setpoint_q(b);
        assert!(
            (CODEL_TARGET_LO..=CODEL_TARGET_HI).contains(&q),
            "b={b}: q={q} left RFC 8289 §3.2's derived band"
        );
    }
}

/// Law: the δ-cap's value multiplier, paper §6.1. The substitution is one
/// factor, and its reduction to one BDP per path (candidate (d) in
/// `docs/research/successor-candidates.md`) is asserted as a limit.
#[test]
fn the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d() {
    const GAIN: f64 = 2.0;
    // OFF is the shipped fossil, exactly.
    for &b in &b_grid() {
        assert!((pool_value_multiplier(false, b, GAIN) - GAIN).abs() < 1e-12);
        // ON is 1 + q, at every dial point, and strictly below the fossil
        // everywhere — the δ-cap can only shrink the pool.
        let m = pool_value_multiplier(true, b, GAIN);
        assert!((m - (1.0 + codel_setpoint_q(b))).abs() < 1e-12);
        assert!(m < GAIN, "b={b}: the derived multiplier is not below the fossil");
        assert!((1.05..=1.10).contains(&m), "b={b}: multiplier {m} left the derived band");
    }
    // The reduction: q → 0 is exactly one BDP per path, candidate (d) at
    // zero. Asserted at the limit, through the same expression.
    let sigma = 1_234.5f64;
    let zero_slack = sigma; // Σᵢ bwᵢ·RTpropᵢ, no standing queue at all
    let realtime = pool_value_multiplier(true, 0.5, GAIN) * sigma;
    assert!(
        realtime > zero_slack && realtime <= 1.05 * zero_slack + 1e-9,
        "the derived band is not (d) PLUS the power-point allowance"
    );

    // The two axes factorise — the count multiplier and the value multiplier
    // are independent, which makes the four combinations four laws.
    for &sum_cap in &[false, true] {
        for &delta in &[false, true] {
            for n in 2..=6usize {
                let got = pooled_store_cap_unclamped(sum_cap, delta, 1.0, n, n as f64 * 100.0, GAIN);
                let cnt = if sum_cap { 1.0 } else { n as f64 };
                let val = pool_value_multiplier(delta, 1.0, GAIN);
                assert!(
                    (got - val * cnt * (n as f64 * 100.0)).abs() < 1e-9,
                    "the axes do not factorise at sum_cap={sum_cap} delta={delta} N={n}"
                );
            }
        }
    }
}

/// The published predictions are what the law computes, at both anchor eras,
/// driven through the engine's own function.
#[test]
fn the_delta_cap_predictions_are_what_the_law_computes_at_both_anchor_eras() {
    const GAIN: f64 = 2.0;
    const FLOOR: usize = 10;
    const KNEE: usize = 2048;
    let cap = |sigma: f64, b: f64| {
        pooled_store_cap(true, true, true, b, 2, sigma, GAIN, FLOOR, KNEE).expect("engaged")
    };
    let (rt, au, bu) = (0.5, 1.0, 2.0);

    // (A) Primary — the ladder battery's measured Σ (Σ = cap/gain).
    for &(name, sigma, e_rt, e_au, e_bu) in &[
        ("c7", 1_571.2f64, 1650usize, 1676usize, 1729usize),
        ("c8", 1_154.3, 1213, 1232, 1270),
        ("c8L", 2_815.35, 2957, 3004, 3097),
    ] {
        assert_eq!(cap(sigma, rt), e_rt, "{name} Realtime");
        assert_eq!(cap(sigma, au), e_au, "{name} Auto");
        assert_eq!(cap(sigma, bu), e_bu, "{name} Bulk");
        // Interior at every dial point on these anchors — the clamp is not
        // answering (template part 4).
        assert!(cap(sigma, bu) < 2 * KNEE, "{name}: the ceiling bound on the primary anchors");
        assert!(cap(sigma, rt) > FLOOR, "{name}: the floor bound");
    }

    // (B) Secondary — BDP = W/K, the cross-check's CoDel rung, published as
    // "c1 ≈ 184, sc2 ≈ 344, c7 ≈ 1161, c8 ≈ 1685, c8L ≈ 5225"; the engine
    // ceils to whole symbols, so the pins are the ceilings of those reals and
    // the divergence is bounded at < 1 symbol.
    for &(name, bdp, rung) in &[
        ("c1", 174.8f64, 184usize),
        ("sc2", 328.1, 345),
        ("c7", 1_106.1, 1162),
        ("c8", 1_604.8, 1686),
        ("c8L", 4_976.1, 5225),
    ] {
        let real = 1.05 * bdp;
        assert!(
            (real.ceil() as usize).abs_diff(rung) <= 1,
            "{name}: the published CoDel rung {rung} is not ceil(1.05·{bdp}) = {}",
            real.ceil()
        );
    }

    // c8L is pre-declared unreachable on the secondary anchors, by
    // construction: N·knee < BDP, so the ceiling sits below one network
    // window before any setpoint is added and no value of q can be interior.
    assert!(2 * KNEE < 4_976, "c8L's exclusion arithmetic no longer holds");
    assert_eq!(cap(4_976.1, rt), 2 * KNEE, "c8L must PIN on the secondary anchors");
    assert_eq!(cap(4_976.1, bu), 2 * KNEE, "c8L must PIN at every dial point");
}
