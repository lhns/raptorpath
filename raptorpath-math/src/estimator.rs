//! Loss rate estimation using Bayesian EWMA + BOCD.
//!
//! Tick-based version for wasm (no std::time::Instant dependency).

use crate::changepoint::BayesianChangepoint;
use crate::gilbert_elliott::GilbertElliottEstimator;

/// Per-path loss estimator (tick-based, wasm-compatible).
#[derive(Debug)]
pub struct LossEstimator {
    /// EWMA of TX loss rate
    tx_ewma_loss: f64,
    alpha: f64,
    /// Beta distribution parameters
    beta_a: f64,
    beta_b: f64,
    beta_decay: f64,
    /// Burst loss detection
    consecutive_losses: u32,
    burst_threshold: u32,
    in_burst: bool,
    /// Gilbert-Elliott HMM
    ge: GilbertElliottEstimator,
    /// BOCD for regime-aware prediction
    bocd: BayesianChangepoint,
    /// Reorder tracking for sequence-aware P_lost
    reorder_count: u64,
    total_arrivals: u64,
    /// Bookkeeping
    total_sent: u64,
    total_received: u64,
    last_update_tick: u64,
}

impl LossEstimator {
    pub fn new() -> Self {
        Self {
            tx_ewma_loss: 0.0,
            alpha: 0.1,
            beta_a: 1.0,
            beta_b: 1.0,
            beta_decay: 0.995,
            consecutive_losses: 0,
            burst_threshold: 3,
            in_burst: false,
            ge: GilbertElliottEstimator::new(),
            bocd: BayesianChangepoint::default_fec(),
            reorder_count: 0,
            total_arrivals: 0,
            total_sent: 0,
            total_received: 0,
            last_update_tick: 0,
        }
    }

    /// Record that `received` out of `sent` symbols arrived in a batch.
    pub fn record_batch(&mut self, sent: u32, received: u32, tick: u64) {
        let lost = sent.saturating_sub(received);
        let batch_loss = if sent > 0 { lost as f64 / sent as f64 } else { 0.0 };

        self.tx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.tx_ewma_loss;

        self.beta_a *= self.beta_decay;
        self.beta_b *= self.beta_decay;
        self.beta_a += received as f64;
        self.beta_b += lost as f64;

        self.bocd.update(received, lost);

        if lost > 0 {
            self.consecutive_losses += lost;
            if self.consecutive_losses >= self.burst_threshold {
                self.in_burst = true;
            }
        } else {
            self.consecutive_losses = 0;
            self.in_burst = false;
        }

        // Replay the batch to the GE estimator as all-losses-then-all-receives.
        // The true interleaving is unknown from (sent, received) counts alone;
        // lumping losses assumes maximal burstiness, which biases q̂ down /
        // burst length up — the CONSERVATIVE direction (more burst margin).
        // Callers with per-symbol loss patterns (SACK gaps) should feed
        // ge.record_symbol() directly in arrival order instead.
        for _ in 0..lost { self.ge.record_symbol(false); }
        for _ in 0..received { self.ge.record_symbol(true); }

        self.total_sent += sent as u64;
        self.total_received += received as u64;
        self.last_update_tick = tick;
    }

    pub fn loss_rate(&self) -> f64 { self.tx_ewma_loss }

    pub fn loss_rate_upper(&self, confidence: f64) -> f64 {
        beta_quantile(self.beta_b, self.beta_a, confidence)
    }

    pub fn predictive_loss_upper(&self, confidence: f64) -> f64 {
        if self.bocd.updates() < 5 {
            return self.loss_rate_upper(confidence);
        }
        self.bocd.predictive_quantile(confidence)
    }

    pub fn loss_variance(&self) -> f64 {
        let (a, b) = (self.beta_a, self.beta_b);
        (a * b) / ((a + b).powi(2) * (a + b + 1.0))
    }

    pub fn loss_rate_mean(&self) -> f64 {
        self.beta_b / (self.beta_a + self.beta_b)
    }

    pub fn total_sent(&self) -> u64 { self.total_sent }
    pub fn ge_estimator(&self) -> &GilbertElliottEstimator { &self.ge }
    pub fn bocd(&self) -> &BayesianChangepoint { &self.bocd }
    pub fn is_in_burst(&self) -> bool { self.in_burst }

    /// Record an out-of-order arrival (for reorder rate estimation).
    pub fn record_reorder(&mut self) { self.reorder_count += 1; self.total_arrivals += 1; }
    /// Record an in-order arrival.
    pub fn record_in_order_arrival(&mut self) { self.total_arrivals += 1; }
    /// Current reorder rate estimate.
    pub fn reorder_rate(&self) -> f64 {
        if self.total_arrivals == 0 { 0.0 }
        else { self.reorder_count as f64 / self.total_arrivals as f64 }
    }
}

impl Default for LossEstimator {
    fn default() -> Self { Self::new() }
}

/// Beta distribution quantile (normal approximation).
fn beta_quantile(a: f64, b: f64, p: f64) -> f64 {
    let mean = a / (a + b);
    let var = (a * b) / ((a + b).powi(2) * (a + b + 1.0));
    let z = crate::normal_quantile(p);
    (mean + z * var.sqrt()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loss_estimator_basic() {
        let mut est = LossEstimator::new();
        for i in 0..100 {
            est.record_batch(100, 90, i);
        }
        let loss = est.loss_rate();
        assert!((loss - 0.1).abs() < 0.02, "Expected ~10% loss, got {loss}");
    }

    #[test]
    fn test_predictive_loss_upper() {
        let mut est = LossEstimator::new();
        for i in 0..100 {
            est.record_batch(100, 90, i);
        }
        let pred = est.predictive_loss_upper(0.95);
        assert!(pred > 0.08 && pred < 0.25, "Predictive upper: {pred}");
    }

    #[test]
    fn test_burst_detection() {
        let mut est = LossEstimator::new();
        est.record_batch(10, 7, 0);
        assert!(est.is_in_burst());
        est.record_batch(10, 10, 1);
        assert!(!est.is_in_burst());
    }
}
