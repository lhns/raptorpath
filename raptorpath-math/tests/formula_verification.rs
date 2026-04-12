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
    let w = 50.0;

    // z_δ values: Bulk=2.33, Auto=3.72, Realtime=4.75
    let z_bulk = normal_quantile(0.99);
    let z_auto = normal_quantile(0.9999);
    let z_rt = normal_quantile(0.999999);

    // DC: ε=0.001, σ²=3.0
    let s_dc = burst_variance_factor(0.001, 0.5);
    let dc_bulk = compute_r_star_with_z(0.001, s_dc, w, z_bulk);
    let dc_auto = compute_r_star_with_z(0.001, s_dc, w, z_auto);
    let dc_rt = compute_r_star_with_z(0.001, s_dc, w, z_rt);
    assert_close(dc_bulk, 0.019, 0.005);  // paper: 1.9%
    assert_close(dc_auto, 0.030, 0.005);  // paper: 3.0%
    assert_close(dc_rt, 0.038, 0.005);    // paper: 3.8%

    // WiFi: ε=0.025, σ²≈2.9
    // NOTE: Paper Section 8.5 has arithmetic errors in WiFi/Satellite examples.
    // The formula r* = ε/(1-ε) + z√(ε×σ²/(W×(1-ε))) gives different values
    // than the paper's claimed percentages. We verify against the FORMULA, not
    // the paper's incorrect worked examples.
    let s_wifi = burst_variance_factor(0.013, 0.5);
    let wifi_bulk = compute_r_star_with_z(0.025, s_wifi, w, z_bulk);
    let wifi_auto = compute_r_star_with_z(0.025, s_wifi, w, z_auto);
    let wifi_rt = compute_r_star_with_z(0.025, s_wifi, w, z_rt);
    // Verify formula: base = 0.025/0.975 = 0.0256
    // margin(bulk) = 2.33 × √(0.025 × 2.9 / (50 × 0.975)) = 2.33 × 0.0386 = 0.0899
    // total(bulk) = 0.0256 + 0.0899 = 0.1155
    let expected_base = 0.025 / 0.975;
    let expected_margin = z_bulk * (0.025 * s_wifi / (w * 0.975)).sqrt();
    assert_close(wifi_bulk, expected_base + expected_margin, 0.001);
    // Monotonicity: rt > auto > bulk
    assert!(wifi_rt > wifi_auto && wifi_auto > wifi_bulk, "Monotone in z_δ");

    // Satellite: ε=0.09, σ²=5.1 (paper's value — verify against formula)
    // NOTE: The paper's (p, q) for satellite give σ² much higher than 5.1.
    // This is another paper discrepancy — the σ² table values may use
    // different (p, q) than the stationary ε=p/(p+q)=0.09 implies.
    // We verify using the paper's stated σ²=5.1 directly.
    let s_sat = 5.1; // paper's value
    let sat_bulk = compute_r_star_with_z(0.09, s_sat, w, z_bulk);
    let sat_auto = compute_r_star_with_z(0.09, s_sat, w, z_auto);
    let sat_rt = compute_r_star_with_z(0.09, s_sat, w, z_rt);
    // Verify formula: base = 0.09/0.91 = 0.0989
    // margin(bulk) = 2.33 × √(0.09 × 5.1 / (50 × 0.91)) = 2.33 × √(0.01009) = 2.33 × 0.1005 = 0.234
    // total = 0.0989 + 0.234 = 0.333 — NOT 17.3% as paper claims!
    // Paper Section 8.5 has systematic arithmetic errors in the margin calculation.
    let expected_base = 0.09 / 0.91;
    let expected_margin = z_bulk * (0.09 * s_sat / (w * 0.91)).sqrt();
    assert_close(sat_bulk, expected_base + expected_margin, 0.001);
    assert!(sat_rt > sat_auto && sat_auto > sat_bulk, "Monotone in z_δ");
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
    // W_min = 1 / (q × ε) for mean burst with r = ε/(1-ε)
    // WiFi: ε=5%, q=0.5 → W_min=40
    assert_close(1.0 / (0.5 * 0.05), 40.0, 1.0);
    // LTE: ε=10%, q=0.2 → W_min=50
    assert_close(1.0 / (0.2 * 0.10), 50.0, 1.0);
    // Satellite: ε=9%, q=0.1 → W_min=111
    assert_close(1.0 / (0.1 * 0.09), 111.1, 1.0);

    // B_99 = ceil(ln(0.01) / ln(1-q))
    let b99_wifi = (0.01_f64.ln() / (1.0 - 0.5_f64).ln()).ceil() as u64;
    assert_eq!(b99_wifi, 7);
    let b99_lte = (0.01_f64.ln() / (1.0 - 0.2_f64).ln()).ceil() as u64;
    assert_eq!(b99_lte, 21);
    let b99_sat = (0.01_f64.ln() / (1.0 - 0.1_f64).ln()).ceil() as u64;
    assert_eq!(b99_sat, 44);
}
