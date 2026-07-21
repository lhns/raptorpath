# ADR-0057: Profile Before Parallelize — the crypto and threading refutations

## Status: Accepted (as method; three threading refutations + one crypto refutation on the record)

**Date**: 2026-07-14 / 2026-07-19

## Context

Two standing hypotheses said the transport was compute-bound: (a) crypto —
software AES-GCM on every packet in the qemu64 era; (b) the
"single-thread receiver ceiling ~93–104 Mbit/s" — task #84 shipped with a
receiver-parallelization plan, and the lever went "live" again at ~147
(#86). Both were attributions nobody had profiled.

## Decision

Measure first; build only what the profile demands. Applied three times,
the answer was NO each time, and each refusal named the real wall:

1. **Crypto (wall #5) — REFUTED.** The hardware divide itself was the
   instrument: AES-NI/AVX2 cut CPU 30–38%/byte and moved NOT ONE
   throughput cell. A CPU-bound wall must move when the CPU gets ~35%
   faster per byte; none did ("Hardware-Honest Re-Baseline").
2. **Receiver threading (wall #6) — REFUTED twice at #84's cells.** Flat
   profile (top symbol 3.9%), server pinned to ONE core loses only 8% at
   0.66 core busy; the engine sinks 187.7 Mbit/s single-path. The true
   C7/C8 binder was the flow-control pool (ADR-0058).
3. **Engine parallelization at ~150 — REFUTED a third time
   (`feat/engine-parallel`).** At the best c7 arm both processes pinned
   to one core EACH sustain full throughput on both seeds; the engine
   receiver task runs 81–87% busy with a near-empty queue (`RWM_RDIAG`
   gauge, built for this). `RWM_ENGINE_PAR` was NOT built — it would
   have measured noise. The profile instead measured the real
   service-time walls (~19.5–20k sym/s sender emission, ~20–22k msgs/s
   receiver engine ≈ 185–200 Mbit/sink, c1-class only) and named the
   actual c7 binder: multipath recovery-plane over-emission — the
   successor that became ADR-0059.

## Consequences

- No threading machinery exists in the transport; the parallelization
  threshold is localized to c1-class cells and documented, not built
  against.
- The refusals were productive: each redirected effort to the real wall
  (pool → ADR-0058; recovery over-emission → ADR-0059; store release →
  ADR-0060), which together took C7 from ~100 to 0.98–1.0×Σ WITHOUT
  threads.
- The method is binding precedent: a parallelization (or any
  "compute-bound") proposal must arrive with a pinning/profile datum, per
  ADR-0052 item 11 pre-registration.

## Evidence

- Ledger: goal-gate.md "Hardware-Honest Re-Baseline + Receiver
  Parallelization (2026-07-14)" (HARDWARE DIVIDE banner, STEP 1 profile);
  "Engine Parallelization (2026-07-19)" (third refutation, RWM_RDIAG,
  service-time walls); "CONSOLIDATED VERDICT" walls #5–#6 rows.
- Paper: §16.19, §16.23.
- Commits: a7ad963 (re-baseline + N× verdict), d27ce30 (RWM_RDIAG probe),
  cee499c (third refutation recorded).

## References

- ADR-0052 (discipline), ADR-0058/0059/0060 (the walls that were actually
  there).
