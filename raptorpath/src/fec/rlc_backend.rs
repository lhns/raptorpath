//! Random Linear Code (RLC) FEC backend.
//!
//! Each repair symbol is a random linear combination of source symbols over GF(2^8).
//! Coefficients are deterministic: seeded by `block_id` and `repair_index`, so the
//! receiver can regenerate them without in-band metadata (only a 4-byte repair_index
//! header is needed per repair symbol).
//!
//! Decoding uses incremental Gaussian elimination over GF(2^8). When rank reaches k,
//! back-substitution recovers all source symbols.
//!
//! Properties:
//! - Truly rateless: unlimited repair symbols (each with fresh PRNG seed)
//! - Near-MDS: GF(256) random matrix is full rank with probability ≈ 1 - 2^(-8)
//! - Foundation for sliding-window FEC (same codec, different framing)
//! - Standardized: RFC 8681 (RLC for FECFRAME)

use bytes::Bytes;
use std::collections::HashSet;
use std::time::Instant;

use super::gf256;
use super::traits::{EncodingParams, FecBackend, FecDecoder, FecEncoder, WireSymbol};

/// Generate deterministic coefficients for a repair symbol.
fn generate_coefficients(block_id: u64, repair_index: u32, k: usize) -> Vec<u8> {
    let seed = (block_id << 32) | repair_index as u64;
    let mut rng = gf256::SplitMix64::new(seed);
    (0..k).map(|_| rng.next_nonzero_u8()).collect()
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

pub struct RlcEncoder {
    params: EncodingParams,
    source_shards: Vec<Vec<u8>>,
}

impl RlcEncoder {
    pub fn new(data: &[u8], params: EncodingParams) -> Self {
        let k = params.source_symbols as usize;
        let symbol_size = params.symbol_size as usize;

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
        while source_shards.len() < k {
            source_shards.push(vec![0u8; symbol_size]);
        }

        Self {
            params,
            source_shards,
        }
    }
}

impl FecEncoder for RlcEncoder {
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
                backend: FecBackend::Rlc,
            })
            .collect()
    }

    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol> {
        self.repair_symbols_from(0, count)
    }

    fn repair_symbols_from(&self, start: u32, count: u32) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        let k = self.params.source_symbols as usize;
        let symbol_size = self.params.symbol_size as usize;

        (start..start.saturating_add(count))
            .map(|i| {
                let coeffs = generate_coefficients(block_id, i, k);

                // Compute repair = Σ coeffs[j] * source[j] over GF(2^8)
                let mut coded = vec![0u8; symbol_size];
                for (j, &coeff) in coeffs.iter().enumerate() {
                    gf256::mul_acc_slice(coeff, &self.source_shards[j], &mut coded);
                }

                // Wire format: [repair_index(4 bytes LE)][coded_data]
                let mut wire_data = Vec::with_capacity(4 + symbol_size);
                wire_data.extend_from_slice(&i.to_le_bytes());
                wire_data.extend_from_slice(&coded);

                WireSymbol {
                    block_id,
                    payload_id: k as u32 + i,
                    is_repair: true,
                    data: wire_data,
                    backend: FecBackend::Rlc,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Decoder — incremental Gaussian elimination over GF(2^8)
// ---------------------------------------------------------------------------

pub struct RlcDecoder {
    params: EncodingParams,
    transfer_length: u64,
    /// Coefficient matrix rows (each row has k entries). Indexed by pivot column.
    /// coeff_rows[col] = Some((coefficients, data)) if pivot at `col` is filled.
    pivot_rows: Vec<Option<(Vec<u8>, Vec<u8>)>>,
    /// Number of pivots filled
    rank: usize,
    total_fed: u32,
    decoded: bool,
    result: Option<Bytes>,
    seen_ids: HashSet<u32>,
    created: Instant,
}

impl RlcDecoder {
    pub fn new(params: EncodingParams, transfer_length: u64) -> Self {
        let k = params.source_symbols as usize;
        Self {
            params,
            transfer_length,
            pivot_rows: vec![None; k],
            rank: 0,
            total_fed: 0,
            decoded: false,
            result: None,
            seen_ids: HashSet::new(),
            created: Instant::now(),
        }
    }

    /// Try to insert a row (coefficients, data) into the pivot matrix.
    /// Uses partial pivoting with incremental forward elimination.
    fn insert_row(&mut self, mut coeffs: Vec<u8>, mut data: Vec<u8>) -> bool {
        let k = self.params.source_symbols as usize;

        // Forward elimination against existing pivots
        for col in 0..k {
            if coeffs[col] == 0 {
                continue;
            }
            if let Some((ref pivot_coeffs, ref pivot_data)) = self.pivot_rows[col] {
                // Eliminate: row -= (row[col] / pivot[col]) * pivot_row
                // In GF(2^8): row[col] * inv(pivot[col]) = scale factor
                let scale = gf256::mul(coeffs[col], gf256::inv(pivot_coeffs[col]));
                for j in col..k {
                    coeffs[j] = gf256::add(coeffs[j], gf256::mul(scale, pivot_coeffs[j]));
                }
                // Eliminate data
                gf256::mul_acc_slice(scale, pivot_data, &mut data);
            }
        }

        // Find the first non-zero coefficient — that's our pivot column
        for col in 0..k {
            if coeffs[col] != 0 {
                // Normalize: divide entire row by the pivot element
                let inv_pivot = gf256::inv(coeffs[col]);
                for j in col..k {
                    coeffs[j] = gf256::mul(coeffs[j], inv_pivot);
                }
                let mut normalized_data = vec![0u8; data.len()];
                gf256::mul_slice(inv_pivot, &data, &mut normalized_data);

                self.pivot_rows[col] = Some((coeffs, normalized_data));
                self.rank += 1;
                return true;
            }
        }

        // All coefficients are zero — linearly dependent row
        false
    }

    /// Back-substitution: recover source symbols from the upper-triangular pivot matrix.
    fn back_substitute(&mut self) -> Option<Bytes> {
        let k = self.params.source_symbols as usize;

        // Process from last pivot to first
        for col in (0..k).rev() {
            let (coeffs, data) = match self.pivot_rows[col].take() {
                Some(row) => row,
                None => return None, // shouldn't happen if rank == k
            };

            // Eliminate this column from all earlier rows
            for earlier_col in 0..col {
                if let Some((ref mut ec, ref mut ed)) = self.pivot_rows[earlier_col] {
                    if ec[col] != 0 {
                        let scale = ec[col]; // already normalized, pivot[col] = 1
                        ec[col] = 0;
                        gf256::mul_acc_slice(scale, &data, ed);
                    }
                }
            }

            self.pivot_rows[col] = Some((coeffs, data));
        }

        // Extract source symbols in order
        let mut result = Vec::with_capacity(k * self.params.symbol_size as usize);
        for col in 0..k {
            if let Some((_, ref data)) = self.pivot_rows[col] {
                result.extend_from_slice(data);
            } else {
                return None;
            }
        }

        let truncated = &result[..std::cmp::min(result.len(), self.transfer_length as usize)];
        self.decoded = true;
        self.result = Some(Bytes::copy_from_slice(truncated));
        self.result.clone()
    }
}

impl FecDecoder for RlcDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes> {
        if self.decoded {
            return self.result.clone();
        }

        if symbol.backend != FecBackend::Rlc {
            return None;
        }

        if !self.seen_ids.insert(symbol.payload_id) {
            return None;
        }

        self.total_fed += 1;

        let k = self.params.source_symbols as usize;
        let symbol_size = self.params.symbol_size as usize;

        if !symbol.is_repair {
            // Source symbol: coefficient row is unit vector e_j
            let j = symbol.payload_id as usize;
            if j >= k {
                return None;
            }
            let mut coeffs = vec![0u8; k];
            coeffs[j] = 1;
            let mut data = vec![0u8; symbol_size];
            let copy_len = symbol.data.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&symbol.data[..copy_len]);
            self.insert_row(coeffs, data);
        } else {
            // Repair symbol: [repair_index(4 bytes LE)][coded_data]
            if symbol.data.len() < 4 {
                return None;
            }
            let repair_index = u32::from_le_bytes(symbol.data[0..4].try_into().unwrap());
            let coded = &symbol.data[4..];

            let coeffs = generate_coefficients(symbol.block_id, repair_index, k);
            let mut data = vec![0u8; symbol_size];
            let copy_len = coded.len().min(symbol_size);
            data[..copy_len].copy_from_slice(&coded[..copy_len]);

            self.insert_row(coeffs, data);
        }

        // When rank reaches k, back-substitute to recover
        if self.rank >= k {
            return self.back_substitute();
        }

        None
    }

    fn is_complete_source(&self) -> bool {
        // All source symbol pivots are filled (no repair needed)
        let k = self.params.source_symbols as usize;
        (0..k).all(|col| {
            if let Some((ref coeffs, _)) = self.pivot_rows[col] {
                // Check it's a unit vector at col (pure source symbol)
                coeffs[col] == 1
                    && coeffs.iter().enumerate().all(|(j, &c)| j == col || c == 0)
            } else {
                false
            }
        }) && !self.decoded
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
        if index >= self.params.source_symbols as usize {
            return None;
        }
        self.pivot_rows.get(index)?.as_ref().map(|(_, data)| data.as_slice())
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
    fn test_coefficient_determinism() {
        // Same (block_id, repair_index) → same coefficients
        let c1 = generate_coefficients(42, 7, 10);
        let c2 = generate_coefficients(42, 7, 10);
        assert_eq!(c1, c2);

        // Different repair_index → different coefficients
        let c3 = generate_coefficients(42, 8, 10);
        assert_ne!(c1, c3);
    }

    #[test]
    fn test_nonzero_coefficients() {
        // next_nonzero_u8() never returns 0 over 1000 iterations
        let mut rng = gf256::SplitMix64::new(12345);
        for _ in 0..1000 {
            assert_ne!(rng.next_nonzero_u8(), 0);
        }
    }

    #[test]
    fn test_repair_only_recovery() {
        // k=4, feed only repair symbols (no source) → GE decode succeeds
        let data = vec![42u8; 800]; // 4 symbols of 200
        let params = make_params(4, 200, 10);
        let encoder = RlcEncoder::new(&data, params);
        let repairs = encoder.repair_symbols(10);

        let mut decoder = RlcDecoder::new(params, data.len() as u64);
        let mut decoded = false;
        for repair in &repairs {
            if let Some(result) = decoder.add_symbol(repair) {
                assert_eq!(&result[..data.len()], &data[..]);
                decoded = true;
                break;
            }
        }
        assert!(decoded, "RLC should decode from repair-only with k=4");
    }

    #[test]
    fn test_linearly_dependent_row() {
        // Two repairs with identical seed → second doesn't increase rank
        let data = vec![11u8; 400];
        let params = make_params(2, 200, 2);
        let encoder = RlcEncoder::new(&data, params);
        let repairs = encoder.repair_symbols(1);

        let mut decoder = RlcDecoder::new(params, data.len() as u64);
        decoder.add_symbol(&repairs[0]);
        assert_eq!(decoder.rank, 1);

        // Feed an identical symbol (same payload_id → deduped, rank stays)
        decoder.add_symbol(&repairs[0]);
        assert_eq!(decoder.rank, 1);
    }

    #[test]
    fn test_repair_wire_format() {
        // Repair data starts with 4-byte LE repair_index header
        let data = vec![5u8; 400];
        let params = make_params(2, 200, 3);
        let encoder = RlcEncoder::new(&data, params);
        let repairs = encoder.repair_symbols(3);

        for (i, repair) in repairs.iter().enumerate() {
            assert!(repair.is_repair);
            assert!(repair.data.len() >= 4);
            let repair_index = u32::from_le_bytes(repair.data[0..4].try_into().unwrap());
            assert_eq!(repair_index, i as u32);
        }
    }
}
