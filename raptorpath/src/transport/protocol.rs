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
/// v6: `WindowAck` carries the receiver's per-path CUMULATIVE
/// `cum_expected`/`cum_received` symbol counters (goal-gate "Unlock The
/// Default 1: ack-merge"). They are the payload of the legacy per-batch
/// `ControlMessage::Ack`, folded onto the SACK ack so window mode can send
/// ONE control datagram per data message instead of two. The fields are
/// unconditional — always present, always populated on a data-triggered ack —
/// so there is exactly one wire format per binary and no gate-dependent
/// framing; `RWM_ACK_MERGE` gates only whether the legacy `Ack` is still
/// ALSO sent. Cumulative (not per-ack) so a dropped control datagram costs
/// nothing: the next ack carries the whole outstanding delta. No new tag is
/// claimed; `COMPACT_DATA_TAG` is untouched.
/// v7: SIX never-sent `ControlMessage` variants removed (refactor arc,
/// "dead code batch 2"): `RepairRequest`, `PathAdd`, `PathRemove`,
/// `WindowNack`, `WindowSwitchAck`, `NackAck`. None had a construction
/// site anywhere in the workspace — their only life was receive-side
/// handlers (a `debug!`/`info!` each, plus `WindowNack`'s handler, which
/// was the sole construction site of `NackAck`) and round-trip codec
/// tests. Because `ControlMessage` rides bincode FIXINT encoding, the
/// variant tag IS the declaration index, so removing them RENUMBERS every
/// later variant — a v6 peer's `WindowAck` would deserialize as something
/// else entirely. Hence the version bump: both `Handshake::deserialize`
/// and `WireMessage::deserialize` hard-refuse a version mismatch, so a
/// mixed v6/v7 pair fails cleanly and loudly at handshake instead of
/// silently mis-parsing control traffic. `WindowSwitch` is KEPT despite
/// also never being sent: its receive arm is a deliberate
/// hostile-peer/version guard that warns and ignores.
/// v8: ONE coordinated bump carrying TWO additions, both required by the
/// placement/receiver-law instrument pass (plan Stage 1 item 3):
///   (a) `SymbolBatch.eta_rel_us: u32` -- the SENDER'S OWN PREDICTION of this
///       batch's remaining delivery time, in us, relative to
///       `send_timestamp_us`. It is the `expected_delivery_load()` of the
///       path the placement law JUST PICKED, stamped at the placement lock,
///       so the receiver can read every arrival as "late against the
///       sender's own model" instead of "out of sequence" -- the lateness
///       measurand 16.80 named and never built. **0 is the "no prediction"
///       sentinel** (the `cum_received` convention): every emitter that is
///       not the window source path leaves it 0, and the receiver's bind
///       gauge reports the zero fraction rather than hiding it. Carried in
///       BOTH framings: one varint after `batch_seq` in the compact frame,
///       one bincode field in the legacy one.
///   (b) `ControlMessage::RepairRequest { spans, cause }` -- the receiver-seat
///       repair vocabulary (16.83). APPENDED AFTER `GenerationDeficit` and
///       never reordered: bincode FIXINT encodes the variant tag as the
///       DECLARATION INDEX, so appending is the only non-breaking edit, and
///       the v7 note above records what happens when that rule is broken.
///       NO SENDER exists in v8 -- the dispatch arm counts and ignores, so a
///       hostile or future peer cannot panic this binary. The variant is on
///       the wire now precisely so the Stage-2 arms need no second bump.
/// Both `Handshake::deserialize` and `WireMessage::deserialize` hard-refuse a
/// version mismatch, so a mixed v7/v8 pair fails cleanly at handshake. Mixed
/// versions are broken BY DESIGN -- one binary per battery.
pub const PROTOCOL_VERSION: u32 = 8;
/// Magic bytes for wire format identification.
pub const WIRE_MAGIC: [u8; 4] = *b"RPTQ";

/// Compact DATA frame tag (v5). MUST stay distinct from the legacy magic's
/// first byte b'R' (0x52) — the receive path classifies on byte 0.
pub const COMPACT_DATA_TAG: u8 = 0xC1;

/// `RWM_WIRE_COMPACT` (default ON since 2026-08-06 — goal-gate "Window
/// Decoupling + MTU Scaling" part 2, flip earned by the pre-registered
/// battery: sc2 +2.59/+3.64, sc3 +0.55/+0.60 Mbit/s ≫σ both seeds, c7
/// +8.1/+4.6, c8 unregressed, crown tail spot unregressed, wire overhead
/// 119 → ~71 B/pkt at the qdisc gauge; `=0` is the legacy-framing opt-out
/// arm): sender-side compact DATA framing. Resolved once per process
/// (transport-layer knob, like `RWM_MTU_FLOOR`/`RWM_QUIC_CC`).
pub fn wire_compact_active() -> bool {
    static ACTIVE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ACTIVE.get_or_init(|| crate::config::env_flag("RWM_WIRE_COMPACT", true))
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
        FecBackend::ReedSolomon => 2,
        FecBackend::Rlc => 3,
    }
}

fn backend_from_u8(v: u8) -> Option<FecBackend> {
    Some(match v {
        0 => FecBackend::RaptorQ,
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
///   [varint send_timestamp_us][varint batch_seq][varint eta_rel_us]
///   [payload to the datagram end]
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
    // v8: the sender's own delivery-time prediction for this batch, us,
    // relative to `send_timestamp_us`. 0 = no prediction (one byte).
    write_varint(&mut buf, batch.eta_rel_us as u64);
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
    // v8: the ETA varint. Unconditional -- there is one compact layout per
    // protocol version and the handshake refuses a mismatch.
    let eta_rel_us = read_varint(data, &mut pos).ok_or_else(|| err("compact truncated"))?;
    if path_id > u32::MAX as u64 || payload_id > u32::MAX as u64 || eta_rel_us > u32::MAX as u64 {
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
        eta_rel_us: eta_rel_us as u32,
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
    /// The path this batch was sent on (receiver keys its per-path batch_seq
    /// gap tracking and loss estimation off it).
    pub path_id: u32,
    /// **v8 -- THE SENDER'S OWN DELIVERY PREDICTION**, us, relative to
    /// `send_timestamp_us`: the `expected_delivery_load()` of the path the
    /// placement law just picked, stamped at the placement lock.
    ///
    /// **0 is the "no prediction" sentinel**, the `cum_received` convention.
    /// Every emitter other than the window source path leaves it 0 today, and
    /// the receiver's `[ETA]` gauge reports that BIND FRACTION on its face
    /// rather than dropping the samples silently -- an absent prediction and a
    /// predicted-zero delivery are not the same reading, and a prediction of
    /// exactly 0 us is physically impossible (the load term carries
    /// `srtt_i/2 > 0` at every path that has ever been measured).
    ///
    /// READ BY NOTHING in the data plane. It feeds two gauges and no law.
    pub eta_rel_us: u32,
}

impl SymbolBatch {
    /// The batch constructor every emission site goes through, so a future
    /// envelope field is ONE edit instead of eleven. `eta_rel_us` defaults to
    /// the 0 sentinel -- a site that has a prediction adds it with
    /// [`Self::with_eta`], which is the only way the field is ever nonzero.
    pub fn new(
        symbols: Vec<WireSymbol>,
        send_timestamp_us: u64,
        batch_seq: u64,
        path_id: u32,
    ) -> Self {
        Self { symbols, send_timestamp_us, batch_seq, path_id, eta_rel_us: 0 }
    }

    /// Stamp the sender's own delivery prediction (us, relative to
    /// `send_timestamp_us`). Saturating: the field is a `u32` and a
    /// prediction beyond ~71 min is pinned rather than wrapped.
    pub fn with_eta(mut self, eta_rel_us: u64) -> Self {
        self.eta_rel_us = eta_rel_us.min(u32::MAX as u64) as u32;
        self
    }
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

    /// Keepalive / path probe.
    Ping { timestamp_us: u64 },
    Pong { echo_timestamp_us: u64 },

    /// Graceful shutdown notification.
    Shutdown,

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
        /// v6 (goal-gate "Unlock The Default 1: ack-merge"): the receiver's
        /// per-path CUMULATIVE expected-symbol counter
        /// (`PathBatchTracker::total_expected` — batch-gap derived). Paired
        /// with `cum_received` below, this is the entire payload of the legacy
        /// per-batch `ControlMessage::Ack`, carried here so window mode can
        /// merge two control datagrams into one. The sender DIFFS it against a
        /// per-path cursor to recover `(expected, received)` for the loss
        /// estimator and the in-flight release — identical totals to the
        /// legacy arm, and robust to a dropped ack (the next one carries the
        /// whole delta).
        cum_expected: u64,
        /// v6: the receiver's per-path CUMULATIVE received-symbol counter
        /// (`PathBatchTracker::total_received`). See `cum_expected`.
        ///
        /// **Zero is the "no counter payload" sentinel**, exactly parallel to
        /// the existing `echo_send_timestamp_us == 0` timer-ack sentinel: the
        /// two timer-driven `WindowAck` sites (hole re-advertisement and
        /// hold-expiry unwedge) broadcast one message to every live path and
        /// therefore cannot carry a per-path counter. A data-triggered ack
        /// always reports at least the symbol that triggered it, so a real
        /// counter payload is never 0.
        cum_received: u64,
    },

    /// Sender signals backend switch at a window flush point (sender → receiver).
    ///
    /// NEVER SENT by this binary (mid-stream FEC backend switching was retired,
    /// ADR-0030 / goal-gate "streaming retirement"). The variant and its receive
    /// arm are KEPT deliberately as a hostile-peer / future-version guard: an
    /// inbound `WindowSwitch` is warned about and ignored rather than silently
    /// mis-parsed. Do not delete without a version bump.
    WindowSwitch {
        /// Last source sequence number under the old backend.
        flush_seq: u64,
        /// The new FEC backend to switch to.
        new_backend: FecBackend,
        /// Symbol size for the new backend.
        symbol_size: u16,
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

    /// **v8 -- THE RECEIVER-SEAT REPAIR REQUEST** (paper 16.83, receiver ->
    /// sender). The vocabulary change S3 names: instead of "resend seq s"
    /// (which a false repair can express) the receiver says "over this span I
    /// need this many more independent equations", which a false repair
    /// cannot express at all.
    ///
    /// APPENDED AFTER `GenerationDeficit` and never to be reordered -- the
    /// bincode FIXINT variant tag IS the declaration index (see the v7 note
    /// on `PROTOCOL_VERSION`).
    ///
    /// **NOTHING SENDS THIS IN v8.** The receive arm counts it
    /// (`repair_request_ignored()`) and returns; the Stage-2 request-law and
    /// rank-feedback arms are what will construct it. It rides the wire now
    /// so those arms need no second version bump.
    RepairRequest {
        /// `(start, count, deficit)` per requested span: the span
        /// `[start, start + count)` and how many MORE independent coded
        /// symbols over it the receiver still needs. `deficit = 1` over a
        /// one-seq span is exactly today's per-seq copy request, so the
        /// shipped machine is the `m = 1` corner of this message rather than
        /// a different one.
        spans: Vec<(u64, u16, u32)>,
        /// Why the request fired -- the `[FCAUSE]` class vocabulary, carried
        /// as a plain `u8` so a future cause never renumbers a variant.
        cause: u8,
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
            (FecBackend::ReedSolomon, false),
        ] {
            let batch = SymbolBatch::new(
                vec![sym(u64::MAX / 3, u32::MAX, is_repair, backend, 1200)],
                123_456_789_012,
                987_654,
                7,
            )
            // v8: a NONZERO eta must survive the compact frame; the 0
            // sentinel is pinned separately below.
            .with_eta(31_415);
            let buf = serialize_data_compact(&batch).expect("one-symbol batch");
            assert_eq!(buf[0], COMPACT_DATA_TAG);
            let msg = WireMessage::deserialize(&buf).expect("compact parse");
            match msg {
                WireMessage::Data(b) => {
                    assert_eq!(b.path_id, batch.path_id);
                    assert_eq!(b.batch_seq, batch.batch_seq);
                    assert_eq!(b.send_timestamp_us, batch.send_timestamp_us);
                    assert_eq!(b.eta_rel_us, batch.eta_rel_us, "v8 ETA must survive");
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
        let batch = SymbolBatch::new(
            vec![sym(50_000, 0, false, FecBackend::Rlc, 1200)],
            30_000_000_000, // ~8.3 h session, worst plausible
            100_000,
            1,
        )
        // v8 worst case for the overhead bound: the widest ETA varint the
        // field can hold (u32::MAX us ~ 71 min => 5 bytes).
        .with_eta(u32::MAX as u64);
        let buf = serialize_data_compact(&batch).unwrap();
        let overhead = buf.len() - 1200;
        assert!(
            overhead <= 29,
            "compact overhead {overhead} B exceeds the derivation bound \
             (24 B through v7 + <= 5 B for the v8 ETA varint)"
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
        let batch = SymbolBatch::new(
            vec![
                sym(1, 0, false, FecBackend::Rlc, 100),
                sym(2, 1, false, FecBackend::Rlc, 100),
            ],
            1,
            1,
            0,
        );
        assert!(serialize_data_compact(&batch).is_none());
    }

    /// Legacy frames still parse unchanged (byte-identical receive path for
    /// all legacy traffic), and malformed/truncated compact frames error
    /// instead of panicking.
    #[test]
    fn legacy_parse_unchanged_and_compact_truncation_safe() {
        let batch = SymbolBatch::new(vec![sym(9, 3, true, FecBackend::Rlc, 64)], 42, 7, 2);
        let legacy = WireMessage::Data(batch).serialize().unwrap();
        assert_eq!(&legacy[..4], &WIRE_MAGIC);
        assert!(WireMessage::deserialize(&legacy).is_ok());
        // Truncations of a compact frame must all error cleanly.
        let full = serialize_data_compact(&SymbolBatch::new(
            vec![sym(1000, 1, false, FecBackend::Rlc, 32)],
            5_000_000,
            3,
            0,
        ))
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

    // -- v8 PINS ---------------------------------------------------------

    /// THE VERSION ITSELF. A silent bump is how a mixed-version battery
    /// happens; the number is pinned here and in the handshake test below so
    /// moving it is a deliberate two-line edit.
    #[test]
    fn protocol_version_is_eight() {
        assert_eq!(PROTOCOL_VERSION, 8, "the wire version moved without its pin");
    }

    /// **THE COMPACT LAYOUT, BYTE BY BYTE.** The v8 ETA varint sits AFTER
    /// `batch_seq` and BEFORE the payload, and the 0 sentinel costs exactly
    /// one byte -- so the "no prediction" case pays 1 B, not 4.
    #[test]
    fn compact_layout_places_the_eta_varint_after_batch_seq() {
        let base = SymbolBatch::new(vec![sym(3, 4, false, FecBackend::Rlc, 8)], 5, 6, 1);
        let zero = serialize_data_compact(&base).unwrap();
        let one = serialize_data_compact(&base.clone().with_eta(1)).unwrap();
        let wide = serialize_data_compact(&base.clone().with_eta(300)).unwrap();
        assert_eq!(one.len(), zero.len(), "a 1-byte varint either way");
        assert_eq!(wide.len(), zero.len() + 1, "300 us needs a second varint byte");
        // Field order: tag, flags, path, block, payload, send_ts, batch_seq,
        // eta, payload bytes. The 0 sentinel is the byte immediately before
        // the payload.
        assert_eq!(zero[0], COMPACT_DATA_TAG);
        assert_eq!(
            &zero[..zero.len() - 8],
            &[COMPACT_DATA_TAG, 3 << 1, 1, 3, 4, 5, 6, 0][..],
            "the compact v8 sub-header layout moved"
        );
        assert_eq!(&zero[zero.len() - 8..], &base.symbols[0].data[..]);
        // And the sentinel survives the round trip AS a sentinel.
        match WireMessage::deserialize(&zero).unwrap() {
            WireMessage::Data(b) => assert_eq!(b.eta_rel_us, 0),
            _ => panic!("expected Data"),
        }
        match WireMessage::deserialize(&wide).unwrap() {
            WireMessage::Data(b) => assert_eq!(b.eta_rel_us, 300),
            _ => panic!("expected Data"),
        }
    }

    /// `with_eta` SATURATES rather than wrapping: a prediction past the
    /// field's range reads as the maximum, never as a small number.
    #[test]
    fn with_eta_saturates_at_u32_max() {
        let b = SymbolBatch::new(vec![], 0, 0, 0).with_eta(u64::MAX);
        assert_eq!(b.eta_rel_us, u32::MAX);
        assert_eq!(SymbolBatch::new(vec![], 0, 0, 0).eta_rel_us, 0, "the constructor's sentinel");
    }

    /// **THE v8 CONTROL VARIANT ROUND-TRIPS, AND IT WAS APPENDED.** The
    /// second assertion is the one that matters: bincode FIXINT encodes the
    /// variant tag as the declaration INDEX, so `RepairRequest` must decode
    /// at a tag STRICTLY GREATER than `GenerationDeficit`'s. A reorder would
    /// make a v8 peer mis-parse every later control message, which is the
    /// exact defect the v7 note records.
    #[test]
    fn repair_request_roundtrips_and_sits_after_generation_deficit() {
        let msg = WireMessage::Control(ControlMessage::RepairRequest {
            spans: vec![(100, 8, 3), (250, 1, 1)],
            cause: 2,
        });
        let bytes = msg.serialize().unwrap();
        match WireMessage::deserialize(&bytes).unwrap() {
            WireMessage::Control(ControlMessage::RepairRequest { spans, cause }) => {
                assert_eq!(spans, vec![(100u64, 8u16, 3u32), (250, 1, 1)]);
                assert_eq!(cause, 2);
            }
            other => panic!("expected RepairRequest, got {other:?}"),
        }
        // The variant tag is the first 4 bytes of the bincode body, after
        // the 8-byte magic+version header and the 4-byte WireMessage tag.
        let tag_of = |m: &WireMessage| -> u32 {
            let b = m.serialize().unwrap();
            u32::from_le_bytes(b[12..16].try_into().unwrap())
        };
        let deficit = WireMessage::Control(ControlMessage::GenerationDeficit {
            deficits: vec![(0, 0)],
        });
        assert!(
            tag_of(&msg) > tag_of(&deficit),
            "RepairRequest ({}) must be APPENDED after GenerationDeficit ({}) --              a bincode variant tag IS its declaration index",
            tag_of(&msg),
            tag_of(&deficit),
        );
    }

    /// The handshake refuses a mismatch, pinned AT v8 -- so a v7 peer fails
    /// cleanly and loudly instead of mis-parsing control traffic.
    #[test]
    fn handshake_refuses_a_version_mismatch_at_v8() {
        let hs = Handshake { version: PROTOCOL_VERSION, max_block_size: 64, symbol_size: 1200, path_id: 0 };
        let good = hs.serialize().unwrap();
        assert_eq!(u32::from_be_bytes(good[4..8].try_into().unwrap()), 8);
        assert!(Handshake::deserialize(&good).is_ok());
        let mut stale = good.clone();
        stale[4..8].copy_from_slice(&7u32.to_be_bytes());
        let e = Handshake::deserialize(&stale).unwrap_err().to_string();
        assert!(e.contains("version mismatch"), "{e}");
        // And the data path refuses it too.
        let mut d = WireMessage::Control(ControlMessage::Shutdown).serialize().unwrap();
        d[4..8].copy_from_slice(&7u32.to_be_bytes());
        assert!(WireMessage::deserialize(&d).is_err());
    }
}
