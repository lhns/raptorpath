# ADR 0028: METTLE Performance Analysis — Edge Probability Bug + Integration Fixes

## Status

Resolved

## Context

Real-world benchmarks (2026-03-14) showed METTLE severely underperforming all other FEC backends:

| Scenario | METTLE Block | METTLE Window | Cross-pipeline Block | RaptorQ/RLC |
|----------|-------------|---------------|---------------------|-------------|
| Recovery rate | 0-70% | 0-6% | 0% | 100% |

METTLE's speed advantage (3.5x encoding, 27-135x window decoding) was irrelevant without functional recovery.

## Root Causes (Ranked by Impact)

### 1. Bug: Edge Probability Off-by-One (FIXED)

**File**: `mettle/src/graph.rs:83`

The first stochastic edge used probability `p = 1/2^(edge_idx - 1)`. For `edge_idx=1`, this gave `p = 1/2^0 = 1.0`, meaning all Bernoulli trials succeeded and `eta = n = (1+c)*w`. The resulting bin position was `right_boundary - n`, which is exactly the TLE bin position.

Since the encoder deduplicates bins (`encoder.rs:89-93`), this edge was silently discarded. Result: only 3 effective edges instead of 4 — **25% of graph connectivity wasted systematically**.

**Fix**: Changed to `p = 1/2^edge_idx`, giving geometrically-spaced spatial coupling:
- i=1: p=0.5 -> mean offset n/2 (halfway through window)
- i=2: p=0.25 -> mean offset n/4
- i=3: p=0.125 -> mean offset n/8

**Diagnostic results** (post-fix):
- TLE-stochastic collision rate: 0% (was ~100%)
- Average unique edges per source: 3.97 (was ~3.0)
- Mean offsets match theory exactly (ratios 1.00, 1.01, 1.00)

### 2. Block Backend: Repair Selection and Symbol Count (FIXED)

**File**: `raptorpath/src/fec/mettle_backend.rs`

Two issues in the block-mode adapter:

a) `repair_symbols()` took only the first N coded packets by bin_index. Since bin indices correlate with source positions, this only protected early source positions. METTLE is fixed-rate — the peeling decoder needs the complete bin structure to cascade. **Fix**: Return ALL coded bins regardless of the count parameter.

b) `MettleBlockDecoder` used `params.source_symbols` as `num_source`, but the encoder splits data at `symbol_size` boundaries, producing `ceil(data_len / symbol_size)` symbols. When these differed (e.g., 50 app-level packets encoded into 42 FEC symbols), the decoder expected positions that never existed. **Fix**: Compute `num_source` from `transfer_length / symbol_size`.

### 3. Window Encoder Repair Distribution (FIXED)

**File**: `raptorpath/src/fec/mettle_window.rs`

METTLE is fixed-rate — `coded_packets()` returns a fixed set of bins. The window encoder selected repair bins sequentially (0, 1, 2...), concentrating coverage on early source positions. **Fix**: Use golden-ratio stride for quasi-uniform bin distribution across the range.

### 4. w=50 vs Paper's w=600 (Not a Bug)

The paper (Yu et al., arxiv 2602.10020) evaluates at w=600. At w=50, spatial coupling has 12x less room for peeling cascades. Post-fix, w=50 achieves 100% recovery at 5% loss in block mode, so this is acceptable.

### 5. No GE Fallback (By Design)

When peeling stalls (no degree-1 bins), METTLE fails completely. RLC/RaptorQ use Gaussian Elimination as fallback. This is METTLE's design choice — speed comes from avoiding GE entirely.

### 6. Window Mode: Fixed-Rate Limitation (By Design)

METTLE window mode generates a fixed set of bins. With limited repairs per window (10-53 for 500 symbols), only a fraction of bins reach the decoder. Unlike rateless RLC (where each repair is unique and independently useful), METTLE needs most of its bin structure for peeling. This is a fundamental limitation of fixed-rate codes in sliding window mode.

## Recovery Results: Before vs After

### Integration Test (fec_realworld_recovery_test, 10 trials)

```
Block-mode:                Datacenter    WiFi      LTE       Congested
  METTLE (before)              70.0%    40.0%    50.0%         0.0%
  METTLE (after)              100.0%   100.0%   100.0%       100.0%
  RaptorQ                     100.0%   100.0%   100.0%        40.0%

Cross-pipeline Block:
  METTLE (before)               0.0%     0.0%     0.0%         0.0%
  METTLE (after)              100.0%   100.0%   100.0%       100.0%
  RaptorQ                     100.0%   100.0%   100.0%        76.0%

Window-mode:
  METTLE (before)               0.0%     5.7%     6.4%        31.1%
  METTLE (after)               14.3%    23.4%    30.2%        54.6%
  RLC Window                  100.0%   100.0%   100.0%        26.2%
```

### Standalone METTLE Tests (statistical.rs, 500 trials each)

| Config | Loss | Before | After |
|--------|------|--------|-------|
| w=50, k=50 | 1% | ~0-70% | **100%** |
| w=50, k=50 | 5% | ~0-70% | **100%** |
| w=50, k=50 | 10% | ~0-50% | **100%** |
| w=600, k=100 | 1% | ~70% | **100%** |
| w=600, k=100 | 5% | ~70% | **100%** |
| w=600, k=100 | 10% | variable | **100%** |

### Edge Analysis (edge_analysis.rs)

| Metric | Before | After |
|--------|--------|-------|
| TLE collision rate | ~100% | **0%** |
| Avg unique edges | ~3.0/4 | **3.97/4** |
| Edge 1 mean offset | n (=TLE) | **n/2** |
| Edge 2 mean offset | n/2 | **n/4** |
| Edge 3 mean offset | n/4 | **n/8** |

## Decision

1. Applied the edge probability fix (`1/2^(i-1)` -> `1/2^i`) — the core bug
2. Fixed block backend to return all coded bins (METTLE needs full graph for peeling)
3. Fixed block decoder to compute num_source from actual data, not params
4. Improved window repair distribution with golden-ratio stride
5. METTLE block mode is now **production-viable** — matches or exceeds other codecs
6. METTLE window mode improved but remains limited by fixed-rate design — use RLC for window mode
7. Added `edge_analysis.rs` regression tests to prevent future regressions

## Files Changed

| File | Change |
|------|--------|
| `mettle/src/graph.rs` | Fixed edge probability formula and updated doc comments |
| `mettle/tests/edge_analysis.rs` | New diagnostic and A/B test suite |
| `raptorpath/src/fec/mettle_backend.rs` | Return all bins; fix num_source computation |
| `raptorpath/src/fec/mettle_window.rs` | Golden-ratio stride for repair bin selection |
| `docs/adr/0028-mettle-performance-analysis.md` | This ADR |
| `docs/adr/README.md` | Added entry |
