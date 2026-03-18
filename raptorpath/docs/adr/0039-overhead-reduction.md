# ADR-0039: Overhead Reduction — Benchmark Fix and Symbol Packing

**Status:** Accepted
**Date:** 2026-03-18

## Context

ADR-0038 identified five layers of overhead in the raptorpath protocol. This ADR addresses two concrete reductions:

1. A benchmark bug that inflates measured overhead by forcing at least 1 repair per batch
2. Window-mode symbol padding waste for small packets

## Decisions

### Decision 1: Fix Benchmark Repair Floor

**Problem:** `transport_comparison_bench.rs` lines 655 and 811 computed repair count as:
```rust
let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).max(1).min(10);
```
The `.max(1)` forced at least 1 repair per 10-symbol batch even when `repair_rate` was 0, creating a 10% overhead floor in zero-loss scenarios. The production sender (`run_window_sender`) uses a fractional repair accumulator (ADR-0037) that correctly produces zero repair at zero loss.

**Fix:** Remove `.max(1)` from both lines:
```rust
let repair_count = ((this_batch as f64 * repair_rate).ceil() as u32).min(10);
```

**Expected impact:** `dc_low_loss` overhead drops from ~38% toward 0%. Other scenarios drop 5-15 percentage points.

### Decision 2: Window-Mode Symbol Packing (SymbolPacker)

**Problem:** `frame_window_packet()` maps 1 IP packet → 1 FEC symbol, padding with zeros to `symbol_size`. For small packets this wastes most of the symbol:

| Packet type | Typical size | In 512B symbol | Utilization |
|-------------|-------------|-----------------|-------------|
| VoIP (G.711 20ms) | 160B | 162B used / 512B | 31% |
| DNS query | 60B | 62B used / 512B | 12% |
| TCP ACK | 52B | 54B used / 512B | 10% |

**Solution:** `SymbolPacker` accumulates multiple small packets into one symbol using block-mode length-prefix framing:
```
[u16 BE len1][pkt1][u16 BE len2][pkt2]...[u16 0x0000 sentinel][zero padding to symbol_size]
```

This reuses the existing `extract_packets()` function — no new parser needed on the receiver side.

**API:**
- `push(packet) -> Option<Vec<u8>>` — append packet; returns packed symbol if buffer would overflow
- `flush() -> Option<Vec<u8>>` — force-emit partial buffer (pad + return)
- `should_flush() -> bool` — true if `flush_timeout` elapsed since last push
- `is_pending() -> bool` — true if buffer contains data
- `time_until_flush() -> Duration` — time until flush timeout expires

**Sender integration:** `run_window_sender()` uses `SymbolPacker` when `protocol_hint == Realtime`. A flush timer arm in `tokio::select!` ensures partial buffers are emitted within `flush_timeout` (default 1ms).

**Receiver integration:** When `WindowStart { packed: true }` is received, the receiver uses `extract_packets()` (block-mode framing with BE u16) instead of `extract_window_packet()` (single-packet LE u16 framing) to recover individual packets from each symbol.

**Protocol negotiation:** `packed: bool` field added to `ControlMessage::WindowStart`. Old peers that don't send this field will have it default to `false` (unpacked).

**Endianness note:** `frame_window_packet` uses LE u16 for the single-packet length prefix; `frame_packet`/`extract_packets` uses BE u16. Packed symbols use the BE path to match `extract_packets()`. The `packed` flag in `WindowStart` tells the receiver which extraction path to use.

**Trade-offs:**
- 2-3x better symbol utilization for small packets (3 VoIP packets per symbol instead of 1)
- Fewer source symbols → fewer repair symbols → less FEC overhead
- Added latency bounded by `flush_timeout` (default 1ms, configurable)
- Losing 1 packed symbol loses 2-3 packets instead of 1 (but fewer total symbols at risk, so net loss probability is lower)

### Decision 3: Per-Symbol Metadata Compression (Future — Not Implemented)

`WireSymbol` serialization could be optimized:
- Use varint encoding for `block_id` and `payload_id` instead of fixed u64/u32
- Drop redundant fields in window mode (`block_id` is the sequence number)
- Estimated savings: ~2% of wire bandwidth

This is documented for future consideration but not implemented in this change.

## Files Changed

- `raptorpath/tests/transport_comparison_bench.rs` — removed `.max(1)` on lines 655 and 811
- `raptorpath/src/net/framing.rs` — added `SymbolPacker` struct with `push()`, `flush()`, `should_flush()`, `is_pending()`, `time_until_flush()` methods and tests
- `raptorpath/src/net/mod.rs` — integrated `SymbolPacker` in `run_window_sender()` with flush timer; updated receiver extraction to use `extract_packets()` when packed
- `raptorpath/src/transport/protocol.rs` — added `packed: bool` to `WindowStart` variant

## Consequences

- Benchmark overhead numbers more accurately reflect actual FEC repair costs
- VoIP and other small-packet workloads see 2-3x improvement in symbol utilization, reducing both padding waste and FEC overhead (fewer symbols need fewer repairs)
- The 1ms flush timeout adds negligible latency (well within VoIP jitter budget of 20ms)
- Receiver automatically adapts extraction method based on the `packed` flag in `WindowStart`
- Future metadata compression can provide an additional ~2% improvement
