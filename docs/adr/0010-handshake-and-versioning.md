# ADR-0010: No Handshake or Protocol Versioning

## Status
**Open** — required for production use

## Context
Peers connect via QUIC and immediately start sending symbol batches. There is no application-level handshake to negotiate capabilities, version, or exchange path metadata.

## Problem
1. **No version negotiation**: if protocol changes, peers can't detect incompatibility. Deserialization silently fails or produces garbage.
2. **No capability exchange**: peers don't know each other's supported features (e.g., max block size, supported FEC schemes, available paths).
3. **No path coordination**: paths are assumed to match 1:1 by index. No way to add/remove paths dynamically.
4. **No authentication**: `SkipCertVerification` means any peer can connect. QUIC TLS provides transport encryption but no identity verification.

## Decision Required
Add a handshake phase after QUIC connection establishment:

### Handshake message (sent on first reliable stream)
```rust
struct Handshake {
    version: u32,                    // protocol version
    features: Vec<String>,          // supported features
    max_block_size: u32,
    supported_symbol_sizes: Vec<u16>,
    path_id: u32,                   // which path this connection represents
    peer_id: [u8; 32],             // stable peer identifier
}
```

### Version in every WireMessage
```rust
enum WireMessage {
    V1(WireMessageV1),
    // future: V2(WireMessageV2),
}
```

Or simpler: add a 4-byte version prefix to the serialized format.

## Consequences
- Peers can detect version mismatch immediately
- Enables future protocol evolution
- Path management becomes explicit
- Slight latency on initial connection (one RTT for handshake)
