# Coverage of plan items, owner decisions and open items

Frozen 2026-11-13 with nodes.yaml; hand-updated the same day for status §55 Erratum 1, the §3.3 additions and the §51 design.


| item | node(s) |
|---|---|
| W1.1 §48b result and cost attribution | `cc-bbrrs-r1` |
| W1.2 who limits at each δ | `store-w1-who-limits` |
| W1.3 implement the picked form (A on store, D in bbr_rs) | `store-delta-seat`, `cc-bbrrs-r2` |
| W1.4 §48c diagnostic and guard | `store-s48c-session` |
| W1.5 promotion | `cc-user-bbrrs-promotion`, `cc-bbrrs-promote` |
| W1 round 2 (i) c8 CPU cause | `perf-bbrrs-ns-cost`, `perf-bbrrs-ring-fix` |
| W1 round 2 (ii) ProbeRtt cost, δ-free | `cc-bbrrs-r2` |
| W1 round 2 (iii) δ on the store law | `store-delta-seat` |
| W1 round 2 §48d session | `cc-s48d-session` |
| W2.1 amend ADR-0068 | `cc-pacing-audit`, `cc-user-adr68-direction` |
| W2.2 pacing-seam audit | `cc-pacing-audit` |
| W2.3 re-measure Copa-sole and BBR-under on adversarial cells | `meas-adversarial-cells-run` |
| W2.4 hybrid build gate | `cc-hybrid` |
| W2.5 vendored pacing seam | `cc-user-pacing-s3`, `cc-pacing-seam`, `cc-pacing-seam-reading` |
| W3.1 store-law runway | `store-w1-who-limits` |
| W3.2 one per-leg state | `place-per-leg-state`, `place-user-perleg-questions`, `place-per-leg-impl` |
| W3.3 store law engine form | `store-engine-form-impl`, `store-engine-form-guard` |
| W3.4 Copa cwnd consumers, queue_target_mult continuous | `cc-copa-consumers` |
| W3.5 rate law | `rate-user-rate-law`, `rate-law-reconcile` |
| W3.6 recovery timeouts | `rec-recovery-timeouts`, `rec-fix-perleg-clock-n1`, `rec-fix-refresh-clock` |
| W3.6a per-leg findings (units, mode-switch sites, two srtt copies) | `place-per-leg-impl`, `cc-copa-consumers` |
| W3.6b c8 completion tail | `rec-gap-horizon` |
| W3.6c §52 lever B | `rec-gap-horizon`, `rec-leverb-guard` |
| W3.6d tail follow-up (cooldown, retransmits behind source in quinn) | `store-s54-budget`, `rec-s54-session`, `rec-retx-behind-source`, `rec-fix-perleg-clock-n1` |
| W3.6e bbr_rs per-packet cost | `perf-bbrrs-ns-cost`, `perf-bbrrs-ring-fix` |
| W3.6f retransmit leg choice | `place-wdiv`, `rec-w0-leg-question` |
| W3.7 paper reconciliation | `model-paper-recon`, `model-bound-tests` |
| W4.1 pre-flight and §50 | `infra-simplify-batch2` |
| W4.2 archive sibling placement arms | `place-t-derived`, `place-wdiv` |
| W4.3 Group C | `infra-simplify-batch2` |
| W4.4 generation seat | `cod-user-generation-seat` |
| W4.5 Stage 4 module splits, paper prune | `infra-restructure` |
| W5.0 harness consolidation | `meas-harness-consolidation` |
| W5.1 §51 off-Bulk rows | `meas-s51-design`, `meas-s51-harness`, `rate-fix-rate-readout`, `meas-s51-session`, `meas-s51b-rho` |
| W5.2 harness robustness | `meas-harness-robustness` |
| W5.3 adversarial cells | `meas-adversarial-cells-build`, `meas-adversarial-cells-run` |
| W5.4 guard resolution | `meas-guard-resolution` |
| W5.5 test hygiene | `meas-test-hygiene` |
| W5.6 competitor battery | `meas-competitor-battery` |
| W5.7 metrics registry and OTel | `obs-otel-design`, `obs-user-otel-answers`, `obs-impl` |
| W6 housekeeping (scratch worktrees, VM dirs, ledger copies) | dropped: housekeeping, never a node (rule 6) |
| W7 repo restructure | `infra-restructure` |
| W7 wasm NO-MODE-SWITCH audit | `model-wasm-audit`, `model-user-wasm-calls`, `model-wasm-fixes` |
| Plan: tech-tree skill PR | `infra-tech-tree-skill` |
| Plan: law ledger | `model-law-ledger` |
| Decision 1 §48b label / bbr_rs default | `cc-user-continue-bbrrs`, `cc-user-bbrrs-promotion` |
| Decision 2 ADR-0068 direction | `cc-user-adr68-direction` |
| Decision 3 δ seat on the store | `store-user-u1u3` |
| Decision 4 generation seat | `cod-user-generation-seat` |
| Decision 5 remove scratch worktrees | dropped: housekeeping, never a node (rule 6) |
| Decision 6 Stage 4 module splits | `infra-restructure` |
| Decision 7 archive sibling placement arms | `place-t-derived`, `rec-w0-leg-question` |
| Decision 8 pacing seam S3 vs S1 | `cc-user-pacing-s3` |
| Decision 9 per-leg open questions | `place-user-perleg-questions` |
| Decision 10 U1-U3 | `store-user-u1u3` |
| Decision 11 rate law | `rate-user-rate-law` |
| Decision 12 OTel Q1-Q6 | `obs-user-otel-answers` |
| §3.1 item 1 self-heal estimator (fixed, battery not re-run) | `rec-receiver-law` |
| §3.1 item 2 CPU-bound sender (fixed) | `infra-fix-program` |
| §3.1 item 3 ETA route and σ witness (open) | `meas-defect-eta-witness` |
| §3.1 item 4 c8 bimodal collapse (open) | `meas-defect-c8-bimodal` |
| §3.1 items 5-7 (fixed) | `obs-gates-echo`, `obs-readout-macro`, `rate-r-battery` |
| §3.2 generation stack | `cod-generation-coding`, `cod-user-generation-seat` |
| §3.3 NO-MODE-SWITCH debts (incl. the 2026-11-13 additions: symbol size, CLI ρ) | `model-mode-switch-debts`, `model-fix-hint-keyed-paths`, `cc-copa-consumers` |
| §3.4 open-constants register | `model-open-constants`, `model-law-ledger` |
| §3.5 items 1-3 (fixed) | `infra-fix-program`, `rec-wire-v9`, `store-cap-unification` |
| §3.5 item 4 [DIAG] rtp whole ms (open) | `meas-defect-diag-rtp-ms` |
| §3.5 item 5 bind fractions not re-measured (open) | `rec-defect-qclk-clamp` |
| §3.6 fixed by the fix program | `infra-fix-program` |
| §3.7 recorded, not fixed | `infra-defect-recorded-not-fixed` |
| §3.7 moot block rows | `cod-block-pipeline` |
| §3.8 GSO loss truth | `meas-gso-truth` |
| §3.9 lock-out classifier rule | `place-wire-price` |
| §3.10 item 1 | `model-defect-tcut` |
| §3.10 item 2 | `rate-defect-pfec` |
| §3.10 item 3 | `model-defect-dead-machine` |
| §3.10 item 4 | `rec-defect-qclk-clamp` |
| §3.10 item 5 | `rec-defect-hole-law` |
| §3.10 item 6 | `rec-defect-refresh-input` |
| §3.10 item 7 | `meas-defect-rtp-floor`, `meas-guard-v2-impl` |
| §3.10 item 8 | `store-defect-cap-constant`, `store-fix-maxbw-minrtt` |
| §3.10 item 9 | `cc-defect-copa-minrtt` |
| §53 open 1 c8 rtp_floor_p0 rises on lever-B arms | `meas-defect-rtp-floor`, `store-s54-budget` |
| §53 open 2 lever B c1d duplicates +13 % | `rec-leverb-c1d-cost` |
| §53 open 3 main's duplicates unexplained | `rec-defect-base-duplicates` |
| §53 open 4 time channel fires on stale reports | `rec-snapver-law` |
| §53 open 5 CW0 duplicate rise, leg question (W0 with the filter) | `rec-w0-leg-question` |
| §53 open 6 [HOLD] skewed on main | `rec-defect-hole-law` |
| §53 Erratum 1 pool-arm causal test, quinn evictions | `rec-s54-session` |
| §53 Erratum 1 [LAT] over-check of earlier delay readings | dropped: superseded by the in-order delay clause of guard v2 (meas-guard-v2-impl); no reading cited since |
| §53 Erratum 2 pre-flight tests not run | `model-bound-tests-unrun` |
| §53 What follows: filter as law, then guard | `rec-snapver-law`, `rec-s54-session` |

Unmapped: none.

