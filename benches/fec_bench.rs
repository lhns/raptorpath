use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_encode");
    for size in [1024, 4096, 16384, 65536] {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| {
                let params = raptorpath::fec::EncodingParams {
                    source_symbols: (data.len() as f64 / 1200.0).ceil() as u32,
                    symbol_size: 1200,
                    repair_count: 10,
                    block_id: 0,
                };
                let encoder = raptorpath::fec::Encoder::new(data, params);
                let _source = encoder.source_symbols();
                let _repair = encoder.repair_symbols(10);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_encode);
criterion_main!(benches);
