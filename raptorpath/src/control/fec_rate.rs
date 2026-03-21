//! FEC rate controller: computes optimal repair symbol count.
//!
//! Uses a principled budget architecture (ADR-0050):
//!
//! 1. **Predictive upper bound**: BOCD posterior quantile at target confidence
//!    provides the loss rate estimate WITH built-in uncertainty margin.
//!    No separate PI controller or margin multiplication needed.
//!
//! 2. **Protocol hint → tail reliability**: The protocol hint (Realtime/Bulk/Auto)
//!    maps to `target_tail_loss`, controlling the FEC/NACK balance. Tighter tail
//!    = more proactive FEC = less NACK latency. No magic additive offsets.
//!
//! 3. **Budget allocation**: Total repair budget is split between proactive FEC
//!    and NACK-based reactive repair, coordinated to avoid double-spending.
//!
//! 4. **Spare capacity gate**: Repair rate is clamped to spare link capacity
//!    (cwnd - in_flight) to ensure FEC never causes congestion.

use super::estimator::LossEstimator;
use crate::fec::FecBackend;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Protocol hint controls the latency/tail-reliability tradeoff.
///
/// The system targets 100% reliability — everything gets through via FEC or NACK.
/// FEC is proactive (zero added latency), NACK is reactive (costs one RTT).
/// The protocol hint controls WHERE on the latency/reliability curve we sit
/// by adjusting `target_tail_loss`:
///
/// - **Realtime**: 100× tighter tail → very aggressive FEC → minimal NACK latency
/// - **Bulk**: 100× looser tail → less FEC → rely on NACK → saves bandwidth
/// - **Auto**: unchanged target → balanced
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProtocolHint {
    /// Real-time traffic (VoIP, gaming): tighter tail loss → more proactive FEC
    Realtime,
    /// Bulk transfer: looser tail loss → rely on NACK for residual
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
    /// Effective target tail loss probability (after hint adjustment)
    target_tail_loss: f64,
    /// Maximum FEC overhead as fraction of source symbols
    max_overhead: f64,
    /// Codec decode overhead factor (raw, before P(decoder_invoked) weighting)
    rq_overhead: f64,
    /// Protocol hint (stored for streaming params and diagnostics)
    hint: ProtocolHint,
    /// Symbol size in bytes (needed to compute T = RTT × throughput / symbol_size)
    symbol_size: u16,
}

impl FecRateController {
    /// Create a new FecRateController with default feature toggles (all enabled).
    pub fn new(target_tail_loss: f64, max_overhead: f64, hint: ProtocolHint, backend: FecBackend, symbol_size: u16) -> Self {
        Self::new_with_toggles(target_tail_loss, max_overhead, hint, backend, true, symbol_size)
    }

    /// Create a new FecRateController with explicit feature toggles.
    ///
    /// `enable_pi_feedback` is accepted for API compatibility but ignored —
    /// BOCD posterior quantile replaces the PI controller entirely.
    ///
    /// The protocol hint adjusts `target_tail_loss` to control the
    /// FEC/NACK balance (latency vs bandwidth tradeoff):
    /// - Realtime: 100× tighter (more FEC, less NACK latency)
    /// - Bulk: 100× looser (less FEC, rely on NACK)
    /// - Auto: unchanged
    pub fn new_with_toggles(
        target_tail_loss: f64,
        max_overhead: f64,
        hint: ProtocolHint,
        backend: FecBackend,
        _enable_pi_feedback: bool,
        symbol_size: u16,
    ) -> Self {
        let codec_overhead = match backend {
            FecBackend::RaptorQ => 0.01,
            FecBackend::Mettle => 0.15,
            FecBackend::ReedSolomon => 0.0,
            FecBackend::Rlc => 0.004,
            FecBackend::Streaming => 0.0,
        };

        // Protocol hint maps to target_tail_loss, not an additive offset.
        // This is the only principled knob: tighter tail = more proactive FEC.
        let effective_tail_loss = match hint {
            ProtocolHint::Realtime => target_tail_loss * 0.01,  // 100× tighter
            ProtocolHint::Bulk => target_tail_loss * 100.0,     // 100× looser
            ProtocolHint::Auto => target_tail_loss,
        };
        let effective_tail_loss = effective_tail_loss.clamp(1e-9, 0.1);

        Self {
            target_tail_loss: effective_tail_loss,
            max_overhead,
            rq_overhead: codec_overhead,
            hint,
            symbol_size,
        }
    }

    /// Compute the number of repair symbols needed for `k` source symbols
    /// given the current loss estimate from `estimator`.
    ///
    /// `window_size`: current encoder window or block size (for codec overhead weighting).
    pub fn compute_repair_count(&self, k: u32, estimator: &LossEstimator, window_size: usize) -> u32 {
        let rate = self.compute_repair_rate(estimator, window_size);
        (k as f64 * rate).ceil() as u32
    }

    /// PI feedback update — no-op in new architecture.
    pub fn feedback_update(&mut self, _block_succeeded: bool) {}

    /// Window-mode PI feedback — no-op in new architecture.
    pub fn feedback_update_window(&mut self, _repairs_fed: u64, _repairs_useful: u64) {}

    /// Compute the repair rate for sliding-window mode: how many repair symbols
    /// to generate per source symbol. E.g., 0.1 = 1 repair per 10 source symbols.
    ///
    /// Uses BOCD predictive quantile as the loss estimate (ADR-0050):
    ///   rate = max(p/(1-p) + effective_codec_overhead, B/T)
    ///
    /// The posterior quantile IS the margin — no separate safety factor needed.
    /// The protocol hint controls tail loss (and thus the quantile), not an offset.
    /// Codec overhead is weighted by P(decoder_invoked) for systematic codecs.
    ///
    /// `window_size`: current encoder window or block size.
    pub fn compute_repair_rate(&self, estimator: &LossEstimator, window_size: usize) -> f64 {
        let confidence = 1.0 - self.target_tail_loss;
        let p = estimator.predictive_loss_upper(confidence);
        if p < 1e-10 {
            return 0.0;
        }

        // --- Codec overhead weighted by P(decoder invoked) ---
        // For systematic codecs, the decoder is only invoked when ≥1 source
        // symbol in the window is lost. No point paying full codec overhead
        // when the decoder runs only a fraction of the time.
        let effective_codec_overhead = if self.rq_overhead > 0.0 && window_size > 0 {
            let p_decoder_invoked = 1.0 - (1.0 - p).powi(window_size as i32);
            self.rq_overhead * p_decoder_invoked
        } else {
            0.0
        };

        // --- Random loss term: information-theoretic minimum ---
        let random_rate = p / (1.0 - p) + effective_codec_overhead;

        // --- Burst loss term: delay-constrained capacity B/T ---
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
        let rate = random_rate.max(burst_rate);
        rate.clamp(0.0, self.max_overhead)
    }

    /// Compute repair rate with spare capacity constraint.
    ///
    /// This is the primary method for the "never hurts" guarantee:
    /// repair_rate ≤ spare_capacity, ensuring FEC never causes congestion.
    pub fn compute_repair_rate_capped(
        &self,
        estimator: &LossEstimator,
        spare_capacity: f64,
        window_size: usize,
    ) -> f64 {
        let rate = self.compute_repair_rate(estimator, window_size);
        rate.min(spare_capacity.max(0.0))
    }

    /// Compute streaming code parameters from the current loss estimator.
    pub fn compute_streaming_params(
        &self,
        estimator: &LossEstimator,
    ) -> crate::fec::StreamingParams {
        let ge = estimator.ge_estimator();
        let burst_length = if ge.is_valid() {
            ge.mean_burst_length().max(1.0)
        } else {
            2.0
        };

        let loss_rate = estimator.predictive_loss_upper(0.95);

        // BOCD quantile already provides the uncertainty margin.
        // No per-hint safety factor needed.
        let safety = 1.0;

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
            actual_failure_rate: 0.0,
            pi_correction: 0.0,
            integral_error: 0.0,
            target_tail_loss: self.target_tail_loss,
        }
    }

    /// Get the maximum overhead setting.
    pub fn max_overhead(&self) -> f64 {
        self.max_overhead
    }

    /// Get the raw codec overhead (before P(decoder_invoked) weighting).
    pub fn codec_overhead(&self) -> f64 {
        self.rq_overhead
    }

    /// Get the effective target tail loss probability (after hint adjustment).
    pub fn target_tail_loss(&self) -> f64 {
        self.target_tail_loss
    }
}

/// Joint FEC/NACK budget allocator.
///
/// Splits the total repair budget between proactive FEC and reactive NACK repair,
/// ensuring they don't compete for the same bandwidth.
///
/// Budget conservation: proactive + nack ≤ total_budget ≤ spare_capacity
pub struct BudgetAllocator {
    total_budget: f64,
    nack_expected: f64,
    proactive_budget: f64,
    nack_cap: f64,
}

impl BudgetAllocator {
    /// Compute budget allocation from current estimates.
    pub fn compute(
        p_upper: f64,
        codec_overhead: f64,
        nack_rate: f64,
        nack_effectiveness: f64,
    ) -> Self {
        let total_budget = if p_upper < 1e-10 {
            0.0
        } else {
            p_upper / (1.0 - p_upper) + codec_overhead
        };

        let nack_expected = (nack_rate * nack_effectiveness).min(total_budget);
        let proactive_budget = (total_budget - nack_expected).max(0.0);
        let nack_cap = (total_budget - proactive_budget).max(0.0);

        Self {
            total_budget,
            nack_expected,
            proactive_budget,
            nack_cap,
        }
    }

    pub fn proactive_rate(&self) -> f64 {
        self.proactive_budget
    }

    pub fn nack_cap(&self) -> f64 {
        self.nack_cap
    }

    pub fn total_budget(&self) -> f64 {
        self.total_budget
    }
}

#[derive(Debug, Clone)]
pub struct FecDiagnostics {
    pub actual_failure_rate: f64,
    pub pi_correction: f64,
    pub integral_error: f64,
    pub target_tail_loss: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::estimator::LossEstimator;

    const W: usize = 50; // typical window size for tests

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
        let ctrl_mettle = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::Mettle, 1200);

        let mut est = LossEstimator::new();
        // Very low loss: P(decoder invoked) is small
        for _ in 0..100 {
            est.record_batch(1000, 999); // 0.1% loss
        }

        let rate_small_window = ctrl_mettle.compute_repair_rate(&est, 10);
        let rate_large_window = ctrl_mettle.compute_repair_rate(&est, 200);

        // Larger window = higher P(decoder invoked) = more codec overhead
        assert!(
            rate_large_window > rate_small_window,
            "Larger window should have more codec overhead: small={rate_small_window}, large={rate_large_window}"
        );

        // With zero window size, no codec overhead at all
        let rate_zero = ctrl_mettle.compute_repair_rate(&est, 0);
        let ctrl_rs = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::ReedSolomon, 1200);
        let rate_rs = ctrl_rs.compute_repair_rate(&est, 0);
        assert!(
            (rate_zero - rate_rs).abs() < 0.001,
            "Zero window should have no codec overhead: mettle={rate_zero}, rs={rate_rs}"
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
}
