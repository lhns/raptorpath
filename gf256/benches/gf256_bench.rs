use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use gf256::{mul_acc_slice, mul_slice};

fn bench_mul_acc(c: &mut Criterion) {
    let sizes = [64, 512, 1200, 4096];
    let mut group = c.benchmark_group("mul_acc_slice");

    for &size in &sizes {
        let src: Vec<u8> = (0..size).map(|i| (i * 37 + 13) as u8).collect();
        let mut dst = vec![0u8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                mul_acc_slice(42, &src, &mut dst);
            });
        });
    }
    group.finish();
}

fn bench_mul_slice(c: &mut Criterion) {
    let sizes = [64, 512, 1200, 4096];
    let mut group = c.benchmark_group("mul_slice");

    for &size in &sizes {
        let src: Vec<u8> = (0..size).map(|i| (i * 37 + 13) as u8).collect();
        let mut dst = vec![0u8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                mul_slice(42, &src, &mut dst);
            });
        });
    }
    group.finish();
}

fn bench_xor_acc(c: &mut Criterion) {
    let sizes = [64, 512, 1200, 4096];
    let mut group = c.benchmark_group("xor_acc_coeff1");

    for &size in &sizes {
        let src: Vec<u8> = (0..size).map(|i| (i * 37 + 13) as u8).collect();
        let mut dst = vec![0u8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                mul_acc_slice(1, &src, &mut dst);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_mul_acc, bench_mul_slice, bench_xor_acc);
criterion_main!(benches);
