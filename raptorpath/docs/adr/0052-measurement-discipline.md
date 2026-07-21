# ADR-0052: L1 Measurement Discipline (liveness proof, pre-registration, era honesty)

## Status: Accepted (binding for every L1 verdict)

**Date**: 2026-07-13, extended through 2026-07-21

## Context

The generation-inert era (ADR-0053): six mechanism verdicts were merged on
measurements in which the mechanism under test never executed, because
nobody checked. Separately, the arc accumulated further measurement
failure classes: a datum voided because repairs were emitted but silently
dropped by a mismatched-backend decoder (#85 span probe); battery arms
silently lost to `set -e` pipelines; same-nominal-config session drift of
2.3× that exceeded every claimed DAPS-era effect; a hardware divide
(qemu64/SSSE3 → passthrough E5-2650 v3 with AES-NI/AVX2) that changes
what absolute numbers mean; and a build (bounded borrowing, §16.22) whose
own derivation already predicted its failure before it was built.

## Decision

No L1 verdict is eligible for merge unless the ledger section carries all
of the following (goal-gate.md "MEASUREMENT DISCIPLINE", items 1–11):

1. **Mechanism-liveness proof** — the recorded run shows the mechanism
   executed (harness `cod>0` guard, enabling-flag echo). Dead code
   measures noise.
2. **Full command line + env + binary hash recorded.**
3. **Interleaved same-binary arms** within one session (documented drift
   2.3× otherwise).
4. **Both seeds + per-run distributions** (pooled means hide bimodality).
5. **Effect must exceed the recorded noise floor** (σ_s and cross-session
   drift).
6. **Liveness proven at the RECEIVER too** (decoder echo,
   `repairs_useful > 0`), not only sender emission counters.
7. **Harness arm-liveness under `set -e`** — per-arm result-count
   assertions; an arm that produced zero summaries fails loudly.
8. **Known abort classes recorded** (seed-7 topo-ping double-abort): n
   per arm quoted, no captured result discarded.
9. **Hardware era is part of the config** — `lscpu` in every log header;
   cross-era comparisons must name the divide.
10. **Ops: CRLF conversion on VM sync** before the first harness
    invocation.
11. **Pre-registration** — before any build, the ledger records the
    mechanism, predicted effect size + cells, the falsification
    condition, and a re-read of the derivation for self-contained failure
    predictions. A build whose prediction fails defaults to the
    deprecation register (ADR-0066), not iteration, unless the failure
    itself names a new mechanism.

Additionally: the env-parse footgun is closed in code —
`config::env_flag` makes `=0`/`=false` OFF for every boolean gate
(commit 2de7589).

## Consequences

- Every 2026-07-14…21 battery (anchors, unified, recovery suppression,
  SACK release, consolidation, shedding) ran under this protocol; the
  three default-flip decisions of 2026-07-21 (ADR-0060, ADR-0064,
  ADR-0067) were pre-registered with fixed flip rules before results.
- Verdicts predating the discipline are cited only with their era named;
  the DAPS-era sections are audit-classified (ADR-0053) rather than
  trusted or silently deleted.
- The discipline is a merge gate, not advice: a section missing any item
  is not a verdict.

## Evidence

- Ledger: goal-gate.md "MEASUREMENT DISCIPLINE (2026-07-13)" (items 1–11
  with the incident that created each).
- Audits: docs/audits/2026-07-13-verdict-audit.md,
  docs/audits/2026-07-13-session-audit.md.
- Commits: 2de7589 (env_flag), bd13985 (harness arm-liveness), 161aff1
  (hard `cod>0` guard), 120d8f8 / 7145fcc (first pre-registrations under
  item 11).

## References

- ADR-0053 (the era this discipline answers), ADR-0044 (benchmark
  methodology, the L0-era precursor), paper §16.15.
