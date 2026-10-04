//! Wire-level FEC types shared by every layer: the symbol, the backend tag
//! it carries, and the (legacy) block parameters.
//!
//! The codec interface is [`super::WindowEncoder`]/[`super::WindowDecoder`];
//! the block-codec traits went with the block pipeline (ADR-0069).

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Parameters of a block-pipeline FEC block. The block pipeline is gone
/// (ADR-0069); the type survives only as the payload of the wire-reserved
/// `ControlMessage::BlockStart`, whose encoding (and so every later
/// variant's index) must not move.
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
    /// The payload. `Bytes` so one source buffer is shared — not copied — by
    /// the encoder window, the sender's retention store and the wire send.
    /// Serialized exactly as the `Vec<u8>` it replaced (see
    /// [`bytes_as_vec`]), so every wire / bincode byte is unchanged.
    #[serde(with = "bytes_as_vec")]
    pub data: Bytes,
    /// Which FEC backend produced this symbol. Decoders reject mismatched backends.
    pub backend: FecBackend,
}

/// Which FEC backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FecBackend {
    /// RaptorQ — block-only; its codec was removed with the block pipeline
    /// (ADR-0069). The variant is kept so the wire tag stays stable
    /// (`transport::protocol` code 0) and a library `PeerConfig` naming it
    /// fails at startup with a clear error.
    RaptorQ,
    /// Reed-Solomon — block-only; removed with the block pipeline
    /// (ADR-0069). Kept for the same reasons as `RaptorQ` (wire code 2).
    ReedSolomon,
    /// Random Linear Code (RFC 8681) — GF(2^8) random combinations, ~0%
    /// overhead, truly rateless, near-MDS. The one pipeline's codec.
    Rlc,
    // Variant 4 (`Streaming`, the Badr/Martinian two-layer code) was removed
    // (paper §10); it was the last variant, so the surviving wire indices are
    // unchanged.
}

impl Default for FecBackend {
    fn default() -> Self {
        Self::Rlc
    }
}

impl FecBackend {
    /// Whether this backend's algorithm is streaming-native (operates over a
    /// sliding window) vs block-only (requires all k sources upfront).
    /// Only a streaming-native backend can run: the block pipeline is gone
    /// (ADR-0069, `net::pipeline_backend`).
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
}

/// Serde for [`WireSymbol::data`]: encoded EXACTLY as a `Vec<u8>` is (a
/// sequence of `u8`, i.e. `collect_seq` — what `impl Serialize for Vec<u8>`
/// does), decoded through `Vec<u8>`'s own `Deserialize`. Not `bytes`' serde
/// feature, whose `serialize_bytes` is a different serde call: identical
/// under bincode 1 today, but this keeps the format byte-identical by
/// construction for any serializer.
mod bytes_as_vec {
    use bytes::Bytes;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &Bytes, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(b.iter())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Bytes, D::Error> {
        Vec::<u8>::deserialize(d).map(Bytes::from)
    }
}
