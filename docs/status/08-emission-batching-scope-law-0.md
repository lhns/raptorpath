## 8. Emission-batching scope (Law 0) — pre-registration

The `RWM_EMIT_BATCH` scope `emit_batch_live = live_paths == 1` (`c639d56`)
was a path-count step: at N ≥ 2 the emission path was bit-identical to
gate-off. V4 (C) recommended the flip on single-path cells only and
qualified it on exactly that step. The step's recorded reason (the wire-v8
global-`batch_seq` striping-gap misread, "amplified by longer same-path
arrival runs") is gone by construction in wire v9: the receiver's
`PathBatchTracker` keys on the per-path `path_seq`, so a same-path run of any
length leaves every delivered symbol's pair at `(1, 1)` on its own path
(bounded by `net::tests::t2_*`). The analysis behind this section is
AN4 (emission-batching scope, 2026-10-05). Committed before VM contact; no
number below is a result. Nothing is flipped by this battery.

**The law (Law 0).** A burst is at most one emission quantum of the sender,
whatever the live set:

```text
   b  =  emit_burst            (every N; no path-count or dial input)       emit_burst_bound
```

Provenance: `emit_burst` is the existing gate (`RWM_EMIT_BURST`, default 64,
clamped [2, 512]; "≈ 64 KB", a thin provenance, listed in the paper's
open-constants register). Shape: constant in N and every dial, so continuous
and monotone by construction; N = 1 is bit-identical to the step it replaces.
The taper/span cache refreshes once per `b` symbols or 50 ms
(`taper_recompute_due`), i.e. it follows the bound in force, and it was never
path-count-dependent (its inputs are aggregate / worst-path). The per-symbol
guards (store headroom, `cc_pace` token bucket) are unchanged.

**Engine commits** (before this one): `90a25e1` (the gauges and tests with
the step still in place), `027a753` (Law 0), `1671cae` (the harness). The
number 8 is assigned by the task; there is no §7. Gauges on `[DIAG]` (cumulative, last-line-wins;
printed whatever the gate, so a gate-off row reads `eb_bursts=0`):
`eb_bursts`, `eb_syms`, `eb_depth` (= syms/bursts), `eb_end=cap:/store:/
tokens:/drained:` (what ended each burst — `cap` is the bound's bind count,
rule 18) and `eb_maxrun=<pid>:<max>/<mean>` (per path, the longest same-path
run inside one burst: max, and mean over the bursts that touched the path).
Tests: T1/T6 `tests/emit_batch_scope_loopback.rs` (the shipped binary over
N = 1, 2, 4 loopback paths, `RWM_EMIT_BURST=8`: `np=N`, mean depth > 1, the
bound holds, the end tallies partition the bursts, bursts striped over ≥ 2
paths at N ≥ 2); T2 `net::tests::t2_*` (any interleaving of per-path runs →
expected == received on both paths, every pair `(1,1)`; the v8 global
numbering charges 4 phantom losses on the same order); T4
`net::emit_burst::tests::t4_*` (a burst of b symbols gets exactly one
recompute, for b ∈ {2, 8, 33, 64, 512}); the pin
`the_batching_path_reads_no_path_count` (the burst block and the emission
step read no `live_paths`; `emit_batch_live` exists nowhere). The default
stays OFF (`gates/tests.rs`).

**Component statement (rule 14).** On loopback (T1/T6, no shaping) the
burst intake is live at N = 1, 2, 4 with mean depth > 1 (the values are
printed by the test and recorded in RED.txt below). What the bench cannot
see: the receiver kernel's `rcvbuf` drops at 1 Gbit/s, quinn's per-path
pacer, and GE loss. The battery should then see: mean depth > 1 at every
cell including both duals; `eb_maxrun` per path well below the burst at the
duals (placement is anti-correlated inside a burst: `charge_in_flight`
pushes the next pick away from the path just chosen); CPU lower or equal;
the fed loss unchanged at c2/c8 and inside the band at c1s/c1d with
`rcvbuf` drops up.

**Binary and session.** One binary: this branch's HEAD, archived with `git
-c core.autocrlf=false -c core.eol=lf archive`, built fresh on the benchmark
VM, copied under its real name `raptorpath`, `sha256` recorded in
`BINSHA.txt` and re-verified before every invocation. Envelope
`tools/l1/emitscope_run_all.sh` (both locks for the whole session via
`lib_battery.sh`; build → tests → red/green record → smoke → GO → budget →
battery → score; hard backstop), driver `emitscope_battery.sh`, scorer
`emitscope_parse.py` (rows by `stage3_parse.make_row`).

**Tests first** (on the VM, inside the locks): `cargo build --release`;
`cargo test -p raptorpath -p raptorpath-math --release --no-fail-fast --
--test-threads=2`; `cargo test --doc -p raptorpath --release`; `cargo test
-p raptorpath-wasm` (`GOLDEN_CAPTURE` unset). rc and passed/failed/ignored
to `TESTS.txt`. A real failure (not a flake that passes on an immediate
re-run of that test alone; the V4-recorded `sigma_diag_reachability`
timing flake is the known class) is `ABORT-TESTS`. **Red/green record** (a
record, not a gate): T1/T6, T4, the pin and T2 with `--nocapture` on this
tree (green) and on the gauges-and-tests commit with the step still in place
(red: N = 2 and N = 4 must fail with `eb_bursts=0` / depth ≤ 1 and the pin
must fail on `emit_batch_live`), outputs to `RED.txt`.

**Cells**: `c1s-400`, `c1d-400`, `c2-100`, `c8-100` (§5's geometry, size,
capacity and > 5 % headroom; `c2-100`'s 11 % is the tightest, so CPU is the
free axis there, rule 16). `c8-100` is the discriminating cell: the July
regression (87 → 52 Mbit/s, rep 1, seed 42, v8) fired there; `c1d-400` is
where `rcvbuf` drops are expected to show.

**Arms**: `NEW` (bulk, gate off) and `EB0` (bulk, `RWM_EMIT_BATCH=1`, Law 0;
`RWM_EMIT_BURST` unset = 64). Every invocation is `perf_rwm_c.sh` with
`RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run,
`--window-reliable`, a fresh topology; every arm first `env -u`'s
`RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST` (rule 15d)
and only EB0 sets one back. **Plan per (rep, seed) block: 8 invocations**,
cells in the order above, the arm order within every cell rotated by the
block index (rule 3). Seeds 42 and 7; blocks rep 1 s42, rep 1 s7, rep 2 s42,
…; **n = 8 per seed per arm** (16 per arm and cell), cut only by the budget
rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`): §6's
(`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both; the RLC line both; no
generation guard; cadence ACTIVE both, OFF neither; `RWM_POOL_ANCHOR=0`
both); EB0: `[GATES] RWM_EMIT_BATCH=1` on both ends, the `emission batching
ACTIVE` echo on the client, and **mean burst depth `eb_syms/eb_bursts > 1`
on the client's last `[DIAG]` — at every cell, both duals included** (a row
at ≤ 1 is `WITNESS-FAIL eb-depth<=1`); NEW: `RWM_EMIT_BATCH=0` on both, the
echo on neither, `eb_bursts=0`; every row: the last `[DIAG]` `np=` equals the
cell's leg count (a dual that ran single-path is `WITNESS-FAIL`); the row's
`sha256` is the binary's.

**Scored quantities**: §6's (goodput, completion, DNF past 150 s, `CPUCLI`,
`[TRUTH]` per leg, `plc`), CPU per datagram = `CPUCLI` / Σ legs
`egress_dgrams` (reported, no MDE), and per leg the fed loss vs truth
`plc`/`[TRUTH] loss=`. **MDE** (§5's committed table, relative to NEW's
median): goodput / completion / CPUCLI = `c1s-400` 4.9 / 5.1 / 2.4 %;
`c1d-400` 5.6 / 5.3 / 6.5 %; `c2-100` 1.4 / 1.4 / 6.3 %; `c8-100` 4.0 /
4.1 / 11.6 %; DNF excess 0.20 at every cell.

**Per-cell clause set** (X = EB0, REF = NEW): **WORSE** iff goodput med(X) <
med(REF)·(1 − rel_gp), or completion p50(X) > p50(REF)·(1 + rel_ct), or
CPUCLI med(X) > med(REF)·(1 + rel_cpu), or dnf(X) − dnf(REF) > 0.20;
**FEED-MOVED** at a leg iff med(`plc`/truth) under EB0 is outside [1/1.3,
1.3] × NEW's (counts as worse); **BETTER** iff neither and (goodput above
med(REF)·(1 + rel_gp) or CPUCLI below med(REF)·(1 − rel_cpu)); **SAME**
otherwise; **UNSCOREABLE** at a cell where either arm has < 3 live rows or ≥
2 witness-failed rows, a feed ratio is unread, or an abort cause fired.
Reported beside every cell, not scored: `rcvbuf_drops` per run (median, max,
rows > 0) per arm — at `c1s-400`/`c1d-400` the feed clause is the score (it
is the channel the drops enter) and `rcvbuf_max` is printed beside it so a
pass "because it is a ratio of ratios" (V4's caveat) is visible; the burst
gauges per arm (depth median [min–max], the `eb_end` bind fractions summed
over rows, `eb_maxrun` per path); GSO per leg.

**Control identity.** NEW's goodput median per cell against V4 §6's NEW
[min, max] widened by the cell's absolute goodput MDE (rel_gp × V4 median):
`c1s-400` [442.1, 561.1], `c1d-400` [220.4, 407.5], `c2-100` [86.06,
90.54], `c8-100` [91.23, 109.37] Mbit/s. Outside is `CONTROL-MOVED`, named
beside that cell's verdict; it does not block the in-session comparison.

**Outcomes, in precedence order** (one verdict for the battery):
1. `UNSCOREABLE` — an abort cause fired.
2. **STOP RULE** `SCOPE-REFUTED-AT-C8` — at `c8-100` (scoreable), EB0 is
   WORSE on goodput or completion, or FEED-MOVED at either leg: the residual
   burst mechanism is real on v9 and Law 0 is wrong at the heterogeneous
   dual. Then Law A is the fallback arm (below).
3. `WORSE-AT-<cells>` — WORSE (any clause, CPU included) or FEED-MOVED at a
   scoreable cell.
4. `UNSCOREABLE` (naming cells) — a hard blocker at a cell.
5. `FLIP-RECOMMENDED` — BETTER at ≥ 1 cell, worse and feed-moved nowhere,
   every cell scoreable, and the depth witness held on every EB0 row. A
   recommendation only: the flip (default ON, `gates/tests.rs`, the echo
   test, `estimator.rs`/`sender_policy.rs` doc, the paper's ledger row, the
   harness's 15d lists) is a separate, reviewed commit.
6. `INERT-AS-DERIVED` — the witnesses fire and nothing moves beyond MDE.

**Prediction** (printed as a check, not an outcome): CPU BETTER at
`c1s-400` and `c2-100` (V4 (C) reproduced), SAME or CPU BETTER at
`c1d-400` and `c8-100`, feed unchanged at `c2-100`/`c8-100`, feed inside
the band at the c1 cells with `rcvbuf_max` up; overall `FLIP-RECOMMENDED`.

**Law A, the fallback arm (run only on `SCOPE-REFUTED-AT-C8`).**

```text
   N_eff  =  1 / Σᵢ pᵢ²            pᵢ = place_probs at burst start
   b      =  emit_burst / (N_eff · p_max)     clamped to [1, emit_burst]
```

"No path receives more than `emit_burst/N_eff` symbols of one burst in
expectation." Identity at N = 1 (b = emit_burst), = emit_burst at the
symmetric dual, continuous over the simplex and as a path dies (pᵢ → 0 ⇒
b → emit_burst). It is implemented behind a sub-gate only if the stop rule
fires, with the T3 shape test (N = 1 identity, symmetric dual, ±2 % nudges,
non-increasing in p_max at fixed N_eff, the membership edge), then run as
arm `EBA` vs `NEW` at `c8-100` and `c1d-400` with the clause set above, in
a second session under an amendment committed before that session. If Law A
also fails, `eb_maxrun` and the per-path `srtt` under EB vs NEW are read
before any mechanism is named.

**Budget** (5 h cap from the first ssh; hard backstop = first ssh + 4 h 50
min; soft = hard − 10 min). Priors: build + tests ≈ 35–45 min; red/green
record ≈ 10–15 min; smoke ≈ 1 min; per block `R_PRIOR` = 120 s (8
invocations at V4's measured ≈ 12 s mean plus margin; 16 blocks ≈ 32 min).
The smoke (`c8-100` EB0, `c1s-400` NEW, `c1d-400` EB0, `c2-100` NEW; `C_PRED`
= 60 s) sets `R_est` = `R_PRIOR`·max(1, c_meas/60 s); n per seed = min(8,
⌊(soft − now)/(2·R_est)⌋); n < 3 is `ABORT-BUDGET`. The battery starts no
block that would cross the soft deadline (`TRUNCATED-AT-REP-BOUNDARY`,
scored at the n reached). **Smoke pass** requires every row `LIVE` with its
CPU line, `[TRUTH]` per leg (`rcvbuf_drops` included), `plc` per leg, CPU
per datagram and the `eb_` gauges, both arms present, and **a dual EB0 row
with mean depth > 1** (the rule-1 witness at the dual proven before GO).
**GO** is automatic iff `TESTS-OK` and `SMOKE-PASS`; otherwise the operator
reads `TESTS.txt` and writes GO (a recorded flake) or NOGO.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD`, `ABORT-TESTS`,
`ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE`, `ABORT-BUDGET`,
`ABORT-RC` (row `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (`NO_DATA`
after 2 attempts). Void class `VOID-COTENANT`.

**Session rules.** §6's: both locks for the whole session; detached
envelope; earned sentinels (`DONE-ALL` only with `ES-BATTERY-DONE`,
`emitscope_parse.py check` rc 0 and no truncation); the operator reads only
sentinels (and `TESTS.txt` before a manual GO), waiting in bounded loops;
`pkill -x raptorpath` only; no `ens18`, firewall, `sshd` or non-`rp-*`
namespace is touched. Compact ledgers are copied to `docs/l1-raw/emitscope/`.

**Result** (scored 2026-10-05 against this pre-registration, literally; no
amendment was made): **`WORSE-AT-c1s-400,c1d-400`**, by the feed clause
only (FEED-MOVED at `c1s-400` p0 and at both `c1d-400` legs). The stop rule
did **not** fire: at `c8-100` EB0 is SAME on every clause and the feed is
unchanged at both legs, so Law A was not run. Nothing is flipped.

*Binary and session.* `788d2ef` (this section's pre-registration), built
fresh on the benchmark VM, `sha256
24fe4eca24b3c84742151a2b9354b875fec5cbd5d26dff5bd89d091bbfd3a28f`. The
envelope waited (bounded) for another agent's locks and took both at
00:43:05Z; build 00:43–00:52Z; tests 00:52–01:23Z; red/green record
01:23–01:25Z; smoke 01:25Z (`SMOKE-PASS`, 57 s); operator GO 02:01Z (below);
battery 02:01–02:28Z (1623 s, 128 invocations); locks released 02:28:48Z.
Session wall 1 h 46 min of the 5 h cap. Budget n = 8 per seed, no cut. The
battery was not polled. The VM was left with 0 `raptorpath` processes and 0
`rp-*` namespaces from this session (the next tenant took the locks seconds
later).

*Tests.* `cargo build --release` rc 0; the main suite rc 101 with 1042
passed, **1 failed**, 50 ignored (87 binaries); doc rc 0 (0 doc tests); wasm
rc 0, 35 passed. The one failure is `sigma_diag_reachability`, the
V4-recorded timing-flake class; re-run alone 3 times on the same tree it
passed 3/3 (`RED2.txt`), so it is a flake by the pre-registered rule, not
`ABORT-TESTS`. GO was written by the operator on that record (the
envelope's automatic GO requires `TESTS-OK`). The new tests passed on the
green tree: T1/T6 at N = 1, 2, 4 (mean depth 7.87 / 8.00 / 8.00 at burst 8;
mean per-burst longest same-path run 7.87 at N = 1, 4.4–5.6 per path at
N = 2, 1.7–1.9 per path at N = 4), T2 ×2, T4, the pin.

*Red record.* The envelope's first red run (`RED.txt`) is **void**: it
shared the green target directory and the red archive's old mtimes looked
fresh to cargo, so it re-ran the green artefacts (all green). The operator
re-ran it with the red sources touched (forced rebuild, `RED2.txt`), inside
the session's locks before GO: on `90a25e1` (step still in place) T1 is
**red at N = 2** (`eb_bursts=0`, "no burst at all", with `np=2 np_act=2`
on the same line) after passing N = 1 (depth 7.86), and the pin is **red**
("`emit_batch_live` is back"); T2/T4 green on both trees.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 00:43:05Z after a bounded wait) |
| `ABORT-CRLF` | no (0 CR bytes in `lib.sh` and the `emitscope_*` scripts) |
| `ABORT-BUILD` | no |
| `ABORT-TESTS` | no (one failure, a flake by the re-run rule) |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: 4 rows LIVE; the dual EB0 rows read depth 34.6 (`c8-100`) and 60.4 (`c1d-400`), `np=2` |
| `ABORT-BUDGET` | no (n = 8) |
| `ABORT-RC` | 0 of 128 |
| `ABORT-BRINGUP` | 0 |
| `VOID-COTENANT` | 0 of 128 |

*What ran.* 16 blocks × 8 = 128 rows, all `LIVE` (0 witness failures: the
depth > 1 witness held on all 64 EB0 rows, duals included; `np` = legs on
every row; `eb_bursts=0` on every NEW row); 16 per (cell, arm); 0 DNF.
Harness note: the scorer's ledger-header sha regex did not match the
header's two-token form, so it printed `BINARY sha256=-`; the driver's
per-invocation sha check is what held, and `BINSHA.txt` carries the sha. The regex was fixed after scoring (with the red-tree forced rebuild in
`emitscope_run_all.sh`); re-scored from `es.log` it reads the sha, 0
CONTAMINATED, and an identical verdict.
Ledgers: `docs/l1-raw/emitscope/` (`es.log` sha256 6b76201c…, `score.txt`
with every per-rep value, the smoke, `TESTS.txt`, `RED.txt`, `RED2.txt`,
`PLAN.txt`, `BINSHA.txt`, `GO`, `all-era.txt`).

*Per cell* (EB0 vs NEW, n = 16 each; goodput median Mbit/s; CPUCLI median
s; §5 relative MDE):

| cell | goodput NEW → EB0 | completion | CPUCLI NEW → EB0 | µs CPU / dgram | plc/truth per leg NEW → EB0 (band) | rcvbuf drops med (max) NEW → EB0 | clause |
|---|---|---|---|---|---|---|---|
| `c1s-400` | 436.6 → 446.3 (+2.2 %, within 4.9 %) | within | 8.44 → **6.77** (−19.8 %) | 24.8 → 19.8 | 2.43 → **3.96** ([1.87, 3.16]) MOVED | 54 (121) → 117 (244) | **FEED-MOVED** (CPU better) |
| `c1d-400` | 277.5 → **391.4** (+41.1 %, MDE 5.6 %) | 11.53 → **8.18 s** (−29.1 %) | 18.29 → **11.74** (−35.8 %) | 52.3 → 33.7 | 1.33 → **1.84**, 1.34 → **2.20** ([1.03, 1.73]) MOVED both | 14 (100) → 39 (112) | **FEED-MOVED** (goodput, completion, CPU better) |
| `c2-100` | 88.40 → 88.72 (+0.4 %, within 1.4 %) | within | 4.01 → **3.63** (−9.6 %, MDE 6.3 %) | 43.3 → 39.1 | 1.007 → 1.000 unchanged | 0 → 0 | **BETTER** (CPU) |
| `c8-100` | 101.4 → 99.1 (−2.2 %, within 4.0 %) | within (+2.2 %, MDE 4.1 %) | 6.71 → 5.96 (−11.2 %, within 11.6 %) | 69.4 → 62.8 | 1.003 → 1.001, 0.991 → 1.006 unchanged | 0 → 0 | **SAME** |

Both seeds agree in direction at every cell (per-seed medians in
`score.txt`). Control identity: NEW is IN-BAND at `c1d-400`, `c2-100`,
`c8-100` and **`CONTROL-MOVED` at `c1s-400`** (436.6 against V4's band
[442.1, 561.1]; session drift, named beside that cell; the in-session
comparison stands). **Prediction** (a check): `MISSED`. The feed left its
band at both c1 cells (predicted inside), and `c1d-400` moved on goodput
(+41 %) where SAME-or-CPU was predicted; `c1s-400`/`c2-100` CPU BETTER and
`c8-100` SAME with the feed unchanged were as predicted.

*Burst gauges* (EB0; rule-18 bind fractions summed over rows):

| cell | depth median [min–max] | `eb_end` cap / store / tokens / drained | `eb_maxrun` per path: max (median over rows) / mean per-burst longest run |
|---|---|---|---|
| `c1s-400` | 57.2 [56.2–57.6] | 0.795 / 0.205 / 0 / 0 | p0 64 / 57.2 |
| `c1d-400` | 57.0 [54.4–60.3] | 0.813 / 0.186 / 0 / 0 | p0 64 / 7.9; p1 64 / 7.6 |
| `c2-100` | 20.6 [19.9–20.9] | 0.006 / 0.994 / 0 / 0 | p0 64 / 20.6 |
| `c8-100` | 33.8 [32.5–35.7] | 0.126 / 0.873 / 0 / 0.001 | p0 (fast) 64 / 14.4; p1 (slow) 29 / 1.8 |

The token guard never ended a burst (inert, as derived). At the loss-bound
cells the store headroom, not the bound, ends 87–99 % of bursts. At the c1
cells the bound binds about 80 % of the time, so there Law 0 operates as its
constant (rule 18: the value 64 is the open constant). Inside a dual burst
the runs are short (mean longest run 7–8 of ~57 symbols at `c1d-400`; 1.8 on
the slow `c8-100` leg): placement stripes the burst rather than lengthening
same-path runs. The July "longer same-path arrival runs" hypothesis does not
describe what bursting does on v9.

*What it means.* Removing the path-count step costs nothing at the
heterogeneous dual. The cell where the July regression fired is SAME on
every clause with an untouched loss feed, and the symmetric 1 Gbit/s dual
gains the most from batching measured anywhere (+41 % goodput, −36 % sender
CPU). What blocks a flip is the c1 loss feed. Under batching the receiver
kernel drops about twice as many datagrams per run (`rcvbuf` medians 54 →
117 at `c1s-400`, 14 → 39 at `c1d-400`). No engine token counts them, so they
enter `plc` as path loss and the fed loss moves to 1.4–1.6× NEW's (2.2–4.0×
the wire's). That is V4's finding 1 (the receiver-saturation term), now
large enough to cross the pre-registered band. It is not the v8
striping-gap misread, which v9 removed (the c8/c2 feed ratio sits at
1.00). Under the pre-registered precedence the verdict is
`WORSE-AT-c1s-400,c1d-400` and no flip is recommended. The next lever is the
receiver side (the rcvbuf drop channel), after which this battery can re-run
unchanged.

**Amendment (re-run)** (committed before any VM contact of the re-run
session; no number below it is a result yet). §8 re-run on `927bb00`
(`main`; includes the §7 receive-buffer fix, `SO_RCVBUF` = 4 000 000 B on
every endpoint socket); design unchanged. Same arms (`NEW`, `EB0`), cells
(`c1s-400`, `c1d-400`, `c2-100`, `c8-100`), seeds (42, 7), n = 8 per seed per
arm, witnesses, MDEs, clause set, stop rule, outcome vocabulary, budget and
abort causes as above; the same harness (`emitscope_{run_all.sh,battery.sh,
parse.py}`, unchanged since `641a674`). The binary is this branch's tree
(the engine of `927bb00`), archived and built fresh on the VM in a fresh
target directory; the red/green record runs on `90a25e1` as before. The
re-run result is appended below as "Result (re-run)"; the first Result above
stands as the pre-fix record.

**Result (re-run)** (scored 2026-10-05 against this pre-registration and the
re-run amendment, literally; no further amendment): **`FLIP-RECOMMENDED`**.
EB0 is BETTER at every cell (`c1s-400` CPU, `c1d-400` goodput + completion
+ CPU, `c2-100` CPU, `c8-100` CPU), WORSE on no clause anywhere, the feed is
unchanged at every leg, every cell is scoreable, and the depth witness held
on all 64 EB0 rows. The stop rule did not fire; Law A was not run. **Nothing
is flipped**: the flip (default ON, `gates/tests.rs`, the echo test,
`estimator.rs`/`sender_policy.rs` doc, the paper's ledger row, the
harness's 15d lists) is a separate, reviewed commit, not made here.

*Binary and session.* Tree `06ee1fb` (the re-run amendment on `927bb00`;
engine identical to `main`), archived with `git -c core.autocrlf=false -c
core.eol=lf archive`, built fresh on the benchmark VM in a fresh run root
and target directory (`/home/vibe/es2`), `sha256
57980f4b43cfc628d74366df560e7000c103d4513c7e87ed70fdc9e120bbae3d`
(`BINSHA.txt`; re-checked before every invocation). First ssh 03:40:30Z.
Another agent (the QUIC feeder battery) held both locks; a detached waiter
polled the lock files (bounded, give-up at hard − 2 h) and launched the
envelope when both were free: locks taken 04:30:24Z (attempt 1); build
04:30–04:35Z; tests 04:35–05:01Z; red/green record 05:01–05:12Z; smoke
05:13Z (`SMOKE-PASS`, 38 s); automatic GO 05:13:24Z (`TESTS-OK`); battery
05:13–05:35Z (1330 s, 128 invocations); locks released 05:35:34Z (the next
tenant took them one second later). Budget n = 8 per seed, no cut. The
battery was not polled. The VM was left with 0 `raptorpath` processes and 0
`rp-*` namespaces from this session.

*Tests.* `cargo build --release` rc 0; main suite rc 0, **1049 passed, 0
failed**, 50 ignored (88 binaries); doc rc 0 (0 doc tests); wasm rc 0, 35
passed (`GOLDEN_CAPTURE` unset). `sigma_diag_reachability` did not recur.

*Red/green record* (`RED.txt`, valid this time: the red tree's sources were
touched, so cargo rebuilt them). Green (`06ee1fb`): T1/T6 at N = 1, 2, 4
(mean depth 7.90 / 7.96 / 8.00 at burst 8), T4, the pin, T2 ×2 pass. Red
(`90a25e1`): T1 passes N = 1 (7.88) and fails at N = 2 with `eb_bursts=0`
("no burst at all", `np=2` on the same line); the pin fails
("`emit_batch_live` is back"); T2/T4 green.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 04:30:24Z after a bounded wait on the other tenant) |
| `ABORT-CRLF` | no |
| `ABORT-BUILD` | no |
| `ABORT-TESTS` | no (0 failures) |
| `ABORT-SHA` | no |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: 4 rows LIVE; the dual EB0 rows read depth 33.07 (`c8-100`) and 56.82 (`c1d-400`), `np=2` |
| `ABORT-BUDGET` | no (n = 8) |
| `ABORT-RC` | 0 of 128 |
| `ABORT-BRINGUP` | 0 |
| `VOID-COTENANT` | 0 of 128 |

*What ran.* 16 blocks × 8 = 128 rows, all `LIVE`; 0 CONTAMINATED, 0
WITNESS-FAIL, 0 NO_DATA; 16 per (cell, arm), 8/8 per seed; 0 DNF.
Ledgers: `docs/l1-raw/emitscope/rerun-927bb00/` (`es.log` sha256
33c05247…, `score.txt` with every per-rep value, `smoke.log`,
`smoke-check.txt`, `TESTS.txt`, `RED.txt`, `PLAN.txt`, `BINSHA.txt`, `GO`,
`all-era.txt`).

*Per cell* (EB0 vs NEW, n = 16 each; goodput median Mbit/s; CPUCLI median
s; §5 relative MDE):

| cell | goodput NEW → EB0 | completion | CPUCLI NEW → EB0 | µs CPU / dgram | plc/truth per leg NEW → EB0 (band) | rcvbuf drops med (max) NEW → EB0 | clause |
|---|---|---|---|---|---|---|---|
| `c1s-400` | 498.1 → 517.8 (+4.0 %, within 4.9 %) | 6.42 → 6.18 s (−3.8 %, within 5.1 %) | 7.54 → **5.63** (−25.3 %, MDE 2.4 %) | 22.2 → 16.5 | 1.021 → 1.011 ([0.785, 1.327]) unchanged | 0 (0) → 0 (0) | **BETTER** (CPU) |
| `c1d-400` | 301.6 → **425.4** (+41.1 %, MDE 5.6 %) | 10.61 → **7.52 s** (−29.1 %) | 16.94 → **11.10** (−34.5 %, MDE 6.5 %) | 48.6 → 31.9 | 1.032 → 1.005, 0.993 → 0.976 ([0.794, 1.342], [0.764, 1.291]) unchanged | 0 (0) → 0 (0) | **BETTER** (goodput, completion, CPU) |
| `c2-100` | 88.29 → 88.85 (+0.6 %, within 1.4 %) | within (−0.6 %) | 3.68 → **3.25** (−11.8 %, MDE 6.3 %) | 39.2 → 34.8 | 1.005 → 1.006 unchanged | 0 (0) → 0 (0) | **BETTER** (CPU) |
| `c8-100` | 99.87 → 100.91 (+1.0 %, within 4.0 %) | within (−1.0 %, MDE 4.1 %) | 6.27 → **4.98** (−20.5 %, MDE 11.6 %) | 65.3 → 52.9 | 1.003 → 1.002, 0.994 → 1.010 unchanged | 0 (0) → 0 (0) | **BETTER** (CPU) |

`rcvbuf` drops are 0 in all 128 rows (0/16 rows > 0 in every cell and arm).
Both seeds agree in direction at every cell (per-seed medians in
`score.txt`). Control identity: NEW is IN-BAND at all four cells (`c1s-400`
498.1 in [442.1, 561.1], the first run's CONTROL-MOVED is gone). DNF 0 in
both arms everywhere.

*Burst gauges* (EB0; rule-18 bind fractions summed over rows; NEW reads
`eb_bursts=0` on all 64 rows):

| cell | depth median [min–max] | `eb_end` cap / store / tokens / drained | `eb_maxrun` per path: max (median over rows) / mean per-burst longest run |
|---|---|---|---|
| `c1s-400` | 58.6 [58.2–59.1] | 0.838 / 0.162 / 0 / 0 | p0 64 / 58.6 |
| `c1d-400` | 57.8 [55.2–59.3] | 0.831 / 0.169 / 0 / 0 | p0 64 / 7.7; p1 64 / 7.4 |
| `c2-100` | 20.5 [19.8–20.8] | 0.005 / 0.994 / 0 / 0 | p0 64 / 20.5 |
| `c8-100` | 33.3 [32.0–34.3] | 0.118 / 0.881 / 0 / 0.001 | p0 (fast) 64 / 14.4; p1 (slow) 30 / 1.75 |

GSO per leg (NEW → EB0): `c1s-400` 9.47 → 9.36; `c1d-400` 3.46 → 4.92 and
3.45 → 4.89; `c2-100` 4.77 → 5.00; `c8-100` 4.14 → 4.56 and 1.59 → 1.81.

**Prediction** (a check): `MISSED` in two sub-clauses, the overall outcome
as predicted. CPU BETTER at `c1s-400` and `c2-100`, the feed unchanged at
`c2-100`/`c8-100` and inside the band at the c1 cells, and
`FLIP-RECOMMENDED` all held. `c1d-400` moved on goodput (+41 %, as in the
first run) where SAME-or-CPU was predicted, and `rcvbuf_max` did not go up:
it is 0 in both arms, because the §7 fix (written after this prediction)
removed the drop channel.

*What it means.* The first run's only blocker was the c1 loss feed, and its
cause was the receiver kernel's receive-buffer drops counted as path loss.
With the 4 MB `SO_RCVBUF` on `main`, the receiver drops nothing under
batching either (0 in 64/64 EB0 rows), and the fed loss matches the wire's
under both arms (`plc`/truth 0.98–1.03). The fix's mechanism executed on
every row: the per-invocation endpoint logs (`/home/vibe/es2/run/diag-es/`,
read after the session, not by the scorer) carry 384 `[RCVBUF] req=4000000
granted=8000000 via=SO_RCVBUF clamped=0` echoes, exactly one per socket per
end (128 rows, 192 legs, both ends), and no other value. The batching gains of the first run
reproduce at the same size: `c1d-400` +41.1 % goodput and −34.5 % sender
CPU, −25 % CPU at `c1s-400`, and now a CPU gain beyond MDE at `c8-100`
(−20.5 % against 11.6 %), the cell where the July regression fired. Under
the pre-registered precedence the verdict is `FLIP-RECOMMENDED`. It is a
recommendation only. The flip is a separate, reviewed commit.

**Flipped in `bf3a636`** (the separate commit the Result names; nothing
re-measured). `RWM_EMIT_BATCH` resolves ON when unset; the shipped form is
the measured one, Law 0 with `RWM_EMIT_BURST` unchanged at 64 (it stays an
open constant, §3.4: the bound ends ~83 % of bursts at the c1 cells).
`[GATES]` reads `RWM_EMIT_BATCH=1` (the byte pin re-pinned deliberately);
the `=0` position now prints its own echo, `emission batching OFF
(RWM_EMIT_BATCH=0: per-symbol emission)`, keyed on the knob rather than on
the composed policy, so the control arm is distinguishable from a run the
scope excludes (that prints `emission batching out of scope`). The gate's
effects, all through `SenderPolicy::emit_batch_on` (`gates.emit_batch &&
reliable && !coded_wire && !use_packing`): the burst intake loop
(`net/mod.rs`, `[EMIT-BURST-BEGIN]`…`[EMIT-BURST-END]`), the per-burst
taper/span refresh (`net/emit_source.rs`, `taper_recompute_due` with the
50 ms `TAPER_CACHE_MAX_AGE_US`), the DIAG burst gauges and the echoes. All
of it ran in the EB0 arm; nothing unmeasured rides the gate. Not measured
by §8 and shipped by the same law: the Auto hint (EB0 ran Bulk; the burst
loop reads no δ; the cached taper/span values are Auto's own, refreshed per
burst exactly as at Bulk). Outside
its scope, at ρ < 1 and at Realtime packing, batching steps off: that is
the pre-existing `reliable` / `use_packing` debt of §3.3, which now names
it. Harness: the per-symbol control arms carry an explicit
`RWM_EMIT_BATCH=0` (emitscope `NEW`, verify4's non-`EMB` arms and its
tunnel arms); arms that mean "the shipped default" (stage3, `tun_bulk.sh`
when the caller sets nothing) now batch; tail_matrix `ship` runs Realtime,
outside the scope, and its `prior` arm already carried `=0`. Tests:
`gates/tests.rs` (default ON, burst 64, the echo),
`tests/emit_batch_default_loopback.rs` (the shipped binary, both positions).
