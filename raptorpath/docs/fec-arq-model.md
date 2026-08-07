# FEC/ARQ Unified Correction Symbol Model

## Abstract

Data traversing lossy multipath channels (WiFi, LTE, satellite) has
specific requirements: some needs low latency (VoIP), some needs high
throughput (file transfer), some tolerates partial loss (sensor telemetry).
Each available path has different characteristics — bandwidth, latency,
and loss rate. The goal of this model is to **optimally use each path or
combination of paths to satisfy the constraints of the data sent over
them**, without ad-hoc tuning knobs.

Three properties — bandwidth, tail latency, and reliability — form a
triangle: fix any two, the third is determined by the channel. The protocol
hint (Realtime, Bulk, etc.) selects which two to fix. The model computes
the optimal correction rate, taper schedule, and path allocation from
measured channel parameters — no magic offsets or manual thresholds.

The core mechanism is the **correction symbol** — a unified concept that
subsumes both FEC repair symbols (proactive, flexible) and ARQ source
retransmits (reactive, immediately decodable). A single taper function,
shaped by the Gilbert-Elliott channel model's burst survival function,
controls correction density over time. A probabilistic per-slot decision
based on loss confidence P_lost(t) — driven by time since send —
determines whether each correction is a retransmit or a repair.

The optimal correction rate r* = e/(1-e) + z_delta x sqrt(e x s2_burst /
(W(1-e))) combines the information-theoretic minimum with a burst-variance-
corrected tail margin. Copa congestion control is recommended over BBR for
its taper-compatible rate oscillation (no FEC protection gaps from ProbeRTT).
*[Measured status 2026-07-19: at the substrate this is now a POLICY surface —
BBR-under is the measured bulk-throughput champion, Copa-sole (wire-signal)
the queue/tail champion at 0.86–0.89× bulk that strictly dominates on
heterogeneous C8; Copa's missing TCP-competitive mode gates any default. See
§12.11 and §17.]*

For multipath, each path runs its own Copa, taper, and GE estimator with a
unified symbol stream preserving interleaved burst protection. A global
correction deficit tracks outstanding recovery needs across paths. The
scheduler adjusts per-path source/correction ratios using an interpolated
latency/bandwidth objective, with QoS priority cascading for mixed traffic.

ACK-based feedback with SACK replaces the NACK mechanism — the same proven
approach TCP has used for 40 years, with sender-side loss inference from
ACK absence.

---

## Table of Contents

1. [System Model](#1-system-model)
   - [1.1 Notation](#11-notation)
   - [1.2 Glossary](#12-glossary)
   - [1.3 Components](#13-components)
   - [1.4 The Bandwidth / Latency / Reliability Triangle](#14-the-bandwidth--latency--reliability-triangle)
2. [Channel Model](#2-channel-model)
   - [2.1 Gilbert-Elliott Two-State HMM](#21-gilbert-elliott-two-state-hmm)
   - [2.2 Stationary Properties](#22-stationary-properties)
   - [2.3 Burst Length Distribution](#23-burst-length-distribution)
   - [2.4 Concrete Examples](#24-concrete-examples)
3. [Recovery Fundamentals](#3-recovery-fundamentals)
   - [3.1 The Problem: Recovering Lost Symbols](#31-the-problem-recovering-lost-symbols)
   - [3.2 FEC: Proactive Recovery](#32-fec-proactive-recovery)
   - [3.3 ARQ: Reactive Recovery](#33-arq-reactive-recovery)
   - [3.4 Recovery Latency and the P_lost(t) Model](#34-recovery-latency-and-the-plostt-model)
4. [The Taper Function](#4-the-taper-function)
   - [4.1 Definition](#41-definition)
   - [4.2 Why Match the Loss Distribution?](#42-why-match-the-loss-distribution)
   - [4.3 Total Correction Rate](#43-total-correction-rate)
   - [4.4 The Taper Never Reaches Zero](#44-the-taper-never-reaches-zero)
   - [4.5 Real-Time Adaptation](#45-real-time-adaptation)
5. [Unified Correction Symbol Model](#5-unified-correction-symbol-model)
   - [5.1 Why Unify FEC and ARQ?](#51-why-unify-fec-and-arq)
   - [5.2 Correction Symbols — The Unified Concept](#52-correction-symbols--the-unified-concept)
   - [5.3 Three-Stream View: Source, Repair, Retransmit](#53-three-stream-view-source-repair-retransmit)
   - [5.4 Per-Slot Decision](#54-per-slot-decision)
6. [Protocol Mechanics](#6-protocol-mechanics)
   - [6.1 The Retransmit Buffer](#61-the-retransmit-buffer)
   - [6.2 SACK-Extended ACK](#62-sack-extended-ack)
   - [6.3 Per-Symbol Delivery Outcomes](#63-per-symbol-delivery-outcomes)
   - [6.4 The Triangle in Action](#64-the-triangle-in-action)
   - [6.5 Four-Mechanism Composition](#65-four-mechanism-composition)
7. [Estimation — From Observations to Channel Parameters](#7-estimation--from-observations-to-channel-parameters)
   - [7.1 What We Observe](#71-what-we-observe)
   - [7.2 EWMA — Fast Point Estimate](#72-ewma--fast-point-estimate)
   - [7.3 Beta-Binomial Posterior — Uncertainty Quantification](#73-beta-binomial-posterior--uncertainty-quantification)
   - [7.4 BOCD — Regime-Aware Prediction](#74-bocd--regime-aware-prediction)
   - [7.5 GE Parameter Estimation](#75-ge-parameter-estimation)
   - [7.6 ACK Loss and Self-Healing](#76-ack-loss-and-self-healing)
   - [7.7 Estimation Error and Overhead](#77-estimation-error-and-overhead)
8. [The Optimization Problem](#8-the-optimization-problem)
   - [8.1 Formal Statement](#81-formal-statement)
   - [8.2 Corrected Model (canonical)](#82-corrected-model-canonical)
   - [8.3 Burst Variance Correction](#83-burst-variance-correction)
   - [8.4 The Corrected Optimal Correction Rate](#84-the-corrected-optimal-correction-rate)
   - [8.4.1 Burst-Tail Provisioning: r* Against the Measured Window-Mass Quantile](#841-burst-tail-provisioning-r-against-the-measured-window-mass-quantile)
   - [8.5 Worked Examples](#85-worked-examples)
   - [8.6 Three-Variable Optimization](#86-three-variable-optimization)
   - [8.7 Exact P_fec via Transfer-Matrix DP](#87-exact-p_fec-via-transfer-matrix-dp)
   - [8.8 Choosing the Window](#88-choosing-the-window)
   - [8.9 The Unified Deadline-Constrained r*](#89-the-unified-deadline-constrained-r)
9. [Codec Overhead Integration](#9-codec-overhead-integration)
   - [9.1 Decoder Invocation Probability](#91-decoder-invocation-probability)
   - [9.2 Effective Codec Overhead](#92-effective-codec-overhead)
   - [9.3 Impact on METTLE at DC](#93-impact-on-mettle-at-dc)
10. [Multi-Protocol Extension](#10-multi-protocol-extension)
    - [10.1 Per-Symbol Latency Classes](#101-per-symbol-latency-classes)
    - [10.2 Interleave Before vs After FEC](#102-interleave-before-vs-after-fec)
    - [10.3 When Shared Wins](#103-when-shared-wins)
    - [10.4 Extending the Formula](#104-extending-the-formula)
11. [Verification](#11-verification)
    - [11.1 Simulation Approach](#111-simulation-approach)
    - [11.2 Analytical Predictions to Verify](#112-analytical-predictions-to-verify)
    - [11.3 Boundary Cases](#113-boundary-cases)
    - [11.4 Connection to Existing Benchmarks](#114-connection-to-existing-benchmarks)
12. [Congestion Control Integration](#12-congestion-control-integration)
    - [12.1 Why Delay-Based CC is Required](#121-why-delay-based-cc-is-required)
    - [12.2 QUIC Datagrams Bypass Quinn's CC](#122-quic-datagrams-bypass-quinns-cc)
    - [12.3 Copa vs BBR](#123-copa-vs-bbr)
    - [12.4 Copa's Rate Formula](#124-copas-rate-formula)
    - [12.5 CC + Taper: The Complete Architecture](#125-cc--taper-the-complete-architecture)
    - [12.6 BtlBw-Anchored Recovery](#126-btlbw-anchored-recovery)
    - [12.7 ECN as Opportunistic Enhancement](#127-ecn-as-opportunistic-enhancement)
    - [12.8 Application Back-Pressure](#128-application-back-pressure)
13. [Multi-Path Scheduling](#13-multi-path-scheduling)
    - [13.1 Why FEC Beats MPTCP](#131-why-fec-beats-mptcp)
    - [13.2 Per-Path Model](#132-per-path-model)
    - [13.3 Unified Symbol Stream and Interleaving](#133-unified-symbol-stream-and-interleaving)
    - [13.4 Correction Deficit](#134-correction-deficit)
    - [13.5 Effective Delivery Time and Bandwidth](#135-effective-delivery-time-and-bandwidth)
    - [13.6 Scheduler Ratio Adjustment](#136-scheduler-ratio-adjustment)
    - [13.7 Burst Protection During Ratio Adjustment](#137-burst-protection-during-ratio-adjustment)
    - [13.8 Interpolated Objective Function](#138-interpolated-objective-function)
    - [13.9 QoS Priority Cascade](#139-qos-priority-cascade)
    - [13.10 Cross-Path Retransmit](#1310-cross-path-retransmit)
14. [Future Directions and Considered Improvements](#14-future-directions-and-considered-improvements)
    - [14.1 Reconsidering the Triangle](#141-reconsidering-the-triangle-bandwidth-as-constraint-not-variable)
    - [14.2 FEC Latency Is Not Zero](#142-fec-latency-is-not-zero)
    - [14.3 Unified FEC Latency Distribution](#143-unified-fec-latency-distribution)
    - [14.4 Ambient FEC and the Pipeline Effect](#144-ambient-fec-and-the-pipeline-effect)
    - [14.5 Optimal Encoder Window Size](#145-optimal-encoder-window-size)
    - [14.6 ARQ Latency and the Retransmit Sweet Spot](#146-arq-latency-and-the-retransmit-sweet-spot)
    - [14.7 When FEC Beats ARQ](#147-when-fec-beats-arq-and-vice-versa)
    - [14.8 Per-Symbol Recovery Probability](#148-per-symbol-recovery-probability-function)
    - [14.9 Reconceived Delivery Time Distribution](#149-reconceived-delivery-time-distribution)
    - [14.10 Latency vs Throughput Trade-off](#1410-latency-tail-vs-throughput-not-always-a-trade-off)
    - [14.11 Application Profiles Revisited](#1411-application-profiles-revisited)
    - [14.12 ARQ After FEC Decode](#1412-arq-after-fec-decode-not-redundant-for-the-decoder)
    - [14.13 Proactive Retransmit vs FEC](#1413-proactive-retransmit-vs-fec-fec-is-strictly-better)
    - [14.14 Marginalizing Over Burst Length](#1414-marginalizing-over-burst-length)
    - [14.15 In-Burst FEC Survival](#1415-in-burst-fec-survival)
    - [14.16 The FEC/ARQ Race](#1416-the-fecarq-race)
    - [14.17 Decode-Induced Jitter](#1417-decode-induced-jitter)
    - [14.18 Estimator-Rate Feedback Stability](#1418-estimator-rate-feedback-stability)
    - [14.19 Consistency of P_fec Models](#1419-consistency-of-p_fec-models)
    - [14.20 The δ Definition Question](#1420-the-δ-definition-question)
    - [14.21 Sub-Capacity Operation](#1421-sub-capacity-operation-emergent-behavior)
    - [14.22 Sequence-Aware P_lost](#1422-sequence-aware-p_lost)
    - [14.23 Post-Burst FEC Boost](#1423-post-burst-fec-boost-reactive-deficit-recovery)
    - [14.24 Jitter-Horizon Encoder Lag](#1424-jitter-horizon-encoder-lag)
    - [14.25 Completion-Tail FEC](#1425-completion-tail-fec)
    - [14.26 Completion-Exposure δ](#1426-completion-exposure-δ-the-bulk-glide)
    - [14.27 Block-Mode ARQ via Batch Acknowledgements](#1427-block-mode-arq-via-batch-acknowledgements)
    - [14.28 Inner-Feedback Flows and the Repair Floor](#1428-inner-feedback-flows-and-the-repair-floor)
    - [14.29 The End-of-Stream Taper Completion Term (All Hints)](#1429-the-end-of-stream-taper-completion-term-all-hints)
15. [The Unified Sliding-Window Model (Blocks and Streams as Two Knobs)](#15-the-unified-sliding-window-model-blocks-and-streams-as-two-knobs)
    - [15.1 The Defect: One Triangle per Tunnel](#151-the-defect-one-triangle-per-tunnel)
    - [15.2 The Unified Sliding-Window RLC Model](#152-the-unified-sliding-window-rlc-model)
    - [15.3 Block Mode as a Limiting Case](#153-block-mode-as-a-limiting-case)
    - [15.4 Per-Stream Triangle Multiplexing](#154-per-stream-triangle-multiplexing)
    - [15.5 Cost and Benefit (Honest)](#155-cost-and-benefit-honest)
    - [15.6 Migration Sketch](#156-migration-sketch)
    - [15.7 Amendment: Retention Is the Triangle's ρ, Not a New Axis (measured)](#157-amendment-retention-is-the-triangles-ρ-not-a-new-axis-measured)
16. [Reliable Windowed Multipath: an Order-Statistic Formulation](#16-reliable-windowed-multipath-an-order-statistic-formulation)
    - [16.1 Three Regimes, Three Decode Predicates](#161-three-regimes-three-decode-predicates)
    - [16.2 The Sliding-Window Realization (In-Order Is Not the Bottleneck)](#162-the-sliding-window-realization-in-order-is-not-the-bottleneck)
    - [16.3 The Missing Quadrant: Reliable Windowed Multipath](#163-the-missing-quadrant-reliable-windowed-multipath)
    - [16.4 One Pipeline, Not Mode Switching](#164-one-pipeline-not-mode-switching)
    - [16.5 Choosing W for Multipath: a Fourth Bound](#165-choosing-w-for-multipath-a-fourth-bound)
    - [16.6 Predictions, Prerequisites, and the Experiment](#166-predictions-prerequisites-and-the-experiment)
    - (16.7–16.24: the measured arc — reorder horizon, the concluded-then-
      reopened aggregation verdicts, the methodology audit, the substrate
      chain, the unified span machine, anchor hygiene, bounded account
      borrowing, engine parallelization, multipath recovery suppression;
      headers in-text)
17. [The Measured Regime Map (2026-07-19)](#17-the-measured-regime-map-2026-07-19)
    - [17.1 The Substrate Chain](#171-the-substrate-chain-eight-walls-in-order)
    - [17.2 The CC Policy Surface](#172-the-cc-policy-surface)
    - [17.3 Aggregation vs Σ](#173-aggregation-vs-σ--the-bulk-n-verdict)
    - [17.4 The FEC Story](#174-the-fec-story-honestly)
    - [17.5 The Three-Machine Map](#175-the-three-machine-map)
    - [17.6 The Roadmap](#176-the-roadmap-named-not-built)
    - [17.7 The Shipped Default Stack](#177-the-shipped-default-stack-2026-07-21--the-maps-recommendation-is-the-default)
    - [17.8 Supersession Index](#178-supersession-index)

**Appendices:**
- [A: Summary of Key Formulas](#appendix-a-summary-of-key-formulas)
- [B: Related Work](#appendix-b-related-work)
- [C: Model Extensions](#appendix-c-model-extensions-from-related-work)
- [D: Open Questions](#appendix-d-open-questions)
- [E: Preliminary Poisson Model](#appendix-e-preliminary-poisson-model-superseded)
- [References](#references)

---

## 1. System Model

### 1.1 Notation

| Symbol | Meaning | Unit | Example |
|--------|---------|------|---------|
| ε      | Average channel loss rate | probability (0-1) | 0.025 (2.5% WiFi) |
| p      | P(Good → Bad) in GE model | probability (0-1) | 0.013 |
| q      | P(Bad → Good) in GE model | probability (0-1) | 0.5 |
| B      | Mean burst length = 1/q | symbols (count) | 2.0 |
| W      | Encoder window size | symbols (count) | 50 |
| RTT    | Round-trip time | seconds | 0.050 (50ms) |
| P_lost(t) | P(symbol lost given no ACK after time t) | probability (0-1) | ≈2ε at t=SRTT |
| t_fec  | FEC recovery time = m × (1+r) / (r × (1-ε)) × t_sym | seconds | 0.0013 (1.3ms) |
| t_sym  | Symbol transmission time = symbol_size / throughput | seconds | 0.000096 (96μs at 100Mbps) |
| SRTT   | Smoothed RTT estimate | seconds | 0.050 (50ms) |
| RTTVAR | RTT variance estimate (standard deviation) | seconds | 0.005 (5ms) |
| r      | Total correction rate | ratio (corrections/source) | 0.08 (8%) |
| τ(t)   | Taper function: correction density at offset t | ratio (corrections/symbol) | 0.04 |
| A      | Taper amplitude (scaling factor) | ratio (corrections/symbol) | 0.04 |
| P_fec  | Probability a lost symbol is FEC-recovered | probability (0-1) | 0.95 |
| δ      | Tail latency target: P(late delivery) ≤ δ | probability (0-1) | 1e-4 |
| ρ      | Reliability target: P(symbol delivered) ≥ ρ | probability (0-1) | 1.0 (100%) |
| T_cut  | Taper cutoff time (stop corrections after this) | seconds | ∞ (100% reliability) |
| B_max  | 99.99th percentile burst length = ceil(ln(0.0001)/ln(1-q)) | symbols (count) | 14 (WiFi, q=0.5) |
| buffer_max | Max retransmit buffer size (derived) | symbols (count) | 700 (WiFi 100Mbps) |
| L_prop | Propagation delay (base latency) | seconds | 0.025 (25ms) |
| L_arq  | ARQ recovery latency (time from loss to retransmit arrival) | seconds | 0.075-0.100 |
| σ²_burst | Burst variance inflation factor | dimensionless | 2.9 |
| z_δ    | Standard normal quantile for δ | dimensionless | 3.72 (for δ=1e-4) |
| ε_burst | Current channel loss rate (fast EWMA or GE state, optional — see C.6) | probability (0-1) | 0.5 (during burst) |
| ε_codec | Codec decode overhead | ratio (0-1) | 0.01 (RaptorQ) |

### 1.2 Glossary

**Correction symbol** — any symbol sent to recover lost data. Either a source
retransmit (exact copy, immediate decode) or a repair symbol (FEC linear
combination, needs decoder). The taper function controls how many; P_lost(t)
controls which type. See Section 5.2.

**Repair symbol** — a correction symbol generated by the FEC encoder. It is a
random linear combination of source symbols in the encoder window over GF(256).
The receiver feeds it to the decoder. See Section 3.2.

**Source retransmit** — a correction symbol that is an exact copy of a
previously-sent source symbol from the retransmit buffer. The receiver can
use it immediately without decoding. See Section 3.3.

**Encoder window** — the W most recent source symbols tracked by the FEC
encoder. Repair symbols are linear combinations of symbols in this window.
When a symbol is evicted (window slides forward), it can no longer be
covered by new repair symbols. See Section 3.2.

**Taper function τ(t)** — specifies the correction symbol density at time
offset t from a source symbol. Shaped to match the channel's burst-length
distribution: τ(t) = A × (1-q)^t for Gilbert-Elliott channels. See Section 4.

**Systematic code** — an FEC scheme where source symbols are sent as-is (not
encoded). The decoder is only invoked when at least one source symbol is lost.
If nothing is lost, the data passes through without any decoding cost.

**Codec overhead** — the extra symbols the decoder needs beyond the number of
losses, because not all random linear combinations are linearly independent.
E.g., RaptorQ needs ~1% extra, METTLE needs ~15%. See Section 9.

**SACK (Selective Acknowledgement)** — an extension to cumulative ACK that
reports out-of-order received blocks beyond the cumulative point. Lets the
sender know exactly which symbols arrived despite gaps. See Section 6.1.

**GE states (Good/Bad)** — the two states of the Gilbert-Elliott channel
model. In the simplified model: Good = no loss, Bad = total loss. Transition
probabilities p (Good->Bad) and q (Bad->Good) determine burst behavior.
See Section 2.

**P_lost(t)** — probability that a specific symbol was lost, given no ACK
received after time t. The per-slot mixing probability for repair vs
retransmit. See Section 3.4.

**P_fec** — probability that a lost symbol is recovered by FEC (proactive
correction) before ARQ retransmit kicks in. See Section 8.2.

**P_arq** — probability that ARQ retransmit succeeds, given FEC failed.
Derived from reliability target: P_arq = 1-(1-ρ)/(ε×(1-P_fec)).
See Section 6.3.

**z_delta** — standard normal quantile at probability (1-δ). Controls the
tail margin in the repair rate formula. See Section 8.4.

**σ²_burst (s2_burst)** — burst variance inflation factor for the GE
channel: 1+2(1-p-q)/(p+q). Corrects the iid normal approximation for
burst-correlated losses. See Section 8.3.

**BDP (Bandwidth-Delay Product)** — link capacity × round-trip time:
BDP = BtlBw × RTT. It is the amount of data that must be in flight
(unacknowledged) to keep a path fully utilized — the ideal in-flight
window. A congestion window near BDP fills the pipe with no standing
queue; above BDP the excess sits in the bottleneck buffer (queuing
delay, bufferbloat); below BDP the link is underutilized. Section 12's
Copa-lite anchor sizes cwnd to ≈ 1·BDP. The quantity is foundational to
congestion control: Jacobson & Karels observed that the largest sensible
window for a path is the bottleneck bandwidth times the round-trip delay
[Jacobson1988]. See Section 12.

**DAPS (Delay-Aware Packet Scheduling)** — a multipath transport
scheduler that assigns each packet to a subflow by its *expected arrival
time*, deliberately sending out of order (a packet destined to arrive
later goes earlier, offset by the RTT skew between paths) so that packets
arrive at the receiver in order despite that skew. This proactively
minimizes receiver-buffer blocking — the head-of-line (HOL) stall where a
late packet on a slow path holds up already-arrived fast-path packets in a
reliable in-order stream [Sarwar2013, Kuhn2014]. It is one of a family of
blocking-aware MPTCP schedulers: BLEST [Ferlin2016] skips a slow subflow
when using it would cause HOL blocking; ECF [Lim2017] (Earliest Completion
First) sends on the subflow that delivers the data soonest under path
heterogeneity; both contrast with MPTCP's default minRTT scheduler, which
fills the lowest-RTT subflow's congestion window first. raptorpath sidesteps
HOL blocking entirely — the FEC decoder needs any k of n symbols, so arrival
order does not matter (Section 13.1).

### 1.3 Components

```
 Sender                       Channel                      Receiver
┌───────────┐              ┌───────────┐              ┌───────────┐
│ Source    ├─ source ────►│           ├─ surviving ─►│ Decode    │
│ packets   │   syms       │  Erasure  │              │           │
│           ├─ correction ►│  Channel  ├─ surviving ─►│ FEC       │
│ Taper     │   symbols    │  (GE)     │              │ Decoder   │
│ Function  │              │           │              │           │
│           │◄─ ACK+SACK ──┤           │◄─ ACK+SACK ──┤ Gap       │
│ Retransmit│              │           │              │ Detect +  │
│ Buffer    │              │           │              │ SACK      │
└───────────┘              └───────────┘              └───────────┘
```

The system provides **reliable** delivery. Three properties form a triangle —
bandwidth, tail latency, and reliability. Fix any two, the third is determined
by the channel. The protocol hint selects which two to fix.

One unified mechanism — **correction symbols** — handles both proactive and
reactive recovery. The taper function controls correction symbol density, and
each correction slot either retransmits an exact source symbol from the
retransmit buffer (preferred, if old enough) or generates a random repair
symbol (FEC fallback):

| Aspect         | Correction Symbol (retransmit) | Correction Symbol (FEC repair) |
|----------------|-------------------------------|-------------------------------|
| When chosen    | With probability P_lost(t) | With probability 1-P_lost(t) |
| Content        | Exact copy of source symbol   | Random linear combination      |
| Decode cost    | Zero (immediate use)          | Needs FEC decoder              |
| Bandwidth cost | Same (one symbol slot)        | Same (one symbol slot)         |
| Latency cost   | Depends on when P_lost triggers | Zero additional (arrives with source) |

### 1.4 The Bandwidth / Latency / Reliability Triangle

Three properties are linked by the channel. Fix any two, the third is determined:

```
              Bandwidth (r)
              correction symbols
              per source symbol
                   / \
                  /   \
                 / FIX \
                / any 2 \
               / compute \
              /  the 3rd  \
             /             \
            /_______________\
  Tail latency (δ)      Reliability (ρ)
  P(late delivery)      P(symbol delivered)
```

| Mode | Fix | Compute | Use case |
|------|-----|---------|----------|
| **Bulk transfer** | ρ=100%, minimize r | δ (tail latency) | File transfer, backup |
| **VoIP** | δ (max latency), r (codec rate) | ρ (reliability) | Interactive voice |
| **Live video** | δ (frame deadline), ρ (≥99.9%) | r (bandwidth) | Streaming, conferencing |
| **Gaming** | δ (tight), ρ (≥99%) | r (bandwidth) | Real-time game state |
| **Sensor/IoT** | r (minimal), ρ (≥95%) | δ (tail latency) | Periodic telemetry |

For Bulk transfer, "minimize r" is taken literally: the tail target is
"late is fine" (δ_bulk = ε̂ + (0.05 − ε̂)·χ, Sections 5.3 and 14.26), so
r = 0 identically in the steady state (χ = 0) and the residual is pure
ARQ — except over the final ~1.5 SRTT of a known-length transfer, where
the completion-exposure χ ramps r up to the tail budget and buys
completion time (Sections 14.25, 14.26).

The protocol hint selects the mode and constraints:

```
 INPUTS                      OPTIMIZATION            OUTPUTS
┌──────────────────────┐    ┌────────────────┐    ┌────────────────┐
│ Mode (from protocol  │    │                │    │                │
│ hint): which two     ├───►│ Given two,     ├───►│ Third          │
│ properties to fix    │    │ compute the    │    │ property       │
│                      │    │ third via      │    │                │
│ Constraint values    ├───►│ the taper      ├───►│ τ*(t) = opt.   │
│ (δ, ρ, or r)         │    │ function       │    │ taper func     │
│                      │    │                │    │                │
│ Channel observations ├───►│                │    │ T_cut = taper  │
│ (e, p, q, RTT)       │    │                │    │ cutoff time    │
└──────────────────────┘    └────────────────┘    └────────────────┘
```

When ρ < 100%, the taper has a finite cutoff T_cut. Symbols not recovered by
T_cut are permanently lost. When ρ = 100%, T_cut = ∞ (correction symbols
continue until ACK, the infinite-tailed taper from Section 4).

---

## 2. Channel Model

### 2.1 Gilbert-Elliott Two-State HMM

**Why a two-state model?** Real wireless channels don't lose packets
independently. WiFi interference causes consecutive packet drops during
fading events. LTE handovers create burst gaps as the device switches
cells. Satellite links suffer weather-induced outages lasting many packets.

An independent (i.i.d.) loss model — where each packet is lost with
probability ε regardless of its neighbors — badly underestimates the
probability of burst loss. If ε = 5%, the i.i.d. model predicts
P(3 consecutive losses) = 0.05^3 = 0.0125%. In reality, once the channel
enters a bad state, consecutive losses are highly correlated, and
P(3 consecutive) might be 2-5% — orders of magnitude higher.

The Gilbert-Elliott model captures this **memory** in the loss process with
just two parameters (p, q). The channel switches between a Good state
(no/low loss) and a Bad state (high/total loss). Once in the Bad state,
it tends to STAY there for multiple symbols (a burst). This simple structure
is rich enough to match real channel behavior [Gilbert1960] yet tractable
enough for closed-form analysis — the burst survival function (1-q)^t
directly gives us the optimal taper function shape (Section 4).

The channel alternates between Good (low loss) and Bad (high loss) states
[Gilbert1960], [Elliott1963]:

```
            p
     .---------->.
     |           |
   +---+       +---+
   | G |       | B |
   |   |<------|   |
   +---+   q   +---+
     |           |
     '---. .-----'
      1-p   1-q
      (self-loops)

   p   = P(Good -> Bad)  = probability of entering a burst
   q   = P(Bad -> Good)  = probability of exiting a burst
   1-p = P(Good -> Good) = probability of staying in Good
   1-q = P(Bad -> Bad)   = probability of burst continuing
```

**Simplified model** (used throughout): in Good state, no loss (h_G = 0).
In Bad state, total loss (h_B = 1). For packet-level erasure channels, this
is not an approximation — it is the correct model. UDP datagrams either
arrive intact (transport-layer checksums verify integrity) or are fully
dropped. There is no partial packet delivery. A symbol is either received
completely or lost completely.

This would be an approximation for bit-level or symbol-level channels where
partial corruption exists (e.g., analog radio). But raptorpath operates at
the packet level over UDP/QUIC, where checksum verification makes h_G = 0
and h_B = 1 exact.

### 2.2 Stationary Properties

Stationary state probabilities:

```
   π_B = p / (p + q)       probability of being in Bad state
   π_G = q / (p + q)       probability of being in Good state
```

Average loss rate:

```
   e = π_B x h_B + π_G x h_G = π_B = p / (p + q)
```

### 2.3 Burst Length Distribution

Given we just entered the Bad state, the burst length T follows a geometric
distribution:

```
   P(T = t) = q x (1-q)^(t-1)        for t = 1, 2, 3, ...

   P(T ≥ t) = (1-q)^(t-1)            survival function

   E[T] = 1/q = B                     mean burst length
```

This survival function is the key quantity — it tells us, given that a burst
started, how likely it is to still be ongoing after t symbols.

### 2.4 Concrete Examples

| Scenario   | ε (loss) | p (G→B)  | q (B→G) | B = 1/q | Character          |
|------------|----------|----------|---------|---------|---------------------|
| DC         | 0.1%     | 0.0005   | 0.5     | 2.0     | Rare, short bursts  |
| WiFi       | 2.5%     | 0.013    | 0.5     | 2.0     | Moderate, short     |
| LTE        | 5%       | 0.02     | 0.4     | 2.5     | Moderate, medium    |
| Satellite  | 9%       | 0.03     | 0.3     | 3.3     | Frequent, long      |
| Bad WiFi   | 15%      | 0.053    | 0.3     | 3.3     | Frequent, long      |

(Consistency check: ε = p/(p+q). E.g. DC: 0.0005/0.5005 ≈ 0.1%;
WiFi: 0.013/0.513 ≈ 2.5%; Satellite: 0.03/0.33 ≈ 9.1%.)

### 2.5 GE Adequacy vs Real Traces (MEASURED-against-real-traces)

Every result downstream of this section is proven *for a GE world*: the
formula, the oracles, and the netem rung all draw loss from the two-state chain
above. This subsection tests the assumption itself against **real** cellular
link traces and reports where GE is — and is not — adequate.

**Traces and loss derivation.** Five real U.S. cellular capacity traces
(Verizon/AT&T/T-Mobile LTE/UMTS, recorded with the *Saturator* tool,
Winstein et al., NSDI 2013; via the mahimahi repository) are replayed. These
are *capacity* traces (per-ms 1500-byte delivery opportunities), so a loss
process is derived honestly with a drop-tail queue: a rate-controlled sender
offers ρ = 0.5 of the trace's mean capacity into a 64-packet buffer drained at
the trace's *instantaneous* capacity; a packet arriving to a full buffer is
dropped. A real capacity fade (an outage in the trace) backs the queue up and
overflows a burst of packets — the real-world "outage → loss burst" GE is meant
to model. Derived loss rates span ε = 5.2%–24.5%. (Harness:
`raptorpath-math/tests/real_trace_validation.rs`; provenance:
`tests/data/traces/PROVENANCE.md`.)

**What GE misses (structure).** Fitting GE to each real loss sequence the way
production does (transition-count p̂, q̂) and comparing the fit's own
predictions against the trace:

- **Long memory.** GE is Markov: autocorrelation decays as ρ(L) = (1−p−q)^L,
  effectively zero by lag 20. Real loss stays strongly correlated far beyond
  lag 1 — measured lag-20 autocorrelation is **5×–4100×** what the fitted GE
  predicts (e.g. Verizon-LTE-short: real 0.54 vs GE 0.0001). This is the
  headline mismatch: GE is memoryless beyond the current state; real fades are
  not.
- **Heavy burst tail.** GE burst lengths are geometric; real fade bursts are
  **3.8×–26×** heavier in the extreme tail (max real bursts of 210–597 symbols
  vs a geometric expectation of tens).
- **Non-stationarity.** Within a single trace, ε drifts across regimes (0%–87%
  across sixths) and q̂ swings by up to 0.47 — a single stationary (p, q) cannot
  represent the process.

**Consequence for r\* (fidelity).** Feeding the *actual* real loss sequence
through the FEC/ARQ window process at the r\* the closed form (§8.4) prescribes —
fitted to the trace's own (ε, σ²_burst), with the full burst-variance margin —
the achieved residual window-failure **under-provisions**: it runs up to **12.8×**
the target δ/ε, and **1.2×–3.7×** worse than the GE-ideal that the model predicts
for the *same* r\* (1 − P_fec via the §8.7 exact DP). The gap beyond the GE-ideal
is *pure channel-model mismatch*: it persists even at r\* ≈ 55%–100% overhead and
even when the exact-DP r\* (§8.7) is used, because both target the GE curve. The
σ²_burst correction (§8.3) is a **partial** answer — it inflates the *lag-1*
variance but cannot capture long memory, heavy fade tails, or regime shifts.

**Verdict.** For our r\*, **GE is INADEQUATE on real single-path loss**: it
systematically under-provisions the tail (by ~2×–4× beyond its own prediction),
because real loss carries structure — long memory, heavy fade tails,
non-stationarity — that a stationary two-state Markov chain omits. The
*aggregation* result is more robust: replaying two independent real traces as
two paths through the stable-generation design (§16.3) still aggregates above
the fast path (×1.178, tracking the GE control ×1.180 and the real goodput
ceiling ×1.188), so real per-path *dynamics* do not break the coding mechanic.

**Enrichment (BUILT — Section 8.4.1, task #46).** The recommendation that
closed this section's earlier revisions — *provision r\* against the empirical
window-loss quantile rather than the Gaussian/GE tail* — is now implemented and
shipped. The receiver measures the multi-scale tail of its own **window
loss-mass** (losses per m-block span; the exact window-failure statistic — a
window fails iff its total loss mass exceeds its repair count, Section 8.4.1),
fits a discrete-Weibull tail to it (the k = 1 case is exactly GE's geometric
law), and r\* is the larger of the Section 8.4 closed form and the mass-quantile
rate. Validation through the same replay machinery as this section
(`raptorpath-math/tests/rstar_tail_validation.rs`): on GE draws the corrected
r\* tracks the Section 8.7 exact optimum (×0.92–1.11, no over-provisioning); on
the five real traces the worst delivered residual over feasible cells improves
from **2.88× to 1.41×** the target (the remainder is non-stationarity), and
cells no in-window rate ≤ 200 % can meet (deep multi-window fades) are
**declared infeasible** by the solver rather than silently missed. What remains
open beyond the tail fix: regime-switching non-stationarity (partially absorbed
by the BOCD posterior and the decayed counters) and cross-path correlation
(below).

**Correlation gap (open).** Public single-path traces are independent by
construction, so the multipath result above tests real per-path *dynamics* but
**not** path *correlation* (shared-bottleneck WiFi+LTE losing together).
Settling the correlation question needs simultaneous dual-link capture or a
dual-radio hardware testbed — the remaining rung of the validation ladder.

---

## 3. Recovery Fundamentals

### 3.1 The Problem: Recovering Lost Symbols

When a source symbol is erased by the channel, the receiver has a gap in the
data stream. There are two fundamentally different ways to fill that gap:

1. **Send extra data proactively** (before knowing what will be lost): the
   sender generates redundant symbols alongside source data. If a source
   symbol is lost, the redundant symbols can reconstruct it. This costs
   bandwidth (the redundant symbols are sent whether needed or not) but adds
   no latency (they arrive at roughly the same time as the source).

2. **Detect the loss and resend** (after knowing what was lost): the sender
   discovers that a symbol wasn't received (via ACK timeout) and retransmits
   it. This costs latency (at least one round-trip to detect + retransmit)
   but wastes no bandwidth (only lost symbols are resent).

These two approaches are called **FEC** (Forward Error Correction) and **ARQ**
(Automatic Repeat reQuest). The challenge is finding the right balance between
them — more FEC means lower latency but higher bandwidth; more ARQ means lower
bandwidth but higher latency.

### 3.2 FEC: Proactive Recovery

**The encoder window.** The sender maintains a sliding window of the W most
recent source symbols. This window is the encoder's "memory" — it tracks
which source symbols are available for generating repair:

```
  Encoder window (W = 5 symbols):

  sent:   [S1] [S2] [S3] [S4] [S5] [S6] [S7] [S8] ...
                     |----- window ------|
                     S3  S4  S5  S6  S7

  As new symbols arrive, old ones are evicted from the left.
```

**Repair symbols** are random linear combinations of source symbols in the
window. Each repair symbol is computed as:

```
  R = c1*S3 + c2*S4 + c3*S5 + c4*S6 + c5*S7
```

where c1..c5 are random coefficients in GF(256) (a finite field — arithmetic
wraps around so values stay within 0-255). Each repair symbol is a different
random combination, giving the decoder independent equations.

**How decoding works.** If k source symbols in the window are lost, the
receiver needs at least k repair symbols that survived the channel. Each
repair symbol provides one linear equation relating the lost symbols. With
k equations and k unknowns, the decoder solves the system (Gaussian
elimination over GF(256)) to recover the lost data.

```
  Example: source S3 and S5 lost, S4/S6/S7 received

  Time ----------------------------------------------------------->

  Source:  [S1] [S2] [S3] [S4] [S5] [S6] [S7]
                       X         X              <- lost
  Repair:    [R1]  [R2]    [R3]       [R4]
              |     |        |          |
              v     v        v          v
         covers   covers   covers    covers
         S1-S3    S2-S4    S3-S6     S5-S7

  Decoder receives R1, R2, R3, R4 and knows S4, S6, S7.
  Substituting known values into R3 and R4:
    R3 = c1*S3 + c2*(known S4) + c3*S5 + c4*(known S6)
    R4 = c5*S5 + c6*(known S6) + c7*(known S7)
  -> 2 equations, 2 unknowns (S3, S5) -> solve.
```

**Systematic codes.** In our system, source symbols are sent as-is — they
are not encoded. This means if nothing is lost, the receiver can use the
data immediately without any decoding. The decoder is only invoked when at
least one source symbol is missing. This is important for efficiency: at
low loss rates, the decoder rarely runs.

**Codec overhead.** Not all random linear combinations are independent.
Some may be (near-)linearly dependent, providing redundant equations. This
means the decoder sometimes needs slightly more repair symbols than the
number of losses. This extra requirement is the codec overhead — e.g., 1%
for RaptorQ means the decoder needs 1% more symbols than the theoretical
minimum of k.

**Properties of FEC:**
- Bandwidth cost: r repair symbols per source symbol (always-on, paid whether
  loss occurs or not)
- Latency cost: zero additional (repair arrives at roughly the same time as
  source)
- Bandwidth fraction used: r/(1+r) of total link capacity

### 3.3 ARQ: Reactive Recovery

ARQ recovers lost symbols by retransmitting them after detecting the loss.
In our system, loss detection is **sender-side**: the sender tracks which
symbols have been ACKed and retransmits any that remain un-ACKed after a
timeout.

This is how TCP has worked for 40 years [RFC2018] and is proven robust for
unicast connections. The alternative (receiver-side detection via NACK
messages) adds detection delay and is fragile when the reverse path is lossy.

**Timeline of ARQ recovery:**

```
  Time ---------------------------------------------------------->

  Sender:  [S1] [S2] [S3] [S4] [S5] ...            [S3']
                       |                              ^
                       X lost                         | retransmit
                       |                              |
  Receiver: S1   S2  (gap)  S4   S5   ... ACK ...    S3'
                                        |             immediate
                                   ACK says:          use!
                                   "got up to S2,
                                    SACK: S4,S5"
            |                           |             |
            |<----- T_retx ------------>|<-- RTT/2 -->|
            |   (sender waits for       | (one-way    |
            |    ACK, then times out)   | propagation)|
```

The sender sent S3 at time 0. After T_retx (roughly one RTT, enough time for
an ACK to return), no ACK has confirmed S3. The sender retransmits S3. The
retransmitted symbol travels one-way (RTT/2) to the receiver.

**Key advantage over FEC:** A retransmitted source symbol is immediately usable
by the receiver — no decoder needed, no dependency on other symbols. The
receiver gets the exact data it was missing and can process it right away.

**Key disadvantage:** The retransmission adds L_arq = T_retx + RTT/2 of latency.
For a link with 50ms RTT and T_retx = 75ms: L_arq = 100ms. This is acceptable
for bulk transfer but too slow for real-time applications.

**Properties of ARQ:**
- Bandwidth cost: approximately ε per source symbol (only retransmit what's
  actually lost — no waste)
- Latency cost: L_arq = T_retx + RTT/2 per recovery event
- Self-healing: if the ACK itself is lost on the reverse path, the sender just
  waits longer and retransmits anyway. No special mechanism needed.

### 3.4 Recovery Latency and the P_lost(t) Model

A lost symbol is recovered by whichever mechanism finishes first — FEC or ARQ.
Rather than a hard timeout threshold, the choice between repair and retransmit
is **probabilistic**, based on the sender's confidence that a symbol was lost.

**FEC recovery time (t_fec):** After a loss, repair symbols keep arriving.
Every repair is a linear combination of the entire encoder window (Section
3.2), so every surviving repair gives the decoder one more equation for the
lost symbol — not just the repairs "attributed" to that symbol by its own
taper. In steady state, corrections occupy a fraction r/(1+r) of wire slots
(Section 5.3), so useful equations arrive at rate r/(1+r) × (1-e) per slot.
For m lost symbols in the window, the decoder needs m surviving repairs:

```
  t_fec = m x (1+r) / (r x (1-e)) x t_sym

  where:
    m     = number of lost symbols in the window (usually 1)
    r     = total correction rate (corrections per source symbol)
    1-e   = probability each repair survives the channel
    t_sym = symbol_size / throughput (time to transmit one symbol)
```

Concrete examples for a single loss (m=1), r=0.08, e=0.025:

```
  At 100 Mbps, 1200-byte symbols:  t_sym = 0.096ms, t_fec = 1.3ms
  At  10 Mbps, 1200-byte symbols:  t_sym = 0.96ms,  t_fec = 13.3ms
  At   1 Mbps, 1200-byte symbols:  t_sym = 9.6ms,   t_fec = 133ms
```

For burst loss (m=5 on WiFi at 100 Mbps): t_fec = 5 x 1.3ms = 6.6ms.

**P_lost(t): the probability a symbol was lost.** At time t after sending a
symbol, given no ACK has arrived, what is the probability it was actually lost?

If the symbol was received, the ACK should arrive after roughly one RTT. If
the symbol was lost, no ACK will ever come (until a correction recovers it).
Using Bayes' theorem:

```
  P_lost(t) = e / [e + (1-e) x P(RTT > t)]

  where:
    e          = channel loss rate (prior probability of loss)
    P(RTT > t) = probability the ACK is delayed beyond time t
               = tail of the RTT distribution (from SRTT and RTTVAR)
```

Assuming RTT is normally distributed with mean SRTT and standard deviation
RTTVAR: P(RTT > t) = 1 - Phi((t - SRTT) / RTTVAR), where Phi is the
standard normal CDF. This is the same normal approximation used in TCP's
RTO computation [RFC6298].

This gives a smooth transition from "probably fine" to "certainly lost":

```
  t = 0:            P_lost = e              (just the base loss rate)
  t = SRTT:         P_lost ≈ 2e             (ACK expected by now)
  t = SRTT + 2s:    P_lost ≈ 0.5            (loss is now the likelier explanation)
  t = SRTT + 4s:    P_lost > 0.99           (very confident it's lost)
  t >> SRTT:        P_lost -> 1.0            (certainly lost)
```

Concrete example (WiFi, e = 0.025, SRTT = 50ms, RTTVAR = 5ms):

```
  t = 0ms:    P_lost = 0.025    -> 97.5% repair, 2.5% retransmit
  t = 40ms:   P_lost = 0.026    -> 97% repair, 3% retransmit
  t = 50ms:   P_lost = 0.049    -> 95% repair, 5% retransmit
  t = 55ms:   P_lost = 0.14     -> 86% repair, 14% retransmit
  t = 60ms:   P_lost = 0.53     -> 47% repair, 53% retransmit
  t = 70ms:   P_lost = 0.999    -> ~0% repair, ~100% retransmit
```

Note how sharp the transition is: P_lost stays near ε until roughly SRTT +
1×RTTVAR, then rises to near-certainty within another 2-3×RTTVAR. The
smoothness of the mix in practice comes from the spread of symbol ages in
the retransmit buffer, not from the curve itself being gradual.

The per-slot decision uses P_lost(t) directly:

```
  P(retransmit) = P_lost(t_k)
```

**No hard threshold.** The transition from repair to retransmit is driven by
TIME since send. Early (t small): P_lost is low, so corrections are mostly
repair — flexible coverage for uncertain losses. Late (t >> SRTT): P_lost
approaches 1, so corrections are mostly retransmit — targeted recovery of
confirmed losses. No tuning knobs.

See Appendix C.6 for an optional channel-state discount (ε_burst) that
provides modest improvement for long-burst scenarios.

**Bandwidth efficiency.** A retransmit is wasted if the symbol wasn't actually
lost (duplicate). A repair is never wasted (always provides information to the
decoder). The expected waste per correction slot is:

```
  E[waste] = P(retransmit chosen) x P(symbol not actually lost)
           = P_lost(t) x (1 - P_lost(t))
```

This is maximized at P_lost = 0.5 (maximum uncertainty) and zero at the
extremes. The probabilistic model automatically minimizes waste.

**Proactive retransmit emerges naturally.** At high loss rates (e.g., e = 0.5),
P_lost(0) = 0.5 — half the correction slots are retransmits even at t = 0.
This is proactive redundant source sending, no special mode needed. At low
loss rates (e = 0.001), P_lost(0) = 0.001 — virtually all repair. The model
adapts to the channel automatically.

**Which mechanism wins on latency?** FEC recovery (t_fec) is typically much
faster than ARQ recovery (waiting for P_lost to rise + RTT/2):

```
  Time ---------------------------------------------------------->
  t=0          t_fec                                    retransmit
  |             |                                       arrives
  S3 lost       FEC decodes      P_lost rises,           |
                (if enough       retransmit chosen       |
                 repairs         in correction slots     |
                 arrived)                                |
  |<-- FEC -->|
  |<---- taper generating corrections the whole time --->|
  |     (gradually shifting from repair to retransmit)   |
```

At 100 Mbps on WiFi: t_fec = 1.3ms. Most losses are FEC-recovered long
before P_lost rises high enough for retransmission. ARQ retransmit is only
relevant for burst losses that overwhelm the FEC budget.

---

## 4. The Taper Function

Why not send correction symbols at a constant rate? If loss were uniformly
distributed over time (i.i.d.), a constant rate would be optimal — every
position is equally likely to need correction. But on real wireless channels,
loss comes in **bursts** (Section 2). A burst wipes out consecutive symbols,
then the channel recovers. Right after a burst starts, the probability of
continued loss is high; as time passes, it decays exponentially.

A constant correction rate misallocates: it sends too many corrections during
good periods (wasted) and too few right after a burst (when they're needed
most). The taper function solves this by matching correction density to the
burst survival probability — more corrections where loss is likely, fewer
where it isn't. This minimizes the total correction budget for a given
recovery target.

### 4.1 Definition

The taper function τ(t) specifies the correction density at time offset t from
a source symbol. At offset t after symbol s enters the window, we generate
τ(t) correction symbols covering s.

```
  Correction
  density
  τ(t)
    |
  A +--\
    |   \
    |    \
    |     \
    |      \
    |       \------------------------------ (never reaches 0)
    |
    +--+------+------+------+-------------- time offset t
    0  B     2B     3B     4B    ...

  τ(t) = A x (1-q)^t

  A     = amplitude (what we solve for)
  (1-q)^t = GE burst survival function
```

### 4.2 Why Match the Loss Distribution?

The taper should allocate more correction where it does the most good. Given
that a symbol was lost (we're in a burst), the conditional probability that
the burst is still active at offset t is:

```
   P(burst active at offset t | burst at offset 0) = (1-q)^t
```

A correction at offset t is useful only if BOTH (a) the lost symbol still
needs help at t, and (b) the correction itself survives the channel. These
pull in opposite directions: (a) decays like the burst survival function
(1-q)^t, while (b) — conditioned on the loss at offset 0 — is the
probability the burst has ENDED by t, roughly 1 - (1-q)^t. Corrections sent
into an ongoing burst are themselves lost (Section 14.15). The product

```
   benefit(t) ∝ (1-q)^t × (1 - (1-q)^t)
```

is hump-shaped: zero at t = 0, peaking around one mean burst length
(t ≈ B), then decaying like (1-q)^t. The taper's exponential decay matches
the correct TAIL behavior; its peak at t = 0 is an approximation that
over-weights the first few offsets, where corrections have the lowest
conditional survival probability. Two effects limit the cost of this
approximation:

1. **Repairs are window-fungible.** Every repair covers the whole encoder
   window (Section 3.2), so a correction "attributed" to offset 0 of a fresh
   symbol simultaneously sits at larger offsets for every older symbol in
   the window. Per-symbol attribution is bookkeeping, not physics — recovery
   depends on the aggregate correction rate (Section 8.2), which the shape
   does not change in steady state (see below).

2. **The exponential tail is the operationally important part.** The (1-q)^t
   decay controls how long un-ACKed (increasingly likely lost) symbols keep
   drawing correction coverage after everything else has been ACKed.

**Steady-state shape invariance.** With a continuous source stream, the
aggregate correction rate per wire slot is Σ_t τ(t) = r regardless of the
taper's shape — every in-window symbol contributes its taper at a different
age, and the sum over ages telescopes to the same total. The shape only
affects behavior in transients: when the source pauses, when the window is
not yet full, and — most importantly — through ACK truncation (Section 4.4):
once a symbol is ACKed, its taper contribution stops, so beyond one RTT only
un-ACKed (likely lost) symbols continue to generate corrections. The decay
rate q determines how aggressively that residual coverage tapers off. The
taper shape is therefore a policy for allocating corrections across
UNCERTAIN symbols, not a mechanism that changes the steady-state budget.

**Truncation at the stream end (forward integral).** The shape-invariance
argument assumes the sum telescopes over a FULL window of ages — every
in-window symbol present at every offset. That holds mid-stream but breaks at
the END of a finite transfer. A symbol at source position i draws correction
coverage from repairs generated while it is in the encoder window, i.e. over
the FUTURE source positions [i, i+W). When i+W exceeds the last source
position N, those future positions never arrive, so the symbol's forward
taper integral is TRUNCATED: a symbol at distance j = N − i from the end
(j < W) receives coverage from only j of its W window-lifetimes, i.e. it is
missing ≈ r·(W − j) of its steady-state repairs. The last W symbols are
therefore progressively under-covered (full at j = W, ~none at j = 0), and
because end-of-stream losses have nothing to overlap their recovery with,
each falls to a serial ARQ round (~1.5 RTT). This is the end-of-stream
reliability cliff; Section 14.29 derives the loss and the completion term
that refills it for every hint (Section 14.26 is the Bulk special case).

For an i.i.d. channel (q = 1, no burst memory): τ(t) = constant (flat taper).
This is correct — every position is equally likely to need correction.

### 4.3 Total Correction Rate

The total correction rate (correction symbols per source symbol) is:

```
   r = Σ_{t=0}^{∞} τ(t) = A x Σ_{t=0}^{∞} (1-q)^t = A / q
```

Since 0 < q ≤ 1, this geometric series converges. Therefore:

```
   A = r x q
```

The amplitude is uniquely determined by the correction rate and the GE parameter.

### 4.4 The Taper Never Reaches Zero

The exponential (1-q)^t is always positive for 0 < q < 1. This is correct
behavior: there is always a nonzero probability of a burst still continuing.
As long as a symbol has not been ACK'd, there is a nonzero probability it
was lost, so we should continue generating (increasingly rare) correction coverage.

```
  t = 0:    τ(0) = A                        peak correction density
  t = B:    τ(B) = A x e^{-1} ≈ 0.37 x A   one mean burst length
  t = 2B:   τ(2B) = A x e^{-2} ≈ 0.14 x A  two mean burst lengths
  t = 5B:   τ(5B) = A x e^{-5} ≈ 0.007 x A five mean burst lengths
  t → ∞:    τ(t) → 0                        but never zero
```

In practice, once a symbol is ACK'd, we stop generating correction for it (the
encoder window advances past it). The theoretical infinite tail is truncated
by the ACK mechanism.

### 4.5 Real-Time Adaptation

The taper function adapts in real time through two mechanisms:

1. **GE parameter updates**: The estimator continuously tracks q (and p) from
   observed loss patterns. As q changes, the taper shape changes — slower
   decay for longer bursts, faster decay for shorter bursts.

2. **BOCD changepoint detection**: If the loss regime changes abruptly (e.g.,
   path switches from WiFi to LTE), BOCD detects the changepoint within 5-15
   batches (a batch = one ACK feedback cycle, typically at intervals of 10-100ms depending on sending rate and path RTT) and widens the posterior, increasing the correction budget until the
   new regime is characterized.

```
  Before changepoint:            After changepoint:
  (short bursts, q=0.5)          (long bursts, q=0.2)

  τ(t)                           τ(t)
    |                              |
  A +\                           A'+--\
    |  \                           |    \
    |   \                          |      \
    |    \                         |        \
    |     \------                  |          \-----------
    +------------ t                +---------------------- t
    fast decay                     slow decay, higher A'
```

---

## 5. Unified Correction Symbol Model

### 5.1 Why Unify FEC and ARQ?

Both FEC and ARQ produce one symbol on the wire. Both cost the same bandwidth
per symbol. The difference is timing and content:

```
  FEC repair:        generated immediately, random combination, needs decoder
  ARQ retransmit:    generated after timeout, exact source copy, immediately usable

  Both occupy one symbol slot on the wire.
  Both are subject to the same channel loss rate e.
  Both serve the same purpose: recover a lost source symbol.
```

The key insight: **the taper function controls WHEN to generate correction
symbols** (the density schedule). **The per-slot decision controls WHAT to
generate** (repair or retransmit). These are orthogonal choices:

- **Early in the taper** (right after sending source): P_lost(t) is low, so
  almost all correction symbols are FEC repair. This is pure proactive
  protection — we don't yet know what's lost.

- **Late in the taper** (t >> SRTT): P_lost(t) approaches 1, so correction
  symbols are almost all retransmits. We're confident about which symbols
  are lost and send the exact data the receiver needs.

- **In between**: a smooth probabilistic mix. No hard phase switch.

This means the taper function **naturally transitions from FEC to ARQ** as
time passes, driven by the P_lost(t) posterior. The unified mechanism is
called a **correction symbol** — it is either a repair symbol or a source
retransmit, decided probabilistically at generation time.

### 5.2 Correction Symbols — The Unified Concept

A **correction symbol** is any symbol sent to recover lost data. It occupies
one symbol slot on the wire and serves one of two purposes:

| Aspect         | Source retransmit (ARQ)              | Repair symbol (FEC)               |
|----------------|--------------------------------------|-----------------------------------|
| When chosen    | With probability P_lost(t)           | With probability 1-P_lost(t)      |
| Content        | Exact copy of source symbol          | Random linear combination         |
| Receiver action| Immediate use, no decoder            | Feed to FEC decoder               |
| Bandwidth cost | Same (one symbol slot)               | Same (one symbol slot)            |
| Waste risk     | Duplicate if symbol wasn't lost      | Never wasted (always useful)      |
| Best for       | Confirmed loss + good channel        | Uncertain loss or burst (flexible)|

The taper function (Section 4) determines the **density** of correction
symbols over time. This section explains what happens in each slot.

### 5.3 Three-Stream View: Source, Repair, Retransmit

Conceptually, the sender manages **three streams** that compete for wire
capacity. Each stream has a different effect on the bandwidth/latency/
reliability triangle:

```
  Stream         | Latency       | Bandwidth     | Reliability
  ---------------+---------------+---------------+--------------
  Source         | ++ immediate  | neutral       | neutral
  Repair (FEC)   | - decoder     | + (no waste   | ++ (covers
                 |   wait        |   if taper    |    any loss)
                 |               |   matched)    |
  Retransmit     | + immediate   | - (duplicate  | + (targeted
  (ARQ)          |   decode      |   risk)       |    recovery)
```

The hierarchical model (taper + P_lost) determines the three-stream mix:

```
  Total capacity: C (from Copa)

  source_rate     = C / (1 + r)                          [from taper ratio r]
  retransmit_rate = C x r / (1+r) x P_lost(t)            [confirmed losses]
  repair_rate     = C x r / (1+r) x (1 - P_lost(t))      [uncertain losses]

  where r        = taper correction ratio (Section 4)
        P_lost(t) = loss confidence (Section 3.4)
```

The three rates sum to C. The taper ratio r controls the source/correction
split. P_lost(t) controls the repair/retransmit split within corrections.

**How the protocol hint shifts the mix:**

```
  Realtime (low latency):
    -> tight delta -> high r (more corrections for fast FEC recovery)
    -> BUT also high P_retx when confident (retransmit = immediate decode)
    -> Mix: moderate source, moderate repair, moderate retransmit

  Bulk (high bandwidth):
    -> loose delta -> low r (fewer corrections, more source)
    -> low P_retx (prefer repair over retransmit, less duplicate risk)
    -> Mix: lots of source, some repair, little retransmit

    Concretely, Bulk's tail target is "late is fine", weighted by the
    completion exposure chi (Section 14.26):

      delta_bulk = epsilon_hat + (delta_tail - epsilon_hat) x chi(T_rem)

    Mid-stream chi = 0, so delta_bulk = epsilon_hat and the continuous
    r* formula (Section 8.4, z_{delta/epsilon} = Phi^-1(1 -
    delta/epsilon) = Phi^-1(0) = -inf) yields r = 0 IDENTICALLY in the
    steady state — independent of estimator uncertainty: pure ARQ, wire
    volume at parity with retransmission transports (~1 +
    epsilon/(1-epsilon) per source symbol). A mid-transfer loss
    recovered one RTT late costs a bulk transfer nothing — recovery
    overlaps ongoing sends. The one place FEC still buys completion
    time is the stream tail, where chi rises smoothly to 1 and
    delta_bulk glides to the delta_tail = 0.05 completion budget
    (Sections 14.25, 14.26).

  VoIP (fixed bandwidth + latency, variable reliability):
    -> r constrained by bandwidth budget
    -> high P_retx when confident (immediate decode matters)
    -> Mix: fixed source rate, split corrections by P_retx
```

**Per-path three-stream fractions:** Each path has its own r_i and P_retx_i,
so the three-stream fractions naturally differ per path. A low-latency path
with low loss has: high source, low repair, low retransmit. A high-loss path
has: lower source, high repair, moderate retransmit. The hierarchical model
produces these automatically — no separate three-way optimization needed.

### 5.4 Per-Slot Decision

When the taper function (Section 4) decides to generate a correction symbol, the sender
makes a probabilistic choice based on P_lost(t) for the oldest un-ACKed
symbol in the retransmit buffer:

```
  Taper decides: "generate a correction symbol now"
                      |
                      v
  +-------------------------------------------+
  | Compute mixing probability:               |
  |                                           |
  |   P_retx = P_lost(t_k)                    |
  |                                           |
  | With probability P_retx:                  |
  |   -> Retransmit exact source              |
  |      (immediate decode at receiver)       |
  |                                           |
  | With probability 1 - P_retx:              |
  |   -> Generate random repair symbol        |
  |      (FEC, covers any loss in window)     |
  +-------------------------------------------+

  P_lost(t_k) = e / [e + (1-e) x P(RTT > t_k)]    per-symbol loss confidence
```

**The FEC/ARQ mix is regulated by TIME since send.** Early correction slots
(t small) have low P_lost — the ACK hasn't had time to return, so loss is
uncertain. Almost all corrections are FEC repair, which flexibly covers any
lost symbol. Late correction slots (t >> SRTT) have high P_lost — the ACK
should have arrived by now, so loss is confirmed. Corrections shift to
retransmit, which is immediately decodable.

This naturally produces Mehrotra & Li's optimal policy [Mehrotra2010]:
during bursts (before RTT elapses), P_lost stays low → mostly repair.
After bursts (ACKs reveal gaps), P_lost rises → targeted retransmit.

The expected WASTE, E[waste] = P_lost × (1 - P_lost) (Section 3.4), is zero
at the extremes (P_lost = 0: all repair, no waste; P_lost = 1: all
retransmit, definitely lost so no waste) and maximized at P_lost = 0.5
(maximum uncertainty). The model automatically allocates the right mix.

See Appendix C.6 for an optional channel-state discount (ε_burst) that
provides modest improvement for long-burst scenarios.

---

## 6. Protocol Mechanics

### 6.1 The Retransmit Buffer

The sender maintains a **retransmit buffer**: an ordered list of source symbols
that have been sent but not yet ACKed.

```
  Retransmit buffer (ordered by send time, oldest first):

  +-----+-----+-----+-----+-----+-----+-----+
  | S12 | S13 | S17 | S18 | S19 | S24 | S25 |  <- un-ACKed symbols
  +-----+-----+-----+-----+-----+-----+-----+
    ^                                    ^
    oldest                               newest
    (first retransmit                    (too recent,
     candidate if                        wait for ACK)
     age > T_retx)
```

**Operations:**
- **Enqueue**: every sent source symbol is added to the tail
- **Dequeue (ACK)**: when an ACK or SACK confirms receipt, remove the symbol
- **Peek (retransmit)**: when generating a correction symbol, check the head —
  if its age exceeds T_retx, retransmit it (but keep it in the buffer until ACKed)

**The retransmit buffer is NOT bounded by the encoder window.** The encoder
window (W symbols) bounds what FEC repair can cover. The retransmit buffer
bounds what ARQ retransmit can cover. They serve different purposes:

- Symbol in encoder window AND buffer: protected by FEC + ARQ
- Symbol evicted from window, still in buffer: protected by ARQ only
- Symbol ACKed: removed from buffer (delivered)

Two independent mechanisms manage the buffer:

**Age eviction (T_cut):** Symbols older than T_cut (derived from the reliability target ρ in Section 8.6) are evicted from the
buffer — accepted as permanently lost. T_cut is derived from the triangle
optimization (Section 8.6): for reliability target ρ, T_cut is the time
where P(not recovered) = 1-ρ. When ρ = 100%, the equation has no finite
solution, so T_cut = ∞ (never evict). This emerges from the math — not a
special case.

**Size backpressure (buffer_max):** When the buffer reaches buffer_max
entries, the sender pauses accepting new source symbols from the
application (backpressure). Correction symbols continue being generated
for existing buffer contents — corrections recover lost symbols, ACKs
arrive, and the buffer drains. Once space is available, source resumes.

buffer_max is derived, not configured:

    For ρ < 100%:  buffer_max = source_rate x T_cut
                   (eviction keeps buffer within this bound)

    For ρ = 100%:  buffer_max = source_rate x (RTT + B_max / (r* x (1-e)) x t_sym)
                   where B_max = ceil(ln(0.0001) / ln(1-q))
                   (99.99th percentile burst length from GE model;
                    ≈ 9.2/q for small q, = 14 exactly at q = 0.5)

Both mechanisms always run. T_cut determines which triggers first:
- Finite T_cut → eviction keeps buffer small → backpressure rarely triggers
- T_cut = ∞ → no eviction → backpressure is the only limit

What bends when:
- ρ < 100%: reliability bends (eviction at T_cut)
- ρ = 100%: latency bends (backpressure at buffer_max, rare)

### 6.2 SACK-Extended ACK

The receiver sends an **ACK per received symbol** — every incoming symbol
triggers an immediate ACK in the reverse direction. This provides the
fastest possible loss detection and the most RTT samples for estimation.

**Overhead:** Each ACK is ~80 bytes (fields + QUIC/UDP headers). With
1200-byte symbols, per-symbol ACKs generate reverse-path traffic of about
80/1200 ≈ 6.7% of the forward data rate. On symmetric links this is an
acceptable price for per-symbol loss detection and RTT sampling. On
strongly asymmetric uplinks (LTE, satellite, DOCSIS) it can be
significant, and the per-batch or piggyback alternatives below become
attractive; the model itself is agnostic to ACK batching (it only slows
the P_lost transition by the batch interval).

**Alternatives considered:**
- Per-batch ACK: lower overhead, but slower loss detection (batch granularity)
- Delayed ACK (TCP style, every 200ms): 50% less traffic, but adds 200ms
  detection delay — too slow for our P_lost model
- Piggyback ACK on reverse-direction data: zero overhead for bidirectional
  traffic — a future optimization for the bidirectional case

**ACK contents (5 fields):**

```
  +-----------------------------------------------------------+
  | cumulative_ack:       u64    all seqs <= this received    |
  | sack_ranges:          [(u64,u64)]  additional ranges      |
  | echo_timestamp:       u64    sender's timestamp           |
  | jitter_us:            u32    interarrival jitter (us)     |
  | cumulative_received:  u64    total symbols received       |
  +-----------------------------------------------------------+
```

- **cumulative_ack**: highest sequence number such that ALL symbols up to and
  including it have been received. An optimization — equivalent to a SACK
  range starting at 0, but compressed to one number.
- **sack_ranges** (Selective ACK [RFC2018]): out-of-order ranges received beyond
  the cumulative point. Tells the sender exactly which symbols arrived despite
  gaps. Cumulative within the T_cut window — all received fragments are
  reported, not just the most recent (unlike TCP which limits to 3-4 blocks).
- **echo_timestamp**: sender's own send timestamp echoed back for RTT
  measurement. RTT = now - echo. No clock synchronization needed.
- **jitter_us**: interarrival jitter in microseconds [RFC3550 A.8]. u32 holds
  up to 4300 seconds (matches RFC 3550's 32-bit field). Used for reorder
  buffer sizing, congestion detection, and real-time jitter budgets.
- **cumulative_received**: running total of symbols received. Self-healing
  counter that survives ACK loss and SACK pruning. Gives the sender a
  reliable aggregate reliability metric: ρ_actual = received / sent.

**Gap pruning at T_cut:** For ρ < 100%, gaps older than T_cut are pruned by
the receiver — cumulative_ack advances past abandoned symbols. Both sender
(buffer eviction) and receiver (gap pruning) use the same T_cut, communicated
at connection start and updatable mid-connection via control message.

For ρ = 100% (T_cut = ∞): no pruning. cumulative_ack advances only when
gaps are filled by corrections. SACK ranges are temporary.

**ACK loss handling:** Cumulative ACKs are self-healing — each new ACK
supersedes all previous ones. If ACK_5 is lost but ACK_6 arrives, the sender
gets all information from ACK_6. No ACK retransmission needed.

### 6.3 Per-Symbol Delivery Outcomes

A lost symbol has three possible outcomes, depending on whether the taper
function's correction symbols recover it before the cutoff T_cut:

```
   Outcome 1: FEC-recovered (proactive repair arrives before T_retx)
     Latency: L_prop + small delay (same as source)
     Probability: e x P_fec

   Outcome 2: ARQ-recovered (retransmit arrives after T_retx, before T_cut)
     Latency: L_prop + L_arq  where L_arq = T_retx + RTT/2
     Probability: e x (1 - P_fec) x P_arq

   Outcome 3: Lost (not recovered by T_cut)
     Latency: infinity (never delivered)
     Probability: e x (1 - P_fec) x (1 - P_arq)
```

**P_arq** is the probability that ARQ retransmission succeeds, given that
FEC failed to recover the symbol. It is a conditional probability in this
chain:

```
    Symbol sent
      |
      +-- (1-e): arrives directly          -> delivered (on-time)
      |
      +-- e: lost on the channel
           |
           +-- P_fec: FEC recovers it      -> delivered (on-time)
           |
           +-- (1-P_fec): FEC failed
                |
                +-- P_arq: ARQ succeeds    -> delivered (late, L_arq)
                |
                +-- (1-P_arq): ARQ fails   -> permanently lost
```

P_arq is derived from the reliability target ρ, not computed independently:

```
    P(lost) = e x (1 - P_fec) x (1 - P_arq) = 1 - ρ

    Solving: P_arq = 1 - (1-ρ) / (e x (1-P_fec))
```

For ρ = 100%: P_arq = 1. We keep retransmitting until ACK (T_cut = infinity),
so ARQ eventually succeeds. FEC is still active — it handles most recoveries
fast. ARQ is the backstop for what FEC misses.

For ρ < 100%: P_arq < 1. We stop retransmitting at T_cut. Symbols not
recovered by then are permanently lost.

The full delivery distribution:

```
    P(on-time delivery) = (1 - e) + e x P_fec           not lost, or FEC
    P(late delivery)    = e x (1 - P_fec) x P_arq       ARQ retransmit
    P(permanent loss)   = e x (1 - P_fec) x (1 - P_arq) not recovered

    Reliability: ρ = 1 - P(permanent loss)
    Tail latency: δ = P(late delivery) / ρ               among delivered symbols
```

### 6.4 The Triangle in Action

Under **100% reliability** (ρ = 1, T_cut = infinity): outcome 3 never occurs.
"Tail loss from FEC" equals "tail latency events" — they are the same thing.
This is the special case from Section 8.

Under **variable reliability** (ρ < 1, T_cut < infinity): the taper is cut off.
Symbols beyond T_cut are permanently lost. This saves bandwidth (fewer
correction symbols) and bounds latency (no recovery beyond T_cut), at the
cost of reliability.

```
  p = 100%:  ---------------------------------- (taper runs until ACK)
             all symbols eventually delivered

  p = 98%:   ----------------+
             98% delivered   | T_cut
             2% lost         +-- (taper stops, accept loss)

  p = 95%:   ----------+
             95%       | T_cut (shorter)
             5% lost   +-- accept loss (sensor/VoIP)
```

### 6.5 Four-Mechanism Composition

The complete system is four independent mechanisms that compose without
interfering:

    Mechanism       Controls              Derives from
    -------------------------------------------------------
    Copa            Total wire rate        RTT, bandwidth (delay-based)
    Taper           Source/correction      e, q, delta (GE + triangle)
                    ratio
    T_cut + buffer  Reliability guarantee  rho (triangle), GE B_max
    P_lost          Repair/retransmit      ACK timing, e
                    mix

Each mechanism has one job and one set of inputs. None needs to know about the
others:

- Copa doesn't know about FEC — it just limits total rate
- The taper doesn't know about Copa — it just sets the ratio
- T_cut doesn't know about the taper — it just evicts old symbols
- P_lost doesn't know about T_cut — it just picks repair vs retransmit

The protocol hint flows through the triangle to set delta and rho, which
determine the taper amplitude and T_cut. Copa independently discovers the
pipe capacity. P_lost independently tracks per-symbol loss confidence.
The four mechanisms compose to produce the correct behavior for each
protocol class without branching or mode switches.

---

## 7. Estimation — From Observations to Channel Parameters

### 7.1 What We Observe

At the sender, we receive periodic feedback:

| Observation | Source | Frequency |
|-------------|--------|-----------|
| (sent, received) per batch | ACK messages | Every batch (~10-100ms) |
| RTT | Echoed timestamps in ACK | Every batch |
| SACK ranges (out-of-order blocks) | ACK+SACK messages | Every ACK |
| Throughput | Delivery rate tracking | Continuous |

The sender infers losses from gaps: symbols not covered by the cumulative ACK
or any SACK range are presumed lost after T_retx has elapsed.

### 7.2 EWMA — Fast Point Estimate

Exponentially Weighted Moving Average of the loss rate:

```
   e_hat_ewma(n) = α x (lost/sent) + (1-α) x e_hat_ewma(n-1)

   α = 0.1 → time constant of 10 samples (half-life ≈ 7 samples)
```

**Strengths:** Simple, fast, responsive to changes.

**Limitations:** Single number — cannot express "confident at 2%" vs "uncertain,
somewhere between 0% and 10%". This inability to express uncertainty is why the
old system needed three stacked mechanisms (EWMA + Beta margin + PI controller)
to avoid under-provisioning.

### 7.3 Beta-Binomial Posterior — Uncertainty Quantification

The Beta distribution is the conjugate prior for Binomial observations:

```
   Prior:     Beta(a, b)        (a = received, b = lost)
   Update:    a' = a x decay + received
              b' = b x decay + lost
   Posterior: Beta(a', b')

   Mean loss rate:  b' / (a' + b')
   Variance:        a'b' / ((a'+b')² (a'+b'+1))
   Upper quantile:  beta_quantile(b', a', confidence)
```

The decay factor (0.995) causes old observations to fade, allowing
adaptation. This gives a half-life of approximately 138 observations
(ln(0.5)/ln(0.995) ≈ 138). The value is not sensitive — decay factors
between 0.99 and 0.999 produce similar steady-state behavior. The choice
trades off adaptation speed (lower decay = faster forgetting) against
estimation stability (higher decay = smoother estimates).

**Strengths:** Principled uncertainty — the spread of the posterior tells us how
confident we are. Tight posterior → low uncertainty → small safety margin needed.

**Limitations:** Cannot detect regime changes. If loss jumps from 1% to 10%, the
posterior slowly drifts — it doesn't know the old data is from a different regime.

### 7.4 BOCD — Regime-Aware Prediction

Bayesian Online Changepoint Detection [Adams2007] maintains a
distribution over "how long since the last regime change?" (the run length).

```
  Observation stream:   ...oooooooooxxxxxooooooooooooooxxxooo...
                        <- regime 1  ->   <- regime 2 -> <r3>
                                     ^                ^
                                changepoint      changepoint

  Run-length distribution P(r_t | data):

  Regime 1 (steady):   Changepoint:          Regime 2 (steady):
  Mass at r=50         Mass splits:          Mass at r=20
  (confident)          r=0  (new regime)     (confident again)
                       r=51 (old continues)
   |                    |      |               |
  _|_                  _|_    _|_             _|_
  | |                  | |    | |             | |
  ------- r            ----------- r          ------- r
    50                 0      51                20
```

For each run length, BOCD maintains Beta-Binomial sufficient statistics.
The predictive quantile integrates over all possible run lengths:

```
   P̂_upper(confidence) = Σ_r P(r_t = r | data) x beta_quantile(stats_r, confidence)
```

**Key properties:**
- Steady state: mass concentrates at one run length → tight posterior → small margin
- Changepoint: mass spreads → wide posterior → large margin (conservative)
- The predictive quantile IS the safety margin — no separate PI or margin needed

### 7.5 GE Parameter Estimation

The GE estimator tracks transition counts with exponential decay:

```
   On each symbol observation (Good or Bad):
     decay all counters by factor 0.999
     increment the appropriate transition counter:
       Good→Good: g_to_g     Good→Bad: g_to_b
       Bad→Good:  b_to_g     Bad→Bad:  b_to_b

   Estimated parameters:
     p̂ = g_to_b / (g_to_g + g_to_b)         P(Good → Bad)
     q̂ = b_to_g / (b_to_g + b_to_b)         P(Bad → Good)
     B̂ = 1/q̂                               mean burst length
```

**Initialization:** All transition counters start at zero, initial state
assumed Good. The GE estimates are gated by a minimum sample count (the
is_valid() check) — until enough transitions are observed (~50 symbols),
the estimator returns default values (B=2.0, q=0.5). During this bootstrap
period, the Beta prior (Section 7.3) and BOCD (Section 7.4) provide loss
estimation without relying on GE burst parameters.

These estimates feed directly into the taper function shape: τ(t) = A × (1-q̂)^t.

### 7.6 ACK Loss and Self-Healing

The feedback channel (receiver → sender) may also be lossy. If ACKs get lost,
the sender lacks up-to-date knowledge of what the receiver has.

**Self-healing property:** Unlike a NACK-based system where a lost NACK means
the sender never learns about the loss, an ACK-based system is inherently
self-healing:

- If an ACK is lost, the sender simply does not advance its knowledge of
  receiver state. Symbols remain in the retransmit buffer.
- After T_retx elapses without acknowledgment, the sender retransmits — this
  is correct behavior whether the original was lost or just the ACK was lost.
- If the original arrived but the ACK was lost, the receiver gets a duplicate
  (harmless — deduplicated by sequence number) and sends another ACK.
- Eventually an ACK gets through, and the sender's state catches up.

```
  ACK lost scenario:

  Sender:    [S1] [S2] ... wait T_retx ... [S1'] ... receives ACK ... done
                                              ↑
                                    retransmit (safe: either original
                                    or ACK was lost, both handled)

  No separate mechanism needed — the retransmit timeout handles both cases.
```

This eliminates the need for RX path loss estimation, NackAck echo, and NACK
effectiveness tracking. The system is simpler and more robust.

### 7.7 Estimation Error and Overhead

Estimation error directly maps to overhead:

```
   If e_hat > e_true:  over-provisioning → wasted bandwidth
   If e_hat < e_true:  under-provisioning → more ARQ latency events

   Overhead gap = (e_hat - e_true) / e_true
```

BOCD minimizes this gap by adapting the estimation confidence to the regime:
- Steady state: ε̂ ≈ ε_true (tight posterior)
- Transition: ε̂ > ε_true (conservative, correct behavior)

---

## 8. The Optimization Problem

### 8.1 Formal Statement

```
   minimize:    r = A/q                      (correction rate = bandwidth cost)

   subject to:  e x (1 - P_fec(A, q)) ≤ δ    (tail latency constraint)

   where:       τ(t) = A x (1-q)^t           (taper function)
                P_fec depends on A, q, e, W  (FEC recovery probability)
```

**Input:** δ (tail latency target, from protocol hint)

**Output:** A* (optimal taper amplitude), r* = A*/q (optimal correction rate)

**Note on the constraint:** Section 8.2 actually derives r* from the
stricter per-window surrogate P(repairs < K) ≤ δ rather than
e × (1 - P_fec) ≤ δ — see the "Which δ is this?" note there.

### 8.2 Corrected Model (canonical)

Let's reconsider. The taper generates correction symbols at known positions. For a
window of size W, the taper generates exactly r × W correction symbols total. The
question is: given that a source symbol is lost, how many correction symbols
covering it will arrive at the receiver?

In a sliding window code, repair symbols are linear combinations of all source
symbols in the window. A repair at offset t from source s covers s as long as
s is still in the window (t < W).

Number of correction symbols generated while s is in the window:
```
   N_correction = Σ_{t=0}^{W-1} τ(t)
```

Of these, each survives independently with probability (1-ε). Expected arrivals:
```
   R = N_correction x (1-e) = (A/q) x (1-(1-q)^W) x (1-e)
```

For the decoder to recover s, we need at least 1 linearly independent repair
(with systematic code, if all other source symbols arrived, one repair suffices).

But the actual requirement is more nuanced: we need as many repair symbols as
there are lost source symbols in the window. If k symbols are lost in the
window, we need k repair arrivals.

**Average case:** In a window of size W with loss rate ε, expected losses = εW.
Expected repair arrivals = r × W × (1-ε). For recovery: r × W × (1-ε) ≥ εW,
giving r ≥ ε/(1-ε) = r_IT. This confirms the IT minimum.

**Tail case:** We need recovery even when more symbols are lost than average.
The number of losses in a window follows a distribution determined by the GE
model. The tail latency constraint requires recovery at the δ-th percentile.

Let K be the number of lost symbols in a window. For the GE model:
```
   P(K = k) depends on the burst structure
   For large W: K ≈ e*W with variance determined by burstiness
```

For recovery: number of arriving repairs ≥ K.

Repairs arrive = Binomial(r × W, 1-ε), approximately Normal(rW(1-ε), rW(1-ε)ε).

Constraint: P(repairs < K) ≤ δ.

**Which δ is this?** Section 6.3 defines δ as P(late delivery) =
e × (1 - P_fec). Since a symbol can only be late if it was lost first, the
constraint on the window is P(repairs < K) ≤ δ/e — the canonical form used
in Section 8.4, giving z_{δ/e} in the margin. Two consequences:

1. **Continuity.** As the channel improves relative to the target
   (e → δ), z_{δ/e} falls through zero and the required rate decreases
   continuously to r* = 0 — pure ARQ already meets the tail target. No
   cutoff rule is needed; the δ = e boundary behavior of Section 11.3
   emerges from the closed form.
2. **A conservative variant exists.** Imposing P(repairs < K) ≤ δ
   directly (z_δ instead of z_{δ/e}) is stricter — it partially
   compensates for the size-bias of conditioning on "this symbol was
   lost" (a window known to contain a loss has more losses than average).
   At WiFi (e = 0.025, δ = 1e-4): z_{δ/e} = 2.65 vs z_δ = 3.72.
   Deployments wanting extra safety can use z_δ; for exactness, use the
   transfer-matrix computation (Appendix D, item 4) instead of either
   approximation.

Using normal approximation:
```
   P(repairs < K) ≈ P(Normal(rW(1-e), rW(1-e)e) < K)
```

This requires the δ-quantile of repairs to exceed the (1-δ)-quantile of losses.
The algebra gives:

```
   r ≥ e/(1-e) + z_δ x √(e/(W(1-e)))
```

where z_δ is the standard normal quantile [Abramowitz1964] for probability δ.

**The explicit P_fec formula.** Given correction rate r, loss rate e,
burst variance s2_burst, and window size W:

    Losses in window:      K ~ Normal(We, We(1-e)s2_burst)
    Surviving corrections: C ~ Normal(rW(1-e), rWe(1-e))

    P_fec = P(C >= K) = Phi(z)

    where z = sqrt(W) x (r(1-e) - e) / sqrt(e(1-e)(r + s2_burst))

At the IT minimum (r = e/(1-e)): z = 0, P_fec = 0.5 (coin flip — half the
time corrections suffice, half the time they don't). The margin term in
Section 8.4's r* pushes z positive, giving P_fec > 0.5.

**Note:** The r* formula from Section 8.4 is a first-order approximation
of inverting this P_fec formula. It drops r from the variance denominator
(assumes r << s2_burst). For practical r values (0.05-0.25) this is
accurate. For exact computation, invert the P_fec formula numerically.

This normal approximation is valid when W >> B (window much larger than mean
burst) — which covers all practical scenarios (W=50, B=2-3). For edge cases
where W ≈ B, the exact GE distribution is computable via transfer matrix
dynamic programming in O(W^2). See Appendix D item 4.

### 8.3 Burst Variance Correction

The normal approximation in 8.2 assumes iid losses (Binomial variance). On a
GE channel, losses are correlated — bursts inflate the variance.

The GE autocorrelation decays with eigenvalue (1-p-q). The variance of losses
in a window of size W is:

```
   Var_iid(K) = W x e x (1-e)                    (independent losses)

   Var_GE(K)  = W x e x (1-e) x σ²_burst         (burst-correlated losses)

   σ²_burst = 1 + 2(1-p-q)/(p+q)                  (variance inflation factor)
```

| Scenario  | p+q    | σ²_burst | Meaning                                  |
|-----------|--------|----------|------------------------------------------|
| DC        | 0.5005 | 3.0      | 3× wider variance than iid assumption    |
| WiFi      | 0.513 | 2.9      | similar                                  |
| LTE       | 0.42  | 3.8      | significant inflation                    |
| Satellite | 0.33  | 5.1      | iid approximation seriously wrong        |

We compute σ²_burst directly from the GE estimator's p̂ and q̂. Degenerate
estimates need care: when the estimator has observed no Bad-state
transitions (very clean channels — the decayed counters empty out), q̂ = 0
is a NO-DATA sentinel, not a measurement of infinite bursts. Treating it
as a measurement makes σ²_burst = 1 + 2(1-p̂)/p̂ ≈ 2/p̂ explode (≈4000 at
ε = 0.1%) and massively over-provisions exactly the cleanest links.
No-data must map to the iid default σ² = 1.

**Known limitation — repair survival is treated as i.i.d.** The model
inflates the variance of the loss count K by σ²_burst but keeps the repair
count C ~ Binomial(rW, 1-ε) with independent survival. Repairs cross the
same GE channel: a burst that inflates K simultaneously kills interleaved
repairs, so Var(C) is also burst-inflated and Cov(K, C) < 0. Both effects
widen Var(K - C) beyond what the formula assumes, making P_fec optimistic
in exactly the bursty regime σ²_burst is meant to protect. Monte Carlo
validation shows the normal approximation diverging by up to ~12% on
high-loss/long-burst channels (LTE-like). For implementation-grade
precision, use the exact O(W²) transfer-matrix computation (Section 8.7),
which captures loss/repair correlation exactly.

### 8.4 The Corrected Optimal Correction Rate

```
  r* = max(0,  e/(1-e) + z_{delta/e} x sqrt(e x s2_burst / (W x (1-e))) )
              '--v--'   '--------------------v----------------------'
            IT minimum                  tail margin
                              (accounts for burst correlation)

  s2_burst = 1 + 2(1-p-q)/(p+q)

  z_{delta/e} = standard normal quantile at (1 - delta/e)
```

**Continuity — no cutoffs, the boundary emerges from the closed form.**
The quantile is taken at 1 - δ/e: the margin depends on how tight the
target is RELATIVE to the channel. When δ << e (target much tighter than
the loss rate), z is large and the margin dominates. As the channel
improves toward the target (e → δ), z falls through zero, r* drops below
the IT minimum, and reaches 0 continuously — at which point pure ARQ
meets the tail target with no FEC at all. The max(0, ·) is the physical
floor (repair counts cannot be negative), not a policy cutoff; there is
no mode switch anywhere on the path from "heavy FEC" to "pure ARQ".

**Properties:**
- r*(δ, e) is continuous — it decreases to 0 as δ → e (Section 11.3
  boundary) with no branch or threshold; all regime behavior comes from
  one closed-form expression
- IT minimum ε/(1-ε) is the dominant term when the margin is active
- Tail margin scales as 1/√W — larger windows need proportionally less margin
- z_{δ/ε} controls the margin: tighter δ RELATIVE to ε → larger z → more margin
- σ²_burst amplifies the margin for bursty channels (large for small p+q)
- On strongly bursty channels this closed form UNDER-provisions the tail
  (Gaussian tail + ignored loss/repair correlation) — see Section 8.7 for
  the exact GE computation and the size of the gap, and Section 8.4.1 for
  the measured-tail correction that production applies on top (real
  traces are 2-4× worse than even the GE-exact prediction; Section 2.5)

**Note:** This formula uses the raw loss rate e. For the canonical production
formula including codec overhead, replace e with e_hat = e + e_codec x
(1-(1-e)^W) from Section 9.2. The codec-adjusted version accounts for decoder
invocation probability on systematic codes.

**Measured caveat (2026-07-08):** r* assumes each proactive repair symbol has a
`(1-e)` chance of *arriving and being useful*. On the current droppable-datagram
substrate that assumption fails — proactive repair is dropped/wasted such that the
observed proactive-recovery fraction stays ≈0.3–0.6 at high RTT+loss regardless of
r (r-sweep) or of receiver NACK timing (§12.9 repair-wait). Raising r* to compensate
does not help because the added repair is dropped at the same rate. See §12.9.
*(Update 2026-07-18, task #85: one instance of this class — the plain-mode taper
reset that emitted r per ack cycle instead of r per source — is FIXED as a
mechanism (`RWM_TAPER_R` budget law, Section 8.4.1 "L1 status"); the measured L0
2×2 shows the remaining binders are the spare-capacity gate and the
leading-window coding span, not the emission quantity.)*

### 8.4.1 Burst-Tail Provisioning: r* Against the Measured Window-Mass Quantile

> **Status: DERIVED + MEASURED-through-oracle + SHIPPED (task #46).** The
> Section 8.4 closed form (and even the Section 8.7 exact GE computation)
> provisions against *GE-geometric* bursts. Real traces carry heavier burst
> tails AND burst clustering (long memory), and the delivered window-failure
> misses the δ/ε target by 2–4× beyond the GE-ideal (Section 2.5, task #43).
> This subsection derives the corrected provisioning and is what production
> ships (env gate `RWM_RSTAR_TAIL`, default ON; `=0` restores legacy GE-only
> provisioning for A/B).

**The exact failure statistic.** A window of W source symbols with R = rW
repairs spans N = W + R wire slots. Let K be the number of lost slots and x
of them repairs. The window fails iff source losses exceed surviving repairs:

```
   K − x  >  R − x      ⟺      K > R          (independent of x!)
```

A loss that hits a repair removes one loss AND one repair — the deficit is
unchanged. So the per-window failure probability is EXACTLY the upper tail
of the window loss mass K_N, and the right quantity to provision against is
the receiver's own measured window-mass distribution. This also explains why
a *single-burst-length* quantile is not enough: a window is killed just as
dead by two clustered 20-loss bursts as by one 40-loss burst, and real loss
clusters far beyond GE (the Section 2.5 long-memory miss — lag-20
autocorrelation 5×–4100× the GE prediction). The mass statistic contains the
burst-length tail, the clustering, and the loss/repair correlation at once.

**Measurement (all online, no new contract parameters).** The receiver bins
its per-symbol loss observations into blocks of w0 = 64 wire slots and, with
the same decayed-counter pattern as the GE transition counts, tracks for
each span length m = 1..8 blocks (sliding at block granularity):

```
   p_nz(m)  = P(J_m ≥ 1)            fraction of m-spans with any loss
   m1(m)    = E[J_m | J_m ≥ 1]      conditional mean mass
   m2(m)    = E[J_m² | J_m ≥ 1]     conditional second moment
```

where J_m = losses in m consecutive blocks. Each conditional tail is
extended parametrically with a **discrete Weibull**: S(t) = P(J > t) =
θ^(t^k). k = 1 is *exactly* the geometric law (θ = 1−q) — GE is the special
case, not a competitor — and k < 1 is the stretched-exponential heavy tail
real fades show. (θ, k) come from midpoint-corrected moment matching (X =
J − ½):

```
   E[X²]/E[X]² = Γ(1+2/k) / Γ(1+1/k)²        (strictly decreasing in k
                                              → binary search)
   c = (Γ(1+1/k) / E[X])^k,   θ = e^(−c)
```

On geometric moments this fit returns k ≈ 1, θ ≈ 1−q (unit-tested): a GE
channel measures itself back.

**The corrected rate.** P(window fails) = P(K_N > R) is read from the
measured tails at the window's own scale x = N/w0 ∈ [1, 8], interpolating
linearly between the two bracketing spans (the conservative side of the
log-linear reading); beyond the largest tracked span the window is chunked
with a union bound:

```
   F(r) = (1−f)·T_lo(R) + f·T_hi(R),     T_m(R) = p_nz(m)·S_m(R)
   r*_mass = min { r ∈ [0, 2] : F(r) ≤ δ_wf },   δ_wf = δ/ε
```

and production emits

```
   r* = max( r*_{8.4} ,  r*_mass )
```

(`r_star_mass` / `MassStats` in raptorpath-math; composition in
`controller_rate`).

**Level rescaling (regime adaptation).** The mass moments deliberately
carry a LONG memory (rare tails need many samples: one decay step per
block sample, not per symbol), so after a loss-regime change they would
lag the fast BOCD level estimate by ~64× — a reactivity regression the
control-loop tests catch. The resolution is level equivariance: a regime
LEVEL shift rescales the whole mass distribution while the tail SHAPE
keeps its long memory. The solver reads the tail at the current level,

```
   P(K > R) = T( R / s ),    s = ε̂_now / ε_mass,
   ε_mass = p_nz(1)·m1(1) / w0     (the level the mass stats embody)
```

with ε̂_now the BOCD posterior upper quantile — so the term follows
regime changes at estimator speed and inherits the architecture's
estimation-uncertainty margin (ε̂_now is the conservative upper, making
s ≳ 1 on stationary channels). s → 0 (channel now clean) sends the term
to 0 continuously; the oracle-side full-trace fits have s ≈ 1 by
construction. Properties, all preserved from Section 8.4: continuous
in δ and in the measured moments; r\*_mass = 0 when the measured tail
already meets the target at r = 0 (pure ARQ) and identically when δ_wf ≥ 1 —
so the Bulk χ = 0 identity r\*(δ = ε̂) = 0 survives; inert (exactly the old
controller) until the receiver has observed 30 nonzero-mass blocks — cold
start is unchanged. When even the r = 2 ceiling cannot meet δ_wf (a fade
deeper than any in-window budget), the solver returns the ceiling: the
contract is **declared infeasible in-window** and the max_overhead clamp
governs — the miss is explicit, not silent.

**Worked example (old vs new on a heavy-tail channel).** Semi-Markov
channel, geometric Good sojourns, discrete-Weibull(k = 0.5, θ = 0.55) Bad
sojourns, ε = 12.5%, max burst 310 — the controlled version of the real
traces. A GE fit sees σ²_burst = 9.8. At W = 50, δ_wf = 0.02:

```
                     r*        delivered WF/target   (replay, 27k windows)
   old (§8.4)       0.486            5.1×    MISS
   new (§8.4.1)     1.268            0.99×   HIT
```

The same comparison on a *GE-generated* WiFi/LTE/Sat draw shows why this is
not blanket over-provisioning — the corrected r\* lands ON the Section 8.7
exact GE optimum (which the closed form itself under-shoots):

```
   GE draw (W=50, δ_wf=0.02)   r*_old   r*_new   r*_exact(§8.7)   WF_old  WF_new
   WiFi  (ε=2.5%)               0.105    0.137      0.130          2.4×    0.90×
   LTE   (ε=4.8%)               0.177    0.239      0.230          2.2×    0.80×
   Sat   (ε=9.1%)               0.306    0.434      0.390          2.7×    0.60×
```

**Validation on the real traces (task #43 machinery, all five traces).**
Worst delivered residual over cells where the contract is feasible in-window
improves from **2.88× to 1.41×** the target; the residual above 1× is
non-stationarity (the moments are one number for a drifting trace). Six of
ten trace/target cells are declared infeasible at W = 50 (e.g.
TMobile-UMTS-driving at ε = 24.5%: even r = 2 leaves ~21% of windows inside
fades no in-window rate covers) — on those the ceiling still improves the
residual (12.8× → 10.7× at worst) and the infeasibility is reported.
Full tables: `rstar_tail_validation.rs`.

**L1 status (MEASURED 2026-07-13; emission fix built + L0-measured
2026-07-18, task #85).** The corrected r\* is realized where the computed
rate is consumed directly (the L0 gate suite; the oracle replay). At L1 in
*plain window mode* it was INERT at the wire: the taper emission path
resets its offset on cumulative-ack advancement, so emitted proactive
repair ≈ Σ τ(t) = r symbols *per ack cycle* (~r/cycle-length overhead,
nearly independent of r's magnitude), and an x8 two-seed A/B on
c3-realtime showed the arms tied at equal emitted overhead. That QUANTITY
defect is now FIXED: the budget-conserving taper (`TaperBudget`, env
`RWM_TAPER_R`, default OFF) banks the computed rate per source symbol and
re-times the spend with the same taper shape renormalized over the coding
window, so emitted repair tracks r × source — unit-proven, and measured
live at the wire on the heavy-tail L0 shim (cod/src 0.03–0.05 → 0.21–0.34
on a 2×2 A/B). The full claim is still NOT realized there, honestly: the
2×2 shows two further binders — the spare-capacity gate compresses the
legacy/corrected arms (controller 0.248 vs 0.445 → wire Δ ≈ +0.03), and
the emitted repair codes over the LEADING sliding window (in-flight
entanglement), making it recovery-inert within realtime's reorder horizon
(delivered reliability *degrades* −22 pp with the fix on; a trailing-span
differential at the same rate recovers over half the gap). The flag stays
default-off; the solvable-span emission follow-up and the queued L1
re-run are specified in goal-gate "Taper Emission Fix".
*L1 spot check (2026-07-19, c3-realtime 2×2 ×8, seeds 42+7,
`meas/percap-battery`): the budget law is live on the real netem substrate
(cod/src 0.06–0.09 → 0.32–0.35) and consuming r DEGRADES delivered
reliability there too (−25/−19 pp, both seeds, ≈2.5–3× the per-rep
spread) while the r\* arms stay wire-tied (Δcod/src ≤ 0.03) — all three
L0 findings reproduced at L1; the leading-window entanglement attribution
stands with the substrate's signature, and the flip remains closed
pending the solvable-span follow-up.*

**Cost and scoping (honest).** On heavy-tail channels the corrected r\* is
large *because the contract is expensive there* — reliability against fades
is bought with bandwidth. The (δ, ρ, r) contract itself scopes the cost: Bulk
(δ_eff = ε̂) pays nothing (identity above); loose targets on GE-like channels
pay ≤ a few points of r (the term tracks the exact GE requirement); only
tight-δ profiles on measured-heavy channels pay materially — which is exactly
the regime the contract demands be paid for. The saturation cap
(Section 14.21) still overrides where more FEC measurably hurts the tail.
Remaining limits: non-stationarity (~1.4× residual), the block-aligned
measurement (alignment slack absorbed by the conservative interpolation), and
the decayed-moment memory horizon.

### 8.5 Worked Examples

Using z_{δ/ε} = Φ⁻¹(1 - δ/ε) — the margin depends on how tight the target
is relative to the channel loss rate.

The margin term is: `z_{δ/ε} × √(ε × σ²_burst / (W × (1-ε)))`

**DC (ε=0.001, W=50, σ²_burst=3.0):**
```
   Bulk (δ=1e-2):     δ ≥ ε → r* = 0   (pure ARQ already meets the target)
   Auto (δ=1e-4):     r* = 0.1% + 1.28x0.78% = 0.1% + 1.0% = 1.1%
   Realtime (δ=1e-6): r* = 0.1% + 3.09x0.78% = 0.1% + 2.4% = 2.5%
```

**WiFi (ε=0.025, W=50, σ²_burst=2.9):**
```
   Bulk (δ=1e-2):     r* = 2.6% + 0.25x3.86% = 2.6% +  1.0% =  3.5%
   Auto (δ=1e-4):     r* = 2.6% + 2.65x3.86% = 2.6% + 10.2% = 12.8%
   Realtime (δ=1e-6): r* = 2.6% + 3.94x3.86% = 2.6% + 15.2% = 17.8%
```

**Satellite (ε=0.09, W=50, σ²_burst=5.1):**
```
   Bulk (δ=1e-2):     r* = 9.9% + 1.22x10.0% =  9.9% + 12.3% = 22.2%
   Auto (δ=1e-4):     r* = 9.9% + 3.06x10.0% =  9.9% + 30.7% = 40.6%
   Realtime (δ=1e-6): r* = 9.9% + 4.24x10.0% =  9.9% + 42.6% = 52.5%
```

**Note the emergent behavior across rows.** At DC, a Bulk target is looser
than the channel loss itself, so FEC vanishes entirely — ARQ suffices. On
satellite the SAME δ = 1e-2 sits 9× below ε, so even Bulk needs a 12%
margin. The margin responds to the ratio δ/ε, not to δ alone — one
protocol hint adapts continuously across channels. The σ²_burst
factor still amplifies the margin on bursty channels (satellite's unit
margin is 10% vs WiFi's 3.9% per unit z).

### 8.6 Three-Variable Optimization

Section 8.4 solves for r* given δ (with ρ=100%). Here we generalize to all
three modes of the bandwidth/latency/reliability triangle.

#### Taper with cutoff

When ρ < 100%, the taper is truncated at T_cut:

```
   τ(t) = A x (1-q)^t    for t ≤ T_cut
   τ(t) = 0              for t > T_cut
```

Total correction rate with cutoff:

```
   r = A x Σ_{t=0}^{T_cut} (1-q)^t = A x (1 - (1-q)^{T_cut+1}) / q
```

For T_cut = ∞ (ρ = 100%): reduces to r = A/q (Section 4.3).

#### Mode 1: Given (δ, ρ) → compute r

Fix tail latency target δ and reliability ρ. Compute minimum bandwidth r*.

Step 1: From ρ, find T_cut. The reliability ρ = P(recovered within T_cut).
Using the corrected model (Section 8.4):

```
   T_cut such that: e x (1 - P_fec(T_cut)) x (1 - P_arq(T_cut)) = 1 - ρ
```

For ρ = 100%: T_cut = ∞ (no finite solution — see Section 6.4).
For ρ < 100%: solve via binary search. P(recovered by T_cut) is monotone
increasing in T_cut (more time = more corrections = higher recovery):

```
  Algorithm: find T_cut from ρ

  P_fec = Phi(sqrt(W) x (r(1-e)-e) / sqrt(e(1-e)(r+s2_burst)))   (Section 8.2, time-independent)

  lo = 0
  hi = W x 10                        (upper bound: many window lengths)
  while hi - lo > tolerance:
      mid = (lo + hi) / 2
      corrections_by_mid = r x (1 - (1-q)^(mid+1))       (taper integral up to mid)
      P_arq(mid) = min(corrections_by_mid / (e/(1-e)), 1) (fraction of needed corrections available)
      P_recovered(mid) = 1 - e x (1-P_fec) x (1-P_arq(mid))
      if P_recovered(mid) < ρ:
          lo = mid                   (need more time)
      else:
          hi = mid                   (enough time)
  T_cut = hi
```

Convergence is guaranteed because P(recovered) is monotone in T_cut.
Typically converges in ~20 iterations (log2 of search range).

Step 2: From δ, find A using the tail latency constraint (Section 8.4):

```
   A* such that: e x (1 - P_fec(A*)) ≤ δ    (among delivered symbols)
```

Step 3: Compute r* = A* × (1 - (1-q)^{T_cut+1}) / q.

**Special case ρ = 100%**: T_cut = ∞, r* = A*/q = ε/(1-ε) + z_δ√(...).
This is the formula from Section 8.4.

#### Mode 2: Given (r, ρ) → compute δ

Fix bandwidth r and reliability ρ. Compute resulting tail latency δ.

```
   From ρ: find T_cut (same as Mode 1, Step 1)
   From r and T_cut: A = r x q / (1 - (1-q)^{T_cut+1})
   From r and W: P_fec = Phi(sqrt(W) x (r(1-e)-e) / sqrt(e(1-e)(r+s2_burst)))  (Section 8.2)
   Result: δ = e x (1 - P_fec) x P_arq / ρ
```

#### Mode 3: Given (r, δ) → compute ρ

Fix bandwidth r and tail latency δ. Compute resulting reliability ρ.

```
   From r: A = r x q / (1 - (1-q)^{T_cut+1})     (depends on T_cut)
   From δ: determine how much of the taper is "on-time" vs "late"
   From A and the taper integral: ρ = total recovery probability within T_cut
```

With r fixed, the implied tail latency is monotone in ρ: writing
F = e(1-P_fec), Mode 2 gives δ(ρ) = F x P_arq(ρ) / ρ = 1 - (1-F)/ρ, which
increases with ρ (demanding higher reliability forces more of the FEC
misses to be recovered late by ARQ instead of dropped). So binary search
on ρ finds the largest reliability whose implied lateness stays within
the δ budget:

```
  Algorithm: find ρ given (r, δ)

  P_fec = Phi(sqrt(W) x (r(1-e)-e) / sqrt(e(1-e)(r+s2_burst)))   (Section 8.2)

  lo = 0.5
  hi = 1 - 1e-12
  while hi - lo > tolerance:
      mid = (lo + hi) / 2                        (candidate ρ)
      P_arq(mid) = clamp(1 - (1-mid)/(e x (1-P_fec)), 0, 1)   (Section 6.3)
      δ(mid) = e x (1-P_fec) x P_arq(mid) / mid   (Mode 2 with ρ = mid)
      if δ(mid) > δ:
          hi = mid        (candidate ρ implies more lateness than allowed)
      else:
          lo = mid        (budget allows higher ρ)
  ρ = lo
  T_cut = find T_cut from ρ                       (Mode 1, Step 1)
```

Convergence guaranteed by monotonicity. The resulting ρ is the maximum
reliability achievable within the bandwidth budget r at tail latency δ.

#### Worked examples

**Example 1: Bulk file transfer (WiFi, ε=0.025)**

```
   Fix: ρ = 100%, minimize r
   Compute: δ (tail latency)

   r* = 2.6% + z_{δ/ε} x 3.9%     (from Section 8.5, WiFi row)

   At minimum r = r_IT = 2.6%:  P_fec = 0.5 (Section 8.2: z = 0), so
   δ = e x (1-P_fec) = 1.25% — half the lost symbols go to ARQ
   Tail latency ≈ T_retx + RTT/2 for those symbols
   For RTT = 50ms: L_arq ≈ 100ms for 1.25% of symbols

   To get δ = 1e-2: r* = 3.5% (Section 8.5, WiFi Bulk row)
```

**Example 2: VoIP (WiFi, ε=0.025)**

```
   Fix: δ = 150ms budget → T_cut = 150ms / symbol_time
        r = 5% (codec + small overhead)
   Compute: ρ (reliability)

   With r = 5%, the correction budget gives (Section 8.2, W=50, σ²=2.9):
   - P_fec ≈ 0.73 (73% of lost symbols FEC-recovered, no added latency)
   - Remaining 0.68% of all symbols go to ARQ; one retransmit round
     (L_arq ≈ 100ms) fits the 150ms budget, succeeding w.p. ≈ 1-e
   - P(miss deadline) ≈ e x (1-P_fec) x e ≈ 0.02%  ->  ρ(150ms) ≈ 99.98%

   The 150ms budget is generous at RTT = 50ms: reliability is limited by
   double losses (symbol AND its retransmit both lost), not by the
   correction budget. The VoIP codec conceals the residual frame loss.
```

**Example 3: Live video (WiFi, ε=0.025)**

```
   Fix: δ = 33ms (one frame at 30fps), ρ = 99.9%
   Compute: r (bandwidth)

   Need 99.9% of symbols delivered within 33ms.
   T_cut determined by ρ = 99.9%: T_cut ≈ 3 x RTT
   A determined by δ: need P(recovery within 33ms) ≥ 0.999
   33ms < L_arq, so recovery must be FEC — r* ≈ 13-18%
   (between the Auto and Realtime rows of Section 8.5)
```

**Example 4: Gaming (LTE, ε=0.05)**

```
   Fix: δ = 20ms (tight), ρ = 99% (1% loss acceptable)
   Compute: r (bandwidth)

   Very tight latency + moderate reliability → aggressive FEC
   T_cut ≈ 2 x RTT (short: accept 1% loss)
   r* ≈ 11% (δ/ε = 0.2 → z = 0.84 → margin ≈ 5.3% over the 5.3% IT
   minimum; the ρ = 99% cutoff trims the taper tail further)
```

**Example 5: Sensor telemetry (Satellite, ε=0.09)**

```
   Fix: r = 5% (minimal bandwidth), ρ = 95%
   Compute: δ (tail latency)

   Low bandwidth budget + high loss → many symbols go to ARQ
   T_cut determined by ρ = 95%
   δ ≈ 15% (15% of delivered symbols arrive late)
   Acceptable for periodic sensor readings.
```

### 8.7 Exact P_fec via Transfer-Matrix DP

Both gaps in the normal model — burst-inflated repair variance and the
negative K–C correlation (Section 8.3) — vanish if we compute the joint
distribution of losses and surviving repairs EXACTLY, by walking the GE
chain across the interleaved wire sequence.

**Setup.** A window of W source symbols with R = round(r × W) repairs
interleaved evenly: N = W + R wire slots with fixed types T_i ∈ {source,
repair} (slot i is a repair iff ⌊(i+1)R/N⌋ > ⌊iR/N⌋). The channel is the
two-state GE chain started from its stationary distribution; a symbol is
lost iff the chain is Bad at its slot. Because ONE chain walks across
both slot types, burst-correlated source losses, burst-correlated repair
erasures, and the negative correlation between them are all captured.

**Recursion.** Track the running deficit D = (#source losses) −
(#surviving repairs). FEC succeeds for the window iff D ≤ 0 at the end —
the same criterion as Section 8.2, applied to the exact joint
distribution. With f_i(x, d) = P(chain in state x, deficit d after slot i):

```
  f_0(G, 0) = π_G = q/(p+q),   f_0(B, 0) = π_B = p/(p+q)

  step to slot i+1 of type T:
    mass arriving in state x':  Σ_x f_i(x, d) × P(x → x')
    d' = d + 1   if T = source and x' = B     (source lost)
    d' = d − 1   if T = repair and x' = G     (repair survives)
    d' = d       otherwise

  P_fec = 1 − Σ_{d > 0} Σ_x f_N(x, d)
```

State space 2 × (W+R+1) over N slots → O(W²) work (≈ 6,000 operations at
W = 50, r = 0.1) — trivially cheap. Validation: on a memoryless channel
(p + q = 1) the DP reproduces the independent-Binomial reference to
machine precision, and against Monte Carlo it agrees to sampling error
(< 0.002 at 20k trials), where the normal approximation errs by ≈ 1–2%:

```
  Scenario (W=50)          r      exact    normal(8.2)  |error|
  WiFi  (p=.013, q=.5)    0.10   0.9522    0.9695       0.017
  LTE   (p=.02,  q=.4)    0.12   0.8868    0.8694       0.017
  Sat   (p=.03,  q=.3)    0.25   0.9180    0.9272       0.009
```

(R = round(r × W), half rounding away from zero — e.g. Sat: R = 13.)

**Exact r*.** P_fail(r) = 1 − P_fec(r) decreases in r (up to the 1/W
rounding of R), so binary search yields the exact minimum rate for
P_fail ≤ δ. The tail is where the normal approximation is weakest, and
the effect is material:

```
  δ = 1e-2, W = 50       r*_exact    r*_normal (8.4)
  WiFi                     0.170       0.116
  LTE                      0.270       0.193
  Satellite                0.450       0.334
```

The closed-form r* UNDER-provisions by ~30–50% of itself on bursty
channels: the K–C correlation widens Var(K − C) beyond the model, and
the true tail of K − C is heavier than Gaussian. The closed form remains
valuable for insight and cheap incremental updates; rate selection on
strongly bursty channels should use the exact computation (implemented
as `p_fec_exact` / `compute_r_star_exact` in raptorpath-math).

**Caveats.** Codec overhead is not modeled (apply ε_codec_eff from
Section 9.2 on top). The success criterion inherits Section 8.2's
per-window block view of the sliding-window code. R is rounded to an
integer, so r* is resolved in steps of 1/W.

### 8.8 Choosing the Window

Every rate formula so far takes W as a given. But W is itself a knob with
large, opposing effects, and picking it by hand ("W = 64") leaves free
overhead or free latency on the table. This section derives W* from the
same channel state the rate uses, closing the last free parameter.

**W pulls three ways.** All three appear elsewhere in this document; here
they are read as bounds on a single choice.

```
  1. Overhead  (Section 8.4).  The r* margin is
        margin(W) = z x sqrt(eps x sigma2 / (W (1-eps))),   z = z_{delta/eps}
     It decays as W^{-1/2}, with slope d(r*)/dW ~ -W^{-3/2}: strong at
     small W, flattening fast. Bigger W => less overhead. (favours large W)

  2. Latency   (Sections 14.5, 14.21, 14.25).  A window loss waits for a
     covering repair within the window horizon, which is traversed at the
     source rate: recovery latency ~ W x t_sym = W / send_rate. It is also
     the tail_svc dilution term of Section 14.21 and the coverage span of
     the completion glide (Section 14.26). Bigger W => slower recovery,
     and once in-flight span >> W the last window cannot cover the exposed
     tail (Section 14.25). (favours small W)

  3. Burst absorbency (Section 14.5).  Ambient FEC at r = eps/(1-eps) must
     accumulate B surviving repairs to cover a mean burst B = (sigma2+1)/2.
     (needs W at least a floor)
```

There is no scale-free "knee" in a bare W^{-1/2} law — diminishing returns
appear only RELATIVE to a fixed scale. The natural scale is the IT floor
eps/(1-eps): keep sizing the window up while the margin is a meaningful
fraction of the floor it sits on; stop once the margin has been pushed to
a fraction alpha of it. That, the latency ceiling, and the burst floor give
three closed-form bounds:

```
  W_over = z^2 x sigma2 x (1-eps) / (eps x alpha^2)      (margin <= alpha x floor)
  W_lat  = budget x send_rate                            (recovery <= budget)
  W_bur  = B / (eps x (1-eps)),   B = (sigma2+1)/2        (absorb a mean burst)

  W* = clamp( W_over,  min(W_bur, W_lat),  W_lat ),  then clamp to [16, 512]
```

with z = z_{delta/eps} = Phi^{-1}(1 - delta/eps) (the SAME quantile as r*,
Section 8.4), alpha = 0.25, and `budget` the Realtime hint's latency budget
or ~1 RTT otherwise — the Section 14.5 setting W x t_sym ~ RTT that aligns
the FEC and ARQ recovery horizons. The burst floor never overrides the
latency ceiling (`min(W_bur, W_lat)`): if a mean burst cannot be absorbed
within the budget, no window size fixes it and latency wins — the Realtime
reliability/latency trade made explicit.

**Continuity and the three regimes.** `clamp`/`min`/`max` are continuous
(piecewise-linear), so W* is continuous in every input and finite by the
[16, 512] clamp. Which term binds is itself the story:

- **Loose delta (Bulk, delta >= eps).** z <= 0, so W_over = 0: no margin to
  amortise. The window collapses to the burst floor min(W_bur, W_lat) — the
  SMALLEST window that still catches a mean burst, minimising recovery
  latency and decode cost. This is exactly the "no steady-state FEC" stance
  of Sections 14.25/14.26 read through W instead of r.
- **Tight delta (Auto/Realtime).** The margin dominates the small IT floor,
  W_over is large (often unreachable), and W* rides the latency ceiling
  W_lat: as large as the budget allows, to shrink the overhead as far as
  latency permits.
- **Moderate delta/eps.** W_over lands between the floor and the ceiling and
  the overhead knee itself binds (WiFi-Bulk below).

**Monotonicities** (verified as unit tests in raptorpath-math): tighter
delta raises z and W_over, so W* is non-decreasing as delta tightens (until
capped by W_lat); higher sigma2 raises both W_over and W_bur, so W*
increases with burst variance; a tighter latency budget lowers W_lat and
shrinks W*. Degenerate inputs are safe: no throughput/RTT sample disables
the latency ceiling (W_lat = 512, overhead knee governs), and eps -> 0
leaves the burst/overhead terms inert so latency alone sets W.

**Worked examples.** Per-hint delta and latency budget: Bulk (delta = 1e-2,
budget = 2 RTT), Auto (1e-4, 1 RTT), Realtime (1e-6, 0.5 RTT). `r*(W*)` is
the resulting overhead at the derived window; `r*(64)` is the overhead a
fixed W = 64 would pay at the same target; `recov` = W* / send_rate.

```
  DC          eps=0.001  sigma2=3.0  RTT=1 ms    send_rate=100k/s
    hint       delta     W*    r*(W*)   r*(64)   recov    binding
    Bulk       1e-2     200     0.0%     0.0%    2.0 ms   latency (pure ARQ)
    Auto       1e-4     100     0.8%     1.0%    1.0 ms   latency
    Realtime   1e-6      50     2.5%     2.2%    0.5 ms   latency

  WiFi        eps=0.025  sigma2=3.0  RTT=13 ms   send_rate=10k/s
    hint       delta     W*    r*(W*)   r*(64)   recov    binding
    Bulk       1e-2     120     3.2%     3.4%   12.0 ms   overhead knee
    Auto       1e-4     130     9.0%    11.8%   13.0 ms   latency
    Realtime   1e-6      65    16.1%    16.2%    6.5 ms   latency

  Satellite   eps=0.09   sigma2=5.0  RTT=210 ms  send_rate=2.04k/s
    hint       delta     W*    r*(W*)   r*(64)   recov    binding
    Bulk       1e-2     512    13.7%    20.6%  250.9 ms   overhead (W_max)
    Auto       1e-4     429    20.3%    36.8%  210.0 ms   latency
    Realtime   1e-6     214    30.3%    47.2%  105.0 ms   latency
```

The overhead saving is largest exactly where W matters most: on Satellite
the derived window nearly halves the fixed-64 overhead (Auto 20.3% vs
36.8%) because the 1/sqrt(W) margin is fattest at high eps, while recovery
latency stays within the hint's budget by construction. On DC every hint is
latency-bound — the channel is clean enough that overhead is negligible at
any window, so W* simply maximises the window the budget allows. `derive_window`
in raptorpath-math is the shared implementation; the production window-mode
sender and the visualizer both read W* from it.

> **Unified-machine extension (§16.20, 2026-07-18).** Under the unified
> span machine the same per-hint (δ, budget) rows additionally derive the
> emission-span parameters — quantum width A\* = clamp(rate·D, 1, W\*)
> with D = min(H, 2·RTprop), pipeline depth M\* = ceil(rate·2·RTprop/A\*_q)+1,
> and trailing offset Δ = ceil(rate·J). The hint stops selecting a decoder
> machine; it selects a point on the δ axis and every machine parameter
> follows from that point plus the measured anchors. See the §16.20.5
> constants audit.

### 8.9 The Unified Deadline-Constrained r* (single- and multi-path)

> **MEASURED breakeven note (branch `feat/fec-arq-crossover`, 2026-07-08).** The
> `R_recover` term below charges 0 for an FEC-covered symbol and 1.5·RTT for an
> ARQ-recovered one, so the model predicts proactive FEC dominating ARQ as RTT
> grows (§14.7 crossover). An L1 RTT sweep {10,30,50,100,200} ms (single path,
> 100 mbit, GE ≈ 2.5 %) shows this is **NOT** realized for a *plain-reliable
> in-order frontier* hole: pure-ARQ beats proactive frontier-FEC at every RTT
> (FEC/ARQ ≈ 0.61-0.75, no crossover). The `R_recover = 0` assumption for FEC only
> holds when the covering repair is present AND isolating at decode time; under
> bursty loss at an in-order frontier a pre-position-vs-isolate catch-22 forces
> ~97 % of proactive repair to arrive after ARQ (`present_at_stall = 0`), so the
> hole pays the ARQ term anyway PLUS the displaced-bandwidth tax. The `R_recover=0`
> FEC branch is therefore valid for systematic/generation coding (all window
> members received; §16.3) — where the value prop actually lives — not for frontier
> repair. Decode COMPUTE is never the limiter (~10 µs/symbol measured). Details:
> §14.7 and goal-gate "FEC-vs-ARQ Crossover". UPDATE (§14.33): even when the
> covering repair IS forced present at the stall (a filling-generation pacer that
> raises `present_at_stall` measurably), the single-path crossover still does not
> materialize — the "displaced-bandwidth tax" above is not a side effect but the
> binding constraint: `R_frontier = R_cc·(1 − φ_early(present))`, so buying
> presence on one link necessarily lowers the frontier rate. The `R_recover = 0`
> premium is realizable only across an ORTHOGONAL path, making the crossover a
> multipath-aggregation result rather than a single-path timing one.

Sections 8.4–8.8 solve for r\* on ONE path: pick the least FEC that keeps a
symbol's *within-window-or-ARQ* miss below δ. Section 16.7 introduced a
second knob, the reorder horizon H, and asserted the two are dual — both buy
"fungibility," one in latency, one in bandwidth. This section makes that
precise: **H and r spend the SAME budget — one deadline D** — and the
single-path r\* of §8.4 is the N = 1 limit of one deadline-constrained
optimum. The result is DERIVED here and verified MEASURED-through-oracle
(`raptorpath-math/tests/temporal_oracle.rs`, Part 4).

**The one budget.** A symbol is LATE if its TOTAL delivery delay exceeds a
deadline D. Decompose the delay of a symbol carried on path i (DERIVED — a
first-order decomposition, in the spirit of §8.4's own approximation):

```
   T_delay =  d_i                         one-way propagation + queueing  [path-fixed]
            + R_recover                    0                if arrived or FEC-covered
                                           1.5·RTT_i = 3·d_i if ARQ-recovered   [FEC/ARQ]
            + L_reorder                    cross-path resequencing wait    [ordering]
```

`L_reorder` is present only for **in-order** delivery: a symbol on a lagging
path cannot be released until the frontier reaches it, and the receiver holds
an out-of-order symbol at most H before force-delivering a hole (§16.2). Thus
H is literally the *reorder-share of D*, and §16.2's **eligibility set**
`E = { i : d_i − d_min ≤ H }` is the set of paths whose resequencing lag fits
that share. Writing D = H + D_fec splits the deadline: H pays the reorder
wait, D_fec = D − H pays the within-path arrival + FEC/ARQ recovery.

**P(late) — the objective's constraint.** A symbol lands on path i with
probability g_i/Σg (work-conserving pull, §16.2). It is late in one of two
disjoint-to-first-order ways: its path is reorder-*ineligible* (the whole
share is holes), or its path is eligible but a window FEC-miss forces an ARQ
that overflows the remaining budget. Hence (DERIVED, union/first-order bound):

```
   P(late) ≈ Σ_i (g_i/Σg) · [  1{ d_i − d_min > H }                              (reorder)
                             + 1{ d_i − d_min ≤ H } · e_i(1 − P_fec,i)
                                                    · 1{ d_i + 1.5·RTT_i > D } ]  (FEC-miss ARQ)

   P_fec,i = Φ( √W (r(1−e_i) − e_i) / √(e_i(1−e_i)(r + σ²_i)) )        (§8.2, per path)
```

**The controller's program.** Minimize overhead (proportional to the FEC rate
r) subject to the deadline tail:

```
   minimize    r
   subject to  P(late)(r, H; {e_i, d_i, C_i}, W, ordering) ≤ δ
```

**Convexity / KKT (DERIVED, where tractable).** For fixed H (so E and the
reorder term are fixed), P(late) is a sum over eligible paths of
e_i(1 − P_fec,i(r)). P_fec,i is Φ of an argument that is strictly increasing
in r (∂/∂r of (r(1−e)−e)/√(r+σ²) > 0 for r ≥ 0), so each e_i(1 − P_fec,i) is
strictly *decreasing* in r; P(late) is therefore strictly decreasing and
continuous in r. The feasible set { r : P(late) ≤ δ } is thus an upper
interval [r_min, ∞) — a convex set — and the objective r is linear, so the
minimizer is the **unique boundary point** r\* = r_min where the constraint is
active (P(late) = δ). This is the KKT stationary point: the overhead gradient
(= 1) is balanced by the multiplier on the single active tail constraint. The
Lagrangian ∂/∂r [ r + λ(P(late) − δ) ] = 0 has 1 = −λ ∂P(late)/∂r > 0, i.e.
λ > 0 — the constraint binds, as expected for a minimal-overhead solution.

Because each path must independently keep its own losses under budget (the
conservative "all symbols meet D" contract; cross-path fungible repair is a
separate throughput lever — see the scope note), the binding path is the
worst one, and the unified rate is the **max over the eligible set**:

```
   ┌─────────────────────────────────────────────────────────────────────┐
   │  r*_unified  =  max_{ i ∈ E }  [  e_i/(1−e_i)                        │
   │                                 + z_{δ_i/e_i} · √( e_i σ²_i /        │
   │                                                    (W(1−e_i)) ) ]    │
   │                                                                       │
   │  E = { i : d_i − d_min ≤ H },   z_{δ_i/e_i} = Φ⁻¹(1 − δ_i/e_i)        │
   │  δ_i = the per-path tail share of δ on the ARQ-overflow paths         │
   └─────────────────────────────────────────────────────────────────────┘
```

with the convention max(0, ·) (§8.4's physical floor). H is chosen first (the
reorder budget — loose δ admits more paths, §16.7); r covers the residual
FEC-miss tail on the admitted paths. The two knobs are the two ways to spend
D: raise H to admit a lagging path at the cost of latency, or raise r to make
its window fungible at the cost of bandwidth (§16.7's (H, r) surface).

#### Theorem (N = 1 reduction to §8.4)

*With a single path, r\*_unified reduces EXACTLY to §8.4's r\*(δ, e, σ², W).*

**Proof (DERIVED).** With N = 1, d_1 = d_min, so d_1 − d_min = 0 ≤ H for every
H ≥ 0: the eligibility test is satisfied unconditionally and the reorder term
1{d_i − d_min > H} is identically 0 — there is no cross-path skew to
resequence, so L_reorder ≡ 0 and the whole deadline is available for recovery
(D_fec = D). P(late) collapses to its second term for the one path,

```
   P(late) = e(1 − P_fec)      (on the ARQ-overflow deadline band d < D < d+1.5·RTT)
```

which is exactly §8.4's within-window-or-ARQ tail. The "max over E" degenerates
to a single term, and requiring e(1 − P_fec) ≤ δ with equality gives
P_fec = 1 − δ/e, i.e. the Gaussian argument equals z = Φ⁻¹(1 − δ/e). Solving
§8.2's P_fec expression for r at that quantile yields, to first order,

```
   r*_unified |_{N=1}  =  e/(1−e) + z_{δ/e} · √( e σ² / (W(1−e)) )  =  r*_{§8.4}.   ∎
```

So §8.4 is not a special-cased formula — it *emerges* as the one-path limit of
the deadline program, the moment the reorder term vanishes by construction.

#### Limits and monotonicities (DERIVED)

- **Loose deadline → pure ARQ.** As δ → e from below, δ/e → 1, z_{δ/e} =
  Φ⁻¹(1 − δ/e) → −∞, the margin → −∞ and r\* → max(0, ·) = 0. When the deadline
  budget is loose enough that a bare ARQ round fits (d_i + 1.5·RTT_i ≤ D on
  every path), the ARQ-overflow indicator is 0, the FEC-miss term drops out
  entirely, and r\* = 0 — FEC buys nothing the deadline needs (continuous with
  §8.4's δ → e boundary and §11.3).
- **Out-of-order is H → ∞.** As H → ∞, E = {all paths}, the reorder term
  vanishes for every path, and the reorder-share of D goes to zero — the
  §16.7 "out-of-order = decode-on-total" corner. Ordering is then a pure
  delivery policy: the *unordered* flag removes L_reorder outright (deliver on
  recovery), the *in-order* flag reinstates it (resequence through H). This is
  §16.7's ordering-as-policy, now visible as a single indicator in P(late).
- **Monotone in e.** Both the IT floor e/(1−e) and the margin √(e σ²/·)
  increase with e, so r\*_i is increasing in each path's loss rate; the
  binding path is the dirtiest eligible one.
- **Monotone knobs.** P(late) is ↓ in r (more FEC) and the reorder share is ↓
  in H (larger budget admits more paths) — the two levers of §16.7, now with
  signs proven from the closed form.

#### Verification through the corrected temporal oracle (MEASURED-through-oracle)

`temporal_oracle.rs` Part 4 adds a per-symbol *deadline-lateness* process
(distinct from Parts 1–3's throughput process): stripe K symbols ∝ goodput,
run each path's continuous GE channel window-by-window with r·W repairs,
assign each symbol prop-only or prop+1.5·RTT (ARQ) arrival by whether its
window's surviving repairs cover its losses, then release in the requested
order through an H-hold resequencing frontier and measure the tail. Four
findings (K up to 1.2 M symbols, W = 64, seeded):

```
   check                                    result                              gate
   ──────────────────────────────────────── ─────────────────────────────────── ──────
   N=1 reduction (5 scenarios)   at r*(§8.4), measured late tail = 1.20–1.52×δ;  PASS
   (4a)                          EVERY late symbol is an ARQ miss (reorder ≡ 0),
                                 tail == e(1−P_fec) — §8.4 emerges exactly
   reorder term & ordering flag  H<skew: p_reorder = 0.258 ≈ slow share 0.25     PASS
   (4b)                          (E={fast}); H≥skew: 0.0025 (collapse);
                                 unordered: 0.000 — ordering is a policy
   monotonicities (4c)           P(late) ↓ in r, reorder ↓ in H, P(late) ↑ in e  PASS
   full-grid union-bound (4d)    closed form tracks the measured tail; worst     PASS
                                 ratio 1.37, and it OVER-estimates (conservative)
```

**The one honest discrepancy (reported, not forced).** At r = r\*(§8.4) the
oracle's measured miss tail is **1.2–1.5× δ**, i.e. the closed form
*under-provisions* — the oracle needs ≈ **1.51× r\*** to actually reach δ
(Part 4c). This is not a new defect: it is exactly the Gaussian-tail +
ignored loss/repair-correlation gap §8.4 already flags and §8.7's exact DP
already corrects. The oracle CONFIRMS the sign and bounds the size (~1.5×,
within the §8.7 exact-DP band). Production therefore uses r\*_unified as the
analytic *floor* and closes the gap with `compute_min_rate_exact` (§8.7) on
the binding path — the closed form never dangerously over-provisions
(r_min ≥ 0.85·r\* always), and the multipath reorder term is faithful to the
eligibility-set prediction to within 3% of the slow-path share.

The union bound in P(late) OVER-estimates where the two late causes co-occur
(a slow-path symbol can be *both* a reorder hole and an ARQ overflow — the
bound double-counts). Over-estimation is the safe direction for a controller
that must not under-provision; the oracle's worst-case 1.37× over-count is
logged rather than tuned away.

#### Scope: r\* is orthogonal to the throughput ceiling (honest)

The L1 record (§16.7) showed heterogeneous **throughput** aggregation is
transport-ceiling-limited: at bulk's systematic operating point the moving
window strands slow-path source, and the aggregate sits at MPTCP parity, not
Σg_i — a *transport* limitation (per-path-affine atomic units + throttled
recovery), reproduced by Parts 1–3 of the same oracle. **That ceiling does
not touch this derivation.** r\*_unified is the FEC-rate controller for the
reliability/latency budget (the deadline tail), which is orthogonal to how
much *goodput* the transport can extract from the path set. The r\* model
assumes only that the transport can deliver at the per-path rates g_i; whether
it aggregates those rates above one path is the separate, measured-open
question of §16.7. Concretely: Part 4 credits each path's own FEC to its own
budget (no cross-path fungible repair), so its verdict is independent of the
Parts 1–3 aggregation result — the two processes share the channel model and
nothing else.

> **UPDATE (2026-07-12) — the aggregation ceiling is being closed in stages, and
> now reads ceiling-relative, not as a bare "parity" factor.** Against a *measured
> recovery ceiling* (single-path, same binary) `C8 = single-c2 + single-c3 =
> 16.54 + 3.26 = 19.80 Mbit/s` for the c2+c3 heterogeneous pair, three levers have
> been added and measured at L1:
> 1. **DAPS delay-aware scheduling + right-sized r** removed the slow-path
>    frontier-serialization long pole (frontier pause 13–68%→0%): C8 0.48×→0.80×
>    single-fast in that arc's conditions.
> 2. **Per-path BLEST cap + BBR pacer** (queue management) — correct in-model
>    (temporal_oracle PART 6e: given a correct per-path BDP, the queue collapses
>    and C8 reaches ×1.195) but INERT in production because generation mode never
>    *estimated* a per-path rate.
> 3. **Per-path delivered-rate estimator** (branch `feat/per-path-estimator`):
>    per-path ACK attribution (seq→path ownership) now drives a real per-path
>    BtlBw/RTprop/BDP — established in **93%** of sender-DIAG windows vs **0%**
>    before — and the min-filtered RTprop stays at the 44 ms propagation base, so
>    the per-path source pacer drains the slow-path bufferbloat from **3734 ms to
>    ~300 ms**. This ELIMINATES the catastrophic-bloat seed and STABILIZES C8:
>    two-seed pooled C8 rises from **0.40 to 0.52 of the recovery ceiling**
>    (×0.47→×0.62 single-fast; baseline was bimodal 5.88/9.81, post-estimator
>    stable 9.58/10.90 Mbit/s), C7 no-regression (21.41, 1.29× single-c2), all
>    arms dnf=0.
>
> **Revised verdict.** Heterogeneous throughput aggregation is *not one* ceiling
> but a stack: scheduling-bound (DAPS, escaped), rate-estimation-bound (the
> per-path estimator, now closed — a real per-path BtlBw exists where there was
> none), and still queue-bound on the *non-source* traffic (the coded/repair
> emission and the fast-path queue are not yet per-path pace-bounded, so live RTT
> holds ~140–380 ms above the propagation base and C8 sits at 0.52, not 1.0, of
> the recovery ceiling). The pre-estimator "sits at MPTCP parity" reading was a
> *rate-signal* artifact — with a real per-path rate the aggregate is materially
> above parity and stable, but the remaining gap to the ceiling is now a
> non-source queue-management residual, not a rate-estimation or scheduling one.
> None of this touches the r\*_unified derivation (it assumes only per-path
> deliverability at g_i, which the estimator now measures directly).

---

## 9. Codec Overhead Integration

### 9.1 Decoder Invocation Probability

For systematic codes (METTLE, RLC, RaptorQ), the decoder is only invoked when
at least one source symbol in the window is lost:

```
   P(decoder invoked) = 1 - (1-e)^W
```

| Scenario | ε     | W=50                | W=200               |
|----------|-------|---------------------|----------------------|
| DC       | 0.001 | 4.9%                | 18.1%                |
| WiFi     | 0.025 | 72.1%               | 99.4%                |
| Satellite| 0.09  | 99.1%               | ≈100%                |

### 9.2 Effective Codec Overhead

The codec overhead (ε_codec) represents extra symbols the decoder needs beyond
the information-theoretic minimum:

| Backend      | ε_codec | Reason                                |
|--------------|---------|---------------------------------------|
| Reed-Solomon | 0.0%    | MDS: any k of n suffices                        |
| RLC          | 0.4%    | Near-MDS: rare rank deficiency [RFC8681]         |
| RaptorQ      | 1.0%    | Fountain code convergence overhead [RFC6330]     |
| METTLE       | 15.0%   | Sparse random matrix: needs more eqns [Yu2025]  |
| Streaming    | 0.0%    | Rate-optimal by construction [Badr2017]          |

Weighted by decoder invocation probability:

```
   e_codec_eff = e_codec x P(decoder invoked) = e_codec x (1 - (1-e)^W)
```

The corrected correction rate becomes:

```
   r* = max(0, (e + e_codec_eff)/(1-e)
               + z_{δ/e_hat} x √((e + e_codec_eff) x σ²_burst / (W x (1-e))))
```

(the same margin structure as Section 8.4, with e replaced by
e_hat = e + e_codec_eff — including inside the quantile ratio δ/e_hat).

### 9.3 Impact on METTLE at DC

At DC (ε=0.1%, W=50, σ²=3.0, Auto δ=1e-4 — under Bulk the DC rate is
0 with or without weighting, so Auto is the informative case):

Without weighting: ε_hat = 0.1% + 15% = 15.1%, z_{δ/ε_hat} = 3.21
→ r* = 15.1% + 30.6% = 45.7%.
With weighting: ε_codec_eff = 0.15 × 0.049 = 0.74%, ε_hat = 0.84%,
z_{δ/ε_hat} = 2.26 → r* = 0.8% + 5.1% = 5.9%.

The weighting reduces METTLE's DC overhead by ~8×.

---

## 10. Multi-Protocol Extension

### 10.1 Per-Symbol Latency Classes

Different traffic types have different δ targets:

```
   Symbol s has latency class c(s) with target δ_{c(s)}
```

### 10.2 Interleave Before vs After FEC

**After FEC (separate streams):**
```
  Realtime packets ──► [FEC encoder (δ=1e-6)] ──► channel
  Bulk packets     ──► [FEC encoder (δ=1e-2)] ──► channel
```
Each stream has its own taper. No correction sharing. Simple.

**Before FEC (shared stream):**
```
  All packets ──► [mixed FEC encoder] ──► channel
```
One taper covers everything. Repair symbols are linear combinations of ALL
source symbols [RFC8681] — a repair can recover ANY lost symbol regardless of class.

**Advantage of shared:** repair symbols are fungible. A repair generated "for"
a Realtime symbol can recover a Bulk symbol if needed. Total correction budget can
be lower than the sum of separate budgets (statistical multiplexing).

**Disadvantage of shared:** the taper must be sized for the tightest class.
If 1% of traffic is Realtime (δ=1e-6) and 99% is Bulk (δ=1e-2), you pay
Realtime-level overhead on everything.

### 10.3 When Shared Wins

Shared FEC is cheaper when the traffic mix is balanced or dominated by the
tight class. Separate streams are cheaper when the tight class is a small
fraction. The crossover depends on the specific δ values and loss rate.

**Decision rule:** Compare total correction bandwidth:
```
   shared_cost = r*(e, min(δ_c))         (one encoder, tightest δ)
   separate_cost = Σ_c f_c x r*(e, δ_c)  (per-class, weighted by fraction f_c)
```

Choose whichever is lower.

### 10.4 Extending the Formula

For shared FEC with per-symbol δ, the constraint becomes:

```
   For each class c: e x (1 - P_fec) ≤ δ_c
```

Since P_fec is the same for all symbols (shared repair), the binding constraint
is the tightest: δ_min = min(δ_c). The formula reduces to the single-class case
with δ = δ_min.

---

## 11. Verification

### 11.1 Simulation Approach

Generate synthetic loss traces from the GE model, apply the taper function,
and measure whether the actual tail latency matches the theoretical prediction.

```
   For each trial:
     1. Generate GE loss trace: {lost_1, lost_2, ..., lost_N}
     2. Generate correction schedule from taper: {correction_1, correction_2, ...}
     3. Apply channel loss to both source and correction symbols
     4. For each lost source symbol:
        a. Check if enough repair symbols arrived (FEC recovery)
        b. If not, mark as ARQ-recovered (retransmit needed)
     5. Count ARQ events (symbols not FEC-recovered)
     6. Measure: actual_arq_fraction = arq_events / N

   Over many trials:
     Verify: P(actual_arq_fraction > δ) is small
     Verify: mean correction rate ≈ r*
```

### 11.2 Analytical Predictions to Verify

| Prediction | Formula | Test |
|------------|---------|------|
| IT minimum dominates at high loss | r* ≈ ε/(1-ε) when W large | Satellite scenario |
| Tail margin scales as 1/√W | r*(W=200) < r*(W=50) | Compare window sizes |
| Taper shape matches GE | Decay rate = q̂ from estimator | Compare simulated vs theoretical |
| Codec overhead weighting | METTLE DC overhead ~ 0.74% not 15% | DC scenario with METTLE |
| Protocol hint only affects δ | r*(Realtime, δ=1e-6) vs r*(Auto, δ=1e-4) differ only via z_δ | Same estimator, different hints |

### 11.3 Boundary Cases

| Case | Expected behavior | Why |
|------|-------------------|-----|
| ε = 0 (no loss) | r* = 0 | No correction needed |
| ε → 1 (total loss) | r* → ∞ | Can't recover anything with FEC alone |
| δ = ε (every loss to ARQ) | r* = 0 | No FEC needed, all ARQ |
| δ → 0 (zero ARQ tolerance) | r* → ∞ | Must FEC-recover everything |
| W = 1 (no window) | Margin term large | Single-symbol recovery needs more redundancy |
| q = 1 (no burst memory) | τ(t) = flat | Reduces to iid case |

### 11.4 Connection to Existing Benchmarks

The bench_suite already measures `overhead_pct` and `recovery_pct` per scenario.
To verify the model:

1. Compute r* from the formula for each scenario's (ε, δ, W)
2. Compare with the bench_suite's measured overhead
3. The gap between r* and measured overhead = estimation tax + implementation overhead
4. Track this gap across benchmark runs — it should decrease as we improve the implementation

**Canonical evaluation suite.** The scenario matrix, baseline fidelity
ladder (in-process model → real TCP/QUIC/MPTCP stacks over netem → real
links), and the quantitative win conditions that make "surpasses TCP/MPTCP"
a falsifiable claim are defined in ADR-0051 (canonical evaluation
scenarios). The channel rows there are exactly the Section 2.4
parameterizations, so the model, the simulator, and the benchmarks share
one vocabulary. The suite deliberately includes cells where naive FEC
loses (clean links, congestion-dominant loss with a competing TCP flow,
mid-transfer path outage) — ties are the win condition there.

---

## 12. Congestion Control Integration

> **Amendment (measured, `feat/transport-substrate`): pace FEC emission — source
> AND repair — at the CC rate, and put reactive recovery UNDER the CC.** The
> window/generation FEC sender originally emitted the systematic SOURCE unpaced
> (gated by a BDP-sized window, not a rate) and ran the reactive deficit loop
> EXEMPT from the congestion cap. At high RTT this burst-overran the droppable
> datagram path and let reactive recovery run away (measured `recovery_coded`
> 90 k–252 k for a ~5 k-symbol object). The fix routes source + proactive + reactive
> through a token bucket paced at **max(Copa cwnd/SRTT, delivered-goodput EWMA)** —
> the cwnd/SRTT term is essential because the goodput EWMA is clocked on the
> in-order ack, which stalls to 0 on any hole and would pin the pace at the
> bootstrap floor — and bounds reactive to one deficit-batch per SRTT per
> generation, non-exempt from the in-flight cap. This eliminates the runaway/DNF
> and stabilizes throughput (see §14.7 "Transport-substrate correction" and
> goal-gate "Transport Substrate Fix"). Note it does NOT by itself make FEC beat
> ARQ at high RTT — a receiver-side frontier-serialized reactive tail remains.
>
> **Follow-on (measured, `feat/receiver-tail`): a BDP-derived in-flight cap for the
> generation/FEC mode.** The plain-reliable path already bounds its outstanding
> window to gain × BtlBw·RTprop (Copa `bdp_anchor`) so the standing queue — and thus
> recovery-round RTT — stays ≈ 1 RTT (§12, delay-based window). The generation mode
> lacked this: its store-based backpressure (`store_max`) is a memory bound, not a
> pipe bound, so a wide retention store bloats the wire queue. `RWM_INFL_BDP` adds
> the same BtlBw·RTprop-derived cap to total in-flight for the generation mode,
> gating BOTH proactive AND (non-exempt) reactive/deficit emission, so the parallel
> receiver-tail flush cannot re-bloat the queue. It bounds the queue as intended, but
> — like the sender pacing above — does not by itself produce a throughput crossover
> (§14.7 "Receiver-tail correction").

### 12.1 Why Delay-Based CC is Required

On lossy links (WiFi, LTE, satellite), loss-based CC (NewReno, CUBIC) is
catastrophic. Every random channel loss triggers cwnd halving, collapsing
throughput on exactly the links raptorpath is designed for:

```
  NewReno on 2.5% WiFi loss:
    -> cwnd halves on every loss event
    -> throughput oscillates wildly, average << capacity
    -> our FEC tries to compensate -> more traffic -> more congestion

  Delay-based CC on 2.5% WiFi loss:
    -> loss + stable RTT = channel loss -> ignore, let FEC handle it
    -> loss + rising RTT = congestion   -> reduce rate
    -> throughput stays near capacity
```

Delay-based CC distinguishes congestion (queue buildup = rising RTT) from
channel loss (random drops = stable RTT). This is essential for raptorpath.

### 12.2 QUIC Datagrams Bypass Quinn's CC

> **REFUTED (2026-07-13, §16.17 / §12.11).** Datagrams bypass quinn's
> RELIABILITY machinery but NOT its congestion controller: quinn gates every
> packet send on its congestion window. The paragraph below is retained as
> the original (incorrect) design assumption; see §12.11 for the corrected
> model (substrate CC as policy) and §16.17 for the measurement that exposed
> it.

Raptorpath sends symbol data as QUIC unreliable datagrams, which bypass
Quinn's built-in congestion control (NewReno). Our own CC is the sole rate
limiter for data traffic. Quinn's CC only applies to QUIC streams (handshake,
reliable control messages — small and infrequent).

This means our CC is solely responsible for not flooding the network.

### 12.3 Copa vs BBR

The current implementation uses BBR (ADR-0019). We recommend migrating to
Copa [Copa2018] because of better interaction with the taper function:

| Aspect | BBR | Copa |
|--------|-----|------|
| Core idea | measure max_bw x min_rtt = BDP | rate = 1/(d x dq) |
| Queue draining | ProbeRTT: 200ms forced drain every 10s | Natural oscillation |
| Phases | Startup -> ProbeBw -> ProbeRtt (state machine) | No phases, one formula |
| Lossy links | RTT-trend check to ignore random loss | Delay-based, same effect |
| Complexity | ~300 lines, multiple edge cases | One rate formula |
| Taper interaction | ProbeRTT creates FEC protection gap | Smooth, no gaps |

**Implementation status:** Copa is recommended as the target congestion
control algorithm. The current codebase uses BBR (ADR-0019). Migration
to Copa is a future implementation task. The model and formulas in this
paper are CC-agnostic — they work with any delay-based CC that provides
a total sending rate.

**The critical difference — taper compatibility:**

```
  Copa:                            BBR ProbeRTT:

  Rate                             Rate
    |  /\/\/\/\/\/\/\/\              |  ________            ________
    | /                \             | |        |          |
    |/                  \/\/\/\      | |        |__________|
    +----------------------> t       +----------------------> t

  Taper                            Taper
  coverage                         coverage
    |  ==================            |  ========            ========
    |  continuous protection         |  gap!     ^^^^^^^^^^
    +----------------------> t       +----------|-----------> t
                                         200ms FEC blind spot
```

Copa's rate oscillation is smooth and fast (order of RTT). The taper function
uses RATIO (corrections per source symbol), which stays constant. Absolute
rates oscillate gently, but the EWMA smooths throughput estimates. The taper
doesn't notice Copa's oscillation.

BBR's ProbeRTT drops cwnd to 4 symbols for 200ms. During this period:
- Source rate drops to nearly zero
- Correction rate drops proportionally (ratio constant, absolute count tanks)
- Source symbols sent during ProbeRTT get almost no FEC protection
- If a burst loss hits during ProbeRTT, recovery is severely degraded
- After ProbeRTT exits: data burst -> potential queue buildup

### 12.4 Copa's Rate Formula

Copa targets a queue occupancy that balances throughput and delay:

```
  rate = 1 / (d_copa x dq)

  where:
    d_copa = Copa parameter (controls target queue depth; default 0.5)
    dq     = queuing_delay = RTT_current - RTT_min
```

When dq is small (queue empty): rate is high -> fill the pipe.
When dq is large (queue building): rate drops -> drain the queue.

This naturally oscillates: send fast -> queue builds -> send slow -> queue
drains -> send fast again. The oscillation frequency is ~1/RTT and amplitude
depends on d_copa. No periodic forced drain phase needed.

**Copa's d_copa is NOT the same as our tail latency target δ.** Copa's
d_copa controls the target queue depth at the bottleneck (1/d_copa packets).
Our δ controls the tail probability of late delivery. They are independent
parameters that happen to use similar notation in their respective papers.

d_copa = 0.5 (from the Copa paper [Copa2018]) nominally targets a queue of
2 packets. That does NOT mean the realized cost is ~1 packet of delay: at
high utilization the sender oscillates around the controller's queue
target, so the median delivery latency carries a standing queue on the
order of that target — not of a single packet. Measured in the gate
simulation (100 Mbps WiFi under sustained load), the tolerated standing
queue adds ~9 ms to the median latency versus an idle baseline (p50
~17.6 ms loaded vs ~8.1 ms idle) — a dominant latency term, not a
negligible one. The protocol hint must therefore set the delay target too,
not only the FEC rate: Realtime → a near-empty queue target (accepting
some utilization loss), Bulk → a deeper queue target for better link
utilization, and Auto keeps the d_copa = 0.5-style default. A continuous
mapping from the tail latency budget to d_copa is the natural refinement;
a coarse per-hint target already recovers most of the standing-queue
latency.

**min_rtt estimation:** Copa uses the minimum observed RTT in a 10-second
sliding window (same duration as BBR's min_rtt window). Copa's natural rate
oscillation periodically reduces the queue to near-empty, refreshing the
minimum within this window. If min_rtt has not been refreshed for an extended
period (>20s), the system can force a brief rate reduction as a fallback
— though this is rarely needed because Copa's oscillation provides
natural refreshing.

**Production implementation notes (P7 port).** The production scheduler
implements the Copa-lite scheme exactly as proven in the gate driver
(windowed-min queue signal, hint-coupled queue target x{1.08, 1.125,
1.25}, two-speed ramp x1.5+1 then +2/x0.92, per-SRTT update cadence,
token-bucket pacing at cwnd/SRTT with burst max(10, cwnd/8)), with these
honest deviations in constants and mechanics:

- **Propagation floor** = min RTT sample over the 10 s sliding window
  (the driver used the lifetime min because its trials were shorter than
  any sane window; the 10 s window is the faithful production analogue
  and self-heals after a route change).
- **dq clamp at 0.1 ms**, applied to BOTH the queuing-delay estimate and
  the backoff threshold (queue_mult - 1) x floor. This keeps the
  closed-form rate 1/(d_copa x dq) finite on LAN-class floors and makes
  the backoff comparison continuous there (dq at the clamp cannot exceed
  a threshold at the same clamp) — no branch cliffs. The rate formula
  itself is retained as a diagnostic only; the cwnd dynamics are the
  ramp/backoff scheme.
- **Jitter-robust queue signal** (three coupled changes, one cause).
  The P1 mapping implicitly assumed path jitter << the queue target;
  the L1 C2 cell (10 ms floor, netem +-3 ms per direction) violates
  it, and the sender took a x0.92 backoff on ~60% of updates — cwnd
  pinned near the 8-symbol floor vs BDP ~160 (the dominant term in the
  16x rp-vs-quinn gap at C2). Measured root cause: the queue signal
  compares a min-of-N statistic (N ~ 4-30 ACK samples per SRTT window)
  against the 10 s propagation floor, a min-of-thousands. Under jitter
  these are different statistics: at C2 the 10 s floor found 7.0 ms
  while a typical window min sits at 12-13 ms — a permanent apparent
  dq of ~5 ms with an EMPTY queue, which no windowed min can see
  through (and the link's jitter FIFO correlates consecutive samples,
  so consecutive-difference jitter measures only ~0.85 ms of the
  ~6 ms spread). The production remedy:
  (1) **Quantile queue floor**: the backoff comparison uses
  queue_floor = P10 of the per-update window-min history (10 s
  window) instead of the raw 10 s min — the floor becomes the same
  min-of-N statistic as the signal, so it is self-calibrating under
  any jitter correlation structure. A genuine standing queue shifts
  every window min within one SRTT while the 10 s quantile lags, so
  congestion is still detected within a few updates; a queue
  SUSTAINED longer than the window becomes the new baseline (the same
  staleness bound the 10 s min floor already had).
  (2) **Jitter headroom**: threshold = (queue_mult - 1) x queue_floor
  + k x max(jitter_est, win_jitter_est), k = 2. jitter_est is an EWMA
  (gain 1/8) of |consecutive RTT sample differences| (RFC 3550-style);
  win_jitter_est is the SAME consecutive-difference EWMA one level up
  (gain 1/4, over per-update window minima) — under correlated jitter
  the raw differences collapse (~0.85 ms at C2) while window minima
  wander 3-5 ms per update, and only the window-level estimator sees
  that amplitude. Both are shift-robust: a standing queue contributes
  one transition sample, not persistent inflation (a quantile-spread
  term was rejected for exactly this reason — a level shift inflates
  it for a full window and kills congestion detection; caught by the
  cwnd-floor unit test).
  (3) **Ramp fast-exit needs >= 3 samples**: a partial window's min
  can be a single draw from the jitter tail; one sample must not end
  the exponential ramp.
  Continuity: on a clean link every window min equals the floor, the
  quantile equals the floor, jitter_est -> 0, and P1 semantics are
  recovered exactly; no cliff in any variable. Honest cost: the
  tolerated standing queue grows to ~(queue_mult - 1) x queue_floor +
  2 x jitter above the TYPICAL window min rather than the extreme
  min; for Realtime this is fundamental, not incidental — a queue
  smaller than the jitter spread is statistically indistinguishable
  from jitter at windowed-min sample counts.
- **cwnd floor = 8 symbols** (the driver backed off to >= 4). An L1 run
  on a real emulated link (100 Mbit, 10 ms RTT) showed the pre-P7
  rate-formula collapse crawling at cwnd = 2; the raised floor guarantees
  a trickle that keeps RTT samples, and therefore recovery, flowing.
- **Ramp fast-exit**: during the ramp the backoff check also runs per
  ACK (not only per SRTT), so the exponential phase ends within one
  feedback message of the first standing-queue evidence.
- **Pacing is symbol-level via a carry queue**: the interleaver's drain
  is all-or-nothing, so drained symbols land in a per-path carry queue
  and each pace tick sends only floor(tokens) symbols; the remainder
  waits for the next tick (wakeup = next-token refill time, clamped
  0.5-50 ms). The first cut gated at batch granularity and let a whole
  block overdraft; measured at L1 (C2, Bulk) every 56-symbol block
  burst serialized ~5.4 ms of self-queue — above Bulk's 2.5 ms backoff
  threshold — so every block bought a x0.92 backoff and cwnd pinned
  just under one block (~34 symbols). Symbol-level pacing removes the
  self-queue entirely. Carried symbols count toward the TUN-read gate
  (in_flight + carried >= cwnd), which remains the outer backpressure.
- **Bulk flush timeout 5 ms** (was 50 ms): while the CC gate pauses TUN
  reads, 64KB block assembly stalls mid-block; a 50 ms flush serialized
  with the congestion window and clumped the pipeline into ~300 ms ACK
  bursts at L1. 5 ms bounds the assembly wait well under one C2 RTT.
- **in_flight is a schedule-time budget with time-based release.** Each
  symbol is charged exactly once when scheduled (covering interleaver +
  pacing carry + wire) and released by ACK feedback. Because ACKs are
  best-effort datagrams, a lost ACK strands its release; stranded budget
  expires after max(4 x SRTT, 250 ms) (RFC 9002-style time threshold at
  budget granularity) instead of jamming the TUN gate until a 2 s decay
  backstop. The second L1 round found the gate cycling at exactly that
  2 s cadence (~30 KB/s): the send path was charging the budget a SECOND
  time at wire time, leaking +1 per symbol. Pacing tokens, not the gate,
  remain the wire-rate limiter, so an early expiry can only let the
  encoder run ahead, never the wire.
- **Loss reaction** (the driver has none, by design): a decode FAILURE
  with the standing queue above target takes the same x0.92 backoff as
  the delay signal; a decode failure with an empty queue ends the ramp
  and steps cwnd down by 1 (FEC under-provision, not congestion). Loss
  that FEC recovered never touches cwnd.

**Loss-blind claim — VERIFIED in code (L1, 2026-07-07).** The C2 single-path
throughput collapse under ~2.5 % bursty GE loss (76 → 14 Mbit) was investigated
as a suspected loss-triggered cwnd reduction (which would violate this section).
It is NOT: `RWM_DIAG` cwnd traces under C2 show cwnd GROWING (plain 29→628,
systematic 254→3390), never collapsing, and `on_loss` provably touches cwnd only
on a decode FAILURE, never on FEC-recovered loss. The loss-blind, delay-only CC
holds end to end. The collapse has two other causes, one an honest deviation from
this section's intent:

- **The reliable window sender BYPASSED the delay-based CC (bufferbloat).** Its
  TUN-read backpressure gated on a fixed retention-store cap (`RELIABLE_STORE_MAX
  = 1024` symbols ≈ 12× the C2 BDP), not on the Copa-lite window, so nothing bound
  the standing queue to the pipe (MEASURED RTT 0.41–0.52 s vs a 10 ms base). §12's
  premise — the delay-based CC sets the total wire rate — was not actually enforced
  on this data path. FIX: the plain-reliable sender now caps its outstanding window
  at gain × (BtlBw×RTprop) using the same anchor as §12.6 (bufferbloat-robust:
  windowed-max rate × min-RTT floor), restoring RTT to ~40 ms with no clean-link
  regression. The generation/systematic paths already carried an analogous
  structural cap (store = 2·G).
- **A recovery-latency floor at the in-order cumulative-ack frontier** (independent
  of the CC): under the Bulk operating point (§12.5, r*→0, pure ARQ) a hole freezes
  the contiguous ack frontier and recovery is one reactive round-trip, so goodput
  ≈ window/RTT — a ~16 Mbit ceiling that the CC cannot lift and that the bufferbloat
  fix does not change. This is a transport-pipeline limit (pipelined or rateless
  frontier recovery), not a CC or FEC-sizing limit. See goal-gate "Loss-Recovery".

**Wire-clocked delay term + the hint→δ mapping (feat/copa-wire-signal,
2026-07-13, measured).** The §12.11 Copa-sole battery named the bulk gap's
mechanism: the CC delay term was fed the APP-LAYER ECHO RTT (batch timestamp
echoed by the receiver), which includes the sender's OWN store/reservoir
dwell in quinn's datagram queue — Copa backed off against self-inflicted
pipeline delay that is not in the network (§12.11 arm D proved the term:
shrinking the reservoir raised throughput +13–23% AND tightened the queue).
Active only when the engine owns/feeds the substrate window
(`RWM_QUIC_CC=passthrough` or `RWM_COPA_FEED=1`; `RWM_COPA_WIRE=0` restores
the app-echo behavior for A/B; everything unset = shipped path
byte-identical), four coupled changes:

1. **The wire clock.** Copa's queue signal is quinn's packet-timed path RTT
   (`Connection::rtt()`, RFC 9002 estimator, ack-delay corrected): send of
   an ack-eliciting packet → ACK receipt, measured BELOW the datagram queue,
   so the sender's own reservoir dwell is structurally excluded. The
   app-echo RTT stays with the reliability/tail machinery (LossEstimator,
   ARQ timeouts), where end-to-end pipeline delay is the right quantity —
   two clocks, each at its own layer. d_q = wire_standing − wire_RTTmin(10s)
   − k·jitter, where wire_standing is the LATEST smoothed sample (Copa's
   RTTstanding), NOT a per-window min: the δ-law's own drain trough falls
   inside every update window, so a windowed min reads "queue empty" every
   update and the direction ratchets (measured: cwnd pinned MAX_CWND).
2. **The hint→δ mapping (no new constants).** Copa's utility is
   U = log(throughput) − δ·log(delay): δ IS the marginal latency price. The
   protocol hint already declares exactly one price ratio — the tail-loss
   scale ζ ∈ {0.01 Realtime, 1 Auto, 100 Bulk} (this section's r* margin
   knob). Anchoring Auto at the Copa-paper default δ = 0.5:

       δ(hint) = 0.5 / ζ(hint)  ∈ {50 Realtime, 0.5 Auto, 0.005 Bulk}

   Equilibrium standing queue = 1/δ packets (rate = 1/(δ·d_q) at bottleneck
   rate μ ⇒ q = 1/δ), i.e. d_q* = 1/(δ·μ): Bulk tolerates 200 symbols
   (≈19 ms at c2's 10.4 k sym/s at equilibrium; the sawtooth's drain
   phases pull the MEASURED p50 to 4–7 ms vs BBR-under's 38 ms), Realtime
   an essentially empty queue (jitter headroom governs), Auto the classic
   2-packet target. One continuous knob (`RWM_COPA_DELTA` overrides for
   frontier measurement), no mode switch. (§16.20 extends the same
   principle to the coding machine itself: the hint's δ point also derives
   the emission-span parameters A*/M*/Δ — the CC price knob and the span
   law are the two consumers of ONE hint→δ mapping, and neither selects a
   different machine.)
3. **The actual Copa update law** (replacing the ramp/±2 scheme, wire mode
   only): per SRTT, direction = (cwnd/srtt ≤ 1/(δ·d_q)), step = v/δ with
   velocity v doubling only after the direction persists ≥3 updates (Copa
   §2.2 hysteresis; every-window doubling measured cwnd→MAX_CWND) and
   resetting on a flip. The legacy +2 additive probe is exactly this law's
   up-step at δ = 0.5, v = 1 — continuity with P1. Down-steps are capped at
   the measured queue μ̂·d_q (never drain the pipe itself). Two supports:
   a **coupling cap** cwnd ≤ BDP + 2/δ (fixed point + one dither amplitude;
   above the sender's outstanding store the delay signal is DECOUPLED — the
   queue no longer grows with cwnd — and the jitter-clamped d_q votes "up"
   forever: measured ratchet to 4 000–7 800 vs the ≈300 fixed point, with
   window/RTT bursts tail-dropping the 1000-packet qdisc); and **CC-rate
   source pacing** (`RWM_CC_PACE`, default ON under the wire signal): Copa
   assumes a paced wire, but quinn's pacer derives from the engine window
   (≈5×BDP at Bulk's δ) and never binds — pure ack-clocking lets every GE
   recovery micro-stall idle the bottleneck (measured: 55.7 → 67 Mbit/s at
   c2 from the pacing default alone, wire queue p50 3–5 ms).
4. **Floor freshness without ProbeRTT**: the ±v/δ dither drains the queue
   to ~empty regularly, so the RAW 10 s min of the smoothed wire samples
   stays at base (measured per path vs the known netem base: rtp = 10–12 ms
   at c2's 10 ms, 40–42 ms at c3's 40 ms). The legacy quantile queue-floor
   is NOT used in wire mode — under a deep Bulk standing queue it would
   creep up to the queue itself within its 10 s window.

Measured verdict and the arm table: goal-gate "Copa Wire-Signal"; §12.11
addendum below for how it changes the Copa-sole conclusion.

**TCP-competitive mode (feat/copa-compete, 2026-07-19 — Copa §2.2 built on
the wire signal).** Copa (Arun & Balakrishnan, "Copa: Practical Delay-Based
Congestion Control for the Internet", NSDI 2018, §2.2 — the earlier ledger
sections cited this as "Copa §4"; the mechanism lives in the paper's §2.2)
defines two operating modes, and the second was the named deployment gap of
§12.11: without it, "when the bottleneck is shared with loss-based
congestion-controlled flows that fill up buffers, Copa, like other
delay-sensitive schemes, achieves low throughput."

1. **Detection (verbatim mechanism).** Copa's own dynamics empty the
   bottleneck queue at least once every 5·RTT when only Copa flows share it
   (Copa §3); a concurrent long-running buffer-filling flow breaks that
   periodicity. Per the paper: if the sender sees a "nearly empty" queue in
   the last 5 RTTs it remains in (or returns to) the DEFAULT mode; otherwise
   it switches to COMPETITIVE mode, where "nearly empty" is any queuing
   delay d_q < 0.1·(RTTmax − RTTmin), RTTmax over the past 4 RTTs, RTTmin
   the long-term minimum — the RTTmax term self-calibrates the test to the
   path's short-term RTT variance. Our detector runs on the WIRE clock
   (§12.4 wire addendum): d_q and RTTmax/RTTmin are quinn's packet-timed
   samples, so the sender's own reservoir dwell cannot masquerade as a
   competitor (app-echo detection would have inherited exactly the #80
   self-signal failure). One guard for the degenerate case: a d_q at the
   0.1 ms clamp floor counts as nearly-empty even when the 4-RTT variance
   term is ~0 (an idle/clean link must never read as "never empty").
2. **The competitive law (AIMD on 1/δ).** In competitive mode the sender
   varies 1/δ "according to whatever buffer-filling algorithm one wishes to
   emulate"; the paper's implementation — and ours — performs NewReno-style
   AIMD on 1/δ on packet success or loss: no loss in the update window ⇒
   1/δ += 1 (per RTT = per SRTT update), loss ⇒ 1/δ halves. The underlying
   delay-sensitive law is UNCHANGED — competition only moves its price
   knob, which is why Copa in competitive mode keeps better RTT fairness
   and loss resilience than the TCP it emulates (paper §5.5). The loss
   signal is quinn's wire-level loss detection, read from the pass-through
   shim's recorded `congestion_events` counter — the same packet-timed
   layer as the d_q clock; FEC recovery status is deliberately irrelevant
   here (the AIMD prices aggressiveness against a loss-based competitor;
   it does not gate delivery, which remains the FEC layer's job, §12.1).
3. **Composition with δ(hint) — the hint sets the BASE, competition adapts
   around it.** The paper's default-mode δ is 0.5 and its competitive bound
   is "δ ≤ 0.5, reset δ to 0.5 on switch-back". Under the §12.4 mapping the
   default-mode δ is δ_base = δ(hint) = 0.5/ζ, so the faithful
   generalization substitutes δ_base for 0.5 everywhere: competitive mode
   ENTERS at δ = δ_base, AIMD keeps 1/δ ≥ 1/δ_base (never more
   latency-sensitive than the hint's declared price while competing;
   for Bulk the AIMD starts from 1/δ = 200 — the hint's aggressiveness is
   the floor, not a ceiling), and switch-back RESETS δ = δ_base. 1/δ is
   bounded above so the coupling cap's 2/δ term stays ≤ MAX_CWND (the
   §12.4 decoupling lesson applied to the adapted δ).
4. **Hysteresis = the paper's own 5-RTT window, both edges.** A
   competitive-mode Copa cohort still empties the queue every ~5 RTT when
   no buffer-filler is present, so an erroneous entry self-corrects within
   a few RTTs (the paper documents — and accepts — brief flaps around
   losses). No extra state was invented; mode evaluation runs at the
   per-SRTT update cadence and is skipped during the startup ramp (the
   ramp's own transient queue is not competitor evidence).

Gated `RWM_COPA_COMPETE` (default OFF) and only on top of the wire-clocked
law (the adapted δ feeds the §12.4 velocity/target dynamics; the legacy
app-echo dynamics do not consume δ). Env unset ⇒ byte-identical. Measured
(goal-gate "Copa Competitive Mode + Cross-Traffic", the first
shared-bottleneck battery, roadmap item 6): at the lossy c2 cell Copa-sole
never needed it (0.88–0.90 share vs a Mathis-bound Cubic, with or without
competitive mode); at the CLEAN shared bottleneck the mode detects and
adapts exactly as specified (8/8 engagement, δ → 0.0032–0.0043; zero
false engagement in the clean solo control) but CANNOT restore a fair
share (2.24–2.37 vs 2.15–2.21 Mbit compete-off, Cubic at 93) — because δ
is not the binder there: a fixed δ = 0.001 probe (queue tolerance = the
entire 1000-packet qdisc) moves nothing. The starvation is the
plain-window ARQ/retention pipeline under contention tail-drop (the
single-path 1024 outstanding pool × a frontier frozen by drop bursts:
goodput ≈ pool/dwell ≈ 2.5 Mbit — Little's law), a transport mechanism
BELOW the CC policy surface; BBR-under passes 22 Mbit through the same
pipeline only by holding ~250 packets (305–316 ms) resident in the shared
queue. The competitive-mode deployment gap is therefore closed IN CODE
but the shared-clean-bottleneck deployment gap is re-attributed to the
contention-recovery pipeline (the successor roadmap item).

### 12.5 CC + Taper: The Complete Architecture

```
  Copa determines: total_rate (symbols/sec on the wire)
  Taper determines: r* (correction symbols per source symbol)

  source_rate     = total_rate / (1 + r*)
  correction_rate = total_rate x r* / (1 + r*)

  When channel worsens: e rises -> r* rises -> source_rate falls
  When congestion:      dq rises -> total_rate falls -> both fall
  When channel clears:  e drops -> r* drops -> source_rate rises
```

The two controllers are orthogonal:
- Copa controls HOW FAST (total symbols per second)
- Taper controls HOW MUCH redundancy (correction per source ratio)

Neither needs to know about the other. Copa sees total traffic; taper sees
loss rate. They compose naturally.

**Bulk operating point.** Under the Bulk hint the taper side degenerates
by design: the tail target is "late is fine" weighted by completion
exposure (δ_bulk = ε̂ + (0.05 − ε̂)·χ, see Sections 5.3, 14.26), so the
continuous r* is 0 identically mid-stream (χ = 0) and the steady state is
pure ARQ — source_rate ≈ total_rate, wire volume at parity with a
retransmission transport. FEC reappears only near a known end of stream,
where χ ramps to 1 and recovery can no longer overlap sending
(Sections 14.25, 14.26).

### 12.6 BtlBw-Anchored Recovery

Copa-lite's steady-state recovery is additive: after a ×0.92 delay
backoff, cwnd climbs at +2/SRTT. From a trough at half the pipe that is
dozens of SRTTs of under-utilization. Measured at L1 C2 (100 Mbit, 5 ms
one-way, ±3 ms jitter, 1.3% loss): cwnd p50 sat at ~80-110 symbols against
a BDP of ~160 — the additive climb never caught up before the next
jitter-driven backoff (the residual ~30% backoff rate the §12.4
jitter-robust signal could only halve).

BBR does not crawl: it holds an explicit operating point BtlBw × RTprop
and returns *to* it [BBR-Queue]. We already track both quantities — a
delivery-rate max-filter `max_bw` and the 10 s min-RTT floor `min_rtt`
(§12.4) — but they fed only the diagnostic rate formula. This section
promotes their product to an active recovery anchor, keeping the
continuous, phase-free character of Copa-lite (no Startup/Drain/ProbeBW
state machine, no ProbeRTT drain — §12.3's taper-compatibility argument is
preserved because nothing here drains the pipe).

**The estimator.**

```
  BtlBw  = max_bw  = windowed MAX of per-update delivery-rate samples
                     (symbols/s), 10 s window
  RTprop = min_rtt = windowed MIN of RTT samples (s), 10 s window
  BDP    = BtlBw × RTprop                              (symbols)
```

Units check: (symbols/s) × s = symbols — the in-flight the bottleneck
rate keeps outstanding over one propagation RTT, i.e. a congestion window.

**The continuous anchor pull.** In the steady-state (post-first-backoff)
per-SRTT update, the additive step is replaced by a proportional pull
toward the BDP target that decays into the additive probe as cwnd
approaches it:

```
  target = cwnd_gain · BDP                             (cwnd_gain = 1.0)
  cwnd  += max( ADDITIVE_STEP, α · (target − cwnd) )   when cwnd < target
  cwnd  +=      ADDITIVE_STEP                           when cwnd ≥ target
```

with α = 0.25. This is a first-order relaxation toward the operating
point: from a trough at 0.5·BDP it closes ~90% of the gap in ~8 SRTTs
versus ~40 for +2, and the pull term vanishes smoothly into the gentle +2
probe as cwnd → target (no discontinuity, no phase counter). cwnd_gain is
1.0, not BBR's ProbeBW cwnd_gain = 2: BBR sizes cwnd to 2·BDP so it keeps
sending through one RTT of delayed ACKs and deliberately tolerates a
1·BDP standing queue [CACM]; here the standing queue ABOVE BDP is still
governed by the §12.4 hint-coupled delay backoff, so the anchor only needs
to restore the pipe, not to buffer a second one.

**Why it is a FLOOR, not a cap — the app-limited caveat.** BBR's BtlBw is
trustworthy only because of two mechanisms we do not have: **per-packet**
delivery-rate sampling, and **app-limited detection** that *discards*
samples taken while the sender was application- (or window-) limited,
because such samples measure the application's send rate, not the
bottleneck's [BBR-Draft, delivery-rate-estimation]. Our `record_delivery`
divides coarse ACK-batch counts by wall-clock elapsed and has no
app-limited flag. For a warm-up-limited transfer — the dominant regime for
a 1.8 MB object, which spends much of its life in inner-flow slow-start —
`max_bw` reads LOW exactly when we would want it high. A BtlBw used as a
*cap* would then throttle a flow that could have gone faster; a BtlBw used
only as a *floor* can, at worst, fail to lift cwnd — it can never suppress
it. So the anchor is applied strictly to RAISE cwnd:

- the recovery pull only ever increases the additive step (never below
  +2);
- an explicit floor `cwnd ≥ ANCHOR_FLOOR_GAIN · BDP` ratchets cwnd up
  after any backoff, bounded above by the hard cwnd ceiling.

Both are gated: the anchor is `None` until the delivery-rate window holds
≥ 8 samples AND a min-RTT sample exists (a handful of coarse samples is
too noisy to steer cwnd; before an RTT floor there is no RTprop). A
stale or over-read `max_bw` (ACK aggregation can momentarily inflate a
batch rate, the mirror risk to under-read) is bounded by
`ANCHOR_FLOOR_GAIN < 1` and by the delay backoff retaining authority above
the floor.

**Honest constants (and how ANCHOR_FLOOR_GAIN was set).**

| Symbol | Value | Meaning |
|--------|------:|---------|
| `ANCHOR_MIN_SAMPLES` | 8 | delivery samples before the anchor is trusted |
| `ANCHOR_RECOVERY_GAIN` (cwnd_gain) | 1.0 | pull target = 1·BDP |
| `ANCHOR_PULL_ALPHA` (α) | 0.25 | relaxation rate toward the target per SRTT |
| `ANCHOR_FLOOR_GAIN` | 0.85 | floor = 0.85·BDP |

The floor gain started at 1.0 (floor = full BDP) and was corrected to 0.85
by L1 measurement. At 1.0 the floor pinned cwnd exactly at the BDP
estimate even when the delay signal reported queue-above-target — the C2
cwnd trace showed `above=true` on ~100% of updates with cwnd held at
bdp_anchor, i.e. the floor was *maintaining* a ~16 ms standing queue the
backoff could no longer drain. 0.85 leaves the §12.4 delay backoff ~15%
of authority around BDP: the above-target update fraction fell to ~52% (the
queue drains on roughly half the updates), while the recovery pull still
re-fills toward full BDP each clean update, so cwnd oscillates just under
the pipe instead of sitting in standing bufferbloat.

**Interaction with the taper.** None — this changes only the cwnd
trajectory, not the correction ratio r*, and introduces no drain phase, so
the §12.5 taper coverage stays continuous (the reason §12.3 rejected BBR's
ProbeRTT does not apply). The two controllers remain orthogonal (§12.5):
Copa-lite-with-anchor sets HOW FAST, the taper sets HOW MUCH redundancy.

**Measured result — mechanism confirmed, completion refuted.** L1
rp-native (`perf`, no inner TCP, 1.8 MB, seed 42, 10 runs/arm) at C2:
cwnd p50 rose from the ~80-110 baseline to **139** (peaks 165, bdp_anchor
p50 137) — cwnd now reaches BDP as intended. But the C2 median completion
was flat: 0.883 s baseline → 0.911 s anchored, inside one run-to-run
standard deviation (~0.2 s), and each aggressiveness step moved the median
<10% (converged). C3 was flat within its larger variance. This matches the
bounded-leverage prediction that motivated the change
(`docs/research/bbr-lessons.md` #1): the 1.8 MB completion is dominated by
the tunnel-pipeline / inner-flow warm-up term (L2 ws3, fair-geometry), and
the delay backoff binds only 8-24% of ACKs, so re-filling the window to
BDP does not move that headline. The change fixes a genuine, measured cwnd
deficiency and is retained for that reason — its expected payoff is the
sustained-throughput and multipath-aggregation cells, where `B_eff` reads
cwnd/SRTT directly (§13.5) — but the completion improvement is a
**refutation**, recorded honestly alongside P10a (`docs/goal-gate.md`,
P-CC).

### 12.7 ECN as Opportunistic Enhancement

If the network path supports ECN [RFC3168], congestion is signaled by router
marking (CE bit) instead of dropping. This provides:
- Congestion detection without loss -> even better for delay-based CC
- Positive identification: marked = congestion, dropped = channel loss
- No need to distinguish via RTT trends (direct signal)

QUIC validates ECN support at connection startup. If supported, use it.
If not (common on wireless), fall back to Copa's delay-based detection.

### 12.8 Application Back-Pressure

When r* is so high that source_rate = total_rate/(1+r*) drops below the
application's minimum (e.g., VoIP codec needs 64kbps), the system signals
back-pressure: "the channel cannot support your required quality at this
loss rate."

The application must respond:
- Reduce quality (lower bitrate codec, lower video resolution)
- Accept lower reliability (increase d, decrease p)
- Wait for better conditions

This is a resource allocation problem, not a CC problem. The CC works
correctly; it's the application that needs to adapt.

**Back-pressure mechanism:** The sender stops reading from the source (TUN
interface, application socket) when the retransmit buffer reaches buffer_max.
This causes the kernel's write buffer to fill, which causes the application's
write() to block — identical to TCP behavior when the send buffer is full.

No special API is needed for basic operation: applications using the TUN
interface experience standard blocking write() semantics. Data is accepted
when the channel can handle it and blocked when it can't.

**Optional stats API (for advanced applications):** Applications that want
finer control can subscribe to channel statistics:
- Per-path: ε, RTT, throughput, correction rate r, Copa rate
- Aggregate: effective reliability ρ, tail latency δ, correction deficit
- Events: backpressure start/end, path added/removed, regime change

The API also allows overriding protocol characteristics mid-connection:
- Adjust δ (tail latency target) or ρ (reliability target)
- Change T_cut (affects eviction and gap pruning)
- Force path preferences

This API is implementation-specific and not part of the core model.

### 12.9 Repair-wait / FEC-before-ARQ — a receiver-side discipline, MEASURED and refuted (2026-07-08)

A natural refinement of the reactive/proactive interplay (§5.4, §12.6) is the
classic FEC discipline: on a frontier hole, **wait for the covering proactive
repair to arrive+decode before falling back to a reactive NACK** — a
repair-coverage horizon ≈ one generation-span (NOT an RTT), so a hole proactive
FEC would cover does not eat a redundant round-trip. This was implemented
(`horizon_gate_deficits`, env `RWM_REPAIR_WAIT`, δ-aware clamp ≤ ½·SRTT) and
measured at L1 to test whether it raises the proactive-recovery fraction toward 1.

**It does not.** At RTT 100 / 10% loss the proactive fraction FALLS as the horizon
grows (0.27 → 0.22 over 0–48 ms) and `recovery_coded` rises; at 2.6% it moves only
marginally and noisily; throughput never improves. `FDIAG` isolates the reason:
when the frontier stalls, a covering proactive equation is **never present**
(`present_at_stall = 0`; `repairs_useful ≈ 7 / repairs_fed ≈ 4600`). The proactive
repair is not late — it is **absent** (dropped or linearly useless on the
droppable-datagram substrate). The reactive fraction is therefore governed by how
often a generation's proactive budget is lost on the wire (a **substrate**
property), not by *when* the receiver emits the NACK. This positively excludes the
"NACK-fires-too-early" alternative to the substrate root cause: like raising r*
(§8.4), delaying the NACK cannot recover coverage the substrate never delivered.
The knob is retained env-gated + default-off; it helps only where the proactive
fraction is already high (low RTT / low loss), where FEC does not need help.

### 12.10 SACK sender-decoupling + BDP reassembly — the in-order frontier bound is RECOVERY-LATENCY, not backpressure (2026-07-08)

The §12.8 back-pressure mechanism ("sender stops reading the source when the
retransmit buffer reaches buffer_max") gates on the CONTIGUOUS in-order
cumulative-ack frontier: the sent-store drains only for seqs at-or-below the
frontier, so a single hole freezes the frontier, pins the store full, and stalls
source intake — goodput collapses to ≈ window/RTT (the ~16 Mbit C2 single-path
lossy ceiling, §14.7). This section tests the natural fix and reports the result.

**The fix (two composed pieces, both env-gated, default-off).**
*Sender* (`RWM_SACK_PRUNE`): SACK-based flow control — prune the sent-store (and
the per-seq ARQ maps) for ANY out-of-order-received (SACKed) seq, so the store
tracks TRUE outstanding-unacked and the send window stays BDP-full ACROSS a hole
(the hole itself stays retained and recovers via the orthogonal NACK/tail-sweep).
*Receiver* (`RWM_REASM_BDP`): a reliability guard clamping the decoder/received-seq
prune so it can never advance ABOVE the delivered frontier (the reorder buffer is
already non-evicting), so a symbol the sender has SACK-pruned is NEVER evicted at
the receiver before its frontier passes — the invariant the prior SACK attempt
violated. An occupancy probe (`[REASM]`) reports the held-behind-frontier symbols.

**MEASURED (L1, netem, 50 MB × 3, seed 42).**
- **Single-path C2 (~2.5 % GE loss): NO throughput lift.** baseline (gate off)
  **16.54 Mbit/s** → SACK+REASM (in-order) **17.09** → +OOO completion **17.22** —
  all within the ~5 % run-to-run stdev. Decoupling the sender buys nothing because
  the sender was never the bottleneck: throughput is store-cap-invariant (§14.7),
  and completion still waits for the in-order frontier to walk each hole at ≈ 1
  ARQ round / RTT. **The bound is receiver-side RECOVERY LATENCY, not sender
  backpressure.**
- **The reliability invariant HOLDS on single-path.** dnf 0 on every arm; the
  reassembly occupancy stays BOUNDED at ≈ BDP — peak held-behind-frontier = 1541–
  1888 symbols out of a ~50 000-symbol object as the frontier advances — every byte
  delivered. This fixes the #52 SACK break (which evicted a pruned-but-unconsumed
  symbol); the composed guard makes the decoupling safe.
- **Heterogeneous dual (C8, c2+c3): the BDP bound FAILS and it stalls.** With the
  sender decoupled, it races the FAST path ahead while the SLOW path's frontier
  hole lingers ≈ its (larger) RTT; the dual store cap = gain · Σ BtlBw×RTprop sums
  BOTH paths' anchors (slow-path RTT-inflated), so outstanding is NOT bounded to
  the fast path's BDP → the receiver reassembly grows toward the WHOLE object
  (`max_pending` 38 820 / ~50 000 ≈ 78 %) and bufferbloat stalls the transfer
  (single rep did not complete in 300 s vs baseline ~37 s). SACK+REASM makes C8
  strictly WORSE than the plain baseline (10.86 Mbit/s, 0.66× fast-alone); the
  >15.7 factor > 1 aggregation bar is not crossed.

**Verdict.** Sender-side SACK flow control is safe (with the BDP reassembly guard)
but is NOT the fix for the lossy-throughput collapse: the in-order cumulative-ack
frontier's serialization is a RECOVERY-LATENCY bound, structural to reliable
in-order-capable delivery on the droppable-datagram substrate, unmoved by any
sender flow-control law — consistent with §14.7 and the six prior L1 investigations.
On heterogeneous multipath the decoupling actively unbounds the receiver buffer
(the slow path's RTT-inflated BDP anchor defeats the store cap), so it regresses
C8. Closing the collapse still needs the transport-pipeline change named in §14.7
(pipelined per-RTT frontier recovery, or a genuinely rateless ack-frontier where a
hole is never a fixed in-order position) plus a per-path (not summed) outstanding
cap. Both knobs are retained env-gated + default-off; the shipped path is untouched.

### 12.11 Substrate CC is POLICY: the pass-through window and Copa-sole ownership (2026-07-13, measured)

> **Correction to §12.2.** "QUIC datagrams bypass Quinn's CC" is FALSE on the
> send side: quinn gates EVERY packet send — DATAGRAM frames included — on its
> congestion window (§16.17 named this as the generation substrate ceiling, and
> the same lever exposed that plain mode's 15–17 Mbit/s "link ceiling" was
> quinn's stock Cubic, not the link: plain+BBR = 76). The engine's CC was never
> the sole rate limiter; the effective window was min(app CC, quinn CC) — a
> hidden loss-reactive controller UNDERNEATH a loss-tolerant FEC transport.

With that finding, the substrate controller becomes an explicit POLICY surface
(`RWM_QUIC_CC`, default UNSET = stock Cubic, byte-identical):

```
  cubic | newreno   loss-reactive stock controllers (the old hidden default)
  bbr               quinn's model-based controller (§16.17's ×3.4 lever)
  passthrough       quinn's controller is a pass-through shim: window() reads
                    a per-path atomic OUR engine writes — the raptorpath
                    Copa-lite per-path cwnd (per connection = per path) IS the
                    substrate congestion window. Loss events are recorded,
                    never acted on (loss is FEC's job, §12.1; congestion
                    safety is Copa's delay backoff); quinn's pacer derives
                    from the window, so the wire is paced at Copa's cwnd/RTT.
```

Copa-sole rationale (vs leaving BBR underneath): (a) **the δ-triangle
mapping** — Copa's δ is our hint-coupled queue target (§12.4/P1:
1.08/1.125/1.25), so the substrate's operating point inherits the stream's
declared latency/throughput profile instead of a fixed BBR gain; (b) **no
phases** — Copa's natural oscillation drains its own standing queue within
~5 RTT continuously, where BBR's ProbeRTT is a forced 200 ms drain (a FEC
protection gap, §12.3) and ProbeBW overshoots by design; (c) **one
controller, one signal path** — min()-coupling two independent controllers
makes the tighter one the binder in an uncontrolled way (§16.17's wall was
exactly that).

Prerequisite (the plain-mode feeding fix, and a code-fact correction): plain
window-reliable mode was believed to leave Copa delivery-blind; in fact the
receiver's per-batch Ack fires in window mode too, so Copa was fed the
ACK-INTERVAL Δt estimator all along — whose windowed max over-reads ~×10
under ack bunching (measured: 108 k vs true 10.4 k sym/s on the c2-shaped L0
shim), pinning cwnd and the plain outstanding cap via the anchor floor:
bufferbloat by estimator. The Copa-sole feed replaces it with BBR-correct
SEND-interval rate samples (the §16.13 machinery) attributed per path from
each WindowAck's cumulative-frontier/SACK diff, suppresses the legacy
samples, and re-keys the plain outstanding cap to gain×Σcwnd (with honest
samples an anchor-keyed cap is circular — samples can never read above the
cap they themselves set).

Measured (L1 plain mode, 25 MB ×8 ×2 seeds, arms interleaved; full tables in
goal-gate "Copa-Sole Substrate CC"): Copa-sole does NOT reach BBR-under's
bulk throughput — single-c2 28.9/31.2 vs 75.9/75.4 (0.4×), C7 57.1/51.1 vs
96.7/99.9, C8 28.4/29.4 vs 54.5/52.9 — the named mechanism being Copa's own
delay-targeting (it equilibrates perceived queue at the hint target while BBR
runs 2×BDP and eats the queue) compounded by the app-layer echo-RTT reading
the sender's own store reservoir as queue (shrinking the reservoir from
2×cwnd to 1.25×cwnd raised throughput +13–23 % AND tightened the queue).
What Copa-sole DELIVERS: a 3–6× tighter standing queue than BBR-under in
every cell with dramatically better tails (single-c2 p90 78/38 ms vs 512 ms;
C8 slow-path p90 321 ms vs 2 474 ms — no ProbeBW overshoot, no
ProbeRTT-class stalls), elimination of plain-BBR's c3 bimodal collapse mode
(σ 6.5 → 0.63, zero collapse runs), and near-perfect symmetric aggregation
relative to its own single (C7 = ×1.98). The rate is genuinely OWNED: the
substrate window tracked Copa's cwnd end to end with GE loss present and no
loss-reactive collapse.

Deployment caveat: Copa-lite has no TCP-competitive mode (Copa §4 mode
switching not built — deliberately out of scope here); against loss-based
cross-traffic on a shared bottleneck a delay-based controller yields, and no
cross-traffic cell was measured. `passthrough` is an experiment knob;
BBR-under remains the bulk-throughput reference/default-fallback and the
shipped default remains stock Cubic. **[UPDATE 2026-07-21: the shipped
default IS now BBR-under (goal-gate "Default CC Flip"; §17.2); stock Cubic
is the explicit `RWM_QUIC_CC=cubic` legacy arm.]** **[UPDATE 2026-07-19,
feat/copa-compete: BUILT and measured. The mechanism is the Copa paper's
§2.2 (the "Copa §4" reference above was imprecise); see the §12.4
competitive-mode addendum for the law and goal-gate "Copa Competitive Mode
+ Cross-Traffic" for the first shared-bottleneck battery.]** **[UPDATE
2026-07-22, feat/copa-sole-clean: the clean-substrate re-measure (goal-gate
"Copa-Sole on Clean Substrate") settled whether the consolidated stack
collapses the two-value CC surface to ONE δ-controller: it does NOT. On the
fixed substrate Copa-sole is copa/bbr 0.89× sc2, 0.97× sc3, 0.73× c7,
0.57× c8, 0.66× dc1 (≫σ both seeds) — the walls WIDENED the gap (they lift
BBR's aggregation while Copa's δ-equilibrium leaves the freed pipe on the
table), and the §12.11-era C8 domination inverted (it was a broken-
substrate artifact). Copa keeps the network standing queue ×18/×16/×6–7
tighter (sc2/sc3/c7) and ties BBR on the realtime tail. NO default flip;
the surface stays two-valued as a MEASURED TRADEOFF; the fusion (ADR-0068,
§17.6 item 10) inherits the bulk gap.]**

> **ADDENDUM (2026-07-13, feat/copa-wire-signal — the bulk gap CLOSED where
> it was named).** The §12.4 wire-signal fix (wire-clocked delay term +
> δ(hint) = 0.5/ζ mapping + Copa's actual velocity law + CC-rate pacing)
> re-ran this battery (v4, same discipline; goal-gate "Copa Wire-Signal"):
> Copa-sole bulk goes 0.40× → **0.86–0.89× BBR-under at single-c2** (68.1/64.3
> vs 76.5/75.1), **0.95–1.01× at C8** (55.0/55.3 vs 54.6/58.1 — parity, σ
> collapsed to 3.7/1.6), 0.73–0.76× at C7, 0.78× at sc3 — while the NETWORK
> standing queue stays 4–7 ms p50 at c2-class paths vs BBR's 38 ms and the
> C8 slow path holds 3–7 ms vs BBR's 88–124 ms (×18–25). C8 is the first
> cell where Copa-sole strictly DOMINATES BBR-under (≥ throughput, far
> tighter queue, lower variance). Arm D's reservoir term is RESOLVED: under
> the wire clock, shrinking the reservoir no longer buys throughput (−5%,
> less recovery runway) — the self-queue signal is gone. The measured δ
> frontier (0.05/0.005/0.001 at sc2) has its knee AT the hint-mapped Bulk
> δ = 0.005: tighter δ costs −38%, deeper δ buys ≈σ. Honest residuals: a
> pre-existing CROSS-ARM receiver-side frontier wedge (~2.2–3.3 Mbit
> collapse runs, ~60 s, self-resolving; B 2/59, C0 3/57, C1 7/59 — C1's
> larger operating point triggers it more often; forensics in the goal-gate
> section) is now the top blocker for a Copa-sole substrate default; C7
> aggregation (×1.11–1.23 of own single vs B's ×1.35) and the c3 cell
> (anchor ×4 over-read; deep tolerated queue by design) remain named gaps.

### 12.12 The substrate's OTHER control loops: the ~60 s "collapse run" was quinn's PMTU black-hole detector, not the frontier (2026-07-13, fix/frontier-wedge)

The §12.11-era "receiver-side frontier wedge" is root-caused and the lesson
generalizes §12.2/§16.17: the CC window was not the only hidden substrate
controller under the datagram path — PMTUD is another. Every wire symbol is
one ~1279-byte QUIC datagram, sendable only because post-handshake PMTUD
raises quinn's MTU above its 1200-byte default floor; on a GE-lossy bulk
wire, where essentially every packet is symbol-sized, a loss burst is
indistinguishable (to quinn's black-hole heuristic, which looks for lost
bursts containing no small packets) from an MTU black hole, so it resets the
MTU to 1200 and pauses discovery for its 60 s cooldown — during which every
symbol send, including every ARQ retransmit of the frontier blocker, fails
sender-side as `TooLarge` while small control datagrams keep the connection
looking healthy. That is the entire "collapse run": deterministic same-binary
repro 63.5 s vs 5.8 s (tests/mtu_blackhole_wedge.rs). The fix declares the
requirement the code always had: `min_mtu = initial_mtu = 1350`, so a
black-hole reset lands at a floor that still carries a symbol. The
state-machine lesson: a transport composed OVER another transport must
enumerate the substrate's autonomous control loops (CC, PMTUD, pacing, idle
timers) and pin every one whose failure mode silently violates an invariant
the overlay's design assumes (here: "a datagram that fit yesterday fits
today"); an unpinned substrate loop is a hidden state machine that will
eventually take a transition the overlay cannot observe — the overlay saw
only its symptom, 60 s of inexplicably inert retransmits, and the first
forensic hypothesis (receiver dup-filter) was wrong precisely because the
failing component was invisible from both endpoints' application state.

---

## 13. Multi-Path Scheduling

### 13.1 Why FEC Beats MPTCP

MPTCP (Multi-Path TCP) schedulers are fundamentally limited by **head-of-line
(HOL) blocking**: TCP requires in-order delivery, so a packet on the slow path
blocks all fast-path packets at the receiver. MPTCP schedulers (round-robin,
weighted, BLEST [Ferlin2016]) try to minimize this by avoiding slow paths,
but they can't eliminate it.

Our FEC-based model is fundamentally different:

```
  MPTCP:                          Raptorpath:

  Path A: [P1] [P2] [P5] [P6]     Path A: [S1] [C] [S3] [C]
  Path B: [P3] [P4]               Path B: [S2] [S4] [C]

  Receiver must wait for P3       Decoder needs ANY k of n symbols.
  before delivering P4,P5,P6.     Order doesn't matter.
  HOL blocking on slow path.      No HOL blocking.
```

The decoder doesn't care WHICH symbols arrive — just HOW MANY. A repair
symbol on Path A can recover a lost source on Path B. Source symbols can
arrive in any order. The reorder buffer handles sequencing after decode.

### 13.2 Per-Path Model

Each path i runs independently with its own:

```
  Copa_i:     rate_i = 1 / (d x dq_i)         total sending rate
  GE_i:       (e_i, p_i, q_i)                 loss model
  Taper_i:    tau_i(t) = A_i x (1-q_i)^t      correction density
  r_i:        correction rate = A_i / q_i     source/correction ratio
```

All paths share:
- **One source stream**: source symbols are distributed across paths
- **One retransmit buffer**: any path can retransmit any un-ACKed symbol
- **One FEC encoder window**: repair symbols cover the same source window

### 13.3 Unified Symbol Stream and Interleaving

Each path carries a **unified stream** of source + correction symbols,
interleaved at that path's own taper ratio. The interleaving is essential
for burst protection — corrections scattered among source symbols survive
bursts that wipe out consecutive symbols:

```
  Path A (e=0.05, r=0.05):  [S][S][S][S][C][S][S][S][S][C]...   5% corrections
  Path B (e=0.10, r=0.11):  [S][S][C][S][S][C][S][S][C]...     11% corrections

  During burst on Path A:   [S][X][X][X][C][S]...
                                 lost      ^ this correction survives!
                                             -> decoder can recover
```

**Source and corrections must NOT be separated between paths.** If source
goes on one path and corrections on another, the source path has no
interleaved corrections, and burst protection fails:

```
  BAD: source/correction separation
  Path A: [S][S][S][S][S][S]...  <- burst wipes everything, no protection
  Path B: [C][C][C][C][C][C]...  <- corrections exist but on wrong path
```

The scheduler distributes the combined stream across paths. Each path
independently interleaves at its own ratio.

### 13.4 Correction Deficit

The **correction deficit** is the total expected corrections still needed
across all paths:

```
  deficit = SUM_{s in un-ACKed} e_s

  where e_s = channel loss rate of path(s) at the time symbol s was sent
```

Each source or correction symbol sent on path i adds e_i to the deficit
(it might be lost). Each ACKed symbol removes its send-time e_s (confirmed
survived).

**Why e/(1-e) is a geometric series:** When a correction symbol is itself
lost, it needs replacement. This creates a chain:

```
  Source lost:               0.30   (30% loss)
  Corrections also lost:     0.30 x 0.30 = 0.09
  Replacements also lost:    0.30^3 = 0.027
  ...
  Total = 0.30 + 0.09 + 0.027 + ... = 0.30 / (1 - 0.30) = 0.4286
```

This is why r_IT = e/(1-e), not just e. The (1-e) denominator accounts
for the infinite chain of correction-of-correction loss. The deficit
counter captures this naturally: lost corrections add to the deficit,
generating more corrections, until enough survive.

**Cross-path correction deficit:** When source is on path A (e_A) and
corrections go on path B (e_B), the formula becomes:

```
  r = e_A / (1 - e_B)
```

This emerges from the deficit dynamics: source adds e_A, each surviving
correction on path B removes (1-e_B). Equilibrium: e_A = r x (1-e_B).

### 13.5 Effective Delivery Time and Bandwidth

For each path i:

```
  E_i     = RTT_i/2 + e_i x t_recovery_i       effective delivery time  [sec]
  B_eff_i = C_i / (1 + r_i)                    source-carrying capacity [sym/s]
  e_combined = SUM(C_i x e_i) / SUM(C_i)       throughput-weighted loss [prob]
```

where t_recovery_i = P_fec_i x t_fec_i + (1 - P_fec_i) x L_arq_i is the
expected recovery time on path i if the symbol is lost. t_fec_i is the FEC
recovery time (Section 3.4) and L_arq_i is the ARQ recovery latency on
that path. The delay spectrum concept [Facenda2022] provides related
per-link rate allocation theory for multi-path streaming.

### 13.6 Scheduler Ratio Adjustment

The scheduler can adjust the per-path source/correction ratio to favor
certain paths for source symbols. This is only beneficial for
**latency-sensitive** traffic (source symbols are immediately processable,
corrections are not — so putting more source on a fast path saves latency):

```
  Default (natural taper):
    Path A (fast, e=0.05):  [S][S][S][S][C][S][S][S][S][C]...   r=0.05

  Latency-optimized (scheduler shifts source to fast path):
    Path A (fast):          [S][S][S][S][S][S][S][S][S][C]...   r'=0.02
    Path B (slow):          [S][C][C][S][C][C][S][C][C]...      r'=0.30
                                                                (absorbs deficit)
```

For **bandwidth-optimized** traffic: no adjustment needed. Source and
corrections cost the same bandwidth. The natural taper ratio is optimal.

**Constraint:** Each path must maintain minimum interleaving for burst
protection. The scheduler cannot reduce the correction ratio below a
floor that leaves the path unprotected during bursts.

### 13.7 Burst Protection During Ratio Adjustment

When the scheduler reduces a path's correction ratio (more source, fewer
corrections), that path has fewer same-path corrections to survive a burst.
But burst protection is already handled by two existing mechanisms:

**Global correction deficit (Section 13.4):** When one path's corrections
are reduced, the deficit grows — and other paths absorb it by generating
more corrections. The total correction budget across all paths remains
matched to the total loss. No per-path floor is needed because the
correction budget is global, not per-path.

**Cross-path diversity (Section 13.10):** Corrections on the slow path
survive bursts on the fast path (different channel, different burst
pattern). A burst on path A is covered by corrections from path B.
P(both paths burst simultaneously) = ε_A × ε_B — negligible for
independent paths.

No correction floor (r_min) is needed. The global deficit and cross-path
diversity provide burst protection without any per-path minimum. The
taper self-corrects via BOCD if loss increases, and P_lost(t) naturally
produces Mehrotra's optimal policy (repair during uncertainty, retransmit
after confirmation).

### 13.8 Interpolated Objective Function

One parameterized objective with weights from the protocol hint:

```
  minimize: w_lat x SUM(x_i x E_i) + w_bw x SUM(x_i x r_i)
             ^                          ^
        latency cost               bandwidth overhead cost

  subject to: SUM(x_i) = 1                  all source distributed
              x_i x source_rate <= B_eff_i  per-path capacity
```

```
  Realtime:   w_lat = 1.0,  w_bw = 0.0   minimize latency at any bandwidth cost
  Balanced:   w_lat = 0.5,  w_bw = 0.5   balance latency and bandwidth
  Bulk:       w_lat = 0.0,  w_bw = 1.0   minimize bandwidth waste (overhead)
```

**In-order delivery coupling (L2 refinement — measured, asymmetric
paths).** The objective above assumes per-symbol delivery costs are
independent: latency composes linearly in the allocation x_i. Two
delivery mechanisms break that assumption:

1. **Block decode coupling.** A block decodes only when enough of its
   symbols have arrived across ALL the paths it was striped over. Its
   completion time is not a weighted sum but a max:

   ```
     T_blk(b) = max_{i : b_i > 0} ( b_i/C_i + E_i )

     b_i = the block's symbols on path i (serialization + delivery)
   ```

   Any nonzero share on the slowest path makes the WHOLE block pay that
   path's delivery time — and losses in that share recover at that
   path's RTT (its FEC/ARQ round), not the fast path's.

2. **Cross-block in-order delivery.** Decoded blocks are released in
   block-id order (the delivery contract that shields an inner TCP from
   tunnel-induced reordering; hold H = 4 x SRTT_max, clamped). A slow
   block at the head of the sequence delays every faster block behind
   it, and a block slower than H is force-delivered as a HOLE in the
   inner byte stream — the latency skew converts into inner
   retransmissions and cwnd collapse, i.e. into THROUGHPUT loss. This
   is how Bulk (w_bw = 1), which nominally ignores latency, still pays
   for latency skew: under in-order delivery there is no allocation
   whose latency cost is free.

Measured at L1 C8 (path A 100 Mbit / 10 ms RTT / eps 2.5%, path B
20 Mbit / 40 ms RTT / eps 4.8%, bulk 50 MB, per-symbol striping):
27% of source landed on B, 15% of blocks striped across both paths;
striped blocks completed at mean 189 ms vs 17.5 ms for A-only blocks
(p50 131 vs 13 ms); 92% of in-order head-of-line waits were caused by
blocks touching B; 151 holds per 100 MB expired at the 300 ms cap and
were force-delivered as holes. Aggregate 8.8 Mbit/s — BELOW the fast
path alone (14.0). The linear objective cannot see any of this.

**Refinement: allocation granularity must match the delivery unit.**
Realize x_i per BLOCK, not per symbol: block k rides one path, and y_i
is the long-run fraction of blocks assigned to path i. Per block of K
symbols the delivery time on path i is D_i = K/C_i + E_i, and:

```
  striping:        EVERY block pays   max_i (b_i/C_i + E_i)
  block-granular:  y_i of blocks pay  D_i        (their own path only)
```

The block delivery time needs the per-BLOCK recovery term, not the
per-symbol one: a block of K symbols on a path with loss eps needs a
recovery round at THAT path's RTT with probability

```
  P_blk = 1 - (1-eps)^K        (~1 for realistic K: K=56, eps=4.8% -> 0.94)

  D_i = K/C_i + RTT_i/2 + P_blk x 2 x RTT_i
```

The per-symbol E_i (Section 13.5) undercounts this by roughly an order
of magnitude (measured C8: E_B = 22 ms vs B-blocks actually completing
at p50 94 ms). The interpolated objective keeps its form, evaluated per
delivery unit:

```
  minimize: w_lat x SUM(y_i x D_i) + w_bw x SUM(y_i x r_i)

  subject to: SUM(y_i) = 1
              y_i x block_rate <= B_eff_i / K            per-path capacity
              D_i - min_j D_j <= H/4  for all y_i > 0    in-order hold horizon
```

The third constraint is the in-order coupling term: a path whose block
delivery time exceeds the fastest path's by more than the reorder hold
budget cannot carry source at all — its blocks would be force-delivered
as holes — but it remains fully useful for corrections and cross-path
retransmit (Section 13.10), which have no ordering deadline (this also
keeps its estimators warm for re-admission when it improves). The H/4
factor maps the MEDIAN estimate D_i onto the TAIL event the constraint
guards against: an expiry fires when a single block exceeds H, ARQ
rounds stack the delivery tail to ~3-4x the median (measured C8:
median 134 ms with expiries at 301+ ms), so a median skew above H/4
already pushes the tail past the horizon. For Bulk the solution is y_i
proportional to B_eff_i over the feasible paths (fill capacities at
block granularity); for Realtime it degenerates to the lowest-D_i path
with capacity spill. Note this does NOT violate the Section 13.3 rule
(source and corrections must not be separated per path): correction
placement is unchanged, and burst protection on a source-carrying path
is provided by cross-path diversity (Sections 13.7, 13.10).

The production scheduler realizes y_i with a smooth weighted
round-robin on B_eff_i (Copa pacing rate deflated by 1 + r_i): the
long-run shares converge to the capacity split while consecutive
blocks alternate paths as evenly as the weights allow, minimizing the
skew the reorder buffer must absorb.

**Measured after the refinement (same C8, one arm at a time):**
block-granular affinity alone lifted 8.8 -> 11.4 Mbit/s (striped
blocks 15% -> 0, but B-blocks still delivered at p50 94-134 ms — the
ARQ round at B's own RTT); adding the D_i eligibility constraint
starved B of source (6% residual, estimator warm-up only) and reached
12.6 Mbit/s on 50 MB bulk and cut 1.8 MB object median completion
from 3.07 s to 1.15 s (2.7x). Hold expiries fell 151 -> 96 per
100 MB. Symmetric C7 is unaffected (23.3 vs 23.9). At these
parameters the model's optimum is fast-path source + slow-path
corrections; the ~10% residual gap to the fast path alone (14.0) is
warm-up admission plus tail expiries. Kernel MPTCP aggregates beyond
its own single path here (12.6 vs 10.6) because its receiver absorbs
cross-subflow reordering inside one sequence space — an option the
tunnel's inner-TCP in-order delivery contract forecloses.

### 13.9 QoS Priority Cascade

When multiple protocol classes share the same paths, they pick in priority
order from tightest to loosest latency requirement:

```
  1. Realtime picks first:  lowest E_i paths, up to its source volume
  2. Balanced picks next:   best remaining path capacity
  3. Bulk gets the rest:    whatever capacity remains
```

### 13.10 Cross-Path Retransmit

The shared retransmit buffer enables recovery across paths. When source
symbol S_k was sent on path A and lost, any path's correction slot can
retransmit it:

```
  P(retransmit on path j) = P_lost(t_k, e_A)
```

Cross-path diversity: P(both fail) = e_A x e_j. For e_A=0.10, e_j=0.02:
P(both fail) = 0.002 — 50x improvement over single-path.

**No explicit path routing needed.** Cross-path retransmit emerges from the
shared buffer. Each path independently generates correction symbols from its
taper. When a correction slot produces a retransmit (via P_lost), it pulls
from the shared buffer — which may contain symbols from any path. P_lost
uses per-path ε, so symbols from lossier paths have higher P_lost and are
retransmitted first.

**Work-stealing analogy.** Paths with spare Copa capacity (typically low-loss
paths whose tapers need fewer corrections) have room to pull retransmits
from the shared pool — like idle threads stealing work. High-loss paths are
busy with their own corrections. This naturally routes retransmits through
reliable paths without explicit selection.

**Potential refinement:** For latency-sensitive traffic, weighting retransmit
pulls toward the lowest-RTT path would reduce recovery time. This follows
the same interpolated objective as source scheduling (Section 13.8).

---

## 14. Future Directions and Considered Improvements

The following sections document insights discovered through building and
testing the interactive visualizer simulation. They represent deeper
considerations about how FEC latency, ARQ latency, bandwidth, and the
encoder window interact — areas where the current model can be refined
in future work.

### 14.1 Reconsidering the Triangle: Bandwidth as Constraint, Not Variable

The current triangle (Section 1.4) treats r (bandwidth overhead) as a
variable alongside δ (tail latency) and ρ (reliability). But Copa
(Section 12) discovers a fixed link capacity C. We cannot create more
bandwidth — we can only choose how to fill the pipe. Every FEC symbol
displaces a source symbol:

```
  source_rate = C / (1 + r)

  More FEC (higher r) → faster per-window recovery → but slower completion
```

This means "bandwidth overhead" is not an independent dimension — it IS
long-window latency at a fixed link rate. The real trade-off space may be:

```
         Short-window latency
         (per-window tail delivery)
              / \
             /   \
            / FIX \
           / any 2 \
          /         \
         /___________\
Long-window         Reliability
latency              (ρ)
(throughput/
completion time)
```

r is the internal mechanism that trades short-window for long-window
latency. The user sets latency targets; the system computes r. This
reframing may lead to more intuitive protocol hint mappings.

### 14.2 FEC Latency Is Not Zero

Section 3.2 states "Latency cost: zero additional (repair arrives at
roughly the same time as source)." This is an approximation. FEC repair
symbols arrive AFTER the source symbols they cover. A lost symbol S3 is
only recovered when enough repairs covering S3 have arrived AND the
decoder resolves S3's equation. This takes time t_fec > 0.

**Per-symbol:** FEC can never make a single symbol arrive faster than if
it wasn't lost. The repair always arrives after the source it covers.

**Per-window:** FEC can recover all losses within a window once enough
repairs accumulate. The decode latency depends on the encoder window
size W and the correction rate r.

The distinction matters for real-time applications where per-symbol
latency is the constraint. For file transfers, only per-window/total
completion time matters, and the approximation is acceptable.

### 14.3 Unified FEC Latency Distribution

For m concurrent losses (a burst of length m), the time until all m are
recovered follows a counting process. Every repair covers the entire encoder
window (Section 3.2), so the useful-equation arrival rate after a loss is
the AGGREGATE correction rate — corrections occupy a fraction r/(1+r) of
wire slots, each surviving with probability (1-ε) — not the lost symbol's
own taper density. Using the Poisson approximation (valid when per-slot
correction probabilities are small):

```
  λ(T) = r × (1-ε) × T / (1+r)          T in wire slots after the loss

  P(t_fec ≤ T | m) = P(Poisson(λ(T)) ≥ m)
                    = 1 - Σ_{k=0}^{m-1} e^{-λ(T)} × λ(T)^k / k!
```

λ(T) grows linearly until the window slides past the lost symbol — after
W further source symbols, i.e. T ≈ W(1+r) wire slots — at which point
λ = rW(1-ε), exactly the per-window Binomial mean of Section 8.2.

(An earlier formulation summed only the lost symbol's own taper,
λ(T) = A(1-ε)(1-(1-q)^(T+1))/q, which saturates at r(1-ε) < 1 and wrongly
predicts that even a single loss is usually unrecoverable — the same
per-symbol undercount that invalidates Appendix E.)

This is the regularized incomplete gamma function Q(m, λ(T)).

**Unified across codec types:**

- Block codec (RaptorQ, RS): m = losses in block of size K. All m
  symbols decode simultaneously when the m-th repair arrives.
- Window codec (RLC, Streaming): m = losses in window of size W.
  Decoder can cascade — recovering one symbol may immediately resolve
  others via Gaussian elimination. The block CDF is a conservative
  lower bound (window codecs recover faster due to cascade).

### 14.4 Ambient FEC and the Pipeline Effect

With a sliding window codec, repair symbols are generated continuously
as part of the steady-state r/(1+r) interleaving (Section 5.3). When a
loss occurs, there are already repair symbols "in the pipeline" that
cover the lost position — they were generated while the lost symbol was
in the encoder window.

```
  Window:  [S1][S2]...[S50][R1][S51][R2]...[S100][R3]...
                         ^          ^
                         S50 lost   R1 already covers [S1..S100]
                                    → decoder has one equation instantly
```

For a symbol that has been in the window for T_w ticks before loss:

```
  λ_total(T) = λ_prior(T_w) + λ_new(T)

  λ_prior(T_w) = accumulated surviving repairs from before the loss
               = r × (1-ε) × T_w / (1+r)
  λ_new(T)     = new corrections generated after the loss (Section 14.3)
               = r × (1-ε) × T / (1+r)
```

**Larger windows accumulate more ambient FEC → faster recovery.** This
means FEC latency is not just about new repairs generated after the
loss — it includes the pipeline of repairs already in flight.

### 14.5 Optimal Encoder Window Size

The window should be large enough that ambient FEC covers typical bursts.
For a burst of length B, we need B surviving repairs in the pipeline:

```
  W × r × (1-ε) / (1+r) ≥ B

  W_min(B) = B × (1+r) / (r × (1-ε))
```

With r = ε/(1-ε): r(1-ε) = ε and 1+r = 1/(1-ε), so W_min(B) = B/(ε(1-ε)).
For the mean burst (B = 1/q), dropping the small (1-ε) correction:

```
  W_min = 1 / (q × ε × (1-ε)) ≈ 1 / (q × ε)
```

Using the Section 2.4 scenario parameters (W_min ≈ B/ε):

```
  Scenario          ε      q     W_min(mean)   B_99   W_min(p99)
  ---------------------------------------------------------------
  WiFi            2.5%   0.50        80          7       ~280
  LTE               5%   0.40        50         10       ~200
  Satellite         9%   0.30        37         13       ~144
```

Note the two opposing effects: longer bursts (small q) raise W_min via B,
but higher loss ALSO means more repairs per window at r = ε/(1-ε), which
lowers it — so satellite (high ε) needs a smaller minimum window than
WiFi (low ε) despite its longer bursts.

Setting W so that W × t_sym ≈ RTT gives FEC and ARQ roughly equal
recovery latency. Below that threshold, FEC is strictly faster than ARQ
for all burst lengths up to the pipeline capacity.

### 14.6 ARQ Latency and the Retransmit Sweet Spot

ARQ latency is well-defined: L_arq = T_retx + RTT/2 ≈ 1.5 × RTT.
But T_retx depends on confidence that the symbol is actually lost.
P_lost(t) (Section 3.4) models this confidence.

As T_retx shrinks (retransmit sooner):

- ↑ Faster recovery for truly lost symbols
- ↓ More false-positive retransmits (duplicates)
- ↓ Duplicates waste bandwidth → worse long-window latency

P_lost(t) is exactly this probability/waste trade-off curve. The sweet
spot is where P_lost(T_retx) is high enough that few retransmits are
wasted, but low enough for timely recovery.

For T_retx < RTT: **proactive retransmit** territory. We retransmit
before knowing if the symbol was lost. This is bandwidth-expensive but
latency-optimal for the individual symbol. Viable only at high ε where
most symbols are lost anyway (P_lost(0) = ε is already high).

### 14.7 When FEC Beats ARQ (and Vice Versa)

**FEC wins when:** t_fec(W) < L_arq ≈ 1.5 × RTT

```
  - High RTT (satellite): ARQ is slow, FEC's window decode is faster
  - Short bursts: m small, few repairs needed, quick decode
  - High bandwidth: t_sym small, window fills quickly
```

**ARQ wins when:** L_arq < t_fec(W)

```
  - Low RTT (datacenter): ARQ round-trip is fast
  - Very long bursts: m large, FEC needs many repairs
  - Low bandwidth: large t_sym, window takes long to fill
```

The crossover point t_fec(W) = 1.5 × RTT determines the optimal
FEC/ARQ balance for a given link. The window size optimization
(Section 14.5) can be tuned to align this crossover with the link's
RTT, making FEC optimal for all bursts shorter than the pipeline.

**MEASURED CORRECTION (L1, branch `feat/fec-arq-crossover`, 2026-07-08).** This
analytic crossover is a per-hole LATENCY comparison and it holds in isolation:
at RTT 200 ms an L1 decode-resolved frontier hole recovers in **8.5 ms** vs the
ARQ round **279 ms** (≈ 1.4 · RTT) — FEC decode is 33× faster, and raw GF(256)
decode COMPUTE is only ~10 µs/symbol (≈ 54 ms total per 1.8 MB transfer, < 1 %).
But t_fec(W) < 1.5·RTT does NOT make FEC win on THROUGHPUT for a *proactive
sliding-window* frontier repair. An RTT sweep {10,30,50,100,200} ms (single
path, 100 mbit, GE ≈ 2.5 %) shows pure-ARQ beating frontier-FEC at EVERY RTT,
FEC/ARQ ≈ 0.61-0.75 flat, **no crossover**, and no win under six tuned W/offset/r
configs at RTT 200. Two effects the isolated latency model omits: (i) a
**pre-position-vs-isolate catch-22** — to arrive before the frontier reaches the
hole a repair must code fresh (still-in-flight) neighbours, so it cannot isolate;
to isolate it must code already-received neighbours, so it arrives after ARQ
already fired. Measured `present_at_stall = 0` always; only ~3 of 86 holes decode,
the rest fall back to ARQ. (ii) **Displacement** — the ~97 %-wasted proactive
repair (rf 486 / ru 16) competes for the shared cwnd/pacing budget with the source
and ARQ retransmits that actually advance the window/RTT-limited frontier. The
model's crossover therefore applies to a window whose members are all RECEIVED at
decode time (systematic / generation coding, §16.3 — fungible cross-path recovery
with no fixed-position in-order hole), NOT to a plain-reliable in-order frontier
hole under bursty loss. See §8.9 and goal-gate "FEC-vs-ARQ Crossover".

**MEASURED CORRECTION 2 — the FUNGIBLE mode ALSO fails to cross (L1, branch
`feat/proactive-fec-highrtt`, 2026-07-08).** The correction above deferred the
crossover to the fungible systematic/generation mode. That mode was then measured
head-to-head vs pure-ARQ across an RTT sweep {10,30,50,100,200} ms (single path,
100 mbit, GE ≈ 2.6 %, systematic source + windowed generation repair + out-of-order
completion). **The crossover does NOT appear for reliable bulk transfer — it
INVERTS with RTT.** FEC/ARQ falls monotonically (≈1.0 tie at RTT 10, where a 1.8 MB
object is warmup-dominated; 0.78 at 50; 0.55 at RTT 200) while the *proactive*
recovery fraction collapses 0.95 → 0.23. Four measured mechanisms defeat the
isolated latency model even in the fungible mode: (i) larger RTT ⇒ larger BDP ⇒
bigger bursts on the DROPPABLE datagram path ⇒ overrun loss that exceeds the fixed
`ceil(len·r)` proactive budget, so generations arrive short and recovery becomes
REACTIVE (round-trip-bound); (ii) the reactive deficit loop, exempt from the
congestion cap so it can always fund a hole, RUNS AWAY under ~RTT-stale feedback at
high RTT (recovery symbols overrun, drop, and are re-sent — measured 30–120× the
object); (iii) a PURE-proactive arm (reactive disabled, measured
`proactive_fraction = 1.0000`, zero round-trips) is genuinely open-loop but DNFs —
the coupon-collector tail leaves some generation a few DoF short of its fixed upfront
budget and it wedges, so open-loop FEC cannot guarantee delivery without the feedback
that reintroduces the round-trip; (iv) generation-mode sender retention prunes on the
IN-ORDER cumulative ack, so a single hole stalls the send window for the recovery
latency even under out-of-order delivery, reproducing ARQ's ∝1/RTT serialization. The
per-hole latency advantage (8.5 ms decode vs 279 ms ARQ at RTT 200, 33×) is real but
does not convert to a throughput win on a reliable bulk transfer at high RTT on the
droppable-datagram transport substrate. The analytic §14.7 crossover holds only as a
per-hole LATENCY statement; it is not a reliable-throughput crossover in EITHER the
frontier or the fungible realization. See goal-gate "Proactive FEC vs ARQ (high RTT)".

**Transport-substrate correction (measured, branch `feat/transport-substrate`).**
Mechanisms (i), (ii), (iv) above are TRANSPORT defects below the FEC layer, and all
three were subsequently FIXED and measured: (i) CC-rate pacing of the source+repair
(token bucket at max(Copa cwnd/SRTT, delivered-goodput EWMA), no BDP-sized burst)
removes the datagram burst-overrun; (ii) per-generation RTT-spacing ("send the
deficit, wait ~SRTT, re-evaluate") + a non-exempt congestion cap ELIMINATES the
reactive runaway — measured `recovery_coded` **90 118 → 436** for a ~5 k-symbol
object (207×) and removes the DNF, restoring `proactive_fraction` **0.042 → 0.90**;
(iv) a coding floor (`code_base`) decoupled from the in-order retention floor lets
proactive coding follow the send frontier while a stalled generation is left to the
bounded reactive tail, collapsing the run-to-run variance (stdev **7.2 s → 0.6 s**).
Together these lift **FEC/ARQ from 0.55 to ≈0.85 at RTT 200** (and hold 0.76–0.86
across RTT 50/100/200) — up, DNF-free, and tighter — **but STILL do not cross 1.0.**
The residual is a FOURTH, receiver-side constraint the three sender fixes do not
touch: at RTT 200 both ARQ and FEC sit at ~1 % of link (a shared LATENCY-bound
regime), and the reliable transfer's last-ε recovery is a reactive tail that the
receiver serializes FRONTIER-FIRST (deficit reports cover only frontier ±
`MAX_REPORTED_GENS`), each round costing an inflated RTT — so the last few DoF cost
as much as ARQ's per-loss round-trip. Raising the proactive `r` (0.2→1.0) does not
help (proactive fraction was already 0.90; the tail is round-trip-bound at the
receiver, not coverage-bound at the sender). **Verdict: the isolated §14.7 crossover
does not appear for reliable bulk transfer even after the three transport fixes; the
remaining blocker is a receiver-side frontier-serialized reactive tail + the shared
latency-bound regime.** See goal-gate "Transport Substrate Fix".

**Receiver-tail correction (measured, branch `feat/receiver-tail`).** The FOURTH
constraint — the frontier-serialized reactive tail — was then addressed directly:
the receiver now reports EVERY outstanding generation's residual deficit in one
report (the `MAX_REPORTED_GENS = 6` cap lifted to a BDP-scaled `RWM_REPORT_GENS`),
so all in-flight holes are NACKed/repaired in ONE round-trip (parallel tail flush)
rather than ≈6 generations per round; and a BDP-derived in-flight cap
(`RWM_INFL_BDP` × Σ Copa `bdp_anchor`) bounds the recovery-round queue so its RTT is
not inflated. **The mechanism is verified at L1** — deficit reports were measured
spanning up to 11 generations in a single round (> the legacy 6-cap), total residual
up to ~5.2 k symbols requested at once. **But the throughput crossover STILL does not
appear.** A single-path LOSS sweep (100 mbit, jitter=0, GE loss ∈ {2.6, 5, 10}%)
gives FEC/ARQ ≈ **0.90** at RTT 100 (loss-independent) and **0.77** at RTT 200/2.6%;
at RTT 100/10% the two TIE on mean (0.69 vs 0.68 Mbit/s) and at RTT 200/10% BOTH
arms DNF (a shared collapse). Raising `r` (0.2 → 0.35 → 0.6) does NOT lift throughput
or the proactive fraction (which stays ≈ 0.35–0.50 regardless): the extra proactive
coded symbols are themselves dropped at the link loss rate on the droppable-datagram
substrate (and/or arrive after the receiver's reactive NACK), so coverage cannot be
bought — **confirming the binding constraint is the transport SUBSTRATE, not the
receiver report bound and not the coding rate.** The one measured GAIN is
tail-latency STABILITY: at RTT 100/10 % the receiver-tail arm's completion-time
stdev is **0.66 s vs ARQ's 61.6 s (≈93× tighter)**, and it stays DNF-free where a
wide send-store without the flush wedges — i.e. FEC buys PREDICTABILITY under loss,
not higher mean goodput. **Verdict: the §14.7 crossover is REFUTED for reliable bulk
transfer on this substrate across the sender fixes AND the receiver-tail fix; the
FEC value proposition here is variance/reliability, not throughput.** See goal-gate
"Receiver Tail + FEC Regimes".

### 14.8 Per-Symbol Recovery Probability Function

Given GE parameters (p, q), the taper, and the encoder window size, we
can compute a per-symbol recovery probability as a function of time:

```
  P_recovery(T | m, T_w) = P(Poisson(λ_total(T)) ≥ m)

  where:
    λ_total(T) = λ_prior(T_w) + λ_new(T)
    m          = burst length (geometric(q) from GE model)
    T_w        = time symbol has been in window before loss
```

This is computable analytically via the Poisson CDF (regularized
incomplete gamma function). No Monte Carlo simulation needed, though
a Markov chain on GE states would give the joint distribution of
(burst_length, recovery_time) for more precise analysis.

Since we measure p and q directly from the GE estimator (Section 7.5),
these parameters are available at runtime. This opens the possibility
of computing the recovery CDF for each symbol to make optimal FEC/ARQ
decisions — a refinement of the current P_lost heuristic.

### 14.9 Reconceived Delivery Time Distribution

The corrected per-symbol delivery time CDF:

```
  P(delivered by T) =
      (1-ε)                                     not lost (arrives at RTT/2)
    + ε × P(t_fec ≤ T | m)                     lost, FEC recovers by T
    + ε × (1-P(t_fec ≤ T | m)) × I(T ≥ L_arq) lost, ARQ recovers by T
```

The tail: δ(T) = 1 - P(delivered by T) = P(delivery takes > T)

This allows the triangle to use a **time budget** T_budget instead of
the current binary FEC/ARQ classification:

```
  Fix T_budget + ρ → compute r   (minimum r for the latency target)
  Fix r + ρ       → compute T    (what latency does this r achieve?)
  Fix r + T_budget → compute ρ   (reliability at this latency budget)
```

The time-based formulation connects naturally to application requirements:
"99% of packets within 33ms" maps directly to δ(33ms) ≤ 0.01, ρ ≥ 0.999.

### 14.10 Latency Tail vs Throughput: Not Always a Trade-off

With perfectly matched FEC (r = ε/(1-ε), zero waste):

- Short-window latency: FEC recovers bursts within the window ✓
- Long-window latency: minimal overhead (~ε/(1-ε) extra symbols) ✓
- Both improve simultaneously vs the no-FEC baseline

The trade-off only appears when:

1. **Over-provisioning FEC:** wastes bandwidth → worse completion time
2. **Over-provisioning ARQ:** duplicates waste bandwidth
3. **Under-provisioning either:** tail latency degrades

The taper function's role (Section 4) is to match FEC exactly to the
loss distribution, avoiding both over- and under-provisioning. When
well-matched, there is no latency-vs-throughput trade-off — only the
inherent cost of channel loss (ε/(1-ε) overhead is the theoretical
minimum regardless of mechanism).

### 14.11 Application Profiles Revisited

With the refined latency model, the application profiles from
Section 1.4 gain additional precision:

**VoIP/Gaming** (T_budget ≈ 20ms, ρ = 98%):

```
  FEC window should contain < 20ms of data.
  At high ε: accept 2% loss rather than ARQ delay.
  Proactive retransmit viable if ε > 30% (P_lost(0) = ε is already high).
  W_optimal: small (minimize t_fec), accept higher per-window loss.
```

**Large file transfer** (T_budget = relaxed, ρ = 100%):

```
  Only throughput (long-window latency) matters.
  Minimize r → maximize source_rate = C/(1+r).
  ARQ handles the tail — per-symbol latency irrelevant.
  r = ε/(1-ε) is optimal (information-theoretic minimum).
  W_optimal: large (maximize pipeline, minimize FEC overhead variance).
```

**Live video** (T_budget ≈ 33ms per frame, ρ = 99.9%):

```
  Per-frame deadline. FEC window ≈ one frame of data.
  Moderate FEC for fast burst recovery within frame.
  ARQ backstop for rare multi-frame bursts.
  W_optimal: ≈ frame_size / symbol_size (natural alignment).
```

**Sensor/IoT** (T_budget = relaxed, ρ = 95%):

```
  Bandwidth-constrained (low-power link).
  Minimal FEC, minimal ARQ. Accept 5% loss.
  r kept minimal to maximize source throughput.
  W_optimal: small (save memory on constrained device).
```

### 14.12 ARQ After FEC Decode: Not Redundant for the Decoder

When the decoder recovers a lost symbol S3 via FEC, the application can
consume S3 immediately. But if an ARQ retransmit of S3 was already in
flight, the retransmit still arrives at the receiver.

**This ARQ is NOT redundant at the receiver.** The decoder should still
feed it as an equation — it is a source symbol (unit vector, guaranteed
linearly independent) that can help decode OTHER lost symbols still in
the decoder window. From the receiver's perspective, every arriving
symbol is useful to the decoder until the window slides past it.

**The ARQ IS redundant at the sender** — once the sender knows (via ACK)
that the receiver has decoded S3, further retransmits of S3 waste
bandwidth. The sender should stop retransmitting ACKed symbols. This is
already handled by the existing ACK mechanism: once the sender receives
an ACK covering S3, it removes S3 from the retransmit buffer.

The interesting case: the sender sends an ARQ for S3 BEFORE learning
that FEC decoded it. This is unavoidable due to ACK delay (~RTT). The
"wasted" bandwidth is bounded by one RTT worth of unnecessary ARQ.
But the receiver still benefits from the equation — so the waste is
only sender-side bandwidth, not receiver-side utility.

### 14.13 Proactive Retransmit vs FEC: FEC is Strictly Better

At the same overhead, FEC is strictly superior to proactive retransmit
(sending every source symbol twice):

- One FEC repair covers ANY single loss in the window (W positions)
- One retransmit covers exactly ONE specific position

For a window of W=50, a single FEC repair is 50x more flexible than a
single retransmit. Proactive retransmit only approaches FEC efficiency
when ε → 50% (half of all symbols are lost, so random "insurance" is
as good as targeted repair). At typical loss rates (1-20%), FEC
dominates by a factor of W/1.

Many real-world systems use packet duplication for simplicity. This model
shows the quantitative cost of that simplicity.

### 14.14 Marginalizing Over Burst Length

The FEC latency CDF (Section 14.3) conditions on m (burst length). The
unconditional CDF marginalizes over the GE burst length distribution:

```
  P(t_fec ≤ T) = Σ_{m=1}^{∞} P(burst = m) × P(t_fec ≤ T | m)
               = Σ_{m=1}^{∞} (1-q)^{m-1} × q × Q(m, λ(T))
```

where Q(m, λ) is the regularized incomplete gamma function.

In practice, truncate at m_max = B_99 (99th percentile burst length).
The tail P(burst > B_99) < 1% contributes negligibly to the CDF.

### 14.15 In-Burst FEC Survival

The Poisson model uses (1-ε) survival probability for all repairs. But
during a burst (channel in Bad state), ALL symbols are lost — including
FEC repairs. Only POST-burst repairs survive.

The corrected λ(T) should split into two phases:

```
  For a burst starting at t=0 with length B:

    λ(T) = 0                                         for T < B (in-burst)
          = Σ_{t=B}^{T} τ(t) × (1-ε_good)           for T ≥ B (post-burst)

  where ε_good ≈ p (probability of re-entering Bad at a post-burst slot;
  the Good state itself is loss-free, h_G = 0) ≈ 0 for packet erasure
```

The practical effect: recovery begins only after the burst ends. The
longer the burst, the later recovery starts. This makes burst length m
doubly important — it determines both HOW MANY repairs are needed AND
HOW LONG before repairs start arriving.

For the ambient pipeline (Section 14.4): repairs generated BEFORE the
burst that are still in transit may arrive during or after the burst.
These contribute if they survive the channel at the receiver — which
they do, because they were transmitted during the Good state before the
burst. The pipeline effect thus provides a head start on recovery.

### 14.16 The FEC/ARQ Race

FEC and ARQ work in parallel for a lost symbol. The actual delivery
time is min(t_fec, L_arq) — whichever recovers the symbol first.

```
  P(delivered by T) = 1 - P(t_fec > T) × P(L_arq > T)
```

P_lost(t) (Section 3.4) currently decides the MIX in correction slots —
FEC or ARQ per slot. But both mechanisms are running simultaneously:
FEC repairs accumulate regardless of whether ARQ fires, and ARQ
retransmits arrive regardless of FEC state.

The min() combination means the actual tail latency is BETTER than
either mechanism alone. The two mechanisms are complementary, not
alternatives. The P_lost mix controls the bandwidth split, but both
contribute to recovery probability.

### 14.17 Decode-Induced Jitter

FEC cascade decoding produces micro-bursts: when the m-th repair arrives
and the decoder resolves a pivot row, cascade recovery may immediately
decode several other symbols. Multiple symbols become available to the
application simultaneously.

This creates "negative jitter" — symbols arriving too close together.
While not harmful (data is available earlier than expected), it means
the delivery time distribution has a step-function component at decode
events, not a smooth CDF.

For jitter-sensitive applications (VoIP): a de-jitter buffer after the
decoder smooths the micro-bursts into a steady stream. The buffer adds
a constant baseline latency (typically one frame period). The net effect
on jitter depends on whether the tail improvement from FEC exceeds the
baseline latency added by the de-jitter buffer.

Tail latency improvement always reduces jitter in the upward direction
(fewer late symbols). Decode micro-bursts create jitter in the downward
direction (symbols arriving early). The de-jitter buffer absorbs the
downward jitter at the cost of baseline latency. As long as:

```
  de_jitter_buffer_delay < tail_latency_improvement
```

the net result is lower jitter. For most FEC-protected links, this
condition holds because FEC reduces tail latency by an RTT (replacing
ARQ with FEC recovery), while the de-jitter buffer adds only a few ms.

### 14.18 Estimator-Rate Feedback Stability

In "compute r" mode, the estimator's ε feeds into the triangle solver
which produces r. But r affects correction volume, which affects how
many symbols the ESTIMATOR observes as lost-and-recovered vs lost-
permanently.

Potential oscillation:

```
  high r → good recovery → estimator sees low ε → solver reduces r
  → poor recovery → estimator sees high ε → solver increases r → ...
```

This feedback loop is stabilized by two properties:

1. **BOCD's inertia**: the Bayesian changepoint detector maintains a
   run-length distribution with prior mass. It doesn't react instantly
   to a few ticks of different ε — it integrates over many observations
   before shifting the predictive quantile.

2. **The estimator observes CHANNEL loss, not APPLICATION loss**: the
   estimator tracks which symbols were lost on the channel (not received
   at the receiver), not which symbols the application eventually got
   (via FEC recovery). FEC recovery doesn't reduce the estimator's ε.

Property 2 is critical. If the estimator instead measured "application-
level loss" (post-FEC), the feedback loop would be unstable. The
estimator must measure RAW channel loss, independent of FEC recovery.

### 14.19 Consistency of P_fec Models

The paper has two FEC recovery models:

- **Section 8.2**: P_fec = Φ(√W × (r(1-ε)-ε) / √(ε(1-ε)(r+σ²_burst)))
  Normal approximation to binomial. Answers: "given r and W, what's the
  probability that FEC recovers ALL losses in the window?"

- **Section 14.3**: P(t_fec ≤ T | m) = Q(m, λ(T))
  Poisson CDF of the correction arrival process. Answers: "given m losses,
  what's the probability that m surviving repairs arrive by time T?"

These answer different questions:

- Section 8.2: probability of FEC success (regardless of time)
- Section 14.3: time distribution of FEC recovery

They should be consistent: by the time the window slides past the lost
symbol, the Poisson CDF should approach the Section 8.2 P_fec.

Verify: the lost symbol stays in the window for W further source symbols
= W(1+r) wire slots, so λ(window exit) = r(1-ε)/(1+r) × W(1+r) = rW(1-ε)
— exactly the mean of the Binomial repair count in Section 8.2. The
Poisson(λ) tail P(≥ m) matches the first moment with a slightly wider
distribution than the Binomial. The Monte Carlo suite compares the two
models numerically across scenarios (tolerance 0.15).

### 14.20 The δ Definition Question

The paper currently has two candidate definitions for δ:

- **Section 6.3**: δ = P(late delivery) / ρ — a probability (dimensionless)
- **Section 14.9**: δ(T) = P(delivery > T) — a function of time

These serve different purposes:

- The probability definition is useful for the triangle solver (binary
  search on r to achieve target δ)
- The time-based definition connects directly to application requirements
  ("99% of packets within 33ms")

**Recommendation**: keep both. The probability δ is the triangle variable.
T_budget is the application requirement. They're connected by:

```
  δ = δ(T_budget) = P(delivery > T_budget)
```

The user specifies T_budget (from application requirements). The system
computes δ = δ(T_budget) using the delivery time CDF, then uses δ in
the triangle solver. This preserves the triangle's mathematical
structure while connecting to real-world requirements.

### 14.21 Sub-Capacity Operation (Emergent Behavior)

When the application data rate is below link capacity, the model should
handle this naturally without a separate code path.

In the current model, Copa (Section 12) controls the total sending rate
C. The taper determines the source/correction split: source_rate =
C/(1+r). If the application produces data at rate S < C/(1+r), the
sender has idle capacity.

In the current architecture, Copa already handles this: if the
application has no data to send, Copa's window isn't fully utilized,
and the sending rate naturally drops to match. FEC symbols are only
generated when source symbols are sent (the taper is driven by source
symbols entering the window).

An interesting possibility: when idle slots exist, the sender COULD
generate additional FEC repairs proactively — filling spare capacity
with extra protection. The taper's r determines the minimum FEC rate;
spare capacity allows exceeding it at no throughput cost.

This should emerge from the triangle solver: when solving for r with a
tight δ target, the solver may compute r > r_min. If the link has
capacity for this higher r without reducing source throughput (because
source_rate < C/(1+r)), the system naturally operates at the higher r.

The diminishing return: when the FEC pipeline λ ≫ B_99 (more ambient
repairs than any likely burst needs), additional FEC provides negligible
improvement. The marginal benefit approaches zero exponentially (Poisson
tail). The triangle solver would compute r at this saturation point
when δ is set very tight — the solver naturally stops increasing r when
further increase doesn't improve δ.

**Open question**: should the taper continue generating repairs during
idle periods (no source data)? These repairs cover the existing window
and strengthen the pipeline. The current model ties correction slots to
source slots — decoupling them would allow "idle FEC" without changing
the architecture, only the scheduling policy.

**A quantitative saturation model.** The L0 gate measured the
diminishing return turning NEGATIVE: at C4-Satellite, Realtime
(δ = 1e-7 → r ≈ 0.49) has a WORSE p99 than Auto (412 ms vs 297 ms).
Past ~43% overhead the extra repairs displace source symbols and
pressure the queue — more FEC hurts the tail. The controller increases
r monotonically with hint tightness and needs a saturation point. The
model below is continuous (no mode switch) and uses only
estimator-known quantities: ε (loss), σ²_burst (Section 8.3), W
(window), SRTT, and t_sym = symbol_size / throughput (the wire slot
time).

The p99 tail has components that pull in opposite directions in r:

```
  tail_fec(r) = (1 - P_fec(r)) × L_arq                       [decreasing]
      FEC-miss cost: misses fall through to ARQ. P_fec from the
      Section 8.2 normal formula; L_arq ≈ 1.5 × SRTT (Section 14.9).

  tail_rec(r) = B × t_sym × (1+r) / (r × (1-ε)),  B = (σ²+1)/2 [decreasing]
      Recovery wait: repairs occupy an r/(1+r) share of wire slots and
      survive with probability (1-ε), so the wait for the B surviving
      repairs a mean burst needs is B × t_sym × (1+r)/(r(1-ε)). This is
      why moderate r keeps helping even after P_fec ≈ 1: repairs arrive
      SOONER. B is the mean burst length recovered from the variance
      factor (σ² = 2B - 1 when p ≪ q, Section 8.3).

  tail_svc(r) = c × (1+r) × W × t_sym,  c = 1/2                [increasing]
      Dilution cost: corrections consume wire share r/(1+r), stretching
      the effective per-source service time — the recovery window
      passes at the diluted source rate, taking (1+r) × W × t_sym.

  p99_model(r) = tail_fec(r) + tail_rec(r) + tail_svc(r)
```

The sum has an interior minimum r_sat, and the controller should emit
`min(r_hint, r_sat)`: the hint still picks the operating point on the
rising side of reliability, but cannot push past the point where more
FEC hurts. The cap is hint-independent — saturation is a channel
property, not a preference.

**Worked example (C4-Satellite).** ε ≈ 9%, σ² ≈ 5 (B = 3), W = 64,
SRTT ≈ 0.21 s (L_arq = 315 ms), throughput 2.5 MB/s, symbol 1225 B →
t_sym = 4.9e-4 s, W × t_sym = 31.4 ms:

```
  r      tail_fec   tail_rec   tail_svc   total
  0.20    40.9 ms     9.7 ms    18.8 ms    69.4 ms
  0.30     4.1 ms     7.0 ms    20.4 ms    31.5 ms
  0.35     0.9 ms     6.2 ms    21.2 ms    28.3 ms
  0.40     0.2 ms     5.7 ms    22.0 ms    27.8 ms   <- r_sat
  0.49     0.0 ms     4.9 ms    23.4 ms    28.3 ms
```

r_sat = 0.40, below Realtime's uncapped 0.49 — consistent with the
measured reversal. At C2-WiFi-like numbers (ε = 2.5%, σ² = 3, W = 64,
SRTT = 13 ms, t_sym = 1e-4 s) the minimum sits at r_sat ≈ 0.255, ABOVE
Realtime's ~0.20 request there: the cap is non-binding exactly where
the measurements show more FEC still helping (C2 Realtime p99 28.1 ms
beats Bulk's 31.5 ms).

**Honesty about roughness.** The dilution constant c = 1/2 is a fitted
scale, not derived; queueing is ignored beyond linear dilution, so the
model's minimum is much shallower than the measured one (the model says
+0.5 ms from r_sat to 0.49 at C4; the gate measured +115 ms — the real
cost past saturation is a queueing knee, not a line). The model is
therefore trusted for the LOCATION of the minimum, not the depth. B
from σ² assumes p ≪ q. Estimation error in ε moves r_sat by a few
percent (higher ε → later saturation), which is the safe direction:
when in doubt the cap loosens. When the estimator has no throughput
sample, t_sym is unknown and no cap applies (degenerate inputs → no
cap): the monotone hint behavior of the base model is preserved.

### 14.21.1 Soft Saturation (Continuous Approach to r_sat)

Section 14.21 emits `min(r_hint, r_sat)`. That hard minimum has a KINK at
r_hint = r_sat: the emitted rate tracks the hint on the low side and then
flat-lines the instant the request crosses r_sat. Physically nothing is
discontinuous there — the p99 curve has a SMOOTH interior minimum, so the
cost of one more repair grows CONTINUOUSLY as r passes r_sat (queue delay
rises smoothly toward the knee; there is no wall). The hard min was an
implementation shortcut. This section replaces it with a kink-free cap and,
as a by-product, turns the binary "cap binding / not binding" signal into a
continuous **saturation pressure**.

**What the p99 model implies.** Near its interior minimum the tail model is
locally quadratic, p99(r) ≈ p99(r_sat) + ½·p99''(r_sat)·(r − r_sat)², so the
marginal p99 cost of exceeding r_sat is p99'(r) ≈ p99''(r_sat)·(r − r_sat) —
zero at r_sat and rising linearly past it. A controller that "does not want
to pay p99" should therefore not hit a wall at r_sat; it should ease off as
the marginal penalty accrues. The natural kink-free family is a one-sided
**softplus** cap:

```
  r_eff = r_sat − s · softplus( (r_sat − r_hint) / s ),   softplus(x)=ln(1+eˣ)

    s = SAT_SOFTNESS · r_sat        (smoothing scale, a rate)
```

with the exact properties (all provable from softplus(x) ≥ max(x,0)):

```
  r_hint ≪ r_sat : r_eff → r_hint           (unsaturated: request honored)
  r_hint = r_sat : r_eff = r_sat − s·ln2     (smoothly just below)
  r_hint ≫ r_sat : r_eff → r_sat             (asymptote — never crossed)
  r_eff ≤ min(r_hint, r_sat)   ALWAYS        (the cap never ADDS FEC)
  dr_eff/dr_hint = 1 − σ((r_hint−r_sat)/s) ∈ (0,1)   (C^∞, monotone)
```

The last line is the key: the derivative eases from 1 (below saturation, every
requested increment is admitted) through ½ (at r_sat) to 0 (deep in
saturation, the cap absorbs the whole increment). Its complement is a natural
[0,1] indicator,

```
  saturation_pressure(r_hint) = σ( (r_hint − r_sat) / s )
                              = 1 − dr_eff/dr_hint
```

— the fraction of a marginal repair-rate increment the cap is currently
absorbing: 0 far below r_sat (more FEC still helps the tail), ½ exactly at
r_sat (marginal p99 benefit = marginal harm — the Section 14.21 balance
point), → 1 past it. This is the continuous quantity the visualizer now
shows in place of the binary CAP BINDING badge.

**Choosing the smoothing scale s (honest constant).** The width over which
the cap bends could be derived from the model's own curvature. Matching the
softplus pressure slope dσ/dr = 1/(4s) at r_sat to the balance-point slope of
the marginal terms gives s = C'_svc(r_sat) / p99''(r_sat) — the ratio of the
(linear) dilution slope to the p99 curvature. But Section 14.21 is explicit
that the model's DEPTH — hence its curvature — is NOT trusted: the true cost
past r_sat is a queueing knee far steeper than the model's shallow quadratic
(+115 ms measured vs +0.5 ms modeled at C4). A curvature-derived s would
therefore track the model's OWN shallow curvature and pick too WIDE a band,
softening the cap so much that FEC drifts into the measured-disaster regime.
The safe, honest choice is a deliberately NARROW fixed fraction,
SAT_SOFTNESS = 0.1, so the soft cap stays within 10 % of the gate-validated
hard min while removing the discontinuity. As SAT_SOFTNESS → 0 the hard min
`min(r_hint, r_sat)` is recovered exactly; the constant trades a hair of
faithfulness to the hard cap for a continuous rate and a continuous pressure
signal. (This mirrors the c = ½ dilution constant of 14.21: a fitted scale,
declared as such.)

**Effect.** At the C4 worked example (r_sat = 0.40, Realtime's uncapped
request 0.49, s = 0.04) the soft cap emits 0.396 with pressure σ(2.25) = 0.90
— indistinguishable from the hard cap's 0.40 in rate, but now the rate is a
smooth function of every input and the operator sees a 0.90 pressure reading
(“the cap is holding, hard”) rather than a lit boolean. Where the request
sits well below r_sat the soft cap is inert to O(e^−1/SAT_SOFTNESS); where it
sits well above, r_eff pins to r_sat to machine precision. The change is
purely in HOW the same ceiling is approached, so the 14.21 gate results are
preserved.

### 14.22 Sequence-Aware P_lost

The current P_lost(t) (Section 3.4) uses only time since send. But SACK
feedback (Section 6.2) provides stronger evidence: if subsequent symbols
have been ACKed, an un-ACKed symbol is almost certainly lost.

```
  P_lost_seq(k) = P(S_n lost | S_{n+1}..S_{n+k} all received)
```

On a FIFO channel (no reordering), if the next symbol arrived, the
previous one was lost — P_lost_seq(1) ≈ 1.0. Real links are not
perfectly FIFO: network jitter, multipath, and switch buffering can
reorder packets.

**Reorder probability estimation**: the system should track the observed
reorder rate (fraction of symbols that arrive out of order) as a running
estimate. Starting assumption: FIFO (reorder_rate = 0). The SACK ranges
provide the observation data — each out-of-order arrival updates the
estimate.

```
  P_lost_seq(k, reorder_rate) = 1 - reorder_rate^k

  For FIFO (reorder_rate = 0):     P_lost_seq(1) = 1.0
  For mild reorder (rate = 0.05):  P_lost_seq(1) = 0.95
                                   P_lost_seq(3) = 0.9999
```

The combined P_lost uses both time AND sequence evidence:

```
  P_lost_combined = max(P_lost_time(t), P_lost_seq(k, reorder_rate))
```

This makes the FEC/ARQ decision faster: instead of waiting for P_lost_time
to rise (which takes ~SRTT), a single subsequent ACK gives near-certain
evidence on a FIFO link. Correction slots can switch to ARQ sooner for
confirmed losses, freeing FEC capacity for uncertain losses.

### 14.23 Post-Burst FEC Boost (Reactive Deficit Recovery)

After an unexpectedly long burst (longer than the taper was provisioned
for), the system has a correction deficit — more losses occurred than
the ambient FEC pipeline can cover. The BOCD estimator will adapt ε
within 5-15 batches, but during that lag the system is under-protected.

The GE model's state transitions provide a faster signal: the Bad→Good
transition indicates burst end. At that moment, the deficit is known:

```
  On Bad→Good transition:
    burst_length = observed consecutive losses
    repairs_in_pipeline = λ_prior (ambient FEC accumulated)
    deficit = max(0, burst_length - repairs_in_pipeline)
```

If deficit > 0, the system should temporarily boost FEC:

```
  boost_duration = deficit / (r × (1-ε))    ticks
  boost_r = r + deficit / boost_duration    (spread the extra FEC)
```

This is faster than waiting for BOCD to increase ε → solver to increase
r → taper to generate more corrections. The GE state transition is
observable within one symbol interval; BOCD takes 5-15 batches.

**Interaction with ARQ**: at burst end, the oldest lost symbols are
~burst_length ticks old. If burst_length > SRTT, P_lost_time is already
high for those symbols — ARQ fires naturally. The boost is needed for
the symbols where FEC should have covered them but didn't (pipeline
was insufficient).

**Open question**: should the boost be additive (extra FEC on top of
normal taper) or multiplicative (temporarily higher r)? Additive is
simpler and doesn't affect the taper shape for new symbols. The extra
repairs specifically target the deficit, not general protection.

### 14.24 Jitter-Horizon Encoder Lag

Discovered while building the L0 gate suite (ADR-0051): with per-packet
jitter J, a repair symbol can overtake up to J × send_rate of the source
symbols it covers. At arrival, those covered-but-still-in-flight symbols
are unknowns to the decoder, so the repair parks as a deep pivot equation
instead of decoding the actual loss — measured hole-fill p50 at C2-WiFi
was ~21 ms instead of the expected ~2 ms.

A repair covering symbols that cannot yet have arrived carries no usable
information at arrival time. The encoder should therefore TRAIL the send
stream by the jitter horizon:

```
  L = ceil(J × send_rate)          [symbols; 2..48 in practice]

  repairs cover [sent − L − W, sent − L]   instead of  [sent − W, sent]
```

With the lag, a repair's unknowns at arrival are true losses, decodable
immediately. The correction budget is unchanged (Section 4.2 shape
invariance) — only the window placement moves. L adapts with measured
jitter and send rate; on a jitter-free channel L → 0 and the windows
coincide. This composes with Section 14.5's window sizing: the effective
protection span for fresh symbols shrinks by L, which matters only if
L approaches W.

### 14.25 Completion-Tail FEC

For a finite transfer, completion time decomposes as:

```
  T_completion = T_send  +  T_tail_recovery

  T_send          = N x t_sym / (1 + r)⁻¹-adjusted source rate
                    (the time to push all N source symbols)
  T_tail_recovery = the time to recover losses among the LAST
                    window's symbols after the send stream ends
```

The two terms have completely different loss economics. A mid-transfer
loss is recovered in parallel with ongoing sends: ARQ (or a later
repair) rides alongside new source symbols, so the recovery consumes
wire budget but adds ZERO completion time — the link never goes idle
waiting for it. A tail loss is different: once the last source symbol
has been sent there is nothing left to overlap with, so every ARQ round
on a final-window hole is serial — ~1.5 RTT each (detection at ~9/8
SRTT plus the retransmit flight), and a retransmit that is itself lost
pays the full round again.

This yields an end-of-stream policy that is nearly free:

```
  At end-of-stream (last source symbol sent), send a burst of
  n_tail = ceil(r_tail x W) repair symbols covering the final window.

  Cost:    r_tail x W symbols  ≈ negligible vs N for a large transfer
           (e.g. 0.2 x 64 = 13 symbols on a 1500-symbol transfer < 1%)
  Saving:  P(≥1 tail loss) x ~1.5 RTT of completion time per avoided
           serial ARQ round, where
           P(≥1 tail loss) = 1 - (1-ε)^W    (≈ 80% at ε=2.5%, W=64)
```

r_tail comes from the exact transfer-matrix computation (Section 8.7):
the smallest r such that P_fail(r, W) ≤ δ_tail for a modest tail-failure
budget (e.g. δ_tail = 0.05 — one residual serial ARQ round in 20
transfers). The exact DP matters here: the tail burst is a one-shot,
small-W event where the normal approximation's 30-50% tail
under-provisioning (Section 8.7) directly converts into completion
regressions.

**Composition with Bulk's r → 0 steady state (Sections 5.3, 12.5).**
Bulk maps δ to "late is fine" (δ_bulk = ε̂ mid-stream, Section 14.26),
so the continuous r* formula (Section 8.4) is 0 mid-transfer: pure ARQ,
volume parity with retransmission transports. Completion-tail FEC is
the complement, not a contradiction: FEC vanishes in the steady state
(where recovery overlaps sending and buys nothing) and reappears
exactly at the one place it buys completion time — the stream tail.
The r-δ-ρ triangle is respected at both operating points; only the
effective δ differs, because the COST MODEL differs between
mid-transfer (parallel recovery, late is genuinely fine) and the tail
(serial recovery, late is 1.5 RTT each). Section 14.26 makes this
composition CONTINUOUS: the one-shot burst described above is the
limiting case of a δ glide driven by a completion-exposure kernel χ,
and the glide supersedes the burst wherever T_rem is known.

**Multipath corollary — tail reinjection.** On asymmetric paths the same
end-of-stream logic applies to slow-path IN-FLIGHTS, not just losses:
once nothing overlaps recovery, an undelivered symbol whose path's
residual wait (queue + propagation) exceeds a fast-path flight is worth
duplicating onto the fast path (cross-path retransmit, Section 13.10).
The duplicate rides spare end-of-stream tokens, so the cost is a few
symbols of fast-path capacity against a saving of the slow path's queue
drain (~10-25 ms measured on WiFi+LTE). Same principle, ARQ flavor:
completion is bought exactly at the stream tail, nowhere else.

### 14.26 Completion-Exposure δ (the Bulk glide)

Section 14.25 established the two-regime cost model for Bulk: a
mid-transfer loss recovers in parallel with ongoing sends (zero
completion cost), a tail loss recovers serially (~1.5 SRTT per ARQ
round). It implemented the tail side as a one-shot repair burst at
end-of-stream. This section replaces the sharp mid/tail boundary with a
continuous kernel and, in doing so, fixes two measured flaws in the
original Bulk hint mapping δ_bulk = min(0.1, ε̂).

**The two flaws of min(0.1, ε̂)** (measured in the wasm simulator, which
shares `controller_rate` with production):

```
  M1 — cold start. The loss estimator's Beta(1,1) prior puts the 95%
  predictive upper quantile at ε̂₉₅ ≈ 0.975 for the first ~1.5-3 RTT.
  The clamp maps this to δ_eff = 0.1 << ε̂₉₅, so z_{δ/ε} is large, the
  IT term ε̂/(1-ε̂) ≈ 39 dominates, and r pins at max_overhead = 0.5 —
  wasting ~1/3 of the wire for 2-3 RTTs on a channel about which
  nothing bad is actually known. Measured: a FIXED r = 0.01 floor beat
  Bulk on completion in 20/24 grid cells (median +5%, worst +17%,
  gap growing with RTT) and on overhead in 24/24 (excess overhead
  2-14% vs ~0-1%).

  M2 — moderate loss. At ε ≥ ~0.1 the upper quantile sits above the
  0.1 clamp FOREVER, so δ_eff = 0.1 < ε̂₉₅ permanently and r* > 0 in
  the steady state: Bulk pays FEC ≈ the IT floor AND ARQ for the same
  losses — double payment, directly against the 14.25 cost model in
  which mid-stream lateness is free.
```

Both flaws share one root: any δ_eff strictly below ε̂ re-activates the
margin machinery, whose entire purpose Bulk rejects mid-stream.

**The completion-exposure kernel.** What actually distinguishes a loss
that costs completion time from one that does not is whether its ARQ
round (~1.5 SRTT, Section 3.4) can still hide behind remaining sends.
Let T_rem be the remaining send time (in seconds, like Section 5.4's
timing quantities: T_rem = remaining source symbols / send rate).
Reusing the Section 3.4/5.4 normal-RTT-tail machinery (Φ̄ = normal
survival function):

```
  chi(T_rem) = Phi_bar( (T_rem - 1.5 x SRTT) / sigma_arq )

  sigma_arq  = max(4 x RTTVAR, SRTT / 4)
```

χ is the probability that a loss suffered NOW is EXPOSED — that its
recovery outlives the send stream and becomes serial completion time.
The 1.5·SRTT center is the Section 14.25 serial-round cost; σ_arq
aggregates detection and flight-time variance (4×RTTVAR, floored at
SRTT/4 against degenerate RTTVAR estimates). Mid-stream (T_rem ≫ SRTT)
χ = 0; over the final ~1.5 SRTT it rises smoothly to 1. No cutoff
anywhere: χ inherits its continuity from Φ̄.

**The δ glide.** Bulk's effective tail target is the exposure-weighted
blend of "the channel itself" and the 14.25 tail budget:

```
  delta_bulk = eps_hat + (delta_tail - eps_hat) x chi,   delta_tail = 0.05
```

- **Mid-stream (χ = 0):** δ_bulk = ε̂ exactly, so z_{δ/ε} = Φ⁻¹(0) = −∞
  and r* = 0 IDENTICALLY — independent of the estimator's uncertainty,
  because the target tracks the estimate ITSELF rather than a constant
  the estimate is compared against. This kills M1 (ε̂₉₅ ≈ 0.975 → δ_eff
  = 0.975 → r* = 0) and M2 (ε̂₉₅ = 0.12 → δ_eff = 0.12 → r* = 0) in one
  stroke. It is exactly the "late is fine" semantics of the 14.25 cost
  model: mid-stream recovery is parallel and therefore free, so NO
  channel state justifies steady-state FEC under Bulk.
- **Stream tail (χ → 1):** δ_bulk → δ_tail = 0.05 regardless of ε̂, and
  r ramps continuously to the exact rate that meets the completion
  budget — reaching it about 1.5 SRTT before the last source symbol,
  precisely when losses start being exposed.
- **T_rem unknown:** a production tunnel is an endless stream — there
  is no "last symbol" the sender can see, so χ = 0 permanently and the
  steady state is pure ARQ (existing production tail behavior is
  unchanged). Feeding χ from an application-known transfer size, or
  from an idle-onset heuristic (send queue drained = provisional end of
  stream, retracted if new data arrives), is future work; simulation
  drivers that know the transfer length feed χ directly.

**The tail burst as the glide's limiting case.** The 14.25 one-shot
burst is what the χ ramp degenerates to as σ_arq → 0: a step from r = 0
to the tail rate at T_rem = 1.5 SRTT. The glide is therefore not a
second mechanism but the burst made continuous — and it supersedes the
burst wherever T_rem is known (firing both would double-pay the tail
budget). Beyond removing the discontinuity, the ramp fixes a coverage
gap of the burst at high RTT: with in-flight span (symbols per RTT)
≫ W, the last-window burst cannot protect late-stream losses that fall
OUTSIDE the final window yet inside the final serial-recovery horizon.
The ramp's repairs are emitted across the whole final ~1.5 SRTT of the
send stream, each covering the sliding window at its emission time, so
the entire exposed span gets coverage. The same mechanism suppresses
the pure-ARQ (r = 0 floor) serial-retransmit tail outliers: a tail hole
whose retransmit is itself lost pays full serial rounds (one observed
865-tick outlier vs 559 ticks with a 1% floor); ramped repairs give
those holes a parallel recovery path.

**Scope of the mid-stream guarantee.** χ = 0 requires T_rem to exceed
the exposure horizon ~1.5·SRTT + 8·σ_arq (≈ 3.5-5.5 SRTT). A transfer
SHORTER than that never has a mid-stream phase: χ > 0 from the first
symbol, and during the estimator cold start the glide inherits M1's
inflated ε̂₉₅ (measured at 150 ms RTT with a 0.5 s transfer: neither
mapping wins — the early rate is governed by the cold-start prior in
both). The glide's guarantee is for streams long enough to HAVE a
middle; the cold-start prior itself is estimator work, not δ-mapping
work. A second CANDIDATE scope limit is the PAYLOAD's dynamics:
"parallel recovery is free" prices the outer stream's volume, not
delivery latency, so a payload whose own delivery latency feeds back
into its throughput (TCP inside the tunnel) could in principle pay for
every unrepaired loss. Section 14.28 derives the mid-stream repair
floor for those inner-feedback flows — and reports the L1 measurement
that, once 14.27's reactive leg and in-order delivery exist, the inner
flow absorbs the residual stalls and the floor buys nothing (C2) or
actively hurts (C3). The glide as stated survives that test.

**Verification** (wasm simulator, shared formula, same seeds per cell;
ε = 0.05/0.10, q = 0.5, RTT = 50 ms, W = 64): the glide vs the old
mapping cuts completion 599 → 562 ticks and excess overhead 5.99% →
0.04% at ε = 0.05, and 674 → 620 ticks / 8.21% → 0.91% at ε = 0.10;
Bulk now beats the fixed r = 0.01 floor on BOTH completion (562 vs 598
ticks) and overhead (5.31% vs 6.11%) at the reference cell — the
mapping the 20/24 finding said it must at least match.

### 14.27 Block-Mode ARQ via Batch Acknowledgements

Section 5 defines corrections as repairs ∪ retransmits, both driven by
P_lost; Section 14.26 then makes mid-stream r* = 0 for Bulk on the
grounds that a mid-stream loss recovers "for free" through the reactive
path. The production BLOCK pipeline implemented neither reactive leg:
on a decode failure the sender only updated stats and congestion
control, and the receiver evicted the incomplete decoder after a
timeout. Nothing was ever retransmitted. Under the glide this is not a
degraded mode but a contradiction — r* = 0 is only correct BECAUSE the
retransmit path exists — and the L1 harness measured the consequence
directly: at C2 (100 Mbit, 10 ms RTT, GE 1.3%/50% ≈ 2.6% loss) the
production tunnel completed a 1.8 MB transfer in ~8 s against quinn's
0.175 s. The inner TCP saw the raw 2.6% loss and collapsed; every
block with a hole waited out the 30 s decoder eviction. The window
pipeline has its own retransmit buffer (Section 6.1); this section
specifies the block-mode equivalent.

**The Ack IS the P_lost evidence.** The block receiver already
acknowledges every SymbolBatch it receives — each QUIC datagram is one
batch, so batch loss is symbol loss. The v4 Ack echoes the batch's
`batch_seq`, which keys a sender-side ledger of
(batch_seq → path, [(block, symbol)], send time). This is the SACK
limiting case of the Section 3.4 P_lost model: an Ack for batch j on a
path, with none for an earlier batch i on the same path, drives
P(lost_i) → 1 without waiting for a timer. Concretely a batch is
declared lost when 3 later same-path batches have been acked (the
dup-ACK analogue; tolerates datagram reordering) or when
max(1.5·SRTT, 50 ms) elapses without an Ack (the timeout leg — needed
for transfer tails, where no later traffic exists to accuse the loss;
a 25 ms sweep task covers that case). The ledger is keyed by
`batch_seq` and NOT by anything already in the v3 Ack: the echoed send
timestamp is shared by every chunk of one drain call, and a batch may
interleave symbols of several blocks, so the v3 fields identify neither
the batch nor the victims. Adding the 8-byte field (protocol v4) was
the robust option.

**Fresh repairs beat resends.** The sender retains source data for the
last 64 blocks (LRU, ≤ 4 MB). On a loss event it re-derives the block
encoder and, for rateless codes (RaptorQ, RLC), mints repair symbols at
ESIs beyond anything previously sent: any repair fills any hole, so a
fresh repair strictly dominates resending the specific lost symbol
(which a burst may kill again — fresh repairs also compose across
multiple holes in one block). Fixed-rate codes (RS, METTLE) cannot mint
post-hoc, so they fall back to resending the exact missing symbols from
the retained data, which every decoder accepts. Correction volume per
event is missing + ⌊accumulated ε̂ margin⌋ — the margin is fractional
and carried continuously across events (at MTU-sized batches an event
is typically ONE symbol; a per-event ceil would double the correction
volume, i.e. cost 2ε instead of ε(1+ε)). Repair batches re-enter the
ledger, so a lost repair triggers the next round with doubled margin,
capped at 3 rounds per block; the receiver's eviction timeout remains
the final backstop.

**Recovery bound.** Detection costs one RTT (the Ack of the next
surviving batch), repair transmission another half: a mid-stream hole
closes in ~1.5 RTT wall time WITHOUT stalling the send stream — which
is exactly the "parallel recovery" the 14.25 cost model prices at zero
completion cost, now actually implemented. Ack loss (~ε̂ of batches) is
indistinguishable from batch loss and costs one spurious repair event,
bounded overhead ~ε̂²; the receiver drops symbols for already-decoded
blocks, so spurious repairs are harmless. Corrections are charged
against the same in_flight budget and pacing tokens as scheduled
symbols (Section 12.5), and the estimator is NOT fed from the ledger —
the receiver-side batch-gap accounting already reports these losses,
and double-feeding would bias ε̂ upward.

Honest constants: 64 retained blocks / 4 MB, dup-ACK threshold 3, loss
timeout max(1.5·SRTT, 50 ms) clamped to 2 s, sweep cadence 25 ms,
margin ε̂·2^round continuous, 3 rounds max, 4096-entry ledger cap.
L1 verification of the C2 completion gap (8 s → competitive with the
0.175 s quinn bound) is pending; the mechanism is unit-verified at L0
(Ack-diff, dup-ACK and timeout legs, mixed-block batches, lost-Ack
non-amplification, lost-repair second round, LRU caps, margin math,
fresh-vs-resend per backend, decode-after-repair for RaptorQ and RS).

### 14.28 Inner-Feedback Flows and the Repair Floor

Section 14.26's mid-stream guarantee — δ_bulk = ε̂, r* = 0, pure ARQ —
rests on the 14.25 cost model: a mid-stream loss recovers in parallel
with ongoing sends, so lateness costs no completion time. This section
derives the model revision that argument SEEMED to demand for
tunneled control-loop payloads, and then reports the L1 measurement
that refuted the revision's premise — kept in full because both the
derivation and the refutation constrain the model. The suspicion: at
C2 (100 Mbit, 10 ms RTT, GE 1.3%/50%) the production tunnel carrying a
kernel-TCP transfer completed 1.8 MB in 1.11 s median against quinn's
0.20 s, WITH block-mode ARQ (14.27) closing every hole in ~1.5 RTT;
the P9b analysis attributed the residual gap to each GE loss event
stalling the inner flow's in-order delivery ~1 ARQ round, ~20 events
per transfer. A similar effect had appeared at L0 in the P6 goal-gate
ablation: a small FIXED r = 0.01 floor beat pure ARQ on completion
through straggler coverage.

**Why "parallel recovery is free" could fail here.** The 14.25/14.26
argument prices the VOLUME of the outer stream: while a hole waits for
its repair, other symbols keep flowing, so outer throughput is
unharmed and outer completion is untouched. But the tunnel's payload
is not inert bytes — it is a control loop. Inner TCP consumes the
tunnel's output IN ORDER; an unrepaired outer loss halts that delivery
for one outer ARQ round, the inner ACK clock stalls with it, and — IF
the stall exceeds the inner loss-detection tolerance — the inner
congestion controller converts the stall into a rate cut. Late is fine
for a file; late is potentially a throughput signal for a flow that
watches its own latency. (The L1 verification below measures that
"if": post-14.27 the stalls stay inside the inner tolerance.)

**The stall cost model.** Per unrepaired loss event the inner flow
stalls for one outer ARQ round, capped by the inner transport's own
patience (its RTO — after which it retransmits through the tunnel and
eats the loss the expensive way):

```
  L_stall = min(1.5 x SRTT_outer, RTO_inner)

  RTO_inner >= max(RTO_MIN, SRTT_inner),  RTO_MIN = 200 ms (Linux),
  SRTT_inner >= SRTT_outer (the inner path IS the outer path plus
  tunnel processing), so the implementation uses the observable bound
  L_stall = min(1.5 x SRTT, max(0.2, SRTT)).
```

Loss events are GE burst onsets: probability ≈ ε̂ · q̂ per wire slot
(q̂ = 1/B̂ from the estimator; q̂ = 1 degenerates to iid). A proactive
repair stream at rate r races each event against the ARQ horizon
T_arq = L_stall / t_sym wire slots exactly as in Section 14.16, with
the Section 14.14 burst-marginalized recovery probability:

```
  C(r) = P(event repaired within T_arq)
       = sum_m (1-q̂)^{m-1} q̂ x P(Poisson(lambda) >= m),
         lambda = T_arq x r(1-ε̂)/(1+r)
```

The expected fraction of wall time the inner delivery spends stalled
is then

```
  S(r) = ε̂ x q̂ x T_arq x (1 - C(r))        [stalled slots per slot]
```

**The floor.** A stall is invisible to the inner flow when it is
indistinguishable from the delivery jitter the flow already absorbs.
The repair floor is the smallest rate whose residual stall sits at or
below that jitter scale — a continuous trade, not a cutoff:

```
  r_min = min { r >= 0 : S(r) <= theta },   theta = sigma_j / L_stall,
  sigma_j = SRTT/4
```

σ_j is the 14.26 σ_arq evaluated at its SRTT/4 floor: the sender
cannot observe the inner flow's actual tolerance, and the production
RTTVAR estimate is a fixed-fraction heuristic, so the deterministic
branch is the honest choice (it errs toward slightly more repair).
S is continuous and nonincreasing in r, so r_min is continuous in
every input; when S(0) ≤ θ already — clean channel, short horizon —
the floor is exactly 0 with no branch. At C2 timescales the floor
engages only above ε̂ ≈ 0.0016; at the operating point it solves to

```
  ε̂ = 0.026 → r_min = 0.029      (T_arq ≈ 200 slots, theta = 1/6)
  ε̂ = 0.035 → r_min = 0.033
  ε̂ = 0.045 → r_min = 0.036
```

— the 0.01-0.04 band the P6 fixed-floor ablation pointed at, now
derived rather than tuned. An unknown t_sym (no throughput estimate)
disables the floor, the same sentinel convention as the burst B/T
term. The floor composes with the rest of the controller by max():
the χ glide still owns the stream tail, and the 14.21 saturation cap
still overrides (where more FEC hurts the tail, the floor must not
insist).

**Who gets the floor: the weight w ∈ [0, 1].** The protocol hint alone
cannot decide this. Bulk's "late is fine" is a statement about the
PAYLOAD BYTES; whether lateness feeds back is a statement about the
PAYLOAD'S DYNAMICS — a second, orthogonal input:

```
  rate = max(rate_glide, w x r_min)

  w = 1: inner-feedback payload (TCP-in-tunnel: the payload is a
         control loop). Opt-in via the tunnel's inner_feedback_weight
         config; the ORIGINAL plan made this the Bulk-tunnel default,
         retracted after the L1 verification below.
  w = 0: file-transfer semantics — the L0 gate driver, the wasm
         simulator, bench_suite, and any payload that is itself the
         object being measured. There mid-stream ARQ recovery is
         GENUINELY free and 14.26 applies unweakened; the gate's Bulk
         volume-parity claims are stated at w = 0. Also the production
         tunnel default (see the verification).
```

Intermediate w scales the floor linearly (a mixed payload tolerates
proportionally more stall); the rate is continuous in w, and w = 0
reproduces the old controller identically. When the future
idle-onset/T_rem heuristics of 14.26 land, χ and w remain independent:
χ says WHERE in the stream you are, w says WHAT the stream carries.

**What the floor is not.** It is not a retreat from 14.26: M1 and M2
(cold-start pin, permanent double payment) stay dead because the floor
is finite, derived from the stall budget, and vanishes on clean
channels — unlike the old min(0.1, ε̂) target, which re-activated the
full margin machinery against an arbitrary constant. Nor does it
replace 14.27's reactive leg: ARQ still recovers every hole; the floor
only buys back the ~1.5-RTT DETECTION latency that no reactive scheme
can remove, and only where that latency is a cost. The honest price is
volume parity: with w = 1 the tunnel pays ~r_min ≈ ε̂ extra overhead
mid-stream, conceding 14.26's parity claim for inner-feedback payloads
— measured against the stall cost it removes (L1 ablation, w = 0 vs
w = 1 at C2, below).

**L1 verification — the premise is refuted post-14.27 (negative
result).** Measured on the L1 harness (rp-bulk tunnel, 1.8 MB objects,
seed 42, fresh topology per arm, cross-session interference audited
via the sudo journal):

```
  Instrumentation first: the floor was initially INERT at L1 — the
  production estimator had no local throughput measurement (the only
  record_throughput feed was the peer's PathReport value, which is the
  peer's estimator.throughput(): circular, both sides 0.0 forever), so
  t_sym = 0 sentinel-disabled the floor — and, silently, the 14.21
  saturation cap and the 8.4 burst B/T term on every real link. Fixed:
  the report task now feeds the achieved send rate (symbols sent per
  report interval), which is also the correct T_arq slot semantics.
  The peer-feed is removed (it would mix the REVERSE direction's send
  rate — an ACK trickle — into the data direction's t_sym).

  C2 (100 Mbit, 10 ms RTT, GE 1.3%/50%, eps ~ 2.6%), 10 runs/arm:
    w = 0:  median 1.179 s, mean 1.107   (second arm, 5 runs: 0.784/0.955)
    w = 1:  median 1.192 s, mean 1.121   (second arm, 5 runs: 1.158/1.171)
    client FEC volume: 2.46% (w=0, reactive 14.27 repairs only)
                       4.66% (w=1, floor verified ACTIVE on top)
    inner TCP RetransSegs per 5 transfers: 11 (w=0) vs 21 (w=1)
  C3 (20 Mbit, 40 ms RTT, GE 2%/40%, eps ~ 4.8%), 5 runs/arm:
    w = 0:  median 5.34 s, mean 5.90
    w = 1:  median 6.82 s, mean 7.00     (+28% median — the floor HURTS)
```

The floor demonstrably fires and pays its budget, and buys nothing:
completion is flat at C2, the inner flow's loss-recovery signature does
not shrink, and C3 regresses 28%. Two lessons, honestly taken:

1. **The stall premise over-priced lateness.** The P9b residual
   analysis (~20 events × ~1.5 SRTT) dated from before 14.27's
   reactive leg and the P9b in-order delivery hold worked TOGETHER: a
   mid-stream hole now closes in ~1.5 RTT while the receiver's
   SRTT-adaptive hold delivers around it, so the inner TCP sees
   delivery jitter well inside its own tolerance (RTO_inner ≥ 200 ms ≫
   the 20-60 ms stalls; 11 retransmits per 9 MB). The model priced
   EVERY unrepaired loss at L_stall; the true cost gates on
   P(stall exceeds the inner loss-detection tolerance), which
   post-14.27 is ~0 at C2/C3 — i.e. the honest r_min is 0, which is
   what production now defaults to (inner_feedback_weight = 0, knob
   kept for genuinely brittle payloads — measure before enabling).
2. **Floor repairs are not free in the closed loop.** Repairs are
   charged against the same cwnd/pacing budget as source symbols
   (12.5), so at inner-limited rates they displace the very traffic
   whose latency they protect — the 14.21 dilution cost made
   system-visible. Worse, T_arq is measured in SEND-PROCESS slots: at
   low operating rates the horizon holds few slots, r_min grows
   (5-10% at C3's 2-3 Mbit/s effective rate), and the tax compounds
   exactly where the link is tightest. That is the C3 regression.

The 14.26 mid-stream guarantee therefore survives its hardest test to
date: even for the TCP-in-tunnel payload it was suspected to fail, pure
mid-stream ARQ (with 14.27 implemented) beats ARQ + repair floor. What
remains of the C2 gap (P9b: ~1.1 s vs quinn 0.20 s) is NOT unrepaired-
loss stalls; the suspects are the residual Copa backoff ceiling and the
inner flow's own slow-start (goal-gate P9b items b and c).

### 14.29 The End-of-Stream Taper Completion Term (All Hints)

Section 4.2's truncation note established the problem: at a finite transfer's
end the taper's forward integral is cut, so the last W source symbols are
progressively under-covered and their losses fall to serial ARQ — an
end-of-stream reliability cliff. Section 14.25 patched it for Bulk with a
one-shot repair burst at end-of-stream, and Section 14.26 made THAT
continuous via the completion-exposure δ glide — but only for Bulk. The tight
hints (Auto, Realtime) were left with the ad-hoc one-shot burst. This section
derives the truncation loss and gives one continuous completion term that
serves every hint, of which the 14.25 burst is the discrete limiting case.

**The truncation loss.** Let a symbol sit at distance j = N − i from the last
source position N. Mid-stream a symbol accumulates coverage r·W over its W
window-lifetimes (r = the hint's steady correction rate); at distance j < W
only j of those lifetimes occur before the stream ends, so its coverage is
r·j and its DEFICIT is

```
  Δcov(j) = r · (W − j),     0 ≤ j ≤ W       (0 for j ≥ W)
```

Repairs are window-fungible (Section 3.2), so what matters for the final
window is the TOTAL repair mass emitted into [N−W, N): mid-stream that mass is
r·W, at the tail it is only what the truncated positions provide. The
expected uncovered tail loss is the probability that the final window carries
more losses than its (deficient) repairs cover — for the whole window,

```
  P(uncovered tail) = P_fail(r_eff, W),   r_eff = coverage mass / W
```

with P_fail the exact transfer-matrix tail (Section 8.7; the normal
approximation under-provisions small-W one-shot events by 30–50 %, Section
14.25). Untreated, r_eff → 0 as the window empties and P_fail → 1 − (1−ε)^W ≈
80 % at ε = 2.5 %, W = 64 — every finite transfer ends with a near-certain
serial-ARQ round. The position dependence is entirely through j: the loss is
concentrated in the last W symbols and is RTT-independent (the final window
has nothing to overlap recovery with, at any RTT).

**The completion term.** Restore the final window to its full repair mass by
injecting the missing budget. The 14.25 burst does this in one shot:
B_tail = r_tail·W repairs, r_tail = the exact-DP rate meeting the hint's tail
target δ_hint on a window of W (for Bulk, δ_tail = 0.05). The continuous
generalization meters that SAME budget as a Stieltjes measure over a
completion kernel χ_trunc that rises 0 → 1 across the truncated region:

```
  completion debt per source symbol = B_tail · dχ_trunc

  χ_trunc(remaining) = Φ̄( (remaining − W/2) / (W/4) )      remaining = N − i
```

Because χ_trunc is monotone 0 → 1 as the window empties, the total metered is
exactly B_tail — one window's worth, released continuously instead of dumped
at an instant. Two properties make this the RIGHT kernel:

- **It is over SOURCE POSITION, not wall time.** The truncation is a
  source-position phenomenon (only the last W symbols), unlike Section
  14.26's completion-exposure χ(T_rem), which is a wall-time ECONOMICS kernel
  ("is this loss's recovery serial?"). Driving the metering by T_rem would
  spread the fixed budget over the final ~1.5 SRTT of WALL time — a span of
  many W at high RTT — diluting the final window and REGRESSING its tail
  (measured: Realtime last-window p99 27 → 49 ms). The source-position kernel
  concentrates the budget on exactly the deficient symbols.
- **The one-shot burst is its σ → 0 limit.** As the kernel width W/4 → 0,
  χ_trunc becomes a step at remaining = W/2 and the whole budget releases at
  once — the 14.25 burst. The continuous form additionally covers late-stream
  losses that the burst's single final window misses but that still recover
  serially at high RTT (the Section 14.26 high-RTT coverage gap), because its
  repairs are emitted ACROSS the tail rather than only at [N−W, N).

**Relation to the Bulk glide (14.26).** Bulk and the tight hints now both get
a continuous, χ-driven completion term, but through different channels that
must not double-fire. Bulk maps completion exposure into δ_eff (the glide
raises r from 0 mid-stream to the tail-budget rate), which is simultaneously
its "late is fine → r = 0 mid-stream" economics AND its truncation refill;
that is correct BECAUSE Bulk's mid-stream rate is 0, so its final-window
refill is the whole of its tail FEC. The tight hints already run r > 0
mid-stream (their δ is tight), so their truncation refill is ADDITIVE on top
of the steady rate — the B_tail·dχ_trunc term — and their δ is untouched. In
both cases mid-stream coverage is unchanged and exactly one window's worth of
extra repairs lands at the tail.

**Scope.** Like 14.26 this needs a KNOWN end of stream: the driver (or an
application-declared transfer size) supplies N. An endless production stream
has no last window, so remaining = ∞, χ_trunc = 0, and the term vanishes —
production window mode instead closes tail holes with its NACK/tail-sweep
path (Sections 6.1, 14.27). Verification (wasm simulator, ε = 5 %/8 %,
RTT 50/150 ms, W = 64, shared seeds): replacing the one-shot burst with the
metered ramp holds last-window p99 at parity mid-stream (Auto 25 → 26 ms,
Realtime 27 → 27 ms at RTT 50) and IMPROVES it where the burst's single
window under-covers the exposed span (both hints 80 → 76 ms at RTT 150), for
≤ 2 % extra overhead. The end-of-stream cliff test (last-window p99 ≈
mid-stream p99) passes for both tight hints.

---

### 14.31 The generation decoder must admit late sources (an implementation invariant, not a model term)

The Section 16.3 systematic-repair recovery model assumes a generation of K_g
sources with `h` holes decodes from exactly `h` windowed repair symbols — the
deficit is the hole count, and the pre-received sources contribute their K_g − h
degrees of freedom "for free." An L1 measurement (2026-07-08) exposed a decoder-
side violation of this assumption that made proactive recovery appear DEAD: only
`repairs_useful ~ 7` of `repairs_fed ~ 4600` arriving repair symbols ever added
rank (0.15 %). The cause was NOT a modelling error and NOT the substrate — the
repairs arrived — but an implementation defect. The dense per-generation decoder
pre-loaded its known-source pivots ONLY when the generation's first repair
created its matrix; because source and repair symbols interleave and reorder on
the wire, a generation's own non-lost sources routinely arrive AFTER that first
repair, and those late sources were recorded for delivery but never injected into
the live matrix. The matrix therefore kept K_g − present unknowns instead of `h`,
the reported deficit inflated by the late-source count, and the surplus repairs
were linearly redundant (they re-derived already-received sources). The
invariant, now enforced: every received source is fed into every live generation
matrix whose span covers it, at the moment it arrives (the unit equation
e_c·x = data), so the unknown space always equals the true hole set. With this,
`repairs_useful` rose to 66–72 % of fed and the deficit collapsed to the hole
count, restoring the model's `h`-repairs-per-generation prediction.

This fix is necessary but does not by itself deliver the FEC-over-ARQ throughput
crossover (Section 8's optimization presumes the proactive budget is IN FLIGHT
when the hole is exposed). The measured residual is a transport-timing race: the
proactive repair for a generation is paced out and arrives ~a generation-span
after the generation's sources, so the receiver's reactive deficit report (which
fires the instant the hole is seen) still triggers a round-trip-bound repair
before the now-useful proactive repair can decode the hole. At RTT 100/10 % this
pins the proactive fraction at 0.13–0.28 and FEC/ARQ at 0.58–0.88. Closing it is
a substrate problem — co-schedule the proactive budget in the same flight as the
sources — not a decoder or a rate (`r`) one: raising `r` post-fix LOWERS
throughput (extra coded congests the droppable datagram path), confirming the
bottleneck is delivery timing, not coding quantity.

### 14.32 The ARQ over-request: the reactive deficit must be requested once per RTT, not once per report

The §14.31 residual was re-examined at L1 (2026-07-08) and partly RE-ATTRIBUTED.
Two corrections. First, the instrument: the receiver-side `present_at_stall`
probe — the count of frontier holes with a proactive repair already buffered —
was reading the *default* `(0,0)` for the dense generation decoder (it never
implemented the probe), so "proactive repair is never present" was in part a
measurement artifact. With a real probe, `present_at_stall` is nonzero and rises
as the generation size G shrinks (repair for a smaller generation flows sooner).

Second, and materially: the dominant loss at high loss/RTT was NOT the proactive
repair arriving late but the REACTIVE ARQ being OVER-REQUESTED. The deficit
`d_g = K_g − rank(g)` is honest (rank counts buffered repair), but the receiver
re-reports it on every sub-RTT decode-progress event, and the sender's in-flight
subtraction resets each report, so the sender re-emits ~the full deficit faster
than a round-trip can reflect the symbols already sent. This is a control-loop
instability, not a coding one: the request rate exceeds the feedback rate. The
model correction is a request-side stability condition — a generation's deficit
may be acted upon at most once per RTT (the time for its recovery symbols to
arrive and be reflected in the next report), with a brief coalescing window so
in-flight/just-arrived repair shrinks the request before it fires:

  request_g(t) = max(0, d_g(t) − in_flight_g),   acted on at most once per SRTT.

MEASURED (c2r100l10, systematic single path): unbounded, `recovery_coded` = 30 703
for a 6 k-symbol object (≈5 ARQ/source), throughput 0.32 Mbit/s; bounded to the
once-per-SRTT honest deficit, `recovery_coded` = 437, throughput 0.913 Mbit/s
against a pure-ARQ 0.919 — i.e. FEC/ARQ 0.35→0.99, from round-trip-flood to
PARITY. Because FEC's advantage over ARQ (Section 14.7) is realized only when
recovery is NOT round-trip-bound, an unbounded reactive request erases the entire
FEC premium: the arm was paying ARQ round-trips AND FEC overhead. A decisive
FEC>ARQ crossover additionally requires `present_at_stall` to dominate (proactive
present for nearly all holes); with the request bounded, a smaller G lifts it
(present_at_stall 1→16) and yields a slight 1.04× edge at RTT200/10 %, but not
dominance — that remains open. The interspersed trailing-window repair proposed to
force in-flight presence is refuted at the transport layer for two structural
reasons (it cannot emit during a send stall; and a repair window narrower than the
generation forms a disjoint linear system that cannot combine with the reactive
generation repair), leaving smaller-G proactive as the only fungible lever.

### 14.33 Present-at-stall is real but self-defeating on a single path: the presence/throughput tension

Section 14.32 left the crossover as a `present_at_stall`-dominance problem. This
section resolves what happens when you actually force presence, and it is a
NEGATIVE result with a precise cause. The mechanism is a dedicated proactive-repair
pacer that emits repair for a generation while it is still FILLING — coded over the
retained contiguous prefix `[anchor, anchor+w)` but expressed at the full generation
matrix width `G` (a wire `coded_width = w` field zeroes columns `[w, G)`), so every
symbol for a generation keys to one `(anchor, G)` system and combines fungibly (no
cross-width stranding, the defect that refuted the sub-generation inline repair of
§14.31). Paced independently of source intake and of the ack-clock, the covering
equation is buffered at the receiver BEFORE the in-order frontier reaches the hole,
and a decoder change (deliver a source the instant its pivot row becomes an isolated
unit row, not only at full generation rank) turns that buffered equation into an
early recovered hole. The instrument confirms the intent: `present_at_stall` rises in
every measured cell (present-fraction e.g. 0.04→0.26 at RTT100/10 %, 0.00→0.23 at
RTT200/10 %) and the proactive fraction rises (reactive `recovery_coded` falls, e.g.
435→335 at RTT200/10 %).

But throughput does NOT follow presence — it regresses 3–21 % and, at RTT200,
occasionally WEDGES (a generation never completes). The cause is a bandwidth identity
the earlier sections did not make explicit. On a single path there is ONE CC-paced
send budget shared by source and repair. To be PRESENT at the stall, repair must be
sent EARLY — concurrently with the source of the very generation it protects — so
early repair displaces source send capacity, the in-order frontier LAGS (measured
frontier gap 0→507 symbols), and goodput falls by more than the round-trips saved are
worth. The round-trip the presence eliminates is cheap here because the baseline
already recovers most holes from late-but-still-proactive repair with zero per-seq
ARQ (systematic mode never resends a source; `source_n = 0` throughout). Formally,
presence `P` and frontier rate `R_f` trade off as `R_f = R_cc·(1 − φ_early(P))`,
where `φ_early` is the fraction of the paced budget spent on early (filling-generation)
repair to achieve presence `P`; raising `P` raises `φ_early` and lowers `R_f`. The
model correction: **on a single path, present-at-stall and throughput are in direct
tension — the crossover is not reachable by re-timing repair earlier on the same
link.** The resolution the tension points to is orthogonal capacity: emit the early
proactive repair on a SECOND path (cross-path fungible repair, Section 16.3) so
presence is bought without displacing source on the first — i.e. the crossover is a
multipath-aggregation result, not a single-path timing result. Testing that is gated
on FEC first reaching parity-or-better single-path in the same cell (it does not
here: single-path in-order systematic FEC runs at 0.7–0.9× ARQ across
RTT∈{100,200}×loss∈{2.6,5,10}%), so it is left open. The pacer is retained
env-gated, default-off, as a documented negative result.

---

## 15. The Unified Sliding-Window Model (Blocks and Streams as Two Knobs)

The document so far has treated FEC and ARQ as one correction stream (Section
5) but has left a second split standing: the transport still carries *two*
FEC data paths, chosen once per tunnel by protocol hint. This section removes
that split. It shows that the BLOCK path and the WINDOW path are not two
codes but one sliding-window RLC evaluated at two settings of two continuous
knobs — window advance and repair timing — and that block mode is the
limiting case of streaming under exactly the σ → 0 collapse the paper already
uses for the end-of-stream burst (Section 14.29) and the χ glide (Section
14.26). Making them one code makes *per-stream* triangles possible: one
tunnel carrying a tight-δ realtime flow and a loose-δ bulk flow over the same
paths at the same time, which the global-mode design structurally cannot do.
(A measured amendment, Section 15.7: the two knobs capture the shared coding
*algebra* but not the delivery *semantics* — retention policy turned out to
be primary — and Section 15.7 further resolves it INTO the triangle: it is ρ, not a new dimension.)

This is a design section (no measured implementation yet); it reuses the
existing `RlcEncoder`/`RlcDecoder` (raptorpath-math/src/rlc.rs), `TaperFunction`
(Section 4), `derive_window` (Section 8.8), and the Section 13.8 scheduler,
and states honestly what it costs and buys.

### 15.1 The Defect: One Triangle per Tunnel

Two independent FEC data paths exist today, selected per tunnel by the
protocol hint:

- **BLOCK mode.** Accumulate K source symbols into a block, FEC-encode the
  block (RaptorQ / Reed-Solomon / block-RLC backends), send source + repairs,
  and recover holes reactively with the batch-ACK ARQ of Section 14.27. A
  dedicated block decoder runs per block.
- **WINDOW / streaming mode.** A sliding-window RLC (the `RlcEncoder` /
  `RlcDecoder` of raptorpath-math/src/rlc.rs) emits repairs continuously per
  the taper τ(t) and recovers holes reactively via the NACK / SACK-gap path
  (Section 14.27's window analogue, P10b).

Because the choice is **global to the tunnel**, the transport commits to a
single point of the bandwidth/latency/reliability triangle (δ, ρ, r) of
Section 1.4. A tunnel in block mode is optimising one (δ, ρ, r); a tunnel in
window mode is optimising another. Neither can carry a realtime stream (tight
δ, small W, high r) *and* a bulk stream (loose δ, large W, r → 0) at the same
time over the same paths. That is the core defect: the triangle is a
per-tunnel constant when it should be a per-stream one.

### 15.2 The Unified Sliding-Window RLC Model

Fix a single mechanism: the sliding-window RLC of Section 3.2, exactly as
`RlcEncoder` implements it. The encoder holds a **live window** of the source
symbols whose sequence numbers lie in [w_start, w_start + W). A **repair** is
a random linear combination over GF(256) of the live window,

```
  p = Σ_{j ∈ window} c_j · s_j ,   c_j = gf256::generate_window_coefficients(w_start, W, k)
```

(`RlcEncoder::generate_repair`), carrying its (w_start, W, repair_index) so
the decoder can reconstruct the coefficients. The window advances by dropping
symbols below a new oldest sequence (`RlcEncoder::advance`), and source data
is retained for exact ARQ resends (`RlcEncoder::get_source`). Two continuous
knobs parameterise the whole design:

```
  (a) advance / overlap.  Per emitted window state, move w_start forward by a
      source positions, a ∈ [1, W]. Define overlap o = (W − a)/W ∈ [0, 1−1/W].
        a = 1  (o → 1):  slide-by-1  → consecutive windows overlap in W−1
                          symbols; a symbol is covered by ~W successive windows.
        a = W  (o = 0):  RESET       → consecutive windows are DISJOINT; a
                          repair never mixes symbols across the boundary.
                          Disjoint windows ARE blocks (K = W).

  (b) repair schedule.  A per-window-lifetime measure dμ(t) of repair mass,
      total r per source symbol (Section 4.3):
        continuous:  dμ(t) = τ(t) dt = A(1−q)^t dt,  A = r·q   → streaming
        spike:       dμ(t) = (r·W) · δ_Dirac(t − W)           → block batch
```

**A third axis this section originally missed: retention policy.** As first
written, this section claimed the two knobs above were sufficient — that block
and window mode "are one sliding-window RLC at two settings of two continuous
knobs." A subsequent experiment (the windowed-RLC-all-profiles run, Section
15.7) refuted the sufficiency: the two production modes also differ in
*reliability policy* — whether un-acked source symbols may be **evicted**
(window mode force-advances past MAX_WINDOW_SIZE and force-delivers past
unrecoverable holes) or must be **retained until acked/decoded** (block mode's
Section 14.27 ledger). That policy is a delivery-semantics contract, not a
schedule, and switching it changes outcomes categorically (10/10 completions
→ 0/10 DNF, Section 15.7). The knob algebra of (a) and (b) stands; it is just
not the whole state. Section 15.7 amends this section accordingly.

**One decoder subsumes both — algebraically.** `RlcDecoder` runs incremental Gaussian
elimination over equations keyed by sequence number: a source symbol is an
identity equation, a repair is the window combination above. It recovers a
hole the instant the pivot table spans it — the number of linearly
independent equations touching the unknown reaches the number of unknowns.
This single procedure *is* both decoders of Section 15.1:

- **Block decode** is the special case where the fed equations partition by
  disjoint window (o = 0): the K × K submatrix over one block reaches full
  rank exactly at the classic "K-of-N present" condition, and the block's
  pivots complete together.
- **Streaming decode** is the overlapping case (o → 1): pivots complete
  incrementally as repairs arrive, each covering the whole live window
  (Section 3.2 window-fungibility).

There is no second code and no second decoder — *as decoders*. But "one
decoder subsumes both" is a statement about the shared linear algebra, not
about the delivery semantics wrapped around it: the same decoder embedded in
an evicting pipeline and in a retaining pipeline produces categorically
different transports (Section 15.7). Block and stream differ in (a), (b),
**and** in retention policy.

### 15.3 Block Mode as a Limiting Case

The reduction is exact. Take the unified encoder of Section 15.2 and set

```
  BLOCK     = ( advance a = W  [reset, o = 0],
                repair schedule = a spike of mass r·W at the window boundary )

  STREAMING = ( advance a = 1  [slide, o = 1−1/W],
                repair schedule = τ(t) = A(1−q)^t at rate r,  A = r·q )
```

Both feed the *same* `RlcDecoder`. What differs is (a) which symbols a repair
may combine — a reset window's repair is a random linear combination over
exactly the K = W symbols of one block, i.e. a block fountain/RLC repair;
a slid window's repair is a combination over the W most-recent symbols — and
(b) *when* the r·W repairs per window are emitted.

**Block is the σ → 0 limit of the continuous schedule.** The block batch
"emit nothing until the window is full, then dump r·W repairs at the
boundary" is the repair schedule collapsed to a Dirac spike of mass r·W. This
is precisely the limiting-case pattern the paper already uses twice:

- Section 14.29 meters the end-of-stream completion budget B_tail = r_tail·W
  as a Stieltjes measure B_tail · dχ_trunc over a source-position kernel of
  width W/4, and calls the one-shot burst its **σ → 0 (width → 0) limit**.
- Section 14.26's χ glide raises Bulk's r from 0 to the tail-budget rate over
  the final ~1.5 SRTT, with the discrete burst as the width → 0 endpoint.

The block repair batch is the *same* width → 0 spike of the *same* mass r·W —
here applied at every window boundary (period W) rather than only once at
end-of-stream. Block mode is therefore not a different mechanism; it is
streaming with the repair kernel's width taken to zero and its period set to
W. The continuous knob is the kernel width σ (and the advance a); block and
stream are its two endpoints, with every intermediate (a partial-overlap
window emitting a narrow but non-zero repair burst) a valid operating point.

**The latency difference falls out of (a) and (b).** Under the boundary
spike, no repair for source symbol s_i exists until its window fills, so s_i
has *no proactive protection* until W − 1 further source symbols arrive:

```
  block fill latency   L_fill = W · t_sym = W / send_rate      (before ANY repair)
  streaming fill latency ≈ 0   (τ(0) = A > 0: protection from offset 0)
```

L_fill is the same quantity as the Section 8.8 W_lat term and the Section
14.5 W·t_sym recovery span — but paid **up front as encode latency**, not as
recovery span. Streaming pays ≈ 0 fill latency and instead *spreads* the same
r·W repairs across the window lifetime (each early repair covers fewer
already-present symbols; the Section 4.4 taper-never-zero and Section 14.24
encoder-lag considerations apply). The trade is continuous in a: at a = 1 the
fill wait is ~t_sym, at a = W it is W·t_sym, and every batch size in between
interpolates.

**Where each is optimal — as a knob setting, not a mode.** Batching
amortises: one decode per W symbols instead of incremental GE per symbol
(Sections 9, 14.17), and one batch-ACK per block (Section 14.27) instead of
per-symbol SACK accounting. So

```
  block-like (large a, spike):  optimal when W·t_sym ≪ latency budget AND
                                amortisation matters (high send_rate, non-
                                negligible per-symbol decode/ACK cost).
  stream-like (a = 1, τ(t)):    optimal when W·t_sym is a meaningful fraction
                                of the budget (tight δ, low rate, or high RTT).
```

This is exactly the Section 8.8 W* binding logic read through the advance
knob: loose-δ Bulk rides a large W with the overhead/latency slack to batch
(→ block-like), tight-δ Realtime rides a small W at slide-1 (→ stream-like).
The current binary mode is just these two endpoints with the interior of the
(a, σ) square deleted.

### 15.4 Per-Stream Triangle Multiplexing

Once block and stream are one code, a tunnel need not pick one triangle. Give
each application stream m ∈ {1..N} its own sliding-window context with its own
triangle (δ_m, ρ_m, r_m, W_m): W_m from `derive_window` at δ_m (Section 8.8),
r_m from the controller at δ_m and the stream's own operating point (Section
8.4), advance/repair schedule from where (δ_m, W_m) lands on the Section 15.3
knobs. Each context is an independent `RlcEncoder` / `RlcDecoder` pair.

**Independent coding = budget isolation.** A repair for stream m combines only
stream m's live window, so a bulk stream's burst of losses draws down only the
bulk stream's repair budget — it cannot consume the realtime stream's
repairs. This is the property the global mode cannot provide and the reason
mixed-δ streams need separate contexts (below).

**The scheduler multiplexes; it does not block-align.** There are no blocks to
align across streams: a small message emits its symbols on arrival, into its
own window. The Section 13.8 objective already ranks a mixed source+repair
symbol stream across paths; it gains a per-stream weight. For a symbol i
belonging to stream m = m(i), the per-symbol scheduling cost becomes

```
  cost_i = w_lat^{m} · u_m(i) · E_i  +  w_bw^{m} · r_m

    w_lat^{m}, w_bw^{m} : stream m's hint weights (Section 13.8; Realtime
                          1/0, Balanced ½/½, Bulk 0/1)
    u_m(i)              : δ-urgency of symbol i — a monotone function of how
                          close i is to stream m's deadline relative to δ_m.
```

A principled u_m reuses existing machinery: the Section 14.26 exposure kernel
χ evaluated at stream m's δ_m and remaining slack, so a symbol whose recovery
is about to become serial for a *tight-δ* stream outranks a bulk symbol with
ample slack. The scheduler then interleaves all streams' source and repair
symbols across paths in urgency order, subject to the Section 13.8 per-path
capacity and (for any block-like stream) the in-order delivery-unit coupling
already derived there.

**N = 1 recovers today's behaviour.** One stream, u ≡ 1, weights = the tunnel
hint: cost_i reduces to the Section 13.8 objective verbatim, and the single
context reduces to the current single global-δ mode. The unification is a
strict generalisation — the present design is its degenerate point.

**Coding gain vs isolation — stated honestly.** Per-stream contexts *lose*
cross-stream coding gain: a single shared window over stream A ∪ B would let
one repair cover a hole in either, and the combined loss process has lower
relative variance (statistical multiplexing), so a shared window needs
slightly less *total* overhead for the same aggregate reliability — the
Section 8.4 margin scales as z·√(ε σ² / (W(1−ε))), and W_A + W_B > either W
alone, so the shared-window margin is O(1/√W) smaller. Against that, the
isolation failure of a shared window is a *δ violation on the tight stream*:
a loose-δ flow's loss burst consuming the shared repair budget pushes the
realtime flow past its δ_m — a latency-class breach the triangle treats as
categorical, not marginal. Trading a bounded O(1/√W) overhead saving for a
categorical latency-class guarantee favours **isolation whenever the streams'
δ differ materially**. When the δ's are equal (homogeneous streams), they
*should* share a window — which is simply N = 1 at a larger W, no contradiction.

**UEP as a named advanced variant.** The alternative that keeps shared coding
gain *and* δ-differentiation is unequal error protection: one shared window,
but repair coefficients weighted to preferentially protect the tight-δ
symbols. It recovers the O(1/√W) gain, at the cost of a coupled encoder
(repairs are no longer independent per stream, a shared-window decode failure
can strand both classes, and the weighting is an extra continuous knob to
tune and estimate). Per-stream contexts are the **recommended default** — they
are the principled construction (clean isolation, and they reuse the existing
single-window encoder N times with no new coding machinery); UEP is future
work for capacity-critical links where the coding-gain term is worth the
coupling.

### 15.5 Cost and Benefit (Honest)

**What it costs.**

- **Per-stream state.** N encoder/decoder contexts, N triangles, N
  `derive_window` / controller evaluations, and a stream demultiplexer on the
  receive side. Memory and decode work scale with the number of *active*
  streams, not tunnels.
- **Scheduler changes.** Section 13.8 must carry per-stream weights and the
  u_m urgency term, and the interleaver must tag every symbol with its stream
  id (already needed for demultiplexing).
- **The block backends.** RaptorQ and Reed-Solomon are *not* sliding-window
  RLC. Two honest options: (i) retire them, accepting RLC's higher decode
  overhead (Section 9) as the price of one code path; or (ii) keep them as the
  o = 0 (reset), spike-schedule special case of Section 15.3 — a "large-W
  batched-repair" backend selected when a stream lands on the block-like
  corner — behind the same context interface. Option (ii) preserves RaptorQ's
  low codec overhead for bulk while still presenting one decoder abstraction
  to the scheduler; it is the recommended migration target.

**What it buys.**

- **Mixed-δ streams on one tunnel** — the defect of Section 15.1 removed: a
  realtime and a bulk flow share the paths with their own triangles, isolated
  budgets, and urgency-ranked multiplexing.
- **One decoder, block and stream as continuous limits** — fewer latent bugs.
  The two worst FEC bugs the L1 harness found were both artefacts of the *two
  separate paths*: (1) a **dead reactive-repair path** in window mode —
  `WindowNack` deprecated with no producer, the SACK-gap wiring cut, the
  sender draining NACKs only after a TUN read, so *only* proactive FEC ever
  repaired a window-mode loss (P10b); and (2) a **mis-wired BlockStart**
  (ADR-0008) — the sender never emitted `BlockStart` and the receiver had no
  match arm, so block decoders were created with `source_symbols = 0` and
  silently produced empty data. Neither failure mode exists in a single
  code path where source and repair feed one incremental decoder.
- **The split is not even delivering its intended benefit** — the measured
  motivation. At C2 (100 Mbit, 10 ms RTT, GE 1.3%/50%), 1200 B messages at
  50/s (goal-gate L2 workstream 2), block mode (Bulk) held a **91 ms p99**
  tail while window mode (Realtime) — the mode whose entire purpose is the low
  tail — sat at **513 ms p99**, at equal p50. The mode meant to be the
  low-latency path had the *worse* tail, for two path-specific reasons the
  unification erases: 508-byte window-mode MTU symbols fragmenting inner
  segments (P9a) and window mode's late-maturing NACK path (P10b, item above).

### 15.6 Migration Sketch

Paper-level, not code. Three phases, each shippable and reversible, ordered so
that the risky decoder change lands first behind the existing behaviour.

```
  Phase 1 — unify DECODE on the sliding-window RLC decoder.
      Route both paths' received symbols through one RlcDecoder. Block mode
      becomes "feed a disjoint window, decode when the block's pivots
      complete" (Section 15.2). Behaviour-preserving: the reset/spike setting
      reproduces block decode exactly. Retires the separate block decoder.

  Phase 2 — express block mode as batched-repair, large-W streaming.
      Move the block ENCODER onto the unified (advance, repair-schedule)
      knobs: block = (reset advance, spike of mass r·W at the boundary),
      Section 15.3. RaptorQ/RS survive as the o = 0 spike-schedule backend
      (option (ii) above) behind the context interface. The binary hint
      switch becomes a point in the continuous (a, σ) square.

  Phase 3 — per-stream contexts + scheduler urgency weighting.
      Give each stream its own context and triangle; extend the Section 13.8
      objective with the per-stream weights and u_m urgency term (Section
      15.4). N = 1 stays bit-identical to today; N > 1 unlocks mixed-δ tunnels.
      UEP (shared window, weighted repairs) is a later, optional variant.
```

**Already exists** (reused, not built): `RlcEncoder` / `RlcDecoder` with
GF(256) window combinations and incremental Gaussian elimination
(raptorpath-math/src/rlc.rs); `TaperFunction` for τ(t) (Section 4); the
block-mode batch-ACK ARQ (Section 14.27) and window SACK-gap recovery (P10b);
`derive_window` for W_m (Section 8.8); the Section 13.8 multipath scheduler
and its delivery-unit coupling.

**New** (to build): the (advance, repair-schedule) knob generalisation of the
encoder emit loop; the per-stream context table and receive-side stream
demultiplexer; the per-stream weight and u_m urgency term in the scheduler;
and the RaptorQ/RS wrapper that presents the o = 0 spike-schedule backend
through the context interface (or their retirement, per Section 15.5).

### 15.7 Amendment: Retention Is the Triangle's ρ, Not a New Axis (measured)

This subsection amends Sections 15.2–15.3 in light of a negative experiment
(branch `exp/windowed-rlc-all`, goal-gate 2026-07-05) that tested the
unification's implicit premise directly.

An earlier draft of this amendment called retention "a third, primary
axis" with two values {evict | retain-until-acked}. That was a new binary
where the model already has a continuum: **retention is the triangle's ρ,
realized by T_cut** (Section 6.1 age-based give-up; Section 6.2 receiver
pruning). The sent-data store's removal rule is: remove on ACK, or when
the entry's age exceeds T_cut(ρ) — with ρ = 1 giving T_cut = ∞ (ack-only,
the bulk contract) and ρ < 1 giving bounded retention continuously.
"Reliable" and "lossy" are not policies to switch between; they are the
ρ → 1 limit and the finite-T_cut interior of one dial. A corollary that
the current window pipeline violates: give-up must be AGE-based (T_cut,
from ρ), never SPACE-based — buffer fullness is a flow-control signal
(backpressure), not a licence to destroy data. The measured failure below
is exactly a space-based eviction masquerading as a reliability policy.

**The experiment.** Production Bulk/Auto were switched onto the window
pipeline — `is_window_mode` extended from Realtime-only to all hints, RLC
selected — exactly the "one sliding-window code for everything" this section
argues for. Result at C2 (100 Mbit, 10 ms RTT, GE 1.3%/50%, seed 42, rp-native
`perf`, 1.8 MB × 10, same binary A/B):

```
  block-mode Bulk (RaptorQ + §14.27 batch-ACK ARQ):  10/10 complete,
                                                     mean 0.895 s (16.1 Mbit/s)
  window-pipeline Bulk (RLC, the realtime pipeline): 0/10 — ALL DNF
                                                     (600 s wall)
```

**Root cause — a policy, not the code.** The window pipeline is
*loss-tolerant by design*: the sender never blocks on window fullness — past
`MAX_WINDOW_SIZE = 200` it force-advances, **evicting un-acked source
symbols** that can then never be regenerated or retransmitted — and the
receiver **force-delivers past unrecoverable holes** on reorder-buffer expiry.
Both behaviours are *correct* for Realtime's triangle (a drop-tolerant δ: a
stale packet is worthless, so eviction is the right spend of the budget) and
*fatal* for bulk's every-byte contract: at C2's loss rate a 1.8 MB object is
~1520 symbols with ~20 loss events, and any loss not repaired within the ~200
symbols the window spans (≈ one RTT at line rate) is permanently gone → DNF.

**The amendment.** Sections 15.2–15.3 present block and window as *two
settings of two knobs* (advance/overlap, repair schedule) over one decoder.
That is true of the coding algebra and remains the basis for the migration
sketch — but the experiment shows the two production modes ALSO differ in a
third property the knobs do not capture:

```
  retention policy ∈ { EVICT              (advance unconditionally; losses
                                           past the horizon become holes;
                                           bounded memory, bounded delay)
                     | RETAIN-UNTIL-ACKED (advance only on ack/decode;
                                           window fullness becomes back-
                                           pressure on the source, never
                                           data loss) }
```

This axis is **primary, not a tuning knob**: moving a bulk flow across it
flipped completions from 10/10 to 0/10. It is a *delivery-semantics contract*
— the ρ corner of the Section 1.4 triangle made structural — where (a) and
(b) merely reshape latency and overhead within a fixed contract.

**Policy is not codec.** The failure was NOT "RLC is unreliable": RLC with a
retention policy — sent source bytes retained in an ARQ-layer store until
acked, retransmitted until delivered (exactly what the wasm visualizer sim
implements at ρ = 1, and what the Section 6.1 retransmit machinery provides
once it carries data, not just metadata) — is a fully reliable code. The
coding window itself need not be gated: reliability is the ARQ store's
contract, the window is only the FEC horizon (see 16.3).
Conversely RaptorQ blocks are reliable only because the Section 14.27
retention machinery (64-block LRU ledger, batch-ACK diff, fresh-repair
minting) around them makes them so — strip it (the pre-P8 production state)
and block mode abandoned failed blocks too. The reliability axis lives in the
*pipeline policy*; the codec (RLC / RaptorQ / RS / METTLE) is orthogonal
symbol algebra. Any unified design must therefore carry retention as an
explicit per-stream policy parameter alongside (a) and (b) — the per-stream
contexts of Section 15.4 are its natural home (Realtime streams run EVICT at
their δ; bulk streams run RETAIN-UNTIL-ACKED), and the Section 15.6 migration
phases inherit it as a third context field. Section 16.3 builds on exactly
this axis; Section 16.4 explains why the resolution is one
policy-parameterised pipeline rather than mode/backend switching.

---

## 16. Reliable Windowed Multipath: an Order-Statistic Formulation

This section replaces an earlier draft ("Fountain Multipath Aggregation —
Out-of-Order is the Unlock") whose central claim — that *in-order delivery*
is the structural obstacle to heterogeneous multipath aggregation, and that
out-of-order fountain delivery is the unlock — did not survive scrutiny. Two
things overturned it. First, a negative experiment: moving bulk onto the
window pipeline (the paper's own unification, Section 15) regressed 10/10
completions at 0.90 s to 0/10 DNF, because the pipeline's *eviction policy*,
not its ordering, destroyed the transfer (Section 15.7). Second, a sharper
reading of the measured C8 numbers: what caps aggregation is not the in-order
contract itself but **per-path-affine atomic delivery units** — with
cross-path coding over a sliding window, an in-order frontier can aggregate
at the full Σ g_i (Section 16.2). Out-of-order delivery survives only in two
subordinate roles: as the mechanism *within* the coding window, and as a
convenience for native whole-object delivery.

**Epistemic convention for this section.** Every quantitative claim is
tagged:

- **MEASURED** — a number from the goal-gate record (L1/L2 harness, real
  links, seeds stated there).
- **DERIVED** — follows from the stated model (bounds, functionals); no new
  measurement needed.
- **PREDICTION** — awaits the Section 16.6 experiment; falsifiable as stated.

The measured baseline this section builds on (all at L2 workstream 1,
topology C8 = WiFi 100 Mbit / 10 ms RTT / GE ε ≈ 2.5% + LTE 20 Mbit / 40 ms
RTT / GE ε ≈ 4.8%; 50 MB bulk, seed 42):

```
  within-block per-symbol striping     8.8 Mbit/s   (below fast path alone)
  whole-block path affinity (§13.8)   12.6 Mbit/s   (= kernel-MPTCP parity)
  fast path alone                     14.0 Mbit/s
  kernel MPTCP dual                   12.6 Mbit/s
  C7 symmetric dual (control)         23.9 Mbit/s   (MPTCP: 15.4)
```

Notation as in Sections 1.1 and 13.2: g_i = C_i·(1−ε_i) the per-path goodput
[sym/s], d_i the one-way delay, K the object size in source symbols, s the
symbol size, W the window, φ the rateless/codec overhead (Section 9), δ/ρ/r
the Section 1.4 triangle.

### 16.1 Three Regimes, Three Decode Predicates

A transport delivers data in **units** — a packet, a 64 KB block, a coding
window, a whole object. Multipath completion is governed by *which symbols
the decoder must wait for* before a unit (or the stream frontier) can
advance. The three schedules measured at C8 correspond to three decode
predicates, and each predicate has a known queueing-theoretic shape.

**(1) Within-unit striping across paths = fork-join.** The unit's K_u
symbols are split x_i·K_u to path i; the decoder needs **all of these
specific symbols, scattered over paths**. Per-unit completion is a maximum:

```
  T_unit = max_i ( x_i·K_u / g_i + d_i + a_i·RTT_i )            (16.1)
```

— path i's share serialised at its goodput, plus its delay, plus a_i ≥ 0
recovery rounds at *its own* RTT when its share contains a loss (per-unit
loss probability 1−(1−ε_i)^{x_i·K_u}: at C8, 1−(1−ε_B)^{K_u} = 0.94 for a
whole K_u = 56 unit at ε_B = 4.8% — the Section 13.8 measured mechanism). Completion pays the **expectation of a
maximum, once per unit**. This is the classic fork-join queue: for
homogeneous branches the mean response grows as ~H_N (the harmonic-number
law of [Nelson1988]); for heterogeneous branches the slowest leg dominates
and E[max] ≫ max_i E[·] because the max concentrates on the straggler's
*tail*, not its mean — the redundant-request literature quantifies exactly
this penalty ([Joshi2017]). Heterogeneous delays and losses therefore make
striping strictly worse than not using the slow path at all. **MEASURED:
8.8 Mbit/s at C8 — below the 14.0 fast path alone.**

**(2) Whole-unit path affinity + in-order release = resequencing queue.**
Each unit rides one path (block-granular affinity), so units complete
independently at their path's own rate — but they must be *released* in unit
order. The delivery frontier is then a **running maximum** over unit
completion times: frontier(n) = max_{m ≤ n} T_m. This is the resequencing
buffer of the multipath literature ([Xia2003]); kernel MPTCP is the same
queue with unit = packet. The receiver holds an out-of-order unit at most H
(production: 4·SRTT clamped [60, 300] ms) before force-delivering a hole, so
a path may carry ordered units only while its per-unit delivery time stays
within H of the fastest path's. Define the **eligibility set**

```
  E = { i : D_i − min_j D_j ≤ H },   D_i = K_u/C_i + d_i + P_blk,i·2·RTT_i
```

Then for any affinity schedule the sustained in-order rate collapses to the
eligible paths' goodput (DERIVED — the ordered frontier can make progress
at most as fast as ordered units arrive, and an ineligible path's units
arrive as holes, contributing zero or negative useful throughput):

```
  T_inorder ≥ K / Σ_{i ∈ E} g_i                                 (16.2)
```

On sufficiently heterogeneous paths E collapses to {fast} (at C8:
D_B − D_A ≈ 130 ms > H/4 — the slow path is hold-infeasible, Section 13.8)
and the bound degenerates to K/g_A — **the fast path alone**. MEASURED:
12.6 Mbit/s at C8, kernel-MPTCP parity (MPTCP: 12.6), below fast-path-alone
14.0 — both transports sitting on the same resequencing bound. On
*homogeneous* paths E = {all} and affinity aggregates fine — MEASURED: C7
symmetric dual 23.9 Mbit/s vs MPTCP 15.4.

**(3) Rateless coding over a horizon = K-of-N.** Code over a horizon of K_h
source symbols (a window, or a whole object) and pour encoded symbols across
all paths. The decoder needs **any K_h·(1+φ) useful symbols from the pooled
arrival process** — no symbol is bound to a path or a position within the
horizon. Completion is the **K_h·(1+φ)-th order statistic of the superposed
arrival process**, whose rate is Σ_i g_i:

```
  T_horizon ≈ K_h·(1+φ) / Σ_i g_i  +  skew term paid ONCE per horizon (16.3)
```

The skew term (startup d_i spread, one decode pass, geometric short-fall
top-up) is O(RTT_max) and does not scale with K_h. The structural difference
from (1) and (2): there is **no per-unit max and no resequencing coupling**.
The coding gain for multipath is exactly the move from *the expectation of a
per-unit maximum* (paid K/K_u times) to *one interior order statistic of the
pooled process* (paid once per horizon) — a **mean AND variance win, and one
that grows with path heterogeneity**, because E[max] − (interior order
statistic) widens as the per-path delay/loss distributions spread apart,
while on symmetric paths the two coincide and the gain vanishes. That
monotonicity is this section's sharp, testable signature (PREDICTION for our
stack; the functional forms themselves are DERIVED, and are the transport
analogue of the coded-download latency results of [Joshi2014, Joshi2017]).

Summary:

```
  schedule                predicate            completion functional   C8 measured
  ─────────────────────── ──────────────────── ─────────────────────── ───────────
  within-unit striping    ALL of these,        Σ_units E[max_i(...)]    8.8 Mbit/s
                          scattered            (max per unit)
  whole-unit affinity +   units independent,   running max ⇒            12.6 Mbit/s
  in-order release        released in order    K/Σ_{E} g_i              (= MPTCP)
  rateless over horizon   ANY K_h(1+φ) of      K_h(1+φ)/Σ_all g_i +     — (§16.6)
                          the pooled arrivals  skew once
```

### 16.2 The Sliding-Window Realization (In-Order Is Not the Bottleneck)

Regime (3) as stated assumes a horizon. The earlier draft took the horizon
to be the *whole object* — encode the object as one fountain, deliver
out-of-order, decode on total. That is **not implementable as the general
mechanism**: a tunnel does not know object boundaries or sizes ahead of
time, streams are unbounded, and a whole-object horizon means unbounded
encoder/decoder memory and a decode that cannot begin until the end. It
survives only as a special case for the native object API, where the object
is known and bounded (Section 16.6).

**The realistic carrier of the order-statistic gain is a sliding window**
spanning multiple former blocks. Repairs are combinations over the current
window (the Section 15.2 algebra, unchanged); source and repair symbols are
distributed across paths by work-conserving pull (Section 16.6,
prerequisite 2 — each path drains the shared window at its own CC-gated
rate, so allocation converges to per-path goodput without an explicit
splitter); and the receiver maintains an **in-order
delivery frontier** that advances whenever the window *prefix* decodes from
ANY sufficient subset of received symbols. A gap at the frontier does not
wait for a specific path's retransmit: it is filled by whichever path's
repair lands first — the fill rate is the pooled Σ g_i, not the losing
path's RTT. In-order delivery-delay analyses of exactly this construction
(streaming code + in-order release) exist in the literature and show the
frontier delay collapsing once repairs are cross-scheduled [Cloud2014].

This yields the correction that renames this section (DERIVED):

> **In-order delivery is not the bottleneck.** With cross-path coding over a
> sliding window and a retention policy (Section 15.7), the in-order frontier
> advances at ≈ Σ_i g_i — the full aggregate. What caps aggregation in the
> measured system is (i) **per-path-affine atomic units**: a 64 KB block
> whose source symbols ride exactly one path makes the *unit*, not the
> ordering, the thing that serialises at one path's rate and recovers at one
> path's RTT — the eligibility set E and the resequencing bound (16.2) are
> consequences of block atomicity, and MPTCP hits the same bound because its
> unit (the packet with a fixed sequence number) is equally path-atomic once
> sent; and (ii) in the lossy window pipeline, **eviction** — which converts
> a late repair into a permanent hole (Section 15.7).

**Caveat measured after the fact (see Section 16.7).** The claim above —
frontier at Σ g_i — holds only when the window is **rateless-fungible**
(the frontier advances on ANY sufficient K_h(1+φ) symbols). At bulk's
operating point r ≈ ε (~2%), the sliding window is *systematic* (source
striped in sequence order, tiny redundancy), so a source symbol on the slow
path IS a specific in-order position the fast path cannot decode around, and
the frontier reverts to fork-join. Section 16.7 states this r-regime
qualifier precisely and gives the measured pivot (RWM Phase B: symmetric
1.41×, heterogeneous 12.5). Two dials — the reorder horizon H and the repair
rate r — restore aggregation; the triangle's δ picks which.

The earlier draft's theorem ("FEC-multipath beats ARQ-multipath **iff**
delivery is out-of-order") overclaimed. What the eligibility argument
actually proves is bound (16.2) *for per-path-affine atomic units*; it says
nothing about in-order delivery of a cross-path-coded window, which evades
the bound not by dropping order but by making symbols fungible across paths
*within the horizon*. Out-of-order delivery retains two legitimate roles:

- **Within the window horizon** symbols arrive in any order, from any path,
  and are fungible inputs to the prefix decode — this is where the
  order-statistic gain (16.3) lives, W symbols at a time.
- **Whole-object atomic release** remains a convenience for the native
  object API (`raptorpath perf` reassembles by (obj_id, chunk_idx) and
  tolerates arbitrary reordering) — it lets the frontier machinery be
  skipped entirely, but it is an API simplification, not the source of the
  aggregation gain.

### 16.3 The Missing Quadrant: Reliable Windowed Multipath

Two independent design axes emerged from the measured record — and neither
is the codec. The **reliability policy** axis (Section 15.7):
{ EVICT | RETAIN-UNTIL-ACKED } — a pipeline policy; RLC under retention is
fully reliable, RaptorQ under the pre-P8 pipeline (no ledger) silently
abandoned failed blocks. The **unit structure** axis (Section 16.1):
{ atomic blocks, path-affine | sliding window, cross-path striped }. The
production system and the experiments populate three of the four quadrants:

```
                       atomic blocks,             sliding window,
                       path-affine                cross-path striped
  ─────────────────────────────────────────────────────────────────────────
  EVICT                (uninteresting: lossy      production REALTIME.
  (lossy by policy)    blocks would drop 64 KB    Correct for its δ —
                       at a time)                 MEASURED working; single
                                                  path by construction (F3:
                                                  no striping sender exists)
  ─────────────────────────────────────────────────────────────────────────
  RETAIN-UNTIL-ACKED   production BULK/AUTO       ── EMPTY ──
  (reliable)           (RaptorQ 64 KB blocks +    Reliable Windowed
                       §14.27 batch-ACK ARQ).     Multipath (RWM):
                       Correct single-path —      the code this section
                       MEASURED 10/10 @ 0.90 s;   proposes. Exists nowhere
                       multipath capped by        in production today.
                       (16.2) — MEASURED 12.6
  ─────────────────────────────────────────────────────────────────────────
  and the measured anti-diagonal: EVICT × window for BULK — the
  exp/windowed-rlc-all experiment — 10/10 @ 0.90 s → 0/10 DNF (Section
  15.7). Reliability policy is a PRIMARY axis, not a tuning knob.
```

**RWM defined.** Reliable Windowed Multipath = RETAIN × sliding window ×
cross-path striped:

1. **Retention — at the ARQ layer, not in the coding window.** The window
   slides freely: it is only the FEC horizon (fungible repair coverage for
   recent, not-yet-localized losses). Reliability is the contract of a
   **sent-data store**: every sent source symbol's bytes are retained
   until ACKed or until age exceeds T_cut(ρ) (Section 6.1) — ρ = 1 gives
   T_cut = ∞, i.e. ack-only removal; never removal by space pressure; a
   SACK-confirmed hole that has aged out of the window is recovered by a
   targeted retransmit of exactly that symbol from the store, on the best
   available path — once a loss is localized, fungibility has no value and
   one exact symbol is the cheapest correction (Section 5's
   corrections = repairs ∪ retransmits, with the window/ARQ split falling
   out of loss-localization). Store fullness (~ a few × BDP of plain
   bytes, no coding cost) becomes **backpressure** on the source (flow
   control), never data loss — the exact place the evicting pipeline
   instead destroys data. The Section 14.27 batch-ACK ledger (retained
   source, ACK-diff, targeted resend) is the donor: it already implements
   this for blocks. A consequence: the W_mp bound of 16.5 SOFTENS from a
   requirement to a continuous trade — W sets the share of recovery that
   is fungible-repair (in-window) versus targeted-ARQ (aged), so an
   undersized window costs recovery latency on aged holes, not
   correctness.
2. **Striping — one continuous placement law, no load regimes.** No
   proportional (and certainly no equal-weight) splitter, and no
   backlog-vs-partial-load case split either: every symbol is placed by
   a single marginal-cost rule — the Section 13.8 objective completed
   with live congestion and fate terms,
   `cost_i(s) = w_lat·E_i(load) + w_bw·r_i + w_div·ρ_fate(s,i)`,
   sampled as P(i) ∝ exp(−cost_i/T). Because E_i includes the path's
   CURRENT queueing delay (the dq the delay-based CC already measures),
   the apparent regimes are equilibria of this one rule, not modes:
   under light load the empty best path has lowest cost and traffic
   concentrates there; as its queue builds, its marginal cost rises
   continuously until the next path's cost is crossed and symbols spill
   gradually; under backlog all marginal costs equalize at the capacity
   waterlines — water-filling is the FIXED POINT of marginal-cost
   equalization, and per-path CC-gated pull is its distributed
   implementation (each path's token availability IS its marginal-cost
   signal). The temperature T is the one dial from strict ordering
   (T→0) to burst-decorrelating dithering (deterministic order maps
   consecutive window positions to one path, so a GE burst punches a
   contiguous window hole; dithering scatters the damage — PREDICTION,
   measure before claiming). Repair placement uses the SAME law: the
   old hard avoid-rule becomes the continuous ρ_fate penalty
   (fate-correlation with the symbols the repair covers). Block
   affinity dissolves per-symbol (no block unit remains to bind).
   Convention note: hard sets remain legitimate inside derived BOUNDS
   (e.g. the 16.2 eligibility set is an analysis device); the
   no-cutoffs rule binds MECHANISMS — no control law may case-split.
   No symbol is bound to a path.
3. **Frontier decode.** The receiver delivers the in-order prefix the
   moment any sufficient symbol subset decodes it; holes at the frontier
   are raced by all paths' repairs (Section 16.2).

RWM is the transport that realises predicate (3) for bulk: completion
governed by the pooled order statistic (16.3) window-by-window, target rate
Σ_i g_i.

**What exists today, honestly.** Production window mode has **no
multipath**: every source symbol goes to `best_source_path()` — the single
lowest-cost path with cwnd headroom, spilling to the next-best only on
saturation — and repairs to `best_repair_path()`; nothing stripes (MEASURED
code fact, Phase-0 reconnaissance of the windowed-RLC experiment). Bulk
never touches the window pipeline (Section 15.1). And the closest existing
relative of RWM is not in production at all: it is the **wasm visualizer
simulation**, which runs a sliding-window RLC for all hints with ρ = 1 —
retransmit-until-delivered, no forced eviction — i.e. a *reliable sliding
window*, but single-path. The sim therefore models RWM's reliability
semantics and window mechanics while modelling neither its striping nor,
for bulk, anything production actually runs — a correspondence gap the
paper's earlier sections (which lean on the sim) inherit and that is now
stated explicitly.

**The fungible construction, made concrete (the coded-object mode this task
builds).** §16.7 measured that the reliable striped window, at bulk's
systematic operating point, caps at fork-join parity (≈ ×0.92 in the oracle,
0.76–0.81 at L1) — and localized *why*: a **systematic** window sends raw
source symbols in sequence order plus sparse repair, so a source symbol
striped onto the slow path IS a specific in-order position the fast path
holds no degrees of freedom to decode around. It arrives at the slow path's
rate; the frontier waits for it; the aggregate collapses to the order-
eligible set E = {fast}. That specific-symbol **long pole** is the whole of
the cap. The empty quadrant is filled by removing it at the source:

> **Emit CODED symbols only.** In coded-object mode every transmitted symbol
> is a random linear combination over the *current sliding window* of K_W
> source symbols (the §15.2 RLC algebra the encoder already generates as
> "repairs"; the change is that this mode sends **no raw systematic source at
> all** — the systematic pass-through is switched off). Each coded symbol
> carries its window coordinates + coefficient seed on the existing repair
> wire, so it is self-describing to the decoder.

Three consequences make this the fungible regime (3) of §16.1, not the
fork-join regime (1):

1. **No symbol is specific → no long pole (DERIVED).** Any K_W linearly
   independent coded symbols — *from any path, in any order* — reconstruct
   the K_W window sources by one bounded Gaussian elimination. Nothing waits
   for a *particular* symbol, so a slow-path symbol is never a position the
   fast path must stall on; it is one interchangeable degree of freedom among
   many. Reception overhead is effectively MDS over GF(256): expected excess
   ≈ 1/255 of a symbol for independence (tighter than RaptorQ's +1–2 per
   block), so completion is the K_W(1+φ)-th order statistic of the *pooled*
   cross-path arrival process — rate Σ_i g_i (16.3), not the straggler's rate.
   This is exactly why the systematic window caps at ≈0.92 (a source symbol is
   a fixed position → §16.2 fork-join bound) while the coded window reaches
   Σ g_i (no fixed position exists). The two differ only in whether a
   transmitted symbol is raw-and-positional or coded-and-fungible; everything
   else — retention, placement, frontier — is shared.

2. **Window sized to the cross-path lag: W ≈ W_mp (DERIVED + oracle-checked).**
   The §16.5 lower bound W_mp ≳ Σ_i g_i·(RTT_max + t_slack) ≈ 600 symbols at
   C8 sets how far the window must span so a slow-path symbol's lag still
   falls inside the fungible horizon (covered by a later coded symbol, not
   stranded as aged ARQ). The independent-GE oracle
   (`multipath_oracle.rs::oracle_c8_fungible_wmp_window`), run at the exact C8
   netem params, confirms the *finite* window suffices: a coded fungible
   window aggregates to **×1.19** (the goodput ceiling ×1.195) for every
   W ≥ 384 at a modest repair rate r ≥ 0.05, and to ×1.15–1.18 even at r = 0
   for W = 600–1024 (MEASURED, oracle). The earlier §16.7 sweep's ×0.99 at
   H = 256 was simply W < W_mp; at and above W_mp the fungible ceiling is
   reached. So the production target is proven reachable by *this specific
   finite-window design*, not only by the unbounded whole-object horizon.

3. **Object completion out-of-order; ARQ backstops the tail (DERIVED).** The
   receiver decodes each window on any sufficient K_W-subset and delivers the
   recovered symbols out-of-order, reassembled by offset (the §16.7 Phase C
   delivery policy, H → ∞ for a file). The retention/ARQ layer (§16.3 point 1,
   Phase A) remains the backstop for the last partial window and any window
   that never accumulates K_W in-flight.

**Decode-cost bound (MEASURED).** Coded-only decode is one incremental RLC
Gaussian elimination over W_mp, cost ~O(W²) in the window. The direct
throughput measurement (§16.5: 1200 B symbols, single core, encode+decode)
gives **708 Mbit/s at W = 512** and 1.28 Gbit/s at W = 256 — 7–35× headroom
over the 20–100 Mbit/s lossy cells at W ≈ W_mp, so compute does not bind
below roughly gigabit line rates at these windows.

**Scope caveat — bulk / loose-δ only (DERIVED).** Coded-only pays a K_W-symbol
**decode latency before *any* byte is delivered**: nothing decodes until the
window accumulates K_W(1+φ) independent symbols. That is correct for a bulk
object (no consumer reads offset k before the file is whole — §16.7's
decode-on-total equivalence) but **wrong for realtime or in-order low-latency
byte streams**, which need the systematic window's immediate per-symbol
pass-through. So coded-object is a *bulk-object, loose-δ* mode, composed
behind a flag with the reliable window (Phase A) and out-of-order delivery
(Phase C); realtime and in-order-stream profiles stay on their existing
systematic modes untouched (§16.4's one-pipeline, per-stream-parameterised
thesis — coded-vs-systematic is one more parameter point, not a new pipeline).

**Status — built, oracle-confirmed reachable, MEASURED at L1, and REFUTED
there (goal-gate "Fungible Frontier", 2026-07-07).** The coded-object mode was
implemented exactly as above (coded-only wire symbols, W widened to W_mp = 640,
retention/ARQ backstop, out-of-order object completion; behind a
`window_coded_only` flag composing with `window_reliable` + out-of-order) and
measured at C8 = c2+c3 netem (independent qdiscs, 50 MB native `perf`). The
prediction that it would strictly beat fast-path-alone (15.68 Mbit/s) is
**REFUTED**:

```
  coded-only C8 het  (c2+c3) dual    3.9 Mbit/s mean (median 4.5, stdev high, x6)
  coded-only C7 sym  (c2+c2) dual    5.5 Mbit/s mean                        (x3)
  coded-only         SINGLE c2       12.9 Mbit/s                            (x2)
  systematic fast-path-alone c2      15.68 Mbit/s   (the bar)
```

Three facts localize the failure and are sharper than "it did not aggregate":

1. **The coded-object mechanism is CORRECT.** Loopback and netem both complete
   with all bytes, decode-on-K, zero systematic passthrough. It is a working
   fungible transport, not a broken build.
2. **The bottleneck is NOT heterogeneity/straggler — it is cross-path coding
   itself.** Coded-only *single-path* runs at 12.9 (near systematic, the ~18 %
   gap is the O(W)-per-symbol codec cost of making 100 % of symbols
   W_mp-wide combinations instead of ~2 %). But *dual* is WORSE than single on
   BOTH symmetric (5.5) and heterogeneous (3.9) paths. Adding any second path
   drags the coded pipeline down — the opposite of the oracle's monotone
   aggregation. So the independent-GE oracle's ×1.19 is **not realized on the
   real sliding-window + per-path-timing + CC + ARQ stack**.
3. **The mechanism of the drag (DERIVED from the above).** A coded symbol is a
   combination over the sender's window *at send time*; a symbol striped to a
   path arrives one path-delay later, by which point the sender's window (and
   the receiver's decoded frontier) has advanced — on the fast path by
   ~Σg·RTT ≫ W_mp — so a second path's symbols land covering windows already
   decoded past (redundant) or misaligned, contributing little useful pooled
   rank while adding decode load, cross-path reordering, and NACK/recovery
   churn (each transient undecoded seq is congestion-throttled ARQ, §16.7).
   The oracle abstracts all of this away (instantaneous pooled decode over a
   fixed horizon); the gap between ×1.19 (oracle) and ×0.25 (L1) is precisely
   that abstraction.

**Honest §16 position (updated).** The §16.3 empty quadrant now has a *correct
implementation* and an *independent-GE proof of achievability* (×1.19), but
the L1 transport does not realize it: heterogeneous aggregation-above-fast-path
remains **OPEN and unrealized in production**, and coded-only over the current
per-path-timed sliding window aggregates *negatively*. What the oracle's ×1.19
and the L1's ×0.25 together establish is that the missing piece is not
fungibility-in-the-abstract (built, proven) but **cross-path window alignment**
— coding horizons whose per-path arrivals pool over the *same* live window —
which the send-time-windowed RLC does not provide. That is a named,
scoped next mechanism, not a knob; §16.5's W_mp sizing is necessary (it lifted
the number from 2 to 4.5) but not sufficient.

**MEASURED (generation coding + per-generation deficit feedback, branch
`feat/gen-deficit-feedback`, 2026-07-07).** The stable-anchor generation codec
(§16.3 "fungible construction") and its named missing mechanism, per-generation
**deficit feedback**, are now both implemented and measured at L1. The deficit
loop CLOSES the prior build's multi-generation stall: the receiver reports each
frontier generation's residual rank `K_g − rank_g` (`WindowDecoder::rank_in` +
a `GenerationDeficit` control message, paced on decode progress and a periodic
~SRTT timer), and the sender emits exactly that residual per generation
(`generate_repair_for`, in-flight-accounted so it never double-sends). C8 (c2+c3)
50 MB transfers now **complete 6/6**, where the feedback-free build stalled at
1–2 generations. **But the DECISIVE C8 goodput FAILS the >15.7 Mbit/s bar and
the aggregation factor is 1.00 (NONE):** matched at 50 MB, dual C8 = 10.97 and
single-path c2 = 10.95 Mbit/s. The binding constraint has moved one layer down,
from a transport deadlock (now fixed) to the **RLC generation-DECODE throughput**:
completion goodput scales inversely with G (G=384 → 3.4, G=192 → 11, G=96 → 12.6
Mbit/s at C8) — the O(G) total-decode-work signature — and at the oracle's
aggregating G=384 the decode is so slow (3.4 Mbit/s, network-INDEPENDENT: in-proc
loopback gives the same) that the pipeline STALLS at 20 MB (0/3, timeout) and
cross-path aggregation has zero headroom (the receiver already saturates its
decode on one path). The `RlcWindowDecoder`'s incremental GE with per-pivot
`BTreeMap<u64,u8>` coefficients runs ~200× below the §16.3 "708 Mbit/s" dense-GE
decode-cost figure that the achievability argument assumed. So the oracle's ×1.19
remains **proven-but-not-realized**, now bottlenecked on **generation-decode
performance (a fast dense/SIMD GF(256) solver)** rather than transport plumbing —
a scoped codec-perf step. Regression: the non-generation modes are untouched
(single-path coded 15.66, C7 21.25 Mbit/s at 50 MB).

**MEASURED (fast dense generation decoder, branch `feat/fast-gen-decoder`,
2026-07-07).** The named codec-perf step above is DONE: a dense per-generation
GF(256) Gauss–Jordan decoder (`GenerationDecoder`: fused `[coeffs|payload]`
rows, incremental RREF over the SIMD `mul_acc_slice` kernel, per-generation
independent, decode-on-K) replaces the sparse `RlcWindowDecoder` on the
generation path. Microbench (1200 B, single core, AVX2): at G=384 it decodes at
**83 Mbit/s vs the sparse path's 3.1 (27×)**, clearing the link rate — decode is
**provably no longer the binding constraint**, and the oracle's G=384 config
that DNF'd at 20 MB now **COMPLETES** at L1 (16/16 isolated single-object 25 MB
transfers). **But the DECISIVE C8 goodput STILL FAILS the >15.7 bar: dual C8
G=384 = 8.90 Mbit/s (8/8), single-path c2 = 9.11, aggregation factor 0.98
(NONE).** The binding constraint has moved one layer DOWN AGAIN — off decode
(now 9× the achieved rate) onto the **coded-datagram transport control loop**.
A client trace shows the uploader `tx_paused` ~90 % of the time with coded
emission ack-clocked at ~1.3× delivered: coded symbols ride QUIC's DROPPABLE
datagram path, so the pacing is deliberately clocked to the delivered-ack rate
to avoid overrunning it — and pushing harder confirms the tension (forcing
~57 Mbit/s coded via `RWM_GEN_RATE_FLOOR` DROPS C8 to 5.2 Mbit/s: overrun →
datagram drops → generations stall). Combined with per-generation decode-on-K /
deficit-feedback RTT serialization (the M=2 pipeline hides only 2 generations of
the slow path's 40 ms straggler latency; deepening to M=8 lifts it only to 10.6),
the generation transport tops out at ~9–12 Mbit/s and the second heterogeneous
path adds nothing. So the oracle's ×1.19 is **still proven-but-not-realized**,
now bottlenecked on the coded-datagram emission model rather than decode or
plumbing — the fundamental tension of racing a rateless coded stream over a
droppable, bandwidth-limited, high-RTT datagram path. Closing it needs a
different emission substrate (systematic-symbol pass-through to kill the
decode-on-K latency, or coded-over-a-reliable-substream), not more decode or
feedback work. Regression: non-generation modes untouched (the dense decoder is
gated on generation mode; single-path / C7 systematic and coded-only paths use
the unchanged `RlcWindowDecoder`).

**A cheaper realization — SYSTEMATIC source + deficit-driven cross-path REPAIR
(oracle-VALIDATED; a BUILD recommendation, not built; branch
`feat/oracle-systematic-repair`, `temporal_oracle.rs` PART 3).** The "systematic
pass-through" substrate named just above is now made precise and adjudicated in
the same faithful oracle class. The coded-only generation design pays its three
L1-killing costs *because every symbol is a coded combination*: whole-generation
**O(G²) decode**, **decode-on-K latency**, and an ack-clocked **coded-datagram**
loop. A systematic realization avoids all three at once yet keeps the same
cross-path fungibility. In the model, the K source symbols are striped
work-conserving (one path per source; the fast path pulls ∝ its rate) and a
delivered source is one degree of freedom used **directly** — zero decode,
out-of-order. A path with no fresh source emits **windowed REPAIR** (an RLC over
the live window `[F−W_span, F)`, W_span ≈ W_mp ≈ 500 at C8) on the best path; a
received repair is one fungible dof for ANY missing source in its window. The
receiver decodes only the **local deficit** (a tiny dense solve over the current
holes) and completes at rank K = K/(Σg_i). Four questions, all
DERIVED/MEASURED in-oracle at C8 (c2+c3), r = 0.06:

1. **Aggregation (Q1).** C8 het **×1.188** (99.4 % of the ceiling ×1.195); C7
   symmetric control **×1.992** (~2×, no drag). Recovery uses zero per-seq ARQ.
2. **Repair volume bounded (Q2).** φ = repair/K = **0.060** (= r, the loss-FEC
   baseline; bounded), and the *structural* deficit-driven cross-path repair
   → **0** as K grows: φ_tail = 0.0030 (5 MB) → 0.0000 (25/50/200 MB), because
   the repair needed ≈ the slow path's in-flight window (≈ g_slow·OWD_slow ≈ 32
   symbols), which is INDEPENDENT of K. There is **no structural deficit** — the
   fast path does not re-cover a growing fraction.
3. **Decode cost small (Q3).** The dense solve's max concurrent unknowns is
   **7–10 symbols** (1.8–2.6 % of a G = 384 generation), K-INDEPENDENT (7 at
   25 MB, 7 at 50 MB, 10 at 200 MB). It is O(deficit²) over ≈ 10 unknowns, NOT
   the whole-object O(384²) of coded-only, and it does not grow with the object.
4. **Contrast / provisioning (Q4).** The paper's ≈0.92 fork-join long pole is an
   **in-order-delivery artifact**: in-order + finite store + path-affine =
   **×0.932** (reproduced), but the same config with **cross-path repair**
   advances the frontier fungibly = **×1.188**, and the bulk out-of-order regime
   the design targets avoids the pole even *without* cross-path repair (affine
   out-of-order = ×1.171). The explicit knob is the proactive repair rate: r ≲ ε
   strands mid-object losses past the W_span horizon (DNF), r ≥ ~1.5·ε reaches
   the ceiling.

**Verdict — BUILD.** Systematic source + deficit-driven cross-path windowed
repair reaches ×1.19 with bounded φ (→ r), a tiny K-independent deficit-decode
(~10 vs 384 unknowns), no decode-on-K (source delivers on arrival), and no
per-seq ARQ — strictly cheaper than coded-only on exactly the two axes that sank
its L1 build (decode cost, delivery latency), while matching its aggregation. The
minimal production change is a *modification* of the merged generation machinery,
reusing striping + deficit-feedback + the dense GF(256) decoder: (i) send raw
**systematic source** as primary (drop coded-only primary), delivered
out-of-order; (ii) emit **windowed RLC repair** at proactive r ≳ ε plus a
deficit-driven top-up on the best path, reusing `GenerationDeficit` re-scoped to
per-window rank deficit; (iii) **decode the deficit only** with the existing
dense decoder, sized to the ~10-symbol hole set, not to G; (iv) **no per-seq
ARQ**. Honest scope: this is an independent-GE model (same fidelity class as the
corrected oracle) and does not simulate the QUIC datagram control loop — but the
two coded-only L1-killers are *structurally absent* (systematic source rides the
reliable path with zero decode; the solve is ~10 symbols), so the residual L1
risk is materially smaller than for coded-only. `cargo test -p raptorpath-math`
green incl. `temporal_oracle` 7 tests (3 corrected-oracle + 4 systematic-repair).

**MEASURED (production build + L1, branch `feat/systematic-repair`,
2026-07-07).** Built behind `--window-systematic-repair` as the modification
above (`GenerationEncoder::new_systematic` → proactive budget `ceil(len·r)`;
raw source on the wire as primary striped ∝-goodput; deficit-only decode via the
dense decoder; per-seq ARQ off). The design's **structural claims hold in
production**: C8 (c2+c3, 50 MB ×6, G=480/M=2, r=0.15) COMPLETES **6/6 (dnf:0)**,
the deficit loop is essentially IDLE (holes covered inline by proactive r), and
decode is a non-factor — so the two coded-only L1-killers are confirmed removed,
and the anti-aggregation *drag* is gone (C8 dual **15.0 Mbit/s** sits AT the
single-path rate 15.2, vs plain-systematic 12.11 and coded-only 8.90, both of
which fell BELOW single). BUT the **DECISIVE >15.7 bar is NOT met (×0.99
aggregation)** — and the residual constraint is now cleanly the **per-connection
transport control loop, not the FEC**: a SYMMETRIC-path control (C7 c2+c2) also
does NOT aggregate (**15.4**, ×1.02 with two IDENTICAL paths), a single perf
connection extracts only ~15 Mbit from a 100 Mbit link, and loosening the store
(M=4, M=8) OVERRUNS the droppable datagram path to DNF rather than aggregating.
The oracle's independent-GE model (each path delivers at its link goodput, store
unbounded) does not capture production's per-connection cwnd/pacing ceiling and
its bounded store pruned by the IN-ORDER cumulative ack (which serializes the
paths even though delivery is out-of-order — sender `tx_paused` ~87 %). Honest
FAIL-WITH-MECHANISM: closing it is a TRANSPORT change (decouple backpressure from
the in-order frontier; grow the datagram send window without overrun), below this
design's FEC scope. Full record: goal-gate "Systematic+Repair — PRODUCTION BUILD
+ L1 MEASURED".

**C8 Final update (MEASURED, `feat/c8-final`).** The one remaining structural
blocker to BDP-scale (small-G) operation — a generation-decoder **frontier-advance
deadlock** — is FIXED. Root cause: the receiver learned a generation's width K_g
only from a repair header, so a generation whose entire `ceil(G·r)` proactive
repair was LOST never entered the deficit map — it reported ZERO deficit while its
hole wedged the in-order frontier forever (only bit **small G**; at G=480 the whole
budget is never lost). Fix: the receiver now **seeds K_g = G for any provably-full
generation from the primary seqs alone**, so the ack-clock-INDEPENDENT deficit loop
always funds the frontier hole. A/B on the same VM: clean base **WEDGES at G=96**
(no 50 MB run completes in 210 s); the fix completes **6/6**. C8 (c2+c3, 50 MB ×6,
store=2·G, r=0.15) now completes **6/6 at every G ∈ {96,192,384}** with low variance
(stdev 1.3–1.9 s), best **15.07 Mbit/s at G=384** — but **still < 15.7 (aggregation
factor 0.98** vs single c2 = 15.36). With the deadlock gone, the residual is
unambiguously the **per-connection PROCESSING ceiling** (systematic-repair extracts
~15 Mbit from one 100 Mbit path — window/BW/RTT-independent, loss-sensitive — and a
heterogeneous second path adds nothing), a transport-substrate limit ~4.5× below
native quinn, one layer below the FEC and below this deadlock. (The plain-reliable
symmetric win C7 = 22.3 ×1.43 is intact and untouched — it does NOT run
systematic-repair; systematic-repair itself has never aggregated, ×1.02.) Honest
FAIL-WITH-MECHANISM. Full record: goal-gate "C8 Final".

**Corrected oracle — the ×1.19 achievability claim, re-adjudicated (goal
`feat/oracle-temporal`, `raptorpath-math/tests/temporal_oracle.rs`).** The
×1.19 achievability above came from an oracle
(`multipath_oracle.rs::oracle_c8_fungible_wmp_window`) that
**abstracted away time**: it credited every arrived coded symbol with useful
rank over its whole window, with no notion that a coded symbol is a
combination over the sender's window *as of its send time* and lands a
path-delay later.  A corrected oracle was built that models the send-time
window, per-path one-way delay, a finite store, per-generation rank decode,
and the production reliability layer.  Two findings, one correcting the record
and one rendering the verdict:

1. **The pure temporal-drift hypothesis does NOT, by itself, explain L1
   (DERIVED).**  At W ≈ W_mp = 640 the send-time-vs-arrival drift is
   negligible — a slow-path symbol's window-top exceeds the receiver frontier
   by ≈ W − D·owd ≈ 640 − 7·20 ≈ 500 ≫ 0, so nothing is stranded — and a
   faithful *ideal* fungible model aggregates.  This is consistent with the L1
   observation that the drag is **W-insensitive** (W = 200 → ×0.13, W = 640 →
   ×0.29, W = 2048 → ×0.16 in raw goodput; all deep sub-parity).  The L1
   ×0.26 is therefore not an information-theoretic alignment barrier of coded
   multipath; it is a **realization pathology**: beneath the fungible coding
   the production stack runs a *per-seq* reliability/ARQ/reorder layer, so a
   moving coding anchor makes each window's per-path shares behave
   **path-affine** (the fast path's fresh symbols code the current window and
   cannot retroactively supply a prior window's stranded position — only a
   targeted, congestion-throttled per-seq ARQ can), and the per-seq in-order
   delivery beneath the code imposes a per-window cross-path reorder/ARQ tax
   present *even on symmetric paths*.

2. **With that realization layer modeled, the corrected oracle REPRODUCES the
   L1 refutation (MEASURED against L1).**  A single fitted constant — the
   throttled-recovery collapse stall (the ADR-0046 congestion-multiplier
   collapse), 190 ms — reproduces *both* L1 numbers at once, the het/sym ratio
   falling out rather than fit:

   ```
     signature                    L1 measured   corrected oracle
     C8 het dual / fast-alone         ×0.26          ×0.259
     C7 sym dual / fast-alone         ×0.36          ×0.362
     coded single / systematic        ×0.85          ×0.94 (codec cost only)
     dual < single on BOTH?            YES            YES  (0.259 & 0.362 < 1)
   ```

   Only a corrected oracle that reproduces the failure is trustworthy to
   judge the fix.  This one does.

The earlier ×1.19 "achievable" claim is therefore **corrected**: it was from
an oracle lacking temporal alignment and the per-seq realization layer, so it
described an *idealization*, not the transport that was built.  What is true —
and now measured in the faithful oracle — is that the idealization's ×1.19 is
recoverable, but only under a **stable coding anchor** (next paragraph), not
under the moving sliding window that was actually shipped.

**Verdict — heterogeneous aggregation IS achievable, via generation-based
coding with a stable anchor (MEASURED in the faithful oracle; a BUILD
recommendation, not built here).**  In the corrected oracle, replacing the
moving window with **fixed generations** — partition the source into
generations of ≈ W_mp symbols, code coded symbols *within* each generation
(a stable target), stripe each generation's coded symbols ∝ goodput across
paths, decode generation *g* on any K_g independent symbols from any paths,
pipeline generations — removes both drag mechanisms *by construction*: a
slow-path symbol for generation *g* stays useful until *g* decodes regardless
of arrival time (no stranding, no per-seq throttle), and a lost or late symbol
is replaced by the next coded symbol for the *same* generation from *either*
path (fungible cross-path recovery).  The measured result at C8 (K = 20 000,
r = 0.10):

```
  aligned generation coding, C8 het, sweep G × pipeline depth M:
    G = 256  384  512  640  768 1024      (all M ∈ {2,3,4})
       1.19 1.19 1.19 1.18 1.17 1.17      → best ×1.194 at G ≈ 384–512
  C7 symmetric control (G = 640, M = 3):  ×1.96   (ideal 2.0, no drag)
  lever decomposition (C8 het, W = 640):
    moving anchor + throttled recovery, M=1  ×0.21   (== L1 refutation)
    moving anchor + throttled recovery, M=3  ×0.60   (pipelining alone: partial)
    stable anchor + fungible recovery,  M=1  ×1.13   (stable anchor alone: works)
    stable anchor + fungible recovery,  M=3  ×1.18   (full fix → ceiling)
```

Aligned generations reach the goodput ceiling (×1.195) *without* the
dual-worse-than-single drag, and the lever decomposition isolates the cause:
the **stable anchor is the dominant lever** (×0.21 → ×1.13), pipelining is
secondary (×0.21 → ×0.60).  So the grounded position is:

- **Heterogeneous aggregation-above-fast-path is achievable (DERIVED + oracle-
  MEASURED), and the required production mechanism is named:**
  generation-based cross-path fungible coding with a **stable per-generation
  anchor** (generation size ≈ W_mp / 384–512 symbols at C8, pipeline depth
  M ≥ 2, ∝-goodput striping of each generation's coded symbols, out-of-order
  per-generation decode, fungible cross-path recovery — *no* per-seq targeted
  ARQ beneath the code).
- The shipped coded-*sliding*-window (moving anchor) cannot reach it and is
  correctly recorded as REFUTED at L1 (×0.26); the barrier it hit is the
  moving anchor + per-seq throttled recovery, not fungible coding as such.

**Production realization — IMPLEMENTED (branch `feat/generation-coding`).** The
stable-generation design above is now built in the transport, not only the
oracle. A `GenerationEncoder` (`raptorpath/src/fec/generation.rs`) partitions the
object's source symbols into FIXED generations of `RWM_GEN` (default 384)
symbols and emits RANDOM-LINEAR-COMBINATION coded symbols WITHIN each generation
(a stable anchor: `window_start = g·G` never moves). It is deliberately built on
the EXISTING coded-only wire and decoder: a generation-coded symbol is an RLC
repair over the fixed span `[g·G, g·G+gen_len)`, carrying the identical
self-describing header (`window_start` = the generation anchor, `window_count` =
K_G), so the existing `RlcWindowDecoder` solves each generation's K_G×K_G system
independently the instant K_G independent symbols for that anchor arrive
(decode-on-K), with ZERO decoder change. It is gated behind a
`window_generation_coding` flag composing with the object/perf (bulk, loose-δ)
path — realtime and the in-order TCP-in-tunnel stream are untouched. The three
production specifics: (a) **generation framing** — coded symbols round-robin
across the M in-flight generations, each provisioned to `ceil(len·(1+r))` before
recovery, so no generation is a moving anchor; (b) **generation-level ARQ
granularity** — the per-seq targeted-retransmit/NACK layer (the sent-data store,
the ADR-0046 throttle) is switched OFF in this mode (the receiver installs no
NACK producer); a short generation is recovered by MORE coded symbols for THAT
generation (fungible, from either path), never by resending a specific seq;
(c) **pipeline** — an ack-clocked flow-control window bounds coded to
`ack·(1+r) + W_inflight` ahead of the decode frontier (bounding the QUIC
datagram buffer), so M generations stay concurrently in flight. Verification:
`fec::generation` codec unit tests (decode-on-K, out-of-order, per-generation
independence, pipeline bound) and `perf_loopback_generation_object` /
`perf_loopback_generation_dual_path` — a generation-coded 1 MB object completes
over a real (in-process) dual-path QUIC link with every byte recovered purely by
per-generation GE and NO per-seq retransmit (≈15–17 Mbit/s loopback).

**L1 status — MEASURED, and it does NOT yet beat fast-path-alone: a
fail-with-mechanism (goal-gate "Generation Coding", 2026-07-07).** At C8 = c2+c3
netem the production build does NOT clear the 15.7 Mbit/s fast-path-alone bar.
The mechanism is localized and sharp, and it is NOT the generation design:
the **first generation decodes correctly end-to-end on real netem** (the full
stable-anchor + out-of-order + generation-recovery + no-per-seq-ARQ pipeline
works over real per-path timing/loss), but **generations after the first stall**
— the cumulative-ack frontier advances one generation then wedges. Instrumented,
the arriving coded for generation *g* > 0 span only the first few source symbols
of the generation (decoder rank stalls ≈ K_G past the frontier) despite ample
coded emission, zero send failures, and no datagram-size drops on the object
path — i.e. a per-generation source-span anomaly in the emission/intake/advance
interaction that appears only under real one-way delay (it does not reproduce on
the zero-RTT loopback, which completes). So the ×1.19 remains **oracle-proven and
loopback-realized but not yet L1-realized**: the generation *codec* and the
*stable-anchor mechanism* are correct (proven), while a residual transport
plumbing bug in the multi-generation pipeline over real-RTT netem holds
production heterogeneous aggregation OPEN. This is an honest fail-with-mechanism,
not a refutation of the design.

### 16.4 One Pipeline, Not Mode Switching

The two-pipeline structure (Section 15.1) invites a tempting patch: keep
both pipelines and *switch* between them — or between codecs — as conditions
change. The record says the switching itself is the defect, for four
reasons, while *codec choice per stream* is legitimate and cheap:

**(a) No cross-code algebra.** A repair symbol of one code cannot help
decode another code's in-flight data. Any mid-stream switch therefore either
strands every in-flight symbol of the old code or forces a drain barrier
(stop, flush, restart) — a latency cliff by construction. MEASURED: the P9a
bring-up found window-mode backend switching had to be **pinned off** in
production — a switch rebuilt the encoder with sequence numbers restarting
at 0 while the receiver's delivery/ACK state (highest_delivered_seq, SACK
ranges) and the sender's retransmit buffer kept the old numbering, leaving
the ACK/NACK repair machinery blind for ~a full window of traffic; at lossy
cells that repair blackout wedged the inner TCP for minutes (the hazard
note in net/mod.rs stands).

**(b) State does not transfer.** The estimator, controller, and ARQ ledgers
carry code-specific semantics — a block ledger keyed by (block, batch) has
no meaning to a window's per-seq SACK state, a window's taper phase has no
block analogue. A switch discards exactly the state the next seconds of
recovery need.

**(c) The existing auto-switch is a threshold machine.** Block mode today
selects its backend by hard loss thresholds: ε̂ < 0.01 → RaptorQ,
0.01 ≤ ε̂ < 0.12 → RLC, ε̂ ≥ 0.12 → METTLE, re-evaluated with a ≥ 5 s
minimum interval plus debounce (`config.rs`, `backend_selector.rs`). This
violates the paper's own no-hard-cutoffs convention (the same discipline
that replaced the r* cutoff branch in Section 8.4 and the hard saturation
cap in Section 14.21.1) and carries the standard oscillation hazard: an ε̂
sitting near a threshold buys periodic switches, each paying cost (a). An
honest note: this existing mechanism deserves reconsideration on the same
grounds as this section — it predates the switching analysis above.

**(d) Two pipelines means everything is built twice — and debugged never.**
MEASURED: window mode's reactive repair path was *dead code* (no NACK
producer, SACK gaps ignored, drain gated on TUN reads — P10b) while the
block pipeline received P7 pacing and P8 ARQ; the two worst L1 FEC bugs were
each single-mode artefacts (Section 15.5). Every subsystem — CC coupling,
ARQ, reorder delivery, MTU handling — exists twice and diverges.

**The principled resolution** (strengthening Section 15.4): **one pipeline,
parameterised by the policy axes** — retention (Section 15.7), advance/
overlap a, window W, the (δ, ρ, r) triangle, and striping — with codecs as
per-STREAM plug-ins behind the context interface, chosen once at stream
setup by workload economics: RaptorQ where its near-optimal overhead at
large K pays (bulk objects), RLC where the natural sliding window fits
(streams), and **never switched mid-stream**. A new stream gets a new
context, so no cross-code boundary ever exists *inside* a stream — cost (a)
becomes structurally impossible rather than carefully avoided. Profiles
(Realtime/Bulk/Auto) become parameter points of one machine, not modes of
two; mixed-profile streams coexist on one tunnel via the Section 15.4
per-stream contexts.

### 16.5 Choosing W for Multipath: a Fourth Bound

Section 8.8 derives W* from three bounds — the overhead knee W_over, the
latency ceiling W_lat, the burst floor W_bur. RWM adds a fourth, a **lower**
bound: the window must span the cross-path recovery horizon, or the frontier
starves on skew.

The argument (DERIVED): a hole at the frontier is raced by repairs that are
combinations over the *current window*. The race takes up to one feedback
round on the slowest useful path, ≈ RTT_max, plus scheduling slack t_slack
(one repair-generation + pacing interval, of order the fast path's RTT).
During that race the aggregate keeps arriving at Σ_i g_i; if those arrivals
overrun the window span, the encoder faces exactly the choice Section 15.7
forbids resolving by eviction — and under retention the window instead
stalls the source (backpressure), collapsing the pour to stop-and-wait on
skew events. So sustained Σ g_i aggregation requires

```
  W_mp ≳ Σ_i g_i · (RTT_max + t_slack)        [g_i in sym/s]     (16.4)

  W*   = clamp( max(W_over, W_mp),  min(W_bur, W_lat),  W_lat ) — multipath
         reading of §8.8; single path leaves W_mp = g·RTT ≈ the §14.5 term
```

**Worked C8 example.** g_A = 100 Mbit/s·(1−0.025) ≈ 10 160 sym/s at
s = 1200 B; g_B = 20·(1−0.048) ≈ 1 980 sym/s; Σ ≈ 12 100 sym/s.
RTT_max = 40 ms, t_slack ≈ RTT_A = 10 ms:

```
  W_mp ≈ 12 100 × 0.050 ≈ 600 symbols
```

— i.e. **3× the window pipeline's MAX_WINDOW_SIZE = 200 and above Section
8.8's [16, 512] clamp ceiling**. The current constants would starve RWM at
C8 by construction; this is a paper-level extension of the Section 8.8
derivation (the production `derive_window` is not changed here). The upper
bounds still apply and now genuinely bind: W remains capped by the latency
budget (W_lat), by memory (W symbols retained per stream under the retention
policy), and by decode cost — RLC's incremental pivot GE grows ~O(W²) in
the window, though the measured costs at production windows leave headroom
(MEASURED, P9b non-finding: RLC decode p50 37 µs, p99 < 1 ms; it was
explicitly ruled out as a bottleneck at C2 rates). A direct
decode-throughput measurement at bulk-realistic parameters (MEASURED,
2026-07-06: 1200 B symbols, 2.6 % GE loss, r = 5 %, single core,
encode + decode combined) gives 2.84 Gbit/s at W = 64, 1.28 Gbit/s at
W = 256, and 708 Mbit/s at W = 512 — 7–140× headroom over the 20–100
Mbit/s lossy cells, and reception overhead is effectively MDS
(expected excess ≈ 1/255 of one symbol over GF(256), tighter than
RaptorQ's ~+1–2 symbols per block at K = 56). Compute, not overhead,
is therefore RLC's only scaling cost, and it does not bind below
roughly gigabit line rates at these windows. A W ≈ 600 window at C8's
symbol rate is ~50 ms of traffic and ~0.7 MB retained — well within all
three ceilings; the binding constraint at satellite-class RTT × high rate
would be memory and decode, and (16.4) says such links need either a larger
W or an honest admission that Σ g_i is not reachable there.

**CORRECTION — the 708 Mbit/s figure is a DENSE-solver number; the sliding-
window and generation decoders in production were NOT dense (MEASURED,
2026-07-07).** The throughput quoted above (2.84 Gbit/s / 1.28 Gbit/s / 708
Mbit/s at W = 64 / 256 / 512) was measured with a *dense* Gaussian-elimination
solver. The production **`RlcWindowDecoder`** (used by both the sliding window
and, until this build, the generation-coding decode path) is NOT that solver:
it stores each pivot row's coefficients in a per-pivot `BTreeMap<u64,u8>` and
resolves single-unknowns by cascade — allocation-heavy and pointer-chasing.
Direct microbench of the two on identical generation-coded 1200 B streams
(single core, AVX2) shows the gap widens with the generation size G (= the
per-generation K):

```
   G     dense (Gauss–Jordan, SIMD GF(256))   sparse (RlcWindowDecoder)   ratio
   96              405 Mbit/s                        67 Mbit/s             6×
  192              201 Mbit/s                        16 Mbit/s            13×
  384               83 Mbit/s                       3.1 Mbit/s            27×
  512               66 Mbit/s                       1.7 Mbit/s            38×
```

At the oracle's aggregating **G = 384** the sparse decoder runs at **3.1
Mbit/s — BELOW the 20–100 Mbit/s cells** (it was the binding constraint, not
the network: goal-gate "Generation Coding" measured C8 = 10.97 Mbit/s,
aggregation factor 1.00, network-independent). So the §16.5 "compute does not
bind" claim holds ONLY for a **dense/SIMD GF(256)** solver; **generation coding
REQUIRES it**, and this build ships one (`GenerationDecoder`: dense fused
`[coeffs|payload]` rows, incremental reduced-row-echelon Gauss–Jordan over the
existing SIMD `mul_acc_slice` kernel, per-generation independent, decode-on-K,
out-of-order). The dense decoder clears the link rate at G = 384 (83 Mbit/s,
27× the sparse path) and unblocks *completion* at that G (which DNF'd on the
sparse decoder at 20 MB). The effective ~3.8 GB/s of the dense kernel is
per-call PSHUFB-table-build bound; it is O(G) per delivered byte (O(G²) per
symbol, K/G generations), so the dense figure still falls with G but stays far
above the cells for every G ≤ 512. **This corrects the achievability argument:
the 708 Mbit/s headroom was real but only for the dense solver the generation
path did not use; with the dense solver now in place, decode is no longer the
generation-coding bottleneck (the constraint moves one layer down to the
coded-datagram transport control loop — see §16.3).**

### 16.6 Predictions, Prerequisites, and the Experiment

**Claim ledger.**

DERIVED (no new measurement needed):
- The resequencing bound (16.2): no per-path-affine in-order transport can
  exceed Σ_{i∈E} g_i; at C8's heterogeneity E = {fast}, so 14.0 Mbit/s
  (fast-path-alone) is a ceiling for that whole design class — raptorpath's
  affinity scheduler and kernel MPTCP both sit under it (12.6 ≈ 12.6).
- The fork-join penalty (16.1): striping atomic units across heterogeneous
  paths pays E[max] per unit; consistent with the measured 8.8 < 14.0.
- The frontier rate: cross-path window coding + retention advances the
  in-order frontier at ≈ Σ g_i (16.3), window-by-window (16.4 sizing).

MEASURED (goal-gate, cited above): 8.8 / 12.6 / 14.0 / 23.9 Mbit/s;
MPTCP 15.4 (C7) and 12.6 (C8); the eviction DNF (10/10 → 0/10, Section 15.7); the P9a
switch hazard; no striping sender in window mode; sim-vs-production
correspondence (Section 16.3).

MEASURED AFTER THE FACT (RWM Phase C, see Section 16.7 — this P1 prediction
was tested and REFUTED):
- **P1 — aggregation. REFUTED.** RWM at C8, 50 MB native bulk out-of-order
  (the H → ∞ corner) measured **11.9 Mbit/s** — 0.76× fast-path-alone
  (15.7), i.e. it does NOT cross the (16.2) ceiling; it lands at kernel-MPTCP
  parity (12.6). Out-of-order is 1.42× the in-order mean and far steadier
  (stdev 3.2 vs 6.9) — a real robustness gain — but not the aggregation-
  above-fast-path the prediction claimed. The raise-r companion (r ≈ 0.18)
  did not cross it either (7.9). Full numbers and mechanism: Section 16.7.
- **P1b — cross-path proactive repair on the SPARE path. ALSO REFUTED**
  (goal-gate "C8 Cross-Path Repair", 2026-07-08). The last untested lever: place
  the proactive/deficit repair on the *underutilized* (slow) path — spare-capacity
  placement, `RWM_XPATH_REPAIR` — so a fast-path loss is covered by repair already
  in flight on the slow path WITHOUT displacing fast-path source (the presence⊥
  throughput escape the single-path "present-at-stall" result named). The
  independent temporal oracle CONFIRMS this placement in theory (systematic source
  + fungible cross-path deficit repair, out-of-order, no per-seq ARQ → **×1.188**,
  toward the Σg ceiling ×1.195). At L1 (VM, G=192, 50 MB ×5) it does NOT cross:
  fast-alone 15.18; dual C8 **baseline** plain-systematic **14.70 (0.97×)** is the
  BEST dual, and every cross-path-repair arm is STRICTLY worse (13.6 / 11.3 / 7.5).
  Mechanism: diverting the slow path's capacity from SOURCE to REPAIR loses real
  aggregate throughput, while the fast path already recovers its own losses cheaply
  (r=0.15) so the cross-path repair is largely redundant — a net-negative trade.
  The presence⊥throughput identity holds in the cross-path case too. The gap
  between the oracle's ×1.188 and L1's 0.97× is the in-order cumulative-ack frontier
  serialization (§16.7 / Loss-Recovery defect 2), which caps the slow path's usable
  SOURCE contribution at ~parity, so repair cannot buy back more than it costs.
  **Grounded verdict: heterogeneous throughput aggregation is bounded even with
  working FEC + cross-path proactive repair; the bottleneck is frontier-recovery
  latency, not repair placement or FEC recovery.** *[2026-07-19: era-bound —
  this held at the Cubic-substrate ~15 Mbit operating point; the substrate
  chain beneath it (§16.17, §12.12, §16.19) was found and dissolved later.
  See §17.]*

Superseded PREDICTION (kept for the record, now MEASURED-refuted above):
- ~~**P1 — aggregation.** RWM at C8, 50 MB native bulk: completion goodput
  **strictly > 14.0 Mbit/s** — beyond the (16.2) ceiling of every
  per-path-affine in-order transport measured on this topology — trending
  toward Σ ≈ g_A + g_B.~~
- **P2 — no regression.** C7 symmetric control stays ≈ 23 Mbit/s: where
  E = {all}, affinity already aggregates, and RWM must not lose to it
  (the order-statistic gain → 0 on symmetric paths, so parity is the
  prediction, not improvement).
- **P3 — the signature scaling law.** RWM's advantage over whole-unit
  affinity **grows with path heterogeneity** (sweep RTT_B and ε_B upward
  from C7-symmetric toward C8 and beyond): the E[max]-to-order-statistic
  gap widens with skew while both schemes coincide at zero skew. This
  monotonicity is the falsifiable fingerprint that distinguishes the
  order-statistic mechanism from generic "more tuning" — a flat or
  non-monotone curve refutes the formulation even if P1 happens to pass.

**Implementation prerequisites, plainly.** None of this exists today; the
experiment requires building, in order:

1. **ARQ-layer retention** — a sent-data store in the window pipeline:
   sent source bytes retained until acked or T_cut(ρ)-aged (ρ = 1 ⇒
   ack-only; Section 6.1 — one dial, no reliable/lossy mode), targeted
   retransmit from the store for aged SACK-confirmed holes on the best
   path, store-full ⇒ source backpressure; receiver never force-delivers
   past a hole on a reliable stream. The coding window keeps sliding
   freely (today's retransmit buffer holds metadata only — the data dies
   with window eviction; carrying the bytes is the change). The Section
   14.27 P8 ledger (retained source, ACK-diff, targeted resend, sweep) is
   the natural donor machinery.
2. **A striping window sender** — window mode has none (16.3): stripe
   source + repairs ∝ g_i, reusing the proportional-goodput logic
   `Scheduler::schedule` already applies to block repairs.
3. **Frontier decode** — deliver the in-order prefix on any sufficient
   subset; the RlcDecoder's incremental GE already exposes exactly this
   (Section 15.2); what is new is wiring delivery to pivot-completion of
   the prefix rather than to per-block completion.

Workload: C8 per ADR-0051 (netem harness), `raptorpath perf` native
objects, 50 MB (aggregate goodput) + 1.8 MB × N (completion distribution),
seed 42, one arm at a time against the affinity baseline; C7 as the P2
control; then the P3 heterogeneity sweep.

**What carries over unchanged from the earlier draft.** Single-path bulk
stays ARQ: with N = 1 there is nothing to pool — Σ g_i = g_1 — and the
fountain/window overhead φ is wasted wire against ARQ's retransmit-exactly-
the-erasures, whose mid-stream recovery is completion-free (Section 14.26).
The Bulk strategy therefore remains **path-count dependent**: N = 1 → r* = 0
pure ARQ (the χ glide); N ≥ 2 heterogeneous → cross-path window coding (RWM)
— the same result as before, with "rateless fountain, out-of-order" replaced
by its implementable form.

### 16.7 The Reorder Horizon H as the Aggregation Dial; Ordering as a Delivery Policy

Section 16.2 asserted, unqualified, that "in-order delivery is not the
bottleneck" — cross-path window coding advances the frontier at Σ g_i. RWM
Phase B measured the qualifier the claim was missing.

**MEASURED (goal-gate, RWM Phase B — the striping placement law §16.3, built
and run at C8, seed 42):**

```
  symmetric  C7 (c2+c2):  21.7 Mbit/s = 1.41× fast-path-alone (15.4)  — aggregates
  heterog.   C8 (c2+c3):  12.5 Mbit/s = 0.81× fast-path-alone (15.4)  — FAILS
```

The symmetric win proves the striping MECHANISM is sound; the heterogeneous
failure localizes the missing assumption. §16.2's frontier-at-Σg holds only
when the window is **rateless-fungible** — the frontier advances on ANY
sufficient K_h(1+φ) symbols, so no symbol is a specific position anyone must
wait for. At bulk's operating point r ≈ ε (~2%, loss-matched), the sliding
window is **systematic** (source striped in sequence order, tiny
redundancy): a source symbol placed on the slow path IS a specific in-order
position, and the fast path lacks the coded degrees of freedom to decode
around it. The frontier is then fork-join (§16.1 regime 1, the §16.2 bound),
and the aggregate collapses to the order-eligible set E = {fast} = 14.0.
This **reconciles the section's two prior framings**, each of which was one
regime of a single dial (DERIVED):

- *"out-of-order is the unlock"* (the pre-§16 draft) — TRUE at low r / for
  objects;
- *"in-order is fine; per-path affinity is the bottleneck"* (§16.2) — TRUE
  only in the rateless (high-r) regime, where the window is fungible and
  affinity is the last thing binding.

**H, the reorder horizon, is the continuous dial.** §16.2's eligibility set
E = {i : D_i − D_min ≤ H} already contains it: H is the delivery-latency
budget the CONSUMER tolerates before a lagging path's unit counts as a hole.
It is not binary (DERIVED):

- Small H (tight latency) ⇒ the slow path is excluded, E = {fast}, aggregate
  = fork-join/MPTCP parity — **MEASURED 12.5 at C8**.
- Large H ⇒ the slow path is admitted, completion → K / Σ_{all} g_i.
- **Out-of-order delivery is simply H → ∞** — the limit of the same dial,
  not a separate mechanism.

**The equivalence that makes this precise (DERIVED).** For a bounded OBJECT,
these two are IDENTICAL in completion time:

1. deliver each decoded symbol immediately, out of order, and reassemble by
   offset;
2. deliver strictly in order through a reorder buffer deep enough to hold to
   completion.

Both finish at **decode-on-total** — the instant the last still-missing
symbol decodes anywhere, on any path. The in-order frontier costs throughput
ONLY for a consumer that must eat bytes incrementally, in order, at low
latency (an inner TCP byte stream, live media). A file has no such consumer:
nothing reads offset k before the file is whole. Phase B's 12.5 therefore
measured the frontier under the WRONG completion metric for an object — it
imposed a small-H incremental-consumer contract on a workload whose correct
metric is decode-on-total (H = ∞). **The L0 visualizer, whose completion
metric IS decode-on-total, already shows ×1.18 at C8** (goal-gate L0) — same
topology, same code, the H → ∞ metric.

**Two knobs buy heterogeneous aggregation; δ picks between them.**
Fungibility can be bought with latency OR with bandwidth (DERIVED):

```
  knob  what it buys                        cost           free in    right for
  ────  ──────────────────────────────────  ─────────────  ─────────  ──────────────────
  H     admit the slow path by tolerating   LATENCY        bandwidth  files / loose-δ
        its lag (H→∞ = out-of-order =                                 (bulk objects)
        decode-on-total)
  r     make the window rateless-fungible    BANDWIDTH      latency    tight-δ streams
        so no symbol is a fixed position     (≈ slow path's            (live media,
        (raise r to K_h(1+φ), §16.5)         share ≈16% @C8)           TCP-in-tunnel)
```

At C8 the slow path carries ≈ g_B/Σg ≈ 16% of the symbols, so making the
window rateless-fungible costs ≈ 16% repair overhead (the §16.5 K_h(1+φ)
provisioning) — paid in bandwidth, buying aggregation with NO added latency,
so even a tight-δ in-order stream can aggregate. Conversely H → ∞ buys the
same aggregation FREE in bandwidth, paid in delivery latency a file does not
feel. This is one **(H, r) surface**, and the triangle's **δ selects the
operating point**: loose δ ⇒ raise H (out-of-order the natural limit); tight
δ ⇒ raise r (§16.5). The multipath dial collapses into the same (δ, ρ, r)
triangle as the rest of §15/§16, with H a fourth axis dual to r through the
fungibility each buys.

**Ordering is a per-stream delivery POLICY, orthogonal to the coding
triangle.** It composes with the reliability policy ρ (§15.7) into four real,
all-useful modes (DERIVED):

```
                  reliable (ρ=1, retain)         lossy (ρ<1, evict)
  ordered         file / TCP-in-tunnel           ordered media
  (H = ∞)         (byte stream — must wait)      (live video, skip stale)
  unordered       reliable messaging / objects   datagram / telemetry
  (H = 0 hold)    (reassemble-by-offset,         (fire-and-forget,
                   all delivered)                 newest wins)
```

The two readings of H are dual: the reorder HOLD you impose at the receiver
(H = 0 unordered ↔ H = ∞ strict in-order) is the same quantity as the
delivery LAG you tolerate from a path (the eligibility H of §16.2), read from
the receiver vs the scheduler side. **Ordering and multipath eligibility are
one horizon seen from two workloads.** And crucially **unordered is the
SIMPLER implementation**: it REMOVES the reorder buffer (deliver each decoded
unit the instant it decodes) rather than adding machinery — reinforcing the
profiles-as-parameters thesis (§16.4). RWM Phase C implements exactly this
general unordered-delivery capability on the reliable window as a delivery
policy flag (off = today's in-order); the native object API is its first
consumer (reassemble-by-offset), but message / datagram / RPC / telemetry
streams are equally served.

**Phase C result (MEASURED, goal-gate RWM Phase C — native `perf`, C8 =
c2+c3, 50 MB × 8, seed 42, floor-free binary; single-path fast-alone
15.68 Mbit/s this binary).** The prediction that the H → ∞ corner would
**strictly beat fast-path-alone** is **REFUTED**:

```
  C8 in-order (H bounded)   8.4 Mbit/s mean / 8.1 median   stdev 6.9  (8 runs)
  C8 out-of-order (H→∞)    11.9 Mbit/s mean / 12.0 median   stdev 3.2  (8 runs)
  fast-path-alone (c2)     15.7 Mbit/s
  C7 symmetric o-o-o (ctl) 21.6 Mbit/s                      stdev 0.5  (3 runs)
```

Three things the numbers say, honestly:

1. **H → ∞ does NOT reach Σ g_i, and does not beat fast-alone.** Out-of-order
   completion goodput is 11.9 Mbit/s — 0.76× the single fast path, ≈ kernel-
   MPTCP / whole-block-affinity parity (12.6). The §16.1(3)/§16.2 picture
   that a bounded object at H → ∞ completes at K/Σ g_i is **not realized at
   L1**: the slow path's *source* symbols still gate decode-on-total (at
   bulk's systematic r the window is not fungible, so the fast path cannot
   reconstruct them), and the straggler drag holds the aggregate below one
   fast path. The earlier expectation (and the L0 sim's ×1.18) overstated
   what H alone buys on a real heterogeneous link.
2. **The equivalence holds in direction, not in constant.** Out-of-order and
   in-order-with-retention are *supposed* to be identical (both decode-on-
   total). Measured, out-of-order is **1.42× the in-order mean and ~2× lower
   variance** (stdev 3.2 vs 6.9). The gap is implementation overhead, not
   theory: the in-order reorder buffer accumulates the entire out-of-order
   suffix behind each hole and drains it in bursts, and its recovery
   interacts with the CC/ARQ timing more erratically; removing the buffer
   (H = 0 hold, deliver-on-decode) removes that overhead and its tail. So
   out-of-order is the **more robust** realization of decode-on-total, but it
   moves the median from 8 to 12, not past 15 — it buys stability, not
   aggregation.
3. **The r knob did not unlock it either (at the level tried).** The
   companion raise-r arm — in-order frontier, per-symbol repair floor
   r ≈ 0.18 (the slow path's symbol share) — measured **7.9 Mbit/s**, no
   better than r ≈ 0 (8.4): forcing 18% repair added straggler load without
   making the window fungible enough to recover the slow-path source
   positions from fast-path repairs. Raising r on a genuinely congested
   lossy path spends bandwidth the straggler cannot afford (the same reason a
   blanket reactive-repair floor was measured to *regress* C8 14→9, goal-gate
   Phase C). Whether a much larger r (with repairs pinned to the slow path so
   they are fungible degrees of freedom, not straggler load) crosses
   fast-alone is **open** — neither knob crossed it here.

**Regression control (MEASURED).** C7 symmetric out-of-order = 21.6 Mbit/s
(≈ Phase B's 21.7), stdev 0.5 — where the paths match there is no straggler,
the order-statistic gain is zero, and out-of-order neither helps nor hurts.
The mechanism is sound; C8 heterogeneity is where both knobs fall short.

**Standing interpretation.** Out-of-order delivery is a correct, useful,
lower-variance *general* capability (the four-mode table above — objects,
messaging, datagram), and it is the right delivery policy for bulk objects.
But at C8 it does **not** deliver the heterogeneous aggregation the earlier
draft and the L0 sim advertised: the H → ∞ corner lands at MPTCP parity,
not above the fast path. The honest §16 position is therefore weaker than
"out-of-order is the unlock" and weaker than "either knob aggregates" — it
is: *for a file, deliver out-of-order (it is simpler and more stable); the
heterogeneous-aggregation-above-fast-path result is unproven on this stack by
either H or a modest r, and is a measured open problem, not a demonstrated
win.*

**Oracle adjudication of the L0/L1 contradiction (formula- AND sim-independent
Monte-Carlo; `raptorpath-math/tests/multipath_oracle.rs`).** The L0 sim said
×1.18, L1 measured ×0.76 — a hard contradiction. An independent oracle (models
per-path capacity + one-way delay + GE loss, striped placement, fungible
repairs over a sliding horizon *with eviction*, cross-path ARQ, in-order
frontier decode; calls none of the model formulas or the sim) was run at the
exact C8 netem params. It reproduces BOTH numbers as different transport
configs, and thereby localizes the cause:

```
  goodput ceiling  Σg_i/g_fast                              ×1.195   physical max
  FUNGIBLE cross-path RWM, whole-object horizon (H→∞)       ×1.19    == L0 sim
  ATOMIC path-affine (regime 1/2) + cross-path ARQ          ×0.92    sub-unity
  ATOMIC + SAME-path recovery                               ×0.48–0.57
```

The physically-correct object case (fungible, H → ∞) AGGREGATES to the goodput
ceiling — so §16.1(3)/§16.2's K/Σg_i is REALIZABLE and heterogeneous
aggregation is **NOT fundamentally fork-join-bounded**. L1's ×0.76 sits inside
the *broken-transport* band (between atomic-clean ×0.92 and atomic+same-path
×0.48–0.57), reproducible only by removing fungibility AND cross-path recovery.
So the measured refutation is a **production limitation, not a theorem**:
block/path-affine atomic units (§16.2(i)) + same-path/suppressed recovery +
eviction (§16.2(ii)) — precisely the two caps §16.2 already names.

**Lever ordering (oracle, independent-GE), best→worst — a correction to the
intuition:** (1) **fungible cross-path frontier decode (RWM) is DOMINANT** —
atomic ×0.92 → fungible ×1.19; without it even perfect pull + cross-path
recovery caps sub-unity. (2) **cross-path recovery** — same-path ×0.48 →
cross-path ×0.92. (3) **placement (pull vs push) is NEGLIGIBLE** — ×1.190 vs
×1.190 in the fungible case, because fungible frontier fill masks the
slow-path long pole. The r-sweep confirms the raise-r finding mechanistically:
at H → ∞ the dual beats fast-alone at r = 0 already; raising r only helps a
too-small coding window (crosses fast-alone at r ≈ 0.18 only when H ≈ 256).
Thus Phase C's raise-r = 0.18 "no unlock" is expected — r cannot make a
path-affine atomic unit fungible.

**Updated §16 position (oracle-grounded).** Symmetric aggregates (measured C7
×1.71; oracle ×1.99). Heterogeneous OBJECT completion aggregates to ~×1.19 at
C8 **iff** the transport realizes windowed fungible cross-path frontier decode
— the §16.3 RWM (the EMPTY quadrant). Production BULK (RaptorQ 64 KB atomic
blocks) is path-affine → oracle-capped at ~×0.92 even with perfect pull +
cross-path recovery; the measured ×0.76 is that ceiling dragged down by
same-path/suppressed recovery + eviction. The route to ×1.19 is the RWM
subsystem (fungible sliding-window frontier decode + never-suppressed
cross-path repair supply; the ADR-0046 idle-triggered recovery fix, now landed,
is one prerequisite of the latter), NOT a placement change or a modest r.
Heterogeneous aggregation-above-fast-path is **OPEN in production but proven
ACHIEVABLE in principle (oracle ×1.19)** with a named, scoped mechanism —
sharper than "unproven open problem." (Caveat: the oracle's per-path GE chains
are independent; shared-bottleneck path CORRELATION — where pull placement may
matter more — is not modeled and is flagged for real-trace validation.)

**Superseded by the temporal correction (see §16.3, `feat/oracle-temporal`).**
The ×1.19 figures in this block are from an oracle that abstracts away *time*
(it credits every arrived symbol with whole-window rank, ignoring send-time vs
arrival-time drift and the per-seq realization layer).  The corrected temporal
oracle (`temporal_oracle.rs`) shows two things this one could not: (a) that
oracle's ×1.19 is an *idealization* — the shipped moving-window realization is
correctly REFUTED, reproduced at ×0.259 (het) / ×0.362 (sym), matching the L1
×0.26 / ×0.36 with one fitted constant; and (b) the ×1.19 is nonetheless
recoverable, but only under a **stable per-generation anchor** (measured
×1.194 at C8, no drag), not the moving sliding window.  Read the §16.3
"Corrected oracle" and "Verdict" paragraphs for the adjudicated position;
this block is retained for the L0/L1 reconciliation history.

### 16.8 Final status of §16 (2026-07-08) — the arc concluded

> **⚠ SUPERSEDED-ERA VERDICT (banner added 2026-07-19).** This was the
> settled position of the systematic-repair era, measured on the qemu64 vCPU
> with quinn's stock Cubic silently underneath (§16.17), the PMTU black-hole
> wedge live (§12.12), and the per-transfer 1024-symbol flow-control pool
> (§16.19) — three unmeasured binders below the mechanisms under test. Its
> structural content survives (presence⊥throughput; the oracle-vs-L1 gap
> reading); its numbers and its "production-bounded at parity" ceiling do
> not (post-substrate-chain: C7 0.87–0.97×Σ, C8 0.74–0.80×Σ, gen-sys single
> at 0.97–1.0× plain+BBR). The current regime map is **§17**. Retained
> verbatim as the record of its era.

The heterogeneous-multipath-aggregation arc that motivated this section is
CONCLUDED. The honest, settled position:

**The order-statistic / RWM model is SOUND IN THEORY and codec-correct, but
production heterogeneous throughput aggregation is PRODUCTION-BOUNDED AT
PARITY by the in-order frontier recovery latency — and the binding mechanism
is now identified.** Specifically:

- **DERIVED / oracle-validated.** The design target is real: an independent
  Monte-Carlo oracle (`temporal_oracle.rs`, `multipath_oracle.rs`) confirms
  that generation-based cross-path fungible coding with a **stable
  per-generation anchor** (no per-seq ARQ beneath the code) reaches **×1.194**
  at C8 — essentially the Σg goodput ceiling (×1.195). The codec is correct
  (the decoder-revival fix took `repairs_useful` 0.15% → 66–72%, MEASURED).
  The RWM/order-statistic formulation of §16.1–16.7 is retained: it is the
  right theory, and it says heterogeneous aggregation is not fork-join-bounded
  in principle.

- **MEASURED / production-bounded.** No shipped realization crosses the C8 bar
  (>15.7 Mbit/s, factor > 1). Best dual C8 (c2+c3) is the plain-systematic
  baseline at **14.70 Mbit/s = ×0.97 fast-alone**; every lever the arc built —
  out-of-order H → ∞ (11.9, ×0.76), raised r, cross-path spare-path proactive
  repair (all arms strictly worse than baseline), and SACK+BDP sender
  decoupling (single-path FLAT, C8 REGRESSES to bufferbloat) — lands at or
  below parity. The gap between the oracle's ×1.19 and L1's ×0.97 is precisely
  what the **independent-Monte-Carlo oracle does NOT model: the in-order
  cumulative-ack frontier recovery-latency serialization** (a hole walks the
  frontier at ≈ 1 ARQ round / RTT), which caps the slow path's usable source
  contribution at ~parity so repair cannot buy back more than it costs (the
  presence⊥throughput identity, confirmed to hold cross-path too). Closing it
  requires a recovery-pipeline redesign — pipelined per-RTT frontier recovery
  or a genuinely rateless ack-frontier (a hole is never a fixed in-order
  position) — plus a per-path (not summed-across-paths) outstanding cap. This
  is a scoped BUILD recommendation, not a demonstrated win.

- **What FEC's demonstrated value IS (MEASURED).** Not bulk throughput —
  latency and predictability. On lossy moderate-RTT single links raptorpath's
  message-p99 is 12–60× better than QUIC/kernel-TCP; completion-time variance
  under high loss is ~93× lower; symmetric multipath aggregates ×1.26–1.55
  over kernel MPTCP; and single-path bulk is at ARQ parity under loss and
  BEATS quinn on clean links (after the O(n²) CPU fix). Single-path reliable
  BULK throughput is FEC = ARQ parity max — the presence⊥throughput identity:
  a saturated reliable path has no spare bandwidth to carry a repair that would
  let a loss decode without a round-trip. That is a property of reliable
  delivery, not an engineering gap.

The one-line §16 verdict: **the RWM/order-statistic model is sound in theory
(oracle ×1.19) and the codec is correct, but heterogeneous throughput
aggregation is production-bounded at parity by in-order frontier recovery
latency; the mechanism is identified and the fix is scoped but unbuilt. FEC's
proven value on this stack is tail latency and predictability, not bulk
throughput.** Full primary record: goal-gate.md, "FINAL CONSOLIDATED VERDICT
(2026-07-08)".

### 16.9 The FMTCP-class pure decode-on-total build (2026-07-08) — measured

The literature-blessed retry: build the FMTCP/SCDP pure config — *total-in-flight
flow control* + *fungible fountain redundancy (no per-hole ARQ)* + *decode-on-
total, out of order* — the empty quadrant §16.7/16.8 named as the unbuilt fix, and
which the temporal oracle (PART 5/5c) confirms reaches **×1.19 at C8** with 0 idle
slots and in-flight bounded ≈ aggregate BDP. Built behind `RWM_FMTCP` (the four
changes: total-in-flight FC gating on a bounded win-backstop past the in-order
frontier; per-path — not summed-anchor #64 — BDP in-flight cap; forced cross-path
fungible repair with per-seq ARQ off; forced OOO retention decouple + receiver
reassembly clamp; r = 0.10). Oracle param-confirm passed (PART 5c: shipped params
reach ×1.190, 0 ARQ, 0 idle, emergent in-flight 195 ≈ BDP 145). L1 (25 MB × 6,
independent netem GE qdiscs, no path correlation):

| Arm | Mbit/s | factor | stdev | dnf |
|---|---:|---:|---:|---:|
| **C8 het (c2+c3) FMTCP r=0.10** | **7.58** | **0.48×** | 12.3 s | 0 |
| C8 het FMTCP r=0.20 | 10.43 | 0.67× | 4.2 s | 0 |
| C8 het plain systematic (baseline) | 14.37 | 0.92× | 1.7 s | 0 |
| single-fast FMTCP (parity denom) | 15.65 | — | 0.55 s | 0 |
| **C7 sym (c2+c2) FMTCP** | **25.39** | **1.62×** | 0.57 s | 0 |

**The result is a REFUTATION of the oracle target at C8, and a strong win at C7.**
Reliability held everywhere (dnf 0, every byte delivered, reassembled by offset).
Occupancy: the win-backstop kept the receiver OOO backlog bounded (`[REASM]`
max_span ≈ 1520 ≈ 4·G, max_pending ≈ 990 — NOT the whole object), so change 1's
anti-bufferbloat bound held. But two oracle-signature predictions FAILED in
production at C8:

1. **NOT 0 idle slots.** The oracle predicts the total-in-flight sender never
   idles; the C8 sender is TUN-paused 13–68 % of iterations (`RWM_DIAG`) — the
   recovery-latency stall the oracle does not model, now measured directly.
2. **Anti-aggregation, not ×1.19.** C8 het regressed to 0.48× (r=0.10) / 0.67×
   (r=0.20) — BELOW both the ×0.97 plain baseline and the ×1 bar. The high
   variance (min 13 s ≈ near-baseline, max 45 s crawl) is generation STRANDING:
   a heterogeneous-path generation that loses more than its budget recovers over
   a bufferbloated RTT (MEASURED RTT spikes to 2 s), and the total-in-flight
   decouple lets the send frontier run generations past the stranded one, so the
   whole object waits on the slow tail. This is **exactly the FMTCP abstract's own
   stated pathology** ("a subflow experiencing high delay and loss becomes the
   bottleneck, significantly degrading aggregate goodput") — reproduced, not
   escaped.

**Why the model and production diverge (the residual, precisely).** The oracle
models recovery as an idealized cross-path order statistic that clears a hole
within the fungible horizon at 0 idle. Production recovery is a *scheduled,
congestion-controlled, RTT-clocked* process that (a) contends with fresh proactive
coding for the in-flight budget (exempting it floods → 2 s bufferbloat; gating it
starves → the stranded generation crawls/wedges — both MEASURED), and (b) pays a
real bufferbloat-inflated RTT per recovery round on the slow path. On SYMMETRIC
paths (C7) there is no slow path, no stranding long-pole, and the same build
aggregates cleanly at **×1.62** (better than the arc's prior ×1.26–1.55). ε
under-provisioning is a contributing but secondary factor: raising r 0.10 → 0.20
lifted C8 7.58 → 10.43 and cut variance 12.3 → 4.2 s, but did not reach parity.

**Settled position, unchanged and now doubly-confirmed.** §16.8 stands: the C8
heterogeneous bound is production recovery-latency, not any single sender-side
law, and flipping *both* levers cleanly does not escape it — it makes C8 WORSE
(the decouple amplifies the slow-path long pole) while confirming FEC's real value
is symmetric aggregation + tail latency. The `RWM_FMTCP` path is env-gated and
default-off; the shipped path is byte-untouched. **§16.10 revises this:** the C8
cap was NOT purely recovery-latency — it was the cost-based-CURRENT placement
stranding the slow path at the frontier. Delay-aware (DAPS) scheduling lifts C8
0.48× → 0.80×. *[AUDIT 2026-07-13: that revision note is WITHDRAWN — the §16.10
DAPS measurement was generation-inert (see §16.15); the 0.48×→0.80× lift and the
"not purely recovery-latency" revision are not validly established. §16.9's own
FMTCP numbers stand.]*

---

### 16.10 Delay-aware (DAPS) scheduling + right-sized FEC (2026-07-12) — measured

> **[STATUS, AUDIT 2026-07-13: DAPS arms measured generation-inert — UNCERTAIN,
> not validly established; superseded by §16.15; retained for the record. The
> FMTCP arms stand (`RWM_FMTCP` self-enables generation); the 0.48×→0.80×
> headline, the r-sweep, and the queue-management addendum are unverified.]**

§16.9's FMTCP build placed CURRENT coded symbols by marginal cost (§16.3): the
slow path carried near-frontier data that landed one slow-RTT late, so the object
waited on the slow tail (C8 = 0.48× fast-alone, TUN-paused 13–68%). This section
tests two ideas against the honest single-path-measured ceiling: **(A)** *delay-
aware scheduling* — the slow path carries FUTURE stream data offset by the latency
skew so it arrives in sync with the fast path reaching that position, and a slow-
path loss is a loss of FUTURE data with pre-fetch slack to recover before the
frontier catches up; **(B)** *right-sized FEC* — the derived §8.4 r* for the bulk
profile instead of the fixed r=0.10 (≈4× the 2.6% loss).

**Published algorithm (not a re-derivation).** We follow the delay-aware MPTCP
scheduling family: **DAPS** (G. Sarwar, R. Boreli, E. Lochin, A. Mifdaoui,
G. Smith, "Mitigating Receiver's Buffer Blocking by Delay Aware Packet Scheduling
in Multipath Data Transfer," WAINA/PAMS 2013; N. Kuhn, E. Lochin, A. Mifdaoui,
G. Sarwar, O. Mehani, R. Boreli, "DAPS: Intelligent Delay-Aware Packet Scheduling
for Multipath Transport," IEEE ICC 2014) — schedule over the LCM of per-path
forward delays so segments arrive in order; slow-path offset **Δ_j =
(RTprop_j − RTprop_min)·Σ_i BtlBw_i** symbols — under the **ECF** completion-time
guard (Y. Lim, E. Nahum, D. Towsley, R. Gibbens, "ECF: An MPTCP Path Scheduler to
Manage Heterogeneous Paths," ACM CoNEXT 2017): a path is eligible for a source at
lead L iff L ≥ Δ_j (only use the slow path for data the fast path could not deliver
sooner — the published fix for DAPS's static-schedule failure mode). **BLEST**
(S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, IFIP Networking 2016) and MPTCP-default
**minRTT** are the send-window-blocking / lowest-RTT baselines the cost-based build
corresponds to. Env-gated `RWM_DAPS`, composed on the FMTCP total-in-flight base.

**Oracle-confirm (temporal_oracle.rs PART 6).** Extending the PART-5 model with
the per-path latency skew + bounded in-order reassembly buffer + bursty strand:
cost-based-current **anti-aggregates** at bounded buffers (0.68× at buffer 192,
42% slow-path frontier-freeze — reproducing the 0.48× production regime), while
DAPS never drops below 1.0× and reaches the ×1.195 ceiling at HALF the buffer
occupancy (252 vs 566) and 7× less stall (0.24% vs 1.71%). DAPS ESCAPES the
slow-path long pole. The r-sweep is throughput-optimal at r≈0.05 (2.6% loss); the
fixed 0.10 wastes ≈2× the wire — the over-FEC hypothesis confirmed in-model.

**L1 measurement (VM dual netns, seed 42, 25MB×5, rp-native perf; every arm
dnf=0).** Baselines: single-c2 (fast) = 16.41 Mbit/s, single-c3 (slow) = 3.14 →
C8 recovery ceiling 19.55, C7 ceiling 32.82.

| C8 (c2+c3) arm | Mbit/s | ×fast | ÷ceiling |
|---|---:|---:|---:|
| FMTCP r=0.10 (§16.9) | 7.58 | 0.48× | 0.39 |
| FMTCP-only r=0.03 (right-sized r ALONE) | 7.14 | 0.44× | 0.37 |
| DAPS r=0.10 (placement ALONE) | 8.65 | 0.53× | 0.44 |
| **DAPS r=0.03 (both levers)** | **13.12** | **0.80×** | **0.67** |

DAPS + right-sized r lift C8 **0.48× → 0.80×** (13.12 Mbit/s = 0.67 of the
recovery ceiling), stabilizing it (stdev 8.8→1.2 s) with **paused=0%** (vs the
§16.9 build's 13–68% frontier stall — the long pole is gone). The two levers are
SYNERGISTIC and each necessary: right-sized r alone does not help (7.14, unstable);
DAPS placement alone helps modestly (8.65); together, +73%. The r-sweep is monotone
(0.03 > 0.05 > 0.10 = 13.12 > 10.47 > 8.65) — fixed r=0.10 wasted ≈34% of C8; r=0.02
under-provisions (near-DNF). r*≈0.03 = loss + small margin = the §8.4 bulk r*. C7
symmetric: DAPS 20.87 ≈ shipped-default 20.29 (skew 0 ⇒ Δ=0 ⇒ gate inert; no
regression).

**Honest residual.** 0.80× is still below parity. DAPS removed the frontier-stall
long pole, but a SECOND cap appears: the slow path bufferbloats to ~834 ms RTT
under the deep read-ahead, so the future data's pre-fetch slack is partly consumed
by queue latency. The gap to parity/ceiling is slow-path queue management (BLEST-
style BDP cap on the slow path), NOT the frontier serialization DAPS fixed.

**Revision to §16.8/§16.9.** The C8 heterogeneous cap was NOT purely production
recovery-latency: it was substantially the cost-based-CURRENT placement stranding
the slow path at the frontier. Arrival-aligned (delay-aware) scheduling escapes
that long pole and materially lifts C8; the heterogeneous regime is scheduling-
bound (fixable) AND, at the residual, queue-bound — not recovery-latency-bound as
previously concluded.

**§16.10 — DAPS queue management (per-path BDP cap + BtlBw pacing).** The
residual above bloats the slow path DESPITE the FMTCP per-path BDP cap because
that cap only gates the aggregate TUN-read PAUSE (pause when EVERY path is full),
not per-path PLACEMENT: the softmax keeps committing the slow path its capacity
share of the deep DAPS read-ahead past its BDP, and there is no per-path pacing,
so the read-ahead is dumped faster than BtlBw_slow drains. The two published
bounds — BLEST (bound each subflow's outstanding to its BDP; Ferlin 2016) and
BBR-style per-path pacing at BtlBw_i (Cardwell 2016) — are implemented and
env-gated under DAPS. The temporal_oracle PART 6e queue model confirms that,
GIVEN a correct per-path BDP, they collapse the standing queue (344 ms → 0) and
lift C8 from parity (×1.00) to the ceiling (×1.195), with gain 1.0 optimal.
**L1 (25 MB×5, seed 42), however, shows the fix is rate-signal-limited:** it lifts
C8 modestly (~10.0 → ~11.5 Mbit/s) and cuts within-run stdev (~5.5 → ~2.9 s) with
reliability intact (dnf 0), but does NOT bound the slow-path RTT (still bloats to
~1.8 s). The RWM_DIAG per-path probe shows the per-path Copa BtlBw anchor is only
intermittently established in generation mode (WindowAcks do not drive
`record_delivery`; in_flight releases by time-expiry, not per-path ack), so the
cap/pace have no stable per-path BDP; and the bufferbloat queue lives in the QUIC
datagram send buffer BELOW the in_flight gauge (which reads 0), so the queue
never drains within the min-RTT window and RTprop itself pollutes to ~1.8 s. The
queue bound is the correct mechanism (oracle-confirmed); the TRUE residual is now
**per-path BtlBw estimation + QUIC-send-buffer visibility in generation mode**,
not placement queue depth — a per-path delivered-rate estimator (cumulative ack ×
per-path ownership) is the follow-on.

### 16.11 Pace-all traffic: pacing SOURCE + REPAIR at BtlBw_i (2026-07-12) — measured

> **[STATUS, AUDIT 2026-07-13: measured generation-inert — UNCERTAIN, not
> validly established; superseded by §16.15; retained for the record.]**

The per-path estimator (§16.10 follow-on) made BtlBw_i real and paced SOURCE
placement at it, but §12.5's per-path pacer metered only source: the coded/repair
emission (proactive budget, filling pacer, deficit top-up, inline) was placed by a
separate law and clocked only by a GLOBAL delivered-goodput bucket. PART 6e's PACE
scheduler, however, admits ≤ BtlBw_i **total** — it never split source vs repair —
so the model already assumed total-pacing; the production gap was purely the
source-only pacer. TOTAL per-path emission = source (paced) + repair (unpaced per
path) exceeded BtlBw_i and a standing queue persisted on BOTH paths, fed by
unpaced repair (the per-path SOURCE gauge `sinfl≈0` throughout — source drains
promptly, so it is not the backlog).

The fix routes every repair symbol through the SAME per-path BtlBw token bucket as
source: emit on the candidate if its bucket is funded, else spill to the fast
(min-RTprop) path, else HOLD. The HOLD (repair only ever draws from a bucket ≥ 1)
enforces total-per-path-emission ≤ BtlBw_i on BOTH paths, giving source priority
and repair the leftover capacity — realizing PART 6e's PACE assumption in
production (the model is unchanged; `temporal_oracle` 19/19 stands).

**L1 (VM, dual netns, 25 MB × 8, rp-native, same-binary A/B, seeds 42 & 7).**
Recovery ceiling C8 = single-c2 (16.71) + single-c3 (3.13) = **19.84 Mbit/s**.
Pace-all lifts C8 on BOTH seeds — seed42 7.67→11.88 (+55%), seed7 6.96→10.34
(+49%), pooled ~7.31→~11.11 (**0.37→0.56 of the recovery ceiling**, ×0.44→×0.67
single-fast) — and STABILIZES: within-arm σ_s collapses 4.5/9.6 s → 1.9/2.5 s. The
per-path DIAG confirms the mechanism: the slow-path standing queue is ~halved
(live RTT ~650–1030 ms → ~200–540 ms) with RTprop pinned at the 42–46 ms
propagation base (min-filter clean), and the fast-path queue eases (~130 → ~90 ms).
It does NOT reach the ceiling: the slow queue is halved not collapsed, and a
~100 ms fast-path queue persists. The named residual is now the SOURCE spill (the
source gate spills to the fast path but does NOT hold, so it can drive the fast
bucket negative) + the fast-path source burst — a queue PART 6e abstracts away (it
models only the slow path). C7 (symmetric) is unchanged (21.02, no regression),
every arm dnf=0. The queue-management arc is thus: scheduling (DAPS) → per-path
rate estimation (§16.10) → repair pacing (this section) → the residual source-spill
/ fast-path queue — each bound realized in turn, C8 lifted and stabilized to 0.56
of the recovery ceiling, not yet at the goodput ceiling.

### 16.12 Source backpressure: REFUTED — source is the pipeline clock, not a holdable emitter (2026-07-12) — measured

> **[STATUS, AUDIT 2026-07-13: measured generation-inert — UNCERTAIN; the
> REFUTED verdict and the "source is the pipeline clock" mechanism are not
> validly established (unsafe either way); the default-OFF ship decision
> stands on prudence alone; superseded by §16.15; retained for the record.]**

§16.11's residual named the SOURCE spill: pace-all held the rateless repair when both
per-path buckets were dry, but the source placement gate still spilled to the fast
path and drove its BtlBw bucket negative (an unmetered burst). The natural symmetry
argument — source is payload, so DEFER (backpressure the send loop) rather than
discard, admitting a source symbol only when some per-path bucket is funded — was
implemented and tested. The temporal oracle was extended to model it: PART 6f adds
the fast-path FIFO that PART 6e abstracted, and predicts that deferring source
collapses the fast queue (374 ms → 0) and, by keeping the fast bucket non-negative,
removes the repair-to-slow coupling (344 ms → 0), lifting C8 to the resequencing
optimum ×1.195 with no queue residual left in the model.

**L1 refuted the prediction.** Same-binary A/B (`RWM_SRC_BP`, 25 MB × 8, seeds 42 and
7): source backpressure REGRESSED C8 ~53% on BOTH seeds (seed42 14.35 → 6.60, seed7
15.63 → 7.39 Mbit/s, pooled ~14.99 → ~7.00, 0.76 → 0.35 of the recovery ceiling) and
destabilized it (σ_s 1.1/1.3 → 9.5/4.1 s), every arm dnf=0. Two mechanisms, from the
per-path DIAG: (i) unlike a rateless repair symbol, the source read is the
generation-FILL clock — deferring it starves coded emission too (long paused-100 %
stalls), so backpressuring source wedges the whole pipeline; and (ii) the gate is
largely inert anyway, because the per-path BtlBw ANCHOR is over-read under fast-path
bufferbloat (measured BtlBw ≈ 1.2 M sym/s vs the true ≈ 8.3 k sym/s, ~145×), so the
pace bucket almost never goes dry and the backpressure rarely engages where the queue
is. The fast-path live RTT did not collapse under backpressure — confirming the
residual is the anchor over-read, not the source spill.

Crucially, the source SPILL is BENIGN: the fast link drains the spilled source, so a
fast-path queue is a latency cost, not a throughput cost, for a bulk transfer. The
pace-all default (spill) already sits at 0.76 of the recovery ceiling, stable on both
seeds — the assumed 0.56 residual was a lineage artefact the fuller x8 does not
reproduce. The corrected regime map: heterogeneous aggregation is scheduling-bound
(DAPS), rate-estimation-bound (§16.10, closed), and repair-pacing-bound (§16.11,
closed); the remaining lift to the ceiling is NOT source spill but the **per-path
BtlBw anchor over-read under bufferbloat** (ack-aggregation / delivered-rate
over-read) — the signal any per-path pacer needs to bind. Source backpressure is
retained only as a default-off, oracle-modelled, unit-tested knob; the shipped
default is byte-identical to §16.11. §16.13 fixes that anchor over-read and reports
what it does — and does NOT — buy.

### 16.13 BtlBw rate-sample: the per-path anchor over-read, fixed — and the C8 residual it exposes (2026-07-12) — measured

> **[STATUS, AUDIT 2026-07-13: measured generation-inert — UNCERTAIN, not
> validly established; the C7 "politeness regression" leg is refuted-as-noise
> by §16.14's own symmetric identical-code arms; superseded by §16.15;
> retained for the record.]**

§16.10–16.12 each named the SAME residual: the per-path BtlBw anchor is over-read
under bufferbloat, so no per-path pacer/cap can bind. This section fixes the anchor
and reports the honest consequence.

**The bug.** The per-path estimator (§16.10) derived BtlBw_i from a delivered-count
ratio measured over the ACK-ARRIVAL interval (`Δdelivered/Δt_ack`). Under DAPS acks
arrive BATCHED (ack-aggregation), collapsing Δt_ack toward zero, so the windowed-MAX
filter locked onto the spike. L1 DIAG (client/sender side, C8 dual, 1200-B symbols ⇒
true fast link 10 416 sym/s): the fast-path anchor read **1 644 200 sym/s — a ~158×
over-read** — with the fast path bufferbloated to a **1 573 ms** live RTT (RTprop
12 ms). A 158× rate anchor makes the fast-path BDP cap 19 403 symbols and the pace
bucket refill ≫ drain, so the cap/pacer are INERT (temporal_oracle PART 6g models
this: the over-read makes the bucket occupancy swamp the deep read-ahead share, so
the path DUMPS its whole share — C8 falls to fast-path parity).

**The fix (Cardwell/Cheng, draft-cheng-iccrg-delivery-rate-estimation).** Per-path
BBR delivery-rate sampling: each source symbol snapshots `(sent_time, delivered,
delivered_time, first_sent_time, app_limited)` at send; its ack yields ONE rate
sample `Δdelivered / max(send_elapsed, ack_elapsed)` — Δt over the SEND interval, so
a batched ack (tiny ack_elapsed) is overridden by the true send spacing. Samples
spanning less than one RTprop are rejected (BBR's `interval < MinRTT` guard — the
ack-aggregation / send-burst artefact), app-limited samples may only RAISE the
max-filter, and BtlBw_i = the windowed-max over ~10·RTprop. Gated `RWM_RATE_SAMPLE`
(on by default under the per-path estimator; =0 reproduces the legacy ack-interval
anchor for a same-binary A/B). Shipped non-DAPS default byte-identical.

**Anchor over-read — CLOSED (the primary metric).** Same DIAG, `RWM_RATE_SAMPLE=1`:
the fast-path anchor reads **≈ 10 900 sym/s vs the true 10 416 — ~1.05×** (from
158×), the fast-path BDP cap collapses 19 403 → ~90 symbols, and the fast-path live
RTT collapses **1 573 ms → ~30 ms** (RTprop base 8 ms). The slow-path RTprop, which
the queue had polluted to 128 ms, returns to the **41 ms** propagation base and its
anchor over-read drops from ~9 700 to ~3 200–7 700 sym/s. On a SINGLE 100-Mbit path
the over-read bufferbloat is pure LATENCY (the link drains at line rate regardless of
buffer), so single-c2 throughput is UNCHANGED by the fix — 16.65 (fix) vs 16.29
(legacy) Mbit/s, both σ_s ≈ 1.2 s on x8 — i.e. the corrected anchor is a latency/
correctness win on single-path, not a throughput one.

**C8 does NOT rise — the honest critical finding.** Same-binary A/B (25 MB × 8,
seeds 42 & 7): correcting the anchor REGRESSES C8 under DAPS (fix pooled
≈ 9.7 vs legacy-anchor ≈ 10.7 Mbit/s, seed-dependent: seed42 13.2→10.7 regresses, seed7 8.2→8.7 neutral). The reason is exactly what
§16.12 measured: the fast-path SPILL the over-read enabled was BENIGN — the 100-Mbit
fast link drained it, a latency not a throughput cost. Binding the fast pacer (via
the correct anchor) REMOVES that benign spill and forces load onto the slow path,
whose live RTT then bloats to **~3–4 s** even though its anchor and BDP cap are now
correct and its per-path SOURCE gauge sits at the cap. The slow-path queue is
therefore NOT the source rate anchor: it is the DEEP DAPS read-ahead
(`(pipeline+6)·G`) + future-offset placement + coded/repair, which over-commits the
slow path and holds the receiver's resequencing frontier — a queue the corrected
per-path SOURCE pacer does not bound. Oracle PART 6g assumed a correct anchor on
BOTH paths collapses BOTH queues; L1 corrects the anchor on the FAST path (×1) but
the slow path carries a second, read-ahead-driven queue the model omits.

**Verdict.** The rate anchor was genuinely broken and is now fixed (fast ×158→×1,
fast-path bufferbloat 1573→30 ms) — a real correctness win (a truthful per-path rate
signal + collapsed fast-path latency) and the necessary precondition for any binding
per-path pacer. But it does
NOT lift heterogeneous C8: the 0.76-of-ceiling gap is NOT the source rate anchor. The
true C8 residual, with DIAG evidence, is the slow-path DEEP READ-AHEAD over-commit
(DAPS future placement + coded/repair depth, not source pacing) — the next lever
(§16.14). The anchor fix ships on by default under the estimator (it is the correct
rate); the DAPS C8 regression it exposes is the honest handoff to the read-ahead work.

A framing correction §16.11–16.13 earned in retrospect (§16.14): each of those pacers
was inert or harmful because it bounded the wrong quantity. A per-path token bucket
throttles the emission RATE; but the heterogeneous-aggregation queue is a DEPTH problem
(how far ahead of the fast frontier the slow path is committed), and throttling rate to
bound depth either leaves the link idle (§16.13's politeness regression) or cannot bind
at all. The correct limiter is the read-ahead DEPTH, capped at the latency-skew — which
§16.14 isolates, and finds bounded by a deeper blocker still.

### 16.14 DAPS read-ahead depth: the correct lever, bounded by the missing slow anchor (2026-07-12) — measured

> **[STATUS, AUDIT 2026-07-13: INVALID (proven) — measured generation-inert
> (saved sender logs: `cod=0`/`eff_pace=0`; arms A/B/C ran identical code) and
> the mechanism DIAG was read from the RECEIVER log; "slow anchor never
> establishes" is refuted and the CONSOLIDATE recommendation is void;
> superseded by §16.15; retained for the record.]**

§16.13 isolated the C8 residual as the slow-path DEEP READ-AHEAD: with the anchor
correct, the BDP cap engaged, and the source gauge at the cap, the slow path still
bloated to ~3–4 s. This section builds the intended fix, confirms it in the oracle, and
reports the honest bound L1 returns.

**The mechanism (correct DAPS/ECF, on DEPTH not RATE).** DAPS places future stream
positions on the slow path offset by the skew Δ so they arrive in order (§16.10). The
over-commit is DEPTH: the deep read-ahead pushes far more than `skew·BtlBw_slow` symbols
onto the slow path, so they queue behind their own not-yet-due position and arrive after
the fast path has already delivered that region — head-of-line-blocking the receiver's
reassembly. The fix bounds each non-fastest path j to at most `skew_j·BtlBw_j` symbols
of read-ahead beyond the frontier (queue delay = outstanding/BtlBw ≤ skew ⇒ in-order-
aligned), dropping j from the DAPS-eligible set and steering repair off it once the
budget is full. This is strictly tighter than the BLEST BDP cap (skew ≤ RTprop) and — the
decisive distinction from §16.11–16.13 — it is a DEPTH limiter, not a rate throttle: the
pace bucket still refills at BtlBw_j, so within the budget the path emits at full link
rate and the link stays FULL. That is what escapes §16.13's rate-throttle idle. Gated
`RWM_DAPS_DEPTH` (on under DAPS+rate-sample; =0 = unbounded read-ahead, same-binary A/B).

**Oracle (temporal_oracle PART 6h).** PART 6h adds the UTILIZATION axis the pure queue
model (6e–6g) lacked, so it can tell a DEPTH bound (link full) from a RATE throttle (link
idle). Three regimes for the slow path: DEPTH-UNBOUNDED — full link but the whole
read-ahead queues (~3.5 s) → useful→0 → **×1.000** (parity); RATE-THROTTLE — queue
bounded but link idled at util η → **×1.158**, and applied symmetrically to C7 it
reproduces the measured **20.96→16.97 exactly** (0.810), proving the model reproduces
§16.13's failure; DEPTH-BOUND — full link AND read-ahead within one skew → **×1.195**
(ceiling), C7 restored. Depth-bound beats both traps → the build proceeded.

**L1 — best arm, most stable, but NO aggregation (25 MB × 8, seeds 42 & 7, same-binary
three-arm, interleaved, dnf=0).** Ceilings (arm-C binary): single-c2 (fast) 16.45,
single-c3 (slow) 3.24 ⇒ recovery ceiling 19.69 Mbit/s. Arms: A legacy (`RS=0`), B
rate-sample only (`RS=1 DEPTH=0`), C rate-sample+depth (`RS=1 DEPTH=1`).

| arm | C8 pooled Mbit/s | σ_s (worst run) | ×fast | eff ÷19.69 |
|---|---:|---:|---:|---:|
| A legacy | ~6.50 | 10.3 | 0.40× | 0.33 |
| B rate-sample | ~7.22 | 26.8/29.7 (1.9–2.2) | 0.44× | 0.37 |
| **C depth-bound** | **~8.40** | **5.6/9.1 (5.3–6.7)** | **0.51×** | **0.43** |

Arm C is the best and by far the most stable of the three — σ_s collapses ~3–5× and it
removes B's catastrophic bimodal bloat tail (worst-run floor ~1.9 → 5.3 Mbit/s). **But
C8 = 8.40 is 0.51× fast-path-alone (16.45): adding the slow path leaves dual-path at
HALF of using the fast path alone. It does not aggregate.** This holds across every arm
and both sessions (§16.13's best C8 10.7 was already < its single-c2 16.65).

**Why the correct lever is inert.** Sender per-path DIAG: the slow path's BtlBw anchor
NEVER establishes (`est=n`, `btlbw=0`), so the depth budget `skew·BtlBw_slow` is
UNDEFINED (`dbud=0`) throughout — there is no rate signal for the depth (or any per-path)
bound to act on, and the slow RTT bloats unbounded to ~1.5 s. The chain
estimator→correct-anchor→depth-bound breaks at the SLOW anchor, which does not warm in
this loss (ε≈0.026) / skew (Δ≈30 ms) regime: the slow path is acked too sparsely and too
batched for the BBR min-RTT-guarded sampler to populate a max-filter. C7 is
noise-dominated (depth is a provable symmetric no-op, yet the two identical-behaviour
arms differ 20% — implying §16.13's C7 "regression" was itself largely noise).

**Verdict — the honest bound; consolidate.** The depth bound is the correct mechanism
(oracle-confirmed to the ×1.195 ceiling, unit-tested, byte-identical default) and
empirically the best, most stable, harmless arm; it ships on by default under
DAPS+rate-sample. But it does not land heterogeneous aggregation, because the binding
constraint is not a scheduler-side depth or rate over-commit — it is the slow path's
failure to ESTABLISH a usable rate anchor in this regime, which no source-side scheduler
can synthesize. Bulk C8 is bounded below fast-path-alone: the queue is
latency-not-throughput (HoL/resequencing coupling) and the slow path's marginal
~3.2 Mbit/s is not economically aggregatable for bulk under this loss/skew. This was the
last structural scheduling lever; the recommendation is to consolidate the pacing/
scheduling line with the full evidence chain (§16.10 DAPS → §16.11–16.13 pacers →
§16.14 depth), reproducible via the same-binary `RWM_DAPS_DEPTH=0` / `RWM_RATE_SAMPLE=0`.

---

### 16.15 Methodology correction + the generation-ON re-baseline: the arc's coded path was DEAD in measurement (proven for §16.14; §16.10–16.13 unverifiable) (2026-07-13)

**The bug (a measurement-harness bug, not a model bug).** The L1 aggregation harness
`perf_rwm_c.sh` passed only `--window-reliable`. Generation is enabled ONLY by
`--window-generation-coding` (or `--window-systematic-repair`/`RWM_FMTCP`):
`window_generation = window_reliable && (window_generation_coding || window_systematic_repair
|| fmtcp)`. Every §16.10–16.14 mechanism gates on `generation` —
`daps = RWM_DAPS && generation`, `per_path_est = generation && (daps || RWM_PER_PATH_EST)`,
`rate_sample = per_path_est && …`, `daps_depth_on = rate_sample && …`. So the entire
DAPS + per-path-estimator + rate-sample + read-ahead-depth + source-backpressure stack
was **INERT** in the very measurements that evaluated it — the saved §16.14 sender logs
show `cod=0`/`eff_pace=0`. The arc's C8 arms ran plain `--window-reliable` mode — the
windowed-RLC streaming backend + ARQ (bulk with `--window-reliable` auto-selects RLC; NOT
block mode).
**Every §16.11–16.14 mechanism verdict is therefore SUSPECT** and must be re-established
generation-ON. A hard guard now fails any generation run with `cod=0` loudly (asserting
cumulative `total_coded>0` on the SENDER `/tmp/rwm-c.log`, not the receiver log — the
§16.14 wrong-log trap).

**The first VALID measurement (generation ON, current-main stack, 25 MB × 8, seeds 42 & 7,
dnf=0, guard OK every arm).** Ceilings: single-c2 (fast) **13.99** Mbit/s, single-c3 (slow)
**3.04** ⇒ recovery ceiling **17.03** (the fast ceiling is LOWER than §16.14's inert 16.45 —
generation coding costs ~15 % single-path, the coding/decode tax). C8 (c2+c3): seed42
**9.97** (σ_s 3.43, a warm-up ramp 7.8→11.1), seed7 **13.52** (σ_s 0.67) ⇒ **0.72× / 0.96×
fast-alone**, pooled ~11.74 (0.84×). C7 (c2+c2): **12.05 / 12.59** ⇒ **0.87–0.89× fast-alone**.

**What this overturns.** (1) §16.14's quantitative "C8 = 0.51× fast, 8.40 Mbit/s,
structurally bounded, consolidate" is invalid — measured on dead code; gen-ON C8 is 0.84×
pooled and reaches **parity (0.96×) on seed7**, a far narrower gap. Its "channel starvation
no source-side scheduler can synthesise" framing is overturned (the §16.15-prior diagnosis
already showed the anchor establishes). (2) §16.11 pace-all / §16.12 source-backpressure
(REFUTED) / §16.13 rate-sample verdicts were all generation-inert A/Bs and do not stand.
(3) A NEW, load-bearing finding the inert regime hid: **symmetric C7 is ALSO below
fast-alone (0.88×)**. The depth bound is a provable no-op on symmetric skew, so this is a
residual dual-path **generation-mode** throughput tax (coding overhead + cross-path
reassembly/decode coupling) independent of the slow-anchor — it caps even two identical
paths below one, and bounds what any slow-anchor fix can buy for C8. C8 het is
seed-unstable and ≤ fast-alone, so the slow-anchor de-noise (BtlBw_slow, §16.15
diagnosis) was built and A/B-tested (same-binary `RWM_RATE_WIRE`, robust quantile of the
per-path delivered-rate samples for the DAPS pace/offset/depth signal, gated default-off).
**Result — REFUTED:** the de-noise REGRESSES C8 3–6× (seed42: OFF 7.80 → ON ~1.3 Mbit/s
median / ~2.7 at q=0.9), because the generation-mode per-path rate samples are
**decode-FRONTIER-clocked** — mostly-low with the true link rate at the burst-peak TOP — so
the windowed-MAX is the near-correct recovery statistic and any sub-max quantile lands in the
low cluster and UNDER-reads the fast path ~65× (btlbw 159–198 sym/s vs true ~10 400),
collapsing the pace bucket. The §16.15-diagnosis "over-read to 20 950" is a rare upper-tail
spike, not the bulk; rejecting the top removes the signal. The correct fix is therefore NOT a
filter over the decode-clocked samples but a per-path rate measured from the path's OWN WIRE
acks (link-clocked) — a larger change, deferred; the knob ships default-off (byte-identical),
unit-tested, oracle-modelled (temporal_oracle PART 6i: a STABLE anchor WOULD reach the ×1.195
depth-bound ceiling, but the quantile does not produce one). The honest standing conclusion:
with the mechanism genuinely ON, heterogeneous bulk C8 is at parity-to-slightly-below
fast-alone (not the inert 0.51× bound, not aggregating), and the residual is BOTH the
decode-clocked rate signal (needs a wire-clocked estimator, not a filter) AND a newly-exposed
symmetric dual-path generation-mode coding tax (C7 ≤ fast-alone), not only the slow anchor.

**Audit addendum (2026-07-13).** A read-only audit (in-repo:
`docs/audits/2026-07-13-verdict-audit.md`, `docs/audits/2026-07-13-session-audit.md`;
summarized in goal-gate.md "Methodology Audit (2026-07-13)") classified the era
section-by-section. Precision on "measured with the coded path DEAD": it is
*proven* only for §16.14 (its saved sender logs show `cod=0`/`eff_pace=0`);
§16.10 (DAPS arms) through §16.13 are classified **UNCERTAIN** — their ledger
sections recorded no command lines or env, so generation-ON can neither be
established nor definitively excluded for them (their FMTCP arms, which
self-enable generation, stand). Either way none of their verdicts is validly
established, for a second, independent reason: **era noise exceeded every
claimed effect** — the same nominal config measured 14.99 / 10.74 / 6.50 Mbit/s
across three sessions (2.3× spread), and plain window-reliable dual C8 is
heavy-tailed/bimodal (mean 5.43, σ_s 11.7), covering every claimed §16.10–16.14
delta (+15%, +30%, +52%, −53%, −19%, the §16.14 arm ordering). Two env footguns
compound the hazard: `RWM_FMTCP=0` and `RWM_DAPS=0` still count as SET
(`.is_ok()` gates); the harness fix's `RWM_GEN=0` sentinel avoids this. Future
L1 verdicts are bound by the MEASUREMENT DISCIPLINE checklist at the top of
goal-gate.md (mechanism-liveness proof, recorded command line + env,
interleaved same-binary arms, both seeds + distributions, effect above the
recorded noise floor).

---

### 16.16 Generation-ON stack ablation: the symmetric collapse is the STACK, not the code; gen-mode throughput is substrate-bound at ≈10 Mbit/s per path (2026-07-13)

§16.15 left the attribution open: with generation ON, symmetric C7 collapsed 21→12 —
coding-intrinsic, or the DAPS-era stack live for the first time? A five-arm same-binary
ablation (P plain control; G0 generation BARE; G1 +DAPS; G2 +rate-sample; G3 +depth =
the §16.15 config; r=0.03, 25 MB × 8 reps, seeds 42 & 7, arms interleaved per rep,
`cod>0` guard on every gen run) answers it. Prerequisite fix: every boolean `RWM_*`
gate parsed with `.is_ok()`, so `=0` counted as ON and "explicitly off" arms were
inexpressible; one `env_flag` helper now parses all 22 boolean gates (unset → shipped
default; ""/"0"/"false" → OFF), defaults for unset unchanged.

**C7 symmetric (pooled both seeds):** P **22.8** · G0 **20.7** · G1 20.4 · G2 15.9 ·
G3 **11.7** (G3 replicates §16.15's 12.1/12.6 — consistency PASS). Attribution:
coding intrinsic (P→G0) is 0…−18 % seed-dependent and G0 keeps REAL aggregation
(×1.37 fast-alone, plain-class); DAPS placement (G0→G1) is free; **rate-sample
(G1→G2) costs −22 % on both seeds; the depth bound (G2→G3) another −17…−30 %** —
§16.14/16.15's "depth is a symmetric no-op" is false in practice because the
decode-clocked anchors make one path always look transiently slower and it gets
depth-throttled on garbage skew. The §16.15 symmetric-collapse question resolves to:
**the stack, ~45 % of it, on top of a nearly-free code.**

**C8 heterogeneous (pooled):** P **14.96** (0.99× same-day fast-alone 15.15, σ_s 2.0) ·
G0 9.19 (σ_s **0.14/0.48** — the most repeatable multipath number on this rig) ·
G1 8.10 · G2 8.30 (one DNF) · G3 **10.04** (the depth bound's one real win: +0.85 vs
G0 on hetero). Plain beats every gen arm ×1.5–1.8; nothing has ever exceeded fast-alone
on C8 when measured against the LINK ceiling (§16.15's "0.96× parity" was against
gen's own depressed 13.99).

**The structural finding.** Singles (same day): plain-c2 15.15, gen-bare-c2 **9.70**
(σ 0.32), plain-c3 3.31, gen-bare-c3 COLLAPSED (0.78 then DNF — bare r=0.03 is not
viable on the lossy path alone; §16.15's 3.04 needed the DAPS-deepened window). Line
up gen-bare: single 9.70 ≈ C8 9.19–9.27 ≪ C7 20.7 (×2.15 its own single). In
generation mode each path delivers ≈10 Mbit/s regardless of link capacity — the
binder is the generation pipeline itself (in-flight window / window-fill decode
serialization; C7's two source-carrying paths fill generations in parallel, single
and C8-fast hit the same wall). Gen C8 = 0.95× of gen's OWN substrate ceiling: the
scheduler was never the constraint; **no DAPS-era lever could have lifted C8.**

**Where this points.** (1) Don't ship the stack ON under generation: rate-sample+depth
tax symmetric duals −45 %; depth is at most a hetero-specific opt-in. (2) The next
lever is the SUBSTRATE: raise the per-path ~10 Mbit/s generation ceiling — deepen/
overlap the generation pipeline (DAPS's window-floor already demonstrates +44 %
single-path headroom: 9.70→13.99), and/or reduce decode-gating via the systematic-
source submode. (3) The wire-clocked estimator (§16.15's deferred fix) is third: a
stable anchor is worthless while the substrate caps ×0.64 below the link.

---

### 16.17 The generation substrate ceiling named and raised: the wall was quinn's loss-reactive Cubic under the datagram path × generation-mode queue-bloat, not the coding pipeline; plus the derived pipeline depth M* (discharging most of #61) (2026-07-13)

**Diagnosis method (L0 first).** Two instruments separate the app pipeline from the
transport substrate: (a) an env-gated netem shim INSIDE the QUIC transport's
datagram send path (`RWM_L0_NETEM` — per-path rate/delay/jitter/GE-loss with the
L1 scenario parameters) lets the in-process loopback bench run the full engine
under c2/c3 shaping while quinn's own congestion controller still sees a clean
loopback; (b) a DIAG-gated stall attribution over the generation data plane's
gates + per-generation lifecycle timing. The L0 result is the pivot: the app
generation machine does **34 Mbit/s** under c2 parameters (plain 67) — the L1 wall
(9.7) is not in the app. The instrumented L1 run then names it: sender gates open,
**RTT 312–802 ms vs RTprop 12–41 ms** (a multi-second standing queue draining
through quinn's loss-collapsed CUBIC window — quinn gates every send, datagram
frames included, on its congestion window), per-generation coded phase 410–874 ms,
pace EWMA pinned at its floor between decode-clocked acks, and 1.76× coded waste
from deficit re-sends at the bloated RTT. Per connection = per path — hence
§16.16's per-path signature (C7 ×2 while single/C8-fast hit the same wall).

**The fix (both env-gated, defaults byte-identical).** (1) `RWM_QUIC_CC=bbr`:
the substrate CC of a loss-tolerant FEC transport must not be loss-reactive;
quinn's BBR is delivery-rate-based. (2) `RWM_GEN_PIPE=1`: per-path BDP in-flight
cap (1.5, the FMTCP mechanism — RTT stays ≈ RTprop, which is what made DAPS's
window-floor accidentally help); derived pipeline depth
**M\* = ceil(rate·2·RTprop/G) + 1** clamped [2,32] — task #61's
A\* = clamp(D·rate, 1, W) quantized to generations, with D = one delivery + one
deficit round, rate = windowed-MAX delivered rate (§16.15's statistic), RTprop
not live-SRTT (self-inflation); sent-frontier coded budget clock; windowed-max
×1.25 pacing (BBR probe gain) replacing the decaying EWMA ×1.5; once-per-SRTT
deficit action.

**Measured (VM, 25 MB×8, seeds 42/7, interleaved, guard-verified; full tables in
goal-gate "Gen Substrate Ceiling").** Gen-bare single-c2 9.77/9.87 (exact §16.16
replication) → app fix alone **14.3/14.6** (the ≥14 target, still on Cubic;
coded waste 1.76→1.15×) → BBR alone **32.9/33.7** → both **33.8/34.1** = the L0
app ceiling. Single-c3 bare collapse (0.78-DNF) → **13.0/12.7, σ ≤ 0.26, dnf 0** (plain c3 is 3.2–3.7).
C8 het 9.4/9.1 → **32.3/27.7** = ×1.9/×1.5 the same-day plain fast-alone — the
first C8 above the historic link-class fast-alone. C7 20.6→33.2/31.1.
**And the control that reframes the arc: plain+BBR single-c2 = 76, C7 = 89.5,
C8 = 45.5 (bimodal σ 25)** — the 15–17 "link ceiling" every prior section
measured against was itself the Cubic substrate, not the 100 Mbit link.

**Honest framing.** On the SAME (BBR) substrate nothing changes structurally:
gen C8 = 0.95× of gen's own single (parity, not aggregation), plain C8 = 0.61×
of plain's single (bimodal); the gen machine now caps ≈34 Mbit/s TOTAL regardless
of path count (single 33.9 ≈ C7 32.1 ≈ C8 30.0 = the L0 ceiling) — the next
binder is the app/decode machine: per-generation dense Gauss–Jordan is
O(G²·S) ≈ 90 ms CPU per G=384 generation ≈ 4 000 sym/s ≈ 39 Mbit/s on one core,
plus the residual fill/serialization. Raising THAT (smaller effective decode
cost, overlap, or G) is the next lever; it is CPU work, not networking.
`RWM_QUIC_CC=bbr` remains an experiment knob — BBR's fairness on shared lossy
bottlenecks was not evaluated and flipping the shipped default is a separate
decision. **[UPDATE 2026-07-21: measured (0.95–0.96 share vs Cubic at c2)
and flipped — BBR is the shipped default; goal-gate "Default CC Flip".]** **What remains of #61:** the M\* depth term only engages when
BDP > G (RTT100/200 classes) — at c2/c3 M\* = 2 and GP's +47 % came from the
queue/pacing/reactive discipline; the depth law is implemented and unit-tested
but not yet validated in its engagement regime. **Follow-on:** §12.11 turns
the substrate controller into an explicit policy surface and measures the
third policy — `RWM_QUIC_CC=passthrough`, our Copa-lite owning the quinn
window outright (plain mode: 0.4–0.6× BBR-under's bulk throughput, 3–6×
tighter standing queue, c3 collapse mode eliminated).

---

### 16.18 The decode-CPU ceiling dissolved: the O(G²·S) was the coded-only WIRE, not the solver — the systematic-repair wire is the O(k·G·S+k³) machine, and generation mode now rides at 0.92× of the plain link-class rate (2026-07-13)

§16.17 ended with the generation machine binding the whole transport at ~34
Mbit/s regardless of path count, attributed to the receiver's dense
per-generation Gauss–Jordan (O(G²·S) ≈ 90 ms CPU per G=384 generation ≈
4 000 sym/s). Profiling the coding machine ALONE (a new L0 micro-bench with
per-call bucket attribution; G=384, S=1200, r=0.03, ε=2.6 %) shows that
attribution to be half right — and locates the quadratic one level up.

**The profile (L1 VM — `QEMU Virtual CPU 2.5+`, SSSE3 only, GF(256) kernel
4.1 GB/s).** On the coded-only wire the decoder does 5 943 sym/s (166 µs per
dense row ⇒ ~64 ms/generation) and the ENCODER does 4 922 sym/s (203 µs per
coded symbol — the sender sums ~G sources into every symbol): enc+dec
together are the measured ~34 Mbit/s ceiling, exactly. On the SAME VM, the
same decoder fed the systematic wire (raw source as unit rows + only
⌈G·r⌉+deficit dense repairs) does 125 000 sym/s ≈ 1.2 Gbit/s·core. **The
quadratic is the WIRE MODE:** the L1 generation arm (`--window-generation-
coding`) is coded-only — no raw source rides it (§16.17's waste arithmetic:
38 280 coded for ~21 800 source) — so all G DoF per generation arrive AND
depart as dense combinations, making Ω(G²·S) work per generation
information-structural on both ends. At ε=2.6 % only k ≈ ε·G ≈ 10 DoF are
actually missing; O(k·G·S + k³) is achievable only if the other G−k DoF ride
the wire raw — which is precisely §16.3's systematic-repair submode
(`--window-systematic-repair`), implemented and idle since that section.

**Fix 1 — sparse-aware decoder (pure speedup, unconditional,
output-identical).** The decoder still wasted systematic-mode work: known
sources were materialized as full-width fused unit pivot rows (slot creation
alone copied O(G·(G+S)) bytes — measured 1.26 ms), every dense repair reduced
against all G rows with (G+S)-byte SIMD calls where 374 of 384 merely
subtracted a known source, and late-source injection (#59) built full-width
rows. The rewrite keeps known sources OUT of the matrix (a `known` bitmap;
payload-only elimination against the recovered store), keeps only the ≤ k
coded rows in incremental RREF, converts unit rows to `known` on the spot
(the active system stays k×k; completion needs no back-substitution pass),
and recognizes an already-complete span in O(G) with zero GF work — the k=0
common case never builds a matrix. Per generation: O(k·G·S + k²·(G+S)). The
pre-rewrite decoder is kept verbatim as a reference oracle; a differential
test drives both over randomized traces (both wires, 5–25 % loss, reorder,
FILL_FLAG, duplicates, `advance`) asserting identical delivered sets, bytes,
rank and useful-repair accounting per call. Micro gains (VM): ×5.0 on
in-order clean (129 k→643 k sym/s), ×1.27–1.29 on lossy/fill traces, ×1.20
even on coded-only (early-exit unit detection).

**Fix 2 — the systematic wire as the generation arm.** L0 full-engine (netem
shim, c2): coded-only 32.5 → systematic 70.1 = plain's 70.4. SIMD was already
present (ADR-0041); parallel decode and G-shrink were not needed.

**L1 (VM, 25 MB×8, seeds 42/7, interleaved, guard-verified, CPU recorded;
full tables in goal-gate "Decode-CPU Ceiling"; before=02d240c,
after=da926a5).** Single-c2: coded-only replicates 33.5/33.1; the decoder
rewrite alone +3.1/+2.4 (2–3σ — the wire, not the solver, was the wall);
**systematic 70.9/70.8 (σ ≤ 3.2, dnf 0) = ×2.1 = 0.92× of plain+BBR's
same-session 77.1/77.6**. Single-c3: 13.0 → 14.9-median/15.1 = **0.95× of
plain+BBR-c3's own 15.6/15.8** (the recovery ceiling, measured as a control
for the first time) — the lossy-path FEC win (~4× plain-Cubic) holds. C7:
72.3/72.4 ≈ ×1.02 of gen's single (still parity; plain+BBR aggregates ×1.21
to 93/95). **C8 heterogeneous: 69.8/69.1 (σ 5.0/5.5, dnf 0) vs plain+BBR's
own C8 55.7/45.4 (σ 12–13, bimodal 29–71) — ×1.25/×1.52 with half the
variance, 0.90× of the fast-alone control, 0.98× of gen's own single: the
best C8 configuration measured on this testbed.** CPU (per 25 MB): receiver
5.54→3.38 s, sender 4.45→2.25 s while throughput doubled — within 14 %/11 %
of plain+BBR's CPU at the same rate; per delivered bit the coding machine's
CPU fell ×3.4/×4.2. All 3 battery DNFs were on coded-only gen arms; the
systematic arms ran 60/60.

**Where the machine binds NOW.** In every link-class arm (gen-sys C7/C8 AND
plain C7) the RECEIVER process saturates ~1.0 core (3.6 s CPU / 3.5 s wall):
the single-threaded receive/reassembly/delivery engine caps one sink at ≈72
Mbit/s (gen-sys) / ≈93 (plain) on this VM core — decode is now ≤ ~15 % of
that budget (160 k sym/s available vs ~7.3 k consumed at 72 Mbit/s). Gen-mode
C7 aggregation above its single is therefore a receiver-ENGINE problem
(parallelize/offload the receive path), not a coding problem. The remaining
honest gap to plain (0.92× single) is the r+deficit repair overhead plus the
generation bookkeeping — a real but constant tax. The coded-only wire remains
available where its property (no positional source at all) is wanted, but as
a THROUGHPUT configuration it is dominated: same recovery, ~29× the decode
arithmetic, ~2× the CPU per bit, and it carries the battery's only DNFs.

---

### 16.19 The bulk N× test on honest hardware: AES-NI moved the CPU and not one wall — the multipath binder was the flow-control pool, not the receiver thread; the path-scaled pool takes C7 to ×1.72–1.89 of single (0.86–0.94 of Σ) and C8 to 0.79–0.80 of Σ with σ halved (2026-07-14)

**The hardware divide as an instrument.** Every prior L1 measurement ran on
a qemu64 vCPU (SSSE3 only): quinn's TLS did software AES-GCM per packet,
and §16.18 ended attributing gen-mode C7 parity to "the single-threaded
receive/reassembly/delivery engine caps one sink at ≈72–93 Mbit/s". The VM
now passes through the host CPU (E5-2650 v3: AES-NI, AVX2, PCLMULQDQ). The
re-baseline (8 reps × 2 seeds, interleaved, same protocol as every recent
battery) is a controlled experiment on the attribution itself: CPU per
25 MB fell ~30–38 % on both sides (plain recv 2.97→1.99 s; gen-sys recv
3.38→2.36 s) while **throughput did not move in any plain/Copa cell**
(sc2 78.1/75.9 vs 76.1/72.6; C7 102.3/100.2 vs the historic 93–104; sc3 and
C8 likewise inside spread). A CPU wall must move when the CPU gets faster;
this one did not — the receiver-wall attribution is refuted by the upgrade
it ignored. (The one honest mover: gen systematic-repair single-c2
70.9→75.7 = 0.97× plain+BBR — on AVX2 the FEC tax is ~0.37 s recv CPU per
25 MB, essentially free.)

**Where the C7 wall actually lives.** The profile chain, each step
measured: (i) perf on the C7 receiver is FLAT (top symbol 3.9 %; AES-GCM
1.31 % — crypto is noise now); (ii) pinned to ONE core the receiver still
does 95.5 Mbit/s at 0.66 core busy (unpinned −8 % only); (iii) the engine
sink ceiling is **187.7 Mbit/s** (single-path c1; dual-c1 185.3 — the same
wall), ~1.9× the C7 plateau, through the same single receiver task;
(iv) out-of-order delivery does not move C7 (105.6 vs 103) — not the
frontier; (v) the sender DIAG shows `win=1024/1024` pegged with per-path
in-flight collapsing to zero: the plain-reliable OUTSTANDING pool —
`RELIABLE_STORE_MAX` = 1024 symbols, which the delay-based dynamic cap
latches at because the legacy anchor over-reads (§16.13) — is a
per-TRANSFER constant. Little's law closes the case: 1024·1250 B·8 /
80–100 ms echo-RTT ≈ 100–128 Mbit/s — the historic "receiver wall" ~93–104,
CPU-invariant by construction. The same-binary static sweep proves the
mechanism: C7 1024→103, 2048→122.7, 4096→141.3, 8192→143.7 (saturation);
C8 4096→71.5 but 8192→31.8 (slow-path bufferbloat collapse); singles
collapse past 4096 (sc2@8192 = 43). The knee: **≈2048 outstanding symbols
per live path**.

**The fix (measured, gated, minimal).** `RWM_STORE_PATHS` (default OFF,
shipped byte-identical): for N ≥ 2 live paths the dynamic cap becomes
clamp(gain·N·pipe_sum, floor, N·2048); N = 1 is the legacy law bit-exactly
(singles measured inert with the flag ON). Receiver parallelization —
the lever this task was expected to need — was NOT built: the profile
refutes it below ~150 Mbit per sink, and dead mechanisms measure noise
(§16.15's lesson).

**The N× verdict (same-binary A/B, interleaved, 8×2 seeds).** C7 plain+BBR
100.4/101.2 → **136.0/142.1** = ×1.72/×1.89 of the same-session single
(0.86/0.94 of Σ per-path singles 157.7/150.5), Δ = 3–6× the arm σ_s. C8
64.9/55.9 → **75.8/72.3** = 0.80/0.79 of Σ (from 0.69/0.61) with σ halved
(8.9/14.4 → 4.0/6.1) — the historic C8 bimodality was largely
store-starvation. Copa-sole C7 rides the same unlock (×1.44/×1.71 of its
single, from ×1.16/×1.13); Copa C8 is unchanged (its own cwnd law is the
binder there). **The user's bulk claim substantially lands, with the
mechanism chain now honest end-to-end: CC substrate (§16.17) → PMTU floor
(§12.12) → wire mode (§16.18) → decode (§16.18) → crypto (this section:
never the wall on real silicon) → threading (refuted) → FLOW CONTROL (the
actual multipath binder).** Bulk multipath ARQ striping approaches N× once
nothing artificial serializes it — and every "wall" so far has been an
unscaled constant or a hidden substrate controller, not the architecture.

**Residual, named.** C7's last 6–14 %: the pooled flow control's self-queue
equilibrium (deeper pools buy queue, not rate — 4096→8192 is +2.4 Mbit) and,
exactly at that operating point, the engine begins to bind (server pinned
to 1 CPU at pool 4096: 125.6 vs 138.8 on 2 — the first true engine-CPU
signal, at ~140+ Mbit). C8's gap to 0.9: a SHARED pool cannot be sized for
a c2 and a c3 path at once (the slow path needs it shallow — 8192
collapses; the fast path wants it deep). Both point at the same next
lever: PER-PATH outstanding accounting (the FMTCP percap structure) instead
of a pooled cap — after which receiver/sender task parallelization becomes
the relevant frontier above ~150–190 Mbit per sink.

**Addendum (2026-07-18, built, L1 pending).** The named lever is now built
(`RWM_STORE_PERCAP`, default OFF, byte-identical; task #86, branch
`feat/store-percap`): per-path Little's law on the retention store itself.
Each live path gets its own outstanding ACCOUNT with a derived cap

    cap_i = clamp(gain × BtlBw_i × echoRTT_i, floor, pool_knee)

— the per-path delivered-rate anchor times that path's smoothed ack-ECHO
RTT (the account's residence clock is the ack, so the echo RTT, not
RTprop, is the Little's-law time constant; the measured 2048-per-path pool
knee is the ceiling that bounds the echo-RTT positive feedback). A symbol
placed on path i draws account i and releases on the ack that removes it
from the store (SACK/OOO or cumulative); admission pauses only when NO
account has headroom, and a cap-full placement redirects to the path with
headroom — the `fmtcp_percap_full` per-path in-flight structure (§16.7's
#64 fix), generalized to the plain-reliable store. Warm-up inherits an
equal share of the legacy pooled cap and converges as the anchor
establishes; N = 1 keeps the legacy pooled law bit-exactly. This is the
shape that can hold a c3-shallow account at its own pipe while the
c2-deep account deepens — the configuration no SHARED pool can express.
Unit-tested (incl. the deep+shallow C8 conflict in miniature) and
L0-validated at the mechanism level.

**L1 verdict (2026-07-19, `meas/percap-battery`; 25 MB × 8 × 2 seeds,
interleaved, same binary, same-session Σ denominators re-measured).** The
per-path Little's-law story survives at the SYMMETRIC cell and fails at
the heterogeneous one — the opposite of the design's own bet. C7: percap
= the pooled fix or better — plain+BBR 136.5/147.4 = 0.87/**0.97** of Σ
(the ≈1.0 target touched at s7), with the pooled arm's collapse mode
absent (percap σ 4.8–7.8 vs a pooled n=4 run at 84.5), and Copa-sole c7
gains +11/+21 over its own baseline. C8: **regression to 0.38–0.43 of Σ
under BOTH CC families** (plain 37.0/35.1 vs pooled 62.8/67.6; Copa 34.8/
35.2 vs 55.8/55.9; σ collapsed to 1.4–5.8 — a mechanism, not noise). The
gauge-level forensics name the binder: both accounts peg at their caps
(out/cap ≈ 1.0), and the cap-full placement REDIRECT — "send the overflow
to the account with relative headroom" — fills the slow path to its full
cap: ~2048 symbols on a 15.7 Mbit path is ≈1.3 s of store dwell, so
slow-path holes recover ~13× slower, the frontier serializes behind them,
the echo-RTT feedback (dwell→echo→cap) holds the account open (slow-path
echo measured up to 811 ms under Copa), and the all-full admission gate
pauses intake 30–36 % of ticks. Per-path admission accounting per se is
not refuted — per-path admission **with an unguarded redirect** is: the
redirect needs a delay-aware bound (redirect only while the target
account's dwell stays within ~1 recovery round), or the slow cap must
bind by dwell (cap_i ≤ rate_i × recovery budget) rather than the shared
2048 knee. `RWM_STORE_PERCAP` stays default OFF; the C8 record remains
the pooled path-scaled fix. One more datum: percap-c7 lands at ~147
Mbit/s with receiver CPU still falling per bit — the §16.19 "~150
threshold" for receiver/sender task parallelization is now the live
frontier at the symmetric cell.

**Guard addendum (2026-07-19, `fix/percap-redirect-guard`).** The named
delay-aware redirect bound was derived, built, and L1-measured. The
derivation is itself a datum: the natural law "redirect to j only while
its account drains within one echo round" (out_j/rate_j ≤ κ·echoRTT_j,
κ = 1) is PROVABLY vacuous on the loaded echo clock — the app echo is
store-dwell-inclusive (echoRTT ≈ RTprop + out/rate), so the bound chases
its own congestion; that is exactly the measured feedback that held the
slow account open. Solving with κ < 1 collapses to κ = 1 on the FLOOR
clock: **bound_j = rate_j × RTprop_j — a redirect may park at most one
un-queued pipe on the target** (Copa feed: cwnd_j; warm-up: cap_j/gain).
When some account is cap-full and no target is within its bound, the
store reads FULL and the existing admission pause engages — backpressure,
not parking. Measured (same-binary A/B against the unguarded redirect,
`RWM_PERCAP_GUARD=0`, both seeds, both CC families, same-session Σ): the
redirect channel CLOSES — the slow account pins at its bound
(sout = 508/b508; 323/b323 under Copa's honest caps), the parked dwell
collapses ≈4× (echo 1004 ms → 121–301 ms), and c8 recovers half the
regression (0.41→0.55 / 0.40→0.52 of Σ; Copa 0.43→0.54/0.57) with c7
preserved (0.87–0.89 of Σ; the Copa c7 win intact). But the guarded arm
still trails the pooled fix at c8 (0.52–0.55 vs 0.67–0.72 of Σ), because
closing the redirect EXPOSED a second parking channel: the placement
softmax's own picks fill the slow account below a cap the plain-anchor
over-read holds knee-clamped (≈2048; the honest derivation never
engages), and under Copa's honest caps the account structure denies the
fast path the pooled law's borrowing of the slow path's unused share.
The flip stays NO; the follow-ups are named in §17.6 item 1: give the
CAP the same floor-clock dwell bound the redirect got
(cap_i ≤ gain·rate_i·RTprop_i), generalize the §16.15 send-interval
sampler to plain mode, or accept account isolation's asymmetric-cell tax
as structural and keep the pooled law there.

**Honest-cap addendum (2026-07-19, `feat/percap-honest-cap`) — the cap
re-derived on the honest anchor, and the derivation corrected by its own
smoke.** With the §16.21 plain-mode sampler (`RWM_PLAIN_RS`) supplying a
≈1×-truth BtlBw, the named follow-up "cap_i ≤ gain·rate_i·RTprop_i" was
built first in exactly that literal form — and REFUTED before the
battery: c2's true RTprop is 8 ms, so the legacy-good 1024-symbol store
is ~12× the floor BDP, and the floor law computes cap ≈ 150–170 → −25%
throughput (the Anchor-Hygiene sc2 −20% reproduced). The headroom the
old anchor over-read supplied by accident was never ack-batching: it is
the RECOVERY clock. A plain-window hole is recovered by the SACK
re-advertisement/tail-sweep engine, whose round is clamped to
[25, 100] ms — at short RTprop a recovery round is an order of magnitude
longer than the wire round trip, and GE burst loss drives it to the
ceiling. The landed law decomposes on honest clocks only:

    cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i)),  R = 100 ms

with K_i = the windowed-MIN echoSRTT_i/RTprop_i (self-queue-proof: own
dwell can only raise the ratio, so the c8 dwell→echo→cap spiral has no
handle; seed-identity samples where srtt ≡ RTprop are discarded, not
clamped — feeding them latched the min at 1.00). The legacy law
gain·anchor is the K=1, R=0 degenerate, so honest anchors can never
shrink a cap below it. Cross-checks: sc2 → ≈1290 → latches the proven
1024 store; c8-slow → ≈470–500 ≈ the guard battery's measured good pin
(508 outstanding, 0.26 s dwell). Measured (same binary, 5 arms × 4 cells
× 2 seeds × 8 reps, interleaved): the sc2 −20% is RESOLVED exactly
(honest arm = baseline both seeds; the law-off control reproduces
−18/−22%); sc3 keeps a −4.3/−2.3% named residual (the deep store's tail
runway beyond one recovery round); c7 percap lands ABOVE the pooled fix
both seeds (0.89–0.90 of Σ); c8 improves +3.4/+3.8 over the knee-clamped
percap control with the slow-path parking tail halved (per-rep p50 echo
358→204 ms, p90 943→433) — but still trails pooled PBS (0.54–0.55 vs
0.62–0.69 of Σ), and Copa percap trails Copa pooled AGAIN (−8.5/−9.8;
prior session −9.6/−11.2) with caps that are honest by construction.
**Conclusion: with both parking channels closed, the account structure's
NO-BORROWING property is the confirmed c8 binder** — out_fast ≤ cap_fast
denies the fast path the slow path's unused share, which the pooled Σ law
grants for free; the measured tax is ~0.13–0.16 of Σ at the asymmetric
cell. One measured sub-residual: the slow path's send-interval anchor
still over-reads ×3–5 UNDER MULTIPATH PLACEMENT (honest at N=1 and on
the fast path) — suspected frontier-advance burst attribution (a
slow-hole fill releases a burst of already-received fast-path symbols
into the cumulative frontier) — so the honest cap law runs on a
dishonest input at exactly the account it most needs to bound. Flip
stays NO; §17.6 item 1 redirects from cap hygiene to bounded account
borrowing (a borrowed symbol parks on the lender's account but flies on
the borrower's pipe — the lender's dwell-bound derivation no longer
describes its own queue, so this needs a new law, not a clamp), or to
accepting pooled PBS as the c8 record.

---

### 16.20 One decoder, one continuous mechanism: the unified span machine (task #61, the principle debt) (2026-07-18)

**The debt, stated exactly.** The axiom of this whole document is ONE
mechanism parameterized by the (δ, ρ, r) triangle (§1.4, §15). The
implementation has instead accreted THREE receive machines and a HARD
protocol switch between them:

1. the **sliding-window RLC machine** (`RlcWindowDecoder`): per-seq
   incremental Gaussian elimination over a moving window — the plain
   window-reliable path (Bulk/Auto + `--window-reliable`, generation off);
2. the **generation machine** (`GenerationDecoder`, §16.3/§16.18): dense
   per-(anchor, width) systems with the sparse-aware known-column
   elimination — the bulk generation/systematic path;
3. the **streaming two-layer code** (`StreamingDecoder`, wrapping the
   `streaming-codes` crate): the code the Realtime hint actually
   auto-selects (`net/mod.rs` backend selection) — a genuinely different
   code family (diagonal burst layer + random layer, its own 15-byte
   repair header).

The switch is at the PROFILE level: hint = Realtime picks machine (3);
`--window-generation-coding`/`--window-systematic-repair` pick machine (2);
otherwise machine (1). Nothing continuous connects them, double (triple)
maintenance, and the M*/A* depth law of §16.17 engages only in machine (2)
and only at BDP > G — unvalidated in exactly the regime it was derived for.

This section derives the unification for the RLC family — machines (1) and
(2), which already share one wire format by design — and states honestly
where machine (3) stands.

#### 16.20.1 Both RLC machines decode the same equation set — the difference is algebra SCOPE, not code

Every RLC-family wire symbol is a self-describing linear equation over the
global source-sequence variable space:

```
   repair (a, w, i):   Σ_{c=0}^{w-1} coeff(a,w,i)[c] · x_{a+c} = payload
   source s:           x_s = payload            (a unit equation)
```

(the FILL_FLAG variant is the same equation with columns [cw, w) zeroed —
§16.3). The receiver holds a consistent affine system; linear algebra fixes,
uniquely, the **maximal determined subset** of variables — the delivered
set is a property of the EQUATIONS, not of the decoder. The two machines
differ only in how much of that closure they compute:

- `RlcWindowDecoder` computes the FULL closure: one global incremental GE;
  any two equations that overlap on an unknown can eliminate against each
  other, whatever spans they came from.
- `GenerationDecoder` computes a **block-restricted closure**: equations
  are keyed by `(anchor, width)`, each key gets an isolated RREF system,
  and only **rank-1 information** (fully solved sources) propagates between
  systems (`inject_source_into_active_gens` / `propagate`).

**Where they provably coincide:** when the wire's spans are ALIGNED (every
equation for a region carries the same `(anchor, width)`) the global system
is block-diagonal, cross-block elimination is vacuous, and the two closures
are equal. That is generation mode by construction — which is why the
sliding decoder could decode the generation wire unchanged (§16.3) and the
old-vs-new differential test could demand set equality (§16.18).

**Where they provably differ:** two or more UNDERDETERMINED equations with
different `(a, w)` keys whose unknown sets overlap and are jointly
determining. Minimal trap: holes {h₁, h₂}, repair A over a span covering
both, repair B over a DIFFERENT span also covering both. Global GE: rank 2
over 2 unknowns ⇒ both solve. Keyed machine: two systems, each rank 1 over
2 unknowns, no partial-row propagation ⇒ both strand (until ARQ). On a
MOVING-window wire this is not a corner case — it is the generic 2-loss
burst: consecutive repairs are emitted over the current window, whose
anchor slides with every source symbol, so the two covering repairs of a
burst almost always carry different spans. **The keyed machine is therefore
not a valid drop-in for the sliding wire; the sliding machine IS valid for
the aligned wire (only slow — §16.18's measured ~200×).** The `(anchor,
width)` keying is an OPTIMIZATION exploiting block-diagonal structure, not
a semantic.

#### 16.20.2 The unified decoder: global incremental RREF with the sparse-aware cost model

The unified machine (`UnifiedDecoder`, `src/fec/unified.rs`) is the global
closure of (1) computed with the cost model of (2):

- **Known columns never enter the matrix.** Received/recovered sources
  live in a payload store; an incoming row's known columns are eliminated
  payload-only (S bytes each) — §16.18's sparse-aware core, now global.
- **Only coded rows are matrix rows**, kept in RREF, stored as one fused
  contiguous buffer `[coeffs over the row's SPAN | payload]`. A row's span
  is an interval `[start, start+len)`; eliminating two overlapping rows
  yields a row whose span is the union — still an interval — so rows stay
  dense-over-span (no per-coefficient BTreeMap, no cascade allocation: the
  two measured killers of the old sliding decoder).
- **Unit rows deliver immediately** and convert to known columns
  (payload-only back-elimination through every covering row, worklist for
  the transitive cascade). A source symbol arriving late is just a unit
  equation — the whole `inject/propagate` apparatus of the keyed machine
  becomes a one-line invariant.
- **k = 0 fast path:** a repair whose span is fully known is recognized
  redundant in O(w) with zero GF work.

Cost per solve involving k coded rows of span ≤ L: **O(k·L·S + k²·(L+S))**
— identical to §16.18's per-generation bound when the wire is aligned
(L = G, systems block-diagonalize automatically), and bounded by L ≤ W on
the sliding wire. Delivered set: the full closure — **equal to
`RlcWindowDecoder` on every wire** (both compute the maximal set, per
arrival), and **⊇ `GenerationDecoder` on the aligned wire** with equality
whenever blocks are disjoint (the extra deliveries appear exactly in the
mixed-width same-anchor overlap the keyed machine documents as separate
systems — the object-tail case). Both statements are enforced by
differential test (below), with the pre-§16.18 dense decoder kept as the
third, reference oracle.

**The A\*=1 degeneracy, stated precisely.** At the decoder there is nothing
left to degenerate: sliding-window and generation wires are the SAME input
language, and the machine is span-agnostic. The two "machines" were the two
extreme SENDER span policies: moving anchor with w = W (sliding) vs pinned
anchor with w = G, M deep (generation). The realtime/bulk switch was never
about decode — it was an emission-span policy switch. So the unification
lives on the sender:

#### 16.20.3 The sender span law: (δ, ρ, r) → (A*, M*, Δ) with no mode bit

All emission-span structure derives from one dimensionless quantity, the
**recovery budget in symbols** `N_δ = rate · D`, with

```
   D  = min(H, 2·RTprop)          the recovery deadline:
        H       = the delivery horizon the receiver will actually wait
                  (EVICT/realtime: reorder_timeout, the δ dial; RETAIN/ρ=1: ∞)
        2·RTprop = delivery + one feedback round — past this, recovery
                  belongs to ARQ/deficit anyway (§16.17's D)
   rate = the windowed-MAX delivered rate (§16.15's statistic), RTprop
          min-filtered — the two measured anchors, never live-SRTT
```

and three derived parameters, all continuous in δ:

```
   A* = clamp(N_δ, 1, W)                     the coding-quantum / span width
   M* = ceil(rate · 2·RTprop / A*_q) + 1     quanta in flight (A*_q = A*
                                             quantized to the retained grid)
   Δ  = ceil(rate · J)                       trailing offset: the span must
                                             end Δ behind the send frontier
                                             so every member has LANDED when
                                             the repair does (J = jitter
                                             anchor; same-path FIFO makes
                                             Δ small, not zero)
```

Emission: per source symbol the quantity law banks `owed += r` (the #85
`TaperBudget` — the wire consumes r as computed, §16.17/goal-gate "Taper
Emission Fix"), and each granted repair is coded over the TRAILING span
`[F, F+A*)` with `F+A* ≤ sent − Δ`, `F` = the oldest unresolved position
(cumulative ack + 1, clamped into the retained window). This is #85's
solvable-span requirement made structural: a repair is solvable AT ARRIVAL
iff its span contains no still-in-flight member, i.e. iff it trails the
frontier by Δ — the leading-window emission (the measured −22 pp defect)
violates exactly this.

**The two limits.**

- **Realtime (small δ):** H ≈ 20 ms ⇒ D = H ⇒ A* = rate·H (e.g. ~4 at
  200 sym/s voice, ~98 at c3-class 20 Mbit) — small fresh spans trailing
  the frontier, solvable on arrival, delivery at the k-th covering
  equation's ARRIVAL; M* ≤ 2 (the depth term is inert below BDP ≈ A*).
  This IS the sliding-window machine's behaviour, now derived instead of
  hard-coded — **the property that buys the tail win is (a) per-arrival
  incremental decode and (b) span freshness (spans the receiver can solve
  inside H), both of which the unified machine preserves by construction
  (delivered-set-and-timing equality with the legacy sliding machine is
  the differential test's assertion, not an aspiration).**
- **Bulk (large δ, ρ = 1):** H = ∞ ⇒ D = 2·RTprop ⇒ A* = 2·BDP clamped by
  W → the retained-grid quantum G (§16.5's four bounds cap W; A*_q = G),
  M* = ceil(rate·2·RTprop/G)+1 — **verbatim §16.17's derived pipeline
  depth**, now reached as the large-δ limit of the same formula that
  yields the realtime span. Stable anchors (the generation grid) are the
  A*_q quantization, kept because fungible cross-path DoF (§16.3) requires
  a pinned span, not because "bulk is a different machine."

Between the limits δ moves A* and M* smoothly; there is no value of δ at
which a different machine takes over — the oracle's continuity sweep
(temporal_oracle PART 7) checks precisely that no completion/tail cliff
appears at any δ between the two limits, and validates the M* depth term
in its engagement regime (RTT 100/200, BDP > G — the §16.17 residual).

#### 16.20.4 Honest re-examination of the #85 span probe

Re-deriving the span law forced a re-read of the #85 differential probe,
and it does not survive: the probe emitted trailing-span repairs tagged
`FecBackend::Rlc` (`build_frontier_repair`) into a tunnel whose Realtime
hint had auto-selected the STREAMING backend — the receiver's
`StreamingDecoder` drops mismatched-backend symbols on entry
(`add_symbol`'s backend guard), so the probe's repairs were **never fed to
any decoder**. The 62.5%-vs-50–57.5% "span, not quantity" datum therefore
measured trailing-span repair as PURE WIRE LOAD, not as recovery; binder #3
(emission span) remains PLAUSIBLE — the taper arms' leading-window repairs
WERE decoded and still degraded delivery — but its L0 confirmation is VOID.
The span law above is re-established in this task's L0 battery with the
whole RLC family end-to-end (encoder, wire, unified decoder), where the
trailing-span repairs demonstrably enter the decoder (repairs_useful > 0
is the liveness gate). This is what MEASUREMENT DISCIPLINE rule 1
(mechanism liveness at the RECEIVER, not just the sender) exists to catch.

**MEASURED (the re-run, goal-gate "Unified Decoder", 2026-07-18).** On the
RLC family at the same c3heavy cell (seeds 42/7, 40 objects/arm,
interleaved, same binary) the #85 −22 pp does not exist AT ALL: every RLC
arm — taper off, leading-window taper ON (cod/src 0.42–0.43), and the
unified trailing span (0.43–0.47) — delivers 40/40 with 0 DNFs, at the
very cell where the shipped streaming arms DNF 35–62%. The #85
degradation was therefore a property of the STREAMING-family arms, not of
r-consumption per se; on the RLC family the question moves to the
completion TAIL, where the trailing span beats the leading window at p90
on both seeds (0.405 vs 0.936 s; 0.263 vs 0.313 s, medians tied) and the
p99 verdict (1–2 outliers at n = 40) is queued to L1.

#### 16.20.5 Constants audit

Every parameter of the unified machine, and where it comes from:

```
  parameter        derivation                                   anchor/source
  ─────────────    ──────────────────────────────────────────   ─────────────
  r                §8.4/§8.4.1 r* (GE + measured tail mass)     ε̂, σ²_burst, δ
  taper shape      GE survival (1−q̂)^t renorm. over window      q̂ (GE estimator)
  owed cap         r·W (the budget IS r × window)               r, W
  A*               clamp(rate·D, 1, W)                          rate, D(δ)
  D                min(H, 2·RTprop)                             δ (H), RTprop
  M*               ceil(rate·2·RTprop/A*_q)+1                   rate, RTprop, A*_q
  Δ                ceil(rate·J)                                 jitter anchor
  W                §16.5 four bounds (latency/burst/mem/decode)  δ, ε̂, memory
  H                hint δ dial (EVICT) / ∞ (ρ=1 RETAIN)          δ, ρ
  retention        ρ = 1 ⇒ RETAIN-UNTIL-ACKED, else EVICT       ρ
  RESIDUAL CONSTANTS (named, not derived):
  M* clamp [2,32]  cold-start floor / memory ceiling            GEN_PIPE_MAX_GENS
  pacing ×1.25     BBR probe gain (§16.17)                      literature const
  Δ floor 1        FIFO-per-path minimum                        structural
  grant ≤ 1/src    source clock paces repair (no bursts)        structural (#85)
```

#### 16.20.6 What is and is not unified (status)

**Built (this task, branch `feat/decoder-unify`, env `RWM_UNIFIED`, default
OFF = legacy byte-identical):** the unified decoder replaces BOTH RLC-family
decoders behind the gate; plain-mode proactive emission becomes
quantity-lawed (TaperBudget) trailing-span emission with A*/Δ from the
derivation; generation mode keeps its machine (it already IS the aligned
large-δ limit) with the M* law active; the Realtime hint under the gate
rides the RLC family (δ-parameterized) instead of switching code families.
Differential tests (unified vs legacy sliding on moving-span traces;
unified vs keyed + pre-§16.18 reference oracle on aligned traces) and the
local L0 δ-sweep are the evidence rung; the L1 parity battery (realtime
tail p50/p99 vs legacy, bulk throughput vs gen-sys, RTT 100/200 depth-term
cells) is QUEUED — the shipped default stays legacy until it passes.

**Evidence status (2026-07-18, goal-gate "Unified Decoder"):** the
differential suite is green on all three legacy machines — EXACT per-call
equality on the aligned wire (vs the keyed machine AND the reference
oracle, including the added-rank accounting) and on in-order moving-span
traces (vs the sliding machine); under reorder the unified machine is a
strict superset because the sliding machine measurably LOSES RANK when a
late source arrives for a seq already held as a row pivot (it discards
the displaced, still-informative row — a legacy defect found by this
unification, isolated in its own unit test). Oracle PART 7: δ continuum
with no cliff, anchor handoff metric-inert, M* knee at RTT100 (m=2 0.64×
of M*=6) and RTT200 (0.39× of M*=10). L0: δ-sweep no-cliff on both
machines, bulk gen-sys median parity ≤ 1.2%, realtime cell tail class
preserved (p50 tie, p90 ≤ legacy on both seeds).

**Not unified, honestly:** (a) the STREAMING two-layer code remains a
separate family — its diagonal burst layer is a genuinely different
construction (deterministic burst-optimal interleaving) that the random
RLC span machine does not reproduce; the shipped Realtime hint still
selects it by default, and the 12–48× message-tail crown jewel (goal-gate
"Full Benchmark Re-Run", Metric A) was measured ON it — retiring it in
favour of the unified small-δ machine is gated on the queued L1 tail
parity, not asserted here. (b) The block pipeline (RaptorQ bulk default
without `--window-reliable`) is §15's other knob and untouched. The
principle debt this section discharges is the RLC-family fork — the two
machines that were one mechanism all along, split only by an emission-span
policy that is now a formula.

#### 16.20.7 L1 status (the flip-gate battery, 2026-07-19) — both flips NO, with the properties named

The queued parity battery ran on the passthrough VM (goal-gate "Unified
Decoder" L1 RESULTS: one binary, sha256-identical to the prior batteries',
seeds 42+7, interleaved arms, liveness echoes at both endpoints). What it
settled:

- **The large-δ half of the unification passes every gate it was given.**
  On the gen-sys wire the unified global decoder is throughput-parity with
  the keyed machine (sc2 72.2/72.1 vs 75.3/73.4; c7 81.8/82.4 vs 83.9/77.6
  Mbit/s — every Δ within σ_s) and CPU-parity at sc2 (2.38 → 2.40 s recv
  per 25 MB; c7 +3–5% recv, recorded), i.e. the global sparse-aware RREF
  really does block-diagonalize to §16.18's cost on the aligned wire at
  L1, not just in the differential suite.

- **The small-δ limit is delivery-complete but not tail-parity.** At the
  c3 realtime perf cell the unified machine delivers 99.4/100% where the
  shipped streaming code leaves 24–26% DNFs, with cod/src 0.34–0.42 — the
  §16.20.3 span law consuming r as computed AND recovering in-window at
  the receiver (the liveness #85 lacked). But under sustained streaming
  load its p99 medians run 2.7–3.3× the legacy-RLC arm's at the bursty
  cell, with a 3/10-rep stream-collapse class (p50 in seconds) that
  neither legacy machine's completed reps show. `RWM_UNIFIED` therefore
  stays DEFAULT OFF: the flip gate demanded ≥ legacy-RLC everywhere, and
  the collapse class is a named blocker, not a rounding error. The
  streaming two-layer keeps Realtime. The measured trade for the roadmap:
  +24–26 pp delivered reliability for ×3–4 completer medians and the
  collapse tail — a different (δ, ρ) operating point, not parity.

- **The M\* knee is unreachable on today's L1 wire.** All four
  depth×machine arms sit flat (≈35 Mbit/s at RTT 100, ≈20 at RTT 200)
  because M\* never leaves its cold-start floor: the RTprop floor the
  depth law reads carries a 50-ms default-seeded sample for the life of a
  10-s run at a 200-ms cell (floor-freshness failure), the delivered-rate
  anchor is itself window-throttled (warm-up loop), and the win backstop
  is a static (pipeline+2)·G. The probes show the resulting signature —
  `win=768/768` pegged, budget-stall 90–95%, cwnd and BDP caps slack.
  PART 7b's knee prediction (m=2 at 0.64×/0.39× of M\*) is neither
  confirmed nor refuted at L1; it is gated behind fixing those anchors.
  The §16.17 depth law's first L1 validation therefore remains OPEN, with
  the two anchor defects as the named prerequisite. Unified ≡ legacy at
  every knee point — the decoder swap is knee-neutral.
  **[CLOSED 2026-07-19 (§16.21): with the anchor pair fixed
  (`RWM_MSTAR_ANCHOR`) the knee ENGAGES — r100 +25–31%, r200 +62–82%
  over the same-session hygiene-off control, both seeds, n=8, 0 DNF;
  PART 7b confirmed in direction/ordering, measured m=2 deficit
  shallower than in-model (other wire binders).]**

- **An ordering surprise worth keeping:** at these L1 tail cells the
  legacy-RLC realtime arm posts the best p99 medians of all three machines
  (234/273 ms at c3 vs streaming's 510/822) — the streaming code's L0/
  benchmark superiority does not automatically transfer to every L1 cell.
  Any future streaming-retirement case must engage with this datum, not
  only with unified-vs-streaming.

#### 16.20.8 The collapse class attributed (2026-07-19) — the decoder is exonerated; the blocker is an anchor defect plus a δ-contract violation under overload

The §16.20.7 blocker — the unified-realtime 3/10 stream-collapse class at
c3-1200B — was reproduced at a new L0 sustained-stream rung (the L1 stream
shape over two real engines under the transport netem shim at the exact c3
parameters) and traced with decoder-internal, sender-span, and
transit-layer instrumentation (goal-gate "Unified Decoder" → COLLAPSE
ATTRIBUTION for the full evidence). Three structural findings:

1. **The global RREF's realtime cost model is confirmed at the wire — by
   emptiness.** In every rep, collapse or clean, the unified decoder holds
   ZERO coded rows at essentially every sample: trailing-span repairs
   arrive solvable (their members already delivered ⇒ the k=0 fast path)
   or deliver immediately; per-arrival decode is 6–11 µs, total decode
   compute 12–21 ms per 20-s stream. The feared small-δ failure modes of a
   global closure — active-set growth under a stalled frontier,
   re-elimination storms, allocation churn — do not occur, because span
   freshness (§16.20.3) keeps the matrix empty. The collapse is not decode.

2. **A span law is only as good as its anchors — and A\* joins M\* in the
   anchor-defect family.** The A\* = clamp(rate·D, 1, W) rate anchor is a
   2-s-interval, α=0.125 EWMA of the send rate: at a 150-sym/s realtime
   stream it holds A\* = 1 for the first ~10 s (a width-1 span is a
   duplicate of one near-frontier symbol ⇒ repairs_useful/repairs_fed
   ≈ 9%, recovery ARQ-bound), and the post-transient ack flood poisons it
   (A\* 1→38; delivered-rate ×13 over the link rate; cwnd ×16). §16.20.7's
   M\* verdict already named the RTprop floor and warm-up loop; the
   realtime limit inherits the same disease. The derivation stands; the
   MEASUREMENT of its inputs is the open engineering.

3. **The collapse class itself is family-level and semantic, not
   algorithmic.** A whole-process transient (~1 s scheduler/timer stall —
   observed directly via transit counters freezing in both engines at
   once) is amplified by BOTH RLC-family realtime arms into chained
   multi-second whole-stream backlog: the reliable-in-order EVICT pipeline
   serializes every message behind post-stall recovery while poisoned
   anchors extend the disturbed regime. The streaming two-layer arm under
   the SAME transient sheds ~1% of messages past its reorder horizon and
   its p50/p90 never move. That is the δ-contract stated operationally:
   at small δ, overload must be shed, not serialized — the property the
   streaming machine has and the RLC realtime parameterization currently
   lacks. Flip (a) is therefore gated on the anchor repair (+ clock-gap
   estimator hygiene) and on giving the unified small-δ machine an
   explicit overload-shedding policy, not on any decoder work.

---

### 16.21 Anchor hygiene: three laws for a measured anchor, and the convergent defect family they name (2026-07-19, `feat/anchor-hygiene`)

Three independent investigations ended at the same place. The unified-realtime
collapse attribution (§16.20.8) found A\* pinned at 1 by a cold 2-s EWMA and
flood-poisoned 1→38 by a post-stall ack burst; the #61 knee battery (§16.20.7)
found M\* pinned at its cold-start floor by a 50-ms RTprop "sample" that never
expired plus a static `(pipeline+2)·G` backstop; the per-path outstanding
accounting battery (§16.19 guard addendum) found the plain-mode delivered-rate
anchor over-reading ~8–10× and knee-clamping every derived cap. Different
subsystems, one disease. The unifying principle, stated as three laws:

**An anchor is trustworthy only if:**

1. **It is seeded from measured sends.** The estimate must be a windowed
   statistic (max, for a bottleneck-rate anchor — §16.15/§16.17's lesson) of
   REAL samples, live within ~1 RTT of the first send — never a constructor
   default or a multi-second EWMA crawl standing in for a measurement during
   warm-up. A default that survives warm-up doesn't just delay the anchor: it
   gets RECORDED as if it were data (the 50-ms peer-report seed entering the
   min-RTT floor window every 2 s) and then outlives every real sample.
2. **Its samples exclude scheduler clock gaps.** A sample whose interval
   spans a whole-process stall measures the stall, not the link: the release
   flood's ack-interval Δt collapses (delivered-rate reads ×13 the link), and
   its echo RTTs carry the stall (EWMA-RTT ×3). Detection must be on the
   PROCESS clock — a fixed-cadence timer whose tick interval stretches ≫ its
   period — because on the ack clock, silences of 0.5–3 s are legitimate
   protocol behavior at high-RTT lossy cells (frontier waves, deficit
   rounds), and an arrival-clock detector was MEASURED discarding exactly
   the post-recovery ack waves that carry the true rate. Discard the
   quarantined samples; never average them in.
3. **Its floors and backstops expire.** A floor that outlives its min-window,
   or a "cold-start backstop" that governs the whole transfer, is a constant
   wearing a floor's clothes. The static FMTCP win backstop bounded every
   r100/r200 transfer end-to-end (win pegged, budget-stall 90–95%); the
   repaired form derives from M\* the moment the anchors are live, so the
   static value's reign is bounded to the measurement warm-up (~one rate
   bucket).

Every defect in the family violates at least one law, and every fix is an
application of one (branch `feat/anchor-hygiene`, all env-gated, shipped
default byte-identical; umbrella `RWM_ANCHOR_HYGIENE=1`):

- **A\* rate anchor** (`RWM_ASTAR_ANCHOR`, law 1+2): the span law's rate is a
  windowed-max send-rate anchor (bucket ≈ SRTT/2, window ≈ 8 SRTT) fed by the
  sender's own send events, replacing the 2-s-interval α=0.125 EWMA; buckets
  spanning a clock gap — and the release-flood buckets behind them — are
  discarded, and the window holds its pre-gap value through the disturbance.
  The unit law is the flood-poison injection: a synthetic 1-s gap plus the
  full backlogged burst must not move the anchor.
- **M\* anchor pair** (`RWM_MSTAR_ANCHOR`, law 1+3): the PathReport's
  `avg_rtt_us` — the PEER'S ESTIMATOR VALUE, seeded at 50 ms and, on a pure
  receiver, never fed by a measurement — is no longer recorded as an RTT
  sample; the local RTT EWMA seeds from its first measured sample; the
  delivered-rate filter seeds from 500-ms buckets; the win backstop derives
  `(M*+2)·G` once anchors are live (cold-start M\*=2 reproduces the legacy
  4·G exactly).
- **Plain-mode send-interval sampler** (`RWM_PLAIN_RS`, law 1): the §16.13
  BBR sampler (send-interval Δt = max(send_elapsed, ack_elapsed), windowed
  max, app-limited exclusion), already carried by the Copa-feed WindowAck
  attribution machinery, generalized to plain window-reliable mode under ANY
  substrate CC — sampling-only: the per-path BtlBw/BDP anchor gets clean
  samples while cwnd dynamics keep their legacy cadence and the store-cap /
  percap laws stay on their legacy branches. This is the fix the §16.19
  guard residual (i) named: honest `cap_i = gain·BtlBw_i·RTT_i` needs an
  honest BtlBw_i.
- **Post-stall hygiene at the shared sampling layer** (`RWM_CLOCK_GAP`,
  law 2): one process-clock `StallWitness` (50-ms tick, quarantine =
  min(gap, 2 s)) consulted at the ack feed sites (Ack/WindowAck/PathReport
  arms, report-tick throughput feed) — factored once, not scattered per
  estimator. The first arrival-clock implementation is retained in the
  module doc as a REFUTED design with its measurement (the r200 discard
  storm) — the negative result is part of the law's derivation.

Measured before/after (goal-gate "Anchor Hygiene" for the full tables;
same-session interleaved hygiene-off controls everywhere):

- **A\*** ([SPAN] trace, c3-1200B stream): base `a_star=1` at every sample
  of a 20-s stream; hygiene a\* at its derived value by the second 500-ms
  sample. Stream p90 improves 94 → 78 ms (median of 14 seeds) with p50
  unchanged — the FEC inertness (ru/rf ≈ 9%) closing into in-window
  recovery. The flood-poison injection is a permanent unit law.
- **M\* knee** (L1, gen-sys 25 MB, n=8 × 2 seeds, 0 DNF): r100 36.5/38.8 →
  47.9/48.5 Mbit/s (+31/+25%); r200 19.2/20.3 → 34.9/32.9 (+82/+62%,
  non-overlapping per-rep distributions). Oracle PART 7b
  (raptorpath-math): the m=2 deficit is REAL, in the predicted direction
  and ordering (deeper at r200); measured m=2/M\* ratios 0.76–0.80 (r100)
  and 0.55–0.62 (r200) vs the in-model 0.64/0.39 — the wire keeps binders
  the oracle does not model.
- **Plain BtlBw truth** (L1, DIAG gauge vs known link rates): sc2
  ×4.6–6.2 over-read → 1.02× truth; the c8 slow path's ×4.7–7.4
  knee-clamp over-read → ≤1×. c8 plain throughput improves with the
  bimodal spread collapsed (σ 19.1 → 4.0 at s7); named cost: sc2 single
  −20% (the over-read was accidentally load-bearing for the anchor-sum
  store cap — the §16.19-documented circularity), so the sampler is a
  cap-derivation/measurement arm, not a default candidate as-is.
- **Collapse incidence** — honest: the environmental trigger did NOT fire
  in 68 local reps this session (0 outage-class in BOTH arms, quiet and
  compile-loaded), so no incidence delta is claimable; the amplifier
  removal is claimed at the unit/anchor level, and the one-pass L1
  readiness probe (0/13 unified-1200B collapse reps; unified best-of-three
  p99 median in its session) reads clean.

Honest scope: the collapse TRIGGER is environmental (a host scheduler
stall is outside the transport); these fixes remove the transport's
AMPLIFICATION of it (anchor poisoning extending the disturbed regime), not
the transient itself, and the δ-honest overload policy (§16.20.8 fix C)
remains a separate, unbuilt gate.

---

### 16.22 Bounded account borrowing: the principled point between isolation and the pool (2026-07-19, `feat/store-borrowing`)

Two batteries confirmed the same structural fact (§16.19 guard + honest-cap
addenda): per-path outstanding accounts with honest caps WIN the symmetric
cell (c7 ≥ pooled, both CC families, both seeds, three sessions) and LOSE
the asymmetric cell (c8 PBP-H 0.54–0.55×Σ vs pooled PBS 0.62–0.69; Copa
C1P-H −8.5/−9.8 vs C1 with caps honest by construction) — because
`out_fast ≤ cap_fast` denies the fast path the slow path's unused share,
which the pooled Σ law grants for free. The two extremes are both wrong:
unlimited pooling reproduces the #86 parking collapse (any path's symbols
can bloat the shared budget onto the slow pipe), and total isolation pays
the measured ~0.13–0.16×Σ no-borrowing tax exactly where paths are
asymmetric. This section derives the point between them. The derivation
precedes the build.

#### 16.22.1 Semantics: what an account actually bounds, and what a loan is

The retention store is memory plus recovery-runway. Account i bounds the
store share whose symbols FLY on path i; its honest cap (§16.19 honest-cap
addendum) decomposes on measured clocks only:

    cap_i = rate_i·K_i·RTprop_i            (residence: one honest pipe)
          + rate_i·(gain−1)·(R + RTprop_i) (runway: one recovery round)

A **loan** is a placement in which the symbol FLIES on borrower j's pipe
(the placement law chose j; j has pipe headroom) but is CHARGED to lender
i's account (j's account is cap-full; i has account headroom it is not
using). Two facts make this well-posed, and they are the whole reason
borrowing differs from the refuted #86 redirect:

1. **The lender's dwell law is NOT violated.** The borrowed symbol's dwell
   is `fly_j/rate_j` on j's pipe — it never queues on i. The account
   ledger moves; the wire placement does not. (The #86 redirect moved the
   WIRE placement — parking fast overflow on the slow PIPE at ≈1.3 s
   dwell. Borrowing is the accounting dual: the pipe keeps the symbol,
   the ledger absorbs it.)
2. **The lender's future recovery-runway IS consumed.** Until the borrowed
   symbol acks, cap_i − out_i is smaller than i's own derivation assumed;
   if i takes a loss burst during the loan, part of its runway is lent
   out. The loan's cost to i is therefore measured in TIME: the return
   latency, which is the borrowed symbol's expected residence on j.

#### 16.22.2 The bound

Let the loan's return latency be the borrowed symbol's expected residence
on the borrower's pipe, on the FLOOR clock (the loaded echo clock is
self-referential — the §16.19 guard derivation refuted it once already):

    T_return(j) = fly_j / rate_j + RTprop_j

(`fly_j` = symbols currently FLYING on j — the pipe gauge, which under
borrowing is `out_j − lent_j + borrowed_j`, the account occupancy corrected
by the loan ledger; queue drain plus one flight.) During the loan, lender i
can newly place at most rate_i·T_return(j) of its own traffic (its intake
is bounded by its own drain rate). "Lend only headroom the lender cannot
use within the loan's return latency" is then exact:

    lend_i→j ≤ max(0, cap_i − out_i − rate_i·T_return(j))

Every term is measured or already derived: out_i is the account gauge,
cap_i the honest cap, rate_i the honest per-path anchor (BtlBw_i under
`RWM_PLAIN_RS`; cwnd_i/RTprop_i under the Copa-sole feed), RTprop the
windowed-min floor. **No new constants.** Warm-up (no anchor on either
side) lends nothing — the degenerate is isolation, not the pool.

**Post-loan solvency invariant.** After any admissible loan, by
construction `cap_i − out_i ≥ rate_i·T_return(j)`: the lender retains at
least its own full intake rate for the loan's whole expected residence —
its own picks are never admission-blocked by lending before the loan is
expected back. Repayment is the existing release machinery: the loan is
charged to account i at placement (`percap_charge` to i) and released by
the ack that removes the symbol from the store — a loan self-liquidates,
no new lifecycle.

#### 16.22.3 What the bound implies (the four required properties)

**(a) The aggregate law — no pooled regression.** Loans are charged inside
the lender's account and gated on that account's headroom, so
`out_i ≤ cap_i` for every i at all times, hence `Σ outstanding ≤ Σ cap_i`
— the same honestly-derived aggregate the isolation arm has. Borrowing
moves headroom between ledgers; it can never mint it. (The pooled arm's
failure mode — one path's symbols bloating the whole N×knee budget onto a
slow pipe — requires exceeding some cap_i, which no loan can.)

**(b) The asymmetric (c8) shape — and lending is one-directional by
construction.** Take the honest c8 anchors (rate_f ≈ 10.4k sym/s,
RTprop_f 8 ms; rate_s ≈ 2.1k, RTprop_s 60 ms; caps ≈ 1230/500):

- slow → fast: T_return(fast) ≈ cap_f/rate_f + RTprop_f ≈ 0.13 s;
  reservation = rate_s·0.13 ≈ 260. The slow account's runway slack beyond
  ~260 symbols is lendable — the fast path rides through slow-hole
  frontier stalls (whose duration is the SLOW path's recovery round,
  R + RTprop_s ≈ 160 ms, which its OWN runway term never funded) on
  headroom the slow path cannot use in that horizon. This is exactly the
  share the pooled law was granting implicitly.
- fast → slow: T_return(slow) ≈ fly_s/rate_s + RTprop_s ≈ 0.2–0.3 s;
  reservation = rate_f·T_return ≈ 2000–3200 ≫ cap_f ≈ 1230 ⇒ lend ≡ 0.
  **The fast path can never lend toward a slow pipe**: the reservation
  term prices the lender's refill during the loan, and a fast lender
  refills its whole cap many times over while a slow-pipe loan is out.
  The #86 parking direction (deep budget onto the shallow pipe) is not
  guarded against — it is UNREPRESENTABLE under the law.

**(c) The symmetric (c7) neutrality theorem — loans are identically zero.**
A borrower asks only when its account is cap-full (`out_j ≥ cap_j`). Then

    T_return(j) ≥ cap_j/rate_j + RTprop_j
    reservation = rate_i·T_return(j)
                ≥ (rate_i/rate_j)·cap_j + rate_i·RTprop_j

At a symmetric cell (rate_i = rate_j, RTprop_i = RTprop_j, hence
cap_i = cap_j): reservation ≥ cap_i + anchor_i > cap_i ≥ cap_i − out_i,
so lend_i→j = 0 for every state of the lender. Symmetric neutrality is
EXACT, not approximate: the c7 percap win (0.89–0.90×Σ, three sessions)
is preserved by proof, not by tuning — the borrowing arm at c7 must
measure as the no-borrow arm plus noise, and that prediction is part of
the battery.

**(d) The degenerate cases frame the design space.**
- Borrowing disabled (or T_return := ∞): the current percap arm, verbatim.
- Reservation dropped (T_return := 0): lend up to cap_i − out_i — the
  pooled Σcap law with honest per-path sizing (any account's slack is
  anyone's), which restores the parking channel in the fast→slow
  direction; the reservation term is precisely what separates the
  principled point from the pool.
- Caps un-derived (knee-clamped) + reservation dropped: the PBS
  path-scaled pool itself.

#### 16.22.4 Composition, and what borrowing does not touch

Placement order for a softmax pick landing on a cap-full account j:
**borrow first** (stay on j's pipe, charge the best lender — the one with
the largest lend room), **else the §16.19 guarded redirect** (move the
symbol to a pipe that can drain it within its floor-clock bound), **else
FULL** — the existing admission pause (backpressure, don't park). The
admission gate opens iff some account has own headroom or some (i, j) lend
edge is open; it is the guarded gate plus the loan edges. Own picks below
cap are never gated (unchanged). N = 1 computes nothing (percap_caps
empty — bit-exact singles, the standing identity-control obligation).

Borrowing changes ledgers only. The wire placement law, the send-interval
sampler and its flight-witness attribution (residual (iii), fixed in this
same branch), the SACK/cumulative release machinery, and the honest-cap
derivation (whose terms are all self-queue-proof and carry no handle for
loaned dwell) are untouched.

**Honest limits, named.** (1) T_return is the EXPECTED residence: a
borrowed symbol lost on j returns one borrower-recovery-round late
(R + RTprop_j), eating lender runway the reservation did not price; the
gate's backpressure and the (gain−1) runway term bound the exposure, and
the loan ledger gauge (`loan=` DIAG) is the mechanism witness that it
stays small. A loss-inflated reservation (rate_i·T_return·(1+p_j·R/…)) is
derivable but adds a term the batteries have not asked for — not built.
(2) The law prices the lender's intake at rate_i; a placement burst can
transiently exceed it (softmax quantization) — the clamp to
[0, cap_i − out_i] keeps even that case inside the aggregate law. (3) At
c8 the lendable slack (~ the slow runway term, low hundreds of symbols)
is small against the pooled arm's N×knee budget: bounded borrowing repays
the tax the ACCOUNTS charge, it does not reproduce the pool's unbounded
depth. Whether that suffices to beat PBS at c8 is exactly what the
battery must decide — if it does not, the pooled path-scaled law is
vindicated as the c8 answer and percap remains a symmetric-cell tool.

#### 16.22.5 Measured outcome (2026-07-19, L1 battery, commit 477ab32): the law is gauge-perfect and limit (3) decides — the pool is vindicated at c8

The full battery (goal-gate "Per-Path Outstanding Accounting" →
BORROWING RESULTS: 8 reps × seeds 42+7 × interleaved arms × {sc2, sc3,
c7, c8}, same binary, 279 result-bearing runs, 0 liveness mismatches, 0
DNF) measured every clause of the derivation:

- **§16.22.3(c), exact**: c7 loans were IDENTICALLY zero — `loan=0/0` at
  every DIAG tick of every borrowing rep, both seeds — and the borrowing
  arm tied its no-borrow control (141.7 vs 141.9 s42; 136.5 vs 143.2 s7,
  ≈1σ), both at 0.90/0.89 of Σ, above pooled PBS both seeds. The
  symmetric cell is preserved by theorem, and measured so.
- **§16.22.3(b), exact**: every nonzero loan gauge at c8 has the slow
  path lending and the fast path borrowing (plain: cumulative 34–916 per
  run, ~100–250 active; Copa: 1747–3318, 65–638 active); the parking
  direction NEVER occurred; every loan repaid to zero on ack.
- **§16.22.4 limit (3), decisive**: c8 borrowing vs no-borrow is
  statistically neutral under plain+BBR (−3.7/+5.8, sign-flipping, ≪
  joint σ) and BOTH trail pooled PBS on both seeds (0.56–0.62 vs
  0.72×Σ). The honestly-lendable slack is an order of magnitude below
  the pooled arm's effective depth. One suggestive counter-datum: under
  Copa's honest cwnd caps the borrowing arm erased the prior sessions'
  −8.5…−11 Copa-percap isolation tax (C1P-B ≈ C1, heavy loan traffic) —
  cross-session and uncontrolled, and C1 itself trails PBS, so it cannot
  move the verdict.

**Conclusion.** The no-borrowing tax was real, but repaying it within a
lender-solvent law recovers only the slow path's HONEST slack — and the
pooled law's c8 advantage is not that slack: it is the pool's willingness
to let the fast path run past every honest per-path bound. That depth
cannot be granted by any per-path derivation that keeps the lender
solvent; it can only be granted by not having per-path bounds. So the
design space is now closed at both ends by measurement: **per-path
accounts (± bounded borrowing) own the symmetric cell (0.89–0.94×Σ, no
collapse mode, Copa +13–21); the pooled path-scaled pool owns the
asymmetric cell (0.72×Σ this session)**; `RWM_STORE_PERCAP` /
`RWM_STORE_BORROW` stay default OFF. Residual (iii) is attributed and
half-fixed in the same battery (the flight-witness attribution law in
§16.21's sampler: spurious cross-path-retransmit acks — 57–76% of
1057–1857 cross-path attributions per c8 run — no longer advance the
retransmit path's delivered counter; slow BtlBw p50 ×2.3 → ×1.4 over
truth; the p90 tail channel (iii-b) and the recurring honest-anchor
throughput circularity at c8-plain are the named remainders).

---

### 16.23 Engine parallelization: the third threading refutation, and the walls that are actually there (2026-07-19, `feat/engine-parallel`)

§16.19 refuted receiver parallelization below ~150 Mbit per sink and
predicted it would become the lever above that threshold; the per-path
accounting arc (§16.21–16.22) then moved the best symmetric operating
point to 137–147 Mbit/s — at the threshold. This section is the
profile-first test of that prediction, run at the best c7 arm (percap +
guard + honest caps + witness, plain+BBR), and its result is a third
refutation that finally names the wall the threshold estimate was
standing in for.

**The threading null, measured harder than §16.19.** At 132–144 Mbit/s
aggregate the two processes pinned to ONE core each (taskset, whole
process) sustain the full operating point on both seeds (pinned mean
136.3, n=8; unpinned 136.2, n=10) — and the server-pinned arm posts the
session's fastest run (143.9). Pinning removes ~40% of the measured CPU
at equal throughput: the unpinned 1.34/1.59 cores per side are
one-third scheduler-migration overhead, not parallel work. The flat
profiles reproduce §16.19's shape at +35 Mbit: top symbol 5–8%
(estimator/control math ~14–18% in aggregate), no stage to parallelize.

**The new instrument.** A receiver-side gauge (`RWM_RDIAG`) samples the
engine task's busy fraction (1 − time awaiting its select) and the
inbound message-queue depth — the direct discriminator between "the
single engine task is the service-rate wall" (busy → 100%, queue deep)
and "the wall is upstream" (busy low, queue empty). At c7 the engine
runs 81–87% busy with the queue near-EMPTY (avg 14–32 of 4096): it
drains everything the wire delivers, with headroom. Its service wall,
measured where it is actually approached (dual-c1 aggregate), is
~20–22k msgs/s; the sender's emission loop saturates first, at
~19.5–20k sym/s (single-c1: emission loop ≈ 1 core, wire and kernel
idle — system-wide 2.6 of 6 cores, zero UDP drops). These two
service-time walls (~45–50 µs/symbol of store/placement/serialize/
send and deserialize/estimator/frontier/ack work respectively) bracket
§16.19's "engine sink ceiling 187.7" exactly — and explain why neither
AES-NI (§16.19) nor core count (here) ever moved it: τ per symbol is
not reducible by more threads while the pipeline is one task deep on
each side.

**The actual c7 binder: the wire is full of recovery-plane waste.** At
c7 the emission integral is ~1.34× source (retx 14.2–14.7% of source
vs 7.5–9.1% for the SAME configuration single-path; proactive/reactive
repair 15.5–19.2% vs 7.7–9.3%) — ~190 Mbit/s emitted on a 2×100 wire.
The extra multipath waste (~16 pp of source ≈ 12–13% of the saturated
wire) equals the measured Σ-gap (c7 = 0.85–0.86×Σ this session). The
controlled proof is dual-c1: at GE 0.1% there is nothing real to
recover, yet the dual arm retransmits 9.1–9.3% of source (single-c1:
0.2%) and aggregates BELOW one path alone (174–176 vs 180–184) — a
spurious cross-path recovery flood: per-path sequence gaps created by
striping and inter-path skew are read as holes by the SACK-gap /
hole-refresh / tail-sweep machinery, which re-pulls symbols whose
originals or repairs are still in flight on the other path. This is
the same spurious-retransmit class §16.22's flight witness caught at
the attribution layer, now measured at the emission layer, and it is
the fourth consecutive multipath wall that is control-plane, not
compute (substrate CC → PMTU → pool law → recovery over-emission).

**Disposition.** `RWM_ENGINE_PAR` was not built — a parallel engine
would have measured session drift (the §16.14 lesson); the probe ships
default-off. The parallelization threshold is re-stated with measured
units: it is not a throughput ("~150–190 Mbit") but a symbol rate —
~19.5–20k sym/s per sender process, ~20–22k msgs/s per receiver
process — reachable only by c1-class wire aggregates. The named
successor for the remaining c7 gap (and the dual-c1 anti-scaling) is
multipath-aware recovery suppression: cross-path in-flight awareness
in the hole-refresh/tail-sweep engine, i.e. do not re-pull a sequence
whose latest transmission (any path) is younger than the current
inter-path skew plus that path's RTprop — the emission-layer sibling
of §16.22's flight witness. Per the measurement discipline it is
named, not built, and gates on its own interleaved battery. **[Built
and measured: §16.24 — the attribution is REVISED there: the waste was
real and is killed, but it was only partially causal for the Σ-gap.]**

---

### 16.24 Multipath recovery suppression: the fifth wall, and per-path loss detection as its law (2026-07-21, `feat/recovery-suppression`)

§16.23 closed with the c7 Σ-gap's measured owner — multipath
recovery-plane over-emission (retransmit share ×1.8, repair share
×2.2–2.5 versus the same configuration run single-path; the dual-c1
control retransmitting 9.3% of source at a 0.1%-loss cell where a
single path retransmits 0.2%) — and named its successor lever:
cross-path in-flight awareness for the hole-refresh engine. This
section builds that lever, and the per-NACK trace it starts from
sharpens the diagnosis into a form worth stating generally: **the
over-emission was two instances of one mistake — recovery clocks and
loss serials kept GLOBAL where a multipath transport needs them
PER-PATH.**

**The trace.** Instrumenting every targeted retransmit with its
flight's age against its own path's smoothed-RTT clock shows 82% of
c7's retransmits fire while the sequence's live flight is still inside
its path's expected-arrival window (mean age at fire 45 ms): the
legacy hole gate — age ≥ max-path-SRTT/2 since the ORIGINAL send,
never reset by a retransmit — reads gaps the SCHEDULER created
(striping + inter-path skew) as holes, and re-fires them every
cooldown while copies still fly. The delivery side corroborates
independently: §16.22's flight witness credits the ORIGINAL flight for
82% of cross-path-history attributions (the ack arrives sooner after
the retransmit than that path's RTprop — the retransmitted copy cannot
have completed the round trip). Meanwhile the per-path loss estimators
read 0.62–0.77 at the 0.1%-loss dual-c1 cell: the batch serial is a
global counter, but per-path loss is estimated from serial GAPS, so
under striping every path switch counts the other path's run as loss —
poisoning the proactive repair budget (the ×2.2–2.5 repair share), the
P_lost retransmit branch, the NACK budgets, and the per-batch
in-flight release, all at once.

**The law.** RFC 9002's loss detection, generalized per path — both
channels, no new constants. *Time threshold* (§6.1.2, the safety
net): a reported gap sequence is a candidate hole only once its LIVE
flight — the last (re)send, which now inherits the in-flight state, so
a retransmit is clocked on its own path — is older than 9/8 × its
path's smoothed RTT (kTimeThreshold; granularity floor = the existing
per-seq cooldown floor). *Packet threshold* (§6.1.1, the fast
channel): the original flight on path j is declared lost as soon as ≥3
later path-j symbols are known delivered — same-path FIFO evidence
that a scheduler-created cross-path gap can never produce (its
same-path successors are exactly as un-arrived as it is), and that
real same-path losses produce within one skew rather than one RTT.
The cross-path packet-threshold is deliberately NOT used: cross-path
sequence gaps are precisely RFC 4737's reordering caveat, the problem
multipath QUIC solves with per-path packet-number spaces. Suppression
is the law's only power — the receiver's hole-refresh keeps
re-advertising until a channel fires, so reliability is untouched
(loopback and battery: dnf = 0 everywhere). A first build with the
time threshold alone measured the cautionary tale: waste fell exactly
as designed and throughput FELL with it (c7 139→134, dual-c1 181→142)
— on a frontier-serialized retention store, recovery latency buys back
every megabit the waste had cost. Loss detection needs its fast
channel.

**What it measures.** Both target cells clear their waste: c7's
retransmit share falls 14.9% → 4.5% of source (BELOW the single-path
8.2%) and its repair share 0.185 → 0.059, for +5.3/+6.4 Mbit
(s42 Δ ≫ σ_s; s7 consistent) to 0.88–0.89×Σ; dual-c1 — the controlled cell with
nothing real to recover — falls from a 27k-retransmit flood (8.5% of
source) to 0.7%, its bimodal collapse mode damps (σ 15.4 → 6.9), and
the dual aggregate lands ABOVE the same-session single (192.3 vs
186.0 at seed 42, σ halved; 193.2 vs 181.0 at seed 7, Δ = +24.2 ≫ σ_s
against an all-flood baseline): the anti-scaling §16.23 measured is eliminated.
The bimodality itself is explained en passant: the poisoned loss
estimate (~0.6) collapses the ADR-0046 congestion multiplier and
thereby *accidentally suppresses* the flood in some runs — the runs
where it engaged early were §16.23's fast dual-c1 runs.

**What it refutes, honestly.** The freed wire does not convert 1:1
into goodput: with c7's emission now ~155 of 200 Mbit — the wire no
longer full — goodput stops at 0.89×Σ. §16.23's "the over-emission
occupies ≈ exactly the Σ-gap" is therefore revised: co-located, only
partially causal. The residual owner is frontier-recovery latency on
the ack-serialized retention store (a real hole still freezes the
cumulative frontier for at least a skew plus a report round), which
composes with the SACK-clocked store-release machinery as the next
lever. And the serial fix — per-path batch namespaces, the
multipath-QUIC-shaped repair for the poisoned estimators — is
diagnostically vindicated but operationally REFUTED for now: honest
(small) RTT and honest (small) loss re-heat every SRTT- and
loss-scaled recovery cadence that the poisoned values were
accidentally damping (hole-refresh clamp 25 ms instead of 100 ms,
cooldowns at their floors, no congestion damping of the legacy flood):
dual-c1 181→134 with sender CPU ×2.4. It ships default-off inside the
umbrella; re-deriving the cadences under honest per-path signals is
the named follow-up. `RWM_RECOV_MP` itself stays default-off — c7 and
the control sweep clean on both seeds, but c8 is null (its binder
remains §16.22's pool story) and the N=1 sc3 identity carries a
consistent-signed sub-σ cost — the flip gates on the composed
frontier-release battery.

### 16.25 SACK-clocked store release: the sixth wall's lever, and the first default-flipped mechanism (2026-07-21, `feat/bbr-default-and-store-release`, `RWM_STORE_SACK_RELEASE`, DEFAULT ON)

§16.24 ended with a measured hand-off: with the recovery waste
suppressed, the c7 wire is no longer full yet goodput stops at
0.88–0.89×Σ — the residual owner named as frontier-recovery latency on
the ack-serialized retention store. The mechanism, pre-registered in
the ledger before the build (goal-gate "SACK-Clocked Store Release",
MEASUREMENT DISCIPLINE item 11): the store releases slots only on the
cumulative frontier (`split_off(ack+1)` — the retention contract), so a
SACKed-but-not-cumulative symbol holds its flow-control slot for a full
frontier round, and the store recycles at frontier latency instead of
path rate.

**The law.** On a SACK range the sender UNCOUNTS every retained seq
from the outstanding gate (a released-mark set; `outstanding =
retained − released` at the single site every store gate reads, so the
path-scaled pool composes with zero extra code) — and touches NOTHING
else. The payload stays in `sent_store` (the only copy — the
retransmit buffer is metadata), every ARQ map and `RWM_RECOV_MP`
per-flight clock survives, and the marks prune on the same cumulative
split_off twin (subset invariant). This differs BY CONSTRUCTION from
the refuted `RWM_SACK_PRUNE` (2026-07-07, UNSAFE: pruning the store on
SACK destroyed the only copy of a received-then-evicted symbol —
in-order duals wedged): under the release law a receiver eviction
costs a wasted retransmit, never a wedge, and the sender's race-ahead
is bounded because never-SACKed seqs still count against the cap. The
2026-07-07 null ("sender-side decoupling lifts nothing — the sender
was never the bottleneck") is era-resolved rather than contradicted:
that verdict was measured single-path under the Cubic substrate with
walls 1–8 still standing; on the post-wall substrate the sender store
IS the binder at the striped cell, which is exactly what the
pre-registration predicted and the battery confirmed.

**Measured (both seeds, 8 reps interleaved, same-session Σ, dnf=0 on
all 200 completed runs; goal-gate section for full tables).** On the
best-c7 arm (plain + BBR-default + path-scaled pool): c7 SR-only
142.9→154.8 (0.959×Σ, s42) / 141.8→152.3 (0.934×Σ, s7), Δ ≫ σ_s both
seeds; composed with `RWM_RECOV_MP`: **168.7 ± 0.85 = 1.045×Σ (s42)
and 165.9 ± 2.0 = 1.018×Σ (s7)** — above the base singles' Σ, 0.98–0.99
of the SR-arm's own Σ. The dwell gauges show the mechanism, not just
the effect: mean counted occupancy at c7 falls 3,157→~1,460 (cap
4,096) with ~167k slots released per 200 MB, while retransmits FALL
(21.6k→17.2k SR-only; 5.2k composed) — releasing the window did not
buy throughput with waste. The N=1 term is real and positive: sc2
+4.31/+2.93 ≫ σ (a single-path SACK above a hole also holds a slot),
sc3 +0.66/+0.31. Dual-c1 composed: 204.2/208.2 vs 181.9/187.8, retx
×11–12 down, above the same-session single on both seeds. c8 stays
inside its documented bimodality (composed arm best mean both seeds,
no Δ≫σ claim; the asymmetric binder remains §16.22's pool story).

**Flip.** The pre-registered gate was met on both seeds and
`RWM_STORE_SACK_RELEASE` ships DEFAULT ON — the first mechanism gate
flipped under the item-11 discipline (prediction written before the
build, battery confirming it, falsification path never triggered).
`=0` restores the frontier-only release; `RWM_SACK_PRUNE=1` (kept to
reproduce the refutation) takes precedence with a warning. Composed
follow-ups named, not built: the `RWM_RECOV_MP` + pool flips ride the
consolidation battery (roadmap item 2), where the composed stack
measured here (1.02–1.05×Σ at c7) is the candidate default.

---

### 16.26 δ-honest overload shedding: the δ-continuum completed, and the unified machine ships (2026-07-21, `feat/unified-shedding`, `RWM_UNIFIED` DEFAULT ON)

§16.20.8 ended with the δ-contract stated operationally: at small δ,
overload must be shed, not serialized — the property the streaming
machine had and the RLC realtime parameterization lacked. This section
derives that property from the (δ, ρ, r) triangle instead of hard-coding
it, and reports the pre-registered flip battery that made the unified
machine the shipped default.

**The shed law.** The streaming machine's shedding was always the
δ-price — a reorder horizon past which a hole is abandoned — but its
horizon was a constant and its loss unbounded. The unified machine
already carries everything needed to make both ends honest:

```
   D(δ)  = min(b(hint)·RTprop, 2·RTprop)     the span law's own deadline
                                             (b = ½/1/2; §16.20.3's H)
   1−ρ   = ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))
                                             the §8.1 residual the design
                                             already concedes past
                                             in-window FEC at the live
                                             operating point
```

A symbol is SHEDDABLE iff its projected delivery exceeds D(δ) — a
retransmit fired at age > D lands after the receiver's own δ-horizon
give-up (send + owd + D), pure waste that serializes the stream — AND
cumulative shed stays within the 1−ρ budget. Beyond the budget the
machine SERIALIZES: ρ wins over δ, the completeness contract survives
overload. Sender arm: past-deadline holes leave the ARQ set at the
recovery decision points (P_lost branch, SACK-gap service, tail sweep),
budget-bounded, refused candidates counted and served. Receiver arm: the
in-order EVICT hold becomes the δ dial b·SRTT (replacing the bulk-shaped
4×SRTT ∈ [60, 300] ms clamp) while holes-given-up ≤ ε̂_recv × frontier
(the loss-class bound — give-up is intrinsically holes-only), reverting
to the legacy hold when spent. The ρ = 1 RETAIN contract is excluded BY
CONSTRUCTION (the law compiles out on the reliable path). No new
constants; every threshold is a measured anchor or an already-derived
parameter. Composition: the unified machine ships with its repaired A\*
anchor (§16.21 fix A, `RWM_ASTAR_ANCHOR` default ON under the umbrella)
— a shed law in front of width-1 spans would shed what inert FEC failed
to recover; the anchor is what makes the budget small.

**Measured (goal-gate "Unified Shedding + Flip Battery"; L0 dev box +
L1 VM, seeds 42+7, pre-registered, all five predictions confirmed, no
falsification clause triggered).** At L0 the collapse signature (p50 in
seconds) is 0/14 in every arm at the attribution cell (base: 3/14), the
unified+shed arm posts the best p90 of all four machines (62–78 ms vs
79–96), and the gauges show the law's ρ-conservatism directly: the
sender budget reads ~0.000–0.002 and REFUSES 52–773 past-deadline
candidates per rep while shedding 0–25; the receiver gives up ≤ 2.5% of
frontier and closes its budget. Losses: unified 0.25% of messages
(excluding one 7-s whole-process-freeze rep whose losses the gauges
attribute to the environment) vs streaming's 1.0% — the machine sheds
LESS than streaming while beating its tails, because in-window FEC now
recovers what streaming abandons. At L1: zero collapse reps in 96
completed tail reps; unified+shed p99 medians ≤ streaming at every
cell-size-seed row (c2 37/40 vs 40–43/52; c3 101–111 vs 108–133) and ≥
legacy-RLC within the noise floor; 100% delivered at the c3 perf cell
(streaming: 79/81%) at completer parity (0.12 vs 0.10 s — the #61 ×3–4
price dissolved by the live anchor); cod/src 0.38–0.50 consumed — **r\*
is finally realized at the realtime wire (the §8.4.1 chain's last link)
and it buys the measured 100%**; bulk gen-sys parity within σ; the
§16.17/§16.20 depth knee engaged in both machines with unified
at-or-ahead (the first fully-live L1 look, closing the §16.20.7 residual
on the wire side).

**The flip.** `RWM_UNIFIED` ships DEFAULT ON (2026-07-21). The three
receive machines of §16.20 are now one shipped machine parameterized by
(δ, ρ, r): Realtime is the small-δ point (EVICT + δ-derived hold +
budget-bounded shedding), bulk the large-δ/ρ=1 limit (RETAIN, no
shedding, M\* depth), with the span law continuous between them.
`RWM_UNIFIED=0` is the legacy three-machine opt-out arm. The streaming
two-layer code is NOT removed: it enters the deprecation register behind
a re-test clause — the 12–48× crown record spans historic cells this
battery did not re-run, and retirement requires a later pass to hold
that record cell-by-cell on the unified default. Named residuals: the
sender budget uses the EVENTUAL P_fec (the within-deadline form would be
smaller ⇒ a larger honest budget — the principled refinement if a cell
ever shows the receiver arm carrying too much); multi-second
whole-process freezes overwhelm every machine including streaming (an
environment class, not a machine class — the §16.20.8 confirmation
protocol stays open); the c7 unified arm's −5 Mbit/0.6σ direction under
gen-sys duals stays on the §17.6 watch list.

### 16.27 Copa-sole on the clean substrate: the mode switch is a measured tradeoff, not a wish (2026-07-22, `feat/copa-sole-clean`)

The one open question the consolidation left on the CC surface (§17.2):
does the fixed substrate — SACK-clocked store release (§16.25), per-path
recovery suppression (§16.24), the path-scaled pool (wall #7), anchor
hygiene (§16.21), all now default ON — close Copa-sole's §12.11/#82 bulk
gap, letting the two-value `RWM_QUIC_CC` surface collapse to ONE
δ-parameterized controller (the vision's one-continuous-mechanism axiom)?
Pre-registered prediction (goal-gate "Copa-Sole on Clean Substrate",
committed before the battery per the discipline's item 11): the walls
throttled exactly the full-pipe regime where Copa trailed, so Copa should
reach ~parity while keeping its queue/tail advantage. **Measured:
FALSIFIED.** On the consolidated stack, arms A = BBR-under (default) vs
B = passthrough+Copa, same binary interleaved, seeds 42+7 ×8:
copa/bbr = 0.89× sc2, 0.97× sc3, 0.73× c7, 0.57× c8, 0.66× dc1, every gap
but sc3 ≫σ and reproduced to three digits across seeds. The walls
WIDENED the gap: they lifted BBR-under's aggregation (c7 ~100→166, c8
~54→82 vs the #82 broken-substrate numbers) while Copa's δ-equilibrium
caps cwnd near BDP + 1/δ regardless of freed pipe — it leaves the
capacity on the table (that tight queue is its design), so BBR eats the
unlock and Copa does not. The §12.11 "C8 domination" (0.95–1.01×) is
superseded as a broken-substrate artifact (it had suppressed BBR); on the
fixed substrate BBR leads C8 (0.57×). What Copa KEEPS, re-confirmed on
this substrate (not assumed): the NETWORK standing queue ×18/×16/×6–7
tighter at sc2/sc3/c7 (wireQ 5/30/7 ms vs 89/487/50), and tail PARITY at
the realtime c2 message cell (p99 medians tie BBR arm-for-arm on the
shipped unified machine, both seeds). δ(hint) = 0.5/ζ live-verified
(Bulk 0.005 / Auto 0.5 / Realtime 50). **No default flip**: `RWM_QUIC_CC`
stays BBR-under, the surface stays two-valued as a MEASURED queue/tail-vs-
bulk TRADEOFF, and the CC endgame (one controller across the surface)
moves to the fusion (§17.6 item 10 / ADR-0068), which inherits this bulk
gap as its target — and gains a sharpened rationale: the very mechanism
Copa lacks, a BBR-style measured rate model as feed-forward baseline, is
what would let a δ-priced controller convert the freed pipe.

### 16.28 Emission batching: the c1 lever profiled, built sender-only, and honestly bounded (2026-07-27, `feat/emission-batching`, `RWM_EMIT_BATCH` DEFAULT OFF)

The §17.9 c1 loss (×5.5 vs quinn-BBR's 915 Mbit/s of userspace QUIC on
the same box) named "emission batching/GSO" as its lever. The
profile-first pass corrected the mechanism before building: **syscall
density was NOT the wall** — quinn-udp's GSO transmit path was already
engaged under the engine (≈7.6 segments/sendmsg; quinn-perf itself runs
≈10.5 at 922 Mbit/s — the same order per byte), and AEAD is 1.7% noise.
The sender's core-second goes to PER-SYMBOL control machinery: the
taper/span derivation (repair rate, A*/Δ, shed budget — recomputed per
source symbol, ≈15–17%/core with its exp/log), per-ack estimator math
(the receiver acks every in-order symbol, ~20k WindowAcks/s), and one
full select!-loop iteration per symbol (τ ≈ 45–50 µs/sym total).

Built (`RWM_EMIT_BATCH`, default OFF; burst `RWM_EMIT_BURST` = 64
symbols ≈ the 64 KB pacer quantum): pacer-quantum burst TUN intake
inside the flow-control/pacing contracts (checked per symbol) +
per-burst taper/span refresh (50 ms staleness bound; the A* anchor
stays fed per symbol). Scope is measurement-carved: **single-live-path
only** (dual-path bursting amplified the wall-#8 striping-gap loss
misread — per-path ε̂ read 0.74 at a 2.6% cell, c7 167→115 — and was
scoped out; dual cells then measure null, the built-in control) and
**Realtime excluded** (the per-packet latency path never trades a
wakeup for a burst). Three receiver-side variants (engine-loop burst
drain ± per-burst ack coalescing) were built and REFUTED — any drain
between arrival and ack emission inflates the echo RTT (11→76 ms), the
echo-RTT-derived store cap grows on it, and the tail-sweep/hole-refresh
machinery floods spurious retransmits (c1 collapsed to 136–144); the
code is removed, the mechanism recorded.

Measured (both seeds, ×8 interleaved, dnf 0): **c1 +10–16% with
disjoint per-run ranges** (def 186.2±9.8 / 190.8±2.6 → eb 216.2±10.7 /
210.5±4.7) at **−24–27% sender CPU/bit** (1.10 → 0.94 cores); sc2
−15–20% sender CPU at equal (wire-bound) goodput; c7/c8 null; tail
crown unregressed (structurally inert in tunnels + measured parity;
1000/1000 every rep). Syscall density unchanged — the win is CPU, as
the corrected profile predicted.

**No flip** (the pre-registered c1 ≥ 400 gate failed): the gate ships
as a measured opt-in. The honest ceiling after this branch: sender
~24k sym/s/core batched (was ~19.5–20k); **the engine-receiver
per-message service wall (~22–23k msgs/s ≈ 210–230 Mbit/s per sink,
receiver pinned at ~1.1 cores in the batched arm) is the measured
system ceiling** — the named successor, with the refuted drain family
bounding its solution space: reduce per-message/per-ack work or ack
density at the protocol level, never by queueing in front of the ack
clock. Goal-gate "Emission Batching" carries the full tables.

### 16.29 The c8-aware pool law: capacity-weighted sizing measured, the span story sharpened, and the real binder renamed (2026-07-27, `feat/c8-pool-law`, `RWM_STORE_CAPW` DEFAULT OFF)

The wall-#7 coda's owed follow-up (§17.7 residual (i), ADR-0058): under
SACK-clocked release the LEGACY 1024 pool reads better at c8 than the
shipped path-scaled N×2048 pool (0.85–0.87×Σ vs 0.72–0.76), and kernel
MPTCP-BBR prices the cell externally at 89.7–92.6. Pre-registered
derivation (goal-gate "C8-Aware Pool Law", written before any run): the
count-scaled pool over-weights the slow path, so the fix is the
CAPACITY-WEIGHTED shared pool — pool = clamp(Σ_i cap_i, floor, N·knee),
cap_i = the honest per-path law rate_i·(K_i·RTprop_i + (gain−1)·(R +
RTprop_i)) — each path earns unacked-frontier depth for its own pipe +
recovery round, summed as ONE pool (ADR-0058's pooled-borrowing verdict
kept). Predicted ≥ max(legacy, path-scaled) at c7 AND c8.

**Diagnosis first (the new DIAG-only per-path store-attribution gauge):
the hypothesis's villain was wrong.** The slow path never holds the
depth — in EVERY pool arm it holds ≤~10% of the outstanding. Under the
shipped pool (latched at 4096 by the ×5–9 legacy anchor over-read) it is
the FAST path that parks up to ~3,950 un-SACKed slots, echo RTT inflates
to 279–452 ms over a 9–13 ms RTprop, and goodput runs stall-then-burst
(16–33 Mbit while the span fills, 170–234 at release) — the c8
bimodality, mechanism now on gauges. Legacy-1024 wins by bounding that
span at ~the fast path's own pipe+runway — and starving the slow path
entirely (its outstanding decays to ≈0; legacy c8 = the fast single +
only 2.6–2.7 Mbit).

**Battery (seeds 42+7 ×8 interleaved, same binary, echo-asserted,
same-session Σ):** the law engaged exactly as derived (pool live at
1.3–2.5k on honest anchors, between the incumbents) and the prediction
FAILED both cells: c8 capw 79.1/74.2 vs legacy 86.7/87.9 (0.79/0.74 vs
0.866/0.868×Σ, consistent both seeds); c7 capw 143.9/143.8 vs pbs
166.3/166.2 — where the attribution top-up showed the c7 cost is owned
by the `RWM_PLAIN_RS` composition the law rides (rs-without-capw =
139.4/142.8, cap non-binding), pricing a NEW datum: the RS witness cost
scales from −1.2…−1.8 at N=1 to −22…−27 ≫σ at the symmetric dual — the
"carry RS as a full stack member" candidate is refuted at c7. **Verdict
per the pre-registered falsification clause: the c8 binder is NOT pool
sizing.** Σ-shaped pools of any honesty buy fast-path frontier-span
exposure with no slow-path payback, because the slow path CONVERTS
almost nothing (the data supports pool ≈ max_i cap_i ≈ 1024–1250 — which
legacy latches at by accident and which can only formalize the
0.85–0.87×Σ it already measures). The true c8 binder, renamed with
numbers: **slow-path conversion** — placement + recovery leave the c3
path's ~16 Mbit unbanked while kernel MPTCP-BBR banks it (89.7–92.6 vs
rp-legacy 86.7–87.9 = fast + 2.7). No flip: `RWM_STORE_CAPW` and
`RWM_PLAIN_RS` stay OFF, the shipped default is unchanged, and the next
pre-registerable item at this cell is a conversion mechanism (or the
mechanical per-topology gate to the legacy span law, worth the measured
+11–14), not pool arithmetic. Piggybacked on the same battery: the
DEPRECATION REGISTER's owed `RWM_FMTCP` re-test on the clean substrate —
c7 ×0.11, c8 ×0.20 of the same-session stack, strictly worse everywhere
≫σ, cod_share > 1 — CONFIRMED-REFUTED, never wall-tainted, cleared for
deletion (register row updated; goal-gate "C8-Aware Pool Law").

### 16.30 The lossy-single residual: a CLOSED accounting of the −9…−14% vs BBR-class, and the reactive plane's queue-sustained over-fire (2026-07-27, `diag/lossy-residual`, `RWM_RECOV_SP` DEFAULT OFF)

The §17.9 c2/c3 losses (rp 78.6–78.7 vs quinn-BBR 91.9–92.4; 16.1 vs
18.6) diagnosed to closure with wire-truth instruments (qdisc byte/pkt/
drop counters + DIAG cumulative src/cod/ack + emission/arrival gap
gauges; goal-gate "Lossy-Single Residual"). The gap is NOT idle wire and
NOT engine service: at both cells the wire runs ≥98% utilized (98.4/100
and 19.87/20 Mbit) with the receiver at 8–45% busy. It decomposes, and
the terms SUM to the measured gap at both cells:

- **Framing/MTU tax (structural, the largest c2 term): ~4.3 Mbit at c2,
  ~0.95 at c3.** rp carries 1200 payload bytes per ~1319 wire bytes
  (the 1350 MTU floor, one symbol per packet: efficiency 0.910) vs
  quinn's MTUD-sized ~0.957.
- **Reactive-plane over-fire: ~2.7 at c2, ~1.7 at c3.** The sender
  retransmits ×5.0–5.7 the realized loss (3313 fired vs ~580 drops at
  sc2-100M; 2556 vs ~510 at sc3). Root: (i) the RFC 9002 hole law
  (§16.24) is `N>1`-gated — inert at singles; (ii) the legacy plain
  anchor still over-reads ×5.7–10 at singles, latching the dynamic
  store cap at 1024 (4–13×BDP) whose standing queue (echo RTT 110 ms at
  c2, 530 ms at c3 vs RTprop 13/45) keeps every hole's recovery
  crossing 100–500 ms of queue while the receiver re-advertises it
  each [25,100] ms sweep.
- **Proactive FEC is DEAD at singles** (the honest surprise): [SPAN]
  rr = 0.000 everywhere — the sender estimator reads pl ≈ 0 at
  2.5–4.8% cells, so r* = 0 and ALL overhead is reactive. The #46-era
  "taper overspend" concern is refuted in the opposite direction.
- **Object-scale ramp** (the 25 MB bar geometry vs steady): +1.7 this
  session (historic up to +6) at c2.

The pre-registered fix (`RWM_RECOV_SP`: the same 9/8× time-threshold
hole law applied at N = 1, time channel only, suppression-only) is
mechanically LIVE (young-fires → 0, supp_law 12–18k) but banks only
**+0.32/+0.35 Mbit ≫σ at sc3 (both seeds) and a tie at sc2** — fired
drops just 24–31%, because the y-class was a QUEUE-SUSTAINED RE-FIRE
LOOP, not one-shot spuriousness: while a hole's recovery crosses the
store-cap queue it keeps legitimately re-ripening. Predictions failed
per pre-registration ⇒ NO FLIP (ships default OFF, the only measured
≫σ singles lever this session). The named successor levers: **MTU/
payload scaling** (up to +4/+1), **window/inflight decoupling** (the
1024-latch is at once the re-fire queue AND the only stall insurance —
an honest-sized static window idles the wire 12% and LOSES 1.3 Mbit at
sc3), and estimator honesty for a live proactive plane. Realtime tails
untouched (gate OFF; shipped path byte-identical).
### 16.31 The streaming crown re-test: the historic record held cell-by-cell, and the second machine cleared for retirement (2026-07-27, `meas/streaming-retirement`, measurement only)

The deprecation register's streaming clause (§16.26's honest reservation:
the 12–48× message-tail crown spans historic cells — the L2/L3
message-tail batteries and quinn-vs-rp Metric A — that the flip battery
did not re-run) was discharged by a dedicated pre-registered battery: the
historic crown cells reproduced era-honestly on today's substrate
(Metric-A tail_matrix c2/c3 × 400/1200 B at 50 msg/s × 20 s, plus the
L2-era 30-s stream_bench shape with its p99.9 metric, plus a bulk-hint
inertness spot), `RWM_UNIFIED=0` (the streaming two-layer machine, echo-
verified) vs the shipped unified default, interleaved per rep, both
seeds, ×8 (crown) / ×5 (L2 shape) — with the pre-divide historic
absolutes explicitly NOT the bar (hardware divide + substrate walls);
the comparison is same-day arm-vs-arm, the external crown having already
been re-verified on the unified default by the §17.9 competitive
baseline. Result: the unified machine matched-or-beat streaming's p99
medians at all five cells on both seeds (10/10 cell-seeds, −1.2…−26.8 ms,
largest at c3), p50 equal-class, delivery identical-complete (163/163
reps full delivery — at these cells the tail, not delivery, separates
the machines), bulk-hint inert. One datum recorded at full strength
because it is the only direction the old machine still shows: at the
30-s shape the streaming p99.9 MEDIANS are lower by 6.7/12.2 ms on both
seeds — deep inside the rep spread, with the sign reversed at the worst
rep (streaming owns the battery's only >200 ms excursion, 335 vs 129 ms)
— a sub-noise residual signature of the diagonal layer, named the cell-5
p999 WATCH, not a gate. Disposition: the register row is RE-TESTED/
CLEARED and streaming deletion (~1,230 LOC: adapter + `streaming-codes`
crate + selection glue) is GO for the next consolidation pass — scoped
to streaming only, since the legacy RLC decoders' separate retirement
clause (unified ≥ legacy everywhere) was not re-argued here. The
architectural claim completes its arc: the one-machine default now holds
every cell of the record that justified keeping the second machine.

*Coda (consolidation pass 2, 2026-07-27): the deletion this section
cleared was executed — commit bccb32a removed the adapter, the
`streaming-codes` crate and the selection glue (−1,708 LOC), scoped
streaming-only. `RWM_UNIFIED=0` + Realtime now selects the legacy-RLC
windowed machine (the stated opt-out semantics change); the cell-5 p999
WATCH above is thereby historical — the record of the retired machine's
last measured edge. Identity + tail-crown smoke on the deletion binary:
sc2 84.1–85.6, c7 164.7–168.6, tail c2 p99 med 35/40 ms, 1000/1000 —
the shipped default untouched (goal-gate "Code Consolidation 2").*

### 16.32 C8 slow-path conversion: the question answered structurally — feeding the slow path source is negative-margin at this cell, under every placement law measured (2026-08-06, `feat/c8-conversion`)

The §16.29/"C8-Aware Pool Law" arc ended by naming SLOW-PATH CONVERSION —
not pool sizing — as the c8 binder (every pool law converges to
fast-single + ~2.6 of the slow path's ~16 Mbit Σ-share). This section
closes that question with a diagnosis-first instrumented answer.

**Diagnosis (per-path conversion gauges, DIAG-only: placement counts,
retransmits by original placement path, frontier-stall owner/resolver
attribution, receiver first-copy vs duplicate classification with
frontier lead).** The displacement hypothesis is REFUTED: ~90% of
slow-path arrivals are FIRST copies in every arm — slow deliveries
convert when they happen. Under the legacy-1024 pool the limiter is
PLACEMENT STARVATION (slow share 4–9% vs its ~16% capacity share: the
Bulk placement softmax's idle propagation term alone is worth e^10:1
odds against the slow path, and the pool gate pauses admission before
the fast path's queue term can ever spill placement); under the
path-scaled pool placement DOES reach capacity share and the cell nets
LESS — the lateness tax: 16–29% of slow-placed symbols are re-served
cross-path (vs ~4.8% realized loss), partly through a named defect (the
`RWM_RECOV_MP` hole law keys its path count and clocks on the
saturation-filtered `active_paths()`, so a cwnd-full path collapses the
law to its N=1 bypass mid-transfer — the same filter trap documented at
the store laws), and the slow queue is need-time-unbounded (echo RTT to
~560 ms over a 34–40 ms RTprop).

**The pre-registered fix and its honest fate.** A frontier-slack
placement law (`RWM_PLACE_SLACK`, default OFF): charge only the lateness
beyond the in-order frontier's need-time, cost_i = max(0, Ê_i − D_i)/ref
with D_i = min(span/R_ack, 9/8·srtt_i) — the second bound added after a
smoke falsification showed placement must never budget past the recovery
plane's patience (9/8 = RFC 9002's kTimeThreshold, the hole law's own
constant). The law is a strict continuous generalization (S = 0 is the
shipped cost bit-exactly; no mode, no threshold, no per-topology
branch), and it WORKED as a mechanism: placement reached capacity share
with the starvation gone. The battery refuted the PREDICTION both ways:
c8 never beat both incumbents, and c7 fails its protection clause ≫σ on
both seeds (0.858/0.896×Σ vs ≥0.97 required — flattening the short-term
queue differential costs the symmetric cell its per-symbol load
balancing). Register row; no tuning pass. The companion defect fix
(`RWM_RECOV_MP_LIVE`: hole-law N/clocks on `live_paths()`) is
gauge-proven — young fires collapse 412–749 → ~16, slow re-serving
26–29% → 4–8% — and lifts the path-scaled pool's c8 collapse floor
(+8–12 in 3 of 4 pairings, min run 49.5 → 73.3), but a 3/3-pairwise
dual-c1 regression (−11 mean) blocks its default flip; it ships OFF as a
measured A/B arm with the dc1 interaction as the named follow-up.

**The structural result (the table that closes the chapter).** Across
five placement arms spanning slow-source shares of 6–18%, c8 goodput is
MONOTONICALLY ANTI-CORRELATED with the slow path's source share on both
seeds: 6.2% → 88.6 (0.874×Σ); 11.2% → 88.4; 16–18% → 70–83 — and the
ordering survives killing the re-serving tax entirely. What conversion
banks, the in-order frontier pays back with interest: slow-owned
frontier stalls (24–39% of stall time on ≤18% of placements) and the
end-of-object drain tail (the last slow-queued symbols serialize
completion). **At a 5×-rate / 4×-RTT / 2×-loss asymmetry, an in-order
object transport should NOT feed the slow path source; its optimum is
fast-path source + slow-path recovery traffic ≈ fast single + 2–3
Mbit.** The external reference obeys the same law: kernel MPTCP-BBR
banks +3.1/−2.4 vs its own same-session single-path BBR at this cell.
The pre-registered ≥0.87×Σ line is held by the legacy-1024 arm
(0.874/0.871×Σ, both seeds), 1–4 Mbit under kernel MPTCP-BBR — and the
REMAINING distance to that bar is the single-path c2 gap (§16.30's
framing + reactive accounting), not multipath: closing c2 closes c8.
Pool-law re-settlement: no law wins both cells (legacy c8, path-scaled
c7 — the shipped default unchanged); the c8 WATCH stands with its
mechanism now fully named.
### 16.33 The adversarial cells: where each controller actually breaks (2026-08-06, `meas/adversarial-cells`, measurement only)

ADR-0068's fusion was gated on a prerequisite the clean rig could not
supply: MEASURE the predicted Copa breakage on the three link classes
where a rate model should structurally beat a delay law — delay-jitter
(aggregation class), shallow buffers, and policers. The battery
(goal-gate "Adversarial Cells (B1)": three new L1 cells, each
mechanism-validated before any transport run — jitter shows in ping
mdev 4.8/13/21 ms for J=5/15/25; the 8-packet buffer caps
RTT-under-overload at +0.4 ms where the deep control bloats +93 ms; the
policer drops 18.8% of a 120 Mbit overload with ZERO RTT inflation —
then shipped-BBR-under vs Copa-sole ×5–8 reps, seeds 42+7, interleaved,
pre-registered predictions and falsification conditions committed
first) returned a map that confirms the dose-response, refutes two of
the three breakage stories, and indicts the shipped default on one:

- **Jitter (20 ms base): the dose-response is real, the base collapse
  is not the delay law's.** Copa decays strictly monotonically with J
  (29.9→20.5 Mbit s42, every step ≫ σ) — delay noise does talk a
  delay-based law down, as predicted. But Copa is already at 0.38× BBR
  at ZERO jitter: the binder is the 1024-slot store's Little's-law
  dwell ceiling (~36 Mbit at the measured ~250–350 ms dwell) — Copa's
  empty pipe pays a full recovery round per GE hole at 40 ms RTprop,
  while BBR's ~60 ms standing queue hides repair latency. The gap is a
  CC×store interaction, invisible on every clean cell (sc3's 40 ms sat
  below the ceiling at 20 mbit).
- **Shallow buffer: INVERTED, ×7.7–7.9 for Copa.** The predicted Copa
  loss-conversion (1/δ = 200-packet target vs an 8-packet buffer) did
  not happen — Copa holds its full clean class (75–79 Mbit, 1.5%
  drops, 4 ms wireQ). The shipped BBRv1-class arm collapses instead
  (9.8–10.0 Mbit, 7.3% sustained drops): token-bucket dequeue
  quantizes delivery into line-rate microbursts that poison the
  max-filter rate anchor (measured btlbw ≈ 10× the link), sustaining
  probe overshoot into the tiny buffer — the documented BBRv1
  shallow-buffer pathology, reproduced. The cell ADR-0068 listed as
  BBR's structural win is, on our shipped controller, Copa's biggest.
- **Policer: CC-independent starvation.** Both controllers pin at
  8.0 ± 0.4 Mbit (ratio 0.99–1.00) at identical ~3.8% police-drop
  fractions and zero queue. The token-exhaustion drop BURSTS stall the
  recovery/frontier pipeline behind either controller — the same
  binder family as the clean-contention starvation (§16.27 caveat,
  goal-gate 2026-07-19). No CC swap, and no ε̂-referenced loss regime,
  can move this cell until that pipeline survives burst loss; the
  policer row is a recovery-plane work order, not a CC verdict.
- **Competitive mode: confirmed inert where pre-registered.** The Copa
  §2.2 detector structurally cannot fire on a policer (it keys on the
  queue signal the policer suppresses — measured: 0 engagements in 15
  of 16 runs, one transient self-corrected switch) and did not fire on
  the shallow cell; compete arms moved nothing by ≥ σ.
- **The realtime crown survives real jitter** (external validity for
  §16.26/§16.31): tail_matrix p99 medians at jit15 are 92–96 ms vs
  36–39 clean — wire-class inflation (RTprop ×4 + jitter tails), not
  machine collapse; 1000/1000 delivered every rep, both seeds.

The fusion's targets are now numbers (ADR-0068 MEASURED-BASELINE
addendum): hold Copa's shal8/queue/tail class, reach ≥ 0.9× BBR-under
across the jitter dose-response WITHOUT the standing queue (the honest
mechanism bar — a rate-model feed-forward alone is not predicted
sufficient, since the jitter-cell binder is the store dwell), and treat
the policer as blocked on the burst-loss recovery prerequisite. A
prediction table where the most confident row (shallow-buffer
loss-conversion) inverts is exactly what the pre-registration
discipline is for: the map, not the indictment, is the deliverable.
### 16.34 The lossy-singles structural terms executed: compact wire framing flips ON; the window-decoupling law is refuted and re-attributes the re-fire loop (2026-08-06, `feat/window-mtu`)

The §16.30 closed accounting left two structural terms and one
interaction on the table: the framing/MTU tax (~4.3/0.95 Mbit at c2/c3),
the spurious-retx term (~2.7/1.7, attributed to the 1024-latch's
standing queue), and the B1 jitter-cell Copa dwell ceiling (§16.33).
Both levers were pre-registered, built default-OFF, and measured on
seeds 42+7 ×8 interleaved (goal-gate "Window Decoupling + MTU Scaling").
One flipped; one refuted with the more valuable result.

**Part 2 — the framing tax is mostly rp's own 65 B/packet, and it is
gone.** The per-symbol wire overhead (119 B, all fixed) decomposes as
28 IP/UDP + ~26 QUIC + **65 B of rp framing** (8 magic+version + 57
bincode-fixint, including two 8-byte length fields for lengths the QUIC
datagram boundary already carries). The derivation refuted both named
MTU options first — filling the 1350 floor is worth +0.1 Mbit
(arithmetic), MTUD-style payload scaling is worth ≤ +1 and re-exposes
the §12.12 black-hole wedge geometry unless the floor rises with it —
and named the third: a compact v5 DATA frame (tag byte + varints,
payload to the datagram boundary, ~15 B) recovers ~50 B/packet with NO
MTU change, no symbol-size change, and shrinking datagrams (no wedge
surface). Measured: wire 116.1 → 111.9 MB per 100 MB object (overhead
119 → ~71 B/pkt), **sc2 +2.59/+3.64 Mbit ≫σ, sc3 +0.55/+0.60 ≫σ, c7
+8.1/+4.6, c8 unregressed, realtime crown spot unregressed, wedge
green** — every pre-registered clause on both seeds. `RWM_WIRE_COMPACT`
ships DEFAULT ON (PROTOCOL_VERSION 5; `=0` = legacy-framing opt-out).
The c2 gap to quinn-BBR narrows −14% → −4…−5% (87.8–88.1 vs 91.9); by
§16.32's identity (the residual c8 gap ≡ the single-path c2 gap) the
same term moves the c8-to-kernel-MPTCP distance.

**Part 1 — the window/inflight decoupling is REFUTED, and the
refutation buys three attributions.** The law family (wire budget = the
live head span above the SACK frontier, gated at
anchor·(K+gain−1) + rate·min(stall_age, 100 ms) — the 1024-latch's
stall insurance made explicit and continuous; holes and retention on a
separate backstop; N = 1 only; the B1 ceiling released under
Copa-sole) engaged exactly as derived: the standing queue died (echo
RTT 108 → 27 ms at sc2, 520 → 230 ms at sc3). The goodput did not
follow (sc2 −1.76/−0.37, sc3 +0.09/+0.22 — both prediction bands
missed), and the mechanism gauges name why, superseding three standing
attributions: (i) **the §16.30 re-fire loop is NOT queue-sustained** —
with the queue gone, fired stays ×3.3–4.2 realized drops; the re-fires
are re-serve-clocked (receiver hole re-advertisement each [25,100] ms +
per-seq cooldown), so window laws cannot kill them and `RWM_RECOV_SP`
is not subsumed; (ii) **the 1024-latch's honest insurance value at sc2
is ~0.4–1.8 Mbit** of sub-sweep ack-granularity and drop-granularity
cover (the −20% floor-law cliff of §16.22 sits below ~256, not at the
honest size); (iii) **the §16.33 jitter-cell dwell ceiling was not the
store** — with the ceiling released (outstanding free to 1900) Copa
holds 0.29–0.35× BBR-under unchanged (−1.0…−1.4 if anything): the
CC×store interaction is the empty-pipe recovery stall alone, which
re-scopes ADR-0068's jitter bar onto the recovery plane's dwell. One
scope defect (the paused N1 sampler leaking src-inflight at duals) was
caught by the pre-registered c7 clause, fixed, and the duals re-measured
TIE before any verdict. Register row; the law and its tests remain as
the measured A/B arm. The composed arm (decouple + compact) is the best
sc3 ever measured (16.86/16.84) — recorded as the starting point for
any future re-ask, which must attack the re-serve clock or the recovery
dwell, not the window.

### 16.35 The receiver per-message wall: one mis-scaled detector was a quarter of every core-second, and the c1 sink more than doubles (2026-08-06, `feat/recv-permsg`, `RWM_EST_CADENCE` DEFAULT OFF)

§16.28's verdict named its successor with numbers: after sender
batching, the engine RECEIVER saturated (~1.1 cores) at its ~22–23k
msgs/s per-message service wall ≈ 210–230 Mbit/sink — ×4.3 under
quinn-BBR's 915 on the same box. The v5 re-baseline moved the def arm
+7% (the compact parse is measurably cheaper) and left the receiver
the binder, so the profile targeted it at the wall.

**The profile (both sides + the reference).** The dominant family on
BOTH sides is one call chain: `LossEstimator::record_batch` → the
inlined Adams-MacKay BOCD changepoint update — 22.4%/core on the
receiver (which runs it per received Data message before sending the
legacy per-batch Ack) and 25.9%/core on the sender (which runs it per
received Ack). The detector is O(MAX_RUN_LENGTH = 200) with ~2 ln +
1 exp per run length and two Vec allocations PER UPDATE — designed,
per its own constructor comment, for ~2 s batch cadence ("regime
changes every ~100 batches"), and driven by the window wire at
~22 kHz: ≈ 4.4 M transcendentals + 44k allocations per second per
side. Everything else is flat (allocator ~5%, loop machinery ~3%,
AEAD 2.2% — noise again). The same-box reference row: quinn-perf's
receiver takes 946 Mbit/s at 0.455 cores ≈ **5.1 µs/QUIC-packet**,
AEAD-dominated, acking once per ~24 packets — while rp paid **~48
µs/message** with ~2 control datagrams emitted per data message.
Decoder/reassembly/frontier — the FEC feature's own cost — did not
chart (< 0.4%/core at c1): the gap was overhead, not feature.

**The build (`RWM_EST_CADENCE`, default OFF, pure compute, no wire or
timing change):** restore the detector's design cadence. Clean
observations accumulate; the BOCD flushes the accumulated counts on
every LOSS-bearing call (zero staleness on informative observations)
or a 10 ms heartbeat (≪ the 100 ms recovery round). EWMA, Beta,
burst flag, and the per-symbol GE chain stay per-call.

**Measured (seeds 42+7, ×8 interleaved, dnf 0):** c1 def 193.7/197.8
→ est **314.8/323.1 (+62.5/+63.3%, per-run ranges disjoint)**;
composed with §16.28's sender batching: **446/460 at 400 MB and
480–505 Mbit/s sustained at 1.2 GB** — the per-message service wall
moved from ~22–23k to **~46–62k msgs/s serviced (~23 µs/message, was
~48)**, and the estimator family is gone from the post-build chart.
sc2/sc3 hold within σ at −22…−40% CPU (the tax was real everywhere;
only c1 had wire headroom to convert it); realtime crown unregressed
(medians 36.4/42.6 ms, 1000/1000 every rep). **No flip:** the c7
dual clause failed (0.942/0.951×Σ vs required 0.97, both seeds) with
the mechanism gauge-attributed — the faster ack clock feeds the
LEGACY plain anchor's windowed-MAX burst peaks (btlbw over-read a
further ×3.4–3.7, echo 265 ms, sweeps ×7), and only the N ≥ 2 pooled
store has headroom to convert that into a standing queue; at N = 1
the 1024 latch clamps it inert. The named successor composes the
cadence with the honest-anchor family (ADR-0061) at duals. Distance
to the external bar after this branch: quinn-BBR 915–922 = ×1.8–2.0
of the measured opt-in ceiling (was ×4.3); the remaining per-message
terms are flat, with the dual-ack density (legacy per-batch Ack +
WindowAck vs quinn's 1-per-~24) the largest named structural
residual, measured below this session's 5% build bar. Goal-gate
"Receiver Per-Message Wall" carries the full tables.

### 16.36 The est×honest-anchor composition: the c7 blocker is two mechanisms deep, and the fast path stays an opt-in (2026-08-07, `feat/ship-est-cadence`, `RWM_POOL_ANCHOR`; composed default flip measured and REVERTED by its pre-set clause)

§16.35's named successor was built and measured: at N ≥ 2 the pooled
store's cap reads a per-path HYGIENE-GRADE SEND-INTERVAL anchor
(`SendRateAnchor` fed at `charge_in_flight` — every wire send on the
path; clock-gap discard per ADR-0061) through the honest-cap law
(Σ_i anchor_i·(K_i+gain−1) + rate_i·(gain−1)·R, clamped [floor,
N·knee]), while the Copa cwnd feed stays byte-identically on the
legacy path — the measured −22…−27 c7 RS-composition price and the
§16.34 src_inflight leak both structurally unreachable. Getting an
honest SEND-side rate took three statistic iterations, each
smoke-falsified and recorded pre-battery: the windowed-max latches
store-refill bursts (an admission-gated sender legitimately bursts
whole buckets at emission speed — sr read 53k vs 8.9k truth); the
plain mean inherits the anchor⇄cap circularity (cap oscillation
3588→938); the shipped statistic is the RATCHETED MEAN (max of two
rolling half-window means — BBR's filter structure over interval
means, on the `EchoRatioMin` two-half-window pattern).

**Measured (seeds 42+7, ×8 interleaved, dnf 0, crown clean):** the
composition works as far as the anchor goes — per-path sr ≈ 1× truth,
the 4096 clamp gone from the c7 operating point (cap at the derived
2–3k), echo out of the est 265 ms class, the est-only control
reproducing the §16.35 blocker exactly (0.938/0.949×Σ) — and **c1 =
463/482 Mbit/s mean (min 405/454; 477/482 sustained at 1.2 GB) at
−72% receiver CPU/bit**. But the c7 clause failed its pre-set rule:
new 0.968/0.959×Σ vs ≥ 0.97 (prior default 0.981/0.972). The gauges
name the second mechanism: with the over-read removed, the honest
pool BECOMES the binder — the store pins at its own cap (win = cap;
sweeps 8–21 vs prior 0–7) because a send-derived rate can never
ratchet above the cap-limited carried rate. **At N ≥ 2 the engine has
no honest un-self-referential rate source for the pool: the
ack-interval reads burst peaks (over), the send-interval reads the
cap's own shadow (under).** The prior default escapes only by
accident: its Σcwnd governor floats the store BELOW a pool the
over-read inflated into slack. NO FLIP — defaults reverted; the
documented fast single-path opt-in is `RWM_EST_CADENCE=1` (which now
carries `RWM_POOL_ANCHOR`) + `RWM_EMIT_BATCH=1`: 463–508 at c1, and
at duals −1.3…−2.2% instead of est-alone's −4.3…−6.2%. One honest
side-datum: c8 improves under the composition (0.758/0.777×Σ vs
0.746/0.715, σ halved, collapse tail cut) — the honest small pool
moves toward the max_i-cap class the c8 attribution predicted. Named
successor (not built): a delivery-clocked per-path rate sampler
decoupled from the cwnd consumer (physics-bounded by delivered
packets, feeding ONLY the pool law), or bounding the est-arm's
anchor_floor cwnd inflation so Σcwnd stays the dual governor.

### 16.37 The delivery-clocked pool anchor: the successor was built exactly as specified, did exactly what it promised, and moved c7 the wrong way — so the blocker was never the anchor (2026-08-07, `feat/pool-delivery-anchor`, `RWM_POOL_DELIV` + `RWM_FLOOR_BOUND`, both DEFAULT OFF; sub-goal closed as a STRUCTURAL BOUND)

§16.36 named two successors and this section measured both, on a
battery identical to §16.36's so the numbers compose.

**Arm A, the delivery-clocked pool anchor.** §16.36's verdict was that
at N ≥ 2 the engine has no honest un-self-referential rate source for
the pooled store: the ack-interval clock reads burst peaks (over), the
send-interval clock reads the cap's own shadow (under, and the store
pins at its own ceiling). Arm A supplies one. `DeliveryRateAnchor` is
the BBR `GenerateRateSample` statistic — `delivered /
max(send_elapsed, ack_elapsed)`, windowed-max over ≈10·RTprop, samples
below one RTprop REJECTED and ACCUMULATED rather than latched, ADR-0061
clock-gap discard with hold-through-disturbance — rebuilt as a
STANDALONE SHADOW estimator on aggregate per-path cursors (a monotone
send cursor fed at `charge_in_flight`; an accounted cursor advanced by
delivered + LOST at the ack arm, which is what lets a non-per-seq
cursor resolve send spacing at all). It reaches exactly one consumer:
`pool_rate_anchor() = max(delivery_max_bw, send_ratcheted_mean)` — one
formula, no branch, both terms honest lower bounds on the bottleneck —
feeding only the N ≥ 2 pool law. No CopaFeed is instantiated, so the
measured −22…−27 Mbit c7 RS-composition price and the §16.34
`src_inflight` leak stay structurally unreachable, and N = 1 is
bit-exact. It needed no statistic iterations, where the send-side
anchor needed three.

**It worked.** At c7 the delivery clock read 1.5–3.4× the send mean
(`dr` 15 665–41 163 vs `sr` 9 065–13 382 sym/s) while sitting 4–20×
BELOW the same paths' legacy ack-interval `btlbw` (57 531–309 504) at
the same instant — so it is genuinely a different clock, and its guards
genuinely held. The pool Σ rose to 3 878–7 326 against §16.36's
1 697–3 103; the store stopped pinning at its ceiling (800–1 800
symbols of slack where §16.36 measured win = cap); sweeps fell from
8–21 back into the prior default's 0–7 class. Every element of the
pre-registered mechanism prediction landed, on both seeds.

**And c7 got worse: 0.958/0.931×Σ**, against the SAME session's
attempt-1 arm at 0.977/0.956 and the shipped default at 0.975/0.995 —
below the required 0.97 on both seeds, and below the mechanism it was
built to improve. c1 held at 454.6/480.8 Mbit/s (sustained 498.4/484.0,
1.2 GB) and sc2/sc3 held; crown clean 1000/1000; dnf 0.

**Arm B, bounding the anchor floor**, failed both of its own clauses:
`min(gain·max_bw·RTprop, gain·sr·RTprop)` cut c7 cwnd to 237–305 (vs
1 006–2 356) exactly as designed, and landed c7 0.969/0.969 — under the
clause on both seeds — while costing **c1 396.4/398.0, −14% and below
the 430 PRIMARY.** The ack-interval over-read is doing load-bearing
work at N = 1: making the prior default's Σcwnd escape *derived*
instead of *accidental* is not a free correction.

**What this refutes is §16.36's own attribution.** A mechanism that is
supplied in full, behaves exactly as its theory says, and moves the
target the wrong way was not the mechanism. The gauge that names the
real one is the stall/sweep signature at end of transfer: BOTH est arms
carry ≈2× the shipped default's stall-idle time (1 400–2 178 ms /
≈200–245 stalls vs 822–1 026 ms / 109–157) and ≈3× its sweep count —
**and that signature is INVARIANT to the pool**, barely moving while
the pool's Σ nearly doubles between arms A and attempt 1. With the
stall rate fixed, a larger pool simply strands more outstanding per
stall, which is why the arm that fixed the pool most thoroughly scored
lowest. **The c7 cost of the faster ack clock is owned by the recovery
plane's patience/stall behaviour, not by the pooled store's rate
input.** Three rate sources have now been built and measured on
identical batteries — ack-interval windowed-max (0.938/0.949),
send-interval ratcheted mean (0.968/0.959, and 0.977/0.956 re-measured
here), delivery-clocked windowed-max (0.958/0.931) — and the c7
ordering does not track anchor honesty at all.

**The sub-goal therefore closes as a documented STRUCTURAL BOUND, and
the c1 win ships as a documented OPT-IN rather than a default.**
`RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1` (which carries `RWM_POOL_ANCHOR`;
leave `RWM_POOL_DELIV=0` — arm A's term is strictly worse at duals)
measures **c1 454–493 Mbit/s mean, per-run 425–521, 1.2 GB sustained
484–498, at −72% receiver CPU/bit against the 179–206 shipped default —
×2.3–2.5, crown-clean, sc2/sc3 unaffected, dnf 0** — for a dual-path
price of **c7 −1.8…−4.0% of Σ** and c8 inside its noisy WATCH band. The
single-path user pays nothing and gains ×2.4; the dual-path user pays up
to 4%. Both numbers come from the same battery, which is what makes
reopening this a decision rather than a rediscovery. Anyone who does
reopen it should start at the recovery plane's ack-clock sensitivity —
not at the anchor, which has now been eliminated three ways.

## 17. The Measured Regime Map (2026-07-19)

This section is the paper's standing verdict on what the model's
implementation actually does on the wire, synthesized from the post-audit
evidence base (§16.15 onward, §12.11–§12.12, §8.4.1's validation, and the
2026-07-19 batteries) and replacing every earlier regime claim: §16.8's
"production-bounded at parity" (bannered), §16.6's grounded verdict
(bannered), the abstract's unqualified Copa recommendation (annotated), and
the goal-gate ledger's 2026-07-08 FINAL CONSOLIDATED VERDICT / L3 REGIME MAP
(bannered there). The primary record for every number is the goal-gate
ledger; this section states positions and mechanisms.

*Decision index (2026-07-21): the DECISIONS of this arc are recorded as
architecture decision records ADR-0052…0067 (`docs/adr/README.md`), one
per decision, each linking its ledger evidence; the code-consolidation
feature triage is `docs/adr/VISION-TRIAGE-2026-07.md`. The ledger and
this paper remain the measurement record; the ADRs are the decision
index.*

The map has been overturned three times — by the methodology audit (§16.15:
the coded path was dead in a whole era of measurement), by the substrate
chain (§16.17–§16.19, §12.12: the "walls" were controllers and constants
under the transport, not the architecture), and by the hardware divide
(§16.19: everything before 2026-07-14 ran on a qemu64 vCPU doing software
AES-GCM; the passthrough re-baseline reproduced every plain/Copa cell, so
pre-divide ratios carry, but absolute cross-era comparisons must name the
divide). This rewrite is the first coherent statement since.

### 17.1 The substrate chain: eight walls, in order

The transport's measured history is a chain of walls, each named with a
mechanism before it was fixed or refuted:

1. **quinn's hidden loss-reactive Cubic** (§16.17, §12.11). Every send —
   datagrams included — is gated on quinn's congestion window, so the
   effective controller was min(app CC, quinn Cubic), and Cubic under GE
   loss WAS the "15–17 Mbit/s link ceiling" (plain+BBR: 74.5 pooled,
   ×4.3). Fixed as a policy surface (`RWM_QUIC_CC`, §17.2).
2. **quinn's PMTU black-hole detector** (§12.12). A GE all-large loss burst
   is indistinguishable from an MTU black hole to quinn's heuristic; the
   reset (MTU 1200 < the 1279-byte symbol) turned every data send into
   sender-side `TooLarge` for the 60-s cooldown — the cross-arm "collapse
   run". Fixed, ships default-ON (`min_mtu = initial_mtu = 1350`);
   deterministic repro 63.5 s → 5.8 s; 0/68 collapse runs post-fix.
3. **The coded-only wire's O(G²·S)** (§16.18). The ~34 Mbit/s "generation
   machine" ceiling was the wire MODE — every DoF arriving dense at both
   ends — not the solver. The systematic-repair wire is the
   O(k·G·S + k³) machine: gen single-c2 33.9 → 70.9 (×2.1, = 0.92× the
   plain+BBR single of its era).
4. **Decoder waste** (§16.18). The sparse-aware rewrite (known columns
   never enter the matrix) — a pure, output-identical speedup, ×1.2–5.0
   at L0, differential-tested against the retained reference decoder.
5. **Crypto — refuted as a wall** (§16.19). AES-NI cut CPU per byte
   30–38% and moved no throughput cell. A CPU wall must move when the CPU
   gets faster; none did.
6. **Receiver threading — refuted below ~150 Mbit/sink** (§16.19). The
   single-threaded engine sinks 187.7 Mbit/s; the C7 receiver pinned to
   one core runs at 0.66 core busy. Parallelization was correctly NOT
   built; it becomes the live lever only above ~150 per sink (§17.6
   item 2).
7. **Per-transfer flow control — the actual multipath binder** (§16.19).
   The outstanding pool (1024 symbols, a per-transfer constant) is a
   Little's-law ~100–128 Mbit wall, CPU-invariant — which is exactly why
   it survived every CPU-era lever and the hardware upgrade. The
   path-scaled pool (knee ≈ 2048/path) unlocked C7; the per-path-accounts
   refinement won symmetric cells and regressed heterogeneous ones
   (§16.19 addendum; §17.3). The pool ships DEFAULT ON since 2026-07-21
   (the consolidation LOO battery: removal re-opens a c7 collapse class
   on both seeds; goal-gate "Consolidation") — with the c8 pool-law WATCH
   recorded in §17.7.
8. **Multipath recovery-plane over-emission — the fifth control-plane
   wall** (§16.23–16.24). The recovery engine kept GLOBAL clocks and
   serials under striping: cross-path scheduler-created gaps read as
   holes (82% of c7 retransmits fired inside their flight's own-path
   RTT clock) and global batch serials poisoned the per-path loss
   estimators (0.62–0.77 read at a 0.1%-loss cell). Fixed as a knob
   (`RWM_RECOV_MP`): RFC 9002 loss detection generalized per path —
   the c7 retransmit share drops below single-path parity and the
   dual-c1 anti-scaling is eliminated; the freed wire not converting
   1:1 relocates the residual Σ-gap to frontier-recovery latency on
   the ack-serialized retention store (§16.24) — which the SACK-clocked
   store release then closed (§16.25). Ships DEFAULT ON since 2026-07-21
   (consolidation LOO: removal −12.3/−13.9 ≫σ at c7 both seeds).

What remains after the chain is structural, not artifactual: the
presence⊥throughput identity (§14.33, §16.8). A saturated single reliable
path has no spare bandwidth to carry a repair that buys back a round-trip;
FEC = ARQ parity is the single-path bulk ceiling, re-confirmed on honest
hardware (gen-sys single at 0.97–1.0× plain+BBR — the coding is free, not
free throughput).

### 17.2 The CC policy surface

Substrate CC is POLICY (`RWM_QUIC_CC`; §12.11), with three measured
positions:

- **Cubic** (now the explicit `RWM_QUIC_CC=cubic` legacy arm): dead as a
  performance choice — it was wall #1. It was the unset default until
  2026-07-21 (the Default CC Flip below).
- **BBR-under** (**the shipped default since 2026-07-21**, goal-gate
  "Default CC Flip"): the bulk-throughput champion (plain single-c2
  74.5–79; C7 ~100–105 baseline), at the cost of standing queue (38 ms at
  c2-class; 88–124 ms p50 on the C8 slow path, p90 to 2.5 s) and a
  residual dual-cell bimodality.
- **Copa-sole** (passthrough + the §12.4 wire-signal addendum): the
  queue/tail champion. With the delay term wire-clocked (the sender's own
  reservoir dwell structurally excluded) and δ mapped from the hint with
  no new constants (δ = 0.5/ζ, live-verified Bulk 0.005 / Auto 0.5 /
  Realtime 50), Copa-sole holds the NETWORK standing queue ×18/×16/×6–7
  tighter than BBR-under at sc2/sc3/c7 (5/30/7 ms vs 89/487/50, measured
  on the consolidated stack) and ties BBR on the realtime c2 message tail.
  Bonus property the model predicted (§12.3): the ±v/δ dither keeps the
  RTT floor fresh without ProbeRTT — no FEC protection gap. **Its bulk
  cost is a MEASURED TRADEOFF that does NOT close on the fixed substrate**
  (§16.27, goal-gate "Copa-Sole on Clean Substrate", 2026-07-22):
  copa/bbr 0.89× sc2, 0.97× sc3, 0.73× c7, 0.57× c8, 0.66× dc1 (≫σ both
  seeds). The consolidation walls WIDENED the gap — they lift BBR's
  aggregation while Copa's δ-equilibrium caps cwnd near BDP + 1/δ and
  leaves the freed pipe on the table — and the §12.11-era C8 domination
  inverted (it was a broken-substrate artifact suppressing BBR).

The fairness gap first measured 2026-07-19 (the first cross-traffic
battery, goal-gate "Copa Competitive Mode + Cross-Traffic"): BBR vs one
Cubic flow takes a 0.95–0.96 share at the lossy c2 cell (Cubic is
Mathis-bound there) and 0.24 on the clean bufferbloated bottleneck —
mildly aggressive under loss, yielding under standing queue, within the
deployed-BBRv1 envelope; documented at the flip site as the caveat, not a
blocker. The shipped default is BBR (2026-07-21), and the clean-substrate
re-measure (2026-07-22) CONFIRMED it: the two-value CC surface does NOT
collapse to one δ-controller — Copa's bulk gap is its own δ-equilibrium
dynamics, not a substrate artifact, so **NO flip to passthrough**. **Copa
is NOT deprecated** — it is retained as the δ-capable controller the hint
contract structurally requires (BBR has no latency price) and the
queue/tail champion (×18/×16/×6–7 tighter, tail parity, no ProbeRTT
stalls). The endstate is the hint's declared price choosing the
controller — bulk → BBR-under, latency-priced → passthrough+Copa — a
policy mapping over this surface documented as a MEASURED TRADEOFF, not a
mode switch to be collapsed on a wish. The CC endgame (one controller
across the surface) is the fusion, §17.6 item 10 / ADR-0068: δ-priced
probing over a BBR-style rate model, which inherits this battery's bulk
gap as its target.

*External validity (2026-08-06, §16.33 / goal-gate "Adversarial Cells
(B1)"): the clean-cell map above does NOT extrapolate to adversarial
links, in both directions. At an 8-packet bottleneck buffer the surface
INVERTS — shipped BBR-under collapses to 0.12× its clean class (burst-
quantized delivery poisons the max-filter anchor ~10×) while Copa-sole
holds its full class, ×7.7–7.9 over BBR; at a 100 mbit policer BOTH
controllers starve identically at ~8 Mbit (the burst-loss recovery
pipeline, not the CC, binds); on aggregation-class jitter cells Copa's
gap widens to 0.29–0.38× with a strictly monotone jitter dose-response
whose dominant term is the store-dwell ceiling at 40 ms RTprop, not the
delay law. The two-value policy surface therefore has measured
adversarial edges: "bulk → BBR-under" is clean-deep-buffer advice, not
a universal; the realtime crown's tail class survives jitter (p99
92–96 ms vs 36–39 clean, wire-class inflation only).*

### 17.3 Aggregation vs Σ — the bulk N× verdict

Claim under test: bulk multipath ARQ striping should approach N× the
per-path rate (nothing about bulk is latency-bound; the resequencing
buffer absorbs skew). Verdict: **substantially validated at the symmetric
cell; mechanism-named gap at the heterogeneous one** (§16.19 + addendum).

- **C7 (symmetric): best 0.87–0.97×Σ** of same-session per-path singles —
  ×1.72–1.89 of a single path with the path-scaled pool, and the per-path
  accounts arm touched 0.97×Σ with the pooled arm's collapse mode absent.
  The binder was flow control (wall #7), not the receiver thread; the
  mechanism of the user's claim was right, the conjectured constraint was
  not. **[UPDATE 2026-07-21, §16.25: with SACK-clocked store release
  (now default ON) composed with recovery suppression, c7 = 1.018–1.045×Σ
  of the base singles (0.98–0.99× the SR arm's own Σ) on both seeds — the
  symmetric Σ-gap is CLOSED.]**
- **C8 (heterogeneous): best ~0.74×Σ (pooled arm; 0.79–0.80×Σ in the #84
  session — session drift is why all verdicts are same-session
  interleaved).** One shared pool cannot fit a c2-deep and a c3-shallow
  path simultaneously; the per-path accounts built for exactly this cell
  regressed it to 0.38–0.43×Σ under BOTH CC families, because the
  cap-full placement redirect over-commits the slow account (~2048
  symbols parked on a 15.7 Mbit path ≈ 1.3 s of dwell → holes recover
  ~13× slower → the frontier serializes → the echo-RTT feedback holds the
  account open). Per-path admission is not refuted; per-path admission
  with an UNGUARDED redirect is.
- The two named residuals are §17.6 items 1–2. No cell exceeds its
  link-class Σ ceiling; every "wall" so far has been an unscaled constant
  or a hidden substrate controller, not the architecture.

### 17.4 The FEC story, honestly

- **Single-path bulk: parity, and that is the theorem, not the failure.**
  Gen-sys single = 0.97–1.0× plain+BBR (post-divide); c3 recovery rides
  at 0.95× the substrate's own recovery ceiling. The identity (§14.33)
  held through every era.
- **Coding is ~free on real silicon** (§16.19): ~0.37 s receiver CPU per
  25 MB over plain at r = 0.03. The "decode ceiling" era (§16.18) is
  closed.
- **Generation coding is the stabilizer.** Coded arms collapse variance
  wherever plain arms go bimodal (gen-bare C8 σ 0.14–0.48 vs plain
  2.0–2.1; gen-sys C8 σ halved vs plain+BBR with no bimodality). Where
  plain+BBR pays a bimodal penalty for touching a lossy path, the coded
  transport parks stably — often the operationally decisive property.
- **Tail latency is the crown: 12–48× message-p99** vs QUIC/kernel-TCP at
  WiFi-class loss (goal-gate Full Benchmark Re-Run, Metric A; pre-divide,
  reproduced across re-runs), held by the shipped streaming machine and
  DEFENDED at the 2026-07-19 flip gate (§16.20.7).
- **A NEW measured point on the (δ, ρ) surface: delivery-complete
  realtime.** The unified small-δ machine at the c3 realtime cell
  delivers 99.4–100% where the shipped streaming machine leaves 24–26%
  DNFs, at ×3–4 completer medians and cod/src 0.34–0.42 (r consumed as
  computed AND recovered in-window — §16.20.3's span law live at the
  receiver). That is a distinct profile candidate — reliability bought
  with completion tail — not a defect of either machine.
- **r\* under bursty loss: correct at the solver, entangled at the shipped
  realtime wire.** The §8.4.1 window-mass-quantile solver is
  oracle-validated on real traces (feasible-cell worst residual 2.88× →
  1.41×; heavy-tail synthetic 5.1×-miss → 0.99×-hit; infeasibility
  DECLARED, per §8.8 a contract renegotiation not a solver fix) and ships
  ON. The emission quantity law is fixed (the TaperBudget: the wire
  consumes r as computed, L1-confirmed) — but consuming r DEGRADES
  streaming-family delivery (−19/−25 pp, both seeds, both rungs): the
  leading-window unsolvable-span entanglement, confirmed on the real
  substrate. The solvable span exists structurally in the unified machine
  (trailing span, §16.20.3), where it IS delivery-complete; the RLC
  family at the same cell is ARQ-complete (§16.20.4's rescope). The flip
  chain is §17.6 item 9.

### 17.5 The three-machine map

**[SUPERSEDED 2026-07-21 by §16.26: the unified span machine is now the
SHIPPED DEFAULT (`RWM_UNIFIED` ON) — one machine across the δ axis. The
map below is retained as the record of the pre-flip standings; the
streaming and legacy-RLC machines survive as the `RWM_UNIFIED=0` opt-out
arm, with streaming's retirement in the deprecation register behind a
re-test clause.]**

Three receive machines exist (§16.20); each has a measured niche, a
flip-gate status, and a retirement condition:

- **Streaming two-layer** (shipped Realtime default). Niche: the 12–48×
  message-tail crown. Status: defended at the 2026-07-19 flip gate —
  unified realtime is not tail-parity (p99 medians ×2.7–3.3 at the bursty
  cell plus a 3/10 stream-collapse class), so streaming keeps Realtime.
  Honest liabilities on the record: 24–26% DNFs at the c3 100 KB realtime
  perf cell, and the L1 ordering surprise (§16.20.7) — the legacy-RLC
  realtime arm posts BETTER p99 medians at the c3 cells (234/273 ms vs
  510/822). Retires only if a case engages both the unified trade AND
  that ordering datum (§17.6 item 7).
- **Legacy RLC family** (`RlcWindowDecoder` + `GenerationDecoder`).
  Niche: the bulk half — the gen-sys wire is the bulk-champion
  realization (parity with plain at ~free CPU); and, unexpectedly, the
  best c3 realtime p99 medians of all three machines. Status: the
  unified machine reached throughput-parity + CPU-parity against it on
  the bulk wire (PASS) but lost to it at the realtime tail; it carries
  its own 2/10 total-wedge class and a proven rank-loss defect under
  reorder (which unified fixes). Retires — both decoders — when unified
  passes ≥ legacy-RLC everywhere; the remaining gap is realtime tail
  only.
- **Unified span machine** (`RWM_UNIFIED`, default OFF; §16.20). Niche:
  the destination — one global sparse-aware decoder, one δ-continuous
  span law (A\*/M\*/Δ), differential-proven against all three legacy
  decoders, bulk parity + CPU parity passed at L1, and the
  delivery-complete realtime point above. Status: both flips NO
  (2026-07-19); the named blocker is the c3-1200B stream-collapse class,
  and the M\* knee is unreachable behind two anchor defects (§17.6
  items 3, 5).

(The block RaptorQ pipeline remains §15's other knob, untouched; its lossy
completion is recovery-bound and was never lifted by the CPU-era fixes —
the window/generation path is where the measured wins live.)

### 17.6 The roadmap (named, not built)

Prioritized; each item carries its gating decision. Nothing here is
asserted beyond its naming evidence.

1. **percap-redirect-guard** — **[MEASURED 2026-07-19, §16.19 guard
   addendum: the floor-clock bound closes the redirect channel (dwell
   ~4× down, half the c8 regression recovered, both CC families) but
   PBP-G < pooled PBS at c8 both seeds — flip stays NO.]** ~~The residual
   inherits the gate: dwell-bound the CAP itself
   (cap_i ≤ gain·rate_i·RTprop_i), generalize the plain-mode rate anchor
   (§16.15 sampler), and weigh account isolation's no-borrowing tax at
   asymmetric cells.~~ **[Cap re-derivation MEASURED 2026-07-19, §16.19
   honest-cap addendum: the honest law (residence K·RTprop +
   recovery-clock runway R = 100 ms — the literal floor-clock form was
   refuted by its own smoke) resolves the sc2 −20% exactly and puts
   percap ≥ pooled at c7 both seeds, but c8 still trails pooled under
   BOTH CC families with honest caps — the no-borrowing tax is the
   CONFIRMED c8 binder. Item redirects to bounded account borrowing
   (needs a new dwell law: borrowed symbols park on the lender, fly on
   the borrower) or accepting pooled PBS as the c8 record. Sub-residual:
   the slow path's send-interval anchor over-reads ×3–5 under multipath
   placement (frontier-advance burst attribution suspected).]**
   **[CLOSED 2026-07-19, §16.22 (`feat/store-borrowing`): bounded
   borrowing derived, built, measured — the law is gauge-perfect (c7
   loans ≡ 0 by theorem; c8 loans one-directional and repaid) but the
   lender-solvent slack cannot match the pool's depth: PBP-B < PBS both
   seeds. VERDICT: pooled `RWM_STORE_PATHS` is the c8 answer; percap(±
   borrow) the symmetric-cell tool; all default OFF. Sub-residual (iii)
   attributed (spurious cross-path-retransmit attribution, 57–76% of the
   cross-path class) and half-fixed (the §16.21 flight witness,
   `RWM_RS_ATTR`); named remainders: (iii-b) the p90 slow-anchor tail
   channel, and the honest-anchor c8-plain throughput circularity (third
   instance). This item is no longer a roadmap lever; the C8 0.9×Σ
   target is retired in favor of the pooled record.]**
2. ~~**Receiver/sender task parallelization** — live above ~150 Mbit/sink;
   the symmetric cell now operates at ~147 with the engine sink at 187.7.
   The next C7 lever after flow control.~~ **[CLOSED 2026-07-19, §16.23:
   third refutation — 1+1 pinned cores sustain the full c7 operating
   point both seeds; the engine task drains c7 with headroom (81–87%
   busy, empty queue); the c7 binder is multipath recovery-plane
   over-emission on a saturated wire (retx ×1.8, repair ×2.2–2.5 the
   single-path share ≈ the Σ-gap), and the sink ceiling is the pair of
   per-process service-time walls (~19.5–22k sym/s), c1-class only.
   Successor lever: multipath-aware recovery suppression (named, not
   built).]** **[Successor DONE 2026-07-21, §16.24: the per-path
   RFC-9002 law kills the waste (c7 retx 14.9→4.5%, dual-c1
   anti-scaling eliminated) and REVISES the attribution — the freed
   wire does not convert 1:1; the residual Σ-gap owner is
   frontier-recovery latency on the ack-serialized store.]**
3. **Unified-realtime stream-collapse attribution** (c3-1200B, 3/10 reps,
   p50 in seconds; candidates: EVICT-window × trailing-span interaction
   under sustained bursts, decode/delivery backlog, retention pressure).
   THE `RWM_UNIFIED` blocker; closing it re-opens the flip and schedules
   legacy-decoder removal.
   **[ATTRIBUTED 2026-07-19 (§16.20.8): not the decoder — an anchor
   defect + family-level transient amplification. The named fixes A
   (A\* anchor repair) and B (clock-gap estimator hygiene) are BUILT
   2026-07-19 (§16.21, `feat/anchor-hygiene`); the flip battery re-run
   is UNBLOCKED on the measurement side. Fix C (δ-honest overload
   shedding) remains open and still gates the flip itself.]**
4. **Legacy-RLC realtime total-wedge class** (2/10 reps, no stream summary
   within 30 s, same cell) — a distinct failure class, unattributed.
   **[Same root family as item 3 (§16.20.8); closes with its fixes.]**
5. **The M\* anchor pair + knee re-run** — the RTprop floor under-read
   (a DEFAULT_SRTT-class 50-ms seed surviving the 10-s min-window at a
   200-ms cell) and the static (pipeline+2)·G win backstop; then re-run
   the RTT-100/200 knee cells. Oracle PART 7b's depth-term prediction is
   neither confirmed nor refuted until this lands (§16.20.7).
   **[DONE 2026-07-19 — §16.21: the floor was the peer-report estimate
   recorded as a sample; with the pair fixed the knee ENGAGES at L1
   (r100 +25–31%, r200 +62–82%, both seeds, n=8, 0 DNF). PART 7b's
   deficit is confirmed in direction and ordering; measured ratios
   0.76–0.80/0.55–0.62 vs the in-model 0.64/0.39 (other wire binders
   remain).]**
6. **Copa competitive mode + the first cross-traffic cell** — Copa §4
   mode switching plus a shared-bottleneck battery (also carries BBR's
   unevaluated fairness). Gates any substrate-CC default flip (§17.2).
7. **The streaming-retirement gap** — attribute the L1 ordering surprise
   (legacy-RLC beats streaming's p99 medians at c3; the L0 proxy
   predicted the opposite). Prerequisite to any retirement case.
   **[MOVED 2026-07-21 (§16.26): the unified machine took the Realtime
   default and beat streaming's p99 medians at every battery cell; the
   ordering surprise is subsumed (all three machines are now one tail
   class at these cells, unified at-or-ahead). What remains is the
   register's re-test clause: hold the historic 12–48× record cells on
   the unified default before any code removal.]**
8. **The r200 M\*-arm bookkeeping cost** (~1–2 Mbit below fixed-depth at
   RTT 200, both machines, both seeds).
9. **The solvable-span default-flip chain** — trailing-span (decodable)
   proactive emission for the streaming/plain realtime path, or realtime
   routed through the RLC/unified family; revisit whether contract-priced
   repair should bypass the spare-cap gate. Gates `RWM_TAPER_R` and the
   full wire realization of the corrected r\* (§8.4.1) at realtime δ.
   **[CLOSED 2026-07-21 (§16.26): realtime IS routed through the unified
   family by default; `RWM_TAPER_R` rides the umbrella; cod/src 0.38–0.50
   consumed at the realtime wire and delivered 100% — r\* realized.]**
10. **The CC endgame: adversarial cells → measured Copa breakage → the
   fusion (ADR-0068)** — the named future lever for the one-controller
   endstate. The current cells (clean delay, deep buffers, no policers)
   cannot falsify a Copa/BBR fusion; the prerequisite is adversarial
   cells (delay-jitter / shallow-buffer / policer) with Copa-sole's
   measured breakage as the pre-registered baseline, then ONE controller:
   δ-priced probing over a measured rate model (the §16.21 anchors as
   feed-forward baseline; probe amplitude+dwell derived from δ — δ→0
   recovers BBR-class discovery, δ large recovers Copa's gentleness)
   with ε̂-referenced loss discrimination (loss ≤ ε̂ is FEC's job;
   persistent loss > ε̂ ⇒ bounded inflight — the channel estimator as
   the measured reference generic CCs lack). Literature verification
   (BBRv2 / PCC-Vivace / Nimbus, from sources) required before build.

Minor named items: the c7 unified-receiver +3–5% CPU signal; the np 2→1
live-path flap under saturation; the `RWM_STORE_PATHS` default battery;
flipping the harness gen-arm default to the systematic wire (§16.18's
recommendation).

### 17.7 The shipped default stack (2026-07-21) — the map's recommendation IS the default

The consolidation pass (goal-gate "Consolidation"; roadmap item 2) closed
the default-honesty gap: the shipped default, with every `RWM_*` env
unset, is now the composed best-measured stack — BBR-under (§17.2) +
SACK-clocked store release (§16.25) + the path-scaled outstanding pool
(wall #7) + per-path recovery suppression (wall #8) + the anchor-hygiene
pair (`RWM_MSTAR_ANCHOR`'s plain-live subset, `RWM_CLOCK_GAP`; §16.21) —
and, since 2026-07-21, the UNIFIED span machine (`RWM_UNIFIED`, §16.26:
one decoder on both RLC wires, the (δ,ρ,r) span law with its live A\*
anchor, δ-honest overload shedding on the realtime EVICT path).
Each member holds a per-member LEAVE-ONE-OUT row on both seeds against
the composed stack (the strictly-better criterion, pre-registered):
removing the pool re-opens a c7 collapse class (86–97 Mbit runs);
removing recovery suppression costs 12–14 Mbit ≫σ at c7 and returns the
dual-c1 retransmit flood; the anchor pair is measured free at every bulk
cell, unregressed at the realtime tail cell (the 12–48× crown survives
the stack), and carries its §16.21 wins. Measured at the default: c7
0.982–0.988×Σ of same-session singles, dual-c1 +15 above single with
retransmits ×10 down, singles and fairness class unchanged.

Honest residuals of the default: (i) heterogeneous c8 — under the
SACK-release law the LEGACY 1024 pool now reads better at c8
(0.85–0.87×Σ, the new best-measured c8 class, vs the stack's 0.72–0.76;
sub-σ per seed, consistent direction) — the pool law's c8 story has
MOVED since §16.22 and a c8-aware pool law is the named, pre-registered
follow-up (it also carries `RWM_PLAIN_RS`, whose composition probe
showed its witness cost resolved) **[EXECUTED 2026-07-27, §16.29: the
capacity-weighted pool law was derived, built and REFUTED at the cell —
the binder is not pool sizing but SLOW-PATH CONVERSION (the fast path
parks the un-SACKed span; the slow path converts ~nothing in any pool
arm), legacy-1024 remains the best-measured c8 class (0.87×Σ = fast
single + 2.7), and the RS witness cost was re-priced at −22…−27 ≫σ for
the symmetric dual (its full-stack-member candidacy refuted); the c8
residual stands with a sharper name]**; (ii) every REFUTED mechanism is now
in the goal-gate DEPRECATION REGISTER (two-stage: warn-on-activation →
clean-substrate re-test → delete), with the walls active at each
refutation argued per gate — nothing was deleted on a wall-tainted
verdict.


### 17.8 Supersession index

Bannered as superseded-era, retained as the record: §16.8 (the 2026-07-08
"arc concluded" verdict), §16.6's grounded verdict, §16.10–§16.14 (the
generation-inert era, audit-classified per section in §16.15), the
abstract's unqualified Copa recommendation (annotated in place), and — in
the goal-gate ledger — the FINAL CONSOLIDATED VERDICT (2026-07-08), the L3
REGIME MAP, and the Full Benchmark Re-Run's throughput rows. The §16.x
sections are deliberately NOT rewritten: they are the primary record of how
each wall was found, and the audit trail is the method's proof.

Code status (2026-07-27): the deprecation register's no-re-test rows were
EXECUTED — the refuted mechanism code (SACK_PRUNE, RECOV_MP_SERIAL,
INLINE_REPAIR/FRONTIER, RATE_WIRE, SRC_BP, the DAPS chain; ~1.9 kLOC) is
deleted from the tree; the sections above remain the record. FMTCP and the
streaming machine keep their re-test clauses (goal-gate "Code Consolidation
(2026-07-27)"). No regime-map claim changes. **[Later same day: FMTCP's
re-test RAN (§16.29) → CONFIRMED-REFUTED on the clean substrate; its chain
is cleared for deletion at the next consolidation pass. The streaming
machine's clause is unchanged.]** **[Consolidation pass 2, same day: both
cleared chains DELETED — FMTCP f841757 (−264 LOC), streaming machine
bccb32a (−1,708 LOC, scoped streaming-only; `RWM_UNIFIED=0` + Realtime
now = the legacy-RLC windowed machine). The register is fully executed;
no deprecated mechanism code remains (goal-gate "Code Consolidation 2
(2026-07-27)").]**

### 17.9 Competitive Position (2026-07-22) — the shipped default vs QUIC, TCP, MPTCP, measured

The first external verification of this map on the CURRENT shipped
binary (goal-gate "Competitive Baseline" — pre-registered matrix, both
netem seeds, same cells, same day, same VM; the raptorpath binary is
byte-identical to the §16.26 flip binary). Competitors: quinn-perf
(native QUIC, stock Cubic AND `--congestion bbr`), kernel TCP (Cubic and
BBR — reported on APP-LEVEL-ACKED delivery, the same completion
semantics as rp's perf; iperf3's sender-side numbers were measured
line-rate-clamped on short objects and are recorded only as a
cross-check), and kernel MPTCP v1 (native, subflow join proven by MIB
counters, CC per-netns). Bulk = 25 MB objects; realtime = 50 msg/s
messages (400/1200 B) with one-way delivered latency. Every sender
traverses the same seeded GE direction.

| condition × workload | rp shipped default | best competitor | verdict |
|---|---|---|---|
| c1 bulk (clean 1 Gbit) | 164–168 Mbit/s → **~200–204 shipped default (v5 framing, §16.34); 454–521 measured opt-in, reproduced in TWO independent sessions (`RWM_EMIT_BATCH=1 RWM_EST_CADENCE=1` — the est opt-in carries the `RWM_POOL_ANCHOR` honest dual-store law, §16.36; means 463/482 then 454/481–493, sustained 477–498 at 1.2 GB. The composed DEFAULT flip was measured and REVERTED TWICE by its pre-set c7 ≥ 0.97×Σ clause — 0.968/0.959 (§16.36) and 0.958/0.931 for the delivery-clocked successor (§16.37). CLOSED as a structural bound: the c1 win ships as a documented OPT-IN, and the recommended opt-in leaves `RWM_POOL_DELIV=0`)** | quinn-BBR 915; kernel TCP ~900 | ~~LOSS ×5.5~~ → **LOSS ×4.5 shipped / ×1.8–2.0 at the measured opt-in ceiling** — the §16.23 engine service walls, externally priced; per-message wall executed 2026-08-06/07 (§16.35–16.37: BOCD cadence + sender batching + honest pool; wall 22–23k → 46–62k msgs/s; the DEFAULT column is blocked NOT by the store's rate anchor — three anchors were built and the c7 ordering does not track anchor honesty (§16.37) — but by the recovery plane's stall/patience sensitivity to the denser ack clock, the named successor) |
| c2 bulk (GE 2.6%) | 78.6–78.7 → **87.8–88.1 (compact framing default ON, 2026-08-06, §16.34)** | quinn-BBR 91.9–92.4 | ~~LOSS −14%~~ → **LOSS −4…−5%** (kernel TCP-BBR delivery-acked: seed-split 61.5/91.6 → tie-class; all Cubic-family arms: WIN ×3–7). **Accounted to closure 2026-07-27, §16.30**: framing/MTU tax ~4.3 + reactive over-fire ~2.7 + ramp/idle margin; wire ≥98% utilized — not idle, not engine. **Framing term EXECUTED 2026-08-06, §16.34: v5 compact DATA framing (`RWM_WIRE_COMPACT`, flipped default ON) banks +2.6/+3.6 ≫σ both seeds; the window-decoupling lever was measured and refuted (register)** |
| c3 bulk (20 Mbit lossy) | 16.1 → **16.6 (compact framing, §16.34)** | quinn-BBR 18.6; TCP-BBR 17.5–19.4 | ~~LOSS −9…−13%~~ → **LOSS −8…−11%** (vs Cubic-family: WIN ×4–11). **Accounted to closure 2026-07-27, §16.30**: framing ~0.95 + over-fire ~1.7; `RWM_RECOV_SP` banks +0.32/+0.35 ≫σ both seeds (no flip — band missed); levers: window/inflight decoupling + MTU/payload scaling. **Executed 2026-08-06, §16.34: compact framing +0.55/+0.60 ≫σ (flipped ON); decoupling refuted at the singles (the re-fire loop is re-serve-clocked, not queue-sustained — the §16.30 spurious-retx term is re-attributed)** |
| c7 bulk (dual c2+c2) | 147–151 | MPTCP-BBR 149 (s42) / 169 (s7) | **TIE / LOSS −13%** — kernel MPTCP-BBR matches the crown cell |
| c8 bulk (dual c2+c3) | 67–74 (shipped; session-episodic 70–88 — §16.32) | MPTCP-BBR 90–93; single-path TCP-BBR 89.5–92.1 | **LOSS −21…−27%**, below even single-path kernel BBR — the §17.7 c8 WATCH externally confirmed. **Closed structurally 2026-08-06, §16.32: slow-path source is negative-margin at this asymmetry under every placement law measured (share↑ ⇒ goodput↓, monotone, both seeds); the legacy-1024 arm holds 0.874/0.871×Σ (88.6/87.6) — 1–4 Mbit under the kernel MPTCP bar, whose own slow-path banking is +3.1/−2.4; the remaining c8 gap ≡ the single-path c2 gap (§16.30)** |
| c2 realtime tails | p99 med 36–39 ms, 1000/1000 delivered | QUIC 55–342 ms; TCP 209–1407 ms + delivery cliffs (to 687/1000) | **WIN ×1.4–8.8 / ×5–38** |
| c3 realtime tails | p99 med 92–103 ms, 1000/1000 | QUIC 150–759 ms (worst reps 38–44 s); TCP 830–3878 ms (delivered to 525/1000) | **WIN ×1.5–41**; only delivery-complete arm |

Honest readings, in both directions:

- **Bulk, everywhere, the shipped default LOSES to the best-tuned
  competitor** — by ×5.5 on the clean path (the engine's ~190 Mbit
  service walls vs 915 Mbit of userspace QUIC on the same box: the wall
  is ours, not "userspace transport" physics), by 9–14% at the lossy
  singles (BBR-class stacks extract 18.6/92 where rp holds 16.1/79), by
  0–13% at symmetric dual (kernel MPTCP over BBR subflows is a solved
  aggregator), and by 21–27% at heterogeneous dual — where the shipped
  stack also sits below single-path kernel BBR on the fast path alone.
  Each loss names a lever: emission batching/GSO (c1 — **executed
  2026-07-27, §16.28: sender batching banks +10–16% (c1 210–216, gate
  `RWM_EMIT_BATCH`, no flip at the pre-registered ≥400 bar); GSO was
  already engaged and AEAD is noise; the residual c1 binder is the
  engine-receiver per-message service wall ~22–23k msgs/s ≈ 210–230
  Mbit/s per sink — ITSELF executed 2026-08-06, §16.35: the wall was
  one mis-scaled changepoint detector (22–26%/core per side), and the
  cadence fix (`RWM_EST_CADENCE`) + sender batching take the measured
  opt-in sink to 446–505 Mbit/s (~×2.2 the shipped c1 class); no flip
  (the c7 anchor-interaction clause), gap to quinn now ×1.8–2.0 at the
  opt-in ceiling**), the
  recovery-plane residual (c2/c3 — quinn-BBR's numbers are the measured
  bar for the same pipes — **executed 2026-07-27, §16.30: the gap is
  ACCOUNTED TO CLOSURE (framing/MTU tax ~4.3/0.95 + reactive over-fire
  ~2.7/1.7 Mbit; wire ≥98% utilized, engine non-binding, proactive FEC
  measured dead at singles); `RWM_RECOV_SP` banks +0.32/+0.35 ≫σ at sc3,
  no flip; successor levers named: window/inflight decoupling +
  MTU/payload scaling**), and the c8-aware pool law (now priced at
  ~+20 Mbit by an external referee — **executed to structural closure
  2026-08-06, §16.32: the "+20" decomposes as ~+15 legacy-pool class
  (0.874×Σ, reachable today via the pool WATCH) + the rest owned by the
  single-path c2 gap; slow-path source conversion itself is measured
  negative-margin at this cell under every placement law, matching the
  kernel referee's own +3.1/−2.4 slow-path banking**).
- **The Cubic-collapse findings that motivated wall #1 are confirmed on
  the reference stacks themselves**: quinn stock (Cubic) does 24–26 at
  c2 and 3.2–4.8 at c3 (same stack, CC swap to BBR = ×3.8); kernel
  Cubic 11/1.4–2.2; MPTCP-Cubic 23–38/11–17. "QUIC/TCP/MPTCP as
  commonly deployed" still collapse under bursty loss; the BBR arms are
  the strong competitors.
- **Realtime is the durable, decisive win — and it is a CLASS, not a
  multiplier.** rp's worst rep across all 32 realtime cells is 164 ms
  p99 with 1000/1000 delivered in every rep; kernel TCP's medians run
  0.2–3.9 s with delivery cliffs (down to 525/1000 still blocked at
  the harness window), and QUIC — which delivers everything — carries
  38–44 s worst-rep tails at c3 (the HoL cascade of a reliable ordered
  stream under GE bursts). The historic 12–48× crown reproduces vs
  kernel TCP at the medians (×5–38); vs QUIC the MEDIAN gap narrows to
  ×1.4–8.8 (quinn's c2-400B median is only ×1.5 rp's — recorded), but
  no competitor bounds its tail. The p50 cost of the rp tunnel is
  ~2.5 ms.
- **Fairness (reused, not re-run):** rp/BBR-under share 0.94–1.0
  against a Cubic flow at the lossy c2 cell; the clean-bottleneck
  contention blocker (share 0.02–0.24) stands unchanged (§16.19-era
  finding, goal-gate "Copa Competitive Mode + Cross-Traffic").

Position statement (replacing any older "beats X" phrasing where it
conflicts): raptorpath's measured value on real links today is (i)
bounded message tails + complete delivery under bursty loss —
realtime/messaging semantics no deployed reliable stream offers at
these cells — and (ii) loss-robust bulk far above every Cubic-family
deployment; it is NOT currently a faster bulk pipe than well-tuned
BBR-class singles or kernel MPTCP-BBR aggregation, and at c8 it is
measurably behind them. The bulk gaps are named, mechanism-attributed
levers (§17.6 additions), not mysteries.

---

## Appendix A: Summary of Key Formulas

```
   Channel (Section 2):
     e = p/(p+q)                           average loss rate           [probability]
     B = 1/q                               mean burst length           [symbols]
     P(burst ≥ t) = (1-q)^{t-1}            burst survival              [probability]

   Taper function (Section 4):
     τ*(t) = A* x (1-q)^t                  optimal taper function      [corrections/symbol]
     A* = r* x q                           taper amplitude (ρ=100%)    [corrections/symbol]

   Burst variance correction (Section 8.3):
     σ²_burst = 1 + 2(1-p-q)/(p+q)         burst variance inflation    [dimensionless]
     Var_GE(K) = W x e x (1-e) x σ²_burst  loss count variance         [symbols^2]

   Optimal correction rate (Section 8.4):
     r* = max(0, e/(1-e) + z_{delta/e} x sqrt(e x s2_burst / (W x (1-e))))  [ratio]
     z_{delta/e} = normal_quantile(1 - delta/e)  (r* -> 0 smoothly as delta -> e)
     With codec: replace e with e_hat (see Section 9.2)
     P_fec = Phi(sqrt(W) x (r(1-e)-e) / sqrt(e(1-e)(r+s2_burst)))      [probability]
     Exact P_fec: transfer-matrix DP over the GE chain (Section 8.7)   [probability]

   Codec overhead (Section 9.2):
     e_codec_eff = e_codec x (1-(1-e)^W)   weighted codec overhead     [probability]
     e_hat = e + e_codec_eff               effective loss rate         [probability]

   Bulk completion-exposure glide (Section 14.26):
     chi(T_rem) = Phi_bar((T_rem - 1.5 x SRTT) / sigma_arq)            [probability]
     sigma_arq = max(4 x RTTVAR, SRTT/4)                               [seconds]
     delta_bulk = e_hat + (delta_tail - e_hat) x chi,  delta_tail=0.05 [probability]
     (chi = 0 mid-stream / unknown T_rem -> delta_bulk = e_hat -> r* = 0)

   Inner-feedback repair floor (Section 14.28):
     L_stall = min(1.5 x SRTT, max(0.2, SRTT))                         [seconds]
     T_arq = L_stall / t_sym                                           [slots]
     C(r) = sum_m (1-q)^{m-1} q x P(Poisson(T_arq x r(1-e)/(1+r)) >= m) [probability]
     S(r) = e x q x T_arq x (1 - C(r))                                 [stall fraction]
     r_min = min{ r : S(r) <= theta },  theta = (SRTT/4) / L_stall     [ratio]
     rate = max(rate_glide, w x r_min),  w = inner-feedback weight     [ratio]
     (w = 0 everywhere by default: the L1 ablation refuted the premise
      post-14.27 — completion-neutral at C2, -28% at C3; opt-in knob)

   Three-variable optimization (Section 8.6):
     Taper cutoff: τ(t) = 0 for t > T_cut                (ρ<100%)
     Correction rate with cutoff: r = A x (1-(1-q)^{T_cut+1}) / q
     Given (δ, ρ) → r:  find T_cut from ρ, find A from δ, r = A(1-(1-q)^{T_cut+1})/q
     Given (r, ρ) → δ:  find T_cut from ρ, compute A from r, δ = e(1-P_fec)P_arq/ρ
     Given (r, δ) → ρ:  iterate T_cut until budget constraint met, ρ = recovery within T_cut

   Per-symbol delivery (Section 6.3):
     P(on-time)   = (1-e) + e x P_fec                                  [probability]
     P(late)      = e x (1-P_fec) x P_arq                              [probability]
     P(lost)      = e x (1-P_fec) x (1-P_arq) = 1-ρ                    [probability]
     P_arq = 1 - (1-rho) / (e x (1-P_fec))                             [probability]

   Recovery latency (Section 3.4):
     t_sym = symbol_size / throughput      symbol transmission time    [seconds]
     t_fec = m x (1+r) / (r x (1-e)) x t_sym   FEC recovery time       [seconds]
     t_recovery_i = P_fec_i x t_fec_i + (1-P_fec_i) x L_arq_i          [seconds]
     P_lost(t) = e / [e + (1-e) x P(RTT>t)]  loss confidence           [probability]
     P(RTT > t) = 1 - Phi((t - SRTT) / RTTVAR)                         [probability]
     L_actual = min(t_fec, retransmit arrival)                         [seconds]

   Retransmit buffer (Section 6.1):
     B_max = ceil(ln(0.0001) / ln(1-q))                                [symbols]
     buffer_max = source_rate x (RTT + B_max/(r*(1-e))*t_sym)          [symbols, rho=100%]
     buffer_max = source_rate x T_cut                                  [symbols, rho<100%]

   Per-slot decision (Section 5.4):
     P_retx = P_lost(t)                                                (time-based mixing)
     with probability P_retx:     send source retransmit               (immediate decode)
     with probability 1-P_retx:   send repair symbol                   (FEC, any loss)
     Optional refinement: P_retx = P_lost(t) x (1 - e_burst)
       for long-burst scenarios (Appendix C.6)

   Congestion control (Section 12):
     Copa: rate = 1 / (d_copa x dq)                                    [symbols/sec]
     dq = RTT_current - RTT_min                                        [seconds]
     source_rate = total_rate / (1 + r*)                               [symbols/sec]
     correction_rate = total_rate x r* / (1 + r*)                      [symbols/sec]

   Multi-path scheduling (Section 13):
     E_i = RTT_i/2 + e_i x t_recovery_i                                [seconds]
     B_eff_i = C_i / (1 + r_i)                                         [symbols/sec]
     e_combined = SUM(C_i x e_i) / SUM(C_i)                            [probability]
     deficit = SUM_{un-ACKed s}(e_s)                                   [expected corrections]
     cross-path: r = e_source / (1 - e_correction)                     [ratio]
     minimize: w_lat x SUM(x_i x E_i) + w_bw x SUM(x_i x r_i)
     P(cross-path retx) = P_lost(t, e_src)
     P(both paths fail) = e_src x e_retx                               [probability]
```

## Appendix B: Related Work

### ACK-Based vs NACK-Based Recovery

Our model uses ACK-based (positive acknowledgment) feedback, the same approach
that TCP has used successfully for over 40 years. In contrast, many real-time
protocols (e.g., RTCP NACK [RFC4585]) use negative acknowledgment where the
receiver explicitly reports losses.

**Why ACK-based is more robust for unicast:**
- ACKs confirm what works; NACKs report what failed. If a NACK is lost, the
  sender never learns about the loss. If an ACK is lost, the sender simply
  retransmits after a timeout (self-healing).
- SACK [RFC2018] provides the same information as NACKs (which symbols are
  missing) but derived from positive evidence (which symbols arrived).
- The sender can infer losses from SACK gaps without requiring the receiver
  to detect and report them.

NACK-based approaches remain useful for multicast (one sender, many receivers)
where ACK implosion is a concern [RFC4585]. For our unicast tunnel model,
ACK+SACK is strictly superior.

### Hybrid FEC-ARQ for Lossless Streaming

The closest prior work is Mehrotra, Li & Huang [Mehrotra2010], who solve the
same core problem: minimize delivery delay for lossless, in-order streaming
over a lossy channel using hybrid FEC+ARQ. Their key result: sometimes it is
optimal to **preempt original data packets with FEC packets** — delaying new
data to send proactive repair prevents an ARQ round-trip and reduces overall
latency. This maps directly to our taper function concept.

**Differences from our model:**
- They use a Markov Decision Process (MDP) formulation and compute the optimal
  policy numerically. We derive a closed-form taper function from the GE
  survival function, which is more directly implementable.
- Their model doesn't account for burst correlation in the margin term (our
  σ²_burst correction).
- They don't address multi-protocol traffic classes or interleaving.

The precursor paper [Mehrotra2009] establishes the basic hybrid FEC-ARQ
protocol framework.

### Streaming Codes over Gilbert-Elliott Channels

Vajha et al. [Vajha2020] provide the first **analytical** (not simulation-only)
bounds on block-erasure probability for streaming codes over GE channels.
Previously, streaming codes were designed for a simplified proxy channel (the
delay-constrained sliding window model from [Badr2017]) and then evaluated via
simulation on GE. Their upper and lower bounds could be used to verify our
P_fec predictions without simulation.

Very recent work [RLC_GE2025] analyzes Random Linear Codes (our RLC backend)
specifically over GE channels, providing analytical performance characterization
that directly applies to our system. A related result [Vajha2020b] formalizes
the sliding window approximation of the GE channel used in streaming code design.

The foundational work on delay-constrained burst erasure correction
[Martinian2004] established the theoretical framework that later streaming
code constructions build upon. Krishnan & Ramkumar [Krishnan2020] provide the
simplest rate-optimal streaming code construction (staggered diagonal embedding),
demonstrating that optimal streaming codes need not be complex. Tambur
[Rudow2023] is a production-quality implementation of streaming codes for
videoconferencing, demonstrating 26% fewer decode failures and 35% less
redundancy bandwidth — validating that streaming code theory translates to
practical gains.

### Tail Latency Optimization with Proactive FEC

CloudBurst [Zeng2021] uses proactive FEC over multipath to reduce p99 tail
latency by 60-75% in commodity datacenters. Different setting (datacenter LAN
vs our WAN/wireless focus) but the same core insight: proactive FEC-coded
packets spread across paths, recovered from the first arrivals. They use
rateless fountain codes; we use windowed RLC/streaming codes with a taper.

### Fork-Join Queues, Coded Queueing, and Resequencing Delay

Section 16's three-regime formulation grounds each multipath delivery
discipline in a known queueing-theoretic object.

**Fork-join queues.** Nelson & Tantawi [Nelson1988] give the classic
approximate analysis of fork/join synchronisation in parallel queues — the
mean response of a job forked to K servers grows with the *maximum* of the
branch times (harmonic growth for homogeneous exponential branches). This is
Section 16.1's regime (1): a delivery unit striped across paths completes at
the max over the paths it touches, and heterogeneity inflates E[max] well
past any single path's mean.

**Coded queueing / redundancy for latency.** Joshi, Liu & Soljanin
[Joshi2014] analyse content download from (n, k) coded storage as a fork-join
system where the job completes when ANY k of n branches finish — the k-th
order statistic replaces the maximum, trading storage/traffic redundancy for
latency. Joshi, Soljanin & Wornell [Joshi2017] generalise to full/partial
forking and quantify when redundancy reduces both mean latency and cost.
Section 16.1's regime (3) is the transport-layer analogue: rateless symbols
poured over N paths make completion the K(1+φ)-th order statistic of the
superposed arrival process, and the E[max] → interior-order-statistic gap is
exactly the coded-queueing gain, growing with branch heterogeneity.

**Resequencing delay.** Xia & Tse [Xia2003] analyse the resequencing buffer
of reliable protocols under out-of-order arrivals — packets delivered over
parallel channels wait for the in-order point to catch up. Section 16.1's
regime (2) (whole-unit path affinity + in-order release) is this queue at
block granularity: the delivery frontier is a running max over per-unit
completion times, which is why raptorpath's affinity scheduler and kernel
MPTCP (the same queue with unit = packet) measure identically at C8 (12.6
Mbit/s parity).

**In-order delivery of transport-layer coding.** Cloud, Leith & Médard
[Cloud2014] analyse the in-order delivery delay of a streaming code that
inserts coded packets into the stream — showing coding shrinks head-of-line
waits without abandoning in-order semantics. This is the direct antecedent
of Section 16.2's claim that in-order delivery is NOT the aggregation
bottleneck once repairs are fungible across the window: the frontier
advances on any sufficient subset, at the pooled arrival rate.

### Joint FEC/ARQ under Gilbert-Elliott

Razavi et al. [Razavi2008] develop adaptive heuristic algorithms that jointly
select FEC redundancy and ARQ persistence under GE channel conditions, applied
to video streaming. They search the joint parameter space numerically. Our
approach is more principled: the GE parameters directly determine the taper
shape, and the tail latency constraint determines the amplitude analytically.

### Water-Filling and Optimal Resource Allocation

The water-filling principle [Gallager1968] — allocating resources proportional
to channel quality — is foundational in information theory. Our taper function
applies this principle in the time domain: allocate correction density proportional
to the conditional loss probability (1-q)^t. While water-filling is well-known
for power allocation in OFDM, its application to correction symbol density matching
the GE burst survival function appears novel.

### What This Model Contributes

| Aspect | Prior work | This model |
|--------|-----------|------------|
| Optimization target | Throughput, avg delay, or weighted cost | Tail latency only (= tail loss under 100% reliability) |
| FEC/ARQ balance | MDP or heuristic search | Closed-form from GE params + tail constraint |
| Taper function | Not formalized | GE survival function τ(t) = A(1-q)^t |
| Burst correction | Not addressed | σ²_burst = 1+2(1-p-q)/(p+q) |
| Protocol hint | Separate FEC/ARQ tuning knobs | Single knob: tail latency target δ |
| Correction rate formula | Numerical | r* = ε/(1-ε) + z_δ√(ε·σ²_burst/(W(1-ε))) |
| Feedback mechanism | NACK-based or separate FEC/ARQ | Unified correction symbols with ACK+SACK |

## Appendix C: Model Extensions from Related Work

The following extensions are motivated by concrete results from related research
and represent improvements over the base model in Sections 1-11.

### C.1 Correction Symbol Preemption of Source Data [Mehrotra2010] (resolved)

Mehrotra & Li show via MDP that during detected bursts, the optimal policy
shifts from sending new source data to sending correction symbols. Our model
produces the same behavior through two mechanisms:

1. **P_lost(t) during bursts**: Since the ACK hasn't had time to return
   (RTT hasn't elapsed), P_lost stays at the base rate ε — almost all
   correction slots are FEC repair. This is exactly Mehrotra's recommendation:
   send repair (not retransmit) during bursts.

2. **P_lost(t) after bursts**: Once ACKs reveal gaps (after RTT), P_lost
   rises to ~1 for lost symbols — correction slots switch to retransmit.
   Immediate decode at receiver, targeted recovery.

3. **Source preemption via r***: When BOCD detects increased ε, the taper
   amplitude A* increases, and τ(t) naturally exceeds 1.0 at small t. This
   means more correction slots than source slots — source is effectively
   paused. No special τ > 1.0 rule needed; it emerges from the formula.

The P_lost model and Mehrotra's MDP agree on the optimal policy. The
difference is derivation: we arrive at the answer from a Bayesian posterior
(P_lost) and rate adaptation (r*), while Mehrotra uses dynamic programming.

### C.2 Information Debt for Exact P_fec [RLC_GE2025]

The base model uses a normal approximation for P_fec. The information debt
framework tracks the running deficit between received and needed symbols:

```
   I_d(t) = symbols_needed(t) - symbols_received(t)
```

Recovery occurs when I_d returns to 0. The slot error probability is:

```
   p_e = E{error_slots} / E{debt_cycle_length}
```

where error_slots counts time steps where I_d ≥ ζ (the maximum recoverable
debt, determined by window size and code rate).

**Key finding:** For systematic codes, successfully receiving a source symbol
can sometimes **reduce** decodability of earlier lost symbols through Gaussian
elimination interactions. Our normal approximation misses this effect.

**Extension:** Replace the normal-approximation P_fec with debt-tracking:

```
   Current model:  P_fec ≈ 1 - Φ(-z) where z depends on (e, W, σ²_burst)
   Extended model: P_fec = 1 - p_e where p_e from debt Markov chain
```

The debt Markov chain has state space {0, 1, ..., ζ} with transition
probabilities depending on GE state and repair density. This is computable
(finite-state Markov chain) though not as simple as the closed-form formula.

**Status:** The normal formula is used in the paper for analytical insight.
For implementation, the exact O(W²) transfer matrix computation (Section
8.7, implemented as `p_fec_exact`) provides the same precision as the debt
model. Both are finite-state Markov chain computations over the GE channel
— the debt model tracks decoder state, the transfer matrix tracks the
loss-minus-repair deficit. Either gives exact P_fec for implementation use.

### C.3 Analytical P_fec Bounds [Vajha2020]

Vajha et al. derive upper and lower bounds on block-erasure probability for
streaming codes over GE without simulation. Their bounds depend on:
- GE parameters (p, q)
- Code rate R = K/N
- Decoding delay constraint T

**Extension to verification (Section 11):**

```
   For our taper with rate r* and window W:
     Effective code rate R = 1/(1+r*)
     Delay constraint T = W

   Vajha lower bound ≤ P(erasure) ≤ Vajha upper bound

   If our predicted P_fec falls within these bounds: ✓ model validated
   If not: normal approximation is inadequate → use debt model (C.2)
```

This gives us an analytical verification path that complements simulation.
Every verification method strengthens confidence in the model — simulation
validates end-to-end behavior, Vajha bounds validate the P_fec formula
analytically, and the exact O(W²) computation validates the normal
approximation numerically.

### C.4 Multipath Diversity Gain [Zeng2021]

CloudBurst sends FEC-coded packets across multiple paths and recovers from
whichever path delivers first. This is a **diversity gain**: the same repair
on two independent paths has survival probability `1 - ε₁ε₂` instead of
`1 - ε₁`.

**Extension:** For multipath, the effective repair arrival rate is higher
than single-path. With M independent paths of loss rates ε₁...εₘ:

```
   P(repair arrives on at least one path) = 1 - Π(e_i)

   Single path:  P(arrive) = 1-e
   Dual path:    P(arrive) = 1-e^2 ≈ 1 for small e
```

This means the multipath taper can use a **lower amplitude** A than single-path
for the same P_fec target. The optimal multipath taper:

```
   τ_multi(t) = A_multi x (1-q)^t

   where A_multi = A_single x (1-e) / (1-Π(e_i))
```

For two paths with 5% loss each: A_multi = A_single × 0.95/0.9975 ≈ 0.95 × A_single.
Modest gain for similar paths, but significant when paths have different
characteristics (one lossy WiFi, one reliable Ethernet).

**Not applicable to our model.** CloudBurst duplicates the same repair symbol
across paths. Our model uses shared-buffer cross-path retransmit instead
(Section 13.10): each path generates its own corrections, and the shared
retransmit buffer enables cross-path recovery. The diversity gain
P(both fail) = ε_A × ε_B is already captured in Section 13.10 without
repair duplication.

### C.5 DCSW Worst-Case Taper Floor

The Delay-Constrained Sliding Window model [Badr2017], [Fong2019] provides
a **hard guarantee**: within any window of W symbols, at most B consecutive
erasures or N random erasures can occur, and recovery must happen within
delay T.

The streaming capacity for this model is C(T,B) = T/(T+B) [Badr2017].

**Extension:** Regardless of the probabilistic taper, enforce a minimum
correction density that survives the worst-case DCSW pattern:

```
   τ_floor = B / W                    (enough to survive one full burst per window)

   τ*(t) = max(A x (1-q)^t, τ_floor)  (probabilistic taper with hard floor)
```

The floor ensures that even if the GE estimator underestimates burst length,
we always have enough correction to survive at least B consecutive erasures.

```
  Correction
  density
  τ(t)
    |
  A +--\
    |   \
    |    \
    |     \
  --+------\------------------------------ τ_floor = B/W
    |       (taper hits floor,
    |        floor continues indefinitely)
    +-------------------------------------- time offset t
```

**Not applicable to our model.** The DCSW floor is a hard guarantee from a
worst-case adversarial channel model. Our model already handles burst
protection through multiple mechanisms: (1) the taper's front-loaded density
exceeds B/W for any reasonable r*, (2) the retransmit buffer holds burst-lost
symbols for ARQ recovery after detection, (3) cross-path diversity on
multipath, and (4) backpressure guarantees delivery for ρ=100%. Even if a
burst overwhelms the taper entirely, ARQ kicks in — the retransmit buffer
IS the hard guarantee. The DCSW floor is redundant.

**Corrected total correction rate with floor (for reference):**

```
   r* = max(A/q, τ_floor x W) / W

   In practice: A/q dominates when loss is high (large A).
   Floor dominates when loss is low but bursts are long (large B, small e).
```

### C.6 Burst-Aware Channel Discount (considered refinement)

The per-slot decision can optionally include a channel-state discount:

```
  P_retx = P_lost(t) x (1 - e_burst)
```

where e_burst is a fast-reacting estimate of current channel loss (from GE
state or fast EWMA). This suppresses retransmit during ongoing bursts, where
repair is more flexible (covers any lost symbol).

Two separate loss estimates serve different purposes:

- **ε** (long-run, from BOCD): Bayesian prior for per-symbol loss confidence
  in P_lost
- **ε_burst** (fast, from GE state): current channel condition discount

This was considered for the primary model but deemed unnecessary because:

1. P_lost(t) alone handles the primary FEC/ARQ regulation via timing
2. Before burst detection (t < RTT): P_lost is already low → mostly repair
3. After burst detection (t > RTT): burst is usually over → ε_burst has dropped
4. The improvement is only significant for long bursts still ongoing at
   detection time (~1ms improvement)

The source/correction ratio was also considered for fast burst adjustment
(two-speed taper: A_effective = A_baseline x (1 + burst_boost)), but the
existing BOCD rate adaptation handles this with negligible delay.

Preserved here for future reference if extreme burst scenarios require it.

### C.7 Summary of Extensions

| Extension | From | Effect on correction rate | Effect on P_fec accuracy |
|-----------|------|-------------------------|--------------------------|
| Correction preemption | [Mehrotra2010] | Reduces delay during bursts | Improves burst recovery |
| Information debt | [RLC_GE2025] | More precise | Exact (Markov chain) |
| Analytical bounds | [Vajha2020] | Verification only | Bounds, not point estimate |
| Multipath diversity | [Zeng2021] | Reduces A_multi | Higher for same budget |
| DCSW taper floor | [Badr2017] | Hard minimum guarantee | Worst-case protection |
| Burst-aware discount | C.6 | Modest improvement for long bursts | Suppresses retransmit during bursts |

## Appendix D: Open Questions

1. **Finite window truncation (resolved):** The taper is infinite-tailed but
   the encoder window has finite size W. After eviction, FEC repair is no
   longer possible — but the retransmit buffer (Section 5.3) still holds the
   exact source symbol for ARQ retransmission. The truncation error (1-q)^W
   only affects the FEC component; the unified model's ARQ fallback covers it.

2. **Multi-path (resolved):** Each path has its own taper, Copa, and GE
   estimator (Section 13). Unified stream per path preserves interleaving.
   Scheduler adjusts per-path source/correction ratio (latency mode only).
   Global correction deficit tracks outstanding corrections. Cross-path
   retransmit via shared buffer. Burst protection during scheduler ratio
   adjustment is largely resolved: P_lost timing + BOCD adapts within ~1ms
   (Section 13.7). Two-speed taper is an optional refinement for extreme
   scenarios (Appendix C.6).

3. **Interaction with congestion control (resolved):** Copa [Copa2018] controls
   total rate, taper controls source/correction split (Section 12). When
   source_rate = total_rate/(1+r*) drops below the app's minimum, the system
   signals back-pressure. Copa is preferred over BBR: no ProbeRTT, no FEC
   protection gaps, simpler formula, taper-compatible.

4. **Normal approximation validity (resolved).** The normal approximation
   uses the EXACT mean (We) and EXACT variance (We(1-e)s2_burst) from the
   GE model — only the distribution SHAPE is approximated as a bell curve.
   For W >> B (all practical scenarios: W=50, B=2-3), the CLT makes this
   accurate. For edge cases (W close to B), the exact GE tail probability is
   computable via transfer matrix dynamic programming in O(W^2) — trivially
   cheap (2500 operations for W=50). The normal formula provides analytical
   insight (clean, closed-form); the exact computation is recommended for
   implementation and validation. IMPLEMENTED: Section 8.7 specifies the
   recursion; `p_fec_exact` / `compute_r_star_exact` in raptorpath-math
   implement it, verified against Monte Carlo and an independent-Binomial
   reference.

5. **Optimal retransmit timing (resolved):** Replaced by the P_lost(t) model
   (Section 3.4). No hard timeout — the repair/retransmit mix is determined
   probabilistically by P_lost(t) = ε / [ε + (1-ε) × P(RTT > t)]. This
   smoothly transitions from repair to retransmit as confidence in loss grows,
   naturally handles proactive retransmit at high loss rates, and minimizes
   expected waste at E[waste] = P_lost × (1 - P_lost).

### Remaining Open Points (full audit)

The following were identified by systematic review of the paper. Rated by
difficulty: LOW = can be resolved with a sentence or formula, MEDIUM = needs
a paragraph or derivation, HIGH = needs new analysis or algorithm design.

**Core model gaps (critical):**

6. **RTT distribution for P_lost not specified (resolved).** (Section 3.4) [LOW]
   P_lost uses P(RTT > t) but never specifies the distribution. Assuming
   normal with SRTT and RTTVAR gives P(RTT > t) = 1 - Phi((t-SRTT)/RTTVAR).
   Needs one sentence.

7. **P_arq never defined (resolved).** (Section 6.3, 8.6, Appendix A) [MEDIUM]
   Derived from reliability target:
   P_arq = 1 - (1-rho)/(e x (1-P_fec)). See Section 6.3.

8. **P_fec: two contradictory models (resolved).** (Appendix E vs Section 8.2) [MEDIUM]
   Appendix E sections marked as preliminary (Poisson,
   superseded). Section 8.2 marked as canonical (Binomial/Normal). Progression
   preserved to show why per-symbol model fails.

9. **Poisson model error not characterized (resolved).** (Appendix E.4) [LOW]
   The transition from Poisson (Appendix E.3) to corrected (Section 8.2) says "too
   pessimistic" but doesn't explain why. The error: per-symbol accounting
   counts only the lost symbol's own taper (~r corrections) instead of all
   window-covering repairs (~rW) — a factor-of-W undercount. See E.4.

10. **r* formula: which version is canonical (resolved)?** (Section 8.4 vs 9.2) [LOW]
    Section 8.4 uses raw ε. Section 9.2 adds codec overhead ε_codec.
    The canonical formula should be r* with ε_hat = ε + ε_codec × P(decoder).
    Needs a clarifying note in Section 8.4.

11. **T_cut computation algorithm (resolved).** (Section 8.6) [MEDIUM]
    Binary search on T_cut. P(recovered) is monotone in T_cut. Algorithm
    added to Mode 1. Converges in ~20 iterations.

12. **Mode 3 convergence (resolved).** (Section 8.6) [MEDIUM]
    Binary search on T_cut with monotonicity in ρ and r. Algorithm added
    to Mode 3. Convergence guaranteed.

13. **t_recovery_i undefined in multipath context (resolved).** (Section 13.5) [LOW]
    E_i = RTT_i/2 + ε_i × t_recovery_i but t_recovery_i not defined for
    multipath. It should reference Section 3.4: t_recovery_i = P_fec_i ×
    t_fec_i + (1-P_fec_i) × L_arq_i. One sentence.

**Implementation details (medium priority):**

14. **Retransmit buffer saturation (resolved).** (Section 6.1) [LOW]
    Resolved: dual-mechanism model — T_cut age eviction + buffer_max size
    backpressure. Buffer is NOT bounded by encoder window. See Section 6.1.

15. **SACK timing and format (resolved).** (Section 6.2) [MEDIUM]
    Per-packet ACK (0.6% overhead). 5 fields: cumulative_ack, sack_ranges
    (cumulative within T_cut), echo_timestamp, jitter_us (u32), cumulative_received.
    Gap pruning at T_cut. ACK loss self-healing via cumulative semantics.

16. **GE parameter initialization (resolved).** (Section 7.5) [LOW]
    Initial state: all counters = 0, start in Good state, use the Beta
    prior (weak uniform) until enough transitions observed. GE is_valid()
    already gates usage (existing code). One sentence.

17. **Copa parameter d tuning (resolved).** (Section 12.4) [MEDIUM]
    Renamed to d_copa to avoid confusion with tail latency delta.
    Default 0.5 sufficient. Adaptive d_copa (Copa+) is a negligible
    optimization (~1ms gain).

18. **Copa min_rtt refresh (resolved).** (Section 12.4) [LOW]
    Copa's natural oscillation refreshes min_rtt. Sliding window of 10s
    (same as BBR). If min_rtt seems stale (RTT consistently above by 2x),
    force a brief rate reduction. One paragraph.

19. **Back-pressure signaling (resolved).** (Section 12.7) [MEDIUM]
    Blocking write() like TCP — sender stops reading from source when
    buffer_max reached. Optional stats API for advanced applications.
    Implementation-specific, not part of core model.

20. **Scheduler burst protection floor (resolved).** (Section 13.7) [LOW]
    No floor needed. Global correction deficit + cross-path diversity
    handle burst protection when the scheduler reduces a path's correction
    ratio. The three options and floor discussion were removed.

21. **Cross-path retransmit path selection (resolved).** (Section 13.10) [LOW]
    No explicit path selection needed. Cross-path retransmit emerges from
    shared buffer + per-path P_lost. Paths with spare capacity naturally
    pull retransmits (work-stealing). Weighted path preference for
    latency-sensitive traffic noted as potential refinement.

**Minor (notation, justification):**

22. **z_d vs z_δ inconsistency (resolved).** (Section 8.4) [LOW]
    Line uses "z_d" and "d" where it should be "z_δ" and "δ".

23. **Beta decay 0.995 not justified (resolved).** (Section 7.3) [LOW]
    Standard value for slow-forgetting Bayesian update. Half-life ≈ 138
    samples. Sensitivity is low — values 0.99-0.999 give similar results.

24. **BOCD "5-15 batches" — what's a batch (resolved)?** (Section 4.5) [LOW]
    A batch = one ACK feedback cycle. With per-batch ACKs at ~10-100ms
    intervals, 5-15 batches = 50ms-1.5s adaptation time.

25. **GE simplified model error bounds (resolved).** (Section 2.1) [MEDIUM]
    h_G=0, h_B=1 is exact for packet-level erasure (UDP checksums ensure
    no partial delivery). Not an approximation for our use case.

26. **P_arq missing from Appendix A (resolved).** [LOW]
    Added P_arq formula to Appendix A.

---

## Appendix E: Preliminary Poisson Model (superseded)

These sections were originally Section 6.2-6.5. They present a Poisson model
that gives unrealistic results. The corrected Binomial/Normal model is in
Section 8.2. This appendix is preserved for pedagogical value — it shows why
the simpler per-symbol approach fails.

### E.1 FEC Recovery Probability (preliminary — superseded by Section 8.2)

*Note: Sections E.1-E.4 develop a preliminary Poisson model that gives
unrealistic results. The model is superseded by the corrected Binomial
model in Section 8.2. The preliminary derivation is kept to show why the
simpler per-symbol approach fails and motivate the per-window model.*

Consider a symbol lost at position 0. Correction symbols generated at offsets
t = 0, 1, 2, ... each have:
- Probability τ(t) of being generated (fractional: may or may not generate one)
- Probability (1-ε) of surviving the channel

The expected number of correction symbols covering the lost position that arrive:

```
   R(A, q) = Σ_{t=0}^{W-1} τ(t) x (1-e)
           = A x (1-e) x Σ_{t=0}^{W-1} (1-q)^t
           = A x (1-e) x (1 - (1-q)^W) / q
```

For large W (window much larger than burst length): (1-q)^W ≈ 0, so:

```
   R(A, q) ≈ A x (1-e) / q = r x (1-e)
```

**Recovery model:** The number of useful correction symbols arriving is approximately
Poisson(R). For FEC recovery, we need at least 1 repair symbol (plus codec
overhead, see Section 9). Simplified to needing at least 1:

```
   P_fec(A, q) = 1 - P(Poisson(R) = 0) = 1 - e^{-R}
```

### E.2 Solving for A* (preliminary)

Substituting into the constraint:

```
   e x (1 - P_fec) ≤ δ
   e x e^{-R} ≤ δ
   e^{-R} ≤ δ/e
   -R ≤ ln(δ/e)
   R ≥ ln(e/δ)                    (note: e > δ, so ln(e/δ) > 0)
```

Using R ≈ A × (1-ε) / q:

```
   A x (1-e) / q ≥ ln(e/δ)

   A* = q x ln(e/δ) / (1-e)

   r* = A*/q = ln(e/δ) / (1-e)
```

### E.3 The Optimal Correction Rate Formula (preliminary — see Section 8.4)

```
  ┌───────────────────────────────────────────┐
  │                                           │
  │   r* = ln(e/δ) / (1-e)                    │
  │                                           │
  │   where:                                  │
  │     e = average loss rate (from BOCD)     │
  │     δ = tail latency target               │
  │     r* = optimal correction rate          │
  │                                           │
  └───────────────────────────────────────────┘
```

**Properties:**
- r* depends only on ε and δ, not on q (burst length doesn't affect the TOTAL
  correction budget, only its distribution over time via the taper shape)
- As δ → 0 (tighter tail): r* → ∞ (need infinite FEC for zero ARQ events)
- As δ → ε (loose tail = every loss goes to ARQ): r* → 0 (no FEC needed)
- As ε → 0 (perfect channel): r* → 0 (no correction needed)

The **taper amplitude** is:
```
   A* = r* x q = q x ln(e/δ) / (1-e)
```

And the **complete taper function** is:
```
   τ*(t) = A* x (1-q)^t = q x ln(e/δ) / (1-e) x (1-q)^t
```

### E.4 Comparison with Information-Theoretic Minimum

The IT minimum (Shannon limit for the erasure channel [Shannon1948]) is:

```
   r_IT = e / (1-e)
```

**Why ε/(1-ε), not just ε?** Correction symbols are also subject to channel
loss. A correction sent to replace a lost source might itself be lost, needing
another correction, which might also be lost:

```
   e + e^2 + e^3 + ... = e / (1-e)
```

At 30% loss: need 43% overhead (not 30%), because 30% of corrections are lost
too. The (1-ε) denominator IS this geometric series. See also Section 13.4.

Our optimal rate:

```
   r* = ln(e/δ) / (1-e) = r_IT x ln(e/δ) / e = r_IT x ln(1/δ)/e + r_IT x ln(e)/e
```

The ratio r*/r_IT = ln(ε/δ)/ε. This is the **unavoidable overhead** of
targeting a tail latency of δ — it's the price of proactive protection.

For ε = 0.025 (WiFi), δ = 1e-4 (Realtime):
```
   r*/r_IT = ln(0.025/0.0001) / 0.025 = ln(250) / 0.025 = 5.52/0.025 = 221
```

This seems very high. Let's check: r_IT = 0.025/0.975 = 0.0256, so r* = 0.0256 × 221 = 5.66.
That's 566% overhead — clearly too much.

**The issue:** The dominant error is not Poisson-vs-Binomial — it is
per-symbol accounting. E.1 counts only the corrections attributed to the
lost symbol by its own taper, Σ_t τ(t) = r per source symbol, so the
expected help is R ≈ r(1-e) < 1 and the model concludes that even a single
loss usually cannot be recovered. But every repair is a combination of the
ENTIRE window (Section 3.2): while the lost symbol remains in the window,
roughly rW repairs are generated that all cover it. The corrected model in
Section 8.2 counts repairs per window — Binomial(rW, 1-e) — and compares
them against the per-window loss count K. (The secondary Poisson-vs-Binomial
distinction matters far less than this factor-of-W undercount.)

---

## References

### Channel Models

- **[Gilbert1960]** E.N. Gilbert, "Capacity of a burst-noise channel,"
  *Bell System Technical Journal*, vol. 39, pp. 1253-1265, 1960.
  The original two-state channel model.

- **[Elliott1963]** E.O. Elliott, "Estimates of error rates for codes on
  burst-noise channels," *Bell System Technical Journal*, vol. 42, pp. 1977-1997, 1963.
  Extension of Gilbert's model with per-state loss probabilities.

### Estimation and Detection

- **[Adams2007]** R.P. Adams, D.J.C. MacKay, "Bayesian Online Changepoint
  Detection," arXiv:0710.3742, 2007.
  The BOCD algorithm used in Section 7.4 for regime-aware loss estimation.
  Maintains run-length distribution with O(r_max) per update.

- **[RFC3550]** H. Schulzrinne, S. Casner, R. Frederick, V. Jacobson,
  "RTP: A Transport Protocol for Real-Time Applications," IETF RFC 3550, 2003.
  Appendix A.8 defines the interarrival jitter calculation used in our estimator.

- **[RFC6298]** V. Paxson, M. Allman, J. Chu, M. Sargent, "Computing TCP's
  Retransmission Timer," IETF RFC 6298, 2011.
  Standard for TCP RTO computation using SRTT and RTTVAR.

- **[Abramowitz1964]** M. Abramowitz, I.A. Stegun, *Handbook of Mathematical
  Functions*, National Bureau of Standards, 1964.
  Rational approximation for the standard normal quantile (Section 26.2.23),
  used in our Beta quantile and z_δ computation.

### FEC Codes

- **[RFC6330]** M. Luby, A. Shokrollahi, M. Watson, T. Stockhammer, L. Minder,
  "RaptorQ Forward Error Correction Scheme," IETF RFC 6330, 2012.
  Fountain code with ~1% decode overhead. Our RaptorQ backend.

- **[RFC8681]** V. Roca, B. Teibi, "Sliding Window Random Linear Code (RLC)
  Forward Erasure Correction (FEC) Schemes," IETF RFC 8681, 2020.
  Window-mode FEC over GF(2^8). Our RLC backend.

- **[Yu2025]** S. Yu, J. Yang, Q. Meng, L. Xu, "METTLE: Streaming Codes
  Based on SC-MET-LDGM," arXiv:2602.10020, 2025.
  Sparse random matrix streaming code. ~15% decode overhead at small windows.

### Streaming Codes Theory

- **[Martinian2004]** E. Martinian, C.-E.W. Sundberg, "Burst erasure correction
  codes with low decoding delay," *IEEE Trans. Information Theory*, 2004.
  Foundational work on delay-constrained erasure correction.

- **[Badr2017]** A. Badr, P. Patil, A. Tan, A. Dey, "Layered Constructions for
  Low-Delay Streaming Codes," *IEEE Trans. Information Theory*, 2017,
  arXiv:1308.3827.
  Proves streaming capacity C(T,B) = T/(T+B). Layered burst+random construction.

- **[Fong2019]** S.L. Fong, A. Khisti, B. Li, A. Tan, "Optimal Streaming
  Codes for Channels with Burst and Arbitrary Erasures," *IEEE Trans. Information
  Theory*, vol. 65 no. 7, 2019, arXiv:1801.04241.
  Generalizes to adversarial model: C(T,B,N) = (T-N+1)/(T-N+1+B).

- **[Krishnan2020]** M.N. Krishnan, D. Ramkumar, "Simple Streaming Codes for
  Reliable, Low-Latency Communication," *IEEE Comm. Letters*, vol. 24 no. 2, 2020.
  Staggered diagonal embedding (SDE): simplest rate-optimal construction.

- **[Rudow2023]** M. Rudow et al., "Tambur: Efficient loss recovery for
  videoconferencing via streaming codes," NSDI, 2023.
  Production implementation with ML-based rate adaptation.
  github.com/Thesys-lab/tambur

### Multipath and Scheduling

- **[Sarwar2013]** G. Sarwar, R. Boreli, E. Lochin, A. Mifdaoui, G. Smith,
  "Mitigating Receiver's Buffer Blocking by Delay Aware Packet Scheduling in
  Multipath Data Transfer," 3rd Intl. Workshop on Protocols and Applications
  with Multi-Homing Support (PAMS), IEEE WAINA, Barcelona, 2013.
  Origin of DAPS: schedule each packet by its expected arrival time so that
  deliberately out-of-order sends arrive in order across RTT-skewed paths,
  cutting receiver-buffer (head-of-line) blocking.

- **[Kuhn2014]** N. Kuhn, E. Lochin, A. Mifdaoui, G. Sarwar, O. Mehani,
  R. Boreli, "DAPS: Intelligent Delay-Aware Packet Scheduling For Multipath
  Transport," IEEE ICC, 2014.
  Extends DAPS with an analytical model of maximum receiver-buffer blocking
  time; ns-2 evaluation vs CMT-SCTP.

- **[Ferlin2016]** S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, "BLEST:
  Blocking Estimation-based MPTCP Scheduler for Heterogeneous Networks,"
  IFIP Networking, 2016, pp. 431-439.
  Estimates whether sending on a slow subflow would cause head-of-line
  blocking when the fast subflow reopens, and skips it if so.

- **[Lim2017]** Y. Lim, E.M. Nahum, D. Towsley, R.J. Gibbens, "ECF: An MPTCP
  Path Scheduler to Manage Heterogeneous Paths," ACM CoNEXT, 2017.
  Earliest Completion First: allocate each packet to the subflow that will
  complete its delivery soonest, using more than RTT alone; better path
  utilization than minRTT/BLEST under heterogeneity.

- **[Xia2003]** Y. Xia, D.N.C. Tse, "Analysis on Packet Resequencing for
  Reliable Network Protocols," *IEEE INFOCOM*, San Francisco, 2003,
  pp. 990-1000.
  Resequencing-buffer delay of reliable protocols under out-of-order
  arrivals — the queue behind Section 16.1's regime (2) bound.

- **[Cloud2014]** J. Cloud, D. Leith, M. Médard, "In-Order Delivery Delay of
  Transport Layer Coding," arXiv:1408.1440, 2014.
  Expected in-order delivery delay (and variance) when coded packets are
  inserted into a reliable stream — coding shrinks head-of-line waits
  without dropping in-order semantics (Section 16.2).

### Fork-Join and Coded Queueing

- **[Nelson1988]** R. Nelson, A.N. Tantawi, "Approximate Analysis of
  Fork/Join Synchronization in Parallel Queues," *IEEE Trans. Computers*,
  vol. 37, no. 6, pp. 739-743, 1988.
  Classic fork-join response-time analysis: completion is the max over
  parallel branches (Section 16.1 regime (1)).

- **[Joshi2014]** G. Joshi, Y. Liu, E. Soljanin, "On the Delay-Storage
  Trade-off in Content Download from Coded Distributed Storage Systems,"
  *IEEE JSAC*, vol. 32, no. 5, May 2014.
  (n, k) fork-join: download completes on ANY k of n coded branches — the
  k-th order statistic replaces the fork-join max (Section 16.1 regime (3)).

- **[Joshi2017]** G. Joshi, E. Soljanin, G.W. Wornell, "Efficient Redundancy
  Techniques for Latency Reduction in Cloud Systems," *ACM ToMPECS*,
  vol. 2, no. 2, 2017. arXiv:1508.03599.
  When and how much redundancy reduces latency and cost in (n, k) fork-join
  service; quantifies the E[max] vs order-statistic gap Section 16.1 uses.

- **[Facenda2022]** T. Facenda, M.N. Krishnan et al., "Error-correcting codes
  for low latency streaming over multiple link relay networks,"
  arXiv:2201.06609, 2022.
  Introduces delay spectrum concept for per-link rate allocation.

### Hybrid FEC-ARQ

- **[Mehrotra2010]** S. Mehrotra, J. Li, Y. Huang, "Optimizing FEC Transmission
  Strategy for Minimizing Delay in Lossless Sequential Streaming," *IEEE Trans.
  Multimedia*, 2010. MSR-TR-2010-134.
  Closest prior work: MDP-based optimization of when to send FEC vs data for
  lossless streaming. Key insight: preempting data with FEC can reduce latency.

- **[Mehrotra2009]** S. Mehrotra, J. Li, "A hybrid FEC-ARQ protocol for
  low-delay lossless sequential data streaming," *IEEE MMSP*, 2009.
  Precursor establishing the hybrid FEC-ARQ protocol framework.

- **[Razavi2008]** R. Razavi et al., "Performance Evaluation of Joint FEC and
  ARQ Optimization Heuristic Algorithms under Gilbert-Elliot Wireless Channel,"
  *IEEE CCNC*, 2008.
  Adaptive joint FEC/ARQ parameter search under GE channels for video.

### Streaming Code Analysis over GE

- **[Vajha2020]** M. Vajha, V. Ramkumar, M. Jhamtani, P.V. Kumar, "On the
  Performance Analysis of Streaming Codes over the Gilbert-Elliott Channel,"
  arXiv:2005.06921, 2020. *ITW 2021*.
  First analytical bounds on block-erasure probability for streaming codes
  over GE channels, replacing simulation-only evaluation.

- **[RLC_GE2025]** "On the Analysis of Random Linear Streaming Codes in
  Stochastic Channels," arXiv:2509.01894, 2025.
  Analytical performance of RLC over GE channels — directly applicable to
  our RLC backend.

### Tail Latency

- **[Zeng2021]** G. Zeng, L. Chen, B. Yi, K. Chen, "Optimizing Tail Latency
  in Commodity Datacenters using Forward Error Correction,"
  arXiv:2110.15157, 2021. (CloudBurst)
  Proactive FEC over multipath reduces p99 latency by 60-75% in datacenters.

### Congestion Control

- **[Jacobson1988]** V. Jacobson, M.J. Karels, "Congestion Avoidance and
  Control," ACM SIGCOMM, 1988. (ACM SIGCOMM Comput. Commun. Rev. 18(4),
  pp. 314-329.)
  Introduced slow-start and congestion avoidance for TCP. Established the
  bandwidth-delay product — bottleneck bandwidth × round-trip delay — as the
  largest sensible in-flight window (cwnd ≈ BDP), the foundation of the
  Section 12 anchor.

- **[Copa2018]** V. Arun, H. Balakrishnan, "Copa: Practical Delay-Based
  Congestion Control for the Internet," NSDI 2018.
  Delay-based CC with natural queue draining (no ProbeRTT). Rate formula:
  rate = 1/(δ × dq). Recommended for raptorpath (Section 12).

### Information Theory

- **[Shannon1948]** C.E. Shannon, "A mathematical theory of communication,"
  *Bell System Technical Journal*, vol. 27, pp. 379-423, 623-656, 1948.
  The erasure channel capacity result C = 1-ε gives the IT minimum repair
  rate r_IT = ε/(1-ε) used throughout this paper.

- **[Gallager1968]** R.G. Gallager, *Information Theory and Reliable
  Communication*, John Wiley & Sons, 1968.
  Water-filling theorem for optimal resource allocation across channels.

### TCP SACK

- **[RFC2018]** M. Mathis, J. Mahdavi, S. Floyd, A. Romanow, "TCP Selective
  Acknowledgment Options," IETF RFC 2018, 1996.
  Defines SACK for TCP: receiver reports non-contiguous blocks of received
  data, allowing sender to infer exactly which segments were lost. Our
  ACK+SACK feedback mechanism adapts this proven approach.

### ECN and Multicast Feedback

- **[RFC3168]** K. Ramakrishnan, S. Floyd, D. Black, "The Addition of Explicit
  Congestion Notification (ECN) to IP," IETF RFC 3168, 2001.
  ECN mechanism for router-signaled congestion without packet drops.

- **[RFC4585]** J. Ott, S. Wenger, N. Sato, C. Burmeister, J. Rey, "Extended
  RTP Profile for RTCP-Based Feedback," IETF RFC 4585, 2006.
  RTCP NACK-based feedback for RTP multicast — the multicast feedback
  mechanism where ACK implosion is relevant.

### Sliding Window Channel Models

- **[Vajha2020b]** M. Vajha, V. Ramkumar, P.V. Kumar, "On Sliding Window
  Approximation of Gilbert-Elliott Channel for Delay Constrained Setting,"
  arXiv:2005.06914, 2020.
  Formalizes the DCSW-to-GE approximation used in streaming code design.
