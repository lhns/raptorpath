# ADR-0011: TUN and Message Channels Can Stall Under Load

## Status
**Resolved** — Channel capacities increased from 256/512 to 4096. TUN inject path uses `try_send` to drop packets instead of blocking the receiver.

## Context
The system uses bounded mpsc channels at several points:
- TUN read → sender: capacity 256
- Receiver → TUN write: capacity 256
- All paths → receiver: capacity 512

## Problem
When any channel fills up:
- **TUN read channel full**: the TUN read loop blocks, OS packets queue in the kernel, eventually the TUN device drops packets with no visibility
- **TUN write channel full**: decoded blocks can't be injected, receiver stalls, symbols for new blocks pile up
- **Message channel full**: QUIC datagram receivers block, datagrams are silently dropped by QUIC

There's no backpressure signal — producers just block and consumers have no way to signal "slow down."

## Decision Required
### Short-term: increase capacities and add drop-oldest policy
For the TUN channels, use a ring buffer with drop-oldest semantics (newer packets are more valuable):
```rust
// Use tokio::sync::broadcast or a custom ring buffer
let (tx, rx) = ring_channel::<Bytes>(4096);
```

### Medium-term: propagate backpressure
- TUN write full → pause decoder → pause receivers → QUIC datagrams dropped (acceptable, FEC handles it)
- TUN read full → block the sender task → blocks stop being produced → OS buffers packets (has its own 256KB+ buffer)

### Long-term: adaptive channel sizing
Size channels based on measured throughput and RTT:
```
capacity = throughput_pps * rtt_seconds * 2
```

## Consequences
- Prevents silent packet drops at channel boundaries
- Backpressure naturally rate-limits the sender
- Ring-buffer approach trades old data for fresh data (correct for real-time)
