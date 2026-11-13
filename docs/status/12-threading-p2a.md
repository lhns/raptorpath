## 12. Threading P2a — the logic actor — pre-registration

Phase P2a of the threading plan (owned paths, batched rings, no shared hot
locks): **one task owns the scheduler and the FEC controller.** Committed
before any VM contact of this battery (the dev builds and the red/green
test runs on the VM preceded it; they are tests, not results). No number
below is a result. Nothing is flipped by this battery; shipping P2a is the
operator's merge.

**What P2a changes** (branch from `main` e655484 = P0 + P1; engine commits
`711cfc0` step 0, `133adb9` the actor, `06837b7` the bounded yield;
exploration `73694a1`; harness `0ded0f1`). No wire change, no law change, no new gate on the
data path, the same structure at every path count and every (δ, ρ):

1. **Step 0 — the instrument is opt-in.** `RWM_RTOBS` (default off, echoed
   on `[GATES]`, in `RWM_FORWARD`): unset, `runtime_obs::arm` is a no-op —
   no 100 Hz `[LAG]` probe task, no `[THR]` `/proc` reads, no lines (§9
   finding 6: the always-on instrument cost c1s goodput −6.7 %). Thread
   names stay. Instrumentation gating, not a δ/ρ mode. Routing test
   `tests/runtime_obs_reachability.rs`, two-sided (lines + `RWM_RTOBS=1` on
   both perf ends with it set; zero `[THR]`/`[LAG]` lines and `RWM_RTOBS=0`
   on both ends without it).
2. **The logic actor** (`src/actor.rs`; exploration and classification in
   `docs/thread-p2-scheduler-access.md`: 77 scheduler + 5 FEC-controller
   sites in 5 tasks; 71 + 5 class (a), 5 class (b), 0 class (c), 1 class
   (d)). ONE tokio task (on the current multi-thread runtime) owns the
   `Scheduler` and the `FecRateController` in `ActorCell`s — no mutex: a
   borrow never blocks, an off-owner borrow panics, the guard is `!Send`
   so a borrow across an `.await` does not compile — and runs the window
   sender, the receiver role (decode → deliver → ack state; on the client
   the inbound `WindowAck` handling), the 2 s report, the control fast path
   and the path-command processor as five sub-futures with per-sub-future
   wakers. The parent task is woken only when the actor is not already in
   its poll loop (the Cats Effect `notifyParked` rule); a receiver → sender
   wake (`AckWake`) inside the actor is served in the same poll. The poll
   loop runs while tokio's coop budget remains and for at most one pass
   per sub-future (a resource bound against a self-waking sub-future, not
   a law). P1's lock-order witness is kept on the new cell (no quinn call
   with the scheduler borrowed).
3. **Acks as batches.** The receiver role drains the inbound channel with
   `recv_many` up to its own depth (`MSG_CHANNEL_DEPTH` = 4096, ADR-0011's
   bound; no new constant); one wake of the parked actor per batch. The
   per-message body is unchanged; the hold/deficit deadlines (and their
   `[QCLK]` sample) are evaluated when the receiver is about to wait, not
   per message of a batch. One cooperative-budget unit per buffered
   message (`coop::consume_budget`), so the yield cadence inside a batch is
   main's (one `recv()` per message) — the bounded auto-yield the plan
   takes from Cats Effect.
4. **The server's receiver role is the same actor**, not a sibling: its
   per-datagram writes (`touch_path`, `record_arrival`,
   `record_incoming_loss`) land in the `PathState` estimators the sender
   role reads; a sibling would need a per-datagram message or a `PathState`
   rx/tx split (P2b/P3). P3's receiver split is not in scope.
5. **perf on a worker (D9).** The perf client/server body runs as a task on
   a runtime worker, not on the `block_on` main thread, so the generator /
   sink ↔ engine hop is task-to-task (production TUN's shape, D10).
6. **No measurement arm.** The cell replaces the mutex at the type level in
   every signature; an `RWM_TOPO` arm would duplicate the machine. The
   comparison is P2a's binary against main's binary, interleaved.

**Red / green (dev, VM).** Red by construction on `main` e655484 (the
types do not exist there): `p2a_an_ack_batch_wakes_the_actor_with_zero_timer_advance`
(paused clock: three WindowAcks queued at once wake the parked actor
exactly once and end the sender's paused wait with the clock unmoved), its
control `p2a_control_a_non_ack_batch_leaves_the_sender_to_its_timer` (a
PathReport batch: the 1 ms poll fires, the clock advances — the harness
can see a failure), `p2a_no_mutex_wraps_the_scheduler_or_the_fec_controller`
(source scan: the same patterns over `main` e655484's non-test `src/`
read 28 lines — `SchedMutex`, `Mutex<FecRateController>`),
the `actor::tests` (owner check, re-borrow, the one-parent-wake rule), the
`ActorCell` `compile_fail` doctest (guard across an await), and the
`RWM_RTOBS` off side of `runtime_obs_reachability` (run against `main`
e655484's binary on the VM: both of its tests FAIL, each at its first
assertion — no `RWM_RTOBS` echo on `[GATES]`). Green (dev, VM, before this
commit): the release suite on the actor tree `133adb9` 1 078 passed, 0
failed, 51 ignored (92 binaries), doc + wasm 37 passed; the debug suite on
`06837b7` (the witness and the `ActorCell` owner checks compiled in) 941
passed, 0 failed, 49 ignored (84 binaries), 0 `lock order: quinn seam`
and 0 `ActorCell` panics in the captured output; the Windows host `cargo
check -p raptorpath --tests --bin raptorpath` clean. The merge base itself
(`main` e655484, never tested as merged) was run first: release suite +
doc + wasm 1 103 passed, 0 failed, 51 ignored (95 binaries) — no merge
breakage. Adapted because they test the removed mechanism (said here):
`net::tests::c1_attribution_lock_blocking_bench` (`#[ignore]`d; two OS
threads sharing the scheduler mutex — now a local `parking_lot` mutex, it
measures the removed mechanism); `s10_*` and `p1_d1_*` (a `SchedCell` in
place of `Arc<SchedMutex>`); `sched_lock`'s witness unit test (the same
assertion on the cell). `lock_order_loopback`, `ack_wake_loopback`,
`reliable_delivery_hold`, the loss-truth (`s10_*`), store-gate and SACK
suites are unchanged.

**Mechanism and predictions** (rule 11; derivation, then the named
refutation risks).

- *What is removed.* Per datagram, ≈ 6–8 scheduler-mutex acquisitions
  that could contend across workers (th2 §4; P1 measured the residual
  sender lock sleeps at ≈ 1 % of wall under cadence), the cross-task
  ack → sender hop (now intra-task), and the main-thread futex on the perf
  generator/sink hop (D9: the main thread carried 0.11–0.18 core in §9).
- *What is added.* Everything that touched the scheduler now shares ONE
  task: the client's ack handling runs between the sender's polls, never
  in parallel with them, and while the sender waits on quinn's connection
  mutex (a blocking wait inside its poll) the whole actor waits.
- **c1s**: client sender busy ≈ 36 % (§11) + ack role ≈ 10 % (§9 `[RDIAG]`)
  ≪ 1 core; server receiver ≈ 79 % with the server's sender role idle.
  Predicted **SAME** (goodput WITHIN; CPU WITHIN or TREND-BETTER).
- **c1d — the named refutation risk.** Client sender busy 95 % (§11) of
  which 0.226 of wall is asleep in quinn (§10), + the ack role 19 % (§9):
  the sum exceeds one core-equivalent, so the actor is predicted to be the
  client's bottleneck: goodput and `CPUCLI` are the clauses most at risk;
  **WORSE at c1d is a possible outcome of this design and is pre-stated
  here, not explained after.** The server receiver at 92 % (§9 finding 1)
  gains no work.
- **c2 / c8** (ack-clocked, many ack wakes per intake wake, §11 finding 1):
  an ack batch is now one actor poll and at most one sender re-evaluation
  instead of one sender loop body per ack; predicted `CPUCLI` WITHIN or
  TREND-BETTER at c2.
- **RTprop floor** (risk at c1d and c2): the floor is the minimum app-echo
  RTT; an ack that waits for the sender's poll to end is processed later,
  and the ConnectionDriver woken from the actor is still deferred to the
  actor's yield (D8, the FDT precedent: +5.6 % at c1d, +11 % at c2).
- **`[LAG]` p99** (risk): longer actor polls delay the probe task on the
  same worker.

**Known effects, declared** (what the battery measures besides the
mechanism): (1) the receiver's hold/deficit deadlines and their `[QCLK]`
sample are evaluated once per wait, not once per message of a batch
(`[QCLK]` sample counts change; diagnostic only); (2) `[RDIAG] busy` now
counts time spent in sibling sub-futures as idle (the probe is not used
here); (3) the perf body runs on a worker (D9); (4) one tokio coop budget
(128 units per poll) is shared by the five sub-futures, where each task
had its own; (5) `[DIAG]`'s sender `busy` share is unchanged in meaning
(the sender sub-future's own loop body).

**Binaries.** P2A = this section's commit (archived with `git -c
core.autocrlf=false -c core.eol=lf archive`, built fresh on the benchmark
VM in a fresh target dir); MAIN = `main` e655484, archived and built the
same way in its own fresh target dir. Both copied under the real name
`raptorpath`; `sha256` in `BINSHA.txt`, re-verified before every
invocation. MAIN ignores `RWM_RTOBS` (its instrument is always on), so
"MAIN with `RWM_RTOBS=1`" is MAIN; both arms carry the same instrument
(§9 finding 6's offset is common to the arms).

**Harness.** Envelope `tools/l1/threadp2a_run_all.sh` (both locks for the
whole session via `lib_battery.sh`; build → tests → MAIN build → smoke →
budget → battery → ack-cadence block → score; hard backstop), driver
`threadp2a_battery.sh`, scorer `threadp2a_parse.py` (rows by
`stage3_parse.make_row`, helpers from `threadp1_parse`; offline test
`test_threadp2a_parse.py`). No operator GO gate: SMOKE-PASS proceeds.

**Tests first** (inside the locks, on the P2A tree): `cargo build
--release`; `cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`; `cargo test --doc -p raptorpath
--release`; `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE` unset); the
debug run `cargo test -p raptorpath --no-fail-fast -- --test-threads=2`
(the lock-order witness and the `ActorCell` owner checks compiled in; the
counts of `lock order: quinn seam` and `ActorCell` panics are recorded and
must be 0); the python parser tests (`test_l1common.py`,
`test_stage3_parse.py`, `test_threadp2a_parse.py`). A failure that passes
on an immediate solo re-run is `FLAKE`; any other is `ABORT-TESTS`.

**Cells** (§5's geometry, size, capacity and > 5 % headroom, as §11):
`c1s-400`, `c1d-400`, `c2-100`, `c8-100`. **Arms**: `MAIN` and `P2A`,
both bulk, `--window-reliable`, the shipped defaults (every arm `env -u
RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS
RWM_ACKDIAG`, then `RWM_RTOBS=1` on both; rule 15d), `perf_rwm_c.sh` with
`RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, a
fresh topology. **Plan per (rep, seed) block: 8 invocations**, cells in the
order above, arm order within a cell rotated by the block index (rule 3).
Seeds 42 and 7; blocks rep 1 s42, rep 1 s7, rep 2 s42, …; **n = 3 per seed
(6 per arm and cell)**, cut only by the budget rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`):
`stage3_parse`'s set (`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]`
both; the RLC line both; no generation guard; cadence ACTIVE both;
`RWM_POOL_ANCHOR=0` both); `[GATES] RWM_EMIT_BATCH=1` both and the
`emission batching ACTIVE` echo on the client; the client's `[DIAG]`
`wake[` token (both arms); **the P2a execution witness, two-sided**: the
`[TOPO] logic actor` echo on BOTH endpoints of every P2A row and on
NEITHER endpoint of a MAIN row, and `[GATES] RWM_RTOBS=1` on both P2A
endpoints; the `[THR]`/`[LAG]` window of the measured object on both ends
of both arms (client `run=1`, server `obj=1`); the row's `sha256` is its
arm's binary.

**Scored clauses per cell**, P2A against MAIN, with §11's min–max rule
(**WORSE** iff P2A's median is beyond MAIN's median·(1 ∓ rel) in the worse
direction **and** the two arms' [min, max] ranges are disjoint in that
direction; **TREND-WORSE** beyond the band with overlapping ranges —
reported and named, not a fail; **BETTER** / **TREND-BETTER**
symmetrically; **WITHIN** otherwise):

| clause | direction | rel |
|---|---|---|
| goodput | higher is better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| `CPUCLI` per GB | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| `CPUSRV` per GB | lower | the same, as a declared transfer (§11) |
| RTprop floor, per leg (min non-zero `rtp_us` of the client's `[DIAG]`) | lower | **max(5 %, MAIN's half-range / MAIN's median)** (§11's rule; no committed MDE exists) |
| `[LAG]` p99, client and server (the object's window) | lower | **the same rule, max(5 %, MAIN's half-range / median)** — relative, because the VM is shared and oversubscribed at c8 |
| fed loss | — | per leg med(`plc`/`[TRUTH]`) under P2A within [1/1.3, 1.3] × MAIN's (§11) |
| DNF | — | excess > 0.20 (§5) |

**Per-cell verdict.** **WORSE** iff any clause is WORSE or the feed moved
or the DNF excess fired; **BETTER** iff not WORSE and goodput or a CPU
clause is BETTER; **SAME** otherwise; **UNSCOREABLE** at a cell where
either arm has < 3 live rows or ≥ 2 witness-failed rows, a feed ratio is
unread, or an abort cause fired.

**Outcomes, in precedence order** (one verdict):
1. `UNSCOREABLE` — an abort cause fired.
2. `REFUTED-WITH-RECORD (WORSE-AT-<cells>)` — WORSE at any scoreable cell:
   P2a does not ship; the per-cell record and the reported columns below
   (the `[THR]` per-thread budget first) are the diagnosis. **No tuning in
   this battery.**
3. `UNSCOREABLE-AT-<cells>` — a hard blocker at a cell, none WORSE.
4. `DELIVERED (BETTER-AT-<cells>)` or `DELIVERED (SAME everywhere)` — no
   cell WORSE, every cell scoreable, every witness held: P2a ships (the
   merge is the operator's).

**Reported, not gated** (printed per cell and arm, median [min–max], n):
acks per data datagram (the server's last `[CTLD]`: Σ control frames sent /
Σ data frames received); the `[THR]` per-thread budget (process cores, the
three hottest threads with their `comm`, the main thread, per rep the four
hottest threads per side); worker parks per second (`[THR] rt`, Σ park ÷
wall); `[LAG]` p50/p99/max; sender `busy`. **The D9 readout**: the main
thread's (`comm=raptorpath`) cores per side under P2A, predicted ≈ 0 (the
perf body left it), against MAIN's 0.11–0.18 (§9); the battery cannot
separate D9's share of any goodput/CPU change from the actor's (one binary
carries both) — this is the attribution, stated in advance. **The ack
inter-arrival** comes from a separate reported-only block run after the
scored battery if it fits before the soft deadline (tag `ackd`: the same
plan with `RWM_ACKDIAG=1` on both arms, one rep per seed, 16 invocations;
the client's last `[ACKDIAG]` `gap_us[p50 p90 p99 n]` per path); those rows
never enter a clause (the gauge's per-ack lock and clock reads are an
instrument cost the scored rows must not carry). **The c8 fast-leg RTprop
re-check** (§11's TREND-WORSE, P1 vs 8d7d8c1, ranges touching): MAIN here
carries P1, so its c8 p0 floor (n = 6) is printed beside §11's MAIN
8d7d8c1 (8 840 [7 819–9 822] µs) and P1 (10 670 [9 773–10 819] µs) —
cross-session, reported, no verdict.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either tree),
`ABORT-TESTS`, `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (one
invocation per arm at `c1s-400` and `c8-100`, seed 42: every row LIVE with
goodput, both CPU lines, `busy`, `[LAG]` p99 on both ends, and per leg
`[TRUTH]`, `plc` and an RTprop floor — the scored inputs only; both arms
present; nothing in it is a result), `ABORT-BUDGET` (n < 2 after the budget
rule), `ABORT-RC` (that row `VOID-RC`, the battery goes on),
`ABORT-BRINGUP` (no summary after 2 attempts: `NO_DATA`); void class
`VOID-COTENANT` (a `cargo`/`rustc` process before or after an invocation).

**Budget.** Hard backstop = launch + 3 h, soft = hard − 10 min (the task's
≈ 2 h aim; the 5 h cap is not approached). Priors: builds ≈ 9 min, release
tests ≈ 25 min, debug tests ≈ 25 min, `R_PRIOR` = 240 s per 8-invocation
block (§11 measured ≈ 80 s); scored battery ≤ 6 blocks ≈ 8–25 min; the
`ackd` block ≈ 3–8 min. n per seed = min(3, ⌊(soft − now) / (2·R_est)⌋),
`R_est` = `R_PRIOR`·max(1, c_meas/60 s) from the smoke; the battery starts
no block that would cross soft (`TRUNCATED-AT-REP-BOUNDARY`, scored at the
n reached).

**Session rules.** Both locks for the whole session; detached envelope;
earned sentinels (`DONE-ALL` only with `TP2-BATTERY-DONE`, `check` rc 0 and
no truncation); the operator reads `all-era.txt` at most once per ≈ 20 min
(rule 13, recorded); `pkill -x raptorpath` only; no `ens18`, firewall,
`sshd` or non-`rp-*` namespace is touched; exit state verified (0
`raptorpath`, 0 `rp-*` namespaces, both locks released). Ledgers are copied
to `docs/l1-raw/thread-p2a/`.

### 12. Threading P2a — result

Scored 2026-10-05 against the pre-registration above, literally; no
amendment was made. **`REFUTED-WITH-RECORD (WORSE-AT-c1s-400)`**: at
`c1s-400` goodput −33.0 %, `CPUCLI` per GB +86.7 % and `CPUSRV` per GB
+54.8 %, every one with disjoint ranges (n = 6 each). `c1d-400`, `c2-100`
and `c8-100` are `SAME`. **P2a does not ship.** Nothing was tuned in this
battery.

*Binary and session.* P2A = `75d3ab4` (engine tree `06837b7`), `sha256
5e74772c…c45a37`; MAIN = `e655484`, `sha256 c1e99422…f006c66c`; both
archived LF and built fresh on the benchmark VM (Xeon E5-2650 v3 era) in
fresh target directories (`BINSHA.txt`). Launch 20:18:08Z (hard
23:18:08Z); P2A build 254 s; tests 20:22–21:05Z; MAIN build 257 s; smoke
21:12:57Z (`c_meas` 43 s < `C_PRED` 60 s, so `R_est` = `R_PRIOR`; n = 3
per seed, no cut); battery 21:12:57–21:21:31Z (514 s, 48 invocations);
the `ackd` block 164 s (16 invocations); locks released 21:24:16Z.
**Session wall 1 h 06 min.** Exit state recorded by the envelope: 0
`raptorpath`, 0 `rp-*` namespaces, both locks released. The operator read
`all-era.txt` at 20:38, 20:58, 21:18 and 21:39Z (rule 13's ≈ 20 min).

*Tests* (inside the locks, `TESTS.txt`). Release suite rc 0, **1 078
passed, 0 failed**, 51 ignored (92 binaries); doc rc 0 (2 passed); wasm rc
0 (35 passed); **debug suite rc 0, 939 passed, 0 failed**, 49 ignored (83
binaries); parser tests rc 0; `LOCKORDER-PANICS 0`, `ACTORCELL-PANICS 0`.
No flake fired.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 20:18:08Z) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script) |
| `ABORT-BUILD` | no (either tree) |
| `ABORT-TESTS` | no (0 failures) |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 4 rows LIVE, `[TOPO]` on both P2A ends and neither MAIN end |
| `ABORT-BUDGET` | no (n = 3 per seed) |
| `ABORT-RC` | 0 of 48 |
| `ABORT-BRINGUP` | 0 (0 `RUN-RETRY`) |
| `VOID-COTENANT` | 0 of 48 |

*What ran.* 6 blocks × 8 = 48 rows, all `LIVE` (0 witness failures, 0
contaminated, 0 DNF); 6 per (cell, arm), 3 per seed; the `ackd` block 16
rows, all LIVE (reported only). Ledgers: `docs/l1-raw/thread-p2a/`
(`tp2.log` sha256 3a0dc219…, `score.txt` 24b4e47f… with every per-rep
value, the per-thread lists and both seeds' medians, `ackd.log` 30c56186…,
`TESTS.txt`, `smoke.log`, `smoke-check.txt`, `PLAN.txt`, `BINSHA.txt`,
`all-era.txt`); the per-invocation endpoint logs stay on the VM under
`/home/vibe/p2arun/run/diag-tp2`.

*Per cell* (P2A vs MAIN, n = 6 each; median [min–max]; relative tolerance
in brackets; the min–max rule as pre-registered):

| cell | goodput Mbit/s MAIN → P2A | CPUCLI s/GB | CPUSRV s/GB | RTprop floor µs per leg | `[LAG]` p99 µs cli / srv | plc/truth per leg MAIN → P2A | verdict |
|---|---|---|---|---|---|---|---|
| `c1s-400` | 539.9 [478.7–555.9] → 362.0 [144.3–403.0] (−33.0 %, **WORSE**, 4.9 %) | 14.21 → 26.54 (+86.7 %, **WORSE**, 2.4 %) | 19.90 → 30.80 (+54.8 %, **WORSE**) | 2 325 → 2 361 (WITHIN) | 1 860 → 1 546 / 1 630 → 1 167 (WITHIN) | 1.027 → 1.034 | **WORSE** |
| `c1d-400` | 430.5 [405.3–443.8] → 401.4 [371.3–416.7] (−6.8 %, TREND-WORSE, 5.6 %) | 27.34 → 28.10 (+2.8 %, WITHIN) | 29.48 → 32.62 (+10.7 %, TREND-WORSE) | 2 252 → 2 244; 2 277 → 2 308 (WITHIN) | 2 116 → 2 262 / 2 210 → 2 458 (WITHIN) | 1.009 → 1.003; 1.016 → 1.014 | **SAME** |
| `c2-100` | 89.24 → 89.41 (+0.2 %, WITHIN, 1.4 %) | 34.6 [33.6–35.4] → 37.1 [33.9–40.7] (+7.2 %, TREND-WORSE, 6.3 %) | 47.8 → 45.6 (WITHIN) | 12 310 → 12 020 (WITHIN, tol 15.1 %) | 1 444 → 1 490 / 1 698 → 1 564 (WITHIN) | 1.002 → 0.990 | **SAME** |
| `c8-100` | 100.7 → 102.5 (+1.8 %, WITHIN, 4.0 %) | 53.75 → 56.55 (+5.2 %, WITHIN) | 59.8 → 60.2 (WITHIN) | p0 10 520 → 10 060; p1 40 920 → 39 920 (WITHIN) | 2 009 → 1 472 / 1 559 → 1 494 (WITHIN) | 1.005 → 0.999; 1.006 → 0.993 | **SAME** |

**Predictions** (checks): c1s `SAME` — **MISSED** (WORSE); the named c1d
refutation risk — did **not** fire as WORSE (goodput and `CPUSRV`
TREND-WORSE, ranges overlapping); c2 `CPUCLI` WITHIN or TREND-BETTER —
**MISSED** (TREND-WORSE, +7.2 %); the RTprop and `[LAG]` p99 risks — did
not fire (WITHIN at every cell and leg); the D9 readout — **MET**: the main
thread (`comm=raptorpath`) reads 0.000 core on both sides of every P2A row
(MAIN: client 0.115, server 0.180 at c1s; 0.122 / 0.130 at c1d).

*Reported, not gated* (`score.txt` has every value):

- **Acks per data datagram** unchanged: 0.997–1.000 in both arms at every
  cell (one WindowAck per data datagram; the batching is on the consumer
  side, the wire cadence is the server's).
- **Worker parks per second** (Σ `[THR] rt` park ÷ wall): c1s client
  8 800 → 24 306, server 9 631 → 26 406 (×2.7); c1d client 22 570 → 24 922,
  server 18 880 → 27 934; c2 client 6 121 → 8 067 (+32 %), server
  unchanged; c8 unchanged on both sides.
- **Per-thread budget** (process cores, median): c1s client 0.931 → 1.194,
  server 1.327 → 1.372 — more CPU for 33 % fewer bytes; the hottest thread
  stays ≤ 0.45 core in every P2A row (the actor still wanders across the
  six workers: no per-thread concentration).
- **Ack inter-arrival** (`ackd` block, client `[ACKDIAG]` `gap_us`, n = 2
  per arm): c1s p50 2 → 1 µs, p99 554 → 494 µs; c1d p99 885/806 →
  1 257/1 547 µs; c2 p90 112 → 264 µs; c8 slow leg p50 44 → 10 µs.
- **The c8 fast-leg RTprop re-check** (§11's TREND-WORSE): MAIN here (which
  carries P1) reads p0 10 522 [5 150–10 689] µs, at P1's §11 level (10 670
  [9 773–10 819]) rather than 8d7d8c1's (8 840 [7 819–9 822]) —
  cross-session, so consistent with, not proof of, P1 having raised the
  floor; P2A 10 064 [5 999–12 061]. No verdict.

*Outside the pre-registered set (findings, no verdict).*

1. **At c1s the sender loop iterates ≈ 3× more often under P2A on the
   ordinary rows, up to 11× on the collapsed ones.** From
   the rows' cumulative `wake[..]` tokens (client `[DIAG]`, both arms):
   `wake[tun]` 7 414–8 150 (MAIN) against 23 391–28 281 on the four
   ordinary P2A rows and 47 807 / 82 634 on the two collapsed rows (144 and
   237 Mbit/s); `wake[ack]` 8 504–10 193 against 43 342–53 224 and 89 575 /
   154 078. The more loop iterations a row has, the lower its goodput.
   **[H]** The actor serves each small ack batch and the sender's
   re-evaluation in one poll, and its polls are cut short more often (one
   coop budget shared by five sub-futures; the per-message budget unit in
   the receiver), so the sender is re-entered per ack batch and emits a few
   symbols per entry: per-iteration overhead multiplies and the emission
   bursts (and GSO batches) shrink — an ack-clocked trickle. The ×2.7 worker
   parks per second on both sides fit the same picture (every yield of the
   actor re-queues it and notifies a parked worker). Not measured
   directly: the burst sizes per iteration and the actor's yield count.
   **A named confound.** The per-message budget unit
   (`coop::consume_budget` at the receiver's loop top, `06837b7`) was added
   after the actor design and before the pre-registration, to bound the
   batch's hold on the worker; it shortens the actor's polls by itself. One
   binary carries both it and the actor, so this battery cannot separate
   them: what is refuted is **P2a as built** (`06837b7`), not the
   one-task ownership on its own. Isolating the budget unit (the tree with
   and without it) is the first question for whatever comes next; it is
   recorded here, not acted on.
2. **c1d did not saturate as predicted** (sender `busy` 97 % → 70 %): the
   client actor carried the sender and the ack role without reaching
   one core-equivalent of busy; goodput and server CPU trended worse
   within overlapping ranges.
3. **c2's client CPU moved up again** (+7.2 %, TREND-WORSE), on top of
   §11's +5.3 % for P1 at the same cell: batching the acks did not buy back
   the per-ack wake cost.

*What it means.* Owning the scheduler in one task removes the mutex and
makes the ack wake an intra-task wake, as built — the witnesses held, the
ack batch wakes the actor once, the main-thread hop is gone, and the
latency clauses (RTprop floor, `[LAG]` p99) are unchanged everywhere. But
on the cell where the sender is lightly loaded and ack-clocked at
≈ 50 k acks/s (c1s), P2a as built (`06837b7`, the actor plus the
receiver's per-message budget unit) turned the sender into a
per-ack-batch loop: ≈ 3× (up to 11×) more sender iterations, ×2.7 worker
parks, +87 % client CPU per byte, −33 % goodput. The pre-stated outcome
rule applies: P2a does not ship, the result is recorded, and no tuning was
done in this battery. The diagnosis the next step starts from is finding 1
(the sender's per-entry work under ack-batch wakes, the actor's yield
cadence, and the named budget-unit confound that this battery cannot
separate from the ownership change), together with the per-thread budget above, which
shows the work still spread over all six workers (P2b's owned-thread
placement is untested).

### 12. Threading P2a — diagnosis addendum (code read, after the result; no new measurement)

The refutation is a defect of the implementation shape, not of single ownership. P2a is deleted, not patched; the code is kept at tag `archive/thread-p2a`.

**A. Coop-budget starvation: the numeric cause.**
- tokio's cooperative budget (128 units) is per task. `ActorSet` ran five loops in one task, so they shared one budget.
- `06837b7` charged one unit per buffered inbound message, so a single ack batch exhausted the budget.
- With the budget exhausted, every tokio resource returns Pending. `mpsc` recv enters through `coop::poll_proceed` (tokio-1.50.0 `sync/mpsc/chan.rs:295`), so `tun.read_packet()` returned Pending while thousands of packets were queued.
- The sender's iteration therefore emitted nothing, and the actor yielded through `wake_by_ref()`, which tokio treats as `yield_now` (back of the queue, deferred to the next driver poll).
- This matches the ledger: sender iterations 3–11×, `wake[tun]` 3–10× for the same data, worker parks 2.7×, and CPU per byte doubled on fixed per-iteration overhead.

**B. Structure: why removing A would not rescue it.**
- Two loops that ran on two cores (the sender, and the ack/receiver task) became one task: one core, one budget, one LIFO slot.
- `ActorSet` is an executor inside a tokio task, competing with the executor it runs on.
- The plan's "one actor owns the scheduler for both roles" forced that shape, and is withdrawn.

**The same class, smaller, in P1 (§11).** `AckWake` wakes the sender loop per ack to do a sliver of work. That is the c2 client CPU +5.3 % (≈ 3.9 ack wakes per intake wake). It is retired in Q2.

**Rules for the redo** (plan v2: Q1 per-path I/O owner, then Q2 scheduler split by direction):
1. One tokio task per concurrent loop.
2. Ownership by direction: the sender owns the TX state and handles acks; the receiver owns the RX state.
3. Batch at the consumer with `recv_many`, one budget unit per call, never per message.
4. Wake coalescing comes from the channel, not a Notify.
5. Exactly one thread ever touches a given `quinn::Connection`.
