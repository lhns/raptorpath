//! Packet framing for block assembly and extraction.
//!
//! IP packets are length-prefixed before being concatenated into FEC blocks.
//! After FEC decode, the framing allows us to recover individual packet boundaries.
//!
//! Wire format per packet: [u16 BE length][packet data]
//! End-of-block sentinel:  [u16 0x0000]

/// Frame multiple packets into a block buffer with length prefixes.
/// Each packet is prefixed with its length as a big-endian u16.
pub fn frame_packet(block_buf: &mut Vec<u8>, packet: &[u8]) {
    assert!(
        packet.len() <= u16::MAX as usize,
        "packet too large to frame: {} bytes",
        packet.len()
    );
    block_buf.extend_from_slice(&(packet.len() as u16).to_be_bytes());
    block_buf.extend_from_slice(packet);
}

/// Write the end-of-block sentinel (zero-length marker).
pub fn frame_end(block_buf: &mut Vec<u8>) {
    block_buf.extend_from_slice(&0u16.to_be_bytes());
}

/// Extract individual packets from a decoded block.
/// Returns a Vec of packets. Stops at end-of-block sentinel or end of data.
pub fn extract_packets(data: &[u8]) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut cursor = 0;

    while cursor + 2 <= data.len() {
        let len = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
        cursor += 2;

        if len == 0 {
            break; // end-of-block sentinel
        }

        if cursor + len > data.len() {
            // Truncated packet — block may have been padded by FEC
            break;
        }

        packets.push(data[cursor..cursor + len].to_vec());
        cursor += len;
    }

    packets
}

#[cfg(test)]
mod tests {
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
}
