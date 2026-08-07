use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use raptorpath::fec::{
    EncodingParams, FecBackend,
    RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder, WireSymbol,
};

fn make_params(data_len: usize, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / 1200.0).ceil() as u32,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    }
}

const ALL_BACKENDS: [(&str, FecBackend); 3] = [
    ("raptorq", FecBackend::RaptorQ),
    ("rs", FecBackend::ReedSolomon),
    ("rlc", FecBackend::Rlc),
];

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_encode");
    for (backend_name, backend) in ALL_BACKENDS {
        for size in [1024, 4096, 16384, 65536] {
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let id = format!("{backend_name}/{size}");
            group.bench_with_input(BenchmarkId::from_parameter(&id), &data, |b, data| {
                b.iter(|| {
                    let params = make_params(data.len(), 10);
                    let encoder = backend.create_encoder(data, params);
                    let _source = encoder.source_symbols();
                    let _repair = encoder.repair_symbols(10);
                });
            });
        }
    }
    group.finish();
}

fn bench_decode_no_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_decode_no_loss");
    for (backend_name, backend) in ALL_BACKENDS {
        for size in [1024, 4096, 16384, 65536] {
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let params = make_params(data.len(), 10);
            let encoder = backend.create_encoder(&data, params);
            let source = encoder.source_symbols();

            let id = format!("{backend_name}/{size}");
            group.bench_with_input(BenchmarkId::from_parameter(&id), &source, |b, source| {
                b.iter(|| {
                    let mut decoder = backend.create_decoder(params, data.len() as u64);
                    for sym in source {
                        if decoder.add_symbol(sym).is_some() {
                            break;
                        }
                    }
                });
            });
        }
    }
    group.finish();
}

fn bench_decode_5pct_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_decode_5pct_loss");
    for (backend_name, backend, repair_count) in [
        ("raptorq", FecBackend::RaptorQ, 20u32),
        ("rs", FecBackend::ReedSolomon, 10u32),
        ("rlc", FecBackend::Rlc, 15u32),
    ] {
        for size in [4096, 16384, 65536] {
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let params = make_params(data.len(), repair_count);
            let encoder = backend.create_encoder(&data, params);
            let source = encoder.source_symbols();
            let repair = encoder.repair_symbols(repair_count);

            // Drop every 20th source symbol (5% loss)
            let mut transmitted: Vec<WireSymbol> = source
                .into_iter()
                .enumerate()
                .filter(|(i, _)| i % 20 != 0)
                .map(|(_, s)| s)
                .collect();
            transmitted.extend(repair);

            let id = format!("{backend_name}/{size}");
            group.bench_with_input(BenchmarkId::from_parameter(&id), &transmitted, |b, transmitted| {
                b.iter(|| {
                    let mut decoder = backend.create_decoder(params, data.len() as u64);
                    for sym in transmitted {
                        if decoder.add_symbol(sym).is_some() {
                            break;
                        }
                    }
                });
            });
        }
    }
    group.finish();
}

fn bench_per_symbol_ns(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_per_symbol_ns");
    let size = 16384;
    for (backend_name, backend, repair_count) in [
        ("raptorq", FecBackend::RaptorQ, 20u32),
        ("rs", FecBackend::ReedSolomon, 10u32),
        ("rlc", FecBackend::Rlc, 15u32),
    ] {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let params = make_params(data.len(), repair_count);
        let encoder = backend.create_encoder(&data, params);
        let source = encoder.source_symbols();
        let repair = encoder.repair_symbols(repair_count);
        let mut all: Vec<WireSymbol> = source;
        all.extend(repair);

        group.bench_with_input(BenchmarkId::from_parameter(backend_name), &all, |b, all| {
            b.iter(|| {
                let mut decoder = backend.create_decoder(params, data.len() as u64);
                for sym in all {
                    let _ = decoder.add_symbol(sym);
                }
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Window-mode benchmarks
// ---------------------------------------------------------------------------

const WINDOW_SYMBOL_SIZE: u16 = 1200;

fn bench_window_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("window_encode");
    let num_symbols = 100;
    let packet_data: Vec<Vec<u8>> = (0..num_symbols)
        .map(|i| vec![(i % 256) as u8; 1000])
        .collect();

    group.bench_function("rlc", |b| {
        b.iter(|| {
            let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
            for pkt in &packet_data {
                encoder.add_source(pkt);
            }
            for _ in 0..10 {
                encoder.generate_repair();
            }
        });
    });

    group.finish();
}

fn bench_window_decode_no_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("window_decode_no_loss");
    let num_symbols = 100;

    // Pre-generate RLC source symbols
    let rlc_syms: Vec<WireSymbol> = {
        let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
        (0..num_symbols)
            .map(|i| encoder.add_source(&vec![(i % 256) as u8; 1000]))
            .collect()
    };

    group.bench_with_input(BenchmarkId::from_parameter("rlc"), &rlc_syms, |b, syms| {
        b.iter(|| {
            let mut decoder = RlcWindowDecoder::new(WINDOW_SYMBOL_SIZE);
            for sym in syms {
                decoder.add_symbol(sym);
            }
        });
    });

    group.finish();
}

fn bench_window_decode_with_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("window_decode_10pct_loss");
    let num_symbols = 100;

    // RLC: 10% loss + repair
    let (rlc_transmitted,): (Vec<WireSymbol>,) = {
        let mut encoder = RlcWindowEncoder::new(WINDOW_SYMBOL_SIZE);
        let sources: Vec<WireSymbol> = (0..num_symbols)
            .map(|i| encoder.add_source(&vec![(i % 256) as u8; 1000]))
            .collect();
        let repairs: Vec<WireSymbol> = (0..15).map(|_| encoder.generate_repair()).collect();
        let mut transmitted: Vec<WireSymbol> = sources
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 10 != 0)
            .map(|(_, s)| s)
            .collect();
        transmitted.extend(repairs);
        (transmitted,)
    };

    group.bench_with_input(
        BenchmarkId::from_parameter("rlc"),
        &rlc_transmitted,
        |b, syms| {
            b.iter(|| {
                let mut decoder = RlcWindowDecoder::new(WINDOW_SYMBOL_SIZE);
                for sym in syms {
                    decoder.add_symbol(sym);
                }
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_encode,
    bench_decode_no_loss,
    bench_decode_5pct_loss,
    bench_per_symbol_ns,
    bench_window_encode,
    bench_window_decode_no_loss,
    bench_window_decode_with_loss,
);
criterion_main!(benches);
