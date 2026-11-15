## 21. Receiver per-datagram cost, R1 with the [LAT] floor fix (R2) — pre-registration

§17 scored R1 `REFUTED-WITH-RECORD (FAIL-AT-c1s-400)`; §20 diagnosed the
collapse (H1 CONFIRMED: the `[LAT]` `SeqRing` store pads and trims across a
dead duplicate record, O(gap) per in-order message) and validated the fix as
a diagnostic arm (`FIX` recovered at c1s, 483–599 Mbit/s against R1's 177).
This section re-runs §17's battery, unchanged in design, on **R2 = R1 + the
`[LAT]` floor fix, merged onto current main**. Committed before any battery
VM contact (the red/green runs and the suites below were run on the VM under
both locks; they are tests, not results). No number below the "Dev evidence"
paragraph is a result. Nothing is flipped by this battery; shipping is the
operator's merge.

**What R2 is.** `main` 4e50504 plus `archive/thread-r1` (a5ef8b2, based on
ce60079), merged (`b30d80f`), plus two commits:
- `daa719e` the red test `net::lat::tests::a_dead_duplicate_does_not_make_the_store_cost_o_gap`
  (§20's proposal: 50 000 in-order arrive/deliver pairs, a duplicate of a seq
  delivered 1 000 earlier every 5 600 arrivals; after each in-order insert the
  ring slot span `last_key − first + 1 ≤ 2`; after each `note_delivery(d)` no
  record below `d` is held);
- `c33638c` the fix (`fix-lat-floor.patch`): `Pending` keeps `floor` = one past
  the last in-order delivery; `note_delivery(d)` takes `d`'s record and prunes
  every record below the floor (`SeqRing::prune_below`); `note_arrival`
  refuses a seq below it; both are counted in `over=` as a FIFO eviction is.
  `debug_assert!(seq >= floor)` in `note_delivery` pins the non-decreasing
  delivery premise. No law reads `[LAT]`; no threshold, hint or mode is
  introduced (NO-MODE-SWITCH untouched).

R1's other changes ride along unchanged (see §17): (a) the inbound `Vec`
pool, (b) `lat_arrivals` removed, (c) the `SeqBits` ring bitmap, (e)
`received_ids` built only when sent, `[RXSEC]` under `RWM_RTOBS=2`
(never set in the battery), `tests/recv_alloc_bound.rs` (bound 23.5) and
`tests/inbound_pool_reachability.rs`. **Merge resolutions.** `status.md`:
main's text kept (it already holds §17 verbatim and §18–§20). `net/tests.rs`
`q2_one_task_per_loop_no_multiplexer`: both sides had fixed the source pin
(main `fa0f915`, R1 `a402054`); main's version kept (it names the
`task_obs::timed` wrapper for all four loops). No store-cap file differs
from main (`store_cap.rs`, `sender_policy.rs`, `store_cap_bench.rs`,
`store_cap_sf_bench.rs`, `formula_agreement.rs`: ADR-0072's one law at every
N is intact). `fec-arq-model.md` gains only R1's one sentence on the
per-path inbound pool.

**MAIN moved.** §17's MAIN was ce60079. MAIN here is 4e50504, which
contains ADR-0072 (the pooled store law at N = 1): at c1s the cap is no
longer the constant 1024 (§19: `c1s-400` reads `PINNED` at the 2048 knee in
most windows). R2 inherits the same law, so MAIN and R2 differ only in R1's
receiver changes plus the fix; §17's c1s figures are not the reference.

**Dev evidence (not a result).**
- *Red / green* (VM, release, inside both locks, tree `daa719e` then
  `c33638c`). Red on the merged R1 `lat.rs`: `a_dead_duplicate_does_not_make_the_store_cost_o_gap`
  FAILED at seq 5 600, the first arrival after the first duplicate, "ring slot
  span 1002 after an in-order insert" (bound 2; the gap grows to 16 381 by the
  FIFO's end, §20). Green with the fix: `ok` in the release suite and in the
  debug suite (the `debug_assert` armed). The older
  `the_ring_store_matches_the_hash_map_it_replaced` is unchanged and green
  (it drives `take` directly, not the delivery floor).
- *Suites, tree `acc0ed0` (engine = `c33638c`), fresh target, inside the
  locks:* build 362 s; release `cargo test -p raptorpath -p raptorpath-math
  --release --no-fail-fast -- --test-threads=2` **1 105 passed, 0 failed**,
  50 ignored (95 binaries, 2 013 s); doc rc 0; `cargo test -p raptorpath-wasm`
  35 passed (136 s); debug `cargo test -p raptorpath --no-fail-fast --
  --test-threads=2` **963 passed, 1 failed**, 49 ignored (86 binaries,
  4 621 s, `IDENTITY-PANICS 0`); parsers rc 0 (`test_threadr2_parse` 39
  checks). The one failure, `place_arms_reachability::the_derived_temperature_arm_resolves_a_finite_t_eff_on_the_dual`
  (T_eff read the shipped 0.15 on a loopback run where the cold rule fired
  on 49 % of resolutions; a timing-dependent place-arm test, no `[LAT]` or
  receiver code on its path), **passed 3 of 3 on an immediate solo re-run**
  (debug, `--test-threads=1`): `FLAKE` by the pre-registered rule, and it is
  noted here, not hidden. Run solo, both profiles, all `ok`:
  `recv_alloc_bound` (bound 23.5), `inbound_pool_reachability`,
  `reliable_delivery_hold`, `lock_order_loopback`, `ack_delivery`,
  `io_owner_topology`, `store_cap_bench` (the ADR-0072 law, 6 passed, 3
  ignored). The `SeqBits` / SACK differential tests (`net::seq_bits::tests`,
  `net::tests::the_wire_laws_read_the_ring_bitmap_as_the_btreeset`) are in the
  lib runs above.
- Windows host `cargo check -p raptorpath --all-targets` (under the host
  mutex, `CARGO_BUILD_JOBS=1`) rc 0. One warning is new against main and is
  R1's: `unused variable batch_path_id` at `net/receiver.rs:1188` (the
  `lat_arrivals` removal left the binding dead). It is left as merged so the
  tested tree is the one measured; delete it in the shipping merge.

**Mechanism and predictions** (rule 11). §17's derivation stands: what is
removed per Data message on the receiver task is two `Vec`s, a `BTreeSet`
node and its walks, a SipHash `HashMap` insert/remove, and per owner poll the
growth of a fresh inbound `Vec`. §20 adds the one thing §17 missed: the
`[LAT]` store must be O(1) when a dead record exists, and the floor makes it so.
- **c1s — named prediction: goodput SAME as MAIN or better** (R2's median not
  beyond MAIN's median·(1 − 4.9 %) with disjoint ranges). A c1s gain is not
  predicted (the receiver is not the c1s ceiling; §17); a collapse would
  refute the fix.
- **c1s mechanism witness: the server's cumulative `[LAT]` `over=` of every
  R2 row lies within [min, max] of MAIN's `over=` at c1s-400** (§17: MAIN
  23–34, collapsed R1 51–77; §20 diagnostic reads varied 29–110 on MAIN
  itself, so the band is wide). Read per run as `WITNESS c1s_lat_over`
  (`WITHIN` / `OUTSIDE` / `UNREAD`). It is a mechanism witness, **reported,
  not outcome-gating**: `over=` counts dead records, which depends on how
  many duplicates a run draws, and the fix counts them sooner than a FIFO
  eviction would. `OUTSIDE` is reported with its numbers and does not
  decide the outcome.
- **c1d (receiver-bound).** §17's prediction is kept as written: goodput
  BETTER with the server `[RDIAG]` busy median below 90 %. **Note: §17 measured
  the c1d receiver at 100 % busy on both arms** (median 100 on MAIN and R1),
  so the busy clause is the likely miss; PARTIAL is the expected reading.
  Goodput TREND-WORSE −6.2 % at c1d in §17 was within the pass band.
- **CPUSRV per byte BETTER at c1s and c1d** (MET if BETTER at both, PARTIAL if
  both at least TREND-BETTER).
- **c2 / c8: WITHIN** on goodput and both CPU per byte (§17: c8 CPUSRV
  −11.9 %, goodput +11.6 %; c2 CPUSRV TREND-WORSE +9.6 %).
- **CPUCLI WITHIN; RTprop floor and `[LAG]` p99 WITHIN or better.**
The four predictions are reported, named `PREDICTION` lines
(`c1d_goodput_better`, `cpusrv_better`, `c1s_goodput_within`,
`c2_c8_within`) plus the `WITNESS c1s_lat_over` line; they do not decide the
outcome.

**Design (copied from §17; nothing changed but the arm).**
*Cells* (§5's geometry, size, capacity and > 5 % headroom): `c1s-400`,
`c1d-400`, `c2-100`, `c8-100`. *Arms*: `MAIN` = 4e50504, `R2` = this
section's HEAD (the engine tree is `c33638c`'s; the commits after it are
the harness and this text), both bulk, `--window-reliable`, shipped defaults:
every arm `env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG RWM_IO_RT`, then
`RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1` on every arm (rule 15d), `perf_rwm_c.sh`
with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, a
fresh topology per invocation. **Plan per (rep, seed) block: 8 invocations**,
cells in the order above, arm order within a cell rotated by the block index
(rule 3). Seeds 42 and 7; **n = 3 per seed (6 per arm and cell)**,
interleaved, cut only by the budget rule.

*Witnesses per invocation* (a failing row is `CONTAMINATED` / `WITNESS-FAIL`,
excluded and counted; no client summary = `NO_DATA`): `stage3_parse`'s set
(`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both; the RLC line both; no
generation guard; cadence ACTIVE both; `RWM_POOL_ANCHOR=0` both);
`[GATES] RWM_EMIT_BATCH=1` both and the `emission batching ACTIVE` echo on the
client; the client's `[DIAG]` `wake[` token; `[GATES] RWM_RTOBS=1` and
`RWM_RDIAG=1` both; at least one in-transfer `[RDIAG]` line on the server;
the main runtime's `[THR]`/`[LAG]` window of the measured object and its
`[IOWN]` lines on both ends; the Q2 sender-lane witness on both arms
(4e50504 contains Q2); **the R1 execution witness, two-sided, as §17**: on
every R2 row every `[IOWN]` line of the window carries `ibp_hit=` on both
ends and the server's owners reused pooled `Vec`s (`ibp_hit` > 0 summed); on
every MAIN row no `ibp_hit=` token; the row's `sha256` is its arm's binary.

*Pass rule per cell* (§17's, R2 against MAIN): a clause is **WORSE** iff R2's
median is beyond MAIN's median·(1 ∓ rel) in the worse direction **and** the
two arms' [min, max] ranges are disjoint in that direction (TREND-WORSE =
beyond the band with overlapping ranges: reported, not a fail):

| clause | direction | rel |
|---|---|---|
| goodput ≥ MAIN | higher is better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| client CPU per byte ≤ MAIN + MDE | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| server CPU per byte ≤ MAIN + MDE | lower | the same, as a declared transfer (§11–§14) |
| RTprop floor ≤ MAIN + tolerance, per leg | lower | max(5 %, MAIN's half-range / MAIN's median) |
| `[LAG]` p99 ≤ MAIN + tolerance, client and server | lower | the same rule |
| fed loss = `[TRUTH]` | — | per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's |
| DNF | — | excess > 0.20 (§5) |

*Per-cell verdict*: **FAIL** iff any clause is WORSE, the feed moved or the
DNF excess fired; **UNSCOREABLE** iff R2 or MAIN has < 3 live rows or ≥ 2
witness-failed rows at the cell, a feed ratio is unread, or an abort cause
fired; **PASS** otherwise. Per arm: **PASS-EVERYWHERE**, **FAIL-AT-<cells>**,
or **UNSCOREABLE-AT-<cells>** (a FAIL outranks an UNSCOREABLE).

*Outcomes, in precedence order* (one verdict):
1. `UNSCOREABLE` — an abort cause fired.
2. `DELIVERED` — R2 passes at every cell: **ships** (the operator's merge).
3. `REFUTED-WITH-RECORD (FAIL-AT-<cells>)` — R2 fails somewhere: nothing
   ships; the per-clause readings are the record. **No tuning in this
   battery** (no re-run with a changed pool size, span cap, bound or floor
   rule; a follow-up is a new pre-registration).
4. `UNSCOREABLE-AT (…)` — R2 is not fully scoreable and fails nowhere.

*Reported, not gated* (per cell and arm, median [min–max], n): the client's
`[DIAG]` sender `busy`; the `[RDIAG]` busy and `q_avg` on both ends; the
server's pool `ibp_hit` / `ibp_miss`; the server's `[LAT]` `over=`;
`wake[..]`; the owner hops' datagrams per batch, `rx_capped`, `send_err`,
the lock-wait gauge; GSO factor per leg; acks per data datagram; `[THR]`;
parks per second.

*Abort causes, in priority order*: `ABORT-LOCK`, `ABORT-CRLF`,
`ABORT-BUILD` (either tree), `ABORT-TESTS`, `ABORT-SHA`,
`ABORT-SENTINEL-UNWRITABLE`, `ABORT-PRECONDITION` (load or cotenant, below),
`ABORT-SMOKE` (one invocation per arm at `c1s-400` and `c8-100`, seed 42:
every row LIVE with goodput, both CPU lines, `busy`, `[LAG]` p99 on both
ends, the owners on both ends, the server's `[RDIAG]` and `[LAT]` `over=`,
per leg `[TRUTH]`, `plc` and an RTprop floor, the client's `ack_dg`, and on
R2 rows the server's `ibp_hit`; both arms present; nothing in it is a
result), `ABORT-BUDGET` (n < 2 per seed after the budget rule), `ABORT-RC`
(that row `VOID-RC`, the battery goes on), `ABORT-BRINGUP` (no summary after
2 attempts: `NO_DATA`); void class `VOID-COTENANT` (a `cargo`/`rustc`
process before or after an invocation).

**Launch precondition and deadline** (§17 Amendment 2, adopted here from the
start). Both VM locks free, `/proc/loadavg` 1-minute value < 2.0, and no
`cargo` / `rustc` process (`pgrep -x`, never `-f`); the envelope checks the
load and cotenants before taking the locks and prints `ABORT-PRECONDITION`
otherwise. The loadavg at launch is in the ledger (`all-era.txt`, `TR2-ALL
start ... load=`). If the precondition does not hold within 1 h of the locks
freeing, the run is reported, not launched; another agent's locks are never
broken or taken over. **Hard deadline = launch + 4 h** (soft = hard − 10
min; inside the 5 h cap).

**Harness.** Envelope `tools/l1/threadr2_run_all.sh` (both locks for the
whole session; R2 build → tests → MAIN build → smoke → budget → battery →
score; hard backstop), driver `threadr2_battery.sh`, scorer
`threadr2_parse.py` (`TR2ROW`; offline test `test_threadr2_parse.py`, 39
checks), derived from `threadr1_*` (the arm, row, tag and path names are
renamed; the added code is the `[LAT]` `over=` column, its witness and its
smoke requirement). Ledgers to `docs/l1-raw/thread-r2/`. No operator GO gate
once the operator has said go: SMOKE-PASS proceeds.

**Tests first** (inside the locks, on the R2 tree): `cargo build --release`;
`cargo test -p raptorpath -p raptorpath-math --release --no-fail-fast --
--test-threads=2`; `cargo test --doc -p raptorpath --release`; `cargo test -p
raptorpath-wasm` (`GOLDEN_CAPTURE` unset); the debug run `cargo test -p
raptorpath --no-fail-fast -- --test-threads=2` (`IDENTITY-PANICS` = count of
`io owner: quinn seam`, must be 0); the python parser tests. A failure that
passes on an immediate solo re-run is `FLAKE`; any other is `ABORT-TESTS`.

**Budget.** Priors: R2 build ≈ 5–35 min (loaded), release + doc + wasm +
debug tests ≈ 45 min (quiet) to ≈ 1 h 50 (loaded), MAIN build ≈ 5–10 min,
smoke ≈ 1–2 min, `R_PRIOR` = 240 s per 8-invocation block (§17 measured the
scored battery at 550 s for 48 rows); Measured today on this VM, co-tenanted (loadavg up to 7.8): R2 build 362 s,
release 2 013 s, doc 15 s, wasm 136 s, debug 4 621 s = 1 h 59 min before the
MAIN build; plus MAIN build ≈ 8 min, smoke ≈ 2 min and the scored battery
≈ 10 min (§17: 550 s for 48 rows). **Expected session wall ≈ 2 h 20 min on
a loaded VM, ≈ 1 h 30 min on a quiet one (§17 priors); the 4 h hard deadline
leaves room for the loaded case.** n per seed = min(3, ⌊(soft −
now) / (2·R_est)⌋); the battery starts no block that would cross soft
(`TRUNCATED-AT-REP-BOUNDARY`, scored at the n reached).

**Session rules.** Both locks for the whole session (the envelope's
`take_lock` refuses when a lock is held, so it is launched only when both
are free); detached envelope; earned sentinels (`DONE-ALL` only with
`TR2-BATTERY-DONE`, `check` rc 0 and no truncation); the operator reads
`all-era.txt` at most once per ≈ 20 min (rule 13); `pkill -x raptorpath`
only; no `ens18`, firewall, `sshd` or non-`rp-*` namespace is touched; exit
state verified (0 `raptorpath`, 0 `rp-*` namespaces, both locks released).
Ledgers are copied to `docs/l1-raw/thread-r2/`.

### 21. Receiver per-datagram cost, R1 with the [LAT] floor fix (R2) — result

**Outcome: `DELIVERED` (R2 passes at every cell: ships).** Scored strictly as
pre-registered; no threshold, clause or prediction was changed after the data
was read. Session 2026-10-26 13:38–15:24Z, one envelope, both locks held
throughout (released 15:24Z). Launch precondition met: both locks free,
loadavg 0.97 (< 2.0), 0 cargo / rustc. Exit state 0 `raptorpath`, 0 `rp-*`
namespaces, VM dir removed. R2 = `73d868d` (engine `c33638c`) `sha256
f3c74848…a1477f`, MAIN = 4e50504 `sha256 4b2b9534…57f230`, each built fresh
(377 s, 298 s). Tests inside the locks: release **1 105 passed, 0 failed**
(50 ignored, 95 binaries), doc rc 0, wasm 35 passed, parsers rc 0, debug 962
passed + **2 failures, both `FLAKE`** (passed on the immediate solo re-run):
`place_arms_reachability::the_derived_temperature_arm_resolves_a_finite_t_eff_on_the_dual`
(the same one as the dev run) and `the_diag_line_reports_all_three_candidate_dispersion_gauges_beside_the_shipped_one`
(`tests/common/loopback.rs:431`), `IDENTITY-PANICS 0`; smoke passed (38 s);
battery 446 s: **48 rows, all 48 LIVE**, no void, no witness failure, no abort
token, no truncation, n = 6 per arm and cell. Ledgers:
`docs/l1-raw/thread-r2/` (`all-era.txt`, `tr2.log`, `score.txt`,
`TESTS.txt`, `BINSHA.txt`, `diag-tr2/`, `diag-smoke/`).

| cell | verdict | clauses (R2 vs MAIN, median [min–max]) |
|---|---|---|
| c1s-400 | PASS | goodput WITHIN 720.8 [687–746] vs 717.3 [638–754] Mbit/s (+0.5 %); CPUCLI WITHIN (−0.7 %); CPUSRV TREND-BETTER (−3.0 %); `[LAG]` p99 and RTprop floor WITHIN; feed SAME |
| c1d-400 | PASS | goodput TREND-BETTER 656.8 [611–686] vs 612.1 [592–647] (+7.3 %); CPUCLI WITHIN (−3.6 %); CPUSRV TREND-BETTER (−7.6 %); lag, RTprop WITHIN |
| c2-100 | PASS | goodput WITHIN (+0.2 %); CPUCLI WITHIN; CPUSRV TREND-BETTER (−7.6 %); RTprop floor TREND-BETTER |
| c8-100 | PASS | goodput WITHIN (−0.4 %); CPUCLI WITHIN; CPUSRV WITHIN (−9.9 %); lag, RTprop WITHIN |

**The §17 collapse is gone.** c1s R2 reads 687–746 Mbit/s in all six rows
(§17's R1: 185–192); the server receiver `[RDIAG]` busy median is 90 %
(MAIN 94 %). The fix does what §20 said, and R1's allocation work no longer
costs anything at N = 1.

**Predictions (reported, not gating).** `c1s_goodput_within` **MET**;
`c2_c8_within` **MET**; `c1d_goodput_better` **PARTIAL** (goodput
TREND-BETTER, +7.3 %, ranges overlap; the server `[RDIAG]` busy median is
99 % on R2 and 100 % on MAIN, not < 90 %: the c1d receiver is still
saturated, as §17 said); `cpusrv_better` **PARTIAL** (TREND-BETTER at c1s
and c1d, not BETTER).

**Witness `c1s_lat_over`: OUTSIDE** (R2 `over=` 45, 48, 58, 80, 82, 99 against
MAIN's 52, 54, 55, 56, 73, 84). Reported, not gating, as pre-registered.
The medians are close (R2 69, MAIN 55) and the spread on MAIN alone was
23–110 across sessions; one R2 run is above MAIN's maximum and one below its
minimum. It says the fix counts a similar number of dead records, not a
different order, and the throughput shows no O(gap) cost; it is not read as
a defect.

**What it does not show.** No claim is made that c1d moved beyond the §5
MDE (TREND-BETTER only), nor that CPUSRV improved by more than the MDE at
c1s and c1d. The receiver-bound c1d claim of §16 finding 4 stands. The
`batch_path_id` dead binding from R1 (`net/receiver.rs:1188`) is to be
deleted in the shipping merge. Nothing is merged or flipped here; shipping
is the operator's.
