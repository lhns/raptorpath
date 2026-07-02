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
   - [8.5 Worked Examples](#85-worked-examples)
   - [8.6 Three-Variable Optimization](#86-three-variable-optimization)
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
    - [12.6 ECN as Opportunistic Enhancement](#126-ecn-as-opportunistic-enhancement)
    - [12.7 Application Back-Pressure](#127-application-back-pressure)
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
per-symbol constraint would be P(repairs < K) ≤ δ/e, i.e. z_{δ/e} in the
margin. We deliberately impose the STRICTER per-window constraint
P(repairs < K) ≤ δ, for two reasons: (1) conditioning on "this symbol was
lost" size-biases K upward — a window known to contain a loss has more
losses than average — which the unconditional P(C ≥ K) does not capture;
the extra strictness compensates. (2) It keeps z_δ independent of e. The
cost is modest over-provisioning: at WiFi (e = 0.025, δ = 1e-4), z_δ = 3.72
vs z_{δ/e} = 2.65. Treat the resulting r* as conservative by construction.

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

We compute σ²_burst directly from the GE estimator's p̂ and q̂.

**Known limitation — repair survival is treated as i.i.d.** The model
inflates the variance of the loss count K by σ²_burst but keeps the repair
count C ~ Binomial(rW, 1-ε) with independent survival. Repairs cross the
same GE channel: a burst that inflates K simultaneously kills interleaved
repairs, so Var(C) is also burst-inflated and Cov(K, C) < 0. Both effects
widen Var(K - C) beyond what the formula assumes, making P_fec optimistic
in exactly the bursty regime σ²_burst is meant to protect. Monte Carlo
validation shows the normal approximation diverging by up to ~12% on
high-loss/long-burst channels (LTE-like). For implementation-grade
precision, use the exact O(W²) transfer-matrix computation (Appendix D,
item 4), which captures loss/repair correlation exactly.

### 8.4 The Corrected Optimal Correction Rate

```
  r* = e/(1-e) + z_delta x sqrt(e x s2_burst / (W x (1-e)))
       '--v--'   '------------------v---------------------'
    IT minimum                 tail margin
                     (accounts for burst correlation)

  s2_burst = 1 + 2(1-p-q)/(p+q)

  z_delta = standard normal quantile for (1-delta)
```

**Properties:**
- IT minimum ε/(1-ε) is the dominant term
- Tail margin scales as 1/√W — larger windows need proportionally less margin
- z_δ controls the margin: tighter δ → larger z_δ → more margin
- σ²_burst amplifies the margin for bursty channels (large for small p+q)

**Note:** This formula uses the raw loss rate e. For the canonical production
formula including codec overhead, replace e with e_hat = e + e_codec x
(1-(1-e)^W) from Section 9.2. The codec-adjusted version accounts for decoder
invocation probability on systematic codes.

### 8.5 Worked Examples

Using z_δ values: z(1e-2) = 2.33, z(1e-4) = 3.72, z(1e-6) = 4.75

The margin term is: `z_δ × √(ε × σ²_burst / (W × (1-ε)))`

**DC (ε=0.001, W=50, σ²_burst=3.0):**
```
   Bulk (δ=1e-2):     r* = 0.001 + 2.33x√(0.001x3.0/49.95) = 0.1% + 1.8% = 1.9%
   Auto (δ=1e-4):     r* = 0.001 + 3.72x√(0.001x3.0/49.95) = 0.1% + 2.9% = 3.0%
   Realtime (δ=1e-6): r* = 0.001 + 4.75x√(0.001x3.0/49.95) = 0.1% + 3.7% = 3.8%
```

**WiFi (ε=0.025, W=50, σ²_burst=2.9):**
```
   Bulk (δ=1e-2):     r* = 2.6% + 2.33x√(0.025x2.9/48.75) = 2.6% +  9.0% = 11.5%
   Auto (δ=1e-4):     r* = 2.6% + 3.72x√(0.025x2.9/48.75) = 2.6% + 14.3% = 16.9%
   Realtime (δ=1e-6): r* = 2.6% + 4.75x√(0.025x2.9/48.75) = 2.6% + 18.3% = 20.9%
```

**Satellite (ε=0.09, W=50, σ²_burst=5.1):**
```
   Bulk (δ=1e-2):     r* = 9.9% + 2.33x√(0.09x5.1/45.5) = 9.9% + 23.4% = 33.3%
   Auto (δ=1e-4):     r* = 9.9% + 3.72x√(0.09x5.1/45.5) = 9.9% + 37.4% = 47.3%
   Realtime (δ=1e-6): r* = 9.9% + 4.75x√(0.09x5.1/45.5) = 9.9% + 47.7% = 57.6%
```

**Note:** These corrected values show that the σ²_burst margin dominates for
lossy/bursty channels. At WiFi 2.5% loss, the margin (9-18%) exceeds the
IT minimum (2.6%). At Satellite 9% loss, the margin (23-48%) far exceeds
the IT minimum (9.9%). This reflects the cost of burst correlation — the
system needs substantial extra capacity to handle worst-case burst patterns
with high confidence.

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

   r* = 2.6% + z_δ x 3.9%     (from Section 8.5, WiFi row)

   At minimum r = r_IT = 2.6%:  P_fec = 0.5 (Section 8.2: z = 0), so
   δ = e x (1-P_fec) = 1.25% — half the lost symbols go to ARQ
   Tail latency ≈ T_retx + RTT/2 for those symbols
   For RTT = 50ms: L_arq ≈ 100ms for 1.25% of symbols

   To get δ = 1e-2: r* = 11.5% (Section 8.5, WiFi Bulk row)
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
   33ms < L_arq, so recovery must be FEC — r* ≈ 17-21%
   (between the Auto and Realtime rows of Section 8.5)
```

**Example 4: Gaming (LTE, ε=0.05)**

```
   Fix: δ = 20ms (tight), ρ = 99% (1% loss acceptable)
   Compute: r (bandwidth)

   Very tight latency + moderate reliability → aggressive FEC
   T_cut ≈ 2 x RTT (short: accept 1% loss)
   r* ≈ 15-20% (most budget goes to proactive FEC within 20ms;
   the ρ = 99% cutoff trims the taper tail below the full 20% margin)
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
   r* = (e + e_codec_eff)/(1-e) + z_δ x √((e + e_codec_eff) x σ²_burst / (W x (1-e)))
```

(the same margin structure as Section 8.4, with e replaced by
e_hat = e + e_codec_eff).

### 9.3 Impact on METTLE at DC

At DC (ε=0.1%, W=50, σ²=3.0, Bulk δ=1e-2):

Without weighting: ε_hat = 0.1% + 15% = 15.1% → r* = 15.1% + 22.2% = 37.3%.
With weighting: ε_codec_eff = 0.15 × 0.049 = 0.74%, ε_hat = 0.84%
→ r* = 0.8% + 5.2% = 6.0%.

The weighting reduces METTLE's DC overhead by ~6×.

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

d_copa = 0.5 (from the Copa paper [Copa2018]) targets a queue of 2 packets
— very shallow. The Copa+ follow-up paper explores adaptive d_copa, which
could be coupled to the protocol hint: Realtime (tight latency) → higher
d_copa (shallower queue, ~1 less packet of delay) and Bulk (throughput) →
lower d_copa (deeper queue, better link utilization). However, the gain is
approximately 1 packet of queue delay (~1ms at typical speeds) — negligible
compared to RTT. The default d_copa = 0.5 is sufficient for all scenarios.

**min_rtt estimation:** Copa uses the minimum observed RTT in a 10-second
sliding window (same duration as BBR's min_rtt window). Copa's natural rate
oscillation periodically reduces the queue to near-empty, refreshing the
minimum within this window. If min_rtt has not been refreshed for an extended
period (>20s), the system can force a brief rate reduction as a fallback
— though this is rarely needed because Copa's oscillation provides
natural refreshing.

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

### 12.6 ECN as Opportunistic Enhancement

If the network path supports ECN [RFC3168], congestion is signaled by router
marking (CE bit) instead of dropping. This provides:
- Congestion detection without loss -> even better for delay-based CC
- Positive identification: marked = congestion, dropped = channel loss
- No need to distinguish via RTT trends (direct signal)

QUIC validates ECN support at connection startup. If supported, use it.
If not (common on wireless), fall back to Copa's delay-based detection.

### 12.7 Application Back-Pressure

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
     Base:  r* = e/(1-e) + z_delta x sqrt(e x s2_burst / (W x (1-e)))  [ratio]
     With codec: replace e with e_hat (see Section 9.2)
     P_fec = Phi(sqrt(W) x (r(1-e)-e) / sqrt(e(1-e)(r+s2_burst)))      [probability]
     z_delta = normal_quantile(1-delta)                                [dimensionless]

   Codec overhead (Section 9.2):
     e_codec_eff = e_codec x (1-(1-e)^W)   weighted codec overhead     [probability]
     e_hat = e + e_codec_eff               effective loss rate         [probability]

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
For implementation, the exact O(W²) transfer matrix computation (see open
point #4, resolved) provides the same precision as the debt model. Both
are finite-state Markov chain computations over the GE channel — the debt
model tracks decoder state, the transfer matrix tracks loss counts. Either
gives exact P_fec for implementation use.

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
   implementation and validation.

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

- **[Ferlin2016]** S. Ferlin et al., "BLEST: Blocking Estimation-based MPTCP
  Scheduler," IFIP Networking, 2016.
  Skip slow paths that would stall receiver.

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
