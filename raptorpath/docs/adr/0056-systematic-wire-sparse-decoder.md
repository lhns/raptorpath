# ADR-0056: Systematic-Repair Wire as the Generation Arm + Sparse-Aware Decoding

## Status: Accepted

**Date**: 2026-07-13

## Context

Walls #3 and #4: the generation machine capped the whole gen transport at
~34 Mbit/s regardless of path count, attributed to the receiver's dense
per-generation Gauss–Jordan O(G²·S). Profiling found the attribution half
right — the quadratic is real but lives in the WIRE MODE, not the solver.
The measured generation arm (`--window-generation-coding`) is coded-only
on the wire: every DoF arrives as a dense combination, so ~G dense rows ×
O(G) row-ops at the receiver AND ~G coded emissions × O(G·S) at the
sender are information-structural for that wire. At ε=2.6% only k ≈ ε·G ≈
10 DoF are actually missing. Separately (wall #4) the decoder wasted work
even in systematic mode: known sources materialized as full-width fused
pivot rows, dense repairs reduced against all G rows, full-width
late-source injection.

## Decision

1. **The systematic-repair wire is the generation arm.** Source symbols
   ride raw (unit rows, O(S) delivery); only ~⌈G·r⌉+deficit repair rows
   are dense — the O(k·G·S + k³) machine. The coded-only wire is retained
   as an explicit experiment arm, not the measured default; the harness
   gen-arm recommendation is the systematic wire (§16.18).
2. **Sparse-aware decoding, unconditionally.** GenerationDecoder rewrite:
   per-slot `known` bitmap — known sources never enter the matrix
   (payload-only elimination), k=0 generations skip decode entirely.
   Pure speedup, delivered set byte-identical, differential-tested
   against the pre-rewrite decoder kept as a reference oracle.
3. This cost model becomes the foundation of the unified decoder
   (ADR-0064): the full global closure WITH the sparse-aware cost,
   block-diagonalizing to this bound on aligned wires.

## Consequences

- Gen single-c2 33.9 → 70.9 Mbit/s (×2.1, = 0.92× plain+BBR); c3 13.0 →
  15.0 (0.95× the recovery ceiling); C8 het 30.0 → 69.8, beating
  plain+BBR's own C8 ×1.25–1.5 with σ halved (pre-divide numbers).
- With wall #5 (crypto) also refuted on real silicon, "coding is ~free":
  gen-sys single = 0.97–1.0× plain+BBR at ~0.37 s recv CPU per 25 MB.
  The FEC throughput story becomes the presence⊥throughput identity —
  parity on a saturated single path, stabilization + tails where FEC
  wins — rather than a CPU tax.
- Generation coding is the measured STABILIZER: gen-sys C8 σ halved vs
  plain+BBR with the bimodality gone.

## Evidence

- Ledger: goal-gate.md "Decode-CPU Ceiling (2026-07-13)" (profile,
  rewrite, L1 re-measure); "CONSOLIDATED VERDICT" walls #3–#4 rows.
- Paper: §16.18.
- Commits: da926a5 (sparse-aware decoder + differential oracle), 2122481
  (wire-mode verdict + L1 numbers).

## References

- ADR-0041 (SIMD GF(256) — why SIMD could not fix an asymptotic),
  ADR-0064 (the unified decoder that inherits the cost model).
