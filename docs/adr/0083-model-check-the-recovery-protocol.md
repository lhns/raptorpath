# ADR-0083: Model-check the hole lifecycle, first by hand and then over the real decisions

## Status: Proposed (2026-11-20). Plan: `docs/derivations/test-detection-plan.md` S7, S9.

## Context

- The evicting-seat stuck states (clamp before shed, an inert tail sweep, an unresolved lost tail) are protocol-order defects. A checker over every loss pattern at window 3 and 8 packets reaches them in a few steps.
- `raptorpath-core` is planned to hold the I/O-free decisions (ADR-0077). But `shared-core-design.md` keeps the `reliable` clamp and the channel selection in the engine "until owner question 1", and moves neither the tail-sweep target (`net/mod.rs:2789`, `:3035`) nor the intake pause (`net/mod.rs:2473`). A checker over the planned core functions would never run the defect code.
- Kani documents Linux and macOS only.

## Decision

1. **A new workspace member `raptorpath-model`** (`publish = false`, dependency `stateright` 0.31). It stays outside `raptorpath-core`, whose boundary allows only `raptorpath-math`.
2. **First step: a hand-written model** of every seq's state under arbitrary loss, copying the shipped order. Time steps are bounded so every run ends, and the property ("at every terminal state each seq is acked, filled, decoded past or shed, and outstanding ≤ store cap") is checked on terminal states. This does not depend on how stateright treats cycles. The model is checked; its match to the engine is assumed, and the doc says so.
3. **Second step: the model calls the real core decisions.** That needs the extraction to grow: the gap clamp, the tail-sweep target and the intake pause become core functions, with the window bounds and ρ as numbers. `reliable` becomes the clamp's floor, not a bool. These are engine moves under ADR-0077's rules (zero behaviour change, proof ladder, guard batch), and they need shared-core owner question 1.
4. **Kani runs on CI (Linux) for the pure integer functions:** seq wrap, the window clamp, span bounds, `SeqRing` indexing. It verifies no panic or overflow and the stated bounds, under unwind limits. Floating-point laws are out of its scope.

## Consequences

- The hand-written model finds design-level liveness defects cheaply. It can drift from the code until step 3 lands.
- Step 3 removes one existing seat bool from the recovery path (NO-MODE-SWITCH) and gives S8's simulator testable decisions. It costs one guard batch.
- Kani would have caught none of this week's defects. It guards against a later arithmetic bug.
- `raptorpath-model` must be added to `src_manifest` and the harness path tests, as core was.
