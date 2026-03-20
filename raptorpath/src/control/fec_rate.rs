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
use crate::fec::FecBackend;
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
    /// Codec decode overhead factor
    rq_overhead: f64,
    /// Protocol hint
    hint: ProtocolHint,
    /// Symbol size in bytes (needed to compute T = RTT × throughput / symbol_size)
    symbol_size: u16,

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
    /// Whether PI feedback loop is enabled
    enable_pi_feedback: bool,
}

impl FecRateController {
    /// Create a new FecRateController with default feature toggles (all enabled).
    pub fn new(target_tail_loss: f64, max_overhead: f64, hint: ProtocolHint, backend: FecBackend, symbol_size: u16) -> Self {
        Self::new_with_toggles(target_tail_loss, max_overhead, hint, backend, true, symbol_size)
    }

    /// Create a new FecRateController with explicit feature toggles.
    pub fn new_with_toggles(
        target_tail_loss: f64,
        max_overhead: f64,
        hint: ProtocolHint,
        backend: FecBackend,
        enable_pi_feedback: bool,
        symbol_size: u16,
    ) -> Self {
        // METTLE requires significantly more overhead than RaptorQ for reliable decoding,
        // especially at small window sizes (w=50).
        let codec_overhead = match backend {
            FecBackend::RaptorQ => 0.01,  // RaptorQ decodes with <1% overhead
            FecBackend::Mettle => 0.15,   // METTLE needs ~5-25%; 15% is conservative
            FecBackend::ReedSolomon => 0.0, // MDS: zero overhead, any k of n suffices
            FecBackend::Rlc => 0.004,     // Near-MDS: ~0.4% overhead (GF(256) random matrix)
            FecBackend::Streaming => 0.0, // Streaming codes: rate-optimal, no systematic overhead
        };
        Self {
            target_tail_loss,
            max_overhead,
            rq_overhead: codec_overhead,
            hint,
            symbol_size,
            integral_error: 0.0,
            kp: 0.5,
            ki: 0.1,
            pi_correction: 0.0,
            prev_actual_tail_loss: 0.0,
            actual_failure_rate: 0.0,
            enable_pi_feedback,
        }
    }

    /// Compute the number of repair symbols needed for `k` source symbols
    /// given the current loss estimate from `estimator`.
    pub fn compute_repair_count(&self, k: u32, estimator: &LossEstimator) -> u32 {
        let rate = self.compute_repair_rate(estimator);
        (k as f64 * rate).ceil() as u32
    }

    /// Update the PI controller with actual block decode results.
    /// Call this after each block decode attempt.
    pub fn feedback_update(&mut self, block_succeeded: bool) {
        if !self.enable_pi_feedback {
            return;
        }
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

    /// Update the PI controller from window-mode decoder stats.
    ///
    /// Call periodically (e.g., every REPORT_INTERVAL) from the window receive loop.
    /// Uses repair efficiency (useful/fed ratio) as a proxy for the binary
    /// success/failure signal that block mode provides.
    pub fn feedback_update_window(&mut self, repairs_fed: u64, repairs_useful: u64) {
        if !self.enable_pi_feedback || repairs_fed == 0 {
            return;
        }
        // Useful ratio > 0.5 means most repairs were needed → under-provisioned
        // Useful ratio ≤ 0.5 means comfortable margin → not under-provisioned
        let useful_ratio = repairs_useful as f64 / repairs_fed as f64;
        let not_under_provisioned = useful_ratio <= 0.5;
        self.feedback_update(not_under_provisioned);
    }

    /// Compute the repair rate for sliding-window mode: how many repair symbols
    /// to generate per source symbol. E.g., 0.1 = 1 repair per 10 source symbols.
    ///
    /// Uses information-theoretic optimal formula (ADR-0043):
    ///   rate = max(p/(1-p) + codec_overhead, B/T) × (1 + margin) + pi + hint_offset
    ///
    /// Where T = (RTT × throughput) / symbol_size accounts for RTT naturally:
    /// - Low RTT → large T → random-loss term dominates → low overhead
    /// - High RTT → small T → burst term dominates → more proactive FEC
    pub fn compute_repair_rate(&self, estimator: &LossEstimator) -> f64 {
        // Adaptive confidence: after 500+ samples the posterior is tight; 85th percentile suffices
        let confidence = if estimator.total_sent() > 500 { 0.85 } else { 0.95 };
        let p = estimator.loss_rate_upper(confidence);
        if p < 1e-10 {
            return 0.0;
        }

        // --- Random loss term: information-theoretic minimum ---
        let random_rate = p / (1.0 - p) + self.rq_overhead;

        // --- Burst loss term: delay-constrained capacity B/T ---
        // Only applies when we have both a valid GE model and throughput data.
        // Without throughput data, T is undefined — fall back to random-loss only.
        let ge = estimator.ge_estimator();
        let burst_rate = if ge.is_valid() && estimator.throughput() > 0.0 {
            let burst_length = ge.mean_burst_length().max(1.0);
            let rtt_secs = estimator.rtt().as_secs_f64();
            let t_symbols = (rtt_secs * estimator.throughput() / self.symbol_size as f64).max(1.0);
            burst_length / t_symbols
        } else {
            0.0
        };

        // --- Optimal rate: max of random and burst ---
        let base_rate = random_rate.max(burst_rate);

        // --- Single safety margin from estimation uncertainty ---
        let z = normal_quantile_upper(self.target_tail_loss);
        let uncertainty = estimator.loss_uncertainty(0.95);
        let margin = (z * uncertainty * 0.25).clamp(0.0, 1.0);

        // --- PI feedback correction (reduced gains) ---
        let pi = if self.enable_pi_feedback { self.pi_correction.max(0.0) } else { 0.0 };

        // --- Protocol hint: additive offset, not multiplicative ---
        // Reduced from 0.05 to 0.02: 5% was too aggressive for low-loss scenarios
        let hint_offset = match self.hint {
            ProtocolHint::Realtime => 0.02,
            ProtocolHint::Bulk => -0.02,
            ProtocolHint::Auto => 0.0,
        };

        let rate = base_rate * (1.0 + margin) + pi + hint_offset;
        rate.clamp(0.0, self.max_overhead)
    }

    /// Compute streaming code parameters from the current loss estimator.
    ///
    /// Returns `StreamingParams` with T, B, ε derived from the channel model:
    /// - B: from Gilbert-Elliott mean burst length × safety factor
    /// - ε: from upper-bound loss rate
    /// - T: set to B (delay constraint = burst tolerance)
    pub fn compute_streaming_params(
        &self,
        estimator: &LossEstimator,
    ) -> crate::fec::StreamingParams {
        let ge = estimator.ge_estimator();
        let burst_length = if ge.is_valid() {
            ge.mean_burst_length().max(1.0)
        } else {
            2.0 // Default assumption: short bursts
        };

        let loss_rate = estimator.loss_rate_upper(0.95);

        // Safety factor: 10% over-provisioning for Realtime, 5% otherwise (ADR-0035)
        let safety = match self.hint {
            ProtocolHint::Realtime => 1.10,
            _ => 1.05,
        };

        crate::fec::StreamingParams::from_channel(burst_length, loss_rate, safety)
    }

    /// Update the codec overhead for a new backend (used during runtime switching).
    pub fn update_backend(&mut self, backend: FecBackend) {
        self.rq_overhead = match backend {
            FecBackend::RaptorQ => 0.01,
            FecBackend::Mettle => 0.15,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
            FecBackend::Streaming => 0.0,
        };
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
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
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
        let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
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
        let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
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
        let ctrl_rt = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);
        let ctrl_bulk = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::RaptorQ, 1200);

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
