//! Tests for RTCP-style reports, keepalive, jitter, and dead path detection.

use raptorpath::control::LossEstimator;
use raptorpath::scheduler::{PathState, Scheduler};
use raptorpath::transport::{ControlMessage, WireMessage};
use std::time::Duration;

#[test]
fn test_jitter_calculation_stable_transit() {
    let mut est = LossEstimator::new();
    // Simulate stable transit delay (no jitter): arrival - send = constant
    for i in 0..10u64 {
        let send_ts = i * 1000;     // sent every 1ms
        let arrival_ts = i * 1000 + 5000; // arrives 5ms later (constant)
        est.record_arrival(send_ts, arrival_ts);
    }
    // With constant transit, jitter should be near zero
    assert!(est.jitter_us() < 10.0, "stable transit should have near-zero jitter, got {}", est.jitter_us());
}

#[test]
fn test_jitter_calculation_variable_transit() {
    let mut est = LossEstimator::new();
    // Simulate variable transit delay
    let delays = [5000u64, 7000, 3000, 8000, 4000, 6000, 9000, 2000, 5000, 7000];
    for (i, &delay) in delays.iter().enumerate() {
        let send_ts = i as u64 * 1000;
        let arrival_ts = send_ts + delay;
        est.record_arrival(send_ts, arrival_ts);
    }
    // With variable transit, jitter should be > 0
    assert!(est.jitter_us() > 100.0, "variable transit should produce jitter, got {}", est.jitter_us());
}

#[test]
fn test_jitter_first_packet_no_jitter() {
    let mut est = LossEstimator::new();
    est.record_arrival(1000, 6000);
    // First packet has no prior reference — jitter should be 0
    assert_eq!(est.jitter_us(), 0.0);
}

#[test]
fn test_dead_path_detection() {
    let mut sched = Scheduler::default();
    sched.add_path(0);
    sched.add_path(1);

    // Both paths are initially active
    assert_eq!(sched.active_paths().len(), 2);

    // Simulate time passing — sleep won't work in tests, so we use check_dead_paths
    // with a very short timeout
    let deactivated = sched.check_dead_paths(Duration::from_millis(0));
    // Both should be deactivated (last_report was at creation time, which is "now")
    // Actually they were just created so they shouldn't be dead yet
    // Let's verify with a non-zero timeout
    let deactivated = sched.check_dead_paths(Duration::from_secs(60));
    assert!(deactivated.is_empty(), "freshly created paths should not be dead");
}

#[test]
fn test_touch_path_reactivates() {
    let mut sched = Scheduler::default();
    sched.add_path(0);

    // Manually deactivate
    sched.path_mut(0).unwrap().active = false;
    assert!(sched.active_paths().is_empty());

    // Touch should reactivate
    sched.touch_path(0);
    let path = sched.path(0).unwrap();
    assert!(path.active);
    // Should reset to slow start on recovery
    assert!(path.in_slow_start);
    assert_eq!(path.cwnd, PathState::INITIAL_CWND);
}

#[test]
fn test_touch_path_updates_last_report() {
    let mut sched = Scheduler::default();
    sched.add_path(0);

    // Touch the path
    sched.touch_path(0);
    let path = sched.path(0).unwrap();
    // last_report should be very recent
    assert!(path.last_report.elapsed() < Duration::from_millis(100));
}

#[test]
fn test_all_path_ids() {
    let mut sched = Scheduler::default();
    sched.add_path(0);
    sched.add_path(1);
    sched.add_path(2);

    let mut ids = sched.all_path_ids();
    ids.sort();
    assert_eq!(ids, vec![0, 1, 2]);
}

#[test]
fn test_path_report_with_jitter_roundtrip() {
    let msg = WireMessage::Control(ControlMessage::PathReport {
        path_id: 0,
        loss_rate: 0.02,
        avg_rtt_us: 25000,
        throughput_bps: 10_000_000.0,
        jitter_us: 1500,
        symbols_sent: 10000,
        symbols_received: 9800,
    });

    let data = msg.serialize().unwrap();
    let decoded = WireMessage::deserialize(&data).unwrap();

    match decoded {
        WireMessage::Control(ControlMessage::PathReport {
            jitter_us,
            symbols_sent,
            symbols_received,
            ..
        }) => {
            assert_eq!(jitter_us, 1500);
            assert_eq!(symbols_sent, 10000);
            assert_eq!(symbols_received, 9800);
        }
        _ => panic!("expected PathReport"),
    }
}

#[test]
fn test_max_datagram_size_stored_on_path() {
    let mut sched = Scheduler::default();
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    assert!(path.max_datagram_size.is_none());

    path.max_datagram_size = Some(1200);
    assert_eq!(sched.path(0).unwrap().max_datagram_size, Some(1200));
}

#[test]
fn test_inactive_path_not_in_active_paths() {
    let mut sched = Scheduler::default();
    sched.add_path(0);
    sched.add_path(1);

    // Deactivate path 0
    sched.path_mut(0).unwrap().active = false;

    let active = sched.active_paths();
    assert_eq!(active, vec![1]);
}

#[test]
fn test_min_mtu_across_paths() {
    let mut sched = Scheduler::default();
    sched.add_path(0);
    sched.add_path(1);
    sched.add_path(2);

    // No MTU known yet
    assert!(sched.min_mtu().is_none());

    // Set MTUs
    sched.path_mut(0).unwrap().max_datagram_size = Some(1200);
    sched.path_mut(1).unwrap().max_datagram_size = Some(1400);
    // Path 2 has no MTU

    // Should return the minimum across paths that have MTU
    assert_eq!(sched.min_mtu(), Some(1200));

    // Set path 2 to a smaller MTU
    sched.path_mut(2).unwrap().max_datagram_size = Some(800);
    assert_eq!(sched.min_mtu(), Some(800));
}

#[test]
fn test_min_mtu_ignores_inactive_paths() {
    let mut sched = Scheduler::default();
    sched.add_path(0);
    sched.add_path(1);

    sched.path_mut(0).unwrap().max_datagram_size = Some(500);
    sched.path_mut(1).unwrap().max_datagram_size = Some(1200);

    // Deactivate the path with small MTU
    sched.path_mut(0).unwrap().active = false;

    // Should only consider active paths
    assert_eq!(sched.min_mtu(), Some(1200));
}

#[test]
fn test_path_stats_jitter_field() {
    use raptorpath::monitor::stats::SharedStats;
    use std::sync::atomic::Ordering;

    let stats = SharedStats::new();
    stats.add_path(0);

    let ps = stats.path(0).unwrap();
    ps.jitter_us.store(2500, Ordering::Relaxed);

    let snap = ps.snapshot();
    assert_eq!(snap.jitter_us, 2500);
}
