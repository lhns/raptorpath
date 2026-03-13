//! Tests for the control loop: loss estimation → FEC rate → PI feedback.

use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};

#[test]
fn test_estimator_tracks_loss_correctly() {
    let mut est = LossEstimator::new();

    // Feed correct sent/received data (ADR-0003 fix)
    for _ in 0..50 {
        est.record_batch(100, 85); // 15% loss
    }

    let loss = est.loss_rate();
    assert!(
        (loss - 0.15).abs() < 0.03,
        "Expected ~15% loss, got {loss}"
    );
}

#[test]
fn test_estimator_upper_bound_conservative() {
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 90); // 10% loss
    }

    let point = est.loss_rate();
    let upper = est.loss_rate_upper(0.95);
    let upper99 = est.loss_rate_upper(0.99);

    assert!(upper > point, "95th percentile should exceed mean");
    assert!(upper99 > upper, "99th percentile should exceed 95th");
    assert!(upper < 0.5, "Upper bound should be reasonable");
}

#[test]
fn test_estimator_adapts_to_changing_loss() {
    let mut est = LossEstimator::new();

    // Start with low loss
    for _ in 0..50 {
        est.record_batch(100, 99); // 1% loss
    }
    let low_loss = est.loss_rate();

    // Switch to high loss
    for _ in 0..50 {
        est.record_batch(100, 70); // 30% loss
    }
    let high_loss = est.loss_rate();

    assert!(high_loss > low_loss * 5.0, "Should adapt to higher loss");
}

#[test]
fn test_estimator_zero_loss() {
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 100); // 0% loss
    }

    let loss = est.loss_rate();
    assert!(loss < 0.01, "Should be near-zero: {loss}");
}

#[test]
fn test_estimator_total_loss() {
    let mut est = LossEstimator::new();
    for _ in 0..50 {
        est.record_batch(100, 0); // 100% loss
    }

    let loss = est.loss_rate();
    assert!(loss > 0.9, "Should be near-100%: {loss}");
}

#[test]
fn test_fec_rate_increases_with_loss() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);

    let mut est_low = LossEstimator::new();
    for _ in 0..100 {
        est_low.record_batch(100, 99); // 1%
    }

    let mut est_high = LossEstimator::new();
    for _ in 0..100 {
        est_high.record_batch(100, 80); // 20%
    }

    let r_low = ctrl.compute_repair_count(100, &est_low);
    let r_high = ctrl.compute_repair_count(100, &est_high);

    assert!(
        r_high > r_low,
        "Higher loss ({r_high}) should need more repair than low loss ({r_low})"
    );
}

#[test]
fn test_fec_rate_respects_max_overhead() {
    let ctrl = FecRateController::new(1e-5, 0.3, ProtocolHint::Auto); // max 30%

    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 50); // 50% loss
    }

    let r = ctrl.compute_repair_count(100, &est);
    assert!(r <= 30, "Should be capped at 30% of 100: got {r}");
}

#[test]
fn test_pi_controller_adapts_on_failure() {
    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);

    // Repeated failures → PI should increase correction
    for _ in 0..30 {
        ctrl.feedback_update(false);
    }
    let diag_after_fail = ctrl.diagnostics();
    assert!(
        diag_after_fail.pi_correction > 0.0,
        "PI should correct upward after failures"
    );

    // Recovery → PI should decrease correction
    for _ in 0..200 {
        ctrl.feedback_update(true);
    }
    let diag_after_recovery = ctrl.diagnostics();
    assert!(
        diag_after_recovery.pi_correction < diag_after_fail.pi_correction,
        "PI should decrease after recovery"
    );
}

#[test]
fn test_pi_controller_stable_on_success() {
    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);

    // All successes — PI should stay near zero
    for _ in 0..100 {
        ctrl.feedback_update(true);
    }

    let diag = ctrl.diagnostics();
    assert!(
        diag.actual_failure_rate < 0.01,
        "Failure rate should be near zero: {}",
        diag.actual_failure_rate
    );
}

#[test]
fn test_full_control_loop() {
    // Simulate the full loop: estimate loss → compute FEC → simulate decode → feedback
    let mut est = LossEstimator::new();
    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto);

    // Phase 1: 10% loss, system should converge
    for round in 0..100 {
        est.record_batch(100, 90);
        let repair = ctrl.compute_repair_count(100, &est);

        // Simulate: with 10% loss and repair symbols, does block decode?
        // Simplified: if repair >= 10% of source, block succeeds
        let success = repair >= 10;
        ctrl.feedback_update(success);

        if round == 99 {
            assert!(repair >= 10, "Should compute enough repair at round 100");
        }
    }

    // Phase 2: loss drops to 1%
    for _ in 0..100 {
        est.record_batch(100, 99);
        let repair = ctrl.compute_repair_count(100, &est);
        ctrl.feedback_update(true);

        // Repair count should decrease
        // (may take time due to EWMA)
    }

    let final_repair = ctrl.compute_repair_count(100, &est);
    // After low loss, should need fewer repairs
    assert!(final_repair < 20, "Should need fewer repairs at 1% loss: {final_repair}");
}

#[test]
fn test_burst_detection_increases_fec() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime);

    let mut est_normal = LossEstimator::new();
    for _ in 0..50 {
        est_normal.record_batch(100, 90);
    }
    // Not in burst (recovered)
    est_normal.record_batch(100, 100);

    let mut est_burst = LossEstimator::new();
    for _ in 0..50 {
        est_burst.record_batch(100, 90);
    }
    // In burst (consecutive losses)
    est_burst.record_batch(10, 5);

    let r_normal = ctrl.compute_repair_count(100, &est_normal);
    let r_burst = ctrl.compute_repair_count(100, &est_burst);

    assert!(
        r_burst >= r_normal,
        "Burst mode should use >= repair: burst={r_burst}, normal={r_normal}"
    );
}

#[test]
fn test_rtt_estimation() {
    let mut est = LossEstimator::new();

    for _ in 0..50 {
        est.record_rtt(std::time::Duration::from_millis(20));
    }

    let rtt = est.rtt();
    assert!(
        rtt.as_millis() >= 15 && rtt.as_millis() <= 25,
        "RTT should converge to ~20ms: {:?}",
        rtt
    );
}

#[test]
fn test_throughput_estimation() {
    let mut est = LossEstimator::new();

    for _ in 0..50 {
        est.record_throughput(1_000_000.0); // 1 MB/s
    }

    let tp = est.throughput();
    assert!(
        tp > 800_000.0 && tp < 1_200_000.0,
        "Throughput should converge to ~1MB/s: {tp}"
    );
}
