# Knee margin, re-derived after s61's DISAGREES: what need scales with, and the allowance that follows

Status: DERIVATION. Docs and Python only: no engine code, no cargo, no VM. Branch `knee-margin-rederive` off
`github/main` `4559bbee`. It answers the registered consequence of the §61a re-run Result ("S_F is not the knee's
term, and the form is re-derived"), the roadmap node `store-law-g-rederive`, and the F07 ledger update that Result
says is owed. **Every reading here is post hoc.** None was registered, and §4 sketches the registration that would
turn the leads into results.

**For the owner, in plain words.**
- **The "constant 4 ms" lead is a property of how the minima are counted, not of the path.** The knee margin need is
  the window minimum at the knee minus the floor. As s61 reads it, the floor window holds about 280 echo samples and
  the knee window about 2 340, and a minimum falls as its window grows. So need is two things netted together:
  - L, how far load lifts the minimum at equal sample count. **L is jitter-sized, not constant:** ×1.57 from c2 to
    c3 (×1.41 on s61's first c3 session, ×1.89 with the floor at the intercept), against ×1.67 for netem's jitter.
  - N, how much the floor's minimum would fall if it held as many samples as the knee's. N is ×2.75 from c2 to c3
    (×2.24 on the first session), because c3's echo distribution has a longer thin lower tail.
  - They nearly cancel: 5.66 − 1.61 = 4.05 ms at c2, and 8.87 − 4.44 = 4.43 ms at c3.
  - Read every window as the engine sees it, need is 0.92× from c2 to c3, and "constant" is excluded there too
    (90 % [0.84, 0.97]).
- **Two cells cannot say which spread carries L.** netem's jitter, the floor's median-minus-minimum S_F and its
  p90 − p10 all fit within where the c3 floor is read. Constant and the loop-, rate- and packet-time families do not.
  c3's equal-n floor also rests on thin data (4 complete groups per W 12 row).
- **What I propose: keep decision 8's expression, A = g(δ)·S_F + q(δ)·F̂, as a measured cover, not as a law.**
  - At Bulk and at Realtime it covers both cells under every counting convention read here (§2.1). need/S_F is 0.80
    at c2 and 0.46–0.63 at c3.
  - The cost is over-allocation on long loops: at c3, 9.2 ms above the knee.
  - Its sentence ("the knee's minimum rises by a fixed fraction of S_F") is refuted. So F07 stays UNDERIVED, with a
    new evidence line.
  - q(δ)·F̂ survives unchanged, and σ leaves the allowance.
- **g's Realtime end can no longer come from need/S_F,** because the ratio is not shape-free. It becomes your
  price-table call. Decision 10 is unaffected.
- **One measurement would separate the candidates:** the c3 line at jitter 0 and 3 ms, on the same binary, with floor
  rows long enough for an equal-count floor. That is ≈ 3 h of VM (§4).

**Advisor (Fable) log.**
1. **After orientation, before writing.**
   - Reproduce the score first, then build Q1 on a table of conventions with intervals. The equal-n reading had
     already shown the ranking flipping.
   - Give every candidate its two-point exponent, and say that any affine form fits two points.
   - Check need − σ (A(c)'s shape).
   - Divide the datagram rate by gso before calling it a per-skb candidate.
   - Validate the new QDISC utilisation instrument where W₉₅ is known before reading c8, and report c8 as bounds.
   - Check s56/s57 for c1s `[SLAW]` rows rather than assume. They have none (§1.5).
   - Do not upgrade a fitted constant. Size any form to the upper replicate, and replay it through r68's gates, c1s
     direction included.
   - Lead Q3 with q_B·F̂ at c3. Write the F07 diff against `law-ledger.yaml`.
   - Do not build an order-statistics model of the floor.
   - All applied.

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place) | output | what it computes |
|---|---|---|---|
| `store-law/kmr.py` | `l1-raw/s60/rep60a-diag-ld.tgz`, `rep60b-diag-ld.tgz`, `l1-raw/s61/rep61c-diag-ld.tgz`, and `rep61a-diag-ld.tgz` loaded on its own (its row keys collide with `rep61c`'s); imports `r68allow.py` and `s61pred.py` (their loader and G(n*)) | `kmr.out` | §0 the witness; §1 need under five conventions with a bootstrap on need₃/need₂; §2 need = L − N, with groups per row; §2d its replication on `rep61a`; §2b the c3-shaped floor against load (c8 leg 1); §2c k = L/X per spread and ρ_L; §3 the candidate table; §4 per-leg link utilisation (c2/c3 validation, c8, c1s); §5 the forms replayed (D per hint, R67 at the controls, the c1s size); §5b form S under every convention |

`rep61b` (c2's add-on) is not read: its W 160 / 2 048 rows share keys with §60's c2 rows in `r68allow.load`, and
nothing here needs the price below c2's knee.

---

## 0. Witness (`kmr.out` §0)

The port reproduces the scored c3 reading to the printed digit:
- κ 0.00962, W\* 11.6, F_c 38.998 and S_F,c 9.709;
- the six W₉₅ and needs (4.256 / 4.498 / 4.404 / 4.464 / 3.610 / 4.478);
- ρ₃ 0.457;
- the NATIVE line: F 42.523, S_F 5.861, ρ 0.633.

c2's frozen inputs reproduce too: F 8.679, S_F 5.083, need 4.049, ρ₂ 0.797.

---

## 1. Q1: what the knee margin scales with

### 1.1 need under each convention (`kmr.out` §1)

```text
   need_c  =  median_r T̂min_r(W₉₅,r; n_K)  −  F_c(n_F, floor position)
```

need has two free conventions:
- the sample count each minimum is read at (n_F at the floor, n_K at the knee);
- where on the floor line F is read (c3 at W\* = u₂/κ, or at the intercept W → 0).

The bootstrap resamples replicates and floor rows within pools at both cells and refits the floor lines per draw
(2 000 draws, seed 62).

| convention | need₂ (ms) | need₃ (ms) | need₃ / need₂, 90 % |
|---|---|---|---|
| **registered** (c2 native; c3 G(250) floor, G(2 100) knee; u-matched) | 4.049 | 4.434 | **1.095 [0.961, 1.179]** |
| engine-native (every window as the engine's estimator sees it; u-matched) | 4.049 | 3.709 | 0.916 [0.838, 0.973] |
| registered, floor at the intercept | 4.585 | 5.623 | 1.226 [1.014, 1.390] |
| **equal n** (floor and knee both at ≈ 2 130–2 390 samples; u-matched) | 5.661 | 8.872 | **1.567 [1.479, 1.639]** |
| equal n, floor at the intercept | 6.183 | 11.691 | 1.891 [1.746, 2.020] |

- **"Constant in ms" lies inside only the registered row's interval.** The engine-native interval excludes 1 from
  below and the intercept interval from above.
- **At equal sample count, need is jitter-sized:** ×1.57 to ×1.89, against netem's jitter ×1.67 and S_F ×1.91.

### 1.2 The decomposition need = L − N (`kmr.out` §2)

```text
   need  =  [ T̂min(W₉₅; n_K) − F(n_K) ]  −  [ F(n_F) − F(n_K) ]   ≡   L − N
   L: the load rise at equal sample count (a path property under a FIFO with jitter, §1.4)
   N: the floor minimum's sample-count drop from n_F to n_K (a property of the estimator's counts and the echo tail)
```

| cell | T̂min(W₉₅) | F(n_F ≈ 280) | F(n_K ≈ 2 200) | **L** | **N** | need = L − N |
|---|---|---|---|---|---|---|
| c2 | 12.728 | 8.679 | 7.067 | **5.661** | **1.612** | 4.049 |
| c3 | 43.432 | 38.998 | 34.560 | **8.872** | **4.438** | 4.434 |
| ratio c3 / c2, 90 % | | | | **1.567 [1.479, 1.639]** | **2.753 [2.445, 3.167]** | 1.095 |

**Replication on s61's first c3 session** (`rep61a`, `kmr.out` §2d; post hoc, with K 104 / 112 / 124 and P1 136 as the
path exercise counted them, and the W 2 048 control read by goodput): need 4.376 (ρ 0.458), **L 7.989, N 3.613**.
- Against c2 that is need ×1.08, **L ×1.41, N ×2.24.**
- The shape of the decomposition replicates. Its size moves by about 0.9 ms between sessions, mostly in the n 2 100
  floor (W 12: 35.29 against 34.32).

**The equal-n floor is thin at c3.** Under G(2 100), a c3 W 12 row holds 4 complete groups (W 16: 5, W 24: 8),
against c2 W 16's 30 (`kmr.out` §2). The registration's floor rule asks ≥ 6 groups under G(250). So c3's F(n_K) is a
median of 4 minima per row, and its OLS line over the pools is the weakest number in this document.

**The floor's minimum against its sample count** (median over the floor pool's rows, `kmr.out` §2):

| n per group | ≈ 60 | ≈ 280 | ≈ 620–820 | ≈ 1 220–1 370 | ≈ 2 130–2 240 | ≈ 4 220–4 340 |
|---|---|---|---|---|---|---|
| c2 W 16 | – | 8.68 | 7.80 | 7.32 | 7.07 | 6.73 |
| c3 W 12 | 42.75 | 38.78 | 37.01 | 35.59 | 34.32 | 33.23 |

- c3's floor minimum falls 4.5 ms from n 280 to 2 100, against c2's 1.6 ms. That is ×2.75, against the spreads'
  ×1.67–1.91.
- **This refutes the registration's normalisation argument** (s61 §3, "Why matching n is the right normalisation").
  That argument assumed a window-minimum bias proportional to the cell's own spread ("J/√n"), so that equal counts
  on each side would leave ρ invariant under S's sentence. N's ratio says c3's lower tail is longer than its spread
  predicts.
  - The mechanism is not diagnosed here (advisor: no order-statistics model).
  - Candidates: correlated samples per ack or per skb (one jitter draw serves several echoes), and an echo-timing
    component outside netem's jitter. At c3 the floor's p50 sits ≈ 8.7 ms above the netem median of 40 ms, and at
    c2 ≈ 3.7 ms above 10 ms.
- **So DISAGREES does not by itself say that S_F misses the load term. It says that need, as read, mixes L with N,**
  and N does not scale with S_F.

### 1.3 The candidate scalings (`kmr.out` §3)

A one-parameter law need = k·X, with k fitted at c2, is a genuine one-point test at c3: X₃/X₂ must match need₃/need₂.
α = ln(need ratio)/ln(X ratio) is the two-point exponent (registered convention). **Any affine form c + k·X fits two
points exactly,** so two cells cannot reject an affine pair. The table rejects only single-term scalings.

| X | c2 | c3 | X₃/X₂ | α | inside the registered [0.96, 1.18] | inside equal-n, u-matched [1.48, 1.64] / intercept [1.75, 2.02] |
|---|---|---|---|---|---|---|
| **constant (ms)** | 1 | 1 | 1.00 | – | **yes** | no / no |
| S_F (floor p50 − min, registered counts) | 5.083 | 9.709 | 1.91 | 0.14 | no | no / **yes** |
| S_F at n 2 100 | 6.516 | 14.038 | 2.15 | 0.12 | no | no / no |
| floor p90 − p10 (native) | 5.535 | 8.169 | 1.48 | 0.23 | no | edge / no |
| netem jitter J (±J each way, the QDISC line) | 3 | 5 | 1.67 | 0.18 | no | edge (1.64) / no |
| σ at the knee (P90 − P50 at load) | 1.785 | 4.778 | 2.68 | 0.09 | no | no / no |
| loop floor F_c | 8.679 | 38.998 | 4.49 | 0.06 | no | no / no |
| netem propagation RTT | 10 | 40 | 4.00 | 0.07 | no | no / no |
| BDP in slots (W₉₅) | 168.0 | 108.7 | 0.65 | −0.21 | no | no / no |
| datagram service time at the link rate | 0.099 | 0.490 | 4.93 | 0.06 | no | no / no |
| per-skb service time (gso × datagram) | 0.589 | 2.697 | 4.58 | 0.06 | no | no / no |
| skb inter-arrival at the knee | 0.609 | 2.872 | 4.72 | 0.06 | no | no / no |
| ack inter-arrival at the knee | 0.437 | 2.216 | 5.07 | 0.06 | no | no / no |
| need − σ (A(c)'s c_B) | 2.264 | −0.344 | – | – | **shape fails**: c_B would be negative at c3 | – |

- **The rate and loop families are out under every convention** (×4–5). So are σ (×2.7) and the BDP in slots
  (×0.65).
- **A(c) as W1 wrote it (σ + c_B·q/q_B) fails in shape.** need − σ is 2.26 ms at c2 and −0.34 ms at c3. W1's
  c_B ≈ 2.7 ms had no source, and none is found here.
  - The constant that does fit the registered row is need itself, with no σ term.
  - c_B's value in §60's coordinates was 2.26 (r68 §1.5).

### 1.4 At equal n: which spread carries L (`kmr.out` §2c)

The sentence that makes L jitter-sized comes from netem's FIFO (`tools/l1/lib.sh:173–176`: with `rate` set, netem
schedules each packet no earlier than the previous one). Per skb i on the jittered direction:

```text
   X_i  =  max( j_i ,  X_{i−1} − (Δa_i − s_i) )        j_i ~ U(−J, +J), Δa the skb inter-arrival, s the skb service time
```

- Δa − s → 0 as the link saturates. A low draw j_i then waits behind the preceding high ones, so the window minimum
  climbs from −J toward +J.
- **At the knee it has risen by up to the jitter range 2J per jittered direction.** Acks are tiny and their drift is
  their spacing, so the ack direction absorbs part of its 2J too.
- This bounds the shape (L of the order of 2J). It does not give a coefficient, since no model is fitted here.

**k = L/X per cell, k₃/k₂ = 1 meaning that X carries L at both cells:**

| X | u-matched floor | floor at the intercept |
|---|---|---|
| netem 2J (one way) | 0.94 / 0.89, ratio **0.94** | 1.03 / 1.17, **1.13** |
| S_F (registered counts) | 1.11 / 0.91, **0.82** | 1.22 / 1.20, **0.99** |
| floor p90 − p10 (native) | 1.02 / 1.09, **1.06** | 1.12 / 1.43, **1.28** |
| S_F at n 2 100 | 0.87 / 0.63, **0.73** | 0.95 / 0.83, **0.88** |
| constant | 5.66 / 8.87, **1.57** | 6.18 / 11.69, **1.89** |

The intercept column rests on c3's G(2 100) floor line, whose slope (25 ± 4 ms per unit u) c8 leg 1 does not
corroborate (7 ± 0.7, §1.5). Read it as the weaker column.

- **s61's falsifier re-read with L** (`kmr.out` §2c). ρ_L = L/S_F, and d = ρ_L,3 − ρ_L,2 against s61's Δ = 0.085:

  | S_F read at | floor position | ρ_L,2 | ρ_L,3 | d, 90 % |
  |---|---|---|---|---|
  | the registered counts (**mixed**: L at n 2 100) | u-matched | 1.114 | 0.914 | −0.200 [−0.261, −0.154] |
  | the registered counts (mixed) | intercept | 1.183 | 1.119 | −0.063 [−0.168, +0.020] |
  | n 2 100 (one count) | u-matched | 0.869 | 0.632 | −0.237 [−0.267, −0.210] |
  | n 2 100 (one count) | intercept | 0.929 | 0.725 | −0.204 [−0.242, −0.175] |

  - Read at one count, S_F disagrees at equal n by about two-thirds of the registered gap (−0.34).
  - Only the mixed-count intercept reading is unresolved, and it rests on the weak column.
  - So equal counting does not rescue S_F as the knee's term. It narrows the gap.
- **What the two cells say at equal n:**
  - netem's 2J tracks L within 0.94–1.13 across the floor readings;
  - S_F (registered counts) within 0.82–0.99, and p90 − p10 within 1.06–1.28;
  - constant is ≥ 1.57 off.
  - 2J is a testbed parameter the engine cannot observe. S_F and p90 − p10 are its observable proxies, and they
    differ at c3 because c3's echo carries spread beyond netem's jitter (§1.2).

### 1.5 More knee points from existing ledgers (`kmr.out` §2b, §4)

**The instrument.** u_link = netem `Sent` bytes × 8 / (transfer seconds × link rate), per leg, from the driver's
QDISC lines. It counts FEC repair and retransmissions.
- **Validated where W₉₅ is known:**
  - c2: u_link at each replicate's W₉₅ is 0.943–0.948 (median 0.947);
  - c3: 0.937–0.945 (median 0.941).
  - So the goodput knee sits at u_link ≈ 0.94–0.95 at both cells.

**c8 leg 0 (c2-shaped)** gives no knee point.
- u_link is 0.564 at W 96 and 0.992 at W 256, with no arm between.
- So only the bracket need₀ ∈ [tmin(96) − F, tmin(256) − F] = [1.1, 6.3] ms is available. It contains both cells'
  values and decides nothing.

**c8 leg 1 (c3-shaped)** never reaches its knee.
- u_link is 0.865 at W 512 and 0.919 at W 1 280 (0.907–0.968 by row), so a lower bound is the most it gives.
- **At equal n, against its own floor,** L₁(u 0.92) ≥ 40.85 − 32.41 = 8.4 ms. c3's L at the same u_link is ≈ 7.8 ms.
  That is the same size, in another session.
- **Leg 1 is not a c3 floor.** At matched u_link it reads 2.2–2.6 ms below c3 under G(2 100) at u 0.55–0.9 (`kmr.out`
  §4: 36.54 at 0.57 against c3's 39.01 at 0.55; 37.87 at 0.76 against 40.15 at 0.73). It also reads 3.9 ms below
  under G(250) at u 0.23–0.31. That is a leg offset (a coupled leg sharing one sender), so its floor is not
  substituted for c3's.
- **Its slope corroborates c3's floor line at n 250.** Leg 1's own floor over u_link 0.03–0.31 rises 9.44 ± 0.63 ms
  per unit u under G(250). c3's G(250) floor line is 0.1021 ms/slot / κ = 10.6 ± 2.9 ms per unit u.
  - So the registered floor's slope is real, and the u-matched and intercept readings at n 250 differ by a real
    b·W\* ≈ 1.2 ms.
  - Under G(2 100) they do not agree: leg 1 gives 7.06 ± 0.72 and c3's line 25 ± 4. c3's n 2 100 floor (4–8 groups
    per row) is the suspect.

**c1s** has no knee.
- §60's c1s G arms reach u_link 0.22 / 0.38 / 0.50, and the W 2 048 controls 0.92 (all flagged COLLAPSED by §60's rule).
- tmin is 2.38 / 2.41 / 2.52 ms, so need is not readable.
- §56 and §57 (c1s and c1d sweeps) carry no `[SLAW]` / `[WSTAT]` lines (0 in every tgz), so their window minima do
  not exist.
- c1s's netem line has jitter 0, which makes it the discriminator. Constant predicts need ≈ 4 ms there. Any
  jitter-derived form predicts a margin of the order of its S_F (0.43 ms), plus whatever the rate queue adds.

### 1.6 What two cells cannot separate, and what would

- **They can separate:** constant from jitter-type, but only once the convention is fixed. The registered and native
  rows say constant; the equal-n rows say jitter-type. The rate, loop, σ and BDP families are out under every
  convention.
- **They cannot separate:**
  - 2J, S_F and p90 − p10 as the carrier of L (§1.4), since the floor-position band (±0.15 in k₃/k₂) is wider than
    their differences, and c3's n 2 100 floor is thin;
  - any affine pair c + k·X.
- **The data's main message is that need is not yet a well-posed quantity.** It depends on two estimator choices
  that are still open:
  - the sample-count rule (R6.7 open item 4);
  - where F̂ is read in load (R6.7 open item 1 and the boot-hold item that s61 §11 names for FLOOR-SENSITIVE).
- **One cell would separate them: the c3 line with the jitter changed and nothing else** (rate 20 Mbit/s, 20 ms, GE
  loss unchanged), at J = 0 and J = 3, beside 61c's J = 5.
  - Constant predicts the same need at all three J under its convention.
  - 2J-type predicts L ∝ J, with L(J0) of the order of the rate queue alone.
  - S_F-type predicts L ∝ S_F(J). S_F(J0) is not zero if c3's extra echo spread is engine-side (§1.2), so 2J and S_F
    come apart at J0.
  - Floor rows long enough for ≥ 8 complete G(2 100) groups make the equal-n floor readable (§4).
  - §60's c8 and c1s rows cannot do this. c8 leg 1 has no 0.95 crossing, and c1s has no window minima near its knee.

---

## 2. Q2: the proposed allowance

### 2.1 The form (proposed: decision 8's expression, re-provenanced as a two-cell cover)

```text
   Aᵢ(δ)  =  g(δ) · S_F,ᵢ  +  q(δ) · F̂ᵢ          g(Bulk) = 1,  g continuous and monotone in δ,  q(δ) = (b + 1)/30 ∈ [0.05, 0.10]
   fixed point (one leg):   T̂min(n*)  =  F̂ + g(δ)·S_F + q(δ)·F̂          T̂min and F̂ on the engine's native windows
```

The sentence it now carries: **at both measured leg shapes, the knee's window-minimum rise (as the engine's windows
read it) does not exceed the floor windows' median − minimum.** So the S_F term covers the knee at Bulk, and q(δ)·F̂ is
the δ-priced margin above it. This is a cover with measured slack. It is not the knee's law.

| symbol | provenance |
|---|---|
| S_F | **measured** (median over the floor windows of p50 − T̂min, r68 §1.4). Its role is a **measured cover**: need/S_F = 0.797 [0.780, 0.825] at c2, and 0.457 [0.409, 0.483] at c3 (registered) or 0.633 (native). It has no derivation as the knee's term (s61 DISAGREES), and it is UNDERIVED in the ledger's sense |
| F̂ | measured (R6.7 candidate C's floor, r68 §2.4) |
| q(δ) | declared dial, band cited (RFC 8289's 5–10 %) |
| g(δ) | declared dial: g(Bulk) = 1 (owner decision 8), the Realtime end an owner call (§3) |
| the knee (u = 0.95) | the registered knee definition (s61 §5) |

**Coverage under every convention** (`kmr.out` §5b). D = need − A per replicate, with g = 1 and S_F and F̂ read on the
same windows and floor position (decision 8: S_F is read on the windows F̂ is read on). The worst replicate is in
brackets.

| convention | c2 Bulk | c2 Realtime | c3 Bulk | c3 Realtime |
|---|---|---|---|---|
| registered | −1.90 (−1.84) | −1.47 (−1.40) | −9.17 (−9.11) | −7.22 (−7.16) |
| engine-native | −1.90 (−1.84) | −1.47 (−1.40) | −6.40 (−6.16) | −4.28 (−4.03) |
| equal n | −1.56 (−1.50) | −1.21 (−1.14) | −8.62 (−8.56) | −6.89 (−6.83) |
| registered, intercept | −1.46 (−1.39) | −1.05 (−0.98) | −8.60 (−8.54) | −6.71 (−6.65) |
| equal n, intercept | −1.13 (−1.07) | −0.80 (−0.74) | −7.61 (−7.55) | −6.02 (−5.96) |

- **The cover claim rests on all five rows.** It is COVERED at both cells, at both ends of the dial, in every
  convention, including the upper replicate (c2 4.11, c3 4.50 under registered counts).
- **That is what makes recommendation 1(a) compatible with 3 (equal n).** Under equal n, F̂ falls by N and S_F grows
  (6.52 at c2, 14.04 at c3), which keeps the cover.
- **The thinnest margin is c2 Realtime at equal n with the intercept floor: −0.74 ms** at the worst replicate.
- A cover built on S_F at the registered counts with minima at equal n (mixed) would not hold at c2:
  5.08 + 0.71 against 5.66 at u-matched, and SHORT at the intercept (5.08 + 0.65 against 6.18). That mix is not
  proposed.

**The gates (r68 §1.4).**
- **Mode, oracle and ratchet:** pass. It is one expression, δ enters only through g and q, and there is no predicate
  on k, t, n, δ or ρ.
- **Uniqueness:** pass. S_F and F̂ are floor reads, constant in n.
- **R-sign at the controls** (single-leg R67, r68's formula):
  - c2 0.103;
  - c3 0.255;
  - c1s 0.819 (direction only, since §60 flags the c1s controls).
- **The c1s size:** A = 0.67 ms on a 2.38 ms floor.
- **c8:** coupling-set (> 512 under every form, r68 §1.4), and not replayed here.

**The bind-fraction gauge.**
- A has no clamp. Its only clamp is the law's clamp(floor, N·knee) (F01's), whose gauge is `[SUMCAP]` `pin=` /
  `ask=`. Report it on the after-build battery, as F01 requires.
- **A cover needs a slack gauge, by the same rule** (MEASUREMENT DISCIPLINE 17: a law pinned or degenerate over its
  range is a defect finding).
  - The gauge is need / A at Bulk per battery cell: 0.68 at c2 and 0.33 at c3.
  - A cover slack by more than 2× on every cell would make S_F decorative. That is a ledger finding, not a footnote.
- If the engine's S_F estimator carries a sample-count eligibility hold (R6.7 item 4), its hold fraction is gauged
  too.

**What survives.**
- **q(δ)·F̂, unchanged.** It is not under test, and at c3 Bulk it alone supplies 3.90 of the 4.43 ms.
- **S_F, re-typed** from "the knee's term" to "a measured cover".
- **σ leaves the allowance.** Its family (A0, A0f, Ac, wq) is refuted at c2 (SHORT) and has the wrong shape across
  the two cells (§1.3).

### 2.2 Forms not proposed

| form | why not |
|---|---|
| **K: g·c_K + q·F̂, c_K = 4.50 ms** (the max replicate need) | **UNDERIVED constant.** Covers c2 −1.32 / c3 −3.96. It fits only the registered convention: engine-native excludes a constant from below, and equal n by ×1.6–1.9. At c1s it is A = 4.74 ms, 2× the floor, and R67 at the controls is 1.03 (max 1.11): **it fails the R-sign gate in direction**. It funds a 4 ms standing queue on a jitter-free 2.4 ms loop |
| **J: g·k·Ŝ + q·F̂ at equal n** (T̂min and F̂ both read at one count n\*, with k covering L/Ŝ ≤ 1.28) | **The physically better-shaped candidate** (§1.4), but nothing validates it yet. Its proxy Ŝ is not chosen (read at one count, S_F disagrees at equal n by −0.20 to −0.24, and p90 − p10 drifts 1.06–1.28). It needs a count-based window estimator, since a 250 ms time window cannot hold a fixed n across rates, and that is a design item (R6.7 item 4). It is the candidate the §4 cell tests |
| Jc: g·0.731·(p90 − p10)_F + q·F̂ (k fitted at c2, registered convention) | Covers both cells (c2 −0.87, c3 −5.44), but k is a c2 fit, and in the registered convention a spread form only adds the slack S already has |
| A0 / A0f / Ac / wq (σ family) | SHORT at c2 (§60), and the shape fails (need − σ changes sign) |

---

## 3. Q3: decision 8's g(δ) and decision 10

**q_B·F̂ already covers most of c3's knee.**
- At c3 Bulk, q_B·F̂ = 3.90 ms against need 4.43. With g = 0, Bulk lands at W\* 105.1, u 0.932 (the score's landing
  table).
- At c2, q_B·F̂ is 0.87 against 4.05.
- The S_F term's share of the cover is therefore loop-dependent. g multiplies a term that is most of the margin on a
  short loop and mostly slack on a long one.

**g(δ)'s Realtime end can no longer be measured as decision 8 planned.**
- Decision 8 set it "by the c3 sweep's need/S_F reading against c2's 0.80". That reading presumed a shape-free ρ, and
  s61 refuted it.
- The per-cell g_knee = ρ − q_R·F̂/S_F (where Realtime lands at that cell's knee) is 0.711 at c2 and 0.256 at c3,
  under the registered counts (the score). Engine-native gives c3 ≈ 0.27, and c2 is unchanged.
- **No single g_R lands Realtime at the knee at both cells:**
  - g_R ≥ 0.711 keeps Realtime at or above the knee everywhere measured. It costs c3 a landing at W\* ≈ 132,
    u 0.99, Tp50 ≈ 66 ms against the knee's 55;
  - g_R = 0.256 lands c3 at its knee and c2 below it, at u ≈ 0.65 (c2's landing table at g 0.25: W\* 102.7, u 0.647).
- So g_R becomes **purely the owner's price-table call** (decision 8's own deferred item: "whether Realtime may land
  below the knee is decided later, from a price table that includes Tp50"), with both cells' landing tables as the
  input.
- **g(Bulk) = 1 stands.** Under the proposed form it means "Bulk keeps the whole cover".

**Decision 10 (per-leg `queue_target_mult` = 1 + q(δ)) is unaffected.**
- It uses only q(δ), whose band is unchanged.
- It multiplies Copa's queue floor (`scheduler/copa.rs:314`, K01), not F̂ or the knee term.
- It would be affected only if the q band were re-declared (U2's branch (c)), which nothing here proposes.

---

## 4. Q4: a registration sketch for the separating measurement (s62, not registered)

**Question.** At fixed rate, delay and loss, does the knee's load rise L follow the path's jitter (2J), the floor
spread S_F, or neither (constant)? And is the equal-n floor readable with enough groups?

**Cells.**
- `c3-25` at netem jitter **0** and **3 ms** (tools-only cell names, e.g. `c3j0-25` / `c3j3-25`, with `lib.sh`
  `scenario_params` lines `20mbit 20 0 2 40` / `20mbit 20 3 2 40`);
- 61c's J = 5 rows as the third point. They are not re-run, but a two-row overlap arm at J = 5 bounds the drift, as
  §61b did.
- The same binary `e410d640` on the §60 gauge tree, Bulk only.

**Arms per cell** (61a's design, minus the B arms):
- floor pools 12 / 16 / 24;
- **floor rows sized for ≥ 8 complete G(2 100) groups.** Today's 15 MB W 12 rows hold 4, so W 12 needs ≈ 30 MB and
  W 16 ≈ 35 MB (bytes scale with LATE time at a fixed per-window sample rate). The u-matched floor stays the scored
  position: at n 250, c8 leg 1 corroborates the floor line's slope (§1.5);
- the K bracket placed by the rehearsal pilot (Amendment 2's proportional rule);
- P1 and the W 512 control;
- 7–8 arms × 6 replicates ≈ 45 rows per cell.

**Readings,** every one under the five conventions of §1.1, each printed. The labels are scored on one convention,
which **the owner fixes first** (§6, question 1):
- need and L per cell, with the bootstrap;
- k = L/X for X ∈ {2J, S_F, p90 − p10}, and k(J)/k(J5) with the interval.

**Predictions** (ms; from 61c's J = 5 and §1):

| sentence | equal-n L at J 0 / 3 / 5 |
|---|---|
| constant (in the registered convention: need 4.4 at every J) | 8.9 / 8.9 / 8.9 |
| 2J-scaled | ≈ rate queue only / ≈ 5.3 / 8.87 |
| S_F-scaled | 0.91–1.20 × S_F(J) (registered-count S_F) |

- Only L is predicted. Registered and native need are L − N, and N(J) is not predicted: N is not
  spread-proportional (×2.75 against ×1.91 for S_F, §1.2). Both are printed, decide nothing, and are read as data
  on N.
- A constant-L reading would be the surprise. Under the registered counts, constant still predicts need 4.4 at
  every J, which tests §1.1's lead directly.

- At J 0, 2J-scaled and S_F-scaled separate if S_F(J0) is ≥ 1 ms. The §1.2 extra echo spread predicts several ms.
- The predicted half-width of L is ≈ 0.3–0.5 ms (61c's per-replicate spread), so J 0 and J 3 each separate constant
  from either spread form by ≥ 3 ms.

**Cost.**
- VM: two rehearsals (pilot rows) and two launches of ≈ 45 rows, each ≈ 1 h rows + 20 min overhead, so ≈ 3 h.
- A tools-only harness change (cell names, the longer floor rows), done by a Sonnet worker with stub runs.
- Registration review rounds.
- No build: the same binary.

**Not covered.** c1s's knee (its controls collapse under the current rule, CR61 at large pools) and c8's coupling.
J = 0 on the c3 line is the cheaper discriminator for c1s's question, constant against jitter-derived.

---

## 5. Q5: the F07 ledger update (proposed diff; not committed to the ledger)

Against `docs/derivations/law-ledger.yaml` (the source `law-ledger/render.py` reads). The tags are unchanged and in
`render.py`'s `TAGS`. Re-render `law-ledger.md` after applying.

```diff
 - id: F07
-  name: store-law candidate allowance sigma + q(delta)*l-hat (R6.2 / R6.7 line; derivation only, not built)
+  name: store-law candidate allowance (registered sigma + q(delta)*l-hat; decided form g(delta)*S_F + q(delta)*F-hat, owner decision 8; derivation only, not built)
@@
-  formula: "T*_i = Phi_i + A_i + S_i ; A_i = sigma_i + q(delta)*l_i ; one leg: Tmin(n*) = F + sigma + q(delta)*l"
+  formula: "T*_i = Phi_i + A_i + S_i ; registered A_i = sigma_i + q(delta)*l_i ; decided A_i = g(delta)*S_F,i + q(delta)*F-hat_i, g(Bulk) = 1 ; one leg: Tmin(n*) = F + A"
@@
-  regime: "s60 c2 (status §60, store-law-r68.md §1): the knee needs Tmin 4.05 ms above the converged floor (E = 0.18 +- 0.03); the allowance supplies 1.79 + 0.56 = 2.34; D = +1.692 ms [+1.588, +1.836], SHORT on both bracket arms; R~67 crosses below W160 on 6/6 replicates at every delta. c8's landing is set by the half-gap coupling (> 512 under every form); c1s has no knee bracket"
+  regime: "s60 c2 (status §60, store-law-r68.md §1): the knee needs Tmin 4.05 ms above the floor; sigma + q*l-hat supplies 2.34, D = +1.692 ms [+1.588, +1.836], SHORT. s61 c3 (status §61a re-run Result): need_3 4.43 ms, rho_3 = need/S_F = 0.457 [0.409, 0.483] against rho_2 0.797, DISAGREES (S_F is not the knee's term); every candidate COVERED at c3 (Bulk D -2.69 to -10.57). knee-margin-rederive.md: need = L - N mixes the load rise at equal sample count L (c3/c2 x1.41-1.89 over two c3 sessions and two floor positions, jitter-sized) with the floor's sample-count drop N (x2.24-2.75); constant-in-ms holds only under s61's registered counts. c8's landing is coupling-set (> 512); c1s has no knee bracket"
@@
-  fix_class: "owner call (U2: a delta-free spread term, a delta-scaled knee term, or a re-declared q band), then a law change needing a battery; a measured-spread branch (S_F + q(delta)*F-hat, no candidate yet fully provenanced) needs one c3 static sweep first (store-law-r68.md §3)"
+  fix_class: "owner call (knee-margin-rederive.md §6: keep g(delta)*S_F + q(delta)*F-hat as a two-cell cover with a need/A slack gauge, or fix the estimator's sample-count and floor-position rule and register the jitter-only c3 cell, §4), then a law change needing a battery; DISHONEST-INPUT leaves with E13's fix (q*F-hat in place of q*l-hat)"
@@
-  evidence: "s60 C2-SHORT: D = +1.692 ms [+1.588, +1.836] on a CONVERGED floor (status §60 Result item 1); delta's whole authority is 0.28 ms (~5 slots) against a 31-slot shortfall; fixed forms reach the knee only through a constant fitted at c2 Bulk (c_B 2.26 ms, q' 0.26)"
+  evidence: "s61 DISAGREES (status §61a re-run Result): rho_3 0.457 [0.409, 0.483] vs rho_2 0.797, d_rho -0.340 [-0.396, -0.311] against Delta 0.085, not FLOOR-SENSITIVE; sigma-scaled separable (+0.320); the decided form covers both cells as a bound (Bulk D -1.90 at c2, -9.17 at c3) but its sentence is refuted, so S_F is a measured cover, not a derived term. s60 C2-SHORT for the sigma family: D = +1.692 ms [+1.588, +1.836]; need - sigma is +2.26 at c2 and -0.34 at c3, so A(c)'s sigma + c_B shape fails"
```

`verdicts` is unchanged and keeps both tags:
- UNDERIVED, because no term of either form has a derived knee sentence;
- DISHONEST-INPUT, because the registered σ + q·ℓ̂ still reads ℓ̂ (E13). It leaves when the decided form's q·F̂
  replaces q·ℓ̂.

The `derivation:` field stays. `measured`, `gauge`, `depends_on` and `ships_with` are unchanged.

---

## 6. Owner decisions

1. **Which form does the allowance take now?**
   - **(a) Keep A = g(δ)·S_F + q(δ)·F̂ as a two-cell cover** (§2.1). It covers both cells with no estimator change and
     its c1s size is sane. The cost is over-allocation on long loops (c3 Bulk lands 9.2 ms above its knee, W\* 150
     against 109). F07 stays UNDERIVED, and a need/A slack gauge goes on the battery.
   - **(b) Fix the estimator's convention first** (one sample count for both minima, and where F̂ is read in load),
     then run §4's jitter-only cell to choose the knee's carrier (2J-proxy, S_F, or p90 − p10) at equal n (form J,
     §2.2). That is ≈ 3 h of VM plus a design item (R6.7 items 1 and 4).
   - (c) A declared constant c_K ≈ 4.5 ms. It has no sentence, it fails R-sign at c1s in direction, and it holds
     only under one counting convention.
   - **Recommendation: (a) now as the engine form's allowance, and (b) as the next store-law measurement.** (a) is
     the cheapest correct cover and is reversible. (b) is the only route to a derived knee term, and its convention
     question must be answered before any further knee sweep, since the convention flips the ranking.
2. **g's Realtime end: choose it from the price tables, not from a ratio.** No single g_R lands Realtime at the knee
   at both cells (g_knee 0.711 at c2 and 0.256 at c3, under the registered counts).
   - Options: g_R ≥ 0.71 (Realtime at or above every measured knee; c3 Realtime Tp50 ≈ 66 against 55 ms at its knee),
     or g_R lower with Realtime below the knee on short loops (c2 g_R 0.25: u 0.65, Tp50 ≈ 15.5 ms).
   - **Recommendation: g_R = 0.71 until the §4 cell reads.** It is the one value that keeps Realtime off the
     goodput cliff at both measured cells. Lowering it is your U3-style trade, and the price tables carry it.
3. **The counting convention for every future knee reading** (needed by 1(b) and by §4): registered (s61's), engine-native,
   or equal n.
   - **Recommendation: equal n, with floor rows long enough for ≥ 8 groups at the knee's count.** It is the only
     convention under which need is a path property (L). Today's c3 floor holds 4 groups per W 12 row at that count,
     which is why its intercept is the weakest reading here.
