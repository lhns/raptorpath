//! METTLE FEC backend adapter.
//!
//! Wraps the standalone `mettle` crate to implement the FecEncoder/FecDecoder traits.
//! Maps raptorpath's block-based API onto METTLE's streaming encoder/decoder.

use bytes::Bytes;
use mettle::{MettleConfig, MettleDecoder, MettleEncoder};
use std::collections::HashSet;
use std::time::Instant;

use super::traits::{EncodingParams, FecDecoder, FecEncoder, WireSymbol};

/// METTLE encoder adapted for raptorpath's block-based interface.
///
/// Takes a data block, splits it into source symbols of `symbol_size`, and encodes
/// using METTLE's streaming encoder. The coded packets become repair symbols.
pub struct MettleBlockEncoder {
    params: EncodingParams,
    encoder: MettleEncoder,
}

impl MettleBlockEncoder {
    pub fn new(data: &[u8], params: EncodingParams) -> Self {
        let config = MettleConfig::small_window();
        // Use block_id as seed for deterministic, per-block graph generation
        let seed = params.block_id;

        let mut encoder = MettleEncoder::new(config, seed);

        // Split data into source symbols of symbol_size bytes
        let symbol_size = params.symbol_size as usize;
        for chunk in data.chunks(symbol_size) {
            // Pad last chunk to symbol_size if needed
            if chunk.len() == symbol_size {
                encoder.add_source_packet(chunk);
            } else {
                let mut padded = vec![0u8; symbol_size];
                padded[..chunk.len()].copy_from_slice(chunk);
                encoder.add_source_packet(&padded);
            }
        }

        Self { params, encoder }
    }
}

impl FecEncoder for MettleBlockEncoder {
    fn source_symbols(&self) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        self.encoder
            .source_packets()
            .iter()
            .enumerate()
            .map(|(i, data)| WireSymbol {
                block_id,
                payload_id: i as u32,
                is_repair: false,
                data: data.clone(),
                backend: super::traits::FecBackend::Mettle,
            })
            .collect()
    }

    fn repair_symbols(&self, _count: u32) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        let coded = self.encoder.coded_packets();
        let num_source = self.encoder.num_source();

        // METTLE is a fixed-rate code — the peeling decoder needs the complete
        // bin structure to cascade. Unlike rateless codes (RaptorQ/RLC) where
        // each repair is independently useful, METTLE's bins form an
        // interdependent graph. Return ALL coded bins regardless of `count`;
        // the decoder stops processing once it completes.
        coded
            .into_iter()
            .map(|cp| {
                // Encode bin_index and members into the repair symbol data.
                // The decoder needs members to know which source positions are in this bin.
                // We encode: [bin_index(4 bytes)][num_members(4 bytes)][members...][coded_data]
                let mut wire_data = Vec::new();
                wire_data.extend_from_slice(&(cp.bin_index as u32).to_le_bytes());
                wire_data.extend_from_slice(&(cp.members.len() as u32).to_le_bytes());
                for &m in &cp.members {
                    wire_data.extend_from_slice(&(m as u32).to_le_bytes());
                }
                wire_data.extend_from_slice(&cp.data);

                WireSymbol {
                    block_id,
                    // Offset payload_id past source symbols to avoid collision
                    payload_id: (num_source + cp.bin_index) as u32,
                    is_repair: true,
                    data: wire_data,
                    backend: super::traits::FecBackend::Mettle,
                }
            })
            .collect()
    }
}

/// METTLE decoder adapted for raptorpath's block-based interface.
pub struct MettleBlockDecoder {
    params: EncodingParams,
    decoder: MettleDecoder,
    decoded: bool,
    result: Option<Bytes>,
    total_fed: u32,
    seen_ids: HashSet<u32>,
    created: Instant,
    transfer_length: u64,
    /// Symbols rejected due to out-of-bounds or adversarial values
    rejected_symbols: u32,
}

impl MettleBlockDecoder {
    pub fn new(params: EncodingParams, transfer_length: u64) -> Self {
        let config = MettleConfig::small_window();
        let seed = params.block_id;
        // Compute actual number of source symbols from data length and symbol size,
        // not from params.source_symbols which may represent application-level packet
        // count rather than FEC symbol count.
        let num_source = (transfer_length as usize + params.symbol_size as usize - 1)
            / params.symbol_size as usize;

        Self {
            params,
            decoder: MettleDecoder::new(config, num_source, seed),
            decoded: false,
            result: None,
            total_fed: 0,
            seen_ids: HashSet::new(),
            created: Instant::now(),
            transfer_length,
            rejected_symbols: 0,
        }
    }

    /// Try to extract the decoded block, truncated to transfer_length.
    fn try_complete(&mut self) -> Option<Bytes> {
        if let Some(data) = self.decoder.recovered_data() {
            self.decoded = true;
            // Truncate to actual transfer length (last symbol may have been padded)
            let truncated = &data[..std::cmp::min(data.len(), self.transfer_length as usize)];
            self.result = Some(Bytes::copy_from_slice(truncated));
            self.result.clone()
        } else {
            None
        }
    }
}

impl FecDecoder for MettleBlockDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes> {
        if self.decoded {
            return self.result.clone();
        }

        // Reject symbols from a different backend
        if symbol.backend != super::traits::FecBackend::Mettle {
            return None;
        }

        // Deduplicate
        if !self.seen_ids.insert(symbol.payload_id) {
            return None;
        }

        self.total_fed += 1;

        if !symbol.is_repair {
            // Source symbol: payload_id is the position index
            let position = symbol.payload_id as usize;
            self.decoder.add_source_packet(position, &symbol.data);
        } else {
            // Repair symbol: decode the encoded bin_index + members + coded_data
            let data = &symbol.data;
            if data.len() < 8 {
                self.rejected_symbols += 1;
                return None; // malformed
            }
            let bin_index = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;

            // Reject adversarial bin_index values to prevent HashMap pressure
            let config = mettle::MettleConfig::small_window();
            let max_bin = mettle::graph::total_bins(
                self.params.source_symbols as usize,
                &config,
            ) * 2;
            if bin_index >= max_bin {
                self.rejected_symbols += 1;
                return None; // adversarial bin_index
            }
            let num_members = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;

            // Overflow guard: num_members * 4 can overflow on 32-bit or with
            // malicious input. Also sanity-check against source_symbols.
            let source_symbols = self.params.source_symbols as usize;
            if num_members > source_symbols {
                self.rejected_symbols += 1;
                return None; // malformed: more members than source symbols
            }
            let members_bytes = match num_members.checked_mul(4) {
                Some(n) => n,
                None => return None, // overflow
            };
            let members_end = match members_bytes.checked_add(8) {
                Some(n) => n,
                None => return None, // overflow
            };
            if data.len() < members_end {
                return None; // malformed
            }
            let members: Vec<usize> = (0..num_members)
                .map(|j| {
                    let offset = 8 + j * 4;
                    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize
                })
                .collect();
            let coded_data = data[members_end..].to_vec();

            let cp = mettle::CodedPacket {
                bin_index,
                data: coded_data,
                members,
            };
            self.decoder.add_coded_packet(&cp);
        }

        // Check if decoding is complete
        if self.decoder.is_complete() {
            return self.try_complete();
        }

        None
    }

    fn is_complete_source(&self) -> bool {
        self.decoder.is_complete() && !self.decoded
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
        self.decoder.get_source(index)
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
    use super::super::traits::FecBackend;

    fn make_params(k: u32, symbol_size: u16, repair_count: u32) -> EncodingParams {
        EncodingParams {
            source_symbols: k,
            symbol_size,
            repair_count,
            block_id: 42,
        }
    }

    #[test]
    fn test_malformed_repair_too_short() {
        // Repair with data.len() < 8 → None
        let params = make_params(4, 200, 4);
        let mut decoder = MettleBlockDecoder::new(params, 800);

        let bad_sym = WireSymbol {
            block_id: 42,
            payload_id: 10,
            is_repair: true,
            data: vec![0u8; 4], // too short
            backend: FecBackend::Mettle,
        };
        assert!(decoder.add_symbol(&bad_sym).is_none());
        assert_eq!(decoder.rejected_symbols, 1);
    }

    #[test]
    fn test_adversarial_bin_index() {
        // Repair with bin_index > max_bin → rejected
        let params = make_params(4, 200, 4);
        let mut decoder = MettleBlockDecoder::new(params, 800);

        // Build a repair with an absurd bin_index
        let mut wire_data = Vec::new();
        wire_data.extend_from_slice(&999999u32.to_le_bytes()); // bin_index
        wire_data.extend_from_slice(&1u32.to_le_bytes()); // num_members
        wire_data.extend_from_slice(&0u32.to_le_bytes()); // member[0]
        wire_data.extend_from_slice(&vec![0u8; 200]); // coded data

        let bad_sym = WireSymbol {
            block_id: 42,
            payload_id: 100,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Mettle,
        };
        assert!(decoder.add_symbol(&bad_sym).is_none());
        assert_eq!(decoder.rejected_symbols, 1);
    }

    #[test]
    fn test_num_members_exceeds_source_count() {
        // num_members > source_symbols → rejected
        let params = make_params(4, 200, 4);
        let mut decoder = MettleBlockDecoder::new(params, 800);

        let mut wire_data = Vec::new();
        wire_data.extend_from_slice(&0u32.to_le_bytes()); // bin_index
        wire_data.extend_from_slice(&100u32.to_le_bytes()); // num_members > 4
        // No actual member data needed — should reject before reading members
        wire_data.extend_from_slice(&vec![0u8; 200]);

        let bad_sym = WireSymbol {
            block_id: 42,
            payload_id: 101,
            is_repair: true,
            data: wire_data,
            backend: FecBackend::Mettle,
        };
        assert!(decoder.add_symbol(&bad_sym).is_none());
        assert_eq!(decoder.rejected_symbols, 1);
    }

    #[test]
    fn test_round_trip_small_block() {
        // k=4, 1 loss, verify recovery
        let data = vec![77u8; 800]; // 4 symbols of 200 bytes
        let params = make_params(4, 200, 10);
        let encoder = MettleBlockEncoder::new(&data, params);
        let sources = encoder.source_symbols();
        let repairs = encoder.repair_symbols(10);

        let mut decoder = MettleBlockDecoder::new(params, data.len() as u64);
        // Feed all sources except index 1
        for (i, src) in sources.iter().enumerate() {
            if i == 1 {
                continue;
            }
            decoder.add_symbol(src);
        }
        // Feed repairs until decode
        let mut decoded = false;
        for repair in &repairs {
            if let Some(result) = decoder.add_symbol(repair) {
                assert_eq!(&result[..data.len()], &data[..]);
                decoded = true;
                break;
            }
        }
        assert!(decoded, "METTLE should recover from 1 loss with sufficient repair");
    }

    #[test]
    fn test_dedup_same_payload_id() {
        // Duplicate symbol → None
        let data = vec![3u8; 800];
        let params = make_params(4, 200, 4);
        let encoder = MettleBlockEncoder::new(&data, params);
        let sources = encoder.source_symbols();

        let mut decoder = MettleBlockDecoder::new(params, data.len() as u64);
        decoder.add_symbol(&sources[0]);
        assert_eq!(decoder.total_fed(), 1);

        // Same symbol again
        let result = decoder.add_symbol(&sources[0]);
        assert!(result.is_none());
        assert_eq!(decoder.total_fed(), 1);
    }

    #[test]
    fn test_repair_symbols_returns_all_bins() {
        // Regression: repair_symbols() must return ALL coded bins, not just `count`.
        let data = vec![0xABu8; 10_000]; // k=50 symbols of 200 bytes
        let params = make_params(50, 200, 5);
        let encoder = MettleBlockEncoder::new(&data, params);

        let repairs = encoder.repair_symbols(5);
        let total_coded = encoder.encoder.coded_packets().len();

        // Must return every coded bin, not just the first 5
        assert_eq!(
            repairs.len(),
            total_coded,
            "repair_symbols() should return ALL {} coded bins, got {}",
            total_coded,
            repairs.len()
        );
        assert!(
            repairs.len() > 5,
            "total coded bins ({}) should be >> 5",
            repairs.len()
        );
    }

    #[test]
    fn test_decoder_num_source_from_transfer_length() {
        // Regression: decoder must compute num_source from transfer_length / symbol_size,
        // NOT from params.source_symbols.
        // 50000 / 1200 = 41.67 → ceil = 42, but params says 50.
        let params = make_params(50, 1200, 10);
        let transfer_length: u64 = 50_000;
        let expected_k = (transfer_length as usize + 1200 - 1) / 1200; // 42
        assert_eq!(expected_k, 42);

        let mut decoder = MettleBlockDecoder::new(params, transfer_length);

        // Feed exactly 42 source symbols (payload_id 0..41)
        for i in 0..expected_k {
            let payload = vec![i as u8; 1200];
            let sym = WireSymbol {
                block_id: 42,
                payload_id: i as u32,
                is_repair: false,
                data: payload,
                backend: FecBackend::Mettle,
            };
            decoder.add_symbol(&sym);
        }

        assert!(
            decoder.is_decoded(),
            "Decoder should complete with {} source symbols (from transfer_length), not wait for {}",
            expected_k,
            params.source_symbols
        );
    }

    #[test]
    fn test_decoder_num_source_matches_encoder() {
        // End-to-end: encoder and decoder agree on num_source when
        // transfer_length is not a clean multiple of symbol_size.
        let transfer_length: u64 = 50_000;
        let symbol_size: u16 = 1200;
        let data: Vec<u8> = (0..transfer_length as usize).map(|i| (i % 251) as u8).collect();
        let params = make_params(50, symbol_size, 20);

        let encoder = MettleBlockEncoder::new(&data, params);
        let sources = encoder.source_symbols();
        let repairs = encoder.repair_symbols(20);

        // Encoder produces ceil(50000/1200) = 42 source symbols
        let expected_k = (transfer_length as usize + symbol_size as usize - 1) / symbol_size as usize;
        assert_eq!(sources.len(), expected_k);

        let mut decoder = MettleBlockDecoder::new(params, transfer_length);

        // Feed all source symbols
        for src in &sources {
            decoder.add_symbol(src);
        }
        assert!(
            decoder.is_decoded(),
            "Decoder should complete with all {} source symbols",
            sources.len()
        );

        // Also verify data integrity via a fresh decoder using some repair
        let mut decoder2 = MettleBlockDecoder::new(params, transfer_length);
        // Feed all sources except index 0
        for src in sources.iter().skip(1) {
            decoder2.add_symbol(src);
        }
        // Feed repairs until decoded
        let mut recovered = false;
        for repair in &repairs {
            if let Some(result) = decoder2.add_symbol(repair) {
                assert_eq!(
                    &result[..data.len()],
                    &data[..],
                    "Recovered data must match original"
                );
                recovered = true;
                break;
            }
        }
        assert!(recovered, "Should recover from 1 loss with repair symbols");
    }

    #[test]
    fn test_repair_symbols_cover_all_source_positions() {
        // Every source position 0..k-1 must appear in at least one repair bin's members.
        let k = 50;
        let data = vec![0xCDu8; k * 200];
        let params = make_params(k as u32, 200, 10);
        let encoder = MettleBlockEncoder::new(&data, params);

        let coded = encoder.encoder.coded_packets();
        assert!(!coded.is_empty(), "Should have coded bins");

        for pos in 0..k {
            let covered = coded.iter().any(|cp| cp.members.contains(&pos));
            assert!(
                covered,
                "Source position {} is not covered by any repair bin",
                pos
            );
        }
    }
}
