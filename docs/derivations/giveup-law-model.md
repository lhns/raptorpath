# The model's give-up law: option (ii) (owner decision 2026-11-14, item 3)

Derivation only; implementation follows in `raptorpath-wasm` and the page.

**Done with Fable.** Round 1 (on the draft) changed six things:
- test 4 split: the sim never ran max(D, T_cut);
- monotonicity asserted in ρ only;
- the paper symbols separated: R against ρ;
- a bind-fraction gauge for the min, and a ≥ 1-symbol cell for every give-up test;
- test 3 renamed to a law-form bound;
- the Realtime-at-ρ = 1 page change made explicit.

## Law

§5.6 today writes the residual as "1 − ρ". It defines ρ from the physics, while CLAUDE.md makes ρ an independent dial. This block separates the two symbols:

```text
   D(δ)    =  min( b(δ)·RTT , 2·RTT )                                 deadline (§5.6, unchanged)
   R       =  ε̂ · (1 − P_fec(r_live, ε̂, W_site, σ²_burst))           residual after in-window FEC
   budget  =  min( 1 − ρ , R )                                        ρ = the declared contract dial, ρ ∈ [0.9, 1]
   give up a hole  ⇔  age > D(δ)  ∧  given_up + 1 ≤ budget · src_total
```

**The sentence:** a hole is given up only after the deadline D(δ), and never beyond both what the contract allows (1 − ρ) and what FEC leaves unrecovered (R).

**Shape:**
- min of two continuous terms, so continuous in ρ, in δ (through D, r and the window) and in ε̂;
- non-increasing in ρ;
- **not** monotone in δ, because R rises from Auto to Realtime as smaller spans concede more;
- at ρ = 1 the budget is 0, so the retain seat is the ρ = 1 point, with no branch;
- units: fractions of src_total.

**Provenance:**
- **D and R:** §5.6.
- **R inherits W22's step** (the `p_fec_normal` cut; bounded by `test_p_fec_normal_vs_paper_phi_divergence_is_bounded`) until the sliding per-loss P_fec law lands (`pfec-continuous.md`, open). Its values are provisional.
- **1 − ρ:** the declared dial.
- **min:** the contract against the physics, with no constant.

**Window per site (W_site):**
- the sim: its sliding window W (`encoder.window_size()`, golden W 64);
- the cartoon: A* (`span_width_a_star`, fractional);
- the engine: A* (`net/emit_source.rs` ~:578).

**The engine (unchanged; §11.3 divergence row):** the engine evaluates the law at two seats only.
- **Evicting seat** (`shed_on`, i.e. not `--window-reliable`): budget = R.
- **Retain seat:** budget = 0.
- **Agreement:** the model agrees at ρ = 1, and wherever 1 − ρ ≥ R. It is tighter only for 0 < 1 − ρ < R.
- **The engine CLI reaches ρ < 1 only at Realtime** (status: "the CLI's ρ"). The engine's seat switch is the `if reliable` debt listed in `w3-per-leg-state.md`.

**Bind-fraction gauge (FORMULA-FIRST):**
- The min is a clamp, so the share of slider positions where 1 − ρ binds against R is reported per preset and per cell.
- On the shipped R, R binds at every ρ < 1 at Auto and Realtime on four of the five audit cells: ρ is inert there, a degenerate-law ledger entry. Under any continuous P_fec this changes (the `pfec-continuous` erratum in status).

## What it replaces

- **Cartoon V5/V6:** `budget = userRho < 1 ? (1 − userRho) : derived` and the `retain = userRho ≥ 1 ∧ rStar ≤ 1e-6` flag (`interactive-visualizer.html` ~:2650–2651; the page's own JS, not the `build_visualizer.sh` template).
- **Sim W14:** age-only give-up at `max(t_cut, 2·RTT)` under `rho < 1.0` (`raptorpath-wasm/src/lib.rs`).
  - The sim stops calling `find_t_cut`.
  - The triangle solvers and the JS export keep it.

**Visible change at the page's default** (the ρ slider defaults to 1.0):
- The cartoon sheds R at Realtime today. Under the law it sheds nothing at ρ = 1, at every δ.
- The engine's Realtime hint runs the evicting seat (budget R).
- The page states this with a table row: the model's ρ = 1 is the retain seat at every δ; the engine's hint selects its seat.

**The sim's rule change:**
- The age floor drops from 2·RTT to D(δ). At Auto and Realtime, D(δ) (RTT and RTT/2) is at or before the first retransmit (one RTT). So the sim can give up a hole before ARQ has fired, within the budget.
- The budget conjunct is new. Without it, a hole would be given up at ρ = 1.

## Tests (red first where they assert a property)

1. **Anchors.**
   - budget(ρ = 1) = 0 at every δ preset;
   - budget = R exactly when 1 − ρ ≥ R;
   - budget = 1 − ρ exactly when 1 − ρ < R.
2. **Continuity and monotonicity.**
   - Monotone non-increasing in ρ over [0.9, 1], including the approach to ρ → 1.
   - Continuous at ±2 % nudges in ρ and in δ at every preset. The preset nudges never reach W22's step, so no exclusion is needed there.
   - A dial sweep in δ, if added, excludes only the neighbour pair that straddles r = ε/(1−ε), and asserts that the excluded jump in the budget is ≤ R's own jump there.
3. **Law-form bound, at equal R** (`raptorpath-wasm` cannot call the engine's `residual_loss_after_fec`):
   - |min(1 − ρ, R) − R_seat(ρ)| = 0 at ρ = 1 and wherever 1 − ρ ≥ R;
   - otherwise it is ≤ R − (1 − ρ), with R_seat(ρ) = R for ρ < 1 and 0 at ρ = 1.
   - The real agreement test waits for D11 in `raptorpath-core`.
4. **Reduced form against the general form (the T_cut drop):**
   - Over the slider grid × presets × the audit cells, bound (T_cut(ρ) − D(δ))⁺ / RTT, where T_cut = RTT·max(0, ln x / ln ε̂_retx) and x = (1 − ρ)/R.
   - Report where it is > 0 (on the cartoon's harsh-Bulk inputs: about 8 and 26 of 100 positions at ε 0.2 and 0.3).
   - Assert only the bound, not the count; the 8 sits on a float tie.
5. **The sim's rule change:**
   - At Auto and Realtime, on a cell where budget·src_total ≥ 1, assert given_up > 0 at ρ < 1 and = 0 at ρ = 1.
   - Assert every given-up hole is older than D(δ).
   - Report how many were given up before their first retransmit.
6. **Existing tests that move:** `test_custom_rho_gives_up_late_losses` and `test_bulk_composes_with_rho`. Each needs a cell where the budget admits ≥ 1 symbol, and asserts given_up > 0 there. State the new counts and why.
7. **Gate change:**
   - `test_visualizer_ui.mjs` ~:181: `!lawRt.retain` becomes "budget = 0 at ρ = 1 at both ends", keeping r* > 0 at Realtime and r* = 0 at Bulk.
   - ~:205 (ε 10 %, ρ .95, Realtime) is re-checked with budget = min(0.05, R).
   - ~:233 still holds.
8. **Gauge test:** the bind fraction of each side of the min is computed and reported (not pinned) per preset.

**Sequencing:** this edits `lib.rs` together with phase 6 (the formulas into `raptorpath-math`), so the two are done in sequence, not in parallel. Goldens: none expected for the cartoon (JS only). The sim rows at ρ = 1 are unchanged (budget 0, T_cut = ∞ today). Run without capture first.
