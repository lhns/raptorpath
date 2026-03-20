# Predictions: Fractional Repair Accumulator — 2026-03-20

## Diagnosis

DC overhead stuck at 10.5% despite all NACK fixes. Root cause: `ceil(batch × rate)`
rounds up to 1 even when rate is tiny. At DC (rate ≈ 0.02-0.03), `ceil(10 × 0.025) = 1`,
giving 1 repair per 10-symbol batch = 10% overhead.

WiFi/LTE overhead still 16-17% — same issue: proactive repair rounds up, plus NACK
adds on top.

## Change

| ID | Change | Mechanism |
|----|--------|-----------|
| 4.1 | Fractional repair accumulator | Track `repair_debt: f64` across batches. Each batch: `debt += batch × rate`. Send `floor(debt)` repairs, subtract sent from debt. At DC: debt grows 0.025/batch, sends 1 repair every ~40 batches = 0.5% overhead from proactive. |

This is the same pattern as the production code (`repair_debt` in `net/mod.rs:1294`).

## Per-Cell Predictions (baseline config, 1-path)

### DC (rate ≈ 0.025 after hint)

- Proactive: 1 repair per ~40 batches = **0.5% overhead**
- NACK: age-gated, rarely fires at 0.1% loss = **~0.5%**
- Total predicted: **1-2% overhead**
- Recovery: **100%** (unchanged)

### WiFi (rate ≈ 0.08-0.10)

- Proactive: debt accumulates 0.8-1.0/batch → ~1 repair/batch on average, but some
  batches get 0 = **8-10% overhead** from proactive
- NACK: age-gated, adds 0-1 for genuine gaps = **2-4%**
- Total predicted: **10-14% overhead**
- Recovery: **100%**

### LTE (rate ≈ 0.08-0.12)

- Proactive: ~1 repair/batch average = **8-12%**
- NACK: age-gated with 40ms delay, adds 0-1 = **2-4%**
- Total predicted: **10-14% overhead**
- Recovery: **99-100%**

### Satellite (rate ≈ 0.12-0.15)

- Proactive: ~1.2-1.5/batch → alternating 1-2 repairs = **12-15%**
- NACK: age-gated with 200ms, adds 0-1 = **1-3%**
- Total predicted: **14-18% overhead**
- Recovery: **96-100%** (similar to current — age gate unchanged)

## Summary

| Scenario | Before | Predicted | Change driver |
|----------|--------|-----------|---------------|
| DC 1p RLC | 10.5% | 1-2% | Fractional accumulator eliminates ceil() rounding |
| WiFi 1p RLC | 17.0% | 10-14% | Fractional reduces proactive from 1/batch to 0.8/batch |
| LTE 1p RLC | 16.3% | 10-14% | Same mechanism |
| Satellite 1p RLC | 18.8% | 14-18% | Smaller effect — rate already ≥ 1/batch |

## Risk

Low risk. Fractional accumulation is a standard technique (already in production code).
The only risk is if the debt accumulates and sends a burst of 2-3 repairs at once,
which could cause momentary congestion. But the budget cap (2/batch) prevents this.
