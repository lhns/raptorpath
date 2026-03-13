//! Statistical evaluation: run many trials to characterize METTLE's behavior.
//!
//! These tests run 1000+ random encode/decode trials and report aggregate statistics.

use mettle::{MettleConfig, MettleDecoder, MettleEncoder};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

struct TrialResult {
    success: bool,
    coded_packets_fed: usize,
    total_coded_available: usize,
    source_lost: usize,
    source_total: usize,
}

fn run_trials(
    config: MettleConfig,
    num_packets: usize,
    loss_rate: f64,
    num_trials: usize,
) -> Vec<TrialResult> {
    let mut rng = StdRng::seed_from_u64(99999);
    let mut results = Vec::with_capacity(num_trials);

    for trial in 0..num_trials {
        let seed = trial as u64 * 31337;

        let packets: Vec<Vec<u8>> = (0..num_packets)
            .map(|i| vec![(i % 256) as u8; 100])
            .collect();

        let mut encoder = MettleEncoder::new(config, seed);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }
        let coded = encoder.coded_packets();

        let mut decoder = MettleDecoder::new(config, num_packets, seed);

        let mut source_lost = 0;
        for (i, pkt) in packets.iter().enumerate() {
            if rng.gen::<f64>() < loss_rate {
                source_lost += 1;
            } else {
                decoder.add_source_packet(i, pkt);
            }
        }

        let mut coded_fed = 0;
        for cp in &coded {
            coded_fed += 1;
            decoder.add_coded_packet(cp);
            if decoder.is_complete() {
                break;
            }
        }

        results.push(TrialResult {
            success: decoder.is_complete(),
            coded_packets_fed: coded_fed,
            total_coded_available: coded.len(),
            source_lost,
            source_total: num_packets,
        });
    }

    results
}

fn summarize(label: &str, results: &[TrialResult]) {
    let total = results.len();
    let successes = results.iter().filter(|r| r.success).count();
    let success_rate = successes as f64 / total as f64;

    let successful_results: Vec<&TrialResult> = results.iter().filter(|r| r.success).collect();
    let avg_coded_needed = if !successful_results.is_empty() {
        successful_results.iter().map(|r| r.coded_packets_fed).sum::<usize>() as f64
            / successful_results.len() as f64
    } else {
        0.0
    };

    let avg_loss = results.iter().map(|r| r.source_lost).sum::<usize>() as f64 / total as f64;

    println!("--- {label} ---");
    println!("  Trials: {total}");
    println!("  Success rate: {:.1}% ({successes}/{total})", success_rate * 100.0);
    println!("  Avg source lost: {avg_loss:.1}");
    println!("  Avg coded packets needed (successful): {avg_coded_needed:.1}");
    if !results.is_empty() {
        println!("  Total coded available: {}", results[0].total_coded_available);
    }
    println!();
}

#[test]
fn statistical_default_window() {
    let config = MettleConfig::default();
    let trials = 500;

    for loss_pct in [1, 5, 10] {
        let loss_rate = loss_pct as f64 / 100.0;
        let results = run_trials(config, 100, loss_rate, trials);
        summarize(&format!("w=600, k=100, {loss_pct}% loss"), &results);

        if loss_pct <= 5 {
            let success_rate =
                results.iter().filter(|r| r.success).count() as f64 / trials as f64;
            assert!(
                success_rate > 0.7,
                "Default config should decode >70% at {loss_pct}% loss, got {:.1}%",
                success_rate * 100.0
            );
        }
    }
}

#[test]
fn statistical_small_window() {
    let config = MettleConfig::small_window();
    let trials = 500;

    for loss_pct in [1, 5, 10] {
        let loss_rate = loss_pct as f64 / 100.0;
        let results = run_trials(config, 50, loss_rate, trials);
        summarize(&format!("w=50, k=50, {loss_pct}% loss"), &results);
    }
}

#[test]
fn statistical_overhead_sweep() {
    let trials = 200;
    let loss_rate = 0.05;

    println!("=== Overhead factor sweep at w=50, k=50, 5% loss ===");
    for c_pct in [5, 10, 15, 20, 25, 30] {
        let c = c_pct as f64 / 100.0;
        let config = MettleConfig {
            window_size: 50,
            num_edges: 4,
            overhead_factor: c,
        };
        let results = run_trials(config, 50, loss_rate, trials);
        summarize(&format!("c={c:.2}"), &results);
    }
}
