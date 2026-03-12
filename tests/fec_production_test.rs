//! Production stability tests for the FEC codec.
//!
//! These tests verify that the RaptorQ-based FEC encoding/decoding pipeline
//! handles every combination of loss, reordering, and boundary condition
//! that a real network deployment would encounter.

use bytes::Bytes;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::fec::{Decoder, Encoder, EncodingParams, WireSymbol};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_params(data_len: usize, symbol_size: u16, repair_count: u32) -> EncodingParams {
    EncodingParams {
        source_symbols: (data_len as f64 / symbol_size as f64).ceil() as u32,
        symbol_size,
        repair_count,
        block_id: 0,
    }
}

/// Build deterministic test data of the requested length.
fn make_data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Encode data and return (source_symbols, repair_symbols, params, data).
fn encode_block(
    data: &[u8],
    symbol_size: u16,
    repair_count: u32,
    block_id: u64,
) -> (Vec<WireSymbol>, Vec<WireSymbol>, EncodingParams) {
    let params = EncodingParams {
        source_symbols: (data.len() as f64 / symbol_size as f64).ceil() as u32,
        symbol_size,
        repair_count,
        block_id,
    };
    let encoder = Encoder::new(data, params);
    let source = encoder.source_symbols();
    let repair = encoder.repair_symbols(repair_count);
    (source, repair, params)
}

/// Feed symbols into a fresh decoder and return decoded bytes (if any).
fn decode_symbols(
    symbols: &[WireSymbol],
    params: EncodingParams,
    transfer_length: u64,
) -> Option<Bytes> {
    let mut decoder = Decoder::new(params, transfer_length);
    let mut result = None;
    for sym in symbols {
        if let Some(data) = decoder.add_symbol(sym) {
            result = Some(data);
        }
    }
    result
}

/// Assert that decoded output matches the original data.
/// The fast-path (all source symbols present) may produce output padded to
/// a multiple of symbol_size, so we compare only the first `expected.len()` bytes.
fn assert_data_eq(decoded: &Bytes, expected: &[u8]) {
    assert!(
        decoded.len() >= expected.len(),
        "decoded length {} is shorter than expected {}",
        decoded.len(),
        expected.len(),
    );
    assert_eq!(
        &decoded[..expected.len()],
        expected,
        "decoded data does not match original"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. Feed exactly K-1 source symbols (no repair). Decoder must NOT produce output.
#[test]
fn test_k_minus_1_source_symbols_cannot_decode() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let repair_count = 5;
    let (source, _repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    assert!(k >= 2, "need at least 2 source symbols for this test");

    let mut decoder = Decoder::new(params, data.len() as u64);
    for sym in source.iter().take(k - 1) {
        let result = decoder.add_symbol(sym);
        assert!(
            result.is_none(),
            "decoder should not produce output with only K-1 source symbols"
        );
    }
    assert!(!decoder.is_decoded());
}

/// 2. Feed exactly K source symbols. Decoder MUST decode (fast path).
#[test]
fn test_exactly_k_source_symbols_decodes() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let repair_count = 5;
    let (source, _repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let mut decoder = Decoder::new(params, data.len() as u64);
    let mut decoded = None;
    for sym in &source {
        if let Some(d) = decoder.add_symbol(sym) {
            decoded = Some(d);
        }
    }
    assert!(decoder.is_decoded(), "decoder must be decoded after all K source symbols");
    let decoded = decoded.expect("must have decoded data");
    assert_data_eq(&decoded, &data);
}

/// 3. Boundary sweep: for K source + R repair, feed K, K+1, ... K+R symbols.
///    K should decode. Everything above K should also decode.
#[test]
fn test_boundary_sweep_k_to_k_plus_r() {
    let data = make_data(2048);
    let symbol_size = 256u16;
    let repair_count = 10;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    let r = repair.len();

    // Build a combined list: all source symbols first, then repair.
    let mut all_symbols: Vec<WireSymbol> = Vec::new();
    all_symbols.extend(source.clone());
    all_symbols.extend(repair.clone());

    // For each count from K to K+R, feed that many symbols and check.
    for count in k..=(k + r) {
        let feed: Vec<WireSymbol> = all_symbols.iter().take(count).cloned().collect();
        let result = decode_symbols(&feed, params, data.len() as u64);
        // K symbols should always be enough (fast path with all source).
        // Anything beyond K also has all source symbols, so must decode.
        assert!(
            result.is_some(),
            "failed to decode with {} symbols (K={}, R={})",
            count,
            k,
            r
        );
        assert_data_eq(&result.unwrap(), &data);
    }
}

/// 4. Drop source symbol 0 (first), add remaining source + 1 repair. Must recover.
#[test]
fn test_drop_first_source_recover_with_repair() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let repair_count = 5;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    // Drop first source symbol, keep the rest, add repair symbols.
    let mut symbols: Vec<WireSymbol> = source[1..].to_vec();
    symbols.extend(repair.iter().cloned());

    let result = decode_symbols(&symbols, params, data.len() as u64);
    let decoded = result.expect("must decode after dropping 1 source + adding repair");
    assert_data_eq(&decoded, &data);
}

/// 5. Drop last source symbol, recover with repair.
#[test]
fn test_drop_last_source_recover_with_repair() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let repair_count = 5;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    let mut symbols: Vec<WireSymbol> = source[..k - 1].to_vec();
    symbols.extend(repair.iter().cloned());

    let result = decode_symbols(&symbols, params, data.len() as u64);
    let decoded = result.expect("must decode after dropping last source + adding repair");
    assert_data_eq(&decoded, &data);
}

/// 6. Burst loss: drop the first half of source symbols, recover with enough repair.
#[test]
fn test_burst_loss_first_half() {
    let data = make_data(4096);
    let symbol_size = 256u16;
    let repair_count = 20; // plenty of repair
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    let half = k / 2;
    // Keep only the second half of source symbols.
    let mut symbols: Vec<WireSymbol> = source[half..].to_vec();
    symbols.extend(repair.iter().cloned());

    let result = decode_symbols(&symbols, params, data.len() as u64);
    let decoded = result.expect("must decode after first-half burst loss with repair");
    assert_data_eq(&decoded, &data);
}

/// 7. Burst loss: drop the second half of source symbols, recover with enough repair.
#[test]
fn test_burst_loss_second_half() {
    let data = make_data(4096);
    let symbol_size = 256u16;
    let repair_count = 20;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    let half = k / 2;
    // Keep only the first half of source symbols.
    let mut symbols: Vec<WireSymbol> = source[..half].to_vec();
    symbols.extend(repair.iter().cloned());

    let result = decode_symbols(&symbols, params, data.len() as u64);
    let decoded = result.expect("must decode after second-half burst loss with repair");
    assert_data_eq(&decoded, &data);
}

/// 8. Interleaved blocks: create 2 blocks with different block_ids.
///    Feed symbols interleaved. Both must decode correctly.
#[test]
fn test_interleaved_blocks() {
    let data0 = make_data(2048);
    let data1: Vec<u8> = (0..2048).map(|i| ((i * 7 + 13) % 251) as u8).collect();
    let symbol_size = 256u16;
    let repair_count = 5;

    let (source0, _repair0, params0) = encode_block(&data0, symbol_size, repair_count, 0);
    let (source1, _repair1, params1) = encode_block(&data1, symbol_size, repair_count, 1);

    // Interleave: block0 sym0, block1 sym0, block0 sym1, block1 sym1, ...
    let max_len = source0.len().max(source1.len());
    let mut decoder0 = Decoder::new(params0, data0.len() as u64);
    let mut decoder1 = Decoder::new(params1, data1.len() as u64);
    let mut result0: Option<Bytes> = None;
    let mut result1: Option<Bytes> = None;

    for i in 0..max_len {
        if i < source0.len() {
            if let Some(d) = decoder0.add_symbol(&source0[i]) {
                result0 = Some(d);
            }
        }
        if i < source1.len() {
            if let Some(d) = decoder1.add_symbol(&source1[i]) {
                result1 = Some(d);
            }
        }
    }

    assert!(decoder0.is_decoded(), "block 0 must decode");
    assert!(decoder1.is_decoded(), "block 1 must decode");
    assert_data_eq(&result0.unwrap(), &data0);
    assert_data_eq(&result1.unwrap(), &data1);
}

/// 9. Shuffle all symbols randomly with a fixed seed, feed in random order.
///    Must decode correctly.
#[test]
fn test_symbol_reordering() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let repair_count = 5;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let mut all_symbols: Vec<WireSymbol> = Vec::new();
    all_symbols.extend(source);
    all_symbols.extend(repair);

    // Shuffle with a fixed seed for reproducibility.
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    all_symbols.shuffle(&mut rng);

    let result = decode_symbols(&all_symbols, params, data.len() as u64);
    let decoded = result.expect("must decode after shuffling symbols");
    assert_data_eq(&decoded, &data);
}

/// 10. Drop ALL source symbols. Feed repair symbols only.
///     RaptorQ typically needs ~K+2 repair symbols to decode without any source.
#[test]
fn test_repair_only_minimum_symbols() {
    let data = make_data(4096);
    let symbol_size = 512u16;
    let k = (data.len() as f64 / symbol_size as f64).ceil() as u32; // 8
    // Generate generous repair: K+10 to ensure decoding succeeds.
    let repair_count = k + 10;
    let (_source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    // Feed only repair symbols — no source at all.
    let result = decode_symbols(&repair, params, data.len() as u64);
    let decoded = result.expect(
        "must decode from repair symbols only (feeding K+10 repair for K source symbols)",
    );
    assert_data_eq(&decoded, &data);
}

/// 11. Data integrity across varied sizes.
///     Test blocks of sizes 1, 100, 1200, 4096, 16384, 65536 bytes.
#[test]
fn test_data_integrity_varied_sizes() {
    let sizes = [1usize, 100, 1200, 4096, 16384, 65536];
    let symbol_size = 256u16;
    let repair_count = 5;

    for &size in &sizes {
        let data = make_data(size);
        let params = make_params(size, symbol_size, repair_count);
        let encoder = Encoder::new(&data, params);
        let source = encoder.source_symbols();
        let _repair = encoder.repair_symbols(repair_count);

        let result = decode_symbols(&source, params, data.len() as u64);
        let decoded = result.unwrap_or_else(|| {
            panic!("must decode data of size {} with all source symbols", size)
        });
        assert_data_eq(&decoded, &data);
    }
}

/// 12. Massive loss with insufficient repair: drop more than R source symbols
///     when only R repair are available, so total symbols < K. Must fail to decode.
#[test]
fn test_massive_loss_insufficient_repair() {
    let data = make_data(1000);
    let symbol_size = 100u16;
    let repair_count = 3;
    let (source, repair, params) = encode_block(&data, symbol_size, repair_count, 0);

    let k = source.len();
    assert!(k >= 6, "need at least 6 source symbols for this test, got {k}");

    // Drop more than R source symbols so total < K.
    // Keep only k/2 source symbols (rounded down), add all R repair.
    let keep = k / 2;
    let mut symbols: Vec<WireSymbol> = source[..keep].to_vec();
    symbols.extend(repair.iter().cloned());
    let total = symbols.len();
    assert!(
        total < k,
        "total symbols ({total}) must be fewer than K ({k}) for this test"
    );

    let mut decoder = Decoder::new(params, data.len() as u64);
    let mut decoded = false;
    for sym in &symbols {
        if decoder.add_symbol(sym).is_some() {
            decoded = true;
        }
    }
    assert!(
        !decoded,
        "decoder should NOT succeed with only {total} of {k} required symbols"
    );
    assert!(!decoder.is_decoded());
    assert_eq!(decoder.total_fed(), total as u32);
}
