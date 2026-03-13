# ADR-0016: Block Interleaving

## Status: Resolved

## Context

A burst loss event of duration D destroys all symbols in flight during that period.
Without interleaving, those symbols all belong to the same block, potentially making
it undecodable even with FEC. With interleaving across N blocks, the same burst
destroys at most 1/N of each block's symbols, spreading the damage.

## Decision

Insert an interleaving buffer between the scheduler (which assigns symbols to paths)
and the transport (which sends them). Symbols from up to `depth` blocks accumulate,
then drain in round-robin order across blocks.

### Design

- **Buffer location**: `src/net/interleave.rs`, sits between `scheduler.schedule()` output and `transport.send_symbols()`
- **Round-robin**: for each path, emit one symbol per block in rotation
- **Depth**: configurable via `--interleave-depth` (CLI) or `interleave_depth` (TOML)
  - Realtime: default 2 (minimize added latency)
  - Bulk: default 4 (maximize burst protection)
  - Auto: default 3
- **Drain triggers** (whichever fires first):
  1. Depth reached: `slots.len() >= depth`
  2. Timeout: oldest slot exceeds `2 * flush_timeout`
  3. Buffer size: total buffered symbols >= 1024
- **Depth 1 = disabled**: backward-compatible passthrough

### Latency impact

Interleaving adds at most `(depth - 1) * block_flush_timeout`:
- Auto (depth 3, 10ms flush): worst case +20ms
- Realtime (depth 2, 2ms flush): worst case +2ms

### Receiver changes

None. Symbols carry `block_id`; the receiver already dispatches to the correct decoder.

## Alternatives Considered

1. **Symbol-level interleaving within a block**: doesn't help against burst loss
   since all symbols belong to the same block anyway.
2. **Time-based interleaving with fixed window**: harder to reason about, depth-based
   is more predictable.

## Consequences

- Burst loss resilience improves proportionally to depth
- Small latency increase bounded by `(depth-1) * flush_timeout`
- Memory overhead: ~500KB at depth 4 with 64KB blocks (negligible)
