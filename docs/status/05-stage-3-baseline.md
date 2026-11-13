## 5. Stage-3 baseline — pre-registration

The pre-registered baseline on the fixed binary (the plan's "Then Stage 3"):
an A/A noise floor, block vs window at bulk and auto, the crown
no-regression spot, and one estimator-cadence arm. Committed before VM
contact; no number below is a result. Nothing is flipped by this battery.

**Binary.** Built fresh on the benchmark VM from the commit that carries this
section (engine tree = `main` e74891d; the later commits touch `tools/l1` and
docs only), `cargo build --release --bin raptorpath`, copied under its real
name, `sha256` recorded by the envelope in `BINSHA.txt` immediately before
the smoke and re-verified before every invocation (the source commit is in
the archive's `COMMIT` file and in every ledger header).

**Harness.** Envelope `tools/l1/stage3_run_all.sh` (both locks for the whole
session, build → smoke → operator GO → budget → battery → crown → score, a
hard backstop), driver `tools/l1/stage3_battery.sh`, scorer
`tools/l1/stage3_parse.py` (its constants are the rules below; offline test
`test_stage3_parse.py`), crown via `crownspot8.sh` / `tail_matrix.sh` and
`blockretest_parse.py crown`. Every invocation is `perf_rwm_c.sh` with
`RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, a fresh
topology; `RWM_EST_CADENCE` and `RWM_POOL_ANCHOR` are `env -u`'d on every arm
and set only on CAD (rule 15d).

**Cells.** Every name carries geometry and size and is used in every table.
Shaped capacity and the headroom above the last measured window-bulk goodput
(rule 16) are stated; every cell has more than 5 % headroom, so goodput may be
scored in both directions everywhere.

| cell | geometry (`lib.sh scenario_params`) | size | capacity | last measured (window bulk) | headroom |
|---|---|---|---|---|---|
| `c1s-400` | c1 single: 1 Gbit, 1 ms, GE 0.05/50 | 400 MB | 1000 Mbit | v2 284.8–293.9 | ≈ 70 % |
| `c1d-400` | c1 ‖ c1 dual | 400 MB | 2000 Mbit | V3 B2 186.9–201.5 | ≈ 90 % |
| `c2-100` | c2 single: 100 Mbit, 5 ms, jitter 3, GE 1.3/50 | 100 MB | 100 Mbit | V3 B2 88.9–89.3 | ≈ 11 % |
| `c3-25` | c3 single: 20 Mbit, 20 ms, jitter 5, GE 2/40 | 25 MB | 20 Mbit | V3 B2 17.05–17.48 | ≈ 13 % |
| `c7-100` | c2 ‖ c2 dual | 100 MB | 200 Mbit | v2 169.2–177.8 | ≈ 12 % |
| `c8-100` | c2 ‖ c3 dual | 100 MB | 120 Mbit | V3 B2 99.5–103.0 | ≈ 15 % |

(`c7-100` and `c8-100` are not §4's `c7` (200 MB) and `c8` (25 MB); nothing
here is compared with §4's numbers except as a cross-era remark.)

**Arms** (one binary):

| arm | pipeline | hint | extra env | role |
|---|---|---|---|---|
| `A1`, `A2` | window (`--window-reliable`, RLC) | bulk | — | the CTL, run twice as two independent arms: the A/A |
| `CAD` | window | bulk | `RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0` | part (d) |
| `BLKb` | block (RaptorQ, block ARQ) | bulk | — | part (b) |
| `WINa` | window | auto | — | part (b) |
| `BLKa` | block | auto | — | part (b) |

`CAD` pins `RWM_POOL_ANCHOR=0` explicitly: unset, the pool anchor follows
`RWM_EST_CADENCE` (`resolve_pool_anchor`), which would change the N ≥ 2 store
law as well; D1's decisive arm (dual c1 176–194 → 242–260 Mbit/s) was this
isolated pair. The composed form (pool anchor riding the cadence) is not
tested here.
*(After the result: `83462ae` decoupled the pool anchor — it no longer
follows the cadence and resolves off unless `RWM_POOL_ANCHOR=1` — and flipped
`RWM_EST_CADENCE` on, so the shipped default is exactly the `CAD` arm.)*

**Plan per (rep, seed) block: 32 invocations**, cells in the order below, the
arm order within every cell rotated by the block index (rule 3):
`c1s-400` A1 A2 BLKb WINa BLKa · `c1d-400` A1 A2 CAD · `c2-100`, `c3-25`,
`c7-100`, `c8-100` each A1 A2 CAD BLKb WINa BLKa. So part (a) runs at all six
cells, (b) at the five cells other than `c1d-400` (WIN at bulk is the CTL,
A1 ∪ A2), (d) at the five cells other than `c1s-400` (the task's four plus
`c7-100`, the cell whose clause the composed cadence flip once failed,
`estimator.rs` doc comment). Seeds 42 and 7; blocks run rep 1 seed 42, rep 1
seed 7, rep 2 seed 42, … **Planned n = 5 per seed** (10 per arm and cell; the
CTL is 20 rows).

**Budget** (5 h cap from the first ssh; hard backstop = first ssh + 4 h 50
min; soft = hard − 10 min). Priors per invocation (transfer + ≈ 8 s harness
overhead, from V1b/V2/V3/D1 and §4's 35 s per completing invocation): a block
is ≈ 15 min typical, **`R_PRIOR` = 20 min** allowing DNFs at the auto cells;
sync + build + smoke + GO ≈ 30 min; crown ≈ 33 min (§4: 1959 s), reserved as
35 min. 10 blocks = 200 min, total ≈ 265 min. The smoke's summed invocation
wall `c_meas` against its predicted 128 s sets `R_est = R_PRIOR ·
max(1, c_meas/128)`, and n per seed = min(5, ⌊(soft − now − crown reserve) /
(2·R_est)⌋). **Cut order**, each applied only while n < 3: (1) crown reps 8
→ 6 (reserve 27 min); (2) drop CAD (R_est × 27/32); (3) drop the crown. n < 2
after all cuts is `ABORT-BUDGET` (nothing runs). The battery starts no (rep,
seed) block whose `R_est` would cross soft − crown reserve
(`TRUNCATED-AT-REP-BOUNDARY`, scored at the n reached). Priority is (a) >
(b) > (c) > (d); (d) rides inside the same blocks because its control is
byte-identical to (a)'s arms at the same cells, so it costs 5 invocations a
block and is cut before the crown.

**Witnesses per invocation** (a row failing one is `CONTAMINATED` or
`WITNESS-FAIL`, excluded from scoring and counted): the driver's `pipeline=`
and `hint=` header and the `[PIPE]` echo on both endpoints match the arm;
`[GATES]` on both endpoints; the RLC auto-select line present on both
endpoints of window arms and absent on block arms; no generation guard line;
the estimator-cadence echo (`estimator heavy-math cadence ACTIVE`) on both
endpoints of `CAD` and on neither endpoint of every other arm; `[GATES]
RWM_POOL_ANCHOR=0` on both endpoints of every arm; `sha256` unchanged. A row
without a client summary is `NO_DATA`.

**Scored quantities** per invocation (`stage3_parse.py row`): goodput `mbps`
(whole-transfer mean, bytes·8/seconds), completion `seconds`, DNF (past
150 s); sender CPU `CPUCLI` (whole invocation) and `util` = CPUCLI/seconds;
invocation wall; busy share = median over the run of the client `[DIAG]`
`busy=` (last value also recorded); per leg the truth loss from `[TRUTH]
loss=` (rule 19; netem counters are never read as truth), the fed loss `plc=`
from the client's last `[DIAG]` (cumulative), their ratio, `plu=` (median
over the run's snapshots; "on the floor" iff in [0.0350, 0.0360]); coded
repair share = cod/(src+cod) from the last `cum=`; `rcvbuf_drops`.
Goodput, completion, CPU, util and busy are read over completed rows only; a
DNF enters the DNF rate. Per-seed medians and every per-rep value are printed
(rule 4); scoring is pooled over seeds.

**(a) A/A noise floor.** For each cell and each metric m ∈ {goodput,
completion, CPUCLI, util, busy}, over completed rows:

MDE(cell, m) = max( 2·|med(A1) − med(A2)|, ½·(max − min) of A1 ∪ A2 ),
rel(cell, m) = MDE / med(A1 ∪ A2).

The second term floors the statistic so that a lucky A/A median agreement
cannot shrink it to zero. States: `MDE-COMMITTED`; `NOISE-BOUND` if rel >
0.25 (the pair is too noisy to resolve anything there; every clause on that
metric at that cell reads `UNSCOREABLE`); `MDE-UNDEFINED` if either arm has
fewer than 3 completed rows (likewise `UNSCOREABLE`). "Beyond MDE" means
outside ref·(1 ± rel) with ref the comparison's reference median. The DNF
threshold at a cell is max(0.20, 2·|dnf_rate(A1) − dnf_rate(A2)|). The
committed MDE table is part of the result, and every later comparison on
these cells cites it. The A/A medians are also set beside the last measured
values (table above) as a cross-era remark, not scored.

**(b) Block vs window at bulk and auto** (cells `c1s-400`, `c2-100`, `c3-25`,
`c7-100`, `c8-100`; WIN = A1 ∪ A2 at bulk, WINa at auto; BLK = BLKb / BLKa;
the relative MDE of the cell and metric is applied to BLK's median; the A/A
is window-bulk, so at auto this is a transfer, recorded as weaker). At each
(cell, hint) the window is **worse** iff any clause fails:
goodput med(WIN) < med(BLK)·(1 − rel_gp); completion p50(WIN) >
p50(BLK)·(1 + rel_ct); DNF rate(WIN) − rate(BLK) > the cell's DNF threshold.
Where BLK completed no row the goodput and completion clauses are vacuous;
where WIN completed none and BLK some, both fail. Outcomes (no other verdict
may be recorded for (b)):
- **`WINDOW-NOT-WORSE`** ⇔ no clause fails at any (cell, hint) and every
  (cell, hint) is scoreable.
- **`BLOCK-BETTER-AT-<cell/hint,…>`** ⇔ any scoreable clause fails; it lists
  every failing (cell, hint).
- **`UNSCOREABLE`** ⇔ any of the first five abort causes fired (whatever
  the clauses say), or (naming the cells) no scoreable clause fails but
  some (cell, hint) cannot be scored: an arm with fewer than 3 live rows or
  ≥ 2 witness-failed rows of an (arm, cell) (the whole (cell, hint)), or a
  `NOISE-BOUND`/`MDE-UNDEFINED` goodput or completion metric (those clauses
  only; the DNF clause still scores).
Per (cell, hint) the reading `WIN>BLK` / `TIE-WITHIN-MDE` / `BLK>WIN` is
also printed (goodput beyond MDE in either direction), descriptive only.

*The recorded Auto-on-block c3 finding* (§3.7: congestion-window-bound at
7.2 Mbit/s) is not scored as a regression unless it moves outside the
recorded band widened by the c3 MDE: band [6.31, 7.63] Mbit/s (the union of
V1b 7.14–7.27, V2 6.62–7.63 and V2's same-day base 6.31–7.52), widened to
[6.31·(1 − rel_gp(c3-25)), 7.63·(1 + rel_gp(c3-25))]. BLKa's median at
`c3-25` inside → `AUTO-BLOCK-C3-AS-RECORDED`; below, or a majority of DNFs →
`AUTO-BLOCK-C3-REGRESSED`; above → `AUTO-BLOCK-C3-MOVED-UP`; fewer than 3
live rows or no MDE → `AUTO-BLOCK-C3-UNSCOREABLE`. It also enters (b)'s auto
comparison at c3 like any other cell.

**(c) Crown no-regression spot** (same session, after the battery):
`tail_matrix.sh` arm `ship` (env unset), hint realtime, no
`--window-reliable`, cells c2 and c3, 400 B and 1200 B, ×8 reps (×6 if cut),
seeds 42 and 7, 50 msg/s × 20 s — exactly §4's spot with §4's bands, scored
by `blockretest_parse.py crown`: `REPAIRS-INERT-ON-CROWN` (the crown held:
every cell-size-seed's p99 median inside c2·400B [34–199] s42, [34–56] s7;
c2·1200B [35–57] s42, [35–169] s7; c3·400B [87–154] s42, [88.5–297] s7;
c3·1200B [84.3–175] s42, [90.8–139.1] s7; p50 median in 7.0–9.0 ms (c2) or
22.0–27.0 ms (c3); `count = 1000` in ≥ 62 of 64 reps (≥ 46 of 48 at ×6) and
none below 995; the scorer's 62-of-64 constant is for ×8 and is applied by
hand as 46 of 48 if the cut fires), else `CROWN-MOVED(cell, seed, metric, direction)` (an
improvement also counts as moved); fewer than 6 reps with a summary at any
cell-size-seed is `SPOT-UNSCOREABLE`; not run by the budget rule is
`SPOT-NOT-RUN`.

**(d) `RWM_EST_CADENCE`: CTL (A1 ∪ A2) vs CAD** at `c1d-400`, `c2-100`,
`c3-25`, `c7-100`, `c8-100`. Per cell, relative MDEs applied to the CTL
median: CAD is **worse** iff goodput med < CTL·(1 − rel_gp), completion p50 >
CTL·(1 + rel_ct), CPUCLI med > CTL·(1 + rel_cpu), or DNF rate excess > the
DNF threshold; **better** iff not worse and (goodput med > CTL·(1 + rel_gp)
or CPUCLI med < CTL·(1 − rel_cpu)) — CPUCLI is admitted as the free axis
because c2/c3/c7/c8 sit within 11–15 % of their ceilings. The **fed loss vs
truth** clause, per leg: med(plc/truth) under CAD must lie within
[1/1.3, 1.3] × med(plc/truth) under CTL (a ratio of ratios, so the control's
own c1 excess — V3: 0.89–1.41 at c1 dual leg 1, a few tens of datagrams
tracking `RcvbufErrors`, and ±5 % from the 4-decimal `plc` print at truth ≈
0.001 — does not fail CAD by itself). Outcomes, in precedence order:
- **`UNSCOREABLE`** — any of the first five abort causes fired;
- **`WORSE-AT-<cells>`** — worse at any cell that has no hard blocker (an
  arm with fewer than 3 live rows, or ≥ 2 witness-failed rows);
- **`FEED-MOVED-AT-<cell:leg,…>`** — the feed clause fails at any leg of
  such a cell;
- **`UNSCOREABLE`** (naming cells) — at any cell a hard blocker, a
  `NOISE-BOUND` / `MDE-UNDEFINED` goodput, completion or CPU metric, or an
  unread feed ratio;
- **`FLIP-RECOMMENDED`** — better at ≥ 1 cell, not worse at any, the feed
  unchanged at every leg, every cell scoreable. It recommends flipping
  `RWM_EST_CADENCE` on **with the pool anchor decoupled** (`RWM_POOL_ANCHOR`
  stays off; today unset it follows the cadence), as a separate reviewed
  commit;
- **`INERT-AS-DERIVED`** — the witness fires, nothing moves beyond MDE.
Also reported, not scored: `plu` per leg (share of rows on the 0.0354 floor,
CTL vs CAD — D1 saw CAD pull it to ≈ 0.001, which moves the rate law's input,
so CAD is not a pure CPU change) and the coded repair share.

**Known effects, declared so they are not misread.**
1. Honest SACK (738008c, wire v9) releases less at wide spans than the old
   lying report did; lower release at dual cells is expected.
2. Live-path membership (e1ce7e7) removed the store-cap 128 cliff; the old
   empty-set ticks are gone.
3. Per-path loss has been honest since wire v9: balanced striping at `c1d-400`
   costs ≈ 1.34× sender kernel CPU (§3.7), and `c1d-400` goodput sits below
   `c1s-400` on the window pipeline; the dual c1 sender is CPU-bound (busy
   88–97 %).
4. Since the RX-slot fix (F4, 487ca7b) the receiver packs more acks per QUIC
   packet at c2/c3/c8 (1.24 → 1.81 frames/packet at c2) and fewer at c1
   dual; receiver CPU is ≈ 30 % lower at c2/c3/c8. The message count is
   unchanged.
5. `plu` sits on the BOCD floor 0.0354 at c1 and c2 in the CTL (§3.7).
6. Auto-on-block at c3 is cwnd-bound at ≈ 7.2 Mbit/s (§3.7), scored only as
   above; c8 Auto-on-block has been bimodal (32.7/35.2 vs 57.6, §3.7).
7. The CTL at `c1d-400` drifted between D1's batteries (Q1 186–191, Q2
   191–198, Q2b 176–194 Mbit/s); the A/A exists to measure exactly this.
8. §4's `BLOCK-BETTER-AT-…` ran before the CPU fix (1b890e0) and the Auto
   block fixes; it is not a prior for this battery's direction.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK` (either lock), `ABORT-CRLF` (`lib.sh` or a
battery file carries CR), `ABORT-BUILD` (the fresh build fails), `ABORT-SHA`
(binary changed; checked at start and before every invocation),
`ABORT-SENTINEL-UNWRITABLE` (probed at launch), `ABORT-SMOKE` (one invocation
per arm — `c2-100` A1, `c7-100` A2, `c1d-400` CAD, `c8-100` BLKb, `c7-100`
WINa, `c3-25` BLKa, seed 42 — plus one `tail_matrix.sh ship` 400 B rep at
c2; every row `LIVE` with its CPU line, one `[TRUTH]` per data leg and, on
window arms, a `[DIAG]` carrying `plc`/`plu`/`busy`/`cum` per leg; nothing
in it is a result), `ABORT-BUDGET` (above), `ABORT-RC` (non-zero driver
exit: that row is `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (no
summary after 2 attempts: `NO_DATA`). Void class `VOID-COTENANT`: a
`cargo`/`rustc` process on the box before or after an invocation voids it.
The first five (lock, CRLF, build, SHA, sentinel) and the smoke stop the
session before any row exists.

**Session rules.** Both locks via `lib_battery.sh` for the whole session;
detached envelope; earned sentinels (`DONE-ALL` only with a complete ledger,
`stage3_parse.py check` rc 0, no truncation, and the crown's own `DONE-ALL`
or its cut by the budget rule); the operator reads only sentinels (and the
smoke check, before GO), at most every 5 min; `pkill -x raptorpath` only; no
`ens18`, firewall, `sshd` or non-`rp-*` namespace is touched. Ledgers are
copied to `docs/l1-raw/stage3/`.

**Result** (scored 2026-09-30 against this pre-registration, literally; no
amendment was made): **(a) `MDE-COMMITTED` at every cell and metric;
(b) `WINDOW-NOT-WORSE`, with `AUTO-BLOCK-C3-AS-RECORDED`; (c)
`REPAIRS-INERT-ON-CROWN`; (d) `FLIP-RECOMMENDED`.** Nothing is flipped by this
battery.

*Binary and session.* Commit b3c6923 (this section's pre-registration; engine
tree = `main` e74891d), built fresh on the benchmark VM (Xeon E5-2650 v3
era) in 4 min 20 s, `sha256
f3743664cb48ad81deef239d44710f4b12bf08f2db65c6d90fc5a947ed500035` — byte-equal
to V3's NEW binary (2e264b7), as expected since only `tools/l1` and docs
changed since. First ssh 12:28:02Z (hard backstop 17:18:02Z); build
12:28–12:32Z; smoke 12:32–12:34Z; GO 12:36Z; battery 12:36–13:46Z (4198 s,
320 invocations, 13 s mean); crown 13:46–14:18Z (1958 s); locks released
14:18:46Z. **Session wall 1 h 51 min** of the 5 h cap. The budget rule gave
n = 5 per seed with no cut (`c_meas` 78 s < `C_PRED` 128 s, so `R_est` =
`R_PRIOR`). VM left quiet: 0 `raptorpath`, 0 `cargo`, 0 `rp-*` namespaces,
both locks absent.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken at 12:28:23Z, no `LOCK-TRUNCATED-BY-FOREIGN`) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script after sync) |
| `ABORT-BUILD` | no |
| `ABORT-SHA` | no (checked at start and before each of 326 invocations) |
| `ABORT-SENTINEL-UNWRITABLE` | no (17 paths probed at launch) |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 6 rows `LIVE` with every gauge, cadence echo 1/1 on CAD and 0/0 elsewhere, `POOL_ANCHOR` 0/0 everywhere, one `ship` rep |
| `ABORT-BUDGET` | no (n = 5, no cut) |
| `ABORT-RC` | 0 of 320 |
| `ABORT-BRINGUP` | 0 (0 `RUN-RETRY`, 0 `NO_DATA`) |
| `VOID-COTENANT` | 0 of 320 |

*What ran.* 10 blocks (5 reps × seeds 42, 7) × 32 invocations = 320 rows,
all `LIVE` (0 contaminated, 0 witness failures); 10 rows per (cell, arm), 5
per seed; 0 DNF anywhere. Crown: 64 reps. Ledgers: `docs/l1-raw/stage3/`
(`s3.log` sha256 7df3dd0a…, `crown/crown-s42.log` 44aebf97…,
`crown/crown-s7.log` e68fd0ce…, the smoke, `PLAN.txt`, `BINSHA.txt`,
`all-era.txt`, and the scorer's full output `score.txt` with every per-rep
value); per-invocation endpoint logs (32 MB) stay on the VM under
`/home/vibe/stage3/run/diag-s3`.

*(a) The committed MDE table* (window pipeline, bulk, A1 vs A2, n = 10 each;
MDE = max(2·|Δmed|, half-range of A1 ∪ A2); every later comparison on these
cells cites it). A/A DNF 0 everywhere, so the DNF threshold is 0.20 at every
cell.

| cell | goodput med (Mbit/s) | MDE goodput | MDE completion | MDE CPUCLI | MDE util | MDE busy |
|---|---|---|---|---|---|---|
| `c1s-400` | 303.2 | 14.8 (4.9 %) | 0.54 s (5.1 %) | 0.35 s (2.4 %) | 3.5 % | 3.8 pt (7.1 %) |
| `c1d-400` | 193.9 | 10.8 (5.6 %) | 0.88 s (5.3 %) | 1.89 s (6.5 %) | 2.0 % | 0.25 pt (0.3 %) |
| `c2-100` | 89.1 | 1.2 (1.4 %) | 0.13 s (1.4 %) | 0.31 s (6.3 %) | 6.4 % | 2.0 pt (10.8 %) |
| `c3-25` | 17.25 | 0.28 (1.6 %) | 0.19 s (1.6 %) | 0.22 s (9.6 %) | 9.1 % | 0.5 pt (7.1 %) |
| `c7-100` | 176.1 | 5.0 (2.8 %) | 0.13 s (2.9 %) | 0.25 s (3.6 %) | 4.8 % | 4.8 pt (5.7 %) |
| `c8-100` | 102.1 | 4.1 (4.0 %) | 0.32 s (4.1 %) | 0.85 s (11.6 %) | 11.4 % | 11.5 pt (24.9 %) |

Every entry is `MDE-COMMITTED`; the half-range term set the MDE everywhere
except `c2-100` busy (2·|Δmed|). The A/A medians sit inside or at the last
measured ranges (`c1d-400` 187.7–209.3 vs V3 186.9–201.5; `c2-100`
86.9–89.3; `c3-25` 16.9–17.5; `c8-100` 97.5–105.7), except `c1s-400`
282–312 vs V2's 285–294 (cross-era, not scored).

*(b) Block vs window* (goodput median [min–max] Mbit/s, n = 10 per arm, 20 for
the window at bulk; completion p50 in s; 0 DNF in every arm):

| cell / hint | window | block | window − block | reading |
|---|---|---|---|---|
| `c1s-400` bulk | 303.2 [282.1–311.8], 10.55 s | 267.9 [244.2–280.4], 11.94 s | +13.2 % | `WIN>BLK` |
| `c1s-400` auto | 247.4 [238.7–254.2], 12.93 s | 154.5 [150.6–160.1], 20.72 s | +60 % | `WIN>BLK` |
| `c2-100` bulk | 89.1 [86.9–89.3], 8.98 s | 89.6 [86.9–89.8], 8.93 s | −0.6 % (MDE 1.4 %) | `TIE-WITHIN-MDE` |
| `c2-100` auto | 73.1 [71.9–74.1], 10.94 s | 59.0 [56.7–59.6], 13.56 s | +24 % | `WIN>BLK` |
| `c3-25` bulk | 17.25 [16.9–17.5], 11.59 s | 15.98 [15.7–17.1], 12.52 s | +8.0 % | `WIN>BLK` |
| `c3-25` auto | 15.22 [14.6–15.5], 13.14 s | 7.21 [6.5–7.4], 27.74 s | +111 % | `WIN>BLK` |
| `c7-100` bulk | 176.1 [169.1–179.1], 4.54 s | 173.4 [166.1–176.6], 4.61 s | +1.5 % (MDE 2.8 %) | `TIE-WITHIN-MDE` |
| `c7-100` auto | 131.5 [126.5–141.6], 6.09 s | 116.7 [114.1–117.7], 6.85 s | +12.7 % | `WIN>BLK` |
| `c8-100` bulk | 102.1 [97.5–105.7], 7.83 s | 101.0 [91.7–103.0], 7.92 s | +1.2 % (MDE 4.0 %) | `TIE-WITHIN-MDE` |
| `c8-100` auto | 91.4 [83.9–95.2], 8.75 s | 56.8 [50.5–62.3], 14.09 s | +61 % | `WIN>BLK` |

No clause failed at any (cell, hint): **`WINDOW-NOT-WORSE`**. The window is
better beyond MDE at 7 of 10 and tied within MDE at the other three (bulk at
c2, c7, c8); both seeds' medians agree in direction at every row (per-seed
medians in `score.txt`). `AUTO-BLOCK-C3-AS-RECORDED`: BLKa at `c3-25` median
7.21 Mbit/s (s42 7.2–7.3, s7 6.5–7.4) inside [6.21, 7.75] (the recorded band
widened by 1.6 %). Headroom (rule 16): the best arm's goodput is 30 %, 89 %,
86 %, 88 % and 85 % of the shaped capacity at c1s, c2, c3, c7, c8.

*(c) Crown spot:* **`REPAIRS-INERT-ON-CROWN`**. p99 median / p50 median (ms),
all inside their bands: c2·400B 36.7/7.92 (s42), 38.1/7.91 (s7); c2·1200B
38.5/8.20, 39.1/8.22; c3·400B 105.5/24.05, 107.0/23.71; c3·1200B
96.6/25.16, 96.2/25.03. `count = 1000` in 64 of 64 reps. Single-rep
outliers inside the median rule: c2·400B s42 one rep p99 371.8 ms; c2·400B s7
165.8; c2·1200B s7 190.8.

*(d) `RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0` (CAD, n = 10) vs CTL (A1 ∪ A2, n =
20)*:

| cell | goodput CTL → CAD | CPUCLI CTL → CAD | busy CTL → CAD | plc/truth per leg CTL → CAD | `plu` on floor CTL → CAD | coded share CTL → CAD |
|---|---|---|---|---|---|---|
| `c1d-400` | 193.9 → **274.1** (+41 %, MDE 5.6 %) | 28.87 → **19.58 s** (−32 %) | 87 → 83 % | 1.07 → 1.01; 1.03 → 1.09 | 20/20 → 0/10 (CAD `plu` ≈ 0.0010) | 0.00044 → 0.00040 |
| `c2-100` | 89.09 → 89.11 (within) | 4.88 → **3.93 s** (−19 %) | 18.5 → 16.5 % | 1.00 → 1.00 | 20/20 → 0/10 (0.013) | 0.0048 → 0.0049 |
| `c3-25` | 17.25 → 17.40 (within) | 2.25 → **1.97 s** (−12 %, MDE 9.6 %) | 7 → 7 % | 1.00 → 1.00 | 0/20 → 0/10 (0.043 → 0.027) | 0.0106 → 0.0107 |
| `c7-100` | 176.1 → 176.3 (within) | 6.79 → **5.81 s** (−15 %) | 83.5 → 67.5 % | 1.00 → 1.00; 1.00 → 1.00 | 12/40 → 0/20 | 0.0054 → 0.0063 |
| `c8-100` | 102.1 → 103.7 (within) | 7.35 → **6.27 s** (−15 %, MDE 11.6 %) | 46 → 42 % | 1.00 → 1.00; 1.02 → 0.99 | 18/20 → 0/10 (fast leg) | 0.0075 → 0.0086 |

Not worse at any cell on goodput, completion, CPU or DNF; better on goodput
at `c1d-400` and on sender CPU beyond MDE at all five cells; the fed loss
against per-datagram truth unchanged at every leg (every CAD ratio within
0.99–1.09, inside [1/1.3, 1.3] × CTL's); every cell scoreable:
**`FLIP-RECOMMENDED`** — flip `RWM_EST_CADENCE` on with the pool anchor
decoupled (`RWM_POOL_ANCHOR` stays off, whereas today, unset, it follows the
cadence), as a separate reviewed commit. **Flipped in `83462ae`**: the
pool anchor no longer follows the cadence (`resolve_pool_anchor` defaults off,
pinned by `pool_anchor_does_not_follow_the_estimator_cadence`) and
`RWM_EST_CADENCE` defaults on, so the default is the measured `CAD` form and
not e84ef1c's composed one. D1's dual-c1 gain reproduced (D1:
242–260; here 258.5–329.0). The rate law's input moves with it: `plu` leaves
the 0.0354 BOCD floor at every cell where the CTL sat on it, while proactive
coded output stays negligible (coded share ≤ 0.9 % in both arms), so at these
cells the move does not show up as coded repair.

*Outside the pre-registered set (findings, no verdict).*
1. **The block/window picture reversed against §4.** §4 (older binary, before
   the CPU fix and the Auto block fixes) read `BLOCK-BETTER-AT-c1,c2,c3,c7,c8`
   with the window DNF'ing c2 at bulk; on this binary the window pipeline is
   not worse anywhere and ahead at every auto cell and at c1s/c3 bulk. §4's
   pre-registration tied `WINDOW-NOT-WORSE` to the ADR-0069 flip; this
   battery pre-registered that nothing is flipped, so the flip (and the block
   deletion) is a separate decision this result now supports.
2. **The block pipeline spends less sender CPU at bulk** where both saturate
   the link: CPUCLI BLKb vs window at `c7-100` 4.2–4.6 vs 6.6–7.1 s, at
   `c8-100` 4.3–4.8 vs 6.8–8.5 s, at `c2-100` 3.7–4.0 vs 4.6–5.2 s. CAD closes
   part of that gap. Not scored (the (b) clauses are goodput, completion and
   DNF).
3. **Auto on block is slow everywhere, not only at c3**: its goodput is a
   fraction of the window's at auto (BLKa/WINa medians: c1s 0.62, c2 0.81, c3
   0.47, c7 0.89, c8 0.62), though it completed every run (the §4 Auto DNFs
   are gone).
4. `c1d-400` CTL busy sits at 86–87 % in all 20 rows, so its busy MDE is
   0.25 pt: the gauge is saturated there (the sender is CPU-bound), a
   degenerate reading rather than a quiet one.
5. The block pipeline prints no `[DIAG]`, so its fed loss against truth is
   not measured by this harness (the truth column is).
6. Kernel receive-buffer drops (`rcvbuf_drops`) reach 40–59 per run at
   `c1s-400` on the window arms (2–4 on block) and up to 51 at `c1d-400` CAD;
   the c1 fed/truth ratios (1.0–1.4 at c1s, `plc` 0.0016–0.0018 vs truth
   0.0012) carry that term, as §3.8 recorded.

*What it means.* With the fixes in, the noise floor on these cells is small
(goodput MDE 1.4–5.6 %), and against it the window pipeline is at least as
good as the block pipeline at bulk and clearly better at Auto on every cell;
the crown did not move. The per-ack BOCD update is a real cost: batching it
lifts CPU-bound dual c1 by about 40 % and cuts sender CPU 12–32 % everywhere
without changing what the loss estimator is fed. Both results are
recommendations for separate, reviewed commits; this battery changed no
default.
