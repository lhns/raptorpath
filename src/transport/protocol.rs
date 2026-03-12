//! Wire protocol definitions.

use crate::fec::{EncodingParams, WireSymbol};
use serde::{Deserialize, Serialize};

/// A batch of symbols sent over a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolBatch {
    /// Symbols in this batch
    pub symbols: Vec<WireSymbol>,
    /// Sending timestamp (microseconds since epoch)
    pub send_timestamp_us: u64,
    /// Batch sequence number for loss detection
    pub batch_seq: u64,
}

/// Control messages exchanged between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Announce encoding parameters for a new block.
    BlockStart {
        params: EncodingParams,
        transfer_length: u64,
    },

    /// Acknowledge received symbols.
    Ack {
        block_id: u64,
        /// Bitmap of received symbol payload_ids
        received_ids: Vec<u32>,
        /// Receiver timestamp for RTT calculation
        recv_timestamp_us: u64,
    },

    /// Report block decode success/failure.
    BlockResult {
        block_id: u64,
        success: bool,
        symbols_received: u32,
        symbols_needed: u32,
    },

    /// Path quality report (receiver → sender).
    PathReport {
        path_id: u32,
        loss_rate: f64,
        avg_rtt_us: u64,
        throughput_bps: f64,
    },

    /// Request more repair symbols for a block.
    RepairRequest {
        block_id: u64,
        additional_count: u32,
    },

    /// Keepalive / path probe.
    Ping { timestamp_us: u64 },
    Pong { echo_timestamp_us: u64 },
}

/// Top-level wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessage {
    Data(SymbolBatch),
    Control(ControlMessage),
}

impl WireMessage {
    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).expect("serialization should not fail")
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}
