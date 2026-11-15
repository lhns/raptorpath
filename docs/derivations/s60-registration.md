# s60: the store law's gauge session (B-1), pre-registration

**For the owner, in plain words.**
- On a static store pool the store fills within half a second. So the law's two floors, b̂₀⁺ (the boot brake) and ℓ̂
  (the lifetime echo minimum), are read from windows that already carry queue.
  - p27 and §56 predict this (P1, P3), but the gauge has not shown it yet.
  - R6.7 (`store-law-r67.md`) also shows that even the boot cap 128 is not queue-free at c2.
- So this session cannot test the law itself. It does three things:
  - it measures where the queue-free floor actually lies, on pools below anything measured so far (R6.7's open
    item 1);
  - it tests the target form with those honest floors, decisively at c8 (B2);
  - at c2 it measures whether the allowance σ + q·ℓ̂ covers the knee (R6.7's open item 2). R6.7 predicts it falls
    ≈ 0.9 ms short.
- The next step is R6.7's derivation, finishing its floor estimator and its allowance form. A large sweep belongs
  after that.

**c2 (entry 10): its scored reading is the allowance shortfall at the knee** (§3.1).
- D = (the window-minimum rise above the floor at the in-session knee) − (σ + q·ℓ̂), from printed `tmin`, `sig` and
  `l`.
- Predicted at +0.9 ms (floor flat below W 64) to +2.2 ms (floor on the secant down to W 16). The 90 % half-width is
  0.22–0.42 ms, so SHORT is predicted and resolved.
- It replaces the per-K anchor flip, which measured b̂₀⁺(K) and was expected UNRESOLVED; the flip is now reported only.
- c2's B2 stays unscored: below the anchor flip the line keeps R > 1.

Status: DRAFT REGISTRATION (derivation style). Docs and Python only: no engine code, no cargo, no VM contact. It is
written to be copied into `docs/status.md` as §60. It is built on `store-law.md` R4.8, R5.6, R6.2 and R6.7, the gauge
branch **`slaw-gauge` `d416d59b`** (it carries the `fec=` token), and `harness59` `b1af3fa4` (plan 59).

**Advisor (Fable) log: what each call or ruling changed** (later entries supersede earlier ones).
1. **Before the arms.** Printed b0p tracks the standing queue, so the replay R̃ was introduced, with a witness. B1
   moved to the calibration arm, and B6 was dropped. G/C was sized with t quantiles, `[CAPLOG]` gating was checked,
   and a K4 print was specified.
2. **After the first sizing.** A c1s input was fixed; integer-ms and prefix-minimum caveats were added; gate timing,
   bracketing and realised-n rules were added.
3. **Before the first push (`ac32ad3a`).** The c1s top is open; every C-row gauge site is verified `slaw`-gated; the
   worst case uses 210 s per c1s row; the warm-up cut was added.
4. **Ruling on `ac32ad3a`.**
   - R̃ is accepted, as a test of the target FORM.
   - P1 leads, with R6.7 (boot hold, n₀) and the R5.5 erratum.
   - Calibration uses W 128 plus a flatness pool, with latch rules.
   - `Rb` is not scored, and the witness is per row, with a negative control.
   - c1s is cut to its control sign, and G/C to a gross-error check.
5. **Ruling on `0eb08d97`.**
   - Calibration-ℓ̂ at c2 and c8 is accepted, and carries B2 alone; the own-ℓ̂ veto is dropped and the masking outcome
     named.
   - P3 (printed `l`) is registered, with a witness that `m` = `l` for t < 10 s.
   - The anchor was checked row by row: c2 keeps both states, c8 is anc 1.
   - Leg-1 boot windows can be empty, so b0₁ is taken from `b0k` entries with `b0kn` ≥ 10.
   - B2 was put in R units.
6. **Ruling on `c3ce83cc` (this revision, worked with the advisor).**
   - c2's B2 is dropped for a conditioning readout. An advisor check then found the premise depends on the
     calibration choice (top of the doc).
   - The c8 and c1s arithmetic was done first (`s60cond.py`). c8's B2 can fire, as a gross-miss detector. c1s's
     control sign discriminates the two T̂min readings, so it is kept.
   - **Session-level calibration** is chosen (§2) after a drift check on §56 (`s60drift.py`). Drift was not detected,
     but the check is unpowered.
   - The session is cut to readings that can come out two ways: **127 rows in two launches** (from 205 in three).
   - K4 uses the existing `fec=` token, re-pinned to `d416d59b`. The witness and negative control were re-run on its
     sample.
   - **Advisor, before the push:** the first commit of this revision (`f8afe926`) claimed c2's B2 was conditioned under
     session calibration. It had left out the row's m̂. The per-row late-m̂ check (`s60abs.py`) gives the anchor-flip
     result at the top of the doc.
   - Restored as Appendix A: the stand-in statistic, the anchor shift, the bands, B6, the 5 % G/C sizing, and the
     out-of-band meanings.
7. **Check of `8536f053` (structure sound).** Settled before the harness brief:
   - **Suites:** a pre-flight runs `suites_run.sh` on the pinned tree, then a reference manifest generated from that
     tree is passed as `LD_REF_MANIFEST`, so both launches hit suite reuse (§7).
   - **c2's verdict probabilities** are computed from §56's rows for each K = 1..8 (`s60flip.py`). At K = 8 the 5-of-6
     vote is a coin flip: STRUCTURAL 0.47, NOT 0.34, MIXED 0.20.
     - It is replaced by a POOLED flip − knee readout per K.
     - The verdict changes with K. At K ≤ 4 the flip falls at or below the calibration arm, which is a finding about
       K.
     - Conditioning becomes the fallback.
   - **The collapse flag does not gate calibration rows** (pools below c1s's BDP). A go/no-go on the rehearsal's c1s
     W 128 row is registered (§2, §5).
8. **Check of `f8ea7d6c`** (entry 7 had no advisor call; the advisor call for this entry returned "temporarily overloaded"; it is reviewed in entry 9).
   - **c2's interval** is set by the threshold ℓ̂_c + b̂₀⁺_c, not by the replicates (`s60flip.out`: 10–90 % width 53
     slots from the threshold alone, 16 from the replicates alone, 52 combined).
     - Calibration rows narrow it only slowly (34 slots at 12 rows, 16 at 48), and the prediction is |D| ≈ 4 slots.
     - So c2 is registered as a measurement, with UNRESOLVED as the expected label.
   - **K:** K = 8 is SCORED at every cell, and K = 1..7 are REPORTED.
     - Brake 1 is 0 by construction (Copa's minimum at the first close is that window's own minimum; p27: 0.00 on all
       6 rows), so K = 1 is no finding. A witness checks it on every row.
     - The latch set is defined per K, and the per-K table is labelled a PREDICTION.
   - **Calibration validity uses an ABSOLUTE collapse rule on printed `tmean − l`.** It flags 22 of §57a's 23
     collapse rows and 1 of 40 unflagged (`s60collapse.py`).
   - **The c1s go/no-go thresholds are set from R̃'s sensitivity** (2/T̂ per ms of b0), inside the gap between the
     registered readings. c8 gets its own rehearsal go/no-go.
   - **The R6.7 note is corrected** to the per-K prediction.
   - **Harness-brief items are added (§9):** a MISS stub case, one archive with equal `bin_sha` across 60-pre,
     rehearsal, 60a and 60b, and what a 60-pre failure outside the gauge's tests means.
9. **Main-session review of `ec627abf`** (entry 8's owed review).
   - With the advisor; the first push of this entry, `66eb4dfd`, was corrected before pushing.
   - **The recipe as fed to the replay is stated once (§1.1):** `b0` = b̂₀⁺_c and `l` = T_c − b̂₀⁺_c, where T_c is
     the median of the per-row sum `l` + `b0p`. So the replay's anchor test and the flip readout use one threshold.
   - **c2's threshold noise does not cancel.**
     - On §56, `l` is reached at boot on only 4 of 14 small-pool rows (1 of 6 at c2; `s60lmin.out`). During boot
       Copa's m̂ sits above `l`, so the sum keeps `l`'s noise.
     - On p27 the joint sum has row sd 0.92 ms, giving a best-case width of 26–30 slots against |D| ≈ 4.
     - UNRESOLVED stays the expected label. The per-K table uses the independent draw, the realistic case.
   - **The collapse rule is defined on the `[DIAG]` fields it was checked on** (the EWMA `rtt=` minus the minimum
     `rtp_us`). X = E90 + 2·(E90 − E50) comes from each cell's own spread: c1s 6.1 ms, which flags 20 of §57a's 23
     collapses and 0 of 40 others. At c2 and c8 the check is in-sample only.
   - **Two rehearsal rows each at c8 and at c1s W 128, GO on either.** The c1s `b0p` ≤ 0.6 ms is reconciled: 0.30
     central plus a 0.30 tolerance, where the KA's 0.12 is an over-read.
   - **The `b0k[0]` = 0 and `m` = `l` witnesses are UNVERIFIED on engine output.** The expected answer is read from
     the code, and the coordinator is asked to run `slaw_reachability`. A failing row is reported, not voided.
   - **60-pre adds the `binid_run.sh` crate-directory build,** whose sha every later stage must match.
   - **The R6.7 note and the top of the doc cite the pooled readout,** replacing the FLAT-AT-KNEE 0.58.
10. **Main session folds in R6.7 (`github/store-law-r67` `4cff88d0`), worked with the advisor.**
    - **Calibration moves to floor pools** below R6.7's smallest measured pool:
      - c2: W 16 / 32 / 48;
      - c8: W 32 / 64 / 96;
      - c1s: W 64 / 128 / 192.
      - Each has its own bytes and a 150 s timeout. Per-leg floor pools use an `nk` ≥ 10 rule.
    - The b0p-flatness gate and "calibrate at n₀ = 128" are withdrawn, because W 128 is not queue-free at c2.
    - **A `tmin` convergence gate** between the two smallest pools replaces them. It is a two-way reading of R6.7's
      open item 1, and its NOT-CONVERGED consequences are signed.
    - **c2's scored reading is the allowance shortfall D** (`s60short.py`): predicted +0.9 ms (flat) to +2.2 (secant),
      half-width 0.22–0.42 ms, so SHORT is resolved. The per-K flip is reported only.
    - **c8's B2 stands.** Its band and firing region are labelled cap-128-based, beside R6.7's 440–446.
    - **R̃₆₇, the R6.7-form replay,** is reported beside every R̃. It is frozen from `r67floor.py` `rcell` and
      self-tested in `s60replay.py`.
    - **The b0k[0] / `m` = `l` witness and c8 leg 1's sampler attachment** become rehearsal checks, since the host
      `slaw_reachability` run prints no lines. A failing row is reported, not voided.
    - **No new engine print.**
    - Rows: 145 (+ ≤ 5), plus 10 rehearsal rows, in two launches.
    - **Advisor, before the push** (`86039f95` was corrected):
      - The adjacent-pool gate could not separate flat from slope. It is now a slope fit over the three floor pools
        (se 0.002–0.005 ms/slot against 0.020–0.027), with a three-way convergence reading that has to be shown.
        At c1s the gate does not cover the scored sign.
      - b0p rises as the pool shrinks (p27 c2: 2.87 / 2.46 / 2.18 at W 64 / 96 / 128). So P1 at c2 is predicted
        ≈ 4.4 or less. A P1 fail no longer swaps the scored quantity, and the absolute b0p go/no-go limits are now
        reported only.
      - A c8 W 96 rehearsal row reads leg 1's attachment, with an unattached-vs-starved test from `[DIAG]` counts.
      - Bytes are sized for the faster rate: c8 200 / 400 / 600 MB, c1s 1 / 1.5 / 2 GB.
      - D and R̃₆₇ are one reading at c2, with a sign witness. The c8 crossing is predicted ≈ 25–30 slots lower.
      - `s60replay.py replay` takes `l` as well as `b0`.

**Helper scripts** (stdlib Python, run with `-I`; outputs committed beside them in `store-law/`).

| script | what it computes |
|---|---|
| `s60boot.py` | per arm and row on §56: boot queue (first 2 s), late queue, m̂'s lift, `l`, σ at boot and at load |
| `s60p27.py` | per static cap and row from p27/p34 window minima: `b0p` per leg, late T̂min − ℓ̂, spread, `l`, rebuilt printed R and Rcom |
| `s60ell.py` | row-level Spearman of `l` against W |
| `s60abs.py` | per arm: row sd of `l` against the row sd of the ABSOLUTE late floor, and corr(`l`, floor − `l`) |
| `s60drift.py` | §56 small pools: do `l` and the boot-queue proxy drift with row order? |
| `s60cond.py` | the conditioning arithmetic per cell (slope of R̃ in W, σ_R, 2σ_R in slots), and c1s verdict probabilities per reading |
| `s60flip.py` | the c2 anchor flip against the knee from §56's rows, per K = 1..8: verdict probabilities and the pooled flip − knee |
| `s60collapse.py` | the absolute collapse rule checked against §57a's collapse-flagged rows |
| `s60collapse56.py` | the per-cell and per-leg collapse threshold X from §56's queue-free rows, and its flags on them |
| `s60lmin.py` | §56 small pools: the tick at which `rtp_us` first reaches the row's minimum `l` (is `l` reached at boot?) |
| `s60band.py` | the crossing bands (reported context) |
| `s60size.py` | row times; G/C pair sds and n |
| `s60replay.py`, `s60witness_ka.log` | `slaw_r` transcribed: R̃, the per-row witness, and its negative control |

---

## 1. What R̃ is, what it tests, and the floor predictions

### 1.1 R̃

**Definition.**
- R̃ is the gauge's `slaw_r` (R6.2's line), replayed from each window's printed `[SLAW]` fields.
- **What the replay is fed (frozen), per cell and leg, at K = 8** (each K's report uses its own K):
  - `b0` = **b̂₀⁺_c**, the session median, over the leg's **floor-pool** valid rows (§2, entry 10; previously W 128),
    of the per-row `b0p` (max of the first K `b0k` brakes);
  - at c2 and c8, `l` = **T_c − b̂₀⁺_c**, where **T_c** is the session median over the same rows of the per-row sum
    `l` + `b0p`. The anchor test m̂ ≥ `l` + `b0` is then exactly m̂ ≥ T_c, and F = T_c + b̂₀⁺_c where the anchor
    binds;
  - at c1s, `l` = the row's own (flat there: `s60ell.out` Spearman +0.03, p 0.43), with `b0` = b̂₀⁺_c;
  - every other input is the row's own: `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `sig` (σ at load) and `q`.
- **At c8** this holds for both legs. B2's firing region (321–332) and the band (433–586) were computed from p27's
  cap-128 values. **They are labelled cap-128-based**, beside R6.7's crossing of 440–446 under honest floors, and are
  recomputed in-session from the floor-pool values.
- Per row, R̃_row is the median over LATE windows (t ≥ 15 s at c2 and c8, ≥ 5 s at c1s). `Rb` is not scored.
- **R̃ alone carries B2 (c8) and the c1s control sign.**

**R̃₆₇, the R6.7-form replay, REPORTED beside every R̃** (frozen now, from `r67floor.py` `rcell`; transcribed and
self-tested in `s60replay.py r67_r`: hand values 1.02 and 1.16625):

```text
   R̃₆₇ = Σᵢ wᵢ·( Fᵢ + ½·max(0, maxⱼ Fⱼ − Fᵢ) + σᵢ + q·ℓ̂_c,ᵢ + T̂ᵢ − T̂min,ᵢ ) / Σᵢ wᵢ·T̂ᵢ
```

- The mapping from r67floor to the gauge:
  - w = `nk` (r67floor: `dlv`, the delivered rate; both are proportional to the leg's echo rate);
  - T̂ = `tp50`, T̂min = `tmin`;
  - σ = the printed `sig` (r67floor: a constant per cell);
  - Fᵢ = the session floor F_c,i (§2: the median LATE `tmin` at the floor pool; r67floor's "truth" row);
  - ℓ̂_c,i = the session `l` at the floor pool (r67floor's `lF`); q = the printed `q`.
- R̃₆₇ is computed per window and taken as the median over LATE windows per row, as for R̃.
- It reads no m̂ and no b̂₀⁺. Its crossing is reported beside R̃'s at every cell.
  - **At c2 it IS the scored reading in R units.** On one leg, with the same F_c, σ and ℓ̂_c, R̃₆₇(W₉₅,r) − 1 = −D_r / T̂. So SHORT means that the honest-floor form crosses below the knee, the B2 event for the R6.7 form.
  - **Scorer witness:** sign(R̃₆₇,r(W₉₅,r) − 1) = −sign(D_r) on every replicate. A mismatch is a scorer bug.
  - **Elsewhere (c8, c1s) it decides nothing.**

**What it tests.**
- R̃ is the law fed floors the flow did not measure itself. R5.4 was withdrawn as an oracle for exactly that.
- So **a B2 fire refutes the target FORM** given queue-free floors, and **a pass says nothing about the law's own
  floor inputs**.
- The own-ℓ̂ R̃ is reported beside it, with no veto. The named outcome is that, if the scored recipe fires and the
  own-ℓ̂ recipe does not, "the form fails on honest floor inputs, and the flow's own ℓ̂ masks it".

**The witness, a per-row gate.**
- `s60replay.py witness`: replayed with the row's **own** `b0p` and `l`, R̃ must reproduce printed `R` and `Rb` on
  every defined window to 6·10⁻⁶. A failing row is VOID for every R reading.
- **Demonstrated** (`s60replay.out`):
  - on the renderer's sample at `d416d59b`: PASS, worst 4.9·10⁻⁶;
  - on the hand-formatted `s60witness_ka.log`: two legs, one anchor-bound, an nk = 0 leg, and a warm-up window to
    cut; PASS;
  - negative control (`tmin` + 1 µs): FAIL on both, as required.
- A renderer-generated sample with an anchor-bound leg and an empty leg is a pre-launch item.

### 1.2 P1 (LEAD): printed b0p rises with W

- **Basis.** The gauge's `b0p` is the maximum brake T̂min,k − m̂k over the row's first K = 8 windows. On p27's window
  minima it tracks the late queue:
  - c2: 2.2 / 4.2 / 12.6 / 23.5 ms at W 128 / 192 / 256 / 384;
  - c8 leg 0: 2.5 / 8.1 / 26.7 ms at 128 / 256 / 512;
  - c1s: 0.30 / 1.45 / 18.8 ms at 256 / 512 / 2 048.
- **Registered:** b0p(arm)/b0p(floor pool) ≥ 3 at c2 W 256, c8 W 512 (leg 0) and c1s W 2 048.
  - **b0p RISES as the pool shrinks.** p27 c2 reads 2.87 / 2.46 / 2.18 ms at W 64 / 96 / 128, because the running
    anchor converges later (R6.7 §2.2). So the floor-pool base is larger than the cap-128 one.
  - Predicted: c2 ≈ 12.6 / 2.87 ≈ **4.4** on a W 64 base, and possibly below 3 on a W 16 base. c8 ≈ 10 and c1s ≈ 60
    on their measured bases, which may also fall.
  - P1 can therefore fail at c2 by its base alone. That is reported as such.
- **Meaning.** "The first K windows" is a valid b̂₀⁺ source only under a boot held at a queue-free store. Otherwise the
  start-up queue is latched into the target, as rev 3's σ̂⁺ was. That makes the boot hold and n₀ terms of the law:
  `store-law.md` R6.7.
- **If P1 fails at a cell,** that is reported: printed b0p did not latch the arm's queue by the ratio rule. **It no
  longer changes the scored quantity.** R̃ is fed the floor-pool calibration regardless, since R6.7 showed the latch
  independently. The old consequence ("printed R becomes the scored R") is withdrawn; it would have displaced R̃ at
  c8.

**P2 (reported, decides nothing).** Printed R stays above 1 until the late queue outgrows twice the 2 s boot queue.
It is predicted to be > 1 at every c8 and c1s arm, and at c2 up to W 384.

### 1.3 P3: printed `l` rises with W

- **The quantity is the printed field.** The gauge's `l` is `WinSampler::life_min`, a running minimum from the first
  sample. Its sampler is attached on the first sender-loop pass under `RWM_DIAG`.
  - §56's proxy is the minimum over ticks of `rtp_us`, which is `p.min_rtt()` = `copa.min_rtt()` (`diag.rs:579`),
    the gauge's `m`.
  - p27's is the minimum of `rtt_min_us`.
  - Both are lifetime minima.
  - **Witness:** `m` = `l` on every window with t < 10 s.
- **Size (arm medians, ms):**
  - c2 §56: 8.07 / 8.87 / 9.66 / 9.52 / 10.21 / 10.13 at W 128 / 160 / 192 / 256 / 384 / 2 048;
  - c8 leg 0 §56: 8.22 / 9.42 / 11.12 / 10.87 / 10.31 / 11.13 at W 160 / 224 / 320 / 512 / 768 / 1 280.
  - The rank trend is significant: s56 c2 +0.58 (p 0.007), c8 +0.46 (p 0.014); p27 c2 +0.70 (p 0.001).
- **But the row scatter of `l` is sampling noise of a minimum** (`s60abs.out`).
  - At c2 the absolute late floor varies by 0.10–0.25 ms across rows of one arm, while `l` varies by 0.55–2.94 ms.
  - corr(`l`, floor − `l`) is −0.96 to −1.00, and c8 on §56 reads the same.
- **Registered:** Δℓ = (arm median `l`) − (floor-pool median `l`) per leg, with SE_Δ from the row sds.
  - **RISES** iff Δℓ − 2·SE_Δ > q·ℓ̂_c; **FLAT** iff Δℓ + 2·SE_Δ < q·ℓ̂_c; otherwise **UNRESOLVED**.
  - Arms: c2 W 256 (predicted +1.3 to +1.5, likely UNRESOLVED against the 1.4 ms row noise of `l`), c8 leg 0 W 512 (+2.7, RISES) and leg 1 W 512 (+3
    against its own 3.2 allowance, FLAT or UNRESOLVED).
  - RISES puts ℓ̂'s source into R6.7. FLAT means the two ℓ̂ recipes coincide there.

---

## 2. Calibration: session-level, frozen now

**Superseded by entry 10 (R6.7, `store-law-r67.md` §1.2):** W 128 is NOT queue-free at c2. c2's late window minimum
reads 9.45 / 10.08 / 11.18 ms at W 64 / 96 / 128, so "calibrate at n₀ = 128" and the b0p-flatness gate are
withdrawn. A brake is a difference, so it stays flat while `tmin` and Copa's minimum rise together.

**The floor pools (entry 10).**
- **Pools below R6.7's smallest measured pool** (its open item 1):
  - c2: W 16 / 32 / 48;
  - c8: W 32 / 64 / 96 (P16 / 32 / 48);
  - c1s: W 64 / 128 / 192.
- **Per leg, the floor pool** is the smallest of these at which the leg has ≥ 80 % of its LATE windows with `nk` ≥ 10.
  c8's leg 1 may need a larger pool than leg 0. Its slope is only 0.008 ms/slot (R6.7 §1.2), so a pool 64 slots
  higher costs it ≤ 0.5 ms, against its 2.5 ms tolerance.
- **The session floor** F_c,i is the median, over the floor pool's valid rows, of each row's median LATE `tmin` (on
  windows with `nk` ≥ 10). That is R6.7's "truth (small-pool late T̂min)" row.
  - ℓ̂_c,i is the median `l` there.
  - b̂₀⁺_c,i is the median `b0p` there, and T_c,i the median of `l` + `b0p`. These are R̃'s inputs.
- **The floor gate** replaces the b0p flatness gate. It is a fit, not an adjacent difference (`s60short.py`, gate
  section):
  - Per leg, b is the least-squares slope of the per-row median LATE `tmin` against W over the three floor pools
    (18 rows).
  - E = b·W₁ is the floor pool's excess over the W → 0 limit, extrapolated linearly.
  - **The adjacent difference is rejected.** It cannot separate flat from slope: at c2, Δ(16 → 32) is 0.32–0.43 ms
    under R6.7's slope, against 2·SE of 0.14–0.36 ms.
- **Reading 1, the sub-floor slope (R6.7 open item 1), two-way:**
  - FLAT iff b + 2·se_b < 0.010 ms/slot (half the low end of R6.7's secant);
  - SLOPED iff b − 2·se_b > 0.010;
  - otherwise UNRESOLVED.
  - se_b is 0.0018–0.0045 ms/slot at c2 and 0.0023–0.0045 at c8 leg 0, against a secant of 0.020–0.027. So
    flat-against-slope **resolves**, marginally at the high-noise end.
- **Reading 2, floor convergence, three-way, with convergence having to be shown:**
  - CONVERGED iff E + 2·se_E ≤ τ;
  - NOT-CONVERGED iff E − 2·se_E > τ;
  - otherwise UNRESOLVED.
  - τ is 0.3 ms at c2 (a third of the smallest predicted shortfall), 1.0 ms on c8 leg 0 and 2.5 ms on c8 leg 1 (the
    R̃ sensitivities, §5).
  - **Predicted:**
    - c2: under the slope E = 0.32–0.43 ± 0.06–0.14, so NOT-CONVERGED or UNRESOLVED; flat gives CONVERGED;
    - c8 leg 0: under a c2-like slope E = 0.64–0.86 ± 0.14–0.29, so CONVERGED or UNRESOLVED.
  - The extrapolated floor F_c − E is **reported** beside F_c, not scored.
- **What anything short of CONVERGED does.** A floor read too high under-reads the rise.
  - **c2:** SHORT stands as a lower bound; **COVERED is admissible only if CONVERGED** (§3.1).
  - **c8's B2:** a floor too high raises T_c and R̃ at the knee, so a B2 fire stands and a pass is weakened.
    Reported.
  - **c1s: the gate does not cover c1s's scored sign.** R̃ at c1s is fed b̂₀⁺_c and the row's own `l`, which a `tmin`
    gate does not constrain. The gate is reported there, and it bears only on R̃₆₇ at c1s, which uses F_c.

**The choice of a session value, and its reason.**
- Per cell and leg, the calibration is **one session value** over the floor pool's valid, non-latched rows, as above
  (§1.1 states what the replay is fed).
  - Leg 1's b0 per row is the maximum of its `b0k` entries with `b0kn` ≥ 10, and needs ≥ 4 such windows of K. A row
    failing that leaves leg 1's session median, but stays in leg 0's.
- The reason is `s60abs.out`: the row scatter of the floors is sampling noise of a minimum, not scatter of the loop.
  A median over rows estimates the floor better than any single row.
- Per-replicate pairing would carry that noise into every replicate's R̃: σ_R ≈ 0.15 at c2 and 0.04 at c8, against
  0.008–0.015 under the session value (`s60cond.out`).
- It also removes the latch-driven scoreability problem. A latched row drops out of the median instead of voiding a
  replicate.

**Drift: not detected, and the check is unpowered** (`s60drift.out`, §56 small pools, value centred per pool, against
replicate order).

| cell / leg | n rows | `l` | boot-queue proxy |
|---|---|---|---|
| c2 | 6 | −0.42 (p 0.47) | +0.42 (p 0.46) |
| c8 leg 0 | 8 | +0.15 (p 0.76) | −0.10 (p 0.86) |
| c8 leg 1 | 8 | −0.20 (p 0.67) | +0.20 (p 0.66) |
| c1s | 12 | +0.49 (p 0.11) | +0.46 (p 0.14) |

§57a also showed a c1s session-position effect. The registered check is in-session.

**In-session drift check and fallback (fixed now).**
- **Check:** per cell and leg, Spearman of the calibration rows' `b0p` and `l` against run order, with a permutation p.
- **Fallback:** if p < 0.05 **and** the fitted change across the session exceeds q·ℓ̂_c, the cell's R̃ switches to
  per-replicate pairing (each replicate with its own calibration row). The session value is then reported as the
  sensitivity, and the cell's conditioning is re-stated from the realised σ_R.

**Pools and bytes.**
- Every floor pool binds (`STORE_CAP_FLOOR` = 10; the ceiling is max(N·pool, floor)).
- A store-limited row delivers ≈ W/T̂. Bytes are sized for the **faster** plausible rate, so that every row clears
  LATE (t ≥ 15 s at c2 and c8, ≥ 5 s at c1s) with ≥ 30 s of LATE windows. The 150 s timeout covers the slower rate.
  - c2: W 16 ≈ 1 700 sym/s (main session), i.e. ≈ 106 sym/s per slot.
  - c8: its leg-0 loop is like c2's, so sized at ≈ 106 per slot. §56's W 160 basis (≈ 50 per slot, u ≈ 0.75) would
    make the rows ≈ 2× longer, still within the timeout.
  - c1s: ≈ 182 sym/s per slot (§56 W 384).

  | pool | c2 | c8 (fast / slow basis) | c1s |
  |---|---|---|---|
  | rate | W 16 / 32 / 48: ≈ 1.7 / 3.4 / 5.1 k sym/s | W 32 / 64 / 96: ≈ 3.4 / 6.8 / 10 k (1.6 / 3.2 / 4.8 k) | W 64 / 128 / 192: ≈ 12 / 23 / 35 k |
  | bytes | 100 / 200 / 300 MB (≈ 48 s) | 200 / 400 / 600 MB (≈ 48 s fast, ≈ 100 s slow) | 1 / 1.5 / 2 GB (≈ 70 / 53 / 46 s) |

  - Every other arm keeps 600 MB (c2, c8) or 5 GB (c1s control).
  - The per-row timeout is RWM_PERF_TIMEOUT_S = 150 s on every floor-pool row. A DNF is a datum, and the row is
    re-run once.
  - The warm-up is a single-chunk object (`perf.rs:292`), unaffected by the pool.
  - **Per-arm bytes are a new harness feature.**
- **c1s's small pools may collapse:** p27's c1s W 192 was a quinn-collapse pool (R6.7 §1.2). The collapse rule and the
  two-row rehearsal (§5) cover it. A cell whose floor pool collapses on every row moves to the next pool, and the
  move is reported.

**Validity and latching:**
- **LATCHED:** b0p − median_LATE(brake) > q·ℓ̂ + ō, at the floor pool. ō is c2 0.90, c8 leg 0 1.8 and leg 1 6.3, c1s
  0.20 ms. These were derived at caps 64–256 (`b0bound.out`) and are untested at the new pools.
- **Minimum:** ≥ 3 valid, non-latched floor-pool rows per cell and leg.
- **The collapse flag does NOT gate calibration rows** (ruling entry 7, item 3).
  - The §56 flag fires when quinn's qcwnd sits below the cell's BDP on > 10 % of windows. The floor pools lie below
    every cell's BDP, so a store-limited flow may hold quinn's window under the BDP by construction. That is the flag's own condition, not collapse: §57b's c1d P192 rows carried a one-leg flag on 4
    of 5.
  - The flag share is reported on calibration rows. The earlier "no qcwnd below the BDP for t < 2 s" rule is
    withdrawn, because it would fail by construction.
- **A calibration row is valid** iff all three hold:
  - (a) its boot is complete: `b0k` holds K = 8 brakes, with leg-0 `b0kn` ≥ 10 on each (leg 1 as above);
  - (b) it is not LATCHED;
  - (c) it is **not COLLAPSED**, by an **absolute** rule **on the fields it was checked on**. COLLAPSED iff more than
    10 % of its LATE `[DIAG]` ticks have excess > X(cell, leg), where:
    - excess = the tick's `rtt=` minus the row's minimum `rtp_us`;
    - `rtt=` is top-level, 0.1 ms, at one leg; per leg, 1 ms, at c8;
    - `rtt=` is `p.estimator.rtt()` = the estimator's **EWMA** RTT (`estimator.rs:523`), not a window mean.
    - `[DIAG]` is on every row, G and C, so the rule needs no gauge field.
  - **Why absolute:** a relative rule (against the same-pool median) fails when the median row is itself collapsed,
    and §56's relative flag came out inverted in rehearsal 1.
  - **X is derived from each cell's own queue-free tick spread** (`s60collapse56.py`, §56's small pools,
    collapse-flagged rows excluded): **X = E90 + 2·(E90 − E50)**, the 90th percentile plus twice its upper half-spread.

    | cell / leg | E50 | E90 | E99 | X (ms) |
    |---|---|---|---|---|
    | c1s leg 0 | 1.88 | 3.28 | 6.48 | **6.1** |
    | c2 leg 0 | 7.63 | 10.00 | 11.80 | **14.7** |
    | c8 leg 0 | 8.76 | 11.76 | 13.86 | **17.8** |
    | c8 leg 1 | 14.00 | 17.41 | 21.50 | **24.2** |

  - **Checks.**
    - **c1s, out of sample, on §57a** (`s60collapse.py`, 5 GB rows at pools 448–512): at 6.1 ms the rule flags 20 of
      the 23 registered-collapse rows and 0 of the 40 unflagged.
      - The three missed rows (B64-448 s42 r5 and r19, B64-512 s7 r4) are partial collapses with ≤ 14 % of ticks above 5 ms.
      - For comparison, 5 ms flags 22 / 1 and 2 ms flags every row.
    - **c2 and c8, in sample only.** On §56's small-pool rows the rule flags only the known c1s collapse row (P384
      s42 r1). The c8 P112 leg-1 row that sat at 0.102 under the previous margin is no longer flagged.
      - **Its sensitivity at c2 and c8 is untested,** because no collapse rows exist there.
    - c1s's 5 ms could not carry over: it flags every queue-free c2 and c8 row (82–100 % of ticks above 5 ms),
      because their no-queue echo spread is 7–10 ms.
    - **At the new floor pools X is lenient.** It was derived on §56 W 128–384 pools, which R6.7 shows are partially
      loaded. The normal excess at W 16–64 is smaller, so X there flags fewer valid rows but may miss mild
      collapses. The in-session excess E50 / E90 at each floor pool is reported beside X.
  - **Leg 1:** the row's COLLAPSED state is read on leg 0. A leg-1 flag removes the row from **leg 1's** session
    medians only, and is reported.
  - **Per K:** b0p(K) = max(first K `b0k` brakes), and LATCHED(K) uses b0p(K) in the latch rule, so the valid set is
    recomputed for each K.
- **B1** is evaluated only on valid rows, so a collapse's queue cannot pose as `err` > 0.
- **c1s calibration rows run 1 GB.**

**B1** fires iff a calibration row's median LATE `err` > 0. Predicted not to fire (`b0bound.out` margins +0.43 / +0.46 /
+0.19 ms). When B1 does not fire, R − 1 ≥ σ/T̂ > 0 on that row (anc 0, single leg), so the calibration-end sign holds by
construction.

---

## 3. What each cell can decide

The conditioning arithmetic (`s60cond.py`):
- dR̃/dW = −(w₀/Σw)·(dT̂min₀/dW)/T̄, and an input scatter s moves R̃ by s/T̄.
- B2 resolves a miss of 2σ_R/|dR̃/dW| slots, and fires iff the crossing lies below knee − 2σ_R/|dR̃/dW|.

**The anchor decides where this arithmetic applies.**
- Under session calibration, F = min(m̂_row, ℓ̂_c + b̂₀⁺_c) + b̂₀⁺_c.
- §56's late m̂ (the gauge's `m`) is tight across rows (sd 0.2–0.6 ms at c2, 0.25–1.24 ms at c8 W 224–320). Against
  the session threshold it reads (`s60abs.out`):
  - **c2:** anc 0 at W 128 (3 of 3 rows) and 160 (2 of 3); anc 1 from 192.
  - **c8:** anc 0 at W 160 (4 of 4) and 224 (1 of 4); anc 1 from 320.
- **On anc 0 arms** the row's m̂ enters F, and R̃ − 1 ≥ σ/T̂ > 0 whenever the late brake T̂min − m̂ is ≤ b̂₀⁺_c + q·ℓ̂_c.
  So R̃ cannot reach 1 below the **anchor flip**.
- **The slope and σ_R figures below hold on anc 1 arms only.**

| cell | slope \|dR̃/dW\| on anc 1 arms (per slot) | σ_R, per-replicate calibration | σ_R, session calibration | B2 fires iff R̃(knee) < 1 − 2σ_R, i.e. the crossing lies below |
|---|---|---|---|---|
| c2 (knee 169) | 0.0012–0.0026 | 0.149 (114–256 slots) | 0.008–0.015 | **not scored** (entries 6 and 10). The flip sits at the knee at K = 5–8 (R̃ ≈ R̃_flip > 1 there). c2's scored reading is the allowance shortfall (§3.1) |
| c8 (knee 340, anchor flip ≈ 224–320) | 0.0017–0.0020 | 0.039 (39–47 slots) | 0.008–0.015 (8–19 slots) | per-replicate 293–301; **session 321–332**, i.e. ≥ 101–112 slots below the band's low edge (433) |

The scatter inputs are each measured and cited in `s60cond.py`:
- the absolute late floor's row sd: 0.10–0.25 ms at c2, 0.25–0.50 at c8 (§56; p27's 9 s c8 rows do not settle);
- σ at load: ≈ 0.1 ms;
- for the per-replicate case, also the calibration row's `l` (sd up to 1.4 ms) and `b0p` (sd ≈ 0.9, entering twice at
  anc 1).

### 3.1 c2: the allowance shortfall at the knee (SCORED; entry 10, replacing the per-K flip)

**The reading.** It comes from R6.7 §5: with honest floors, the form's allowance σ + q·ℓ̂ has no demonstrated margin
at c2. Per replicate r, on leg 0, from printed `[SLAW]` fields (`tmin`, `sig`, `l`):

```text
   D_r  =  ( T̂min_r(W₉₅,r) − F_c )  −  ( σ_r + q·ℓ̂_c )          q = 0.10 (Bulk)
```

- T̂min_r(W₉₅,r) is the replicate's per-row median LATE `tmin`, interpolated in W at its own goodput knee W₉₅,r (each
  row against the same replicate's control).
- F_c is the session floor (§2).
- σ_r is the median LATE `sig` over the knee-bracketing rows.
- ℓ̂_c is the session `l` at the floor pool.
- **The sentence:** "the window-minimum rise the knee needs, less what the allowance supplies". D > 0 means the
  allowance does not reach the knee.
- **Shape:** ms; order 0 in N; it rises with the floor's honesty, since a lower F raises D.

**Scoring.** The pooled D is the median over replicates of D_r. Its 90 % interval is a bootstrap (2 000, seed 60) over
replicates and floor-pool rows.
- **SHORT** iff the interval > 0.
- **COVERED** iff < 0, meaning the allowance covers the knee. It is admissible only if the floor gate read CONVERGED
  (§2), because a floor read too high biases D toward COVERED.
- **UNRESOLVED** otherwise.
- **Bracket:** `tmin` is convex across the knee, so a chord between the bracketing arms over-reads toward SHORT. D is
  therefore also reported at the arm just below the knee (160) and at the arm just above (176 or 192), as a bracket.
  SHORT is read on the bracket's lower arm as well, and is reported as robust if both arms agree.

**Prediction and resolution, computed first** (`s60short.py`).
- **Prediction:** D ≈ **+0.9 ms** at the goodput knee (169) with the floor read at W 64 (R6.7's 9.45 ms, BELOW = flat).
  It is ≈ **+1.9 to +2.2** if the floor follows the measured secant (0.020–0.027 ms/slot) down to W 16.
  - The allowance at q = 0.10 is ≈ 2.6 ms. R6.7's 2.3–2.6 spans q = 0.05–0.10, so +1.7 applies only at the delivery
    knee (181).
- **Resolution:** pooled se 0.13–0.25 ms, so a 90 % half-width of 0.22–0.42 ms.
  - The terms are: the absolute late `tmin` row sd (0.10–0.25 ms); the knee position, 3 slots × the slope at the knee
    (p27 0.043, §56 0.094 ms/slot); `sig`'s row sd; the floor's session se; and q·ℓ̂_c's.
  - The interval's lower edge is predicted at **+0.48 to +0.68 ms**: **SHORT, resolved**.
  - COVERED would need a pooled D ≤ −0.22 to −0.42 ms.
- **It can come out two ways.** SHORT is what R6.7 predicts. COVERED would mean the partial-load queue at the knee is
  within the allowance, against R6.7's reading of p27. Either goes to R6.7 open item 2 (the allowance form) and to the
  law ledger.

**Reported, not scored.**
- The **brakes**: each floor-pool row's `b0k` (K = 1..8) and `b0kn`.
- The **per-K flip readout** of entry 7–9: W_flip(K) − W₉₅, where the replicate's late `m` crosses T_c(K). This measures
  b̂₀⁺(K), and is expected UNRESOLVED.
  - Its prediction (`s60flip.out`) is kept as context: −41 slots at K ≤ 4; −27 / +4 / +26 at K = 8.
  - The interval analysis (threshold-dominated, with no cancellation of `l`'s noise: `s60lmin.out`, 4 of 14 §56 rows
    reach `l` at boot) stands as stated in entries 8–9.
- R̃ − 1 at the knee arms, R̃₆₇ beside it, and each replicate's crossing W_r against the cap-128-based band
  (Appendix A).

**Pools:**
- 16 / 32 / 48 (floor pools, at 100 / 200 / 300 MB);
- 160 / 176 / 192 (the knee bracket);
- 256 (P1 and P3, the slope above the knee);
- 2 048 (control).
- The W 96 / 128 calibration rows are withdrawn.

### 3.2 c8: B2, live as a gross-miss detector

- **B2:** replicate r counts iff R̃_r(W₉₅,r) < 1 − 2·σ_R. Here W₉₅,r is interpolated from goodput against the same
  replicate's control, and σ_R is the in-session sd over replicates at the bracketing arms.
- **It fires on ≥ 2 of the 5–6 scoreable replicates,** on the scored recipe alone.
- **It is read only where the replay's anc bind fraction is ≥ 0.9 at both bracketing arms.** If the in-session knee
  falls below 320, the 256 arm may still be anc 0, and that replicate is then reported, not counted.
- **It can fire.** Leg 0's window minimum climbs 0.063–0.076 ms per slot between the knee and the band. B2 fires for a
  crossing below ≈ 321–332 under session calibration, which is ≥ 101–112 slots below the band's low edge (433).
- **The direction of the floor-pool change.** Floors read below W 128 lower T_c, and so the target, which moves the
  crossing **down, toward the knee**.
  - Rough size: with a c2-like slope of ≈ 0.02 ms/slot on leg 0 over 128 → 32, T_c falls ≈ 1.9 ms. R̃ at the knee
    falls ≈ 0.87·1.9/33 ≈ 0.05, and the crossing moves ≈ 25–30 slots lower, to ≈ 405.
  - That is still ≈ 65 slots above the knee, so B2 remains a gross-miss detector.
  - The firing threshold, set relative to the in-session knee, is unchanged.
- **The band (reported context; cap-128-based):** 433–586, anc 1, from single-row corners, beside R6.7's honest-floor crossing 440–446. A session value narrows it. The own-ℓ̂
  sensitivity is 462–605.
- **Pools:**
  - 32 / 64 / 96 (P16 / 32 / 48; floor pools, at 200 / 400 / 600 MB);
  - 256 / 320 / 384 (the knee at ≈ 340 and R̃ at it);
  - 512 (P1, P3);
  - 1 280 (control).
  - The band-scoring arms 448 / 576 / 640 are cut, because the band is no longer scored.
- **Scoreability:** with one calibration statistic, a replicate needs only its 320 / 384 / control rows valid.
  P(≥ 5 of 6) ≈ 0.90 at 0.97 per row.

### 3.3 c1s: the control sign, kept because it discriminates the two T̂min readings

Verdicts at W 2 048, by sign test on R̃_row − 1 over scoreable replicates (n_s ~ Binomial(12, 0.67)): PINNED,
CROSSES or SPLIT (`s60cond.out`, per reading, no averaged prior).

| reading | μ = E[R̃ − 1] | PINNED | CROSSES | SPLIT |
|---|---|---|---|---|
| prefix minima (p27/p34) | −0.20 | 0.00 | 0.91–0.94 | 0.00–0.02 |
| midway | −0.10 | 0.00 | 0.53–0.94 | 0.00–0.40 |
| exact Copa lift (§56) | ≈ 0 | 0.01 | 0.01 | 0.92 |
| pinned side | +0.05 | 0.15–0.75 | 0.00 | 0.19–0.78 |

UNDERPOWERED is 0.06 throughout.

- **Meaning.**
  - CROSSES means the law lands inside the pool range, and favours the prefix-minimum reading.
  - SPLIT means R̃ ≈ 1 at the control, and favours the exact-minimum reading.
  - PINNED means the law would grow the store to its clamp. That is a FORMULA-FIRST PINNED finding, and needs a
    law-ledger verdict (`law-ledger.md` F01).
  - All three go to R6.7.
- **Marginal cost over the G/C pairs:** 2 extra control G rows. The 18 floor-pool rows are needed anyway for c1s's
  P1, B1 and convergence gate.
- **Pools:** 64 / 128 / 192 (1 / 1.5 / 2 GB; floor pools), and 2 048 (5 GB; 12 G rows, 10 of them paired with C).

---

## 4. G/C: gross-error check

- **Rows:** G = `RWM_SLAW=1`, C = `RWM_SLAW=0`, on one binary, with `RWM_DIAG=1` on both. Every gauge site is gated by
  `slaw`, including `note_fec_inputs` (`emit_source.rs:535`).
- **Predicted cost:** 0.22 % of a core on the coordinator's bench figures, about 0.12 % of c1s client CPU per GB.
  Printing is unbenched; `[CAPLOG]` can reach ≈ 200 lines/s, and its rate is a smoke readout. Registered prediction:
  within +1 %.
- **10 interleaved pairs at c1s W 2 048:** PERTURBING iff the median log(G/C) of CPU s/GB exceeds +10 %.
  - Below 4 unflagged pairs the check is UNRESOLVED. P(≥ 4) = 0.83 / 0.73 / 0.27 at collapse 0.29 / 0.33 / 0.48.
  - Goodput is reported with its minimum detectable difference.
  - C is re-based against §56's c1s control.

## 5. Witnesses

- **`RWM_SLAW`:** scrubbed with `-u RWM_SLAW -u RWM_THREE_TERM`, set per arm, and echoed in `[GATES]` on both ends. A
  mismatch voids the row, which is re-run once.
- **Smokes:**
  - launch 1: c2 W 192, one C row and one G row.
    - C carries no gauge tags.
    - G carries all tags, the witness PASS, the CAPLOG replay PASS, and `kw` / `b0k` after the warm-up cut.
  - launch 2: one c1s W 64 G row (1 GB) and one W 2 048 C row.
- **Rehearsal checks on the floor pools** (entry 10; the floor pools are unmeasured at all three cells). All rows are
  G:
  - one c2 W 16 row (sized as in §2);
  - two c8 W 32 rows, plus **one c8 W 96 row**, the pool leg 1 is expected to calibrate on;
  - two c1s W 64 rows.
  - **The coordinator named W 128 rows for the witness check; it moves to these floor-pool rows,** because W 128 is no
    longer a calibration pool.
  - **Witness check (UNVERIFIED until here).** On every rehearsal G row, per leg, read:
    - `b0k[0]` (expected 0);
    - the first windows' `m` against `l` (expected equal while t < 10 s);
    - **whether c8 leg 1 gets a sampler attached**, read on the W 96 row.
      - `render_window` prints a `[SLAW] … nk=0` line for a leg whether or not a sampler exists, so an all-zero leg is
        ambiguous.
      - The test is therefore: `[DIAG]`'s leg-1 echo count (`sig_us=…/n`, cumulative) grows while `[SLAW]` leg 1 reads
        `nk=0` on every window ⇒ **unattached**. Both growing ⇒ attached. Neither ⇒ starved, which is not a sampler
        fault.
    - **Basis.** `record_rtt_sample` (`path.rs:606`) feeds the leg's `WinSampler` (`:616`) and Copa (`:619`) the same
      sample. The sampler is attached by `Scheduler::enable_slaw` from `diag::report`'s first pass (`diag.rs:449`),
      so a sample before that pass would sit in Copa's minimum only.
    - The host `slaw_reachability` run (main session, `d416d59b`: 1/1 pass) asserts its lines internally and prints
      none, so this rehearsal is the first engine reading.
    - **A failing row is reported, not voided.** Its b0p over-reads by the offset, the latch rule sees it, and K = 1
      becomes a finding.
    - An **unattached** c8 leg 1 leaves c8 without a leg-1 floor, so B2 is not scored and the cause is diagnosed.
    - A **starved** leg 1 at 32 / 64 / 96 takes its floor pool from the smallest arm where it qualifies (256 at the
      latest: 0.008 ms/slot × 160 slots ≈ 1.3 ms, within its 2.5 ms τ). If it qualifies nowhere, B2 is not scored.
  - **Go/no-go per cell**, GO iff at least one of the cell's floor-pool rehearsal rows passes all of these:
    - (a) the row completes within its 150 s timeout (no DNF);
    - (b) it is not COLLAPSED (§2; X is lenient at these pools);
    - (c) leg 0 has ≥ 80 % of its LATE windows with `nk` ≥ 10;
    - (d) `b0k` holds 8 brakes with leg-0 `b0kn` ≥ 10;
    - (e) it is not LATCHED.
  - **The absolute b0p limits of entry 8** (c1s ≤ 0.6 ms, c8 leg 0 ≤ 3.9 ms) were derived on cap-256 / cap-128 bases.
    b0p rises as the pool shrinks (§1.2), so a valid floor-pool row may exceed them. They are now **reported, not
    gating**. R̃'s sensitivity to the session b̂₀⁺_c is reported instead: 2/T̂ per ms at c1s, and 2·w₀/T̂ at c8.
  - Two rows each at c8 and c1s bound the chance NO-GO: at c8's ≈ 25 % latch rate it is ≈ 6 %, and at c1s's collapse
    share 0.29–0.48 it is ≈ 8–23 %. c2's single row checks timing and samples.
  - **NO-GO:**
    - c2: its floor pool moves to W 32, and the move is reported.
    - c8: B2 is not scored. c8's rows still run, for P1, P3, the gate and the reported R̃ / R̃₆₇.
    - c1s: the control sign is dropped. 60b then runs only the G/C pairs and the evicting rows.
  - The decisions and their numbers are recorded in the launch log.


- **`[SLAW] q`:** 0.100000 on Bulk G rows, and 0.050000 on Realtime rows.
- **`[CAPLOG]`:** every logged cap is reproduced by `caplog_replay.py` (seat `plain`, branch `pooled`/`boot`; `tt`
  is impossible), and the bind fraction is reported.
- **R-sign at c2 and c8 controls:** R̃ < 1, read after replicate 2. A failure stops the session and is amended.
- **Bit-identity:** the gauge's host cap-sequence test and goldens pass on `d416d59b`.
- **Host:** host cargo is paused. Arm order rotates per replicate, and seeds alternate 42 / 7.

## 6. The evicting rows (plan 59) on the gauge

- **Rows:** c2-100, 600 MB, Realtime, evicting seat, n = 5 (each VOID slot re-run once), `RWM_SLAW=1`.
- **m̄** = Δ`cum` ack / Σ `adv` count over LATE (t ≥ 5 s). Witness: `mbar_lo` ≤ m̄.
- **W1** per window = `dbt_share`; the row's W1 is Σ`dbt_em`/Σ`dbt_acc`. Witness: it lies inside plan 59's bracket.
- **K4 on the existing token**, `[WSTORE] fec=eps/sigma2/r/astar/n`. These are window means of the rate seat's inputs
  at each debt accrual; n is the accrual count.
  - **Δ is not in the token.** It is read from the `[SPAN] delta=` lines whose t falls in the window.
  - **Steady window:** n ≥ 1, and every `[SPAN]` line in that window and both neighbours reads one `a_star` and one
    `delta`. The token's astar mean then equals that integer, as a witness.
  - K4 compares the oracle's W1 at (eps, sigma2, r, astar, Δ, m̄) with that window's `dbt_share`.
  - A row needs ≥ 30 steady LATE windows, or it is K4-UNSCOREABLE.

## 7. Rows and launches

**Suites pre-flight (ruling entry 7, item 1).**
- **The problem.** The gauge tree's engine sources differ from `tools/l1/ref56-manifest.sha256`, so every launch would
  miss suite reuse and run the full engine suites (≈ 1 h). That does not fit in 60b. A rehearsal skips them
  altogether.
- **60-pre** (no battery, no netns; as §58 Part A): `tools/l1/suites_run.sh` on the **pinned tree**: `harness60`
  with `slaw-gauge` `d416d59b` merged, recorded by sha.
  - It runs release, doc, wasm (`env -u GOLDEN_CAPTURE`), debug, core and parsers, with the flake rule.
  - ≈ 50–60 min. Both locks are held by the runner, and host cargo is paused.
  - **Plus the harness binary.** `suites_run.sh` builds no harness binary, so 60-pre also runs
    `tools/l1/binid_run.sh`'s crate-directory build (`cd src/raptorpath && CARGO_BUILD_JOBS=4 cargo build --release
    --bin raptorpath`, fresh target, the same path as the harness; §58 Part B) on the pinned tree.
    - That build's sha256 is **the registered `bin_sha`**. The rehearsal, 60a and 60b must each log the same
      `bin_sha`, or the launch stops.
    - This adds ≈ 15 min to 60-pre.
- **On TESTS-OK:**
  - generate `bash tools/l1/ref_manifest.sh <pinned sha> > tools/l1/ref60-manifest.sha256`;
  - commit it on the pinned tree, as a tools-only commit that leaves the engine manifest unchanged;
  - pass it to both launches as `LD_REF_MANIFEST`.
- **Each launch must log `LD-ALL suite-reuse HIT`.** A MISS stops the launch.
- **Harness work:** `harness_paths.py` admits reuse-hit only for plans 56 / 57a / 57b. Plan 60 must be added.
- **Any REAL failure in 60-pre** blocks the session and is diagnosed (§58's rules).

**The rehearsal** (`LD_REHEARSE=1`, which runs no suites) runs both smokes, plus one c2 W 16, two c8 W 32, one c8 W 96 and two c1s
W 64 floor-pool rows for §5's witness check and go/no-go. That is 10 rows. Its release binary must equal 60-pre's
`bin_sha`.

**Row times.**
- Floor-pool rows are sized for ≈ 48 s at the faster rate (§2): ≈ 65–75 s typical with overhead, up to ≈ 115 s at c8's slow basis, and 150 s worst (the timeout).
- c2 / c8 600 MB rows take 75–90 s.
- c1s floor rows (1 / 1.5 / 2 GB) take ≈ 46–70 s (150 s worst); 5 GB rows take 90 s (210 s worst, §57a's convention).
- Evicting rows take ≤ 150 s + 40 s.

| launch | block | rows | typical | worst |
|---|---|---|---|---|
| **60-pre** | engine suites on the pinned tree, the crate-directory release build (the registered `bin_sha`), then the reference manifest | — | 65–75 min | 85 min |
| **60a** | release build (suite reuse HIT) | — | 6 min | 10 min |
| | smoke (c2 W 192, C + G) | 2 | 3 min | 3 min |
| | c2: 16, 32, 48 (floor), 160, 176, 192, 256, 2 048 × 6 | 48 | 60 min | 90 min |
| | c8: 32, 64, 96 (floor), 256, 320, 384, 512, 1 280 × 6 | 48 | 60 min | 90 min |
| | **total** | **98** | **≈ 2.2 h** | **≈ 3.2 h** |
| **60b** | release build, or 60a's binary reused | — | 0–6 min | 10 min |
| | smoke (c1s W 64 G 1 GB, W 2 048 C) | 2 | 3 min | 7 min |
| | c1s: 64, 128, 192 (1 / 1.5 / 2 GB) × 6; 2 048 G × 12; C × 10 | 40 | 54 min | 122 min |
| | evicting rows (plan 59) | 5 (+ ≤ 5) | 12 min | 32 min |
| | **total** | **47 (+ ≤ 5)** | **≈ 1.25 h** | **≈ 2.9 h** |

- **Total:** 145 rows (+ ≤ 5) in 60a and 60b, plus the 10 rehearsal rows and the 60-pre suites. Before entry 10 it
  was 127.
  - The withdrawn W 96 / 128 calibration rows are replaced by 54 floor-pool rows (18 per cell).
  - The c2 B2 / flip arms were already cut.
- **Why two launches:** together they exceed 4 h worst case (6.1 h).
- **`LD_HARD`:** launch + 4 h (60a) and + 4 h (60b). The budget guard refuses with less than 3.2 / 2.9 h left.
  60-pre holds the locks through `suites_run.sh`, and `LD_HARD` does not apply to it (as §58).

- **Order:** 60-pre, then the rehearsal (the floor-pool witness check and go/no-go), then 60a, then 60b. 60b runs after 60a's smoke, witnesses, R-sign gate and floor convergence gates are read. It reuses 60a's
  binary (sha recorded), in which case its build row is 0 min.
- **What was cut:** the c2 B2 grid; c8's band arms (448, 576, 640); the band IN/OUT scoring; the second and third
  per-replicate calibration rows; and the c2/c8 C rows.

## 8. Open

1. **s60 measures R6.7's open item 1** (the sub-floor map) through the floor pools and their convergence gate (§2). If
   c2's gate reads NOT-CONVERGED, the floor at n₀ = 10 stays unknown, SHORT is a lower bound, and COVERED is
   inadmissible.
2. **R6.7's open item 2, the allowance form,** is what c2's scored shortfall (§3.1) measures.
3. **Pre-launch:**
   - a renderer-generated witness sample (an anchor-bound leg plus an empty leg);
   - the bit-identity tests on `d416d59b`;
   - `harness60`:
     - arm tables with **per-arm bytes and timeouts**;
     - session calibration at the floor pools, with the convergence, drift and latch rules;
     - the per-row witness, the R-sign gate, the shortfall D and R̃₆₇ in the scorer;
     - the CAPLOG replay;
     - the rehearsal witness check and go/no-go;
     - plan 59's rows.
     - It needs stub tests and a rehearsal.
4. **The floor pools are unmeasured at every cell.** A NO-GO or a missing leg-1 sampler removes that cell's scored
   reading (c8: B2; c1s: the control sign).
5. **No new engine print is needed** (entry 10, item 4). Every reading uses fields the frozen gauge `d416d59b`
   already prints:
   - `[SLAW]`: `n`, `nk`, `tmin`, `tp50`, `tmean`, `m`, `l`, `b0p`, `b0k`, `b0kn`, `sig`, `anc`, `err`, `R`, `Rb` and
     `q` (checked against `render_window`'s format string);
   - `[WSTORE]`: `adv`, `dbt_*` and `fec=`;
   - `[DIAG]`: `rtt=`, `rtp_us`, `cum`, `wait n`, `win` and `src`.

## 9. For the harness brief (registered requirements)

1. **"A suite-reuse MISS stops the launch" needs a stub case.**
   - `test_harness_paths.py`, or a new `test_harness60.py`, drives plan 60 with a manifest that differs by one line
     and asserts the abort token (e.g. `ABORT-SUITES plan 60 suite-reuse MISS`) before any row.
   - It also asserts that a HIT proceeds.
   - Plan 60 must be added to `harness_paths.py`'s reuse-hit plans.
2. **One archive for every stage.**
   - 60-pre runs from one `git archive` of the pinned tree. The rehearsal, 60a and 60b run from one `git archive` of the pinned tree plus its `ref60-manifest.sha256` commit. Both are recorded by sha.
   - The release binary's `bin_sha` must be **equal** in 60-pre (its `binid_run.sh` build, the registered value), the rehearsal, 60a and 60b.
     An unequal `bin_sha` stops the launch. The `ref60-manifest.sha256` commit touches only `tools/l1`, so the archive after it builds the same binary, and the equal `bin_sha` is the witness of that.
   - `ref60-manifest.sha256` is generated from that archive's tree.
3. **What a 60-pre failure means.**
   - **In the gauge's own tests** (`slaw_tests`, `slaw_reachability`, the bit-identity host test, `slaw_parse`,
     `caplog_replay`): it blocks the session, and the gauge is fixed.
   - **Outside the gauge's tests:** this is the **first full engine-suite run since the math-crate merges**
     (`endspeed-relaid`, phase 6's span forms, the shared-core steps) and the later main merges. Such a failure is
     a finding about main, not about the gauge.
     - Rule, as §58 Part A: re-run the failing suite on the base tree (main at the pinned tree's merge base, without
       the gauge).
     - A failure that is identical on base is recorded as main's defect. The session waits for its diagnosis, or for
       the owner's ruling to proceed.
     - A failure only with the gauge blocks, as a gauge defect.
   - Flakes follow §8/§11/§12's rule.

---

## Appendix A. Settled items carried from earlier entries (reported context; nothing here is scored)

Everything in this appendix was computed on cap-128 (c1s: cap-256) calibration values from p27 / §56, before
entry 10. It is kept as context, beside R6.7's honest-floor crossings:

| cell | crossing (slots) |
|---|---|
| c2 | 148–164 |
| c8 | 440–446 |
| c1s | 651–685 |

### A.1 The stand-in for the window minimum

The KA crossings (194.2 / 478.2 / 495.4 at q 0.10) read T̂min(W) off `s56cls.py`'s `CURVE`. That curve is §56's
arm-median **T̄ − T₀**: T̄ is the rate-weighted LATE echo loop from integer-ms tick values, and T₀ is its per-leg floor
on the smallest-W rows. It is a central statistic standing in for a minimum. The KA fed the ledger over-read
(0.41 / 1.0 / 0.12 ms) as b0.

The window spread grows with W, which breaks the substitution (`s60p27.out`):
- c2: 4.8 → 6.4 ms;
- c8: 5.5 → 10.2 ms;
- c1s: 1.1 → 5.8 ms.

### A.2 The anchor shift

With m̂ ≥ ℓ̂ + b̂₀⁺ the target gains a full b̂₀⁺. Method B, calibration ℓ̂ (`s60band.out`):
- c2: 158–203 → 188–242 (+30 to +39 slots);
- c8: 403–505 → 433–551 (+30 to +46).

### A.3 The bands

From single-row corners (`s60band.out`, "FROZEN"). A session calibration value narrows them.

| cell | band | own-ℓ̂ sensitivity |
|---|---|---|
| c2 | 158–242 (both anchor states) | 182–258 |
| c8 | 433–586 (anc 1) | 462–605 |
| c1s | 406–981 as corners; the top is open (prefix minima) | — |

### A.4 What a crossing outside the band means (reported W_r)

- **c2 below 158:** the window minimum rises faster than both readings.
- **c2 above 242:** the form over-allows. That is a queue cost without goodput loss, against R5.5's 17–33-slot
  margin.
- **c8 below 433:** the coupling ½Δ or leg 1's pull is smaller than measured.
- **c8 above 586:** leg 1's σ at load grows faster than §56 read (B6's question), read from the leg-1 pull readout.

### A.5 B6 on §56

- c8 leg 1's σ at load over its boot value is 0.24–0.72 at every pool, so B6 as registered cannot fire. The boot
  maximum carries the fill transient.
- Against the calibration arm's load σ (4.92 ms), it is already 3.1× / 5.3× / 6.4× at W 512 / 768 / 1 280.
- So §56 has answered B6. It is dropped, and kept as a readout (leg-1 `sig` per arm, and leg 1's pull on R̃).

### A.6 The 5 % G/C gate, sized and not run

`s60size.out`, t quantiles, a pair counting only with both rows unflagged:
- CPU s/GB needs 7–8 unflagged pairs, i.e. 14–30 launched at collapse 0.29–0.48.
- Goodput needs 13, i.e. 26–48 launched.

It is not run because the benched gauge cost predicts ≈ 0.1 % of CPU/GB. The 10-pair gross-error check (§4) rules out
more than 10 %.
