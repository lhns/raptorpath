# store-law: the sender store size as a time law (F01 + E03 + E04 + C02)

Status: DERIVATION, revision 3. No engine change and no VM contact. Revisions 1–2: `store-law-derive` on
`fd81f408`. Revision 3: `store-law-rev3` on main `e195c8c0`, on status §56's maps.

**Read §R6, then §R5–§R3.** R6 withdraws R5.1, R5.2, R5.4 and R5.8. It supersedes §0's formula, the static points of §4, §5.1 and §5.2, and §12's open decisions.
The rest stands as the derivation's record.

Scope: law ledger §4 items 1–2, rows F01, E03, E04 and C02. The owner paused the lever-B guard on 2026-11-14 to
derive this law first.

Builds on:
- W1 (`ed4e3405:raptorpath/docs/derivations/w1-who-limits.md`): candidate A, the τ windows and U1–U3;
- W3 (`9e03d84a:…/w3-per-leg-state.md`): the per-leg state and the (1 + r) unit mismatch;
- `p0-dwell-diag.md` §2 and §8;
- status §23, §24, §34, §42, §43/§43b, §44 and its erratum, §54 (both Results) and §55.

Where this text departs from one of them, it says so at that point.

Helper scripts (stdlib Python, run with `-I`; each output is committed beside its script):

| script | input | output | what it computes |
|---|---|---|---|
| `store-law/little54b.py` | `s54/rep54b-diag-ld.tgz`, extracted | `little54b.out` | per-leg Little's law, holes and `[HOLD]`/`[SUCC]` on the §54b block |
| `store-law/map27.py` | `loop-diag/p27/diag-ld`, `loop-diag/p34/run/diag-ld` | `map27-p27.out`, `map27-p34.out` | the law and its rejected forms on the static-cap maps |
| `store-law/b0est.py` | the same two ledgers | `b0est.out` | the test of a b₀ estimator |
| `store-law/b0bound.py` | the same two ledgers | `b0bound.out` | one-sided b₀ bounds against each row's no-queue brake |
| `store-law/wire48b.py` | `bbr48b/c2c8-diag-qcc.tgz`, extracted | `wire48b.out` | the wire-clock anchor candidates (`bbr_rs` m̂, quinn `rtt.min()`) |
| `store-law/s56.py` | `s56/rep56-diag-ld.tgz`, extracted | `s56.out` | rev 3: the per-tick Little residual with means, the W1.1 who-limits classes, the knee-scaling candidates |

## R6. Revision 6: the curve hypothesis fails, the oracle is removed, one δ seat, and a registration that can lose

What this revision withdraws:
- R5.2's utilisation–delay curve "U" and R5.4's power formula;
- R5.1's c1d registration;
- R5.8.

What stands: the time law of R3/R4 with σ at load (R5.5); R5.3's burst arm, in its R6.5 redesign; R5.6's B-1/B-2 split; and
R5.7, the §56 addendum now in status.

### R6.1 One capacity, control arm included: the product rises

`s56curve.py` → `s56curve.out` uses one rate definition for every row: R = Δtx / Δt over the LATE window (the §56
score's "sum tx rate"). q is the mean of the 0.1 ms top-level rtt minus T₀ = 4.0 ms, over unflagged rows.

| c1s W | R /s | q ms | q·(C − R), C = control R 92 992 | C = 96 836 (rev 5) | C = 1 Gbit/s wire 101 379 |
|---|---|---|---|---|---|
| 384 | 73 156 | 0.61 | 12.0 | 14.4 | 17.1 |
| 512 | 82 708 | 1.16 | 11.9 | 16.4 | 21.7 |
| 768 | 86 525 | 3.18 | 20.6 | 32.8 | 47.2 |
| 1 024 | 89 457 | 5.29 | 18.7 | 39.1 | 63.1 |
| 2 048 (control) | 92 992 | 14.19 | **0 (u = 1 by construction)** | 54.5 | 119.0 |

Fable's arithmetic is verified: 14 / 16 / 33 / 39 / 54 symbols at C = 96.8 k.

**Where rev 5's flatness came from.** Rev 5's C (96 836) was the control's median of *per-tick-mean* rates, a
different average from the score's lap rate (92 992). Its u came from the same per-tick means, which made it
effectively the C = R_ctl column. That column is flat only because the control is left out: the control sits at
u = 1 with a finite 14 ms queue, where a K/(1 − u) curve must diverge.

**Under any one fixed capacity above every row's rate, the product rises monotonically with W**, ×3.8 (rev 5's C) to
×7 (the wire rate).

**The curve hypothesis fails as stated.** c1s's goodput keeps rising with standing queue, all the way to the control.
That is the §23 and §16 reading (receiver batching), and it gives no capacity at which a curve diverges. R5.2's U,
R5.4's formula and the c1d registration built on U (R5.1) are withdrawn.

### R6.2 The formula line reads only engine quantities; R5.4 was an oracle

**R5.4 was an oracle.**
- n·T₀/T̄ is the store's *current* delivered rate times T₀, not C·T₀. Steering to a u* needs the capacity C, which
  is E04, the input every candidate failed.
- K had no engine source.
- R5.4 is labelled **ORACLE** and withdrawn. It no longer bears on the law.

**The law that stands** (R3.1's line, with R4/R5's amendments):

```text
   n_{k+1} = clamp( n_k · Σᵢ wᵢ( Fᵢ + σᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ T̂ᵢ , floor , N·knee )
   Fᵢ = min(m̂ᵢ, ℓ̂ᵢ + b̂₀ᵢ⁺) + b̂₀ᵢ⁺ + ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺))
```

Each input and its source:

| input | engine source | status |
|---|---|---|
| wᵢ, T̂ᵢ, T̂minᵢ, Ŝᵢ | the per-leg window sampler of `record_rtt_sample` | to be built (archived on the oracle branch) |
| m̂ᵢ | Copa `min_rtt` | present |
| ℓ̂ᵢ | a running minimum of the same series | one field |
| b̂₀ᵢ⁺ | the sampler's first K windows | with the sampler |
| σᵢ (at load) | `qsp_us`, P90 − P50 of the leg's last 256 echo samples | present (`path.rs:848–855`) |
| q(δ) | `codel_setpoint_q` | present |
| floor, N·knee | resource bounds | present |

**No rate and no capacity appear.** n is the store's own state.

**Uniqueness of the fixed point.** R − 1 = Σwᵢ·(targetfloorᵢ − T̂minᵢ)/Σwᵢ·T̂ᵢ, where targetfloor =
F + σ_load + q·ℓ. The fixed point is unique iff, on the leg set carrying the weight, the window minimum grows with n
faster than σ_load does: d(T̂min − σ_load)/dn > 0.

On §56:
- c2: σ is flat at 1.7–1.95 while T̂min rises.
- c8 leg 0: σ 2.0–2.7 against T̂min rising 16 → 92 ms.
- c1s: σ *falls*, 0.64 → 0.44, as W rises.
- So R crosses 1 once at every cell: one crossing on each §56 map.

The exception is c8 leg 1, where σ rises 4.9 → 31.6 ms. It carries ≤ 16 % of the weight, and leg 0 dominates the
sum. **Gauge:** the number of R = 1 crossings per row in Part B (B-1), predicted to be 1.

### R6.3 One δ seat: q(δ)·ℓ̂ only

With R5.4 withdrawn, α = b(δ) does not enter the law, so **q(δ)·ℓ̂ is δ's only seat**.

**Reconciling q's provenance with §29.**
- q(δ) is RFC 8289's cited 5–10 % setpoint band. It comes from Kleinrock's power argument, used *once, offline*, to
  fix a ratio (`store_cap.rs`, `codel_setpoint_q`).
- §29's candidate D, the roadmap node `store-power-law`, *sought* the power argmax online from measured delivery and
  loop time. It failed on noise: §30's window needed ≫ a transfer.
- The two do not conflict. This law seeks nothing online: it holds a cited setpoint. It inherits RFC 8289's power
  derivation as provenance, and none of §29's estimator.

### R6.4 c1d: a registration that can lose informatively, and not on noise (replaces R5.1; amended after review)

**Data already seen** (disclosed, so excluded from the evaluation):
- p34's static table, contaminated (R4.1);
- the s53 pin readouts (`c1dpin.out`);
- §53's and §54's c1d per-row `qcwnd` and goodput (`c1dflag.out`, `c1dflag54.out`).

None is a W sweep.

**Registered: T, the receiver-queue transfer.** On a receiver-bound cell goodput depends on the standing queue at the
receiver's batching. So c1d's goodput relative to its own control is the same function of the standing queue as
c1s's.

**One q reference for both sweeps.** Each sweep's smallest arm must not set its own zero: c1s's smallest arm is 384
per leg, c1d's is 192 per leg. Both therefore use **q_ℓ = T̄ − ℓ̂**:
- T̄ is the rate-weighted mean echo loop over LATE ticks, from per-tick means.
- ℓ̂ is each leg's lifetime echo minimum (the lowest `rtp_us` in the row's first 2 s), rate-weighted.

On c1s (`s56curve.out`, `c1srows.out`; ℓ̂ = 2.4–2.6 ms, so q_ℓ ≈ q + 1.5):

| c1s q_ℓ ms | 2.1 | 2.7 | 4.7 | 6.8 | 15.7 (control) |
|---|---|---|---|---|---|
| goodput / control | 0.79 | 0.89 | 0.94 | 0.96 | 1 |
| unflagged rows behind the point | 2 | 5 | 3 | 2 | 6 |

**The band is sized from the spread.** The two sources of noise:
- The c1s reference points rest on 2–5 unflagged rows. Their goodput/control spread is ±0.01–0.02 at 512–1 024; the
  2-row points and the control (whose 6 rows include one at 727) are looser.
- The c1d side is a per-pool median of ≥ 3 unflagged rows over a control median. The §53 c1d 5 GB unflagged row sd is
  ≈ 3.5 %, which makes the ratio's sd ≈ 0.03.

Combined, the sd is ≈ 0.04, so the band is **±0.08** (≈ 2 sd). That matches §54's measured c1d goodput resolution of
8 %.

**What the band can resolve.** c1s's relation is steep only at q_ℓ ≤ 2.7 ms (0.79 → 0.89 over 0.6 ms, about 0.17 per
ms). Above that it is flat (≈ 0.01 per ms), where ±0.08 spans ±8 ms of q and cannot discriminate. So:
- **(a) falsifying:** c1d goodput/control at q_ℓ = 2.1 and 2.7 ms equals 0.79 and 0.89, **±0.08 each**. It is
  interpolated on c1d's arm medians in q_ℓ; at least one c1d arm must fall in q_ℓ ≤ 3.0 ms.
- **(b) reported only:** at q_ℓ = 4.7 and 6.8 ms, 0.94 and 0.96. c1d's q at the 0.95 point is reported, not
  falsified: near 0.95 a ±0.03 uncertainty in the goodput ratio is ±3 ms of q.
- **(c) the c1d control's own queue:** q_ℓ = **13–21 ms** at W 4 096. This is predicted from the seen s53 pin rows
  (echo loop 18.5–21.5 ms against ℓ̂ ≈ 2.5). The c1d control is interior (s53 a/W 0.47–0.63), so this tests the
  control's regime, not T. A control queue below 10 ms means the sweep's reference is a different operating point,
  and is reported before (a) is read.

**What each outcome means, stated now:**
- **(a) passes:** the steep part of the receiver-bound relation is a receiver property. It transfers across path
  counts, and is measured once per receiver class.
- **(a) fires low** (c1d at least 0.08 below at the same q_ℓ): two connections need more standing queue for the same
  utilisation. The relation depends on the connection count.
- **(a) fires high:** c1d needs less queue; the receiver batches across two connections more efficiently.
- **W is not a falsifier.** W = R·(T₀ + q) by Little, and R and T₀ are each cell's own. "W differs, q passes" means
  c1d's rate or floor differs, not its queue mechanism.
- **Disclosed:** p34's contaminated c1d, re-read in q_ℓ from its window medians, sits inside (a)'s band at 384 and
  512 (q_ℓ ≈ 2.2 and 3.0; 0.76 and 0.87 against c1s's 0.80 and 0.90). It is not counted.

**Amendment (after the second review).** Three rules are added; the band is unchanged.

**1. Floor offset.** T's x-axis carries each cell's no-queue offset, δ₀ = T₀ − ℓ̂: the no-queue echo loop above the
lifetime echo minimum. At c1s it is 4.0 − 2.5 ≈ 1.5 ms, while the falsifying region is only 0.6 ms wide.
- **Measurement:** c1d's offset is its smallest store-bound pool's q_ℓ, which is predicted to be at or near no queue.
  Both offsets are reported.
- **"Offset differs" reading:** if |δ₀(c1d) − δ₀(c1s)| > 0.5 ms, (a) is also read on the offset-corrected axis
  q_ℓ − δ₀. If (a) passes on one axis and fails on the other, the reading is **OFFSET DIFFERS**: the cells' no-queue
  floors differ, and the queue relation is not tested.
- **No extrapolation:** an (a) point outside c1d's measured q_ℓ range (its arm medians' min to max) is **UNSCOREABLE**.

**2. Flagged rows.** T is read twice: on all rows, and on unflagged rows only. The c1d flag flagged nothing on today's
engine at guard size (R6.5). If the two readings disagree (one passes (a), the other fails), T is **UNSCOREABLE**.

**3. Store-bound pools.** T is read only on c1d pools whose mean a/W is ≥ 0.80 (see R6.5's bind check). A pool below
0.80 is not store-bound, and reads a different regime.


### R6.5 Registration: two launches, §57a (the burst block) and §57b (the c1d pool sweep) (reported only, no build; copyable into status; amended after the second review)

**Why.**
- At c1s, more standing queue keeps buying goodput (R6.1). Two sender-side mechanisms could size that queue: the
  emission burst (F03's `RWM_EMIT_BURST`, pinned at 64 on c1s/c1d) and the ack-driven release batch.
- **A null burst result means only "not the emission burst"**, not "the receiver".
- §57b is the held-out c1d map for R6.4's T.
- No build: both knobs exist on main. Both launches run before the gauge build (B-1).
- **The block is split into two launches**, because the burst block alone has a worst case of ≈ 3.7 h (below).

**What §56 already shows about the release batch** (`c1srows.py` → `c1srows.out`, unflagged c1s rows):

| c1s W | 384 | 512 | 768 | 1 024 | 2 048 |
|---|---|---|---|---|---|
| release batch (symbols per ack wake) | 72–73 | 102–112 | 141–168 | 134–181 | 177–270 |
| release batch / W | 0.19 | 0.20–0.22 | 0.18–0.22 | 0.13–0.18 | 0.09–0.13 |
| ack wakes per echo loop | 4.7 | 3.7–4.3 | 3.6–4.4 | 4.4–6.5 | 6.3–10.8 |
| mean a/W | 0.85 | 0.85–0.86 | 0.85–0.87 | 0.86–0.88 | 0.87–0.92 |
| sender busy % / ack-wait % | 41–43 / 90–91 | 45 / 81–84 | 48 / 61–68 | 48–50 / 57–66 | 45–57 / 45–61 |

At W ≤ 768 the store is released in ≈ 4–5 batches per loop, each ≈ 0.2·W. This is registered as a readout in both
launches.

**Common to both launches.**
- **Tree:** current main, pinned by amendment.
- **Pool knob:** `RWM_STORE_PATH_POOL` as §56. The ceiling is max(N·pool, floor), boot min(128, store_max), and every
  W ≥ 384 > 128.
- **Burst knob:** `RWM_EMIT_BURST` (`gates.rs:194/363`, default 64, clamped [2, 512], echoed at `gates.rs:443`).
- **Scrub:** `-u RWM_STORE_PATH_POOL -u RWM_EMIT_BURST`.
- **Suites:** engine suites are skipped only on a byte-identical reuse-manifest hit against §56's tree. The tools
  tests and `LD_REHEARSE=1` always run.
- **Run settings:** Bulk, `--window-reliable`, `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300`, 5 GB.
- **Liveness:** `[GATES]` echoes the requested pool and burst on both ends. Otherwise the row is VOID and replaced
  once.
- **Bind:** a non-control row binds if its bind fraction is ≥ 0.95. Otherwise it is reported at its realised cap and
  not replaced.

---

#### §57a. The burst block (c1s)

**Arms** (c1s-400, n = 21 each, 63 rows):

| arm | pool (W) | `RWM_EMIT_BURST` | role |
|---|---|---|---|
| **B64-512** | 512 (512) | 64 (shipped) | control, re-based against §56's P512 |
| **B64-448** | 448 (448) | 64 | the in-session map: the R₆₄ chord from 448 to 512 |
| **B256-512** | 512 (512) | 256 | the test arm |

- The burst-16 arm is dropped.
- **Order:** Latin-square rotation over the three arms per replicate, seeds 42 / 7 alternating.
- **Smoke:** one B256-512 row. It must read a realised mean burst > 64 (Δ`eb_syms`/Δ`eb_bursts` over LATE);
  otherwise **ABORT-SMOKE**, because the knob does not act.

**Why n = 21.**
- Every arm needs ≥ 8 unflagged rows: the interpolation uses all three arms.
- §56 collapsed 4/9 rows at P512; pooled over c1s it was 9/27 = 1/3.
- With X ~ Binomial(n, 1 − f), P(X ≥ 8):

  | f | n 12 | 14 | 16 | 18 | 20 | **21** | 24 |
  |---|---|---|---|---|---|---|---|
  | 1/3 | 0.632 | 0.851 | 0.950 | 0.986 | 0.996 | **0.998** | 1.000 |
  | 4/9 | 0.318 | 0.563 | 0.759 | 0.882 | 0.948 | **0.966** | 0.992 |

- At n = 21, all three arms clear 8 with p = 0.966³ ≈ **0.90** at f = 4/9, and 0.998³ ≈ 0.99 at f = 1/3.
- At n = 16, the three arms would clear together with only 0.76³ ≈ 0.44 at f = 4/9.

**The hypothesis, read directly from the session.** Let b̄₆₄ and b̄₂₅₆ be the realised mean bursts at W 512. If the
emission burst holds ΔQ_b = (b̄₂₅₆ − b̄₆₄)/2 slots of store that buy no transit, then:

```text
   R_pred  =  R₆₄(512)  −  (ΔQ_b / 64) · ( R₆₄(512) − R₆₄(448) )
```

Burst 256 at W 512 then sits ΔQ_b/64 of the way from the 512 control down to the 448 arm. All three terms are
in-session, so neither §56's map nor any drift enters. The chord is linear between two in-session points; §56's
384–512 chord (75 /s per slot, its 384 end on 2 rows, and the map concave there) is no longer used.

**Scoring.** Every R is the per-arm median lap rate over unflagged rows. Each arm's SE = 1.253·sd/√n_u, from that
arm's own realised unflagged row count and sd.
- z_null = (R₂₅₆ − R₆₄(512)) / √(SE₂₅₆² + SE₅₁₂²).
- z_hyp = (R₂₅₆ − R_pred) / SE_pred, where SE_pred combines all three arms' SEs.
- Separation S = (ΔQ_b/64)·(R₆₄(512) − R₆₄(448)) / √(SE₂₅₆² + SE₅₁₂²): the predicted shift in SE units.

**Pre-registered expectation.**
- b̄₂₅₆ ≈ 90–105, because the release batch (≈ 105 per wake at W 512) caps the burst, against b̄₆₄ ≈ 48. So
  ΔQ_b ≈ 21–29 slots.
- The chord R₆₄(512) − R₆₄(448) is ≈ 2–4.8 k/s; 4.8 k is §56's upper bound, which concavity lowers.
- So the predicted shift is ≈ −0.7 to −2.2 k/s. At n_u ≈ 12 per arm (row sd ≈ 1.1 k) the separation is S ≈ 1.2–3.9.

**Outcome table** (every reading uses the realised b̄ and the in-session chord):

| reading | condition | what it means (stated now) |
|---|---|---|
| **UNDERPOWERED** | S < 2 | the burst's predicted shift is below resolution at this chord. E1/E2 are not read; the arms are reported |
| **E1: BURST HOLDS STORE** | S ≥ 2, z_null ≤ −2 and \|z_hyp\| < 2 | the emission burst holds wasted store at c1s, sized as predicted: F03's cost |
| **E2: NOT THE BURST** | S ≥ 2, \|z_null\| < 1 and z_hyp ≥ +2 | the predicted shift is absent; the receiver and the release batch remain |
| **RISING** | z_null ≥ +2 | a larger burst *raises* c1s's rate, through fewer wakes (CPU) or larger receiver batches. F03's cap of 64 then costs goodput rather than holding store; read with the CPU gate |
| **PARTIAL** | S ≥ 2, z_null ≤ −2 and z_hyp ≤ −2 or ≥ +2 | a burst effect exists but is not (b − 1)/2-sized; reported with its ratio to the prediction |
| **INCONCLUSIVE** | any other combination | reported |

How a realised burst outside the expected band is treated:
- **b̄₂₅₆ between 64 and 90:** scored with that b̄. The shift is smaller, and UNDERPOWERED is likelier.
- **b̄₂₅₆ above 105:** scored with that b̄. The release batch is then not the burst's bound, and that is reported.
- **b̄₂₅₆ ≤ 64:** the arm is INERT; the smoke already aborts on this.

**Gates on the reading (reported, never voiding).**
- **CPU on both test arms.** The client CPU/GB of B256-512 and of B64-448 must each be within 5 % of B64-512's.
  Otherwise that arm's reading is labelled **CPU-CONFOUNDED**. Burst 256 is not assumed clean: larger bursts can help
  CPU or receiver batching (RISING).
- **Collapse rate per arm.** It is the share of rows flagged by §56 Amendment 2's c1s rule, registered per arm. If
  B256-512's rate differs from B64-512's by more than 0.25 (absolute), the unflagged subsets are differently selected,
  and the reading is labelled **BIASED**.
- **Re-basing of B64-512 against §56's P512,** with the same lap-rate definition: §56's unflagged median is 82.7 k/s,
  range 80.3–83.3 k. It is re-based if the in-session median lies outside 82.7 k ± 3.0 k (the §56 range, floored at
  1 %). The scoring uses only in-session arms, so re-basing reports drift; it changes no reading.

**Readouts.**
- **Primary:** R per arm.
- **Secondary:**
  - Q = a − R·T₀, from the mean counted store and per-tick-mean rates, with T₀ fixed at §56's c1s 4.0 ms;
  - q from 0.1 ms per-tick means;
  - the realised mean burst;
  - the release batch and its share of W, ack wakes per loop, and mean a/W;
  - sender busy and ack-wait %;
  - goodput, client and server CPU/GB, and the collapse share per arm.
- **E5:** release batch / W at W 512 outside 0.17–0.24 in any arm. That means the release is not per-loop as §56 read
  it.

**Length.**

| | rows | estimate |
|---|---|---|
| unflagged | ≈ 35–42 | × 75 s ≈ 44–53 min |
| collapsed, typical | ≈ 21–28 | × ≈ 120 s (§56's collapsed rows ran 237–800 Mbit/s, ≈ 50–170 s) ≈ 42–56 min |
| plus tools tests, rehearsal and smoke | — | ≈ 30 min |

- Typical ≈ **2 h 10**.
- Worst with every collapsed row at §56's slowest (170 s): ≈ 2 h 45.
- Worst with every collapsed row at the 300 s timeout: ≈ 3 h 40.
- `LD_HARD` = launch + 4 h.

---

#### §57b. The c1d pool sweep

**Bind check, from §53's 30 c1d 5 GB rows at the shipped law** (`c1dcap.py` → `c1dcap.out`):
- np = 2 on every row.
- The late cap has a median of 3 568 (row medians 3 206–4 096); its p10 is 2 052–4 096, median 2 850.
- The mean a/W is 0.42–0.69 on 29 rows (0.97 and 0.86 on the two collapse rows), i.e. ≈ 1 700–2 300 symbols of
  occupancy.
- So **any pool W ≤ 2 048 binds the cap** (the law's ask exceeds 2 052 on ≥ 90 % of ticks on every row).
- But the store fills only where W drops below the occupancy the receiver-bound pair holds, ≈ 1 700–2 300 symbols.
- **W 2 048 would be cap-bound but likely store-open**, the same regime as the control. It is dropped.
- **W 1 536 is the first store-bound candidate.** Each pool's mean a/W is reported, and T reads only pools at
  a/W ≥ 0.80 (R6.4 amendment).

**Arms** (c1d-400, n = 5 each, 30 rows):

| arm | pool (W) |
|---|---|
| **control** (shipped) | 2 048 (4 096) |
| | 768 (1 536) |
| | 512 (1 024) |
| | 384 (768) |
| | 256 (512) |
| | 192 (384) |

- **Order:** Latin-square rotation, seeds 42 / 7 alternating.
- **Smoke:** pools 192 and 2 048.

**Flags (never voiding; flagged rows enter no median).**
- **c1d collapse: both legs below BDP in > 0.10 of windows** (`c1dflag.py`). Its validation:
  - **§53's 5 GB rows (pre-law engine):** it flags 5 of 30, goodput median 729 against unflagged 827 (−12 %).
  - **§54's 24 guard rows (today's engine, 400 MB):** it flags 0 (`c1dflag54.out`).
  - **So validation on today's engine is in-session:** the flagged-against-unflagged goodput gap is a readout, and T
    is read on all rows and on unflagged rows (R6.4 amendment).
- **Minimum:** ≥ 3 unflagged rows per pool for any reading. P(< 3 of 5) ≈ 0.04 per pool at §53's 5/30 rate. A pool
  below the minimum is `UNREAD`.
- CPU/GB is reported per pool.

**Readouts** (per pool): goodput/control; q_ℓ and c1d's floor offset δ₀ (R6.4); R; a/W; the release batch and ack
wakes per loop; both-leg and one-leg flag shares; CPU/GB.

**Predictions:** R6.4 (a), (b) and (c), with the amendment's offset, range and flag rules.

**Length.**

| | rows | estimate |
|---|---|---|
| all rows | 30 | × 75 s ≈ 38 min |
| collapses, typical | ≈ 5 | + ≈ 4 min |
| collapses, worst (5 rows at 300 s) | ≈ 5 | + ≈ 19 min |
| plus tools tests, rehearsal and smoke | — | ≈ 30 min |

- Typical ≈ **1 h 15**; worst ≈ 1 h 30.
- `LD_HARD` = launch + 2.5 h.

---

**Decision rule (both launches).** None; reported only.
- §57a decides only whether F03's burst holds, or costs, any of c1s's queue.
- §57b decides whether the steep part of the receiver-bound relation transfers across path counts.
- Neither changes the time law. B-1 follows.

### R6.6 Removed: R5.8

R5.8 is removed and not forwarded. On a K/(1 − u) curve, "Bulk = full rate" has no finite point: it is the clamp,
today's defect. If E1 fires, the c1s cost belongs to F03 (the emission-burst law), not to δ.

The derivation carries no owner question at this revision. That does not mean settled.

**Open design items** (frozen until data arrives):
- **δ's authority is ≈ 1 ms:** q·ℓ ≤ 1–1.4 ms at c2/c8 and ≤ 0.26 ms at c1s. Realtime to Bulk moves the law's point by
  a few symbols (R3.2, R4.7).
- **The receiver-bound relation's mechanism** (R6.5: E1/E2, and the release batch).
- **b̂₀⁺'s tail and the K, n₀ provenance** (§13.1).
- **σ at load on a bursty leg** (B6).
- **The asymmetric-delay case of the ½** (B5).

The derivation is frozen; the next input must be data.

## R5. Revision 5: the receiver-bound curve, one c1d variant, Part B split

### R5.1 c1d: ONE registered variant (committed before any further c1d data) [WITHDRAWN: U failed at c1s (R6.1); replaced by R6.4]

**Registered: U, the utilisation–delay curve of R5.2.** M and Mb (R4.1) are retired:
- **M is retired** because both its inputs fail at c1d.
  - Its σ input (c1s load σ, 0.5 ms) reads 1.1–1.3 ms at the c1d pin.
  - Its backlog term is withdrawn as a cause (R4.3).
- **Mb is retired** because its input, the boot σ̂⁺, is rejected (R4.4). Its fit to the contaminated p34 map is
  therefore not evidence.

That leaves one model for a receiver-bound cell, and it is the one under test.

**c1d data already seen**, and so not part of the held-out evaluation:
- p34's static table (contaminated, R4.1);
- the s53 pin readouts (`c1dpin.out`: σ, quinn backlog dwell, a/W).

Neither is a W sweep.

**U at c1d.** The model is q·(1 − u) = K/C on the receiver-bound curve, so q = K/(C·(1 − u)) and, by Little,
W ≈ C·u·(T₀ + q).

Its inputs:
- K = the constant measured at c1s (R5.2): 12.5–21 symbols at W 384–1 024. It is carried unchanged, because the
  receiver host, the receiver code and the emission-burst cap (64) are the same.
- T₀ = 4.0 ms, c1s's floor; c1d's boot ℓ is 2.5 ms on both legs, as at c1s.
- C = the c1d control arm's own LATE send rate, read from the sweep itself. The bands below use C = 92 k/s.
  - Rescale linearly if the measured C differs.
  - The prediction is the *shape*: q at given u. W is secondary.

| point | u | q = K/(C·(1 − u)) | W ≈ C·u·(T₀ + q) |
|---|---|---|---|
| 0.95 of control | 0.95 | 2.7–4.6 ms | **590–750** |
| 0.98 of control | 0.98 | 6.8–11.4 ms | **975–1 390** |

**Falsifiers** (on the R5.4 c1d sweep):
- q·(1 − u)·C at W 384–1 024 outside 8–32 symbols (c1s's 12.5–21, widened ×1.5);
- or not flat within ×2 across those W;
- or W₉₅ outside 550–800.

Any one of them refutes U at c1d.

### R5.2 Is the receiver-bound "knee" a utilisation–delay curve? (post hoc test on §56) [WITHDRAWN by R6.1: the product rises under one capacity]

`s56util.py` → `s56util.out`.

**Definitions.**
- Capacity C is the control arm's median LATE send rate over unflagged rows: c1s 96 836 /s, c2 10 224 /s, c8
  12 141 /s.
- u = R/C.
- q = T̄ − T₀ (rate-weighted echo loop minus the §56 floor).
- At N = 1 the 0.1 ms top-level rtt replaces the integer per-leg value.
- The table gives the row-level product q·(1 − u). The per-tick mean is printed beside it in the output.

| c1s W | 384 | 512 | 768 | 1 024 | 2 048 (ctl) |
|---|---|---|---|---|---|
| u | 0.79 | 0.89 | 0.93 | 0.96 | 1 (by definition) |
| q ms | 0.61 | 1.17 | 3.19 | 5.29 | 14.2 |
| **q·(1 − u)·C, symbols** | **12.5** | **12.7** | **21.3** | **19.8** | — |
| mean emission burst (`eb_syms/eb_bursts`; cap 64) | 42.6 | 47.9 | 52.4 | 52.6 | 57.2 |
| release batch (symbols per ack wake) | 72 | 105 | 151 | 158 | 246 |

**c1s: the product is flat within ×1.7 over W 384–1 024.**
- That is 4 points, two of them at n = 2. A knee would have the product rise then fall to 0, as c2 and c8 do here
  (c2 −1…0.3 symbols; c8 0.8 → 6.8 → 0.6).
- So c1s behaves as a utilisation–delay curve q ≈ K/(C·(1 − u)), with K ≈ 12.5–21 symbols. c2 and c8 are knees.
- The per-tick product is not flat. A 250 ms rate is too noisy for 1 − u at the per-tick scale.

**Batch match.** Kingman's batch term gives q·(1 − u)·C ≈ (b − 1)/2, i.e. b_eq = 26–44.
- The **emission burst** (mean 43–53, cap 64) is the same order: (b − 1)/2 = 21–26 against 12.5–21 measured.
- The **release batch** grows with W (72 → 246), so it is not the constant.
- **But the emission-burst term fails at c2.** There it predicts (11–15 − 1)/2 ≈ 5–7 symbols, against ≈ 0.1–0.4
  measured.
- So the variability is not the sender's burst alone. The other candidate is the **receiver's** batched service: c1s
  is receiver-bound, c2 is not.
- The mechanism stays undetermined between those two. The `RWM_EMIT_BURST` arm (R5.3) decides between them.

**Against p34 c1d (contaminated, R4.1).**
- The product reads 10.4 / 21.5 / 26 symbols at W 512 / 768 / 1 024. That is inside U's 8–32 band, and flat within
  ×2.5.
- W₉₅ ≈ 1 080 lies outside U's 550–800.
- On contaminated data, U's shape holds and its W does not. The clean c1d sweep (R5.6, B-1) decides.

### R5.3 The build-free falsifier: an `RWM_EMIT_BURST` arm at c1s

`RWM_EMIT_BURST` is live on main: `gates.rs:194/363`, default 64, clamped to [2, 512]. F03 pins it on c1s/c1d.

**Arms.** c1s-400, 5 GB, pools giving W = 768 and 1 024, `RWM_EMIT_BURST` ∈ {16, 64 (control), 256}, n = 6 each,
36 rows.
- The quinn-collapse flag (§56 Amendment 2) applies, with flagged rows excluded.
- Client CPU/GB and the mean emission burst are readouts.

**Predictions, at W = 768** (u = 0.93 at the control):

| hypothesis | what it says | burst cap 16 | burst cap 256 |
|---|---|---|---|
| U-sender | K ∝ the mean emission burst | K ÷ ≈ 3.5, u ≈ 0.985: +5–6 % goodput | K × ≈ 2.3, u ≈ 0.89: −4 % |
| U-receiver | K is set by the receiver | within ±1.5 % | within ±1.5 % |

**Reading.**
- |Δgoodput| ≥ 3 % in the U-sender direction, with q·(1 − u)·C moving in proportion to the mean burst: U-sender.
- ≤ 1.5 %: U-receiver.
- **Confound, stated:** a smaller burst means more wakes on a CPU-bound sender. A loss at cap 16 together with a rise
  in client CPU/GB is reported as CPU, not as either hypothesis.

### R5.4 How δ would pick the utilisation: one formula, continuous [WITHDRAWN by R6.2: an oracle, it needs capacity C (E04) and K]

On the curve T(u) = T₀ + K/(C·(1 − u)), maximise the generalised power Π_α = u^α/T. α(δ) = b(δ) is paper §6.1's
power exponent: ½ Realtime, 1 Auto, 2 Bulk. With x = 1 − u and k = K/(C·T₀):

```text
   α · x · (x + k)  =  k · (1 − x)
   x*  =  [ −(α + 1)·k  +  √( (α + 1)²·k²  +  4·α·k ) ] / (2α) ,   u* = 1 − x* ,   q* = T₀ · k / x*
```

- **k is a symbol ratio:** the curve's batch constant ÷ the no-queue BDP in symbols. By Little, C·T₀ = n·T₀/T̄, so k
  needs no rate.
- **Continuous and monotone in δ** through α = b(δ). There is no branch.
- **Hard knees fall out of the same formula.** As k → 0 (c2, c8: K ≈ 0), x* → 0, i.e. the knee itself. One formula
  covers knees and curves, and the knee terms σ and ½Δ stay additive in the target.

| c1s, k ≈ 0.04 (0.032–0.054) | Realtime (α ½) | Auto (1) | Bulk (2) |
|---|---|---|---|
| u* | 0.77 | 0.84 | 0.885 |
| q* | 0.7 ms | 1.0 ms | 1.4 ms |
| goodput against control | −23 % | −16 % | −11.5 % |

This is the power point the paper already accepted. **At Bulk it would still fail the guard at c1s** (5 % d80).
That is the owner question R5.8.

### R5.5 The landing, redone with σ at load (replaces R4.4's void boot-σ̂⁺ table)

σ at load is each row's median `qsp_us` over its LATE ticks, read at that row's static W. In closed loop it would be
read at the law's own point. c2's σ is invariant across W (1.7–1.95), and c8 leg 0's is stable (2.0–2.7). b̂₀⁺'s
over-read is the ledger value (`s56cls.out`, `/sigma@load` lines).

| cell (0.95 knee) | rows | law's W per row, median over-read | below the knee | ledger-max over-read |
|---|---|---|---|---|
| c2 (169) | 18 | 186–202 | **0 / 18** | 191–202, 0 / 18 |
| c8 (340) | 24 | 417–551 | **0 / 24** | 1 029–1 141: the b̂₀⁺ tail, 0 / 24 |
| c1s (903) | 27 | 467–550 | **27 / 27** | 477–550, 27 / 27 |

At c2 the spread across rows is ≈ 16 slots, against a margin of 17–33 above the knee. The counts are tight, but the
margin is only about one 32-slot step from the 6 % loss below the knee.

### R5.6 Part B, split into two sessions

**B-1: the gauge build on §56's cells, plus the c1d sweep and the R5.3 burst arm.**
- Gauge build: print-only, no law change.
- Rows: §56's c8/c2/c1s arms.
- **The c1d sweep:** W = 4 096 (control), 2 048, 1 536, 1 024, 768, 512, 384, n = 4. It scores R5.1.
- The R5.3 burst arm. This is build-free and can also run without the gauge.

**What the engine prints: raw window statistics** (estimators are then chosen offline), as `[WSTAT]` per 250 ms
window and live leg:
- echo samples: count, min, p10, p50, p90, mean;
- the release batches: count, and size p50/p90/max (symbols released per SACK-release event);
- the emission bursts: count, and size p50/p90/max;
- ack-wake count;
- a/W mean and minimum over the window;
- quinn backlog p50/max;
- Copa `min_rtt`, the lifetime echo minimum, and `qsp_us`.

The law's counterfactual R (`[SLAW]`, R4.8) is printed beside it and is never applied.

**"Bit-identical cap sequence" needs a test:**
- **Host unit test:** drive the store-cap seat from a recorded input trace with the gauge on and off, and assert
  equal cap sequences.
- **VM witness:** the gauge logs (inputs, cap) per refresh. An offline replay through the pure `pooled_store_cap`
  must reproduce every logged cap.
- Runs are not deterministic across rows, so no cross-row comparison is claimed.

**Re-base before reading.** The sampler sits on a CPU-bound cell's ack path, so the c1s control is re-based against
§56's control (goodput 875 Mbit/s; client and server CPU/GB) first.
- If c1s goodput falls outside §56's control range, or client CPU/GB rises more than 5 %, the gauge perturbs c1s.
- Its c1s readings are then reported as perturbed and not used to choose an estimator.
- c2 and c8 are re-based the same way (B640 for c8).

**What the open-loop ratio can do.** It can **refute** the law (a crossing below the knee, B2), but never confirm it.
A ratio that crosses 1 at a good W on static rows says nothing about the closed loop (§43).

**B-2: the jitter cells and c8a/c8b**, after three prerequisites:
- `adv-cells` is merged and rehearsed;
- the harness has a separate server-side delay option;
- B-1 has been read.

B-2 carries R4.1's dose-response falsifier (B4) and R4.5's asymmetric-delay falsifier (B5).

### R5.7 §56 Addendum (POST HOC; a stand-alone block for status)

> **§56 Addendum (post hoc, 2026-11-14; from `docs/derivations/store-law.md` §R3–§R5 on the §56 ledger; nothing
> registered is re-scored).**
> - **F5 is resolved as a parser artefact.** Recomputed per tick with means (`store-law/s56.py`), |W − Σrᵢ·rttᵢ| is
>   ≤ 0.06·W on every arm, c1s included. The scored medians used integer-millisecond echo values. A small time
>   outside the echo loop remains: 0.3–0.9 ms at c2/c8 for W ≤ 512, growing with W at c1s. It changes the store size,
>   not the ratio law's fixed point.
> - **c1s is store-bound.** One class rule across cells (store full iff a ≥ 0.95·W; quinn backlogged iff B ≥ 98;
>   `store-law/s56cls.py`):
>   - c1s's mean occupancy is 0.85–0.90 of W, with a 10–15 % ack-batch sawtooth that a per-tick threshold misses;
>   - ticks with quinn's backlog carry the higher send rate, and the rate falls with W in every class;
>   - so quinn's backlog is an effect of W, not a limiter. Checked on three rows by the coordinator: occupancy
>     0.85–0.88, backlogged rate ≥ the other ticks.
>   - Post hoc, c1s's goodput curve is utilisation–delay-like: q·(1 − u)·C ≈ 12.5–21 symbols at W 384–1 024, unlike
>     c2's and c8's knees.
> - **The held-out c1d prediction missed** (`store-law.md` R4.1, committed `bcafa8c3` before the evaluation).
>   - At the s53 pin, c1d's σ reads 1.1–1.3 ms and its quinn backlog dwell 0.9 ms, so the decomposition's inputs do
>     not transfer.
>   - On p34 (contaminated: seen in rev 2), the knee queue is ×2.7 the decomposition's.
>   - One variant (U, R5.1, committed `441f5def`) is registered for a clean c1d sweep.
> - **c8's ½ comes from symmetric shaping.** `topo_dual.sh` shapes both directions (5 + 5 ms, 20 + 20 ms), so the
>   legs' forward arrivals equalise at a leg-0 queue of ½ the loop gap. That is ≈ 15 ms, a leg-0 loop of ≈ 30 ms, at
>   W ≈ 320–340, where the knee is. Asymmetric cells c8a/c8b are its falsifier (Part B-2).
> - **Withdrawn derivation claims:**
>   - rev 2's "c8 −3 to −5 %, fails the guard" (p27's 8 s rows; on §56 the coupled point reads within 0.5 %);
>   - rev 3's "c1s 2 048 → 768 is a substrate cost" and its quinn-backlog knee term;
>   - rev 3's boot-latched σ̂⁺ (one start-up transient becomes a lifetime queue: one c2 row at 133 ms);
>   - rev 3's close matches of a three-term knee model (fitted post hoc; it holds only at c2 and c8);
>   - rev 4's two-variant c1d registration (replaced by one variant, U).

### R5.8 Owner question (one; R4.7's Q2 is dropped) [REMOVED by R6.6]

R4.7's Q2 is dropped: −11 % at c1s would fail the guard, so the store law is held until the c1s mechanism is known
(R5.3, R5.6).

**Q1. What should Bulk mean at a receiver-bound cell?**
- On c1s's measured curve, the paper's power criterion with α = b(δ) puts Bulk at ≈ 88 % utilisation (−11.5 %
  goodput) and a 1.4 ms queue, and Realtime at 77 %.
- The guard demands no goodput loss, which on a curve means utilisation → 1 at any delay: today's 19 ms loop at
  2 048.
- Should Bulk keep the power criterion, accepting a c1s goodput cost for a ≈ ×3.5 shorter loop (19 → 5.4 ms) and a ×10 smaller queue? Or should Bulk's α grow
  without bound so that Bulk means "full rate", leaving δ's delay authority to Auto and Realtime?
- One formula (R5.4) serves either answer. Only α(δ)'s Bulk end differs.

## R4. Revision 4: held-out test, one class rule, per-row spread (review of rev 3)

Revision 3's three knee terms were fitted post hoc to the three cells they explain, so its close matches test
nothing. This revision adds a held-out cell first.

### R4.1 Held-out prediction: c1d, from boot inputs only (committed before any c1d map is read)

**Disclosure.** p34's c1d static table was printed and committed during revision 2 (`map27-p34.out`), before this
test was designed. **p34 is therefore not a held-out map for this prediction.** It is reported in R4.2 as
contaminated. The s53 c1d block has not been read beyond its boot ticks (`c1dboot.py`: the first 8 `[DIAG]` ticks
of each of its 30 rows).

**Boot inputs** (`c1dboot.out`):

| input | median | range |
|---|---|---|
| ℓ₀, ℓ₁ | 2.52, 2.53 ms | 2.29–3.19 |
| σ⁺₀, σ⁺₁ (boot `qsp_us` maximum) | 4.52, 6.17 ms | 1.26–70.9 |
| Δ = ℓ₁ − ℓ₀ | ≈ 0 | — |

**The model under test** is revision 3's knee cause, with no estimator terms (R4.4): q_knee = σ + ½Δ + B/r.
- σ is the release-jitter cover.
- ½Δ is forward-delay equalisation: 0 here.
- B/r is quinn's backlog dwell, which has no boot input. It is carried from c1s, the same substrate on the same link
  class, as a per-connection dwell of q − σ at c1s: 3.8 ms at the 0.95 point and 9.3 ms at the 0.98 point.
- **σ at load has no boot input either.** c1d's boot σ⁺ (4.5–6.2 ms) is start-up spread: it is ×9–12 c1s's load σ
  of 0.5 ms.
- Two variants are therefore registered. The prediction is the *model*; the variant tells which σ input survives.

**Rate.** c1d is receiver-bound, so the receiver ceiling is taken as c1s's §56 control (875 Mbit/s ≈ 92.5 k
symbols/s at 1 182 B). T₀ is c1s's §56 floor of 4.0 ms.

| | σ = c1s load value 0.5 ms (model M) | σ = c1d boot σ⁺ 4.5–6.2 ms (variant Mb) |
|---|---|---|
| q at the 0.95 point | 4.3 ms | 8.3–10.0 ms |
| **W at the 0.95 point** = μ·(T₀ + q₉₅) | **≈ 770, band 600–950** | **≈ 1 140–1 290** |
| q at the 0.98 point | 9.8 ms | 13.8–15.5 ms |
| **W at the 0.98 point** | **≈ 1 280, band 1 050–1 550** | **≈ 1 650–1 800** |

**Falsifiers:**
- the measured W₉₅ outside 600–950 refutes M;
- W₉₅ outside 1 050–1 400 refutes Mb;
- both outside refutes the decomposition at c1d.

**Where the law would land** (estimators included, R4.4): b̂₀⁺ over-read ≈ 0.2 + σ̂⁺ (boot) 4.5–6.2 + q·ℓ
0.13–0.25 = 4.8–6.7 ms, so W ≈ 92.5 × (4 + 4.8–6.7) ≈ 810–990. That is near M's knee only because the boot σ̂⁺ is
start-up-contaminated. The near-miss is an accident of the estimator, not support for the law.

**What a c1d map can and cannot test:**
- *s53 c1d block* (30 rows, long, at the shipped pin): it can test only the pin's loop, backlog and σ at load. It
  cannot test a knee, because it has no W sweep.
  - Readable without any law: whether c1d's load σ is ≈ 0.5 ms, deciding M against Mb's input.
  - Readable without any law: whether quinn's backlog dwell at the pin matches c1s's.
- *p34 c1d* (static caps 384–2 048, older era, truncated windows): a knee, but contaminated as disclosed above.
- *A clean held-out map* needs §56's sweep run at c1d (a §56b row block). It is registered in R4.6.

**σ's dose-response falsifier: the `adv-cells` jitter cells** (branch `adv-cells` `5f2a29ad`, `tools/l1/adv_cells.sh`).
The cells are jit0 / jit5 / jit15 / jit25: netem delay 20 ms ± J ms (correlation 25 %) on both directions, at
c2-class rate and loss. The model's knee queue is q₉₈ ≈ σ (no leg gap, small backlog at 100 Mbit/s).

Predicted σ (the echo P90 − P50 at load), scaled from c2's 1.8 ms at J = 3 ms each way (σ ≈ 0.6·J), and q₉₈ ≈ 1.3·σ
(c2's ratio):

| cell | J | σ predicted | q₉₈ predicted | W₉₈ ≈ 9.6 k·(40 + q₉₈ ms) |
|---|---|---|---|---|
| jit0 | 0 | 0.1–0.5 ms | ≤ 1 ms | ≈ 390–400 |
| jit5 | 5 | 2–4 | 3–5 | ≈ 410–435 |
| jit15 | 15 | 6–12 | 8–15 | ≈ 460–530 |
| jit25 | 25 | 10–20 | 13–26 | ≈ 510–635 |

**Falsifier:** q₉₈ not monotone in J, or q₉₈/σ outside 0.8–2.0 on any jitter cell. Either means the knee is not
release-jitter cover.

Caveat: under `rate` the forward draws become a running maximum at flow density. That shrinks the forward spread,
so the low halves of the σ bands are the likelier.

### R4.2 The held-out prediction, evaluated (after commit `bcafa8c3`)

**s53 c1d block at the shipped pin** (`c1dpin.py` → `c1dpin.out`, LATE window, 30 rows):
- **σ at load is 1.08–1.32 ms** (arm medians). That is ×2.2–2.6 M's input and ×0.2–0.3 Mb's. Neither input was
  right; M's was closer.
- **Quinn's backlog dwell is 0.84–1.00 ms per leg**, against c1s's carried 3.8–9.3 ms. The carried B/r term does not
  exist at c1d. It also turns out not to be a cause at c1s (R4.3).
- The store is not full at the pin (mean a/W 0.47–0.63), so c1d at W = 4 096 is interior, as the guard row showed.
- **No knee can be read here:** the block has no W sweep.

**p34 c1d (contaminated, older era, truncated windows), reported as instructed.**
- The reference is Gmax 818 at 1 536 (p34 has no shipped control): W₉₅ ≈ 1 080, q₉₅ ≈ 5.6 ms; W₉₈ ≈ 1 350,
  q₉₈ ≈ 7.5 ms.
- M's band (600–950) misses, and Mb's band (1 050–1 400) contains it.
- With c1d's own measured inputs, the decomposition gives q₉₅ = σ 1.2 + ½Δ 0 + dwell 0.9 ≈ 2.1 ms, i.e. W₉₅ ≈ 530.
  That is ×2.7 short of p34's knee queue.

**Verdict: the decomposition is not supported at c1d.** On the only c1d map, contaminated as it is, the knee queue is
×2.7 larger than σ + ½Δ + B/r. Mb's hit comes from a start-up-inflated input and is not evidence.

A clean test is §56's sweep at c1d (R4.6), with the adv-cells dose-response beside it.

### R4.3 c1s: one class rule for every cell, and the attribution withdrawn

Revision 3 used two different tests: "paused ≥ 50 %" for the store, and B ≥ 98 for quinn. On c1s the first never
fires, and it was replaced by a ≥ 0.95·W on c1s alone. That was mislabelled.

`s56cls.py` → `s56cls.out` applies **one rule to c1s, c2 and c8**: FULL iff a ≥ 0.95·W, and "+B" iff quinn's backlog
is ≥ 98 datagrams. It also prints the mean occupancy and the send rate within each class.

| cell, W | mean a/W | FULL+B / FULL / OPEN+B / OPEN | send rate /s in those classes |
|---|---|---|---|
| c1s 2 048 | 0.90 | 0.45 / 0.05 / 0.44 / 0.04 | 97.4 k / 90.2 k / 96.5 k / 89.3 k |
| c1s 1 024 | 0.87 | 0.34 / 0.07 / 0.47 / 0.12 | 92.9 k / 90.9 k / 94.9 k / 88.2 k |
| c1s 768 | 0.86 | 0.28 / 0.10 / 0.45 / 0.18 | 89.7 k / 87.8 k / 89.8 k / 91.8 k |
| c1s 512 | 0.86 | 0.15 / 0.13 / 0.20 / 0.49 | 81.1 k / 90.9 k / 85.2 k / 85.7 k |
| c1s 384 | 0.85 | 0.11 / 0.17 / 0.13 / 0.58 | 74.8 k / 77.9 k / 77.1 k / 76.2 k |
| c2 ≤ 384 | 0.96–0.99 | 0 / 0.65–0.89 / 0 / 0.11–0.35 | ≈ equal within a row |
| c8 ≤ 1 280 | 0.96–0.99 | 0–0.31 / 0.65–0.85 / ≈ 0 / 0.03–0.29 | ≈ equal within a row |

**What was mislabelled.**
- At c1s the store averages 0.85–0.90 of W, so it *is* store-bound. Its occupancy is a sawtooth of ≈ 10–15 % of W,
  because slots return in ack batches (§23: ≈ 400 symbols per ≈ 1 ms).
- A per-tick ≥ 0.95 test therefore misses most store-bound ticks. Revision 3's "store full 5–18 %" and "OPEN 49–58 %"
  measured the sawtooth's phase, not the limiter.
- At c2 and c8 the sawtooth is small against W, and the same rule reads FULL on 0.65–0.89 of ticks.

**"+B" is not a limiter either.**
- Within a c1s row, ticks with quinn's backlog ≥ 98 have the *higher* send rate (at 2 048: 97 k against 89–90 k). The
  backlog marks the moments the store has just refilled quinn.
- The rate falls with W in every class (from 89–97 k at 2 048 to 75–78 k at 384).
- The backlog's share falls with W *because* the store feeds quinn less. It is an effect of W, not a cause.

**Withdrawn:**
- "2 048 → 768 is a substrate cost";
- the B/r term of R3.1's c1s decomposition.

**What the data supports:** the c1s slope is a store-size cost at every W measured. It is the release granularity
(ack-batch sawtooth) plus receiver batching, which keeps raising goodput with transit (§23 Q3, §16 finding 4). The
c1s knee queue (9.8 ms at 0.98) is **unexplained** by any term here, beyond σ ≈ 0.5 ms.

### R4.4 Separating "what the knee is" from "where the law lands"

No law ran in §56, so no estimator over-read can be part of the knee's cause. Revision 3's c2 and c8 rows mixed them;
they are split here.

**The knee (§56 only, no estimator terms):**

| cell | measured q₉₈ | candidate cause | its value | ratio |
|---|---|---|---|---|
| c2 | 2.4 ms | σ (load echo spread, 1.7–1.95 at every W) | 1.8 | 1.3 |
| c8 | 18.2 ms | ½Δ + σ₀ (Δ 29.5, σ₀ 2.0–2.7) | 17.0–17.5 | 1.04–1.07 |
| c1s | 9.8 ms | σ | 0.5 | 19.6: **unexplained** (R4.3) |
| c1d (p34, contaminated) | ≈ 7.5 ms | σ + ½Δ | 1.2 | 6.3: **unexplained** |

Read honestly, the decomposition holds only where the knee is a network-placement or jitter knee (c2, c8). It fails on
both receiver-bound cells. On c1s and c1d some other store-size mechanism sets the knee, and no measured term
here predicts it.

**Where the law lands, with its estimators.** The law's queue target is b̂₀⁺'s over-read + σ̂⁺ + q·ℓ (+ ½Δ̂). Both
b̂₀⁺ and σ̂⁺ are latched at boot, and both measure echo jitter from the same boot windows:
- b̂₀⁺ is the lower tail: the window minimum against the filter minimum.
- σ̂⁺ is the upper half: P90 − P50.
- They are not the same statistic, but **they are contaminated by the same start-up transient**, so their errors
  add rather than cancel.

**VOID (R5.5 replaces this table; it used the boot σ̂⁺ rejected just below).** `s56cls.out` §(2) gives each row's point from its own boot ℓ and σ̂⁺, with the ledger b̂₀⁺ over-read (median and max):

| cell | rows | law's W per row (median) | below the 0.95 knee | upper tail |
|---|---|---|---|---|
| c2 (knee 169) | 18 | 194–210 typical (199–204) | **0 / 18** | one row's boot σ̂⁺ latches a start-up spread: target 132 ms, W ≈ 1 470 |
| c8 (knee 340) | 24 | 428–1 280 (474–483) | **0 / 24** | at the ledger max over-read (48.8 ms): 1 038–1 280 |
| c1s (knee 903) | 27 | 510–2 048 (638–664) | **21–23 / 27** | boot σ̂⁺ up to 21 ms reaches the clamp |

- At c2 the margin is 25–41 slots above the knee for typical rows. Every row clears the knee, where 32 slots below it
  cost 6 %.
- **But a boot-latched σ̂⁺ turns one start-up transient into a lifetime standing queue.** The c2 outlier carries a 133 ms
  standing queue, about 0.7 × the shipped pin's, and c1d's boot σ̂⁺ reaches 71 ms (`c1dboot.out`).
- **σ̂⁺ at boot is rejected as an input.** σ at load is stable on single FIFO legs (c2 1.7–1.95 ms across W 128–2 048;
  c8 leg 0 2.0–2.7). It inflates only on a placement-bursty leg (c8 leg 1: 4.9 → 31.6).
- The candidate is σ read at load on each leg, and the open question is the bursty leg (Part B, R4.7).

### R4.5 The ½ is derived from the harness, not a coincidence; an asymmetric cell is its falsifier

**What the harness does.** `tools/l1/topo_dual.sh` shapes **both** directions:
- `shape … cli0/cli1` puts the full scenario (delay, jitter, rate, loss) on the client's egress (lines 95–96);
- `tc qdisc add dev srv0/srv1 root netem delay "${ow}ms" $jitter rate …` puts the same one-way delay, jitter and rate,
  with no loss, on the server's egress (lines 104–106).

So c8's legs are 5 + 5 ms and 20 + 20 ms. p27's `QDISC srv0/srv1` lines confirm `delay 5ms 3ms` and `delay 20ms 5ms`.
The store's queue sits on the forward (client-egress) qdisc only. Forward-arrival equalisation therefore needs a
leg-0 queue of (20 − 5) ms = ½·Δ on these legs. The ½ is a consequence of symmetric shaping, not a fit.

**The falsifier (a design point; it needs a harness option to set the server-side delay independently):**

| cell | leg 1 forward / return | Δ (loop gap) | forward gap | knee predicted by the ½ form | by forward equalisation |
|---|---|---|---|---|---|
| c8 (as run) | 20 / 20 ms | 30 ms | 15 ms | ≈ 340 | ≈ 340 (both; measured 340) |
| **c8a** | 35 / 5 ms | 30 ms | 30 ms | ≈ 340 | **≈ 470–530** |
| **c8b** | 5 / 35 ms | 30 ms | 0 ms | ≈ 340 | **≈ 200–250** (leg 1 fed with no leg-0 queue) |

The ½ form is refuted if c8a's knee is ≥ 450 or c8b's is ≤ 280. Then the coupling term must read a forward-delay gap,
not ½ of the loop gap. Its gauge is the receiver's reorder-hole lifetime (`[SUCC] orig_p50`).

### R4.6 N = 1 check

Revision 3's line is Fᵢ = m̃ᵢ + b̂₀ᵢ⁺ + ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺)). At N = 1 the inner max is the
leg's own ℓ̂ + b̂₀⁺, so the gap term is max(0, 0) = 0 and Fᵢ = m̃ᵢ + b̂₀ᵢ⁺. That is revision 2's per-leg floor, own leg
included.

The full target at N = 1 is revision 2's per-leg target **plus σ⁺**. That difference is deliberate: it is the
release-jitter term, present at every N, and not an N artefact. The line needs no fix for N.

σ⁺'s source changes per R4.4: boot is rejected, load is the candidate.

### R4.7 Owner questions (replace R3.7)

1. **Where should δ put the operating point on the measured curves?** Above the knee, the trade is measured:
   - c2 and c8 gain under 1 % of goodput for 2.5–11 × the delay (c2 192 → 2 048: +0.7 %, loop 19 → 208 ms;
     c8 512 → 1 280: +0.5 %, loop 42 → 105 ms).
   - c1s gains 8–12 % for 1.9–3.8 × the delay (W 512 → 1 024 → 2 048: loop 5 → 9.5 → 19 ms).
   - δ's present seat (q·ℓ ≤ 1–1.4 ms) cannot reach c1s's knee at all.
2. **c1s and c1d:** accept that the law sits below the receiver-bound knee (≈ −11 % at c1s) until the knee's
   mechanism is measured, or hold the store law until it is?

(Revision 3's Q3 is withdrawn: the ½ is derived from the harness, and its falsifier is the c8a/c8b design point in
R4.5.)

### R4.8 Part B: registration of a gauge-only, open-loop build (no law change)

**What.**
- A build that **prints** the derived law's inputs and its counterfactual update at every window close, and never
  applies it.
- The shipped cap sequence must be bit-identical with the gauge on. Gates: a host test comparing cap sequences, and
  the native golden test with `GOLDEN_CAPTURE` unset.
- It decides nothing. It replaces post hoc fits with logged inputs, and supplies the held-out maps.

**What the engine must print: `[SLAW]`, one line per window close (250 ms), per live leg i:**

| field | definition | why |
|---|---|---|
| `n`, `W`, `aw` | the store cap, the counted store at the close, and the mean a/W over the window | occupancy without the sawtooth phase (R4.3) |
| `nk`, `tmin`, `tp50`, `tmean` | echo samples in the window: count, minimum, median, mean | the law's T̂, T̂min and w; tmean for Little (R3.4) |
| `m`, `l` | Copa's 10 s echo minimum; the lifetime echo minimum (new running min) | the anchor and its bound |
| `b0p`, `b0k` | b̂₀⁺ candidate (max of the first K brakes), and the K brakes themselves (once, at K) | B1, and b̂₀⁺'s tail per row (13.1) |
| `sig`, `sigb` | P90 − P50 of the window's samples (load), and the boot maximum | R4.4's σ input choice |
| `gap` | ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺)) | the coupling term |
| `anc` | 1 iff m̂ > ℓ̂ + b̂₀⁺ | E03's bound bind |
| `R` | the counterfactual Σwᵢ·targetᵢ / Σwᵢ·T̂ᵢ, at q(δ) of the run, with σ at load and at boot (two values) | the law's sign per window, never applied |
| `qb`, `qbt` | quinn backlog B (datagrams) and B/r (ms) | the substrate's share |

**Counters.**
- The share of gap reports truncated at `MAX_NACK_GAPS` (visibility).
- The C09 per-report cap bind.
- The quinn-collapse flag (§56 Amendment 2's absolute rule).

**Arms** (all reported only; static pools via `RWM_STORE_PATH_POOL` as §56):
1. §56's c8/c2/c1s sweep, re-run on the gauge build: the R crossing against the measured knee per cell.
2. **c1d sweep, the held-out map.** W = 4 096 (control), 2 048, 1 536, 1 024, 768, 512, 384, n = 4. It scores R4.1's M
   and Mb.
3. **adv-cells jitter:** jit0 / jit5 / jit15 / jit25 (branch `adv-cells` `5f2a29ad`). Single path, W = 2 048 (control)
   plus a sweep of 384–1 024 around the predicted W₉₈, n = 3. It scores R4.1's dose-response.
4. **c8a / c8b**, which need the server-side delay option: the c8 sweep's W values, n = 3. They score R4.5.

**Readouts.**
- Per cell: the measured W₉₅ and W₉₈ and q₉₅ and q₉₈ (§56's definitions).
- The W where the median R crosses 1, for each σ choice.
- Per-row law points from logged boot and load inputs, and rows below the 0.95 knee.
- The `anc` bind share, and the b̂₀⁺ tail (max − median of its K brakes).

**Falsifiers.**

| # | what fires it | what it means |
|---|---|---|
| B1 | b̂₀⁺ < the row's later no-queue brake − q·ℓ̂ on any row | the collapse condition |
| B2 | R's crossing (σ at load) below the 0.95 knee on more than 1 row in 6 at c2 or c8 | the law lands below the knee |
| B3 | c1d W₉₅ outside both M's and Mb's bands | the decomposition fails at c1d |
| B4 | adv-cells q₉₈ not monotone in J, or q₉₈/σ outside 0.8–2.0 | the knee is not release-jitter cover |
| B5 | c8a knee ≥ 450 or c8b ≤ 280 | the ½ form fails |
| B6 | `sig` at load on c8 leg 1 above 3 × its boot value at the law's crossing | σ at load is not usable on a bursty leg |

**Decision rule.** None. Arms 2–4 are the first held-out tests of the law's terms. A closed-loop arm is registered only
after B2–B6 are read.

## R3. Revision 3: the law on §56's maps (supersedes §0's formula, §4, §5.1 and §5.2's numbers; revised by §R4: the c1s decomposition, the B/r term and boot σ̂⁺ are withdrawn, and R3.7 is replaced)

**Sources.** Status §56 Result (main `e195c8c0`), its ledger `docs/l1-raw/s56/rep56-*`, and `store-law/s56.py` →
`s56.out` (per-tick readouts on the raw `[DIAG]`). The p27/p34 maps are kept only as the record of revisions 1–2.
Every prediction and fixed point below is read on §56's 600 MB / 5 GB current-era maps. **They are still static
crossings, and still not registrable** (§43).

### R3.1 What the knee queue scales with

Below, q₉₈ is the standing queue T̄ − T₀ at the W where goodput first reaches 0.98 of control (§56 score,
interpolated). All candidate values are measured at that W, from the §56 rows (`s56.out`; §56 score lines 259–268):

| cell | q₉₈ ms | ℓ (q/ℓ) | echo spread σ = P90 − P50 (`qsp_us`), leg 0 (q/σ) | BDP μ·T₀ in symbols (q·μ/BDP) | quinn backlog time B/r (q/that) | leg-floor gap Δ = T₀,₁ − T₀,₀ (q/Δ) |
|---|---|---|---|---|---|---|
| c2 | 2.4 | 10.0 (0.24) | 1.8 (1.3) | 154 (0.15) | ≈ 1.0 (2.4) | — |
| c8 | 18.2 | 14.4 (1.26) | 2.5 (7.3); leg 1 15.5 | ≈ 247 (0.9) | ≈ 0.9 (20) | 29.5 (0.62) |
| c1s | 9.8 | 2.6 (3.85) | 0.5 (19) | ≈ 352 (2.4) | ≈ 6.6 (1.5) | — |

**No single quantity scales.** The table rejects ℓ (×16 across the cells), σ (×15), the BDP (×16), quinn's backlog
(×13) and the wire RTT. (The wire RTT at the knee is 18 / 25–37 / 3–4 ms, and the wire loop is not where the c1s
queue sits.)

The knee queue is a sum of three measured mechanisms, one per cell type:

| cell | decomposition of q₉₈ | sum | measured q₉₈ |
|---|---|---|---|
| c2 (one jittered FIFO leg) | σ (release jitter: the bottleneck idles unless its queue covers the report spread) + the b̂₀⁺ over-read 0.4–0.9 | 2.2–2.7 | 2.4 |
| c8 (two legs, delivery-time placement) | ½·Δ (forward-delay equalisation, R3.3) + σ₀ + b̂₀⁺ excess ≈ 14.75 + 2.5 + 1.0 | 18.3 | 18.2 |
| c1s (receiver/substrate-bound) | σ + quinn's backlog dwell B/r ≈ 0.5 + 6.6 | 7.1 | 9.8 (rest unexplained, R3.5) |

**The allowance's form changes accordingly.** The δ term is a margin above the measured mechanisms, not the knee
itself:

```text
   target_i  =  Fᵢ  +  σ̂ᵢ⁺  +  q(δ)·ℓ̂ᵢ  +  Ŝᵢ                         (replaces Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ)

   Fᵢ    =  m̃ᵢ + b̂₀ᵢ⁺  +  ½ · max( 0 ,  maxⱼ (ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺) )        (half the floor gap: R3.3)
   σ̂ᵢ⁺   =  max over the K boot windows of the leg's echo spread P90 − P50            (one-sided, like b̂₀⁺)
```

- **σ̂ᵢ⁺** is the release-jitter cover.
  - *Sentence:* the bottleneck stays busy only if its queue floor covers the spread of the report times that
    release the store's slots.
  - Measured: the engine already computes `qsp_us`, P90 − P50 of the leg's last 256 echo samples
    (`path.rs:848–855`).
  - It is taken at boot, as b̂₀⁺ is, because the spread inflates with load on a placement-bursty leg (c8 leg 1:
    4.9 ms at W 160, 31.6 ms at 1 280). A load-read σ would close §2's max-form loop again.
  - It is δ-free.
- **q(δ)·ℓ̂ᵢ** stays the one δ seat, continuous in δ. It is now an explicit margin above the knee.
- **c1s's backlog term is not added.** R3.5 shows c1s is QUINN-LIMITED on most ticks. Funding quinn's backlog
  would remove the store's only signal there and run to the clamp. That is owner question R3-Q1.

### R3.2 Static points on §56's maps

Each cell's target queue is the sum of its terms. The point is read off the cell's (W, q) curve in §56 score lines
260–266.

| cell | target queue (Realtime → Bulk) | law's point W | goodput / control | loop T̄ | in-order late mean | knee (0.95) | margin above the knee |
|---|---|---|---|---|---|---|---|
| c2 | 0.4–0.9 + 1.8 + 0.5 → 1.0 = 2.7–3.7 ms | ≈ 187–196 | 0.985–0.99 (88.8–89.6 / 90.2) | ≈ 19–20 ms (shipped: 208) | 0.02 s (shipped: 0.41) | 169 | +18–29 symbols, ≈ +2–3 ms of loop |
| c8 | 14.75 + 2.5 + 1.0 + 0.7 → 1.4 = 18.9–19.7 ms (leg 0) | ≈ 460–475 | 0.98–0.99 | ≈ 38–40 ms (shipped W 4 096: 343) | ≈ 0.08 s (shipped 1 280: 0.37) | 340 | +120–135 symbols, ≈ +10 ms of loop, ≈ +0.03 s in order |
| c1s | 0.15 + 0.5 + 0.13 → 0.26 = 0.8–0.9 ms | ≈ 485–500 | **0.87–0.88** (763–772 / 875) | ≈ 5 ms (shipped 19) | — | 903 | **below the knee**: R3.5 |

On the revision-2 forms, with Fable's first read confirmed on the score's curves:

| cell | form | §56 reading |
|---|---|---|
| c2 | per-leg | 174–192 → 0.96–0.99; "expired" 140–164 → 0.84–0.94, on the cliff |
| c8 | coupled, full floor gap | ≈ 550 → within 0.5 % of control |
| c8 | per-leg | 148–212 → ≈ 0.75–0.85 (W 160: 0.75; 224: 0.87) |
| c1s | — | 370–509 → 0.79–0.89 |

**The guard claim is withdrawn.** Revision 2's "c8 −3 to −5 %, fails the guard" came from p27's 8 s rows.
- On §56's maps the coupled point reads within 0.5 % (full gap), and 0.98–0.99 with the half-gap F. That is inside
  c8's 2 % d80 at the full gap, and at its edge at the half gap.
- c2 reads −0.6 to −0.9 %, inside its 1 % resolution.
- **Only c1s fails** (−12 to −13 % at Bulk, against a 5 % d80), and its cost is the substrate's (R3.5).
- The trade at c8 between the half and the full floor gap: the full gap costs ≈ +15 ms of p0 first arrival and
  ≈ +80 symbols of store, for ≈ +1 % goodput. Neither is picked here; see R3.3.

### R3.3 The c8 knee miss: forward delays equalise, not loops

**Measured knee W ≈ 340 against a predicted 500–700.** The shoulder is leg 1 starving: r₁ 1 853 → 1 718 → 1 113 →
405 → 188 at W 1 280 → 512 → 320 → 224 → 160.

**What the prediction assumed.** Leg 1 is fed once leg 0's *loop* reaches leg 1's no-queue loop (T₀,₁ ≈ 46 ms),
i.e. a leg-0 queue of Δ ≈ 29.5 ms.

**What placement prices.** Placement prices expected *arrival*: E_i = (I/W)·s + (½ + ε)·wire (K05). Half the wire
RTT stands in for the forward delay. The c8 legs are symmetric: netem holds 5 + 5 ms on leg 0 and 20 + 20 ms on
leg 1, and the queue sits on the forward direction only. So the legs' arrivals equalise when leg 0's forward delay
reaches leg 1's, i.e. at a leg-0 queue of ≈ Δ/2 ≈ 15 ms. That is a leg-0 loop of ≈ 30 ms.

On §56 that loop is W ≈ 320–340 (rtt₀ 26 ms at 320, 39 at 512), which is where the knee is. The prediction confused
loop with forward delay by a factor of 2 in the gap. The softmax temperature (K06) rounds the shoulder: leg 1 still
carries 405 /s at W = 224.

**The same correction applies to the law's coupling.**
- The in-order frontier waits for the slowest leg's *arrival*, so the coupled floor term should fund half the
  floor gap, not all of it. That is R3.1's ½·max(0, …), continuous, and 0 at N = 1.
- **Domain:** per-leg symmetric delay. An asymmetric leg (a long return path) breaks the ½.
- **Gauge:** the receiver's reorder-hole lifetime (`[SUCC] orig_p50`), which measures the forward-arrival gap
  directly (49–106 ms on the §54b arms). It must read ≈ 0 at the law's point.
- The full-gap form of revision 2 over-funds leg 0 by the return-delay gap. It is kept as the alternative with its
  cost in R3.2.

### R3.4 F5: the per-tick Little residual

Recomputed per tick with means (`s56.py`): a − Σᵢ rᵢ(t)·rttᵢ(t), averaged over the LATE window. At N = 1 it was
cross-checked with the 0.1 ms top-level `rtt`; both agree within 0.05 ms.

| cell | resid / W (unflagged arm medians) | resid ÷ Σr, ms (store time outside the echo loop) |
|---|---|---|
| c1s | −0.058 … +0.027 | −0.08 (W 384), 0.16, 0.57, 0.72, 1.28 (W 2 048) |
| c2 | −0.042 … −0.010 | 0.33–0.57 at W ≤ 384; −0.13 at 2 048 |
| c8 | −0.032 … +0.011 | 0.27–0.91 at W ≤ 512; 1.9, 5.1 at 768, 1 280 |

- **F5's firing was the integer-millisecond median, not Little.** With per-tick means, |resid| ≤ 0.06·W on every
  arm. That includes c1s, where the scored medians read 0.15–0.24.
- **A small out-of-loop store time remains:** ≈ 0.3–0.9 ms at c2 and c8 up to W 512 (the ACK→SACK release lag, W1's
  λ). It is not fixed: it grows with W at c1s (with quinn's backlog) and at c8.
- **It does not enter the reference.** In the ratio law the fixed point is a condition on the echo loop (R = 1).
  Store time outside the loop changes the store size n* = Σλᵢ(T*ᵢ + τ_out) and the loop gain, as H does (§3.4), but
  not the fixed point. It is ≤ b̂₀⁺'s over-read at every cell, so no separate term is added.

### R3.5 c1s: who limits, per tick (W1.1 classes)

The classes are those of status §48b and W1 §5, computed per tick on unflagged rows:
- QUINN-LIMITED: quinn's backlog B ≥ 98 datagrams.
- STORE-LIMITED: paused ≥ 50 % and B < 98.
- The pause test never fires at c1s, because the sender wakes on every ack batch even with the store at its cap.
  A second split uses the store itself: FULL is a ≥ 0.95·W with B < 98; OPEN is a < 0.95·W with B < 98.

| c1s W | 2 048 | 1 024 | 768 | 512 | 384 |
|---|---|---|---|---|---|
| QUINN-LIMITED | **0.89** | **0.82** | **0.73** | 0.39 | 0.25 |
| store FULL, B < 98 | 0.05 | 0.07 | 0.10 | 0.13 | 0.18 |
| OPEN (neither) | 0.04 | 0.12 | 0.18 | **0.49** | **0.58** |
| quinn backlog / W | 0.43 | 0.36 | 0.26 | 0.13 | 0.14 |
| goodput / control | 1 | 0.96 | 0.94 | 0.89 | 0.79 |

c2 and c8 at reduced W: STORE-LIMITED 0.97–1.00 and FULL 0.65–0.89, QUINN-LIMITED ≤ 0.03 (c8 1 280: 0.33).

**Reading: the c1s slope is not a store cost at W ≥ 768, and only partly one below it.**
- **2 048 → 768 (−6 %): substrate.** Quinn holds 73–89 % of ticks with ≥ 98 datagrams queued, and 26–43 % of the
  store sits in quinn's send buffer. The store's transit is W·(1 − B/W) ≈ 1 160 → 567. Shrinking W shrinks quinn's
  backlog first.
- **768 → 384 (−15 %): neither limiter.** The store is not full and quinn has no backlog on 49–58 % of ticks, so the
  pipe empties between ack batches. That is the release granularity at 1 Gbit/s (§23: acks arrive in bursts of
  ≈ 400 symbols), and it is a store-size cost.
- A store law cannot see quinn's buffer as anything but standing queue (the echo includes it). So at c1s it
  shrinks W into the OPEN regime. The c1s cost is quinn's window (K09) plus the release burst. The allowance cannot
  close it without funding quinn's backlog (R3-Q1).

### R3.6 Quinn collapse at c1s, and any closed-loop c1s test

- Collapses hit 9 of 27 c1s rows, flat in W (P2048 3/9, P512 4/9).
- Post hoc, onsets come earlier at pools ≤ 512: 1–25 s, against 29–49 s.
- On flagged rows, quinn's backlog is 0.63–0.97 of W, against 0.13–0.43 on unflagged rows (`s56.out`).

What this means for a closed-loop test:
1. The law's c1s point (≈ 500) sits in the region of early onsets, so closed-loop c1s rows will carry more
   collapse time than the shipped pin does. A collapse flag and exclusion (§56 Amendment 2's absolute flag), and
   n ≥ 9 per arm, are prerequisites.
2. During a collapse the echo loop jumps (rtt 7–80 ms against a wire 2–5 ms), so the law reads it as standing queue
   and shrinks W. That is the right direction for delay, but the shrink is driven by the substrate, not the
   network. The test must report W and R across each onset, and must not score goodput on collapsed spans.
3. Since §43's c1s "runaway" was a collapse under an oracle L, the time law's failure mode at c1s is the opposite: a
   collapse-driven contraction. Its gauge is the share of collapsed windows with R < 1.
4. **A closed-loop c1s test waits** for R3-Q1 and, likely, for K09/K10 (quinn's BBR against `bbr_rs`).

### R3.7 Owner questions (revision 3; these replace §12 for decisions now open)

1. **c1s.** At c1s the substrate (quinn's window), not the store, sets the knee.
   - Option (a): accept ≈ −12 % at c1s until the substrate question (K09/K10) is settled.
   - Option (b): let the store also fund quinn's own send backlog. That recovers goodput, but it ties the store's
     size to the substrate's queue and can run to the clamp.
2. **δ's authority.** Moving from Realtime to Bulk changes the law's point by only a few symbols (q·ℓ ≤ 1 ms at c2,
   ≤ 1.4 ms at c8).
   - Is that the intended authority?
   - Or should δ scale a larger quantity, for example the whole margin above the knee?
3. **The two-leg term** assumes each leg's delay is the same in both directions, so it funds half the floor gap.
   - Accept that domain, with the receiver's reorder-hole lifetime as its gauge? Or require a measured forward
     delay?
   - Separately, half gap or full gap at c8: ≈ −1 % goodput against ≈ +15 ms of fast-leg first arrival.

## 0. Answer

*Superseded in its formula and predictions by §R3; kept as revision 2's record.*

*Revision 2 (after review).* Three blockers are closed in this revision:
- b₀ is now a one-sided upper bound (§3.5);
- the anchor's rise is bounded, with a gauge (§3.3);
- the leg target is coupled by the in-order argument, settled on the map (§5.2).

O1 is no longer an owner option: it is the formula.

**The law needs no rate.** Fed an honest delivered rate, the shipped law is already a pure time law,
cap' = cap · T*/T̂. Its collapse at c2 is a time-target defect, T* < T₀, not a rate defect (§2). The derived
law reads only times, plus dimensionless per-leg weights. One update per 250 ms window k:

```text
   n_{k+1}  =  clamp( n_k · Σᵢ wᵢ·( Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ·T̂ᵢ ,  floor ,  N·knee )

   Fᵢ    =  max( m̃ᵢ + b̂₀ᵢ⁺ ,  maxⱼ ( ℓ̂ⱼ + b̂₀ⱼ⁺ ) )         target queue-free floor of leg i
   m̃ᵢ    =  min( m̂ᵢ ,  ℓ̂ᵢ + b̂₀ᵢ⁺ )                          10 s echo anchor, rise-bounded
   b̂₀ᵢ⁺  =  max over the first K = 8 windows of ( T̂minᵢ,ₖ − m̂ᵢ,ₖ )   one-sided upper bound of the no-queue brake
```

> *Sentence:* once per window, scale the store by the ratio of target loop to measured loop, averaged over the
> legs by delivered share. A leg's target loop is the later of its own queue-free floor and the slowest leg's
> lifetime floor, plus CoDel's setpoint q(δ) on its own lifetime floor, plus its spread. In-order delivery waits
> for the slowest leg, so a faster leg's queue up to that floor costs no in-order delay.

**At N = 1 it reduces to the per-leg law**, since ℓ̂ ≤ m̃. Its fixed point is a time condition: the standing
queue floor equals q·ℓ plus the bound's over-read (b̂₀⁺ − b₀). Little then gives the store as
n* = Σλᵢ·T*ᵢ + H. The holes are funded implicitly (§3.4).

**The three asymmetric inputs, each closed by a one-sided form:**
- **b₀.** An under-read by more than q·ℓ collapses the store; an over-read only adds that many ms of queue
  (§3.5).
  - The unbiased boot median collapses 6/6 c2 rows, 4/4 c8 leg-0 rows and 3/4 c8 leg-1 rows on the static
    maps.
  - The boot sample maximum b̂₀⁺ collapses none of the no-queue rows on either ledger, apart from one row that
    was not queue-free: a known quinn-window-collapse outlier.
  - Its median cost is 0.41 ms (c2), 1.0 ms (c8 leg 0), 3.7 ms (c8 leg 1) and 0.1–0.5 ms (c1).
  - The total allowance above b₀ at c2 is ≈ 0.8–1.4 ms, against the ≈ 2.7 ms of §43b/W1, the thin allowances
    measured as working.
- **The anchor.** The anchor can rise above the lifetime floor by at most b̂₀⁺, so it cannot drift (§3.3).
  - Unbounded, the drift would reach 16–24 ms/min at c2 and c8, and 2–4 ms/min at c1d. That is enough to re-pin
    a long c1d row within minutes.
  - The wire candidates read honest values here (`bbr_rs` m̂ and quinn's `rtt.min()`), but only on the `bbr_rs`
    substrate. They remain a later replacement.
- **The slow leg's target.** A per-leg target leaves an idle leg at its own target, whatever the weights.
  - Equal weights land at 164–220 against 148–212 with delivered share.
  - Only the coupled F moves c8, to ≈ 545–553 on both legs.
  - Every form has one attractor on the static map, and every start-high returns (§5.2).

**Predictions are static-map crossings. They are NOT registrable: §43 shows static reasoning fails in closed
loop** (c1s predicted to converge, one runaway).

| cell | point, Realtime → Bulk (fresh) | goodput on the static map | guard consequence |
|---|---|---|---|
| c2 | 174 → 192 | 0.92 → 0.984 | inside the 1 % c2 resolution at Bulk only |
| c1s | 370 → 509 | 0.79 → 0.84 | **−10 to −20 %: fails the guard** (c1s goodput d80 5 %) |
| c1d | 518 → 622 | 0.87 → 0.89 | **fails the guard** (d80 8 %) unless main is itself below Gmax |
| c8 | ≈ 545–553, both legs | ≈ 0.95 on 8 s rows | likely −3 to −5 %: **fails** at the c8 d80 of 2 % unless the 600 MB maps (§13) show less |

The Bulk costs are the δ trade (U3). As written, the law would not pass guard v2 at c1s, c1d or probably c8.

**§54b does not bracket the knee.**
- The 640 arms sit at 6.8 × BDP, and leg 1 is already on its shoulder there (L r₁ 1 974–2 009 at W = 4 096
  against 1 866–1 946 at 1 280).
- c8 has a leg-1 shoulder near W ≈ 500–700 and a leg-0 shoulder near 180–250.
- **§13 is a stand-alone, reported-only registration** of the pool sweep that measures both. It needs no build
  and does not depend on the blockers.

**hn_n** is (reorder holes) × (visibility through the 20-run horizon), explained in both directions. Its p0 excess
on a FIFO leg is open, and it touches launch 1's guard witness (§6).

## 1. Little's law on §54b, per leg (check 1)

LATE window: sending ticks at t ≥ 15 s. The inputs per row:
- rᵢ = Δ`dgqᵢ tx`/Δt;
- rttᵢ = app-echo EWMA, `wrtt` = quinn;
- T_W = W/Σr is the store's residence time if the whole store were the queue;
- T̄ = Σrᵢ·rttᵢ/Σrᵢ is the rate-weighted mean echo loop;
- resid = `win` − Σrᵢ·rttᵢ.

Medians of per-row values, with [row ranges] where they matter. Source: `little54b.out`.

| arm | W | r₀ /s | r₁ /s | rtt₀ / wrtt₀ ms | rtt₁ / wrtt₁ ms | q₀ | **T_W** ms | **T̄** ms | resid (old holes) |
|---|---|---|---|---|---|---|---|---|---|
| B2048 | 4 096 | 9 811 | 1 938 [1 897–1 979] | 198 / 118 | 235 / 222 | 873 | 349 | 205 | 1 601 (1 289) |
| B1024 | 2 048 | 9 828 | 1 923 [1 862–1 995] | 152 / 132 | 224 / 191 | 153 | 174 | 162 | 118 (0) |
| B640 | 1 280 | 9 811 | 1 882 [1 839–1 913] | 96 / 91 | 146 / 133 | 18 | 109 | 104 | 48 (0) |
| L2048 | 4 096 | 9 845 | 1 982 [1 974–2 009] | 354 / 112 | 294 / 284 | 2 374 | 346 | 343 | 35 (0) |
| L1024 | 2 048 | 9 802 | 1 989 [1 922–2 006] | 162 / 126 | 217 / 197 | 343 | 174 | 171 | 17 (0) |
| L640 | 1 280 | 9 826 | 1 916 [1 866–1 946] | 98 / 92 | 146 / 130 | 22 | 109 | 107 | 19 (0) |

**It holds, per leg.**
- On the L arms (no old holes), T_W and T̄ agree within 1–3 % (resid/W 0.3–3.6 %).
- On B2048 the residual is the hole population (old holes 1 289 ≤ resid 1 601 ≤ all open holes 2 023).
- The brief's comparison with p0's RTT alone (354 / 162 / 98 against 350 / 175 / 109) is the 83 %-weighted
  approximation. Leg 1's loop (294 / 217 / 146 ms) is the rest.

**Units.**
- `dgq tx` counts wire datagrams: source, retransmits (≈ 14–16 k per run, ≈ 2.8 % of tx) and coded symbols
  (≈ 0.4 %).
- The counted store holds source seqs, and a retransmit re-occupies its seq's slot.
- So Σr reads the store's unit flow high by ≈ 3 %, which is the size of the L residuals. The law below never
  imports a rate, so this mismatch, and W3's (1 + r), do not reach it (§3.2).

**Consequences.**
- At every arm both legs run at or near link rate: Σr = 11.63–11.85 k/s. The store is the queue:
  **W − H = Σᵢ λᵢ·Tᵢ, and T inflates until it fills W.**
- T_W = W/Σλ is linear in W at a fixed rate: 349 → 174 → 109 ms.
- The extra population sits wherever the binding window lets it:
  - quinn's p0 buffer when W > qcwnd₀ (L2048: q₀ 2 374);
  - netem's queue when W < qcwnd₀ (B640: q₀ 18, wrtt₀ 91 ms on a 10 ms leg).
- In-order delay tracks the loop: late mean ÷ T̄ is 2.7–3.4 on B640 and the three L arms (B1024 5.2; B2048 27,
  with old holes). It is a loop for the frontier plus about two repair loops, because a hole opens every
  ≈ 3 ms at 3 % loss and takes ≈ 1.5 loops to close (`[SUCC] rep_p50` 147–295 ms).

**Shape of the law, from the identity.** The size the store needs is
n* = Σᵢ λᵢ·T*ᵢ + H(horizon, T*), and it is wrong whenever T* is wrong.

The question is the target time T*, not the store count. A fixed constant (640, or any other) pins T_W = W/Σλ
to whatever the cell's rate makes it: 109 ms at c8, ≈ 133 ms at c2, ≈ 14 ms at c1s. That is not a law.

## 2. Does the law need a rate? No, and why every rate candidate failed

**Derivation.** On a store-bound sender, Little gives the delivered rate as λ = (n − H)/T̄. Any law of the form
n' = λ̂·T* fed by the store's own delivery is

```text
   n'  =  (n − H) · T* / T̄  +  H·(…)          →  a ratio of two times; the rate cancels
```

Its fixed point is T̄ = T*. It grows when the leg has no queue (T̄ = T₀ < T*) and shrinks above. §23's
"circularity" is this loop, and it is harmless iff **T* > T₀**, where T₀ is the no-queue loop on the same clock.

The shipped law sets T* = (1 + q)·RTprop. RTprop is the 10 s minimum and T₀ is the loop the store holds a symbol
for: minimum + b₀ (the short-against-long minimum gap) + S (spread), plus the ACK→SACK release lag. So
T*/T₀ < 1 wherever b₀ + S > q·RTprop:

| cell | (1 + q)·RTprop | T₀ (no-queue loop) | g = T*/T₀ | measured with an honest rate |
|---|---|---|---|---|
| c2 | 1.1 × 6.5–6.9 ≈ 7.4 ms | 12–15 ms (§23); p27 at 64–128: Tp50 14.9–16.1 | 0.5–0.6 | 41 → 10 in 3 s, 7.2 Mbit/s (§23 Q2) |
| c1s | 1.1 × 2.24 ≈ 2.5 ms | ≈ 2.9 ms (§23) | ≈ 0.85 | 188 against 663 Mbit/s |

The ack-interval over-read (×8–29) lifts g·x above 1, which is why it was "load-bearing". **The law needs a time
target above T₀, not a rate.**

**Why each recorded candidate failed, against this form:**

| candidate (status) | same form? | what killed it | does this law escape it? |
|---|---|---|---|
| shipped law on the send-interval sampler (§23 Q2) | yes, with T* = (1 + q)·RTprop | T* < T₀ (table above) | conditionally: T* = m̂ + b̂₀ + Ŝ + q·ℓ̂ exceeds T₀ = m + b₀ + S iff b̂₀ > b₀ − q·ℓ̂. The margin is only q·ℓ (≈ 0.35–1.0 ms at c2), so b̂₀ is load-bearing (§3.5) |
| min-filtered loop time 1.1·D·Tmin (§24 O1, `INFLATES`) | yes, with T* = 1.1·Tmin | the allowance 0.1·Tmin < S, so the map is below 1 at every cap | yes: S is in T* (it cancels in the fixed point; only the floor error q·ℓ − (β − b₀) remains) |
| quinn's BBR rate (§24, `OVER-READ` ×3–20) | no: an exogenous rate | ack compression at c2 jitter | no rate is read |
| (b1)/(b2) quinn's window K (§34) | no: a window as the store | over-reads ×5–15 at c2/c8; Spearman(K, n) 0.90 | not read |
| (b4) receiver rate / μ_R / dispersion (§34, §36, §40) | no: rates | circular by identity / over-asks where the receiver is not the bottleneck / `NOT-YET` | not read |
| `bbr_rs` b̂ (§27, `CIRCULAR` at c2) | no: a rate | a delivered rate on a store-bound sender | not read |
| residence-time form g·L̂·(R̄ − brake) (§44 item 2, refuted) | partly: the reference is R̄ − brake = m + S | the brake undershoot (b₀ read as queue); an L̂ was still required; c1s spread 0.32–0.41 | the target adds b₀ + q·ℓ; no L̂; noise is §3.3's open item |
| oracle (1 + q)·ΣL(RTprop + S) (§36, §43) | yes, with an oracle L | an oracle L; a second crossing near 927 at c2 with an expired anchor (§42); q multiplies the anchor | L cancels; q·ℓ is not scaled by the anchor, so the expired c2 map has no upper crossing (§4.1) |
| G·ΣL(RTprop + S + τ), G 0.9, τ 5 ms (§43b, passed at c2) | yes, with an oracle L | G and τ unprovenanced; c1s τ window unsettled | G = 1 (derived); τ = b₀ + q(δ)·ℓ (cited q; b₀ blocking); see §3.3 for when G < 1 returns |
| W1 A(q): τ = b₀ + q·RTprop at G = 0.9 | yes | ≈ 8.5 % short at c2 | at G = 1 with q on ℓ, c2 lands on the knee, 192 at Bulk (§4.1). **This departs from W1, which held G < 1 required; see §3.3** |
| W1 A(c): τ = b₀ + c_B·q/0.1 | yes | c_B ≈ 2.7 ms has no source | no c_B |
| this derivation's first draft: max over legs of (m + b₀ + S) | yes, coupled | at c8, S₁ rises 8.6 → 118 ms with n; ask/n 1.9–2.9, crossing near 1 850 (`map27` history). On §54b the shipped anchors order the legs wrongly (`rtp0` medians 43–54 ms > `rtp1` 38 ms at W = 1 280) | the max structure is kept on non-inflating inputs (ℓ̂ + b̂₀⁺): the coupled F (§5.2) |

## 3. The law

### 3.1 Formula and provenance

```text
   n_{k+1}  =  clamp( n_k · R_k ,  floor ,  N·knee )

   R_k   =  Σᵢ wᵢ,ₖ · ( Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ,ₖ )  /  Σᵢ wᵢ,ₖ · T̂ᵢ,ₖ           Ŝᵢ,ₖ = T̂ᵢ,ₖ − T̂minᵢ,ₖ
   Fᵢ    =  max( m̃ᵢ + b̂₀ᵢ⁺ ,  maxⱼ ( ℓ̂ⱼ + b̂₀ⱼ⁺ ) )
   m̃ᵢ    =  min( m̂ᵢ ,  ℓ̂ᵢ + b̂₀ᵢ⁺ )
   b̂₀ᵢ⁺  =  max_{k ≤ K} ( T̂minᵢ,ₖ − m̂ᵢ,ₖ ) ,  K = 8 boot windows (n ≤ n₀ = 128)
```

| symbol | meaning | provenance |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ | median and minimum of leg i's app-echo loop samples in window k | measured. The echo is the loop the store holds a symbol for; the key is the placement stamp (§55 Erratum 1). Sampler: the oracle arm's (`[OCAP]`, archived), not on main |
| wᵢ,ₖ | leg i's sample count in window k (∝ delivered rate) | measured; dimensionless. The weighting is settled on the map (§5.2): it does not decide the attractor |
| m̂ᵢ | Copa's 10 s minimum of the echo series | shipped (`copa.rs:1000-1008`). It is E03's dishonest input; it enters only through m̃ |
| ℓ̂ᵢ | lifetime minimum of the echo series | measured; never expires. Under-read only on a route increase (§3.3) |
| m̃ᵢ | the anchor, its rise above ℓ̂ bounded by b̂₀⁺ | derived: at no queue the 10 s minimum exceeds the lifetime minimum only by the brake's spread, which b̂₀⁺ bounds. Any larger rise is own queue (E03) or a route change. Gauge: `anc=` bind fraction (§8) |
| b̂₀ᵢ⁺ | one-sided upper bound of the no-queue brake: the sample maximum over the first K windows | derived as one-sided (§3.5). K = 8 (2 s) is **declared**: the calibration length at n ≤ n₀. The bound's width (b̂₀⁺ − b₀) is the standing queue it adds; measured median 0.1–3.7 ms by cell (§3.5) |
| Fᵢ | target queue-free floor: the later of leg i's own floor and the slowest leg's lifetime floor | derived from ρ = 1 in-order delivery (§5.2). It is a max of continuous values and reads no dial. At N = 1, Fᵢ = m̃ᵢ + b̂₀ᵢ⁺ |
| q(δ) | CoDel setpoint (b + 1)/30 ∈ [0.05, 0.10] | cited, RFC 8289 §3.2; shipped `codel_setpoint_q`. **The only δ seat** |
| window T_w | 250 ms, tumbling | **declared**, as paper §6.1's oracle arm: inside its derived range only near Bulk, with the `wbind=` gauge. Every number here is a 250 ms statistic. The scale-free Tp50/q(δ) is not adopted (Q8): it depends on n, puts q in a second place, and is ambiguous at N = 2 |
| n₀ (start) | 128 | shipped `RWM_STORE_BOOT`. "A fit to one cell" (§11.2); no provenance (Q7) |
| empty window | a leg with no samples drops out of both sums; with no samples on any leg, R := 1 | a statistic of an empty set does not exist (§6.1's oracle boot rule) |
| floor | 10 | resource bound (shipped) |
| N·knee | 2 048 per live path | resource bound (shipped). Bind gauge `[SUMCAP] pin=` |

**No rate and no gain constant.** G = 1 is the update that makes T̄ = T* the fixed point. The declared constants
are K = 8, T_w = 250 ms and n₀ = 128, each with its open question.

### 3.2 Shape check (FORMULA-FIRST)

The sentence and the expression agree in shape:

- **Order in N.**
  - R is a weighted mean ratio, order 0 in N; symmetric legs give the same R at every N.
  - The fixed point n* = Σᵢ λᵢ·T*ᵢ + H is a sum over legs, order 1.
  - The clamp is linear in N. At N = 1 the sums have one term, and no path-count branch exists.
  - Compare the shipped predecessor's N²·a (paper §6.1, "The count multiplier").
- **Units.**
  - R is time ÷ time.
  - n stays in the store's own unit (source seqs) by construction.
  - There is no wire-against-source (1 + r) factor (W3 §2 item 8), because no rate is imported.
- **Monotonicity in δ.** R and n* are strictly increasing in q, and q is continuous and strictly monotone in
  b(δ) between the Realtime and Bulk anchors.
- **ρ.** No ρ input. Holes are funded implicitly (§3.4), whatever their residence, so §6.1's
  ε̂·bw·stall(δ, ρ) term is not needed. The store gate still requires `reliable` (F02); extending the gate to
  ρ < 1 is F02's question, and this formula needs no change for it.
- **NO-MODE-SWITCH.** One expression. δ enters only through q. There is no hint branch, no δ/ρ threshold, and no
  path-count branch. The clamp's bounds are resource bounds, carried with bind gauges.
- **Departure from the shipped sentence** ("BDP plus one recovery round of runway", the stale doc comment):
  - the new sentence is about time (target loop ÷ measured loop);
  - the shipped expression's (1 + q)·Σbw·RTprop is its special case λ̂·T* with T* = (1 + q)·RTprop.

### 3.3 Dynamics, noise and the anchor

**Saturated leg.** The brake rises one-for-one with the queue: β = Q/μ + b₀, and T̂ ≈ n/μ. So
n·R ≈ n − (Q − Q*)·n/(Σλ·T̂). The loop gain is n/(Σλ·T̂), which reads 1.03–1.11 at c2 and 1.12–1.33 at c1 on
the static maps (`map27` column "Little"). That gives one-window convergence with at most a 0.3 overshoot. It is
stable while the gain is < 2.

**Unsaturated leg.** Here β ≈ b₀ and R ≈ 1 + q·ℓ/T₀ ≈ 1.06 per window at Bulk, at every cell (the ratio is
scale-free). At T_w = 250 ms the climb from n₀ = 128 to n* takes:
- c2: ≈ 7 windows ≈ 1.7 s;
- c1s: ≈ 24 windows ≈ 6 s.

**This ramp is a stated cost.** Most c1s guard rows are shorter than 6 s (14 `[DIAG]` ticks), so they would run
in start-up. A faster ramp needs either a shorter window or a different n₀; both are open (Q7, Q8).

**Noise.** At gain ≈ 1, sd(ln n) ≈ the per-window coefficient of variation of (β − b₀)/T̂. §42 measured the
oracle form at T_w = 250 ms: 0.08 at c2, 0.11 at c1s. **Open:** the noise of this R (§10 Part B).

**The anchor: E03 is not fixed by the law's fixed point alone. It is bounded.**
- *What the shipped anchor reads* (§54b, `little54b.out`):
  - leg 1 at W ≤ 2 048: `rtp1` 35–46 ms against a boot value of 33–37;
  - leg 0: `rtp0` 34–60 ms at W = 1 280 and 237–319 ms at L2048, on a 10 ms leg.
  - That is E03: a minimum of a dwell-inclusive clock whose queue never drains.
- *Unbounded drift at the law's fixed point.* The queue floor target is (b̂₀⁺ − b₀) + q·ℓ. If the floor's
  fluctuation f within 10 s is below that, m̂ climbs by up to (b̂₀⁺ − b₀ + b₀ + q·ℓ − f) per filter period, and the
  target follows it. With f = 0 that is:

  | cell | per 10 s | per minute | what a long block shows |
  |---|---|---|---|
  | c2 | 2.6–3.3 ms | 16–20 ms/min | the loop doubles (T₀ ≈ 17 ms) in ≈ 1 min |
  | c8 leg 0 | 3.3–4.0 ms | 20–24 ms/min | the same in ≈ 40 s; F is unaffected (it is set by ℓ̂₁) |
  | c1d | 0.4–0.65 ms | 2.2–3.9 ms/min | T₀ ≈ 5–7 ms, so n ∝ T grows ×1.4–1.7 per minute and reaches the 4 096 clamp in ≈ 3–5 min |

  On a §53-length c1d block (5 GB rows, ≈ 60 s each) that is +2–4 ms of `rtp_us` and +40–70 % of the cap per
  row, re-pinning the longest rows. Linear is not bounded, so the formula bounds it.
- **The mechanism (closes the anchor question): m̃ = min(m̂, ℓ̂ + b̂₀⁺).**
  - The anchor cannot rise more than b̂₀⁺ above the lifetime floor.
  - The total queue floor is then at most 2·b̂₀⁺ − b₀ + q·ℓ, with zero drift after that, whatever f is.
  - On §54b it would read ℓ̂₀ + b̂₀⁺ ≈ 8.4–14.2 + 1–3 ms on leg 0, where `rtp0` reads 34–319.
  - The price is that a route increase Δm is under-read by Δm − b̂₀⁺, and the law collapses if that exceeds
    q·ℓ. L1 cells have no route change, so this is a **domain condition** with a gauge: the `anc=` bind
    fraction (m̂ − ℓ̂ > b̂₀⁺). Persistent binding with the queue floor at target is the signature of a route
    change.
- **The wire candidates the brief names** (`wire48b.py` on status §48b's `c2c8-diag-qcc` rows, 12 per arm, LATE
  medians, [boot]):

  | cell, leg | `bbr_rs` m̂ (`qrt`) | quinn `rtt.min()` (`qmin`) | Copa echo `rtp_us` | wire `wrtt` |
  |---|---|---|---|---|
  | c2 p0 (CAND = `bbr_rs`) | 7.8 [8.0] | 6.2 [8.0] | 189.8 [11.1] | 22.0 |
  | c8 p0 (CAND) | 8.2 [7.4] | 6.9 [7.1] | 80.8 [12.1] | 21.5 |
  | c8 p1 (CAND) | 39.6 [37.9] | 36.8 [37.8] | 121.2 [40.7] | 87.2 |

  Both read propagation while the echo anchor reads the store's queue:
  - `bbr_rs` m̂ is drained by ProbeRTT and is windowed, so it would also follow a route increase;
  - quinn's `rtt.min()` is a lifetime minimum, with the same route-change exposure as ℓ̂.

  Neither is a drop-in source, for three reasons:
  1. Both are on the wire clock. They exclude the echo's engine, quinn-buffer and report delays, so they need
     the echo-minus-wire no-queue offset. That is another boot calibration (the same structure as ℓ̂ + b̂₀⁺).
  2. The `[QCC]` gauge prints only under `bbr_rs`. Quinn's BBR (the default) exposes `rtt.min()` without a
     gauge, so no log here shows it.
  3. `bbr_rs` is not the default (K09/K10, §48b).

  **Decision:** m̃ ships with ℓ̂. `bbr_rs` m̂ plus the boot offset becomes the route-following replacement for ℓ̂
  if K10 promotes `bbr_rs`. The `anc=` gauge decides whether that is needed.

- **Departure from W1 (G < 1 required).**
  - W1's requirement came from (a) the oracle form with q·RTprop, and (b) c8's trap edge at G = 1 for a
    constant τ: τ < 3.08 ms.
  - This law's τ on leg 0 is b₀ + q·ℓ ≈ 2.85 + 0.4–1.2 ≈ 3.3–4.0 ms, above that edge at every δ.
  - The `map27` c8 expired map shows no upper crossing at G = 1, but both readings rest on the thin p27 c8 grid
    (two rows per cap, a lock-out row at 768). **c8 start-high is undecided**, and today's c8 start is the pin
    (Q1).
  - At c2 the allowance does not scale with the anchor, and the expired map is trap-free at G = 1.
  - G < 1 is incompatible with a q-sized allowance: a leg saturates only if G·(1 + q·ℓ/T₀) > 1. On every map,
    G = 0.9 gives no crossing at all (`map27-*.out`): it collapses.
  - What G = 1 cannot be shown to do from the logs is leave today's start-high state. §43's start-high hold ratios
    (1.006 / 1.059 / 1.005 at gain 1.1) transfer to R ≈ 0.98–1.03 at H = 1 536, using the c2 Little factor 1.05.
    That is marginal. **c2 does not collapse; it is not shown to leave the pin** (owner question Q1).
  - So at G = 1 neither c2 nor c8 is shown to leave today's start-high state.

### 3.4 The hole term H and the C02 coupling

**H is implicit.** Write n' = n·R(n). At the fixed point R = 1 whatever H is, and
dn'/dn = 1 + n·R'(n) = 1 − (1 − H')·n/(n − H).

H' is the share of an added slot that becomes a hole. It lowers the loop gain and moves neither the point nor the
sign of the slope.

**Measured H' per horizon** (`little54b.out`; residual ≈ holes):

| horizon (MAX_NACK_GAPS) | W 4 096 → 2 048 | W 2 048 → 1 280 | where old holes appear |
|---|---|---|---|
| 20 (B) | (1 601 − 118)/2 048 = **0.72** | (118 − 48)/768 = 0.09 | between transit 1.9 k (B1024: 0) and 2.5 k (B2048: 1 289) |
| 64 (L) | (35 − 17)/2 048 ≈ 0.01 | ≈ 0 | none up to transit 4.1 k |

**The coupling, as a condition.** Here "binds" means **old holes accumulate**, not that reports are truncated.
Reports are truncated at every arm (visibility 0.30 even at W = 1 280, §6) while H' ≈ 0.09. Old holes accumulate
when the 20 oldest runs are all long-lived holes. That happens once the open runs ε_run·c·transit exceed what
repair clears per horizon, so it sets in above a transit P_h ∝ G_h:
- P_h(20) ∈ (1.9 k, 2.5 k) at c8;
- predicted P_h(64) ≈ 3.2 × that, about 6–8 k. That is consistent with L2048, which shows no old holes at 4.1 k.

Above P_h, H' → 0.72 and the store law loses most of its authority. Its slots go to holes.

**P_h per cell, as a domain condition.** P_h scales as G_h/ε_run, where ε_run is the loss runs per symbol. From
the c8 measurement and each cell's calibrated loss and burst (`lib.sh`):

| cell | ε_run (runs per symbol) | P_h(20), estimated | law's transit* | margin |
|---|---|---|---|---|
| c8 | ≈ 4.5·10⁻³ (leg 0 2.7 %/8 + leg 1 4.1 %/4) | 1.9–2.5 k, **measured** | ≈ 550 | 3.5–4.5 × |
| c2 | ≈ 2.6·10⁻³ (2.6 %/10) | ≈ 3.3–4.3 k | ≈ 190 | ≈ 20 × |
| c1s, c1d | ≈ 2·10⁻⁴ (0.1 %/5) | ≈ 40–55 k | 370–654 | ≥ 60 × |

- **Lever B is not needed for the store's sake on these four cells only.** The condition transit* < P_h(G_h)
  fails on a cell whose loss-run rate × transit is about 5 × c8's.
- Gauges: old holes and `ripe_frac`.
- Lever B's residual at W = 1 280 (in-order 0.30 against 0.35 s, no overlap) is a recovery-latency gain, not a
  store one.

The store size and the gap horizon are one question, as the brief says. With a time law the answer is the
inequality transit* < P_h(G_h). The gauges (§8) are old holes and `[HOLD]` `ripe_frac` = `age_ripe/age_n` (p0
0.98 at B2048 against 0.13 at B640), both of which exist.

### 3.5 b₀: an asymmetric input, so a one-sided bound

**The asymmetry, from the fixed point.** At no own queue, the window minimum sits at m + b₀ (b₀ is the true
no-queue brake) and T̂ = T₀. The law's target floor is m + b̂₀ + q·ℓ. So:

```text
   no queue:    R − 1  =  ( b̂₀ − b₀ + q·ℓ ) / T₀             grows iff  b̂₀ > b₀ − q·ℓ
   fixed point: queue floor  =  b̂₀ − b₀ + q·ℓ                (ms of standing queue; the over-read adds 1:1)
```

- An **under-read** by more than q·ℓ (≈ 0.35–1.0 ms at c2, 0.12–0.26 ms at c1s) gives R < 1 at no queue. There is
  no restoring force, so the store decays to its floor: §23's collapse.
- An **over-read** by Δ costs Δ ms of standing queue and nothing else. At c2 1 ms is ≈ 10 symbols and +5 % of the
  loop.
- So b₀ needs a one-sided **upper** bound, not an unbiased estimate. The bound's width is a term of the formula:
  the fixed-point queue floor is (b̂₀⁺ − b₀) + q·ℓ.

**Candidates evaluated against each row's own no-queue brake** (`b0bound.py` → `b0bound.out`):
- Rows: the static caps at or below the BDP.
- The candidate reads only the row's first 8 windows. The law starts at n₀ = 128, at or below the BDP at all four
  cells.
- margin = candidate − b₀,row + q·ℓ at Realtime q = 0.05, the thinnest; collapse iff margin < 0.

| cell, leg (ledger) | b₀,row | median of first 8 (unbiased boot) | max of first 8 (b̂₀⁺) | max over the whole row |
|---|---|---|---|---|
| c2 p0 (p27, 6 rows) | 1.27–2.82 | **collapses 6/6** (margin −1.99) | 0/6; cost median 0.41, max 0.90 | 0/6; cost 1.74 / 2.50 |
| c8 p0 (p27, 4) | 1.87–4.29 | **4/4** (−2.26) | 0/4; cost 1.0 / 48.8 | 0/4; 1.8 / 48.8 |
| c8 p1 (p27, 4) | 3.49–4.94 | **3/4** (−0.77) | 0/4; cost 3.7 / 45.0 | 0/4; 6.3 / 45.0 |
| c1s p0 (p34, 3) | 0.09–0.22 | 0/3 (+0.09) | 0/3; cost 0.12 / 0.20 | 0/3; 0.20 / 0.21 |
| c1d p0 / p1 (p34, 3) | 0.13–0.36 | 1/3 on p0 | 0/3; cost 0.33 / 0.34 and 0.20 / 0.35 | 0/3 |
| c1s p0 (p27, 4) | 0.15–26.4 | 1/4 | 1/4 | 0/4 |

Notes on the table:
- The c1s p27 failure is `DEF192 s42 r1`, the §41b quinn-window-collapse row (brake 26 ms, not a no-queue row).
- With 8 samples, the nearest-rank 90th percentile is the maximum, so "window minimum quantile" and "sample
  maximum" coincide at K = 8.

**Reading.**
- The unbiased boot estimate collapses most c2 and c8 rows. Boot windows read a *lower* brake than the row,
  because the running anchor is still falling, so the asymmetry is not hypothetical.
- The boot sample maximum covers every no-queue row on both ledgers.
- Its cost has a heavy tail at c8 (one 45–49 ms window per leg, from windows with few samples on a starved leg).
  This is the bound's known weakness. It is visible in the `[SLAW]` b̂₀⁺ readout. A robust order statistic (the
  second-largest) would add a constant, so it is not proposed before Part B measures the tail.
- Against the measured working allowances: §43b's τ = 5 ms is b₀ + 2.7 ms at c2, and W1 A(c) has c_B ≈ 2.7 ms.
  b̂₀⁺ + q·ℓ puts c2 at b₀ + 0.8–1.4 ms (median), thinner than the measured 2.7 ms, but with zero collapses on the
  map.
- The spread estimator of revision 1 (max_k S_k − mean_k S_k, `b0est.out`) is kept only as the record of a failed
  candidate (it inflates at load).

**Part B falsifier B1 (one-sided, absolute):** on any row, b̂₀⁺ < that row's later no-queue median brake − q·ℓ̂,
or R < 1 in the boot windows.

## 4. Per-cell predictions (static maps, the inputs the law reads)

**Not registrable: §43 shows static-map reasoning fails in closed loop.** These are the maps' crossings, kept to
size the closed-loop arm (§10 Part B) and to read against §13's current-era maps. The Bulk costs at c1s (−10 to
−20 %), c1d (≈ −10 %) and c8 (≈ −3 to −5 % with the coupled F) would fail guard v2's goodput clauses. A ship
needs either U3 (accept the cost) or a larger allowance (U2).

The c2/c1 numbers below use the per-leg form with the ledger b₀. The coupled F is identical at N = 1. b̂₀⁺ adds its
cost: +0.4 ms at c2 moves n* by ≈ +4.

How these are computed:
- The fixed points are crossings of R(n) = 1 on the static-cap ledgers (`map27-*.out`, form `ratioL`, G = 1,
  H = 0; H = ε·λ·2·srtt changes no crossing by more than 2).
- Era:
  - p27 (2026-10-27, 100 MB rows, 2 rows per cap) for c2 and c8;
  - p34 (post-§32 receiver, 3 rows per cap, c1 windows truncated at 8 192 samples) for c1s and c1d.
- "Fresh" is the boot anchor. "Expired" is the row's lowest window minimum, a pessimistic expired filter.
- **None of these is a closed-loop result.** The closed-loop arm is §10.

### 4.1 c2 (the "must not collapse" test)

| q | fresh n* | expired n* | goodput at n* (p27 interpolated) | Tp50 at n* |
|---|---|---|---|---|
| 0.05 (Realtime) | 174 | 140 (and U81) | 0.92 / 0.80 | ≈ 18 ms |
| 0.0667 (Auto) | 180 | 148 (and U77) | 0.94 / 0.84 | ≈ 18.5 ms |
| 0.10 (Bulk) | **192** | 164 (and U69) | **0.984** / 0.89 | ≈ 19 ms |

Against MAIN, Tp50 is ≈ 208 ms at 600 MB (§43).

**No collapse with a fresh anchor:** R ≥ 1.01 at every cap 64–128. The expired map's lower unstable point (69–81)
sits below boot 128, and at boot the anchor is fresh.

**From today's pin** (c2 `pin` 0.80 on the guard's CAND rows): the expired map has R = 0.94 at 1 024 (Bulk), so it
descends. The §43 hold transfer reads 0.98–1.03, which is marginal (§3.3).

**Falsifiers:**
- the cap below 100 for more than 3 s after t = 2 s;
- goodput < 0.85 × main;
- the upper crossing: n not below 512 within 15 s of a start at 2 048.

### 4.2 c1s (today the clamp binds ≈ 0.7)

| q | fresh n* | expired n* | goodput at n* (p34; Gmax 832) |
|---|---|---|---|
| 0.05 | 370 | 561 | 0.79 / 0.86 |
| 0.0667 | 385 | 590 | 0.80 / 0.87 |
| 0.10 | 509 | 645 | 0.84 / 0.88 |

- **Bind fraction: 0.71 (guard CAND row) → predicted 0.** n* ≪ 2 048. Falsifier: `pin` > 0.1.
- **Cost:** at the power point, not the 0.95 knee (1 024). This is the §6.1 reading: on a receiver-bound cell
  the receiver's throughput keeps rising with queue (S 1.2 → 10.2 ms over 256 → 2 048), and a delay target
  declines the last 10–15 %. That is −10 to −20 % against a pinned main (≈ 826 Mbit/s): owner question Q3.
- **Era caveat:** p34's c1 windows are truncated (8 192 samples, start of window), with an unknown direction.

### 4.3 c1d (p34)

n* = 518 / 555 / 622 (fresh), 514 / 565 / 654 (expired). At Bulk with an expired anchor there is a touch at
1 020–1 025.

Goodput is 0.87–0.90 of Gmax 818. Today c1d is already interior (guard CAND row: `pin` 0.00, cap ≈ 1 230), so
"bind 0" tests nothing.

**Falsifiers:** a law cap outside 400–800, or goodput < 0.85 × main.

### 4.4 c8 (N = 2)

| form | fresh n* (Realtime / Auto / Bulk) | expired n* | leg 1 at n* |
|---|---|---|---|
| **coupled F (the law)** | **546 / 548 / 553** | 545 / 547 / 552 | ≈ 1 550 /s (p27 512) |
| per leg, delivered-share weights | 195 / 201 / 212 | 148 / 159 / 181 | 103–232 /s (starved) |
| per leg, equal weights | 204 / 210 / 220 | 164 / 187 / 203 | starved |

- p27 goodput at 512–768 is 0.95 of Gmax 100.9 (8 s rows). In-order delay is ≈ 3 × T̄ ≈ 3 × 47 ms.
- **Falsifiers:**
  - a law cap outside 400–750;
  - leg 1 < 1 200 /s;
  - Σ send rate < 0.95 × the W = 1 280 control.

## 5. c8: why §54b does not bracket the knee, and where it breaks (check 2)

**The 640 arms are not near the knee.** W = 1 280 is 6.8 × the base BDP: ≈ 190 symbols at 1 200 B (leg 0
125 kB, leg 1 100 kB; `place46_parse.py`). Its p0 loop of 96 ms is 9.6 × leg 0's propagation.

**What 1 280 does show** is that leg 1 is on its shoulder already:
- L r₁ is [1 974–2 009] at W = 4 096 and [1 866–1 946] at 1 280. The ranges do not overlap, and Σr does not
  overlap either (11 809–11 852 against 11 712–11 765). This is the status §54 send-rate fall.
- On B, r₁ is [1 897–1 979] against [1 839–1 913], which overlaps.

**Two shoulders.** Placement prices delivery time: E_i = (I/W)·s + (½ + ε)·wire (K05). So leg 1 gets traffic only
in proportion to how far leg 0's loop has risen toward leg 1's. Across the ledgers, leg 1's rate share against
leg 0's loop is:

| p0 loop | 34 ms | 45 | 73 | 96 | 152 | 198 | 354 |
|---|---|---|---|---|---|---|---|
| r₁ / ≈ 2 000 | 0.75 (p27 384) | 0.77 (p27 512) | 0.88 (p27 1 024) | 0.93 (B640) | 0.95 (B1024) | 0.96 (B2048) | 0.99 (L2048) |

The p27 columns predate the wire placement base (ADR-0073) and come from 8 s rows: shape only.

**The shoulders, predicted for the current era:**
- **K₁, the leg-1 shoulder:** r₁ falls below 0.8 × its W = 1 280 value once leg 0's loop is below T₀,₁ ≈ 46 ms
  (leg 1's no-queue loop, p27: Tp50 46–47 at 128–192). That is W ≈ Σμ·T₀,₁ ≈ 500–700.
- **K₂, the leg-0 shoulder:** leg 0 leaves link rate below W ≈ μ₀·(m₀ + b₀ + S₀) + H ≈ 9.8 × 18 + 10 ≈ 180–250
  (p27 c8: r₀ 9 258 at 192, 9 608 at 256).
- Between K₂ and K₁, goodput ≈ leg 0 alone. That is −13 to −17 %, at an in-order delay that keeps falling
  ∝ W.

**Do not fit to 640.** The slope §54b anchors is T̄ ≈ W/Σλ (109 ms per 1 280). The optimum is a δ choice on the
curve below.

### 5.1 The delay-against-goodput curve at c8 (shown, not picked)

| W | Σ send rate /s | T̄ (≈ W/Σλ) | in-order late mean | source |
|---|---|---|---|---|
| 4 096 | 11 826 | 343 ms | 0.93 s | L2048, measured |
| 2 048 | 11 790 | 171 ms | 0.49 s | L1024, measured |
| 1 280 | 11 745 | 107 ms | 0.30 s | L640, measured |
| 768 | 11 450–11 650 (predicted) | 60–70 ms | 0.18–0.25 s | §13 arm |
| 512 | 11 100–11 600 | 42–48 ms | 0.13–0.17 s | §13 arm (≈ the law's coupled point, 545–553) |
| 320 | 10 300–10 900 | 28–32 ms | 0.08–0.12 s | §13 arm |
| 224 | 9 700–10 100 | ≈ 22 ms | 0.06–0.08 s | §13 arm (≈ the rejected per-leg point) |
| 160 | ≤ 9 300 | ≈ 17 ms | — | §13 arm (below K₂) |

The δ dial moves the law along this curve only through q·ℓ: a few ms, ≈ 10–30 symbols. **At c8 the curve is
shaped by the placement coupling, not by δ.**

### 5.2 The leg target, settled on the map

**Why the per-leg target starves leg 1, whatever the weights.** An idle leg has a brake of ≈ b₀, so it already sits
at its own target. Its term in R is ≈ 1 + q·ℓ₁/T₁, a push of a few percent, whatever weight it gets. The store
grows; placement (K05, delivery-time price) sends the extra slots to leg 0; leg 0's brake rises past its own target;
and R → 1 at the fast-leg point.

This does not come from the weights. Equal weights land at 164–220, against 148–212 with delivered share
(`map27-p27.out`, `ratioE`). The attractor is set by the target.

**Attractors** (`map27-p27.out`, c8, G = 1, all q, fresh and expired):

| form | ask/n at 128 → 2 048 | stable crossings | start-high (2 048) |
|---|---|---|---|
| per leg, delivered share | 1.02–1.08 … 0.71–0.81 (1 024), 0.28–0.32 | one: 148–212 | returns (0.28–0.32) |
| per leg, equal weights | 1.02–1.07 … 0.79–0.87, 0.30–0.34 | one: 164–220 | returns |
| coupled F (ℓ̂-based) | 2.63–2.65 … 1.06–1.08 (512), 0.58–0.59 (768), 0.93–0.94 (1 024), 0.42–0.44 | one: 545–553 | returns |

There is no second attractor on the static map, so §43's BISTABLE is not reproduced there. The thin grid has two
rows per cap. The 768 row carries a leg-1 lock-out. 1 024 reads 0.93–0.94 under the coupled F, so a closed-loop
start-high remains Part B's readout.

**Why the max-over-legs form was discarded in revision 1, re-read.** It was discarded for its inputs, not its
structure:
- it took the maximum of m̂ⱼ + b₀ⱼ + Ŝⱼ, and Ŝ₁ inflates with load (8.6 → 118 ms);
- m̂₀ inflates under the queue the coupling itself creates (§54b `rtp0` 43–54 > `rtp1` 38).

F keeps the max, on lifetime floors ℓ̂ⱼ + b̂₀ⱼ⁺. That input neither inflates with load nor with the fast leg's own
queue.

**Why the coupling is derived, not a preference.**
- Under ρ = 1 the in-order frontier waits for the slowest leg. A faster leg's queue up to that leg's floor moves
  waiting from the reorder buffer into the fast leg's queue, and adds no in-order delay.
- The current placement law feeds the slow leg only once the fast leg's loop approaches it. A per-leg target
  therefore contradicts placement, and the store cannot fund the slow leg.
- The coupled F is continuous (a max of values), reads no dial, and reduces exactly at N = 1.
- What remains a preference is narrower (Q2): a consumer of first arrivals (ρ < 1, Realtime) pays ≈ +30 ms of
  fast-leg first-arrival delay for it. That is F02/ρ's question, not the ρ = 1 store's.

## 6. hn_n: 168 → 11 994 → 55 038 (check 4)

`[HOLD] hn_n` counts holes that a gap report named (`on_reported`, stamped only inside the report's 20 oldest
runs, and only before the C09 repair cap breaks the loop, `sender_phases.rs:806`) and that a later report showed
filled, with no retransmit ever emitted. From `little54b.out` (s42 r1 rows for `fed`, medians otherwise):

| arm | receiver `det` (all holes) | receiver `orig_n` (filled by the original) | sender p0 `fed` (holes it was told about) | p0 `hn_n` | hn₀ / det |
|---|---|---|---|---|---|
| B2048 | 200 k | 199 k | 7.0 k | 168 | 0.0008 |
| B1024 | 185 k | 183 k | 51 k | 11 994 | 0.065 |
| B640 | 181 k | 179 k | 63 k | 55 038 | 0.30 |
| L2048 | 273 k | 272 k | 174 k | 174 618 | 0.64 |
| L1024 | 183 k | 182 k | — | 98 678 | 0.54 |
| L640 | 175 k | 174 k | 94 k | 89 199 | 0.51 |

**Explained.**
- The receiver sees ≈ 180–270 k short-lived reorder holes on every arm. `orig_p50` is 49–106 ms, about the legs'
  loop difference. Receiver-side, the population hardly moves on B.
- hn_n is the share of them visible to the sender, so **hn_n ≈ det × visibility(G_h, W)**:
  - On B (20 runs), the oldest runs are occupied by long-lived holes: old holes at 4 096, and loss holes in
    repair at 2 048. Their count ≈ λ_run·T_rep falls with the loop, so visibility rises 0.0008 → 0.065 → 0.30
    as W shrinks. `age_ripe/age_n` on p0 falls 0.98 → 0.13 (B2048 → B640): holes are first reported already
    ripe when the horizon is full.
  - On L (64 runs), visibility is high and nearly flat (0.51–0.64). hn_n follows the reorder volume itself, which
    falls with W (det 273 k → 175 k). That is the opposite direction, reproduced by the same product.
- **It is a gauge of the horizon's headroom, not of a store regime.** Launch 1's witness reading (CAND 174 against
  BASE 10 848 at the default pool) is horizon visibility plus the filter, and holds only at that pool, as the
  §54 erratum says.

**Flagged remainder.**
- p0 holes should not arise from cross-leg reorder at B640. Leg 1's forward one-way is ≈ 35–50 ms longer than
  leg 0's (rtt₁ 146 against rtt₀ 96, with 20 against 5 ms returns), and netem is FIFO. So no later-placed seq can
  overtake an earlier p0 seq.
- Yet p0 hn_n = 55 k ≫ p0's real losses (≈ 2.7 % × 425 k ≈ 11.5 k).
- The attribution fallback (`last_source_path`) is not it: `source_path_map` is pruned only under EVICT, so the
  map holds every seq.
- **Open, and it touches a guard witness.** p0 holes come either from a per-seq delay inversion not visible in
  the medians (quinn GSO batching across connections, the receiver's per-path I/O batching) or from the counter's
  resolution rule.
  - Launch 1's c8 outcome witness (`hn_n` < 2 000 on p0, §54 Amendment 1 deviation 2) reads this counter.
  - Until the anomaly is explained, that witness's reading as "the filter removed spurious fires" rests on an
    unexplained counter.
  - One `RWM_SUCC_DUMP` row settles it; it is in §13. It does not affect the law.

## 7. δ's seat and the trade curve (check 5)

δ enters only through q(δ)·ℓ.

| cell | n* Realtime → Bulk (fresh) | goodput | delay |
|---|---|---|---|
| c2 | 174 → 192 | 0.92 → 0.984 | Tp50 18 → 19 ms |
| c1s | 370 → 509 | 0.80 → 0.84 | — |
| c1d | 518 → 622 | 0.87 → 0.89 | — |
| c8 | 546 → 553 (coupled F, both legs) | ≈ 0.95 (p27) | ≈ 3 × 47 ms in-order |

δ's lever stays small, as W1 U2 found. q's 5–10 % band is the whole of δ's authority. At c8 the coupled F (not
δ-keyed) sets the curve's position. Widening δ's authority means widening q's band (U2). Weighting the coupling by
a δ readout would be a second δ seat, so it is not proposed.

## 8. Gauges

| gauge | content | why |
|---|---|---|
| `[SUMCAP] pin=` / `floor=` (exists) | clamp bind fractions, whole row and last third | rule 18; predicted 0 / 0 at every cell |
| **`[SLAW]` (new)**, per window close | n, R, T_w; per leg: wᵢ, T̂, T̂min, m̂, ℓ̂, b̂₀, the error (β − b̂₀ − q·ℓ̂) | the law's own trace; routing witness: the `[DIAG] win=` denominator = `[SLAW] n` on ≥ 99 % of ticks |
| **`anc=` (new)**, in `[SLAW]` | per leg, the bind fraction of m̃ = ℓ̂ + b̂₀⁺, plus (m̂ − ℓ̂) median and last-third max | E03's bound. Binding is expected on a queued fast leg. Persistent binding with the floor at target is the route-change signature (the domain condition, §3.3) |
| **b̂₀⁺ (new)**, in `[SLAW]` | the bound and its K windows' sample counts | the bound's tail cost (§3.5) |
| old holes (exists, server `[RANK]`) and `ripe_frac` (exists, `[HOLD]`) | holes older than one readout; share of holes already ripe at first report | the horizon binding as §3.4 defines it (old holes accumulate) |
| **truncation share (new)**, counter in `sack_to_gaps` | share of gap reports truncated at `MAX_NACK_GAPS` | C02's clamp has no gauge (ledger C02 column 6). It measures visibility (§6), not binding |
| **C09 cap bind (new)** | share of reports whose loop broke on `cached_max_repairs` / budget | it gates which holes get stamped (§6). C09's own gauge, owed by the ledger |
| per-leg placement share (exists, P38ROW) | shares[i] | reads K₁ |
| quinn backlog B = Σ`dgq[hand − tx]` (exists) | authority gauge (paper §6.1) | must stay ≈ 0 at the law's point |

## 9. Engine sources per input

| input | source | status |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ, wᵢ,ₖ | a per-leg window sampler of `record_rtt_sample` | **missing on main.** The uncapped sampler exists on archived branches (oracle arm `run43`; `net/loopdiag.rs` on `archive/store-power`); port it |
| m̂ᵢ | Copa `min_rtt` (10 s) | present (E03; honest under §3.3's condition) |
| ℓ̂ᵢ | running minimum of the same echo series | missing (one field). quinn's `rtt.min()` is the wire clock, not the echo, so it is not a substitute |
| b̂₀ᵢ⁺ | the sampler's first K windows | missing (with the sampler); the map test passes (§3.5); closed loop is Part B |
| wire anchor (later) | `bbr_rs` `qrt` plus the boot echo−wire offset | present only on `bbr_rs` (K10) |
| q(δ) | `codel_setpoint_q` | present |
| T_w = 250 ms | the sampler's window close | missing (with the sampler) |
| n₀ = 128 | `RWM_STORE_BOOT` | present; unprovenanced (Q7) |
| a rate | — | **none needed.** E04 (`max_bw`) leaves the store law; its other readers (Copa's anchor floor, K01) are out of scope |

## 10. Diagnostic design (reported only; nothing scored)

### Part A: no build

Moved to §13 as a stand-alone registration.

### Part B: a gauge-only build

Port the window sampler, ℓ̂, m̃ and the boot b̂₀⁺, and compute R as a counterfactual readout
(`[SLAW]`, observation only). Gates:
- `GOLDEN_CAPTURE` unset;
- the shipped cap sequence is bit-identical with the gauge on.

Rows: §13's arms.

**Readouts:**
- R per window → its crossing W per cell, predicted c2 174–192, c8 545–553, c1s 370–509 (fresh; not registrable as values);
- b̂₀ at boot against the ledger values (2.28 / 2.85 / 3.91 / 0.29);
- the anchor creep m̂ − ℓ̂;
- sd(ln R) per window at T_w = 250 ms (the noise item of §3.3);
- the horizon bind and C09 bind fractions.

**Falsifiers:**
- the crossing outside ±30 % of prediction;
- **B1 (collapse condition):** on any row, b̂₀⁺ < (that row's later no-queue median brake) − q·ℓ̂, or R < 1 in
  the boot windows while n ≤ 128. It is one-sided and absolute;
- the `anc=` bind fraction > 0.5 on a leg whose queue floor is below target (a spurious bound);
- a second attractor: from a 15 s hold at 2 048, n not below 1 000 within 15 s of release (c2, c8);
- sd(ln R) at T_w = 250 ms > 0.15 (§42's oracle form read 0.08–0.11).

Part B decides whether an engine law can be registered. Nothing ships from either part.

## 11. Paper draft (text for `fec-arq-model.md` §6.1; not applied)

> ### 6.1 The outstanding store: a time law
>
> The store holds every sent symbol until its receipt is reported (Section 6.3). By Little's law, on each leg
> i the store holds λᵢ·Tᵢ symbols in transit plus the holes H awaiting repair; Tᵢ is the leg's echo loop
> from the placement stamp to the report. When the links are full, any store beyond Σλᵢ·T₀ᵢ + H is standing
> queue, and Tᵢ grows until it fills the store (status §54b: W/Σλ = 349 / 174 / 109 ms against a
> rate-weighted echo loop of 343 / 171 / 107 ms). The store's size therefore needs a target *time*, not a rate:
>
> ```text
>    n_{k+1}  =  clamp( n_k · Σᵢ wᵢ( Fᵢ + σ̂ᵢ⁺ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ T̂ᵢ ,  floor ,  N·knee )
>    Fᵢ = min(m̂ᵢ, ℓ̂ᵢ + b̂₀ᵢ⁺) + b̂₀ᵢ⁺ + ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺)) ,   b̂₀ᵢ⁺, σ̂ᵢ⁺ = boot maxima over K windows
>
> (Revision 3: σ̂⁺, the release-jitter cover, and the half floor gap, the forward-delay equalisation of symmetric
> legs. Both are measured on status §56's maps; see store-law §R3.)
> ```
>
> b₀ enters as a one-sided bound because an under-read collapses the store while an over-read costs only that many
> ms of queue. The anchor's rise above the lifetime floor is bounded by the same b̂₀⁺, which caps E03's creep. The
> maximum over legs is ρ = 1's in-order frontier: a faster leg may queue up to the slowest leg's floor at no
> in-order cost.
>
> Once per window, the store is scaled by the delivered-share-weighted ratio of each leg's target loop to its
> measured loop. The target is the no-queue loop (the 10 s echo minimum m̂, the window-against-filter minimum
> offset b̂₀, and the window spread Ŝ = T̂ − T̂min) plus CoDel's setpoint q(δ) on the leg's lifetime-minimum
> loop ℓ̂ [RFC8289]. Its fixed point is the time condition T̂min − m̂ − b̂₀ = q(δ)·ℓ̂: the standing queue floor
> is the setpoint. The store is then Σλᵢ·T*ᵢ + H, with the holes funded implicitly; H affects only the loop
> gain, 1 − (1 − H′)·n/(n − H), not the fixed point.
>
> The law reads no rate. The shipped (1 + q)·Σ bwᵢ·RTpropᵢ fed the store's own delivered rate is the map
> cap′ = cap·(1 + q)·RTprop/T̄, which contracts wherever (1 + q)·RTprop is below the no-queue loop
> (status §23: g ≈ 0.55–0.6 at c2). The defect was the time target, not the sampler. Every rate source on
> record (§23, §24, §27, §34, §36, §40, §43b) is therefore unnecessary.
>
> | symbol | provenance |
> |---|---|
> | T̂ᵢ, T̂minᵢ, wᵢ | measured: window median, minimum and sample count of the leg's echo loop |
> | m̂ᵢ | measured: Copa's 10 s echo minimum; honest while the leg's queue floor drains within the window (gauge: m̂ − ℓ̂) |
> | ℓ̂ᵢ | measured: lifetime echo minimum; scales the allowance only |
> | b̂₀ᵢ⁺ | measured: sample maximum of the no-queue brake over K boot windows (K = 8, declared) |
> | m̃ᵢ, Fᵢ | derived: the bounded anchor; the in-order floor |
> | q(δ) | cited: RFC 8289 §3.2, (b(δ) + 1)/30 |
> | window | 250 ms, declared (as the oracle arm), with the `wbind=` gauge |
> | n₀ | the shipped boot cap 128; no provenance (open) |
> | floor, knee | resource bounds; bind gauges `[SUMCAP] floor=` / `pin=` |
>
> Shape: order 0 in N for the update and order 1 for the fixed point; dimensionless in rates, so in the store's
> own unit; increasing and continuous in δ through q; no ρ input; one expression at every N.
>
> At N ≥ 2 the fixed point is a single scalar condition, and placement decides the per-leg split. With
> delivery-time placement (Section 5.7) a slow leg is fed only once the fast leg's loop approaches it. Under ρ = 1
> in-order delivery waits for the slowest leg anyway, so the floor term Fᵢ lets every leg queue up to the slowest
> leg's lifetime floor. A per-leg target would leave the slow leg idle at every weighting (static c8 map: 148–220
> against 545–553 with Fᵢ).

## 12. Open questions for the owner

- **Q1. G = 1 and start-high in closed loop.**
  - Every static map has one attractor with a returning start-high.
  - The c2 hold transfer (§43) reads R ≈ 0.98–1.03, and c8's coupled F reads 0.93–0.94 at 1 024.
  - Accept Part B's start-high readout as the test?
- **Q2 (narrowed). First-arrival consumers.** The ρ = 1 store couples to the slowest leg (derived, §5.2). Should
  the ρ < 1 gate (F02), when it exists, keep the coupling? Its consumers pay ≈ +30 ms of fast-leg first arrival.
- **Q3 (U3). The Bulk goodput cost** at c1s, c1d and c8 would fail guard v2 (§4): accept it, or widen the
  allowance (Q6).
- **Q6 (U2).** Is q's 5–10 % band δ's intended authority?
- **Q7. n₀ = 128** has no provenance, sets the c1s ramp (≈ 6 s at 250 ms) and is b̂₀⁺'s calibration condition.
- **Q8. The window:** 250 ms (declared, evaluated) or a scale-free derivation.

Closed in revision 2: b₀'s source (now a one-sided bound, §3.5); the anchor (m̃, a bound with a gauge, §3.3).

## 13. Registration: the store-pool sweep (reported only, no build; copyable into status)

**Why.**
- §54b measured three pools at 6.8–21.8 × the c8 BDP and does not bracket the knee.
- The store-law derivation (`docs/derivations/store-law.md`) predicts two shoulders at c8 and a single-leg knee at
  c2. Its inputs are static maps of an older era (p27: 100 MB, 2 rows per cap; p34 for c1).
- This block measures current-era static maps with the existing knob. It decides nothing by itself. It feeds the
  store law's closed-loop registration (Part B), the δ-authority question U2, and the lever-B guard.
- It is independent of the law's open blockers, because no law is built.

**Tree, knob and suites.**
- **Tree:** current main. The SHA is pinned by amendment after the VM pre-flight.
- **Knob:** `RWM_STORE_PATH_POOL=<pool>` (`gates.rs:323`).
  - The realised ceiling is `max(N·pool, floor)` (`store_cap.rs:90`). The boot cap is `min(128, store_max)`.
  - Every arm's W is ≥ 160 > 128, so neither bound inverts against the boot cap.
  - W binds only while the shipped ask exceeds it; see Liveness.
  - `RWM_STORE=<n>` is not used: it switches the law off and moves the seat.
  - The battery scrubs `-u RWM_STORE_PATH_POOL` (as plan 54b).
- **Engine suites** are skipped only if `raptorpath/src` and `raptorpath/tests` are byte-identical to the tree that
  passed §54's suites (`cand54` `7fd92dd5` / its merge on main). This is checked by a reuse manifest: a per-file
  sha256 list, diffed on the VM.
  - Any difference runs the full suites as in §54.
  - The tools tests (`tools/l1` python and bash tests) and a rehearsal (`LD_REHEARSE=1`) always run, because the
    sweep is a new harness path.
- **`RWM_SUCC_DUMP`** exists on main (default `RWM_SUCC_DUMP_MAX` = 200 000 records). The dump is written under
  the run tree on `/dev/shm`. The pre-flight checks that `/dev/shm` has ≥ 1 GB free, and the rehearsal records the
  dump's actual size.

**Arms** (Bulk, `--window-reliable`, `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300`):

| cell, size | pools (W) | n per pool | rows |
|---|---|---|---|
| c8-100, 600 MB | **640 (W 1 280, control = §54b B640)**, 384 (768), 256 (512), 160 (320), 112 (224), 80 (160) | 4 | 24 |
| c2-100, 600 MB | **2 048 (control, the shipped pool)**, 384, 256, 192, 160, 128 | 3 | 18 |
| c1s-400, 5 GB | **2 048 (control)** and 512: n = 6; 1 024, 768 and 384: n = 3 | 6 / 3 | 21 |
| c8-100, 600 MB, `RWM_SUCC_DUMP=1` | 640 | 1 | 1 (the hn_n anomaly, §6) |

- **Total:** 64 rows.
- **Order:** cells in blocks, c8 → c2 → c1s. Within a block, a Latin-square rotation of pools per replicate, seeds
  42 / 7 alternating. The c1s n = 6 arms take the extra replicates at the block's end, rotated.
- **Smoke:** one row per cell at its smallest pool.
- **Length:** about 24 + 18 + 22 + 1 min of rows. With the tools tests, the rehearsal and overheads that is about
  1.5 h. `LD_HARD` = launch + 3 h.

**Liveness and row treatment.**
- **Pool echo.** `[GATES]` and `pool_per_path` echo the requested pool on both ends. If they do not, the row is
  VOID and is replaced once.
- **Binding.**
  - **Control arms are exempt.** They are the shipped behaviour: c1s binds ≈ 0.7 at 2 048, and c2 is not pinned
    at 2 048.
  - A **non-control** row binds if its LATE-window bind fraction is ≥ 0.95. The LATE window is sending ticks at
    t ≥ 15 s (c1s: t ≥ 5 s).
  - An unbound non-control row is **not replaced**, since a rerun would repeat the miss. It is reported at its
    realised cap: the LATE-window mean of `win=…/b`. It is plotted at that W, and labelled `UNBOUND`.
- **DNF** (as 54b): goodput counts as 0, and every other readout comes from the row's `[DIAG]` lines.
- **Flags** (reported, never voiding):
  - **c1s quinn-window collapse:** the share of windows with `qcwnd` < ¼ of the row median exceeds 0.1 (§43b's
    duration measure).
    - Flagged rows are reported in every table.
    - They are **excluded from F8**, and the count is stated.
  - **c8:** the lock-out classifier (§3.9).
- **The SUCC_DUMP row** must write a dump with ≥ 100 000 records. Otherwise it is re-run once.

**Readouts per row** (LATE window; `store-law/little54b.py` and `pool54_score.py` definitions):
- per leg: tx rate rᵢ, app-echo rtt and wire rtt, `rtp_us` against its boot value, q₀ (`dgq hand − tx`), and the
  placement share;
- Σ send rate, goodput, tail, H0;
- in-order delay, late mean and p95;
- old holes (server `[RANK]`), `[HOLD]` `hn_n`, `fed` and `ripe_frac` per path, `[SUCC]` `det`;
- the Little residual (W − Σrᵢ·rttᵢ)/W;
- duplicates, retransmits/GB, client and server CPU/GB;
- the realised cap and the bind fraction.

**Readout per cell (new): the standing-queue time at the knee.**
- **Definitions:**
  - T̄(W) = the rate-weighted LATE echo loop Σrᵢ·rttᵢ/Σrᵢ.
  - T₀ = the same statistic over the first 1 s of ticks of the cell's smallest-W rows (no queue).
  - ℓ = the rate-weighted boot `rtp_us`.
- **Readout:** from the arm medians, interpolated in W, the W where goodput first reaches 0.95 and 0.98 of the
  control. At those two points, report T̄ − T₀ in ms and as a multiple of ℓ.
- **Why.** The derived law puts the standing queue at (b̂₀⁺ − b₀) + q(δ)·ℓ.
  - The estimator slack (0.4–3.7 ms) already exceeds δ's whole term (q·ℓ ≈ 0.12–4 ms).
  - The static-map points move only 174 → 192 (c2) and 370 → 509 (c1s) from Realtime to Bulk.
  - So δ has almost no authority. That is U2, which goes to the owner with this readout after Part A.

**Predictions** (bands; these are static-cap measurements, so registrable here).

c8, every control band from the §54b **B640** rows (`docs/l1-raw/s54/rep54b-*`, main's 20-run horizon;
`little54b.out`):

| c8 W | 1 280 (control) | 768 | 512 | 320 | 224 | 160 |
|---|---|---|---|---|---|---|
| Σ send rate /s | 11 629–11 732 (B640) | 11 400–11 650 | 11 050–11 600 | 10 300–10 900 | 9 700–10 100 | ≤ 9 300 |
| r₀ /s | 9 776–9 840 | ≥ 9 750 | ≥ 9 700 | ≥ 9 600 | 9 400–9 800 | ≤ 9 300 |
| r₁ /s | 1 839–1 913 | 1 650–1 900 | 1 400–1 800 | ≤ 1 000 | ≤ 400 | ≤ 300 |
| T̄ ms (≈ W/Σr) | 103–107 | 60–70 | 42–48 | 28–32 | ≈ 22 | ≈ 17 |
| rtt₀ / rtt₁ ms | 96 / 138–161 | — | — | — | — | — |
| goodput Mbit/s | 106.5–107.6 | — | — | — | — | — |
| in-order late mean s | 0.34–0.39 | 0.18–0.26 | 0.13–0.18 | 0.08–0.12 | 0.06–0.08 | — |
| old holes | 0 | 0 | 0 | 0 | 0 | 0 |

The c8 standing-queue readout is predicted to sit at W ≈ 500–700 (leg-1 shoulder), with T̄ − T₀ ≈ 25–35 ms
(the coupled floor, §5.2).

- **c2:** goodput ≥ 0.97 × control at W ≥ 192; 0.88–0.95 at 160; 0.72–0.82 at 128. T̄ ≈ W/9.6 k.
- **c1s:** goodput / control ≈ 0.94 (1 024), 0.92 (768), 0.84 (512), 0.80 (384), ± 0.05 (p34 era). Flagged rows
  are excluded.
- **Anchor:** `rtp_us` on c8 p0 falls toward boot as W shrinks, ≤ boot + 10 ms at W ≤ 320. On leg 1 it stays within
  boot + 8 ms at every W.

**Re-basing (control drift).**
- The c8 control is compared with B640 by **medians**, not by six-row min–max (that check nearly always fires,
  §54 launch 2).
- A readout is re-based iff |control median − B640 median| > tol, where tol = the B640 rows' full range for that
  readout, floored at 1 % of the B640 median. For example: Σ send rate tol = 103 /s; r₁ tol = 74 /s; in-order
  late tol = 0.05 s.
- If a readout re-bases, that readout's bands at the other W shift by the same median offset, and the shift is
  reported.
- The c2 and c1s controls have no prior bands. They set their cell's reference directly.

**Falsifiers (reported; each changes the derivation, not a ship decision):**

| # | what fires it | what it means |
|---|---|---|
| F1 | c8 r₁ at W = 320 ≥ 0.8 × r₁ at 1 280 | leg 1 is fed without a fast-leg queue; the coupled F is not needed at c8 |
| F2 | c8 Σ rate at 768 < 0.97 × control | the knee is above 768 |
| F3 | c8 r₀ < 0.97 × 9 800 at W = 224 | the leg-0 shoulder is above prediction |
| F4 | c8 `rtp0` at W ≤ 320 > boot + 10 ms | E03's queue persists even at small W; the bound m̃ is load-bearing at c8 |
| F5 | \|W − Σrᵢ·rttᵢ − old holes\| > 0.1·W at any bound W ≥ 320 | Little fails |
| F6 | old holes > 50 at any W ≤ 1 280 | P_h is lower than the derivation says |
| F7 | c2 goodput median at 192 < 0.95 × control | the c2 knee moved up in this era |
| F8 | c1s goodput median at 512 ≥ 0.95 × control, over unflagged rows of n = 6 (excluded count stated) | the c1s cost in the derivation is overstated |
| F9 | the SUCC_DUMP row shows p0 holes overtaken by later-placed p1 seqs | the hn_n anomaly is a real delay inversion, not a counter rule |
| F10 | the standing-queue multiple (T̄ − T₀)/ℓ at the 0.95 point differs across c1s, c2 and c8 by more than ×3 (max/min) | **the q·ℓ form is wrong**, not just its band: the knee's queue does not scale with the lifetime loop |

**Decision rule.** None; reported only. The maps replace p27/p34 as the inputs of Part B. F9's reading goes to the
guard-witness note (§6). F10 and the standing-queue readout go to the owner with U2.

### 13.1 Carried to Part B (open; nothing solved here)

- **K = 8 and n₀ = 128 have no provenance.** "Declared constant" is not an allowed class (FORMULA-FIRST). Each needs
  a derivation or a resource bound stated outside the law.
- **b̂₀⁺'s tail.** One bad boot window (45–49 ms at c8) becomes a standing queue for the transfer's lifetime. Gauge
  the K windows' spread per row (max − median, sample counts), and derive a bound that a single window cannot set.
- **The max over lifetime floors steps when a leg joins or dies.** An idle or dead slow leg sets the fast leg's
  target indefinitely, because ℓ̂ never expires. Check the shape in N (legs joining, symmetric and asymmetric), and
  the leg-death case: the term must leave F continuously when the leg leaves the live set.
- **ρ.** The coupling is justified at ρ = 1 only, and "a future ρ < 1 gate" is mode-switch wording. The coupling
  needs a form continuous in ρ, or an explicitly stated domain (the store gate exists only at ρ = 1, F02).
