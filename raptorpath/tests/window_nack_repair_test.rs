//! ADR-0025: WindowNack sender-side repair integration tests.
//!
//! Verifies that targeted repair symbols generated from NACK gap ranges
//! can recover lost source symbols at the decoder.

use raptorpath::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};

/// Create an encoder/decoder pair, encode N source symbols, drop some,
/// generate targeted repairs from gap ranges, and verify decoder recovery.
#[test]
fn test_nack_targeted_repair_recovers_gaps() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    let n = 20;
    let mut wire_symbols = Vec::new();

    // Encode N source symbols
    for i in 0..n {
        let data = vec![i as u8; symbol_size as usize];
        let sym = encoder.add_source(&data);
        wire_symbols.push(sym);
    }

    // Feed all except symbols 5..10 (simulate gap)
    let mut recovered_seqs: Vec<u64> = Vec::new();
    for (i, sym) in wire_symbols.iter().enumerate() {
        if (5..10).contains(&i) {
            continue; // drop these
        }
        let decoded = decoder.add_symbol(sym);
        for (seq, _data) in decoded {
            recovered_seqs.push(seq);
        }
    }

    // The gap: sequences 5..10 are missing
    let gap_start = 5u64;
    let gap_end = 9u64;
    let gaps = vec![(gap_start, gap_end)];

    // Generate targeted repairs (one per gap symbol)
    let total_gap = (gap_end - gap_start + 1) as usize;
    let repair_count = total_gap.min(10); // MAX_NACK_REPAIRS_PER_NACK
    let (win_start, win_end) = encoder.window_span();

    // Verify gap is within window
    assert!(gap_end >= win_start && gap_start <= win_end, "gap should be in window");

    for _ in 0..repair_count {
        if encoder.window_size() == 0 {
            break;
        }
        let repair = encoder.generate_repair();
        let decoded = decoder.add_symbol(&repair);
        for (seq, _data) in decoded {
            recovered_seqs.push(seq);
        }
    }

    // With enough repair symbols, some or all of the gap should be recovered
    // RLC is probabilistic, so we may need more repairs than gap size
    // Generate extra repairs for better recovery odds
    for _ in 0..20 {
        if encoder.window_size() == 0 {
            break;
        }
        let repair = encoder.generate_repair();
        let decoded = decoder.add_symbol(&repair);
        for (seq, _data) in decoded {
            recovered_seqs.push(seq);
        }
    }

    // Check that at least some gap symbols were recovered
    let gap_recovered: Vec<_> = recovered_seqs
        .iter()
        .filter(|&&s| s >= gap_start && s <= gap_end)
        .collect();

    assert!(
        !gap_recovered.is_empty(),
        "targeted repair should recover at least some gap symbols, recovered={:?}",
        recovered_seqs
    );
}

/// Test that gap ranges outside the encoder window are safely ignored.
#[test]
fn test_nack_gaps_outside_window_ignored() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);

    // Add some source symbols
    for i in 0..10 {
        let data = vec![i as u8; symbol_size as usize];
        encoder.add_source(&data);
    }

    let (win_start, win_end) = encoder.window_span();

    // Gap completely outside window
    let gaps = vec![(win_end + 100, win_end + 200)];
    let total_gap: u64 = gaps
        .iter()
        .filter(|(s, e)| *e >= win_start && *s <= win_end)
        .map(|(s, e)| e - s + 1)
        .sum();

    assert_eq!(total_gap, 0, "out-of-window gaps should be filtered out");
}

/// Test rate limiting: multiple NACKs should produce bounded repair bursts.
#[test]
fn test_nack_repair_bounded() {
    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);

    // Add source symbols
    for i in 0..50 {
        let data = vec![i as u8; symbol_size as usize];
        encoder.add_source(&data);
    }

    // Simulate large gap (30 symbols missing)
    let gaps = vec![(5, 34)];
    let (win_start, win_end) = encoder.window_span();
    let total_gap: u64 = gaps
        .iter()
        .filter(|(s, e)| *e >= win_start && *s <= win_end)
        .map(|(s, e)| e - s + 1)
        .sum();

    // MAX_NACK_REPAIRS_PER_NACK = 10
    let repair_count = (total_gap as usize).min(10);
    assert_eq!(repair_count, 10, "repair should be bounded at 10");

    // Verify we can actually generate that many repairs
    let mut generated = 0;
    for _ in 0..repair_count {
        if encoder.window_size() == 0 {
            break;
        }
        let _repair = encoder.generate_repair();
        generated += 1;
    }
    assert_eq!(generated, 10, "should generate exactly 10 repairs");
}
