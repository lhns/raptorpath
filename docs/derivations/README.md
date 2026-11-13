# Derivations

One home for the worked derivations that back a law or a decision (ADR-0076
phase 4). One row per file: its question, its status, and the ADR or status
section it feeds.

| file | question | feeds |
|---|---|---|
| `dup-diag.md` | why c8 duplicates doubled under the 64-gap horizon: packet-threshold fires on gap snapshots applied out of receiver order; proposed snapshot-version law (diagnosis, law not shipped) | status §52, ADR-0059 |

No derivation file is on `main` yet; the branches that add them (`derive-w1`,
`adr68-amend`, `paper-recon`, `perleg-design`, `w1r2`, `otel-design`) each add
a row here when they merge.
