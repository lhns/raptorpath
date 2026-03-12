//! Scheduler tests: multipath symbol distribution.

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
fn test_single_path_gets_everything() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let repair: Vec<_> = (100..103).map(|i| make_symbol(i, true)).collect();

    let result = sched.schedule(source, repair);
    let total: usize = result.iter().map(|(_, s)| s.len()).sum();
    assert_eq!(total, 8);
}

#[test]
fn test_source_prefers_low_rtt() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.add_path(1);

    sched
        .path_mut(0)
        .unwrap()
        .estimator
        .record_rtt(Duration::from_millis(100));
    sched
        .path_mut(1)
        .unwrap()
        .estimator
        .record_rtt(Duration::from_millis(5));

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    let path1_source = result
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, s)| s.len())
        .unwrap_or(0);

    assert!(path1_source > 0, "Low-RTT path should get source symbols");
}

#[test]
fn test_all_symbols_distributed() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.add_path(1);
    sched.add_path(2);

    let source: Vec<_> = (0..20).map(|i| make_symbol(i, false)).collect();
    let repair: Vec<_> = (100..110).map(|i| make_symbol(i, true)).collect();

    let result = sched.schedule(source, repair);
    let total: usize = result.iter().map(|(_, s)| s.len()).sum();
    assert_eq!(total, 30, "All symbols should be distributed");
}

#[test]
fn test_inactive_path_excluded() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.add_path(1);
    sched.path_mut(1).unwrap().active = false;

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    let path1_count = result
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, s)| s.len())
        .unwrap_or(0);

    assert_eq!(path1_count, 0, "Inactive path should get no symbols");
}

#[test]
fn test_in_flight_tracking() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);

    assert_eq!(sched.path(0).unwrap().in_flight, 5);

    sched.ack(0, 3);
    assert_eq!(sched.path(0).unwrap().in_flight, 2);

    sched.ack(0, 10); // over-ack
    assert_eq!(sched.path(0).unwrap().in_flight, 0);
}

#[test]
fn test_cwnd_limits_scheduling() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.path_mut(0).unwrap().cwnd = 3; // very small window

    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    // Should still schedule all symbols (overflow goes to first path)
    let total: usize = result.iter().map(|(_, s)| s.len()).sum();
    assert_eq!(total, 10);
}

#[test]
fn test_remove_path() {
    let mut sched = Scheduler::new();
    sched.add_path(0);
    sched.add_path(1);
    assert!(sched.path(1).is_some());

    sched.remove_path(1);
    assert!(sched.path(1).is_none());
}

#[test]
fn test_schedule_empty_symbols() {
    let mut sched = Scheduler::new();
    sched.add_path(0);

    let result = sched.schedule(vec![], vec![]);
    let total: usize = result.iter().map(|(_, s)| s.len()).sum();
    assert_eq!(total, 0);
}

#[test]
fn test_no_paths_available() {
    let mut sched = Scheduler::new();
    // No paths added

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    let result = sched.schedule(source, vec![]);

    // Should not panic, symbols are just not assigned
    let total: usize = result.iter().map(|(_, s)| s.len()).sum();
    // Symbols are lost if no paths available — this is expected
}
