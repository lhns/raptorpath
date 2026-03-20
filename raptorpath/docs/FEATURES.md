# RaptorPath Feature & Algorithm Reference

Comprehensive inventory of all algorithms, features, and configuration knobs.
For architecture and design rationale, see [../DESIGN.md](../DESIGN.md).
For consolidated benchmark data and deployment profiles, see [comprehensive-overview.md](comprehensive-overview.md).

---

## 1. FEC Backends

RaptorPath supports five swappable FEC backends via the `FecEncoder`/`FecDecoder` traits
([ADR-0021](adr/0021-swappable-fec-backend.md)). Select with `--fec-backend <name>` or
`fec_backend = "<name>"` in TOML config.

| Backend | File(s) | Mode | Overhead | Decode Strategy | Config Value |
|---------|---------|------|----------|-----------------|--------------|
| RaptorQ | `fec/raptorq_backend.rs` | Block | ~1% | LDPC + Inactivation decoding | `raptorq` (default) |
| METTLE | `fec/mettle_backend.rs`, `fec/mettle_window.rs` | Block + Window | ~15% | XOR peeling (pure GF(2)) | `mettle` |
| Reed-Solomon | `fec/rs_backend.rs` | Block | 0% (MDS) | GF(256) matrix inversion | `rs` |
| RLC | `fec/rlc_backend.rs`, `fec/rlc_window.rs` | Block + Window | ~0.4% | GF(256) Gaussian elimination | `rlc` |
| Streaming | `fec/streaming.rs` | Window-only | Channel-adaptive | Diagonal XOR + GF(256) layered | `streaming` |

**Shared infrastructure**: `fec/traits.rs` (encoder/decoder traits, `FecBackend` enum),
`fec/window_traits.rs` (window-mode traits), `fec/gf256.rs` (Galois field arithmetic),
`fec/stream.rs` (streaming interface).

---

## 2. Control Plane Algorithms

| Algorithm | File | Description |
|-----------|------|-------------|
| Loss Estimator | `control/estimator.rs` | EWMA + Bayesian Beta-Binomial loss estimation with 95th-percentile upper confidence bound |
| Gilbert-Elliott HMM | `control/gilbert_elliott.rs` | Two-state Hidden Markov Model for correlated loss / burst detection. Feeds `burst_factor` and `mean_burst_length` into FEC rate and streaming params ([ADR-0023](adr/0023-gilbert-elliott-hmm.md)) |
| FEC Rate Controller | `control/fec_rate.rs` | Information-theoretic optimal formula: `max(p/(1-p), B/T) × (1+margin) + PI + hint_offset`. RTT-aware via B/T burst term ([ADR-0043](adr/0043-information-theoretic-fec-rate.md)) |
| Backend Selector | `control/backend_selector.rs` | Runtime loss-based auto-switching between FEC backends. Hysteresis thresholds prevent flapping ([ADR-0030](adr/0030-runtime-fec-backend-switching.md)) |

---

## 3. Network Data Path

| Feature | File | Description |
|---------|------|-------------|
| Block interleaving | `net/interleave.rs` | Spreads symbols from N blocks across time so a single burst doesn't wipe out one block. Tapered repair distribution for burst resilience ([ADR-0029](adr/0029-tapered-repair-interleaving.md)) |
| Packet framing | `net/framing.rs` | Length-prefixed framing for both block and window mode, preserving IP packet boundaries |
| Sliding-window FEC pipeline | `net/mod.rs` | Window-mode encode/decode pipeline for RLC, METTLE-window, and Streaming backends |
| WindowNack repair | `net/mod.rs` | Sender-initiated repair triggered by receiver NACKs in window mode ([ADR-0025](adr/0025-window-nack-sender-repair.md)) |
| Reorder buffer | `net/mod.rs` | Holds out-of-order packets (20ms timeout, max 500) before delivery in window mode |

---

## 4. Scheduler & Congestion Control

| Feature | File | Description |
|---------|------|-------------|
| BBR-inspired CC | `scheduler/mod.rs` | Delay-based congestion control using RTT gradient (not loss-based). Distinguishes wireless loss from congestion ([ADR-0020](adr/0020-bbr-congestion-control.md)) |
| ProbeRTT phase | `scheduler/mod.rs` | Periodic min-RTT recalibration: 10s interval, 200ms hold at cwnd=4. Prevents standing-queue drift ([ADR-0024](adr/0024-probe-rtt-phase.md)) |
| Multipath scheduling | `scheduler/mod.rs` | Source symbols to lowest-RTT paths, repair to highest-goodput paths. Window-mode symbol scheduling for sliding-window backends ([ADR-0026](adr/0026-multipath-window-scheduling.md)) |
| Clock abstraction | `scheduler/clock.rs` | Testable time source for deterministic scheduler tests |

---

## 5. Transport & Infrastructure

| Feature | File | Description |
|---------|------|-------------|
| QUIC transport | `transport/quic.rs` | Per-path QUIC connections via quinn, datagram framing |
| Protocol versioning | `transport/protocol.rs` | 8-byte magic + version header (v3), handshake negotiation |
| TLS cert pinning | `transport/quic.rs` | Optional pinned certificate (DER/PEM) for server verification |
| HTTP monitoring | `monitor/http.rs` | Axum HTTP endpoint (`/status`, `/health`) with runtime stats |
| Lock-free stats | `monitor/stats.rs` | `SharedStats` with atomics for contention-free metric collection |
| Preflight checks | `preflight.rs` | Environment validation (`raptorpath check`) |
| Route/DNS management | `routing.rs` | Automatic route and DNS setup with cleanup on shutdown |
| TUN interface | `tun/mod.rs` | Platform-agnostic TUN: Linux kernel driver (`tun/linux/`) and Windows WinTUN (`tun/windows/`) |

---

## 6. Modularity Matrix for Benchmarking

### Tier 1: Fully toggleable via config / CLI

| Feature | Config Key | CLI Flag | Default |
|---------|-----------|----------|---------|
| FEC backend | `fec_backend` | `--fec-backend` | `raptorq` |
| Protocol hint | `protocol_hint` | `--protocol-hint` | `auto` |
| Interleave depth | `interleave_depth` | `--interleave-depth` | auto (hint-based: realtime=2, auto=3, bulk=4) |
| Auto backend switching | `fec_auto_switch` | — | `true` (unless `fec_backend` is explicitly set) |
| Switch threshold (low) | `fec_switch_threshold_low` | — | `0.01` |
| Switch threshold (high) | `fec_switch_threshold_high` | — | `0.12` |
| Switch interval | `fec_switch_interval` | — | `5` seconds |
| Target tail loss | `target_tail_loss` | `--target-tail-loss` | `1e-5` |
| Max FEC overhead | `max_fec_overhead` | `--max-fec-overhead` | `0.5` |
| Monitoring endpoint | `status_addr` | `--status-addr` | off |
| TLS cert pinning | `pin_cert` | `--pin-cert` | off |
| PI feedback loop | `enable_pi_feedback` | — | `true` |
| ~~GE burst scaling~~ | ~~`ge_burst_factor`~~ | — | Removed in ADR-0043 (integrated into B/T formula) |
| ~~Realtime burst extra~~ | ~~`realtime_burst_extra`~~ | — | Removed in ADR-0043 (replaced by +0.05 hint offset) |
| ProbeRTT phase | `enable_probe_rtt` | — | `true` |
| Reorder buffer timeout | `reorder_timeout_ms` | — | `20` (0 = disabled) |
| Reorder buffer capacity | `reorder_max_size` | — | `500` |

### Tier 2: Always-on (not independently toggleable)

| Algorithm | File | Reason |
|-----------|------|--------|
| Loss estimation (EWMA + Beta-Binomial) | `control/estimator.rs` | Core FEC rate dependency — all backends need loss estimates |
| Gilbert-Elliott HMM | `control/gilbert_elliott.rs` | Integrated in LossEstimator; feeds burst params to FEC rate |
| FEC rate controller (info-theoretic) | `control/fec_rate.rs` | Core repair symbol computation via `max(p/(1-p), B/T)` formula (PI feedback is toggleable separately) |
| BBR congestion control | `scheduler/mod.rs` | Only CC algorithm implemented (ProbeRTT phase is toggleable separately) |
| Tapered interleaving | `net/interleave.rs` | Automatic when interleave depth >= 2 |

---

## 7. Benchmarking Quick Reference

### Run with a specific backend (disable auto-switching)

```bash
raptorpath run --fec-backend rlc --fec-auto-switch false ...
```

The `fec_auto_switch = false` config key (or explicit `--fec-backend`) pins the backend
so the runtime selector doesn't override it.

### Control interleaving

```bash
# Disable interleaving
raptorpath run --interleave-depth 1 ...

# Deep interleaving (4 blocks)
raptorpath run --interleave-depth 4 ...
```

### Suggested test matrix

| Dimension | Values |
|-----------|--------|
| Backend | `raptorq`, `mettle`, `rs`, `rlc`, `streaming` |
| Loss rate | 0%, 1%, 5%, 10%, 20% |
| Protocol hint | `realtime`, `bulk`, `auto` |
| Interleave depth | 1 (off), 2, 4 |

### Consolidated benchmark suite

Run `cargo test --test bench_suite -- --nocapture` to produce 4 focused tables
([ADR-0042](adr/0042-bench-suite-consolidation.md)):

1. **Backend Loss Sweep** — recovery rate vs uniform loss for all 5 backends with 95% CI
2. **Wire Overhead Breakdown** — all 5 overhead layers from ADR-0038 (info-theoretic rate, ADR-0043)
3. **Feature Ablation** — one-feature-off under WiFi bursty with 8% FEC budget (tightened from 20% in ADR-0043)
4. **FEC vs Retransmit** — FEC dual-path vs retransmit dual-path across 3 scenarios
5. **Transport Comparison** — QUIC single vs MPTCP (rr + minRTT) vs FEC (single + dual) across 3 scenarios (ADR-0036)

### Existing benchmarks

- `tests/bench_suite.rs` — Consolidated benchmark suite (ADR-0042)
- `benches/fec_bench.rs` — Microbenchmarks for encode/decode throughput per backend
- `docs/benchmark-results-2026-03-19.md` — Latest: ADR-0043 rate controller + transport comparison
- `docs/benchmark-results-2026-03-15.md` — Post-METTLE bug fix, tapered interleaving
- `docs/benchmark-realworld-results-2026-03-14.md` — Real-world network test results
- `docs/benchmark-results-2026-03-13.md` — Initial FEC backend comparison
- `docs/algorithm-competitive-analysis.md` — Detailed algorithm comparison

---

## 8. Algorithm Tradeoffs & Benefits

Each toggleable algorithm has a specific cost and a specific benefit. This section documents
**when each algorithm is worth its cost** and when it can be disabled.

### ProbeRTT Phase

**What it does**: Every 10 seconds, drops cwnd to 4 packets for 200ms to drain queues and
re-measure the true propagation delay (min_rtt).

**Benchmark note** (ADR-0034): ProbeRTT did not differentiate in simulation because BBR's
`min_rtt` is private and the sim queue model is simplified. Values below are theoretical.

| Dimension | Impact |
|-----------|--------|
| **Cost** | ~2% throughput loss (200ms drain / 10s interval). Causes periodic cwnd dip visible in throughput traces |
| **Benefit** | Keeps min_rtt accurate. Without it, standing queues inflate RTT estimates, causing BDP overestimation → more queuing → latency spiral |
| **When to enable** | Long-lived connections, latency-sensitive traffic (VoIP, gaming), paths with variable queuing |
| **When to disable** | Short transfers (<10s), bulk throughput-only workloads, benchmarks measuring raw throughput |
| **Key metric** | `min_rtt_accuracy`: ratio of scheduler's min_rtt to true propagation delay (1.0 = perfect) |
| **Config** | `enable_probe_rtt = true` (default) |

### Reorder Buffer

**What it does**: Holds out-of-order recovered symbols and delivers them in sequence order.
Symbols are held until their predecessor arrives, or a configurable timeout expires.

**Measured** (ADR-0034 tradeoff bench, dual-path WiFi 5ms + LTE 30ms, 10 trials):

| Timeout | OOO rate | Avg latency | Jitter |
|---------|----------|-------------|--------|
| 0ms (off) | 1.4% | 6.2ms | 1.9ms |
| 25ms | 0.0% | 6.9ms | 3.4ms |

| Dimension | Impact |
|-----------|--------|
| **Cost** | Adds up to `reorder_timeout_ms` of delivery delay (+0.7ms measured at 25ms timeout) |
| **Benefit** | Eliminates 1.4% out-of-order delivery. Absorbs multipath jitter |
| **When to enable** | Multipath with asymmetric RTTs (WiFi + LTE), applications requiring ordered delivery, video streaming |
| **When to disable** | Single-path setups, applications that handle reordering internally, ultra-low-latency requirements |
| **Sweet spot** | Timeout ~= RTT_difference / 2 between fastest and slowest paths (e.g., 12-15ms for WiFi 5ms + LTE 30ms) |
| **Key metric** | `out_of_order_rate`: fraction of symbols delivered before their predecessor |
| **Config** | `reorder_timeout_ms = 20` (default), `0` = disabled |

### NACK Repair (Window Mode)

**What it does**: Receiver detects gaps in the received sequence and sends NACKs to the sender.
Sender generates targeted repair symbols for the missing range within 1 RTT.

**Benchmark note** (ADR-0034): NACK did not differentiate in sim because RLC window's proactive
repair naturally recovers bursts. Theoretical analysis applies; most valuable at tight budgets.

| Dimension | Impact |
|-----------|--------|
| **Cost** | Extra repair symbols per detected gap (~3-10 symbols per burst event). Small bandwidth overhead (+0.2-0.3pp recovery) |
| **Benefit** | Fast burst-loss recovery without over-provisioning proactive FEC. Fills gaps within 1 RTT instead of waiting for proactive repairs to arrive |
| **When to enable** | Bursty loss channels (WiFi), tight FEC budgets (<=12% overhead), real-time traffic |
| **When to disable** | Very low loss environments (datacenter), generous FEC budgets (>=20% where proactive FEC covers bursts alone) |
| **Critical threshold** | At <=12% FEC budget, NACK dramatically improves burst recovery. At >=20%, proactive FEC alone is sufficient |
| **Key metric** | `burst_recovery_rate`: fraction of burst events (>=5 consecutive drops) fully recovered |
| **Config** | Always enabled in window mode (controlled via FEC rate controller) |

### Backend Auto-Switch

**What it does**: Monitors loss rate and automatically switches between FEC backends based on
configurable thresholds. RLC is efficient at low loss; Streaming/Mettle handles high loss better.

**Measured** (ADR-0034 tradeoff bench, 5-phase loss sweep 0.5%→5%→15%→5%→0.5%):
Auto-switch detects 2-3 transitions across phases. Hysteresis prevents flapping.

| Dimension | Impact |
|-----------|--------|
| **Cost** | Possible overhead spike during transition. Switch event itself is safe (flush protocol) but the new backend needs warm-up. Risk of flapping at threshold boundaries (mitigated by hysteresis) |
| **Benefit** | Optimal codec efficiency across changing conditions. RLC has near-zero overhead at low loss; Streaming/Mettle handles burst loss better at high loss |
| **When to enable** | Variable loss environments (mobile, WiFi roaming), mixed-condition tunnels |
| **When to disable** | Stable loss environments, benchmarking a specific backend, debugging codec issues |
| **Threshold tuning** | Default (1%, 12%) — high threshold raised from 8%/10% per ADR-0043 bench data showing block codes cliff at ~12-15% loss (Table 1) |
| **Key metric** | Per-phase overhead (low-loss phase vs high-loss phase), `backend_switches` count |
| **Config** | `fec_auto_switch = true` (default), `fec_switch_threshold_low = 0.01`, `fec_switch_threshold_high = 0.12` |

### Multipath Scheduling

**What it does**: Distributes symbols across multiple network paths. Source symbols go to
lowest-RTT paths (minimize first-byte latency), repairs to highest-goodput paths (maximize
decode probability). Optional redundant send mode sends source on all paths.

**Measured** (ADR-0034 tradeoff bench, WiFi 5ms/2% + LTE 25ms/0.5%, 10 trials):

| Config | P99 latency | Jitter | Overhead |
|--------|-------------|--------|----------|
| single_wifi | 72ms | 12.1ms | 10% |
| dual_primary_wifi | 57ms | 6.8ms | 10% |
| dual_redundant | 30ms | 0.1ms | 5% |

| Dimension | Impact |
|-----------|--------|
| **Cost** | Redundant mode uses 2x bandwidth. Smart scheduling uses ~1.3x bandwidth |
| **Benefit** | P99 reduction: 72ms → 57ms (smart) → 30ms (redundant). Jitter: 12.1ms → 0.1ms |
| **When to enable** | Multiple available paths (WiFi + LTE + Ethernet), latency-sensitive traffic, unreliable individual paths |
| **When to disable** | Single available path, bandwidth-constrained environments where 2x overhead is unacceptable |
| **Smart scheduling** | Source on fast path, repair on reliable path → 80% of redundant's latency benefit at ~30% less bandwidth |
| **Key metric** | `p99_delivery_latency_ms`, `recovery_rate` under path failure |
| **Config** | Number of paths configured via `bind`/`peer` addresses. Scheduling is automatic |

### FEC Rate: PI Feedback Loop

**What it does**: Observes actual block decode success/failure rate and adjusts a correction
term added to the information-theoretic base rate. Uses a PI (proportional-integral) controller
(Kp=0.5, Ki=0.1) to converge actual failure rate toward the target.

Since ADR-0043, PI gains are reduced (from Kp=2.0/Ki=0.5) because the information-theoretic
base formula is accurate enough that PI only handles residual model mismatch.

**Measured impact** (bench suite Table 3, 2026-03-19, 8% FEC budget):
Disabling PI (`no_pi`) showed 0pp delta — the base formula's accuracy means PI contributes
minimally in steady-state simulation. PI remains valuable in production where loss
characteristics change over time and the estimator may lag.

| Dimension | Impact |
|-----------|--------|
| **Cost** | Minimal in steady state. Can overshoot if loss changes rapidly (integral windup, mitigated by clamping) |
| **Benefit** | Corrects for model mismatch between estimated and actual loss over long sessions |
| **When to enable** | Production deployments, long-lived connections, changing channel conditions |
| **When to disable** | Short benchmarks, well-characterized channels |
| **Config** | `enable_pi_feedback = true` (default) |

### Gilbert-Elliott Burst Model

**What it does**: Two-state HMM detects correlated loss bursts. Feeds `mean_burst_length`
into the FEC rate controller's B/T delay-constrained capacity term.

As of ADR-0043, the GE model is integrated into the rate formula rather than being a
separate multiplicative scaling factor. The `B/T` term (`burst_length / T_symbols`) naturally
captures the delay-constrained capacity of a burst erasure channel, where
`T = (RTT × throughput) / symbol_size`.

| Dimension | Impact |
|-----------|--------|
| **Cost** | None — the B/T term is built into the base formula |
| **Benefit** | RTT-aware burst protection. High RTT → small T → B/T dominates → more proactive FEC. Low RTT → NACK can fill gaps cheaply |
| **When relevant** | WiFi, LTE, satellite — any channel with correlated loss |
| **Config** | Always on (no separate toggle needed) |

### Block Interleaving

**What it does**: Spreads symbols from N blocks across time so a single burst doesn't wipe out
one block. Uses tapered repair distribution (exponential decay) for burst resilience.

**Measured** (2026-03-15, tapered vs flat, Congested scenario):
RaptorQ +5pp, RLC +10pp, METTLE +11pp (LTE). No regressions at low loss.

| Dimension | Impact |
|-----------|--------|
| **Cost** | Adds ~1 block-period of delivery latency. Higher depth = more latency but better burst resilience |
| **Benefit** | +5-11pp recovery improvement in high-loss scenarios (measured). Converts burst loss into distributed random loss |
| **When to enable** | Bursty channels, block-mode FEC backends (RaptorQ, RS) |
| **When to disable** | Window-mode backends (RLC, Streaming) which handle bursts natively, ultra-low-latency requirements |
| **Config** | `interleave_depth = 3` (default for auto), `1` = disabled |
