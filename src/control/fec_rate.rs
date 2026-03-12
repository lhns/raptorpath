//! FEC rate controller: computes optimal repair symbol count.
//!
//! Uses a hybrid feedforward + feedback architecture:
//!
//! 1. **Feedforward (statistical)**: Given estimated loss rate p and target tail
//!    loss probability δ, compute the minimum number of repair symbols r such that:
//!
//!      P(Binomial(k + r, 1-p) < k(1+ε)) ≤ δ
//!
//!    where k = source symbols, ε = RaptorQ overhead (~0.01).
//!    Using normal approximation:
//!
//!      r = ceil(k·p/(1-p) + z_δ · sqrt(n·p·(1-p)) + k·ε)
//!
//! 2. **Feedback (PI controller)**: Corrects for model mismatch by observing
//!    actual block decode failures and adjusting a correction term.

use super::estimator::LossEstimator;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Protocol hint affects FEC aggressiveness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProtocolHint {
    /// Real-time traffic (VoIP, gaming): more aggressive FEC, no retransmission
    Realtime,
    /// Bulk transfer: less FEC, rely on retransmission for residual loss
    Bulk,
    /// Auto-detect based on packet patterns
    Auto,
}

impl std::str::FromStr for ProtocolHint {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "realtime" | "rt" => Ok(Self::Realtime),
            "bulk" => Ok(Self::Bulk),
            "auto" => Ok(Self::Auto),
            _ => Err(anyhow::anyhow!("unknown protocol hint: {s}")),
        }
    }
}

/// FEC rate controller.
pub struct FecRateController {
    /// Target tail loss probability (e.g. 1e-5)
    target_tail_loss: f64,
    /// Maximum FEC overhead as fraction of source symbols
    max_overhead: f64,
    /// RaptorQ decode overhead factor
    rq_overhead: f64,
    /// Protocol hint
    hint: ProtocolHint,

    // PI feedback controller state
    /// Integral error accumulator
    integral_error: f64,
    /// Proportional gain
    kp: f64,
    /// Integral gain
    ki: f64,
    /// PI correction term (added to feedforward output)
    pi_correction: f64,
    /// Previous actual tail loss for derivative/tracking
    prev_actual_tail_loss: f64,
    /// Exponential moving average of actual block failure rate
    actual_failure_rate: f64,
}

impl FecRateController {
    pub fn new(target_tail_loss: f64, max_overhead: f64, hint: ProtocolHint) -> Self {
        Self {
            target_tail_loss,
            max_overhead,
            rq_overhead: 0.01, // RaptorQ typically decodes with <1% overhead
            hint,
            integral_error: 0.0,
            kp: 2.0,
            ki: 0.5,
            pi_correction: 0.0,
            prev_actual_tail_loss: 0.0,
            actual_failure_rate: 0.0,
        }
    }

    /// Compute the number of repair symbols needed for `k` source symbols
    /// given the current loss estimate from `estimator`.
    pub fn compute_repair_count(&self, k: u32, estimator: &LossEstimator) -> u32 {
        // Use the upper bound of loss rate for conservative estimation
        let p = estimator.loss_rate_upper(0.95);
        if p < 1e-10 {
            return 0; // No measurable loss
        }

        let feedforward = self.feedforward_repair(k, p);
        let correction = self.pi_correction.max(0.0) as u32;
        let total = feedforward + correction;

        // Apply protocol hint multiplier
        let total = match self.hint {
            ProtocolHint::Realtime => {
                // More aggressive: also account for burst losses
                let burst_extra = if estimator.is_in_burst() {
                    (k as f64 * 0.1) as u32 // Extra 10% during bursts
                } else {
                    0
                };
                total + burst_extra
            }
            ProtocolHint::Bulk => {
                // Less aggressive: we can retransmit
                (total as f64 * 0.7) as u32
            }
            ProtocolHint::Auto => total,
        };

        // Cap at max overhead
        let max_repair = (k as f64 * self.max_overhead) as u32;
        let result = total.min(max_repair);

        debug!(
            k,
            p,
            feedforward,
            correction,
            result,
            "computed repair count"
        );

        result
    }

    /// Feedforward: compute repair count from binomial model.
    ///
    /// We want: P(received < k(1+ε)) ≤ δ
    /// where received ~ Binomial(n, 1-p), n = k + r
    ///
    /// Normal approximation: need n such that
    ///   n(1-p) - z_δ·√(n·p·(1-p)) ≥ k(1+ε)
    ///
    /// Solving for n (quadratic in √n):
    ///   n ≈ (k(1+ε) + z²·p(1-p)/2 + z·√(k(1+ε)·p + z²·p²(1-p)²/4)) / (1-p)
    ///
    /// Then r = n - k.
    fn feedforward_repair(&self, k: u32, p: f64) -> u32 {
        let kf = k as f64;
        let eps = self.rq_overhead;
        let target = kf * (1.0 + eps); // symbols needed to decode
        let q = 1.0 - p;

        // z-score for target tail loss
        let z = normal_quantile_upper(self.target_tail_loss);

        // Quadratic solution for n
        let _z2 = z * z;
        let pq = p * q;

        // Iterative refinement (Newton's method on the normal CDF constraint)
        // Start with the simple estimate
        let mut n = target / q + z * (target * p / q).sqrt();

        for _ in 0..5 {
            let mean = n * q;
            let std = (n * pq).sqrt();
            let current_quantile = mean - z * std;
            let gap = target - current_quantile;

            // Derivative of (n*q - z*sqrt(n*p*q)) w.r.t. n
            let deriv = q - z * pq / (2.0 * (n * pq).sqrt());
            if deriv.abs() < 1e-12 {
                break;
            }
            n += gap / deriv;
            n = n.max(kf); // n must be at least k
        }

        let r = (n - kf).ceil() as u32;
        r.max(0)
    }

    /// Update the PI controller with actual block decode results.
    /// Call this after each block decode attempt.
    pub fn feedback_update(&mut self, block_succeeded: bool) {
        // Track actual failure rate
        let sample = if block_succeeded { 0.0 } else { 1.0 };
        let alpha = 0.05; // slow EWMA for failure rate
        self.actual_failure_rate =
            alpha * sample + (1.0 - alpha) * self.actual_failure_rate;

        // Error: actual failure rate vs target
        let error = self.actual_failure_rate - self.target_tail_loss;

        // PI update
        self.integral_error += error;
        // Anti-windup: clamp integral
        self.integral_error = self.integral_error.clamp(-10.0, 10.0);

        self.pi_correction = self.kp * error + self.ki * self.integral_error;
        self.prev_actual_tail_loss = self.actual_failure_rate;

        debug!(
            actual_failure_rate = self.actual_failure_rate,
            error,
            pi_correction = self.pi_correction,
            "PI controller update"
        );
    }

    /// Get diagnostics for monitoring.
    pub fn diagnostics(&self) -> FecDiagnostics {
        FecDiagnostics {
            actual_failure_rate: self.actual_failure_rate,
            pi_correction: self.pi_correction,
            integral_error: self.integral_error,
            target_tail_loss: self.target_tail_loss,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FecDiagnostics {
    pub actual_failure_rate: f64,
    pub pi_correction: f64,
    pub integral_error: f64,
    pub target_tail_loss: f64,
}

/// Standard normal upper quantile: returns z such that P(Z > z) = p.
fn normal_quantile_upper(p: f64) -> f64 {
    // P(Z > z) = p means P(Z < z) = 1-p
    normal_quantile(1.0 - p)
}

fn normal_quantile(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    if (p - 0.5).abs() < 1e-12 {
        return 0.0;
    }

    let (sign, q) = if p < 0.5 { (-1.0, p) } else { (1.0, 1.0 - p) };
    let t = (-2.0 * q.ln()).sqrt();

    let c0 = 2.515517;
    let c1 = 0.802853;
    let c2 = 0.010328;
    let d1 = 1.432788;
    let d2 = 0.189269;
    let d3 = 0.001308;

    let num = c0 + c1 * t + c2 * t * t;
    let den = 1.0 + d1 * t + d2 * t * t + d3 * t * t * t;

    sign * (t - num / den)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::estimator::LossEstimator;

    #[test]
    fn test_zero_loss_no_repair() {
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);
        let mut est = LossEstimator::new();
        // Feed some zero-loss observations to overcome the weak prior
        for _ in 0..50 {
            est.record_batch(100, 100);
        }
        let r = ctrl.compute_repair_count(100, &est);
        assert!(r < 5, "Expected minimal repair for zero loss, got {r}");
    }

    #[test]
    fn test_high_loss_more_repair() {
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 80); // 20% loss
        }
        let r = ctrl.compute_repair_count(100, &est);
        assert!(r >= 20, "Expected significant repair for 20% loss, got {r}");
        assert!(r <= 50, "Repair should be capped at max overhead, got {r}");
    }

    #[test]
    fn test_pi_correction() {
        let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);
        // Simulate repeated failures
        for _ in 0..20 {
            ctrl.feedback_update(false);
        }
        assert!(
            ctrl.pi_correction > 0.0,
            "PI should correct upward on failures"
        );
    }

    #[test]
    fn test_protocol_hint_realtime_more_aggressive() {
        let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime);
        let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk);

        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let r_rt = ctrl_rt.compute_repair_count(100, &est);
        let r_bulk = ctrl_bulk.compute_repair_count(100, &est);
        assert!(
            r_rt >= r_bulk,
            "Realtime ({r_rt}) should use >= repair than bulk ({r_bulk})"
        );
    }
}
