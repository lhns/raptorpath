//! Wire protocol definitions.

use crate::fec::{EncodingParams, FecBackend, WireSymbol};
use bincode::Options;
use serde::{Deserialize, Serialize};

/// Maximum serialized message size (2 MB). Prevents OOM from crafted length fields.
const MAX_MESSAGE_SIZE: u64 = 2 * 1024 * 1024;

/// Maximum number of symbols allowed in a single batch.
const MAX_SYMBOLS_PER_BATCH: usize = 1_000;

/// Maximum number of ACK IDs in a single Ack message.
const MAX_ACK_IDS: usize = 2_000;

/// Create a size-limited bincode deserializer.
fn bincode_options() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_MESSAGE_SIZE)
}

/// Protocol version. Increment on breaking changes.
/// v4: `Ack` carries `batch_seq` (block-mode ARQ, P8) — the acked batch is
/// identified exactly instead of by the first symbol's block_id. Required
/// because `send_timestamp_us` is shared by every chunk of one drain call
/// and a batch may mix symbols from several blocks, so neither field keys
/// the sender-side batch ledger unambiguously.
pub const PROTOCOL_VERSION: u32 = 4;
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
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        let mut data = Vec::new();
        data.extend_from_slice(&WIRE_MAGIC);
        data.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        data.extend(bincode::serialize(self)?);
        Ok(data)
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
        Ok(bincode_options().deserialize(&data[8..])?)
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
        backend: FecBackend,
    },

    /// Acknowledge received symbols (receiver → sender).
    Ack {
        block_id: u64,
        /// The `batch_seq` of the SymbolBatch this Ack covers (v4, P8).
        /// Keys the sender's per-batch ledger: sent-vs-received diff tells
        /// the sender exactly which symbols died, one RTT after sending.
        batch_seq: u64,
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

    /// Announce that the sender is entering sliding-window FEC mode.
    WindowStart {
        symbol_size: u16,
        backend: FecBackend,
        /// Whether the sender packs multiple small packets into each symbol.
        /// When true, the receiver must use `extract_packets()` (block-mode framing)
        /// instead of `extract_window_packet()` to recover individual packets.
        packed: bool,
    },

    /// Acknowledge received window-mode symbols with SACK (receiver → sender).
    ///
    /// Replaces the former WindowNack mechanism. Per-packet ACK with selective
    /// acknowledgment ranges, RTT echo, jitter, and cumulative received count.
    /// See paper Section 6.2 (SACK-Extended ACK).
    WindowAck {
        /// All sequences up to this have been received or recovered.
        received_up_to: u64,
        /// Selective ACK: out-of-order ranges received beyond cumulative point.
        /// Cumulative within T_cut window — all received fragments reported.
        sack_ranges: Vec<(u64, u64)>,
        /// Echo the sender's timestamp for RTT measurement (sender's clock).
        echo_send_timestamp_us: u64,
        /// Interarrival jitter in microseconds (RFC 3550 A.8).
        jitter_us: u32,
        /// Running total of symbols received (self-healing reliability metric).
        cumulative_received: u64,
    },

    /// DEPRECATED: WindowNack replaced by SACK-extended WindowAck.
    /// Kept for wire compatibility during transition.
    WindowNack {
        gaps: Vec<(u64, u64)>,
    },

    /// Sender signals backend switch at a window flush point (sender → receiver).
    WindowSwitch {
        /// Last source sequence number under the old backend.
        flush_seq: u64,
        /// The new FEC backend to switch to.
        new_backend: FecBackend,
        /// Symbol size for the new backend.
        symbol_size: u16,
    },

    /// Receiver confirms it drained up to flush_seq and is ready (receiver → sender).
    WindowSwitchAck {
        flush_seq: u64,
    },

    /// DEPRECATED: NackAck is no longer used. SACK-extended WindowAck replaces
    /// the NACK mechanism. Kept for wire compatibility during transition.
    NackAck {
        nack_id: u32,
    },

    /// Per-generation deficit feedback (receiver → sender, generation coding
    /// mode; paper §16.3). For each in-flight / frontier generation the receiver
    /// still needs, carries `(anchor, deficit)` where `anchor` is the
    /// generation's stable coding anchor (= `window_start`, a multiple of the
    /// generation size) and `deficit = K_g − rank_g` is how many MORE independent
    /// coded symbols that generation needs to decode. This closes the rateless-
    /// with-feedback loop: the sender emits exactly the residual deficit for each
    /// generation (bounding recovery — no bursty flood) while a stalled frontier
    /// generation keeps a nonzero deficit until it decodes (funding it), which
    /// the feedback-free cumulative-ack proxy could not do simultaneously.
    GenerationDeficit {
        /// `(generation_anchor, residual_deficit)` for the frontier generations.
        deficits: Vec<(u64, u32)>,
    },
}

/// Top-level wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessage {
    Data(SymbolBatch),
    Control(ControlMessage),
}

impl WireMessage {
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        let mut data = Vec::new();
        data.extend_from_slice(&WIRE_MAGIC);
        data.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        data.extend(bincode::serialize(self)?);
        Ok(data)
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
        let msg: Self = bincode_options().deserialize(&data[8..])?;

        // Post-deserialization validation: reject oversized collections
        match &msg {
            WireMessage::Data(batch) => {
                if batch.symbols.len() > MAX_SYMBOLS_PER_BATCH {
                    return Err(Box::new(bincode::ErrorKind::Custom(format!(
                        "symbol batch too large: {} > {}",
                        batch.symbols.len(),
                        MAX_SYMBOLS_PER_BATCH
                    ))));
                }
            }
            WireMessage::Control(ControlMessage::Ack { received_ids, .. }) => {
                if received_ids.len() > MAX_ACK_IDS {
                    return Err(Box::new(bincode::ErrorKind::Custom(format!(
                        "ack received_ids too large: {} > {}",
                        received_ids.len(),
                        MAX_ACK_IDS
                    ))));
                }
            }
            WireMessage::Control(ControlMessage::GenerationDeficit { deficits }) => {
                // Only the M frontier generations are ever reported (M ~ 2–4);
                // reject anything absurd as a malformed/hostile message.
                if deficits.len() > MAX_ACK_IDS {
                    return Err(Box::new(bincode::ErrorKind::Custom(format!(
                        "generation deficits too large: {} > {}",
                        deficits.len(),
                        MAX_ACK_IDS
                    ))));
                }
            }
            _ => {}
        }

        Ok(msg)
    }
}
