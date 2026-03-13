//! ADR-0009 + ADR-0019: BBR-style delay-based congestion control tests.

use raptorpath::fec::WireSymbol;
use raptorpath::scheduler::{PathState, Scheduler};
use std::time::Duration;

fn make_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64],
    }
}

#[test]
fn test_initial_cwnd() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    assert_eq!(sched.path(0).unwrap().cwnd, 10);
}

#[test]
fn test_startup_grows_cwnd() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    // Simulate time passing so delivery rate is computable
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 5);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_after > cwnd_before,
        "cwnd should grow during startup: {cwnd_before} -> {cwnd_after}"
    );
}

#[test]
fn test_startup_doubles_per_rtt() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // In startup, cwnd should grow aggressively
    let initial = sched.path(0).unwrap().cwnd;
    assert_eq!(initial, PathState::INITIAL_CWND);

    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, initial);
    let after_first = sched.path(0).unwrap().cwnd;

    // Startup adds acked to cwnd (like slow-start)
    assert!(
        after_first >= initial * 2,
        "Startup should at least double cwnd: {initial} -> {after_first}"
    );
}

#[test]
fn test_loss_with_stable_rtt_no_reduction() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Feed stable RTT samples (no congestion signal)
    let path = sched.path_mut(0).unwrap();
    for _ in 0..5 {
        path.record_rtt_sample(Duration::from_millis(50));
    }

    // Grow cwnd
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 10);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    // FEC-recovered loss with stable RTT → should NOT reduce
    sched.on_loss(0, true);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert_eq!(
        cwnd_after, cwnd_before,
        "Wireless loss (stable RTT, FEC recovered) should not reduce cwnd"
    );
}

#[test]
fn test_loss_with_rising_rtt_reduces_cwnd() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow cwnd first
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 50);
    let cwnd_before = sched.path(0).unwrap().cwnd;
    assert!(cwnd_before > PathState::MIN_CWND);

    // Feed rising RTT samples → triggers congestion detection
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(70));
    path.record_rtt_sample(Duration::from_millis(100));
    path.record_rtt_sample(Duration::from_millis(140));

    // Now loss with congestion signal
    sched.on_loss(0, false);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_after <= cwnd_before,
        "Loss with rising RTT should reduce cwnd: {cwnd_before} -> {cwnd_after}"
    );
}

#[test]
fn test_decode_failure_with_congestion_aggressive_drain() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow cwnd
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 100);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    // Rising RTT → congestion
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(80));
    path.record_rtt_sample(Duration::from_millis(120));
    path.record_rtt_sample(Duration::from_millis(170));

    // Decode failure (fec_recovered=false) with congestion
    sched.on_loss(0, false);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_after < cwnd_before,
        "Decode failure + congestion should aggressively drain: {cwnd_before} -> {cwnd_after}"
    );
}

#[test]
fn test_cwnd_never_below_minimum() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Force congestion signal
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(80));
    path.record_rtt_sample(Duration::from_millis(120));
    path.record_rtt_sample(Duration::from_millis(170));

    for _ in 0..20 {
        sched.on_loss(0, false);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd >= PathState::MIN_CWND, "cwnd should never go below MIN_CWND=2, got {cwnd}");
}

#[test]
fn test_cwnd_capped_at_max() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Aggressive growth
    for i in 0..100 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let batch = std::cmp::min(cwnd, 1000);
        let source: Vec<_> = (0..batch).map(|j| make_symbol(i * 1000 + j, false)).collect();
        sched.schedule(source, vec![]);
        std::thread::sleep(Duration::from_millis(1));
        sched.ack(0, batch);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd <= PathState::MAX_CWND, "cwnd should be capped at MAX_CWND, got {cwnd}");
}

#[test]
fn test_loss_exits_startup() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    assert!(sched.path(0).unwrap().in_slow_start, "should start in startup");

    // Rising RTT + loss exits startup
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(80));
    path.record_rtt_sample(Duration::from_millis(120));
    path.record_rtt_sample(Duration::from_millis(170));

    sched.on_loss(0, false);
    assert!(!sched.path(0).unwrap().in_slow_start, "congestion + loss should exit startup");
}

#[test]
fn test_recovery_after_loss() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow cwnd
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 50);
    let peak = sched.path(0).unwrap().cwnd;

    // Congestion loss
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(80));
    path.record_rtt_sample(Duration::from_millis(120));
    path.record_rtt_sample(Duration::from_millis(170));
    sched.on_loss(0, false);

    let trough = sched.path(0).unwrap().cwnd;
    assert!(trough < peak, "loss should reduce cwnd");

    // Feed stable RTT to clear congestion signal, then ack to recover
    let path = sched.path_mut(0).unwrap();
    for _ in 0..5 {
        path.record_rtt_sample(Duration::from_millis(50));
    }

    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(1));
        let cwnd = sched.path(0).unwrap().cwnd;
        sched.ack(0, std::cmp::min(cwnd, 100));
    }
    let recovered = sched.path(0).unwrap().cwnd;
    assert!(
        recovered > trough,
        "cwnd should recover after loss: trough={trough} recovered={recovered}"
    );
}

#[test]
fn test_multipath_independent_cc() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.add_path(1);

    // Grow both paths
    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    std::thread::sleep(Duration::from_millis(2));
    sched.ack(0, 5);
    sched.ack(1, 5);

    let cwnd0_before = sched.path(0).unwrap().cwnd;
    let cwnd1_before = sched.path(1).unwrap().cwnd;

    // Only path 0 gets congestion signal
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(Duration::from_millis(50));
    path.record_rtt_sample(Duration::from_millis(80));
    path.record_rtt_sample(Duration::from_millis(120));
    path.record_rtt_sample(Duration::from_millis(170));
    sched.on_loss(0, false);

    let cwnd0_after = sched.path(0).unwrap().cwnd;
    let cwnd1_after = sched.path(1).unwrap().cwnd;

    assert!(cwnd0_after < cwnd0_before, "path 0 cwnd should decrease");
    assert_eq!(cwnd1_after, cwnd1_before, "path 1 cwnd should be unchanged");
}
