//! Behaviour pin for the window sender's per-symbol hot path (perf work on
//! allocation / copy / clock reads must be byte-identical).
//!
//! Pinned at the pure layer — no `RuntimeGates`, no transport, no clock, no
//! RNG — so the digests depend only on the code under test:
//!
//! * the source path: `frame_window_packet` → `WindowEncoder::add_source` →
//!   the one-symbol `SymbolBatch` → `serialize_data_compact` (wire v9
//!   compact framing) AND the bincode `WireMessage::Data` framing, for a
//!   scripted packet sequence whose lengths and envelope fields straddle the
//!   padding edge and every varint width boundary;
//! * the repair path: `generate_repair` / `generate_repair_range` /
//!   `get_source` / `advance` interleaved with the intake (RLC sliding
//!   window and the systematic generation encoder);
//! * the receiver's view: every compact frame parses back to the batch it
//!   was serialized from.
//!
//! The digests were captured on the untouched tree (main 773ca1c) before any
//! hot-path change. A deliberate wire-format change re-captures them
//! (`HOTPATH_PIN_PRINT=1` prints the live values) and must say so.

use sha2::{Digest, Sha256};

use crate::fec::{GenerationEncoder, RlcWindowEncoder, WindowEncoder, WireSymbol};
use crate::net::framing::frame_window_packet;
use crate::transport::{serialize_data_compact, SymbolBatch, WireMessage};

const T: u16 = 1200;

/// Scripted packet lengths: empty, tiny, mid, the padding edge (T−2 is the
/// largest payload that fits the 2-byte length prefix) and an oversize one
/// that `frame_window_packet` truncates.
const LENS: [usize; 9] = [0, 1, 63, 600, 1197, 1198, 1199, 1200, 1500];

/// Envelope values straddling the varint width boundaries (7/14/21/28 bits
/// and the 64-bit top).
const VARINTS: [u64; 10] = [
    0,
    1,
    127,
    128,
    16_383,
    16_384,
    2_097_151,
    2_097_152,
    1_759_000_000_000_000, // an epoch-µs timestamp
    u64::MAX,
];

fn packet(i: usize, len: usize) -> Vec<u8> {
    (0..len).map(|j| (i.wrapping_mul(131) ^ j.wrapping_mul(7) ^ (j >> 3)) as u8).collect()
}

/// Hash one symbol's fields, framed so adjacent fields cannot alias.
fn hash_sym(h: &mut Sha256, s: &WireSymbol) {
    h.update(s.block_id.to_le_bytes());
    h.update(s.payload_id.to_le_bytes());
    h.update([s.is_repair as u8]);
    h.update(format!("{:?}", s.backend).as_bytes());
    h.update((s.data.len() as u64).to_le_bytes());
    h.update(&s.data[..]);
}

/// Serialize `sym` both ways for envelope variant `k`, hash both frames,
/// and check the compact frame parses back to exactly this batch.
fn hash_wire(h: &mut Sha256, sym: &WireSymbol, k: usize) {
    let ts = VARINTS[k % VARINTS.len()];
    let bseq = VARINTS[(k + 3) % VARINTS.len()];
    let pseq = VARINTS[(k + 5) % VARINTS.len()];
    let path = (VARINTS[(k + 7) % VARINTS.len()] % (u32::MAX as u64 + 1)) as u32;
    let eta = VARINTS[(k + 2) % VARINTS.len()];
    let batch = SymbolBatch::new(vec![sym.clone()], ts, (bseq, pseq), path).with_eta(eta);

    let compact = serialize_data_compact(&batch).expect("one-symbol batch is compact");
    h.update((compact.len() as u64).to_le_bytes());
    h.update(&compact[..]);

    let bin = WireMessage::Data(batch.clone()).serialize().expect("bincode frame");
    h.update((bin.len() as u64).to_le_bytes());
    h.update(&bin[..]);

    // The receiver's view of both frames is the batch that was sent.
    for frame in [&compact[..], &bin[..]] {
        match WireMessage::deserialize(frame).expect("frame parses") {
            WireMessage::Data(b) => {
                assert_eq!(b.send_timestamp_us, batch.send_timestamp_us);
                assert_eq!(b.batch_seq, batch.batch_seq);
                assert_eq!(b.path_seq, batch.path_seq);
                assert_eq!(b.path_id, batch.path_id);
                assert_eq!(b.eta_rel_us, batch.eta_rel_us);
                assert_eq!(b.symbols.len(), 1);
                let r = &b.symbols[0];
                assert_eq!(r.block_id, sym.block_id);
                assert_eq!(r.payload_id, sym.payload_id);
                assert_eq!(r.is_repair, sym.is_repair);
                assert_eq!(r.backend, sym.backend);
                assert_eq!(&r.data[..], &sym.data[..]);
            }
            WireMessage::Control(_) => panic!("data frame parsed as control"),
        }
    }
}

/// Drive one encoder through the scripted intake/repair/advance sequence.
fn drive(enc: &mut dyn WindowEncoder, h: &mut Sha256) {
    let mut k = 0usize;
    for i in 0..96usize {
        let len = LENS[i % LENS.len()];
        let framed = frame_window_packet(&packet(i, len), T);
        h.update((framed.len() as u64).to_le_bytes());
        h.update(&framed[..]);

        let src = enc.add_source(&framed);
        hash_sym(h, &src);
        hash_wire(h, &src, k);
        k += 1;

        // The exact source the retransmit / taper-copy paths serve.
        if let Some(back) = enc.get_source(src.block_id) {
            hash_sym(h, &back);
        }

        if i % 3 == 2 {
            let rep = enc.generate_repair();
            hash_sym(h, &rep);
            hash_wire(h, &rep, k);
            k += 1;
        }
        if i % 5 == 4 {
            let (ws, we) = enc.window_span();
            let start = ws + (we - ws) / 2;
            let count = ((we + 1 - start) as u16).max(1);
            if let Some(rep) = enc.generate_repair_range(start, count) {
                hash_sym(h, &rep);
                hash_wire(h, &rep, k);
                k += 1;
            }
        }
        if i % 16 == 15 {
            let (_, we) = enc.window_span();
            enc.advance(we.saturating_sub(10));
            let span = enc.window_span();
            h.update(span.0.to_le_bytes());
            h.update(span.1.to_le_bytes());
            h.update((enc.window_size() as u64).to_le_bytes());
        }
    }
}

fn digest(enc: &mut dyn WindowEncoder) -> String {
    let mut h = Sha256::new();
    drive(enc, &mut h);
    hex::encode(h.finalize())
}

fn check(name: &str, got: &str, want: &str) {
    if std::env::var_os("HOTPATH_PIN_PRINT").is_some() {
        println!("HOTPATH_PIN {name} = {got}");
    }
    assert_eq!(got, want, "{name}: the sender hot path's wire/encoder output changed");
}

/// RLC sliding window (the shipped window sender's encoder).
#[test]
fn pin_rlc_window_source_and_repair_wire_bytes() {
    let mut enc = RlcWindowEncoder::new(T);
    check(
        "rlc",
        &digest(&mut enc),
        "98e7b35280352b079c57463e16b11898bc568facf70e68ae1be86cf17389cc9e",
    );
}

/// Systematic generation encoder (the `RWM_GEN` systematic arm).
#[test]
fn pin_systematic_generation_source_and_repair_wire_bytes() {
    let mut enc = GenerationEncoder::new_systematic(T, 16, 4, 0.15);
    check(
        "gen_systematic",
        &digest(&mut enc),
        "446fe60d36755c5cd6fda8d83387714a5d76f5d9da4580b1f4d1b6c3fbd6c1aa",
    );
}

/// A frame is exactly `symbol_size` bytes: LE u16 length, the (truncated)
/// payload, zero padding — and the encoder stores and emits it unchanged.
#[test]
fn pin_frame_layout_and_add_source_identity() {
    let mut enc = RlcWindowEncoder::new(T);
    for (i, &len) in LENS.iter().enumerate() {
        let pkt = packet(i, len);
        let framed = frame_window_packet(&pkt, T);
        assert_eq!(framed.len(), T as usize);
        let n = len.min(T as usize - 2);
        assert_eq!(u16::from_le_bytes([framed[0], framed[1]]) as usize, n);
        assert_eq!(&framed[2..2 + n], &pkt[..n]);
        assert!(framed[2 + n..].iter().all(|&b| b == 0));
        let s = enc.add_source(&framed);
        assert_eq!(&s.data[..], &framed[..]);
        assert!(!s.is_repair);
        assert_eq!(s.payload_id, 0);
        assert_eq!(&enc.get_source(s.block_id).unwrap().data[..], &framed[..]);
    }
}
