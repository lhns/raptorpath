//! Bayesian Online Changepoint Detection (BOCD).
//!
//! Implements Adams & MacKay (2007) with Beta-Binomial sufficient statistics.
//! Maintains a run-length distribution P(r_t | data) truncated at MAX_RUN_LENGTH.
//!
//! Key properties:
//! - Steady state: tight posterior → small margin → low overhead
//! - Changepoint: wide posterior → large margin → conservative protection
//! - Prediction: integrates over run-length uncertainty automatically
//!
//! Cost: O(MAX_RUN_LENGTH) per update — ~2 ln + 1 exp per run length plus
//! two Vec allocations and a stats-vector shift. Negligible at the BATCH
//! cadence it was designed for (`default_fec()`: ~2 s intervals); at the
//! window wire's per-message cadence (~22k msgs/s) it measured 22–26% of a
//! core per side (goal-gate "Receiver Per-Message Wall" STEP-1 profile) —
//! `RWM_EST_CADENCE` (control/estimator.rs) restores the design cadence.

/// Maximum run length tracked (truncation point for the distribution).
const MAX_RUN_LENGTH: usize = 200;

/// Sufficient statistics for a Beta-Binomial segment.
#[derive(Debug, Clone)]
struct BetaBinomialStats {
    /// Prior + observed successes (received symbols)
    alpha: f64,
    /// Prior + observed failures (lost symbols)
    beta: f64,
}

impl BetaBinomialStats {
    fn new(alpha: f64, beta: f64) -> Self {
        Self { alpha, beta }
    }

    /// Predictive probability of observing `lost` losses in `total` trials
    /// under this Beta-Binomial model (integrated over the Beta posterior).
    ///
    /// For a single Bernoulli trial: P(loss) = beta / (alpha + beta)
    fn predictive_loss_prob(&self) -> f64 {
        self.beta / (self.alpha + self.beta)
    }

    /// Update stats with new observation.
    fn update(&mut self, received: u32, lost: u32) {
        self.alpha += received as f64;
        self.beta += lost as f64;
    }

    /// Loss rate quantile using normal approximation to the Beta posterior.
    fn loss_quantile(&self, p: f64) -> f64 {
        let a = self.beta; // losses = "successes" in this Beta
        let b = self.alpha; // receives = "failures" in this Beta
        let total = a + b;
        if total < 2.0 {
            return 0.5; // uninformative
        }
        let mean = a / total;
        let var = (a * b) / (total * total * (total + 1.0));
        let std = var.sqrt();
        let z = normal_quantile(p);
        (mean + z * std).clamp(0.0, 1.0)
    }
}

/// Bayesian Online Changepoint Detector.
///
/// Tracks a distribution over run lengths (how long since the last changepoint).
/// Each run length has associated Beta-Binomial sufficient statistics.
/// On each update, the distribution is updated via message passing:
///
/// 1. Growth: each run length grows by 1 (accumulating evidence)
/// 2. Changepoint: probability mass flows to run length 0 (fresh start)
/// 3. Normalization: the distribution is renormalized
#[derive(Debug)]
pub struct BayesianChangepoint {
    /// Run-length probabilities: run_probs[r] = P(run_length = r | data)
    run_probs: Vec<f64>,
    /// Sufficient statistics for each run length
    stats: Vec<BetaBinomialStats>,
    /// Hazard rate: P(changepoint) per time step. Higher = more sensitive.
    /// 1/200 means expected segment length of 200 batches.
    hazard: f64,
    /// Prior parameters for new segments
    prior_alpha: f64,
    prior_beta: f64,
    /// Total updates processed
    updates: u64,
}

impl BayesianChangepoint {
    /// Create a new BOCD detector.
    ///
    /// - `hazard`: probability of a changepoint at each step (1/expected_segment_length)
    /// - `prior_alpha`, `prior_beta`: Beta prior for new segments (weak: 1.0, 1.0)
    pub fn new(hazard: f64, prior_alpha: f64, prior_beta: f64) -> Self {
        let mut run_probs = vec![0.0; MAX_RUN_LENGTH + 1];
        run_probs[0] = 1.0; // start with run length 0

        let mut stats = Vec::with_capacity(MAX_RUN_LENGTH + 1);
        for _ in 0..=MAX_RUN_LENGTH {
            stats.push(BetaBinomialStats::new(prior_alpha, prior_beta));
        }

        Self {
            run_probs,
            stats,
            hazard,
            prior_alpha,
            prior_beta,
            updates: 0,
        }
    }

    /// Create with default parameters suitable for FEC rate control.
    pub fn default_fec() -> Self {
        // hazard = 1/100: expect regime changes every ~100 batches (200s at 2s intervals)
        // Weak prior: Beta(1,1) = uniform
        Self::new(0.01, 1.0, 1.0)
    }

    /// Update with a new batch observation.
    ///
    /// `received`: number of symbols received in this batch
    /// `lost`: number of symbols lost in this batch
    pub fn update(&mut self, received: u32, lost: u32) {
        self.updates += 1;
        let total = received + lost;
        if total == 0 {
            return;
        }

        // Step 1: Compute predictive probabilities for each run length
        let mut predictive = vec![0.0f64; MAX_RUN_LENGTH + 1];
        for r in 0..=MAX_RUN_LENGTH {
            if self.run_probs[r] < 1e-300 {
                continue;
            }
            // Predictive probability of this observation under run length r's model
            // Using Beta-Binomial predictive: for batch of `total` with `lost` losses,
            // approximate as product of independent Bernoulli trials
            let p_loss = self.stats[r].predictive_loss_prob();
            // Log-likelihood for binomial observation (avoid underflow)
            let ll = if lost == 0 {
                total as f64 * (1.0 - p_loss).max(1e-300).ln()
            } else if received == 0 {
                total as f64 * p_loss.max(1e-300).ln()
            } else {
                lost as f64 * p_loss.max(1e-300).ln()
                    + received as f64 * (1.0 - p_loss).max(1e-300).ln()
            };
            predictive[r] = ll.exp();
        }

        // Step 2: Compute growth probabilities (run length increases by 1)
        let mut new_probs = vec![0.0f64; MAX_RUN_LENGTH + 1];
        let mut changepoint_mass = 0.0;

        for r in 0..MAX_RUN_LENGTH {
            let joint = self.run_probs[r] * predictive[r];
            // Growth: this run continues
            new_probs[r + 1] += joint * (1.0 - self.hazard);
            // Changepoint: mass flows to r=0
            changepoint_mass += joint * self.hazard;
        }
        // Run length at MAX: stays at MAX (absorbing state)
        let joint_max = self.run_probs[MAX_RUN_LENGTH] * predictive[MAX_RUN_LENGTH];
        new_probs[MAX_RUN_LENGTH] += joint_max * (1.0 - self.hazard);
        changepoint_mass += joint_max * self.hazard;

        // Step 3: Changepoint creates fresh run length 0
        new_probs[0] = changepoint_mass;

        // Step 4: Normalize
        let total_mass: f64 = new_probs.iter().sum();
        if total_mass > 1e-300 {
            for p in &mut new_probs {
                *p /= total_mass;
            }
        } else {
            // Numerical underflow — reset to uniform
            new_probs[0] = 1.0;
        }

        // Step 5: Update sufficient statistics
        // Shift stats: stats[r+1] inherits from stats[r] (then update)
        // Work backwards to avoid overwriting
        for r in (1..=MAX_RUN_LENGTH).rev() {
            if r > 0 {
                self.stats[r] = self.stats[r - 1].clone();
            }
            self.stats[r].update(received, lost);
        }
        // Fresh stats for run length 0 (new segment)
        self.stats[0] = BetaBinomialStats::new(
            self.prior_alpha + received as f64,
            self.prior_beta + lost as f64,
        );

        self.run_probs = new_probs;
    }

    /// Compute the predictive quantile of the loss rate.
    ///
    /// Integrates over the run-length distribution to produce a single
    /// loss rate estimate at the given confidence level.
    ///
    /// `confidence`: e.g., 0.95 for 95th percentile upper bound
    pub fn predictive_quantile(&self, confidence: f64) -> f64 {
        if self.updates == 0 {
            return 0.5; // no data yet — uninformative
        }

        // Mixture of Beta posteriors weighted by run-length probabilities
        // For each run length, compute the quantile, then take weighted average
        //
        // More precisely: we want the quantile of the mixture distribution.
        // The weighted-average-of-quantiles is an approximation, but for
        // the FEC use case it's sufficiently accurate (and O(MAX_RUN_LENGTH)).
        let mut weighted_quantile = 0.0;
        let mut total_weight = 0.0;

        for r in 0..=MAX_RUN_LENGTH {
            let w = self.run_probs[r];
            if w < 1e-300 {
                continue;
            }
            let q = self.stats[r].loss_quantile(confidence);
            weighted_quantile += w * q;
            total_weight += w;
        }

        if total_weight > 1e-300 {
            weighted_quantile / total_weight
        } else {
            0.5
        }
    }

    /// Point estimate: expected loss rate (mixture mean).
    // Test-only consumer: the BOCD law tests in this file's `mod tests` assert
    // the mixture mean tracks a regime change. Not on the data path.
    #[allow(dead_code)]
    pub fn predictive_mean(&self) -> f64 {
        let mut weighted_mean = 0.0;
        let mut total_weight = 0.0;

        for r in 0..=MAX_RUN_LENGTH {
            let w = self.run_probs[r];
            if w < 1e-300 {
                continue;
            }
            let mean = self.stats[r].predictive_loss_prob();
            weighted_mean += w * mean;
            total_weight += w;
        }

        if total_weight > 1e-300 {
            weighted_mean / total_weight
        } else {
            0.0
        }
    }

    /// Number of updates processed.
    pub fn updates(&self) -> u64 {
        self.updates
    }
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
    fn test_bocd_steady_state() {
        let mut bocd = BayesianChangepoint::default_fec();

        // Feed steady 10% loss for 100 batches
        for _ in 0..100 {
            bocd.update(90, 10);
        }

        let mean = bocd.predictive_mean();
        assert!(
            (mean - 0.1).abs() < 0.02,
            "Expected ~10% loss mean, got {mean}"
        );

        let upper = bocd.predictive_quantile(0.95);
        assert!(
            upper > mean,
            "95th percentile ({upper}) should exceed mean ({mean})"
        );
        assert!(
            upper < 0.2,
            "95th percentile should be reasonable: {upper}"
        );
    }

    #[test]
    fn test_bocd_changepoint_detection() {
        let mut bocd = BayesianChangepoint::default_fec();

        // Phase 1: steady 1% loss
        for _ in 0..50 {
            bocd.update(99, 1);
        }
        let mean_before = bocd.predictive_mean();

        // Phase 2: jump to 10% loss
        for _ in 0..15 {
            bocd.update(90, 10);
        }
        let mean_after = bocd.predictive_mean();

        assert!(
            mean_after > mean_before * 2.0,
            "Mean should increase significantly after changepoint: {mean_before} → {mean_after}"
        );
    }

    #[test]
    fn test_bocd_zero_loss() {
        let mut bocd = BayesianChangepoint::default_fec();

        for _ in 0..100 {
            bocd.update(100, 0);
        }

        let mean = bocd.predictive_mean();
        assert!(mean < 0.02, "Zero-loss mean should be near zero: {mean}");

        let upper = bocd.predictive_quantile(0.95);
        assert!(
            upper < 0.05,
            "Zero-loss upper bound should be small: {upper}"
        );
    }

    #[test]
    fn test_bocd_high_loss() {
        let mut bocd = BayesianChangepoint::default_fec();

        for _ in 0..50 {
            bocd.update(70, 30);
        }

        let mean = bocd.predictive_mean();
        assert!(
            (mean - 0.3).abs() < 0.05,
            "Expected ~30% loss mean, got {mean}"
        );
    }

    #[test]
    fn test_bocd_adaptation_speed() {
        let mut bocd = BayesianChangepoint::default_fec();

        // Steady state
        for _ in 0..50 {
            bocd.update(99, 1);
        }

        // Changepoint: jump to 10%
        for i in 0..20 {
            bocd.update(90, 10);
            if i >= 10 {
                let mean = bocd.predictive_mean();
                // After 10 samples of 10% loss, BOCD should have adapted
                assert!(
                    mean > 0.03,
                    "BOCD should adapt within 10 samples: mean={mean} at step {i}"
                );
            }
        }
    }

    #[test]
    fn test_normal_quantile() {
        let z50 = normal_quantile(0.5);
        assert!(z50.abs() < 1e-10, "z(0.5) should be 0, got {z50}");

        let z95 = normal_quantile(0.95);
        assert!(
            (z95 - 1.645).abs() < 0.01,
            "z(0.95) should be ~1.645, got {z95}"
        );
    }
}
