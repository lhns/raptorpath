# ADR-0046: NACK Congestion Awareness + Simulation Realism

## Status

Accepted

## Context

The ADR-0045 matrix benchmark (189 cells × 30 trials) exposed several issues:

1. **NACK congestion feedback loop** — On WiFi/LTE congested links, NACK detects
   gaps → generates repairs → repairs flood the bottleneck → tail-drops → more gaps
   → more NACKs. Disabling NACK *improved* recovery from 73→97% (WiFi) and 1→21%
   (LTE). This is a production bug.

2. **Simulation unfairness** — `ReliableSimChannel` had no `LinkModel` (no
   tail-drops), so the Retransmit backend never experienced congestion while FEC
   backends did. The send loop had no congestion control, blasting all symbols
   at once, causing LTE scenarios to tail-drop 100% of block backend symbols.

3. **Block latency distortion** — All source symbols were delivered at block-decode
   time, giving symbol 0 a latency equal to the full block processing time even
   though it arrived intact immediately.

4. **Retransmit ordering** — No reorder buffer meant multipath round-robin produced
   52–70% in-order delivery, unfairly penalizing Retransmit vs FEC backends.

## Decision

### 1. NACK Congestion Awareness (production fix)

Added `NackCongestionState` to `src/net/mod.rs` that tracks:
- Loss rate trend (rising = congestion, stable = wireless)
- RTT trend (rising RTT = queue buildup)
- NACK repair multiplier (1.0 normal, 0.0 fully suppressed)

Behavior:
- **Congestion detected** (both loss AND RTT rising for ≥2 consecutive periods):
  exponentially halve the repair multiplier.
- **Both stable**: linearly recover (+0.1 per period).
- **Only one rising**: hold steady (ambiguous signal).

The multiplier scales `MAX_NACK_REPAIRS_PER_NACK` (10). At multiplier=0, NACKs
are drained without sending repairs, breaking the feedback loop. The existing
`nack_auto_disable_threshold` remains as a static upper bound.

### 2a. LinkModel for ReliableSimChannel

Added `link: Option<LinkModel>` to `ReliableSimChannel` with congested presets
matching `SimChannel`:
- `wifi_congested()`: 10 Mbps, 20-pkt buffer
- `lte_congested()`: 2 Mbps, 10-pkt buffer

Tail-drops in the link model trigger retransmission (same as GE loss), adding
realistic congestion behavior to the Retransmit baseline.

### 2b. Cwnd-based pacing

Added BDP-derived cwnd to all three trial branches. Before each symbol send,
the bench checks `in_flight_count() < cwnd`. If full, the clock advances ticks
until deliveries free capacity.

Cwnd derivation: `capacity_bps × base_delay / symbol_wire_size`:
- WiFi: ~5 symbols (10 Mbps, 5ms)
- LTE: ~4 symbols (2 Mbps, 20ms)
- DC/Satellite: no pacing (no link model)

### 2c. Scheduler/BBR integration

Window trial now creates a `Scheduler` with paths, feeds delivery events via
`scheduler.ack(path_id, count)` and `path.record_rtt_sample()`, and uses
`scheduler.best_source_path()` for path selection. Loss events are fed back
via `scheduler.on_loss()`.

### 2d. Block backend early source delivery

`process_block_deliveries!` now tracks per-block source symbol arrivals.
Intact source symbols are delivered immediately upon arrival. On block decode,
only previously-missing (FEC-recovered) symbols are delivered. Expected impact:
DC p50 latency drops from ~92ms to ~2ms.

### 2e. Retransmit reorder buffer

Added `ReorderBuffer` (25ms timeout, 500 capacity) to the retransmit trial.
Multipath round-robin delivery now passes through reorder buffering before
recording delivery order, matching FEC backend behavior.

## Key Files

- `src/net/mod.rs` — `NackCongestionState`, NACK handler integration
- `tests/common/mod.rs` — `ReliableSimChannel` with `LinkModel`, congested presets
- `tests/bench_suite.rs` — cwnd pacing, scheduler integration, early source
  delivery, retransmit reorder buffer

## Expected Impact

| Metric | Before | After |
|--------|--------|-------|
| WiFi NACK overhead | ~39% | ~10-15% (congestion backoff) |
| LTE block recovery | 0-1% | >0% (paced sending avoids queue overflow) |
| Block DC p50 latency | ~92ms | ~2ms (early source delivery) |
| Retransmit tail-drops | 0 (unrealistic) | realistic congestion |
| Retransmit in-order | 52-70% | >90% (reorder buffer) |

## Consequences

- NACK repairs are no longer fire-and-forget; congestion signals modulate output.
- Benchmark results are no longer comparable to pre-ADR-0046 runs (different
  simulation model). All comparisons should use post-0046 results.
- The `NackCongestionState` adds ~100 bytes of state per window sender connection.
