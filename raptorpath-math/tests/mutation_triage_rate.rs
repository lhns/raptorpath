//! Absolute tests for the rate-law functions, written to kill the surviving
//! mutants listed in docs/derivations/mutation-triage-rate.md (S6).
//!
//! Every assertion pins a law value at an anchor point (closed form, a
//! defining property of the solver, or a stated domain guard), never "A is
//! more than B". Public API only; no engine behaviour is involved.

use raptorpath_math::*;

fn approx(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
}

// ---------------------------------------------------------------- r* ----

#[test]
fn r_star_closed_form_and_domain_guards() {
    let (e, s2, w): (f64, f64, f64) = (0.1, 2.6, 50.0);
    // paper 4.2: r* = e/(1-e) + z sqrt(e s2 / (W (1-e))), z = 2.33
    let want = e / (1.0 - e) + 2.33 * (e * s2 / (w * (1.0 - e))).sqrt();
    approx(compute_r_star(e, s2, w), want, 1e-12, "compute_r_star anchor");
    // eps >= 1 and eps <= 0 are outside the law's domain: 0
    assert_eq!(compute_r_star_with_z(1.0, s2, w, 2.33), 0.0);
    assert_eq!(compute_r_star_with_z(0.0, s2, w, 2.33), 0.0);
    // W = 0: no margin, exactly the information-theoretic base term
    assert_eq!(compute_r_star_with_z(e, s2, 0.0, 2.33), e / (1.0 - e));
    // a very negative z floors at the physical 0
    assert_eq!(compute_r_star_with_z(e, s2, w, -100.0), 0.0);
}

// -------------------------------------------------------- delta dial ----

#[test]
fn zeta_of_delta_at_the_presets() {
    approx(zeta_of_delta(50.0), 0.01, 1e-15, "zeta(Realtime)");
    assert_eq!(zeta_of_delta(0.5), 1.0);
    approx(zeta_of_delta(0.005), 100.0, 1e-9, "zeta(Bulk)");
    // involution: delta(zeta) = delta_Auto / zeta
    for d in [50.0, 5.0, 0.5, 0.05, 0.005] {
        approx(DELTA_AUTO / zeta_of_delta(d), d, d * 1e-12, "involution");
    }
    // clamped, finite at delta = 0
    assert_eq!(zeta_of_delta(0.0), 0.5 / 1e-12);
}

#[test]
fn bulkness_is_the_log_position_between_the_anchors() {
    // beta(d) = (log10(delta_Auto) - log10(d)) / (log10(delta_Auto) - log10(delta_Bulk)), denominator = 2
    let beta = |d: f64| (DELTA_AUTO.log10() - d.log10()) / 2.0;
    for d in [0.4, 0.2, 0.1, 0.05, 0.02, 0.01, 0.007] {
        approx(bulkness_of_delta(d), beta(d), 1e-12, "beta in the open interval");
    }
    approx(bulkness_of_delta(0.05), 0.5, 1e-12, "beta at the log midpoint");
    // the presets, and the clamp outside the dial
    approx(bulkness_of_delta(50.0), 0.0, 0.0, "beta(Realtime)");
    approx(bulkness_of_delta(0.5), 0.0, 0.0, "beta(Auto)");
    approx(bulkness_of_delta(0.005), 1.0, 1e-12, "beta(Bulk)");
    approx(bulkness_of_delta(0.0005), 1.0, 0.0, "beta below Bulk");
}

// ---------------------------------------------------------- sigma2 ------

#[test]
fn burst_variance_factor_anchors_and_guards() {
    // sigma2 = 1 + 2(1-p-q)/(p+q): p = 0.1, q = 0.5 -> 1 + 0.8/0.6
    approx(burst_variance_factor(0.1, 0.5), 1.0 + 2.0 * 0.4 / 0.6, 1e-12, "sigma2 anchor");
    // a zero transition probability is the no-data sentinel: iid
    assert_eq!(burst_variance_factor(0.0, 0.5), 1.0);
    assert_eq!(burst_variance_factor(0.1, 0.0), 1.0);
    // p + q < 1e-10 is iid; at exactly 1e-10 the closed form applies
    assert_eq!(burst_variance_factor(1e-11, 1e-11), 1.0);
    let at = burst_variance_factor(5e-11, 5e-11);
    assert!(at > 1e10, "p + q = 1e-10 exactly is not below the guard: {at}");
}

// ------------------------------------------------------------ P_fec -----

#[test]
fn p_fec_normal_closed_form_and_domain_guards() {
    let (r, e, w, s2): (f64, f64, f64, f64) = (0.3, 0.1, 50.0, 2.6);
    let z = w.sqrt() * (r * (1.0 - e) - e) / (e * (1.0 - e) * (r + s2)).sqrt();
    approx(p_fec_normal(r, e, w, s2), 1.0 - normal_survival(z), 1e-15, "P_fec anchor");
    // each domain violation alone returns 0
    assert_eq!(p_fec_normal(r, e, 0.0, s2), 0.0, "W = 0");
    assert_eq!(p_fec_normal(r, 0.0, w, s2), 0.0, "eps = 0");
    assert_eq!(p_fec_normal(0.0, e, w, s2), 0.0, "r = 0");
    assert_eq!(p_fec_normal(r, 1.0, w, s2), 0.0, "eps = 1");
    // eps >= 1 and r <= 0 together, where the numerator turns positive
    assert_eq!(p_fec_normal(-10.0, 1.5, w, 2.0), 0.0, "eps > 1 and r < 0");
}

#[test]
fn p_fec_exact_anchors_and_search_ties() {
    // no repairs: FEC succeeds iff the single source symbol arrives, 1 - pi_bad
    approx(p_fec_exact(0.1, 1.0, 0.0, 1), 1.0 - 0.1 / 1.1, 1e-12, "no repairs");
    // q = 1: a Bad slot is never followed by Bad, so one repair always covers one loss
    assert_eq!(p_fec_exact(0.1, 1.0, 0.5, 1), 1.0);
    assert_eq!(p_fec_exact(0.1, 1.0, 0.5, 0), 1.0);
    assert_eq!(p_fec_exact(0.0, 1.0, 0.5, 1), 1.0);
    // r* is the least r with P_fail <= delta: the target is MET (not beaten)
    // at fail == delta, so the answer is the first r with one repair, 0.5,
    // not the 2.0 ceiling reserved for an unreachable target.
    assert_eq!(compute_r_star_exact(0.1, 1.0, 1, 0.0), 0.5);
    // domain guards: no window or no loss means no rate
    assert_eq!(compute_r_star_exact(0.1, 1.0, 0, 0.01), 0.0);
    assert_eq!(compute_r_star_exact(0.0, 1.0, 10, 0.01), 0.0);
    // unreachable target returns the 2.0 ceiling
    assert_eq!(compute_r_star_exact(0.5, 0.01, 10, 1e-9), 2.0);
}

// ----------------------------------------------------- P_fec by time ----

#[test]
fn p_fec_recovery_by_time_is_the_poisson_tail() {
    // lambda = T r (1-eps)/(1+r) = 10 * 0.5 * 0.9 / 1.5 = 3
    let e3 = (-3.0f64).exp();
    approx(p_fec_recovery_by_time(10.0, 1, 0.5, 0.3, 0.1), 1.0 - e3, 1e-14, "m = 1");
    approx(p_fec_recovery_by_time(10.0, 2, 0.5, 0.3, 0.1), 1.0 - e3 * (1.0 + 3.0), 1e-14, "m = 2");
    approx(p_fec_recovery_by_time(10.0, 3, 0.5, 0.3, 0.1), 1.0 - e3 * (1.0 + 3.0 + 4.5), 1e-14, "m = 3");
    assert_eq!(p_fec_recovery_by_time(10.0, 0, 0.5, 0.3, 0.1), 1.0);
    // no loss or no repairs: nothing recovered
    assert_eq!(p_fec_recovery_by_time(10.0, 1, 0.5, 0.3, 0.0), 0.0, "eps = 0");
    assert_eq!(p_fec_recovery_by_time(10.0, 1, 0.0, 0.3, 0.1), 0.0, "r = 0");
}

// ------------------------------------------------------------ taper -----

#[test]
fn taper_density_and_generation_decision() {
    let tf = TaperFunction::new(0.6, 0.5); // A = 0.3, decay 0.5
    approx(tf.density(0.0), 0.3, 1e-15, "tau(0)");
    approx(tf.density(2.0), 0.075, 1e-15, "tau(2) = A (1-q)^2");
    let d = tf.density(0.0);
    assert!(tf.should_generate(0.0, d - 1e-9), "rng just below the density generates");
    assert!(!tf.should_generate(0.0, d), "rng equal to the density does not (strict <)");
    assert!(!tf.should_generate(0.0, d + 1e-9));
    assert!(!tf.should_generate(0.0, 0.9));
    // density at or above 1 always generates, even at rng = 1
    let hot = TaperFunction::new(2.0, 0.5); // A = 1.0 exactly
    assert_eq!(hot.density(0.0), 1.0);
    assert!(hot.should_generate(0.0, 1.0));
    let hotter = TaperFunction::new(3.0, 0.5); // A = 1.5
    assert!(hotter.should_generate(0.0, 1.0));
    assert!(!hotter.should_generate(10.0, 0.9), "decayed below 1: the rng decides");
}

// ------------------------------------------------------ soft saturate ---

#[test]
fn soft_saturate_anchors() {
    // rate = r_sat: r_eff = r_sat - s ln 2, s = 0.1 r_sat
    let r_sat = 0.4;
    approx(soft_saturate(r_sat, r_sat), r_sat - SAT_SOFTNESS * r_sat * 2f64.ln(), 1e-14, "at r_sat");
    // degenerate r_sat falls back to the hard min
    assert_eq!(soft_saturate(0.3, 0.0), 0.0);
    assert_eq!(soft_saturate(0.0, 0.0), 0.0);
}

// ------------------------------------------------ r_star_mass (4.3) ----

fn uniform_mass(w0: f64, nz: f64, m1: f64, m2: f64) -> MassStats {
    MassStats { block_scale: w0, nz: [nz; MASS_SCALES], m1: [m1; MASS_SCALES], m2: [m2; MASS_SCALES] }
}

/// The failure-tail bound F(r) that `r_star_mass` searches, restated from
/// its doc comment through the public fit and survival law, in the same
/// arithmetic order.
fn mass_bound(stats: &MassStats, window: f64, level_scale: f64, r: f64) -> f64 {
    let fits = stats.fitted();
    let tail = |idx: usize, x: f64| {
        let nz = stats.nz[idx].clamp(0.0, 1.0);
        if nz <= 0.0 {
            0.0
        } else {
            let (theta, k) = fits[idx];
            nz * burst_tail_survival(theta, k, x)
        }
    };
    let w0 = stats.block_scale;
    let x = window * (1.0 + r) / w0;
    let rr = r * window / level_scale;
    if x <= 1.0 {
        return tail(0, rr);
    }
    let max_m = MASS_SCALES as f64;
    if x >= max_m {
        let c = (x / max_m).ceil();
        return (c * tail(MASS_SCALES - 1, rr / c)).min(1.0);
    }
    let lo = x.floor() as usize - 1;
    let hi = (lo + 1).min(MASS_SCALES - 1);
    let f = x - x.floor();
    (1.0 - f) * tail(lo, rr) + f * tail(hi, rr)
}

#[test]
fn r_star_mass_is_the_least_r_with_bound_at_most_delta() {
    let stats = uniform_mass(64.0, 0.6, 4.0, 60.0);
    let (window, level) = (64.0, 1.0);
    // r* of the bound's own value at r0 is r0 (definition of the least r),
    // at a grid point of the search (step 0.005) ...
    let r_grid = 200.0 * 0.005;
    // ... and at the first bisection midpoint inside the bracket below it.
    let r_mid = 0.5 * (199.0 * 0.005 + 200.0 * 0.005);
    for r0 in [r_grid, r_mid] {
        let delta = mass_bound(&stats, window, level, r0);
        assert!(mass_bound(&stats, window, level, 0.0) > delta);
        assert!(mass_bound(&stats, window, level, R_STAR_TAIL_CEILING) < delta);
        assert_eq!(r_star_mass(&stats, window, delta, level), r0, "r*(F(r0)) = r0 at r0 = {r0}");
    }
}

#[test]
fn r_star_mass_zero_level_scale_is_zero_even_for_chunked_windows() {
    let stats = uniform_mass(64.0, 0.6, 4.0, 60.0);
    // window = 8 w0 takes the chunk + union-bound branch at every r
    assert_eq!(r_star_mass(&stats, 512.0, 0.01, 0.0), 0.0);
    assert_eq!(r_star_mass(&stats, 64.0, 0.01, 0.0), 0.0);
}

// -------------------------------------------------- controller_rate ----

fn base_inputs() -> RateInputs {
    RateInputs {
        p_upper: 0.09,
        sigma2: 5.0,
        mean_burst: 3.3,
        mass: MassStats::default(),
        tail_provision: false,
        window: 64.0,
        t_symbols: 0.0,
        srtt: 0.21,
        t_sym: 0.0,
        codec_overhead: 0.0,
        tail_target: 1e-4,
        bulk_late_is_fine: false,
        completion_exposure: 0.0,
        inner_feedback: 0.0,
        saturation_cap: false,
        max_overhead: 1.5,
    }
}

/// The GE margin term of the rate law, restated from its doc comment.
fn random_term(inp: &RateInputs) -> f64 {
    let z = z_for_tail_target(inp.tail_target, inp.p_upper);
    compute_r_star_with_z(inp.p_upper, inp.sigma2, inp.window, z)
}

#[test]
fn controller_rate_is_the_margin_term_when_nothing_else_binds() {
    let inp = base_inputs();
    let core = random_term(&inp);
    assert!(core > 0.2 && core < 0.5, "anchor operating point: {core}");
    assert_eq!(controller_rate(&inp), core);
}

#[test]
fn controller_rate_p_floor_is_exactly_1e_minus_10() {
    let mut inp = base_inputs();
    inp.sigma2 = 1.0;
    inp.tail_target = 1e-12;
    inp.p_upper = 1e-10;
    let want = random_term(&inp);
    assert!(want > 0.0);
    assert_eq!(controller_rate(&inp), want, "p = 1e-10 is not below the floor");
    inp.p_upper = 0.9e-10;
    assert_eq!(controller_rate(&inp), 0.0);
    inp.p_upper = 1e-11;
    assert_eq!(controller_rate(&inp), 0.0);
    inp.p_upper = 0.0;
    assert_eq!(controller_rate(&inp), 0.0);
}

#[test]
fn controller_rate_codec_overhead_is_weighted_by_p_decoder() {
    let mut inp = base_inputs();
    let core = random_term(&inp);
    inp.codec_overhead = 0.004;
    let p_dec = 1.0 - (1.0 - 0.09f64).powi(64);
    approx(controller_rate(&inp), core + 0.004 * p_dec, 1e-14, "codec term");
    // a negative overhead is not a rebate
    inp.codec_overhead = -0.004;
    assert_eq!(controller_rate(&inp), core);
    // nor is a negative window an overhead
    inp.codec_overhead = 0.004;
    inp.window = -4.0;
    assert_eq!(controller_rate(&inp), compute_r_star_with_z(0.09, 5.0, -4.0, z_for_tail_target(1e-4, 0.09)));
}

#[test]
fn controller_rate_saturation_applies_only_with_cap_t_sym_and_srtt() {
    let t_sym = 1225.0 / 2.5e6;
    let mut inp = base_inputs();
    let core = random_term(&inp);
    let r_sat = r_saturation(0.09, 5.0, 64.0, 0.21, t_sym);
    // all three present: the soft cap, strictly below the request
    inp.saturation_cap = true;
    inp.t_sym = t_sym;
    let capped = controller_rate(&inp);
    assert_eq!(capped, soft_saturate(core, r_sat));
    assert!(capped < core);
    // no cap flag: the request stands
    inp.saturation_cap = false;
    assert_eq!(controller_rate(&inp), core);
    // cap flag but unknown t_sym or srtt: the request stands
    inp.saturation_cap = true;
    inp.t_sym = 0.0;
    assert_eq!(controller_rate(&inp), core, "t_sym = 0 disables the cap");
    inp.t_sym = t_sym;
    inp.srtt = 0.0;
    assert_eq!(controller_rate(&inp), core, "srtt = 0 disables the cap");
}

#[test]
fn controller_rate_mass_term_needs_the_flag_and_the_data() {
    let heavy = uniform_mass(64.0, 0.9, 6.4, 400.0);
    let mut inp = base_inputs();
    inp.p_upper = 0.09;
    inp.mean_burst = 3.3;
    let none = controller_rate(&inp);
    // data but no flag: the term is off
    inp.mass = heavy;
    inp.tail_provision = false;
    assert_eq!(controller_rate(&inp), none);
    // flag and data: the term runs and raises the rate (it executes)
    inp.tail_provision = true;
    let on = controller_rate(&inp);
    assert!(on > none + 1e-6, "mass term must execute: {on} vs {none}");
    // flag but no data: inert
    inp.mass = MassStats::default();
    assert_eq!(controller_rate(&inp), none);
}

// ------------------------------------------ inner_feedback_floor --------

#[test]
fn inner_feedback_floor_solves_its_defining_equation() {
    // S(r) = p q_hat T_arq (1 - C(r)), floor = least r with S(r) <= theta (paper 4.4)
    let check = |p: f64, mean_burst: f64, srtt: f64, t_sym: f64, cap: f64| {
        let q_hat = (1.0 / mean_burst.max(1.0)).clamp(0.01, 1.0);
        let l_stall = (1.5 * srtt).min(0.2f64.max(srtt));
        let t_arq = l_stall / t_sym;
        let theta = (srtt / 4.0) / l_stall;
        let stall = |r: f64| p * q_hat * t_arq * (1.0 - p_fec_recovery_marginalized(t_arq, r, q_hat, p));
        let floor = inner_feedback_floor(p, mean_burst, srtt, t_sym, cap);
        assert!(floor > 0.0 && floor < cap, "interior solution: {floor}");
        assert!(stall(floor) <= theta, "S(floor) <= theta");
        assert!(stall(floor - 1e-6) > theta, "and floor is the least such r");
    };
    check(0.026, 2.0, 0.013, 1200.0 * 8.0 / 100e6, 0.5);
    check(0.026, 4.0, 0.013, 1200.0 * 8.0 / 100e6, 0.5);
    check(0.05, 3.0, 0.02, 2e-4, 0.5);
}

#[test]
fn inner_feedback_floor_domain_guards() {
    let (mb, srtt, ts) = (2.0, 0.013, 1200.0 * 8.0 / 100e6);
    // p = 1 is outside the domain (the floor is the repair rate for loss < 1)
    assert_eq!(inner_feedback_floor(1.0, mb, srtt, ts, 0.5), 0.0);
    assert_eq!(inner_feedback_floor(0.0, mb, srtt, ts, 0.5), 0.0);
    assert_eq!(inner_feedback_floor(0.026, mb, srtt, ts, 0.0), 0.0);
    assert_eq!(inner_feedback_floor(0.026, mb, 0.0, ts, 0.5), 0.0);
    assert_eq!(inner_feedback_floor(0.026, mb, srtt, 0.0, 0.5), 0.0);
}

// ----------------------------------------------------- compute_delta ----

#[test]
fn compute_delta_closed_form_and_guards() {
    let (e, r, w, s2) = (0.1, 0.15, 50.0, 2.6);
    let fm = e * (1.0 - p_fec_normal(r, e, w, s2));
    assert!(fm > 0.02 && fm < 0.04, "anchor miss probability {fm}");
    // rho = 1: delta = fec_miss
    approx(compute_delta(e, r, 1.0, w, s2), fm, 1e-15, "rho = 1");
    // rho < 1 with an ARQ share: delta = fm (1 - (1-rho)/fm) / rho = (fm - 1 + rho)/rho
    let rho = 0.98;
    let want = fm * (1.0 - (1.0 - rho) / fm) / rho;
    approx(compute_delta(e, r, rho, w, s2), want, 1e-15, "rho = 0.98");
    approx(want, (fm - 1.0 + rho) / rho, 1e-12, "algebra");
    // guards: no loss or no reliability target is 0, not NaN
    assert_eq!(compute_delta(0.0, r, 1.0, w, s2), 0.0);
    assert_eq!(compute_delta(e, r, 0.0, w, s2), 0.0);
    assert_eq!(compute_delta(e, r, -0.5, w, s2), 0.0);
    // P_fec = 1: no miss, exactly 0 (also at rho = 1, where 0/0 would be NaN)
    assert_eq!(compute_delta(0.05, 1.0, 1.0, 100.0, 1.0), 0.0);
    // a miss probability below 1e-15 is rounded to 0
    let (e2, r2) = (0.05, 0.25);
    let fm2 = e2 * (1.0 - p_fec_normal(r2, e2, 100.0, 1.0));
    assert!(fm2 > 0.0 && fm2 < 1e-15, "sub-floor miss {fm2}");
    assert_eq!(compute_delta(e2, r2, 1.0, 100.0, 1.0), 0.0);
}

// ----------------------------------------------- the three-variable solvers

#[test]
fn solve_r_from_delta_rho_meets_the_target_at_the_least_r() {
    let (e, q, w, s2) = (0.05, 0.5, 100.0, 1.0);
    for (delta, rho) in [(0.01, 1.0), (0.002, 0.999)] {
        let r = solve_r_from_delta_rho(e, q, w, s2, delta, rho).r;
        assert!(compute_delta(e, r, rho, w, s2) <= delta, "delta met at r = {r}");
        assert!(compute_delta(e, r - 2e-6, rho, w, s2) > delta, "and not 2e-6 below it");
    }
}

#[test]
fn solve_r_from_delta_rho_loose_target_returns_the_information_floor() {
    // delta >= everything achievable: r is the IT minimum eps/(1-eps) plus the
    // bisection bracket left at the 1e-6 stop, (2 - lo)/2^21 after 21 halvings.
    let (e, q, w, s2) = (0.05, 0.5, 100.0, 1.0);
    let lo = e / (1.0 - e);
    let r = solve_r_from_delta_rho(e, q, w, s2, 1.0, 1.0).r;
    approx(r, lo + (2.0 - lo) / 2f64.powi(21), 1e-12, "loose target");
    assert!(r >= lo, "never below the information-theoretic minimum");
}

#[test]
fn solve_r_from_delta_rho_zero_target_needs_p_fec_one() {
    // delta = 0: the least r whose miss probability rounds to 0, well below the 2.0 ceiling
    let (e, q, w, s2) = (0.05, 0.5, 100.0, 1.0);
    let r = solve_r_from_delta_rho(e, q, w, s2, 0.0, 1.0).r;
    assert!(r < 0.3, "{r}");
    assert_eq!(compute_delta(e, r, 1.0, w, s2), 0.0);
    assert!(compute_delta(e, r - 2e-6, 1.0, w, s2) > 0.0);
}

#[test]
fn solve_rho_from_r_delta_matches_the_closed_form() {
    // for rho > 1 - fec_miss: delta = (fm - 1 + rho)/rho, so rho = (1 - fm)/(1 - delta)
    let (e, q, w, s2, r) = (0.1, 0.5, 50.0, 2.6, 0.15);
    let fm = e * (1.0 - p_fec_normal(r, e, w, s2));
    for delta in [0.0, 0.02] {
        let want = (1.0 - fm) / (1.0 - delta);
        let got = solve_rho_from_r_delta(e, q, w, s2, r, delta).rho;
        assert!(got <= want + 1e-12 && want - got < 1e-6, "rho {got} vs {want} at delta {delta}");
    }
}

#[test]
fn solve_rho_from_r_delta_loose_target_returns_the_upper_bound() {
    // delta = 1: every rho meets it; the answer is the cap 1 - 1e-12 less the
    // bracket left at the 1e-6 stop, (hi - 0.5)/2^19 after 19 halvings.
    let hi = 1.0 - 1e-12;
    let got = solve_rho_from_r_delta(0.1, 0.5, 50.0, 2.6, 0.15, 1.0).rho;
    approx(got, hi - (hi - 0.5) / 2f64.powi(19), 1e-13, "loose target");
    assert!(got < 1.0 - 1e-12);
}

#[test]
fn solve_r_from_time_budget_meets_the_target_at_the_least_r() {
    // srtt = 50 s: the ARQ share has not arrived by T = 20 s, so FEC alone must deliver
    let (e, q, t, rho, srtt) = (0.1, 0.5, 20.0, 0.97, 50.0);
    let r = solve_r_from_time_budget(e, q, t, rho, srtt);
    assert!(p_delivered_by_time(t, e, q, r, srtt) >= rho);
    assert!(p_delivered_by_time(t, e, q, r - 2e-6, srtt) < rho);
    assert!(r > e / (1.0 - e) && r < 0.2, "{r}");
}

#[test]
fn solve_r_from_time_budget_loose_target_returns_the_information_floor() {
    let (e, q, t, srtt) = (0.1, 0.5, 20.0, 50.0);
    let lo = e / (1.0 - e);
    let r = solve_r_from_time_budget(e, q, t, 0.0, srtt);
    approx(r, lo + (2.0 - lo) / 2f64.powi(21), 1e-12, "loose target");
}

#[test]
fn solve_r_from_time_budget_target_met_exactly_is_met() {
    // eps = 0.5 and the ARQ deadline inside the budget: P(delivered) = 1.0
    // exactly for every r, so rho = 1 is met (>=) at the floor 1.0, not
    // pushed to the 2.0 ceiling.
    let r = solve_r_from_time_budget(0.5, 0.5, 10.0, 1.0, 1.0);
    approx(r, 1.0 + 1.0 / 2f64.powi(20), 1e-12, "met-at-floor");
}

// ----------------------------------------- FecRateController wiring -----

fn ge_free_estimator() -> LossEstimator {
    // count-only updates: the Gilbert-Elliott estimator stays invalid
    let mut est = LossEstimator::new();
    for i in 0..50 {
        est.record_counts(100, 90, i);
    }
    assert!(!est.ge_estimator().is_valid());
    est
}

/// sigma2 as the controller derives it with no GE data (q = 0.5, p = eps q/(1-eps)).
fn fallback_sigma2(eps: f64) -> f64 {
    let q = 0.5;
    let p_gb = eps * q / (1.0 - eps);
    1.0 + 2.0 * (1.0 - p_gb - q) / (p_gb + q)
}

#[test]
fn controller_without_ge_data_uses_the_iid_fallback_transition() {
    let est = ge_free_estimator();
    let eps = est.loss_rate().max(1e-6);
    assert!(eps > 0.09 && eps < 0.1);
    let w = 50usize;
    let (delta, rho) = (0.01, 1.0);
    let want_r = solve_r_from_delta_rho(eps, 0.5, w as f64, fallback_sigma2(eps), delta, rho).r;
    let mode = TriangleMode::ComputeR { delta, rho };

    // no codec overhead, no burst floor (GE invalid): the rate is the solver's r
    let ctrl = FecRateController::new(1.0, 0.0);
    assert_eq!(ctrl.compute_repair_rate(&est, &mode, w), want_r);
    assert_eq!(ctrl.compute_triangle(&est, &mode, w).r, want_r);

    // codec overhead weighted by P(decoder invoked)
    let p_dec = 1.0 - (1.0 - eps).powi(w as i32);
    let with_codec = FecRateController::new(1.0, 0.004);
    approx(with_codec.compute_repair_rate(&est, &mode, w), want_r + 0.004 * p_dec, 1e-14, "codec");
    // a negative overhead is not a rebate
    let neg = FecRateController::new(1.0, -0.004);
    assert_eq!(neg.compute_repair_rate(&est, &mode, w), want_r);
    // max_overhead clamps
    let tight = FecRateController::new(0.1, 0.0);
    assert_eq!(tight.compute_repair_rate(&est, &mode, w), 0.1);

    // the other two modes read the same eps, q, sigma2
    let s2 = fallback_sigma2(eps);
    let m2 = TriangleMode::ComputeDelta { r: 0.2, rho: 1.0 };
    approx(ctrl.compute_triangle(&est, &m2, w).delta, compute_delta(eps, 0.2, 1.0, w as f64, s2), 1e-15, "delta");
    let m3 = TriangleMode::ComputeRho { r: 0.2, delta: 0.01 };
    assert_eq!(ctrl.compute_triangle(&est, &m3, w).rho, solve_rho_from_r_delta(eps, 0.5, w as f64, s2, 0.2, 0.01).rho);
}
