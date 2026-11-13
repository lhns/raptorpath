## 25. The c8 client `[LAG]` spread — diagnosis — pre-registration

§15 read the c8-100 client `[LAG]` p99 of one binary at 1248–3636 µs over
12 reps (2.9×); §17 and §21 read c8 client lag TREND-BETTER for R1/R2,
which may be that spread. Question: what makes one rep's p99 two to three
times another's, and is any of it an engine defect? Committed before any
VM contact. A diagnostic session: one binary, nothing is scored against an
arm and nothing ships from it. Ledgers go to `docs/l1-raw/lag-diag/`.

**Step 1 — what the existing ledgers say** (every c8-100 LIVE row of
`thread-p2a`, `-q1`, `-q2`, `-d9`, `-r1`, `-r2`: 112 rows, 14 (session,
arm) groups; correlations are Spearman within a group, pooled):

1. *c8-100 is two paths, not eight* (`spec=c2/c3/dual`: two owners,
   `owner-client-p0/p1`). Owners and every engine task are ordinary tokio
   tasks that migrate across the six workers; no path is placed on a
   worker. Per-worker `[THR]` cores are flat (0.06–0.12) and neither the
   hottest worker (ρ = +0.09) nor the worker spread (ρ = −0.01) co-varies
   with the lag. **Placement / owner co-location is ruled out**: there is
   no placement to vary and no signal.
2. *An instrument offset.* tokio rounds every timer deadline up to the
   next millisecond (`time/source.rs` `deadline_to_tick`, +999 999 ns) and
   the probe's 10 ms period is a whole number of milliseconds, so every
   tick of one process carries the same rounding offset φ ∈ [0, 1 ms)
   (re-drawn only when a tick is > 5 ms late, tokio `Interval`); the
   driver's own `now` is floored to the millisecond, adding a per-tick
   U ∈ [0, 1 ms). Lateness = φ + U + S, S the scheduling delay the probe is
   for. The module doc says "0–1 ms late by construction" but not that φ is
   per process, so it leaks into p99. Evidence: the per-rep client p50
   ranges 150–1440 µs, and in the pre-Q2 sessions the whole p99 spread is
   φ: q1 MAIN p99 sd 382 µs, p99 − p50 sd 40 µs (D9 56, IOS 76).
   ρ(p99, p50) = +0.53 pooled.
3. *A real tail in the current era.* Subtracting p50 removes almost
   nothing where the spread is large: d9 MAIN p99 sd 745 → p99 − p50 sd
   757; r1 MAIN 626 → 560; r2 MAIN 688 → 654 (pooled within-group
   variance ratio 0.72). p99 − p50 medians: q1 MAIN 932, q2 MAIN 946,
   q2 Q2 1438, d9 MAIN 1642, r1 MAIN 3708, r2 MAIN 2568, r2 R2 2218 µs
   (cross-session, report-only). In d9 the four slowest MAIN rows (82.9–
   88.8 Mbit/s) are its four highest-lag rows (ρ(lag, goodput) = −0.83,
   ρ(lag, client CPU s) = +0.74), but that sign flips in other sessions
   (q2 MAIN +0.48); only `rx_capped` (owner inbound drains hitting the
   cap) co-varies positively in most groups, a consequence of a late
   runtime as much as a cause. The 1-min loadavg before each invocation and
   the invocation order do not co-vary consistently.
4. Between d9 (88476a4, 2026-10-06) and r1 (ce60079, 2026-10-26) the only
   engine change is §16's opt-in instrument (pass-through below
   `RWM_RTOBS=2`), yet the c8 MAIN client cores rose 0.52 → 0.65 and the
   lag 2708 → 4647 µs: a day-to-day VM shift is as plausible as code.

So the ledgers settle the placement question (no) and part of the spread
(φ), but not what S's tail is: an in-process long poll, or the guest /
host not running the worker in time.

**The instrument** (measurement only; `RWM_RTOBS=0/1` do the same work in
the same order; one token added at level 1):

1. `[LAG]` gains `min_us=` (the window minimum ≈ φ: lateness cannot be
   below the rounded deadline), so the floor-corrected tail E = p99 − min
   is read per row.
2. `RWM_RTOBS=2`: `[LAGRAW]`, every probe sample (`at`, lateness) with
   the process origin in Unix µs and the transfer window; `[LPOLL]`, every
   poll of a timed task with wall ≥ `LONG_POLL` = 1 ms (the probe's
   rounding band: a tick later than floor + 1 ms is not rounding), with its
   end time (Unix µs), wall, CPU and context switches; `[TASK]` gains
   `long_polls=` and `max_poll_us=`. The engine's remaining untimed spawns
   (`net::run_with_tun` from `perf.rs`, and the helpers in `net/mod.rs`
   and `transport/quic.rs`) are wrapped in the same pass-through timer, so
   every engine task is covered.
3. Harness (`tools/l1/lagdiag_run.sh`): per-vCPU `/proc/stat` at 10 Hz
   around every invocation (steal and idle per vCPU).
4. Digest (`tools/l1/lagdiag_digest.py`, unit-tested): per row, the late
   ticks (lateness − min ≥ 1 ms) of the transfer window, each with its
   stall interval [observed − (lateness − min), observed]; a late tick is
   *overlapped* when a long poll on the same process intersects its stall
   by ≥ 500 µs (credited to the task with the largest intersection);
   F = overlapped / late ticks; long-poll wall per second; steal share and
   busy-vCPU counts (busy = idle + iowait < 50 % of the 100 ms interval).

**The session.** Binary = `main` 0972c3c + the instrument (commit named in
the result), archived LF, built fresh in a fresh target on the benchmark
VM, `sha256` in `BINSHA.txt` and re-verified before every row; the
instrument's unit tests (`runtime_obs`, `task_obs`, `gates`) and the
parser tests run in the session (a failure is `ABORT-TESTS`). c8-100 only
(`perf_rwm_c.sh c2 c3 bulk 100000000 1 dual`), the §21 battery env
(`env -u` the standard list, then `RWM_RDIAG=1 RWM_GEN=0 RWM_DIAG=1
RWM_PERF_TIMEOUT_S=150`) with `RWM_RTOBS=2` on 24 rows (12 blocks × seeds
42 and 7, seed order swapped on even blocks) and `RWM_RTOBS=1` on 6 rows
(one after each even block, seeds alternating) as the deep gauge's
overhead control. Smoke: one RTOBS=2 row, seed 42 (both `[LAGRAW]`, both
`[LPOLL]` summaries, `max_poll_us` on `[TASK]`, `min_us` on `[LAG]`, ≥ 20
stat samples); nothing in it is a result. Launch precondition as §21
(both locks free, 1-min loadavg < 2.0, no `cargo`/`rustc`); both locks held
for the whole session (build included); hard deadline launch + 2 h, soft =
hard − 10 min. Expected wall: build ≈ 10 min, tests ≈ 3 min, rows ≈ 8 min.

**Witnesses per row** (a failing row is `WITNESS-FAIL`, excluded and
counted): client `[LAGRAW]`, the client `[LPOLL]` summary, `[TASK]` lines
on both ends, ≥ 20 stat samples spanning the transfer, the row's `sha256`.
Mechanism proof (rule 1): the smoke shows `[LPOLL]` entries or an explicit
`n=0` summary on both ends and a `[TASK]` line naming `engine` (a newly
wrapped task).

**Decision rule (fixed now; client side of the RTOBS=2 rows; one outcome,
in precedence order).** Terciles by E (top 8 vs bottom 8).

0. `UNSETTLED-INSTRUMENT`: fewer than 18 live RTOBS=2 rows, or an abort
   cause fired.
1. **`IN-PROCESS(<task>)`**: pooled F ≥ 0.5 **and** the high tercile's
   median long-poll wall per second ≥ 1.5 × the low tercile's. The task
   holding ≥ 50 % of the overlapped late ticks is named
   (`IN-PROCESS(MIXED: t1, t2)` otherwise). Next: read that task's long
   poll, design the fix (a bounded per-poll budget or yield if it is an
   unbounded drain), red-first test, and a pre-registered ship battery
   (MAIN vs NEW, c1s-400, c1d-400, c2-100, c8-100, n = 6, the §21 pass
   rule plus a scored c8 client E spread clause), stopped at the gate.
2. **`VM-STEAL`**: F < 0.5 **and** (P(steal > 0 | the interval of a
   non-overlapped late tick) ≥ 2 × the base P(steal > 0) over the
   transfer, **or** the high tercile's median steal share ≥ 2 × the low
   tercile's and > 0). The spread is the host's; no engine change.
3. **`GUEST-CPU`**: F < 0.5 **and** P(≥ 5 of 6 vCPUs busy | the interval of
   a non-overlapped late tick) ≥ 2 × its base rate: client, server and
   netem softirqs contend for the guest's six vCPUs, a property of the
   testbed, not an engine defect.
4. **`UNSETTLED`**: none of the above; the result names what would decide.

**Reported, not gated:** finding 2's prediction (`PHASE-OFFSET`: client
`min_us` spreads over ≥ 500 µs across rows and ρ(min_us, p50) ≥ 0.6); the
deep gauge's perturbation (`PERTURB`: RTOBS=2 vs RTOBS=1 client E medians;
> 1.5 × marks every finding **[H]**); the server side's same numbers (same
VM, the control); per-task long-poll wall and max poll; `[TASK]`
`asl_inv`/`asl_none`; goodput per row. Whatever the outcome, finding 2's
instrument fix (score the lag clause on E = p99 − min, not p99) is
proposed for future batteries; it changes no engine behaviour.

**Abort causes** (the result opens with this table, filled):
`ABORT-PRECONDITION`, `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD`,
`ABORT-TESTS`, `ABORT-SHA`, `ABORT-SMOKE`, `ABORT-BUDGET` (rows truncated at
the soft deadline: scored on what ran, `UNSETTLED-INSTRUMENT` below 18);
void class `VOID-COTENANT` (a `cargo`/`rustc` before or after a row).

### 25. The c8 client `[LAG]` spread — result

Read 2026-10-26 against the pre-registration above, literally. **`UNSETTLED`**:
long task polls cover only a third of the late ticks (F = 0.33 < 0.5), and
neither `/proc/stat` steal nor busy vCPUs are elevated at the late ticks.
Placement is ruled out (step 1). Nothing in this session names an engine
defect that explains the rep-to-rep spread, so no fix is designed and no
ship battery is pre-registered. Nothing ships from this section.

*Session.* Binary `eebb80f8…3ed10fcb` (pre-rebase commit `adb48f8` = 0972c3c + the
instrument `e006240` + harness `adb48f8`; rebased onto 3982bd1 as
`b2a5fcc` / `73bd3df`, same content), built fresh on the benchmark VM
(330 s). In-session tests: cargo `runtime_obs task_obs gates` 33 passed,
0 failed; `test_l1common.py` and `test_lagdiag_digest.py` rc 0. Locks were
taken at 16:43:17Z and released at 17:01:36Z. Smoke passed. 30 rows, 30
LIVE, 0 witness failures. Exit state: 0 `raptorpath`, 0 `rp-*`, run dir
removed. Ledgers are in `docs/l1-raw/lag-diag/` (`digest.txt`, `era.txt`,
`diag/` with every row's logs and 10 Hz stat file).

*Abort table.* No abort fired: `ABORT-PRECONDITION`, `-LOCK`, `-CRLF`,
`-BUILD`, `-TESTS`, `-SHA`, `-SMOKE` and `-BUDGET` all read *no*.
`VOID-COTENANT`: 0 of 30 (no `cargo`/`rustc`). Note: the 1-min loadavg
rose to 5.8–9.8 over the last 8 rows. The cause is D-state block-I/O
kworkers (`blkcg_punt_bio`), not CPU, and it persisted after the session
with no `raptorpath` running. Those rows' E median is 2700 µs against
3277 µs for the rest, so they did not drive the high lags.

*The decision numbers* (client, 24 RTOBS=2 rows; E = p99 − min):

| quantity | value | rule |
|---|---|---|
| late ticks / overlapped by a long poll | 3789 / 1239, **F = 0.327** | IN-PROCESS needs ≥ 0.5 |
| long-poll wall/s, high vs low tercile of E | 0.229 vs 0.201 (×1.14) | IN-PROCESS needs ≥ 1.5 |
| P(steal > 0): base vs at non-overlapped late ticks | 0.378 vs 0.347 | VM-STEAL needs ≥ 2× |
| steal share, high vs low tercile median | 0.0100 vs 0.0082 (×1.22) | VM-STEAL needs ≥ 2× |
| P(≥ 5 of 6 vCPUs busy): base vs at late ticks | 0.004 vs 0.003 | GUEST-CPU needs ≥ 2× |
| client E, high vs low tercile median | 4110 vs 2004 µs | the spread under study (2.0×) |

*Reported, not gated.*

- **`PHASE-OFFSET`.** Client `min_us` spans 43–973 µs over the 30 rows
  (spread 930 µs). Finding 2's per-process rounding offset is confirmed in
  size. ρ(min_us, p50) = 0.48, below the predicted 0.6: p50 also carries
  U and S.
- **`PERTURB`.** RTOBS=2 vs RTOBS=1 client E medians are 3261 vs 2528 µs
  (×1.29, under the 1.5 flag). p99 medians are 3514 vs 3020 µs.
- **Server, the same-VM control.** Server F is 0.05–0.21. Its late ticks are
  even less a matter of long polls.

*Findings.*

1. **The sender task is the only in-process holder of the lag probe, and its
   long polls are CPU.** 97.6 % of overlapped late ticks fall under a
   `sender` poll. The sender logs ≈ 920 polls ≥ 1 ms per row (≈ 110/s).
   Their wall is p50 1.4 ms, p99 11 ms, max 115 ms, and their CPU is p50
   1.4 ms, p99 10 ms, max 112 ms. They hold 44.5 of 47.1 s of their own
   wall in CPU, and 83 % of them show no context switch. The ≥ 10 ms polls
   (≈ 11 per row) fall throughout the transfer, weighted to its second
   half. They are not ramp-up. A single cooperative task running up to
   0.11 s of CPU without yielding is a starvation hazard for anything
   queued behind it on its worker. But it does not explain the spread:
   long-poll wall barely differs between high- and low-lag reps (×1.14).
   **[H]**, not localised: which phase of `run_window_sender` runs 10–112 ms
   per poll is not measured (no section split inside the loop).
2. **Two thirds of the late ticks have no long poll over them.** They do
   not coincide with guest steal or saturated vCPUs either. Post hoc (not
   pre-registered; n = 24, Spearman): E co-varies with goodput (ρ = −0.71,
   reproducing §15's −0.83), with the `[TASK]` `asl_none` sum (ρ = +0.54:
   wall lost inside polls with no guest context switch, i.e. vCPU
   descheduling or clock granularity) and weakly with steal share
   (ρ = +0.32). It does not co-vary with long-poll wall (+0.23), the loadavg
   (−0.06) or row order (+0.11). The remaining candidates are tokio's timer
   hand-off and host-side vCPU scheduling. tokio's timer hand-off: the
   driver is serviced only by a parking worker, so a tick waits until some
   worker parks. Host-side vCPU scheduling would be invisible to the guest's
   steal counter at 100 ms. The probe as built cannot separate them.
3. **What would decide it.** (a) A tokio-independent reference clock: a
   plain OS thread sleeping 10 ms with `clock_nanosleep` and recording its
   own lateness. If its tail tracks the `[LAG]` tail per rep, the cause is
   host/vCPU; if it stays flat, it is the runtime's timer hand-off. (b) For
   finding 1, a section split of the sender loop (the drain, the SACK
   drain, emission, the throttled refreshes) under RTOBS=2, to name the
   phase that runs 10–112 ms. Both are measurement-only. Neither is run
   here.
4. **The battery lag clause.** Finding 2 of step 1 stands, and the session
   measured its size directly (`min_us` 43–973 µs). Future batteries should
   score `[LAG]` on E = p99 − `min_us`, which `[LAG]` now prints at
   `RWM_RTOBS=1`, rather than on raw p99. This changes no engine behaviour.
   It removes a per-process uniform offset of up to ≈ 1 ms from the
   comparison.

**Scoring decision (operator, 2026-10-26).** From the next battery on, the
`[LAG]` clause (client and server) is scored on **E = p99 − `min_us`** of
the window, not raw p99; `min_us` is printed at `RWM_RTOBS=1` (rebased
`b2a5fcc`). Raw p99 stays reported.
