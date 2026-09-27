use super::*;
use raptorpath_math::normal_survival;
use crate::control::estimator::LossEstimator;

const W: usize = 50; // typical window size for tests

// ── The rate mix's byte-identity pin (paper §4.5) ───────
//
// The mix `r(β) = (1−β)·r_anchor + β·r_late-is-fine` must be bit-exact
// at every preset against the single-term hint rule
//     `bulk_late_is_fine = hint == Bulk && bulk_pure_arq`
// so this reproduces that expression inline and compares with
// `assert_eq!` (not a tolerance) at all three hints, both settings of the
// ablation flag, over the suite's estimator fixtures plus a 200-point grid.

/// The single-term reference rate, computed with the `hint == Bulk` rule.
/// Kept deliberately in that shape so "identical" is evidence and not a
/// restatement of the mix.
fn legacy_rate(ctrl: &FecRateController, est: &LossEstimator, window_size: usize) -> f64 {
    let ge = est.ge_estimator();
    let (sigma2, mean_burst) = if ge.is_valid() {
        (
            raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg()),
            ge.mean_burst_length(),
        )
    } else {
        (1.0, 1.0)
    };
    let tput = est.throughput();
    let rtt_secs = est.rtt().as_secs_f64();
    let t_symbols = if ge.is_valid() && tput > 0.0 {
        (rtt_secs * tput / ctrl.symbol_size as f64).max(1.0)
    } else {
        0.0
    };
    let t_sym = if tput > 0.0 { ctrl.symbol_size as f64 / tput } else { 0.0 };
    raptorpath_math::controller_rate(&raptorpath_math::RateInputs {
        p_upper: est.predictive_loss_upper(0.95),
        sigma2,
        mean_burst,
        mass: ge.mass_stats(),
        tail_provision: ctrl.tail_provision,
        window: window_size as f64,
        t_symbols,
        srtt: rtt_secs,
        t_sym,
        codec_overhead: ctrl.rq_overhead,
        tail_target: ctrl.target_tail_loss,
        // The single-term rule, verbatim.
        bulk_late_is_fine: ctrl.hint == ProtocolHint::Bulk && ctrl.bulk_pure_arq,
        completion_exposure: ctrl.completion_exposure,
        inner_feedback: ctrl.inner_feedback,
        saturation_cap: ctrl.saturation_cap_enabled,
        max_overhead: ctrl.max_overhead,
    })
}

/// The mix is byte-identical at every preset. `assert_eq!`, not
/// `abs() < ε`: an approximate match would move every measured baseline
/// silently.
#[test]
fn the_rate_mix_is_byte_identical_at_the_presets() {
    // The fixtures the suite already runs: a settled 5 % channel with no
    // throughput estimate, and the C2 tunnel operating point.
    let mut plain = LossEstimator::new();
    for _ in 0..100 {
        plain.record_batch(100, 95);
    }
    let mut c2 = LossEstimator::new();
    for _ in 0..200 {
        c2.record_batch(1000, 974);
        c2.record_rtt(std::time::Duration::from_millis(13));
        c2.record_throughput(12_500_000.0);
    }
    // A 200-point estimator grid: loss from clean to catastrophic across
    // three window sizes, with and without a throughput estimate, so the
    // burst, mass, saturation and floor terms all take both branches.
    let mut grid: Vec<(LossEstimator, usize)> = Vec::new();
    for i in 0..100 {
        let lost = (i * 7) % 60; // 0 .. 59 lost per 1000, cycling
        let mut e = LossEstimator::new();
        for _ in 0..120 {
            e.record_batch(1000, 1000u32 - lost as u32);
            if i % 2 == 0 {
                e.record_rtt(std::time::Duration::from_millis(5 + (i as u64 % 60)));
                e.record_throughput(1e6 * (1.0 + i as f64 % 40.0));
            }
        }
        grid.push((e, [16usize, 64, 256][i % 3]));
    }
    assert_eq!(grid.len(), 100, "the grid is the pin's own denominator");

    for hint in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        for pure_arq in [true, false] {
            for inner in [0.0f64, 1.0] {
                let mut ctrl =
                    FecRateController::new(1e-5, 0.5, hint, FecBackend::Rlc, 1200);
                ctrl.set_bulk_pure_arq(pure_arq);
                ctrl.set_inner_feedback(inner);
                for (label, est, w) in [
                    ("plain-5pct", &plain, W),
                    ("c2-tunnel", &c2, 56usize),
                ] {
                    assert_eq!(
                        ctrl.compute_repair_rate(est, w),
                        legacy_rate(&ctrl, est, w),
                        "{hint:?} pure_arq={pure_arq} inner={inner} {label}: \
                         the mix is not the pre-repair machine"
                    );
                }
                for (i, (est, w)) in grid.iter().enumerate() {
                    assert_eq!(
                        ctrl.compute_repair_rate(est, *w),
                        legacy_rate(&ctrl, est, *w),
                        "{hint:?} pure_arq={pure_arq} inner={inner} grid[{i}] (W={w}): \
                         the mix is not the pre-repair machine"
                    );
                }
                // And with χ armed, where the Bulk term is not r = 0 and
                // the two branches of the single-term rule actually differ.
                for chi in [0.25f64, 0.5, 1.0] {
                    ctrl.set_completion_exposure(chi);
                    assert_eq!(
                        ctrl.compute_repair_rate(&c2, 56),
                        legacy_rate(&ctrl, &c2, 56),
                        "{hint:?} pure_arq={pure_arq} χ={chi}: the mix diverges \
                         where the two terms are genuinely different"
                    );
                }
                ctrl.set_completion_exposure(0.0);
            }
        }
    }
}

/// The mixing weight is exactly 0 / 0 / 1 at the presets, which is why
/// the identity above holds: `1·a + 0·b == a` and `0·a + 1·b == b`
/// bit-exactly for finite `a`, `b`, and `controller_rate` clamps into
/// `[0, max_overhead]`, so both terms are always finite.
#[test]
fn the_bulkness_weight_is_exact_at_the_presets_and_the_ablation_zeroes_it() {
    for (hint, want) in [
        (ProtocolHint::Realtime, 0.0f64),
        (ProtocolHint::Auto, 0.0),
        (ProtocolHint::Bulk, 1.0),
    ] {
        let mut c = FecRateController::new(1e-5, 0.5, hint, FecBackend::Rlc, 1200);
        assert_eq!(c.bulkness, want, "{hint:?}: β");
        assert_eq!(c.effective_bulkness(), want, "{hint:?}: effective β");
        // The P4a ablation is `β := 0` at every point of the dial, which
        // makes it exactly inert at Realtime and Auto, where β is already 0.
        c.set_bulk_pure_arq(false);
        assert_eq!(c.effective_bulkness(), 0.0, "{hint:?}: the ablation is β := 0");
        assert_eq!(c.bulkness, want, "{hint:?}: the ablation must not move β itself");
    }
}

/// The Bulk point does not step (CLAUDE.md: a behaviour step across a
/// preset is a defect even if each side is individually correct).
///
/// At a fixed estimator state, δ just either side of the Bulk preset must
/// move the rate by no more than the mix's own arithmetic allows: the two
/// terms differ by at most `max_overhead` (both are clamped into
/// `[0, max_overhead]`), so a step of `Δβ` in the weight can move `r` by
/// at most `Δβ·max_overhead`. The bound is therefore
/// `(1 − β(δ'))·max_overhead` for the off-preset δ', and it is asserted
/// over the whole dial, not only at the Bulk seam.
#[test]
fn the_rate_does_not_step_across_the_bulk_preset_or_anywhere_on_the_dial() {
    let mut est = LossEstimator::new();
    for _ in 0..200 {
        est.record_batch(1000, 974);
        est.record_rtt(std::time::Duration::from_millis(13));
        est.record_throughput(12_500_000.0);
    }
    const MAX_OH: f64 = 0.5;
    // Evaluate the mix directly at an arbitrary β, which is what a δ
    // between the presets produces. (`RWM_DELTA` is the shipped route to
    // this; the unit test drives the same arithmetic without the
    // process-global env resolve, which a parallel runner cannot own.)
    let r_at = |beta: f64| {
        let mut c =
            FecRateController::new(1e-5, MAX_OH, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
        c.bulkness = beta;
        c.compute_repair_rate(&est, 56)
    };
    // 1. The Bulk point: δ ∈ {0.005/1.02, 0.005, 0.005·1.02}.
    let d0 = 0.005f64;
    let (b_lo, b_at, b_hi) = (
        raptorpath_math::bulkness_of_delta(d0 * 1.02),
        raptorpath_math::bulkness_of_delta(d0),
        raptorpath_math::bulkness_of_delta(d0 / 1.02),
    );
    assert_eq!(b_at, 1.0);
    assert_eq!(b_hi, 1.0, "below the Bulk anchor the weight is clamped at 1");
    let (r_lo, r_at0, r_hi) = (r_at(b_lo), r_at(b_at), r_at(b_hi));
    assert_eq!(r_at0, r_hi, "the clamped side must be flat, not stepped");
    assert!(
        (r_lo - r_at0).abs() <= (1.0 - b_lo) * MAX_OH,
        "step across the Bulk preset: r={r_lo} at β={b_lo} vs r={r_at0} at β=1, \
         bound {}",
        (1.0 - b_lo) * MAX_OH
    );
    // 2. The whole dial: no adjacent pair on a 400-point log-uniform sweep
    //    of δ may move r by more than the weight's own change allows.
    const N: usize = 400;
    let (lo, hi) = (0.001f64, 50.0f64);
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..=N {
        let d = lo * (hi / lo).powf(i as f64 / N as f64);
        let beta = raptorpath_math::bulkness_of_delta(d);
        let r = r_at(beta);
        if let Some((pb, pr)) = prev {
            assert!(
                (r - pr).abs() <= (beta - pb).abs() * MAX_OH + 1e-12,
                "δ={d}: r stepped from {pr} to {r} on a Δβ of {}",
                (beta - pb).abs()
            );
        }
        prev = Some((beta, r));
    }
    // 3. The other δ-priced leg of the same rate — the effective tail
    //    target `base·ζ(δ)` — is a continuous, strictly decreasing
    //    function of δ with no step at any preset either, so neither term
    //    of the mix carries a hidden seam. Checked as the formula it is.
    let tail = |d: f64| (1e-5f64 * raptorpath_math::zeta_of_delta(d)).clamp(1e-9, 0.1);
    let mut prev_t = f64::INFINITY;
    for i in 0..=N {
        let d = lo * (hi / lo).powf(i as f64 / N as f64);
        let t = tail(d);
        assert!(t <= prev_t, "δ={d}: the effective tail target stepped UP");
        prev_t = t;
    }
    // And it reproduces `base × ζ(hint)` exactly at every preset — the
    // product, not the decimal it prints as: `1e-5 × 0.01` is
    // `1.0000000000000001e-7`, one ulp above the literal `1e-7`.
    for (h, want) in [
        (ProtocolHint::Realtime, 1e-7f64),
        (ProtocolHint::Auto, 1e-5),
        (ProtocolHint::Bulk, 1e-3),
    ] {
        let c = FecRateController::new(1e-5, MAX_OH, h, FecBackend::Rlc, 1200);
        assert_eq!(
            c.target_tail_loss,
            (1e-5f64 * h.tail_loss_scale()).clamp(1e-9, 0.1),
            "{h:?}: ζ(δ(hint)) is not the hint's own scale"
        );
        assert!(
            (c.target_tail_loss - want).abs() <= want * 1e-12,
            "{h:?}: the effective tail target is {} and the paper publishes {want}",
            c.target_tail_loss
        );
    }
}

/// δ-honest shed budget (paper §5.6): the 1−ρ allowance is the design
/// residual ε·(1−P_fec) — 0 with no loss or no
/// FEC sample (cold start sheds nothing), ε itself when r cannot
/// overcome the loss (P_fec = 0), monotone non-increasing in r, and in
/// the ~1% class at a lossy dual-path operating point.
#[test]
fn residual_loss_after_fec_is_the_design_residual() {
    // Cold / degenerate inputs: budget 0.
    assert_eq!(residual_loss_after_fec(0.0, 0.3, 5.0, 3.0), 0.0);
    assert_eq!(residual_loss_after_fec(-1.0, 0.3, 5.0, 3.0), 0.0);
    // r too low to overcome loss: residual = ε (pure-loss allowance).
    let eps = 0.048;
    let all = residual_loss_after_fec(eps, 0.0, 5.0, 3.0);
    assert!((all - eps).abs() < 1e-12, "P_fec=0 ⇒ residual=ε, got {all}");
    // Monotone non-increasing in r.
    let mut prev = all;
    for r in [0.05, 0.1, 0.2, 0.34, 0.5, 1.0] {
        let v = residual_loss_after_fec(eps, r, 5.0, 3.76);
        assert!(v <= prev + 1e-12, "residual must fall as r rises");
        prev = v;
    }
    // A lossy dual-path operating point (ε≈4.8%, consumed r≈0.34,
    // A*≈3–5, GE σ²≈3.76): the residual sits in the ~1% class — well
    // below ε, well above zero.
    let c3 = residual_loss_after_fec(eps, 0.34, 4.0, 3.76);
    assert!(c3 > 0.001 && c3 < eps, "c3-class residual out of class: {c3}");
}

#[test]
fn test_zero_loss_no_repair() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..50 {
        est.record_batch(100, 100);
    }
    let r = ctrl.compute_repair_count(100, &est, W);
    assert!(r < 5, "Expected minimal repair for zero loss, got {r}");
}

#[test]
fn test_high_loss_more_repair() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 80); // 20% loss
    }
    let r = ctrl.compute_repair_count(100, &est, W);
    assert!(r >= 20, "Expected significant repair for 20% loss, got {r}");
    assert!(r <= 50, "Repair should be capped at max overhead, got {r}");
}

#[test]
fn test_protocol_hint_realtime_more_aggressive() {
    let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);
    let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::RaptorQ, 1200);

    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 90);
    }

    let r_rt = ctrl_rt.compute_repair_count(100, &est, W);
    let r_bulk = ctrl_bulk.compute_repair_count(100, &est, W);
    assert!(
        r_rt >= r_bulk,
        "Realtime ({r_rt}) should use >= repair than bulk ({r_bulk})"
    );
}

#[test]
fn test_hint_controls_tail_loss_not_offset() {
    // Realtime with target_tail_loss=1e-5 should behave like Auto with 1e-7
    // (because Realtime applies 100× tighter = 1e-5 * 0.01 = 1e-7)
    let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);
    let ctrl_auto_tight = FecRateController::new(1e-7, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);

    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 90);
    }

    let r_rt = ctrl_rt.compute_repair_rate(&est, W);
    let r_auto = ctrl_auto_tight.compute_repair_rate(&est, W);
    assert!(
        (r_rt - r_auto).abs() < 0.001,
        "Realtime(1e-5) should equal Auto(1e-7): rt={r_rt}, auto={r_auto}"
    );
}

#[test]
fn test_continuous_rate_no_fec_when_target_met() {
    // Paper §4.2 continuity: the z_{δ/ε} margin lets the rate
    // decrease to 0 when pure ARQ meets the tail target — no cutoff
    // branch. Clean link (0.1% loss) under Bulk (δ = 1e-5 × 100 = 1e-3).
    let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
    let ctrl_auto = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::Rlc, 1200);
    let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(1000, 999); // 0.1% loss
    }
    let r_bulk = ctrl_bulk.compute_repair_rate(&est, 50);
    let r_auto = ctrl_auto.compute_repair_rate(&est, 50);
    let r_rt = ctrl_rt.compute_repair_rate(&est, 50);
    // Bulk target (1e-3) ≈ channel loss → essentially no FEC
    assert!(r_bulk < 0.01, "Bulk at 0.1% loss should carry ~no FEC: {r_bulk}");
    // Tighter hints → continuously more FEC
    assert!(r_bulk <= r_auto && r_auto <= r_rt,
        "rate must be monotone in tail tightness: bulk={r_bulk}, auto={r_auto}, rt={r_rt}");
    assert!(r_rt > 0.0, "Realtime at 0.1% loss should still use FEC: {r_rt}");
}

#[test]
fn test_saturation_cap_binds_with_throughput() {
    // C4-like estimator state: 5% loss, long RTT, known throughput.
    // Realtime's aggressive request must be capped at r_sat; without
    // the flag (or without a throughput estimate) it must not be.
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 95); // 5% loss
        est.record_rtt(std::time::Duration::from_millis(210));
    }

    // No throughput estimate -> cap skipped even when enabled.
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    let uncapped_no_tput = ctrl.compute_repair_rate(&est, 64);

    for _ in 0..100 {
        est.record_throughput(2_500_000.0);
    }
    let capped = ctrl.compute_repair_rate(&est, 64);

    let mut ctrl_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    ctrl_off.set_saturation_cap(false);
    let uncapped = ctrl_off.compute_repair_rate(&est, 64);

    assert!(
        (uncapped - uncapped_no_tput).abs() < 1e-9,
        "cap must be inert without a throughput estimate: {uncapped_no_tput} vs {uncapped}"
    );
    assert!(
        capped < uncapped,
        "saturation cap must bind for an aggressive request: capped={capped}, uncapped={uncapped}"
    );
    // The capped rate must be the SOFT saturation of the uncapped request
    // (paper §4.4): it sits just below r_sat, approaching it
    // asymptotically rather than pinning to it exactly.
    let p = est.predictive_loss_upper(0.95);
    let ge = est.ge_estimator();
    let sigma2 = if ge.is_valid() {
        raptorpath_math::burst_variance_factor(ge.p_gb(), ge.p_bg())
    } else {
        1.0
    };
    let r_sat = raptorpath_math::r_saturation(
        p, sigma2, 64.0, est.rtt().as_secs_f64(), 1200.0 / est.throughput(),
    );
    let expected = raptorpath_math::soft_saturate(uncapped, r_sat);
    assert!(
        (capped - expected).abs() < 1e-9,
        "capped rate must equal soft_saturate(uncapped, r_sat): capped={capped}, expected={expected}"
    );
    assert!(
        capped < r_sat && capped > r_sat * (1.0 - raptorpath_math::SAT_SOFTNESS),
        "soft cap sits just below r_sat: capped={capped}, r_sat={r_sat}"
    );
}

#[test]
fn test_bulk_pure_arq_zero_steady_state_rate() {
    // P4a/P6 (paper §4.6): Bulk's effective tail target is the
    // completion-exposure glide δ_eff = ε̂ + (0.05 − ε̂)·χ; the tunnel
    // never sets χ, so δ_eff = ε̂ ("late is fine") and even at 5% loss
    // the steady-state rate is 0 identically (pure ARQ, volume parity
    // with retransmission transports).
    let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 95); // 5% loss
    }
    let r_bulk = ctrl_bulk.compute_repair_rate(&est, W);
    assert!(r_bulk < 0.01, "Bulk at 5% loss must be ~pure ARQ: {r_bulk}");

    // Ablation arm: with the flag off, Bulk falls back to the plain
    // 100×-loosened target and pays steady-state FEC again.
    let mut ctrl_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
    ctrl_off.set_bulk_pure_arq(false);
    let r_off = ctrl_off.compute_repair_rate(&est, W);
    assert!(r_off > r_bulk, "flag off must restore steady-state FEC: on={r_bulk}, off={r_off}");

    // Realtime is untouched by the flag (Bulk-only mapping).
    let ctrl_rt_on = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    let mut ctrl_rt_off = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    ctrl_rt_off.set_bulk_pure_arq(false);
    let rt_on = ctrl_rt_on.compute_repair_rate(&est, W);
    let rt_off = ctrl_rt_off.compute_repair_rate(&est, W);
    assert_eq!(rt_on, rt_off, "Realtime must be unaffected: on={rt_on}, off={rt_off}");
    assert!(rt_on > 0.05, "Realtime at 5% loss still carries FEC: {rt_on}");
}

#[test]
fn test_inner_feedback_floor_tunnel_bulk() {
    // P10a (paper §4.4): at a tunnel operating point (ε ≈ 2.6%,
    // SRTT ≈ 13 ms, 100 Mbit) the Bulk glide alone is pure ARQ
    // mid-stream, but with the inner-feedback weight set (TCP-in-tunnel
    // payload) the repair floor keeps a small proactive rate that
    // covers loss events within the inner flow's stall horizon.
    let mut est = LossEstimator::new();
    for _ in 0..200 {
        est.record_batch(1000, 974); // 2.6% loss (C2 GE average)
        est.record_rtt(std::time::Duration::from_millis(13));
        est.record_throughput(12_500_000.0); // 100 Mbit/s
    }

    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, 1200);
    let base = ctrl.compute_repair_rate(&est, 56);
    assert!(base < 0.005, "weight 0 keeps the pure Bulk glide: {base}");

    ctrl.set_inner_feedback(1.0);
    let floored = ctrl.compute_repair_rate(&est, 56);
    assert!(
        (0.01..=0.06).contains(&floored),
        "C2 tunnel floor must sit in the sane band: {floored}"
    );

    // Continuous in the weight: half weight lands strictly between.
    ctrl.set_inner_feedback(0.5);
    let half = ctrl.compute_repair_rate(&est, 56);
    assert!(
        base < half && half < floored,
        "floor must scale continuously with the weight: {base} < {half} < {floored}"
    );

    // No throughput estimate -> floor inert (same sentinel convention
    // as the burst term): a fresh estimator has no t_sym.
    let mut est_no_tput = LossEstimator::new();
    for _ in 0..200 {
        est_no_tput.record_batch(1000, 974);
        est_no_tput.record_rtt(std::time::Duration::from_millis(13));
    }
    ctrl.set_inner_feedback(1.0);
    let no_tput = ctrl.compute_repair_rate(&est_no_tput, 56);
    assert!(no_tput < 0.005, "floor needs a throughput estimate: {no_tput}");
}

#[test]
fn test_tail_provision_bursty_channel_raises_rate() {
    // #46 (paper §4.3): feed a heavy-clustered per-symbol loss
    // pattern (fade episodes of ~48 lost symbols every ~1500) so the
    // measured window-mass tail is far beyond what the GE margin
    // models. With the tail term on (shipped default) the rate must
    // rise materially above the GE-only rate; with it off
    // (RWM_RSTAR_TAIL=0 arm, via the setter) the GE-only rate returns.
    let mut est = LossEstimator::new();
    for _ in 0..60 {
        // one fade episode + clean stretch, fed with true interleaving
        est.record_counts(1500, 1452);
        for _ in 0..48 {
            est.record_symbol(false);
        }
        for _ in 0..1452 {
            est.record_symbol(true);
        }
    }
    let ge = est.ge_estimator();
    assert!(
        ge.mass_stats().is_valid(),
        "mass statistics must be live after 60 fade episodes"
    );

    let mut ctrl = FecRateController::new(1e-4, 1.0, ProtocolHint::Auto, FecBackend::Rlc, 1200);
    ctrl.set_tail_provision(false);
    let legacy = ctrl.compute_repair_rate(&est, 64);
    ctrl.set_tail_provision(true);
    let corrected = ctrl.compute_repair_rate(&est, 64);
    println!("legacy={legacy:.3} corrected={corrected:.3}");
    assert!(
        corrected > 1.2 * legacy,
        "clustered fades must raise the corrected rate materially: {corrected} vs {legacy}"
    );

    // On a non-bursty channel of the same average loss the two arms
    // stay close (no over-provisioning where GE is adequate): iid-fed
    // pattern (isolated losses).
    let mut est_iid = LossEstimator::new();
    for _ in 0..2000 {
        est_iid.record_counts(31, 30);
        for _ in 0..30 {
            est_iid.record_symbol(true);
        }
        est_iid.record_symbol(false);
    }
    let mut ctrl2 = FecRateController::new(1e-4, 1.0, ProtocolHint::Auto, FecBackend::Rlc, 1200);
    ctrl2.set_tail_provision(false);
    let legacy_iid = ctrl2.compute_repair_rate(&est_iid, 64);
    ctrl2.set_tail_provision(true);
    let corrected_iid = ctrl2.compute_repair_rate(&est_iid, 64);
    println!("iid: legacy={legacy_iid:.3} corrected={corrected_iid:.3}");
    assert!(
        corrected_iid <= 1.35 * legacy_iid,
        "near-iid channel must not be materially over-provisioned: {corrected_iid} vs {legacy_iid}"
    );
}

#[test]
fn test_spare_capacity_capping() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 80);
    }

    let uncapped = ctrl.compute_repair_rate(&est, W);
    let capped = ctrl.compute_repair_rate_capped(&est, 0.05, W);
    assert!(uncapped > 0.05, "Uncapped rate should be > 5%: {uncapped}");
    assert!(capped <= 0.05, "Capped rate should be ≤ 5%: {capped}");
}

#[test]
fn test_codec_overhead_weighted_by_decoder_invocation() {
    // Compare RaptorQ (1% codec overhead) vs ReedSolomon (0% overhead) at same window.
    // The difference isolates the codec overhead contribution.
    let ctrl_rq = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
    let ctrl_rs = FecRateController::new(1e-5, 1.0, ProtocolHint::Auto, FecBackend::ReedSolomon, 1200);

    let mut est = LossEstimator::new();
    // Low-moderate loss so rate doesn't hit max_overhead cap
    for _ in 0..100 {
        est.record_batch(100, 95); // 5% loss
    }

    // At same window, RaptorQ should have higher rate than RS due to codec overhead
    let rate_rq = ctrl_rq.compute_repair_rate(&est, 50);
    let rate_rs = ctrl_rs.compute_repair_rate(&est, 50);
    assert!(
        rate_rq > rate_rs,
        "RaptorQ should have higher rate than RS due to codec overhead: rq={rate_rq}, rs={rate_rs}"
    );

    // With zero window size, no codec overhead → RaptorQ ≈ RS
    let rate_rq_zero = ctrl_rq.compute_repair_rate(&est, 0);
    let rate_rs_zero = ctrl_rs.compute_repair_rate(&est, 0);
    assert!(
        (rate_rq_zero - rate_rs_zero).abs() < 0.01,
        "Zero window should have no codec overhead: rq={rate_rq_zero}, rs={rate_rs_zero}"
    );
}

#[test]
fn test_budget_allocator_basic() {
    let budget = BudgetAllocator::compute(0.1, 0.01, 0.05, 0.8);
    assert!(budget.total_budget() > 0.1);
    assert!(budget.proactive_rate() > 0.0);
    assert!(budget.nack_cap() > 0.0);
    assert!(
        (budget.proactive_rate() + budget.nack_cap() - budget.total_budget()).abs() < 1e-10,
        "Budget should be conserved"
    );
}

#[test]
fn test_budget_allocator_no_nack() {
    let budget = BudgetAllocator::compute(0.1, 0.01, 0.0, 1.0);
    assert!(
        (budget.proactive_rate() - budget.total_budget()).abs() < 1e-10,
        "Without NACK, all budget goes to proactive"
    );
}

#[test]
fn test_budget_allocator_zero_loss() {
    let budget = BudgetAllocator::compute(0.0, 0.01, 0.05, 0.8);
    assert!(budget.total_budget() < 1e-10);
    assert!(budget.proactive_rate() < 1e-10);
}

// --- Taper function tests ---

#[test]
fn test_taper_density_decays() {
    let taper = TaperFunction {
        amplitude: 0.04,
        decay: 0.5, // q=0.5, mean burst = 2
        total_rate: 0.08,
        q: 0.5,
    };

    let d0 = taper.density(0.0);
    let d1 = taper.density(1.0);
    let d2 = taper.density(2.0);

    assert!((d0 - 0.04).abs() < 1e-10, "density(0) = amplitude");
    assert!((d1 - 0.02).abs() < 1e-10, "density(1) = A * 0.5");
    assert!((d2 - 0.01).abs() < 1e-10, "density(2) = A * 0.25");
    assert!(d0 > d1, "density decays");
    assert!(d1 > d2, "density decays monotonically");
}

#[test]
fn test_taper_from_estimator() {
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 90); // 10% loss
    }

    let rate = 0.12; // 12% correction rate
    let taper = TaperFunction::from_estimator(&est, rate);

    assert!(taper.amplitude > 0.0, "amplitude should be positive");
    assert!(taper.decay > 0.0 && taper.decay < 1.0, "decay in (0,1)");
    assert!((taper.total_rate - rate).abs() < 1e-10, "total rate preserved");
    assert!((taper.amplitude - rate * taper.q).abs() < 1e-10, "A = r * q");
}

#[test]
fn test_taper_total_rate_geometric_sum() {
    // The geometric series sum of τ(t) for t=0..∞ should equal total_rate
    let taper = TaperFunction {
        amplitude: 0.06,
        decay: 0.7, // q=0.3
        total_rate: 0.06 / 0.3,
        q: 0.3,
    };

    // Sum first 1000 terms (approximates infinite sum)
    let sum: f64 = (0..1000).map(|t| taper.density(t as f64)).sum();
    assert!(
        (sum - taper.total_rate).abs() < 0.001,
        "geometric sum ≈ A/q = total_rate: sum={sum}, expected={}",
        taper.total_rate
    );
}

// --- #85 TaperBudget tests (RWM_TAPER_R budget law) ---

/// #85 attribution probe (not a gate; `--ignored`): the controller-level
/// r for the two RWM_RSTAR_TAIL arms on a heavy-burst cell
/// (heavy:20;20;5;0.6;0.55;0.5 — semi-Markov, Weibull k=0.5 theta=0.55
/// bursts, onset 0.6% => eps ~3.6%), realtime hint, W=64, with rate/RTT
/// anchors that keep the saturation cap live. Prints GE-only vs
/// tail-provisioned r — the number the emission path consumes per arm.
#[test]
#[ignore = "measurement probe for the #85 L0 cell, not a CI gate"]
fn probe_rstar_arms_c3heavy() {
    let mut est = LossEstimator::new();
    // Deterministic semi-Markov replay of the c3heavy law (splitmix-ish
    // LCG for portability; the exact stream is irrelevant — the shape
    // is the cell's).
    let mut state = 42u64;
    let mut rand = move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let (onset, theta, k) = (0.006f64, 0.55f64, 0.5f64);
    let mut sent = 0u64;
    let mut got = 0u64;
    let mut batch = Vec::with_capacity(64);
    let mut n = 0usize;
    while n < 200_000 {
        // Good sojourn
        let g = ((rand().max(1e-12).ln() / (1.0 - onset).ln()).ceil()).max(1.0) as usize;
        for _ in 0..g {
            batch.push(true);
            n += 1;
        }
        // Weibull bad sojourn
        let b = ((rand().max(1e-300).ln() / theta.ln()).powf(1.0 / k).ceil()).max(1.0)
            .min(10_000.0) as usize;
        for _ in 0..b {
            batch.push(false);
            n += 1;
        }
        // Feed in 64-symbol batches like the receiver's block cadence.
        while batch.len() >= 64 {
            let chunk: Vec<bool> = batch.drain(..64).collect();
            let ok = chunk.iter().filter(|&&x| x).count() as u32;
            sent += 64;
            got += ok as u64;
            est.record_counts(64, ok);
            for &x in &chunk {
                est.record_symbol(x);
            }
        }
    }
    // Anchors: 20 mbit, 40 ms RTT, realtime symbol size 512.
    for _ in 0..100 {
        est.record_rtt(std::time::Duration::from_millis(40));
        est.record_throughput(2_500_000.0);
    }
    let eps = 1.0 - got as f64 / sent as f64;
    let mut ctrl =
        FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 512);
    ctrl.set_tail_provision(false);
    let r_legacy = ctrl.compute_repair_rate(&est, 64);
    ctrl.set_tail_provision(true);
    let r_corrected = ctrl.compute_repair_rate(&est, 64);
    println!(
        "c3heavy probe: eps={:.3} mass_valid={} r_legacy={r_legacy:.3} r_corrected={r_corrected:.3}",
        eps,
        est.ge_estimator().mass_stats().is_valid()
    );
}

/// Replicates the plain-mode emission loop's accounting: per source
/// symbol accrue into the fractional debt, emit whole symbols, reset
/// the taper offset at each ack (per `ack_every`; 0 = never = one
/// endless cycle). Returns emitted repair symbols.
///
/// `budget = true` runs the #85 TaperBudget law; `false` runs the
/// per-ack-cycle density accrual (τ at the offset, spare-capped), kept
/// here as the executable statement of the failure the law prevents.
fn simulate_emission(
    rate: f64,
    q: f64,
    span: usize,
    n_sources: u64,
    ack_every: u64,
    spare: f64,
    budget: bool,
) -> u64 {
    let taper = TaperFunction {
        amplitude: rate * q,
        decay: 1.0 - q,
        total_rate: rate,
        q,
    };
    let mut tb = TaperBudget::new();
    let mut debt = 0.0f64;
    let mut emitted = 0u64;
    let mut offset = 0u64;
    for i in 0..n_sources {
        let add = if budget {
            tb.accrue(rate, offset, &taper, span, spare)
        } else {
            taper.density(offset as f64).min(spare.max(0.0))
        };
        debt += add;
        offset += 1;
        while debt >= 1.0 {
            debt -= 1.0;
            emitted += 1;
        }
        // Cumulative-ack advancement resets the taper phase (the
        // sender's `taper_offset = 0` on window advancement).
        if ack_every > 0 && (i + 1) % ack_every == 0 {
            offset = 0;
        }
    }
    emitted
}

#[test]
fn test_taper_budget_tracks_r_magnitude() {
    // With the per-cycle accrual, r = 0.05 and r = 0.25 emit the same
    // repair (≈ r per ack cycle → cycle-count-sized, not r-sized). The
    // budget law must emit ~5x apart and ≈ r × source.
    let (q, span, n, ack_every) = (0.4, 64, 20_000u64, 200u64);
    let lo = simulate_emission(0.05, q, span, n, ack_every, f64::INFINITY, true);
    let hi = simulate_emission(0.25, q, span, n, ack_every, f64::INFINITY, true);
    // Budget law: emitted ≈ r × n within 15%.
    let (exp_lo, exp_hi) = (0.05 * n as f64, 0.25 * n as f64);
    assert!(
        (lo as f64) > 0.85 * exp_lo && (lo as f64) < 1.15 * exp_lo,
        "budget law must emit ~r x source at r=0.05: {lo} vs {exp_lo}"
    );
    assert!(
        (hi as f64) > 0.85 * exp_hi && (hi as f64) < 1.15 * exp_hi,
        "budget law must emit ~r x source at r=0.25: {hi} vs {exp_hi}"
    );
    let ratio = hi as f64 / lo.max(1) as f64;
    assert!(
        (4.0..=6.0).contains(&ratio),
        "5x the rate must emit ~5x the repair: {ratio:.2}x ({lo} vs {hi})"
    );

    // The per-cycle arm documents the pathology: both rates emit ≈ r per
    // ack cycle (n/ack_every cycles), an order below the budget and
    // nearly invariant in r.
    let lo_legacy = simulate_emission(0.05, q, span, n, ack_every, f64::INFINITY, false);
    let hi_legacy = simulate_emission(0.25, q, span, n, ack_every, f64::INFINITY, false);
    let cycles = (n / ack_every) as f64;
    assert!(
        (hi_legacy as f64) < 1.5 * 0.25 * cycles + 2.0,
        "legacy emits ~r per ack cycle, not r per source: {hi_legacy} vs {} cycles",
        cycles
    );
    assert!(
        (hi as f64) > 10.0 * (hi_legacy.max(1) as f64),
        "the budget law must break the per-ack-cycle ceiling: budget={hi} legacy={hi_legacy}"
    );
}

#[test]
fn test_taper_budget_ack_cadence_invariance() {
    // The budget must be governed by source count, not ack cadence:
    // burst acks (reset every symbol), a BDP-sized cycle (hundreds of
    // symbols), and sparse acks
    // (one endless cycle) must all emit ≈ r × source.
    let (r, q, span, n) = (0.23, 0.4, 64, 20_000u64);
    let expect = r * n as f64;
    for (name, ack_every) in [("burst(1)", 1u64), ("cycle(300)", 300), ("sparse(0)", 0)] {
        let e = simulate_emission(r, q, span, n, ack_every, f64::INFINITY, true);
        assert!(
            (e as f64) > 0.8 * expect && (e as f64) < 1.2 * expect,
            "budget law must emit ~r x source under {name} acks: {e} vs {expect:.0}"
        );
    }
    // Contrast: the per-cycle accrual under burst acks pins the phase at 0 → emits
    // A = r·q per symbol (under), and under sparse acks emits ~r TOTAL.
    let sparse_legacy = simulate_emission(r, q, span, n, 0, f64::INFINITY, false);
    assert!(
        sparse_legacy <= 1,
        "legacy sparse-ack cycle emits ~r total (the pathology): {sparse_legacy}"
    );
}

#[test]
fn test_taper_budget_spare_cap_and_expiry() {
    // Zero spare ⇒ zero grants (the never-hurts anchor is respected)
    // and the banked budget must expire at one window's worth
    // (max(r·W, 1)) instead of accumulating unboundedly.
    let (r, q, span) = (0.25, 0.4, 64usize);
    let taper = TaperFunction {
        amplitude: r * q,
        decay: 1.0 - q,
        total_rate: r,
        q,
    };
    let mut tb = TaperBudget::new();
    for t in 0..10_000u64 {
        let g = tb.accrue(r, t, &taper, span, 0.0);
        assert_eq!(g, 0.0, "no spare ⇒ no grant");
    }
    let cap = (r * span as f64).max(1.0);
    assert!(
        tb.owed() <= cap + 1e-9,
        "starved budget must expire at one window's budget: owed={} cap={cap}",
        tb.owed()
    );

    // When spare returns, the backlog drains paced at <= 1 repair per
    // source send (the source clock), never a burst.
    let mut max_grant = 0.0f64;
    let mut drained = 0.0;
    for t in 0..64u64 {
        let g = tb.accrue(r, t, &taper, span, f64::INFINITY);
        assert!(g <= 1.0 + 1e-9, "grant must never exceed 1 per source send");
        max_grant = max_grant.max(g);
        drained += g;
    }
    assert!(
        drained > cap * 0.9,
        "backlog must drain once spare returns: drained={drained:.2} of cap={cap}"
    );
    assert!(max_grant > r, "frontier drain must front-load above the flat rate");
}

#[test]
fn test_taper_budget_front_loads_at_frontier() {
    // The taper's intent survives: with banked budget, the grant right
    // after a frontier advance (offset 0) exceeds the mid-span grant —
    // repair is still concentrated where it recovers a hole without a
    // round-trip. (Total is budget-governed; only the timing is shaped.)
    let (r, q, span) = (0.10, 0.4, 64usize);
    let taper = TaperFunction {
        amplitude: r * q,
        decay: 1.0 - q,
        total_rate: r,
        q,
    };
    let mut tb = TaperBudget::new();
    // Bank some budget under zero spare.
    for t in 0..40u64 {
        tb.accrue(r, t, &taper, span, 0.0);
    }
    let g_frontier = tb.accrue(r, 0, &taper, span, f64::INFINITY);
    // Re-bank, then read a mid-span grant with the same backlog.
    let mut tb2 = TaperBudget::new();
    for t in 0..40u64 {
        tb2.accrue(r, t, &taper, span, 0.0);
    }
    let g_mid = tb2.accrue(r, 32, &taper, span, f64::INFINITY);
    assert!(
        g_frontier > g_mid,
        "frontier grant must exceed mid-span grant: {g_frontier} vs {g_mid}"
    );
    assert!(
        (g_mid - r).abs() < 1e-9,
        "mid-span drains at the uniform budget rate r: {g_mid}"
    );
}

// --- P_lost tests ---

#[test]
fn test_p_lost_at_zero() {
    // At t=0, P_lost ≈ ε (just the base loss rate)
    let p = p_lost(0.0, 0.025, 0.050, 0.005);
    assert!(
        (p - 0.025).abs() < 0.005,
        "P_lost(0) ≈ ε: got {p}"
    );
}

#[test]
fn test_p_lost_at_srtt() {
    // At t = SRTT, P_lost should be elevated (ACK expected by now)
    let p = p_lost(0.050, 0.025, 0.050, 0.005);
    assert!(
        p > 0.025 * 1.5,
        "P_lost(SRTT) should be well above ε: got {p}"
    );
}

#[test]
fn test_p_lost_at_large_t() {
    // At t >> SRTT, P_lost → 1.0
    let p = p_lost(0.200, 0.025, 0.050, 0.005);
    assert!(
        p > 0.95,
        "P_lost(4×SRTT) should be near 1.0: got {p}"
    );
}

#[test]
fn test_p_lost_monotone() {
    // P_lost should increase with age
    let srtt = 0.050;
    let rttvar = 0.005;
    let eps = 0.025;

    let p1 = p_lost(0.01, eps, srtt, rttvar);
    let p2 = p_lost(0.03, eps, srtt, rttvar);
    let p3 = p_lost(0.05, eps, srtt, rttvar);
    let p4 = p_lost(0.10, eps, srtt, rttvar);

    assert!(p1 < p2, "P_lost should increase: {p1} < {p2}");
    assert!(p2 < p3, "P_lost should increase: {p2} < {p3}");
    assert!(p3 < p4, "P_lost should increase: {p3} < {p4}");
}

#[test]
fn test_p_lost_high_epsilon() {
    // With high loss rate, P_lost(0) should be high
    let p = p_lost(0.0, 0.5, 0.050, 0.005);
    assert!(
        (p - 0.5).abs() < 0.01,
        "P_lost(0) with ε=0.5 should be ~0.5: got {p}"
    );
}

#[test]
fn test_normal_survival_basic() {
    // P(Z > 0) = 0.5
    let s = normal_survival(0.0);
    assert!((s - 0.5).abs() < 0.01, "survival(0) ≈ 0.5: got {s}");

    // P(Z > 2) ≈ 0.0228
    let s2 = normal_survival(2.0);
    assert!((s2 - 0.0228).abs() < 0.005, "survival(2) ≈ 0.023: got {s2}");

    // P(Z > -2) ≈ 0.9772
    let sm2 = normal_survival(-2.0);
    assert!((sm2 - 0.9772).abs() < 0.005, "survival(-2) ≈ 0.977: got {sm2}");
}

// --- Burst variance tests (Phase 5) ---

#[test]
fn test_burst_variance_iid_channel() {
    // For iid channel (p+q ≈ 1), σ²_burst → 1
    // This is the p=0.5, q=0.5 case: 1 + 2*(1-1)/1 = 1
    // We can't easily set GE params on LossEstimator, so test the formula directly
    // via p_fec_normal which uses sigma2_burst parameter
    let p_fec_iid = p_fec_normal(0.15, 0.10, 50.0, 1.0);   // σ²=1 (iid)
    let p_fec_burst = p_fec_normal(0.15, 0.10, 50.0, 3.0);  // σ²=3 (bursty)

    assert!(
        p_fec_iid > p_fec_burst,
        "iid should have higher P_fec than bursty: iid={p_fec_iid}, burst={p_fec_burst}"
    );
}

#[test]
fn test_burst_variance_scenarios() {
    // Reference values (formula: paper §2.4):
    // DC: σ²≈3.0, WiFi: σ²≈2.9, LTE: σ²≈3.8, Satellite: σ²≈5.1
    // Test the formula: σ² = 1 + 2(1-p-q)/(p+q)

    // DC: p=0.001, q=0.5 → 1 + 2*(1-0.501)/0.501 ≈ 2.99
    let s_dc: f64 = 1.0 + 2.0 * (1.0 - 0.001 - 0.5) / (0.001 + 0.5);
    assert!((s_dc - 3.0).abs() < 0.1, "DC σ²≈3.0: got {s_dc}");

    // LTE: p=0.01, q=0.2 → 1 + 2*(1-0.21)/0.21 ≈ 8.5
    // (actual values depend on exact p,q — test formula is correct)
    let s_lte: f64 = 1.0 + 2.0 * (1.0 - 0.01 - 0.2) / (0.01 + 0.2);
    assert!(s_lte > 1.0, "LTE σ² > 1 (bursty): got {s_lte}");
    assert!(s_lte > s_dc, "LTE more bursty than DC");
}

#[test]
fn test_burst_variance_no_ge_data() {
    let est = LossEstimator::new();
    let s = burst_variance_factor(&est);
    assert_eq!(s, 1.0, "No GE data → σ²=1.0 (iid fallback)");
}

// --- P_fec normal approximation tests ---

#[test]
fn test_p_fec_normal_basic() {
    // With r well above ε/(1-ε), P_fec should be high
    let p = p_fec_normal(0.20, 0.10, 50.0, 1.0);
    assert!(p > 0.9, "r=0.20, ε=0.10, W=50 should have high P_fec: {p}");

    // With r barely above ε/(1-ε), P_fec should be moderate
    let p2 = p_fec_normal(0.12, 0.10, 50.0, 1.0);
    assert!(p2 > 0.0 && p2 < p, "Marginal r should give lower P_fec: {p2}");

    // With r below ε/(1-ε), P_fec = 0
    let p3 = p_fec_normal(0.05, 0.10, 50.0, 1.0);
    assert!(p3 < 0.01, "r < ε/(1-ε) should give P_fec ≈ 0: {p3}");
}

#[test]
fn test_p_fec_increases_with_window() {
    // Larger window → tighter concentration → higher P_fec
    let p_small = p_fec_normal(0.15, 0.10, 20.0, 1.0);
    let p_large = p_fec_normal(0.15, 0.10, 200.0, 1.0);
    assert!(
        p_large > p_small,
        "Larger window should increase P_fec: W=20: {p_small}, W=200: {p_large}"
    );
}
