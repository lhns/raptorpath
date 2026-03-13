# ADR-0009: No Congestion Control

## Status
**Resolved** — AIMD congestion control with FEC-aware loss response. Slow start, congestion avoidance, multiplicative decrease on congestion loss, gentle reduction on FEC-recovered loss.

## Context
The scheduler tracks a per-path `cwnd` (congestion window) initialized to 10 symbols. This window never grows or shrinks — there is no congestion control algorithm.

## Problem
- `cwnd` starts at 10 and stays at 10 forever
- On a fast link, throughput is capped at 10 * 1200 bytes / RTT ≈ 480 Kbps at 20ms RTT
- On a congested link, the sender keeps pushing at the same rate, causing more loss
- QUIC has its own congestion control, but we're using unreliable datagrams which bypass it
- This means raptorpath can flood a network without backing off

## Decision Required
Implement per-path congestion control. Two options:

### Option A: Leverage QUIC CC (preferred short-term)
Use QUIC streams instead of datagrams for symbol transport. This gives us QUIC's built-in congestion control (typically Cubic or BBR). Downside: adds head-of-line blocking within a stream.

### Option B: Custom CC on datagrams (better long-term)
Implement a loss-based or delay-based CC algorithm per path:

1. **Slow start**: double cwnd every RTT until loss
2. **Congestion avoidance**: increase cwnd by 1/cwnd per ACK (AIMD)
3. **Loss response**: halve cwnd on loss detection
4. **Minimum cwnd**: never go below 2 symbols

A BBR-inspired approach (model-based, not loss-based) is ideal for lossy wireless links where loss doesn't always mean congestion. This is listed in DESIGN.md as future work.

### Interaction with FEC
FEC introduces interesting dynamics: some "loss" is expected and already compensated by repair symbols. The CC should distinguish between:
- **Congestion loss**: back off
- **Random/wireless loss**: don't back off (FEC handles it)

Use the FEC decode success rate as a signal: if blocks decode successfully despite loss, the link isn't congested.

## Consequences
- Without CC: raptorpath is a network-hostile application
- With CC: throughput adapts to available capacity
- FEC-aware CC is a research-level problem but essential for correctness

## Related
- ADR-0005 (ACK mechanism — required for CC)
