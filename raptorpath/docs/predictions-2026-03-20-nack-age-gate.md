# Predictions: NACK Age Gate Fix — 2026-03-20

## Diagnosis

All window backends converged to exactly 19.9% overhead (399/2000) across ALL scenarios.
This equals the budget cap of 2 repairs per 10-symbol batch = 20%. The budget cap became
a floor because NACK always finds gaps and fills remaining budget.

Root cause: NACK scans `recovered` set for gaps in a 50-symbol trailing window. At DC
with 0.1% loss, most symbols arrive instantly — but there's always a 1-2 symbol timing
gap between "sent" and "recovered via reorder buffer." These timing artifacts trigger
NACK repairs every batch, consuming the full budget.

## Changes

| ID | Change | Mechanism |
|----|--------|-----------|
| 3.1 | NACK age gate: only count gaps older than 2× base_delay | Symbols that were just sent shouldn't trigger NACK. At DC (1ms delay), a gap must be >2ms old. At Satellite (100ms), >200ms old. This filters timing artifacts. |
| 3.2 | NACK gap check uses `received_set` again (revert 2.3) | `recovered` includes FEC recoveries that haven't been delivered yet — creates false negatives. `received_set` is the right source for "what actually arrived from the network." The age gate solves the spurious-gap problem instead. |
| 3.3 | Budget cap only limits NACK, doesn't fill to cap | Current: `nack_repairs = min(gaps×mult, 3, budget)`. This is correct — the budget limits, doesn't fill. But combined with age gate, fewer gaps → fewer NACK repairs. |

## Per-Cell Predictions (baseline config, 1-path)

### DC (0.1% loss, 1ms delay → age gate = 2ms)

| Backend | Metric | Before (broken) | Predicted | Reasoning |
|---------|--------|-----------------|-----------|-----------|
| RLC | overhead | 19.9% | 0-3% | Age gate filters ALL timing gaps at DC. 2ms gate means gaps from the current/previous batch are ignored. At 0.1% loss, genuine old gaps are ~0.1% of symbols. Proactive repairs ≈ 0 (rate ~0.02). NACK should fire rarely. |
| RLC | recovery | 100% | 100% | DC doesn't need NACK — loss is negligible |
| RaptorQ | overhead | 5.6% | 5.6% | Block mode unaffected |

### WiFi (2.5% loss, 5ms delay → age gate = 10ms)

| Backend | Metric | Before (broken) | Predicted | Reasoning |
|---------|--------|-----------------|-----------|-----------|
| RLC | overhead | 19.9% | 12-16% | 10ms age gate filters recent-batch gaps. At 2.5% loss, ~2.5 symbols per 100 are genuinely lost. After 10ms, proactive FEC has had time to recover some. Remaining gaps trigger NACK. Proactive ≈ 1/batch (rate ~0.08). NACK adds 0-1/batch for real gaps. Total 1-2/batch = 10-20%. |
| RLC | recovery | 100% | 100% | Was 100% even with 21% overhead. Less overhead but proactive FEC + delayed NACK should still cover. |

### LTE (3.5% loss, 20ms delay → age gate = 40ms)

| Backend | Metric | Before (broken) | Predicted | Reasoning |
|---------|--------|-----------------|-----------|-----------|
| RLC | overhead | 19.9% | 10-14% | 40ms age gate is generous (4 batch intervals). Only genuinely lost symbols older than 40ms trigger NACK. Most losses recovered by proactive FEC within 40ms. |
| RLC | recovery | 100% | 99-100% | 40ms delay before NACK gives FEC time to recover. Slight risk of late recovery for burst losses. |

### Satellite (9% loss, 100ms delay → age gate = 200ms)

| Backend | Metric | Before (broken) | Predicted | Reasoning |
|---------|--------|-----------------|-----------|-----------|
| RLC | overhead | 19.9% | 12-18% | 200ms age gate is very conservative (20 batch intervals). Most gaps younger than 200ms. Only sustained losses trigger NACK. Proactive ≈ 2/batch. NACK rare with 200ms gate. |
| RLC | recovery | 99.9% | 95-99% | Risk: 200ms age gate delays NACK significantly. Symbols lost at high loss rate may not get NACK help in time. But proactive FEC handles most losses. |
| Streaming | recovery | 97.2% | 94-98% | Same risk as RLC |

## Key Risk

The age gate of 2× base_delay might be too conservative for Satellite (200ms).
Satellite has 9% loss — waiting 200ms before NACKing means ~20 batches pass, during
which proactive FEC may not fully cover burst losses. If Satellite recovery drops
below 95%, need to use min(2× base_delay, 50ms) as the gate.

## Verification Checklist

- [ ] DC overhead < 5%? (main validation)
- [ ] WiFi overhead < 16%?
- [ ] LTE overhead < 16%?
- [ ] Satellite recovery ≥ 95%?
- [ ] No scenario at exactly 19.9% (confirms cap is no longer the floor)
- [ ] Block backends unchanged?
