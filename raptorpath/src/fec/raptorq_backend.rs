//! RaptorQ FEC backend.
//!
//! Wraps the `raptorq` crate to implement the FecEncoder/FecDecoder traits.
//! Latency-optimized strategy:
//! 1. Emit source symbols first (passthrough — zero encoding latency)
//! 2. Generate repair symbols as a stream (fountain property)
//! 3. Receiver processes source symbols immediately, uses decoder only on loss

use bytes::Bytes;
use raptorq::{
    Decoder as RqDecoder, Encoder as RqEncoder, EncodingPacket, ObjectTransmissionInformation,
    SourceBlockEncoder,
};
use std::collections::HashSet;
use std::time::Instant;

use super::traits::{EncodingParams, FecDecoder, FecEncoder, WireSymbol};

/// RaptorQ encoder: takes a block of data and produces source + repair symbols.
pub struct RaptorqEncoder {
    params: EncodingParams,
    rq_encoder: RqEncoder,
}

impl RaptorqEncoder {
    pub fn new(data: &[u8], params: EncodingParams) -> Self {
        let oti = ObjectTransmissionInformation::with_defaults(
            data.len() as u64,
            params.symbol_size,
        );
        let rq_encoder = RqEncoder::new(data, oti);
        Self { params, rq_encoder }
    }
}

impl FecEncoder for RaptorqEncoder {
    fn source_symbols(&self) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        self.rq_encoder
            .get_block_encoders()
            .iter()
            .flat_map(|block: &SourceBlockEncoder| {
                block
                    .source_packets()
                    .into_iter()
                    .map(move |pkt: EncodingPacket| {
                        let serialized = pkt.serialize();
                        let payload_id = u32::from_be_bytes(
                            serialized[..4].try_into().unwrap(),
                        );
                        WireSymbol {
                            block_id,
                            payload_id,
                            is_repair: false,
                            data: serialized[4..].to_vec(),
                            backend: super::traits::FecBackend::RaptorQ,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol> {
        self.repair_symbols_from(0, count)
    }

    fn repair_symbols_from(&self, start: u32, count: u32) -> Vec<WireSymbol> {
        let block_id = self.params.block_id;
        self.rq_encoder
            .get_block_encoders()
            .iter()
            .flat_map(move |block: &SourceBlockEncoder| {
                block
                    .repair_packets(start, count)
                    .into_iter()
                    .map(move |pkt: EncodingPacket| {
                        let serialized = pkt.serialize();
                        let payload_id = u32::from_be_bytes(
                            serialized[..4].try_into().unwrap(),
                        );
                        WireSymbol {
                            block_id,
                            payload_id,
                            is_repair: true,
                            data: serialized[4..].to_vec(),
                            backend: super::traits::FecBackend::RaptorQ,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// RaptorQ decoder: reassembles a block from received symbols.
///
/// Key optimization: source symbols that arrive intact can be used directly
/// without waiting for full block decoding. Only when there are losses do we
/// need to invoke the fountain decoder.
pub struct RaptorqDecoder {
    params: EncodingParams,
    rq_decoder: RqDecoder,
    /// Track which source symbols we've received directly
    received_source: Vec<Option<Vec<u8>>>,
    /// Total source symbols received
    source_count: u32,
    /// Total symbols (source + repair) fed to decoder
    total_fed: u32,
    /// Whether decoding has completed
    decoded: bool,
    /// The decoded result, if available
    result: Option<Bytes>,
    /// Deduplication: track seen payload_ids
    seen_ids: HashSet<u32>,
    /// When this decoder was created (for timeout eviction)
    created: Instant,
}

impl RaptorqDecoder {
    pub fn new(params: EncodingParams, transfer_length: u64) -> Self {
        let oti = ObjectTransmissionInformation::with_defaults(
            transfer_length,
            params.symbol_size,
        );
        let rq_decoder = RqDecoder::new(oti);
        let received_source = vec![None; params.source_symbols as usize];
        Self {
            params,
            rq_decoder,
            received_source,
            source_count: 0,
            total_fed: 0,
            decoded: false,
            result: None,
            seen_ids: HashSet::new(),
            created: Instant::now(),
        }
    }
}

impl FecDecoder for RaptorqDecoder {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes> {
        if self.decoded {
            return self.result.clone();
        }

        // Reject symbols from a different backend
        if symbol.backend != super::traits::FecBackend::RaptorQ {
            return None;
        }

        // Deduplicate
        if !self.seen_ids.insert(symbol.payload_id) {
            return None;
        }

        // Track source symbols for direct passthrough
        if !symbol.is_repair {
            let idx = symbol.payload_id as usize;
            if idx < self.received_source.len() && self.received_source[idx].is_none() {
                self.received_source[idx] = Some(symbol.data.clone());
                self.source_count += 1;
            }
        }

        // Build the raptorq EncodingPacket
        let mut serialized = symbol.payload_id.to_be_bytes().to_vec();
        serialized.extend_from_slice(&symbol.data);
        let pkt = EncodingPacket::deserialize(&serialized);
        self.total_fed += 1;

        // Try to decode
        if let Some(data) = self.rq_decoder.decode(pkt) {
            self.decoded = true;
            self.result = Some(Bytes::from(data));
            return self.result.clone();
        }

        // If all source symbols arrived, we can reconstruct directly
        if self.params.source_symbols > 0
            && self.source_count == self.params.source_symbols
        {
            let data: Vec<u8> = self
                .received_source
                .iter()
                .filter_map(|s| s.as_ref())
                .flat_map(|s| s.iter().copied())
                .collect();
            self.decoded = true;
            self.result = Some(Bytes::from(data));
            return self.result.clone();
        }

        None
    }

    fn is_complete_source(&self) -> bool {
        self.params.source_symbols > 0 && self.source_count == self.params.source_symbols
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
        self.received_source.get(index)?.as_deref()
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
            block_id: 0,
        }
    }

    #[test]
    fn test_source_symbol_fast_path() {
        // All k source symbols arrive → is_complete_source() true, decode via fast-path
        let data = vec![42u8; 4800]; // 4 symbols of 1200 bytes
        let params = make_params(4, 1200, 2);
        let encoder = RaptorqEncoder::new(&data, params);
        let sources = encoder.source_symbols();

        let mut decoder = RaptorqDecoder::new(params, data.len() as u64);
        for src in &sources {
            let result = decoder.add_symbol(src);
            if decoder.is_complete_source() {
                assert!(result.is_some());
                assert_eq!(&result.unwrap()[..data.len()], &data[..]);
                break;
            }
        }
        assert!(decoder.is_complete_source() || decoder.is_decoded());
    }

    #[test]
    fn test_backend_mismatch_rejected() {
        let data = vec![1u8; 1200];
        let params = make_params(1, 1200, 1);
        let mut decoder = RaptorqDecoder::new(params, data.len() as u64);

        let wrong_sym = WireSymbol {
            block_id: 0,
            payload_id: 0,
            is_repair: false,
            data: data.clone(),
            backend: FecBackend::ReedSolomon,
        };
        assert!(decoder.add_symbol(&wrong_sym).is_none());
        assert_eq!(decoder.total_fed(), 0);
    }

    #[test]
    fn test_dedup_same_payload_id() {
        let data = vec![7u8; 2400];
        let params = make_params(2, 1200, 1);
        let encoder = RaptorqEncoder::new(&data, params);
        let sources = encoder.source_symbols();

        let mut decoder = RaptorqDecoder::new(params, data.len() as u64);
        decoder.add_symbol(&sources[0]);
        assert_eq!(decoder.total_fed(), 1);

        // Feed same symbol again
        let result = decoder.add_symbol(&sources[0]);
        assert!(result.is_none());
        assert_eq!(decoder.total_fed(), 1); // not incremented
    }

    #[test]
    fn test_k1_single_symbol() {
        // k=1 edge case: encode/decode round-trip
        let data = vec![99u8; 1200];
        let params = make_params(1, 1200, 0);
        let encoder = RaptorqEncoder::new(&data, params);
        let sources = encoder.source_symbols();

        let mut decoder = RaptorqDecoder::new(params, data.len() as u64);
        let result = decoder.add_symbol(&sources[0]);
        assert!(result.is_some());
        assert_eq!(&result.unwrap()[..data.len()], &data[..]);
    }

    #[test]
    fn test_repair_recovery() {
        // Drop 1 source, feed repair → decode succeeds
        let data = vec![55u8; 3600]; // 3 symbols of 1200
        let params = make_params(3, 1200, 5);
        let encoder = RaptorqEncoder::new(&data, params);
        let sources = encoder.source_symbols();
        let repairs = encoder.repair_symbols(5);

        let mut decoder = RaptorqDecoder::new(params, data.len() as u64);
        // Feed sources 0 and 2, skip 1
        decoder.add_symbol(&sources[0]);
        decoder.add_symbol(&sources[2]);

        // Feed repairs until decode
        let mut decoded = false;
        for repair in &repairs {
            if let Some(result) = decoder.add_symbol(repair) {
                assert_eq!(&result[..data.len()], &data[..]);
                decoded = true;
                break;
            }
        }
        assert!(decoded, "Should decode with repair symbols");
    }
}
