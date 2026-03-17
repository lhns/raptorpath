# ADR-0035: Algorithm Recommendations and Metric Architecture Review

## Status

Resolved

## Context

Three benchmark suites now provide comprehensive data on every major algorithm in the pipeline:

1. **FEC-only ablation** (2026-03-16) — isolated PI feedback, GE burst factor, and RT burst extra in block vs window mode
2. **Pipeline ablation** (ADR-0033) — one-feature-off analysis across the full stack
3. **Tradeoff ablation** (ADR-0034) — per-feature cost/benefit with tight budgets and feature-specific metrics

After fixing three weak tests (ProbeRTT min_rtt_stamp refresh, NACK deterministic burst injection, Backend Switch multi-phase loss), all 5 tradeoff ablation tests produce strong, differentiable signals. This ADR consolidates findings into actionable recommendations and identifies a metric architecture gap.

## Consolidated Findings

### ProbeRTT (Tradeoff Ablation Test 1)

- **cwnd**: 23.9 (ON) vs 56.3 (OFF) — 2.4x reduction
- **min_rtt accuracy**: 1.1x baseline (ON) vs 10.9x baseline (OFF) — 10x more accurate
- **Cost**: ~2% throughput (periodic drain to cwnd=4 for 200ms every 10s)

The min_rtt_stamp refresh bug was also fixed — previously it refreshed on every sample, so ProbeRTT's "is min_rtt stale?" check never triggered. Now it only refreshes when a genuinely new minimum is observed.

### ReorderBuffer (Tradeoff Ablation Test 2)

- **OOO rate**: 1.4% at t=0ms → 0.0% at t=25ms
- **Latency cost**: 6.2ms at t=0ms → 6.9ms at t=25ms (+0.7ms)
- Sweet spot near 20-25ms for dual-path with asymmetric RTTs

### NACK Repair (Tradeoff Ablation Test 3)

- **At 5% FEC budget**: burst recovery 100% (NACK) vs 44.7% (no NACK) — 2.2x improvement
- **At 12%+ budget**: both achieve 100% — NACK adds no value when proactive FEC is sufficient
- NACK is most valuable at tight budgets (≤12%) and pure overhead at generous budgets (≥20%)

### Backend Auto-Switch (Tradeoff Ablation Test 4)

- **auto_switch**: 19.7% overhead vs **forced_streaming**: 29.5% — saves 10 percentage points
- Forced modes show 0 switches (expected); auto shows 4 transitions across 5 loss phases
- RLC is more efficient below ~5% loss; streaming codes are better above ~10%

### Multipath Scheduling (Tradeoff Ablation Test 5)

- **P99 latency**: 72ms (single WiFi) → 30ms (dual redundant) — 58% reduction
- **Jitter**: 12.1ms → 0.1ms — virtually eliminated
- dual_primary_wifi (P99=57ms) is a good middle ground with no overhead increase

### FEC Rate Features (Pipeline Ablation, 2026-03-16)

- **PI feedback**: Saves 7-16pp overhead in block mode. **No effect in window mode** — `compute_repair_rate()` doesn't use `pi_correction` in the window path. This is a bug.
- **GE burst factor**: Most impactful single feature. Saves 7-16pp under normal budgets. Also the biggest overhead contributor when over-tuned.
- **Realtime burst extra**: Saves 3-13pp in block mode. **No effect in window mode** — same gap as PI feedback.

## Decision

### Recommendation Summary

| Feature | Verdict | Action | Priority |
|---------|---------|--------|----------|
| ProbeRTT | **KEEP** | Enabled by default, no tuning needed | — |
| ReorderBuffer | **KEEP** | Default 20ms; set 0 for single-path | Low |
| NACK Repair | **KEEP** | Auto-disable when `max_overhead ≥ 0.20` | Medium |
| Backend Auto-Switch | **KEEP** | Current thresholds (1%/10%) work well | — |
| Multipath Scheduling | **KEEP** | dual_primary_wifi as default multi-path mode | — |
| PI Feedback | **FIX** | Port to `compute_repair_rate()` for window mode | High |
| GE Burst Factor | **TUNE** | Reduce default from 0.15 → 0.10 | Medium |
| RT Burst Extra | **FIX** | Port to `compute_repair_rate()` for window mode | Medium |

### Detailed Recommendations

**ProbeRTT** — Keep enabled. Prevents BDP overestimation under queue buildup. The 2% throughput cost is negligible compared to the 10x min_rtt accuracy improvement. Essential for any path with bufferbloat.

**ReorderBuffer** — Keep at 20ms default for multipath deployments. Disable (timeout=0) for single-path where out-of-order delivery is irrelevant. The 0.7ms latency cost at 25ms timeout is acceptable.

**NACK Repair** — Keep enabled but make budget-adaptive. At tight budgets (≤12%), NACK provides 2.2x burst recovery improvement. At generous budgets (≥20%), proactive FEC already covers all bursts and NACK is pure overhead. Implement: auto-disable NACK when `max_overhead ≥ 0.20`.

**Backend Auto-Switch** — Keep enabled. Saves ~10pp overhead by using RLC in low-loss phases instead of always paying the streaming codes premium. The 1%/10% switch thresholds produce 4 clean transitions across 5 loss phases with no oscillation.

**Multipath Scheduling** — Keep dual_primary_wifi as the default multi-path mode. It gets most of dual_redundant's latency benefit (P99=57ms vs 30ms) without the 2x bandwidth overhead. Reserve dual_redundant for ultra-low-latency requirements.

**PI Feedback** — Fix the window-mode gap. Currently `compute_repair_rate()` doesn't apply `pi_correction` in window mode because `feedback_update(block_succeeded)` is never called outside block mode. This means window mode has no closed-loop FEC rate control. High priority.

**GE Burst Factor** — Tune down from 0.15 to 0.10. The burst factor is the single most impactful overhead contributor. At 0.15 it over-provisions by 7-16pp in bursty scenarios. Reducing to 0.10 still covers burst correlation while saving ~5pp. Consider making it adaptive based on actual burst recovery success rate.

**RT Burst Extra** — Fix the window-mode gap (same issue as PI feedback). The realtime burst extra logic exists only in the block-mode path. Port to `compute_repair_rate()` for window mode parity. Medium priority since the savings (3-13pp) are smaller than PI feedback.

## Metric Architecture Gap

### Current Architecture

```
Channel loss → LossEstimator.loss_rate_upper(0.95) → FecRateController.compute_repair_rate()
                                                        ↓
                                            repair symbols sent
                                                        ↓
                              feedback_update(block_succeeded: bool) → PI correction (binary!)
```

### Problems

1. **PI feedback is binary** — it only knows "block decoded" or "block failed." It doesn't know if 10 repairs were sent and only 3 needed, or 10 sent and 9 needed. Both cases look like success.
2. **No repair efficiency metric** — decoders don't expose how many repair symbols were actually useful for recovery.
3. **Window mode has no PI feedback** — `compute_repair_rate()` recently gained PI correction, but the PI state only updates from `feedback_update(block_succeeded)` which is never called in window mode.
4. **Overspending is invisible** — at normal budgets, all scenarios show 100% recovery. There is no visibility into whether we are 2x or 10x overprovisioned.

### Proposed Fix: Repair Efficiency Metric

The right metric for FEC effectiveness is **repair efficiency** = `repairs_used / repairs_sent`:

- efficiency → 1.0: every repair was needed (tight budget, at risk of failure)
- efficiency → 0.0: massive overspend (wasting bandwidth)
- Sweet spot: ~0.3-0.5 (comfortable margin without waste)

**A. Add `repairs_consumed` tracking to decoders:**

In `RlcWindowDecoder` and `StreamingDecoder`, track how many repair symbols actually contributed to Gaussian elimination cascades (pivots that unlocked new source symbol recoveries). Expose via `WindowDecoder::repair_efficiency() -> f64`.

Implementation approach for `RlcWindowDecoder`:
- Already tracks `total_fed` (all symbols fed to decoder)
- Already distinguishes repair vs source symbols
- When a repair symbol triggers a cascade in `try_recover()`, increment `useful_repairs`
- `repair_efficiency = useful_repairs / total_repair_fed`

**B. Efficiency-based PI feedback for window mode:**

Replace binary feedback with richer signal:
```rust
fn feedback_update_rich(&mut self, repairs_sent: u32, repairs_useful: u32, recovered: bool)
```

The PI controller can then target `repair_efficiency ≈ 0.4` rather than `block_failure_rate ≈ 0`. This gives proportional control instead of bang-bang.

**C. Add repair efficiency to benchmark output:**

Add `repair_efficiency: f64` to `TradeoffResult` and `PipelineResult`. Report alongside `overhead_pct`. This gives visibility into overspending that current metrics miss.

### Scope

This ADR documents the metric gap and recommends the fix. Implementation of repair efficiency tracking is a separate work item. All existing benchmarks would benefit from the new metric — particularly the NACK and backend switch tradeoff tests where 100% recovery masks efficiency differences.

## Consequences

- All 8 major algorithms now have data-backed keep/tune/fix verdicts
- Three high/medium priority implementation items identified: PI window-mode fix, GE burst factor tuning, RT burst extra window-mode port
- Metric architecture gap documented with concrete fix proposal (repair efficiency)
- Future benchmark runs can validate tuning changes against these baseline numbers
- Budget-adaptive NACK provides a concrete optimization path for bandwidth-constrained deployments
