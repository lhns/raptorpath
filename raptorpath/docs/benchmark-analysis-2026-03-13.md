# Benchmark Analysis — METTLE vs RaptorQ

Companion to [benchmark-results-2026-03-13.md](benchmark-results-2026-03-13.md).
Interprets the raw numbers, analyzes architectural implications, and provides
deployment guidance.

---

## 1. Why METTLE Is Faster

METTLE's speed advantage comes from three design choices:

1. **Pure peeling decoder** — each received symbol triggers at most one XOR cascade.
   The work per symbol is O(1). RaptorQ falls back to Gaussian elimination (GE) when
   the systematic path fails, which is O(k^2) in the worst case.

2. **GF(2) only** — all operations are packet-level XOR. No finite field
   multiplication (GF(256) or larger) as in RaptorQ's intermediate symbol recovery.
   XOR is a single CPU instruction per byte; field multiply requires lookup tables
   or SIMD tricks.

3. **No pre-computation** — RaptorQ must build and invert a constraint matrix before
   encoding begins. METTLE's encoder just hashes each source packet position to
   determine bin assignments, then XORs. This explains the 54x encoding gap at 1 KB
   (RaptorQ's matrix setup dominates at small sizes) narrowing to 4x at 64 KB
   (where both are dominated by the actual XOR work).

The no-loss fast path is the exception: both backends just pass source symbols
through, but METTLE still builds the bin graph eagerly, costing ~1.5-2x vs
RaptorQ's lighter bookkeeping.

## 2. Why METTLE Has Lower Decode Success Rates

METTLE's peeling decoder works like a chain of dominoes:

- Each repair packet (bin) is the XOR of several source packets.
- If a bin has exactly one unknown source packet, it can be solved immediately.
- Solving one packet may reduce other bins to degree 1, triggering a cascade.

**The cascade needs a starting point.** If every bin has 2+ unknowns, peeling
stalls permanently — there is no fallback. RaptorQ avoids this by switching to
Gaussian elimination, which can solve any system with enough equations.

### The window/block ratio is what matters

The waterfall data reveals a clear pattern:

| w/k ratio | Example         | Success at 10% loss, 100% overhead |
|-----------|-----------------|-------------------------------------|
| 5.0       | w=50, k=10      | 89%                                 |
| 1.0       | w=50, k=50      | 49%                                 |
| 6.0       | w=600, k=100    | 100% (statistical test)             |

Success tracks **w/k**, not w or k individually. The paper's w=600 was designed
for large blocks where w/k >= 3-6. At w=k (our k=50 waterfall test), the
Touch-less Leading Edge (TLE) bins span the entire block with no room for the
stochastic edges to create the spatial coupling that makes peeling reliable.

### Why the standalone test shows 100% but the waterfall doesn't

| Test                        | What it feeds the decoder          | Result at w=50, k=50 |
|-----------------------------|------------------------------------|----------------------|
| Statistical (mettle crate)  | All source + **all** coded packets | 100% at all loss rates |
| Waterfall (raptorpath)      | Source + **limited** repair budget | 0.5-67%              |

The decoder *can* always recover — it just needs more repair packets than the
waterfall test provisions. METTLE's overhead isn't about decoder failure; it's
about needing a larger repair budget. The P1-5 fix (setting codec overhead to
15% for METTLE vs 1% for RaptorQ in the FEC rate controller) directly addresses
this, but the fundamental gap remains: METTLE needs more repair symbols than
RaptorQ to achieve the same reliability.

## 3. Block Interleaving Does Not Help METTLE's Cascade

Raptorpath's block interleaving (ADR-0016) spreads symbols from N blocks across
time in round-robin order: A₁B₁C₁A₂B₂C₂... instead of A₁A₂A₃...B₁B₂B₃....

This is an **inter-block** technique. A burst loss that would have destroyed 30%
of one block instead destroys 10% of three blocks. Both RaptorQ and METTLE
benefit equally — it reduces per-block loss rate.

**Interleaving does not change the peeling graph within a block.** METTLE's
cascade problem is about the structure of connections between source packets and
bins inside a single block. Interleaving doesn't add new bins, change edge
assignments, or widen the window. The dominos are in the same positions; they
just arrive in a different order over the wire (and order doesn't matter for
the peeling decoder — it processes all received symbols regardless of arrival
order).

### What would actually help METTLE

| Approach                     | Effect on cascade                  | Trade-off           |
|------------------------------|------------------------------------|---------------------|
| Increase w (e.g. w=200)      | More spatial coupling, w/k >> 1    | Higher decode latency |
| Decrease k (smaller blocks)  | Same w covers more of the block    | More blocks, more overhead per byte |
| More repair symbols          | More bins = more starting points   | Bandwidth cost      |
| Gaussian elimination fallback | Catches cascade stalls             | Destroys METTLE's speed advantage |

The first three are viable tuning knobs. The fourth defeats the purpose.

## 4. FEC vs Retransmission: When Is Repair Worth It?

### Cost model

**Retransmission (ARQ):**
- Detect loss: ~1 RTT (via ACK timeout or NACK)
- Request + receive retransmit: ~1 RTT
- Total per-lost-packet cost: **~2 × RTT latency**
- Bandwidth cost: **0% on non-lost packets** (only retransmit what's actually lost)
- Reliability: **100%** (repeat until delivered)

**Forward Error Correction (FEC):**
- Overhead paid proactively: **r/k bandwidth cost, always** (even when nothing is lost)
- On success: **0 extra latency** — lost packet recovered immediately from repair
- On failure: fall back to retransmission → **2 × RTT + wasted overhead bandwidth**

### Expected latency comparison

For a link with loss rate `p` and FEC success rate `s`:

```
Latency(no FEC)   = p × 2 × RTT
Latency(with FEC) = (1-s) × p × 2 × RTT     (only failures need retx)
Bandwidth(FEC)    = overhead_ratio             (always paid)
```

FEC reduces tail latency by factor `s`. The question is whether the bandwidth
cost is justified.

### Breakeven success rate

FEC is worth the bandwidth when the latency savings outweigh the cost. In a
bandwidth-constrained scenario, every byte of FEC overhead reduces available
throughput. The breakeven depends on how much you value latency vs throughput.

For a **latency-first** application (VoIP, gaming), FEC is worth it whenever:
```
success_rate > overhead / loss_rate
```

This says: the fraction of losses you fix must be worth more than the fraction
of bandwidth you sacrifice.

| Loss Rate | Overhead | Breakeven Success Rate | Interpretation |
|-----------|----------|------------------------|----------------|
| 1%        | 5%       | 100%+                  | FEC rarely worth it at 1% loss — just retransmit |
| 5%        | 10%      | 40%                    | Even modest FEC helps |
| 5%        | 20%      | 80%                    | Need good success rate to justify 4x overhead/loss |
| 5%        | 50%      | 100%+                  | Only if latency is worth 10x the bandwidth cost |
| 10%       | 20%      | 50%                    | Reasonable — fix half the losses |
| 10%       | 50%      | 100%+                  | Only for extreme latency sensitivity |
| 20%       | 50%      | 40%                    | Lossy link, FEC makes sense even with moderate success |

For a **throughput-first** application (bulk transfer), the calculus is simpler:
FEC overhead is only worth it if it prevents retransmission of more bytes than
the overhead itself:

```
overhead < loss_rate × success_rate
```

At 10% loss with 20% overhead, you need >200% success rate — impossible. This
means **FEC is almost never bandwidth-efficient for bulk transfer**. Use
retransmission instead (which is what the `ProtocolHint::Bulk` mode does,
reducing FEC to 70% of the feedforward amount).

### METTLE-specific assessment

Using waterfall data at different block sizes:

**k=10 (small blocks, w/k=5):**

| Loss | Overhead | METTLE Success | Worth it? (latency-first) |
|------|----------|----------------|---------------------------|
| 5%   | 50%      | 76%            | Marginal — fixing 3.8% of 5% loss for 50% BW |
| 5%   | 100%     | 94.5%          | Only if latency >> bandwidth value |
| 10%  | 100%     | 89%            | Yes for realtime — fixing 8.9% of 10% loss |

**k=50 (large blocks, w/k=1):**

| Loss | Overhead | METTLE Success | Worth it? |
|------|----------|----------------|-----------|
| 5%   | 100%     | 67.5%          | No — doubling BW to fix 3.4% of 5% loss |
| 10%  | 100%     | 49%            | No — doubling BW to fix 4.9% of 10% loss |
| 20%  | 100%     | 25%            | No — doubling BW to fix 5% of 20% loss |

**Verdict:** METTLE's FEC is only justified at **small block sizes (k ≤ 20)**
where w/k >= 3. At k=50 with w=50, the success rates are too low to justify
the overhead — retransmission is strictly better.

RaptorQ, by contrast, is justified at any block size: 100% success at 20%
overhead means you eliminate all retransmission latency for 20% bandwidth.

## 5. Recommendations by Use Case

### Datacenter (loss < 1%, RTT < 1ms)
- **Backend:** RaptorQ or no FEC
- FEC adds overhead for almost no benefit at sub-1% loss
- Retransmission at <1ms RTT is ~2ms — negligible
- If using FEC: RaptorQ at 5% overhead as insurance

### WiFi / moderate loss (loss 2-10%, RTT 5-30ms)
- **Backend:** RaptorQ with 10-20% overhead
- Sweet spot for FEC: retransmission costs 10-60ms, FEC eliminates it
- RaptorQ achieves 97-100% success at these overheads
- Block interleaving depth 2-3 for burst protection

### Cellular / high loss / realtime (loss 5-20%, RTT 30-100ms, latency-critical)
- **Backend:** RaptorQ default, METTLE viable with constraints
- METTLE: only at k ≤ 20, with 30-50% overhead, `ProtocolHint::Realtime`
- METTLE's encoding speed advantage matters here (4.8µs vs 257µs) for
  packet-at-a-time VoIP/gaming
- RaptorQ remains the safer choice unless encode latency is the bottleneck

### Bulk transfer (any loss, throughput-first)
- **Backend:** RaptorQ with `ProtocolHint::Bulk` (70% of feedforward FEC)
- Rely primarily on retransmission
- FEC as a thin layer to avoid retx stalls, not to eliminate retx entirely
- METTLE not recommended — success rates too low to justify bandwidth

### Summary matrix

| Scenario           | Backend  | Block size | Overhead | Interleave |
|--------------------|----------|------------|----------|------------|
| Datacenter         | RaptorQ  | 64 KB      | 5%       | Off        |
| WiFi streaming     | RaptorQ  | 16-64 KB   | 15%      | Depth 3    |
| VoIP / gaming      | METTLE*  | 1-4 KB     | 30%      | Depth 2    |
| Cellular bulk      | RaptorQ  | 64 KB      | 10%      | Depth 4    |
| High-loss realtime | RaptorQ  | 4-16 KB    | 25%      | Depth 2    |

*METTLE for VoIP/gaming only if encode latency is the bottleneck and w/k >= 3.
RaptorQ is the safe default in all scenarios.
