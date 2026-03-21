# FEC/ARQ Unified Correction Symbol Model

A principled model for optimal correction symbol allocation in raptorpath.

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
| When chosen    | Un-ACKed symbol older than T_retx exists | No eligible retransmit candidate |
| Content        | Exact copy of source symbol   | Random linear combination      |
| Decode cost    | Zero (immediate use)          | Needs FEC decoder              |
| Bandwidth cost | Same (one symbol slot)        | Same (one symbol slot)         |
| Latency cost   | T_retx + RTT/2 (ARQ recovery) | Zero additional (arrives with source) |

### 1.2 Notation

| Symbol | Meaning | Unit | Example |
|--------|---------|------|---------|
| ε      | Average channel loss rate | probability (0-1) | 0.025 (2.5% WiFi) |
| p      | P(Good → Bad) in GE model | probability (0-1) | 0.013 |
| q      | P(Bad → Good) in GE model | probability (0-1) | 0.5 |
| B      | Mean burst length = 1/q | symbols (count) | 2.0 |
| W      | Encoder window size | symbols (count) | 50 |
| RTT    | Round-trip time | seconds | 0.050 (50ms) |
| T_retx | Retransmit timeout | seconds | 0.075 (1.5×RTT) |
| r      | Total correction rate | ratio (corrections/source) | 0.08 (8%) |
| τ(t)   | Taper function: correction density at offset t | ratio (corrections/symbol) | 0.04 |
| A      | Taper amplitude (scaling factor) | ratio (corrections/symbol) | 0.04 |
| P_fec  | Probability a lost symbol is FEC-recovered | probability (0-1) | 0.95 |
| δ      | Tail latency target: P(late delivery) ≤ δ | probability (0-1) | 1e-4 |
| ρ      | Reliability target: P(symbol delivered) ≥ ρ | probability (0-1) | 1.0 (100%) |
| T_cut  | Taper cutoff time (stop corrections after this) | seconds | ∞ (100% reliability) |
| L_prop | Propagation delay (base latency) | seconds | 0.025 (25ms) |
| L_arq  | ARQ recovery latency = T_retx + RTT/2 | seconds | 0.100 (100ms) |
| σ²_burst | Burst variance inflation factor | dimensionless | 2.9 |
| z_δ    | Standard normal quantile for δ | dimensionless | 3.72 (for δ=1e-4) |
| ε_codec | Codec decode overhead | ratio (0-1) | 0.01 (RaptorQ) |

### 1.3 The Bandwidth / Latency / Reliability Triangle

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
│ (ε, p, q, RTT)      │    │                │    │ cutoff time    │
└──────────────────────┘    └────────────────┘    └────────────────┘
```

When ρ < 100%, the taper has a finite cutoff T_cut. Symbols not recovered by
T_cut are permanently lost. When ρ = 100%, T_cut = ∞ (correction symbols
continue until ACK, the infinite-tailed taper from Section 4).

---

## 2. Channel Model

### 2.1 Gilbert-Elliott Two-State HMM

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
   ε = π_B × h_B + π_G × h_G = π_B = p / (p + q)
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

### 3.1 Correction Symbols — The Unified Concept

Instead of separate FEC (proactive repair) and ARQ (reactive retransmit)
mechanisms, we have **one mechanism**: the taper function controls the density
of **correction symbols**. Each correction symbol occupies one symbol slot on
the wire and serves exactly one of two purposes:

1. **Source retransmit**: an exact copy of a previously-sent source symbol that
   the receiver has not yet ACKed. The receiver can use it immediately — no
   decoder needed.

2. **Repair symbol**: a random linear combination of source symbols in the
   encoder window (standard FEC). The receiver feeds it to the FEC decoder.

From the channel's perspective, both are identical: one symbol slot, subject to
the same erasure probability ε. From the bandwidth budget's perspective, both
cost the same. The only difference is what the receiver does with them.

### 3.2 Per-Slot Decision

When the taper function decides to generate a correction symbol, the sender
makes a per-slot decision:

```
  Taper decides: "generate a correction symbol now"
                      │
                      ▼
  ┌───────────────────────────────────────┐
  │ Retransmit buffer has un-ACKed       │
  │ source symbol older than T_retx?     │
  ├──── YES ──────────┬──── NO ──────────┤
  │                   │                  │
  │ Retransmit        │ Generate random  │
  │ exact source      │ repair symbol    │
  │ (immediate        │ (FEC, needs      │
  │  decode)          │  decoder)        │
  └───────────────────┴──────────────────┘
```

**Why prefer retransmit?** A retransmitted source symbol is immediately usable
by the receiver — no FEC decoding needed, no dependency on other symbols. It is
strictly better than a repair symbol when the sender has high confidence the
original was lost (because enough time has passed without an ACK).

**Why T_retx?** The timeout T_retx prevents premature retransmission. If we
retransmit too early, the original might still be in flight and we waste a
correction slot. T_retx should be set to approximately RTT + margin, so that
an ACK would have arrived by now if the symbol was received.

### 3.3 The Retransmit Buffer

The sender maintains a **retransmit buffer**: an ordered list of source symbols
that have been sent but not yet ACKed.

```
  Retransmit buffer (ordered by send time, oldest first):

  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┐
  │ S12 │ S13 │ S17 │ S18 │ S19 │ S24 │ S25 │  <- un-ACKed symbols
  └─────┴─────┴─────┴─────┴─────┴─────┴─────┘
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

### 3.4 SACK-Extended WindowAck

The receiver sends periodic **ACK+SACK** messages (replacing the former
WindowNack/NackAck mechanism):

```
  ACK+SACK message:
  ┌──────────────────────────────────────────────────┐
  │ cumulative_ack: 42    (all symbols ≤ 42 received)│
  │ sack_ranges: [(45,47), (50,55)]                  │
  │ echo_timestamp: 1705012345.678                   │
  └──────────────────────────────────────────────────┘
```

- **Cumulative ACK**: the highest sequence number such that all symbols up to
  and including it have been received. Same as TCP's cumulative ACK.
- **SACK ranges** [RFC2018]: out-of-order blocks received beyond the cumulative
  ACK. These tell the sender exactly which symbols arrived despite gaps.
- **Echo timestamp**: for RTT measurement.

**Advantages over the former NACK-based approach:**
- ACK-based protocols are proven more robust for unicast (TCP's 40-year track
  record). ACKs confirm what works; NACKs report what failed.
- The sender infers losses from gaps in SACK — no separate NACK message needed.
- If an ACK is lost, the sender simply waits longer and retransmits anyway
  (self-healing). No NackAck echo mechanism needed.
- SACK is a well-understood, widely-deployed mechanism [RFC2018].

### 3.5 Per-Symbol Delivery Outcomes

A lost symbol has three possible outcomes, depending on whether the taper
function's correction symbols recover it before the cutoff T_cut:

```
   Outcome 1: FEC-recovered (proactive repair arrives before T_retx)
     Latency: L_prop + small delay (≈ same as source)
     Probability: ε × P_fec

   Outcome 2: ARQ-recovered (retransmit arrives after T_retx, before T_cut)
     Latency: L_prop + L_arq  where L_arq = T_retx + RTT/2
     Probability: ε × (1 - P_fec) × P_arq

   Outcome 3: Lost (not recovered by T_cut)
     Latency: ∞ (never delivered)
     Probability: ε × (1 - P_fec) × (1 - P_arq)
```

Where P_arq = probability that a retransmitted correction symbol succeeds
within the cutoff. When ρ = 100% (T_cut = ∞), P_arq = 1 and outcome 3
never occurs.

The full delivery distribution:

```
   P(on-time delivery) = (1 - ε) + ε × P_fec           not lost, or FEC
   P(late delivery)    = ε × (1 - P_fec) × P_arq       ARQ retransmit
   P(permanent loss)   = ε × (1 - P_fec) × (1 - P_arq) not recovered

   Reliability: ρ = 1 - P(permanent loss)
   Tail latency: δ = P(late delivery) / ρ               among delivered symbols
```

### 3.6 The Triangle in Action

Under **100% reliability** (ρ = 1, T_cut = ∞): outcome 3 never occurs.
"Tail loss from FEC" equals "tail latency events" — they are the same thing.
This is the special case from Section 6.

Under **variable reliability** (ρ < 1, T_cut < ∞): the taper is cut off.
Symbols beyond T_cut are permanently lost. This saves bandwidth (fewer
correction symbols) and bounds latency (no recovery beyond T_cut), at the
cost of reliability.

```
  ρ = 100%:  ─────────────────────────────── (taper runs until ACK)
             all symbols eventually delivered

  ρ = 98%:   ──────────────┐
             98% delivered  │ T_cut
             2% lost        └── (taper stops, accept loss)

  ρ = 95%:   ────────┐
             95%      │ T_cut (shorter)
             5% lost  └── accept loss (sensor/VoIP)
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
   ε̂_ewma(n) = α × (lost/sent) + (1-α) × ε̂_ewma(n-1)

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
   If ε̂ > ε_true:  over-provisioning → wasted bandwidth
   If ε̂ < ε_true:  under-provisioning → more ARQ latency events

   Overhead gap = (ε̂ - ε_true) / ε_true
```

BOCD minimizes this gap by adapting the estimation confidence to the regime:
- Steady state: ε̂ ≈ ε_true (tight posterior)
- Transition: ε̂ > ε_true (conservative, correct behavior)

---

## 6. The Optimization Problem

### 6.1 Formal Statement

```
   minimize:    r = A/q                     (correction rate = bandwidth cost)

   subject to:  ε × (1 - P_fec(A, q)) ≤ δ  (tail latency constraint)

   where:       τ(t) = A × (1-q)^t          (taper function)
                P_fec depends on A, q, ε, W  (FEC recovery probability)
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
   R(A, q) = Σ_{t=0}^{W-1} τ(t) × (1-ε)
           = A × (1-ε) × Σ_{t=0}^{W-1} (1-q)^t
           = A × (1-ε) × (1 - (1-q)^W) / q
```

For large W (window much larger than burst length): (1-q)^W ≈ 0, so:

```
   R(A, q) ≈ A × (1-ε) / q = r × (1-ε)
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
   ε × (1 - P_fec) ≤ δ
   ε × e^{-R} ≤ δ
   e^{-R} ≤ δ/ε
   -R ≤ ln(δ/ε)
   R ≥ ln(ε/δ)                    (note: ε > δ, so ln(ε/δ) > 0)
```

Using R ≈ A × (1-ε) / q:

```
   A × (1-ε) / q ≥ ln(ε/δ)

   A* = q × ln(ε/δ) / (1-ε)

   r* = A*/q = ln(ε/δ) / (1-ε)
```

### 6.4 The Optimal Correction Rate Formula

```
  ┌───────────────────────────────────────────┐
  │                                           │
  │   r* = ln(ε/δ) / (1-ε)                   │
  │                                           │
  │   where:                                  │
  │     ε = average loss rate (from BOCD)     │
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
   A* = r* × q = q × ln(ε/δ) / (1-ε)
```

And the **complete taper function** is:
```
   τ*(t) = A* × (1-q)^t = q × ln(ε/δ) / (1-ε) × (1-q)^t
```

### 6.5 Comparison with Information-Theoretic Minimum

The IT minimum (Shannon limit for the erasure channel [Shannon1948]) is:

```
   r_IT = ε / (1-ε)
```

Our optimal rate:

```
   r* = ln(ε/δ) / (1-ε) = r_IT × ln(ε/δ) / ε = r_IT × ln(1/δ)/ε + r_IT × ln(ε)/ε
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
   R = N_correction × (1-ε) = (A/q) × (1-(1-q)^W) × (1-ε)
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
   For large W: K ≈ εW with variance determined by burstiness
```

For recovery: number of arriving repairs ≥ K.

Repairs arrive = Binomial(r × W, 1-ε), approximately Normal(rW(1-ε), rW(1-ε)ε).

Constraint: P(repairs < K) ≤ δ.

Using normal approximation:
```
   P(repairs < K) ≈ P(Normal(rW(1-ε), rW(1-ε)ε) < K)
```

This requires the δ-quantile of repairs to exceed the (1-δ)-quantile of losses.
The algebra gives:

```
   r ≥ ε/(1-ε) + z_δ × √(ε/(W(1-ε)))
```

where z_δ is the standard normal quantile [Abramowitz1964] for probability δ.

### 6.7 Burst Variance Correction

The normal approximation in 6.6 assumes iid losses (Binomial variance). On a
GE channel, losses are correlated — bursts inflate the variance.

The GE autocorrelation decays with eigenvalue (1-p-q). The variance of losses
in a window of size W is:

```
   Var_iid(K) = W × ε × (1-ε)                    (independent losses)

   Var_GE(K)  = W × ε × (1-ε) × σ²_burst         (burst-correlated losses)

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
   T_cut such that: ε × (1 - P_fec(T_cut)) × (1 - P_arq(T_cut)) = 1 - ρ
```

For ρ = 100%: T_cut = ∞ (no cutoff). For ρ = 98%: solve for finite T_cut.

Step 2: From δ, find A using the tail latency constraint (Section 6.8):

```
   A* such that: ε × (1 - P_fec(A*)) ≤ δ    (among delivered symbols)
```

Step 3: Compute r* = A* × (1 - (1-q)^{T_cut+1}) / q.

**Special case ρ = 100%**: T_cut = ∞, r* = A*/q = ε/(1-ε) + z_δ√(...).
This is the formula from Section 6.8.

#### Mode 2: Given (r, ρ) → compute δ

Fix bandwidth r and reliability ρ. Compute resulting tail latency δ.

```
   From ρ: find T_cut (same as Mode 1, Step 1)
   From r and T_cut: A = r × q / (1 - (1-q)^{T_cut+1})
   From A: P_fec = 1 - exp(-R)  where R = A(1-ε)/q × (1-(1-q)^W)
   Result: δ = ε × (1 - P_fec) × P_arq / ρ
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

   At minimum r = r_IT = 2.6%:  δ = ε = 2.5% (every lost symbol goes to ARQ)
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
   P(decoder invoked) = 1 - (1-ε)^W
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
   ε_codec_eff = ε_codec × P(decoder invoked) = ε_codec × (1 - (1-ε)^W)
```

The corrected correction rate becomes:

```
   r* = (ε + ε_codec_eff)/(1-ε) + z_δ × √((ε + ε_codec_eff) / (W × (1-ε)))
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
   shared_cost = r*(ε, min(δ_c))          (one encoder, tightest δ)
   separate_cost = Σ_c f_c × r*(ε, δ_c)  (per-class, weighted by fraction f_c)
```

Choose whichever is lower.

### 8.4 Extending the Formula

For shared FEC with per-symbol δ, the constraint becomes:

```
   For each class c: ε × (1 - P_fec) ≤ δ_c
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

## Appendix A: Summary of Key Formulas

```
   Channel (Section 2):
     ε = p/(p+q)                          average loss rate          [probability]
     B = 1/q                              mean burst length          [symbols]
     P(burst ≥ t) = (1-q)^{t-1}          burst survival             [probability]
     σ²_burst = 1 + 2(1-p-q)/(p+q)       burst variance inflation   [dimensionless]

   Taper function (Section 4):
     τ*(t) = A* × (1-q)^t                optimal taper function     [corrections/symbol]
     A* = r* × q                          taper amplitude (ρ=100%)  [corrections/symbol]
     τ(t) = 0 for t > T_cut              taper cutoff (ρ<100%)

   Optimal correction rate (Section 6.8, ρ=100%):
     r* = ε̂/(1-ε̂) + z_δ × √(ε̂ × σ²_burst / (W(1-ε̂)))            [ratio]
     where ε̂ = ε + ε_codec × (1-(1-ε)^W)  effective loss            [probability]
           z_δ = Φ⁻¹(1-δ)                  normal quantile           [dimensionless]

   Three-variable optimization (Section 6.10):
     Given (δ, ρ) → r:  find T_cut from ρ, find A from δ, r = A(1-(1-q)^T_cut)/q
     Given (r, ρ) → δ:  find T_cut from ρ, compute A from r, δ = ε(1-P_fec)P_arq/ρ
     Given (r, δ) → ρ:  iterate T_cut until budget constraint met, ρ = recovery within T_cut

   Per-symbol delivery (Section 3.5):
     P(on-time)   = (1-ε) + ε × P_fec                               [probability]
     P(late)      = ε × (1-P_fec) × P_arq                           [probability]
     P(lost)      = ε × (1-P_fec) × (1-P_arq) = 1-ρ                [probability]

   Retransmit buffer (Section 3.3):
     T_retx ≈ RTT + margin               retransmit timeout         [seconds]
     L_arq = T_retx + RTT/2              ARQ recovery latency       [seconds]

   Correction symbol per-slot decision (Section 3.2):
     if retransmit_buffer.peek().age > T_retx:
       send exact source retransmit       (preferred: immediate decode)
     else:
       send random repair symbol          (FEC fallback: needs decoder)
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

### C.1 Correction Symbol Preemption of Source Data [Mehrotra2010]

The base model treats source and correction as independent streams. Mehrotra & Li
show that it is sometimes optimal to **delay source packets to send correction
symbols first**. During a detected burst, sending source into a known-bad channel
wastes bandwidth. Sending correction for already-transmitted source recovers data
without ARQ latency.

**Extension:** Allow the taper function to exceed 1.0:

```
   τ(t) = A × (1-q)^t

   When τ(t) > 1.0: send more correction than source at this offset.
   This means pausing source transmission to send correction instead.
```

This naturally occurs when A is large (high loss) and t is small (burst just
started). The sender observes the GE state via recent loss observations and
increases τ(t) when in a detected burst.

**Decision rule:** Preempt source with correction when the expected value of
sending correction (prevents one ARQ round-trip) exceeds the cost of delaying
source (adds one slot of latency):

```
   preempt when:  P(burst active) × L_arq > L_slot
   i.e., when:    (1-q)^t × (T_retx + RTT/2) > 1/throughput
```

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
   Current model:  P_fec ≈ 1 - Φ(-z) where z depends on (ε, W, σ²_burst)
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
   P(repair arrives on at least one path) = 1 - Π(εᵢ)

   Single path:  P(arrive) = 1-ε
   Dual path:    P(arrive) = 1-ε² ≈ 1 for small ε
```

This means the multipath taper can use a **lower amplitude** A than single-path
for the same P_fec target. The optimal multipath taper:

```
   τ_multi(t) = A_multi × (1-q)^t

   where A_multi = A_single × (1-ε) / (1-Π(εᵢ))
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
   Floor dominates when loss is low but bursts are long (large B, small ε).
```

### C.6 Summary of Extensions

| Extension | From | Effect on correction rate | Effect on P_fec accuracy |
|-----------|------|-------------------------|--------------------------|
| Correction preemption | [Mehrotra2010] | Reduces delay during bursts | Improves burst recovery |
| Information debt | [RLC_GE2025] | More precise | Exact (Markov chain) |
| Analytical bounds | [Vajha2020] | Verification only | Bounds, not point estimate |
| Multipath diversity | [Zeng2021] | Reduces A_multi | Higher for same budget |
| DCSW taper floor | [Badr2017] | Hard minimum guarantee | Worst-case protection |

## Appendix D: Open Questions

1. **Finite window truncation:** The taper is theoretically infinite-tailed
   but the encoder window has finite size W. What's lost by truncation?
   For W >> B (window much larger than mean burst), the truncation error is
   (1-q)^W which is negligible. For W ≈ B, it may matter.

2. **Multi-path:** With multiple paths, losses are correlated differently
   per path. Should each path have its own taper, or should there be a
   joint taper across paths? See [Facenda2022] for delay spectrum concepts
   in multi-link streaming.

3. **Interaction with congestion control:** The spare_capacity gate limits
   correction rate. When r* > spare_capacity, we can't achieve the tail target.
   How should the system signal this to the application?

4. **Normal approximation validity:** Even with the burst variance correction
   (σ²_burst), the normal approximation to the loss count may be inaccurate
   for small windows or very bursty channels. Could we use the exact GE
   distribution (computable from the transition matrix) for higher precision?

5. **Optimal T_retx tuning:** The retransmit timeout T_retx trades off between
   premature retransmission (wasting correction slots on symbols that will be
   ACKed) and delayed recovery (waiting too long to retransmit genuinely lost
   symbols). The optimal T_retx likely depends on RTT variance and loss rate.

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
