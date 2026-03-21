//! Consolidated benchmark suite (ADR-0042, ADR-0045).
//!
//! Tables 1/1b (codec recovery sweep) and Table 2 (overhead breakdown) remain unchanged.
//! Tables 3/4/5 replaced by unified matrix: 6 backends × 4 configs × 2 paths × 5 scenarios.
//!
//! Output: markdown + JSON files with git commit info.
//! Run with: cargo test --test bench_suite -- --nocapture --release

mod common;

use common::*;
use mettle::MettleConfig;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};
use raptorpath::fec::{
    EncodingParams, FecBackend, FecDecoder, MettleWindowDecoder, MettleWindowEncoder,
    RlcWindowDecoder, RlcWindowEncoder, StreamingDecoder, StreamingEncoder, WindowDecoder,
    WindowEncoder, WireSymbol,
};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::net::{compute_gap_ranges, MAX_NACK_GAPS};
use raptorpath::scheduler::{Clock, MockClock, Scheduler};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Shared parameters
// ---------------------------------------------------------------------------

const SYMBOL_SIZE: u16 = 1200;
const NUM_SYMBOLS: u32 = 2000;
const BATCH_SIZE: u32 = 10;
const NUM_TRIALS: u64 = 30;
const MAX_FEC_OVERHEAD: f64 = 0.20;
const BLOCK_SIZE: u32 = 200;
const MATRIX_FEC_OVERHEAD: f64 = 0.08;

// ---------------------------------------------------------------------------
// BackendChoice — replaces WindowBackendKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
enum BackendChoice {
    RaptorQ,
    ReedSolomon,
    Rlc,
    Mettle,
    Streaming,
    Retransmit,
}

impl BackendChoice {
    fn all() -> &'static [BackendChoice] {
        &[
            Self::RaptorQ,
            Self::ReedSolomon,
            Self::Rlc,
            Self::Mettle,
            Self::Streaming,
            Self::Retransmit,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            Self::RaptorQ => "RaptorQ",
            Self::ReedSolomon => "ReedSolomon",
            Self::Rlc => "RLC",
            Self::Mettle => "Mettle",
            Self::Streaming => "Streaming",
            Self::Retransmit => "Retransmit",
        }
    }

    fn is_block(&self) -> bool {
        matches!(self, Self::RaptorQ | Self::ReedSolomon)
    }

    fn is_window(&self) -> bool {
        matches!(self, Self::Rlc | Self::Mettle | Self::Streaming)
    }

    fn is_retransmit(&self) -> bool {
        matches!(self, Self::Retransmit)
    }

    fn fec_backend(&self) -> FecBackend {
        match self {
            Self::RaptorQ => FecBackend::RaptorQ,
            Self::ReedSolomon => FecBackend::ReedSolomon,
            Self::Rlc => FecBackend::Rlc,
            Self::Mettle => FecBackend::Mettle,
            Self::Streaming => FecBackend::Streaming,
            Self::Retransmit => unreachable!("Retransmit has no FecBackend"),
        }
    }

    fn create_window_encoder(
        &self,
        symbol_size: u16,
        ctrl: &FecRateController,
        estimator: &LossEstimator,
    ) -> Box<dyn WindowEncoder> {
        match self {
            Self::Rlc => Box::new(RlcWindowEncoder::new(symbol_size)),
            Self::Mettle => Box::new(MettleWindowEncoder::new(
                MettleConfig::small_window(),
                symbol_size,
                42,
            )),
            Self::Streaming => {
                let params = ctrl.compute_streaming_params(estimator);
                Box::new(StreamingEncoder::new(symbol_size, params))
            }
            _ => unreachable!("{:?} is not a window backend", self),
        }
    }

    fn create_window_decoder(
        &self,
        symbol_size: u16,
        ctrl: &FecRateController,
        estimator: &LossEstimator,
    ) -> Box<dyn WindowDecoder> {
        match self {
            Self::Rlc => Box::new(RlcWindowDecoder::new(symbol_size)),
            Self::Mettle => Box::new(MettleWindowDecoder::new(symbol_size)),
            Self::Streaming => {
                let params = ctrl.compute_streaming_params(estimator);
                Box::new(StreamingDecoder::new(symbol_size, params))
            }
            _ => unreachable!("{:?} is not a window backend", self),
        }
    }
}

// ---------------------------------------------------------------------------
// ScenarioConfig
// ---------------------------------------------------------------------------

struct ScenarioConfig {
    name: &'static str,
    base_delay_ms: u64,
    pre_warm_loss: f64,
    paths_supported: &'static [u32],
}

fn scenario_configs() -> Vec<ScenarioConfig> {
    vec![
        ScenarioConfig {
            name: "DC",
            base_delay_ms: 1,
            pre_warm_loss: 0.001,
            paths_supported: &[1, 2],
        },
        ScenarioConfig {
            name: "WiFi",
            base_delay_ms: 5,
            pre_warm_loss: 0.025,
            paths_supported: &[1, 2],
        },
        ScenarioConfig {
            name: "LTE",
            base_delay_ms: 20,
            pre_warm_loss: 0.035,
            paths_supported: &[1, 2],
        },
        ScenarioConfig {
            name: "Satellite",
            base_delay_ms: 100,
            pre_warm_loss: 0.09,
            paths_supported: &[1, 2],
        },
        ScenarioConfig {
            name: "DC+LTE",
            base_delay_ms: 1,
            pre_warm_loss: 0.02,
            paths_supported: &[2],
        },
    ]
}

fn make_sim_channels(
    scenario: &str,
    num_paths: u32,
    clock: Arc<MockClock>,
    seed: u64,
) -> (SimChannel, Option<SimChannel>) {
    let secondary = num_paths >= 2;
    match scenario {
        "DC" => (
            SimChannel::datacenter(clock.clone(), seed),
            if secondary {
                Some(SimChannel::datacenter(clock, seed + 1000))
            } else {
                None
            },
        ),
        "WiFi" => (
            SimChannel::wifi_congested(clock.clone(), seed),
            if secondary {
                Some(SimChannel::wifi_congested(clock, seed + 1000))
            } else {
                None
            },
        ),
        "LTE" => (
            SimChannel::lte_congested(clock.clone(), seed),
            if secondary {
                Some(SimChannel::lte_congested(clock, seed + 1000))
            } else {
                None
            },
        ),
        "Satellite" => (
            SimChannel::satellite(clock.clone(), seed),
            if secondary {
                Some(SimChannel::satellite(clock, seed + 1000))
            } else {
                None
            },
        ),
        "DC+LTE" => {
            assert!(secondary, "DC+LTE only supports 2 paths");
            (
                SimChannel::datacenter(clock.clone(), seed),
                Some(SimChannel::lte_congested(clock, seed + 1000)),
            )
        }
        _ => unreachable!("unknown scenario: {}", scenario),
    }
}

fn make_reliable_channels_for_scenario(
    scenario: &str,
    num_paths: u32,
    clock: Arc<MockClock>,
    seed: u64,
) -> (ReliableSimChannel, Option<ReliableSimChannel>) {
    let secondary = num_paths >= 2;
    match scenario {
        "DC" => (
            ReliableSimChannel::datacenter(clock.clone(), seed),
            if secondary {
                Some(ReliableSimChannel::datacenter(clock, seed + 1000))
            } else {
                None
            },
        ),
        "WiFi" => (
            ReliableSimChannel::wifi_congested(clock.clone(), seed),
            if secondary {
                Some(ReliableSimChannel::wifi_congested(clock, seed + 1000))
            } else {
                None
            },
        ),
        "LTE" => (
            ReliableSimChannel::lte_congested(clock.clone(), seed),
            if secondary {
                Some(ReliableSimChannel::lte_congested(clock, seed + 1000))
            } else {
                None
            },
        ),
        "Satellite" => (
            ReliableSimChannel::satellite(clock.clone(), seed),
            if secondary {
                Some(ReliableSimChannel::satellite(clock, seed + 1000))
            } else {
                None
            },
        ),
        "DC+LTE" => {
            assert!(secondary, "DC+LTE only supports 2 paths");
            (
                ReliableSimChannel::datacenter(clock.clone(), seed),
                Some(ReliableSimChannel::lte_congested(clock, seed + 1000)),
            )
        }
        _ => unreachable!("unknown scenario: {}", scenario),
    }
}

// ---------------------------------------------------------------------------
// AblationConfig (matrix feature configs)
// ---------------------------------------------------------------------------

struct AblationConfig {
    name: &'static str,
    enable_nack: bool,
    reorder_timeout_ms: u64,
    enable_pi: bool,
}

fn ablation_configs() -> Vec<AblationConfig> {
    vec![
        AblationConfig {
            name: "baseline",
            enable_nack: true,
            reorder_timeout_ms: 25,
            enable_pi: true,
        },
        AblationConfig {
            name: "no_nack",
            enable_nack: false,
            reorder_timeout_ms: 25,
            enable_pi: true,
        },
        AblationConfig {
            name: "no_reorder",
            enable_nack: true,
            reorder_timeout_ms: 0,
            enable_pi: true,
        },
        AblationConfig {
            name: "no_pi",
            enable_nack: true,
            reorder_timeout_ms: 25,
            enable_pi: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// TrialResult — unified per-trial output (10 metrics)
// ---------------------------------------------------------------------------

struct TrialResult {
    throughput_mbps: f64,
    recovery_rate: f64,
    overhead_pct: f64,
    total_repair_count: u32,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    deadline_miss_pct: f64,
    in_order_rate: f64,
    tail_drops: u64,
}

// ---------------------------------------------------------------------------
// Serializable output structs
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct MetricStats {
    mean: f64,
    stddev: f64,
    ci95: f64,
}

#[derive(serde::Serialize)]
struct MatrixCell {
    backend: String,
    scenario: String,
    config: String,
    paths: u32,
    metrics: BTreeMap<String, MetricStats>,
}

#[derive(serde::Serialize)]
struct LossSweepRow {
    loss_pct: f64,
    backend: String,
    recovery: MetricStats,
}

#[derive(serde::Serialize)]
struct OverheadRow {
    layer: String,
    values: BTreeMap<String, f64>,
}

#[derive(serde::Serialize)]
struct BenchmarkParameters {
    symbol_size: u16,
    num_symbols: u32,
    batch_size: u32,
    num_trials: u64,
    fec_overhead: f64,
    block_size: u32,
}

#[derive(serde::Serialize)]
struct BenchmarkOutput {
    commit_hash: String,
    commit_message: String,
    timestamp: String,
    parameters: BenchmarkParameters,
    table1_uniform: Vec<LossSweepRow>,
    table1b_bursty: Vec<LossSweepRow>,
    table2_overhead: Vec<OverheadRow>,
    matrix: Vec<MatrixCell>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_estimator_for_loss(loss_rate: f64) -> LossEstimator {
    let mut estimator = LossEstimator::new();
    let batch = 1000u32;
    let received = ((1.0 - loss_rate) * batch as f64) as u32;
    for _ in 0..50 {
        estimator.record_batch(batch, received);
        estimator.record_rtt(Duration::from_millis(5));
        estimator.record_throughput(100_000_000.0);
    }
    estimator
}

fn percentile_ms(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn compute_in_order_rate(delivery_order: &[u64]) -> f64 {
    if delivery_order.len() <= 1 {
        return 1.0;
    }
    // RFC 4737: packet is in-order if seq > max_seen_so_far
    let mut max_seen = delivery_order[0];
    let mut in_order = 0u64;
    for &seq in &delivery_order[1..] {
        if seq > max_seen {
            in_order += 1;
        }
        max_seen = max_seen.max(seq);
    }
    in_order as f64 / (delivery_order.len() - 1) as f64
}

fn stats_to_metric(s: &TrialStats) -> MetricStats {
    MetricStats {
        mean: s.mean(),
        stddev: s.stddev(),
        ci95: s.ci95(),
    }
}

/// Compute BDP-based cwnd per path for a scenario.
/// Returns (primary_cwnd, secondary_cwnd). usize::MAX means no pacing.
fn scenario_cwnd(scenario: &ScenarioConfig) -> (usize, usize) {
    let symbol_wire_size = SYMBOL_SIZE as f64 + 25.0;
    match scenario.name {
        "WiFi" => {
            // 10 Mbps, 5ms base delay
            let bps = 10_000_000.0 / 8.0;
            let delay = scenario.base_delay_ms as f64 / 1000.0;
            let cwnd = ((bps * delay / symbol_wire_size) as usize).max(2);
            (cwnd, cwnd)
        }
        "LTE" => {
            // 2 Mbps, 20ms base delay
            let bps = 2_000_000.0 / 8.0;
            let delay = scenario.base_delay_ms as f64 / 1000.0;
            let cwnd = ((bps * delay / symbol_wire_size) as usize).max(2);
            (cwnd, cwnd)
        }
        "DC+LTE" => {
            // Primary=DC (no link), Secondary=LTE congested
            let bps = 2_000_000.0 / 8.0;
            let delay = 0.02; // LTE base delay
            let sec_cwnd = ((bps * delay / symbol_wire_size) as usize).max(2);
            (usize::MAX, sec_cwnd) // DC has no congestion
        }
        _ => (usize::MAX, usize::MAX), // DC, Satellite: no link-model pacing
    }
}

fn fec_backend_name(b: FecBackend) -> &'static str {
    match b {
        FecBackend::RaptorQ => "RaptorQ",
        FecBackend::ReedSolomon => "ReedSolomon",
        FecBackend::Rlc => "RLC",
        FecBackend::Mettle => "Mettle",
        FecBackend::Streaming => "Streaming",
    }
}

// ===========================================================================
// Table 1: Backend Loss Sweep
// ===========================================================================

fn loss_sweep_block_trial(backend: FecBackend, loss_rate: f64, seed: u64) -> f64 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let num_blocks = NUM_SYMBOLS / BLOCK_SIZE;
    let mut repair_per_block = (BLOCK_SIZE as f64 * MAX_FEC_OVERHEAD).ceil() as u32;
    if backend == FecBackend::ReedSolomon {
        repair_per_block = repair_per_block.min(255 - BLOCK_SIZE);
    }

    let mut total_recovered = 0u32;

    for block_idx in 0..num_blocks {
        let block_data =
            vec![(block_idx % 256) as u8; BLOCK_SIZE as usize * SYMBOL_SIZE as usize];

        let params = EncodingParams {
            source_symbols: BLOCK_SIZE,
            symbol_size: SYMBOL_SIZE,
            repair_count: repair_per_block,
            block_id: block_idx as u64,
        };

        let encoder = backend.create_encoder(&block_data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_per_block);

        let mut all_syms: Vec<WireSymbol> = source;
        all_syms.extend(repairs);
        let surviving: Vec<WireSymbol> = all_syms
            .into_iter()
            .filter(|_| rng.gen::<f64>() >= loss_rate)
            .collect();

        let mut decoder = backend.create_decoder(params, block_data.len() as u64);
        let mut decoded = false;
        for sym in &surviving {
            if decoder.add_symbol(sym).is_some() {
                decoded = true;
                break;
            }
        }

        if decoded {
            total_recovered += BLOCK_SIZE;
        }
    }

    total_recovered as f64 / NUM_SYMBOLS as f64 * 100.0
}

fn loss_sweep_window_trial(backend: BackendChoice, loss_rate: f64, seed: u64) -> f64 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let estimator = make_estimator_for_loss(loss_rate);
    let ctrl = FecRateController::new(
        1e-5,
        MAX_FEC_OVERHEAD,
        ProtocolHint::Realtime,
        backend.fec_backend(),
        SYMBOL_SIZE,
    );

    let mut encoder = backend.create_window_encoder(SYMBOL_SIZE, &ctrl, &estimator);
    let mut decoder = backend.create_window_decoder(SYMBOL_SIZE, &ctrl, &estimator);
    let mut recovered = BTreeSet::new();

    let repair_per_batch = (BATCH_SIZE as f64 * MAX_FEC_OVERHEAD).ceil() as u32;
    let mut sym_idx: u32 = 0;

    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        let mut batch_syms: Vec<WireSymbol> = Vec::new();
        for _ in 0..this_batch {
            let data = vec![(sym_idx % 256) as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            batch_syms.push(sym);
            sym_idx += 1;
        }

        for _ in 0..repair_per_batch {
            if encoder.window_size() == 0 {
                break;
            }
            batch_syms.push(encoder.generate_repair());
        }

        let surviving: Vec<WireSymbol> = batch_syms
            .into_iter()
            .filter(|_| rng.gen::<f64>() >= loss_rate)
            .collect();

        for sym in &surviving {
            for (seq, _) in decoder.add_symbol(sym) {
                recovered.insert(seq);
            }
        }
    }

    recovered.len() as f64 / NUM_SYMBOLS as f64 * 100.0
}

fn loss_sweep_block_ge_trial(backend: FecBackend, target_loss: f64, seed: u64) -> f64 {
    let mut ge = ge_for_target_loss(target_loss, 3.0);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let num_blocks = NUM_SYMBOLS / BLOCK_SIZE;
    let mut repair_per_block = (BLOCK_SIZE as f64 * MAX_FEC_OVERHEAD).ceil() as u32;
    if backend == FecBackend::ReedSolomon {
        repair_per_block = repair_per_block.min(255 - BLOCK_SIZE);
    }

    let mut total_recovered = 0u32;

    for block_idx in 0..num_blocks {
        let block_data =
            vec![(block_idx % 256) as u8; BLOCK_SIZE as usize * SYMBOL_SIZE as usize];

        let params = EncodingParams {
            source_symbols: BLOCK_SIZE,
            symbol_size: SYMBOL_SIZE,
            repair_count: repair_per_block,
            block_id: block_idx as u64,
        };

        let encoder = backend.create_encoder(&block_data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_per_block);

        let mut all_syms: Vec<WireSymbol> = source;
        all_syms.extend(repairs);
        let surviving: Vec<WireSymbol> = all_syms
            .into_iter()
            .filter(|_| !ge.should_drop(&mut rng))
            .collect();

        let mut decoder = backend.create_decoder(params, block_data.len() as u64);
        let mut decoded = false;
        for sym in &surviving {
            if decoder.add_symbol(sym).is_some() {
                decoded = true;
                break;
            }
        }

        if decoded {
            total_recovered += BLOCK_SIZE;
        }
    }

    total_recovered as f64 / NUM_SYMBOLS as f64 * 100.0
}

fn loss_sweep_window_ge_trial(backend: BackendChoice, target_loss: f64, seed: u64) -> f64 {
    let clock = Arc::new(MockClock::new());
    let ge = ge_for_target_loss(target_loss, 3.0);
    let mut channel = SimChannel::new(clock.clone(), seed, Duration::ZERO, 0, ge);

    let estimator = make_estimator_for_loss(target_loss);
    let ctrl = FecRateController::new(
        1e-5,
        MAX_FEC_OVERHEAD,
        ProtocolHint::Realtime,
        backend.fec_backend(),
        SYMBOL_SIZE,
    );

    let mut encoder = backend.create_window_encoder(SYMBOL_SIZE, &ctrl, &estimator);
    let mut decoder = backend.create_window_decoder(SYMBOL_SIZE, &ctrl, &estimator);
    let mut recovered = BTreeSet::new();

    let repair_per_batch = (BATCH_SIZE as f64 * MAX_FEC_OVERHEAD).ceil() as u32;
    let mut sym_idx: u32 = 0;

    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        for _ in 0..this_batch {
            let data = vec![(sym_idx % 256) as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);
            channel.send(sym);
            sym_idx += 1;
        }

        for _ in 0..repair_per_batch {
            if encoder.window_size() == 0 {
                break;
            }
            channel.send(encoder.generate_repair());
        }

        clock.advance(Duration::from_millis(1));
        for pkt in channel.deliver() {
            for (seq, _) in decoder.add_symbol(&pkt.symbol) {
                recovered.insert(seq);
            }
        }
    }

    recovered.len() as f64 / NUM_SYMBOLS as f64 * 100.0
}

fn run_loss_sweep_table(
    title: &str,
    loss_rates: &[f64],
    use_ge: bool,
) -> (String, Vec<LossSweepRow>) {
    let mut text = String::new();
    let mut rows = Vec::new();

    text.push_str(&format!("{}\n\n", title));

    // Header
    text.push_str(&format!("| {:>6} ", "Loss %"));
    for name in &["RaptorQ", "RS", "RLC-win", "Mettle-win", "Streaming"] {
        text.push_str(&format!("| {:>16} ", name));
    }
    text.push_str("|\n");

    text.push_str(&format!("|{:-<8}", ""));
    for _ in 0..5 {
        text.push_str(&format!("|{:-<18}", ""));
    }
    text.push_str("|\n");

    for &loss in loss_rates {
        text.push_str(&format!("| {:>5.0}% ", loss * 100.0));

        // Block backends
        for &backend in &[FecBackend::RaptorQ, FecBackend::ReedSolomon] {
            let mut stats = TrialStats::new();
            for trial in 0..NUM_TRIALS {
                if use_ge {
                    stats.push(loss_sweep_block_ge_trial(backend, loss, trial * 137 + 42));
                } else {
                    stats.push(loss_sweep_block_trial(backend, loss, trial * 137 + 42));
                }
            }
            text.push_str(&format!("| {:>16} ", stats.fmt_ci()));
            rows.push(LossSweepRow {
                loss_pct: loss * 100.0,
                backend: fec_backend_name(backend).to_string(),
                recovery: stats_to_metric(&stats),
            });
        }

        // Window backends
        for &kind in &[BackendChoice::Rlc, BackendChoice::Mettle, BackendChoice::Streaming] {
            let mut stats = TrialStats::new();
            for trial in 0..NUM_TRIALS {
                if use_ge {
                    stats.push(loss_sweep_window_ge_trial(kind, loss, trial * 137 + 42));
                } else {
                    stats.push(loss_sweep_window_trial(kind, loss, trial * 137 + 42));
                }
            }
            text.push_str(&format!("| {:>16} ", stats.fmt_ci()));
            rows.push(LossSweepRow {
                loss_pct: loss * 100.0,
                backend: kind.name().to_string(),
                recovery: stats_to_metric(&stats),
            });
        }

        text.push_str("|\n");
    }
    text.push('\n');

    (text, rows)
}

fn table1_backend_loss_sweep() -> (String, Vec<LossSweepRow>, Vec<LossSweepRow>) {
    let loss_rates = [0.01, 0.02, 0.05, 0.08, 0.10, 0.15, 0.20, 0.25];

    let (text1, rows1) = run_loss_sweep_table(
        &format!(
            "\n## Table 1: Backend Recovery vs Loss Rate — Uniform ({}% FEC budget, {}B symbols, {} trials)",
            (MAX_FEC_OVERHEAD * 100.0) as u32,
            SYMBOL_SIZE,
            NUM_TRIALS
        ),
        &loss_rates,
        false,
    );

    let (text1b, rows1b) = run_loss_sweep_table(
        &format!(
            "## Table 1b: Backend Recovery vs Loss Rate — GE Bursty (mean burst ~3, {}% FEC, {} trials)",
            (MAX_FEC_OVERHEAD * 100.0) as u32,
            NUM_TRIALS
        ),
        &loss_rates,
        true,
    );

    let combined_text = format!("{}{}", text1, text1b);
    (combined_text, rows1, rows1b)
}

// ===========================================================================
// Table 2: Wire Overhead Breakdown
// ===========================================================================

fn table2_wire_overhead() -> (String, Vec<OverheadRow>) {
    let mut text = String::new();
    let mut rows = Vec::new();

    text.push_str(&format!(
        "## Table 2: Wire Overhead Breakdown ({} x {}B symbols)\n\n",
        NUM_SYMBOLS, SYMBOL_SIZE
    ));

    let scenarios = [
        ("DC (0.1%)", 0.001f64),
        ("WiFi (2.5%)", 0.025),
        ("Congested (12%)", 0.12),
    ];

    let per_symbol_meta: u64 = 25;
    let batch_header: u64 = 32;
    let repair_header: u64 = 14;
    let source_data = NUM_SYMBOLS as u64 * SYMBOL_SIZE as u64;

    // Header
    text.push_str(&format!("| {:<24} ", "Layer"));
    for &(name, _) in &scenarios {
        text.push_str(&format!("| {:>15} ", name));
    }
    text.push_str("|\n");

    text.push_str(&format!("|{:-<26}", ""));
    for _ in &scenarios {
        text.push_str(&format!("|{:-<17}", ""));
    }
    text.push_str("|\n");

    let mut totals = vec![0.0f64; scenarios.len()];

    // Layer 1: FEC repair symbols
    {
        let layer_name = "1. FEC repair symbols";
        text.push_str(&format!("| {:<24} ", layer_name));
        let mut vals = BTreeMap::new();
        for (i, &(name, loss_rate)) in scenarios.iter().enumerate() {
            let estimator = make_estimator_for_loss(loss_rate);
            let ctrl = FecRateController::new(
                1e-5,
                MAX_FEC_OVERHEAD,
                ProtocolHint::Realtime,
                FecBackend::Rlc,
                SYMBOL_SIZE,
            );
            let repair_rate = ctrl.compute_repair_rate(&estimator, NUM_SYMBOLS as usize);
            let repair_count = (NUM_SYMBOLS as f64 * repair_rate).ceil() as u64;
            let overhead = repair_count as f64 * SYMBOL_SIZE as f64 / source_data as f64 * 100.0;
            totals[i] += overhead;
            text.push_str(&format!("| {:>14.1}% ", overhead));
            vals.insert(name.to_string(), overhead);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: layer_name.to_string(),
            values: vals,
        });
    }

    // Layer 2: Symbol padding
    {
        let layer_name = "2. Symbol padding";
        text.push_str(&format!("| {:<24} ", layer_name));
        let mut vals = BTreeMap::new();
        for (i, &(name, _)) in scenarios.iter().enumerate() {
            let overhead = 0.0;
            totals[i] += overhead;
            text.push_str(&format!("| {:>14.1}% ", overhead));
            vals.insert(name.to_string(), overhead);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: layer_name.to_string(),
            values: vals,
        });
    }

    // Layer 3: Per-symbol metadata
    {
        let layer_name = "3. Per-symbol metadata";
        text.push_str(&format!("| {:<24} ", layer_name));
        let mut vals = BTreeMap::new();
        for (i, &(name, loss_rate)) in scenarios.iter().enumerate() {
            let estimator = make_estimator_for_loss(loss_rate);
            let ctrl = FecRateController::new(
                1e-5,
                MAX_FEC_OVERHEAD,
                ProtocolHint::Realtime,
                FecBackend::Rlc,
                SYMBOL_SIZE,
            );
            let repair_rate = ctrl.compute_repair_rate(&estimator, NUM_SYMBOLS as usize);
            let total_symbols = NUM_SYMBOLS as f64 * (1.0 + repair_rate);
            let overhead =
                total_symbols * per_symbol_meta as f64 / source_data as f64 * 100.0;
            totals[i] += overhead;
            text.push_str(&format!("| {:>14.1}% ", overhead));
            vals.insert(name.to_string(), overhead);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: layer_name.to_string(),
            values: vals,
        });
    }

    // Layer 4: Batch/wire header
    {
        let layer_name = "4. Batch/wire header";
        text.push_str(&format!("| {:<24} ", layer_name));
        let mut vals = BTreeMap::new();
        for (i, &(name, loss_rate)) in scenarios.iter().enumerate() {
            let estimator = make_estimator_for_loss(loss_rate);
            let ctrl = FecRateController::new(
                1e-5,
                MAX_FEC_OVERHEAD,
                ProtocolHint::Realtime,
                FecBackend::Rlc,
                SYMBOL_SIZE,
            );
            let repair_rate = ctrl.compute_repair_rate(&estimator, NUM_SYMBOLS as usize);
            let total_symbols = NUM_SYMBOLS as f64 * (1.0 + repair_rate);
            let num_batches = (total_symbols / BATCH_SIZE as f64).ceil();
            let overhead =
                num_batches * batch_header as f64 / source_data as f64 * 100.0;
            totals[i] += overhead;
            text.push_str(&format!("| {:>14.1}% ", overhead));
            vals.insert(name.to_string(), overhead);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: layer_name.to_string(),
            values: vals,
        });
    }

    // Layer 5: Repair metadata
    {
        let layer_name = "5. Repair metadata";
        text.push_str(&format!("| {:<24} ", layer_name));
        let mut vals = BTreeMap::new();
        for (i, &(name, loss_rate)) in scenarios.iter().enumerate() {
            let estimator = make_estimator_for_loss(loss_rate);
            let ctrl = FecRateController::new(
                1e-5,
                MAX_FEC_OVERHEAD,
                ProtocolHint::Realtime,
                FecBackend::Rlc,
                SYMBOL_SIZE,
            );
            let repair_rate = ctrl.compute_repair_rate(&estimator, NUM_SYMBOLS as usize);
            let repair_count = (NUM_SYMBOLS as f64 * repair_rate).ceil();
            let overhead =
                repair_count * repair_header as f64 / source_data as f64 * 100.0;
            totals[i] += overhead;
            text.push_str(&format!("| {:>14.1}% ", overhead));
            vals.insert(name.to_string(), overhead);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: layer_name.to_string(),
            values: vals,
        });
    }

    // Total row
    {
        text.push_str(&format!("| **{:<22}** ", "Total"));
        let mut vals = BTreeMap::new();
        for (i, &(name, _)) in scenarios.iter().enumerate() {
            text.push_str(&format!("| **{:>12.1}%** ", totals[i]));
            vals.insert(name.to_string(), totals[i]);
        }
        text.push_str("|\n");
        rows.push(OverheadRow {
            layer: "Total".to_string(),
            values: vals,
        });
    }

    text.push('\n');

    (text, rows)
}

// ===========================================================================
// Unified Matrix Trial
// ===========================================================================

fn run_matrix_trial(
    seed: u64,
    backend: BackendChoice,
    config: &AblationConfig,
    scenario: &ScenarioConfig,
    num_paths: u32,
) -> TrialResult {
    if backend.is_window() {
        run_matrix_trial_window(seed, backend, config, scenario, num_paths)
    } else if backend.is_block() {
        run_matrix_trial_block(seed, backend, config, scenario, num_paths)
    } else {
        run_matrix_trial_retransmit(seed, scenario, num_paths)
    }
}

// ---------------------------------------------------------------------------
// Window backend trial (RLC, Mettle, Streaming)
// ---------------------------------------------------------------------------

fn run_matrix_trial_window(
    seed: u64,
    backend: BackendChoice,
    config: &AblationConfig,
    scenario: &ScenarioConfig,
    num_paths: u32,
) -> TrialResult {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary) =
        make_sim_channels(scenario.name, num_paths, clock.clone(), seed);

    let estimator = make_estimator_for_loss(scenario.pre_warm_loss);
    let mut fec_ctrl = FecRateController::new_with_toggles(
        1e-5,
        MATRIX_FEC_OVERHEAD,
        ProtocolHint::Realtime,
        backend.fec_backend(),
        config.enable_pi,
        SYMBOL_SIZE,
    );
    for _ in 0..10 {
        fec_ctrl.feedback_update(true);
    }

    let mut encoder = backend.create_window_encoder(SYMBOL_SIZE, &fec_ctrl, &estimator);
    let mut decoder = backend.create_window_decoder(SYMBOL_SIZE, &fec_ctrl, &estimator);
    let mut reorder_buf = ReorderBuffer::new(config.reorder_timeout_ms, 500);
    let mut live_estimator = make_estimator_for_loss(scenario.pre_warm_loss);

    // Scheduler/BBR integration (ADR-0046: 2c)
    let mut scheduler = Scheduler::new(clock.clone() as Arc<dyn Clock>);
    scheduler.add_path(0);
    if num_paths >= 2 {
        scheduler.add_path(1);
    }

    // Cwnd pacing (ADR-0046: 2b)
    let (primary_cwnd, secondary_cwnd) = scenario_cwnd(scenario);

    // NACK congestion tracking: production-style exponential backoff
    let mut nack_repair_multiplier: f64 = 1.0;
    let mut nack_prev_loss_rate: f64 = 0.0;
    let mut nack_rising_loss: u32 = 0;
    let mut nack_prev_rtt: Option<Duration> = None;
    let mut nack_rising_rtt: u32 = 0;

    // Window PI feedback tracking
    let mut last_fed: u64 = 0;
    let mut last_useful: u64 = 0;

    let mut recovered = BTreeSet::new();
    let mut received_set = BTreeSet::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let mut delivery_latencies_ms: Vec<f64> = Vec::new();
    let mut encode_times: Vec<Instant> = Vec::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut repair_debt: f64 = 0.0; // Fractional repair accumulator

    let tick = Duration::from_micros(500);
    let mut sym_idx: u32 = 0;

    // Inline helper: process deliveries and feed scheduler
    macro_rules! process_window_deliveries {
        ($now:expr) => {{
            let mut delivered = primary.deliver();
            let primary_count = delivered.len() as u32;
            if let Some(ref mut sec) = secondary {
                let sec_pkts = sec.deliver();
                let sec_count = sec_pkts.len() as u32;
                delivered.extend(sec_pkts);
                if sec_count > 0 {
                    scheduler.ack(1, sec_count);
                }
            }
            if primary_count > 0 {
                scheduler.ack(0, primary_count);
            }

            // Compute actual RTT from delivery timestamps
            let mut rtt_sum = Duration::ZERO;
            let mut rtt_count = 0u32;
            for pkt in &delivered {
                let seq = pkt.seq as usize;
                if seq < encode_times.len() {
                    let actual_rtt = pkt.delivery_time.duration_since(encode_times[seq]);
                    rtt_sum += actual_rtt;
                    rtt_count += 1;
                }
            }
            if rtt_count > 0 {
                let avg_rtt = rtt_sum / rtt_count;
                live_estimator.record_rtt(avg_rtt);
                // Feed measured RTT to scheduler paths
                if primary_count > 0 {
                    if let Some(p) = scheduler.path_mut(0) {
                        p.record_rtt_sample(avg_rtt);
                    }
                }
                if num_paths >= 2 && delivered.len() as u32 > primary_count {
                    if let Some(p) = scheduler.path_mut(1) {
                        p.record_rtt_sample(avg_rtt);
                    }
                }
            }

            for pkt in &delivered {
                received_set.insert(pkt.seq);
                for (seq, data) in decoder.add_symbol(&pkt.symbol) {
                    for (rseq, _) in reorder_buf.push_with_time(seq, data, $now) {
                        if recovered.insert(rseq) {
                            delivery_order.push(rseq);
                            if (rseq as usize) < encode_times.len() {
                                let lat = $now.duration_since(encode_times[rseq as usize]);
                                delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
                            }
                        }
                    }
                }
            }
            for (seq, _) in reorder_buf.drain_expired($now) {
                if recovered.insert(seq) {
                    delivery_order.push(seq);
                    if (seq as usize) < encode_times.len() {
                        let lat = $now.duration_since(encode_times[seq as usize]);
                        delivery_latencies_ms.push(lat.as_secs_f64() * 1000.0);
                    }
                }
            }
            delivered.len()
        }};
    }

    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);
        let mut batch_survived: u32 = 0;
        let mut batch_dropped: u32 = 0;

        for _ in 0..this_batch {
            // Cwnd pacing: if primary is full, drain until capacity frees up
            let mut pacing_ticks = 0;
            while primary.in_flight_count() >= primary_cwnd && pacing_ticks < 200 {
                clock.advance(tick);
                let now = clock.now();
                process_window_deliveries!(now);
                pacing_ticks += 1;
            }

            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            encode_times.push(clock.now());
            let sym = encoder.add_source(&data);

            // Path selection via scheduler
            let use_secondary = num_paths >= 2
                && scheduler.best_source_path().map_or(false, |p| p == 1);

            if use_secondary {
                if let Some(ref mut sec) = secondary {
                    if sec.send(sym.clone()) {
                        batch_survived += 1;
                    } else {
                        batch_dropped += 1;
                    }
                }
                // Also send on primary for redundancy
                primary.send(sym);
            } else {
                if primary.send(sym.clone()) {
                    batch_survived += 1;
                } else {
                    batch_dropped += 1;
                }
                if let Some(ref mut sec) = secondary {
                    // Cwnd check for secondary before sending
                    if sec.in_flight_count() < secondary_cwnd {
                        sec.send(sym);
                    }
                }
            }

            sym_idx += 1;
        }
        total_source_sent += this_batch;

        // Adaptive repair with fractional accumulator — avoids ceil() rounding overhead
        let repair_rate = fec_ctrl.compute_repair_rate(&live_estimator, encoder.window_size());
        repair_debt += this_batch as f64 * repair_rate;
        let repair_count = (repair_debt.floor() as u32).min(10);
        repair_debt -= repair_count as f64;
        let mut batch_repairs_sent: u32 = 0;
        for _ in 0..repair_count {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            primary.send(repair);
            total_repair_sent += 1;
            batch_repairs_sent += 1;
        }

        // 0.5ms tick loop (20 ticks = 10ms per batch)
        for _ in 0..20 {
            clock.advance(tick);
            let now = clock.now();
            process_window_deliveries!(now);
        }

        // NACK repair with congestion awareness
        // Per-batch budget cap: total repairs (proactive + NACK) ≤ overhead budget
        let max_batch_repairs = ((this_batch as f64 * MATRIX_FEC_OVERHEAD * 2.0).ceil() as u32)
            .max(1);
        let nack_budget = max_batch_repairs.saturating_sub(batch_repairs_sent);

        // Age gate: only NACK symbols older than 2× base_delay to filter timing artifacts
        let nack_age_gate = Duration::from_millis(scenario.base_delay_ms * 2);
        let nack_oldest_eligible = if clock.now() > encode_times[0] + nack_age_gate {
            // Find the newest seq whose encode_time is old enough
            let cutoff = clock.now() - nack_age_gate;
            // Binary-ish search: the seq that was encoded before cutoff
            let mut eligible_end = sym_idx as u64;
            for seq in (0..sym_idx as u64).rev() {
                if (seq as usize) < encode_times.len() && encode_times[seq as usize] <= cutoff {
                    eligible_end = seq;
                    break;
                }
            }
            eligible_end
        } else {
            0 // Nothing old enough yet
        };

        if config.enable_nack && nack_oldest_eligible > BATCH_SIZE as u64 && nack_budget > 0 {
            let window_start = if nack_oldest_eligible > 50 {
                nack_oldest_eligible - 50
            } else {
                0
            };
            let window_end = nack_oldest_eligible;
            // Use received_set (network arrivals) — age gate filters timing artifacts
            let gaps = compute_gap_ranges(&received_set, window_start, window_end);

            if !gaps.is_empty() {
                // Production-style congestion-aware NACK scaling with exponential backoff
                let current_loss = live_estimator.loss_rate();
                let current_rtt = live_estimator.rtt();

                // Detect rising loss (>10% relative increase + 0.1% absolute floor)
                if current_loss > nack_prev_loss_rate * 1.1 + 0.001 {
                    nack_rising_loss += 1;
                } else {
                    nack_rising_loss = 0;
                }
                nack_prev_loss_rate = current_loss;

                // Detect rising RTT
                if let Some(prev_rtt) = nack_prev_rtt {
                    if current_rtt > prev_rtt + Duration::from_millis(1) {
                        nack_rising_rtt += 1;
                    } else {
                        nack_rising_rtt = 0;
                    }
                }
                nack_prev_rtt = Some(current_rtt);

                // Congestion = both rising loss AND rising RTT
                let congested = nack_rising_loss >= 2 && nack_rising_rtt >= 2;
                if congested {
                    nack_repair_multiplier = (nack_repair_multiplier * 0.5).max(0.0);
                } else if nack_rising_loss == 0 && nack_rising_rtt == 0 {
                    nack_repair_multiplier = (nack_repair_multiplier + 0.1).min(1.0);
                }

                let nack_repairs = ((gaps.len().min(MAX_NACK_GAPS).min(3) as f64
                    * nack_repair_multiplier)
                    .round() as u32)
                    .min(3)
                    .min(nack_budget);

                for _ in 0..nack_repairs {
                    if encoder.window_size() == 0 {
                        break;
                    }
                    let repair = encoder.generate_repair();
                    primary.send(repair);
                    total_repair_sent += 1;
                }
            }
        }

        live_estimator.record_batch(this_batch, batch_survived);
        // RTT is now fed from actual delivery timestamps in process_window_deliveries!

        // Window PI feedback: use repair efficiency instead of block-mode binary signal
        {
            let fed = decoder.repairs_fed();
            let useful = decoder.repairs_useful();
            fec_ctrl.feedback_update_window(fed - last_fed, useful - last_useful);
            last_fed = fed;
            last_useful = useful;
        }

        // Feed loss events to scheduler
        if batch_dropped > 0 {
            scheduler.on_loss(0, batch_survived > 0);
        }
    }

    // Drain remaining in-flight
    drain_and_collect_window(
        &clock,
        &mut primary,
        &mut secondary,
        &mut decoder,
        &mut reorder_buf,
        &mut recovered,
        &mut delivery_order,
        &mut delivery_latencies_ms,
        &encode_times,
        tick,
    );

    delivery_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let tail_drops =
        primary.tail_drop_count() + secondary.as_ref().map_or(0, |s| s.tail_drop_count());

    compute_trial_result(
        &recovered,
        &delivery_latencies_ms,
        &delivery_order,
        &encode_times,
        &clock,
        total_source_sent,
        total_repair_sent,
        tail_drops,
        scenario.base_delay_ms,
    )
}

/// Drain remaining in-flight packets and final reorder buffer flush (window mode).
fn drain_and_collect_window(
    clock: &Arc<MockClock>,
    primary: &mut SimChannel,
    secondary: &mut Option<SimChannel>,
    decoder: &mut Box<dyn WindowDecoder>,
    reorder_buf: &mut ReorderBuffer,
    recovered: &mut BTreeSet<u64>,
    delivery_order: &mut Vec<u64>,
    delivery_latencies_ms: &mut Vec<f64>,
    encode_times: &[Instant],
    tick: Duration,
) {
    for _ in 0..800 {
        clock.advance(tick);
        let now = clock.now();
        let mut d = primary.deliver();
        if let Some(ref mut sec) = secondary {
            d.extend(sec.deliver());
        }
        let empty = d.is_empty()
            && primary.in_flight_count() == 0
            && secondary.as_ref().map_or(true, |s| s.in_flight_count() == 0);

        for pkt in &d {
            for (seq, data) in decoder.add_symbol(&pkt.symbol) {
                for (rseq, _) in reorder_buf.push_with_time(seq, data, now) {
                    if recovered.insert(rseq) {
                        delivery_order.push(rseq);
                        if (rseq as usize) < encode_times.len() {
                            delivery_latencies_ms
                                .push(now.duration_since(encode_times[rseq as usize]).as_secs_f64() * 1000.0);
                        }
                    }
                }
            }
        }
        for (seq, _) in reorder_buf.drain_expired(now) {
            if recovered.insert(seq) {
                delivery_order.push(seq);
                if (seq as usize) < encode_times.len() {
                    delivery_latencies_ms
                        .push(now.duration_since(encode_times[seq as usize]).as_secs_f64() * 1000.0);
                }
            }
        }

        if empty {
            break;
        }
    }

    // Final reorder buffer drain
    clock.advance(Duration::from_secs(1));
    let now = clock.now();
    for (seq, _) in reorder_buf.drain_expired(now) {
        if recovered.insert(seq) {
            delivery_order.push(seq);
            if (seq as usize) < encode_times.len() {
                delivery_latencies_ms
                    .push(now.duration_since(encode_times[seq as usize]).as_secs_f64() * 1000.0);
            }
        }
    }
}

/// Compute TrialResult from collected data.
fn compute_trial_result(
    recovered: &BTreeSet<u64>,
    delivery_latencies_ms: &[f64],
    delivery_order: &[u64],
    encode_times: &[Instant],
    clock: &Arc<MockClock>,
    total_source_sent: u32,
    total_repair_sent: u32,
    tail_drops: u64,
    base_delay_ms: u64,
) -> TrialResult {
    let first_send = encode_times.first().copied().unwrap_or_else(|| clock.now());
    let elapsed = clock.now().duration_since(first_send).as_secs_f64().max(0.001);
    let throughput_mbps =
        (recovered.len() * SYMBOL_SIZE as usize) as f64 / elapsed / 1_000_000.0;

    let deadline_ms = base_delay_ms as f64 * 2.0;
    let misses = delivery_latencies_ms
        .iter()
        .filter(|&&l| l > deadline_ms)
        .count();
    let deadline_miss_pct =
        misses as f64 / delivery_latencies_ms.len().max(1) as f64 * 100.0;

    TrialResult {
        throughput_mbps,
        recovery_rate: recovered.len() as f64 / NUM_SYMBOLS as f64 * 100.0,
        overhead_pct: total_repair_sent as f64 / total_source_sent.max(1) as f64 * 100.0,
        total_repair_count: total_repair_sent,
        p50_latency_ms: percentile_ms(delivery_latencies_ms, 0.50),
        p95_latency_ms: percentile_ms(delivery_latencies_ms, 0.95),
        p99_latency_ms: percentile_ms(delivery_latencies_ms, 0.99),
        deadline_miss_pct,
        in_order_rate: compute_in_order_rate(delivery_order) * 100.0,
        tail_drops,
    }
}

// ---------------------------------------------------------------------------
// Block backend trial (RaptorQ, ReedSolomon)
// ---------------------------------------------------------------------------

fn run_matrix_trial_block(
    seed: u64,
    backend: BackendChoice,
    config: &AblationConfig,
    scenario: &ScenarioConfig,
    num_paths: u32,
) -> TrialResult {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary) =
        make_sim_channels(scenario.name, num_paths, clock.clone(), seed);
    let fec_backend = backend.fec_backend();

    let mut reorder_buf = ReorderBuffer::new(config.reorder_timeout_ms, 500);
    let mut live_estimator = make_estimator_for_loss(scenario.pre_warm_loss);
    let mut fec_ctrl = FecRateController::new_with_toggles(
        1e-5,
        MATRIX_FEC_OVERHEAD,
        ProtocolHint::Realtime,
        fec_backend,
        config.enable_pi,
        SYMBOL_SIZE,
    );
    for _ in 0..10 {
        fec_ctrl.feedback_update(true);
    }

    // Cwnd pacing (ADR-0046: 2b)
    let (primary_cwnd, _secondary_cwnd) = scenario_cwnd(scenario);

    let num_blocks = NUM_SYMBOLS / BLOCK_SIZE;
    let mut recovered = BTreeSet::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let mut delivery_latencies_ms: Vec<f64> = Vec::new();
    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;

    // Keep decoders alive for late-arriving symbols
    let mut block_decoders: Vec<(Box<dyn FecDecoder>, bool)> = Vec::new();
    // Track which source symbols arrived intact per block (for early delivery)
    let mut block_arrived: Vec<BTreeSet<u64>> = Vec::new();

    let tick = Duration::from_micros(500);

    // Helper: process deliveries for block mode with early source delivery (ADR-0046)
    macro_rules! process_block_deliveries {
        ($now:expr, $delivered:expr) => {
            for pkt in $delivered {
                let bid = pkt.symbol.block_id as usize;
                if bid < block_decoders.len() {
                    let bstart = (bid as u32 * BLOCK_SIZE) as u64;

                    // Early source delivery: deliver intact source symbols immediately
                    if !pkt.symbol.is_repair {
                        let seq = bstart + pkt.symbol.payload_id as u64;
                        if bid < block_arrived.len() {
                            block_arrived[bid].insert(seq);
                        }
                        if recovered.insert(seq) {
                            for (rseq, _) in
                                reorder_buf.push_with_time(seq, bytes::Bytes::from_static(&[0u8]), $now)
                            {
                                delivery_order.push(rseq);
                                if let Some(&st) = send_times.get(&rseq) {
                                    delivery_latencies_ms.push(
                                        $now.duration_since(st).as_secs_f64() * 1000.0,
                                    );
                                }
                            }
                        }
                    }

                    let (ref mut dec, ref mut decoded) = block_decoders[bid];
                    if !*decoded {
                        if dec.add_symbol(&pkt.symbol).is_some() {
                            *decoded = true;
                            // On decode: deliver only previously-missing symbols
                            let arrived = if bid < block_arrived.len() {
                                &block_arrived[bid]
                            } else {
                                &BTreeSet::new()
                            };
                            for j in 0..BLOCK_SIZE as u64 {
                                let seq = bstart + j;
                                if !arrived.contains(&seq) {
                                    if recovered.insert(seq) {
                                        for (rseq, _) in
                                            reorder_buf.push_with_time(seq, bytes::Bytes::from_static(&[0u8]), $now)
                                        {
                                            delivery_order.push(rseq);
                                            if let Some(&st) = send_times.get(&rseq) {
                                                delivery_latencies_ms.push(
                                                    $now.duration_since(st).as_secs_f64() * 1000.0,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (seq, _) in reorder_buf.drain_expired($now) {
                if recovered.insert(seq) {
                    delivery_order.push(seq);
                    if let Some(&st) = send_times.get(&seq) {
                        delivery_latencies_ms
                            .push($now.duration_since(st).as_secs_f64() * 1000.0);
                    }
                }
            }
        };
    }

    for block_idx in 0..num_blocks {
        // Compute repair count from PI-adjusted controller
        let repair_rate = fec_ctrl.compute_repair_rate(&live_estimator, BLOCK_SIZE as usize);
        let mut repair_count = ((BLOCK_SIZE as f64 * repair_rate).ceil() as u32).max(1);
        if fec_backend == FecBackend::ReedSolomon {
            repair_count = repair_count.min(255 - BLOCK_SIZE);
        }

        let block_data =
            vec![(block_idx % 256) as u8; BLOCK_SIZE as usize * SYMBOL_SIZE as usize];
        let params = EncodingParams {
            source_symbols: BLOCK_SIZE,
            symbol_size: SYMBOL_SIZE,
            repair_count,
            block_id: block_idx as u64,
        };

        let encoder = fec_backend.create_encoder(&block_data, params);
        let source_syms = encoder.source_symbols();
        let repair_syms = encoder.repair_symbols(repair_count);
        let decoder = fec_backend.create_decoder(params, block_data.len() as u64);
        block_decoders.push((decoder, false));
        block_arrived.push(BTreeSet::new());

        let block_start_seq = (block_idx * BLOCK_SIZE) as u64;
        let mut repairs_cursor: usize = 0;

        // Send source symbols in batches with interleaved repairs
        let mut batch_start = 0u32;
        while batch_start < BLOCK_SIZE {
            let batch_end = (batch_start + BATCH_SIZE).min(BLOCK_SIZE);
            let this_batch = batch_end - batch_start;
            let mut batch_survived: u32 = 0;
            let mut batch_dropped: u32 = 0;

            for i in batch_start..batch_end {
                // Cwnd pacing: drain until primary has capacity
                let mut pacing_ticks = 0;
                while primary.in_flight_count() >= primary_cwnd && pacing_ticks < 200 {
                    clock.advance(tick);
                    let now = clock.now();
                    let mut d = primary.deliver();
                    if let Some(ref mut sec) = secondary {
                        d.extend(sec.deliver());
                    }
                    process_block_deliveries!(now, &d);
                    pacing_ticks += 1;
                }

                let sym = &source_syms[i as usize];
                let global_seq = block_start_seq + i as u64;
                send_times.insert(global_seq, clock.now());
                if primary.send(sym.clone()) {
                    batch_survived += 1;
                } else {
                    batch_dropped += 1;
                }
                if let Some(ref mut sec) = secondary {
                    sec.send(sym.clone());
                }
            }
            total_source_sent += this_batch;

            // Interleaved repairs for this batch
            let batch_repair_count = ((this_batch as f64 * repair_rate).ceil() as usize)
                .min(repair_syms.len() - repairs_cursor);
            for i in 0..batch_repair_count {
                primary.send(repair_syms[repairs_cursor + i].clone());
                total_repair_sent += 1;
            }
            repairs_cursor += batch_repair_count;

            // 0.5ms tick loop (20 ticks = 10ms per batch)
            for _ in 0..20 {
                clock.advance(tick);
                let now = clock.now();

                let mut all_delivered = primary.deliver();
                if let Some(ref mut sec) = secondary {
                    all_delivered.extend(sec.deliver());
                }

                process_block_deliveries!(now, &all_delivered);
            }

            live_estimator.record_batch(this_batch, batch_survived);
            // RTT fed from actual delivery timestamps (block trial measures via send_times)
            {
                let now_block = clock.now();
                let mut rtt_sum_block = Duration::ZERO;
                let mut rtt_n = 0u32;
                for seq in (block_start_seq + batch_start as u64)..(block_start_seq + batch_end as u64) {
                    if recovered.contains(&seq) {
                        if let Some(&st) = send_times.get(&seq) {
                            rtt_sum_block += now_block.duration_since(st);
                            rtt_n += 1;
                        }
                    }
                }
                if rtt_n > 0 {
                    live_estimator.record_rtt(rtt_sum_block / rtt_n);
                }
            }
            fec_ctrl.feedback_update(batch_dropped == 0);

            batch_start = batch_end;
        }

        // Send remaining repairs for this block
        while repairs_cursor < repair_syms.len() {
            primary.send(repair_syms[repairs_cursor].clone());
            total_repair_sent += 1;
            repairs_cursor += 1;
        }
    }

    // Drain remaining in-flight
    for _ in 0..800 {
        clock.advance(tick);
        let now = clock.now();
        let mut d = primary.deliver();
        if let Some(ref mut sec) = secondary {
            d.extend(sec.deliver());
        }
        let empty = d.is_empty()
            && primary.in_flight_count() == 0
            && secondary.as_ref().map_or(true, |s| s.in_flight_count() == 0);

        process_block_deliveries!(now, &d);

        if empty {
            break;
        }
    }

    // Final reorder buffer drain
    clock.advance(Duration::from_secs(1));
    let now = clock.now();
    for (seq, _) in reorder_buf.drain_expired(now) {
        if recovered.insert(seq) {
            delivery_order.push(seq);
            if let Some(&st) = send_times.get(&seq) {
                delivery_latencies_ms.push(now.duration_since(st).as_secs_f64() * 1000.0);
            }
        }
    }

    delivery_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let tail_drops =
        primary.tail_drop_count() + secondary.as_ref().map_or(0, |s| s.tail_drop_count());

    let first_send = send_times
        .values()
        .min()
        .copied()
        .unwrap_or_else(|| clock.now());
    let elapsed = clock.now().duration_since(first_send).as_secs_f64().max(0.001);
    let throughput_mbps =
        (recovered.len() * SYMBOL_SIZE as usize) as f64 / elapsed / 1_000_000.0;

    let deadline_ms = scenario.base_delay_ms as f64 * 2.0;
    let misses = delivery_latencies_ms
        .iter()
        .filter(|&&l| l > deadline_ms)
        .count();
    let deadline_miss_pct =
        misses as f64 / delivery_latencies_ms.len().max(1) as f64 * 100.0;

    TrialResult {
        throughput_mbps,
        recovery_rate: recovered.len() as f64 / NUM_SYMBOLS as f64 * 100.0,
        overhead_pct: total_repair_sent as f64 / total_source_sent.max(1) as f64 * 100.0,
        total_repair_count: total_repair_sent,
        p50_latency_ms: percentile_ms(&delivery_latencies_ms, 0.50),
        p95_latency_ms: percentile_ms(&delivery_latencies_ms, 0.95),
        p99_latency_ms: percentile_ms(&delivery_latencies_ms, 0.99),
        deadline_miss_pct,
        in_order_rate: compute_in_order_rate(&delivery_order) * 100.0,
        tail_drops,
    }
}

// ---------------------------------------------------------------------------
// Retransmit backend trial
// ---------------------------------------------------------------------------

fn run_matrix_trial_retransmit(
    seed: u64,
    scenario: &ScenarioConfig,
    num_paths: u32,
) -> TrialResult {
    let clock = Arc::new(MockClock::new());
    let (mut primary, mut secondary) =
        make_reliable_channels_for_scenario(scenario.name, num_paths, clock.clone(), seed);

    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut deliver_times: HashMap<u64, Instant> = HashMap::new();
    let mut delivery_order: Vec<u64> = Vec::new();
    let mut reorder_buf = ReorderBuffer::new(25, 500);

    // Cwnd pacing (ADR-0046: 2b)
    let (primary_cwnd, secondary_cwnd) = scenario_cwnd(scenario);

    let tick = Duration::from_micros(500);
    let mut sym_idx: u32 = 0;

    // Inline helper for retransmit delivery processing
    macro_rules! drain_retransmit {
        ($now:expr) => {{
            let mut all_delivered = primary.deliver();
            if let Some(ref mut sec) = secondary {
                all_delivered.extend(sec.deliver());
            }
            for pkt in all_delivered {
                let id = pkt.symbol.payload_id as u64;
                if !deliver_times.contains_key(&id) {
                    for (rseq, _) in reorder_buf.push_with_time(
                        id,
                        bytes::Bytes::from_static(&[0u8]),
                        $now,
                    ) {
                        deliver_times.insert(rseq, $now);
                        delivery_order.push(rseq);
                    }
                }
            }
            for (seq, _) in reorder_buf.drain_expired($now) {
                if !deliver_times.contains_key(&seq) {
                    deliver_times.insert(seq, $now);
                    delivery_order.push(seq);
                }
            }
        }};
    }

    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        for i in 0..this_batch {
            // Cwnd pacing: drain until path has capacity
            let target_cwnd = if secondary.is_some() && i % 2 != 0 {
                secondary_cwnd
            } else {
                primary_cwnd
            };
            let mut pacing_ticks = 0;
            while pacing_ticks < 200 {
                let in_flight = if secondary.is_some() && i % 2 != 0 {
                    secondary.as_ref().unwrap().in_flight_count()
                } else {
                    primary.in_flight_count()
                };
                if in_flight < target_cwnd {
                    break;
                }
                clock.advance(tick);
                let now = clock.now();
                drain_retransmit!(now);
                pacing_ticks += 1;
            }

            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let symbol = WireSymbol {
                block_id: 0,
                payload_id: sym_idx,
                is_repair: false,
                data,
                backend: FecBackend::Rlc,
            };
            send_times.insert(sym_idx as u64, clock.now());

            // Round-robin for 2-path, primary-only for 1-path
            if let Some(ref mut sec) = secondary {
                if i % 2 == 0 {
                    primary.send(symbol);
                } else {
                    sec.send(symbol);
                }
            } else {
                primary.send(symbol);
            }
            sym_idx += 1;
        }

        // 0.5ms tick loop (20 ticks = 10ms per batch)
        for _ in 0..20 {
            clock.advance(tick);
            let now = clock.now();
            drain_retransmit!(now);
        }
    }

    // Drain remaining
    for _ in 0..800 {
        clock.advance(tick);
        let now = clock.now();
        let empty = primary.in_flight_count() == 0
            && secondary.as_ref().map_or(true, |s| s.in_flight_count() == 0);
        drain_retransmit!(now);
        if empty {
            break;
        }
    }

    // Final reorder buffer drain
    clock.advance(Duration::from_secs(1));
    let now = clock.now();
    drain_retransmit!(now);

    let mut latencies_ms: Vec<f64> = deliver_times
        .iter()
        .filter_map(|(id, &dt)| {
            send_times
                .get(id)
                .map(|&st| dt.duration_since(st).as_secs_f64() * 1000.0)
        })
        .collect();
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let total_tx = primary.total_transmissions()
        + secondary.as_ref().map_or(0, |s| s.total_transmissions());
    let total_unique =
        primary.total_unique() + secondary.as_ref().map_or(0, |s| s.total_unique());
    let overhead = if total_unique > 0 {
        (total_tx as f64 - total_unique as f64) / total_unique as f64 * 100.0
    } else {
        0.0
    };
    let retransmit_count = (total_tx - total_unique) as u32;

    let first_send = send_times
        .values()
        .min()
        .copied()
        .unwrap_or_else(|| clock.now());
    let last_deliver = deliver_times
        .values()
        .max()
        .copied()
        .unwrap_or_else(|| clock.now());
    let elapsed = last_deliver
        .duration_since(first_send)
        .as_secs_f64()
        .max(0.001);
    let throughput_mbps =
        (deliver_times.len() * SYMBOL_SIZE as usize) as f64 / elapsed / 1_000_000.0;

    let deadline_ms = scenario.base_delay_ms as f64 * 2.0;
    let misses = latencies_ms.iter().filter(|&&l| l > deadline_ms).count();
    let deadline_miss_pct = misses as f64 / latencies_ms.len().max(1) as f64 * 100.0;

    let tail_drops =
        primary.tail_drop_count() + secondary.as_ref().map_or(0, |s| s.tail_drop_count());

    TrialResult {
        throughput_mbps,
        recovery_rate: deliver_times.len() as f64 / NUM_SYMBOLS as f64 * 100.0,
        overhead_pct: overhead,
        total_repair_count: retransmit_count,
        p50_latency_ms: percentile_ms(&latencies_ms, 0.50),
        p95_latency_ms: percentile_ms(&latencies_ms, 0.95),
        p99_latency_ms: percentile_ms(&latencies_ms, 0.99),
        deadline_miss_pct,
        in_order_rate: compute_in_order_rate(&delivery_order) * 100.0,
        tail_drops,
    }
}

// ===========================================================================
// Matrix runner
// ===========================================================================

fn run_matrix() -> Vec<MatrixCell> {
    let scenarios = scenario_configs();
    let configs = ablation_configs();
    let mut cells = Vec::new();

    // Compute total cell count for progress reporting
    let mut total_cells = 0u32;
    for backend in BackendChoice::all() {
        for scenario in &scenarios {
            for &_paths in scenario.paths_supported {
                if backend.is_retransmit() {
                    total_cells += 1;
                } else {
                    total_cells += configs.len() as u32;
                }
            }
        }
    }

    let mut completed = 0u32;

    for backend in BackendChoice::all() {
        for scenario in &scenarios {
            for &num_paths in scenario.paths_supported {
                let run_configs: Vec<&str> = if backend.is_retransmit() {
                    vec!["baseline"]
                } else {
                    configs.iter().map(|c| c.name).collect()
                };

                for cfg_name in &run_configs {
                    let cfg = configs.iter().find(|c| c.name == *cfg_name).unwrap();
                    completed += 1;
                    eprint!(
                        "\r[{}/{}] {} / {} / {} / {} paths   ",
                        completed, total_cells, backend.name(), scenario.name, cfg.name, num_paths
                    );

                    let mut thru_s = TrialStats::new();
                    let mut recovery_s = TrialStats::new();
                    let mut overhead_s = TrialStats::new();
                    let mut repair_s = TrialStats::new();
                    let mut p50_s = TrialStats::new();
                    let mut p95_s = TrialStats::new();
                    let mut p99_s = TrialStats::new();
                    let mut deadline_s = TrialStats::new();
                    let mut inorder_s = TrialStats::new();
                    let mut drops_s = TrialStats::new();

                    for trial in 0..NUM_TRIALS {
                        let r = run_matrix_trial(
                            trial * 137 + 42,
                            *backend,
                            cfg,
                            scenario,
                            num_paths,
                        );
                        thru_s.push(r.throughput_mbps);
                        recovery_s.push(r.recovery_rate);
                        overhead_s.push(r.overhead_pct);
                        repair_s.push(r.total_repair_count as f64);
                        p50_s.push(r.p50_latency_ms);
                        p95_s.push(r.p95_latency_ms);
                        p99_s.push(r.p99_latency_ms);
                        deadline_s.push(r.deadline_miss_pct);
                        inorder_s.push(r.in_order_rate);
                        drops_s.push(r.tail_drops as f64);
                    }

                    let mut metrics = BTreeMap::new();
                    metrics.insert("throughput_mbps".to_string(), stats_to_metric(&thru_s));
                    metrics.insert("recovery_rate".to_string(), stats_to_metric(&recovery_s));
                    metrics.insert("overhead_pct".to_string(), stats_to_metric(&overhead_s));
                    metrics.insert(
                        "total_repair_count".to_string(),
                        stats_to_metric(&repair_s),
                    );
                    metrics.insert("p50_latency_ms".to_string(), stats_to_metric(&p50_s));
                    metrics.insert("p95_latency_ms".to_string(), stats_to_metric(&p95_s));
                    metrics.insert("p99_latency_ms".to_string(), stats_to_metric(&p99_s));
                    metrics.insert(
                        "deadline_miss_pct".to_string(),
                        stats_to_metric(&deadline_s),
                    );
                    metrics.insert("in_order_rate".to_string(), stats_to_metric(&inorder_s));
                    metrics.insert("tail_drops".to_string(), stats_to_metric(&drops_s));

                    cells.push(MatrixCell {
                        backend: backend.name().to_string(),
                        scenario: scenario.name.to_string(),
                        config: cfg.name.to_string(),
                        paths: num_paths,
                        metrics,
                    });
                }
            }
        }
    }
    eprintln!(); // newline after progress

    cells
}

// ===========================================================================
// Timestamp & Git info
// ===========================================================================

fn format_timestamp() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds_val = time_of_day % 60;

    // Civil date from days since epoch (Howard Hinnant's algorithm)
    let z = (secs / 86400) as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let file_suffix = format!(
        "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
        y, m, d, hours, minutes, seconds_val
    );
    let readable = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, d, hours, minutes, seconds_val
    );

    (file_suffix, readable)
}

fn git_commit_info() -> (String, String) {
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let message = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    (hash, message)
}

// ===========================================================================
// Output formatting
// ===========================================================================

fn format_matrix_markdown(matrix: &[MatrixCell]) -> String {
    let mut md = String::new();
    let scenarios = scenario_configs();

    md.push_str("## Matrix: Comprehensive FEC Comparison\n\n");

    for scenario in &scenarios {
        for &paths in scenario.paths_supported {
            let suffix = if paths > 1 { "s" } else { "" };
            md.push_str(&format!(
                "### {} — {} path{}\n\n",
                scenario.name, paths, suffix
            ));

            md.push_str("| Backend     | Config   | Recovery | Thru MB/s | Overhead% (repairs) | p50  | p95  | p99  | Deadline% | In-order% | Drops |\n");
            md.push_str("|-------------|----------|----------|-----------|---------------------|------|------|------|-----------|-----------|-------|\n");

            let cells: Vec<&MatrixCell> = matrix
                .iter()
                .filter(|c| c.scenario == scenario.name && c.paths == paths)
                .collect();

            for cell in &cells {
                let get = |name: &str| -> f64 {
                    cell.metrics.get(name).map(|m| m.mean).unwrap_or(0.0)
                };
                let recovery = get("recovery_rate");
                let throughput = get("throughput_mbps");
                let overhead = get("overhead_pct");
                let repairs = get("total_repair_count");
                let p50 = get("p50_latency_ms");
                let p95 = get("p95_latency_ms");
                let p99 = get("p99_latency_ms");
                let deadline = get("deadline_miss_pct");
                let inorder = get("in_order_rate");
                let drops = get("tail_drops");

                md.push_str(&format!(
                    "| {:<11} | {:<8} | {:>8.1} | {:>9.2} | {:>5.1}% ({:>5.0}) | {:>4.1} | {:>4.1} | {:>4.1} | {:>9.1} | {:>9.1} | {:>5.0} |\n",
                    cell.backend, cell.config, recovery, throughput, overhead, repairs,
                    p50, p95, p99, deadline, inorder, drops,
                ));
            }

            md.push('\n');
        }
    }

    md
}

fn write_results(
    table1_text: &str,
    table2_text: &str,
    table1_data: Vec<LossSweepRow>,
    table1b_data: Vec<LossSweepRow>,
    table2_data: Vec<OverheadRow>,
    matrix: Vec<MatrixCell>,
) {
    let (timestamp_suffix, timestamp_readable) = format_timestamp();
    let (commit_hash, commit_message) = git_commit_info();

    // Build full markdown
    let mut md = String::new();
    md.push_str("# Benchmark Results\n\n");
    md.push_str(&format!(
        "- **Commit**: {} — {}\n",
        commit_hash, commit_message
    ));
    md.push_str(&format!("- **Date**: {}\n", timestamp_readable));
    md.push_str(&format!(
        "- **Parameters**: {}B symbols, {} symbols, batch={}, {} trials, {}% FEC\n\n",
        SYMBOL_SIZE,
        NUM_SYMBOLS,
        BATCH_SIZE,
        NUM_TRIALS,
        (MATRIX_FEC_OVERHEAD * 100.0) as u32
    ));
    md.push_str(table1_text);
    md.push_str(table2_text);
    md.push_str(&format_matrix_markdown(&matrix));

    // Print matrix to stdout too
    print!("{}", format_matrix_markdown(&matrix));

    // Build JSON
    let output = BenchmarkOutput {
        commit_hash: commit_hash.clone(),
        commit_message: commit_message.clone(),
        timestamp: timestamp_suffix.clone(),
        parameters: BenchmarkParameters {
            symbol_size: SYMBOL_SIZE,
            num_symbols: NUM_SYMBOLS,
            batch_size: BATCH_SIZE,
            num_trials: NUM_TRIALS,
            fec_overhead: MATRIX_FEC_OVERHEAD,
            block_size: BLOCK_SIZE,
        },
        table1_uniform: table1_data,
        table1b_bursty: table1b_data,
        table2_overhead: table2_data,
        matrix,
    };

    let json = serde_json::to_string_pretty(&output).expect("JSON serialization failed");

    // Write files
    let base_dir = env!("CARGO_MANIFEST_DIR");
    let md_path = format!(
        "{}/docs/benchmark-results-{}.md",
        base_dir, timestamp_suffix
    );
    let json_path = format!(
        "{}/docs/benchmark-results-{}.json",
        base_dir, timestamp_suffix
    );

    std::fs::write(&md_path, &md).expect("Failed to write markdown file");
    std::fs::write(&json_path, &json).expect("Failed to write JSON file");

    println!("\nOutput files:");
    println!("  {}", md_path);
    println!("  {}", json_path);
    println!(
        "  Commit: {} — {}",
        commit_hash, commit_message
    );
    println!("  Matrix cells: {}", output.matrix.len());
}

// ===========================================================================
// Main entry point
// ===========================================================================

#[test]
fn bench_suite() {
    // Tables 1/1b: codec recovery sweep
    let (table1_text, table1_data, table1b_data) = table1_backend_loss_sweep();
    print!("{}", table1_text);

    // Table 2: wire overhead
    let (table2_text, table2_data) = table2_wire_overhead();
    print!("{}", table2_text);

    // Matrix: comprehensive comparison (replaces Tables 3/4/5)
    let matrix = run_matrix();

    // Write all results to files
    write_results(
        &table1_text,
        &table2_text,
        table1_data,
        table1b_data,
        table2_data,
        matrix,
    );
}
