# ADR-0025: WindowNack Sender-Side Repair

## Status: Resolved

## Context

In sliding-window FEC mode (ADR-0022), the receiver detects gaps in the received
sequence and sends `WindowNack` control messages containing gap ranges back to the
sender. However, the sender ignores these NACKs — the `handle_control_message()`
function logs them but takes no action, and `run_window_sender()` has no mechanism
to receive them.

This means the only repair mechanism is **proactive** (periodic repair symbols
generated at a rate determined by the FEC controller). When a burst loss exceeds the
proactive repair budget, recovery relies entirely on future proactive repairs that
may not cover the specific lost sequences.

## Decision

Add a targeted repair loop driven by NACK feedback:

1. **Channel plumbing**: Create an `mpsc::channel<Vec<(u64, u64)>>(16)` connecting
   `handle_control_message()` (sender) to `run_window_sender()` (receiver). The
   channel carries gap ranges from incoming WindowNack messages.

2. **Sender repair loop**: After proactive repair generation, drain the NACK channel
   and generate targeted repair symbols:
   - Filter gaps to only those within the current encoder window
   - Generate `min(total_gap, MAX_NACK_REPAIRS_PER_NACK=10)` repair symbols per NACK
   - Send on the repair path (same path selection as proactive repair)

3. **Rate limiting**: Enforce a minimum 5ms cooldown between NACK repair bursts to
   prevent a flood of NACKs from overwhelming the sender or network.

## Consequences

- Lost sequences get targeted repair within one RTT of gap detection
- Bounded repair burst (max 10 per NACK) prevents sender overload
- Gaps outside the encoder window are safely filtered (encoder has already advanced)
- The 5ms cooldown prevents NACK storms from causing excessive repair traffic
- Proactive repair continues independently — NACK repair is additive
