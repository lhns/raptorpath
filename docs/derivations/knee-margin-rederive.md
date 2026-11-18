# Knee margin after s61's DISAGREES: what need scales with, and which statistic decides it

Status: DERIVATION, round 3. Docs and Python only: no engine code, no cargo, no VM. Branch `knee-margin-rederive` off
`github/main` `4559bbee`. Round 3 answers the coordinator's round-3 points after Fable's review, on top of round 2 and status "§61a re-run Result,
Erratum 1" (main `face6093`).

**Every reading here is post hoc,** and so is every statistic chosen on these rows. None was registered.
Confirmation is out of sample, in s62 (§5), and §5's predictions are built around the statistic §2 proposes.

**In plain words.**
- **DISAGREES stands** (Erratum 1). "Constant in ms" and "S_F is not the term" stay withdrawn.
- **The counting question is settled** (round 2, §2.1–2.2). The floor's drop follows the sample count, through the
  jitter, and not the time span.
- **But the label depends on the statistic, so no label chosen on these rows is evidence.**
  - Δρ reads −0.34 with the registered minimum, about −0.2 with the minimum at one count, −0.07 with p10, and +0.20
    with p50.
  - With location and spread taken apart (§2.3), the same two cells give AGREES, UNRESOLVED and DISAGREES (both
    signs).
  - Fable's reading holds. The median's rise over the registered S_F is 0.711 at c2 and 0.713 / 0.750 on the two c3
    sessions. **Two cells can yield every label.**
- **The engine fixes the statistic (§2.4).** The store law holds the window minimum of its own native windows against
  floor + allowance (R6.2: R = 1 iff T̂min = F̂ + A), and decision 8 reads S_F on the windows F̂ is read on.
  - On that pair, the native minimum over the native S_F, ρ DISAGREES on both c3 sessions: d [−0.22, −0.13] and
    [−0.17, −0.087], against the re-derived margin Δ = 0.085. On 61a it clears Δ by 0.002.
  - The minimum's count dependence is then the law's own, not a measurement convention.
- **The quantile statistic of round 2 (window median + IQR) is a candidate, untested.**
  - The IQR is not logged.
  - Its stand-in p90 − p10 fails the estimator criterion at c3, so the stand-in's DISAGREES does not test it.
  - The criterion (≥ 5 independent draws beyond the quantile) and the ≈ 17–21 independent draws per c3 floor window
    are now derived and measured (§2.5).
- **Decision 8's form is unchanged and is not a derivation here.** F07 stays UNDERIVED.
- **s62 (§5) scores one number:** the knee-native need at J = 0, if the floor's count drop there is < 0.5 ms, with a
  derived margin Δ₀ = ½·q_R·F̂₀. J = 5 is run in session, and the J 0 / 3 / 5 line is reported only.
- **The owner decides whether to run s62** (§7): ≈ 3.4 h of VM typical, host builds paused, no F2 fix needed.

**Advisor (Fable) log.**
1. **Round 1.** Reproduce first; tabulate conventions with intervals; give each candidate its two-point exponent;
   check need − σ; validate the new utilisation instrument; do not upgrade a fitted constant; write the F07 diff
   against `law-ledger.yaml`.
2. **Round 1 review.** c8 leg 1 is a leg offset and must not be substituted. Show form S's coverage under every
   convention. Replicate on `rep61a`. State the groups per row.
3. **Round 2.**
   - The first count-ladder test (§8 of `kmr.out`) compared unequal counts at its low end, and is superseded by a
     window ladder (§8b).
   - The doubling-drop regression (§7) has no power, because the drop per doubling is nearly constant. It is
     replaced by a fixed-count test: stride, shuffle and lag-1 (§7b).
   - The p10 nk-split failed its own criterion, so all quantiles are shown in one table and the statistic is chosen
     on estimator grounds.
   - The claims that fail on the session bands are named.
   - The text is aligned with Erratum 1, and c1s is pre-read for s62's J = 0 prediction.
   - All applied.
4. **The coordinator's three constraints from Fable** (one statistic for engine and measurement, computable on native
   windows with count-free bias; applied to all three inputs; shown flat on the count table, with ρ consistent across
   c2 and both c3 sessions). Round 2's §2.3 applied them.
5. **Round 3 (coordinator, after Fable).**
   - The label depends on the statistic (−0.34 / −0.2 / −0.07 / +0.20), so no statistic chosen on these rows is
     evidence, and the flattening test is passed by any quantile by construction.
   - Location and spread are shown apart, in ms (§2.3).
   - The statistic is fixed from the engine (§2.4).
   - The criterion and the draw count are derived (§2.5), and the quantile candidate is marked untested (§2.6).
   - s62 registers rise against spread, not a ρ (§5), and there is no owner question.

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place) | output |
|---|---|---|
| `store-law/kmr.py` | `l1-raw/s60/rep60a-diag-ld.tgz`, `rep60b-diag-ld.tgz`, `l1-raw/s61/rep61c-diag-ld.tgz`; `rep61a-diag-ld.tgz` and `rep61b-diag-ld.tgz`, each loaded on its own because their row keys collide with 61c's and §60's; imports `r68allow.py` and `s61pred.py` | `kmr.out` |

`kmr.out` sections:
- §0 the witness;
- §1–§5b round 1;
- §6 both-session bands;
- §7 / §7b what the floor's drop follows;
- §8b L on a window ladder (§8 superseded);
- §9 / §9b the statistics side by side;
- §10 the quantile statistic: the count table, the independent draws per window, and ρ_q;
- §11 location and spread in ms, apart, and the label of every pair;
- §12 the engine's own pair and its margin;
- §13 c1s ranks at jitter 0.

**`rep61b` is now used.** It is loaded on its own (the collision fix) and serves only §7's doubling-drop regression, as
c2's lower arms W 96 / 128 / 144. No floor or knee reading needs it, since c2's floor is W 16 and its knee arms are
§60's 160–256.

---

## 0. Witness (`kmr.out` §0)

The script reproduces the scored c3 reading exactly:
- κ 0.00962, W\* 11.6, F_c 38.998 and S_F,c 9.709;
- the six needs, and ρ₃ 0.457;
- the NATIVE line (F 42.523, S_F 5.861, ρ 0.633).

c2's frozen inputs reproduce too: F 8.679, S_F 5.083, need 4.049, ρ₂ 0.797.

---

## 1. What need scales with, on bands that span both c3 sessions (`kmr.out` §6)

**The two c3 sessions.**
- 61c is the scored re-run.
- 61a is s61's first session, read post hoc, with K 104 / 112 / 124 and P1 136 as the path exercise counted them, and
  the W 2 048 control read by goodput.
- Each session gets a 90 % bootstrap (1 000 draws, c2 and that session resampled). **BAND** is the union of the two
  sessions' intervals: a session-to-session spread, not one session's bootstrap.
- **c2 has one session.** Its drift is bounded only at the W 160 overlap arm (§61b: |Δtmin| 0.035 ms, |Δu| 0).

### 1.1 need₃ / need₂ under each convention

| convention | 61c | 61a | **BAND** |
|---|---|---|---|
| registered (c2 native; c3 G(250) floor, G(2 100) knee; u-matched) | 1.095 [0.968, 1.175] | 1.081 [0.958, 1.222] | **[0.96, 1.22]** |
| engine-native | 0.916 [0.846, 0.969] | 1.012 [0.932, 1.104] | **[0.85, 1.10]** |
| registered, floor at the intercept | 1.226 [1.006, 1.384] | 1.179 [0.999, 1.378] | **[1.00, 1.38]** |
| one count (≈ 2 100) for both sides, u-matched (= L) | 1.567 [1.487, 1.643] | 1.411 [1.295, 1.558] | **[1.30, 1.64]** |
| one count, floor at the intercept | 1.891 [1.749, 2.033] | 1.545 [1.342, 1.767] | **[1.34, 2.03]** |

**Withdrawn from round 1, because they fail on the bands:**
- "constant is excluded under engine-native": the band [0.85, 1.10] contains 1;
- "the intercept excludes 1": its band starts at 1.00;
- "L ×1.57 [1.48, 1.64]": the second session reads 1.41, so the band is [1.30, 1.64].

### 1.2 need = L − N

```text
   need  =  [ T̂min(W₉₅; n_K) − F(n_K) ]  −  [ F(n_F) − F(n_K) ]   ≡   L − N
```

| | c2 | c3 61c | c3 61a | c3/c2 BAND |
|---|---|---|---|---|
| L (load rise at one count ≈ 2 100) | 5.66 | 8.87 | 7.99 | **[1.30, 1.64]** |
| N (the floor's drop from ≈ 280 to ≈ 2 100 samples) | 1.61 | 4.44 | 3.61 | **[1.85, 3.18]** |

**For the coordinator: N's band contains S_F's ratio** (1.88–1.91; it excludes netem J's 1.67). So round 1's "N refutes
the registration's §3 assumption (a minimum bias proportional to the spread)" does not survive on the band.
- Erratum 1's sentence "matching each side's count across cells does not make ρ shape-invariant, which the
  registration's §3 assumed" is still true of the label: the registered ρ disagrees.
- But these data do not show that the §3 assumption itself failed.

**The count-matched floor drop, both ways** (`kmr.out` §7b):

| pool | 61c: median of row differences | 61c: difference of medians | 61a: median of row differences | 61a: difference of medians |
|---|---|---|---|---|
| W 12 | 4.48 | 4.46 | 2.78 | 3.61 |
| W 16 | 3.15 | 3.73 | 3.78 | 3.92 |
| W 24 | 2.97 | 2.88 | 2.78 | 2.98 |

- c2: 1.42 / 1.44 at W 16.
- The coordinator's figures are the differences of medians.
- c3's W 12 floor holds 4 complete G(2 100) groups per row, so these are thin.

### 1.3 Candidates against the bands (`kmr.out` §6)

X₃/X₂ is given per session (61c / 61a). "Overlaps" means the pair overlaps that convention's BAND.

| X | X₃/X₂ | registered | native | one count | reg. @ intercept | one count @ intercept |
|---|---|---|---|---|---|---|
| constant | 1 / 1 | **yes** | **yes** | no | **yes** | no |
| S_F (registered counts) | 1.91 / 1.88 | no | no | no | no | **yes** |
| S_F at n 2 100 | 2.15 / 2.05 | no | no | no | no | no |
| floor p90 − p10 (native) | 1.48 / 1.50 | no | no | **yes** | no | **yes** |
| netem J | 1.67 | no | no | no (band ends 1.64) | no | **yes** |
| σ (window `sig` over the K arms) | 2.70 / 2.71 | no | no | no | no | no |
| loop floor F | 4.49 / 4.46 | no | no | no | no | no |

- **What survives:**
  - under the registered and native counts, a constant fits and no spread does;
  - at one count, constant is out and jitter-type spreads fit, but which one depends on where the floor is read;
  - σ, the loop, and the per-datagram, per-skb and per-ack times (×4.6–5.1 from round 1) are out under every
    convention.
- **need − σ** is +2.26 ms at c2 and −0.34 at c3, so W1's A(c) form (σ + c_B) fails in shape, and its c_B ≈ 2.7 ms
  still has no source.
- **The lesson is that what need "scales with" depends on how the minimum is counted.** That puts the estimator
  first (§2).

### 1.4 Other ledgers (round 1, unchanged)

- **The utilisation instrument** (netem Sent bytes, per leg) puts the goodput knee at u_link 0.947 at c2 and 0.941 at
  c3.
- **c8 leg 0** has no arm between u 0.56 and 0.99, so it gives no knee point.
- **c8 leg 1** never crosses 0.95, which leaves only a bound: L₁(u 0.92) ≥ 8.4 ms against its own floor.
  - It reads 2.2–3.9 ms below c3 at matched load, a leg offset, so it is not substituted for c3.
  - Its n 250 floor slope (9.4 ± 0.6 ms per unit u) corroborates c3's G(250) floor line (10.6 ± 2.9).
- **c1s** has no knee, and s56/s57 carry no window minima.

---

## 2. The count question (Erratum 1: "a count rule, or a count-free statistic, before any knee reading is interpreted")

### 2.1 The count sets the minimum, not the time span (`kmr.out` §7b)

**The test.** Within each floor arm, at a fixed count (k = 8 windows per group), compare four ways of forming the
groups:
- contiguous windows (span 8);
- windows at stride 2 and stride 4, interleaved (span 16 and 32 at the same count and group number);
- a seeded shuffle (span = the whole row).

| arm | contiguous | stride 2 | stride 4 | shuffled | lag-1 r of window minima | acks per sample |
|---|---|---|---|---|---|---|
| c2 W 16 / 32 / 48 | 7.12 / 7.35 / 7.63 | 7.12 / 7.29 / 7.65 | 7.08 / 7.31 / 7.68 | 7.12 / 7.36 / 7.66 | −0.01 to 0.00 | 0.23–0.27 |
| c3 61c W 12 / 16 / 24 | 38.00 / 38.42 / 38.85 | 37.50 / 38.33 / 38.73 | 37.81 / 38.32 / 38.85 | 37.39 / 38.25 / 38.50 | +0.04 to +0.07 | 0.26–0.35 |
| c3 61a W 12 / 16 / 24 | 37.62 / 37.58 / 38.44 | 37.65 / 37.89 / 38.43 | 37.74 / 37.66 / 38.39 | 38.06 / 37.71 / 38.38 | +0.02 to +0.05 | 0.26–0.35 |

- **The groups are equal to within noise:** ≤ 0.06 ms at c2, and no consistent sign at c3 (±0.6 ms). The window minima
  are uncorrelated at lag 1.
- So span and time drift do not set the floor's drop. The count does.
- **Acks against samples cannot be separated** on these arms, since acks per sample is 0.23–0.35 everywhere. §7's
  doubling-drop regression could not separate samples, span and acks either: all three fit equally (residual sd
  0.074–0.076 ms at c2, 0.38–0.41 at c3), because the drop per doubling is nearly constant.
- **The jitter-free line** (`kmr.out` §7b, c1s): the floor minimum falls only 0.12 / 0.12 / 0.18 ms over k = 1 → 32
  windows at W 64 / 128 / 192. Over a similar pooling c2 W 16 falls 2.2 ms (k 1 → 32) and c3 W 12 falls 7.8 ms.
  - The drop scales with the path's jitter. That is consistent with the minimum being an extreme of the jittered
    echo distribution: more draws, lower minimum.
  - It does not separate netem's jitter from the echo-timing spread c3 carries beyond it (round 1 §1.2).

### 2.2 Does L move with the count? (`kmr.out` §8b, a window ladder at equal realised counts)

The knee arms are pooled over k windows, and the floor is grouped to the knee's realised count.

| ladder step | 61c n / L | 61a n / L | c2 n / L |
|---|---|---|---|
| k 1 (native knee) | 462 / 8.55 | 468 / 8.75 | 2 342 / 5.75 |
| k 2 | 914 / 8.44 | 930 / 8.39 | 4 688 / 5.60 |
| k 4 | 1 819 / 8.52 | 1 856 / 8.53 | 9 359 / 5.45 |
| k 8 | 3 637 / 8.83 | 3 692 / 7.93 | – |
| k 16 | 7 255 / 8.90 | 7 372 / 7.33 | – |

- **Each minimum falls 0.6–5.0 ms over the ladder** (c2 floor 0.57, c3 61c floor 4.96).
- **L moves at most 0.46 ms on 61c over 16×, and 0.30 ms at c2 over 4×.** On 61a it moves 0.36 ms up to k 4, then
  1.4 ms once the floor holds only 1–3 groups per row.
- So differencing at one count cancels most, but not all, of the count dependence. The residual (0.3–0.5 ms, more on
  thin floors) is the size of s61's Δ·S_F. The c3 knee cannot be read below ≈ 460 samples, nor c2's below ≈ 2 250.
- **"Equal count makes need a path property" holds only approximately.** A count rule would also need a declared n\*
  and floor dwell long enough for ≥ 6 groups at n\*. §2.4 shows the engine settles which statistic counts, and §2.6 keeps a count-free candidate for review.

### 2.3 Location and spread apart: two cells can yield every label (`kmr.out` §11)

**The knee's rise above the floor, in ms, for each location statistic** (u-matched floor; median over replicates):

| location | c2 | c3 61c | c3 61a |
|---|---|---|---|
| minimum, engine-native windows | 4.05 | 3.71 | 4.10 |
| minimum, registered counts | 4.05 | 4.43 | 4.38 |
| minimum, one count (L) | 5.66 | 8.87 | 7.99 |
| window p10 | 4.63 | 6.40 | 6.57 |
| window p50 | 3.62 | 7.21 | 7.43 |

**The floor spread, in ms:**

| spread | c2 | c3 61c | c3 61a |
|---|---|---|---|
| S_F at the registered counts (p50 − min) | 5.08 | 9.71 | 9.56 |
| S_F on native windows (p50 − min) | 5.08 | 5.86 | 6.09 |
| p90 − p10 (native windows) | 5.54 | 8.44 | 8.47 |

**Fable's reading from the price tables** (arm medians: the floor pool's tp50, and the knee's tp50 interpolated at the
median W₉₅):

| | floor tp50 | knee tp50 | rise | rise / S_F (registered) |
|---|---|---|---|---|
| c2 | 13.70 | 17.31 | 3.61 ms | **0.711** |
| c3 61c | 48.73 | 55.65 | 6.92 ms | **0.713** |
| c3 61a | 48.47 | 55.64 | 7.17 ms | **0.750** |

**It holds.**

**Every location / spread pair**, as d = ρ₃ − ρ₂ with its 90 % interval (`kmr.out` §11, 400 draws), against
Δ = 0.085:
- **AGREES:**
  - p50 over registered S_F on 61c ([−0.045, +0.077]);
  - L over p90 − p10 on 61c ([−0.029, +0.078]).
- **UNRESOLVED:**
  - the registered minimum over native S_F, both sessions;
  - p10 over p90 − p10, both;
  - the 61a halves of the two AGREES pairs.
- **DISAGREES, negative:**
  - the registered minimum over registered S_F (s61's label);
  - the native minimum over every spread;
  - L and p10 over registered S_F.
- **DISAGREES, positive:**
  - L, p10 and p50 over native S_F;
  - p50 over p90 − p10.

**So two cells can yield every label from the same rows.** A statistic chosen because of its label is a fit. The
statistic has to be fixed from outside the data, which §2.4 does from the engine.

### 2.4 The engine fixes the location: the law holds the native window minimum

**What the store law keeps below its target** (`store-law.md` R6.2, lines 1380 and 1926; `store-law-r67.md` §5, the
R6.7 line; `store-law-r68.md` §1.1):

```text
   R_k  =  Σᵢ wᵢ,ₖ · ( Fᵢ + Aᵢ + Ŝᵢ,ₖ ) / Σᵢ wᵢ,ₖ · T̂ᵢ,ₖ          Ŝᵢ,ₖ = T̂ᵢ,ₖ − T̂minᵢ,ₖ        (T̂ the window median, T̂min the minimum, w the count)
   R = 1   ⇔   Σ w·T̂min  =  Σ w·(F + A)            one leg:   T̂min(n*)  =  F̂ + A
   F̂ᵢ  =  median of T̂min,k over the K windows of lowest store size          (R6.7 candidate C: also a window minimum)
```

- **The location is fixed by the law.** The median cancels from the fixed point, and what the law holds against
  floor + allowance is the **window minimum, on the engine's own (native) windows**, at both the operating point and
  the floor.
- **Decision 8 fixes the spread:** "S_F is the floor windows' median − minimum, read on the windows F̂ is read on".
  That is native p50 − min.
- **What the engine can compute:**
  - per window, the count, minimum, median and mean (`store-law.md` R6.2 table: "w, T̂, T̂min, Ŝ: the per-leg window
    sampler of `record_rtt_sample`");
  - σ (`qsp_us`, P90 − P50 over the last 256 echoes);
  - ℓ̂, a lifetime minimum on the gauge branch, DISHONEST-INPUT (E13);
  - Copa's 10 s running minimum m̂.
  - The per-window p10 / p90 exist only in the gauge (`[WSTAT]`, `net/slaw.rs` `WinSampler::close`, nearest-rank on a
    bounded buffer). They are computable in principle, but they are not law inputs.
- **The engine's own pair, then:** the native minimum's rise over native S_F (`kmr.out` §11 row 2, §12).

  | | ρ | d = ρ₃ − ρ₂, 90 % | label |
  |---|---|---|---|
  | c2 | 0.797 | – | – |
  | c3 61c | 0.633 | [−0.222, −0.131] | DISAGREES |
  | c3 61a | 0.674 | [−0.169, −0.087] | DISAGREES (clears Δ by 0.002) |

- **Its margin, re-derived for this pair** (s61 §5.2's derivation: a common g_R keeps Realtime above the knee iff
  |Δρ|·S_F < q_R·F̂ at every cell):

  ```text
     Δ  =  min_c q_R·F_c / S_F,c  =  min(0.085, 0.363, 0.345)  =  0.085
  ```

  c2 binds, because native S_F at c3 is small (5.9–6.1 ms, against 9.7 at the registered counts).
- **The need in ms that the law's allowance must cover** is the native one: 4.05 at c2, 3.71 and 4.10 at c3.
- **The minimum's count dependence belongs to the law, not to the measurement.**
  - The engine's floor windows hold ≈ 60 samples at c3 and 282 at c2. Its knee windows hold ≈ 460 and ≈ 2 340.
  - The fixed point therefore includes the floor's sample-count drop by construction.
  - Any engine rule on window counts (R6.7 open item 4) or any change of statistic changes need and ρ themselves.
    That is a law change, reviewed as one.

### 2.5 The estimator limits, derived and measured

**The "≥ 5 draws beyond the quantile" criterion.**
- For m independent continuous draws, the k-th order statistic sits at probability level F(X₍ₖ₎) ~ Beta(k, m − k + 1),
  with mean k/(m + 1) (David & Nagaraja 2003, §2.1–2.2).
- The sample p-quantile takes k = ⌈p·m⌉. Its level differs from p by at most 1/(m + 1), so its relative level error
  is ≈ 1/(m·p), and its relative standard deviation is √((1 − p)/(m·p)).
- **Its level is count-free to a relative tolerance ε iff m·p ≥ 1/ε.** With ε = 0.2 (declared), that is m·p ≥ 5,
  where the relative standard deviation is ≤ 0.45. In ms the bias is the level bias divided by the density at the
  quantile.
- **The minimum (k = 1)** sits at level 1/(m + 1). That level halves with every doubling of m, so the minimum is never
  count-free. This is §2.1's non-flattening.

**The independent draws per window** (`kmr.out` §10b; measured from the netem counters).
- A c3 W 12 floor window holds ≈ 60 echo samples, ≈ 21 ack packets (srv0 84 pkt/s × 0.25 s) and ≈ 17 data skbs.
  c2 W 16: 282 samples, 75 acks, 59 skbs.
- **The declared model:** netem draws one delay per skb on the data direction (it schedules GSO skbs as units, status
  §3.8) and one per ack packet on the return. Echo samples that share an skb or an ack share a draw.
- So the independent draws are ≈ 17–21 per c3 floor window, not 60.
- **With m = 17–21:** p10 gives m·p 1.7–2.1 (fails), p25 4.3–5.3 (at the edge), and p50 8.5–10.5 (passes).

These two lines now carry the candidate of §2.6. Each states its declared part: ε = 0.2, and the independence model.

### 2.6 The candidate statistic (window median + IQR), untested

- **The candidate.** Location: the median of native windows. Spread: the IQR on the floor windows. These are the most
  extreme rank fractions that are central at c3's floor windows (§2.5).
- **Untested.**
  - The IQR is not logged (`[WSTAT]` prints p10 / p50 / p90).
  - Its stand-in, p90 − p10, fails §2.5's criterion at c3 (m·p ≈ 2). So the stand-in's DISAGREES (d +0.20 / +0.22,
    `kmr.out` §10c) does not test the candidate.
  - The count table's flatness (`kmr.out` §10a) is by construction, since a group's quantile is carried as the
    weighted median of its windows' own quantiles.
- **It differs from the law.** The law holds the minimum (§2.4). Adopting a quantile changes the law's sentence ("the
  operating point's median sits A above the floor's median") and is a law change. It is offered for design review,
  not as the statistic that decides any label here.
- **`RWM_RTT_DUMP` carries the `[SLAW]` sample stream** (code read).
  - On the gauge tree (`github/slaw-gauge` `d416d59b`), `raptorpath/src/scheduler/path.rs`, `record_rtt_sample(&mut
    self, rtt)`, "the single delegate every ack path funnels through", calls
    `crate::net::rttdump::gauge().note_rtt(self.id, rtt_us)` and then `self.slaw.observe(rtt_us)` with the same µs
    value. So the dump and the `[SLAW]` window sampler see the same stream, in the same order.
  - The sampler's quantiles are `monitor::quantile::nearest_rank` on a stride-subsampled bounded buffer, whose bind is
    printed as `nkq=` (`net/slaw.rs` `close`).
  - The dump keeps a prefix of at most `RWM_RTT_DUMP_MAX` = 400 000 samples per path (`net/rttdump.rs`).
  - On main the dump hook sits in the same function, without the sampler.
- **What the dump enables in s62:** exact window quantiles (p25 / p75 included) and a real count test, subsampling
  windows to m draws. No rebuild is needed, and the dump pass reports its own perturbation (rttdump.rs: "a dump-on pass
  is run separately").

---

## 3. Decision 8's form, as decided (not a derivation)

```text
   Aᵢ(δ)  =  g(δ)·S_F,ᵢ + q(δ)·F̂ᵢ          g(Bulk) = 1        (owner decision 8, unchanged; not in the engine)
```

This document does not derive it, and does not re-type its provenance. F07 stays UNDERIVED.

**For reference, its coverage on these rows** (`kmr.out` §5b). S_F and F̂ are read on the same windows.
- **"Realtime" means g held at 1 with q at its Realtime value (0.05), so g_R = 1.** No g_R < 1 was assumed.
- D = need − A, median (worst replicate):

  | convention | c2 Bulk | c2 Realtime (g_R = 1) | c3 Bulk | c3 Realtime (g_R = 1) |
  |---|---|---|---|---|
  | registered | −1.90 (−1.84) | −1.47 (−1.40) | −9.17 (−9.11) | −7.22 (−7.16) |
  | engine-native | −1.90 (−1.84) | −1.47 (−1.40) | −6.40 (−6.16) | −4.28 (−4.03) |
  | one count | −1.56 (−1.50) | −1.21 (−1.14) | −8.62 (−8.56) | −6.89 (−6.83) |
  | one count, intercept | −1.13 (−1.07) | −0.80 (−0.74) | −7.61 (−7.55) | −6.02 (−5.96) |

- **Its delay price at c3 Bulk** (the score's landing table): g = 1 lands at W\* 150.3 with Tp50 75.1 ms, against Tp50
  55.4 ms at W 108 (the knee). That is +20 ms of median loop for the cover.
  - At c2, g = 1 lands at W\* 198.5, against the knee's 168.
- **Rejected alternative:** a constant c_K ≈ 4.5 ms. It has no sentence, it fits only under minimum-based counting,
  and at c1s it gives A = 4.74 ms on a 2.38 ms floor, with R67 at the controls 1.03 (direction only).

## 4. Decisions 8 (g) and 10

- **g's Realtime end cannot be measured from need/S_F,** since that ratio is not invariant under the registered
  counting (DISAGREES). On the law's own pair (the native minimum over native S_F) it disagrees too (§2.4), and other
  pairs give every label (§2.3).
  - g_knee per cell (registered counts, the score) is 0.711 at c2 and 0.256 at c3.
  - No g_R value is recommended: nothing consumes one until an engine form exists.
- **Decision 10** (per-leg `queue_target_mult` = 1 + q(δ)) uses only q(δ), and multiplies Copa's queue floor
  (`scheduler/copa.rs:314`), so it is unaffected.

---

## 5. s62 sketch (not registered): one scored number at J = 0, and the jitter line reported

**Question.** With rate, delay and loss fixed and the netem jitter removed, does the knee still need the window minimum
to rise about 4 ms above the floor (a constant), or about nothing (a spread-carried margin)?

### 5.1 Cells and arms (binary `e410d640`, Bulk only)

- **J = 0 and J = 3:** tools-only cell names, `scenario_params` `20mbit 20 0 2 40` / `20mbit 20 3 2 40`.
  - Each cell gets its own rehearsal (the proportional pilot, Amendment 2).
  - Arms: the floor arm (§5.3), the K bracket placed by the pilot, P1, and the W 512 control, × 6 replicates =
    36 rows per cell.
- **J = 5 in session** (c3's own netem line). 61c cannot stand in: the two c3 sessions differ by ≈ 0.4 ms in native
  need (3.71 against 4.10), which is a tenth of the gap the scored test reads.
  - It is a short overlap, with no rehearsal: the floor arm, 61c's K bracket 100 / 108 / 120, and the W 512 control,
    × 6 = 30 rows.
  - **Drift rule (as 61b against §60):** DRIFT-OK iff |tmin₁₀₈(s62) − 45.96| ≤ 0.3 ms and |u₁₀₈(s62) − 0.947| ≤ 0.02,
    as arm medians against 61c's native arm medians. 0.3 ms = 0.05 × native S_F 5.86, the τ rule of s61 §5.1.
  - If DRIFT, the in-session J = 5 reading is used, and 61c / 61a are reported beside it, not merged.
- **One `RWM_RTT_DUMP=1` pass per cell** (floor arm and K₂, 2 rows), reported separately for its perturbation. It
  carries the same stream as `[SLAW]` (§2.6) and serves the reported quantile readings only.

### 5.2 The scored reading: need at J = 0 on the knee-native window minimum

```text
   need₀  =  median_r T̂min_r(W₉₅,r)  −  F₀           T̂min on the knee arms' native windows (the law's statistic, §2.4);  F₀ the floor arm at its fixed count (§5.3)
```

- **The scoreability gate, checked in session first:** the floor's count drop at J = 0, D₀ = F(G250) − F(G2100) on
  the floor arm (difference of arm medians).
  - **Scoreable iff D₀ < 0.5 ms.** Otherwise need₀ is **UNSCOREABLE**: the floor still depends on its count, so need₀
    would depend on §5.3's choice of count.
  - The prediction for D₀: ≈ 0.1 ms if the count drop is jitter sampling (c1s pre-read, ≈ 0.035 ms per doubling ×
    three doublings), against 2.8–4.5 ms at J = 5 today.
- **Predictions:**

  | sentence | need₀ |
  |---|---|
  | spread-carried (the knee margin is the jitter the FIFO absorbs) | ≈ 0 (only the rate queue at u 0.95; c1s's jitter-free minimum rises 0.14 ms by u 0.5) |
  | constant (the knee margin is not a spread) | need₅ in session (3.7–4.1 ms on 61c / 61a) |

  The two sit ≈ 4 ms apart.

**The margin for this one-number test, derived.**
- The consequence is s61 §5.2's. An error ε in the knee term is a landing error of ε, and Realtime stays above its
  knee iff ε < q_R·F̂₀.
- The two acceptance bands must not overlap. So each takes half the consequence tolerance:

  ```text
     Δ₀  =  ½ · q_R · F̂₀
  ```

- F̂₀ is predicted at ≈ 40.4–43 ms (the 40 ms propagation plus the echo's own excess), so Δ₀ ≈ 1.0–1.1 ms. It is
  recomputed in session from the measured F₀.
- The bands are disjoint iff 2·Δ₀ < need₅ (≈ 2.1 < 3.7). That holds, with 1.6 ms to spare.
- **Labels** (90 % bootstrap over replicates and floor rows, 2 000 draws):
  - **SPREAD-CARRIED** iff CI(need₀) ⊂ (−Δ₀, +Δ₀);
  - **CONSTANT** iff CI(need₀) ⊂ (need₅ − Δ₀, need₅ + Δ₀), with need₅ the in-session J = 5 point value;
  - **INTERMEDIATE** iff the CI lies inside neither band and between them (a partly spread-carried, affine margin);
  - **UNRESOLVED** otherwise (the CI straddles a band edge);
  - **UNSCOREABLE** if D₀ ≥ 0.5 ms (above), or if fewer than 5 replicates have W₉₅ inside the K bracket (s61 §2.2's
    rule).
- **Resolution.** The native need's per-replicate spread at J = 5 is 3.15–3.95 ms (61c), so a 90 % half-width of
  ≈ 0.3 ms is expected. That resolves either band.

### 5.3 The floor's role, and its fixed count

- **The law's F is not yet defined.** It is built from m̂ / ℓ̂ / b̂₀⁺ in R6.2's anchor, or from R6.7 candidate C's
  median of the lowest-n windows' minima. Its count is R6.7's open n₀ and open item 4.
- So the floor arm's count is a measurement choice, not the law's.
- **s62 fixes it:** the floor arm is W 12 at **25 MB** (150 s timeout), with F₀ read under G(250). That is s61's floor
  count, and it is held at every J.
- **Why the count does not decide need₀:** the gate D₀ < 0.5 ms bounds how far any count in [250, 2 100] can move F₀,
  to < 0.5 ms, which is half of Δ₀.
- **Groups and timeout.** At ≈ 240 samples/s, 25 MB gives ≈ 85–95 s of transfer. That is ≥ 8 complete G(2 100) groups
  for D₀, and ≥ 60 G(250) groups for F₀, inside the 150 s timeout (1.6–1.8×). At J = 0 the loop is shorter, so rows
  are faster.
- **The J = 0 rehearsal also reports netem reordering** (the receiver's out-of-order count). The prediction is 0
  (`rate` keeps netem FIFO). Any reorder stops the cell.

### 5.4 Reported, not scored

- **The three-point line rise(J) = a + b·R(J)** over J 0 / 3 / 5, for the law's pair (native minimum over native S_F)
  and for p10, p50 and the dump's IQR.
  - Two parameters on three points leave one degree of freedom, which is not a test. The line is printed with its
    residual and decides nothing.
- **Every convention of §1.1** at each J, and the dump's count test of the median and IQR (§2.6).

### 5.5 What each scored outcome means for decision 8 (A = g(δ)·S_F + q(δ)·F̂, g(Bulk) = 1)

- **SPREAD-CARRIED:** with the jitter removed, the knee needs no margin beyond the rate queue. So on this line the knee
  term is a spread.
  - Decision 8's S_F term gains a sentence along the jitter axis, at a fixed loop.
  - c2 against c3's DISAGREES then lies in rate or delay, which needs that axis before g has provenance across cells.
- **CONSTANT:** the margin survives without jitter. S_F does not carry it, and decision 8's S_F term is a cover with no
  sentence. The knee term needs a non-spread source with its own derivation (for example an engine timing constant),
  and the form is re-derived.
- **INTERMEDIATE:** decision 8 lacks a term. a = need₀ is the part no spread carries, and it needs a sentence before
  anything ships.
- **UNSCOREABLE or UNRESOLVED:** NEEDS-MORE, named (the floor's count, or replicates). Never a refutation.

### 5.6 Cost

| launch | rows | VM time, typical (61a's pace ≈ 1 min/row + ≈ 16 min launch overhead) |
|---|---|---|
| J = 0 rehearsal | pilot + controls | ≈ 22 min |
| J = 0 cell | 36 + 2 dump | ≈ 55 min |
| J = 3 rehearsal | pilot + controls | ≈ 22 min |
| J = 3 cell | 36 + 2 dump | ≈ 55 min |
| J = 5 overlap | 30 + 2 dump | ≈ 50 min |
| **total** | | **≈ 3.4 h typical** |

- The worst case is ≈ 6.5 h, at s61's registered worst-to-typical ratio of ≈ 2× (rows timing out at 150 s).
- **Host cargo is paused for every launch,** as in s61.
- No rebuild: the same binary `e410d640`.
- A tools-only harness change, by a Sonnet worker with stub runs: cell names, the 25 MB floor arm, the D₀ gate, the
  J = 5 drift rule, and the dump pass.

---

## 6. F07 ledger update (proposed diff against `law-ledger.yaml`; not committed)

`law-ledger.yaml` is unchanged on main since `4559bbee`, so the diff applies as written. The tags stay in
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
+  regime: "s60 c2 (status §60, store-law-r68.md §1): need 4.05 ms; sigma + q*l-hat supplies 2.34, D = +1.692 ms [+1.588, +1.836], SHORT. s61 c3 (status §61a re-run Result and its Erratum 1): rho_3 0.457 [0.409, 0.483] against rho_2 0.797, DISAGREES under the registered counting; every candidate COVERED at c3. The window minimum depends on its sample count and does not flatten (c3 floor -7.3 to -9.5 ms native -> 4 200 samples), so need is estimator-dependent, and the label depends on the statistic (d -0.34 registered, about -0.2 at one count, -0.07 p10, +0.20 p50; knee-margin-rederive.md §2.3). The law's own pair, the native-window minimum over native S_F (R6.2's fixed point), DISAGREES on both c3 sessions (d [-0.22, -0.13] and [-0.17, -0.09] against Delta 0.085). c8's landing is coupling-set (> 512); c1s has no knee bracket"
@@
-  fix_class: "owner call (U2: a delta-free spread term, a delta-scaled knee term, or a re-declared q band), then a law change needing a battery; a measured-spread branch (S_F + q(delta)*F-hat, no candidate yet fully provenanced) needs one c3 static sweep first (store-law-r68.md §3)"
+  fix_class: "the law's statistic is the native window minimum (R6.2), whose count dependence is the law's own (R6.7 open item 4); a quantile alternative (window median + IQR) is a candidate, untested (knee-margin-rederive.md §2.6); s62 (sketch, §5) registers the knee rise against the spread along jitter; then the allowance's form, then a law change needing a battery; DISHONEST-INPUT leaves with E13's fix (q*F-hat in place of q*l-hat)"
@@
-  evidence: "s60 C2-SHORT: D = +1.692 ms [+1.588, +1.836] on a CONVERGED floor (status §60 Result item 1); delta's whole authority is 0.28 ms (~5 slots) against a 31-slot shortfall; fixed forms reach the knee only through a constant fitted at c2 Bulk (c_B 2.26 ms, q' 0.26)"
+  evidence: "s61 DISAGREES (status §61a re-run Result; Erratum 1 withdraws 'S_F is not the knee's term' and 'constant in ms'): d_rho -0.340 [-0.396, -0.311] against Delta 0.085 under the registered counting. On bands spanning both c3 sessions, need_3/need_2 is [0.96, 1.22] registered, [0.85, 1.10] engine-native, [1.30, 1.64] at one count: what need scales with depends on the counting. s60 C2-SHORT for the sigma family (D = +1.692 ms); need - sigma is +2.26 at c2 and -0.34 at c3, so A(c)'s sigma + c_B shape fails"
```

- `verdicts` is unchanged: [UNDERIVED, DISHONEST-INPUT].
- DISHONEST-INPUT stays while the registered σ + q·ℓ̂ reads ℓ̂ (E13).
- `derivation:`, `measured`, `gauge`, `depends_on` and `ships_with` are unchanged.

---

## 7. What the owner decides

**Run s62 (§5), or not.**
- **What it gives:** one scored number, need₀ at J = 0. It tells whether the knee margin is a spread or a constant
  along the jitter axis, which is what decision 8's S_F term needs before it has a sentence.
- **What it costs:** ≈ 3.4 h of VM typical (≈ 6.5 h worst) over five launches, with host cargo builds paused for each.
  It also needs a tools-only harness change and its review.
- **What it does not need:** the F2 fix. s62 runs Bulk only on binary `e410d640`, and Bulk does not pack, so F2's
  Realtime framing defect does not reach it.
- **If it is not run,** decision 8's form stays as decided, unsettled and out of the engine, and F07 stays UNDERIVED.
