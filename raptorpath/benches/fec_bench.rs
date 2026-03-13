use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use raptorpath::fec::{EncodingParams, FecBackend};

fn make_params(data_len: usize, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / 1200.0).ceil() as u32,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    }
}

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_encode");
    for (backend_name, backend) in [("raptorq", FecBackend::RaptorQ), ("mettle", FecBackend::Mettle)] {
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

criterion_group!(benches, bench_encode);
criterion_main!(benches);
