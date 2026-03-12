# ADR-0005: No ACK/Feedback Loop Between Receiver and Sender

## Status
**Resolved** — ACKs, BlockResults, and Pong messages are now sent via `send_control_datagram()`. Uni-stream receiver added for reliable control. Echo-based RTT measurement works end-to-end.

## Context
The protocol defines `ControlMessage::Ack`, `BlockResult`, and `PathReport` messages, but none of them are ever sent. The receiver processes incoming data but never sends anything back to the sender.

## Problem
Without receiver → sender feedback:
1. **Loss estimation**: sender can't know what was lost (ADR-0003)
2. **RTT measurement**: sender can't compute round-trip time
3. **Congestion control**: sender can't detect congestion (cwnd never changes)
4. **Repair requests**: receiver can't request additional repair symbols for blocks that are close to decodable but have too many losses
5. **Path liveness**: sender can't detect if a path is dead

## Decision Required
Implement a periodic ACK/report mechanism:

### Per-batch ACK (frequent, lightweight)
After receiving each `SymbolBatch`, the receiver sends:
```rust
ControlMessage::Ack {
    block_id,
    received_ids: vec![...],       // which payload_ids were received
    recv_timestamp_us: now(),       // for sender-side RTT calculation
}
```

### Per-block result (on decode or timeout)
```rust
ControlMessage::BlockResult {
    block_id,
    success: true/false,
    symbols_received: n,
    symbols_needed: k,
}
```

### Periodic path report (every ~1s)
```rust
ControlMessage::PathReport {
    path_id,
    loss_rate: estimated,
    avg_rtt_us: measured,
    throughput_bps: measured,
}
```

### Keepalive (every ~5s if no other traffic)
```rust
ControlMessage::Ping { timestamp_us }
// Receiver responds with:
ControlMessage::Pong { echo_timestamp_us }
```

## Transport
ACKs should be sent via reliable QUIC streams (not datagrams) to ensure they arrive even under high loss.

## Consequences
- Adds ~1-5% control overhead
- Enables all adaptive mechanisms (FEC rate, congestion, scheduling)
- RTT measurement becomes accurate
- Dead path detection becomes possible

## Related
- ADR-0003 (loss estimation)
- ADR-0007 (RTT calculation)
- ADR-0009 (congestion control)
