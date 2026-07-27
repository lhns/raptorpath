# ADR-0066: The Deprecation Register — two-stage retirement for refuted mechanisms

## Status: Accepted (register live 2026-07-21; per-gate dispositions below; EXECUTED 2026-07-27 — the no-re-test rows + the argued DAPS/SRC_BP rows deleted on `refactor/consolidation`, per-row commits in goal-gate "Code Consolidation (2026-07-27)"). **FULLY EXECUTED 2026-07-27, consolidation pass 2 (`refactor/consolidation-2`): the two re-test clauses were both discharged same-day WITH DATA and their code deleted — FMTCP re-tested on the clean substrate → CONFIRMED-REFUTED → f841757; streaming crown re-test → CLEARED → bccb32a (scoped streaming-only; `RWM_UNIFIED=0` now = legacy-RLC). No Class-C gate remains in the tree; the two-stage discipline itself (deprecate → re-test → delete) stays the standing rule for future refuted mechanisms**

**Date**: 2026-07-21

## Context

The consolidation pass had to decide what to do with a decade's worth (in
project time) of refuted mechanisms. The project's own history forbids
naive deletion: DAPS was "refuted" on dead code (ADR-0053); FMTCP's
"strictly worse" was measured before the MTU-wedge/pool/recovery walls
existed. A refutation is only as good as the substrate it was measured
on.

## Decision

**Two-stage retirement (Class-C gates):** deprecate (the gate `warn!`s on
activation naming its refuting section, via `config::deprecated_env_flag`)
→ re-test-on-clean-substrate where owed → delete. Nothing is deleted in
the pass that refutes it; deletion requires the re-test clause satisfied
on the consolidated stack. Each register row must argue which walls
(W1 Cubic, W2 MTU wedge, W7 pool law, W8 global recovery clocks,
GEN-INERT, PRE-DIV) were ACTIVE at the refutation.

**Dispositions recorded 2026-07-21** (full table: goal-gate.md
DEPRECATION REGISTER):

- `RWM_FMTCP` (+`_WIN`) — refuted pre-EVERY-wall; **the strongest
  re-test case** (its named failure mechanism is exactly the W7/W8
  class), counter-weighted by reproducing FMTCP's own published
  pathology. Retained pending clean-substrate re-test.
- DAPS chain — re-test owed formally, LOW priority (the live ablation
  already covered the mechanism space): ADR-0065.
- `RWM_SRC_BP` — YES in principle, LOW (space superseded by the percap
  family, ADR-0058).
- `RWM_SACK_PRUNE` — **NO re-test: deprecate-HARD**, removal next pass;
  the unsafety is structural and the goal is achieved safely by
  ADR-0060.
- `RWM_RECOV_MP_SERIAL` — NO re-test (refuted ON the clean substrate);
  retained as diagnostic probe (ADR-0059).
- `RWM_INLINE_REPAIR`, `RWM_FRONTIER*` — NO on supersession grounds
  (goal achieved by `RWM_PROACTIVE_PACER`, whose own null resolved into
  the structural presence⊥throughput identity); retained as documented
  negatives / diagnosis arms.
- `RWM_RATE_WIRE` (+`RWM_RATE_Q`) — NO (refuted by the sample-clocking
  argument, not a wall); need met by the honest-anchor family
  (ADR-0061).
- **STREAMING MACHINE** — not refuted, DISPLACED (ADR-0064). Retained
  as the `RWM_UNIFIED=0` opt-out with NO warning; retirement governed
  by the re-test clause: the 12–48× crown record spans historic cells
  the flip battery did not re-run — code removal requires a later pass
  holding that record cell-by-cell on the unified default.

Class-B gates (concept incomplete, successor scheduled — percap family,
`RWM_COPA_COMPETE`, formerly `RWM_TAPER_R`/`RWM_UNIFIED` before they
flipped ON) are NOT register members: they deprecate or flip when their
successor's battery settles.

## Consequences

- Every refuted gate now warns with a pointer to its refuting evidence;
  nothing can be silently resurrected or silently deleted.
- The register is the code-consolidation pass's input: the no-re-test
  rows (SACK_PRUNE hard; RECOV_MP_SERIAL; INLINE_REPAIR; FRONTIER*;
  RATE_WIRE) are the deletion work-list; the re-test rows (FMTCP, DAPS,
  SRC_BP) require an explicit decision, argued in
  VISION-TRIAGE-2026-07.md.
- Discipline item 11 (ADR-0052) feeds the register: a failed
  pre-registered prediction defaults here, not to iteration.

## Evidence

- Ledger: goal-gate.md "DEPRECATION REGISTER (2026-07-21)" (the argued
  table — linked, not copied); refuting sections per row.
- Commits: 3bcf869 (register + activation warnings + item 11), c3a9d76
  (streaming-machine entry).

## References

- ADR-0052/0053 (why refutations need provenance), ADR-0059/0060
  (clean-substrate refutation vs structural supersession examples),
  VISION-TRIAGE-2026-07.md (the removal recommendations).
