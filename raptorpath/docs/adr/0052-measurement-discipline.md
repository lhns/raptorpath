# ADR-0052: L1 Measurement Discipline (liveness proof, pre-registration, era honesty)

## Status: Accepted (binding for every L1 verdict)

## Context

Six mechanism verdicts were once merged on measurements in which the
mechanism under test never executed (the "generation-inert era": the harness
never checked that generation coding actually coded). Other failure classes
followed: repairs emitted but silently dropped by a mismatched-backend
decoder; battery arms silently lost to `set -e` pipelines; same-config
session drift (2.3×) larger than every claimed effect; a hardware change
(qemu64/SSSE3 → passthrough with AES-NI/AVX2) that changed what absolute
numbers mean; and a build whose own derivation predicted its failure before
it was built.

## Decision

No L1 verdict is eligible for merge unless it satisfies the rules in
[`docs/measurement-discipline.md`](../measurement-discipline.md): mechanism
liveness proven at sender and receiver, full command/env/binary recorded,
interleaved same-binary arms, both seeds with per-run distributions, effects
above the recorded noise floor, per-arm result-count assertions, recorded
abort classes, the hardware era in every log, a CR-free synced tree, and
pre-registration (mechanism, predicted effect and cells, falsification
condition) committed before any build or VM contact. That document also
defines the verdict taxonomy and the VM protocol.

A build whose pre-registered prediction fails goes to deprecation, not
iteration, unless the failure itself names a new mechanism. Refuted
mechanisms retire in two stages: deprecate (the gate warns on activation)
with a re-test clause where the refuting substrate may have been broken →
re-test on the current substrate → delete.

The env-parse footgun is closed in code: `config::env_flag` makes `=0` /
`=false` OFF for every boolean gate.

## Consequences

- The discipline is a merge gate, not advice: a result missing any item is
  not a verdict.
- Verdicts predating the discipline are cited only with their era named.

## Evidence

- Commits: 2de7589 (`env_flag`), bd13985 (harness arm-liveness), 161aff1
  (hard `cod>0` guard), 120d8f8 / 7145fcc (first pre-registrations).
- Incident history per rule: the ledger at ac1aed1, section "MEASUREMENT
  DISCIPLINE".
