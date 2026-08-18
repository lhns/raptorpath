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
            let engine_raw = pooled_store_cap_unclamped(sum_cap, n, sigma, POOL_GAIN);
            assert!(
                (engine_raw - paper).abs() < 1e-9,
                "N={n} sum_cap={sum_cap}: unclamped engine {engine_raw} vs paper {paper}"
            );

            // (ii) The realized value: the paper's expression, ceil'd.
            let engine = pooled_store_cap(true, sum_cap, n, sigma, POOL_GAIN, POOL_FLOOR, POOL_INERT)
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
                pooled_store_cap(true, sum_cap, n, 1.0e12, POOL_GAIN, POOL_FLOOR, KNEE).expect("on");
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
            pooled_store_cap(true, true, n, 1.0e12, POOL_GAIN, POOL_FLOOR, 1),
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
        let shipped = pooled_store_cap(true, false, n, sigma, POOL_GAIN, POOL_FLOOR, KNEE)
            .expect("on");
        assert_eq!(shipped, 2 * KNEE, "{cell}: the shipped arm is not pinned at 2·knee");
        assert_eq!(shipped, 4_096);

        // The CORRECTED arm: interior, and exactly the published integer.
        let corrected = pooled_store_cap(true, true, n, sigma, POOL_GAIN, POOL_FLOOR, KNEE)
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
