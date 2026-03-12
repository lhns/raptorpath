//! Loss rate estimation using Bayesian EWMA.
//!
//! Combines:
//! - Beta-Binomial conjugate prior for principled uncertainty quantification
//! - EWMA for fast adaptation to changing conditions
//! - Burst detection for non-iid loss patterns

use std::time::{Duration, Instant};

/// Per-path loss estimator.
#[derive(Debug)]
pub struct LossEstimator {
    /// EWMA of loss rate
    ewma_loss: f64,
    /// EWMA smoothing factor (higher = more responsive)
    alpha: f64,
    /// Beta distribution parameters (Bayesian prior)
    beta_a: f64, // successes (received)
    beta_b: f64, // failures (lost)
    /// Decay factor for Beta params to forget old data
    beta_decay: f64,

    /// RTT estimation (EWMA)
    ewma_rtt: Duration,
    rtt_alpha: f64,

    /// Throughput estimation (bytes/sec EWMA)
    ewma_throughput: f64,

    /// Burst loss detection
    consecutive_losses: u32,
    burst_threshold: u32,
    in_burst: bool,

    /// Bookkeeping
    total_sent: u64,
    total_received: u64,
    last_update: Instant,
}

impl LossEstimator {
    pub fn new() -> Self {
        Self {
            ewma_loss: 0.0,
            alpha: 0.1, // ~10-sample half-life
            // Weak prior: Beta(1,1) = uniform
            beta_a: 1.0,
            beta_b: 1.0,
            beta_decay: 0.995, // slowly forget old observations
            ewma_rtt: Duration::from_millis(50),
            rtt_alpha: 0.125, // standard TCP EWMA
            ewma_throughput: 0.0,
            consecutive_losses: 0,
            burst_threshold: 3,
            in_burst: false,
            total_sent: 0,
            total_received: 0,
            last_update: Instant::now(),
        }
    }

    /// Record that `received` out of `sent` symbols arrived in a batch.
    pub fn record_batch(&mut self, sent: u32, received: u32) {
        let lost = sent.saturating_sub(received);
        let batch_loss = if sent > 0 {
            lost as f64 / sent as f64
        } else {
            0.0
        };

        // EWMA update
        self.ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.ewma_loss;

        // Beta-Binomial update with decay
        self.beta_a *= self.beta_decay;
        self.beta_b *= self.beta_decay;
        self.beta_a += received as f64;
        self.beta_b += lost as f64;

        // Burst detection
        if lost > 0 {
            self.consecutive_losses += lost;
            if self.consecutive_losses >= self.burst_threshold {
                self.in_burst = true;
            }
        } else {
            self.consecutive_losses = 0;
            self.in_burst = false;
        }

        self.total_sent += sent as u64;
        self.total_received += received as u64;
        self.last_update = Instant::now();
    }

    /// Record an RTT measurement.
    pub fn record_rtt(&mut self, rtt: Duration) {
        let rtt_secs = rtt.as_secs_f64();
        let old_secs = self.ewma_rtt.as_secs_f64();
        let new_secs = self.rtt_alpha * rtt_secs + (1.0 - self.rtt_alpha) * old_secs;
        self.ewma_rtt = Duration::from_secs_f64(new_secs);
    }

    /// Record throughput measurement.
    pub fn record_throughput(&mut self, bytes_per_sec: f64) {
        self.ewma_throughput =
            self.rtt_alpha * bytes_per_sec + (1.0 - self.rtt_alpha) * self.ewma_throughput;
    }

    /// Current loss rate estimate (point estimate).
    pub fn loss_rate(&self) -> f64 {
        self.ewma_loss
    }

    /// Upper bound of loss rate at given confidence level.
    /// Uses the Beta posterior: quantile at (1 - confidence).
    /// This is what we use for computing FEC rate — we want to be conservative.
    pub fn loss_rate_upper(&self, confidence: f64) -> f64 {
        beta_quantile(self.beta_b, self.beta_a, confidence)
    }

    /// Variance of loss estimate (from Beta posterior).
    pub fn loss_variance(&self) -> f64 {
        let a = self.beta_a;
        let b = self.beta_b;
        (a * b) / ((a + b).powi(2) * (a + b + 1.0))
    }

    pub fn rtt(&self) -> Duration {
        self.ewma_rtt
    }

    pub fn throughput(&self) -> f64 {
        self.ewma_throughput
    }

    pub fn is_in_burst(&self) -> bool {
        self.in_burst
    }

    pub fn time_since_update(&self) -> Duration {
        self.last_update.elapsed()
    }
}

impl Default for LossEstimator {
    fn default() -> Self {
        Self::new()
    }
}

/// Approximate Beta distribution quantile using the normal approximation.
/// For Beta(a, b), mean = a/(a+b), var = ab/((a+b)^2(a+b+1))
/// Returns the `p`-th quantile.
fn beta_quantile(a: f64, b: f64, p: f64) -> f64 {
    let mean = a / (a + b);
    let var = (a * b) / ((a + b).powi(2) * (a + b + 1.0));
    let std = var.sqrt();

    // Normal approximation: quantile ≈ mean + z_p * std
    let z = normal_quantile(p);
    (mean + z * std).clamp(0.0, 1.0)
}

/// Standard normal quantile (rational approximation, Abramowitz & Stegun).
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

    // Rational approximation
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

    #[test]
    fn test_loss_estimator_basic() {
        let mut est = LossEstimator::new();

        // Simulate 10% loss
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let loss = est.loss_rate();
        assert!((loss - 0.1).abs() < 0.02, "Expected ~10% loss, got {loss}");
    }

    #[test]
    fn test_loss_upper_bound() {
        let mut est = LossEstimator::new();
        for _ in 0..50 {
            est.record_batch(100, 90);
        }

        let upper = est.loss_rate_upper(0.95);
        assert!(upper > est.loss_rate(), "Upper bound should exceed point estimate");
        assert!(upper < 0.3, "Upper bound should be reasonable: {upper}");
    }

    #[test]
    fn test_burst_detection() {
        let mut est = LossEstimator::new();
        est.record_batch(10, 7); // 3 losses
        assert!(est.is_in_burst());
        est.record_batch(10, 10); // no loss
        assert!(!est.is_in_burst());
    }
}
