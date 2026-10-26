# ADR-0072: One store-cap law at every path count — the `n_live < 2` guard is removed

## Status: Proposed (law, paper and engine written; `RWM_STORE_PATHS=0` is the control; awaits the battery under Validation, status §18)

Supersedes the N = 1 clauses of ADR-0058 ("N = 1 keeps the single-path law
bit-exactly") and ADR-0070 (the guard and `RELIABLE_STORE_MAX` as the
single-path ceiling). Both were deleted in `94bf58d`; read them at `94bf58d^`.

## Context

`pooled_store_cap` returns `None` below two live paths, so one path runs a
different law, `clamp(2.0·BtlBw·RTprop, 10, 1024)` (paper §6.2). Nothing in
the pooled law needs two paths: its Σ has one term at N = 1 and its ceiling is
1·knee. The guard was A/B hygiene (the path-scaled pool, then the ×N and
δ-cap arms, "bit-identical at N = 1") that outlived its A/Bs. Its effects:

- **A constant over the N = 1 range (rule 18).** 1024 binds in every
  post-warm-up c1s window (status §16, finding 3) and 17/18 sc2 reps. 1024
  has no provenance; `RWM_STORE_GAIN = 2.0` has none in this seat either.
- **A path-count step (the CLAUDE.md pattern).** At a symmetric per-path
  anchor a, per-path cap goes `min(2a, 1024)` → `min(1.1a, 2048)` (Bulk)
  across N = 1 → 2: ×1.80 at c1, ×0.55 at sc2.
- **δ never reaches one path.** The single-path cap is the same at every
  hint.

## Decision

One law at every N ≥ 1 (paper §6.1):

```text
   cap  =  clamp( (1 + q(δ)) · Σᵢ∈live bwᵢ·RTpropᵢ ,  floor ,  N·knee )
```

- Delete the `n_live < 2` clause from `pooled_store_cap`, at both seats that
  call it (plain Σ-anchor and Copa-sole Σ cwnd). The legacy `gain·Σ` branch
  then runs only under `RWM_STORE_PATHS = 0`, the predecessor arm.
- floor and N·knee are resource bounds outside the law, each with its
  `[SUMCAP]` bind fraction (`floor=`, `pin=`), now reported at N = 1 too.
- `RELIABLE_STORE_MAX` leaves the shipped seat; it stays the
  `RWM_STORE_PATHS = 0` ceiling and the `min` under the boot cap.
- The default-off arms with the same step (`capw_store_cap`'s
  `terms.len() < 2`, the pool-anchor `n_live >= 2`, the honest arm's
  `N·knee if N ≥ 2 else store_max` ceiling) take the same change, so no arm
  keeps it.
- The flip is gated for one battery (`=0` reproduces main bit-exactly) and
  flips in its own commit, per the pre-registration.

## Consequences

- c1s: the law reads the c1 anchor, which over-reads ×7–9, so it asks
  1.37–2.34 k and the knee binds in about half the windows (7/15 at the r1
  trajectory). Expected: the store, and so the RTT, grows at roughly the same
  goodput. That is the anchor defect showing through, not a reason to keep the
  constant; it is recorded in the register (paper §11.2) and the battery
  reports it.
- c2/sc2: the cap drops from ≈ 989–1024 to ≈ 544–850, the δ-priced setpoint
  §6.2 already named as the successor.
- Single-path recovery headroom H falls from one RTprop to q(δ)·RTprop
  (§7.3) at an honest anchor; at c1's over-read the realized H stays
  ≈ 13–19 ms.
- Tests that pin the guard or 1024 at N = 1 are rewritten to the law's
  absolute values at N = 1 and to linearity from N = 1.

## Validation

One binary. At N = 1, `RWM_STORE_PATHS=0` is the control: it runs exactly
main's single-path chain (pinned by a test), so no new gate is added. At
N ≥ 2 the change is a no-op by construction (pinned by a test), so c1d and c8
run the law arm only, as a control-moved check. Scored at c1s and c2 (Bulk,
n = 6 per arm, two seeds), with a δ leg (Auto, Realtime) proving δ reaches
one path. Outcomes, thresholds and the flip rule go in their own
pre-registration commit before any VM contact.
