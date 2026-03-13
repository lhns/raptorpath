use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mettle::{MettleConfig, MettleDecoder, MettleEncoder};

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("mettle_encode");
    let config = MettleConfig::small_window();

    for num_packets in [10, 50, 100, 500] {
        let packets: Vec<Vec<u8>> = (0..num_packets).map(|i| vec![(i % 256) as u8; 1200]).collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_packets),
            &packets,
            |b, packets| {
                b.iter(|| {
                    let mut encoder = MettleEncoder::new(config, 42);
                    for pkt in packets {
                        encoder.add_source_packet(pkt);
                    }
                    let _coded = encoder.coded_packets();
                });
            },
        );
    }
    group.finish();
}

fn bench_decode_no_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("mettle_decode_no_loss");
    let config = MettleConfig::small_window();

    for num_packets in [10, 50, 100] {
        let packets: Vec<Vec<u8>> = (0..num_packets).map(|i| vec![(i % 256) as u8; 1200]).collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_packets),
            &packets,
            |b, packets| {
                b.iter(|| {
                    let mut decoder = MettleDecoder::new(config, packets.len(), 42);
                    for (i, pkt) in packets.iter().enumerate() {
                        decoder.add_source_packet(i, pkt);
                    }
                    assert!(decoder.is_complete());
                });
            },
        );
    }
    group.finish();
}

fn bench_decode_with_peeling(c: &mut Criterion) {
    let mut group = c.benchmark_group("mettle_decode_peeling");
    let config = MettleConfig::small_window();

    for num_packets in [10, 50, 100] {
        let packets: Vec<Vec<u8>> = (0..num_packets).map(|i| vec![(i % 256) as u8; 1200]).collect();

        // Pre-encode
        let mut encoder = MettleEncoder::new(config, 42);
        for pkt in &packets {
            encoder.add_source_packet(pkt);
        }
        let coded = encoder.coded_packets();

        // Drop 5% of source packets
        let drop_every = 20;

        group.bench_with_input(
            BenchmarkId::from_parameter(num_packets),
            &(packets.clone(), coded),
            |b, (packets, coded)| {
                b.iter(|| {
                    let mut decoder = MettleDecoder::new(config, packets.len(), 42);
                    for (i, pkt) in packets.iter().enumerate() {
                        if i % drop_every != 0 {
                            decoder.add_source_packet(i, pkt);
                        }
                    }
                    for cp in coded {
                        decoder.add_coded_packet(cp);
                        if decoder.is_complete() {
                            break;
                        }
                    }
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_encode, bench_decode_no_loss, bench_decode_with_peeling);
criterion_main!(benches);
