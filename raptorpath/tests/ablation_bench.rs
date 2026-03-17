//! Ablation benchmark: measures each toggleable feature's real-world impact.
//!
//! Strategy: one-feature-off ablation. Run a baseline (all features on), then
//! disable one feature at a time. Measures both overhead cost and recovery rate
//! under normal and tight FEC budgets.
//!
//! Additional benchmarks:
//! - Cross-backend comparison: all 4 block + 3 window backends under identical scenarios
//! - Protocol-hint ablation: Realtime vs Bulk vs Auto across block and window modes
//!
//! Run with: cargo test --test ablation_bench -- --nocapture

use mettle::MettleConfig;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{FecRateController, ProtocolHint};
use raptorpath::fec::{
    EncodingParams, FecBackend, MettleWindowDecoder, MettleWindowEncoder, RlcWindowDecoder,
    RlcWindowEncoder, StreamingDecoder, StreamingEncoder, WindowDecoder,
    WindowEncoder, WireSymbol,
};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Gilbert-Elliott channel simulator (same as fec_realworld_recovery_test)
// ---------------------------------------------------------------------------

struct GilbertElliottChannel {
    p_gb: f64,
    p_bg: f64,
    loss_good: f64,
    loss_bad: f64,
}

impl GilbertElliottChannel {
    fn apply<T: Clone>(&self, symbols: &[T], rng: &mut ChaCha8Rng) -> (Vec<T>, BTreeSet<usize>) {
        use rand::Rng;
        let mut in_bad = false;
        let mut surviving = Vec::new();
        let mut dropped = BTreeSet::new();

        for (i, sym) in symbols.iter().enumerate() {
            let loss_prob = if in_bad { self.loss_bad } else { self.loss_good };
            if rng.gen::<f64>() < loss_prob {
                dropped.insert(i);
            } else {
                surviving.push(sym.clone());
            }
            let transition: f64 = rng.gen();
            if in_bad {
                if transition < self.p_bg {
                    in_bad = false;
                }
            } else if transition < self.p_gb {
                in_bad = true;
            }
        }

        (surviving, dropped)
    }
}

struct Scenario {
    name: &'static str,
    channel: GilbertElliottChannel,
    stationary_loss: f64,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Datacenter",
            channel: GilbertElliottChannel {
                p_gb: 0.0,
                p_bg: 1.0,
                loss_good: 0.001,
                loss_bad: 0.0,
            },
            stationary_loss: 0.001,
        },
        Scenario {
            name: "WiFi",
            channel: GilbertElliottChannel {
                p_gb: 0.03,
                p_bg: 0.5,
                loss_good: 0.01,
                loss_bad: 0.3,
            },
            stationary_loss: 0.025,
        },
        Scenario {
            name: "LTE",
            channel: GilbertElliottChannel {
                p_gb: 0.02,
                p_bg: 0.25,
                loss_good: 0.005,
                loss_bad: 0.4,
            },
            stationary_loss: 0.035,
        },
        Scenario {
            name: "Congested",
            channel: GilbertElliottChannel {
                p_gb: 0.08,
                p_bg: 0.15,
                loss_good: 0.05,
                loss_bad: 0.6,
            },
            stationary_loss: 0.12,
        },
    ]
}

// ---------------------------------------------------------------------------
// Feature toggle config
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AblationConfig {
    name: &'static str,
    max_fec_overhead: f64,
    enable_pi_feedback: bool,
    ge_burst_factor: f64,
    realtime_burst_extra: f64,
    // ProbeRTT and reorder buffer don't affect pure FEC pipeline tests
    // (no real network), but we include them for completeness
}

fn ablation_configs() -> Vec<AblationConfig> {
    vec![
        AblationConfig {
            name: "baseline",
            max_fec_overhead: 0.5,
            enable_pi_feedback: true,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "no_pi",
            max_fec_overhead: 0.5,
            enable_pi_feedback: false,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "no_ge_burst",
            max_fec_overhead: 0.5,
            enable_pi_feedback: true,
            ge_burst_factor: 0.0,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "no_rt_extra",
            max_fec_overhead: 0.5,
            enable_pi_feedback: true,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.0,
        },
    ]
}

/// Tight-budget configs: cap FEC at 15% to stress the controller.
/// Under this budget, removing features should degrade recovery.
fn tight_budget_configs() -> Vec<AblationConfig> {
    vec![
        AblationConfig {
            name: "tight_base",
            max_fec_overhead: 0.15,
            enable_pi_feedback: true,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "tight_no_pi",
            max_fec_overhead: 0.15,
            enable_pi_feedback: false,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "tight_no_ge",
            max_fec_overhead: 0.15,
            enable_pi_feedback: true,
            ge_burst_factor: 0.0,
            realtime_burst_extra: 0.10,
        },
        AblationConfig {
            name: "tight_no_rt",
            max_fec_overhead: 0.15,
            enable_pi_feedback: true,
            ge_burst_factor: 0.15,
            realtime_burst_extra: 0.0,
        },
    ]
}

// ---------------------------------------------------------------------------
// Trial result
// ---------------------------------------------------------------------------

struct TrialResult {
    recovery_rate: f64,
    repair_count: u32,
    overhead_pct: f64,
    repair_efficiency: f64,
}

const NUM_TRIALS: u64 = 100;
const SYMBOL_SIZE: u16 = 1200;

// ---------------------------------------------------------------------------
// Warm up an estimator for a scenario, ending in burst state
// ---------------------------------------------------------------------------

fn warm_estimator(scenario: &Scenario) -> LossEstimator {
    let mut estimator = LossEstimator::new();
    let batch_size = 1000u32;
    let received = ((1.0 - scenario.stationary_loss) * batch_size as f64) as u32;
    for _ in 0..50 {
        estimator.record_batch(batch_size, received);
    }
    // Feed GE state with bursty loss pattern to warm up HMM
    if scenario.channel.p_gb > 0.0 {
        for _ in 0..10 {
            estimator.record_batch(10, 3); // lossy batch (burst)
            estimator.record_batch(10, 10); // clean batch (good)
        }
    }
    // Fix C: End with a lossy batch so is_in_burst() == true.
    // record_batch(10, 3) means 7 losses → consecutive_losses=7 ≥ 3 → in_burst=true
    // This ensures the realtime_burst_extra path is exercised.
    estimator.record_batch(10, 3);
    estimator
}

// ---------------------------------------------------------------------------
// Block-mode ablation (RaptorQ representative)
// ---------------------------------------------------------------------------

fn block_ablation_trial(
    backend: FecBackend,
    scenario: &Scenario,
    cfg: &AblationConfig,
) -> TrialResult {
    block_ablation_trial_with_hint(backend, scenario, cfg, ProtocolHint::Realtime)
}

fn block_ablation_trial_with_hint(
    backend: FecBackend,
    scenario: &Scenario,
    cfg: &AblationConfig,
    hint: ProtocolHint,
) -> TrialResult {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let k = (data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;

    let estimator = warm_estimator(scenario);

    let mut ctrl = FecRateController::new_with_toggles(
        1e-5,
        cfg.max_fec_overhead,
        hint,
        backend,
        cfg.enable_pi_feedback,
        cfg.ge_burst_factor,
        cfg.realtime_burst_extra,
    );

    // Fix D: Stress PI controller — 200 iterations at 80% failure rate
    for i in 0..200 {
        ctrl.feedback_update(i % 5 == 0); // 80% failure rate
    }

    let mut repair_count = ctrl.compute_repair_count(k, &estimator);
    // RS has max n=255, so clamp repair count
    if backend == FecBackend::ReedSolomon {
        repair_count = repair_count.min(255 - k);
    }
    let overhead_pct = repair_count as f64 / k as f64 * 100.0;

    let mut successes = 0u64;

    for seed in 0..NUM_TRIALS {
        let params = EncodingParams {
            source_symbols: k,
            symbol_size: SYMBOL_SIZE,
            repair_count,
            block_id: 0,
        };
        let encoder = backend.create_encoder(&data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_count);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, _) = scenario.channel.apply(&source, &mut rng);
        let mut all_syms: Vec<WireSymbol> = surviving;
        all_syms.extend(repairs);

        let mut decoder = backend.create_decoder(params, data.len() as u64);
        for sym in &all_syms {
            if decoder.add_symbol(sym).is_some() {
                successes += 1;
                break;
            }
        }
    }

    TrialResult {
        recovery_rate: successes as f64 / NUM_TRIALS as f64 * 100.0,
        repair_count,
        overhead_pct,
        repair_efficiency: 0.0, // block mode — no per-repair tracking
    }
}

// ---------------------------------------------------------------------------
// Window-mode ablation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum WindowBackendKind {
    Rlc,
    Mettle,
    Streaming,
}

impl WindowBackendKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Rlc => "RLC",
            Self::Mettle => "Mettle",
            Self::Streaming => "Streaming",
        }
    }

    fn fec_backend(&self) -> FecBackend {
        match self {
            Self::Rlc => FecBackend::Rlc,
            Self::Mettle => FecBackend::Mettle,
            Self::Streaming => FecBackend::Streaming,
        }
    }

    fn create_encoder(&self, symbol_size: u16, ctrl: &FecRateController, estimator: &LossEstimator) -> Box<dyn WindowEncoder> {
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
        }
    }

    fn create_decoder(&self, symbol_size: u16, ctrl: &FecRateController, estimator: &LossEstimator) -> Box<dyn WindowDecoder> {
        match self {
            Self::Rlc => Box::new(RlcWindowDecoder::new(symbol_size)),
            Self::Mettle => Box::new(MettleWindowDecoder::new(symbol_size)),
            Self::Streaming => {
                let params = ctrl.compute_streaming_params(estimator);
                Box::new(StreamingDecoder::new(symbol_size, params))
            }
        }
    }
}

fn window_ablation_trial(scenario: &Scenario, cfg: &AblationConfig) -> TrialResult {
    window_ablation_trial_full(WindowBackendKind::Rlc, scenario, cfg, ProtocolHint::Realtime)
}

fn window_ablation_trial_full(
    backend_kind: WindowBackendKind,
    scenario: &Scenario,
    cfg: &AblationConfig,
    hint: ProtocolHint,
) -> TrialResult {
    let num_symbols = 500usize;

    let estimator = warm_estimator(scenario);

    let mut ctrl = FecRateController::new_with_toggles(
        1e-5,
        cfg.max_fec_overhead,
        hint,
        backend_kind.fec_backend(),
        cfg.enable_pi_feedback,
        cfg.ge_burst_factor,
        cfg.realtime_burst_extra,
    );

    // Fix D: Stress PI controller
    for i in 0..200 {
        ctrl.feedback_update(i % 5 == 0); // 80% failure rate
    }

    let repair_rate = ctrl.compute_repair_rate(&estimator);
    let repair_count = (num_symbols as f64 * repair_rate).ceil() as usize;
    let repair_count = repair_count.max(5);
    let overhead_pct = repair_count as f64 / num_symbols as f64 * 100.0;

    let mut total_lost = 0usize;
    let mut total_recovered = 0usize;
    let mut total_repairs_fed = 0u64;
    let mut total_repairs_useful = 0u64;

    for seed in 0..NUM_TRIALS {
        let mut encoder = backend_kind.create_encoder(SYMBOL_SIZE, &ctrl, &estimator);
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();
        let sources: Vec<WireSymbol> = packet_data
            .iter()
            .map(|pkt| encoder.add_source(pkt))
            .collect();
        let repairs: Vec<WireSymbol> = (0..repair_count)
            .map(|_| encoder.generate_repair())
            .collect();

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, dropped) = scenario.channel.apply(&sources, &mut rng);

        let mut decoder = backend_kind.create_decoder(SYMBOL_SIZE, &ctrl, &estimator);
        let mut recovered_seqs = BTreeSet::new();

        for sym in &surviving {
            for (seq, _) in decoder.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }
        for sym in &repairs {
            for (seq, _) in decoder.add_symbol(sym) {
                recovered_seqs.insert(seq);
            }
        }

        let lost_seqs: BTreeSet<u64> = dropped.iter().map(|&i| i as u64).collect();
        total_lost += lost_seqs.len();
        total_recovered += lost_seqs
            .iter()
            .filter(|s| recovered_seqs.contains(s))
            .count();
        total_repairs_fed += decoder.repairs_fed();
        total_repairs_useful += decoder.repairs_useful();
    }

    let recovery = if total_lost == 0 {
        100.0
    } else {
        total_recovered as f64 / total_lost as f64 * 100.0
    };

    TrialResult {
        recovery_rate: recovery,
        repair_count: repair_count as u32,
        overhead_pct,
        repair_efficiency: total_repairs_useful as f64 / total_repairs_fed.max(1) as f64,
    }
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn print_table_header() {
    println!(
        "| {:14} | {:>8} | {:>10} | {:>14} | {:>10} | {:>14} |",
        "Config", "Repairs", "Overhead", "Delta Overhead", "Recovery", "Delta Recovery"
    );
    println!(
        "|{:-<16}|{:-<10}|{:-<12}|{:-<16}|{:-<12}|{:-<16}|",
        "", "", "", "", "", ""
    );
}

fn print_baseline_row(name: &str, result: &TrialResult) {
    println!(
        "| {:14} | {:>8} | {:>9.1}% | {:>14} | {:>9.1}% | {:>14} |",
        name, result.repair_count, result.overhead_pct, "—", result.recovery_rate, "—"
    );
}

fn print_ablation_row(name: &str, result: &TrialResult, baseline: &TrialResult) {
    let delta_recovery = result.recovery_rate - baseline.recovery_rate;
    let delta_overhead = result.overhead_pct - baseline.overhead_pct;
    println!(
        "| {:14} | {:>8} | {:>9.1}% | {:>+13.1}pp | {:>9.1}% | {:>+13.1}pp |",
        name,
        result.repair_count,
        result.overhead_pct,
        delta_overhead,
        result.recovery_rate,
        delta_recovery
    );
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn ablation_benchmark() {
    let scenarios = scenarios();
    let configs = ablation_configs();
    let tight_configs = tight_budget_configs();

    // ===== Block-mode (RaptorQ) — normal budget =====
    println!("\n## Ablation Results — RaptorQ (block mode, max_overhead=50%)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        let baseline = block_ablation_trial(FecBackend::RaptorQ, scenario, &configs[0]);
        print_baseline_row(configs[0].name, &baseline);

        for cfg in &configs[1..] {
            let result = block_ablation_trial(FecBackend::RaptorQ, scenario, cfg);
            print_ablation_row(cfg.name, &result, &baseline);
        }
        println!();
    }

    // ===== Block-mode (RaptorQ) — tight budget =====
    println!("## Ablation Results — RaptorQ (block mode, max_overhead=15% TIGHT)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        let baseline = block_ablation_trial(FecBackend::RaptorQ, scenario, &tight_configs[0]);
        print_baseline_row(tight_configs[0].name, &baseline);

        for cfg in &tight_configs[1..] {
            let result = block_ablation_trial(FecBackend::RaptorQ, scenario, cfg);
            print_ablation_row(cfg.name, &result, &baseline);
        }
        println!();
    }

    // ===== Window-mode (RLC) — normal budget =====
    println!("## Ablation Results — RLC (window mode, max_overhead=50%)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        let baseline = window_ablation_trial(scenario, &configs[0]);
        print_baseline_row(configs[0].name, &baseline);

        for cfg in &configs[1..] {
            let result = window_ablation_trial(scenario, cfg);
            print_ablation_row(cfg.name, &result, &baseline);
        }
        println!();
    }

    // ===== Window-mode (RLC) — tight budget =====
    println!("## Ablation Results — RLC (window mode, max_overhead=15% TIGHT)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        let baseline = window_ablation_trial(scenario, &tight_configs[0]);
        print_baseline_row(tight_configs[0].name, &baseline);

        for cfg in &tight_configs[1..] {
            let result = window_ablation_trial(scenario, cfg);
            print_ablation_row(cfg.name, &result, &baseline);
        }
        println!();
    }

    // Caveat note
    println!("### Caveats: Features NOT covered by FEC-only benchmarks");
    println!();
    println!("The following features require a full network pipeline:");
    println!("- **ProbeRTT / BBR phases**: need RTT measurements + scheduler state machine");
    println!("- **Reorder buffer**: needs out-of-order packet delivery simulation");
    println!("- **NACK repair bursts**: needs bidirectional communication channel");
    println!("- **Tapered interleaving**: needs multi-block InterleavingBuffer pipeline");
    println!("- **Block profile / framing**: needs packet assembly pipeline");
    println!("- **Path scheduling**: needs multipath simulation with heterogeneous RTTs");
    println!("- **Backend auto-switching**: needs loss measurement over time to trigger heuristic");
    println!();
    println!("These features are benchmarked in `pipeline_ablation_bench.rs` using the full SimChannel pipeline (ADR-0033).");
}

// ---------------------------------------------------------------------------
// Cross-backend comparison benchmark
// ---------------------------------------------------------------------------

#[test]
fn backend_comparison_benchmark() {
    let scenarios = scenarios();
    let baseline_cfg = AblationConfig {
        name: "baseline",
        max_fec_overhead: 0.5,
        enable_pi_feedback: true,
        ge_burst_factor: 0.3,
        realtime_burst_extra: 0.10,
    };
    let tight_cfg = AblationConfig {
        name: "tight_base",
        max_fec_overhead: 0.15,
        enable_pi_feedback: true,
        ge_burst_factor: 0.3,
        realtime_burst_extra: 0.10,
    };

    let block_backends = [
        FecBackend::RaptorQ,
        FecBackend::ReedSolomon,
        FecBackend::Mettle,
        FecBackend::Rlc,
    ];
    let window_backends = [
        WindowBackendKind::Rlc,
        WindowBackendKind::Mettle,
        WindowBackendKind::Streaming,
    ];

    // ===== Block-mode cross-backend comparison =====
    for (budget_label, cfg) in [("50%", &baseline_cfg), ("15% TIGHT", &tight_cfg)] {
        println!("\n## Backend Comparison — Block mode (max_overhead={budget_label})");
        println!();
        for scenario in &scenarios {
            println!("### {} scenario", scenario.name);
            println!();
            print_table_header();

            // RaptorQ is the baseline
            let baseline = block_ablation_trial(FecBackend::RaptorQ, scenario, cfg);
            print_baseline_row("RaptorQ", &baseline);

            for &backend in &block_backends[1..] {
                let label = match backend {
                    FecBackend::ReedSolomon => "ReedSolomon",
                    FecBackend::Mettle => "Mettle",
                    FecBackend::Rlc => "RLC",
                    _ => unreachable!(),
                };
                let result = block_ablation_trial(backend, scenario, cfg);
                print_ablation_row(label, &result, &baseline);
            }
            println!();
        }
    }

    // ===== Window-mode cross-backend comparison =====
    for (budget_label, cfg) in [("50%", &baseline_cfg), ("15% TIGHT", &tight_cfg)] {
        println!("\n## Backend Comparison — Window mode (max_overhead={budget_label})");
        println!();
        for scenario in &scenarios {
            println!("### {} scenario", scenario.name);
            println!();
            print_table_header();

            // RLC is the baseline for window mode
            let baseline = window_ablation_trial_full(
                WindowBackendKind::Rlc,
                scenario,
                cfg,
                ProtocolHint::Realtime,
            );
            print_baseline_row("RLC", &baseline);

            for &backend_kind in &window_backends[1..] {
                let result = window_ablation_trial_full(
                    backend_kind,
                    scenario,
                    cfg,
                    ProtocolHint::Realtime,
                );
                print_ablation_row(backend_kind.name(), &result, &baseline);
            }
            println!();
        }
    }

    println!("### Caveats");
    println!();
    println!("- Streaming backend falls back to RaptorQ in block mode (window-only codec)");
    println!("- RS capped at max n=255 (repair_count clamped to 255-k)");
    println!("- METTLE uses `small_window()` config; production may differ");
}

// ---------------------------------------------------------------------------
// Protocol-hint ablation benchmark
// ---------------------------------------------------------------------------

#[test]
fn protocol_hint_benchmark() {
    let scenarios = scenarios();
    let baseline_cfg = AblationConfig {
        name: "baseline",
        max_fec_overhead: 0.5,
        enable_pi_feedback: true,
        ge_burst_factor: 0.3,
        realtime_burst_extra: 0.10,
    };

    let hints = [
        (ProtocolHint::Auto, "Auto"),
        (ProtocolHint::Realtime, "Realtime"),
        (ProtocolHint::Bulk, "Bulk"),
    ];

    // ===== Block-mode hint comparison (RaptorQ) =====
    println!("\n## Protocol Hint Comparison — RaptorQ (block mode, max_overhead=50%)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        // Auto as baseline
        let baseline = block_ablation_trial_with_hint(
            FecBackend::RaptorQ,
            scenario,
            &baseline_cfg,
            ProtocolHint::Auto,
        );
        print_baseline_row("Auto", &baseline);

        for &(hint, label) in &hints[1..] {
            let result = block_ablation_trial_with_hint(
                FecBackend::RaptorQ,
                scenario,
                &baseline_cfg,
                hint,
            );
            print_ablation_row(label, &result, &baseline);
        }
        println!();
    }

    // ===== Window-mode hint comparison (RLC) =====
    println!("## Protocol Hint Comparison — RLC (window mode, max_overhead=50%)");
    println!();
    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_table_header();

        // Auto as baseline
        let baseline = window_ablation_trial_full(
            WindowBackendKind::Rlc,
            scenario,
            &baseline_cfg,
            ProtocolHint::Auto,
        );
        print_baseline_row("Auto", &baseline);

        for &(hint, label) in &hints[1..] {
            let result = window_ablation_trial_full(
                WindowBackendKind::Rlc,
                scenario,
                &baseline_cfg,
                hint,
            );
            print_ablation_row(label, &result, &baseline);
        }
        println!();
    }

    println!("### Expected behavior");
    println!();
    println!("- **Realtime** uses 1.2× repair rate (window) or burst_extra (block) → higher overhead, better recovery");
    println!("- **Bulk** uses 0.7× multiplier → lower overhead, relies on retransmission");
    println!("- **Auto** uses 1.0× baseline → middle ground");
}
