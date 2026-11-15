# ADR-0077: The model shares the engine's laws and decisions through `raptorpath-math` and `raptorpath-core`

## Status: Accepted (user, 2026-11-14). Amends CLAUDE.md's scope rule, and with it ADR-0076's drafted phase-6 bullet.

## Context

The L0 model (`raptorpath-wasm` + the visualizer) shares only closed-form functions with the engine, through `raptorpath-math`. Everything that drives them is a model-only copy: about 2 900 lines in `raptorpath-wasm/src/lib.rs`, plus the page's JavaScript.

The copies drift from the engine, and they grow mode switches:
- `docs/derivations/wasm-modeswitch-audit.md`: 12 B sites, 4 of them live.
- `docs/derivations/visualizer-drift-2026-11.md`: stale page claims about the store.

CLAUDE.md's scope rule forbade any engine-crate change for a visualizer feature. That rule is what kept the copies separate.

`docs/derivations/shared-core-design.md` inventories 24 engine decisions and proposes an I/O-free crate that both sides can call.

## Decision

1. **Formulas** the model evaluates move to `raptorpath-math` (ADR-0076 phase 6).
2. **Decisions** the model takes move to a new crate, `raptorpath-core`.
   - It depends only on `raptorpath-math`.
   - It is std without I/O and compiles for wasm32.
   - Time, configuration and randomness are inputs, not reads. Neither crate reads the environment.
   - The engine keeps its gauges, its env/gate wrappers and its I/O.
3. **Engine-side moves are zero-behaviour-change refactors.** Each is proven by:
   - the decision pins (`decision_trace_pin` plus the existing pins);
   - a host ns bench on hot paths;
   - a §41c guard.

   The guard may be waived only when the release binary's sha is identical before and after. Model-side adoption is a deliberate model change with `GOLDEN_CAPTURE=1`. No step is both.
4. **Only class-A code moves from now on** (design §1): no hint type, no δ/ρ- or path-count-selected branch, and no env read.
5. **Known debt already inside `raptorpath-math` is named, not denied:**
   - `p_fec_normal`'s cut to 0 at r ≤ ε/(1−ε) (audit W22, ledger R03). Its replacement is the derivation now open.
   - The cap and floor around `find_t_cut` (audit W14).

   Each carries a bounding test until its fix lands.
6. **A decision is shared only when the model holds all of its inputs** as idealised state. So the store law, recovery verdicts and placement stay engine-only until the model has a send queue and gap reports.
   - Every model-adoption step needs an "executes" witness. Example: the model's bind fraction at the analog cell against the engine's ledger value.

## Consequences

- **Near term,** the model can execute only the span, shed and taper decisions. Store, recovery and placement stay unshared.
- **VM cost:** three guard batches (α, β, γ in the design's §4), queued behind F1, B-1 and §51.
- **ADR-0076 phase 6's drafted CLAUDE.md bullet is replaced** by the one below. Phase 5 (the `net/` split) must not interleave with the core moves.
- **New enforcement:**
  - `raptorpath-core` builds for wasm32, and a source deny-list test runs in `build_visualizer.sh`;
  - every core function taking δ or ρ has the preset ±2 % continuity test;
  - every function taking N has an absolute test at N = 1 and N = 2.
