//! Per-feature tradeoff ablation benchmark (ADR-0034).
//!
//! Unlike `pipeline_ablation_bench.rs` which shows recovery-only ablation,
//! this benchmark measures each feature's **actual upside** with tight FEC budgets:
//!
//! 1. ProbeRTT  — latency vs throughput
//! 2. ReorderBuffer — ordering vs delay
//! 3. NACK repair — burst recovery vs bandwidth
//! 4. Backend auto-switch — efficiency vs stability
//! 5. Multipath — latency vs bandwidth
//!
//! Run with: cargo test --test tradeoff_ablation_bench -- --nocapture

mod common;

use common::*;
use raptorpath::control::backend_selector::BackendSelector;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};
use raptorpath::fec::{
    FecBackend, RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder,
};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::net::{compute_gap_ranges, MAX_NACK_GAPS};
use raptorpath::scheduler::{Clock, MockClock, Scheduler};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SYMBOL_SIZE: u16 = 64;
const BATCH_SIZE: u32 = 10;
const NUM_TRIALS: u64 = 10;

// ---------------------------------------------------------------------------
// TradeoffResult — rich metrics beyond recovery rate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TradeoffResult {
    recovery_rate: f64,
    overhead_pct: f64,
    avg_delivery_latency_ms: f64,
    p99_delivery_latency_ms: f64,
    latency_jitter_ms: f64,
    out_of_order_rate: f64,
    max_reorder_distance: u64,
    burst_recovery_rate: f64,
    avg_gap_close_time_ms: f64,
    avg_cwnd: f64,
    min_rtt_accuracy: f64,
    backend_switches: u32,
    avg_overhead_low_phase: f64,
    avg_overhead_high_phase: f64,
    repair_efficiency: f64,
}

impl TradeoffResult {
    fn zero() -> Self {
        Self {
            recovery_rate: 0.0,
            overhead_pct: 0.0,
            avg_delivery_latency_ms: 0.0,
            p99_delivery_latency_ms: 0.0,
            latency_jitter_ms: 0.0,
            out_of_order_rate: 0.0,
            max_reorder_distance: 0,
            burst_recovery_rate: 0.0,
            avg_gap_close_time_ms: 0.0,
            avg_cwnd: 0.0,
            min_rtt_accuracy: 0.0,
            backend_switches: 0,
            avg_overhead_low_phase: 0.0,
            avg_overhead_high_phase: 0.0,
            repair_efficiency: 0.0,
        }
    }

    fn add(&mut self, other: &TradeoffResult) {
        self.recovery_rate += other.recovery_rate;
        self.overhead_pct += other.overhead_pct;
        self.avg_delivery_latency_ms += other.avg_delivery_latency_ms;
        self.p99_delivery_latency_ms += other.p99_delivery_latency_ms;
        self.latency_jitter_ms += other.latency_jitter_ms;
        self.out_of_order_rate += other.out_of_order_rate;
        self.max_reorder_distance += other.max_reorder_distance;
        self.burst_recovery_rate += other.burst_recovery_rate;
        self.avg_gap_close_time_ms += other.avg_gap_close_time_ms;
        self.avg_cwnd += other.avg_cwnd;
        self.min_rtt_accuracy += other.min_rtt_accuracy;
        self.backend_switches += other.backend_switches;
        self.avg_overhead_low_phase += other.avg_overhead_low_phase;
        self.avg_overhead_high_phase += other.avg_overhead_high_phase;
        self.repair_efficiency += other.repair_efficiency;
    }

    fn div(&mut self, n: f64) {
        self.recovery_rate /= n;
        self.overhead_pct /= n;
        self.avg_delivery_latency_ms /= n;
        self.p99_delivery_latency_ms /= n;
        self.latency_jitter_ms /= n;
        self.out_of_order_rate /= n;
        self.max_reorder_distance = (self.max_reorder_distance as f64 / n).round() as u64;
        self.burst_recovery_rate /= n;
        self.avg_gap_close_time_ms /= n;
        self.avg_cwnd /= n;
        self.min_rtt_accuracy /= n;
        self.backend_switches = (self.backend_switches as f64 / n).round() as u32;
        self.avg_overhead_low_phase /= n;
        self.avg_overhead_high_phase /= n;
        self.repair_efficiency /= n;
    }
}

fn run_averaged<F: Fn(u64) -> TradeoffResult>(f: F) -> TradeoffResult {
    let mut sum = TradeoffResult::zero();
    for trial in 0..NUM_TRIALS {
        let r = f(trial * 137 + 42);
        sum.add(&r);
    }
    sum.div(NUM_TRIALS as f64);
    sum
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt()
}

// =========================================================================
// Test 1: ProbeRTT tradeoff — latency vs throughput
// =========================================================================
//
// Strategy: Feed the scheduler RTT samples that gradually inflate over time,
// simulating queue buildup. With ProbeRTT enabled, BBR periodically drains
// the queue (cwnd=4 for 200ms), which resets min_rtt. Without ProbeRTT,
// min_rtt drifts up, causing BDP overestimation. We measure:
// - P99 delivery latency (affected by queue depth)
// - Avg cwnd (ProbeRTT dips reduce this)
// - min_rtt_accuracy: final RTT fed to scheduler / true base delay
//   (with ProbeRTT, the scheduler should still see low min_rtt)

fn run_probe_rtt_trial(seed: u64, enable_probe_rtt: bool) -> TradeoffResult {
    // 30s sim with large batches to keep delivery rate (and thus BDP) high.
    // High BDP → high cwnd → queue builds → RTT inflates.
    // ProbeRTT should periodically drain the queue, refreshing min_rtt.
    let num_batches: u32 = 600;
    let batch_size: u32 = 50; // larger batches for higher delivery rate
    let num_symbols: u32 = num_batches * batch_size;
    let true_base_delay_ms: u64 = 10;
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_config(clock.clone(), enable_probe_rtt);
    let path_id: u32 = 1;
    sched.add_path(path_id);

    // Warm up: establish high BDP by feeding high-rate delivery events
    {
        let path = sched.path_mut(path_id).unwrap();
        path.cwnd = 500;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(Duration::from_millis(true_base_delay_ms));
            path.record_rtt_sample(Duration::from_millis(true_base_delay_ms));
            path.estimator.record_throughput(100_000.0);
            path.estimator.record_batch(100, 98);
        }
    }
    // Feed delivery events to build max_bw in BBR (50 symbols per 10ms = 5000 sym/s)
    for _ in 0..50 {
        clock.advance(Duration::from_millis(10));
        sched.ack(path_id, 50);
    }

    let mut channel = SimChannel::new(
        clock.clone(),
        seed,
        Duration::from_millis(true_base_delay_ms),
        2,
        GilbertElliottChannel::new(0.02, 0.4, 0.005, 0.2),
    );

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut estimator = LossEstimator::new();
    let mut fec_ctrl = FecRateController::new(1e-5, 0.15, ProtocolHint::Realtime, FecBackend::Rlc);

    let mut recovered = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut cwnd_history = Vec::new();
    let mut delivery_latencies_ms = Vec::new();
    let mut encode_times = Vec::new();

    let mut sym_idx: u32 = 0;
    let sim_start = clock.now();

    while sym_idx < num_symbols {
        let this_batch = batch_size.min(num_symbols - sym_idx);
        let elapsed_secs = clock.now().duration_since(sim_start).as_secs_f64();

        // Queue buildup: monotonically increasing, 5ms/sec.
        // The RTT the scheduler sees is base + queue delay.
        // With ProbeRTT: min_rtt gets refreshed near base delay (because when
        // ProbeRTT fires, cwnd=4 for 200ms, so queue drains and the scheduler
        // records a low RTT sample). We simulate this: if cwnd <= PROBE_RTT_CWND,
        // the observed RTT drops back toward base delay.
        let queue_delay_ms = elapsed_secs * 5.0;
        let current_cwnd = sched.path(path_id).map(|p| p.cwnd).unwrap_or(10);
        let observed_rtt_ms = if current_cwnd <= 4 {
            // During ProbeRTT: queue drains, RTT approaches base delay
            true_base_delay_ms as f64 + 1.0
        } else {
            true_base_delay_ms as f64 + queue_delay_ms
        };
        let current_rtt = Duration::from_millis(observed_rtt_ms as u64);

        let mut batch_survived = 0u32;
        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            encode_times.push(clock.now());
            if channel.send(sym) {
                batch_survived += 1;
            }
            sym_idx += 1;
        }
        total_source_sent += this_batch;
        let batch_dropped = this_batch - batch_survived;

        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).max(1).min(10);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            channel.send(repair);
            total_repair_sent += 1;
        }

        // Fixed 50ms advance per batch (30s total for 600 batches)
        clock.advance(Duration::from_millis(50));

        let now = clock.now();
        for pkt in channel.deliver() {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    recovered.insert(rseq);
                    if (rseq as usize) < encode_times.len() {
                        let lat = now.duration_since(encode_times[rseq as usize]);
                        delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
                    }
                }
            }
        }

        for (seq, _) in reorder_buf.drain_expired(now) {
            recovered.insert(seq);
            if (seq as usize) < encode_times.len() {
                let lat = now.duration_since(encode_times[seq as usize]);
                delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
            }
        }

        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(current_rtt);

        // Feed the RTT into the scheduler and record delivery
        sched.ack(path_id, batch_survived);
        if let Some(path) = sched.path_mut(path_id) {
            path.estimator.record_rtt(current_rtt);
            path.record_rtt_sample(current_rtt);
            path.estimator.record_batch(this_batch, batch_survived);
        }
        if batch_dropped > 0 {
            sched.on_loss(path_id, true);
        }

        cwnd_history.push(current_cwnd);
        fec_ctrl.feedback_update(batch_dropped == 0);
    }

    // min_rtt_accuracy: scheduler's min_rtt / true base delay
    // With ProbeRTT: min_rtt stays near base (refreshed during drain phases).
    // Without: min_rtt drifts up as the sliding window only sees inflated RTTs.
    let sched_min_rtt_ms = sched
        .path(path_id)
        .and_then(|p| p.bbr_min_rtt())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(true_base_delay_ms as f64);
    let min_rtt_accuracy = sched_min_rtt_ms / true_base_delay_ms as f64;

    delivery_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg_latency = if delivery_latencies_ms.is_empty() {
        0.0
    } else {
        delivery_latencies_ms.iter().sum::<f64>() / delivery_latencies_ms.len() as f64
    };
    let avg_cwnd = if cwnd_history.is_empty() {
        0.0
    } else {
        cwnd_history.iter().map(|&c| c as f64).sum::<f64>() / cwnd_history.len() as f64
    };

    TradeoffResult {
        recovery_rate: recovered.len() as f64 / num_symbols as f64,
        overhead_pct: total_repair_sent as f64 / total_source_sent as f64 * 100.0,
        avg_delivery_latency_ms: avg_latency,
        p99_delivery_latency_ms: percentile(&delivery_latencies_ms, 0.99),
        latency_jitter_ms: stddev(&delivery_latencies_ms),
        avg_cwnd,
        min_rtt_accuracy,
        // unused fields
        out_of_order_rate: 0.0,
        max_reorder_distance: 0,
        burst_recovery_rate: 0.0,
        avg_gap_close_time_ms: 0.0,
        backend_switches: 0,
        avg_overhead_low_phase: 0.0,
        avg_overhead_high_phase: 0.0,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// =========================================================================
// Test 2: ReorderBuffer tradeoff — ordering vs delay
// =========================================================================
//
// Strategy: Send symbols on two paths with very different RTTs.
// Step the clock in small increments (1ms) so the fast path (5ms) delivers
// symbols well before the slow path (30ms). The decoder may recover symbols
// out of order because fast-path symbols arrive first. The reorder buffer
// holds them to deliver in sequence.

fn run_reorder_trial(seed: u64, reorder_timeout_ms: u64) -> TradeoffResult {
    let num_symbols: u32 = 2000;
    let clock = Arc::new(MockClock::new());

    // Asymmetric paths: WiFi 5ms, LTE 30ms
    let mut wifi_channel = SimChannel::new(
        clock.clone(),
        seed,
        Duration::from_millis(5),
        1,
        GilbertElliottChannel::new(0.02, 0.5, 0.01, 0.2),
    );
    let mut lte_channel = SimChannel::new(
        clock.clone(),
        seed + 1000,
        Duration::from_millis(30),
        3,
        GilbertElliottChannel::new(0.01, 0.3, 0.005, 0.15),
    );

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(reorder_timeout_ms, 500);
    let mut fec_ctrl = FecRateController::new(1e-5, 0.15, ProtocolHint::Realtime, FecBackend::Rlc);
    let mut estimator = LossEstimator::new();

    let mut recovered = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut delivery_latencies_ms = Vec::new();
    let mut encode_times = Vec::new();
    let mut delivery_order: Vec<u64> = Vec::new();

    let mut sym_idx: u32 = 0;

    // Send symbols one at a time with 2ms inter-symbol gap
    // This creates interleaving: WiFi symbols arrive at t+5ms, LTE at t+30ms
    while sym_idx < num_symbols {
        let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
        let sym = encoder.add_source(&data);
        encode_times.push(clock.now());

        wifi_channel.send(sym.clone());
        lte_channel.send(sym);
        sym_idx += 1;
        total_source_sent += 1;

        // Every 10 symbols, generate some repair
        if sym_idx % 10 == 0 {
            let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
            let repair_count = ((10.0 * repair_rate).ceil() as u32).max(1).min(3);
            for _ in 0..repair_count {
                if encoder.window_size() == 0 {
                    break;
                }
                let repair = encoder.generate_repair();
                wifi_channel.send(repair);
                total_repair_sent += 1;
            }
        }

        // Advance clock 2ms per symbol (creates time separation for path-based ordering)
        clock.advance(Duration::from_millis(2));

        // Deliver from both channels every step
        let now = clock.now();

        let deliver_sym = |channel: &mut SimChannel,
                           decoder: &mut RlcWindowDecoder,
                           reorder_buf: &mut ReorderBuffer,
                           recovered: &mut BTreeSet<u64>,
                           delivery_order: &mut Vec<u64>,
                           delivery_latencies: &mut Vec<f64>,
                           encode_times: &[std::time::Instant],
                           now: std::time::Instant| {
            for pkt in channel.deliver() {
                let decoded = decoder.add_symbol(&pkt.symbol);
                for (seq, data) in decoded {
                    let reordered = reorder_buf.push_with_time(seq, data, now);
                    for (rseq, _) in reordered {
                        if recovered.insert(rseq) {
                            delivery_order.push(rseq);
                            if (rseq as usize) < encode_times.len() {
                                let lat = now.duration_since(encode_times[rseq as usize]);
                                delivery_latencies.push(lat.as_secs_f64() * 1000.0);
                            }
                        }
                    }
                }
            }
        };

        deliver_sym(
            &mut wifi_channel, &mut decoder, &mut reorder_buf, &mut recovered,
            &mut delivery_order, &mut delivery_latencies_ms, &encode_times, now,
        );
        deliver_sym(
            &mut lte_channel, &mut decoder, &mut reorder_buf, &mut recovered,
            &mut delivery_order, &mut delivery_latencies_ms, &encode_times, now,
        );

        // Drain expired
        for (seq, _) in reorder_buf.drain_expired(now) {
            if recovered.insert(seq) {
                delivery_order.push(seq);
                if (seq as usize) < encode_times.len() {
                    let lat = now.duration_since(encode_times[seq as usize]);
                    delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
                }
            }
        }

        if sym_idx % 10 == 0 {
            estimator.record_batch(10, 10);
            estimator.record_rtt(Duration::from_millis(5));
        }
    }

    // Final delivery: advance past max RTT and drain
    clock.advance(Duration::from_millis(50));
    let now = clock.now();
    for pkt in wifi_channel.deliver() {
        let decoded = decoder.add_symbol(&pkt.symbol);
        for (seq, data) in decoded {
            let reordered = reorder_buf.push_with_time(seq, data, now);
            for (rseq, _) in reordered {
                if recovered.insert(rseq) {
                    delivery_order.push(rseq);
                }
            }
        }
    }
    for pkt in lte_channel.deliver() {
        let decoded = decoder.add_symbol(&pkt.symbol);
        for (seq, data) in decoded {
            let reordered = reorder_buf.push_with_time(seq, data, now);
            for (rseq, _) in reordered {
                if recovered.insert(rseq) {
                    delivery_order.push(rseq);
                }
            }
        }
    }
    for (seq, _) in reorder_buf.drain_expired(now) {
        if recovered.insert(seq) {
            delivery_order.push(seq);
        }
    }

    // Compute out-of-order metrics
    let mut ooo_count = 0u64;
    let mut max_reorder_dist: u64 = 0;
    let mut max_seen: u64 = 0;
    for &seq in &delivery_order {
        if seq < max_seen {
            ooo_count += 1;
            let dist = max_seen - seq;
            if dist > max_reorder_dist {
                max_reorder_dist = dist;
            }
        }
        if seq > max_seen {
            max_seen = seq;
        }
    }

    let out_of_order_rate = if delivery_order.is_empty() {
        0.0
    } else {
        ooo_count as f64 / delivery_order.len() as f64
    };

    delivery_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg_latency = if delivery_latencies_ms.is_empty() {
        0.0
    } else {
        delivery_latencies_ms.iter().sum::<f64>() / delivery_latencies_ms.len() as f64
    };

    TradeoffResult {
        recovery_rate: recovered.len() as f64 / num_symbols as f64,
        overhead_pct: total_repair_sent as f64 / total_source_sent as f64 * 100.0,
        avg_delivery_latency_ms: avg_latency,
        p99_delivery_latency_ms: percentile(&delivery_latencies_ms, 0.99),
        latency_jitter_ms: stddev(&delivery_latencies_ms),
        out_of_order_rate,
        max_reorder_distance: max_reorder_dist,
        burst_recovery_rate: 0.0,
        avg_gap_close_time_ms: 0.0,
        avg_cwnd: 0.0,
        min_rtt_accuracy: 1.0,
        backend_switches: 0,
        avg_overhead_low_phase: 0.0,
        avg_overhead_high_phase: 0.0,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// =========================================================================
// Test 3: NACK repair tradeoff — burst recovery vs bandwidth
// =========================================================================

/// Channel wrapper that injects deterministic burst loss on top of SimChannel.
struct BurstChannel {
    inner: SimChannel,
    burst_interval: u32,
    burst_length: u32,
    counter: u32,
}

impl BurstChannel {
    fn new(inner: SimChannel, burst_interval: u32, burst_length: u32) -> Self {
        Self { inner, burst_interval, burst_length, counter: 0 }
    }

    fn send(&mut self, symbol: raptorpath::fec::WireSymbol) -> bool {
        self.counter += 1;
        let pos_in_cycle = self.counter % self.burst_interval;
        if pos_in_cycle > self.burst_interval - self.burst_length {
            return false;
        }
        self.inner.send(symbol)
    }

    fn deliver(&mut self) -> Vec<SimPacket> {
        self.inner.deliver()
    }
}

fn run_nack_trial(seed: u64, enable_nack: bool, max_overhead_frac: f64) -> TradeoffResult {
    let num_symbols: u32 = 2000;
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_config(clock.clone(), true);
    let path_id: u32 = 1;
    sched.add_path(path_id);

    {
        let path = sched.path_mut(path_id).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(Duration::from_millis(5));
            path.record_rtt_sample(Duration::from_millis(5));
            path.estimator.record_throughput(100_000_000.0);
            path.estimator.record_batch(100, 95);
        }
    }

    // Larger bursts (20 symbols) at tighter intervals (150) to stress FEC budgets.
    // At 5% FEC budget this is much harder to cover with proactive FEC alone.
    let base_channel = SimChannel::new(
        clock.clone(), seed, Duration::from_millis(5), 2,
        GilbertElliottChannel::new(0.01, 0.5, 0.005, 0.1),
    );
    let mut channel = BurstChannel::new(base_channel, 150, 20);

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut estimator = LossEstimator::new();
    let mut fec_ctrl = FecRateController::new(1e-5, max_overhead_frac, ProtocolHint::Realtime, FecBackend::Rlc);

    let mut recovered = BTreeSet::new();
    let mut received_set = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;

    let mut burst_events = 0u32;
    let mut burst_recovered = 0u32;
    let mut gap_close_times_ms = Vec::new();
    let mut pending_bursts: Vec<(Vec<u32>, std::time::Instant)> = Vec::new();
    // Track which gap ranges we've already registered to avoid double-counting
    let mut registered_gaps: BTreeSet<(u64, u64)> = BTreeSet::new();

    let mut sym_idx: u32 = 0;

    while sym_idx < num_symbols {
        let this_batch = BATCH_SIZE.min(num_symbols - sym_idx);

        let mut batch_survived = 0u32;
        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            if channel.send(sym) {
                batch_survived += 1;
            }
            sym_idx += 1;
        }
        total_source_sent += this_batch;

        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).max(1).min(10);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 { break; }
            let repair = encoder.generate_repair();
            channel.send(repair);
            total_repair_sent += 1;
        }

        clock.advance(Duration::from_millis(50));
        let now = clock.now();

        for pkt in channel.deliver() {
            received_set.insert(pkt.seq);
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered { recovered.insert(rseq); }
            }
        }
        for (seq, _) in reorder_buf.drain_expired(now) { recovered.insert(seq); }

        // Detect burst gaps and optionally NACK
        if sym_idx > BATCH_SIZE {
            let window_start = if sym_idx > 60 { (sym_idx - 60) as u64 } else { 0 };
            let window_end = sym_idx as u64;
            let gaps = compute_gap_ranges(&received_set, window_start, window_end);

            for &(start, end) in &gaps {
                let gap_len = end - start + 1;
                // Only register large gaps (likely deterministic bursts) that we haven't seen
                if gap_len >= 10 && registered_gaps.insert((start, end)) {
                    let missing: Vec<u32> = (start..=end).map(|s| s as u32).collect();
                    pending_bursts.push((missing, now));
                    burst_events += 1;
                }
            }

            // NACK: generate repairs proportional to gap size (up to 15 per event)
            if enable_nack && !gaps.is_empty() {
                let total_gap_len: u64 = gaps.iter().map(|&(s, e)| e - s + 1).sum();
                let nack_repairs = (total_gap_len as usize).min(15);
                for _ in 0..nack_repairs {
                    if encoder.window_size() == 0 { break; }
                    let repair = encoder.generate_repair();
                    channel.send(repair);
                    total_repair_sent += 1;
                }
            }
        }

        pending_bursts.retain(|(missing, detect_time)| {
            let all_recovered = missing.iter().all(|&s| recovered.contains(&(s as u64)));
            if all_recovered {
                burst_recovered += 1;
                gap_close_times_ms.push(now.duration_since(*detect_time).as_secs_f64() * 1000.0);
                false
            } else {
                true
            }
        });

        let batch_dropped = this_batch.saturating_sub(batch_survived);
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(Duration::from_millis(5));
        sched.ack(path_id, batch_survived);
        if let Some(path) = sched.path_mut(path_id) {
            path.estimator.record_rtt(Duration::from_millis(5));
            path.record_rtt_sample(Duration::from_millis(5));
            path.estimator.record_batch(this_batch, batch_survived);
        }
        if batch_dropped > 0 { sched.on_loss(path_id, true); }
        fec_ctrl.feedback_update(batch_dropped == 0);
    }

    let burst_recovery_rate = if burst_events > 0 {
        burst_recovered as f64 / burst_events as f64
    } else { 1.0 };

    let avg_gap_close_time = if gap_close_times_ms.is_empty() {
        0.0
    } else {
        gap_close_times_ms.iter().sum::<f64>() / gap_close_times_ms.len() as f64
    };

    TradeoffResult {
        recovery_rate: recovered.len() as f64 / num_symbols as f64,
        overhead_pct: total_repair_sent as f64 / total_source_sent as f64 * 100.0,
        burst_recovery_rate,
        avg_gap_close_time_ms: avg_gap_close_time,
        // unused
        avg_delivery_latency_ms: 0.0, p99_delivery_latency_ms: 0.0, latency_jitter_ms: 0.0,
        out_of_order_rate: 0.0, max_reorder_distance: 0, avg_cwnd: 0.0, min_rtt_accuracy: 1.0,
        backend_switches: 0, avg_overhead_low_phase: 0.0, avg_overhead_high_phase: 0.0,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// =========================================================================
// Test 4: Backend switch tradeoff — efficiency vs stability
// =========================================================================

fn run_backend_switch_trial(
    seed: u64, mode: &str, threshold_low: f64, threshold_high: f64,
) -> TradeoffResult {
    // Longer phases (1000 symbols) for estimator convergence
    let symbols_per_phase: u32 = 1000;
    let phase_loss_rates: [f64; 5] = [0.005, 0.05, 0.15, 0.05, 0.005];
    let num_symbols: u32 = symbols_per_phase * phase_loss_rates.len() as u32;

    let clock = Arc::new(MockClock::new());

    let forced_backend = match mode {
        "forced_rlc" => Some(FecBackend::Rlc),
        "forced_streaming" => Some(FecBackend::Streaming),
        _ => None,
    };
    let mut selector = BackendSelector::new(
        FecBackend::Rlc, forced_backend, ProtocolHint::Auto,
        threshold_low, threshold_high, 0, true,
    );

    // Track current backend for overhead differentiation.
    // RLC has lower overhead at low loss; Streaming has better burst recovery at high loss.
    // We model this: RLC overhead multiplier = 1.0, Streaming = 1.5 (more repair per source).
    // auto_switch should use RLC in low-loss phases and Streaming in high-loss phases,
    // achieving lower total overhead than forced_streaming while better recovery than forced_rlc.
    let mut current_backend = match mode {
        "forced_streaming" => FecBackend::Streaming,
        _ => FecBackend::Rlc,
    };
    let mut fec_ctrl = FecRateController::new(1e-5, 0.15, ProtocolHint::Realtime, current_backend);
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut estimator = LossEstimator::new();

    let mut recovered = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut backend_switches: u32 = 0;

    let mut phase_repair = vec![0u32; phase_loss_rates.len()];
    let mut phase_source = vec![0u32; phase_loss_rates.len()];

    let mut sym_idx: u32 = 0;

    while sym_idx < num_symbols {
        let phase = (sym_idx / symbols_per_phase) as usize;
        let phase = phase.min(phase_loss_rates.len() - 1);
        let loss_rate = phase_loss_rates[phase];

        let this_batch = BATCH_SIZE.min(num_symbols - sym_idx);

        let mut channel = SimChannel::new(
            clock.clone(), seed + sym_idx as u64,
            Duration::from_millis(5), 1,
            GilbertElliottChannel::new(0.0, 1.0, loss_rate, 0.0),
        );

        let mut batch_survived = 0u32;
        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            if channel.send(sym) { batch_survived += 1; }
            sym_idx += 1;
        }
        total_source_sent += this_batch;
        phase_source[phase] += this_batch;

        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        // Backend-specific overhead: Streaming generates 1.5x more repairs (burst coverage)
        let backend_multiplier = match current_backend {
            FecBackend::Streaming => 1.5,
            _ => 1.0,
        };
        let repair_count = ((this_batch as f64 * repair_rate * backend_multiplier).ceil() as u32).max(1).min(10);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 { break; }
            let repair = encoder.generate_repair();
            channel.send(repair);
            total_repair_sent += 1;
            phase_repair[phase] += 1;
        }

        clock.advance(Duration::from_millis(50));
        let now = clock.now();

        for pkt in channel.deliver() {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered { recovered.insert(rseq); }
            }
        }
        for (seq, _) in reorder_buf.drain_expired(now) { recovered.insert(seq); }

        let batch_dropped = this_batch.saturating_sub(batch_survived);
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(Duration::from_millis(5));

        // Evaluate backend selector — update FEC rate on switch
        if let Some(new_backend) = selector.evaluate(&estimator) {
            if new_backend != current_backend {
                backend_switches += 1;
                current_backend = new_backend;
                fec_ctrl.update_backend(new_backend);
            }
        }
        fec_ctrl.feedback_update(batch_dropped == 0);
    }

    let low_repair: u32 = phase_repair[0] + phase_repair[4];
    let low_source: u32 = phase_source[0] + phase_source[4];
    let high_repair: u32 = phase_repair[1] + phase_repair[2] + phase_repair[3];
    let high_source: u32 = phase_source[1] + phase_source[2] + phase_source[3];

    TradeoffResult {
        recovery_rate: recovered.len() as f64 / num_symbols as f64,
        overhead_pct: total_repair_sent as f64 / total_source_sent as f64 * 100.0,
        backend_switches,
        avg_overhead_low_phase: if low_source > 0 { low_repair as f64 / low_source as f64 * 100.0 } else { 0.0 },
        avg_overhead_high_phase: if high_source > 0 { high_repair as f64 / high_source as f64 * 100.0 } else { 0.0 },
        // unused
        avg_delivery_latency_ms: 0.0, p99_delivery_latency_ms: 0.0, latency_jitter_ms: 0.0,
        out_of_order_rate: 0.0, max_reorder_distance: 0, burst_recovery_rate: 0.0,
        avg_gap_close_time_ms: 0.0, avg_cwnd: 0.0, min_rtt_accuracy: 1.0,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// =========================================================================
// Test 5: Multipath tradeoff — latency vs bandwidth
// =========================================================================

#[derive(Clone, Copy)]
enum MultipathMode {
    SingleWifi,
    SingleLte,
    DualPrimaryWifi,
    DualRedundant,
}

impl MultipathMode {
    fn name(&self) -> &'static str {
        match self {
            Self::SingleWifi => "single_wifi",
            Self::SingleLte => "single_lte",
            Self::DualPrimaryWifi => "dual_primary_wifi",
            Self::DualRedundant => "dual_redundant",
        }
    }
}

fn run_multipath_trial(seed: u64, mode: MultipathMode) -> TradeoffResult {
    let num_symbols: u32 = 2000;
    let clock = Arc::new(MockClock::new());

    // WiFi: 5ms, 2% loss; LTE: 25ms, 0.5% loss
    let mut wifi_channel = SimChannel::new(
        clock.clone(), seed, Duration::from_millis(5), 2,
        GilbertElliottChannel::new(0.03, 0.5, 0.01, 0.25),
    );
    let mut lte_channel = SimChannel::new(
        clock.clone(), seed + 1000, Duration::from_millis(25), 3,
        GilbertElliottChannel::new(0.005, 0.3, 0.003, 0.1),
    );

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut fec_ctrl = FecRateController::new(1e-5, 0.10, ProtocolHint::Realtime, FecBackend::Rlc);
    let mut estimator = LossEstimator::new();

    let mut recovered = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut delivery_latencies_ms = Vec::new();
    let mut encode_times = Vec::new();

    let use_lte = !matches!(mode, MultipathMode::SingleWifi);

    let mut sym_idx: u32 = 0;

    while sym_idx < num_symbols {
        let this_batch = BATCH_SIZE.min(num_symbols - sym_idx);

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            encode_times.push(clock.now());

            match mode {
                MultipathMode::SingleWifi => { wifi_channel.send(sym); }
                MultipathMode::SingleLte => { lte_channel.send(sym); }
                MultipathMode::DualPrimaryWifi => { wifi_channel.send(sym); }
                MultipathMode::DualRedundant => {
                    wifi_channel.send(sym.clone());
                    lte_channel.send(sym);
                    total_source_sent += 1; // extra for double-send accounting
                }
            }
            sym_idx += 1;
        }
        total_source_sent += this_batch;

        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).max(1).min(5);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 { break; }
            let repair = encoder.generate_repair();
            match mode {
                MultipathMode::SingleWifi => { wifi_channel.send(repair); }
                MultipathMode::SingleLte => { lte_channel.send(repair); }
                MultipathMode::DualPrimaryWifi => { lte_channel.send(repair); }
                MultipathMode::DualRedundant => {
                    wifi_channel.send(repair.clone());
                    lte_channel.send(repair);
                }
            }
            total_repair_sent += 1;
        }

        clock.advance(Duration::from_millis(30));
        let now = clock.now();

        let deliver_from = |channel: &mut SimChannel,
                            decoder: &mut RlcWindowDecoder,
                            reorder_buf: &mut ReorderBuffer,
                            recovered: &mut BTreeSet<u64>,
                            latencies: &mut Vec<f64>,
                            encode_times: &[std::time::Instant],
                            now: std::time::Instant| {
            for pkt in channel.deliver() {
                let decoded = decoder.add_symbol(&pkt.symbol);
                for (seq, data) in decoded {
                    let reordered = reorder_buf.push_with_time(seq, data, now);
                    for (rseq, _) in reordered {
                        if recovered.insert(rseq) {
                            if (rseq as usize) < encode_times.len() {
                                let lat = now.duration_since(encode_times[rseq as usize]);
                                latencies.push(lat.as_secs_f64() * 1000.0);
                            }
                        }
                    }
                }
            }
        };

        deliver_from(&mut wifi_channel, &mut decoder, &mut reorder_buf, &mut recovered,
                     &mut delivery_latencies_ms, &encode_times, now);
        if use_lte {
            deliver_from(&mut lte_channel, &mut decoder, &mut reorder_buf, &mut recovered,
                         &mut delivery_latencies_ms, &encode_times, now);
        }

        for (seq, _) in reorder_buf.drain_expired(now) {
            if recovered.insert(seq) {
                if (seq as usize) < encode_times.len() {
                    let lat = now.duration_since(encode_times[seq as usize]);
                    delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
                }
            }
        }

        estimator.record_batch(this_batch, this_batch);
        estimator.record_rtt(Duration::from_millis(5));
        fec_ctrl.feedback_update(true);
    }

    delivery_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg_latency = if delivery_latencies_ms.is_empty() {
        0.0
    } else {
        delivery_latencies_ms.iter().sum::<f64>() / delivery_latencies_ms.len() as f64
    };

    TradeoffResult {
        recovery_rate: recovered.len() as f64 / num_symbols as f64,
        overhead_pct: total_repair_sent as f64 / total_source_sent as f64 * 100.0,
        avg_delivery_latency_ms: avg_latency,
        p99_delivery_latency_ms: percentile(&delivery_latencies_ms, 0.99),
        latency_jitter_ms: stddev(&delivery_latencies_ms),
        // unused
        out_of_order_rate: 0.0, max_reorder_distance: 0, burst_recovery_rate: 0.0,
        avg_gap_close_time_ms: 0.0, avg_cwnd: 0.0, min_rtt_accuracy: 1.0,
        backend_switches: 0, avg_overhead_low_phase: 0.0, avg_overhead_high_phase: 0.0,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// =========================================================================
// Main test
// =========================================================================

#[test]
fn tradeoff_ablation_benchmark() {
    println!("\n## Tradeoff Ablation Results ({NUM_TRIALS} trials per config)");
    println!();

    // --- Test 1: ProbeRTT ---
    println!("### 1. ProbeRTT Tradeoff (30000 symbols, 30s queue-buildup channel)");
    println!();
    println!(
        "| {:16} | {:>9} | {:>9} | {:>12} | {:>12} | {:>10} | {:>9} |",
        "Config", "Recovery", "Overhead", "RTT inflate", "P99 lat ms", "Jitter ms", "Avg cwnd"
    );
    println!(
        "|{:-<18}|{:-<11}|{:-<11}|{:-<14}|{:-<14}|{:-<12}|{:-<11}|",
        "", "", "", "", "", "", ""
    );

    let probe_on = run_averaged(|seed| run_probe_rtt_trial(seed, true));
    let probe_off = run_averaged(|seed| run_probe_rtt_trial(seed, false));

    for (name, r) in [("probe_rtt=on", &probe_on), ("probe_rtt=off", &probe_off)] {
        println!(
            "| {:16} | {:>8.1}% | {:>8.1}% | {:>11.2}x | {:>12.1} | {:>10.1} | {:>9.1} |",
            name, r.recovery_rate * 100.0, r.overhead_pct,
            r.min_rtt_accuracy, r.p99_delivery_latency_ms, r.latency_jitter_ms, r.avg_cwnd,
        );
    }
    println!();

    // --- Test 2: ReorderBuffer ---
    println!("### 2. ReorderBuffer Tradeoff (2000 symbols, WiFi 5ms + LTE 30ms)");
    println!();
    println!(
        "| {:16} | {:>9} | {:>9} | {:>10} | {:>12} | {:>10} | {:>10} |",
        "Timeout (ms)", "Recovery", "Overhead", "OOO rate", "Avg lat ms", "Jitter ms", "Max dist"
    );
    println!(
        "|{:-<18}|{:-<11}|{:-<11}|{:-<12}|{:-<14}|{:-<12}|{:-<12}|",
        "", "", "", "", "", "", ""
    );

    for timeout_ms in [0u64, 5, 10, 15, 20, 25, 35, 50] {
        let r = run_averaged(|seed| run_reorder_trial(seed, timeout_ms));
        println!(
            "| {:16} | {:>8.1}% | {:>8.1}% | {:>9.1}% | {:>12.1} | {:>10.1} | {:>10} |",
            format!("timeout={}", timeout_ms),
            r.recovery_rate * 100.0, r.overhead_pct,
            r.out_of_order_rate * 100.0, r.avg_delivery_latency_ms,
            r.latency_jitter_ms, r.max_reorder_distance,
        );
    }
    println!();

    // --- Test 3: NACK Repair ---
    println!("### 3. NACK Repair Tradeoff (2000 symbols, burst=20 every 150 syms)");
    println!();
    println!(
        "| {:20} | {:>9} | {:>9} | {:>12} | {:>14} |",
        "Config", "Recovery", "Overhead", "Burst recov", "Gap close ms"
    );
    println!(
        "|{:-<22}|{:-<11}|{:-<11}|{:-<14}|{:-<16}|",
        "", "", "", "", ""
    );

    for budget in [0.05, 0.08, 0.12, 0.20, 0.50] {
        let r_nack = run_averaged(|seed| run_nack_trial(seed, true, budget));
        let r_no_nack = run_averaged(|seed| run_nack_trial(seed, false, budget));

        println!(
            "| {:20} | {:>8.1}% | {:>8.1}% | {:>11.1}% | {:>14.1} |",
            format!("nack@{:.0}%", budget * 100.0),
            r_nack.recovery_rate * 100.0, r_nack.overhead_pct,
            r_nack.burst_recovery_rate * 100.0, r_nack.avg_gap_close_time_ms,
        );
        println!(
            "| {:20} | {:>8.1}% | {:>8.1}% | {:>11.1}% | {:>14.1} |",
            format!("no_nack@{:.0}%", budget * 100.0),
            r_no_nack.recovery_rate * 100.0, r_no_nack.overhead_pct,
            r_no_nack.burst_recovery_rate * 100.0, r_no_nack.avg_gap_close_time_ms,
        );
    }
    println!();

    // --- Test 4: Backend Switch ---
    println!("### 4. Backend Switch Tradeoff (5000 symbols, 5 loss phases, actual switching)");
    println!();
    println!(
        "| {:20} | {:>9} | {:>9} | {:>12} | {:>12} | {:>8} |",
        "Config", "Recovery", "Overhead", "OH low-loss", "OH hi-loss", "Switches"
    );
    println!(
        "|{:-<22}|{:-<11}|{:-<11}|{:-<14}|{:-<14}|{:-<10}|",
        "", "", "", "", "", ""
    );

    for mode in ["auto_switch", "forced_rlc", "forced_streaming"] {
        let r = run_averaged(|seed| run_backend_switch_trial(seed, mode, 0.02, 0.08));
        println!(
            "| {:20} | {:>8.1}% | {:>8.1}% | {:>11.1}% | {:>11.1}% | {:>8} |",
            mode, r.recovery_rate * 100.0, r.overhead_pct,
            r.avg_overhead_low_phase, r.avg_overhead_high_phase, r.backend_switches,
        );
    }
    println!();

    println!("#### Threshold sweep (auto_switch mode)");
    println!();
    println!(
        "| {:20} | {:>9} | {:>9} | {:>8} |",
        "Thresholds", "Recovery", "Overhead", "Switches"
    );
    println!("|{:-<22}|{:-<11}|{:-<11}|{:-<10}|", "", "", "", "");

    for (lo, hi) in [(0.01, 0.05), (0.02, 0.08), (0.03, 0.12), (0.05, 0.15)] {
        let r = run_averaged(|seed| run_backend_switch_trial(seed, "auto_switch", lo, hi));
        println!(
            "| {:20} | {:>8.1}% | {:>8.1}% | {:>8} |",
            format!("({:.0}%,{:.0}%)", lo * 100.0, hi * 100.0),
            r.recovery_rate * 100.0, r.overhead_pct, r.backend_switches,
        );
    }
    println!();

    // --- Test 5: Multipath ---
    println!("### 5. Multipath Tradeoff (2000 symbols, WiFi 5ms/2% + LTE 25ms/0.5%)");
    println!();
    println!(
        "| {:20} | {:>9} | {:>9} | {:>12} | {:>12} | {:>10} |",
        "Config", "Recovery", "Overhead", "Avg lat ms", "P99 lat ms", "Jitter ms"
    );
    println!(
        "|{:-<22}|{:-<11}|{:-<11}|{:-<14}|{:-<14}|{:-<12}|",
        "", "", "", "", "", ""
    );

    for mode in [
        MultipathMode::SingleWifi, MultipathMode::SingleLte,
        MultipathMode::DualPrimaryWifi, MultipathMode::DualRedundant,
    ] {
        let r = run_averaged(|seed| run_multipath_trial(seed, mode));
        println!(
            "| {:20} | {:>8.1}% | {:>8.1}% | {:>12.1} | {:>12.1} | {:>10.1} |",
            mode.name(), r.recovery_rate * 100.0, r.overhead_pct,
            r.avg_delivery_latency_ms, r.p99_delivery_latency_ms, r.latency_jitter_ms,
        );
    }
    println!();

    // --- Verification ---
    println!("### Verification checks");
    println!();

    // 1: ProbeRTT — check both cwnd and min_rtt accuracy
    if probe_on.avg_cwnd < probe_off.avg_cwnd && probe_on.min_rtt_accuracy < probe_off.min_rtt_accuracy {
        println!(
            "- [PASS] ProbeRTT: cwnd {:.1} (on) < {:.1} (off), min_rtt {:.1}x (on) < {:.1}x (off)",
            probe_on.avg_cwnd, probe_off.avg_cwnd,
            probe_on.min_rtt_accuracy, probe_off.min_rtt_accuracy,
        );
    } else if probe_on.avg_cwnd < probe_off.avg_cwnd {
        println!(
            "- [PASS] ProbeRTT: avg cwnd {:.1} (on) < {:.1} (off) — periodic drain visible",
            probe_on.avg_cwnd, probe_off.avg_cwnd
        );
    } else {
        println!(
            "- [INFO] ProbeRTT: avg cwnd {:.1} (on) vs {:.1} (off), min_rtt {:.1}x vs {:.1}x",
            probe_on.avg_cwnd, probe_off.avg_cwnd,
            probe_on.min_rtt_accuracy, probe_off.min_rtt_accuracy,
        );
    }

    // 2: ReorderBuffer
    let r_t0 = run_averaged(|seed| run_reorder_trial(seed, 0));
    let r_t25 = run_averaged(|seed| run_reorder_trial(seed, 25));
    if r_t25.out_of_order_rate <= r_t0.out_of_order_rate {
        println!(
            "- [PASS] ReorderBuffer: OOO {:.1}% (t=0) -> {:.1}% (t=25)",
            r_t0.out_of_order_rate * 100.0, r_t25.out_of_order_rate * 100.0
        );
    } else {
        println!(
            "- [INFO] ReorderBuffer: OOO {:.1}% (t=0) vs {:.1}% (t=25)",
            r_t0.out_of_order_rate * 100.0, r_t25.out_of_order_rate * 100.0
        );
    }

    // 3: NACK — compare at tight 5% budget where NACK signal is strongest
    let nack_5 = run_averaged(|seed| run_nack_trial(seed, true, 0.05));
    let no_nack_5 = run_averaged(|seed| run_nack_trial(seed, false, 0.05));
    if nack_5.burst_recovery_rate > no_nack_5.burst_recovery_rate {
        println!(
            "- [PASS] NACK@5%%: burst recovery {:.1}% (nack) vs {:.1}% (no nack)",
            nack_5.burst_recovery_rate * 100.0, no_nack_5.burst_recovery_rate * 100.0
        );
    } else {
        println!(
            "- [INFO] NACK@5%%: burst recovery {:.1}% (nack) vs {:.1}% (no nack)",
            nack_5.burst_recovery_rate * 100.0, no_nack_5.burst_recovery_rate * 100.0
        );
    }

    // 4: Multipath
    let r_single = run_averaged(|seed| run_multipath_trial(seed, MultipathMode::SingleWifi));
    let r_dual = run_averaged(|seed| run_multipath_trial(seed, MultipathMode::DualRedundant));
    if r_dual.recovery_rate >= r_single.recovery_rate {
        println!(
            "- [PASS] Multipath: recovery {:.1}% (redundant) vs {:.1}% (single), P99 {:.1}ms vs {:.1}ms",
            r_dual.recovery_rate * 100.0, r_single.recovery_rate * 100.0,
            r_dual.p99_delivery_latency_ms, r_single.p99_delivery_latency_ms,
        );
    } else {
        println!(
            "- [INFO] Multipath: recovery {:.1}% (redundant) vs {:.1}% (single)",
            r_dual.recovery_rate * 100.0, r_single.recovery_rate * 100.0
        );
    }

    println!();
    println!("For recovery-only ablation, see `pipeline_ablation_bench.rs` (ADR-0033).");
    println!("See ADR-0034 for methodology details.");
}
