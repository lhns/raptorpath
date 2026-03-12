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

- [ ] UDP passthrough mode (no TUN, just wrap UDP streams)
- [ ] Layer 2 / Ethernet frame transport
- [ ] Path MTU discovery per link
- [ ] Congestion control (BBR-inspired per path)
- [ ] Connection migration (path add/remove at runtime)
- [ ] Encryption (currently relies on QUIC TLS)
- [ ] Interleaving across blocks for burst loss resilience
