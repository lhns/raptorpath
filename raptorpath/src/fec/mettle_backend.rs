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
            })
            .collect()
    }

    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        let coded = self.encoder.coded_packets();
        let num_source = self.encoder.num_source();

        // Return up to `count` coded packets as repair symbols
        coded
            .into_iter()
            .take(count as usize)
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
}

impl MettleBlockDecoder {
    pub fn new(params: EncodingParams, transfer_length: u64) -> Self {
        let config = MettleConfig::small_window();
        let seed = params.block_id;
        let num_source = params.source_symbols as usize;

        Self {
            params,
            decoder: MettleDecoder::new(config, num_source, seed),
            decoded: false,
            result: None,
            total_fed: 0,
            seen_ids: HashSet::new(),
            created: Instant::now(),
            transfer_length,
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
                return None; // malformed
            }
            let bin_index = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            let num_members = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
            let members_end = 8 + num_members * 4;
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
