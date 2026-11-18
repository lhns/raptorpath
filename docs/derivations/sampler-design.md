# Sampler design: how the store law should sample delay

Status: DESIGN, not code. Docs and Python only: no engine code, no cargo, no VM. Branch `sampler-design` off
`github/main` `e696fc77`. It answers the "sampler-design derivation" that status "The count-rule derivation" names
(`count-rule` `2d557191`): how to estimate the brake, whether the knee's minimum is set by post-loss drains, and what
anchors the floor under jitter.

**Every reading here is post hoc,** on existing rows: s62's rehearsal (J = 0; one row per arm except the floor pair,
the D12 dump row and two W 512 controls), 61c and 61a (c3, J = 5, six replicates per arm), and s60's c2 (W 16–2 048)
and c1s (W 64–2 048) rows, six replicates per arm. Nothing here is registered, and every number is scoped to those rows.

**In plain words.**
- **The brake cannot be estimated well, and designing it out moves it into the allowance's spread S (§2.1).** The brake
  exists only because the law compares a
  window minimum against an anchor taken at another sample count. The minimum is never count-free, so any brake must
  be read at the operating window's own count, from zero-load samples, and that takes several knee-sized windows of
  zero-load history the start-up does not have.
  - Read at load, a brake is circular. It funds its own queue, and R never crosses 1 (gate table, §1.5, K2).
  - Read at boot, as b̂₀⁺ is, it is the wrong count and the wrong moment. Even a perfect steady-state brake at the
    floor's count exceeds the brake at the knee's count by 3.2 ms (J = 0), 4.6 / 4.2 ms (J = 5) and 1.6 ms (c2)
    (§1.1). b̂₀⁺ adds its own start-transient spread on top, up to one row's 103 ms latch.
  - **A median law** is count-free at a declared ε once a window holds m ≥ 1/(ε·½) = 10 independent draws.
    b̂₀⁺, ℓ̂ and the min(m̂, ·) branch leave the law.
  - **But the brake moves into S.** To state "the queue never empties", g·S must carry the zero-load
    median-to-minimum gap at the operating count. That gap is S_F plus the brake shift.
    - S_F as decided brings the count dependence back.
    - The IQR is count-free, but it no longer states the sentence.
  - **So the choice of statistic and the choice of S are one owner question** (§6, O1): which sentence the law holds.
    I recommend the median with the IQR, conditional on M1.
- **The knee's minimum is lower in ticks that carry a retransmit** (an association; the drain mechanism is a
  hypothesis).
  - At W 80–124 on both c3 sessions, the ticks without a retransmit read a higher window minimum than those with one,
    on 6 of 6 rows at every arm.
  - The gap is +1.2 to +4.2 ms at W 100–112, and grows above it.
  - The window median moves less below the knee (−0.8 to +1.2 ms by class, of mixed sign), against the minimum's −0.2
    to −3.1 ms in retransmit-heavy ticks. At and above the knee it also rises in retransmit-free ticks (+0.6 to
    +2.8 ms).
  - The send-pause mechanism is observed only on one floor dump row.
- **The floor under jitter: no minimum anchors it.**
  - At J = 5, m̂ rises with load, by +3.2 to +6.7 ms from the floor arm to the knee arms. That is more than the native
    minimum rises.
  - Across replicates, the floor arm's window median is the tightest location at c3 and c2. It spans 0.10 / 0.39 /
    0.54 / 0.11 ms (J = 0 / 61c / 61a / c2), against 1.4–96 ms for R6.2's F.
  - **Not at c1s:** there the median spans 1.91 ms against the minimum's 0.16. Two rows' medians move where their
    minima do not (§4.2).
  - Count-freeness on raw samples is shown only at J = 0 (the D12 dump).
- **The proposed floor is boot-set, and it cannot see a delay drop.**
  - It is the median of the K lowest-store windows, which needs K ≥ 7 on these rows. Its boot error is ≤ 0.11–0.28 of
    each cell's knee need.
  - A path delay that falls after boot leaves a permanent standing queue. That blocks adoption, as does the deleted
    multi-leg coupling (§4.2).
- **One cheap measurement tests the proposal, M1** (§4.1): four `RWM_RTT_DUMP` rows with G twins (c3 J = 5 floor and
  knee, c2 floor, c3 J = 0 knee), about 1 h of VM. It registers g at the Realtime knee in [0.84, 1.06] at c2 and c3.
- **E14 (b̂₀⁺) is committed to the ledger** (`acb62033`). F07 is held.

**Advisor (Fable) log.**
1. **Before the first write** (applied):
   - screen every brake and floor candidate for circularity with a gate table (§1.5);
   - give section A's power per arm, with a dose reading and the no-retransmit counts;
   - lead Q3 with m̂'s rise under load;
   - report the median of the boot brakes as biased low by construction, not as a candidate;
   - report the anchor branch's bind fraction;
   - make the statistic change an owner question;
   - start the F07 text from `knee-margin-rederive` §6;
   - close the B1 check.
2. **Review of `3822f960`** (applied):
   - read the proposal's start-up on the boot windows (new §1.6), since a lowest-store floor in a closed loop is a
     boot-set floor;
   - scope or correct four claims:
     - m̂ at J = 0 W 128 is on the low mode in 79 % of windows, not always;
     - the median at W 16;
     - "drains" is now an association;
     - the gate table's PASS is for the form, on a session-calibrated floor;
   - scope §4's formula to N = 1, with the half-gap coupling open;
   - note that crfollow's criterion saturates at load.
3. **Coordinator's round after Fable's review** (applied):
   - §2.1 sets each law beside its sentence, and merges O1 and O2;
   - §2.2 names the lowest-store set, its re-anchoring and a slower link;
   - §1.6 gives the error per cell against the knee need;
   - MODE-SWITCH is checked against the ledger's definition (neither set is tagged);
   - the gate table covers c2 and c1s;
   - §4.2 lists what blocks adoption, the multi-leg coupling first;
   - §4.1 sketches M1's registration and verifies the g_R prediction;
   - E14 is committed.

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place) | output | what it computes |
|---|---|---|---|
| `store-law/sdalign.py` | `l1-raw/s62/rep62r0-diag.tgz`, `l1-raw/s61/rep61c-diag-ld.tgz`, `rep61a-diag-ld.tgz`, `l1-raw/s60/rep60a-diag-ld.tgz`, `rep60b-diag-ld.tgz` | `sdalign.out` | A: retransmits against the window minimum per arm, with the dose and the anchor bind. B: the brake estimators per floor row. C: the floor's location statistics and their rise to each arm. D: the gate table for five floor candidates. E: start-up (the first 8 windows, the K-window medians), and m̂'s low-mode share at J = 0 |

**The witness.** The script's floor criterion reproduces `crfollow.out` D on all three J = 0 floor rows: 0.32 / 0.87,
0.31 / 0.84 and 0.29 / 0.84 (share of windows, and their retransmit share). Every window's paired `[DIAG]` tick lies
within 60 ms of its `[SLAW]` close (0 mismatches).

---

## 0. Constraints

| id | constraint | source |
|---|---|---|
| C1 | One formula, continuous in δ; ρ never selects; no predicate on k, t, n or a count | CLAUDE.md NO-MODE-SWITCH; R6.7's mode screen (`store-law-r67.md` §3) |
| C2 | Every symbol has provenance; every clamp has a bind-fraction gauge | CLAUDE.md FORMULA-FIRST |
| C3 | No term read at load may fund its own queue: R > 1 at the floor, R < 1 at the controls, one crossing | R6.2 uniqueness, `store-law-r68.md` §1.4 gates 1–2 and the R-sign gate |
| C4 | The floor and the steered statistic are the same statistic at a matched count, or a count-free one | §61a re-run Erratum 1; `knee-margin-rederive.md` §2.4 ("the count dependence belongs to the law") |
| C5 | Defined from the first window. No hold and no k ≤ K predicate. Any value the boot sets comes through an order statistic that survives transient windows, not through an extreme | R6.7 erratum to R5.5 (a hold is a term with no provenance); the b̂₀⁺ defect (count-rule §2) |
| C6 | Inputs come only from `record_rtt_sample`'s stream, O(1) amortised per sample, within the existing per-window bound | R6.2 input table; `net/slaw.rs` `WIN_SAMPLE_CAP` |
| C7 | Decision 8's form stays as decided: g(δ)·S_F + q(δ)·F̂, g(Bulk) = 1 | Owner decision 8 (2026-11-17) |

---

## 1. What the rows show (established, scoped)

### 1.1 The brake against the count (`sdalign.out` B)

The brake is the window minimum minus the long-run minimum m̂, at zero load. Here it is read on the floor arms.
Columns: min / median / max over rows, in ms.

| cell (rows) | b̂₀⁺ as printed | median of the 8 boot brakes | steady state at the native count (= s62's D₀ construction) | at the knee's count (8 pooled floor windows) |
|---|---|---|---|---|
| c3 J = 0 (3) | 4.03 / 5.35 / 5.40 | 1.13 / 2.99 / 3.38 | 3.23 / 3.32 / 3.61 | 0.12 / 0.12 / 0.28 |
| c3 J = 5, 61c (6) | 3.26 / 9.53 / 12.62 | 0.00 / 2.15 / 5.35 | 7.46 / 8.12 / 8.64 | 2.72 / 3.56 / 4.73 |
| c3 J = 5, 61a (6) | 6.35 / 11.01 / **103.40** | 0.34 / 2.51 / 7.30 | 5.59 / 6.25 / 8.38 | 0.95 / 2.02 / 4.30 |
| c2 (6) | 2.04 / 2.80 / 4.10 | 0.00 / 1.14 / 2.07 | 2.17 / 2.38 / 2.70 | 0.45 / 0.76 / 1.13 |

- **The brake is a function of the count, b(n).** From the floor's native count (60 at c3, 282 at c2) to the knee's
  (≈ 480 / ≈ 2 250), it falls by 3.2 ms (J = 0), 4.6 / 4.2 ms (J = 5) and 1.6 ms (c2).
- **b̂₀⁺ is neither.** It is a maximum over 8 start-transient windows, so one window sets it, as count-rule §2 found.
- **The median of the 8 boot brakes is biased low by construction.** A window that sets m̂ has T̂min = m̂, so its brake
  is exactly 0, and window 1 always does. The rows hold 1–6 exact zeros among their 8. It is reported here, not
  proposed.
- **The B1 check owed by count-rule is closed.** 61a G12 s42 r5 is the 103.40 ms brake row, and its LATE median `err` is
  −101.25 ms. That is the B1 outlier status §61a lists as G12 s42 r5 with margin +101.2.

### 1.2 Retransmits and the window minimum, at every load (`sdalign.out` A, A2)

- **Why not crfollow's criterion alone.** "T̂min more than 4 ms below the row's median tp50" marks 0.29–0.32 of windows at
  the J = 0 floor. At W ≥ 80 it marks 0.65–1.00, so it saturates under load. The threshold-free dT and the dose carry
  the alignment there.
- **Power.** The share of ticks carrying a retransmit (base) is 0.28–0.35 at the c3 floors and 0.72–0.90 at W 80–112.
  At W ≥ 120 and the controls it is 0.94–1.00, so low-decile and group-minimum shares there carry no information.
  - The informative arms are W 80–112.
  - J = 0 W 80 and W 128 are one row each, with 16 and 6 no-retransmit windows.
- **dT**, the median T̂min of no-retransmit ticks minus that of retransmit ticks, is > 0 on 6 of 6 rows at every
  J = 5 arm from W 12 to W 512 (61c), and from W 12 to W 124 (61a). For one arm, 6 of 6 has p = 1/64 under no
  association. At c2 it is > 0 on 4 of 6 rows.
- **The dose** (T̂min minus the row's median, pooled per arm; the class's median nk in brackets):

  | arm | retx 0 | retx 1–2 | retx 3–5 | retx ≥ 6 |
  |---|---|---|---|---|
  | c3 J = 0 W 12 | +0.89 (60) | −2.47 (48) | −1.71 (60) | −3.11 (42) |
  | 61c W 80 | +0.55 (388) | +0.04 | −0.29 | −0.19 (352) |
  | 61c W 100 | +1.21 (467) | +1.10 | +1.08 | −0.48 (432) |
  | 61c W 108 | +3.06 (481) | +1.00 | +1.48 | −0.67 (454) |
  | 61a W 104 | +2.30 (479) | +1.05 | +1.41 | −0.37 (446) |
  | 61a W 112 | +4.16 (480) | +3.01 | +2.84 | −0.57 (463) |
  | c2 W 16 | +0.07 (294) | −0.25 | +0.00 | −0.03 (254) |

- **The count does not explain it.** Retransmit-heavy ticks hold fewer samples (42 against 60 at the floor, 454 against
  481 at W 108). Fewer samples raise a minimum, so the count works against the association. It does not produce it.
- **The window median, same classes:**
  - below the knee it moves −0.82 to +1.21 ms by class, of mixed sign:
    - within ±0.21 ms at W 80–104 and J = 0 W 80;
    - at the floors, within ±0.15 ms except the small 3–5 class (−0.46 to −0.82);
    - up to +1.21 in the 1–2 and 3–5 classes at W 16 / 24;
    - against the minimum's −0.2 to −3.1 in the ≥ 6 class;
  - at and above the knee it responds: no-retransmit ticks read +0.63 (61c W 108), +1.29 (61a W 112) and +2.2 to +2.8
    (W 120–152) ms.
  - Hypothesis: above the knee a repair drains the queue for a large part of the window, not only for one sample.
- **The anchor branch of R6.2** (`ancf`: windows where m̂ > ℓ̂ + b̂₀⁺, cumulative over the row) binds on 0 windows on
  64 of the 71 c3 rows below the controls (and on 5 of 6 c2 rows). The others are 15–103 of ≈ 220 on single rows (61c W 12 / 80 / 100; 61a W 92;
  3 of 6 at 61a W 104; c2 1 of 6, 28 / 310). At 61a's W 2 048 control it binds 154–169 of ≈ 208. **Over the c3
  operating range the min(m̂, ·) clamp is degenerate:** F reduces to m̂ + b̂₀⁺.

### 1.3 The floor's location statistics (`sdalign.out` C)

**Across replicates of one floor arm**, in ms: the median over rows, [min, max] and range.

| statistic | c3 J = 0 (3) | 61c J = 5 (6) | 61a J = 5 (6) | c2 (6) |
|---|---|---|---|---|
| m̂ | 41.33, range 0.07 | 34.16, range 1.68 | 35.33, range 3.42 | 6.35, range 0.89 |
| ℓ̂ | 41.03, range 0.23 | 32.95, range 2.70 | 33.07, range 2.62 | 5.59, range 1.24 |
| R6.2's F | 46.61, range 1.37 | 44.20, range 9.00 | 46.74, range **95.56** | 9.38, range 2.47 |
| C (last 4 native minima) | 45.57, range 2.45 | 43.35, range 1.42 | 41.03, range 6.83 | 8.84, range 1.34 |
| window T̂min | 44.60, range 0.24 | 42.75, range 0.52 | 42.36, range 1.84 | 8.68, range 0.17 |
| window p10 | 46.49, range 0.08 | 44.42, range 0.66 | 44.38, range 1.51 | 10.69, range 0.17 |
| **window p50** | **46.74, range 0.10** | **48.73, range 0.39** | **48.47, range 0.54** | **13.70, range 0.11** |

**From the floor arm to the load arms** (the rise of each arm's median over rows, in ms):

| arm | m̂ | T̂min | p10 | p50 |
|---|---|---|---|---|
| J = 0 W 80 (u 0.81) | −0.24 | **−2.26** | −1.06 | +0.03 |
| J = 0 W 128 (u 0.99) | −0.06 | +10.08 | +11.66 | +17.65 |
| 61c W 80 | **+3.79** | +0.35 | +2.45 | +3.13 |
| 61c W 100 | +4.82 | +2.21 | +4.18 | +4.76 |
| 61c W 108 | +6.15 | +3.21 | +5.78 | +6.65 |
| 61c W 120 | +6.70 | +6.48 | +9.71 | +11.38 |
| 61a W 92 / 104 / 112 | +3.16 / +4.22 / +5.17 | +1.44 / +3.08 / +4.42 | +3.31 / +5.17 / +6.99 | +4.09 / +5.92 / +8.43 |

- **c1s (W 64, 6 rows):**
  - m̂ 2.26 (range 0.12), T̂min 2.39 (range 0.16), p50 2.84 (range **1.91**);
  - its median rises +0.37 / +0.77 at W 128 / 192, against T̂min's +0.04 / +0.14.
  - c2's median rises with W at every arm (+0.57 at W 32 to +11.70 at W 256).
- **At J = 5, m̂ is not a floor.** It rises with load by more than the native minimum below the knee. It is a 10 s
  minimum of a queued, jittered stream, so it tracks the queue's low excursions.
- **At J = 0, m̂ sits on the queue-free low mode (< 41.9 ms):**
  - in 1.00 of LATE windows at the floor and at W 80;
  - in 0.79 at W 128 (range 41.18–44.65 ms);
  - in 0 at W 512.
  - That is one row per load arm (`sdalign.out` E).
- **At J = 0 the native minimum falls under load** (−2.26 ms at W 80), because the W 80 window holds 395 samples and
  carries a retransmit in 90 % of ticks. At a matched count the W 80 rise is +0.84 (`crrule.out` §2). A native-count
  law therefore reads a negative rise below the J = 0 knee.
- **The window median below the knee:**
  - flat below the J = 0 knee (+0.03 at W 80);
  - at J = 5, within the floor's replicate range at W 16 (−0.15 / +0.03);
  - rising from W 24 (+0.67 / +0.70) and over every later arm read.
  - The native minimum is not monotone there: −0.5 to −0.6 at W 16 / 24, s61's known "Monotone: NO".

### 1.4 Count-freeness on raw samples (cited)

- **J = 0, the D12 dump** (`crdump.out` §4), native → 36 pooled windows:
  - minimum +3.27 ms, p05 +0.39, p10 +0.22, p25 +0.08, **median +0.045**.
- **J = 5:** no dump exists.
  - `kmr.out` §10a's flat quantile table is flat by construction: a group's quantile there is the weighted median of
    its windows' quantiles.
  - So count-freeness under jitter is untested on raw samples.
- **The criterion** (`knee-margin-rederive.md` §2.5; David & Nagaraja 2003 §2.1–2.2): the sample p-quantile of m
  independent draws is count-free to relative tolerance ε iff m·p ≥ 1/ε.
  - With ε = 0.2 (declared there): p = ½ needs m ≥ 10.
  - c3 floor windows hold ≈ 17–21 independent draws (the netem one-draw-per-skb model, kmr §2.5), so the median
    passes, p25 sits at the edge and p10 fails.

### 1.5 The gate table (`sdalign.out` D; one leg, every cell, medians over rows)

**Setup.**
- margin(W) = F + A − statistic(W), with A from decision 8 at Bulk (g = 1, q = 0.10).
- A uses the cell's native floor numbers: S_F = the floor arm's median window (p50 − T̂min), F̂ = its median T̂min.
- For K5 that S_F is a placeholder, because decision 8's spread mapped to the median is not logged. So K5's crossing W
  is not read; only its signs and its count of sign changes are.
- **The gates:** margin > 0 at the floor arm, < 0 at the control, and exactly one sign change.

| | floor candidate F | statistic | J = 0 | 61c | 61a | c2 (knee W₉₅ ≈ 168) | c1s (control COLLAPSED: direction only) |
|---|---|---|---|---|---|---|---|
| K1 | R6.2 as gauged on that arm: min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺ | T̂min | FAIL (+178 at W 512) | FAIL (+189 at W 512) | PASS | PASS | FAIL (+4.1 at W 2 048) |
| K2 | a running brake: m̂ + median over the arm's windows of (T̂min − m̂) | T̂min | FAIL (+5.9 to +6.7 everywhere) | FAIL (+7.3 to +10.2) | FAIL (+9.4 to +10.2) | FAIL (+5.8 to +6.1) | FAIL (+0.6 to +0.7) |
| K3 | candidate C at the floor's count: the floor's median T̂min, fixed | T̂min | PASS, crosses W 80–128 | PASS, crosses W 120–152 | PASS, crosses W 124–2 048 | PASS, crosses W 192–256 | PASS, crosses W 192–2 048 |
| K4 | count-matched: M₀(n_W), the median minimum of round(n_W / n_floor) pooled floor windows | T̂min | PASS, crosses W 80–128 | PASS, crosses W 108–120 | PASS, crosses W 112–124 | PASS, crosses W 160–176 | PASS, crosses W 192–2 048 |
| K5 | the floor's median window p50, fixed | window p50 | PASS (crossing not read) | PASS (crossing not read) | PASS (crossing not read) | PASS (crossing not read) | PASS (crossing not read) |

- **What PASS means here.** K3–K5 take their floor from the session's own floor arm, on static pools. A PASS clears the
  form of the target (sign at the floor, sign at the control, one crossing). It does not clear an engine estimator of
  that floor: §1.6 reads the boot-set version.
- The c3 J = 5 rows include W 16 and W 24, and c2 has W 16–256 plus the 2 048 control. They do not change any label.
- At c2, K4 crosses at the knee (W 160–176), and K3, the native-count floor, crosses 24–88 slots above it.

- **K1** fails on static pools because b̂₀⁺ latches each large pool's own start queue (R6.7 §1.2, already known). In a
  closed loop booting from n₀ this replay does not apply. It is listed for completeness, not as a new finding.
- **K2 fails by construction.** Any brake read at load makes F ≈ T̂min, so R − 1 ≈ A/T̂ > 0 everywhere. This rules out
  every running or at-load brake estimator (C3).
- **K3 against K4 is the hidden term.** At the knee arms, K3's margin exceeds K4's by 3.15 ms (J = 0 W 128), 4.76
  (61c W 108) and 4.74 (61a W 104 / 112).
  - That is b(n_floor) − b(n_knee): an allowance with no provenance, which depends on the jitter, the loss process and
    the rate ratio.
  - It is consistent with the native need (3.71 / 4.10 ms at c3, kmr §2.3) being smaller than the one-count need
    (8.87 / 7.99).

### 1.6 Start-up: what a boot-set floor reads (`sdalign.out` E)

**The setting.**
- In a closed loop booting at n₀, the store ramps up, so the K lowest-store windows are the boot windows. So F̂ of §2.2
  is set at boot, like b̂₀⁺: a median over K windows where b̂₀⁺ takes a maximum over 8.
- On a static row the lowest-store set with ties to the most recent is the last K windows. So this section reads the
  **first K windows**, which is what the lowest-store set is in a ramping boot (§2.2).
- This is a stand-in, not a closed-loop boot from n₀ = 10. Those rows do not exist.

**What the boot windows hold.**
- **Window 1 is a start transient on all 21 floor rows:**
  - nk 145–438 against a steady 60 (c3) or 282 (c2);
  - p50 +1.1 to +64.9 ms above the LATE median.
- **Two rows carry more transient windows:**
  - 61a G12 s7 r6: 2 windows (p50 82.4, 139.3);
  - 61a G12 s42 r5: 3 windows (61.4, 250.8, 71.1). This is the b̂₀⁺ 103 ms row.

**The median of the first K windows minus the LATE median**, ms (min / max over the 21 c3 / c2 rows):

| statistic | K = 1 | K = 3 | K = 5 | K = 8 |
|---|---|---|---|---|
| window p50 | +1.09 / +64.93 | −3.11 / +34.01 | −1.64 / +13.07 | −1.21 / +2.00 |
| window T̂min | −3.68 / +5.15 | −5.49 / +1.84 | −3.28 / +1.69 | −2.55 / +1.13 |

**Reading.**
- **A median over K windows survives ⌊(K − 1)/2⌋ transient windows.** On these rows the transient lasts up to 3
  windows, so K ≥ 7 is needed. At K = 8 the boot-set median lands within −1.21 / +2.00 ms of the LATE median on all 21
  rows, up to 20× the floor's replicate range (§1.3).
  - K = 8 sits inside R6.7 §3's band of 3–11 and is forced toward its top. Under the order-statistics rule this is
    a condition, not a constant: K = 2·(transient windows) + 1.
  - The transient length is measured on these rows only.
- **Per cell, at K = 8, against the knee need in median terms** (need₅₀, kmr §2.3):

  | cell | K = 8 error (min / max) | |error| / need₅₀ |
  |---|---|---|
  | c3 J = 0 (3 rows) | −1.21 / +0.05 ms | no knee bracket |
  | 61c (6) | −0.58 / +0.91 | ≤ 0.13 (need₅₀ 6.92) |
  | 61a (6) | +0.39 / +2.00 | ≤ 0.28 (need₅₀ 7.17) |
  | c2 (6) | −0.18 / +0.39 | ≤ 0.11 (need₅₀ 3.61) |
  | c1s (6) | −1.92 / +0.19 | no knee bracket; 0.68 of c1s's 2.84 ms floor median |

- **At start-up the count-free median carries no advantage over the minimum here.** Both are set by the boot windows'
  transient. The median's advantage is the count (§1.4), not the boot.
- **So the proposed F̂ does have a boot latch,** as r67's candidate C does. Its costs carry over:
  - a path change is seen only after a collapse to the store floor (r67 §4.3, filed under §13.1);
  - the floor tracks the boot window's state, which differs from LATE by up to 2 ms on these rows. One J = 0 row,
    G12 s42 r1, reads −1.2 ms at every K ≥ 3: its early bulk sat lower.

---

## 2. Q1: the brake

**Forced by C3 and C4.**
- The steered statistic and the floor must be the same statistic at the same count.
- Under a minimum, F must be the zero-load expected minimum at the operating window's own count:

```text
   F(n)  =  M₀(n)  =  E₀[ min of n echo samples ]        brake b(n) = M₀(n) − M₀(∞)
```

- M₀(n) is not available at load: K2 shows that any at-load reading is circular.
- So it must come from zero-load samples at counts ≥ n.

**The three estimator families**:

| family | example | fails | evidence |
|---|---|---|---|
| at load | running median of T̂min − m̂ | C3: circular, no crossing | §1.5 K2 |
| boot-time | b̂₀⁺ (the max of 8); the median of 8 | C4: the boot count, not n*; C5: the max latches one transient, the median is biased to 0 by the m̂-setting windows | §1.1 |
| zero-load ladder | K4: the median of B group minima, each over ⌈n/n̄₀⌉ pooled floor windows | **its input count** (below) | §1.5 K4 passes the gates |

**The ladder's input count.**
- B ≥ 3 groups are needed for a median, so it needs B·n* zero-load samples: ≥ 3·n*/n̄₀ floor windows.
- On these rows n*/n̄₀ ≈ 8 (c3 480 / 60, c2 2 250 / 282), so that is ≥ 24 zero-load windows (6 s).
- **At start-up the store ramps from n₀,** at R₀ ≈ 1.19 per window (`store-law-r67.md` §4.3). It passes the knee's count
  long before it has 24 zero-load windows.
  - The law would then need M₀ at a count it has not observed.
  - That takes an extrapolation, which has no provenance, or a hold, which C1 / C5 forbid.
- **At J = 5, M₀(n) has no limit.** It falls 0.3–1.6 ms per doubling up to 4 200 samples (`crrule.out` §1, 61c /
  61a). So any residual count mismatch is a bias of that size.
- Pooling non-contiguous windows is allowed by `knee-margin-rederive.md` §2.1: at a fixed count, interleaved and
  shuffled pooling read the same as contiguous pooling, on c2 / c3 rows.

### 2.1 The brake moves into S; it does not leave (round 3)

**The two laws, each beside its sentence** (one leg, at the operating point n*):

```text
   minimum law:  T̂min(n*)  =  M₀(n*) + A
      sentence:  the window's least-queued sample still waits A above the zero-load minimum at the same count,
                 so the queue never empties within a window and the bottleneck is never idle (R3.1's sentence)

   median law:   T̂(n*)  =  (1 + q(δ))·F̂₅₀ + g(δ)·S
      sentence:  the window median sits q·F̂₅₀ + g·S above the zero-load window median
```

**What keeps the bottleneck busy under the median law.**
- Busy means T̂min(n*) > M₀(n*). Write Ŝ(n*) = T̂ − T̂min at load and S₀(n*) = F̂₅₀ − M₀(n*), the zero-load
  median-to-minimum gap at the same count. Then:

```text
   busy   ⇔   T̂(n*) − Ŝ(n*)  >  M₀(n*)   ⇔   q·F̂₅₀ + g·S  >  Ŝ(n*) − S₀(n*)
   S₀(n*)  =  S₀(n_floor) + [ M₀(n_floor) − M₀(n*) ]  =  S_F + Δb          (Δb: §1.1, 3.2 / 4.6 / 4.2 / 1.6 ms)
```

- **So g·S has to carry the median-to-floor gap the brake carried.** Stating "the queue never empties" under the
  median law takes S₀ at the operating count, and that is S_F plus the brake shift Δb.
- **With S = S_F as decided** (the floor windows' median − minimum, at the floor's count):
  - the busy condition carries Δb, which is the count dependence of §1.1 come back inside S;
  - native S_F is 5.86 against 9.71 at c3's registered counts (r68 §1.4), and c8 leg 1's S_F rises with nk;
  - this mix carries (a)'s brake without stating (a)'s sentence.
- **With S = the floor IQR**, every term is count-free, but the sentence no longer says the queue never empties.
  - It places the median and says nothing about Ŝ(n*) or the minimum.
  - Whether the bottleneck stays busy at the landing is carried by g, measured at the knee per cell. That is where
    decision 8 already stands.
- **So the earlier O1 and O2 are one question** (§6, O1): which sentence the law holds.

**S cannot be chosen by which one agrees.**
- On rows in hand, the knee need in median terms (3.61 / 6.92 / 7.17 ms at c2 / 61c / 61a, kmr §2.3) is:
  - 0.711 / 0.713 / 0.750 of the registered S_F, which agrees across cells;
  - 0.65 / 0.82 / 0.85 of native p90 − p10, which does not.
- **Neither is evidence for a sentence:**
  - the registered S_F is a G(250)-count statistic, so its agreement may come from its count convention;
  - `knee-margin-rederive.md` §2.3 shows that two cells can give every label from the same rows;
  - a spread picked for its agreement is a fit.
- S must be fixed by its sentence, then tested (M1, §4.1).

### 2.2 The proposal, conditional on M1: a count-free location

```text
   T̂ᵢ,ₖ  =  median of window k's echo samples                       (already the R denominator)
   F̂ᵢ    =  median{ T̂ᵢ,ₖ : the K windows of leg i with the lowest store size so far; ties → the most recent }
   window k closes at the later of 250 ms and M independent draws    (M = 1/(ε·½) = 10 at ε = 0.2)
   brake  ≡  0                                                       (both sides are the same count-free statistic)
```

**Which floor set: the K lowest-store windows, not the first K.**
- **No index predicate.** "The first K windows" is the k ≤ K set b̂₀⁺ uses. The lowest-store set is an order
  statistic on the store size over every window so far.
  - By the ledger's own definition (`law-ledger.md` §1), MODE-SWITCH is only a hint equality, the ρ boolean, or a
    δ / ρ threshold selecting a law. Neither set is tagged, and both carry a column-8 note. E14 is committed that way.
- **It can re-anchor.** A window at a store size at or below the boot's replaces a boot window, ties going to the most
  recent. The first-K set never changes.
- **In a closed loop booting from n₀ the two coincide** until the store first returns to its boot size. §1.6 reads the
  first K windows for that reason. On a static row the lowest-store set with ties to the most recent is the last K
  windows, a rolling LATE reading that says nothing about boot.

**When the path's delay changes after boot** (m̂ rolled every 10 s; this floor does not):
- **The delay rises by Δd.**
  - If Δd < q·F̂ + g·S, the law lands Δd lower and keeps less margin, possibly below the knee, with no signal.
  - If Δd is larger, R < 1 at every n and the store falls to its floor bound. Once a window's store size ties or
    undercuts the boot windows', F̂ refreshes and the law recovers.
  - So each rise costs one collapse, and recovery needs the store floor ≤ the boot size (r67 §4.3's "route seen only
    after a collapse").
- **The delay falls by Δd.** F̂ stays Δd too high. The law holds a standing queue of about (1 + q)·Δd for good: the
  store grows, no window undercuts the boot size, and F̂ never refreshes.
  - **That is a defect, and it blocks adoption (§4.2).**
- **The options all fail a constraint today:**
  - a rolling lowest-store set over a horizon H reads load windows, K2's circularity in part;
  - a periodic zero-load probe (BBR's ProbeRTT, Cardwell et al. 2016) is a phase predicate on t, which fails C1's
    screen;
  - a rolling minimum of window medians rises with load, as m̂ does at J = 5 (§1.3).
- R6.2's F had the same exposure through b̂₀⁺, and m̂ was its only rolling part.

**The input count, and a slower link.**
- A window's median is count-free to ε iff it holds m ≥ 1/(ε·½) independent draws: 10 at the declared ε = 0.2.
- c3's floor windows hold 17–21 (kmr §2.5's model: one draw per data skb and per ack packet; the binding count is the
  smaller).
- On a slower link a 250 ms window holds fewer. Two continuous choices, neither a rate threshold:
  - **(i) Proposed: the window closes at the later of 250 ms and M = 10 draws.**
    - The window length is max(T₀, M/λ), continuous in the rate λ. Every link runs the same statistic and the same
      law, and only the cadence of R slows below about 40 draws per second.
    - The draw count is estimated in the engine as min(ack packets received, data send batches) in the window. That
      is kmr §2.5's model, declared.
    - The max is a clamp, so it gets a bind gauge: the share of windows extended past 250 ms.
    - At c3's floor (≈ 21 acks per window), c2 and c1s it does not bind.
  - **(ii) Not taken:** keep 250 ms and accept a level error that grows as 1/(m·½). It keeps the cadence and loses
    the stated ε.

**Start-up.**
- F̂ is defined from the first window, then from the K lowest-store windows as they arrive. There is no hold.
- K must exceed twice the boot transient (§1.6): K = 2·(transient windows) + 1, so K ≥ 7 on these rows. It is read as
  8, inside R6.7 §3's band.
- Its error at K = 8, against each cell's knee need, is ≤ 0.11 (c2), 0.13 (61c) and 0.28 (61a) of need₅₀ (§1.6).
- Until K windows exist the floor includes the transient (+1 to +65 ms at K = 1), so the first windows run on a high
  floor.
- A closed-loop boot from n₀ is unread.

**What this deletes:**
- b̂₀⁺ and `K_GAUGE`;
- ℓ̂ (already E13);
- the min(m̂, ·) branch (degenerate, §1.2);
- m̂ and T̂min as law inputs. They stay as gauges, and m̂ stays as Copa's input.
- **The multi-leg half-gap coupling goes too, with no replacement** (§4.2).

**If the owner holds the minimum's sentence instead** (O1 (a)):
- The ladder K4 is the only family that passes C3 and C4.
- It carries the input count and start-up gap above, and it must interpolate in log n between group sizes (C1: no step
  in n).
- Its clamp at the largest available group needs a bind-fraction gauge (C2).

---

## 3. Q2 and Q3

**Q2: is the knee's window minimum also set by post-loss drains?**
- **Established, as an association** (§1.2):
  - on the informative arms (W 80–112, base 0.72–0.90), ticks without a retransmit read a higher minimum, on 6 of 6
    rows of every arm in both c3 sessions;
  - the gap grows with load: +0.6 at W 80 to +3.1 / +4.2 at W 108 / 112;
  - the count works against the association.
  - c2's knee arms have only 6–8 retransmit-free windows per arm, so they carry no power.
- **Hypothesis:** the mechanism is count-rule's, a repair stalls the release and the following flight meets a drained
  queue.
  - The send-pause alignment is observed on the D12 floor dump only.
  - At the knee no dump exists, and J = 0 has no knee bracket (W 80 and W 128 are one row each).
- **For the law:** on these rows the window minimum is a statistic of the loss process at every load, the knee
  included. The window median moves less below the knee, and responds at and above it.

**Q3: what anchors the floor under jitter? Not a minimum.**
- **Established** (§1.3):
  - m̂ rises +3.2 to +6.7 ms under load at J = 5;
  - ℓ̂ is DISHONEST-INPUT (E13);
  - R6.2's F spreads 9.0 / 95.6 ms across replicates.
  - The floor window's median spreads 0.39 / 0.54 ms at J = 5 and 0.11 at c2, the tightest location in those cells. At
    J = 0, m̂ ties it (0.07 against 0.10).
  - **At c1s the opposite holds:** the median spreads 1.91 ms against T̂min's 0.16 (§1.3).
- **Proposed:** the anchor is F̂ of §2.2, the zero-load window median. Its sentence is "the loop the store sees with no
  standing queue". At J = 0 that includes the bulk's ≈ 5.4 ms offset above the queue-free mode (count-rule §1b), which
  the store cannot avoid at zero load.
- **Not established:**
  - the median's count-freeness under jitter on raw samples (§1.4; M1 tests it);
  - what moves c1s's median (§4.2).

---

## 4. Q4: the proposed sampler

**Per leg, per window** (§2.2), from `record_rtt_sample`'s stream:

| quantity | statistic | count / window | provenance | bind gauge |
|---|---|---|---|---|
| window | closes at max(250 ms, M draws) | M = 10 | ε = 0.2 declared, p = ½, order statistics cited (kmr §2.5); the draw model declared | the share of windows extended past 250 ms |
| wₖ | the window's sample count | – | as R6.2 | – |
| T̂ₖ | median of the window's samples (nearest rank on `WinSampler`'s stride buffer) | ≥ M draws | as above | `nkq=` against `nk=` (exists) |
| F̂ | median of T̂ₖ over the K lowest-store windows, ties to the most recent | K = 2·(transient) + 1, 8 here | R6.7 candidate C's set; K from §1.6 | – (no clamp) |
| S | the floor spread on F̂'s windows | as F̂ | decision 8's S; **which spread is O1** | – |

**The law** (scoped to N = 1; δ only through g and q; no ρ input):

```text
   R_k  =  wₖ·( (1 + q(δ))·F̂ + g(δ)·S ) / wₖ·T̂ₖ          fixed point:  T̂(n*) = (1 + q(δ))·F̂ + g(δ)·S
```

- **Decision 8's terms.**
  - q(δ)·F̂ maps onto the median floor. Its sentence "a fraction of the zero-load loop" is unchanged, and its value
    rises by q·(F̂₅₀ − F̂_min): +0.6 ms at c3 J = 5 Bulk, +0.5 at c2.
  - The factor (1 + q(δ)) is the multiplier decision 10 gives Copa's queue target. That is an observation, not a
    derivation.
  - g(δ)·S is §2.1's question.
- **T̂min and Ŝ leave the law.** With the median as the location, Ŝ cancels.
- **Gates on existing rows** (§1.5, with the placeholder S):
  - K5 passes at all five cells. c1s's control is direction only.
  - **Uniqueness:** the window median rises with W over every arm read from W 24 at J = 5, from W 80 at J = 0, and at
    c2 and c1s.
- **Cost:** the median comes from the buffer `WinSampler::close` already sorts under `RWM_DIAG`, with its cost in
  `bench_close_23k`. Moving it into the engine is a law change. Under the scope rules it is a per-tick decision input,
  so it moves class A (no hint, no δ / ρ / count branch).

### 4.1 M1: registration sketch (the coordinator registers it)

**Rows.** Bulk, prebuilt gauge binary `e410d640`, static pools, one replicate each, smoke first. Each dump row has a
G twin in the same session, so the dump's perturbation is read as D12's floor pair did.

| row | cell | arm | what it reads |
|---|---|---|---|
| D1 / G1 | c3-25 (J = 5, single leg) | D12 / G12 | the median and the IQR under jitter, against the count; IQR_F at c3 |
| D2 / G2 | c3-25 | D108 / G108 | the knee: the send gap before each window-minimum sample; the knee-count median |
| D3 / G3 | c2-100 | D16 / G16 | IQR_F at c2 |
| D4 / G4 | c3j0-25 (J = 0) | D112 / G112 | the J = 0 knee, where floor and knee came apart: at W 80 the native minimum fell 2.26 ms below the floor arm's while the median stayed flat. W₉₅ ≈ 116 on the u chord from 80 to 128, lower given u's concavity (61b) |

**The harness.**
- `harness62`'s dump arm exists: `D<W>` in `plan62.py` `armspec` sets `RWM_RTT_DUMP=1`, and `loopdiag_battery.sh`
  preflights the binary for it.
- But it covers one cell only. `plan62.CELLS = (C3J0,)`, and the battery's P62 block writes `UNKNOWN-CELL` for any
  other cell.
- **Needed, tools only:**
  - a plan module copying plan62's armspec, dump flag and replacement rule for the three cells. c3-25 single-leg and
    c2-100 are already defined in plan61 / plan60, with their LATE_T;
  - the battery's cell guard widened for that plan;
  - stub-run tests.
- **The dump cap.** `RWM_RTT_DUMP_MAX` is 400 000 samples per path. D12 wrote 21 248 in ≈ 93 s. At ≈ 1 920 samples / s
  (c3 W 108) and ≈ 1 130 / s (c2 W 16), a row under ≈ 200 s fits. Check the rows' lengths at registration.

**Reading rules, registered before any row.**
- **Count-freeness:** per dump row, the pooled raw-sample minimum, median and IQR over native, 4 and 36 windows
  (crdump §4's construction).
  - Prediction for D1: median drift ≤ 0.3 ms, minimum ≥ 7 ms, IQR ≤ 10 %.
  - D4 is reported only.
- **IQR_F,c** = the median over LATE native windows of (p75 − p25) of the raw samples, nearest rank. Prediction:
  0.50–0.63 × (p90 − p10), i.e. 2.77–3.49 ms (c2) and 4.22–5.33 ms (c3), if the spread is jitter-dominated.
- **g at the Realtime knee:**

  ```text
     g_R,c  =  ( need₅₀,c − q_R·F̂₅₀,c ) / IQR_F,c          q_R = 0.05
  ```

  - need₅₀ and F̂₅₀ are frozen from existing rows: c2 3.61 / 13.70; c3 6.92 / 48.73 (61c), with 61a's 7.17 / 48.47
    reported beside it.
- **The prediction** (verified here from the frozen numbers and the IQR band, with p90 − p10 = 5.54 / 8.44 / 8.47):

  | cell | need₅₀ − q_R·F̂₅₀ | predicted g_R |
  |---|---|---|
  | c2 | 2.93 | [0.84, 1.06] |
  | c3 (61c) | 4.48 | [0.84, 1.06] |
  | c3 (61a, reported) | 4.75 | [0.89, 1.12] |

  - Fable's 0.8–1.1 (c2) holds.
  - At c3 the 61c inputs put the lower edge at 0.84, not 0.9. The 61a inputs give 0.89–1.12.
  - Registered: g_R ∈ [0.84, 1.06] at both cells.
- **What each outcome means:**
  - **Inside at both cells:** one g_R ≈ 1 places Realtime at the knee on both shapes, under (b) with S = IQR. With
    decision 8's g(Bulk) = 1, Bulk then lands (q_B − q_R)·F̂₅₀ above the knee.
  - **Outside:** either the IQR band or need / IQR's shape-freeness failed. IQR_F is reported with g_R, so the two can
    be told apart.
- **Cost:**
  - smoke + 8 rows at s62's pace (its rehearsal ran 7 rows in 20 min, 05:18:54–05:38:48Z) ≈ 25–30 min per launch;
  - with the registered rehearsal, ≈ 1 h typical and ≈ 2 h worst;
  - prebuilt binary (no build), host cargo paused.

### 4.2 What blocks adoption (beyond O1 and M1)

1. **The multi-leg half-gap coupling is deleted with no replacement.**
   - R6.2's ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺)) is built from the two deleted terms.
   - The direct mapping, ½·max(0, maxⱼ F̂ⱼ − F̂ᵢ), is untested, and c8's leg 1 has a count-dependent spread (r68 §1.4).
   - Every row here is single-leg.
2. **Re-anchoring after a path change** (§2.2).
   - A delay drop leaves a permanent standing queue.
   - A rise costs a collapse.
   - No option passes C1 and C3 today.
3. **A closed-loop boot from n₀ is unread:** the transient length behind K, and the high floor during the first K
   windows.
4. **c1s's median floor moves where its minimum does not.**
   - On 2 of 6 W 64 rows (s7 r4 / r6), the LATE median sits 1.9 / 0.9 ms above the row's own boot windows, at lower nk
     (3 308 / 4 122 against ≈ 5 200). T̂min moves 0.1 ms.
   - Hypothesis: receiver-side delay at a receiver-bound cell. It is not read here.
   - A median law would treat that delay as queue.

---

## 5. Q5: ledger rows

**E14 (b̂₀⁺) is committed:** `acb62033`, its own commit, with `law-ledger.md` re-rendered (`render.py --check`: 73 rows,
in sync).
- Verdicts: UNDERIVED and DISHONEST-INPUT, rank 5, ships with F07 / F01 / E13.
- **Not MODE-SWITCH.** The ledger's §1 definition covers only a hint equality, the ρ boolean, or a δ / ρ threshold
  selecting a law. The k ≤ K predicate is a column-8 note, and the proposed lowest-store floor gets the same treatment
  (§2.2).
- Ranks from 5 move down by one. The roadmap's six rank citations follow, including the no-fix range, which is now
  64–73, and the tracks are re-rendered (`tools/roadmap/check.py` OK).

**F07 (the allowance) is held,** as instructed. Proposed text for when it is written:
- start from `knee-margin-rederive.md` §6's uncommitted diff;
- fix_class: "owner question O1 (sampler-design.md §6): which sentence the law holds. Under (a), the minimum law with
  the count-matched floor M₀(n*); under (b), the median law with S = the floor IQR, its g measured. M1 first. Then a
  law change needing a battery. DISHONEST-INPUT leaves with E13's and E14's fixes";
- evidence, appended: "sampler-design.md §1.5: at the native count the floor over-funds the knee by
  b(n_floor) − b(n_knee) = 3.15 (J = 0) and 4.76 / 4.74 ms (J = 5), and by 1.6 at c2 (§1.1)";
- clamp: "min(m̂, ℓ̂ + b̂₀⁺) binds on 0 windows on 64 of 71 c3 rows below the controls, and on 154–169 / ≈ 208 at
  61a's W 2 048 control (§1.2)". Add `DEGENERATE` by MEASUREMENT DISCIPLINE 17–18.

**E13** is unchanged.

---

## 6. Q6: the owner question

**O1. Which sentence does the store law hold at the knee?** This merges the earlier O1 and O2. It is a law change
(`knee-margin-rederive.md` §2.4).

| option | law | what it states | cost |
|---|---|---|---|
| **(a)** | minimum, floor M₀(n*) (ladder K4) | the queue never empties within a window (R3.1) | needs ≥ 3·n*/n₀ zero-load windows (≈ 24 here); undefined at start-up past that pool; no limit at J = 5 |
| **(b)** | median, S = the floor IQR | the median sits q·F̂ + g·IQR above the zero-load median | the bottleneck's busy-ness is not stated, and is carried by a measured g; changes decision 8's S_F definition; ε = 0.2 declared |
| (c) | median, S = S_F as decided | (b)'s sentence | carries (a)'s brake inside S (§2.1); not recommended |

- **My recommendation is (b), conditional on M1.** M1 must confirm the median is count-free under jitter, and g_R must
  land inside its registered band at both cells.
- If either fails, fall back to (a).
- The §4.2 items block either choice.

**O2. M1** (§4.1): register and run it. It needs a tools-only harness change.

---

## 7. Design ledger

- **Constraints:** C1–C7 (§0).
- **Forced:**
  - the floor at the operating count, or a count-free statistic (C4);
  - no at-load brake (C3, K2);
  - no index-predicate boot set (C1 / C5);
  - under the median, the busy condition needs S₀(n*), so the choice of S is the choice of sentence (§2.1).
- **Designer choices:**
  - the median as the count-free location (p = ½ is the most extreme rank fraction central at c3's 17–21 draws; kmr
    §2.5);
  - the lowest-store floor set over the first K (§2.2);
  - K = 2·(transient windows) + 1, read as 8 on these rows (§1.6);
  - the window closes at max(250 ms, M draws), over a fixed 250 ms (§2.2);
  - ties broken to the most recent window;
  - M1's four row pairs.
- **Deletions under O1 (b):**
  - b̂₀⁺ and `K_GAUGE`;
  - ℓ̂ (E13);
  - the min(m̂, ·) branch (degenerate);
  - T̂min and Ŝ as law inputs;
  - m̂ as a law input (it stays Copa's).
  - Each problem these solved:
    - the anchor (now F̂);
    - the zero-load gap (0 by construction, and moved into S's choice);
    - the ℓ̂ cap on m̂'s load rise (no longer read);
    - m̂'s 10 s roll, the only re-anchoring R6.2 had (**open**, §4.2 item 2);
    - the multi-leg half-gap coupling (**open**, §4.2 item 1).
- **Counts:**
  - law inputs per leg: 7 (w, T̂, T̂min, m̂, ℓ̂, b̂₀⁺, σ) → 3 (w, T̂, F̂) plus decision 8's S;
  - exception clauses: the k ≤ K predicate and the min branch → 0;
  - clamps: one added (the window's max), with its gauge.
