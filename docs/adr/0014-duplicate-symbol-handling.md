# ADR-0014: No Duplicate Symbol Detection

## Status
**Resolved** — `Decoder` now tracks `seen_ids: HashSet<u32>` and skips duplicates.

## Context
With multipath transport, the same symbol could potentially arrive twice (retransmission, routing anomaly, or implementation bug). The `Decoder::add_symbol()` method feeds every received symbol into the raptorq decoder without deduplication.

## Problem
- Source symbols: tracked in `received_source` vec, duplicates overwrite (harmless but wasteful)
- Repair symbols: fed directly to `rq_decoder.decode()`. RaptorQ handles duplicates gracefully (they don't help decoding but don't corrupt), so this is not a correctness bug
- However, duplicate symbols inflate the `total_fed` counter and could affect loss statistics

## Decision Required
Add a lightweight `HashSet<u32>` per decoder to track seen `payload_id`s:
```rust
if !self.seen_ids.insert(symbol.payload_id) {
    return None; // duplicate, skip
}
```

Cost: ~4 bytes per symbol in a HashSet. For 64KB blocks with 1200-byte symbols, that's ~55 entries — negligible.

## Consequences
- Accurate symbol counting
- Slightly more accurate loss estimation
- Negligible overhead
