# ADR-0059: Per-Path Recovery Clocks — RFC 9002 loss detection generalized per path

## Status: Accepted (`RWM_RECOV_MP` default ON since 2026-07-21; per-path SERIAL namespaces refuted as runtime, retained as diagnostic)

**Date**: 2026-07-21

## Context

Wall #8: with the pool fixed (ADR-0058) and threading refuted (ADR-0057),
the c7 wire was measured SATURATED by the recovery plane itself — retx
share ×1.8 and repair share ×2.2–2.5 the same-config single-path level ≈
exactly the Σ-gap, and dual-c1 sank BELOW single (retx 9.3% of source at
~zero real loss). Root cause: TWO instances of one mistake — recovery
clocks/serials GLOBAL where multipath striping demands PER-PATH. (1) 82%
of c7 retransmits fired inside their flight's own-path RTT clock:
scheduler-created striping gaps read as holes, and retransmits never
reset the clock. (2) Global batch serials poisoned the per-path loss
estimators (0.62–0.77 read at a 0.1%-loss cell).

## Decision

`RWM_RECOV_MP`: rebuild the hole law as RFC 9002 loss detection
generalized per path — a 9/8 time threshold on the LIVE flight's own-path
clock (safety net) + kPacketThreshold=3 same-path delivered successors
(fast channel) + snapshot coalescing for gap reports; retransmits inherit
their own flight clock. Default ON since the consolidation battery.

The companion per-path SERIAL-namespace fix (`RWM_RECOV_MP_SERIAL`) is
**vindicated as diagnosis, refuted as runtime**: honest per-path signals
re-heat every SRTT/loss-scaled cadence and cost ×2.4 sender CPU. Default
OFF, retained as the diagnostic probe arm; the honest-signal cadence
re-derivation is the named follow-up (register: no re-test owed — refuted
on the clean substrate itself).

## Consequences

- The waste is killed at both target cells: c7 retx 14.9→4.5% of source
  (below single-path parity), +5.3/+6.4 Mbit; dual-c1 retx 8.5–9.5% →
  0.3–0.7% and the anti-scaling ELIMINATED (dual above single, both
  seeds). LOO row: removal costs −12.3/−13.9 ≫σ at c7 and returns the
  dual-c1 retx flood.
- The miss is the discovery: the freed wire did NOT convert 1:1 into
  goodput — the ~1.0×Σ c7 target failed at first, moving the Σ-gap's
  residual owner from emission to frontier-recovery latency on the
  ack-serialized store. That named successor became ADR-0060, and the
  pair COMPOSES to 1.018–1.045×Σ.
- Recovery state (clocks, flights, loss estimators) is per-path by
  design; any future recovery mechanism inherits that requirement.

## Evidence

- Ledger: goal-gate.md "Multipath Recovery Suppression (2026-07-21)"
  (per-NACK trace, law, both batteries, serial-fix verdict);
  "Consolidation (2026-07-21)" (LOO flip); "CONSOLIDATED VERDICT" wall
  #8 row.
- Paper: §16.24.
- Commits: 8a34520 (per-flight hole law + serials), a0dbd98 (packet-
  threshold fast channel), 2c632c0 (snapshot coalescing; SERIAL default
  OFF), 6a95193 (ledger verdict), 5ebbcda (default flip).

## References

- ADR-0057 (the profile that named this successor), ADR-0060 (the
  composing successor), RFC 9002 §6.1.
