# ADR-0037: NACK Source Retransmit, Cross-Path Repair, Fractional Repair Accumulator

## Status: Accepted

## Context

The NACK repair path (ADR-0025) generates random repair symbols in response to
receiver gap reports. This works but is suboptimal in three ways:

1. **Repair vs. retransmit**: When the sender still has the exact source symbol in
   its window, generating a random repair symbol is wasteful. A direct retransmit of
   the source symbol is cheaper to decode (no GF(256) arithmetic needed) and
   guarantees the receiver fills the exact gap.

2. **Same-path retransmit**: Retransmitted symbols are sent on any available path,
   which may be the same path that lost the original. If that path is experiencing
   congestion or loss, the retransmit is likely to be lost again.

3. **Repair rate granularity**: The proactive repair system uses two mechanisms that
   interact poorly. Burst repairs (`ceil(loss_rate * BURST_FACTOR)` per source symbol)
   over-produce at low loss rates due to `ceil()` rounding — at 1% loss the ceil
   rounds up to 1 repair per source, a 100x overestimate. Interval repairs (one repair
   per N sources) are too coarse-grained for high loss rates. Neither mechanism
   accounts for ACK feedback reducing the need for further repair.

## Decision

### 1. NACK source retransmission

Add `get_source(seq: u64) -> Option<Vec<u8>>` to `WindowEncoder` trait and its
implementations (RLC, Mettle, streaming wrapper). When the sender handles a NACK:

- Call `get_source(seq)` for each gap sequence number.
- If `Some(data)` is returned, retransmit the exact source symbol.
- If `None` is returned (source evicted from window), fall back to
  `generate_repair()` as before.

### 2. Cross-path retransmission

Add `best_repair_path_avoiding(avoid: PathId) -> PathId` to the scheduler. The sender
tracks which path originally carried each source symbol in a `source_path_map`. When
retransmitting a NACK'd symbol:

- Look up the original path from `source_path_map`.
- Call `best_repair_path_avoiding(original_path)` to select a different path.
- If only one path exists, fall back to that path (no alternative available).

### 3. Fractional repair accumulator

Replace both burst repairs and interval repairs with a single `repair_debt: f64`
accumulator:

- Each source symbol adds `loss_rate * REPAIR_FACTOR` (4.0) to `repair_debt`.
- When `repair_debt >= 1.0`, emit one repair symbol and subtract 1.0.
- ACK feedback and NACK-triggered retransmissions reduce `repair_debt`
  proportionally, avoiding redundant proactive repair for already-recovered gaps.

At 1% loss this produces `0.01 * 4.0 = 0.04` debt per source, so one repair every
25 source symbols — matching the actual need without ceil() waste. At 10% loss it
produces `0.10 * 4.0 = 0.40` debt per source, one repair every 2.5 source symbols.
The accumulator naturally scales across all loss levels.

## Files Changed

- `raptorpath/src/fec/window_traits.rs` — added `get_source()` default method
- `raptorpath/src/fec/rlc_window.rs` — implemented `get_source()`
- `raptorpath/src/fec/mettle_window.rs` — implemented `get_source()`
- `streaming-codes/src/encoder.rs` — added `get_source()` accessor
- `raptorpath/src/fec/streaming.rs` — implemented `get_source()` via core
- `raptorpath/src/scheduler/mod.rs` — added `best_repair_path_avoiding()`
- `raptorpath/src/net/mod.rs` — fractional repair accumulator, NACK handler rewrite, source_path_map, cross-path selection

## Consequences

- NACK'd symbols are recovered with exact source retransmits when possible, avoiding
  decode overhead and guaranteeing gap fill
- Retransmits avoid the lossy path, increasing the probability of successful delivery
- Proactive repair rate matches actual loss without ceil() rounding waste, reducing
  unnecessary bandwidth consumption at low loss rates
- ACK/NACK feedback closes the loop on repair debt, preventing repair pile-up after
  transient losses resolve
- Falls back gracefully: evicted sources fall back to random repair, single-path
  setups fall back to the only available path
