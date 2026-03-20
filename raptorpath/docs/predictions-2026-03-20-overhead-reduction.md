# Iteration 2 Predictions — Pre-Benchmark

## Changes Made

| ID | Change | Mechanism |
|----|--------|-----------|
| 2.1 | Remove `.max(1)` floor on proactive repairs | Rate controller can emit 0 repairs when loss is negligible |
| 2.2 | Per-batch overhead budget cap | `max_batch_repairs = ceil(batch_size × MATRIX_FEC_OVERHEAD × 2.0)` = ceil(10 × 0.08 × 2.0) = 2 per batch. NACK budget = 2 - proactive_sent |
| 2.3 | NACK gap detection uses `recovered` instead of `received_set` | FEC-recovered symbols no longer appear as gaps → fewer spurious NACKs |
| 2.4 | Realtime hint offset 0.05 → 0.02 | Reduces additive FEC floor from 5% to 2% |
| 2.5 | NACK repairs capped by remaining budget | Prevents proactive + NACK from exceeding 2× overhead budget |

## Per-Cell Predictions (baseline config, 1-path)

### DC (0.1% loss, 1ms base delay)

| Backend | Metric | Before | Predicted | Reasoning |
|---------|--------|--------|-----------|-----------|
| RLC | recovery | 100.0% | 100.0% | DC loss is negligible; even fewer repairs suffice |
| RLC | overhead | 10.5% | 3-5% | `.max(1)` removed → 0 proactive repairs most batches. Hint 2% + small base rate ≈ 2-3%. Some batches still get 1 repair from ceiling. Budget cap (2/batch = 20%) won't bind. |
| RLC | p50 | 1.0ms | 1.0ms | No change to timing |
| RaptorQ | overhead | 7.2% | 4-6% | Block mode still uses `.max(1)` on block repair (unchanged). But hint reduction 5%→2% reduces rate. Block PI feedback unchanged. |
| Retransmit | overhead | 0.1% | 0.1% | No FEC changes affect retransmit |

**Risk**: If the rate controller returns exactly 0 for DC and we get a rare loss burst, we might see a very slight recovery dip (~99.9%). Unlikely given 0.1% loss + 2% hint floor.

### WiFi (2.5% loss, 5ms base delay, 10Mbps, cwnd-paced)

| Backend | Metric | Before | Predicted | Reasoning |
|---------|--------|--------|-----------|-----------|
| RLC | recovery | 100.0% | 99.5-100% | Budget cap limits total repairs. Some loss events that needed 3 NACK repairs now get 0-1 (budget exhausted by proactive). Slight recovery risk. |
| RLC | overhead | 21.4% | 10-14% | Budget cap: max 2 repairs/batch → max 20%. But proactive repairs already consume ~1/batch at 2.5% loss (rate ≈ 0.08-0.10), leaving 1 for NACK. Using `recovered` set eliminates spurious NACK gaps. Key question: how many REAL gaps remain after FEC recovery? |
| RLC | p50 | 9.0ms | 9.0ms | Timing unchanged |
| Mettle | recovery | 98.4% | 97-99% | Could go either way. Mettle has higher codec overhead (15%) so rate controller gives it more repairs. But budget cap limits NACKs. |
| Streaming | recovery | 99.8% | 99-100% | Similar to RLC but streaming has 0% codec overhead → lower base rate → more NACK budget |

**Risk**: WiFi recovery could drop if budget cap is too tight. The 2× multiplier (budget = 2× overhead) was a guess. If WiFi needs 3 repairs/batch to maintain 100% and we cap at 2, we'll see it.

### LTE (3.5% loss, 20ms base delay, 2Mbps, cwnd-paced)

| Backend | Metric | Before | Predicted | Reasoning |
|---------|--------|--------|-----------|-----------|
| RLC | recovery | 100.0% | 98-100% | Budget cap same as WiFi. LTE has higher loss → more gaps → more NACK pressure against cap. But now with measured RTT, rate controller is already more aggressive. |
| RLC | overhead | 21.4% | 10-16% | Budget cap at 2/batch = max 20%. `recovered` set eliminates FEC-recovered gaps. But LTE has more real gaps (3.5% loss). Proactive repairs higher (rate ≈ 0.10-0.12 at 3.5% loss). |
| RLC | p50 | 28.6ms | 28-29ms | Unchanged |
| Mettle | recovery | 98.0% | 96-99% | Mettle's 15% codec overhead means rate controller gives more proactive repairs → less NACK budget. Could hurt or help. |

**Risk**: LTE is the scenario most likely to show recovery regression from budget cap. The iteration 1 fix (measured RTT) gave us 100% by increasing proactive FEC. If budget cap reduces proactive FEC, we lose that gain. But: we removed `.max(1)`, not `.max(proactive)` — the proactive count comes from the rate controller, which already accounts for loss. The cap only limits NACK on top.

Wait — re-reading my code: the budget cap is `ceil(batch × overhead × 2.0) = ceil(10 × 0.08 × 2.0) = 2`. The proactive repair count is `ceil(batch × repair_rate)`. At 3.5% loss with 85th percentile and Realtime +2%, repair_rate ≈ 0.06-0.10, so proactive = ceil(0.6-1.0) = 1. That leaves budget 1 for NACK. Previously NACK could send up to 3. This is a meaningful reduction.

### Satellite (9% loss, 100ms base delay, no cwnd)

| Backend | Metric | Before | Predicted | Reasoning |
|---------|--------|--------|-----------|-----------|
| RLC | recovery | 100.0% | 96-100% | Highest risk scenario. 9% loss + burst means many real gaps. Budget cap of 2/batch severely limits NACK. Proactive ≈ ceil(10 × 0.12) = 2, leaving 0 for NACK. |
| RLC | overhead | 33.2% | 14-20% | Budget cap dominates. Max 2/batch = 20%. Previous 33% came from proactive (10%) + NACK (~23%). Now capped. |
| Mettle | recovery | 95.2% | 90-96% | Mettle already struggled at satellite. Less NACK could hurt. |
| RaptorQ | recovery | 92.4% | 92-93% | Block mode — NACK changes don't apply. Hint reduction might slightly change block repair count. |
| Streaming | recovery | 98.7% | 94-99% | Streaming had best window recovery on satellite. Budget cap could reduce it. |

**Risk**: Satellite is where I'm most worried. The budget cap of 2 per batch might be too tight for 9% loss. If proactive uses both slots, NACK gets zero. The old behavior sent ~6.6 repairs/batch (33.2% × 10 source / 5 ≈ 6.6). Now capped at 2. This is a 3× reduction in repair capacity.

**Possible outcome**: Satellite recovery drops significantly (to ~90-95%) while overhead drops to 16-20%. If this happens, the budget cap multiplier needs to be loss-adaptive rather than fixed at 2×.

## Summary of Predictions

| Scenario | Recovery (before→predicted) | Overhead (before→predicted) |
|----------|---------------------------|----------------------------|
| DC 1p RLC | 100% → 100% | 10.5% → 3-5% |
| WiFi 1p RLC | 100% → 99.5-100% | 21.4% → 10-14% |
| LTE 1p RLC | 100% → 98-100% | 21.4% → 10-16% |
| Satellite 1p RLC | 100% → 96-100% | 33.2% → 14-20% |

## What Could Go Wrong

1. **Budget cap too tight for satellite** — 2× overhead at 9% loss gives only 2 repairs/batch. May need loss-adaptive budget: `max(2, ceil(batch × loss_rate × safety))`.

2. **Hint reduction hurts recovery** — Going from +5% to +2% reduces the FEC floor. On scenarios where the rate controller underestimates loss (e.g., bursty loss not yet reflected in EWMA), this 3% reduction could matter.

3. **`recovered` set for NACK is too optimistic** — A symbol might be "recovered" by FEC but the decoder hasn't advanced yet, or the recovery is delayed. If the NACK was needed to trigger faster recovery, removing it could increase latency even if total recovery is unchanged.

4. **Block backends unaffected** — Changes 2.1/2.2/2.3/2.5 only touch the window trial. Block overhead (RaptorQ at 7.2%) will only see the hint reduction effect (2.4), dropping ~1-2pp.

## Verification Checklist

After benchmark completes, check:
- [ ] DC overhead < 5%? (validates 2.1 + 2.4)
- [ ] WiFi overhead < 15%? (validates 2.2 + 2.3 + 2.5)
- [ ] WiFi recovery still ≥ 99%? (validates budget cap not too tight)
- [ ] LTE recovery still ≥ 98%? (validates budget cap on congested link)
- [ ] Satellite recovery — did it drop below 95%? (budget cap risk)
- [ ] Satellite overhead < 25%? (validates budget cap)
- [ ] No regression in DC, WiFi, LTE recovery for block backends
