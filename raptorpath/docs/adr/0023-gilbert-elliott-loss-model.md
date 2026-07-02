# ADR-0023: Gilbert-Elliott HMM Loss Model

## Status: Resolved (burst multiplier superseded)

> **Note (2026-07):** The `GilbertElliottEstimator` itself remains in use, but
> the multiplicative `burst_factor = 1 + ln(mean_burst_length − 1) × 0.3`
> described below was removed by ADR-0043 (replaced with the additive B/T
> burst term) and the controller was redesigned again in ADR-0050
> (BOCD quantile + r* with σ²_burst = 1 + 2(1−p−q)/(p+q)). See those ADRs
> for the current burst handling.

## Context

Raptorpath's FEC rate controller uses an i.i.d. (independent and identically distributed)
loss model — it assumes each symbol is lost independently with probability p. This works
well for random packet drops but significantly underestimates FEC needs on wireless
channels where losses are **bursty**: a fade or interference event causes many consecutive
losses (a "burst"), then the channel recovers.

The Gilbert-Elliott model captures this with a two-state hidden Markov model:
- **Good state**: low loss probability (channel is healthy)
- **Bad state**: high loss probability (burst in progress)
- Transitions between states are governed by probabilities p_gb and p_bg

The mean burst length (1/p_bg) tells us how many consecutive symbols a typical burst
destroys. When this exceeds 2, the i.i.d. model's repair count is insufficient — we
need extra repair symbols to survive correlated losses.

## Decision

Add a `GilbertElliottEstimator` that:

1. Tracks transition counts between Good and Bad states with exponential decay (0.999)
2. Estimates p_gb (enter burst) and p_bg (exit burst) from transition frequencies
3. Computes `mean_burst_length = 1/p_bg`
4. Is embedded in `LossEstimator` and fed from `record_batch()` — each batch is
   approximated as `lost` Bad symbols followed by `received` Good symbols

The `FecRateController` uses the GE estimator as a multiplier:
```
if mean_burst_length > 2.0:
    burst_factor = 1.0 + ln(mean_burst_length - 1) × 0.3
    repair_count *= burst_factor
```

This logarithmic scaling adds ~20-40% extra repair for typical wireless burst lengths
(5-10 symbols) without over-provisioning for very long bursts.

## Consequences

- Bursty wireless channels get more FEC protection, reducing decode failures
- Clean channels (burst_length ≈ 1) see no change in FEC overhead
- The estimator requires ~30 transitions before producing valid estimates (min_samples)
- Exponential decay (0.999) means the model adapts to changing channel conditions
  within a few hundred symbols
