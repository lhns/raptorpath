//! Runtime FEC backend selection based on channel conditions.
//!
//! Evaluates loss estimates and selects the optimal FEC backend:
//! - Low loss → RaptorQ (near-MDS, lowest overhead)
//! - Moderate loss → RLC (rateless, good moderate-loss recovery)
//! - High loss → Mettle (fast XOR decode, robust at high loss)
//!
//! Hysteresis prevents oscillation: minimum interval between switches
//! and condition must persist for multiple consecutive evaluations.

use crate::control::estimator::LossEstimator;
use crate::control::fec_rate::ProtocolHint;
use crate::fec::FecBackend;
use std::time::{Duration, Instant};

/// Selects the optimal FEC backend based on current channel conditions.
pub struct BackendSelector {
    current: FecBackend,
    forced: Option<FecBackend>,
    hint: ProtocolHint,
    last_switch: Instant,
    min_switch_interval: Duration,
    debounce_count: u32,
    debounce_target: u32,
    pending_backend: Option<FecBackend>,
    /// Below this loss rate → RaptorQ (block) or RLC (window)
    threshold_low: f64,
    /// Above this loss rate → Mettle
    threshold_high: f64,
    /// Whether this is window mode (restricts to window-capable backends)
    window_mode: bool,
}

impl BackendSelector {
    pub fn new(
        initial: FecBackend,
        forced: Option<FecBackend>,
        hint: ProtocolHint,
        threshold_low: f64,
        threshold_high: f64,
        switch_interval_secs: u64,
        window_mode: bool,
    ) -> Self {
        Self {
            current: initial,
            forced,
            hint,
            last_switch: Instant::now(),
            min_switch_interval: Duration::from_secs(switch_interval_secs),
            debounce_count: 0,
            debounce_target: 3,
            pending_backend: None,
            threshold_low,
            threshold_high,
            window_mode,
        }
    }

    /// Evaluate current conditions and return `Some(new_backend)` if a switch
    /// is warranted, `None` if no change.
    pub fn evaluate(&mut self, estimator: &LossEstimator) -> Option<FecBackend> {
        // Forced mode: never auto-switch
        if self.forced.is_some() {
            return None;
        }

        // Hysteresis: minimum interval between switches
        if self.last_switch.elapsed() < self.min_switch_interval {
            return None;
        }

        let loss = estimator.loss_rate_upper(0.95);
        let desired = if self.window_mode {
            self.select_window_backend(loss, estimator)
        } else {
            self.select_block_backend(loss)
        };

        if desired == self.current {
            // Reset debounce if condition no longer holds
            self.debounce_count = 0;
            self.pending_backend = None;
            return None;
        }

        // Debounce: require N consecutive evaluations wanting the same switch
        if self.pending_backend == Some(desired) {
            self.debounce_count += 1;
        } else {
            self.pending_backend = Some(desired);
            self.debounce_count = 1;
        }

        if self.debounce_count >= self.debounce_target {
            self.current = desired;
            self.last_switch = Instant::now();
            self.debounce_count = 0;
            self.pending_backend = None;
            Some(desired)
        } else {
            None
        }
    }

    /// Select best block-mode backend based on loss rate.
    fn select_block_backend(&self, loss: f64) -> FecBackend {
        if loss < self.threshold_low {
            FecBackend::RaptorQ
        } else if loss < self.threshold_high {
            FecBackend::Rlc
        } else {
            FecBackend::Mettle
        }
    }

    /// Select best window-mode backend (only window-capable: RLC, Mettle, Streaming).
    fn select_window_backend(&self, loss: f64, estimator: &LossEstimator) -> FecBackend {
        let ge = estimator.ge_estimator();
        if ge.is_valid() && ge.mean_burst_length() > 3.0 {
            return FecBackend::Streaming;
        }
        if loss < self.threshold_low {
            FecBackend::Rlc
        } else {
            FecBackend::Mettle
        }
    }

    /// Get the current active backend.
    pub fn current(&self) -> FecBackend {
        self.current
    }

    /// Force immediate switch (for API/testing).
    pub fn force(&mut self, backend: FecBackend) {
        self.current = backend;
        self.last_switch = Instant::now();
        self.debounce_count = 0;
        self.pending_backend = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::estimator::LossEstimator;

    fn make_selector(window_mode: bool) -> BackendSelector {
        BackendSelector::new(
            FecBackend::RaptorQ,
            None,
            ProtocolHint::Auto,
            0.01,
            0.10,
            0, // no delay for tests
            window_mode,
        )
    }

    fn estimator_with_loss(loss_rate: f64, samples: usize) -> LossEstimator {
        let mut est = LossEstimator::new();
        let sent = 1000u32;
        let received = ((1.0 - loss_rate) * sent as f64) as u32;
        for _ in 0..samples {
            est.record_batch(sent, received);
        }
        est
    }

    #[test]
    fn test_low_loss_selects_raptorq() {
        let mut sel = make_selector(false);
        let est = estimator_with_loss(0.001, 100); // 0.1% loss
        // Need 3 consecutive evaluations (debounce)
        // Current is already RaptorQ, so evaluate should return None
        assert!(sel.evaluate(&est).is_none());
        assert_eq!(sel.current(), FecBackend::RaptorQ);
    }

    #[test]
    fn test_high_loss_selects_mettle() {
        let mut sel = make_selector(false);
        let est = estimator_with_loss(0.15, 100); // 15% loss
        // First eval: debounce = 1
        assert!(sel.evaluate(&est).is_none());
        // Second eval: debounce = 2
        assert!(sel.evaluate(&est).is_none());
        // Third eval: debounce = 3 → switch
        let result = sel.evaluate(&est);
        assert_eq!(result, Some(FecBackend::Mettle));
        assert_eq!(sel.current(), FecBackend::Mettle);
    }

    #[test]
    fn test_moderate_loss_selects_rlc() {
        let mut sel = make_selector(false);
        let est = estimator_with_loss(0.05, 100); // 5% loss
        assert!(sel.evaluate(&est).is_none());
        assert!(sel.evaluate(&est).is_none());
        let result = sel.evaluate(&est);
        assert_eq!(result, Some(FecBackend::Rlc));
    }

    #[test]
    fn test_hysteresis_prevents_oscillation() {
        let mut sel = make_selector(false);
        let high = estimator_with_loss(0.15, 100);
        let low = estimator_with_loss(0.001, 100);

        // Two high evals, then a low eval → should reset debounce
        sel.evaluate(&high);
        sel.evaluate(&high);
        sel.evaluate(&low); // resets because desired == current (RaptorQ)

        // Now high again — debounce restarted, need 3 more
        assert!(sel.evaluate(&high).is_none()); // 1
        assert!(sel.evaluate(&high).is_none()); // 2
        assert_eq!(sel.evaluate(&high), Some(FecBackend::Mettle)); // 3
    }

    #[test]
    fn test_forced_backend_no_switch() {
        let mut sel = BackendSelector::new(
            FecBackend::RaptorQ,
            Some(FecBackend::RaptorQ), // forced
            ProtocolHint::Auto,
            0.01,
            0.10,
            0,
            false,
        );
        let est = estimator_with_loss(0.20, 100);
        // Should never switch when forced
        for _ in 0..10 {
            assert!(sel.evaluate(&est).is_none());
        }
        assert_eq!(sel.current(), FecBackend::RaptorQ);
    }

    #[test]
    fn test_configurable_thresholds() {
        // Set thresholds so 5% loss goes to Mettle (high threshold = 0.03)
        let mut sel = BackendSelector::new(
            FecBackend::RaptorQ,
            None,
            ProtocolHint::Auto,
            0.01,
            0.03, // lower high threshold
            0,
            false,
        );
        let est = estimator_with_loss(0.05, 100);
        sel.evaluate(&est);
        sel.evaluate(&est);
        assert_eq!(sel.evaluate(&est), Some(FecBackend::Mettle));
    }

    #[test]
    fn test_window_mode_low_loss_selects_rlc() {
        let mut sel = make_selector(true);
        // Window mode starts at RaptorQ (default), but low loss → RLC (window-capable)
        let est = estimator_with_loss(0.001, 100);
        sel.evaluate(&est);
        sel.evaluate(&est);
        let result = sel.evaluate(&est);
        assert_eq!(result, Some(FecBackend::Rlc));
    }

    #[test]
    fn test_window_mode_high_loss_selects_mettle_or_streaming() {
        let mut sel = BackendSelector::new(
            FecBackend::Rlc,
            None,
            ProtocolHint::Auto,
            0.01,
            0.10,
            0,
            true,
        );
        let est = estimator_with_loss(0.15, 100);
        sel.evaluate(&est);
        sel.evaluate(&est);
        let result = sel.evaluate(&est);
        // GE estimator may detect burst patterns → Streaming, otherwise → Mettle
        assert!(
            result == Some(FecBackend::Mettle) || result == Some(FecBackend::Streaming),
            "expected Mettle or Streaming, got {:?}",
            result
        );
    }
}
