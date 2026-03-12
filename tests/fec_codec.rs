//! FEC codec tests: encode/decode with various loss patterns.

use raptorpath::fec::{Decoder, Encoder, EncodingParams, WireSymbol};

fn make_params(data_len: usize, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / 1200.0).ceil() as u32,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    }
}

fn encode_decode(data: &[u8], drop_indices: &[usize]) -> Option<Vec<u8>> {
    let params = make_params(data.len(), 20);
    let encoder = Encoder::new(data, params);

    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(params.repair_count);

    let mut all: Vec<WireSymbol> = source;
    all.extend(repair);

    let transmitted: Vec<_> = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_indices.contains(i))
        .map(|(_, s)| s)
        .collect();

    let mut decoder = Decoder::new(params, data.len() as u64);
    for sym in &transmitted {
        if let Some(result) = decoder.add_symbol(sym) {
            return Some(result.to_vec());
        }
    }
    None
}

#[test]
fn test_decode_no_loss() {
    let data = vec![42u8; 5000];
    let result = encode_decode(&data, &[]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_decode_with_source_loss() {
    let data = vec![0xAB; 5000];
    let result = encode_decode(&data, &[0, 1, 2]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_decode_source_only_no_loss() {
    // When all source symbols arrive, decoder should use fast path
    let data = vec![0xCD; 3600]; // 3 source symbols
    let params = make_params(data.len(), 10);
    let encoder = Encoder::new(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = Decoder::new(params, data.len() as u64);
    let mut result = None;
    for sym in &source {
        if let Some(r) = decoder.add_symbol(sym) {
            result = Some(r);
            break;
        }
    }

    assert!(result.is_some());
    assert!(decoder.is_complete_source());
}

#[test]
fn test_decode_only_repair_symbols() {
    // Drop ALL source symbols, decode from repair only
    let data = vec![0xEF; 5000];
    let params = make_params(data.len(), 20);
    let encoder = Encoder::new(&data, params);

    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(20);

    let source_count = source.len();

    // Skip all source symbols
    let mut decoder = Decoder::new(params, data.len() as u64);
    let mut result = None;
    for sym in &repair {
        if let Some(r) = decoder.add_symbol(sym) {
            result = Some(r);
            break;
        }
    }

    assert!(result.is_some(), "Should decode from repair symbols alone");
    assert_eq!(result.unwrap().to_vec(), data);
}

#[test]
fn test_duplicate_symbols_ignored() {
    let data = vec![0x11; 3600];
    let params = make_params(data.len(), 5);
    let encoder = Encoder::new(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = Decoder::new(params, data.len() as u64);

    // Feed same symbol twice
    let first = &source[0];
    decoder.add_symbol(first);
    decoder.add_symbol(first); // duplicate

    assert_eq!(decoder.total_fed(), 1, "Duplicate should not be counted");
}

#[test]
fn test_decoder_received_ids() {
    let data = vec![0x22; 3600];
    let params = make_params(data.len(), 5);
    let encoder = Encoder::new(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = Decoder::new(params, data.len() as u64);
    for sym in &source[..2] {
        decoder.add_symbol(sym);
    }

    let ids = decoder.received_ids();
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_large_block_encode_decode() {
    // 64KB block (typical max block size)
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let result = encode_decode(&data, &[0, 5, 10, 15, 20]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_small_block_encode_decode() {
    // Very small block (flush timeout scenario)
    let data = vec![1, 2, 3, 4, 5];
    let result = encode_decode(&data, &[]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_single_symbol_block() {
    // Block that fits in a single symbol
    let data = vec![0xFF; 100];
    let params = make_params(data.len(), 3);
    let encoder = Encoder::new(&data, params);

    let source = encoder.source_symbols();
    assert!(source.len() >= 1);

    let repair = encoder.repair_symbols(3);
    assert!(repair.len() >= 1);

    // Should decode from either source or repair
    let mut decoder = Decoder::new(params, data.len() as u64);
    let mut result = None;
    for sym in source.iter().chain(repair.iter()) {
        if let Some(r) = decoder.add_symbol(sym) {
            result = Some(r);
            break;
        }
    }
    assert!(result.is_some());
}

#[test]
fn test_created_at_is_recent() {
    let params = EncodingParams {
        source_symbols: 10,
        symbol_size: 1200,
        repair_count: 5,
        block_id: 0,
    };
    let decoder = Decoder::new(params, 12000);
    assert!(decoder.created_at.elapsed().as_secs() < 1);
}
