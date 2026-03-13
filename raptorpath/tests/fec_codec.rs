//! FEC codec tests: encode/decode with various loss patterns.
//! Tests both RaptorQ and METTLE backends via the FecBackend trait.

use raptorpath::fec::{EncodingParams, FecBackend, WireSymbol};

fn make_params(data_len: usize, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / 1200.0).ceil() as u32,
        symbol_size: 1200,
        repair_count,
        block_id: 0,
    }
}

fn encode_decode(backend: FecBackend, data: &[u8], drop_indices: &[usize]) -> Option<Vec<u8>> {
    let params = make_params(data.len(), 20);
    let encoder = backend.create_encoder(data, params);

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

    let mut decoder = backend.create_decoder(params, data.len() as u64);
    for sym in &transmitted {
        if let Some(result) = decoder.add_symbol(sym) {
            return Some(result.to_vec());
        }
    }
    None
}

// ===== RaptorQ Backend Tests =====

#[test]
fn raptorq_decode_no_loss() {
    let data = vec![42u8; 5000];
    let result = encode_decode(FecBackend::RaptorQ, &data, &[]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn raptorq_decode_with_source_loss() {
    let data = vec![0xAB; 5000];
    let result = encode_decode(FecBackend::RaptorQ, &data, &[0, 1, 2]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn raptorq_decode_source_only_no_loss() {
    let data = vec![0xCD; 3600];
    let params = make_params(data.len(), 10);
    let encoder = FecBackend::RaptorQ.create_encoder(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = FecBackend::RaptorQ.create_decoder(params, data.len() as u64);
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
fn raptorq_decode_only_repair_symbols() {
    let data = vec![0xEF; 5000];
    let params = make_params(data.len(), 20);
    let encoder = FecBackend::RaptorQ.create_encoder(&data, params);

    let _source = encoder.source_symbols();
    let repair = encoder.repair_symbols(20);

    let mut decoder = FecBackend::RaptorQ.create_decoder(params, data.len() as u64);
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
fn raptorq_duplicate_symbols_ignored() {
    let data = vec![0x11; 3600];
    let params = make_params(data.len(), 5);
    let encoder = FecBackend::RaptorQ.create_encoder(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = FecBackend::RaptorQ.create_decoder(params, data.len() as u64);

    let first = &source[0];
    decoder.add_symbol(first);
    decoder.add_symbol(first);

    assert_eq!(decoder.total_fed(), 1, "Duplicate should not be counted");
}

#[test]
fn raptorq_large_block() {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    let result = encode_decode(FecBackend::RaptorQ, &data, &[0, 5, 10, 15, 20]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn raptorq_small_block() {
    let data = vec![1, 2, 3, 4, 5];
    let result = encode_decode(FecBackend::RaptorQ, &data, &[]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn decoder_created_at_is_recent() {
    let params = EncodingParams {
        source_symbols: 10,
        symbol_size: 1200,
        repair_count: 5,
        block_id: 0,
    };
    let decoder = FecBackend::RaptorQ.create_decoder(params, 12000);
    assert!(decoder.created_at().elapsed().as_secs() < 1);
}

// ===== METTLE Backend Tests =====

#[test]
fn mettle_decode_no_loss() {
    let data = vec![42u8; 5000];
    let result = encode_decode(FecBackend::Mettle, &data, &[]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn mettle_decode_with_source_loss() {
    let data = vec![0xAB; 5000];
    let result = encode_decode(FecBackend::Mettle, &data, &[0, 1, 2]).unwrap();
    assert_eq!(result, data);
}

#[test]
fn mettle_source_only_no_loss() {
    let data = vec![0xCD; 3600];
    let params = make_params(data.len(), 10);
    let encoder = FecBackend::Mettle.create_encoder(&data, params);
    let source = encoder.source_symbols();

    let mut decoder = FecBackend::Mettle.create_decoder(params, data.len() as u64);
    let mut result = None;
    for sym in &source {
        if let Some(r) = decoder.add_symbol(sym) {
            result = Some(r);
            break;
        }
    }

    assert!(result.is_some());
}

#[test]
fn mettle_large_block() {
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    // METTLE with small window needs more repair symbols for large blocks.
    // Use a custom encode/decode with higher repair_count.
    let repair_count = 40;
    let params = make_params(data.len(), repair_count);
    let encoder = FecBackend::Mettle.create_encoder(&data, params);

    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(repair_count);

    let drop_indices: &[usize] = &[0, 5, 10, 15, 20];
    let mut all: Vec<WireSymbol> = source;
    all.extend(repair);

    let transmitted: Vec<_> = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_indices.contains(i))
        .map(|(_, s)| s)
        .collect();

    let mut decoder = FecBackend::Mettle.create_decoder(params, data.len() as u64);
    let mut result = None;
    for sym in &transmitted {
        if let Some(r) = decoder.add_symbol(sym) {
            result = Some(r.to_vec());
            break;
        }
    }

    assert!(result.is_some(), "METTLE should decode large block with 40 repair symbols");
    assert_eq!(result.unwrap(), data);
}
