# s62: the c3 jitter sweep (does the knee need survive with the jitter removed?), pre-registration

Status: DRAFT REGISTRATION, round 2 (derivation style). Docs and Python only: no engine code, no cargo, no VM contact.
Branch `s62-design` off `github/main` `3d871561`. It is written to be copied into `docs/status.md` as §62.

It builds on:
- owner decision 13 (2026-11-17);
- the sketch in `knee-margin-rederive` `c6c55958` §5;
- the closed post-hoc analysis (status: "The knee-margin re-derivation, closed post hoc");
- s61's registration, harness and Results.

Round 2 applies Fable's review (§10).

**For the owner, in plain words.**
- **The question.** At c3's rate, delay and loss, with netem's jitter set to 0, does the store law's knee still need
  the window minimum to rise above the floor? Or does that need vanish once the jitter is gone?
- **What is scored.** One number, the knee need at J = 0 on the law's own statistic, tested against zero only. A J = 3
  cell is run beside it, reported and not scored.
- **The cost.** ≈ 2.7 h of VM typical (≈ 5.5 h worst) over four launches, with host cargo paused. The binary is
  `e410d640`, Bulk only, so no F2 fix is needed. It also needs a tools-only harness change.
- **What a Result contains.** Only the label and its registered consequence (§6). Interpretation goes to a
  derivation.

**Script** (stdlib, `python -I -X utf8`, run from `docs/`; output beside it):

| script | input (read in place, each loaded on its own) | output | what it does |
|---|---|---|---|
| `store-law/s62pred.py` | `l1-raw/s61/rep61c-diag-ld.tgz` and `rep61a-diag-ld.tgz` (c3 at J = 5, two sessions), `l1-raw/s60/rep60b-diag-ld.tgz` (c1s, the jitter-free shape) | `s62pred.out` | the registered scorer, prototyped and run end to end: CR62, the scored path with both floor arms, the label on every branch, the rehearsal checks, the pilot, the reported readings |

---

## 1. The scored quantity and its label

```text
   need₀  =  median_r [ T̂min_r(W₉₅,r) ]  −  F₀            (the J = 0 cell)
      T̂min_r(W₉₅,r): the replicate's knee window minimum on NATIVE windows (nk ≥ 10, t ≥ 15 s), interpolated over the K and P1
                     arm minima at its W₉₅,r
      W₉₅,r:         where gp / gp_ctl(own replicate) crosses 0.95 over every non-control arm (cross_up)
      F₀:            the floor arm W 12's median over rows of the row's G(250) median group minimum (a fixed count, §1.2)
      need₀,int:     the same knee minus the intercept of the OLS line through the W 12 and W 24 floor arms' G(250) row values
```

- **Why this statistic.** It is the law's own. The store law's fixed point is R = 1 iff T̂min = F̂ + A on the engine's
  native windows (`store-law.md` R6.2; `knee-margin-rederive.md` §2.4).
- **Tested against zero only.** A comparison with J = 5's need would be count-bound (Erratum 1's defect).
  - On the same J = 5 rows, need's value depends on the floor's count: 3.48 ms on the native floor, 7.46 at G(250) and
    11.91 at G(2 100) (61c, `s62pred.out` §2).
  - So no J = 5 number is a comparator. J = 5 numbers are reported only, at all three counts (§6).

### 1.1 The gate: the floor's count drop at J = 0

```text
   D₀  =  max( F_native, F_G(250) )  −  F_G(2100)          on the W 12 floor arm, differences of arm medians
   need₀ is SCOREABLE only if D₀ < 0.5 ms                  (D₀ at W 24 is reported)
```

- **Why.** At J = 5 the W 12 floor minimum falls 8.43 ms from native to 2 100 samples (61c; 7.07 on 61a). The gate
  admits need₀ only if the J = 0 floor's count moves it by < 0.5 ms anywhere from native to 2 100. That is half of Δ₀.
- **Prediction:**
  - D₀ ≈ 0.1 ms if the drop is jitter sampling: c1s's jitter-free minimum falls 0.035 ms per doubling
    (`knee-margin-rederive.md` §2.1);
  - ≥ 1.5 ms if an echo-timing spread outside netem's jitter carries it.

### 1.2 The floor's role, its fixed count, and the second arm

- **The law's F is not yet defined.** It comes from m̂ / ℓ̂ / b̂₀⁺ in R6.2's anchor, or from R6.7 candidate C, with
  R6.7's open n₀ and item 4. So the floor's count here is a measurement choice, and s62 fixes it: G(250) at every arm
  and J.
- **The second floor arm is W 24**, six rows per cell.
  - At J = 5 the floor's load slope moved need by b_F·W₁ = 1.07–1.23 ms (61a / 61c G(250) lines), which is at or
    above Δ₀. In the prototype, the W 12 and intercept readings differ by 0.99 / 1.18 ms (`s62pred.out` §2).
  - So need₀ is read at W 12 (scored) and at the intercept (the bracket). **FLOOR-SENSITIVE** iff the two give
    opposite definite labels (VANISHES against REMAINS), as s61 §5.1 registered.
- **Bytes and counts.**
  - W 12 at 25 MB and W 24 at 30 MB, both with a 150 s timeout.
  - At ≈ 240 / 480 samples/s that gives ≈ 85–95 s / ≈ 50–60 s. That is ≥ 8 complete G(2 100) groups for D₀ and ≥ 60
    G(250) groups for F₀.

### 1.3 Δ₀ and the label

```text
   Δ₀  =  ½ · q_R · F₀            q_R = 0.05 (RFC 8289's band, cited);  F₀ measured in session
```

- **Derivation** (s61 §5.2's consequence). An error ε in the knee term is a landing error of ε, and Realtime stays
  above its knee iff ε < q_R·F̂. Half of that tolerance is kept for the reading's own uncertainty band.
- Δ₀ never divides by a spread. It is predicted at ≈ 1.0–1.1 ms (F₀ ≈ 40.4–43 ms at J = 0); the stand-ins give 0.97.
- **Bootstrap:** 90 %, 2 000 draws over replicates and floor rows on both arms (the line refitted per draw), seed 62.

| LABEL62 NEED0 | condition (in this order) |
|---|---|
| CONTROL-MOVED | fewer than 4 unflagged W 512 controls |
| UNSCOREABLE | D₀ ≥ 0.5 ms; or fewer than 3 valid rows on either floor arm (§3.1); or fewer than 5 replicates with W₉₅,r ∈ [K₁, K₃] |
| **VANISHES** | CI(need₀) lies below Δ₀ (upper bound < Δ₀) |
| **REMAINS** | CI(need₀) lies above Δ₀ (lower bound > Δ₀) |
| UNRESOLVED | otherwise |
| **FLOOR-SENSITIVE** | the W 12 and intercept readings give opposite definite labels (replaces either) |

**Predictions:**

| sentence | need₀ | label |
|---|---|---|
| the jitter carries the knee need (the FIFO absorbs it) | ≈ 0 to +0.5 ms: the rate queue alone at u 0.95; c1s's jitter-free minimum rises 0.14 ms by u 0.5 | VANISHES |
| the need does not come from the jitter | of the order of J = 5's law's need (3.5–3.8 ms on the native floor) | REMAINS |

- The expected 90 % half-width is ≈ 0.5 ms (61c / 61a: [6.62, 7.72] and [6.83, 7.89] under the failing gate). It is
  smaller at J = 0 if the floor stops moving with its count, so either label resolves against Δ₀ ≈ 1 ms.

---

## 2. Cells and arms (binary `e410d640`, Bulk, single leg)

| cell | netem line (`scenario_params`) | rows |
|---|---|---|
| `c3j0-25` (J = 0, scored) | `20mbit 20 0 2 40` | floor W 12 × 6, floor W 24 × 6, K₁/K₂/K₃ × 6, P1 × 6, control W 512 × 6 = **42**; plus 2 dump rows |
| `c3j3-25` (J = 3, reported) | `20mbit 20 3 2 40` | the same: 42 + 2 |

- **The J = 5 overlap is cut.** It could only feed the reported line, so its ≈ 50 min of VM is not spent. The
  reported line takes J = 5 from 61c and 61a, labelled as other sessions.
- **K arms** come from the pilot (§3.2):
  - K₁ / K₂ / K₃ = r4(1.00·Ŵ) / r4(1.10·Ŵ) / r4(1.20·Ŵ), and P1 = r8(1.5·Ŵ);
  - the K, P1 and control arms are 120 MB with a 120 s timeout, as 61a.
- **Order:** a cyclic Latin square over replicates, seeds 42 / 7 alternating.
- **Dump rows:** one `RWM_RTT_DUMP=1` row at W 12 and one at K₂ per cell, kind `D`.
  - **Forwarding:** `tools/l1/lib.sh` at `3cdc4470` lists `RWM_RTT_DUMP RWM_RTT_DUMP_MAX` in its one forwarded-knob
    list (`lib.sh:48`). `loopdiag_battery.sh`'s `env -u` scrub (`:459`) does not unset them, so a `D` row's arm env
    reaches the binary.
  - **Same stream:** the dump shares `record_rtt_sample` with the `[SLAW]` sampler (`github/slaw-gauge` `d416d59b`,
    `raptorpath/src/scheduler/path.rs`).
  - **Excluded from scoring:** `D` rows are excluded from every scored median, from CR62's X_own reference and from the
    arm medians, by kind, and a scorer test asserts it. They feed only §6's reported quantile readings, and they print
    their coverage dumped/n against the final `[DIAG]` `sig_us=…/n`.

---

## 3. Validity, the pilot and the rehearsal

### 3.1 CR62 (CR61 made J = 0-safe)

```text
   e_own(tick) = rtt=(tick) − (row's median native LATE tmin)
   X_own = E90 + 2·(E90 − E50)     E50 / E90: medians over the cell's W 12 rows of each row's own percentile of e_own
   X_min = q_R · F_floor           X_use = max(X_own, X_min)
   floor arms (below K₁):  COLLAPSED iff share{e_own > X_use} > 0.10  AND  gp < 0.95 × the arm's median gp
   at and above K₁:        COLLAPSED iff ( qshare > 0.10 AND gp low )  OR  E90_own / max(p90 − tmin, X_min) > 1.5
```

- **A valid floor row:** not COLLAPSED, live, no DNF, and ≥ 6 complete G(250) groups.
- **Bind gauges, printed per cell:** whether X_min binds, and on how many rows the ratio's denominator is floored.
- **The floored ratio is an absolute test** whenever p90 − tmin < X_min: E90_own > 1.5·X_min ≈ 3 ms at c3
  (`s62pred.out` §5: 2.91–2.92 ms). It is untested on any c3 row at J = 0. Its J = 0 predictions are in §3.3.
- **On the stand-ins** (`s62pred.out` §1):
  - 61c 0 / 66 flagged;
  - 61a only its W 2 048 controls (CR61's large-pool defect; s62's control is W 512);
  - c1s W 64 2/6, W 192 1/6, W 2 048 6/12 (receiver-bound collapses, r68 §1.4).
  - X_min binds nowhere there.

### 3.2 The pilot (proportional rule, W 512 control, probes per J)

```text
   Ŵ  =  min( P_lo · 0.95 / u_lo ,  P_hi )     u = gp / gp(R6, W 512); clamp [48, 192]; rounded to 0.1
   probes (P_lo, P_hi):  J = 0: 72 / 104     J = 3: 80 / 116
```

- **The probes are placed for the predicted knees:** W₉₅(J0) ≈ 85–90 and W₉₅(J3) ≈ 97–100, from the loop
  (`s62pred.out` §6). s61's probes 88 / 128 would put J = 0's knee on the lower probe.
- **The bias** Ŵ / W₉₅ is 0.93–0.97 on 61c (from W 80 / 88, `s62pred.out` §6) and 0.89–0.95 on c2 (s61 Amendment 3).
  c1s's concavity estimate is ≈ 0.96. So W₉₅ ∈ [Ŵ/0.97, Ŵ/0.89], and K is centred on that band.
- **On 61c's curve:** K = 104 / 112 / 124 from either probe, with 6 / 6 W₉₅ inside.

**Arms and margins over the plausible lower-probe utilisation** (K₁ margin against the smallest W₉₅, K₃ margin against
the largest):

| J | u_lo | Ŵ | K₁ / K₂ / K₃ | P1 | W₉₅ band | K₁ margin | K₃ margin |
|---|---|---|---|---|---|---|---|
| 0 | 0.75 | 91.2 | 92 / 100 / 108 | 136 | 94.0–102.5 | +2.1 % | +5.4 % |
| 0 | 0.80 | 85.5 | 84 / 96 / 104 | 128 | 88.1–96.1 | +4.7 % | +8.3 % |
| 0 | 0.85 | 80.5 | 80 / 88 / 96 | 120 | 83.0–90.4 | +3.6 % | +6.2 % |
| 0 | 0.90 | 76.0 | 76 / 84 / 92 | 112 | 78.4–85.4 | +3.0 % | +7.7 % |
| 3 | 0.75 | 101.3 | 100 / 112 / 120 | 152 | 104.5–113.9 | +4.3 % | +5.4 % |
| 3 | 0.80 | 95.0 | 96 / 104 / 112 | 144 | 97.9–106.7 | +2.0 % | +4.9 % |
| 3 | 0.85 | 89.4 | 88 / 100 / 108 | 136 | 92.2–100.5 | +4.5 % | +7.5 % |
| 3 | 0.90 | 84.4 | 84 / 92 / 100 | 128 | 87.1–94.9 | +3.5 % | +5.4 % |

- **The other cases:**
  - both-above (u_lo ≥ 0.95) reads high on a concave curve;
  - secant and degenerate (both probes below 0.95) put the knee beyond P_hi.
  - Each prints `case=`. Any case except proportional stops the rehearsal (§3.3).
- **The harness must parameterise the pilot.** `plan61.py` at `3cdc4470` hardcodes `PILOT_W = (88, 128)` (`:28`),
  the probes R4 / R5 in the rehearsal plan (`:46`), and the cap `hi` = 128 inside `pilot_detail` (`:82–97`).
  - `plan62.py` takes probes from a table, `PILOT62 = {0: (72, 104), 3: (80, 116)}`, keyed by the cell.
  - Its `pilot_detail(u_lo, u_hi, p_lo, p_hi)` returns (Ŵ, case, clamped), with the cap = p_hi and the multipliers
    (1.00, 1.10, 1.20, 1.5).
  - It prints `PILOT62 W=… u_lo=… u_hi=… arms=… case=…`.
  - Tests cover every row of the table above, every case, and `arms(float(printed W)) == printed arms`.

### 3.3 The rehearsal's go/no-go (per cell; it agrees with the cell rules)

**Rehearsal rows:**
- R1 / R6 control W 512;
- R2 / R3 floor W 12 (25 MB);
- R4 / R5 the probes;
- D1 a dump row at W 12.

**GO iff all of these hold:**

| check | rows | rule (the cell's own) | on NO-GO |
|---|---|---|---|
| floor validity | R2, R3 | CR62 floor clause (X_own from the pair), ≥ 6 G(250) groups | NO-GO-FLOOR |
| the gate's pre-read | R2, R3 | D₀ on the pair **< 0.5 ms**, the cell's threshold | NO-GO-GATE |
| knee-arm collapse | R5 | CR62 ratio ≤ 1.5 and qshare rule, as the cell | NO-GO-CR62-KNEE |
| control | R1, R6 | CR62 ratio ≤ 1.5; goodput ≥ 0.98 × max(R4, R5); loss truth ≤ 4.76 % + 1 pp; R1 / R6 within 1 % | NO-GO-CONTROL |
| pilot | R4, R5 | case = proportional | NO-GO-PILOT |
| LATE length | every row | ≥ 100 native LATE windows | NO-GO-SHORT |
| dump | D1 | coverage dumped/n ≥ 0.95, no `[RTTDUMP-CAP]` | dump readings reported as missing; not a NO-GO |

- **What a NO-GO means** (registered, so there is no amendment-then-session loop):
  - **J = 0:** s62 ends. Its Result is the NO-GO label with consequence UNSCOREABLE (§6). The J = 3 cell is not run
    (it is context only).
    - The rule that failed is re-derived in a derivation, never in an amendment to this session.
    - NO-GO-GATE's consequence is R6.7 item 4's count rule first.
  - **J = 3:** its cell is not run, and s62's scored reading stands.
- **Predictions at J = 0, from the jitter-free c1s rows** (`s62pred.out` §5):
  - **The knee arm.** On c1s, at u 0.5, E90_own is 1.5–2.2 ms over p90 − tmin of 1.6–1.9 (ratio 0.94–1.14). At c3 J0
    the floor X_min ≈ 2.0 ms, so the rule is E90_own > ≈ 3 ms. The prediction is a ratio of 0.8–1.2 (GO).
  - **The W 512 control.** Its windows' spread is the standing queue's oscillation, not jitter. c1s's healthy
    W 2 048 controls have a denominator of 15–25 ms and a ratio of 0.86–1.06, and 61c's W 512 controls 1.05–1.16. The
    prediction is 0.9–1.2 (GO).
  - **The gate's pre-read.** D₀ < 0.2 ms if the drop is jitter sampling (GO). Otherwise NO-GO-GATE, which is itself an
    answer (§6).
- **Netem reordering is not checked.** No such counter exists in the harness output.
  - `tools/l1` at `3cdc4470` has only `lib.sh:174–178`'s comment ("0 out-of-order in 100 000", an earlier offline
    measurement).
  - The binary prints `out_of_order` only as a config field (`PeerConfig.window_out_of_order`,
    `raptorpath/src/config.rs:92`).
  - netem's `tc -s` stats carry no reorder count.
  - So reordering is moved from go/no-go to reported: "not measured; netem is FIFO under `rate` (`lib.sh:174–178`)".

---

## 4. Every spread- or tail-divided rule, checked on jitter-free rows (c1s, s60)

| rule (s61 form) | on c1s | J = 0 verdict | s62 form |
|---|---|---|---|
| CR61 X_own = E90 + 2(E90 − E50) | 1.59 ms on a 2.38 ms loop (c3 J5: 16.5) | tail-scaled, unbounded below as the jitter → 0 | floored at X_min = q_R·F_floor (≈ 2.0 ms at c3), with a bind gauge |
| CR61 ratio E90_own / (p90 − tmin) > 1.5 | denominators 1.5–25 ms | spread-divided, unbounded as p90 − tmin → 0 | denominator floored at X_min: an absolute ≈ 3 ms test at c3 J0 (§3.3), with a bind gauge |
| τ = 0.05·S_F | 0.02 ms | **fails** | not used (the J = 5 overlap and its drift rule are cut) |
| s61's Δ = q_R·F / S_F, and ρ | 0.28 here, unbounded as S_F → 0 | **fails** | replaced by Δ₀ = ½·q_R·F₀ in ms, with need₀ scored in ms against zero |
| the D₀ gate (absolute 0.5 ms) | 0.000 (trivially) | safe | as registered |
| floor validity, ≥ 6 G(250) groups | every native window ≥ 250 samples | safe | as registered |

---

## 5. The scorer (`tools/l1/s62_score.py`, the worker's port of `s62pred.py`)

**Inputs:**
- the launch's ledger tgz;
- `LD_KNEE62` = the rehearsal's printed `PILOT62 W=`;
- `LD_CTL62=512` (refuse otherwise).

**Steps** (each printed):
1. CR62 per row, with the bind gauges.
2. The control (≥ 4 unflagged, else CONTROL-MOVED).
3. Both floor arms (≥ 3 valid rows each): native, G(250) and G(2 100) per arm; D₀ (W 12) and the gate; the W 24 D₀
   reported.
4. The knees (native tmin; scoreable iff W₉₅,r ∈ [K₁, K₃]).
5. need₀ and need₀,int with the bootstrap (seed 62, the line refitted per draw).
6. Δ₀.
7. The label and FLOOR-SENSITIVE.
8. The reported items (§6).

**Labels printed:**
- `LABEL62 CONTROL-<cell>`
- `LABEL62 GATE-J0 PASS|FAIL`
- `LABEL62 NEED0 <label>`
- `LABEL62 GO-<cell> GO|NO-GO-<rule>` (rehearsal)

---

## 6. Results report the label and its registered consequence only

**Reported, deciding nothing:**
- need at J = 0 and J = 3 at the native, G(250) and G(2 100) floors (W 12);
- the same at J = 5 from 61c and 61a, as other sessions;
- the line need(J) = a + b·S_F,native(J) over J 0 / 3 / 5, by OLS with its residual (two parameters on three points,
  one of them out of session);
- the p10 / p50 / dump-IQR rises;
- the dump's count test;
- D₀ at W 24 and at J = 3;
- reordering (not measured).

**Consequence table.** It claims only whether the knee need depends on the jitter at a fixed loop, never what carries
it.

| LABEL62 NEED0 | registered consequence |
|---|---|
| **VANISHES** | At c3's loop, rate and loss, removing the jitter removes the knee need beyond Δ₀. The knee need depends on the jitter at a fixed loop. What carries it (S_F or another spread) is not settled by s62. |
| **REMAINS** | At c3's loop, rate and loss, a knee need above Δ₀ survives with no jitter. At least part of the knee need does not come from the jitter at a fixed loop. Decision 8's g·S_F term cannot be the whole knee term on this line. |
| **FLOOR-SENSITIVE** | Where the floor is read decides the label. R6.7's floor position (item 1) comes before any reading of the knee need. |
| **UNRESOLVED** | NEEDS-MORE-replicates. Never a refutation. |
| **UNSCOREABLE** (gate, floor or knee bracket), or a J = 0 NO-GO | NEEDS-MORE-\<named\>. A gate failure means the J = 0 floor still depends on its count, so R6.7 item 4's count rule comes first. |
| **CONTROL-MOVED** | NEEDS-MORE-control. CR62 at W 512 is re-derived in a derivation. |

---

## 7. The scorer, run end to end on real rows before registration (`s62pred.out`)

**Executed on real rows:**
- CR62 with both clauses and bind gauges, on 61c, 61a and c1s;
- **CONTROL-MOVED** (61a's W 2 048 controls, 6 / 6);
- the control-by-goodput exercise run;
- both floor arms and the intercept line;
- the gate **FAIL** (D₀ 8.43 / 7.07) and **PASS** (c1s, trivially);
- the knees (12 / 12 inside [K₁, K₃]);
- the bootstrap with the line refitted, and Δ₀ (0.97);
- **UNSCOREABLE** from the gate on both stand-ins;
- **REMAINS** with the gate forced open as an exercise: need₀ 7.46 / 7.23 and intercept 8.44 / 8.42, all above Δ₀.
  Under the failing gate need₀ carries J = 5's count drop, which is why the gate exists;
- the rehearsal's gate pre-read (NO-GO-GATE at J = 5: D₀ 8.50 / 7.17 on a pair, as it should read);
- the knee-arm and control ratios, with the absolute threshold printed;
- the pilot on 61c (6 / 6 inside);
- the three-floor need readings.

**Executed on synthetic inputs only, marked:** VANISHES, UNRESOLVED, FLOOR-SENSITIVE, and UNSCOREABLE by replicates.
No real row can reach them before the J = 0 cell exists.

---

## 8. The harness change (a Sonnet worker; tools only, the `harness61` pattern)

**Branch:** `harness62` off `3cdc4470`.
- The engine diff against `1ecee59d` is empty, including `ref60-manifest.sha256`, asserted by a test.
- `bin_sha` = `e410d640…5f25d`, with `ABORT-BIN-SHA` otherwise.

**`lib.sh`.**
- `scenario_params`: `c3j0` → `20mbit 20 0 2 40`, `c3j3` → `20mbit 20 3 2 40`.
- `cell_spec`: `c3j0-25`, `c3j3-25`.
- `test_topo.sh` asserts the netem lines, with jitter 0 / 3.

**`plan62.py`.**
- Plans `62r0` (the J0 rehearsal, §3.3 rows, probes 72 / 104), `62a` (the J0 cell, §2), `62r3` (probes 80 / 116) and
  `62b` (the J3 cell).
- The pilot is parameterised as §3.2.
- Bytes: W 12 25 MB and W 24 30 MB with 150 s; the rest as 61a.
- `D` rows carry `RWM_RTT_DUMP=1`.
- `LD_KNEE62` is required (`ABORT-PILOT`), and so is `LD_CTL62=512` (`ABORT-CONTROL62`).
- 62a refuses unless the rehearsal printed `GO-c3j0 GO`.

**`s62_score.py`.**
- Ports `s62pred.py`.
- A host test reproduces `s62pred.out`'s numbers on the three stand-in tgz.
- Tests cover:
  - every label branch, and FLOOR-SENSITIVE;
  - every go/no-go row of §3.3 (each NO-GO separately);
  - `D` rows excluded from every median;
  - the pilot table.
- Red-first.

**`LD_HARD` per launch:**

| launch | LD_HARD |
|---|---|
| 62r0 / 62r3 | launch + 1.0 h |
| 62a / 62b | launch + 2.5 h (44 rows, worst 150 s each ≈ 110 min plus overhead) |

**Run rules:**
- host cargo is paused for every launch;
- one `git archive` per launch, extracted fresh;
- the VM tools tests run before any row.

**Cost:**

| launch | typical |
|---|---|
| 62r0 | ≈ 22 min |
| 62a | ≈ 60 min |
| 62r3 | ≈ 22 min |
| 62b | ≈ 60 min |
| **total** | **≈ 2.7 h** (≈ 5.5 h worst) |

The worker does one harness pass with stub runs. Fable reviews the registration and the harness diff.

## 9. Owner questions

None. Owner decision 13 approved the run. Its cost is now ≈ 2.7 h typical, down from 3.4 h, because the J = 5 overlap
is cut and a W 24 floor arm is added.

## 10. Review log

**Round 1 → 2 (Fable):**
1. The CONSTANT band was count-bound. need₀ is now scored against zero only; J = 5 is reported at three counts, and
   the J = 5 overlap is cut.
2. A W 24 floor arm and FLOOR-SENSITIVE are added.
3. The rehearsal's gate uses the cell's 0.5 ms. The floored ratio is named as an absolute ≈ 3 ms test, with J = 0
   predictions, and every NO-GO's consequence is registered.
4. No reorder counter exists, so reordering is moved to reported.
5. The arm table is added, K is re-centred on the measured bias band, and the pilot is parameterised.
6. Dump forwarding is confirmed, and dump rows are excluded by kind.
