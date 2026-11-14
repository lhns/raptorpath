# P_fec, continuous and exactly 0 at r = 0 (W22 / ledger R03)

Derivation only: no code changes. Mandate: `docs/status.md` "Owner decisions
(2026-11-14)" item 2. Numbers: `pfec-continuous/pfec_check.py` →
`pfec_check.out`. Sections of that file are cited as **out§N**; plain §N is
this doc, and paper sections are named "paper §". Base: `db012f16`.

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
3. *Before the final commit:* see the last commit's message.

## 0. Result

Neither the shipped form nor the paper's form is right, for one shared reason:
**§4.2 computes the wrong event.** The paper defines P_fec as "probability a
lost symbol is FEC-recovered" (`fec-arq-model.md:205`), and every use of it
reads it that way: R = ε·(1 − P_fec) (§3.5 `:573–576`, §5.6 `:1723`). But §4.2
(`:693–699`) derives the **window** event P(surviving repairs ≥ K), which
includes K = 0, and then approximates it by a two-moment normal. On the
correct per-loss event (the size-biased or Palm event), the r = 0 pin is a fact
about support: C ≤ N_R = 0 < K. No normal form of C − K can carry that. The
recommended form keeps the paper's assumptions and replaces the normal with the
exact counts:

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
- It changes **no** L1 cell (proof in §4): r* stays bit-identical 0 on c1s,
  c1d, c2 and c8.

## 1. What P_fec must mean

**Coding model as executed.**
- Engine: sliding-window RLC (`FecBackend::Rlc`). Each repair combines the
  last A* source symbols, with A* = clamp(⌈rate·D⌉, 1, encoder.window_size())
  (`net/emit_source.rs:578–580`). The encoder window is ≤ `MAX_WINDOW_SIZE` =
  200 (`net/mod.rs:205`).
- Repairs come from deterministic debt accrual. The engine banks r per source
  symbol (`TaperBudget`, `control/fec_rate.rs:678–703`); the sim does the same
  (`raptorpath-wasm/src/lib.rs:1358`, emitted at `debt ≥ 1`, `:1289`).
- The sim's window slides to its slider W (`:1314`, golden W = 64).

**The event.** Take a tagged source symbol that is lost. It is FEC-recovered
iff, over the W source slots that protect it, the surviving repairs C reach
the window's loss count K (which includes the tagged loss). Losses are
size-biased: a window with K losses holds K tagged losses. So

```text
P_fec = P(recovered | lost) = E[K·1{C ≥ K}] / E[K],     E[K] = W·ε
```

- This is the block-equivalent reading the paper uses (§4.2, §4.7).
- A sliding-window decoder can also use overlapping spans. That makes this a
  model of the decoder, not a bound on it (not verified here, see §6).

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
| **C6** | **recommended (formula in §0)** | **0** | below |

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
- Cost: π_W once per (ε, σ², W) is O(W²). G(n) for all n ∈ [0, W+1] is
  O(W²). Each of `r_saturation`'s 199 points is then O(1). That is about
  8·10⁴ flops at W = 200 per call, and `controller_rate` makes two calls per
  rate.

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
  - So C6 passes a slope-scaled gate. It does not read ~0 under any fixed
    absolute tolerance.
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

**Callers.**

| caller | window | event read | effect of C6 |
|---|---|---|---|
| `r_saturation` `tail_fec` (`raptorpath-math/src/lib.rs:935`) → `controller_rate` soft cap (`:825`), on by default (`control/fec_rate.rs:346`, passed `:515`) | encoder `window_size()` (`fec_rate.rs:499`) | per-loss (cost of a loss that falls to ARQ) | r_sat rises: P_fec is lower, so more FEC is worth more |
| engine shed budget `residual_loss_after_fec` (`fec_rate.rs:784–790`, `net/emit_source.rs:587–598`), only when `shed_on` (ρ < 1; `net/shed.rs:23`) | A* | per-loss | R is higher at every r > 0 (e.g. 0.0049 → 0.0309 at the fixture) |
| `compute_delta`, `p_recovered_within`, `find_t_cut` (`lib.rs:1188`, `:1198`, `:1213`) → sim T_cut (wasm `:1404`) | sim window | per-loss | moves. T_cut (W14) is retired by owner decision 3 |
| wasm `shed_budget_residual` (`:217–219`), the cartoon R, sim `get_r_sat` (`:1697`), sim rate via `controller_rate` (`:844`, `:884–898`) | A* / sim window | per-loss | the cartoon R becomes continuous through δ 0.045 / 0.058 (the W22 rows of the audit) |
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

**No new r_sat staircase (out§19).** I swept ε in 0.1 % relative steps
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
- `RWM_DELTA` is unset, so the dial sits at the Bulk point.
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

So **a guard on these four cells cannot execute the changed law**. It can only
witness a no-op: an identical spare-capped rate trace, or the §41c guard as a
zero-change check. The first measurable cells are off-Bulk (Auto, β = 0, where
the soft cap acts) or ρ < 1 (shed). Neither has been measured
(`law-ledger.md:919`).

## 5. What the code change would touch (not done)

**`raptorpath-math`.**
- `p_fec_normal` (`lib.rs:885–893`) becomes the C6 form under a new name. The
  cut at `:888` goes.
- `r_saturation` (`:917–944`) precomputes G(n) once per call.
- The doc comments at `:883–884` and `:895–916` change.
- Expectations that move:
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
    `temporal_oracle.rs:1504`, `:1743`: re-check.

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
  (`raptorpath-wasm/src/lib.rs:1840–1866`) need `GOLDEN_CAPTURE=1`. The four
  auto/realtime rows move for certain: their rates of 0.33–0.51 sit at or above
  r_sat (0.255/0.42 → 0.315/0.56), so the soft cap binds. Bulk ε .05 (fec 0)
  should not move. Bulk ε .10 (fec 85, a non-zero rate through the soft cap)
  may move.
- `shed_budget_residual` test `:2706–2708` holds (0.05 and 1.4e-4).
- `test_custom_rho_gives_up_late_losses` and `test_bulk_composes_with_rho` may
  move, as the audit says.
- The W22 exclusion in the continuity gate (owner decision 3) is deleted.
- `shed_budget_residual` (`:217–219`) passes the cartoon's fractional A*. It
  takes the real-W extension of §1, and σ² is floored at 1 there.

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
  `min(8.0)`-capped debt (`:1358`).
- **Block-equivalent vs sliding-window decoding.** Whether the overlapping
  spans make the true per-loss recovery higher than C6 is not measured.
- **Whether the engine ever ran on the P_fec > 0 side.** The "consumed r ≈ 0.34
  at ε ≈ 4.8 %" fixture comment (`fec_rate/tests.rs:277–279`) is unsourced.
  §4's proof needs only the Bulk configuration, not this.
- **Estimator inputs for the nominal r_sat rows.** The engine's measured σ², W
  and t_sym per cell are not in the ledgers (ε̂ under-reads 3–5×,
  `docs/status.md` §3.8 :282). The rows are labelled nominal.
- **§4.2's r\* inversion.** It uses the window event and the normal tail. By
  out§7's tail numbers it under-provisions more than §4.2's "30–50 %" note says.
  That is a separate finding for the r* rows of the ledger and §4.3, not part of this change.
- **The Rust cost.** About 8·10⁴ flops per `r_saturation` at W 200 is an
  operation count, not a bench.
