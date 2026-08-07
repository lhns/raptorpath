use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{
    EncodingParams, FecBackend, RlcWindowDecoder,
    RlcWindowEncoder, WindowDecoder,
    WindowEncoder, WireSymbol,
};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Gilbert-Elliott channel simulator (generates loss, not estimates)
// ---------------------------------------------------------------------------

struct GilbertElliottChannel {
    p_gb: f64,   // Good→Bad transition probability
    p_bg: f64,   // Bad→Good transition probability
    loss_good: f64, // Loss probability in Good state
    loss_bad: f64,  // Loss probability in Bad state
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
            // State transition
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

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

struct Scenario {
    name: &'static str,
    channel: GilbertElliottChannel,
    stationary_loss: f64,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "datacenter",
            channel: GilbertElliottChannel {
                p_gb: 0.0,
                p_bg: 1.0,
                loss_good: 0.001,
                loss_bad: 0.0,
            },
            stationary_loss: 0.001,
        },
        Scenario {
            name: "wifi_home",
            channel: GilbertElliottChannel {
                p_gb: 0.03,
                p_bg: 0.5,
                loss_good: 0.01,
                loss_bad: 0.3,
            },
            stationary_loss: 0.025,
        },
        Scenario {
            name: "lte_mobile",
            channel: GilbertElliottChannel {
                p_gb: 0.02,
                p_bg: 0.25,
                loss_good: 0.005,
                loss_bad: 0.4,
            },
            stationary_loss: 0.035,
        },
        Scenario {
            name: "congested_wifi",
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
// Helpers
// ---------------------------------------------------------------------------

fn make_params(data_len: usize, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / 1200.0).ceil() as u32,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    }
}

fn repair_count_for_scenario(stationary_loss: f64, k: u32) -> u32 {
    // Target ~99% recovery: overhead = max(loss * 2.5, 3)
    let base = (k as f64 * stationary_loss * 2.5).ceil() as u32;
    let min_repair = 3u32;
    base.max(min_repair)
}

// ---------------------------------------------------------------------------
// Block-mode benchmarks
// ---------------------------------------------------------------------------

fn bench_block_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_encode_realworld");
    group.sample_size(30);
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();

    for scenario in scenarios() {
        for (backend_name, backend) in [
            ("raptorq", FecBackend::RaptorQ),
            ("rs", FecBackend::ReedSolomon),
            ("rlc", FecBackend::Rlc),
        ] {
            let k = (data.len() as f64 / 1200.0).ceil() as u32;
            let repair = repair_count_for_scenario(scenario.stationary_loss, k);
            let params = make_params(data.len(), repair);
            let id = format!("{}/{}", scenario.name, backend_name);

            group.bench_with_input(BenchmarkId::from_parameter(&id), &data, |b, data| {
                b.iter(|| {
                    let encoder = backend.create_encoder(data, params);
                    let _source = encoder.source_symbols();
                    let _repair = encoder.repair_symbols(repair);
                });
            });
        }
    }
    group.finish();
}

fn bench_block_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_decode_realworld");
    group.sample_size(30);
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();

    for scenario in scenarios() {
        for (backend_name, backend) in [
            ("raptorq", FecBackend::RaptorQ),
            ("rs", FecBackend::ReedSolomon),
            ("rlc", FecBackend::Rlc),
        ] {
            let k = (data.len() as f64 / 1200.0).ceil() as u32;
            let repair = repair_count_for_scenario(scenario.stationary_loss, k);
            let params = make_params(data.len(), repair);
            let encoder = backend.create_encoder(&data, params);
            let source = encoder.source_symbols();
            let repair_syms = encoder.repair_symbols(repair);

            // Apply channel once to get a representative set
            let mut rng = ChaCha8Rng::seed_from_u64(0);
            let mut all_syms: Vec<WireSymbol> = Vec::new();
            let (surviving, _) = scenario.channel.apply(&source, &mut rng);
            all_syms.extend(surviving);
            all_syms.extend(repair_syms);

            let id = format!("{}/{}", scenario.name, backend_name);
            group.bench_with_input(
                BenchmarkId::from_parameter(&id),
                &all_syms,
                |b, syms| {
                    b.iter(|| {
                        let mut decoder = backend.create_decoder(params, data.len() as u64);
                        for sym in syms {
                            if decoder.add_symbol(sym).is_some() {
                                break;
                            }
                        }
                    });
                },
            );
        }
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Window-mode benchmarks
// ---------------------------------------------------------------------------

const WINDOW_SYMBOL_SIZE: u16 = 1200;

fn bench_window_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("window_encode_realworld");
    group.sample_size(30);
    let num_symbols = 200;
    let packet_data: Vec<Vec<u8>> = (0..num_symbols)
        .map(|i| vec![(i % 256) as u8; 1000])
        .collect();

    for scenario in scenarios() {
        let repair_per_source =
            (scenario.stationary_loss * 2.0).max(0.05);
        let num_repairs = (num_symbols as f64 * repair_per_source).ceil() as usize;

        // RLC
        let id = format!("{}/rlc", scenario.name);
        group.bench_function(BenchmarkId::from_parameter(&id), |b| {
            b.iter(|| {
                let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
                for pkt in &packet_data {
                    encoder.add_source(pkt);
                }
                for _ in 0..num_repairs {
                    encoder.generate_repair();
                }
            });
        });

    }
    group.finish();
}

fn bench_window_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("window_decode_realworld");
    group.sample_size(30);
    let num_symbols = 200usize;
    let packet_data: Vec<Vec<u8>> = (0..num_symbols)
        .map(|i| vec![(i % 256) as u8; 1000])
        .collect();

    for scenario in scenarios() {
        let repair_per_source = (scenario.stationary_loss * 2.0).max(0.05);
        let num_repairs = (num_symbols as f64 * repair_per_source).ceil() as usize;

        // RLC window decode
        {
            let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
            let sources: Vec<WireSymbol> = packet_data
                .iter()
                .map(|pkt| encoder.add_source(pkt))
                .collect();
            let repairs: Vec<WireSymbol> =
                (0..num_repairs).map(|_| encoder.generate_repair()).collect();

            let mut rng = ChaCha8Rng::seed_from_u64(0);
            let (surviving, _) = scenario.channel.apply(&sources, &mut rng);
            let mut transmitted: Vec<WireSymbol> = surviving;
            transmitted.extend(repairs);

            let id = format!("{}/rlc", scenario.name);
            group.bench_with_input(
                BenchmarkId::from_parameter(&id),
                &transmitted,
                |b, syms| {
                    b.iter(|| {
                        let mut decoder = RlcWindowDecoder::new(WINDOW_SYMBOL_SIZE);
                        for sym in syms {
                            decoder.add_symbol(sym);
                        }
                    });
                },
            );
        }

    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Cross-pipeline comparison
// ---------------------------------------------------------------------------

fn bench_cross_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_pipeline");
    group.sample_size(20);
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let packet_data: Vec<Vec<u8>> = data.chunks(1200).map(|c| c.to_vec()).collect();

    for scenario in scenarios() {
        // Block backends
        for (backend_name, backend) in [
            ("raptorq", FecBackend::RaptorQ),
            ("rs", FecBackend::ReedSolomon),
            ("rlc", FecBackend::Rlc),
        ] {
            let k = (data.len() as f64 / 1200.0).ceil() as u32;
            let repair = repair_count_for_scenario(scenario.stationary_loss, k);
            let params = make_params(data.len(), repair);

            let id = format!("{}/block_{}", scenario.name, backend_name);
            group.bench_function(BenchmarkId::from_parameter(&id), |b| {
                let mut rng = ChaCha8Rng::seed_from_u64(0);
                b.iter(|| {
                    let encoder = backend.create_encoder(&data, params);
                    let source = encoder.source_symbols();
                    let repair_syms = encoder.repair_symbols(repair);

                    let mut rng2 = rng.clone();
                    let (surviving, _) = scenario.channel.apply(&source, &mut rng2);
                    let mut all: Vec<WireSymbol> = surviving;
                    all.extend(repair_syms);

                    let mut decoder = backend.create_decoder(params, data.len() as u64);
                    for sym in &all {
                        if decoder.add_symbol(sym).is_some() {
                            break;
                        }
                    }
                });
            });
        }

        // Window backends
        {
            let num_repairs = (packet_data.len() as f64 * scenario.stationary_loss * 2.0)
                .ceil() as usize;
            let num_repairs = num_repairs.max(5);

            // RLC window
            let id = format!("{}/window_rlc", scenario.name);
            group.bench_function(BenchmarkId::from_parameter(&id), |b| {
                b.iter(|| {
                    let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
                    let sources: Vec<WireSymbol> = packet_data
                        .iter()
                        .map(|pkt| encoder.add_source(pkt))
                        .collect();
                    let repairs: Vec<WireSymbol> =
                        (0..num_repairs).map(|_| encoder.generate_repair()).collect();

                    let mut rng = ChaCha8Rng::seed_from_u64(0);
                    let (surviving, _) = scenario.channel.apply(&sources, &mut rng);
                    let mut all: Vec<WireSymbol> = surviving;
                    all.extend(repairs);

                    let mut decoder = RlcWindowDecoder::new(WINDOW_SYMBOL_SIZE);
                    for sym in &all {
                        decoder.add_symbol(sym);
                    }
                });
            });

        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_block_encode,
    bench_block_decode,
    bench_window_encode,
    bench_window_decode,
    bench_cross_pipeline,
);
criterion_main!(benches);
