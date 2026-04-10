//! Pure math functions for the raptorpath FEC/ARQ model.
//!
//! No IO, no async, no platform dependencies. Used by both the main
//! raptorpath crate and the raptorpath-wasm crate.

pub mod changepoint;
pub mod gilbert_elliott;
pub mod estimator;
pub mod fec_rate_controller;
pub mod rlc;

pub use changepoint::BayesianChangepoint;
pub use gilbert_elliott::GilbertElliottEstimator;
pub use estimator::LossEstimator;
pub use fec_rate_controller::FecRateController;
pub use rlc::{RlcEncoder, RlcDecoder};

/// Standard normal survival function: P(Z > z) = 1 - Phi(z).
/// Abramowitz & Stegun rational approximation.
pub fn normal_survival(z: f64) -> f64 {
    if z > 8.0 { return 0.0; }
    if z < -8.0 { return 1.0; }
    let x = z / std::f64::consts::SQRT_2;
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t * (0.254829592
        + t * (-0.284496736
            + t * (1.421413741
                + t * (-1.453152027
                    + t * 1.061405429))));
    let erfc_ax = poly * (-ax * ax).exp();
    if x >= 0.0 { 0.5 * erfc_ax } else { 1.0 - 0.5 * erfc_ax }
}

/// Standard normal quantile (inverse CDF). Abramowitz & Stegun rational approximation.
pub fn normal_quantile(p: f64) -> f64 {
    if p <= 0.0 { return f64::NEG_INFINITY; }
    if p >= 1.0 { return f64::INFINITY; }
    if (p - 0.5).abs() < 1e-12 { return 0.0; }
    let (sign, q) = if p < 0.5 { (-1.0, p) } else { (1.0, 1.0 - p) };
    let t = (-2.0 * q.ln()).sqrt();
    let num = 2.515517 + 0.802853 * t + 0.010328 * t * t;
    let den = 1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t;
    sign * (t - num / den)
}

/// Compute P_lost(t): probability a symbol was lost given no ACK after time t.
///
/// P_lost(t) = epsilon / [epsilon + (1-epsilon) * P(RTT > t)]
///
/// See paper Section 3.4.
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    if epsilon >= 1.0 { return 1.0; }
    if epsilon <= 0.0 { return 0.0; }
    let rttvar = rttvar_secs.max(0.001);
    let z = (age_secs - srtt_secs) / rttvar;
    let p_rtt_exceeds = normal_survival(z);
    let denom = epsilon + (1.0 - epsilon) * p_rtt_exceeds;
    if denom < 1e-300 { return 1.0; }
    (epsilon / denom).clamp(0.0, 1.0)
}

/// Burst variance inflation factor sigma2_burst for the GE channel.
///
/// sigma2_burst = 1 + 2(1-p-q)/(p+q)
///
/// where p = P(Good->Bad), q = P(Bad->Good).
/// Returns 1.0 (iid) when parameters are degenerate.
/// See paper Section 8.3.
pub fn burst_variance_factor(p: f64, q: f64) -> f64 {
    let sum = p + q;
    if sum < 1e-10 { return 1.0; }
    let factor = 1.0 + 2.0 * (1.0 - p - q) / sum;
    factor.max(1.0)
}

/// Compute the optimal correction rate r* with sigma2_burst margin.
///
/// r* = epsilon/(1-epsilon) + z * sqrt(epsilon * sigma2 / (W * (1-epsilon)))
///
/// where z = 2.33 (99th percentile). See paper Section 8.4.
pub fn compute_r_star(epsilon: f64, sigma2: f64, window_size: f64) -> f64 {
    if epsilon <= 0.0 || epsilon >= 1.0 { return 0.0; }
    let base = epsilon / (1.0 - epsilon);
    let margin = if window_size > 0.0 {
        2.33 * (epsilon * sigma2 / (window_size * (1.0 - epsilon))).sqrt()
    } else {
        0.0
    };
    base + margin
}

/// Taper function: time-decaying correction density tau(t) = A * (1-q)^t.
///
/// See paper Section 4.
#[derive(Debug, Clone)]
pub struct TaperFunction {
    pub amplitude: f64,
    pub decay: f64,
    pub total_rate: f64,
    pub q: f64,
}

impl TaperFunction {
    /// Create from correction rate and GE parameter q.
    pub fn new(rate: f64, q: f64) -> Self {
        let q = q.clamp(0.01, 1.0);
        Self {
            amplitude: rate * q,
            decay: 1.0 - q,
            total_rate: rate,
            q,
        }
    }

    /// Correction density at time offset t.
    pub fn density(&self, t: f64) -> f64 {
        self.amplitude * self.decay.powf(t)
    }

    /// Probabilistic generation decision.
    pub fn should_generate(&self, t: f64, rng_value: f64) -> bool {
        let d = self.density(t);
        if d >= 1.0 { true } else { rng_value < d }
    }
}

/// P_fec using normal approximation (paper Section 8.1).
///
/// P_fec = Phi(sqrt(W) * (r(1-epsilon)-epsilon) / sqrt(epsilon*(1-epsilon)*(r+sigma2)))
pub fn p_fec_normal(r: f64, epsilon: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if window_size <= 0.0 || epsilon <= 0.0 || epsilon >= 1.0 || r <= 0.0 { return 0.0; }
    let numerator = r * (1.0 - epsilon) - epsilon;
    if numerator <= 0.0 { return 0.0; }
    let denominator = (epsilon * (1.0 - epsilon) * (r + sigma2_burst)).sqrt();
    if denominator < 1e-300 { return 1.0; }
    let z = window_size.sqrt() * numerator / denominator;
    1.0 - normal_survival(z)
}

/// Compute delta (tail latency) from r and rho.
///
/// delta = P(late delivery) / rho. See paper Section 6.3.
pub fn compute_delta(epsilon: f64, r: f64, rho: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if epsilon <= 0.0 || rho <= 0.0 { return 0.0; }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    let fec_miss = epsilon * (1.0 - p_fec);
    if fec_miss < 1e-15 { return 0.0; }
    let p_arq = (1.0 - (1.0 - rho) / fec_miss).clamp(0.0, 1.0);
    fec_miss * p_arq / rho
}

/// P(symbol recovered within time T) using FEC + ARQ.
pub fn p_recovered_within(t: f64, epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    if epsilon <= 0.0 { return 1.0; }
    let p_fec = p_fec_normal(r, epsilon, window_size, sigma2_burst);
    let decay = (1.0 - q).max(0.0);
    let corrections_by_t = r * (1.0 - decay.powf(t + 1.0));
    let r_needed = epsilon / (1.0 - epsilon);
    let p_arq = if r_needed > 0.0 { (corrections_by_t / r_needed).min(1.0) } else { 1.0 };
    1.0 - epsilon * (1.0 - p_fec) * (1.0 - p_arq)
}

/// Find T_cut from target reliability rho via binary search.
/// Returns Infinity at rho=1.0. See paper Section 9.4.
pub fn find_t_cut(epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64, target_rho: f64) -> f64 {
    if target_rho >= 1.0 { return f64::INFINITY; }
    if target_rho <= 0.0 || epsilon <= 0.0 { return 0.0; }
    let mut lo: f64 = 0.0;
    let mut hi: f64 = window_size * 10.0;
    for _ in 0..100 {
        if hi - lo < 0.01 { break; }
        let mid = (lo + hi) / 2.0;
        if p_recovered_within(mid, epsilon, q, r, window_size, sigma2_burst) < target_rho {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Maximum burst length B_max at 99.99th percentile.
/// B_max = ceil(ln(0.0001) / ln(1-q)). See paper Section 9.3.
pub fn b_max(q: f64) -> u64 {
    if q <= 0.0 || q >= 1.0 { return 1; }
    let ln_persist = (1.0 - q).ln();
    if ln_persist >= 0.0 { return 1; }
    ((0.0001_f64).ln() / ln_persist).ceil() as u64
}

/// Buffer max from T_cut. See paper Section 9.3.
pub fn compute_buffer_max(epsilon: f64, q: f64, r: f64, t_cut: f64) -> f64 {
    if t_cut.is_infinite() {
        let bmax = b_max(q) as f64;
        let drain_rate = r * (1.0 - epsilon);
        if drain_rate > 0.0 { bmax / drain_rate } else { bmax }
    } else {
        t_cut
    }
}

/// Three-variable result.
#[derive(Debug, Clone)]
pub struct ThreeVarResult {
    pub r: f64,
    pub delta: f64,
    pub rho: f64,
    pub t_cut: f64,
    pub buffer_max: f64,
}

/// Mode 1: Given (delta, rho) -> compute r. See paper Section 8.6.
pub fn solve_r_from_delta_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, delta: f64, rho: f64) -> ThreeVarResult {
    let mut lo = epsilon / (1.0 - epsilon);
    let mut hi = 2.0;
    for _ in 0..100 {
        if hi - lo < 1e-6 { break; }
        let mid = (lo + hi) / 2.0;
        if compute_delta(epsilon, mid, rho, window_size, sigma2_burst) > delta { lo = mid; } else { hi = mid; }
    }
    let r = hi;
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

/// Mode 2: Given (r, rho) -> compute delta. See paper Section 8.6.
pub fn solve_delta_from_r_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, rho: f64) -> ThreeVarResult {
    let delta = compute_delta(epsilon, r, rho, window_size, sigma2_burst);
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

/// Mode 3: Given (r, delta) -> compute rho. See paper Section 8.6.
pub fn solve_rho_from_r_delta(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, delta: f64) -> ThreeVarResult {
    let mut lo = 0.5;
    let mut hi = 1.0 - 1e-12;
    for _ in 0..100 {
        if hi - lo < 1e-6 { break; }
        let mid = (lo + hi) / 2.0;
        if compute_delta(epsilon, r, mid, window_size, sigma2_burst) > delta { hi = mid; } else { lo = mid; }
    }
    let rho = lo;
    let t_cut = find_t_cut(epsilon, q, r, window_size, sigma2_burst, rho);
    ThreeVarResult { r, delta, rho, t_cut, buffer_max: compute_buffer_max(epsilon, q, r, t_cut) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_survival_basic() {
        assert!((normal_survival(0.0) - 0.5).abs() < 0.01);
        assert!((normal_survival(2.0) - 0.0228).abs() < 0.005);
    }

    #[test]
    fn test_p_lost_monotone() {
        let p1 = p_lost(0.01, 0.025, 0.050, 0.005);
        let p2 = p_lost(0.05, 0.025, 0.050, 0.005);
        let p3 = p_lost(0.10, 0.025, 0.050, 0.005);
        assert!(p1 < p2 && p2 < p3);
    }

    #[test]
    fn test_burst_variance() {
        let s = burst_variance_factor(0.001, 0.5);
        assert!((s - 3.0).abs() < 0.1);
    }

    #[test]
    fn test_taper_density_decays() {
        let taper = TaperFunction::new(0.08, 0.5);
        assert!(taper.density(0.0) > taper.density(1.0));
        assert!(taper.density(1.0) > taper.density(2.0));
    }

    #[test]
    fn test_three_var_consistency() {
        let eps = 0.10;
        let q = 0.3;
        let w = 20.0;
        let s2 = 3.0;
        let m1 = solve_r_from_delta_rho(eps, q, w, s2, 0.05, 0.999);
        let m2 = solve_delta_from_r_rho(eps, q, w, s2, m1.r, 0.999);
        assert!((m1.delta - m2.delta).abs() < 0.01);
    }
}
