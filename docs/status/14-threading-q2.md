## 14. Threading Q2 — the scheduler split by direction — pre-registration

Phase Q2 of threading plan v2 (status §12 diagnosis addendum, rules 1–5;
Q1 shipped the shared placement, §13): **the scheduler is split by
direction — the window sender owns the TX half and the FEC controller by
plain `&mut`, the receiver owns the RX half — and the client's ack handling
runs in the sender.** Committed before any VM contact of this battery (the
dev builds and the red/green test runs on the VM preceded it; they are
tests, not results). No number below is a result. Nothing is flipped by this
battery; shipping is the operator's merge.

**What Q2 changes** (branch from `main` 0ef0e0d; commits `84f6c90` step 0,
`0aa617a` the split, `e0caca7` docs, `54c301b` harness, `b51f531` a test fix). No wire change,
no law change, the same structure at every path count and every (δ, ρ):

0. **Step 0 — the losing Q1 arm deleted** (`84f6c90`): `RWM_IO_RT`, the
   `rp-io-<k>` pool, the K rule, the least-loaded assignment, `[TOPO]`,
   `[LAG] io` / `[THR] io` and the `own`-only tests. The owner (on the main
   runtime), `[IOWN]` and the `drv_on`/`drv_off` probe stay.
1. **Ownership by direction** (rule 2). `net/tx_inputs.rs` `TxCore` holds
   the `Scheduler` (the TX half: placement, charge/release, the RTT / loss /
   rate estimators, Copa, the anchors, liveness) and the
   `FecRateController` by value; the sender task owns it and lends both by
   plain `&mut` to every phase (`SenderCtx`, `StoreCapCtx`, `GenEmitCtx`,
   `ServeGapsCtx`, `AckAdvanceCtx`, `DiagCtx`, `ControlCtx` — no mutex, no
   cell; `SchedMutex` and its guard are deleted). The receiver owns
   `scheduler/rx.rs` `RxScheduler` (the RX half: the RFC 3550 arrival
   jitter and the incoming-loss EWMA, `control::RxEstimator`) as a local.
   The exploration's class (b) sites land in the RX half; the sites that
   fit neither half are reported in `docs/thread-p2-scheduler-access.md`
   §4 (the receiver's reads of SRTT / RTprop / RTT jitter / σ and of
   liveness; the data-arrival `touch_path`; the sender's reads of the RX
   loss and jitter; the FEC controller's receiver feedback) and resolved by
   published atomics (`PathStats::xdir`) and messages, not forced.
2. **The acks go to the sender** (rules 3, 4). Each path's I/O owner routes
   the TX-direction control datagrams (WindowAck, Ack, PathReport, Ping —
   `control_msg::is_tx_control`) of each poll to the sender's input channel
   as one batch (`[IOWN]` `ack_batches` / `ack_dg`); the control fast path
   forwards the stream-borne PathReport / Ping there too. The sender drains
   it with ONE `recv_many` at its loop top (polled once, only when
   non-empty) and in an always-armed `select!` arm (`wake[ack]`), and runs
   `on_window_ack` itself. P1's `AckWake` Notify and its `wake[timer_acked]`
   plumbing are deleted; `timer_acked` now counts a 1 ms paused / pacing
   timer wake that resolved with acks queued.
3. **Cold tasks by message.** The 2 s report tick
   (`SenderCmd::ReportTick`: the send-rate feed, dead-path check, MTU store,
   in-flight expiry and the PathReport build run in the sender), path
   add / remove, the receiver's FEC feedback and its dead-path revival
   nudge reach TX state on a second channel (`wake[cmd]`). The report
   task keeps its stream sends.
4. **One task per loop, one budget unit per drain** (rules 1, 3): five
   loops, five tasks; no `consume_budget`, no multiplexer, no sub-executor
   (grep gates).

**Known effects, declared** (behaviour changes that are not the mechanism):

1. **Cross-direction values are one publication old at worst.** The
   receiver's hold / deficit / refresh clocks and its `[ETA]` reference read
   SRTT / RTprop / RTT jitter / σ as the sender last published them (after
   every input batch it processed); the sender reads the RX loss and jitter
   as the receiver last published them (mirrored at every loop top).
   Through Q1 both read the live values under the mutex.
2. **Liveness is applied by the sender.** A data arrival is stamped by the
   receiver and applied as `touch_path` at the sender's next loop top (or
   on the revival nudge for a dead path); the dead check runs in the
   report tick's command. `PathStats::active` is now published both ways
   (through Q1 a revived path's flag stayed false — a monitoring fix).
3. **The input arm is always armed.** An ack now ends any sender wait,
   including an intake wait (through P1/Q1 the ack wake was armed only
   while paused or pacing-dry; the acks were handled on the receiver task
   meanwhile). The ack's RTT sample is taken when the sender processes it.
4. **The live-path order** the receiver broadcasts in is ascending path id
   (the stats table) instead of the scheduler map's iteration order.
5. `[DIAG]` `wait[..]` / `wake[..]` gain a tenth bucket, `cmd`; `[IOWN]`
   gains `ack_batches` / `ack_dg`.

**Red / green (dev, VM; inside both locks).** Red on `main` 0ef0e0d (the
Q2 source-scan tests appended to the 0ef0e0d tree's `net/tests.rs`,
release-built on the VM): `q2_the_scheduler_halves_sit_behind_no_mutex`
FAILS at its first assertion (`net/ackdiag.rs:555`, `net/control_msg.rs:33`,
… `&Arc<SchedMutex>`; `Mutex<FecRateController>` in `net/mod.rs`). The two
grep gates are regression guards, green on 0ef0e0d in their banned-token
clauses (`q2_no_coop_budget_charge_per_message` fails there only on its
positive half, the sender's `recv_many` drains, which do not exist yet);
against `archive/thread-p2a` all three FAIL (`actor.rs`: `ActorSet`;
`net/receiver.rs`: `consume_budget`; `emit_source.rs`: `ActorCell<
FecRateController>`) — the gates catch P2a's shape. Red by construction
(the APIs do not exist on 0ef0e0d, the tests do not compile there):
`q2_an_ack_batch_ends_a_paused_sender_wait_with_zero_timer_advance` (the
REAL `run_window_sender`, paused clock: the control — a loop-top-only SACK
report — stays untaken through a 5 000-yield spin, the ack batch is taken
in it with the ack point at 7, and the clock never moves; a mutation that
disarms the input arm (`if false && !in_closed`) FAILS it: "the ack batch
was not handled without a timer: ack point 0"),
`q2_the_owner_routes_acks_to_the_sender_and_the_sender_takes_them`,
`q2_the_tx_control_set_is_the_scheduler_touching_set`,
`q2_one_task_per_loop_no_multiplexer` (its spawn half),
`io_owner::tests::the_owner_routes_tx_control_to_the_sender_lane`. Adapted
because they test the removed or moved mechanism (said here):
`lock_order_loopback` (the mutex witness is gone; it now reads the owner
identity witness, whose count survives a view's drop), the `s10_*`
`ControlCtx` constructions (`&mut`; the RX feed is checked through the
publication and the sender's mirror), P1's two D1 tests (replaced by the
Q2 routing and paused-wait tests), the `[DIAG]` wait/wake bucket tests (10
buckets), the `[ETA]` flush test, `io_owner_topology` (step 0: no arm).
`ack_wake_loopback` is unchanged in its assertions (`wake[ack] > 0`,
`timer_acked ≤ 5 %` of it). One pre-existing timing fragility fixed in its
own commit (`b51f531`): `holeclass_reachability`'s lossless floor failed 3
of 8 release runs on 0ef0e0d itself (2 × 50 MB can end before the first
1 s `[SUCC]`), now 2 × 400 MB (6 of 6 + debug green), assertions
unchanged. Green on the Q2 tree (`54c301b` + `b51f531`): debug suite rc 0,
**937 passed, 0 failed**, 49 ignored (84 binaries), `IDENTITY-PANICS 0`;
release suite **1 077 passed, 1 failed** (the holeclass fragility above,
before `b51f531`), 50 ignored (93 binaries); doc rc 0; wasm rc 0 (35
passed); parser tests rc 0 (`test_l1common` 89, `test_stage3_parse` 45,
`test_threadq1_parse` 26, `test_threadq2_parse` 26 checks); the Windows
host `cargo check -p raptorpath --tests --bin raptorpath` rc 0 with no
warning in a Q2 file. The envelope re-runs every suite from scratch
before the battery.

**Mechanism and predictions** (rule 11; derivation, then the named risks).

- *What is removed.* From the client: the receiver task's per-ack work
  (`on_window_ack` under the scheduler mutex, ≈ 1 per data datagram) and
  the per-ack `AckWake` notify that woke the sender for a sliver of work
  (§12 addendum: ≈ 3.9 ack wakes per intake wake at c2 — P1's c2 client
  CPU +5.3 %); every scheduler mutex take on both ends (77 sites).
- *What is added.* The ack handling runs on the sender (≈ 0.12–0.2 core at
  c1s/c1d by the plan's estimate); one relaxed-atomic publication per input
  batch and one mirror read per loop iteration.
- **c2 / c8** (ack-clocked, the sender mostly paused): client CPU per byte
  predicted TREND-BETTER or BETTER — **named prediction: the c2 client CPU
  per byte recovers P1's +5.3 %** (Q2 median ≤ MAIN median × (1 − 0.053),
  reported as `MET` / `MISSED`); goodput WITHIN.
- **c1s** (sender ≈ 36 % busy): goodput WITHIN; CPUCLI WITHIN or better.
- **c1d — the load-risk cell, named.** The sender was ≈ 96 % busy at c1d
  before Q1 moved its quinn waits out; the split moves the ack handling
  onto it. **Named refutation risk: c1d goodput WORSE** (the sender
  saturates). The client sender's `busy` is reported per arm.
- **CPUSRV**: WITHIN (the server's ack production is unchanged; its
  scheduler loses the mutex).
- **RTprop floor**: WITHIN (an ack's RTT sample is taken when the sender
  processes it; at c1s/c1d a busy sender takes it at its next loop top, at
  most one loop iteration later than the receiver task did).
- **`[LAG]` p99**: WITHIN.

**Binaries.** Q2 = this section's commit, MAIN = `main` 0ef0e0d (the
Q1-shipped tree), each archived with `git -c core.autocrlf=false -c
core.eol=lf archive` and built fresh on the benchmark VM in its own fresh
target dir; copied under the real name `raptorpath`; `sha256` in
`BINSHA.txt`, re-verified before every invocation. Both resolve
`RWM_RTOBS` as an opt-in; every arm runs with `RWM_RTOBS=1`.

**Harness.** Envelope `tools/l1/threadq2_run_all.sh` (both locks for the
whole session via `lib_battery.sh`; Q2 build → tests → MAIN build → smoke →
budget → battery → ackd block → score; hard backstop), driver
`threadq2_battery.sh`, scorer `threadq2_parse.py` (rows by
`stage3_parse.make_row`, helpers from `threadp1_parse` / `threadq1_parse`;
offline test `test_threadq2_parse.py`, 26 checks). No operator GO gate:
SMOKE-PASS proceeds.

**Tests first** (inside the locks, on the Q2 tree): `cargo build
--release`; `cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`; `cargo test --doc -p raptorpath
--release`; `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE` unset); the
debug run `cargo test -p raptorpath --no-fail-fast -- --test-threads=2`
(`IDENTITY-PANICS` = count of `io owner: quinn seam`, must be 0; the P1
lock-order witness went with the mutex); the python parser tests
(`test_l1common.py`, `test_stage3_parse.py`, `test_threadq1_parse.py`,
`test_threadq2_parse.py`). A failure that passes on an immediate solo
re-run is `FLAKE`; any other is `ABORT-TESTS`.

**Cells** (§5's geometry, size, capacity and > 5 % headroom, as §11–§13):
`c1s-400`, `c1d-400`, `c2-100`, `c8-100`. **Arms**: `MAIN`, `Q2`, both
bulk, `--window-reliable`, the shipped defaults (every arm `env -u
RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS
RWM_ACKDIAG RWM_IO_RT`, then `RWM_RTOBS=1` on every arm; rule 15d),
`perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150
SEED=<seed>`, one run, a fresh topology. **Plan per (rep, seed) block: 8
invocations**, cells in the order above, arm order within a cell rotated
by the block index (rule 3). Seeds 42 and 7; blocks rep 1 s42, rep 1 s7,
rep 2 s42, …; **n = 3 per seed (6 per arm and cell)**, interleaved, cut
only by the budget rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`):
`stage3_parse`'s set (`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]`
both; the RLC line both; no generation guard; cadence ACTIVE both;
`RWM_POOL_ANCHOR=0` both); `[GATES] RWM_EMIT_BATCH=1` both and the
`emission batching ACTIVE` echo on the client; the client's `[DIAG]`
`wake[` token; `[GATES] RWM_RTOBS=1` both; the main runtime's
`[THR]`/`[LAG]` window of the measured object and its `[IOWN]` lines on
both ends (client `run=1`, server `obj=1`); **the Q2 execution witness,
two-sided**: on every Q2 row every `[IOWN]` line of the window carries
`ack_batches=` on both ends, the client's owners routed acks to the sender
(`ack_dg` > 0 summed), the `wake[` token carries `cmd=`, and neither
endpoint prints a `[TOPO] io_rt` line or a `RWM_IO_RT` token; on every
MAIN row one `[TOPO] io_rt=shared` line per leg and `[GATES]
RWM_IO_RT=shared` on both ends, no `ack_batches=` token and no `cmd=`;
the row's `sha256` is its arm's binary.

**Pass rule per cell** (the plan's, as §13; Q2 against MAIN): a clause is
**WORSE** iff Q2's median is beyond MAIN's median·(1 ∓ rel) in the worse
direction **and** the two arms' [min, max] ranges are disjoint in that
direction (TREND-WORSE = beyond the band with overlapping ranges:
reported, not a fail):

| clause (plan) | direction | rel |
|---|---|---|
| goodput ≥ MAIN | higher is better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| client CPU per byte ≤ MAIN + MDE | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| server CPU per byte ≤ MAIN + MDE | lower | the same, as a declared transfer (§11–§13) |
| RTprop floor ≤ MAIN + tolerance, per leg (min non-zero `rtp_us` of the client's `[DIAG]`) | lower | max(5 %, MAIN's half-range / MAIN's median) (§12) |
| `[LAG]` p99 ≤ MAIN + tolerance, client and server (main runtime, the object's window) | lower | the same rule (§12) |
| fed loss = `[TRUTH]` | — | per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's (§11–§13) |
| DNF | — | excess > 0.20 (§5) |

**Per-cell verdict**: **FAIL** iff any clause is WORSE, the feed moved or
the DNF excess fired; **UNSCOREABLE** iff Q2 or MAIN has < 3 live rows or
≥ 2 witness-failed rows at the cell, a feed ratio is unread, or an abort
cause fired; **PASS** otherwise. Per arm: **PASS-EVERYWHERE**,
**FAIL-AT-<cells>**, or **UNSCOREABLE-AT-<cells>** (a FAIL outranks an
UNSCOREABLE).

**Outcomes, in precedence order** (one verdict):
1. `UNSCOREABLE` — an abort cause fired.
2. `DELIVERED` — Q2 passes at every cell: the split ships.
3. `REFUTED-WITH-RECORD (FAIL-AT-<cells>)` — Q2 fails somewhere: nothing
   ships; the busy / wake / thread data are the diagnosis. **No tuning in
   this battery.**
4. `UNSCOREABLE-AT (…)` — Q2 is not fully scoreable and fails nowhere.

**Reported, not gated** (per cell and arm, median [min–max], n): the
client's `[DIAG]` sender `busy`; `wake[ack]`, `wake[cmd]`, `wake[tun]`,
`wake[paused]`, `wake[timer_acked]`; the sender-lane batch size (client
`ack_dg / ack_batches`), the owner hops' datagrams per batch, `rx_capped`,
`send_err`, the owner lock-wait gauge; GSO factor per leg; acks per data
datagram (the server's last `[CTLD]`); the ack inter-arrival (the `ackd`
block: the same plan with `RWM_ACKDIAG=1`, one rep per seed, 16
invocations, run after the scored battery only if it fits before soft;
never scored); `[THR]` (process cores, the hottest threads with `comm`,
the main thread); parks per second. **The named predictions** (reported,
`PREDICTION` lines; they do not decide): `c2_cpucli_recovers_p1` and the
c1d sender `busy` per arm.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either tree),
`ABORT-TESTS`, `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (one
invocation per arm at `c1s-400` and `c8-100`, seed 42: every row LIVE with
goodput, both CPU lines, `busy`, `[LAG]` p99 on both ends, the owners on
both ends, `wake[ack]`, per leg `[TRUTH]`, `plc` and an RTprop floor, and
on Q2 rows the client's `ack_dg`; both arms present; nothing in it is a
result), `ABORT-BUDGET` (n < 2 per seed after the budget rule),
`ABORT-RC` (that row `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (no
summary after 2 attempts: `NO_DATA`); void class `VOID-COTENANT` (a
`cargo`/`rustc` process before or after an invocation).

**Budget.** Hard backstop = launch + 3 h, soft = hard − 10 min; the 5 h cap
is not approached. Priors (§13's measured session): Q2 build ≈ 5 min,
release + doc + wasm + debug tests ≈ 45 min, MAIN build ≈ 5 min,
`R_PRIOR` = 240 s per 8-invocation block (§13 measured ≈ 9.5 s per
invocation, i.e. ≈ 76 s); scored battery 6 blocks ≈ 8–24 min; smoke ≈ 1–2
min; `ackd` block ≈ 3–8 min. **Expected session wall ≈ 1 h 05 min – 1 h
30 min.** n per seed = min(3, ⌊(soft − now) / (2·R_est)⌋), `R_est` =
`R_PRIOR`·max(1, c_meas/60 s) from the smoke; the battery starts no block
that would cross soft (`TRUNCATED-AT-REP-BOUNDARY`, scored at the n
reached).

**Session rules.** Both locks for the whole session; detached envelope;
earned sentinels (`DONE-ALL` only with `TQ2-BATTERY-DONE`, `check` rc 0 and
no truncation); the operator reads `all-era.txt` at most once per ≈ 20 min
(rule 13, recorded); `pkill -x raptorpath` only; no `ens18`, firewall,
`sshd` or non-`rp-*` namespace is touched; exit state verified (0
`raptorpath`, 0 `rp-*` namespaces, both locks released). Ledgers are copied
to `docs/l1-raw/thread-q2/`.

**Amendment 1** (committed before launch; no scored result exists or was
read). **The pre-battery c1d component check** (the operator's request
before the gate; one invocation per arm per seed at `c1d-400`, seeds 42 and
7, n = 2, inside both locks, binaries built from 0ef0e0d and the Q2 tree;
UNSCORED, not part of the battery) read: client sender `busy` MAIN 62.2 %
[59.5–65.0] → Q2 55.5 % [54.0–57.0] (the split leaves the c1d sender room);
goodput 734 [710–758] → 725 [724–727] Mbit/s; CPUCLI/GB 6.95 → 6.70;
**client `[LAG]` p99 1916 [1811–2022] → 2469 [2079–2859] µs (median beyond
the 5.5 % tolerance, ranges disjoint at n = 2)**; **server CPU/GB 8.29 →
8.70 (+5 %, ranges disjoint at n = 2, inside the 6.5 % MDE)**. Therefore:
(i) **c1d client `[LAG]` p99 is a named risk** of this battery — the
prediction "`[LAG]` p99: WITHIN" stands as written, and the pass rule and
outcome vocabulary are unchanged (a WORSE there is a c1d FAIL, as
pre-registered); (ii) the result reports `[LAG]` p99 on both sides at every
cell and whether its rise tracks the sender's input-batch processing (the
existing `wake[ack]` / sender-lane `ack_dg` / `ack_batches` / `busy`
readings; no instrument is added); (iii) **`wake[ack]` (bucket 8) is not
comparable between the arms**: on MAIN it is P1's `AckWake` Notify arm,
armed only while paused or pacing-dry; on Q2 it is the always-armed input
channel arm — the two columns are different counters, and MAIN's 7048 vs
Q2's 5086 at c1d is not "fewer ack wakes".

### 14. Threading Q2 — result

Scored 2026-10-06 against the pre-registration above and amendment 1,
literally. **`DELIVERED`**: Q2 passes at every cell (no clause WORSE, the
feed unmoved, no DNF). Per the outcome rule the split ships — the merge is
the operator's. Nothing was tuned in this battery. **The amendment's named
risk did not fire**: c1d client `[LAG]` p99 reads 1712 → 1544 µs (−9.8 %,
WITHIN) at n = 6; the n = 2 pre-check's +29 % was not reproduced. **The
named c2 prediction MISSED**: c2 client CPU per byte −2.2 % (WITHIN), not
the −5.3 % that would have recovered P1's rise.

*Binaries and session.* Launched 07:54:38Z from `bc6429a` (engine
byte-identical to `b51f531`; the commits after it are `docs/`). Q2
`sha256 8d194b65…0fa70dd7`, MAIN `76decc7f…012e7c9ef` (0ef0e0d — the same
hash §13's Q1 binary had: the engine is unchanged since `7897ff1`); both
archived LF and built fresh on the benchmark VM in fresh targets
(`BINSHA.txt`). Q2 build 253 s; tests 07:58–08:44Z; MAIN build 253 s; smoke
08:49:07Z (`c_meas` 34 s < `C_PRED` 60 s, `R_est` = `R_PRIOR`, n = 3 per
seed, no cut); battery 08:49:07–08:56:07Z (420 s, 48 invocations, 8.8 s
mean); the `ackd` block 137 s (16 invocations); locks released 08:58:24Z.
**Session wall 1 h 04 min.** Exit state recorded by the envelope: 0
`raptorpath`, 0 `rp-*` namespaces, both locks released. `all-era.txt`
read at ≈ 08:14, 08:34, 08:54 and 09:14Z (rule 13's ≈ 20 min).

*Tests* (inside the locks, `TESTS.txt`): release suite rc 0, **1 078
passed, 0 failed**, 50 ignored (93 binaries); doc rc 0; wasm rc 0 (35
passed); **debug suite rc 0, 937 passed, 0 failed**, 49 ignored (84
binaries), `IDENTITY-PANICS 0`; parser tests rc 0. No flake fired.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 07:54:38Z) |
| `ABORT-CRLF` | no |
| `ABORT-BUILD` | no (either tree) |
| `ABORT-TESTS` | no (0 failures) |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 4 rows LIVE, both arms |
| `ABORT-BUDGET` | no (n = 3 per seed) |
| `ABORT-RC` | 0 of 48 |
| `ABORT-BRINGUP` | 0 |
| `VOID-COTENANT` | 0 of 48 |

*What ran.* 6 blocks × 8 = 48 rows, **all LIVE** (0 witness failures, 0
contaminated, 0 DNF); 6 per (cell, arm), 3 per seed; the `ackd` block 16
rows (reported only). The two-sided Q2 witness held on every row: every Q2
row's owners carried the sender lane on both ends with the client's acks
routed to the sender (`ack_dg` ≈ 338 500 at c1s, 339 600 at c1d, 85 000 at
c2/c8 — the client receiver's inbound batches read 1.0 datagram: it no
longer sees acks), a `cmd=` wake token and no `[TOPO]` / `RWM_IO_RT`;
every MAIN row the reverse. Ledgers: `docs/l1-raw/thread-q2/` (`tq2.log`,
`score.txt` with every per-rep value, `ackd.log`, `smoke.log`,
`smoke-check.txt`, `TESTS.txt`, `PLAN.txt`, `BINSHA.txt`, `all-era.txt`,
and `precheck-c1d/` — amendment 1's unscored c1d check); the
per-invocation endpoint logs stayed on the VM and were deleted with the run
directory.

*Per cell* (Q2 vs MAIN, n = 6 each; median [min–max]; the min–max rule as
pre-registered; `TB` = TREND-BETTER, `TW` = TREND-WORSE, `=` = WITHIN):

| cell | goodput Mbit/s (MAIN → Q2) | CPUCLI s/GB | CPUSRV s/GB | `[LAG]` p99 client µs | `[LAG]` p99 server µs | RTprop floor per leg | feed | verdict |
|---|---|---|---|---|---|---|---|---|
| `c1s-400` | 787.8 [743.9–810.1] → 764.9 [724.1–814.2] (−2.9 %, =) | 13.80 → 13.14 (−4.8 %, TB) | 18.52 → 18.67 (=) | 1918 [1436–2245] → 2180 [1876–2655] (+13.7 %, =, tol 21 %) | 1846 [1257–2342] → 1260 [1148–1845] (−31.7 %, TB) | 2500 → 2446 (=) | SAME | **PASS** |
| `c1d-400` | 738.7 [707.7–764.8] → 741.9 [710.4–755.4] (+0.4 %, =) | 17.01 → 16.47 (−3.2 %, =) | 20.99 → 21.22 (+1.1 %, =) | 1712 [1346–3521] → 1544 [1068–2720] (−9.8 %, =) | 1920 [1214–2678] → 1745 [1215–2489] (−9.1 %, =) | 2384 → 2310 (=); 2414 → 2308 (=) | SAME | **PASS** |
| `c2-100` | 89.87 [88.01–90.07] → 89.61 [83.33–90.25] (−0.3 %, =) | 29.45 → 28.80 (−2.2 %, =) | 32.75 → 32.35 (=) | 2042 [1806–2847] → 1189 [1106–1911] (−41.8 %, TB) | 1844 [1643–1904] → 1097 [886–1789] (−40.5 %, TB) | 12 700 → 11 580 (=) | SAME | **PASS** |
| `c8-100` | 95.94 [84.03–103.1] → 98.00 [91.33–103.6] (+2.1 %, =) | 45.35 → 44.10 (=) | 43.05 → 42.00 (=) | 1358 [1105–2609] → 2202 [2044–2361] (+62.2 %, **TW**) | 1875 [1202–2400] → 1889 [1192–2144] (=) | 9734 → 9836 (=); 43 940 → 38 510 (−12.4 %, TB) | SAME | **PASS** |

Arm verdict: **Q2 `PASS-EVERYWHERE`**.

*Predictions* (checks): c2/c8 client CPU TREND-BETTER or better —
**MISSED** (c2 −2.2 %, c8 −2.8 %, both WITHIN); **the named
`c2_cpucli_recovers_p1` — MISSED** (−2.2 % against the −5.3 % threshold);
c1s goodput WITHIN — **MET**; c1s CPUCLI WITHIN or better — **MET** (−4.8 %,
TB); the named c1d load risk (goodput WORSE) — **did not fire** (+0.4 %);
the client sender `busy` at c1d 59.0 [56.0–64.0] → 56.0 [54.5–69.0] %, at
c1s 43.5 → 39.5 %: taking the ack handling did not load the sender — it
fell where the mutex takes and the per-ack Notify wakes went; CPUSRV WITHIN
— **MET** everywhere; RTprop floor WITHIN — **MET** everywhere (c8 slow leg
TB); `[LAG]` p99 WITHIN — **MET as scored** (no WORSE), with c8 client
TREND-WORSE (+62 %, ranges overlapping) and c2 both sides and c1s server
TREND-BETTER.

*`[LAG]` p99 and the sender's input-batch processing* (amendment 1 (ii);
from the existing readings, no instrument added). Per Q2 row the client lag
p99 does not order with the sender's input processing: at c8 the six Q2
rows read 2044–2361 µs while `wake[ack]` spans 7956–8645, `busy` 24.5–30 %
and acks per sender-lane batch 8.3–9.0, in no common order; at c1d the two
highest-lag rows (2571, 2720 µs) have ordinary `busy` (55, 54.5 %) and
batch sizes (20.6, 20.5), while the highest-`busy` row (69 %) reads 1551
µs; at c1s the lag rises weakly with batch size (27.4 → 28.2 acks per batch
across 1876 → 2655 µs). What the instruments can say: the client-side lag
moved up at c8 (and c1s) and down at c2 and c1d, `wake[ack]` is
not comparable across arms (amendment 1 (iii)), and the per-row variation
within Q2 is not explained by the sender-lane batch size or the sender's
busy fraction. A per-iteration timing of the input drain (its share of the
sender's wall) does not exist in this build; it is the instrument that
would separate the two readings, and it is recorded here as the open
question, not added mid-battery.

*Reported, not gated* (`score.txt` has every value): acks per sender-lane
batch (client, Q2) c1s 27.7, c1d 20.9, c2 10.3, c8 8.6; client process
cores c1s 1.355 → 1.260, c1d 1.559 → 1.498, c2 0.323 → 0.315, c8 0.528 →
0.535; GSO factor unchanged (c1s 8.89 / 8.89); acks per data datagram
0.993–1.000 both arms; `timer_acked` per run c1s 45 → 88, c1d 15 → 8, c2 15
→ 10, c8 13 → 12 (out of 139–2146 paused wakes); parks per second within
± 7 % everywhere; owner lock-wait max (client) c1s 0.197 → 0.175, c1d 0.081
→ 0.058. Ack inter-arrival (`ackd`, client, n = 2): p99 c1s 392 → 410 µs,
c1d 661/708 → 578/770, c2 2449 → 2424, c8 2495/7762 → 2390/7358; p90 at c2
32 → 18 µs and c8 40 → 13 µs.

*Design note — the one departure from the plan's letter (rule 2).* The
plan says cross-direction reads "use what already exists (`SharedStats`
atomics, `cc_windows`) … class (c) = 0, no new snapshots". That held for
the one-actor shape the exploration was made under; the two-owner split
found sites that fit neither half (`docs/thread-p2-scheduler-access.md`
§4). Q2 resolves them with **seven new relaxed atomics per path in the
existing `SharedStats` (`PathStats::xdir`)**: the receiver writes the
incoming-loss EWMA, the arrival jitter and the last-arrival stamp; the
sender writes SRTT, RTprop, the RTT jitter and σ (and keeps the existing
`active` flag current both ways). It is a publication, not a lock: a reader
sees a value at most one publication old (declared effect 1), and the
battery shows no clause moved by it.

*Outside the pre-registered set (findings, no verdict).*

1. **The receiver task on the client is idle in bulk.** With the acks
   routed to the sender, the client receiver's inbound batches read 1.0
   datagram (MAIN: 21–28 at c1s/c1d); client process cores fell 2–7 %
   at c1s/c1d/c2.
2. **The c2 CPU the plan attributed to the per-ack wake did not come back
   at the size it went.** P1's +5.3 % (§11) was attributed (§12 addendum)
   to ≈ 3.9 `AckWake` wakes per intake wake; Q2 removes that wake and the
   per-ack mutex takes, and c2 client CPU per byte moved −2.2 %. Either
   most of P1's rise was elsewhere, or Q1 (which moved the sender's quinn
   calls to the owner and cut c2 CPUCLI −12.9 %, §13) already absorbed it.
   **[H]**, not separated by this battery.
3. **The amendment-1 pre-check over-read c1d client `[LAG]` p99 at n = 2**
   (+29 %, disjoint); at n = 6 the same clause reads −9.8 % with MAIN's own
   range 1346–3521 µs. The p99 of this probe at c1d varies by 2.6× within an
   arm.
