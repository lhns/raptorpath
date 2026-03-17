# ADR-0036: Raptorpath vs Reliable QUIC / MPTCP Transport Comparison Benchmark

**Status:** Accepted
**Date:** 2026-03-17

## Context

All existing benchmarks compare raptorpath FEC backends against each other. There is no comparison against plain reliable QUIC (retransmission-based recovery) and MPTCP-like multipath with retransmission. We need this to quantify whether the FEC approach actually provides value over standard reliable transport in various network conditions.

## Decision

Introduce a `ReliableSimChannel` that models retransmission-based reliable transport (QUIC-like) by re-enqueuing dropped packets with an additional RTT delay instead of discarding them. Use this alongside the existing lossy `SimChannel` in a benchmark that compares 5 transport configurations across 6 network scenarios.

### Transport Configurations

| Configuration | Channel | Paths | Scheduling | Description |
|---|---|---|---|---|
| `quic_single` | ReliableSimChannel | 1 | — | Plain reliable QUIC baseline (no FEC) |
| `quic_dual_rr` | ReliableSimChannel | 2 | Round-robin | MPTCP-like dual path |
| `quic_dual_minrtt` | ReliableSimChannel | 2 | Min-RTT selection | MPTCP-like with RTT-aware path selection |
| `raptorpath_single` | SimChannel (lossy) | 1 | — | RLC window FEC, single path |
| `raptorpath_dual` | SimChannel (lossy) | 2 | Multipath scheduler | RLC window FEC + multipath |

### Network Scenarios

| Scenario | RTT | Loss Profile | Expected Winner | Rationale |
|---|---|---|---|---|
| `dc_low_loss` | 1ms | 0.1% uniform | QUIC | Minimal loss; retransmit penalty negligible |
| `wifi_bursty` | 5ms | ~2.5% bursty | raptorpath | FEC avoids retransmit chains under burst loss |
| `lte_high_rtt` | 20ms | ~3.5% bursty | raptorpath | FEC avoids 40ms+ retransmit penalty per loss |
| `wifi_lte_hetero` | 5ms / 20ms | mixed | raptorpath_dual | Multipath scheduling advantage over hetero paths |
| `lossy_satellite` | 100ms | 8% uniform | raptorpath | Retransmission extremely expensive at 100ms RTT |
| `wifi_lte_asymmetric` | 5ms / 50ms | 1% / 5% | raptorpath_dual | Asymmetric RTT is MPTCP's known weakness |

### Metrics

- `recovery_rate` — fraction of source symbols successfully recovered
- `goodput_ratio` — useful data delivered per unit of channel capacity consumed
- `latency_p50`, `latency_p95`, `latency_p99` — end-to-end delivery latency percentiles
- `completion_time` — wall-clock time to deliver all symbols
- `overhead_pct` — repair symbol overhead as a percentage of source symbols
- `in_order_rate` — fraction of symbols delivered in original order

### Retransmission Model

When the Gilbert-Elliott channel model drops a packet, instead of discarding it, `ReliableSimChannel` re-enqueues the packet with:

```
delivery_time = base_delay + retransmit_delay + jitter
```

Retransmitted packets are themselves subject to loss (up to `max_retries = 3`), with each retry adding another full RTT to the delivery time. This models the compounding latency penalty of retransmission-based recovery under lossy conditions, including the common case of multiple consecutive losses on bursty links.

### Benchmark Parameters

- **Symbols per trial:** 4000 source symbols
- **Trials per configuration/scenario pair:** 20
- **Statistical summary:** mean, stddev, and per-percentile latency across trials

## Consequences

- We can now quantify the value of FEC-based recovery versus retransmission-based recovery across a range of network conditions.
- Expected outcome: raptorpath wins on tail latency (p95/p99) and completion time in lossy and bursty environments; QUIC wins on goodput_ratio in low-loss datacenter conditions where retransmit penalty is negligible.
- `ReliableSimChannel` is reusable as a baseline channel for future benchmarks comparing against reliable transport primitives.
- The 6-scenario matrix covers the main deployment environments (datacenter, WiFi, LTE, satellite, heterogeneous multipath), making results directly actionable for deployment decisions.
- The benchmark does not model congestion control interactions or flow control; those remain out of scope.

## Files

- `tests/common/mod.rs` — `ReliableSimChannel` added alongside existing `SimChannel`
- `tests/transport_comparison_bench.rs` — benchmark implementation with all 5 × 6 configuration/scenario combinations
