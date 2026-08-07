//! Waterfall curve comparison: all backends success rate at various loss/overhead combos.
//!
//! Run with: cargo test -p raptorpath --test fec_waterfall -- --nocapture

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{EncodingParams, FecBackend, WireSymbol};

const TRIALS: usize = 200;
const SYMBOL_SIZE: u16 = 1200;

fn make_params(num_source: u32, repair_count: u32, block_id: u64) -> EncodingParams {
    EncodingParams {
        source_symbols: num_source,
        symbol_size: SYMBOL_SIZE,
        repair_count,
        block_id,
    }
}

/// Run a single trial: encode data, simulate `loss_rate` loss on source symbols,
/// provide `overhead_pct` extra repair symbols, and return whether decode succeeded
/// plus how many symbols were fed.
fn trial(
    backend: FecBackend,
    data: &[u8],
    num_source: u32,
    loss_rate: f64,
    overhead_pct: f64,
    rng: &mut ChaCha8Rng,
) -> (bool, u32) {
    let num_repair = ((num_source as f64) * overhead_pct / 100.0).ceil() as u32;
    // Generate enough repair for all backends
    let actual_repair = match backend {
        FecBackend::RaptorQ => num_repair.max(num_source),
        FecBackend::ReedSolomon => num_repair.max(num_source),
        FecBackend::Rlc => num_repair.max(num_source),
    };
    let params = make_params(num_source, actual_repair, 0);

    let encoder = backend.create_encoder(data, params);
    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(actual_repair);

    // Simulate loss: drop each source symbol independently with probability loss_rate
    let mut available_source: Vec<&WireSymbol> = source
        .iter()
        .filter(|_| {
            let r: f64 = rng.gen();
            r >= loss_rate
        })
        .collect();

    // Shuffle to avoid ordering bias
    available_source.shuffle(rng);

    // Limit repair to overhead_pct of source symbols
    let repair_budget = num_repair as usize;
    let available_repair: Vec<&WireSymbol> = repair.iter().take(repair_budget).collect();

    let mut decoder = backend.create_decoder(params, data.len() as u64);
    let mut fed = 0u32;

    // Feed surviving source symbols first
    for sym in &available_source {
        fed += 1;
        if decoder.add_symbol(sym).is_some() {
            return (true, fed);
        }
    }

    // Feed repair symbols
    for sym in &available_repair {
        fed += 1;
        if decoder.add_symbol(sym).is_some() {
            return (true, fed);
        }
    }

    (decoder.is_decoded(), fed)
}

use rand::Rng;

struct BackendResult {
    name: &'static str,
    success_count: u32,
    total_syms: u64,
}

fn all_backends() -> Vec<(&'static str, FecBackend)> {
    vec![
        ("raptorq", FecBackend::RaptorQ),
        ("rs", FecBackend::ReedSolomon),
        ("rlc", FecBackend::Rlc),
    ]
}

#[test]
fn waterfall_comparison_small_block() {
    let num_source = 10u32;
    let data_len = num_source as usize * SYMBOL_SIZE as usize;
    let data: Vec<u8> = (0..data_len).map(|i| (i % 256) as u8).collect();

    let loss_rates = [0.01, 0.02, 0.05, 0.10, 0.15, 0.20];
    let overhead_pcts = [10.0, 20.0, 30.0, 50.0, 75.0, 100.0];
    let backends = all_backends();

    println!();
    println!("=== Waterfall comparison: {} source symbols, {} byte symbols ===", num_source, SYMBOL_SIZE);

    // Print header
    print!("{:<6} {:<10}", "loss%", "overhead%");
    for (name, _) in &backends {
        print!(" {:<14}", format!("{name}_ok%"));
    }
    println!();

    for &loss_rate in &loss_rates {
        for &overhead_pct in &overhead_pcts {
            let mut results: Vec<BackendResult> = backends
                .iter()
                .map(|(name, _)| BackendResult {
                    name,
                    success_count: 0,
                    total_syms: 0,
                })
                .collect();

            for t in 0..TRIALS {
                let seed = (loss_rate * 10000.0) as u64 * 1_000_000
                    + (overhead_pct * 100.0) as u64 * 1_000
                    + t as u64;

                for (i, (_name, backend)) in backends.iter().enumerate() {
                    let mut rng = ChaCha8Rng::seed_from_u64(seed);
                    let (ok, fed) = trial(*backend, &data, num_source, loss_rate, overhead_pct, &mut rng);
                    if ok {
                        results[i].success_count += 1;
                    }
                    results[i].total_syms += fed as u64;
                }
            }

            print!(
                "{:<6} {:<10}",
                (loss_rate * 100.0) as u32,
                overhead_pct as u32,
            );
            for r in &results {
                let pct = r.success_count as f64 / TRIALS as f64 * 100.0;
                print!(" {:<14.1}", pct);
            }
            println!();
        }
    }
}

#[test]
fn waterfall_comparison_large_block() {
    let num_source = 50u32;
    let data_len = num_source as usize * SYMBOL_SIZE as usize;
    let data: Vec<u8> = (0..data_len).map(|i| (i % 256) as u8).collect();

    let loss_rates = [0.05, 0.10, 0.20];
    let overhead_pcts = [20.0, 50.0, 100.0];
    let backends = all_backends();

    println!();
    println!("=== Waterfall comparison: {} source symbols, {} byte symbols ===", num_source, SYMBOL_SIZE);

    print!("{:<6} {:<10}", "loss%", "overhead%");
    for (name, _) in &backends {
        print!(" {:<14}", format!("{name}_ok%"));
    }
    println!();

    for &loss_rate in &loss_rates {
        for &overhead_pct in &overhead_pcts {
            let mut results: Vec<BackendResult> = backends
                .iter()
                .map(|(name, _)| BackendResult {
                    name,
                    success_count: 0,
                    total_syms: 0,
                })
                .collect();

            for t in 0..TRIALS {
                let seed = 1_000_000_000
                    + (loss_rate * 10000.0) as u64 * 1_000_000
                    + (overhead_pct * 100.0) as u64 * 1_000
                    + t as u64;

                for (i, (_name, backend)) in backends.iter().enumerate() {
                    let mut rng = ChaCha8Rng::seed_from_u64(seed);
                    let (ok, fed) = trial(*backend, &data, num_source, loss_rate, overhead_pct, &mut rng);
                    if ok {
                        results[i].success_count += 1;
                    }
                    results[i].total_syms += fed as u64;
                }
            }

            print!(
                "{:<6} {:<10}",
                (loss_rate * 100.0) as u32,
                overhead_pct as u32,
            );
            for r in &results {
                let pct = r.success_count as f64 / TRIALS as f64 * 100.0;
                print!(" {:<14.1}", pct);
            }
            println!();
        }
    }
}
