## 11. Threading P1 — topology-independent defect fixes — pre-registration

(Section number assigned at merge: the P0 section is written concurrently.)
Phase P1 of the threading plan: the defect fixes that are better in every
topology, from the threading audit (th2 §6: D1, D2, D5, D11, D16, D17). No
wire change, no law change, no new gate: the arms differ by binary only.
Committed before VM measurement (the dev builds and the red/green test runs
on the VM preceded it; they are tests, not results). No number below is a
result.

**What P1 changes** (branch `feat/thread-p1`, engine commits `73b0092` red
tests + witness, `2a04fb2` D2, `9e3b2e1` D5, `e8f876c` D1/D11/D16/D17,
`a557afc` end-to-end test + harness, `1c2a57c` the `AckWake` ack counter
and `wake[timer_acked]`):

- **D1, the ack wakes the sender.** `on_window_ack` ends with
  `notify_one()` on a `tokio::sync::Notify` shared with the window sender,
  after every state the ack carries is published. The sender's `select!`
  gains one arm, `ack_wake.notified()`, armed under exactly the union of the
  guards of the two 1 ms polls it shortcuts (`tx_paused`, or the `cc_pace`
  bucket dry), charged to a new bucket 8. The polls and the `sack_rx`
  drain stay. Before: a paused sender saw an ack only at the next 1 ms
  `sleep_until`, which tokio rounds up to 1–2 ms.
- **D2, no quinn call with the scheduler held.** A debug-build witness
  (`scheduler::sched_lock`: `SchedMutex`/`SchedGuard` count live guards per
  thread; every quinn seam of `transport::quic` asserts zero) and the fixes
  at every site it or the audit found: `net/mod.rs` WindowStart and
  Shutdown broadcasts; the five `for pid in recv_scheduler.lock()
  .live_paths()` loops in `net/receiver.rs`; `tasks/report.rs`
  `max_datagram_size`; `control_msg.rs` `wire_rtt` (quinn, before the guard)
  and `set_cc_window_bytes` (after it) in `on_ack` / `on_path_report` /
  `on_window_ack`; `net/diag.rs` per-path `wire_rtt` + `quinn_path_stats`
  inside the `[DIAG]` scheduler block (every 250 ms under `RWM_DIAG`, so
  every prior battery ran it); `copa_feed.rs` window writes.
- **D5.** `connections` holds `Arc<quinn::Connection>`; one accessor clones
  the `Arc` and drops the DashMap shard guard before any quinn call or
  `.await`. (A `quinn::Connection` clone takes quinn's connection mutex
  twice — `ConnectionRef::clone`/`drop`, quinn-0.11.9 `connection.rs`
  920–940 — so cloning the connection itself per datagram would add the
  very lock D4 measures.) `connect`/`accept` clone the endpoint out (cold).
- **D11.** One pinned tail-sweep `Sleep`, `reset()` only when the µs
  deadline moves.
- **D16.** `SharedStats::path_ref`: a write-once dense table (64 slots, a
  resource bound) read with no lock, scan or `Arc` clone at the
  per-datagram / per-ack sites.
- **D17.** `BatchCounter` per-path sequences are atomics in a dense table
  (64, a resource bound; larger ids take the cold mutex map); numbering
  identical (tested).

**Red / green (tests, on the VM, inside the locks; logs in
`docs/l1-raw/threadp1/`).** On the red commit `73b0092` (instrumentation and
tests, no fix): 8 of the 13 P1 lib tests fail (the D1 permit, the D1
routing scrape, the wait-bucket audit at 9 buckets, the `wait[]` format,
D5/D11/D16/D17 shape) and `lock_order_loopback` fails with 4 violations
(`send_control_datagram` under the WindowStart broadcast's guard, and
`max_datagram_size` under the report tick's guard, on both endpoints). On
`e8f876c`: the lib suite 529 passed / 0 failed, `lock_order_loopback`
`checks=10582 violations=0 acquires=64894`, and the in-process / spawned
loopbacks `perf_loopback` (8), `shutdown_test`, `walldiag_loopback`,
`emit_batch_default_loopback`, `wire_compact_loopback`,
`ackdiag_repair_recon`, `copa_sole_loopback` green in the debug build (the
witness compiled in); on `1c2a57c` the 15 P1-filtered lib tests green and
`lock_order_loopback` `checks=10612 violations=0`. (`RED.txt`,
`GREEN-dev.txt`.) One red reading was void and fixed:
`the_histogram_is_wide_enough_for_every_bucket` scraped its own literal and
was green on red; it now reads the gauge source only. Red by construction,
not run on `main`: `ack_wake_loopback` (no `wake[` token exists there).

**Component statement (rule 14).** `tests/ack_wake_loopback.rs` (the
shipped binary, one c3-shaped loopback path, bulk, 4 MB × 2), on
`1c2a57c`, three debug runs and one release run: `wake[ack]` 3 275–3 565,
`wake[timer_acked]` 2–8 (0.06–0.23 % of the ack wakes), `wake[paused]`
922–1 022 (0.26–0.31 × `ack`: timer wakes in true ack gaps, legitimate on a
20 Mbit / 20 ms cell), `wake[tun]` 854–902. So the wiring holds (an ack
during a paused wait ends it), and a paused sender takes ≈ 3.8 ack wakes
per intake wake: most ack wakes re-evaluate and wait again, which is the
CPU cost side. (The first form of the invariant, `wake[paused] ≤ 0.05 ×
wake[ack]`, failed on this bench at 0.27–0.33 for exactly that reason and
was replaced by `timer_acked` before this pre-registration.) What the bench
cannot see: the c1 cells' ack rate (one WindowAck per data datagram,
≈ 50 k/s at c1s), where the extra wakes cost most, and the dual cells'
contention.

**Mechanism and prediction.** P1 removes lock nesting and per-datagram
locking that sits mostly off the critical path (D2's hot sites are cold or
DIAG-only; D5/D16/D17 replace uncontended or read-mostly locks with
atomics), so the CPU and goodput clauses are predicted WITHIN at every cell.
D1 is the one behaviour change: a paused sender wakes per ack instead of
per 1–2 ms. Predicted: `wake[timer_acked]` ≈ 0 on every P1 row where the
sender pauses; c1s sender `busy` lower (a paused wait ends earlier, so less of it
is spent asleep before the next useful iteration — the plan's expectation;
it may instead rise, since each ack wake is an iteration body). **The
refutation risk named in advance:** at c1s/c1d the ack rate is tens of
thousands per second, so a sender that is paused for much of the run takes
one loop body per ack where it took one per millisecond: CPUCLI is the
clause most at risk.

**Binaries.** P1 = this section's commit (the engine tree is `1c2a57c`;
this commit touches docs only); MAIN = `main`
`8d7d8c1`. Both archived with `git -c core.autocrlf=false -c core.eol=lf
archive`, built fresh on the benchmark VM in fresh target directories,
copied under the real name `raptorpath`; `sha256` in `BINSHA.txt`,
re-verified before every invocation.

**Harness.** Envelope `tools/l1/threadp1_run_all.sh` (both locks for the
whole session via `lib_battery.sh`; build → tests → MAIN build → smoke →
budget → battery → score; hard backstop), driver `threadp1_battery.sh`,
scorer `threadp1_parse.py` (rows by `stage3_parse.make_row`). No operator
GO gate: SMOKE-PASS proceeds (the plan is fixed here).

**Tests first** (inside the locks, on the P1 tree): `cargo build
--release`; `cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`; `cargo test --doc -p raptorpath
--release`; `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE` unset); and
**the debug-witness run** `cargo test -p raptorpath --no-fail-fast --
--test-threads=2` (debug build: the lock-order witness is compiled in, so
every in-process and spawned-binary test doubles as a lock-order audit; the
count of `lock order: quinn seam` panics is recorded, and must be 0).
rc and passed/failed/ignored to `TESTS.txt`. A failure that passes on an
immediate solo re-run of that test is `FLAKE` (the
`sigma_diag_reachability` timing class is the known one); any other is
`ABORT-TESTS` and the battery does not run.

**Cells** (§5's geometry, size, capacity and > 5 % headroom; rule 16 as §5
and §8 state it): `c1s-400`, `c1d-400`, `c2-100`, `c8-100`. **Arms**: `MAIN`
and `P1`, both bulk, `--window-reliable`, the shipped defaults (every arm
`env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST`,
nothing set back; rule 15d), `perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1
RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, a fresh topology. **Plan per
(rep, seed) block: 8 invocations**, cells in the order above, arm order
within a cell rotated by the block index (rule 3). Seeds 42 and 7; blocks
rep 1 s42, rep 1 s7, rep 2 s42, …; **n = 3 per seed** (6 per arm and cell),
cut only by the budget rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`):
`stage3_parse`'s (`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both; the
RLC line both; no generation guard; cadence ACTIVE both, OFF neither;
`RWM_POOL_ANCHOR=0` both); `[GATES] RWM_EMIT_BATCH=1` both and the `emission
batching ACTIVE` echo on the client (the shipped default on both arms); the
P1 execution witness: the client's `[DIAG]` carries `wake[` on every P1 row
and on no MAIN row; the row's `sha256` is its arm's binary.

**Scored quantities** per invocation: goodput, completion, DNF (past
150 s), `CPUCLI` and `CPUSRV` (whole invocation; per GB moved, the bytes
being fixed per cell), per leg the RTprop floor (the minimum non-zero
`p<i>: … rtp_us=` over the client's `[DIAG]` lines), per leg the fed loss
against truth `plc`/`[TRUTH] loss=` (rule 19), `busy` (median of the
client's `wait[… busy=]`), and the cumulative `wake[..]` counts of the
client's last `[DIAG]` (per arm: `tun paused pace gen nack defc tail flush
ack`, plus `timer_acked`, the `paused`/`pace` timer wakes during whose wait
the ack counter moved).

**MDE and the min–max rule.** Relative tolerance per clause: goodput §5's
committed goodput MDE (`c1s-400` 4.9 %, `c1d-400` 5.6 %, `c2-100` 1.4 %,
`c8-100` 4.0 %); `CPUCLI` per GB §5's CPUCLI MDE (2.4 / 6.5 / 6.3 /
11.6 %); `CPUSRV` per GB the same CPUCLI MDE as a **declared transfer** (§5
measured no server-CPU MDE); RTprop floor max(5 %, MAIN's half-range /
MAIN's median) per leg (no committed MDE exists; the second term is §5's
half-range floor applied to the control). A clause reads, P1 against MAIN:
**WORSE** iff P1's median is beyond MAIN's median·(1 ∓ rel) in the worse
direction **and** the two arms' [min, max] ranges are disjoint in that
direction; **TREND-WORSE** iff beyond the band with overlapping ranges
(reported and named, not a fail); **BETTER** / **TREND-BETTER**
symmetrically; **WITHIN** otherwise. With n = 6 per arm the min–max
condition is the guard against reading one outlier rep as an effect.

**Per-cell verdict.** **WORSE** iff any clause is WORSE (goodput, `CPUCLI`
per GB, `CPUSRV` per GB, RTprop floor per leg), or the feed moved at a leg
(med(`plc`/truth) under P1 outside [1/1.3, 1.3] × MAIN's), or the DNF
excess > 0.20; **BETTER** iff not WORSE and goodput or a CPU clause is
BETTER; **SAME** otherwise; **UNSCOREABLE** at a cell where either arm has
< 3 live rows or ≥ 2 witness-failed rows, a feed ratio is unread, or an
abort cause fired.

**Outcomes, in precedence order** (one verdict):
1. `UNSCOREABLE` — an abort cause fired.
2. `REFUTED-WITH-RECORD (WORSE-AT-<cells>)` — WORSE at any scoreable cell:
   P1's "better in every topology" claim is refuted at those cells and P1
   does not ship as is; the per-cell record and the `wake`/`busy` columns are
   the diagnosis. No tuning in this battery.
3. `UNSCOREABLE-AT-<cells>` — a hard blocker at a cell, none WORSE.
4. `DELIVERED (BETTER-AT-<cells>)` or `DELIVERED (SAME everywhere)` — no
   cell WORSE, every cell scoreable, every witness held. P1 ships (it is a
   fix set, not a default flip; the merge is the operator's).

**The D1 mechanism reading** (reported beside the verdict, not part of it):
per P1 row with `wake[paused] + wake[ack] ≥ 100`, D1 **holds** iff
`wake[timer_acked] ≤ 0.05 × wake[ack]` — a 1 ms timer wake during whose
wait an ack landed is, with the wiring intact, only a `select!` tie (both
ready at one poll); with the wiring broken every such wake is one. The raw
`wake[paused]` is reported, not bounded: it also counts timer wakes in true
ack gaps longer than the poll, which are legitimate (the component bench
below has them). Per cell: `D1-WAKE-HOLDS` (every read
row holds), `D1-WAKE-FAILS(k/m)`, or `D1-INERT-NEVER-PAUSED` (no row reads:
the sender never waited on the brake there, so the arm had nothing to
shortcut — a finding, not a failure). **Predictions** (checks, not
outcomes): `DELIVERED`; `D1-WAKE-HOLDS` at every cell that pauses; c1s
sender busy lower under P1 (printed MET/MISSED).

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either tree),
`ABORT-TESTS`, `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (one
invocation per arm at `c1s-400` and `c8-100`, seed 42: every row LIVE with
goodput, both CPU lines, `busy`, `[TRUTH]`, `plc` and an RTprop floor per
leg; nothing in it is a result), `ABORT-BUDGET` (n < 2 after the budget
rule), `ABORT-RC` (that row `VOID-RC`, the battery goes on),
`ABORT-BRINGUP` (no summary after 2 attempts: `NO_DATA`); void class
`VOID-COTENANT` (a `cargo`/`rustc` process before or after an invocation).

**Budget.** Hard backstop = launch + 2 h 30 min, soft = hard − 10 min (the
task's ≈ 2 h; the 5 h cap is not approached). Priors: builds ≈ 9 min,
release tests ≈ 25 min (V4/§8: 20–24 min), debug-witness tests ≈ 25 min,
`R_PRIOR` = 240 s per 8-invocation block (§8: 128 rows in ≈ 26 min);
battery ≈ 6 blocks ≈ 25 min. n per seed = min(3, ⌊(soft − now) /
(2·R_est)⌋), `R_est` = `R_PRIOR`·max(1, c_meas/60 s) from the smoke; the
battery starts no block that would cross soft
(`TRUNCATED-AT-REP-BOUNDARY`, scored at the n reached).

**Session rules.** Both locks for the whole session; detached envelope;
earned sentinels (`DONE-ALL` only with `TP1-BATTERY-DONE`, `check` rc 0 and
no truncation); the operator reads sentinels at ≥ 10 min intervals;
`pkill -x raptorpath` only; no `ens18`, firewall, `sshd` or non-`rp-*`
namespace is touched; exit state verified (0 `raptorpath`, 0 `rp-*`
namespaces, both locks released). Ledgers are copied to
`docs/l1-raw/threadp1/`.

### 11. Threading P1 — result

Scored 2026-10-05 against the pre-registration above, literally; no
amendment was made. **`DELIVERED (SAME everywhere)`**: no cell WORSE, every
cell scoreable, every witness held. **D1 mechanism: `D1-WAKE-HOLDS` at all
four cells.** Prediction "c1s sender busy falls": **MISSED** (36.5 % →
36.8 %).

*Binary and session.* P1 = `c765e3f` (engine tree `1c2a57c`), `sha256
ea773ba6…a2ab962e`; MAIN = `8d7d8c1`, `sha256 5dccf33b…ce763d12`; both
built fresh on the benchmark VM (Xeon E5-2650 v3 era) in fresh target
directories (`BINSHA.txt`). Launch 16:40:26Z (hard 19:10:26Z); P1 build
269 s; tests 17:05–17:29Z; MAIN build 252 s; smoke 17:36Z (`c_meas` 36 s <
`C_PRED` 60 s, so `R_est` = `R_PRIOR`; n = 3 per seed, no cut); battery
17:36:48–17:44:43Z (475 s, 48 invocations, 9.7 s mean); locks released
17:44:43Z. **Session wall 1 h 04 min.** Exit state recorded by the
envelope: 0 `raptorpath`, 0 `rp-*` namespaces, both locks released (the
next tenant took them afterwards). The operator read `all-era.txt` at
≈ 10-min intervals (16:50, 17:01, 17:11, 17:21, 17:31, 17:41, 17:51Z) and
`TESTS.txt` once (17:12Z) — the pre-registered cadence, which is more
frequent than rule 13's ≈ 20 min (a declared deviation; one short ssh
read each, no abort signature followed).

*Tests.* `cargo build --release` rc 0; release suite rc 0, **1061 passed, 0
failed**, 51 ignored (91 binaries); doc rc 0; wasm rc 0, 35 passed;
**debug-witness suite rc 0, 922 passed, 0 failed**, 49 ignored (82
binaries), 0 `lock order: quinn seam` panics in the captured test output.
That grep sees uncaptured output only: a spawned child's stderr reaches the
log only when its test fails, so a witness panic in a non-fatal task of a
child would not show there. What the debug run does show is that the whole
suite is green with the witness compiled in (a violation in a critical task
is a failed test); the witness counter itself is read in-process in
`lock_order_loopback` (dev runs: `checks=10612 violations=0`). No flake
fired.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 16:40:26Z) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script after extract) |
| `ABORT-BUILD` | no (either tree) |
| `ABORT-TESTS` | no (0 failures) |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 4 rows LIVE, `wake[` on both P1 rows and on neither MAIN row |
| `ABORT-BUDGET` | no (n = 3 per seed) |
| `ABORT-RC` | 0 of 48 |
| `ABORT-BRINGUP` | 0 (0 `RUN-RETRY`) |
| `VOID-COTENANT` | 0 of 48 |

*What ran.* 6 blocks × 8 = 48 rows, all `LIVE` (0 witness failures, 0
contaminated, 0 DNF); 6 per (cell, arm), 3 per seed. Ledgers:
`docs/l1-raw/threadp1/` (`tp1.log` sha256 ca0caf15…, `score.txt` ceaf65df…
with every per-rep value and per-seed median, `TESTS.txt`, `smoke.log`,
`smoke-check.txt`, `PLAN.txt`, `BINSHA.txt`, `all-era.txt`, and the dev
`RED.txt` / `GREEN-dev.txt`); per-invocation endpoint logs stay on the VM
under `/home/vibe/tp1run/run/diag-tp1`.

*Per cell* (P1 vs MAIN, n = 6 each; median [min–max]; relative tolerance in
brackets; the min–max rule as pre-registered):

| cell | goodput Mbit/s MAIN → P1 | CPUCLI s/GB | CPUSRV s/GB | RTprop floor µs per leg | plc/truth per leg MAIN → P1 | verdict |
|---|---|---|---|---|---|---|
| `c1s-400` | 522.2 [484.7–578.6] → 535.9 [512.3–586.5] (+2.6 %, WITHIN 4.9 %) | 14.21 → 13.79 (−3.0 %, **TREND-BETTER**, 2.4 %) | 20.41 → 19.88 (−2.6 %, **TREND-BETTER**, 2.4 % transfer) | 2386 → 2335 (WITHIN) | 1.031 → 0.911 (band [0.79, 1.34]) | **SAME** |
| `c1d-400` | 434.2 [423.0–474.7] → 431.7 [405.5–450.6] (−0.6 %, WITHIN 5.6 %) | 26.77 → 27.49 (+2.7 %, WITHIN 6.5 %) | 29.05 → 29.91 (+3.0 %, WITHIN) | 2216 → 2279; 2232 → 2255 (WITHIN) | 1.007 → 0.993; 0.982 → 1.019 | **SAME** |
| `c2-100` | 88.56 [88.34–88.88] → 89.15 [87.32–89.57] (+0.7 %, WITHIN 1.4 %) | 32.15 [30.0–32.6] → 33.85 [33.2–35.7] (+5.3 %, WITHIN 6.3 %) | 46.65 → 46.55 (WITHIN) | 12 460 → 12 290 (WITHIN, tol 16.9 %) | 1.005 → 1.012 | **SAME** |
| `c8-100` | 103.3 [96.3–105.2] → 102.3 [99.9–105.3] (−0.9 %, WITHIN 4.0 %) | 53.3 → 50.9 (−4.5 %, WITHIN 11.6 %) | 59.3 → 57.1 (WITHIN) | p0 8 840 [7 819–9 822] → 10 670 [9 773–10 819] (+20.8 %, **TREND-WORSE**, tol 11.3 %); p1 40 100 → 40 610 (WITHIN) | 1.004 → 1.002; 1.006 → 1.007 | **SAME** |

Both seeds' medians are in `score.txt`. The two `TREND-*` readings are
named as pre-registered (beyond the band, ranges overlapping; not a fail):
c1s CPU on both ends lower, and the c8 fast leg's RTprop floor higher by
21 % with the ranges touching (P1's minimum 9 773 µs against MAIN's maximum
9 822 µs).

*The D1 mechanism reading* (P1 rows; cumulative counts off the client's
last `[DIAG]`):

| cell | `wake[ack]` per row | `wake[timer_acked]` | `timer_acked`/`ack` | `wake[paused]` | `wake[tun]` | reading |
|---|---|---|---|---|---|---|
| `c1s-400` | 8 987–10 048 | 2–15 | ≤ 0.16 % | 196–463 | 7 595–8 240 | **HOLDS** 6/6 |
| `c1d-400` | 1 201–3 542 | 0–4 | ≤ 0.11 % | 43–97 | 5 631–6 139 | **HOLDS** 6/6 |
| `c2-100` | 19 006–19 783 | 7–18 | ≤ 0.09 % | 1 369–1 530 | 4 831–5 342 | **HOLDS** 6/6 |
| `c8-100` | 19 392–25 190 | 6–16 | ≤ 0.08 % | 353–483 | 2 428–2 613 | **HOLDS** 6/6 |

With acks flowing a paused sender is woken by the ack, not the timer, at
every cell: of the waits in which an ack landed, the 1 ms poll won at most
0.16 %. The remaining `wake[paused]` are timer wakes in true ack gaps.

*Outside the pre-registered set (findings, no verdict).*
1. **The c2 sender CPU moved up with disjoint ranges.** `CPUCLI` at
   `c2-100` +5.3 % (33.2–35.7 vs 30.0–32.6 s/GB: every P1 row above every
   MAIN row), inside the 6.3 % MDE so WITHIN by the rule. It is the cell
   with the most ack wakes per intake wake (≈ 3.9: 19 k `ack` vs 5 k `tun`):
   each ack wake is one loop body that mostly re-evaluates and waits again
   — the cost side named in advance. At c1s (≈ 1.2 ack wakes per intake
   wake) the CPU went the other way (−3.0 %); at c8 (≈ 9 per intake wake)
   −4.5 % within a wide MDE. A cheaper wake (signal only when the ack can
   change the pause predicate) is the obvious follow-up; it was not tuned
   here.
2. **The c1s busy prediction missed.** The sender loop's `busy` share is
   unchanged at c1s (36.5 % → 36.8 %) and c1d (95.0 % → 94.5 %, the
   CPU-bound cell); it fell at c8 (35.0 % → 30.8 %, within the spread). At
   c1s the sender is not paused for long stretches (≈ 200–460 timer
   wakes against ≈ 8 000 intake wakes per run), so a faster wake while
   paused has little wall time to recover; the ≈ 520 Mbit/s c1s ceiling is
   not set by the sender's pause latency.
3. **c1s goodput is above §8's era** (MAIN 522 median vs §8 NEW 437 and
   EB0 446), with emission batching ON in both arms here and the §7
   receive-buffer fix in; cross-session, not scored.
4. The c8 fast-leg RTprop floor (finding as the `TREND-WORSE` above): P1's
   six rows sit at 9.8–10.8 ms against MAIN's 7.8–9.8 ms. The floor is
   the min over a run of the app-echo RTT; one unmeasured hypothesis is
   that a sender woken sooner while paused hands quinn datagrams that queue
   behind the previous burst. n = 6 cannot separate an effect from the
   spread (the ranges touch); a follow-up should read it with more reps
   before P2 builds on it.

*What it means.* The P1 fix set is non-regressing at the four cells on
every pre-registered clause, and its one behaviour change does what it was
built to do: an ack ends a paused wait (the 1 ms poll wins ≤ 0.16 % of the
waits an ack lands in). It does not buy throughput or sender idle time at
these cells, because the sender is rarely paused at c1s and CPU-bound at
c1d; the defects it removes (lock nesting under quinn, DashMap guards
across awaits, per-iteration timer churn, per-datagram stats/batch locks)
were off the critical path, as predicted. Shipping P1 is the operator's
merge.
