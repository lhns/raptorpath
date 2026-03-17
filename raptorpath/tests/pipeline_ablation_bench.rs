//! Full-pipeline ablation benchmark (ADR-0033).
//!
//! Unlike `ablation_bench.rs` which tests FEC-only features, this benchmark
//! uses the SimChannel infrastructure to measure end-to-end impact of:
//! - ProbeRTT / BBR phases
//! - Reorder buffer
//! - NACK repair
//! - Backend auto-switching
//! - Multipath scheduling
//!
//! Strategy: one-feature-off ablation with 6 configs × 3 scenarios × 20 trials.
//!
//! Run with: cargo test --test pipeline_ablation_bench -- --nocapture

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
// Configuration
// ---------------------------------------------------------------------------

struct PipelineAblationConfig {
    name: &'static str,
    enable_probe_rtt: bool,
    reorder_timeout_ms: u64,
    enable_nack_repair: bool,
    auto_switch: bool,
    num_paths: u32,
}

fn ablation_configs() -> Vec<PipelineAblationConfig> {
    vec![
        PipelineAblationConfig {
            name: "baseline",
            enable_probe_rtt: true,
            reorder_timeout_ms: 25,
            enable_nack_repair: true,
            auto_switch: true,
            num_paths: 2,
        },
        PipelineAblationConfig {
            name: "no_probe_rtt",
            enable_probe_rtt: false,
            reorder_timeout_ms: 25,
            enable_nack_repair: true,
            auto_switch: true,
            num_paths: 2,
        },
        PipelineAblationConfig {
            name: "no_reorder",
            enable_probe_rtt: true,
            reorder_timeout_ms: 0,
            enable_nack_repair: true,
            auto_switch: true,
            num_paths: 2,
        },
        PipelineAblationConfig {
            name: "no_nack",
            enable_probe_rtt: true,
            reorder_timeout_ms: 25,
            enable_nack_repair: false,
            auto_switch: true,
            num_paths: 2,
        },
        PipelineAblationConfig {
            name: "no_auto_switch",
            enable_probe_rtt: true,
            reorder_timeout_ms: 25,
            enable_nack_repair: true,
            auto_switch: false,
            num_paths: 2,
        },
        PipelineAblationConfig {
            name: "single_path",
            enable_probe_rtt: true,
            reorder_timeout_ms: 25,
            enable_nack_repair: true,
            auto_switch: true,
            num_paths: 1,
        },
    ]
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

struct PipelineAblationResult {
    recovery_rate: f64,
    overhead_pct: f64,
    avg_cwnd: f64,
    cwnd_stability: f64,
    backend_switches: u32,
    probe_rtt_entries: u32,
    repair_efficiency: f64,
}

// ---------------------------------------------------------------------------
// Scenario
// ---------------------------------------------------------------------------

enum ScenarioKind {
    DatacenterStable,
    WiFiBursty,
    DcToWiFi,
}

struct PipelineScenario {
    name: &'static str,
    kind: ScenarioKind,
}

fn pipeline_scenarios() -> Vec<PipelineScenario> {
    vec![
        PipelineScenario {
            name: "Datacenter_Stable",
            kind: ScenarioKind::DatacenterStable,
        },
        PipelineScenario {
            name: "WiFi_Bursty",
            kind: ScenarioKind::WiFiBursty,
        },
        PipelineScenario {
            name: "DC_to_WiFi",
            kind: ScenarioKind::DcToWiFi,
        },
    ]
}

// ---------------------------------------------------------------------------
// Pipeline runner
// ---------------------------------------------------------------------------

const NUM_SYMBOLS: u32 = 2000;
const BATCH_SIZE: u32 = 10;
const NUM_TRIALS: u64 = 20;
const SYMBOL_SIZE: u16 = 64;
const PROBE_RTT_CWND: u32 = 4;

fn run_ablation_pipeline(
    seed: u64,
    scenario: &PipelineScenario,
    cfg: &PipelineAblationConfig,
) -> PipelineAblationResult {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new_with_config(clock.clone(), cfg.enable_probe_rtt);

    // Set up paths
    let primary_id: u32 = 1;
    sched.add_path(primary_id);
    let secondary_id: u32 = 2;
    let use_secondary = cfg.num_paths >= 2;
    if use_secondary {
        sched.add_path(secondary_id);
    }

    // Warmup paths
    for id in std::iter::once(primary_id).chain(if use_secondary { Some(secondary_id) } else { None }) {
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

    // Create channels based on scenario
    let (mut primary_channel, primary_rtt, mut secondary_channel, secondary_rtt) =
        match scenario.kind {
            ScenarioKind::DatacenterStable => (
                SimChannel::datacenter(clock.clone(), seed),
                Duration::from_millis(1),
                SimChannel::datacenter(clock.clone(), seed + 1000),
                Duration::from_millis(2),
            ),
            ScenarioKind::WiFiBursty => (
                SimChannel::wifi(clock.clone(), seed),
                Duration::from_millis(5),
                SimChannel::lte(clock.clone(), seed + 1000),
                Duration::from_millis(20),
            ),
            ScenarioKind::DcToWiFi => (
                SimChannel::datacenter(clock.clone(), seed),
                Duration::from_millis(1),
                SimChannel::lte(clock.clone(), seed + 1000),
                Duration::from_millis(20),
            ),
        };

    // Components
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let mut reorder_buf = ReorderBuffer::new(cfg.reorder_timeout_ms, 500);
    let mut estimator = LossEstimator::new();

    let forced_backend = if cfg.auto_switch {
        None
    } else {
        Some(FecBackend::Rlc)
    };
    let mut selector = BackendSelector::new(
        FecBackend::Rlc,
        forced_backend,
        ProtocolHint::Auto,
        0.02,
        0.08,
        0, // switch_interval_secs=0 bypasses hysteresis
        true,
    );

    let mut fec_ctrl = FecRateController::new(
        1e-5,
        0.5,
        ProtocolHint::Realtime,
        FecBackend::Rlc,
    );

    // Tracking state
    let mut recovered = BTreeSet::new();
    let mut received_set = BTreeSet::new();
    let mut total_source_sent: u32 = 0;
    let mut total_repair_sent: u32 = 0;
    let mut cwnd_history = Vec::new();
    let mut backend_switches: u32 = 0;
    let mut probe_rtt_entries: u32 = 0;
    let mut was_in_probe_rtt = false;

    let mut sym_idx: u32 = 0;
    let phase_switch_sym: u32 = 1000; // For DC_to_WiFi scenario

    while sym_idx < NUM_SYMBOLS {
        let this_batch = BATCH_SIZE.min(NUM_SYMBOLS - sym_idx);

        // DC_to_WiFi: switch primary channel at phase boundary
        let current_primary_rtt = match scenario.kind {
            ScenarioKind::DcToWiFi if sym_idx >= phase_switch_sym => {
                // After phase switch, use WiFi channel characteristics
                // We switch by creating a new channel only once
                if sym_idx == phase_switch_sym {
                    primary_channel = SimChannel::wifi(clock.clone(), seed + 5000);
                }
                Duration::from_millis(5)
            }
            ScenarioKind::DcToWiFi => primary_rtt,
            _ => primary_rtt,
        };

        // Encode and send source symbols
        let mut batch_survived = 0u32;
        let mut batch_dropped = 0u32;

        for _ in 0..this_batch {
            let data = vec![sym_idx as u8; SYMBOL_SIZE as usize];
            let sym = encoder.add_source(&data);

            if primary_channel.send(sym.clone()) {
                batch_survived += 1;
            } else {
                batch_dropped += 1;
            }

            // Send on secondary path too if multipath
            if use_secondary {
                secondary_channel.send(sym);
            }

            sym_idx += 1;
        }
        total_source_sent += this_batch;

        // Generate adaptive repair count
        let repair_rate = fec_ctrl.compute_repair_rate(&estimator);
        let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).max(1).min(10);

        for _ in 0..repair_count {
            if encoder.window_size() == 0 {
                break;
            }
            let repair = encoder.generate_repair();
            primary_channel.send(repair);
            total_repair_sent += 1;
        }

        // Advance clock: max(rtt, 50ms) to ensure >10s simulated time for ProbeRTT
        let step = current_primary_rtt.max(Duration::from_millis(50));
        clock.advance(step);

        // Deliver from primary channel
        let now = clock.now();
        let delivered_primary = primary_channel.deliver();
        for pkt in &delivered_primary {
            received_set.insert(pkt.seq);
            let decoded = decoder.add_symbol(&pkt.symbol);
            for (seq, data) in decoded {
                let reordered = reorder_buf.push_with_time(seq, data, now);
                for (rseq, _) in reordered {
                    recovered.insert(rseq);
                }
            }
        }

        // Deliver from secondary channel
        if use_secondary {
            let delivered_secondary = secondary_channel.deliver();
            for pkt in &delivered_secondary {
                received_set.insert(pkt.seq);
                let decoded = decoder.add_symbol(&pkt.symbol);
                for (seq, data) in decoded {
                    let reordered = reorder_buf.push_with_time(seq, data, now);
                    for (rseq, _) in reordered {
                        recovered.insert(rseq);
                    }
                }
            }
        }

        // Drain expired from reorder buffer
        let expired = reorder_buf.drain_expired(now);
        for (seq, _) in expired {
            recovered.insert(seq);
        }

        // NACK flow: detect gaps and generate targeted repairs
        if cfg.enable_nack_repair && sym_idx > BATCH_SIZE {
            let window_start = if sym_idx > 50 { (sym_idx - 50) as u64 } else { 0 };
            let window_end = sym_idx as u64;
            let gaps = compute_gap_ranges(&received_set, window_start, window_end);

            if !gaps.is_empty() {
                let nack_repairs = gaps.len().min(MAX_NACK_GAPS).min(3);
                for _ in 0..nack_repairs {
                    if encoder.window_size() == 0 {
                        break;
                    }
                    let repair = encoder.generate_repair();
                    primary_channel.send(repair);
                    total_repair_sent += 1;
                }
            }
        }

        // Update estimator
        estimator.record_batch(this_batch, batch_survived);
        estimator.record_rtt(current_primary_rtt);

        // Feed scheduler
        sched.ack(primary_id, batch_survived);
        if let Some(path) = sched.path_mut(primary_id) {
            path.estimator.record_rtt(current_primary_rtt);
            path.record_rtt_sample(current_primary_rtt);
            path.estimator.record_batch(this_batch, batch_survived);
        }
        if use_secondary {
            sched.ack(secondary_id, this_batch); // secondary gets everything
            if let Some(path) = sched.path_mut(secondary_id) {
                path.estimator.record_rtt(secondary_rtt);
                path.record_rtt_sample(secondary_rtt);
                path.estimator.record_batch(this_batch, this_batch);
            }
        }
        if batch_dropped > 0 {
            let fec_ok = (batch_dropped as f64 / this_batch as f64) < 0.20;
            sched.on_loss(primary_id, fec_ok);
        }

        // Track cwnd
        let cwnd = sched.path(primary_id).map(|p| p.cwnd).unwrap_or(0);
        cwnd_history.push(cwnd);

        // Detect ProbeRTT entry (cwnd drops to PROBE_RTT_CWND)
        if cwnd == PROBE_RTT_CWND {
            if !was_in_probe_rtt {
                probe_rtt_entries += 1;
                was_in_probe_rtt = true;
            }
        } else {
            was_in_probe_rtt = false;
        }

        // Check backend selector
        if let Some(_new_backend) = selector.evaluate(&estimator) {
            backend_switches += 1;
        }

        // PI feedback
        let batch_ok = batch_dropped == 0;
        fec_ctrl.feedback_update(batch_ok);
    }

    // Compute metrics
    let recovery_rate = recovered.len() as f64 / NUM_SYMBOLS as f64;
    let overhead_pct = total_repair_sent as f64 / total_source_sent as f64 * 100.0;

    let avg_cwnd = if cwnd_history.is_empty() {
        0.0
    } else {
        cwnd_history.iter().map(|&c| c as f64).sum::<f64>() / cwnd_history.len() as f64
    };

    let cwnd_stability = if cwnd_history.len() < 2 || avg_cwnd < 1e-10 {
        0.0
    } else {
        let variance = cwnd_history
            .iter()
            .map(|&c| {
                let diff = c as f64 - avg_cwnd;
                diff * diff
            })
            .sum::<f64>()
            / cwnd_history.len() as f64;
        variance.sqrt() / avg_cwnd // coefficient of variation
    };

    PipelineAblationResult {
        recovery_rate,
        overhead_pct,
        avg_cwnd,
        cwnd_stability,
        backend_switches,
        probe_rtt_entries,
        repair_efficiency: decoder.repairs_useful() as f64 / decoder.repairs_fed().max(1) as f64,
    }
}

// ---------------------------------------------------------------------------
// Averaging over trials
// ---------------------------------------------------------------------------

fn run_averaged(
    scenario: &PipelineScenario,
    cfg: &PipelineAblationConfig,
) -> PipelineAblationResult {
    let mut sum_recovery = 0.0;
    let mut sum_overhead = 0.0;
    let mut sum_avg_cwnd = 0.0;
    let mut sum_cwnd_stability = 0.0;
    let mut sum_backend_switches = 0u32;
    let mut sum_probe_rtt_entries = 0u32;
    let mut sum_repair_efficiency = 0.0;

    for trial in 0..NUM_TRIALS {
        let r = run_ablation_pipeline(trial * 137 + 42, scenario, cfg);
        sum_recovery += r.recovery_rate;
        sum_overhead += r.overhead_pct;
        sum_avg_cwnd += r.avg_cwnd;
        sum_cwnd_stability += r.cwnd_stability;
        sum_backend_switches += r.backend_switches;
        sum_probe_rtt_entries += r.probe_rtt_entries;
        sum_repair_efficiency += r.repair_efficiency;
    }

    let n = NUM_TRIALS as f64;
    PipelineAblationResult {
        recovery_rate: sum_recovery / n,
        overhead_pct: sum_overhead / n,
        avg_cwnd: sum_avg_cwnd / n,
        cwnd_stability: sum_cwnd_stability / n,
        backend_switches: (sum_backend_switches as f64 / n).round() as u32,
        probe_rtt_entries: (sum_probe_rtt_entries as f64 / n).round() as u32,
        repair_efficiency: sum_repair_efficiency / n,
    }
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn print_pipeline_header() {
    println!(
        "| {:16} | {:>9} | {:>9} | {:>9} | {:>8} | {:>8} | {:>8} |",
        "Config", "Recovery", "Overhead", "Avg cwnd", "cwnd CV", "Switches", "ProbeRTT"
    );
    println!(
        "|{:-<18}|{:-<11}|{:-<11}|{:-<11}|{:-<10}|{:-<10}|{:-<10}|",
        "", "", "", "", "", "", ""
    );
}

fn print_baseline_result(r: &PipelineAblationResult) {
    println!(
        "| {:16} | {:>8.1}% | {:>8.1}% | {:>9.1} | {:>8.3} | {:>8} | {:>8} |",
        "baseline",
        r.recovery_rate * 100.0,
        r.overhead_pct,
        r.avg_cwnd,
        r.cwnd_stability,
        r.backend_switches,
        r.probe_rtt_entries,
    );
}

fn print_ablation_result(name: &str, r: &PipelineAblationResult, baseline: &PipelineAblationResult) {
    let delta_recovery = (r.recovery_rate - baseline.recovery_rate) * 100.0;
    let delta_overhead = r.overhead_pct - baseline.overhead_pct;
    println!(
        "| {:16} | {:>+8.1}pp | {:>+8.1}pp | {:>9.1} | {:>8.3} | {:>8} | {:>8} |",
        name,
        delta_recovery,
        delta_overhead,
        r.avg_cwnd,
        r.cwnd_stability,
        r.backend_switches,
        r.probe_rtt_entries,
    );
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
fn pipeline_ablation_benchmark() {
    let scenarios = pipeline_scenarios();
    let configs = ablation_configs();

    println!("\n## Pipeline Ablation Results ({NUM_TRIALS} trials, {NUM_SYMBOLS} symbols per trial)");
    println!();

    for scenario in &scenarios {
        println!("### {} scenario", scenario.name);
        println!();
        print_pipeline_header();

        let baseline = run_averaged(scenario, &configs[0]);
        print_baseline_result(&baseline);

        for cfg in &configs[1..] {
            let result = run_averaged(scenario, cfg);
            print_ablation_result(cfg.name, &result, &baseline);
        }
        println!();
    }

    // Verification checks (soft — print warnings, don't fail the bench)
    println!("### Verification checks");
    println!();

    // Check: no_probe_rtt should have 0 probe_rtt_entries in datacenter
    let dc_baseline = run_averaged(&scenarios[0], &configs[0]);
    let dc_no_probe = run_averaged(&scenarios[0], &configs[1]);
    if dc_no_probe.probe_rtt_entries == 0 && dc_baseline.probe_rtt_entries > 0 {
        println!("- [PASS] no_probe_rtt: 0 entries (baseline: {})", dc_baseline.probe_rtt_entries);
    } else {
        println!(
            "- [INFO] probe_rtt: baseline={}, no_probe_rtt={} (ProbeRTT may not trigger in short sim)",
            dc_baseline.probe_rtt_entries, dc_no_probe.probe_rtt_entries
        );
    }

    // Check: no_auto_switch should have 0 backend switches
    let wifi_no_switch = run_averaged(&scenarios[1], &configs[4]);
    if wifi_no_switch.backend_switches == 0 {
        println!("- [PASS] no_auto_switch: 0 backend switches");
    } else {
        println!(
            "- [WARN] no_auto_switch: {} backend switches (expected 0)",
            wifi_no_switch.backend_switches
        );
    }

    // Check: single_path should have lower recovery than baseline in bursty scenario
    let wifi_baseline = run_averaged(&scenarios[1], &configs[0]);
    let wifi_single = run_averaged(&scenarios[1], &configs[5]);
    let delta = wifi_single.recovery_rate - wifi_baseline.recovery_rate;
    if delta <= 0.0 {
        println!(
            "- [PASS] single_path: recovery {:.1}pp lower than baseline",
            -delta * 100.0
        );
    } else {
        println!(
            "- [INFO] single_path: recovery {:.1}pp higher than baseline (multipath redundancy minor in this scenario)",
            delta * 100.0
        );
    }

    println!();
    println!("These results complement `ablation_bench.rs` which covers FEC-only features.");
    println!("For per-feature tradeoff analysis with latency/ordering/burst metrics, see `tradeoff_ablation_bench.rs` (ADR-0034).");
    println!("See ADR-0033 for methodology details.");
}
