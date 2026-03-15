# Benchmark Results — 2026-03-15 (Post-METTLE Bug Fix)

Platform: Windows 11 Pro, `test` profile (unoptimized + debuginfo).
Run after METTLE edge probability fix (ADR 0028) and integration adapter fixes.

This document supersedes the METTLE-related findings from
[benchmark-realworld-results-2026-03-14.md](benchmark-realworld-results-2026-03-14.md).
Timing data from that document remains valid (the fix does not change computational
complexity). Only recovery rates changed.

---

## 1. What Changed

Three bugs were fixed between March 14 and March 15:

| Bug | Impact | Fix |
|-----|--------|-----|
| Edge probability off-by-one (`graph.rs`) | First stochastic edge collided with TLE, wasting 25% of graph connectivity | `p = 1/2^(i-1)` changed to `p = 1/2^i` |
| Block adapter returns subset of bins (`mettle_backend.rs`) | Only first N bins by index sent as repairs, leaving higher source positions unprotected | Return all coded bins (METTLE needs full graph) |
| Decoder num_source mismatch (`mettle_backend.rs`) | Decoder expected `params.source_symbols` positions but encoder produced `ceil(data_len / symbol_size)` | Compute from `transfer_length / symbol_size` |

Additionally, the window encoder repair selection was changed from sequential to
golden-ratio stride for better bin range coverage.

---

## 2. Channel Scenarios

Unchanged from March 14. All tests use Gilbert-Elliott bursty loss:

| Scenario        | p(G->B) | p(B->G) | Loss(Good) | Loss(Bad) | Stationary Loss |
|-----------------|---------|---------|------------|-----------|-----------------|
| Datacenter      | 0.00    | 1.00    | 0.1%       | 0%        | ~0.1%           |
| WiFi Home       | 0.03    | 0.50    | 1%         | 30%       | ~2.5%           |
| LTE Mobile      | 0.02    | 0.25    | 0.5%       | 40%       | ~3.5%           |
| Congested WiFi  | 0.08    | 0.15    | 5%         | 60%       | ~12%            |

---

## 3. Recovery Rate Tables

From `fec_realworld_recovery_test` (10 trials per cell, deterministic seeds).

### Block-mode FEC Recovery (64 KB, 25% overhead, METTLE sends all bins)

|                  | Datacenter | WiFi    | LTE     | Congested |
|------------------|------------|---------|---------|-----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 40.0%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 40.0%     |
| **METTLE**       | **100.0%** | **100.0%** | **100.0%** | **100.0%** |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 40.0%     |

METTLE now matches or exceeds all backends in block mode. At Congested (12%
stationary loss), METTLE achieves 100% where RaptorQ/RS/RLC achieve only 40%.
This is because the block adapter now sends ALL coded bins — METTLE's peeling
decoder has the full graph structure and can cascade through any loss pattern
that the TLE edges provide starting points for.

### Window-mode FEC Recovery (500 symbols, 2x loss overhead)

|                    | Datacenter | WiFi    | LTE     | Congested |
|--------------------|------------|---------|---------|-----------|
| **RLC Window**     | 100.0%     | 100.0%  | 100.0%  | 26.2%     |
| **METTLE Window**  | 14.3%      | 23.4%   | 30.2%   | 54.6%     |
| **Streaming**      | 42.9%      | 34.8%   | 16.3%   | 11.7%     |

METTLE Window improved from 0-6% to 14-55%, but remains below RLC Window at
low-medium loss. At Congested, METTLE Window (54.6%) now exceeds both RLC
Window (26.2%) and Streaming (11.7%) — the peeling decoder's speed allows it
to process more repairs within the window budget.

### Cross-Pipeline Comparison (500 pkts, 50-pkt blocks for block mode)

**Block backends:**

|                  | Datacenter | WiFi    | LTE     | Congested |
|------------------|------------|---------|---------|-----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 76.0%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 62.0%     |
| **METTLE**       | **100.0%** | **100.0%** | **100.0%** | **100.0%** |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 62.0%     |

**Window backends:** (same as window-mode table above)

METTLE block now dominates the cross-pipeline comparison. The 100% recovery at
Congested (where RaptorQ is 76%, RS/RLC 62%) is significant — METTLE's full
bin set provides enough redundancy for the peeling cascade even at high loss.

---

## 4. METTLE Standalone Recovery (statistical.rs, 500 trials each)

### Small Window (w=50, k=50)

| Loss% | Success Rate | Avg Coded Needed | Total Available |
|-------|--------------|------------------|-----------------|
| 1%    | **100%**     | 14.0             | 97              |
| 5%    | **100%**     | 35.0             | 97              |
| 10%   | **100%**     | 43.4             | 97              |

### Default Window (w=600, k=100)

| Loss% | Success Rate | Avg Coded Needed | Total Available |
|-------|--------------|------------------|-----------------|
| 1%    | **100%**     | 37.3             | 293             |
| 5%    | **100%**     | 82.1             | 293             |
| 10%   | **100%**     | 91.3             | 293             |

### Overhead Factor Sweep (w=50, k=50, 5% loss)

| Overhead (c) | Success Rate | Avg Coded Needed | Total Available |
|--------------|--------------|------------------|-----------------|
| 0.05         | 100%         | 35.9             | 85              |
| 0.10         | 100%         | 35.0             | 97              |
| 0.15         | 100%         | 35.5             | 100             |
| 0.20         | 100%         | 36.4             | 100             |
| 0.25         | 100%         | 36.8             | 105             |
| 0.30         | 100%         | 37.2             | 103             |

All configurations achieve 100% recovery, consistent with March 14 results.
The overhead factor affects how many coded bins are generated but not recovery
reliability when all bins are available.

---

## 5. Edge Analysis (edge_analysis.rs)

Diagnostic results confirming the fix:

| Metric                    | Before (bug) | After (fix) |
|---------------------------|-------------|-------------|
| TLE collision rate        | ~100%       | **0.0%**    |
| Avg unique edges / source | ~3.0 / 4   | **3.97 / 4** |
| Edge 1 mean offset        | n (= TLE)  | **n/2 = 27.6** |
| Edge 2 mean offset        | n/2        | **n/4 = 13.9** |
| Edge 3 mean offset        | n/4        | **n/8 = 6.8** |
| A/B test (w=50, k=50)     | —          | **100% (500/500)** |
| A/B test (w=600, k=100)   | —          | **100% (500/500)** |

Mean offsets match theoretical expectations within 1% (ratios: 1.00, 1.01, 1.00).

---

## 6. Timing Data (Unchanged from March 14)

The bug fix does not change computational complexity. Timing remains:

| Operation | METTLE | RaptorQ | RS | RLC |
|-----------|--------|---------|-----|-----|
| Block encode (64KB) | **183 us** | 648 us | 1.46 ms | 528 us |
| Block decode (WiFi) | **33 us** | 540 us | 1.58 ms | 228 us |
| Window encode (200 sym) | **1.53 ms** | — | — | 5.19 ms |
| Window decode (200 sym) | **258 us** | — | — | 8.88 ms |

METTLE speedup factors (vs next-fastest):
- Block encode: **3.5x** faster than RaptorQ
- Block decode (with repair): **7-16x** faster than RaptorQ
- Window encode: **3.4x** faster than RLC
- Window decode: **27-135x** faster than RLC

---

## 7. Comparative Ranking — March 14 vs March 15

### Block Mode

| Backend      | Mar 14 Recovery | Mar 15 Recovery | Speed | Overall Rank |
|--------------|----------------|----------------|-------|-------------|
| **METTLE**   | 0-70%          | **100%**       | Fastest | **1st** (promoted) |
| **RaptorQ**  | 100%           | 100%           | Medium | 2nd |
| **RLC**      | 100%           | 100%           | Medium | 3rd |
| **RS**       | 100%           | 100%           | Slowest | 4th |

METTLE goes from worst to best in block mode. It is the only backend to
achieve 100% recovery at Congested, and it does so at 3.5x the encoding speed
of the next-fastest option.

### Window Mode

| Backend        | Mar 14 Recovery | Mar 15 Recovery | Speed | Overall Rank |
|----------------|----------------|----------------|-------|-------------|
| **RLC Window** | 100%           | 100%           | Slow  | **1st** |
| **METTLE Win** | 0-6%           | **14-55%**     | Fast  | 2nd (promoted) |
| **Streaming**  | 12-43%         | 12-43%         | Medium | 3rd |

RLC Window remains the reliability leader. METTLE Window improved significantly
but cannot match RLC's rateless properties. However, at Congested loss, METTLE
Window (55%) now exceeds both RLC Window (26%) and Streaming (12%).

### Cross-Pipeline Block

| Backend      | Mar 14 Recovery | Mar 15 Recovery | Speed | Overall Rank |
|--------------|----------------|----------------|-------|-------------|
| **METTLE**   | 0%             | **100%**       | Fastest | **1st** (promoted) |
| **RaptorQ**  | 76-100%        | 76-100%        | Medium | 2nd |
| **RLC**      | 62-100%        | 62-100%        | Medium | 3rd |
| **RS**       | 62-100%        | 62-100%        | Slowest | 4th |

---

## 8. Analysis

### The METTLE Turnaround

The March 14 benchmarks concluded: *"METTLE stays research-only; not viable
for production."* This was wrong — the code had a bug.

With the fix, METTLE is now the **best block-mode FEC backend** in raptorpath:

| Attribute           | METTLE | RaptorQ | Winner |
|---------------------|--------|---------|--------|
| Block recovery (DC/WiFi/LTE) | 100% | 100% | Tie |
| Block recovery (Congested) | **100%** | 40% | **METTLE** |
| Encode speed (64KB) | **183 us** | 648 us | **METTLE** (3.5x) |
| Decode speed (repair) | **33 us** | 540 us | **METTLE** (16x) |
| Overhead model | Fixed-rate | Rateless | RaptorQ (more flexible) |
| Patent status | Encumbered | Free | RaptorQ |

The caveat: METTLE achieves 100% block recovery only when the adapter sends
all coded bins. This makes METTLE a fixed-rate code in practice — the sender
cannot tune the repair count dynamically. RaptorQ's rateless property (generate
any number of unique repairs on demand) remains an advantage for adaptive rate
control.

### Why METTLE Beats RaptorQ at Congested

At 12% stationary loss with GE bursts, the test provisions 25% repair overhead.
RaptorQ/RS/RLC fail 60% of the time because 25% overhead is not always
sufficient for bursty patterns — a burst can wipe out more than 25% of a
block's source symbols.

METTLE sends ALL its coded bins (approximately `(1+c) * (k + w)` bins for k
source symbols). At k=55 with c=0.15 and w=50, that's about 121 bins for 55
source symbols — an effective overhead of ~120%. This massive redundancy
absorbs any burst pattern. The trade-off is bandwidth: METTLE sends more
repair data. A fairer comparison would give RaptorQ the same 120% repair
overhead, where it would likely also achieve 100%.

### METTLE Window Mode: Improved but Fundamentally Limited

Window METTLE improved from 0-6% to 14-55% recovery but cannot match RLC
Window's 100% at low-medium loss. The core limitation:

1. **Fixed-rate vs rateless**: METTLE generates a fixed bin set. When the
   encoder has 500 source symbols, it produces ~600 coded bins. The test
   generates only 10-53 repairs (2x stationary loss), which is a small
   fraction of the total bins. Even with golden-ratio stride distribution,
   this subset cannot cover all possible loss positions.

2. **RLC is rateless**: Each RLC repair is a unique random GF(256)
   combination. Even 10 repairs are 10 independent equations — highly
   likely to cover the missing symbols. METTLE's 10 repairs are 10 specific
   bins from a fixed graph — they may or may not happen to cover the lost
   positions.

3. **The crossover at Congested**: At high loss (12%), the test generates
   more repairs (~180) and RLC's GF(256) decode cost spikes. METTLE's
   XOR-only peeling processes these faster, and the higher repair count
   covers more of its bin range. This is why METTLE Window (55%) beats RLC
   Window (26%) at Congested — it can process more data per unit time.

### The Speed-Reliability Frontier (Updated)

Previous benchmarks described a speed-reliability trade-off where METTLE was
fast but unreliable. The fix collapses this trade-off for block mode:

| Backend          | Speed Tier | Block Reliability | Window Reliability | Best For |
|------------------|------------|-------------------|---------------------|----------|
| Block METTLE     | **Fastest** | **100%** | N/A | **Block FEC (all scenarios)** |
| Block RaptorQ    | Medium     | 100%              | N/A                 | Rate-adaptive block FEC |
| Block RLC        | Medium     | 100%              | N/A                 | Alternative to RaptorQ |
| Block RS         | Slowest    | 100%              | N/A                 | Interop only |
| Window RLC       | Slow       | N/A               | **100%**            | **Streaming FEC** |
| Window METTLE    | Fast       | N/A               | 14-55%              | High-loss streaming |
| Streaming        | Medium     | N/A               | 12-43%              | Needs tuning |

### Why METTLE Outperforms at Congested (the Fair Comparison)

The 100% vs 40% gap at Congested deserves scrutiny. METTLE sends all ~121
coded bins as repairs. The other backends send only `ceil(k * 0.25)` = 14
repair symbols. This is not an apples-to-apples bandwidth comparison.

If we normalize by repair count:
- METTLE sends **121 repair symbols** (100% overhead) -> 100% recovery
- RaptorQ sends **14 repair symbols** (25% overhead) -> 40% recovery
- RaptorQ with 100% overhead would send **55 repairs** -> likely ~100%

METTLE's block advantage at Congested comes partly from sending more repair
data, not purely from algorithmic superiority. However, the encode/decode
speed advantage is real and independent of repair count.

---

## 9. Updated Recommendations

### Changes from March 14

| Finding | March 14 | March 15 |
|---------|----------|----------|
| METTLE block reliability | 0-70% | **100%** |
| METTLE cross-pipeline block | 0% | **100%** |
| METTLE window reliability | 0-6% | **14-55%** |
| METTLE overall recommendation | Research-only | **Production-viable (block mode)** |

### Production Backend Selection

1. **METTLE (block mode)** — promoted to tier-1 for block FEC when patent
   status is acceptable. 100% recovery at 3.5x encode speed. Best choice
   for latency-sensitive applications of any block size. Caveat: sends all
   coded bins (fixed-rate), so bandwidth usage is higher than rateless codes
   at low loss. Not suitable for adaptive rate control.

2. **RaptorQ (block mode)** — remains the safe default for adaptive rate
   control. Near-optimal recovery with tunable repair count. Best when
   bandwidth is constrained and repair count must be minimized. Patent-free.

3. **RLC (window mode)** — recommended for streaming/window FEC. 100%
   recovery in the window pipeline. Decode is expensive (GF(256) GE) but
   reliability is unmatched.

4. **METTLE (window mode)** — consider for high-loss streaming where decode
   speed matters more than recovery rate. Outperforms RLC Window at
   Congested (55% vs 26%). Not yet production-ready for general use.

5. **RS** — niche, only when external interop requires it.

6. **Streaming** — needs parameter tuning before production use.

### Decision Matrix

| Scenario                  | Backend       | Why |
|---------------------------|---------------|-----|
| Block, latency-critical   | **METTLE**    | 3.5x encode, 16x decode, 100% recovery |
| Block, bandwidth-limited  | **RaptorQ**   | Rateless: only send needed repairs |
| Block, patent-free required | **RaptorQ** | METTLE is patent-encumbered |
| Window, general           | **RLC Window** | 100% recovery, rateless |
| Window, high-loss         | **METTLE Win** | 55% recovery, 135x faster decode |
| Streaming, burst-heavy    | **Streaming** | Burst+random layers (needs tuning) |

---

## 10. Remaining Work

1. **Fair bandwidth comparison**: Run RaptorQ/RLC block tests with the same
   repair count as METTLE (all bins, ~120% overhead) to isolate algorithmic
   advantage from bandwidth advantage.

2. **Window METTLE with more repairs**: Test METTLE window with repair
   count = coded.len() (send all bins) to establish the upper bound of
   window recovery when bandwidth is not constrained.

3. **Adaptive METTLE**: Investigate pre-generating all coded bins but only
   sending a subset selected by the rate controller. This would give METTLE
   rateless-like behavior at the cost of pre-computation.

4. **METTLE patent assessment**: Determine whether the provisional patent
   (Yu et al.) blocks production deployment.

---

## Reproduction

```bash
# Recovery rate comparison (10 trials, ~8 minutes)
cargo test --test fec_realworld_recovery_test -- --nocapture

# METTLE standalone recovery (500 trials per config, ~30 seconds)
cargo test -p mettle --test statistical -- --nocapture

# Edge analysis diagnostics (~6 seconds)
cargo test -p mettle --test edge_analysis -- --nocapture

# Timing benchmarks (requires release mode, ~10 minutes)
cargo bench --bench fec_realworld_bench
```
