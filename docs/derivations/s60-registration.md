# s60: the store law's gauge session (B-1), pre-registration

Status: DRAFT REGISTRATION (derivation style). Docs and Python only: no engine code, no cargo, no VM contact. It is
written to be copied into `docs/status.md` as §60. It is built on `store-law.md` R4.8, R5.6 and R6.2, the gauge
branch `slaw-gauge` `2547fc07`, and `harness59` `b1af3fa4` (plan 59).

**Advisor (Fable) log: what each call changed.** Entries 1–3 are this draft's history, and entry 4 supersedes them
where they differ.
1. **Before the arms were fixed.**
   - The finding that b0p under a static pool tracks the standing queue became the printed-R predictions.
   - A replay R̃ with one calibration b0 per cell carries the crossing, with a replay witness.
   - B1 moves to the calibration arm. B6 is checked on §56 and dropped.
   - G/C is sized with t quantiles.
   - Checked `[CAPLOG]`'s gating, and specified the K4 print.
2. **After the predictions and sizing.**
   - A mistranscribed c1s input was fixed.
   - Added the integer-ms resolution and the 0.1 ms c1s curve.
   - The c1s minima are caveated as prefix minima.
   - Added the gate timing, the bracketing rule and the realised-n rule.
   - Build and suites are in the budget, and K4 is printed as window aggregates.
   - ℓ̂ substitution was added; entry 4 reverses it.
3. **Before the first push (`ac32ad3a`).**
   - The c1s band top is open, and R̃ ≥ 1 at the c1s control is a law outcome.
   - B2 is scoreable at 5 of 6 replicates.
   - Every C-row gauge site was verified to be gated by `slaw`.
   - The worst case uses 210 s per c1s row.
   - Added the warm-up cut, the gate deferral and the per-cell switch back to printed R.
4. **Main-session ruling on `ac32ad3a`: R̃ ACCEPTED on conditions; this revision implements them, worked with the
   advisor.**
   - **What R̃ tests is stated (§1.4).** b0p's rise with W is the LEAD prediction P1, with its meaning: boot hold and
     n₀ are part of the law. This is recorded as an open derivation item and as the R5.5 erratum in `store-law.md`
     (R6.7).
   - **Calibration** uses two small pools per cell under a flatness validity gate, at n₀ = 128.
     - The queue-free check is arithmetic and measured.
     - Each replicate is paired with its own calibration row.
     - Latched boots are counted and handled, with two n₀ rows per replicate.
   - **The recipe substitutes b0**, and keeps each row's own `m` and `sig`.
     - **ℓ̂ invokes the ruling's exception clause at c2 and c8.** A row-level rank test (`s60ell.py`) shows `l`
       rising with W:
       - s56: Spearman +0.58 (p 0.007) at c2, +0.46 (p 0.014) at c8 leg 0 and +0.77 (p 0.0001) at c8 leg 1;
       - p27: +0.70 (p 0.001) at c2 and +0.43 (p 0.046) at c8;
       - c1s is flat on s56 (+0.03, p 0.43).
     - So at c2 and c8, ℓ̂ also comes from the same calibration row, and own-ℓ̂ R̃ is a reported sensitivity. At c1s
       the row's own `l` is kept.
     - The coordinator confirmed this reading in entry 5.
     - The c2 band quoted here (158–242) is superseded by entry 5's anchor-consistent 180–242.
   - **The second calibration pool lies BELOW n₀** at every cell, because a pool above n₀ tests the next pool. The
     flatness tolerance is q·ℓ̂ on the n₀ rows' `l`, beyond noise.
   - **One calibration row feeds each replicate**, so every b0, ℓ̂ and coupling corner is a row range.
   - The arm lists now match the row counts: c2 gains 320, and c8 gains 576.
   - **`Rb` is not scored.**
   - **The witness is a per-row gate.** It is shown on a two-leg, anchor-bound, empty-leg sample that is
     hand-formatted (no cargo), with a negative control.
   - **c1s is cut to the one thing it can decide**, the sign of R̃ − 1 at the control (PINNED / CROSSES / SPLIT).
     The calibration-end sign is a consequence of B1 not firing, not a gate.
   - **G/C:** the gauge's cost is predicted from its benches, and 60c shrinks to a gross-error check.
   - **Out-of-band meanings** are stated at c2 and c8.
   - The rows and launches are recounted.
5. **Main-session ruling on `0eb08d97`: calibration-ℓ̂ at c2 and c8 is ACCEPTED, and that recipe carries B2's verdict
   alone.** Worked with the advisor.
   - The ℓ-DEPENDENT veto is dropped. The own-ℓ̂ result is reported beside the verdict, and its masking outcome is
     named (§1.4). The rule is idle at c8.
   - The contamination is sized in ms per arm (§1.2). The statistic is identified as the printed field's lifetime
     minimum, with a witness (`m` = `l` for t < 10 s).
   - **P3** (printed `l` rises by more than q·ℓ̂ beyond noise) is registered, and goes to R6.7. `[DIAG] rtp_us` is
     the gauge's `m` (`diag.rs:579`, `p.min_rtt()` = `copa.min_rtt()`).
   - **The anchor, checked row by row:**
     - At c8 W ≥ 320 it binds under substitution. F is all calibration there, and the row supplies only T̂min, T̂, σ
       and w. The c8 band holds the anchor fixed at **433–586**.
     - At c2 it does not bind row by row. A W 192 row's m̂ can fall below the calibration row's ℓ̂, so c2 keeps both
       states at **158–242**.
     - An advisor check overturned an arm-median version that read c2 as bound at W ≥ 192.
   - **c8's leg 1 has boot windows with 0 or 1 echo sample** at P80. Its b0 is therefore taken from the printed
     `b0k` brakes whose `b0kn` ≥ 10.
   - **B2 resolution is in R units** (§1.6). At c2 the slot equivalent of the margin lies below the calibration arm,
     so B2 at c2 is ill-conditioned **(needs the coordinator's ruling)**.
   - **c8 gets a third n₀ row:** P(scoreable) goes from 0.76 to 0.85 at 0.9 validity, or from 0.90 to 0.97 at 0.97,
     for +6 rows.
   - 205 rows in total.

Helper scripts (stdlib Python, run with `-I`; each output is committed beside its script in `store-law/`):

| script | input | output | what it computes |
|---|---|---|---|
| `s60boot.py` | `s56/rep56-diag-ld.tgz`, extracted | `s60boot.out` | per arm and per row: boot queue (first 2 s), late queue, m̂'s lift, `l`, σ at boot and at load (proxies: `[DIAG]` has no window minimum) |
| `s60p27.py` | `loop-diag/p27/diag-ld`, `loop-diag/p34/run/diag-ld` | `s60p27.out`, `s60p34.out` | per static cap and row, from true window minima: the gauge's `b0p` per leg, late T̂min − ℓ̂, the spread, `l`; printed R rebuilt (Rlo, Rhi) beside Rcom |
| `s60band.py` | numbers from the `.out` files (cited per line) | `s60band.out` | the crossing band per cell under the frozen recipe |
| `s60size.py` | `docs/l1-raw` (s56, s57a, s57b ledgers) | `s60size.out` | row wall times; G/C pair sds and n (t quantiles) |
| `s60replay.py` | a gauge client log | `s60replay.out` | `slaw_r` transcribed: R̃, the per-row witness, its negative control |
| `s60witness_ka.log` | — | — | the hand-formatted two-leg witness sample (§1.4) |
| `s60ell.py` | `s60boot.out`, `s60p27.out` | `s60ell.out` | row-level Spearman of `l` against W, with a permutation p (§1.2) |

---

## 1. What is predicted, and what R̃ tests

### 1.1 What the known-answer crossings assumed

The gauge's `known_answer_r_on_the_s56_maps` gives these crossings at q 0.05 / 0.10:

| cell | q 0.05 | q 0.10 |
|---|---|---|
| c2 | 188.9 | 194.2 |
| c8 | 467.7 | 478.2 |
| c1s | 479.4 | 495.4 |

They come from fixed inputs: m̂ = ℓ̂, one b0 per cell, and a fixed T̂min(W).

**The stand-in for the window minimum** is `s56cls.py`'s `CURVE`, copied into the KA test. It is §56's arm-median
**T̄ − T₀** (`rep56-score.txt`, "STANDING QUEUE AT THE KNEE"):
- T̄ is the rate-weighted LATE echo loop, built from integer-millisecond per-tick app-echo values;
- T₀ is its per-leg floor on the smallest-W rows.

So a central loop statistic's shift stands in for the window minimum's shift, which is what R reads
(R − 1 = Σw(F + σ + q·ℓ̂ − T̂min)/ΣwT̂). The KA feeds the ledger over-read b̂₀⁺ − b₀ (0.41 / 1.0 / 0.12 ms), consistent
with reading T̄ − T₀ as T̂min − (ℓ̂ + b₀).

The substitution holds only if the window spread is W-invariant. It is not (`s60p27.out`, column S):
- c2: 4.8 → 6.4 ms over W 128–256;
- c8: 5.5 → 10.2 over 128–512;
- c1s: 1.1 → 5.8 over 256–1 024.

At c1s that growth is first order.

### 1.2 P1 (LEAD): printed b0p rises with W; boot hold and n₀ are part of the law

The gauge's `b0p` is the maximum of T̂min,k − m̂k over the row's first K = 8 windows (2 s). A static-pool row boots at
pool W, and the store fills in under 0.5 s (`[DIAG]` `win=192/192` at t = 0.5 s, c2 W 192). Above the queue-free
pools, the boot windows therefore carry that row's standing queue.

**Measured on window minima** (`s60p27.out`; 2 rows per cap):

| | c2 W 128 / 192 / 256 / 384 / 1 024 | c8 W 128 / 256 / 512 / 2 048 | c1s W 256 / 512 / 1 024 / 2 048 |
|---|---|---|---|
| b0p, leg 0 (ms) | 2.2 / 4.2 / 12.6 / 23.5 / 90.2 | 2.5 / 8.1 / 26.7 / 119.7 | 0.30 / 1.45 / 5.9 / 18.8 |
| late T̂min − ℓ̂ (ms) | 2.4 / 3.8 / 9.0 / 21.5 / 84.6 | 2.4 / 5.8 / 23.6 / 94.4 | 0.24 / 0.71 / 2.2 / 5.5 |

**P1 (registered, LEAD).** b0p(arm) / b0p(calibration n₀ row), per-arm medians, is:
- ≥ 3 at c2 W 384 (predicted ≈ 10);
- ≥ 3 at c8 W 512, leg 0 (≈ 10);
- ≥ 3 at c1s W 2 048 (≈ 60).

**Its meaning, stated now.**
- "The first K windows" is a valid b̂₀⁺ source **only under a boot held at a queue-free store**.
- Otherwise the start-up queue is latched into the law's target: rev 3's boot-latched σ̂⁺ failure, here in b̂₀⁺.
- R6.2's line has no boot hold. Its "n₀ = 128 at or below the BDP" was an assumption about where the closed loop
  starts, not a term of the law.
- A P1 PASS therefore makes the boot hold (hold the store at n₀ until K windows close) and n₀ itself part of the law.
  That is recorded as an open derivation item, with the R5.5 erratum: `store-law.md` R6.7.
- **If P1 fails at a cell** (ratio < 3), the boot windows were not queued. Printed R is then that cell's scored R, as
  the brief first asked, and R̃ is reported. P1 is read per cell, over all unflagged rows, before any crossing.

**P2 (printed R's signs; reported, decides nothing).**
- R − 1 ∝ min(μ, b0p) + b0p + σ + q·ℓ̂ − x, with x = T̂min − ℓ̂ and μ = m̂ − ℓ̂. So printed R stays above 1 until the late
  queue outgrows twice the 2 s boot queue, which is a start-up dynamic.
- Predicted: printed R > 1 at every c8 and c1s arm, controls included, and at every c2 arm ≤ 384. At c2 2 048 it is
  row-dependent (§56: 2 of 3 control rows had boot queue ≲ half the late queue).
- On printed R, then, the sign gate would stop the session at c8 and c1s, and B2 could not fire. That is why R̃ is
  scored.

**ℓ̂ is contaminated at c2 and c8, so the ruling's exception clause applies there.**
- Within one pool the row spread is wide: c2 `l` spans 6.30–9.09 at P128.
- Across pools, however, the rank order rises with W. Row-level Spearman of `l` against W (`s60ell.out`, one-sided
  permutation p):

  | ledger | c2 | c8 leg 0 | c8 leg 1 | c1s |
  |---|---|---|---|---|
  | s56 | +0.58 (p 0.007) | +0.46 (p 0.014) | +0.77 (p 0.0001) | +0.03 (p 0.43) |
  | p27 | +0.70 (p 0.001) | +0.43 (p 0.046) | — | +0.55 (p 0.015) |

- The mechanism is P1's: the store fills before the path's no-queue samples have accumulated, so the lifetime
  minimum carries queue too.
- **Recipe consequence (§1.4):** at c2 and c8, ℓ̂ also comes from the same calibration row. At c1s, where the
  current-era rows are flat (§57b also read c1d's ℓ̂ flat at 2.52–2.62), the row's own `l` is kept. p27's c1s trend
  is the older era's, and is reported.
- This follows the ruling's own clause ("unless a prediction shows them contaminated"). Its default (own ℓ̂) is
  scored as a **sensitivity**.
- Per-arm `l` medians and this rank test are re-run on s60's rows and reported.

**The size of the contamination, in ms** (arm medians of `l`, leg 0 unless noted):

| cell, ledger | W: `l` (ms) |
|---|---|
| c2, §56 | 128: 8.07 · 160: 8.87 · 192: 9.66 · 256: 9.52 · 384: 10.21 · 2 048: 10.13 |
| c2, p27 | 64: 6.86 · 96: 7.92 · 128: 8.77 · 192: 10.18 · 256: 10.07 · 384: 10.75 · 512: 11.44 · 1 024: 11.17 |
| c8 leg 0, §56 | 160: 8.22 · 224: 9.42 · 320: 11.12 · 512: 10.87 · 768: 10.31 · 1 280: 11.13 |
| c8 leg 1, §56 | 160: 31.64 · 224: 31.46 · 320: 34.38 · 512: 34.71 · 768: 35.88 · 1 280: 35.33 |
| c8, p27 | 128: 8.10 · 192: 9.33 · 256: 11.80 · 384: 9.96 · 512: 10.88 · 768: 11.23 |

**Which statistic this is, and whether it is the printed field.**
- §56: the minimum over the row's 250 ms `[DIAG]` ticks of `rtp_us`, i.e. Copa's `min_rtt`, a 10 s windowed minimum
  of the echo samples. The minimum over all ticks is the lifetime minimum of the series Copa sees, the first tick
  included.
- p27: the minimum over the row's `[LOOPDIAG]` windows of `rtt_min_us`, the window echo minima.
- Both are lifetime minima from the first sample, not boot-tick or first-2-s values. The gauge's `l` is the same
  quantity: `WinSampler::life_min`, a running minimum from the first sample.
  - The sampler is attached on the first sender-loop pass under `RWM_DIAG`, so it starts with the row.
  - Read at the LATE windows, it equals the row-end minimum, since the minimum is reached early.
- The test therefore touches the printed field's definition. One assumption remains: Copa's series and the
  sampler's are the same echo samples.
  - **Witness:** on every G row, printed `m` = printed `l` on every window with t < 10 s (Copa's window has not yet
    expired). The mismatch fraction is reported, and a non-zero one voids the §56 transfer of this argument.

**P3 (registered prediction on printed `l`).**
- **Quantity:** Δℓ = (median printed `l` at the arm) − (median at the W 128 calibration rows), per leg.
  - SE_Δ = 1.253·√(s²_arm/n_arm + s²_cal/n_cal), from the row sds. They reach 1.4 ms (c2 P128: 6.30–9.09).
  - The threshold is the leg's q·ℓ̂_c, the law's allowance: ≈ 0.8–0.9 ms at c2 and c8 leg 0, ≈ 3.2 at c8 leg 1.
- **Verdicts:**
  - **RISES** iff Δℓ − 2·SE_Δ > q·ℓ̂_c;
  - **FLAT** iff Δℓ + 2·SE_Δ < q·ℓ̂_c;
  - **UNRESOLVED** otherwise.
- **Predicted:**
  - c2 at W 384: Δℓ ≈ +2.1 ms (§56 10.21 − 8.07; p27 10.75 − 8.77) and 2·SE ≈ 1.7, so RISES or UNRESOLVED.
  - c8 leg 0 at W 512: ≈ +2.7, so RISES. Note §56's c8 baseline is W 160 (P80); W 128 is unmeasured.
  - c8 leg 1 at W 512: ≈ +3, which is under that leg's own allowance (3.2), so FLAT or UNRESOLVED.
- **Meaning of RISES:** the law's ℓ̂ source (the lifetime minimum) fails when a flow starts into queue, the same class
  as P1's b̂₀⁺. It goes to `store-law.md` R6.7.
- **If FLAT at a cell,** the two recipes coincide there within the allowance. The scored and own-ℓ̂ R̃ are then
  reported as one reading.

### 1.3 The anchor bind adds one b̂₀⁺ to the target

F = min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺. With m̂ ≥ ℓ̂ + b̂₀⁺ (`anc = 1`), the target gains a full b̂₀⁺, not the over-read.

On 600 MB rows, Copa's 10 s minimum tracks the queue (`s60boot.out` `lm`):
- c2: μ 2.7 ms at W 192 and 6.5 at 256;
- c8 leg 0: 1.9 at 320 and 10.5 at 512;
- c1s: 0.27–0.96 at 512–2 048.

So the bind is marginal at c2 W 192 and at c1s. Under the scored recipe, c8's band holds the anchor fixed, while c2's
spans both states (§1.4). `anc` and `ancf` decide in-session.

Shift anc 0 → 1 (`s60band.out`, method B, scored recipe):
- c2: 158–203 → 188–242 (+30 to +39);
- c8: 403–505 → 433–551 (+30 to +46).

### 1.4 R̃: what it is, what it tests, and its witness

**What R̃ tests.**
- R̃ is the law fed a b̂₀⁺ that the flow did not measure itself: the b0p of a different row, the replicate's
  calibration row. R5.4 was withdrawn as an oracle for exactly that kind of input.
- So a **B2 fire refutes the target FORM**, F + σ + q·ℓ̂ with the coupling term, given a queue-free b̂₀⁺.
- **A pass says nothing about the law's own b̂₀⁺ input.** Whether the flow can obtain that value itself is P1's
  question, and the boot hold's (§1.2).

**The recipe, frozen.**
- **Substituted:** per leg, the `b0p` of **the same replicate's** calibration row (§1.5). At c2 and c8 its `l` is
  substituted too (§1.2); at c1s the row's own `l` is kept.
- **The row's own printed fields** supply every other input: `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `sig`
  (σ at load) and `q`.
- The coupling term is recomputed from the inputs in use.
- **The calibration-ℓ̂ recipe carries B2's verdict alone** (ruling entry 5).
  - R̃ with the row's own `l` is computed and reported beside the verdict. It does not veto it.
  - A veto would remove the falsifier: at c2 the own-ℓ̂ band (182–258) starts above the 169 knee, so the two recipes
    could only ever agree on a pass.
  - **The named outcome:** if the scored recipe fires B2 and the own-ℓ̂ R̃ does not, then **"the form fails on honest
    floor inputs, and the flow's own ℓ̂ masks it"**. That outcome goes to R6.7 as evidence on the ℓ̂ source.
  - At c8 neither band reaches the knee (W₉₅ 340, against low edges of 433 and 462), so this rule is idle there.

**What the row still supplies (the anchor term).**
- Under substitution, F = min(m̂_row, ℓ̂_c + b0_c) + b0_c. If ℓ̂ is contaminated, m̂ is at least as much, since
  m̂_row ≥ ℓ̂_row.
- The row's m̂ is selected only if m̂_row − ℓ̂_c < b0_c, where m̂_row − ℓ̂_c = (ℓ̂_row − ℓ̂_c) + μ_row. One calibration
  row feeds each replicate, so this is checked **row by row** (`s60boot.out` per-row `l` and `lm` ranges):
  - **c2:** ℓ̂_c can be 9.09 ms (a P128 row), while a W 192 row reads `l` 7.35 with a lift of 1.21. That gives −0.53 ms,
    below b0_c (1.27–3.09). So **the row's m̂ can enter at c2**, even below ℓ̂_c.
    - On arm medians the lift reads 4.3 ms at W 192, which a previous pass took as binding. That was wrong.
  - **c8 leg 0:** ℓ̂_c ≤ 8.49 (P80 rows; P64 is unmeasured). At W 320 the rows read `l` ≥ 10.72 with a lift ≥ 1.05,
    giving ≥ 3.28 ms, above b0_c ≤ 2.90. At W 512 the margin is ≥ 7.9.
- **So the anchor is fixed (anc = 1) only at c8, over its band region (W ≥ 320), and only thinly** (a 0.4 ms margin
  at W 320).
  - There F = ℓ̂_c + 2·b0_c is **all calibration**, and the row supplies only T̂min (`tmin`), T̂ (`tp50`), σ (`sig`)
    and w (`nk`).
  - At c2 the row's m̂ still enters, and the band keeps both anchor states.
- **Reported:** the replay's per-window anc bind fraction under substitution, per arm.
  - Predicted: ≥ 0.9 at c8 W ≥ 320.
  - If it is below 0.9 at a replicate's crossing bracket, that replicate is read against the anc 0 corners and
    flagged.
- `slaw_r` is transcribed in `s60replay.py`. **`Rb` is not scored**: its σ is the max over the same boot windows,
  so it carries the same latch.
- Per row, R̃_row is the median over LATE windows (t ≥ 15 s at c2 and c8, ≥ 5 s at c1s).

**Witness: a PER-ROW gate.**
- On every G row, `s60replay.py witness`, replaying with the row's **own** b0p, must reproduce printed `R` and `Rb`
  on every defined window to 6·10⁻⁶. Every input is printed exactly.
- A failing row is VOID for every R reading.

**Demonstrated** (`s60replay.out`):
- (a) On the gauge's renderer output (`tools/l1/testdata/slaw_sample.log`): two legs, anc 0. PASS, worst 4.9·10⁻⁶.
- (b) On `s60witness_ka.log`, which is **hand-formatted** in the renderer's line format: slaw.rs's N = 2 anchor case
  (R = 1.11625, as the engine test asserts), with leg 1 anchor-bound, a third leg at nk = 0, and a warm-up window
  that must be cut. PASS.
- Negative control on both: every `tmin` + 1 µs ⇒ FAIL (|ΔR| 2.1·10⁻⁴ and 3.8·10⁻⁵).
- **Pre-launch item:** a renderer-generated sample with an anchor-bound leg and an empty leg, via `SLAW_SAMPLE`
  from `render_window` on a cargo-capable host, must pass the same witness and negative control.

### 1.5 Calibration: two small pools per cell, queue-free, at n₀ = 128

**Queue-free, by arithmetic and as measured.**

| cell | wire BDP (rate × ℓ̂) | echo-loop BDP (rate × T₀) | n₀ = 128 | measured |
|---|---|---|---|---|
| c2 | 10.2 k/s × 10.0 ms ≈ 102 | × 16.0 ms ≈ 163 | above the wire BDP, below the echo-loop BDP | p27 b0p flat over caps 64 / 96 / 128 (2.87 / 2.46 / 2.18), late T̂min − ℓ̂ flat at 2.2–2.6 ms; §56 W 128 goodput 0.79 of control (not saturated) |
| c8 | 9.8 k × 10 + 1.9 k × 40 ms ≈ 174 | larger | below | p27 cap 128 b0p 2.07 / 2.90 (leg 1 9.4 / 6.1); late queue 2.4 ms |
| c1s | 83–93 k/s × 2.5 ms ≈ 208–243 | × 4.0 ms ≈ 330–370 | below | **no ledger has c1s ≤ 192 except one p27 192 row pair, of which one row collapsed** |

The ruling's 104 and 208 are reproduced. W 128 is the law's provisional n₀, so the R̃ calibration row is at **W 128
at every cell**.

**The second pool is W 96 at every cell** (c8: P48). It is the flatness check only, and stands for no other n₀.
- It lies **below** n₀ because the question is whether n₀ itself sits on the queue-free plateau. A pool above n₀
  would test the next pool.
- Both 96 and P48 bind: `STORE_CAP_FLOOR` = 10 (`sender_policy.rs:79`), and the ceiling is max(N·pool, floor).

**Calibration validity gate (per cell).**
- **Rule:** PASS unless |Δ| − 2·SE_Δ > q·ℓ̂₀, per leg, where:
  - Δ = (median non-latched `b0p` at W 96) − (the same at W 128);
  - SE_Δ = 1.253·√(s₉₆²/n₉₆ + s₁₂₈²/n₁₂₈), from the rows' sds;
  - ℓ̂₀ = the median `l` of the W 128 rows.
- **Reasons.** A difference inside the law's allowance cannot move its target by more than δ's seat. The noise term
  keeps max-of-8 scatter (c2 cap-128 rows 1.27 / 3.09 ms) from failing a flat cell.
- **Predicted per cell:**
  - c2: PASS. p27 96 vs 128 gives Δ ≈ +0.28 ms against q·ℓ̂₀ ≈ 0.88.
  - c8 and c1s: **unmeasured.** No ledger has c8 below cap 128, or c1s at ≤ 128. At c8, the step 128 → 192 above n₀
    reads +1.5 ms (unlatched rows), so the plateau may end near n₀.
- **When it is read:** provisionally at each replicate-2 gate read, on the calibration rows so far; finally over all
  rows, before any crossing is scored.
- **A cell failing the gate has no R̃.** Printed R and the readouts are reported, and the evidence goes to R6.7. At
  c1s this costs the control-sign verdict (§2.4).

**Latched boots.**
- Max-of-8 latches start-up transients even at small pools. Measured:
  - p27 c8 cap 192: b0p 53.1 / 3.96 ms, leg 1 49.7 / 6.4;
  - §56 boot proxies: c2 P160 1 of 3, c8 P80 1 of 4, c8 P112 1 of 4 (leg 0);
  - none on p27 c2 caps 64–128 (6 rows) or c8 cap 128 (2 rows).
  - That is ≈ 1 in 12 rows at c2 and ≈ 1 in 4 at c8, any leg. The latch rate is itself evidence for P1's meaning,
    and is reported per cell and leg.
- **Rule, in printed fields:** a calibration row is LATCHED iff, on some leg,
  b0p − median_LATE(brake) > q·ℓ̂ + ō, where brake = `err` + b0p + q·ℓ̂ (printed `err`).
  - ō is the cell's declared no-queue over-read bound (`b0bound.out` max_row cost medians): c2 0.90 ms, c8 leg 0
    1.8 and leg 1 6.3, c1s 0.20.
- **W 128 rows per replicate: two at c2 and c1s, three at c8.**
  - The replicate's calibration is the first non-LATCHED one, in run order.
  - If all latch, the replicate has no R̃. It is counted as LATCHED, not VOID.
  - **Why c8 gets a third row** (ruling entry 5, item 5). The earlier 0.75 assumed two rows and a 0.9 bracketing
    validity. §56 suggests 0.97 (0 of 24 c8 rows flagged). At a 25 % latch rate, P(≥ 5 of 6 replicates scoreable)
    is:

    | n₀ rows | P(calibration) | at validity 0.9 | at validity 0.97 |
    |---|---|---|---|
    | 2 | 0.94 | 0.76 | 0.90 |
    | 3 | 0.98 | 0.85 | 0.97 |

    The third row costs 6 rows (≈ 9 min worst) and is bought.
  - At c2 (latch ≈ 1 in 12) two rows give P(calibration) ≈ 0.99.
- **c8 leg 1 at the calibration pools.** c8's coupling term now comes from leg 1's calibration values.
  - **Leg 1's boot windows are sometimes empty.** On §56's four P80 rows (W 160), differencing the cumulative `n` of
    `[DIAG]` p1 `sig_us/n` gives per-window counts mostly of 14–137 samples, against ≈ 2 000 for leg 0.
    - The last boot window held **1 sample** on `s7-r2` and **0** on `s7-r4`.
    - W 128 and 96 will be thinner still.
  - **What that does to the gauge's leg-1 `b0p`.**
    - The gauge counts K over windows *with* samples, so a 0-sample window stretches K past 2 s, into windows that
      may already carry queue.
    - A 1-sample window counts, and its "minimum" is one sample, which can latch high.
    - A thin window's minimum over-reads in general, consistent with leg 1's larger `b0p` (6.1–9.4 ms against leg 0's
      2.1–2.9).
  - **Registered source of leg 1's calibration values.**
    - b0₁ is the maximum of the printed `b0k` brakes whose `b0kn` ≥ 10, and needs ≥ 4 such windows of the K.
    - ℓ̂₁ is the row's printed leg-1 `l`, a lifetime minimum over all samples and robust to thin windows.
    - A row with fewer than 4 qualifying windows is THIN for leg 1. The replicate's next calibration row is used; if
      all are THIN, the replicate has no R̃ (counted THIN).
    - Leg 1's `b0kn` and the THIN count are reported.
- **c1s validity is boot-only.** A c1s calibration row is valid for b0 if its `[WSTAT]` windows with t < 2 s show no
  qcwnd below the cell's BDP. The parser's absolute collapse rule (`slaw_parse.py --bdp-bytes`) is restricted to
  those windows, because `qcol` only counts from t ≥ 1 s and b0p uses the first 2 s.
  - A row that collapses later still calibrates, but cannot score B1.
  - Its LATCHED test then uses the median brake over windows from t = 2 s up to the first collapse-flagged window,
    and needs ≥ 8 such windows. With fewer, the row's latch status is UNREAD and the row cannot calibrate.
  - c1s calibration rows run **1 GB**, since only their boot windows and LATE `err` are read. That is a new
    per-arm bytes feature for the harness.

**B1 and the calibration-end sign.**
- B1 fires iff, on a calibration row, the median LATE `err` > 0. Predicted not to fire: `b0bound.out` max8 margins
  are +0.43 / +0.46 / +0.19 ms.
- R̃ on the calibration row equals printed R, because the pairing feeds it its own b0p.
- When B1 does not fire, brake ≤ b0p + q·ℓ̂. With anc 0 that gives
  R − 1 = (b0p − brake + σ + q·ℓ̂)/T̂ ≥ σ/T̂ > 0 (single leg; window by window, so also for the median).
  - The stronger (σ + q·ℓ̂)/T̂ needs brake ≤ b0p.
- So **the calibration-end sign holds by construction** whenever B1 does not fire. It is not a gate.

### 1.6 The frozen bands (Bulk, q = 0.10, recipe of §1.4)

`s60band.py` propagates every input over its measured range, with both anchor states, through two readings of T̂min:
- **A:** §56's central curve at ±0.5 ms integer-ms resolution, plus R6.1's 0.1 ms c1s curve;
- **B:** p27/p34 window minima.

**Its inputs**, all over single-row ranges because one calibration row feeds each replicate:
- σ at load;
- b0 at cap 128, plus the over-read from 0 to the ledger max;
- ℓ̂ from single cap-128 rows; method B compares p27 with p27, since absolute echo floors differ between ledgers;
- c8's coupling ½Δ from one calibration row's legs (13.1–15.7 ms);
- c8's leg-1 pull (w₁/w₀)·D₁ (1.4–3.2 ms).

The b0 and ℓ̂ corners rest on 2 rows per cap.

**The anchor state, checked row by row (§1.4; `s60band.out`, "FROZEN").**
- c8 is anc = 1 over its band region, so its band is the anc 1 corners.
- c2 keeps both states, because a row's m̂ can enter.

| cell | KA point | **band (frozen)** | own-ℓ̂ sensitivity, like for like | in-session knee (§56) |
|---|---|---|---|---|
| c2 | 194.2 | **158–242** (both anchor states) | 182–258 (both states) | W₉₅ 169: inside the band, 11 slots above its low edge |
| c8 | 478.2 | **433–586** (anc 1) | 462–605 (anc 1) | W₉₅ 340: 93 slots below the band's low edge |

The recipes differ by 24 slots (c2) and 29 slots (c8) at the low edges, and by 16–19 at the tops.

**B2's resolution, in R units, and why not in slots.**
- **The event:** "the crossing lies below the knee", with both interpolated per replicate. It is equivalent to R̃ < 1 at
  the replicate's own knee.
- **Registered per replicate:** replicate r counts iff R̃_r(W₉₅,r) < 1 − 2·σ_R.
  - R̃_r(W) is interpolated linearly between that replicate's arms bracketing W₉₅,r.
  - W₉₅,r is interpolated from each row's goodput against the same replicate's control.
  - σ_R is the in-session sd over replicates of R̃_row at those bracketing arms.
  - B2 fires on ≥ 2 of the 5 or 6 scoreable replicates. One rule holds at both cells.
- **Why R units.** A slot margin is ill-conditioned wherever R̃ is flat in W, and below the c2 knee it is nearly flat.
  - p27's Rcom falls only 0.0015 per slot from W 128 to 192 (1.183 → 1.090; `s60p27.out`), against 0.0036 per slot
    from 192 to 256.
  - The knee's own error is small: §56's goodput row sd of 0.24–0.30 % gives σ_W95 ≈ 3 slots at c2 and ≈ 4 at c8.
- **A priori σ_R at c2.**
  - From p27's row pairs (Rcom, which uses each row's own `l`): sd 0.018 at W 128 and 0.046 at 192, pooled ≈ 0.035.
  - Under the scored recipe, the floor scatter does not cancel. ℓ̂_c − ℓ̂_row varies by ≈ √2 × 1.4 ms across rows,
    which is ≈ 0.10 in R at T̂ ≈ 19 ms.
  - So **σ_R ≈ 0.04–0.11**, and the per-replicate threshold is R̃ < 0.78–0.92.
- **The slot equivalent (ruling item 4):** 2·σ_R / slope = **55–150 slots at c2**.
  - A replicate would have to cross below ≈ 114 or lower. That is below the W 128 calibration arm, where R̃ > 1 by
    construction (§1.5).
  - **So B2 at c2, as a per-replicate count, is ill-conditioned: it cannot fire on the a priori σ_R.**
  - The cause is not the veto the ruling removed. It is that R̃ hardly moves with W below the knee, while the
    calibration-ℓ̂ recipe adds floor scatter.
  - **Coordinator's ruling needed.** The alternative, registered as a REPORTED readout: the median over replicates of
    R̃_r(W₉₅,r) below 1 − 2·1.253·σ_R/√n_s. At n_s = 6 that is 0.041–0.11 in R, or 28–77 slots, i.e. a crossing below
    ≈ 92–141.
    - It is decidable at c2 only if σ_R is near its low end. It would replace the count rule if ruled so.
- **c8:** p27's 9 s rows cannot size σ_R. Their row pairs differ by 0.2–0.6 in R at W 256–768, being shorter than
  Copa's 10 s window and leg 1's settling. σ_R is in-session only.
  - B2 at c8 additionally needs the crossing ≥ 93 slots below the band.

c1s carries no crossing band (§2.4). Its corners (406–981) are kept in `s60band.out` as context.

**Scoring.** A cell's crossing prediction is IN iff the median over scoreable replicates of R̃'s W_r lies in the band.

**What a crossing outside the band means.**
- **c2 below 158.**
  - Every propagated corner is exhausted: the low b0 row, low σ, anc 0, both T̂min readings.
  - So the window minimum rises faster with W than both the central curve and p27, or the calibration row's ℓ̂ is
    high against the arms. The per-arm `tmin − l` and `l` readouts say which.
  - B2 fires if it is also below the in-session W₉₅: the target form lands on the cliff.
- **c2 above 242.**
  - The target exceeds every corner: anc 1 with a large b0, or σ at load rising with W beyond the measured range.
  - The law sits higher on the flat region. That costs no goodput, but adds standing queue: q at W_r is reported
    against q₉₈.
  - It is an over-allowance finding against R5.5's frozen landing (R5.5: a margin of 17–33 slots above the knee),
    not a refutation.
- **c8 below 433.**
  - Leg 0's coupling ½Δ or leg 1's pull is smaller than measured, which bears on the ½ form's magnitude under
    symmetric shaping (B5's subject in B-2).
  - B2 fires only if it is also below W₉₅ (≈ 340).
- **c8 above 586.**
  - Leg 1's pull is larger than 3.2 ms, i.e. leg 1's σ at load grows faster than §56 read. That is the question B6
    asked, answered from the leg-1 pull readout (§2.3).
  - The law parks c8 at a larger store, a delay cost reported as q and in-order delay at W_r.

---

## 2. Arms and falsifiers

All arms are static pools through `RWM_STORE_PATH_POOL` (per path; c8's P = W/2), on Bulk, as in §56.

### 2.1 Arm lists (per replicate; arm order rotated per replicate, seeds alternating 42 / 7)

| cell | rows per replicate (total W) | replicates |
|---|---|---|
| c2 (600 MB) | **128 ×2** (cal n₀), 96 (flatness), 160, 176, 192, 224, 256, 320, 384, **2 048** (ctl) | 6 |
| c8 (600 MB) | **128 ×3** (cal n₀, P64), 96 (flatness, P48), 256, 320, 384, 448, 512, 576, 640, **1 280** (ctl, P640) | 6 |
| c1s | **128 ×2** (cal n₀, 1 GB), 96 (flatness, 1 GB), **2 048** (ctl, 5 GB) | 12 |

- **c2:** the knee arms are 160–192, with 16-slot steps where 32 slots cost 6 %. The band arms (158–242) are
  160–256, and 320 brackets the sensitivity's top.
- **c8:** the knee arms are 256–384, and the band (433–586) is covered by 384–640 at 64-slot steps. B2 at c8 needs a
  gross miss (§1.6), and the arms read either outcome without extrapolation.

### 2.2 Crossing per replicate

- **W_r:** the linear interpolation between the last arm with R̃_row > 1 and the next arm with R̃_row ≤ 1. It uses
  unflagged, witness-passing rows of a replicate that has a calibration.
- **A flagged row at the sign change:** bracket with the nearest valid arms, and report the width.
- **More than one sign change:** the replicate is NON-MONOTONE. It is reported, and W_r is its first downward
  crossing. R6.2's uniqueness prediction is one sign change per replicate. Per-row `xs`/`xb` count window flicker on
  a static pool and are reported only.
- **Scoreable cell:** ≥ 5 of 6 replicates.
- **Knee in-session:** W₉₅ and W₉₈ from the arm-median goodput against the control (§56 definitions), reported per cell. For B2 the knee is taken **per replicate** (W₉₅,r: each row's goodput against the same replicate's control, interpolated in W), and B2 reads R̃ at it in R units (§1.6).

### 2.3 Falsifiers

| # | what fires it (printed fields only) | what it means |
|---|---|---|
| B1 | on a calibration (W 128) row: median LATE `err` > 0 | the collapse condition: b̂₀⁺ + q·ℓ̂ below the row's own later no-queue brake |
| B2 | at c2 or c8: R̃_r(W₉₅,r) < 1 − 2·σ_R (per-replicate knee, in-session σ_R; §1.6) on ≥ 2 of the 5 or 6 scoreable replicates, read on the scored (calibration-ℓ̂) recipe alone; at c2 ill-conditioned a priori, see §1.6 (R4.8's "more than 1 in 6") | the target FORM lands below the knee, given a queue-free b̂₀⁺ (§1.4) |

- **B1 off the calibration arm** is not evaluated: later windows carry queue there, and m̂ tracks it.
- **B6 is dropped and replaced by a readout.** On §56, c8 leg 1's σ at load over its boot value is 0.24–0.72, so it
  cannot fire, because the boot max carries the fill transient. Against the calibration arm's load σ (4.92 ms) it is
  already 3.1× at W 512, 5.3× at 768 and 6.4× at 1 280.
  - **Readout:** per arm, leg 1's `sig` against calibration, and leg 1's pull on R̃, w₁·D₁/ΣwT̂.
- **B3–B5 are not in s60.** B3: c1d ran as §57b, and a re-run needs its own registration. B4 and B5 are B-2's.
- **No decision rule.** A closed-loop arm is registered only after B1, B2, P1 and R6.7 are read.

### 2.4 c1s: the control sign only

c1s's crossing band covers the sweep, so no rule could fire on it. The one thing c1s can decide is **the sign of
R̃ − 1 at the control** (W 2 048):
- R̃ ≥ 1 means the law, at its own target, would keep growing the store to its clamp N·knee. That is a **PINNED**
  finding under FORMULA-FIRST, and needs a verdict in the law ledger (`law-ledger.md`, row F01).
- R̃ < 1 means it **CROSSES**: the law lands inside the pool range.

**Prediction: the sign is open.**
- p34/p27 minima (prefix minima, over-reading T̂min) give R̃ − 1 ≈ −0.2.
- §56's exact Copa lift at 2 048 (μ = 0.96 ms) against an anchor-bound target 2·b0 + σ + q·ℓ̂ ≈ 1.4–1.7 ms gives
  R̃ − 1 ≈ 0 if the late brake is ≲ 0.5–0.8 ms.
- So a SPLIT is likely, and is a registered outcome.

**Verdict.**
- **Scoreable replicates:** an unflagged, witness-passing control G row and a valid calibration row in the same
  replicate.
- **Sign test (two-sided, 0.05) on R̃_row − 1:**
  - PINNED iff R̃ ≥ 1 on ≥ k of n_s replicates;
  - CROSSES iff R̃ < 1 on ≥ k;
  - otherwise SPLIT.
  - k = 6 / 7 / 8 / 8 / 9 / 10 / 10 for n_s = 6 … 12; n_s < 6 is UNDERPOWERED.
- The median R̃ − 1 and its range are reported.
- **Sizing:** 12 replicates at P(scoreable) ≈ 0.64 (0.67 unflagged control × 0.95 valid calibration) give
  P(n_s ≥ 6) ≈ 0.90.
- **Discriminator readout:** the control's median `tmin − l`. Against the anchor-bound target of ≈ 1.4–1.7 ms it shows
  which T̂min reading holds.

---

## 3. Perturbation control (G / C on one binary)

- **The binary and the arms.** One gauge build, one sha recorded in every row. **G** = `RWM_SLAW=1`; **C** =
  `RWM_SLAW=0`. Both set `RWM_DIAG=1`.
- **Every gauge site on `2547fc07` is gated by `slaw`:**
  - `note_gap_report` / `sack_gaps_truncated`: `gates.diag && gates.slaw`, `control_msg.rs:732`;
  - `note_c09_*` and the three `note_debt_*`: `pol.slaw_on` (`sender_phases.rs:816, 862, 1234, 1401`;
    `emit_source.rs:697, 704`);
  - `note_adv` / `note_release`: `dg.slaw` (`sender_phases.rs:1346`, `mod.rs:2195`), armed only under `pol.slaw_on`;
  - the `[CAPLOG]` recorder and `WinSampler`: constructed only under `pol.slaw_on`.
  - C rows still execute the flag and `Option` checks, one per echo sample in `record_rtt_sample`. These are identical
    with `RWM_DIAG` off, and G/C does not bound them.
- **Predicted cost.**
  - **Benched** (`bench_close_23k`; the figures are the coordinator's, from the gauge's bench run, which is not
    committed on `slaw-gauge`): 14 ns per observed sample and 0.22 ms per window close at 23 000 samples per 250 ms
    window. That is (23 000 × 14 ns + 0.22 ms)/250 ms = **0.22 % of one core** on the c1s sender.
  - **In CPU s/GB:** a GB takes ≈ 9.5 s at c1s (≈ 840 Mbit/s), so the gauge adds ≈ 0.021 s/GB against the client's
    15.8–17.2 s/GB, i.e. **≈ 0.12 % of client CPU/GB**.
  - **Unbenched: printing.** That is 4 lines per 250 ms per leg ([WSTAT], [SLAW], [WSTORE], [SLAWC]), plus `[CAPLOG]`.
    `[CAPLOG]` logs every 5 ms refresh whose inputs moved, so up to 200 lines/s if Σbdp moves on every refresh; this
    is the main unbenched cost. Its rate (lines/s) is a smoke readout.
  - **Registered prediction:** G/C client CPU/GB within +1 % at c1s. If the smoke's `[CAPLOG]` rate exceeds 50 lines/s,
    that prediction is restated before 60c, from the measured rate.
- **60c is a gross-error check** that rules out a cost above 10 %, since the predicted cost is ~0.1 %:
  - **10 launched pairs** at the c1s control, interleaved, with order alternating.
  - Sized from `s60size.out` (pair sd 0.040 CPU, 0.057 goodput; t quantiles): 4 unflagged pairs resolve 10 % in CPU
    s/GB, and 5 resolve it in goodput.
  - P(≥ 4 unflagged of 10) = 0.83 / 0.73 / 0.27 at collapse 0.29 / 0.33 / 0.48.
  - **Gate:** the median log(G/C) of client and server CPU s/GB. The gauge is PERTURBING iff it exceeds +10 %.
    UNRESOLVED below 4 unflagged pairs, reported with the realised MDD.
  - Goodput is reported with its MDD.
  - The 5 % gate of entry 3 (24 pairs) is not worth a launch against a predicted 0.1 %.
- **Re-base:** C rows against §56's c1s control (goodput 875 [727–887]; client CPU 15.82 [15.28–18.99] s/GB). G
  controls at c2 and c8 against §56 (c8 via the §54b B640 re-base).
- **Collapse share** G against C is reported. A gap above 0.25 marks the pairs BIASED.
- **If PERTURBING:** G's c1s readings are reported as perturbed (R5.6).

---

## 4. Witnesses

1. **`RWM_SLAW` is scrubbed and set.** The harness adds `-u RWM_SLAW -u RWM_THREE_TERM` to its env scrub, then sets
   `RWM_SLAW=1` or `=0` explicitly per arm. `[GATES]` echoes it on **both** ends of every row (`gates.rs:453`). A
   mismatch voids the row, which is re-run once.
2. **Smokes.**
   - **60a: c2 W 192, one C row and one G row.**
     - C carries `[DIAG]` and none of `[WSTAT]`, `[WSTORE]`, `[SLAW]`, `[SLAWC]`, `[CAPLOG]`.
     - G carries all five, `R` from window K = 8 on, the per-row witness PASS, and the CAPLOG replay PASS.
     - The measured run's `kw` counts from 1, and its `b0k` prints after the last warm-up marker.
     - The `[CAPLOG]` rate is reported.
   - **60c: one c1s W 128 G row (1 GB) and one c1s W 2 048 C row.** The W 128 row is the first measurement of c1s at
     n₀: its b0p, boot collapse check and LATE `err` are reported before the c1s block runs.
   - A smoke failure stops the launch.
3. **`[SLAW] q`** reads `0.100000` on every Bulk G row, and `0.050000` on plan 59's Realtime rows. Any other value voids
   the row.
4. **`[CAPLOG]` replays every logged cap.**
   - On the shipped seat, every refresh logs `seat=plain`, with `br=pooled` or `br=boot` before the first BDP sample
   (`sender_phases.rs:235–262`). The default is `RWM_QUIC_CC=bbr` with no Copa feed, and `RWM_THREE_TERM` is off.
   - `tt` cannot occur, since the scrub prevents it, so the gate is not empty.
   - `caplog_replay.py` must reproduce every cap with 0 mismatches.
   - On static pools the cap binds at N·pool, so the **bind fraction** (FORMULA-FIRST) is reported per arm, beside
     the unclamped gain·Σbdp.
5. **The R-sign gate (on R̃) at c2 and c8: R̃_row < 1 at the control.** The calibration end holds by construction
   (§1.5).
   - It is read after replicate 2, on unflagged rows. If none is unflagged, it is read at the first later replicate
     that has one; if still unread after replicate 4, it is UNREAD and the cell is flagged.
   - Predicted R̃ − 1 at the control: c2 ≈ −0.9; c8 ≈ −0.2 to −0.6. At c8, leg 0's Copa minimum lifts only 33 ms at the
     control, against window minima up to ≈ 85.
   - **A failure stops the session (ABORT-RGATE) and is amended.**
   - At c1s the control sign is the measurement (§2.4), not a gate.
6. **Bit-identity.** The gauge's host cap-sequence test and the native golden test, with `GOLDEN_CAPTURE` unset,
   pass on the built tree, including the K4 print (§5).
7. **Host and order.** Host cargo is paused, with the build lock held by the main session through every launch. Arm
   order rotates per replicate, and the session-position readout is reported.

---

## 5. The evicting rows (plan 59) on the gauge build

- **Rows.** Plan 59's cell and arm, unchanged: c2-100, 600 MB, Realtime, no `--window-reliable`, n = 5
  (seeds 42 / 7 / 42 / 7 / 42), each VOID slot re-run once. Env `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150`, plus
  `RWM_SLAW=1`.
- **m̄ = Δ`cum` ack / Σ `adv` count over LATE** (t ≥ 5 s).
  - `adv` p50 and p90 are reported as medians over windows.
  - **Witness:** plan 59's `mbar_lo` (Δcum/Σ`wait n`) ≤ m̄.
- **W1 per window = `dbt_share` = `dbt_em`/`dbt_acc`.** The row's W1 is Σ`dbt_em`/Σ`dbt_acc`.
  - **Witness:** it lies inside plan 59's bracket.
- **K4 runs on per-window values.** The earlier Realtime row ran A\* 1–201 and rr 0.004–0.5 within one row.
  - Steady windows: ≥ 1 refresh, A\* min = max, Δ min = max.
  - A row with < 30 steady LATE windows is K4-UNSCOREABLE.
- **K4 needs σ²**, because the GE channel's p and q come from (ε, σ²). The engine computes σ² only at
  `emit_source.rs:597` and prints it nowhere. **One print is added to the gauge before the build:**

  ```text
  [WSTORE] … fec=<nref>/<eps_mean>/<sig2_mean>/<astar_min>-<astar_max>/<delta_min>-<delta_max> nacc=<n>
  ```

  - It is accumulated in gauge atomics at the shed-refresh site (`emit_source.rs` ≈ 583–598), under
    `pol.shed_on && pol.slaw_on`, and reset per window.
  - The window's mean rr is `dbt_acc / nacc`, because the refresh site runs on the taper-cache cadence.
  - `fec=-` when `nref` = 0, which is expected on Bulk rows.
  - It is print-only, and is re-checked by the bit-identity tests.
  - Without it, K4 needs a second session, while m̄ and W1 still come from s60.

---

## 6. Rows and length

**Row times:**
- c2/c8: 75–90 s per 600 MB row with the gauge (brief).
- c1s 5 GB: 75–100 s typical, 210 s worst (§57a's guard convention: the 300 s timeout on the collapse share).
- c1s 1 GB calibration rows: ≈ 45 s typical, budgeted at the same 210 s worst.
- Evicting rows: ≤ 150 s + 40 s.
- Measured slot spacing without the gauge: 55–61 s (`s60size.out`).

| launch | block | rows | typical | worst |
|---|---|---|---|---|
| **60a** | build + engine suites (new tree, no reuse) | — | 60 min | 70 min |
| | smoke (c2 W 192, C + G) | 2 | 3 min | 3 min |
| | c2: 11 rows × 6 replicates | 66 | 83 min | 99 min |
| | **total** | **68** | **≈ 2.4 h** | **≈ 2.9 h** |
| **60b** | c8: 12 rows × 6 replicates (three n₀ rows) | 72 | 90 min | 108 min |
| | evicting rows (plan 59 on the gauge) | 5 (+ ≤ 5) | 12 min | 32 min |
| | **total** | **77 (+ ≤ 5)** | **≈ 1.7 h** | **≈ 2.3 h** |
| **60c** | smoke (c1s W 128 G, c1s W 2 048 C) | 2 | 4 min | 10 min |
| | c1s: 4 rows × 12 replicates (24 cal + 12 flatness at 1 GB; 12 control) | 48 | 45 min | 168 min |
| | G/C: 10 C rows paired with 10 control G rows | 10 | 15 min | 35 min |
| | **total** | **60** | **≈ 1.1 h** | **≈ 3.6 h** |

- **Total:** 205 rows (+ ≤ 5 evicting re-runs). The C rows at c2 and c8 are dropped: the smoke
  witnesses tag absence, and their G controls re-base against §56.
- **`LD_HARD`:** launch + 3.5 h (60a), + 3 h (60b) and + 4 h (60c).
- **The budget guard** refuses with less than the worst case left: 2.9 / 2.3 / 3.6 h.
- **Three launches, with the reason.** Every two-launch split exceeds 4 h worst case in one launch:
  - 60a + 60b ≈ 5.2 h;
  - 60b + 60c ≈ 5.9 h.
  - Three launches also keep each cell's gate and calibration read (§4.5, §1.5) ahead of the next cell.
- **Order and reuse.** Each launch runs only after the previous launch's smoke, witnesses, gates and (60a/60b)
  calibration validity are read. 60b and 60c reuse 60a's binary.

---

## 7. Open or not settled

1. **The boot hold and n₀ are now law terms with no derivation** (P1's meaning). This is recorded as `store-law.md`
   R6.7, with the R5.5 erratum. It is read against P1 and the latch rate.
2. **ℓ̂'s source joins R6.7 if P3 passes**, alongside b̂₀⁺: the lifetime minimum also latches start-up queue. (The
   calibration-ℓ̂ recipe itself was accepted in entry 5.)
3. **B2 at c2 is ill-conditioned a priori** (§1.6): R̃ is nearly flat below the knee, and the scored recipe adds floor scatter, so the per-replicate count cannot fire unless the in-session σ_R is far below 0.04. **The coordinator rules** between the count rule and the pooled-median form. c8's σ_R is in-session only.
4. **The K4 print (§5) is a gauge-code change** that is specified but not made. It must land on `slaw-gauge` before
   the build.
5. **The renderer-generated witness sample** (anchor-bound leg plus empty leg) is a pre-launch item (§1.4).
6. **c1s and c8 at n₀ = 128 have no measured flatness pool.** If a flatness gate fails, that cell has no R̃ (c1s: no control-sign verdict; c8: no B2).
7. **The harness (`harness60`) does not exist.** It needs:
   - the arm tables for 60a / 60b / 60c, with per-arm bytes;
   - the two- or three-n₀-row calibration rule, the LATCHED and thin-leg-1 tests;
   - the scrub and per-arm `RWM_SLAW`;
   - the smokes;
   - the per-row witness, the sign gate and the validity gate in the scorer;
   - the CAPLOG replay;
   - plan 59's rows behind `RWM_SLAW=1`.
   It needs stub tests and a rehearsal first, as in §56 and §57.
