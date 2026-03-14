# ADR-0024: BBR ProbeRTT Phase

## Status: Resolved

## Context

Raptorpath's BBR-style congestion controller (ADR-0019) tracks min_rtt in a 10-second
sliding window to estimate the propagation delay baseline. However, once the pipe is
full in steady state, measured RTTs include queuing delay — so the true propagation
delay may be lower than any sample in the window. Over time, min_rtt drifts upward
(becomes "stale"), causing BDP overestimation and persistent queue buildup.

BBRv1 addresses this with a **ProbeRTT phase**: periodically drain the pipe to near-empty,
measure the true propagation RTT, then resume normal operation.

## Decision

Replace the `in_startup: bool` field in `BbrState` with a `BbrPhase` enum:
```rust
enum BbrPhase { Startup, ProbeBw, ProbeRtt }
```

ProbeRTT state machine:
1. **Entry**: If `now - min_rtt_stamp > 10s` and not already in ProbeRTT → save current
   cwnd, set cwnd to 4 (PROBE_RTT_CWND), record done_stamp = now + 200ms
2. **During**: cwnd is held at 4; loss events don't further reduce cwnd
3. **Exit**: After 200ms, refresh min_rtt_stamp, restore prior cwnd, transition to ProbeBw

Key constants (matching BBRv1 spec):
- PROBE_RTT_INTERVAL = 10s
- PROBE_RTT_DURATION = 200ms
- PROBE_RTT_CWND = 4

min_rtt_stamp is refreshed whenever a new RTT sample matches or improves min_rtt,
preventing unnecessary ProbeRTT entries on channels with stable propagation delay.

## Consequences

- min_rtt stays fresh, preventing BDP overestimation on long-lived flows
- Brief throughput dip during the 200ms hold period (acceptable for correctness)
- ProbeRTT doesn't fire during startup (only from ProbeBw phase in practice)
- The 10s interval ensures ProbeRTT is rare — less than 2% of runtime is spent probing
