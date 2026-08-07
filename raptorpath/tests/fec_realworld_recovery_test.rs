//! Real-world FEC recovery rate comparison across Gilbert-Elliott channel scenarios.
//!
//! Run with: cargo test --test fec_realworld_recovery_test -- --nocapture

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{
    EncodingParams, FecBackend, FecEncoder,
    RlcWindowDecoder, RlcWindowEncoder,
    WindowDecoder, WindowEncoder, WireSymbol,
};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Gilbert-Elliott channel simulator
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

fn make_params(k: u32, symbol_size: u16, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: k,
        symbol_size,
        repair_count,
        block_id: 0,
    }
}

const NUM_TRIALS: u64 = 10;
const SYMBOL_SIZE: u16 = 1200;

// ---------------------------------------------------------------------------
// Block-mode recovery test
// ---------------------------------------------------------------------------

/// Block recovery with identical 25% overhead for all backends (apples-to-apples).
fn block_recovery_rate_same_overhead(backend: FecBackend, scenario: &Scenario) -> (f64, u32) {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let k = (data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;
    let repair_count = (k as f64 * 0.25).ceil() as u32;

    let mut successes = 0u64;

    for seed in 0..NUM_TRIALS {
        let params = make_params(k, SYMBOL_SIZE, repair_count);
        let encoder = backend.create_encoder(&data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_count);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, _) = scenario.channel.apply(&source, &mut rng);
        let mut all_syms: Vec<WireSymbol> = surviving;
        all_syms.extend(repairs);

        let mut decoder = backend.create_decoder(params, data.len() as u64);
        let mut decoded = false;
        for sym in &all_syms {
            if decoder.add_symbol(sym).is_some() {
                decoded = true;
                break;
            }
        }
        if decoded {
            successes += 1;
        }
    }

    (successes as f64 / NUM_TRIALS as f64 * 100.0, repair_count)
}

/// Block recovery with full repair budget — the budget backend (RS) sets max_repairs(), others get 25%.
fn block_recovery_rate_full_budget(backend: FecBackend, scenario: &Scenario) -> (f64, u32) {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let k = (data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;
    let base_repair = (k as f64 * 0.25).ceil() as u32;

    // Probe max_repairs from a sample encoder
    let sample_params = make_params(k, SYMBOL_SIZE, base_repair);
    let sample_encoder = backend.create_encoder(&data, sample_params);
    let max_rep = sample_encoder.max_repairs();
    let repair_count = if max_rep < u32::MAX { max_rep } else { base_repair };

    let mut successes = 0u64;

    for seed in 0..NUM_TRIALS {
        let params = make_params(k, SYMBOL_SIZE, repair_count);
        let encoder = backend.create_encoder(&data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_count);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, _) = scenario.channel.apply(&source, &mut rng);
        let mut all_syms: Vec<WireSymbol> = surviving;
        all_syms.extend(repairs);

        let mut decoder = backend.create_decoder(params, data.len() as u64);
        let mut decoded = false;
        for sym in &all_syms {
            if decoder.add_symbol(sym).is_some() {
                decoded = true;
                break;
            }
        }
        if decoded {
            successes += 1;
        }
    }

    (successes as f64 / NUM_TRIALS as f64 * 100.0, repair_count)
}

// ---------------------------------------------------------------------------
// Block-mode recovery — same bandwidth for all backends (RS's max_repairs is the budget)
// ---------------------------------------------------------------------------

/// All backends get the same repair budget (RS's max_repairs), answering:
/// "at equal bandwidth, which backend recovers most?" Returns (recovery%, repair_count, overhead%).
fn block_recovery_rate_same_bandwidth(backend: FecBackend, scenario: &Scenario) -> (f64, u32, f64) {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let k = (data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;
    let base_repair = (k as f64 * 0.25).ceil() as u32;

    // Determine the budget backend's max_repairs for this k
    let budget_params = make_params(k, SYMBOL_SIZE, base_repair);
    let budget_encoder = FecBackend::ReedSolomon.create_encoder(&data, budget_params);
    let budget_max = budget_encoder.max_repairs();

    // All backends get the same repair count
    let repair_count = budget_max;
    let overhead = repair_count as f64 / k as f64 * 100.0;

    let mut successes = 0u64;

    for seed in 0..NUM_TRIALS {
        let params = make_params(k, SYMBOL_SIZE, repair_count);
        let encoder = backend.create_encoder(&data, params);
        let source = encoder.source_symbols();
        let repairs = encoder.repair_symbols(repair_count);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, _) = scenario.channel.apply(&source, &mut rng);
        let mut all_syms: Vec<WireSymbol> = surviving;
        all_syms.extend(repairs);

        let mut decoder = backend.create_decoder(params, data.len() as u64);
        let mut decoded = false;
        for sym in &all_syms {
            if decoder.add_symbol(sym).is_some() {
                decoded = true;
                break;
            }
        }
        if decoded {
            successes += 1;
        }
    }

    (successes as f64 / NUM_TRIALS as f64 * 100.0, repair_count, overhead)
}

// ---------------------------------------------------------------------------
// Cross-pipeline block — same bandwidth for all backends
// ---------------------------------------------------------------------------

/// Cross-pipeline version: all backends get the same repair budget per block.
fn cross_block_recovery_same_bandwidth(backend: FecBackend, scenario: &Scenario) -> (f64, u32, f64) {
    let num_symbols = 500usize;
    let block_size = 50usize;
    let num_blocks = (num_symbols + block_size - 1) / block_size;

    // Determine the budget backend's max_repairs for k=block_size
    let probe_data: Vec<u8> = vec![0u8; block_size * SYMBOL_SIZE as usize];
    let probe_k = block_size as u32;
    let probe_repair = (probe_k as f64 * 0.25).ceil() as u32;
    let probe_params = make_params(probe_k, SYMBOL_SIZE, probe_repair);
    let probe_encoder = FecBackend::ReedSolomon.create_encoder(&probe_data, probe_params);
    let budget_max = probe_encoder.max_repairs();
    let overhead = budget_max as f64 / probe_k as f64 * 100.0;

    let mut total_blocks = 0u64;
    let mut successful_blocks = 0u64;

    for seed in 0..NUM_TRIALS {
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();

        for block_idx in 0..num_blocks {
            let start = block_idx * block_size;
            let end = (start + block_size).min(num_symbols);
            let block_packets = &packet_data[start..end];
            let k = block_packets.len() as u32;

            let block_data: Vec<u8> = block_packets.iter().flat_map(|p| p.iter().copied()).collect();

            let params = EncodingParams {
                source_symbols: k,
                symbol_size: SYMBOL_SIZE,
                repair_count: budget_max,
                block_id: block_idx as u64,
            };

            let encoder = backend.create_encoder(&block_data, params);
            let source = encoder.source_symbols();
            let repairs = encoder.repair_symbols(budget_max);

            let mut rng = ChaCha8Rng::seed_from_u64(seed * 100 + block_idx as u64);
            let (surviving, _) = scenario.channel.apply(&source, &mut rng);
            let mut all_syms: Vec<WireSymbol> = surviving;
            all_syms.extend(repairs);

            let mut decoder = backend.create_decoder(params, block_data.len() as u64);
            let mut decoded = false;
            for sym in &all_syms {
                if decoder.add_symbol(sym).is_some() {
                    decoded = true;
                    break;
                }
            }

            total_blocks += 1;
            if decoded {
                successful_blocks += 1;
            }
        }
    }

    (successful_blocks as f64 / total_blocks as f64 * 100.0, budget_max, overhead)
}

// ---------------------------------------------------------------------------
// Window-mode recovery test
// ---------------------------------------------------------------------------

fn window_recovery_rlc(scenario: &Scenario) -> f64 {
    let num_symbols = 500usize;
    let mut total_lost = 0usize;
    let mut total_recovered = 0usize;

    for seed in 0..NUM_TRIALS {
        let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();
        let sources: Vec<WireSymbol> = packet_data
            .iter()
            .map(|pkt| encoder.add_source(pkt))
            .collect();

        let repair_count = (num_symbols as f64 * scenario.stationary_loss * 2.0)
            .ceil() as usize;
        let repair_count = repair_count.max(5);
        let repairs: Vec<WireSymbol> = (0..repair_count)
            .map(|_| encoder.generate_repair())
            .collect();

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, dropped) = scenario.channel.apply(&sources, &mut rng);

        let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
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
    }

    if total_lost == 0 {
        100.0
    } else {
        total_recovered as f64 / total_lost as f64 * 100.0
    }
}

// ---------------------------------------------------------------------------
// Cross-pipeline comparison: block backends on streaming data
// ---------------------------------------------------------------------------

/// Cross-pipeline block recovery with same overhead for all backends.
fn cross_block_recovery_same_overhead(backend: FecBackend, scenario: &Scenario) -> (f64, u32) {
    let num_symbols = 500usize;
    let block_size = 50usize;
    let num_blocks = (num_symbols + block_size - 1) / block_size;

    let mut total_blocks = 0u64;
    let mut successful_blocks = 0u64;
    let mut repair_used = 0u32;

    for seed in 0..NUM_TRIALS {
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();

        for block_idx in 0..num_blocks {
            let start = block_idx * block_size;
            let end = (start + block_size).min(num_symbols);
            let block_packets = &packet_data[start..end];
            let k = block_packets.len() as u32;

            let block_data: Vec<u8> = block_packets.iter().flat_map(|p| p.iter().copied()).collect();
            let repair_count = (k as f64 * 0.25).ceil() as u32;
            repair_used = repair_count;

            let params = EncodingParams {
                source_symbols: k,
                symbol_size: SYMBOL_SIZE,
                repair_count,
                block_id: block_idx as u64,
            };

            let encoder = backend.create_encoder(&block_data, params);
            let source = encoder.source_symbols();
            let repairs = encoder.repair_symbols(repair_count);

            let mut rng = ChaCha8Rng::seed_from_u64(seed * 100 + block_idx as u64);
            let (surviving, _) = scenario.channel.apply(&source, &mut rng);
            let mut all_syms: Vec<WireSymbol> = surviving;
            all_syms.extend(repairs);

            let mut decoder = backend.create_decoder(params, block_data.len() as u64);
            let mut decoded = false;
            for sym in &all_syms {
                if decoder.add_symbol(sym).is_some() {
                    decoded = true;
                    break;
                }
            }

            total_blocks += 1;
            if decoded {
                successful_blocks += 1;
            }
        }
    }

    (successful_blocks as f64 / total_blocks as f64 * 100.0, repair_used)
}

/// Cross-pipeline block recovery with full repair budget per backend.
fn cross_block_recovery_full_budget(backend: FecBackend, scenario: &Scenario) -> (f64, u32) {
    let num_symbols = 500usize;
    let block_size = 50usize;
    let num_blocks = (num_symbols + block_size - 1) / block_size;

    let mut total_blocks = 0u64;
    let mut successful_blocks = 0u64;
    let mut repair_used = 0u32;

    for seed in 0..NUM_TRIALS {
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();

        for block_idx in 0..num_blocks {
            let start = block_idx * block_size;
            let end = (start + block_size).min(num_symbols);
            let block_packets = &packet_data[start..end];
            let k = block_packets.len() as u32;

            let block_data: Vec<u8> = block_packets.iter().flat_map(|p| p.iter().copied()).collect();
            let base_repair = (k as f64 * 0.25).ceil() as u32;

            let params = EncodingParams {
                source_symbols: k,
                symbol_size: SYMBOL_SIZE,
                repair_count: base_repair,
                block_id: block_idx as u64,
            };

            let encoder = backend.create_encoder(&block_data, params);
            let max_rep = encoder.max_repairs();
            let repair_count = if max_rep < u32::MAX { max_rep } else { base_repair };
            repair_used = repair_count;

            let source = encoder.source_symbols();
            let repairs = encoder.repair_symbols(repair_count);

            let mut rng = ChaCha8Rng::seed_from_u64(seed * 100 + block_idx as u64);
            let (surviving, _) = scenario.channel.apply(&source, &mut rng);
            let mut all_syms: Vec<WireSymbol> = surviving;
            all_syms.extend(repairs);

            let mut decoder = backend.create_decoder(params, block_data.len() as u64);
            let mut decoded = false;
            for sym in &all_syms {
                if decoder.add_symbol(sym).is_some() {
                    decoded = true;
                    break;
                }
            }

            total_blocks += 1;
            if decoded {
                successful_blocks += 1;
            }
        }
    }

    (successful_blocks as f64 / total_blocks as f64 * 100.0, repair_used)
}

// ---------------------------------------------------------------------------
// Tapered interleaving: flat vs tapered block recovery comparison
// ---------------------------------------------------------------------------

use raptorpath::net::interleave::InterleavingBuffer;
use std::time::Duration;

/// Simulate cross-block recovery with interleaving applied.
///
/// - `tapered`: if true, use tapered interleaving; else flat round-robin.
/// - Encodes multiple blocks, pushes them into InterleavingBuffer, drains, then
///   passes the interleaved symbol stream through the channel. Decodes per-block.
fn cross_block_recovery_interleaved(
    backend: FecBackend,
    scenario: &Scenario,
    tapered: bool,
) -> (f64, u32) {
    let num_symbols = 500usize;
    let block_size = 50usize;
    let num_blocks = (num_symbols + block_size - 1) / block_size;

    let mut total_blocks = 0u64;
    let mut successful_blocks = 0u64;
    let mut repair_used = 0u32;

    for seed in 0..NUM_TRIALS {
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();

        // Encode all blocks, collect (block_data_len, params, WireSymbols)
        let mut encoded_blocks: Vec<(Vec<u8>, EncodingParams, Vec<WireSymbol>)> = Vec::new();
        let depth = num_blocks.min(4);
        let mut ileave = if tapered {
            InterleavingBuffer::new_tapered(depth, Duration::from_secs(60))
        } else {
            InterleavingBuffer::new(depth, Duration::from_secs(60))
        };

        for block_idx in 0..num_blocks {
            let start = block_idx * block_size;
            let end = (start + block_size).min(num_symbols);
            let block_packets = &packet_data[start..end];
            let k = block_packets.len() as u32;

            let block_data: Vec<u8> = block_packets.iter().flat_map(|p| p.iter().copied()).collect();
            let repair_count = (k as f64 * 0.25).ceil() as u32;
            repair_used = repair_count;

            let params = EncodingParams {
                source_symbols: k,
                symbol_size: SYMBOL_SIZE,
                repair_count,
                block_id: block_idx as u64,
            };

            let encoder = backend.create_encoder(&block_data, params);
            let mut all_syms = encoder.source_symbols();
            all_syms.extend(encoder.repair_symbols(repair_count));

            encoded_blocks.push((block_data, params, Vec::new()));

            // Push into interleaving buffer (single path 0)
            ileave.push_block(block_idx as u64, vec![(0u32, all_syms)]);
        }

        // Drain all symbols through the interleaving buffer
        let batches = ileave.drain_all(scenario.stationary_loss);
        let interleaved_syms: Vec<WireSymbol> = batches
            .into_iter()
            .flat_map(|(_, syms)| syms)
            .collect();

        // Pass the interleaved stream through the channel
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, _) = scenario.channel.apply(&interleaved_syms, &mut rng);

        // Group surviving symbols by block_id and try to decode
        let mut per_block: std::collections::HashMap<u64, Vec<WireSymbol>> =
            std::collections::HashMap::new();
        for sym in surviving {
            per_block.entry(sym.block_id).or_default().push(sym);
        }

        for (block_idx, (block_data, params, _)) in encoded_blocks.iter().enumerate() {
            let block_syms = per_block.get(&(block_idx as u64)).cloned().unwrap_or_default();
            let mut decoder = backend.create_decoder(*params, block_data.len() as u64);
            let mut decoded = false;
            for sym in &block_syms {
                if decoder.add_symbol(sym).is_some() {
                    decoded = true;
                    break;
                }
            }
            total_blocks += 1;
            if decoded {
                successful_blocks += 1;
            }
        }
    }

    (successful_blocks as f64 / total_blocks as f64 * 100.0, repair_used)
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn fec_realworld_recovery_comparison() {
    let scenarios = scenarios();

    // Part 1a: Block-mode recovery — same 25% overhead for all
    let k_64kb = (65536.0f64 / SYMBOL_SIZE as f64).ceil() as u32;
    println!("\n=== Block-mode FEC Recovery — Same Overhead (64KB, k={}, 25%, {} trials) ===", k_64kb, NUM_TRIALS);
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );

    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32)> = scenarios.iter().map(|s| block_recovery_rate_same_overhead(backend, s)).collect();
        let oh = results[0].1 as f64 / k_64kb as f64 * 100.0;
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, oh
        );
    }

    // Part 1b: Block-mode recovery — full budget (each backend's natural limit)
    println!("\n=== Block-mode FEC Recovery — Full Budget (64KB, each backend's natural repair limit, {} trials) ===", NUM_TRIALS);
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );

    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32)> = scenarios.iter().map(|s| block_recovery_rate_full_budget(backend, s)).collect();
        let oh = results[0].1 as f64 / k_64kb as f64 * 100.0;
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, oh
        );
    }

    // Part 1c: Block-mode recovery — same bandwidth budget for all backends
    println!("\n=== Block-mode FEC Recovery — Same Bandwidth Budget (64KB, {} trials) ===", NUM_TRIALS);
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );

    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32, f64)> = scenarios.iter().map(|s| block_recovery_rate_same_bandwidth(backend, s)).collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, results[0].2
        );
    }

    // Part 2: Window-mode recovery
    println!(
        "\n=== Window-mode FEC Recovery (500 symbols, 2x loss overhead, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name
    );

    {
        let rates: Vec<f64> = scenarios.iter().map(|s| window_recovery_rlc(s)).collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "RLC Window", rates[0], rates[1], rates[2], rates[3]
        );
    }

    // Part 3: Cross-pipeline comparison
    let cross_k = 50u32; // block size for cross-pipeline
    println!(
        "\n=== Cross-Pipeline Block — Same Overhead (500 pkts, 50-pkt blocks, 25%, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>20} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );
    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32)> = scenarios
            .iter()
            .map(|s| cross_block_recovery_same_overhead(backend, s))
            .collect();
        let oh = results[0].1 as f64 / cross_k as f64 * 100.0;
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, oh
        );
    }

    println!(
        "\n=== Cross-Pipeline Block — Full Budget (500 pkts, 50-pkt blocks, each backend's natural limit, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>20} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );
    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32)> = scenarios
            .iter()
            .map(|s| cross_block_recovery_full_budget(backend, s))
            .collect();
        let oh = results[0].1 as f64 / cross_k as f64 * 100.0;
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, oh
        );
    }
    println!(
        "\n=== Cross-Pipeline Block — Same Bandwidth Budget (500 pkts, 50-pkt blocks, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>20} {:>12} {:>12} {:>12} {:>12} {:>8} {:>10}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name, "repairs", "overhead"
    );
    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("RLC", FecBackend::Rlc),
    ] {
        let results: Vec<(f64, u32, f64)> = scenarios
            .iter()
            .map(|s| cross_block_recovery_same_bandwidth(backend, s))
            .collect();
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}% {:>8} {:>9.0}%",
            name, results[0].0, results[1].0, results[2].0, results[3].0, results[0].1, results[0].2
        );
    }

    // Part 4: Tapered vs Flat interleaving comparison
    println!(
        "\n=== Tapered vs Flat Interleaving (500 pkts, 50-pkt blocks, 25%, {} trials) ===",
        NUM_TRIALS
    );
    println!(
        "{:>24} {:>12} {:>12} {:>12} {:>12}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name
    );
    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("RLC", FecBackend::Rlc),
    ] {
        let flat: Vec<(f64, u32)> = scenarios
            .iter()
            .map(|s| cross_block_recovery_interleaved(backend, s, false))
            .collect();
        let tapered: Vec<(f64, u32)> = scenarios
            .iter()
            .map(|s| cross_block_recovery_interleaved(backend, s, true))
            .collect();
        println!(
            "{:>20} Flat {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            name, flat[0].0, flat[1].0, flat[2].0, flat[3].0
        );
        println!(
            "{:>20} Taper {:>10.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "", tapered[0].0, tapered[1].0, tapered[2].0, tapered[3].0
        );
    }

    println!("  Window:");
    {
        let rates: Vec<f64> = scenarios.iter().map(|s| window_recovery_rlc(s)).collect();
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "RLC Window", rates[0], rates[1], rates[2], rates[3]
        );
    }
    println!();
}
