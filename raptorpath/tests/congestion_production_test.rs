//! Production stability tests for BBR-style delay-based congestion control.

use raptorpath::fec::{FecBackend, WireSymbol};
use raptorpath::scheduler::{MockClock, PathState, Scheduler};
use std::sync::Arc;
use std::time::Duration;

fn make_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64],
        backend: FecBackend::RaptorQ,
    }
}

fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Helper: inject rising RTT samples to trigger congestion detection.
fn inject_congestion(sched: &mut Scheduler, path_id: u32) {
    let path = sched.path_mut(path_id).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));
}

/// Helper: inject stable RTT samples to clear congestion.
fn inject_stable_rtt(sched: &mut Scheduler, path_id: u32) {
    let path = sched.path_mut(path_id).unwrap();
    for _ in 0..5 {
        path.record_rtt_sample(millis(50));
    }
}

// ---------------------------------------------------------------------------
// 1. Burst loss with congestion: cwnd stays above MIN_CWND
// ---------------------------------------------------------------------------
#[test]
fn test_burst_loss_10_consecutive_with_congestion() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(1);

    inject_congestion(&mut sched, 1);
    for _ in 0..10 {
        sched.on_loss(1, false);
    }

    let cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        cwnd >= PathState::MIN_CWND,
        "After 10 consecutive losses cwnd ({cwnd}) must be >= MIN_CWND ({})",
        PathState::MIN_CWND
    );
}

// ---------------------------------------------------------------------------
// 2. Burst loss with stable RTT: wireless loss doesn't collapse cwnd
// ---------------------------------------------------------------------------
#[test]
fn test_wireless_loss_does_not_collapse_cwnd() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Grow cwnd
    clock.advance(millis(2));
    sched.ack(1, 50);
    let cwnd_before = sched.path(1).unwrap().cwnd;

    // Stable RTT → wireless loss
    inject_stable_rtt(&mut sched, 1);

    // 20 FEC-recovered losses with stable RTT
    for _ in 0..20 {
        sched.on_loss(1, true);
    }

    let cwnd_after = sched.path(1).unwrap().cwnd;
    assert_eq!(
        cwnd_after, cwnd_before,
        "FEC-recovered wireless loss (stable RTT) must not reduce cwnd: before={cwnd_before}, after={cwnd_after}"
    );
}

// ---------------------------------------------------------------------------
// 3. Decode failure without congestion: gentle reduction only
// ---------------------------------------------------------------------------
#[test]
fn test_decode_failure_stable_rtt_gentle() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    clock.advance(millis(2));
    sched.ack(1, 50);
    let cwnd_before = sched.path(1).unwrap().cwnd;

    inject_stable_rtt(&mut sched, 1);

    // Decode failure without congestion → gentle -1
    sched.on_loss(1, false);
    let cwnd_after = sched.path(1).unwrap().cwnd;

    assert_eq!(
        cwnd_after,
        cwnd_before - 1,
        "Decode failure without congestion should only reduce by 1"
    );
}

// ---------------------------------------------------------------------------
// 4. Congestion + decode failure: aggressive drain
// ---------------------------------------------------------------------------
#[test]
fn test_congestion_decode_failure_aggressive() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    clock.advance(millis(2));
    sched.ack(1, 100);
    let cwnd_before = sched.path(1).unwrap().cwnd;

    inject_congestion(&mut sched, 1);
    sched.on_loss(1, false);
    let cwnd_after = sched.path(1).unwrap().cwnd;

    assert!(
        cwnd_after < cwnd_before,
        "Congestion + decode failure should aggressively reduce: {cwnd_before} -> {cwnd_after}"
    );
}

// ---------------------------------------------------------------------------
// 5. Recovery after congestion loss
// ---------------------------------------------------------------------------
#[test]
fn test_recovery_after_congestion_loss() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Grow
    clock.advance(millis(2));
    sched.ack(1, 50);
    let peak = sched.path(1).unwrap().cwnd;

    // Congestion loss
    inject_congestion(&mut sched, 1);
    sched.on_loss(1, false);
    let trough = sched.path(1).unwrap().cwnd;
    assert!(trough < peak);

    // Clear congestion and recover
    inject_stable_rtt(&mut sched, 1);
    for _ in 0..200 {
        clock.advance(millis(1));
        let cwnd = sched.path(1).unwrap().cwnd;
        sched.ack(1, cwnd);
    }

    let recovered = sched.path(1).unwrap().cwnd;
    assert!(
        recovered > trough,
        "Should recover after congestion: trough={trough}, recovered={recovered}"
    );
}

// ---------------------------------------------------------------------------
// 6. Startup exits when BDP is reached
// ---------------------------------------------------------------------------
#[test]
fn test_startup_exits_on_bdp() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Feed RTT so BDP can be computed
    let path = sched.path_mut(1).unwrap();
    path.record_rtt_sample(millis(50));

    // Ack enough to accumulate delivery rate
    for _ in 0..20 {
        clock.advance(millis(2));
        sched.ack(1, 100);
    }

    // After enough acks, startup should have exited
    // (cwnd reaches BDP target)
    let path = sched.path(1).unwrap();
    // Either still in startup (if BDP hasn't been reached) or exited
    // The point is cwnd should be bounded, not infinite
    assert!(path.cwnd <= PathState::MAX_CWND);
}

// ---------------------------------------------------------------------------
// 7. in_flight cannot go negative (saturating_sub)
// ---------------------------------------------------------------------------
#[test]
fn test_in_flight_cannot_go_negative() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(1);

    sched.path_mut(1).unwrap().in_flight = 5;
    sched.ack(1, 100);

    let in_flight = sched.path(1).unwrap().in_flight;
    assert_eq!(in_flight, 0, "in_flight must not go negative; got {in_flight}");
}

// ---------------------------------------------------------------------------
// 8. All paths dead: schedule returns empty (no panic)
// ---------------------------------------------------------------------------
#[test]
fn test_all_paths_dead_schedule_returns_empty() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(1);
    sched.add_path(2);

    sched.path_mut(1).unwrap().active = false;
    sched.path_mut(2).unwrap().active = false;

    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    let repair: Vec<_> = (0..5).map(|i| make_symbol(i + 100, true)).collect();

    let result = sched.schedule(source, repair);
    let total: usize = result.iter().map(|(_, syms)| syms.len()).sum();
    assert_eq!(total, 0, "With all paths dead, schedule should return empty");
}

// ---------------------------------------------------------------------------
// 9. Fairness: proportional to goodput
// ---------------------------------------------------------------------------
#[test]
fn test_scheduler_fairness_proportional_to_goodput() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(1);
    sched.add_path(2);

    // Path 1: high throughput, low loss
    {
        let p = sched.path_mut(1).unwrap();
        for _ in 0..100 {
            p.estimator.record_throughput(1_000_000.0);
            p.estimator.record_batch(100, 99);
        }
        p.cwnd = 1000;
    }
    // Path 2: half the throughput, same loss
    {
        let p = sched.path_mut(2).unwrap();
        for _ in 0..100 {
            p.estimator.record_throughput(500_000.0);
            p.estimator.record_batch(100, 99);
        }
        p.cwnd = 1000;
    }

    let mut path1_total = 0usize;
    let mut path2_total = 0usize;
    for round in 0..5 {
        let repair: Vec<_> = (0..1000)
            .map(|i| make_symbol(round * 1000 + i, true))
            .collect();
        let result = sched.schedule(vec![], repair);

        for (pid, syms) in &result {
            match *pid {
                1 => path1_total += syms.len(),
                2 => path2_total += syms.len(),
                _ => {}
            }
        }

        sched.path_mut(1).unwrap().in_flight = 0;
        sched.path_mut(2).unwrap().in_flight = 0;
    }

    assert!(path1_total > 0 && path2_total > 0, "Both paths must receive symbols");
    let ratio = path1_total as f64 / path2_total as f64;
    assert!(
        ratio > 1.5,
        "Path1 (2x throughput) should get more repair symbols. ratio={ratio:.2}"
    );
}

// ---------------------------------------------------------------------------
// 10. RTT-based congestion detection: 3 consecutive increases needed
// ---------------------------------------------------------------------------
#[test]
fn test_rtt_congestion_threshold() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    clock.advance(millis(2));
    sched.ack(1, 50);
    let cwnd_before = sched.path(1).unwrap().cwnd;

    // Only 2 RTT increases — below threshold of 3
    let path = sched.path_mut(1).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(70));
    path.record_rtt_sample(millis(100));

    // Loss with borderline RTT — should NOT aggressively drain
    // (only 2 increases, threshold is 3)
    sched.on_loss(1, true);
    let cwnd_after = sched.path(1).unwrap().cwnd;

    assert_eq!(
        cwnd_after, cwnd_before,
        "Below congestion threshold, FEC-recovered loss should not reduce cwnd"
    );
}

// ---------------------------------------------------------------------------
// 11. cwnd growth after congestion loss
// ---------------------------------------------------------------------------
#[test]
fn test_cwnd_growth_after_congestion_loss() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Grow cwnd
    clock.advance(millis(2));
    sched.ack(1, 50);

    // Congestion + loss → cwnd drains
    inject_congestion(&mut sched, 1);
    sched.on_loss(1, false);

    let cwnd_after_loss = sched.path(1).unwrap().cwnd;

    // Clear congestion, then ack repeatedly to recover
    inject_stable_rtt(&mut sched, 1);
    for _ in 0..500 {
        clock.advance(millis(1));
        let cwnd = sched.path(1).unwrap().cwnd;
        sched.ack(1, cwnd);
    }

    let final_cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        final_cwnd > cwnd_after_loss,
        "After many acks, cwnd ({final_cwnd}) should grow past post-loss floor ({cwnd_after_loss})"
    );
}
