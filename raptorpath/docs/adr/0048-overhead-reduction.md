# ADR-0048: FEC Overhead Reduction

## Status

Accepted

## Context

After ADR-0047 fixed LTE recovery (1%→100%) and reduced WiFi overhead (39.6%→21.4%),
window backends still had excessive overhead:

- DC: 10.5% (target <4%) — `.max(1)` floor on proactive repairs guaranteed 1 repair/batch
- WiFi: 21.4% (target <15%) — NACK feedback loop from timing artifacts
- LTE: 21.4% — same issue
- Satellite: 33.2% — NACK + high proactive rate

Three iterations of benchmark-driven optimization were needed.

## Decision

### Round 1: Budget cap + hint reduction + recovered-set NACKs

- Removed `.max(1)` floor on proactive repairs
- Added per-batch overhead budget cap: `ceil(batch × overhead × 2.0)` = 2/batch
- NACK gap detection using `recovered` set instead of `received_set`
- Reduced Realtime hint offset from +5% to +2%
- Reduced Bulk hint from -5% to -2%

**Result**: All window backends converged to exactly 19.9% overhead — the budget cap
became the floor because NACK always found timing-artifact gaps and filled remaining
budget. The budget cap was correct but the gap detection was too sensitive.

### Round 2: NACK age gate

- Added age gate: only NACK symbols older than 2× `base_delay_ms`
- Reverted to `received_set` for gap detection (network arrivals are the right signal)
- Age gate filters timing artifacts (symbols in-flight or recently sent)

**Result**: DC 19.9% → 10.5%, WiFi 19.9% → 17.0%, Satellite 19.9% → 18.8%.
Improvement real but DC still at 10.5% because `ceil(batch × rate)` rounds up to 1.

### Round 3: Fractional repair accumulator

- Track `repair_debt: f64` across batches
- Each batch: `debt += batch × rate`, send `floor(debt)` repairs, subtract
- Eliminates per-batch ceiling rounding — at DC (rate ≈ 0.025), sends 1 repair
  every ~40 batches instead of 1 every batch

**Result**: DC 10.5% → 4.3%, WiFi 17.0% → 15.6%, Satellite 18.8% → 18.4%.

## Key Results

| Scenario | ADR-0047 | After ADR-0048 | Delta |
|----------|----------|----------------|-------|
| DC 1p RLC overhead | 10.5% | 4.3% | -6.2pp |
| DC 2p RLC overhead | 10.0% | 3.5% | -6.5pp |
| WiFi 1p RLC overhead | 21.4% | 15.6% | -5.8pp |
| WiFi 2p RLC overhead | 16.7% | 12.2% | -4.5pp |
| LTE 1p RLC overhead | 21.4% | 14.8% | -6.6pp |
| LTE 2p RLC overhead | 13.3% | 10.2% | -3.1pp |
| Satellite 1p RLC overhead | 33.2% | 18.4% | -14.8pp |
| DC 1p RLC recovery | 100% | 100% | unchanged |
| WiFi 1p RLC recovery | 100% | 100% | unchanged |
| LTE 1p RLC recovery | 100% | 99.9% | -0.1pp |
| Satellite 1p RLC recovery | 100% | 99.9% | -0.1pp |

Recovery preserved across all scenarios while overhead dropped significantly.

## Lessons Learned

1. **Budget caps become floors** if the mechanism that fills them (NACK) is always active.
   A cap must be combined with a gate that prevents unnecessary fills.

2. **`ceil()` on small rates creates large relative overhead.** `ceil(10 × 0.025) = 1`
   is a 40× overestimate. Fractional accumulators (common in rate shaping) fix this.

3. **Timing artifacts in gap detection** are as harmful as real gaps. An age gate that
   filters recently-sent symbols is essential when NACK scans a trailing window.

## Files Changed

| File | Changes |
|------|---------|
| `tests/bench_suite.rs` | Budget cap, age gate, fractional accumulator, hint reduction |
| `src/control/fec_rate.rs` | Realtime hint 5%→2%, Bulk hint -5%→-2% |

## Remaining Gaps

- WiFi 1p overhead 15.6% (target <15%) — close but NACK still contributes ~6%
- Satellite 1p overhead 18.4% — dominated by proactive rate (9% loss → high rate)
- RaptorQ/RS Satellite recovery 92.4% — block mode needs more repair budget at 9% loss
- Mettle overhead consistently ~3pp higher than RLC/Streaming (15% codec overhead)
