# RaptorPath: Multipath Transport with Fountain Code FEC

## Architecture

```
┌─────────────┐
│ Application │
│  (IP pkts)  │
└──────┬──────┘
       │ TUN interface
┌──────▼──────┐
│   Block     │  Accumulate packets into ~64KB blocks
│  Assembly   │
└──────┬──────┘
       │
┌──────▼──────┐
│  RaptorQ    │  1. Emit source symbols (zero latency)
│   Encoder   │  2. Stream repair symbols (fountain)
└──────┬──────┘
       │
┌──────▼──────┐
│  Scheduler  │  Route symbols to paths by RTT/goodput
└──┬───┬───┬──┘
   │   │   │   QUIC DATAGRAM per path
┌──▼┐┌─▼─┐┌▼──┐
│WiFi││LTE││Eth│  Independent QUIC connections
└──┬┘└─┬─┘└┬──┘
   │   │   │
   └───┼───┘
       │
┌──────▼──────┐
│  RaptorQ    │  Decode from any k(1+ε) of n symbols
│   Decoder   │
└──────┬──────┘
       │ TUN interface
┌──────▼──────┐
│ Application │
└─────────────┘
```

## Key Design Decisions

### Source-First Transmission
Original (unencoded) symbols are sent first. This lets the receiver:
- Process data immediately without waiting for decoding
- Only invoke the fountain decoder when packets are actually lost
- Achieve near-zero added latency on good links

### FEC Rate Control (Hybrid Feedforward + Feedback)

**Feedforward**: Statistical computation from the binomial loss model.
Given loss rate `p` and target tail loss `δ`:

```
r = k·p/(1-p) + z_δ · √(n·p·(1-p))
```

This gives the exact redundancy for constant tail loss probability.

**Feedback**: PI controller on residual block failure rate.
Compensates for model mismatch (correlated losses, estimation lag).

### Multipath Scheduling
- Source symbols → lowest-RTT paths (minimize latency)
- Repair symbols → highest-goodput paths (maximize reliability)
- Proportional to available capacity (respects congestion)

### Loss Estimation
- Bayesian Beta-Binomial with EWMA decay
- Uses upper confidence bound (95th percentile) for FEC computation
- Burst detection with adaptive response

## Protocol Hints

| Mode     | FEC Strategy                          | Use Case           |
|----------|---------------------------------------|-------------------|
| Realtime | Aggressive FEC, +10% during bursts   | VoIP, gaming      |
| Bulk     | Conservative FEC (70%), retransmit    | File transfer     |
| Auto     | Standard feedforward+feedback         | General traffic   |

## Platforms

- **Linux**: `tun` crate (kernel TUN/TAP driver)
- **Windows**: `wintun` crate (WinTUN userspace driver)

## Future Work

### High Priority

- [x] **Block interleaving** — Spread symbols from multiple blocks across time so a single burst
  doesn't wipe out one block. Interleave N blocks' symbols in round-robin before scheduling.
  Dramatically improves burst resilience without extra FEC overhead.

- [x] **Path MTU discovery** — Use `quinn::Connection::max_datagram_size()` (already queried in
  RTCP reports) to dynamically size symbols per path. Avoids fragmentation on constrained links
  and maximizes goodput on fat pipes. Requires per-path symbol sizing or padding strategy.

- [x] **Connection migration** — Add/remove paths at runtime without tearing down the session.
  Hot-add a new WiFi or LTE path when it becomes available, gracefully drain a dying path.
  Requires a control message to announce new paths and a handshake extension.

### Medium Priority

- [x] **BBR-style congestion control** — Replace AIMD (loss-based) with a delay-based algorithm.
  AIMD interprets wireless loss as congestion, causing unnecessary cwnd reduction. BBR uses
  RTT gradient to distinguish congestion from random loss — much better for mixed wireless paths.

- [x] **TLS cert pinning** — Current QUIC setup uses self-signed certs with insecure client
  validation. Add certificate pinning or a pre-shared key exchange for production deployments.

- [ ] **Correlated loss modeling** — Current Bayesian estimator assumes i.i.d. loss. Real wireless
  channels have Gilbert-Elliott burst patterns. A two-state HMM would improve FEC accuracy
  during burst/recovery transitions.

### Lower Priority

- [ ] **UDP passthrough mode** — Wrap raw UDP streams without TUN, for applications that want
  multipath FEC without a virtual network interface.

- [ ] **Layer 2 / Ethernet frame transport** — Currently Layer 3 (IP only). L2 mode would
  support non-IP protocols and bridging use cases.

- [ ] **Application-layer encryption** — Additional encryption beyond QUIC TLS for
  defense-in-depth or post-quantum considerations.
