# ADR-0017: MTU-Aware Symbol Sizing

## Status: Resolved

## Context

Symbols are sent as QUIC DATAGRAM frames. If a symbol exceeds the path's PMTU,
the QUIC stack either fragments it (adding overhead and fragmentation loss risk)
or rejects it. Fixed symbol sizes (e.g., 1200 bytes) can't adapt to constrained
links (VPNs, tunnels) or take advantage of jumbo frames.

## Decision

Use `quinn::Connection::max_datagram_size()` — already queried every 2 seconds
in the RTCP report task and stored in `PathState.max_datagram_size` — to
dynamically compute the optimal symbol size before encoding each block.

### Implementation

1. **`Scheduler::min_mtu()`**: returns the minimum `max_datagram_size` across all
   active paths that have reported an MTU.

2. **In `encode_to_interleave_buf()`**: before encoding, compute:
   ```
   effective_symbol_size = min(profile_default, min_mtu - WIRE_OVERHEAD)
   ```
   Clamped to [64, profile_default]. Falls back to profile default if no MTU
   is known yet (first few seconds after startup).

3. **WIRE_OVERHEAD = 48 bytes**: accounts for 8-byte wire header + ~40 bytes
   bincode serialization overhead for the SymbolBatch wrapper.

### Why minimum across paths?

All symbols in a block must have the same size (RaptorQ constraint). Using the
minimum ensures every path can carry every symbol without fragmentation. If one
path has a much smaller MTU, it limits all paths — but this is correct behavior
since any symbol might be scheduled to any path.

## Alternatives Considered

1. **Per-path symbol sizing**: Different symbol sizes per path. Not possible with
   RaptorQ since all symbols in a block must be equal-sized.

2. **Symbol splitting**: Split large symbols into sub-symbols for constrained paths.
   Adds significant complexity; deferred to future work.

## Consequences

- No fragmentation on any active path
- Automatically adapts to VPNs, tunnels, and other MTU-constrained environments
- Graceful degradation: slightly smaller symbols = slightly more symbols per block,
  but same total data and FEC protection
