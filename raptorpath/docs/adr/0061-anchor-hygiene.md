# ADR-0061: Anchor Hygiene — three laws for a measured anchor

## Status: Accepted (M*-pair + clock-gap members default ON since 2026-07-21; A* anchor default ON under `RWM_UNIFIED`; `RWM_PLAIN_RS` retained gated)

**Date**: 2026-07-19

## Context

Three independent investigations converged on one defect family: the
unified-realtime collapse attribution (A* span anchor pinned at 1 for
~10 s by a cold 2-s EWMA, and flood-poisonable), the #61 knee battery
(M* unreachable behind a 50-ms RTprop floor — traced to the PEER-REPORT
feedback loop recording the peer's own 50-ms-seeded ESTIMATE as a local
RTT sample every ~2 s, re-planting a perpetual floor inside the 10-s
min-window — plus a static `(pipeline+2)·G` win backstop), and the percap
guard results (plain-mode BtlBw over-reading ×4.6–7.4 from ack-interval
sampling). Every derived law in the system keys on measured anchors;
broken anchors silently convert derived laws into constants.

## Decision

**The principle — an anchor is trustworthy only if:**

1. **Measured-seed**: seeded from measured sends — a windowed statistic of
   real samples, live within ~1 RTT, never a static default surviving
   warm-up (and: estimates are not samples — peer-reported estimator
   values must never enter a local sample window).
2. **Clock-gap discard**: samples spanning a PROCESS-clock stall are
   discarded, not averaged (a process-global `StallWitness`; the
   ack-arrival-clock detector design was built first and REFUTED by
   measurement — ack silences of 0.5–3 s are normal protocol behavior).
3. **Expiring floors**: floors/backstops expire — a floor that outlives
   its min-window is a constant wearing a floor's clothes (the FMTCP win
   backstop becomes the derived `(M*+2)·G` once anchors are live).

**The four fixes** (env-gated, `RWM_ANCHOR_HYGIENE` umbrella):
`RWM_ASTAR_ANCHOR` (windowed-max send-rate anchor with gap quarantine),
`RWM_MSTAR_ANCHOR` (peer-report RTT-feed suppression, seed-from-first-
sample, derived backstop), `RWM_PLAIN_RS` (the #79 send-interval sampler
generalized to plain mode, sampling-only CopaFeed), `RWM_CLOCK_GAP`
(process-clock stall witness at the shared sampling layer).

## Consequences

- The M* knee ENGAGES at L1: c2r100 +31/+25%, c2r200 +82/+62%, non-
  overlapping per-rep distributions at r200 — oracle PART 7b confirmed in
  direction and ordering. A* live in ~1 RTT and flood-poison-proof;
  plain-mode BtlBw reads ≈1× truth (was ×4.6–7.4).
- Defaults: `RWM_MSTAR_ANCHOR` + `RWM_CLOCK_GAP` flipped ON in the
  consolidation LOO battery (measured free at every bulk cell, tail crown
  unregressed, wins elsewhere); `RWM_ASTAR_ANCHOR` ships ON under
  `RWM_UNIFIED` (ADR-0064 — its liveness was a flip-gate). `RWM_PLAIN_RS`
  stays gated: its honest anchor is load-bearingly ENTANGLED with the
  legacy store-cap circularity (sc2 −20% as a cwnd anchor; resolved as a
  cap input), a named flip candidate riding the c8-aware pool follow-up.
- Anchor-defect classes now have named laws and unit injections
  (flood-poison, seeding, witness quarantine); new anchors must satisfy
  the three laws or say why not.

## Evidence

- Ledger: goal-gate.md "Anchor Hygiene (2026-07-19)" (principle, fixes,
  L0 + L1 batteries, gate-readiness); "Unified Decoder" → COLLAPSE
  ATTRIBUTION (the A* defects); "Consolidation (2026-07-21)" (LOO flips).
- Paper: §16.21.
- Commits: 988960c (fixes 1–4), d6bed88 (knee verdict), 5ebbcda (default
  flips), 6568822 (A* default ON under RWM_UNIFIED).

## References

- ADR-0064 (the span law these anchors feed), ADR-0058 (the cap laws),
  ADR-0052 (the discipline that caught the arrival-clock refutation).
