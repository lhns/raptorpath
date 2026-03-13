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

- [ ] **Correlated loss modeling (Gilbert-Elliott HMM)** — Current Bayesian estimator assumes
  i.i.d. loss. Real wireless channels have burst structure: a "good" state with low loss and a
  "bad" state with high loss, with transition probabilities between them. A two-state Hidden
  Markov Model tracks which state the channel is in and conditions FEC on the current state —
  more repair when the HMM predicts "bad state likely," less during stable "good state." Improves
  FEC accuracy during burst/recovery transitions where the i.i.d. model over- or under-estimates.

### Congestion Control & Scheduling Improvements

- [ ] **ProbeRTT phase** — The current BBR implementation never drains queues to measure true
  propagation delay. Real BBR v2 enters a ProbeRTT phase every ~10 seconds, reducing cwnd to 4
  for 200ms to let queues drain and get a clean `min_rtt`. Without this, `min_rtt` drifts upward
  over time as standing queues develop, inflating BDP estimates and causing the controller to
  over-allocate bandwidth. Implementation: add a timer to `BbrState` that triggers ProbeRTT,
  temporarily clamp cwnd, record the min RTT during the drain, then resume normal operation.

- [ ] **Pacing / burst smoothing** — Currently `on_ack` can grow cwnd aggressively during startup
  (`cwnd + acked`), causing micro-bursts that overwhelm WiFi buffers and trigger transient
  congestion. Real BBR uses pacing — spacing packets evenly across one RTT. QUIC itself has
  pacing at the datagram level, but our scheduler dumps a full cwnd-worth of symbols in a burst.
  A token-bucket pacer (one symbol every `RTT / cwnd` interval) would smooth transmission and
  reduce buffer bloat at WiFi access points and cellular base stations.

- [ ] **BLEST-style receive-side reordering awareness** — The scheduler sends source symbols to
  the lowest-RTT path, but when path RTTs differ significantly (e.g. 10ms WiFi vs 80ms LTE),
  symbols arrive wildly out of order at the receiver, causing head-of-line blocking during
  reassembly. BLEST (BLocking ESTimation, Ferlin et al. 2016) estimates whether sending on a
  slow path will stall the receiver and skips it if so. Implementation: before scheduling to a
  path, compute the expected arrival time relative to symbols already in flight on faster paths;
  skip the slow path if the gap exceeds a threshold.
  *Reference: Ferlin et al., "BLEST: Blocking Estimation-based MPTCP Scheduler," IFIP Networking 2016.*

- [ ] **ACK aggregation compensation** — WiFi access points and cellular base stations aggregate
  ACKs, adding jitter to RTT measurements that BBR can misinterpret as congestion (3 consecutive
  >10% increases). Filtering aggregation effects — e.g. tracking the minimum of recent RTT
  samples within each ACK cluster rather than raw samples — would make congestion detection more
  robust on wireless links. Implementation: add a short-window (50ms) min-filter before feeding
  RTT samples into `BbrState::record_rtt()`.

### FEC Architecture Improvements

- [ ] **Streaming codes (sliding window FEC)** — The current approach is block-based: accumulate
  ~64KB, encode, send. This adds latency equal to the block fill time, which hurts realtime
  traffic on slow links. Streaming codes (Badr et al., "Delay-Optimal Streaming Codes") use a
  sliding window — repair symbols continuously protect the last W source symbols with no block
  boundary and no assembly delay. The tradeoff is more complex decoder state (the receiver must
  maintain a window of symbols and attempt decoding as new symbols arrive). This would be the
  largest architectural change, replacing block assembly and the RaptorQ encode/decode pipeline
  with a convolutional coding approach.
  *Reference: Badr et al., "Layered Constructions for Low-Delay Streaming Codes," IEEE Trans. IT, 2017.*

- [ ] **Cross-block / cross-path coding** — Currently FEC is per-block: if an entire path dies
  mid-block, all symbols sent on that path are lost from that one block. Interleaving spreads
  symbols across time but doesn't create algebraic dependencies across blocks. Cross-block coding
  generates repair symbols that span multiple blocks, so even total loss of one block's worth of
  symbols can be recovered from symbols in adjacent blocks. Related work from Médard's group at
  MIT on "coded multipath" uses network coding to optimally exploit path diversity rather than
  treating paths independently.
  *Reference: Cloud et al., "Multi-Path TCP with Network Coding," IEEE INFOCOM, 2013.*

- [ ] **Proactive retransmission (speculative repair)** — When a symbol's expected ACK deadline
  passes on path A, immediately send a fresh repair symbol on path B *before* the block times
  out. This trades bandwidth for latency — cheaper than waiting for full block decode failure.
  The scheduler already knows per-path RTT, so the deadline is `send_time + path_rtt + margin`.
  After the deadline, generate one repair symbol and schedule it on the best alternative path.
  This is particularly effective for tail latency reduction: most blocks decode fine, but the
  rare blocks missing 1-2 symbols benefit enormously from a proactive repair on a different path.
  *Reference: MPTCP "redundant scheduling" literature; Barre et al., "Multipath TCP: From Theory to Practice," 2011.*

- [ ] **Priority-aware / unequal error protection** — Different traffic gets different FEC levels.
  Since we have raw IP packets from the TUN interface, we can inspect IP/TCP headers to classify
  traffic: TCP SYN/SYN-ACK, DNS queries, and video I-frames get stronger FEC protection, while
  bulk data gets standard or reduced FEC. Requires a lightweight DPI classifier in the block
  assembly stage that tags blocks with a priority level, and a modified `compute_repair_count`
  that scales repair symbols by priority.

### Lower Priority

- [ ] **Reinforcement learning for scheduling** — Use RL to learn scheduling policies that
  outperform hand-tuned heuristics. State space: per-path (RTT, loss, cwnd, queue depth, recent
  throughput). Action: which path gets the next symbol. The reward signal is block decode
  success rate weighted by latency. Requires training data from real-world multipath deployments
  or a realistic simulator. Interesting but hard to validate without large-scale deployment.
  *Reference: Wu et al., "ReMP: Learning Multipath Scheduling," NSDI, 2020.*

- [ ] **UDP passthrough mode** — Wrap raw UDP streams without TUN, for applications that want
  multipath FEC without a virtual network interface.

- [ ] **Layer 2 / Ethernet frame transport** — Currently Layer 3 (IP only). L2 mode would
  support non-IP protocols and bridging use cases.

- [ ] **Application-layer encryption** — Additional encryption beyond QUIC TLS for
  defense-in-depth or post-quantum considerations.
