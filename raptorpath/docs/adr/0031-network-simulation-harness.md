# ADR-0031: Network Simulation Harness

## Status
Accepted

## Context
Raptorpath has several network-dependent features (BBR/ProbeRTT, reorder buffer, NACK repair, backend auto-switching) that only had isolated unit tests. There was no test harness exercising them with realistic network behavior (delay, jitter, bursty loss, reordering). The existing `multipath_simulation_test.rs` manually feeds synthetic data but doesn't use an actual channel model.

## Decision

### Test Infrastructure (`tests/common/mod.rs`)
- **SimChannel**: deterministic network simulator using `BinaryHeap<Reverse<SimPacket>>` + `MockClock` for packet delivery with configurable delay, jitter, and Gilbert-Elliott loss
- **GilbertElliottChannel**: two-state Markov loss model (Good ↔ Bad) applied per-packet
- **Presets**: `datacenter()` (1ms, 0.1% loss), `wifi()` (5ms+3ms jitter, ~2.5% bursty), `lte()` (20ms+5ms jitter, ~3.5% bursty)
- Helper functions: `make_wire_symbol()`, `make_source_batch()`, `make_repair_batch()`

### Source Changes
- **ReorderBuffer extracted** to `src/net/reorder.rs` with all methods made `pub`
- Added `push_with_time(&mut self, seq, data, now)` for injectable timestamps (MockClock-driven tests)
- Original `push()` preserved, delegates to `push_with_time(Instant::now())`
- `compute_gap_ranges` and `MAX_NACK_GAPS` made `pub` for integration test access

### Test Files
| File | Tests | Exercises |
|------|-------|-----------|
| `sim_copa_test.rs` | 4 | Copa convergence, delay-based cwnd, wireless-vs-congestion, RTT-weighted scheduling |
| `sim_backend_test.rs` | 4 | Low→high loss switch, SimChannel-driven switch, burst→streaming, hysteresis |
| `sim_reorder_test.rs` | 4 | Jittery delivery reordering, timeout expiry, over-capacity drain, bursty gaps |
| `sim_nack_test.rs` | 4 | Gap detection accuracy, RLC repair recovery, cooldown rate limiting, gap bounds |
| `sim_pipeline_test.rs` | 3 | Full pipeline datacenter/WiFi/multipath-failover integration |

## Key Design Choices
- Test infrastructure lives in `tests/common/` (idiomatic Rust integration test helpers), not `src/sim/`
- SimChannel uses `BinaryHeap + MockClock` for deterministic, reproducible packet delivery ordering
- BackendSelector timer bypassed via `switch_interval_secs=0` (existing pattern)
- All tests are deterministic via seeded `ChaCha8Rng` + `MockClock`

## Consequences
- Network-dependent features now have realistic channel-model coverage
- ReorderBuffer is public and independently testable with injectable timestamps
- `compute_gap_ranges` is accessible from integration tests
- SimChannel presets provide consistent channel models across all test files
