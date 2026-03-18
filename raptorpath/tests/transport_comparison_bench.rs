//! Transport comparison benchmark: raptorpath FEC vs reliable QUIC/MPTCP baselines.
//!
//! Compares 5 transport configurations across 6 network scenarios, measuring
//! recovery rate, goodput, latency percentiles, completion time, overhead,
//! and in-order delivery rate.
//!
//! Run with: cargo test --test transport_comparison_bench -- --nocapture

mod common;

use common::*;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};
use raptorpath::fec::{
    FecBackend, RlcWindowDecoder, RlcWindowEncoder, WireSymbol, WindowDecoder, WindowEncoder,
};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::scheduler::{Clock, MockClock, Scheduler};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NUM_SYMBOLS: u32 = 4000;
const BATCH_SIZE: u32 = 10;
const NUM_TRIALS: u64 = 20;
const SYMBOL_SIZE: u16 = 64;

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct DeliveryMetrics {
    recovery_rate: f64,
    goodput_ratio: f64,
    latency_p50_ms: f64,
    latency_p95_ms: f64,
    latency_p99_ms: f64,
    completion_time_ms: f64,
    overhead_pct: f64,
    in_order_rate: f64,
}

impl DeliveryMetrics {
    fn zero() -> Self {
        Self {
            recovery_rate: 0.0,
            goodput_ratio: 0.0,
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            completion_time_ms: 0.0,
            overhead_pct: 0.0,
            in_order_rate: 0.0,
        }
    }

    fn add(&mut self, other: &DeliveryMetrics) {
        self.recovery_rate += other.recovery_rate;
        self.goodput_ratio += other.goodput_ratio;
        self.latency_p50_ms += other.latency_p50_ms;
        self.latency_p95_ms += other.latency_p95_ms;
        self.latency_p99_ms += other.latency_p99_ms;
        self.completion_time_ms += other.completion_time_ms;
        self.overhead_pct += other.overhead_pct;
        self.in_order_rate += other.in_order_rate;
    }

    fn div(&mut self, n: f64) {
        self.recovery_rate /= n;
        self.goodput_ratio /= n;
        self.latency_p50_ms /= n;
        self.latency_p95_ms /= n;
        self.latency_p99_ms /= n;
        self.completion_time_ms /= n;
        self.overhead_pct /= n;
        self.in_order_rate /= n;
    }
}

/// Compute percentile from sorted durations (0.0 = min, 1.0 = max).
fn percentile_ms(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Compute in-order delivery rate: fraction of symbols delivered in sequence order.
fn compute_in_order_rate(delivery_order: &[u64]) -> f64 {
    if delivery_order.len() <= 1 {
        return 1.0;
    }
    let mut in_order = 0u64;
    for i in 1..delivery_order.len() {
        if delivery_order[i] > delivery_order[i - 1] {
            in_order += 1;
        }
    }
    in_order as f64 / (delivery_order.len() - 1) as f64
}

// ---------------------------------------------------------------------------
// Transport configurations
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum TransportKind {
    QuicSingle,
    QuicDualRR,
    QuicDualMinRtt,
    RaptorpathSingle,
    RaptorpathDual,
}

struct TransportConfig {
    name: &'static str,
    kind: TransportKind,
}

fn transport_configs() -> Vec<TransportConfig> {
    vec![
        TransportConfig {
            name: "quic_single",
            kind: TransportKind::QuicSingle,
        },
        TransportConfig {
            name: "quic_dual_rr",
            kind: TransportKind::QuicDualRR,
        },
        TransportConfig {
            name: "quic_dual_minrtt",
            kind: TransportKind::QuicDualMinRtt,
        },
        TransportConfig {
            name: "raptorpath_single",
            kind: TransportKind::RaptorpathSingle,
        },
        TransportConfig {
            name: "raptorpath_dual",
            kind: TransportKind::RaptorpathDual,
        },
    ]
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ScenarioKind {
    DcLowLoss,
    WiFiBursty,
    LteHighRtt,
    WiFiLteHetero,
    LossySatellite,
    WiFiLteAsymmetric,
}

struct Scenario {
    name: &'static str,
    kind: ScenarioKind,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "dc_low_loss",
            kind: ScenarioKind::DcLowLoss,
        },
        Scenario {
            name: "wifi_bursty",
            kind: ScenarioKind::WiFiBursty,
        },
        Scenario {
            name: "lte_high_rtt",
            kind: ScenarioKind::LteHighRtt,
        },
        Scenario {
            name: "wifi_lte_hetero",
            kind: ScenarioKind::WiFiLteHetero,
        },
        Scenario {
            name: "lossy_satellite",
            kind: ScenarioKind::LossySatellite,
        },
        Scenario {
            name: "wifi_lte_asymmetric",
            kind: ScenarioKind::WiFiLteAsymmetric,
        },
    ]
}

// ---------------------------------------------------------------------------
// Channel factory helpers
// ---------------------------------------------------------------------------

/// Create primary + secondary lossy SimChannels for a scenario.
fn make_lossy_channels(
    scenario: ScenarioKind,
    clock: Arc<MockClock>,
    seed: u64,
) -> (SimChannel, SimChannel, Duration) {
    match scenario {
        ScenarioKind::DcLowLoss => {
            let primary = SimChannel::datacenter(clock.clone(), seed);
            let secondary = SimChannel::new(
                clock,
                seed + 1000,
                Duration::from_millis(2),
                0,
                GilbertElliottChannel::new(0.0, 1.0, 0.001, 0.0),
            );
            (primary, secondary, Duration::from_millis(1))
        }
        ScenarioKind::WiFiBursty => {
            let primary = SimChannel::wifi(clock.clone(), seed);
            let secondary = SimChannel::wifi(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(5))
        }
        ScenarioKind::LteHighRtt => {
            let primary = SimChannel::lte(clock.clone(), seed);
            let secondary = SimChannel::lte(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(20))
        }
        ScenarioKind::WiFiLteHetero => {
            let primary = SimChannel::wifi(clock.clone(), seed);
            let secondary = SimChannel::lte(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(5))
        }
        ScenarioKind::LossySatellite => {
            // Satellite: 100ms base, 10ms jitter, ~8% bursty loss
            let ge = GilbertElliottChannel::new(0.05, 0.4, 0.04, 0.5);
            let primary = SimChannel::new(
                clock.clone(),
                seed,
                Duration::from_millis(100),
                10,
                ge,
            );
            let ge2 = GilbertElliottChannel::new(0.05, 0.4, 0.04, 0.5);
            let secondary = SimChannel::new(
                clock,
                seed + 1000,
                Duration::from_millis(100),
                10,
                ge2,
            );
            (primary, secondary, Duration::from_millis(100))
        }
        ScenarioKind::WiFiLteAsymmetric => {
            // WiFi: 5ms, ~1% loss
            let ge_wifi = GilbertElliottChannel::new(0.01, 0.5, 0.005, 0.15);
            let primary = SimChannel::new(
                clock.clone(),
                seed,
                Duration::from_millis(5),
                2,
                ge_wifi,
            );
            // LTE: 50ms, ~5% loss
            let ge_lte = GilbertElliottChannel::new(0.04, 0.3, 0.01, 0.45);
            let secondary = SimChannel::new(
                clock,
                seed + 1000,
                Duration::from_millis(50),
                8,
                ge_lte,
            );
            (primary, secondary, Duration::from_millis(5))
        }
    }
}

/// Create primary + secondary ReliableSimChannels for a scenario.
fn make_reliable_channels(
    scenario: ScenarioKind,
    clock: Arc<MockClock>,
    seed: u64,
) -> (ReliableSimChannel, ReliableSimChannel, Duration) {
    match scenario {
        ScenarioKind::DcLowLoss => {
            let primary = ReliableSimChannel::datacenter(clock.clone(), seed);
            let secondary = ReliableSimChannel::new(
                clock,
                seed + 1000,
                Duration::from_millis(2),
                0,
                GilbertElliottChannel::new(0.0, 1.0, 0.001, 0.0),
                Duration::from_millis(4),
                5,
            );
            (primary, secondary, Duration::from_millis(1))
        }
        ScenarioKind::WiFiBursty => {
            let primary = ReliableSimChannel::wifi(clock.clone(), seed);
            let secondary = ReliableSimChannel::wifi(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(5))
        }
        ScenarioKind::LteHighRtt => {
            let primary = ReliableSimChannel::lte(clock.clone(), seed);
            let secondary = ReliableSimChannel::lte(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(20))
        }
        ScenarioKind::WiFiLteHetero => {
            let primary = ReliableSimChannel::wifi(clock.clone(), seed);
            let secondary = ReliableSimChannel::lte(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(5))
        }
        ScenarioKind::LossySatellite => {
            let primary = ReliableSimChannel::satellite(clock.clone(), seed);
            let secondary = ReliableSimChannel::satellite(clock, seed + 1000);
            (primary, secondary, Duration::from_millis(100))
        }
        ScenarioKind::WiFiLteAsymmetric => {
            // WiFi: 5ms, ~1% loss, 10ms retransmit
            let ge_wifi = GilbertElliottChannel::new(0.01, 0.5, 0.005, 0.15);
            let primary = ReliableSimChannel::new(
                clock.clone(),
                seed,
                Duration::from_millis(5),
                2,
                ge_wifi,
                Duration::from_millis(10),
                5,
            );
            // LTE: 50ms, ~5% loss, 100ms retransmit
            let ge_lte = GilbertElliottChannel::new(0.04, 0.3, 0.01, 0.45);
            let secondary = ReliableSimChannel::new(
                clock,
                seed + 1000,
                Duration::from_millis(50),
                8,
                ge_lte,
                Duration::from_millis(100),
                5,
            );
            (primary, secondary, Duration::from_millis(5))
        }
    }
}

// ---------------------------------------------------------------------------
// QUIC transport runners
// ---------------------------------------------------------------------------

fn run_quic_single(
    seed: u64,
    scenario: &Scenario,
) -> DeliveryMetrics {
    let clock = Arc::new(MockClock::new());
    let (mut primary, _secondary, primary_base) =
        make_reliable_channels(scenario.kind, clock.clone(), seed);

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let start_time = clock.now();

    let mut sym_idx: u32 = 0;
    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let symbol = WireSymbol {
                block_id: 0,
                payload_id: sym_idx,
                is_repair: false,
                data,
                backend: FecBackend::Rlc,
            };
            send_times.insert(sym_idx as u64, clock.now());
            primary.send(symbol);
            sym_idx += 1;
        }

        let step = primary_base.max(Duration::from_millis(5));
        clock.advance(step);

        for pkt in primary.deliver() {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    // Drain remaining in-flight
    for _ in 0..100 {
        clock.advance(primary_base.max(Duration::from_millis(10)));
        let delivered = primary.deliver();
        if delivered.is_empty() && primary.in_flight_count() == 0 {
            break;
        }
        for pkt in delivered {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    let end_time = clock.now();
    compute_metrics(
        &send_times,
        &deliver_times,
        &delivery_order,
        NUM_SYMBOLS,
        primary.total_transmissions(),
        primary.total_unique(),
        start_time,
        end_time,
    )
}

fn run_quic_dual_rr(
    seed: u64,
    scenario: &Scenario,
) -> DeliveryMetrics {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary, primary_base) =
        make_reliable_channels(scenario.kind, clock.clone(), seed);

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let start_time = clock.now();

    let mut sym_idx: u32 = 0;
    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        for i in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let symbol = WireSymbol {
                block_id: 0,
                payload_id: sym_idx,
                is_repair: false,
                data,
                backend: FecBackend::Rlc,
            };
            send_times.insert(sym_idx as u64, clock.now());
            // Round-robin: even on primary, odd on secondary
            if i % 2 == 0 {
                primary.send(symbol);
            } else {
                secondary.send(symbol);
            }
            sym_idx += 1;
        }

        let step = primary_base.max(Duration::from_millis(5));
        clock.advance(step);

        for pkt in primary.deliver() {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
        for pkt in secondary.deliver() {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    // Drain remaining
    for _ in 0..100 {
        clock.advance(primary_base.max(Duration::from_millis(10)));
        let d1 = primary.deliver();
        let d2 = secondary.deliver();
        if d1.is_empty() && d2.is_empty()
            && primary.in_flight_count() == 0
            && secondary.in_flight_count() == 0
        {
            break;
        }
        for pkt in d1.into_iter().chain(d2.into_iter()) {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    let end_time = clock.now();
    let total_tx = primary.total_transmissions() + secondary.total_transmissions();
    let total_unique = primary.total_unique() + secondary.total_unique();
    compute_metrics(
        &send_times,
        &deliver_times,
        &delivery_order,
        NUM_SYMBOLS,
        total_tx,
        total_unique,
        start_time,
        end_time,
    )
}

fn run_quic_dual_minrtt(
    seed: u64,
    scenario: &Scenario,
) -> DeliveryMetrics {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary, primary_base) =
        make_reliable_channels(scenario.kind, clock.clone(), seed);

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let start_time = clock.now();

    // Min-RTT: send on the path with lower base delay
    let use_primary = primary.base_delay() <= secondary.base_delay();

    let mut sym_idx: u32 = 0;
    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let symbol = WireSymbol {
                block_id: 0,
                payload_id: sym_idx,
                is_repair: false,
                data,
                backend: FecBackend::Rlc,
            };
            send_times.insert(sym_idx as u64, clock.now());
            if use_primary {
                primary.send(symbol);
            } else {
                secondary.send(symbol);
            }
            sym_idx += 1;
        }

        let step = primary_base.max(Duration::from_millis(5));
        clock.advance(step);

        for pkt in primary.deliver() {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
        for pkt in secondary.deliver() {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    // Drain remaining
    for _ in 0..100 {
        clock.advance(primary_base.max(Duration::from_millis(10)));
        let d1 = primary.deliver();
        let d2 = secondary.deliver();
        if d1.is_empty() && d2.is_empty()
            && primary.in_flight_count() == 0
            && secondary.in_flight_count() == 0
        {
            break;
        }
        for pkt in d1.into_iter().chain(d2.into_iter()) {
            let id = pkt.symbol.payload_id as u64;
            if !deliver_times.contains_key(&id) {
                deliver_times.insert(id, clock.now());
                delivery_order.push(id);
            }
        }
    }

    let end_time = clock.now();
    let total_tx = primary.total_transmissions() + secondary.total_transmissions();
    let total_unique = primary.total_unique() + secondary.total_unique();
    compute_metrics(
        &send_times,
        &deliver_times,
        &delivery_order,
        NUM_SYMBOLS,
        total_tx,
        total_unique,
        start_time,
        end_time,
    )
}

// ---------------------------------------------------------------------------
// Raptorpath FEC transport runners
// ---------------------------------------------------------------------------

fn run_raptorpath_single(
    seed: u64,
    scenario: &Scenario,
) -> DeliveryMetrics {
    let clock = Arc::new(MockClock::new());
    let (mut primary, _secondary, primary_base) =
        make_lossy_channels(scenario.kind, clock.clone(), seed);

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut estimator = LossEstimator::new();
    let mut fec_ctrl = FecRateController::new(
        1e-5,
        0.5,
        ProtocolHint::Realtime,
        FecBackend::Rlc,
    );

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let start_time = clock.now();

    let mut sym_idx: u32 = 0;
    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);
        let mut batch_survived = 0u32;

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            send_times.insert(sym_idx as u64, clock.now());
            let sym = encoder.add_source(&data);

            if primary.send(sym) {
                batch_survived += 1;
            }
            sym_idx += 1;
        }
        total_source_sent += this_batch;

        // Adaptive repair
        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).min(10);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            primary.send(repair);
            total_repair_sent += 1;
        }

        let step = primary_base.max(Duration::from_millis(5));
        clock.advance(step);

        // Deliver and decode
        let now = clock.now();
        for pkt in primary.deliver() {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    if !deliver_times.contains_key(&rseq) {
                        deliver_times.insert(rseq, now);
                        delivery_order.push(rseq);
                    }
                }
            }
        }

        // Drain expired from reorder buffer
        for (seq, _) in reorder_buf.drain_expired(now) {
            if !deliver_times.contains_key(&seq) {
                deliver_times.insert(seq, now);
                delivery_order.push(seq);
            }
        }

        // Update estimator and FEC controller
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(primary_base);
        let batch_ok = batch_survived == this_batch;
        fec_ctrl.feedback_update(batch_ok);
    }

    // Drain remaining
    for _ in 0..100 {
        clock.advance(primary_base.max(Duration::from_millis(10)));
        let now = clock.now();
        let delivered = primary.deliver();
        if delivered.is_empty() && primary.in_flight_count() == 0 {
            break;
        }
        for pkt in delivered {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    if !deliver_times.contains_key(&rseq) {
                        deliver_times.insert(rseq, now);
                        delivery_order.push(rseq);
                    }
                }
            }
        }
        for (seq, _) in reorder_buf.drain_expired(now) {
            if !deliver_times.contains_key(&seq) {
                deliver_times.insert(seq, now);
                delivery_order.push(seq);
            }
        }
    }

    let end_time = clock.now();
    let total_tx = (total_source_sent + total_repair_sent) as u64;
    compute_metrics(
        &send_times,
        &deliver_times,
        &delivery_order,
        NUM_SYMBOLS,
        total_tx,
        total_source_sent as u64,
        start_time,
        end_time,
    )
}

fn run_raptorpath_dual(
    seed: u64,
    scenario: &Scenario,
) -> DeliveryMetrics {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary, primary_base) =
        make_lossy_channels(scenario.kind, clock.clone(), seed);

    let mut sched = Scheduler::new_with_config(clock.clone(), true);
    let primary_id: u32 = 1;
    let secondary_id: u32 = 2;
    sched.add_path(primary_id);
    sched.add_path(secondary_id);

    // Warmup scheduler paths
    for &id in &[primary_id, secondary_id] {
        let path = sched.path_mut(id).unwrap();
        path.cwnd = 200;
        path.in_slow_start = false;
        for _ in 0..20 {
            path.estimator.record_rtt(Duration::from_millis(5));
            path.record_rtt_sample(Duration::from_millis(5));
            path.estimator.record_throughput(100_000_000.0);
            path.estimator.record_batch(100, 98);
        }
    }

    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(25, 500);
    let mut estimator = LossEstimator::new();
    let mut fec_ctrl = FecRateController::new(
        1e-5,
        0.5,
        ProtocolHint::Realtime,
        FecBackend::Rlc,
    );

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let start_time = clock.now();

    let mut sym_idx: u32 = 0;
    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);
        let mut batch_survived = 0u32;
        let mut batch_dropped = 0u32;

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            send_times.insert(sym_idx as u64, clock.now());
            let sym = encoder.add_source(&data);

            // Send on primary
            if primary.send(sym.clone()) {
                batch_survived += 1;
            } else {
                batch_dropped += 1;
            }
            // Redundant send on secondary
            secondary.send(sym);

            sym_idx += 1;
        }
        total_source_sent += this_batch;

        // Adaptive repair on primary
        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).min(10);
        for _ in 0..repair_count {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            primary.send(repair);
            total_repair_sent += 1;
        }

        let step = primary_base.max(Duration::from_millis(5));
        clock.advance(step);

        // Deliver from both channels
        let now = clock.now();
        let all_delivered: Vec<SimPacket> = primary
            .deliver()
            .into_iter()
            .chain(secondary.deliver().into_iter())
            .collect();

        for pkt in &all_delivered {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    if !deliver_times.contains_key(&rseq) {
                        deliver_times.insert(rseq, now);
                        delivery_order.push(rseq);
                    }
                }
            }
        }

        for (seq, _) in reorder_buf.drain_expired(now) {
            if !deliver_times.contains_key(&seq) {
                deliver_times.insert(seq, now);
                delivery_order.push(seq);
            }
        }

        // Feed scheduler
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(primary_base);

        sched.ack(primary_id, batch_survived);
        if let Some(path) = sched.path_mut(primary_id) {
            path.estimator.record_rtt(primary_base);
            path.record_rtt_sample(primary_base);
            path.estimator.record_batch(this_batch, batch_survived);
        }
        sched.ack(secondary_id, this_batch);
        if let Some(path) = sched.path_mut(secondary_id) {
            path.estimator.record_rtt(primary_base * 2);
            path.record_rtt_sample(primary_base * 2);
            path.estimator.record_batch(this_batch, this_batch);
        }
        if batch_dropped > 0 {
            let fec_ok = (batch_dropped as f64 / this_batch as f64) < 0.20;
            sched.on_loss(primary_id, fec_ok);
        }

        let batch_ok = batch_dropped == 0;
        fec_ctrl.feedback_update(batch_ok);
    }

    // Drain remaining
    for _ in 0..100 {
        clock.advance(primary_base.max(Duration::from_millis(10)));
        let now = clock.now();
        let d1 = primary.deliver();
        let d2 = secondary.deliver();
        if d1.is_empty() && d2.is_empty()
            && primary.in_flight_count() == 0
            && secondary.in_flight_count() == 0
        {
            break;
        }
        for pkt in d1.into_iter().chain(d2.into_iter()) {
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    if !deliver_times.contains_key(&rseq) {
                        deliver_times.insert(rseq, now);
                        delivery_order.push(rseq);
                    }
                }
            }
        }
        for (seq, _) in reorder_buf.drain_expired(now) {
            if !deliver_times.contains_key(&seq) {
                deliver_times.insert(seq, now);
                delivery_order.push(seq);
            }
        }
    }

    let end_time = clock.now();
    let total_tx = (total_source_sent + total_repair_sent) as u64;
    compute_metrics(
        &send_times,
        &deliver_times,
        &delivery_order,
        NUM_SYMBOLS,
        total_tx,
        total_source_sent as u64,
        start_time,
        end_time,
    )
}

// ---------------------------------------------------------------------------
// Metrics computation
// ---------------------------------------------------------------------------

fn compute_metrics(
    send_times: &HashMap<u64, Instant>,
    deliver_times: &HashMap<u64, Instant>,
    delivery_order: &[u64],
    num_symbols: u32,
    total_transmissions: u64,
    total_unique: u64,
    start_time: Instant,
    end_time: Instant,
) -> DeliveryMetrics {
    let recovered = deliver_times.len() as u32;
    let recovery_rate = recovered as f64 / num_symbols as f64;

    // Goodput: unique symbols delivered / total transmissions
    let goodput_ratio = if total_transmissions > 0 {
        recovered as f64 / total_transmissions as f64
    } else {
        0.0
    };

    // Latency percentiles
    let mut latencies_ms: Vec<f64> = Vec::new();
    for (id, &deliver_time) in deliver_times {
        if let Some(&send_time) = send_times.get(id) {
            let lat = deliver_time.duration_since(send_time);
            latencies_ms.push(lat.as_secs_f64() * 1000.0);
        }
    }
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let latency_p50_ms = percentile_ms(&latencies_ms, 0.50);
    let latency_p95_ms = percentile_ms(&latencies_ms, 0.95);
    let latency_p99_ms = percentile_ms(&latencies_ms, 0.99);

    // Completion time
    let completion_time_ms = end_time.duration_since(start_time).as_secs_f64() * 1000.0;

    // Overhead: extra transmissions beyond unique symbols
    let overhead_pct = if total_unique > 0 {
        (total_transmissions as f64 - total_unique as f64) / total_unique as f64 * 100.0
    } else {
        0.0
    };

    let in_order_rate = compute_in_order_rate(delivery_order);

    DeliveryMetrics {
        recovery_rate,
        goodput_ratio,
        latency_p50_ms,
        latency_p95_ms,
        latency_p99_ms,
        completion_time_ms,
        overhead_pct,
        in_order_rate,
    }
}

// ---------------------------------------------------------------------------
// Single trial dispatcher
// ---------------------------------------------------------------------------

fn run_trial(
    seed: u64,
    scenario: &Scenario,
    transport: &TransportConfig,
) -> DeliveryMetrics {
    match transport.kind {
        TransportKind::QuicSingle => run_quic_single(seed, scenario),
        TransportKind::QuicDualRR => run_quic_dual_rr(seed, scenario),
        TransportKind::QuicDualMinRtt => run_quic_dual_minrtt(seed, scenario),
        TransportKind::RaptorpathSingle => run_raptorpath_single(seed, scenario),
        TransportKind::RaptorpathDual => run_raptorpath_dual(seed, scenario),
    }
}

// ---------------------------------------------------------------------------
// Averaging over trials
// ---------------------------------------------------------------------------

fn run_averaged(
    scenario: &Scenario,
    transport: &TransportConfig,
) -> DeliveryMetrics {
    let mut sum = DeliveryMetrics::zero();

    for trial in 0..NUM_TRIALS {
        let r = run_trial(trial * 137 + 42, scenario, transport);
        sum.add(&r);
    }

    sum.div(NUM_TRIALS as f64);
    sum
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn print_header() {
    println!(
        "| {:20} | {:>8} | {:>7} | {:>7} | {:>7} | {:>7} | {:>9} | {:>8} | {:>8} |",
        "Transport", "Recovery", "Goodput", "p50 ms", "p95 ms", "p99 ms", "Compl ms", "Overhead", "InOrder"
    );
    println!(
        "|{:-<22}|{:-<10}|{:-<9}|{:-<9}|{:-<9}|{:-<9}|{:-<11}|{:-<10}|{:-<10}|",
        "", "", "", "", "", "", "", "", ""
    );
}

fn print_row(name: &str, m: &DeliveryMetrics) {
    println!(
        "| {:20} | {:>7.1}% | {:>7.3} | {:>7.1} | {:>7.1} | {:>7.1} | {:>9.1} | {:>7.1}% | {:>7.1}% |",
        name,
        m.recovery_rate * 100.0,
        m.goodput_ratio,
        m.latency_p50_ms,
        m.latency_p95_ms,
        m.latency_p99_ms,
        m.completion_time_ms,
        m.overhead_pct,
        m.in_order_rate * 100.0,
    );
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
fn transport_comparison_benchmark() {
    let all_scenarios = scenarios();
    let all_transports = transport_configs();

    println!(
        "\n## Transport Comparison Results ({} trials, {} symbols per trial)\n",
        NUM_TRIALS, NUM_SYMBOLS
    );

    // Cache all results to avoid re-running
    let mut all_results: Vec<Vec<(&str, DeliveryMetrics)>> = Vec::new();

    for scenario in &all_scenarios {
        println!("### {} scenario\n", scenario.name);
        print_header();

        let mut scenario_results = Vec::new();
        for transport in &all_transports {
            let result = run_averaged(scenario, transport);
            print_row(transport.name, &result);
            scenario_results.push((transport.name, result));
        }
        println!();
        all_results.push(scenario_results);
    }

    // Summary: identify best transport per scenario per metric
    println!("### Summary\n");
    println!(
        "| {:20} | {:20} | {:20} | {:20} |",
        "Scenario", "Best Recovery", "Best p99 Latency", "Best Goodput"
    );
    println!(
        "|{:-<22}|{:-<22}|{:-<22}|{:-<22}|",
        "", "", "", ""
    );

    for (i, scenario) in all_scenarios.iter().enumerate() {
        let results = &all_results[i];

        let best_recovery = results
            .iter()
            .max_by(|a, b| a.1.recovery_rate.partial_cmp(&b.1.recovery_rate).unwrap())
            .map(|(name, _)| *name)
            .unwrap_or("?");

        let best_p99 = results
            .iter()
            .min_by(|a, b| a.1.latency_p99_ms.partial_cmp(&b.1.latency_p99_ms).unwrap())
            .map(|(name, _)| *name)
            .unwrap_or("?");

        let best_goodput = results
            .iter()
            .max_by(|a, b| a.1.goodput_ratio.partial_cmp(&b.1.goodput_ratio).unwrap())
            .map(|(name, _)| *name)
            .unwrap_or("?");

        println!(
            "| {:20} | {:20} | {:20} | {:20} |",
            scenario.name, best_recovery, best_p99, best_goodput
        );
    }

    println!();
    println!("Transport configs: quic_single (reliable retransmit, 1 path),");
    println!("  quic_dual_rr (reliable, 2 paths round-robin),");
    println!("  quic_dual_minrtt (reliable, 2 paths min-RTT selection),");
    println!("  raptorpath_single (FEC+lossy, 1 path),");
    println!("  raptorpath_dual (FEC+lossy, 2 paths multipath scheduler).");
}
