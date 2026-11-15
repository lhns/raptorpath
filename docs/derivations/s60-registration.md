# s60: the store law's gauge session (B-1), pre-registration

**For the owner, in plain words.**
- On a static store pool, the store fills within half a second. So the law reads its two floors, b̂₀⁺ (the boot brake)
  and ℓ̂ (the lifetime echo minimum), from windows that already carry queue.
- That is what p27 and §56 predict (P1, P3 below). The gauge has not yet shown it.
- So this session cannot test the law itself. It tests the law's target FORM with floors supplied from queue-free
  calibration rows, decisively only at c8 (B2), and measures whether the floors are contaminated (P1, P3).
- The next step is a derivation, not a sweep: the boot hold and n₀ (`store-law.md` R6.7). A large sweep belongs after
  it.

**c2: B2 is unscoreable under either calibration, and the reason is the form's own anchor term, not noise.**
- **Per-replicate calibration:** the row scatter of `l` (sampling noise of a minimum, `s60abs.out`) makes the
  resolvable miss 114–256 slots, which is below the calibration arm.
- **Session-level calibration** removes that noise. But the row's Copa minimum m̂ then decides the anchor: F = min(m̂,
  ℓ̂_c + b̂₀⁺_c) + b̂₀⁺_c.
  - On §56 the late m̂ is tight (row sd 0.20–0.58 ms). It sits **below** ℓ̂_c + b̂₀⁺_c ≈ 10.25 ms (anc 0) at W 128
    (3 of 3 rows) and W 160 (2 of 3), and above it from W 192 on (`s60abs.out`).
  - In the anc 0 zone, R̃ − 1 ≥ σ/T̂ > 0 whenever the late brake is ≤ b̂₀⁺_c + q·ℓ̂_c. So R̃ cannot reach 1 below the
    W where m̂ lifts b̂₀⁺ above the floor. That **anchor flip** lies at ≈ 160–192, around the knee (169), and R̃ just
    past it is still ≈ 1.1–1.2.
- **So at c2 the form needs a standing 10 s minimum queue of ≈ b̂₀⁺ (≈ 2.2–2.7 ms) before it can cross.** c2's
  knee queue is ≈ 0.8 ms.
  - Whether the flip lands above the knee depends on K, the number of boot windows b̂₀⁺ is maxed over (`s60flip.out`).
  - At the gauge's K = 8 the pooled readout (the median over replicates of flip − knee) is predicted at −27 / +4 / +26
    slots (10 / 50 / 90 %). UNRESOLVED is the expected label: the interval is 26–53 slots wide, against ≈ 4.
  - At K ≤ 4 the flip falls at or below the calibration arm.
- As ruled, B2 is **not scored**. It is replaced by a **pooled flip − knee readout for each K** (§3.1), with
  conditioning as the fallback.
- This revision's first push had claimed a session-calibrated c2 B2 at 6–26 slots. That claim is withdrawn: it left
  out the row's m̂.

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
  - `b0` = **b̂₀⁺_c**, the session median over valid W 128 rows of the per-row `b0p` (max of the first K `b0k`
    brakes);
  - at c2 and c8, `l` = **T_c − b̂₀⁺_c**, where **T_c** is the session median over the same rows of the per-row sum
    `l` + `b0p`. The anchor test m̂ ≥ `l` + `b0` is then exactly m̂ ≥ T_c, the threshold the flip readout (§3.1)
    uses, and F = T_c + b̂₀⁺_c where the anchor binds;
  - at c1s, `l` = the row's own (flat there: `s60ell.out` Spearman +0.03, p 0.43), with `b0` = b̂₀⁺_c;
  - every other input is the row's own: `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `sig` (σ at load) and `q`.
- **At c8** this holds for both legs. B2's firing region (321–332) was computed on separate medians. A median of
  sums and a sum of medians differ by less than one row's noise (p27 cap 128, two rows), so the region is unchanged
  within its corners. It is recomputed in-session on the fed values.
- Per row, R̃_row is the median over LATE windows (t ≥ 15 s at c2 and c8, ≥ 5 s at c1s). `Rb` is not scored.

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
- **Registered:** b0p(arm)/b0p(calibration) ≥ 3 at c2 W 256, c8 W 512 (leg 0) and c1s W 2 048. Predicted ≈ 5.7 / ≈ 10 /
  ≈ 60.
- **Meaning.** "The first K windows" is a valid b̂₀⁺ source only under a boot held at a queue-free store. Otherwise the
  start-up queue is latched into the target, as rev 3's σ̂⁺ was. That makes the boot hold and n₀ terms of the law:
  `store-law.md` R6.7.
- **If P1 fails at a cell,** printed R becomes that cell's scored R.

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
- **Registered:** Δℓ = (arm median `l`) − (calibration median `l`) per leg, with SE_Δ from the row sds.
  - **RISES** iff Δℓ − 2·SE_Δ > q·ℓ̂_c; **FLAT** iff Δℓ + 2·SE_Δ < q·ℓ̂_c; otherwise **UNRESOLVED**.
  - Arms: c2 W 256 (predicted +1.3 to +1.5, likely UNRESOLVED against the 1.4 ms row noise of `l`), c8 leg 0 W 512 (+2.7, RISES) and leg 1 W 512 (+3
    against its own 3.2 allowance, FLAT or UNRESOLVED).
  - RISES puts ℓ̂'s source into R6.7. FLAT means the two ℓ̂ recipes coincide there.

---

## 2. Calibration: session-level, frozen now

**The choice, and its reason.**
- Per cell and leg, the calibration floors are **one session value**, over the cell's valid, non-latched W 128 rows:
  b̂₀⁺_c = the median `b0p`, and T_c = the median of `l` + `b0p`, with ℓ̂_c := T_c − b̂₀⁺_c (§1.1 states what the
  replay is fed).
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

**Pools.**
- W 128 (the law's provisional n₀) is the calibration pool. W 96 (c8: P48) is the flatness pool, which lies below n₀.
- Both bind (`STORE_CAP_FLOOR` = 10).
- W 128 is queue-free by measurement: at c2 it is above the wire BDP (102 symbols) but below the echo-loop BDP (163),
  with p27 b0p flat over caps 64–128.
- c8 and c1s have no measured pool at ≤ 128.

**Validity and latching** (rules as in entry 4):
- **Flatness gate:** PASS unless |Δb0p(96 − 128)| − 2·SE > q·ℓ̂_c, per leg. A cell failing it has no R̃.
- **LATCHED:** b0p − median_LATE(brake) > q·ℓ̂ + ō, where ō is c2 0.90, c8 leg 0 1.8 and leg 1 6.3, c1s 0.20 ms.
- **Minimum:** ≥ 3 non-latched W 128 rows per cell. With 6 rows at c8's ≈ 25 % latch rate, P ≈ 0.96.
- **The collapse flag does NOT gate calibration rows** (ruling entry 7, item 3).
  - The §56 flag fires when quinn's qcwnd sits below the cell's BDP on > 10 % of windows. The calibration pools (96,
    128) lie below c1s's BDP (≈ 208–243), so a store-limited flow may hold quinn's window under the BDP by
    construction. That is the flag's own condition, not collapse: §57b's c1d P192 rows carried a one-leg flag on 4
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
| c2 (knee 169; anchor flip depends on K, `s60flip.out`) | 0.0012–0.0026 | 0.149 (114–256 slots) | 0.008–0.015 | **at K = 5–8 the flip sits at the knee** (pooled flip − knee −3 to +4 slots), so R̃ ≈ R̃_flip > 1 there; **at K ≤ 4** the flip is at or below 128, and the conditioning fallback applies |
| c8 (knee 340, anchor flip ≈ 224–320) | 0.0017–0.0020 | 0.039 (39–47 slots) | 0.008–0.015 (8–19 slots) | per-replicate 293–301; **session 321–332**, i.e. ≥ 101–112 slots below the band's low edge (433) |

The scatter inputs are each measured and cited in `s60cond.py`:
- the absolute late floor's row sd: 0.10–0.25 ms at c2, 0.25–0.50 at c8 (§56; p27's 9 s c8 rows do not settle);
- σ at load: ≈ 0.1 ms;
- for the per-replicate case, also the calibration row's `l` (sd up to 1.4 ms) and `b0p` (sd ≈ 0.9, entering twice at
  anc 1).

### 3.1 c2: anchor-flip and conditioning readout (B2 not scored, as ruled)

**PREDICTION** (not a measurement), with the threshold drawn independently from §56's `l` rows and p27's b0p rows (the realistic case, `s60lmin.out`): verdict probabilities from p27's brakes and three §56 replicates (`s60flip.py`:
late m̂ at W 128 / 160 / 192 / 256, the per-replicate knee at 168.7–169.1, b̂₀⁺_c(K) from p27's six queue-free c2
rows).
- The Monte Carlo is over the session threshold T = ℓ̂_c + b̂₀⁺_c(K), which is one draw per session and common to all
  replicates, and over the replicates resampled to 6.

| K | b̂₀⁺_c(K) (ms) | T (ms) | flips on §56 replicates | P(STRUCTURAL) | P(NOT) | P(MIXED) | P(FLAT-AT-KNEE) | pooled median(flip − knee), 10 / 50 / 90 % |
|---|---|---|---|---|---|---|---|---|
| 1–2 | 0.00 | 8.07 ± 0.8 | ≤ 128 | 0.00 | 1.00 | 0.00 | 0.00 | −41 / −41 / −41 |
| 3 | 0.75 | 8.82 ± 0.8 | ≤ 128 | 0.01 | 0.98 | 0.01 | 0.01 | −41 / −41 / −23 |
| 4 | 1.01 | 9.08 ± 0.8 | ≤ 128 | 0.02 | 0.95 | 0.03 | 0.03 | −41 / −41 / −11 |
| 5 | 2.38 | 10.45 ± 0.9 | 160 / 167 / 173 | 0.35 | 0.48 | 0.17 | 0.45 | −41 / −3 / +23 |
| 6–7 | 2.46 | 10.53 ± 0.85 | 161 / 169 / 175 | 0.37 | 0.45 | 0.19 | 0.47 | −37 / −1 / +24 |
| 8 (the gauge's) | 2.67 | 10.74 ± 0.8 | 165 / 173 / 181 | 0.47 | 0.34 | 0.20 | 0.58 | −27 / +4 / +26 |

FLAT-AT-KNEE means the pooled flip is at or above the knee, so the knee arms sit at anc 0, where R̃ is flat in W.

- **At the gauge's K = 8 the 5-of-6 vote is a coin flip, so the vote is dropped.**
- **Registered instead: a POOLED readout, SCORED at K = 8 and REPORTED at K = 1..7.**
  - D_K = the median over replicates of (W_flip,r(K) − W₉₅,r).
  - W_flip,r(K) is where the replicate's late `m` (per-arm median) crosses ℓ̂_c + b̂₀⁺_c(K).
  - **The threshold is calibrated on the SUM.** T_c(K) = the session median over the valid set for that K (W 128 rows,
    LATCHED(K) excluded) of the per-row sum `l` + max(first K printed `b0k` brakes).
    - The anchor depends only on this sum, so ℓ̂ and b̂₀⁺ are drawn jointly from each row.
    - Where the anchor binds, F = ℓ̂_c + 2·b̂₀⁺_c also needs the separate b̂₀⁺_c (the session median of max(first K)),
      so F uses T_c + b̂₀⁺_c. Any cancellation inside the sum does not carry into R̃ there.
  - **K = 1 is no finding, IF Copa and the sampler see the same first samples.** Then brake 1 is 0 by construction:
    at the first window close, Copa's minimum is that window's own minimum. p27, a different sampler, reads 0.00 on
    all 6 rows. K = 2 is then near-trivial.
    - **Witness, UNVERIFIED on engine output.**
      - On `d416d59b` only `slaw.rs` and the renderer's synthetic `tools/l1/testdata/slaw_sample.log` contain
        `b0k=` (`git grep`), and that sample's m̂ is a fixed test input. No committed log carries a real `b0k`.
      - **Expected answer, from the code.** `record_rtt_sample` (`path.rs:606`) feeds the leg's `WinSampler`
        (`:616`, if attached) and Copa (`:619`) from the same sample. The sampler is attached by
        `Scheduler::enable_slaw`, called from `diag::report`'s first pass (`diag.rs:449`).
        - So `b0k[0]` = 0, and `m` = `l` for t < 10 s, **iff no echo sample reaches `record_rtt_sample` before that
          first pass**.
        - A sample in between (e.g. from the handshake or the first acks) would sit in Copa's minimum only.
        - A path joining later, such as c8 leg 1, is attached only if `enable_slaw` reaches it. That is to be
          confirmed in the same run.
      - **Requested:** the coordinator runs `cargo test --release --test slaw_reachability -- --nocapture`, and reads
        each leg's `b0k` at window K and `m` against `l` at the first windows.
    - **On 600 MB rows the premise behind cancellation fails** (`s60lmin.out`, §56 c2 and c8 small pools). The row's
      lifetime minimum `l` is reached **at boot on only 4 of 14 rows** (c2: 1 of 6). On the rest it is reached at
      t = 2.8–59 s, so during boot Copa's m̂ sits above `l`, and the boot brakes are not taken against `l`.
      - The sum `l` + b0p therefore keeps `l`'s late-found noise.
      - It equals the largest boot-window minimum only on the 4 rows whose minimum was reached at boot.
    - **A failing row** (`b0k[0]` ≠ 0, or `m` ≠ `l` at t < 10 s) is **reported, not voided**. Its brakes are then
      measured against a lower m̂ than the sampler's own minimum, so b0p over-reads by that offset, and the latch rule
      sees it.
    - K = 1 is then a finding, the size of that offset, and the "no finding" claim is withdrawn for that session.
- **The 90 % interval is a bootstrap (2 000 resamples, seed 60) that resamples BOTH terms:**
  - the calibration rows, which re-estimates T_c(K) jointly;
  - the sweep replicates.
- **Which term sets the width** (`s60flip.out`, K = 8).
  - Drawing §56's `l` rows and p27's b0p rows independently (as before): 53 slots from the threshold alone, 16 from
    the replicates alone, 52 combined.
  - **Drawing the sum jointly from p27's rows: the best case.**
    - The within-cap row sd of `l` + b0p(8) is 0.92 ms, against 0.45 for `l` and 0.77 for b0p.
    - p27's `l` lacks §56's row noise (0.45 against 1.4 ms), so p27 **cannot test** whether that noise cancels; §56
      shows it does not (above).
    - The threshold's se is then 0.47 ms, and the width is 26 slots from the threshold alone and 30 combined.
  - **Even the best case (26–30 slots) far exceeds the predicted |D| ≈ 4.** Calibration rows narrow it only slowly
    (independent draw: 34 slots at 12 rows, 16 at 48).
  - **So c2 stays a MEASUREMENT, with UNRESOLVED as its expected label.** No calibration rows are added.
  - The per-K table above uses the independent draw, which is the realistic case given `s60lmin.out`. In-session the
    interval is a bootstrap on the joint per-row sums.
- **Labels:** **ABOVE** if the interval lies > 0 (at that K the anchor keeps the form at or above the knee); **BELOW**
  if < 0; otherwise **UNRESOLVED**.
  - **Predicted:** BELOW at K ≤ 4 (D ≈ −41 slots, the flip at the calibration arm); UNRESOLVED at K 5–8 (−3 to +4,
    interval about −35 to +25).
- **K = 8 is scored at every cell** (c2 here, c8's B2, c1s's sign). K = 1..7 are reported at each.
- **A verdict that changes with K is a finding about K.** K has no provenance (R6.2, §13.1). The K at which D_K turns
  goes to R6.7 beside n₀.
- **The fallback, conditioning,** is read only for K values whose D_K is BELOW:
  - R̃_r(W₉₅,r) is replayed at that K;
  - the anc-1 slope and σ_R per arm are reported;
  - ILL-CONDITIONED iff 2σ_R/|slope| > W₉₅ − 128, CONDITIONED iff < W₉₅ − 158, otherwise MARGINAL.
- **Reported:** R̃ − 1 at the knee arms (160 / 176) per K, and each replicate's crossing W_r against the band
  (Appendix A).
- All of this goes to the law ledger and R6.7.
- **Reported, not scored:** the pooled median over replicates of R̃_r(W₉₅,r) − 1, with its minimum detectable miss
  2·1.253·σ_R/√n_s.
- **Pools:**
  - 96 and 128 (calibration);
  - 160 / 176 / 192 (knee and flip);
  - 256 (slope above the flip, and P1 and P3 at c2, predicted ratio ≈ 5.7 and Δℓ ≈ +1.3–1.5 ms);
  - 2 048 (control).

### 3.2 c8: B2, live as a gross-miss detector

- **B2:** replicate r counts iff R̃_r(W₉₅,r) < 1 − 2·σ_R. Here W₉₅,r is interpolated from goodput against the same
  replicate's control, and σ_R is the in-session sd over replicates at the bracketing arms.
- **It fires on ≥ 2 of the 5–6 scoreable replicates,** on the scored recipe alone.
- **It is read only where the replay's anc bind fraction is ≥ 0.9 at both bracketing arms.** If the in-session knee
  falls below 320, the 256 arm may still be anc 0, and that replicate is then reported, not counted.
- **It can fire.** Leg 0's window minimum climbs 0.063–0.076 ms per slot between the knee and the band. B2 fires for a
  crossing below ≈ 321–332 under session calibration, which is ≥ 101–112 slots below the band's low edge (433).
- **The band (reported context):** 433–586, anc 1, from single-row corners. A session value narrows it. The own-ℓ̂
  sensitivity is 462–605.
- **Pools:**
  - 96 and 128 (calibration);
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
- **Marginal cost over the G/C pairs:** 2 extra control G rows. The 12 calibration rows are needed anyway for c1s's
  P1, B1 and flatness gate.
- **Pools:** 96 and 128 (1 GB), and 2 048 (5 GB; 12 G rows, 10 of them paired with C).

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
  - launch 1: c2 W 192, one C row (no gauge tags) and one G row (all tags, witness PASS, CAPLOG replay PASS, `kw`/`b0k`
    after the warm-up cut);
  - launch 2: one c1s W 128 G row (1 GB) and one W 2 048 C row.
- **Go/no-go on the rehearsal's c1s W 128 rows: TWO rows (1 GB), GO iff at least one passes** (registered now; this is the first c1s measurement at n₀). At a collapse share of 0.29–0.48 one row would give a chance NO-GO well above c8's 25 %; two give ≈ 8–23 %.
  - **Where the thresholds come from.** At the c1s control, anc 1 holds, so an admitted b0 error δ moves R̃ − 1 by
    2δ/T̂ ≈ 0.105 per ms (T̂ ≈ 19 ms).
    - The registered readings differ by 0.20 (prefix minima −0.2 against exact minima ≈ 0), and σ_R is 0.03–0.08
      (§3.3).
    - So δ must stay ≤ 0.3 ms, a shift ≤ 0.03: the low end of σ_R and 15 % of the gap.
    - **Reconciling 0.6, 0.3 and 0.12.** δ is the error against the expected queue-free b̂₀⁺, which is the
      max-of-8 brake: 0.21–0.37 ms at cap 256 on p27/p34, central ≈ 0.30.
      - The GO threshold is that central value plus the tolerance: 0.30 + 0.30 = 0.6 ms.
      - The KA's 0.12 is not b̂₀⁺. It is the ledger **over-read** b̂₀⁺ − b₀ (≈ 0.30 − 0.18, with b₀ the median
        no-queue brake, 0.09–0.22).
    - The earlier 1.0 ms thresholds exceeded c1s's whole target above its floor (≈ 0.87 ms) and are withdrawn.
  - **GO** iff all of these hold:
    - (a) `b0k` holds 8 brakes, each with leg-0 `b0kn` ≥ 10;
    - (b) **`b0p` ≤ 0.6 ms**;
    - (c) the row is not LATCHED;
    - (d) it is not COLLAPSED (the absolute rule of §2);
    - (e) **its median LATE `tmin − l` is ≤ 0.5 ms.** p34 reads 0.13 at cap 256; a larger value carries a queue
      comparable to δ.
  - **Reported, not gating:** the §56 collapse-flag share (it may exceed 0.10 by construction, §2), and `b0p` and the
    latch state themselves.
  - **NO-GO** drops c1s's calibration rows and its control-sign verdict before 60b. 60b then runs only the G/C pairs
    and the evicting rows: 22 + 5 rows. P1 at c1s is reported against printed `b0p` alone.
- **Go/no-go on the rehearsal's c8 W 128 (P64) rows: TWO rows** (c8's 128 and 96 pools are unmeasured, and leg 1's
  boot windows were sometimes empty at P80).
  - About 1 c8 W 128 row in 4 latches, so a single-row NO-GO would drop B2 by chance 25 % of the time.
  - The rehearsal therefore runs two c8 W 128 rows. **GO iff at least one passes**, with a chance NO-GO of ≈ 6 %.
  - Both are reported.
  - **Where the thresholds come from.** At c8's knee the scored R̃ is predicted ≈ 1.16–1.19, against B2's threshold
    1 − 2σ_R ≈ 0.97–0.98, a gap of ≈ 0.2.
    - An admitted b0 error moves R̃(knee) by 2·w₀/T̂ ≈ 0.053 per ms on leg 0, and by ≈ 0.02 per ms on leg 1 (through
      the coupling term and leg 1's own weight).
    - Holding the combined shift to ≤ 0.05, a quarter of the gap, allows δ₀ ≤ 1.0 ms and δ₁ ≤ 2.5 ms.
  - **GO** iff all of these hold:
    - (a) `b0k` holds 8 brakes, with leg-0 `b0kn` ≥ 10 on each, and leg 1 has ≥ 4 windows with `b0kn` ≥ 10;
    - (b) **leg-0 `b0p` ≤ 3.9 ms** (p27 cap 128: 2.07 / 2.90, plus 1.0);
    - (c) **leg-1 b0 (from the qualifying `b0k` entries) ≤ 10 ms** (p27: 6.12 / 9.37, plus 2.5 over the central
      value);
    - (d) the row is neither LATCHED nor COLLAPSED;
    - (e) **leg-0 median LATE `tmin − l` ≤ 3.4 ms** (p27 cap 128: 2.43, plus 1.0).
  - **NO-GO** leaves c8's B2 unscored. c8's rows still run, for P1, P3 and the reported R̃.
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

**The rehearsal** (`LD_REHEARSE=1`, which runs no suites) runs both smokes, plus two c8 W 128 (600 MB) rows and two c1s W 128 (1 GB) rows for §5's go/no-go. Its release binary must equal 60-pre's `bin_sha`.

| launch | block | rows | typical | worst |
|---|---|---|---|---|
| **60-pre** | engine suites on the pinned tree, the crate-directory release build (the registered `bin_sha`), then the reference manifest | — | 65–75 min | 85 min |
| **60a** | release build (suite reuse HIT) | — | 6 min | 10 min |
| | smoke (c2 W 192, C + G) | 2 | 3 min | 3 min |
| | c2: 96, 128, 160, 176, 192, 256, 2 048 × 6 | 42 | 53 min | 63 min |
| | c8: 96, 128, 256, 320, 384, 512, 1 280 × 6 | 42 | 53 min | 63 min |
| | **total** | **86** | **≈ 1.9 h** | **≈ 2.3 h** |
| **60b** | release build (suite reuse HIT) | — | 6 min | 10 min |
| | smoke (c1s W 128 G 1 GB, W 2 048 C) | 2 | 3 min | 7 min |
| | c1s: 96 and 128 (1 GB) × 6; 2 048 G × 12; C × 10 | 34 | 45 min | 119 min |
| | evicting rows (plan 59) | 5 (+ ≤ 5) | 12 min | 32 min |
| | **total** | **41 (+ ≤ 5)** | **≈ 1.1 h** | **≈ 2.8 h** |

- **Total:** 127 rows (+ ≤ 5), down from 205 in three launches, plus the 60-pre suites.
- **Row times:** c2/c8 600 MB rows take 75–90 s. c1s rows take 45 s (1 GB) or 90 s (5 GB) typical, and 210 s worst
  (§57a's convention). Evicting rows take ≤ 150 s + 40 s.
- **Why two launches:** together they exceed 4 h worst case (5.1 h).
- **`LD_HARD`:** launch + 3 h (60a) and + 3.5 h (60b). The budget guard refuses with less than 2.3 / 2.8 h left. 60-pre holds the locks through `suites_run.sh`; `LD_HARD` does not apply to it (as §58).
- **Order:** 60-pre, then the rehearsal (with the c8 and c1s go/no-go), then 60a, then 60b. 60b runs after 60a's smoke, witnesses, R-sign gate and calibration validity are read. It reuses 60a's
  binary (sha recorded), in which case its build row is 0 min.
- **What was cut:** the c2 B2 grid; c8's band arms (448, 576, 640); the band IN/OUT scoring; the second and third
  per-replicate calibration rows; and the c2/c8 C rows.

## 8. Open

1. **c2: the anchor-flip readout (§3.1)** decides whether the form's c2 landing is structurally above the knee. If NOT STRUCTURAL, reinstating a c2 B2 under session calibration is the coordinator's call.
2. **R6.7:** the boot hold and n₀, plus ℓ̂'s source if P3 RISES. This is the next derivation, before any large sweep.
3. **Pre-launch:**
   - a renderer-generated witness sample (anchor-bound leg plus empty leg);
   - the bit-identity tests on `d416d59b`;
   - `harness60`: arm tables with per-arm bytes; session calibration with its drift and flatness checks and latch
     rule; the per-row witness and R-sign gate in the scorer; the conditioning readout; the CAPLOG replay; and plan
     59's rows. It needs stub tests and a rehearsal.
4. **c8 and c1s at ≤ 128 are unmeasured.** A failed flatness gate removes that cell's R̃ (c8: B2; c1s: the control
   sign).

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
