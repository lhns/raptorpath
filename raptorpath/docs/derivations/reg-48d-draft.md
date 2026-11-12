# DRAFT: §48d. The fixed `bbr_rs` (one-round ProbeRtt [+ the c8 CPU fix]) against main: guard battery (pre-registration draft, not in status.md)

**Status.** This is a draft for the coordinator to move into status.md once:
- the CPU placeholder is resolved;
- the trees are pinned;
- an advisor review is done.

Nothing here has touched the VM. The derivation behind it is `derivations/w1-round2-probertt.md`.

**Question.** With ProbeRtt's goodput cost removed δ-free (and the c8 CPU cost, if the ns bench finds its cause), does `bbr_rs` leave §48b's `REGRESSION`, by §48's ladder unchanged?

## 1. Arms and trees

- **BASE:** main at launch, pinned by amendment with its sha. Quinn's BBR is the default; the binary has no `qm=` and no `[QCC]`.
- **BBRRS-fixed (CAND):** BASE plus the §48 commits (as in §48b's `bbr48cand`), plus:
  - (i) the one-round ProbeRtt of the derivation (section 2), with its tests T1–T7 and the cumulative `[QCC]` counters `qrn`, `qrus`, `qrb` and `qab`, plus the `qage_us` gauge;
  - (ii) **[PLACEHOLDER: the c8 CPU fix, if the pending host/VM ns bench of `bbr_rs`'s per-packet path against quinn's BBR finds one. Fill in: the bench result, the commit, the predicted CPU/GB delta at c8. If the bench finds none, write "none". CAND is then (i) only, and the prediction branch "no CPU fix" in section 6 applies.]**
  - It runs with `RWM_QUIC_CC=bbr_rs`. The default is not flipped.
- **Pre-launch diff check:** `git diff --stat BASE CAND -- raptorpath/src` lists only §48b's file list, plus (ii)'s files if any.
- **Host gate before VM contact:**
  - T1, T2, T4 and T5 are red on `e5d7ffbd` and green on CAND, with the red and green outputs saved under `docs/l1-raw/bbr48d/`;
  - T3, T6 and T7 are green on both;
  - T6's f = 1.25 mutation run is red, with its output saved;
  - all of §48's host tests stay green;
  - the full suites pass as §48b item 5.
  - `GATE-48d PASS` iff all of the above hold. Otherwise stop: no VM.

## 2. Design (§48b's, unchanged)

- **Cells and sizes:** c1s and c1d at 5 GB; c2 and c8 at 600 MB. Default store law, Bulk, `--window-reliable`, the §41b env.
- **Rows:** n = 12 BASE + 12 CAND per cell (seeds 42 and 7 × 6 reps per arm), 96 rows.
- **Ordering and session:** rep-major interleave as in §41c, a CAND then BASE check row at c8 first (`ABORT-SMOKE` rule), and a rehearsal first.
- **Rules carried over:** §41c's replacement rule (≤ 4 witness-voided slots, re-run once) and NO VERDICT for an incomplete design.
- **Duration:** ≈ 105 min plus builds and suites, inside launch + 4 h.
- **Harness:** plan 48 (`LD_PLAN=48`), `bbr48_score.py`, plus the readouts in section 4.

## 3. Guard and ladder (§48 / §48b, UNCHANGED)

- **R1:** the §41c guard. Rank-minp, 26 clauses, N_PERM 10 000, seed 4141, one-sided, PASS iff p > 0.05, with the DNF rule.
- **p_rest:** the same statistic over the 24 clauses without the goodput clauses of c2 and c8.
- **R2b:** the share of lines at t ≥ 1 s with `qcwnd` < 0.5·BDP_leg and B ≥ 98.
- **Tail clause:** CAND's lowest row ≥ 0.8 × CAND's cell median.
- **Ladder, in order:**
  1. `NOT-CONTROLLED`: median R2b > 0.05 at any cell, or any CAND row with R2b > 0.25.
  2. `PROMOTE-ELIGIBLE`: R1 PASS and the tail clause holds.
  3. `CONTROLLED-WITH-COST`: R1 fails, p_rest > 0.05, CAND goodput median ≥ 0.90 × BASE at c2 and c8, `wrtt` p50 ≤ 0.7 × BASE at c2 and on c8's slow leg, and the tail clause holds.
  4. `REGRESSION` otherwise.
- **Reported beside it:** the §41c resolution table from this session's BASE rows.

## 4. Readouts

### Registered, reported, not scored

**A. Delivered delay A_x** (app-echo `rtt`), per leg: the mean over rows of the row median, for both arms, with A_x CAND/BASE per cell.
- It is printed beside `wrtt`, so that `CONTROLLED-WITH-COST`'s claim about what it buys reads in delivered terms (§48 Amendment 2's cost label).

**B. ProbeRtt goodput, line method** (the §48b supplement's `supp48.py` form, now registered):
- per cell, CAND only, lines at t ≥ 1 s;
- a line is R if the nearest `[QCC]` shows any leg qm = R;
- C_R,lines = share_R × (1 − ḡ_R/ḡ_nonR).
- Its bias is stated: a 250 ms tick dilutes a short episode, so after the fix this readout can **under-read toward 0**. Readout C is the primary one.

**C. ProbeRtt goodput, counter method** (new). Per CAND row and leg, from the last `[QCC]` line's cumulative counters:
- `qab`: bytes acked;
- `qrb`: bytes acked in R;
- `qrus`: µs in R;
- `qrn`: R entries;
- T: the leg's span from its first to its last `[QCC]` line.

```text
   r_nonR   = (qab − qrb) / (T − qrus)
   C_R,leg  = (r_nonR · qrus − qrb) / qab               (the bytes the leg would have delivered at its own non-R rate)
   C_R,row  = Σ_leg C_R,leg · qab_leg / Σ_leg qab_leg
```

- Also: the time share in R per 10 s (qrus / T), and the mean R duration (qrus / qrn) in units of that leg's median `qsr_us`.
- **Witness, keyed on the filter's age, not on the entry count** (a leg whose filter refreshes naturally rightly has fewer entries): on every active leg, every `[QCC]` line shows `qage_us` ≤ 10 s + 3 × the leg's `qsr_us` + one tick (250 ms).
  - This proves that an expired filter was refreshed, whether by ProbeRtt or naturally (measurement discipline rule 1).
  - A leg that fails it reads `VOID-R` for this readout only.

**D. Goodput-weighted cost attribution** (replaces §48b W1.1's line count, which the §48b erratum withdrew):
- **Classes:** each active leg of a CAND line takes the first matching §48b W1.1 class, in `attr48.py`'s order:
  1. STORE-LIMITED (`paused` ≥ 50 %, B < 98);
  2. QUINN-LIMITED (B ≥ 98, qm ∈ {B, S, D});
  3. PROBE-RTT (qm = R);
  4. UNDER-READ (qbw < 0.8 L);
  5. OTHER.
- **The line's class** is the highest-precedence class over its active legs, so every line has exactly one class (a partition, which c8's two legs need).
- The reference ḡ_ref is **BASE's mean line `good`** in the same cell (t ≥ 1 s), fixed now.

```text
   C_c   = s_c · (1 − ḡ_c / ḡ_ref)                 (s_c: CAND line share of class c; ḡ_c: its mean `good`)
   Σ_c C_c = 1 − ḡ_CAND / ḡ_ref                    (exact: the line-mean deficit against BASE; a class can be negative)
```

- The identity is on line means. It is printed beside the row-median goodput clause, and the two are not expected to match exactly.
- It is **descriptive, not causal**. BASE also holds B ≥ 98 on most lines (§48b supplement: joint 0.83 at c2), so a class names where the deficit sits, not why.

**E. Everything else in §48b's readouts:** R2a, R3 (B, `wrtt`, A_x), R4 (qm occupancy, `qrt_us`, `qsr_us`, the loop margin, now read as window-limitation per the erratum), `qmin_us`, `dgq full`, `qcns`, and the c8 split (leg shares, coded share, CPU totals, seconds).

## 5. Outcome additions (none change the ladder)

- **`FIX-INERT` (reported):** C_R,row's CAND median > 0.4 % at c2, or > 0.6 % at c8. The ProbeRtt fix did not remove its cost in vivo. This is read before any label discussion.
- **`RATCHET` (reported):** median `qrt_us` at c2 > 10.0 ms, or any c2 row's `qrt_us` p90 > 12 ms. This is the upward failure direction of the derivation's section 0: either f·b̂/L ≥ 1 in vivo (check qbw against L) or the ×0.8-per-period decay has not reached the fixed point. Re-derive f from the measured b̂/L; do not tune it.

## 6. Predictions (before any number)

| cell | C_R,row (counter) | time in R per 10 s | CAND/BASE goodput | `qrt_us` p50 | `wrtt` p50 | A_x (CAND) | R2b |
|---|---|---|---|---|---|---|---|
| c1s | ≤ 0.05 % | ≤ 0.1 % | better or equal (§48b −2.3 %, i.e. CAND better; tail CAND ≥ 0.95 ×) | 2.15–2.4 ms | as §48b | 17.6 ms ± 20 % | ≤ 0.02 |
| c1d | ≤ 0.05 % | ≤ 0.1 % | better or equal (§48b −4.9 %) | 2.15–2.4 ms | as §48b | as §48b ± 20 % | ≤ 0.02 |
| c2 | **0.13–0.23 %** (from 0.84) | ≤ 0.5 % | **0.987–0.995** (deficit ≈ 0.8 %, range 0.5–1.3 %) | **7.8–9.2 ms** (from 7.81) | 21.8–25 ms (BASE ≈ 112) | ≈ 206 ms ± 15 % (unchanged; the queue sits in quinn) | ≤ 0.02 |
| c8 | **0.17–0.32 %, ≤ 0.4 %** (from 0.78) | fast leg ≤ 0.5 %, slow leg ≤ 2.5 % | **0.974–0.985** (deficit ≈ 2.0 %, range 1.5–2.6 %), before any CPU fix | 8.3–9.6 / 39.4–41 ms | fast ≈ 22–25 ms; slow ≈ 88 ms ± 15 % | fast ≈ 122 ms, slow ≈ 251 ms, each ± 15 % | ≤ 0.02 |

**The label, conditional on the CPU placeholder.**
- **R1:**
  - c2 goodput's d80 is 1 %. A 0.8 % shift is caught in roughly half to two thirds of the draws.
  - c8 goodput's d80 is 2.4 %, against a 2.0 % shift.
  - **R1 is predicted to FAIL (p ≤ 0.05) more likely than not.** `PROMOTE-ELIGIBLE` is not predicted from this fix alone: the unattributed ≈ 0.6 % (c2) and ≈ 1.8 % (c8) remain.
- **Branch "no CPU fix":** c8 `cpu_cli/GB` +3.5 % and `cpu_srv/GB` +5.3 % stand, so p_rest ≈ 0.01 as in §48b. **Predicted `REGRESSION` again.** The readouts B–D still say whether ProbeRtt's cost is gone.
- **Branch "CPU fix" (filled in from the placeholder):** if the fix's host bench predicts c8 CPU/GB within ±2 % of BASE, then p_rest > 0.05 is predicted. The other `CONTROLLED-WITH-COST` conditions held in §48b (0.985 / 0.974 ≥ 0.90; `wrtt` 0.19 × / 0.38 ×; tail 0.98 / 0.95), so **`CONTROLLED-WITH-COST` is predicted**, with `PROMOTE-ELIGIBLE` second. That second outcome needs c2's R1 clause to miss a 0.8 % shift.

## 7. After the verdict (§48b item 7, unchanged)

- `PROMOTE-ELIGIBLE` or `CONTROLLED-WITH-COST` goes to the user, who makes the ADR-0073 call on `RWM_QUIC_CC`.
- `REGRESSION` or `NOT-CONTROLLED` goes to the user to choose: archive, §47's option 1 or 3, or the next δ-free fix, the unattributed rest. Readout D says where it sits.
- Nothing merges to main before a passing registration.

## 8. Not this registration: what the δ-on-store step (W1.2 candidate A) will need

The δ step needs its own registration, after `bbr_rs` passes and is promoted or ruled to continue. It needs:
1. **An engine store-law path.** The τ store law (n = G·Σ L̂ᵢ·(RTpropᵢ + Sᵢ + τ_s(δ))) is oracle-only today (§43b). It also needs:
   - an engine L̂: `bbr_rs`'s b̂, published through a per-path atomic, not the `[QCC]` clone (W1.2 §1);
   - an answer to its staleness under an app-limited, raise-only max filter (W1.2 open item 5).
2. **An engine b₀ source.** `qsp_us` fails at c1s. The candidates are `EchoRatioMin` and `bbr_rs`'s ê (W1.2 open item 3).
3. **The user's U2 ruling** (A(q), provenanced but ≈ −8.5 % at c2 Bulk, against A(c) with c_B ≈ 2.7 ms unsourced), plus **G ≤ 0.946** with a provenance.
4. **Re-measured τ windows at c1s under `bbr_rs`.** The current ones come from truncated minima with quinn uncontrolled.
5. **The c8 per-leg split** of the pooled store against per-leg windows (W1.2 open item 7).
6. **Readouts the δ step would score:**
   - `[SUMCAP]` `pin=` and the cap value, plus `btlbw=`/`est=` (H1: does `bbr_rs` remove the shipped law's accidental probe?);
   - B and `paused` per leg;
   - A_x at all three presets;
   - ±2 % δ-nudge continuity across presets in vivo (one cell, three hints).
7. **BASE for that registration:** BBRRS-fixed, if promoted, so the δ effect is not confounded with the substrate change.
