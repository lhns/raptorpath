## 37. Model review — joint fixed point, internal audit, literature — no VM, record

The user asked for the whole model and pipeline to be rated, with the
coupled laws considered together and against newer research. Three
read-only reports (a joint fixed-point derivation, an internal audit and a
2021–2026 literature scan) were reviewed with the advisor. This section
records what was checked here; it changes no code and runs nothing.

**Rating.** The architecture holds, but the paper as written does not.

| part | rating |
|---|---|
| one pipeline, one decoder, SACK release | holds (differential tests; the same family as Tetrys RFC 9407 and FlEC) |
| rate law r* | the maths holds; inert at Bulk (r* = 0, coded share ≤ 0.9 %); open-loop where the field closes on feedback |
| continuous in δ, no mode bit | structural, unevaluated on the wire: δ never run off a preset, δ does not reach the CC, hint sites remain (paper q13 under-lists them), ρ < 1 ungated |
| store law | fails as a derivation (circular, §23–§34); repair in progress (§36) |
| placement | weak, with one verified defect (below) |
| recovery | weak: what ships is RFC thresholds and a binding clamp, validated only at today's dwell-inflated H |
| evaluation | unconfirmed on the current binary: July competitor table, no same-session baseline, sc2 "undiagnosed" though fixed in `1b890e0`, c1 numbers stale (paper 229 / 446–505, shipped 826 / 751) |

**Verified here (file evidence).**
- *Idle legs are a stale-price lock-out.* In c1d DEF1024 s42 r2 (p34), leg 1
  carries `infl=545` with `rtt=18 ms` at t = 0.3 s. By 0.5 s it reads
  `infl=0` with `rtt=156 ms` against quinn's `wrtt=3 ms`. The value then
  stays frozen for the rest of the row, and its `[ETA]` sample count stops
  at 4 590. Placement prices on the dwell-inclusive echo srtt, which freezes
  when the leg carries nothing.
  - Joint report: the same pattern holds at c8 from cap 256 up.
  - Joint report: one-leg c1d rows ran ×1.35 μ_R and about 12 % more
    goodput than two-leg rows, so a fix may cost c1d goodput.
- *The cold price never binds.* `cold_r=0.0000` on all 31 807 `[ETA]` lines
  under `docs/l1-raw`. An unmeasured leg's ε̂ starts at 0, so
  `correction_rate()` is 0, not 10. Paper §5.7/§11.2's "hard exclusion"
  describes an unreachable branch.
- *Quinn's `max_bw` is a ratchet.* quinn-proto 0.11.14 `bw_estimation.rs:65`
  calls `update_max` only when a sample exceeds the current max, and
  `update_max` then refills every slot, so the window expiry never runs.
  K only grows except through min_rtt. Paper §6.1 (b1)'s "windowed max" is
  wrong.
- *FlEC (ToN 2023) and AC-RLNC (IEEE TCOM 2020) are absent from §11.4.* They
  are the nearest prior art: sliding-window RLC in QUIC, feedback DoF
  accounting, and multipath FEC/ARQ allocation.
- *`docs/goal-gate.md` was deleted in `22b56d9`.* The repo CLAUDE.md still
  cites it.

**Corrections to paper §6.1's oracle-L paragraph (owed, not yet edited).**
- c8's 324 comes from surviving rows only; the void rows are the lock-outs.
- c1s's S may contain the law's own dwell (B ≈ 81 ≈ 0.8 ms at 614; at slope
  0.54 each 0.1 ms moves the point down by about 23). §36 D6 decides this.
- c2 is the one robust cell.
- At c8 the fast-leg S is produced by placement, so a placement price that
  reads S needs its own fixed-point check.

**From the joint derivation (candidate, not yet verified here).**
- The candidate law takes min{link Σ, receiver μ_R}, with source share φ
  (repairs are not in the cap but are on the wire), S from dwell-free
  samples, and the hole term.
- Placement prices from RTprop + S, a per-leg state kept when idle; a
  never-measured leg takes the best measured leg's values.
- New H = q·RTprop + (1+q)·S, about ×5 below today's derived H: c1s 3.6 ms
  against 19 ms. The 25 ms refresh floor, the 10 ms NACK cooldown and
  `GAP_ACK_MIN_INTERVAL` were validated only at today's H.
- δ has q-only authority on the store law: at most about 10 % across the
  dial. The power argmax moves ×2 between α = 1 and 2.

**Before any build of a dispersion law.** netem is dispersion's best case.
Aggregation (Wi-Fi/LTE), interrupt coalescing and policers inflate short
trains (Dovrolis ToN 2004; Flach SIGCOMM 2016). So the capture is re-run on
an aggregation cell (netem `slot`) and a policer cell (`tbf`). The
receiver → sender field should follow draft-ietf-quic-receive-ts
(`PATH_ACK_RECEIVE_TIMESTAMPS`) rather than a bespoke frame.

**The user's decisions (open).**
- A same-session competitor battery, to re-establish §9.
- Accepting q-only δ authority on the store law.
- Importing FlEC-style feedback DoF accounting for the inert FEC half.
- A paper refresh: related work; stale c1, sc2 and ADR-0072 text; the
  constants missing from §11.2.
- The CLAUDE.md reference to the deleted `goal-gate.md`.
- Fixing the placement lock-out (B2).

### 37. Addendum — what the design passes changed (no VM)

- *The store law's time term has no source either.* §38 found no dwell-free
  per-leg S on main: every ledger's S comes from app-echo samples that
  include the sender's dwell. "L is the missing input" was too narrow: the
  law lacks both L and a dwell-free S. §36 D6 (`rtt` − `wrtt`, whole ms)
  cannot settle the ≈ 0.8 ms question at c1s. The µs fields of the `p<i>:`
  block (`qsp_us`, `msd_us`, `tlag_us`) are to be read offline first.
- *Placement.*
  - Keeping RTprop + S when idle does not cure the lock-out. An idle leg's
    last S is the dwell spike that idled it (140.6 ms at c1d DEF1024 s42
    r2).
  - What prices an unmeasured leg is the 50 ms srtt seed, not r = 10.
  - The lock-out is rare at main's default store law (1 in 149 two-leg
    default rows) and common at static caps.
  - Whether it is a start-up artefact of the static arm (the traced row
    locked by 0.5 s) is open, and it bears on every static grid, §36's c1d
    and c8 rows included.
- *User decisions 1 and 2 (a same-session competitor battery; q-only δ
  authority on the store law) are still open.*
