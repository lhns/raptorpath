//! Integration tests for streaming codes encoder/decoder.

use raptorpath::fec::{StreamingDecoder, StreamingEncoder, StreamingParams, WindowDecoder, WindowEncoder};

const SYMBOL_SIZE: u16 = 128;

fn make_params(t: u32, b: u32, epsilon: f64) -> StreamingParams {
    StreamingParams {
        t,
        b,
        epsilon,
        burst_rate: 1.0 / t as f64,
        random_rate: if epsilon > 0.001 {
            epsilon / (1.0 - epsilon)
        } else {
            0.0
        },
    }
}

/// Feed a bursty channel (GE-style) through streaming encoder/decoder.
/// Compare recovery rate against a baseline with no FEC.
#[test]
fn test_bursty_channel_recovery() {
    let params = make_params(6, 3, 0.05);
    let mut enc = StreamingEncoder::new(SYMBOL_SIZE, params);
    let mut dec = StreamingDecoder::new(SYMBOL_SIZE, params);

    let num_symbols = 100;
    let mut all_sources = Vec::new();
    let mut all_repairs = Vec::new();

    for i in 0..num_symbols as u64 {
        let mut data = vec![0u8; SYMBOL_SIZE as usize];
        // Write sequence number into the first 8 bytes
        data[..8].copy_from_slice(&i.to_le_bytes());
        let src = enc.add_source(&data);
        all_sources.push((i, src, data));

        // Generate repairs at ~2x the total rate for good coverage
        for _ in 0..2 {
            all_repairs.push(enc.generate_repair());
        }
    }

    // Simulate GE-style bursty channel:
    // Good state: 0.5% loss, Bad state: 30% loss
    // Burst at symbols 20-23 and 60-63
    let mut dropped = std::collections::BTreeSet::new();
    for i in 20..=23 {
        dropped.insert(i as u64);
    }
    for i in 60..=63 {
        dropped.insert(i as u64);
    }
    // Random drops
    for i in [7u64, 35, 50, 82, 91] {
        dropped.insert(i);
    }

    let total_dropped = dropped.len();

    // Feed non-dropped sources
    let mut total_recovered = Vec::new();
    for (i, src, _) in &all_sources {
        if dropped.contains(i) {
            continue;
        }
        let r = dec.add_symbol(src);
        total_recovered.extend(r);
    }

    // Feed all repairs
    for repair in &all_repairs {
        let r = dec.add_symbol(repair);
        total_recovered.extend(r);
    }

    let recovered_seqs: std::collections::BTreeSet<u64> =
        total_recovered.iter().map(|(s, _)| *s).collect();

    // Count how many dropped symbols were recovered
    let recovered_dropped = dropped
        .iter()
        .filter(|s| recovered_seqs.contains(s))
        .count();

    println!(
        "Streaming codes: recovered {recovered_dropped}/{total_dropped} dropped symbols \
         ({:.0}% recovery rate)",
        100.0 * recovered_dropped as f64 / total_dropped as f64
    );

    // With 2 repairs per source and T=6/B=3, we should recover a good fraction
    assert!(
        recovered_dropped >= total_dropped / 2,
        "Should recover at least half of dropped symbols, got {recovered_dropped}/{total_dropped}"
    );
}

/// Verify that data integrity is maintained through encode/decode.
#[test]
fn test_data_integrity() {
    let params = make_params(4, 2, 0.05);
    let mut enc = StreamingEncoder::new(SYMBOL_SIZE, params);
    let mut dec = StreamingDecoder::new(SYMBOL_SIZE, params);

    let mut original_data = Vec::new();

    for i in 0..50u64 {
        let mut data = vec![0u8; SYMBOL_SIZE as usize];
        data[..8].copy_from_slice(&i.to_le_bytes());
        // Fill rest with pattern
        for j in 8..SYMBOL_SIZE as usize {
            data[j] = ((i * 7 + j as u64) & 0xFF) as u8;
        }
        original_data.push((i, data.clone()));

        let src = enc.add_source(&data);
        let recovered = dec.add_symbol(&src);

        // Source symbol should be immediately delivered
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].0, i);
        assert_eq!(&recovered[0].1[..], &data[..]);
    }
}

/// Test window advancement evicts old state properly.
#[test]
fn test_window_advance() {
    let params = make_params(4, 2, 0.0);
    let mut enc = StreamingEncoder::new(SYMBOL_SIZE, params);

    for i in 0..20u64 {
        let data = vec![(i & 0xFF) as u8; SYMBOL_SIZE as usize];
        enc.add_source(&data);
    }

    assert_eq!(enc.window_size(), 20);

    enc.advance(10);
    assert_eq!(enc.window_size(), 10);

    let (start, end) = enc.window_span();
    assert_eq!(start, 10);
    assert_eq!(end, 19);
}
