# ADR-0063: r* Window-Mass Provisioning (§8.4.1) and the Wire-Realization Chain

## Status: Accepted (`RWM_RSTAR_TAIL` default ON since 2026-07-13; wire realization closed 2026-07-21 via the unified machine)

**Date**: 2026-07-13 (solver), 2026-07-18/19 (taper + entanglement), 2026-07-21 (realized)

## Context

r* was derived for GE-geometric bursts, but real traces carry burst-length
tails 3.8–26× heavier than geometric AND burst clustering, so delivered
window-failure missed the δ/ε target by 2–4× (worst 12.8×) even at
55–100% overhead — breaking the (δ, ρ) contract exactly where it is
tightest (realtime). A first implementation using the burst-LENGTH
quantile passed the synthetic but still missed on real traces.

## Decision

1. **Provision the window loss-MASS quantile** (paper §8.4.1): a window
   of N slots fails iff total losses K_N > R, so the estimator tracks
   sliding m-block mass tails at the window's own scale (m = 1..8 × 64
   slots), extends beyond observation with a discrete-Weibull fit whose
   k=1 IS the geometric law (a GE channel measures itself back — no new
   contract parameters), and the solver takes the least r with
   F(r) ≤ δ/ε; production emits max(r*_§8.4, r*_mass). Level-rescale ties
   the tail to the current BOCD level estimate (shape keeps long memory,
   level adapts at estimator speed). Infeasible contracts return the
   ceiling — DECLARED, not silently missed.
2. **Ship default ON** (`RWM_RSTAR_TAIL=1`); Bulk's χ=0 identity
   (r*(δ=ε̂)=0) survives — the cost lands only on tight-δ profiles on
   measured-bursty channels.
3. **The realization chain, recorded honestly.** The L1 spot check found
   the wire arms indistinguishable: the corrected r* was DILUTED by the
   plain-mode emission path (taper reset per ack cycle ⇒ ~r per CYCLE,
   not per symbol). The quantity law was fixed as `TaperBudget`
   (`RWM_TAPER_R`, #85: the wire consumes r as computed) — and consuming
   r then DEGRADED streaming-family delivery (−19/−25 pp both seeds,
   both rungs): the leading-window (unsolvable-span) entanglement, plus
   spare-cap compression. The flip stayed closed until the unified
   machine's trailing solvable span (ADR-0064) provided the span that
   decodes: at the 2026-07-21 flip battery cod/src 0.38–0.50 is consumed
   at the realtime wire and BUYS measured 100% delivery. `RWM_TAPER_R`
   now rides the unified umbrella.

## Consequences

- Oracle validation: feasible real-trace cells worst residual 2.88× →
  1.41×; GE control tracks §8.7 exact (×0.92–1.11); heavy-tail synthetic
  5.1×-miss → 0.99×-hit; 6/10 infeasible cells now DECLARED (feasibility
  needs W growth or ARQ — a contract renegotiation, not a solver fix).
- One principled gate recalibration (C8-dual-asym bound 1.1 → 1.15 — the
  physics of honestly priced overhead), 15/15 green.
- The chain is the arc's cleanest example of instrument-vs-mechanism
  separation: solver correct at the oracle, inert at the wire, quantity
  fixed, span named, realized one layer up — each step measured.

## Evidence

- Ledger: goal-gate.md "r* Bursty-Loss Provisioning (2026-07-13)"
  (derivation, trace tables, L1 spot check), "Taper Emission Fix
  (2026-07-18)" (budget law, 2×2, entanglement L0+L1), "Unified Shedding
  + Flip Battery (2026-07-21)" battery 2 (r* realized).
- Paper: §8.4.1, §16.20.3–.4, §16.26.
- Commits: fc104b6 (mass-quantile solver), 4538a9b (level rescale),
  88f94eb (L1 dilution attribution), 33b29f8 (TaperBudget), b317983 +
  4b8e538 (L1 2×2), b849acb (realization at the unified default).

## References

- ADR-0050 (BOCD estimator layer), ADR-0064 (the span that made r
  consumable), ADR-0023 (GE model).
