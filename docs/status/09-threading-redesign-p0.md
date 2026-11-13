## 9. Threading redesign P0 — per-thread core budget — pre-registration

Phase P0 of the threading redesign (owned paths, batched rings, no shared
hot locks): **measure, no behaviour change.** Committed before VM contact;
no number below is a result. Nothing is flipped by this battery.

**Question (D3).** At `c1s-400` the shipped stack is pinned near 510 Mbit/s
with the sender busy 35 % (§10 of `feat/quic-feeder-v2`, recorded in §10
below as evidence), which makes the ceiling a non-sender one. The topology
audit (D3) names the server's single receiver task (`run_receiver`:
decode, deliver, one WindowAck per data datagram) as the best-supported
candidate: P1 measured it at 0.61 core at 315 Mbit/s; scaled to 510 Mbit/s
with §10's 28 % lower server CPU per byte that is ≈ 0.71 core, and ≈ 0.99
if the receiver took no share of the saving. **Is any single server thread
or task at ≥ 0.9 core at `c1s-400`?** `c1d-400` is measured beside it for
the budget table and is not part of the question.

**Stop rule (from the plan).** If `D3-CONFIRMED` (below), P2's server half
— the receiver-role split — becomes the primary gate for `c1s`, and that
is stated in this file before P2 runs.

**What changed in the engine (measurement only).**

1. `main.rs`: `#[tokio::main]` is replaced by its own expansion,
   `Builder::new_multi_thread().enable_all().build()` + `block_on` (same
   default worker count, the available parallelism: 6 on the VM), plus
   `thread_name_fn` naming each pool thread `rp-w-<n>` in spawn order. No
   other runtime knob changes. Workers are launched first, in index order,
   so `rp-w-<i>` for `i < num_workers` is worker `i` — a hypothesis from
   tokio's launch order (the join, `worker_thread_id`, is unstable);
   `n ≥ num_workers` are blocking-pool threads (none expected on Linux).
2. `src/rtobs.rs`: the `[THR]` readout from the STABLE `RuntimeMetrics`
   only (`num_workers`, `worker_total_busy_duration`, `worker_park_count`,
   `worker_park_unpark_count`; no `tokio_unstable`), one `[THR] rt` line per
   worker; on Linux one `[THR] os` line per OS thread from
   `/proc/self/task/<tid>/stat` (utime + stime, `comm`), the `block_on`
   main thread (`comm=raptorpath`) included; one `[THR] sum` line;
   elsewhere `[THR] os unavailable` (Windows builds: the reader is
   `cfg(target_os = "linux")`).
3. The `[LAG]` probe: one task on the runtime ticks every 10 ms (tokio
   `interval`, `MissedTickBehavior::Delay`) and logs `now − scheduled`;
   p50/p99/max printed. Provenance: the Cats Effect starvation checker
   (th1 §1.5; paper §11.2 row `LAG_TICK`). **Instrument floor**: tokio
   rounds timer deadlines up to the next millisecond, so every tick reads
   0–1 ms late by construction — p50 ≈ 0.5–1 ms is the floor, not
   starvation; p99 and max are the signal. It runs on a worker and does not
   see the main thread.
4. Windows of measurement: `phase=xfer` lines bracket one perf object — the
   client's timed run (`run=1`; snapshot before `run_object`, lines after its
   ack) and the server's object (`obj=1`; snapshot at its first packet,
   lines after its completion ack is handed to the engine). Per-thread
   `cores` = thread CPU over that window ÷ the window's wall. `phase=run`
   lines are printed once at process end (cumulative since the runtime was
   built; the server's ride the SIGTERM graceful path; reported only).
5. Harness: `perf_rwm_c.sh` echoes each endpoint's `[THR] sum phase=xfer`
   and `[LAG] phase=xfer` lines into the driver output; the full tables stay
   in the endpoint logs the battery copies. `l1common.thr`/`thr_columns`
   parse them (offline tests in `test_l1common.py`, pinned to the Rust
   renderer's unit-test strings).

**Component characterization (rule 14), committed with the code.** Unit
tests in `rtobs.rs`: the `/proc` stat parser (comm counted from the last
`)`), the quantile rule (= `l1common.q`), the exact `[THR]` token set and
its deltas, the `os unavailable` path, `[LAG]` absent-as-`-`, and a live
runtime whose worker is named `rp-w-*` and whose probe yields ≥ 10 samples
in 250 ms with p99 ≥ p50. The routing test `tests/rtobs_reachability.rs`
runs the SHIPPED binary as perf client and server and asserts both ends'
`phase=xfer` and `phase=run` tables (one runtime: equal worker counts),
`busy_frac` in [0, 1.05], `[LAG]` n > 0, and on Linux an `rp-w-*` thread and
the main thread in `[THR] os`. What the battery should then see: 6 workers
per side; the server's hottest thread somewhere in 0.6–1.0 core; the
client's sender-hosting worker ≈ 0.35–0.6 core. What the benches cannot
see: which task a worker's CPU belongs to (tasks migrate between workers,
D7), hence the `[RDIAG]` reading below.

**Instruments for D3** (both, on the server, in the `c1s-400` P0 rows):

- **task-level**: `RWM_RDIAG=1` (exists, `net/mod.rs` ≈ 1250,
  `receiver.rs` ≈ 1115): `[RDIAG] busy=<%>` = 1 − (time the receiver task
  spent awaiting its `select!`) ÷ wall, every ≥ 500 ms, plus the inbound
  `msg_tx` depth. `busy` counts the task's CPU AND its blocked time inside
  its poll (quinn's mutex, the scheduler lock), so it is ≥ the task's
  cores; a ready-but-unscheduled task counts as idle. The per-row reading
  is the median over the IN-TRANSFER lines (`msgs ≥ 1000/s`; the transfer
  runs ≈ 53 k datagrams/s, the idle tail < 100/s). `busy ≥ 90 %` is read as
  "the receiver task is the service wall (≥ 0.9 core-equivalent of wall,
  lock wait included)";
- **thread-level**: the hottest server OS thread's `cores` in the `obj=1`
  window. A thread can host several tasks and a task can spread over
  threads, so ≥ 0.9 here is sufficient for a saturated thread, not
  necessary for a saturated task.

**Binaries and arms.** Two binaries, interleaved in one session (rule 3):
`P0` = this branch's HEAD (archived with `git -c core.autocrlf=false -c
core.eol=lf archive`, built fresh on the benchmark VM in a fresh run root
and target dir); `MAIN` = `main` 8d7d8c1 (the shipped stack the §10 NEW arm
measured, modulo §10's own known effect 1), archived the same way and built
in its own fresh target dir. `sha256` of both in `BINSHA.txt`, re-verified
before every invocation. MAIN is the control for the no-behaviour-change
check; it is added to the brief's design because §10's numbers are another
session (cross-session drift has measured 2.3×, rule 3) and §10's NEW was
not byte-identical to `main`.

**Invocation.** Every row is `perf_rwm_c.sh c1 c1 bulk 400000000 1
<single|dual>` (`c1s-400` = `c1 c1 single 400000000`, `c1d-400` = `c1 c1
dual 400000000`, §5's geometry), `--window-reliable`, fresh topology, with
`RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`; every
arm first `env -u`'s `RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST RWM_RDIAG` (rule 15d) and sets `RWM_RDIAG=1` explicitly;
`RWM_RDIAG` is in `RWM_FORWARD`, so it reaches BOTH ends (the client's
reading measures its ack task; reported). Neither §10 arm ran `RWM_RDIAG`.

**Plan.** Blocks (rep 1, seed 42), (rep 1, seed 7), (rep 2, seed 42): in
each block `c1s-400` then `c1d-400`, each P0 and MAIN, the arm order
rotated by the block index. **n = 3 per arm and cell** (2 × s42, 1 × s7;
both seeds reported, rule 4); 12 invocations.

**Capacity and headroom (rule 16).** `c1s-400`: 1000 Mbit shaped, §10 NEW
510.4 Mbit/s, ≈ 48 % headroom; `c1d-400`: 2000 Mbit, 424.4, ≈ 79 %. No
throughput target is set; goodput and CPU are no-change guards.

**Witnesses per row** (`p0_parse.py row`; a failing row is `WITNESS-FAIL`
or `CONTAMINATED`, excluded and counted; no client summary is `NO_DATA`):
§5's set via `stage3_parse.make_row` (`[PIPE] window/Rlc/bulk` and the
driver header; `[GATES]` on both ends; the RLC auto-select line on both;
cadence ACTIVE on both; `[GATES] RWM_POOL_ANCHOR=0` on both; no generation
guard), plus `[GATES] RWM_EMIT_BATCH=1` and `RWM_RDIAG=1` on both ends,
plus ≥ 1 in-transfer `[RDIAG]` line on the server; P0 rows: the `phase=xfer`
`[THR] rt` lines, the `[THR] sum` line, `[LAG]` with n > 0, and (Linux)
`[THR] os` lines naming an `rp-w-*` thread, on BOTH ends; MAIN rows: no
`[THR]`/`[LAG]` line on either end (two-sided, rule 15c: the wrong binary
fails).

**Outcomes.**

*D3* (P0 rows at `c1s-400`; medians over the live rows):
- **`D3-CONFIRMED`** iff median server `[RDIAG] busy` ≥ 90 % OR median
  hottest-server-thread `cores` ≥ 0.90. The stop rule fires and is recorded
  in the result.
- **`D3-REFUTED-WITH-RECORD`** iff both are readable at ≥ 3 live rows and
  both are below their threshold.
- **`UNSCOREABLE`** with fewer than 3 live P0 rows at `c1s-400`;
  **`NEEDS-MORE-<rdiag|thr>`** when one instrument is unread at ≥ 3 rows and
  the other is below its threshold.

*No-behaviour-change check* (P0 against MAIN, in session, per cell, for
goodput and `CPUCLI`, §5's committed relative MDE: `c1s-400` 4.9 % / 2.4 %,
`c1d-400` 5.6 % / 6.5 %): a clause is *within* iff |med(P0)/med(MAIN) − 1|
≤ MDE; outside the MDE it is *MOVED* when the two arms' [min, max] ranges are
disjoint and *UNDERPOWERED* when they overlap. **`NO-CHANGE-HELD`** iff every
clause is within; **`REFUTED-WITH-RECORD`** (the instrument changed
behaviour) iff any clause is MOVED; **`GUARD-UNDERPOWERED`** otherwise;
**`UNSCOREABLE`** with fewer than 3 live rows of either arm at a cell.
Reported beside it, not scored: P0's medians against the §10 NEW numbers
the brief names (`c1s-400` 510.4 Mbit/s and 5.73 s `CPUCLI`; `c1d-400` 424.4
and 11.19 s), and MAIN's identity against §10's control band (goodput
median within ±2 × rel_gp: `c1s-400` [467.1, 568.5], `c1d-400` [377.8,
473.0]; outside = `CONTROL-MOVED` beside that cell).

*The deliverable* (not an outcome; reported in every case): a per-thread
core budget for client and server at both cells — the OS threads ranked by
`cores` (`thr_r1..3`, with the hottest thread's `comm` per rep), the main
thread, the `rp-w-*` sum, the process sum, the six workers' `busy_frac`
ranked, park/unpark per second, `[LAG]` p50/p99/max, `[RDIAG]` busy and
`q_avg`, median [min–max] with n, and every per-rep value.

**Predictions** (checks, read as `MET`/`MISSED` in the result): D3 is borderline —
server `[RDIAG] busy` 75–95 % and the hottest server thread 0.6–0.9 core
(the derivation above); `NO-CHANGE-HELD` at both cells (the instrument adds
≈ 100 timer wakes/s and two `/proc` reads per object, ≪ the ≈ 50 k
datagrams/s and 16–39 k worker park/unparks per second).

**Known effects, declared.** (1) The lag probe is a new task on the runtime
in the P0 binary (≈ 100 wakes/s); (2) the perf ends read `/proc` (≈ 10
files) on the main thread at the object's first packet (server) or before
it (client), and again at its end; (3) `RWM_RDIAG=1` adds two clock reads
per inbound message on both ends in BOTH arms; (4) thread names change
`comm` from `tokio-runtime-w` to `rp-w-<n>`. These are what the
no-behaviour-change check measures.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either binary),
`ABORT-TESTS` (`cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`, `cargo test --doc -p raptorpath
--release`, `cargo test -p raptorpath-wasm` with `GOLDEN_CAPTURE` unset, and
the python parser tests; a failure that does not pass on an immediate
re-run of that test alone is real — the automatic GO is withheld and the
operator writes GO only for a documented flake, e.g. the known
`sigma_diag_reachability`), `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`,
`ABORT-SMOKE` (smoke = `c1s-400` P0, `c1s-400` MAIN, `c1d-400` P0, seed 42:
every row `LIVE` with its CPU line and `[TRUTH]` per leg; both arms
present), `ABORT-RC` (row `VOID-RC`, the battery goes on), `ABORT-BRINGUP`
(`NO_DATA` after 2 attempts). Void class `VOID-COTENANT`. The Windows check
(`cargo check -p raptorpath --target x86_64-pc-windows-gnu`, if that target
is installed on the VM) is recorded and does not gate.

**Budget.** ≈ 35 min build + tests (§10's session: 31 min), ≈ 10 min for
main's binary, ≈ 1 min smoke, 12 invocations × ≈ 20 s ≈ 4–5 min; ≈ 55 min
of the 5 h cap; hard backstop at lock acquisition + 4 h 50 min. No cut is
planned; a truncated battery is scored at the n reached, and a null at
reduced n reads `GUARD-UNDERPOWERED` / `NEEDS-MORE-<n>`, never a
refutation.

**Session rules.** §10's: both locks for the whole session via
`lib_battery.sh` (`p0_run_all.sh`, started by the lock waiter
`p0_launch.sh`); the operator checks at most once per ≈ 20 min (rule 13,
recorded); `pkill -x raptorpath` only; no `ens18`, firewall, `sshd` or
non-`rp-*` namespace is touched. Harness: `tools/l1/p0_run_all.sh`,
`p0_battery.sh`, `p0_parse.py` (offline test `test_p0_parse.py`). Compact
ledgers are copied to `docs/l1-raw/thread-p0/`.

**Amendment A1** (committed before any scored result; no battery row
exists). Session 1 (locks 16:05:05Z–16:39:32Z, run root `thrp0`, binary
`743bc004…` built from `dc38801`) ended `ABORT-TESTS`: the suite read 1057
passed, **1 failed**, 50 ignored (doc rc 0; wasm 35 passed; the python
parser tests 88 / 31 / 45 checks, 0 failed; every `runtime_obs` unit test
and the reachability test passed; the Windows check not run: the VM has no
`x86_64-pc-windows-gnu` target). The failure was real, not a flake, and
caused by this tree: `store_cap_sf_bench::the_wires_offered_load_has_no_congestion_control`
bans the substrings of congestion-response names (`rto`, `rtt`, `cwnd`, …)
in `src/perf.rs`, and the new module's name, `rtobs`, contains `rto`. The
operator wrote NOGO (16:33Z); the envelope still built main's binary and
ran the smoke (3 rows `LIVE`, `SMOKE-PASS`; nothing in it is a result),
then stopped on the NOGO and released both locks; the VM was left with 0
`raptorpath` and 0 `rp-*` namespaces. Changes, in `277bd0b`: the module is
renamed `runtime_obs` (and its routing test
`tests/runtime_obs_reachability.rs`); the lexical ban is kept as is; no
other code change. What they amend here: every `rtobs` above reads
`runtime_obs`; the P0 binary is built from this amendment's commit (was
`dc38801`); the run is a fresh session (fresh run root `thrp0b` and fresh
target dirs for both binaries), the 5 h cap counted from its own lock
acquisition. Nothing else changes.

**Result** (scored 2026-10-05 against this pre-registration and amendment
A1, literally): **D3 `D3-REFUTED-WITH-RECORD`** at `c1s-400` (server
receiver task `[RDIAG] busy` median 79.0 %, hottest server thread 0.355
core; both below their thresholds). The stop rule did NOT fire: P2's server
half does not become the `c1s` gate on this evidence. **No-behaviour-change
check `REFUTED-WITH-RECORD`**: at `c1s-400` P0's goodput median is 6.7 %
below MAIN's with disjoint ranges (n = 3 each); every other clause is
within or underpowered. Nothing is flipped.

*Binary and session.* Session 2 (A1): P0 from `f718009` (engine
`277bd0b`), `sha256 047a2270f2726f144ddef46a296b26942fb25f8e0b11ec24a5ae60ca58eeb4ed`;
MAIN from `8d7d8c1`, `sha256 5dccf33bf647f3856a4041085822af0aab8125d7cd9e9302d18d2e61ce763d12`;
both archived LF and built fresh on the benchmark VM (Xeon E5-2650 v3 era,
6 vCPU) in run root `thrp0b` with fresh target dirs. The lock waiter
started 16:45Z and waited for another session's locks; locks held
17:44:48Z–18:20:46Z (**36 min** of the 5 h cap; session 1 used 34 min,
A1). Build 4.3 min, tests 25 min, main's build 4.4 min, smoke 28 s, AUTO-GO
18:18:52Z, battery 114 s (12 invocations). VM left quiet: 0 `raptorpath`,
0 `rp-*` namespaces, both locks released by the envelope.

*Tests (VM, release, session 2).* `cargo test -p raptorpath -p
raptorpath-math --release --no-fail-fast -- --test-threads=2` (90
binaries): **1 058 passed, 0 failed**, 50 ignored (all six `runtime_obs`
unit tests and `the_thr_and_lag_lines_fire_on_both_perf_ends` ok); `cargo
test --doc` rc 0; `cargo test -p raptorpath-wasm` 35 passed
(`GOLDEN_CAPTURE` unset); python parser tests 88 / 31 / 45 checks, 0 failed.
Windows: the VM has no `x86_64-pc-windows-gnu` target, so the envelope's
check did not run; the tree was checked on the Windows host instead (native
MSVC target, `cargo check -p raptorpath --bin raptorpath --test
rtobs_reachability` and `--lib --profile test`, before the rename; clean;
and after it, on `d465adc`, `--bin raptorpath --test
runtime_obs_reachability`: clean).

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (session 2 took both at 17:44:48Z after waiting on a foreign holder) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script) |
| `ABORT-BUILD` | no (both binaries) |
| `ABORT-TESTS` | session 1 yes (A1); session 2 no |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 3 rows `LIVE`, both arms |
| `ABORT-RC` | 0 of 12 |
| `ABORT-BRINGUP` | 0 (0 `NO_DATA`) |
| `VOID-COTENANT` | 0 of 12 |

*What ran.* 3 blocks × 4 = 12 rows, all `LIVE` (every witness on every row:
`[GATES] RWM_EMIT_BATCH=1` and `RWM_RDIAG=1` on both ends; in-transfer
`[RDIAG]` on the server; P0 rows the `[THR] rt/os/sum` and `[LAG]` window
lines on both ends with an `rp-w-*` thread; MAIN rows no `[THR]`/`[LAG]`).
The server's `phase=run` lines were present in 6/6 P0 rows (the graceful
path works). n = 3 per arm and cell (seeds 42, 7, 42). MAIN is `IN-BAND`
against §10's control band at both cells. Ledgers:
`docs/l1-raw/thread-p0/` (`p0.log` sha256 eea18176…, `score.txt` with
every per-rep value, `TESTS.txt`, `BINSHA.txt`, smoke, `all-era.txt`,
session 1's `TESTS`/era/NOGO, and the full `[THR]`/`[LAG]`/`[RDIAG]` lines
of one c1s and one c1d P0 row); the per-invocation endpoint logs stay on
the VM under `thrp0b/run/diag-p0`.

*Goodput and CPU* (median [min–max], n = 3; §5 MDE):

| cell | goodput P0 / MAIN (Mbit/s) | Δ | CPUCLI s P0 / MAIN | Δ | CPUSRV s P0 / MAIN | µs CPU per datagram P0 / MAIN |
|---|---|---|---|---|---|---|
| `c1s-400` | 483.2 [461.7–491.2] / 518.2 [493.3–518.5] | **−6.7 % (MDE 4.9 %), ranges disjoint: MOVED** | 5.98 [5.88–6.04] / 5.79 [5.76–5.99] | +3.3 % (MDE 2.4 %), ranges overlap: UNDERPOWERED | 8.70 / 8.14 | 17.57 / 17.01 |
| `c1d-400` | 411.3 [392.7–424.0] / 413.2 [396.1–413.4] | −0.4 %: within | 11.56 / 11.51 | +0.4 %: within | 12.59 / 12.45 | 33.25 / 33.12 |

Against the §10 NEW numbers the brief names (reported, not scored): P0
`c1s-400` −5.3 % goodput, +4.4 % `CPUCLI`; `c1d-400` −3.1 %, +3.3 %.

*D3* (P0 rows at `c1s-400`; per rep s42 r1 / s7 r1 / s42 r2):

| reading | per rep | median | threshold | |
|---|---|---|---|---|
| server receiver task `[RDIAG] busy` (in-transfer lines) | 79.0 / 79.0 / 80.0 % | 79.0 % | ≥ 90 % | below |
| server `msg_tx` depth `q_avg` | 471 / 516 / 500 | 500 | — | — |
| hottest server OS thread (`cores`) | 0.273 / 0.355 / 0.381 | 0.355 | ≥ 0.90 | below |

**Per-thread core budget** (P0 rows, the object's own window ≈ 6.6 s at
c1s, ≈ 7.8 s at c1d; median [min–max], n = 3; `rp-w-*` are the six tokio
workers, `raptorpath` the `block_on` main thread that runs the perf
generator/sink):

| | c1s client | c1s server | c1d client | c1d server |
|---|---|---|---|---|
| hottest thread | 0.173 [0.162–0.175] | 0.355 [0.273–0.381] | 0.246 [0.232–0.264] | 0.301 [0.277–0.312] |
| 2nd / 3rd thread | 0.160 / 0.158 | 0.263 / 0.206 | 0.244 / 0.227 | 0.268 / 0.247 |
| main thread (`raptorpath`) | 0.109 | 0.175 | 0.123 | 0.137 |
| all workers (`rp-w-*`) | 0.771 | 1.112 | 1.355 | 1.472 |
| process total | 0.879 [0.856–0.918] | 1.287 [1.280–1.305] | 1.478 | 1.609 |
| worker `busy_frac`, ranked 1…6 | .20 .19 .18 .16 .13 .05 | .39 .29 .23 .16 .09 .05 | .35 .34 .32 .32 .28 .24 | .36 .31 .28 .27 .26 .22 |
| worker parks / park-unpark events per s | 8 448 / 16 896 | 8 544 / 17 089 | 22 520 / 45 039 | 19 375 / 38 750 |
| `[LAG]` p50 / p99 / max (µs) | 1 034 / 2 754 / 5 832 | 917 / 1 954 / 3 188 | 179 / 1 907 / 4 916 | 961 / 2 107 / 7 363 |
| `[RDIAG] busy` (its receiver task) | 10 % | 79 % | 19 % | **92 %** |

Per rep the hottest thread was a different worker each time (client c1s
`rp-w-5`, `rp-w-2`, `rp-w-5`; server c1s `rp-w-4`, `rp-w-2`, `rp-w-1`); the
full per-thread lists are in `score.txt`.

**Predictions** (checks): D3 borderline — server `[RDIAG] busy` 75–95 %
`MET` (79 %); hottest server thread 0.6–0.9 core `MISSED` (0.355);
`NO-CHANGE-HELD` at both cells `MISSED-AT-c1s-400`; 6 workers per side
`MET`; the client's sender-hosting worker 0.35–0.6 core `MISSED` (no client
thread above 0.175 at c1s).

*Outside the pre-registered set (findings, no verdict).*

1. **At `c1d-400` the server's receiver task is at the 90 % line**: busy
   92 % in all three reps (`q_avg` 227–390), while at `c1s-400`, the cell
   the question named, it is 79 %. On this reading the single receiver
   task is closer to being the wall on the dual cell than on the single
   one. The D3 stop rule names `c1s` only and does not fire; whether P2's
   server half should gate `c1d` is a question for P2's pre-registration.
2. **No thread is hot, on either side, at either cell: the stock runtime
   spreads the work evenly over all six workers.** At c1s the client's
   ≈ 0.88 core is spread at ≤ 0.175 per thread over five workers, and the
   server's 1.29 cores at ≤ 0.38 per thread; the hottest worker changes
   from rep to rep. Tasks migrate between workers (D7), so per-OS-thread
   CPU cannot localise a task under the stock runtime; the task-level
   reading (`[RDIAG]`) is the one that answers per-task questions until
   tasks are pinned to threads (P2).
3. **Wake churn, measured with stable metrics**: 8.4–8.5 k worker parks per
   second per side at c1s, 19–23 k at c1d (`worker_park_unpark_count` is
   ≈ 2 × `worker_park_count`, so it counts both transitions). This is the
   16–39 k sleeps/s order the off-CPU captures showed.
4. **tokio's busy time exceeds the OS CPU on the client** (c1s window:
   workers' `busy_s` sum 6.32 s against 5.98 s of process CPU): busy
   duration includes time a worker spends blocked inside a poll (quinn's
   connection mutex, `parking_lot` waits), which the OS does not charge.
5. **No starvation beyond a few ms**: `[LAG]` p99 1.9–2.8 ms and max
   3.2–8.7 ms everywhere. The stated instrument floor (p50 ≈ 0.5–1 ms) held
   except on the c1d client (p50 179 µs). A hypothesis, not measured:
   there workers are rarely parked and the timer is serviced by busy
   workers' maintenance ticks rather than by a parked worker's 1 ms-rounded
   sleep. The floor statement is therefore an upper description, not a
   bound.
6. **The no-behaviour-change refutation is not localised.** It rests on
   n = 3 per arm with the ranges 2.1 Mbit/s apart (P0 max 491.2 against
   MAIN min 493.3); the P0 c1s CPU per datagram is +3.3 % (overlapping
   ranges) and the server's `CPUSRV` +6.9 % (8.14 → 8.70 s, overlapping
   ranges; unscored) — the larger move is on the side that reads `/proc`
   at the object's first packet; at c1d nothing moved. Which declared known effect (the 100 Hz
   lag probe; the two `/proc` reads on the perf main thread per object,
   the server's at the object's first packet; the thread names) would
   cost ≈ 3–7 % at c1s only is not measured here. Consequence for P2: every
   arm of the P2 battery must carry the same instrument (the plan's arms
   all print `[THR]`/`[LAG]`), so this offset is common to the arms and is
   not attributed to the topology; a comparison against an
   instrument-free binary must name it.

*What it means.* The ≈ 500 Mbit/s single-path ceiling is not a single
saturated server receiver task and not any single saturated server
thread on this evidence: the receiver task is 79 % busy (lock waits
included) and no thread exceeds 0.4 core, while the
server process uses 1.29 cores spread over six workers that park ≈ 8.5 k
times per second. The other server tasks (quinn's ConnectionDriver and
EndpointDriver, the datagram reader) were not measured per task, and
because tasks migrate (finding 2) the flat per-thread profile cannot bound
them: one of them at ≥ 0.9 core spread over six workers would look the
same. The dual cell is different: there the receiver task is
at 92 %. The per-thread budget the redesign needs is above; its main
lesson is that under the stock scheduler every hot task wanders across all
six workers, so the owned-path layout's first observable is that the work
concentrates on the named owner threads. The instrument itself carried a
c1s goodput cost that the pre-registered check refutes as "no change"; P2
must compare instrumented arms only.
