//! Codec-agnostic FEC traits.
//!
//! These traits define the interface that any FEC backend must implement.
//! The rest of raptorpath uses these traits, making the FEC layer swappable
//! between RaptorQ, METTLE, or any future erasure code.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Parameters for a single FEC block (codec-agnostic).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EncodingParams {
    /// Number of source symbols (k)
    pub source_symbols: u32,
    /// Symbol size in bytes (T) — should align with path MTU
    pub symbol_size: u16,
    /// Number of repair symbols to generate for this block
    pub repair_count: u32,
    /// Block sequence number
    pub block_id: u64,
}

/// Symbol sent over the wire (codec-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSymbol {
    pub block_id: u64,
    pub payload_id: u32,
    pub is_repair: bool,
    pub data: Vec<u8>,
}

/// Trait for FEC block encoders.
pub trait FecEncoder: Send {
    /// Get source symbols — the original data, zero encoding cost.
    fn source_symbols(&self) -> Vec<WireSymbol>;
    /// Generate repair symbols. Can be called multiple times for fountain codes.
    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol>;
}

/// Trait for FEC block decoders.
pub trait FecDecoder: Send + Sync {
    /// Feed a received symbol. Returns `Some(data)` when the block is fully decoded.
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes>;
    /// Whether all source symbols arrived intact (fast path).
    fn is_complete_source(&self) -> bool;
    /// Whether decoding has completed (by any means).
    fn is_decoded(&self) -> bool;
    /// Total symbols fed to this decoder.
    fn total_fed(&self) -> u32;
    /// The encoding params for this block.
    fn params(&self) -> &EncodingParams;
    /// Get an individual source symbol by index.
    fn get_source_symbol(&self, index: usize) -> Option<&[u8]>;
    /// Get all received payload_ids (for ACKs).
    fn received_ids(&self) -> Vec<u32>;
    /// When this decoder was created (for timeout eviction).
    fn created_at(&self) -> Instant;
}

/// Which FEC backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FecBackend {
    /// RaptorQ (RFC 6330) — rateless fountain code with near-optimal erasure recovery.
    RaptorQ,
    /// METTLE — streaming erasure code with pure peeling decoder (research implementation).
    Mettle,
}

impl Default for FecBackend {
    fn default() -> Self {
        Self::RaptorQ
    }
}

impl FecBackend {
    /// Create an encoder for the given data and parameters.
    pub fn create_encoder(&self, data: &[u8], params: EncodingParams) -> Box<dyn FecEncoder> {
        match self {
            Self::RaptorQ => Box::new(super::raptorq_backend::RaptorqEncoder::new(data, params)),
            Self::Mettle => Box::new(super::mettle_backend::MettleBlockEncoder::new(data, params)),
        }
    }

    /// Create a decoder for a block with the given parameters.
    pub fn create_decoder(
        &self,
        params: EncodingParams,
        transfer_length: u64,
    ) -> Box<dyn FecDecoder> {
        match self {
            Self::RaptorQ => {
                Box::new(super::raptorq_backend::RaptorqDecoder::new(params, transfer_length))
            }
            Self::Mettle => {
                Box::new(super::mettle_backend::MettleBlockDecoder::new(params, transfer_length))
            }
        }
    }
}
