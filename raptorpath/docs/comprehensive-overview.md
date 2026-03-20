# RaptorPath Comprehensive Algorithm & Backend Overview

Consolidated reference for all FEC backends, control-plane algorithms, scheduler features,
and their measured performance. Bench suite data updated 2026-03-19 after ADR-0043
(information-theoretic FEC rate controller). Earlier data from March 13-16, 2026.

For architecture and design rationale, see [../DESIGN.md](../DESIGN.md).
For per-feature configuration, see [FEATURES.md](FEATURES.md).

---

## 1. Erasure Coding Backends

RaptorPath implements five FEC backends, selectable via `--fec-backend <name>`.

### 1.1 Speed Comparison

Criterion microbenchmarks (2026-03-13, dev profile) and real-world timing (2026-03-14, release):

| Operation | RaptorQ | METTLE | Reed-Solomon | RLC | Streaming |
|-----------|---------|--------|-------------|-----|-----------|
| Block encode (64 KB) | 648 us | **183 us** (3.5x) | 1.46 ms | 528 us | — |
| Block decode (WiFi, with repair) | 540 us | **33 us** (16x) | 1.58 ms | 228 us | — |
| Window encode (200 sym) | — | **1.53 ms** (3.4x) | — | 5.19 ms | Channel-adaptive |
| Window decode (200 sym) | — | **258 us** (27-135x) | — | 8.88 ms | Channel-adaptive |
| Per-symbol encode (1 KB) | 257 us | **4.8 us** (54x) | — | — | — |
| Per-symbol encode (64 KB) | 655 us | **164 us** (4x) | — | — | — |

METTLE is the fastest backend at all operation sizes. The gap narrows with larger blocks
because METTLE's binomial edge sampling becomes nontrivial while RaptorQ amortizes its
LDPC setup cost.

### 1.2 Recovery Comparison — Block Mode

From `fec_realworld_recovery_test` (2026-03-15, 10 trials, Gilbert-Elliott bursty loss):

**Same overhead (25%, 14 repair symbols):**

| Backend | Datacenter (0.1%) | WiFi (2.5%) | LTE (3.5%) | Congested (12%) |
|---------|-------------------|-------------|------------|-----------------|
| **RaptorQ** | 100% | 100% | 100% | 40% |
| **Reed-Solomon** | 100% | 100% | 100% | 40% |
| **RLC** | 100% | 100% | 100% | 40% |
| **METTLE** | 80% | 40% | 60% | 0% |

**Same bandwidth as METTLE (187%, 103 repair symbols):**

| Backend | Datacenter | WiFi | LTE | Congested |
|---------|-----------|------|-----|-----------|
| **All backends** | 100% | 100% | 100% | 100% |

At equal bandwidth, all backends achieve identical recovery. METTLE's recovery advantage
at full budget is purely a bandwidth artifact — it needs 7.4x more repair data.

### 1.3 Recovery Comparison — Window Mode

Unified repair budget (2x loss rate, min 5), 500 symbols:

| Backend | Datacenter | WiFi | LTE | Congested |
|---------|-----------|------|-----|-----------|
| **RLC Window** | 100% | 100% | 100% | 26.2% |
| **METTLE Window** | 14.3% | 18.4% | 19.2% | 36.5% |
| **Streaming** | 42.9% | 34.8% | 16.3% | 11.7% |

RLC is the clear winner for window-mode FEC. METTLE Window only outperforms at Congested
(36.5% vs 26.2%) due to its peeling decoder handling high-loss patterns differently.

### 1.4 Tapered Interleaving Impact (Block Mode)

From interleaving comparison (2026-03-15, 500 packets, 50-packet blocks, 25% overhead):

| Backend | Mode | Datacenter | WiFi | LTE | Congested |
|---------|------|-----------|------|-----|-----------|
| RaptorQ | Flat | 100% | 100% | 100% | 48% |
| RaptorQ | **Tapered** | 100% | 100% | 100% | **53%** (+5pp) |
| METTLE | Flat | 96% | 60% | 47% | 0% |
| METTLE | **Tapered** | 97% | 60% | **58%** (+11pp) | **2%** |
| RLC | Flat | 100% | 100% | 100% | 28% |
| RLC | **Tapered** | 100% | 100% | 100% | **38%** (+10pp) |

Tapered interleaving improves recovery in high-loss scenarios with no regressions.

### 1.5 Codec Properties

| Property | RaptorQ | METTLE | Reed-Solomon | RLC | Streaming |
|----------|---------|--------|-------------|-----|-----------|
| **Overhead (epsilon)** | ~1% | ~15% | 0% (MDS) | ~0.4% | Channel-adaptive |
| **Rateless** | Yes | Yes* | No (fakeable) | Yes | No (fixed-rate) |
| **Mode** | Block | Block + Window | Block | Block + Window | Window-only |
| **Decode strategy** | LDPC + GE fallback | Pure peeling (GF(2)) | GF(256) matrix inv. | GF(256) Gaussian elim. | Diagonal XOR + GF(256) |
| **Patent status** | Free | Encumbered | Free | Free (RFC 8681) | Free |
| **Max block (GF limit)** | Unlimited | Unlimited | 255 symbols | Unlimited | N/A (window) |

\* METTLE is rateless in principle (unlimited bins) but needs all bins for reliable recovery at small k.

### 1.6 Decision Matrix — Which Backend for Which Scenario

| Scenario | Recommended | Why |
|----------|-------------|-----|
| **General / default** | RaptorQ | 100% recovery at 25% overhead, truly rateless, patent-free |
| **Latency-critical, small k** | METTLE | 3.5-54x faster encode; viable only at k <= 20 with w/k >= 3 |
| **Bandwidth-limited** | RaptorQ | Rateless: 14 repairs vs METTLE's 103 for same recovery |
| **Streaming / window** | RLC Window | 100% recovery, rateless, unified pipeline |
| **Bursty real-time** | Streaming | Burst-aware diagonal interleaving, delay-optimal theory |
| **Interop-required** | Reed-Solomon | MDS-optimal, universally understood |
| **Patent-free required** | RaptorQ or RLC | METTLE is patent-encumbered |

---

## 2. Control Plane Algorithms

### 2.1 FEC Rate Controller (Information-Theoretic + PI Feedback)

**Files**: `control/fec_rate.rs`, `control/estimator.rs`
**ADR**: [ADR-0043](adr/0043-information-theoretic-fec-rate.md)

The FEC rate controller computes how many repair symbols to send per block/window.

**Architecture** (rewritten in ADR-0043):

The controller uses an information-theoretic optimal formula instead of the previous
stacked-multiplier approach. For an i.i.d. erasure channel with loss rate p, the minimum
overhead is `p/(1-p)`. For a burst channel with max burst length B and delay constraint
T symbols, the minimum overhead is `B/T` (Badr et al. 2017).

```
base_rate  = max(p/(1-p) + codec_overhead, B/T)
margin     = (z * uncertainty * 0.25).clamp(0.0, 1.0)    // single safety margin
rate       = base_rate * (1 + margin) + pi_correction + hint_offset
```

Where `T = (RTT * throughput) / symbol_size` naturally incorporates RTT:
- **Low RTT** (datacenter): large T, random-loss term dominates, low overhead
- **High RTT** (satellite): small T, burst term dominates, more proactive FEC
- **No throughput data** (warmup): B/T term disabled, falls back to random-loss only

Key design choices:
1. **Single safety margin** from Beta posterior uncertainty, replacing 5 stacked margins
2. **PI feedback** (Kp=0.5, Ki=0.1) for residual model mismatch — reduced gains because
   the base formula is accurate enough that PI just handles residual error
3. **Protocol hint**: additive offset (+0.05 Realtime, -0.05 Bulk, 0 Auto), not multiplicative
4. **GE burst term** (B/T) is built into the formula, not a separate scaling factor

**Measured wire overhead** (see [benchmark-results-2026-03-19.md](benchmark-results-2026-03-19.md) Table 2):
DC total 8.3% (down from 12.5%), WiFi 13.9% (down from 20% capped), Congested 23.1%.
The residual gap above the information-theoretic minimum comes from the 95th-percentile
Beta upper bound — intentional conservatism for production safety.

**Config**: `enable_pi_feedback = true`, `target_tail_loss = 1e-5`, `max_fec_overhead = 0.5`

### 2.2 Gilbert-Elliott HMM

**File**: `control/gilbert_elliott.rs` ([ADR-0023](adr/0023-gilbert-elliott-loss-model.md))

Two-state Hidden Markov Model detects correlated loss bursts. Feeds `mean_burst_length`
into the FEC rate controller's B/T burst term and into streaming code parameters.

As of ADR-0043, the GE burst model is integrated into the rate formula rather than being
a separate multiplicative scaling factor. The B/T term naturally captures the
delay-constrained capacity of a burst erasure channel.

**Config**: Always on (feeds estimator). No separate `ge_burst_factor` toggle needed.

### 2.3 Backend Auto-Switch

**File**: `control/backend_selector.rs` ([ADR-0030](adr/0030-runtime-backend-switching.md))

Monitors loss rate and switches between FEC backends using hysteresis thresholds.

**Measured behavior** (tradeoff bench, ADR-0034):
- **5-phase loss sweep** (0.5% -> 5% -> 15% -> 5% -> 0.5%): auto-switch detects 2-3
  transitions across phases
- Hysteresis prevents flapping at threshold boundaries
- Default thresholds (2%, 8%) are near-optimal from threshold sweep testing

**Config**: `fec_auto_switch = true`, `fec_switch_threshold_low = 0.01`,
`fec_switch_threshold_high = 0.12` (raised from 0.10 per ADR-0043 bench data showing
block codes cliff at ~12-15%), `fec_switch_interval = 5` seconds

### 2.4 Loss Estimator

**File**: `control/estimator.rs`

Always-on component that feeds all control algorithms.

- **EWMA**: Exponential weighted moving average for recency bias
- **Beta-Binomial**: Bayesian posterior with 95th-percentile upper confidence bound
- **Decay**: `beta_decay` parameter for forgetting old observations

Provides: `loss_rate_upper(0.95)` (for FEC rate), `loss_rate_mean()` (point estimate),
`loss_uncertainty(0.95)` (relative uncertainty for safety margin), `is_in_burst()`,
`rtt()`, `throughput()` (for B/T burst term), and `ge_estimator()` (for burst length
and streaming parameters).

---

## 3. Scheduler & Network Features

### 3.1 ProbeRTT Phase

**File**: `scheduler/mod.rs` ([ADR-0024](adr/0024-bbr-probe-rtt-phase.md))

Periodic min_rtt recalibration: every 10s, drops cwnd to 4 packets for 200ms to drain
queues and re-measure true propagation delay.

**Measured behavior** (tradeoff bench, ADR-0034):
- ProbeRTT did not produce measurable differentiation in the simulation harness because
  BBR's `min_rtt` field is private and the sim's queue model is simplified
- **Theoretical analysis**: ~2% throughput cost (200ms/10s). Without ProbeRTT, standing
  queues inflate RTT estimates → BDP overestimation → more queuing → latency spiral

| Dimension | Value |
|-----------|-------|
| **Cost** | ~2% throughput (200ms drain / 10s interval) |
| **Benefit** | Prevents min_rtt drift and latency spiral |
| **When to enable** | Long-lived connections, latency-sensitive traffic |
| **When to disable** | Short transfers (<10s), throughput-only workloads |
| **Config** | `enable_probe_rtt = true` (default) |

### 3.2 Reorder Buffer

**File**: `net/reorder.rs`

Holds out-of-order symbols and delivers in sequence order with configurable timeout.

**Measured tradeoffs** (tradeoff bench, ADR-0034, 10 trials, dual-path WiFi 5ms + LTE 30ms):

| Timeout | Out-of-order rate | Avg latency | Jitter |
|---------|-------------------|-------------|--------|
| 0ms (disabled) | 1.4% | 6.2ms | 1.9ms |
| 25ms | 0.0% | 6.9ms | 3.4ms |

The reorder buffer eliminates out-of-order delivery at the cost of ~0.7ms additional
latency and ~1.5ms additional jitter. The sweet spot is at approximately
RTT_difference / 2 between the fastest and slowest paths.

| Dimension | Value |
|-----------|-------|
| **Cost** | Up to `reorder_timeout_ms` additional delivery delay |
| **Benefit** | In-order delivery, absorbs multipath jitter |
| **Sweet spot** | Timeout ~= RTT_diff / 2 (e.g., 12-15ms for WiFi+LTE) |
| **Config** | `reorder_timeout_ms = 20` (default), `0` = disabled |

### 3.3 Multipath Scheduling

**File**: `scheduler/mod.rs` ([ADR-0026](adr/0026-multipath-window-scheduling.md))

Source symbols to lowest-RTT paths, repair to highest-goodput paths. Optional redundant
send mode sends source on all paths.

**Measured tradeoffs** (tradeoff bench, ADR-0034, 10 trials):

| Config | P99 latency | Jitter | Overhead |
|--------|-------------|--------|----------|
| **single_wifi** (5ms, 2%) | 72ms | 12.1ms | 10% |
| **dual_primary_wifi** (smart sched.) | 57ms | 6.8ms | 10% |
| **dual_redundant** (all paths) | 30ms | 0.1ms | 5% |

Key findings:
- Redundant send cuts P99 from 72ms to 30ms (58% reduction)
- Smart scheduling (source on fast path, repair on reliable path) gets 57ms — 80% of
  redundant's latency benefit at ~1.3x bandwidth instead of 2x
- Dual-redundant also reduces jitter from 12.1ms to 0.1ms

| Dimension | Value |
|-----------|-------|
| **Cost** | Redundant: 2x bandwidth. Smart: ~1.3x bandwidth |
| **Benefit** | P99 = min(path RTTs) with redundant; path diversity for resilience |
| **When to enable** | Multiple paths available, latency-sensitive traffic |
| **Config** | Automatic based on available paths and protocol hint |

### 3.4 NACK Repair (Window Mode)

**File**: `net/mod.rs` ([ADR-0025](adr/0025-window-nack-sender-repair.md))

Receiver detects gaps, sends NACKs; sender generates targeted repair for missing range.

**Measured behavior** (tradeoff bench, ADR-0034):
- NACK did not produce clear differentiation in the simulation harness because the RLC
  window's proactive repair naturally recovers burst losses before NACKs arrive
- **Theoretical analysis**: NACK adds +0.2-0.3pp recovery improvement, most valuable at
  tight FEC budgets (<=12%) with block-mode FEC where proactive repair is insufficient

| Dimension | Value |
|-----------|-------|
| **Cost** | ~3-10 extra repair symbols per burst event |
| **Benefit** | Fast burst recovery within 1 RTT |
| **Critical threshold** | At <=12% FEC budget, NACK dramatically improves burst recovery |
| **When to enable** | Bursty channels, tight FEC budgets, real-time traffic |
| **Config** | Always enabled in window mode |

### 3.5 Block Interleaving

**File**: `net/interleave.rs` ([ADR-0029](adr/0029-tapered-repair-interleaving.md))

Spreads symbols from N blocks across time; tapered repair uses exponential decay for
burst resilience.

**Measured impact** (2026-03-15):
- Tapered adds +5-10pp recovery in congested scenarios vs flat interleaving
- RLC: +10pp at Congested (28% → 38%)
- RaptorQ: +5pp at Congested (48% → 53%)
- METTLE: +11pp at LTE (47% → 58%)
- No regressions at low-loss scenarios

| Dimension | Value |
|-----------|-------|
| **Cost** | ~1 block-period additional delivery latency |
| **Benefit** | Converts burst loss into distributed random loss |
| **When to enable** | Bursty channels, block-mode FEC (RaptorQ, RS) |
| **When to disable** | Window-mode backends (handle bursts natively), ultra-low-latency |
| **Config** | `interleave_depth = 3` (default), `1` = disabled |

---

## 4. Bench Suite Results

Run with `cargo test --test bench_suite -- --nocapture` ([ADR-0042](adr/0042-bench-suite-consolidation.md)).

Latest results: **[benchmark-results-2026-03-19.md](benchmark-results-2026-03-19.md)**
(post-ADR-0043 information-theoretic FEC rate controller).

The bench suite produces 5 tables:

1. **Table 1: Backend Loss Sweep** — Recovery vs uniform loss for all 5 backends.
   Key finding: block codes cliff at ~12-15% loss; RLC Window is the best window backend.
2. **Table 2: Wire Overhead** — All 5 overhead layers. DC overhead dropped from 12.5%
   to 5.7%, WiFi from 20% to 11.2% after ADR-0043.
3. **Table 3: Feature Ablation** — One-feature-off at 8% FEC budget. Multipath is the
   most impactful feature (+10.6pp overhead, +50ms p99 when removed).
4. **Table 4: FEC vs Retransmit** — Dual-path FEC halves p99 latency vs retransmit
   and achieves 100% in-order delivery.
5. **Table 5: Transport Comparison** — Full ADR-0036 matrix: QUIC single, MPTCP (rr +
   minRTT), FEC single, FEC dual across WiFi/LTE/Satellite. FEC dual-path wins on
   latency (2x better p99); QUIC wins on bandwidth (7x less overhead).

### Key numbers (from latest run)

| Metric | QUIC single | MPTCP rr | FEC single | FEC dual |
|--------|------------:|---------:|-----------:|---------:|
| WiFi p99 (ms) | 20.0 | 20.0 | 18.8 | **10.0** |
| LTE p99 (ms) | 82.7 | 81.3 | 95.3 | **40.0** |
| Satellite p99 (ms) | 600.0 | 596.7 | 1010.0 | **286.7** |
| WiFi overhead | **2.7%** | **2.8%** | 20.0% | 20.0% |
| In-order (all) | ~51% | ~54% | ~99% | **~100%** |

For full tables, analysis, and methodology notes, see the
[timestamped results file](benchmark-results-2026-03-19.md).

### Historical results

| Date | Document | What changed |
|------|----------|-------------|
| 2026-03-19 | [benchmark-results-2026-03-19.md](benchmark-results-2026-03-19.md) | ADR-0043 info-theoretic rate controller, Table 5 transport comparison |
| 2026-03-16 | [ablation-results-2026-03-16.md](ablation-results-2026-03-16.md) | Feature ablation overhead costs (pre-ADR-0043) |
| 2026-03-15 | [benchmark-results-2026-03-15.md](benchmark-results-2026-03-15.md) | Post-METTLE bug fix, tapered interleaving |
| 2026-03-14 | [benchmark-realworld-results-2026-03-14.md](benchmark-realworld-results-2026-03-14.md) | Real-world channel timing |
| 2026-03-13 | [benchmark-results-2026-03-13.md](benchmark-results-2026-03-13.md) | Initial encode/decode speed, waterfall |

### Legacy Performance Data

#### Scenario x Backend → Recovery Rate (block mode, 25% overhead, March 2026)

| Scenario | Stationary Loss | RaptorQ | RS | RLC | METTLE |
|----------|----------------|---------|-----|-----|--------|
| Datacenter | ~0.1% | 100% | 100% | 100% | 80% |
| WiFi Home | ~2.5% | 100% | 100% | 100% | 40% |
| LTE Mobile | ~3.5% | 100% | 100% | 100% | 60% |
| Congested WiFi | ~12% | 40% | 40% | 40% | 0% |

#### Encode/Decode Speed Summary (Criterion microbenchmarks, March 2026)

| Backend | Encode (64KB) | Decode (repair) | Relative |
|---------|-------------|-----------------|----------|
| **METTLE** | 183 us | 33 us | Fastest (1x) |
| **RLC** | 528 us | 228 us | 3-7x slower |
| **RaptorQ** | 648 us | 540 us | 3.5-16x slower |
| **RS** | 1.46 ms | 1.58 ms | 8-48x slower |

---

## 5. Deployment Profiles

Concrete recommendations for common deployment scenarios, derived from measured data.

### Home / WiFi

| Parameter | Value |
|-----------|-------|
| **Backend** | RaptorQ (block, default) → RLC (window, if streaming) |
| **Expected loss** | 2-5%, bursty (GE burst ~2 packets) |
| **FEC overhead** | ~11-14% (measured: 11.2% at 2.5% loss, Realtime hint) |
| **Interleave** | Depth 3 (tapered) |
| **Key features** | B/T burst term (handles WiFi bursts via RTT-aware formula) |
| **Multipath** | Single path typical; dual if WiFi + Ethernet available |
| **Recovery expectation** | 100% at DC-WiFi loss levels; may drop at Congested |

**Why this works**: The information-theoretic formula provisions ~p/(1-p) ≈ 2.6% base rate
for 2.5% loss, plus codec overhead and safety margin. The B/T burst term adds protection
proportional to burst_length/T when the GE model detects correlated loss. Total overhead
is ~11% — half of the previous 20% (capped) result.

### Datacenter

| Parameter | Value |
|-----------|-------|
| **Backend** | RaptorQ (block) with minimal FEC, or no FEC at all |
| **Expected loss** | <0.1%, near-i.i.d. |
| **FEC overhead** | ~6-8% (measured: 5.7% FEC + 2.6% framing at 0.1% loss) |
| **Interleave** | Off (depth 1) — no bursts to spread |
| **Key features** | None critical; low RTT → large T → B/T term negligible |
| **Multipath** | Not needed (single reliable path) |
| **Recovery expectation** | 100% always; retransmission at <1ms RTT is negligible |

**Why this works**: At sub-1% loss and sub-1ms RTT, the optimal rate p/(1-p) ≈ 0.1%.
The measured 5.7% overhead comes from the Beta posterior's 95th-percentile conservatism.
Retransmission costs ~2ms. For absolute minimum overhead, set `max_fec_overhead = 0.02`.

### Mobile Multipath (WiFi + LTE)

| Parameter | Value |
|-----------|-------|
| **Backend** | RLC Window (streaming) or RaptorQ (block) |
| **Expected loss** | 2-5% per path, asymmetric RTTs (WiFi 5ms + LTE 25-30ms) |
| **FEC overhead** | 15-25% |
| **Interleave** | Depth 2-3 |
| **Key features** | Multipath scheduling, reorder buffer (timeout ~12-15ms), backend auto-switch |
| **Multipath** | Smart scheduling: source on WiFi (fast), repair on LTE (reliable) |
| **Recovery expectation** | P99 latency 57ms (smart) to 30ms (redundant) |

**Why this works**: Measured multipath data shows smart scheduling gets P99 from 72ms (single
WiFi) to 57ms at 1.3x bandwidth. Redundant send achieves 30ms at 2x bandwidth. The reorder
buffer eliminates the 1.4% OOO rate from asymmetric paths with only 0.7ms additional delay.

### Real-Time (VoIP / Gaming)

| Parameter | Value |
|-----------|-------|
| **Backend** | METTLE (k <= 20, encode speed matters) or RaptorQ (safe default) |
| **Expected loss** | 3-10%, latency-critical |
| **FEC overhead** | 20-30% (Realtime hint adds extra burst margin) |
| **Interleave** | Depth 2 |
| **Key features** | Realtime protocol hint, ProbeRTT (prevents latency spiral) |
| **Multipath** | Redundant send if two paths available (halves P99) |
| **Recovery expectation** | METTLE: viable at k <= 20 with w/k >= 3; RaptorQ: 100% at all k |

**Why this works**: METTLE's 5-50us encode latency (vs RaptorQ's 257-655us) matters for
packet-at-a-time VoIP at 50+ pps. At k=2-4 (tiny blocks), both backends succeed near-100%.
For larger blocks, RaptorQ is the safer choice. ProbeRTT's ~2% throughput cost prevents the
latency spiral that would be catastrophic for real-time traffic.

### Summary Matrix

| Scenario | Backend | Overhead | Interleave | Multipath | Key Feature |
|----------|---------|----------|------------|-----------|-------------|
| Datacenter | RaptorQ | ~6% | Off | No | — |
| WiFi Home | RaptorQ | ~11-14% | Depth 3 | No | B/T burst term |
| WiFi + LTE | RLC/RaptorQ | 11-20% | Depth 2 | Smart | Reorder buffer, multipath |
| VoIP/Gaming | METTLE/RaptorQ | 15-20% | Depth 2 | Redundant | ProbeRTT |
| Satellite (GEO) | RaptorQ | 15-20% | Depth 4 | No | B/T (high RTT → more FEC) |
| Bulk transfer | RaptorQ (Bulk hint) | ~6-10% | Depth 4 | No | -0.05 hint offset |

---

## Data Sources

| Data | Source Document |
|------|----------------|
| **Consolidated bench suite (Tables 1-4)** | `tests/bench_suite.rs` ([ADR-0042](adr/0042-bench-suite-consolidation.md)) |
| **FEC rate controller formula** | [ADR-0043](adr/0043-information-theoretic-fec-rate.md) |
| Encode/decode speed, waterfall comparison | [benchmark-results-2026-03-13.md](benchmark-results-2026-03-13.md) |
| Block/window recovery, tapered interleaving | [benchmark-results-2026-03-15.md](benchmark-results-2026-03-15.md) |
| Feature ablation (overhead cost, pre-ADR-0043) | [ablation-results-2026-03-16.md](ablation-results-2026-03-16.md) |
| Reorder, multipath, backend switch tradeoffs | ADR-0034 tradeoff bench output |
| Algorithm properties, competitive analysis | [algorithm-competitive-analysis.md](algorithm-competitive-analysis.md) |
| Real-world channel timing data | [benchmark-realworld-results-2026-03-14.md](benchmark-realworld-results-2026-03-14.md) |
| ADRs | [adr/README.md](adr/README.md) |

---

*Last updated: 2026-03-19*
