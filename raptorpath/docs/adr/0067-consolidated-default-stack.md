# ADR-0067: The Consolidated Default Stack — the shipped default IS the best-measured configuration

## Status: Accepted (defaults landed 2026-07-21)

**Date**: 2026-07-21

## Context

The default-honesty scandal: by mid-July every measured winner sat
default-OFF (each gated on a per-knob clean sweep while the features
interact), so the shipped binary with env unset was the WORST measured
configuration — stock Cubic under a legacy 1024 pool with global
recovery clocks. Users of the default were running the condemned arms.

## Decision

The shipped default, with every `RWM_*` env unset, is the composed
best-measured stack, and every member must defend its place with a
pre-registered leave-one-out (LOO) row on both seeds (flip rule fixed
before the battery: a member joins iff removal HURTS, or is neutral
while the member wins elsewhere, with no cell regressed ≫σ):

- BBR-under (`RWM_QUIC_CC` unset ⇒ bbr) — ADR-0054
- MTU floor 1350 — ADR-0055
- SACK-clocked store release (`RWM_STORE_SACK_RELEASE`) — ADR-0060
- Path-scaled outstanding pool (`RWM_STORE_PATHS`) — LOO: removal
  re-opens the c7 collapse class — ADR-0058
- Multipath recovery suppression (`RWM_RECOV_MP`) — LOO: −12.3/−13.9 ≫σ
  at c7, dual-c1 retx flood returns — ADR-0059
- Anchor-hygiene pair (`RWM_MSTAR_ANCHOR`, `RWM_CLOCK_GAP`) — LOO:
  measured free at bulk cells, wins elsewhere ("neutral + wins
  elsewhere" clause) — ADR-0061
- Corrected r* solver (`RWM_RSTAR_TAIL`) — ADR-0063
- The unified span machine + shedding (`RWM_UNIFIED`, + `RWM_TAPER_R`
  and `RWM_ASTAR_ANCHOR` under its umbrella) — ADR-0064

`RWM_PLAIN_RS` was probed (its witness cost resolved in composition,
best-or-equal c8 arm at s7) but NOT flipped — the default bar is the full
LOO criterion and it was probed at one cell; it rides the c8-aware pool
follow-up battery as a named flip candidate.

## Consequences

- Measured at the default (env fully unset, all mechanism echoes live):
  c7 0.982–0.988×Σ; dual-c1 +15 above single with retx ×10 down; singles
  and fairness class unchanged; the 12–48× tail crown survives the stack;
  default smoke c7 167.7, sc2 84.0.
- The one place the default is knowingly not best-measured: heterogeneous
  c8 (the ADR-0058 c8 WATCH — legacy pool + SR reads 0.85–0.87×Σ vs the
  stack's 0.72–0.76) — a named, pre-registerable follow-up worth +11–13
  Mbit, deliberately left to its own battery rather than special-cased.
- Legacy behaviors remain the explicit `=0` opt-out arms; "byte-identical
  default" language in older sections is era-scoped (identity claims for
  other gates compare on the same substrate both sides).
- Precedent: default flips are batch-composed and LOO-defended, not
  per-knob; the pile-up failure mode (winners stranded default-OFF) is
  structurally closed.

## Evidence

- Ledger: goal-gate.md "Consolidation (2026-07-21)" (pre-registration,
  31-arm LOO battery, per-member verdicts, default smoke); "CONSOLIDATED
  VERDICT" §"Default honesty" + "What the transport measures today".
- Paper: §17.7.
- Commits: 5daceab (harness), 5ebbcda (flips landed), 1e2cb9e (paper
  §17.7 + smoke), b849acb (unified joins the default).

## References

- Member ADRs 0054, 0055, 0058, 0059, 0060, 0061, 0063, 0064; ADR-0052
  (pre-registration discipline); ADR-0066 (where the non-members went).
