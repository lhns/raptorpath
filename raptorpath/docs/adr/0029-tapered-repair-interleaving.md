# ADR 0029: Tapered Repair Interleaving

**Status**: Implemented (Phase 1 + Phase 2)
**Date**: 2026-03-15

## Context

Repair symbols were previously sent either as a flat batch after sources (block mode)
or at a constant rate (window mode). In both cases, repairs are uniformly distributed
in time, which is suboptimal: early repairs have the highest marginal recovery value
because they protect data with zero existing coverage.

This is a known family of techniques (expanding-window codes, unequal error protection,
spatially-coupled LDPC), but raptorpath had no temporal tapering.

## Decision

### Phase 1: Block-mode tapered interleaving

When `InterleavingBuffer` depth >= 2, enable tapered mode. Block B's repairs are
interleaved with block B+1's source stream using an exponential decay schedule:

```
weight(i) = exp(-λ * i/k)     where i = source position, k = total sources
λ = 4.605 / (1 + 10 * loss_rate)
```

- At 0% loss: λ ≈ 4.6 (steep decay — concentrate repairs at start)
- At 12% loss: λ ≈ 2.1 (gentler — spread repairs further)

Repairs from the last block in a drain are held as `pending_repairs` until the
next block arrives.

### Phase 2: Window-mode repair burst

When a new source enters the sliding window, generate `ceil(loss_rate * 4)` extra
burst repairs immediately. These naturally cover the new source since it's in the
encoder's current window. At 0% loss = 0 extra repairs. At 12% = 1 extra per source.

The steady-state repair interval remains unchanged; burst repairs are additive.

## Integration

- `src/net/interleave.rs`: `compute_taper_schedule()`, `drain_tapered()`, `new_tapered()`
- `src/net/mod.rs`: `send_interleaved_batches()` computes worst-path loss rate;
  `run_window_sender()` generates burst repairs after `add_source()`
- `tests/fec_realworld_recovery_test.rs`: "Tapered vs Flat Interleaving" benchmark

## Consequences

- Block-mode interleaving now distinguishes source vs repair symbols
- `drain()` and `drain_all()` take a `loss_rate` parameter (ignored in flat mode)
- Tapered mode adds one drain of latency for the last block's repairs (held pending)
- Window burst increases instantaneous bandwidth proportional to loss rate
- Flat (non-tapered) mode is fully preserved for depth=1 or explicit `new()`

## Open Questions

1. Exponential vs linear taper — exponential is optimal for i.i.d. loss but
   Gilbert-Elliott bursts may favor a different shape
2. Pipeline stall — if next block is delayed, pending repairs are also delayed;
   may need timeout-based flush
3. Window burst budget — extra repairs may exceed congestion window; could
   borrow from future steady-state budget
