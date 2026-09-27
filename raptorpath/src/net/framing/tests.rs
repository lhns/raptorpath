use super::*;

#[test]
fn test_frame_single_packet() {
    let mut buf = Vec::new();
    let packet = vec![1, 2, 3, 4, 5];
    frame_packet(&mut buf, &packet);
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], packet);
}

#[test]
fn test_frame_multiple_packets() {
    let mut buf = Vec::new();
    let packets = vec![
        vec![10, 20, 30],
        vec![40, 50],
        vec![60, 70, 80, 90],
    ];
    for p in &packets {
        frame_packet(&mut buf, p);
    }
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted, packets);
}

#[test]
fn test_frame_empty_block() {
    let mut buf = Vec::new();
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert!(extracted.is_empty());
}

#[test]
fn test_frame_large_packet() {
    let mut buf = Vec::new();
    let packet = vec![0xAB; 1500]; // typical MTU-sized packet
    frame_packet(&mut buf, &packet);
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].len(), 1500);
    assert_eq!(extracted[0], packet);
}

#[test]
fn test_frame_max_size_packet() {
    let mut buf = Vec::new();
    let packet = vec![0xFF; 65535]; // max u16 length
    frame_packet(&mut buf, &packet);
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].len(), 65535);
}

#[test]
fn test_extract_truncated_data() {
    // Simulate FEC padding — extra bytes after sentinel
    let mut buf = Vec::new();
    let packet = vec![1, 2, 3];
    frame_packet(&mut buf, &packet);
    frame_end(&mut buf);
    buf.extend_from_slice(&[0xFF; 100]); // padding

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], packet);
}

#[test]
fn test_extract_truncated_packet() {
    // Data ends mid-packet (corrupt/truncated block)
    let mut buf = Vec::new();
    buf.extend_from_slice(&10u16.to_be_bytes()); // claims 10 bytes
    buf.extend_from_slice(&[1, 2, 3]); // only 3 bytes

    let extracted = extract_packets(&buf);
    assert!(extracted.is_empty(), "Should skip truncated packet");
}

#[test]
fn test_extract_empty_data() {
    let extracted = extract_packets(&[]);
    assert!(extracted.is_empty());
}

#[test]
fn test_extract_only_sentinel() {
    let extracted = extract_packets(&[0, 0]);
    assert!(extracted.is_empty());
}

#[test]
fn test_roundtrip_many_packets() {
    let mut buf = Vec::new();
    let mut original = Vec::new();
    for i in 0..100 {
        let packet: Vec<u8> = (0..((i % 50) + 1)).map(|j| (j as u8).wrapping_add(i as u8)).collect();
        frame_packet(&mut buf, &packet);
        original.push(packet);
    }
    frame_end(&mut buf);

    let extracted = extract_packets(&buf);
    assert_eq!(extracted.len(), 100);
    assert_eq!(extracted, original);
}

#[test]
fn test_framing_overhead() {
    let mut buf = Vec::new();
    let packet = vec![0u8; 1000];
    frame_packet(&mut buf, &packet);
    frame_end(&mut buf);

    // 2 bytes length prefix + 1000 data + 2 bytes sentinel = 1004
    assert_eq!(buf.len(), 1004);
}

// -----------------------------------------------------------------------
// Window-mode framing tests
// -----------------------------------------------------------------------

#[test]
fn test_window_frame_roundtrip() {
    let packet = vec![1, 2, 3, 4, 5];
    let symbol = frame_window_packet(&packet, 64);
    assert_eq!(symbol.len(), 64);

    let extracted = extract_window_packet(&symbol).unwrap();
    assert_eq!(extracted, packet);
}

#[test]
fn test_window_frame_empty_packet() {
    // Zero-length packet should yield None on extraction (sentinel-like)
    let symbol = frame_window_packet(&[], 64);
    assert_eq!(symbol.len(), 64);
    assert!(extract_window_packet(&symbol).is_none());
}

#[test]
fn test_window_frame_max_payload() {
    // Packet fills entire symbol (minus 2-byte length prefix)
    let packet = vec![0xAB; 62]; // 64 - 2
    let symbol = frame_window_packet(&packet, 64);
    let extracted = extract_window_packet(&symbol).unwrap();
    assert_eq!(extracted, packet);
}

#[test]
fn test_window_frame_oversized_packet_truncated() {
    // Packet larger than symbol capacity — silently truncated
    let packet = vec![0xFF; 200];
    let symbol = frame_window_packet(&packet, 64);
    let extracted = extract_window_packet(&symbol).unwrap();
    assert_eq!(extracted.len(), 62); // 64 - 2
}

#[test]
fn test_window_frame_too_short_symbol() {
    assert!(extract_window_packet(&[]).is_none());
    assert!(extract_window_packet(&[0]).is_none());
}

#[test]
fn test_window_frame_corrupt_length() {
    // Length field points past end of data
    let mut symbol = vec![0u8; 10];
    symbol[0..2].copy_from_slice(&100u16.to_le_bytes());
    assert!(extract_window_packet(&symbol).is_none());
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
    let extracted = extract_packets(&symbol);
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
    let extracted = extract_packets(&symbol);
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
    let extracted = extract_packets(&flushed);
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0], p1);
    assert_eq!(extracted[1], p2);

    // p3 should still be in the buffer
    let remaining = packer.flush().unwrap();
    let extracted = extract_packets(&remaining);
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
    let extracted = extract_packets(&symbol);
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
