## 15. D9 attribution and the c8 lag re-check — pre-registration

Two questions on `main` 88476a4 (Q1 + Q2 shipped), one session, interleaved.
Committed before any VM contact; no number below is a result. Nothing is
flipped by this battery; a revert on `main` is the operator's merge.

**Q1 — D9.** `95c750a` moved the perf generator / sink body (`cmd_perf`,
`raptorpath/src/main.rs`) off the `block_on` main thread onto a runtime worker
task. Alone on old main (§13) it cost c1s goodput −31.7 % and client CPU per
byte +103 % (c1d −8.6 % / +11.6 %, c2 CPUCLI +10.4 %, c8 WITHIN); it ships
inside the current stack, which is far faster than that main. §13 never
measured D9 *inside* the stack: does the current stack do better, worse or the
same without it?

**Q2 — c8 client `[LAG]` p99.** §14 read it MAIN 1358 [1105–2609] → Q2 2202
[2044–2361] µs (+62 %, TREND-WORSE, ranges overlapping, n = 6). More reps.

**Arms.** `MAIN` = `main` 88476a4 (D9 shipped). `NOD9` = 88476a4 with only the
D9 hunk of `95c750a` reverted (`git show 95c750a -- raptorpath/src/main.rs`
applied in reverse: `perf::server` / `perf::client` awaited directly on the
`block_on` thread, as before `95c750a`), commit `e059e30` on branch
`measure/nod9` (a measurement arm, not shipped). Both bulk, `--window-reliable`,
every arm `env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT` then **`RWM_RTOBS=1` on both
arms**; `perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150
SEED=<seed>`, one run, fresh topology. Each engine archived with `git -c
core.autocrlf=false -c core.eol=lf archive` and built fresh on the benchmark
VM in its own fresh target dir; `sha256` in `BINSHA.txt`, re-verified before
every invocation. No cargo test suite is run: MAIN is unmodified main (green at
§14) and NOD9 differs by the 7-line revert (the pre-D9 code); the parser tests
run in the envelope. This is a declared departure from §13/§14's session shape.

**Cells and n.** `c1s-400`, `c1d-400`, `c2-100`, `c8-100` (§5 geometry,
capacity and > 5 % headroom as §11–§14). Seeds 42 and 7. **n = 6 per arm and
cell (3 per seed) at every cell; `c8-100` n = 12 (3 more per seed, run as
c8-only blocks after the full blocks, same arm rotation).** Plan per full
(rep, seed) block: 8 invocations (cells in the order above, arm order within a
cell rotated by the block index, rule 3); then 6 c8-only blocks of 2. 60
invocations. The c8 verdict uses all 12 rows per arm.

**Mechanism and prediction (rule 11).** D9 alone made the generator task
compete with the sender for workers: sender `busy` 35 → 66 %, GSO factor
halved (8.96 → 4.42) and worker parks ×3 at c1s (§13 finding 1, **[H]**). The
current stack's per-path owner restores GSO (8.94) under D9 (the Q1 arms carry
D9). Whether D9 still costs anything with the owner in place is not derivable
from the existing data, so **no directional prediction is made at c1s/c1d**;
the decision rule below carries the outcome. c2/c8 (ack-clocked, the sender
mostly paused; D9 alone WITHIN at c8): predicted SAME. The execution readout
is predicted as §13 measured it: the client main thread reads ≈ 0 core under
MAIN and the generator's cost (0.11–0.18 core in §9) under NOD9.

**Witnesses per invocation** (a failing row is `CONTAMINATED` / `WITNESS-FAIL`,
excluded and counted; no client summary = `NO_DATA`): `stage3_parse`'s set
(`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both; the RLC line; no
generation guard; cadence ACTIVE; `RWM_POOL_ANCHOR=0`); `[GATES]
RWM_EMIT_BATCH=1` both and the `emission batching ACTIVE` echo; `[GATES]
RWM_RTOBS=1` both; the main runtime's `[THR]`/`[LAG]` window of the measured
object and its `[IOWN]` lines on both ends; the Q2-era execution witness on
**both** arms (every `[IOWN]` line carries `ack_batches=`, the client's
`ack_dg` > 0, `wake[` carries `cmd=`, no `[TOPO]` line, no `RWM_IO_RT` token);
**the D9 execution witness, two-sided, in the binary and in the run**: the
string `perf task failed` is present in MAIN's binary and absent from NOD9's
(checked at launch; a violation refuses the battery), and on every row the
client `[THR]` main thread (`comm=raptorpath`) reads **< 0.002 core under MAIN
and ≥ 0.002 core under NOD9** (D9 rows read 0.000 on every §13/§14 row); the
row's `sha256` is its arm's binary.

**Pass rule per cell** (§13/§14's, NOD9 against MAIN; the min–max rule): a
clause is **WORSE** iff NOD9's median is beyond MAIN's median·(1 ∓ rel) in the
worse direction and the two arms' [min, max] ranges are disjoint in that
direction; **BETTER** the mirror image; **TREND-WORSE / TREND-BETTER** =
beyond the band with overlapping ranges (reported, never decisive).

| clause | direction | rel |
|---|---|---|
| goodput | higher better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| client CPU per byte | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| server CPU per byte | lower | the same, as a declared transfer (§11–§14) |
| RTprop floor, per leg | lower | max(5 %, MAIN's half-range / median) |
| `[LAG]` p99, client and server | lower | the same rule |
| fed loss = `[TRUTH]` | — | per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's |
| DNF | — | excess > 0.20 |

Cell verdict: **FAIL** iff any clause is WORSE, the feed moved or the DNF
excess fired; **UNSCOREABLE** iff either arm has < 3 live rows or ≥ 2
witness-failed rows at the cell, a feed ratio is unread, or an abort cause
fired; **PASS** otherwise. NOD9 arm verdict: **PASS-EVERYWHERE**,
**FAIL-AT-<cells>**, **UNSCOREABLE-AT-<cells>** (a FAIL outranks an
UNSCOREABLE).

**Decision rule (fixed in advance; one outcome, in precedence order).**
1. `UNSCOREABLE` — an abort cause fired. D9 stays (no change on no evidence).
2. `UNSCOREABLE-AT (…)` — NOD9 not fully scoreable and fails nowhere. D9 stays;
   the close names what would decide it.
3. **`REVERT-D9`** — NOD9 is PASS-EVERYWHERE **and** BETTER (median beyond the
   §5 MDE, ranges disjoint) in goodput, client CPU per byte or server CPU per
   byte at **at least one cell**. D9 is reverted on `main` (the operator's
   merge of a revert commit; `measure/nod9`'s hunk is that revert).
4. **`KEEP-D9 (MAIN better)`** — NOD9 FAILS at some cell (a clause WORSE, the
   feed moved or DNF): MAIN is better there; D9 stays. If NOD9 is also BETTER at
   another cell the label is `KEEP-D9 (MIXED)` — outside the revert rule,
   which needs PASS-EVERYWHERE; D9 stays and both readings are recorded.
5. **`KEEP-D9 (SAME)`** — NOD9 PASS-EVERYWHERE and nothing BETTER beyond the
   MDE: D9 stays. TREND-BETTER readings are reported as findings; they do not
   revert D9 (a null at this n is not a refutation of a difference below the
   MDE; no `NEEDS-MORE` is declared).

**c8 client `[LAG]` p99 (Q2), reported.** MAIN's c8 client `[LAG]` p99 median
and [min–max] at n = 12 are reported against §14's MAIN reading (1358 µs) and
Q2 reading (2202 µs). **The cross-session comparison is report-only** (two
sessions on different days; rule 9 and the 2.3× same-configuration drift
record bar a verdict across them). **The within-session read is MAIN vs NOD9
only**, through the cell's `[LAG]` p99 clause (client and server). The §14
TREND-WORSE itself is not re-scored by this battery: its comparator (pre-Q2
main) is not an arm here.

**Reported, not gated** (per cell and arm, median [min–max], n): the `[THR]`
per-thread core budget — the client and server main thread (`comm=raptorpath`),
the `rp-w-*` workers summed, the three hottest threads, the process cores;
worker parks and unparks per second (main runtime); the client sender `busy`
(`[DIAG]`); `wake[ack|cmd|paused|timer_acked|tun]`; GSO factor per leg; acks
per data datagram; the sender-lane batch size; `rx_capped`, `send_err`, the
owner lock-wait gauge. Per-rep values for every row (rule 4).

**Abort causes, in priority order** (the scored section opens with this table,
filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either tree, or the two
binaries byte-identical), `ABORT-TESTS` (the parser tests), `ABORT-SHA`,
`ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (one invocation per arm at
`c1s-400` and `c8-100`, seed 42: every row LIVE with goodput, both CPU lines,
`busy`, `[LAG]` p99 on both ends, the owners on both ends, `wake[ack]`, per
leg `[TRUTH]`, `plc` and an RTprop floor, `ack_dg`, and the D9 witness; both
arms present; nothing in it is a result), `ABORT-BUDGET`, `ABORT-RC` (that
row `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (no summary after 2
attempts: `NO_DATA`); void class `VOID-COTENANT` (a `cargo`/`rustc` process
before or after an invocation). A smoke that fails only on the D9 main-thread
threshold is amended (committed before any scored result) rather than
relabelled.

**Harness.** `tools/l1/threadd9_run_all.sh` (both locks for the whole session
via `lib_battery.sh`; parser tests → MAIN and NOD9 builds → smoke → budget →
battery → score; hard backstop launch + 4 h, soft = hard − 10 min),
`threadd9_battery.sh`, `threadd9_parse.py` + `test_threadd9_parse.py` (37
checks), derived from the V-Q2 files with only what this battery needs
changed (arms, the two-sided D9 witness, the decision rule, the c8 readout,
the c8-only extra reps; the ack-cadence block is dropped). Expected wall:
builds ≈ 9 min, smoke ≈ 1 min, battery ≈ 10–20 min; ≤ 5 h cap. Session
rules as §14 (detached envelope, `all-era.txt` read at most once per ≈ 20
min, `pkill -x raptorpath` only, no `ens18` / firewall / `sshd` / non-`rp-*`
namespace, exit state verified). Ledgers go to `docs/l1-raw/thread-d9/`.

### 15. D9 attribution and the c8 lag re-check — result

Scored 2026-10-06 against the pre-registration above, literally (no
amendment). **`KEEP-D9 (MAIN better: NOD9 FAIL-AT-c1s-400)`**: without D9 the
current stack is **worse at c1s** (goodput −11.4 %, client CPU per byte
+4.9 %, both WORSE, ranges disjoint) and WITHIN at c1d, c2 and c8; NOD9 is
BETTER nowhere. D9 stays on `main`; nothing is reverted. (In the shared
taxonomy `REVERT-D9` would have mapped to `FLIP-RECOMMENDED`; it did not
fire.) Nothing was tuned.

*Binaries and session.* Launched 12:53:51Z. MAIN `sha256 8d194b65…0fa70dd7`
(commit 88476a4; the same hash as §14's Q2 binary), NOD9 `ba70a6aa…38977a`
(commit `e059e30` on `measure/nod9`, 88476a4 minus the D9 hunk), each archived
LF and built fresh in a fresh target on the benchmark VM. Battery
13:03:22–13:12:38Z (556 s, 60 invocations); session wall ≈ 19 min; locks
released 13:12:38Z, exit state 0 `raptorpath`, 0 `rp-*` namespaces.
`all-era.txt` was read at ≈ 12:55 (build running) and ≈ 13:16 (done). The
launch went through a waiting wrapper (`take_lock` fails fast; the VM was
idle, the locks free, no wait occurred). Ledgers: `docs/l1-raw/thread-d9/`
(`td9.log`, `score.txt` with every per-rep value, `smoke.log`,
`smoke-check.txt`, `TESTS.txt`, `PLAN.txt`, `BINSHA.txt`, `all-era.txt`).

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` / `ABORT-CRLF` / `ABORT-BUILD` / `ABORT-SHA` / `ABORT-SENTINEL-UNWRITABLE` / `ABORT-BUDGET` | no |
| `ABORT-TESTS` | no (parser tests: 45 + 26 + 37 checks, 0 failed; no cargo suite, as declared) |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 4 rows LIVE, both arms, D9 witness held |
| `ABORT-RC` / `ABORT-BRINGUP` / `VOID-COTENANT` | 0 / 0 / 0 of 60 |

*What ran.* 60 rows, **all LIVE** (0 witness failures, 0 contaminated, 0
DNF); 6 per (cell, arm), 12 at c8. **The D9 witness held two-sided on every
row**: the string `perf task failed` is in MAIN's binary and not NOD9's, and
the client main thread reads 0.000 core on every MAIN row and 0.123 [0.123–
0.132] (c1s), 0.122 (c1d), 0.019 (c2), 0.020 (c8) under NOD9 (server main
thread under NOD9 0.204 / 0.159 / 0.015 / 0.013). The Q2-era witnesses held
on both arms.

*Per cell* (NOD9 vs MAIN, median [min–max]; `W` = WORSE, `TW` = TREND-WORSE,
`=` = WITHIN; no clause was BETTER or TREND-BETTER):

| cell | n | goodput Mbit/s (MAIN → NOD9) | CPUCLI s/GB | CPUSRV s/GB | `[LAG]` p99 client µs | `[LAG]` p99 server µs | RTprop floor | feed | verdict |
|---|---|---|---|---|---|---|---|---|---|
| `c1s-400` | 6 | 766.7 [721.9–831.4] → 679.6 [664.7–704.6] (−11.4 %, **W**) | 13.25 → 13.9 (+4.9 %, **W**) | 18.86 → 20.1 (+6.6 %, TW) | 2272 → 1936 (=) | 1461 → 1412 (=) | = | SAME | **FAIL** |
| `c1d-400` | 6 | 728.1 [677.9–755.1] → 687.8 [664.2–703.6] (−5.5 %, = at the 5.6 % band) | 16.34 → 16.36 (=) | 21.58 → 21.84 (=) | 2208 → 2211 (=) | 1958 → 1824 (=) | = / = | SAME | PASS |
| `c2-100` | 6 | 90.23 → 89.82 (=) | 28.6 → 28.75 (=) | 32.0 → 33.25 (+3.9 %, =) | 1921 → 1881 (=) | 1644 → 1412 (=) | = | SAME | PASS |
| `c8-100` | 12 | 98.13 [82.9–102.9] → 95.59 [89.0–105.2] (=) | 43.9 → 45.4 (=) | 41.35 → 42.15 (=) | 2708 → 2734 (=) | 1347 → 1778 (+32 %, =, tol 34 %) | = / = | SAME | PASS |

NOD9 arm verdict: **`FAIL-AT-c1s-400`**. c1d goodput (−5.5 %, ranges overlap)
sits just inside the 5.6 % band: a lean toward MAIN, not a clause.

*What the instruments say about c1s* (reported, `score.txt` has every
value; an interpretation, **[H]**). Without D9 the generator runs on the main
thread at 0.124 core (server sink 0.204) and the process cores fall (client
1.249 → 1.167, server 1.794 → 1.710; worker parks/s 17 013 → 13 287 client).
Yet the goodput falls 11 % and the per-byte CPU rises: the sender `busy`
39.2 → 41.2 %, the client `wake[ack]` 3300 → 1990 and `wake[paused]` 420 →
706, GSO 8.95 → 9.03 (unchanged). So with the owner and the sender-lane
acks in place, D9 no longer costs c1s anything; it helps (a worker-to-worker
wake instead of a futex on a thread without a tokio core is the plan's
mechanism; the data are consistent with it, not proof). §13's D9-alone
collapse (−32 %, GSO halved) was an interaction with the pre-Q1 stack: the
stack's own changes absorbed it. The unscored smoke had read one NOD9 c1s row
at 199.8 Mbit/s (nothing in the 6 scored NOD9 c1s rows approaches it, minimum
664.7); smoke is not a result and is mentioned only so the outlier is on the
record.

*c8 client `[LAG]` p99 re-check (Q2 of this section).* MAIN (the Q2 binary,
`sha256` identical to §14's) reads **2708 µs, [1248–3636], n = 12**. NOD9
2734 [1126–4429], n = 12; the **within-session** MAIN-vs-NOD9 clause is WITHIN
(+1.0 %, tolerance 44 %). **Report-only, cross-session** (stated in advance as
no verdict): §14's MAIN reading 1358 [1105–2609] and Q2 reading 2202
[2044–2361]; this session's identical binary reads +23 % over §14's Q2 value
and its 12 rows span 1248–3636 µs, which contains both §14 readings. The
within-session spread of one binary at n = 12 (2.9×) is the point this
battery adds: the p99 of this probe at c8, like c1d (§14 finding 3), moves
more between reps and sessions than the difference §14 read; the +62 %
TREND-WORSE is therefore not corroborated as a Q2 effect, but this battery
has no pre-Q2 arm and does not re-score it.

*Reported, not gated* (client unless named; MAIN → NOD9): sender `busy` c1s
39.2 → 41.2, c1d 52.2 → 57.0, c2 10 → 11, c8 27.2 → 29.2 %; GSO c1d 7.04 →
7.67, c8 unchanged; client process cores c1d 1.465 → 1.401, c2 0.316 →
0.318, c8 0.522 → 0.530; client parks/s c1d 17 378 → 13 112, c2 5 768 →
4 738, c8 6 263 → 5 052 (NOD9 lower everywhere); client `wake[ack]` c1d 5498
→ 2516, c2 and c8 unchanged; acks per data datagram 0.99–1.00 both arms;
owner lock-wait max c1s 0.172 → 0.149.

*Deviations and notes.* (1) No cargo test suite was run (declared). (2) The
decision rule counts server CPU per byte as well as client CPU per byte in
"CPU/GB" (the task named goodput or CPU/GB; stated in the pre-registration).
(3) The envelope launched through a waiting wrapper because `take_lock`
fails fast on a held lock; the locks were free and it ran at once. (4) The
`REVERT-D9` / `KEEP-D9` labels are this section's own outcome set.
