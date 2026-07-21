# ADR-0065: The DAPS-Era Refutations (delay-aware scheduling chain)

## Status: Refuted / Void (era verdicts Void-or-Uncertain per audit; live re-test refuted the stack; gates deprecated-warned, retained pending the register's re-test clause)

**Date**: 2026-07-12 (era) / 2026-07-13 (audit + live ablation) / 2026-07-21 (register disposition)

## Context

The 2026-07-12 arc built a delay-aware multipath stack for generation
mode: `RWM_DAPS` (delay-aware path scheduling + right-sized FEC),
`RWM_DAPS_BDP` (per-path BDP cap), `RWM_DAPS_PACE`/`RWM_PACE_ALL`
(per-path BtlBw pacing of source and repair), `RWM_SRC_BP` (source
backpressure), `RWM_PER_PATH_EST` (per-path delivered-rate estimator),
`RWM_RATE_SAMPLE` (BtlBw rate-sample fix), `RWM_DAPS_DEPTH` (read-ahead
depth bound). Its ledger sections reported effects from +52% to −53%.

## Decision (what the record now says)

1. **The era's verdicts are VOID or UNCERTAIN** (ADR-0053): the harness
   never enabled generation, so the DAPS-era A/B arms compared
   byte-identical behavior; §16.14 is proven-inert (`cod=0` in its own
   logs), §16.10–16.13 are unverifiable (no recorded env); every claimed
   delta sat inside the 2.3× era noise.
2. **The LIVE refutation** ("Gen-ON Stack Ablation", 2026-07-13, with
   generation actually ON, guard-verified): the symmetric C7 collapse
   21→12 IS the stack — rate-sample −22%, depth −20…−30% — NOT the
   coding (gen-bare ≈ plain and keeps ×1.35 aggregation). Defaults
   flipped OFF there. Follow-ups sharpened it: the "slow anchor never
   establishes" claim was itself refuted (it establishes but swings
   ~4000× on decode-clocked samples); `RWM_SRC_BP` was refuted at L1
   (−53% both seeds — source is the pipeline clock, not a holdable
   emitter); depth-bounding was "the correct lever that cannot bind"
   without an honest slow anchor.
3. **The mechanism space was superseded**, not abandoned: per-path BDP
   caps + derived depth live on as `RWM_GEN_PIPE`'s M* law (ADR-0064);
   honest per-path rate anchors became ADR-0061 (`RWM_PLAIN_RS`, the
   send-interval sampler); the admission question was re-asked on live
   code by the percap family and settled at ADR-0058; the substrate wall
   under the whole era was Cubic (ADR-0054).
4. **Register disposition** (ADR-0066): all seven gates
   deprecated-warned, retained. Re-test formally owed (the era refuted
   them with walls W1/W2/W7/W8 + GEN-INERT + PRE-DIV active) but LOW
   priority, argued honestly: the live ablation already re-tested the
   same mechanism space on live code; DAPS is generation-mode-only while
   the shipped default is plain-mode; `RWM_DAPS_DEPTH` keeps its one
   live win (hetero C8 +8%) as a gen-mode opt-in. A deletion decision
   rides the next generation-mode consolidation battery.

## Consequences

- Seven env gates and ~450–700 LOC of scheduling/pacing machinery remain
  in the tree as warned opt-ins with no default role; the
  code-consolidation triage (VISION-TRIAGE-2026-07) recommends removal
  without further VM re-test, with the counter-argument recorded.
- The era is the reference case for measurement discipline (ADR-0052)
  and for the register's walls-active column: "refuted" and "measured
  dead" are different facts, and both are recorded.

## Evidence

- Ledger: goal-gate.md "DAPS + Right-Sized FEC", "DAPS Queue Management",
  "Per-Path Estimator", "Pace-All Traffic", "Source Backpressure",
  "BtlBw Rate-Sample Fix", "DAPS Read-Ahead Depth" (all 2026-07-12,
  bannered), "Slow-Path Anchor Diagnosis", "Gen-ON Stack Ablation"
  (2026-07-13); DEPRECATION REGISTER row "DAPS chain".
- Audits: docs/audits/2026-07-13-verdict-audit.md.
- Paper: §16.10–§16.14 (bannered), §16.15–§16.16.
- Commits: 226bca7, cd2882e, 824461c, 3444997, 4606829, 11e0f5e,
  68d6b6c (the era), d63ffce (diagnosis), d1b3f78 (live ablation),
  3bcf869 (register).

## References

- ADR-0053 (audit), ADR-0052 (discipline), ADR-0066 (register),
  ADR-0058/0061/0064 (the successors that inherited the space).
