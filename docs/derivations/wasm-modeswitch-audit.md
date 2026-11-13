# Mode-switch audit of the L0 model (before ADR-0076 phase 6)

Question: which sites in `raptorpath-wasm/src/lib.rs` and
`raptorpath-visualizer/` select behaviour by hint, by a hint-derived boolean,
or by a threshold on δ or ρ, and which of them break the no-mode-switch
invariant (`CLAUDE.md`)? Phase 6 moves closed-form laws into
`raptorpath-math`, so every such site next to a moving formula has to be
classified first.

Base: `github/main` at `2d3b2069`. No code changed. Line numbers refer to that
commit.

## Method and evidence

- Sites: every match of `hint_bulk|legacy_|fixed_r`, the `hint` string
  `match`/`==`, every `bulkness`/β guard, every `rho`/`delta`/`dp`/`beta`
  comparison in `lib.rs` and in the `<script>` of
  `interactive-visualizer.html`, and `match`/booleans on hints in the two
  `.mjs` gates. The gates hold no model sites: `runSim` (:64–87) only
  constructs a hint and steps it, and the only hint comparison is the gate's
  own assertion `s.hint === "continuum"` (`test_visualizer_ui.mjs` :226).
- The ADR's count of "27 sites" is the number of **lines** that match
  `hint_bulk|legacy_|fixed_r`. I confirm 27: 23 outside tests and 4 inside.
  Those lines include field declarations, constructor parameters, a doc
  comment and the struct initializer. They come down to 9 decision sites
  (W2, W3, W5, W6, W9, W10, W13, W16, W18); the other lines are declarations
  and plumbing. The regex misses the site at :1044, the β guards (:735, :883,
  :1676, :1689), `rho < 1.0` (:1401) and every JS site. It catches the
  `match` at :1022 and the `==` at :1036 only because those lines also name
  `hint_bulk`/`fixed_r`.
- Probes (`node`, read-only) ran on the **committed** `interactive-visualizer.html`,
  which embeds the wasm32 build. It was last built at `8f3fd7c5`
  (2026-09-27). The later commits that touch `lib.rs`/`raptorpath-math` are
  test-only, plus one bit-identical memoization (`b7b1da08`). The embedded
  build reproduces all six golden rows of `test_multipath_n1_identical_golden`
  bit for bit, so it is the current model era. The native build was not run
  (the brief allows no cargo).
- Reachability tags: **UI** means the visualizer page can reach the site
  (`getParams` always passes `hint = 'continuum'`). **ABI** means it is reachable
  only by calling the wasm constructor directly (the native tests, the golden
  test, `test_visualizer.mjs` §1–§5). **native** means a field that is never
  exposed to JS (`legacy_*`).

## Site table

### `raptorpath-wasm/src/lib.rs`

| id | line | expression | selects | reach | class |
|---|---|---|---|---|---|
| W1 | 247–261 | `if x >= x_rt … else if x >= x_auto … else if x >= x_bulk … else` in `sim_tail_target_of_delta` | the segment of the anchor-tail interpolation | UI | **A**: log t(δ) is piecewise linear in log δ through (50, 1e-7), (0.5, 1e-5), (0.005, 0.05), and constant outside [0.005, 50]. The segment values agree at the knots, so it is continuous, with a slope kink at Auto (−1 above, −1.85 below). The slider range is exactly [0.005, 50], so the end clamps never bind from the UI. |
| W21 | 179–220 | `span_deadline_d`, `span_width_a_star`, `pipeline_depth_m_star`, `trailing_offset_delta`, `shed_budget_residual` | — | UI | **A**: pure functions with clamps only, and no δ or ρ branch. |
| W2 | 1022–1035 | `match hint.as_str() { "bulk" => (1e-3, true, 0.0), "realtime" => (1e-7, false, 0.0), "custom" => (custom_delta, false, 0.0), "continuum" => (t(δ), false, β(δ)), _ => (1e-5, false, 0.0) }` | constructor arguments `tail_target`, `hint_bulk`, `bulkness` | ABI (UI: the `"continuum"` arm only) | **B** (structural). The archetype the invariant forbids: a hint string chooses the constructor arguments. Each named arm represents a dial point a second way: "bulk" is `β = 0 ∧ hint_bulk`, not `β = 1`. |
| W3 | 1036 | `if hint == "fixed" { Some(fixed_r.unwrap_or(0.1)) }` | the r-override arm | ABI | **D**: see "fixed r" below. |
| W4 | 1044 | `rho = if hint == "custom" \|\| hint == "bulk" \|\| hint == "continuum" { custom_rho.clamp(0.9,1) } else { 1.0 }` | whether the ρ dial is honoured | ABI | **B**: auto, realtime and fixed silently drop `custom_rho`. |
| W15 | 1045 | `.clamp(0.9, 1.0)` | ρ domain | UI | **A**: the domain equals the slider's domain (`sl-rhoc` min 0.90, max 1.0), so the bind fraction on the UI path is 0. |
| W5 | 666 | `completion_chi`: `if !self.hint_bulk \|\| self.legacy_bulk_delta { return 0.0 }` | χ fed to the classic rate path | ABI / native | **B** for the `hint_bulk` term: χ is gated by the hint instead of weighted by β. The `legacy_bulk_delta` term is test-only (C). |
| W6 | 706 | `completion_debt_increment`: `if self.hint_bulk \|\| self.legacy_tail_burst \|\| self.fixed_r.is_some() { return 0.0 }` | whether the source-position completion ramp runs | ABI / native | **B** for the `hint_bulk` term. The continuum gets the same zero from `·(1−β)` at β = 1 (W7). `legacy_tail_burst` is test-only (C). `fixed_r` is D. |
| W7 | 725 | `r_tail * w * dchi * (1.0 - self.bulkness)` | completion budget cross-fade | UI | **A**: Δdebt = r_tail·W·dχ_trunc·(1−β), with χ_trunc = Φ̄((rem − W/2)/(W/4)). |
| W8 | 735 | `continuum_tail`: `if self.bulkness <= 0.0 { return self.tail_target }` | fast path of the tail-domain blend | UI (display) | **C**: the only caller is `get_delta_eff` (:1678), which feeds the `δ_eff` readout. The value is the same on both sides, since 10^(log t) = t after the clamp. See the headline finding below. |
| W9 | 784–788 | `rate_inputs`: `if legacy_bulk_delta && hint_bulk { (min(0.1, p̂), false) } else { (tail_target, hint_bulk) }` | **which rate law runs**: `bulk_late_is_fine = hint_bulk` | ABI / native | **B**: exactly the `bulk_late_is_fine = hint == Bulk` selection that paper §4.5 names as the retired mode switch. The legacy arm is test-only (C). |
| W10 | 866 | `controller_rate_now`: `if let Some(r) = self.fixed_r { return r }` | the controller bypass | ABI | **D** (fixed r). |
| W11 | 883 | `rate_now`: `if self.bulkness > 0.0 { mix } else { controller_rate(rate_inputs()) }` | a β = 0 fast path, and the router into W9 | UI + ABI | **B** (structural). §4.5: "There is no `if β == 0` fast path; that would be the mode switch again", and the engine's `RateSnapshot::rate` has none. On the continuum the value step is zero (W9 with `hint_bulk = false` is exactly the anchor term), and §7d3 of `test_visualizer.mjs` gates this at Auto ±2 %. The guard's real effect is to let `β = 0 ∧ hint_bulk` run a different law. |
| W12 | 896 | `(1.0 - self.bulkness) * anchor + self.bulkness * bulk` | — | UI | **A**: r(β) = (1−β)·r_anchor + β·r_late, both terms computed. |
| W20 | 892 | bulk term's `tail_target = (BASE·100)` | — | UI | **A**: not read by `controller_rate` when `bulk_late_is_fine` (math :741). |
| W19 | 111 | JS export `controller_rate(…, bulk_late_is_fine: bool, …)` | which of the two mix terms | ABI (the HTML never calls it; `test_visualizer.mjs` §7 does) | **A**: a term selector for the two always-computed terms. For phase 6, prefer exporting a mix function (see order item 4). |
| W13 | 1386–1394 | `legacy_burst = (legacy_bulk_delta && hint_bulk) \|\| (legacy_tail_burst && !hint_bulk)` → one-shot tail burst | ablation arm | native | **C** (test-only counterfactual, never exposed to JS). Keep it out of `raptorpath-math`; move both `legacy_*` fields behind `#[cfg(test)]`. |
| W14 | 1401–1409 | `if self.rho < 1.0 { t_cut = find_t_cut(…); cut = max(t_cut, 2·RTT) … }` | T_cut give-up horizon | UI | **B**: see F4. The guard alone is redundant (`find_t_cut` returns ∞ at ρ ≥ 1); the step is in the law behind it, at a non-preset ρ*. |
| W16 | 1673–1681 | `get_delta_eff`: `if hint_bulk {…} else if bulkness > 0 { continuum_tail } else { tail_target }` | readout | UI (display) | **C**. For "bulk" it differs from continuum@0.005 only by the clamp to [1e-9, 0.99]. |
| W17 | 1689 | `get_completion_exposure`: `if bulkness > 0 { χ_raw } else { completion_chi() }` | accessor | ABI (the HTML does not read it) | **C**. |
| W18 | 1727 | `get_saturation_pressure`: `if fixed_r.is_some() { return 0.0 }` | readout | UI (display) | **C** (follows the fixed-r decision). |

### `raptorpath-visualizer/interactive-visualizer.html`

| id | line | expression | selects | reach | class |
|---|---|---|---|---|---|
| V1 | 1705 | `const fixedR = params.hint === 'fixed' ? params.fixedR : undefined` | constructor argument | none (hint is always `'continuum'`) | **B** (latent): the banned form verbatim. It is redundant, because wasm ignores `fixed_r` unless the hint is `"fixed"`. Fix: pass `params.fixedR`. |
| V2 | 1987 | `const hint = 'continuum'` | — | UI | **A**: one hint at every dial position (`test_visualizer_ui.mjs` gate). |
| V3 | 2020–2023 | `p.W = derive_window(hintDelta(p) = t(δ), …)` | W* when the W slider is at Auto | UI | **A**: continuous in δ through t(δ). It reads the anchor tail, not the β-mixed operating point (documented). |
| V4 | 2059, 2634 | `rStar = (1 − β)·r*(ε, σ², W, t(δ))` | the panel and cartoon rate | UI | **A**: the mix at the true ε, where the late-is-fine term is 0 mid-stream. |
| V5 | 2643 | `budget = userRho < 1 ? (1 − userRho) : derived` | **which shed-budget law the cartoon runs** | UI | **B**. |
| V6 | 2644 (read at 2691) | `retain = userRho >= 1 && rStar <= 1e-6`; `if (age > law.D && !law.retain)` | whether the shed law runs at all | UI | **B**. |
| V14 | 2951–2955 | preset buttons: `slDprice.value = log10(v)` | — | UI | **A**: named points set the dial; no other state. |
| V7 | 1957–1963 | `deltaNote`: `dp >= 45`, `> 1.2`, `[0.4, 0.65]`, `> 0.008` | caption text | UI | **C**. |
| V8 | 2061–2067 | `userEvict = ρ < 1`, `rHere <= 1e-6` | contract text | UI | **C** (it mirrors V6's threshold in prose; it follows V6's fix). |
| V9 | 2069–2071 | `β >= 0.999`, `β > 0` | δ_FEC text | UI | **C**. |
| V10 | 2077–2080, 2338 | `\|log10(dp/v)\| < 0.02` | preset highlight | UI | **C**. |
| V11 | 2531 | `if (p.customRho < 1)` | ρ readout visibility | UI | **C**. |
| V12 | 2428–2429 | `deltaEff > 0`, `rLive <= 0 → "(pure ARQ)"` | text | UI | **C**. |
| V13 | 2779–2805, 2859–2870 | `law.retain ? … : …`, `law.userRho < 1 ? …` | cartoon styling and budget text | UI | **C** (follows V6). |
| V15 | 2051 | `dp >= 1 ? toFixed(1) : toFixed(3)` | number format | UI | **C**. |

**Counts.** A: 11 (W1, W7, W12, W15, W19, W20, W21, V2, V3, V4, V14).
B: 10 (W2, W4, W5, W6, W9, W11, W14, V1, V5, V6). C: 13 (W8, W13, W16, W17, W18, V7–V13, V15).
D: 2 (W3, W10). W5, W6 and W9 are classed by their `hint_bulk` term;
their `legacy_*` terms are test-only C, and the `fixed_r` term of W6 is D.

## Headline finding: `continuum_tail` is not an executed law

ADR-0076 phase 6 lists "the continuum tail blend (`continuum_tail`, :734)" as a
law to move. The comments at :408–411 and :1017–1019 describe the per-tick
effective tail as that log-blend. The sim does not run it:

- the rate is the rate-domain mix (W12);
- `continuum_tail` is read only by `get_delta_eff` (the `δ_eff` readout);
- `rate_now`'s own comment (:879–881) says a tail-domain blend would step at
  the preset, because r*(δ) is near-binary against the cold-start ε̂.

So moving it into `raptorpath-math` would put a display formula into the shared
law crate as if it were a law, and that formula is the one the model rejected.
Recommendation: drop it from the phase 6 list. Correct the two comments. Either
delete `get_delta_eff`'s blend or label the readout as "the tail the anchor
term would need". No δ_eff exists for a rate-domain mix.

## The B sites and their continuous replacements

### F1: one constructor path (W2, W5, W6, W9, W11; and W16/W17 follow)

The step. "bulk" and continuum@0.005 are the same dial point, built two ways
(`β = 0 ∧ hint_bulk` against `β = 1`). Likewise "auto"/"realtime" against
continuum@0.5/50. All 8 fingerprint fields agree at all six golden cells
(wasm32). Model-output step: **zero, measured** on the bulk rows. On the
auto/realtime rows it rests on algebra (see below):

| hint, ε, q, RTT | classic = continuum@δ(hint) | path the continuum arm ran |
|---|---|---|
| auto .05/.5/50 | [667,2000,668,0,101,2000,2668,2532] both | :883 fast path (β = 0) |
| auto .10/.3/80 | [747,2000,985,0,185,2000,2985,2705] both | fast path |
| bulk .05/.5/50 | [569,2000,0,116,113,2000,2116,2000] both | **the mix** (β = 1) |
| bulk .10/.3/80 | [647,2000,85,188,198,2000,2273,2059] both | **the mix** |
| realtime .05/.5/50 | [733,2000,710,3,101,2000,2713,2571] both | fast path |
| realtime .10/.3/80 | [754,2000,1015,0,184,2000,3015,2734] both | fast path |

The accessors still differ for "bulk". Measured after 300 steps at
.05/.5/50: `get_r_star_auto` is 0.1512 against 0, and `get_derived_w` is
200 against 26.4, because both read `tail_target` (1e-3 against 0.05).
`get_delta_eff` differs by 1e-17 (its clamp); `get_completion_exposure` is
equal. These are display only. The defect is structural: a second machine is
reachable from the ABI and owns its own law selection (W9).

The formula, one path for every hint:

```text
   δ(hint)  ∈ { bulk 0.005, auto 0.5, realtime 50 }        named points, nothing else
   t = t(δ),  β = β(δ)                                      W1, bulkness_of_delta
   r        = (1−β)·R(t, late=false) + β·R(·, late=true, χ_raw)    always both; delete the :883 guard and the :898 else
   χ input  = χ_raw always                                  delete completion_chi's hint gate; the anchor term does not read χ
   Δdebt    = r + r_tail·W·dχ_trunc·(1−β)                   delete the hint_bulk early return in :706
```

After this, `hint_bulk` is gone. "custom" (tail target given directly, β ≡ 0)
is the anchor term alone. It is the reference arm of
`test_continuum_one_law_across_the_dial` clause 2, and the explainer's δ_FEC
corner. Keep it only as a test-only constructor, or build it from δ = t⁻¹(tail)
where W1 is invertible. **Owner's call.** `legacy_*` moves behind `#[cfg(test)]`.

Golden impact:

- **Bulk rows: measured neutral.** The mix ran.
- **Auto and realtime rows: neutral by the identity 1·a + 0·b = a** (§4.5;
  bit-exact for finite terms, and `controller_rate` clamps). This is not a
  measurement. The mix cannot run at β = 0 exactly today, because the guard
  takes it.
  - Probing at δ = 0.5·(1 − 1e-12) (β = 2.2e-13) does not isolate the mix
    either. The tail also moves by about 2e-12 relative, and `lost` moved
    101 → 103 and 185 → 186. That shows the fingerprint is sensitive below the
    1e-11 level, and nothing about the identity.
- **Ulp sensitivity.** On wasm32, W1 returns `t(0.5) = 9.999999999999999e-6`
  (1 ulp below the classic `1e-5`) and `t(50) = 1e-7` (1 ulp below the classic
  `1e-5·0.01 = 1.0000000000000001e-7`). The continuum arms ran with those
  values and still matched all 8 fields, so the tail ulp is measured harmless at
  these six cells on wasm32.
- **Native is unverified** (libm differs by an ulp in places; `93d37909`). Run
  `test_multipath_n1_identical_golden` **without** `GOLDEN_CAPTURE` after F1.
  If a row moves, make W1 return the classic literals at the three anchors (no
  golden pins the continuum there), or ask the owner for a re-capture.

### F2: ρ composes with every hint (W4)

The step. At the same dial point (δ, ρ = 0.95), "auto"/"realtime" run ρ = 1
(`get_rho()` = 1), while continuum@δ runs 0.95. At the golden cells nothing is
ever given up, so there the step is routing-only. On harsher cells it is a
behaviour step:

| cell ε/q/RTT | realtime, ρ .95 → given_up / ticks | continuum@50, ρ .95 |
|---|---|---|
| .15/.2/80 | 0 / 778 | 0 / 778 |
| .20/.1/100 | 0 / 895 | **28 / 823** |
| .25/.1/100 | 0 / 991 | **68 / 884** |
| .30/.05/150 | 0 / 1111 | **179 / 1097** |

"auto" against continuum@0.5 gives the same numbers.

```text
   ρ = clamp(custom_rho ?? 1, 0.9, 1)     for every hint
```

Golden impact: none (the golden rows pass `None`).

### F3: span-cartoon shed budget (V5, V6)

This feeds `spanStep`'s shed decision, so it is model, not presentation.

**Root cause.** `derived = ε̂·(1 − P_fec(r))` is the residual past in-window
FEC. It is largest, equal to ε̂, at r = 0, which is the Bulk end. That is
exactly where the cartoon declares the law "compiled out", so the `retain` flag
(V6) has to cut a formula that is maximal there. The cut is the step.

The shape at ρ = 1 (default cell q .5, RTT 50, cap 4, W auto), shipped code:

| δ | .005 | .0055 | .006 | .0075 | .01 | .02 | .05 | .1 | .2 | .5 | 5 | 50 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ε 5 % | 0 | 0 | **5e-2** | 5e-2 | 5e-2 | 5e-2 | 2.3e-2 | 7.5e-3 | 1.1e-3 | 1.9e-5 | 2.7e-5 | 5.1e-5 |
| ε 10 % | 0 | **1e-1** | 1e-1 | 1e-1 | 1e-1 | 1e-1 | 1e-1 | 2.0e-2 | 2.5e-3 | 3.8e-5 | 6.1e-5 | 1.25e-4 |

There are two steps:

- **In δ at ρ = 1**, where `rStar` crosses 1e-6. That is not a preset, so the
  preset-nudge gates miss it. The budget jumps from 0 to ε̂, at δ* = 0.005615
  (ε 5 %), 0.008207 (ε 2.5 %) and 0.0050002 (ε 10 %, i.e. at the Bulk preset).
  On a 2 % grid over the whole dial, the largest jump between neighbours is
  5e-2 (ε 5 %) and 1e-1 (ε 10 %).
- **In ρ at ρ = 1**: lim ρ→1⁻ of the budget is (1−ρ) → 0, but at ρ = 1 it is
  `derived` (1.93e-5 at Auto, ε 5 %).

Candidates:

```text
   (a)  budget = max( 1−ρ , (1−β)·ε̂·(1 − P_fec(r_mix, ε̂, A*, σ²)) )
   (b)  budget = min( 1−ρ , ε̂·(1 − P_fec(r_mix, ε̂, A*, σ²)) )      ← proposed
   in both, retain := (budget = 0); the separate flag goes
```

Shape first:

- (a) is continuous but **not monotone in δ** at ρ = 1. At ε 5 % it runs
  0 (Bulk) → 7.5e-3 (δ .01) → 1.5e-2 (δ .02) → 1.9e-5 (Auto) → 5.1e-5
  (Realtime). Its largest 2 %-neighbour jump is 1.2e-2 (ε 5 %) and 2.7e-2
  (ε 10 %), i.e. 0.24·ε̂. The (1−β) weight has no provenance.
- (b) is continuous in both dials and monotone in ρ. At ρ = 1 it is
  identically 0 at every δ (2 %-grid jump 0). Below 1 it is capped by the
  declared contract 1−ρ.

| corner | shipped | (a) | (b) |
|---|---|---|---|
| Bulk, ρ = 1 | 0 | 0 | 0 |
| Bulk, ρ = .95 | 5e-2 | 5e-2 | 5e-2 |
| Bulk·1.02, ρ = 1, ε 10 % | **1e-1** | 4.3e-4 | 0 |
| Realtime, ρ = 1 (ε 5 % / 10 %) | 5.1e-5 / 1.25e-4 | same | **0** |
| Realtime, ρ = .9999 (ε 10 %) | 1e-4 | **1.25e-4** | 1e-4 |
| Realtime, ρ = .95 | 5e-2 | 5e-2 | **5.1e-5 / 1.25e-4** |

Proposed: **(b)**. It is §5.6's own law made continuous: the retain (ρ = 1)
seat never sheds, the evicting seat sheds at most the derived residual, and the
declared contract caps it.

- It **fails `test_visualizer_ui.mjs` :181** outright. That gate asserts
  `!lawRt.retain` at ρ = 1, which pins the cartoon's present non-§5.6 reading
  ("ρ = 1 target — sheds only the derived residual"). The gate would have to
  change with the fix.
- It changes the Realtime ρ < 1 budget from 1−ρ to the derived residual, so
  :205 ("sheds counted at rt-end with ρ < 1") must be re-checked.

The cap by 1−ρ needs its line in §5.6 before the code (formula-first).
**Owner's call.** Golden impact: none (JS only).

### F4: the T_cut law (W14)

The law: `t_cut = find_t_cut(ε̂, q, max(r, 1e-3), W, σ², ρ)`, then
`cut = max(t_cut, 2·RTT)`. `find_t_cut` bisects on
`p_recovered_within(t) = 1 − ε(1−P_fec)(1 − min(1, r(1−(1−q)^(t+1))(1−ε)/ε))`
over [0, 10·W] and returns ∞ at ρ ≥ 1.

`p_recovered_within` models ARQ progress with the FEC rate r. At r ≈ 0 (Bulk)
its supremum is about 1 − ε̂(1−P_fec), and for ρ above that, the bisection
returns its cap. So the law is two-valued:

- at ε .05, W 64, r = 1e-3: t_cut ≈ 0.01 ticks for ρ ≤ 0.95, where the 2·RTT
  floor binds; 640 = 10·W for ρ ≥ 0.99, where the search cap binds; ∞ at ρ = 1;
- at r = 0.3: ≈ 0.01 for every ρ < 1, where the floor binds.

**Every one of the ten measured ρ < 1 points binds one of the two clamps.**
That is a law pinned over its operating range (MEASUREMENT DISCIPLINE 17/18),
a defect finding for the coordinator's ledger.

The behaviour step sits at a non-preset ρ*, bisected to 1e-6 on
continuum@0.005:

| cell | ρ* | given_up just below → just above ρ* |
|---|---|---|
| .05/.5/50 | 0.983332 | 2 → 0 |
| .10/.3/80 | 0.988175 | 6 → 0 |

At ρ* the give-up horizon jumps from 2·RTT to 10·W. Elsewhere `given_up` is
flat in ρ: on .05/.5/50 it is 3 for ρ .9 to .95 and 0 for ρ .99 to 1. The Auto
and Realtime cells give up 0 at every ρ, so those cells hide the law.

Proposed continuous law (needs a §4.10 derivation): model ARQ as retransmit
rounds, not as r.

```text
   u(t)    = ε̂·(1 − P_fec)·ε̂_retx^(t/RTT)                      P(still unrecovered at age t)
   T_cut(ρ) = RTT · ln( (1−ρ) / (ε̂(1−P_fec)) ) / ln ε̂_retx ,    clamped at 0
```

This is continuous and monotone in ρ, and diverges (logarithmically) as ρ → 1,
so the `rho < 1.0` guard becomes the limit, not a branch. It lives in
`raptorpath-math` (`find_t_cut`/`p_recovered_within`), so it is an engine-crate
change, governed by ADR-0076's amended scope rule. Golden impact: none (rows at
ρ = 1). `test_custom_rho_gives_up_late_losses` and `test_bulk_composes_with_rho`
may move.

### V1

Delete the ternary (see the table). No golden impact.

## D sites: what settles them

**Fixed r (W3, W10, and the `fixed_r` terms of W6/W18).** This is an r-override
off the (δ, ρ) dials: the triangle's r corner, declared, with the controller and
completion ramp bypassed. The UI never reaches it. It is used by
`test_visualizer.mjs` §5, `test_fixed_r_mode`, `test_bulk_beats_fixed_001` and
`test_fixed_r_sets_realized_rate`. What settles it is the owner's word: is the
r corner a model feature (then it needs a declared-r dial that composes with δ
and ρ), or a comparator (then a test-only constructor, class C)? Do not carry it
into `raptorpath-math` either way.

## Coverage by the existing gates

Existing gates:

- `test_continuum_one_law_across_the_dial`:
  - β = 1: settled r ≤ bulk-ref + 1e-4 on ticks [100, 300).
  - β = 0: ticks and FEC equal to "custom".
  - Bulk 5 % nudge: |ΔFEC@300| ≤ 10.
  - continuum@0.005 with ρ = .95 composes.
- `test_visualizer.mjs` §7d3:
  - Law ±2 % continuity and monotonicity of t, β, b, ζ at the presets.
  - Sim at Bulk ±5 %.
  - Sim at Auto ±2 % (the W11 guard).
- `test_visualizer_ui.mjs`: one hint at every dial position, including ρ < 1.

| B site | covered by | uncovered: the absolute test that would catch it |
|---|---|---|
| W11 | 7d3 Auto ±2 % (value only) | structural only; F1's test below makes it observable |
| W2/W5/W6/W9 | nothing: the gates exercise only the continuum arm | **named point = dial point**: for h ∈ {auto, bulk, realtime} × the six golden cells, after N ∈ {300, end} steps, every public accessor of `Simulation(h)` equals the one of `Simulation("continuum", δ(h))`. That covers the 8 fingerprint fields plus `get_delta_eff`, `get_r_star_auto`, `get_derived_w` and `get_completion_exposure`. Red today on the "bulk" accessors (tail 1e-3 vs 0.05). |
| W4 | `test_bulk_composes_with_rho` (bulk only) | anchor cell ε .20, q .1, RTT 100, ρ = 0.95. For every hint, `get_rho() == 0.95` and the fingerprint plus `get_given_up` equal continuum@δ(h) at the same ρ. Red today: realtime gives up 0 against 28, ticks 895 against 823. At the golden cells only `get_rho` is red (nothing is given up there). |
| V5/V6 | :181 and :233 pin the two sides, not continuity | a 2 % grid over the whole dial (δ = 0.005·1.02^k up to 50) at ε ∈ {2.5, 5, 10} %, ρ ∈ {1, 0.9999, 0.95}: every pair of neighbours has \|Δbudget\| ≤ 0.05·ε̂. Today the largest is ε̂ (5e-2 / 1e-1), so it fails. Also \|budget(ρ = 1) − budget(ρ = 1 − 1e-9)\| ≤ 1e-6 at each preset (today 1.9e-5 at Auto, so it fails). A grid is needed, not a point, because the step is not at a preset and δ* moves with ε. Assert on `spanLaw()` through the stub DOM. |
| W14 | nothing (`test_custom_rho_*` and `test_bulk_composes_with_rho` check a single ρ) | an absolute law test on `find_t_cut` at fixed inputs (ε .05, q .5, r ∈ {1e-3, 0.3}, W 64): on a geometric grid of 1−ρ ∈ [1e-6, 0.1] with ratio 1.02, t_cut is non-decreasing in ρ and \|ln t_cut(next) − ln t_cut(this)\| ≤ 0.1. Today the jump 0.01 → 640 (ln ratio 11) fails. Add a bind-fraction gauge of the 2·RTT floor and the 10·W cap over the same grid; today it binds at 10/10 points. At sim level: bisect ρ* on continuum@0.005 at .05/.5/50, then require \|Δgiven_up\| ≤ 1 over ±2 % of 1−ρ* (today 2 → 0 within 1e-6). |
| V1 | nothing | a UI-gate grep: no `hint ===` in the script block |

A weakness of the gates themselves: §7d3's law tolerances (0.2 decades for t,
0.05 for β, at ±2 %) are about 6× the continuous change. The largest t slope is
1.85 decades per decade of δ, so ±2 % moves t by 0.032 decades; for β it is
0.0086. A step of up to about 1.47× in t would pass. Suggested tightening: allow
1.5× the analytic slope times the nudge width.

## Golden impact summary

| fix | moves `test_multipath_n1_identical_golden`? |
|---|---|
| F1 | bulk rows: measured no (the mix ran). Auto and realtime rows: no by the identity 1·a + 0·b = a, which is not measured. The tail ulps (t(0.5), t(50)) are measured harmless on wasm32. Native is unverified: run the test without capture first. A re-capture needs the owner's word. |
| F2 | no (the rows pass ρ = None) |
| F3, V1 | no (JS only) |
| legacy → `#[cfg(test)]` | no |
| F4 (T_cut law) | no (rows run at ρ = 1); `test_custom_rho_*` / `test_bulk_composes_with_rho` may move |

ADR-0076 already plans a deliberate re-capture for phase 6. F1 should land
before it and separately, so that the re-capture records only the moved laws.

Probe scripts (not committed): `probe.mjs`, `probe2.mjs`, `probe3.mjs` and
`probe4.mjs` in the auditor's scratch directory. Each one reads the committed
HTML and evaluates its engine block the way `test_visualizer.mjs` does.

## Fix order, by what phase 6 moves first

1. **The five closed forms** (`span_deadline_d`, `span_width_a_star`,
   `pipeline_depth_m_star`, `trailing_offset_delta`, `sim_tail_target_of_delta`):
   class A, with no site blocking them. Move them first. Keep the realtime
   1-ulp question (F1) in mind when moving W1.
2. **χ_trunc and the completion budget** (`completion_debt_increment`, :705).
   W6 blocks this. Do F1 first (with β = 1 the `·(1−β)` already gives 0, so the
   `hint_bulk` return is redundant), move `legacy_tail_burst` to test-only, and
   settle fixed r (D). Then the formula moves branch-free.
3. **`continuum_tail`**: do not move it. It is display only (headline finding).
   Fix the comments and drop it from the ADR's phase 6 list.
4. **The rate mix itself** (W11/W12/W9; not in the ADR list yet): move
   `(1−β)·R(late = false) + β·R(late = true)` from the engine's
   `RateSnapshot::rate` into `raptorpath-math`, and call it from both sides.
   That deletes the W11 guard and the bool in the W19 export.
5. **F2** (ρ for every hint): independent of the moves; no golden impact.
6. **F4** (T_cut, W14): the law is already in `raptorpath-math`. It needs a
   §4.10 derivation, and its pinned-clamp finding needs a ledger verdict. It
   does not block the moves.
7. **F3 and V1** (visualizer JS): independent of phase 6. F3 needs its §5.6
   line and a change to gate :181.

## Outside scope (noted, not audited)

§5.6 describes the engine's shed law as running "only on the evicting seat",
with "the retain (ρ = 1) seat never sheds". That is a seat keyed on ρ = 1, so
the engine may have the same ρ step as V5. It should be checked against
`net/shed.rs` under the same invariant.
