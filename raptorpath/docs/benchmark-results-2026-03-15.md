# Benchmark Results — 2026-03-15 (Post-METTLE Bug Fix, Fair Benchmarks)

Platform: Windows 11 Pro, `test` profile (unoptimized + debuginfo).
Run after METTLE edge probability fix (ADR 0028), integration adapter fixes,
and benchmark fairness corrections.

This document supersedes the METTLE-related findings from
[benchmark-realworld-results-2026-03-14.md](benchmark-realworld-results-2026-03-14.md).
Timing data from that document remains valid (the fix does not change computational
complexity). Only recovery rates changed.

**Update (golden-ratio stride)**: Block adapter `repair_symbols()` now uses
golden-ratio stride bin selection (same pattern as `mettle_window.rs`) instead
of sequential `.take(count)`. This spreads selected bins quasi-uniformly across
the full bin range so every source position has coverage. Same-overhead METTLE
block recovery improved modestly (see tables below).

---

## 1. What Changed

Three bugs were fixed between March 14 and March 15:

| Bug | Impact | Fix |
|-----|--------|-----|
| Edge probability off-by-one (`graph.rs`) | First stochastic edge collided with TLE, wasting 25% of graph connectivity | `p = 1/2^(i-1)` changed to `p = 1/2^i` |
| Block adapter `repair_symbols()` ignored `count` (`mettle_backend.rs`) | Production code via `stream.rs` sent ~8x more repair data than intended; benchmarks gave METTLE unfair advantage | Respect `count` parameter, add `max_repairs()` to trait |
| Decoder num_source mismatch (`mettle_backend.rs`) | Decoder expected `params.source_symbols` positions but encoder produced `ceil(data_len / symbol_size)` | Compute from `transfer_length / symbol_size` |
| Sequential bin selection in `repair_symbols()` | `.take(14)` grabbed bins 0-13, missing sources whose TLE bins > 13 | Golden-ratio stride spreads bins quasi-uniformly across full range |

Additionally:
- Window encoder repair selection changed from sequential to golden-ratio stride
- Block encoder `repair_symbols()` also uses golden-ratio stride for bin selection
- Window METTLE repair budget unified with RLC (`2.0 × loss_rate`, min 5)
- Block benchmarks split into two tables: same-overhead (fair) and full-budget

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

### Block-mode FEC Recovery — Same Overhead (64 KB, k=55, all backends get 14 repairs)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |
| **METTLE**       | 80.0%      | 40.0%   | 60.0%   | 0.0%      | 14      | 25%      |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |

With identical overhead, METTLE is the **weakest** block-mode backend. Golden-ratio
stride bin selection improved recovery from 30-70% to 40-80% (DC-LTE) by spreading
14 bins across the full ~103 bin range, but the peeling decoder still cannot cascade
with so few bins. RaptorQ/RLC generate independent repair symbols, so each one
contributes useful information.

### Block-mode FEC Recovery — Full Budget (each backend's natural repair limit)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |
| **METTLE**       | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 103     | 187%     |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 40.0%     | 14      | 25%      |

METTLE achieves 100% recovery everywhere — but sends **7.4x more repair data**
(187% vs 25% overhead). This is NOT an apples-to-apples comparison. See the
same-bandwidth table below for the definitive answer.

### Block-mode FEC Recovery — Same Bandwidth as METTLE (all backends get 103 repairs = 187%)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 103     | 187%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 103     | 187%     |
| **METTLE**       | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 103     | 187%     |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 103     | 187%     |

**Key finding**: At equal bandwidth (187% overhead), all backends achieve 100%
recovery across all scenarios. METTLE's Congested advantage in the Full Budget
table is entirely due to sending 7.4x more data — not algorithmic superiority.
RaptorQ/RLC match METTLE's recovery when given the same bandwidth budget.

### Window-mode FEC Recovery (500 symbols, 2x loss overhead, unified budgets)

|                    | Datacenter | WiFi    | LTE     | Congested |
|--------------------|------------|---------|---------|-----------|
| **RLC Window**     | 100.0%     | 100.0%  | 100.0%  | 26.2%     |
| **METTLE Window**  | 14.3%      | 18.4%   | 19.2%   | 36.5%     |
| **Streaming**      | 42.9%      | 34.8%   | 16.3%   | 11.7%     |

All window backends now use the same repair budget formula (`2.0 × loss_rate`,
min 5). Previously METTLE used `3.0 × loss_rate` (min 10), inflating its
numbers. With unified budgets, METTLE Window is below RLC at all loss rates
except Congested (36.5% vs 26.2%).

### Cross-Pipeline Block — Same Overhead (500 pkts, 50-pkt blocks, 25%)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 76.0%     | 13      | 26%      |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 62.0%     | 13      | 26%      |
| **METTLE**       | 99.0%      | 57.0%   | 55.0%   | 4.0%      | 13      | 26%      |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 62.0%     | 13      | 26%      |

### Cross-Pipeline Block — Full Budget (each backend's natural repair limit)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 76.0%     | 13      | 26%      |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 62.0%     | 13      | 26%      |
| **METTLE**       | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 87      | 174%     |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 62.0%     | 13      | 26%      |

### Cross-Pipeline Block — Same Bandwidth as METTLE (all backends get 97 repairs = 194%)

|                  | Datacenter | WiFi    | LTE     | Congested | Repairs | Overhead |
|------------------|------------|---------|---------|-----------|---------|----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 97      | 194%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 97      | 194%     |
| **METTLE**       | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 97      | 194%     |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 100.0%    | 97      | 194%     |

Same pattern: at equal bandwidth, all backends achieve identical 100% recovery.

**Cross-pipeline window backends:**

|                    | Datacenter | WiFi    | LTE     | Congested |
|--------------------|------------|---------|---------|-----------|
| **RLC Window**     | 100.0%     | 100.0%  | 100.0%  | 26.2%     |
| **METTLE Window**  | 14.3%      | 18.4%   | 19.2%   | 36.5%     |
| **Streaming**      | 42.9%      | 34.8%   | 16.3%   | 11.7%     |

Same pattern as block mode: METTLE's cross-pipeline advantage at Congested
(100% vs 76%) comes from sending 6.7x more repair data (174% vs 26% overhead).
The same-bandwidth table confirms all backends match at equal overhead.

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

## 7. Comparative Ranking (Fair Benchmarks)

### Block Mode — Same Overhead (25%)

| Backend      | Recovery (DC/WiFi/LTE) | Recovery (Congested) | Speed | Rank |
|--------------|------------------------|----------------------|-------|------|
| **RaptorQ**  | 100%                   | 40%                  | Medium | **1st** |
| **RLC**      | 100%                   | 40%                  | Medium | 2nd |
| **RS**       | 100%                   | 40%                  | Slowest | 3rd |
| **METTLE**   | 40-80%                 | 0%                   | Fastest | 4th |

At equal overhead, METTLE is the worst block-mode backend. Its fixed-rate
peeling decoder cannot recover with only 14 of ~103 coded bins.

### Block Mode — Full Budget (each backend's max)

| Backend      | Recovery (all) | Repairs | Overhead | Speed | Notes |
|--------------|----------------|---------|----------|-------|-------|
| **METTLE**   | 100%           | 103     | 187%     | Fastest | 7.4x more repair data |
| **RaptorQ**  | 100% (DC-LTE), 40% (Cong) | 14 | 25% | Medium | Rateless, bandwidth-efficient |
| **RLC**      | 100% (DC-LTE), 40% (Cong) | 14 | 25% | Medium | Same as RaptorQ |
| **RS**       | 100% (DC-LTE), 40% (Cong) | 14 | 25% | Slowest | MDS |

METTLE can achieve 100% everywhere but only by spending ~7x the bandwidth.

### Block Mode — Same Bandwidth as METTLE (187% overhead)

| Backend      | Recovery (all) | Repairs | Speed | Rank |
|--------------|----------------|---------|-------|------|
| **RaptorQ**  | 100%           | 103     | Medium | **1st** (tied) |
| **RLC**      | 100%           | 103     | Medium | 1st (tied) |
| **RS**       | 100%           | 103     | Slowest | 1st (tied) |
| **METTLE**   | 100%           | 103     | Fastest | 1st (tied) |

At equal bandwidth (187% overhead), all backends achieve 100% recovery. METTLE's
only differentiator is speed — 3.5x faster encode, 16x faster decode.

### Window Mode (unified 2x loss budget)

| Backend        | Recovery (DC-LTE) | Recovery (Congested) | Speed | Rank |
|----------------|-------------------|----------------------|-------|------|
| **RLC Window** | 100%              | 26.2%                | Slow  | **1st** |
| **Streaming**  | 17-43%            | 11.7%                | Medium | 2nd |
| **METTLE Win** | 14-19%            | 36.5%                | Fast  | 3rd |

With unified repair budgets, METTLE Window only wins at Congested (36.5% vs
26.2%). At low-medium loss, RLC is strictly superior.

---

## 8. Analysis

### The Benchmark Fairness Fix

The previous version of this document reported METTLE as the "best block-mode
FEC backend" — this was misleading. Two benchmark issues inflated METTLE's
numbers:

1. **Block mode**: `repair_symbols()` ignored the `count` parameter and
   returned ALL ~103 coded bins, while other backends received only 14
   (25% overhead). METTLE got **7.4x more redundancy**.

2. **Window mode**: METTLE used `3.0 × loss_rate` (min 10) repair multiplier
   vs RLC's `2.0 × loss_rate` (min 5). Higher loss = more METTLE repairs,
   creating the appearance of recovery improving with loss.

3. **Production bug**: `stream.rs:31` calls `repair_symbols(count)` through
   the trait. METTLE ignoring `count` meant production code silently sent
   ~8x more repair data than intended.

The fix: `repair_symbols()` now respects `count`, a new `max_repairs()` trait
method lets callers discover the codec's repair budget, and benchmarks use
two tables to separate the apples-to-apples comparison from the full-budget
comparison.

### METTLE's Actual Position

| Attribute              | METTLE                  | RaptorQ           | Winner |
|------------------------|-------------------------|-------------------|--------|
| Same-overhead recovery (25%) | 40-80% (DC-LTE), 0% (C) | 100% (DC-LTE), 40% (C) | **RaptorQ** |
| Same-bandwidth recovery (187%) | 100%               | 100%              | **Tied** |
| Full-budget recovery   | 100% (103 rep, 187%)    | 40% (Cong, 14 rep, 25%) | METTLE (at 7x bandwidth) |
| Encode speed (64KB)    | **183 us**              | 648 us            | **METTLE** (3.5x) |
| Decode speed (repair)  | **33 us**               | 540 us            | **METTLE** (16x) |
| Overhead model         | Fixed-rate (~103 bins)   | Rateless          | **RaptorQ** |
| Patent status          | Encumbered              | Free              | **RaptorQ** |

**Same-bandwidth test proves it**: at 187% overhead, RaptorQ/RLC/RS all achieve
100% recovery — matching METTLE exactly. METTLE has no algorithmic recovery
advantage. Its speed advantage is real and significant; its recovery advantage
at full budget is purely a bandwidth artifact. The right use case is when
bandwidth is cheap and latency matters — send all bins and enjoy 3.5-16x
faster encode/decode.

### METTLE Window: Honest Numbers

With unified repair budgets (`2.0 × loss_rate`, min 5):
- **Datacenter**: 14.3% (was 14.3% — unaffected, min-5 dominates)
- **WiFi**: 18.4% (was 23.4% with 3x multiplier)
- **LTE**: 19.2% (was 30.2% with 3x multiplier)
- **Congested**: 36.5% (was 54.6% with 3x/min-10 multiplier)

The previous "recovery increases with loss" pattern was an artifact of the
higher repair multiplier giving METTLE proportionally more repairs at high loss.

### The Speed-Reliability Frontier

| Backend          | Speed Tier | Block (25% OH) | Block (187% OH) | Window | Best For |
|------------------|------------|----------------|-----------------|--------|----------|
| Block METTLE     | **Fastest** | 0-80%         | **100%**        | N/A    | Bandwidth-unlimited block FEC |
| Block RaptorQ    | Medium     | **100%**       | **100%**        | N/A    | **General block FEC** |
| Block RLC        | Medium     | **100%**       | **100%**        | N/A    | Alternative to RaptorQ |
| Block RS         | Slowest    | **100%**       | **100%**        | N/A    | Interop only |
| Window RLC       | Slow       | N/A            | N/A             | **100%** | **Streaming FEC** |
| Window METTLE    | Fast       | N/A            | N/A             | 14-37% | Fast decode, low reliability |
| Streaming        | Medium     | N/A            | N/A             | 12-43% | Needs tuning |

At 187% overhead (METTLE's full budget), all block backends achieve 100%
recovery. METTLE's only advantage at this overhead level is encode/decode speed.

---

## 9. Updated Recommendations

### Changes from Previous Version

| Finding | Previous (unfair) | Corrected (fair) |
|---------|-------------------|------------------|
| METTLE block (same overhead) | "100%" (was getting 103 repairs) | **0-80%** (with 14 repairs, golden-ratio stride) |
| METTLE block (full budget) | N/A | **100%** (103 repairs, 7.4x bandwidth) |
| METTLE window | 14-55% (inflated by 3x multiplier) | **14-37%** (unified 2x budget) |
| METTLE overall | "Best block-mode backend" | **Fast but bandwidth-hungry** |
| Production bug | `repair_symbols()` ignored count | **Fixed**: respects count, `max_repairs()` added |

### Production Backend Selection

1. **RaptorQ (block mode)** — the safe default. 100% recovery at 25% overhead,
   rateless (tunable repair count), patent-free. Best when bandwidth matters.

2. **METTLE (block mode, full budget)** — use when bandwidth is cheap and
   latency is critical. 3.5x encode, 16x decode speed. Must send all ~103
   coded bins. Patent-encumbered.

3. **RLC (window mode)** — recommended for streaming/window FEC. 100%
   recovery, rateless. Decode is expensive (GF(256) GE) but reliability
   is unmatched.

4. **METTLE (window mode)** — consider only for high-loss scenarios where
   decode speed matters more than recovery rate. 36.5% at Congested vs
   RLC's 26.2%, but 14-19% at lower loss rates.

5. **RS** — niche, only when external interop requires it.

6. **Streaming** — needs parameter tuning before production use.

### Decision Matrix

| Scenario                    | Backend        | Why |
|-----------------------------|----------------|-----|
| Block, general              | **RaptorQ**    | 100% recovery at minimal overhead, patent-free |
| Block, latency-critical     | **METTLE**     | 3.5x encode, 16x decode (must send all bins) |
| Block, bandwidth-limited    | **RaptorQ**    | Rateless: 14 repairs vs METTLE's 103 |
| Block, patent-free required | **RaptorQ**    | METTLE is patent-encumbered |
| Window, general             | **RLC Window** | 100% recovery, rateless |
| Window, high-loss + speed   | **METTLE Win** | 37% recovery, 135x faster decode |
| Streaming, burst-heavy      | **Streaming**  | Burst+random layers (needs tuning) |

---

## 10. Remaining Work

1. **Adaptive METTLE**: Investigate pre-generating all coded bins but only
   sending a subset selected by the rate controller. This would give METTLE
   rateless-like behavior at the cost of pre-computation.

2. **Window METTLE with more repairs**: Test METTLE window with repair
   count = coded.len() (send all bins) to establish the upper bound of
   window recovery when bandwidth is not constrained.

3. **METTLE patent assessment**: Determine whether the provisional patent
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

---

## 11. Tapered vs Flat Interleaving (ADR 0029)

**Setup**: 500 packets in 50-packet blocks, 25% overhead, 10 trials per scenario.
Tapered interleaving front-loads repairs from block B into block B+1's source stream
using exponential decay adapted to loss rate.

| Backend | Mode | Datacenter | WiFi | LTE | Congested |
|---------|------|-----------|------|-----|-----------|
| RaptorQ | Flat | 100.0% | 100.0% | 100.0% | 48.0% |
| RaptorQ | **Tapered** | 100.0% | 100.0% | 100.0% | **53.0%** |
| METTLE | Flat | 96.0% | 60.0% | 47.0% | 0.0% |
| METTLE | **Tapered** | **97.0%** | 60.0% | **58.0%** | **2.0%** |
| RLC | Flat | 100.0% | 100.0% | 100.0% | 28.0% |
| RLC | **Tapered** | 100.0% | 100.0% | 100.0% | **38.0%** |

**Key findings**:
- Tapered interleaving improves recovery in all high-loss scenarios with no regressions
- Largest gains on Congested (RLC +10pp, RaptorQ +5pp) and LTE (METTLE +11pp)
- The improvement comes from spreading repairs across burst boundaries — a burst
  that previously destroyed all of block B's repairs now only hits some of them
- Low-loss scenarios are unaffected (repairs still cluster near the front)
