//! Codec-agnostic FEC traits.
//!
//! These traits define the interface that any FEC backend must implement.
//! The rest of raptorpath uses these traits, making the FEC layer swappable
//! between RaptorQ, RLC, or any future erasure code.

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
    /// Which FEC backend produced this symbol. Decoders reject mismatched backends.
    pub backend: FecBackend,
}

/// Trait for FEC block encoders.
pub trait FecEncoder: Send {
    /// Get source symbols — the original data, zero encoding cost.
    fn source_symbols(&self) -> Vec<WireSymbol>;
    /// Generate repair symbols. Can be called multiple times for fountain codes.
    fn repair_symbols(&self, count: u32) -> Vec<WireSymbol>;
    /// Maximum number of useful repair symbols this encoder can produce.
    /// Rateless codes (RaptorQ, RLC) return `u32::MAX`; fixed-rate codes
    /// return the actual coded symbol count.
    fn max_repairs(&self) -> u32 { u32::MAX }
    /// Generate `count` repair symbols starting at repair index `start`.
    ///
    /// Used by block-mode ARQ (P8) to mint FRESH repairs after loss: the
    /// initial proactive repairs occupy indices `0..repair_count`, so
    /// post-hoc corrections start at `repair_count` and never repeat an
    /// already-sent symbol. The default implementation works for any
    /// encoder whose `repair_symbols(n)` is deterministic and prefix-stable
    /// (all current backends): generate `start + count` and skip the first
    /// `start`. Fixed-rate codes self-clamp — they return fewer (or zero)
    /// symbols when `start + count` exceeds their capacity.
    fn repair_symbols_from(&self, start: u32, count: u32) -> Vec<WireSymbol> {
        self.repair_symbols(start.saturating_add(count))
            .into_iter()
            .skip(start as usize)
            .collect()
    }
}

/// Trait for FEC block decoders.
pub trait FecDecoder: Send + Sync {
    /// Feed a received symbol. Returns `Some(data)` when the block is fully decoded.
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Option<Bytes>;
    /// Whether all source symbols arrived intact (fast path).
    // Test-only consumer (tests/fec_backend_switching_test.rs + the backends'
    // own unit tests); the block path decides on `is_decoded`.
    #[allow(dead_code)]
    fn is_complete_source(&self) -> bool;
    /// Whether decoding has completed (by any means).
    fn is_decoded(&self) -> bool;
    /// Total symbols fed to this decoder.
    fn total_fed(&self) -> u32;
    /// The encoding params for this block.
    fn params(&self) -> &EncodingParams;
    /// When this decoder was created (for timeout eviction).
    fn created_at(&self) -> Instant;
}

/// Which FEC backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FecBackend {
    /// RaptorQ (RFC 6330) — rateless fountain code, LDPC+LT hybrid, ~1% overhead.
    /// Block-mode only. Near-optimal recovery probability. Patent-free.
    RaptorQ,
    /// Reed-Solomon (GF(2^8)) — MDS code, 0% overhead, fixed-rate (max n=255).
    /// Block-mode only. Guaranteed recovery with exactly k of n symbols. Patent-free.
    ReedSolomon,
    /// Random Linear Code (RFC 8681) — GF(2^8) random combinations, ~0% overhead, truly rateless.
    /// Block + window modes. Near-MDS via Gaussian elimination. Patent-free.
    Rlc,
    // `Streaming` (Badr/Martinian two-layer, formerly variant 4) was RETIRED
    // 2026-07-28 — displaced by the unified span machine (ADR-0064), register
    // clause discharged (goal-gate "Streaming Crown Re-Test"). Removing the
    // LAST variant keeps the wire indices of the surviving variants stable.
}

impl Default for FecBackend {
    fn default() -> Self {
        Self::RaptorQ
    }
}

impl FecBackend {
    /// Whether this backend's algorithm is streaming-native (operates over a
    /// sliding window) vs block-only (requires all k sources upfront).
    ///
    /// Streaming-native backends can use the sliding-window FEC pipeline;
    /// block-only backends must use the block-based pipeline.
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::Rlc)
    }

    /// Per-repair-symbol wire overhead in bytes. RaptorQ symbols carry no
    /// extra in-band data; RLC repair symbols carry a repair-index header.
    /// The scheduler should subtract this from MTU when computing symbol size.
    pub fn repair_wire_overhead(&self) -> usize {
        match self {
            // RaptorQ: payload_id is already in WireSymbol, no extra in-band data
            Self::RaptorQ => 0,
            // Reed-Solomon: MDS code, no extra wire data
            Self::ReedSolomon => 0,
            // RLC: [repair_index(4 bytes)] header per repair symbol
            Self::Rlc => 4,
        }
    }

    /// Create an encoder for the given data and parameters.
    pub fn create_encoder(&self, data: &[u8], params: EncodingParams) -> Box<dyn FecEncoder> {
        match self {
            Self::RaptorQ => Box::new(super::raptorq_backend::RaptorqEncoder::new(data, params)),
            Self::ReedSolomon => Box::new(super::rs_backend::ReedSolomonEncoder::new(data, params)),
            Self::Rlc => Box::new(super::rlc_backend::RlcEncoder::new(data, params)),
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
            Self::ReedSolomon => {
                Box::new(super::rs_backend::ReedSolomonDecoder::new(params, transfer_length))
            }
            Self::Rlc => {
                Box::new(super::rlc_backend::RlcDecoder::new(params, transfer_length))
            }
        }
    }
}
