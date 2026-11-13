# Derivations

One home for the worked derivations that back a law or a decision (ADR-0076
phase 4). One row per file: its question, its status, and the ADR or status
section it feeds.

| file | question | feeds |
|---|---|---|
| `guard-clause-dry.md` | Do a source-phase rtp floor or a delivered in-order delay behave better than `rtp_floor` as a merge-guard clause, on the existing guard ledgers? (dry run, facts only) | status §3.10 item 7, §53 Erratum 1 |

No derivation file is on `main` yet; the branches that add them (`derive-w1`,
`adr68-amend`, `paper-recon`, `perleg-design`, `w1r2`, `otel-design`) each add
a row here when they merge.
