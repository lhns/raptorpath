# ADR-0003: Loss Estimation Is Currently Broken

## Status
**Resolved** — receiver tracks batch sequence gaps via `PathBatchTracker`, feeds actual sent/received to estimator. ACK echo enables sender-side tracking.

## Context
The FEC rate controller depends on accurate per-path loss estimates to compute the right amount of repair symbols. The `LossEstimator` is well-designed, but it's fed incorrect data.

## Problem
In `net/mod.rs` line 257, after receiving a batch:
```rust
est.record_batch(received, received)  // always reports 0% loss!
```

This records `sent == received`, meaning loss is always zero. The FEC controller will never generate repair symbols beyond what the weak prior suggests.

### Root cause
The receiver only sees symbols that arrived — it has no knowledge of symbols that were lost. To detect loss, one of these is needed:

1. **Sender-side ACK tracking**: sender knows how many symbols it sent per path; receiver ACKs what it got; difference = loss
2. **Sequence gap detection**: receiver detects gaps in `batch_seq` numbers
3. **Receiver-side loss reports**: receiver periodically reports received/expected counts back to sender

## Decision Required
Implement a two-part solution:

### Part 1: Receiver detects loss via batch sequence gaps
Each `SymbolBatch` already has `batch_seq`. The receiver tracks expected sequence numbers per path and detects gaps.

### Part 2: Receiver sends PathReport to sender
Use the existing `ControlMessage::PathReport` to periodically send loss/RTT/throughput stats back. The sender uses these to update its estimator.

### Part 3: Sender tracks sent-vs-ACKed
The sender records how many symbols it sent per path per block. When it receives an `Ack`, it computes `sent - acked = lost` and feeds that to the estimator.

## Consequences
- Adds a feedback loop (receiver → sender)
- Slight increase in control traffic
- Essential for FEC rate control to function at all

## Related
- ADR-0005 (ACK mechanism)
- ADR-0007 (RTT calculation)
