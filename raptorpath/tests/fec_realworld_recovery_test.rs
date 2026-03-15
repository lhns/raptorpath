//! Real-world FEC recovery rate comparison across Gilbert-Elliott channel scenarios.
//!
//! Run with: cargo test --test fec_realworld_recovery_test -- --nocapture

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{
    EncodingParams, FecBackend, MettleWindowDecoder, MettleWindowEncoder, RlcWindowDecoder,
    RlcWindowEncoder, StreamingDecoder, StreamingEncoder, StreamingParams, WindowDecoder,
    WindowEncoder, WireSymbol,
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

fn block_recovery_rate(backend: FecBackend, scenario: &Scenario) -> f64 {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let k = (data.len() as f64 / SYMBOL_SIZE as f64).ceil() as u32;
    // 25% overhead repair
    let repair_count = (k as f64 * 0.25).ceil() as u32;
    // METTLE needs more overhead
    let repair_count = if backend == FecBackend::Mettle {
        repair_count * 2
    } else {
        repair_count
    };

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

    successes as f64 / NUM_TRIALS as f64 * 100.0
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

fn window_recovery_mettle(scenario: &Scenario) -> f64 {
    let num_symbols = 500usize;
    let mut total_lost = 0usize;
    let mut total_recovered = 0usize;

    for seed in 0..NUM_TRIALS {
        let mut encoder = MettleWindowEncoder::new(
            mettle::MettleConfig::small_window(),
            SYMBOL_SIZE,
            seed,
        );
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();
        let sources: Vec<WireSymbol> = packet_data
            .iter()
            .map(|pkt| encoder.add_source(pkt))
            .collect();

        let repair_count = (num_symbols as f64 * scenario.stationary_loss * 3.0)
            .ceil() as usize;
        let repair_count = repair_count.max(10);
        let repairs: Vec<WireSymbol> = (0..repair_count)
            .map(|_| encoder.generate_repair())
            .collect();

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, dropped) = scenario.channel.apply(&sources, &mut rng);

        let mut decoder = MettleWindowDecoder::new(SYMBOL_SIZE);
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

fn window_recovery_streaming(scenario: &Scenario) -> f64 {
    let num_symbols = 500usize;
    let mut total_lost = 0usize;
    let mut total_recovered = 0usize;

    let mean_burst = 1.0 / scenario.channel.p_bg.max(0.01);
    let sparams = StreamingParams::from_channel(mean_burst, scenario.stationary_loss, 1.15);

    for seed in 0..NUM_TRIALS {
        let mut encoder = StreamingEncoder::new(SYMBOL_SIZE, sparams);
        let packet_data: Vec<Vec<u8>> = (0..num_symbols)
            .map(|i| vec![(i % 256) as u8; 1000])
            .collect();

        let mut all_syms: Vec<(bool, WireSymbol)> = Vec::new();
        for pkt in &packet_data {
            let src = encoder.add_source(pkt);
            all_syms.push((false, src));
            // Interleave repairs
            let total_rate = sparams.total_rate();
            if total_rate > 0.0 {
                let repairs_per_source = total_rate.ceil() as usize;
                for _ in 0..repairs_per_source {
                    all_syms.push((true, encoder.generate_repair()));
                }
            }
        }

        // Separate sources and repairs for channel application
        let source_indices: Vec<usize> = all_syms
            .iter()
            .enumerate()
            .filter(|(_, (is_repair, _))| !is_repair)
            .map(|(i, _)| i)
            .collect();
        let sources: Vec<WireSymbol> = source_indices
            .iter()
            .map(|&i| all_syms[i].1.clone())
            .collect();
        let repairs: Vec<WireSymbol> = all_syms
            .iter()
            .filter(|(is_repair, _)| *is_repair)
            .map(|(_, sym)| sym.clone())
            .collect();

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let (surviving, dropped) = scenario.channel.apply(&sources, &mut rng);

        let mut decoder = StreamingDecoder::new(SYMBOL_SIZE, sparams);
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

fn cross_block_recovery(backend: FecBackend, scenario: &Scenario) -> f64 {
    let num_symbols = 500usize;
    let block_size = 50usize; // ~10 blocks
    let num_blocks = (num_symbols + block_size - 1) / block_size;

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

            // Concatenate into a block
            let block_data: Vec<u8> = block_packets.iter().flat_map(|p| p.iter().copied()).collect();
            let repair_count = (k as f64 * 0.25).ceil() as u32;
            let repair_count = if backend == FecBackend::Mettle {
                repair_count * 2
            } else {
                repair_count
            };

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

    successful_blocks as f64 / total_blocks as f64 * 100.0
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn fec_realworld_recovery_comparison() {
    let scenarios = scenarios();

    // Part 1: Block-mode recovery
    println!("\n=== Block-mode FEC Recovery (64KB, 25% overhead, {} trials) ===", NUM_TRIALS);
    println!(
        "{:>16} {:>12} {:>12} {:>12} {:>12}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name
    );

    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("METTLE", FecBackend::Mettle),
        ("RLC", FecBackend::Rlc),
    ] {
        let rates: Vec<f64> = scenarios.iter().map(|s| block_recovery_rate(backend, s)).collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            name, rates[0], rates[1], rates[2], rates[3]
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
    {
        let rates: Vec<f64> = scenarios.iter().map(|s| window_recovery_mettle(s)).collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "METTLE Window", rates[0], rates[1], rates[2], rates[3]
        );
    }
    {
        let rates: Vec<f64> = scenarios
            .iter()
            .map(|s| window_recovery_streaming(s))
            .collect();
        println!(
            "{:>16} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "Streaming", rates[0], rates[1], rates[2], rates[3]
        );
    }

    // Part 3: Cross-pipeline comparison
    println!(
        "\n=== Cross-Pipeline FEC Comparison (500 pkts, GE channel, {} trials avg) ===",
        NUM_TRIALS
    );
    println!(
        "{:>20} {:>12} {:>12} {:>12} {:>12}",
        "", scenarios[0].name, scenarios[1].name, scenarios[2].name, scenarios[3].name
    );
    println!("  Block:");
    for (name, backend) in [
        ("RaptorQ", FecBackend::RaptorQ),
        ("Reed-Solomon", FecBackend::ReedSolomon),
        ("METTLE", FecBackend::Mettle),
        ("RLC", FecBackend::Rlc),
    ] {
        let rates: Vec<f64> = scenarios
            .iter()
            .map(|s| cross_block_recovery(backend, s))
            .collect();
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            name, rates[0], rates[1], rates[2], rates[3]
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
    {
        let rates: Vec<f64> = scenarios.iter().map(|s| window_recovery_mettle(s)).collect();
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "METTLE Window", rates[0], rates[1], rates[2], rates[3]
        );
    }
    {
        let rates: Vec<f64> = scenarios
            .iter()
            .map(|s| window_recovery_streaming(s))
            .collect();
        println!(
            "{:>20} {:>11.1}% {:>11.1}% {:>11.1}% {:>11.1}%",
            "Streaming", rates[0], rates[1], rates[2], rates[3]
        );
    }
    println!();
}
