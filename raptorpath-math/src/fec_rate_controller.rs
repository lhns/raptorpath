//! Simplified FEC rate controller for wasm (no serde/FecBackend deps).

use crate::estimator::LossEstimator;
use crate::burst_variance_factor;

/// FEC rate controller — computes optimal repair rate from estimator state.
pub struct FecRateController {
    /// Confidence level for BOCD quantile
    target_tail_loss: f64,
    /// Maximum repair rate (clamp)
    max_overhead: f64,
    /// Codec-specific overhead (e.g., 0.004 for RLC)
    codec_overhead: f64,
}

impl FecRateController {
    pub fn new(target_tail_loss: f64, max_overhead: f64, codec_overhead: f64) -> Self {
        Self { target_tail_loss, max_overhead, codec_overhead }
    }

    /// Compute the repair rate from current estimator state.
    ///
    /// Uses BOCD predictive quantile + sigma2_burst margin.
    /// See paper Section 8.4.
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, window_size: usize) -> f64 {
        let confidence = 1.0 - self.target_tail_loss;
        let p = estimator.predictive_loss_upper(confidence);
        if p < 1e-10 {
            return 0.0;
        }

        // Codec overhead weighted by P(decoder invoked)
        let effective_codec_overhead = if self.codec_overhead > 0.0 && window_size > 0 {
            let p_decoder_invoked = 1.0 - (1.0 - p).powi(window_size as i32);
            self.codec_overhead * p_decoder_invoked
        } else {
            0.0
        };

        // sigma2_burst from GE model
        let ge = estimator.ge_estimator();
        let sigma2 = if ge.is_valid() {
            burst_variance_factor(ge.p_gb(), ge.p_bg())
        } else {
            1.0
        };

        // r* = p/(1-p) + margin + codec_overhead
        let margin = if window_size > 0 {
            2.33 * (p * sigma2 / (window_size as f64 * (1.0 - p))).sqrt()
        } else {
            0.0
        };
        let random_rate = p / (1.0 - p) + margin + effective_codec_overhead;

        // Burst term: B/T
        let burst_rate = if ge.is_valid() {
            let burst_length = ge.mean_burst_length().max(1.0);
            // Without throughput/symbol_size info, approximate T as window_size
            burst_length / (window_size as f64).max(1.0)
        } else {
            0.0
        };

        let rate = random_rate.max(burst_rate);
        rate.clamp(0.0, self.max_overhead)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_loss_zero_rate() {
        let ctrl = FecRateController::new(1e-5, 0.5, 0.004);
        let mut est = LossEstimator::new();
        for i in 0..50 {
            est.record_batch(100, 100, i);
        }
        let rate = ctrl.compute_repair_rate(&est, 50);
        assert!(rate < 0.05, "Zero loss should produce near-zero rate: {rate}");
    }

    #[test]
    fn test_high_loss_high_rate() {
        let ctrl = FecRateController::new(1e-5, 0.5, 0.004);
        let mut est = LossEstimator::new();
        for i in 0..100 {
            est.record_batch(100, 80, i);
        }
        let rate = ctrl.compute_repair_rate(&est, 50);
        assert!(rate > 0.2, "20% loss should produce >20% rate: {rate}");
    }
}
