# Mode-switch audit of the L0 model (before ADR-0076 phase 6)

Question: which sites in `raptorpath-wasm/src/lib.rs` and
`raptorpath-visualizer/` select behaviour by hint, by a hint-derived boolean,
or by a threshold on δ or ρ, and which of them break the no-mode-switch
invariant (`CLAUDE.md`)? Phase 6 moves closed-form laws into
`raptorpath-math`, so every such site next to a moving formula has to be
classified first.

Base: `github/main` at `2d3b2069`. No model code changed; the only code change is a test tolerance (`test_visualizer.mjs` §7d3, see Coverage). Line numbers refer to that
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
  (2026-09-27). The probes are current, because nothing changed since then
  reaches the sim's executed laws:
  - `git diff 8f3fd7c5..github/main -- raptorpath-wasm/src` is test-only: +31
    lines, one pool test.
  - In `raptorpath-math/src` (+852 lines) the changes are:
    - `changepoint.rs`: one `#[test]`.
    - The two `rstar_bitpin_table_*.in` files: test data.
    - `lib.rs`: tests, plus one non-test change, `b7b1da08`. That change
      memoizes the burst-tail fit inside `r_star_mass` and is declared
      bit-identical.
  - The sim never reaches `r_star_mass` at all. It feeds
    `mass: MassStats::default()` (`lib.rs` :842), so `controller_rate`'s mass
    term is gated off by `inp.mass.is_valid()` (math :786).
  - The embedded build also reproduces all six golden rows of
    `test_multipath_n1_identical_golden` bit for bit.
  - No fresh build and no cargo.
- Reachability tags: **UI** means the visualizer page can reach the site.
  The shipped UI passes ONE hint, `'continuum'` (`interactive-visualizer.html`
  :1987), and `test_visualizer_ui.mjs` :145–158 gates that at every dial
  position, ρ < 1 included. **ABI** means it is reachable
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
| W8 | 735 | `continuum_tail`: `if self.bulkness <= 0.0 { return self.tail_target }` | β = 0 fast path of the tail-domain blend | UI (display) | **C** (β guard). The only caller is `get_delta_eff` (:1678), which feeds the `δ_eff` readout. No step: at β = 0⁺ the blend is 10^(1·log t + 0) = t up to an ulp. Measured δ_eff 9.999999999999999e-6 at δ = 0.5 and 1.0000000036e-5 at δ = 0.5·(1 − 1e-9). See the headline finding below. |
| W9 | 784–788 | `rate_inputs`: `if legacy_bulk_delta && hint_bulk { (min(0.1, p̂), false) } else { (tail_target, hint_bulk) }` | **which rate law runs**: `bulk_late_is_fine = hint_bulk` | ABI / native | **B**: exactly the `bulk_late_is_fine = hint == Bulk` selection that paper §4.5 names as the retired mode switch. The legacy arm is test-only (C). |
| W10 | 866 | `controller_rate_now`: `if let Some(r) = self.fixed_r { return r }` | the controller bypass | ABI | **D** (fixed r). |
| W11 | 883 | `rate_now`: `if self.bulkness > 0.0 { mix } else { controller_rate(rate_inputs()) }` | a β = 0 fast path, and the router into W9 | UI + ABI | **B** (structural). §4.5: "There is no `if β == 0` fast path; that would be the mode switch again", and the engine's `RateSnapshot::rate` has none. On the continuum the value step is zero (W9 with `hint_bulk = false` is exactly the anchor term), and §7d3 of `test_visualizer.mjs` gates this at Auto ±2 %. The guard's real effect is to let `β = 0 ∧ hint_bulk` run a different law. |
| W12 | 896 | `(1.0 - self.bulkness) * anchor + self.bulkness * bulk` | — | UI | **A**: r(β) = (1−β)·r_anchor + β·r_late, both terms computed. |
| W20 | 892 | bulk term's `tail_target = (BASE·100)` | — | UI | **A**: not read by `controller_rate` when `bulk_late_is_fine` (math :741). |
| W19 | 111 | JS export `controller_rate(…, bulk_late_is_fine: bool, …)` | which of the two mix terms | ABI (the HTML never calls it; `test_visualizer.mjs` §7 does) | **A**: a term selector for the two always-computed terms. For phase 6, prefer exporting a mix function (see order item 4). |
| W13 | 1386–1394 | `legacy_burst = (legacy_bulk_delta && hint_bulk) \|\| (legacy_tail_burst && !hint_bulk)` → one-shot tail burst | ablation arm | native | **C** (test-only counterfactual, never exposed to JS). Keep it out of `raptorpath-math`; move both `legacy_*` fields behind `#[cfg(test)]`. |
| W14 | 1401–1409 | `if self.rho < 1.0 { t_cut = find_t_cut(…); cut = max(t_cut, 2·RTT) … }` | T_cut give-up horizon | UI | **B**: see F4. The guard alone is redundant (`find_t_cut` returns ∞ at ρ ≥ 1); the step is in the law behind it, at a non-preset ρ*. |
| W16 | 1673–1681 | `get_delta_eff`: `if hint_bulk {…} else if bulkness > 0 { continuum_tail } else { tail_target }` | readout | UI (display) | **C** (hint guard and β guard, display only). The β branch at :1676 does not step, for the same reason as W8. For "bulk" it differs from continuum@0.005 only by the clamp to [1e-9, 0.99]. |
| W17 | 1689 | `get_completion_exposure`: `if bulkness > 0 { χ_raw } else { completion_chi() }` | accessor | ABI (the HTML does not read it) | **C** (β guard on an accessor no page reads), but it **steps**. At β = 0 the continuum gets `completion_chi()` = 0, since `hint_bulk` is false. At β = 0⁺ it gets χ_raw. Measured over a full run at .05/.5/50: the maximum is 0 at δ = 0.5 and 0.99865 at δ = 0.5·(1 − 1e-9). The accessor misreports the χ the bulk term actually reads. Fix: return χ_raw always (falls out of F1). |
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
D: 2 (W3, W10).

**B split by reachability:**

| set | sites | what the measurements show |
|---|---|---|
| **live**: the UI reaches it | W11 (:883), W14 (F4), V5/V6 (F3) | behaviour steps in W14 and V5/V6. W11 has a zero value step but is the §4.5-banned fast path, and it routes the dead machine. |
| **dead second machine**: reachable only from the native tests, the golden capture and `test_visualizer.mjs`'s classic-hint arms | W2, W5, W6, W9 (F1); W4 (F2); V1 | no behaviour step at any point the UI can reach. The fix is removal, not a new formula. | W5, W6 and W9 are classed by their `hint_bulk` term;
their `legacy_*` terms are test-only C, and the `fixed_r` term of W6 is D.

## Headline finding: `continuum_tail` is not an executed law

ADR-0076 phase 6 lists "the continuum tail blend (`continuum_tail`, :734)" as a
law to move. The comments at :408–411 and :1017–1019 describe the per-tick
effective tail as that log-blend. The sim does not run it:

- the rate is the rate-domain mix (W12);
- `continuum_tail` is read only by `get_delta_eff` (:1678, the `δ_eff` readout);
- `rate_now`'s own comment (:879–881) says a tail-domain blend would step at
  the preset, because r*(δ) is near-binary against the cold-start ε̂.

This does **not** make `test_continuum_one_law_across_the_dial` hollow. Its
assertions are on the executed path: settled r*, full decode, tick and FEC
counts, and the Bulk-nudge FEC delta. None of them reads `continuum_tail`.

So moving it into `raptorpath-math` would put a display formula into the shared
law crate as if it were a law, and that formula is the one the model rejected.
Recommendation: drop it from the phase 6 list. Correct the two comments. Either
delete `get_delta_eff`'s blend or label the readout as "the tail the anchor
term would need". No δ_eff exists for a rate-domain mix.

## The B sites and their fixes

### Live B sites

#### W11: the β = 0 fast path (:883)

The UI reaches this path at every δ ≥ 0.5. The value step on the continuum is
zero: with `hint_bulk = false`, the `else` arm is exactly the anchor term. §7d3
gates this at Auto ±2 %. It is still the fast path §4.5 bans, and it is the
router into the dead machine (W9).

Fix: delete the guard, so that `rate_now` always returns
(1−β)·r_anchor + β·r_late. On the UI path the result is unchanged by the
identity below (F1, "Golden argument").

#### F3 + F4: one give-up law (V5/V6 span budget, W14 T_cut)

These two are the same object. Both answer one question: what fraction of
symbols is allowed to stay unrecovered, and from what age. Both are functions
of

```text
   R  = ε̂·(1 − P_fec(r, ε̂, A*, σ²))          residual past in-window FEC (§5.6)
   x  = (1 − ρ) / R                           declared concession over that residual
```

**The law: the give-up profile u(t).** Let u(t) be the fraction of symbols
still unrecovered at age t. Model ARQ as retransmit rounds, one per RTT, each
lost with probability ε̂_retx:

```text
   u(t)        = R · ε̂_retx^(t/RTT)                                (u(0) = R, u(∞) = 0)

   T_cut(ρ)    = u⁻¹(1 − ρ) = RTT · max( 0 , ln x / ln ε̂_retx )     ρ → 1 ⇒ x → 0 ⇒ T_cut → +∞
   A(δ, ρ)     = max( D(δ) , T_cut(ρ) )                             give-up age; D(δ) is the δ-honest floor (§5.6)
   budget(ρ)   = u(T_cut) = R · min(x, 1) = min( 1 − ρ , R )        = F3 candidate (b)
   give up a hole  ⇔  age > A(δ, ρ)  ∧  shed_total + 1 ≤ budget · src_total
```

What this gives:

- **The saturation points coincide.** Budget (b) stops being capped by 1−ρ
  exactly at x = 1. At the same x, T_cut reaches 0. One condition,
  1 − ρ ≥ ε̂(1−P_fec), saturates both, because the budget is u evaluated at
  T_cut.
- **No ρ = 1 branch.** At ρ = 1, x = 0 and ln 0 = −∞. Since ln ε̂_retx < 0,
  IEEE gives T_cut = +∞ and budget = 0 with no `rho < 1.0` guard (W14) and no
  `retain` flag (V6). RETAIN is the limit of the dial, not a mode.
- **No δ step.** δ enters through D(δ) and through r (and so R). Both are
  continuous. The `rStar ≤ 1e-6` threshold has nothing left to cut.

**Clamps** (each needs a bind-fraction gauge, reported):

| clamp | binds when | meaning |
|---|---|---|
| `max(0, ·)` in T_cut (the lower bound) | x ≥ 1, i.e. 1 − ρ ≥ ε̂(1−P_fec) | the contract concedes at least everything FEC misses, so no ARQ wait is owed. At Bulk (R ≈ ε̂) it binds for ρ ≤ 1 − ε̂: ρ ≤ 0.95 at ε 5 %, ρ ≤ 0.90 at ε 10 %. |
| `max(D(δ), ·)` in A | T_cut < D(δ) | the δ-honest floor. It replaces the wasm's constant 2·RTT floor (:1409), which equals D at Bulk only. |
| ε̂_retx ∈ (0, 1) | input domain | ε̂_retx → 1 makes ln → 0⁻ and T_cut → ∞ (ARQ never succeeds), which is the right limit. ε̂_retx = 0 gives T_cut = 0 (one round always succeeds). |

**Provenance**, needed in §4.10/§5.6 before code:

- R: the §5.6 shed budget as shipped (measured inputs ε̂, r, A*, σ²). One
  window must be chosen for both uses. Today the cartoon uses A* and
  `find_t_cut` uses W. I propose A*, since §5.6's engine budget reads it.
- ε̂_retx: measured, the per-path loss estimate on the retransmit's path. This
  assumes rounds are independent, i.e. RTT ≫ 1/q. Otherwise it is the GE
  bad-to-bad transition probability over one RTT.
- D(δ): shipped (§5.6).
- RTT: measured.
- x: a declared dial (ρ) over a measured quantity.

**What it replaces.**

- F3 (cartoon budget, V5/V6). The shipped cartoon has two steps:
  - In δ at ρ = 1, where `rStar` crosses 1e-6: the budget jumps 0 → ε̂ at
    δ* = 0.005615 (ε 5 %), 0.008207 (ε 2.5 %) and 0.0050002 (ε 10 %, i.e. at
    the Bulk preset). On a 2 % grid the largest neighbour jump is 5e-2 and
    1e-1.
  - In ρ at ρ = 1: the budget is (1−ρ) → 0 from below, but the derived
    residual (1.93e-5 at Auto, ε 5 %) at ρ = 1.
  - Root cause: R is largest (= ε̂) at r = 0, the Bulk end, exactly where the
    cartoon calls the law "compiled out", so a flag had to cut it.
  - Shape of the shipped budget at ρ = 1 (q .5, RTT 50, cap 4, W auto):

    | δ | .005 | .0055 | .006 | .0075 | .01 | .02 | .05 | .1 | .2 | .5 | 5 | 50 |
    |---|---|---|---|---|---|---|---|---|---|---|---|---|
    | ε 5 % | 0 | 0 | **5e-2** | 5e-2 | 5e-2 | 5e-2 | 2.3e-2 | 7.5e-3 | 1.1e-3 | 1.9e-5 | 2.7e-5 | 5.1e-5 |
    | ε 10 % | 0 | **1e-1** | 1e-1 | 1e-1 | 1e-1 | 1e-1 | 1e-1 | 2.0e-2 | 2.5e-3 | 3.8e-5 | 6.1e-5 | 1.25e-4 |

  - The rejected alternative, (a) max(1−ρ, (1−β)·R), is continuous but not
    monotone in δ: 0 → 1.5e-2 → 1.9e-5 → 5.1e-5 across the dial at ε 5 %. Its
    largest 2 %-neighbour jump is 0.24·ε̂, and its (1−β) has no provenance.
  - Budget (b) = min(1−ρ, R) at the corners:

    | corner | shipped | unified (b) |
    |---|---|---|
    | Bulk, ρ = 1 | 0 | 0 |
    | Bulk, ρ = .95 | 5e-2 | 5e-2 |
    | Bulk·1.02, ρ = 1, ε 10 % | **1e-1** | 0 |
    | Realtime, ρ = 1 (ε 5 % / 10 %) | 5.1e-5 / 1.25e-4 | **0** |
    | Realtime, ρ = .95 | 5e-2 | **5.1e-5 / 1.25e-4** |

- F4 (sim T_cut, W14). Today `find_t_cut` + `max(·, 2·RTT)` is two-valued:
  - At ε .05, W 64, r = 1e-3: about 0.01 ticks for ρ ≤ 0.95 (the 2·RTT floor
    binds); 640 = 10·W for ρ ≥ 0.99 (the search cap binds); ∞ at ρ = 1.
  - At r = 0.3: about 0.01 for every ρ < 1.
  - **The floor or the cap binds at every one of the ten measured ρ < 1
    points.** The behaviour step sits at a non-preset ρ*, bisected to 1e-6 on
    continuum@0.005: 0.983332 at .05/.5/50 (given_up 2 → 0) and 0.988175 at
    .10/.3/80 (6 → 0). Elsewhere `given_up` is flat in ρ.
  - Cause: `p_recovered_within` models ARQ progress with the FEC rate r, so at
    r ≈ 0 its supremum is about 1 − R, and any ρ above it returns the
    bisection cap. In u(t) above, ARQ progress comes from retransmit rounds
    instead.
  - **`find_t_cut` has no engine caller** (no match in `raptorpath/src`), so
    this is a model-only law finding. **Defect finding for the ledger** (the
    coordinator writes the verdict): the T_cut law is pinned by its 2·RTT
    floor or its 10·W cap over its whole measured operating range
    (MEASUREMENT DISCIPLINE 17/18).

**Gate change for the owner: `test_visualizer_ui.mjs` :181.** It asserts
`lawBulk.retain && lawBulk.rStar === 0 && !lawRt.retain && lawRt.rStar > 0`.
The term `!lawRt.retain` pins that **at ρ = 1 the Realtime end sheds**, up to
the derived residual. The cartoon labels this "ρ = 1 target — sheds only the
derived §5.6 residual". §5.6 says the opposite: "the retain (ρ = 1) seat never
sheds; the law runs only on the evicting seat".

Under the unified law, budget(ρ = 1) = 0 at every δ, so :181 fails. Replace it
with:

- budget = 0 at ρ = 1 at both ends;
- budget = min(1−ρ, R) at ρ < 1;
- r* > 0 at Realtime and r* = 0 at Bulk (unchanged).

:205 ("sheds counted at rt-end with ρ < 1", ε 10 %, ρ .95) then runs with a
budget of 1.25e-4 instead of 5e-2 and must be re-checked. :233 (budget 0.05 at
Bulk, ρ .95, ε 10 %) still holds: min(0.05, 0.10).

Both the law and the gate change are **the owner's call**. Golden impact: none
for F3 (JS only). For F4: none on the golden rows (ρ = 1, and T_cut = ∞ either
way), but `test_custom_rho_gives_up_late_losses` and
`test_bulk_composes_with_rho` may move.

### Dead second machine (reachable only from tests and the golden capture)

#### F1: delete the classic-hint machine (W2, W5, W6, W9; W16/W17 follow)

"bulk" is `β = 0 ∧ hint_bulk`, a second representation of continuum@0.005
(β = 1), and the same holds for auto/realtime against 0.5/50.

Measured: all 8 fingerprint fields agree at all six golden cells (wasm32).

| hint, ε, q, RTT | classic = continuum@δ(hint) | path the continuum arm ran |
|---|---|---|
| auto .05/.5/50 | [667,2000,668,0,101,2000,2668,2532] both | :883 fast path (β = 0) |
| auto .10/.3/80 | [747,2000,985,0,185,2000,2985,2705] both | fast path |
| bulk .05/.5/50 | [569,2000,0,116,113,2000,2116,2000] both | **the mix** (β = 1) |
| bulk .10/.3/80 | [647,2000,85,188,198,2000,2273,2059] both | **the mix** |
| realtime .05/.5/50 | [733,2000,710,3,101,2000,2713,2571] both | fast path |
| realtime .10/.3/80 | [754,2000,1015,0,184,2000,3015,2734] both | fast path |

Only accessors differ for "bulk": `get_r_star_auto` 0.1512 vs 0, and
`get_derived_w` 200 vs 26.4, because they read `tail_target` 1e-3 vs 0.05.
Nothing the UI reaches steps. **The fix is removal, not a new formula:**

- **The hint names become an alias table**: `"bulk" → δ 0.005`,
  `"auto" → 0.5`, `"realtime" → 50`, all built by the one continuum path. Then
  delete `hint_bulk` and with it W5, the `hint_bulk` term of W6, W9 and W16's
  first branch. W17 becomes `χ_raw`. This keeps the names as "named points on
  the dials" (CLAUDE.md).
- `#[cfg(test)]` alone is not enough. `test_visualizer.mjs` runs on the
  release wasm and constructs the classic hints about 20 times (:89–:131,
  :167, :259, :274–:297, :374–:484). Behind `#[cfg(test)]` the native goldens
  would still run, but every one of those mjs arms would have to be retargeted
  to `"continuum"`. The alias table needs no retargeting.
- **"custom"** (tail target given directly, β ≡ 0, anchor-only) is not a dial
  point. With tail 0.05 it is an anchor-only machine at a Bulk-class tail that
  no δ produces. Remove it from the ABI. Its three users:
  - `test_visualizer.mjs` :121 and :259 test ρ < 1 give-up. Retarget them to
    `"continuum"` with the same ρ.
  - The β = 0 clause of `test_continuum_one_law_across_the_dial` compares
    against it. Under the alias table that clause is tautological. Keep it, if
    wanted, against a `#[cfg(test)]` anchor-only constructor.
- `legacy_*` moves behind `#[cfg(test)]`. Fixed r stays D (owner's word).

**Golden argument.** After the alias table, the golden rows run the mix at
β ∈ {0, 1}. The rows stay identical iff r = (1−β)·a + β·b reproduces the term
the classic path computed. That needs two conditions:

1. **β is exactly 0 or 1 at the presets.** `bulkness_of_delta` gives 0 at 50
   (clamp), 0 at 0.5 ((a−a)/(a−b) = 0) and 1 at 0.005 (x/x). This is pinned
   bit-exact by `test_dial_reexports_are_bit_identical_to_the_shipped_sim_forms`
   (:2302–2304, native) and measured on wasm32 (`bulkness_of_delta(0.5) === 0`
   in §7d3).
2. **The zero-weighted term is finite.** Then 0·term = 0 and a + 0 = a,
   bit-exact. Argued from the code:
   - `controller_rate`: `random_rate` is computed under `if core > 0.0`, so a
     NaN core (−∞·0) gives 0. `burst_rate` is computed under
     `t_symbols > 0.0`. The mass term is inert (`MassStats::default()`).
   - `f64::max` discards NaN, `soft_saturate` is applied to finite values, and
     the result is `clamp(0, max_overhead)`.
   - The completion term: `r_tail` comes from `compute_r_star_exact`, a
     bisection in [0, 2], and dχ ∈ [0, 1].

   Not measured: the mix cannot be made to run at β = 0 exactly today. A probe
   at δ = 0.5·(1 − 1e-12) also moves the tail by 2e-12 relative, and `lost`
   moved 101 → 103.

Measured: on the bulk rows the mix ran and matched. The tail ulps also match:
on wasm32, t(0.5) = 9.999999999999999e-6 against the classic 1e-5, and
t(50) = 1e-7 against 1e-5·0.01 = 1.0000000000000001e-7, and all 8 fields still
agreed. Native is unverified. Run `test_multipath_n1_identical_golden`
**without** `GOLDEN_CAPTURE` first, and re-capture only with the owner's word.

#### F2: ρ for every hint (W4)

ABI-only. "auto"/"realtime" drop `custom_rho`. On harsh cells this is a
behaviour step, for example ε .20, q .1, RTT 100, ρ .95: realtime gives up 0
symbols in 895 ticks, continuum@50 gives up 28 in 823. It disappears with F1's
alias table, which needs no new formula: every name goes through the continuum
constructor, which already honours ρ. Golden impact: none (the rows pass
`None`).

#### V1

`params.hint === 'fixed'` (HTML :1705) is the banned form verbatim. The UI
cannot reach it (the hint is always `'continuum'`), and wasm ignores `fixed_r`
unless the hint is `"fixed"`. Delete the ternary and pass `params.fixedR`.

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

| B site | reach | covered by | uncovered: the absolute test that would catch it |
|---|---|---|---|
| W11 | live | 7d3 Auto ±2 % (value) | structural: after the fix, a source check that `rate_now` has no branch on `bulkness` |
| V5/V6 (F3) | live | :181 and :233 pin the two sides, not continuity | a 2 % grid over the whole dial (δ = 0.005·1.02^k up to 50) at ε ∈ {2.5, 5, 10} % and ρ ∈ {1, 0.9999, 0.95}: every pair of neighbours has \|Δbudget\| ≤ 0.05·ε̂ (today ε̂, so it fails). Also \|budget(1) − budget(1 − 1e-9)\| ≤ 1e-6 (today 1.9e-5 at Auto, so it fails). Assert on `spanLaw()` through the stub DOM. A grid, because the step is not at a preset. |
| W14 (F4) | live | nothing (single-ρ tests only) | `find_t_cut` at fixed inputs (ε .05, q .5, r ∈ {1e-3, 0.3}, W 64), on a geometric grid of 1−ρ ∈ [1e-6, 0.1] with ratio 1.02: non-decreasing in ρ, and \|ln T(next) − ln T(this)\| ≤ 0.1 wherever T > 0 (today the jump 0.01 → 640 has ln ratio 11, so it fails). Plus the clamp bind-fraction gauges above. At sim level: \|Δgiven_up\| ≤ 1 over ±2 % of 1−ρ* at ρ* = 0.983332, .05/.5/50 (today 2 → 0 within 1e-6, so it fails). |
| W2/W5/W6/W9 (F1) | dead | nothing (the gates run only the continuum arm) | until the machine is removed: for h ∈ {auto, bulk, realtime} × the six golden cells, every public accessor equals continuum@δ(h). Red today: "bulk" `get_r_star_auto` and `get_derived_w`. |
| W4 (F2) | dead | `test_bulk_composes_with_rho` (bulk only) | at ε .20, q .1, RTT 100, ρ .95: every hint has `get_rho() == 0.95`, and its fingerprint plus `get_given_up` equal continuum@δ(h). Red today: realtime 0 given up / 895 ticks vs 28 / 823. |
| V1 | dead | nothing | a UI-gate grep: no `hint ===` in the script block |

**§7d3 tolerance, tightened in this branch** (test-only, no model change):

- Before: a flat 0.2 decades (t, b, ζ) and 0.05 (β) for the ±2 % nudge. That is
  about 6× the continuous change, so a step of up to about 1.47× in t passed.
- Now: per law, `tol = 1.5 × slope × 2·log₁₀(1.02)`. The slope is the law's
  steepest analytic slope per decade of δ:

  | law | slope | tolerance |
  |---|---|---|
  | t | (log₁₀0.05 + 5)/2 = 1.8495 | 0.0477 decades |
  | ζ | 1 | 0.0258 decades |
  | b | log₁₀2/2 | 0.0039 decades |
  | β (linear) | 0.5 | 0.0129 |

- The largest continuous change at the presets is 0.0245 decades (t at Auto),
  so the gate passes with margin. A 1.12× step in t, or 1.07× in ζ, would now
  fail.
- `node raptorpath-visualizer/test_visualizer.mjs`: all checks pass (exit 0).

## Golden impact summary

| fix | moves `test_multipath_n1_identical_golden`? |
|---|---|
| W11 (delete the guard) | no on the UI path, by the identity. The golden rows go through it only after F1. |
| F1 (alias table) | bulk rows: measured no (the mix ran). Auto/realtime rows: no if β ∈ {0, 1} exactly (pinned) and the zero-weighted term is finite (argued). Tail ulps measured harmless on wasm32. Native unverified: run without capture first; a re-capture needs the owner's word. |
| F2 | no (falls out of F1; the rows pass ρ = None) |
| F3, V1 | no (JS only) |
| F4 | no (rows at ρ = 1); the ρ < 1 native tests may move |
| `legacy_*` → `#[cfg(test)]` | no |

ADR-0076 already plans a deliberate re-capture for phase 6. F1 should land
before it and separately, so that the re-capture records only the moved laws.

Probe scripts (not committed): `probe.mjs` to `probe5.mjs` in the auditor's
scratch directory. Each one reads the committed HTML and evaluates its engine
block the way `test_visualizer.mjs` does.

## Fix order, by what phase 6 moves first

1. **The five closed forms** (`span_deadline_d`, `span_width_a_star`,
   `pipeline_depth_m_star`, `trailing_offset_delta`, `sim_tail_target_of_delta`):
   class A, with no site blocking them. Move them first.
2. **F1 + W11 together** (the alias table, deleting `hint_bulk`, deleting the
   :883 guard, `legacy_*` → `#[cfg(test)]`), with the golden check run without
   capture. This unblocks item 3.
3. **χ_trunc and the completion budget** (`completion_debt_increment`, :705):
   after F1, its only remaining guard is `fixed_r`, which waits on the owner.
   Then it moves branch-free.
4. **`continuum_tail`: do not move it.** It is display only (headline
   finding). Fix the comments at :408–411 and :1017–1019 and drop it from the
   ADR's phase 6 list.
5. **The rate mix itself** (not in the ADR list yet): move
   `(1−β)·R(late = false) + β·R(late = true)` from the engine's
   `RateSnapshot::rate` into `raptorpath-math`, and call it from both sides.
   That deletes the bool in the W19 export.
6. **F3 + F4, the unified give-up law**: it needs its §4.10/§5.6 derivation and
   the owner's call on the :181 gate change. `find_t_cut` is already in
   `raptorpath-math` and has no engine caller. Independent of the moves.
7. **V1**: any time.

## Outside scope (linked, not a new item)

The engine's ρ step (§5.6: "the retain (ρ = 1) seat never sheds; the law runs
only on the evicting seat") is the existing `if reliable` debt, W3.6a in the
plan: `raptorpath/src/net/emit_source.rs` :308 and :815, and
`raptorpath/src/net/sender_phases.rs` :980 and :1131 (inside the
`if reliable` block that opens at :1122). The unified give-up law above is the
model-side form of the same fix: ρ = 1 as the limit x → 0, not a seat.
