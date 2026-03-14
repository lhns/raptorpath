# ADR-0026: Multipath Window Scheduling

## Status

Resolved

## Context

The sliding-window sender (`run_window_sender`) used `select_window_paths()` which picked
source and repair paths based solely on **loss rate**. This was inconsistent with block mode's
`Scheduler::schedule()`, which uses **RTT** for source symbols and **goodput** for repair
symbols. The window-mode scheduler also:

- Didn't consider available cwnd capacity (could send to a congested path)
- Only used 2 of N paths — wasted available paths
- Didn't track `in_flight` properly (no per-symbol accounting)
- Had no option for redundant scheduling on latency-critical traffic

## Decision

### Per-symbol RTT/goodput-aware scheduling

Replace the single `select_window_paths()` call (which returned a fixed source/repair pair
for all symbols in a loop iteration) with per-symbol path selection using three new methods
on `Scheduler`:

| Method | Criterion | Use |
|--------|-----------|-----|
| `best_source_path()` | Lowest RTT, `available() > 0` | Source symbols |
| `best_repair_path()` | Highest goodput, `available() > 0` | Repair symbols |
| `redundant_source_path(primary)` | Lowest RTT excluding primary | Redundant source copy |

Each method checks `active && available() > 0`, matching block mode's capacity awareness.

### Redundant scheduling for Realtime

When `ProtocolHint::Realtime` and ≥2 active paths, source symbols are sent on both the
primary (lowest-RTT) path **and** a secondary path. This halves tail latency at the cost of
2× source bandwidth. Repair symbols are NOT duplicated — the redundancy targets latency, not
recovery.

The receiver's existing duplicate detection (ADR-0014) handles the duplicate symbols.

### In-flight tracking

Each send now increments `path.in_flight` for proper cwnd accounting, matching block mode
behavior.

## Consequences

- Window mode now uses the same RTT/goodput scheduling strategy as block mode
- Source symbols route to the lowest-latency path (minimizes time-to-receiver)
- Repair symbols route to the highest-goodput path (maximizes recovery probability)
- Cwnd is respected — full paths are skipped
- Realtime traffic gets redundant source scheduling for tail latency reduction
- NACK repairs also use goodput-based path selection
