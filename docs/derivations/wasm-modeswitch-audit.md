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
  `.mjs` gates (they hold no model sites; they only construct hints).
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
| W14 | 1401 | `if self.rho < 1.0 { t_cut = find_t_cut(…); cut = max(t_cut, 2·RTT) … }` | T_cut give-up | UI | **D**: see below. |
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
| V10 | 2077–2080, 2338 | `|log10(dp/v)| < 0.02` | preset highlight | UI | **C**. |
| V11 | 2531 | `if (p.customRho < 1)` | ρ readout visibility | UI | **C**. |
| V12 | 2428–2429 | `deltaEff > 0`, `rLive <= 0 → "(pure ARQ)"` | text | UI | **C**. |
| V13 | 2779–2805, 2859–2870 | `law.retain ? … : …`, `law.userRho < 1 ? …` | cartoon styling and budget text | UI | **C** (follows V6). |
| V15 | 2051 | `dp >= 1 ? toFixed(1) : toFixed(3)` | number format | UI | **C**. |

**Counts.** A: 11 (W1, W7, W12, W15, W19, W20, W21, V2, V3, V4, V14).
B: 9 (W2, W4, W5, W6, W9, W11, V1, V5, V6). C: 13 (W8, W13, W16, W17, W18, V7–V13, V15).
D: 3 (W3, W10, W14). W5, W6 and W9 are classed by their `hint_bulk` term;
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
continuum@0.5/50. The measured step on the model outputs is **zero**: all 8
fingerprint fields agree at all six golden cells (wasm32):

| hint, ε, q, RTT | classic = continuum@δ(hint) |
|---|---|
| auto .05/.5/50 | [667,2000,668,0,101,2000,2668,2532] both |
| auto .10/.3/80 | [747,2000,985,0,185,2000,2985,2705] both |
| bulk .05/.5/50 | [569,2000,0,116,113,2000,2116,2000] both |
| bulk .10/.3/80 | [647,2000,85,188,198,2000,2273,2059] both |
| realtime .05/.5/50 | [733,2000,710,3,101,2000,2713,2571] both |
| realtime .10/.3/80 | [754,2000,1015,0,184,2000,3015,2734] both |

The accessors still differ for "bulk": `get_r_star_auto` and `get_derived_w`
read `tail_target` (1e-3 against 0.05), and `get_delta_eff` differs only by its
clamp. These are display only. The defect is structural: a second machine is
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

Golden impact: the wasm32 measurement says **neutral** for all six rows.
Native is unverified. Realtime is the risk: the classic tail is
`BASE·0.01 = 1.0000000000000001e-7`, and W1 returns `10^−7 = 1e-7`. They differ
by one ulp (checked in Python/IEEE). Run `test_multipath_n1_identical_golden`
**without** `GOLDEN_CAPTURE` after F1. If a realtime row moves, either make W1's
`x >= x_rt` clamp branch return the literal `BASE_TAIL_TARGET * 0.01` (no golden
pins the continuum at δ ≥ 50), or ask the owner for a re-capture.

### F2: ρ composes with every hint (W4)

The step. At the same dial point (δ = 50, ρ = 0.95), "realtime" runs ρ = 1
(measured: `get_rho()` = 1; "auto" is also 1, "bulk" is 0.95), while
continuum@50 runs ρ = 0.95.

```text
   ρ = clamp(custom_rho ?? 1, 0.9, 1)     for every hint
```

Golden impact: none (the golden rows pass `None`).

### F3: span-cartoon shed budget (V5, V6)

This feeds `spanStep`'s shed decision, so it is model, not presentation. The
cartoon has two steps:

- **In δ at ρ = 1**, where `rStar` reaches 1e-6. That is not a preset, so the
  preset-nudge gates miss it. The bisected boundary, with the budget jumping from
  0 to ε̂:

  | ε (default cell q .5, RTT 50, cap 4, W auto) | δ* | budget at δ*/1.02 | budget at δ*·1.02 |
  |---|---|---|---|
  | 2.5 % | 0.008207 | 0 | 2.5e-2 |
  | 5 % | 0.005615 | 0 | 5.0e-2 |
  | 10 % | 0.0050002 (the Bulk preset itself) | 0 | 1.0e-1 |

- **In ρ at ρ = 1**: lim ρ→1⁻ of the budget is (1−ρ) → 0, but at ρ = 1 it is
  `derived`. At Auto, ε 5 %, that is 1.93e-5 at ρ = 1 against 1e-4 at
  ρ = 0.9999. Inside the retain region the ρ side is continuous.

Two continuous candidates, evaluated at the four corners (ε = 5 % / 10 %):

```text
   (a)  budget = max( 1−ρ , (1−β)·ε̂·(1 − P_fec(r_mix, ε̂, A*, σ²)) )
   (b)  budget = min( 1−ρ , ε̂·(1 − P_fec(r_mix, ε̂, A*, σ²)) )
   in both, retain := (budget = 0); there is no separate flag
```

| corner | shipped | (a) | (b) |
|---|---|---|---|
| Bulk, ρ = 1 | 0 | 0 | 0 |
| Bulk, ρ = .95 | 5e-2 | 5e-2 | 5e-2 |
| Bulk·1.02, ρ = 1, ε 10 % | **1e-1** | 4.3e-4 | 0 |
| Realtime, ρ = 1 | 5.1e-5 / 1.25e-4 | same | **0** |
| Realtime, ρ = .95 | 5e-2 | 5e-2 | **5.1e-5 / 1.25e-4** |
| Auto, ρ = .9999 | 1e-4 | 1e-4 | 1.9e-5 / 3.8e-5 |

- (a) keeps every current corner and removes both steps. It passes
  `test_visualizer_ui.mjs` :181 (Bulk RETAIN, r* = 0) and :233 (budget 0.05 at
  Bulk, ρ 0.95). Its (1−β) weight has no paper provenance yet.
- (b) is §5.6 read literally: "the retain (ρ = 1) seat never sheds". It zeroes
  Realtime ρ = 1 shedding and makes Realtime ρ < 1 shed only the derived
  residual. That would put the :205 gate ("sheds counted at rt-end with ρ < 1")
  at risk.

Either needs its derivation in the paper before the code (formula-first).
**Owner's call.** Golden impact: none (JS only).

### V1

Delete the ternary (see the table). No golden impact.

## D sites: what settles them

- **Fixed r (W3, W10, and the `fixed_r` terms of W6/W18).** This is an
  r-override off the (δ, ρ) dials: the triangle's r corner, declared, with the
  controller and completion ramp bypassed. The UI never reaches it. It is used
  by `test_visualizer.mjs` §5, `test_fixed_r_mode`, `test_bulk_beats_fixed_001`
  and `test_fixed_r_sets_realized_rate`. What settles it is the owner's word: is
  the r corner a model feature (then it needs a declared-r dial that composes
  with δ and ρ), or a comparator (then a test-only constructor, class C)?
  Do not carry it into `raptorpath-math` either way.
- **T_cut (W14).** The `rho < 1.0` guard alone is redundant: `find_t_cut`
  returns ∞ at ρ ≥ 1. But the law behind it is degenerate. Measured with ε .05,
  W 64:
  - At r = 1e-3 (Bulk) it is two-valued in ρ: t_cut ≈ 0.01 ticks for ρ ≤ 0.95
    (so the `max(·, 2·RTT)` floor binds), 640 = 10·W (the search cap binds) for
    ρ ≥ 0.99, and ∞ at 1.
  - At r = 0.3 it is ≈ 0.01 for every ρ < 1.
  - So `given_up` runs flat across ρ and then steps at a data-dependent ρ*.
    Continuum@0.005, cell .05/.5/50: ρ 1/.999/.99 → 0; .97 → 2; .95/.92/.9 → 3.
    Cell .10/.3/80: 0, 0, 0, then 8 from .97 down. The Auto and Realtime cells
    give up 0 everywhere.

  What settles it: a bind-fraction gauge on both clamps (the 2·RTT floor and the
  10·W cap) over ρ ∈ [0.9, 1) × the presets × the golden cells. If either clamp
  binds over most of that range, MEASUREMENT DISCIPLINE 17/18 treats it as a
  defect finding (law pinned) and it needs a ledger verdict. A plausible cause
  needs checking: `p_recovered_within` models ARQ progress with the FEC rate r,
  so at r ≈ 0 its supremum is ≈ 1 − ε̂ and ρ above that is "unreachable". The
  candidate continuous law is T_cut(ρ) = RTT·ln((1−ρ)/(ε̂(1−P_fec)))/ln ε̂_retx,
  which diverges as ρ → 1 so the guard becomes a limit. It lives in
  `raptorpath-math` and needs §4.10 derivation. This does not block phase 6.

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
| W4 | `test_bulk_composes_with_rho` (bulk only) | for every hint, `Simulation(h, ρ = 0.95).get_rho() == 0.95` and its fingerprint equals continuum@δ(h) at ρ = 0.95. Red today for auto and realtime. |
| V5/V6 | :181 and :233 pin the two sides, not continuity | anchor δ* = 0.005615 (ε 5 %) and the Bulk preset (ε 10 %): \|budget(δ·1.02) − budget(δ/1.02)\| ≤ 0.02·ε̂ at ρ = 1 (today ε̂, so it fails). Then \|budget(ρ = 1) − budget(ρ = 1 − 1e-9)\| ≤ 1e-6 at each preset (today 1.9e-5 at Auto, so it fails). Asserted on `spanLaw()` through the stub DOM. |
| V1 | nothing | a UI-gate grep: no `hint ===` in the script block |

A weakness of the gates themselves: §7d3's law tolerances (0.2 decades for t,
0.05 for β, at ±2 %) are about 6× the continuous change. The largest t slope is
1.85 decades per decade of δ, so ±2 % moves t by 0.032 decades; for β it is
0.0086. A step of up to about 1.47× in t would pass. Suggested tightening: allow
1.5× the analytic slope times the nudge width.

## Golden impact summary

| fix | moves `test_multipath_n1_identical_golden`? |
|---|---|
| F1 | measured no on wasm32 (8/8 fields × 6 rows). Native unverified; realtime has a 1-ulp tail difference, so run the test without capture first. A re-capture needs the owner's word. |
| F2 | no (the rows pass ρ = None) |
| F3, V1 | no (JS only) |
| legacy → `#[cfg(test)]` | no |
| W14 law change | no (rows run at ρ = 1); `test_custom_rho_*` / `test_bulk_composes_with_rho` may move |

ADR-0076 already plans a deliberate re-capture for phase 6. F1 should land
before it and separately, so that the re-capture records only the moved laws.

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
6. **W14** (T_cut): measure first (D); the law is already in `raptorpath-math`.
7. **F3 and V1** (visualizer JS): independent of phase 6. F3 needs its paper
   derivation first.

## Outside scope (noted, not audited)

§5.6 describes the engine's shed law as running "only on the evicting seat",
with "the retain (ρ = 1) seat never sheds". That is a seat keyed on ρ = 1, so
the engine may have the same ρ step as V5. It should be checked against
`net/shed.rs` under the same invariant.
