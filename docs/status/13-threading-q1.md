## 13. Threading Q1 — the per-path I/O owner — pre-registration

Phase Q1 of threading plan v2 (status §12 diagnosis addendum, rules 1–5):
**one I/O owner per path is the only code that calls quinn for that path.**
Committed before any VM contact of this battery (the dev builds and the
red/green test runs on the VM preceded it; they are tests, not results).
No number below is a result. Nothing is flipped by this battery; shipping
a placement is the operator's merge.

**What Q1 changes** (branch from `main` 69fd846; commits `95c750a` D9,
`881cca2` the owner, `dc9416a` the shipped-binary topology test, `00d2ab1`
docs, `8e62083` harness, `7897ff1` the receiver's ack-flush bound). No
wire change, no law change, the same structure at every path count and
every (δ, ρ):

1. **D9 (its own commit and its own arm).** The perf client/server body
   runs as a task on a runtime worker instead of the `block_on` main thread
   (the `main.rs` hunk of archive/thread-p2a 133adb9, code byte-identical).
2. **The owner** (`transport/io_owner.rs`). The connection is born inside
   the owner task: endpoint bind, connect/accept and the ADR-0010 handshake
   run there, so quinn's EndpointDriver and ConnectionDriver are spawned on
   the owner's runtime (quinn 0.11.9 spawns with a bare `tokio::spawn`). It
   is one loop around one `select!` over its own inputs — the command
   channel (`recv_many`, one coop-budget unit per drain), datagram reads,
   one uni-stream read slot, one control-stream write slot, the L0 shim's
   release timer — with no sub-executor, no per-message budget charge, no
   pass cap and no refresh timer. Every quinn call goes through one wrapper
   (`OwnedConn`) whose check asserts the caller is this path's owner task
   (task-local), on its pinned thread under `own`, with no scheduler guard
   alive (P1's witness). RTT / MTU / stats are published into a per-path
   view.
3. **Batched hops.** Producers stage serialized datagrams per path and hand
   each path's owner one batch per loop iteration over ONE bounded channel
   (`IO_CHANNEL_DEPTH` = 8 batches; full = back-pressure, never a drop):
   the sender one per Law-0 burst (flush right before its `select!`), the
   server receiver one per `recv_many` drain and at least every 32
   processed messages (`7897ff1`: an ack never waits behind more than one
   owner batch of processing) — its WindowAcks, still computed after
   decode, one per data datagram as before, in the control lane of the same
   channel. The owner forwards inbound datagrams as one
   batch per poll (≤ `INBOUND_BATCH_MAX` = 32); the receiver drains them
   with `recv_many`.
4. **Placement arm `RWM_IO_RT`** (default `shared`; echoed on `[GATES]`; in
   `RWM_FORWARD`): `shared` spawns the owner on the main runtime; `own` on
   one of K = max(1, cores − 2) named `current_thread` runtimes `rp-io-<k>`
   (4 on the 6-vCPU VM; least-loaded; the pool is created whole). One
   `[TOPO] io_rt=<arm> side= path= rt= k= cores=` line per path. A
   measurement arm, not a δ/ρ mode; the main runtime keeps its default
   worker count.
5. **Instruments** (`RWM_RTOBS`): `[LAG] io` and `[THR] io` per I/O runtime;
   one `[IOWN]` line per owner per window: loop/drain/batch counters,
   `rx_capped`, `send_err`, `too_large_staged`, the **owner lock-wait
   gauge** (`q_asleep_us`, `asleep_frac`: wall − thread CPU across the
   owner's quinn sections — the connection-mutex wait plus any involuntary
   preemption inside them; Linux) and the **driver-routing probe**
   (`drv_on`/`drv_off`: a delegating congestion controller counts each
   `on_sent` — called inside the ConnectionDriver's poll — on or off the
   owner's last thread).

**Known effects, declared** (behaviour changes that are not the mechanism):

1. **Producers see "staged for the owner", not quinn's verdict.** The
   synchronous `Result` the emission sites and `record_correction` consume
   is now the stage's: `Err` when the path has no owner, or the
   **`TooLarge` pre-check** — a datagram longer than the path's last
   published `max_datagram_size` is refused at stage time (counted,
   `too_large_staged` on `[IOWN]`; the caller logs it as before). A
   datagram that passes the pre-check but meets a smaller MTU at quinn
   (the MTU dropped between two publications) is quinn's `TooLarge` at the
   owner: counted in `send_err` and in the `RWM_DIAG` audit's `err`
   (warned at powers of two), never silent. Any other quinn send error
   (`ConnectionLost`, …) is counted the same way.
2. **The receiver's hold/deficit deadlines and their `[QCLK]` sample are
   evaluated once per wait, not once per message of a batch** (the
   drained batch is processed without re-entering the `select!`), and
   **its control datagrams leave in batches**: before every wait and at
   least every 32 processed messages, so a WindowAck can wait behind up to
   31 other messages' processing before it reaches its owner (on main it
   was sent as its message finished). This is the RTprop-floor risk named
   below, and it is bounded: the unbounded first form (one flush per
   drain) starved paths of the in-process four-path test (2 of 8 debug
   runs), fixed in `7897ff1` (8 of 8).
3. **`wire_rtt` reads the owner's published view**: refreshed every owner
   poll under `RWM_COPA_WIRE` (its only per-ack reader), else with the full
   snapshot. The full snapshot (`stats()`, `max_datagram_size()`) refreshes
   **on activity only**, when the poll did I/O and the snapshot is ≥
   `VIEW_REFRESH` = 1 ms old — no timer, so an idle owner never wakes for
   it; its age is max(1 ms, the gap since the path's last I/O). Off the
   shipped and battery configuration (`RWM_COPA_WIRE` unset) nothing
   per-ack depends on it.
4. **The inbound channel's depth counts batches**, and is
   `MSG_CHANNEL_BATCHES` = 4096 / `INBOUND_BATCH_MAX` = 128 batches: the
   worst case it holds is 128 × 32 = 4096 datagrams, ADR-0011's bound
   unchanged (4096 batches would have been up to 131 072). The cap's bind
   count is `rx_capped`.
5. **`RWM_CPUPROF`'s `hand` seam** now times the staging into the
   producer's batch, not quinn's `send_datagram` (which the owner calls).
6. `[RDIAG]`'s `msgs` counts every message of a wait's drain and its `q`
   samples the depth in owner batches once per wait (probe, unused here).

**Red / green (dev, VM; inside both locks).** Red on `main` 69fd846 (the
69fd846 tree with the two new tests that compile there added, debug):
`owner_rule_tests::no_quinn_connection_is_reachable_outside_the_owner`
FAILS at its first assertion ("connection-level quinn use outside the
owner": 69fd846's `quic.rs` calls `send_datagram`/`read_datagram` itself
and holds the `connections` map; `l0_netem.rs` calls `conn.send_datagram`
from its own task); `io_owner_topology::the_io_owner_topology_is_the_same_
at_every_path_count` FAILS ("client shared N=1: [GATES] does not echo
RWM_IO_RT=shared"); the new `test_l1common` check (a `[LAG] io` line must
not replace the main runtime's `[LAG]`) FAILS against 69fd846's
`l1common.py`. Red by construction (the APIs do not exist on 69fd846, the
tests do not compile there): `owner_runtime_tests::under_own_the_quinn_
driver_polls_on_the_owner_thread` (driver routing under `own`: `drv_off =
0`, `drv_on > 0` on both ends), `…::a_busy_owner_is_not_unparked_by_
producer_batches` (busy I/O thread: 0 parks / 0 unparks of the runtime
thread while K = 6 batches arrive, then ONE `recv_many` drain takes all
six; two-sided control: the same six batches spaced to an idle owner
unpark it ≥ 6 times), `…::the_identity_rule_admits_only_the_owner`, the
`io_owner` unit tests (K, the arm parse, least-loaded assignment, named
pool threads) and the `[IOWN]` renderer test. Green on the Q1 tree
(`7897ff1`, fresh target): release suite rc 0, **1 078 passed, 0
failed**, 51 ignored (93 binaries); doc rc 0; wasm rc 0 (35 passed);
**debug suite rc 0, 939 passed, 0 failed**, 49 ignored (84 binaries),
`LOCKORDER-PANICS 0`, `IDENTITY-PANICS 0`; parser tests rc 0
(`test_l1common` 89 checks, `test_threadq1_parse` 26 checks, 0 failed);
the Windows host `cargo check -p raptorpath --tests --bin raptorpath`
rc 0 with no warning in a Q1 file. One fault found and fixed on the way,
in its own commit: on `8e62083` the debug `quad_path_loopback` failed 2 of
8 runs (0 of 5 on 69fd846) — the receiver held each ack until its whole
`recv_many` drain was processed, and the inflated first RTT samples let
placement starve paths; `7897ff1` bounds the hold at 32 messages (8 of 8
green, then the full debug suite above). Adapted because they test the
removed or moved mechanism (said here): P1's
`connections_are_reached_through_one_guard_dropping_accessor` (asserted
the `connections` map exists) is replaced by the owner scan; the
`datagram_queue_audit` test reads the audit off the view; the rcvbuf
endpoint test binds `add_path`'s endpoint through an owner spawn; the
`ControlCtx` test constructions carry the new `out` field; the pinned
`[GATES]` echo carries `RWM_IO_RT=shared`. `lock_order_loopback`,
`reliable_delivery_hold`, the loss-truth (`s10_*`), store-gate, SACK and
`ack_wake_loopback` suites are unchanged and green.

**Mechanism and predictions** (rule 11; derivation, then the named
refutation risks).

- *What is removed.* From the sender's poll: every quinn call (§10 NEW
  c1d: 0.226 of sender wall asleep in quinn's connection mutex, sender
  96 % busy). From the server receiver: one `send_datagram` (a mutex take
  and a driver wake) per data datagram — now one batch per drain. From
  every reader: the per-datagram `msg_tx.send().await` (one batch per owner
  poll). Under `own` additionally: the ConnectionDriver polls on the
  owner's thread, so the mutex has one user per thread and is never
  contended (rule 5).
- *What is added.* One task hop per direction (the owner), batched; under
  `own` the hop crosses runtimes (a remote schedule + thread unpark when
  the peer is parked: FDH's +31 % c1s CPUCLI is the precedent for a
  cross-thread hop done per edge; Q1's is per batch).
- **c1s** (sender 36 % busy, lightly loaded): goodput predicted WITHIN for
  both placements (the ≈ 510 Mbit/s ceiling is not the sender).
  **Named refutation risk: CPUCLI at c1s** (MDE 2.4 %): the extra hop is
  per-batch work the single path did not need (FDT +22 % at c1s as a task;
  Q1 batches per burst where FDT signalled per edge, so smaller; `own` adds
  thread unparks). WORSE here is a possible outcome and is pre-stated.
- **c1d** (sender 96 % busy, 0.226 asleep in quinn): goodput predicted
  BETTER or TREND-BETTER for both (FDT +18 % from moving the waits off the
  sender), `own` ≥ `shared` (its driver never collides with the owner).
- **c2 / c8** (ack-clocked): goodput WITHIN; CPUCLI WITHIN (the client
  owner forwards acks one batch per poll).
- **CPUSRV**: WITHIN or TREND-BETTER at every cell (WindowAcks batched per
  drain).
- **RTprop floor** (risk at c1d and c2, the FDT precedent +5.6 % / +11 %):
  a datagram now waits for the owner's poll after the sender's flush.
- **`[LAG]` p99 (main runtime)**: WITHIN under `shared`; under `own` the
  quinn work leaves the main runtime, predicted WITHIN or lower.
- **D9**: the perf main thread (`comm=raptorpath`) reads ≈ 0 core on both
  sides (P2a's readout: 0.000 vs 0.11–0.18); goodput and CPU WITHIN.
- **Mechanism predictions (named, reported; they do not decide the
  outcome):** (i) routing — every live IOO row reads `drv_off = 0` with
  `drv_on > 0` on both ends (`routing_own`); the median client
  `drv_off_frac` under IOS is > 0 (`routing_shared_fails`); (ii) lock wait
  — IOO's median client `asleep_max` (max over owners of `asleep_frac`) is
  ≤ `LOCKWAIT_ZERO` = 0.01 at every cell (`lockwait_own_zero`); IOS's
  exceeds 0.01 at some cell (`lockwait_shared_nonzero`).

**Binaries.** Q1 = this section's commit, D9 = `95c750a`, MAIN = `main`
69fd846 (engine-identical to 0c91753: 69fd846 is docs only), each archived
with `git -c core.autocrlf=false -c core.eol=lf archive` and built fresh on
the benchmark VM in its own fresh target dir; copied under the real name
`raptorpath`; `sha256` in `BINSHA.txt`, re-verified before every
invocation. The IOS and IOO arms are the Q1 binary with `RWM_IO_RT=shared`
/ `own`. All three binaries resolve `RWM_RTOBS` as an opt-in (69fd846's
`gates.rs` already does: `[THR]`/`[LAG]` print only with it), so every arm
runs with `RWM_RTOBS=1` and carries the same main-runtime instrument; the
`[IOWN]` / `[LAG] io` / `[THR] io` lines exist on the Q1 binary only.

**Harness.** Envelope `tools/l1/threadq1_run_all.sh` (both locks for the
whole session via `lib_battery.sh`; Q1 build → tests → D9 and MAIN builds
→ smoke → budget → battery → ackd block → score; hard backstop), driver
`threadq1_battery.sh`, scorer `threadq1_parse.py` (rows by
`stage3_parse.make_row`, helpers from `threadp1_parse`; offline test
`test_threadq1_parse.py`; `l1common.thr` no longer lets a `[LAG] io` line
replace the main runtime's `[LAG]`). No operator GO gate: SMOKE-PASS
proceeds.

**Tests first** (inside the locks, on the Q1 tree): `cargo build
--release`; `cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`; `cargo test --doc -p raptorpath
--release`; `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE` unset); the
debug run `cargo test -p raptorpath --no-fail-fast -- --test-threads=2`
(the lock-order witness compiled in; `LOCKORDER-PANICS` = count of `lock
order: quinn seam` and `IDENTITY-PANICS` = count of `io owner: quinn seam`,
both must be 0); the python parser tests (`test_l1common.py`,
`test_stage3_parse.py`, `test_threadq1_parse.py`). A failure that passes on
an immediate solo re-run is `FLAKE`; any other is `ABORT-TESTS`.

**Cells** (§5's geometry, size, capacity and > 5 % headroom, as §11/§12):
`c1s-400`, `c1d-400`, `c2-100`, `c8-100`. **Arms**: `MAIN`, `D9`, `IOS`,
`IOO`, all bulk, `--window-reliable`, the shipped defaults (every arm `env
-u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS
RWM_ACKDIAG RWM_IO_RT`, then `RWM_RTOBS=1` on every arm and
`RWM_IO_RT=shared|own` on IOS|IOO; rule 15d), `perf_rwm_c.sh` with
`RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, a
fresh topology. **Plan per (rep, seed) block: 16 invocations**, cells in
the order above, arm order within a cell rotated by the block index (rule
3). Seeds 42 and 7; blocks rep 1 s42, rep 1 s7, rep 2 s42, …; **n = 3 per
seed (6 per arm and cell)**, interleaved, cut only by the budget rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`):
`stage3_parse`'s set (`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both;
the RLC line both; no generation guard; cadence ACTIVE both;
`RWM_POOL_ANCHOR=0` both); `[GATES] RWM_EMIT_BATCH=1` both and the
`emission batching ACTIVE` echo on the client; the client's `[DIAG]`
`wake[` token; **the Q1 execution witness, two-sided**: on every IOS/IOO
row `[GATES] RWM_IO_RT=<shared|own>` on BOTH endpoints, exactly one `[TOPO]
io_rt=<arm>` line per leg on BOTH endpoints with `rt=main` (shared) or
`rt=rp-io-<k>` (own) and none of the other arm, `[GATES] RWM_RTOBS=1` both,
and the `[IOWN]` window lines of the measured object on both ends; on every
MAIN/D9 row NO `[TOPO] io_rt` line and no `RWM_IO_RT` token on either
endpoint; the main runtime's `[THR]`/`[LAG]` window of the measured object
on both ends of every arm (client `run=1`, server `obj=1`); the row's
`sha256` is its arm's binary.

**Pass rule per cell** (the plan's, read with §5's MDE and §12's min–max
rule; each placement arm and D9 against MAIN): a clause is **WORSE** iff
the arm's median is beyond MAIN's median·(1 ∓ rel) in the worse direction
**and** the two arms' [min, max] ranges are disjoint in that direction
(TREND-WORSE = beyond the band with overlapping ranges: reported, not a
fail):

| clause (plan) | direction | rel |
|---|---|---|
| goodput ≥ MAIN | higher is better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| client CPU per byte ≤ MAIN + MDE | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| server CPU per byte ≤ MAIN + MDE | lower | the same, as a declared transfer (§11, §12) |
| RTprop floor ≤ MAIN + tolerance, per leg (min non-zero `rtp_us` of the client's `[DIAG]`) | lower | max(5 %, MAIN's half-range / MAIN's median) (§12) |
| `[LAG]` p99 ≤ MAIN + tolerance, client and server (main runtime, the object's window) | lower | the same rule (§12) |
| fed loss = `[TRUTH]` | — | per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's (§11, §12) |
| DNF | — | excess > 0.20 (§5) |

**Per-cell verdict**, per arm: **FAIL** iff any clause is WORSE, the feed
moved or the DNF excess fired; **UNSCOREABLE** iff the arm or MAIN has < 3
live rows or ≥ 2 witness-failed rows at the cell, a feed ratio is unread,
or an abort cause fired; **PASS** otherwise. Per arm: **PASS-EVERYWHERE**,
**FAIL-AT-<cells>**, or **UNSCOREABLE-AT-<cells>** (a FAIL outranks an
UNSCOREABLE).

**Outcomes, in precedence order** (one verdict; D9's per-cell verdicts are
reported as its attribution and never decide):
1. `UNSCOREABLE` — an abort cause fired.
2. `DELIVERED (SHIP-SHARED)` — IOS passes everywhere (whether or not IOO
   does): the mutex was not the cause and the simpler arm ships; `own` is
   deleted.
3. `DELIVERED (SHIP-OWN)` — IOO passes everywhere and IOS does not: `own`
   ships, `shared` is deleted.
4. `REFUTED-WITH-RECORD (NEITHER-PLACEMENT-PASSES)` — both arms FAIL
   somewhere: nothing ships; the lock-wait and routing data are the
   diagnosis. **No tuning in this battery.**
5. `UNSCOREABLE-AT (…)` — no arm passes everywhere and one is not fully
   scoreable.

In every outcome the shipped default stays one value for every N (the
placement is a measurement arm, not a mode).

**Reported, not gated** (per cell and arm, median [min–max], n): the
owner's wall asleep in the quinn connection lock (`[IOWN]` `asleep_frac`,
max and sum over owners, both sides) and the routing fraction
(`drv_off/(drv_on+drv_off)`); the GSO factor per leg (`[TRUTH] gso`); acks
per data datagram (the server's last `[CTLD]`); the ack inter-arrival (the
`ackd` block: the same plan with `RWM_ACKDIAG=1`, one rep per seed, 32
invocations, run after the scored battery only if it fits before soft;
never scored); `[THR]` (process cores, the hottest threads with `comm`, the
main thread — the D9 readout); parks per second (main runtime `[THR] rt`
and I/O runtimes `[THR] io`, both); `[LAG] io` p99; datagrams per batch on
the owner hops; `rx_capped`, `send_err`, `too_large_staged`; sender
`busy`. **The bar** (plan): the c1d goodput of each placement against
FDT's 501 Mbit/s (status §10), printed `BAR-MET` / `BAR-MISSED`, reported
(the plan lists it beside the pass rule, not in it).

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (any of the three
trees), `ABORT-TESTS`, `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`,
`ABORT-SMOKE` (one invocation per arm at `c1s-400` and `c8-100`, seed 42:
every row LIVE with goodput, both CPU lines, `busy`, `[LAG]` p99 on both
ends, per leg `[TRUTH]`, `plc` and an RTprop floor, and on IOS/IOO rows the
`[IOWN]` owners and `drv_on` on both ends; all four arms present; nothing
in it is a result), `ABORT-BUDGET` (n < 2 per seed after the budget rule),
`ABORT-RC` (that row `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (no
summary after 2 attempts: `NO_DATA`); void class `VOID-COTENANT` (a
`cargo`/`rustc` process before or after an invocation).

**Budget.** Hard backstop = launch + 3 h, soft = hard − 10 min; the 5 h cap
is not approached. Priors (§12's measured session): Q1 build ≈ 5 min,
release + doc + wasm + debug tests ≈ 45 min, D9 + MAIN builds ≈ 9 min,
`R_PRIOR` = 480 s per 16-invocation block (§12 measured ≈ 11 s per
invocation, i.e. ≈ 180 s); scored battery 6 blocks ≈ 18–48 min; smoke ≈ 2–4
min; `ackd` block ≈ 6–16 min. **Expected session wall ≈ 1 h 30 min – 2 h
10 min.** n per seed = min(3, ⌊(soft − now) / (2·R_est)⌋), `R_est` =
`R_PRIOR`·max(1, c_meas/120 s) from the smoke; the battery starts no block
that would cross soft (`TRUNCATED-AT-REP-BOUNDARY`, scored at the n
reached).

**Session rules.** Both locks for the whole session; detached envelope;
earned sentinels (`DONE-ALL` only with `TQ1-BATTERY-DONE`, `check` rc 0 and
no truncation); the operator reads `all-era.txt` at most once per ≈ 20 min
(rule 13, recorded); `pkill -x raptorpath` only; no `ens18`, firewall,
`sshd` or non-`rp-*` namespace is touched; exit state verified (0
`raptorpath`, 0 `rp-*` namespaces, both locks released). Ledgers are copied
to `docs/l1-raw/thread-q1/`.

**Amendment 1** (committed before session 2's launch; no scored result
exists or was read). **Session 1** (launched 2026-10-06T03:09:34Z from
`99f3132`) ended **`ABORT-TESTS`**, applied literally: the release suite's
`emit_batch_default_loopback::emission_batching_is_on_by_default` failed
and failed its immediate solo re-run (`REAL-FAILURE`) — its 8 MB loopback
transfer finished in 0.245 s, under the sender's 250 ms `[DIAG]` cadence,
so no `[DIAG]` line existed for the test to read. The operator stopped the
session at 03:52:05Z (before the debug suite finished; the outcome was
already fixed by the pre-registered rule), both locks released, 0
`raptorpath`. Release suite before the stop: 1 077 passed, 1 failed; doc
rc 0; wasm 35 passed; Q1 release build 259 s. The test is timing-fragile
and untouched by Q1 (`net/diag.rs` and the test file unchanged since
69fd846); `607f592` makes its transfer 80 MB (≥ 0.64 s at 1 Gbit/s; 3 of 3
release runs green on the VM), assertions unchanged. **Session 2** runs
the unchanged §13 plan from the tip that carries this amendment: the
engine tree is byte-identical to `7897ff1` (the commits after it touch
`tools/l1`, `tests/emit_batch_default_loopback.rs` and `docs/` only); the
Q1 binary is built from that tip, D9 and MAIN as before.

### 13. Threading Q1 — result

Scored 2026-10-06 against the pre-registration above and amendment 1,
literally. **`DELIVERED (SHIP-SHARED)`**: the shared placement (IOS)
passes at every cell; the own placement (IOO) fails at `c1d-400` (its
client `[LAG]` p99 and both legs' RTprop floor WORSE). Per the outcome
rule the simpler arm ships with `RWM_IO_RT=shared` (already the default)
and `own` is deleted — the merge and the deletion are the operator's.
Nothing was tuned in this battery.

*Binaries and session.* Session 2, launched 03:57:38Z from `0e2c051`
(engine byte-identical to `7897ff1`). Q1 `sha256 76decc7f…12e7c9ef`, D9
`81229041…1fa0b271` (`95c750a`), MAIN `7f3d38e5…661bfc13` (69fd846); all
archived LF and built fresh on the benchmark VM in fresh targets
(`BINSHA.txt`). Q1 build 265 s; tests 04:02–04:48Z; D9 and MAIN builds
255 s / 253 s; smoke 04:57:22Z (`c_meas` 78 s < `C_PRED` 120 s, `R_est` =
`R_PRIOR`, n = 3 per seed, no cut); battery 04:58:42–05:13:57Z (915 s, 96
invocations); the `ackd` block 301 s (32 invocations); locks released
05:18:58Z. **Session wall 1 h 21 min** (session 1, amendment 1: 43 min to
its `ABORT-TESTS`). Exit state recorded by the envelope: 0 `raptorpath`, 0
`rp-*` namespaces, both locks released. `all-era.txt` read at 04:17,
04:38, 04:59 and 05:19Z (rule 13's ≈ 20 min).

*Tests* (inside the locks, `TESTS.txt`): release suite rc 0, **1 078
passed, 0 failed**, 51 ignored (93 binaries); doc rc 0; wasm rc 0 (35
passed); **debug suite rc 0**, `LOCKORDER-PANICS 0`, `IDENTITY-PANICS 0`;
parser tests rc 0. No flake fired.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 03:57:38Z) |
| `ABORT-CRLF` | no |
| `ABORT-BUILD` | no (any tree) |
| `ABORT-TESTS` | no in session 2 (session 1: yes — amendment 1) |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 8 rows LIVE, all four arms |
| `ABORT-BUDGET` | no (n = 3 per seed) |
| `ABORT-RC` | 0 of 96 |
| `ABORT-BRINGUP` | 0 |
| `VOID-COTENANT` | 0 of 96 |

*What ran.* 6 blocks × 16 = 96 rows, **all LIVE** (0 witness failures, 0
contaminated, 0 DNF); 6 per (cell, arm), 3 per seed; the `ackd` block 32
rows (reported only). The instrument check the operator asked for: under
`own` the smoke's client logs carry 4 `[LAG] io` and 4 `[THR] io` lines
per window (K = 4 on the 6-vCPU VM) and the server's both windows — the
I/O-runtime instrument arms. Ledgers: `docs/l1-raw/thread-q1/` (`tq1.log`,
`score.txt` with every per-rep value and both seeds' medians, `ackd.log`,
`smoke.log`, `smoke-check.txt`, `TESTS.txt`, `PLAN.txt`, `BINSHA.txt`,
`all-era.txt`, and `session1/`); the per-invocation endpoint logs stayed on
the VM and were deleted with the run directory.

*Per cell* (arm vs MAIN, n = 6 each; median [min–max]; the min–max rule
as pre-registered; `B` = BETTER, `TB` = TREND-BETTER, `W` = WORSE, `TW` =
TREND-WORSE, `=` = WITHIN):

| cell | arm | goodput Mbit/s (MAIN →) | CPUCLI s/GB | CPUSRV s/GB | `[LAG]` p99 cli / srv | RTprop floor per leg | feed | verdict |
|---|---|---|---|---|---|---|---|---|
| `c1s-400` | IOS | 521.7 [500.2–551.4] → **773.5** [744.7–826.9] (+48.3 %, B) | 13.99 → 13.79 (=) | 20.27 → 18.65 (−8.0 %, B) | = / = | 2 378 → 2 480 (+4.3 %, =) | SAME | **PASS** |
| `c1s-400` | IOO | → **778.3** [742.9–825.9] (+49.2 %, B) | → 12.11 (−13.4 %, B) | → 16.25 (−19.9 %, B) | = / = | → 2 519 (+5.9 %, TW) | SAME | **PASS** |
| `c1s-400` | D9 | → 356.1 [333.7–381.5] (−31.7 %, **W**) | → 28.39 (+102.9 %, **W**) | → 31.41 (+54.9 %, **W**) | = / = | = | SAME | FAIL |
| `c1d-400` | IOS | 436.7 [418.7–494.0] → **729.5** [617.5–745.0] (+67.0 %, B) | 27.19 → 17.16 (−36.9 %, B) | 29.31 → 21.25 (−27.5 %, B) | = / = | 2 228 → 2 422 (+8.7 %, TW); 2 227 → 2 401 (+7.8 %, TW) | SAME | **PASS** |
| `c1d-400` | IOO | → **711.5** [672.5–723.6] (+62.9 %, B) | → 16.95 (−37.7 %, B) | → 20.52 (−30.0 %, B) | 2 109 → 2 574 (+22.0 %, **W**) / TW | → 2 493 (+11.9 %, **W**); → 2 556 (+14.8 %, **W**) | SAME | **FAIL** |
| `c1d-400` | D9 | → 399.3 (−8.6 %, **W**) | → 30.35 (+11.6 %, **W**) | → 33.07 (+12.8 %, **W**) | TB / = | = | SAME | FAIL |
| `c2-100` | IOS | 88.79 → 89.83 (+1.2 %, =) | 33.7 → 29.35 (−12.9 %, B) | 46.8 → 33.1 (−29.3 %, B) | = / = | = | SAME | **PASS** |
| `c2-100` | IOO | → 89.97 (+1.3 %, =) | → 27.85 (−17.4 %, B) | → 32.2 (−31.2 %, B) | = / = | = | SAME | **PASS** |
| `c2-100` | D9 | → 89.3 (=) | → 37.2 (+10.4 %, **W**) | = | = / = | = | SAME | FAIL |
| `c8-100` | IOS | 101.9 → 97.47 (−4.4 %, TW) | 53.2 → 44.4 (−16.5 %, TB) | 58.65 → 41.05 (−30.0 %, B) | = / = | = ; = | SAME | **PASS** |
| `c8-100` | IOO | → 98.01 (−3.8 %, =) | → 44.9 (−15.6 %, TB) | → 38.65 (−34.1 %, B) | = / = | = ; = | SAME | **PASS** |
| `c8-100` | D9 | → 102.5 (=) | = | = | = / = | = ; = | SAME | PASS |

Arm verdicts: **IOS `PASS-EVERYWHERE`**; IOO `FAIL-AT-c1d-400`; D9
(attribution, reported) `FAIL-AT-c1s-400,c1d-400,c2-100`. **The bar**
(reported): c1d goodput IOS 729.5, IOO 711.5 Mbit/s against FDT's 501 —
`BAR-MET` for both.

*The mechanism table* (reported; client / server; median of the per-row
max over owners of `asleep_frac` = the owner's wall asleep in quinn's
connection lock as a share of wall; `drv_off_frac` = the share of the
ConnectionDriver's `on_sent` calls that ran off the owner's thread):

| cell | IOS lock wait cli / srv | IOS `drv_off_frac` cli / srv | IOO lock wait cli / srv | IOO `drv_off` |
|---|---|---|---|---|
| `c1s-400` | **0.196** / 0.171 | 0.633 / 0.586 | 0.0115 / 0.0258 | **0** on every row, both ends |
| `c1d-400` | **0.064** / 0.084 | 0.339 / 0.374 | 0.0094 / 0.0202 | 0 |
| `c2-100` | 0.0052 / 0.0042 | 0.419 / 0.439 | 0.0016 / 0.0027 | 0 |
| `c8-100` | 0.0046 / 0.0052 | 0.475 / 0.396 | 0.0014 / 0.0023 | 0 |

**Mechanism predictions:** `routing_own` **MET** (every IOO row
`drv_off = 0`, `drv_on > 0`, both ends); `routing_shared_fails` **MET**
(IOS median client `drv_off_frac` 0.34–0.63); `lockwait_shared_nonzero`
**MET** (0.196 at c1s, 0.064 at c1d); `lockwait_own_zero` **MISSED** —
IOO's client reads 0.0115 at c1s (0.0094 c1d, ≤ 0.0016 elsewhere) against
the 0.01 threshold, and its server 0.020–0.026 at c1s/c1d. With one user of
the mutex per thread under `own`, that residual is the gauge's other term
(involuntary preemption inside a section on the oversubscribed VM), not a
mutex wait — **[H]**, not separated by this instrument.

**Other predictions** (checks): c1s goodput WITHIN — **MISSED** (BETTER,
+48 % both placements); the named c1s CPUCLI refutation risk — did **not**
fire (IOS WITHIN, IOO BETTER); c1d goodput BETTER — **MET**, but `own` ≥
`shared` **MISSED** (711.5 < 729.5, ranges overlapping); c2/c8 goodput
WITHIN — **MET** (c8 IOS TREND-WORSE); c2/c8 CPUCLI WITHIN — exceeded
(BETTER / TREND-BETTER); CPUSRV WITHIN-or-better — **MET** (BETTER at
every cell, −8 to −34 %); the RTprop-floor risk at c1d — **fired** for IOO
(WORSE both legs), TREND-WORSE for IOS; `[LAG]` p99 under `own` WITHIN or
lower — **MISSED** at c1d client (+22 %, WORSE); D9's main thread ≈ 0 core
— **MET** (0.000 on every D9/IOS/IOO row; MAIN 0.11–0.18); D9's goodput
and CPU WITHIN — **MISSED** (finding 1).

*Reported, not gated* (`score.txt` has every value): GSO factor
(`[TRUTH] gso`, client leg 0) c1s MAIN 8.96 / D9 4.42 / IOS 8.94 / IOO
7.49; c1d 5.04 / 3.46 / 7.08 / 6.45; c2 5.09 / 4.36 / 5.86 / 5.99; c8 4.27
/ 4.06 / 5.79 / 6.19. Acks per data datagram 0.992–1.000 everywhere.
Datagrams per owner batch: client data 45 (c1s IOS) to 16 (c8); inbound
8–28; the inbound cap's bind fraction (`rx_capped / rx_batches`, client /
server medians): c1s IOS 0.74 / 0.25, IOO 0.57 / 0.05; c1d IOS 0.37 /
0.22, IOO 0.21 / 0.09; ≤ 0.01 at c2/c8 (finding 4). `send_err` = `too_large_staged` = 0 on
every row. Parks per second (client, main runtime / I/O runtimes): c1s
MAIN 8 742, D9 27 144, IOS 17 428, IOO 4 964 / 7 162; c1d 23 669, 27 915,
15 788, 5 197 / 11 372; c2 6 134, 8 519, 5 802, 2 381 / 3 267; c8 10 940,
11 980, 6 318, 2 745 / 4 232. `[LAG] io` p99 (IOO) 2.0–3.5 ms. Process
cores (client) c1s MAIN 0.89, D9 1.27, IOS 1.32, IOO 1.16. Ack
inter-arrival (`ackd`, client leg 0, n = 2): p99 c1s MAIN 359, IOS 170,
IOO 366 µs; c1d 836 / 1 314 / 600 µs; c2 and c8 ≈ 1.9 → 2.4–2.6 ms under
both IO arms (p90 lower: c2 118 → 26/16 µs).

*Outside the pre-registered set (findings, no verdict).*

1. **D9 alone reproduces §12's c1s collapse.** Moving the perf body onto a
   runtime worker (`95c750a`, the only change in that arm) cost c1s
   goodput −31.7 % and client CPU per byte +103 % with disjoint ranges —
   §12 measured P2a (which carried D9 with the actor) at −33.0 % and +87 %.
   The c1s collapse §12 attributed to the actor and the per-message budget
   unit is therefore at least largely D9's — a re-scoping of §12 finding 1,
   recorded here, not acted on. **[H]** The D9 rows show the sender
   (`busy` 35 → 66 %), GSO halved (8.96 → 4.42) and worker parks ×3: the
   generator task now competes with the sender for workers and its
   per-packet channel wakes land on workers instead of a parked main
   thread. The Q1 arms carry D9 and still gain +48 % at c1s: the owner hop
   removes the sender's quinn waits and restores GSO (8.94).
2. **The owner's gain is not the mutex.** The shared placement keeps the
   driver off the owner's thread on 34–63 % of transmits and the owner
   spends up to 20 % of its wall asleep in quinn's lock at c1s, yet IOS
   matches IOO's goodput and CPU at every cell; what both share — the
   sender no longer calling quinn, batched hops, batched acks — carries
   the gain. This is the plan's "if `shared` also passes everywhere, the
   mutex was not the cause" reading, here measured directly.
3. **`own` pays latency at c1d**: the client `[LAG]` p99 (+22 %) and the
   RTprop floor (+12–15 %) rise with the cross-runtime hops (each
   owner→receiver and sender→owner wake crosses a thread), while main-
   runtime parks fall to a fifth.
4. **The inbound batch cap binds on most client batches at c1s**
   (`INBOUND_BATCH_MAX` = 32: bind fraction 0.74 under IOS, 0.57 under
   IOO; 0.21–0.37 at c1d; ≤ 0.01 at c2/c8). At the bulk single-path cell
   the cap, not quinn's buffered count, sets the inbound batch size, so it
   operates there close to a constant (measurement discipline 18): a
   defect finding to carry, no verdict drawn from it here. It is a memory
   bound (the inbound channel's worst case = ADR-0011's 4096 datagrams), not
   a law; raising it moves that bound, which is why it is recorded rather
   than tuned.
