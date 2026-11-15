# s60: the store law's gauge session (B-1), pre-registration

Status: DRAFT REGISTRATION (derivation style). Docs and Python only: no engine code, no cargo, no VM contact. It is
written to be copied into `docs/status.md` as §60. It is built on `store-law.md` R4.8, R5.6 and R6.2, the gauge
branch `slaw-gauge` `2547fc07`, and `harness59` `b1af3fa4` (plan 59).

**Advisor (Fable) log: what each call changed.**
1. **Before the arms were fixed.**
   - The first finding (b0p under a static pool tracks the standing queue, so printed R has no crossing at c8 or
     c1s) became §1.2's predictions P1/P2. Printed R is kept as briefed, beside a replay R̃ with one b0 per cell.
     R̃ carries the crossing, B2 and the sign gate, and has a replay witness (§1.4).
   - Added a c1s calibration arm at W 256, because 384 is not a no-queue pool.
   - B1 is evaluated on `err=` on the calibration arm only. B6 is checked on §56 and dropped.
   - G/C sizing uses t quantiles and the larger of two pair sds.
   - Checked whether `[CAPLOG]` is gated by `RWM_SLAW`, and specified the K4 print.
2. **After the predictions and sizing.**
   - Fixed one mistranscribed input (c1s 768 → 0.49 ms).
   - ℓ̂ is substituted per cell as well as b0, with per-arm `l` registered. Its corners move c2's band down to the
     knee (§1.5).
   - The band now carries method A's integer-ms resolution, with the 0.1 ms c1s curve as an alternate.
   - The c1s window minima are prefix minima (`ov` > 0); this is caveated.
   - Registered: when the sign gate is read; the calibration-arm minimum and its fallback; the c1s
     bracketing rule; the realised-n rule for G/C; and the build and suites in launch 1's budget.
   - The K4 print uses window aggregates.
3. **Before commit:** see the end of §7.

Helper scripts (stdlib Python, run with `-I`; each output is committed beside its script in `store-law/`):

| script | input | output | what it computes |
|---|---|---|---|
| `s60boot.py` | `s56/rep56-diag-ld.tgz`, extracted | `s60boot.out` | per arm: boot queue (first 2 s), late queue, m̂'s lift, σ at boot and at load (proxies: `[DIAG]` has no window minimum) |
| `s60p27.py` | `loop-diag/p27/diag-ld`, `loop-diag/p34/run/diag-ld` | `s60p27.out`, `s60p34.out` | per static cap, from true window minima: the gauge's `b0p`, late T̂min − ℓ̂, the spread, and printed R rebuilt (Rlo, Rhi) beside Rcom (one b0 per cell) |
| `s60band.py` | numbers from the `.out` files above (cited per line) | `s60band.out` | the crossing band per cell: KA inputs propagated over their ranges, two readings of T̂min(W), both anchor states |
| `s60size.py` | `docs/l1-raw` (s56, s57a, s57b ledgers) | `s60size.out` | row wall times; G/C pair sds and n from §57a's c1s rows (t quantiles) |
| `s60replay.py` | a gauge client log | `s60replay.out` | `slaw_r` transcribed: the replay R̃, and the witness (own b0p ⇒ printed R) |

---

## 1. Frozen predictions as bands

### 1.1 What the known-answer crossings assumed

The gauge's `known_answer_r_on_the_s56_maps` gives these crossings at q 0.05 / 0.10:

| cell | q 0.05 | q 0.10 |
|---|---|---|
| c2 | 188.9 | 194.2 |
| c8 | 467.7 | 478.2 |
| c1s | 479.4 | 495.4 |

They come from fixed inputs: m̂ = ℓ̂, one b0 per cell, and a fixed T̂min(W).

**The stand-in for the window minimum** is `s56cls.py`'s `CURVE`, copied into the KA test. It is the §56 score's
arm-median **T̄ − T₀** (`rep56-score.txt`, "STANDING QUEUE AT THE KNEE"):
- T̄ is the rate-weighted LATE echo loop Σrᵢ·rttᵢ/Σrᵢ, built from integer-millisecond per-tick app-echo values;
- T₀ is the same statistic's per-leg floor on the smallest-W rows.

So a **central** loop statistic's shift stands in for the shift of the window **minimum**, which is what R reads
(R − 1 = Σw(F + σ + q·ℓ̂ − T̂min)/ΣwT̂).

The KA feeds the ledger **over-read** b̂₀⁺ − b₀ (0.41 / 1.0 / 0.12 ms) as `b0`, which is consistent with reading
T̄ − T₀ as T̂min − (ℓ̂ + b₀). The substitution is exact only if the window spread T̂ − T̂min does not change with W.

The p27 and p34 static-cap ledgers carry true window minima (`s60p27.out`, column S). They show the spread growing
with W:
- c2: 4.8 → 6.4 ms from W 128 to 256;
- c8: 5.5 → 10.2 ms from 128 to 512;
- c1s: 1.1 → 5.8 ms from 256 to 1 024.

At c1s the growth is first order: p34's window-minimum queue is only 0.1–1.9 ms over W 256–1 024, while §56's
central curve reads 0–5.5 ms. So the band (§1.5) carries both readings.

**The scored crossing is computed from the engine's printed fields only, and no hand-fed number enters it.**
§1.4 states why the R it reads is R̃, the printed function replayed with one b0 and ℓ̂ per cell, and not the
printed `R`.

### 1.2 b̂₀⁺ under the row's static pool: b0p tracks the standing queue

The gauge's `b0p` is the maximum of T̂min,k − m̂k over the row's first K = 8 windows (2 s). The law takes it at its own
boot, at n₀ = 128, at or below the BDP. A static-pool row instead boots at pool W, and the store fills in under
0.5 s (`[DIAG]` `win=192/192` at t = 0.5 s on c2 W 192). So above the BDP, the boot windows already carry that row's
standing queue.

**Measured on window minima** (`s60p27.out`, 100 MB rows):

| | c2 W 128 / 192 / 256 / 384 / 1 024 | c8 W 128 / 256 / 512 / 2 048 | c1s W 256 / 512 / 1 024 / 2 048 |
|---|---|---|---|
| b0p (ms) | 2.2 / 4.2 / 12.6 / 23.5 / 90.2 | 2.5 / 8.1 / 26.7 / 119.7 | 0.30 / 1.45 / 5.9 / 18.8 |
| late T̂min − ℓ̂ (ms) | 2.4 / 3.8 / 9.0 / 21.5 / 84.6 | 2.4 / 5.8 / 23.6 / 94.4 | 0.24 / 0.71 / 2.2 / 5.5 |

§56's 600 MB / 5 GB rows show the same pattern (`s60boot.out`, mean-based proxies):
- the boot queue equals the late queue at c2 W ≤ 384;
- it exceeds the late queue at every c1s pool, control included (boot 36 against late 17 ms at W 2 048);
- c2's control is the exception: boot 44 against late 197 ms, because 2 s does not fill 192 ms of queue on every row.

**What it means.**
- Arms do not share a target. Printed R at arm W is the R of a law whose calibration ran at pool W.
- R − 1 ∝ min(μ, b0p) + b0p + σ + q·ℓ̂ − x, where x = T̂min − ℓ̂ is the late queue and μ = m̂ − ℓ̂.
- So printed R stays above 1 wherever x ≤ 2·b0p + σ + q·ℓ̂, i.e. wherever the late queue has not outgrown twice the
  2 s boot queue. That is a start-up dynamic, not the law's target.
- On p27 the rebuilt printed R (`Rlo`, `Rhi`) is above 1 at every cap of all three cells.

**Registered predictions for printed R** (reported and scored as predictions; they decide no law question):
- **P1.**
  - Printed R's arm median is > 1 at every c8 and c1s arm, controls included.
  - At c2 it is > 1 at every arm ≤ 384. At c2 2 048 its sign is row-dependent: R < 1 iff that row's 2 s boot queue
    is ≲ half its late queue (2 of §56's 3 control rows).
  - So the R-sign gate, applied to printed R, would stop the session by prediction at c8 and c1s.
- **P2 (b0p rises with W; per-arm medians are registered as readouts).**
  - Predicted b0p(arm)/b0p(calibration) is ≥ 3 at c2 W 384 (≈ 10), c8 W 512 (≈ 10) and c1s W 1 024 (≈ 10–20).
  - **If P2 fails at c2 W 384** (ratio < 3), the boot windows were not queued. Printed R and R̃ then share their
    inputs up to the ℓ̂ term, and printed R becomes the scored R as the brief asked (R̃ is reported).
- **ℓ̂ rises too.**
  - Column `l` climbs with W at c2 (8.1 → 10.1 ms, `s60boot.out`; p27 8.8 → 11.2) and at c8 leg 0 (8.2 → 11.1).
  - It is flat at c1s (2.4–2.6).
  - Per-arm `l` medians are registered beside `b0p`.

### 1.3 The anchor bind adds one b̂₀⁺ to the target

F = min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺. With m̂ ≥ ℓ̂ + b̂₀⁺ (`anc = 1`), the target T̂min gains another full b̂₀⁺, not the over-read.

On 600 MB rows Copa's 10 s minimum tracks the standing queue (`s60boot.out` `lm`):
- c2: μ = 2.7 ms at W 192 and 6.5 at 256;
- c8 leg 0: 1.9 at W 320 and 10.5 at 512;
- c1s: 0.27–0.48 at W 512–1 024.

So the bind is marginal at c2 W 192 (μ ≈ b0 ≈ 2.7) and holds above it. At c1s it is marginal (μ ≈ b0 ≈ 0.3–0.4).
The band therefore spans both states.

**Crossing shift anc 0 → 1, per cell** (`s60band.out`, same method and corners):

| cell | method A (central) | method B (minima) | shift in slots |
|---|---|---|---|
| c2 | 191–201 → 215–232 | 204–216 → 230–251 | +25 to +35 |
| c8 | 464–538 → 502–582 | 458–534 → 497–590 | +37 to +56 |
| c1s | 489–529 → 526–580 | p27 579–658 → 648–754 | +37 to +100 |

`anc` and the `ancf` bind share are printed per window and leg. They decide in-session which state a row is in.

### 1.4 The scored R: the replay R̃ (one b0 and ℓ̂ per cell), with a witness

**Why not printed R.**
- By §1.2, printed R's crossing along W is set by how much queue each row builds in its first 2 s.
- P1 predicts that the sign gate fails and that B2 cannot fire at c8 or c1s.
- A falsifier that cannot fire decides nothing.

**R̃** is `slaw_r` (slaw.rs, transcribed in `s60replay.py`), run on each window's printed per-leg fields:
- `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `sig` (σ at load) and `q`;
- with **b0 and ℓ̂ per cell and leg taken from the cell's calibration arm**: the median of `b0p` and of `l` over
  that arm's unflagged rows;
- the coupling term recomputed from those.

Reason: the law calibrates both at its own boot, at n₀ ≤ BDP. The calibration arm is the only arm whose first 2 s
are a no-queue boot.

Every input is printed exactly (integer µs), so R̃ needs no new print and no build.

**Calibration arms:**

| cell | W | basis |
|---|---|---|
| c2 | 128 | `b0bound.py`'s no-queue caps 64–128 |
| c8 | 160 (P80) | caps 128–192 total |
| c1s | 256 (new) | caps 192–256; §56's 384 is not a no-queue pool: p34 `b0p` 0.67 against 0.29 at 256 |

- **Minimum:** ≥ 4 of 6 calibration rows unflagged at c2 and c8, and ≥ 3 at c1s.
- **c1s fallback:** W 384, with R̃ labelled BIASED-CAL. The declared bias is +0.4 ms on b0, against a ≈ 1.2 ms
  target.
- **Tail, reported per cell and leg:** max − median of the calibration rows' `b0p`. Max-of-8 has start-up outliers
  even at small pools: c8 cap 192 reads 3.96–53 ms.

**Witness (gates every G row).**
- `s60replay.py witness`: replayed with each row's **own** `b0p` and `l`, R̃ must equal the printed `R` and `Rb` to
  print precision (|ΔR| ≤ 6·10⁻⁶) on every defined window.
- On the gauge's own rendered sample (`tools/l1/testdata/slaw_sample.log`): PASS, 6 windows, worst 4.9·10⁻⁶
  (`s60replay.out`).
- A row that fails the witness is VOID for every R reading.

**Printed R stays registered**: P1, P2, `xs`/`xb` and `ra` are reported per row.

### 1.5 The frozen bands (Bulk, q = 0.10; the scored quantity is R̃'s per-replicate crossing)

`s60band.py` propagates every KA input over its measured range, with both anchor states, through two readings of
T̂min(W):
- **A:** §56's central curve, as in the KA test, at ±0.5 ms integer-ms resolution, plus R6.1's 0.1 ms c1s curve.
- **B:** p27 and p34 window minima, as T̂min(W) = ℓ̂_row(W) + (T̂min − ℓ̂)(W). The target uses the calibration ℓ̂
  ("sub", what R̃ does) and, as a corner, the row's own ℓ̂ ("row").

Ranges, each cited in the script:

| input | c2 | c8 (leg 0) | c1s |
|---|---|---|---|
| σ at load | 1.72–1.95 | 2.00–2.67 | 0.46–0.67 |
| b0 (calibration) | 2.18–2.87 | 2.49–3.43 | 0.29–0.40 |
| over-read | 0.41–0.90 | 1.0–1.8 | 0.12–0.20 |
| ℓ̂ (calibration) | 8.07–8.77 | 8.10–8.62 | 2.38–2.50 |
| coupling ½Δ | 0 | 13.5–14.8 | 0 |
| leg-1 pull (w₁/w₀)·D₁ | 0 | 1.4–3.2 | 0 |

The KA test has none of the last row, because its leg 1 carried weight 10⁻⁹.

**Frozen crossing bands:**

| cell | KA point | **band** | where its edges come from | the in-session test that moves within it |
|---|---|---|---|---|
| c2 | 194.2 | **165–251** | low: B "sub", anc 0, low σ and b0; high: B "row", anc 1 | `anc` share at W 192–224 |
| c8 | 478.2 | **424–590** | low: B "sub", anc 0, no leg-1 pull; high: B "row", anc 1 | `anc` share at W 448–576; leg-1 `sig` |
| c1s | 495.4 | **434–990** | low: A at 0.1 ms; high: B p34, anc 1 | per-arm median `tmin − l` at W 512 / 768 / 1 024 |

**The c1s band covers most of the sweep (384–1 024), so it is not a prediction that can miss.** What discriminates
is the c1s per-arm median `tmin − l` (exact in the gauge):
- if it reads ≤ 2.0 ms at W 1 024, the window-minimum reading (B) holds and the crossing lies in its upper part
  (≈ 580–990);
- if it reads ≥ 4.0 ms, the central reading (A) holds and the crossing lies at ≈ 434–580.

p34's c1s minima are minima of a **prefix** of each window: the ACKDIAG sample cap refused samples (`ov` > 0) on 348
of p34's 397 c1s windows. They over-read T̂min, so B's c1s corners are a lower edge for that reading. The gauge's
`WinSampler` minimum is exact over every sample.

**Scoring rule.** A cell's prediction is IN if the median over scoreable replicates of R̃'s crossing lies in its
band.

---

## 2. Arms, so each falsifier can decide

All arms are static pools through `RWM_STORE_PATH_POOL` (per path; c8's P = W/2), on Bulk, as in §56.

### 2.1 Arm lists

| cell | arms (total W) | role |
|---|---|---|
| c2 (600 MB) | **128** (cal), 160, 176, 192, 208, 224, 256, 384, **2 048** (ctl) | in-session knee: §56 W₉₅ 169, W₉₈ 185, with 16-slot steps where 32 slots cost 6 %; the band is 165–251 |
| c8 (600 MB) | **160** (cal, P80), 256, 320, 384, 448, 512, 576, 640, **1 280** (ctl, P640) | the knee (§56 W₉₅ 340): 256–384; the band 424–590: 384–640 |
| c1s (5 GB) | **256** (cal, new), 384, 448, 512, 640, 768, 1 024, **2 048** (ctl) | the brief's 448, plus 640, between the A and B readings; the knee (§56 903) lies between 768 and 1 024 |

**c8 and B2.** The frozen band's low edge (424) is 84 slots above c8's knee (≈ 340), and 138 slots above it at
the KA point (478). B2 can fire at c8 only if the engine lands roughly 85–140 slots lower than registered. The arms
bracket both the knee (256 / 320 / 384) and the band (448–640), so either outcome is read, not extrapolated.

**c2 and B2.** The band's low corner (165) reaches the knee (169), so B2 is live at c2.

### 2.2 Crossing per replicate

- **Per row:** R̃_row = the median over that row's LATE windows (t ≥ 15 s at c2 and c8, t ≥ 5 s at c1s, as §56) of
  the per-window R̃.
- **Per replicate r and cell:** order the arms by W. W_r is the linear interpolation between the last arm with
  R̃ > 1 and the next arm with R̃ ≤ 1, using unflagged rows only.
  - If a flagged row sits at the sign change, bracket with the nearest unflagged arms on each side, and report the
    bracket width.
  - More than one sign change across arms makes the replicate NON-MONOTONE: it is reported, and W_r is its first
    downward crossing.
- **R6.2's uniqueness gauge** is read here, as the number of sign changes across arms per replicate, predicted to
  be 1.
  - The per-row `xs`/`xb` counts are reported only. On a static pool they count window-to-window flicker of R near 1,
    not crossings along n.
- **n:** 6 replicates per cell. A cell's crossing is scoreable with ≥ 6 bracketed replicates at c2 and c8.
  - c1s's collapse share (0.29–0.48) will not give 6.
  - c1s is scored with ≥ 4 bracketed replicates, and is otherwise UNDERPOWERED: per-arm R̃ medians are reported.
  - No falsifier sits at c1s.

**Knee in-session:** W₉₅ and W₉₈ from the arm-median goodput over the control, interpolated in W, with §56's
definitions.

### 2.3 Falsifiers

| # | what fires it (printed fields only) | what it means |
|---|---|---|
| B1 | on a **calibration-arm** row, the median over LATE windows of the printed `err` (= brake − b0p − q·ℓ̂) is > 0 | the collapse condition: b̂₀⁺ + q·ℓ̂ below the row's own later no-queue brake |
| B2 | at c2 or c8, R̃'s W_r is below the in-session W₉₅ on more than 1 of every 6 scoreable replicates | the law lands below the knee |

- **B1 is read on the calibration arm only, with the reason.** On larger pools every later window carries queue, and
  m̂ tracks it, so `err` there is not a no-queue brake and would fire on every control row.
  - Predicted not to fire: `b0bound.out` max8 margins are +0.43 (c2), +0.46 (c8 leg 0) and +0.19 ms (c1s) on p27/p34.
- **B6 is dropped, and replaced by a readout.** On §56 (`s60boot.out`, c8 leg 1), σ at load over its boot value
  is 0.62 / 0.32 / 0.24 / 0.30 / 0.31 / 0.72 at W 160–1 280.
  - Measured as registered, it cannot fire: the boot maximum carries the start-up fill transient (sigb 20–84 ms).
  - Against the calibration arm's σ at load (4.92 ms), it already exceeds 3× at W 512 (3.1×), 768 (5.3×) and
    1 280 (6.4×), i.e. inside the band. §56 has already answered B6 (leg 1's σ grows with W; R6.2's 4.9 → 31.6),
    so it is not registered as a falsifier.
  - **Readout instead:** per arm, leg 1's `sig` against the calibration value, and leg 1's pull on R̃, w₁·D₁/Σw·T̂,
    from printed fields.
- **B3–B5 are not in this session.**
  - B3: c1d ran as §57b (T UNSCOREABLE), and a re-run needs its own registration.
  - B4 and B5 belong to B-2.
  - The R5.3 burst arm ran as §57a.
- **No decision rule.** The arms are the held-out maps, and a closed-loop arm is registered only after B1/B2 are read.

---

## 3. Perturbation control: paired G / C rows on one binary

- **The binary:** the gauge build (one sha, recorded in every row).
  - **G** = `RWM_SLAW=1`; **C** = `RWM_SLAW=0`. Both set `RWM_DIAG=1`, so `[DIAG]` is on in both.
  - The gauge's state, counters, `[CAPLOG]` recorder and debt hooks exist only under `pol.slaw_on` = `RWM_DIAG && RWM_SLAW`
    (`sender_policy.rs:283`, `sender_phases.rs:41`, `emit_source.rs`), so C runs none of it.
- **Placement:** at each cell's control, interleaved per replicate, with order alternating (G first in odd pairs, C
  first in even), to cancel the §57a position effect.
- **Sizing** (`s60size.out`, from §57a's c1s 5 GB rows, t quantiles). sd_d is the larger of the measured cross-arm
  pair sd and √2 × the row sd.

  | readout | sd_d | unflagged pairs for 5 % | launched at collapse 0.29 / 0.33 / 0.48 | MDD at 4 / 6 / 8 pairs |
  |---|---|---|---|---|
  | client CPU s/GB | 0.036 | 7 | 14 / 16 / 26 | 7.4 / 5.1 / 4.1 % |
  | server CPU s/GB | 0.040 | 8 | 16 / 18 / 30 | 8.4 / 5.7 / 4.6 % |
  | goodput | 0.057 | 13 | 26 / 29 / 48 | 11.8 / 8.1 / 6.5 % |

  A pair counts only if both rows are unflagged. The collapse share is 0.33 at §56's c1s control (3/9).
- **Registered:**
  - **c1s: 18 launched pairs at W 2 048.** Six are the sweep's control rows, each paired with a C row; 12 are extra,
    2 per replicate.
  - **CPU gate:** the median over unflagged pairs of log(G/C), client and server CPU s/GB. The gauge PERTURBS c1s iff
    it exceeds +5 %, the R5.6 criterion. If there are fewer than 8 unflagged pairs, the gate reads UNRESOLVED,
    with its realised MDD.
  - **Goodput:** UNRESOLVABLE at 5 %, because 29–48 launched pairs, 58–96 c1s rows, is too costly. The G/C goodput
    ratio is reported with its MDD (≈ 6.5 % at 8 pairs), and is not a gate.
  - **c2 and c8:** 3 C rows each at the control (replicates 1, 3 and 5), reported only. They are wire-bound cells, and
    the rows serve as the re-base and the tag-absence witness.
- **Re-base C** against the old controls:
  - c1s: §56's control (goodput 875 [727–887] Mbit/s; client CPU 15.82 [15.28–18.99] s/GB; server 18.67);
  - c2 and c8: §56 (c8 against its P640, the §54b B640 re-base);
  - §57a ran pool 512, not 2 048, so it enters only as a drift reference.
  - G/C bounds only the gated cost. C against §56 bounds the drift of the binary and the host.
- **Collapse share G against C** is reported. A gap above 0.25 marks the pair set BIASED, as §57a's rule did.
- **If c1s is PERTURBED:** G's c1s readings are reported as perturbed and are not used to choose an estimator
  (R5.6).

---

## 4. Witnesses (each one voids its row or stops the session as stated)

1. **`RWM_SLAW` is scrubbed and set.** The harness adds `-u RWM_SLAW` (and `-u RWM_THREE_TERM`) to its env scrub,
   then sets `RWM_SLAW=1` or `=0` explicitly per arm.
   - `[GATES]` echoes it on **both** ends of every row (`gates.rs:453`).
   - A row whose echo differs from its arm on either end is VOID and re-run once.
2. **The smoke is two rows at c2 W 192:** one C row and one G row, same binary, in launch 1. Launch 2 re-smokes the
   same pair at c1s W 512.
   - The C row must carry `[DIAG]` and **none** of `[WSTAT]`, `[WSTORE]`, `[SLAW]`, `[SLAWC]`, `[CAPLOG]`.
   - The G row must carry all five, `R` defined from window K = 8 on, the R̃ witness PASS (§1.4), and the CAPLOG
     replay PASS (item 4).
   - A smoke failure stops the launch.
3. **`[SLAW] q` reads `0.100000` on every Bulk G row** (the Bulk hint, b = 2: `codel_setpoint_q` = 0.10). Plan 59's
   Realtime rows read `0.050000`. Any other value voids the row.
4. **`[CAPLOG]` replays every logged cap.** On the shipped seat (default `RWM_QUIC_CC=bbr`, so there is no Copa feed
   and `RWM_THREE_TERM` is off), every refresh logs `seat=plain`. The branch is `br=pooled`, or `br=boot` before the
   first BDP sample (`sender_phases.rs:235–262`). **`tt` cannot occur** unless `RWM_THREE_TERM=1`, which the scrub
   prevents.
   - So the gate is not empty: every logged cap must be reproduced by `caplog_replay.py` through the pure
     `pooled_store_cap`, with 0 mismatches.
   - On static-pool rows the pooled cap binds at N·pool (§56: bind 1.000), so the replay mostly reproduces a clamp.
     Its **bind fraction** (the share of pooled refreshes with cap = N·pool, FORMULA-FIRST) is reported per arm,
     beside the unclamped gain·Σbdp replayed from the same logged inputs.
5. **The R-sign gate (on R̃).** At each cell, R̃_row > 1 at the calibration arm and < 1 at the control.
   - It is evaluated by the scorer after that cell's replicate 2. The cell b0 and ℓ̂ are then the median of the ≤ 2
     calibration rows so far.
   - It fails only if the median over unflagged rows so far violates it, at either end.
   - Predicted margins, R̃ − 1: c2 ≈ +0.17 / −0.9; c8 ≈ +1.0 / −0.6; c1s ≈ +0.2 / −0.2. c1s is the thin end.
   - **A failed gate stops the session (ABORT-RGATE) and is amended** before any further row.
   - P1's printed-R signs are recorded at the same point and stop nothing.
6. **Bit-identity.** The gauge's host test (cap sequences equal with the gauge on and off) and the native golden test
   with `GOLDEN_CAPTURE` unset pass on the built tree before launch 1, as `slaw-gauge` records.
7. **Host and order.** Host cargo is paused, with the build lock held by the main session through both launches.
   Arm order rotates per replicate (replicate r starts at arm index r mod A), and seeds alternate 42 / 7 as in §56.
   The session-position readout (first- against second-half medians per arm) is reported.

---

## 5. The evicting rows (plan 59) on the gauge build

- **Rows:** plan 59's cell and arm, unchanged.
  - c2-100, 600 MB, `--protocol-hint realtime`, no `--window-reliable`, on the shipped unified machine.
  - n = 5 (seeds 42 / 7 / 42 / 7 / 42), each VOID slot re-run once.
  - Env: `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150`, plus `RWM_SLAW=1` (G only).
  - A DNF is an expected datum.
- **m̄, direct:** m̄ = Δ`cum` ack / Σ `adv` count, over LATE (t ≥ 5 s, plan 59's window).
  - `[WSTORE] adv=` is count / p50 / p90 / max of `newly_acked` per cumulative-ack advance (`note_adv`).
  - `adv` p50 and p90 are reported per row, as medians over windows.
  - **Witness:** plan 59's lower bound `mbar_lo` = Δcum / Σ`wait n` ≤ m̄ on every row; a violation is a token-semantics
    error. The release batch bounds nothing (plan 59) and is reported.
- **W1, direct:** W1 per window = `dbt_share` = `dbt_em` / `dbt_acc`, the repair debt spent over the debt accrued in
  that window.
  - The row's W1 is Σ`dbt_em`/Σ`dbt_acc` over LATE.
  - **Witness:** it lies inside plan 59's bracket, [max(0, 1 − 1/(rr·mbar_lo)), (cod/src)/rr].
- **K4 on per-window values, not row means.** The earlier Realtime row ran A\* 1–201 and rr 0.004–0.5 within one
  row, so a row mean is not an operating point.
  - K4 compares the oracle's W1 at each window's own (ε̂, σ², rr, A\*, Δ, m̄) with that window's `dbt_share`.
  - It is restricted to **steady windows**: at least one refresh, A\* min = max, Δ min = max.
- **K4 needs σ²:** the GE channel's p and q come from (ε, σ²) (`pfec-oracle-spec.md` §3).
  - The engine computes σ² only at the shed-refresh site, `burst_variance_factor(est)` (`emit_source.rs:597`), and
    prints it nowhere.
  - `[SPAN]` runs on its own clock, and a last value is not a window value.
  - **So one print is added to the gauge now, before the build, so that no second build is needed:**

    ```text
    [WSTORE] … fec=<nref>/<eps_mean>/<sig2_mean>/<astar_min>-<astar_max>/<delta_min>-<delta_max>/<rr_mean>
    ```

    - It is accumulated in gauge atomics at the shed-refresh site (`emit_source.rs` ≈ 583–598) under
      `pol.shed_on && pol.slaw_on`, from the same `est.loss_rate()`, `burst_variance_factor(est)`, `a_star`, `delta`
      and `flat_rate` that the site already computes.
    - It is reset at each window close, like the `dbt_*` deltas.
    - `nref` = refreshes in the window; `fec=-` when `nref` = 0.
  - It is print-only and adds no engine read. The gauge's bit-identity host test and goldens are re-run on the amended
    tree.
  - The site runs only under `shed_on`, which holds on the evicting seat (`RWM_UNIFIED_SHED` defaults on). On Bulk
    rows `fec=-` is expected.
- **If the print is not added before the build,** K4 cannot be run from s60. The evicting rows then still give m̄ and
  W1, and K4 needs a second session.

---

## 6. Rows and length

**Row times:**
- The brief's figures: ≈ 75–90 s per 600 MB c2/c8 row with the gauge, and ≈ 75–170 s per 5 GB c1s row, collapse rows
  longer (DNF at 300 s).
- Measured, without gauge output (`s60size.out`):
  - slot spacing 55 s (§56), 61 s (§57a) and 60 s (§57b);
  - c1s rows up to 302 s;
  - plan 59's evicting rows ≤ 150 s, plus ≈ 40 s of bring-up.

| block | rows | typical | worst |
|---|---|---|---|
| build + engine suites (new gauge tree: no reuse hit) | — | ≈ 60 min | 70 min |
| smoke (c2 W 192, C + G) | 2 | 3 min | 3 min |
| c2: 9 arms × 6 + 3 C | 57 | 71 min (75 s) | 86 min (90 s) |
| c8: 9 arms × 6 + 3 C | 57 | 71 min | 86 min |
| **launch 1 (`LD_PLAN=60a`)** | **116** | **≈ 3.4 h** | **≈ 4.1 h** |
| smoke (c1s W 512, C + G) | 2 | 3 min | 6 min |
| c1s: 8 arms × 6 G | 48 | 60 min (75 s) | 136 min (170 s) |
| c1s G/C extra: 6 C + 12 pairs | 30 | 38 min | 85 min |
| evicting rows (plan 59 on the gauge) | 5 (+ ≤ 5 re-runs) | 12 min | 32 min |
| **launch 2 (`LD_PLAN=60b`)** | **85 (+ ≤ 5)** | **≈ 1.9 h** | **≈ 4.3 h** |

- **Total:** 201 rows (+ ≤ 5 evicting re-runs). The session exceeds 4 h, so it splits into two launches.
- **`LD_HARD`:** launch + 4.5 h for **60a**, and launch + 4.5 h for **60b**.
- **The budget guard** refuses with less than the worst case left: 4.1 h for 60a and 4.3 h for 60b.
- **Order:** 60a's R-sign gate (§4.5) is read at c2 and c8 replicate 2. 60b runs only after 60a's smoke, witnesses and
  gates are read.

---

## 7. Open or not settled

1. **The scored R is R̃, not printed R.** This deviates from the brief, which said "printed R only". The evidence is
   §1.2's P1: printed R has no crossing at c8 and c1s, so its sign gate stops the session and B2 cannot fire.
   - R̃ needs no new print, and its witness ties it to the engine's function.
   - P2 gives the brief's reading a way back: if b0p does not rise with W, printed R is scored.
   - **This needs the coordinator's ruling before the harness is written.**
2. **The K4 print (§5) is a gauge-code change.** It is specified here and not made: no engine code in this task. It
   must land on `slaw-gauge` before the VM build, or K4 moves to a later session.
3. **The c1s band (434–990) is not a prediction that can miss.** Its discriminator is per-arm `tmin − l` (§1.5).
4. **The c1s per-replicate crossing will be UNDERPOWERED** at the observed collapse share, unless ≥ 4 replicates
   bracket.
5. **The harness (`harness60`) does not exist yet.** It needs:
   - arm tables for 60a and 60b;
   - the `RWM_SLAW` scrub and per-arm set;
   - the smoke checks;
   - the replay witness and sign gate in the scorer;
   - the CAPLOG replay call;
   - plan 59's evicting rows behind `RWM_SLAW=1`.
   It needs stub tests and a rehearsal before any battery row, as in §56/§57.
6. **Advisor, before commit:** (filled in at the commit below.)
