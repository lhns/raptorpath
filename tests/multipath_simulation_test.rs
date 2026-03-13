//! Multipath simulation tests.
//!
//! These tests simulate realistic multipath scenarios with different path
//! characteristics and dynamic conditions.  Each test runs a simulated
//! transfer loop that feeds RTT/loss/throughput into the scheduler and
//! verifies that the CC and scheduler adapt correctly.

use raptorpath::fec::WireSymbol;
use raptorpath::scheduler::{PathState, Scheduler};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_symbols(count: u32, repair: bool) -> Vec<WireSymbol> {
    (0..count)
        .map(|i| WireSymbol {
            block_id: 0,
            payload_id: i,
            is_repair: repair,
            data: vec![0u8; 64],
        })
        .collect()
}

/// Simulated path profile.
struct PathProfile {
    id: u32,
    rtt_ms: u64,
    loss_pct: f64, // 0.0 - 1.0
    throughput_bps: f64,
}

/// Set up a scheduler with the given path profiles, running enough
/// warmup rounds for the estimators to converge.
fn setup_paths(profiles: &[PathProfile]) -> Scheduler {
    let mut sched = Scheduler::new();
    for p in profiles {
        sched.add_path(p.id);
        let path = sched.path_mut(p.id).unwrap();
        let rtt = Duration::from_millis(p.rtt_ms);
        // Feed enough samples for EWMA convergence
        for _ in 0..50 {
            path.estimator.record_rtt(rtt);
            path.record_rtt_sample(rtt);
            path.estimator.record_throughput(p.throughput_bps);
            let sent = 100u32;
            let received = ((1.0 - p.loss_pct) * sent as f64) as u32;
            path.estimator.record_batch(sent, received);
        }
        path.cwnd = 200; // start with a reasonable window
        path.in_slow_start = false;
    }
    sched
}

/// Simulate one "round" of transfer on a SINGLE path.
///
/// Unlike schedule() which distributes to all paths, this only feeds
/// estimator/BBR data to the specified path and acks on it.
fn simulate_round(
    sched: &mut Scheduler,
    path_id: u32,
    rtt: Duration,
    loss_pct: f64,
    throughput_bps: f64,
    symbols_per_round: u32,
) {
    // Brief sleep so delivery rate is computable
    std::thread::sleep(Duration::from_millis(2));

    // Simulate ACK with loss
    let received = ((1.0 - loss_pct) * symbols_per_round as f64) as u32;
    let lost = symbols_per_round - received;

    sched.ack(path_id, received);

    if let Some(path) = sched.path_mut(path_id) {
        path.estimator.record_rtt(rtt);
        path.record_rtt_sample(rtt);
        path.estimator.record_throughput(throughput_bps);
        path.estimator.record_batch(symbols_per_round, received);
    }

    // Handle loss events
    if lost > 0 {
        // FEC usually recovers if loss is below ~20%
        let fec_recovered = loss_pct < 0.20;
        sched.on_loss(path_id, fec_recovered);
    }
}

/// Count how many symbols a path gets from schedule().
fn count_for_path(assignments: &[(u32, Vec<WireSymbol>)], path_id: u32) -> usize {
    assignments
        .iter()
        .filter(|(id, _)| *id == path_id)
        .map(|(_, s)| s.len())
        .sum()
}

// ===========================================================================
// 1. Asymmetric paths: WiFi (fast, lossy) + Cellular (slow, reliable)
// ===========================================================================
#[test]
fn test_asymmetric_wifi_cellular() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.05,
            throughput_bps: 50_000_000.0, // 50 Mbps WiFi
        },
        PathProfile {
            id: 2,
            rtt_ms: 60,
            loss_pct: 0.01,
            throughput_bps: 20_000_000.0, // 20 Mbps cellular
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Run 20 rounds per path
    for _ in 0..20 {
        simulate_round(&mut sched, 1, Duration::from_millis(10), 0.05, 50_000_000.0, 50);
        simulate_round(&mut sched, 2, Duration::from_millis(60), 0.01, 20_000_000.0, 50);
    }

    // Reset in_flight for clean scheduling
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    // WiFi path (id=1): lower RTT → should get source symbols preferentially
    let source = make_symbols(100, false);
    let repair = make_symbols(50, true);
    let result = sched.schedule(source, repair);

    let wifi_count = count_for_path(&result, 1);
    let cell_count = count_for_path(&result, 2);

    assert!(
        wifi_count > 0 && cell_count > 0,
        "Both paths should receive symbols: wifi={wifi_count}, cell={cell_count}"
    );

    // WiFi (lower RTT) should get more source symbols
    assert!(
        wifi_count > cell_count,
        "WiFi (lower RTT, higher throughput) should get more symbols: wifi={wifi_count}, cell={cell_count}"
    );

    // WiFi cwnd should NOT have collapsed despite 5% loss (BBR ignores wireless loss)
    let wifi_cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        wifi_cwnd >= 20,
        "WiFi cwnd should not collapse from random loss (stable RTT): cwnd={wifi_cwnd}"
    );
}

// ===========================================================================
// 2. WiFi + Cellular + Ethernet: three-path heterogeneous
// ===========================================================================
#[test]
fn test_three_path_heterogeneous() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 5,
            loss_pct: 0.03,
            throughput_bps: 100_000_000.0, // Ethernet
        },
        PathProfile {
            id: 2,
            rtt_ms: 15,
            loss_pct: 0.05,
            throughput_bps: 50_000_000.0, // WiFi
        },
        PathProfile {
            id: 3,
            rtt_ms: 80,
            loss_pct: 0.01,
            throughput_bps: 10_000_000.0, // Cellular
        },
    ];
    let mut sched = setup_paths(&profiles);

    for _ in 0..20 {
        simulate_round(&mut sched, 1, Duration::from_millis(5), 0.03, 100_000_000.0, 80);
        simulate_round(&mut sched, 2, Duration::from_millis(15), 0.05, 50_000_000.0, 40);
        simulate_round(&mut sched, 3, Duration::from_millis(80), 0.01, 10_000_000.0, 10);
    }

    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;
    sched.path_mut(3).unwrap().in_flight = 0;

    // Schedule a block
    let source = make_symbols(100, false);
    let repair = make_symbols(50, true);
    let result = sched.schedule(source, repair);

    let eth = count_for_path(&result, 1);
    let wifi = count_for_path(&result, 2);
    let cell = count_for_path(&result, 3);

    // Ethernet (lowest RTT, highest throughput) should get the most
    assert!(
        eth > 0 && wifi > 0,
        "Ethernet and WiFi should both receive symbols: eth={eth}, wifi={wifi}, cell={cell}"
    );

    // All symbols should be distributed
    let total = eth + wifi + cell;
    assert_eq!(total, 150, "All symbols should be distributed");
}

// ===========================================================================
// 3. Path degradation mid-transfer: WiFi gets congested
// ===========================================================================
#[test]
fn test_path_degradation_mid_transfer() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.02,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 50,
            loss_pct: 0.01,
            throughput_bps: 30_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Phase 1: Both paths healthy, 20 rounds
    for _ in 0..20 {
        simulate_round(&mut sched, 1, Duration::from_millis(10), 0.02, 50_000_000.0, 50);
        simulate_round(&mut sched, 2, Duration::from_millis(50), 0.01, 30_000_000.0, 30);
    }

    // Phase 2: Path 1 gets congested — RTT rises, loss increases
    let congestion_rtts = [30, 60, 100, 150, 200, 250];
    for &rtt in &congestion_rtts {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(rtt),
            0.15,
            20_000_000.0,
            50,
        );
    }

    let cwnd2_stable = sched.path(2).unwrap().cwnd;

    // Path 2 should be unaffected
    assert!(
        cwnd2_stable >= 20,
        "Stable path cwnd should not be affected: cwnd={cwnd2_stable}"
    );

    // Scheduler should now favor path 2 for source symbols:
    // path 1 RTT is 250ms, path 2 RTT is 50ms
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let p1 = count_for_path(&result, 1);
    let p2 = count_for_path(&result, 2);
    assert!(
        p2 > p1,
        "Stable path (lower RTT) should get more source symbols: p1={p1}, p2={p2}"
    );
}

// ===========================================================================
// 4. Path recovery after going dead
// ===========================================================================
#[test]
fn test_path_death_and_recovery() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.02,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 40,
            loss_pct: 0.01,
            throughput_bps: 30_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Warmup: both paths running
    for _ in 0..10 {
        simulate_round(&mut sched, 1, Duration::from_millis(10), 0.02, 50_000_000.0, 50);
        simulate_round(&mut sched, 2, Duration::from_millis(40), 0.01, 30_000_000.0, 30);
    }

    // Path 1 dies
    sched.path_mut(1).unwrap().active = false;

    // Only path 2 should receive symbols
    sched.path_mut(2).unwrap().in_flight = 0;
    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let p1_dead = count_for_path(&result, 1);
    let p2_solo = count_for_path(&result, 2);

    assert_eq!(p1_dead, 0, "Dead path should receive no symbols");
    assert!(p2_solo > 0, "Surviving path should get all symbols");

    // Path 1 recovers
    sched.touch_path(1);

    assert!(
        sched.path(1).unwrap().active,
        "Path should be active after touch"
    );
    assert!(
        sched.path(1).unwrap().in_slow_start,
        "Recovered path should restart in startup"
    );
    assert_eq!(
        sched.path(1).unwrap().cwnd,
        PathState::INITIAL_CWND,
        "Recovered path should have initial cwnd"
    );

    // After a few rounds of recovery, path 1 should participate again
    for _ in 0..10 {
        simulate_round(&mut sched, 1, Duration::from_millis(10), 0.02, 50_000_000.0, 30);
    }

    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let p1_recovered = count_for_path(&result, 1);
    assert!(
        p1_recovered > 0,
        "Recovered path should receive symbols again: p1={p1_recovered}"
    );
}

// ===========================================================================
// 5. Mixed wireless/congestion: one path random loss, other congested
// ===========================================================================
#[test]
fn test_mixed_wireless_and_congestion() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 15,
            loss_pct: 0.08,
            throughput_bps: 40_000_000.0, // WiFi: lossy but fast
        },
        PathProfile {
            id: 2,
            rtt_ms: 50,
            loss_pct: 0.02,
            throughput_bps: 30_000_000.0, // Wired: low loss
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Path 1: wireless loss (stable RTT, high loss)
    for _ in 0..20 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(15),
            0.08,
            40_000_000.0,
            50,
        );
    }

    // Path 2: real congestion (rising RTT)
    let rtts = [50, 70, 100, 140, 190, 250, 320, 400];
    for &rtt in &rtts {
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(rtt),
            0.10,
            15_000_000.0,
            30,
        );
    }

    // WiFi: stable RTT + loss → BBR should NOT have collapsed cwnd
    let wifi_cwnd = sched.path(1).unwrap().cwnd;
    assert!(
        wifi_cwnd >= 20,
        "WiFi (wireless loss, stable RTT) cwnd should not collapse: cwnd={wifi_cwnd}"
    );

    // Key behavioral test: scheduler should favor the wireless path
    // over the congested one, because wireless path has stable RTT
    // and higher throughput despite loss
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let wifi = count_for_path(&result, 1);
    let wired = count_for_path(&result, 2);

    // WiFi (15ms RTT) should get source symbols over wired (now 400ms RTT)
    assert!(
        wifi > wired,
        "WiFi (stable, low RTT) should get more source symbols than congested wired: wifi={wifi}, wired={wired}"
    );
}

// ===========================================================================
// 6. Hot-add path mid-transfer
// ===========================================================================
#[test]
fn test_hot_add_path_mid_transfer() {
    let profiles = vec![PathProfile {
        id: 1,
        rtt_ms: 20,
        loss_pct: 0.03,
        throughput_bps: 40_000_000.0,
    }];
    let mut sched = setup_paths(&profiles);

    // Transfer on single path for a while
    for _ in 0..20 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(20),
            0.03,
            40_000_000.0,
            50,
        );
    }

    let cwnd1_before = sched.path(1).unwrap().cwnd;

    // Hot-add path 2
    sched.add_path(2);
    let path2 = sched.path(2).unwrap();
    assert_eq!(
        path2.cwnd,
        PathState::INITIAL_CWND,
        "New path starts with initial cwnd"
    );
    assert!(path2.in_slow_start, "New path starts in startup");

    // Warmup path 2
    for _ in 0..20 {
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(10),
            0.01,
            60_000_000.0,
            50,
        );
    }

    // Both paths should be active and have capacity
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    // Use source + repair to ensure both paths get some symbols
    let source = make_symbols(40, false);
    let repair = make_symbols(40, true);
    let result = sched.schedule(source, repair);

    let p1 = count_for_path(&result, 1);
    let p2 = count_for_path(&result, 2);

    // Path 2 (lower RTT) gets source symbols; path 1 (higher goodput)
    // gets repair symbols. Both should participate.
    assert!(
        p1 > 0 && p2 > 0,
        "Both paths should participate: p1={p1}, p2={p2}"
    );

    // Path 1 cwnd should not have been affected by adding path 2
    let cwnd1_after = sched.path(1).unwrap().cwnd;
    assert!(
        cwnd1_after >= cwnd1_before / 2,
        "Existing path cwnd should not collapse from adding new path: before={cwnd1_before}, after={cwnd1_after}"
    );
}

// ===========================================================================
// 7. Hot-remove path mid-transfer
// ===========================================================================
#[test]
fn test_hot_remove_path_mid_transfer() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 15,
            loss_pct: 0.02,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 40,
            loss_pct: 0.01,
            throughput_bps: 30_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Both paths running
    for _ in 0..10 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(15),
            0.02,
            50_000_000.0,
            50,
        );
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(40),
            0.01,
            30_000_000.0,
            30,
        );
    }

    // Remove path 2
    sched.remove_path(2);

    assert!(sched.path(2).is_none(), "Removed path should be gone");

    // All symbols should go to path 1
    sched.path_mut(1).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let repair = make_symbols(20, true);
    let result = sched.schedule(source, repair);

    let p1 = count_for_path(&result, 1);
    assert_eq!(p1, 70, "All symbols should go to remaining path");
}

// ===========================================================================
// 8. Flapping path: dies and recovers repeatedly
// ===========================================================================
#[test]
fn test_flapping_path() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.02,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 40,
            loss_pct: 0.01,
            throughput_bps: 30_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Flap path 1 three times
    for cycle in 0..3 {
        // Path 1 alive
        for _ in 0..5 {
            simulate_round(
                &mut sched,
                1,
                Duration::from_millis(10),
                0.02,
                50_000_000.0,
                50,
            );
            simulate_round(
                &mut sched,
                2,
                Duration::from_millis(40),
                0.01,
                30_000_000.0,
                30,
            );
        }

        // Path 1 dies
        sched.path_mut(1).unwrap().active = false;

        // Only path 2 running
        for _ in 0..3 {
            simulate_round(
                &mut sched,
                2,
                Duration::from_millis(40),
                0.01,
                30_000_000.0,
                30,
            );
        }

        // Path 1 recovers
        sched.touch_path(1);
        assert!(
            sched.path(1).unwrap().in_slow_start,
            "Cycle {cycle}: recovered path should be in startup"
        );
    }

    // After 3 flaps, path 2 should still be healthy
    let cwnd2 = sched.path(2).unwrap().cwnd;
    assert!(
        cwnd2 >= 20,
        "Stable path should maintain healthy cwnd during flaps: cwnd={cwnd2}"
    );

    // Path 1 should be active and usable
    assert!(
        sched.path(1).unwrap().active,
        "Path 1 should be active after recovery"
    );
}

// ===========================================================================
// 9. Gradual degradation: path slowly gets worse
// ===========================================================================
#[test]
fn test_gradual_path_degradation() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.01,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 30,
            loss_pct: 0.01,
            throughput_bps: 30_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Phase 1: both healthy
    for _ in 0..10 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(10),
            0.01,
            50_000_000.0,
            50,
        );
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(30),
            0.01,
            30_000_000.0,
            30,
        );
    }

    // Path 1 gradually degrades: RTT creeps up, throughput drops
    let degradation_steps: Vec<(u64, f64, f64)> = vec![
        (15, 0.02, 45_000_000.0),
        (25, 0.04, 35_000_000.0),
        (40, 0.06, 25_000_000.0),
        (60, 0.08, 15_000_000.0),
        (90, 0.12, 10_000_000.0),
        (130, 0.15, 5_000_000.0),
    ];

    for (rtt_ms, loss, throughput) in &degradation_steps {
        for _ in 0..3 {
            simulate_round(
                &mut sched,
                1,
                Duration::from_millis(*rtt_ms),
                *loss,
                *throughput,
                50,
            );
            simulate_round(
                &mut sched,
                2,
                Duration::from_millis(30),
                0.01,
                30_000_000.0,
                30,
            );
        }
    }

    // After degradation, path 2 (stable) should be preferred for scheduling
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let p1 = count_for_path(&result, 1);
    let p2 = count_for_path(&result, 2);

    // Path 2 (30ms RTT, stable) should now get more source symbols than
    // path 1 (130ms+ RTT, degraded)
    assert!(
        p2 >= p1,
        "Stable path should be preferred over degraded path: p1={p1}, p2={p2}"
    );
}

// ===========================================================================
// 10. Sustained wireless loss: 10% loss for 100 rounds, cwnd stable
// ===========================================================================
#[test]
fn test_sustained_wireless_loss_cwnd_stable() {
    let mut sched = Scheduler::new();
    sched.add_path(1);

    // Warmup
    let path = sched.path_mut(1).unwrap();
    path.cwnd = 200;
    path.in_slow_start = false;
    for _ in 0..20 {
        path.estimator.record_rtt(Duration::from_millis(15));
        path.record_rtt_sample(Duration::from_millis(15));
        path.estimator.record_throughput(50_000_000.0);
        path.estimator.record_batch(100, 95);
    }

    let cwnd_before = sched.path(1).unwrap().cwnd;

    // 100 rounds of 10% wireless loss with perfectly stable RTT
    for _ in 0..100 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(15),
            0.10,
            50_000_000.0,
            100,
        );
    }

    let cwnd_after = sched.path(1).unwrap().cwnd;

    // With BBR delay-based CC, stable RTT = no congestion = cwnd should
    // NOT have collapsed. Some drift is OK but should stay above 50% of start.
    assert!(
        cwnd_after >= cwnd_before / 2,
        "Sustained wireless loss (stable RTT) should not collapse cwnd: before={cwnd_before}, after={cwnd_after}"
    );
}

// ===========================================================================
// 11. Congestion then recovery: cwnd rebounds
// ===========================================================================
#[test]
fn test_congestion_then_full_recovery() {
    let mut sched = Scheduler::new();
    sched.add_path(1);
    sched.add_path(2);

    // Warmup both paths with delivery rate building
    for _ in 0..20 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(20),
            0.02,
            40_000_000.0,
            50,
        );
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(20),
            0.02,
            40_000_000.0,
            50,
        );
    }

    let cwnd1_before = sched.path(1).unwrap().cwnd;

    // Path 1 gets congested — rising RTT
    let congestion_rtts = [30, 50, 80, 120, 170, 230];
    for &rtt in &congestion_rtts {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(rtt),
            0.15,
            15_000_000.0,
            50,
        );
    }

    let cwnd1_congested = sched.path(1).unwrap().cwnd;
    let cwnd2_unaffected = sched.path(2).unwrap().cwnd;

    // Path 2 should be unaffected
    assert!(
        cwnd2_unaffected >= 20,
        "Other path should be unaffected by path 1's congestion: cwnd={cwnd2_unaffected}"
    );

    // Scheduler should favor path 2 during congestion
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(50, false);
    let result = sched.schedule(source, vec![]);

    let p1_during = count_for_path(&result, 1);
    let p2_during = count_for_path(&result, 2);

    // Path 2 (20ms RTT) should get more source symbols than path 1 (230ms RTT)
    assert!(
        p2_during > p1_during,
        "Stable path should get more symbols during congestion: p1={p1_during}, p2={p2_during}"
    );

    // Congestion clears: RTT returns to baseline
    for _ in 0..50 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(20),
            0.02,
            40_000_000.0,
            50,
        );
    }

    let cwnd1_recovered = sched.path(1).unwrap().cwnd;
    assert!(
        cwnd1_recovered > PathState::MIN_CWND,
        "Cwnd should recover after congestion clears: cwnd={cwnd1_recovered}"
    );
}

// ===========================================================================
// 12. Scheduler rebalances: symbols shift to better path
// ===========================================================================
#[test]
fn test_scheduler_rebalances_on_degradation() {
    let profiles = vec![
        PathProfile {
            id: 1,
            rtt_ms: 10,
            loss_pct: 0.01,
            throughput_bps: 50_000_000.0,
        },
        PathProfile {
            id: 2,
            rtt_ms: 10,
            loss_pct: 0.01,
            throughput_bps: 50_000_000.0,
        },
    ];
    let mut sched = setup_paths(&profiles);

    // Phase 1: equal paths — both should get symbols
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(100, false);
    let repair = make_symbols(100, true);
    let result = sched.schedule(source, repair);

    let p1_before = count_for_path(&result, 1);
    let p2_before = count_for_path(&result, 2);

    assert!(
        p1_before > 0 && p2_before > 0,
        "Both equal paths should receive symbols: p1={p1_before}, p2={p2_before}"
    );

    // Phase 2: degrade path 1 — rising RTT + higher loss + lower throughput
    for _ in 0..20 {
        simulate_round(
            &mut sched,
            1,
            Duration::from_millis(100),
            0.20,
            10_000_000.0,
            50,
        );
        simulate_round(
            &mut sched,
            2,
            Duration::from_millis(10),
            0.01,
            50_000_000.0,
            50,
        );
    }

    // Schedule again
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let repair = make_symbols(100, true);
    let result = sched.schedule(vec![], repair);

    let p1_after = count_for_path(&result, 1);
    let p2_after = count_for_path(&result, 2);

    // Path 2 should now get significantly more repair symbols (higher goodput)
    assert!(
        p2_after > p1_after,
        "Scheduler should shift symbols to better path: p1={p1_after}, p2={p2_after}"
    );
}
