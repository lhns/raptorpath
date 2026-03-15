# Algorithm Competitive Analysis for raptorpath

This document evaluates FEC algorithms, scheduling strategies, and control algorithms
used or considered for raptorpath. It covers erasure codes (sections 1-6), non-FEC
algorithms (section 7), scheduling & control approaches (section 8), and streaming
codes (section 9).

For architectural context, see [DESIGN.md](../DESIGN.md) (Research section).
For benchmark data, see [benchmark-results-2026-03-13.md](benchmark-results-2026-03-13.md)
and [benchmark-analysis-2026-03-13.md](benchmark-analysis-2026-03-13.md).

---

## 1. Evaluation Framework

Any candidate FEC algorithm must satisfy raptorpath's trait interface constraints:

| Constraint | Requirement | Why |
|------------|-------------|-----|
| **Systematic** | Source symbols sent unmodified | Zero-loss fast path skips decoder entirely |
| **Rateless / fountain** | `repair_symbols(count)` with arbitrary count | Rate controller adjusts repair budget dynamically; or fakeable by pre-generating a max repair budget |
| **Block-based API** | Encode a chunk of k source symbols | Current pipeline: block assembly -> FEC encode -> multipath schedule |
| **Small block sizes** | k = 2 to 54 | Most codes optimize for k >> 100; we need performance at small k |
| **Symbol-at-a-time decode** | `add_symbol()` in arbitrary order | Symbols arrive out-of-order across multiple paths |
| **Specified overhead (epsilon)** | Codec overhead known at construction time | Rate controller math depends on predictable overhead |
| **Patent-clean** | No encumbered IP for production use | METTLE is patent-encumbered, limiting its deployment |

Algorithms that fail any hard constraint are listed in Tier 4 (Incompatible) for
completeness but not analyzed further.

---

## 2. Incumbent Summary

### RaptorQ (production default)

- **Encode**: O(k) — LDPC pre-code + LT code. 171-655 us for 4-64 KB blocks.
- **Decode**: O(k^2) — peeling first, Gaussian elimination fallback. 114-503 us with 5% loss.
- **Overhead**: ~1% (near-MDS). Decode succeeds with high probability given k(1+0.01) symbols.
- **Systematic fast path**: 1.8-61 us (no-loss decode).
- **Strengths**: near-optimal overhead, truly rateless, battle-tested `raptorq` crate, patent-free.
- **Weakness**: slow encode (4-54x slower than METTLE), GE fallback dominates decode cost.

### METTLE (realtime candidate)

- **Encode**: O(k) — binomial edge XOR into bins. 4.8-164 us (4-54x faster than RaptorQ).
- **Decode**: O(k) — pure peeling, no GE fallback. 9.2-52 us with 5% loss (7-12x faster).
- **Overhead**: ~15% at production loss rates. Peeling cascade stalls when w/k ratio is low.
- **Systematic fast path**: 2.2-101 us (graph construction overhead even without loss).
- **Strengths**: extreme encode/decode speed, pure GF(2) XOR operations.
- **Weaknesses**: unreliable at small k (w/k=1.0 -> 0.5-67% decode success), 15% overhead
  wastes bandwidth, patent-encumbered (Georgia Tech filing), no open-source implementation.

**The gap**: RaptorQ is reliable but slow to encode. METTLE is fast but unreliable at
raptorpath's small block sizes and patent-encumbered. Neither provides both speed and
reliability simultaneously.

See [benchmark-results-2026-03-13.md](benchmark-results-2026-03-13.md) for full data.

---

## 3. Candidate Algorithms

### Tier 1: Worth Benchmarking

#### Reed-Solomon (classic, GF(2^8))

**Crate**: [`reed-solomon-erasure`](https://crates.io/crates/reed-solomon-erasure) (production quality)

**Properties**:
- **MDS** (Maximum Distance Separable) — zero overhead. Needs exactly k of any n symbols
  to decode. epsilon = 0%, guaranteed. No probabilistic failure modes.
- **Systematic**: source symbols sent unmodified.
- **NOT rateless**: must choose n (max codeword length) at encode time. However, this is
  fakeable for raptorpath: pre-generate a max repair budget (e.g. n = 2k) and return
  slices on demand. The rate controller already caps repair count per block.
- **Encode**: O(k * r) GF(2^8) multiplications where r = repair symbol count.
  At k=50, r=10: ~500 GF(2^8) muls. With SIMD-optimized tables, this is sub-10 us.
- **Decode**: O(k^2) via Berlekamp-Massey or matrix inversion.
  At k=50: ~2500 GF(2^8) muls. Comparable to or faster than RaptorQ's GE fallback.
- **Patent-free**: Reed-Solomon patents expired decades ago.

**Why interesting**: could replace METTLE as the Realtime backend.

- Same speed class as METTLE at small k (GF(2^8) arithmetic is fast with SIMD).
- 100% decode success with optimal overhead (0%) — eliminates METTLE's cascade stall
  failure mode entirely.
- Production Rust crate exists — no new implementation needed.
- Patent-free — removes METTLE's IP encumbrance.

**Limitations**:
- Max repair count fixed at encode time. If loss exceeds the provisioned budget (n-k),
  no more repair available without re-encoding the block. In practice, raptorpath's rate
  controller already limits repair generation, so this is rarely a constraint.
- At very large k (>> 256), RS encode becomes expensive. Not a concern at k <= 54.
- GF(2^8) limits n to 255 symbols total. At k=54, this allows up to 201 repair symbols
  (4x redundancy) — more than sufficient for any realistic loss scenario.

**Recommended action**: benchmark against RaptorQ and METTLE at k=2..54. If RS matches
METTLE's encode speed and RaptorQ's decode reliability (expected at small k), it becomes
the default Realtime backend and a strong candidate for replacing METTLE entirely.

---

#### Sliding Window Random Linear Coding (RFC 8681)

**Crate**: none (requires ~500-1000 LOC new Rust implementation, or C FFI to
[swif-codec](https://github.com/irtf-nwcrg/swif-codec) reference)

**Properties**:
- **Random linear code over GF(2^8)**: each repair symbol is a random linear combination
  of source symbols in the current window. Coefficients drawn from GF(2^8) PRNG seeded
  by repair symbol ID.
- **Systematic**: source symbols sent unmodified.
- **Rateless**: each repair uses fresh random coefficients. Unlimited repair generation
  with no pre-commitment.
- **Encode**: O(W) GF(2^8) multiplications per repair symbol (W = window size).
- **Decode**: O(W^3) Gaussian elimination over GF(2^8).
  At W=50: ~125K GF(2^8) muls. Steinwurf benchmarks: 56 Gbps decode at K=16, 3.68 Gbps
  at K=500. Expected sub-100 us at our window sizes.
- **Overhead**: ~0% with high probability. Random codes over GF(2^8) are MDS-like — the
  probability of a rank-deficient random matrix over GF(256) is ~1/256 per excess symbol.
- **Patent-free**: IETF standard (RFC 8681), no patent claims.

**Why interesting**: the only candidate that is both rateless AND serves as the foundation
for raptorpath's future sliding-window architecture (see DESIGN.md medium-term roadmap).

- Testing it in block mode today (window = block) answers whether GF(2^8) Gaussian
  elimination at k=50 beats RaptorQ's LDPC+GE hybrid.
- If it performs well in block mode, the same code becomes the sliding-window encoder
  when the pipeline evolves — no throwaway work.
- Near-MDS overhead (~0%) matches Reed-Solomon and beats both incumbents.
- Truly rateless, unlike Reed-Solomon.

**Limitations**:
- No production Rust crate. Requires new implementation (~500-1000 LOC for the GF(2^8)
  arithmetic + matrix operations + PRNG-based coefficient generation).
- O(W^3) decode is theoretically worse than RaptorQ's O(k^2), but at W=50 the constant
  factors make this irrelevant.
- The reference C implementation (swif-codec) is research-prototype quality.

**Recommended action**: implement a minimal block-mode RLC backend and benchmark against
RaptorQ. This serves double duty: competitive FEC backend today, sliding-window foundation
tomorrow.

---

### Tier 2: Worth Watching

#### Online Codes (Maymounkov & Mazieres, 2002)

- **Rateless, systematic**. Pure XOR (GF(2)), peeling decode like METTLE.
- Simpler than LT codes — no pre-code needed. Direct outer+inner encoding.
- **Overhead**: ~5-10% (worse than RaptorQ's 1%, better than METTLE's 15%).
- **Unmaintained Rust crate**: `online-codes` — last updated years ago.
- **Narrow niche**: occupies the middle ground between RaptorQ and METTLE on the
  speed/overhead tradeoff curve. Likely dominated by RaptorQ in practice (worse overhead,
  similar decode guarantees) and by Reed-Solomon at small k (RS is both faster and
  optimal-overhead).
- **When relevant**: only if both RaptorQ and RS prove too slow AND METTLE's patent
  situation blocks deployment. Unlikely given RS performance at small k.

#### Streaming Codes (Badr/Martinian, 2017)

- **Rate-optimal** for burst+random erasure channels with delay constraints.
  Proven streaming capacity C(T,B) = T/(T+B).
- **Fixed-rate**: requires a known channel model (burst length B, delay constraint T).
  Parameters derived dynamically from GE HMM estimator.
- **Now implemented** as `FecBackend::Streaming` (ADR-0027). Two-layer construction:
  burst layer (diagonal XOR interleaving) + random layer (GF(256) linear combinations).
- **Sliding window** — operates via `WindowEncoder`/`WindowDecoder` traits.
- **When to use**: Realtime traffic on bursty channels (GE burst_length > 2). Selected
  via `--fec-backend streaming`.
- See section 9 below for detailed analysis and parameter formulas.

---

### Tier 3: Dominated

These algorithms are well-known but strictly inferior to at least one incumbent or Tier 1
candidate for raptorpath's use case.

| Algorithm | Why Dominated |
|-----------|---------------|
| **Raptor10** (RFC 5053) | Strictly worse than RaptorQ: ~2% overhead vs ~1%, same complexity class. RaptorQ (RFC 6330) is the direct successor. |
| **LT Codes** (Luby, 2002) | Catastrophic overhead at small k due to coupon collector effect (30-50%). RaptorQ fixes this with its LDPC pre-code. Only competitive at k >> 10,000. |
| **Tornado Codes** | Fixed-rate (not rateless), poor overhead (~5%), requires bipartite graph expansion. Superseded by Raptor/RaptorQ. |
| **Leopard-RS** (FFT-based RS) | FFT advantage requires k >> 256 to amortize transform overhead. At k <= 54, classic GF(2^8) RS is faster. Irrelevant for raptorpath's block sizes. |
| **LDPC (standalone)** | Not rateless. As a fountain code substrate, already captured by RaptorQ's design. Using LDPC alone loses the rateless property. |

### Tier 4: Incompatible

These algorithms operate at the wrong abstraction level or solve a different problem.

| Algorithm | Why Incompatible |
|-----------|------------------|
| **Spinal Codes** | Designed for bit-level errors on AWGN channels, not packet erasures. Wrong error model. |
| **Convolutional / Turbo Codes** | Physical-layer codes for bit-level soft decoding. Wrong abstraction entirely — raptorpath operates on packet erasures. |
| **BATS Codes** | Multi-hop network coding for relay networks. Overkill and architecturally wrong for point-to-point tunnels. |
| **Polar Codes** | Channel coding for bit-level errors with successive cancellation. Not applicable to packet erasure channels. |

---

## 4. Competitive Matrix

| Algorithm | Rateless | Systematic | Encode | Decode | Overhead (epsilon) | Rust Crate | Patent-Free | Window Mode | Verdict |
|-----------|----------|------------|--------|--------|--------------------|------------|-------------|-------------|---------|
| **RaptorQ** | Yes | Yes | O(k) | O(k^2) | ~1% | `raptorq` (prod) | Yes | No | Incumbent default |
| **METTLE** | Yes | Yes | O(k) | O(k) | ~15% | `mettle` (research) | **No** | Yes | Incumbent realtime |
| **Reed-Solomon** | No* | Yes | O(k*r) | O(k^2) | **0%** | `reed-solomon-erasure` (prod) | Yes | No | **Benchmark** |
| **SW-RLC** | Yes | Yes | O(W) | O(W^3) | ~0% | `rlc_backend.rs` (impl) | Yes | Yes | **Implemented** |
| **Streaming** | No | Yes | O(W) | O(W) | Optimal | `streaming.rs` (impl) | Yes | Yes | **Implemented** |
| Online Codes | Yes | Yes | O(k) | O(k) | ~5-10% | Unmaintained | Likely | — | Watch |

\* Fakeable by pre-generating max repair budget. GF(2^8) limits total symbols to 255,
allowing up to 201 repair symbols at k=54 (more than sufficient).

---

## 5. Recommendations

### Completed

- **Reed-Solomon**: implemented as `FecBackend::ReedSolomon` (ADR-0021, ADR-0022).
  MDS-optimal (0% overhead), production `reed-solomon-erasure` crate. Available via
  `--fec-backend rs`.
- **Sliding Window RLC**: implemented as `FecBackend::Rlc` with both block-mode
  (`RlcEncoder`/`RlcDecoder`) and window-mode (`RlcWindowEncoder`/`RlcWindowDecoder`).
  Near-MDS, truly rateless, GF(2^8) Gaussian elimination. Available via `--fec-backend rlc`.
- **Streaming Codes**: implemented as `FecBackend::Streaming` (ADR-0027). Two-layer
  construction (burst XOR + random GF(256)). Available via `--fec-backend streaming`.

### Current recommendation

| Use case | Backend | Why |
|----------|---------|-----|
| Default / general | RaptorQ | Battle-tested, near-optimal, truly rateless |
| Realtime (low burst) | RLC or RS | Low overhead, fast encode/decode |
| Realtime (bursty channel) | Streaming | Burst-aware diagonal interleaving, delay-optimal |
| Bulk transfer | RaptorQ | Tolerant of retransmission-based recovery |

### What to drop

**METTLE** should be deprecated. RS matches METTLE's encode speed at small k while
eliminating METTLE's three critical weaknesses: unreliable decode at small k, 15% overhead
waste, and patent encumbrance.

---

## 6. Sliding Window Architecture Note

The sliding-window FEC pipeline is now implemented alongside the block-based pipeline.
Both coexist — the backend is selected at runtime via `FecBackend`:

- **Block pipeline**: RaptorQ, METTLE, RS, RLC (block mode). Accumulate k symbols, encode.
- **Window pipeline**: RLC (window mode), METTLE (window mode), Streaming. Continuous
  encoding over a sliding window — no block assembly delay.

The `WindowEncoder`/`WindowDecoder` traits (ADR-0022) abstract the window-mode interface.
Three window backends are implemented:

| Backend | Encoder | Decoder | Recovery |
|---------|---------|---------|----------|
| RLC | `RlcWindowEncoder` | `RlcWindowDecoder` | GF(256) Gaussian elimination |
| METTLE | `MettleWindowEncoder` | `MettleWindowDecoder` | Peeling decoder |
| Streaming | `StreamingEncoder` | `StreamingDecoder` | Burst XOR + GF(256) GE |

Reed-Solomon is fundamentally block-based and cannot evolve into sliding window.

The evolution path:

```
Current:    RaptorQ (default) + RS/RLC/Streaming (window modes available)
Near-term:  RaptorQ (bulk) + Streaming (realtime on bursty channels)
Long-term:  SW-RLC or Streaming for all modes, block pipeline retired
```

---

## 7. Implemented Non-FEC Algorithms

These algorithms are implemented in raptorpath but are not erasure codes — they are
estimation, control, and delivery algorithms that feed into or complement the FEC layer.

| Algorithm | ADR | Status | What it does |
|-----------|-----|--------|-------------|
| Gilbert-Elliott HMM | [0023](adr/0023-gilbert-elliott-loss-model.md) | Implemented | Two-state Markov chain for bursty loss estimation. Feeds `burst_factor` into FEC rate controller and `mean_burst_length` into streaming code params. |
| BBR ProbeRTT | [0024](adr/0024-bbr-probe-rtt-phase.md) | Implemented | Periodic min_rtt re-measurement (10s interval, 200ms hold, cwnd=4). Prevents min_rtt drift from standing queues. |
| Beta-Binomial EWMA | — | Implemented | Bayesian loss estimation with uncertainty bounds. Uses 95th percentile upper bound for FEC rate computation. Decays via `beta_decay` for recency weighting. |
| Binomial feedforward + PI feedback | — | Implemented | Hybrid FEC rate controller: statistical model (Newton's method on normal CDF constraint) + error-correcting PI loop (Kp=2.0, Ki=0.5, anti-windup). |
| WindowNack targeted repair | [0025](adr/0025-window-nack-sender-repair.md) | Implemented | NACK-driven repair symbols for specific gaps in sliding window mode. Cooldown-throttled (5ms), capped per-NACK. |
| Reorder buffer | — | Implemented | Sequence-ordered delivery with 20ms timeout, 500-entry capacity. Handles out-of-order arrival from multipath scheduling. |
| Multipath window scheduling | [0026](adr/0026-multipath-window-scheduling.md) | Implemented | Per-symbol RTT/goodput-aware path selection for window mode. Redundant source scheduling for Realtime. |

---

## 8. Scheduling & Control Approaches

Approaches to multipath scheduling, congestion control, and FEC strategy — evaluated
using the same tier structure as FEC algorithms.

### Tier 1 — Implement

| Approach | Status | Why | Reference |
|----------|--------|-----|-----------|
| Multipath window scheduling | Implemented (ADR-0026) | Window mode uses per-symbol RTT/goodput scheduling matching block mode. Source → lowest RTT, repair → highest goodput. | — |
| Redundant scheduling | Implemented (ADR-0026) | Duplicate source symbols on 2nd path for Realtime. Halves tail latency at 2x bandwidth. Receiver dedup handles copies. | Barre et al. 2011 |
| Hybrid proactive/reactive FEC | Planned | Proactive repair before feedback + targeted retransmission after ACKs. Scheduling change, not coding change. | MPLOT (Sharma et al. 2008) |
| Cross-path retransmission | Planned | Retransmit lost symbols on alternative path. Part of reactive phase. | Cloud et al. 2013 |

### Tier 2 — Worth Watching

| Approach | Why watching | Reference |
|----------|-------------|-----------|
| BLEST (Blocking Estimation) | Skip slow paths that would stall receiver. Relevant when path RTTs differ >5x. | Ferlin et al. 2016 |
| Pacing / burst smoothing | Token-bucket pacer to prevent WiFi buffer bloat from cwnd bursts. | BBRv2 spec |
| ACK aggregation compensation | Min-filter for wireless RTT jitter from AP/base station ACK aggregation. | — |
| Proactive retransmission | Send repair on path B before timeout when path A's ACK deadline expires. Tail latency optimization. | Barre et al. 2011 |
| Priority-aware / unequal FEC | DPI-based traffic classification (TCP SYN, DNS, video I-frames get stronger FEC). | — |
| RL scheduling | Reinforcement learning for multipath scheduling policies. Needs deployment data. | Wu et al. (ReMP) 2020 |

---

## 9. Streaming Codes — Detailed Analysis

### Theory

Streaming codes (Martinian & Sundberg 2004, Badr et al. 2017) are the theoretical optimum
for erasure channels with both burst and random loss under a delay constraint.

**Channel model**: a sliding-window erasure channel where:
- Bursts of up to B consecutive erasures can occur
- Random (isolated) erasures occur with probability ε
- The decoder must recover each source symbol within T time steps of its transmission

**Streaming capacity** (Badr 2017, burst-only):

```
C(T, B) = T / (T + B)
```

**Generalized capacity** (Fong et al. 2019, burst OR N arbitrary erasures in window T+1):

```
C(T, B, N) = (T - N + 1) / (T - N + 1 + B)
```

where T is the decoding delay, B is the max burst length, and N is the max number of
arbitrary (random) erasures in any window of T+1 slots. Constraint: T ≥ B + N - 1
(otherwise rate is zero — not enough non-erased slots to carry data).

Block codes cannot match this within the same delay constraint because the block boundary
wastes part of the delay budget.

**Worked example**: T=10, B=5, N=2 → C = (10-2+1)/(10-2+1+5) = 9/14 = 0.643.
For every 9 source symbols, 5 repair symbols are needed. A burst of 5 OR 2 arbitrary
erasures in any window of 11 slots is fully recoverable.

### Layered Construction

The rate-optimal code uses two independent layers:

**Burst layer** (diagonal interleaving):
- Source symbols are grouped into T independent diagonals with stride T
- Each diagonal produces one XOR repair = ⊕ of all symbols in that diagonal
- A burst of B ≤ T symbols hits at most ⌈B/T⌉ = 1 symbol per diagonal
- Recovery: if one symbol is missing from a diagonal, XOR the others
- Rate cost: 1/T repair symbols per source symbol
- Operations: pure XOR (GF(2))

**Random layer** (GF(256) linear combinations):
- Each repair is a random linear combination of all window symbols
- Handles isolated random losses not caught by the burst layer
- Rate: ε/(1-ε) repair symbols per source symbol
- Recovery: incremental Gaussian elimination (same as RLC decoder)

The two layers are independent — burst recovery can cascade into random-layer pivots.

### Parameter Mapping

| Parameter | Source | Formula |
|-----------|--------|---------|
| T (delay) | Profile-based or max path RTT | T ≥ B + N - 1 (Fong et al. minimum). Default: T = B. |
| B (burst tolerance) | `ge.mean_burst_length() × safety` | Safety factor: 1.15 (Realtime), 1.10 (other) |
| N (random erasures/window) | `estimator.loss_rate_upper(0.95) × (T+1)` | Expected random losses per window |
| ε (random loss rate) | `estimator.loss_rate_upper(0.95)` | 95th percentile upper bound |
| Burst repair rate | 1/T | One diagonal XOR per T source symbols |
| Random repair rate | ε/(1-ε) | Compensate for residual random loss |
| Code rate | (T-N+1)/(T-N+1+B) | Fong et al. generalized capacity |
| Redundancy fraction | B/(T-N+1+B) | Must be < 1 for positive rate |

### Connection to GE Estimator

The Gilbert-Elliott HMM (ADR-0023) provides the key parameters:
- `ge.mean_burst_length()` → B (with safety margin)
- `ge.is_valid()` → whether burst structure is detected
- `estimator.loss_rate_upper(0.95)` → ε

When GE detects bursty loss (burst_length > 2), streaming codes are the natural choice:
the burst layer is structurally matched to the channel's burst behavior, unlike RLC which
treats all loss as random.

### Multi-path Considerations

For multipath scenarios with heterogeneous paths:

- **Single global T** for end-to-end deadline, not per-path T values. The receiver sees
  an interleaved sequence from all paths, so a single T covering the overall delay budget
  is correct.
- **Per-path effective delay**: T_j = T_global - RTT_j. Paths with higher RTT get smaller
  effective delay budgets, hence need higher redundancy for the same protection level.
- **Per-path loss parameters**: each path j has its own (N_j, B_j) from its GE estimator.
  Per-path rate R_j = (T_j - N_j + 1) / (T_j - N_j + 1 + B_j).
- **B should cover worst-case burst across all paths**: a burst on one path appears as a
  gap in the merged sequence at the receiver.

**Research findings:**

Facenda, Krishnan et al. (2022) introduced the **delay spectrum** concept for multi-link
streaming: each link has its own propagation delay and erasure parameter N_j. The scheme
allocates coding rates per-link based on this delay spectrum. This is the closest
theoretical framework for raptorpath's multipath scenario.

Fong & Khisti (2020) characterized the rate region for multi-hop relay streaming. Key
insight: the achievable scheme uses **symbol-wise decode-forward** — symbols within the
same message are decoded with different delays, allowing partial early forwarding.

Fong et al. (2019) extended Badr's results to adversarial erasure patterns (not just
burst+random). Their model handles worst-case reordering — relevant for multipath where
path diversity creates reordering patterns that look adversarial to single-path analysis.
Key result: the layered construction remains rate-optimal under the adversarial model when
T ≥ B + N - 1 where N is the adversarial window parameter.

### Practical Implementations

**Tambur** (Rudow et al., NSDI 2023) is a production-quality streaming code implementation
for videoconferencing:
- V/U split construction: splits each frame into two halves, protects with different delays
- ML-based bandwidth overhead prediction for adaptive rate selection
- Results: 26% fewer decoding failures, 35% less bandwidth for redundancy vs baseline FEC
- Open-source: [github.com/Thesys-lab/tambur](https://github.com/Thesys-lab/tambur)

**Staggered Diagonal Embedding (SDE)** (Krishnan & Ramkumar, IEEE Comm. Letters 2020):
- Simplest known rate-optimal construction with linear field size
- Disperses n code symbols across N ≥ n successive packets
- Achieves optimal rate without the algebraic complexity of Badr-Khisti layered codes
- Some binary (GF(2)) rate-optimal codes identified

### Over-provisioning Strategy

In practice, the channel model is estimated, not known exactly. Safety margins:

1. **B over-provisioning**: multiply GE burst length by 1.15-1.2x
2. **ε over-provisioning**: use 95th percentile upper bound (already done)
3. **Rate headroom**: add 10-15% to the total repair rate
4. **Graceful degradation**: when burst > B, the burst layer partially fails but the random
   layer can still recover some symbols. The code doesn't crash — it just loses some symbols.

### Key References

- **Badr, Patil, Tan, Dey** — "Layered Constructions for Low-Delay Streaming Codes,"
  IEEE Trans. IT, 2017 ([arXiv:1308.3827](https://arxiv.org/abs/1308.3827)). Proves
  streaming capacity C(T,B) = T/(T+B) and gives the layered burst+random construction.
  The foundational result for our implementation.
- **Martinian & Sundberg** — "Burst Erasure Correction Codes with Low Decoding Delay,"
  IEEE Trans. IT, 2004. Original streaming erasure codes with delay guarantees.
- **Fong, Khisti, Li, Tan** — "Optimal Streaming Codes for Channels with Burst and
  Arbitrary Erasures," IEEE Trans. IT, vol. 65 no. 7, 2019
  ([arXiv:1801.04241](https://arxiv.org/abs/1801.04241)). Generalizes to adversarial
  model with C(T,B,N) = (T-N+1)/(T-N+1+B). Proves layered construction remains optimal
  when T ≥ B + N - 1.
- **Dudzicz, Fong, Khisti** — "Explicit Construction of Optimal Streaming Codes,"
  2019 ([arXiv:1903.07434](https://arxiv.org/abs/1903.07434)). Systematic construction
  using off-the-shelf MDS + MRD (Gabidulin) codes, valid when rate ≥ 1/2.
- **Facenda, Krishnan et al.** — "Error-correcting codes for low latency streaming over
  multiple link relay networks," 2022 ([arXiv:2201.06609](https://arxiv.org/abs/2201.06609)).
  Introduces delay spectrum concept for per-link rate allocation in multi-path streaming.
- **Fong & Khisti** — "Streaming Erasure Codes over Multi-hop Relay Network," 2020
  ([arXiv:2006.05951](https://arxiv.org/abs/2006.05951)). Per-hop erasure model; symbol-wise
  decode-forward scheme.
- **Krishnan & Ramkumar** — "Simple Streaming Codes for Reliable, Low-Latency Communication,"
  IEEE Comm. Letters, vol. 24 no. 2, 2020. Staggered diagonal embedding (SDE): simplest
  rate-optimal construction with linear field size.
- **Rudow et al.** — "Tambur: Efficient loss recovery for videoconferencing via streaming
  codes," NSDI 2023. Production-quality V/U split construction with ML-based rate adaptation.
  26% fewer decode failures, 35% less redundancy bandwidth. Open-source at
  [github.com/Thesys-lab/tambur](https://github.com/Thesys-lab/tambur).

---

*Last updated: 2026-03-14*
