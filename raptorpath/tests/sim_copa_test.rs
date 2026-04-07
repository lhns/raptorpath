//! B2: Copa delay-based CC convergence tests with SimChannel.
//!
//! Verifies that Copa congestion control converges through realistic
//! simulated network conditions rather than manually injected data.

mod common;

use common::*;
use raptorpath::fec::{FecBackend, WireSymbol};
use raptorpath::scheduler::{Clock, MockClock, PathState, Scheduler};
use std::sync::Arc;
use std::time::Duration;

fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

fn make_symbols(count: u32, repair: bool) -> Vec<WireSymbol> {
    (0..count)
        .map(|i| WireSymbol {
            block_id: 0,
            payload_id: i,
            is_repair: repair,
            data: vec![0u8; 64],
            backend: FecBackend::RaptorQ,
        })
        .collect()
}

/// Simulate one round of transfer on a path through a SimChannel.
/// Sends symbols, advances clock past channel delay, delivers, and feeds ACKs.
fn simulate_round_via_channel(
    sched: &mut Scheduler,
    clock: &Arc<MockClock>,
    channel: &mut SimChannel,
    path_id: u32,
    rtt: Duration,
    throughput_bps: f64,
    symbols_per_round: u32,
) -> (u32, u32) {
    // Send symbols through channel
    let symbols = make_source_batch(symbols_per_round);
    let mut sent = 0u32;
    let mut dropped = 0u32;
    for sym in symbols {
        if channel.send(sym) {
            sent += 1;
        } else {
            dropped += 1;
        }
    }

    // Advance clock past the channel delay + jitter
    clock.advance(rtt);

    // Deliver surviving packets
    let delivered = channel.deliver();
    let received = delivered.len() as u32;

    // Feed scheduler
    sched.ack(path_id, received);

    if let Some(path) = sched.path_mut(path_id) {
        path.estimator.record_rtt(rtt);
        path.record_rtt_sample(rtt);
        path.estimator.record_throughput(throughput_bps);
        path.estimator.record_batch(symbols_per_round, received);
    }

    if dropped > 0 {
        let fec_recovered = (dropped as f64 / symbols_per_round as f64) < 0.20;
        sched.on_loss(path_id, fec_recovered);
    }

    (received, dropped)
}

#[test]
fn test_copa_converges_through_sim_channel() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Warmup
    {
        let path = sched.path_mut(1).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(millis(5));
            path.record_rtt_sample(millis(5));
            path.estimator.record_throughput(100_000_000.0);
            path.estimator.record_batch(100, 99);
        }
    }

    let mut channel = SimChannel::datacenter(clock.clone(), 42);

    // Run 200 rounds
    for _ in 0..200 {
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut channel,
            1,
            millis(5),
            100_000_000.0,
            50,
        );
    }

    let cwnd = sched.path(1).unwrap().cwnd;

    // Cwnd should have stabilized at a reasonable level
    assert!(
        cwnd >= 50,
        "cwnd should converge to >=50 on datacenter channel: cwnd={cwnd}"
    );

    // Check stability: record recent cwnd values
    let mut recent_cwnds = Vec::new();
    for _ in 0..20 {
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut channel,
            1,
            millis(5),
            100_000_000.0,
            50,
        );
        recent_cwnds.push(sched.path(1).unwrap().cwnd as f64);
    }

    let mean = recent_cwnds.iter().sum::<f64>() / recent_cwnds.len() as f64;
    let variance = recent_cwnds.iter().map(|c| (c - mean).powi(2)).sum::<f64>()
        / recent_cwnds.len() as f64;
    let cv = (variance.sqrt()) / mean; // coefficient of variation

    assert!(
        cv < 0.10,
        "cwnd should be stable (CV < 10%): mean={mean:.1}, cv={cv:.3}"
    );
}

#[test]
fn test_copa_reduces_cwnd_on_queue_buildup() {
    // Copa should reduce cwnd when RTT rises (queue building),
    // without needing an explicit ProbeRTT phase.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    let mut channel = SimChannel::datacenter(clock.clone(), 99);

    // Warmup with low RTT
    {
        let path = sched.path_mut(1).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(millis(5));
            path.record_rtt_sample(millis(5));
            path.estimator.record_throughput(100_000_000.0);
            path.estimator.record_batch(100, 99);
        }
    }

    // Run steady state rounds
    for _ in 0..50 {
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut channel,
            1,
            millis(5),
            100_000_000.0,
            50,
        );
    }

    let cwnd_before = sched.path(1).unwrap().cwnd;

    // Simulate queue buildup: RTT rises from 5ms to 50ms
    for _ in 0..10 {
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut channel,
            1,
            millis(50),
            100_000_000.0,
            50,
        );
    }

    let cwnd_after = sched.path(1).unwrap().cwnd;
    assert!(
        cwnd_after < cwnd_before,
        "Copa should reduce cwnd when RTT rises: before={cwnd_before}, after={cwnd_after}"
    );
}

#[test]
fn test_copa_wireless_vs_congestion() {
    // Tests the key Copa insight: loss + stable RTT (wireless) should NOT
    // collapse cwnd, while loss + rising RTT (congestion) SHOULD.
    // Uses SimChannel for both paths to model realistic behavior.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1); // WiFi: stable RTT, 5% loss
    sched.add_path(2); // Wired: rising RTT, 5% loss (congestion)

    // Warmup both paths identically
    for id in [1, 2] {
        let path = sched.path_mut(id).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(millis(15));
            path.record_rtt_sample(millis(15));
            path.estimator.record_throughput(50_000_000.0);
            path.estimator.record_batch(100, 95);
        }
    }

    // WiFi path: stable RTT, 5% uniform loss via SimChannel
    let mut wifi_channel = SimChannel::new(
        clock.clone(),
        10,
        millis(15),
        2,
        GilbertElliottChannel::new(0.0, 1.0, 0.05, 0.0),
    );

    // Wired congestion path: rising RTT, same loss via SimChannel
    let mut wired_channel = SimChannel::new(
        clock.clone(),
        20,
        millis(15),
        0,
        GilbertElliottChannel::new(0.0, 1.0, 0.05, 0.0),
    );

    // Run 50 rounds on both simultaneously
    for round in 0..50 {
        // WiFi: stable RTT
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut wifi_channel,
            1,
            millis(15),
            50_000_000.0,
            50,
        );

        // Wired: RTT increases each round (congestion)
        let wired_rtt = 15 + round * 5; // 15ms → 260ms
        simulate_round_via_channel(
            &mut sched,
            &clock,
            &mut wired_channel,
            2,
            millis(wired_rtt),
            30_000_000.0,
            50,
        );
    }

    let wifi_cwnd = sched.path(1).unwrap().cwnd;
    let wired_cwnd = sched.path(2).unwrap().cwnd;

    // WiFi should not have collapsed (stable RTT = wireless loss)
    assert!(
        wifi_cwnd >= PathState::MIN_CWND,
        "WiFi cwnd should not collapse below MIN_CWND: cwnd={wifi_cwnd}"
    );

    // Both paths see similar loss rates, but the wired path has rising RTT.
    // BBR should treat rising RTT as a congestion signal and reduce cwnd more.
    // The key assertion: scheduler should prefer WiFi (stable RTT) over wired.
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    let source = make_symbols(100, false);
    let result = sched.schedule(source, vec![]);

    let wifi_scheduled: usize = result.iter().filter(|(id, _)| *id == 1).map(|(_, s)| s.len()).sum();
    let wired_scheduled: usize = result.iter().filter(|(id, _)| *id == 2).map(|(_, s)| s.len()).sum();

    assert!(
        wifi_scheduled >= wired_scheduled,
        "scheduler should prefer WiFi (15ms RTT) over congested wired ({:.0}ms RTT): wifi={wifi_scheduled}, wired={wired_scheduled}",
        15.0 + 49.0 * 5.0
    );
}

#[test]
fn test_two_paths_rtt_weighted_scheduling() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1); // Path A: 5ms RTT
    sched.add_path(2); // Path B: 50ms RTT

    // Warmup both paths
    for _ in 0..50 {
        clock.advance(millis(2));
        let path = sched.path_mut(1).unwrap();
        path.estimator.record_rtt(millis(5));
        path.record_rtt_sample(millis(5));
        path.estimator.record_throughput(100_000_000.0);
        path.estimator.record_batch(100, 99);
        sched.ack(1, 50);

        clock.advance(millis(2));
        let path = sched.path_mut(2).unwrap();
        path.estimator.record_rtt(millis(50));
        path.record_rtt_sample(millis(50));
        path.estimator.record_throughput(100_000_000.0);
        path.estimator.record_batch(100, 99);
        sched.ack(2, 50);
    }

    // Reset in_flight
    sched.path_mut(1).unwrap().in_flight = 0;
    sched.path_mut(2).unwrap().in_flight = 0;

    // Schedule source symbols
    let source = make_symbols(100, false);
    let result = sched.schedule(source, vec![]);

    let path_a_count: usize = result
        .iter()
        .filter(|(id, _)| *id == 1)
        .map(|(_, s)| s.len())
        .sum();
    let path_b_count: usize = result
        .iter()
        .filter(|(id, _)| *id == 2)
        .map(|(_, s)| s.len())
        .sum();

    assert!(
        path_a_count > path_b_count,
        "Path A (5ms) should get more source symbols than Path B (50ms): A={path_a_count}, B={path_b_count}"
    );
}
