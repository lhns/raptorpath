# Knee margin after s61's DISAGREES: what need scales with, and the count-free statistic it needs first

Status: DERIVATION, round 2. Docs and Python only: no engine code, no cargo, no VM. Branch `knee-margin-rederive` off
`github/main` `4559bbee`. This round answers the coordinator's round-2 points and status "§61a re-run Result,
Erratum 1" (main `face6093`).

**Every reading here is post hoc,** and so is every statistic chosen on these rows. None was registered.
Confirmation is out of sample, in s62 (§5), and §5's predictions are built around the statistic §2 proposes.

**In plain words.**
- **DISAGREES stands** (Erratum 1). The registered ρ is not invariant across c2 and c3 under the registered counting.
- **"Constant in ms" and "S_F is not the term" are withdrawn,** as the Erratum says. On bands that span both c3
  sessions:
  - need is consistent with constant under the registered counts (×0.96–1.22) and the engine-native counts
    (×0.85–1.10);
  - at one count for both sides it is not (×1.30–1.64). There it is jitter-sized, but no single spread is singled
    out.
- **The minimum is the problem.** A window minimum keeps falling as more samples are pooled, and it does not flatten
  anywhere in these rows: −1.95 ms at c2 and −7.3 to −9.5 ms at c3, from native windows to 4 200 samples.
  - The sample count drives it, not the time span: interleaved and shuffled pooling at a fixed count reads the same.
  - On the jitter-free c1s line it nearly vanishes (−0.12 to −0.18 ms over 32 windows).
  - So any knee reading built on minima carries the estimator's count. Differencing at one count cancels most of it
    (L moves ≤ 0.5 ms over a 4–16× count change), but not to a law-grade level.
- **A count-free replacement:** per-window quantiles, applied to the floor, the knee reading and the spread alike.
  - They are flat across the count table (≤ 0.18 ms, against the minimum's 2–9.5 ms). That is partly by
    construction, as §2.3 explains.
  - **On estimator grounds,** c3's floor windows hold ≈ 21 independent draws. Only quantiles near the centre,
    roughly p25–p75, have ≥ 5 draws beyond them there, so p10 and p90 behave like extremes at c3.
  - So the proposal is the **window median for location and the interquartile range for spread.** The IQR is not
    logged. The logged stand-in (p50 against p90 − p10) DISAGREES consistently, at c2 and both c3 sessions
    (d = +0.20 / +0.22).
- **Decision 8's form is unchanged and is not a derivation here.** F07 stays UNDERIVED.
- **The one owner call is whether to spend ≈ 3 h on s62,** and it comes after review of §2's statistic.

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
   c2 and both c3 sessions). §2.3 applies them.

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
- §10 the quantile statistic: the count table, the independent draws per window, and ρ_q.

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
  and floor dwell long enough for ≥ 6 groups at n\*. That is why §2.3 proposes a count-free statistic instead.

### 2.3 A count-free statistic for all three inputs (`kmr.out` §10; the coordinator's constraints)

**The constraints.**
1. One statistic for the engine and the measurement, computable on the engine's native windows (≈ 60 samples at c3's
   floor, 282 at c2's), with a bias that does not depend on the window's count.
2. Applied to the floor, the knee reading and the spread alike.
3. Shown flat on the count table where the minimum failed, with ρ consistent across c2 and both c3 sessions.

**The family: a per-window sample quantile at a fixed rank fraction p.**

```text
   Q_p(window)  =  the p-quantile of the window's own echo samples            (native windows; no pooling, so no count rule)
   floor   Q_p,F  =  median over the floor windows of Q_p                      knee   Q_p(W₉₅)  =  median over the operating-point windows
   spread  R_F    =  median over the floor windows of (Q_{1−p'} − Q_{p'})      the same windows as Q_p,F
   ρ_q  =  ( Q_p(W₉₅) − Q_p,F ) / R_F
```

**Provenance.**
- p and p′ are declared rank fractions.
- A sample quantile at a fixed fraction is a consistent estimator with O(1/m) bias in its window's count m of
  independent draws. That is standard order-statistics theory (for example Serfling 1980, §2.3; David & Nagaraja
  2003, ch. 4, which also covers the extremes). It holds when the draws beyond the quantile are not few.
- The criterion used here, declared: **m·min(p, 1 − p) ≥ 5 independent draws beyond the quantile.** Below that, the
  quantile behaves as an extreme and inherits the minimum's count dependence.
- **Independent draws per native floor window** (`kmr.out` §10b; acks per window, skbs similar):

  | cell | m per floor window | m·0.10 | m·0.25 | m·0.50 |
  |---|---|---|---|---|
  | c2 W 16 | ≈ 75 acks (59 skbs) | 7.5 | 18.8 | 37.7 |
  | c3 W 12, both sessions | ≈ 21 acks (17 skbs) | 2.1 | 5.2 | 10.5 |

  - At c3's floor, p10 and p90 fail the criterion, the quartiles pass at the edge, and the median passes.
  - **The estimator choice is therefore location Q₅₀ and spread R = Q₇₅ − Q₂₅ (the IQR).** These are the most
    extreme rank fractions that are central at the smallest engine window measured.
  - It is chosen on estimator grounds, before looking at ρ, but it is still chosen on these rows.
- **The IQR is not logged.** `[WSTAT]` prints p10 / p50 / p90 only. So the chosen spread can be shown only through a
  stand-in, p90 − p10, which fails the criterion at c3.

**The count table** (`kmr.out` §10a). Floor pool, median over rows, native windows then G(250 … 4 200); the value is the
change from native to G(4 200):

| statistic | c2 W 16 | c3 61c W 12 | c3 61a W 12 |
|---|---|---|---|
| minimum | −1.95 | **−9.52** | **−7.34** |
| p10 | −0.05 | −0.05 | −0.18 |
| p50 | −0.01 | −0.05 | −0.02 |
| p90 | −0.03 | +0.08 | +0.02 |
| p90 − p10 | +0.03 | +0.18 | +0.11 |

- **The quantiles are flat, but partly by construction.** A group's quantile is carried as the nk-weighted median of
  its windows' quantiles, so the table shows that per-window quantiles are stable when aggregated. It cannot show
  that a window's quantile is free of its own count.
- That needs the raw samples. `RWM_RTT_DUMP` (in the binary, off by default) dumps the RTT sample stream the dispersion
  estimators consume (`raptorpath/src/net/rttdump.rs`). Whether that stream is the `[SLAW]` echo stream must be
  checked before s62 relies on it.
- **The within-arm nk split cannot settle it either** (`kmr.out` §9b). Windows with fewer samples are windows with
  longer delay: under a window-limited store the sample rate is about W/RTT.
  - At c3 W 12, p10 / p50 / p90 / tmin shift +1.80 / +1.39 / +0.03 / +0.07 ms;
  - at c2 W 16, +0.38 / +0.49 / +0.42 / +0.37.
  - c2 is a location shift. c3 is not, and c3's windows are where p10 and p90 are extreme-like.

**ρ built from one statistic, at c2 and both c3 sessions** (`kmr.out` §10c). Every input is a per-window quantile, read
on the u-matched floor. d = ρ₃ − ρ₂ with its 90 % interval, against Δ = 0.085.

| location / spread | ρ₂ | 61c ρ₃ (intercept) | 61c d | 61a ρ₃ (intercept) | 61a d |
|---|---|---|---|---|---|
| **p50 / p90 − p10** (the chosen location; the logged stand-in for the IQR) | 0.655 | 0.855 (1.048) | **+0.200 [+0.121, +0.253] DISAGREES** | 0.878 (1.063) | **+0.223 [+0.160, +0.259] DISAGREES** |
| p10 / p90 − p10 | 0.836 | 0.759 (0.888) | −0.077 [−0.136, −0.029] UNRESOLVED | 0.776 (0.895) | −0.060 [−0.099, −0.014] UNRESOLVED |
| p50 / p50 − p10 | 1.213 | 1.686 (2.116) | +0.473 DISAGREES | 1.719 (2.120) | +0.506 DISAGREES |
| p50 / p90 − p50 | 1.454 | 1.933 (2.485) | +0.479 DISAGREES | 1.923 (2.392) | +0.469 DISAGREES |

- **The chosen statistic disagrees, consistently.** The sign and size are the same on both c3 sessions (d +0.20 /
  +0.22). That is the consistency constraint 3 asks for, here in the DISAGREES direction.
- The p10 row is the only one near agreement. Its location fails the estimator criterion at c3, so it is not chosen,
  and choosing it because it fits would be the fit the advisor warned against.
- **Floor position still matters for quantiles.** ρ₃ moves 0.19 between the u-matched floor and the intercept, because
  the quantile floor rises with W already at the floor pools (FIFO absorption starts there).
- **What this means:**
  - under a count-free statistic, the knee's median rise is a larger fraction of the floor spread at c3 than at c2,
    by about 30 %;
  - no logged statistic makes ρ shape-free across these two cells;
  - whether the IQR does is s62's question.

### 2.4 For the engine-form design (review, not an owner question)

- **The proposed replacement for T̂min and F̂ in the store law's fixed point** is the window median, with the IQR as
  the spread term, all on native windows:

  ```text
     Q₅₀(n*) = Q̂₅₀,F + A
  ```

  - It removes the count rule entirely, which answers R6.7 open item 4 by not needing a rule.
  - It changes the law's sentence: "the operating point's median sits A above the floor's median".
  - The floor dwell and the floor position (R6.7 item 1) remain.
- **It is not proposed for the engine until s62 reads it out of sample.**
- **Gauge needs:**
  - p25 / p75 in `[WSTAT]`. That is a gauge print, so it is zero engine behaviour but a new binary sha; or
  - an `RWM_RTT_DUMP` pass, which computes everything offline with no rebuild but perturbs the sender (rttdump.rs:
    "a dump-on pass is run separately").
  - s62 uses the dump with no rebuild, and reports its own perturbation.

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
  counting (DISAGREES). Under the count-free statistic it is not invariant either (§2.3).
  - g_knee per cell (registered counts, the score) is 0.711 at c2 and 0.256 at c3.
  - No g_R value is recommended: nothing consumes one until an engine form exists.
- **Decision 10** (per-leg `queue_target_mult` = 1 + q(δ)) uses only q(δ), and multiplies Copa's queue floor
  (`scheduler/copa.rs:314`), so it is unaffected.

---

## 5. s62 sketch (not registered): the jitter axis alone, read with the count-free statistic

**Question.**
- With rate, delay and loss fixed, does ρ_q (Q₅₀ rise over the floor IQR, native windows) take the same value at
  netem jitter 0, 3 and 5 ms?
- Does the floor minimum's count drop come from jitter sampling?

**Cells.**
- `c3-25` at J = 0 and J = 3: tools-only cell names, `scenario_params` `20mbit 20 0 2 40` / `20mbit 20 3 2 40`.
- 61c as the J = 5 point, with a two-row overlap arm at J = 5 to bound drift.
- Binary `e410d640`, Bulk only.

**Arms per cell.**
- floors 12 / 16 / 24 (**25 MB**, see below), the pilot-placed K bracket, P1, and the W 512 control: ≈ 45 rows;
- one **`RWM_RTT_DUMP=1` pass** per cell on the floor W 12 and the K₂ arm, reported separately for the dump's
  perturbation.

**Readings** (every convention of §1.1 printed). Scored on ρ_q with the IQR from the dump and Q₅₀ from `[WSTAT]`,
against Δ = 0.085:
- ρ_q(J0) and ρ_q(J3) against ρ_q(J5), and the floor's count drop.

**Predictions** (registered before any row):
1. **The floor minimum's count drop at J = 0.** F(G250) − F(G2 100) at W 12 is 2.8–4.5 ms at J = 5 today. The c1s
   pre-read gives about 0.1 ms on a jitter-free line (≈ 0.035 ms per doubling, three doublings).
   - **< 0.5 ms** means jitter sampling: the drop is netem's jitter.
   - **≥ 1.5 ms** means a non-netem spread (echo timing) or drift.
   - In between is UNRESOLVED.
2. **The dump's count-flatness test for the chosen statistic.** Subsampling the dumped floor windows to m = 60 …
   4 200 samples, Q₅₀ and the IQR stay within 0.05·IQR of their 4 200-sample value for every m at which
   m_acks·0.25 ≥ 5. The minimum fails the same test.
3. **ρ_q shape-free along jitter:** ρ_q(J0) = ρ_q(J3) = ρ_q(J5) within Δ.
   - The post-hoc stand-in reads 0.86–0.88 at J5. The stand-in is not the IQR, so no IQR value is predicted, only
     the equality.
   - A J-dependent ρ_q means the spread does not carry the knee even at fixed rate and delay.
4. **The J = 0 rehearsal reports netem reordering** (the receiver's out-of-order count, as `lib.sh`'s FIFO note
   measured it). The prediction is 0, since `rate` keeps netem FIFO with or without jitter. Any reorder stops the
   cell before 62a.

**Row timeout at W 12.**
- Eight complete G(2 100) groups (for the minimum-based readings) need ≈ 16 800 LATE samples. At ≈ 240 samples/s
  that is ≈ 70 s of LATE after 15 s, so a ≈ 85 s transfer.
- 15 MB took 58.7 s, so that is ≈ 22 MB. **Use 25 MB with a 150 s timeout** (61b's), a 1.6–1.8× margin.
- At J = 0 the loop is shorter, so rows finish sooner.

**Cost.** ≈ 3 h of VM (two rehearsals, two ≈ 45-row launches, two dump passes), a tools-only harness change by a
Sonnet worker, and the registration's review rounds. No rebuild.

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
+  regime: "s60 c2 (status §60, store-law-r68.md §1): need 4.05 ms; sigma + q*l-hat supplies 2.34, D = +1.692 ms [+1.588, +1.836], SHORT. s61 c3 (status §61a re-run Result and its Erratum 1): rho_3 0.457 [0.409, 0.483] against rho_2 0.797, DISAGREES under the registered counting; every candidate COVERED at c3. The window minimum depends on its sample count and does not flatten (c3 floor -7.3 to -9.5 ms native -> 4 200 samples), so need is estimator-dependent (knee-margin-rederive.md §2); under per-window quantiles rho_q also differs across the cells (p50 over p90 - p10: d +0.20 / +0.22 on both c3 sessions). c8's landing is coupling-set (> 512); c1s has no knee bracket"
@@
-  fix_class: "owner call (U2: a delta-free spread term, a delta-scaled knee term, or a re-declared q band), then a law change needing a battery; a measured-spread branch (S_F + q(delta)*F-hat, no candidate yet fully provenanced) needs one c3 static sweep first (store-law-r68.md §3)"
+  fix_class: "estimator first (R6.7 open item 4): a count-free statistic for floor, operating point and spread (proposed: window median and IQR on native windows, knee-margin-rederive.md §2.4), confirmed out of sample (s62 sketch, §5), then the allowance's form, then a law change needing a battery; DISHONEST-INPUT leaves with E13's fix (q*F-hat in place of q*l-hat)"
@@
-  evidence: "s60 C2-SHORT: D = +1.692 ms [+1.588, +1.836] on a CONVERGED floor (status §60 Result item 1); delta's whole authority is 0.28 ms (~5 slots) against a 31-slot shortfall; fixed forms reach the knee only through a constant fitted at c2 Bulk (c_B 2.26 ms, q' 0.26)"
+  evidence: "s61 DISAGREES (status §61a re-run Result; Erratum 1 withdraws 'S_F is not the knee's term' and 'constant in ms'): d_rho -0.340 [-0.396, -0.311] against Delta 0.085 under the registered counting. On bands spanning both c3 sessions, need_3/need_2 is [0.96, 1.22] registered, [0.85, 1.10] engine-native, [1.30, 1.64] at one count: what need scales with depends on the counting. s60 C2-SHORT for the sigma family (D = +1.692 ms); need - sigma is +2.26 at c2 and -0.34 at c3, so A(c)'s sigma + c_B shape fails"
```

- `verdicts` is unchanged: [UNDERIVED, DISHONEST-INPUT].
- DISHONEST-INPUT stays while the registered σ + q·ℓ̂ reads ℓ̂ (E13).
- `derivation:`, `measured`, `gauge`, `depends_on` and `ships_with` are unchanged.

---

## 7. Owner question

**Spend ≈ 3 h of VM on s62 (§5)?**
- It comes after design review of §2's statistic (window median and IQR on native windows). That review is not an
  owner question.
- s62 is the out-of-sample test the statistic needs: is the knee's median rise a fixed fraction of the floor IQR along
  the jitter axis, and is the floor's count drop jitter sampling?
- The alternative is to hold the store-law track until the estimator design is reviewed, and to register s62 then.
- **Recommendation:** run s62 once the review accepts the statistic. Nothing in the engine waits on it today (decision
  8's form is not in the engine), so the order costs nothing.
