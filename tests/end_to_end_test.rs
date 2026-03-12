//! End-to-end tests: frame packets -> encode FEC block -> simulate loss -> decode -> extract.
//! Proves the full codec pipeline without any TUN or network dependency.

use raptorpath::fec::{Decoder, EncodingParams, FecStream, WireSymbol};
use raptorpath::net::framing::{extract_packets, frame_end, frame_packet};

/// Run the full pipeline: frame packets, encode, simulate loss, decode, extract.
fn e2e_pipeline(
    packets: &[Vec<u8>],
    symbol_size: u16,
    repair_fraction: f64,
    drop_indices: &[usize],
) -> Vec<Vec<u8>> {
    // 1. Frame packets into block
    let mut block = Vec::new();
    for pkt in packets {
        frame_packet(&mut block, pkt);
    }
    frame_end(&mut block);

    // 2. Encode
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    let repair_count = (source_symbols as f64 * repair_fraction).ceil() as u32;
    let params = EncodingParams {
        source_symbols,
        symbol_size,
        repair_count,
        block_id: 0,
    };
    let mut fec = FecStream::new(&block, params);
    let source = fec.take_source_symbols();
    let repair = fec.generate_repair(repair_count);

    // 3. Simulate loss
    let mut all: Vec<WireSymbol> = source;
    all.extend(repair);
    let transmitted: Vec<_> = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_indices.contains(i))
        .map(|(_, s)| s)
        .collect();

    // 4. Decode
    let mut decoder = Decoder::new(params, block.len() as u64);
    let mut decoded_data = None;
    for sym in &transmitted {
        if let Some(data) = decoder.add_symbol(sym) {
            decoded_data = Some(data);
            break;
        }
    }

    // 5. Extract
    match decoded_data {
        Some(data) => extract_packets(&data),
        None => vec![],
    }
}

#[test]
fn test_e2e_single_packet_no_loss() {
    let packet = vec![0xABu8; 1400];
    let result = e2e_pipeline(&[packet.clone()], 256, 0.0, &[]);
    assert_eq!(result.len(), 1, "expected exactly 1 packet");
    assert_eq!(result[0], packet, "recovered packet must match original");
}

#[test]
fn test_e2e_multiple_packets_no_loss() {
    let packets: Vec<Vec<u8>> = [100, 500, 1000, 1400, 50]
        .iter()
        .enumerate()
        .map(|(i, &size)| vec![(i as u8).wrapping_add(0x10); size])
        .collect();

    let result = e2e_pipeline(&packets, 256, 0.0, &[]);
    assert_eq!(result.len(), 5, "expected 5 packets");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(&result[i], pkt, "packet {i} mismatch");
    }
}

#[test]
fn test_e2e_with_10_percent_loss() {
    let packets: Vec<Vec<u8>> = vec![
        vec![0x01; 500],
        vec![0x02; 800],
        vec![0x03; 300],
    ];

    // Build block to compute how many symbols we'll have
    let mut block = Vec::new();
    for pkt in &packets {
        frame_packet(&mut block, pkt);
    }
    frame_end(&mut block);

    let symbol_size: u16 = 256;
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    let repair_count = (source_symbols as f64 * 0.3).ceil() as u32;
    let total = source_symbols + repair_count;

    // Drop every 10th symbol
    let drop_indices: Vec<usize> = (0..total as usize).filter(|i| i % 10 == 9).collect();

    let result = e2e_pipeline(&packets, symbol_size, 0.3, &drop_indices);
    assert_eq!(result.len(), 3, "expected 3 packets after 10% loss recovery");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(&result[i], pkt, "packet {i} mismatch after loss recovery");
    }
}

#[test]
fn test_e2e_with_30_percent_loss() {
    let packets: Vec<Vec<u8>> = vec![
        vec![0xAA; 600],
        vec![0xBB; 400],
        vec![0xCC; 700],
    ];

    let mut block = Vec::new();
    for pkt in &packets {
        frame_packet(&mut block, pkt);
    }
    frame_end(&mut block);

    let symbol_size: u16 = 256;
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    let repair_count = (source_symbols as f64 * 0.5).ceil() as u32;
    let total = source_symbols + repair_count;

    // Drop every 3rd symbol
    let drop_indices: Vec<usize> = (0..total as usize).filter(|i| i % 3 == 2).collect();

    let result = e2e_pipeline(&packets, symbol_size, 0.5, &drop_indices);
    assert_eq!(result.len(), 3, "expected 3 packets after 30% loss recovery");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(&result[i], pkt, "packet {i} mismatch after 30% loss");
    }
}

#[test]
fn test_e2e_with_burst_loss() {
    let packets: Vec<Vec<u8>> = vec![
        vec![0x11; 400],
        vec![0x22; 500],
        vec![0x33; 600],
    ];

    let mut block = Vec::new();
    for pkt in &packets {
        frame_packet(&mut block, pkt);
    }
    frame_end(&mut block);

    let symbol_size: u16 = 256;
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    // Need enough repair to cover a burst of 5 consecutive drops
    let repair_fraction = 1.0; // 100% repair for safety
    let repair_count = (source_symbols as f64 * repair_fraction).ceil() as u32;
    let total = source_symbols + repair_count;

    // Drop 5 consecutive symbols from the middle
    let mid = total as usize / 2;
    let drop_indices: Vec<usize> = (mid..mid + 5).collect();

    let result = e2e_pipeline(&packets, symbol_size, repair_fraction, &drop_indices);
    assert_eq!(result.len(), 3, "expected 3 packets after burst loss recovery");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(&result[i], pkt, "packet {i} mismatch after burst loss");
    }
}

#[test]
fn test_e2e_data_integrity_random_patterns() {
    // Generate 10 packets with pseudo-random data using block_id-seeded pattern
    let packets: Vec<Vec<u8>> = (0u8..10)
        .map(|seed| {
            let size = 100 + (seed as usize) * 37; // varying sizes
            (0..size)
                .map(|j| seed.wrapping_mul(7).wrapping_add(j as u8).wrapping_mul(13))
                .collect()
        })
        .collect();

    // Drop first 2 symbols
    let drop_indices: Vec<usize> = vec![0, 1];

    let result = e2e_pipeline(&packets, 256, 0.5, &drop_indices);
    assert_eq!(result.len(), 10, "expected all 10 packets recovered");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(
            &result[i], pkt,
            "packet {i} byte-for-byte mismatch with pseudo-random data"
        );
    }
}

#[test]
fn test_e2e_large_block_many_packets() {
    // 50 small packets, each 100 bytes
    let packets: Vec<Vec<u8>> = (0u8..50)
        .map(|i| vec![i; 100])
        .collect();

    let result = e2e_pipeline(&packets, 256, 0.0, &[]);
    assert_eq!(result.len(), 50, "expected all 50 packets recovered");
    for (i, pkt) in packets.iter().enumerate() {
        assert_eq!(&result[i], pkt, "packet {i} mismatch in large block");
    }
}

#[test]
fn test_e2e_fec_padding_does_not_corrupt() {
    // A tiny 10-byte packet. FEC will pad the block to fill symbols.
    // After decode, extract must return only the original 10-byte packet.
    let packet = vec![0xDE; 10];
    let result = e2e_pipeline(&[packet.clone()], 256, 0.0, &[]);
    assert_eq!(result.len(), 1, "expected exactly 1 packet (no garbage from padding)");
    assert_eq!(result[0].len(), 10, "recovered packet must be exactly 10 bytes");
    assert_eq!(result[0], packet, "recovered packet content must match");
}

#[test]
fn test_e2e_empty_block_sentinel_only() {
    // Frame zero packets — just the sentinel
    let result = e2e_pipeline(&[], 256, 0.0, &[]);
    assert!(result.is_empty(), "empty block must produce no packets");
}

#[test]
fn test_e2e_maximum_loss_recovery() {
    // Frame 1 packet, encode with K source + K repair.
    // Drop ALL source symbols. Feed only repair. Must recover.
    let packet = vec![0xFE; 800];

    let mut block = Vec::new();
    frame_packet(&mut block, &packet);
    frame_end(&mut block);

    let symbol_size: u16 = 256;
    let source_symbols = (block.len() as f64 / symbol_size as f64).ceil() as u32;
    // Use repair_fraction = 1.0 to get K repair symbols
    let repair_count = source_symbols;
    let params = EncodingParams {
        source_symbols,
        symbol_size,
        repair_count,
        block_id: 0,
    };

    let mut fec = FecStream::new(&block, params);
    let _source = fec.take_source_symbols();
    let repair = fec.generate_repair(repair_count);

    // Feed only repair symbols (drop all source)
    let mut decoder = Decoder::new(params, block.len() as u64);
    let mut decoded_data = None;
    for sym in &repair {
        if let Some(data) = decoder.add_symbol(sym) {
            decoded_data = Some(data);
            break;
        }
    }

    let recovered = decoded_data.expect("must decode from repair symbols alone");
    let extracted = extract_packets(&recovered);
    assert_eq!(extracted.len(), 1, "expected 1 packet from repair-only decode");
    assert_eq!(extracted[0], packet, "repair-only recovered packet must match original");
}
