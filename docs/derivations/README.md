# Derivations

One home for the worked derivations that back a law or a decision (ADR-0076
phase 4). One row per file: its question, its status, and the ADR or status
section it feeds.

| file | question | feeds |
|---|---|---|
| `guard-clause-dry.md` | Do a source-phase rtp floor or a delivered in-order delay behave better than `rtp_floor` as a merge-guard clause, on the existing guard ledgers? (dry run, facts only) | status §3.10 item 7, §53 Erratum 1 |
| [law-ledger.md](law-ledger.md) (+ `law-ledger.yaml`, `law-ledger/render.py`) | is every shipped law derived, measured in its regime, unclamped, fed honest inputs and free of mode switches; what to fix first | AUDIT (no engine change); status §3.10, the roadmap |
| `p0-dwell-diag.md` | why lever B triples p0's quinn send backlog at c8 (diagnosis; a pinned store population rebalanced from holes to queue) | status §53 open item 1, `cand52` merge question |
| `dup-diag.md` | why c8 duplicates doubled under the 64-gap horizon: packet-threshold fires on gap snapshots applied out of receiver order; the snapshot-order law, shipped (paper §7.1) | status §52, §53, ADR-0059 |
| `visualizer-drift-2026-11.md` | is the L0 model (wasm + page) current against the engine and the measured findings; which audit defects still stand; what to update, in order (audit, no model change) | law ledger M01–M04, C13, R03; `wasm-modeswitch-audit` |
| `shared-core-design.md` | which engine decisions can move into an I/O-free, wasm32-clean `raptorpath-core` shared with the L0 model; seams, phased zero-behaviour-change order, CLAUDE.md scope amendment (design, for the owner) | ADR-0076 phase 6, W3.2 |
| `store-law.md` (+ `store-law/`) | what size the sender store should be: a time law (no rate input), its anchor and b₀ sources, the C02 horizon coupling, per-cell predictions and a reported-only diagnostic (derivation, for review) | status §54b, §55 F01/E03/E04/C02 |
| `wasm-modeswitch-audit.md` | which hint-, δ- or ρ-keyed sites in the L0 model are mode switches, and what replaces each (open: owner calls on give-up options (i) vs (ii), gate :181, W22 code-vs-paper divergence and its paper-vs-pin conflict, fixed r) | ADR-0076 phase 6 |

No derivation file is on `main` yet; the branches that add them (`derive-w1`,
`adr68-amend`, `paper-recon`, `perleg-design`, `w1r2`, `otel-design`) each add
a row here when they merge.
