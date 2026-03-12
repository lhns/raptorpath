//! Production stability tests for the multipath scheduler and congestion control.

use raptorpath::fec::WireSymbol;
use raptorpath::scheduler::{PathState, Scheduler};


fn make_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64],
    }
}

// ---------------------------------------------------------------------------
// 1. Burst loss: 10 consecutive congestion losses
// ---------------------------------------------------------------------------
#[test]
fn test_burst_loss_10_consecutive() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

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
// 2. Burst loss: 100 consecutive congestion losses — must clamp at MIN_CWND
// ---------------------------------------------------------------------------
#[test]
fn test_burst_loss_100_consecutive() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    for _ in 0..100 {
        sched.on_loss(1, false);
    }

    let cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        cwnd >= PathState::MIN_CWND,
        "After 100 consecutive losses cwnd ({cwnd}) must be >= MIN_CWND ({})",
        PathState::MIN_CWND
    );
    assert_eq!(
        cwnd,
        PathState::MIN_CWND,
        "After 100 consecutive losses cwnd should be exactly MIN_CWND"
    );
}

// ---------------------------------------------------------------------------
// 3. ssthresh is set to cwnd/2 on congestion loss
// ---------------------------------------------------------------------------
#[test]
fn test_ssthresh_set_on_congestion_loss() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Grow cwnd to a known value first
    for _ in 0..50 {
        sched.ack(1, 10);
    }
    let cwnd_before = sched.path(1).unwrap().cwnd;
    assert!(cwnd_before > PathState::MIN_CWND * 2, "precondition: cwnd should be large enough");

    sched.on_loss(1, false);

    let path = sched.path(1).unwrap();
    let expected_ssthresh = std::cmp::max(cwnd_before / 2, PathState::MIN_CWND);
    assert_eq!(
        path.ssthresh, expected_ssthresh,
        "ssthresh ({}) should equal cwnd_before/2 = max({}/2, MIN_CWND) = {expected_ssthresh}",
        path.ssthresh, cwnd_before
    );
    assert_eq!(
        path.cwnd, expected_ssthresh,
        "cwnd should equal ssthresh after congestion loss"
    );
}

// ---------------------------------------------------------------------------
// 4. FEC-recovered loss does NOT change ssthresh
// ---------------------------------------------------------------------------
#[test]
fn test_ssthresh_not_reset_on_fec_loss() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Grow cwnd, then set a known ssthresh via a congestion loss
    for _ in 0..30 {
        sched.ack(1, 10);
    }
    sched.on_loss(1, false); // sets ssthresh
    let ssthresh_before = sched.path(1).unwrap().ssthresh;

    // Now trigger FEC-recovered losses
    for _ in 0..5 {
        sched.on_loss(1, true);
    }

    let ssthresh_after = sched.path(1).unwrap().ssthresh;
    assert_eq!(
        ssthresh_before, ssthresh_after,
        "FEC-recovered loss must not change ssthresh (before={ssthresh_before}, after={ssthresh_after})"
    );
}

// ---------------------------------------------------------------------------
// 5. Recovery after loss reaches prior peak within 200 ack rounds
// ---------------------------------------------------------------------------
#[test]
fn test_recovery_after_loss_reaches_prior_peak() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Grow to a peak
    for _ in 0..100 {
        sched.ack(1, 10);
    }
    let peak = sched.path(1).unwrap().cwnd;

    // Suffer a congestion loss
    sched.on_loss(1, false);
    let cwnd_after_loss = sched.path(1).unwrap().cwnd;
    assert!(cwnd_after_loss < peak, "precondition: loss should reduce cwnd");

    // Recover with acks
    for _ in 0..200 {
        let cwnd = sched.path(1).unwrap().cwnd;
        sched.ack(1, cwnd); // ack one full window
    }

    let recovered = sched.path(1).unwrap().cwnd;
    let threshold = (peak as f64 * 0.9) as u32;
    assert!(
        recovered >= threshold,
        "After 200 ack rounds, cwnd ({recovered}) should reach at least 90% of peak ({peak}), threshold={threshold}"
    );
}

// ---------------------------------------------------------------------------
// 6. Slow start is exponential: cwnd doubles each full-window ack
// ---------------------------------------------------------------------------
#[test]
fn test_slow_start_is_exponential() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Set ssthresh high so we stay in slow start
    sched.path_mut(1).unwrap().ssthresh = 10_000;

    let initial = sched.path(1).unwrap().cwnd;
    assert_eq!(initial, PathState::INITIAL_CWND);

    // Ack one full window: cwnd should double
    sched.ack(1, initial);
    let after_first = sched.path(1).unwrap().cwnd;
    assert_eq!(after_first, initial * 2, "First round: cwnd should double from {initial} to {}", initial * 2);

    // Second round
    sched.ack(1, after_first);
    let after_second = sched.path(1).unwrap().cwnd;
    assert_eq!(after_second, after_first * 2, "Second round: cwnd should double from {after_first} to {}", after_first * 2);
}

// ---------------------------------------------------------------------------
// 7. Congestion avoidance is linear (additive increase, not exponential)
// ---------------------------------------------------------------------------
#[test]
fn test_congestion_avoidance_is_linear() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Force out of slow start
    let path = sched.path_mut(1).unwrap();
    path.cwnd = 100;
    path.in_slow_start = false;
    path.ssthresh = 50;

    let mut prev_cwnd = 100u32;
    let mut max_increment = 0u32;

    for _ in 0..50 {
        let cwnd = sched.path(1).unwrap().cwnd;
        sched.ack(1, cwnd); // ack one full window
        let new_cwnd = sched.path(1).unwrap().cwnd;
        let increment = new_cwnd - prev_cwnd;
        if increment > max_increment {
            max_increment = increment;
        }
        prev_cwnd = new_cwnd;
    }

    // In congestion avoidance, growth per window should be +1 (linear)
    // Due to integer rounding, allow up to +2
    assert!(
        max_increment <= 2,
        "Congestion avoidance growth should be linear (+1 per window), but saw max increment of {max_increment}"
    );
}

// ---------------------------------------------------------------------------
// 8. in_flight cannot go negative (saturating_sub)
// ---------------------------------------------------------------------------
#[test]
fn test_in_flight_cannot_go_negative() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Set in_flight to a small value and ack more
    sched.path_mut(1).unwrap().in_flight = 5;
    sched.ack(1, 100); // ack way more than in_flight

    let in_flight = sched.path(1).unwrap().in_flight;
    assert_eq!(in_flight, 0, "in_flight must not go negative; got {in_flight}");
}

// ---------------------------------------------------------------------------
// 9. All paths dead: schedule returns empty (no panic)
// ---------------------------------------------------------------------------
#[test]
fn test_all_paths_dead_schedule_returns_empty() {
    let mut sched = Scheduler::new();
    sched.add_path(1);
    sched.add_path(2);

    // Deactivate all paths
    sched.path_mut(1).unwrap().active = false;
    sched.path_mut(2).unwrap().active = false;

    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    let repair: Vec<_> = (0..5).map(|i| make_symbol(i + 100, true)).collect();

    let result = sched.schedule(source, repair);

    // No active path => nothing should be scheduled
    let total: usize = result.iter().map(|(_, syms)| syms.len()).sum();
    assert_eq!(
        total, 0,
        "With all paths dead, schedule should return empty assignments, but got {total} symbols"
    );
}

// ---------------------------------------------------------------------------
// 10. Fairness: proportional to goodput
// ---------------------------------------------------------------------------
#[test]
fn test_scheduler_fairness_proportional_to_goodput() {
    let mut sched = Scheduler::new();
    sched.add_path(1);
    sched.add_path(2);

    // Path 1: high throughput, low loss
    {
        let p = sched.path_mut(1).unwrap();
        for _ in 0..100 {
            p.estimator.record_throughput(1_000_000.0);
            p.estimator.record_batch(100, 99); // ~1% loss
        }
        p.cwnd = 1000;
    }
    // Path 2: half the throughput, same loss
    {
        let p = sched.path_mut(2).unwrap();
        for _ in 0..100 {
            p.estimator.record_throughput(500_000.0);
            p.estimator.record_batch(100, 99); // ~1% loss
        }
        p.cwnd = 1000;
    }

    // Schedule a large batch of repair symbols in one round to minimize
    // rounding effects from the scheduler's ceil-based distribution.
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

        // Reset in_flight so paths remain available
        sched.path_mut(1).unwrap().in_flight = 0;
        sched.path_mut(2).unwrap().in_flight = 0;
    }

    assert!(path1_total > 0 && path2_total > 0, "Both paths must receive symbols");

    let ratio = path1_total as f64 / path2_total as f64;
    // The path with 2x throughput should receive more symbols.
    // Due to ceil-based rounding in the scheduler, the ratio may be
    // somewhat higher than the theoretical 2:1.
    assert!(
        ratio > 1.5,
        "Path1 (2x throughput) should get substantially more repair symbols than path2. \
         Got path1={path1_total}, path2={path2_total}, ratio={ratio:.2}"
    );
}

// ---------------------------------------------------------------------------
// 11. cwnd growth from MIN_CWND after loss — eventually grows past ssthresh
// ---------------------------------------------------------------------------
#[test]
fn test_cwnd_growth_from_minimum_after_loss() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Grow then lose hard
    for _ in 0..50 {
        sched.ack(1, 10);
    }
    for _ in 0..100 {
        sched.on_loss(1, false);
    }

    let path = sched.path(1).unwrap();
    assert_eq!(path.cwnd, PathState::MIN_CWND, "precondition: cwnd should be at MIN");
    let ssthresh = path.ssthresh;

    // Now ack repeatedly; cwnd must eventually exceed ssthresh
    for _ in 0..500 {
        let cwnd = sched.path(1).unwrap().cwnd;
        sched.ack(1, cwnd);
    }

    let final_cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        final_cwnd > ssthresh,
        "After many acks from MIN_CWND, cwnd ({final_cwnd}) should grow past ssthresh ({ssthresh})"
    );
}
