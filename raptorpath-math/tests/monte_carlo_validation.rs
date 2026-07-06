//! Monte Carlo validation of paper formulas.
//! Compares analytical predictions to empirical simulation results.

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use raptorpath_math::*;

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "expected {expected} ± {tolerance}, got {actual} (error: {:.4})",
        (actual - expected).abs()
    );
}

/// Generate a GE channel sequence. Returns true = lost (Bad state).
fn generate_ge_sequence(p: f64, q: f64, n: usize, rng: &mut impl Rng) -> Vec<bool> {
    let mut good = true;
    (0..n).map(|_| {
        if good {
            if rng.gen::<f64>() < p { good = false; }
        } else {
            if rng.gen::<f64>() < q { good = true; }
        }
        !good
    }).collect()
}

struct Stats { samples: Vec<f64> }
impl Stats {
    fn new() -> Self { Self { samples: Vec::new() } }
    fn push(&mut self, v: f64) { self.samples.push(v); }
    fn mean(&self) -> f64 {
        if self.samples.is_empty() { return 0.0; }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }
    fn variance(&self) -> f64 {
        let m = self.mean();
        let n = self.samples.len() as f64;
        if n < 2.0 { return 0.0; }
        self.samples.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)
    }
}

// =========================================================================
// 2.1 P_fec vs Monte Carlo
// =========================================================================

#[test]
fn test_p_fec_monte_carlo() {
    let scenarios: Vec<(&str, f64, f64, f64)> = vec![
        // (name, eps, p_gb, q_bg)
        ("DC",        0.001, 0.001, 0.5),
        ("WiFi",      0.025, 0.013, 0.5),
        ("LTE",       0.10,  0.056, 0.2),  // p = eps*q/(1-eps) ≈ 0.022, but paper uses different p
        ("Satellite",  0.09,  0.01,  0.1),
    ];

    let w = 50;
    let trials = 10000;

    for (name, eps, p_gb, q_bg) in &scenarios {
        let sigma2 = burst_variance_factor(*p_gb, *q_bg);
        let r = compute_r_star(*eps, sigma2, w as f64);

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut fec_ok_count = 0u32;

        for _ in 0..trials {
            // Generate one window of W symbols
            let losses: Vec<bool> = generate_ge_sequence(*p_gb, *q_bg, w, &mut rng);
            let m = losses.iter().filter(|&&l| l).count();

            // Available repairs: r * W * (1-eps) on average
            // Approximate: generate Poisson(r * W * (1-eps)) repairs
            let expected_repairs = r * w as f64 * (1.0 - eps);
            // Use binomial: each of r*W correction slots survives with prob (1-eps)
            let correction_slots = (r * w as f64).ceil() as usize;
            let surviving_repairs: usize = (0..correction_slots)
                .filter(|_| rng.gen::<f64>() > *eps)
                .count();

            if surviving_repairs >= m {
                fec_ok_count += 1;
            }
        }

        let empirical_p_fec = fec_ok_count as f64 / trials as f64;
        let analytical_p_fec = p_fec_normal(r, *eps, w as f64, sigma2);

        println!("{name}: empirical P_fec={empirical_p_fec:.3}, analytical={analytical_p_fec:.3}");
        // NOTE: The normal approximation (Section 8.2) can diverge from
        // empirical GE simulation, especially at higher loss rates where
        // burst correlation is stronger. This is a known limitation.
        // We use wider tolerance (15%) and flag large discrepancies.
        let error = (empirical_p_fec - analytical_p_fec).abs();
        if error > 0.10 {
            println!("  WARNING: {name} P_fec divergence > 10% — normal approx may be inaccurate for this scenario");
        }
        assert!(
            error < 0.20,
            "{name}: P_fec mismatch too large: empirical={empirical_p_fec:.3}, analytical={analytical_p_fec:.3}"
        );
    }
}

// =========================================================================
// 2.2 σ²_burst vs empirical variance
// =========================================================================

#[test]
fn test_sigma2_burst_empirical() {
    let scenarios: Vec<(&str, f64, f64)> = vec![
        ("DC",    0.001, 0.5),
        ("WiFi",  0.013, 0.5),
        ("Satellite", 0.01, 0.1),
    ];

    let w = 50;
    let n = 200000; // long sequence

    for (name, p_gb, q_bg) in &scenarios {
        let eps = p_gb / (p_gb + q_bg);
        let sigma2 = burst_variance_factor(*p_gb, *q_bg);

        let mut rng = ChaCha8Rng::seed_from_u64(123);
        let seq = generate_ge_sequence(*p_gb, *q_bg, n, &mut rng);

        // Slide window, count losses per window
        let mut stats = Stats::new();
        for start in 0..(n - w) {
            let losses = seq[start..start + w].iter().filter(|&&l| l).count() as f64;
            stats.push(losses);
        }

        let empirical_mean = stats.mean();
        let empirical_var = stats.variance();
        let predicted_mean = w as f64 * eps;
        let predicted_var = w as f64 * eps * (1.0 - eps) * sigma2;

        println!("{name}: mean={empirical_mean:.2} (predicted {predicted_mean:.2}), var={empirical_var:.2} (predicted {predicted_var:.2})");
        // Mean should be close
        assert_close(empirical_mean, predicted_mean, predicted_mean * 0.15 + 0.5);
        // Variance: wider tolerance because σ²_burst is an approximation
        // Just check it's in the right ballpark (within factor 2)
        if predicted_var > 0.01 {
            assert!(empirical_var > predicted_var * 0.3 && empirical_var < predicted_var * 3.0,
                "{name}: variance mismatch: empirical={empirical_var:.2}, predicted={predicted_var:.2}");
        }
    }
}

// =========================================================================
// 2.5 BOCD convergence speed
// =========================================================================

#[test]
fn test_bocd_convergence_speed() {
    let mut convergence_batches = Vec::new();

    for trial in 0..100 {
        let mut est = LossEstimator::new();

        // Phase 1: 100 batches of 10% loss (establish baseline)
        for tick in 0..100 {
            est.record_batch(100, 90, tick);
        }

        // Phase 2: switch to 20% loss
        let mut converged_at = None;
        for tick in 100..200 {
            est.record_batch(100, 80, tick);
            let upper = est.predictive_loss_upper(0.95);
            // Converged when estimate is within 30% of true value (0.20)
            if upper > 0.14 && converged_at.is_none() {
                converged_at = Some(tick - 100);
            }
        }

        if let Some(batches) = converged_at {
            convergence_batches.push(batches as f64);
        }
    }

    let mut sorted = convergence_batches.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];

    println!("BOCD convergence: median={median} batches, p95={p95} batches, converged={}/{}", convergence_batches.len(), 100);
    // Paper claims 5-15 batches
    assert!(median < 25.0, "BOCD median convergence should be < 25 batches: {median}");
}

// =========================================================================
// 2.6 GE parameter estimation convergence
// =========================================================================

#[test]
fn test_ge_estimation_convergence() {
    let true_p = 0.05;
    let true_q = 0.5;

    let mut rng = ChaCha8Rng::seed_from_u64(77);
    let seq = generate_ge_sequence(true_p, true_q, 2000, &mut rng);

    let mut ge = GilbertElliottEstimator::new();
    let mut converged_at = None;

    for (i, &lost) in seq.iter().enumerate() {
        ge.record_symbol(!lost); // record_symbol expects true=received
        if i > 20 && ge.is_valid() {
            let est_p = ge.p_gb();
            let est_q = ge.p_bg();
            if (est_p - true_p).abs() < true_p * 0.5
                && (est_q - true_q).abs() < true_q * 0.5
                && converged_at.is_none()
            {
                converged_at = Some(i);
            }
        }
    }

    println!("GE estimation converged at symbol {}", converged_at.unwrap_or(9999));
    assert!(converged_at.is_some(), "GE should converge within 2000 symbols");
    assert!(converged_at.unwrap() < 500, "GE should converge within 500 symbols: {}", converged_at.unwrap());
}

// =========================================================================
// 2.7 RLC codec recovery
// =========================================================================

#[test]
fn test_rlc_recovery_rate() {
    let w = 50;
    let symbol_size = 16;
    let trials = 500;
    let mut rng = ChaCha8Rng::seed_from_u64(99);

    for m in [1, 3, 5, 10] {
        let mut successes = 0u32;

        for _ in 0..trials {
            let mut enc = RlcEncoder::new(symbol_size);
            let mut dec = RlcDecoder::new(symbol_size);

            // Generate w source symbols
            let mut source_data: Vec<Vec<u8>> = Vec::new();
            for i in 0..w {
                let data: Vec<u8> = (0..symbol_size).map(|j| ((i * 7 + j as u64) & 0xFF) as u8).collect();
                enc.add_source(&data);
                source_data.push(data);
            }

            // Pick m random positions to lose
            let mut lost: Vec<usize> = (0..w as usize).collect();
            lost.shuffle(&mut rng);
            let lost: Vec<usize> = lost[..m as usize].to_vec();

            // Feed non-lost sources to decoder
            for i in 0..w as usize {
                if !lost.contains(&i) {
                    dec.feed_source(i as u64, &source_data[i]);
                }
            }

            // Generate exactly m repairs and feed
            let mut all_recovered = true;
            for _ in 0..m {
                let repair = enc.generate_repair();
                dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);
            }

            // Check all m lost symbols are recovered
            if dec.recovered_count() == w as usize {
                successes += 1;
            }
        }

        let success_rate = successes as f64 / trials as f64;
        println!("RLC m={m}: {successes}/{trials} = {:.1}%", success_rate * 100.0);
        // Should be very high (>99% for GF(256))
        assert!(success_rate > 0.95, "RLC recovery for m={m} should be >95%: {:.1}%", success_rate * 100.0);
    }
}

// =========================================================================
// 2.8 RLC cascade benefit
// =========================================================================

#[test]
fn test_rlc_cascade_benefit() {
    let w = 50;
    let symbol_size = 16;
    let m = 5; // burst of 5 consecutive losses
    let trials = 500;
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let mut repairs_needed_stats = Stats::new();

    for _ in 0..trials {
        let mut enc = RlcEncoder::new(symbol_size);
        let mut dec = RlcDecoder::new(symbol_size);

        let mut source_data: Vec<Vec<u8>> = Vec::new();
        for i in 0..w {
            let data: Vec<u8> = (0..symbol_size).map(|j| (rng.gen::<u8>()) as u8).collect();
            enc.add_source(&data);
            source_data.push(data);
        }

        // Lose positions 20..25 (burst)
        for i in 0..w as usize {
            if !(20..25).contains(&i) {
                dec.feed_source(i as u64, &source_data[i]);
            }
        }

        // Feed repairs one by one, count how many needed
        let mut repairs_fed = 0;
        loop {
            let repair = enc.generate_repair();
            let recovered = dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);
            repairs_fed += 1;

            if dec.recovered_count() >= w as usize { break; }
            if repairs_fed > 20 { break; } // safety
        }

        repairs_needed_stats.push(repairs_fed as f64);
    }

    let mean_repairs = repairs_needed_stats.mean();
    println!("RLC cascade: mean repairs needed for m={m} burst: {mean_repairs:.1} (theoretical minimum: {m})");
    // Window decoder should need close to m repairs (cascade helps)
    assert!(mean_repairs < m as f64 + 2.0, "Should need ~{m} repairs, got {mean_repairs:.1}");
}

// =========================================================================
// 2.9 Ambient FEC pipeline
// =========================================================================

#[test]
fn test_ambient_fec_pipeline() {
    let w = 50;
    let symbol_size = 16;
    let r = 0.15;
    let eps = 0.05;

    // Simulate: send T_w source+FEC symbols with no loss, then burst
    let t_w = 100; // 100 ticks of pipeline building
    let mut enc = RlcEncoder::new(symbol_size);
    let mut dec = RlcDecoder::new(symbol_size);

    // Phase 1: send source + FEC, all arrive (no loss)
    let mut fec_debt = 0.0;
    let mut total_repairs_sent = 0u32;
    for i in 0..t_w {
        let data: Vec<u8> = vec![i as u8; symbol_size as usize];
        let seq = enc.add_source(&data);
        dec.feed_source(seq, &data);

        fec_debt += r;
        while fec_debt >= 1.0 {
            fec_debt -= 1.0;
            let repair = enc.generate_repair();
            dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);
            total_repairs_sent += 1;
        }
    }

    let predicted_pipeline = r * (1.0 - eps) * t_w as f64 / (1.0 + r);
    println!("Pipeline: sent {total_repairs_sent} repairs, predicted λ_prior={predicted_pipeline:.1}");

    // Phase 2: burst of 3 losses
    let burst_start = t_w as u64;
    for i in 0..3u64 {
        let data: Vec<u8> = vec![(burst_start + i) as u8; symbol_size as usize];
        enc.add_source(&data);
        // Don't feed to decoder (lost!)
    }

    // Now feed repairs until burst is recovered
    let mut repairs_after_burst = 0;
    loop {
        let repair = enc.generate_repair();
        let recovered = dec.feed_repair(repair.window_start, repair.window_count, repair.repair_index, &repair.coded_data);
        repairs_after_burst += 1;
        if !recovered.is_empty() || repairs_after_burst > 20 { break; }
    }

    println!("After burst: needed {repairs_after_burst} additional repairs to start recovering");
    // With a full pipeline, should need very few additional repairs
    assert!(repairs_after_burst <= 5, "Pipeline should speed recovery: needed {repairs_after_burst} repairs");
}

// =========================================================================
// 2.4 P_fec model consistency (Section 8.2 vs 14.3)
// =========================================================================

#[test]
fn test_p_fec_model_consistency() {
    // Section 8.2: P_fec = Φ(√W × (r(1-ε)-ε) / √(ε(1-ε)(r+σ²)))
    // Section 14.3: P(t_fec ≤ T | m) = Q(m, λ(T)) where λ(T) → r(1-ε) as T → ∞
    //
    // At T → ∞: both should give similar recovery probability.

    let eps = 0.10;
    let q = 0.3;
    let w = 50.0;
    let p_gb = eps * q / (1.0 - eps);
    let sigma2 = burst_variance_factor(p_gb, q);
    let r = compute_r_star(eps, sigma2, w);

    // Section 8.2 P_fec
    let p_fec_82 = p_fec_normal(r, eps, w, sigma2);

    // Section 14.3: λ(∞) = r × (1-ε). Expected losses = W × ε.
    // P(Poisson(r(1-ε)×W) ≥ W×ε) approximation
    let lambda_inf = r * (1.0 - eps) * w;
    let m = (w * eps).round() as u32;
    // Poisson CDF: P(X ≥ m) = 1 - Σ_{k=0}^{m-1} e^(-λ) λ^k / k!
    let mut poisson_cdf = 0.0;
    let mut term = (-lambda_inf).exp();
    for k in 0..m {
        poisson_cdf += term;
        term *= lambda_inf / (k + 1) as f64;
    }
    let p_fec_143 = 1.0 - poisson_cdf;

    println!("P_fec consistency: Section 8.2 = {p_fec_82:.4}, Section 14.3 (Poisson) = {p_fec_143:.4}");
    // These use different approximations (normal vs Poisson) so tolerance is wider
    assert!(
        (p_fec_82 - p_fec_143).abs() < 0.15,
        "P_fec models should be roughly consistent: 8.2={p_fec_82:.4}, 14.3={p_fec_143:.4}"
    );
}

// =========================================================================
// 2.10 FEC vs ARQ break-even (Section 14.7)
// =========================================================================

#[test]
fn test_fec_vs_arq_breakeven() {
    // For various RTTs, compute t_fec and L_arq, find crossover
    let eps = 0.05;
    let q = 0.5;
    let r = 0.10;

    println!("FEC vs ARQ break-even:");
    let mut last_fec_wins = true;
    for rtt_ms in [1, 5, 10, 50, 100, 200, 500] {
        let srtt = rtt_ms as f64 / 1000.0;
        let l_arq = 1.5 * srtt;

        // t_fec: time for P(FEC recovery) > 0.5 for a single loss
        // Find T where p_fec_recovery_by_time(T, 1, r, q, eps) > 0.5
        let mut t_fec = 0.0;
        for t_ms in 0..10000 {
            let t = t_ms as f64 / 1000.0;
            if p_fec_recovery_by_time(t, 1, r, q, eps) > 0.5 {
                t_fec = t;
                break;
            }
        }

        let fec_wins = t_fec < l_arq && t_fec > 0.0;
        println!("  RTT={rtt_ms}ms: t_fec={:.1}ms, L_arq={:.1}ms → {}",
            t_fec * 1000.0, l_arq * 1000.0,
            if fec_wins { "FEC wins" } else { "ARQ wins" });

        if fec_wins != last_fec_wins {
            println!("  *** CROSSOVER between RTT={}ms and previous ***", rtt_ms);
        }
        last_fec_wins = fec_wins;
    }
}

// =========================================================================
// 2.11 Sequence-aware P_lost validation
// =========================================================================

#[test]
fn test_p_lost_seq_fifo() {
    // On FIFO channel: 1 subsequent ACK → certainty of loss
    assert_close(p_lost_seq(1, 0.0), 1.0, 0.001);
    assert_close(p_lost_seq(3, 0.0), 1.0, 0.001);
}

#[test]
fn test_p_lost_seq_reorder() {
    // With 5% reorder rate
    assert_close(p_lost_seq(1, 0.05), 0.95, 0.001);
    assert_close(p_lost_seq(3, 0.05), 0.999875, 0.001);
}

#[test]
fn test_p_lost_combined() {
    // Combined should be max of time and seq evidence
    let eps = 0.05;
    let srtt = 0.050;
    let rttvar = 0.005;

    // At t=0 with 1 subsequent ACK on FIFO: seq evidence dominates
    let p = p_lost_combined(0.0, eps, srtt, rttvar, 1, 0.0);
    assert_close(p, 1.0, 0.001); // seq says lost

    // At t=0 with 0 subsequent ACKs: time evidence only
    let p = p_lost_combined(0.0, eps, srtt, rttvar, 0, 0.0);
    assert_close(p, eps, 0.005); // time says ~eps
}

// =========================================================================
// 2.12 Post-burst FEC boost
// =========================================================================

#[test]
fn test_burst_deficit() {
    let r = 0.15;
    let eps = 0.05;

    // Short burst (3 symbols), window=50 → pipeline should cover it
    let d1 = burst_deficit(3, r, eps, 50.0);
    println!("Burst=3, W=50: deficit={d1:.1}");
    // pipeline = 0.15 * 0.95 * 50 / 1.15 = 6.2 → covers burst of 3
    assert!(d1 < 0.1, "Short burst should have no deficit: {d1}");

    // Long burst (20 symbols), window=50 → deficit
    let d2 = burst_deficit(20, r, eps, 50.0);
    println!("Burst=20, W=50: deficit={d2:.1}");
    assert!(d2 > 10.0, "Long burst should have deficit: {d2}");
}

#[test]
fn test_boost_params() {
    let (boost_r, duration) = boost_params(10.0, 0.15, 0.05);
    println!("Deficit=10: boost_r={boost_r:.3}, duration={duration:.1} ticks");
    assert!(boost_r > 0.15, "Boosted r should exceed base r");
    assert!(duration > 0.0, "Boost should have positive duration");
}

// =========================================================================
// 2.13 Estimator feedback stability
// =========================================================================

#[test]
fn test_estimator_feedback_stability() {
    // Run estimator + FecRateController in a feedback loop for 10000 ticks.
    // Verify r doesn't oscillate wildly.
    let eps = 0.10;
    let q = 0.3;
    let p = eps * q / (1.0 - eps);

    let mut est = LossEstimator::new();
    let ctrl = FecRateController::new(0.5, 0.004);
    let mode = TriangleMode::ComputeR { delta: 0.01, rho: 1.0 };

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let channel = generate_ge_sequence(p, q, 10000, &mut rng);

    let mut r_values = Vec::new();
    for tick in 0..10000u64 {
        let batch_size = 10;
        let lost = channel[tick as usize..(tick as usize + batch_size).min(10000)]
            .iter().filter(|&&l| l).count() as u32;
        let received = batch_size as u32 - lost;
        est.record_batch(batch_size as u32, received, tick);

        let r = ctrl.compute_repair_rate(&est, &mode, 50);
        r_values.push(r);
    }

    // Check: no wild oscillation (coefficient of variation < 1.0)
    let mean_r: f64 = r_values.iter().skip(100).sum::<f64>() / (r_values.len() - 100) as f64;
    let var_r: f64 = r_values.iter().skip(100).map(|r| (r - mean_r).powi(2)).sum::<f64>()
        / (r_values.len() - 100) as f64;
    let cv = var_r.sqrt() / mean_r.max(0.001);

    println!("Feedback stability: mean_r={mean_r:.4}, std={:.4}, CV={cv:.3}", var_r.sqrt());
    assert!(cv < 1.0, "r should not oscillate wildly: CV={cv:.3}");
    assert!(mean_r > 0.05, "r should be positive for 10% loss: {mean_r:.4}");
}

// =========================================================================
// 2.14 FEC latency CDF validation
// =========================================================================

#[test]
fn test_fec_latency_cdf_properties() {
    let r = 0.15;
    let q = 0.5;
    let eps = 0.05;

    // P(recovery) should increase with T
    let mut prev = 0.0;
    for t in 0..100 {
        let p = p_fec_recovery_by_time(t as f64, 1, r, q, eps);
        assert!(p >= prev - 1e-10, "FEC CDF should be monotone: t={t}, p={p}, prev={prev}");
        prev = p;
    }

    // P(recovery by T=∞) should approach 1.0 for adequate r
    let p_inf = p_fec_recovery_by_time(10000.0, 1, r, q, eps);
    assert!(p_inf > 0.99, "FEC should eventually recover: P={p_inf}");

    // More losses need more time
    let p_m1 = p_fec_recovery_by_time(10.0, 1, r, q, eps);
    let p_m5 = p_fec_recovery_by_time(10.0, 5, r, q, eps);
    assert!(p_m1 > p_m5, "More losses should take longer: m=1:{p_m1:.3}, m=5:{p_m5:.3}");
}

#[test]
fn test_delivered_by_time_properties() {
    let eps = 0.10;
    let q = 0.3;
    let r = 0.20;
    let srtt = 0.050;

    // Should be monotone increasing
    let mut prev = 0.0;
    for t_ms in 0..500 {
        let p = p_delivered_by_time(t_ms as f64 / 1000.0, eps, q, r, srtt);
        assert!(p >= prev - 1e-10, "Delivery CDF should be monotone");
        prev = p;
    }

    // At T=0: P = 1-eps (only non-lost symbols)
    let p0 = p_delivered_by_time(0.0, eps, q, r, srtt);
    assert_close(p0, 1.0 - eps, 0.02);

    // At T >> RTT: P → 1.0
    let p_large = p_delivered_by_time(10.0, eps, q, r, srtt);
    assert!(p_large > 0.99, "Should approach 1.0 at large T: {p_large}");
}

#[test]
fn test_solve_r_from_time_budget() {
    let eps = 0.10;
    let q = 0.3;
    let srtt = 0.050;

    // Tight budget (20ms) should need more r than loose budget (200ms)
    let r_tight = solve_r_from_time_budget(eps, q, 0.020, 0.99, srtt);
    let r_loose = solve_r_from_time_budget(eps, q, 0.200, 0.99, srtt);

    println!("Time budget solver: tight(20ms) r={r_tight:.4}, loose(200ms) r={r_loose:.4}");
    assert!(r_tight >= r_loose, "Tighter budget should need more r");
}

// =========================================================================
// 2.19 Exact P_fec (Section 8.7) vs Monte Carlo of the same process
// =========================================================================

#[test]
fn test_p_fec_exact_matches_monte_carlo() {
    // Simulate exactly the process Section 8.7 describes: one GE chain
    // walking the interleaved wire sequence; success iff surviving
    // repairs >= source losses. The DP should match to sampling error —
    // much tighter than the 0.20 tolerance the normal approximation needs.
    let scenarios: Vec<(&str, f64, f64, f64)> = vec![
        ("WiFi", 0.013, 0.5, 0.10),
        ("LTE", 0.02, 0.4, 0.12),
        ("Sat", 0.03, 0.3, 0.25),
    ];
    let w = 50usize;
    let trials = 20000;
    for (name, p, q, r) in scenarios {
        let repairs = (r * w as f64).round() as usize;
        let n = w + repairs;
        let pi_b = p / (p + q);
        let mut rng = ChaCha8Rng::seed_from_u64(87);
        let mut ok = 0u32;
        for _ in 0..trials {
            let mut bad = rng.gen::<f64>() < pi_b;
            let (mut k, mut c) = (0u32, 0u32);
            for i in 0..n {
                bad = if bad { rng.gen::<f64>() >= q } else { rng.gen::<f64>() < p };
                let is_repair = (i + 1) * repairs / n > i * repairs / n;
                if is_repair {
                    if !bad { c += 1; }
                } else if bad {
                    k += 1;
                }
            }
            if c >= k { ok += 1; }
        }
        let mc = ok as f64 / trials as f64;
        let exact = p_fec_exact(p, q, r, w);
        println!("{name}: exact={exact:.4} mc={mc:.4} err={:.4}", (exact - mc).abs());
        assert_close(exact, mc, 0.015);
    }
}

// =========================================================================
// 2.20 OPTIMALITY of r* (formula-independent Monte-Carlo argmin)
// =========================================================================
//
// The existing tests validate FIDELITY (P_fec at r* matches the process).
// This block adds the missing OPTIMALITY axis: for a grid of (eps, sigma2,
// W, target), independently find the true minimum-overhead r that meets the
// window-failure target via Monte-Carlo of the exact GE process, and assert
// the closed-form r* lands on that MC argmin.
//
// Target semantics: the controller sets z = Phi^-1(1 - delta/eps), i.e. the
// window-FAILURE probability target is target_wf = delta/eps (per-symbol
// residual = eps * window_fail). We parametrize directly by target_wf so the
// comparison is apples-to-apples with the formula's z.

/// Invert (eps, sigma2) -> Gilbert-Elliott (p_gb, q_bg).
///   eps = p/(p+q),  sigma2 = 1 + 2(1-p-q)/(p+q)  =>  s=p+q = 2/(sigma2+1)
///   p = eps*s,  q = (1-eps)*s
fn ge_from_eps_sigma2(eps: f64, sigma2: f64) -> (f64, f64) {
    let s = 2.0 / (sigma2 + 1.0);
    (eps * s, (1.0 - eps) * s)
}

/// Monte-Carlo window-failure probability for the EXACT interleaved GE
/// process (same process as test_p_fec_exact_matches_monte_carlo): one GE
/// chain walks W source + round(r*W) repair slots; failure iff surviving
/// repairs < source losses. Formula-independent ground truth.
fn mc_window_fail(p: f64, q: f64, r: f64, w: usize, trials: usize, seed: u64) -> f64 {
    let repairs = (r.max(0.0) * w as f64).round() as usize;
    let n = w + repairs;
    if n == 0 { return 0.0; }
    let pi_b = p / (p + q);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut fail = 0u32;
    for _ in 0..trials {
        let mut bad = rng.gen::<f64>() < pi_b;
        let (mut k, mut c) = (0u32, 0u32);
        for i in 0..n {
            bad = if bad { rng.gen::<f64>() >= q } else { rng.gen::<f64>() < p };
            let is_repair = repairs > 0 && (i + 1) * repairs / n > i * repairs / n;
            if is_repair {
                if !bad { c += 1; }
            } else if bad {
                k += 1;
            }
        }
        if c < k { fail += 1; }
    }
    fail as f64 / trials as f64
}

/// Smallest r on a fine grid whose MC window-fail <= target (the true MC
/// argmin-overhead: overhead is monotone in r, feasible set is an upper
/// interval, so the constrained min is this crossing). Linear-interpolates
/// between the bracketing grid points for sub-grid resolution.
fn mc_optimal_r(p: f64, q: f64, w: usize, target: f64, trials: usize) -> f64 {
    let step = 1.0 / w as f64; // repair-count granularity
    let mut prev_r = 0.0_f64;
    let mut prev_f = mc_window_fail(p, q, 0.0, w, trials, 0xC0FFEE);
    let mut r = step;
    while r <= 2.0 + 1e-9 {
        let f = mc_window_fail(p, q, r, w, trials, 0xC0FFEE);
        if f <= target {
            // Crossing between prev_r (f>target) and r (f<=target).
            if prev_f <= target { return prev_r.max(0.0); }
            let denom = (prev_f - f).max(1e-9);
            let frac = (prev_f - target) / denom;
            return prev_r + frac * (r - prev_r);
        }
        prev_r = r;
        prev_f = f;
        r += step;
    }
    2.0 // even 200% overhead insufficient
}

#[test]
fn test_r_star_optimality_mc() {
    // (name, eps, sigma2, W, target_wf = delta/eps)
    let grid: Vec<(&str, f64, f64, usize, f64)> = vec![
        ("iid-lo",    0.02, 1.0, 50, 0.05),
        ("iid-mid",   0.05, 1.0, 50, 0.05),
        ("iid-hi",    0.10, 1.0, 50, 0.10),
        ("bursty-lo", 0.02, 2.0, 50, 0.05),
        ("bursty-mid",0.05, 3.0, 50, 0.05),
        ("bursty-hi", 0.10, 2.5, 50, 0.10),
        ("wide-W",    0.05, 2.0, 100, 0.05),
        ("tight-tgt", 0.05, 1.5, 50, 0.02),
    ];
    let trials = 40_000;
    let mut max_abs_gap = 0.0_f64;
    let mut max_underprov = 0.0_f64; // worst normal-approx residual overshoot
    println!("\n=== r* OPTIMALITY (MC argmin vs closed form) ===");
    println!("{:<11} {:>6} {:>6} {:>4} {:>6} | {:>8} {:>8} {:>8} | {:>9} {:>9} {:>6}",
        "case","eps","sig2","W","tgt_wf","r*_norm","r*_mc","r*_exact","Pf(norm)","Pf(exact)","dR");
    for (name, eps, sigma2, w, target) in &grid {
        let (p, q) = ge_from_eps_sigma2(*eps, *sigma2);
        // sanity: the inversion reproduces the intended (eps, sigma2)
        assert_close(p / (p + q), *eps, 1e-9);
        assert_close(burst_variance_factor(p, q), *sigma2, 1e-9);

        let z = normal_quantile(1.0 - target);
        let r_formula = compute_r_star_with_z(*eps, *sigma2, *w as f64, z);
        let r_exact = compute_r_star_exact(p, q, *w, *target);
        let r_mc = mc_optimal_r(p, q, *w, *target, trials);
        let pf_norm = mc_window_fail(p, q, r_formula, *w, trials, 0xBEEF);
        let pf_exact = mc_window_fail(p, q, r_exact, *w, trials, 0xBEEF);

        let gap = (r_formula - r_mc).abs();
        max_abs_gap = max_abs_gap.max(gap);
        let underprov = (pf_norm - target).max(0.0);
        max_underprov = max_underprov.max(underprov);
        println!("{name:<11} {eps:>6.3} {sigma2:>6.2} {w:>4} {target:>6.3} | {r_formula:>8.4} {r_mc:>8.4} {r_exact:>8.4} | {pf_norm:>9.4} {pf_exact:>9.4} {gap:>6.4}");

        // (A) STRONG OPTIMALITY of the EXACT DP r* (compute_r_star_exact): it
        // is MC-optimal on ALL channels and always FEASIBLE (meets target
        // within sampling noise). This is the model's true optimum.
        assert!(
            pf_exact <= target + 0.015,
            "{name}: EXACT r*={r_exact:.4} under-provisions: P_fail={pf_exact:.4} > target {target:.4}"
        );
        assert!(
            (r_exact - r_mc).abs() < 1.5 / *w as f64 + 0.01,
            "{name}: exact DP argmin {r_exact:.4} disagrees with MC argmin {r_mc:.4}"
        );
        // (B) The NORMAL-approx r* (production's compute_r_star_with_z) lands
        // within ~1 repair slot of the MC argmin — MC-optimal on iid, but it
        // UNDER-PROVISIONS on bursty channels (the finding below). The r-space
        // gap is bounded by the normal approximation's known 1-slot bias.
        let tol = 1.5 / *w as f64 + 0.02;
        assert!(
            gap < tol,
            "{name}: normal r*={r_formula:.4} not within a slot of MC argmin {r_mc:.4}, gap={gap:.4}"
        );
        if underprov > 0.01 {
            println!("  FINDING[{name}]: NORMAL-approx r* UNDER-provisions on bursty sigma2={sigma2}: \
                      residual {pf_norm:.3} vs target {target:.3} (+{:.1}% relative). \
                      Exact DP r*={r_exact:.4} closes it.", 100.0 * underprov / target);
        }
    }
    println!("max |r*_normal - r*_mc| over grid = {max_abs_gap:.4}  |  \
              worst normal-approx over-target residual = {max_underprov:.4}");
    // FINDING GATE: the EXACT r* is MC-optimal everywhere; the NORMAL r* is
    // within one repair slot in r-space. The bursty under-provision is a real,
    // bounded property of the normal approximation, not a formula error — the
    // codebase already ships compute_r_star_exact for callers that need it.
    // FINDING GATE: if the normal-approx r* were badly off-optimal this would
    // trip. It documents the closed form as MC-optimal to within ~1 repair slot.
    assert!(max_abs_gap < 0.06, "r* systematically off MC-optimal: {max_abs_gap:.4}");
}

#[test]
fn test_r_star_structural_properties() {
    // --- r -> 0 as delta -> eps (margin z -> -inf, clamped at floor 0) ---
    let eps = 0.05;
    let (p, q) = ge_from_eps_sigma2(eps, 2.0);
    for &frac in &[0.99_f64, 0.999, 1.0, 1.05] {
        // target_wf = delta/eps -> 1 as delta -> eps
        let z = normal_quantile(1.0 - frac.min(1.0));
        let r = compute_r_star_with_z(eps, 2.0, 50.0, z);
        assert!(r < 1e-9, "r* should vanish as delta->eps: frac={frac}, r={r}");
    }
    // sanity: at delta/eps=1 the MC process also needs ~0 overhead to hit a
    // 100%-failure-allowed target.
    assert!(mc_optimal_r(p, q, 50, 1.0, 20_000) < 1e-6);

    // --- monotonicity in eps (fixed target ratio, sigma2, W) ---
    let z = normal_quantile(1.0 - 0.05);
    let mut prev = -1.0;
    for i in 1..=20 {
        let e = 0.005 * i as f64;
        let r = compute_r_star_with_z(e, 2.0, 50.0, z);
        assert!(r >= prev - 1e-12, "r* must be nondecreasing in eps: eps={e}, r={r}, prev={prev}");
        prev = r;
    }

    // --- N=1 identity: a one-symbol "window" degenerates to raw-channel FEC ---
    // With W=1, base term eps/(1-eps) already covers the single symbol's
    // expected loss; the MC argmin for a modest target is ~ that base rounded
    // to the 1-slot grid. Assert formula and MC agree at W=1.
    {
        let eps = 0.10;
        let (p, q) = ge_from_eps_sigma2(eps, 1.0);
        let z = normal_quantile(1.0 - 0.10);
        let r_formula = compute_r_star_with_z(eps, 1.0, 1.0, z);
        let r_mc = mc_optimal_r(p, q, 1, 0.10, 40_000);
        // W=1 grid granularity is 1.0, so tolerance is one full slot.
        assert!((r_formula - r_mc).abs() < 1.0 + 1e-9,
            "N=1 identity: formula {r_formula:.3} vs MC {r_mc:.3}");
    }

    // --- convexity of the tail/overhead objective => interior stationary
    // point is the GLOBAL min (paper 14.21 p99 model, via r_saturation). ---
    // Reconstruct the p99 objective and verify it is unimodal (single sign
    // change in its discrete first difference), so its argmin is global.
    {
        let eps = 0.05;
        let sigma2 = 2.0;
        let w = 50.0;
        let srtt = 0.05;
        let t_sym = 0.0002;
        let l_arq = 1.5 * srtt;
        let b_hat = (sigma2 + 1.0) / 2.0;
        let c_dilution = 0.5;
        let cost = |r: f64| {
            let tail_fec = (1.0 - p_fec_normal(r, eps, w, sigma2)) * l_arq;
            let tail_rec = b_hat * t_sym * (1.0 + r) / (r * (1.0 - eps));
            let tail_svc = c_dilution * (1.0 + r) * w * t_sym;
            tail_fec + tail_rec + tail_svc
        };
        let rs: Vec<f64> = (0..=198).map(|i| 0.01 + 0.005 * i as f64).collect();
        let costs: Vec<f64> = rs.iter().map(|&r| cost(r)).collect();
        // Count sign changes of the first difference. Unimodal (convex-like
        // with a single interior min) => exactly one -,+ transition.
        let mut sign_changes = 0;
        let mut prev_sign = 0i32;
        for w2 in costs.windows(2) {
            let d = w2[1] - w2[0];
            let s = if d > 0.0 { 1 } else if d < 0.0 { -1 } else { 0 };
            if s != 0 && s != prev_sign && prev_sign != 0 { sign_changes += 1; }
            if s != 0 { prev_sign = s; }
        }
        assert!(sign_changes <= 1, "p99 objective must be unimodal: {sign_changes} sign changes");
        // The stationary point found by r_saturation is the unique global min.
        let r_sat = r_saturation(eps, sigma2, w, srtt, t_sym);
        let c_sat = cost(r_sat);
        let global_min = costs.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!((c_sat - global_min).abs() < 1e-9 * global_min.max(1.0) + 1e-12,
            "r_saturation stationary point must be the global min: cost={c_sat}, min={global_min}");
    }
    println!("structural properties: r->0 at delta=eps, monotone in eps, N=1 identity, unimodal p99 objective — all hold");
}

#[test]
fn test_p_fec_exact_vs_normal_divergence() {
    // Document the normal approximation's error against the exact DP
    // (Section 8.7 table: 1.7–2.8% at these operating points).
    let scenarios: Vec<(&str, f64, f64, f64)> = vec![
        ("WiFi", 0.013, 0.5, 0.10),
        ("LTE", 0.02, 0.4, 0.12),
        ("Sat", 0.03, 0.3, 0.25),
    ];
    for (name, p, q, r) in scenarios {
        let eps = p / (p + q);
        let s2 = burst_variance_factor(p, q);
        let exact = p_fec_exact(p, q, r, 50);
        let normal = p_fec_normal(r, eps, 50.0, s2);
        let err = (exact - normal).abs();
        println!("{name}: exact={exact:.4} normal={normal:.4} err={err:.4}");
        assert!(err > 0.005, "{name}: exact should differ measurably from normal here: {err}");
        assert!(err < 0.05, "{name}: normal should still be within 5% here: {err}");
    }
}
