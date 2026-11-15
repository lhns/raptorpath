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
   - **The recipe substitutes b0 only.** Each row's own `l`, `m` and `sig` are kept.
     - The ℓ̂ rise is smaller than the row spread at a fixed pool (`s60boot.out` per-row table).
     - This undoes entry 2's ℓ̂ substitution. The c2 band's low edge moves from 165 to 186, so **the c2 band no longer
       reaches the knee** (correcting the previous report).
   - **`Rb` is not scored.**
   - **The witness is a per-row gate.** It is shown on a two-leg, anchor-bound, empty-leg sample that is
     hand-formatted (no cargo), with a negative control.
   - **c1s is cut to the one thing it can decide**, the sign of R̃ − 1 at the control (PINNED / CROSSES / SPLIT).
     The calibration-end sign is a consequence of B1 not firing, not a gate.
   - **G/C:** the gauge's cost is predicted from its benches, and 60c shrinks to a gross-error check.
   - **Out-of-band meanings** are stated at c2 and c8.
   - The rows and launches are recounted.

Helper scripts (stdlib Python, run with `-I`; each output is committed beside its script in `store-law/`):

| script | input | output | what it computes |
|---|---|---|---|
| `s60boot.py` | `s56/rep56-diag-ld.tgz`, extracted | `s60boot.out` | per arm and per row: boot queue (first 2 s), late queue, m̂'s lift, `l`, σ at boot and at load (proxies: `[DIAG]` has no window minimum) |
| `s60p27.py` | `loop-diag/p27/diag-ld`, `loop-diag/p34/run/diag-ld` | `s60p27.out`, `s60p34.out` | per static cap and row, from true window minima: the gauge's `b0p` per leg, late T̂min − ℓ̂, the spread, `l`; printed R rebuilt (Rlo, Rhi) beside Rcom |
| `s60band.py` | numbers from the `.out` files (cited per line) | `s60band.out` | the crossing band per cell under the frozen recipe |
| `s60size.py` | `docs/l1-raw` (s56, s57a, s57b ledgers) | `s60size.out` | row wall times; G/C pair sds and n (t quantiles) |
| `s60replay.py` | a gauge client log | `s60replay.out` | `slaw_r` transcribed: R̃, the per-row witness, its negative control |
| `s60witness_ka.log` | — | — | the hand-formatted two-leg witness sample (§1.4) |

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

**ℓ̂ does not need substituting** (ruling item 3). Per row at a fixed pool (`s60boot.out`, per-row table):
- c2 `l` spans 6.30–9.09 at P128 and 9.87–11.74 at P2048;
- in p27, 8.64–8.90 at cap 128 and 8.18–14.16 at 1 024.

The arm-median rise (c2 8.07 → 10.13 ms, c8 leg 0 8.2 → 11.1) is no larger than the within-pool row spread (2–6 ms). c1s
is flat (2.4–2.7; §57b read c1d at 2.52–2.62 across six pools). Per-arm `l` medians are reported. If c2's control-arm
median `l` exceeds its calibration median by more than the calibration rows' range, the c2 band is reported with the
calibration-ℓ̂ corner as well: low edge 165 (entry 2's reading).

### 1.3 The anchor bind adds one b̂₀⁺ to the target

F = min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺. With m̂ ≥ ℓ̂ + b̂₀⁺ (`anc = 1`), the target gains a full b̂₀⁺, not the over-read.

On 600 MB rows, Copa's 10 s minimum tracks the queue (`s60boot.out` `lm`):
- c2: μ 2.7 ms at W 192 and 6.5 at 256;
- c8 leg 0: 1.9 at 320 and 10.5 at 512;
- c1s: 0.27–0.96 at 512–2 048.

So the bind is marginal at c2 W 192 and at c1s. The bands span both states; `anc` and `ancf` decide in-session.

Shift anc 0 → 1 (`s60band.out`, method B):
- c2: 192–220 → 208–258 (+16 to +38);
- c8: 440–550 → 473–597 (+33 to +47).

### 1.4 R̃: what it is, what it tests, and its witness

**What R̃ tests.**
- R̃ is the law fed a b̂₀⁺ that the flow did not measure itself: the b0p of a different row, the replicate's
  calibration row. R5.4 was withdrawn as an oracle for exactly that kind of input.
- So a **B2 fire refutes the target FORM**, F + σ + q·ℓ̂ with the coupling term, given a queue-free b̂₀⁺.
- **A pass says nothing about the law's own b̂₀⁺ input.** Whether the flow can obtain that value itself is P1's
  question, and the boot hold's (§1.2).

**The recipe, frozen.**
- **b0 only is substituted**, per leg: the b0p of **the same replicate's** calibration row (§1.5).
- Every other input is the row's own printed field: `nk` (w), `tp50` (T̂), `tmin` (T̂min), `m` (m̂), `l` (ℓ̂),
  `sig` (σ at load) and `q`.
- The coupling term is recomputed from the row's own `l` and the substituted b0.
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

The second pool is the **flatness check** only, and stands for no other n₀:
- c2 W 96;
- c8 W 160 (P80);
- c1s W 192.

**Calibration validity gate (per cell, read before any R̃).**
- The median non-latched b0p at the second pool must lie within **q·ℓ̂** of the median at W 128, per leg
  (q·ℓ̂ ≈ 1.0 ms at c2 and c8 leg 0, 3.2 at c8 leg 1, 0.25 at c1s).
- Reason: a difference inside the law's own allowance cannot move its target by more than δ's seat.
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
- **Two W 128 rows per replicate.**
  - The replicate's calibration is the first non-LATCHED one, in run order.
  - If both latch, the replicate has no R̃. It is counted as LATCHED, not VOID.
  - At a 25 % latch rate, P(a replicate has a calibration) ≈ 0.94. With a 0.9 bracketing validity,
    P(≥ 5 of 6 replicates scoreable) ≈ 0.75 at c8; it is higher at c2.
- **c1s validity is boot-only.** A c1s calibration row is valid for b0 if its `[WSTAT]` windows with t < 2 s show no
  qcwnd below the cell's BDP. The parser's absolute collapse rule (`slaw_parse.py --bdp-bytes`) is restricted to
  those windows, because `qcol` only counts from t ≥ 1 s and b0p uses the first 2 s.
  - A row that collapses later still calibrates, but cannot score B1.
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

Its inputs: σ at load; b0 over the **row** range at cap 128 (one row feeds each replicate); the over-read; the row's
own ℓ̂; c8's coupling ½Δ from own ℓ̂ and calibration b0 (12.8–15.7 ms); and c8's leg-1 pull (w₁/w₀)·D₁ (1.4–3.2 ms).
The b0 corners rest on 2 rows per cap.

| cell | KA point | **band** | in-session knee (§56) | B2 needs the engine to land |
|---|---|---|---|---|
| c2 | 194.2 | **186–258** | W₉₅ 169 | ≥ 17 slots below the band's low edge |
| c8 | 478.2 | **440–597** | W₉₅ 340 | ≥ 100 slots below the low edge (≈ 138 below the KA point) |

c1s carries no crossing band (§2.4). Its corners (434–981) are kept in `s60band.out` as context.

**Scoring.** A cell's crossing prediction is IN iff the median over scoreable replicates of R̃'s W_r lies in the band.

**What a crossing outside the band means.**
- **c2 below 186.**
  - Every propagated corner is exhausted: the low b0 row, low σ, anc 0, both T̂min readings.
  - So the window minimum rises faster with W than both the central curve and p27, or the row's ℓ̂ is depressed.
    The per-arm `tmin − l` and `l` readouts say which.
  - If it is also below the in-session W₉₅, B2 fires: the target form lands on the cliff.
- **c2 above 258.**
  - The target exceeds every corner: anc 1 with a large b0, or σ at load or ℓ̂ rising with W beyond the measured
    ranges.
  - The law sits higher on the flat region. That costs no goodput, but adds standing queue: q at W_r is reported
    against q₉₈.
  - It is an over-allowance finding against R5.5's frozen landing (the ½-slot margin), not a refutation.
- **c8 below 440.**
  - Leg 0's coupling ½Δ or leg 1's pull is smaller than measured, which bears on the ½ form's magnitude under
    symmetric shaping (B5's subject in B-2).
  - B2 fires only if it is also below W₉₅ (≈ 340).
- **c8 above 597.**
  - Leg 1's pull is larger than 3.2 ms, i.e. leg 1's σ at load grows faster than §56 read. That is the question B6
    asked, answered from the leg-1 pull readout (§2.3).
  - The law parks c8 at a larger store, a delay cost reported as q and in-order delay at W_r.

---

## 2. Arms and falsifiers

All arms are static pools through `RWM_STORE_PATH_POOL` (per path; c8's P = W/2), on Bulk, as in §56.

### 2.1 Arm lists (per replicate; arm order rotated per replicate, seeds alternating 42 / 7)

| cell | rows per replicate (total W) | replicates |
|---|---|---|
| c2 (600 MB) | **128 ×2** (cal n₀), 96 (flatness), 160, 176, 192, 224, 256, 384, **2 048** (ctl) | 6 |
| c8 (600 MB) | **128 ×2** (cal n₀, P64), 160 (flatness, P80), 256, 320, 384, 448, 512, 640, **1 280** (ctl, P640) | 6 |
| c1s | **128 ×2** (cal n₀, 1 GB), 192 (flatness, 1 GB), **2 048** (ctl, 5 GB) | 12 |

- **c2:** the knee arms are 160–192, with 16-slot steps where 32 slots cost 6 %. The band arms are 192–256.
- **c8:** the knee arms are 256–384, and the band is covered by 384–640. B2 can fire at c8 only on a gross miss
  (§1.6), and the arms read either outcome without extrapolation.

### 2.2 Crossing per replicate

- **W_r:** the linear interpolation between the last arm with R̃_row > 1 and the next arm with R̃_row ≤ 1. It uses
  unflagged, witness-passing rows of a replicate that has a calibration.
- **A flagged row at the sign change:** bracket with the nearest valid arms, and report the width.
- **More than one sign change:** the replicate is NON-MONOTONE. It is reported, and W_r is its first downward
  crossing. R6.2's uniqueness prediction is one sign change per replicate. Per-row `xs`/`xb` count window flicker on
  a static pool and are reported only.
- **Scoreable cell:** ≥ 5 of 6 replicates.
- **Knee in-session:** W₉₅ and W₉₈ from the arm-median goodput against the control (§56 definitions).

### 2.3 Falsifiers

| # | what fires it (printed fields only) | what it means |
|---|---|---|
| B1 | on a calibration (W 128) row: median LATE `err` > 0 | the collapse condition: b̂₀⁺ + q·ℓ̂ below the row's own later no-queue brake |
| B2 | at c2 or c8: W_r < in-session W₉₅ on ≥ 2 of the 5 or 6 scoreable replicates (R4.8's "more than 1 in 6") | the target FORM lands below the knee, given a queue-free b̂₀⁺ (§1.4) |

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
  - **Benched:** `bench_close_23k` reads 14 ns per observed sample and 0.22 ms per window close at 23 000 samples per
    250 ms window. That is (23 000 × 14 ns + 0.22 ms)/250 ms = **0.22 % of one core** on the c1s sender.
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
| **60b** | c8: 11 rows × 6 replicates | 66 | 83 min | 99 min |
| | evicting rows (plan 59 on the gauge) | 5 (+ ≤ 5) | 12 min | 32 min |
| | **total** | **71 (+ ≤ 5)** | **≈ 1.6 h** | **≈ 2.2 h** |
| **60c** | smoke (c1s W 128 G, c1s W 2 048 C) | 2 | 4 min | 10 min |
| | c1s: 4 rows × 12 replicates (24 cal + 12 flatness at 1 GB; 12 control) | 48 | 45 min | 168 min |
| | G/C: 10 C rows paired with 10 control G rows | 10 | 15 min | 35 min |
| | **total** | **60** | **≈ 1.1 h** | **≈ 3.6 h** |

- **Total:** 199 rows (+ ≤ 5 evicting re-runs), down from 219. The C rows at c2 and c8 are dropped: the smoke
  witnesses tag absence, and their G controls re-base against §56.
- **`LD_HARD`:** launch + 3.5 h (60a), + 2.5 h (60b) and + 4 h (60c).
- **The budget guard** refuses with less than the worst case left: 2.9 / 2.2 / 3.6 h.
- **Three launches, with the reason.** One worst-case launch at 4 h would leave no margin for 60a's build. Splitting
  c2 and c8 keeps each launch's gate read (§4.5) ahead of the next launch.
- **Order and reuse.** Each launch runs only after the previous launch's smoke, witnesses, gates and (60a/60b)
  calibration validity are read. 60b and 60c reuse 60a's binary.

---

## 7. Open or not settled

1. **The boot hold and n₀ are now law terms with no derivation** (P1's meaning). This is recorded as `store-law.md`
   R6.7, with the R5.5 erratum. It is read against P1 and the latch rate.
2. **The K4 print (§5) is a gauge-code change** that is specified but not made. It must land on `slaw-gauge` before
   the build.
3. **The renderer-generated witness sample** (anchor-bound leg plus empty leg) is a pre-launch item (§1.4).
4. **c1s at n₀ = 128 is unmeasured.** If its flatness gate fails, c1s has no R̃ and no control-sign verdict.
5. **The harness (`harness60`) does not exist.** It needs:
   - the arm tables for 60a / 60b / 60c, with per-arm bytes;
   - the two-n₀-row calibration rule and the LATCHED test;
   - the scrub and per-arm `RWM_SLAW`;
   - the smokes;
   - the per-row witness, the sign gate and the validity gate in the scorer;
   - the CAPLOG replay;
   - plan 59's rows behind `RWM_SLAW=1`.
   It needs stub tests and a rehearsal first, as in §56 and §57.
