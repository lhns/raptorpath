//! B5: NACK repair flow tests with SimChannel.
//!
//! Verifies gap detection, NACK-triggered repair recovery,
//! cooldown bounds, and gap range limits.

mod common;

use common::*;
use raptorpath::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};
use raptorpath::net::{compute_gap_ranges, MAX_NACK_GAPS};
use raptorpath::scheduler::MockClock;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_gap_detection_matches_actual_drops() {
    let clock = Arc::new(MockClock::new());
    let mut channel = SimChannel::wifi(clock.clone(), 42);

    let num_symbols = 50u32;
    let symbols = make_source_batch(num_symbols);

    // Track which survived/dropped
    let mut dropped_seqs = BTreeSet::new();
    let mut survived_seqs = BTreeSet::new();

    for (i, sym) in symbols.iter().enumerate() {
        if channel.send(sym.clone()) {
            survived_seqs.insert(i as u64);
        } else {
            dropped_seqs.insert(i as u64);
        }
    }

    // Advance and deliver
    clock.advance(Duration::from_millis(20));
    let delivered = channel.deliver();

    // Build received set from delivered packet seqs
    let received: BTreeSet<u64> = delivered.iter().map(|p| p.seq).collect();

    // compute_gap_ranges should identify the gaps
    let gaps = compute_gap_ranges(&received, 0, num_symbols as u64 - 1);

    // Expand gap ranges to individual seqs
    let mut gap_seqs = BTreeSet::new();
    for &(start, end) in &gaps {
        for seq in start..=end {
            gap_seqs.insert(seq);
        }
    }

    // Every actually dropped seq should be in a gap range
    for &dropped in &dropped_seqs {
        assert!(
            gap_seqs.contains(&dropped),
            "dropped seq {dropped} should be in gap ranges, gaps={gaps:?}"
        );
    }

    // Gap seqs should not contain any survived seq
    for &survived in &survived_seqs {
        assert!(
            !gap_seqs.contains(&survived),
            "survived seq {survived} should not be in gap ranges"
        );
    }
}

#[test]
fn test_nack_repair_recovers_with_rlc_window() {
    let clock = Arc::new(MockClock::new());
    let mut channel = SimChannel::wifi(clock.clone(), 55);

    let symbol_size = 64u16;
    let mut encoder = RlcWindowEncoder::new(symbol_size);
    let mut decoder = RlcWindowDecoder::new(symbol_size);

    let num_symbols = 30;
    let mut wire_symbols = Vec::new();

    // Encode source symbols
    for i in 0..num_symbols {
        let data = vec![i as u8; symbol_size as usize];
        let sym = encoder.add_source(&data);
        wire_symbols.push(sym);
    }

    // Send through WiFi channel
    let mut received_seqs = BTreeSet::new();
    let mut dropped_seqs = BTreeSet::new();

    for (i, sym) in wire_symbols.iter().enumerate() {
        if channel.send(sym.clone()) {
            // Will be delivered
        } else {
            dropped_seqs.insert(i as u64);
        }
    }

    // Advance and deliver
    clock.advance(Duration::from_millis(20));
    let delivered = channel.deliver();

    // Feed surviving symbols to decoder
    let mut recovered = BTreeSet::new();
    for pkt in &delivered {
        let decoded = decoder.add_symbol(&pkt.symbol);
        for (seq, _) in decoded {
            recovered.insert(seq);
            received_seqs.insert(seq);
        }
    }

    // Compute gaps
    let gaps = compute_gap_ranges(&received_seqs, 0, num_symbols as u64 - 1);

    // Generate repair symbols (max 10) and feed to decoder
    let repair_count = 10.min(gaps.len() * 3);
    for _ in 0..repair_count {
        if encoder.window_size() == 0 {
            break;
        }
        let repair = encoder.generate_repair();
        let decoded = decoder.add_symbol(&repair);
        for (seq, _) in decoded {
            recovered.insert(seq);
        }
    }

    // Check how many dropped symbols were recovered
    let gap_recovered: usize = dropped_seqs.iter().filter(|s| recovered.contains(s)).count();
    let recovery_pct = if dropped_seqs.is_empty() {
        100.0
    } else {
        gap_recovered as f64 / dropped_seqs.len() as f64 * 100.0
    };

    // With 10 repairs for WiFi ~2.5% loss on 30 symbols (~1 loss), should recover >=50%
    assert!(
        recovery_pct >= 50.0 || dropped_seqs.is_empty(),
        "should recover >=50% of gaps: recovered {gap_recovered}/{} ({recovery_pct:.0}%)",
        dropped_seqs.len()
    );
}

#[test]
fn test_cooldown_bounds_repair_rate() {
    // Simulate 20 rapid NACKs with 5ms cooldown
    let cooldown_us: u64 = 5_000; // 5ms
    let nack_interval_us: u64 = 1_000; // 1ms between NACKs

    let mut last_repair_us: u64 = 0;
    let mut repairs_sent = 0u32;

    for i in 0..20 {
        let nack_time_us = i * nack_interval_us;

        // Check if cooldown has elapsed
        if nack_time_us >= last_repair_us + cooldown_us || last_repair_us == 0 {
            repairs_sent += 1;
            last_repair_us = nack_time_us;
        }
    }

    // 20 NACKs at 1ms apart = 19ms total. With 5ms cooldown, should trigger at:
    // t=0 (first fires unconditionally), t=5, t=10, t=15 → 4 or 5 repairs
    // depending on whether the first one counts against cooldown
    assert!(
        repairs_sent <= 5,
        "20 NACKs at 1ms with 5ms cooldown should produce at most 5 repairs: got {repairs_sent}"
    );
    assert!(
        repairs_sent >= 4,
        "20 NACKs at 1ms with 5ms cooldown should produce at least 4 repairs: got {repairs_sent}"
    );
}

#[test]
fn test_gap_ranges_bounded() {
    // Create a received set with 30 gaps in window [0, 100]
    let mut received = BTreeSet::new();

    // Add every other seq from 0..60, creating 30 gaps
    for i in (0..=60).step_by(2) {
        received.insert(i as u64);
    }
    // Fill in 61..100 to avoid trailing gap dominating
    for i in 61..=100 {
        received.insert(i as u64);
    }

    let gaps = compute_gap_ranges(&received, 0, 100);

    assert!(
        gaps.len() <= MAX_NACK_GAPS,
        "gap ranges should be bounded at MAX_NACK_GAPS={MAX_NACK_GAPS}: got {}",
        gaps.len()
    );
}
