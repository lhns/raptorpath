//! ADR-0009 + ADR-0019: BBR-style delay-based congestion control tests.

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

#[test]
fn test_initial_cwnd() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);
    assert_eq!(sched.path(0).unwrap().cwnd, 10);
}

#[test]
fn test_startup_grows_cwnd() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let source: Vec<_> = (0..5).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    // P7 Copa-lite: cwnd updates run once per SRTT, driven by RTT samples.
    sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
    clock.advance(millis(60));
    sched.ack(0, 5);
    let cwnd_after = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_after > cwnd_before,
        "cwnd should grow during startup: {cwnd_before} -> {cwnd_after}"
    );
}

#[test]
fn test_startup_ramp_multiplicative_per_rtt() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // In the startup ramp, cwnd should grow multiplicatively per RTT
    let initial = sched.path(0).unwrap().cwnd;
    assert_eq!(initial, PathState::INITIAL_CWND);

    sched.path_mut(0).unwrap().record_rtt_sample(millis(20));
    clock.advance(millis(60));
    sched.ack(0, initial);
    let after_first = sched.path(0).unwrap().cwnd;

    // P7 Copa-lite ramp: cwnd = cwnd × 1.5 + 1 per window update
    assert!(
        after_first >= initial * 3 / 2,
        "Ramp should grow cwnd at least 1.5x per RTT: {initial} -> {after_first}"
    );
}

#[test]
fn test_loss_with_stable_rtt_no_reduction() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Feed stable RTT samples (no congestion signal)
    let path = sched.path_mut(0).unwrap();
    for _ in 0..5 {
        path.record_rtt_sample(millis(50));
    }

    // Grow cwnd
    clock.advance(millis(2));
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
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Grow cwnd first
    clock.advance(millis(2));
    sched.ack(0, 50);
    let cwnd_before = sched.path(0).unwrap().cwnd;
    assert!(cwnd_before > PathState::MIN_CWND);

    // Feed rising RTT samples → triggers congestion detection
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(70));
    path.record_rtt_sample(millis(100));
    path.record_rtt_sample(millis(140));

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
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Grow cwnd
    clock.advance(millis(2));
    sched.ack(0, 100);
    let cwnd_before = sched.path(0).unwrap().cwnd;

    // Rising RTT → congestion
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));

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
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);

    // Force congestion signal
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));

    for _ in 0..20 {
        sched.on_loss(0, false);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd >= PathState::MIN_CWND, "cwnd should never go below MIN_CWND, got {cwnd}");
}

#[test]
fn test_cwnd_capped_at_max() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Aggressive growth
    for i in 0..100 {
        let cwnd = sched.path(0).unwrap().cwnd;
        let batch = std::cmp::min(cwnd, 1000);
        let source: Vec<_> = (0..batch).map(|j| make_symbol(i * 1000 + j, false)).collect();
        sched.schedule(source, vec![]);
        clock.advance(millis(1));
        sched.ack(0, batch);
    }

    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(cwnd <= PathState::MAX_CWND, "cwnd should be capped at MAX_CWND, got {cwnd}");
}

#[test]
fn test_loss_exits_startup() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);

    assert!(sched.path(0).unwrap().in_slow_start, "should start in startup");

    // Rising RTT + loss exits startup
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));

    sched.on_loss(0, false);
    assert!(!sched.path(0).unwrap().in_slow_start, "congestion + loss should exit startup");
}

#[test]
fn test_recovery_after_loss() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Grow cwnd
    clock.advance(millis(2));
    sched.ack(0, 50);
    let peak = sched.path(0).unwrap().cwnd;

    // Congestion loss
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));
    sched.on_loss(0, false);

    let trough = sched.path(0).unwrap().cwnd;
    assert!(trough < peak, "loss should reduce cwnd");

    // Feed stable RTT to clear congestion signal, then ack to recover
    let path = sched.path_mut(0).unwrap();
    for _ in 0..5 {
        path.record_rtt_sample(millis(50));
    }

    for _ in 0..100 {
        clock.advance(millis(1));
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
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    sched.add_path(1);

    // Grow both paths
    let source: Vec<_> = (0..10).map(|i| make_symbol(i, false)).collect();
    sched.schedule(source, vec![]);
    clock.advance(millis(2));
    sched.ack(0, 5);
    sched.ack(1, 5);

    let cwnd0_before = sched.path(0).unwrap().cwnd;
    let cwnd1_before = sched.path(1).unwrap().cwnd;

    // Only path 0 gets congestion signal
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));
    path.record_rtt_sample(millis(80));
    path.record_rtt_sample(millis(120));
    path.record_rtt_sample(millis(170));
    sched.on_loss(0, false);

    let cwnd0_after = sched.path(0).unwrap().cwnd;
    let cwnd1_after = sched.path(1).unwrap().cwnd;

    assert!(cwnd0_after < cwnd0_before, "path 0 cwnd should decrease");
    assert_eq!(cwnd1_after, cwnd1_before, "path 1 cwnd should be unchanged");
}

// ---------------------------------------------------------------------------
// Copa delay-based congestion control tests (replaces ProbeRTT / ADR-0024)
// ---------------------------------------------------------------------------

#[test]
fn test_copa_cwnd_tracks_queuing_delay() {
    // Copa-lite: when the windowed-min RTT rises above the propagation
    // floor times the queue target, cwnd backs off.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    // Baseline: RTT at the floor → ramp grows cwnd (one update per SRTT)
    for _ in 0..10 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(50));
        clock.advance(millis(60));
        sched.ack(0, 20);
    }
    let cwnd_low_rtt = sched.path(0).unwrap().cwnd;

    // Now RTT rises significantly (queue building) → per-update backoffs
    for _ in 0..5 {
        let path = sched.path_mut(0).unwrap();
        path.record_rtt_sample(millis(200));
        clock.advance(millis(250));
        sched.ack(0, 5);
    }
    let cwnd_high_rtt = sched.path(0).unwrap().cwnd;

    assert!(
        cwnd_high_rtt < cwnd_low_rtt,
        "Copa should reduce cwnd when RTT rises: low_rtt={cwnd_low_rtt}, high_rtt={cwnd_high_rtt}"
    );
}

#[test]
fn test_copa_no_probe_rtt_phase() {
    // Copa has no ProbeRTT phase — cwnd never drops to 4 after 10s idle.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));

    // Grow cwnd
    for _ in 0..10 {
        clock.advance(millis(2));
        sched.ack(0, 20);
    }
    let cwnd_before = sched.path(0).unwrap().cwnd;
    assert!(cwnd_before > 4, "cwnd should be large");

    // Advance 11 seconds (would trigger ProbeRTT under BBR)
    clock.advance(Duration::from_secs(11));
    clock.advance(millis(2));
    sched.ack(0, 1);

    let cwnd_after = sched.path(0).unwrap().cwnd;
    assert!(
        cwnd_after > 4,
        "Copa should NOT enter ProbeRTT, cwnd={cwnd_after}"
    );
}

#[test]
fn test_copa_min_rtt_tracks_baseline() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(100));
    assert_eq!(path.copa_min_rtt(), Some(millis(100)));

    // Lower RTT sample updates min_rtt
    path.record_rtt_sample(millis(50));
    assert_eq!(path.copa_min_rtt(), Some(millis(50)));

    // Higher RTT sample does NOT change min_rtt
    path.record_rtt_sample(millis(80));
    assert_eq!(path.copa_min_rtt(), Some(millis(50)));
}

#[test]
fn test_copa_min_rtt_window_expires() {
    // min_rtt should expire after 10s window
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(30));
    assert_eq!(path.copa_min_rtt(), Some(millis(30)));

    // Advance past 10s window, record higher RTT
    clock.advance(Duration::from_secs(11));
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(80));

    // Old 30ms sample should be expired, min_rtt is now 80ms
    assert_eq!(path.copa_min_rtt(), Some(millis(80)));
}

#[test]
fn test_copa_startup_exits_on_congestion() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    assert!(sched.path(0).unwrap().in_slow_start, "should start in startup");

    // Establish the propagation floor and complete one clean update.
    sched.path_mut(0).unwrap().record_rtt_sample(millis(50));
    clock.advance(millis(60));
    sched.ack(0, 10);
    assert!(sched.path(0).unwrap().in_slow_start, "clean window keeps ramping");

    // A full window of inflated samples: even the windowed MIN is above
    // floor × queue target → congestion → ramp ends.
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(95));
    path.record_rtt_sample(millis(110));
    path.record_rtt_sample(millis(120));

    // ACK to trigger the backoff check
    clock.advance(millis(60));
    sched.ack(0, 10);

    assert!(
        !sched.path(0).unwrap().in_slow_start,
        "should exit startup when the standing queue shows"
    );
}

#[test]
fn test_copa_steady_state_convergence() {
    // In steady state, Copa should converge cwnd toward the delay-based target.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(50));

    // Ramp for a few RTTs, then exit via one inflated window (first backoff)
    for _ in 0..6 {
        sched.path_mut(0).unwrap().record_rtt_sample(millis(50));
        clock.advance(millis(60));
        sched.ack(0, 20);
    }
    sched.path_mut(0).unwrap().record_rtt_sample(millis(100));
    clock.advance(millis(60));
    sched.ack(0, 20);
    assert!(!sched.path(0).unwrap().in_slow_start, "backoff ends the ramp");

    // Record near-floor RTTs and keep ACKing — cwnd stabilizes into the
    // gentle additive oscillation (+2 per RTT)
    let mut cwnds = vec![];
    for _ in 0..20 {
        let path = sched.path_mut(0).unwrap();
        path.record_rtt_sample(millis(55)); // slight queuing, below target
        clock.advance(millis(60));
        sched.ack(0, 5);
        cwnds.push(sched.path(0).unwrap().cwnd);
    }

    // Last 5 cwnds should be close to each other (converged)
    let last5 = &cwnds[cwnds.len() - 5..];
    let max = *last5.iter().max().unwrap();
    let min = *last5.iter().min().unwrap();
    let range = max - min;
    assert!(
        range <= max / 4,
        "cwnd should converge in steady state, range={range}, values={last5:?}"
    );
}
