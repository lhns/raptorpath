//! # THE AGREEMENT-TEST CLASS — code vs the PUBLISHED formula
//!
//! CLAUDE.md **FORMULA-FIRST LAWS** requires that no law ships without its
//! formula and its per-symbol derivation in the paper. It did not, until now,
//! require anything to check that the shipped code still computes THE
//! PUBLISHED EXPRESSION — and the first law the rule governed diverged from
//! its own publication inside one commit. §16.57 measured it: §16.56
//! published term 1 of the composed cap as `rateᵢ·RTpropᵢ` while
//! `net::three_term_store_cap` had always computed `rateᵢ·Kᵢ·RTpropᵢ`, so the
//! window term ran 4–50 % above the paper on the wire (`K` = 1.04–1.505) and
//! **no test could see it**, because every existing pin asserts a property OF
//! the code (linearity in N, continuity in ρ, the clamp's reachability)
//! rather than EQUALITY WITH THE PAPER.
//!
//! That divergence is adjudicated in §16.56's dated amendment of 2026-08-18
//! (in favour of the code: term 1 funds ONE ACK ROUND TRIP, and the round
//! trip an ack actually takes is `K·RTprop`). This file is the other half of
//! what §16.57 said was owed — the standing instrument that makes the next
//! such divergence a red test rather than a battery finding.
//!
//! ## THE TEMPLATE — how to add a law to this class
//!
//! One test per formula-first law, each with exactly these four parts. Copy
//! the shape; the value of the class is that it is mechanical.
//!
//! 1. **TRANSCRIBE.** A local `fn published_*` that is the paper's expression
//!    and NOTHING ELSE — written from the paper, in the paper's own symbols
//!    and order, with the section and date it was transcribed FROM in a
//!    comment. It must not call the engine function it is checking, must not
//!    import the engine's helpers for the parts it is transcribing, and must
//!    not be "simplified": an algebraically-equal rewrite silently re-derives
//!    the thing under test.
//! 2. **DRIVE BOTH** on the same inputs, over a grid that includes the dials'
//!    named points, the WIRE-MEASURED range of every measured symbol, and the
//!    degenerate ends (N = 1, zero skew, the clamp's two sides).
//! 3. **ASSERT EQUALITY, and BOUND every deliberate divergence.** Where the
//!    engine quantizes (integer µs, `ceil` to whole symbols) the paper's real
//!    arithmetic, the residual is asserted against an explicit bound derived
//!    from the quantization — never hidden by a loose tolerance and never
//!    described in prose. CLAUDE.md: *every documented model-vs-engine
//!    divergence must carry a test that BOUNDS it.*
//! 4. **PROVE THE CLAMP IS NOT ANSWERING.** A law compared to its paper
//!    THROUGH a bound that always binds is a comparison of two constants.
//!    Every agreement assertion states, in the same test, that the value is
//!    interior — or, where the clamp is the thing under test, that it is the
//!    clamp and says so.
//!
//! Laws in the class today: the composed / three-term store cap (§16.56 as
//! amended), its contract stall (§16.56), and the δ deadline `D(δ)` (§16.20.3)
//! that the stall is built on.
//!
//! **Joined 2026-08-18**: the pooled store cap, BOTH forms — §16.60 publishes
//! `clamp(gain·Σ, floor, N·knee)` with a provenance line per symbol, so the
//! corrected form now has a derivation to agree WITH. The shipped form's `×N`
//! still has none (ADR-0070 finding 2 records it as PROVENANCE ABSENT), and
//! this file does NOT manufacture one: what it transcribes for the shipped arm
//! is §16.60's statement OF the defect — the expression the code runs, labelled
//! as the thing under review — which is a different act from publishing a
//! derivation for it. The agreement test's job here is to guarantee that the
//! arm a battery scores is the arm the paper describes, on both sides.

use raptorpath::net::{
    contract_stall_s, delta_budget_b, pooled_store_cap, pooled_store_cap_unclamped,
    shed_deadline_us, three_term_store_cap, ThreeTermTerm, WIN_STORE_MAX,
};
use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::net::{
    cantelli_k, codel_setpoint_q, contract_alpha, pool_value_multiplier,
    quantile_recovery_round_us, rack_recovery_round_us, CODEL_TARGET_HI, CODEL_TARGET_LO,
    RACK_MIN_RTT_DIVISOR, RACK_REO_WND_MULT_INIT, RACK_REO_WND_MULT_MAX, TIMER_GRANULARITY_US,
};

// ─────────────────────────────────────────────────────────────────────────
// 1. TRANSCRIPTIONS — the paper, and nothing but the paper
// ─────────────────────────────────────────────────────────────────────────

/// **PUBLISHED**: paper §16.20.3 / §16.56 — `D(δ) = min(b(δ)·RTprop, 2·RTprop)`.
/// Transcribed 2026-08-18. Real-valued, in SECONDS: the paper states a time,
/// not a microsecond count.
fn published_d_of_delta(b: f64, rtprop_s: f64) -> f64 {
    (b * rtprop_s).min(2.0 * rtprop_s)
}

/// **PUBLISHED**: paper §16.56 —
/// `stall(δ, ρ, srtt) = (1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt)`.
/// Transcribed 2026-08-18. Both terms always computed; no branch on ρ, which
/// is the CLAUDE.md no-mode-switch invariant read off the formula itself.
fn published_stall_s(rho: f64, b: f64, rtprop_s: f64, srtt_s: f64) -> f64 {
    (1.0 - rho) * published_d_of_delta(b, rtprop_s) + rho * ((9.0 / 8.0) * srtt_s + srtt_s)
}

/// **PUBLISHED**: paper §16.56 as AMENDED 2026-08-18 —
///
/// ```text
/// cap = Σᵢ over live_paths [ rateᵢ·srttᵢ + rateᵢ·stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew
///   srttᵢ     = Kᵢ·RTpropᵢ
///   skew      = (maxᵢ RTpropᵢ − minᵢ RTpropᵢ) / 2
///   rate_fast = the rate of the LEAST-RTprop path
///   clamp: [floor, WIN_STORE_MAX] — the memory bound stated OUTSIDE the law
/// ```
///
/// Returned UNCLAMPED and real-valued, so the caller can assert the law and
/// its bound separately (template part 4). The engine's `ceil`-to-whole-
/// symbols is a realization of "the cap is a symbol count" and is applied by
/// the caller, where it is visible.
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

/// **PUBLISHED**: paper §16.60 — the pooled outstanding cap, both forms.
///
/// ```text
///   shipped    cap = clamp( gain · N · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
///   corrected  cap = clamp( gain     · Σᵢ(max_bwᵢ·min_rttᵢ), floor, N·knee )
/// ```
///
/// Transcribed 2026-08-18. Written in the paper's own order and symbols, with
/// the multiplier as the single differing factor because that is how §16.60
/// states it. Returned UNCLAMPED and real-valued so the law and its bounds are
/// asserted separately (template part 4); the caller applies the `ceil` and the
/// clamp where they are visible.
///
/// It takes the per-path anchors rather than a pre-summed Σ on purpose: the
/// claim under test is that the Σ *is* the path-count scaling, and handing the
/// transcription an already-summed number would assume exactly that.
fn published_pooled_cap_unclamped(anchors: &[f64], gain: f64, sum_cap: bool) -> f64 {
    let n = anchors.len() as f64;
    let sigma: f64 = anchors.iter().sum();
    if sum_cap { gain * sigma } else { gain * n * sigma }
}

/// **PUBLISHED**: paper §16.60's ceiling — `max(N·knee, floor)`.
///
/// Transcribed 2026-08-18. ADR-0070's own correction to the shorthand used
/// everywhere else in the tree: the upper clamp is `max(N·knee, floor)`, not
/// bare `N·knee`. At the shipped `knee = 2048` the two coincide and no cell has
/// ever reached the difference — which is exactly why it must be transcribed
/// from the paper rather than from memory.
fn published_pooled_ceiling(n: usize, knee: usize, floor: usize) -> usize {
    (n * knee).max(floor)
}

// ─────────────────────────────────────────────────────────────────────────
// 2. THE GRID — dials at their named points, measured symbols at their
//    WIRE-MEASURED range, and both degenerate ends
// ─────────────────────────────────────────────────────────────────────────

/// `K` at the values §16.57 measured over 833 `[3T]` evaluations, plus the
/// synthetic ends. 1.04 = c8, 1.14 = c7/sc2, 1.15 = c1, 1.505 = c8L.
const K_WIRE: &[f64] = &[1.0, 1.04, 1.14, 1.15, 1.505, 2.0];

/// ρ across its whole dial including both ends — the shed term is live only
/// below 1, and ρ = 1 is the shipped retain-until-acked scope.
const RHO_GRID: &[f64] = &[0.0, 0.25, 0.5, 0.75, 0.9, 1.0];

/// b(δ) at the protocol's NAMED POINTS, plus two off-point values, because
/// they are points on a dial and not modes: the law must agree with the paper
/// BETWEEN them too.
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
/// which truncates TWICE toward zero: seconds→µs (< 1 µs) and then the
/// product `b·rtprop_us`→u64 (< 1 µs). So the engine's `D` is below the
/// paper's by strictly less than 2 µs, never above, and the stall carries
/// that residual weighted by `(1 − ρ)`. Stated as an ABSOLUTE bound in
/// seconds rather than a relative tolerance, so it cannot silently absorb a
/// real divergence: at the grid's smallest RTprop it is already 0.02 % of D.
const D_QUANT_BOUND_S: f64 = 2e-6;

// ─────────────────────────────────────────────────────────────────────────
// 3. THE TESTS
// ─────────────────────────────────────────────────────────────────────────

/// **LAW: `D(δ)`, paper §16.20.3.** The engine's `shed_deadline_us` against
/// the published `min(b·RTprop, 2·RTprop)`.
///
/// The engine returns integer µs; the paper returns a time. The divergence is
/// a floor-toward-zero of at most [`D_QUANT_BOUND_S`] and it is asserted
/// SIGNED (the engine is never ABOVE the paper), because an unsigned band
/// would pass on a law that had drifted upward by a µs for a different reason.
#[test]
fn published_delta_deadline_equals_the_engine_shed_deadline() {
    for &b in &b_grid() {
        for &rtprop_ms in &[0.05f64, 1.0, 8.0, 38.0, 60.0, 353.0] {
            let rtprop_s = rtprop_ms / 1e3;
            let engine_s = shed_deadline_us(b, (rtprop_s * 1e6) as u64) as f64 / 1e6;
            let paper_s = published_d_of_delta(b, rtprop_s);
            let err = paper_s - engine_s;
            // The lower end is `-f64::EPSILON`, not `0.0`: where the µs
            // truncation happens to be exact the two sides still differ by one
            // ULP of double rounding (`0.75·38 ms` reads 0.0284999…97 against
            // 0.0285). That is a representation artefact of the comparison, not
            // a divergence of the law, and it is the ONLY slack on this side.
            assert!(
                (-f64::EPSILON..D_QUANT_BOUND_S).contains(&err),
                "D(δ) diverges from §16.20.3 beyond the µs quantization: \
                 b={b} RTprop={rtprop_ms}ms paper={paper_s} engine={engine_s} err={err}"
            );
        }
    }
}

/// **LAW: the contract stall, paper §16.56.** `contract_stall_s` against the
/// published `(1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt)`.
///
/// At ρ = 1 — the SHIPPED scope — the shed term is multiplied by zero, so the
/// agreement is EXACT and is asserted exactly. Below ρ = 1 it carries the
/// `D` quantization, weighted by `(1 − ρ)`, and that weighting is asserted
/// rather than assumed: a residual that did NOT shrink with ρ would mean the
/// divergence is in the retained term, which is a different bug.
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

/// **LAW: the composed / three-term store cap, paper §16.56 AS AMENDED
/// 2026-08-18.** `net::three_term_store_cap` against the published Σ.
///
/// This is the test that would have caught the §16.57 divergence on the
/// commit that introduced it: transcribe the paper's term 1 as `rateᵢ·RTpropᵢ`
/// (its pre-amendment form) and this test fails at every `K > 1` in the grid,
/// which is every wire value ever measured.
///
/// Template part 4 is explicit here: the composed law's ONLY remaining bound
/// is the memory bound, and ADR-0070's entire postmortem is about a clamp
/// that ate its law's evidence. Every geometry in the grid is asserted
/// INTERIOR before its value is compared, so this can never quietly become an
/// assertion that two constants are both 4096.
#[test]
fn published_composed_cap_equals_the_engine_three_term_law() {
    const FLOOR: usize = 0; // inert by construction; the clamp is asserted below
    let mut checked = 0usize;
    for &rho in RHO_GRID {
        for &b in &b_grid() {
            for &k in K_WIRE {
                // The geometries: the single path (span zero BY ARITHMETIC),
                // the symmetric dual (span zero for the other reason — max ==
                // min), the ASYMMETRIC dual that is the only one with a live
                // span term, and the symmetric quad ADR-0070's prevention kit
                // added as the N ≥ 3 axis the tree never had.
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

                    // (4) THE CLAMP IS NOT ANSWERING.
                    assert!(
                        limit < WIN_STORE_MAX && limit > FLOOR,
                        "the memory bound is answering instead of the law \
                         (N={} ρ={rho} b={b} K={k} limit={limit}) — this comparison \
                         would be of two constants",
                        g.len()
                    );

                    // (3) EQUALITY. The engine's total is the same real number
                    // the paper's is, before its `ceil` to whole symbols; the
                    // per-ρ residual is the SAME (1−ρ)·quantization the stall
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
    // MECHANISM LIVENESS (MEASUREMENT DISCIPLINE rule 1): a grid that silently
    // became empty would pass this test while asserting nothing.
    assert_eq!(
        checked,
        RHO_GRID.len() * b_grid().len() * K_WIRE.len() * 4,
        "the agreement grid did not execute at full size"
    );
}

/// **THE AMENDMENT ITSELF, PINNED.** §16.56's pre-amendment term 1
/// (`rateᵢ·RTpropᵢ`) is NOT what the engine computes, and the ratio between
/// them is exactly `K` on the window term.
///
/// This exists so the adjudication cannot be silently reversed in either
/// direction: it states the size of what was adjudicated, at the wire's own
/// `K` values, as a number rather than as the §16.57 sentence "4–50 %".
#[test]
fn the_amended_term_one_is_k_times_the_pre_amendment_term_one() {
    for &k in &[1.04f64, 1.14, 1.15, 1.505] {
        let g = [(LEG_C2.0, LEG_C2.1, k)];
        let terms = [Some(ThreeTermTerm { rate: g[0].0, rtprop_s: g[0].1, k })];
        let (_, window, ..) = three_term_store_cap(true, &terms, 1.0, 1.0, 0).expect("warm");
        let pre_amendment_window = g[0].0 * g[0].1; // rate·RTprop, §16.56 as first published
        assert!(
            (window / pre_amendment_window - k).abs() < 1e-12,
            "the window term is not K× the pre-amendment published term: K={k}"
        );
        // And the §16.57 headline, recomputed rather than quoted: 4–50 % high.
        let pct = 100.0 * (k - 1.0);
        assert!(
            (4.0..=50.5).contains(&pct),
            "K={k} is outside the range §16.57 measured on the wire"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 4. THE POOLED STORE CAP (paper §16.60) — joined 2026-08-18
// ─────────────────────────────────────────────────────────────────────────

/// The wire's own per-path anchors in symbols, reconstructed the way
/// `store_cap_sf_bench::AckShape::anchor_sym` does it (READOUT 3's three
/// measured columns multiplied: `xanchor · rate_lr · RTprop`). Transcribed here
/// so the agreement is driven over the range the law actually operates in —
/// template part 2 — and not only over round synthetic numbers.
const WIRE_C7: [f64; 2] = [9.80 * 9_432.0 * 0.0077, 10.11 * 9_418.0 * 0.0097];
const WIRE_C8: [f64; 2] = [13.29 * 6_948.0 * 0.0084, 13.82 * 1_376.0 * 0.0386];

/// The shipped pooled-law constants (`sender_policy::resolve`, `gates.rs`).
const POOL_GAIN: f64 = 2.0;
const KNEE: usize = 2048;
const POOL_FLOOR: usize = raptorpath::net::sender_policy::STORE_CAP_FLOOR;

/// **LAW: the pooled store cap, paper §16.60, BOTH ARMS.**
/// `net::pooled_store_cap` against the published expressions.
///
/// Driven over the wire's own measured anchors, a symmetric synthetic sweep to
/// N = 8 (the axis no cell reaches, and the only one on which the two arms'
/// shapes are distinguishable), and an asymmetric geometry so nothing here
/// depends on the legs being equal. The engine's `ceil`-to-whole-symbols is the
/// sole deliberate divergence and it is BOUNDED rather than absorbed: the
/// realized value is the published one rounded up by strictly less than one
/// symbol, asserted SIGNED.
#[test]
fn published_pooled_cap_equals_the_engine_pooled_cap_on_both_arms() {
    // A pool large enough that the ceiling is provably inert, so what follows
    // is an assertion about the LAW (template part 4 / DISCIPLINE 17b).
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

            // (i) The UNCLAMPED law, exactly — no quantization on this side.
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

            // (iii) PROVE THE CLAMP IS NOT ANSWERING.
            let ceiling = published_pooled_ceiling(n, POOL_INERT, POOL_FLOOR);
            assert!(
                engine < ceiling && engine > POOL_FLOOR,
                "N={n} sum_cap={sum_cap}: value {engine} is not interior — this \
                 assertion compared two clamps, not two laws"
            );
        }
    }
}

/// **THE CEILING, agreed separately** — `max(N·knee, floor)`, not bare
/// `N·knee`, and it is the same expression on both arms.
///
/// Asserted on its own because the whole ADR-0070 postmortem is that a law and
/// its clamp were never asserted apart, and because the `max(·, floor)` half is
/// exactly the piece the tree's own shorthand keeps dropping.
#[test]
fn published_pooled_ceiling_equals_the_engine_ceiling_on_both_arms() {
    for n in 2..=8usize {
        // A base so large the value cannot be interior: what comes back IS the
        // ceiling, which is what makes this an assertion about the bound.
        for sum_cap in [false, true] {
            let engine =
                pooled_store_cap(true, sum_cap, false, 1.0, n, 1.0e12, POOL_GAIN, POOL_FLOOR, KNEE).expect("on");
            assert_eq!(
                engine,
                published_pooled_ceiling(n, KNEE, POOL_FLOOR),
                "N={n} sum_cap={sum_cap}: the ceiling is not max(N·knee, floor)"
            );
        }
        // The `max(·, floor)` clause exercised where it actually differs: a knee
        // below the floor. No shipped cell reaches this, which is precisely why
        // the shorthand lost it and why it is pinned here.
        assert_eq!(
            pooled_store_cap(true, true, false, 1.0, n, 1.0e12, POOL_GAIN, POOL_FLOOR, 1),
            Some(POOL_FLOOR),
            "N={n}: the ceiling dropped its floor clause"
        );
    }
}

/// **THE PUBLISHED PREDICTIONS, PINNED** — §16.60's table, RECOMPUTED from the
/// wire's measured anchors rather than transcribed from the table.
///
/// These are the numbers the battery's pre-registration is scored against, so
/// they must be a CONSEQUENCE of the published formula and the measured inputs.
/// If an anchor input is ever corrected this test fails and the paper's table is
/// wrong — which is the intended coupling, and the reason the predictions live
/// in a test at all rather than only in prose.
#[test]
fn the_published_predictions_are_what_the_law_computes_at_the_wires_anchors() {
    for (cell, anchors, expect_corrected) in
        [("c7", &WIRE_C7, 3_271usize), ("c8", &WIRE_C8, 3_020usize)]
    {
        let sigma: f64 = anchors.iter().sum();
        let n = anchors.len();

        // The SHIPPED arm: pinned at the ceiling, 2·knee at a dual. This is the
        // 121/126-reps observation, as arithmetic.
        let shipped = pooled_store_cap(true, false, false, 1.0, n, sigma, POOL_GAIN, POOL_FLOOR, KNEE)
            .expect("on");
        assert_eq!(shipped, 2 * KNEE, "{cell}: the shipped arm is not pinned at 2·knee");
        assert_eq!(shipped, 4_096);

        // The CORRECTED arm: interior, and exactly the published integer.
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

        // The ratios §16.60 states, recomputed: c7 0.799, c8 0.737.
        let ratio = corrected as f64 / shipped as f64;
        assert!(
            (0.70..0.81).contains(&ratio),
            "{cell}: the cap ratio {ratio:.4} left the published band"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// JOINED 2026-08-19 — the δ-cap (§16.66), the RACK round (§16.67) and the
// derived quantile round (§16.68). Same four-part template: TRANSCRIBE from
// the paper, DRIVE BOTH on one grid, ASSERT EQUALITY with every deliberate
// divergence BOUNDED, and PROVE THE CLAMP IS NOT ANSWERING.
// ════════════════════════════════════════════════════════════════════════

/// **PUBLISHED**: paper §16.66 —
/// `q(δ) = q_lo + (q_hi − q_lo)·(clamp(b, b_lo, b_hi) − b_lo)/(b_hi − b_lo)`.
/// Transcribed 2026-08-19, in the paper's own symbols and order. The band
/// endpoints are RFC 8289 §3.2's; the dial endpoints are the dial's.
fn published_codel_q(b: f64) -> f64 {
    let (q_lo, q_hi) = (0.05, 0.10);
    let (b_lo, b_hi) = (0.5, 2.0);
    q_lo + (q_hi - q_lo) * ((b.clamp(b_lo, b_hi) - b_lo) / (b_hi - b_lo))
}

/// **PUBLISHED**: paper §16.67 — RFC 8985 §6.2 Step 4 with RFC 9002 §6.1.2's
/// granularity floor: `max(min(mult·min_rtt/4, srtt), G)`. Transcribed
/// 2026-08-19.
fn published_rack_round(srtt_us: u64, min_rtt_us: u64, mult: u64) -> u64 {
    let m = mult.clamp(1, 17);
    ((m * min_rtt_us) / 4).min(srtt_us).max(TIMER_GRANULARITY_US)
}

/// **PUBLISHED**: paper §16.68 — `W(α) = srtt + √((1−α)/α)·σ`, Cantelli.
/// Transcribed 2026-08-19.
fn published_quantile_round(srtt_us: u64, sigma_us: u64, alpha: f64) -> u64 {
    let k = ((1.0 - alpha) / alpha).sqrt();
    ((srtt_us as f64 + k * sigma_us as f64) as u64).max(TIMER_GRANULARITY_US)
}

/// **LAW: `q(δ)`, paper §16.66.** The engine against the published map, over
/// the whole dial including BETWEEN the named points — they are points on a
/// dial, not modes, so the law must agree off them too.
#[test]
fn published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band() {
    // 1. The named points, ABSOLUTELY. These are the numbers §16.66 publishes.
    let rt = delta_budget_b(ProtocolHint::Realtime);
    let au = delta_budget_b(ProtocolHint::Auto);
    let bu = delta_budget_b(ProtocolHint::Bulk);
    assert!((codel_setpoint_q(rt) - CODEL_TARGET_LO).abs() < 1e-12, "Realtime is not CoDel's 0.05");
    assert!((codel_setpoint_q(bu) - CODEL_TARGET_HI).abs() < 1e-12, "Bulk is not CoDel's 0.10");
    assert!(
        (codel_setpoint_q(au) - 1.0 / 15.0).abs() < 1e-12,
        "Auto is not the band's affine midpoint (1/15 = 6.667 %)"
    );

    // 2. The closed form §16.66 states as an algebraic consequence, not a
    //    fifth constant: q(b) = (b+1)/30 on the dial's own interval.
    for i in 0..=150 {
        let b = 0.5 + 1.5 * (i as f64 / 150.0);
        assert!(
            (codel_setpoint_q(b) - (b + 1.0) / 30.0).abs() < 1e-12,
            "b={b}: the engine is not (b+1)/30"
        );
        // 3. AGREEMENT with the paper's transcription, everywhere.
        assert!(
            (codel_setpoint_q(b) - published_codel_q(b)).abs() < 1e-12,
            "b={b}: engine {} vs paper {}",
            codel_setpoint_q(b),
            published_codel_q(b)
        );
    }

    // 4. THE NO-MODE-SWITCH PROPERTY, asserted rather than described:
    //    continuous and strictly monotone through every named point, with
    //    ±2 % nudges either side. A behaviour STEP across a preset is a defect
    //    even if each side is individually correct (CLAUDE.md).
    for &b in &[rt, au, bu] {
        let (lo, hi) = (b * 0.98, b * 1.02);
        let (qlo, q0, qhi) = (codel_setpoint_q(lo), codel_setpoint_q(b), codel_setpoint_q(hi));
        // Bulk saturates at the dial's own b_hi = 2 (the shipped D(δ)'s own
        // `min`), so above it the map is FLAT — continuous, never a step.
        assert!(qlo <= q0 && q0 <= qhi, "b={b}: not monotone through the preset");
        assert!((q0 - qlo).abs() < 0.01 && (qhi - q0).abs() < 0.01, "b={b}: a STEP at the preset");
    }

    // 5. THE BAND IS NEVER LEFT — the design decision §16.66 records, asserted.
    for i in 0..=400 {
        let b = -1.0 + 5.0 * (i as f64 / 400.0);
        let q = codel_setpoint_q(b);
        assert!(
            (CODEL_TARGET_LO..=CODEL_TARGET_HI).contains(&q),
            "b={b}: q={q} left RFC 8289 §3.2's derived band"
        );
    }
}

/// **LAW: the δ-cap's value multiplier, paper §16.66.** The substitution is
/// ONE FACTOR, and the reduction to ADR-0071 candidate (d) is asserted as a
/// limit rather than described in prose.
#[test]
fn the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d() {
    const GAIN: f64 = 2.0;
    // OFF is the shipped fossil, exactly.
    for &b in &b_grid() {
        assert!((pool_value_multiplier(false, b, GAIN) - GAIN).abs() < 1e-12);
        // ON is 1 + q, at every dial point, and it is strictly BELOW the
        // fossil everywhere — the δ-cap can only ever shrink the pool.
        let m = pool_value_multiplier(true, b, GAIN);
        assert!((m - (1.0 + codel_setpoint_q(b))).abs() < 1e-12);
        assert!(m < GAIN, "b={b}: the derived multiplier is not below the fossil");
        assert!((1.05..=1.10).contains(&m), "b={b}: multiplier {m} left the derived band");
    }
    // THE REDUCTION: q → 0 is exactly one BDP per path, which IS candidate
    // (d) ZERO. Asserted at the limit, through the same expression.
    let sigma = 1_234.5f64;
    let zero_slack = sigma; // Σᵢ bwᵢ·RTpropᵢ, no standing queue at all
    let realtime = pool_value_multiplier(true, 0.5, GAIN) * sigma;
    assert!(
        realtime > zero_slack && realtime <= 1.05 * zero_slack + 1e-9,
        "the derived band is not (d) PLUS the power-point allowance"
    );

    // The two axes FACTORISE — the count multiplier and the value multiplier
    // are independent, which is what makes the four combinations four laws.
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

/// **THE PUBLISHED PREDICTIONS ARE WHAT THE LAW COMPUTES**, at BOTH anchor
/// eras §16.66 carries, driven through the engine's own function.
#[test]
fn the_delta_cap_predictions_are_what_the_law_computes_at_both_anchor_eras() {
    const GAIN: f64 = 2.0;
    const FLOOR: usize = 10;
    const KNEE: usize = 2048;
    let cap = |sigma: f64, b: f64| {
        pooled_store_cap(true, true, true, b, 2, sigma, GAIN, FLOOR, KNEE).expect("engaged")
    };
    let (rt, au, bu) = (0.5, 1.0, 2.0);

    // (A) PRIMARY — the ladder battery's own measured Σ (Σ = cap/gain).
    for &(name, sigma, e_rt, e_au, e_bu) in &[
        ("c7", 1_571.2f64, 1650usize, 1676usize, 1729usize),
        ("c8", 1_154.3, 1213, 1232, 1270),
        ("c8L", 2_815.35, 2957, 3004, 3097),
    ] {
        assert_eq!(cap(sigma, rt), e_rt, "{name} Realtime");
        assert_eq!(cap(sigma, au), e_au, "{name} Auto");
        assert_eq!(cap(sigma, bu), e_bu, "{name} Bulk");
        // INTERIOR at every dial point on these anchors — the clamp is
        // provably not answering, which is template part 4.
        assert!(cap(sigma, bu) < 2 * KNEE, "{name}: the ceiling bound on the primary anchors");
        assert!(cap(sigma, rt) > FLOOR, "{name}: the floor bound");
    }

    // (B) SECONDARY — ADR-0071's BDP = W/K, the cross-check's CoDel rung.
    // §16.65 published these as "c1 ≈ 184, sc2 ≈ 344, c7 ≈ 1161, c8 ≈ 1685,
    // c8L ≈ 5225"; the engine ceils to whole symbols, so the pins are the
    // ceilings of those reals and the divergence is BOUNDED at < 1 symbol.
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

    // c8L IS PRE-DECLARED UNREACHABLE ON THE SECONDARY ANCHORS, BY
    // CONSTRUCTION: N·knee < BDP, so the ceiling sits below one network
    // window before any setpoint is added and NO value of q can be interior.
    assert!(2 * KNEE < 4_976, "c8L's exclusion arithmetic no longer holds");
    assert_eq!(cap(4_976.1, rt), 2 * KNEE, "c8L must PIN on the secondary anchors");
    assert_eq!(cap(4_976.1, bu), 2 * KNEE, "c8L must PIN at every dial point");
}

/// **LAW: the RACK round, paper §16.67**, and the two findings that refute the
/// backlog item, asserted as arithmetic rather than described.
#[test]
fn published_rack_round_equals_the_engine_and_its_ceiling_is_unreachable() {
    // 1. AGREEMENT with the paper, over the measured geometries and both ends
    //    of RACK's own multiplier range.
    for &(srtt, mrtt) in &[
        (9_000u64, 2_000u64),
        (87_000, 11_000),
        (104_000, 13_000),
        (376_000, 38_000),
        (464_000, 40_000),
        (150, 100), // loopback
    ] {
        for mult in [1u64, 2, 9, 17, 100] {
            assert_eq!(
                rack_recovery_round_us(srtt, mrtt, mult),
                published_rack_round(srtt, mrtt, mult),
                "srtt={srtt} min_rtt={mrtt} mult={mult}"
            );
        }
        // 2. RACK's `mult` is clamped to RACK's OWN range, both ends.
        assert_eq!(
            rack_recovery_round_us(srtt, mrtt, 0),
            rack_recovery_round_us(srtt, mrtt, RACK_REO_WND_MULT_INIT)
        );
        assert_eq!(
            rack_recovery_round_us(srtt, mrtt, 10_000),
            rack_recovery_round_us(srtt, mrtt, RACK_REO_WND_MULT_MAX)
        );
    }

    // 3. THE DEFECT FINDING, PINNED: at RACK's own initial mult the SRTT
    //    ceiling CANNOT bind, because min_rtt ≤ srtt implies min_rtt/4 < srtt
    //    identically. A bound that provably never binds turns its law into a
    //    constant — CLAUDE.md's bind-fraction rule, asserted here in advance
    //    of any measurement.
    for &(srtt, mrtt) in &[(9_000u64, 2_000u64), (87_000, 11_000), (376_000, 38_000)] {
        let base = RACK_REO_WND_MULT_INIT * mrtt / RACK_MIN_RTT_DIVISOR;
        assert!(base < srtt, "the ceiling became reachable at mult=1 — §16.67 needs rewriting");
    }

    // 4. THE UNREACHABILITY ARITHMETIC §16.67 publishes: the mult at which the
    //    ceiling binds is ⌈4·srtt/min_rtt⌉, and at four of five sender-site
    //    cells it exceeds RACK's own maximum of 17.
    let need = |srtt: u64, mrtt: u64| (4 * srtt).div_ceil(mrtt);
    for &(name, srtt, mrtt, want) in &[
        ("c1", 9_000u64, 2_000u64, 18u64),
        ("c7", 87_000, 11_000, 32),
        ("sc2", 104_000, 13_000, 32),
        ("c8", 376_000, 38_000, 40),
        ("c8-AU", 464_000, 40_000, 47),
    ] {
        assert_eq!(need(srtt, mrtt), want, "{name}: the published ceiling-mult changed");
        assert!(want > RACK_REO_WND_MULT_MAX, "{name}: §16.67 claims this is unreachable");
    }
    // The receiver site, where it IS reachable at exactly two cells.
    assert_eq!(need(77_000, 38_000), 9, "c8 receiver");
    assert_eq!(need(82_000, 40_000), 9, "c8-AU receiver");
    assert!(9 <= RACK_REO_WND_MULT_MAX, "the one reachable row is no longer reachable");
}

/// **LAW: the derived quantile round, paper §16.68**, and its REFUTATION,
/// pinned at the tree's own contract numbers so it cannot be quietly tuned
/// into passing.
#[test]
fn published_quantile_round_equals_the_engine_and_the_refutation_is_arithmetic() {
    // 1. Cantelli's closed form, at the contract's own α per hint.
    for &(hint, want_alpha, want_k) in &[
        (ProtocolHint::Realtime, 1e-7, 3_162.0f64),
        (ProtocolHint::Auto, 1e-5, 316.0),
        (ProtocolHint::Bulk, 1e-3, 31.6),
    ] {
        let a = contract_alpha(hint);
        assert!((a - want_alpha).abs() < want_alpha * 1e-9, "{hint:?}: α = {a}");
        let k = cantelli_k(a);
        assert!(
            (k - want_k).abs() / want_k < 0.01,
            "{hint:?}: k(α) = {k}, §16.68 publishes {want_k}"
        );
        // Cantelli's guarantee, checked as the identity it is: 1/(1+k²) = α.
        assert!((1.0 / (1.0 + k * k) - a).abs() < a * 1e-6, "{hint:?}: not Cantelli");
    }

    // 2. AGREEMENT with the paper's transcription.
    for &(srtt, sigma) in &[(77_000u64, 10_000u64), (2_000, 500), (150, 50)] {
        for &a in &[1e-7, 1e-5, 1e-3, 0.0625, 0.5] {
            assert_eq!(
                quantile_recovery_round_us(srtt, sigma, a),
                published_quantile_round(srtt, sigma, a),
                "srtt={srtt} σ={sigma} α={a}"
            );
        }
    }

    // 3. THE REFUTATION, REASON 1, as arithmetic. At c8's measured srtt and a
    //    10 ms σ the derived clock is SECONDS at the contract's own α — 32×
    //    the 100 ms clamp it would replace, and 4× RWM_DERIVED_SWEEP's already
    //    slow 752 ms. If this ever passes, §16.68's verdict must be revisited
    //    rather than the number quietly adjusted.
    let w_auto = quantile_recovery_round_us(77_000, 10_000, contract_alpha(ProtocolHint::Auto));
    assert!(
        (3_200_000..3_300_000).contains(&w_auto),
        "§16.68 publishes 3.24 s at c8/Auto; the law computes {w_auto} µs"
    );
    assert!(w_auto > 32 * 100_000, "the refutation's 32× statement no longer holds");
    let w_rt = quantile_recovery_round_us(77_000, 10_000, contract_alpha(ProtocolHint::Realtime));
    assert!((31_000_000..32_500_000).contains(&w_rt), "§16.68 publishes 31.7 s at Realtime");
    let w_bulk = quantile_recovery_round_us(77_000, 10_000, contract_alpha(ProtocolHint::Bulk));
    assert!((380_000..400_000).contains(&w_bulk), "§16.68 publishes 393 ms at Bulk");

    // 4. α is CONTINUOUS in the dial — it rides ζ, the hint's one declared
    //    price ratio, and nothing keys on a threshold. Monotone in the same
    //    direction as the latency price, at every named point.
    let (r, a, b) = (
        contract_alpha(ProtocolHint::Realtime),
        contract_alpha(ProtocolHint::Auto),
        contract_alpha(ProtocolHint::Bulk),
    );
    assert!(r < a && a < b, "α is not monotone across the dial's named points");
}
