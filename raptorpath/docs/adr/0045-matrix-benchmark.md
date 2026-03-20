# ADR-0045: Comprehensive Matrix Benchmark

## Status

Accepted

## Context

The benchmark suite output format changed multiple times across ADRs 0033–0044,
making cross-commit comparison impossible. Tables 3 (feature ablation), 4 (FEC vs
retransmit), and 5 (transport comparison) each measured overlapping dimensions with
different structures, preventing unified analysis.

We need one stable, comprehensive matrix that crosses all relevant dimensions and
produces machine-readable output (JSON) alongside human-readable markdown, with
git commit info embedded for provenance.

## Decision

### Replace Tables 3/4/5 with a single matrix

The matrix crosses four dimensions:

**Backends (6)**: RaptorQ, ReedSolomon, RLC, Mettle, Streaming, Retransmit

Retransmit is not a real FEC backend but fits on the axis as a transport baseline:
single-path uses one `ReliableSimChannel`, dual-path uses round-robin across two.

**Feature ablations (4)**: baseline, no_nack, no_reorder, no_pi

`single_path` is removed — paths is now a separate axis. Retransmit only runs
"baseline" (NACK/reorder/PI are FEC-specific).

**Paths (2)**: 1 path, 2 paths

**Scenarios (5)**:

| Scenario   | 1-path channels    | 2-path channels         |
|------------|-------------------|-------------------------|
| DC         | datacenter        | datacenter + datacenter |
| WiFi       | wifi_congested    | wifi_cong + wifi_cong   |
| LTE        | lte_congested     | lte_cong + lte_cong     |
| Satellite  | satellite         | satellite + satellite   |
| DC+LTE     | *(skip)*          | datacenter + lte_cong   |

DC+LTE only runs in 2-path mode (asymmetric scenario).

**Total**: 168 cells × 30 trials = 5,040 trial runs.

### Metrics per cell (10, stable)

throughput_mbps, recovery_rate, overhead_pct, total_repair_count,
p50_latency_ms, p95_latency_ms, p99_latency_ms, deadline_miss_pct,
in_order_rate, tail_drops

### Three trial branches

- **Window backends** (RLC, Mettle, Streaming): generalized from the old
  `run_ablation_trial`, parameterized by scenario and backend.
- **Block backends** (RaptorQ, RS): per-block encode, send individual symbols
  through SimChannel pipeline with multipath/reorder/NACK/PI at symbol level,
  decode per-block.
- **Retransmit**: `ReliableSimChannel` with round-robin for 2-path.

### Auto file output

Each run writes:
- `docs/benchmark-results-YYYY-MM-DD-HHMMSS.md` — human-readable, means only
- `docs/benchmark-results-YYYY-MM-DD-HHMMSS.json` — full stats (mean/stddev/ci95)

Both files include git commit hash and message for provenance.

### Tables 1/1b/2 unchanged

Codec recovery sweep and wire overhead breakdown remain as-is but are refactored
to return structured data for JSON output.

## Consequences

- Cross-commit comparison is now possible via JSON diffing
- Single matrix replaces three overlapping tables, reducing cognitive load
- Block backends are now tested under the full transport pipeline (previously
  only tested in the codec-only loss sweep)
- Retransmit baseline is directly comparable to all FEC backends
- NACK is a no-op for block backends (gap-based NACK requires window-mode
  sequence tracking); this is visible in the data as identical results between
  baseline and no_nack configs for RaptorQ/RS
- The `single_path` ablation config is removed; the same data is now available
  via the paths=1 dimension
- Correlated fading (from old Table 3) is removed from the matrix; it was
  specific to the WiFi+LTE scenario which no longer exists in symmetric form
