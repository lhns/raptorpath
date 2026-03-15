# Real-World Benchmark Results — 2026-03-14

Platform: Windows 11 Pro, `bench` profile (optimized, release mode).
Run after FEC backend production hardening (21 new unit tests, 116 total lib tests passing).

All benchmarks use a **Gilbert-Elliott (GE) bursty channel model** instead of
uniform random loss. This simulates correlated packet loss as seen on real links.

---

## 1. Channel Scenarios

| Scenario        | p(G→B) | p(B→G) | Loss(Good) | Loss(Bad) | Stationary Loss |
|-----------------|--------|--------|------------|-----------|-----------------|
| Datacenter      | 0.00   | 1.00   | 0.1%       | 0%        | ~0.1%           |
| WiFi Home       | 0.03   | 0.50   | 1%         | 30%       | ~2.5%           |
| LTE Mobile      | 0.02   | 0.25   | 0.5%       | 40%       | ~3.5%           |
| Congested WiFi  | 0.08   | 0.15   | 5%         | 60%       | ~12%            |

**Datacenter** is essentially IID with 0.1% loss. **WiFi Home** has short bursts
(mean length 2 packets). **LTE Mobile** has longer bursts (mean 4 packets).
**Congested WiFi** has long, severe bursts (mean 6.7 packets at 60% loss).

---

## 2. Recovery Rate Tables

From `fec_realworld_recovery_test` (10 trials per cell, deterministic seeds).

### Block-mode FEC Recovery (64 KB, 25% overhead, METTLE gets 2x)

|                  | Datacenter | WiFi    | LTE     | Congested |
|------------------|------------|---------|---------|-----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 40.0%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 40.0%     |
| **METTLE**       | 70.0%      | 40.0%   | 50.0%   | 0.0%      |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 40.0%     |

**Key finding:** RaptorQ, RS, and RLC are identical at low-medium loss —
all three achieve 100% block recovery. At 12% stationary loss (Congested),
25% overhead is simply insufficient for any backend. METTLE fails much
earlier due to peeling stalls.

### Window-mode FEC Recovery (500 symbols, 2x loss overhead)

|                    | Datacenter | WiFi    | LTE     | Congested |
|--------------------|------------|---------|---------|-----------|
| **RLC Window**     | 100.0%     | 100.0%  | 100.0%  | 26.2%     |
| **METTLE Window**  | 0.0%       | 5.7%    | 5.8%    | 29.7%     |
| **Streaming**      | 42.9%      | 34.8%   | 16.3%   | 11.7%     |

**Key finding:** RLC Window dominates window-mode. METTLE Window recovers
almost nothing — `small_window()` config is too narrow for 500 symbols.
Streaming code underperforms because `from_channel()` with 1.15x target
generates too few repairs for real burst recovery.

### Cross-Pipeline Comparison (500 pkts, 50-pkt blocks for block mode)

**Block backends:**

|                  | Datacenter | WiFi    | LTE     | Congested |
|------------------|------------|---------|---------|-----------|
| **RaptorQ**      | 100.0%     | 100.0%  | 100.0%  | 76.0%     |
| **Reed-Solomon** | 100.0%     | 100.0%  | 100.0%  | 62.0%     |
| **METTLE**       | 0.0%       | 0.0%    | 0.0%    | 0.0%      |
| **RLC**          | 100.0%     | 100.0%  | 100.0%  | 62.0%     |

**Window backends:** (same as window-mode table above)

METTLE block at k=50 with 25% overhead (even doubled to 50%) fails completely.
This confirms the w/k ratio problem from the March 13 benchmarks: at k=50
with small window, peeling never starts.

---

## 3. Criterion Timing Data (Release Mode)

### Block Encode (64 KB, 1200-byte symbols, k=55)

| Backend      | Datacenter | WiFi Home | LTE Mobile | Congested WiFi |
|--------------|------------|-----------|------------|----------------|
| **RaptorQ**  | 648 µs     | 584 µs    | 578 µs     | 626 µs         |
| **RS**       | 1.46 ms    | 1.54 ms   | 1.49 ms    | 2.09 ms        |
| **METTLE**   | 183 µs     | 171 µs    | 175 µs     | 184 µs         |
| **RLC**      | 528 µs     | 729 µs    | 942 µs     | 2.79 ms        |

METTLE is **3.2-3.5x faster** than RaptorQ at encoding. RLC scales
linearly with repair count (more repairs at higher loss = slower). RS
is the slowest encoder.

### Block Decode (64 KB, after GE channel)

| Backend      | Datacenter | WiFi Home | LTE Mobile | Congested WiFi |
|--------------|------------|-----------|------------|----------------|
| **RaptorQ**  | 62 µs      | 540 µs    | 68 µs      | 39 µs          |
| **RS**       | 1.50 ms    | 1.58 ms   | 1.61 ms    | 15 µs          |
| **METTLE**   | 109 µs     | 33 µs     | 109 µs     | 48 µs          |
| **RLC**      | 59 µs      | 228 µs    | 54 µs      | 2.36 ms        |

Decode times vary by how much loss the GE channel produces:
- **Datacenter** (0.1% loss): systematic fast path dominates. RaptorQ and
  RLC are ~60 µs (just reassembly). METTLE is 109 µs (graph overhead).
- **WiFi** (2.5%): RaptorQ needs GE fallback → 540 µs. METTLE peeling
  is only 33 µs. RLC at 228 µs.
- **Congested** (12%): RS drops to 15 µs — likely fast failure (not enough
  repair symbols to even attempt). RLC jumps to 2.36 ms (GF(256) solving
  a large system).

### Window Encode (200 symbols × 1000 bytes)

| Backend       | Datacenter | WiFi Home | LTE Mobile | Congested WiFi |
|---------------|------------|-----------|------------|----------------|
| **RLC**       | 5.19 ms    | 5.50 ms   | 9.53 ms    | 30.1 ms        |
| **METTLE**    | 1.53 ms    | 1.60 ms   | 2.57 ms    | 7.28 ms        |
| **Streaming** | 1.02 ms    | 736 µs    | 2.59 ms    | 18.5 ms        |

METTLE window encoding is **3.4x faster** than RLC. Streaming is fastest
at low loss (minimal repair generation) but scales poorly under congestion.

### Window Decode (200 symbols, after GE channel)

| Backend       | Datacenter | WiFi Home | LTE Mobile | Congested WiFi |
|---------------|------------|-----------|------------|----------------|
| **RLC**       | 8.07 ms    | 8.88 ms   | 9.54 ms    | 26.4 ms        |
| **METTLE**    | 302 µs     | 258 µs    | 230 µs     | 195 µs         |
| **Streaming** | 338 µs     | 477 µs    | 612 µs     | 5.13 ms        |

METTLE window **decoding is 27-135x faster** than RLC window. This is
the pure peeling advantage — each symbol triggers O(1) XOR work. RLC
needs GF(256) Gaussian elimination, scaling as O(w²).

Note: METTLE's fast decode doesn't help if it can't recover the data
(see recovery rates above — 0-6% success).

### Cross-Pipeline (64 KB data, encode + channel + decode)

| Backend              | Datacenter | WiFi Home | LTE Mobile | Congested WiFi |
|----------------------|------------|-----------|------------|----------------|
| **Block RaptorQ**    | 603 µs     | 1.12 ms   | 603 µs     | 599 µs         |
| **Block RS**         | 2.98 ms    | 3.18 ms   | 3.04 ms    | 2.08 ms        |
| **Block METTLE**     | 306 µs     | 222 µs    | 279 µs     | 227 µs         |
| **Block RLC**        | 602 µs     | 960 µs    | 917 µs     | 5.18 ms        |
| **Window RLC**       | 1.83 ms    | 1.97 ms   | 1.77 ms    | 4.35 ms        |
| **Window METTLE**    | 569 µs     | 575 µs    | 511 µs     | 623 µs         |
| **Window Streaming** | 344 µs     | 316 µs    | 334 µs     | 1.71 ms        |

End-to-end latency ranking (WiFi scenario):
1. Block METTLE — 222 µs (fastest but 0% recovery)
2. Window Streaming — 316 µs (fast but 35% recovery)
3. Window METTLE — 575 µs (fast but 6% recovery)
4. Block RLC — 960 µs (100% recovery)
5. Block RaptorQ — 1.12 ms (100% recovery)
6. Window RLC — 1.97 ms (100% recovery)
7. Block RS — 3.18 ms (100% recovery)

---

## 4. Analysis

### Speed vs Reliability Trade-off

The central finding is a clear **speed-reliability Pareto frontier**:

| Backend          | Speed Tier | Reliability Tier | Sweet Spot |
|------------------|------------|------------------|------------|
| Block METTLE     | Fastest    | Poor (0-70%)     | Only k ≤ 10 with high overhead |
| Window METTLE    | Fast       | Very poor (0-6%) | Not viable with current config |
| Streaming        | Fast       | Poor (12-43%)    | Needs tuning (1.15x too low)   |
| Block RaptorQ    | Medium     | Excellent (100%) | **Production default**          |
| Block RLC        | Medium     | Excellent (100%) | Good alternative to RaptorQ     |
| Window RLC       | Slow       | Excellent (100%) | Best for streaming use cases    |
| Block RS         | Slowest    | Excellent (100%) | Only if interop required        |

### METTLE Performance Paradox

METTLE achieves impressive timing numbers:
- 3.5x faster encoding than RaptorQ
- 27-135x faster window decoding than RLC

But these gains are academic when recovery rates are 0-6%. The peeling
decoder is O(1) per symbol *when it works*, but it rarely works at
k=50/window=small because:

1. **No degree-1 bins** — all bins cover 2+ unknowns, cascade never starts
2. **No GE fallback** — unlike RaptorQ, peeling is the only solver
3. **Window too small** — `small_window()` provides insufficient spatial
   coupling for 500-symbol streams

### RaptorQ vs RLC: Surprising Parity

Block RaptorQ and block RLC achieve identical 100% recovery at Datacenter,
WiFi, and LTE scenarios. They only differ at Congested (40% vs 40% — same).
Performance is also similar (~600 µs encode, ~60 µs decode at low loss).

The differentiation appears at high loss where RLC's GF(256) Gaussian
elimination is expensive: 2.36 ms decode at Congested vs RaptorQ's 39 µs.
RaptorQ's LDPC+GE hybrid is more efficient under stress.

### Reed-Solomon: Consistent but Slow

RS is 2-3x slower than RaptorQ at everything (encode and decode) but
achieves identical recovery rates. The GF(256) Vandermonde matrix approach
is mathematically optimal (MDS code) but computationally heavy compared to
LDPC-family codes. Use only when interoperability with external systems
requires RS specifically.

### Streaming Code: Undertuned

The streaming encoder's `from_channel(mean_burst, loss, 1.15)` generates
repairs targeting 1.15x the estimated loss. This is too thin for GE burst
channels where the actual loss during bursts is 30-60%. The code works
correctly — it just doesn't generate enough repair packets. Tuning the
target to 2-3x would likely improve recovery substantially.

---

## 5. Updated Recommendations

### Changes from March 13 Benchmarks

| Finding | March 13 | March 14 (with GE channel) |
|---------|----------|----------------------------|
| METTLE block reliability | 58-100% at k=10 | 0-70% at k=55 (worse at realistic block sizes) |
| METTLE window reliability | Not tested | 0-6% (very poor) |
| RLC block reliability | Not tested | **100%** at DC/WiFi/LTE (matches RaptorQ) |
| RLC decode speed | Not tested | 59 µs at low loss, 2.4 ms at high loss |
| Streaming reliability | Not tested | 12-43% (undertrained) |

### Production Backend Selection

1. **RaptorQ** — remains the production default. Near-optimal recovery
   (100%) with manageable overhead (25%). Decode is fast at low loss
   (62 µs) and gracefully degrades under congestion.

2. **RLC (block)** — promoted to tier-2 alternative. Identical recovery
   to RaptorQ at low-medium loss. Avoid for congested scenarios where
   GF(256) decode cost spikes.

3. **RLC (window)** — recommended for true streaming use cases where
   block boundaries are undesirable. 100% recovery, but decode is
   expensive (8-26 ms).

4. **METTLE** — demoted further. Block mode only viable at k ≤ 10.
   Window mode not production-ready with current config. Speed advantage
   is real but irrelevant without reliability.

5. **RS** — niche. Only when external interop mandates it.

6. **Streaming** — needs parameter tuning before production use.
   Target multiplier should be 2-3x, not 1.15x.

---

## 6. Reproduction

```bash
# Criterion benchmarks (HTML reports → target/criterion/)
raptorpath/cargo.sh bench --bench fec_realworld_bench

# Recovery rate comparison
raptorpath/cargo.sh test --test fec_realworld_recovery_test -- --nocapture

# Raw output from this run
cat raptorpath/docs/benchmark-realworld-raw-2026-03-14.txt
```

HTML reports viewable at `target/criterion/report/index.html`.
