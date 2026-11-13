# Derivations

One home for the worked derivations that back a law or a decision (ADR-0076
phase 4). One row per file: its question, its status, and the ADR or status
section it feeds.

| file | question | feeds |
|---|---|---|
| `guard-clause-dry.md` | Do a source-phase rtp floor or a delivered in-order delay behave better than `rtp_floor` as a merge-guard clause, on the existing guard ledgers? (dry run, facts only) | status §3.10 item 7, §53 Erratum 1 |
| [law-ledger.md](law-ledger.md) (+ `law-ledger.yaml`, `law-ledger/render.py`) | is every shipped law derived, measured in its regime, unclamped, fed honest inputs and free of mode switches; what to fix first | AUDIT (no engine change); status §3.10, the roadmap |
| `p0-dwell-diag.md` | why lever B triples p0's quinn send backlog at c8 (diagnosis; a pinned store population rebalanced from holes to queue) | status §53 open item 1, `cand52` merge question |
| `wire-clock.md` (+ `wire-clock/clocks.py`) | which per-path clock each recovery law needs (time threshold, cooldown, pooled clock, N = 1 gate, receiver refresh, NACK throttle); the formulas, one law across N, and the post-hoc effect at c1s/c1d/c2/c8 (derivation, no engine change) | ledger rows E01, E11, E10, C07, C05, C01, C04, C09; roadmap `rec-fix-*` |

No derivation file is on `main` yet; the branches that add them (`derive-w1`,
`adr68-amend`, `paper-recon`, `perleg-design`, `w1r2`, `otel-design`) each add
a row here when they merge.
