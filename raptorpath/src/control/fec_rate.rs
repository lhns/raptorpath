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

/// Taper function: time-decaying correction density τ(t) = A × (1-q)^t.
///
/// Matches the GE burst survival function — more corrections where loss is
/// likely (right after a burst), fewer as time passes. The amplitude A is
/// derived from the optimal correction rate r* and the GE parameter q.
///
/// See paper Section 4 (The Taper Function).
#[derive(Debug, Clone)]
pub struct TaperFunction {
    /// Taper amplitude: peak correction density at t=0.
    /// A = r* × q, where r* is the optimal correction rate.
    pub amplitude: f64,
    /// Decay base: (1-q) where q = P(Bad→Good) from GE model.
    /// Each time step, the density multiplies by this factor.
    pub decay: f64,
    /// Total correction rate: A / q = r*.
    pub total_rate: f64,
    /// The GE parameter q (for reference).
    pub q: f64,
}

impl TaperFunction {
    /// Create a taper function from the estimator and rate controller.
    ///
    /// Uses the GE model's q parameter for the decay shape and the
    /// optimal correction rate r* for the amplitude.
    pub fn from_estimator(estimator: &LossEstimator, rate: f64) -> Self {
        let ge = estimator.ge_estimator();
        let q = if ge.is_valid() {
            ge.p_bg().clamp(0.01, 1.0) // q = P(Bad→Good)
        } else {
            0.5 // Default: mean burst length = 2
        };

        let amplitude = rate * q;
        let decay = 1.0 - q;

        Self {
            amplitude,
            decay,
            total_rate: rate,
            q,
        }
    }

    /// Correction density at time offset t (in symbol intervals).
    ///
    /// Returns τ(t) = A × (1-q)^t — the number of correction symbols
    /// to generate per source symbol at offset t.
    pub fn density(&self, t: f64) -> f64 {
        self.amplitude * self.decay.powf(t)
    }

    /// Whether to generate a correction symbol at this offset,
    /// using the taper density as a probability.
    ///
    /// For densities > 1.0 (high loss), always generate.
    /// For densities < 1.0, probabilistic based on density.
    pub fn should_generate(&self, t: f64, rng_value: f64) -> bool {
        let d = self.density(t);
        if d >= 1.0 {
            true
        } else {
            rng_value < d
        }
    }
}

/// Compute P_lost(t): probability a symbol was lost given no ACK after time t.
///
/// Uses Bayes' theorem with the channel loss rate as prior:
///   P_lost(t) = ε / [ε + (1-ε) × P(RTT > t)]
///
/// where P(RTT > t) is the normal survival function.
///
/// Returns a value in [ε, 1.0] that smoothly transitions from "probably fine"
/// (t << SRTT) to "certainly lost" (t >> SRTT).
///
/// See paper Section 3.4 (Recovery Latency and the P_lost(t) Model).
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    if epsilon >= 1.0 {
        return 1.0;
    }
    if epsilon <= 0.0 {
        return 0.0;
    }

    // P(RTT > t) using normal survival function
    let rttvar = rttvar_secs.max(0.001); // avoid division by zero
    let z = (age_secs - srtt_secs) / rttvar;
    // Phi(-z) = P(RTT > t) for normal distribution
    let p_rtt_exceeds = normal_survival(z);

    // Bayes: P(lost | no ACK at t) = ε / [ε + (1-ε) × P(RTT > t)]
    let denom = epsilon + (1.0 - epsilon) * p_rtt_exceeds;
    if denom < 1e-300 {
        return 1.0;
    }
    (epsilon / denom).clamp(0.0, 1.0)
}

/// Standard normal survival function: P(Z > z) = 1 - Φ(z).
/// Uses the same rational approximation as normal_quantile.
fn normal_survival(z: f64) -> f64 {
    // For large positive z, survival is tiny
    if z > 8.0 {
        return 0.0;
    }
    if z < -8.0 {
        return 1.0;
    }

    // Use the error function approximation
    // Φ(z) ≈ 0.5 × (1 + erf(z/√2))
    // P(Z > z) = 1 - Φ(z) = 0.5 × erfc(z/√2)
    //
    // Abramowitz & Stegun approximation for erfc:
    let x = z / std::f64::consts::SQRT_2;
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t * (0.254829592
        + t * (-0.284496736
            + t * (1.421413741
                + t * (-1.453152027
                    + t * 1.061405429))));
    let erfc_ax = poly * (-ax * ax).exp();

    if x >= 0.0 {
        0.5 * erfc_ax
    } else {
        1.0 - 0.5 * erfc_ax
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

    // --- Taper function tests ---

    #[test]
    fn test_taper_density_decays() {
        let taper = TaperFunction {
            amplitude: 0.04,
            decay: 0.5, // q=0.5, mean burst = 2
            total_rate: 0.08,
            q: 0.5,
        };

        let d0 = taper.density(0.0);
        let d1 = taper.density(1.0);
        let d2 = taper.density(2.0);

        assert!((d0 - 0.04).abs() < 1e-10, "density(0) = amplitude");
        assert!((d1 - 0.02).abs() < 1e-10, "density(1) = A * 0.5");
        assert!((d2 - 0.01).abs() < 1e-10, "density(2) = A * 0.25");
        assert!(d0 > d1, "density decays");
        assert!(d1 > d2, "density decays monotonically");
    }

    #[test]
    fn test_taper_from_estimator() {
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90); // 10% loss
        }

        let rate = 0.12; // 12% correction rate
        let taper = TaperFunction::from_estimator(&est, rate);

        assert!(taper.amplitude > 0.0, "amplitude should be positive");
        assert!(taper.decay > 0.0 && taper.decay < 1.0, "decay in (0,1)");
        assert!((taper.total_rate - rate).abs() < 1e-10, "total rate preserved");
        assert!((taper.amplitude - rate * taper.q).abs() < 1e-10, "A = r * q");
    }

    #[test]
    fn test_taper_total_rate_geometric_sum() {
        // The geometric series sum of τ(t) for t=0..∞ should equal total_rate
        let taper = TaperFunction {
            amplitude: 0.06,
            decay: 0.7, // q=0.3
            total_rate: 0.06 / 0.3,
            q: 0.3,
        };

        // Sum first 1000 terms (approximates infinite sum)
        let sum: f64 = (0..1000).map(|t| taper.density(t as f64)).sum();
        assert!(
            (sum - taper.total_rate).abs() < 0.001,
            "geometric sum ≈ A/q = total_rate: sum={sum}, expected={}",
            taper.total_rate
        );
    }

    // --- P_lost tests ---

    #[test]
    fn test_p_lost_at_zero() {
        // At t=0, P_lost ≈ ε (just the base loss rate)
        let p = p_lost(0.0, 0.025, 0.050, 0.005);
        assert!(
            (p - 0.025).abs() < 0.005,
            "P_lost(0) ≈ ε: got {p}"
        );
    }

    #[test]
    fn test_p_lost_at_srtt() {
        // At t = SRTT, P_lost should be elevated (ACK expected by now)
        let p = p_lost(0.050, 0.025, 0.050, 0.005);
        assert!(
            p > 0.025 * 1.5,
            "P_lost(SRTT) should be well above ε: got {p}"
        );
    }

    #[test]
    fn test_p_lost_at_large_t() {
        // At t >> SRTT, P_lost → 1.0
        let p = p_lost(0.200, 0.025, 0.050, 0.005);
        assert!(
            p > 0.95,
            "P_lost(4×SRTT) should be near 1.0: got {p}"
        );
    }

    #[test]
    fn test_p_lost_monotone() {
        // P_lost should increase with age
        let srtt = 0.050;
        let rttvar = 0.005;
        let eps = 0.025;

        let p1 = p_lost(0.01, eps, srtt, rttvar);
        let p2 = p_lost(0.03, eps, srtt, rttvar);
        let p3 = p_lost(0.05, eps, srtt, rttvar);
        let p4 = p_lost(0.10, eps, srtt, rttvar);

        assert!(p1 < p2, "P_lost should increase: {p1} < {p2}");
        assert!(p2 < p3, "P_lost should increase: {p2} < {p3}");
        assert!(p3 < p4, "P_lost should increase: {p3} < {p4}");
    }

    #[test]
    fn test_p_lost_high_epsilon() {
        // With high loss rate, P_lost(0) should be high
        let p = p_lost(0.0, 0.5, 0.050, 0.005);
        assert!(
            (p - 0.5).abs() < 0.01,
            "P_lost(0) with ε=0.5 should be ~0.5: got {p}"
        );
    }

    #[test]
    fn test_normal_survival_basic() {
        // P(Z > 0) = 0.5
        let s = normal_survival(0.0);
        assert!((s - 0.5).abs() < 0.01, "survival(0) ≈ 0.5: got {s}");

        // P(Z > 2) ≈ 0.0228
        let s2 = normal_survival(2.0);
        assert!((s2 - 0.0228).abs() < 0.005, "survival(2) ≈ 0.023: got {s2}");

        // P(Z > -2) ≈ 0.9772
        let sm2 = normal_survival(-2.0);
        assert!((sm2 - 0.9772).abs() < 0.005, "survival(-2) ≈ 0.977: got {sm2}");
    }
}
