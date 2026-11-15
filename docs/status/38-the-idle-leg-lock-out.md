## 38. The idle-leg lock-out — the wire-clocked placement base (`RWM_PLACE_WIRE`) — pre-registration

§37 verified the defect (B2): placement prices a leg from the app-echo
srtt, which freezes when the leg carries nothing, so a leg that idled while
its own backlog drained keeps that dwell as an absorbing price. Paper §5.7
derives the correction and scores it on the committed ledgers (Table 5.7a,
`tools/l1/place38_ledger_score.py`). This section pre-registers its build
gate and a battery against main, same binary. Nothing here touches the VM.

**Premises checked (no VM).**
- *The joint derivation's (RTprop + S), kept when idle, does not cure the
  lock-out.* S on main is app-echo, so its last window is the dwell spike
  that idled the leg: 140.6 ms in c1d DEF1024 s42 r2, 126.6 ms in c8 DEF256
  s42. At I_A = 0 the idle leg's gap is still +2.7 to +58 nats on the 6
  locked c1d rows (4 of 6 ≥ 20) and +1.0 to +25 on the 5 locked c8 rows.
- *Ŝ per leg on main.* The only per-leg spread is Copa's `rtt_win` (the
  last 256 app-echo samples), which is dwell-inclusive and frozen when idle.
  No dwell-free distribution exists on main. The dwell-free per-leg delay on
  main is quinn's smoothed srtt (`PathView.rtt_us`), which reaches the
  scheduler only under `RWM_COPA_WIRE` (off; `copa_wire=false` on the
  ledgers' `[GATES]`).
- *Windowed-min staleness.* Copa's 10 s min expires only inside
  `record_rtt`, so an idle leg holds its lower envelope (optimistic). Under
  a standing queue held for more than 10 s it expires upward, and an idle leg
  then holds a pessimistic floor. Nothing keeps it honest except a drain
  probe, which is a mode. The corrected price does not read it.
- *The defect is absent at main's default store law.* §32's battery and
  store-law-n1 have 0 lock-outs in 24 c1d and 12 c8 rows. Every full-row
  lock-out froze within the first second under a static `RWM_STORE=<n>`:
  - p34 c1d: 6 of 18 rows locked for the whole row, plus 2 that froze
    mid-row;
  - p27 c8: 5 of 8 at caps 256–768.

  A default-only battery would read "no cost" from a no-op, so the cure is
  scored at static caps where main reproduces it, and the default cells are
  no-regression cells.
- *c1d's cost and c8's gain have opposite signs.*
  - c1d, within a cap: locked one-leg rows ran +1.6 to +16.0 % ahead of
    two-leg rows (×1.35 μ_R).
  - c8: locked rows ran 88–90 Mbit/s, against 102–103 for two-leg rows at
    caps 384–768.
- *The cold price.* r = 10 never binds (`cold_r = 0`). The binding cold
  price is the 50 ms srtt seed. Paper §5.7/§11.2 are corrected. The r clamp's
  step at ε̂ = 1 is owed and outside this arm.

**The arm (paper §5.7).** `RWM_PLACE_WIRE=1`, default off, echoed on
`[GATES]`:

```text
   E_i  =  (I_i/W_i)·s_i  +  (½ + ε̂_i)·ŵ_i,    ŵ_i = wire_i ?: min_j wire_j ?: s_i
```

The arm and the placement law are the same everywhere else.
- ŵ_i is mirrored into `PathState` for every leg at every ack batch on any
  leg and at every path report.
- The `[ETA]` stamp (`emit_source.rs` `E_picked`) and X_i read the same E_i.
- Gate off: the cost table is byte-identical
  (`place_costs_match_the_pinned_table`), and `[ETA]`'s added fields render
  `-`.
- New gauges (paper §5.7): `[ETA]` `ref_fl=`, `w_cold=` and `w_mv=` (the
  argmin-moved execution witness), and `[DIAG]` `p<i>: wpl_us=`.

**Build gate (red-first, before any VM contact).** Each test is written
first and fails on main, on its assertion where the API allows it, and
otherwise because the arm's setter is missing.
- Tests in `src/scheduler/tests.rs`. They feed Copa through
  `record_rtt_sample` under `MockClock`, not through `set_rtt` (which feeds
  only the estimator and leaves Copa's min and window empty).
  1. `idle_leg_price_is_its_wire_transit_not_its_frozen_srtt`. This replays
     the p34 c1d r2 shape: leg 1 gets 2.5 ms floor samples, then a spike to
     156 ms, then silence for 5 s on the mock clock. Leg 0 is active at
     9 ms. With I_1 = 0, wire_1 = 3 ms and the arm on, leg 1's cost equals
     (½ + ε̂_1)·3 ms / ref to 1e-12. With the arm off it equals
     ½·s_1/ref, with s_1 ≥ 100 ms (the defect, pinned).
  2. `idle_leg_is_not_absorbing_at_symmetric_legs`. This uses the same
     state, with I_0/W_0 = 0.6. Arm on: P(leg 1) ≥ ½. Arm off:
     P(leg 1) < 1e−15.
  3. `symmetric_legs_at_equal_state_split_exactly_half`. Equal
     (I, W, s, ŵ, ε̂) at three load levels (I/W = 0, 0.5, 1.5), 2 and 3
     legs, arm on and off: every p = 1/N ± 1e-12.
  4. `wire_base_equals_shipped_when_echo_equals_wire`. Every state of
     `cost_table_states()` with wire_i := s_i, arm on, matches the pinned
     table to 1e-12. This is the c8-fresh preservation, made absolute.
  5. `c8_fresh_state_share_is_unchanged`. The p27 c8 DEF128 s42 state
     (s = 16 / 45 ms, ŵ = 14 / 43 ms, ε̂ = 0.026 / 0.017, I_0/W_0 = 0.38,
     I_1 = 0, Bulk) must satisfy |ln(P1_on/P1_off)| ≤ |δg| + 1e-9, where δg
     is paper §5.7's open-loop shift computed in the test (0.007 nats here).
  6. `locked_c8_leg_is_sampled_again`. This is the p27 c8 DEF256 s42 state
     (s = 24 / 390 ms, ŵ = 23 / 45 ms, I_1 = 0). Arm on: P(leg 1) ≥ e^−4.
     Arm off: P(leg 1) < 1e−20.
  7. `own_cost_is_increasing_in_own_in_flight_with_the_wire_base`, the
     fixed point's premise: c_i is strictly increasing in I_i, with the
     other leg held.
  8. `unmeasured_wire_takes_the_best_measured_wire`. With wire_1 absent,
     ŵ_1 = min_j wire_j, and `w_cold` counts it.
  9. `wire_arm_is_independent_of_the_dial`. At δ·(1 ± 0.02) around
     Realtime, Auto and Bulk, the arm's difference c_on − c_off is the same
     to 1e-12. So the arm adds no δ dependence and no step at a preset, and
     the existing continuity gates carry over unchanged.
  10. `every_arm_absent_leaves_the_probabilities_bit_identical`, extended to
      `set_place_wire(false)`.
- `src/gates/tests.rs`: `place_wire_resolves_off_by_default_and_echoes`.
- `tests/place_wire_loopback.rs` (rule 1: the wiring routes there):
  - `wire_srtt_mirrors_every_leg_including_an_idle_one`. A two-leg loopback
    transfer in which leg 1 is held idle for ≥ 1 s by a test-only placement
    override, then released. In every `[DIAG]` line, each leg's `wpl_us`
    lies within 1 ms of `wrtt`, including leg 1 while its `infl` = 0.
  - `eta_stamp_reads_the_arms_E`. With the arm on, the stamped `E_picked`
    equals `expected_delivery_load` of the arm's E for the picked leg.
- Full suites on the build (release, debug, doc, wasm, parsers), as §32.
  A real failure is `ABORT-TESTS`. The wasm golden fingerprints do not move
  (no visualizer change).

**Battery.** One binary, built fresh on disk and run from `/dev/shm`. Arms
`MAIN` (gate unset) and `WIRE` (`RWM_PLACE_WIRE=1`).

| cell | store | role |
|---|---|---|
| `c1s-400` | default | no-op control (N = 1) |
| `c2-100` | default | no-op control (N = 1) |
| `c1d-400` | default | no regression |
| `c8-100` | default | no regression; fresh-share preservation |
| `c1d-400` | `RWM_STORE=1536` | the cure and its c1d cost (p34 rows: 2 of 3 froze) |
| `c8-100` | `RWM_STORE=256` | the cure (p27: 2 of 2 locked) |
| `c8-100` | `RWM_STORE=512` | the cure past the store-limited regime (p27: 1 of 2) |

- *n and seeds.* n = 6 per arm and cell (3 reps × seeds 42 and 7), arm
  order rotated per block, as §32. 84 rows after a 2-row smoke (`c8-100`
  `RWM_STORE=256` in each arm).
- *Env.* §32's: `RWM_RTOBS=1 RWM_RDIAG=1 RWM_GEN=0 RWM_DIAG=1
  RWM_PERF_TIMEOUT_S=150`, the standard `env -u` list, Bulk,
  `--window-reliable`, shipped defaults apart from the column above.
- *Harness.* `tools/l1/place38_run_all.sh`, `place38_battery.sh` and
  `place38_parse.py` (tested by `test_place38_parse.py`), derived from
  §32's `threadpd_*`, scoring through `l1ab.py`.
- *Rules (shared file).* Launch precondition: both locks free (`test -e`
  only), no cargo/rustc (`pgrep -x`), `procs_running` ≤ 2 in each of 5
  samples 2 s apart. Both locks for the whole session. Binary and run dir
  on `/dev/shm`, with the `ABORT-ONDISK` guard. io `full avg10` before and
  after every row, flagged at ≥ 20, never voided.

*Witnesses (row void otherwise).*
- Binary: `[GATES]` echoes `RWM_PLACE_WIRE=0` on MAIN rows and `=1` on WIRE
  rows, and `RWM_STORE` matches the cell.
- Store: `[SUMCAP] eng` is absent or 0. At static cells every post-warm-up
  `win=` denominator equals n.
- WIRE, two-leg cells:
  - `wpl_us` is numeric on both legs in every post-warm-up `[DIAG]` line;
  - it lies within 1 000 µs of `wrtt` in ≥ 95 % of those lines;
  - the final `[ETA]` line has `w_mv` > 0.
- WIRE, N = 1 cells: `w_mv = 0` exactly (one candidate). A non-zero value is
  a `REGRESSES` finding, not a void.
- MAIN rows: `wpl_us=-`.

**Readouts (fixed now).**
- *Lock-out (per row and leg).* A leg is LOCKED iff its `[TRUTH]` egress
  share is < 10 % and its median post-warm-up (t ≥ 1 s) `[DIAG]` `rtt` is
  ≥ 3 × its median `wrtt`. A row is LOCKED iff any leg is. This is wider
  than Table 5.7a's 2 % so that a leg frozen mid-row counts. On the ledgers
  it classifies 7 of 18 p34 c1d rows, 1 p27 c1d row (DEF768 s42) and 5 p27
  c8 rows as LOCKED. No fresh row is LOCKED: the fresh c8 slow legs read
  `rtt`/`wrtt` ≤ 1.25.
- *Share.* The slow leg's (c8) or second leg's (c1d) `[TRUTH]` egress
  share, as median [min–max].
- *Clauses.* §21's pass rule per cell, WIRE against MAIN: goodput, client and
  server CPU per byte, RTprop per leg, `[LAG]` p99 client and server, fed
  loss and DNF. The MDEs are the cell's (c1s 4.9 %, c1d 5.6 %, c2 1.4 %,
  c8 4.0 %), and WORSE / TREND-WORSE / BETTER / TREND-BETTER are as §21.
- *c1d cost (stated, always reported).* At `c1d-400` `RWM_STORE=1536`,
  WIRE − MAIN goodput as % of MAIN's median, with min–max. It is also
  reported stratified:
  - MAIN one-leg rows (LOCKED, or any leg share < 5 %) against MAIN two-leg
    rows, which is the lock-out's accidental gain;
  - WIRE rows against MAIN two-leg rows, which tests whether the arm costs
    anything beyond that gain.
  The same readout is given at `c1d-400` default.
- *Reported only.*
  - Idle-leg ŵ decay: per WIRE row, the largest post-warm-up `wpl_us` on a
    leg with `infl` = 0, and its value 1 s later.
  - `w_cold`, `ref_fl`, `cold_ge`, `cold_r`.
  - The `[ETA]` `e_p50` / `e_p90` per arm.

**Outcomes and decision rule (one verdict, in precedence order).**
1. `UNSCOREABLE`: an abort cause fired, or a cell has < 3 live rows in
   either arm.
2. `NOT-REPRODUCED`: MAIN has < 2 of 6 LOCKED rows at every static cell. The
   cure cannot be read. The default and control readings are recorded, and
   nothing is decided on the arm. A single static cell that does not
   reproduce is named, `NOT-REPRODUCED-AT(<cell>)`, and dropped from
   outcome 4.
3. `REGRESSES-AT(<cells>)`, if any of these holds:
   - a §21 clause is WORSE at a control or default cell;
   - `w_mv` ≠ 0 at a control;
   - c8 default's slow-leg share medians differ by more than 0.03 between
     arms (the fresh-share preservation);
   - goodput is WORSE at a reproduced c8 static cell.
4. `NOT-CURED`: any WIRE row is LOCKED at a reproduced static cell, or a
   reproduced c8 static cell's WIRE goodput is not at least TREND-BETTER.
5. `CURED-AT-COST(<x %>)`: none of the above holds, and the goodput clause
   at `c1d-400` `RWM_STORE=1536` reads WORSE or TREND-WORSE (x = the stated
   cost). Nothing ships on this verdict. It goes to the user with x, the
   stratified c1d readout and the c8 gain. The choice is then either to ship
   the cure and accept x at c1d under large static caps, or first to price
   the receiver's per-leg cost, which is why one-leg rows run ×1.35 μ_R.
6. `CURED`: as 5, with that clause WITHIN or better. This is a ship
   candidate, merged on the user's go.

No tuning in this battery: there is no re-run at another cap, threshold or
gauge. A follow-up is a new registration.

**Predictions (before any number).**
- *MAIN lock-outs.* About 4 of 6 at c1d `RWM_STORE=1536` (p34: 2 of 3; p27
  0 of 2 on an older binary), 5–6 of 6 at c8 `RWM_STORE=256`, about 3 of 6 at
  c8 `RWM_STORE=512`, and 0 of 6 at both default cells.
- *WIRE lock-outs.* 0 everywhere. At I_A = 0 the arm's gap on every ledger
  lock-out is ≤ +3.8 nats, against +17 to +116 shipped.
- *c8 `RWM_STORE=256`.*
  - WIRE share: 0.03–0.19 (wide; the fresh rows bracket it at 192 and 384).
  - WIRE goodput: 95–103, against MAIN's 88–90, so BETTER.
- *c8 `RWM_STORE=512`.*
  - WIRE share ≈ 0.15 and goodput ≈ 102.
  - MAIN mixes locked rows (about 90) with fresh ones (about 102), so the
    clause reads TREND-BETTER or BETTER.
- *c1d `RWM_STORE=1536`.*
  - WIRE goodput ≈ MAIN's two-leg level (p34 762).
  - The pooled cost is −5 to −12 %, so WORSE or TREND-WORSE.
  - WIRE against MAIN's two-leg rows reads WITHIN.
  - Expected verdict: `CURED-AT-COST`.
- *Defaults.* c1d: two legs in both arms (share 0.3–0.7), all clauses
  WITHIN. c8: share 0.17–0.19 in both arms (|Δ| ≤ 0.03), all clauses
  WITHIN.
- *Controls.* All clauses WITHIN, and `w_mv = 0`.
- *Idle-leg ŵ.* It moves on idle legs, as on the ledgers (c1d 3.0 → 2.6 ms;
  c8 p1 42 → 46 → 45 ms). Its decay after a netem queue is slow (269 →
  216 ms over 0.8 s), and it is reported only.

*Abort table:* §32's (`ABORT-LOCK`, `-CRLF`, `-SHA`, `-SMOKE`, `-BUILD`,
`-TESTS`, `-RC`, `-BRINGUP`, `-BUDGET`, `-ONDISK`, `VOID-COTENANT`),
filled in the result.

**Duration.** Build ≈ 6 min, then suites and tests as §32. The 86
invocations take ≈ 35 min (c8 rows ≈ 20 s, others ≈ 12 s), and scoring
≈ 5 min. That is inside the 4 h launch deadline and the 5 h cap. This
section stops here: the build and the VM session start only on the user's
go.

### 38. Amendment 1 — the stamp, the fresh-share cell, the ladder, the confound (before VM contact)

A review of the registration above found four gaps. They are fixed here,
before any build or VM contact.

- *The `[ETA]` stamp stays on the shipped E.*
  - The arm as written above also moved `E_picked` (`emit_source.rs`).
    That stamp rides the wire and feeds `[ETA]`/`[LAT]` and F̂, so the arm
    would have had a second mechanism, and c1s and c2 would not be no-ops.
  - The arm now changes `place_costs` alone; it keeps the ŵ mirror reads and
    its gauges. Paper §5.7 says this.
  - The stamp's predictor then differs from the law's by the local dwell
    (½ + ε̂)·(s − ŵ), and X_i with this arm would compare the two. Neither
    `RWM_PLACE_T_DERIVED` nor `RWM_PLACE_HOL` is combined with it here.
  - Build-gate test `eta_stamp_reads_the_arms_E` is replaced by
    `eta_stamp_is_the_shipped_E_under_the_wire_arm`: arm on and off, the
    stamped `E_picked` is bit-identical.
  - The controls' witness becomes two parts: `w_mv = 0`, and the `[ETA]`
    line's `e_p50`/`e_p90` reads WITHIN between arms. The second is
    reported, not gated.
- *The regime the share must be preserved in is now in the battery.*
  - The share clause above sat at c8 default (link-saturated, share ≈ 0.18).
    The low-share regime is c8 caps 128–192.
  - That regime is where Table 5.7a's largest open-loop shift sits: −2.6 nats
    at DEF192 s42, about 5.7 % → 45 % in share terms. In that row the slow
    leg carried a stale local dwell (`rtt` 84 against `wrtt` 68).
  - Cell added: `c8-100` `RWM_STORE=192`, n = 6 per arm, the same witnesses.
    That makes 96 rows plus the smoke, about 40 min.
  - Clause `SHARE-PRESERVED` at that cell: the WIRE median slow-leg share
    lies within [½, 2] × MAIN's median, or within ±0.03 of it.
  - A miss is a `REGRESSES-AT(c8-192)` finding.
  - Prediction: inside the band. Three of the four ledger rows at 128–192
    had fresh prices, where the arm equals the shipped law, and the slow
    leg's own queue term closes most of the open-loop shift.
  - §21's goodput and CPU clauses are scored there as at the other cells.
- *The verdict ladder keys on the stratified c1d comparison.*
  - As written, the pooled c1d-1536 clause decided `CURED` against
    `CURED-AT-COST`. If MAIN locks only 2–3 of 6 rows, the pooled median sits
    at the two-leg level and would read no cost.
  - Outcome 5 now reads: `CURED-AT-COST(<x %>)` iff §21's goodput rule,
    applied to WIRE rows against MAIN's one-leg rows at `c1d-400`
    `RWM_STORE=1536`, reads WORSE or TREND-WORSE. x is that median
    difference.
  - This needs ≥ 2 MAIN one-leg rows there, otherwise
    `NOT-REPRODUCED-AT(c1d-1536)`. The pooled clause is reported beside it.
  - Outcome 6 (`CURED`) is the same comparison reading WITHIN or better.
  - Outcome 4 (`NOT-CURED`) now fires on lock-outs only: any WIRE row
    LOCKED at a reproduced static cell. The c8 goodput gain is reported
    separately and stratified the same way: WIRE against MAIN's LOCKED rows,
    and WIRE against MAIN's fresh rows.
  - A pooled c8 static goodput WORSE stays a `REGRESSES` finding. The
    clause "not at least TREND-BETTER ⇒ `NOT-CURED`" is withdrawn.
- *The confound behind "absent at the default law", resolved on the
  ledgers.*
  - Every LOCKED row in the p27/p34 tables comes from a static cap on the
    `[LOOPDIAG]` binary, and the clean rows above came from main's binary
    at the default law. Both read `RWM_STORE_BOOT=128` on `[GATES]`. So
    "boots at 128 and grows" does not explain the absence; it was an
    inference and is withdrawn.
  - A scan of every other committed c1d and c8 sender log (anchor-diag,
    p29, store-law-n1, thread-pd, thread-r1, thread-r2, thread-r3-diag,
    thread-sk; the classifier above) separates the two. The instrument is
    not the cause: the same p29 `[LOOPDIAG]` binary locked 0 of 12 rows at
    the default law, and 2 of 3 at c8 `RWM_STORE=384` (c1d
    `RWM_STORE=768` 0 of 3).
  - The default law is not immune. Main's binaries show 1 LOCKED row in 149
    two-leg default-law rows: anchor-diag, c8, `PRS` arm, s7, where leg 1
    froze at 137 ms against `wrtt` 44 from t = 0.5 s.
  - So the static cap raises the rate, and the default cells' expected
    count is about 0 to 1 in 6, not exactly 0. Outcome 3's default cells
    are therefore scored on §21's clauses and the share clause, not on a
    lock-out count.
  - §36's D8, registered on main's binary at static caps (c1d {128, 512,
    2048}, c8 {16 … 1024}), answers reproduction on main's binary first. If
    §36 runs first and D8 shows no frozen idle leg at c8 ≥ 256 or
    c1d ≥ 512, then §38 is re-registered before its session, not run as
    written.
- *Stated for the reader.*
  - Paper §5.7's fixed points are gaps at I_A = 0, not closed-loop shares:
    per-leg W_i is not in the ledgers.
  - Test 6 now pins I_0 = 0, ε̂_0 = 0.026 and ε̂_1 = 0.017, at which the arm's
    gap is +3.8 nats and P(leg 1) = σ(−3.8) = 0.022 ≥ e^−4.

### 38. Amendment 2 — ref's clock, the start-up trigger, the cells and the ladder (before VM contact)

Two checks were asked for before the build: whether ref's clock explains the
large open-loop shifts on two "fresh" c8 rows, and whether the lock-out is
an artefact of the static arm's start-up. Both were settled offline on the
p27, p29, p34, disp36, anchor-diag and thread-pd ledgers, with
`tools/l1/place38_onset.py` (new) and paper §5.7's per-line δg. No engine
code, no VM.

**Check 1, units: ref's clock does not explain it, and the formula is
unchanged.**
- Every cost term is in seconds, and ref divides every term of every leg
  by the same number. So ref sets only the temperature in seconds, T·ref,
  and a different clock for ref rescales every gap without shifting any.
- Measured: ref = max(min_j ŵ_j, 1 ms) moves the 11 fresh c8 rows' per-row
  median shifts by ≤ 0.4 nats, because the fast leg sets ref and has
  s ≈ ŵ. On the locked rows it widens the arm's gaps to −10.4…+0.2 (c1d)
  and −0.9…+4.0 (c8). Every lock-out is still cured.
- What it would change is T wherever s ≫ ŵ on every leg: at c1d default,
  ×7–15 sharper. That is a second mechanism and belongs to T's derived
  form, in which ref cancels, so the arm keeps ref.
- The two "jumps" came from the method, not the law.
  - Table 5.7a's fresh column took δg from each row's medians of s and ŵ.
    δg does not depend on load, so it is now evaluated per in-transfer
    `[DIAG]` line. Per-row medians: −0.2 to −0.5 nats at caps 128–512, −0.9
    at 768, −2.5 / −1.3 at 1 024, +1.0 / +0.1 at 2 048.
  - DEF192 s42 is a role-swap row. Its fast leg froze at 416 ms against
    `wrtt` 23 from 1.0 to 2.5 s (a transient lock-out the share classifier
    misses) while the slow leg carried everything; then the roles swapped.
    Per line its shift is −0.43, not −2.6, so the 5.7 % → 45 % figure is
    withdrawn.
  - The 1 024 rows oscillate within each window. In idle phases the slow
    leg's echo srtt holds up to 130 ms of stale local dwell above ŵ, which
    the arm reads lower by design. Those rows sit at capacity share
    (0.156–0.160 against 0.167), the c8 default cell's regime.
- Paper §5.7 (Table 5.7a, the c8 fixed point and a units paragraph) is
  corrected.

**Check 2, start-up: the trigger is the static arm's start-up; the frozen
price is the law's.**
- *Onset.* All 28 LOCKED rows froze at t ≤ 1.0 s: ≤ 0.5 s in 21, 0.8 s in
  6, 1.0 s in 1. Two recovered late in the row. No lock-out starts later.
- *Rates.*
  - Static arm: 27 of 110 rows LOCKED (c1d 10 of 59, c8 17 of 51).
  - Default law: 1 of 161 (anchor-diag c8 `PRS` s7, onset 0.8 s).
  - disp36 is main's binary and locked 5 of its static rows, so the
    instrument is not the cause.
- *By cap, a band.* c8: 0/6 at 16–64, 1/6 at 128, 2/4 at 192, 5/6 at 256,
  6/9 at 384, 2/6 at 512, 1/4 at 768, 0/10 at 1 024–2 048. c1d: 0/9 at
  128–384, 2–3 per 7–10 at 512–2 048, 0/8 at 3 072–4 096. Above the band
  the active leg's own queue lifts its price past the frozen one.
- *What `RWM_STORE` switches.* It sets `store_env_set`, which turns
  `plain_dyn_cap` off. That removes, together:
  - the boot cap (`RWM_STORE_BOOT=128` is echoed on `[GATES]` but not read);
  - the gain·BDP law and the path pool;
  - `RWM_DELTA_CAP` and `RWM_SUM_CAP`;
  - the honest cap and the per-path `sout` gauge.

  The window is n from the first symbol.
- *Mechanism.* Before any echo both legs carry the 50 ms seed, so the first
  burst splits evenly, whatever the capacities.
  - At c8 the 20 Mbit/s leg takes about half of n (DEF256 s42: 96 of 212
    in flight at 0.3 s), and its echoes read 156–390 ms against `wrtt`
    42–46. Its srtt jumps, placement leaves it, and its price freezes.
  - At c1d one leg's early echoes read 54–369 ms against `wrtt` 2–7. Where
    that dwell sits (sender backlog or receiver) is not resolved offline.
  - The default law's first burst is the whole pool at 128, the band's
    lower edge, hence 1 in 161.
- *The answer.* The lock-out is specific to a large first window, which the
  static arm provides and the default law provides only rarely. The default
  law is not immune, because the absorbing frozen price is the law's own.
  The arm also shrinks the trigger: quinn's RTT exists from the handshake,
  so at c8 the slow leg's cold price sits ½·(43 − 14) ms above the fast
  leg's, not level with it. That is expected, and measured below.

**What changes in the registration (amendment 1 as amended).**
- *Roles of the cells.*
  - The default-law cells (`c1d-400`, `c8-100`) and the controls (c1s, c2)
    now carry the shipping question: does the arm cost anything where the
    machine ships?
  - The static cells (`c1d` 1536, `c8` 192 / 256 / 512) carry the
    mechanism question: does the arm remove the absorbing state where the
    trigger fires?
  - The benefit at the default law (about 1 row in 161) cannot be measured
    at n = 6. That is stated, not hidden; the static cells stand in for it.
- *Outcomes 5 and 6 move to the default cell.*
  - `CURED-AT-COST(<x %>)` iff §21's goodput rule, WIRE against MAIN at
    `c1d-400` default, reads WORSE or TREND-WORSE. x is the median
    difference. MAIN there is two-leg (0 of 81 default c1d rows LOCKED on
    the ledgers), so x is the arm's own cost, not a lost accidental gain.
  - `CURED` iff that clause reads WITHIN or better.
  - Both still require outcomes 2–4 to pass on the static cells.
- *The static c1d cost is reported, not decided on.* Amendment 1's
  stratified c1d-1536 comparison (WIRE against MAIN's one-leg rows)
  measures the lock-out's accidental one-leg gain (the receiver runs
  ×1.35 faster on one leg). It is reported with x beside it and decides
  nothing.
- *The c8-192 share clause compares fresh rows only.* 192 lies in the
  lock-out band (2 of 4 LOCKED), so MAIN's pooled share there includes
  locked rows near 0 %, which WIRE would correctly lift.
  - `SHARE-PRESERVED` now compares the WIRE median with the median of
    MAIN's non-LOCKED rows at that cell.
  - With fewer than 2 non-LOCKED MAIN rows there, the clause is reported,
    not gated.
  - The same applies to `c8-100` default's ±0.03 clause (expected: all rows
    non-LOCKED).
- *Readout added (reported).* At the static cells, per arm: the slow
  (c8) or second (c1d) leg's in-flight share and `rtt`/`wrtt` on the first
  `[DIAG]` line (t ≈ 0.3 s). This is the trigger. Predicted: WIRE puts
  less of the first burst on c8's slow leg than MAIN's even split.
- *Predictions revised.*
  - MAIN at c8-192: 2–4 of 6 LOCKED.
  - WIRE at c8-192: share within [½, 2] × MAIN's non-LOCKED median.
  - `c1d-400` default, WIRE against MAIN: WITHIN, so the expected verdict
    is `CURED`, not `CURED-AT-COST`.
  - The static c1d-1536 pooled cost stays as predicted (−5 to −12 %,
    reported).
- Nothing else changes: cells, n, seeds, env, witnesses and abort table
  stand as amended.

### 38. Amendment 3 — the build, and two spec corrections (before VM contact)

*Build.* `RWM_PLACE_WIRE` is implemented (`eb1d66e`, tests `ac6110b`,
merged on `diag36`).
- With the gate off it is bit-identical to main, which
  `every_arm_absent_leaves_the_probabilities_bit_identical` pins, also with
  a mirrored wire RTT present.
- Suites on the VM: release 1 162/0 (52 ignored), debug 1 025/0 (51
  ignored), doc, wasm 35 (golden fingerprints unmoved), and the `tools/l1`
  python tests.
- Red-first evidence is in the merge's branch report: each behavioural test
  failed against an inert stub. The invariant tests hold on both sides by
  construction.

*Corrections.*
- **Amendment 1's test-6 arithmetic.** For the DEF256 state (s 24/390, ŵ
  23/45, ε̂ 0.026/0.017, I = 0), the gap is +3.04 nats, so P₁ = σ(−3.04) =
  0.046, not +3.8 / 0.022. The difference is Bulk's w_bw·(r₁ − r₀) term.
  The test asserts the formula value. The cure claim (P ≥ e⁻⁴ against
  P_off < 10⁻²⁰) stands.
- **A test-only hook.** `RWM_TEST_PLACE_HOLD=<leg>:<ms>` withholds one leg
  for a set time from the start, for the loopback test. It is unset in every
  shipped configuration and in every battery row; the battery withholds it.
- **Fallback.** `min_j wire_j` ranges over active candidate legs.
- **Gauges.** `w_cold` counts per leg evaluation; `w_mv` and `ref_fl` count
  per `place_costs` call. The denominators print as `w_n=` and `w_calls=`.

### 38. Amendment 4 — the ladder's overlap at c1d default, and the harness's readings (before VM contact)

- *The overlap.* Outcome 3 (`REGRESSES-AT`: a §21 clause WORSE at a control
  or default cell) and Amendment 2's outcome 5 (`CURED-AT-COST`: §21's
  goodput rule at c1d default reads WORSE or TREND-WORSE) both fire on a
  WORSE goodput at c1d default. Amendment 2 is the later and more specific
  clause, and it names that goodput as the arm's own cost. So **§21's
  goodput clause at c1d default is exempt from outcome 3 and decides only
  5 against 6.** A WORSE there gives `CURED-AT-COST(x %)` with x stated. The
  ship decision is the user's either way. Every other §21 clause at c1d
  default, and every clause at the other cells, stays in outcome 3.
- *Readings fixed (from the harness, `place38`, `87b0f31`).*
  1. `[LAG]` is scored on E = p99 − min_us, as §28/§32; raw p99 is printed
     beside it.
  2. The store witnesses (`[SUMCAP] eng` 0 and `win=` denominator = n) apply
     at the static cells only. The default law engages the store law by
     design.
  3. The MAIN witness is `wpl_us=-`, `ref_fl`/`w_cold`/`w_mv` = `-` and
     `w_n = w_calls = 0`. WIRE needs `w_calls > 0`.
  4. Cell order within a block follows the table, with c8-192 before 256
     and 512. Blocks are rep-major over seeds 42 and 7 (3 reps), and arms
     rotate by block (MAIN first on even blocks): 96 rows plus a 2-row
     smoke (c8-100 `RWM_STORE=256`, MAIN and WIRE).
  5. A static cell is reproduced iff MAIN has ≥ 2 LOCKED rows there. At a
     non-reproduced cell the share and goodput clauses are reported, not
     scored as regressions.
  6. At c1s and c2 (N = 1), WIRE rows need `w_calls > 0`, and a non-zero
     `w_mv` is a `REGRESSES-AT` finding. One unscored check row (c2 WIRE)
     confirms the N = 1 print before launch. If it prints `-`, the session
     stops for an amendment.

### 38. Result

Scored 2026-11-10 against the pre-registration and amendments 1–4, literally
(`docs/l1-raw/place38/score.txt`). **`REGRESSES-AT(c1s-400@def)`**, with
c8 192 and 512 not reproduced. The verdict rests on one clause: WIRE's
client CPU per GB at the c1s control is +2.5 %, against a tolerance of
2.4 %, with disjoint ranges (WIRE 16.02–17.62, MAIN 15.58–16.00).
Every other clause of the ladder passes. Without that clause the ladder
reads `CURED`. The arm does not ship.

*Session.* An unscored N = 1 check row (c2 WIRE) printed `w_calls=87 103`
and `w_mv=0`. First launch: `ABORT-PRECONDITION` (`procs_running`
[1,1,1,12,12]; nothing ran). Relaunch 11:10:21Z: both locks, gate, build
(binary `2b960cbd…22ab`, tree `f50cb6d` = `diag36` + the harness), smoke ok,
battery 11:16:17–11:31:41Z, `DONE-ALL`. 96/96 rows LIVE, rc 0, no io flag.
No other abort.

*Readings (WIRE against MAIN, medians, n = 6 per arm).*

| cell | LOCKED MAIN / WIRE | goodput | other |
|---|---|---|---|
| c1s default | 0 / 0 | 788.9 vs 803.1 (−1.8 %, WITHIN; WIRE range 574.7–806.2) | client CPU/GB **+2.5 % WORSE**; server WITHIN; `w_mv` = 0 |
| c2 default | 0 / 0 | 89.75 vs 89.98, WITHIN | all clauses WITHIN |
| c1d default | 0 / 0 | 780.9 vs 777.2 (+0.5 %, WITHIN) | share 0.481 vs 0.483 |
| c8 default | 0 / 0 | 96.9 vs 100.4 (−3.4 %, WITHIN) | slow-leg share 0.183 vs 0.185, preserved |
| c1d 1536 | 5 / 0 | 771.4 vs 816.3 (−5.5 %, WITHIN) | CPU/GB client +38.9 %, server +23.4 % (static cell, reported; two legs cost more than one) |
| c8 192 | 1 / 0 (not reproduced) | 86.55 vs 87.95 | share 0.038 vs 0.032, preserved |
| c8 256 | 3 / 0 | 95.1 vs 90.4 (+5.2 %, TREND-BETTER) | — |
| c8 512 | 1 / 0 (not reproduced) | 103.6 vs 102.2 | — |

No WIRE row locked anywhere. `wpl_us` sits within 1 ms of `wrtt` on every
WIRE row.

*The c1s clause.* At N = 1 the arm moves no placement (`w_mv` = 0 exactly).
What it adds there is the mirror (`control_msg.rs`). At every ack batch and
path report it:
- builds two `Vec`s and sorts the leg ids;
- does a map lookup and an `Arc<PathView>` clone per leg;
- copies the RTT into the scheduler.

The snapshot it reads is refreshed at most once per `VIEW_REFRESH` = 1 ms
(`io_owner.rs`). Reading it more often than that buys nothing. A
+2.5 % client CPU/GB at c1s, the highest-rate cell, fits that per-batch
overhead. This is the expected cause, not a measured attribution. Two
WIRE seed-7 rows at 574.7 and 683.5 Mbit/s widen its goodput range. c1s
has shown such rows before (§34's 277).

*Predictions.*
- `CURED`: MISSED, by the CPU clause.
- No lock-outs under WIRE: MET.
- c8 256 goodput rising from 88–90 to 95–103: MET (95.1).
- Default cells and controls within their bands: MET except c1s client CPU.
- The pooled c1d 1536 cost of −5 to −12 %: MET (−5.5 %).

### 38. Result — addendum (from the ledger, no VM)

- **Attribution: the per-placement cost, not the mirror.** At c1s, WIRE
  rows ran 338 822–338 964 `place_costs` calls (`w_calls`), against
  11 245–13 069 ack batches (`[IOWN] ack_batches`), ×29 more. Each armed
  call fills two extra `Vec`s (`bases`, `shipped_col`), runs two argmin
  scans and updates the gauge cell. The mirror runs per ack batch and is the
  smaller term. The result's cause paragraph is corrected to this.
- **Size at matched speed.**
  - MAIN's CPU/GB is flat in goodput (15.58–16.00 over 737–822 Mbit/s).
  - WIRE's four rows at 788–806 Mbit/s read 16.02–16.25 (mean 16.10)
    against MAIN's 15.81: +1.9 %, still disjoint.
  - The two slow WIRE rows (574.7, 683.5) read 16.65 and 17.62. They
    inflate the registered +2.5 %, but the excess does not depend on them.
  - +1.9 % of ≈ 6.4 s client CPU over 3.4·10⁵ calls is ≈ 360 ns per armed
    call: allocation-sized, an implementation overhead and not the law.
  - Post hoc; the registered verdict stands.
- **First-burst split (registered readout, omitted from the result).**
  - c1d 1536: the second leg's in-flight share at t ≈ 0.3 s is MAIN 0.612
    [0.24–1.00], WIRE 0.628 [0.53–0.69].
  - c8 192: the slow leg's share is MAIN 0.009, WIRE 0.085, with rtt/wrtt
    1.18 against 1.06.
  - The claim that the arm shrinks the start-up trigger is **not shown**.
    What the arm shows is that no leg stays locked.
- **What §38 says about shipping.** At the default law neither arm locked
  (0/6 at c1d and c8), so the arm has no measured benefit where it would
  ship, as Amendment 2 predicted. Default-law goodput point estimates are
  −1.8, −0.3, +0.5 and −3.4 %, all WITHIN. The measured benefit is at
  static caps: c1d 1536 went from 5/6 locked to 0/6, and c8 256 from 3/6
  locked to 0/6 with +5.2 % goodput. So the arm's case is robustness, and
  static diagnostic grids that are not voided by lock-outs (§36/§40's idle
  slow-leg rows), not a default-law gain.
- **Not a fix:** skipping the arm at one leg would be the path-count branch
  ADR-0072 removed.
