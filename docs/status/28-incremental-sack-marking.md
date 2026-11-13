## 28. Incremental SACK marking — ship battery (with a step-0 check of the per-wake bound) — pre-registration

§26 finding 1: each `SackReport` re-sends every range above the stalled
frontier, and the sender's drain re-marks every retained seq in every range
(`sack_release_mark_into`: one `BTreeSet::insert` per seq). The cost is
O(span above the frontier) per report. Measured: 23 % of the c8 client
sender's wall, up to 20 ms per drain, and 81 % of the wall of its polls
≥ 10 ms. Committed before any VM contact. **Step 1 is a ship battery and is
stopped at the gate until the operator says go.** Step 0 is a diagnostic
and ships nothing.

**The change (`SK`, branch `fix/sack-incremental` from main 92934a7).**
- *Incremental marking.* A coalesced interval set holds the ranges already
  processed above the cumulative frontier. Each report visits only the
  uncovered parts of its ranges. Work per report is O(ranges · log + newly
  marked), not O(span).
- *What stays exact.* The `released` set, the store gate's count, the
  per-path releases, `above_report` and the wire-v9 semantics are all
  unchanged. A differential test against the current marking checks this
  over scripted and seeded-random report sequences: the same `released`
  set, the same newly-marked seqs in the same order, the same totals.
- *Cost bound test.* A re-sent report visits no seqs, and a report that adds
  k seqs visits O(k).
- *Constants and modes.* No constant, no mode.
- *Witness.* `[SACKI] reports ranges visited newly` under `RWM_DIAG`.

**Step 0 — does the per-wake bound still matter? (diagnostic, before the
battery, same session.)**
- *Rows.* SK binary, `c8-100`, 6 rows at `RWM_RTOBS=2` (seeds 42 and 7 × 3),
  same env and run-from-tmpfs as §26.
- *Rule (operator's).* The plan-rule-3 per-wake bound (one Law-0 burst of
  work per wake, then one natural yield, no new constant) is **kept** iff,
  under SK, the client sender still logs polls ≥ 10 ms: median count per
  row ≥ 1 in `[LPOLL]`. Otherwise it is dropped.
- *If kept.* The bound is implemented red-first on top of SK. NEW =
  SK + bound, and step 1's arms and harness are amended before go.
- *If dropped.* NEW = SK.

**Step 1 — ship battery (stopped at the gate).** `MAIN` = main 92934a7 vs
`NEW`.
- *Cells and n.* `c1s-400`, `c1d-400`, `c2-100`, `c8-100`; n = 6 per arm
  and cell (3 reps × seeds 42 and 7); arm order rotated per block.
- *Env.* §21's (`RWM_RTOBS=1 RWM_RDIAG=1 RWM_GEN=0 RWM_DIAG=1`, the
  standard `env -u` list).
- *Harness.* `tools/l1/threadsk_run_all.sh`, `threadsk_battery.sh`,
  `threadsk_parse.py` (tested), derived from §21's.
  - Full suites run on the NEW tree: release, doc, wasm, debug, parsers.
  - Precondition per the agent rules: both locks free, no
    `cargo`/`rustc`, and `procs_running` ≤ 2 in 5 samples 2 s apart.
  - Measurement from `/dev/shm/tsk` with an `ABORT-ONDISK` guard.
  - io `full avg10` is recorded before and after every invocation as a
    covariate.
- *Binary witness.* `[SACKI]` must be in NEW's binary and absent from
  MAIN's (two-sided, checked at launch). Every NEW client row must carry
  `[SACKI]` with reports > 0.
- *Pass rule per cell.* §21's clauses and min–max rule, with the `[LAG]`
  clauses (client and server) on **E = p99 − `min_us`**. At `c8-100` only,
  the extra clause `lag_E_spread_cli`: NEW's client-E max/min against
  MAIN's; WORSE iff > ×1.5 MAIN's, BETTER iff < MAIN's / 1.5.
- *Decision rule.* **`SHIP`** iff NEW is PASS-EVERYWHERE;
  **`REFUTED-WITH-RECORD`** if it fails anywhere; **`UNSCOREABLE-AT(…)`**
  as §21.
- *Predictions (reported, not gating).*
  - c8 client CPU per GB: lower (the drain was 23 % of sender wall), and
    TREND-BETTER at least.
  - c8 client E: lower.
  - c1s, c1d, c2: WITHIN. A single path has fewer ranges above the
    frontier.
  - `[SACKI]` visited/report on NEW is far below the c8 span; no number is
    predicted.
- *Abort causes.* §21's table plus `ABORT-ONDISK`. The budget is launch +
  4 h.

### 28. Step 0 — result (diagnostic; ships nothing)

Read 2026-10-27 against step 0's rule. **The per-wake bound is DROPPED:
NEW = SK.** Under SK the client sender logged **no** poll ≥ 10 ms in any of
the 6 c8 rows; the rule kept the bound only for a median ≥ 1 per row.

*Session.* SK binary `c3312f5a…f99d2c` (`12ec8d5`). Built fresh on disk and
run from `/dev/shm/sk0` via `lagdiag2_run.sh` (`LG2_BLOCKS=3 LG2_RT1=0`).
The detached waiter retried from 10:12Z behind §27's locked suite and ran at
10:53–10:59Z.
- In-session tests: 53 passed.
- Smoke passed. 6 of 6 rows live, with io `full avg10` ≤ 3.
- Exit state: 0 `raptorpath`, 0 `rp-*`. The VM dirs were removed.
- Ledgers: `docs/l1-raw/sk-step0/`.

*Readings* (client, 6 rows; the §26 numbers for contrast are cross-session,
report-only):

| quantity | §26 (MAIN, 24 rows) | SK, step 0 (6 rows) |
|---|---|---|
| sender polls ≥ 1 ms per row | ≈ 900 | ≈ 73 (60–135) |
| sender polls ≥ 10 ms, total | 195 | **0** |
| `[TXSEC]` `sack` share of sender wall / max | 0.23 / 19.9 ms | 0.018 / 2.3 ms |
| `[SACKI]` visited / newly | — | equal on every line (zero redundant visits) |
| client E_lag | 1370–4113 µs (median ≈ 3200) | 1096–1693 µs (median ≈ 1460) |

The digest's own `OUTCOME-B` line reads `UNBOUNDED-PER-WAKE(emit)`. That is
§26's rule applied to the polls ≥ 1 ms (p90 of 5 iterations per poll, with
emission the largest share). It is not step 0's rule, which reads polls
≥ 10 ms and finds none. It is recorded as a reading. A sender poll is now
bounded at a few ms of real emission work, which is the Law-0 burst doing
its job. `OUTCOME-A` reads `UNSETTLED-INSTRUMENT` (6 < 18 rows), as expected
for this size.

**§28 step 1 — pre-launch note (operator GO 2026-10-27).** The step-0
caveat is closed. The randomized differential test (`differential_randomized`)
was re-run red on a stub of the subtle bug: a memo that records the
unclipped range, without the clip at `sent_store.last_seq()`. Result: FAILED
(the panic at `sack_memo_tests.rs:58`); the other four tests pass on that
stub. After reverting the stub, all five pass. The pre-registration is
otherwise unchanged.

### 28. Step 1 — ship battery — result

Scored 2026-10-27 against the pre-registration, literally (no amendment
beyond the pre-launch note). **`SHIP`: NEW (incremental SACK marking) is
PASS-EVERYWHERE.** At c8 it is BETTER in client CPU per GB (−30.1 %, ranges
disjoint) and BETTER in the client E-spread clause (max/min 1.64 vs 3.68).
It is WITHIN everywhere else, apart from a c8 goodput TREND-WORSE (−5.7 %,
ranges overlap) and a c2 RTprop TREND-BETTER. Shipping is the operator's
merge.

*Session.* The waiter retried twice behind §29 (`ABORT-PRECONDITION`), then
ran 11:41–13:06Z under both locks.
- Binaries: NEW `c3312f5a…f99d2c` (`634cb4d`, the same engine bytes as step
  0's) and MAIN `37dfd2f8…ffab361` (92934a7, the same bytes as §26's MAIN).
  Both were built fresh on disk and run from `/dev/shm/tsk`.
- Full suites on NEW: release 1127 passed / 0 failed (50 ignored); doc rc 0;
  wasm 35; debug 986 / 0 (49 ignored); parsers rc 0; identity panics 0.
  `TESTS-OK`.
- Smoke passed. Battery 12:59–13:06Z (436 s): 48 invocations, all LIVE,
  0 witness failures, no abort token, no truncation.
- io `full avg10` is ≤ 20 before and after every invocation, so no row is
  flagged. Exit state: 0 `raptorpath`, 0 `rp-*`.
- Ledgers: `docs/l1-raw/thread-sk/` (`score.txt` with every per-rep value,
  `tsk.log` with the `IOP` lines, `TESTS.txt`, `diag-tsk/`).

*Abort table.* None fired: `ABORT-LOCK`, `-CRLF`, `-BUILD`, `-TESTS`,
`-SHA`, `-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUDGET`, `-ONDISK` all read
*no*, and `ABORT-PRECONDITION` fired only on the two retries before launch.
`ABORT-RC` / `ABORT-BRINGUP` / `VOID-COTENANT`: 0 / 0 / 0 of 48.

*Per cell* (NEW vs MAIN, median [min–max], n = 6; E = `[LAG]` xfer p99 −
`min_us`, µs):

| cell | goodput Mbit/s | CPUCLI s/GB | CPUSRV s/GB | client E | server E | other | verdict |
|---|---|---|---|---|---|---|---|
| `c1s-400` | 738.2 vs 747.7 (−1.3 %, =) | 13.99 vs 13.70 (+2.1 %, =) | 19.60 vs 19.23 (=) | 2157 vs 2237 (=) | 1471 vs 1506 (=) | RTprop =, feed SAME | **PASS** |
| `c1d-400` | 660.8 vs 668.5 (−1.1 %, =) | 18.88 vs 17.99 (+4.9 %, = at 6.5 %) | 23.06 vs 23.27 (=) | 2631 vs 2256 (=) | 1822 vs 1598 (=) | RTprop = / =, feed SAME | **PASS** |
| `c2-100` | 89.60 vs 90.03 (=) | 30.75 vs 31.10 (=) | 37.2 vs 35.35 (+5.2 %, = at 6.3 %) | 1262 vs 1264 (=) | 1016 vs 1063 (=) | RTprop TREND-BETTER (−13.1 %) | **PASS** |
| `c8-100` | 95.71 [81.9–101.8] vs 101.5 [89.4–103.4] (−5.7 %, TREND-WORSE) | **34.55 [33.2–37.0] vs 49.45 [42.1–57.3] (−30.1 %, BETTER)** | 43.8 vs 43.85 (=) | 1106 [1069–1753] vs 2562 [1132–4168] (−56.8 %, = within the 59 % band) | 1118 vs 1080 (=) | **E spread 1.64 vs 3.68 (BETTER)**; RTprop =, feed SAME | **PASS** |

Arm verdict: `PASS-EVERYWHERE`, so the decision is **`SHIP`**.

*Predictions.*
- c8 client CPU/GB lower: **MET** (BETTER, −30 %; the client sender busy
  fell 31 → 11.5 %; client cores 0.615 → 0.405).
- c8 client E lower: **MET in direction** (−57 %), WITHIN by the min–max
  rule because MAIN's range is wide.
- c1s / c1d / c2 WITHIN: **MET**.
- `[SACKI]` visited per report: c8 19.2, c1s 61–77, c1d ≈ 1, c2 ≈ 14; equal
  to newly per report everywhere, so no redundant visits.

*What it does not show.*
- The c8 goodput TREND-WORSE (−5.7 %) is one low NEW row (81.9) against
  overlapping ranges. §15 recorded c8 goodput spreads of 83–103 on one
  binary. Recorded, not a clause failure.
- c1d CPUCLI +4.9 % sits inside the 6.5 % band: a lean, not a reading.
- Nothing here re-tests the timer hand-off part of the lag tail (§26 (a)).
