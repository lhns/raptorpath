# Mathematical Verification Plan

This document lists every mathematical formula, claim, and break-even
point in the FEC-ARQ model paper that requires verification through
simulation, numerical testing, or analytical proof.

Status legend: ❌ not tested | ⚠️ partially tested | ✅ verified

---

## 1. Core Formulas

### 1.1 P_lost(t) — Bayesian Loss Confidence (Section 3.4)

```
P_lost(t) = ε / [ε + (1-ε) × P(RTT > t)]
P(RTT > t) = 1 - Φ((t - SRTT) / RTTVAR)
```

**Verification:**
- ❌ Compute P_lost at t = 0, SRTT/2, SRTT, 2×SRTT for each scenario
- ❌ Verify against Monte Carlo: simulate RTT distribution, measure
  empirical P(symbol lost | no ACK at time t)
- ❌ Check the concrete examples from the paper match:
  - WiFi (ε=0.025, SRTT=50ms): P_lost(0)=0.025, P_lost(50ms)=0.05,
    P_lost(60ms)=0.35, P_lost(70ms)=0.85, P_lost(80ms)=0.98

### 1.2 t_fec — FEC Recovery Time (Section 3.4)

```
t_fec = m / (A × (1-ε)) × t_sym
```

**Verification:**
- ❌ Simulate RLC encoding/decoding, measure actual time from loss to
  decode, compare to formula
- ❌ Verify worked examples:
  - 100 Mbps, 1200B symbols, A=0.04, ε=0.025, m=1: t_fec = 2.5ms
  - 10 Mbps: t_fec = 24.6ms
  - 1 Mbps: t_fec = 245ms
  - Burst m=5 at 100 Mbps: t_fec = 12.5ms
- ❌ Measure t_fec distribution (not just expected value) across
  1000+ simulation runs

### 1.3 Taper Function (Section 4)

```
τ(t) = A × (1-q)^t
Total rate: r = A/q
Amplitude: A = r × q
```

**Verification:**
- ❌ Verify geometric series convergence: Σ τ(t) for t=0..1000 ≈ A/q
- ❌ Verify flat taper when q=1 (iid channel): τ(t) = A = constant
- ❌ Verify for q=0.5: τ(0)=A, τ(1)=A/2, τ(2)=A/4 (exponential decay)
- ❌ Run simulation: measure actual correction density at each offset,
  compare to theoretical τ(t)

### 1.4 P_fec — Normal Approximation (Section 8.2)

```
P_fec = Φ(√W × (r(1-ε)-ε) / √(ε(1-ε)(r+σ²_burst)))
```

**Verification:**
- ❌ Monte Carlo: simulate W-symbol windows with GE loss, count how
  often FEC recovers all losses, compare to P_fec formula
- ❌ Boundary: at r = ε/(1-ε), verify P_fec ≈ 0.5 (coin flip)
- ❌ Boundary: at r >> ε/(1-ε), verify P_fec → 1.0
- ❌ Boundary: at r < ε/(1-ε), verify P_fec → 0.0
- ❌ Compare normal approximation to exact binomial for small W (W=5,10)
- ❌ Measure approximation error vs W: should decrease as W increases

### 1.5 σ²_burst — Burst Variance Factor (Section 8.3)

```
σ²_burst = 1 + 2(1-p-q)/(p+q)
```

**Verification:**
- ❌ Verify table values against formula:
  - DC (p=0.001, q=0.5): σ² = 3.0
  - WiFi (p=0.013, q=0.5): σ² = 2.9
  - LTE (p=0.056, q=0.2): σ² = 3.8 (verify p value!)
  - Satellite (p=0.01, q=0.1): σ² = 5.1 (verify p value!)
- ❌ Monte Carlo: simulate GE channel, measure actual variance of
  losses-per-window for W=50, compare to W×ε×σ²_burst
- ❌ Verify σ²_burst = 1.0 when p+q = 1 (iid channel)
- ❌ Verify σ²_burst → ∞ as p+q → 0 (extreme burstiness)

### 1.6 r* — Optimal Correction Rate (Section 8.4)

```
r* = ε/(1-ε) + z_δ × √(ε × σ²_burst / (W × (1-ε)))
```

**Verification:**
- ❌ Verify all 9 worked examples from Section 8.5:
  - DC Bulk: r* = 1.9%, DC Auto: 3.0%, DC Realtime: 3.8%
  - WiFi Bulk: 5.4%, WiFi Auto: 7.1%, WiFi Realtime: 8.4%
  - Satellite Bulk: 17.3%, Sat Auto: 21.7%, Sat Realtime: 25.0%
- ❌ Verify z_δ values: z(1e-2)=2.33, z(1e-4)=3.72, z(1e-6)=4.75
- ❌ Verify r* produces the claimed P_fec at the given δ
- ❌ Verify 1/√W scaling: double W → margin shrinks by √2

---

## 2. Three-Variable Optimization (Section 8.6)

### 2.1 Monotonicity Claims

- ❌ P(recovered by T_cut) is strictly increasing in T_cut
  - Test: sweep T_cut from 0 to 10W for each scenario, verify monotone
- ❌ ρ is monotone in T_cut
- ❌ r is monotone in T_cut for fixed (δ, ρ)

### 2.2 Binary Search Convergence

- ❌ Measure iterations needed for T_cut binary search across scenarios
  - Claim: "typically converges in ~20 iterations"
  - Test: count actual iterations for DC, WiFi, LTE, Satellite at
    ρ = 0.95, 0.99, 0.999, 0.9999

### 2.3 Mode Consistency

- ❌ Mode 1: given (δ, ρ) → compute r. Feed r and ρ into Mode 2 →
  should recover same δ. Test across all 5 scenarios.
- ❌ Mode 1: given (δ, ρ) → compute r. Feed r and δ into Mode 3 →
  should recover same ρ.
- ❌ Cycle: Mode 1 → Mode 2 → Mode 3 → Mode 1: all three should be
  consistent (closed loop within numerical tolerance)

### 2.4 δ Formula Correctness (Section 6.3)

```
δ = P(late delivery) / ρ
P(late) = ε × (1-P_fec) × P_arq
P_arq = 1 - (1-ρ) / (ε × (1-P_fec))
```

- ❌ Verify: P(on-time) + P(late) + P(lost) = 1.0 for all parameter sets
- ❌ Verify: δ = 0 when P_fec = 1.0 (FEC recovers everything)
- ❌ Verify: δ → ε when r → ε/(1-ε) and ρ = 1.0

---

## 3. Estimation and Adaptation (Section 7)

### 3.1 BOCD Convergence

- ❌ Claim: "adapts within 5-15 batches" after regime change
  - Test: simulate loss rate change (5% → 20%), measure batches until
    predictive_loss_upper is within 20% of true value
  - Test across batch sizes: 10, 50, 100 symbols per batch
  - Record: 5th percentile, median, 95th percentile convergence time

### 3.2 GE Parameter Estimation

- ❌ Claim: "bootstrap period ~50 symbols"
  - Test: feed known GE sequence (p=0.05, q=0.5), measure when
    estimated p, q are within 20% of true values
- ❌ Verify: estimated ε = p/(p+q) converges to true ε

### 3.3 Beta Posterior Quantile

- ❌ Verify beta_quantile (normal approximation) against exact Beta CDF
  for various (a, b, p) values
- ❌ Measure approximation error: when does normal approx break down?
  (small a or b?)

---

## 4. Codec Properties (Section 9)

### 4.1 Codec Overhead Weighting

```
P(decoder_invoked) = 1 - (1-ε)^W
e_codec_eff = e_codec × P(decoder_invoked)
```

- ❌ Verify: at ε=0, P(decoder_invoked)=0, effective overhead=0
- ❌ Verify: at ε=1, P(decoder_invoked)=1, effective overhead=e_codec
- ❌ Monte Carlo: simulate W-symbol windows, count decoder invocations,
  compare to formula

### 4.2 RLC Decoder Properties

- ❌ Verify: k source + m repairs → decode probability ≈ 1 - (1/256)^m
  for random GF(256) codes
- ❌ Verify: cascade recovery reduces effective m (window decoder decodes
  faster than block decoder for same m)
- ❌ Measure cascade benefit: for burst of m losses in window W, how
  many fewer repairs does window decoder need vs block decoder?

---

## 5. Section 14 Claims (Future Directions)

### 5.1 FEC Latency CDF — Poisson Model (Section 14.3)

```
λ(T) = A × (1-ε) × (1 - (1-q)^(T+1)) / q
P(t_fec ≤ T | m) = Q(m, λ(T))  [regularized incomplete gamma]
```

- ❌ Monte Carlo validation: simulate 10000 burst events, measure
  empirical t_fec distribution, compare to Poisson CDF
- ❌ Verify Poisson approximation validity: compare to exact
  Poisson-Binomial distribution for typical parameters
- ❌ Test edge cases: m=1, m=10, m=50

### 5.2 P_fec Model Consistency (Section 14.19)

```
As T → ∞: P(t_fec ≤ T | m) should approach P_fec from Section 8.2
```

- ❌ **CRITICAL**: compute both models for same (ε, q, r, W, σ²_burst),
  verify convergence across all scenarios
- ❌ Measure divergence at finite T: how large must T be for < 1% error?

### 5.3 Ambient Pipeline (Section 14.4)

```
λ_prior(T_w) = r × (1-ε) × T_w / (1+r)
```

- ❌ Simulate: pre-loss pipeline of repairs, verify λ_prior matches
  actual decoder equation count
- ❌ Verify: larger W → more ambient FEC → faster recovery
  (measure actual t_fec vs W for same burst length)

### 5.4 Optimal Window Size (Section 14.5)

```
W_min = 1 / (q × ε)    for mean burst
W_min(B_99) = B_99 × (1+r) / (r × (1-ε))
```

- ❌ Verify table values:
  - WiFi: W_min=40, B_99=7, W_min(p99)≈60
  - LTE: W_min=50, B_99=21, W_min(p99)≈150
  - Satellite: W_min=111, B_99=44, W_min(p99)≈350
- ❌ Simulation: sweep W from 10 to 500, measure P(full recovery) for
  each. Verify knee point matches W_min formula.

### 5.5 FEC vs ARQ Break-Even (Section 14.7)

```
FEC wins when: t_fec(W) < L_arq ≈ 1.5 × RTT
```

- ❌ Sweep RTT from 1ms to 1000ms, measure actual t_fec and L_arq,
  find crossover point
- ❌ Verify crossover matches the formula for each (ε, q) scenario
- ❌ Verify claim: "below W×t_sym ≈ RTT, FEC is strictly faster"

### 5.6 Proactive Retransmit vs FEC Dominance (Section 14.13)

```
FEC advantage ≈ W/1 at typical loss rates
Crossover at ε → 50%
```

- ❌ Simulate: same overhead r=0.5, compare recovery probability for
  (a) FEC with W=50 and (b) proactive retransmit (duplicate all)
- ❌ Sweep ε from 1% to 50%, find exact crossover point
- ❌ Verify W/1 advantage ratio at ε=5%, 10%, 20%

### 5.7 In-Burst FEC Survival (Section 14.15)

```
λ(T) = 0 for T < B (in-burst)
      = post-burst taper sum for T ≥ B
```

- ❌ Simulate GE burst events: verify zero FEC recovery during burst
- ❌ Verify pipeline effect: pre-burst repairs DO arrive and help
  (they were sent during Good state, arrive during/after burst)
- ❌ Measure: actual λ(T) vs corrected two-phase formula

### 5.8 Marginalized CDF (Section 14.14)

```
P(t_fec ≤ T) = Σ_{m=1}^{B_99} (1-q)^{m-1} × q × Q(m, λ(T))
```

- ❌ Compute marginalized CDF for each scenario
- ❌ Compare to empirical CDF from 10000 burst simulations
- ❌ Verify truncation error: P(m > B_99) contribution is < 1%

### 5.9 Estimator Feedback Stability (Section 14.18)

- ❌ Verify: estimator observes CHANNEL loss, not APPLICATION loss
  - Check code: record_batch(sent, received) counts raw arrivals
- ❌ Simulate feedback loop: vary r based on estimator, verify no
  oscillation over 10000 ticks
- ❌ Test regime change: ε shifts from 5% to 20%, verify r converges
  to new optimal without overshooting

### 5.10 Sequence-Aware P_lost (Section 14.22)

```
P_lost_seq(k, reorder_rate) = 1 - reorder_rate^k
P_lost_combined = max(P_lost_time(t), P_lost_seq(k, reorder_rate))
```

- ❌ Simulate FIFO channel: verify P_lost_seq(1) = 1.0 (one subsequent
  ACK proves loss)
- ❌ Simulate channel with 5% reorder: verify P_lost_seq(1) ≈ 0.95
- ❌ Compare combined P_lost to time-only P_lost: how much faster does
  combined detect loss?

### 5.11 Post-Burst FEC Boost (Section 14.23)

```
deficit = max(0, burst_length - repairs_in_pipeline)
boost_r = r + deficit / boost_duration
```

- ❌ Simulate: compare recovery time WITH boost vs WITHOUT boost
- ❌ Verify: boost shortens recovery by claimed amount
- ❌ Measure: does boost cause throughput dip (temporary r increase)?

---

## 6. Benchmark Cross-Validation

### 6.1 Benchmark vs Formula Predictions

Using existing benchmark results (docs/benchmark-results-*.json):

- ❌ For each (scenario, backend, config): compute predicted overhead
  from r* formula, compare to measured overhead_pct
- ❌ For each scenario: compute predicted recovery rate from P_fec,
  compare to measured recovery_rate
- ❌ For each scenario: compute predicted tail latency from t_fec,
  compare to measured p99_latency_ms

### 6.2 Visualizer vs Formula

- ❌ Run visualizer simulation with known parameters, compare:
  - Actual loss rate vs slider ε
  - Actual overhead vs predicted r*
  - Actual recovery % vs predicted ρ
  - Actual FEC recovery latency vs t_fec formula

---

## 7. B_max and Buffer Sizing

### 7.1 B_max Formula

```
B_max = ceil(ln(0.0001) / ln(1-q))
```

- ❌ Verify: B_max(q=0.5) = 14
- ❌ Verify: B_max ≈ 9.2/q (approximation)
- ❌ Simulate 100000 GE bursts: verify empirical P(burst > B_max) < 0.01%

### 7.2 Buffer Sizing

```
buffer_max = source_rate × T_cut           (ρ < 100%)
buffer_max = source_rate × B_max/(r*(1-ε)) (ρ = 100%)
```

- ❌ Verify: at ρ=100%, buffer never empties before symbol is recovered
- ❌ Verify: at ρ=95%, buffer correctly evicts symbols at T_cut

---

## Implementation Notes

**Test framework**: Create `raptorpath/tests/mathematical_verification.rs`
that implements all above tests using:
- The `raptorpath-math` crate for formula computation
- The `raptorpath-math::rlc` module for codec simulation
- Monte Carlo loops with fixed seeds for reproducibility

**Also create**: `raptorpath-wasm` tests that verify the wasm simulation
matches the mathematical predictions.

**Output format**: Each test should print a comparison table:
```
| Test | Formula | Simulation | Error | Pass? |
```

**Tolerance**: Most tests should pass within 5% relative error for
Monte Carlo (1000+ trials), or exact match for analytical formulas.
