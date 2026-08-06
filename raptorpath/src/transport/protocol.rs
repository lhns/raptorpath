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
/// v5: compact DATA framing exists (goal-gate "Window Decoupling + MTU
/// Scaling" part 2, `RWM_WIRE_COMPACT`): a one-symbol SymbolBatch may ride
/// a tag-byte + varint frame whose payload runs to the datagram boundary
/// (~14–16 B vs the 65-B magic+bincode framing — the measured ~4.3 Mbit
/// framing tax at c2). RECEIVE support is unconditional in v5 (the tag
/// byte 0xC1 is dead space under the legacy 'R' magic), sending is
/// env-gated; the version bump makes pre-compact binaries refuse cleanly
/// at handshake instead of dropping datagrams mid-stream if the gate is
/// ever flipped on.
pub const PROTOCOL_VERSION: u32 = 5;
/// Magic bytes for wire format identification.
pub const WIRE_MAGIC: [u8; 4] = *b"RPTQ";

/// Compact DATA frame tag (v5). MUST stay distinct from the legacy magic's
/// first byte b'R' (0x52) — the receive path classifies on byte 0.
pub const COMPACT_DATA_TAG: u8 = 0xC1;

/// `RWM_WIRE_COMPACT` (default OFF — the A/B arm): sender-side compact DATA
/// framing. Resolved once per process (transport-layer knob, like
/// `RWM_MTU_FLOOR`/`RWM_QUIC_CC`).
pub fn wire_compact_active() -> bool {
    static ACTIVE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ACTIVE.get_or_init(|| crate::config::env_flag("RWM_WIRE_COMPACT", false))
}

// ── LEB128 varints for the compact frame ────────────────────────────────
fn write_varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(b);
            return;
        }
        buf.push(b | 0x80);
    }
}

fn read_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *data.get(*pos)?;
        *pos += 1;
        if shift >= 64 {
            return None; // overflow — malformed
        }
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
}

fn backend_to_u8(b: FecBackend) -> u8 {
    match b {
        FecBackend::RaptorQ => 0,
        FecBackend::Mettle => 1,
        FecBackend::ReedSolomon => 2,
        FecBackend::Rlc => 3,
    }
}

fn backend_from_u8(v: u8) -> Option<FecBackend> {
    Some(match v {
        0 => FecBackend::RaptorQ,
        1 => FecBackend::Mettle,
        2 => FecBackend::ReedSolomon,
        3 => FecBackend::Rlc,
        _ => return None,
    })
}

/// Serialize a ONE-symbol batch as a compact DATA frame (v5,
/// `RWM_WIRE_COMPACT`). Returns None for multi-symbol batches (block-mode
/// drains keep legacy framing). Layout:
///
///   [tag 0xC1][flags: bit0 = is_repair, bits1-2 = backend]
///   [varint path_id][varint block_id][varint payload_id]
///   [varint send_timestamp_us][varint batch_seq][payload … to end]
///
/// The payload length is the DATAGRAM boundary — both 8-byte bincode
/// length fields (Vec len + data len), the 4-byte enum tags, and the 8-byte
/// magic+version header are gone: ~14–16 B total vs 65.
pub fn serialize_data_compact(batch: &SymbolBatch) -> Option<Vec<u8>> {
    if batch.symbols.len() != 1 {
        return None;
    }
    let sym = &batch.symbols[0];
    let mut buf = Vec::with_capacity(24 + sym.data.len());
    buf.push(COMPACT_DATA_TAG);
    buf.push((sym.is_repair as u8) | (backend_to_u8(sym.backend) << 1));
    write_varint(&mut buf, batch.path_id as u64);
    write_varint(&mut buf, sym.block_id);
    write_varint(&mut buf, sym.payload_id as u64);
    write_varint(&mut buf, batch.send_timestamp_us);
    write_varint(&mut buf, batch.batch_seq);
    buf.extend_from_slice(&sym.data);
    Some(buf)
}

fn parse_data_compact(data: &[u8]) -> Result<WireMessage, bincode::Error> {
    let err = |m: &str| Box::new(bincode::ErrorKind::Custom(m.into()));
    if data.len() < 8 || data[0] != COMPACT_DATA_TAG {
        return Err(err("not a compact data frame"));
    }
    let flags = data[1];
    let is_repair = flags & 1 != 0;
    let backend =
        backend_from_u8((flags >> 1) & 0x7).ok_or_else(|| err("bad compact backend"))?;
    let mut pos = 2usize;
    let path_id = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    let block_id = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    let payload_id = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    let send_ts = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    let batch_seq = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    if path_id > u32::MAX as u64 || payload_id > u32::MAX as u64 {
        return Err(err("compact field overflow"));
    }
    Ok(WireMessage::Data(SymbolBatch {
        symbols: vec![WireSymbol {
            block_id,
            payload_id: payload_id as u32,
            is_repair,
            data: data[pos..].to_vec(),
            backend,
        }],
        send_timestamp_us: send_ts,
        batch_seq,
        path_id: path_id as u32,
    }))
}

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
        // v5 compact DATA frame: classified on byte 0 (0xC1 is dead space
        // under the legacy 'R' magic — unconditional receive support,
        // byte-identical for all legacy traffic).
        if data.first() == Some(&COMPACT_DATA_TAG) {
            return parse_data_compact(data);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(
        block_id: u64,
        payload_id: u32,
        is_repair: bool,
        backend: FecBackend,
        n: usize,
    ) -> WireSymbol {
        WireSymbol {
            block_id,
            payload_id,
            is_repair,
            data: (0..n).map(|i| (i % 251) as u8).collect(),
            backend,
        }
    }

    /// feat/window-mtu part 2: compact frame round-trips bit-exactly into
    /// the SAME SymbolBatch the legacy path would deliver — for source and
    /// repair symbols, every backend, and boundary field values.
    #[test]
    fn compact_data_frame_roundtrip_is_exact() {
        for (backend, is_repair) in [
            (FecBackend::Rlc, false),
            (FecBackend::Rlc, true),
            (FecBackend::RaptorQ, false),
            (FecBackend::Mettle, true),
            (FecBackend::ReedSolomon, false),
        ] {
            let batch = SymbolBatch {
                symbols: vec![sym(u64::MAX / 3, u32::MAX, is_repair, backend, 1200)],
                send_timestamp_us: 123_456_789_012,
                batch_seq: 987_654,
                path_id: 7,
            };
            let buf = serialize_data_compact(&batch).expect("one-symbol batch");
            assert_eq!(buf[0], COMPACT_DATA_TAG);
            let msg = WireMessage::deserialize(&buf).expect("compact parse");
            match msg {
                WireMessage::Data(b) => {
                    assert_eq!(b.path_id, batch.path_id);
                    assert_eq!(b.batch_seq, batch.batch_seq);
                    assert_eq!(b.send_timestamp_us, batch.send_timestamp_us);
                    assert_eq!(b.symbols.len(), 1);
                    let (a, e) = (&b.symbols[0], &batch.symbols[0]);
                    assert_eq!(a.block_id, e.block_id);
                    assert_eq!(a.payload_id, e.payload_id);
                    assert_eq!(a.is_repair, e.is_repair);
                    assert_eq!(a.backend, e.backend);
                    assert_eq!(a.data, e.data);
                }
                _ => panic!("compact frame must parse as Data"),
            }
        }
    }

    /// The derivation's overhead claim, held as a law: compact framing for
    /// a typical mid-transfer symbol is <= 24 B (vs 65 legacy) and the tag
    /// never collides with the legacy magic's first byte.
    #[test]
    fn compact_frame_overhead_is_bounded_and_tag_disjoint() {
        assert_ne!(COMPACT_DATA_TAG, WIRE_MAGIC[0]);
        let batch = SymbolBatch {
            symbols: vec![sym(50_000, 0, false, FecBackend::Rlc, 1200)],
            send_timestamp_us: 30_000_000_000, // ~8.3 h session, worst plausible
            batch_seq: 100_000,
            path_id: 1,
        };
        let buf = serialize_data_compact(&batch).unwrap();
        let overhead = buf.len() - 1200;
        assert!(
            overhead <= 24,
            "compact overhead {overhead} B exceeds the derivation bound"
        );
        // Legacy framing for the SAME batch (magic+version+bincode).
        let legacy = WireMessage::Data(batch).serialize().unwrap();
        assert!(
            legacy.len() - 1200 >= 60,
            "legacy framing measured {} B — the derivation's 65-B claim moved",
            legacy.len() - 1200
        );
    }

    /// Multi-symbol batches (block-mode drains) refuse compact framing —
    /// scope is the window-mode one-symbol datagram path.
    #[test]
    fn compact_refuses_multi_symbol_batches() {
        let batch = SymbolBatch {
            symbols: vec![
                sym(1, 0, false, FecBackend::Rlc, 100),
                sym(2, 1, false, FecBackend::Rlc, 100),
            ],
            send_timestamp_us: 1,
            batch_seq: 1,
            path_id: 0,
        };
        assert!(serialize_data_compact(&batch).is_none());
    }

    /// Legacy frames still parse unchanged (byte-identical receive path for
    /// all legacy traffic), and malformed/truncated compact frames error
    /// instead of panicking.
    #[test]
    fn legacy_parse_unchanged_and_compact_truncation_safe() {
        let batch = SymbolBatch {
            symbols: vec![sym(9, 3, true, FecBackend::Rlc, 64)],
            send_timestamp_us: 42,
            batch_seq: 7,
            path_id: 2,
        };
        let legacy = WireMessage::Data(batch).serialize().unwrap();
        assert_eq!(&legacy[..4], &WIRE_MAGIC);
        assert!(WireMessage::deserialize(&legacy).is_ok());
        // Truncations of a compact frame must all error cleanly.
        let full = serialize_data_compact(&SymbolBatch {
            symbols: vec![sym(1000, 1, false, FecBackend::Rlc, 32)],
            send_timestamp_us: 5_000_000,
            batch_seq: 3,
            path_id: 0,
        })
        .unwrap();
        // QUIC datagrams deliver atomically, so truncation is a hostile-
        // input concern: the sub-header region must error cleanly (beyond
        // it a shorter buffer is a legal shorter payload).
        for cut in 0..8 {
            assert!(
                WireMessage::deserialize(&full[..cut]).is_err(),
                "truncated compact frame (len {cut}) must not parse"
            );
        }
        // A bad backend index errors.
        let mut bad = full.clone();
        bad[1] = 0x7 << 1;
        assert!(WireMessage::deserialize(&bad).is_err());
    }

    /// Varint round-trip across the value spectrum.
    #[test]
    fn varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, 16_383, 16_384, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_varint(&buf, &mut pos), Some(v));
            assert_eq!(pos, buf.len());
        }
        // Truncated varint reads None, never panics.
        let mut buf = Vec::new();
        write_varint(&mut buf, u64::MAX);
        let mut pos = 0;
        assert_eq!(read_varint(&buf[..buf.len() - 1], &mut pos), None);
    }
}
