# s60: the store law's gauge session (B-1), pre-registration

**For the owner, in plain words.**
- On a static store pool, the store fills within half a second. So the law reads its two floors, b̂₀⁺ (the boot brake)
  and ℓ̂ (the lifetime echo minimum), from windows that already carry queue.
- That is what p27 and §56 predict (P1, P3 below). The gauge has not yet shown it.
- So this session cannot test the law itself. It tests the law's target FORM with floors supplied from queue-free
  calibration rows, decisively only at c8 (B2), and measures whether the floors are contaminated (P1, P3).
- The next step is a derivation, not a sweep: the boot hold and n₀ (`store-law.md` R6.7). A large sweep belongs after
  it.

**c2: B2 is unscoreable as ruled (entry 6), and NOT by design under the calibration this revision chooses (needs the
coordinator's ruling).**
- With one calibration row per replicate, a c2 B2 miss smaller than 114–256 slots is undetectable. That is below the
  calibration arm, so B2 cannot fire.
- With the session-level calibration of §2, which entry 6 item 4 asked to consider, it is 6–26 slots. B2 would then
  fire for a crossing below ≈ 143–163, against a knee at 169.
- The cause is that the row scatter of `l` is sampling noise of a minimum (`s60abs.out`). A session median removes it.
- As ruled, B2 at c2 is **not scored**. It is replaced by a **conditioning readout** with a verdict fixed now (§3.1),
  plus the pooled median as a REPORTED readout.
- Whether to reinstate c2's B2 under session calibration is the coordinator's call.

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

**Helper scripts** (stdlib Python, run with `-I`; outputs committed beside them in `store-law/`).

| script | what it computes |
|---|---|
| `s60boot.py` | per arm and row on §56: boot queue (first 2 s), late queue, m̂'s lift, `l`, σ at boot and at load |
| `s60p27.py` | per static cap and row from p27/p34 window minima: `b0p` per leg, late T̂min − ℓ̂, spread, `l`, rebuilt printed R and Rcom |
| `s60ell.py` | row-level Spearman of `l` against W |
| `s60abs.py` | per arm: row sd of `l` against the row sd of the ABSOLUTE late floor, and corr(`l`, floor − `l`) |
| `s60drift.py` | §56 small pools: do `l` and the boot-queue proxy drift with row order? |
| `s60cond.py` | the conditioning arithmetic per cell (slope of R̃ in W, σ_R, 2σ_R in slots), and c1s verdict probabilities per reading |
| `s60band.py` | the crossing bands (reported context) |
| `s60size.py` | row times; G/C pair sds and n |
| `s60replay.py`, `s60witness_ka.log` | `slaw_r` transcribed: R̃, the per-row witness, and its negative control |

---

## 1. What R̃ is, what it tests, and the floor predictions

### 1.1 R̃

**Definition.**
- R̃ is the gauge's `slaw_r` (R6.2's line), replayed from each window's printed `[SLAW]` fields.
- The two floors are substituted: per cell and leg, **b̂₀⁺** and, at c2 and c8, **ℓ̂** take the session calibration
  value (§2). Every other input is the row's own: `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `sig` (σ at load)
  and `q`.
- At c1s the row's own `l` is kept, because it is flat there (`s60ell.out`: Spearman +0.03, p 0.43).
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
- **Registered:** b0p(arm)/b0p(calibration) ≥ 3 at c2 W 384, c8 W 512 (leg 0) and c1s W 2 048. Predicted ≈ 10 / ≈ 10 /
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
  - Arms: c2 W 384 (predicted +2.1, which RISES or is UNRESOLVED), c8 leg 0 W 512 (+2.7, RISES) and leg 1 W 512 (+3
    against its own 3.2 allowance, FLAT or UNRESOLVED).
  - RISES puts ℓ̂'s source into R6.7. FLAT means the two ℓ̂ recipes coincide there.

---

## 2. Calibration: session-level, frozen now

**The choice, and its reason.**
- Per cell and leg, the calibration floors are **one session value**: b̂₀⁺_c = the median `b0p`, and ℓ̂_c = the median
  `l`, over the cell's valid, non-latched W 128 rows (leg 1's b0 from `b0k` entries with `b0kn` ≥ 10).
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
- **c1s boot validity:** no qcwnd below the BDP for t < 2 s. Its latch test runs up to collapse onset, with ≥ 8 windows.
- **c1s calibration rows run 1 GB.**

**B1** fires iff a calibration row's median LATE `err` > 0. Predicted not to fire (`b0bound.out` margins +0.43 / +0.46 /
+0.19 ms). When B1 does not fire, R − 1 ≥ σ/T̂ > 0 on that row (anc 0, single leg), so the calibration-end sign holds by
construction.

---

## 3. What each cell can decide

The conditioning arithmetic (`s60cond.py`):
- dR̃/dW = −(w₀/Σw)·(dT̂min₀/dW)/T̄, and an input scatter s moves R̃ by s/T̄.
- B2 resolves a miss of 2σ_R/|dR̃/dW| slots, and fires iff the crossing lies below knee − 2σ_R/|dR̃/dW|.

| cell | slope \|dR̃/dW\| (per slot) | σ_R, per-replicate calibration | σ_R, session calibration | B2 fires iff the crossing lies below |
|---|---|---|---|---|
| c2 (knee 169) | 0.0012–0.0026 | 0.149 → 2σ_R = 114–256 slots | 0.008–0.015 → 6–26 slots | per-replicate: 55 or less, i.e. never; **session: 143–163** |
| c8 (knee 340) | 0.0017–0.0020 | 0.039 → 39–47 slots | 0.008–0.015 → 8–19 slots | per-replicate: 293–301; **session: 321–332**, i.e. ≥ 101–112 slots below the band's low edge (433) |

The scatter inputs are each measured and cited in the script:
- the absolute late floor's row sd: 0.10–0.25 ms at c2, 0.25–0.50 at c8 (§56; p27's 9 s c8 rows do not settle);
- σ at load: ≈ 0.1 ms;
- for the per-replicate case, also the calibration row's `l` (sd up to 1.4 ms) and `b0p` (sd ≈ 0.9, entering twice
  at anc 1).

### 3.1 c2: conditioning readout (B2 not scored, as ruled)

- **Registered readout.**
  - The slope of R̃_row in W across the knee arms 160 / 176 / 192, as the median over replicates of the per-replicate
    least-squares slope. Predicted 0.0012–0.0026 per slot.
  - σ_R per arm, as the sd over replicates. Predicted 0.008–0.015 under session calibration, and 0.15 per-replicate.
  - Their ratio, 2σ_R/|slope|, in slots: the input scatter's effect on the fixed point. Predicted 6–26 (session) and
    114–256 (per-replicate), against a knee 11 slots inside the band.
- **Verdict, fixed now.**
  - **ILL-CONDITIONED** iff 2σ_R/|slope| > W₉₅ − 128. A B2 miss would then have to cross below the calibration arm,
    where R̃ > 1 by construction.
  - **CONDITIONED** iff 2σ_R/|slope| < W₉₅ − 158, the band's low edge.
  - Otherwise **MARGINAL**.
  - The verdict is reported for both calibrations (the scored session value, and per-replicate pairing as
    sensitivity), and goes to the law ledger and R6.7.
- **Reported, not scored:** the pooled median over replicates of R̃_r(W₉₅,r) − 1, with its minimum detectable miss
  2·1.253·σ_R/√n_s. That is 0.008–0.015 in R under session calibration, ≈ 3–13 slots.
- **Pools:** 96 and 128 (calibration), 160 / 176 / 192 (knee and slope), 384 (P1, P3), and 2 048 (control, for W₉₅).

### 3.2 c8: B2, live as a gross-miss detector

- **B2:** replicate r counts iff R̃_r(W₉₅,r) < 1 − 2·σ_R. Here W₉₅,r is interpolated from goodput against the same
  replicate's control, and σ_R is the in-session sd over replicates at the bracketing arms.
- **It fires on ≥ 2 of the 5–6 scoreable replicates,** on the scored recipe alone.
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

| launch | block | rows | typical | worst |
|---|---|---|---|---|
| **60a** | build + engine suites (new tree) | — | 60 min | 70 min |
| | smoke (c2 W 192, C + G) | 2 | 3 min | 3 min |
| | c2: 96, 128, 160, 176, 192, 384, 2 048 × 6 | 42 | 53 min | 63 min |
| | c8: 96, 128, 256, 320, 384, 512, 1 280 × 6 | 42 | 53 min | 63 min |
| | **total** | **86** | **≈ 2.8 h** | **≈ 3.3 h** |
| **60b** | smoke (c1s W 128 G 1 GB, W 2 048 C) | 2 | 3 min | 7 min |
| | c1s: 96 and 128 (1 GB) × 6; 2 048 G × 12; C × 10 | 34 | 45 min | 119 min |
| | evicting rows (plan 59) | 5 (+ ≤ 5) | 12 min | 32 min |
| | **total** | **41 (+ ≤ 5)** | **≈ 1.0 h** | **≈ 2.6 h** |

- **Total:** 127 rows (+ ≤ 5), down from 205 in three launches.
- **Row times:** c2/c8 600 MB rows take 75–90 s. c1s rows take 45 s (1 GB) or 90 s (5 GB) typical, and 210 s worst
  (§57a's convention). Evicting rows take ≤ 150 s + 40 s.
- **Why two launches:** together they exceed 4 h worst case (5.9 h).
- **`LD_HARD`:** launch + 4 h (60a) and + 3 h (60b). The budget guard refuses with less than 3.3 / 2.6 h left.
- **Order:** 60b runs after 60a's smoke, witnesses, R-sign gate and calibration validity are read. It reuses 60a's
  binary.
- **What was cut:** the c2 B2 grid; c8's band arms (448, 576, 640); the band IN/OUT scoring; the second and third
  per-replicate calibration rows; and the c2/c8 C rows.

## 8. Open

1. **Coordinator: c2's B2 under session calibration.** It is conditioned at 6–26 slots on the a priori σ_R. Reinstate
   it, or keep the conditioning readout only. The readout's verdict decides it either way after the session.
2. **R6.7:** the boot hold and n₀, plus ℓ̂'s source if P3 RISES. This is the next derivation, before any large sweep.
3. **Pre-launch:**
   - a renderer-generated witness sample (anchor-bound leg plus empty leg);
   - the bit-identity tests on `d416d59b`;
   - `harness60`: arm tables with per-arm bytes; session calibration with its drift and flatness checks and latch
     rule; the per-row witness and R-sign gate in the scorer; the conditioning readout; the CAPLOG replay; and plan
     59's rows. It needs stub tests and a rehearsal.
4. **c8 and c1s at ≤ 128 are unmeasured.** A failed flatness gate removes that cell's R̃ (c8: B2; c1s: the control
   sign).
