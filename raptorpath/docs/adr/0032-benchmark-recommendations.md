# ADR-0032: Ablation Benchmark Recommendations

**Status**: Resolved
**Date**: 2026-03-17

## Context

The ablation benchmark (`docs/ablation-results-2026-03-16.md`) revealed three actionable findings:

1. **PI feedback unused in window mode**: `compute_repair_rate()` did not incorporate `pi_correction`, so window-mode FEC had no feedback-loop adaptation. The `no_pi` ablation row showed `+0.0pp` overhead delta, confirming PI was a no-op.

2. **GE burst factor (0.3) is the largest overhead contributor**: Disabling it saved 9–15pp bandwidth while recovery stayed at 100%. The default 0.3 is overly aggressive for non-extreme scenarios.

3. **10 trials insufficient for tight-budget measurements**: The Congested scenario under tight budget showed 20%/0% recovery with high variance, making results unreliable.

## Decision

### 1. Add PI correction to `compute_repair_rate()`

Insert `rate + self.pi_correction.max(0.0)` after the safety margin and before the protocol hint multiplier, gated on `self.enable_pi_feedback`. This mirrors the existing PI application in `compute_repair_count()` (block mode).

### 2. Reduce GE burst factor default: 0.3 → 0.15

Halving the burst factor reduces bandwidth tax in WiFi/LTE scenarios while still providing burst protection. The `ge_burst_factor` config field remains tunable; only the default changes.

Updated in:
- `FecRateController::new()` constructor
- `config.rs` doc comment and `unwrap_or()` default
- All ablation benchmark baseline configs (non-zero values)

### 3. Increase trial count: 10 → 100

`NUM_TRIALS = 100` gives meaningful confidence intervals for tight-budget scenarios. Runtime increases from seconds to tens of seconds — acceptable for a benchmark.

## Consequences

- Window-mode `no_pi` ablation should now show a non-zero overhead delta, confirming PI feedback is active.
- Default bandwidth overhead decreases in bursty scenarios without sacrificing recovery.
- Benchmark results are statistically reliable for all scenarios including tight-budget Congested.
