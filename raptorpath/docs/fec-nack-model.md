# FEC/NACK Mathematical Foundation

A principled model for optimal repair allocation in raptorpath.

---

## 1. System Model

### 1.1 Components

```
  Sender                          Channel                        Receiver
 ┌──────────┐                  ┌───────────┐                  ┌──────────┐
 │          │   source syms    │           │   surviving      │          │
 │  Source  ├─────────────────►│           ├─────────────────►│  Decode  │
 │ packets  │                  │  Erasure  │                  │          │
 │          │   repair syms    │  Channel  │   surviving      │          │
 │  FEC     ├─────────────────►│  (GE)     ├─────────────────►│  FEC     │
 │ Encoder  │                  │           │                  │ Decoder  │
 │          │                  │           │                  │          │
 │          │◄─── ACK ─────────┤           │◄─── ACK ────────┤          │
 │          │◄─── NACK ────────┤           │◄─── NACK ───────┤  Gap     │
 │          ├──── NackAck ────►│           ├──── NackAck ───►│ Detector │
 │          │                  │           │                  │          │
 │          ├──── NACK repair ►│           ├──── repair ────►│          │
 └──────────┘                  └───────────┘                  └──────────┘
```

The system provides **100% reliable** delivery. Every source symbol eventually
reaches the receiver. The only question is **when** — the delivery latency.

Two recovery mechanisms:

| Mechanism | Direction  | Bandwidth cost        | Latency cost              |
|-----------|------------|-----------------------|---------------------------|
| FEC       | Proactive  | Always-on (per source)| Zero (arrives with source)|
| NACK      | Reactive   | Per-event (only loss) | detection_delay + RTT     |

### 1.2 Notation

| Symbol | Meaning | Unit |
|--------|---------|------|
| ε      | Average channel loss rate | probability |
| p      | P(Good → Bad) in GE model | probability |
| q      | P(Bad → Good) in GE model | probability |
| B      | Mean burst length = 1/q | symbols |
| W      | Encoder window size | symbols |
| RTT    | Round-trip time | seconds |
| D_nack | NACK detection delay | seconds |
| r      | Total repair rate (repair symbols per source symbol) | ratio |
| τ(t)   | Taper function: repair density at offset t | repairs/symbol |
| A      | Taper amplitude (scaling factor) | repairs/symbol |
| P_fec  | Probability a lost symbol is FEC-recovered | probability |
| δ      | Tail latency target: P(late delivery) ≤ δ | probability |
| L_prop | Propagation delay (base latency) | seconds |
| L_nack | NACK recovery latency = D_nack + RTT | seconds |

### 1.3 What We Control vs What We Optimize

```
  INPUTS (we control)              OPTIMIZATION              OUTPUTS
 ┌─────────────────────┐         ┌──────────────┐         ┌──────────────┐
 │ δ = tail latency    │         │              │         │              │
 │     target          │────────►│  Minimize r  │────────►│ r* = optimal │
 │ (from protocol hint)│         │  subject to  │         │   repair rate│
 │                     │         │  P(late) ≤ δ │         │              │
 └─────────────────────┘         │              │         │ τ*(t) = opt. │
 ┌─────────────────────┐         │              │         │   taper func │
 │ Channel observations│────────►│              │         │              │
 │ (loss, RTT, bursts) │         │              │         │ P_fec = FEC  │
 └─────────────────────┘         └──────────────┘         │   recovery   │
                                                          │   probability│
                                                          └──────────────┘
```

**Input**: The tail latency target δ is the single control knob. It is set by
the protocol hint:
- Realtime: δ very small (almost everything FEC-recovered, minimal NACK latency)
- Bulk: δ larger (allow more NACK recovery, save bandwidth)
- Auto: moderate δ

**Output**: The taper function τ*(t) that achieves minimum bandwidth (repair
rate r*) while satisfying the tail latency constraint.

---

## 2. Channel Model

### 2.1 Gilbert-Elliott Two-State HMM

The channel alternates between Good (low loss) and Bad (high loss) states
[Gilbert1960], [Elliott1963]:

```
         p                              q
   ┌──────────►┐                  ┌──────────►┐
   │           │                  │           │
 ┌─┴─┐      ┌─┴─┐              ┌─┴─┐      ┌─┴─┐
 │ G │      │ B │              │ B │      │ G │
 │   │◄─────┤   │              │   │◄─────┤   │
 └───┘  q   └───┘              └───┘  p   └───┘
   │           │
   │ 1-p       │ 1-q
   └───────────┘
        self-loops

   p = P(Good → Bad)    = probability of entering a burst
   q = P(Bad → Good)    = probability of exiting a burst
   1-p = P(Good → Good) = probability of staying in Good
   1-q = P(Bad → Bad)   = probability of burst continuing
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

### 3.1 FEC (Forward Error Correction)

FEC generates **repair symbols** — linear combinations of source symbols in
the encoder window. Repair symbols are sent proactively, before knowing what
will be lost.

```
  Time ──────────────────────────────────────────────►

  Source:  [S1] [S2] [S3] [S4] [S5] [S6] [S7] ...
  Repair:    [R1]  [R2]     [R3]        [R4]   ...
                ↑     ↑        ↑            ↑
                │     │        │            │
         covers S1-S3  covers S1-S4  covers S3-S6  covers S4-S7
         (window)      (window)      (window)      (window)
```

If S3 is lost, it can be recovered from any repair symbol whose coding window
includes S3, provided enough linearly independent equations arrive.

**Properties:**
- Cost: r repair symbols per source symbol (always, whether needed or not)
- Latency: zero additional (repair arrives at roughly the same time as source)
- Bandwidth: costs r/(1+r) fraction of link capacity

### 3.2 NACK (Negative Acknowledgement)

When the receiver detects a gap in the sequence, it sends a NACK listing the
missing symbols. The sender retransmits.

```
  Time ──────────────────────────────────────────────────────►

  Sender:  [S1] [S2] [S3] [S4] [S5] ... wait ... [S3']
                       ↓ lost                        ↑ retransmit
  Receiver: S1   S2   gap  S4   S5  ... detect ...
                                          │
                                    NACK {S3} ──────┘
                                          │
            ├──────── D_nack ────────────►├── RTT ──►│
            │         detection           │  round   │
            │         delay               │  trip    │
```

**Properties:**
- Cost: ≈ ε per source symbol (only retransmit what's actually lost)
- Latency: D_nack + RTT per recovery event
- Amortization: one NACK can cover multiple gaps → cost per symbol decreases
  for burst losses

### 3.3 Per-Symbol Delivery Latency

Since the system guarantees 100% reliability, every symbol is delivered.
The delivery latency for symbol s has two cases:

```
   L(s) = L_prop                              if s not lost, or FEC-recovered
   L(s) = L_prop + D_nack + RTT              if NACK-recovered
```

The delivery latency distribution:

```
   P(L(s) = L_prop)              = (1 - ε) + ε × P_fec
   P(L(s) = L_prop + L_nack)     = ε × (1 - P_fec)

   where L_nack = D_nack + RTT
```

### 3.4 Why "Tail Loss" = "Tail Latency"

Since reliability is 100%, there is no permanent loss. What we informally call
"tail loss from FEC" is actually "symbols that FEC didn't recover" — these
symbols get NACK-recovered with additional latency. Therefore:

**Tail loss probability from FEC = tail latency event probability.**

```
   P(late delivery) = P(L(s) > L_prop) = ε × (1 - P_fec)
```

This is the single quantity we optimize. There is no separate "loss vs latency"
tradeoff — they are the same thing under 100% reliability.

---

## 4. The Taper Function

### 4.1 Definition

The taper function τ(t) specifies the repair density at time offset t from
a source symbol. At offset t after symbol s enters the window, we generate
τ(t) repair symbols covering s.

```
  Repair
  density
  τ(t)
    │
  A ┤ ╲
    │   ╲
    │    ╲
    │     ╲
    │      ╲
    │       ╲──────────────────────── (never reaches 0)
    │
    └──────────────────────────────── time offset t
    0      B     2B     3B    ...

  τ(t) = A × (1-q)^t

  A = amplitude (what we solve for)
  (1-q)^t = GE burst survival function
```

### 4.2 Why Match the Loss Distribution?

The taper should allocate more repair where loss is more likely. Given that a
symbol was lost (we're in a burst), the conditional probability that the burst
is still active at offset t is:

```
   P(burst active at offset t | burst at offset 0) = (1-q)^t
```

The optimal repair allocation is proportional to this conditional probability.
This is the **water-filling solution**: given a fixed budget, allocate resources
proportional to the probability of needing them.

**Proof sketch (Lagrange multipliers):** We want to maximize P_fec given a
fixed total repair budget r. The marginal benefit of a repair symbol at offset
t is proportional to P(burst still active at t). The Lagrangian is maximized
when the repair density is proportional to (1-q)^t. This water-filling
principle is analogous to the delay-optimal streaming code constructions
in [Badr2017] and [Fong2019].

For an i.i.d. channel (q = 1, no burst memory): τ(t) = constant (flat taper).
This is correct — every position is equally likely to need repair.

### 4.3 Total Repair Rate

The total repair rate (repair symbols per source symbol) is:

```
   r = Σ_{t=0}^{∞} τ(t) = A × Σ_{t=0}^{∞} (1-q)^t = A / q
```

Since 0 < q ≤ 1, this geometric series converges. Therefore:

```
   A = r × q
```

The amplitude is uniquely determined by the repair rate and the GE parameter.

### 4.4 The Taper Never Reaches Zero

The exponential (1-q)^t is always positive for 0 < q < 1. This is correct
behavior: there is always a nonzero probability of a burst still continuing.
As long as a symbol has not been ACK'd, there is a nonzero probability it
was lost, so we should continue generating (increasingly rare) repair coverage.

```
  t = 0:    τ(0) = A                        peak repair density
  t = B:    τ(B) = A × e^{-1} ≈ 0.37 × A   one mean burst length
  t = 2B:   τ(2B) = A × e^{-2} ≈ 0.14 × A  two mean burst lengths
  t = 5B:   τ(5B) = A × e^{-5} ≈ 0.007 × A five mean burst lengths
  t → ∞:    τ(t) → 0                        but never zero
```

In practice, once a symbol is ACK'd, we stop generating repair for it (the
encoder window advances past it). The theoretical infinite tail is truncated
by the ACK mechanism.

### 4.5 Real-Time Adaptation

The taper function adapts in real time through two mechanisms:

1. **GE parameter updates**: The estimator continuously tracks q (and p) from
   observed loss patterns. As q changes, the taper shape changes — slower
   decay for longer bursts, faster decay for shorter bursts.

2. **BOCD changepoint detection**: If the loss regime changes abruptly (e.g.,
   path switches from WiFi to LTE), BOCD detects the changepoint within 5-15
   batches and widens the posterior, increasing the repair budget until the
   new regime is characterized.

```
  Before changepoint:           After changepoint:
  (short bursts, q=0.5)         (long bursts, q=0.2)

  τ(t)                          τ(t)
    │                             │
  A ┤╲                          A'┤ ╲
    │  ╲                          │  ╲
    │   ╲                         │    ╲
    │     ╲                       │      ╲
    │       ──────                │         ╲──────────
    └─────────── t                └──────────────────── t
    fast decay                    slow decay, higher A'
```

---

## 5. Estimation — From Observations to Channel Parameters

### 5.1 What We Observe

At the sender, we receive periodic feedback:

| Observation | Source | Frequency |
|-------------|--------|-----------|
| (sent, received) per batch | ACK messages | Every batch (~10-100ms) |
| RTT | Echoed timestamps in ACK | Every batch |
| Gap ranges (missing seqs) | WindowNack | Every report interval (2s) |
| NackAck receipt | Sender echo of NACK | Per NACK cycle |
| Throughput | Delivery rate tracking | Continuous |

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
  Observation stream:   ●●●●●●●●●○○○○●●●●●●●●●●●●●●○○○●●
                        ←  regime 1  →← regime 2 →← r3 →
                                     ↑            ↑
                              changepoint    changepoint

  Run-length distribution P(r_t | data):

  Regime 1 (steady):     Changepoint:        Regime 2 (steady):
  Mass at r=50           Mass splits:         Mass at r=20
  (confident)            r=0 (new regime)     (confident again)
                         r=51 (old continues)
  ┌─┐                    ┌─┐    ┌─┐           ┌─┐
  │ │                    │ │    │ │           │ │
  ──┴──── r              ──┴────┴──── r       ──┴──── r
    50                   0      51               20
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

### 5.6 RX Path Loss Estimation

The feedback channel (receiver → sender) may also be lossy. If NACKs get lost,
the sender never knows to retransmit.

**Measurement:** Sender echoes NackAck for each WindowNack received. The
receiver tracks how many NACKs it sent vs how many NackAcks came back:

```
   ε_rx = (nacks_sent - nack_acks_received) / nacks_sent
```

**NACK effectiveness:** A NACK round-trip requires the NACK to survive the
reverse path AND the repair to survive the forward path:

```
   nack_effectiveness = (1 - ε_rx) × (1 - ε_tx) ≈ (1 - ε_rx)²
```

(Approximating ε_tx ≈ ε_rx for symmetric paths, or using separate estimates.)

When nack_effectiveness is low, NACK is unreliable and FEC must compensate.

### 5.7 Estimation Error and Overhead

Estimation error directly maps to overhead:

```
   If ε̂ > ε_true:  over-provisioning → wasted bandwidth
   If ε̂ < ε_true:  under-provisioning → more NACK latency events

   Overhead gap = (ε̂ - ε_true) / ε_true
```

BOCD minimizes this gap by adapting the estimation confidence to the regime:
- Steady state: ε̂ ≈ ε_true (tight posterior)
- Transition: ε̂ > ε_true (conservative, correct behavior)

---

## 6. The Optimization Problem

### 6.1 Formal Statement

```
   minimize:    r = A/q                     (repair rate = bandwidth cost)

   subject to:  ε × (1 - P_fec(A, q)) ≤ δ  (tail latency constraint)

   where:       τ(t) = A × (1-q)^t          (taper function)
                P_fec depends on A, q, ε, W  (FEC recovery probability)
```

**Input:** δ (tail latency target, from protocol hint)

**Output:** A* (optimal taper amplitude), r* = A*/q (optimal repair rate)

### 6.2 FEC Recovery Probability

Consider a symbol lost at position 0. Repair symbols generated at offsets
t = 0, 1, 2, ... each have:
- Probability τ(t) of being generated (fractional: may or may not generate one)
- Probability (1-ε) of surviving the channel

The expected number of repair symbols covering the lost position that arrive:

```
   R(A, q) = Σ_{t=0}^{W-1} τ(t) × (1-ε)
           = A × (1-ε) × Σ_{t=0}^{W-1} (1-q)^t
           = A × (1-ε) × (1 - (1-q)^W) / q
```

For large W (window much larger than burst length): (1-q)^W ≈ 0, so:

```
   R(A, q) ≈ A × (1-ε) / q = r × (1-ε)
```

**Recovery model:** The number of useful repair symbols arriving is approximately
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

### 6.4 The Optimal Repair Rate Formula

```
  ┌───────────────────────────────────────────┐
  │                                           │
  │   r* = ln(ε/δ) / (1-ε)                   │
  │                                           │
  │   where:                                  │
  │     ε = average loss rate (from BOCD)     │
  │     δ = tail latency target               │
  │     r* = optimal repair rate              │
  │                                           │
  └───────────────────────────────────────────┘
```

**Properties:**
- r* depends only on ε and δ, not on q (burst length doesn't affect the TOTAL
  repair budget, only its distribution over time via the taper shape)
- As δ → 0 (tighter tail): r* → ∞ (need infinite FEC for zero NACK events)
- As δ → ε (loose tail = every loss goes to NACK): r* → 0 (no FEC needed)
- As ε → 0 (perfect channel): r* → 0 (no repair needed)

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

**The issue:** Our Poisson approximation is too pessimistic. A single repair
symbol at offset t doesn't independently have probability τ(t) of existing —
the repair symbols are generated deterministically by the taper schedule.
The correct model needs to account for the fact that multiple repair symbols
from the taper collectively protect the lost symbol.

### 6.6 Corrected Model

Let's reconsider. The taper generates repair symbols at known positions. For a
window of size W, the taper generates exactly r × W repair symbols total. The
question is: given that a source symbol is lost, how many repair symbols
covering it will arrive at the receiver?

In a sliding window code, repair symbols are linear combinations of all source
symbols in the window. A repair at offset t from source s covers s as long as
s is still in the window (t < W).

Number of repair symbols generated while s is in the window:
```
   N_repair = Σ_{t=0}^{W-1} τ(t)
```

Of these, each survives independently with probability (1-ε). Expected arrivals:
```
   R = N_repair × (1-ε) = (A/q) × (1-(1-q)^W) × (1-ε)
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

### 6.8 The Corrected Optimal Repair Rate

```
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │   r* = ε/(1-ε) + z_δ × √(ε × σ²_burst / (W × (1-ε)))      │
  │         ╰─┬──╯   ╰──────────────┬─────────────────╯         │
  │      IT minimum           tail margin                        │
  │                  (accounts for burst correlation)             │
  │                                                              │
  │   σ²_burst = 1 + 2(1-p-q)/(p+q)                             │
  │                                                              │
  │   z_δ = Φ⁻¹(1-δ)  (standard normal quantile)                │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
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

The corrected repair rate becomes:

```
   r* = (ε + ε_codec_eff)/(1-ε) + z_δ × √((ε + ε_codec_eff) / (W × (1-ε)))
```

### 7.3 Impact on METTLE at DC

Without weighting: r* includes 15% codec overhead → 16.1% repair rate.
With weighting: ε_codec_eff = 0.15 × 0.049 = 0.74% → 1.8% repair rate.

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
Each stream has its own taper. No repair sharing. Simple.

**Before FEC (shared stream):**
```
  All packets ──► [mixed FEC encoder] ──► channel
```
One taper covers everything. Repair symbols are linear combinations of ALL
source symbols [RFC8681] — a repair can recover ANY lost symbol regardless of class.

**Advantage of shared:** repair symbols are fungible. A repair generated "for"
a Realtime symbol can recover a Bulk symbol if needed. Total repair budget can
be lower than the sum of separate budgets (statistical multiplexing).

**Disadvantage of shared:** the taper must be sized for the tightest class.
If 1% of traffic is Realtime (δ=1e-6) and 99% is Bulk (δ=1e-2), you pay
Realtime-level overhead on everything.

### 8.3 When Shared Wins

Shared FEC is cheaper when the traffic mix is balanced or dominated by the
tight class. Separate streams are cheaper when the tight class is a small
fraction. The crossover depends on the specific δ values and loss rate.

**Decision rule:** Compare total repair bandwidth:
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
     2. Generate repair schedule from taper: {repair_1, repair_2, ...}
     3. Apply channel loss to both source and repair
     4. For each lost source symbol, check if enough repair arrived
     5. Count NACK events (symbols not FEC-recovered)
     6. Measure: actual_nack_fraction = nack_events / N

   Over many trials:
     Verify: P(actual_nack_fraction > δ) is small
     Verify: mean repair rate ≈ r*
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
| ε = 0 (no loss) | r* = 0 | No repair needed |
| ε → 1 (total loss) | r* → ∞ | Can't recover anything with FEC alone |
| δ = ε (every loss to NACK) | r* = 0 | No FEC needed, all NACK |
| δ → 0 (zero NACK tolerance) | r* → ∞ | Must FEC-recover everything |
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
   Channel:
     ε = p/(p+q)                          average loss rate
     B = 1/q                              mean burst length
     P(burst ≥ t) = (1-q)^{t-1}          burst survival

   Taper:
     τ*(t) = A* × (1-q)^t                optimal taper function
     A* = r* × q                          taper amplitude

   Burst variance correction:
     σ²_burst = 1 + 2(1-p-q)/(p+q)        variance inflation from burst correlation

   Optimal repair rate:
     r* = ε̂/(1-ε̂) + z_δ × √(ε̂ × σ²_burst / (W(1-ε̂)))
     where ε̂ = ε + ε_codec × (1-(1-ε)^W)  effective loss with codec overhead
           z_δ = Φ⁻¹(1-δ)                  normal quantile for tail target

   Tail latency:
     P(late delivery) = ε × (1 - P_fec) ≤ δ

   NACK effectiveness:
     nack_eff = (1-ε_rx) × (1-ε_tx)       probability NACK round-trip succeeds

   Budget split:
     total = r*
     proactive = r* - nack_expected
     nack_cap = nack_expected = historical_nack_rate × nack_eff
```

## Appendix B: Open Questions

1. **Finite window truncation:** The taper is theoretically infinite-tailed
   but the encoder window has finite size W. What's lost by truncation?
   For W >> B (window much larger than mean burst), the truncation error is
   (1-q)^W which is negligible. For W ≈ B, it may matter.

2. **Multi-path:** With multiple paths, losses are correlated differently
   per path. Should each path have its own taper, or should there be a
   joint taper across paths? See [Facenda2022] for delay spectrum concepts
   in multi-link streaming.

3. **Interaction with congestion control:** The spare_capacity gate limits
   repair rate. When r* > spare_capacity, we can't achieve the tail target.
   How should the system signal this to the application?

4. **Normal approximation validity:** Even with the burst variance correction
   (σ²_burst), the normal approximation to the loss count may be inaccurate
   for small windows or very bursty channels. Could we use the exact GE
   distribution (computable from the transition matrix) for higher precision?

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

### Information Theory

- **[Shannon1948]** C.E. Shannon, "A mathematical theory of communication,"
  *Bell System Technical Journal*, vol. 27, pp. 379-423, 623-656, 1948.
  The erasure channel capacity result C = 1-ε gives the IT minimum repair
  rate r_IT = ε/(1-ε) used throughout this paper.
