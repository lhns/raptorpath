# s62: the c3 jitter sweep (is the knee margin a spread or a constant, with the jitter removed?), pre-registration

Status: DRAFT REGISTRATION (derivation style). Docs and Python only: no engine code, no cargo, no VM contact. Branch
`s62-design` off `github/main` `3d871561`. It is written to be copied into `docs/status.md` as §62. It builds on:
- owner decision 13 (2026-11-17);
- the sketch in `knee-margin-rederive` `c6c55958` §5;
- that derivation's closed post-hoc analysis (status: "The knee-margin re-derivation, closed post hoc");
- s61's registration, harness and Results.

**For the owner, in plain words.**
- **The question.** At c3's rate, delay and loss, with netem's jitter set to 0, does the store law's knee still need
  the window minimum to rise about 4 ms above the floor (a constant), or about nothing (a margin carried by the jitter
  spread)?
- **The score.** One number is scored: the knee need at J = 0, on the law's own statistic.
- **The cells.** J = 3 and an in-session J = 5 are run beside it. They are reported, not scored.
- **The cost.** ≈ 3.4 h of VM typical (≈ 6.5 h worst) over five launches, with host cargo paused. The binary is
  `e410d640`, Bulk, so no F2 fix is needed. It also needs a tools-only harness change.
- **Every result reports its label and its registered consequence (§6) only.** Interpretation goes to a derivation.

**§61's lessons, applied.**
1. The scorer is specified (§5) and was run end to end on real rows before registering (§7, `s62pred.out`).
2. Every rule that divides by a spread or a tail was checked on jitter-free rows (§4). Two fail, and are replaced.
3. The rehearsal's go/no-go covers every rule that can void the cell, on a knee arm and the control (§3.3).
4. The pilot uses the proportional rule and the W 512 control. Its probes are placed per J and checked against the
   61c and c1s curve shapes (§3.2).
5. The consequence table is in §6.
6. The harness is specified for a Sonnet worker (§8).

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place, each loaded on its own) | output | what it does |
|---|---|---|---|
| `store-law/s62pred.py` | `l1-raw/s61/rep61c-diag-ld.tgz` (J = 5 stand-in), `rep61a-diag-ld.tgz` (J = 0 stand-in), `l1-raw/s60/rep60b-diag-ld.tgz` (c1s, the jitter-free shape) | `s62pred.out` | the registered scorer, prototyped: CR62, the scored path, the drift rule, the label on every branch, the rehearsal checks, the pilot, the reported line |

---

## 1. The scored quantity

```text
   need_J  =  median_r [ T̂min_r(W₉₅,r) ]  −  F_J
      T̂min_r(W₉₅,r): the replicate's knee window minimum on NATIVE windows (nk ≥ 10, t ≥ 15 s), interpolated over the K and P1 arm
                     minima at its W₉₅,r (s61's knee_of, with native tmin)
      W₉₅,r:         where gp / gp_ctl(own replicate) crosses 0.95 over every non-control arm (cross_up)
      F₀  (J = 0):   the floor arm W 12's median over rows of the row's G(250) median group minimum    (a fixed count, §1.2)
      F₅  (J = 5):   the floor arm W 12's median over rows of the row's NATIVE median window minimum    (the law's pair: need₅ is the law's need)
```

**Why this statistic.** It is the law's own (`knee-margin-rederive.md` §2.4).
- The store law's fixed point is R = 1 iff T̂min = F̂ + A on the engine's native windows (`store-law.md` R6.2).
- So the knee reading is the native window minimum, and the constant hypothesis's prediction is the law's native
  need at J = 5.

### 1.1 The gate: the floor's count drop at J = 0

```text
   D₀  =  max( F_native, F_G(250) )  −  F_G(2100)          on the J = 0 floor arm, differences of arm medians
   need₀ is SCOREABLE only if D₀ < 0.5 ms
```

- **Why the gate.** At J = 5 the floor minimum falls 8.4 ms from native windows to 2 100 samples (`s62pred.out` §2),
  so any need there carries the floor's count. The gate admits need₀ only if, at J = 0, the floor's count moves it
  by < 0.5 ms anywhere between native and 2 100. That is half of Δ₀ (§1.3).
- **Prediction.**
  - D₀ ≈ 0.1 ms if the count drop is jitter sampling. On the jitter-free c1s line the minimum falls 0.035 ms per
    doubling (`knee-margin-rederive.md` §2.1).
  - D₀ ≥ 1.5 ms if it is an echo-timing spread outside netem's jitter.
- **The gate executed on real rows** (§7): at J = 5 it FAILS (D₀ 8.43 on 61c, 7.07 on 61a). On c1s it passes
  trivially (native windows hold ≈ 5 400 samples).

### 1.2 The floor's role and its fixed count

- **The law's F is not yet defined.** It comes from m̂ / ℓ̂ / b̂₀⁺ in R6.2's anchor, or from R6.7 candidate C, and its
  count is R6.7's open n₀ and item 4. So the floor's count here is a measurement choice.
- **s62 fixes it:** the floor arm is W 12 at 25 MB, with F₀ read at G(250) at every J. The gate bounds the choice's
  effect at J = 0 to < 0.5 ms.
- **There is one floor arm and no floor line.** At J = 0, netem's FIFO has no jitter to absorb, so the floor's load
  slope reduces to the rate queue's: c1s's floor slope is 0.0009 ms/slot (r68 §1.2).

### 1.3 The margin Δ₀ and the label

```text
   Δ₀  =  ½ · q_R · F₀            q_R = 0.05 (RFC 8289's band, cited);  F₀ measured in session
```

- **Derivation** (s61 §5.2's consequence). An error ε in the knee term is a landing error of ε, and Realtime stays
  above its knee iff ε < q_R·F̂. The two acceptance bands must be disjoint, so each takes half.
- **Predicted** F₀ ≈ 40.4–43 ms (the 40 ms propagation plus the echo excess), so Δ₀ ≈ 1.0–1.1 ms. The bands are
  disjoint iff 2·Δ₀ < need₅. On the stand-ins that is 1.94 < 3.48 (`s62pred.out` §3).
- **Bootstrap:** 90 %, 2 000 draws over replicates and floor rows, seed 62.
- **Labels, in order:**

| label | condition |
|---|---|
| CONTROL-MOVED | fewer than 4 unflagged W 512 controls at J = 0 |
| UNSCOREABLE | D₀ ≥ 0.5 ms; or fewer than 3 valid floor rows (§3.1); or fewer than 5 replicates with W₉₅,r ∈ [K₁, K₃] |
| SPREAD-CARRIED | CI(need₀) ⊂ (−Δ₀, +Δ₀) |
| CONSTANT | CI(need₀) ⊂ (need₅ − Δ₀, need₅ + Δ₀), with need₅ the in-session J = 5 law's need (point value) |
| INTERMEDIATE | CI(need₀) ⊂ [Δ₀, need₅ − Δ₀] |
| UNRESOLVED | otherwise (the CI straddles a band edge) |

- **need₅ comes from the in-session J = 5 overlap** (§2), not from 61c. The two c3 sessions differ by ≈ 0.3–0.4 ms in
  native need (3.48 against 3.77 on W 12's native floor).
- If that overlap reads DRIFT, need₅ is still its own value, and 61c / 61a are reported beside it.

**Predictions:**

| sentence | need₀ |
|---|---|
| spread-carried | ≈ 0 to +0.5 ms (the rate queue alone at u 0.95; c1s's jitter-free minimum rises 0.14 ms by u 0.5) |
| constant | need₅ ≈ 3.5–3.8 ms (the law's native need on the stand-ins) |

- The expected 90 % half-width is ≈ 0.5 ms (61c / 61a: [6.62, 7.71] and [6.83, 7.93] around 7.46 / 7.23 under the
  failing-gate form), and smaller at J = 0 if the floor stops moving with its count. It resolves either band (2·Δ₀ ≈ 2 ms).

---

## 2. Cells, arms and the J = 5 overlap (binary `e410d640`, Bulk, single leg)

| cell | netem line (`scenario_params`) | rows |
|---|---|---|
| `c3j0-25` (J = 0, scored) | `20mbit 20 0 2 40` | floor W 12 (25 MB) × 6; K₁/K₂/K₃ × 6; P1 × 6; control W 512 × 6 = 36; plus 2 dump rows |
| `c3j3-25` (J = 3, reported) | `20mbit 20 3 2 40` | the same: 36 + 2 dump |
| `c3-25` J = 5 overlap (in session) | `20mbit 20 5 2 40` (unchanged) | floor W 12 (25 MB) × 6; K 100 / 108 / 120 × 6; control W 512 × 6 = 30; plus 2 dump rows |

- **K arms** come from the pilot (§3.2): r4(0.95·Ŵ), r4(1.05·Ŵ), r4(1.14·Ŵ), with P1 r8(1.5·Ŵ).
- **Bytes:**
  - the floor arm is 25 MB with a 150 s timeout;
  - the K, P1 and control arms are 120 MB with 120 s, as 61a.
  - At ≈ 240 samples/s a 25 MB W 12 row gives ≈ 85–95 s, which is ≥ 8 complete G(2 100) groups for D₀ and ≥ 60 G(250)
    groups for F₀. That is 1.6–1.8× inside 150 s. At J = 0 the loop is shorter, so rows are faster.
- **Order:** a cyclic Latin square over replicates, with seeds 42 / 7 alternating (`plan60._grid_rows`).
- **Dump rows:** one `RWM_RTT_DUMP=1` row at the floor arm and one at K₂ per cell. They are kind `D` and are never
  scored or merged.
  - The dump shares `record_rtt_sample` with the `[SLAW]` sampler (`github/slaw-gauge` `d416d59b`,
    `raptorpath/src/scheduler/path.rs`).
  - It feeds §6's reported quantile readings, and prints its coverage dumped/n from the final `[DIAG]` `sig_us=…/n`.
- **The J = 5 drift rule** (as 61b against §60):
  - DRIFT-OK iff |tmin₁₀₈ − 45.96| ≤ 0.3 ms and |u₁₀₈ − 0.947| ≤ 0.02, as arm medians of native tmin and
    gp / control.
  - 0.3 ms = 0.05 × 61c's native S_F 5.80 at J = 5. It is a fixed number, never recomputed from a J = 0 spread (§4).
  - On 61c's own rows it reads DRIFT-OK (|d| 0.000 / 0.001), as it must.

---

## 3. Validity, the pilot and the rehearsal

### 3.1 CR62, the collapse rule (CR61 with its tail-scaled thresholds made J = 0-safe)

```text
   e_own(tick) = rtt=(tick) − (row's median native LATE tmin)
   X_own = E90 + 2·(E90 − E50)       E50 / E90: medians over the cell's floor-arm rows of each row's own percentile of e_own
   X_min = q_R · F_floor             (J = 0-safe floor: an excursion below the Realtime margin cannot void a row)
   X_use = max(X_own, X_min)
   below K₁ (floor):         COLLAPSED iff share{LATE ticks: e_own > X_use} > 0.10  AND  gp < 0.95 × the arm's median gp
   at and above K₁:          COLLAPSED iff ( qshare > 0.10 AND gp < 0.95 × arm median )  OR  E90_own / max(p90 − tmin, X_min) > 1.5
```

- **Bind gauges.** The scorer prints whether X_min binds (cell), and on how many rows the ratio's denominator is
  floored.
- **A valid floor row:** not COLLAPSED, live, no DNF, and ≥ 6 complete G(250) groups.
- **Every flag on the stand-ins is unchanged by the floors** (`s62pred.out` §1):
  - 61c: 0 / 66 flagged;
  - 61a: only its six W 2 048 controls (ratio 1.51–1.90, CR61's known large-pool defect; s62's control is W 512);
  - c1s: W 64 2/6, W 192 1/6 and W 2 048 6/12, the receiver-bound collapses r68 §1.4 names.
  - X_min does not bind anywhere there: X_own is 16.5 / 17.5 / 1.59 ms against X_min 1.94 / 1.94 / 0.12.

### 3.2 The pilot (two rows in each rehearsal; the proportional rule; the W 512 control)

```text
   Ŵ  =  min( P_lo · 0.95 / u_lo ,  P_hi )         u = gp / gp(R6, W 512);   clamp [48, 192];  rounded to 0.1
   probes (P_lo, P_hi):  J = 0: 72 / 104      J = 3: 80 / 116       (≈ 0.8 / 1.2 × the predicted knee)
```

- **The predicted knees** come from the loop (`s62pred.out` §6; J = 3 interpolated): W₉₅(J0) ≈ 85–90 and W₉₅(J3) ≈ 97–100, against J = 5's 108.7.
  s61's probes (88 / 128) would put J = 0's knee on the lower probe, so they are moved.
- **Checked on the 61c curve.** From W 80 (u 0.743), Ŵ = 102.2 and the arms are 96 / 108 / 116. From W 88, Ŵ = 103.1
  and the arms are the same. W₉₅ is 106.6–110.1, inside [K₁, K₃] on 6 of 6 (`s62pred.out` §6). The rule's bias there is
  0.94–0.95.
- **Checked against c1s's shape.** c1s has no knee, but its u/W falls 23 % over 3× (3.10 → 2.38 Mbit/s per slot). That
  much concavity over the probe-to-knee factor 1.2 is ≈ 4 % (log scale), so the rule reads ≈ 4 % low. Then
  K₁ ≈ 0.91·W₉₅ and K₃ ≈ 1.09·W₉₅, which covers it.
- **The rule's both-above case** (u_lo ≥ 0.95) reads high on a concave curve. It is printed with `case=`. If it
  occurs, the rehearsal stops and reports (§3.3).

### 3.3 The rehearsal's go/no-go (per J cell; every rule that can void the cell)

**Rehearsal rows:**
- R1 control W 512;
- R2 / R3 floor W 12 (25 MB);
- R4 / R5 the probes;
- R6 a second W 512.

**GO iff all of these hold:**

| check | rows | rule |
|---|---|---|
| floor validity | R2, R3 | not COLLAPSED (CR62 floor clause, X_own from the pair); ≥ 6 G(250) groups each |
| the gate's pre-read | R2, R3 | D₀ on the pair < 1.0 ms. Otherwise stop and report: the cell would likely read UNSCOREABLE |
| knee-arm collapse | R5 (P_hi, at or above the knee) | the CR62 ratio ≤ 1.4 (the stop line of s61 Amendment 3); qshare ≤ 0.10 |
| control | R1, R6 | goodput ≥ 0.98 × max(R4, R5); loss truth ≤ 4.76 % + 1 pp; CR62 ratio ≤ 1.4; the two within 1 % of each other |
| pilot | R4, R5 | case ≠ both-above; u_lo < 0.95 ≤ u_hi or the proportional case |
| ordering | every row | netem reorder count 0 (receiver out-of-order counter). Any reorder stops the cell |
| LATE length | every row | ≥ 100 native LATE windows |

- **A dump row at the rehearsal** also checks coverage: dumped/n ≥ 0.95 and no `[RTTDUMP-CAP]`.
- **On the stand-ins** (`s62pred.out` §5), these checks read as follows:
  - 61c K₃ ratios 0.92–0.99 and control 1.13–1.14 (GO);
  - 61a K₃ 0.89–0.91, and its W 2 048 control 1.67–1.77 (NO-GO, the known large-pool defect);
  - c1s W 192 0.94–1.14;
  - loss truth 0.047–0.049 at c3.

---

## 4. Every spread- or tail-divided rule, checked on jitter-free rows (c1s, s60)

| rule (s61 form) | on c1s | J = 0 verdict | s62 form |
|---|---|---|---|
| CR61 X_own = E90 + 2(E90 − E50) | 1.59 ms on a 2.38 ms loop (c3 J5: 16.5) | **tail-scaled**: shrinks with the jitter. It passes on c1s, where flags are real collapses, but nothing bounds it at c3 J0 | floored at X_min = q_R·F_floor (≈ 2.0 ms at c3 J0), with a bind gauge |
| CR61 ratio E90_own / (p90 − tmin)_own > 1.5 | denominators 1.5–25 ms; c1s W 192 ratio 0.94–1.14 | **spread-divided**: unbounded as p90 − tmin → 0 | denominator floored at X_min, with a bind gauge |
| τ = 0.05·S_F (floor gate, drift) | 0.05 × 0.43 = **0.02 ms** | **FAILS**: unusable at J0 | not used at J0. Only the J5 drift rule keeps τ, fixed at 0.3 ms from J5's S_F |
| s61's margin Δ = q_R·F / S_F | 0.05 × 2.38 / 0.43 = 0.28, and unbounded as S_F → 0 | **FAILS**: a ratio margin is ill-conditioned at J0 | replaced by Δ₀ = ½·q_R·F₀ in ms (§1.3): never divides by a spread |
| ρ = need / S_F itself | – | **FAILS** for the same reason | s62 scores need₀ in ms, not a ratio |
| the D₀ gate (absolute 0.5 ms) | 0.000 (trivially: native windows hold ≈ 5 400 samples) | safe: absolute | as registered |
| floor validity ≥ 6 G(250) groups | every native window is ≥ 250 samples | safe | as registered |

**The two rules that failed the J = 0 check, τ = 0.05·S_F and s61's ratio margin (with ρ), are replaced.** The two
tail-scaled CR61 thresholds pass on c1s but are unbounded at small jitter, so they are floored at q_R·F with bind
gauges.

---

## 5. The scorer (specified; `tools/l1/s62_score.py` is the worker's port of `s62pred.py`)

**Inputs, per launch:**
- the ledger tgz (`diag-ld/<cell>@def-<K><W>-s<seed>-r<rep>-a1-{c.log,drv.out}`);
- `LD_KNEE62` = the rehearsal's printed `PILOT62 W=` (as s61);
- `LD_CTL62=512` (refuse otherwise);
- for the label, the J = 5 overlap's ledger.

**Steps** (each printed):
1. CR62 per row (§3.1), with the bind gauges.
2. The control (≥ 4 unflagged, else CONTROL-MOVED).
3. The floor arm (≥ 3 valid rows): F_native, F_G(250), F_G(2100), D₀ and the gate.
4. The knees per replicate (native tmin; scoreable iff W₉₅,r ∈ [K₁, K₃]).
5. need with the bootstrap (seed 62).
6. Δ₀.
7. The drift rule (J5 launch).
8. The label (J0 launch, reading the J5 launch's need₅).
9. Reported only: §6's reported items.

**Labels printed:**
- `LABEL62 CONTROL-<cell>`
- `LABEL62 GATE-J0 PASS|FAIL`
- `LABEL62 DRIFT-J5 DRIFT-OK|DRIFT`
- `LABEL62 NEED0 <label>`

---

## 6. Results report the label and its registered consequence only

**Reported, deciding nothing:**
- the three-point line need(J) = a + b·S_F,native(J) over J 0 / 3 / 5, by OLS with its residual (two parameters on
  three points);
- need under every convention of `knee-margin-rederive.md` §1.1;
- p10 / p50 / dump IQR rises;
- the dump's count test of the median and IQR;
- the D₀ value at J = 3.

**Consequence table:**

| LABEL62 NEED0 | registered consequence |
|---|---|
| **SPREAD-CARRIED** | Along the jitter axis at a fixed loop, the knee needs no margin beyond the rate queue. Decision 8's S_F term may be designed with a sentence on this axis. c2 against c3's DISAGREES is assigned to rate or delay, and is registered next on that axis before g gets cross-cell provenance |
| **CONSTANT** | The margin survives without jitter. Decision 8's S_F term has no sentence and stays a cover. The knee term is re-derived from a non-spread source, which is a new derivation. F07 stays UNDERIVED |
| **INTERMEDIATE** | Decision 8 lacks a term. A constant part a = need₀ must get a sentence before any engine form. F07 stays UNDERIVED |
| **UNRESOLVED** | NEEDS-MORE-replicates. Never a refutation |
| **UNSCOREABLE** (gate, floor, or knee bracket) | NEEDS-MORE-\<named\>. A gate failure means the J = 0 floor still depends on its count, so R6.7 item 4's count rule must come first |
| **CONTROL-MOVED** | NEEDS-MORE-control. CR62 at W 512 is re-derived before any re-run |

---

## 7. The scorer, run end to end on real rows before registration (`s62pred.out`)

**Branches executed on real rows:**
- CR62 with both clauses and both bind gauges, on 61c, 61a and c1s;
- **CONTROL-MOVED** (61a's W 2 048 controls, 6 / 6 flagged: the real §61a defect);
- the control by goodput (61a, labelled as an exercise);
- the floor arm, and the gate **FAIL** (61c D₀ 8.43, 61a 7.07) and **PASS** (c1s, trivially);
- the knees with W₉₅,r ∈ [K₁, K₃] (12 / 12);
- the bootstrap, Δ₀ (0.97 at c3) and the band check (1.94 < 3.48);
- the drift rule (DRIFT-OK on 61c);
- **UNSCOREABLE** from the gate (`LABEL62 NEED0 UNSCOREABLE`, D₀ 7.07);
- the forced-gate exercise, which reaches **UNRESOLVED**: need₀ 7.23 [6.83, 7.93] against need₅ 3.48. need₀ carries
  the J = 5 floor's 3.46 ms count drop, which is why the gate exists;
- the pilot on the 61c curve;
- the reported points (need_law 3.48 / 3.77 at native S_F 5.80 / 5.94).

**Branches executed on synthetic inputs only (marked in §4 of the output):** SPREAD-CARRIED, CONSTANT, INTERMEDIATE,
and UNSCOREABLE from the replicate count. No real row can reach them before s62's J = 0 cell exists.

---

## 8. The harness change (a Sonnet worker; tools only, the `harness61` pattern)

- **Branch** `harness62` off the s61 launch tree `3cdc4470`. The engine diff against `1ecee59d` must be empty,
  including `ref60-manifest.sha256` (asserted by test). `bin_sha` stays `e410d640…5f25d`, with `ABORT-BIN-SHA`
  otherwise.
- **`lib.sh` `scenario_params`:** `c3j0` → `20mbit 20 0 2 40`, `c3j3` → `20mbit 20 3 2 40`. The `cell_spec` entries
  `c3j0-25` / `c3j3-25` → `c3j0 c3j0 single 25000000` (and j3). `topo.sh up c3j0` must shape like c3 with the jitter
  field 0: `test_topo.sh` asserts the line.
- **`plan62.py`** (from `plan61.py`):
  - plans `62r0` (J0 rehearsal: R1–R6 with probes 72 / 104), `62a` (the J0 cell), `62r3` (probes 80 / 116), `62b`
    (the J3 cell), and `62c` (the J5 overlap on `c3-25`, with the arms of §2);
  - bytes per §2, floor 25 MB / 150 s;
  - two `D`-kind rows per cell with `RWM_RTT_DUMP=1`;
  - `LD_KNEE62` required for 62a / 62b (`ABORT-PILOT`), and `LD_CTL62=512` required (`ABORT-CONTROL62`).
- **`s62_score.py`:** a port of `s62pred.py`, with a host test that reproduces `s62pred.out`'s numbers on the three
  stand-in tgz. It also tests every label branch, the rehearsal's go/no-go (§3.3), the pilot's `case=`, and the drift
  rule. Red-first, as s61's harness items.
- **The reorder counter:** the rehearsal prints the receiver's out-of-order count. If no such field exists in the
  gauge tree's output, the worker reports that back, and the coordinator decides before any VM contact.
- **`LD_HARD` per launch:**

  | launch | LD_HARD |
  |---|---|
  | 62r0 / 62r3 | launch + 1.0 h |
  | 62a / 62b | launch + 2.5 h (38 rows; worst 150 s each ≈ 95 min plus overhead) |
  | 62c | launch + 2.0 h |

  The budget guard is set per launch.
- **Run rules:**
  - host cargo is paused for every launch;
  - one `git archive` of the launch sha, extracted fresh per launch;
  - the python and bash tools tests on the VM before any row;
  - 62c may run first or last, but in the same day as 62a.
- **Cost:**
  - VM: 62r0 ≈ 22 min, 62a ≈ 55, 62r3 ≈ 22, 62b ≈ 55, 62c ≈ 50, so **≈ 3.4 h typical, ≈ 6.5 h worst**;
  - the worker: one harness pass with stub runs;
  - review: Fable on this registration and on the harness diff.

## 9. Owner questions

None. Owner decision 13 approved the run.
