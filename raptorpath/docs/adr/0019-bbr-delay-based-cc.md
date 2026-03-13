# ADR-0019: BBR-Style Delay-Based Congestion Control

## Status: Resolved

## Context

The original AIMD (Additive Increase, Multiplicative Decrease) congestion
control is loss-based: every packet loss reduces the congestion window.
On wireless links (WiFi, cellular), most loss is random interference — not
congestion. AIMD interprets this as congestion anyway, halving cwnd and
collapsing throughput even when the link has plenty of capacity.

This is the single biggest performance problem for multipath tunnels over
wireless networks.

## Decision

Replace AIMD with a BBR-inspired delay-based congestion controller that
uses **RTT gradient** instead of **loss** to detect congestion.

### Core Model

Track two sliding-window estimates (10-second windows):

- **`min_rtt`**: Minimum RTT observed — the propagation delay baseline
- **`max_bw`**: Maximum delivery rate in symbols/second

Compute the bandwidth-delay product:
```
BDP = max_bw × min_rtt
cwnd = gain × BDP
```

Where `gain` is 2.0 during startup (probe for bandwidth) and 1.0 in
steady state.

### Congestion Detection: RTT Gradient

Instead of reacting to loss, detect congestion via **rising RTT**:

- Track consecutive RTT increases (>10% above previous sample)
- After 3 consecutive increases → congestion detected
- Consecutive decreases clear the counter

This cleanly separates:
- **Wireless loss** (stable RTT + loss) → not congestion → no cwnd reduction
- **Real congestion** (rising RTT ± loss) → queue buildup → drain cwnd

### Loss Handling (4 cases)

| RTT Trend | FEC Recovered | Action | Rationale |
|-----------|--------------|--------|-----------|
| Stable | Yes | No change | Wireless loss, FEC handled it |
| Stable | No (decode fail) | cwnd -= 1 | Borderline; need more FEC, not less bandwidth |
| Rising | Yes | cwnd = BDP | Congestion but FEC saved us; drain to pipe size |
| Rising | No (decode fail) | cwnd = 0.75 × BDP | Severe congestion; aggressive drain |

### Startup Phase

Mirrors BBR's startup:
- Begin with 2× gain (cwnd grows aggressively)
- Exit when: RTT starts rising, or cwnd reaches BDP target
- Transition to 1× gain steady state

### Integration

`PathState::record_rtt_sample()` feeds RTT measurements from ACK processing
and PathReport handling into the BBR state. The existing `on_ack()` and
`on_loss()` methods are updated with the new semantics. No changes needed
to the net module's control flow — only the scheduler's internal behavior
changes.

## Alternatives Considered

1. **Full BBR v2**: The complete BBR state machine (Startup → Drain →
   ProbeBW → ProbeRTT) with pacing. Too complex for our symbol-based
   transport where QUIC already handles pacing at the datagram level.

2. **Keep AIMD with FEC-awareness only**: The gentle cwnd -= 1 for
   FEC-recovered loss was a partial fix, but still penalizes wireless
   loss. On a 5% WiFi loss link, cwnd continuously erodes.

3. **Copa/Vivace**: Research delay-based algorithms with formal utility
   optimization. Interesting but harder to implement and tune; BBR's
   approach is more battle-tested.

## Consequences

- Wireless random loss no longer collapses throughput
- Congestion is detected earlier via RTT gradient (queue buildup visible
  before loss occurs)
- Smooth cwnd convergence to BDP instead of sawtooth AIMD pattern
- FEC rate controller can focus on redundancy without fighting the CC
- Startup probing finds available bandwidth quickly (2× BDP gain)
