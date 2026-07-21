# ADR-0060: SACK-Clocked Store Release (supersedes the SACK_PRUNE experiment)

## Status: Accepted (`RWM_STORE_SACK_RELEASE` default ON, 2026-07-21). Supersedes `RWM_SACK_PRUNE` (refuted 2026-07-07; ledger-only experiment, no prior ADR — marked deprecate-HARD in the register)

**Date**: 2026-07-21

## Context

Wall #9, the §16.24 residual: the retention store freed slots only on the
CUMULATIVE frontier (`sent_store.split_off(&(ack+1))`), so
SACKed-but-not-cumulative symbols held flow-control slots a full frontier
round — at c7 the store recycled at frontier latency, not path rate
(wire un-full, waste suppressed, goodput stopped). The 2026-07-07
`RWM_SACK_PRUNE` experiment had attacked the same slot pressure by
REMOVING SACKed symbols from `sent_store`/`retransmit_buffer` — refuted
UNSAFE: it destroyed the only retransmittable copy, so a
received-then-evicted symbol at the receiver's bounded reassembly window
was unrecoverable → C7/C8 in-order DNF wedge. The structural lesson: slot
release ≠ recoverability.

## Decision

On a SACK range, release the STORE SLOT — uncount the seq from the
outstanding/flow-control gate (`store_len = sent_store.len() −
released.len()` at the single site every gate reads) — while RETAINING
the payload and every recovery structure (`sent_store`,
`retransmit_buffer`, `nack_retx_at` + per-flight clocks,
`source_path_map`) until the cumulative frontier passes. The released set
prunes on the same cumulative `split_off` twin (subset invariant);
idempotent under re-advertised snapshots; composes with the path-scaled
pool and percap accounts with no extra code. Worst case under receiver
eviction is one wasted retransmit, never a wedge. First mechanism shipped
under full ADR-0052 item-11 pre-registration (prediction, falsification
clause, and the derivation re-read written before the build).

`RWM_SACK_PRUNE` survives one pass only as the precedence-warned control
arm (explicitly set, it wins over the release law with a warning);
removal is scheduled (ADR-0066 register: no re-test owed — the unsafety
is structural, no wall excuses destroying recoverability).

## Consequences

- The pre-registered prediction held, exceeded: c7 0.959/0.934×Σ SR-only,
  **1.018–1.045×Σ composed with `RWM_RECOV_MP`** (both seeds); sc2
  +4.3/+2.9 ≫σ (single-path SACKs above a hole also hold slots — the N=1
  term is real); dual-c1 composed +20–22 above single; c8 unregressed.
  Occupancy 3,157→1,460 at ~167k slots released/200 MB with retx FALLING.
- The 2026-07-07 "sender was never the bottleneck" null is era-resolved:
  on the post-wall substrate the sender store IS the binder, and
  releasing it converts ~1:1 at the symmetric dual cell.
- Side effect on ADR-0058: under SR the legacy 1024 pool reads better at
  c8 than the path-scaled pool — the c8 WATCH follow-up.

## Evidence

- Ledger: goal-gate.md "SACK-Clocked Store Release (2026-07-21)"
  (pre-registration, law as built, 15-arm battery, dwell gauges, flip
  verdict); "SACK Flow Control (2026-07-07)" and "SACK+BDP Reassembly
  (2026-07-08)" (the refuted precursor era); "CONSOLIDATED VERDICT" wall
  #9 row; DEPRECATION REGISTER row `RWM_SACK_PRUNE`.
- Paper: §16.25.
- Commits: 7145fcc (pre-registration), ff7acb4 (build), a52105d (flip +
  battery).

## References

- ADR-0052 (item 11 — this is its first exercise), ADR-0059 (the
  composing partner), ADR-0058 (c8 WATCH), ADR-0066 (SACK_PRUNE
  disposition).
