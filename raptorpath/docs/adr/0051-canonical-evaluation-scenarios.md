# ADR-0051: Canonical Evaluation Scenarios and Win Conditions

## Status: Accepted

## Context

The project goal is to surpass TCP, MPTCP, and similar retransmission-based
transports over (multiple) unreliable channels. That claim is only meaningful
against a **defined scenario suite with explicit win conditions** — and the
suite must include scenarios where raptorpath could plausibly *lose*, or the
claim is unfalsifiable.

What exists today (survey of `tests/bench_suite.rs`, `tests/common/mod.rs`,
`docs/benchmark-methodology.md`):

- All evaluation is **in-process discrete-event simulation**. The "TCP/QUIC"
  baseline is `ReliableSimChannel` (`tests/common/mod.rs:401`): a fixed-delay
  retransmit model with **no congestion control, no RTO estimation, no flow
  control**. The methodology itself says results "cannot predict actual
  throughput or latency on real networks."
- The "MPTCP" baseline is the same model scheduled **round-robin**; real MPTCP
  uses a min-RTT scheduler. The comparison is currently apples-to-oranges.
- Only one traffic pattern is exercised: a saturating one-way bulk transfer
  (2000 × 1200 B symbols). "Realtime" exists only as a protocol hint, not as
  deadline-driven traffic.
- No scenario has competing traffic, congestion-dominant loss, or AQM. There
  is no scenario designed for raptorpath to lose.
- Doc drift: methodology says 8% FEC budget; code uses
  `MATRIX_FEC_OVERHEAD = 0.12` (`tests/bench_suite.rs:40`).

## Decision

### 1. Scenario axes

A scenario is a point in (channel × path-set × traffic). Channels reuse the
paper's Section 2.4 GE parameterization so model, simulator, and benchmarks
share one vocabulary.

**Channels** (single path unless noted):

| ID | Name         | ε      | GE (p, q)      | RTT    | Capacity | Purpose |
|----|--------------|--------|----------------|--------|----------|---------|
| C1 | Clean DC     | 0.1%   | (0.0005, 0.5)  | 2 ms   | 1 Gbps   | **Do-no-harm floor** — FEC overhead must not cost throughput |
| C2 | Home WiFi    | 2.5%   | (0.013, 0.5)   | 10 ms  | 100 Mbps | Bread-and-butter lossy link |
| C3 | LTE          | 5%     | (0.02, 0.4)    | 40 ms  | 20 Mbps  | Moderate loss, medium bursts |
| C4 | GEO Satellite| 9%     | (0.03, 0.3)    | 200 ms | 20 Mbps  | High RTT — ARQ is expensive, FEC should shine |
| C5 | Bad WiFi     | 15%    | (0.053, 0.3)   | 10 ms  | 50 Mbps  | Stress: high loss, long bursts |
| C6 | Congestion-dominant | ~0% random | queue drops only | 20 ms | 10 Mbps, 25-pkt FIFO | **Adversarial**: loss = congestion; FEC must not fight CC. Includes 1 competing TCP flow |
| C7 | Dual symmetric | 2 × C2 (independent) | — | — | — | Multipath aggregation |
| C8 | Dual asymmetric RTT | C2 + C3 (10 ms / 40 ms) | — | — | — | MPTCP HOL-blocking case — our claimed structural advantage |
| C9 | Dual with outage | C2 + C3, one path drops to 100% loss for 2 s mid-transfer, then recovers | — | — | — | Handover/failover fluidity |

C6, and the outage event in C9, are deliberately scenarios where naive FEC
loses; passing them demonstrates the spare-capacity gate and estimator
adaptation, not raw redundancy.

**Traffic patterns** (each mapped to a protocol hint):

| ID | Pattern | Hint | Primary metric |
|----|---------|------|----------------|
| T1 | Bulk transfer, 100 MB | Bulk | Completion time; goodput ratio = goodput / (fair-share capacity × (1−ε)) |
| T2 | Request/response, 32 KB RPCs, closed loop | Auto | p50 / p99 response time |
| T3 | CBR stream 2 Mbps, 33 ms deadline, ρ = 99.9% | Realtime | Deadline-miss rate, p99/p999 delivery latency |
| T4 | Mixed: T3 + T1 concurrently | both | T3's deadline-miss rate must hold while T1 saturates (QoS cascade, paper §13.9) |

The full matrix is 9 × 4 = 36 cells minus non-sensical combinations (e.g.
T4 on C1) — approximately 30 cells. Every cell runs ≥ 30 trials with fixed
seeds (`trial × 137 + 42` convention) and reports mean ± 95% CI.

### 2. Baselines — fidelity ladder

| Level | Baseline | Where it runs | What it's for |
|-------|----------|---------------|---------------|
| L0 | `ReliableSimChannel` (current) | in-process sim, all platforms | Development signal + regression tracking only. **Never cite as "TCP".** Rename its output label from "Retransmit/QUIC" to "SimRetx". |
| L0.5 | SimQuic: loss-blind delay-based CC + SACK-timed ARQ, no FEC, in-order stream, single path | in-process sim (gate_suite) | The honest QUIC/BBR-class adversary: removes the AIMD-collapse advantage, isolating what FEC/multipath actually buy. Win conditions: tie on bulk completion, win on p99, win on multipath aggregation. |
| L1 | Real stacks: Linux TCP CUBIC and BBR, quinn (QUIC), Linux MPTCP v1 | network namespaces + veth + netem/tc (WSL2 or Linux CI) | **Claim-grade.** netem configured to the same GE/RTT/capacity parameters as the sim channels |
| L2 | Real links (WiFi + LTE modem) | manual field runs | Sanity anchor; anecdotal, not gating |

L0 must also gain an honest **min-RTT MPTCP scheduler** for the dual-path
baseline (the round-robin baseline stays as a secondary reference).

### 3. Win conditions ("surpass" made falsifiable)

Per cell, comparing against the **best** baseline for that cell at L1:

| Scenario class | Condition |
|----------------|-----------|
| Lossy single path (C2–C5) | T1: completion time ≤ 0.9 × best baseline. T2/T3: p99 ≤ 0.7 × best baseline; T3 deadline-miss ≤ 0.1 × baseline at equal bandwidth budget |
| Clean (C1) | **Tie is the win**: completion time within 2% of TCP; measured overhead ≤ 1% |
| Congestion-dominant (C6) | Completion time within 5% of TCP **and** the competing TCP flow retains ≥ 40% of capacity (fairness; 50% is ideal) |
| Multipath (C7–C9) | Beat the best single path **and** beat min-RTT MPTCP on the cell's primary metric; C9: post-outage recovery to 90% steady-state goodput within 3 × RTT of path recovery |

CI-separation required: a win counts only if the 95% CIs do not overlap.
The aggregate claim "surpasses TCP/MPTCP" = wins all lossy and multipath
cells and ties all clean/congestion cells. Any cell regression fails the
gate — no averaging across cells (averaging hides losses).

### 4. Immediate corrections to the existing suite

1. Fix methodology doc drift (FEC budget 8% → 12%; re-derive cell counts).
2. Relabel L0 baseline output "SimRetx" (not QUIC/TCP).
3. Add `completion_time` and `goodput_ratio` to `TrialResult` (currently
   only `throughput_mbps`), counting retransmission volume for the baseline
   and correction volume for raptorpath symmetrically.
4. Add the min-RTT scheduler to the L0 dual-path baseline.
5. Add C6 (congestion-dominant + competing flow) and C9 (outage) to the
   matrix; add T2/T3 traffic generators.

### 5. Out of scope (for now)

- ns-3/mininet integration (netns + netem is sufficient for L1)
- Wireless PHY simulation; the GE abstraction is the model boundary
- More than 2 paths

## Consequences

- "Surpass TCP" becomes a checkable predicate over ~30 defined cells rather
  than a slogan; the gate can run in CI at L0 on every change and at L1
  nightly/on-demand.
- The suite contains cells we may currently lose (C6 fairness, C1 tie,
  T3 on C4 at tight deadlines). That is intentional: losing cells point at
  the next model refinement, which per the project's method must land in
  the paper before the code.
- L1 requires a Linux environment (WSL2 locally); the Windows dev flow keeps
  L0 for fast iteration.

## References

- `docs/benchmark-methodology.md` (to be updated per §4)
- ADR-0036 (transport comparison — historical), ADR-0044 (methodology),
  ADR-0046 (sim realism), ADR-0050 (rate control)
- Paper §2.4 (channel parameters), §11 (verification), §13.9 (QoS cascade)
