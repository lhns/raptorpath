//! ADR-0009: Congestion control tests.

use raptorpath::fec::WireSymbol;
use raptorpath::scheduler::Scheduler;
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
fn test_slow_start_grows_cwnd() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Send and ack — cwnd should grow in slow start
    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    sched.ack(0, 5);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_after > cwnd_before,
        "cwnd should grow in slow start: {cwnd_before} -> {cwnd_after}"
    );
}

#[test]
fn test_slow_start_doubles_per_rtt() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // In slow start, cwnd += acked, so acking 10 from initial 10 => 20
    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    sched.ack(0, 10);

    assert_eq!(sched.path(0).unwrap().cwnd, 20, "slow start should double cwnd");
}

#[test]
fn test_congestion_avoidance_after_ssthresh() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // ssthresh=64, so grow past it with repeated acks
    // After reaching ssthresh, growth should be slower (additive)
    for _ in 0..10 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let source: Vec<_> = (0..cwnd).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, cwnd);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd > 64, "should have passed ssthresh");

    // Now each ack should only add ~1 (congestion avoidance)
    let before = sched.path(0).unwrap().cwnd;
    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    sched.ack(0, 10);
    let after = sched.path(0).unwrap().cwnd;

    // In congestion avoidance, increase is small: max(1, acked/cwnd)
    // With acked=10 and cwnd>64, increase should be 1
    assert!(
        after - before <= 2,
        "congestion avoidance should grow slowly: {before} -> {after}"
    );
}

#[test]
fn test_loss_without_fec_halves_cwnd() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow cwnd first
    for _ in 0..5 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let source: Vec<_> = (0..cwnd).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, cwnd);
    }

    let before = sched.path(0).unwrap().cwnd;
    assert!(before > 10, "cwnd should have grown: {before}");

    // Congestion loss (block failed to decode)
    sched.on_loss(0, false);
    let after = sched.path(0).unwrap().cwnd;

    assert_eq!(after, before / 2, "cwnd should halve on congestion loss");
}

#[test]
fn test_loss_with_fec_gentle_reduction() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow cwnd
    for _ in 0..3 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let source: Vec<_> = (0..cwnd).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, cwnd);
    }

    let before = sched.path(0).unwrap().cwnd;
    // Random loss but FEC recovered
    sched.on_loss(0, true);
    let after = sched.path(0).unwrap().cwnd;

    assert_eq!(
        after,
        before - 1,
        "FEC-recovered loss should only reduce cwnd by 1"
    );
}

#[test]
fn test_cwnd_never_below_minimum() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Force cwnd down with repeated congestion losses
    for _ in 0..20 {
        sched.on_loss(0, false);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd >= 2, "cwnd should never go below MIN_CWND=2, got {cwnd}");
}

#[test]
fn test_cwnd_capped_at_max() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Aggressive growth
    for _ in 0..100 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let batch = std::cmp::min(cwnd, 1000); // don't allocate too much
        let source: Vec<_> = (0..batch).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, batch);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd <= 10_000, "cwnd should be capped at MAX_CWND, got {cwnd}");
}

#[test]
fn test_loss_exits_slow_start() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Should start in slow start
    let path = sched.path(0).unwrap();
    assert!(path.in_slow_start, "should start in slow start");

    // Congestion loss exits slow start
    sched.on_loss(0, false);
    let path = sched.path(0).unwrap();
    assert!(!path.in_slow_start, "loss should exit slow start");
}

#[test]
fn test_ack_with_on_loss_recovers() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    // Grow → lose → recover
    for _ in 0..3 {
        let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, 10);
    }
    let peak = sched.path(0).unwrap().cwnd;

    sched.on_loss(0, false); // halve
    let trough = sched.path(0).unwrap().cwnd;
    assert!(trough < peak);

    // Now grow back (in congestion avoidance, slower)
    for _ in 0..100 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let batch = std::cmp::min(cwnd, 100);
        let source: Vec<_> = (0..batch).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(0, batch);
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
    for pid in [0, 1] {
        let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
        sched.schedule(source, vec![]);
        sched.ack(pid, 10);
    }

    let cwnd0_before = sched.path(0).unwrap().cwnd;
    let cwnd1_before = sched.path(1).unwrap().cwnd;

    // Only path 0 has loss
    sched.on_loss(0, false);

    let cwnd0_after = sched.path(0).unwrap().cwnd;
    let cwnd1_after = sched.path(1).unwrap().cwnd;

    assert!(cwnd0_after < cwnd0_before, "path 0 cwnd should decrease");
    assert_eq!(cwnd1_after, cwnd1_before, "path 1 cwnd should be unchanged");
}
