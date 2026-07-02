//! Analytical verification of paper formulas.
//! No randomness — pure deterministic checks.

use raptorpath_math::*;

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "expected {expected} ± {tolerance}, got {actual} (error: {:.4})",
        (actual - expected).abs()
    );
}

// =========================================================================
// 1.1 P_lost concrete examples (Paper Section 3.4)
// =========================================================================

#[test]
fn test_p_lost_paper_examples() {
    // WiFi: ε=0.025, SRTT=50ms, RTTVAR≈5ms
    let eps = 0.025;
    let srtt = 0.050;
    let rttvar = 0.005;

    // t=0: P_lost = ε (just base rate)
    assert_close(p_lost(0.0, eps, srtt, rttvar), 0.025, 0.005);
    // t=50ms (=SRTT): P_lost elevated
    let p_at_srtt = p_lost(0.050, eps, srtt, rttvar);
    assert!(p_at_srtt > 0.03 && p_at_srtt < 0.10, "P_lost(SRTT) = {p_at_srtt}");
    // t=70ms: P_lost high
    assert!(p_lost(0.070, eps, srtt, rttvar) > 0.5, "P_lost(70ms) should be > 0.5");
    // t=80ms: P_lost near 1
    assert!(p_lost(0.080, eps, srtt, rttvar) > 0.9, "P_lost(80ms) should be > 0.9");
    // t >> SRTT: P_lost → 1
    assert!(p_lost(0.200, eps, srtt, rttvar) > 0.99, "P_lost(200ms) should be ~1.0");
}

// =========================================================================
// 1.3 σ²_burst table (Paper Section 8.3)
// =========================================================================

#[test]
fn test_sigma2_burst_table() {
    // DC: p=0.001, q=0.5 → σ²≈3.0
    assert_close(burst_variance_factor(0.001, 0.5), 3.0, 0.1);
    // WiFi: p≈0.013, q=0.5 → σ²≈2.9
    assert_close(burst_variance_factor(0.013, 0.5), 2.9, 0.15);
    // Satellite: p≈0.01, q=0.1 → σ²≈high
    let sat = burst_variance_factor(0.01, 0.1);
    assert!(sat > 4.0, "Satellite σ² should be > 4: {sat}");
    // iid: p+q=1 → σ²=1
    assert_close(burst_variance_factor(0.5, 0.5), 1.0, 0.01);
    // Extreme burst: p+q small → σ² large
    let extreme = burst_variance_factor(0.01, 0.02);
    assert!(extreme > 50.0, "Extreme burst σ² should be large: {extreme}");
}

// =========================================================================
// 1.4 r* worked examples (Paper Section 8.5)
// =========================================================================

#[test]
fn test_r_star_worked_examples() {
    // Paper Section 8.5 (continuous z_{δ/ε} convention): the quantile is
    // taken at 1 - δ/ε, so the margin responds to the ratio δ/ε and the
    // rate decreases continuously to 0 when δ ≥ ε.
    let w = 50.0;
    let (d_bulk, d_auto, d_rt) = (1e-2, 1e-4, 1e-6);

    // DC: ε=0.001, σ²=3.0
    let e = 0.001;
    let s_dc = burst_variance_factor(0.001, 0.5);
    let dc_bulk = compute_r_star_with_z(e, s_dc, w, z_for_tail_target(d_bulk, e));
    let dc_auto = compute_r_star_with_z(e, s_dc, w, z_for_tail_target(d_auto, e));
    let dc_rt = compute_r_star_with_z(e, s_dc, w, z_for_tail_target(d_rt, e));
    assert_eq!(dc_bulk, 0.0, "δ ≥ ε → pure ARQ, r* = 0");
    assert_close(dc_auto, 0.011, 0.002); // paper: 1.1%
    assert_close(dc_rt, 0.025, 0.002);   // paper: 2.5%

    // WiFi: ε=0.025, σ²≈2.9
    let e = 0.025;
    let s_wifi = burst_variance_factor(0.013, 0.5);
    let wifi_bulk = compute_r_star_with_z(e, s_wifi, w, z_for_tail_target(d_bulk, e));
    let wifi_auto = compute_r_star_with_z(e, s_wifi, w, z_for_tail_target(d_auto, e));
    let wifi_rt = compute_r_star_with_z(e, s_wifi, w, z_for_tail_target(d_rt, e));
    assert_close(wifi_bulk, 0.035, 0.003); // paper: 3.5%
    assert_close(wifi_auto, 0.128, 0.005); // paper: 12.8%
    assert_close(wifi_rt, 0.178, 0.005);   // paper: 17.8%
    assert!(wifi_rt > wifi_auto && wifi_auto > wifi_bulk, "Monotone in tail tightness");

    // Satellite: ε=0.09, (p, q) = (0.03, 0.3) from paper Section 2.4.
    // σ² = 1 + 2(1-p-q)/(p+q) = 1 + 2(0.67)/0.33 ≈ 5.06 ≈ paper's 5.1.
    let e = 0.09;
    let s_sat = burst_variance_factor(0.03, 0.3);
    let sat_bulk = compute_r_star_with_z(e, s_sat, w, z_for_tail_target(d_bulk, e));
    let sat_auto = compute_r_star_with_z(e, s_sat, w, z_for_tail_target(d_auto, e));
    let sat_rt = compute_r_star_with_z(e, s_sat, w, z_for_tail_target(d_rt, e));
    assert_close(sat_bulk, 0.222, 0.005); // paper: 22.2%
    assert_close(sat_auto, 0.406, 0.005); // paper: 40.6%
    assert_close(sat_rt, 0.525, 0.005);   // paper: 52.5%
    assert!(sat_rt > sat_auto && sat_auto > sat_bulk, "Monotone in tail tightness");

    // Continuity: r*(δ) decreases smoothly to 0 as δ approaches ε (WiFi)
    let e = 0.025;
    let mut prev = f64::INFINITY;
    let mut reached_zero = false;
    for k in 1..=24 {
        let delta = e * k as f64 / 25.0; // δ sweeps toward ε
        let r = compute_r_star_with_z(e, s_wifi, w, z_for_tail_target(delta, e));
        assert!(r <= prev + 1e-12, "r*(δ) must be nonincreasing in δ: {r} > {prev}");
        if r == 0.0 { reached_zero = true; }
        prev = r;
    }
    assert!(reached_zero, "r* must reach 0 before δ = ε (continuous boundary)");
}

// =========================================================================
// 1.5 z_δ values (normal quantile)
// =========================================================================

#[test]
fn test_z_delta_values() {
    assert_close(normal_quantile(0.99), 2.33, 0.02);
    assert_close(normal_quantile(0.9999), 3.72, 0.02);
    assert_close(normal_quantile(0.999999), 4.75, 0.05);
    assert_close(normal_quantile(0.5), 0.0, 0.01);
    assert!(normal_quantile(0.975) > 1.9 && normal_quantile(0.975) < 2.0);
}

// =========================================================================
// 1.6 P_fec boundary conditions (Paper Section 8.2)
// =========================================================================

#[test]
fn test_p_fec_boundaries() {
    let eps = 0.10;
    let w = 50.0;
    let s2 = 3.0;

    // At r = ε/(1-ε): P_fec ≈ 0.5 (z=0 in the normal approximation)
    // Use slightly above IT minimum to avoid floating point edge
    let r_it = eps / (1.0 - eps) + 0.001;
    let p = p_fec_normal(r_it, eps, w, s2);
    assert!(p > 0.4 && p < 0.7, "P_fec near IT minimum should be ~0.5: {p}");

    // At r >> ε/(1-ε): P_fec → 1.0
    let p_high = p_fec_normal(0.5, eps, w, s2);
    assert!(p_high > 0.99, "P_fec at r=0.5 should be >0.99: {p_high}");

    // At r < ε/(1-ε): P_fec → 0.0
    let p_low = p_fec_normal(0.05, eps, w, s2);
    assert!(p_low < 0.1, "P_fec at r=0.05 should be <0.1: {p_low}");
}

// =========================================================================
// 1.7 Taper geometric series
// =========================================================================

#[test]
fn test_taper_geometric_sum() {
    let rate = 0.15;
    let q = 0.3;
    let taper = TaperFunction::new(rate, q);

    // Sum of τ(t) for t=0..∞ should = A/q = rate
    let sum: f64 = (0..10000).map(|t| taper.density(t as f64)).sum();
    assert_close(sum, rate, 0.001);
}

#[test]
fn test_taper_flat_when_iid() {
    // q=1.0 (iid) → decay=0 → τ(0)=A, τ(1)=0
    let taper = TaperFunction::new(0.1, 1.0);
    assert_close(taper.density(0.0), 0.1, 0.001);
    assert_close(taper.density(1.0), 0.0, 0.001);
}

// =========================================================================
// 1.8 B_max values
// =========================================================================

#[test]
fn test_b_max_values() {
    assert_eq!(b_max(0.5), 14);
    // B_max ≈ 9.2/q for moderate q
    let bm = b_max(0.1);
    assert!((bm as f64 - 9.2 / 0.1).abs() < 5.0, "B_max(0.1)={bm}");
    assert_eq!(b_max(1.0), 1); // edge: q=1 → no persistence
}

// =========================================================================
// 1.9 Three-variable consistency
// =========================================================================

#[test]
fn test_three_var_cycle_consistency() {
    let eps = 0.10;
    let q = 0.3;
    let w = 30.0;
    let s2 = burst_variance_factor(eps * q / (1.0 - eps), q);

    // Mode 1: fix (δ=0.05, ρ=0.999) → compute r
    let m1 = solve_r_from_delta_rho(eps, q, w, s2, 0.05, 0.999);
    // Mode 2: fix (r, ρ=0.999) → compute δ
    let m2 = solve_delta_from_r_rho(eps, q, w, s2, m1.r, 0.999);
    // Should recover same δ
    assert_close(m2.delta, 0.05, 0.01);

    // Mode 3: fix (r, δ=0.05) → compute ρ
    let m3 = solve_rho_from_r_delta(eps, q, w, s2, m1.r, 0.05);
    // Should recover same ρ
    assert_close(m3.rho, 0.999, 0.01);
}

// =========================================================================
// 1.10 Delivery distribution identity
// =========================================================================

#[test]
fn test_delivery_distribution_sums_to_one() {
    for eps in [0.01, 0.05, 0.10, 0.25] {
        for rho in [0.95, 0.99, 0.999, 1.0] {
            let r = eps / (1.0 - eps) + 0.05; // some overhead
            let w = 50.0;
            let s2 = 3.0;
            let p_fec = p_fec_normal(r, eps, w, s2);

            let fec_miss = eps * (1.0 - p_fec);
            let p_arq = if fec_miss > 1e-15 {
                (1.0 - (1.0 - rho) / fec_miss).clamp(0.0, 1.0)
            } else { 1.0 };

            let p_ontime = (1.0 - eps) + eps * p_fec;
            let p_late = fec_miss * p_arq;
            let p_lost = fec_miss * (1.0 - p_arq);
            let total = p_ontime + p_late + p_lost;

            assert_close(total, 1.0, 1e-10);
        }
    }
}

// =========================================================================
// 1.11 Monotonicity checks
// =========================================================================

#[test]
fn test_p_fec_monotone_in_r() {
    let eps = 0.10;
    let w = 50.0;
    let s2 = 3.0;
    let mut prev = 0.0;
    for r_pct in 1..50 {
        let r = r_pct as f64 / 100.0;
        let p = p_fec_normal(r, eps, w, s2);
        assert!(p >= prev, "P_fec should increase with r: r={r}, p={p}, prev={prev}");
        prev = p;
    }
}

#[test]
fn test_p_fec_monotone_in_w() {
    let eps = 0.10;
    let r = 0.15;
    let s2 = 3.0;
    let mut prev = 0.0;
    for w in [5.0, 10.0, 20.0, 50.0, 100.0, 200.0] {
        let p = p_fec_normal(r, eps, w, s2);
        assert!(p >= prev, "P_fec should increase with W: W={w}, p={p}, prev={prev}");
        prev = p;
    }
}

#[test]
fn test_p_lost_monotone_in_t() {
    let eps = 0.05;
    let srtt = 0.050;
    let rttvar = 0.005;
    let mut prev = 0.0;
    for t_ms in 0..200 {
        let t = t_ms as f64 / 1000.0;
        let p = p_lost(t, eps, srtt, rttvar);
        assert!(p >= prev - 1e-10, "P_lost should increase with t: t={t}s, p={p}, prev={prev}");
        prev = p;
    }
}

#[test]
fn test_find_t_cut_monotone_in_rho() {
    let eps = 0.15;
    let q = 0.2;
    let r = 0.25;
    let w = 30.0;
    let s2 = burst_variance_factor(eps * q / (1.0 - eps), q);

    let mut prev = 0.0;
    for rho_pct in [80, 85, 90, 95, 98, 99] {
        let rho = rho_pct as f64 / 100.0;
        let tc = find_t_cut(eps, q, r, w, s2, rho);
        assert!(tc >= prev, "T_cut should increase with ρ: ρ={rho}, T_cut={tc}, prev={prev}");
        prev = tc;
    }
}

// =========================================================================
// 1.12 W_min table (Paper Section 14.5)
// =========================================================================

#[test]
fn test_w_min_table() {
    // W_min ≈ 1 / (q × ε) for mean burst with r = ε/(1-ε)
    // (exact: 1/(q·ε·(1-ε)); the paper table drops the (1-ε) correction).
    // Section 2.4 scenario parameters (paper Section 14.5 table):
    // WiFi: ε=2.5%, q=0.5 → W_min=80
    assert_close(1.0 / (0.5 * 0.025), 80.0, 1.0);
    // LTE: ε=5%, q=0.4 → W_min=50
    assert_close(1.0 / (0.4 * 0.05), 50.0, 1.0);
    // Satellite: ε=9%, q=0.3 → W_min≈37
    assert_close(1.0 / (0.3 * 0.09), 37.0, 1.0);

    // B_99 = ceil(ln(0.01) / ln(1-q))
    let b99_wifi = (0.01_f64.ln() / (1.0 - 0.5_f64).ln()).ceil() as u64;
    assert_eq!(b99_wifi, 7);
    let b99_lte = (0.01_f64.ln() / (1.0 - 0.4_f64).ln()).ceil() as u64;
    assert_eq!(b99_lte, 10);
    let b99_sat = (0.01_f64.ln() / (1.0 - 0.3_f64).ln()).ceil() as u64;
    assert_eq!(b99_sat, 13);
}

// =========================================================================
// 1.13 Exact P_fec via transfer-matrix DP (Paper Section 8.7)
// =========================================================================

#[test]
fn test_p_fec_exact_boundaries() {
    // No loss → always succeeds
    assert!(p_fec_exact(0.0, 0.5, 0.1, 50) > 0.999);
    // Zero window → vacuous success
    assert_eq!(p_fec_exact(0.013, 0.5, 0.1, 0), 1.0);
    // Generous repairs beat sparse repairs (WiFi)
    let hi = p_fec_exact(0.013, 0.5, 0.5, 50);
    let lo = p_fec_exact(0.013, 0.5, 0.04, 50);
    assert!(hi > 0.99, "r=0.5 should nearly always recover: {hi}");
    assert!(hi > lo, "more repairs → higher P_fec: {hi} vs {lo}");
    // Monotone in the repair count (sampled at exact R steps)
    let mut prev = 0.0;
    for repairs in 1..=25 {
        let r = repairs as f64 / 50.0;
        let pf = p_fec_exact(0.013, 0.5, r, 50);
        assert!(pf >= prev - 1e-12, "P_fec must not decrease with repairs: r={r}, {pf} < {prev}");
        prev = pf;
    }
}

#[test]
fn test_p_fec_exact_iid_matches_binomial() {
    // p + q = 1 → memoryless chain (next state independent of current):
    // K ~ Bin(W, ε) and C ~ Bin(R, 1−ε) independent, ε = p/(p+q) = 0.05.
    // The DP must reproduce the independent-Binomial reference exactly.
    let (p, q, r, w) = (0.05, 0.95, 0.12, 50usize);
    let eps = p / (p + q);
    let repairs = (r * w as f64).round() as usize;
    let binom_pmf = |n: usize, k: usize, pr: f64| -> f64 {
        let mut c = 1.0f64;
        for i in 0..k {
            c = c * (n - i) as f64 / (i + 1) as f64;
        }
        c * pr.powi(k as i32) * (1.0 - pr).powi((n - k) as i32)
    };
    let mut reference = 0.0;
    for k in 0..=w {
        let pk = binom_pmf(w, k, eps);
        let psucc: f64 = if k > repairs {
            0.0
        } else {
            (k..=repairs).map(|c| binom_pmf(repairs, c, 1.0 - eps)).sum()
        };
        reference += pk * psucc;
    }
    let exact = p_fec_exact(p, q, r, w);
    assert_close(exact, reference, 1e-9);
}

#[test]
fn test_p_fec_exact_paper_table() {
    // Section 8.7 table values (W=50)
    assert_close(p_fec_exact(0.013, 0.5, 0.10, 50), 0.9522, 0.001);
    assert_close(p_fec_exact(0.02, 0.4, 0.12, 50), 0.8868, 0.001);
    // Sat: R = round(0.25 × 50) = 13 (half rounds away from zero)
    assert_close(p_fec_exact(0.03, 0.3, 0.25, 50), 0.9180, 0.001);
}

#[test]
fn test_r_star_exact_exceeds_normal_on_bursty() {
    // Section 8.7: the closed-form r* under-provisions the tail on bursty
    // channels (Gaussian tail + ignored loss/repair correlation).
    for (p, q) in [(0.013, 0.5), (0.02, 0.4), (0.03, 0.3)] {
        let eps = p / (p + q);
        let s2 = burst_variance_factor(p, q);
        let r_normal = compute_r_star_with_z(eps, s2, 50.0, normal_quantile(0.99));
        let r_exact = compute_r_star_exact(p, q, 50, 0.01);
        assert!(
            r_exact > r_normal,
            "exact r* should exceed the closed form: exact={r_exact}, normal={r_normal}"
        );
    }
    // Specific values from the Section 8.7 table (resolved in 1/W steps)
    assert_close(compute_r_star_exact(0.013, 0.5, 50, 0.01), 0.170, 0.005);
    assert_close(compute_r_star_exact(0.02, 0.4, 50, 0.01), 0.270, 0.005);
    assert_close(compute_r_star_exact(0.03, 0.3, 50, 0.01), 0.450, 0.005);
}
