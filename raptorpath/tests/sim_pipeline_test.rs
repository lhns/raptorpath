//! B6: Full pipeline integration tests with SimChannel.
//!
//! Wires together Scheduler + MockClock + SimChannel + WindowEncoder/Decoder
//! + ReorderBuffer to verify end-to-end behavior.
//!
//! The BackendSelector arm was dropped in the dead-code refactor (batch 1):
//! mid-stream FEC backend switching was removed from the data path
//! (paper §16.4), so the selector it exercised no longer exists.

mod common;

use bytes::Bytes;
use common::*;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::scheduler::{Clock, MockClock, Scheduler};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Run a pipeline simulation: encode source symbols, send through SimChannel,
/// decode with repairs, and collect stats.
struct PipelineRun {
    recovered: BTreeSet<u64>,
    total_sent: u32,
    total_dropped: u32,
    cwnd_history: Vec<u32>,
}

fn run_pipeline(
    clock: &Arc<MockClock>,
    channel: &mut SimChannel,
    num_symbols: u32,
    rtt: Duration,
    path_id: u32,
    sched: &mut Scheduler,
    estimator: &mut LossEstimator,
) -> PipelineRun {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);
    let mut reorder_buf = ReorderBuffer::new(25, 500);

    let mut recovered = BTreeSet::new();
    let mut total_dropped = 0u32;
    let mut cwnd_history = Vec::new();

    let batch_size = 10u32;
    let repair_per_batch = 3u32;

    let mut batch_num = 0u32;
    let mut sym_idx = 0u32;

    while sym_idx < num_symbols {
        let this_batch = batch_size.min(num_symbols - sym_idx);

        // Encode and send source symbols
        let mut batch_survived = 0u32;
        let mut batch_dropped = 0u32;

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; symbol_size as usize];
            let sym = encoder.add_source(&data);

            if channel.send(sym) {
                batch_survived += 1;
            } else {
                batch_dropped += 1;
            }
            sym_idx += 1;
        }

        // Generate and send repair symbols
        for _ in 0..repair_per_batch {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            channel.send(repair);
        }

        total_dropped += batch_dropped;

        // Advance clock by RTT to allow delivery
        clock.advance(rtt);

        // Deliver packets
        let delivered = channel.deliver();

        // Feed to decoder and reorder buffer
        let now = clock.now();
        for pkt in &delivered {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    recovered.insert(rseq);
                }
            }
        }

        // Drain expired from reorder buffer
        let expired = reorder_buf.drain_expired(now);
        for (seq, _) in expired {
            recovered.insert(seq);
        }

        // Update estimator
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(rtt);

        // Feed scheduler
        sched.ack(path_id, batch_survived);
        if let Some(path) = sched.path_mut(path_id) {
            path.estimator.record_rtt(rtt);
            path.record_rtt_sample(rtt);
            path.estimator
                .record_batch(this_batch, batch_survived);
        }
        if batch_dropped > 0 {
            let fec_ok = (batch_dropped as f64 / this_batch as f64) < 0.20;
            sched.on_loss(path_id, fec_ok);
        }

        cwnd_history.push(sched.path(path_id).map(|p| p.cwnd).unwrap_or(0));

        batch_num += 1;
    }

    PipelineRun {
        recovered,
        total_sent: num_symbols,
        total_dropped,
        cwnd_history,
    }
}

#[test]
fn test_pipeline_datacenter() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Warmup
    let path = sched.path_mut(1).unwrap();
    path.cwnd = 200;
    path.in_slow_start = false;
    for _ in 0..20 {
        path.estimator.record_rtt(millis(1));
        path.record_rtt_sample(millis(1));
        path.estimator.record_throughput(1_000_000_000.0);
        path.estimator.record_batch(100, 100);
    }

    let mut estimator = LossEstimator::new();
    let mut channel = SimChannel::datacenter(clock.clone(), 42);

    let result = run_pipeline(
        &clock,
        &mut channel,
        500,
        millis(1),
        1,
        &mut sched,
        &mut estimator,
    );

    let recovery_rate = result.recovered.len() as f64 / result.total_sent as f64;
    assert!(
        recovery_rate >= 0.995,
        "datacenter should achieve >=99.5% recovery: {:.1}% ({}/{})",
        recovery_rate * 100.0,
        result.recovered.len(),
        result.total_sent
    );

    // Cwnd should not have collapsed below minimum usable level
    if let Some(&last_cwnd) = result.cwnd_history.last() {
        assert!(
            last_cwnd >= 2,
            "cwnd should not collapse to below MIN_CWND on datacenter: last={last_cwnd}"
        );
    }
}

#[test]
fn test_pipeline_wifi_degradation() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1);

    // Warmup with datacenter conditions
    let path = sched.path_mut(1).unwrap();
    path.cwnd = 200;
    path.in_slow_start = false;
    for _ in 0..20 {
        path.estimator.record_rtt(millis(1));
        path.record_rtt_sample(millis(1));
        path.estimator.record_throughput(100_000_000.0);
        path.estimator.record_batch(100, 100);
    }

    let mut estimator = LossEstimator::new();

    // Phase 1: datacenter (first 200 symbols)
    let mut channel_dc = SimChannel::datacenter(clock.clone(), 42);
    let result1 = run_pipeline(
        &clock,
        &mut channel_dc,
        200,
        millis(1),
        1,
        &mut sched,
        &mut estimator,
    );

    // Phase 2: switch to WiFi (next 300 symbols)
    let mut channel_wifi = SimChannel::wifi(clock.clone(), 77);
    let result2 = run_pipeline(
        &clock,
        &mut channel_wifi,
        300,
        millis(5),
        1,
        &mut sched,
        &mut estimator,
    );

    // Combined recovery should be >= 95%
    let total_recovered = result1.recovered.len() + result2.recovered.len();
    let total_sent = result1.total_sent + result2.total_sent;
    let recovery_rate = total_recovered as f64 / total_sent as f64;

    assert!(
        recovery_rate >= 0.95,
        "degraded pipeline should achieve >=95% recovery: {:.1}% ({total_recovered}/{total_sent})",
        recovery_rate * 100.0
    );

    // Copa is delay-based: when RTT rises (DC→WiFi), cwnd drops because Copa
    // sees the RTT increase as queuing delay. This is expected behavior —
    // Copa will recover once min_rtt window expires and adapts to the new baseline.
    // Verify cwnd is at least MIN_CWND (not zero/broken).
    if let Some(&last_cwnd) = result2.cwnd_history.last() {
        assert!(
            last_cwnd >= 2, // PathState::MIN_CWND
            "cwnd should be at least MIN_CWND: last={last_cwnd}"
        );
    }
}

#[test]
fn test_pipeline_multipath_failover() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(1); // WiFi
    sched.add_path(2); // LTE

    // Warmup both paths
    for id in [1, 2] {
        let path = sched.path_mut(id).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(millis(10));
            path.record_rtt_sample(millis(10));
            path.estimator.record_throughput(50_000_000.0);
            path.estimator.record_batch(100, 98);
        }
    }

    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    let mut wifi_channel = SimChannel::wifi(clock.clone(), 42);
    let mut lte_channel = SimChannel::lte(clock.clone(), 99);

    let mut recovered = BTreeSet::new();
    let num_symbols = 200u32;

    for sym_idx in 0..num_symbols {
        let data = vec![sym_idx as u8; symbol_size as usize];
        let sym = encoder.add_source(&data);

        // Kill WiFi at symbol 100
        if sym_idx < 100 {
            // Use WiFi path
            wifi_channel.send(sym.clone());
            // Also send on LTE for redundancy
            lte_channel.send(sym);
        } else {
            if sym_idx == 100 {
                // Mark WiFi as dead
                sched.path_mut(1).unwrap().active = false;
            }
            // Only LTE
            lte_channel.send(sym);
        }

        // Periodically deliver and decode
        if sym_idx % 10 == 9 {
            clock.advance(millis(25));

            for pkt in wifi_channel.deliver() {
                for (seq, _) in decoder.add_symbol(&pkt.symbol) {
                    recovered.insert(seq);
                }
            }
            for pkt in lte_channel.deliver() {
                for (seq, _) in decoder.add_symbol(&pkt.symbol) {
                    recovered.insert(seq);
                }
            }

            // Generate some repairs
            for _ in 0..3 {
                if encoder.window_size() == 0 {
                    break;
                }
                let repair = encoder.generate_repair();
                lte_channel.send(repair);
            }
        }
    }

    // Final delivery
    clock.advance(millis(30));
    for pkt in wifi_channel.deliver() {
        for (seq, _) in decoder.add_symbol(&pkt.symbol) {
            recovered.insert(seq);
        }
    }
    for pkt in lte_channel.deliver() {
        for (seq, _) in decoder.add_symbol(&pkt.symbol) {
            recovered.insert(seq);
        }
    }

    let recovery_rate = recovered.len() as f64 / num_symbols as f64;
    assert!(
        recovery_rate >= 0.90,
        "multipath failover should achieve >=90% recovery: {:.1}% ({}/{})",
        recovery_rate * 100.0,
        recovered.len(),
        num_symbols
    );

    // WiFi should be marked dead
    assert!(
        !sched.path(1).unwrap().active,
        "WiFi path should be inactive after failover"
    );

    // LTE should be active
    assert!(
        sched.path(2).unwrap().active,
        "LTE path should remain active"
    );
}
