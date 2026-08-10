# ADR-0066: The Deprecation Register — two-stage retirement for refuted mechanisms

## Status: Accepted (register live 2026-07-21; per-gate dispositions below; EXECUTED 2026-07-27 — the no-re-test rows + the argued DAPS/SRC_BP rows deleted on `refactor/consolidation`, per-row commits in goal-gate "Code Consolidation (2026-07-27)"). **FULLY EXECUTED 2026-07-27, consolidation pass 2 (`refactor/consolidation-2`): the two re-test clauses were both discharged same-day WITH DATA and their code deleted — FMTCP re-tested on the clean substrate → CONFIRMED-REFUTED → f841757; streaming crown re-test → CLEARED → bccb32a (scoped streaming-only; `RWM_UNIFIED=0` now = legacy-RLC). No Class-C gate remains in the tree; the two-stage discipline itself (deprecate → re-test → delete) stays the standing rule for future refuted mechanisms**. **RE-OPENED 2026-08-10: one Class-C row added — BLOCK MODE, the first non-env member, re-test OPEN (ADR-0069). Same day, one row added and RETIRED unmeasured — `RWM_SCHED_SNAPSHOT`, deleted with NO re-test clause because its premise was refuted by reading the code it shipped in, not by a measurement whose substrate could go stale (goal-gate "Scheduler-Snapshot Adjudication").**

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

**Disposition added 2026-08-10 — the register's first NON-env row:**

- **BLOCK MODE** (the `window_reliable = false` pipeline for Bulk/Auto:
  block assembly + RaptorQ/RS/block-RLC + P8 block-ARQ + interleaver) —
  DEPRECATED, re-test OWED, code FROZEN not deleted. Last measured as an
  arm 2026-07-08 (goal-gate "Full Benchmark Re-Run", C1–C5, C4 DNF 6/6
  still flagged) with **W1, W2, W7, W8 and PRE-DIV all ACTIVE** — the
  same wall profile that made `RWM_FMTCP` the strongest re-test case —
  yet it remains the SHIPPED DEFAULT while every battery since
  2026-07-12 has measured the window pipeline. Counter-weighted by the
  only head-to-head on record, which block WON (C2 1.23×, 2026-07-06).
  Re-test clause, flip rule and removal list: **ADR-0069**. Note the
  register's enforcement mechanism does not reach this row —
  `deprecated_env_flag` warns on an env gate, and block mode is the
  unset state of a CLI flag; the hook is instead the routing pin
  `net::tests::default_config_routes_bulk_and_auto_to_the_block_pipeline`,
  which fails if the default moves without its measurement.

**Disposition added 2026-08-10 — DELETED SAME DAY, no re-test owed (the
register's first row retired by READING rather than by measurement):**

- `RWM_SCHED_SNAPSHOT` (`net/sched_snapshot.rs`, the net-seam-pass-2
  per-iteration scheduler snapshot; shipped OFF 2026-08-09, never
  measured) — **DELETED unmeasured, NO re-test clause.** The register's
  standing rule is deprecate → re-test → delete, and the rule exists
  because a refutation is only as good as its substrate. This row is
  exempt for the one reason that can justify the exemption: it was not
  refuted by a measurement on a possibly-stale substrate, it was refuted
  by its OWN premise not being reachable in the code it shipped in. The
  ledger section "Scheduler-Snapshot Adjudication (2026-08-10)" carries
  the six findings; in short, the "BDP that never existed" it claimed
  to prevent cannot be composed from the five sites it served (each
  already reads under ONE acquisition; the only rate×RTprop product,
  `copa_bdp_anchor`, is atomic inside one `CopaState`), the phases it
  served are independently ~5 ms-throttled and so would have consumed
  DIFFERENT snapshots anyway, and at the one genuinely skew-exposed site
  (the post-`select!` deficit-spacing read) a loop-top capture is
  STRICTLY STALER than the per-phase read it replaced. Measuring an arm
  whose mechanism is absent produces a null that means nothing, so no
  battery is owed and none is queued. **What a future proposal must
  differ in** (recorded so this is not re-derived blind): capture per
  phase-group AFTER the await, not once at the loop top. Removal:
  `net/sched_snapshot.rs`, the `RuntimeGates::sched_snapshot` field +
  echo + default-stack assertion, the `RWM_FORWARD` entry, and the five
  `match sched_snap` sites unwrapped to their OFF arms — VERIFIED
  character-for-character (whitespace-stripped) against
  `17f7fa9:raptorpath/src/net/mod.rs`, so the executed default path is
  exactly what main executes; the compiled output legitimately differs,
  because main also compiled an always-false `Option` test at each site
  and a capture the default never reached. **Finding 6, which is
  INDEPENDENT of this gate and outlives it:** the deleted module's unit
  test named the `active_paths()`/`live_paths()` swap as "exactly the
  failure mode here" and was structurally incapable of detecting it — its
  fixture only ever added FRESH paths, where `in_flight = 0 < cwnd` makes
  the two sets identical, so either accessor passed. A test that asserts
  an invariant only over states where the invariant is degenerate proves
  nothing however exactly it is written, and naming the failure mode in a
  comment is not constructing the state that exhibits it. It is
  therefore REPLACED, not dropped, by
  `scheduler::tests::saturated_path_is_live_but_not_active`, which pins
  the divergence under saturation and asserts the fresh-path trap
  explicitly. Nothing in the crate asserted this distinction before,
  though shipped code is load-bearing on it.

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
