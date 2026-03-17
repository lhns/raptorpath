# Ablation Benchmark Results — 2026-03-16

One-feature-off ablation study measuring the overhead cost and recovery impact
of each toggleable feature in the FEC rate controller.

## Methodology

- **Strategy**: disable one feature at a time, compare against all-features-on baseline
- **Features tested**: PI feedback loop, GE burst factor, realtime burst extra
- **Backends**: RaptorQ (block mode, k=55 symbols from 64KB), RLC (window mode, 500 symbols)
- **Scenarios**: Datacenter (0.1% loss), WiFi (2.5%), LTE (3.5%), Congested (12%)
- **Budgets**: Normal (50% max overhead), Tight (15% max overhead)
- **Trials**: 10 per cell, deterministic seeds
- **Not tested**: ProbeRTT and reorder buffer (affect latency, not FEC recovery)

## Results — Normal Budget (max_overhead=50%)

### RaptorQ Block Mode

| Scenario   | Config       | Repairs | Overhead | Delta OH | Recovery |
|------------|-------------|---------|----------|----------|----------|
| Datacenter | baseline    |      18 |   32.7%  | —        | 100%     |
|            | no_pi       |       9 |   16.4%  | -16.4pp  | 100%     |
|            | no_ge_burst |      13 |   23.6%  | -9.1pp   | 100%     |
|            | no_rt_extra |      11 |   20.0%  | -12.7pp  | 100%     |
| WiFi       | baseline    |      27 |   49.1%  | —        | 100%     |
|            | no_pi       |      20 |   36.4%  | -12.7pp  | 100%     |
|            | no_ge_burst |      19 |   34.5%  | -14.5pp  | 100%     |
|            | no_rt_extra |      22 |   40.0%  | -9.1pp   | 100%     |
| LTE        | baseline    |      27 |   49.1%  | —        | 100%     |
|            | no_pi       |      23 |   41.8%  | -7.3pp   | 100%     |
|            | no_ge_burst |      21 |   38.2%  | -10.9pp  | 100%     |
|            | no_rt_extra |      25 |   45.5%  | -3.6pp   | 100%     |
| Congested  | baseline    |      27 |   49.1%  | —        | 100%     |
|            | no_pi       |      27 |   49.1%  | +0.0pp   | 100%     |
|            | no_ge_burst |      27 |   49.1%  | +0.0pp   | 100%     |
|            | no_rt_extra |      27 |   49.1%  | +0.0pp   | 100%     |

### RLC Window Mode

| Scenario   | Config       | Repairs | Overhead | Delta OH | Recovery |
|------------|-------------|---------|----------|----------|----------|
| Datacenter | baseline    |      19 |    3.8%  | —        | 100%     |
|            | no_pi       |      19 |    3.8%  | +0.0pp   | 100%     |
|            | no_ge_burst |      13 |    2.6%  | -1.2pp   | 100%     |
|            | no_rt_extra |      19 |    3.8%  | +0.0pp   | 100%     |
| WiFi       | baseline    |      98 |   19.6%  | —        | 100%     |
|            | no_pi       |      98 |   19.6%  | +0.0pp   | 100%     |
|            | no_ge_burst |      62 |   12.4%  | -7.2pp   | 100%     |
|            | no_rt_extra |      98 |   19.6%  | +0.0pp   | 100%     |
| LTE        | baseline    |     121 |   24.2%  | —        | 100%     |
|            | no_pi       |     121 |   24.2%  | +0.0pp   | 100%     |
|            | no_ge_burst |      76 |   15.2%  | -9.0pp   | 100%     |
|            | no_rt_extra |     121 |   24.2%  | +0.0pp   | 100%     |
| Congested  | baseline    |     250 |   50.0%  | —        | 100%     |
|            | no_pi       |     250 |   50.0%  | +0.0pp   | 100%     |
|            | no_ge_burst |     172 |   34.4%  | -15.6pp  | 100%     |
|            | no_rt_extra |     250 |   50.0%  | +0.0pp   | 100%     |

## Results — Tight Budget (max_overhead=15%)

### RaptorQ Block Mode

| Scenario   | Config       | Repairs | Overhead | Recovery |
|------------|-------------|---------|----------|----------|
| Datacenter | tight_base  |       8 |   14.5%  | 100%     |
| WiFi       | tight_base  |       8 |   14.5%  | 100%     |
| LTE        | tight_base  |       8 |   14.5%  | 100%     |
| Congested  | tight_base  |       8 |   14.5%  | **20%**  |

All tight-budget RaptorQ configs produce identical results (8 repairs, 14.5%).
The feedforward computation exceeds 15% for all scenarios, so every config
is clamped to the same ceiling. No feature differentiation is possible.

### RLC Window Mode

| Scenario   | Config       | Repairs | Overhead | Delta OH | Recovery |
|------------|-------------|---------|----------|----------|----------|
| Datacenter | tight_base  |      19 |    3.8%  | —        | 100%     |
|            | tight_no_ge |      13 |    2.6%  | -1.2pp   | 100%     |
| WiFi       | tight_base  |      75 |   15.0%  | —        | 100%     |
|            | tight_no_ge |      62 |   12.4%  | -2.6pp   | 100%     |
| LTE        | tight_base  |      75 |   15.0%  | —        | 100%     |
| Congested  | tight_base  |      75 |   15.0%  | —        | **0%**   |

LTE and Congested are capped at 15%. Congested recovery drops to 0% — the
15% budget is insufficient for 12% stationary loss with bursty patterns.

## Interpretation

### Why recovery is 100% nearly everywhere

The FEC controller is designed to achieve near-zero residual loss. With a 50%
overhead budget and target tail loss of 1e-5, the feedforward model computes
enough repair symbols to cover even worst-case channel realizations. Recovery
at 100% is the *expected* outcome — not a sign that features are useless.

The right metric is **overhead cost**: how much bandwidth does each feature
consume? A feature that adds 15pp of overhead for zero recovery improvement
is "free insurance" today, but a bandwidth tax under tight budgets.

### Per-feature analysis

**PI feedback loop** (`no_pi`):
- Block mode: saves 7-16pp overhead. The 200-iteration stress test at 80%
  failure rate accumulates enough integral error to produce meaningful
  correction (+9 repairs in Datacenter).
- Window mode: no effect. `compute_repair_rate()` doesn't incorporate PI
  correction — the PI state only feeds into `compute_repair_count()`.
- Real-world relevance: PI matters in long sessions where loss model drifts.
  The benchmark's closed-loop warm-up doesn't capture this well.

**GE burst factor** (`no_ge_burst`):
- The most impactful feature. Saves 7-16pp overhead in bursty scenarios.
- Scales repair by `1 + ln(mean_burst_length - 1) * 0.3`.
- WiFi (burst ~2.0): ~1.0x (minimal). LTE (burst ~4.0): ~1.33x. Congested:
  even higher due to aggressive HMM training batches.
- Under tight budgets, this extra overhead gets clamped, but under normal
  budgets it's the dominant overhead contributor.
- Trade-off: it's "insurance" against correlated burst losses that i.i.d.
  models underestimate. Whether it's worth 15pp depends on SLA requirements.

**Realtime burst extra** (`no_rt_extra`):
- Block mode: saves 3-13pp overhead when estimator is in burst state.
  Adds `k * 0.10` extra repairs when `is_in_burst() == true`.
- Window mode: no effect. `compute_repair_rate()` doesn't have a burst-extra
  branch (only `compute_repair_count()` does).
- The prior benchmark showed 0pp everywhere because the estimator ended in
  non-burst state. Fixed by ending warm-up with a lossy batch.

### Congested scenario saturation

In both block and window mode, the Congested scenario hits the 50% overhead
cap for the baseline. All feature deltas read +0.0pp because every config
is clamped to the same ceiling. The features *would* differ if the cap were
higher, but 50% is already a severe bandwidth tax.

Under tight 15% budget, Congested recovery collapses: 20% (block) / 0% (window).
This confirms the controller correctly identifies that 15% FEC is insufficient
for 12% stationary loss with burst correlation, but has no room to compensate.

## Overhead Cost Summary

Bandwidth cost of each feature at normal (50%) budget:

| Feature          | Datacenter | WiFi    | LTE     | Congested |
|-----------------|------------|---------|---------|-----------|
| PI feedback      | 16.4pp     | 12.7pp  | 7.3pp   | (capped)  |
| GE burst factor  | 9.1pp      | 14.5pp  | 10.9pp  | (capped)  |
| RT burst extra   | 12.7pp     | 9.1pp   | 3.6pp   | (capped)  |
| **Total stacked**| **32.7pp** | **49.1pp**| **49.1pp**| **49.1pp** |

Note: deltas don't sum to baseline because features interact non-linearly
(e.g., GE burst multiplies the output that includes RT extra).

## Caveats

1. **ProbeRTT and reorder buffer** are not testable in FEC-only isolation.
   They require the full network pipeline with real or simulated delay.
2. **PI feedback** is designed for long sessions with model drift. A 200-iteration
   stress test reveals the mechanism but not its steady-state contribution.
3. **Window mode** doesn't exercise PI or RT-extra paths — those code paths
   only exist in `compute_repair_count()`, not `compute_repair_rate()`.
4. **10 trials** is low for precise recovery measurement at high loss rates.
   The 20% / 0% recovery figures for tight-budget Congested have high variance.

## Recommendations

1. **Consider reducing GE burst factor** from 0.3 to 0.15 for bandwidth-
   sensitive deployments. It's the largest overhead contributor and recovery
   stays at 100% without it under normal budgets.
2. **Add PI correction to `compute_repair_rate()`** so window mode also
   benefits from feedback-loop adaptation.
3. **Full pipeline ablation** needed for ProbeRTT/reorder assessment.
   Use `net::run()` with simulated 50ms RTT and 20ms jitter.
4. **Increase trials to 100+** for tight-budget scenarios where recovery
   is between 0-100% to get statistically meaningful confidence intervals.
