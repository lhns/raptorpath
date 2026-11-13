## 41b. `RWM_PLACE_WIRE` gate-off against main — an A/A-calibrated guard — pre-registration

§41's BASE gate failed on one clause on the better side (c1d `lag_E_cli`
−28.9 % against 28.5 %), and that gate had never been sized against two
identical binaries. The user chose a calibrated re-test over archiving.
This section also fixes the guard rule that §45 uses for every
simplification stage.

**Question.** Is the `diag36` binary with the placement arm off
(`RWM_PLACE_WIRE` unset) worse than main's binary on any §21 clause, beyond
what two runs of main's own binary differ by?

**Trees.**
- BASE: main `4926822`.
- MAIN and WIRE: `diag36` at `85be32a` (engine = main + the placement arm
  and its §41 witness change), gate off for MAIN and on for WIRE.
- Each is built fresh in its own `git archive` tree with its own target
  dir. No suites: the `diag36` tree passed them in §41's session.

**Design.**
- Cells c1s-400 and c1d-400, default store law, Bulk, `--window-reliable`,
  env as §38.
- BASE: n = 12 per cell (seeds 42 and 7 × 6 reps). MAIN: n = 6 per cell
  (seeds 42 and 7 × 3 reps). WIRE: n = 6 per cell, reported only.
- 48 rows plus a 2-row smoke (one per binary, c1s). Arms interleave per
  block (BASE, MAIN, BASE, WIRE patterns rotated so each arm meets each
  block position), fixed by the harness and pinned by a test.
- Harness: plan 41's (`place38_parse.py`, plan 41 in `loopdiag_*`) with
  BASE at n = 12 and MAIN and WIRE at n = 6. §41's rules carry over:
  precondition, both locks (`test -e`), root-owned tmpfs, `ABORT-BIN`, and
  `LD_HARD` = launch + 4 h.

**Why a raw max fails (dry-run on §41's BASE rows, before writing this).**
Over all 3/3 splits of §41's six BASE rows per cell, the clause attaining
the max of raw relative differences was a lag clause in 20 of 20 splits at
both cells (`srv_lag_E` 14, `cli_lag_E` 6). A raw max would therefore be
blind to CPU and goodput. Each clause is scaled by its own null spread.

**The statistic (fixed now; one-sided, one number over both cells).**
- **Rows and clauses.** Per cell, the 18 rows are pooled (12 BASE + 6
  MAIN). The clauses are §21's as scored in §41: goodput, client and server
  CPU/GB, `lag_E_cli`, `lag_E_srv`, and `rtp_floor` per leg. Each has a
  worse direction: lower goodput; higher CPU, lag and rtp floor.
- **One relabelling** picks 6 rows per cell as "MAIN", 3 from each seed
  (matching the true MAIN's seed balance). The rest are the reference.
- **Clause value.** For relabelling π and clause c,
  x_c(π) = worse-signed (med(sel) − med(ref)) / med(ref).
- **Scaling.** z_c(π) = x_c(π) / sd_c, where sd_c is the sd of x_c over
  10 000 relabellings (RNG seed 4141).
- **Statistic.** T(π) = max of z_c(π) over all clauses of both cells.
- **p-value.** p = the share of the 10 000 relabellings with T(π) ≥
  T(true), where "true" is the real MAIN rows.
- **Gate PASS iff p > 0.05.**
- **Resolution table, computed from BASE rows alone before MAIN is read:**
  per clause, the A/A q95 of |x_c| over 3+3-vs-6 splits of BASE's 12 rows,
  beside its §21 tolerance. It states what the guard cannot see. This is
  the calibration the user asked for.
- **Reported beside the gate:** every clause's §21 reading of MAIN against
  all 12 BASE rows, z_c(true), and the run-order values.

**Also run, reported only: WIRE.** `RWM_PLACE_WIRE=1` on the `diag36`
binary, 6 rows per cell (3 per seed), scored by the same statistic against
BASE in its own pooled relabelling. That is 12 more rows. It gives the
promote-or-archive call direct evidence.

**Decision rule.**
- **PASS:** the gate-off binary may reach main. Under §45's rule ("an arm
  is promoted or archived at its verdict"), the user then makes the
  promote-or-archive call on the arm. The WIRE readout and §38/§41 are the
  evidence.
- **FAIL:** tag `archive/place-wire`, revert the arm's engine merges from
  `diag36`, and keep the docs, harness and ledgers.
- **Guard reuse:** this statistic, with BASE at n = 12 per cell, is §45's
  guard from here on.

**Pinning.** MAIN and WIRE are `diag36` at `85be32a`. `raptorpath/src` is
unchanged from §41's tested tree `40c55dd`; that is checked before launch,
which is why no suites run.

**Predictions.** In §41, gate-off against BASE read +0.6 % client CPU and
+1.2 % goodput at c1s, and only the c1d lag clause failed (better side).
So p > 0.05: PASS. WIRE reads about +1–2 % client CPU at c1s: likely
inside the A/A resolution, reported.

**Duration.** Two builds (≈ 11 min) and 50 invocations (≈ 11 min).

### 41b. Result

Scored 2026-11-10 (`docs/l1-raw/place41b/score.txt`). **GATE PASS,
p = 0.7195.** The gate-off `diag36` binary may reach main.

*Session.* 15:04–15:22Z. Precondition met first try. Parser gate passed.
Two builds (331 s and 328 s). Smoke passed. Battery took 362 s: 48/48 rows
LIVE, rc 0, no witness failure, no abort.
- Pre-launch: `raptorpath/src` equals `85be32a` and `40c55dd` (sha256 of 92
  files), and the binary sha equals §41's.
- The scorer dry-run on §41's rows confirmed the scaling fix. After
  scaling, the maxima spread over all 13 clauses (8–11 % each), where the
  raw maximum had been a lag clause every time.
- Readings fixed by the harness: sample sd (n − 1); q95 = rank
  ⌈0.95·n⌉; resolution splits are half of each seed's BASE rows.

*Resolution table (from BASE alone; this is the guard's calibration).*

| clause | c1s q95 / §21 tol | c1d q95 / §21 tol |
|---|---|---|
| goodput | 3.8 % / 4.9 % | **6.4 % / 5.6 %** |
| client CPU/GB | 1.2 % / 2.4 % | 3.1 % / 6.5 % |
| server CPU/GB | 1.5 % / 2.4 % | 2.8 % / 6.5 % |
| `lag_E_cli` / `lag_E_srv` | 31 % / 20 % | 53 % / 53 % |
| `rtp_floor` | 1.6 % | 2.3–3.1 % |

- The guard resolves about 1.2 % CPU and 3.8 % goodput at c1s.
- At c1d it cannot see a goodput loss under 6.4 %.
- The lag clauses carry no information at n = 12.

*MAIN against BASE.*
- T(true) = +1.256, at c1s `lag_E_srv` (x = +16.4 %); every other z is
  below +0.7. p = 0.7195.
- Every §21 reading against all 12 BASE rows is WITHIN, and the feed reads
  SAME.

*WIRE (reported only).*
- p = 0.9316. Every §21 reading is WITHIN.
- c1s client CPU/GB +0.0 %, c1d +0.2 %. Across sessions, WIRE's c1s CPU
  cost reads +2.5 % (§38), +0.8 % (§41) and 0.0 % (§41b): below
  resolution, not zero.

*Anomaly: MAIN c1s s7 r3 ran at 68.3 Mbit/s.*
- From its kept client log, the store is full at the knee (`win=2048/2048`,
  pin 0.987, floor 0). The sender is paused 59–85 % of the time.
- The app-echo RTT reads 242–466 ms while quinn's wire RTT reads 2 ms.
  Quinn's backlog is ≈ 2 000 datagrams; the receiver is busy 36–75 %.
- So it is not the law contracting to its floor. The echo and SACK release
  path stalls, with GSO 7.6 against a typical ≈ 2.7.
- This is an engine fault on the gate-off path, i.e. main's code path; the
  arm is off. Slow c1s rows: 3 of 36 on the `diag36` binary against 0 of 18
  on main's, too few to blame the binary.
- It is the likely answer to the earlier unexplained slow c1s rows (§34's
  277, §38's 575 and 684). It is now an open diagnosis item.
- The guard works on medians, so it is blind to a rare collapse. From now
  on it also reports a tail line: the lowest row per arm against the cell
  median.

*Next.* §41's ladder (`CURED`) together with this PASS puts the arm to the
user's promote-or-archive call (ADR-0073). Promoting deletes the old base
and the gate in the same merge, as a new engine tree, through suites and
the guard. It also removes the test hold hook. `XPATH_REPAIR` waits for
that call.

### 43. Note before the score is read (2026-11-10, while the battery is running)

A row that runs slowly because of the engine stall above would hit
`RWM_PERF_TIMEOUT_S=200` at 5 GB as DNF. Under amendment 2, COLLAPSE is
"any DNF row", so an engine fault could label the law UNSTABLE.
Fixed now, before any score:
- every DNF row is also classified from its n series;
- a DNF row whose n stayed at or above its last-third median until the
  stall, with the sender paused and the echo RTT far above the wire RTT, is
  reported as `ENGINE-STALL` beside the label.

The registered label is still computed and reported literally.

### 41b. User ruling — promote `RWM_PLACE_WIRE` (2026-11-10, ADR-0073)

The wire-clocked placement base becomes the shipped placement price.
- **In the same change**, deleted: the old frozen-srtt base path, the
  `RWM_PLACE_WIRE` gate (with its `RWM_FORWARD` and `[GATES]` entries), and
  the test-only `RWM_TEST_PLACE_HOLD` hook. Its loopback test is rewritten
  on `#[cfg(test)]` seams, or dropped if that is not possible.
- **Kept:** the `[ETA]` stamp stays on the shipped E as before. The
  `w_mv` / `ref_fl` / `w_cold` gauges stay; they become the price's bind
  gauges.
- **Paper §5.7:** the wire base is the shipped law, and §10 gets the old
  base's row (re-ask clause: "if quinn's SRTT is unavailable on a
  substrate").
- **Next:** this new engine tree goes through suites and the guard in the
  merge-candidate session, together with simplification batch 1. Then
  `XPATH_REPAIR` (group A) is deleted on the promoted `place.rs`.
- **Evidence:** §38, §41 and §41b. Not tested off Bulk or at four paths;
  that is stated in §5.7.

### 41b. Promotion — built and merged on `diag36` (`place-promote`, 4265e3d + c1000c3 + 0ae5a3b)

The wire base is the only placement base. Removed:
- the old base;
- the `RWM_PLACE_WIRE` gate (`RWM_FORWARD` 97 → 96, `[GATES]` 84 → 83);
- the `RWM_TEST_PLACE_HOLD` hook.

Red → green on 13 tests. Targeted debug tests 121/0, plus the
`place_wire_loopback`, `place_arms_reachability` and `eta_reachability`
integration tests. The `place_costs` allocation test still makes 0 extra
heap blocks.

**Two deliberate departures from the ruling text above:**
- **The `[ETA]` stamp is now the priced E.** Stamp, price and X_i's
  arrival read one seat (`Scheduler::place_e`), so they cannot drift. As a
  consequence, `[ETA]` residuals and `lag_E` re-baseline, and σ̂_e (read by
  the default-off `RWM_PLACE_T_DERIVED`) measures the wire-base predictor.
  - The stamp adds one min-wire scan per stamped symbol on the sender path.
    That CPU cost is checked by the guard.
- **`w_mv` is removed:** it compared against the deleted base. `w_cold`
  and `ref_fl` stay as the price's bind gauges.

**Harness debt before the merge-candidate session.** `place38_parse.py`'s
`_ETA` regex requires `w_mv=`. `place38_parse.py`, `loopdiag_battery.sh`
and `place41b_score.py` expect `RWM_PLACE_WIRE` in the binary or the
`[GATES]` echo. The MAIN-against-WIRE battery is retired. The §41b guard
and the §46 CAND preflight must accept the promoted tree before VM
contact.
