# ADR-0050: FEC Rate Control Redesign — Principled Budget Architecture

**Status**: Implemented
**Date**: 2026-03-20

## Context

The FEC rate controller used a formula `rate = max(p/(1-p) + codec, B/T) × (1+margin) + PI + hint` that accumulated unnecessary overhead through:

1. A PI controller that could only push rates UP (`.max(0.0)` clamp)
2. Redundant uncertainty margins on top of an already-conservative Beta upper bound
3. No coordination with BBR congestion control
4. Unified TX/RX loss estimation that couldn't account for asymmetric paths
5. Protocol hint as additive overhead offset (+2%/-2%) instead of controlling the actual parameter
6. Codec overhead applied unconditionally (15% for METTLE even when decoder rarely invoked)

Measured overhead gaps vs information-theoretic optimum:
- DC (ε≈0.1%): 4.3% actual vs 0.1% optimal → 43x
- WiFi (ε≈2.5%): 15.6% actual vs 2.56% optimal → 6x
- Satellite (ε≈9%): 18.4% actual vs 9.9% optimal → 1.9x

## Decision

Replace the PI-based feedback architecture with a mathematically principled budget system providing two guarantees:

1. **"Never hurts"**: `repair_rate ≤ spare_capacity` enforced at generation time
2. **Convergence**: overhead → information-theoretic minimum as the posterior concentrates

### Architecture Changes

#### Phase 1: Asymmetric Loss Measurement + Budget Gate

- **NackAck protocol message**: Sender echoes `NackAck { nack_id }` when processing a WindowNack, enabling the receiver to measure RX path loss
- **RX loss tracking in LossEstimator**: Separate `rx_beta_a/b`, `rx_ewma_loss` fields with `update_rx_loss()` and `nack_effectiveness() = (1-ε_rx)²`
- **cwnd budget gate**: Before proactive repair generation, `compute_repair_rate_capped(est, spare_capacity)` ensures repairs never exceed available link capacity

#### Phase 2: Bayesian Online Changepoint Detection (BOCD)

Replaced EWMA + margin + PI with Adams & MacKay (2007) BOCD algorithm:

- **`changepoint.rs`**: Full BOCD with Beta-Binomial sufficient statistics, O(200) per update
- Maintains run-length distribution `P(r_t | data)` truncated at 200
- `predictive_quantile(confidence)` integrates over regime uncertainty
- Steady state → tight posterior → small margin → low overhead
- Changepoint → wide posterior → large margin → conservative protection
- The posterior quantile IS the margin — no separate safety factor needed

**PI controller removal**: `feedback_update()` and `feedback_update_window()` are now no-ops. All existing callers remain compatible.

#### Phase 3: Joint FEC/NACK Budget Allocation

**`BudgetAllocator`** splits total repair budget:
- `total_budget = p_upper / (1 - p_upper) + codec_overhead`
- `nack_expected = nack_rate × nack_effectiveness`
- `proactive_budget = total_budget - nack_expected`
- NACK repairs gated by remaining budget per reporting period

#### Phase 4: BBR/FEC Coordination

- **`PathState::spare_capacity()`**: Returns `(cwnd - in_flight) / in_flight`
- **`Scheduler::spare_capacity()`**: Aggregate minimum across active paths
- Repair rate clamped to spare capacity before generation
- When `spare_capacity < needed_rate`, BBR reduces source rate first

#### Phase 5: Visualization

Dashboard "Budget" tab with:
- Budget waterfall: IT minimum vs estimation tax per backend
- Overhead trend time-series across benchmark runs
- Estimation gap ratio (actual / IT minimum) per scenario

#### Protocol Hint → Tail Reliability (not overhead offset)

The system targets 100% reliability — everything gets through via FEC or NACK. The two mechanisms differ only in latency: FEC is proactive (zero added latency), NACK is reactive (costs one RTT). With perfect bandwidth optimization, the only remaining tradeoff is latency vs tail reliability.

The protocol hint maps to `target_tail_loss` at construction time:
- Realtime: `target × 0.01` (100× tighter → more FEC → less NACK latency)
- Bulk: `target × 100` (100× looser → less FEC → rely on NACK)
- Auto: unchanged

No additive offset. No magic knobs. The BOCD quantile at the adjusted confidence level IS the entire control mechanism.

#### Codec Overhead Weighted by P(decoder invoked)

For systematic codecs (METTLE, RLC, RaptorQ), the decoder is only invoked when ≥1 source symbol in the window is lost. The codec overhead should be weighted accordingly:

```
effective_codec_overhead = raw_overhead × (1 - (1-p)^window_size)
```

At DC (p=0.001, w=50): METTLE's 15% overhead becomes 15% × 4.9% = 0.74%.

The `compute_repair_rate()` method now takes `window_size` as a parameter. Callers pass the actual encoder window/block size.

The streaming params safety factor (previously 1.10 for Realtime, 1.05 otherwise) was removed — the BOCD quantile already provides the uncertainty margin.

## Mathematical Properties

- **Budget conservation**: `proactive + nack ≤ total_budget ≤ spare_capacity`
- **Asymmetry**: `nack_effectiveness = (1-ε_rx)²` correctly discounts NACK value on lossy feedback paths
- **Adaptation**: BOCD adapts to regime changes in 5-15 samples vs 20+ for EWMA

## Files Changed

| File | Action |
|------|--------|
| `src/control/changepoint.rs` | New: BOCD implementation (~250 lines) |
| `src/control/mod.rs` | Export changepoint module |
| `src/control/estimator.rs` | Split TX/RX loss, integrate BOCD, add `predictive_loss_upper()` |
| `src/control/fec_rate.rs` | Remove PI state, add posterior quantile, `BudgetAllocator`, `compute_repair_rate_capped()` |
| `src/transport/protocol.rs` | Add `NackAck { nack_id }` control message |
| `src/scheduler/mod.rs` | Add `spare_capacity()` to PathState and Scheduler |
| `src/net/mod.rs` | NackAck wire-up, cwnd budget gate, NACK budget gating |
| `tools/generate_dashboard.py` | Budget visualization tab |
| `tests/control_loop.rs` | Updated for BOCD (replaced PI assertions) |

## Consequences

- FEC overhead should decrease toward information-theoretic minimum as BOCD posterior concentrates
- FEC can never cause congestion (spare_capacity gate)
- NACK and proactive FEC share a coordinated budget (no double-spending)
- PI controller state removed — simpler, fewer knobs to tune
- `feedback_update()` API preserved as no-op for backward compatibility
