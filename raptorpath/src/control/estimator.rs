//! Loss rate estimation using Bayesian EWMA + BOCD.
//!
//! Combines:
//! - Beta-Binomial conjugate prior for principled uncertainty quantification
//! - EWMA for fast adaptation to changing conditions
//! - Burst detection for non-iid loss patterns
//! - BOCD (Bayesian Online Changepoint Detection) for regime-aware prediction
//! - Separate TX/RX loss tracking for asymmetric path estimation

use super::changepoint::BayesianChangepoint;
use super::gilbert_elliott::GilbertElliottEstimator;
use std::time::{Duration, Instant};

/// Per-path loss estimator.
#[derive(Debug)]
pub struct LossEstimator {
    // --- TX path loss (forward direction) ---
    /// EWMA of TX loss rate
    tx_ewma_loss: f64,
    /// EWMA smoothing factor (higher = more responsive)
    alpha: f64,
    /// Beta distribution parameters (Bayesian prior) for TX path
    beta_a: f64, // successes (received)
    beta_b: f64, // failures (lost)
    /// Decay factor for Beta params to forget old data
    beta_decay: f64,

    // --- RX path loss (reverse direction, from NackAck feedback) ---
    /// Beta distribution parameters for RX path loss
    rx_beta_a: f64,
    rx_beta_b: f64,
    /// EWMA of RX loss rate
    rx_ewma_loss: f64,

    /// RTT estimation (EWMA)
    ewma_rtt: Duration,
    rtt_alpha: f64,
    /// feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`): seed the RTT EWMA from the
    /// FIRST measured sample instead of blending real samples into the 50-ms
    /// DEFAULT_SRTT-class constructor seed (hygiene rule 1: an anchor is
    /// seeded from measurements; the 50-ms seed surviving warm-up was the
    /// measured M* floor-freshness FAIL — goal-gate #61 knee battery).
    rtt_seed_from_sample: bool,
    /// True once a real RTT sample has been recorded (seed consumed).
    rtt_seeded: bool,

    /// Throughput estimation (bytes/sec EWMA)
    ewma_throughput: f64,

    /// Burst loss detection
    consecutive_losses: u32,
    burst_threshold: u32,
    in_burst: bool,

    /// Interarrival jitter (RTCP-style, RFC 3550 A.8)
    jitter: f64,
    /// Last packet arrival timestamp for jitter calculation
    last_arrival_us: Option<u64>,
    /// Last packet send timestamp for jitter calculation
    last_send_ts_us: Option<u64>,

    /// Gilbert-Elliott HMM for bursty loss estimation
    ge: GilbertElliottEstimator,

    /// BOCD for regime-aware prediction
    bocd: BayesianChangepoint,

    /// `RWM_EST_CADENCE` (goal-gate "Receiver Per-Message Wall"): run the
    /// BOCD heavy update at its DESIGN cadence instead of per message. The
    /// detector's own constructor documents "regime changes every ~100
    /// batches (200 s at 2 s intervals)" — a batch-cadence model; the window
    /// wire was calling its O(MAX_RUN_LENGTH) exp/ln update ~22k×/s per side
    /// (~22–26%/core at the c1 wall, STEP-1 profile). With the gate ON,
    /// clean observations ACCUMULATE and flush every `EST_HEAVY_CADENCE`;
    /// any call that carries a loss flushes IMMEDIATELY (every informative
    /// observation reaches the posterior on today's clock — zero staleness
    /// on losses). EWMA/Beta/burst/GE stay per-call. OFF = per-call BOCD,
    /// bit-identical shipped path.
    est_cadence: bool,
    /// Accumulated (received, lost) counts awaiting the next BOCD flush.
    bocd_acc_received: u64,
    bocd_acc_lost: u64,
    /// Instant of the last BOCD flush (cadence clock).
    bocd_last_flush: Instant,

    /// Bookkeeping
    total_sent: u64,
    total_received: u64,
    last_update: Instant,
}

/// `RWM_EST_CADENCE` heartbeat: clean evidence flushes to the BOCD at this
/// cadence (10 ms ≪ the 100 ms recovery round; ~100 updates/s ≈ the
/// detector's design regime). Pre-registered constant — not a tuning knob.
const EST_HEAVY_CADENCE: Duration = Duration::from_millis(10);

/// Resolve `RWM_EST_CADENCE` once (noted in the gates.rs header list of
/// resolve-once sites) and echo mechanism liveness on first resolution
/// (MEASUREMENT DISCIPLINE item 1).
///
/// DEFAULT ON since 2026-08-07 (goal-gate "Ship The Wins 1": the §16.35 c7
/// anchor-interaction blocker resolved by the `RWM_POOL_ANCHOR` composition
/// — the pooled-store cap at N ≥ 2 reads the burst-immune send-interval
/// anchor, so the faster ack clock can no longer inflate the dual-store cap;
/// pre-registered composed battery earned the flip). `RWM_EST_CADENCE=0` is
/// the prior-default opt-out arm (per-call BOCD, and `RWM_POOL_ANCHOR`
/// defaults off with it — one composed default).
pub(crate) fn est_cadence_active() -> bool {
    use std::sync::OnceLock;
    static GATE: OnceLock<bool> = OnceLock::new();
    *GATE.get_or_init(|| {
        let on = crate::config::env_flag("RWM_EST_CADENCE", true);
        if on {
            tracing::info!(
                "estimator heavy-math cadence ACTIVE (RWM_EST_CADENCE: BOCD update at 10 ms/loss-event cadence, accumulated counts)"
            );
        }
        on
    })
}

impl LossEstimator {
    pub fn new() -> Self {
        Self {
            tx_ewma_loss: 0.0,
            alpha: 0.1, // ~10-sample half-life
            // Weak prior: Beta(1,1) = uniform
            beta_a: 1.0,
            beta_b: 1.0,
            beta_decay: 0.995, // slowly forget old observations
            // RX path: weak prior
            rx_beta_a: 1.0,
            rx_beta_b: 1.0,
            rx_ewma_loss: 0.0,
            ewma_rtt: Duration::from_millis(50),
            rtt_alpha: 0.125, // standard TCP EWMA
            // DEFAULT ON (2026-07-21, "Consolidation" battery).
            rtt_seed_from_sample: crate::config::anchor_gate_default("RWM_MSTAR_ANCHOR", true),
            rtt_seeded: false,
            ewma_throughput: 0.0,
            consecutive_losses: 0,
            burst_threshold: 3,
            in_burst: false,
            jitter: 0.0,
            last_arrival_us: None,
            last_send_ts_us: None,
            ge: GilbertElliottEstimator::new(),
            bocd: BayesianChangepoint::default_fec(),
            est_cadence: est_cadence_active(),
            bocd_acc_received: 0,
            bocd_acc_lost: 0,
            bocd_last_flush: Instant::now(),
            total_sent: 0,
            total_received: 0,
            last_update: Instant::now(),
        }
    }

    /// Record that `received` out of `sent` symbols arrived in a batch.
    ///
    /// The Gilbert-Elliott estimator is fed a LUMPED approximation (`lost`
    /// Bad symbols followed by `received` Good ones), which overestimates
    /// burstiness — the conservative direction. Callers that know the true
    /// per-symbol pattern (SACK gaps) should use `record_counts` +
    /// `record_symbol` instead for an unbiased burst estimate.
    pub fn record_batch(&mut self, sent: u32, received: u32) {
        let lost = sent.saturating_sub(received);
        self.record_counts(sent, received);

        // Feed Gilbert-Elliott HMM: approximate as `lost` Bad symbols
        // followed by `received` Good symbols within this batch
        for _ in 0..lost {
            self.ge.record_symbol(false);
        }
        for _ in 0..received {
            self.ge.record_symbol(true);
        }
    }

    /// Count-only update (EWMA + Beta + BOCD + burst flag) WITHOUT the
    /// lumped Gilbert-Elliott approximation. Pair with per-symbol
    /// `record_symbol` calls carrying the actual arrival pattern.
    pub fn record_counts(&mut self, sent: u32, received: u32) {
        let lost = sent.saturating_sub(received);
        let batch_loss = if sent > 0 {
            lost as f64 / sent as f64
        } else {
            0.0
        };

        // EWMA update
        self.tx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.tx_ewma_loss;

        // Beta-Binomial update with decay
        self.beta_a *= self.beta_decay;
        self.beta_b *= self.beta_decay;
        self.beta_a += received as f64;
        self.beta_b += lost as f64;

        // BOCD update — per-call when the cadence gate is OFF (legacy
        // bit-identical); accumulated + flushed on loss / 10 ms heartbeat
        // when ON (goal-gate "Receiver Per-Message Wall": the per-message
        // O(MAX_RUN_LENGTH) update was 22–26%/core at the c1 wall).
        if self.est_cadence {
            self.bocd_acc_received += received as u64;
            self.bocd_acc_lost += lost as u64;
            if lost > 0 || self.bocd_last_flush.elapsed() >= EST_HEAVY_CADENCE {
                self.flush_bocd();
            }
        } else {
            self.bocd.update(received, lost);
        }

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

    /// Flush the accumulated counts into the BOCD (cadence gate). The
    /// posterior sees the same evidence as the per-call path, batched —
    /// exactly the batch-cadence observation model `default_fec()` was
    /// designed for.
    fn flush_bocd(&mut self) {
        if self.bocd_acc_received > 0 || self.bocd_acc_lost > 0 {
            self.bocd.update(
                self.bocd_acc_received.min(u32::MAX as u64) as u32,
                self.bocd_acc_lost.min(u32::MAX as u64) as u32,
            );
            self.bocd_acc_received = 0;
            self.bocd_acc_lost = 0;
        }
        self.bocd_last_flush = Instant::now();
    }

    /// Record one wire-symbol outcome (true = received) into the
    /// Gilbert-Elliott estimator, preserving the true loss interleaving —
    /// e.g., reconstructed from SACK gap patterns (paper Section 7.5).
    pub fn record_symbol(&mut self, received: bool) {
        self.ge.record_symbol(received);
    }

    /// Update RX (reverse path) loss estimate from NackAck feedback.
    ///
    /// `nacks_sent`: number of NACKs the receiver sent in this period
    /// `acks_received`: number of NackAcks received back from the sender
    pub fn update_rx_loss(&mut self, nacks_sent: u32, acks_received: u32) {
        if nacks_sent == 0 {
            return;
        }
        let lost = nacks_sent.saturating_sub(acks_received);
        let batch_loss = lost as f64 / nacks_sent as f64;

        // EWMA update
        self.rx_ewma_loss = self.alpha * batch_loss + (1.0 - self.alpha) * self.rx_ewma_loss;

        // Beta-Binomial update with decay
        self.rx_beta_a *= self.beta_decay;
        self.rx_beta_b *= self.beta_decay;
        self.rx_beta_a += acks_received as f64;
        self.rx_beta_b += lost as f64;
    }

    /// RX path loss rate (point estimate).
    pub fn rx_loss_rate(&self) -> f64 {
        self.rx_ewma_loss
    }

    /// NACK effectiveness: probability that a NACK round-trip succeeds.
    /// = (1 - ε_rx)² where ε_rx is the RX path loss rate.
    /// The NACK must survive the reverse path AND the repair must survive the forward path.
    pub fn nack_effectiveness(&self) -> f64 {
        let rx_loss = self.rx_ewma_loss;
        (1.0 - rx_loss).powi(2)
    }

    /// Record an RTT measurement.
    pub fn record_rtt(&mut self, rtt: Duration) {
        // feat/anchor-hygiene rule 1: the first MEASURED sample replaces the
        // constructor seed outright (no blend with the 50-ms constant).
        if self.rtt_seed_from_sample && !self.rtt_seeded {
            self.rtt_seeded = true;
            self.ewma_rtt = rtt;
            return;
        }
        let rtt_secs = rtt.as_secs_f64();
        let old_secs = self.ewma_rtt.as_secs_f64();
        let new_secs = self.rtt_alpha * rtt_secs + (1.0 - self.rtt_alpha) * old_secs;
        self.ewma_rtt = Duration::from_secs_f64(new_secs);
    }

    /// Test hook (feat/anchor-hygiene): force the seed-from-sample gate
    /// without the process-global env (parallel unit tests must not race
    /// env vars).
    #[cfg(test)]
    pub(crate) fn force_anchor_hygiene(&mut self, seed_from_sample: bool) {
        self.rtt_seed_from_sample = seed_from_sample;
    }

    /// Record throughput measurement.
    pub fn record_throughput(&mut self, bytes_per_sec: f64) {
        self.ewma_throughput =
            self.rtt_alpha * bytes_per_sec + (1.0 - self.rtt_alpha) * self.ewma_throughput;
    }

    /// Current TX loss rate estimate (point estimate, EWMA).
    pub fn loss_rate(&self) -> f64 {
        self.tx_ewma_loss
    }

    /// Upper bound of TX loss rate at given confidence level.
    /// Uses the Beta posterior: quantile at (1 - confidence).
    /// This is what we use for computing FEC rate — we want to be conservative.
    pub fn loss_rate_upper(&self, confidence: f64) -> f64 {
        beta_quantile(self.beta_b, self.beta_a, confidence)
    }

    /// Predictive upper bound from BOCD posterior.
    ///
    /// This integrates over run-length uncertainty, producing a tighter
    /// bound than the Beta posterior when in steady state, and a wider
    /// bound during regime changes. This IS the margin — no additional
    /// safety factor needed.
    pub fn predictive_loss_upper(&self, confidence: f64) -> f64 {
        if self.bocd.updates() < 5 {
            // Not enough data for BOCD — fall back to Beta upper bound
            return self.loss_rate_upper(confidence);
        }
        self.bocd.predictive_quantile(confidence)
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

    /// Record arrival for jitter calculation (RFC 3550 A.8).
    /// `send_ts_us`: sender's timestamp in microseconds.
    /// `arrival_us`: local arrival time in microseconds.
    pub fn record_arrival(&mut self, send_ts_us: u64, arrival_us: u64) {
        if let (Some(last_send), Some(last_arrival)) = (self.last_send_ts_us, self.last_arrival_us) {
            // D(i,j) = (Rj - Ri) - (Sj - Si)
            let transit_diff = (arrival_us as i64 - last_arrival as i64)
                - (send_ts_us as i64 - last_send as i64);
            let d = transit_diff.unsigned_abs() as f64;
            // J(i) = J(i-1) + (|D(i,j)| - J(i-1)) / 16
            self.jitter += (d - self.jitter) / 16.0;
        }
        self.last_send_ts_us = Some(send_ts_us);
        self.last_arrival_us = Some(arrival_us);
    }

    /// Current jitter estimate in microseconds (RFC 3550 style).
    pub fn jitter_us(&self) -> f64 {
        self.jitter
    }

    /// Point estimate of loss rate from Beta posterior mean.
    pub fn loss_rate_mean(&self) -> f64 {
        self.beta_b / (self.beta_a + self.beta_b)
    }

    /// Relative uncertainty: (upper_bound - mean) / mean.
    /// Returns 0.0 when loss is negligible.
    pub fn loss_uncertainty(&self, confidence: f64) -> f64 {
        let mean = self.loss_rate_mean();
        if mean < 1e-10 {
            return 0.0;
        }
        let upper = self.loss_rate_upper(confidence);
        ((upper - mean) / mean).max(0.0)
    }

    /// Total number of symbols sent (for confidence adaptation).
    pub fn total_sent(&self) -> u64 {
        self.total_sent
    }

    pub fn ge_estimator(&self) -> &GilbertElliottEstimator {
        &self.ge
    }

    pub fn bocd(&self) -> &BayesianChangepoint {
        &self.bocd
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

impl LossEstimator {
    /// Test-only constructor with the heavy-math cadence forced ON
    /// (the env gate is process-global; law tests need both arms).
    #[cfg(test)]
    pub fn new_with_cadence_for_test() -> Self {
        let mut e = Self::new();
        e.est_cadence = true;
        e
    }

    /// Test-only constructor with the heavy-math cadence forced OFF — the
    /// `RWM_EST_CADENCE=0` prior-default opt-out arm (per-call BOCD),
    /// env-independent for the law tests.
    #[cfg(test)]
    pub fn new_per_call_for_test() -> Self {
        let mut e = Self::new();
        e.est_cadence = false;
        e
    }

    /// Test/diag: BOCD updates processed (the cadence mechanism gauge).
    pub fn bocd_updates(&self) -> u64 {
        self.bocd.updates()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RWM_EST_CADENCE default: ships ON since 2026-08-07 (goal-gate "Ship
    /// The Wins 1" — the composed default with RWM_POOL_ANCHOR). Relies on
    /// the test env not exporting RWM_* overrides, like every engine-default
    /// test in this crate.
    #[test]
    fn test_est_cadence_default_on() {
        let est = LossEstimator::new();
        assert!(
            est.est_cadence,
            "RWM_EST_CADENCE ships default ON (the est×honest-anchor composed default)"
        );
    }

    /// RWM_EST_CADENCE law: with the gate OFF (`=0`, the prior-default
    /// opt-out arm), every record_batch performs a per-call BOCD update —
    /// the legacy path is bit-identical in call topology.
    #[test]
    fn test_est_cadence_off_is_per_call() {
        let mut est = LossEstimator::new_per_call_for_test();
        assert!(!est.est_cadence);
        for _ in 0..7 {
            est.record_batch(1, 1);
        }
        assert_eq!(est.bocd_updates(), 7);
    }

    /// RWM_EST_CADENCE law: clean observations within the 10 ms heartbeat
    /// ACCUMULATE (no heavy update); a loss-bearing call flushes
    /// IMMEDIATELY — every informative observation reaches the posterior
    /// on the legacy clock.
    #[test]
    fn test_est_cadence_accumulates_clean_flushes_loss() {
        let mut est = LossEstimator::new_with_cadence_for_test();
        for _ in 0..50 {
            est.record_batch(1, 1); // clean
        }
        assert_eq!(
            est.bocd_updates(),
            0,
            "clean sub-cadence evidence must accumulate"
        );
        est.record_batch(2, 1); // one loss → immediate flush with the backlog
        assert_eq!(est.bocd_updates(), 1, "a loss flushes immediately");
        // The flush carried the whole backlog: 52 sent / 51 received.
        std::thread::sleep(std::time::Duration::from_millis(12));
        est.record_batch(1, 1); // heartbeat elapsed → flush
        assert_eq!(est.bocd_updates(), 2, "the 10 ms heartbeat flushes");
    }

    /// RWM_EST_CADENCE law: the cadenced posterior lands in the same class
    /// as the per-call posterior for a steady lossy stream — the consumers
    /// (predictive_loss_upper → r*) read equal-class values.
    #[test]
    fn test_est_cadence_posterior_equal_class() {
        let mut per_call = LossEstimator::new_per_call_for_test();
        let mut cadenced = LossEstimator::new_with_cadence_for_test();
        // ~5% loss in per-message batches (1 symbol per batch, 1 loss / 20).
        for i in 0..400 {
            let received = if i % 20 == 0 { 0 } else { 1 };
            per_call.record_batch(1, received);
            cadenced.record_batch(1, received);
        }
        let a = per_call.predictive_loss_upper(0.95);
        let b = cadenced.predictive_loss_upper(0.95);
        assert!(
            (a - b).abs() < 0.05,
            "cadenced posterior must stay in the per-call class: {a} vs {b}"
        );
        // And the cheap per-call estimates are bit-identical by construction.
        assert!((per_call.loss_rate() - cadenced.loss_rate()).abs() < 1e-12);
    }

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

    #[test]
    fn test_predictive_loss_upper() {
        let mut est = LossEstimator::new();
        for _ in 0..100 {
            est.record_batch(100, 90);
        }

        let pred_upper = est.predictive_loss_upper(0.95);
        assert!(pred_upper > 0.08, "Predictive upper should be above ~10%: {pred_upper}");
        assert!(pred_upper < 0.25, "Predictive upper should be reasonable: {pred_upper}");
    }

    // feat/anchor-hygiene (`RWM_MSTAR_ANCHOR`), hygiene rule 1: the RTT EWMA
    // seeds from the FIRST measured sample; the 50-ms constructor constant
    // never blends into measurements (the DEFAULT_SRTT-class seed surviving
    // warm-up was the #61 M* floor-freshness FAIL). Control arm: the legacy
    // law blends 0.875·50 ms + 0.125·sample — the constant leaks for ~20
    // samples.
    #[test]
    fn rtt_seeds_from_first_measured_sample_under_hygiene() {
        let mut est = LossEstimator::new();
        est.force_anchor_hygiene(true);
        est.record_rtt(Duration::from_millis(200));
        assert_eq!(
            est.rtt(),
            Duration::from_millis(200),
            "first measured sample IS the estimate — no 50-ms blend"
        );
        // Subsequent samples EWMA-blend off the measured seed as before.
        est.record_rtt(Duration::from_millis(100));
        let expected = 0.875 * 0.200 + 0.125 * 0.100;
        assert!((est.rtt().as_secs_f64() - expected).abs() < 1e-9);

        // Control: the legacy path blends the constructor seed.
        let mut legacy = LossEstimator::new();
        legacy.force_anchor_hygiene(false);
        legacy.record_rtt(Duration::from_millis(200));
        let blended = 0.875 * 0.050 + 0.125 * 0.200;
        assert!(
            (legacy.rtt().as_secs_f64() - blended).abs() < 1e-9,
            "legacy control keeps the 50-ms seed blend (the defect, preserved \
             for the A/B): got {:?}",
            legacy.rtt()
        );
    }

    #[test]
    fn test_rx_loss_tracking() {
        let mut est = LossEstimator::new();

        // Simulate 20% RX path loss
        for _ in 0..50 {
            est.update_rx_loss(10, 8);
        }

        let rx_loss = est.rx_loss_rate();
        assert!((rx_loss - 0.2).abs() < 0.05, "Expected ~20% RX loss, got {rx_loss}");

        let effectiveness = est.nack_effectiveness();
        // (1 - 0.2)^2 = 0.64
        assert!((effectiveness - 0.64).abs() < 0.1, "Expected ~0.64 effectiveness, got {effectiveness}");
    }

    #[test]
    fn test_nack_effectiveness_no_loss() {
        let est = LossEstimator::new();
        let eff = est.nack_effectiveness();
        assert!((eff - 1.0).abs() < 0.01, "No RX loss should give ~1.0 effectiveness: {eff}");
    }
}
