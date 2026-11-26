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
| `pfec-continuous.md` (+ `pfec-continuous/`) | what P_fec must mean (per lost symbol, not per window), why the shipped cut and the paper's Φ both fail, the exact block law C6, and the one-sided bracket against a sliding decode (open: the sliding per-loss law, owner path (b); next, a test-only executed oracle) | ledger R03/R04/S04, audit W22, status "Owner decisions (2026-11-14)" item 2 |
| `pfec-oracle-spec.md` | the build spec for the executed P_fec oracle: called-vs-copied pieces, the span-seat extraction (D12), the deadline per caller, known-answer tests, and the registration draft; plus emission readings E1 (taper shape inert as wired) and E2 (acks reduce the repair debt; cadence-dependent, to be witnessed) | `pfec-continuous.md` §8, ledger R03/S04 |
| `giveup-law-model.md` | the model's give-up law at ρ < 1: budget = min(1 − ρ, R) after the deadline D(δ), replacing V5/V6 and W14 (owner decision 2026-11-14 item 3) | derivation, Fable round 1; implementation next |
| `store-law.md` (+ `store-law/`) | what size the sender store should be: a time law (no rate input), its anchor and b₀ sources, the C02 horizon coupling, per-cell predictions and a reported-only diagnostic (derivation, for review) | status §54b, §55 F01/E03/E04/C02 |
| `wasm-modeswitch-audit.md` | which hint-, δ- or ρ-keyed sites in the L0 model are mode switches, and what replaces each (open: owner calls on give-up options (i) vs (ii), gate :181, W22 code-vs-paper divergence and its paper-vs-pin conflict, fixed r) | ADR-0076 phase 6 |
| `f2-guard-registration.md` | the §41c merge guard for engine defect F2's fix: F2 alone, guard v2 on the Bulk cells, a Realtime stage scored on the absolute F2 reading, the `framing_ns` bound, the harness brief (registration draft) | status "Engine defect F2", §63, ADR-0078 |
| `three-knobs.md` | replace the declared (δ, ρ) dials with three application knobs, lifetime T, delay price δ and order hold H: the laws, the mapping of every retired concept, named points, owner questions and the toy stage 3 registration (derivation, for the owner) | ADR-0064, ADR-0084 and its amendments (would supersede in part); roadmap node owed |
| `visualizer-three-knobs-plan.md` | how the L0 model and the page change for the three-knob proposal (T, δ, H; ρ reported): site inventory, adjust-not-rewrite verdict, two pages from one wasm, the law module and its toy vectors, gates, phased brief (plan; implemented on branch `viz-three-knobs`: `raptorpath-wasm/src/knobs.rs`, `consumer.rs`, `raptorpath-visualizer/knobs-visualizer.html`). Recorded divergence: the model's ack timeout R reads the fixed rttvar rtt/8, the offline simulation's (stage 3d, `e17b4c64`) its measured RFC 6298 rttvar (addendum 1 A3 held; the measured one breaks the T-continuity gate) | `three-knobs.md` (branch `three-knobs`), ADR-0077, CLAUDE.md NO-MODE-SWITCH; roadmap node owed |
| `sender-queue-law.md` (+ `docs/l1-raw/bbr48b/handoff/`) | one law for what the sender hands quinn: a δ-free hand-off reserve β = 1.25·ν̂·(g + Δ_v) on quinn's per-path queue, the repair clamp on the same quantity, the store as a resource bound, and δ's one seat on the three-knob intake; the four §48b checks behind it and the §48d predictions (derivation, for the owner) | W1.2 (withdraws its store seat), `three-knobs-revision4.md` §3.3/§11.4 (Q31), §48d draft; roadmap node owed |

No derivation file is on `main` yet; the branches that add them (`derive-w1`,
`adr68-amend`, `paper-recon`, `perleg-design`, `w1r2`, `otel-design`) each add
a row here when they merge.
