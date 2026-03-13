# ADR-0004: Decoder Map Grows Without Bound

## Status
**Resolved** — completed decoders removed immediately. Periodic cleanup task evicts stale decoders after `DECODER_TIMEOUT` (30s) and reports failures to FEC controller.

## Context
The receiver stores per-block decoders in `active_decoders: Arc<DashMap<u64, Decoder>>`. Entries are added when the first symbol of a new block arrives. Entries are never removed.

## Problem
1. **Successful blocks**: after decode, the entry stays in the map forever
2. **Failed blocks**: if a block never receives enough symbols, the decoder persists forever
3. Over time, memory grows linearly with the number of blocks processed

With 64KB blocks and 1 Gbps throughput, that's ~2000 blocks/sec. Each decoder holds received symbol data. After 1 hour: ~7.2M entries, potentially GBs of memory.

## Decision Required
Implement a two-phase cleanup:

### Immediate cleanup on decode
Remove the decoder from the map after successful decode:
```rust
if let Some(data) = decoder.add_symbol(symbol) {
    recv_decoders.remove(&symbol.block_id);
    // ... inject into TUN
}
```

### Timeout-based eviction for failed blocks
Spawn a periodic cleanup task:
```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let cutoff = Instant::now() - Duration::from_secs(30);
        decoders.retain(|_, decoder| decoder.created_at > cutoff);
    }
});
```

This requires adding a `created_at: Instant` field to `Decoder`.

### Feed back failures
When a block times out, call `fec_controller.feedback_update(false)` so the PI controller increases FEC rate.

## Consequences
- Bounded memory usage
- Stale blocks are detected and reported as failures
- FEC controller gets negative feedback from timed-out blocks

## Related
- ADR-0003 (loss estimation)
