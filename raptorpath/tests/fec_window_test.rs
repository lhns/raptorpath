//! End-to-end tests for the sliding-window FEC pipeline.
//!
//! Exercises the full chain: frame packets → window encode → simulate loss → decode → extract.
//! No TUN or network dependency — pure in-process simulation.

use raptorpath::fec::{
    RlcWindowDecoder, RlcWindowEncoder,
    WindowDecoder, WindowEncoder, WireSymbol,
};
use raptorpath::net::framing::{extract_window_packet, frame_window_packet};

const SYMBOL_SIZE: u16 = 128;

/// Helper: generate test packets of varying sizes.
fn make_packets(count: usize, base_size: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|i| {
            let size = base_size.min(SYMBOL_SIZE as usize - 2); // fit within symbol
            vec![(i as u8).wrapping_add(1); size]
        })
        .collect()
}

/// Full pipeline: frame → encode → simulate loss → decode → extract.
/// Returns recovered packets in sequence order.
fn window_pipeline(
    packets: &[Vec<u8>],
    loss_indices: &[usize],
    repair_count: usize,
) -> Vec<Vec<u8>> {
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);

    // Encode all packets as source symbols
    let mut source_symbols: Vec<WireSymbol> = Vec::new();
    for pkt in packets {
        let framed = frame_window_packet(pkt, SYMBOL_SIZE);
        let sym = encoder.add_source(&framed);
        source_symbols.push(sym);
    }

    // Generate repair symbols
    let mut repair_symbols: Vec<WireSymbol> = Vec::new();
    for _ in 0..repair_count {
        repair_symbols.push(encoder.generate_repair());
    }

    // Simulate loss: drop source symbols at specified indices
    let mut recovered_packets: Vec<(u64, Vec<u8>)> = Vec::new();

    for (i, sym) in source_symbols.iter().enumerate() {
        if loss_indices.contains(&i) {
            continue; // dropped
        }
        let results = decoder.add_symbol(sym);
        for (seq, data) in results {
            if let Some(pkt) = extract_window_packet(&data) {
                recovered_packets.push((seq, pkt));
            }
        }
    }

    // Feed repair symbols
    for sym in &repair_symbols {
        let results = decoder.add_symbol(sym);
        for (seq, data) in results {
            if let Some(pkt) = extract_window_packet(&data) {
                recovered_packets.push((seq, pkt));
            }
        }
    }

    // Sort by sequence and deduplicate
    recovered_packets.sort_by_key(|(seq, _)| *seq);
    recovered_packets.dedup_by_key(|(seq, _)| *seq);
    recovered_packets.into_iter().map(|(_, pkt)| pkt).collect()
}

// ---------------------------------------------------------------------------
// No-loss tests
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_no_loss_single_packet() {
    let packets = make_packets(1, 32);
    let recovered = window_pipeline(&packets, &[], 0);
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_no_loss_many_packets() {
    let packets = make_packets(50, 64);
    let recovered = window_pipeline(&packets, &[], 0);
    assert_eq!(recovered.len(), 50);
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_no_loss_max_payload() {
    // Packets that fill the entire symbol payload (symbol_size - 2 bytes for length prefix)
    let packets = make_packets(10, SYMBOL_SIZE as usize - 2);
    let recovered = window_pipeline(&packets, &[], 0);
    assert_eq!(recovered, packets);
}

// ---------------------------------------------------------------------------
// Single-loss recovery
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_single_loss_first() {
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[0], 1);
    assert_eq!(recovered.len(), 10, "Should recover the dropped packet");
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_single_loss_last() {
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[9], 1);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_single_loss_middle() {
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[5], 1);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

// ---------------------------------------------------------------------------
// Multi-loss recovery
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_two_losses() {
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[2, 7], 2);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_three_losses() {
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[1, 4, 8], 3);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

#[test]
fn test_window_e2e_burst_loss() {
    // Consecutive losses (burst)
    let packets = make_packets(10, 32);
    let recovered = window_pipeline(&packets, &[3, 4, 5], 3);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

// ---------------------------------------------------------------------------
// Repair-only recovery (all sources lost)
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_all_sources_lost() {
    let packets = make_packets(5, 32);
    let loss_indices: Vec<usize> = (0..5).collect();
    let recovered = window_pipeline(&packets, &loss_indices, 5);
    assert_eq!(recovered.len(), 5, "Should recover all from repair only");
    assert_eq!(recovered, packets);
}

// ---------------------------------------------------------------------------
// Data integrity
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_data_integrity_varied_sizes() {
    // Packets of different sizes to verify length-prefix framing works
    let packets: Vec<Vec<u8>> = (1..=20)
        .map(|i| {
            let size = (i * 5).min(SYMBOL_SIZE as usize - 2);
            (0..size).map(|j| (j as u8).wrapping_add(i as u8)).collect()
        })
        .collect();

    // Drop every 4th packet
    let loss_indices: Vec<usize> = (0..20).filter(|i| i % 4 == 0).collect();
    let repair_count = loss_indices.len();

    let recovered = window_pipeline(&packets, &loss_indices, repair_count);
    assert_eq!(recovered.len(), packets.len());
    for (i, (got, expected)) in recovered.iter().zip(packets.iter()).enumerate() {
        assert_eq!(got, expected, "Data mismatch at packet {i}");
    }
}

// ---------------------------------------------------------------------------
// Excess repair (more repair than needed)
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_excess_repair() {
    let packets = make_packets(10, 32);
    // 2 losses but 5 repair symbols — should still work fine
    let recovered = window_pipeline(&packets, &[3, 6], 5);
    assert_eq!(recovered.len(), 10);
    assert_eq!(recovered, packets);
}

// ---------------------------------------------------------------------------
// Insufficient repair
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_insufficient_repair() {
    let packets = make_packets(10, 32);
    // 3 losses but only 1 repair — can't recover all
    let recovered = window_pipeline(&packets, &[2, 5, 8], 1);
    assert!(
        recovered.len() < 10,
        "Should not recover all with insufficient repair, got {}",
        recovered.len()
    );
    // The 7 non-dropped sources should still be delivered
    assert!(recovered.len() >= 7);
}

// ---------------------------------------------------------------------------
// Window advance / large streams
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_large_stream_with_periodic_loss() {
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);

    let total_packets = 200;
    let packets = make_packets(total_packets, 48);
    let mut recovered_count = 0;
    let mut source_count = 0;

    for (i, pkt) in packets.iter().enumerate() {
        let framed = frame_window_packet(pkt, SYMBOL_SIZE);
        let sym = encoder.add_source(&framed);
        source_count += 1;

        // Drop every 10th packet
        if i % 10 != 0 {
            let results = decoder.add_symbol(&sym);
            for (_, data) in results {
                if extract_window_packet(&data).is_some() {
                    recovered_count += 1;
                }
            }
        }

        // Generate repair every 5 source symbols
        if source_count % 5 == 0 {
            let repair = encoder.generate_repair();
            let results = decoder.add_symbol(&repair);
            for (_, data) in results {
                if extract_window_packet(&data).is_some() {
                    recovered_count += 1;
                }
            }
        }

        // Advance window periodically to keep memory bounded
        if i > 0 && i % 50 == 0 {
            let advance_to = (i as u64).saturating_sub(30);
            encoder.advance(advance_to);
            decoder.advance(advance_to);
        }
    }

    // With 10% loss and 20% repair rate, we should recover most packets
    let recovery_rate = recovered_count as f64 / total_packets as f64;
    assert!(
        recovery_rate > 0.90,
        "Expected >90% recovery with 10% loss and 20% repair, got {:.1}% ({}/{})",
        recovery_rate * 100.0,
        recovered_count,
        total_packets
    );
}

// ---------------------------------------------------------------------------
// Framing roundtrip edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_window_framing_roundtrip_through_codec() {
    // Verify that frame → pad → encode → decode → extract preserves data exactly
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);

    let test_data = b"Hello, raptorpath window mode!";
    let framed = frame_window_packet(test_data, SYMBOL_SIZE);
    assert_eq!(framed.len(), SYMBOL_SIZE as usize);

    let sym = encoder.add_source(&framed);
    let results = decoder.add_symbol(&sym);

    assert_eq!(results.len(), 1);
    let extracted = extract_window_packet(&results[0].1).unwrap();
    assert_eq!(extracted, test_data);
}

// ---------------------------------------------------------------------------
// Reorder tolerance
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_reordered_symbols() {
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);

    let packets = make_packets(10, 32);
    let mut symbols: Vec<WireSymbol> = Vec::new();

    for pkt in &packets {
        let framed = frame_window_packet(pkt, SYMBOL_SIZE);
        symbols.push(encoder.add_source(&framed));
    }

    // Feed in reverse order — decoder should still recover all
    let mut recovered: Vec<(u64, Vec<u8>)> = Vec::new();
    for sym in symbols.iter().rev() {
        let results = decoder.add_symbol(sym);
        for (seq, data) in results {
            if let Some(pkt) = extract_window_packet(&data) {
                recovered.push((seq, pkt));
            }
        }
    }

    recovered.sort_by_key(|(seq, _)| *seq);
    let recovered_pkts: Vec<Vec<u8>> = recovered.into_iter().map(|(_, p)| p).collect();
    assert_eq!(recovered_pkts, packets);
}

// ---------------------------------------------------------------------------
// Mixed source and repair interleaving
// ---------------------------------------------------------------------------

#[test]
fn test_window_e2e_interleaved_source_and_repair() {
    // Simulate the real sender pattern: send source, periodically send repair
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);

    let packets = make_packets(20, 32);
    let mut recovered: Vec<(u64, Vec<u8>)> = Vec::new();
    let loss_indices = vec![3, 7, 12, 16];

    for (i, pkt) in packets.iter().enumerate() {
        let framed = frame_window_packet(pkt, SYMBOL_SIZE);
        let sym = encoder.add_source(&framed);

        if !loss_indices.contains(&i) {
            let results = decoder.add_symbol(&sym);
            for (seq, data) in results {
                if let Some(p) = extract_window_packet(&data) {
                    recovered.push((seq, p));
                }
            }
        }

        // Send repair every 3 source symbols
        if (i + 1) % 3 == 0 {
            let repair = encoder.generate_repair();
            let results = decoder.add_symbol(&repair);
            for (seq, data) in results {
                if let Some(p) = extract_window_packet(&data) {
                    recovered.push((seq, p));
                }
            }
        }
    }

    // Send a few more repairs at the end to recover any remaining losses
    for _ in 0..4 {
        let repair = encoder.generate_repair();
        let results = decoder.add_symbol(&repair);
        for (seq, data) in results {
            if let Some(p) = extract_window_packet(&data) {
                recovered.push((seq, p));
            }
        }
    }

    recovered.sort_by_key(|(seq, _)| *seq);
    recovered.dedup_by_key(|(seq, _)| *seq);
    let recovered_pkts: Vec<Vec<u8>> = recovered.into_iter().map(|(_, p)| p).collect();
    assert_eq!(
        recovered_pkts.len(),
        packets.len(),
        "Should recover all 20 packets"
    );
    assert_eq!(recovered_pkts, packets);
}
