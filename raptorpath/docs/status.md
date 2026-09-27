# Status

What ships today, the most recent verdicts, the open debts, and the next
pre-registered measurement. Rules for measuring are in
[measurement-discipline.md](measurement-discipline.md). The full measurement
ledger this replaces is in git history (ledger at ac1aed1).

## 1. The default stack (as the code resolves it)

**Pipeline routing** (`net/mod.rs` `is_window_mode`,
`(hint == Realtime || window_reliable) && backend.is_streaming()`):

| hint | default route | with `--window-reliable` |
|---|---|---|
| Realtime | window pipeline, unified RLC span machine, EVICT retention (ρ < 1) | window pipeline, retain-until-acked (ρ = 1) |
| Auto (the default hint) | **block pipeline**, RaptorQ, block ARQ | window pipeline, RLC auto-selected, retain-until-acked |
| Bulk | **block pipeline**, RaptorQ, block ARQ | as Auto |

`window_reliable` defaults to `false` (`config.rs`) and `fec_backend` to the
block-only RaptorQ; with the backend unset the window pipeline auto-selects RLC.
Pinned by `default_config_routes_bulk_and_auto_to_the_block_pipeline` (ADR-0069).

**Wire and substrate.** `PROTOCOL_VERSION = 8`; compact DATA framing
(`RWM_WIRE_COMPACT`) on. Substrate congestion control is quinn BBR
(`RWM_QUIC_CC`, default `bbr`). The wire-clocked Copa signal and Copa pacing
are off unless `RWM_QUIC_CC=passthrough`, `RWM_COPA_FEED` or `RWM_COPA_WIRE`
is set.

**Gates on by default** (`gates.rs` `RuntimeGates::resolve`, resolved once
and read everywhere through `gates::get()`; the default `[GATES]` echo is
byte-pinned by `gates_echo_default_is_byte_pinned`): `RWM_UNIFIED`,
`RWM_UNIFIED_SHED`, `RWM_TAPER_R`, `RWM_ASTAR_ANCHOR`, `RWM_MSTAR_ANCHOR`,
`RWM_HONEST_ANCHOR`, `RWM_STORE_SACK_RELEASE`, `RWM_STORE_PATHS`,
`RWM_GEN_PIPE` (only reached when generation is on), `RWM_RS_ATTR`,
`RWM_RECOV_MP`, `RWM_RECOV_MP_LAW`, `RWM_SUM_CAP`, `RWM_DELTA_CAP`,
`RWM_ACK_MERGE`, `RWM_WIRE_COMPACT`. `RWM_HONEST_CAP` resolves on but is inert
without `RWM_PLAIN_RS`, so `[GATES]` echoes its effective value, 0.

The recovery clocks (tail sweep, refresh, per-sequence cooldown) and the repair
margin pool over the live paths (`recovery_clock_paths`), not the
cwnd-saturation-filtered `active_paths()`. Boolean gates take one strict
dialect (`config::parse_bool`); an unrecognised value is a startup error.

Every other `RWM_*` gate is off by default (an experiment arm or an
instrument). The refuted arms `RWM_POOL_DELIV`, `RWM_FLOOR_BOUND`,
`RWM_PATIENCE_DERIVED`, `RWM_STORE_CAPW`, `RWM_STORE_PERCAP`,
`RWM_PERCAP_GUARD`, `RWM_STORE_BORROW`, `RWM_WIN_DECOUPLE`, `RWM_PLACE_SLACK`,
`RWM_RACK_CLOCKS`, `RWM_RACK_REO_MULT`, `RWM_QUANTILE_CLOCKS`, `RWM_W_FORM`
and `RWM_ALPHA_OVERRIDE` are removed; set in the environment they are ignored
(paper §10). `RWM_RECOV_SP` is kept as an off arm. Store gain 2.0, store
boot 128 and the per-path pool 2048 are the shipped store constants (§3.4).

## 2. Most recent verdicts

| measurement | design | verdict |
|---|---|---|
| Attribution audit (D0) | 4 cells, 3 reps, 12 invocations | `orig_frac` averages two mechanisms: true-heal share π0 is 0.0077 (c1) and 0.0054 (sc2) at single paths, 0.96 (c7) and 0.92 (c8) at duals |
| Crown no-regression spot (wire v8 merges) | tail_matrix `ship`, realtime, c2/c3, 400/1200 B, ×8, seeds 42+7 | Repairs inert on the crown at 7 of 8 cell-size-seeds; c3·400B seed 7 p99 median outside by 0.2 ms, at the pre-declared era-limited cell; the EVICT seat answers 1.8–3.1 repairs per abandoned hole, and 82–99 % of abandoned holes get their data after the give-up |
| r > 0 battery (Track B) | seed 42, n = 4 (truncated by the 5 h cap) | Glide R-FUNDED-NEGATIVE (direction only) at the lossy single; MID `VOID` by W7; entanglement-dominated where scoreable; control confirmed at the corner |
| Receiver-law battery (Track C) | n = 3 × 2 seeds | `REFUTED-WITH-RECORD` on every arm at both duals; KNEE-BOUND (ℓ* = 0, names `RWM_STORE_GAIN`); `CONTROL-MOVED` at c1; sc2 substrate collapsed |
| Placement battery (Track A) | n = 4 × 2 seeds | First readout MIXED: cross-path wait ≈ 6 % of delivered latency, queueing above the path floor the largest term; all arms `INERT-AS-DERIVED` on latency, goodput regressions under the guard; 0.15 is not a stable quantile |

The last three rows are the law-search batteries (the ledger's "Stage 2"). Their
conclusion: a latency win must come from the store/pacing law that
owns the queue, not from placement; the next law to derive is that one, and the
next measurement is its decomposition. No default flipped and no constant was
blessed.

## 3. Open items and debts

### 3.1 Instrument and substrate findings owed from the law-search batteries

1. **Fixed.** The receiver's self-heal estimator (`succ.rs`) counted the sender's retransmit as a self-heal, so π̂0 ≈ 1. A hole closed by a source copy stamped later than its exposer is now `HoleOutcome::Retransmit` (`rtx_n=` in `[SUCC]`, `rtx=` in `[LATE]`) and is excluded from π̂0; a lower bound on copies, no wire change. The receiver-law battery has not been re-run on the fix.
2. sc2 (clean 100 Mbit single path) did not finish 100 MB in 300 s on the shipped window machine (6/6, 8/8; ~2 Mbit/s once); bisect against the crown era. Open; see 3.5 (paper §11.1 open question 0).
3. The sender `[ETA]` has no exit flush; `eta_s4.py` uses RTprop where the law uses SRTT (routes disagree 3–4×); the σ̂ witness fails at c7/sc3.
4. c8 control shows a bimodal fast-path-alone collapse (4/8 reps), outside every pre-registered set; the likely source of c8's 75 % CV.
5. `RWM_COPA_DELTA` has no engine echo, so MID cannot be scored again.
6. `tracing` interleaves records onto readout lines; the diagnostic writer needs a newline discipline.
7. `r_report.py` does not propagate a W7 `VOID`, and its R-FUNDED-NEGATIVE branch fires on any null.

Also fixed with the cleanup: `[DIAG] cod=` counted source copies (gap
retransmits, request copies, taper copies) as coded repair. It now counts coded
symbols actually sent; copies go to `total_copy_symbols` and `retx=`. The r > 0
battery's "coded symbols are 86–90 % retransmits" was read on the old meaning.

### 3.2 Generation-coding stack: no disposition

`--window-generation-coding` (with DAPS, the rate-sample estimator, `RWM_GEN*`,
`RWM_OOO_RETAIN`) is opt-in, never default, and has no keep-or-remove decision.

### 3.3 NO-MODE-SWITCH debts (hint- or ρ-keyed code paths)

Each is a behaviour step at a preset point, which CLAUDE.md forbids.

| site | what it switches |
|---|---|
| `net/mod.rs` `is_window_mode` | the block/window pipeline fork (Bulk/Auto vs Realtime); §4 decides it |
| `net/emit_source.rs` (`protocol_hint == Realtime`) | Realtime duplicate source send, a redundancy decision priced by nothing |
| `net/sender_policy.rs` `use_packing` | symbol packing on Realtime only |
| `scheduler/copa.rs` `queue_target_mult` | Copa queue target 1.08 / 1.125 / 1.25 by hint |
| `gates/scheduler_gates.rs` `copa_compete_active` (`RWM_COPA_COMPETE`) | Copa's TCP-competitive mode switching |
| `reliable` boolean branches in the window sender/receiver | ρ = 1 vs ρ < 1 selecting code paths instead of composing with δ |

### 3.4 Open-constants register

All unprovenanced and uncorrected (correct value unknown). Source: paper §11.2.

| constant | value | where | why unprovenanced |
|---|---|---|---|
| tail-sweep / refresh clamp | `(2·srtt).clamp(25, 100) ms` | `net/recovery_laws.rs` `hole_nack_refresh` | undefeated, not derived; derivable from true-heal F only at duals |
| legacy age gate | `srtt/2` | `net/recovery_laws.rs` `legacy_age_ripe` | pre-RFC; compares flight age with a dwell-inclusive srtt; inert at c1 |
| RECOV_MP thresholds | `9/8·max(srtt, ewma)`, 3 packets | `net/recovery_laws.rs` `mp_time_threshold_split` | RFC 9002 values applied to an age that includes sender dwell |
| `GAP_ACK_MIN_INTERVAL` | 2 ms | `net/mod.rs` | a rate limit that is also the hole sampler; not derived |
| `NACK_RETX_COOLDOWN_FLOOR_US` | 10 ms | `net/mod.rs` | 10× kGranularity by choice; moves W by 6.8× across its range |
| `ELIGIBLE_SKEW` | 75 ms | `scheduler/mod.rs` | a threshold that selects a code path |
| taper copy `p_lost` | model-derived | `net/emit_source.rs` | measured waste is zero; why it never fires is undecided |
| store gain / recovery round | 2.0 / 100 ms | `gates.rs`, `net/store_cap.rs` `HONEST_RECOVERY_ROUND_S` | the headroom H is proportional to gain − 1; κ is a fit |
| heal/closed classifier | `srtt/2` | D0 instrument (offline) | biases π0 upward; gates nothing; the engine's `[LATE]` π̂0 uses sender-stamp order instead |
| `PLACE_TEMPERATURE` | 0.15 | `scheduler/place.rs` | argmax of a four-point sweep whose verdict failed |
| `w_div` | 1.0 | `scheduler/mod.rs` `SchedulingWeights::from_hint` | derived GE form gives 0.475 (c2) / 0.552 (c3) |
| unmeasured-path price | 10.0 | `scheduler/place.rs` `place_costs` | a hard exclusion (e^−66 odds) in a continuous costume |
| near-tie band / floor | 0.8 / 0.25 | `place_repair_spare_path` | a relative band plus an absolute floor: a missing scale |
| stall fraction κ in placement | 1 | `place_costs` | declared upper bound; measured 0.005–0.067 |
| `BULK_TAIL_BUDGET` | 0.05 | `raptorpath-math` | an "e.g." in the derivation promoted to a const |
| `queue_target_mult` | 1.08 / 1.125 / 1.25 | `scheduler/copa.rs` | declared corner; no continuous form fits the three points |
| block shape by hint | `BlockProfile::from_hint`, interleave 2/1/3 | `net/mod.rs`, `config.rs` | declared corners; interleave is non-monotone in δ |
| Realtime duplicate send | on at Realtime | `net/emit_source.rs` | a rate decision taken by a hint equality |
| `use_packing` | on at Realtime | `net/sender_policy.rs` | declared corner |
| receiver hold | `(4·srtt).clamp(60, 300) ms`; armed arm `srtt/2` | `net/shed.rs` `shed_recv_hold` | three constants; `srtt/2` equals b(δ_Realtime) by seat, not by evaluation |

### 3.5 Known issues from the cleanup

1. **Throughput regression on the current binary line.** sc2, sc3 and c7 run
   far below their earlier readings (item 3.1.2; paper §11.1 open question 0).
   Undiagnosed; a bisect against the competitive-baseline binary would decide it.
2. **seq 0 delivered but never pruned.** A lost seq 0 is now SACK-reported, but
   the sender cannot prune a delivered seq 0 until the cumulative ack reaches
   1, and a receiver that delivered only seq 0 advertises nothing new. Fixing
   it needs a wire change.
3. **Same-class `active_paths()` sites not changed.** The recovery clocks moved
   to the live set; these still read the cwnd-saturation-filtered set, which is
   empty when every path is cwnd-full: the react-cap SRTT, the
   NACK-budget and `repair_rate` worst-loss picks, the taper's ε at send
   (`emit_source`), and the Shutdown broadcast (sent only on active paths).
4. **`[DIAG] rtp` prints whole milliseconds.** At sub-millisecond RTprop
   (loopback) it prints `rtp0ms`, which cannot be told from an unset anchor,
   and at c1's 2 ms the rounding is coarse.
5. The recovery-clock bind fractions in paper §7.1 / §9.7 were measured on the
   saturation-filtered set and are not re-measured.

## 4. Block default re-test — pre-registration

Discharges ADR-0069's re-test clause. Committed before VM contact; no number
below is a result. Binary: `main` with the cleanup's bug fixes (including the
recovery-clock saturation-set fix) and refuted-arm removal merged, built fresh
on the VM, `sha256` recorded.

**Deviations from ADR-0069 part 4.** Today's harness cells replace C1–C5
(`perf_native.sh` cannot run duals); reps are cut for the 5 h cap; the rule is
"window not worse", so ADR-0069's C2 reversal clause (b) is not adopted (the
owner's decision: the one-machine design wins a tie).

**Arms** (interleaved within each rep, one binary, fresh topology per
invocation, driver `tools/l1/perf_rwm_c.sh`):
- `BLK` — the default: no `--window-reliable`, backend unset (block, RaptorQ).
- `WIN` — `--window-reliable`, backend unset (RLC), `RWM_GEN=0` (plain
  reliable window, generation off).

Setup owed before the VM, in its own commit: (a) a `perf_rwm_c.sh` switch that
drops `--window-reliable` and the generation flag for `BLK`; (b) a one-line
engine echo of the resolved pipeline and backend on both endpoints
(`pipeline=block backend=RaptorQ` / `pipeline=window backend=Rlc`) with a unit
test. Without (b) the witness is the presence/absence of the "reliable window
mode … auto-selecting RLC" line, which is one-sided and is recorded as weaker.

*Amendment (harness facts, no change to arms, cells or outcomes):* the switch
in (a) is implemented as `RWM_C_PIPELINE=block|window` on `perf_rwm_c.sh`
(default `window`); `block` drops `--window-reliable` and the generation flag,
and the `--- RWM-C perf` header line prints `pipeline=<arm>`. `c8` below always
means the 25 MB dual; the placement battery's 100 MB dual of the same geometry
is named `c8L`. `perf_native.sh`, named in the deviations above, was deleted
in 639929c; `RWM_C_PIPELINE=block` is the block driver.

*Amendment 2 (harness facts, committed before VM contact and before any
number exists; no change to arms, cells, n or outcomes):*
(i) The witness "`[GATES]` present with `RWM_GEN=0`" cannot fire on either
arm: `perf_rwm_c.sh` consumes `RWM_GEN=0` as its generation switch (drops
`--window-generation-coding`) and unsets it before forwarding, and the
engine's `[GATES] RWM_GEN=` token is the generation SIZE (default 384). The
witness is read as: `[GATES]` present on both endpoints, and generation off
shown by the absence of the driver's `GUARD OK: generation ACTIVE` line
(on WIN the receiver's unified-decoder echo `generation=false` is recorded,
not gated). (ii) The echo in (b) prints `[PIPE] pipeline=block|window
backend=<RaptorQ|Rlc> hint=<h>` once per engine start at the routing
decision; a row whose `[PIPE]` (either endpoint) or `pipeline=` header
disagrees with its arm is `CONTAMINATED`. (iii) Driver
`tools/l1/blockretest_battery.sh` under the envelope
`tools/l1/blockretest_run_all.sh` (which holds both locks for the whole
session, crown spot included); scorer `tools/l1/blockretest_parse.py`.
`ABORT-BRINGUP`'s retry protocol: an invocation with driver rc 0 and no
client summary is retried once (2 attempts), each retry logged `RUN-RETRY`.
(iv) Readings the outcome set leaves implicit, fixed here: goodput and
completion are read over completed reps only (a DNF has no goodput and
counts in the DNF clause); where `BLK` completed no rep the goodput clause is
vacuous like the completion clause; where `WIN` completed none and `BLK`
some, both clauses fail; "a failed witness on ≥ 2 reps of an arm-cell"
counts `CONTAMINATED` and witness failures over both hints and seeds; "live
reps" counts completed plus DNF rows. (v) Smoke cost `c` is the smoke's
mean wall time per invocation; the envelope's soft deadline (hard cap −
10 min) stops the battery at a rep boundary.

*Amendment 3 (committed after the first launch aborted and BEFORE any
battery row or crown-spot number was read; no change to arms, cells or
outcomes):* the first envelope launch (15:55Z, session start 15:31Z, hard
cap 20:31Z) ran the crown spot and then fired `ABORT-LOCK` at the start of
seed 42 — nothing of the battery ran. Cause: a co-tenant session locked the
same paths with `exec 8>PATH; flock -n 8`, which truncated our lock files
and, with no flock(2) holder, succeeded; its cargo build/test ran
15:58–16:24Z (through the whole crown spot) and its test suite until
~17:10Z. (i) The first crown spot is VOID (co-tenancy) and is not scored;
its ledgers stay on the VM. (ii) The relaunch holds flock(2) on both lock
paths for the session as well as the token files, re-writes the token
before each stage and logs `LOCK-TRUNCATED-BY-FOREIGN` if it finds it
emptied. (iii) A new void class `VOID-COTENANT`: an invocation with any
`cargo`/`rustc` process on the box immediately before or after it is void
(excluded from every denominator; not a witness failure). (iv) The smoke's
measured cost is `c` = 339 s / 4 = 84.75 s; relaunching at ~17:15Z leaves
~196 min to the cap, ~163 min after the ~33 min crown spot, below `120·c`
= 169.5 min, so by the pre-registered cut **n = floor(163 min / (40·c)) = 2
per seed** (the minimum): 80 invocations. The crown spot is re-run in the
relaunched session.

**Hints**: `bulk`, `auto`. **Seeds**: 42, 7. **Cells** (`lib.sh
scenario_params`; duals as in `perf_rwm_c.sh`):

| cell | geometry | size | shaped capacity |
|---|---|---|---|
| c1 | single, 1 Gbit, 1 ms one-way, GE 0.05/50 | 400 MB | 1000 Mbit |
| c2 | single, 100 Mbit, 5 ms, jitter 3, GE 1.3/50 | 100 MB | 100 Mbit |
| c3 | single, 20 Mbit, 20 ms, jitter 5, GE 2/40 | 25 MB | 20 Mbit |
| c7 | dual c2 ‖ c2 | 200 MB | 200 Mbit |
| c8 | dual c2 ‖ c3 | 25 MB | 120 Mbit |

`RWM_PERF_TIMEOUT_S=150` (at least 4× every cell's nominal transfer time); a
run past it is a DNF. `RWM_DIAG=1` on both arms.

**Budget.** 2 arms × 2 hints × 5 cells = 20 invocations per rep. Measured
cost is ~1.8–3.6 min per invocation (placement battery 1.81 min; r battery
3.59 min). n = 4 per seed (160 invocations, 290–580 min) does not fit, so
**n is cut to 3 per seed**: 120 invocations, ~220 min at 1.8 min. Plus build,
CR check and smoke (~25 min) and the crown spot below (~33 min): ~280 min.
If the smoke's measured cost `c` gives `120·c` above the time left, n per seed
becomes `floor(time_left / (40·c))` before launch, minimum 2, and is written
into an amendment commit. A detached stopper ends the battery at a balanced
rep boundary or at 5 h. Order: crown spot, then seed 42 reps 1–3, then seed 7.

**Witnesses per invocation, both endpoints**: the pipeline echo matches the
arm; `[GATES]` present with `RWM_GEN=0` (the smoke confirms the block path
prints it); `BLK` shows no window/RLC line, `WIN`
shows it; a `"summary"` line (else `NO_DATA`); `sha256` unchanged.

**Scored** per (cell, hint), pooled over seeds and also per seed:
- goodput: whole-transfer mean per run (`mbps` = bytes·8 / seconds), median
  and [min–max] over reps;
- completion time p50 over completed reps;
- DNF count.

**Outcomes** (no other verdict may be recorded):
- **`WINDOW-NOT-WORSE`** ⇔ at every (cell, hint), pooled and on each seed:
  `WIN` median goodput ≥ `BLK` minimum goodput (at or above the bottom of
  `BLK`'s own spread), `WIN` completion p50 ≤ `BLK` maximum completion, and
  `WIN` DNF ≤ `BLK` DNF (where `BLK` completed no rep, the completion clause
  is vacuous and the DNF clause decides). Then flip the default and delete
  block mode per ADR-0069 Appendix A (make `is_window_mode` unconditional, default backend
  RLC, block fallback becomes a config error, update the routing pin).
- **`BLOCK-BETTER-AT-<cell>`** ⇔ any of those three clauses fails at that
  cell for either hint. Stop: block stays the default, the cell, hint and
  numbers are recorded here, and the one-machine claim names the exception.
- **`UNSCOREABLE`** ⇔ fewer than 2 live reps per seed at any (cell, hint,
  arm) after aborts and voids, a failed witness on ≥ 2 reps of an arm-cell,
  or any of the first five abort causes below firing.

**Abort causes, in priority order**: `ABORT-LOCK` (either lock), `ABORT-CRLF`
(`lib.sh` not 0 CR bytes), `ABORT-SHA` (binary changed), `ABORT-SENTINEL-UNWRITABLE`
(probed at launch), `ABORT-SMOKE` (one invocation per arm at c2 and c8, bulk,
seed 42, must show every witness; nothing in it is a result), `ABORT-RC`
(non-zero driver exit: that invocation's rows are void, the battery goes on),
`ABORT-BRINGUP` (retries exhausted: `NO_DATA`, not an abort of the battery).

**Stated in advance.** Finding 3.1.2 (sc2 did not finish 100 MB in 300 s on
the shipped window machine) predicts that `WIN` at c2 may DNF or run far below
line rate. If so, the reading is `BLOCK-BETTER-AT-c2`, and the finding moves
from "substrate" to "the window pipeline" at that cell. c2 and c3 have run at
~100 % of shaped capacity on earlier binaries, so neither arm can gain goodput
there (MEASUREMENT DISCIPLINE 16); a loss is still visible, which is all a
not-worse rule needs.

**Crown no-regression spot, same session** (for the recovery-clock
saturation-set fix, which changes the default path): `tools/l1/tail_matrix.sh`
arm `ship` (env unset), hint `realtime`, no `--window-reliable`, cells c2 and
c3, sizes 400 B and 1200 B, ×8 reps, seeds 42 and 7, 50 msg/s × 20 s. Passes
(`REPAIRS-INERT-ON-CROWN`) iff, per cell-size-seed, the p99 median lies inside
the union of the committed spreads (the wire-v8 spot's [min–max] joined with
the older baselines: c2·400B [34–199] s42, [34–56] s7; c2·1200B [35–57] s42,
[35–169] s7; c3·400B [87–154] s42, [88.5–297] s7; c3·1200B [84.3–175] s42,
[90.8–139.1] s7), the p50 median lies in 7.0–9.0 ms (c2) or 22.0–27.0 ms (c3),
and `count = 1000` in ≥ 62 of 64 reps with none below 995. Otherwise
`CROWN-MOVED(cell, seed, metric, direction)`; an improvement also counts as
moved. Fewer than 6 of 8 reps with a summary at any cell-size-seed is
`SPOT-UNSCOREABLE`.

**Result** (scored 2026-09-27 against the pre-registration and amendments
1–3, literally): **`BLOCK-BETTER-AT-c1,c2,c3,c7,c8`**; crown spot
**`REPAIRS-INERT-ON-CROWN`**. Block stays the default; nothing is flipped.

*Binary and session.* Branch `cleanup/a4-block-retest` (engine at c2ab337
= `main` 5474e9f + the `[PIPE]` echo d58feab; later commits touch
`tools/l1` and docs only), built fresh on the benchmark VM (Xeon E5-2650 v3
era), `sha256 780616883fc5a242ab7b439dcd63d33d357f1d3d0c13141c1f546a3300713003`,
re-verified after the targeted tests (lib 509 passed incl.
`pipe_echo_names_the_route_the_engine_takes`; `protocol_test` 8,
`perf_loopback` 13 passed) and before every invocation. Session start
15:31Z, cap 20:31Z. Scored run: envelope 18:10–19:59Z (crown spot 1959 s,
seed 42 2168 s, seed 7 2426 s), mean 57 s per invocation.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | **attempt 1 only** (15:55Z launch): at seed 42 start, after a co-tenant's `exec 8>PATH` truncated the lock files (amendment 3); no battery row existed. Attempt 2: no. |
| `ABORT-CRLF` | no (`lib.sh` 0 CR bytes) |
| `ABORT-SHA` | no (checked at start and before each of 80 invocations) |
| `ABORT-SENTINEL-UNWRITABLE` | no (all paths probed at launch) |
| `ABORT-SMOKE` | no: `SMOKE-PASS` (see below) |
| `ABORT-RC` | 0 of 80 |
| `ABORT-BRINGUP` | 0 (0 `RUN-RETRY`, 0 `NO_DATA`) |
| `VOID-COTENANT` (amendment 3) | 0 of 80; no `LOCK-TRUNCATED-BY-FOREIGN` |

Reading of the attempt-1 `ABORT-LOCK`: it refused before any battery row
existed and the relaunch (amendment 3) was committed before any number was
read, so attempt 2 is the scored battery. Under the stricter reading (any
firing makes the whole re-test `UNSCOREABLE`) the verdict would be
`UNSCOREABLE`; neither reading licenses the flip.

*Smoke* (15:48–15:54Z, before any co-tenant; one invocation per arm at c2
and c8, bulk, seed 42): every row `LIVE`; `pipeline=` header and `[PIPE]`
matched the arm on both endpoints (`pipeline=block backend=RaptorQ` /
`pipeline=window backend=Rlc`); `[GATES]` on both endpoints; the RLC
auto-select line absent on BLK and present on both WIN endpoints. Measured
cost `c` = 84.75 s/invocation, which set n = 2 per seed (amendment 3).

*What ran.* n = 2 per seed × 2 seeds × 5 cells × 2 hints × 2 arms = 80
invocations, all `LIVE` (witnesses held on every row; 0 contaminated).
Ledgers: `docs/l1-raw/blockretest/` (`br-s42.log` sha256 b93679f3…,
`br-s7.log` af011eb3…, `crown-s42.log` f3cd299f…, `crown-s7.log`
819165ad…, plus the smoke and attempt-1 envelope logs); per-invocation
endpoint logs (26 MB) stay on the VM under `/home/vibe/blockretest/run/diag`.

*Per cell, pooled over seeds* (n = 4 per arm-cell-hint; goodput Mbit/s median
[min–max] over completed reps; completion p50 s; DNF = past 150 s):

| cell | hint | BLK goodput | BLK p50 | BLK DNF | WIN goodput | WIN p50 | WIN DNF | failed clauses |
|---|---|---|---|---|---|---|---|---|
| c1 | bulk | 250.1 [245.5–261.0] | 12.80 | 0/4 | 192.1 [187.1–211.1] | 16.67 | 0/4 | goodput, completion |
| c1 | auto | 162.0 [159.7–172.4] | 19.75 | 0/4 | 168.2 [164.8–171.0] | 19.03 | 0/4 | — |
| c2 | bulk | 86.3 [83.5–87.8] | 9.27 | 0/4 | — | — | **4/4** | goodput, completion, DNF |
| c2 | auto | 58.4 [58.2–58.7] | 13.69 | 2/4 | 18.4 [13.9–23.5] | 43.64 | 0/4 | goodput, completion (pooled, s42) |
| c3 | bulk | 16.4 [15.5–16.8] | 12.23 | 0/4 | 3.3 [2.8–3.4] | 61.26 | 0/4 | goodput, completion |
| c3 | auto | — | — | **4/4** | 3.8 [3.3–4.8] | 53.16 | 0/4 | — (BLK completed none) |
| c7 | bulk | 119.6 [119.2–120.8] | 13.38 | 1/4 | 79.0 [76.4–86.4] | 20.26 | 0/4 | goodput, completion |
| c7 | auto | — | — | **4/4** | 83.2 [81.5–85.1] | 19.24 | 0/4 | — (BLK completed none) |
| c8 | bulk | 82.4 [74.1–83.4] | 2.43 | 1/4 | 19.4 [5.6–59.2] | 12.46 | 0/4 | goodput, completion |
| c8 | auto | — | — | **4/4** | 5.6 [4.2–63.4] | 37.35 | 0/4 | — (BLK completed none) |

Every bulk failure above also fails on each seed separately (n = 2 per seed;
per-rep values in the ledgers and the scorer's `REPS` lines). Shaped
capacities for MEASUREMENT DISCIPLINE 16: c1 1000, c2 100, c3 20, c7 200,
c8 120 Mbit; the block arm's bulk goodput is 25 %, 86 %, 82 %, 60 % and 69 %
of them, so the not-worse clauses were not ceiling-bound anywhere.

*Verdict.* `BLOCK-BETTER-AT-c1,c2,c3,c7,c8`: at the **bulk** hint the window
pipeline is worse than block at every cell, on both seeds, by goodput and
completion time (and by DNF at c2); at c2 it is also worse at auto on seed
42. The stated-in-advance reading fired: `WIN` DNF'd 100 MB at c2 in 4 of 4
reps, so finding 3.1.2 moves from "substrate" to "the window pipeline" at c2.

*Crown spot* (same session, 18:10–18:43Z, `ship` arm, realtime, 64 reps):
`REPAIRS-INERT-ON-CROWN`. p99 median / p50 median per cell-size-seed, all
inside their bands: c2·400B 37.6/8.03 (s42), 36.8/7.90 (s7); c2·1200B
40.2/8.32, 41.3/8.32; c3·400B 114.7/24.10, 113.4/24.03; c3·1200B 93.8/25.68,
100.3/25.85 ms. `count = 1000` in 64 of 64 reps. Single-rep outliers
(inside the median rule): c2·400B s7 one rep p99 134 ms; c3·1200B s7 one rep
p99 1004.7 ms. The attempt-1 crown spot (15:55–16:27Z) ran under a
co-tenant cargo build and is VOID (amendment 3), not scored.

*Outside the pre-registered set (findings, no verdict).*
1. **The shipped default path fails at Auto.** `BLK` at the auto hint — the
   shipped default config (hint Auto, block pipeline, RaptorQ) — did not
   finish in 150 s in 14 of 20 invocations: c3, c7, c8 4/4 each, c2 2/4
   (both seed-7 reps); only c1 completed. `WIN` completed all 20 auto
   invocations. One inspected DNF (c3 auto s42 rep 1) shows the server
   repeatedly logging `evicted timed-out decoders (block decode failures)`
   (count 15–54 per sweep). The not-worse rule is one-sided, so this does not
   enter the verdict, but it means neither pipeline is acceptable at Auto
   today: block stalls, window runs far below capacity (c3 3.8 Mbit, c8 5.6).
2. `WIN` at bulk is 3–5× slower than `BLK` at c3 (3.3 vs 16.4 Mbit) and c8,
   and highly variable at c8 (5.6–59.2 Mbit).

*What it means.* The window pipeline is not a safe default for Bulk: at the
bulk hint the legacy block pipeline wins everywhere this battery looked, so
ADR-0069's flip and the block deletion are not licensed, and the
one-machine claim must name the exception (Bulk/Auto default stays on the
block pipeline). The same run also shows the block default stalling at the
Auto hint on four of five cells, which is a defect on the path users get
with no flags.
