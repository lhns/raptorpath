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
│ FEC Encode  │  1. Emit source symbols (zero latency)
│ (5 backends)│  2. Stream repair symbols (fountain)
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
│ FEC Decode  │  Decode from any k(1+ε) of n symbols
│ (5 backends)│
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

### FEC Rate Control (BOCD + r*, ADR-0050)

The rate controller (`src/control/fec_rate.rs`) uses:

**Loss estimate**: BOCD (Bayesian Online Changepoint Detection) posterior
upper quantile at 95% confidence — the quantile widens automatically at
regime changes, providing the estimation-uncertainty margin. There is no
PI controller (removed by ADR-0050; `feedback_update*()` are no-ops).

**Rate formula** (paper Section 8.4, shared with `raptorpath-math`):

```
r = max( p/(1-p) + z_δ·√(p·σ²_burst/(W·(1-p))) + codec_eff,  B/T )
```

where `z_δ = normal_quantile(1 - target_tail_loss)` (the protocol hint
enters here), `σ²_burst = 1 + 2(1-p-q)/(p+q)` from the GE estimator,
`codec_eff` is codec overhead weighted by `P(decoder invoked) =
1-(1-p)^W`, and `B/T` is the burst term (mean burst length over
symbols-per-RTT).

**Spare-capacity gate**: repair rate is clamped to `(cwnd − in_flight) /
in_flight` so FEC never causes congestion ("never hurts" guarantee).

Note: the visualizer (`raptorpath-wasm`) currently drives the
triangle-solver controller from `raptorpath-math` (EWMA mean + δ/ρ
modes), which is NOT the production controller above — treat visualizer
results as illustrating the model, not validating production behavior.

### Multipath Scheduling
- Source symbols → lowest-RTT paths (minimize latency)
- Repair symbols → highest-goodput paths (maximize reliability)
- Proportional to available capacity (respects congestion)

### Loss Estimation
- Bayesian Beta-Binomial with EWMA decay
- Uses upper confidence bound (95th percentile) for FEC computation
- Burst detection with adaptive response

## Research: Optimal FEC/ARQ Architecture

This section documents research findings on how to evolve raptorpath's FEC layer. These are
design notes for future work, not a description of the current implementation.

### Current Architecture and Its Tradeoffs

RaptorPath uses **block-based RaptorQ** FEC: accumulate ~64KB of IP packets into a block (~50
source symbols), then generate repair symbols on demand. Key properties:

- **Rateless (fountain)**: can generate an unbounded number of repair symbols from any block.
- **Systematic**: source symbols are sent first, unmodified — zero decode cost on lossless links.
- **Near-optimal erasure recovery**: any k(1+ε) symbols suffice to decode k source symbols.
- **Limitation**: block assembly delay — repair symbols are only available after a full block is
  assembled. On slow links, block fill time adds latency before any FEC protection is active.
- **Rateless property is valuable proactively**: before feedback arrives, the encoder can generate
  repair symbols at the estimated loss rate and send them alongside source symbols. The receiver
  doesn't need all of them — any subset helps. This is where rateless codes shine.
- **Rateless property is less valuable reactively**: after ACK/NACK feedback (~1 RTT), the sender
  knows *exactly* which symbols are missing. Targeted retransmission of those specific symbols is
  more efficient than sending random fountain repair.

### Block Codes vs Sliding Window Codes

The fundamental architectural question for FEC in low-latency transport is **block-based vs
sliding-window coding**, not which specific code to use within a block.

**Block codes** (RaptorQ, Reed-Solomon): encode a fixed block of k source symbols into n coded
symbols. The block boundary is the problem — no protection until the block is full, and a block
boundary forces the decoder to wait for block completion before recovering symbols.

**Sliding window codes**: repair symbols continuously protect a sliding window of the most recent
W source symbols. No block boundary, no assembly delay — every repair symbol sent protects
everything currently in the window.

**Why sliding window codes are strictly better for low-latency transport:**

- Badr et al. (2017) proved the streaming capacity `C(T,B) = T/(T+B)` for channels with burst
  erasures of length B and delay constraint T. Block codes cannot match this within the same delay
  constraint because the block boundary wastes part of the delay budget.
- Roca's IETF NWCRG work on sliding window FEC for QUIC demonstrates the inherent latency and
  protection advantage of windowed codes over block codes in QUIC-like transports.
- rQUIC (Garrido et al., 2019) showed up to **60% latency reduction** on WiFi by integrating
  sliding window FEC into QUIC, compared to retransmission-only QUIC.

**The rateless property is not exclusive to block codes.** Bogino et al. (2007) proved that
rateless codes can operate over a sliding window instead of a fixed block, demonstrating that
"rateless/fountain" and "sliding window" are not mutually exclusive. However, Bogino's specific
LT-based construction was a research prototype with no production implementation — the theoretical
point stands, but no practical sliding window fountain code exists today.

### Streaming Codes

Streaming codes (Martinian & Sundberg 2004, Badr et al. 2017) are the theoretical foundation for
sliding window erasure coding:

- **Burst erasure correction with delay guarantees**: Martinian & Sundberg (2004) showed how to
  construct codes that recover from burst erasures of length B within a fixed delay T.
- **Rate-optimal layered constructions**: Badr et al. (2017) proved rate-optimal constructions for
  channels with both burst and random loss — layered codes that simultaneously handle both loss
  patterns within the delay constraint.
- **Systematic**: source symbols are sent first, same as RaptorQ — no decode cost on lossless links.
- **Fixed-rate variants have proven delay-optimal properties**: for a given channel model, these
  codes provably minimize recovery delay.
- **Sliding window fountain variants demonstrated in theory**: Bogino et al. (2007) showed
  rateless sliding window codes are possible as a proof of concept, removing the fixed-rate
  limitation in principle. No production implementation exists; the fixed-rate variants
  (Badr/Martinian) are better understood theoretically.

The real advantage of streaming codes is **continuous protection without block boundaries** and
**proven burst recovery within a delay constraint** — properties that block codes fundamentally
cannot provide regardless of block size.

### When the Block-Based Approach Still Works

RaptorQ is already implemented and working in raptorpath. It remains appropriate when:

- **Block assembly delay is acceptable**: bulk transfer, non-realtime traffic, or links fast enough
  that blocks fill quickly.
- **Loss is low and retransmission handles the rest**: when most blocks decode from source symbols
  alone and the occasional missing symbol is retransmitted, block boundaries don't matter.
- **Proactive FEC before feedback**: the rateless property is useful here — send repair symbols at
  the estimated loss rate alongside source symbols. Any subset the receiver gets helps, and the
  encoder doesn't need to commit to a fixed rate.

### Hybrid Proactive/Reactive Approach (Short-Term Improvement)

The pragmatic short-term improvement requires **no new FEC code** — just changing *when* repair
symbols are generated:

**Proactive phase** (before feedback, covers the first RTT):
- Generate RaptorQ repair symbols alongside source symbols at the estimated loss rate.
- The encoder already exists — this is a scheduling change, not a coding change.
- Rateless property is genuinely useful here: send repair at estimated rate, and any symbols the
  receiver gets help with decoding.

**Reactive phase** (after ACK feedback):
- Retransmit specific missing symbols — no FEC needed, exact knowledge of what's lost.
- **Cross-path retransmission**: if path A lost it, retransmit on path B.
- This is strictly more efficient than blind fountain repair.

The **optimal proactive/reactive split** depends on the `RTT × loss_rate` product:
- **High RTT or high loss** → more proactive FEC (long feedback-blind window, more symbols at risk)
- **Low RTT, low loss** → mostly reactive retransmission (feedback arrives quickly, few losses)

```
Phase 1: Proactive RaptorQ repair (before feedback)
├── Send source symbols + repair symbols at estimated loss rate
├── Uses existing RaptorQ encoder — scheduling change only
└── Rateless property means any repair symbols help

Phase 2: Reactive retransmission (after ACK feedback)
├── Targeted resend of specific missing symbols
├── Cross-path retransmit: if path A lost it, resend on path B
└── Most bandwidth-efficient — uses exact loss information
```

### Future: Sliding Window Coding (Medium-Term Evolution)

The real architectural evolution is replacing block-based RaptorQ with **sliding window erasure
coding**:

- **Eliminates block assembly delay entirely** — repair symbols protect a continuously advancing
  window of source symbols.
- **Better burst recovery within delay constraints** — proven optimal by Badr et al. (2017).
- **Sliding window RLC (RFC 8681)**: the only standardized sliding window FEC today. Reference C
  implementation exists (irtf-nwcrg/swif-codec, research prototype quality). Decoding is GF(2^8)
  Gaussian elimination — O(n³), but at raptorpath's window sizes (~50 symbols) this is fast.
  Steinwurf's RLNC benchmarks show 56 Gbps decode at K=16, 3.68 Gbps at K=500.
- **METTLE (2026)**: SC-MET-LDGM code (Yu, Yang, Meng, Xu — Georgia Tech) that converts spatial
  coupling into time coupling for streaming. The key innovation is a **pure peeling decoder** —
  unlike RaptorQ, which falls back to Gaussian elimination when peeling stalls, METTLE's graph
  structure ensures peeling almost always succeeds:
  - Each source packet is XOR'd into l=4 bins via hash functions with binomial edge placement.
  - **Touch-less Leading Edge (TLE)**: the first edge is deterministic — packet at position x
    always connects to bin `(1+c)·x`. No two first-edges collide, guaranteeing a peeling start.
  - Peeling cascade: 3-5 XOR operations per packet. Pure GF(2), no GF(2^8) multiplication.
  - **2.6 μs/packet decode** vs RaptorQ's 124-220 μs (47-85x faster).
  - Latency depends on window size w (paper uses w=600), **not block size k** — can use k=27,000
    with the same decode speed. This decoupling is the core architectural insight.
  - Overhead: 5.5% at 1% loss (competitive with RaptorQ's 6.14%), but 25% at 10% loss (worse
    than RaptorQ's 15% — the peeling decoder's simplicity costs coding efficiency at high loss).
  - **Caveat for raptorpath**: the paper optimized for w=600. Our windows are ~50 symbols —
    whether the spatially-coupled cascade propagates reliably at small w is unknown.
  - Patent filed covering scheme + implementation. No open-source code. Worth watching.
- **Bogino sliding window fountain codes**: proof of concept only — no implementation exists.
- **Realistic path**: implement sliding window RLC (the GF(2^8) math is straightforward) for our
  small window sizes, or wait for METTLE to become available.
- **Largest architectural change** in the FEC pipeline: replaces the block assembly → RaptorQ
  encode → block decode pipeline with a continuous window encoder/decoder.
- **Consider when**: latency requirements exceed what block-based proactive/reactive can deliver,
  or when burst patterns consistently defeat block-sized FEC.

### Key References

- **Badr et al.** — "Layered Constructions for Low-Delay Streaming Codes," IEEE Trans. IT, 2017.
  Rate-optimal streaming codes for burst+random erasure channels with delay constraints. Proves
  the streaming capacity `C(T,B) = T/(T+B)` that block codes cannot match.
- **Martinian & Sundberg** — "Burst Erasure Correction Codes with Low Decoding Delay," IEEE
  Trans. IT, 2004. Original streaming erasure codes with delay guarantees.
- **Roca et al.** — IETF NWCRG work on sliding window FEC for QUIC. Demonstrates latency and
  protection advantages of windowed codes over block codes in QUIC transports.
- **Bogino et al.** — "Sliding Window Digital Fountain Codes," 2007. Proof-of-concept rateless
  sliding window code. Demonstrated the theoretical compatibility of rateless + sliding window,
  but no production implementation exists.
- **RFC 8681** — Roca et al., "Sliding Window Random Linear Code (RLC) FEC Schemes for
  FECFRAME," 2020. Standardized sliding window FEC; reference implementation at
  github.com/irtf-nwcrg/swif-codec.
- **METTLE** — Yu, Yang, Meng, Xu (Georgia Tech), "Efficient Streaming Erasure Code with Peeling
  Decodability," arxiv 2602.10020, Feb 2026. SC-MET-LDGM construction — first streaming code with
  pure peeling decoder (no Gaussian elimination fallback). 2.6 μs/packet decode (47-85x faster
  than streaming RaptorQ). Decouples latency from block size via window parameter w. Provisional
  patent filed covering scheme and implementation; no open-source code.
- **rQUIC** — Garrido et al., "rQUIC: Integrating FEC with QUIC for Robust Wireless
  Communications," 2019. Up to 60% latency reduction on WiFi with FEC in QUIC.
- **DMTP** — IETF draft, multipath QUIC with deadline-aware streaming codes. Combines multipath
  scheduling with delay-constrained FEC.
- **MPLOT** — Sharma et al., "MPLOT: Multipath Loss-Tolerant Transport," INFOCOM 2008.
  Foundational multipath transport with proactive/reactive FEC split.
- **QUIC-FEC** — Michel et al., "QUIC-FEC: Bringing the benefits of FEC to QUIC," 2019.
  FEC plugin for QUIC; validated on real wireless links.
- **Cloud et al.** — "Multi-Path TCP with Network Coding," IEEE INFOCOM, 2013.
  Cross-path network coding for multipath transport.

## Protocol Hints

| Mode     | Effect (ADR-0050)                          | Use Case           |
|----------|--------------------------------------------|-------------------|
| Realtime | target_tail_loss × 0.01 (100× tighter z_δ) | VoIP, gaming      |
| Bulk     | target_tail_loss × 100 (100× looser z_δ)   | File transfer     |
| Auto     | target_tail_loss unchanged                 | General traffic   |

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

- [x] **Correlated loss modeling (Gilbert-Elliott HMM)** — Two-state HMM implemented
  (ADR-0023). Feeds burst_factor into FEC rate controller and mean_burst_length into
  streaming code params.

### Congestion Control & Scheduling Improvements

- [x] **ProbeRTT phase** — Implemented (ADR-0024). 10s interval, 200ms hold at cwnd=4.
  Prevents min_rtt drift from standing queues.

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

The FEC layer uses a **swappable backend** architecture (see [ADR-0021](docs/adr/0021-swappable-fec-backend.md)).
The `FecEncoder`/`FecDecoder` traits abstract the erasure code, and `FecBackend` enum selects
the implementation at runtime. Currently supported backends:

- **RaptorQ** (default) — RFC 6330 rateless fountain code.
- **METTLE** — SC-MET-LDGM streaming code with pure peeling decoder (patent-encumbered).
- **Reed-Solomon** — MDS erasure code with zero overhead.
- **RLC** — RFC 8681 sliding window random linear code (block + window mode).
- **Streaming** — Badr/Martinian delay-optimal two-layer code (ADR-0027).

Select via `--fec-backend <name>` (CLI) or `fec_backend = "<name>"` (TOML config).

For detailed evaluation, see [algorithm-competitive-analysis.md](docs/algorithm-competitive-analysis.md).

- [x] **WindowNack sender repair** — ADR-0025
- [x] **Multipath window scheduling** — ADR-0026
- [x] **Tapered repair interleaving** — ADR-0029
- [x] **Runtime backend switching** — ADR-0030

- [x] **Hybrid proactive/reactive FEC** — Fractional repair accumulator replaces burst and
  interval repairs (ADR-0037). *The original `loss_rate × 4.0` debt heuristic has since been
  superseded by ADR-0050:* the debt increment now comes from
  `compute_repair_rate_capped()` (BOCD + r*) shaped by the `TaperFunction`. ACK feedback and
  NACKs still reduce debt, so proactive overhead approaches zero at low loss. NACK handler
  retransmits exact source symbols (via `get_source()`) instead of random repairs.

- [x] **Sliding window FEC (streaming codes)** — Implemented three window backends: RLC (ADR-0022),
  METTLE window mode, and Streaming codes (ADR-0027). The streaming backend uses Badr/Martinian's
  layered construction (burst XOR + random GF(256)). Parameters derived from GE HMM estimator.
  *References: Badr et al. 2017, RFC 8681, ADR-0022, ADR-0027.*

- [x] **Cross-path retransmission** — When symbols are detected lost on path A, retransmit them on
  path B via `best_repair_path_avoiding()`. Falls back to any available path for single-path
  setups. Part of the hybrid proactive/reactive approach (ADR-0037).
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

### Benchmark Interpretation

The transport comparison benchmark ([ADR-0036](docs/adr/0036-transport-comparison-benchmark.md)) compares
raptorpath's FEC-based recovery against retransmission-based QUIC/MPTCP across 6 network scenarios. The
`overhead_pct` column measures **FEC repair symbol overhead only** — it does not include wire protocol
overhead (padding, headers, metadata serialization). QUIC retransmissions are internal to the channel
model and appear as increased latency, not explicit overhead.

The benchmark uses a **fractional repair accumulator** aligned with the production send loop.
(Historical note: at the time of ADR-0040 this was `repair_debt += batch * loss_rate * 4.0`;
production now derives the debt increment from the ADR-0050 controller — BOCD + r* via
`compute_repair_rate_capped()` — so benchmark numbers predating that change reflect the old
heuristic.) See [ADR-0040](docs/adr/0040-benchmark-repair-alignment.md) for the repair alignment fix.

**Multi-backend comparison** (ADR-0040): the benchmark tests three FEC backends across all scenarios:
- **RLC** (window mode) — GF(2^8) Gaussian elimination, ~0% coding overhead, O(k^3) decode
- **METTLE** (window mode) — XOR peeling decoder, ~15% coding overhead, O(k) decode
- **RaptorQ** (block mode) — fountain code, ~1% coding overhead, block-granularity delivery

The meaningful comparison is in the latency and recovery columns: raptorpath trades bandwidth (higher
overhead) for lower tail latency (p95/p99), especially on lossy and bursty links.

See [ADR-0038](docs/adr/0038-benchmark-overhead-taxonomy.md) for the full overhead taxonomy (five layers)
and metric definitions.

### Symbol Packing

Window-mode framing maps 1 IP packet → 1 FEC symbol, padding with zeros to `symbol_size`. For small
packets (VoIP 160B, DNS 60B, TCP ACK 52B) in a 512B symbol, 60-90% of each symbol is wasted.

`SymbolPacker` (in `net/framing.rs`) accumulates multiple small packets into one symbol using block-mode
length-prefix framing (`[u16 BE len][data]...[u16 0x0000 sentinel]`). This reuses the existing
`extract_packets()` function — no new parser needed.

- **Enabled for:** `ProtocolHint::Realtime` (VoIP, gaming)
- **Flush timeout:** 1ms (configurable) — partial buffers are emitted before the deadline
- **Protocol negotiation:** `packed: bool` in `ControlMessage::WindowStart` tells the receiver which
  extraction path to use
- **Impact:** 2-3x better symbol utilization for small packets; fewer symbols → fewer repairs → less
  FEC overhead

See [ADR-0039](docs/adr/0039-overhead-reduction.md) for details and trade-offs.

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
