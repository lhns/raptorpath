//! Wire protocol definitions.

use crate::fec::{EncodingParams, WireSymbol};
use serde::{Deserialize, Serialize};

/// Protocol version. Increment on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;
/// Magic bytes for wire format identification.
pub const WIRE_MAGIC: [u8; 4] = *b"RPTQ";

/// Handshake message exchanged on connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub version: u32,
    pub max_block_size: u32,
    pub symbol_size: u16,
    pub path_id: u32,
}

impl Handshake {
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&WIRE_MAGIC);
        data.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        data.extend(bincode::serialize(self).expect("handshake serialization should not fail"));
        data
    }

    pub fn deserialize(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 8 {
            anyhow::bail!("handshake too short");
        }
        if &data[..4] != &WIRE_MAGIC {
            anyhow::bail!("invalid handshake magic");
        }
        let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if version != PROTOCOL_VERSION {
            anyhow::bail!("protocol version mismatch: expected {PROTOCOL_VERSION}, got {version}");
        }
        Ok(bincode::deserialize(&data[8..])?)
    }
}

/// A batch of symbols sent over a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolBatch {
    /// Symbols in this batch
    pub symbols: Vec<WireSymbol>,
    /// Sending timestamp (microseconds since connection epoch) — sender's clock
    pub send_timestamp_us: u64,
    /// Batch sequence number for loss detection (per-path monotonic)
    pub batch_seq: u64,
    /// Total symbols sent in this block on this path (for receiver loss tracking)
    pub path_id: u32,
}

/// Control messages exchanged between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Announce encoding parameters for a new block (sender → receiver).
    BlockStart {
        params: EncodingParams,
        transfer_length: u64,
    },

    /// Acknowledge received symbols (receiver → sender).
    Ack {
        block_id: u64,
        /// Which payload_ids were received
        received_ids: Vec<u32>,
        /// Echo the sender's timestamp for RTT calculation (sender's clock, not receiver's)
        echo_send_timestamp_us: u64,
        /// How many symbols the receiver expected (from batch_seq gaps)
        expected_count: u32,
        /// How many symbols actually received
        received_count: u32,
    },

    /// Report block decode success/failure (receiver → sender).
    BlockResult {
        block_id: u64,
        success: bool,
        symbols_received: u32,
        symbols_needed: u32,
    },

    /// Path quality report (RTCP-style, bidirectional).
    PathReport {
        path_id: u32,
        loss_rate: f64,
        avg_rtt_us: u64,
        throughput_bps: f64,
        /// Interarrival jitter in microseconds (RFC 3550 A.8)
        jitter_us: u64,
        /// Cumulative symbols sent on this path
        symbols_sent: u64,
        /// Cumulative symbols received on this path
        symbols_received: u64,
    },

    /// Request more repair symbols for a block.
    RepairRequest {
        block_id: u64,
        additional_count: u32,
    },

    /// Keepalive / path probe.
    Ping { timestamp_us: u64 },
    Pong { echo_timestamp_us: u64 },

    /// Graceful shutdown notification.
    Shutdown,

    /// Notify peer that a new path is being added (connection migration).
    PathAdd {
        path_id: u32,
        bind_addr: String,
    },

    /// Notify peer that a path is being removed (connection migration).
    PathRemove {
        path_id: u32,
    },
}

/// Top-level wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessage {
    Data(SymbolBatch),
    Control(ControlMessage),
}

impl WireMessage {
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&WIRE_MAGIC);
        data.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        data.extend(bincode::serialize(self).expect("serialization should not fail"));
        data
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::Error> {
        if data.len() < 8 {
            return Err(Box::new(bincode::ErrorKind::Custom(
                "message too short for header".into(),
            )));
        }
        if &data[..4] != &WIRE_MAGIC {
            return Err(Box::new(bincode::ErrorKind::Custom(
                "invalid magic bytes — not a raptorpath message".into(),
            )));
        }
        let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if version != PROTOCOL_VERSION {
            return Err(Box::new(bincode::ErrorKind::Custom(
                format!("protocol version mismatch: expected {PROTOCOL_VERSION}, got {version}"),
            )));
        }
        bincode::deserialize(&data[8..])
    }
}
