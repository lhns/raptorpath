//! Production stability tests for the LossEstimator.

use raptorpath::control::LossEstimator;
use std::time::Duration;

// ---------------------------------------------------------------------------
// 1. Zero sent edge case — must not panic
// ---------------------------------------------------------------------------
#[test]
fn test_zero_sent_edge_case() {
    let mut est = LossEstimator::new();
    let initial_loss = est.loss_rate();

    est.record_batch(0, 0);

    // Must not panic and loss_rate should remain at the initial value
    let after = est.loss_rate();
    assert!(
        (after - initial_loss).abs() < 1e-12,
        "record_batch(0,0) should not change loss_rate (initial={initial_loss}, after={after})"
    );
}

// ---------------------------------------------------------------------------
// 2. Convergence to 10% loss
// ---------------------------------------------------------------------------
#[test]
fn test_convergence_to_10_percent_loss() {
    let mut est = LossEstimator::new();

    for _ in 0..1000 {
        est.record_batch(100, 90);
    }

    let loss = est.loss_rate();
    assert!(
        (0.09..=0.11).contains(&loss),
        "Expected loss_rate in [0.09, 0.11], got {loss}"
    );
}

// ---------------------------------------------------------------------------
// 3. Convergence to 50% loss
// ---------------------------------------------------------------------------
#[test]
fn test_convergence_to_50_percent_loss() {
    let mut est = LossEstimator::new();

    for _ in 0..1000 {
        est.record_batch(100, 50);
    }

    let loss = est.loss_rate();
    assert!(
        (0.48..=0.52).contains(&loss),
        "Expected loss_rate in [0.48, 0.52], got {loss}"
    );
}

// ---------------------------------------------------------------------------
// 4. Convergence to zero loss
// ---------------------------------------------------------------------------
#[test]
fn test_convergence_to_zero_loss() {
    let mut est = LossEstimator::new();

    for _ in 0..1000 {
        est.record_batch(100, 100);
    }

    let loss = est.loss_rate();
    assert!(
        loss < 0.001,
        "Expected loss_rate < 0.001, got {loss}"
    );
}

// ---------------------------------------------------------------------------
// 5. Convergence to total loss
// ---------------------------------------------------------------------------
#[test]
fn test_convergence_to_total_loss() {
    let mut est = LossEstimator::new();

    for _ in 0..1000 {
        est.record_batch(100, 0);
    }

    let loss = est.loss_rate();
    assert!(
        loss > 0.99,
        "Expected loss_rate > 0.99, got {loss}"
    );
}

// ---------------------------------------------------------------------------
// 6. Upper bound exceeds point estimate at 10% loss
// ---------------------------------------------------------------------------
#[test]
fn test_loss_rate_upper_bounds_actual() {
    let mut est = LossEstimator::new();

    for _ in 0..200 {
        est.record_batch(100, 90);
    }

    let point = est.loss_rate();
    let upper = est.loss_rate_upper(0.95);

    assert!(
        upper > point,
        "loss_rate_upper(0.95) ({upper}) must be > loss_rate() ({point})"
    );
}

// ---------------------------------------------------------------------------
// 7. Adaptation from low to high loss
// ---------------------------------------------------------------------------
#[test]
fn test_adaptation_from_low_to_high_loss() {
    let mut est = LossEstimator::new();

    // Phase 1: 1% loss
    for _ in 0..100 {
        est.record_batch(100, 99);
    }
    let after_low = est.loss_rate();
    assert!(after_low < 0.05, "After 1% phase, loss should be low; got {after_low}");

    // Phase 2: 30% loss
    for _ in 0..100 {
        est.record_batch(100, 70);
    }

    let after_high = est.loss_rate();
    assert!(
        after_high > 0.2,
        "After switching to 30% loss, loss_rate ({after_high}) must be > 0.2"
    );
}

// ---------------------------------------------------------------------------
// 8. Adaptation from high to low loss
// ---------------------------------------------------------------------------
#[test]
fn test_adaptation_from_high_to_low_loss() {
    let mut est = LossEstimator::new();

    // Phase 1: 30% loss
    for _ in 0..100 {
        est.record_batch(100, 70);
    }

    // Phase 2: 1% loss
    for _ in 0..100 {
        est.record_batch(100, 99);
    }

    let loss = est.loss_rate();
    assert!(
        loss < 0.1,
        "After switching from 30% to 1% loss, loss_rate ({loss}) must be < 0.1"
    );
}

// ---------------------------------------------------------------------------
// 9. Burst detection: sustained loss triggers burst flag
// ---------------------------------------------------------------------------
#[test]
fn test_burst_detection_sustained() {
    let mut est = LossEstimator::new();

    for i in 0..10 {
        est.record_batch(100, 50); // 50% loss each batch
        assert!(
            est.is_in_burst(),
            "After batch {i} with 50% loss, is_in_burst() should be true"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Burst clears on recovery
// ---------------------------------------------------------------------------
#[test]
fn test_burst_clears_on_recovery() {
    let mut est = LossEstimator::new();

    // Enter burst
    for _ in 0..10 {
        est.record_batch(100, 50);
    }
    assert!(est.is_in_burst(), "precondition: should be in burst");

    // Recover
    for _ in 0..5 {
        est.record_batch(100, 100);
    }

    assert!(
        !est.is_in_burst(),
        "After 5 batches with 0% loss, is_in_burst() must be false"
    );
}

// ---------------------------------------------------------------------------
// 11. Single bad batch does not overreact (EWMA dampening)
// ---------------------------------------------------------------------------
#[test]
fn test_single_batch_does_not_overreact() {
    let mut est = LossEstimator::new();

    // Establish 0% baseline
    for _ in 0..100 {
        est.record_batch(100, 100);
    }

    // One terrible batch
    est.record_batch(100, 50);

    let loss = est.loss_rate();
    assert!(
        loss < 0.1,
        "A single 50% loss batch from 0% baseline should be dampened by EWMA; got {loss}"
    );
}

// ---------------------------------------------------------------------------
// 12. RTT convergence
// ---------------------------------------------------------------------------
#[test]
fn test_rtt_convergence() {
    let mut est = LossEstimator::new();

    for _ in 0..100 {
        est.record_rtt(Duration::from_millis(15));
    }

    let rtt = est.rtt();
    let rtt_ms = rtt.as_secs_f64() * 1000.0;
    assert!(
        (14.0..=16.0).contains(&rtt_ms),
        "After 100 RTT samples of 15ms, rtt() should be within [14ms, 16ms]; got {rtt_ms:.3}ms"
    );
}

// ---------------------------------------------------------------------------
// 13. Jitter with known values (RFC 3550 formula verification)
// ---------------------------------------------------------------------------
#[test]
fn test_jitter_with_known_values() {
    let mut est = LossEstimator::new();

    // Simulate packets with constant transit time of 1000us.
    // If transit time is constant, jitter should converge to 0.
    // Then introduce a known perturbation.

    // Phase 1: constant transit => jitter -> 0
    for i in 0..100u64 {
        let send_ts = i * 1000;       // send every 1000us
        let arrival = i * 1000 + 1000; // constant 1000us transit
        est.record_arrival(send_ts, arrival);
    }
    let jitter_stable = est.jitter_us();
    assert!(
        jitter_stable < 1.0,
        "With constant transit time, jitter should be ~0; got {jitter_stable}"
    );

    // Phase 2: introduce known transit variation.
    // RFC 3550: J(i) = J(i-1) + (|D(i,j)| - J(i-1)) / 16
    // where D = (Rj - Ri) - (Sj - Si) = difference in transit times.
    //
    // We'll send one packet with +160us extra transit.
    // D = 160, so J = 0 + (160 - 0)/16 = 10.0
    let send_ts = 100 * 1000;
    let arrival = 100 * 1000 + 1000 + 160; // transit = 1160us instead of 1000us
    est.record_arrival(send_ts, arrival);

    let jitter_after = est.jitter_us();
    // Expected: J = jitter_stable + (160.0 - jitter_stable) / 16.0
    let expected = jitter_stable + (160.0 - jitter_stable) / 16.0;
    assert!(
        (jitter_after - expected).abs() < 0.01,
        "Jitter should match RFC 3550 formula: expected {expected:.4}, got {jitter_after:.4}"
    );
}
