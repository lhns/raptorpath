# ADR-0033: Full-Pipeline Ablation Benchmark

## Status

Resolved

## Context

The existing `ablation_bench.rs` (ADR-0032) measures FEC-only features (PI feedback, GE burst factor, realtime burst extra) in isolation using a channel model applied directly to symbol arrays. It explicitly documents five features that **cannot** be benchmarked without a full network pipeline:

1. **ProbeRTT / BBR phases** — require RTT measurements and scheduler state machine
2. **Reorder buffer** — requires out-of-order packet delivery simulation
3. **NACK repair** — requires bidirectional gap detection and targeted repair
4. **Backend auto-switching** — requires loss measurement over time to trigger heuristic
5. **Multipath scheduling** — requires heterogeneous path simulation

The SimChannel infrastructure (ADR-0031) provides deterministic delay, jitter, and Gilbert-Elliott loss simulation that makes these features testable.

## Decision

Create `pipeline_ablation_bench.rs` using one-feature-off ablation with the SimChannel pipeline.

### Configs (6)

| Config | ProbeRTT | Reorder | NACK | Auto-switch | Paths |
|--------|----------|---------|------|-------------|-------|
| baseline | on | 25ms | on | on | 2 |
| no_probe_rtt | **off** | 25ms | on | on | 2 |
| no_reorder | on | **0ms** | on | on | 2 |
| no_nack | on | 25ms | **off** | on | 2 |
| no_auto_switch | on | 25ms | on | **forced RLC** | 2 |
| single_path | on | 25ms | on | on | **1** |

### Scenarios (3)

| Scenario | Primary | Secondary | RTTs | Purpose |
|----------|---------|-----------|------|---------|
| Datacenter_Stable | Datacenter | Datacenter | 1ms/2ms | Baseline, ProbeRTT |
| WiFi_Bursty | WiFi | LTE | 5ms/20ms | NACK, reorder, multipath |
| DC_to_WiFi | DC→WiFi | LTE | 1ms→5ms/20ms | Backend auto-switching |

### Methodology

- 2000 symbols per trial, batches of 10
- 20 trials per config, averaged for statistical reliability
- Clock step: `max(rtt, 50ms)` — 200 batches × 50ms = 10s, sufficient for ProbeRTT interval
- ReorderBuffer disabled via `timeout_ms=0` (immediate expiry)
- BackendSelector hysteresis bypassed via `switch_interval_secs=0`
- Adaptive FEC rate via `FecRateController::compute_repair_rate()`
- NACK flow: `compute_gap_ranges` → targeted repair symbols

### Metrics

| Metric | Description |
|--------|-------------|
| recovery_rate | Fraction of source symbols recovered |
| overhead_pct | Repair symbols / source symbols × 100 |
| avg_cwnd | Mean congestion window over simulation |
| cwnd_stability | Coefficient of variation of cwnd |
| backend_switches | Number of backend switch events |
| probe_rtt_entries | Number of ProbeRTT phase entries |

## Consequences

- All five caveats from `ablation_bench.rs` are now covered by pipeline-level benchmarks
- The `ablation_bench.rs` caveats section now references `pipeline_ablation_bench.rs`
- Each feature's end-to-end impact is measurable via delta from baseline
- Verification checks confirm feature toggles work as expected (e.g., `no_probe_rtt` → 0 entries, `no_auto_switch` → 0 switches)
