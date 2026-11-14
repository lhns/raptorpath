# P_fec, continuous and exactly 0 at r = 0 (W22 / ledger R03)

Derivation only: no code changes. Mandate: `docs/status.md` "Owner decisions
(2026-11-14)" item 2. Numbers: `pfec-continuous/pfec_check.py` →
`pfec_check.out`. Sections of that file are cited as **out§N**; plain §N is
this doc, and paper sections are named "paper §". Base: `db012f16`; the branch merged main at `72c42721` (commit `ab9c6246`).

**Done with Fable (advisor), per the owner rule.**
1. *Before the candidate set:* Fable set the event (per-loss, size-biased,
   not the window event). It also proposed the debt-accrual dither in place of
   a smoothing constant and the exact (ε, σ²) → GE inversion, and asked that
   item 4 be a proof rather than a band.
2. *After the numerics and the first commit:* Fable flagged five things, all
   adopted below:
   - the cartoon's fractional A*, which gives the real-W extension in §1;
   - a possible r_sat staircase (checked in out§19: none beyond R04's own
     0.005 grid);
   - the gate's relative ±2 % reading (out§17);
   - the dropped σ² = 0 point (out§16);
   - the missing premises of the L1 proof.

   It also fixed three number slips.
3. *Before the final commit:* Fable corrected my claim that "a guard cannot
   execute the changed law". `r_saturation` runs on every evaluation whenever
   the cap is on, so the law executes and only its output is annihilated; this
   gave the CPU prediction in §4. It also had me:
   - check the generation and coded-only window caps (> 200);
   - recount the cost;
   - soften the claims I had not read (gate `SLOPE` entry, goldens, JS σ²,
     `RWM_DELTA`) and list them in §6;
   - add the ⌊A*⌋/⌈A*⌉ vs ceil divergence to §5.
4. *Main-session Fable review of c397d297* ("strong candidate, not yet a
   result"). It changed §7 and the scope of §0:
   - it required an arbiter and a decoder bound at A*. That gave the oracle
     port (C6 within 1 %) and the sliding decode, which narrowed the
     recommendation to the block callers;
   - it required the measured L1 W and an evaluation scheme. That gave
     W = 201 from `[SPAN] wsize=` and the once-per-snapshot plus age-cache
     scheme in §2;
   - it required the root-cause quotes, the r\* finding and the item-3
     numbers. ρ-inert is no longer total; T_cut is unchanged by identity;
   - it required the slope bound and the recorded change of criterion (§7 R4).

   My own advisor rounds for this review added several checks: the
   `temporal_oracle` port and its self-check, the sliding-sim seeds and
   tolerances, the input sweep for the measured band, `generation=false` on
   L1, the `rce=` baseline, and the dial-sweep requirement for the gate.
5. *Fable, before the round-4 final commit* (after `314e0676`). It rejected
   the "block callers" scope, because every executing caller decodes a
   sliding window, the wasm sim included. That gave one status at every
   caller and the owner's choice (a)/(b) in §0. It also:
   - had the r\* tail checked with the sliding decoder (`pfec_tail.py`). That
     check overturned the "52× to 10³×" engine claim, now a block-law figure
     only;
   - had the r_sat cache declared as a second resource bound, not "meets
     `RATE_CADENCE_MAX_US`";
   - fixed the R1d table, the 20–53 % range and the "later ledgers" sentence,
     and removed the stale sentences in §1 and §2.
6. *Main-session Fable review of `82acbd00`* (the owner chose path (b); C6
   does not ship). It required:
   - the bracket wording, with one table against the sliding decode by caller
     window, including the engine pin point;
   - the oracle as executed code, with a harness search and a design;
   - the r\* item restated as a paper-text erratum;
   - §0 to say plainly that no form is recommended.

   My advisor rounds for it added several checks:
   - the bound is conditional on its spacing and chain premises (the
     `TaperBudget` front-loading, and the loss-free-placement rerun T2);
   - the tail sample cannot resolve the Auto target, and is only a lower bound
     on the miss;
   - main's `7c5872f5` erratum carries the same over-reading;
   - the harness candidates were checked per criterion, not by their headers;
   - the verdict column and a sweep of the retracted phrases.

   This gave §8 and the rewritten §0.

## 0. Result

Neither the shipped form nor the paper's form is right, for one shared reason:
**§4.2 computes the wrong event.** The paper defines P_fec as "probability a
lost symbol is FEC-recovered" (`fec-arq-model.md:205`), and every use of it
reads it that way: R = ε·(1 − P_fec) (§3.5 `:573–576`, §5.6 `:1723`). But §4.2
(`:693–699`) derives the **window** event P(surviving repairs ≥ K), which
includes K = 0, and then approximates it by a two-moment normal. On the
correct per-loss event (the size-biased or Palm event), the r = 0 pin is a fact
about support: C ≤ N_R = 0 < K. No normal form of C − K can carry that. The
candidate C6 keeps the paper's block-window assumptions and replaces the normal
with the exact counts:

```text
P_fec(r) = Σ_{k=1..W} [k·π_W(k)/(W·ε)] · [ (1−f)·S(k; n₀) + f·S(k; n₀+1) ],   n₀ = ⌊rW⌋,  f = rW − n₀,  S(k; n) = P(Bin(n, 1−ε) ≥ k)
```

π_W is the GE window loss-count pmf, with (p, q) recovered from (ε, σ²):
p + q = 2/(σ² + 1) and p = ε·(p + q).

- It is 0 at r = 0 exactly (no branch), continuous and monotone in r, and
  continuous in ε and σ².
- It tends to the Φ form as O(W^−½). That limit is not reached at any
  operating window.
- It passes every assertion of the engine pin.
- It matches the repo's executed **block** oracle (`temporal_oracle.rs:1476`,
  ported) within 1 %, where the shipped form is off by 20–53 % (§7 R1).
**No form is recommended. The owner's path is (b): derive the sliding
per-loss law first. C6 does not ship.** Until that law exists, the W22 step
stays in R, which decision 3's option (ii) reads.

- Every caller that executes decodes a **sliding** window: the engine (A\* 4–7
  on the evicting seat, W 201 on L1) and the wasm sim (`send_repair` over the
  whole window, `raptorpath-wasm/src/lib.rs:899–900`). C6 is the exact law of
  a **block** window only.
- **The bracket we have is one-sided** (§8.3):
  - The Python sliding decode has no deadline and decodes by generic rank, so
    it is an **upper bound** on per-loss FEC recovery. That holds under its
    model premises: uniform repair spacing, one GE chain, and Δ ≤ 8. The
    engine's `TaperBudget` front-loads repairs after each ack, and its Δ
    reaches 64 (§8.1).
  - No lower bound is derived.
  - **Above the bound means refuted:**
    - the shipped form and plain Φ above r_c at every A\* 3–7 cell (20 of 20);
    - Φ below r_c at every window;
    - C6 and C5 below r_c at W 201, c3.

    C6 sits at the bound at one A\* cell (A\* 3, r 0.2).
  - **Below the bound means not ordered:** C6 and C5 elsewhere, and every
    form in the W 201 body above r_c.
  - In the W 201 tail the sliding miss is only a **lower** bound on the miss,
    and its sample cannot resolve the target, so no form is refuted there.
- **Engine pin point** (ε 0.048, r 0.34, A\* 4, σ² 3.76):
  - the bound gives P ≤ 0.416 at Δ 2 and ≤ 0.458 at Δ 8; repairs on a
    loss-free second path give ≤ 0.399;
  - so R ≥ 2.6 %. The shipped form gives P ≈ 0.90, i.e. R = 0.49 %.
  - The shipped form is **outside the bracket on the budget-starving side**,
    under the bound's premises.
  - The one executed datum (decode share 0.71–0.84, one L0 rep) sits on the
    same side as the shipped form. Only the §8.2 oracle can say whether the
    datum is P_fec, or whether a premise of the bound fails.
- **Next deliverable:** the test-only Rust oracle of §8.2. It runs the
  engine's real `RlcWindowEncoder`/`RlcWindowDecoder`, the engine's
  `TaperBudget` emission and trailing span, a GE channel, no ARQ, and a
  deadline D(δ). It reports P(D) and P(∞) at A\* 3–7 and at W 201, against
  per-form predictions frozen before the run. The sliding law is derived
  against it, with Fable.
- On c1s, c1d, c2 and c8, r\* stays bit-identical 0 under any form (proof in
  §4). Any future law executes there through `r_saturation`, so it must meet
  the evaluation-scheme requirement of §8.4.
- **Paper-text erratum, not a rate-law finding:** paper §4.2 (`:693–704`)
  derives the window event, which includes K = 0, instead of the per-loss
  event that `:205` defines.
  - On the block law, the per-loss r\* is 31–47 % above §4.7's.
  - For the executed sliding decoder this under-provisioning is **not
    established**. The sliding check at c2/c3 W 201 cannot resolve the Auto
    target: 0/4 101 and 2/7 472 misses, 95 % upper limits 9e-4 against
    targets 2–4e-4. Its miss is also only a lower bound.
  - Realtime is unresolved and out of reach by counting.

## 1. What P_fec must mean

**Coding model as executed.**
- Engine: sliding-window RLC (`FecBackend::Rlc`). Each repair combines the
  last A* source symbols, with A* = clamp(⌈rate·D⌉, 1, encoder.window_size())
  (`net/emit_source.rs:578–580`). The encoder window cap is mode-dependent
  (`net/sender_policy.rs:443–464`):
  - systematic: `MAX_WINDOW_SIZE` = 200 (`net/mod.rs:205`);
  - coded-only: 640 by default, up to 4096;
  - generation: G·(M+1), at least 200.

  The L1 harness passes `--window-generation-coding` (`tools/l1/perf_rwm_c.sh:71`),
  but the L1 rows log `generation=false` in their gate banner (e.g.
  `docs/l1-raw/anchor-diag/diag-ad/c1s-400-MAIN-s42-r1-a1-c.log`), so the
  systematic cap applies. **Measured W:** the `[SPAN] wsize=` gauge (the
  `encoder.window_size()` that `cadenced_repair_rate` passes,
  `emit_source.rs:522–528`, `:662`) reads p50 **201**, max 213–247 at c1s,
  c1d, c2 and c8 (anchor-diag MAIN rows). A* there is 201–263, i.e.
  W-clamped. The grid's W 200 is therefore the L1 window.
  - The later ledgers I sampled (loop-diag, store-law-n1, thread-sk) were all
    c1d rows, and they read the same.
  - The §57b archive on main `7c5872f5` (`docs/l1-raw/s57/rep57b-diag-ld.tgz`,
    c1d P192 client rows) reads p50 201, max 260–264.
- Span geometry (`emit_source.rs:784–806`): a repair codes over
  [max(ws, end − A*), end) with end = newest + 1 − Δ and
  Δ = clamp(⌈rate·J⌉, 1, 64). Some repair slots carry a source **copy** of a
  P_lost-branch retransmit instead of a coded symbol (`use_retransmit`,
  `:760–783`). That targeted path is not in any model here.
- Repairs come from deterministic debt accrual. The engine banks r per source
  symbol (`TaperBudget`, `control/fec_rate.rs:678–703`); the sim does the same
  (`raptorpath-wasm/src/lib.rs:1348`, emitted at `debt ≥ 1`, `:1279`).
- The sim's window slides to its slider W (`:1304`, golden W = 64).

**The event.** Take a tagged source symbol that is lost. It is FEC-recovered
iff, over the W source slots that protect it, the surviving repairs C reach
the window's loss count K (which includes the tagged loss). Losses are
size-biased: a window with K losses holds K tagged losses. So

```text
P_fec = P(recovered | lost) = E[K·1{C ≥ K}] / E[K],     E[K] = W·ε
```

- This is the block-equivalent reading the paper uses (§4.2, §4.7).
- A sliding-window decoder can also use overlapping spans. That makes this a
  model of the decoder, not a bound on it. §7 R1 and R3 measure the gap by
  simulation: up to 0.15 in P, and two orders of magnitude in the tail miss at
  W 201.

**Counts.**
- Repair count: with debt accrual, the number of repairs falling in a span of
  W sources is N = ⌊rW + U⌋, where U is the accumulator phase at the span's
  start. Taking U ~ U[0,1) gives N = n₀ w.p. 1 − f and n₀ + 1 w.p. f, so
  E[N] = rW exactly.
- Repair survival: C | N ~ Bin(N, 1−ε). Repair erasure is iid, which is
  §4.2's own assumption (Var C = rWε(1−ε), `:695`).
- Losses: K is the source-loss count of the stationary two-state GE chain,
  with loss iff Bad (netem `gemodel … 1-h 100% 1-k 0%`, `tools/l1/test_topo.sh:416`).
  §2.4's identity σ² = 1 + 2(1−p−q)/(p+q) (`raptorpath-math/src/lib.rs:82–92`)
  inverts exactly: p + q = 2/(σ² + 1), p = ε(p + q), q = (1 − ε)(p + q).
  The inversion gives a valid GE (p, q ≤ 1) iff σ² ≥ |1 − 2ε|.
  - Every engine and sim estimator path floors σ² at 1, the iid case
    (`lib.rs:91`, `fec_rate.rs:772`, wasm `:1659–1667`).
  - The raw wasm exports (`p_fec_normal` :83, `shed_budget_residual` :217)
    take σ² from JS unfloored. A code change must project σ² to max(σ², 1)
    there. That projection is continuous; it is the `burst_variance_factor`
    floor, not a new threshold.
  - The task's σ² = 0 point is therefore evaluated for C0 and C1 only
    (out§16). At r = 0 both give P = 0, because z = −∞ when the variance is 0.
    C0 still steps by 0.500 at r_c.

**Required properties.**
- P(0) = 0 exactly.
- Continuous and non-decreasing in r.
- Continuous in ε and σ².
- Equal to the Φ value where the CLT holds (W·ε ≫ σ²).
- ε → 1 ⇒ 0.
- ε → 0 ⇒ the probability that the window holds enough repairs for the tagged
  burst. That is min(1, rW) for iid, not Φ's 1 at any r > 0.
- More burst (σ²) ⇒ lower P.

**Real-valued W.** The engine's windows are integers: A* is a ceil, and
`window_size()` is a usize. The cartoon's A* is not: it is
`span_width_a_star` = clamp(rate·D, 1, W) (wasm `:186–188`), which is
fractional and continuous in δ. Rounding it would put a step on the δ dial, so
the same phase argument extends C6. A span of real length W at uniform phase
covers m₀ = ⌊W⌋ slots w.p. 1 − g and m₀ + 1 w.p. g, where g = W − m₀:

```text
P_fec(r; W) = (1−g)·P_fec(r; m₀) + g·P_fec(r; m₀+1)
```

This equals C6 at integer W, and it is continuous across integers: the jump
at W = 3, 4, 5, 64 ± 1e-9 is ≤ 4.9e-10 (out§18). Monotonicity in W is checked
in out§11.

## 2. Candidates

Each candidate is graded against the event in §1. Numbers are at the pin
(ε 0.048, W 5, σ² 3) unless noted (out§1–2).

| # | form | P(0) | verdict |
|---|---|---|---|
| C0 | shipped: Φ(·), cut to 0 when r(1−ε) ≤ ε (`lib.rs:885–893`) | 0 | **reject**: a 0 → ½ step at r = ε/(1−ε) (out§12: 0.500 jump at h = 1e-6). A threshold selects a branch |
| C1 | paper's Φ form (`fec-arq-model.md:699`) | **0.386** | **reject**: fails the pin (R(0) = 0.0295 ≠ ε) |
| C2 | per-loss event, continuity-corrected normal: Φ((rW(1−ε) − (W−1)ε − ½)/√(rWε(1−ε) + (W−1)ε(1−ε)σ²)) | **0.175** | **reject**: any continuous two-moment law of C − K puts mass on C ≥ K when C ≡ 0 |
| C3 | the paper's window event computed exactly: P(C ≥ K) | **0.864** = P(K = 0) | **reject**: wrong event. It counts loss-free windows as "recovered" |
| C4 | product hybrid P(C ≥ 1)·Φ(·) | 0 | **reject**, not computed: the factorisation assumes independence with no basis, and the Φ factor keeps C1's error |
| C5 | per-loss event, exact joint GE DP (repair erasures GE-correlated) | 0 | the oracle, not the law: it needs the joint of (K, C) per r, O(W³) per evaluation × 199 grid points in `r_saturation`. Used as a Monte Carlo oracle (out§8) |
| **C6** | **the exact block law (formula in §0)**: the candidate, under the owner's choice in §0 | **0** | below |

**C6, term by term.** Each line is next to the sentence it implements.

- "A lost symbol, size-biased over windows":
  ```text
  weight(k) = k·π_W(k) / (W·ε)
  ```
  The weights sum to 1 because E[K] = Wε exactly (out§15: 3.2000 at W 64,
  ε .05).
- "Recovered iff C ≥ K, with C ~ Bin(N, 1−ε)":
  ```text
  S(k; n) = P(Bin(n, 1−ε) ≥ k)
  ```
- "N = ⌊rW + U⌋ under debt accrual":
  ```text
  (1−f)·S(k; n₀) + f·S(k; n₀+1)
  ```

**Provenance of each symbol.**

| symbol | provenance |
|---|---|
| ε | measured: ε̂ / p_upper, as today |
| σ² | measured: GE estimator → `burst_variance_factor` |
| W | per caller: A* (shed) or `encoder.window_size()` (`r_saturation`) |
| r | the rate under evaluation |
| (p, q) | derived: exact inversion of §2.4 |
| π_W | derived: the exact GE count DP, which needs no family assumption |
| U ~ U[0,1) | derived from the executed emission mechanism. Uniformity of the phase is an assumption (§6) |
| iid repair erasure | paper §4.2's stated assumption. Error bounded by C5 (out§8) |
| domain guards ε ∈ (0, 1), W ≥ 1 | the domain, not a threshold. ε ≤ 0 returns 0 although the limit is ≈ min(1, rW); that is harmless because every caller multiplies by ε (R = ε·(1 − P)) |

No constant appears, and nothing is tuned.

**Shape check.**
- Dimensionless: a weighted mean of probabilities, with weights summing to 1.
- Linear in r between integer rW. The slope is W·[G(n₀+1) − G(n₀)] ≥ 0, where
  G(n) = Σ weight(k)·S(k; n), because S(k; n) is non-decreasing in n. So it
  is monotone, and Lipschitz with constant ≤ W.
- At integer rW the two pieces agree (f → 1 on the left equals f = 0 on the
  right). The floor is not a mode switch: one formula, continuous across every
  integer.
- r = 0 gives n₀ = 0 and f = 0. S(k; 0) = 0 for k ≥ 1, and the k = 0 term has
  weight 0. So P = 0 is the formula's value, not a branch. The shipped
  `r <= 0` guard can stay as the domain projection of r < 0 only.
- **Cost and evaluation scheme.** See §7 R2 for the measured baseline.
  - The π_W DP is ≈ 8·W·k_max ops. G(n) for the ≤ 2·199 needed n is
    ≈ 6·199·k_max.
  - k_max is the largest k with tail mass > 1e-17 (out-r§R2). The dropped
    mass is ≤ 1e-17, so the error in P is ≤ 1e-17/ε: below f64 resolution of
    every caller, so not a step.

  The scheme (no quantised key, which would be a step):
  1. `r_saturation` does not read the mix term (`bulk_late_is_fine`), so it
     is evaluated **once per `RateSnapshot`** and passed to both
     `controller_rate` calls. This is exact and halves the cost.
  2. Its result is cached by **age only**, on the `RepairRateCache` precedent
     (`fec_rate.rs:233–245`, "Age only: W and the path are inputs, not
     keys"). The period T_sat is set so that the amortised cost meets the
     existing ~2 % core budget (`RATE_CADENCE_MAX_US` provenance,
     `fec_rate.rs:172–180`): T_sat = max(rate period, C_sat/0.02).
  3. Staleness is ≤ T_sat. This is a resource bound stated outside the law,
     like the rate cadence, with a gauge (evaluations, µs) in `rce=`.

**Where C6 departs from the shipped code.** Everywhere, not just below the
floor (out§7):

- At r = ε/(1−ε), C6 gives 0.29–0.41, not ½ (W 64–200).
- The per-loss miss 1 − P at r = r_c + 4 sd is 3.4e-3 to 1.3e-2, where Φ gives
  4–6e-5. Monte Carlo agrees with C6 (1.7e-2 against C6's 1.33e-2 at W 64,
  out§14).
- Cause: K's GE count tail is far heavier than the normal (out§15:
  P(K ≥ 13) = 1.07e-2 at W 64, where a normal with the paper's variance gives
  1.0e-3), and the size bias weights exactly those windows.
- The difference shrinks as 1.5/√W (out§13: 0.198, 0.111, 0.049, 0.022 at
  W = 64, 200, 1000, 5000), so C6 → Φ asymptotically. At the operating windows
  the Φ form is not in its regime.
- This is §4.2's own "known limitation" (`:739–744`), seen through the event
  the callers actually read.

## 3. Numerical check (pfec_check.out)

Grid:
- ε ∈ {0.005, 0.025, 0.05, 0.1, 0.2, 0.3}.
- r ∈ [0, 1] on h-grids, plus r_c = ε/(1−ε) ± 1e-9.
- W ∈ {3, 4, 5} (A*: the pin's fixture, `fec_rate/tests.rs:277`), 64 (sim
  golden w; the `r_saturation` fixtures) and 200 (the engine's encoder cap).
- σ² ∈ {1, 2.9, 3.0, 3.76, 5.0}: the iid floor; c2 GE 1.3/50; the pin and c1
  GE 0.05/50; c3 GE 2/40; the C4 fixture.

| metric | cur (C0) | phi (C1) | ccn (C2) | **palm (C6)** |
|---|---|---|---|---|
| jump at r_c (±1e-9), worst | **0.500** | 1.6e-7 | 1.4e-7 | 1.5e-7 |
| max jump at h = 1e-6, worst (out§12) | **0.500** | 7.9e-5 | 7.1e-5 | 7.3e-5 |
| max jump at h = 0.02, worst | 1.000 | 0.840 | 0.927 | 0.980 |
| max jump at h = 0.02, W 3/4/5 | 0.69/0.71/0.73 | 0.19/0.22/0.24 | — | **0.059/0.078/0.098** |
| jump at h = 0.01 / 0.005, worst | 0.976 / 0.840 | 0.680 / 0.340 | 0.623 / 0.382 | 0.734 / **0.367** (halves) |
| relative ±2 % nudge, max \|P(1.02r) − P(r/1.02)\| (out§17) | **0.607** | 0.122 | 0.123 | 0.123 |
| worst decrease in r (h 0.005) | 0 | 0 | 0 | 0 |
| engine pin, all three asserts | PASS | FAIL | FAIL | **PASS** |

- **The 2 % grid jump cannot tell a step from a steep slope.** At ε 0.005 and
  W 200 there is one expected loss, and four repairs arrive by r = 0.02. So the
  continuous Φ also jumps 0.84 there.
- The discriminating figures are the h → 0 rows. There C6's jump is ≤ W·h and
  halves with h, while C0's stays at ½.
- At the A* windows, where the shed budget reads P_fec, C6's 2 % jump is
  0.06–0.10. That is its true slope, ≤ 0.02·W.
- **The gate's reading.** The relative ±2 % nudge is what
  `test_visualizer.mjs:319–341` uses. Its tolerance is 1.5 × the law's analytic
  slope × the nudge width.
  - The worst case for C1 and C6 alike is 0.12, at ε .3 and W 200, which is
    Φ's own steepness. It sits inside C6's Lipschitz bound W·r·0.04.
  - C0's 0.61 is the step.
  - A slope-scaled gate can therefore separate C6 from C0. The gate nudges δ
    over a `SLOPE` table of named laws (`:322–338`). P_fec, or R through
    A*(δ), has no entry there yet, so whether the actual gate passes is
    untested (§6).
  - C6 does not read ~0 under any fixed absolute tolerance.
- **Continuity in ε and σ² (out§5):** a 0.1 % nudge moves C6 by ≤ 4.2e-3 (ε) and
  ≤ 7.7e-4 (σ²).
- **Limits (out§6):** at ε → 0 and σ² 1, P = min(1, rW) to 4 digits. At σ² 3.76 it
  is lower (a burst needs several repairs). At ε → 1, P ≈ 1e-25. P falls
  monotonically with σ² (0.898 → 0.473 at ε .05, W 64, r .1).

**The engine pin** (`fec_rate/tests.rs:262–282`) under C6:
- R(0; ε .048, W 5, σ² 3) = 0.048000 = ε exactly.
- The ladder at σ² 3.76 is monotone (0.0452 … 0.0015).
- R(0.34, A* 4, σ² 3.76) = **0.0309**, inside (0.001, 0.048). The shipped
  value is 0.0049.
- Monte Carlo with GE-correlated repairs gives 0.033 (out§8: P 0.308).
- So the pin's "~1 % class" becomes "~3 % class". The assertion still passes,
  but its comment is wrong under C6.

**Monte Carlo oracle (C5, out§8).**
- C6 is within 0.002–0.012 of the oracle at W 64–200.
- At W 4–5 it is optimistic by 0.03–0.05 (0.357 against 0.308): with W 4, a
  burst also kills the repairs. That error is the sign paper §4.2 already
  states.
- At r > 0, Φ is off by 0.12–0.59 at the same points.
- The shipped form matches Φ above r_c. Below r_c it reads 0: at ε .05,
  W 200 and r 0.0526 (just under r_c) the oracle gives 0.377. It is exact only
  at r = 0.

## 4. Reach and predicted engine effect

> **Block-law figures (round 6).** C6 does not ship. Every "effect of C6"
> below is a block-law figure, kept for the §8.2 oracle's registration, not a
> live prediction: the r_sat shifts, the ρ < 1 budget rise of 6–11×, and the
> CPU estimate. The L1 proof (r\* bit-identical 0) holds under any form.

**Callers.**

| caller | window | event read | effect of C6 (block law) |
|---|---|---|---|
| `r_saturation` `tail_fec` (`raptorpath-math/src/lib.rs:935`) → `controller_rate` soft cap (`:825`), on by default (`control/fec_rate.rs:346`, passed `:515`) | encoder `window_size()` (`fec_rate.rs:499`) | per-loss (cost of a loss that falls to ARQ) | r_sat rises: P_fec is lower, so more FEC is worth more |
| engine shed budget `residual_loss_after_fec` (`fec_rate.rs:784–790`, `net/emit_source.rs:587–598`), only when `shed_on` (ρ < 1; `net/shed.rs:23`) | A* | per-loss | R is higher at every r > 0 (e.g. 0.0049 → 0.0309 at the fixture) |
| `compute_delta`, `p_recovered_within`, `find_t_cut` (`lib.rs:1188`, `:1198`, `:1213`) → sim T_cut (wasm `:1396`) | sim window | per-loss | moves. T_cut (W14) is retired by owner decision 3 |
| wasm `shed_budget_residual` (`:217–219`), the cartoon R, sim `get_r_sat` (`:1676`), sim rate via `controller_rate` (`:833`, `:870–890`) | A* / sim window | per-loss | the cartoon R becomes continuous through δ 0.045 / 0.058 (the W22 rows of the audit) |
| §4.2's r* inversion (`compute_r_star_with_z`, `lib.rs:110`) | — | window event | **untouched**: it does not call `p_fec_normal`. Its constraint (`:704–706`) has the same event error (see §6) |

**`r_saturation` shifts (out§9).** r_sat moves from the current value to C6's:

| inputs | current | C6 | note |
|---|---|---|---|
| C4 fixture | 0.400 | **0.550** | outside the band [0.2, 0.45] asserted at `lib.rs:1449–1459` |
| C2 fixture | 0.255 | 0.280 | still > 0.2 |
| sim golden ε .05 / .10 | 0.255 / 0.420 | 0.315 / 0.560 | |
| c1, nominal | 0.140 | 0.140 | nominal inputs, W 200 |
| c2, nominal | 0.140 | 0.145 | nominal inputs, W 200 |
| c3 (c8's slow leg), nominal | 0.160 | 0.180 | nominal inputs, W 200 |

The nominal rows use netem's GE (ε, σ²), 2× one-way delay and the link t_sym.
They are **not** estimator readings: the GSO burst factor g (`tools/l1/lib.sh:150–170`)
makes the estimator's σ² differ.

**No new r_sat staircase in ε (out§19; srtt and σ² were not swept).** I swept
ε in 0.1 % relative steps
(W 64 and 16, and the C4 inputs). The largest step of r_sat is 0.005 under C0
and C6 alike: the grid resolution of `r_saturation`'s argmin. C6's kinks at
r = n/W add no step beyond it.

The argmin grid is itself a 0.005 staircase in every input. It reaches the
rate through `soft_saturate`. That is a pre-existing R04 finding, independent
of P_fec, and is recorded here for R04.

**Registered prediction for the L1 cells (c1s, c1d, c2, c8): r\* UNCHANGED,
bit-identical 0, and no shed-budget change. This is a proof, not a band.**

**Premises.** These are the default arms the L1 batteries run:
- `bulk_pure_arq` is true (`fec_rate.rs:347`; its setter is called only from
  tests), so β = bulkness.
- `inner_feedback` is 0, so the floor is applied before the cap with w = 0
  (`config.rs:355–358`, math `:814–817`).
- `RWM_COMPLETION_EXPOSURE` is unset (`gates.rs:375`), so χ = 0.
- `RWM_DELTA` is assumed unset, so the dial sits at the Bulk point. It is
  forwarded by `tools/l1/lib.sh:71`; I did not read which batteries set it
  (§6).
- The r_battery GLIDE arms (`tools/l1/r_battery.sh:162–163`) set χ > 0. There
  r_bulk > 0, and C6 can act.

The `bulk_rate_is_exactly_zero_through_the_controller` pin is the witness for
the default configuration. My broad grep also matched `net/channel_set.rs:65`
(`p_lost_inputs`) and `net/mod.rs:1023` (a comment). Neither calls a P_fec
function.

1. The L1 harness runs `--protocol-hint bulk` (`tools/l1/perf_rwm_c.sh:38`), so
   β = 1, and χ = 0 on the tunnel (ledger R01, `law-ledger.md:919`).
2. The mix (1−β)·r_anchor + β·r_bulk (`fec_rate.rs:157–170`) is then
   0·r_anchor + r_bulk, and r_bulk = 0.0 exactly. δ_eff = p gives z = −∞
   (`lib.rs:840–845`), the burst fraction 0 and the mass term 0, so the
   pre-cap rate is 0. Pinned by `bulk_rate_is_exactly_zero_through_the_controller`
   (`fec_rate/tests.rs:428`).
3. `soft_saturate(0, r_sat)` = max(0, r_sat − 0.1·r_sat·softplus(10)) = 0 for
   every r_sat > 0 (`lib.rs:1082–1093`; out§10: 0.0 over the whole grid
   [0.01, 1]). So r_sat, the only place P_fec enters the rate, cannot reach
   r*.
4. Shed: L1 runs `--window-reliable` (ρ = 1, `perf_rwm_c.sh:81–85`), and
   `shed_armed` requires `!reliable` (`net/shed.rs:23`). It is also moot: at
   r = 0, C0 and C6 both give R = ε̂.

**The law still executes on these cells.** `controller_rate` calls
`r_saturation` whenever `saturation_cap && t_sym > 0 && srtt > 0`
(math `:824–826`), whatever the pre-cap rate. `RateSnapshot::rate` evaluates
both terms, so P_fec runs 2 × 199 times per rate evaluation on every L1 cell.
Only its output is annihilated (step 3).

**Registered per-cell prediction (c1s, c1d, c2, c8).**
- r* = 0, bit-identical.
- Shed unarmed.
- CPU on the `rce=` gauge. Today it reads 2.2 / 3.2 / 93 / 68 µs per
  evaluation at c1s / c1d / c2 / c8, at about 150–245 evaluations per second
  (§7 R2).
  - Without the §2 scheme, each evaluation would add one `r_saturation` at
    W 201: 1.5–3.1·10⁵ ops (truncated, out-r§R2), i.e. of order +0.2–0.3 ms.
    That is 5–7 % of a core at 200 evaluations per second.
  - With the scheme, the amortised cost meets the 2 % budget by
    construction, and `rce=` µs per evaluation rises only on the evaluations
    that refresh r_sat.
  - This is an **estimate that needs a bench**. The guard's CPU/GB clauses are
    where it would show.
- Rate and goodput clauses should read a no-op.
- The rate itself moves only off-Bulk (Auto, β = 0, where the soft cap acts),
  which L1 has not measured (`law-ledger.md:919`).
- **ρ < 1 has been measured:** the paper §5.6 battery on the evicting seat
  (L0 c3-1200B, ledger `8e77ba67`). There the sender budget read
  bud = 0.000–0.002 at A* 4–7, r 0.2–0.5, and it bound: shed 0–25 against
  52–773 refused per rep. Under C6 the fixture's budget goes 0.49 % → 3.1 %
  (A* 4) and 0.22 % → 2.4 % (A* 7) (out-r§R3d). **Prediction:** at that cell
  the budget rises 6–11×, and with it the number of past-deadline holes the
  sender may shed, up to the budget, against 0.25 % measured shed today.

## 5. What the code change would touch (not done)

The owner chose path (b), so C6 does not ship. This list is what **any**
replacement of `p_fec_normal` touches; the future sliding law takes C6's
place in it. Its bounding test is the §8.2 oracle.

**`raptorpath-math`.**
- `p_fec_normal` (`lib.rs:885–893`) becomes the C6 form under a new name. The
  cut at `:888` goes.
- `r_saturation` (`:917–944`) precomputes G(n) once per call.
- The doc comments at `:883–884` and `:895–916` change.
- Main's W22 bounding test (`ac3ecd13`,
  `raptorpath-wasm/src/lib.rs:2241–2268`
  `test_p_fec_normal_vs_paper_phi_divergence_is_bounded`) bounds the cut
  against Φ. It is retired, or re-targeted to the new law vs the §8.2 oracle,
  when `p_fec_normal` is replaced.
- Expectations that move (values are C6's, the block-law placeholder):
  - `engine_fold_pins.rs:39–48`: three of the four bit pins (0.779 → 0.734,
    0.679 → 0.512, 0.571 → 0.524; the W 384 pin stays 1.0). Engine-fold pin:
    a re-pin needs the owner's word.
  - `formula_verification.rs:143`: 0.335, which fails its band (0.4, 0.7);
    its premise is "½ at the floor".
  - `formula_verification.rs:151`: 0.085, still < 0.1. `:147`: 0.9984, still
    > 0.99. `:226` (sum-to-one) holds by identity. `:251` and `:264`
    (monotone in r and W) hold.
  - `lib.rs:1449` C4 band fails (0.55).
  - `monte_carlo_validation.rs:89` and `:940` (tolerances vs Φ), `:899–918`
    (r_sat argmin re-implementation, which must use the new form) and
    `temporal_oracle.rs:1504`, `:1743`: re-check. The ported `:1504` band
    (0.6, 1.7) holds under C6, at 0.99–1.01 (§7 R1).

**Engine (`raptorpath`).**
- `residual_loss_after_fec` (`fec_rate.rs:784`) is unchanged in shape.
- `fec_rate/tests.rs`:
  - `:1147` fails: 0.064 is not < 0.01. Its premise "r < ε/(1−ε) ⇒ 0" is the
    cut itself.
  - `:1139` (0.933 > 0.9) and `:1143` hold. `:1101` and `:1154` hold.
  - `:262–282` holds; the comment at `:277` becomes "~3 %".
  - `:400–425` (soft cap equals `soft_saturate(uncapped, r_sat)`) holds by
    construction, but its `r_sat` value moves.
- `rstar_bitpin_table_*.in` (audit W22): re-capture if it records r_sat or the
  capped rate.

**wasm and visualizer.**
- The six goldens of `test_multipath_n1_identical_golden`
  (`raptorpath-wasm/src/lib.rs:1815–1845`) need `GOLDEN_CAPTURE=1`.
  - The four auto/realtime rows are expected to move. Their fec/src ratios of
    0.33–0.51 sit near or above the nominal r_sat (0.255/0.42 → 0.315/0.56).
    Both figures are nominal, not read from the sim's estimator.
  - Bulk ε .05 (fec 0) should not move.
  - Bulk ε .10 (fec 85, a non-zero rate through the soft cap) may move.
- `shed_budget_residual` test `:2808–2810` holds (0.05 and 1.4e-4).
- `test_custom_rho_gives_up_late_losses` and `test_bulk_composes_with_rho` may
  move, as the audit says.
- The W22 exclusion in the continuity gate (owner decision 3) is deleted.
- `shed_budget_residual` (`:217–219`) passes the cartoon's fractional A*. It
  takes the real-W extension of §1, and σ² is floored at 1 there.
- **New model-vs-engine divergence.** The cartoon mixes ⌊A*⌋ and ⌈A*⌉,
  while the engine uses ⌈rate·D⌉ (`emit_source.rs:578`). Per CLAUDE.md it needs
  a bounding test: |R_cartoon − R(⌈A*⌉)| ≤ the one-slot difference of C6.
- Add an R(δ) check to `test_visualizer.mjs` with the derived tolerance tol_R
  of §7 R4. It runs in linear space, has no monotonicity clause, and covers a
  dial sweep that includes δ 0.045–0.058, not only the three preset nudges.

**Paper.**
- §4.2 `:693–700`: the Setup becomes the per-loss event and the C6 line. "This
  is exactly ½" goes.
- `:205` already states the right meaning.
- A.3 `:4711+`: the law row.
- §4.4 `:810` (r_sat) and §5.6 `:1723` read through the new P_fec.

**Coupled law.** R04's `r_saturation` constants (1.5·SRTT, `C_DILUTION` 0.5,
the grid) have no provenance. Its argmin was effectively placed with the cut
P_fec, and C6 moves it (C4 0.40 → 0.55, above the ~0.49 Realtime request that
fixture was written to cap). R04 should be re-derived in the same change, or
the cap's role at C4 is lost.

## 6. Not verified here

- **Uniform accumulator phase U.** It is assumed, not measured, in the engine
  `TaperBudget` (its offset re-timing, `fec_rate.rs:698–700`) and in the sim's
  `min(8.0)`-capped debt (`:1348`).
- **Block-equivalent vs sliding-window decoding.** Simulated in §7 R1:
  sliding is +0.04–0.15 over C6 with no deadline. Under the engine's ARQ race
  and the taper-copy slots it is not measured. The sliding per-loss law is not
  derived.
- **Whether the engine ever ran on the P_fec > 0 side.** The "consumed r ≈ 0.34
  at ε ≈ 4.8 %" fixture comment (`fec_rate/tests.rs:277–279`) is unsourced.
  §4's proof needs only the Bulk configuration, not this.
- **Estimator inputs for the nominal r_sat rows.** The engine's measured σ², W
  and t_sym per cell are not in the ledgers (ε̂ under-reads 3–5×,
  `docs/status.md` §3.8 :282). The rows are labelled nominal.
- **§4.2's r\* inversion.** It uses the window event and the normal tail. By
  out§7's tail numbers it under-provisions more than §4.2's "30–50 %" note says.
  That is a separate finding for the r* rows of the ledger and §4.3, not part of this change.
- **The Rust cost.** 1.5–3.1·10⁵ ops per `r_saturation` at W 201 (§7 R2) is an
  operation count, not a bench. The 0.15–0.3 ms per call and the T_sat that
  follows from it are estimates.
- **The coded-only and generation windows** (640 and G·(M+1)) are not on L1
  (`generation=false`) and were not grid-checked. The cost scales as W·k_max.
- **The visualizer gate.** The tol_R check of §7 R4 is specified but not run.
- **The measured A\* datum (0.71–0.84).** Its per-rep inputs are unrecorded,
  so it does not separate the forms (§7 R1d).
- **The `temporal_oracle` port** is Python. The cargo test itself was not run
  (no-cargo brief).
- **Unread premises.** Which batteries set `RWM_DELTA`, and whether the JS UI
  can pass σ² < 1 to the raw wasm exports. The UI derives σ² from (ε, q)
  (`interactive-visualizer.html:129`), but its slider ranges were not read.

## 7. Main-session review (round 4): decoder, cost, reach, gate

Numbers: `pfec-continuous/pfec_review.py` → `pfec_review.out`, cited as
**out-r§Rn**.

> **Superseded in part by §8 (round 6).** Wherever this section compares C6
> or the shipped form with the sliding decode as a point value, the reading is
> retracted: "reads low", "unsafe direction", "nearer", "overstated", "meets
> the Auto target". The sliding decode is a one-sided bound (an upper bound on
> recovery, a lower bound on the miss). §8.3 holds the corrected bracket and
> §0 the corrected status. The numbers below stand; their verdicts do not.

### R1. The decoder at the A* windows

**(a) Arbiter.**
- `raptorpath-math/tests/formula_verification.rs:143` is analytic: it puts
  bands on `p_fec_normal` itself. It is not an arbiter.
- The only executed-process check of P_fec in the repo is
  `temporal_oracle.rs:1476` `unified_rstar_n1_reduces_to_84`. Its process
  (`:1361–1423`) is a block window of W 64, GE-correlated repairs after the
  sources, and probabilistic rounding of rW. It counts per-symbol misses
  (`fec_miss`), which is the per-loss event. So it arbitrates the **count law
  and the event**, not the sliding decoder.
- I ported it to Python (a port, not a cargo run). The port passes its
  self-check: the shipped form's ratios lie inside the test's green band
  (0.6, 1.7).

| ε, δ (W 64, q .5) | shipped | plain Φ | **C6** | C5 (exact joint) |
|---|---|---|---|---|
| .05, .02 | 1.528 | 1.528 | **1.010** | 1.010 |
| .05, .03 | 0.799 | 1.334 | **1.004** | 1.006 |
| .10, .04 | 1.322 | 1.322 | **0.991** | 0.993 |
| .10, .06 | 0.724 | 1.210 | **0.990** | 0.997 |
| .08, .03 | 1.416 | 1.416 | **1.003** | 1.003 |

The ratio is oracle tail / (ε·(1 − P)), out-r§R1c. **C6 sits closest: within
1 % on all five scenarios. Shipped and Φ are off by 20–53 %.**

**(b) The "~1 % class" in the pin comment.**
- It is the streaming machine's measured shed share, not a measured FEC
  residual. Its source is commit `6568822c` ("the residual sits in the ~1 %
  class the streaming machine sheds") and ledger `8e77ba67` (streaming lost
  140/14 000 ≈ 1.0 %).
- The same battery's measured inputs are A* 4–7 and r 0.2–0.5 (L0 c3-1200B,
  GE 2/40, so σ² 3.76 per datagram, `transport/l0_netem.rs:76`). They give
  one attribution datum: data-direction holes resolved by DECODE 51 against
  SOURCE/ARQ 10–21, a share of 0.71–0.84.
- That is one FDIAG rep with ≈ 65 holes, and its r and A* were not recorded
  per hole.

**(c) Exact joint GE and a sliding decode.**
- **C5, the exact joint GE per-loss block value at W 3–7** (out-r§R1a), at
  ε .048, σ² 3.76, r 0.29–0.34: 0.20–0.47. C6 gives 0.26–0.50. C6 is above
  C5 by 0.03–0.06, because correlated repair erasure is ignored.
- **Sliding-window decode simulation** (out-r§R1b):
  - It uses the engine's trailing span [end − A*, end), end = newest + 1 − Δ,
    with debt-accrual emission and one GE chain over the wire.
  - It decodes by generic rank with no deadline, so it is an **upper bound**:
    in the engine, ARQ races it.
  - Runs: 3 seeds × 2 rank tolerances, standard error ≤ 0.006.
  - Result: 0.34–0.62 at A* 4–7. That is 0.04–0.15 **above** C6. The A* 5,
    r .34 jump from Δ 1 to Δ 2 (0.452 → 0.555) is reproduced across seeds
    (SE .003), so it is real.
  - At W 64, r .1 the sliding value is 0.768, C6 0.625 and shipped 0.826, so
    the shipped form is nearer there. At W 200, r_c: sliding 0.434, C6 0.375,
    shipped 0.

**(d) Which inputs reach the measured band** (out-r§R1d, ε .048):

| form | in-band cells (σ², A*, r) |
|---|---|
| shipped | (3.76, 4, 0.2), (3.76, 7, 0.2), (2, 4, 0.2) |
| C6 | (1, 7, 0.2), (2, 4, 0.5), (2, 7, 0.34); none at the nominal σ² 3.76 |
| C5 | same cells as C6 |
| sliding | (3.76, 7, 0.5) only |

So the datum does not separate the forms. At the nominal σ² 3.76, the
shipped form and the sliding bound each reach the band somewhere in the
battery's input range, but C6 and C5 reach it nowhere. The rep's
(σ²_est, r, A\*) are unrecorded.

**Verdict for R1: one status at every caller.**
- **C6 is the exact block law under iid repair erasure.** It is confirmed
  against the repo's executed block oracle.
- **Every executing caller decodes a sliding window:**
  - the engine: the trailing span at A\* 4–7 on the evicting seat, and A\* ≈ W
    201 on L1;
  - the wasm sim: `send_repair` → `generate_repair` over the whole current
    window, `raptorpath-wasm/src/lib.rs:899–900`, `raptorpath-math/src/rlc.rs:41`;
  - the cartoon, which uses the same span law.
- Against the no-deadline sliding bound, C6 reads low at **every** window
  tested. In P it is 0.04–0.15 low at A\* 4–7, 0.14 at W 64 and 0.06 at
  W 200 (≈ 2.5 SE). In the tail at W 201 it is two orders of magnitude low
  (R3).
- Low P is the unsafe direction: the shed budget R is larger (non-conservative
  for ρ), and the r_sat shift of §4 is overstated in the same direction.
- The **sliding per-loss law is the open derivation.** The owner's choice
  between (a) and (b) is stated in §0.
- The shipped form is excluded at every caller: it is the step, and it is
  20–53 % off the block oracle.
- **Gauges that settle the A* question.** In one evicting-seat rep, record
  together:
  - `[SPAN] a_star=`, `rr=` and `shed=/bud=`;
  - ε̂ and σ²_est;
  - the FDIAG decode-vs-SOURCE/ARQ attribution.

  Register the prediction per form before the run.

### R2. W and cost on L1

- **The W `r_saturation` receives on L1 is 201.** This is the `[SPAN] wsize=`
  p50 at c1s, c1d, c2 and c8 (max 213–247), with `generation=false` in the
  gate banner (§1). The generation and coded-only caps do not apply on L1.
- **Measured baseline** (`rce=` gauge, anchor-diag MAIN client rows,
  evaluations / µs total):

  | cell | evaluations / µs | µs per evaluation | evaluations per second |
  |---|---|---|---|
  | c1s | 925 / 2078 | 2.2 | ≈ 190 |
  | c1d | 1423 / 4576 | 3.2 | ≈ 245 |
  | c2 | 1362 / 126 064 | 93 | ≈ 155 |
  | c8 | 1153 / 78 994 | 68 | ≈ 154 |

  The period is min(5 ms, srtt/4) (`fec_rate.rs:233–235`). W changes on about
  half of the evaluations at c1s and c1d (`w447/925`, `w679/1423`), so a key
  on W would miss. A quantised key would be a step.
- **C6 cost at W 201** (out-r§R2), truncated at tail mass 1e-17, error
  ≤ 1e-17/ε: k_max is 55 / 79 / 109 at c1 / c2 / c3 nominal, giving
  1.5–3.1·10⁵ ops per `r_saturation` call. The full un-truncated cost is
  5.7·10⁵.
- **Evaluation scheme** (§2 cost):
  1. r_sat is computed once per snapshot, not twice. This is exact.
  2. It is cached by age only, with period T_sat = max(rate period,
     C_sat/0.02). At about 0.15–0.3 ms per evaluation, T_sat ≈ 8–15 ms.
- **This is a second resource bound for the owner. It does not meet
  `RATE_CADENCE_MAX_US`.** That constant bounds the rate's staleness at 5 ms,
  while r_sat would be up to 8–15 ms stale.
- The 2 % is **additional**: c2 and c8 already spend 1.0–1.4 % of a core on the
  rate (93 µs × 155/s, 68 µs × 154/s).
- **Prediction on `rce=`:** µs per evaluation stays at the baseline
  2.2 / 3.2 / 93 / 68. An additional +150–300 µs appears only on the r_sat
  refresh (≤ 1 per T_sat), so the amortised added load is ≤ 2 % of a core.
  This is an estimate that needs a Rust bench.

### R3. Root cause reach

**Quotes and confirmation.**
- `fec-arq-model.md:205`: "| P_fec | probability a lost symbol is
  FEC-recovered | probability |".
- `:693–699`: "Loss count K in the window is approximately Normal(Wε,
  Wε(1−ε)σ²_burst); surviving repairs are approximately Normal(rW(1−ε),
  rWε(1−ε)). FEC recovers the window iff surviving repairs ≥ K, so
  P_fec(r) = Φ(…)".
- `:703–704`: "A symbol is late only if it was lost, so the per-window
  constraint that meets tail target t_tail is `P(repairs < K) ≤ t_tail/ε`."
- **Confirmed: the §4.2 event includes K = 0.** It is "the window recovers";
  at r = 0 its exact value is P(K = 0) (0.864 at the pin, out§1).
- :703–704 is where the two events are conflated. Dividing a window-event
  probability by ε treats "window fails" as "a given lost symbol is late". It
  ignores that a failing window holds K > 1 losses (size bias).

**Rate-law finding (flagged for the law ledger, the r\* rows, and §51).**
- The machinery first reproduces paper §4.7's exact r\* on the window event
  (WiFi / LTE / Sat 0.170 / 0.270 / 0.450, out-r§R3a, exact match).
- On the per-loss event the exact r\* is **0.250 / 0.370 / 0.590**, +47 / +37 /
  +31 %. C6 gives 0.239 / 0.346 / 0.523.
- **Off-Bulk at the engine targets** (W 201, nominal L1 (ε, σ²), random term
  only, out-r§R3b):

  | cell | Auto (t 1e-5): closed → C6 | Realtime (t 1e-7): closed → C6 |
  |---|---|---|
  | c1 | 0.010 → 0.049 | 0.015 → 0.087 |
  | c2 | 0.091 → 0.141 | 0.113 → 0.191 |
  | c3 | 0.158 → 0.226 | 0.191 → 0.295 |

  - All values stay below `max_overhead` 0.5.
  - At the shipped closed-form r\*, C6's tail is 52–53× the target at Auto and
    850–3 300× at Realtime. The block oracle port reads 1.2–1.5× at W 64 with
    mild targets.
- **The sliding-decode check does not reproduce this**
  (`pfec-continuous/pfec_tail.py` → `pfec_tail.out`).
  - Setup: span A 201, the closed-form Auto r\*, nominal (ε, σ²), no
    deadline, 2 seeds × 80 k sources.
  - The per-loss miss is **0 at c2** (4 101 holes, target t/ε 3.9e-4) and
    **2.7e-4 ± 1.9e-4 at c3** (7 472 holes, target 2.1e-4).
  - C6 predicts 2.0e-2 and 1.1e-2; the shipped Φ predicts 4.7e-4 and 2.8e-4.
  - So under the executed (sliding) decoder, the closed-form r\* meets the
    Auto target within SE at these inputs.
  - Realtime (t 1e-7) is below what 10⁴ holes can resolve, and is not
    checked.
- **Finding, relabelled.** "The §4.2 inversion under-provisions by 52× to
  10³×" holds for the **block** law only. It is **not** an engine finding.
  - The event error at :703–704 is real.
  - Its size for the executed decoder depends on the sliding per-loss law,
    which is open.
  - For the law ledger and §51: record the event error and the block-law
    figures, with the sliding check as the bound on their engine relevance.
- The composed rate (max with the burst term and the §4.3 mass term) is not
  computed. Off-Bulk is unmeasured on L1, which runs Bulk only.

**Status "Owner decisions (2026-11-14)" item 3, recomputed** (out-r§R3c):
- *Cartoon inputs* (`interactive-visualizer.html:2625–2650`): rate = 4(1−ε),
  D = min(b·RTT, 2·RTT), A\* = rate·D. These reproduce the audit's R(code) on
  all ten rows (e.g. 1.9e-5, 5.1e-5 against 1.9e-5, 5.2e-5), which validates
  them.
- **ρ-inert at Auto/Realtime changes.**
  - Under C6, R is 3.5e-4 to 4.1e-3.
  - At Realtime R exceeds the slider's 1e-3 step on all five cells
    (1.2e-3 to 4.1e-3), and at Auto on .025/.5/13 (2.0e-3).
  - The R side binds at 96–99 of 100 positions there, instead of 100. So ρ
    acts on 1–4 positions per cell.
  - R·2000 is 0.7–8 symbols, no longer under one.
  - The recorded owner consequence "ρ is inert over the slider at Auto and
    Realtime" becomes "ρ acts only at the 1–4 lowest-ρ positions". This is
    still a degenerate-law finding, but it is no longer total.
  - Auto on the other four cells stays inert: R 3.5e-4 to 7.9e-4.
- **T_cut at harsh Bulk is unchanged, by identity.** At Bulk r\* = 0, so
  R = ε·(1 − P(0)) = ε under the shipped form and C6 alike. The condition
  1 − ρ < R·ε^(D/RTT) = ε³ (D = 2·RTT) is the same expression. So the audit's
  8/100 (ε .2, threshold 8e-3, where the count sits on a float tie at the
  0.008 grid point) and 26/100 (ε .3, 2.7e-2) stand. They are not a recount.
- The sim-input rows of the audit need the sim itself and were **not
  recomputed**.

### R4. Gate and the change of criterion

**Change of criterion, recorded.**
- The brief asked for "≈ 0 neighbour jump on a 2 % grid". That criterion was
  **replaced** by the h → 0 rows (§3: the jump at h = 1e-6 and at r_c ± 1e-9,
  plus halving with h).
- Reason: a 2 % absolute grid jump cannot separate a step from a steep slope.
  The continuous Φ itself jumps 0.84 there, and C6 jumps 0.98, because the
  law's true slope at ε .005, W 200 is ≈ W·ΔG.
- Only a criterion that shrinks with h tests continuity, which is what the
  NO-MODE-SWITCH invariant asks.

**The derived slope bound (out-r§R4).**

```text
∂P/∂r = W·(G(n₀+1) − G(n₀)) ≤ L_r := W·max_n (G(n+1) − G(n))        (exact on each piece, tight)
```

- Verified: the finite-difference slope over L_r is ≤ 1.0000 on the grid.
- Values: 1.1 at A\* 4, 8.0 at W 64, 9.4 at A\* 51 (.025/.5/13) and 20.1 at
  W 201.
- In W (real-W extension), on each piece:
  ```text
  ∂P/∂W = P(r; m₀+1) − P(r; m₀)
  ```
- For the gate's δ nudge, R(δ) = ε·(1 − P(r\*(δ), A\*(δ))). Compare it in
  **linear** space with
  ```text
  tol_R = 1.5 · ε · ( L_r·|Δr*| + max_m |P(m+1) − P(m)|·|ΔA*| )
  ```
  where Δr\* and ΔA\* are the nudge's own changes. dA\*/d log₁₀δ =
  −A\*·(ln 2)/2 where unclamped, from b(δ) = 2^(−log₁₀(δ/δ_Auto)/2).
- Log space is unusable: d log R = −dP/(1 − P) is unbounded as P → 1.
- R is not monotone in δ (audit "Shape in δ"), so the gate's monotonicity
  clause must not be applied to it.
- **A `SLOPE` entry at the three presets cannot see W22.** The step sits at
  δ ≈ 0.045–0.058, between presets. The R check therefore needs a **dial
  sweep** (e.g. every 1 % of log δ over [0.005, 50]) with the same tol_R per
  step, not only the preset nudges.

## 8. Round 6: path (b), the executed oracle

### 8.1 What exists (an executed oracle for P_fec at the engine's windows)

I checked each candidate against five criteria: real engine codec, trailing
span A\*, the engine's own emission law, deadline D(δ), and no I/O. Every
caller of `generate_repair_range` was grepped.

There are **two production callers**:
- `net/emit_source.rs:798`: the proactive trailing span, the path the law
  models.
- `net/mod.rs:3319`: the paper §7.6 receiver-request arm. It answers a
  receiver-requested span `[a, a+m)` with one coded equation (`m > 1`), and
  falls back to copies on refusal (`:3262–3325`). This is a second FEC path:
  if armed, its decodes would count as DECODE in an FDIAG attribution. It is
  armed only by `RWM_RECV_REQUEST_LAW` or `RWM_RANK_FEEDBACK`, both default
  off (`gates.rs:376–377`, `mod.rs:1676–1678`). So it is out of the default
  machine and out of the oracle, unless a run arms it; then it must be
  counted separately.

The remaining callers are a pin (`net/hotpath_pin.rs:133`) and tests in
`fec/`.

| candidate | real codec | trailing span A\* | engine emission (`TaperBudget`) | deadline D(δ) | no I/O | P_fec isolated (no ARQ race) |
|---|---|---|---|---|---|---|
| `tests/common/gate_harness.rs` `run_fec` (:477) | yes: `RlcWindowDecoder` (:561) | **no**: `generate_repair()` over the leading window (:722–731) | **no**: its own per-path `debt`; `TaperBudget` does not occur in the file | no | **yes**: mock clock, GE channel | **no**: the P_lost-gated retransmit runs (:746+) |
| `tests/unified_stream_l0.rs`, `unified_l0.rs` (`RWM_UNIFIED` arms) | yes | yes | yes | yes, the shed D(δ) on the evicting arm | **no**: memory TUNs plus real QUIC on 127.0.0.1 plus the L0 netem shim | **no**: ARQ races; FDIAG attributes after the fact (the 51 vs 10–21 datum came from `unified_stream_l0`) |
| `tests/taper_emission_l0.rs` | yes | yes | yes | **no**: "plain window-reliable mode" (its header, :4–5), so there is no shed deadline | no (same loopback stack) | no |
| wasm sim (`raptorpath-wasm`) | **no**: `raptorpath-math/src/rlc.rs`, a separate simplified codec (`rlc.rs:1–4`) | **no**: whole-window `generate_repair` (`lib.rs:899–900`) | **no**: its own debt (`:1348`) | via ARQ ticks | yes | no |
| `tests/fec_window_test.rs`, `sim_pipeline_test.rs` | yes | no | no (scripted losses) | no | yes | yes, but no emission law |
| `fec/prune_contract_tests.rs` | yes | range repairs, test-scripted | no | no | yes | — |

**Finding: nothing executes the engine's trailing-span emission with the real
decoder, without I/O, with P_fec isolated from ARQ.** The L0 tests execute
the whole engine but cannot isolate P_fec. The gate harness is I/O-free but
uses the wrong span and races ARQ.

Two premises of the Python sliding bound are model inputs:
- **Emission spacing.** `TaperBudget::accrue` (`fec_rate.rs:703–736`) re-times
  the grant with the GE taper shape W·q·(1−q)^t/(1 − (1−q)^W), so repairs are
  **front-loaded after each ack advance**, not uniform.
- **One GE chain over the wire.** Dual-path placement breaks this (§8.3,
  T2).
- **Δ.** The bound rises with the trailing offset Δ, because a no-deadline
  decode gains from later spans. The engine's Δ is clamp(⌈rate·J⌉, 1, 64).
  §8.3 reports the bound over Δ ∈ {1, 2, 3, 4, 8}; beyond 8 it is not
  computed.

So the sliding bound holds only for uniform spacing on one chain, for the Δ
range computed.

### 8.2 Next deliverable: a test-only Rust harness (design only, no code)

`raptorpath/tests/pfec_sliding_oracle.rs`: test-only, with no engine edit
(ADR-0077 scope).

- **Drives:** the engine's real `RlcWindowEncoder` / `RlcWindowDecoder`
  (`fec/rlc_window.rs:31`, `:248`). Emission goes through the engine's own code:
  `TaperBudget::accrue` and the span placement `[max(ws, end − A*), end)`,
  end = newest + 1 − Δ (`emit_source.rs:784–806`).
  - If that placement is not callable from a test, the harness re-states it
    behind a pin test that asserts byte-identity with `emit_source`'s range
    arguments on a recorded trace.
  - Under shared-core-0 (`raptorpath-core`, empty at main `e9188e01`), the
    span decision is a candidate to move there first, so the harness and the
    engine share it.
- **Channel:** `tests/common/mod.rs` `GilbertElliottChannel` (:29), loss iff
  Bad, stepped per wire symbol. Each repair is either on the same chain or on
  a second independent chain (the dual-path premise). (p, q) come from
  (ε, σ²) by §1's inversion.
- **No ARQ:** recovery is FEC only, so P_fec(D) is well defined.
- **Feedback clock.** `TaperBudget::accrue(rate, offset, taper, span, spare)`
  needs `offset` (sources since the last cumulative-ack advance) and `spare`.
  So the harness carries:
  - a feedback delay F in slots, set from the RTT at the send rate;
  - the cumulative-ack rule: ack = highest contiguous delivered-or-given-up
    seq, reported F slots later;
  - the receiver's give-up at D. A hole older than D is declared lost and the
    ack passes it. Without this, one unrecovered hole stalls the ack, and
    `offset` grows for the rest of the run;
  - `spare` as a declared input. Default: non-binding, i.e. ≥ the grant.
    Swept once at a binding value.
- **Deadline:**
  - A\* and D are not independent: A\* = ⌈rate·D⌉ (`emit_source.rs:578–580`).
    So D in slots is A\*, and the harness takes A\* as the input. D(δ) =
    min(b(δ)·RTT, 2·RTT) is reported alongside.
  - A lost source s counts as recovered at D iff the decoder outputs it before
    send slot s + A\*. P(∞) is also reported.
  - `r_saturation` has no δ input. Its tail_fec term charges a miss with
    L_arq = 1.5·srtt (`raptorpath-math/src/lib.rs:926`), so at W 201 the harness reports P at
    D = L_arq in slots as well as at D = A\*.
- **Counts:**
  - lost sources;
  - recovered by D;
  - recovered at all;
  - range repairs emitted, with `delta=` and `a_star=` logged per repair;
  - leading-window fallbacks (the `emit_source` fallback when the window is
    too young to trail). This is a witness that the range path fired.
  - the grant trace (Σ grants against Σ r, `owed`).
- **Mapping:**
  - **shed budget:** span A\* ∈ {3..7}, encoder window 200, r ∈ {0.2, 0.29,
    0.34, 0.5}, σ² ∈ {1, 2, 3.76}, ε 0.048, D at b(Realtime). This gives
    R = ε·(1 − P(D)).
  - **r_sat:** A\* = W = 201, the nominal c1/c2/c3 (ε, σ²), r on
    `r_saturation`'s grid. The p99 model's tail_fec reads 1 − P(D).
- **Cross-checks:**
  - P(∞) must match the Python sliding bound (`pfec_review.py`
    `sliding_decode` with the same spacing) within SE, once its spacing is
    switched to the taper shape.
  - P(D) ≤ P(∞) must hold on every row.
- **Sample size:**
  - The body needs about 10³ holes per row (SE about 0.015).
  - The Auto tail at W 201 needs about **10⁵ holes**: t/ε ≈ 2–4e-4, and
    resolving that to ±30 % takes ~10⁵. That is ≈ 4·10⁶ sources at c2, a
    few seconds in Rust.
  - Realtime (t/ε ≈ 2–4e-6) needs ~10⁷ holes. It is **out of reach by
    counting** and needs the derived law plus a bound, not a count.
- **Registration:** before the run, freeze per-form predictions from the §8.3
  table (shipped, Φ, C6, C5, sliding bound) in the test file. The harness then
  becomes the bounding test the sliding law will be held to.

### 8.3 The bracket: every candidate against the sliding decode

Sources: `pfec-continuous/pfec_bracket.py` → `pfec_bracket.out` (T1–T4), and
`pfec_delta.py` → `pfec_delta.out` (D1: the Δ sweep; D2: the rows below r_c).

**Premises of the bound.**
- The sliding decode has no deadline and decodes by generic rank. That makes
  it an **upper** bound on per-loss FEC recovery, for its spacing (uniform
  debt accrual) and its channel (one GE chain over sources and repairs).
- The bound value per cell is the larger of Δ ∈ {1, 2}: the 3-seed mean, with
  its SE. At the two checked cells it keeps rising with Δ, so those rows give
  the Δ ≤ 8 value (`pfec_delta.out` D1).
- **Verdict:** a form is **refuted** when P > bound + 3 SE. Below that it is
  **not ordered**, because no lower bound exists.
- The binomial 95 % limits in the tail rows are optimistic, because holes
  cluster in bursts. A real limit is wider.

| caller window | inputs | shipped / Φ | C6 | C5 (exact joint, block) | sliding bound | verdict |
|---|---|---|---|---|---|---|
| **A\* 3–7, body** (shed budget) | ε .048, σ² 3.76, r 0.2–0.5, 20 cells | 0.72–0.995 | 0.18–0.70 | 0.14–0.65 | 0.17–0.86 (Δ ≤ 2) | shipped/Φ **refuted at 20/20**; C6 **at the bound** at A\* 3, r 0.2 (0.180 against 0.168 + 0.007; a 3-seed SE, with C5 below), not ordered elsewhere; C5 not ordered |
| A\* 4, r 0.34, **engine pin** | as above | **0.899** (R 0.49 %) | 0.357 | 0.305 | **0.416** at Δ 2; 0.458 (SE 0.002) at Δ 8; loss-free repair path 0.399 | shipped **refuted, budget-starving side**: P ≤ 0.46 ⇒ R ≥ 2.6 % against 0.49 %, under the premises |
| A\* 7, r 0.5 | as above | 0.995 | 0.696 | 0.654 | 0.855 at Δ 2; 0.940 (SE 0.004) at Δ 8; loss-free repair path 0.889 | shipped refuted at every Δ ≤ 8 and with placement |
| A\* 4, **r = 0.5 r_c** (0.025) | as above | **0** / 0.454 | 0.026 | 0.021 | 0.025 (0.002) | Φ refuted; shipped and C6 not ordered |
| W 201, **r = 0.5 r_c** | c2 r 0.013 / c3 r 0.025 | **0** / 0.252, 0.208 | 0.102 / 0.082 | 0.103 / 0.084 | 0.086 (0.010) / 0.050 (0.001) | Φ refuted at both; **C6 and C5 refuted at c3**; not ordered at c2 |
| **W 201, body** (r_sat) | c2 at 1.5 / 2 r_c | 0.748 / 0.908 | 0.580 / 0.770 | 0.579 / 0.768 | 0.753 (SE 0.029) / 0.943 (0.011) | all not ordered |
| W 201, body | c3 at 1.5 / 2 r_c | 0.791 / 0.947 | 0.650 / 0.850 | 0.647 / 0.844 | 0.869 (0.013) / 0.987 (0.009) | all not ordered |
| **W 201, tail** (miss 1 − P), Auto r\* | c2 r 0.091 | 4.7e-4 | 2.0e-2 | 2.2e-2 | miss ≥ 0/4 101 (95 % upper 9.0e-4) | target 3.9e-4 unresolved; no form refuted |
| W 201, tail | c3 r 0.158 | 2.7e-4 | 1.1e-2 | 1.3e-2 | miss ≥ 2/7 472 (95 % upper 9.7e-4) | target 2.1e-4 unresolved; no form refuted |

**Reading.**
- **The established statements, all under the bound's premises:**
  - the shipped and Φ forms overstate recovery at the shed budget's windows
    above r_c, which under-states R and starves ρ < 1 shedding;
  - Φ overstates recovery below r_c at every window;
  - the block law (C6, C5) overstates it at W 201 below r_c at c3.
- **The one executed datum sits on the shipped side of the bound.** Its
  decode share 0.71–0.84 (§7 R1b) is above the bound at σ² 3.76 at every
  r ≤ 0.34. So either the datum is not P_fec (the attribution counts decode
  wins in an ARQ race, with unrecorded inputs), or a premise of the bound
  fails (spacing, chain, Δ > 8). The §8.2 oracle separates the two.
- The placement check (T2) does not close the gap: repairs on a loss-free
  path still give 0.399 at the pin.
- The spacing premise is open. `TaperBudget` front-loads repairs after an
  ack, and the §8.2 oracle runs that law.
- **Retracted** from rounds 4–5: "C6 reads P low", "the unsafe direction", "the
  shipped Φ is nearer", "r_sat is overstated", and "the shipped r\* meets the
  Auto target". Each compared a form with a one-sided bound as if it were the
  truth. Main's status erratum (`7c5872f5`, "its tail is ≈ 100× off", "the
  sliding check has the shipped r\* meeting the Auto target") carries the same
  over-reading and needs the same correction.

### 8.4 Evaluation-scheme requirement carried to the sliding law

Any future law at the `r_saturation` caller must meet the §2 scheme:
- computed once per `RateSnapshot`;
- cached by **age only**, with no quantised key;
- a declared resource bound T_sat = max(rate period, cost/budget), stated as
  a second bound next to `RATE_CADENCE_MAX_US`, with the `rce=` gauge.

Its cost must be stated at W 201 against the measured 2.2–93 µs per
evaluation.
