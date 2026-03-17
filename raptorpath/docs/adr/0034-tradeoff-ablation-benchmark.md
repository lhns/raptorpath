# ADR-0034: Per-Feature Tradeoff Ablation Benchmark

## Status

Resolved

## Context

ADR-0033 introduced a full-pipeline ablation benchmark with one-feature-off analysis. While effective at confirming all features contribute to the system, **every configuration achieves 100% recovery** across all scenarios. This is because:

1. **FEC is massively over-provisioned**: `max_overhead=50%` vs channel loss of 0.1-3.5% gives 15-500x headroom
2. **No latency metric**: ProbeRTT's min_rtt freshness benefit is invisible
3. **No ordering metric**: ReorderBuffer's in-order delivery value is unmeasured
4. **No burst-stress test**: NACK's targeted burst-recovery advantage is hidden by excess proactive FEC
5. **Dual-path redundancy**: Every symbol sent on both paths masks multipath scheduling quality

Each algorithm was designed for a specific upside **beyond recovery rate**:

| Algorithm | Overhead cost | Actual upside |
|-----------|--------------|---------------|
| **ProbeRTT** | cwnd drop every 10s (~2% throughput) | Fresh min_rtt prevents queue buildup, bounds P99 latency |
| **ReorderBuffer** | Adds configurable delivery delay | In-order delivery, absorbs jitter from multipath |
| **NACK repair** | Extra repair symbols per burst event | Targeted burst-loss recovery within 1 RTT |
| **Backend auto-switch** | Possible overhead spike during transition | Optimal codec efficiency across changing loss regimes |
| **Multipath** | 2x source bandwidth (redundant send) | Latency reduction (P99), path diversity for resilience |

## Decision

Create **5 focused tradeoff benchmarks** in `tests/tradeoff_ablation_bench.rs`, one per feature, each with:

- **Tight FEC budgets** (5-15% max overhead) to make features matter
- **Feature-specific metrics** (latency, ordering, burst recovery, efficiency)
- **Parameter sweeps** to find optimal thresholds

### Test 1: ProbeRTT Tradeoff (Latency vs Throughput)

- **Scenario**: Long run with queue-buildup channel (base_delay + elapsed * growth)
- **Sweep**: ProbeRTT enabled vs disabled
- **Metrics**: min_rtt accuracy, P99 delivery latency, avg cwnd
- **Expected**: ProbeRTT costs ~2% throughput but keeps P99 latency bounded

### Test 2: ReorderBuffer Tradeoff (Ordering vs Delay)

- **Scenario**: Dual-path with asymmetric RTTs (WiFi 5ms + LTE 30ms)
- **Sweep**: `reorder_timeout_ms` = [0, 5, 10, 15, 20, 25, 35, 50]
- **Metrics**: out-of-order rate, avg delivery latency, jitter, max reorder distance
- **Expected**: Sweet spot near RTT_difference / 2 (~12-15ms)

### Test 3: NACK Repair Tradeoff (Recovery vs Bandwidth)

- **Scenario**: Deterministic burst loss (15 consecutive drops every 200 symbols)
- **Sweep**: NACK on/off x FEC budgets [5%, 8%, 12%, 20%, 50%]
- **Metrics**: recovery rate, burst recovery rate, gap close time
- **Expected**: NACK critical at <=12% budget; redundant at >=20%

### Test 4: Backend Switch Tradeoff (Efficiency vs Stability)

- **Scenario**: 5 loss phases (0.5% -> 5% -> 15% -> 5% -> 0.5%)
- **Sweep**: auto_switch vs forced_rlc vs forced_streaming; threshold pairs
- **Metrics**: per-phase overhead, switch count, recovery rate
- **Expected**: Auto-switch gets best of both backends

### Test 5: Multipath Tradeoff (Latency vs Bandwidth)

- **Scenario**: Asymmetric paths (WiFi 5ms/2% + LTE 25ms/0.5%)
- **Sweep**: single_wifi, single_lte, dual_primary_wifi, dual_redundant
- **Metrics**: P99 latency, recovery rate, overhead
- **Expected**: Redundant halves P99 at 2x bandwidth; smart scheduling gets 80% benefit at lower cost

## Consequences

- Each feature's **cost/benefit is quantified** with the right metrics
- Optimal parameter thresholds are identified through sweeps
- Results show **when each feature is worth its cost** and when it isn't
- Tight FEC budgets expose differences that were hidden by over-provisioning
- Complements ADR-0033's recovery-only ablation with multi-dimensional metrics
