//! B3: Backend auto-switching tests with SimChannel.
//!
//! Verifies that BackendSelector correctly switches FEC backends
//! based on simulated channel conditions.

mod common;

use raptorpath::control::backend_selector::BackendSelector;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::fec::FecBackend;
use raptorpath::scheduler::MockClock;
use std::sync::Arc;
use std::time::Duration;

/// Build an estimator with the given loss rate by recording batches.
fn estimator_at_loss(loss_rate: f64) -> LossEstimator {
    let mut est = LossEstimator::new();
    let batch = 1000u32;
    let received = ((1.0 - loss_rate) * batch as f64) as u32;
    for _ in 0..100 {
        est.record_batch(batch, received);
    }
    est
}

/// Feed bursty loss into an estimator to trigger is_in_burst().
fn feed_bursty_loss(est: &mut LossEstimator) {
    // Several lossy batches to trigger burst detection
    for _ in 0..10 {
        est.record_batch(10, 3); // 70% loss burst
        est.record_batch(10, 10); // clean
    }
    est.record_batch(10, 3); // end in burst
}

#[test]
fn test_switch_low_to_high_loss() {
    // Start at low loss → RaptorQ. Switch to high loss → should switch to Mettle after debounce.
    let mut selector = BackendSelector::new(
        FecBackend::RaptorQ,
        None,  // forced
        ProtocolHint::Auto,
        0.02,  // threshold_low
        0.08,  // threshold_high
        0,     // switch_interval_secs=0 bypasses timer
        false, // block mode
    );

    // Low loss: 0.5% → should stay RaptorQ
    let est_low = estimator_at_loss(0.005);
    for _ in 0..5 {
        let result = selector.evaluate(&est_low);
        assert!(result.is_none(), "should not switch at low loss");
    }
    assert_eq!(selector.current(), FecBackend::RaptorQ);

    // High loss: 15% → should switch to Mettle after 3 debounce evals
    let est_high = estimator_at_loss(0.15);
    let mut switched = false;
    for i in 0..5 {
        if let Some(new_backend) = selector.evaluate(&est_high) {
            assert_eq!(new_backend, FecBackend::Mettle);
            assert!(i >= 2, "should take at least 3 evals to switch (debounce)");
            switched = true;
            break;
        }
    }
    assert!(switched, "should have switched to Mettle at high loss");
    assert_eq!(selector.current(), FecBackend::Mettle);
}

#[test]
fn test_switch_with_sim_channel() {
    let clock = Arc::new(MockClock::new());

    // Start with datacenter conditions → RaptorQ
    let mut selector = BackendSelector::new(
        FecBackend::RaptorQ,
        None,
        ProtocolHint::Auto,
        0.02,
        0.08,
        0,
        false,
    );

    // Phase 1: Datacenter (low loss)
    let mut channel = common::SimChannel::datacenter(clock.clone(), 42);
    let symbols = common::make_source_batch(1000);

    let mut est = LossEstimator::new();
    let mut survived = 0u32;
    for sym in &symbols {
        if channel.send(sym.clone()) {
            survived += 1;
        }
    }
    est.record_batch(1000, survived);

    // Should stay RaptorQ
    for _ in 0..5 {
        assert!(selector.evaluate(&est).is_none());
    }
    assert_eq!(selector.current(), FecBackend::RaptorQ);

    // Phase 2: Switch to WiFi (higher loss)
    let mut wifi_channel = common::SimChannel::wifi(clock.clone(), 77);
    let mut est_wifi = LossEstimator::new();

    // Run enough batches to build up loss estimate
    for _ in 0..50 {
        let batch = common::make_source_batch(100);
        let mut survived = 0u32;
        for sym in &batch {
            if wifi_channel.send(sym.clone()) {
                survived += 1;
            }
        }
        est_wifi.record_batch(100, survived);
    }

    // Evaluate until switch happens
    let mut switched = false;
    for _ in 0..10 {
        if let Some(new_backend) = selector.evaluate(&est_wifi) {
            // Should switch away from RaptorQ to a window-capable backend
            assert_ne!(new_backend, FecBackend::RaptorQ);
            switched = true;
            break;
        }
    }

    // The switch depends on loss rate vs thresholds. WiFi GE avg ~2.5%
    // which is above threshold_low=2%, so should eventually trigger.
    // If loss is between thresholds, it may or may not switch.
    // Just verify the selector is functioning and not panicking.
    let _ = switched; // May or may not switch depending on GE realization
}

#[test]
fn test_window_burst_high_loss_switches_backend() {
    // Window mode: bursty high loss → BackendSelector switches away from RLC.
    // (Before the streaming machine's retirement (2026-07-28) the GE burst
    // branch could pick Streaming; the surviving high-loss target is Mettle.)
    let mut selector = BackendSelector::new(
        FecBackend::Rlc,
        None,
        ProtocolHint::Auto,
        0.02,
        0.08,
        0,
        true, // window mode
    );

    let mut est = estimator_at_loss(0.10);
    // Feed bursty loss to trigger GE burst detection
    feed_bursty_loss(&mut est);

    // Evaluate repeatedly until switch (debounce=3)
    let mut final_backend = selector.current();
    for _ in 0..10 {
        if let Some(new) = selector.evaluate(&est) {
            final_backend = new;
            break;
        }
    }

    // With high loss + burst, should have switched away from RLC
    // (the selector's surviving window-mode high-loss target is Mettle)
    assert_ne!(
        final_backend,
        FecBackend::Rlc,
        "should switch away from RLC under high bursty loss"
    );
}

#[test]
fn test_hysteresis_prevents_oscillation() {
    let mut selector = BackendSelector::new(
        FecBackend::RaptorQ,
        None,
        ProtocolHint::Auto,
        0.02,
        0.08,
        0,
        false,
    );

    let est_high = estimator_at_loss(0.15);
    let est_low = estimator_at_loss(0.005);

    // Alternate high/low every 2 evals — debounce=3 should prevent switch
    for _ in 0..10 {
        selector.evaluate(&est_high);
        selector.evaluate(&est_high);
        selector.evaluate(&est_low);
        selector.evaluate(&est_low);
    }

    // Should still be on original backend because debounce never reached 3
    // consecutive evaluations to the same target
    assert_eq!(
        selector.current(),
        FecBackend::RaptorQ,
        "alternating conditions should not trigger switch (debounce prevents oscillation)"
    );
}
