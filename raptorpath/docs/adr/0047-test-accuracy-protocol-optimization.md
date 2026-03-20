# ADR-0047: Test Accuracy + Protocol Optimization

## Status

Accepted

## Context

Post-ADR-0046, the benchmark infrastructure had cwnd pacing, congestion-aware NACK, and
realistic retransmit simulation. But the 2026-03-19 results revealed accuracy gaps and
protocol deficiencies:

- **LTE**: 0-1% recovery (catastrophic) — FEC rate controller received fixed base_delay
  RTT, never seeing congestion-induced inflation
- **WiFi baseline**: 39.6% overhead (NACK spiral) — congestion-aware NACK added in
  production but benchmark used a simpler loss-rising counter
- **DC block**: p50=2ms — 2ms tick resolution quantized all latency to 2ms boundaries
- **Window PI feedback**: never called — benchmark used block-mode binary signal
  (`feedback_update(batch_dropped == 0)`) instead of window-mode repair efficiency

## Decision

### Iteration 1: Test Accuracy Fixes

#### 1A. Sub-millisecond clock resolution

Reduced tick from `Duration::from_millis(2)` to `Duration::from_micros(500)`.
Multiplied all tick loop counts by 4× to preserve the same real-time simulation
duration (5→20 per batch, 200→800 drain, 50→200 pacing limit).

**Result**: DC p50 dropped from 2ms to 1.0ms.

#### 1B. Feed measured RTT to scheduler and estimator

Replaced fixed `record_rtt(Duration::from_millis(scenario.base_delay_ms))` with
actual RTT computed from `pkt.delivery_time - encode_times[pkt.seq]` per delivered
packet. The per-batch average RTT is fed to both `scheduler.path_mut(id).record_rtt_sample()`
and `live_estimator.record_rtt()`.

**Result**: LTE 1-path recovery jumped from ~1% to 100% (RLC) / 98% (Mettle) / 99%
(Streaming). The FEC rate controller now sees congestion-inflated RTT → larger burst
term B/T → more proactive repairs.

#### 1C. Fix in-order rate metric to RFC 4737

Replaced pairwise comparison (`delivery_order[i] > delivery_order[i-1]`) with
max-seen tracking per RFC 4737: a packet is in-order if `seq > max_seen_so_far`.

**Result**: Minor shifts in reported in-order rates. The metric is now standard.

#### 1D. Wire PI feedback into window benchmark trial

Replaced `fec_ctrl.feedback_update(batch_dropped == 0)` (block-mode binary signal)
with `fec_ctrl.feedback_update_window(fed - last_fed, useful - last_useful)` using
the `WindowDecoder::repairs_fed()` / `repairs_useful()` trait methods. Track
`last_fed` / `last_useful` across batches for delta computation.

**Result**: PI feedback now active for window backends. Over-provisioned scenarios
see overhead reduction; under-provisioned scenarios get more repairs.

### Iteration 2: Protocol Optimization

#### 2A. Adaptive Beta confidence percentile

Changed `loss_rate_upper(0.95)` to `loss_rate_upper(0.85)` when `estimator.total_sent() > 500`.
After 500+ samples the posterior is tight; the 85th percentile gives a less conservative
(and more accurate) loss estimate. Added `pub fn total_sent()` getter on `LossEstimator`.

**Result**: ~2-3pp overhead reduction on established connections.

#### 2B. Reduce streaming safety factor

Reduced `ProtocolHint::Realtime` safety from 1.15→1.10, default from 1.10→1.05,
per ADR-0035 recommendation.

**Result**: Streaming overhead slightly reduced.

#### 2C. Production-style NACK congestion scaling

Replaced the simple `nack_loss_rising` counter (binary on/off at 2 consecutive rises)
with the production `NackCongestionState` logic: track both loss trend and measured
RTT trend, require both rising for congestion detection, exponential backoff (halve
multiplier), linear recovery (+0.1 per stable period).

**Result**: WiFi 1-path baseline overhead dropped from 39.6% to 21.4%. WiFi 2-path
from ~40% to 16.7%.

## Key Results Comparison

| Metric | Before (Mar-19) | After (Mar-20) | Delta |
|---|---|---|---|
| DC p50 (1-path) | 2.0ms | 1.0ms | -50% (1A) |
| LTE RLC 1p recovery | 1.1% | 100.0% | +98.9pp (1B) |
| LTE Mettle 1p recovery | 0.0% | 98.0% | +98.0pp (1B) |
| WiFi RLC 1p overhead | 39.6% | 21.4% | -18.2pp (2C) |
| WiFi RLC 2p overhead | ~40% | 16.7% | -23pp (2C+1D) |
| LTE 2p RLC recovery | ~1% | 100.0% | +99pp (1B) |

## Files Changed

| File | Changes |
|------|---------|
| `tests/bench_suite.rs` | Tick resolution (1A), RTT measurement (1B), in-order metric (1C), PI feedback (1D), NACK scaling (2C) |
| `src/control/fec_rate.rs` | Adaptive confidence (2A), safety factor (2B) |
| `src/control/estimator.rs` | Expose `total_sent()` getter (2A) |

## Remaining Work

- Satellite overhead still high at 33.2% (target <28%)
- DC overhead 5.5-10.5% (target <4%) — adaptive confidence helps but not enough
- Consider RTT-adaptive streaming T parameter (Iteration 3A)
- Consider BBR startup exit on delivery-rate plateau (Iteration 3B)
