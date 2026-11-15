## 26. The c8 lag tail and the sender's long polls — the two deciding measurements — pre-registration

§25 left two questions open, and its finding 3 named the measurement that
decides each. **(a)** Two thirds of the late `[LAG]` ticks have no long poll
over them. Is their per-rep tail made outside tokio (host or guest OS wake
latency), or inside the runtime (the timer hand-off: the driver is serviced
only by a parking worker)? **(b)** Which phase of `run_window_sender` runs
the sender's polls of ≥ 1 ms (up to 112 ms CPU in one poll)? Does it break
plan rule 3 ("per wake, a task's work is bounded by its own batch law (Law 0
emit burst); after that it takes one natural yield")?

This section is committed before any VM contact. It is a diagnostic
session: one binary, measurement only, nothing ships from it. A fix follows
only per the decision rule below, and goes through its own ship battery
stopped at the gate. Ledgers go to `docs/l1-raw/lag-diag2/`.

**The instrument** (measurement only; `RWM_RTOBS=0/1` do the same work in
the same order; everything below is `RWM_RTOBS=2`):

1. *(a) An OS reference clock.* `[OSLAG]` / `[OSLAGRAW]` come from one plain
   OS thread (`rp-osref`). It sleeps to absolute 10 ms deadlines on
   `CLOCK_MONOTONIC` (`clock_nanosleep`, `TIMER_ABSTIME`) and records its own
   lateness over the same windows and on the same time base as `[LAG]`. It
   has the same period as the probe, but no tokio driver, no ms rounding and
   no worker hand-off. Its tail is what the OS alone does to a thread that
   wants to wake on time. `[THR] os` also gains `run_delay_us=` per thread
   (`/proc/<tid>/schedstat`: time runnable but waiting for a guest CPU).
2. *(b) The sender loop's section split* (`[TXSEC]`). One loop iteration of
   `run_window_sender` is partitioned contiguously into named sections at
   code landmarks (input drain, SACK drain, pre-select refreshes, the flush
   and `select!` awaits, the post-select body by wake arm: emission, input,
   NACK, deficit, request, tail, packer, idle, and the end-of-loop work).
   Per-iteration section times sum to the iteration time. Each section is
   timed like `[RXSEC]`: a vDSO monotonic read, no thread-CPU syscall. Every
   sender long poll in `[LPOLL]` carries `iters=` (the loop iterations it
   ran without returning `Pending`) and `secs=` (its time per non-await
   section, plus `other` = poll wall minus their sum). Await sections are
   excluded from the per-poll split because their wall can include sleep.

**The session.** The binary is this branch: 3982bd1 + §25's instrument + the
above. The commit is named in the result. It is archived LF, built fresh in
a fresh target on the benchmark VM, and its `sha256` is re-verified before
every row. The in-session tests are the instrument's unit tests
(`runtime_obs`, `task_obs`, `gates`, the `q2_` source pins) and the parser
tests; a failure is `ABORT-TESTS`.

- **Cell and env.** c8-100 only, with §25's env (the §21 battery env).
- **Rows.** `RWM_RTOBS=2` on 24 rows: 12 blocks × seeds 42 and 7, with the
  seed order swapped on even blocks. `RWM_RTOBS=1` on 4 rows, after blocks
  3, 6, 9 and 12, as the overhead control.
- **Harness.** `tools/l1/lagdiag2_run.sh`, the same envelope as §25, with
  the 10 Hz per-vCPU `/proc/stat` and the 1-min loadavg recorded per row.
- **Smoke.** One RTOBS=2 row, seed 42. It must show:
  - client `[OSLAG]`/`[OSLAGRAW]` and `[TXSEC]` with `iters > 0`;
  - sender `[LPOLL]` entries carrying `iters=`/`secs=`;
  - `run_delay_us=` on `[THR] os`;
  - §25's witnesses.

  Nothing in the smoke is a result.
- **Precondition and budget** as §25. Hard deadline is launch + 2 h; soft is
  hard − 10 min.

**Decision rule (a)** (fixed now; client, live RTOBS=2 rows; E_lag = `[LAG]`
xfer p99 − `min_us`, E_os = `[OSLAG]` xfer p99 − `min_us`; terciles by
E_lag). One outcome, in precedence order:

0. `UNSETTLED-INSTRUMENT`: fewer than 18 live rows.
1. **`OS-LEVEL`**: Spearman ρ(E_lag, E_os) ≥ 0.5 **and** the high tercile's
   median E_os ≥ 1.5 × the low tercile's. The per-rep spread is made below
   tokio (host or guest). It is not an engine defect, and no engine change
   follows. `run_delay` then says guest (rises with E_lag) or host (flat).
2. **`RUNTIME`**: ρ < 0.3 **and** the E_os tercile ratio < 1.2. The OS
   thread wakes on time while the probe does not, so the spread is in the
   runtime. Since (b)'s long polls do not covary with E (§25), the next step
   is tokio's timer hand-off. That is designed and reported, not built here.
3. **`MIXED`**: otherwise.

**Decision rule (b)** (fixed now; client `task=sender` `[LPOLL]` entries of
live RTOBS=2 rows; on the polls ≥ 10 ms, or on all ≥ 1 ms polls if fewer
than 20 polls reach 10 ms, stated). One outcome, in precedence order:

1. **`UNBOUNDED-PER-WAKE(<top section>)`**: the p90 of `iters` per poll is
   ≥ 2. The sender runs several loop iterations per wake without yielding,
   so its per-wake work is bounded by nothing of its own. That breaks plan
   rule 3. **Next:** design the bound per rule 3: per wake, at most the work
   of one Law-0 emit burst (`RWM_EMIT_BURST`, the shipped burst, no new
   constant), then one natural yield. Then a red-first test, implementation
   by Sonnet, and a pre-registered ship battery stopped at the gate:
   - MAIN vs NEW, c1s-400 / c1d-400 / c2-100 / c8-100, n = 6;
   - §21's pass rule, with the `[LAG]` clause scored on E = p99 − `min_us`;
   - plus the sender's long-poll count and max poll reported.
2. **`LONG-ITERATION(<section>)`**: the p90 of `iters` ≤ 1 and one section
   holds ≥ 50 % of the long-poll wall. One phase does unbounded work inside
   one iteration. **Next:** read that section. If its work per wake is a
   drain or scan with no bound from its own batch law, the fix is that bound
   per rule 3, with the same gate as above. Otherwise report it as bounded
   work that is merely large, with no fix.
3. **`UNATTRIBUTED`**: `other` holds ≥ 50 % of the wall. The partition
   misses the heavy work; report it and stop.
4. **`MIXED`**: otherwise. Report the shares and stop.

**Reported, not gated:**
- ρ(E_lag, run_delay/s) and ρ(E_os, run_delay/s);
- the `[TXSEC]` pooled section table (share, count, max);
- iters per poll for the ≥ 1 ms polls;
- PERTURB (RTOBS=2 vs RTOBS=1 client E_lag medians);
- §25's long-poll overlap F, re-read on this binary;
- the server side;
- loadavg per row.

**Abort causes:** §25's table.

**§26 amendment 1 (committed before any VM contact for this section).** The
benchmark VM had a multi-hour storage stall at low loadavg (I/O pressure
`full` 75–99 %). Three changes:

1. **Launch precondition.** It gains `/proc/pressure/io` `full avg60` < 5.0
   (`lagdiag2_run.sh`, `suites_run.sh`).
2. **I/O pressure is recorded.** The `full` line goes into the 10 Hz stat
   file of every row, and `full avg10` / `avg60` into every ROW line. A row
   whose `full avg10` reaches 20 at any sample is **`VOID-IOSTALL`**:
   excluded and counted (`lagdiag2_digest.py`, tested). The ≥ 18-live-rows
   bar is unchanged.
3. **Full suites first.** The full test suites run before the diagnostic, in
   their own locked session on the same tree (`suites_run.sh`: release,
   doc, wasm and debug cargo suites plus the parser tests, with §8's flake
   rule). The diagnostic launches only after they are green.

Both stages are driven by a detached waiter on the VM (`lag26_waiter.sh`).
It retries a stage every 5 min while that stage exits on the precondition
or on a held lock. It never touches a lock path itself. Hard deadline for
the diagnostic is launch + 4 h. Nothing else in §26 changes.

**§26 amendment 2 (pre-launch, agent rules).** The measurement runs from
tmpfs:

- The measured binary and the run dir live under `/dev/shm/lag26`. The
  driver logs were already in `/tmp`, which is tmpfs on this VM.
- The run dir is copied to disk (`<root>/run`) when the session exits.
- The build and the full suites stay on disk, and the I/O precondition is
  kept.

**§26 amendment 3 (pre-launch; agent rules, user-approved; supersedes
amendment 1's load and io gates and its void).**

1. **Launch precondition.** Both locks free, no `cargo`/`rustc`, and
   `/proc/stat` `procs_running` ≤ 2 in each of 5 samples taken 2 s apart.
   Loadavg is not used: it counts disk-blocked tasks.
2. **Disk pressure is a covariate, not a gate.** The rows run in memory
   (amendment 2). `full avg10` is recorded before and after every row (ROW
   line) and at 10 Hz (stat file). A row that reached `full avg10` ≥ 20 is
   **flagged**, never voided, when its client E_lag is a Tukey outlier
   (outside Q1 − 1.5 IQR … Q3 + 1.5 IQR of the live RTOBS=2 rows).
3. **On-disk rows are void.** A row whose binary or run dir is not on tmpfs
   is `VOID-ONDISK`. The harness checks both with `stat -f` before the
   locks and refuses (`ABORT-ONDISK`).
4. **Builds and the full suites** have no load or io gate: locks and no
   other `cargo`/`rustc` only. The io state is recorded per suite.

### 26. The c8 lag tail and the sender's long polls — result

Read 2026-10-27 against the pre-registration above and its amendments 1–3,
literally. **(a) `MIXED`. (b) `UNBOUNDED-PER-WAKE(sack)`**: the sender runs
several loop iterations per wake without yielding, and 81 % of the wall of
its polls of 10 ms or more is the SACK drain. Per (b)'s rule this breaks plan
rule 3; the bound is designed in §28 and stops at the ship-battery gate.

*Session.* The binary is `37dfd2f8…ffab361` (tree `4abe306`, archived LF,
built fresh on disk). It ran from `/dev/shm/lag26`, so every row was in
memory and none is `VOID-ONDISK`. A detached waiter on the VM started at
20:31Z and retried through the I/O stall and other agents' builds and locks.
- *Suites.* They ran 21:23–23:08Z under both locks:
  - release: 1122 passed, 0 failed, 50 ignored;
  - doc: rc 0;
  - wasm: 35 passed;
  - debug: 981 passed, 0 failed, 49 ignored;
  - parser tests: rc 0;
  - 0 identity panics.

  `TESTS-OK`.
- *Diagnostic.* The rows ran 23:12–23:25Z: 28 rows, 28 LIVE, 0 witness
  failures, in-session tests 53 passed. The `procs_running` precondition
  held. Exit state: 0 `raptorpath`, 0 `rp-*`; the VM dirs were removed.
- *Ledgers.* `docs/l1-raw/lag-diag2/`: `digest2.txt`, `digest.txt` (§25's
  digest re-read on this binary), `era.txt` (io `full avg10` before and after
  every row), `SUITES-TESTS.txt`, `diag/`.

*Abort table.* No abort fired: `ABORT-PRECONDITION`, `-LOCK`, `-ONDISK`,
`-BUILD`, `-TESTS`, `-SHA`, `-SMOKE` and `-BUDGET` all read *no*.
`VOID-COTENANT`: 0. Four RTOBS=2 rows reached io `full avg10` ≥ 20, and
none is an E outlier, so 0 are flagged (`IO-COVARIATE`).

*(a) The OS reference clock* (24 RTOBS=2 rows):

| quantity | value | rule |
|---|---|---|
| ρ(E_lag, E_os) | **0.26** | OS-LEVEL needs ≥ 0.5; RUNTIME needs < 0.3 |
| E_os median, high vs low tercile of E_lag | 912 vs 522 µs (×1.75) | OS-LEVEL needs ≥ 1.5; RUNTIME needs < 1.2 |
| E_lag median, high vs low tercile | 4035 vs 2387 µs | — |
| ρ(E_lag, worker run_delay/s) | −0.47 | reported |

Neither bar is met: the rank correlation is weak, but the OS thread's tail
is larger in the high-lag reps. Read plainly, both a host/OS component (E_os
p99 300–2000 µs, about a quarter of E_lag) and a larger runtime component
are present. Guest run-queue wait does not drive it: more run_delay goes
with *less* lag.

*(b) The sender's long polls* (client, 21 558 sender polls ≥ 1 ms over 24
rows):

| quantity | polls ≥ 1 ms | polls ≥ 10 ms (n = 195) |
|---|---|---|
| loop iterations per poll, p50 / p90 | 3 / 4 | **8 / 12.6** |
| SACK drain share of poll wall | 0.71 | **0.81** |
| emission share | 0.13 | 0.08 |
| unattributed (`other`) | 0.02 | 0.02 |

`[TXSEC]`, pooled over all iterations (with `select` and `flush` the
awaits):

| section | share of sender wall | per iteration | max |
|---|---|---|---|
| `select` (await, incl. sleep) | 0.65 | — | 94 ms |
| **`sack`** | **0.23** | 183 µs mean | 19.9 ms |
| `emit` | 0.05 | 178 µs mean, over the 58 862 emitting iterations | 8.2 ms |
| `arm_nack` | 0.03 | — | 3.3 ms |
| `arm_in` | 0.02 | — | 11.2 ms |

*Reported.* PERTURB is ×1.03 (RTOBS=2 vs RTOBS=1 E_lag medians), so the
deep gauge does not move the lag. §25's long-poll overlap re-read on this
binary is F = 0.29, and 97 % of the overlapped late ticks again fall under
the sender.

*Findings.*

1. **Rule 3 is broken, and the SACK drain is the work.** At c8 one sender
   wake runs up to 12+ loop iterations. Each iteration drains *every* queued
   `SackReport` (`while let Ok(report) = sack_rx.try_recv()`; the channel
   depth is 64) and marks every retained seq in every range. Every report
   re-sends all ranges above the stalled frontier, so most seqs are already
   marked and the work is re-done per report: O(span above the frontier) per
   SACK-bearing ack. That is why the drain is 23 % of the sender's whole wall
   and its single longest section. The plan's rule-3 bound (one iteration
   per wake, then a natural yield) is designed in §28. **The redundant
   re-marking is a separate CPU defect.** It is recorded here and not fixed:
   making the marking incremental changes the release law's input path and
   needs its own derivation and battery.
2. **The lag tail is not one cause.** The sender's polls cover 29 % of the
   late ticks; the OS's own wake tail is about a quarter of E. The remainder
   is in the runtime (timer hand-off). The rule-3 bound should shorten the
   sender's polls (§28 predicts a lower c8 client E, reported). It is not
   expected to remove the rep-to-rep spread.
