# s62: the c3 knee need with the jitter removed (J = 0), pre-registration

Status: DRAFT REGISTRATION, round 3 (derivation style). Docs and Python only: no engine code, no cargo, no VM contact.
Branch `s62-design` off `github/main` `3d871561`. It is written to be copied into `docs/status.md` as §62.

It builds on:
- owner decision 13 (2026-11-17);
- the sketch in `knee-margin-rederive` `c6c55958` §5;
- the closed post-hoc analysis (status: "The knee-margin re-derivation, closed post hoc");
- s61's registration, harness and Results.

The review log is §10.

**For the owner, in plain words.**
- **The question.** On c3's netem line (20 Mbit/s, 20 ms one way, GE loss) with the jitter set to 0, how far must the
  store law's knee window minimum rise above the floor? Is that need small enough for Realtime's own margin to
  cover it?
- **What is scored.** One number: the knee need at J = 0 in ms, on the law's own statistic, against a threshold
  derived from the law's consequence.
- **What a Result contains.** The label and its registered consequence (§6) only. Interpretation goes to a derivation.
- **Scope.** J = 0 only: one rehearsal plus one cell. The J = 3 cell is split off, and it is decided after this
  Result.
- **Cost.** ≈ 1.4 h of VM typical (≈ 2.8 h worst), with host cargo paused. The binary is `e410d640`, Bulk only, so no
  F2 fix is needed. It also needs a tools-only harness change.

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place, each loaded on its own) | output | what it does |
|---|---|---|---|
| `store-law/s62pred.py` | `l1-raw/s61/rep61c-diag-ld.tgz` and `rep61a-diag-ld.tgz` (c3 at J = 5, two sessions), `l1-raw/s60/rep60b-diag-ld.tgz` (c1s) | `s62pred.out` | the registered scorer, prototyped and run end to end (§7) |

**c1s is not a J = 0 stand-in for c3.**
- `lib.sh` `scenario_params`: c1 is `1gbit 1 0 0.05 50` and c3 is `20mbit 20 5 2 40`.
- c1s is a 1 Gbit/s link, 50× faster, with a 2.4 ms floor against c3's ≈ 40 ms, and it is receiver-bound (R5.5).
- So it is used here only to show that a rule which divides by a spread breaks when the spread is small (§4). None of
  its numbers predicts a c3 J = 0 value.

---

## 1. The scored quantity and its label

```text
   need₀  =  median_r [ T̂min_r(W₉₅,r) ]  −  F₀
      T̂min_r(W₉₅,r): the replicate's knee window minimum on NATIVE windows (nk ≥ 10, t ≥ 15 s), interpolated over the K and P1 arm
                     minima at its W₉₅,r                                  (the law's statistic: store-law.md R6.2, R = 1 iff T̂min = F̂ + A)
      W₉₅,r:         where gp / gp_ctl(own replicate) crosses 0.95 over every non-control arm (cross_up)
      F₀:            the W 12 floor arm's median over rows of the row's G(250) median group minimum (a fixed count, §1.2)
      need₀,int:     the same knee minus the intercept of the OLS line through the W 12 and W 24 floor arms' G(250) row values
```

### 1.1 The gate: the floor's count drop at J = 0

```text
   D₀  =  max( F_native, F_G(250) )  −  F_G(2100)          on the W 12 floor arm, differences of arm medians
   need₀ is SCOREABLE only if D₀ < 0.5 ms                  (D₀ at W 24 is reported)
```

- **Why.** At J = 5 the W 12 floor minimum falls 8.43 / 7.07 ms from native to 2 100 samples (61c / 61a,
  `s62pred.out` §2). So need there depends on the floor's count: 3.48 ms on the native floor, 7.46 at G(250) and
  11.91 at G(2 100). The gate admits need₀ only if, at J = 0, the count moves the floor by < 0.5 ms, which is half
  of Δ₀.
- **D₀'s expected size at J = 0 is not bounded by existing rows, so it is registered as the rehearsal's first
  reading** (§3.3). The size depends on what drives the drop:
  - An absolute timing effect (CPU, timers) would give about c1s's 0.035 ms per doubling, so D₀ ≈ 0.1–0.2 ms over
    the ≈ 5 doublings from ≈ 60 to 2 100 samples.
  - A service-time effect would scale with the datagram time. c1s's 0.035 ms is ≈ 3.6 datagram times at 1 Gbit/s
    (1 225 B, 9.8 µs). At c3 a datagram takes 0.49 ms and an skb (gso ≈ 3.7 at the floor) ≈ 1.8 ms, so that would
    give D₀ of several ms.
  - The 0.5 ms gate is about one datagram time at c3. It admits need₀ only if the floor's count dependence is below one
    datagram's service time.

### 1.2 The floor's role, its fixed count, and the second arm

- **The law's F is not yet defined.** It comes from m̂ / ℓ̂ / b̂₀⁺ in R6.2's anchor, or from R6.7 candidate C, with
  R6.7's open n₀ and item 4. So the floor's count here is a measurement choice, and s62 fixes it at G(250).
- **The second floor arm is W 24** (six rows).
  - At J = 5 the floor's load slope moved need by b_F·W₁ = 1.07–1.23 ms. In the prototype the intercept reading is
    0.99–1.18 ms higher than the W 12 reading.
  - **FLOOR-SENSITIVE** iff the W 12 and intercept readings give opposite definite labels (s61 §5.1).
- **Bytes.** W 12 at 25 MB and W 24 at 30 MB, both with a 150 s timeout, give ≥ 8 complete G(2 100) groups per row
  for D₀ (§2).

### 1.3 The threshold Δ₀ and the label

```text
   Δ₀  =  ½ · q_R · F₀            q_R = 0.05 (RFC 8289's band, cited);  F₀ measured in session
```

- **What Δ₀ is.** It is a consequence threshold, not a hypothesis test.
  - Realtime's own margin q_R·F̂ lands it above its knee iff the knee need is below q_R·F̂ (s61 §5.2's consequence).
  - need₀ < Δ₀ means a jitter-free c3 line needs no knee term beyond half of Realtime's own margin.
- **Predicted F₀ ≈ 41–48 ms, so Δ₀ ≈ 1.0–1.2 ms.** That is the 40 ms propagation plus the echo's own excess. At J = 5
  the floor median sits 8.7 ms above netem's 40, and how much of that excess survives at the minimum is unknown. On
  the stand-ins Δ₀ is 0.97.
- **Bootstrap:** 90 %, 2 000 draws over replicates and floor rows on both arms (the line refitted per draw), seed 62.

| LABEL62 NEED0 | condition (in this order) |
|---|---|
| CONTROL-MOVED | fewer than 4 unflagged W 512 controls |
| UNSCOREABLE | D₀ ≥ 0.5 ms; or fewer than 3 valid rows on either floor arm; or fewer than 5 replicates with W₉₅,r ∈ [K₁, K₃] |
| **BELOW** | CI(need₀) upper bound < Δ₀ |
| **ABOVE** | CI(need₀) lower bound > Δ₀ |
| UNRESOLVED | otherwise |
| **FLOOR-SENSITIVE** | the W 12 and intercept readings give opposite definite labels (replaces either) |

**No prediction for need₀ is registered.** Existing rows cannot bound it.
- Its jitter-free part includes the rate queue's own rise at the knee, R_q.
- J = 5 bounds R_q only from above, R_q ≲ the J = 5 native need of 3.5–3.8 ms, if jitter's net effect on need is
  ≥ 0. So R_q may lie on either side of Δ₀.
- c1s's +0.14 ms (W 64 → 192) is 14 of its datagram times. Scaled to c3's datagram time that would be ≈ 7 ms at u 0.5,
  which J = 5's 3.5–3.8 ms at u 0.95 rules out. So c1s's rise is not a rate-queue figure at c3's scale.
- So the label measures a number against a consequence threshold. It is not a test of "the jitter carries the need".
  Either label is reachable.
- The expected 90 % half-width is ≈ 0.5 ms (61c / 61a under the failing gate). It resolves against Δ₀ ≈ 1 ms unless
  need₀ lies within ≈ 0.5 ms of Δ₀.

---

## 2. The cell and arms (binary `e410d640`, Bulk, single leg; J = 0 only)

| cell | netem line (`scenario_params`) | rows |
|---|---|---|
| `c3j0-25` | `20mbit 20 0 2 40` | floor W 12 (25 MB) × 6, floor W 24 (30 MB) × 6, K₁/K₂/K₃ × 6, P1 × 6, control W 512 × 6 = **42**; plus 2 dump rows |

- **K arms** come from the pilot (§3.2): r4(0.92·Ŵ) / r4(1.06·Ŵ) / r4(1.20·Ŵ), and P1 = r8(1.5·Ŵ).
- **The K, P1 and control arms** are 120 MB with a 120 s timeout, as 61a.
- **Order:** a cyclic Latin square over replicates, seeds 42 / 7 alternating.
- **Dump rows:** one `RWM_RTT_DUMP=1` row at W 12 and one at K₂, kind `D`.
  - **Forwarding:** `tools/l1/lib.sh:48` at `3cdc4470` lists `RWM_RTT_DUMP RWM_RTT_DUMP_MAX`, and the `env -u` scrub
    (`loopdiag_battery.sh:459`) does not unset them.
  - **Same stream:** the dump shares `record_rtt_sample` with the `[SLAW]` sampler (`github/slaw-gauge` `d416d59b`,
    `raptorpath/src/scheduler/path.rs`).
  - **Excluded from scoring:** `D` rows are excluded by kind from every scored median, from CR62's reference and from
    the arm medians (a scorer test asserts it). They feed only §6's reported items.
- **J = 3 is a separate launch,** decided after this Result. It feeds no scored reading here.

---

## 3. Validity, the pilot and the rehearsal

### 3.1 CR62 (CR61 made J = 0-safe; every clause now needs the goodput cut)

```text
   e_own(tick) = rtt=(tick) − (row's median native LATE tmin)
   X_own = E90 + 2·(E90 − E50)     over the cell's W 12 rows        X_min = q_R · F_floor        X_use = max(X_own, X_min)
   floor arms (below K₁):  COLLAPSED iff share{e_own > X_use} > 0.10                       AND  gp < 0.95 × the arm's median gp
   at and above K₁:        COLLAPSED iff ( qshare > 0.10  OR  E90_own / max(p90 − tmin, X_min) > 1.5 )  AND  gp < 0.95 × arm median
   a clause that fires at full goodput prints RULE-FINDING-CR62 and voids nothing
```

- **Why the goodput cut on every clause.** A rule that fires at full goodput is a rule finding, not a collapse.
  - CR61's ratio clause alone flagged healthy W 2 048 controls (§61a Result, defect 1).
  - Under CR62, 61a's six W 2 048 controls (ratio 1.51–1.90 at 17.7–17.8 Mbit/s) print RULE-FINDING and are not
    voided (`s62pred.out` §1).
  - c1s's flags drop from 6/12 to 5/12 at W 2 048. The rest are its receiver-bound collapses, with low goodput.
- **At c3 J = 0, q_R·F ≈ 2.0–2.4 ms, so X_min will likely bind.** The floored ratio then becomes an absolute test,
  E90_own > 1.5·X_min ≈ 3–3.6 ms.
  - E90_own at J = 0 is not bounded by existing rows. At J = 5 the floor's E90 is 9.3 ms, dominated by jitter, and
    c1s's 0.83 ms is a different link.
  - So E90_own, in ms, on the floor arm, a knee arm and the control is registered as the rehearsal's first reading
    (§3.3), with no prediction.
- **Bind gauges, printed:** whether X_min binds, the number of rows with a floored denominator, and the RULE-FINDING
  count per arm.

### 3.2 The pilot (proportional rule, W 512 control)

```text
   Ŵ  =  min( 80 · 0.95 / u₈₀ ,  128 )          u = gp / gp(R6, W 512); clamp [48, 192]; rounded to 0.1;  probes W 80 / W 128
```

**Where the knee should be at J = 0** (from printed quantities):
- The floor's per-slot utilisation at J = 5 is κ = 0.00962 per slot. The straight line u = κ·W reaches 0.95 at
  W 99, and J = 5's knee (108.7) sits 1.10× beyond that linear point. c2's sits 1.24× beyond its own.
- At J = 0 the floor loop is no longer than J = 5's, since jitter does not move the floor median. It may be shorter if
  part of J = 5's 8.7 ms median excess is jitter-driven.
  - So κ₀ ∈ [0.0096, 0.0112] per slot, and the linear reach is W 85–99.
  - With the 1.10–1.24 concavity factor, **W₉₅(J0) ≈ 93–123.**
- Round 2's probes 72 / 104 assumed 85–90, a loop scaling that took J = 0's floor loop as shorter than J = 5's. That
  premise does not hold, and J = 5's own curve has u₁₀₄ ≈ 0.93. So the probes are moved to **80 / 128**. W 128
  reaches 0.95 for any knee ≤ ≈ 125.

**The bracket against the rule's bias.**
- The bias Ŵ / W₉₅ is 0.93–0.97 on 61c (`s62pred.out` §6) and 0.89–0.95 on c2. A straighter J = 0 curve could push it
  to 1.00.
- **K is set to r4(0.92 / 1.06 / 1.20·Ŵ).** K₁ margin = (W₉₅ − K₁)/W₉₅ and K₃ margin = (K₃ − W₉₅)/W₉₅:

| u₈₀ | Ŵ | K₁ / K₂ / K₃ | P1 | bias 0.89 | 0.92 | 0.95 | 0.97 | 1.00 |
|---|---|---|---|---|---|---|---|---|
| 0.78 | 97.4 | 88 / 104 / 116 | 144 | +19.6 / +6.0 % | +16.9 / +9.5 | +14.2 / +13.1 | +12.4 / +15.5 | +9.7 / +19.1 |
| 0.82 | 92.7 | 84 / 100 / 112 | 136 | +19.3 / +7.5 | +16.6 / +11.2 | +13.9 / +14.8 | +12.1 / +17.2 | +9.4 / +20.8 |
| 0.86 | 88.4 | 80 / 92 / 108 | 136 | +19.4 / +8.8 | +16.7 / +12.4 | +14.0 / +16.1 | +12.2 / +18.5 | +9.5 / +22.2 |
| 0.90 | 84.4 | 76 / 88 / 100 | 128 | +19.9 / +5.4 | +17.2 / +8.9 | +14.5 / +12.5 | +12.7 / +14.9 | +10.0 / +18.4 |
| 0.93 | 81.7 | 76 / 88 / 100 | 120 | +17.2 / +8.9 | +14.4 / +12.6 | +11.7 / +16.2 | +9.8 / +18.7 | +7.0 / +22.4 |

- **Every margin is positive,** from +5.4 % to +22 %. No fourth arm is needed.
- **The low-u corner.** A knee near 123 puts u₈₀ ≈ 0.65–0.70, where c2 measured biases down to 0.81–0.85. At u₈₀ 0.70
  (Ŵ 108.6, K 100 / 116 / 132), bias 0.85 leaves K₃ +3.3 % and K₁ +21.7 %.
- **On 61c's curve,** K = 96 / 108 / 124 from W 80 or W 88, with 6 / 6 W₉₅ inside.
- **The other cases** (both-above, secant, degenerate, non-monotone) print `case=` and are NO-GO-PILOT (§3.3).
- **The harness must parameterise the pilot.** `plan61.py` at `3cdc4470` hardcodes `PILOT_W = (88, 128)` (`:28`), the
  rehearsal probes (`:46`), and the cap `hi` = 128 inside `pilot_detail` (`:82–97`).
  - `plan62.py` takes `PILOT62 = (80, 128)` and the multipliers (0.92, 1.06, 1.20, 1.5).
  - `pilot_detail(u_lo, u_hi, p_lo, p_hi)` uses cap = p_hi and prints `PILOT62 W=… u_lo=… u_hi=… arms=… case=…`.
  - Tests cover every row above and `arms(float(printed W)) == printed arms`.

### 3.3 The rehearsal: its first readings, and its go/no-go (thresholds identical to the cell's)

**Rehearsal rows:**
- R1 / R6 control W 512;
- R2 / R3 floor W 12 (25 MB);
- R4 / R5 the probes W 80 / W 128;
- D1 a dump row at W 12.

**First readings, registered and printed before any GO decision.** They are J = 0's first measurements of quantities
no existing row bounds:
- D₀ on the pair;
- F_native, F_G(250) and F_G(2100) on the pair;
- E90_own (ms) and p90 − tmin on R2/R3, R5 and R1/R6;
- X_own on the pair, and whether X_min binds;
- u₈₀ and u₁₂₈;
- the dump's coverage.

**GO iff all of these hold:**

| check | rows | rule (the cell's own) | NO-GO label |
|---|---|---|---|
| floor validity | R2, R3 | not COLLAPSED (CR62), and ≥ 6 G(250) groups | NO-GO-FLOOR |
| the gate's pre-read | R2, R3 | D₀ on the pair < 0.5 ms | NO-GO-GATE |
| pilot | R4, R5 | case = proportional | NO-GO-PILOT |
| control health | R1, R6 | goodput ≥ 0.98 × max(R4, R5); loss truth ≤ 4.76 % + 1 pp; R1 / R6 within 1 % | NO-GO-CONTROL |
| LATE length | every row | ≥ 100 native LATE windows | NO-GO-SHORT |

- **CR62's ratio and qshare clauses are not go/no-go items.** On a single row the goodput cut cannot be evaluated
  (§61 Amendment 2), so a fire there prints RULE-FINDING-CR62 and decides nothing. The cell applies CR62 with its
  goodput cut.
- **Dump coverage** below 0.95, or a `[RTTDUMP-CAP]`, marks the dump readings missing and is not a NO-GO.
- **Reordering** is reported as not measured. No counter exists:
  - `tools/l1` at `3cdc4470` has only `lib.sh:174–178`'s comment about an earlier offline measurement;
  - the binary's `out_of_order` is a config field (`raptorpath/src/config.rs:92`);
  - netem's stats carry none.
- **A NO-GO** (registered, so there is no amendment-then-session loop) ends s62. Its Result is the NO-GO label, the
  first readings, and consequence UNSCOREABLE (§6). The failed rule is re-derived in a derivation. NO-GO-GATE's
  consequence is R6.7 item 4's count rule first.

---

## 4. Every spread- or tail-divided rule, checked for small spreads (c1s shows the failure mode only)

| rule (s61 form) | on c1s | small-spread verdict | s62 form |
|---|---|---|---|
| CR61 X_own = E90 + 2(E90 − E50) | 1.59 ms on a 2.38 ms loop | tail-scaled, unbounded below | floored at X_min = q_R·F_floor (≈ 2.0–2.4 ms at c3); bind gauge |
| CR61 ratio E90_own / (p90 − tmin) > 1.5 | denominators 1.5–25 ms | spread-divided, unbounded | denominator floored at X_min: an absolute ≈ 3–3.6 ms test at c3 J0, with the goodput cut (§3.1) |
| CR61 ratio alone, at full goodput | 61a's healthy W 2 048 controls flagged | **fails**: a collapse rule firing on healthy rows | the goodput cut on every clause; RULE-FINDING otherwise |
| τ = 0.05·S_F | 0.02 ms | **fails** | not used |
| s61's Δ = q_R·F / S_F, and ρ | unbounded as S_F → 0 | **fails** | replaced by Δ₀ = ½·q_R·F₀ in ms; need₀ scored in ms |
| the D₀ gate (absolute 0.5 ms ≈ one c3 datagram) | trivially 0 (≈ 5 400-sample windows) | safe as a rule; its J = 0 value is a first reading | as registered |
| floor validity, ≥ 6 G(250) groups | – | safe | as registered |

---

## 5. The scorer (`tools/l1/s62_score.py`, the worker's port of `s62pred.py`)

**Inputs:** the ledger tgz, `LD_KNEE62` (the rehearsal's `PILOT62 W=`), and `LD_CTL62=512` (refuse otherwise).

**Steps** (each printed):
1. CR62 per row, with its bind gauges and RULE-FINDING count.
2. The control (≥ 4 unflagged, else CONTROL-MOVED).
3. Both floor arms: native, G(250) and G(2 100); D₀ and the gate.
4. The knees (native tmin; scoreable iff W₉₅,r ∈ [K₁, K₃]).
5. need₀ and need₀,int with the bootstrap.
6. Δ₀.
7. The label and FLOOR-SENSITIVE.
8. §6's reported items.

**Labels printed:**
- `LABEL62 GO-c3j0 GO|NO-GO-<rule>` (rehearsal)
- `LABEL62 CONTROL-c3j0`
- `LABEL62 GATE-J0 PASS|FAIL`
- `LABEL62 NEED0 <label>`

---

## 6. Results report the label and its registered consequence only

**Reported, deciding nothing:**
- need at J = 0 at the native, G(250) and G(2 100) floors (W 12), and at the intercept;
- J = 5's need at the same three counts, from 61c / 61a (other sessions);
- **S_F(J0)** on the native floor windows (p50 − min) and at G(250);
- decision 8's Bulk supply at J = 0, A₀ = S_F(J0) + q_B·F₀, and D = need₀ − A₀;
- the p10 / p50 / dump-IQR rises;
- the dump's count test;
- D₀ at W 24;
- reordering (not measured).

**Consequence table.** It is worded on the printed numbers only.

| LABEL62 NEED0 | registered consequence |
|---|---|
| **BELOW** | On c3's loop, rate and loss with no jitter, the knee need is below half of Realtime's own margin q_R·F̂. A knee term in the allowance is needed on this line only where jitter (or another shape) is present. What carries it is not settled by s62. |
| **ABOVE** | On c3's loop, rate and loss with no jitter, a knee need of need₀ ms (printed) survives above Δ₀. The Result prints S_F(J0) and D = need₀ − (S_F(J0) + q_B·F₀). Whether decision 8's Bulk allowance covers it on this line is that printed D's sign, nothing more. |
| **FLOOR-SENSITIVE** | Where the floor is read decides the label. R6.7's floor position (item 1) comes before any knee reading. |
| **UNRESOLVED** | NEEDS-MORE-replicates. Never a refutation. |
| **UNSCOREABLE**, or any rehearsal NO-GO | NEEDS-MORE-\<named\>, with the first readings reported. A gate failure means the J = 0 floor still depends on its count, so R6.7 item 4's count rule comes first. |
| **CONTROL-MOVED** | NEEDS-MORE-control. CR62 at W 512 is re-derived in a derivation. |

**What follows any label:** the J = 3 cell is decided after this Result, as a separate registration amendment and
launch.

---

## 7. The scorer, run end to end on real rows before registration (`s62pred.out`)

**Executed on real rows:**
- CR62 with every clause, the goodput cut and the bind gauges. 61a's W 2 048 controls now print **RULE-FINDING-CR62**
  6 / 6 and are not voided; c1s's W 2 048 flags fall to 5/12;
- both floor arms and the intercept line;
- the gate **FAIL** at J = 5 (D₀ 8.43 / 7.07), and **PASS** trivially on c1s;
- the knees (12 / 12 inside [K₁, K₃]);
- the bootstrap with the line refitted, and Δ₀ (0.97);
- **UNSCOREABLE** from the gate on both stand-ins;
- with the gate forced open (an exercise), need₀ 7.46 / 7.23 and intercept 8.44 / 8.42, all above Δ₀, so the label is
  ABOVE. Under the failing gate need₀ carries J = 5's count drop, which is
  why the gate exists;
- the rehearsal's gate pre-read (NO-GO-GATE at J = 5: D₀ 8.50 / 7.17 on a pair);
- the ratio, E90_own and denominators with the absolute threshold printed;
- the pilot on 61c (6 / 6 inside);
- the three-floor need readings.

**Executed on synthetic inputs only, marked:** BELOW, UNRESOLVED, FLOOR-SENSITIVE,
UNSCOREABLE by replicates, and CONTROL-MOVED, which no stand-in reaches once the goodput cut applies.

---

## 8. The harness change (a Sonnet worker; tools only, the `harness61` pattern)

**Branch:** `harness62` off `3cdc4470`.
- The engine diff against `1ecee59d` is empty, including `ref60-manifest.sha256`, asserted by a test.
- `bin_sha` = `e410d640…5f25d`, with `ABORT-BIN-SHA` otherwise.

**`lib.sh`.**
- `scenario_params`: `c3j0` → `20mbit 20 0 2 40`.
- `cell_spec`: `c3j0-25`.
- `test_topo.sh` asserts the line, with jitter 0.

**`plan62.py`.**
- Plans `62r0` (the rehearsal, §3.3 rows) and `62a` (the cell, §2).
- The pilot is parameterised as §3.2.
- Bytes: W 12 25 MB and W 24 30 MB with 150 s; the rest as 61a.
- `D` rows carry `RWM_RTT_DUMP=1`.
- `LD_KNEE62` is required (`ABORT-PILOT`), and so is `LD_CTL62=512` (`ABORT-CONTROL62`).
- 62a refuses unless 62r0 printed `GO-c3j0 GO`.

**`s62_score.py`.**
- Ports `s62pred.py`, with the labels BELOW / ABOVE.
- A host test reproduces `s62pred.out`'s numbers on the three stand-in tgz.
- Tests cover:
  - every label branch, and FLOOR-SENSITIVE;
  - every go/no-go row (each NO-GO separately);
  - RULE-FINDING at full goodput;
  - `D` rows excluded from every median;
  - the pilot table.
- Red-first.

**`LD_HARD` per launch:**

| launch | LD_HARD |
|---|---|
| 62r0 | launch + 1.0 h |
| 62a | launch + 2.5 h (44 rows, worst 150 s each ≈ 110 min plus overhead) |

**Run rules:**
- host cargo is paused for each launch;
- one `git archive` per launch, extracted fresh;
- the VM tools tests run before any row.

**Cost:**

| launch | typical |
|---|---|
| 62r0 | ≈ 22 min |
| 62a | ≈ 60 min |
| **total** | **≈ 1.4 h** (≈ 2.8 h worst) |

The worker does one harness pass with stub runs. Fable reviews the registration and the harness diff.

## 9. Owner questions

None. Owner decision 13 approved s62. Its J = 0 part now costs ≈ 1.4 h typical. J = 3 is a later, separate decision.

## 10. Review log

**Round 1 → 2:**
- need₀ is scored against zero only, and J = 5 is reported at three counts.
- A W 24 floor arm and FLOOR-SENSITIVE are added.
- The rehearsal thresholds equal the cell's.
- The reorder counter is moved to reported.
- An arm table is added and the pilot is parameterised.
- Dump forwarding is confirmed.

**Round 2 → 3:**
1. **Pilot.** The probes 72 / 104 rested on a loop scaling that does not hold. They are now 80 / 128, from κ and the
   concavity factor (W₉₅(J0) ≈ 93–123). K is r4(0.92 / 1.06 / 1.20·Ŵ), with positive margins at every bias from 0.89
   to 1.00.
2. **c1s is not a stand-in.** D₀, the rate-queue rise and E90_own are not bounded by existing rows, so they are
   rehearsal first readings with no prediction. Δ₀ is restated as a consequence threshold (labels BELOW / ABOVE), not
   a test of the jitter hypothesis. CR62 now needs the goodput cut on every clause (RULE-FINDING otherwise).
3. **J = 3 is split off,** for ≈ 1.4 h typical.
4. **ABOVE's consequence** is worded on the printed need₀, S_F(J0) and D.
