use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use raptorpath::fec::{
    RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder, WireSymbol,
};

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
    bench_window_encode,
    bench_window_decode_no_loss,
    bench_window_decode_with_loss,
);
criterion_main!(benches);
