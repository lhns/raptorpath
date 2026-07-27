# ADR-0064: One Machine Across the δ Axis — the Unified Span Machine + δ-Honest Shedding

## Status: Accepted (`RWM_UNIFIED` default ON, 2026-07-21 — the mode-switch removal ships). Streaming re-test clause DISCHARGED 2026-07-27: unified held the historic crown cell-by-cell (goal-gate "Streaming Crown Re-Test"); streaming deletion GO at the next consolidation pass (register row RE-TESTED/CLEARED; cell-5 p999 WATCH recorded)

**Date**: 2026-07-18 (built) … 2026-07-21 (flipped)

## Context

The principle debt (task #61; paper §16.4 "One Pipeline, Not Mode
Switching"): the transport shipped THREE receive machines — the streaming
two-layer code (Realtime), `RlcWindowDecoder` (plain window), and
`GenerationDecoder` (gen wire) — selected by a hint-keyed mode switch.
The two RLC decoders decode the SAME self-describing wire equations and
differ only in algebra scope: global closure at ~200× cost (plus a proven
rank-loss defect on late sources) vs a block-keyed closure that provably
strands the generic 2-loss burst on moving-span wires. The realtime/bulk
split was an emission-SPAN policy wearing a machine switch's clothes.

## Decision

1. **One decoder** (`fec/unified.rs`): the full global closure WITH the
   sparse-aware cost model (ADR-0056) — known columns payload-only, coded
   rows dense over interval spans, unit rows deliver per-arrival,
   O(k·L·S + k²·(L+S)), block-diagonalizing to the §16.18 bound on
   aligned wires. Differential-proven per-call against all three legacy
   decoders (and it FIXES the legacy rank-loss defect).
2. **One continuous law, no mode switch**: span width
   A* = clamp(rate·D, 1, W) with D = min(H, 2·RTprop); depth
   M* = ceil(rate·2·RTprop/A*)+1; trailing offset Δ = clamp(⌈rate·jitter⌉,
   1, 64) — every parameter from (δ, ρ, r) + measured anchors (ADR-0061).
   Realtime is the small-δ limit; bulk the large-δ limit.
3. **δ-honest overload shedding** (fix C, the flip's last gate): at small
   δ, overload must be shed, not serialized. A hole is sheddable iff its
   projected delivery exceeds the span law's own deadline D(δ) AND
   cumulative shed stays within the derived 1−ρ budget
   (ε̂·(1−P_fec) at the live operating point; receiver bound ε̂_recv);
   past-budget candidates are SERVED — ρ wins over δ. The law is
   compiled OUT on the reliable (ρ=1) contract. No new constants.
4. **Flip discipline**: the first flip attempt (2026-07-19) FAILED its
   pre-registered tail gate (p99 ×2.7–3.3 + a 3/10 stream-collapse
   class) and was refused; the blocker was attributed (NOT the decoder —
   anchor defects + family-level transient amplification), decomposed
   into fixes A (A* anchor), B (clock-gap hygiene), C (shedding), and
   the flip re-ran pre-registered on the full default stack: all five
   predictions confirmed (0 collapse reps; unified ≤ streaming at all
   eight tail rows; 100% delivery at the c3 perf cell vs streaming's
   79/81%; A*-inertness resolved; bulk parity + knee no-regression).
   `RWM_UNIFIED` default ON at commit b849acb.

## Consequences

- The shipped transport is ONE mechanism parameterized by (δ, ρ, r) on
  measured anchors — the paper's central architectural claim is now the
  default binary, with `RWM_UNIFIED=0` as the legacy three-machine
  opt-out arm.
- r* is realized at the realtime wire for the first time (ADR-0063
  chain closed); the #61 completeness trade DISSOLVED (100% delivery at
  ×1.2 completer cost, was ×3–4).
- The streaming machine loses the Realtime default but is RETAINED: its
  12–48× historic tail crown spans cells this battery did not re-run —
  retirement sits in the deprecation register behind a cell-by-cell
  re-test clause (ADR-0066; triage: VISION-TRIAGE-2026-07). The legacy
  RLC decoders retire when the same pass confirms unified ≥ legacy
  everywhere on the historic cells.
- Named follow-ups: within-deadline P_fec refinement of the ρ budget;
  the environment-bound extreme-stall class (collapses ALL machines,
  streaming included); c7 unified −5 Mbit direction (0.6σ) watched.

## Evidence

- Ledger: goal-gate.md "Unified Decoder (2026-07-18)" (derivation,
  differential + oracle PART 7, L0/L1 batteries, the honest #85-VOID
  finding, COLLAPSE ATTRIBUTION), "Unified Shedding + Flip Battery
  (2026-07-21)" (pre-registration, shed law, L0/L1 results, flip).
- Paper: §16.20 (machine), §16.26 (shedding + flip), §17.5 (superseded
  three-machine map).
- Commits: 206b90c (derivation), 1eec34d (UnifiedDecoder), 28138b9
  (gate), a54cbf5 (oracle PART 7), eb9fae4 (first flip NO), 326db4f
  (collapse attribution), 988960c (fixes A+B), 120d8f8 (pre-reg),
  6568822 (shedding), b849acb (FLIP ON), c3a9d76 (battery record).

## References

- ADR-0056 (cost model), ADR-0061 (anchors), ADR-0063 (r*), ADR-0022 /
  ADR-0027 (the sliding-window and streaming machines this unifies /
  displaces), ADR-0067 (the stack it ships on).
