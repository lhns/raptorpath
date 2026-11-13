## 43. The oracle store law in closed loop: one operating point or two? — diagnostic arm `RWM_STORE_LAW=oracle` at c2 and c1s — pre-registration

This is §42's step 2; the user approved the design. Paper §6.1 ("The
closed-loop oracle arm") fixes the law's free choices first:

```text
   n(t) = clamp( (1 + q(δ)) · Σᵢ Lᵢ · (RTpropᵢ(t) + Sᵢ(t)) ,  max(floor, h(t)) ,  N·knee )
```

- L comes from the environment.
- RTprop is Copa's shipped 10 s windowed min.
- S = Tp50 − Tmin of the last closed 250 ms window, from an uncapped
  app-echo sampler.
- n is recomputed at the shipped ≤ 5 ms refresh, with no step limit.
- Boot is a zero initial state.
- h(t) is the start-high hold.

The question is whether this loop, run for several filter lengths, has one
operating point from below and from above. §42's expired-anchor map says c2
may have two. Diagnostic: the arm is default off and never merged, nothing
ships, and no law is built from it.

**Premises checked (no VM).**
- *The filter is 10 s.* `CopaState::window_duration` is 10 s at every gate,
  and `expire_old_samples` applies it to the RTT samples. The ≈ 10·RTprop
  [1 s, 10 s] cutoff (`rs_on_delivered`) evicts bandwidth samples only.
- *The truncation being replaced.* `[LOOPDIAG]` (`archive/store-power`,
  `net/loopdiag.rs`) buffers the first `RTT_CAP` = 8 192 samples per path
  per window and counts the rest in `ov=`. At c1, windows hold 8–15 k
  samples (§42 addendum).
- *The payload at 5 GB.* The perf client streams the object from a
  one-chunk buffer (`perf.rs` `run_object`), so its memory does not grow
  with the size. The chunk count, 5·10⁹ ÷ payload ≈ 4.2 M, fits its `u32`.
  The receiver's memory at 5 GB is not checked from source, so the smoke
  rows run at full size.
- *Row lengths.* Three filter lengths are counted from the release, so the
  brief's sizes were raised:
  - At 400 MB c2 runs ≈ 36–40 s and leaves ≈ 21–25 s after a 15 s hold.
    At 4 GB c1s runs ≈ 39–43 s and leaves ≈ 24–28 s.
  - **c2-100 runs 600 MB** (≈ 53 s at MAIN's ≈ 90 Mbit/s, ≈ 60 s at the
    law's ≈ 80; ≥ 38 s after the hold).
  - **c1s-400 runs 5 GB** (≈ 48 s at ≈ 826 Mbit/s, ≈ 54 s at ≈ 740;
    ≥ 37 s after the hold).
  - One size per cell for every arm. The cell names keep their scenario
    (c2 100 Mbit/s, c1 1 Gbit/s); the size is this section's.

**Tree and build.**
- Branch `oracle43`: `ce389a8` (github/diag36), then the paper `9a42b2a`,
  this section, the arm commit (engine code, default off, not for merge, as
  `run34`), and harness plan 43 (`tools/l1/loopdiag_*`).
- One binary, built fresh on disk with a fresh target dir, and measured from
  root-owned `/dev/shm`.
- *Host build gate, red first, before VM contact.*
  - (a) Default off: with `RWM_STORE_LAW` unset, the shipped cap sequence is
    bit-identical.
  - (b) Absolute values. Bulk, L 9 773, RTprop 7.0 ms, S 5.4 ms gives ask
    133.3 and n = 134. L 97 733, RTprop 2.60 ms, S 2.60 ms gives 559.0 and
    n = 560. The clamp gives 10 and 2 048.
  - (c) A window of 30 000 samples is kept whole, with the exact
    nearest-rank median, and Σ n_k equals the offered count.
  - (d) The hold: n = max(ask, H) while t < t_h and ask after. ask is
    logged in both.
  - (e) An L list whose length differs from the configured path count
    refuses to start.
  - (f) n is continuous and monotone in b through every named point (±2 %
    nudges), in the form of the no-mode-switch gates.
  - (g) Boot: n = floor before any sample; from the first sample to the
    first close, n = clamp((1 + q)·L·RTprop).
- *VM suites,* as §34 amendment 1: release, debug, doc, wasm and parsers on
  the arm tree in a fresh target. A failure that passes its immediate solo
  re-run is `FLAKE`; any other is `REAL` and fires `ABORT-TESTS`.

**Arms.** All arms run Bulk, `--window-reliable`, `RWM_GEN=0 RWM_DIAG=1
RWM_ORACLE_DIAG=1 RWM_PERF_TIMEOUT_S=200`. The environment withholds §27's
list plus `RWM_STORE_LAW` and `RWM_ORACLE_*`. `RWM_ORACLE_L_DGPS` is 9 773
(c2) or 97 733 (c1), the netem rate ÷ (8·1 279 B).
- `SL`, ORACLE start-low: `RWM_STORE_LAW=oracle`, from boot.
- `SH`, ORACLE start-high: `SL` plus `RWM_ORACLE_HOLD=<H>:15`, with H = 1 536
  at c2 and 2 048 at c1s. The hold raises the clamp's lower bound to H for
  15 s, then drops it (paper §6.1: an exogenous initial condition, not a law
  mode).
  - c2's H is ln(1 536/927) = 0.50 above §42's threshold, about 6 sd. At
    1 024 the margin would be ≈ 1.2 sd, and noise alone could split the
    rows.
  - c1s's H is N·knee, the highest cap available. c1s has no threshold.
- `MAIN`: the shipped law. The arm's sampler and the counterfactual ask run
  as observation only (`RWM_ORACLE_DIAG=1` without `RWM_STORE_LAW`).
- `RWM_ORACLE_DIAG=1` prints one `[OCAP]` line per path per window close: t,
  n, ask, h, rtprop_us, tmin_us, tp50_us, S, brake_us = tmin − rtprop, n_k,
  offered, the loop count r_k = T_w/Tp50, the binds `floor` / `pin` /
  `hold` / `wbind`, and n's min and max over the window. Between closes only
  RTprop moves n, so the min/max shows how far the per-window model is
  from literal.

**Rows.** n = 3 per (cell, arm). Three blocks, (42, 1), (7, 1), (42, 2).
Cells alternate (c2 first on even blocks), and the arm order rotates per
block: MAIN SL SH / SL SH MAIN / SH MAIN SL. That makes 18 rows, plus a
2-row smoke at full size (c2 SH, c1s SL), 20 invocations in all.

*Rules (shared file).*
- Launch precondition: both locks free (`test -e` only), no cargo/rustc
  (`pgrep -x`), and `procs_running` ≤ 2 in 5 samples 2 s apart. The locks
  are taken via `lib_battery.sh`.
- The run tree and binary are root-owned on `/dev/shm`, with an `ABORT-BIN`
  sha check before every row.
- io `full avg10` is read before and after every row; ≥ 20 is flagged,
  never voided. Off tmpfs, a row is `VOID-ONDISK`.
- `LD_HARD` = launch + 4 h. The battery does not start with less than
  45 min left (`ABORT-BUDGET`).

*Witnesses (the row is void otherwise).*
- `[GATES]` carries the arm's environment. ORACLE rows carry the echo
  "oracle store law ACTIVE" with the L list and the hold. MAIN rows carry no
  oracle law echo and `[SUMCAP] eng>0`.
- Every path has an `[OCAP]` line for every window with t ≥ 1 s.
- Σ n_k equals the sampler's offered count exactly; otherwise the row is
  `VOID-SAMPLER`.
- *Routing* (`goal-gate.md` rule 1). In ORACLE rows, the `[DIAG]` `win=`
  denominator equals the same tick's `[OCAP]` n on ≥ 99 % of ticks with
  t ≥ 1 s, so the gate runs on the law's n.
- A `dgq` field appears on every `[DIAG]` line with t ≥ 1 s.

*Labels (the row stays live).*
- `SHORT`: an SL or MAIN row under 30 s, or an SH row under t_h + 30 s =
  45 s. Such rows are reported and left out of the outcome.
- `DNF`: the perf timeout. It is not `ABORT-RC`. The row is scored from its
  log, because a collapsed ORACLE row is data. Any other rc ≠ 0 is
  `ABORT-RC`.
- SH rows only, `HOLD-EXPIRED` or `HOLD-INERT`. `HOLD-EXPIRED` iff
  RTprop(t_h) ≥ RTprop(1 s) + ½·(min Tmin_k over the hold's last 2 s −
  RTprop(1 s)), i.e. the anchor rose at least halfway to the queue's lower
  envelope. A windowed min can reach only that envelope, never the median.
  On the ledger rows the threshold reads 50.2 ms at c2 1 024 (expired ≈ 89)
  and 3.49 ms at c1s 2 048 (expired ≈ 4.31), and both fresh anchors fail
  it. A `HOLD-INERT` row would "return" trivially, so it is reported and
  left out of the outcome.
- SH rows: H ≤ N·knee is a startup configuration check, as the L list's
  length is, so the clamp's bounds never invert.

*Abort table:* `ABORT-LOCK`, `-CRLF`, `-SHA`, `-BIN`,
`-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUILD`, `-TESTS`, `-RC`, `-BRINGUP`,
`-BUDGET`, `VOID-COTENANT`, `VOID-ONDISK`, `VOID-SAMPLER`, filled in the
result. If the 5 GB smoke fails on memory, the size falls back, by an
amendment before any scored row.

**Readouts (per row; windows with t ≥ 1 s; the last third is the windows
with t ≥ ⅔ of the row's last window time).**
- **R1, the operating point.**
  - n_end = the median n over the last third; s_end = sd(ln n) over the
    same windows.
  - ñ_SL = the median of n_end over the cell's live SL rows.
- **R2, goodput and delay.** Goodput in Mbit/s from perf. A_x p50 over the
  whole transfer, as §29. The sampler's Tp50, median over the last third.
  SL is scored against MAIN; SH is reported on its post-release windows.
- **R3, return (SH).**
  - t_ret = the first window after t_h that opens 4 consecutive windows
    with |ln(n/ñ_SL)| ≤ 2·sd_pred; otherwise "none".
  - Beside it, the median ask/H over the hold's last 2 s: the open map at H
    with the anchor expired.
- **R4, the brake.** Tmin_k − RTprop per window: medians per third and at
  the release, plus RTprop at 1 s, 10 s, t_h and the end.
- **R5, the binds.** The `floor`, `pin`, `hold` and `wbind` fractions, over
  the whole row and over the last third.
- **R6, the sampler.** n_k min, median and max per row and in the last
  third, and the share of windows with n_k > 8 192 (what the `[LOOPDIAG]`
  cap would have cut).
- **R7, authority.** B = Σ `dgq[hand − tx]`, median per third. Reported
  against 1 ms of L: 98 datagrams at c1, 10 at c2.
- **R8, MAIN on the oracle map.** MAIN's counterfactual ask/n, median per
  third. Reported.

**Outcomes per cell (fixed now).** The constants:
- predicted bands: c2 [157, 164], c1s [614, 699];
- sd_pred: 0.08 at c2, 0.11 at c1s (§42);
- d(x) = 0 inside the band, else |ln| to the nearer edge;
- k = 2.

Each scored SH row (`HOLD-EXPIRED`, not `SHORT`) is `RETURNED` iff
|ln(n_end/ñ_SL)| ≤ 2·sd_pred, else `NOT-RETURNED`, with its direction
(HIGH or LOW) reported. The cell takes the first label that matches:

1. **`UNSCOREABLE`**: fewer than 2 scored SL rows, or fewer than 2 scored
   SH rows.
2. **`UNSTABLE`**: any live ORACLE row shows one of the following (the label
   names the rows and the modes):
   - `COLLAPSE`: `floor` ≥ 0.25 over the last third, or a `DNF` row with
     `floor` ≥ 0.25 over its windows after 5 s;
   - `OSCILLATION`: s_end > 3·sd_pred (0.24 at c2, 0.33 at c1s);
   - `RUNAWAY`, SL rows only: `pin` ≥ 0.25 over the last third. A pinned SH
     row is evidence for `BISTABLE`, not `RUNAWAY`.
3. **`BISTABLE`**: ≥ 2 SH rows `NOT-RETURNED`.
4. **`MIXED`**: exactly 1 SH row `NOT-RETURNED`; the threshold lies within
   noise of the start.
5. **`CONVERGES-ONE-POINT`**: no SH row `NOT-RETURNED`, and
   d(ñ_SL) ≤ 2·sd_pred.
6. **`ONE-POINT-OFF-PREDICTION`**: no SH row `NOT-RETURNED`, and
   d(ñ_SL) > 2·sd_pred. There is one point, but not where §42 put it; the c1
   truncation caveat is the first suspect.

Reported beside the label:
- `SPREAD`: the median of s_end over SL rows ÷ sd_pred, `AS-PREDICTED` iff
  in [0.5, 2].
- `GOODPUT`: SL − MAIN against ±2σ_pooled (the two arms' sample sds),
  `BETTER`, `WITHIN` or `WORSE`.
- `DELAY`: A_x p50 and the last-third Tp50, each `BETTER`, `WITHIN` or
  `WORSE` by 2σ_pooled.
- Fewer than 2 live rows in an arm makes that comparison `UNSCOREABLE`.

**Decision rule (diagnostic; nothing ships; the arm is never merged).**
- *`CONVERGES-ONE-POINT` at both cells:* the shipped anchor gives one
  operating point in practice. The next steps go to the user as a proposal;
  none starts from this section:
  - the engine L̂ (§42 (a): socket timestamps, wire v10, an ADR);
  - c8's idle-leg rule;
  - the goodput the power point gives up (the §40 standing decision on δ's
    authority).
- *`BISTABLE` or `MIXED` at any cell:* the anchor becomes the user's design
  decision: a lifetime minimum, a drain, or another brake. It is presented
  with R3's ask/H and R4's brake trajectories, and no engine L̂ work starts
  before that decision.
- *`UNSTABLE` at any cell:* §42's next step 3, the split diagnostic (sender
  dwell against the endpoint loop, at µs), comes before anything else.
- *`ONE-POINT-OFF-PREDICTION` at any cell:* that cell's map is re-derived
  from this battery's uncapped windows before any prediction for it is
  quoted again.
- *`UNSCOREABLE` at any cell:* the cell's question stays open. The missing
  rows are named, and a re-run needs an amendment that says why they failed.
- *`GOODPUT` and `DELAY`* go to the user as the price of the power point.
  No flip.

**Predictions (before any number).**
- **c2:**
  - SL: ñ_SL ≈ 157–164, with spread `AS-PREDICTED` (≈ 0.08).
  - SH: `HOLD-EXPIRED` (a ≈ 150 ms queue held 15 s); ask/H ≥ 1.0 at the
    end of the hold; the row runs to the pin.
  - Label: **`BISTABLE`**.
  - `GOODPUT` `WORSE` by 5–12 %; `DELAY` `BETTER` (Tp50 ≈ 16–19 ms against
    ≈ 40–100 ms).
  - `wbind` ≈ 0 in SL's last third and > 0 during SH's hold. n_k ≈ 2–2.5 k.
- **c1s:**
  - SL: ñ_SL ≈ 614–699, spread ≈ 0.11.
  - SH: ask/H ≈ 0.76; it returns within ≈ 3 windows.
  - Label: **`CONVERGES-ONE-POINT`**. The stated risk is
    `ONE-POINT-OFF-PREDICTION`, because the predicted map was built from
    truncated windows.
  - `HOLD-EXPIRED` is expected, with RTprop rising from ≈ 2.6 to ≈ 4.3 ms
    (the ledger's Tmin floor at 2 048). The stated risk is `HOLD-INERT`,
    which would leave c1s's start-high question `UNSCOREABLE`; a return
    from 2 048 is expected even with a fresh anchor (ask 1 372).
  - `GOODPUT` `WORSE` by 7–17 % (≈ 685–773 against ≈ 826 Mbit/s); `DELAY`
    `BETTER` (≈ 6–7.5 against ≈ 19 ms).
  - B ≈ 50–190 datagrams (§34: ≤ 50 at caps ≤ 512, 128–192 at 768).
  - n_k ≈ 8–18 k (§42 saw 8–15 k at c1; the bound is L·T_w ≈ 24 k), so
    most windows lie above 8 192.

**Duration.** Build ≈ 6 min. Suites ≈ 1.5–2 h. The 20 invocations take
≈ 25 min (rows of 48–60 s plus ≈ 15 s of bring-up). All of it fits inside
`LD_HARD`.

**Gate.** STOPPED before the build. The arm is engine code (default off,
not for merge). Its build, the host gate (a)–(g), the harness plan and the
VM session are the next phase, on the coordinator's go.


### 43. Amendment 1 — readouts, the timeout bound, branches and order (before any build)

No outcome or label changes.
- **R9 (reported): store occupancy per window**, the `win=` numerator ÷ n,
  beside quinn's backlog B. The predicted c1s point is where quinn shares
  authority (§34 R4). If the store is not full there, n is not the brake,
  and R1's operating point is read with that caveat.
- **R10 (reported): the lowest per-window sample count per row**, and the
  share of windows with fewer than 8 samples. S from a one-sample window is
  0 and drops n to (1+q)·L·RTprop.
- **Timeout bound.** `RWM_PERF_TIMEOUT_S=200` bounds a collapsed row (c2 at
  the floor would need ≈ 11 min for 600 MB). The worst case is 20 × 200 s
  ≈ 67 min, inside `LD_HARD`.
- **Branches.** The arm, the hold and the MAIN-side sampler readout are
  built on `run43`, which is never merged (as `run34`). Only this section,
  the paper block and the plan-43 harness go on `diag36`.
- **Order.** §43 launches only after §41's BASE gate reads WITHIN, because
  §43's MAIN arm is `diag36`'s engine. If §41 reverts the placement arm,
  `run43` is rebased first.
- **Before VM contact:**
  - the plan-43 scorer is dry-run on ledger rows (p29's power rows, where
    the cap moves, and the p27/p34 static rows). Its labels — returned,
    OSCILLATION, RUNAWAY, SHORT — must behave as specified;
  - one unscored check row confirms the sampler and routing witnesses.
- **No VM suites:** `run43` is not a ship tree. The host gate is debug-
  profile targeted tests (a)–(g).
- **Open for any real law, not this diagnostic.** The 250 ms window sits
  below its own lower bound Tp50/q off Bulk (≈ 285–380 ms at c2).



### 43. Amendment 2 — the scorer rulings, the two diagnostic envs, the c2 offset and gain rows (before VM contact)

- **Rulings applied** (`harness43` `6e9ff1b`):
  - COLLAPSE is the DNF row only, because the floor clause is unreachable:
    the ask is at least (1+q)·L·RTprop after the first sample.
  - "Scored" rows are LIVE and not SHORT. SH rows count only if
    HOLD-EXPIRED.
  - At c1s, when at least 2 SH rows read HOLD-INERT and at least 2 SL rows
    are scored, the SL rows carry the cell. The label then carries
    `[SL-ONLY: SH cannot test bistability at c1s]`.
- **Two diagnostic envs on `run43`.** Both are declared oracle inputs, not
  law terms, and are echoed. Red-first and default-off tests are in place;
  the debug lib passes 632/0.
  - `RWM_ORACLE_GAIN` (`2ca74cb`) replaces (1+q(δ)) with a fixed
    multiplier.
  - `RWM_ORACLE_BRAKE_OFFSET_US` (`b2b75b7`) adds b₀ to the time term,
    giving RTprop + S + b₀.
- **Added rows (reported only; they do not enter §43's outcome).** All are
  start-low at c2, 600 MB, n = 3 each (seeds 42 / 7 / 42):
  - `SL-B0`: b₀ = 2 470 µs (the §44-erratum no-queue brake), gain unset.
    Predicted operating point ≈ 195, goodput ≈ 0.985 Gmax, Tp50 ≈ 19.5 ms.
  - `SL-G12`: gain 1.2, b₀ unset. Predicted ≈ 184, ≈ 0.957 Gmax.
  - Readouts R1–R10 as for SL rows, plus goodput and delay against §43's
    MAIN rows.
  - 6 more rows; the total is 26 invocations, within `LD_HARD`.
- **Prediction caveat.** These are offline-map predictions from p27
  (static rows, pre-§32 receiver), not closed-loop ones.

### 43. Result

Scored 2026-11-10 from the `oracle43` ledger on `harness43` (`4ad1fdf`):
24/24 LIVE, no DNF, no abort in the main run. **c2 `BISTABLE`; c1s
`UNSTABLE`.** Diagnostic only; nothing ships.

*Session.*
- **Tree:** `run43` `b2b75b7` plus `harness43` `f6b06ff`. Binary
  `94959c05…c1dc008`, built fresh.
- **Check run:** the first launch hit `ABORT-PRECONDITION`, caused by the
  worker's own ssh. The relaunch passed all witnesses: sampler Σ n_k =
  offered, `nk_min` 14 253 at c1s (above the old 8 192 cap), routing 1.0,
  and the 5 GB row ran. It then stopped by its deliberate `ABORT-BUDGET`.
- **Main run:** 15:54Z, `DONE-ALL`, battery 1 398 s.
- **The `ENGINE-STALL` rule** (the note before the score) was implemented
  as a DNF-only classifier. With 0 DNF rows it never fired.

*c2 (600 MB; 3 rows per arm).*

| arm | n_end | goodput | Tp50 | note |
|---|---|---|---|---|
| SL | 168 / 156 / 163 | 82.8–85.3 (WORSE than MAIN by 6.3) | ≈ 17 ms | `wbind` 0 |
| SH | 1 814 / 1 855 / 1 618 | — | — | all NOT-RETURNED (HIGH), HOLD-EXPIRED; ask/H over the hold's last 2 s: 1.006 / 1.059 / 1.005 |
| MAIN | pinned at 2 048 | 89.9–90.3 | ≈ 208 ms | A_x ≈ 180 ms |
| SL-B0 (b₀ = 2 470 µs, reported) | 207 / 239 / 233.5 | 89.3 / 90.1 / 89.6 (WITHIN MAIN) | 20.6–23.3 ms | A_x 9–11 ms (BETTER) |
| SL-G12 (gain 1.2, reported) | 201.5 / 206 / 165 | 87.7 / 89.6 / 84.6 (WITHIN) | 17.0–20.2 ms | A_x 7–9 ms (BETTER) |

*c1s (5 GB).*
- SL: 828.5 / 588 / one RUNAWAY row (`SL s42 r2`, pinned at N·knee from
  5.8 s, 292 Mbit/s, 136.7 s).
- SH: s42 r1 and r2 RETURNED (16.6 s and 16.8 s; ask/H 0.83 and 0.80).
  SH s7 was HOLD-INERT, left out.
- MAIN pinned at 2 048, Tp50 20–29 ms.
- Goodput and delay of SL against MAIN: WITHIN, at a wide 2σ.

*Readouts R1–R10.* The full set is in `oracle43/score.txt`: final point and
spread, goodput and delay against MAIN, return time, ask/H, the brake over
time, bind fractions (`wbind`, floor, pin, hold), sampler counts, quinn's
backlog, occupancy, and the lowest window count.
- Spread: c2 0.05 against the predicted 0.08; c1s 0.103 against 0.11.
- The c1s hold threshold margin was thin, as predicted (3.0–3.2 ms against
  RTprop 3.2–3.5 ms).

*Predictions.*
- c2 `BISTABLE`: MET.
- Plain start-low goodput −5…−12 % at c2: MET (−7 %).
- c1s `CONVERGES`: MISSED (one runaway).
- The offset arm's location: MISSED (median 233.5 against 195). The offline
  map under-reads, so its 648 threshold is equally soft.
- Gain 1.2: 201.5 against 184.

*The c1s runaway is the substrate, not the law.* From its kept client log:
- up to t = 4.0 s, quinn's window `qcwnd` is ≈ 500 KB and goodput ≈ 850;
- at 4.3 s `qcwnd` falls to ≈ 100 KB and stays there. Goodput drops to
  ≈ 330, and wire RTT stays at 2 ms;
- only then does n climb to the clamp, because with an oracle L any
  delivery shortfall reads as a need for more store.

It is not §42's anchor mechanism: it happened before the 10 s filter could
expire. §42's split diagnostic is not the next step for it.

*Reading for the user.*
- At c2, with an oracle L, the offset arm held MAIN's goodput with no
  goodput loss resolvable at ≈ 2 %, at about a tenth of MAIN's delay
  (A_x 9–11 ms against ≈ 180 ms). Caveats: three reported-only rows,
  start-low, Bulk, oracle L and oracle offset.
- MAIN's delay grows with transfer length: 208 ms Tp50 at 600 MB, against
  the 40–100 ms quoted from 100 MB rows.
- The δ-authority revisit trigger (a measured δ→latency trade on the wire)
  is met.

*Next (offline, then possibly one short session; no build).* At gain 1.1
the high point is held by the last few percent of the gain: ask/H was
1.006–1.059, so at gain 1.0 it would read 0.91–0.96 and fall. Check a third
anchor option before "lifetime minimum or drain" goes to the user: gain ≤ 1
with the headroom as an additive time, whose share of ask shrinks as n
grows. Sweep gain and offset offline on c2 and c8, looking for a stable
point in the knee band with no upper crossing on the expired-anchor map.
If one exists, register start-low and start-high at c2 with `run43`'s
existing envs (≈ 8 min, no build).

### 41b. Erratum — the slow c1s row is a quinn congestion-window collapse, not a SACK-release stall

The §41b result called the 68.3 Mbit/s MAIN row an echo and SACK-release
stall. Its kept client log shows otherwise:
- quinn's `qcwnd` falls from ≈ 600 KB to **13.8 KB** at t ≈ 1.8 s and
  stays there;
- goodput sits at ≈ 39 Mbit/s;
- by Little, 2 048 / 6.7 k/s ≈ 310 ms, which is the echo RTT it showed,
  with the store queued behind quinn's tiny window (backlog 2 041 of 2 048).

§43's c1s runaway is the same event: `qcwnd` drops from ≈ 500 KB to
≈ 100 KB. **Rare c1s rows run with quinn's BBR window collapsed and not
recovering.** That replaces both the "SACK-release stall" and §42's split
diagnostic as the c1 diagnosis item. It is also the likely cause of every
earlier unexplained slow c1s row (§34's 277, §38's 575 and 684).

### 43. Addendum — gain ≤ 1 plus an additive time (offline, `s42/s43tau.out`), and the §43b registration

*Sweep.* The form is ask = G·ΣL·(RTprop + S + τ), over G ∈ {0.85…1.0} × τ ∈
{0…10 ms}, on p27 at c2 and c8, on both fresh and expired anchor maps.
- **Eight pairs give** a stable point in the knee band on both maps, with no
  upper crossing inside the grid, at both cells. They also stay in band
  after the ×1.2 closed-loop correction (§43: 233.5 / 195).
- For example, (0.9, 5 ms) gives c2 181/178 (≈ 0.95 Gmax, Tp50 ≈ 17–19 ms)
  and c8 313/288.

*This does not dissolve the anchor question (advisor).*
- Once the anchor has absorbed the queue, the law is affine:
  ask = G·(L/D)·n + G·L·(τ − β), with β the remaining brake. A pair is
  trap-free only while τ < β(n) + (1 − G·L/D)·n/(G·L) above the knee, and
  it reaches the knee only if τ clears the no-queue brake b₀.
- So τ has a window per link. At c2 it is roughly 4–9 ms. At c1s, β ≈ 1–4 ms
  and b₀ < 1 ms, so a fixed 5 ms likely lies outside it.
- The eight pairs are a two-parameter fit on two cells, made after seeing
  §43.
- **Pending (offline):** print b₀, β(n) and the window per cell, including
  c1s from `oracle43`'s uncapped rows. Test which form of τ fits every
  window with one coefficient: a constant, k·RTprop, or k·b₀. That is τ's
  provenance and δ's seat.

### 43b. The gain ≤ 1 + additive-time law at c2, from both starts — pre-registration

*Question.* With G·L/D < 1 and an additive time τ, does c2 settle at one
point in the knee band from both a low and a high start, with goodput not
worse than MAIN's?

*Pair selection (the rule is fixed now and applied by script to
`s43tau.out` before VM contact; the chosen pair is recorded in an amendment
before launch):*
1. G·1.016·1.04 < 1. This keeps the map's high-n limit below 1 with room
   for L̂'s measured +1–4 % over-read, so G ≤ 0.9 on this grid.
2. Both ×1.2 points (fresh and expired) at c2 are at least 25 below the
   band's upper edge (288).
3. Among those, the lowest expired-map slope at c2, ties within 0.1.
4. Then the highest predicted c2 G/Gmax.

*Design.*
- **Tree:** the §43 binary (`run43` `b2b75b7`, sha `94959c05…`; the sha must
  match), using `RWM_ORACLE_GAIN` = G and `RWM_ORACLE_BRAKE_OFFSET_US` = τ.
- **Rows:** c2-100, 600 MB, Bulk, three arms with 3 rows each (seeds
  42 / 7 / 42), interleaved:
  - SL-τ: start-low;
  - SH-τ: start-high, hold 1 536 for 15 s, then the law;
  - MAIN, same session (cross-session references drift, §19).
- 9 rows plus a 2-row smoke, about 12 min. No suites.
- **Scoring:** §43's ladder and scorer, unchanged, with this cell's
  predicted band restated as "offline value to ×1.2".
- **Witnesses:** each row's echo declares its own `gain` and `b0_us`.
- **Reported per row:** the lowest `qcwnd` against its median, so a quinn
  window collapse is not scored as the law.

*Predictions.*
- From §43's hold, ask/H = G·r/1.1 + G·L·τ/H, with r ∈ {1.006, 1.059,
  1.005}. That is below 1 for every eligible pair, so the start-high rows
  return. The return alone is weak evidence.
- The informative readouts are:
  - where both starts settle (inside the ×1–1.2 predicted band);
  - the return time;
  - goodput WITHIN MAIN's;
  - delay BETTER than MAIN's.
- **Expected label:** CONVERGES-ONE-POINT.

### 43b. Amendment 1 — the pair, the band, a closed-loop prediction, and the limit of a pass (before VM contact)

*Pair, chosen by the registered rule* (`s42/s43win.out`): **G = 0.9,
τ = 5 000 µs.** Rule 1 leaves G ≤ 0.9. Rule 2 leaves (0.85, 5), (0.85, 7.5)
and (0.9, 5). Rule 3 keeps (0.85, 5) and (0.9, 5) at expired slope 0.28.
Rule 4 picks (0.9, 5).

*Band:* `BAND43B = [178, 217]`, the offline point to ×1.2 over the fresh
and expired maps.

*Second prediction (closed-loop line, `s42/s43win2.out`).* §43's c2 rows
give the law's time term T(n) = 14.7–14.9 ms + 0.057–0.062 ms·(n − 163).
This fit predicts §43's arms within 2–8 symbols. It predicts the
start-low point at ≈ 186–193. Start-high should return to the same point:
(0.9, 5) applied to §43's SH windows gives ask/n of 0.84–0.85 in the
median.

*The limit of a pass, stated now.* c2's τ window is [0.9, 9.2) ms. A pass
shows that a gain below 1 removes the high point at c2. It does not settle
τ's value or form across links.
- Constant τ fits c2/c8/c1s only within [2.88, 3.94) ms at G = 0.9.
- τ = b₀ + c fits with c ∈ [1.40, 3.64) ms, twice as wide. Every edge is
  offline, and c1s's lower edge rests on truncated minima.
- A c1s registration follows only if c2 converges. At G = 0.9, c1s is
  predicted to have one settling point per τ: ≈ 1 250–1 300 at 3.5 ms and
  ≈ 1 530–1 560 at 5 ms, with no second point.

*b₀'s engine source.* `qsp_us` (P90 − P50 of a leg's last 256 RTT
samples, `path.rs:848–855`) is on b₀'s scale at c2 and c8 (×0.75–1.15) but
reads 2–3 × b₀ at c1s. It is not a drop-in source. The dispersion gauges
stay out of the simplify lane until this is decided.

### 43b. Result

Scored 2026-11-10 from `docs/l1-raw/oracle43b/` (`harness43c` `f787c96`):
9/9 LIVE, no DNF, no abort. **c2 `CONVERGES-ONE-POINT`** at G = 0.9,
τ = 5 ms.
- Binary `94959c05…`, equal to §43's. Battery 17:11–17:20Z.
- The score's "at both cells" decision line is §43's wording. It is
  cosmetic: only c2 ran.

*Readings.*

| start | n_end (s42 r1 / s7 / s42 r2) | median | t_ret after release | ask/H, hold's last 2 s |
|---|---|---|---|---|
| SL-τ | 191 / 196 / 184.5 | 191 | — | — |
| SH-τ | 198 / 178 / 187 | 187 | 11.8 / 5.8 / 9.3 s | 0.840 / 0.870 / 0.859 |

- **Goodput:** SL-τ 89.4 / 89.5 / 87.0 against MAIN 90.3 / 90.3 / 90.3, so
  −1.68 against a 2σ of 2.00. **About 2 % lower, not resolved at n = 3.**
  SH-τ reads 88.4–90.0 after release.
- **Delay:** Tp50 18–19.6 ms against MAIN's ≈ 208 ms (MAIN pinned at
  2 048). A_x is BETTER.
- **Spread** s_end 0.042–0.068 (predicted 0.08). `wbind` 0. Sampler
  `nk_min` > 1 500.
- All SH rows HOLD-EXPIRED.

*Predictions.*
- The band [178, 217]: MET on all 6 rows (178 sits on the edge).
- The closed-loop line, 186–193: MET on both medians (191, 187).
- The predicted ask/H of 0.84–0.85: MET.
- The closed-loop line T(n) = 14.7–14.9 ms + 0.057–0.062 ms/symbol has now
  predicted four c2 arms. It is preferred over the offline maps.

*Reading.*
- **Return time is a cost:** 5.8–11.8 s of high delay after an excursion.
  The floor is W·ln(H/n*) ÷ −ln(G·L/D), about 5.8 s at one window per step.
  If it holds against the rows, G has a meaning, escape rate against
  goodput, which would be its provenance. To be checked.
- **No δ in this form.** q(δ) was δ's only seat on the store law, and the
  gain 0.9 replaces it. δ would have to enter through τ. (RFC 8289's
  absolute 5 ms target is a candidate provenance for τ, to be verified.)
- **Any delivery shortfall runs it to the clamp.** With L as link capacity,
  ask/n > 1 once D/L < G. §43's quinn-collapse row is that case, and a
  competing flow or a receiver-bound cell is the same algebra. It is no
  worse than MAIN, which already sits there. It is a paper item before any
  engine L̂ work.
- **Anchor:** the lifetime-minimum-or-drain choice is not needed for
  own-queue excursions at c2.
- **The qcwnd readout does not discriminate.** Healthy rows read 0.11–0.15
  and the collapse row 0.10. It is replaced by a duration measure: the
  share of windows with qcwnd below ¼ of the row median, checked on
  `oracle43` and `place41b`.

*Next.*
1. The merge-candidate VM session.
2. The quinn window-collapse diagnosis, read-only: kept logs and quinn's
   BBR source.
3. c1s at τ = 3.5 and 5 ms, to test the window prediction, with the fixed
   collapse witness.
