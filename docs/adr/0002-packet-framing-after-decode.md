# ADR-0002: Packet Framing After FEC Decode

## Status
**Resolved** — length-prefix framing implemented in `net/framing.rs` with `frame_packet()` / `extract_packets()`

## Context
The sender concatenates multiple IP packets into a single block buffer, then FEC-encodes the block. After decoding, the receiver gets the raw byte blob and injects it into the TUN interface as a single write.

## Problem
After FEC decode, there is **no way to recover individual IP packet boundaries**. The TUN device receives a concatenated blob that is not a valid IP packet. The OS will drop it or misparse it.

This is a fundamental correctness bug — the tunnel cannot work as-is.

## Decision Required
Add length-prefix framing before FEC encoding:

```
[u16 len][packet data][u16 len][packet data]...[u16 0x0000 = end]
```

### Sender (block assembly)
```rust
block_buf.extend_from_slice(&(packet.len() as u16).to_be_bytes());
block_buf.extend_from_slice(&packet);
```

### Receiver (after decode)
```rust
let mut cursor = 0;
while cursor + 2 <= data.len() {
    let len = u16::from_be_bytes(data[cursor..cursor+2].try_into()?) as usize;
    if len == 0 { break; }
    let packet = &data[cursor+2..cursor+2+len];
    tun.write_packet(Bytes::copy_from_slice(packet)).await?;
    cursor += 2 + len;
}
```

Overhead: 2 bytes per packet. For 1500-byte packets, this is 0.13% — negligible.

## Alternatives Considered
- **IP header parsing**: extract packet length from IP header. Fragile, IPv4/IPv6 specific, doesn't work for non-IP payloads.
- **One packet per block**: simple but wastes FEC overhead (one full block per packet).

## Consequences
- Small per-packet overhead (2 bytes)
- Enables correct multi-packet block assembly
- Must be implemented before anything else works end-to-end
