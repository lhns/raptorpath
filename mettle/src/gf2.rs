//! GF(2) packet-level XOR operations.
//!
//! METTLE operates entirely over GF(2) — all coding operations are bitwise XOR
//! between equal-length byte slices (packets). No field multiplication tables,
//! no GF(2^8) arithmetic — just XOR.

/// XOR `src` into `dst` in place. Panics if lengths differ.
pub fn xor_packets(dst: &mut [u8], src: &[u8]) {
    assert_eq!(dst.len(), src.len(), "XOR packet length mismatch");
    // Process 8 bytes at a time for better throughput
    let chunks = dst.len() / 8;
    let (dst_chunks, dst_tail) = dst.split_at_mut(chunks * 8);
    let (src_chunks, src_tail) = src.split_at(chunks * 8);

    for i in 0..chunks {
        let offset = i * 8;
        let d = u64::from_ne_bytes(dst_chunks[offset..offset + 8].try_into().unwrap());
        let s = u64::from_ne_bytes(src_chunks[offset..offset + 8].try_into().unwrap());
        dst_chunks[offset..offset + 8].copy_from_slice(&(d ^ s).to_ne_bytes());
    }
    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= s;
    }
}

/// XOR `src` into `acc`, extending `acc` with zeros if it's shorter than `src`,
/// or extending the XOR region if `src` is shorter.
pub fn xor_into(acc: &mut Vec<u8>, src: &[u8]) {
    if acc.len() < src.len() {
        acc.resize(src.len(), 0);
    }
    for (d, s) in acc.iter_mut().zip(src.iter()) {
        *d ^= s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_basic() {
        let mut dst = vec![0xFF, 0x00, 0xAA];
        let src = vec![0xFF, 0xFF, 0x55];
        xor_packets(&mut dst, &src);
        assert_eq!(dst, vec![0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn xor_self_is_zero() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut dst = data.clone();
        xor_packets(&mut dst, &data);
        assert_eq!(dst, vec![0; 10]);
    }

    #[test]
    fn xor_identity() {
        let mut dst = vec![0; 16];
        let src = vec![42; 16];
        xor_packets(&mut dst, &src);
        assert_eq!(dst, src);
    }

    #[test]
    fn xor_into_extends() {
        let mut acc = vec![0xFF, 0x00];
        let src = vec![0x01, 0x02, 0x03, 0x04];
        xor_into(&mut acc, &src);
        assert_eq!(acc, vec![0xFE, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn xor_large_packet() {
        // Test the u64-chunked path
        let mut dst = vec![0xAA; 1500];
        let src = vec![0x55; 1500];
        xor_packets(&mut dst, &src);
        assert!(dst.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn xor_empty() {
        let mut dst: Vec<u8> = vec![];
        let src: Vec<u8> = vec![];
        xor_packets(&mut dst, &src);
        assert!(dst.is_empty());
    }

    #[test]
    fn xor_odd_length() {
        // Length not divisible by 8
        let mut dst = vec![0xFF; 13];
        let src = vec![0xFF; 13];
        xor_packets(&mut dst, &src);
        assert_eq!(dst, vec![0; 13]);
    }
}
