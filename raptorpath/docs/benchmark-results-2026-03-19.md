# Benchmark Results — 2026-03-19 (ADR-0043 Information-Theoretic FEC Rate Controller)

Platform: Windows 11 Pro, `test` profile (optimized + debuginfo).
Run after ADR-0043 (information-theoretic FEC rate controller replacing stacked multipliers).

Run with: `cargo test --test bench_suite -- --nocapture`

---

## What Changed

ADR-0043 replaced the FEC rate controller's 5 stacked safety margins with an
information-theoretic optimal formula:

```
rate = max(p/(1-p) + codec_overhead, B/T) × (1 + margin) + pi + hint_offset
```

Key changes:
- GE burst factor and realtime burst extra removed (integrated into B/T term)
- PI gains reduced (Kp 2.0→0.5, Ki 0.5→0.1)
- Protocol hint: additive offset (+0.05/-0.05) instead of multiplicative (×1.2/×0.7)
- Production send loop now uses `compute_repair_rate()` instead of hardcoded `REPAIR_FACTOR = 4.0`
- Table 3 FEC budget tightened from 20% to 8% for feature differentiation
- Table 5 added: full QUIC vs MPTCP vs FEC transport comparison (ADR-0036)

---

## Table 1: Backend Recovery vs Uniform Loss

20% FEC budget, 1200B symbols, 30 trials per cell, 95% CI.

| Loss | RaptorQ | RS | RLC-win | Mettle-win | Streaming |
|-----:|----------------:|----------------:|----------------:|-----------------:|-----------------:|
| 1% | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 99.4 +/- 0.1 | 99.9 +/- 0.1 |
| 2% | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 98.7 +/- 0.1 | 99.9 +/- 0.1 |
| 5% | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 96.8 +/- 0.1 | 99.8 +/- 0.3 |
| 8% | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 94.8 +/- 0.2 | 100.0 +/- 0.0 |
| 10% | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 100.0 +/- 0.0 | 93.4 +/- 0.2 | 98.0 +/- 1.2 |
| 15% | 82.0 +/- 3.8 | 82.3 +/- 3.7 | 98.9 +/- 0.7 | 89.7 +/- 0.2 | 92.3 +/- 1.7 |
| 20% | 11.0 +/- 3.8 | 11.0 +/- 3.8 | 80.7 +/- 0.5 | 85.3 +/- 0.3 | 80.6 +/- 0.4 |
| 25% | 0.0 +/- 0.0 | 0.0 +/- 0.0 | 75.1 +/- 0.4 | 80.5 +/- 0.3 | 75.2 +/- 0.4 |

### Analysis

- **RaptorQ and RS** (block mode): perfect recovery up to 10% loss, cliff sharply at 15%.
  Block codes fail entirely when loss exceeds the redundancy budget.
- **RLC Window**: best window backend at all loss rates up to ~15% (98.9% at 15% vs 82%
  for block codes). Near-MDS with only 0.4% codec overhead.
- **Mettle Window**: degrades more gracefully than block codes at high loss (85.3% at 20%
  vs 11.0%) but underperforms RLC at all rates due to ~15% codec overhead.
- **Streaming**: tracks RLC closely at moderate loss, drops below Mettle at 20-25%.
  Delay-optimal diagonal interleaving is most valuable for bursty (not uniform) loss.
- **Block→Window crossover**: at ~12-15% loss, window codes overtake block codes.
  Confirms `threshold_high = 0.12` for backend auto-switching.

---

## Table 2: Wire Overhead Breakdown

2000 × 1200B symbols, Realtime hint, RLC window mode.

| Layer | DC (0.1%) | WiFi (2.5%) | Congested (12%) |
|-------|----------:|------------:|----------------:|
| FEC repair symbols | 5.7% | 11.2% | 20.0% |
| Symbol padding | 0.0% | 0.0% | 0.0% |
| Per-symbol metadata | 2.2% | 2.3% | 2.5% |
| Batch/wire header | 0.3% | 0.3% | 0.3% |
| Repair metadata | 0.1% | 0.1% | 0.2% |
| **Total** | **8.3%** | **13.9%** | **23.1%** |

### Analysis

- FEC repair is the dominant overhead layer. Per-symbol metadata (25B bincode header) is
  the second-largest at ~2%.
- **DC overhead: 5.7%** (down from 12.5% pre-ADR-0043). Gap above the information-theoretic
  minimum (~0.1%) comes from the 95th-percentile Beta upper bound — conservative by design.
- **WiFi: 11.2%** (down from 20% capped).
- **Congested: 20.0%** (hits max_fec_overhead cap). At 12% loss, optimal rate p/(1-p) = 13.6%
  plus codec overhead and safety margin exceeds cap.

### Comparison with pre-ADR-0043

| Scenario | Before (ADR-0042) | After (ADR-0043) | Change |
|----------|------------------:|-----------------:|-------:|
| DC (0.1%) | 12.5% | 5.7% | -6.8pp |
| WiFi (2.5%) | 20.0% (capped) | 11.2% | -8.8pp |
| Congested (12%) | 20.0% (capped) | 20.0% (capped) | 0pp |

---

## Table 3: Feature Ablation

WiFi bursty scenario, **8% FEC budget** (tightened from 20% to force differentiation),
30 trials per config.

| Config | Recovery | Overhead | p99 lat (ms) | In-order |
|--------|----------------:|----------------:|----------------:|--------:|
| baseline | 100.0 +/- 0.0 | 10.8 +/- 0.1 | 50.0 +/- 0.0 | 100.0% |
| no_nack | +0.0pp | -0.8pp | +0.0ms | 100.0% |
| no_reorder | +0.0pp | +0.0pp | +0.0ms | 100.0% |
| single_path | -0.0pp | +10.6pp | +50.0ms | 99.8% |
| no_pi | +0.0pp | +0.0pp | +0.0ms | 100.0% |

### Analysis

- **Multipath is the most impactful feature**: removing the second path doubles overhead
  (+10.6pp) and doubles p99 latency (+50.0ms). Path diversity halves effective loss rate.
- **NACK**: small -0.8pp overhead reduction when disabled. Proactive FEC covers most gaps
  at 2.5% loss. NACK is more valuable at tighter budgets or higher loss.
- **PI feedback** and **reorder buffer**: no measurable impact in this scenario. The
  simulated estimator warms up quickly and 50ms time steps mask reorder latency.
- Previous results at 20% budget showed 0pp deltas across all features — the budget was
  so generous that disabling features didn't matter.

---

## Table 4: FEC vs Retransmit (dual-path)

30 trials per scenario.

| Scenario | Transport | Recovery | p99 lat (ms) | Overhead | In-order |
|----------|-----------|----------------:|----------------:|----------------:|--------:|
| WiFi | FEC | 100.0 +/- 0.0 | 10.0 +/- 0.0 | 20.0 +/- 0.0 | 100.0% |
| | Retransmit | 100.0 +/- 0.0 | 20.0 +/- 0.0 | 2.8 +/- 0.1 | 53.5% |
| LTE | FEC | 100.0 +/- 0.0 | 40.0 +/- 0.0 | 20.0 +/- 0.0 | 100.0% |
| | Retransmit | 100.0 +/- 0.0 | 81.3 +/- 2.6 | 3.5 +/- 0.2 | 54.2% |
| Satellite | FEC | 100.0 +/- 0.0 | 286.7 +/- 12.4 | 20.0 +/- 0.0 | 99.6% |
| | Retransmit | 100.0 +/- 0.0 | 596.7 +/- 6.5 | 10.1 +/- 0.3 | 54.1% |

### Analysis

- FEC halves p99 latency vs retransmit in all scenarios.
- FEC achieves 100% in-order delivery vs ~54% for retransmit.
- FEC costs more bandwidth (20% vs 2.8-10.1%).
- Satellite shows the largest gap (2.1x) due to high RTT retransmit penalty.

---

## Table 5: Transport Comparison — QUIC vs MPTCP vs FEC

Full ADR-0036 comparison. 5 transport configs × 3 scenarios × 30 trials.

| Scenario | Transport | Recovery | p99 lat (ms) | Overhead | In-order |
|----------|-----------|----------------:|----------------:|----------------:|--------:|
| WiFi | QUIC single | 100.0 +/- 0.0 | 20.0 +/- 0.0 | 2.7 +/- 0.1 | 50.3% |
| | MPTCP rr | 100.0 +/- 0.0 | 20.0 +/- 0.0 | 2.8 +/- 0.1 | 53.5% |
| | MPTCP minRTT | 100.0 +/- 0.0 | 20.0 +/- 0.0 | 2.7 +/- 0.1 | 50.3% |
| | FEC single | 99.9 +/- 0.1 | 18.8 +/- 1.1 | 20.0 +/- 0.0 | 100.0% |
| | FEC dual | 100.0 +/- 0.0 | 10.0 +/- 0.0 | 20.0 +/- 0.0 | 100.0% |
| LTE | QUIC single | 100.0 +/- 0.0 | 82.7 +/- 3.1 | 3.4 +/- 0.3 | 51.8% |
| | MPTCP rr | 100.0 +/- 0.0 | 81.3 +/- 2.6 | 3.5 +/- 0.2 | 54.2% |
| | MPTCP minRTT | 100.0 +/- 0.0 | 82.7 +/- 3.1 | 3.4 +/- 0.3 | 51.8% |
| | FEC single | 99.9 +/- 0.1 | 95.3 +/- 10.1 | 20.0 +/- 0.0 | 99.5% |
| | FEC dual | 100.0 +/- 0.0 | 40.0 +/- 0.0 | 20.0 +/- 0.0 | 100.0% |
| Satellite | QUIC single | 100.0 +/- 0.0 | 600.0 +/- 0.0 | 10.1 +/- 0.3 | 52.6% |
| | MPTCP rr | 100.0 +/- 0.0 | 596.7 +/- 6.5 | 10.1 +/- 0.3 | 54.1% |
| | MPTCP minRTT | 100.0 +/- 0.0 | 600.0 +/- 0.0 | 10.1 +/- 0.3 | 52.6% |
| | FEC single | 99.7 +/- 0.1 | 1010.0 +/- 169.5 | 20.0 +/- 0.0 | 97.0% |
| | FEC dual | 100.0 +/- 0.0 | 286.7 +/- 12.4 | 20.0 +/- 0.0 | 99.6% |

### Analysis

**QUIC single-path**: simplest transport, lowest overhead (2.7-10.1%). Achieves 100%
recovery through retransmission but pays steep latency penalty. In-order delivery ~50%.

**MPTCP round-robin**: almost no improvement over QUIC single. Round-robin across paths
of different RTTs increases reordering. Second path's main benefit is failover redundancy
(not modeled here).

**MPTCP min-RTT**: sends all traffic on fastest path — identical to QUIC single when
paths are homogeneous (as in this test with symmetric path pairs). Would help in
heterogeneous scenarios.

**FEC single-path**: modest latency improvement at WiFi (18.8ms vs 20ms). At Satellite,
*underperforms* QUIC (1010ms vs 600ms) because FEC hits 20% cap and can't recover all
lost symbols — unrecovered symbols timeout. Retransmit guarantees eventual delivery.

**FEC dual-path**: clear winner for latency-sensitive workloads:
- WiFi: p99 = 10ms (2x better than QUIC 20ms)
- LTE: p99 = 40ms (2x better than QUIC 83ms)
- Satellite: p99 = 287ms (2.1x better than QUIC 600ms)
- 100% in-order delivery (vs ~50% for retransmit)
- Cost: 20% bandwidth overhead

**Key takeaways:**
1. FEC + multipath is the optimal combination for latency. Neither alone matches it.
2. MPTCP provides minimal latency benefit — retransmit delays dominate regardless of path count.
3. Single-path FEC struggles at high loss (Satellite). Argues for higher `max_fec_overhead`
   or combining FEC with NACK retransmission (which raptorpath does in production).
4. Fundamental tradeoff: FEC trades ~20% bandwidth for 2x latency reduction + in-order delivery.

---

## Notes

- "QUIC single" and "Retransmit" use `ReliableSimChannel` — a simplified retransmission
  model that re-enqueues dropped packets with RTT delay. Not real QUIC congestion control.
- All FEC configs use RLC window mode with adaptive repair rate from the information-theoretic
  controller (ADR-0043).
- Satellite scenario uses GE channel (p_gb=0.05, p_bg=0.4, loss_good=0.04, loss_bad=0.5),
  100ms base delay.
