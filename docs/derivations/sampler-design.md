# Sampler design: how the store law should sample delay

Status: DESIGN, not code. Docs and Python only: no engine code, no cargo, no VM. Branch `sampler-design` off
`github/main` `e696fc77`. It answers the "sampler-design derivation" that status "The count-rule derivation" names
(`count-rule` `2d557191`): how to estimate the brake, whether the knee's minimum is set by post-loss drains, and what
anchors the floor under jitter.

**Every reading here is post hoc,** on existing rows: s62's rehearsal (J = 0; one row per arm except the floor pair,
the D12 dump row and two W 512 controls), 61c and 61a (c3, J = 5, six replicates per arm), and s60's c2 W 16 floor
(six replicates). Nothing here is registered, and every number is scoped to those rows.

**In plain words.**
- **The brake should not be estimated; it should be designed out.** The brake exists only because the law compares a
  window minimum against an anchor taken at another sample count. The minimum is never count-free, so any brake must
  be read at the operating window's own count, from zero-load samples, and that takes several knee-sized windows of
  zero-load history the start-up does not have.
  - Read at load, a brake is circular. It funds its own queue, and R never crosses 1 (gate table, §1.5, K2).
  - Read at boot, as b̂₀⁺ is, it is the wrong count and the wrong moment. At the knee it over-funds by 3.2 ms (J = 0),
    4.7 ms (J = 5) and 1.6 ms (c2), on top of the 103 ms latch of one row.
  - **Proposed: hold the window median, not the window minimum.** With m independent draws per window, the median is
    count-free to a declared ε once m ≥ 1/(ε·½) = 10 at ε = 0.2. The brake is then 0 by construction, and b̂₀⁺, ℓ̂ and
    the min(m̂, ·) branch leave the law. That is a law change, so it is the owner's call (§6, O1).
- **Post-loss drains set the knee's minimum too.** At W 80–124 on both c3 sessions, the ticks without a retransmit read a
  higher window minimum than those with one, on 6 of 6 rows at every arm. The gap is +1.2 to +4.2 ms at
  W 100–112 and grows above it. The window median is insensitive to retransmits below the knee (|Δ| ≤ 0.15 ms) and
  responds at and above it (+0.6 to +2.8 ms). The send-pause mechanism is observed only on one floor dump row.
- **The floor under jitter: no minimum anchors it.**
  - At J = 5, m̂ rises with load, by +3.2 to +6.7 ms from the floor arm to the knee arms. That is more than the native
    minimum rises.
  - Across replicates, the floor arm's window median is the tightest location: it spans 0.10 / 0.39 / 0.54 / 0.11 ms
    (J = 0 / 61c / 61a / c2), against 1.4–96 ms for R6.2's F.
  - Count-freeness on raw samples is shown only at J = 0 (the D12 dump).
- **One cheap measurement tests the proposal:** three `RWM_RTT_DUMP` rows (c3 J = 5 floor and knee, c2 floor), with
  predictions in §4.

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

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place) | output | what it computes |
|---|---|---|---|
| `store-law/sdalign.py` | `l1-raw/s62/rep62r0-diag.tgz`, `l1-raw/s61/rep61c-diag-ld.tgz`, `rep61a-diag-ld.tgz`, `l1-raw/s60/rep60a-diag-ld.tgz` | `sdalign.out` | A: retransmits against the window minimum per arm, with the dose and the anchor bind. B: the brake estimators per floor row. C: the floor's location statistics and their rise to each arm. D: the gate table for five floor candidates |

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
| C5 | Defined from the first window: no hold, no latch, no K-window boot predicate | R6.7 erratum to R5.5 (a hold is a term with no provenance); the b̂₀⁺ defect (count-rule §2) |
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
  - within ±0.15 ms at every arm below the knee (all J = 5 arms ≤ W 104, J = 0 W 12 / W 80, c2), except one small
    class (3–5, 11–23 windows, −0.46 to −0.82);
  - at and above the knee it responds: no-retransmit ticks read +0.63 (61c W 108), +1.29 (61a W 112) and +2.2 to +2.8
    (W 120–152) ms.
  - So above the knee a repair drains the queue for a large part of the window, not only for one sample.
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

- **At J = 5, m̂ is not a floor.** It rises with load by more than the native minimum below the knee. It is a 10 s
  minimum of a queued, jittered stream, so it tracks the queue's low excursions.
- **At J = 0, m̂ stays on the queue-free low mode through W 128,** on one row per arm. Drains still empty the queue at
  least once per 10 s there.
- **At J = 0 the native minimum falls under load** (−2.26 ms at W 80), because the W 80 window holds 395 samples and
  carries a retransmit in 90 % of ticks. At a matched count the W 80 rise is +0.84 (`crrule.out` §2). A native-count
  law therefore reads a negative rise below the J = 0 knee.
- **The window median is flat below the J = 0 knee** (+0.03 at W 80) and rises monotonically over every J = 5 arm.

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

### 1.5 The gate table (`sdalign.out` D; one leg, c3 arms, medians over rows)

**Setup.**
- margin(W) = F + A − statistic(W), with A from decision 8 at Bulk (g = 1, q = 0.10).
- A uses the cell's native floor numbers: S_F = the floor arm's median window (p50 − T̂min), F̂ = its median T̂min.
- For K5 that S_F is a placeholder, because decision 8's spread mapped to the median is not logged. So K5's crossing W
  is not read; only its signs and its count of sign changes are.
- **The gates:** margin > 0 at the floor arm, < 0 at the control, and exactly one sign change.

| | floor candidate F | statistic | J = 0 | 61c | 61a |
|---|---|---|---|---|---|
| K1 | R6.2 as gauged on that arm: min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺ | T̂min | FAIL (+178 at W 512) | FAIL (+189 at W 512) | PASS |
| K2 | a running brake: m̂ + median over the arm's windows of (T̂min − m̂) | T̂min | FAIL (+5.9 to +6.7 everywhere) | FAIL (+7.3 to +10.2) | FAIL (+9.4 to +10.2) |
| K3 | candidate C at the floor's count: the floor's median T̂min, fixed | T̂min | PASS, crosses W 80–128 | PASS, crosses W 120–152 | PASS, crosses W 124–2 048 |
| K4 | count-matched: M₀(n_W), the median minimum of round(n_W / n_floor) pooled floor windows | T̂min | PASS, crosses W 80–128 | PASS, crosses W 108–120 | PASS, crosses W 112–124 |
| K5 | the floor's median window p50, fixed | window p50 | PASS, crosses W 80–128 | PASS, crosses W 108–120 | PASS, crosses W 112–124 |

- **K1** fails on static pools because b̂₀⁺ latches each large pool's own start queue (R6.7 §1.2, already known). In a
  closed loop booting from n₀ this replay does not apply. It is listed for completeness, not as a new finding.
- **K2 fails by construction.** Any brake read at load makes F ≈ T̂min, so R − 1 ≈ A/T̂ > 0 everywhere. This rules out
  every running or at-load brake estimator (C3).
- **K3 against K4 is the hidden term.** At the knee arms, K3's margin exceeds K4's by 3.15 ms (J = 0 W 128), 4.76
  (61c W 108) and 4.74 (61a W 104 / 112).
  - That is b(n_floor) − b(n_knee): an allowance with no provenance, which depends on the jitter, the loss process and
    the rate ratio.
  - It is why the native need (3.71 / 4.10 ms at c3, kmr §2.3) is smaller than the one-count need (8.87 / 7.99).

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
| boot-time | b̂₀⁺ (the max of 8); the median of 8 | C4: the boot count, not n*; C5: the K-window predicate; the max latches one transient, the median is biased to 0 by the m̂-setting windows | §1.1 |
| zero-load ladder | K4: the median of B group minima, each over ⌈n/n̄₀⌉ pooled floor windows | **its input count** (below) | §1.5 K4 passes the gates |

**The ladder's input count.**
- B ≥ 3 groups are needed for a median, so it needs B·n* zero-load samples: ≥ 3·n*/n̄₀ floor windows.
- On these rows n*/n̄₀ ≈ 8 (c3 480 / 60, c2 2 250 / 282), so that is ≥ 24 zero-load windows (6 s).
- **At start-up the store ramps from n₀,** at R₀ ≈ 1.19 per window (`store-law-r67.md` §4.3). It passes the knee's count
  long before it has 24 zero-load windows.
  - The law would then need M₀ at a count it has not observed.
  - That takes an extrapolation, which has no provenance, or a hold, which C1 / C5 forbid.
- **At J = 5, M₀(n) has no limit.** It falls 0.3–1.6 ms per doubling up to 4 200 samples (`crrule.out` §1, 61c / 61a). So any
  residual count mismatch is a bias of that size.
- Pooling non-contiguous windows is allowed by `knee-margin-rederive.md` §2.1: at a fixed count, interleaved and
  shuffled pooling read the same as contiguous pooling, on c2 / c3 rows.

**Proposed (designer choice; the owner decides, O1): a count-free location, so the brake is identically 0.**

```text
   T̂ᵢ,ₖ  =  median of the window's echo samples                       (already the R denominator)
   F̂ᵢ    =  median{ T̂ᵢ,ₖ : the K windows of leg i with the lowest store size so far; ties → the most recent }
   brake  ≡  0                                                         (both sides are the same count-free statistic)
```

- **Input count:** a window's median is count-free to ε iff it holds m ≥ 1/(ε·½) independent draws. At the declared
  ε = 0.2 that is 10. F̂ needs K such windows, with K in R6.7 §3's band of 3–11 (the pick is open there).
- **Start-up:** F̂ is defined from the first window: the median of one window, then of the K lowest-store windows as they
  arrive. There is no hold, no latch and no K-window predicate.
  - A window with fewer than 10 draws contributes a median with a larger level error. That error grows smoothly as m
    falls, so no rule switches on it.
  - At n₀ = 10 on c3 a window holds an estimated 14–17 draws. That is scaled from W 12's 17–21 by 10/12, and it is a
    hypothesis.
- **What this deletes:** b̂₀⁺, `K_GAUGE`, ℓ̂ (already E13), the min(m̂, ·) branch (degenerate, §1.2), and m̂ and T̂min
  as law inputs. They stay as gauges, and m̂ stays as Copa's input.
- **If the owner keeps the minimum,** the ladder K4 is the only family that passes C3 and C4. It carries the input count
  and start-up gap above, and it must interpolate in log n between group sizes (C1: no step in n). Its clamp at the
  largest available group needs a bind-fraction gauge (C2).

---

## 3. Q2 and Q3

**Q2: is the knee's window minimum also set by post-loss drains? On these rows, yes.**
- **Established** (§1.2):
  - on the informative arms (W 80–112, base 0.72–0.90), ticks without a retransmit read a higher minimum, on 6 of 6
    rows of every arm in both c3 sessions;
  - the gap grows with load: +0.6 at W 80 to +3.1 / +4.2 at W 108 / 112;
  - the count works against the association.
- **Hypothesis:** the mechanism is count-rule's, a repair stalls the release and the following flight meets a drained
  queue.
  - The send-pause alignment is observed on the D12 floor dump only.
  - At the knee no dump exists, and J = 0 has no knee bracket (W 80 and W 128 are one row each).
- **For the law:** the window minimum is a statistic of the loss process at every load, the knee included. The window
  median is not, below the knee, and responds at and above it, where repairs drain real queue.

**Q3: what anchors the floor under jitter? Not a minimum.**
- **Established** (§1.3):
  - m̂ rises +3.2 to +6.7 ms under load at J = 5;
  - ℓ̂ is DISHONEST-INPUT (E13);
  - R6.2's F spreads 9.0 / 95.6 ms across replicates.
  - The floor window's median spreads 0.39 / 0.54 ms at J = 5, the tightest location in every cell; at J = 0 m̂ ties
    it (0.07 against 0.10).
- **Proposed:** the anchor is F̂ of §2, the zero-load window median. Its sentence is "the loop the store sees with no
  standing queue". At J = 0 that includes the bulk's ≈ 5.4 ms offset above the queue-free mode (count-rule §1b), which
  the store cannot avoid at zero load.
- **Not established:** the median's count-freeness under jitter on raw samples (§1.4). §4's M1 tests it.

---

## 4. Q4: the proposed sampler

**Per leg, per 250 ms window**, from `record_rtt_sample`'s stream:

| quantity | statistic | count / window | provenance | bind gauge |
|---|---|---|---|---|
| wₖ | the window's sample count | 250 ms | declared: the gauge's existing cadence. Count-free statistics make the fixed point first-order insensitive to it | – |
| T̂ₖ | median of the window's samples (nearest rank on `WinSampler`'s stride buffer) | m ≥ 10 independent draws (ε = 0.2) | order statistics, cited (kmr §2.5); ε declared | `nkq=` against `nk=` (exists): the share of windows whose buffer was decimated |
| F̂ | median of T̂ₖ over the K lowest-store windows, ties to the most recent | K in 3–11 | R6.7 candidate C's set (r67 §3) | – (no clamp) |
| S | the floor spread, on F̂'s windows | as F̂ | **decision 8's S_F** (owner, O2) | – |

**The law, with decision 8's terms mapped** (one leg; δ only through g and q; no ρ input):

```text
   R_k  =  Σᵢ wᵢ,ₖ·( (1 + q(δ))·F̂ᵢ + g(δ)·Sᵢ ) / Σᵢ wᵢ,ₖ·T̂ᵢ,ₖ          fixed point:  T̂(n*) = (1 + q(δ))·F̂ + g(δ)·S
```

- **Every term is read count-free.**
  - T̂min and Ŝ = T̂ − T̂min leave the law: with the median as the location, Ŝ cancels.
  - **q(δ)·F̂ maps to q(δ)·F̂ on the median floor.** Its sentence "a fraction of the zero-load loop" is unchanged. Its
    value rises by q·(F̂₅₀ − F̂_min): +0.6 ms at c3 J = 5 Bulk, +0.5 at c2.
  - The factor (1 + q(δ)) is the same multiplier decision 10 gives Copa's queue target. That is an observation, not a
    derivation.
  - **g(δ)·S_F:** decision 8 defines S_F as the floor windows' median − minimum. On a median law that term still holds
    the minimum, so it carries the count dependence the law has otherwise dropped: native 5.86 against 9.71 at c3's
    registered counts, and c8 leg 1 rising with nk (r68 §1.4).
    - The count-free alternative is the floor windows' IQR (`knee-margin-rederive.md` §2.6, untested; `[WSTAT]` does
      not log it).
    - Changing it changes decision 8's definition, so it is the owner's call (O2).
- **Gates on existing rows:** K5 passes every gate on all three c3 cells (§1.5), with the placeholder S.
  - **Uniqueness:** the window median rises monotonically over every J = 5 arm, and from W 80 to W 128 at J = 0. It is
    flat below the J = 0 knee (+0.03 ms), where R > 1, so there is no false crossing.
  - **The knee need in median terms** already exists on these rows: 3.61 ms at c2, 6.92 / 7.17 at c3 (kmr §2.3, Fable's
    price table). Against the registered S_F that is 0.711 / 0.713 / 0.750. Against native p90 − p10 it is 0.65 / 0.82
    / 0.85. Against the IQR it is unknown.
- **Cost:** the median comes from the buffer `WinSampler::close` already sorts under `RWM_DIAG`, with its cost in
  `bench_close_23k`. Moving it into the engine is a law change. Under the scope rules it is a per-tick decision input,
  so it moves class A (no hint, no δ / ρ / count branch).

**Per claim: what the rows show, and the one cheap measurement.**

**M1** is three `RWM_RTT_DUMP` rows on gauge binary `e410d640`, Bulk, static pools: c3-25 W 12, c3-25 W 108 and
c2-100 W 16. It needs its own registration (O3). A dump-on pass reports its own perturbation, as D12 did.

| claim | existing rows | M1 reading | prediction |
|---|---|---|---|
| the window median is count-free under jitter | J = 0 only (D12, +0.045 ms); J = 5 untested | the c3 W 12 dump: pooled-sample median, native → 36 windows | drift ≤ 0.3 ms, against the minimum's ≥ 7 ms on the same row (61c's G-ladder: 9.5 native → 4 200) |
| the floor IQR is count-free, and its size | not logged | the c3 W 12 and c2 W 16 dumps | drift ≤ 10 % of the IQR. If the spread is jitter-dominated, IQR ≈ 0.50–0.63 × (p90 − p10), i.e. 4.2–5.3 ms at c3 (sum-of-uniform to normal shapes) |
| knee need / IQR is shape-free | – | IQR at c2 and c3 against the existing median needs 3.61 / 6.92 | **none** (as s62 did for need₀) |
| drains set the knee's minimum | dose at W 80–112 (§1.2); send pauses on D12 only | the c3 W 108 dump: send gap before each window-minimum sample | in retransmit ticks, the minimum sits in the first flight after a send gap ≥ 1 loop in ≥ 50 % of cases (D12 floor: 0.92 for gaps ≥ 60 ms) |
| the median's start-up at n₀ | – | not M1. It needs closed-loop rows, and comes with the engine form's battery | – |

---

## 5. Q5: ledger rows owed (proposed text; `law-ledger.yaml` is not edited)

**F07** (the allowance). Apply `knee-margin-rederive.md` §6's diff first, which is uncommitted and still applies to main.
Then make these changes:

```diff
-  fix_class: "the law's statistic is the native window minimum (R6.2), whose count dependence is the law's own (R6.7 open item 4); a quantile alternative (window median + IQR) is a candidate, untested (knee-margin-rederive.md §2.6); s62 (sketch, §5) registers the knee rise against the spread along jitter; then the allowance's form, then a law change needing a battery; DISHONEST-INPUT leaves with E13's fix (q*F-hat in place of q*l-hat)"
+  fix_class: "owner call O1 (sampler-design.md §6): hold the window median (brake = 0 by construction; F-hat = median window median of the K lowest-store windows) or keep the minimum with a count-matched zero-load ladder M0(n) (needs >= 3 n*/n0 zero-load windows, undefined at start-up past the pool); then O2 (S_F as decided, median - min, or the floor IQR); M1 (three dump rows) tests count-freeness under jitter; then a law change needing a battery; DISHONEST-INPUT leaves with E13's and E14's fixes"
+  evidence: append to kmr §6's evidence string: "; sampler-design.md §1.5: on c3 rows the floor at the native count over-funds the knee by b(n_floor) - b(n_knee) = 3.15 (J = 0), 4.76 / 4.74 ms (J = 5), 1.6 at c2 (§1.1): an allowance term with no provenance inside every native-minimum need"
```

**E14 (new): b̂₀⁺, the boot brake.** Branch-only, with C03 as the precedent. Tags are from `render.py`'s `TAGS`.

```yaml
- id: E14
  name: boot brake b-hat0+ (WinSampler::close; gauge branch github/slaw-gauge, not on main)
  area: estimator
  priority: P0
  reach: candidate (gauge branch); read by R6.2's F = min(m, l + b0p) + b0p and printed as b0p= / b0k= on [SLAW]
  loc: "github/slaw-gauge d416d59b: net/slaw.rs WinSampler::close :150-165 (brake_k = tmin_k - m_k pushed for k <= K_GAUGE = 8; b0p = max once k >= 8)"
  formula: "b0p_i = max{ tmin_i,k - m_i,k : k = 1..8 }"
  paper: "store-law.md R6.2 input table ('the sampler's first K windows'); K = 8 declared without provenance (§13.1, R6.7)"
  derivation: "sentence: the zero-load gap between the window minimum the law steers and the long-run minimum it anchors on. That gap is a function of the window's sample count, b(n); the estimator reads it at the boot count, as one extreme of 8 start-transient windows, never at the operating count"
  regime: "count-rule.md §2, sampler-design.md §1.1: rows 4.03-5.40 (J = 0), 3.26-12.62 (61c), 6.35-103.40 (61a), 2.04-4.10 (c2) against a steady native brake of 3.2-3.6 / 7.5-8.6 / 5.6-8.4 / 2.2-2.7 and a knee-count brake of 0.1-0.3 / 2.7-4.7 / 1.0-4.3 / 0.5-1.1; 61a G12 s42 r5's 103.4 ms brake (k = 2) is the B1 outlier (LATE median err -101.25)"
  clamp: "the k <= K window predicate; not a clamp on a value"
  inputs: "DISHONEST: a maximum over start-transient differences against an m-hat seconds old, at the boot count; neither the operating count nor zero load"
  modeswitch: "a predicate on k (k <= K_GAUGE); R6.7's mode screen flags it"
  verdicts: [UNDERIVED, DISHONEST-INPUT, MODE-SWITCH]
  measured: "yes"
  gauge: "yes"
  fix_class: "delete from the law with a count-free location (sampler-design.md §2, owner O1); under a minimum, replace by the zero-load ladder M0(n) (K4)"
  depends_on: []
  ships_with: [F07, F01, E13]
  evidence: "count-rule.md §2 (F spread 37.2-138.5 ms through b0p); sampler-design.md §1.1 and §1.5 (K1/K2)"
```

**R6.2's anchor branch** (inside F07's row, `clamp:`):
- Proposed text: "min(m̂, ℓ̂ + b̂₀⁺) binds (ancf > 0) on 0 windows on 64 of 71 c3 rows below the controls. It binds on
  154–169 / ≈ 208 at 61a's W 2 048 control (sampler-design.md §1.2): DEGENERATE over the operating range."
- Add `DEGENERATE` to F07's verdicts, a defect finding by MEASUREMENT DISCIPLINE 17–18.

**E13** is unchanged. Its fix already removes ℓ̂ from the law, and §2 removes the min branch that still read it.

**Rank** for E14: next after E13. Re-number by the r68 §5 procedure and re-check with `render.py`.

---

## 6. Q6: owner questions

- **O1. The law's location statistic** (a law change, `knee-margin-rederive.md` §2.4).
  - **(a) The window median (recommended).** Count-free at the declared ε = 0.2, so the brake is 0. It deletes b̂₀⁺,
    ℓ̂, the min branch and T̂min from the law.
  - **(b) The window minimum, with the count-matched ladder M₀(n).** It needs ≥ 3·n*/n₀ zero-load windows (≈ 24 on
    these rows) and is undefined at start-up past that pool.
  - ε = 0.2 rides with (a) as a declared tolerance.
- **O2. Decision 8's S_F under (a).**
  - **Keep median − minimum, as decided.** The spread term then keeps the minimum's count dependence.
  - **Or take the floor IQR,** which is count-free. Its value is unmeasured until M1.
  - g(Bulk) = 1 and the form are unchanged either way.
- **O3. M1:** three dump rows on `e410d640`. It needs its own registration and a short session, about one rehearsal's
  length. It tests §1.4 and §4 before any engine form.

---

## 7. Design ledger

- **Constraints:** C1–C7 (§0).
- **Forced:**
  - the floor at the operating count, or a count-free statistic (C4);
  - no at-load brake (C3, K2);
  - no boot predicate (C1 / C5).
- **Designer choices:**
  - the median as the count-free location (p = ½ is the most extreme rank fraction central at c3's 17–21 draws; kmr
    §2.5);
  - ties broken to the most recent window;
  - M1's three rows.
- **Deletions under O1 (a):**
  - b̂₀⁺ and `K_GAUGE`;
  - ℓ̂ (E13);
  - the min(m̂, ·) branch (degenerate);
  - T̂min and Ŝ as law inputs;
  - m̂ as a law input (it stays Copa's).
  - Each problem these solved: the anchor (now F̂), the zero-load gap (now 0 by construction), and the ℓ̂ cap on m̂'s
    load rise (no longer read).
- **Counts:**
  - law inputs per leg: 7 (w, T̂, T̂min, m̂, ℓ̂, b̂₀⁺, σ) → 3 (w, T̂, F̂) plus decision 8's S;
  - exception clauses: the k ≤ K predicate and the min branch → 0.
