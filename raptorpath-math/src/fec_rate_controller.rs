//! FEC rate controller: wraps the triangle solver with production concerns.
//!
//! The triangle solver computes the theoretical r from (ε, q, W, σ², mode).
//! The controller adds: codec overhead, burst floor, max_overhead cap.
//! Same code path for simulation and production.

use crate::estimator::LossEstimator;
use crate::{TriangleMode, burst_variance_factor, solve_r_from_delta_rho};

pub struct FecRateController {
    /// Maximum repair rate (clamp)
    pub max_overhead: f64,
    /// Codec-specific overhead (e.g., 0.004 for RLC)
    pub codec_overhead: f64,
}

impl FecRateController {
    pub fn new(max_overhead: f64, codec_overhead: f64) -> Self {
        Self { max_overhead, codec_overhead }
    }

    /// Compute the repair rate from estimator state and triangle mode.
    ///
    /// 1. Get ε, q, σ² from estimator
    /// 2. Call triangle solver based on mode → base r
    /// 3. Add codec overhead
    /// 4. Apply burst floor
    /// 5. Clamp to max_overhead
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, mode: &TriangleMode, window_size: usize) -> f64 {
        let ge = estimator.ge_estimator();
        let eps = estimator.loss_rate().max(1e-6);
        let q = if ge.is_valid() { ge.p_bg().max(0.01) } else { 0.5 };
        let p_gb = if ge.is_valid() { ge.p_gb() } else { eps * q / (1.0 - eps) };
        let sigma2 = burst_variance_factor(p_gb, q);
        let w = window_size as f64;

        // Base r from triangle solver
        let base_r = match mode {
            TriangleMode::ComputeR { delta, rho } => {
                solve_r_from_delta_rho(eps, q, w, sigma2, *delta, *rho).r
            }
            TriangleMode::ComputeDelta { r, .. } => *r,
            TriangleMode::ComputeRho { r, .. } => *r,
        };

        // Add codec overhead weighted by P(decoder invoked)
        let codec_oh = if self.codec_overhead > 0.0 && window_size > 0 {
            let p_decoder = 1.0 - (1.0 - eps).powi(window_size as i32);
            self.codec_overhead * p_decoder
        } else {
            0.0
        };

        // Burst floor: B/T
        let burst_floor = if ge.is_valid() {
            let burst_length = ge.mean_burst_length().max(1.0);
            burst_length / w.max(1.0)
        } else {
            0.0
        };

        let r = (base_r + codec_oh).max(burst_floor);
        r.clamp(0.0, self.max_overhead)
    }

    /// Compute the current triangle result for diagnostics.
    pub fn compute_triangle(&self, estimator: &LossEstimator, mode: &TriangleMode, window_size: usize) -> crate::ThreeVarResult {
        let ge = estimator.ge_estimator();
        let eps = estimator.loss_rate().max(1e-6);
        let q = if ge.is_valid() { ge.p_bg().max(0.01) } else { 0.5 };
        let p_gb = if ge.is_valid() { ge.p_gb() } else { eps * q / (1.0 - eps) };
        let sigma2 = burst_variance_factor(p_gb, q);
        let w = window_size as f64;

        match mode {
            TriangleMode::ComputeR { delta, rho } =>
                crate::solve_r_from_delta_rho(eps, q, w, sigma2, *delta, *rho),
            TriangleMode::ComputeDelta { r, rho } =>
                crate::solve_delta_from_r_rho(eps, q, w, sigma2, *r, *rho),
            TriangleMode::ComputeRho { r, delta } =>
                crate::solve_rho_from_r_delta(eps, q, w, sigma2, *r, *delta),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_r_mode() {
        let ctrl = FecRateController::new(0.5, 0.004);
        let mut est = LossEstimator::new();
        for i in 0..100 { est.record_batch(100, 90, i); }
        let mode = TriangleMode::ComputeR { delta: 0.01, rho: 1.0 };
        let r = ctrl.compute_repair_rate(&est, &mode, 50);
        assert!(r > 0.05, "10% loss should produce >5% rate: {r}");
        assert!(r < 0.5, "Should be under max_overhead: {r}");
    }

    #[test]
    fn test_fixed_r_mode() {
        let ctrl = FecRateController::new(0.5, 0.004);
        let mut est = LossEstimator::new();
        for i in 0..50 { est.record_batch(100, 100, i); }
        let mode = TriangleMode::ComputeDelta { r: 0.10, rho: 1.0 };
        let r = ctrl.compute_repair_rate(&est, &mode, 50);
        // r should be close to 0.10 (+ small codec overhead)
        assert!(r >= 0.10 && r < 0.15, "Fixed r=0.10 should be near 0.10: {r}");
    }

    #[test]
    fn test_zero_loss_low_r() {
        let ctrl = FecRateController::new(0.5, 0.004);
        let mut est = LossEstimator::new();
        for i in 0..100 { est.record_batch(100, 100, i); }
        let mode = TriangleMode::ComputeR { delta: 0.01, rho: 1.0 };
        let r = ctrl.compute_repair_rate(&est, &mode, 50);
        assert!(r < 0.05, "Zero loss should produce low r: {r}");
    }
}
