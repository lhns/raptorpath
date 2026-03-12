# ADR-0001: Block Assembly Needs Flush Timeout

## Status
**Open** — must fix before any real use

## Context
The sender in `net/mod.rs` accumulates IP packets into a 64KB block buffer before FEC encoding. It only flushes when the buffer reaches `MAX_BLOCK_SIZE`. On low-traffic or bursty connections (gaming, SSH, DNS), packets can sit in the buffer indefinitely, causing unbounded latency.

A single DNS query (< 100 bytes) would never be sent until 63KB more traffic arrives.

## Problem
- No timeout on partial blocks
- No minimum flush interval
- Latency is unbounded for small/sparse traffic
- This is the single biggest latency problem in the current design

## Decision Required
Implement a dual-trigger flush strategy:

1. **Size trigger**: flush when buffer reaches `MAX_BLOCK_SIZE` (existing)
2. **Time trigger**: flush after N milliseconds since first packet in current block (new)
3. **Adaptive sizing**: for realtime traffic, use smaller blocks (e.g., 1-4KB) to minimize FEC latency at the cost of slightly higher overhead

### Suggested implementation
```rust
tokio::select! {
    packet = tun.read_packet() => { /* add to block_buf */ }
    _ = tokio::time::sleep(flush_deadline) => { /* flush partial block */ }
}
```

The timeout should be configurable (e.g., `--flush-timeout-ms`, default 5ms for realtime, 50ms for bulk).

## Consequences
- Smaller blocks = more FEC overhead per byte (fixed cost per block)
- But dramatically better latency for interactive traffic
- Need to handle minimum block size (at least 1 symbol worth of data)

## Related
- ADR-0002 (packet framing)
- ADR-0006 (protocol hint should influence block size)
