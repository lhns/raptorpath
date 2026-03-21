//! Tests for the control loop: loss estimation → FEC rate → BOCD prediction.

use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};
use raptorpath::fec::FecBackend;

const W: usize = 50; // typical window size for tests

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
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);

    let mut est_low = LossEstimator::new();
    for _ in 0..100 {
        est_low.record_batch(100, 99); // 1%
    }

    let mut est_high = LossEstimator::new();
    for _ in 0..100 {
        est_high.record_batch(100, 80); // 20%
    }

    let r_low = ctrl.compute_repair_count(100, &est_low, W);
    let r_high = ctrl.compute_repair_count(100, &est_high, W);

    assert!(
        r_high > r_low,
        "Higher loss ({r_high}) should need more repair than low loss ({r_low})"
    );
}

#[test]
fn test_fec_rate_respects_max_overhead() {
    let ctrl = FecRateController::new(1e-5, 0.3, ProtocolHint::Auto, FecBackend::RaptorQ, 1200); // max 30%

    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 50); // 50% loss
    }

    let r = ctrl.compute_repair_count(100, &est, W);
    assert!(r <= 30, "Should be capped at 30% of 100: got {r}");
}

#[test]
fn test_bocd_adapts_to_regime_change() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);

    // Phase 1: low loss
    let mut est = LossEstimator::new();
    for _ in 0..50 {
        est.record_batch(100, 99); // 1% loss
    }
    let repair_low = ctrl.compute_repair_count(100, &est, W);

    // Phase 2: high loss — BOCD should adapt within 15 samples
    for _ in 0..15 {
        est.record_batch(100, 85); // 15% loss
    }
    let repair_high = ctrl.compute_repair_count(100, &est, W);

    assert!(
        repair_high > repair_low,
        "After regime change, repair should increase: low={repair_low}, high={repair_high}"
    );
}

#[test]
fn test_feedback_update_is_noop() {
    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);
    for _ in 0..30 {
        ctrl.feedback_update(false);
    }
    let diag = ctrl.diagnostics();
    assert!(diag.pi_correction.abs() < 1e-10, "PI correction should be zero");
    assert!(diag.actual_failure_rate.abs() < 1e-10, "Failure rate should be zero");
}

#[test]
fn test_full_control_loop() {
    let mut est = LossEstimator::new();
    let mut ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::RaptorQ, 1200);

    // Phase 1: 10% loss
    for round in 0..100 {
        est.record_batch(100, 90);
        let repair = ctrl.compute_repair_count(100, &est, W);
        let success = repair >= 10;
        ctrl.feedback_update(success);

        if round == 99 {
            assert!(repair >= 10, "Should compute enough repair at round 100");
        }
    }

    // Phase 2: loss drops to 1%
    for _ in 0..100 {
        est.record_batch(100, 99);
        ctrl.feedback_update(true);
    }

    let final_repair = ctrl.compute_repair_count(100, &est, W);
    assert!(final_repair < 20, "Should need fewer repairs at 1% loss: {final_repair}");
}

#[test]
fn test_burst_detection_increases_fec() {
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::RaptorQ, 1200);

    let mut est_normal = LossEstimator::new();
    for _ in 0..50 {
        est_normal.record_batch(100, 90);
    }
    est_normal.record_batch(100, 100);

    let mut est_burst = LossEstimator::new();
    for _ in 0..50 {
        est_burst.record_batch(100, 90);
    }
    est_burst.record_batch(10, 5);

    let r_normal = ctrl.compute_repair_count(100, &est_normal, W);
    let r_burst = ctrl.compute_repair_count(100, &est_burst, W);

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
        est.record_throughput(1_000_000.0);
    }

    let tp = est.throughput();
    assert!(
        tp > 800_000.0 && tp < 1_200_000.0,
        "Throughput should converge to ~1MB/s: {tp}"
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
    assert!(uncapped > 0.05, "Uncapped should want > 5%: {uncapped}");
    assert!(capped <= 0.05, "Capped should be ≤ 5%: {capped}");
}

#[test]
fn test_predictive_loss_tracks_bocd() {
    let mut est = LossEstimator::new();

    for _ in 0..30 {
        est.record_batch(100, 90);
    }

    let pred = est.predictive_loss_upper(0.95);
    assert!(
        pred > 0.05 && pred < 0.3,
        "Predictive upper bound should be reasonable at 10% loss: {pred}"
    );

    for _ in 0..50 {
        est.record_batch(100, 100);
    }
    let pred_low = est.predictive_loss_upper(0.95);
    assert!(
        pred_low < pred,
        "Predictive should decrease after zero loss: {pred_low} < {pred}"
    );
}

#[test]
fn test_hint_controls_tail_loss_not_offset() {
    // Realtime with target_tail_loss=1e-5 should behave like Auto with 1e-7
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
