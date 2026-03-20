# ADR-0042: Consolidate Benchmarks into Actionable Suite

**Status:** Accepted
**Date:** 2026-03-19

## Context

The project had 4 integration test benchmarks (`transport_comparison_bench`,
`pipeline_ablation_bench`, `tradeoff_ablation_bench`, `ablation_bench`) that
produced hundreds of lines across dozens of tables but failed to answer basic
questions like "which backend should I use?" or "does NACK actually help?".

Root causes:

1. **64B symbol size** in transport/tradeoff benchmarks — real MTU is 1200B, so
   results were incomparable with `fec_bench` Criterion benchmarks.
2. **50% max FEC overhead** masked everything — all configs showed 100% recovery,
   ablation was meaningless.
3. **No loss sweep** — no way to see crossover points between backends.
4. **No confidence intervals** — impossible to distinguish signal from noise.
5. **Duplicated infrastructure** — `ablation_bench.rs` reimplemented its own
   `GilbertElliottChannel` instead of using `common/mod.rs`.
6. **5 overhead layers but only 1 measured** (ADR-0038) — reported "overhead" was
   just FEC repair symbols, not true wire cost.
7. **Too many tables with no narrative** — 8 transports × 6 scenarios × 8 metrics
   produced a wall of numbers.

## Decision

Replace the 4 integration test benchmarks with **one file** (`bench_suite.rs`)
that produces **4 focused tables**, each answering one clear question. Keep the 4
Criterion benchmarks unchanged (they measure encode/decode latency well).

### Shared parameters (fixing inconsistencies)

| Parameter        | Old value(s)           | New value | Rationale                           |
|------------------|------------------------|-----------|-------------------------------------|
| `SYMBOL_SIZE`    | 64 (transport/tradeoff), 1200 (ablation) | 1200 | Realistic MTU, matches `fec_bench` |
| `NUM_SYMBOLS`    | 500–2000               | 2000      | Enough for window-mode stability    |
| `BATCH_SIZE`     | 10–50                  | 10        | Matches production                  |
| `NUM_TRIALS`     | 10–100                 | 30        | Enough for 95% CI                   |
| `MAX_FEC_OVERHEAD` | 0.15–0.50            | 0.20      | Tight enough to reveal differences  |

### Table 1: Backend Loss Sweep — "Which backend should I use?"

Sweeps uniform loss from 1% to 25%. For each loss rate × backend, runs 30 trials
measuring recovery rate with 95% CI. Block backends (RaptorQ, RS) use block-mode
encode/decode with BLOCK_SIZE=200. Window backends (RLC, Mettle, Streaming) use
interleaved streaming with repair per batch.

Replaces `ablation_bench::backend_comparison_benchmark`.

### Table 2: Wire Overhead Breakdown — "How much does FEC actually cost?"

Computes all 5 overhead layers from ADR-0038 for 3 representative loss scenarios
(datacenter 0.1%, WiFi 2.5%, congested 12%). Pure arithmetic using
`FecRateController::compute_repair_rate` for layer 1 and serialization constants
for layers 2–5.

This is new — previously no benchmark measured true wire overhead.

### Table 3: Feature Ablation — "Does each feature help?"

WiFi-bursty scenario, 20% FEC budget, 30 trials. Full pipeline with scheduler,
reorder buffer, NACK repair, and multipath. Baseline with all features on, then
one-off: no_nack, no_reorder, single_path, no_pi, no_ge_burst. Reports recovery,
overhead, p99 latency, and in-order delivery with CI.

Consolidates `pipeline_ablation_bench` + feature-toggle parts of `ablation_bench`.

### Table 4: FEC vs Retransmit — "Is FEC worth the overhead?"

Two transports: `raptorpath_dual` (RLC window, dual-path) vs `retransmit_dual`
(reliable retransmit model, dual-path). Three scenarios: WiFi, LTE, Satellite.
The retransmit model is explicitly labeled as simplified — not real QUIC.

Replaces `transport_comparison_bench` — drops the 8×6 matrix to 2×3.

### Table 5: Transport Comparison — "QUIC vs MPTCP vs FEC" (added post-ADR-0043)

Five transport configurations: QUIC single-path, MPTCP round-robin, MPTCP min-RTT,
FEC single-path, FEC dual-path. Three scenarios: WiFi, LTE, Satellite.
Implements the full ADR-0036 comparison within the consolidated bench suite.

## Changes

### New
- `tests/bench_suite.rs` — single consolidated benchmark

### Modified
- `tests/common/mod.rs`:
  - Added `UniformChannel` (fixed loss rate, no GE state machine)
  - Added `TrialStats` (collects `Vec<f64>`, computes mean/stddev/ci95)
  - Added `make_wire_symbol_sized` (configurable data size)

### Deleted
- `tests/transport_comparison_bench.rs`
- `tests/pipeline_ablation_bench.rs`
- `tests/tradeoff_ablation_bench.rs`
- `tests/ablation_bench.rs`

### Unchanged
- All 4 Criterion benchmarks (`gf256_bench`, `fec_bench`, `fec_realworld_bench`,
  `mettle_bench`) — fine for timing

## Runtime

- Table 1: 5 backends × 8 loss rates × 30 trials = 1200 runs (~2–3 min)
- Table 2: Pure computation (~instant)
- Table 3: 6 configs × 30 trials = 180 runs (~1–2 min)
- Table 4: 2 transports × 3 scenarios × 30 trials = 180 runs (~1 min)
- Table 5: 5 transports × 3 scenarios × 30 trials = 450 runs (~5 min)
- **Total: ~17 min** (down from ~20 min for 4 separate benchmarks)

## Consequences

- Each table answers one question with CI — no more "wall of numbers"
- 1200B symbols make results directly comparable with Criterion benchmarks
- 20% FEC cap reveals real differences between configs
- `TrialStats` and `UniformChannel` in `common/mod.rs` are reusable by future tests
- Old ADR references (0033, 0034, 0036) still apply to the underlying features;
  this ADR replaces only the benchmark methodology
