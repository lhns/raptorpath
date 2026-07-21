# ADR-0053: The Generation-Inert Era — audit, classification, and the hard guard

## Status: Accepted (methodology lesson; the era's verdicts are Void/Uncertain)

**Date**: 2026-07-13

## Context

The 2026-07-12 DAPS-era arc (paper §16.10–§16.14; seven ledger sections)
merged mechanism verdicts for delay-aware scheduling, per-path pacing,
source backpressure, rate-sample anchors and read-ahead depth. On
2026-07-13 it was discovered that the L1 harness never enabled generation
mode: `perf_rwm_c.sh` passed only `--window-reliable`, and every DAPS-era
mechanism chains off the `generation` gate — so every A/B toggle compared
byte-identical behaviour against itself. Compounding: `RWM_*` `.is_ok()`
env gates counted `=0` as ON; the era's ledger sections recorded no
command lines or env; era noise (2.3× same-config spread) exceeded every
claimed effect; and §16.14's mechanism DIAG was read from the wrong
process's log.

## Decision

1. **Audit, classify, retain.** A full verdict audit classifies each
   section VALID / INVALID / UNCERTAIN
   (docs/audits/2026-07-13-verdict-audit.md; session-level error audit in
   2026-07-13-session-audit.md). §16.14 is INVALID-proven (saved logs show
   `cod=0`); §16.10–§16.13 are UNCERTAIN (no recorded env — they can be
   neither retro-validated nor definitively voided). The sections are
   bannered in place, NOT rewritten or deleted: the audit trail is the
   method's proof.
2. **Hard guard in the harness.** Any generation-requested run whose
   sender log shows cumulative `total_coded = 0` fails FATAL ("mechanism
   inert"). The harness passes the generation flag explicitly, gated
   `RWM_GEN`.
3. **Re-baseline before re-verdict.** The first valid generation-ON
   ceilings and C7/C8 numbers were measured fresh ("Generation-ON
   Re-Baseline"), and the DAPS-era stack was re-ablated on live code
   ("Gen-ON Stack Ablation") before any of its mechanisms were re-judged
   (ADR-0065).
4. **The discipline is codified** as ADR-0052 (binding checklist) and the
   env-parse fix (`config::env_flag`).

## Consequences

- Six merged verdicts lost their evidentiary status; the mechanisms were
  re-tried on live code where the mechanism space still mattered, and the
  live results superseded the era (ADR-0065).
- The class of bug cannot recur silently: liveness is asserted by the
  harness, and absence of a recorded env is now itself a merge-blocker.
- The audit is the reference precedent for the deprecation register's
  "walls active at refutation" column (ADR-0066): a refutation measured
  on dead code or a walled substrate is not a deletion warrant.

## Evidence

- Ledger: goal-gate.md "Methodology Audit (2026-07-13)", "Generation-ON
  Re-Baseline (2026-07-13)", "Gen-ON Stack Ablation (2026-07-13)".
- Audits: docs/audits/2026-07-13-verdict-audit.md,
  docs/audits/2026-07-13-session-audit.md.
- Commits: 161aff1 (re-baseline + hard guard), d82600e / 50071d8 (voided
  verdicts stamped), 2de7589 (env-parse fix), d1b3f78 (live ablation).

## References

- ADR-0052 (the binding discipline), ADR-0065 (the DAPS-era mechanism
  dispositions), paper §16.15.
