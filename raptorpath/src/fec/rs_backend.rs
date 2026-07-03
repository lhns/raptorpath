//! Reed-Solomon FEC backend.
//!
//! Wraps the `reed_solomon_erasure` crate to implement FecEncoder/FecDecoder traits.
//! Reed-Solomon is an MDS (Maximum Distance Separable) code: any k of n symbols
//! are sufficient to recover the original data, with zero overhead.
//!
//! Limitations:
//! - Not rateless: max n = 255 (GF(2^8)), so k + r ≤ 255
//! - Pre-generates all repair symbols upfront (no streaming generation)

use bytes::Bytes;
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::HashSet;
use std::time::Instant;
use tracing::warn;

use super::traits::{EncodingParams, FecBackend, FecDecoder, FecEncoder, WireSymbol};

/// Reed-Solomon encoder.
pub struct ReedSolomonEncoder {
    params: EncodingParams,
    /// Source shards (k shards of symbol_size bytes each)
    source_shards: Vec<Vec<u8>>,
    /// Pre-generated repair shards
    repair_shards: Vec<Vec<u8>>,
}

impl ReedSolomonEncoder {
    pub fn new(data: &[u8], params: EncodingParams) -> Self {
        let k = params.source_symbols as usize;
        let symbol_size = params.symbol_size as usize;

        // Split data into k source shards
        let mut source_shards: Vec<Vec<u8>> = Vec::with_capacity(k);
        for chunk in data.chunks(symbol_size) {
            if chunk.len() == symbol_size {
                source_shards.push(chunk.to_vec());
            } else {
                let mut padded = vec![0u8; symbol_size];
                padded[..chunk.len()].copy_from_slice(chunk);
                source_shards.push(padded);
            }
        }
        // Pad with zero shards if data produces fewer than k shards
        while source_shards.len() < k {
            source_shards.push(vec![0u8; symbol_size]);
        }

        // Cap repair count at 255 - k (GF(2^8) limit)
        let max_repair = 255usize.saturating_sub(k);
        let r = (params.repair_count as usize).min(max_repair);

        let repair_shards = if r > 0 && k > 0 {
            // Build the full shard matrix: k source + r parity
            let mut shards: Vec<Vec<u8>> = source_shards.clone();
            shards.extend((0..r).map(|_| vec![0u8; symbol_size]));

            let rs = ReedSolomon::new(k, r).expect("RS::new failed");
            rs.encode(&mut shards).expect("RS encode failed");

            // Extract the parity shards (indices k..k+r)
            shards.split_off(k)
        } else {
            Vec::new()
        };

        Self {
            params,
            source_shards,
            repair_shards,
        }
    }
}

impl FecEncoder for ReedSolomonEncoder {
    fn source_symbols(&self) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        self.source_shards
            .iter()
            .enumerate()
            .map(|(i, shard)| WireSymbol {
                block_id,
                payload_id: i as u32,
                is_repair: false,
                data: shard.clone(),
                backend: FecBackend::ReedSolomon,
            })
            .collect()
    }

    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        let available = self.repair_shards.len();
        let requested = count as usize;

        if requested > available {
            warn!(
                requested,
                available, "RS: requested more repair symbols than pre-generated"
            );
        }

        self.repair_shards
            .iter()
            .take(requested)
            .enumerate()
            .map(|(i, shard)| WireSymbol {
                block_id,
                payload_id: (self.params.source_symbols + i as u32),
                is_repair: true,
                data: shard.clone(),
                backend: FecBackend::ReedSolomon,
            })
            .collect()
    }

    /// RS is fixed-rate: only the pre-generated parity shards exist.
    fn max_repairs(&self) -> u32 {
        self.repair_shards.len() as u32
    }
}

/// Reed-Solomon decoder.
pub struct ReedSolomonDecoder {
    params: EncodingParams,
    transfer_length: u64,
    /// Shard slots: k source + r repair. None = not yet received.
    shards: Vec<Option<Vec<u8>>>,
    /// Number of shards received
    received_count: u32,
    /// Number of source shards received
    source_count: u32,
    total_fed: u32,
    decoded: bool,
    result: Option<Bytes>,
    seen_ids: HashSet<u32>,
    created: Instant,
    /// Max repair shards (capped at 255 - k)
    max_repair: usize,
}

impl ReedSolomonDecoder {
    pub fn new(params: EncodingParams, transfer_length: u64) -> Self {
        let k = params.source_symbols as usize;
        let max_repair = 255usize.saturating_sub(k).min(params.repair_count as usize);
        let total_shards = k + max_repair;

        Self {
            params,
            transfer_length,
            shards: vec![None; total_shards],
            received_count: 0,
            source_count: 0,
            total_fed: 0,
            decoded: false,
            result: None,
            seen_ids: HashSet::new(),
            created: Instant::now(),
            max_repair,
        }
    }

    fn try_decode(&mut self) -> Option<Bytes> {
        let k = self.params.source_symbols as usize;
        let r = self.max_repair;
        if k == 0 || r == 0 {
            // Degenerate: if r=0, we need all k source shards
            // Check if all source shards are present
            if self.shards[..k].iter().all(|s| s.is_some()) {
                let data: Vec<u8> = self.shards[..k]
                    .iter()
                    .flat_map(|s| s.as_ref().unwrap().iter().copied())
                    .collect();
                let truncated = &data[..std::cmp::min(data.len(), self.transfer_length as usize)];
                self.decoded = true;
                self.result = Some(Bytes::copy_from_slice(truncated));
                return self.result.clone();
            }
            return None;
        }

        let rs = ReedSolomon::new(k, r).expect("RS::new failed");

        // reconstruct() works in-place on a Vec<Option<Vec<u8>>>
        let mut shards = self.shards.clone();
        if rs.reconstruct(&mut shards).is_ok() {
            // Extract source shards
            let data: Vec<u8> = shards[..k]
                .iter()
                .flat_map(|s| s.as_ref().unwrap().iter().copied())
                .collect();
            let truncated = &data[..std::cmp::min(data.len(), self.transfer_length as usize)];
            self.decoded = true;
            self.result = Some(Bytes::copy_from_slice(truncated));
            self.result.clone()
        } else {
            None
        }
    }
}

impl FecDecoder for ReedSolomonDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes> {
        if self.decoded {
            return self.result.clone();
        }

        if symbol.backend != FecBackend::ReedSolomon {
            return None;
        }

        if !self.seen_ids.insert(symbol.payload_id) {
            return None;
        }

        self.total_fed += 1;

        let idx = symbol.payload_id as usize;
        if idx >= self.shards.len() {
            return None; // out of bounds
        }

        let symbol_size = self.params.symbol_size as usize;
        let mut shard = vec![0u8; symbol_size];
        let copy_len = symbol.data.len().min(symbol_size);
        shard[..copy_len].copy_from_slice(&symbol.data[..copy_len]);
        self.shards[idx] = Some(shard);
        self.received_count += 1;
        if !symbol.is_repair {
            self.source_count += 1;
        }

        // Need at least k shards to attempt decode
        let k = self.params.source_symbols as usize;
        if self.received_count as usize >= k {
            return self.try_decode();
        }

        None
    }

    fn is_complete_source(&self) -> bool {
        self.params.source_symbols > 0
            && self.source_count == self.params.source_symbols
    }

    fn is_decoded(&self) -> bool {
        self.decoded
    }

    fn total_fed(&self) -> u32 {
        self.total_fed
    }

    fn params(&self) -> &EncodingParams {
        &self.params
    }

    fn get_source_symbol(&self, index: usize) -> Option<&[u8]> {
        self.shards.get(index)?.as_deref()
    }

    fn received_ids(&self) -> Vec<u32> {
        self.seen_ids.iter().copied().collect()
    }

    fn created_at(&self) -> Instant {
        self.created
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_params(k: u32, symbol_size: u16, repair_count: u32) -> EncodingParams {
        EncodingParams {
            source_symbols: k,
            symbol_size,
            repair_count,
            block_id: 0,
        }
    }

    #[test]
    fn test_repair_count_capped_at_255_minus_k() {
        // k=200, repair=100 → encoder caps at 55
        let data = vec![1u8; 200 * 100]; // k=200, symbol_size=100
        let params = make_params(200, 100, 100);
        let encoder = ReedSolomonEncoder::new(&data, params);
        let repairs = encoder.repair_symbols(100);
        assert_eq!(repairs.len(), 55, "Should cap at 255 - 200 = 55");
    }

    #[test]
    fn test_padding_short_data() {
        // Data shorter than k×symbol_size → padded, round-trip works
        let data = vec![42u8; 100]; // Only 100 bytes but k=4, symbol_size=200
        let params = make_params(4, 200, 4);
        let encoder = ReedSolomonEncoder::new(&data, params);
        let sources = encoder.source_symbols();
        let repairs = encoder.repair_symbols(4);

        let mut decoder = ReedSolomonDecoder::new(params, data.len() as u64);
        // Drop source 1, feed the rest + 1 repair
        decoder.add_symbol(&sources[0]);
        decoder.add_symbol(&sources[2]);
        decoder.add_symbol(&sources[3]);
        let result = decoder.add_symbol(&repairs[0]);
        assert!(result.is_some());
        assert_eq!(&result.unwrap()[..data.len()], &data[..]);
    }

    #[test]
    fn test_zero_repair_all_sources() {
        // r=0, all sources arrive → decode succeeds
        let data = vec![7u8; 600];
        let params = make_params(3, 200, 0);
        let encoder = ReedSolomonEncoder::new(&data, params);
        let sources = encoder.source_symbols();

        let mut decoder = ReedSolomonDecoder::new(params, data.len() as u64);
        decoder.add_symbol(&sources[0]);
        decoder.add_symbol(&sources[1]);
        let result = decoder.add_symbol(&sources[2]);
        assert!(result.is_some());
        assert_eq!(&result.unwrap()[..], &data[..]);
    }

    #[test]
    fn test_out_of_bounds_payload_id() {
        // payload_id >= k+r → None
        let data = vec![1u8; 400];
        let params = make_params(2, 200, 2);
        let mut decoder = ReedSolomonDecoder::new(params, data.len() as u64);

        let bad_sym = WireSymbol {
            block_id: 0,
            payload_id: 100, // way out of bounds
            is_repair: true,
            data: vec![0u8; 200],
            backend: FecBackend::ReedSolomon,
        };
        assert!(decoder.add_symbol(&bad_sym).is_none());
    }

    #[test]
    fn test_single_loss_repair_recovery() {
        // Drop 1 source, feed 1 repair → decode succeeds
        let data = vec![88u8; 1000];
        let params = make_params(5, 200, 5);
        let encoder = ReedSolomonEncoder::new(&data, params);
        let sources = encoder.source_symbols();
        let repairs = encoder.repair_symbols(5);

        let mut decoder = ReedSolomonDecoder::new(params, data.len() as u64);
        // Feed all sources except index 2
        for (i, src) in sources.iter().enumerate() {
            if i == 2 {
                continue;
            }
            decoder.add_symbol(src);
        }
        // Feed one repair → should decode
        let result = decoder.add_symbol(&repairs[0]);
        assert!(result.is_some());
        assert_eq!(&result.unwrap()[..data.len()], &data[..]);
    }
}
