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

The optimal correction rate r* = e/(1-e) + z_d x sqrt(e x s2_burst /
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

## 1. System Model

### 1.1 Components

```
 Sender                       Channel                      Receiver
┌───────────┐              ┌───────────┐              ┌───────────┐
│ Source    ├─ source ────►│           ├─ surviving ─►│ Decode    │
│ packets   │   syms       │  Erasure  │              │           │
│           ├─ correction ►│  Channel  ├─ surviving ─►│ FEC       │
│ Taper     │   symbols    │  (GE)     │              │ Decoder   │
│ Function  │              │           │              │           │
│           │◄─ ACK+SACK ─┤           │◄─ ACK+SACK ─┤ Gap       │
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

### 1.2 Notation

| Symbol | Meaning | Unit | Example |
|--------|---------|------|---------|
| ε      | Average channel loss rate | probability (0-1) | 0.025 (2.5% WiFi) |
| p      | P(Good → Bad) in GE model | probability (0-1) | 0.013 |
| q      | P(Bad → Good) in GE model | probability (0-1) | 0.5 |
| B      | Mean burst length = 1/q | symbols (count) | 2.0 |
| W      | Encoder window size | symbols (count) | 50 |
| RTT    | Round-trip time | seconds | 0.050 (50ms) |
| P_lost(t) | P(symbol lost given no ACK after time t) | probability (0-1) | 0.5 at t=SRTT |
| t_fec  | FEC recovery time = m / (A × (1-ε)) × t_sym | seconds | 0.003 (3ms) |
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
| L_prop | Propagation delay (base latency) | seconds | 0.025 (25ms) |
| L_arq  | ARQ recovery latency (time from loss to retransmit arrival) | seconds | 0.075-0.100 |
| σ²_burst | Burst variance inflation factor | dimensionless | 2.9 |
| z_δ    | Standard normal quantile for δ | dimensionless | 3.72 (for δ=1e-4) |
| ε_burst | Current channel loss rate (fast EWMA or GE state, optional — see C.6) | probability (0-1) | 0.5 (during burst) |
| ε_codec | Codec decode overhead | ratio (0-1) | 0.01 (RaptorQ) |

### 1.3 Glossary

**Correction symbol** — any symbol sent to recover lost data. Either a source
retransmit (exact copy, immediate decode) or a repair symbol (FEC linear
combination, needs decoder). The taper function controls how many; P_lost(t)
controls which type. See Section 3.6.

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
E.g., RaptorQ needs ~1% extra, METTLE needs ~15%. See Section 7.

**SACK (Selective Acknowledgement)** — an extension to cumulative ACK that
reports out-of-order received blocks beyond the cumulative point. Lets the
sender know exactly which symbols arrived despite gaps. See Section 3.9.

**GE states (Good/Bad)** — the two states of the Gilbert-Elliott channel
model. In the simplified model: Good = no loss, Bad = total loss. Transition
probabilities p (Good->Bad) and q (Bad->Good) determine burst behavior.
See Section 2.

### 1.4 The Bandwidth / Latency / Reliability Triangle

Three properties are linked by the channel. Fix any two, the third is determined:

```
              Bandwidth (r)
              correction symbols
              per source symbol
                   /\
                  /  \
                 / FIX \
                / any 2  \
               / compute  \
              /   the 3rd  \
             /              \
            /________________\
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
│ (δ, ρ, or r)        │    │ function       │    │ taper func     │
│                      │    │                │    │                │
│ Channel observations ├───►│                │    │ T_cut = taper  │
│ (e, p, q, RTT)      │    │                │    │ cutoff time    │
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
     '---. .---'
      1-p   1-q
      (self-loops)

   p   = P(Good -> Bad)  = probability of entering a burst
   q   = P(Bad -> Good)  = probability of exiting a burst
   1-p = P(Good -> Good) = probability of staying in Good
   1-q = P(Bad -> Bad)   = probability of burst continuing
```

**Simplified model** (used throughout): in Good state, no loss (h_G = 0).
In Bad state, total loss (h_B = 1). This makes the math tractable and is a
reasonable approximation for packet-level erasure channels.

### 2.2 Stationary Properties

Stationary state probabilities:

```
   π_B = p / (p + q)       probability of being in Bad state
   π_G = q / (p + q)       probability of being in Good state
```

Average loss rate:

```
   e = π_B × h_B + π_G × h_G = π_B = p / (p + q)
```

### 2.3 Burst Length Distribution

Given we just entered the Bad state, the burst length T follows a geometric
distribution:

```
   P(T = t) = q × (1-q)^(t-1)        for t = 1, 2, 3, ...

   P(T ≥ t) = (1-q)^(t-1)            survival function

   E[T] = 1/q = B                     mean burst length
```

This survival function is the key quantity — it tells us, given that a burst
started, how likely it is to still be ongoing after t symbols.

### 2.4 Concrete Examples

| Scenario   | ε (loss) | p (G→B)  | q (B→G) | B = 1/q | Character          |
|------------|----------|----------|---------|---------|---------------------|
| DC         | 0.1%     | 0.001    | 0.5     | 2.0     | Rare, short bursts  |
| WiFi       | 2.5%     | 0.013    | 0.5     | 2.0     | Moderate, short     |
| LTE        | 5%       | 0.02     | 0.4     | 2.5     | Moderate, medium    |
| Satellite  | 9%       | 0.03     | 0.3     | 3.3     | Frequent, long      |
| Bad WiFi   | 15%      | 0.05     | 0.3     | 3.3     | Frequent, long      |

---

## 3. Recovery Mechanisms

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
                     |----- window -----|
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
            |<----- T_retx ----------->|<-- RTT/2 -->|
            |   (sender waits for       | (one-way    |
            |    ACK, then times out)   |  propagation)|
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

**FEC recovery time (t_fec):** The taper function generates repair symbols
continuously after a source symbol is sent. Each repair that covers the lost
symbol AND survives the channel gives the decoder one more equation. For m
lost symbols in the window, the decoder needs m surviving repairs:

```
  t_fec = m / (A x (1-e)) x t_sym

  where:
    m     = number of lost symbols in the window (usually 1)
    A     = taper amplitude (corrections/symbol at offset 0)
    1-e   = probability each repair survives the channel
    t_sym = symbol_size / throughput (time to transmit one symbol)
```

Concrete examples for a single loss (m=1), A=0.04, e=0.025:

```
  At 100 Mbps, 1200-byte symbols:  t_sym = 0.096ms, t_fec = 2.5ms
  At  10 Mbps, 1200-byte symbols:  t_sym = 0.96ms,  t_fec = 24.6ms
  At   1 Mbps, 1200-byte symbols:  t_sym = 9.6ms,   t_fec = 245ms
```

For burst loss (m=5 on WiFi at 100 Mbps): t_fec = 5 x 2.5ms = 12.5ms.

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

This gives a smooth transition from "probably fine" to "certainly lost":

```
  t = 0:            P_lost = e              (just the base loss rate)
  t = SRTT:         P_lost ≈ 2e             (ACK expected by now)
  t = SRTT + 2s:    P_lost ≈ 0.98           (very confident it's lost)
  t >> SRTT:        P_lost -> 1.0            (certainly lost)
```

Concrete example (WiFi, e = 0.025, SRTT = 50ms):

```
  t = 0ms:    P_lost = 0.025    -> 97.5% repair, 2.5% retransmit
  t = 40ms:   P_lost = 0.08     -> 92% repair, 8% retransmit
  t = 50ms:   P_lost = 0.05     -> 95% repair, 5% retransmit
  t = 60ms:   P_lost = 0.35     -> 65% repair, 35% retransmit
  t = 70ms:   P_lost = 0.85     -> 15% repair, 85% retransmit
  t = 80ms:   P_lost = 0.98     -> 2% repair, 98% retransmit
```

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
  S3 lost       FEC decodes      P_lost rises,          |
                (if enough       retransmit chosen       |
                 repairs         in correction slots     |
                 arrived)                                |
  |<-- FEC -->|
  |<---- taper generating corrections the whole time --->|
  |     (gradually shifting from repair to retransmit)   |
```

At 100 Mbps on WiFi: t_fec = 2.5ms. Most losses are FEC-recovered long
before P_lost rises high enough for retransmission. ARQ retransmit is only
relevant for burst losses that overwhelm the FEC budget.

### 3.5 Why Unify FEC and ARQ?

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

### 3.6 Correction Symbols — The Unified Concept

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

### 3.7 Three-Stream View: Source, Repair, Retransmit

Conceptually, the sender manages **three streams** that compete for wire
capacity. Each stream has a different effect on the bandwidth/latency/
reliability triangle:

```
  Stream         | Latency       | Bandwidth     | Reliability
  ---------------+---------------+---------------+--------------
  Source          | ++ immediate  | neutral       | neutral
  Repair (FEC)   | - decoder     | + (no waste   | ++ (covers
                 |   wait        |   if taper     |    any loss)
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

### 3.8 Per-Slot Decision

When the taper function decides to generate a correction symbol, the sender
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
  | With probability 1 - P_retx:             |
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

P_retx is zero at the extremes (P_lost = 0: all repair, no waste; P_lost = 1:
all retransmit, definitely lost so no waste) and maximized at P_lost = 0.5
(maximum uncertainty). The model automatically allocates the right mix.

See Appendix C.6 for an optional channel-state discount (ε_burst) that
provides modest improvement for long-burst scenarios.

### 3.9 The Retransmit Buffer

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

The buffer is bounded by the encoder window size W — symbols that leave the
encoder window are removed regardless of ACK status (the FEC decoder can no
longer use them, so the system relies on the decoder having recovered them).

### 3.10 SACK-Extended WindowAck

The receiver sends periodic **ACK+SACK** messages to tell the sender what
arrived. This is the same mechanism TCP has used since RFC 2018:

```
  ACK+SACK message:
  +---------------------------------------------------+
  | cumulative_ack: 42    (all symbols <= 42 received) |
  | sack_ranges: [(45,47), (50,55)]                    |
  | echo_timestamp: 1705012345.678                     |
  +---------------------------------------------------+
```

- **Cumulative ACK**: the highest sequence number such that all symbols up to
  and including it have been received. The sender knows everything up to 42
  arrived. Symbols 43 and 44 are missing (gaps).
- **SACK ranges** [RFC2018]: out-of-order blocks received beyond the cumulative
  ACK. Here, symbols 45-47 and 50-55 also arrived. Combined with the
  cumulative ACK, the sender knows exactly that symbols 43, 44, 48, 49 are
  missing.
- **Echo timestamp**: the sender's own timestamp echoed back for RTT
  measurement (no clock synchronization needed).

**Why ACK+SACK instead of NACK?** ACK-based protocols are more robust for
unicast connections:
- If an ACK is lost, the sender simply waits longer and retransmits anyway
  (self-healing). A lost NACK means the sender never learns about the loss.
- ACKs confirm what works; NACKs report what failed. Confirming success is
  more reliable than reporting failure.
- No separate NackAck echo mechanism needed to measure reverse path loss.

### 3.11 Per-Symbol Delivery Outcomes

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

Where P_arq = probability that a retransmitted correction symbol succeeds
within the cutoff. When ρ = 100% (T_cut = infinity), P_arq = 1 and outcome 3
never occurs.

The full delivery distribution:

```
   P(on-time delivery) = (1 - e) + e x P_fec           not lost, or FEC
   P(late delivery)    = e x (1 - P_fec) x P_arq       ARQ retransmit
   P(permanent loss)   = e x (1 - P_fec) x (1 - P_arq) not recovered

   Reliability: p = 1 - P(permanent loss)
   Tail latency: d = P(late delivery) / p               among delivered symbols
```

### 3.12 The Triangle in Action

Under **100% reliability** (ρ = 1, T_cut = infinity): outcome 3 never occurs.
"Tail loss from FEC" equals "tail latency events" — they are the same thing.
This is the special case from Section 6.

Under **variable reliability** (ρ < 1, T_cut < infinity): the taper is cut off.
Symbols beyond T_cut are permanently lost. This saves bandwidth (fewer
correction symbols) and bounds latency (no recovery beyond T_cut), at the
cost of reliability.

```
  p = 100%:  ---------------------------------- (taper runs until ACK)
             all symbols eventually delivered

  p = 98%:   ----------------+
             98% delivered    | T_cut
             2% lost          +-- (taper stops, accept loss)

  p = 95%:   ----------+
             95%        | T_cut (shorter)
             5% lost    +-- accept loss (sensor/VoIP)
```

---

## 4. The Taper Function

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

The taper should allocate more correction where loss is more likely. Given that a
symbol was lost (we're in a burst), the conditional probability that the burst
is still active at offset t is:

```
   P(burst active at offset t | burst at offset 0) = (1-q)^t
```

The optimal correction allocation is proportional to this conditional probability.
This is the **water-filling solution**: given a fixed budget, allocate resources
proportional to the probability of needing them.

**Proof sketch (Lagrange multipliers):** We want to maximize P_fec given a
fixed total correction budget r. The marginal benefit of a correction symbol at
offset t is proportional to P(burst still active at t). The Lagrangian is
maximized when the correction density is proportional to (1-q)^t. This
water-filling principle is analogous to the delay-optimal streaming code
constructions in [Badr2017] and [Fong2019].

For an i.i.d. channel (q = 1, no burst memory): τ(t) = constant (flat taper).
This is correct — every position is equally likely to need correction.

### 4.3 Total Correction Rate

The total correction rate (correction symbols per source symbol) is:

```
   r = Σ_{t=0}^{∞} τ(t) = A × Σ_{t=0}^{∞} (1-q)^t = A / q
```

Since 0 < q ≤ 1, this geometric series converges. Therefore:

```
   A = r × q
```

The amplitude is uniquely determined by the correction rate and the GE parameter.

### 4.4 The Taper Never Reaches Zero

The exponential (1-q)^t is always positive for 0 < q < 1. This is correct
behavior: there is always a nonzero probability of a burst still continuing.
As long as a symbol has not been ACK'd, there is a nonzero probability it
was lost, so we should continue generating (increasingly rare) correction coverage.

```
  t = 0:    τ(0) = A                        peak correction density
  t = B:    τ(B) = A × e^{-1} ≈ 0.37 × A   one mean burst length
  t = 2B:   τ(2B) = A × e^{-2} ≈ 0.14 × A  two mean burst lengths
  t = 5B:   τ(5B) = A × e^{-5} ≈ 0.007 × A five mean burst lengths
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
   batches and widens the posterior, increasing the correction budget until the
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

## 5. Estimation — From Observations to Channel Parameters

### 5.1 What We Observe

At the sender, we receive periodic feedback:

| Observation | Source | Frequency |
|-------------|--------|-----------|
| (sent, received) per batch | ACK messages | Every batch (~10-100ms) |
| RTT | Echoed timestamps in ACK | Every batch |
| SACK ranges (out-of-order blocks) | ACK+SACK messages | Every ACK |
| Throughput | Delivery rate tracking | Continuous |

The sender infers losses from gaps: symbols not covered by the cumulative ACK
or any SACK range are presumed lost after T_retx has elapsed.

### 5.2 EWMA — Fast Point Estimate

Exponentially Weighted Moving Average of the loss rate:

```
   e_hat_ewma(n) = α × (lost/sent) + (1-α) × e_hat_ewma(n-1)

   α = 0.1 → approximately 10-sample half-life
```

**Strengths:** Simple, fast, responsive to changes.

**Limitations:** Single number — cannot express "confident at 2%" vs "uncertain,
somewhere between 0% and 10%". This inability to express uncertainty is why the
old system needed three stacked mechanisms (EWMA + Beta margin + PI controller)
to avoid under-provisioning.

### 5.3 Beta-Binomial Posterior — Uncertainty Quantification

The Beta distribution is the conjugate prior for Binomial observations:

```
   Prior:     Beta(a, b)        (a = received, b = lost)
   Update:    a' = a × decay + received
              b' = b × decay + lost
   Posterior: Beta(a', b')

   Mean loss rate:  b' / (a' + b')
   Variance:        a'b' / ((a'+b')² (a'+b'+1))
   Upper quantile:  beta_quantile(b', a', confidence)
```

The decay factor (0.995) causes old observations to fade, allowing adaptation.

**Strengths:** Principled uncertainty — the spread of the posterior tells us how
confident we are. Tight posterior → low uncertainty → small safety margin needed.

**Limitations:** Cannot detect regime changes. If loss jumps from 1% to 10%, the
posterior slowly drifts — it doesn't know the old data is from a different regime.

### 5.4 BOCD — Regime-Aware Prediction

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
   P̂_upper(confidence) = Σ_r P(r_t = r | data) × beta_quantile(stats_r, confidence)
```

**Key properties:**
- Steady state: mass concentrates at one run length → tight posterior → small margin
- Changepoint: mass spreads → wide posterior → large margin (conservative)
- The predictive quantile IS the safety margin — no separate PI or margin needed

### 5.5 GE Parameter Estimation

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
     B̂ = 1/q̂                                mean burst length
```

These estimates feed directly into the taper function shape: τ(t) = A × (1-q̂)^t.

### 5.6 ACK Loss and Self-Healing

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

### 5.7 Estimation Error and Overhead

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

## 6. The Optimization Problem

### 6.1 Formal Statement

```
   minimize:    r = A/q                     (correction rate = bandwidth cost)

   subject to:  e × (1 - P_fec(A, q)) ≤ δ  (tail latency constraint)

   where:       τ(t) = A × (1-q)^t          (taper function)
                P_fec depends on A, q, e, W  (FEC recovery probability)
```

**Input:** δ (tail latency target, from protocol hint)

**Output:** A* (optimal taper amplitude), r* = A*/q (optimal correction rate)

### 6.2 FEC Recovery Probability

Consider a symbol lost at position 0. Correction symbols generated at offsets
t = 0, 1, 2, ... each have:
- Probability τ(t) of being generated (fractional: may or may not generate one)
- Probability (1-ε) of surviving the channel

The expected number of correction symbols covering the lost position that arrive:

```
   R(A, q) = Σ_{t=0}^{W-1} τ(t) × (1-e)
           = A × (1-e) × Σ_{t=0}^{W-1} (1-q)^t
           = A × (1-e) × (1 - (1-q)^W) / q
```

For large W (window much larger than burst length): (1-q)^W ≈ 0, so:

```
   R(A, q) ≈ A × (1-e) / q = r × (1-e)
```

**Recovery model:** The number of useful correction symbols arriving is approximately
Poisson(R). For FEC recovery, we need at least 1 repair symbol (plus codec
overhead, see Section 7). Simplified to needing at least 1:

```
   P_fec(A, q) = 1 - P(Poisson(R) = 0) = 1 - e^{-R}
```

### 6.3 Solving for A*

Substituting into the constraint:

```
   e × (1 - P_fec) ≤ δ
   e × e^{-R} ≤ δ
   e^{-R} ≤ δ/e
   -R ≤ ln(δ/e)
   R ≥ ln(e/δ)                    (note: e > δ, so ln(e/δ) > 0)
```

Using R ≈ A × (1-ε) / q:

```
   A × (1-e) / q ≥ ln(e/δ)

   A* = q × ln(e/δ) / (1-e)

   r* = A*/q = ln(e/δ) / (1-e)
```

### 6.4 The Optimal Correction Rate Formula

```
  ┌───────────────────────────────────────────┐
  │                                           │
  │   r* = ln(e/δ) / (1-e)                   │
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
   A* = r* × q = q × ln(e/δ) / (1-e)
```

And the **complete taper function** is:
```
   τ*(t) = A* × (1-q)^t = q × ln(e/δ) / (1-e) × (1-q)^t
```

### 6.5 Comparison with Information-Theoretic Minimum

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
too. The (1-ε) denominator IS this geometric series. See also Section 11.4.

Our optimal rate:

```
   r* = ln(e/δ) / (1-e) = r_IT × ln(e/δ) / e = r_IT × ln(1/δ)/e + r_IT × ln(e)/e
```

The ratio r*/r_IT = ln(ε/δ)/ε. This is the **unavoidable overhead** of
targeting a tail latency of δ — it's the price of proactive protection.

For ε = 0.025 (WiFi), δ = 1e-4 (Realtime):
```
   r*/r_IT = ln(0.025/0.0001) / 0.025 = ln(250) / 0.025 = 5.52/0.025 = 221
```

This seems very high. Let's check: r_IT = 0.025/0.975 = 0.0256, so r* = 0.0256 × 221 = 5.66.
That's 566% overhead — clearly too much.

**The issue:** Our Poisson approximation is too pessimistic. A single correction
symbol at offset t doesn't independently have probability τ(t) of existing —
the correction symbols are generated deterministically by the taper schedule.
The correct model needs to account for the fact that multiple correction symbols
from the taper collectively protect the lost symbol.

### 6.6 Corrected Model

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
   R = N_correction × (1-e) = (A/q) × (1-(1-q)^W) × (1-e)
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

Using normal approximation:
```
   P(repairs < K) ≈ P(Normal(rW(1-e), rW(1-e)e) < K)
```

This requires the δ-quantile of repairs to exceed the (1-δ)-quantile of losses.
The algebra gives:

```
   r ≥ e/(1-e) + z_δ × √(e/(W(1-e)))
```

where z_δ is the standard normal quantile [Abramowitz1964] for probability δ.

### 6.7 Burst Variance Correction

The normal approximation in 6.6 assumes iid losses (Binomial variance). On a
GE channel, losses are correlated — bursts inflate the variance.

The GE autocorrelation decays with eigenvalue (1-p-q). The variance of losses
in a window of size W is:

```
   Var_iid(K) = W × e × (1-e)                    (independent losses)

   Var_GE(K)  = W × e × (1-e) × σ²_burst         (burst-correlated losses)

   σ²_burst = 1 + 2(1-p-q)/(p+q)                  (variance inflation factor)
```

| Scenario  | p+q   | σ²_burst | Meaning                                  |
|-----------|-------|----------|------------------------------------------|
| DC        | 0.501 | 3.0      | 3× wider variance than iid assumption    |
| WiFi      | 0.513 | 2.9      | similar                                  |
| LTE       | 0.42  | 3.8      | significant inflation                    |
| Satellite | 0.33  | 5.1      | iid approximation seriously wrong        |

We compute σ²_burst directly from the GE estimator's p̂ and q̂.

### 6.8 The Corrected Optimal Correction Rate

```
  r* = e/(1-e) + z_d x sqrt(e x s2_burst / (W x (1-e)))
       '--v--'   '--------------v-----------------'
    IT minimum             tail margin
                 (accounts for burst correlation)

  s2_burst = 1 + 2(1-p-q)/(p+q)

  z_d = standard normal quantile for (1-d)
```

**Properties:**
- IT minimum ε/(1-ε) is the dominant term
- Tail margin scales as 1/√W — larger windows need proportionally less margin
- z_δ controls the margin: tighter δ → larger z_δ → more margin
- σ²_burst amplifies the margin for bursty channels (large for small p+q)

### 6.9 Worked Examples

Using z_δ values: z(1e-2) = 2.33, z(1e-4) = 3.72, z(1e-6) = 4.75

The margin term is: `z_δ × √(ε × σ²_burst / (W × (1-ε)))`

**DC (ε=0.001, W=50, σ²_burst=3.0):**
```
   Bulk (δ=1e-2):    r* = 0.001 + 2.33×√(0.001×3.0/49.95) = 0.1% + 1.8% = 1.9%
   Auto (δ=1e-4):    r* = 0.001 + 3.72×√(0.001×3.0/49.95) = 0.1% + 2.9% = 3.0%
   Realtime (δ=1e-6): r* = 0.001 + 4.75×√(0.001×3.0/49.95) = 0.1% + 3.7% = 3.8%
```

**WiFi (ε=0.025, W=50, σ²_burst=2.9):**
```
   Bulk (δ=1e-2):    r* = 2.6% + 2.33×√(0.025×2.9/48.75) = 2.6% + 2.8% = 5.4%
   Auto (δ=1e-4):    r* = 2.6% + 3.72×√(0.025×2.9/48.75) = 2.6% + 4.5% = 7.1%
   Realtime (δ=1e-6): r* = 2.6% + 4.75×√(0.025×2.9/48.75) = 2.6% + 5.8% = 8.4%
```

**Satellite (ε=0.09, W=50, σ²_burst=5.1):**
```
   Bulk (δ=1e-2):    r* = 9.9% + 2.33×√(0.09×5.1/45.5) = 9.9% + 7.4% = 17.3%
   Auto (δ=1e-4):    r* = 9.9% + 3.72×√(0.09×5.1/45.5) = 9.9% + 11.8% = 21.7%
   Realtime (δ=1e-6): r* = 9.9% + 4.75×√(0.09×5.1/45.5) = 9.9% + 15.1% = 25.0%
```

### 6.10 Three-Variable Optimization

Section 6.8 solves for r* given δ (with ρ=100%). Here we generalize to all
three modes of the bandwidth/latency/reliability triangle.

#### Taper with cutoff

When ρ < 100%, the taper is truncated at T_cut:

```
   τ(t) = A × (1-q)^t    for t ≤ T_cut
   τ(t) = 0               for t > T_cut
```

Total correction rate with cutoff:

```
   r = A × Σ_{t=0}^{T_cut} (1-q)^t = A × (1 - (1-q)^{T_cut+1}) / q
```

For T_cut = ∞ (ρ = 100%): reduces to r = A/q (Section 4.3).

#### Mode 1: Given (δ, ρ) → compute r

Fix tail latency target δ and reliability ρ. Compute minimum bandwidth r*.

Step 1: From ρ, find T_cut. The reliability ρ = P(recovered within T_cut).
Using the corrected model (Section 6.8):

```
   T_cut such that: e × (1 - P_fec(T_cut)) × (1 - P_arq(T_cut)) = 1 - ρ
```

For ρ = 100%: T_cut = ∞ (no cutoff). For ρ = 98%: solve for finite T_cut.

Step 2: From δ, find A using the tail latency constraint (Section 6.8):

```
   A* such that: e × (1 - P_fec(A*)) ≤ δ    (among delivered symbols)
```

Step 3: Compute r* = A* × (1 - (1-q)^{T_cut+1}) / q.

**Special case ρ = 100%**: T_cut = ∞, r* = A*/q = ε/(1-ε) + z_δ√(...).
This is the formula from Section 6.8.

#### Mode 2: Given (r, ρ) → compute δ

Fix bandwidth r and reliability ρ. Compute resulting tail latency δ.

```
   From ρ: find T_cut (same as Mode 1, Step 1)
   From r and T_cut: A = r × q / (1 - (1-q)^{T_cut+1})
   From A: P_fec = 1 - exp(-R)  where R = A(1-e)/q × (1-(1-q)^W)
   Result: δ = e × (1 - P_fec) × P_arq / ρ
```

#### Mode 3: Given (r, δ) → compute ρ

Fix bandwidth r and tail latency δ. Compute resulting reliability ρ.

```
   From r: A = r × q / (1 - (1-q)^{T_cut+1})     (depends on T_cut)
   From δ: determine how much of the taper is "on-time" vs "late"
   From A and the taper integral: ρ = total recovery probability within T_cut
```

This mode requires solving for T_cut and A simultaneously (they're coupled).
In practice, iterate: start with T_cut = ∞, compute r needed for δ,
if r > budget, reduce T_cut until the bandwidth constraint is met.
The resulting ρ falls out.

#### Worked examples

**Example 1: Bulk file transfer (WiFi, ε=0.025)**

```
   Fix: ρ = 100%, minimize r
   Compute: δ (tail latency)

   r* = 2.6% + z_δ × 1.2%     (from Section 6.9, WiFi row)

   At minimum r = r_IT = 2.6%:  δ = e = 2.5% (every lost symbol goes to ARQ)
   Tail latency ≈ T_retx + RTT/2 for 2.5% of symbols
   For RTT = 50ms: L_arq ≈ 100ms for 2.5% of symbols

   To get δ = 1e-2: r* = 5.4% (from worked examples)
```

**Example 2: VoIP (WiFi, ε=0.025)**

```
   Fix: δ = 150ms budget → T_cut = 150ms / symbol_time
        r = 5% (codec + small overhead)
   Compute: ρ (reliability)

   With r = 5%, the taper generates enough correction symbols that:
   - P_fec ≈ 0.90 (90% of lost symbols FEC-recovered, zero latency)
   - P_arq within 150ms ≈ 0.08 (8% recovered by retransmit)
   - P_lost = 0.02 (2% permanently lost)
   - ρ = 98%

   The VoIP codec conceals the 2% frame loss.
```

**Example 3: Live video (WiFi, ε=0.025)**

```
   Fix: δ = 33ms (one frame at 30fps), ρ = 99.9%
   Compute: r (bandwidth)

   Need 99.9% of symbols delivered within 33ms.
   T_cut determined by ρ = 99.9%: T_cut ≈ 3 × RTT
   A determined by δ: need P(recovery within 33ms) ≥ 0.999
   r* ≈ 8.4% (close to Realtime from Section 6.9)
```

**Example 4: Gaming (LTE, ε=0.05)**

```
   Fix: δ = 20ms (tight), ρ = 99% (1% loss acceptable)
   Compute: r (bandwidth)

   Very tight latency + moderate reliability → aggressive FEC
   T_cut ≈ 2 × RTT (short: accept 1% loss)
   r* ≈ 12% (most budget goes to proactive FEC within 20ms)
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

## 7. Codec Overhead Integration

### 7.1 Decoder Invocation Probability

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

### 7.2 Effective Codec Overhead

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
   e_codec_eff = e_codec × P(decoder invoked) = e_codec × (1 - (1-e)^W)
```

The corrected correction rate becomes:

```
   r* = (e + e_codec_eff)/(1-e) + z_δ × √((e + e_codec_eff) / (W × (1-e)))
```

### 7.3 Impact on METTLE at DC

Without weighting: r* includes 15% codec overhead → 16.1% correction rate.
With weighting: ε_codec_eff = 0.15 × 0.049 = 0.74% → 1.8% correction rate.

The weighting reduces METTLE's DC overhead by ~9×.

---

## 8. Multi-Protocol Extension

### 8.1 Per-Symbol Latency Classes

Different traffic types have different δ targets:

```
   Symbol s has latency class c(s) with target δ_{c(s)}
```

### 8.2 Interleave Before vs After FEC

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

### 8.3 When Shared Wins

Shared FEC is cheaper when the traffic mix is balanced or dominated by the
tight class. Separate streams are cheaper when the tight class is a small
fraction. The crossover depends on the specific δ values and loss rate.

**Decision rule:** Compare total correction bandwidth:
```
   shared_cost = r*(e, min(δ_c))          (one encoder, tightest δ)
   separate_cost = Σ_c f_c × r*(e, δ_c)  (per-class, weighted by fraction f_c)
```

Choose whichever is lower.

### 8.4 Extending the Formula

For shared FEC with per-symbol δ, the constraint becomes:

```
   For each class c: e × (1 - P_fec) ≤ δ_c
```

Since P_fec is the same for all symbols (shared repair), the binding constraint
is the tightest: δ_min = min(δ_c). The formula reduces to the single-class case
with δ = δ_min.

---

## 9. Verification

### 9.1 Simulation Approach

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

### 9.2 Analytical Predictions to Verify

| Prediction | Formula | Test |
|------------|---------|------|
| IT minimum dominates at high loss | r* ≈ ε/(1-ε) when W large | Satellite scenario |
| Tail margin scales as 1/√W | r*(W=200) < r*(W=50) | Compare window sizes |
| Taper shape matches GE | Decay rate = q̂ from estimator | Compare simulated vs theoretical |
| Codec overhead weighting | METTLE DC overhead ~ 0.74% not 15% | DC scenario with METTLE |
| Protocol hint only affects δ | Realtime(1e-5) = Auto(1e-7) | Same estimator, different hints |

### 9.3 Boundary Cases

| Case | Expected behavior | Why |
|------|-------------------|-----|
| ε = 0 (no loss) | r* = 0 | No correction needed |
| ε → 1 (total loss) | r* → ∞ | Can't recover anything with FEC alone |
| δ = ε (every loss to ARQ) | r* = 0 | No FEC needed, all ARQ |
| δ → 0 (zero ARQ tolerance) | r* → ∞ | Must FEC-recover everything |
| W = 1 (no window) | Margin term large | Single-symbol recovery needs more redundancy |
| q = 1 (no burst memory) | τ(t) = flat | Reduces to iid case |

### 9.4 Connection to Existing Benchmarks

The bench_suite already measures `overhead_pct` and `recovery_pct` per scenario.
To verify the model:

1. Compute r* from the formula for each scenario's (ε, δ, W)
2. Compare with the bench_suite's measured overhead
3. The gap between r* and measured overhead = estimation tax + implementation overhead
4. Track this gap across benchmark runs — it should decrease as we improve the implementation

---

## 10. Congestion Control Integration

### 10.1 Why Delay-Based CC is Required

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

### 10.2 QUIC Datagrams Bypass Quinn's CC

Raptorpath sends symbol data as QUIC unreliable datagrams, which bypass
Quinn's built-in congestion control (NewReno). Our own CC is the sole rate
limiter for data traffic. Quinn's CC only applies to QUIC streams (handshake,
reliable control messages — small and infrequent).

This means our CC is solely responsible for not flooding the network.

### 10.3 Copa vs BBR

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
    |  ==================           |  ========            ========
    |  continuous protection        |  gap!     ^^^^^^^^^^
    +----------------------> t      +----------|-----------> t
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

### 10.4 Copa's Rate Formula

Copa targets a queue occupancy that balances throughput and delay:

```
  rate = 1 / (d x dq)

  where:
    d  = Copa parameter (controls target queue depth; default d = 0.5)
    dq = queuing_delay = RTT_current - RTT_min
```

When dq is small (queue empty): rate is high -> fill the pipe.
When dq is large (queue building): rate drops -> drain the queue.

This naturally oscillates: send fast -> queue builds -> send slow -> queue
drains -> send fast again. The oscillation frequency is ~1/RTT and amplitude
depends on d. No periodic forced drain phase needed.

**min_rtt estimation:** Copa uses the minimum observed RTT in a sliding
window, same as BBR. Copa's natural oscillation causes the queue to
periodically drain to near-empty, refreshing min_rtt as a side effect.
No explicit ProbeRTT phase needed.

### 10.5 CC + Taper: The Complete Architecture

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

### 10.6 ECN as Opportunistic Enhancement

If the network path supports ECN [RFC3168], congestion is signaled by router
marking (CE bit) instead of dropping. This provides:
- Congestion detection without loss -> even better for delay-based CC
- Positive identification: marked = congestion, dropped = channel loss
- No need to distinguish via RTT trends (direct signal)

QUIC validates ECN support at connection startup. If supported, use it.
If not (common on wireless), fall back to Copa's delay-based detection.

### 10.7 Application Back-Pressure

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

---

## 11. Multi-Path Scheduling

### 11.1 Why FEC Beats MPTCP

MPTCP (Multi-Path TCP) schedulers are fundamentally limited by **head-of-line
(HOL) blocking**: TCP requires in-order delivery, so a packet on the slow path
blocks all fast-path packets at the receiver. MPTCP schedulers (round-robin,
weighted, BLEST [Ferlin2016]) try to minimize this by avoiding slow paths,
but they can't eliminate it.

Our FEC-based model is fundamentally different:

```
  MPTCP:                           Raptorpath:

  Path A: [P1] [P2] [P5] [P6]     Path A: [S1] [C] [S3] [C]
  Path B: [P3] [P4]               Path B: [S2] [S4] [C]

  Receiver must wait for P3        Decoder needs ANY k of n symbols.
  before delivering P4,P5,P6.      Order doesn't matter.
  HOL blocking on slow path.        No HOL blocking.
```

The decoder doesn't care WHICH symbols arrive — just HOW MANY. A repair
symbol on Path A can recover a lost source on Path B. Source symbols can
arrive in any order. The reorder buffer handles sequencing after decode.

### 11.2 Per-Path Model

Each path i runs independently with its own:

```
  Copa_i:     rate_i = 1 / (d x dq_i)         total sending rate
  GE_i:       (e_i, p_i, q_i)                  loss model
  Taper_i:    tau_i(t) = A_i x (1-q_i)^t       correction density
  r_i:        correction rate = A_i / q_i       source/correction ratio
```

All paths share:
- **One source stream**: source symbols are distributed across paths
- **One retransmit buffer**: any path can retransmit any un-ACKed symbol
- **One FEC encoder window**: repair symbols cover the same source window

### 11.3 Unified Symbol Stream and Interleaving

Each path carries a **unified stream** of source + correction symbols,
interleaved at that path's own taper ratio. The interleaving is essential
for burst protection — corrections scattered among source symbols survive
bursts that wipe out consecutive symbols:

```
  Path A (e=0.05, r=0.05):  [S][S][S][S][C][S][S][S][S][C]...   5% corrections
  Path B (e=0.10, r=0.11):  [S][S][C][S][S][C][S][S][C]...      11% corrections

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

### 11.4 Correction Deficit

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
  Source lost:                0.30   (30% loss)
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

### 11.5 Effective Delivery Time and Bandwidth

For each path i:

```
  E_i     = RTT_i/2 + e_i x t_recovery_i       effective delivery time  [sec]
  B_eff_i = C_i / (1 + r_i)                     source-carrying capacity [sym/s]
  e_combined = SUM(C_i x e_i) / SUM(C_i)        throughput-weighted loss  [prob]
```

### 11.6 Scheduler Ratio Adjustment

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

### 11.7 Burst Protection During Ratio Adjustment (largely resolved)

When the scheduler reduces a path's correction ratio (more source, fewer
corrections), that path becomes more vulnerable to burst loss. The question:
how to maintain burst protection?

**Option 1: Hard floor.** r_i' >= B_i/W (mean burst length / window size).
Guarantees enough corrections for one expected burst per window. Conservative.

**Option 2: Protocol-hint-dependent floor.**
Realtime: r_i' >= B_i/W (hard floor). Bulk: r_i' >= 0 (let taper
self-correct via BOCD). Balanced: r_i' >= B_i/(2W) (softer floor).

**Option 3: Let the taper self-correct.** No floor. If a burst overwhelms
the reduced correction ratio, BOCD detects increased loss, raises ε, and
the taper amplitude A increases automatically. The first burst after
scheduler adjustment is under-protected (5-15 sample detection delay);
subsequent bursts are covered.

**Resolution:** The P_lost(t) timing naturally produces Mehrotra's optimal
policy [Mehrotra2010]: before RTT elapses (during burst), P_lost is low so
corrections are mostly repair — flexible coverage for any lost symbol.
After RTT (burst usually over), P_lost rises and corrections shift to
targeted retransmit. BOCD + taper adapts the correction rate within ~1ms
of burst detection, which is negligible relative to RTT.

A two-speed taper (A_effective = A_baseline x (1 + burst_boost)) is an
optional micro-optimization for extreme scenarios where a long burst is
still ongoing at detection time. See Appendix C.6 for details.

### 11.8 Interpolated Objective Function

One parameterized objective with weights from the protocol hint:

```
  minimize: w_lat x SUM(x_i x E_i) + w_bw x SUM(x_i x r_i)
             ^                          ^
        latency cost               bandwidth overhead cost

  subject to: SUM(x_i) = 1              all source distributed
              x_i x source_rate <= B_eff_i    per-path capacity
```

```
  Realtime:   w_lat = 1.0,  w_bw = 0.0   minimize latency at any bandwidth cost
  Balanced:   w_lat = 0.5,  w_bw = 0.5   balance latency and bandwidth
  Bulk:       w_lat = 0.0,  w_bw = 1.0   minimize bandwidth waste (overhead)
```

### 11.9 QoS Priority Cascade

When multiple protocol classes share the same paths, they pick in priority
order from tightest to loosest latency requirement:

```
  1. Realtime picks first:  lowest E_i paths, up to its source volume
  2. Balanced picks next:   best remaining path capacity
  3. Bulk gets the rest:    whatever capacity remains
```

### 11.10 Cross-Path Retransmit

The shared retransmit buffer enables recovery across paths. When source
symbol S_k was sent on path A and lost, any path's correction slot can
retransmit it:

```
  P(retransmit on path j) = P_lost(t_k, e_A)
```

Cross-path diversity: P(both fail) = e_A x e_j. For e_A=0.10, e_j=0.02:
P(both fail) = 0.002 — 50x improvement over single-path.

---

## Appendix A: Summary of Key Formulas

```
   Channel (Section 2):
     e = p/(p+q)                          average loss rate          [probability]
     B = 1/q                              mean burst length          [symbols]
     P(burst ≥ t) = (1-q)^{t-1}          burst survival             [probability]
     σ²_burst = 1 + 2(1-p-q)/(p+q)       burst variance inflation   [dimensionless]

   Taper function (Section 4):
     τ*(t) = A* × (1-q)^t                optimal taper function     [corrections/symbol]
     A* = r* × q                          taper amplitude (ρ=100%)  [corrections/symbol]
     τ(t) = 0 for t > T_cut              taper cutoff (ρ<100%)

   Optimal correction rate (Section 6.8, ρ=100%):
     r* = e_hat/(1-e_hat) + z_δ × √(e_hat × σ²_burst / (W(1-e_hat)))            [ratio]
     where e_hat = e + e_codec × (1-(1-e)^W)  effective loss            [probability]
           z_δ = Φ⁻¹(1-δ)                  normal quantile           [dimensionless]

   Three-variable optimization (Section 6.10):
     Given (δ, ρ) → r:  find T_cut from ρ, find A from δ, r = A(1-(1-q)^T_cut)/q
     Given (r, ρ) → δ:  find T_cut from ρ, compute A from r, δ = e(1-P_fec)P_arq/ρ
     Given (r, δ) → ρ:  iterate T_cut until budget constraint met, ρ = recovery within T_cut

   Per-symbol delivery (Section 3.5):
     P(on-time)   = (1-e) + e × P_fec                               [probability]
     P(late)      = e × (1-P_fec) × P_arq                           [probability]
     P(lost)      = e × (1-P_fec) × (1-P_arq) = 1-ρ                [probability]

   Recovery latency (Section 3.4):
     t_sym = symbol_size / throughput     symbol transmission time   [seconds]
     t_fec = m / (A x (1-e)) x t_sym     FEC recovery time          [seconds]
     P_lost(t) = e / [e + (1-e) x P(RTT>t)]  loss confidence        [probability]
     L_actual = min(t_fec, retransmit arrival)                       [seconds]

   Per-slot decision (Section 3.8):
     P_retx = P_lost(t)                                    (time-based mixing)
     with probability P_retx:     send source retransmit   (immediate decode)
     with probability 1-P_retx:   send repair symbol       (FEC, any loss)
     Optional refinement: P_retx = P_lost(t) x (1 - e_burst)
       for long-burst scenarios (Appendix C.6)

   Congestion control (Section 10):
     Copa: rate = 1 / (d x dq)                              [symbols/sec]
     dq = RTT_current - RTT_min                              [seconds]
     source_rate = total_rate / (1 + r*)                     [symbols/sec]
     correction_rate = total_rate x r* / (1 + r*)            [symbols/sec]

   Multi-path scheduling (Section 11):
     E_i = RTT_i/2 + e_i x t_recovery_i                      [seconds]
     B_eff_i = C_i / (1 + r_i)                               [symbols/sec]
     e_combined = SUM(C_i x e_i) / SUM(C_i)                  [probability]
     deficit = SUM_{un-ACKed s}(e_s)                          [expected corrections]
     cross-path: r = e_source / (1 - e_correction)           [ratio]
     minimize: w_lat x SUM(x_i x E_i) + w_bw x SUM(x_i x r_i)
     P(cross-path retx) = P_lost(t, e_src)
     P(both paths fail) = e_src x e_retx                      [probability]
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
where ACK implosion is a concern [RFC3208]. For our unicast tunnel model,
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
that directly applies to our system.

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
and represent improvements over the base model in Sections 1-9.

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

### C.3 Analytical P_fec Bounds [Vajha2020]

Vajha et al. derive upper and lower bounds on block-erasure probability for
streaming codes over GE without simulation. Their bounds depend on:
- GE parameters (p, q)
- Code rate R = K/N
- Decoding delay constraint T

**Extension to verification (Section 9):**

```
   For our taper with rate r* and window W:
     Effective code rate R = 1/(1+r*)
     Delay constraint T = W

   Vajha lower bound ≤ P(erasure) ≤ Vajha upper bound

   If our predicted P_fec falls within these bounds: ✓ model validated
   If not: normal approximation is inadequate → use debt model (C.2)
```

This gives us an analytical verification path that complements simulation.

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
   τ_multi(t) = A_multi × (1-q)^t

   where A_multi = A_single × (1-e) / (1-Π(e_i))
```

For two paths with 5% loss each: A_multi = A_single × 0.95/0.9975 ≈ 0.95 × A_single.
Modest gain for similar paths, but significant when paths have different
characteristics (one lossy WiFi, one reliable Ethernet).

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

   τ*(t) = max(A × (1-q)^t, τ_floor)  (probabilistic taper with hard floor)
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

**Corrected total correction rate with floor:**

```
   r* = max(A/q, τ_floor × W) / W

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
   longer possible — but the retransmit buffer (Section 3.7) still holds the
   exact source symbol for ARQ retransmission. The truncation error (1-q)^W
   only affects the FEC component; the unified model's ARQ fallback covers it.

2. **Multi-path (resolved):** Each path has its own taper, Copa, and GE
   estimator (Section 11). Unified stream per path preserves interleaving.
   Scheduler adjusts per-path source/correction ratio (latency mode only).
   Global correction deficit tracks outstanding corrections. Cross-path
   retransmit via shared buffer. Burst protection during scheduler ratio
   adjustment is largely resolved: P_lost timing + BOCD adapts within ~1ms
   (Section 11.7). Two-speed taper is an optional refinement for extreme
   scenarios (Appendix C.6).

3. **Interaction with congestion control (resolved):** Copa [Copa2018] controls
   total rate, taper controls source/correction split (Section 10). When
   source_rate = total_rate/(1+r*) drops below the app's minimum, the system
   signals back-pressure. Copa is preferred over BBR: no ProbeRTT, no FEC
   protection gaps, simpler formula, taper-compatible.

4. **Normal approximation validity:** Even with the burst variance correction
   (σ²_burst), the normal approximation to the loss count may be inaccurate
   for small windows or very bursty channels. Could we use the exact GE
   distribution (computable from the transition matrix) for higher precision?

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

6. **RTT distribution for P_lost not specified.** (Section 3.4) [LOW]
   P_lost uses P(RTT > t) but never specifies the distribution. Assuming
   normal with SRTT and RTTVAR gives P(RTT > t) = 1 - Phi((t-SRTT)/RTTVAR).
   Needs one sentence.

7. **P_arq never defined.** (Section 3.11, 6.10, Appendix A) [MEDIUM]
   Used in delivery outcome formulas but never derived. P_arq = probability
   that a retransmitted correction succeeds within T_cut. Depends on loss
   rate of the retransmit path, number of retransmit attempts within T_cut.

8. **P_fec: two contradictory models.** (Section 6.2 vs 6.6) [MEDIUM]
   Section 6.2 says "need at least 1 repair." Section 6.6 says "need k
   repairs for k losses." The corrected model (6.6+) is canonical but the
   Poisson model (6.2-6.4) should be explicitly marked as superseded.

9. **Poisson model error not characterized.** (Section 6.5) [LOW]
   The transition from Poisson (6.4) to corrected (6.6) says "too
   pessimistic" but doesn't explain why. The error: Poisson treats each
   repair as independent, ignoring that repairs are deterministically
   generated by the taper. One paragraph.

10. **r* formula: which version is canonical?** (Section 6.8 vs 7.2) [LOW]
    Section 6.8 uses raw ε. Section 7.2 adds codec overhead ε_codec.
    The canonical formula should be r* with ε_hat = ε + ε_codec × P(decoder).
    Needs a clarifying note in Section 6.8.

11. **T_cut computation algorithm.** (Section 6.10) [MEDIUM]
    Finding T_cut from ρ requires solving an implicit equation. Binary search
    works (T_cut is monotone in ρ). Needs a 3-line algorithm.

12. **Mode 3 convergence.** (Section 6.10) [MEDIUM]
    The iterative approach (reduce T_cut until r fits) needs convergence
    justification. Since ρ is monotone in T_cut and T_cut is monotone in r,
    binary search converges. One paragraph.

13. **t_recovery_i undefined in multipath context.** (Section 11.5) [LOW]
    E_i = RTT_i/2 + ε_i × t_recovery_i but t_recovery_i not defined for
    multipath. It should reference Section 3.4: t_recovery_i = P_fec_i ×
    t_fec_i + (1-P_fec_i) × L_arq_i. One sentence.

**Implementation details (medium priority):**

14. **Retransmit buffer saturation.** (Section 3.9) [LOW]
    If ACKs stall, buffer fills. The encoder window eviction already handles
    this — symbols leaving the window are removed. No deadlock because the
    window advances regardless of ACK state. One sentence clarification.

15. **SACK timing and format.** (Section 3.10) [MEDIUM]
    "Periodic" is vague. Should specify: receiver sends ACK+SACK on every
    incoming batch (piggybacked). Format follows RFC 2018 SACK blocks.

16. **GE parameter initialization.** (Section 5.5) [LOW]
    Initial state: all counters = 0, start in Good state, use the Beta
    prior (weak uniform) until enough transitions observed. GE is_valid()
    already gates usage (existing code). One sentence.

17. **Copa parameter δ tuning.** (Section 10.4) [MEDIUM]
    Copa's δ controls target queue depth. Default 0.5 is from the Copa
    paper. Adaptive δ is future work (Copa+ paper addresses this).

18. **Copa min_rtt refresh.** (Section 10.4) [LOW]
    Copa's natural oscillation refreshes min_rtt. Sliding window of 10s
    (same as BBR). If min_rtt seems stale (RTT consistently above by 2x),
    force a brief rate reduction. One paragraph.

19. **Back-pressure signaling.** (Section 10.7) [MEDIUM]
    The signal mechanism is implementation-specific (API callback, error
    code, or rate-limit feedback). The paper should note this is an API
    design question, not a model question.

20. **Scheduler burst protection floor choice.** (Section 11.7) [LOW]
    Three options given. Recommendation: Option 3 (let taper self-correct)
    for simplicity, with Option 1 (hard floor) available as a safety net
    for production deployments. One sentence.

21. **Cross-path retransmit path selection.** (Section 11.10) [LOW]
    Path with lowest ε_burst and available capacity. Greedy selection.
    Already implied by P_retx formula (highest (1-ε_burst) wins).

**Minor (notation, justification):**

22. **z_d vs z_δ inconsistency.** (Section 6.8) [LOW]
    Line uses "z_d" and "d" where it should be "z_δ" and "δ".

23. **Beta decay 0.995 not justified.** (Section 5.3) [LOW]
    Standard value for slow-forgetting Bayesian update. Half-life ≈ 138
    samples. Sensitivity is low — values 0.99-0.999 give similar results.

24. **BOCD "5-15 batches" — what's a batch?** (Section 4.5) [LOW]
    A batch = one ACK feedback cycle. With per-batch ACKs at ~10-100ms
    intervals, 5-15 batches = 50ms-1.5s adaptation time.

25. **GE simplified model error bounds.** (Section 2.1) [MEDIUM]
    The h_G=0, h_B=1 approximation ignores partial loss in each state.
    Error grows when real h_G > 0 or h_B < 1. Sensitivity analysis would
    quantify this but is not critical for the model's validity.

26. **P_arq missing from Appendix A.** [LOW]
    Add once P_arq is defined (see item 7).

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
  The BOCD algorithm used in Section 5.4 for regime-aware loss estimation.
  Maintains run-length distribution with O(r_max) per update.

- **[RFC3550]** H. Schulzrinne, S. Casner, R. Frederick, V. Jacobson,
  "RTP: A Transport Protocol for Real-Time Applications," IETF RFC 3550, 2003.
  Appendix A.8 defines the interarrival jitter calculation used in our estimator.

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
  rate = 1/(δ × dq). Recommended for raptorpath (Section 10).

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

### Sliding Window Channel Models

- **[Vajha2020b]** M. Vajha, V. Ramkumar, P.V. Kumar, "On Sliding Window
  Approximation of Gilbert-Elliott Channel for Delay Constrained Setting,"
  arXiv:2005.06914, 2020.
  Formalizes the DCSW-to-GE approximation used in streaming code design.
