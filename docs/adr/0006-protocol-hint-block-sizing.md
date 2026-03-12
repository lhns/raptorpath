# ADR-0006: Protocol Hint Should Influence Block Size and Timing

## Status
**Resolved** — BlockProfile derived from ProtocolHint controls max_block_size, flush_timeout, and symbol_size. Realtime: 4KB/2ms/512B. Bulk: 64KB/50ms/1200B. Auto: 16KB/10ms/1200B.

## Context
The `--protocol-hint` flag (realtime/bulk/auto) currently only affects the FEC repair multiplier in `fec_rate.rs`. But the biggest latency knob is block assembly, not FEC rate.

## Problem
A realtime application (VoIP, gaming) sends small packets frequently. Accumulating 64KB before encoding introduces 10-100ms+ of latency. A bulk transfer can tolerate that delay.

The hint should control the entire pipeline behavior, not just FEC aggressiveness.

## Decision Required
Protocol hint should set a profile that controls:

| Parameter           | Realtime      | Bulk          | Auto          |
|---------------------|---------------|---------------|---------------|
| Max block size      | 2-4 KB        | 64 KB         | 16 KB         |
| Flush timeout       | 2 ms          | 50 ms         | 10 ms         |
| FEC multiplier      | 1.0x (full)   | 0.7x          | 1.0x          |
| Burst extra FEC     | +10%          | none          | none          |
| Retransmit on fail  | no            | yes           | conditional   |
| Symbol size         | 256-512 bytes | 1200 bytes    | 1200 bytes    |

### Auto-detection
When hint is `auto`, detect traffic pattern:
- Many small packets (< 200 bytes) with < 20ms inter-arrival → realtime
- Large packets or continuous stream → bulk
- Mixed → default profile

## Consequences
- Realtime traffic gets sub-5ms added latency
- Bulk traffic gets optimal throughput with large blocks
- More configuration complexity, but profiles hide it from users
