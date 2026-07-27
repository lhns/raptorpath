# ADR-0058: Path-Scaled Outstanding Pool; the Per-Path Account Family Refuted for Asymmetric Cells

## Status: Accepted (`RWM_STORE_PATHS` default ON since 2026-07-21; percap/borrowing family Refuted for c8, retained gated for symmetric cells; c8 pool law re-opened — the "c8 WATCH")

**Date**: 2026-07-14 … 2026-07-21

## Context

Wall #7, the actual multipath binder: the outstanding pool
(`RELIABLE_STORE_MAX` = 1024) is a per-TRANSFER constant — a Little's-law
~100–128 Mbit wall, CPU-invariant. C7 sat "pinned at the receiver wall"
for a month; the wall was an unscaled constant (`win=1024/1024` pegged in
every dual-path run). The vision question underneath: should flow-control
capacity scale as one pooled law or as per-path accounts?

## Decision

**The pooled path-scaled law ships**: `RWM_STORE_PATHS` — pool size
scales with active path count (knee ≈ 2048/path), one shared pool.
Default ON since the consolidation LOO battery (ADR-0067).

**The per-path account family was built, chased through three derived
refinements, and refuted for the asymmetric cell** — one chain, three
sub-experiments, each pre-registered:

1. `RWM_STORE_PERCAP` (#86): per-path accounts (cap_i from
   gain·BtlBw_i·echoRTT_i), admission = any account has headroom. c7
   symmetric: parity-or-better with pooled (0.87/0.97×Σ, pooled collapse
   mode absent). c8 — the cell it was BUILT for — REGRESSED to
   0.38–0.43×Σ under both CC families (cap-full placement redirect
   over-commits the slow account).
2. Redirect guard (`RWM_PERCAP_GUARD`, 689b9f1) + honest caps
   (`RWM_HONEST_CAP`, efc8f75/5d30c02: residence K·RTprop +
   recovery-clock runway): recovered half the regression, resolved the
   sc2 −20% exactly, c7 percap ≥ pooled — but c8 still < pooled. With
   caps honest by construction, the residual is the account structure
   itself: the NO-BORROWING TAX is the confirmed c8 binder.
3. Bounded borrowing (`RWM_STORE_BORROW`, §16.22): the lender-solvent
   loan law behaved exactly as derived (c7 loans ≡ 0 by theorem AND by
   gauge; c8 loans one-directional, bounded, repaid) and STILL could not
   repay the tax — the neutrality result: lender-solvent slack cannot
   match pooled depth. Flip NO; the pooled design vindicated as the c8
   answer at that date.

**The honest coda (the c8 WATCH, 2026-07-21):** under SACK-clocked
release (ADR-0060) the LEGACY 1024 pool reads better at c8 than the
path-scaled pool (0.85–0.87×Σ vs 0.72–0.76) — the §16.22 pooled-c8
verdict was pre-SR and has MOVED. A c8-aware pool law (asymmetric scaling
or per-topology gating) is the named, pre-registerable follow-up worth a
measured +11–13 Mbit.

**Follow-up EXECUTED and REFUTED (2026-07-27, `feat/c8-pool-law`,
`RWM_STORE_CAPW` default OFF; goal-gate "C8-Aware Pool Law" + paper
§16.29):** the capacity-weighted shared pool (Σ honest per-path caps) was
derived, pre-registered, built and measured — it engaged exactly as
derived (pool 1.3–2.5k on honest anchors, between the incumbents) and
LOST to legacy-1024 at c8 on both seeds. The per-path gauge shows the
binder is not pool sizing: the FAST path parks the un-SACKed frontier
span in every deep-pool arm and the SLOW path converts ~nothing
(legacy c8 = fast single + 2.7 Mbit). The c8 residual's true name is
slow-path CONVERSION (placement/recovery at the asymmetric cell);
the mechanical alternative worth the measured +11–14 is a per-topology
gate to the legacy span law. Same session, the RS witness cost was
priced at −22…−27 ≫σ for the symmetric dual (`RWM_PLAIN_RS` full-stack
candidacy refuted at c7).

## Consequences

- C7 plain+BBR 100 → 136/142 at introduction; in the composed stack the
  LOO row shows removal re-opens a c7 collapse class on both seeds (3/8
  runs at 86–97 Mbit) — the member's default is LOO-defended.
- `RWM_STORE_PERCAP`/`_GUARD`/`_HONEST_CAP`/`_BORROW` stay default OFF:
  percap(+honest caps) is the symmetric-cell tool; borrowing is a
  measured, law-perfect negative. Retention pending the c8-aware pool
  follow-up (which also carries `RWM_PLAIN_RS`, ADR-0061).
- Flow control is now a derived, path-aware law rather than a constant —
  but the pool-vs-account question is settled only per-topology; the c8
  asymmetric cell keeps the register's follow-up clause.

## Evidence

- Ledger: goal-gate.md "Hardware-Honest Re-Baseline" (wall named,
  RWM_STORE_PATHS built), "Per-Path Outstanding Accounting (2026-07-18)"
  (the full percap→guard→honest-cap→borrowing chain with GUARD/
  HONEST-CAP/BORROWING RESULTS), "Consolidation (2026-07-21)" (LOO flip +
  c8 WATCH), "CONSOLIDATED VERDICT" wall #7 row.
- Paper: §16.19, §16.21 (addenda), §16.22.
- Commits: 5cace52 (path-scaled pool), d190b29 (percap), 689b9f1 (guard),
  efc8f75 + 5d30c02 (honest caps), 477ab32 + 7c3343f (borrowing +
  verdict), 5ebbcda (default flip).

## References

- ADR-0057 (the refuted receiver-wall attribution this replaced),
  ADR-0060 (the release law that moved the c8 story), ADR-0067 (LOO).
