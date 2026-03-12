# ADR-0007: RTT Calculation Depends on Clock Synchronization

## Status
**Open** — correctness bug

## Context
RTT is calculated in `net/mod.rs` as:
```rust
let rtt_us = now.saturating_sub(recv_timestamp_us);
```

Where `recv_timestamp_us` comes from the receiver's ACK message. This computes `sender_now - receiver_then`, which is only meaningful if both clocks are synchronized.

## Problem
- Clocks are never synchronized — no NTP guarantee
- Clock skew of even 1 second makes RTT useless
- Negative RTT is silently clamped to 0 by `saturating_sub`
- This feeds garbage into the scheduler's RTT-based path selection

## Decision Required
Use echo-based RTT measurement (standard approach):

### Sender stamps outgoing batch
```rust
SymbolBatch {
    send_timestamp_us: sender_clock(),  // already present
    ...
}
```

### Receiver echoes timestamp in ACK
```rust
ControlMessage::Ack {
    echo_send_timestamp_us: batch.send_timestamp_us,  // echo, don't use own clock
    recv_timestamp_us: receiver_clock(),                // for receiver-side metrics
}
```

### Sender computes RTT
```rust
let rtt = sender_now - ack.echo_send_timestamp_us;  // same clock!
```

This only uses the sender's clock for RTT, eliminating clock skew.

For one-way delay estimation (needed for scheduling), use QUIC's built-in RTT tracking via `quinn::Connection::rtt()`.

## Consequences
- Accurate RTT without clock sync
- Scheduler makes correct path selections
- Requires modifying ACK message format
