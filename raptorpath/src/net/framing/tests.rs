use super::*;

/// Pack `pkts` back to back into a zeroed `size`-byte symbol with the one writer.
fn pack(pkts: &[&[u8]], size: usize) -> Vec<u8> {
    let mut sym = vec![0u8; size];
    let mut at = 0;
    for p in pkts {
        at += put_packet(&mut sym[at..], p);
    }
    sym
}

fn read_packets_ok(sym: &[u8]) -> Vec<Vec<u8>> {
    read_packets(sym).unwrap()
}

/// Run a packer over `pkts` then flush: every symbol it emits, in order.
fn packer_symbols(size: u16, pkts: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut packer = SymbolPacker::new(size, Duration::from_millis(1));
    let mut out: Vec<Vec<u8>> = pkts.iter().filter_map(|p| packer.push(p)).collect();
    out.extend(packer.flush());
    out
}

// -----------------------------------------------------------------------
// The one framing: [u16 LE len][bytes], zero padding terminates
// -----------------------------------------------------------------------

/// Endianness by byte literal (F2 design §8.1): the reader and the writer are
/// pinned to LE absolutely, not by round trip.
#[test]
fn framing_is_little_endian_by_byte_literal() {
    let mut sym = vec![3, 0, 0xAA, 0xBB, 0xCC, 1, 0, 0xDD, 0, 0];
    sym.resize(16, 0);
    assert_eq!(
        read_packets(&sym).unwrap(),
        vec![vec![0xAA, 0xBB, 0xCC], vec![0xDD]],
        "a BE reader would read 768 and yield nothing"
    );

    let mut dst = [0xEEu8; 8];
    assert_eq!(put_packet(&mut dst, &[0xAA, 0xBB, 0xCC]), 5);
    assert_eq!(&dst[..5], &[3, 0, 0xAA, 0xBB, 0xCC]);
    assert_eq!(&dst[5..], &[0xEE; 3], "the writer touches exactly 2 + len bytes");

    let mut big = vec![0u8; 304];
    assert_eq!(put_packet(&mut big, &[7u8; 300]), 302);
    assert_eq!(&big[..2], &[0x2C, 0x01], "300 = 0x012C little-endian");
}

/// One-packet and packed round trips, absolute, at both symbol sizes (§8.2).
#[test]
fn round_trips_at_1200_and_512() {
    for size in [1200u16, 512] {
        let max = size as usize - 2;
        // 508 is Realtime's TUN maximum (S-4), 1196 Bulk's.
        for len in [1usize, 508, 1196, max].into_iter().filter(|&l| l <= max) {
            let pkt: Vec<u8> = (0..len).map(|i| (i % 251) as u8 + 1).collect();
            let sym = frame_window_packet(&pkt, size);
            assert_eq!(sym.len(), size as usize);
            assert_eq!(read_packets(&sym).unwrap(), vec![pkt], "S={size} len={len}");
        }
        // Over capacity: truncated to S-2, still one packet.
        let sym = frame_window_packet(&vec![9u8; size as usize + 100], size);
        assert_eq!(read_packets(&sym).unwrap(), vec![vec![9u8; max]], "S={size} truncation");

        let pkts = [vec![1u8; 160], vec![2u8; 160], vec![3u8; 160]];
        let refs: Vec<&[u8]> = pkts.iter().map(|p| &p[..]).collect();
        let syms = packer_symbols(size, &refs);
        assert_eq!(syms.len(), 1, "3 x 160 share one symbol at S={size}");
        assert_eq!(syms[0].len(), size as usize);
        assert_eq!(read_packets(&syms[0]).unwrap(), pkts.to_vec(), "S={size} packed");
    }
}

/// The two writers agree: a packer holding one packet emits the symbol
/// `frame_window_packet` would (§8.2).
#[test]
fn packer_and_window_framer_agree_on_one_packet() {
    for size in [1200u16, 512, 64] {
        let pkt = vec![0x5Au8; 100.min(size as usize - 4)];
        let syms = packer_symbols(size, &[&pkt]);
        assert_eq!(syms, vec![frame_window_packet(&pkt, size)], "S={size}");
    }
    // The packer's oversize truncation is S-4 (its own, kept) and the framer's
    // is S-2: both still read back as one packet.
    let big = vec![0xABu8; 200];
    let p = packer_symbols(64, &[&big]);
    assert_eq!(read_packets(&p[0]).unwrap(), vec![vec![0xABu8; 60]]);
}

// -----------------------------------------------------------------------
// Unframable symbols and the packer's dirty buffer (§8.3)
// -----------------------------------------------------------------------

#[test]
fn read_packets_unframable_cases() {
    // A length that overruns the symbol, after a valid first packet.
    let mut sym = pack(&[&[1, 2, 3]], 16);
    sym[5..7].copy_from_slice(&100u16.to_le_bytes());
    assert_eq!(read_packets(&sym), Err(Unframable));

    // A first length that overruns.
    let mut sym = vec![0u8; 10];
    sym[0..2].copy_from_slice(&100u16.to_le_bytes());
    assert_eq!(read_packets(&sym), Err(Unframable));

    // Non-zero bytes, no packet: a zero length first, junk after.
    assert_eq!(read_packets(&[0, 0, 5, 0, 0, 0]), Err(Unframable));
    // A single non-zero byte: fewer than 2 bytes left, no packet.
    assert_eq!(read_packets(&[5]), Err(Unframable));

    // All zeros (any length, including empty) is the frame of an empty packet.
    assert_eq!(read_packets(&vec![0u8; 512]), Ok(vec![]));
    assert_eq!(read_packets(&[]), Ok(vec![]));
    assert_eq!(read_packets(&[0]), Ok(vec![]));
}

/// Packets that end exactly at the symbol end, and a 1-byte tail after the
/// last packet, both terminate cleanly (no terminator bytes needed).
#[test]
fn read_packets_terminates_at_symbol_end() {
    let sym = pack(&[&[1u8; 10], &[2u8; 50]], 64);
    assert_eq!(read_packets(&sym).unwrap(), vec![vec![1u8; 10], vec![2u8; 50]]);
    let sym = pack(&[&[1u8; 10], &[2u8; 49]], 64);
    assert_eq!(read_packets(&sym).unwrap().len(), 2, "one tail byte left");
}

/// Dirty buffer (§8.3): the packer reuses its buffer. After a 3-packet symbol
/// (and after an oversize truncated push), the next symbol carries only its own
/// bytes followed by zeros.
#[test]
fn packer_never_leaks_a_previous_symbol_into_the_next() {
    let mut packer = SymbolPacker::new(512, Duration::from_millis(1));
    for _ in 0..3 {
        assert!(packer.push(&[0xEEu8; 160]).is_none());
    }
    let first = packer.flush().unwrap();
    assert_eq!(read_packets(&first).unwrap().len(), 3);

    assert!(packer.push(&[0x11u8; 10]).is_none());
    let second = packer.flush().unwrap();
    assert_eq!(&second[..2], &[10, 0]);
    assert_eq!(&second[2..12], &[0x11u8; 10]);
    assert!(second[12..].iter().all(|&b| b == 0), "only zeros after the packet");
    assert_eq!(read_packets(&second).unwrap(), vec![vec![0x11u8; 10]]);

    // Repeat after an oversize (truncated) push.
    let big = packer.push(&[0xEEu8; 900]).expect("oversize emits solo");
    assert_eq!(read_packets(&big).unwrap(), vec![vec![0xEEu8; 508]]);
    assert!(packer.push(&[0x22u8; 10]).is_none());
    let third = packer.flush().unwrap();
    assert!(third[12..].iter().all(|&b| b == 0));
    assert_eq!(read_packets(&third).unwrap(), vec![vec![0x22u8; 10]]);
}

// -----------------------------------------------------------------------
// Window-mode framing tests
// -----------------------------------------------------------------------

#[test]
fn test_window_frame_roundtrip() {
    let packet = vec![1, 2, 3, 4, 5];
    let symbol = frame_window_packet(&packet, 64);
    assert_eq!(symbol.len(), 64);

    let extracted = read_packets(&symbol).unwrap().remove(0);
    assert_eq!(extracted, packet);
}

#[test]
fn test_window_frame_empty_packet() {
    // An empty packet frames to all zeros, which reads as no packet (not an error).
    let symbol = frame_window_packet(&[], 64);
    assert_eq!(symbol.len(), 64);
    assert_eq!(read_packets(&symbol), Ok(vec![]));
}

#[test]
fn test_window_frame_max_payload() {
    // Packet fills entire symbol (minus 2-byte length prefix)
    let packet = vec![0xAB; 62]; // 64 - 2
    let symbol = frame_window_packet(&packet, 64);
    let extracted = read_packets(&symbol).unwrap().remove(0);
    assert_eq!(extracted, packet);
}

#[test]
fn test_window_frame_oversized_packet_truncated() {
    // Packet larger than symbol capacity — silently truncated
    let packet = vec![0xFF; 200];
    let symbol = frame_window_packet(&packet, 64);
    let extracted = read_packets(&symbol).unwrap().remove(0);
    assert_eq!(extracted.len(), 62); // 64 - 2
}

#[test]
fn test_window_frame_too_short_symbol() {
    assert_eq!(read_packets(&[]), Ok(vec![]));
    assert_eq!(read_packets(&[0]), Ok(vec![]));
}

#[test]
fn test_window_frame_corrupt_length() {
    // Length field points past end of data
    let mut symbol = vec![0u8; 10];
    symbol[0..2].copy_from_slice(&100u16.to_le_bytes());
    assert_eq!(read_packets(&symbol), Err(Unframable));
}

// -----------------------------------------------------------------------
// SymbolPacker tests
// -----------------------------------------------------------------------

#[test]
fn test_packer_roundtrip_single_packet() {
    let mut packer = SymbolPacker::new(64, Duration::from_millis(1));
    let packet = vec![1, 2, 3, 4, 5];
    // First push shouldn't emit (buffer not full)
    assert!(packer.push(&packet).is_none());
    // Flush should emit a packed symbol
    let symbol = packer.flush().unwrap();
    assert_eq!(symbol.len(), 64);
    let extracted = read_packets_ok(&symbol);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], packet);
}

#[test]
fn test_packer_roundtrip_multi_packet() {
    let mut packer = SymbolPacker::new(512, Duration::from_millis(1));
    let packets = vec![
        vec![10; 50],  // 52 bytes framed
        vec![20; 80],  // 82 bytes framed
        vec![30; 100], // 102 bytes framed
    ];
    // All three fit: 52 + 82 + 102 = 236 < 510 (capacity)
    for p in &packets {
        assert!(packer.push(p).is_none());
    }
    let symbol = packer.flush().unwrap();
    assert_eq!(symbol.len(), 512);
    let extracted = read_packets_ok(&symbol);
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted, packets);
}

#[test]
fn test_packer_auto_flush_on_full() {
    let mut packer = SymbolPacker::new(64, Duration::from_millis(1));
    // Capacity = 62 bytes. Pack two small packets, then a third that triggers flush.
    let p1 = vec![1; 20]; // 22 framed
    let p2 = vec![2; 20]; // 22 framed → 44 total
    let p3 = vec![3; 20]; // 22 framed → would be 66, exceeds 62

    assert!(packer.push(&p1).is_none());
    assert!(packer.push(&p2).is_none());
    // p3 should trigger a flush of p1+p2
    let flushed = packer.push(&p3).unwrap();
    assert_eq!(flushed.len(), 64);
    let extracted = read_packets_ok(&flushed);
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0], p1);
    assert_eq!(extracted[1], p2);

    // p3 should still be in the buffer
    let remaining = packer.flush().unwrap();
    let extracted = read_packets_ok(&remaining);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], p3);
}

#[test]
fn test_packer_oversized_packet() {
    let mut packer = SymbolPacker::new(64, Duration::from_millis(1));
    // Packet larger than capacity (62 bytes) — gets truncated
    let big = vec![0xAB; 200];
    let symbol = packer.push(&big).unwrap();
    assert_eq!(symbol.len(), 64);
    let extracted = read_packets_ok(&symbol);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].len(), 60); // 62 capacity - 2 length prefix
}

#[test]
fn test_packer_flush_empty() {
    let mut packer = SymbolPacker::new(64, Duration::from_millis(1));
    assert!(packer.flush().is_none());
}

#[test]
fn test_packer_should_flush() {
    let mut packer = SymbolPacker::new(64, Duration::from_millis(1));
    // Empty buffer → should not flush
    assert!(!packer.should_flush());
    packer.push(&[1, 2, 3]);
    // Just pushed → should not flush yet
    assert!(!packer.should_flush());
    // After timeout, should flush
    std::thread::sleep(Duration::from_millis(2));
    assert!(packer.should_flush());
}

#[test]
fn test_packer_voip_packing_ratio() {
    // VoIP scenario: 160B packets in 512B symbols
    // Without packing: 1 packet per symbol = 31% utilization
    // With packing: ~3 packets per symbol = 94% utilization
    let mut packer = SymbolPacker::new(512, Duration::from_millis(1));
    let voip_packet = vec![0x42; 160]; // typical G.711 20ms frame

    let mut symbols_emitted = 0;
    let mut packets_sent = 0;
    for _ in 0..30 {
        if let Some(_sym) = packer.push(&voip_packet) {
            symbols_emitted += 1;
        }
        packets_sent += 1;
    }
    if packer.flush().is_some() {
        symbols_emitted += 1;
    }

    // 30 packets should fit in ~10 symbols (3 per symbol)
    // vs 30 symbols without packing
    assert!(
        symbols_emitted <= 11,
        "Expected <=11 symbols for 30 VoIP packets, got {}",
        symbols_emitted
    );
    assert!(
        packets_sent == 30,
        "Should have sent all 30 packets"
    );
}

// -----------------------------------------------------------------------
// Every decoder hands the reader a full, zero-padded symbol (§1.1, §8.3)
// -----------------------------------------------------------------------

/// For each decoder: a source arriving with a short payload (its zero tail cut
/// off) and a seq recovered from repair both come out as exactly `symbol_size`
/// bytes, and `read_packets` gives back the packet that was sent. The packets
/// end in zero bytes, so a decoder that trimmed would be caught.
#[test]
fn every_decoder_returns_a_full_size_symbol_that_reads_back() {
    use crate::fec::{
        GenerationDecoder, GenerationEncoder, RlcWindowDecoder, RlcWindowEncoder, UnifiedDecoder,
        WindowDecoder, WindowEncoder,
    };
    use bytes::Bytes;

    const S: u16 = 64;
    const G: u64 = 6;
    let pkt = |i: u64| -> Vec<u8> {
        let mut p = vec![i as u8 + 1; 20];
        p.extend_from_slice(&[0, 0, 0, 0]); // trailing zeros
        p
    };

    fn run(
        name: &str,
        enc: &mut dyn WindowEncoder,
        dec: &mut dyn WindowDecoder,
        pkt: &dyn Fn(u64) -> Vec<u8>,
    ) {
        let srcs: Vec<_> = (0..G).map(|i| enc.add_source(&frame_window_packet(&pkt(i), S))).collect();
        let mut got: Vec<(u64, Bytes)> = Vec::new();
        // Source 0 arrives with its zero tail cut off: the decoder must pad it.
        let mut short = srcs[0].clone();
        short.data = short.data.slice(..2 + pkt(0).len());
        got.extend(dec.add_symbol(&short));
        // Source 2 is lost; the rest arrive full.
        for s in srcs.iter().skip(1).filter(|s| s.block_id != 2) {
            got.extend(dec.add_symbol(s));
        }
        // Repair until the hole is recovered.
        for _ in 0..8 {
            got.extend(dec.add_symbol(&enc.generate_repair()));
        }
        for seq in 0..G {
            let (_, data) = got
                .iter()
                .find(|(s, _)| *s == seq)
                .unwrap_or_else(|| panic!("{name}: seq {seq} not delivered"));
            assert_eq!(data.len(), S as usize, "{name}: seq {seq} is not full size");
            assert_eq!(read_packets(data).unwrap(), vec![pkt(seq)], "{name}: seq {seq}");
        }
    }

    run("unified/rlc", &mut RlcWindowEncoder::new(S), &mut UnifiedDecoder::new(S), &pkt);
    run("rlc_window", &mut RlcWindowEncoder::new(S), &mut RlcWindowDecoder::new(S), &pkt);
    run(
        "generation",
        &mut GenerationEncoder::new(S, G as usize, 4, 0.25),
        &mut GenerationDecoder::new(S),
        &pkt,
    );
    run(
        "unified/generation",
        &mut GenerationEncoder::new(S, G as usize, 4, 0.25),
        &mut UnifiedDecoder::new(S),
        &pkt,
    );
}

// -----------------------------------------------------------------------
// framing_ns: host ns bench (F2 design §7), ignored. BASE = v9's code inlined,
// CAND = the shipped framer and reader.
//
//   taskset -c 2 cargo test -p raptorpath --lib --release framing_ns -- --ignored --nocapture
// -----------------------------------------------------------------------

mod base_v9 {
    /// v9 `frame_window_packet_inner`.
    pub fn frame(data: &[u8], symbol_size: u16) -> Vec<u8> {
        let size = symbol_size as usize;
        let mut buf = vec![0u8; size];
        let max_payload = size.saturating_sub(2);
        let len = data.len().min(max_payload);
        buf[0..2].copy_from_slice(&(len as u16).to_le_bytes());
        buf[2..2 + len].copy_from_slice(&data[..len]);
        buf
    }

    /// v9 `extract_window_packet`.
    pub fn extract_one(symbol_data: &[u8]) -> Option<Vec<u8>> {
        if symbol_data.len() < 2 {
            return None;
        }
        let len = u16::from_le_bytes([symbol_data[0], symbol_data[1]]) as usize;
        if len == 0 || 2 + len > symbol_data.len() {
            return None;
        }
        Some(symbol_data[2..2 + len].to_vec())
    }

    /// v9 BE `extract_packets`.
    pub fn extract_packets(data: &[u8]) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let mut cursor = 0;
        while cursor + 2 <= data.len() {
            let len = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
            cursor += 2;
            if len == 0 {
                break;
            }
            if cursor + len > data.len() {
                break;
            }
            packets.push(data[cursor..cursor + len].to_vec());
            cursor += len;
        }
        packets
    }
}

#[test]
#[ignore = "host ns bench: run by hand, release, pinned core"]
fn framing_ns() {
    use std::hint::black_box;
    const CALLS: u32 = 2_000_000;
    const PAIRS: usize = 5;

    fn median_ns(mut v: Vec<f64>) -> (f64, f64, f64) {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (v[v.len() / 2], v[0], v[v.len() - 1])
    }
    // Alternating BASE/CAND pairs; each closure is one call on one input.
    fn point(name: &str, mut base: impl FnMut(), mut cand: impl FnMut()) {
        let (mut b, mut c) = (Vec::new(), Vec::new());
        for _ in 0..PAIRS {
            let t = Instant::now();
            for _ in 0..CALLS {
                base();
            }
            b.push(t.elapsed().as_nanos() as f64 / CALLS as f64);
            let t = Instant::now();
            for _ in 0..CALLS {
                cand();
            }
            c.push(t.elapsed().as_nanos() as f64 / CALLS as f64);
        }
        let (bm, blo, bhi) = median_ns(b);
        let (cm, clo, chi) = median_ns(c);
        println!("{name:<28} BASE {bm:7.1} [{blo:.1}-{bhi:.1}]  CAND {cm:7.1} [{clo:.1}-{chi:.1}]  ns/call");
    }

    // One-packet symbols at S = 1200: frame, then read.
    for len in [64usize, 600, 1196] {
        let pkt = vec![0x5Au8; len];
        point(
            &format!("frame S=1200 len={len}"),
            || {
                black_box(base_v9::frame(black_box(&pkt), 1200));
            },
            || {
                black_box(frame_window_packet(black_box(&pkt), 1200));
            },
        );
        let sym = frame_window_packet(&pkt, 1200);
        point(
            &format!("read  S=1200 len={len}"),
            || {
                black_box(base_v9::extract_one(black_box(&sym)));
            },
            || {
                // What the receiver does: unframe, then copy each packet out once.
                if let Ok(p) = unframe(black_box(&sym)) {
                    for s in p {
                        black_box(s.to_vec());
                    }
                }
            },
        );
    }

    // Packed: S = 512 with 3 x 160. BASE reads with v9's BE reader on a BE
    // symbol, CAND with `read_packets` on the LE one.
    let p160 = vec![0x42u8; 160];
    let le = packer_symbols(512, &[&p160, &p160, &p160]).remove(0);
    let mut be = vec![0u8; 512];
    let mut at = 0;
    for _ in 0..3 {
        be[at..at + 2].copy_from_slice(&160u16.to_be_bytes());
        be[at + 2..at + 162].copy_from_slice(&p160);
        at += 162;
    }
    point(
        "read  S=512 3x160 packed",
        || {
            black_box(base_v9::extract_packets(black_box(&be)));
        },
        || {
            if let Ok(p) = unframe(black_box(&le)) {
                for s in p {
                    black_box(s.to_vec());
                }
            }
        },
    );
}

/// `unframe` and `read_packets` agree on every symbol (the one reader), and the
/// slices it hands out are the packets in order.
#[test]
fn unframe_agrees_with_read_packets() {
    let mut cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        vec![5],
        vec![0, 0, 5, 0],
        pack(&[&[1, 2, 3], &[4]], 16),
        pack(&[&[9u8; 60]], 64),
        vec![3, 0, 1],
    ];
    cases.push(frame_window_packet(&[7u8; 300], 512));
    for sym in cases {
        let a = read_packets(&sym);
        let b = unframe(&sym).map(|p| p.map(<[u8]>::to_vec).collect::<Vec<_>>());
        assert_eq!(a, b, "{sym:?}");
    }
}
