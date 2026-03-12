//! Integration tests for packet framing + FEC encode/decode pipeline.
//!
//! Tests the full path: packets → frame → encode → (simulate loss) → decode → extract.

use raptorpath::fec::{Decoder, Encoder, EncodingParams, WireSymbol};
use raptorpath::net::framing;

/// Helper: frame packets into a block, encode, decode, extract.
fn roundtrip_with_loss(
    packets: &[Vec<u8>],
    loss_indices: &[usize],
) -> Vec<Vec<u8>> {
    // Frame packets into a block
    let mut block = Vec::new();
    for pkt in packets {
        framing::frame_packet(&mut block, pkt);
    }
    framing::frame_end(&mut block);

    let transfer_length = block.len() as u64;
    let source_symbols = (block.len() as f64 / 1200.0).ceil() as u32;
    let repair_count = (source_symbols as f64 * 0.5).ceil() as u32; // 50% overhead

    let params = EncodingParams {
        source_symbols,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    };

    // Encode
    let encoder = Encoder::new(&block, params);
    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(repair_count);

    // Combine all symbols
    let mut all_symbols: Vec<WireSymbol> = source;
    all_symbols.extend(repair);

    // Drop some symbols (simulate loss)
    let transmitted: Vec<WireSymbol> = all_symbols
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !loss_indices.contains(i))
        .map(|(_, s)| s)
        .collect();

    // Decode
    let mut decoder = Decoder::new(params, transfer_length);
    let mut decoded_data = None;

    for symbol in &transmitted {
        if let Some(data) = decoder.add_symbol(symbol) {
            decoded_data = Some(data);
            break;
        }
    }

    let data = decoded_data.expect("should decode successfully");

    // Extract packets
    framing::extract_packets(&data)
}

#[test]
fn test_roundtrip_no_loss() {
    let packets = vec![
        vec![1, 2, 3, 4, 5],
        vec![10, 20, 30],
        vec![100; 1000],
    ];

    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted[0], packets[0]);
    assert_eq!(extracted[1], packets[1]);
    assert_eq!(extracted[2], packets[2]);
}

#[test]
fn test_roundtrip_with_source_loss() {
    // Use enough data to produce many source symbols so dropping a few is recoverable
    let packets: Vec<Vec<u8>> = (0..20)
        .map(|i| vec![i as u8; 500])
        .collect();

    // Drop a few source symbols — should recover via repair
    let extracted = roundtrip_with_loss(&packets, &[0, 2, 4]);
    assert_eq!(extracted.len(), 20);
    for (i, pkt) in extracted.iter().enumerate() {
        assert_eq!(pkt.len(), 500);
        assert_eq!(pkt[0], i as u8);
    }
}

#[test]
fn test_roundtrip_single_small_packet() {
    // Test minimum viable block (one tiny packet)
    let packets = vec![vec![42]];
    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], vec![42]);
}

#[test]
fn test_roundtrip_many_small_packets() {
    // Simulate gaming traffic: many small packets
    let packets: Vec<Vec<u8>> = (0..50)
        .map(|i| vec![(i as u8); 64])
        .collect();

    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 50);
    for (i, pkt) in extracted.iter().enumerate() {
        assert_eq!(pkt.len(), 64);
        assert_eq!(pkt[0], i as u8);
    }
}

#[test]
fn test_roundtrip_mtu_packets() {
    // Full MTU-sized packets
    let packets: Vec<Vec<u8>> = (0..10)
        .map(|i| vec![(i as u8); 1500])
        .collect();

    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 10);
}

#[test]
fn test_roundtrip_mixed_sizes() {
    let packets = vec![
        vec![1; 10],     // tiny
        vec![2; 100],    // small
        vec![3; 500],    // medium
        vec![4; 1500],   // MTU
        vec![5; 50],     // small again
    ];

    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 5);
    for (i, pkt) in extracted.iter().enumerate() {
        assert!(pkt.iter().all(|&b| b == (i + 1) as u8));
    }
}

#[test]
fn test_roundtrip_heavy_loss() {
    let packets = vec![vec![0xDE; 200]; 5];

    // Drop many symbols but stay within repair capacity
    // With 50% overhead we can tolerate significant loss
    let extracted = roundtrip_with_loss(&packets, &[0, 2, 4]);
    assert_eq!(extracted.len(), 5);
}

#[test]
fn test_framing_preserves_empty_looking_data() {
    // Packets containing bytes that look like framing (0x00)
    let packets = vec![
        vec![0, 0, 0, 0],           // all zeros
        vec![0, 5, 0, 0],           // looks like length prefix
        vec![0xFF, 0xFF],           // max bytes
    ];

    let extracted = roundtrip_with_loss(&packets, &[]);
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted[0], packets[0]);
    assert_eq!(extracted[1], packets[1]);
    assert_eq!(extracted[2], packets[2]);
}
