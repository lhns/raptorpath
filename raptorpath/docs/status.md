# Status

What ships today, the most recent verdicts, the open debts, and the next
pre-registered measurement. Rules for measuring are in
[measurement-discipline.md](measurement-discipline.md). The full measurement
ledger this replaces is in git history (ledger at ac1aed1).

## 1. The default stack (as the code resolves it)

**Pipeline routing**: one pipeline, the sliding window, for every hint
(ADR-0069, executed in `dacfd7c`). `net::pipeline_backend` accepts only the
streaming RLC codec; a block-only backend, `interleave_depth` or
`mp_block_affinity` is a startup error. The retention contract ρ is the
named point's preset, an independent dial that never selects a pipeline:

| hint | default | with `--window-reliable` |
|---|---|---|
| Realtime | window, unified RLC span machine, EVICT retention (ρ < 1) | retain-until-acked (ρ = 1) |
| Auto (the default hint) | window, RLC, retain-until-acked (ρ = 1) | unchanged |
| Bulk | as Auto | unchanged |

Pinned by `default_config_routes_every_hint_to_the_window_pipeline` and
`block_only_config_is_an_error_naming_adr_0069`.

**Wire and substrate.** `PROTOCOL_VERSION = 9` (per-path `path_seq`,
`next_expected`, `received_above`); compact DATA framing
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
`RWM_ACK_MERGE`, `RWM_WIRE_COMPACT`, `RWM_EST_CADENCE` (the loss
estimator's BOCD update batched: clean evidence accumulates and flushes every
10 ms, a loss-bearing ack flushes at once; flipped in `83462ae` on Stage 3
(d), §5; own echo, ACTIVE or OFF, not on `[GATES]`). `RWM_POOL_ANCHOR` is
independent of the cadence and ships off (it used to follow it when unset).
`RWM_HONEST_CAP` resolves on but is inert
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
| Stage-3 baseline (§5) | A/A + block vs window (bulk, auto) + `RWM_EST_CADENCE` arm, 6 cells, n = 5 × 2 seeds, 320 invocations; crown spot | MDE committed (goodput 1.4–5.6 %); `WINDOW-NOT-WORSE` (window ahead at every auto cell); crown `REPAIRS-INERT-ON-CROWN`; cadence `FLIP-RECOMMENDED` (dual c1 +41 %, sender CPU −12 to −32 %); nothing flipped by the battery; the cadence flipped in `83462ae` with the pool anchor decoupled |
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
2. **Fixed** (1b890e0, 5af2687). sc2 (clean 100 Mbit single path) did not finish 100 MB in 300 s on the shipped window machine. Cause: the CPU-bound sender — since d60f3ab the r(β) mix evaluated the window-mass anchor term per source symbol (~6 ms a call). The rate is now evaluated on a cadence of min(5 ms, SRTT/4) of the worst-ε path, lock-free, with one lock order.
3. **Exit flush fixed** (d6c1c41): the sender `[ETA]` prints once more at exit, `final=1`. Open: `eta_s4.py` uses RTprop where the law uses SRTT (routes disagree 3–4×); the σ̂ witness fails at c7/sc3.
4. c8 control shows a bimodal fast-path-alone collapse (4/8 reps), outside every pre-registered set; the likely source of c8's 75 % CV.
5. **Fixed** (50b17af). `RWM_COPA_DELTA` has an engine echo in `[GATES]`, from the one resolution the CC reads.
6. **Fixed** (5bad678). Every readout is one write (`crate::readout!`); `tracing` no longer glues onto readout lines.
7. **Fixed** (8f484be). `r_report.py` propagates a W7 `VOID`, and R-FUNDED-NEGATIVE needs a bounded shift.

Also fixed with the cleanup: `[DIAG] cod=` counted source copies (gap
retransmits, request copies, taper copies) as coded repair. It now counts coded
symbols actually sent; copies go to `total_copy_symbols` and `retx=`. The r > 0
battery's "coded symbols are 86–90 % retransmits" was read on the old meaning.

### 3.2 Generation-coding stack: no disposition

`--window-generation-coding` (with DAPS, the rate-sample estimator, `RWM_GEN*`,
`RWM_OOO_RETAIN`) is opt-in, never default, and has no keep-or-remove decision.

### 3.3 NO-MODE-SWITCH debts (hint- or ρ-keyed code paths)

Each is a behaviour step at a preset point, which CLAUDE.md forbids. (The
block/window pipeline fork, `is_window_mode`, and the block hint tables —
block size, flush timeout, interleave depth — were removed with the block
pipeline in `dacfd7c`.)

| site | what it switches |
|---|---|
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

1. **Fixed** (1b890e0). Throughput regression on the current binary line: the
   CPU-bound sender of item 3.1.2.
2. **Fixed** (wire v9, 7825196). seq 0 delivered but never pruned: the
   cumulative point is `next_expected`, a count, so a delivered seq 0 alone is
   acked and pruned.
3. **Fixed** (e1ce7e7). The store-cap path-set cliff and the same-class
   `active_paths()` sites: every pool and worst-path reader (store-cap Σ, the
   react-cap SRTT, the NACK-budget and `repair_rate` worst-loss picks, the
   taper's ε at send, WindowStart/Shutdown) reads `live_paths()`. Placement
   picks, `spare_capacity()` and the `np_act`/`[SF]` gauges keep the
   saturation-filtered set.
4. **`[DIAG] rtp` prints whole milliseconds.** At sub-millisecond RTprop
   (loopback) it prints `rtp0ms`, which cannot be told from an unset anchor,
   and at c1's 2 ms the rounding is coarse.
5. The recovery-clock bind fractions in paper §7.1 / §9.7 were measured on the
   saturation-filtered set and are not re-measured.

### 3.6 Fixed by the fix program (not listed above)

| defect | fix |
|---|---|
| SACK truncation: a capped report claimed unreported holes as received | 738008c (honest prefix, cap derived from the datagram); wire v9 `received_above` lets the store gate converge past the cap (7825196) |
| Auto on the block pipeline DNF'd (§4 finding 1) | be13c8e (retention bounded by bytes only), f21902c (unconfirmed blocks never evicted; retention is the flow-control window, the sender back-pressures), f30d0c1 (completed-block ring sized by the done horizon) |
| ρ = 1 receiver dropped decoded packets on a full consumer channel | fbde330 (holds and caps the advertised point below the held seq) |
| per-path loss read other paths' batches as loss (ε̂ ≈ 0.5 at 50/50) | wire v9 per-path `path_seq` (7825196) |
| v9 rate-cache churn: the cadence re-evaluated on every W and worst-ε-path change | 7636a4c (keyed on age only) |
| the loss estimator was fed more loss than the wire drops (`plc=` 0.024 vs 0.005 at c2). **Premise void (§3.8):** 0.005 was netem's skb counter; the per-datagram truth at c2 is 0.026 | `fix/loss-feed`: the tracker credits reorder instead of charging it; the sender carries the late-arrival credit instead of clamping it; the `PathReport` loss feed is deleted; the receiver's incoming loss feeds the RX slot only (so `nack_effectiveness()`, which reads it, is no longer a constant 1.0 on an endpoint that also receives); `[DIAG] dgev` / `[CTLD] dgrx` count local datagram drops. Against per-datagram truth, fed loss already matched before the fix and still does: `plc`/truth 0.95–1.03 on every run at c2, c3 and both c8 legs (n = 3 per binary); the c1 dual reads 0.89–1.41, a few tens of datagrams per run that track the receiver's kernel `RcvbufErrors`, which no engine token counts |

### 3.7 Recorded, not fixed

| finding | evidence (VM) |
|---|---|
| BOCD `predictive_loss_upper` (`plu=`) reads ≈ 0.0354 on clean links: a floor from the prior and the run-length mix, unverified | `/home/vibe/s9/out-main/c1dual400-fix-r*-c.log` |
| balanced v9 striping costs ≈ 1.34× sender kernel CPU at the dual c1 cell | `/home/vibe/s9/out-perf/perf-c1dual-{base,fix}-r*.flat.txt` |
| Auto on block at c3 is congestion-window-bound at 7.2 Mbit/s (bulk block 17), not retention-bound. **Moot**: the block pipeline is removed (`dacfd7c`) | `/home/vibe/v1b/out/dbg-c3autoblk-r*-c.log` |
| the per-batch `Ack` arm (`RWM_ACK_MERGE=0`; formerly also the block pipeline) releases in-flight from the raw wire counts, `received + (expected − received)⁺`: under reorder it releases more than was sent (6,8,7,9 → 5 for 4; 6 before the tracker fix). The merged `WindowAck` arm releases through the credited pair and closes exactly | `s10_per_batch_ack_carries_the_late_arrival_credit` (loss feed only; the release is not asserted) |
| c8 Auto-on-block goodput is bimodal: 32.7 and 35.2 Mbit/s plain, 57.6 in the debug run. **Moot**: the block pipeline is removed (`dacfd7c`) | `/home/vibe/v1b/out/c8autoblk-r*-drv.out` |

### 3.8 Finding: every netem-counter loss truth was low by the GSO factor

netem's `dropped` counts skbs; quinn sends UDP GSO super-packets, and netem
decides loss per skb. So `dropped / Sent` read datagram loss low by the
datagrams-per-skb factor, measured on main 2e264b7's binary at 4.9 (c2), 3.4
(c3), 2.7 (c1), 3.9 / 1.5 (c8 fast / slow leg) (6.1 at c2 on 949e06b: a
sender property, not a cell constant). Calibration on a bare netem with no
delay or rate: 5 300 datagrams lost at 5 per skb while `dropped` read 1 060; at
1 per skb it matched (5 183 / 5 182). The realised per-datagram loss RATE is
still p/(p+q) (2.60 % at c2, 4.86 % at c3); what is longer than documented is
the burst, by the same factor, and differently on the two legs of one dual.
And netem never reorders on these cells: every cell sets `rate`, under which
netem is FIFO (0 out-of-order in 10⁵ on the c2 and c3 shapes; 82 920 with the
same jitter and no `rate`). Reorder handling was never exercised on the wire.

The harness now measures truth per datagram (`[TRUTH]` per leg, a clsact +
egress matchall counter ahead of netem; measurement discipline rule 19;
`tools/l1/lib.sh`). Validation on main 2e264b7's binary (sha256 f3743664…),
25 MB, seed 42, one run each: c2 single `loss=0.020575` (483 of 23 475
datagrams, g 4.82) against `plc=0.0204` (0.99×), where netem's counter read
113 / 22 992 = 0.0049; c8 dual fast leg `loss=0.021377` (g 5.53) against
`plc=0.0211` (0.99×), slow leg `loss=0.044158` (g 1.54) against `plc=0.0450`
(1.02×); `rcvbuf_drops=0` on both runs. The factor also moves with transfer
size (c8 fast leg 3.9 at 100 MB, 5.5 at 25 MB).

What this voids or restates (each traced to its source):

| claim | source | status |
|---|---|---|
| the loss feed over-counts ~4–5× (`plc` 0.024 vs "wire" 0.005 at c2; §3.6) | `perf_rwm_c.sh` QDISC `dropped / Sent` | void: the feed matched the per-datagram wire before and after `fix/loss-feed` |
| "realized packet loss 0.55 % (c7) / 1.96 % (c8 slow leg), 0.81 % (c2r100)"; cross-path contamination "37–93×" | goal-gate READOUT 4, `tc:` counters (b21174a's message); `xpath_loss_replay.py` | low by the GSO factor; the contamination ratios shrink by the same factor (the defect itself stands: `fed_old` read 0.50 at c7) |
| c8 realized p = 0.0055 / 0.0196 | `docs/research/cost-ratio-memo.md` (from `xpath_loss_replay.py`) | restated in the memo: 0.027 / 0.041 |
| model inputs "c7/c8 legs 0.55 % / 1.96 %" | comments in `src/net/tests.rs` (`xpath_loss_model` tests) | synthetic model inputs; the tests hold at any ε, but the cell labels quote the skb-counter values (engine file, not edited here) |
| "same-path reordering" 0.21 / 0.32 of delivered latency at c7/c8 | paper §9.8 placement decomposition (`[LAT] rw_sp`) | relabelled: `rw_sp` is in-path reorder OR a loss on that path; these cells do not reorder, so it is loss wait |
| netem jitter reorders (the premise of `fix/loss-feed`'s reorder credit, F1) | `fix/loss-feed` (F1) | never held on L1; F1 is exercised by unit tests only |
| per-path ε̂ under-reads the channel 3–5× (0.0056 / 0.0184 vs 0.025 / 0.048; paper §2.6) | estimator gauge vs p/(p+q) | not voided: p/(p+q) is the realised datagram rate; the ε̂ gauges are not re-read on the current feed |
| paper §2.5 GE adequacy against real traces | offline cellular traces through a drop-tail queue | not affected: no netem |
| D0 attribution audit (π0 heal share) | engine-side hole accounting (`holeaudit_*`) | not affected: never reads tc |
| sender-truth estimator refuted (27e36e3) | fed ε̂ against the wire | not reversed: it feeds ≈ 0.94 in window mode (V3), wrong at any truth |

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

## 6. V4 verification — pre-registration

A verification battery of the merged `main` (window-only pipeline with the
block pipeline deleted, ADR-0069 executed; the sender allocation and clock
cleanup; batched estimator updates `RWM_EST_CADENCE` ON by default with the
pool anchor untied and off) against the Stage-3 binary, plus one opt-in arm,
the tunnel's inner-TCP cell and the crown spot. Committed before VM contact;
no number below is a result. Nothing is flipped by this battery.

**Binaries.** NEW = this branch's HEAD (`meas/verify4`, engine tree = `main`
5fb230b; the commits after it touch `tools/l1`, one test harness file and
docs), archived with `git -c core.autocrlf=false -c core.eol=lf archive`,
built fresh on the benchmark VM (`cargo build --release --bin raptorpath`
after the test suite, below), copied under its real name to its own
directory, `sha256` recorded in `BINSHA.txt` and re-verified before every
invocation. OLD = the Stage-3 binary, `sha256
f3743664cb48ad81deef239d44710f4b12bf08f2db65c6d90fc5a947ed500035` (`main`
2e264b7's tree; §5): the copy kept on the VM from V3 is reused if its
`sha256` is exactly that, else it is rebuilt from an archive of 2e264b7; any
other `sha256` is `ABORT-OLD-BINARY` (nothing runs). Both binaries run under
the same harness (this commit's `tools/l1`), as in V3.

**Tests first** (on the NEW tree, on the VM, inside the session's locks):
`cargo build --release`; `cargo test -p raptorpath -p raptorpath-math
--release --no-fail-fast -- --test-threads=2`; `cargo test --doc -p
raptorpath --release`; `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE`
unset). Per-command rc and passed/failed/ignored go to `TESTS.txt`. A real
failure (not a flake that passes on an immediate re-run of that test alone)
is `ABORT-TESTS`: the operator writes NOGO and nothing is measured. The
harness fix (c) is verified here: `gate_suite` runs twice on the fixed
`gate_harness.rs` and twice on the pre-fix one (`--nocapture
--test-threads=1`, timing lines stripped); the fix is `DELIVERED` iff both
fixed runs pass and print identical output. The pre-fix pair is a contrast,
reported either way.

**Harness.** Envelope `tools/l1/verify4_run_all.sh` (both locks for the whole
session via `lib_battery.sh`; build → tests → binaries → smoke → operator GO
→ budget → battery → tunnel → crown → score; hard backstop), driver
`verify4_battery.sh`, scorer `verify4_parse.py` (rows by `stage3_parse.py
make_row`; offline test `test_verify4_parse.py`), tunnel driver
`tun_bulk.sh`, crown `crownspot8.sh` scored by `stage3_parse.py crown`.
Every perf invocation is `perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1
RWM_PERF_TIMEOUT_S=150 SEED=<seed>`, one run, `--window-reliable`, a fresh
topology; every arm first `env -u`'s `RWM_EST_CADENCE RWM_POOL_ANCHOR
RWM_EMIT_BATCH RWM_EMIT_BURST` (rule 15d) and only EMB sets one back.

**Cells**: §5's six, unchanged (geometry, size, capacity and the > 5 %
headroom of §5's table; `c2-100` 11 %, `c3-25` 13 % are the tightest, so CPU
is the free axis there as in §5): `c1s-400`, `c1d-400`, `c2-100`, `c3-25`,
`c7-100`, `c8-100`.

**Arms**:

| arm | binary | hint | extra env | expected cadence echo (both ends) | part |
|---|---|---|---|---|---|
| `NEW` | NEW | bulk | — | `cadence ACTIVE` | A, C (control) |
| `OLD` | OLD | bulk | — | none (that binary prints the echo only when on) | A |
| `NEWa` | NEW | auto | — | `cadence ACTIVE` | B |
| `OLDa` | OLD | auto | — | none | B |
| `EMB` | NEW | bulk | `RWM_EMIT_BATCH=1` | `cadence ACTIVE` | C |

**Plan per (rep, seed) block: 19 invocations**, cells in the order below, the
arm order within every cell rotated by the block index (rule 3): `c1s-400`
NEW OLD EMB · `c1d-400` NEW OLD · `c2-100` and `c3-25` NEW OLD NEWa OLDa EMB
· `c7-100` NEW OLD · `c8-100` NEW OLD. Seeds 42 and 7; blocks run rep 1 s42,
rep 1 s7, rep 2 s42, … **Planned n = 8 per seed** (16 per arm and cell), cut
by the budget rule. EMB runs only at the single-path cells: it batches only
while exactly one path is live (`emit_batch_live = live_paths == 1`), so the
dual cells are out of its scope (and that scope is itself a path-count step,
flagged by the R6 research; see (C)).

**Witnesses per invocation** (a row failing one is `CONTAMINATED` or
`WITNESS-FAIL`, excluded and counted; a row without a client summary is
`NO_DATA`): the driver header and the `[PIPE]` echo on both endpoints read
`window/Rlc/<hint>`; `[GATES]` on both endpoints; the RLC auto-select line on
both; no generation guard line; the cadence echo as the table says,
two-sided (NEW arms: ACTIVE on both and OFF on neither; OLD arms: neither
line on either end — this is also a binary-identity witness, since NEW always
prints one of the two); `[GATES] RWM_POOL_ANCHOR=0` on both; EMB: `[GATES]
RWM_EMIT_BATCH=1` on both and the `emission batching ACTIVE` echo on the
client, every other arm `RWM_EMIT_BATCH=0` on both and the echo on neither;
the row's `sha256` is its arm's binary.

**Scored quantities** per invocation: §5's (goodput, completion, DNF past
150 s, `CPUCLI`, `[TRUTH]` per leg, `plc`, `plu`, coded share), plus sender
CPU per datagram = `CPUCLI` / Σ legs `egress_dgrams` (from `[TRUTH]`).
**The MDE** is §5's committed table, applied as the relative MDE of the cell
and metric to the reference median: goodput / completion / CPUCLI rel =
`c1s-400` 4.9 / 5.1 / 2.4 %; `c1d-400` 5.6 / 5.3 / 6.5 %; `c2-100` 1.4 / 1.4
/ 6.3 %; `c3-25` 1.6 / 1.6 / 9.6 %; `c7-100` 2.8 / 2.9 / 3.6 %; `c8-100` 4.0
/ 4.1 / 11.6 %; DNF threshold 0.20 at every cell. No new A/A is run; the
in-session OLD arm is checked against its §5 identity instead (below). CPU per
datagram has no committed MDE: the scored CPU clause uses `CPUCLI` with
`rel_cpu` (the byte count is fixed per cell, so the datagram count moves only
by repair and ack share), and CPU per datagram is reported beside it as a
measurement.

**Per-cell clause set** (X against REF, REF's median ± rel): X is **WORSE**
iff goodput med(X) < med(REF)·(1 − rel_gp), or completion p50(X) >
p50(REF)·(1 + rel_ct), or CPUCLI med(X) > med(REF)·(1 + rel_cpu), or
dnf(X) − dnf(REF) > 0.20; **BETTER** iff not worse and (goodput above
med(REF)·(1 + rel_gp) or CPUCLI below med(REF)·(1 − rel_cpu)); **SAME**
otherwise; **UNSCOREABLE** at a cell where either arm has fewer than 3 live
rows or ≥ 2 witness-failed rows, or after any of the first five abort causes.

**(A) NEW vs OLD at bulk, every cell** (X = NEW, REF = OLD). Outcome per
cell: `BETTER` / `SAME` / `WORSE` / `UNSCOREABLE`. **Prediction**: BETTER at
`c1s-400` and `c1d-400` (cadence: §5 (d) +41 % at c1d and P1's +43 % at c1s,
n = 1; plus the allocation/clock cleanup), SAME at `c2-100`, `c3-25`,
`c7-100`, `c8-100` on goodput (ceiling-bound), where a CPU BETTER is
possible (§5 (d) measured −12 to −19 % CPUCLI there, beyond the CPU MDE at
`c2-100`, `c3-25`, `c7-100`, `c8-100`). The prediction is printed as `MET` or
`MISSED-AT-<cells>`; it is a check, not an outcome, and a CPU BETTER where
SAME was predicted is a miss to be read, not a defect. **Control identity**:
OLD's goodput median at each cell is set against §5's CTL [min, max]
widened by the cell's absolute goodput MDE (`c1s-400` [267.3, 326.6],
`c1d-400` [176.9, 220.1], `c2-100` [85.7, 90.5], `c3-25` [16.62, 17.78],
`c7-100` [164.1, 184.1], `c8-100` [93.4, 109.8] Mbit/s): outside is recorded
as `CONTROL-MOVED` (session drift) at that cell. It does not block (A)'s
in-session comparison (both arms are interleaved in this session), but it is
named beside that cell's verdict and no historical number at that cell is
read.

**(B) Auto at `c2-100` and `c3-25`** (window is now the default for Auto).
Scored: NEWa vs OLDa in-session with the clause set above (the §5 bulk MDE
transferred to auto, recorded as weaker, as §5 (b) did): `BETTER` / `SAME` /
`WORSE` / `UNSCOREABLE` per cell. OLDa is the §5 WINa configuration (same
binary, same flags) re-measured under rule 3. The historical reading NEWa vs
§5 WINa (c2 73.1, c3 15.22 Mbit/s) is scored only if OLDa reproduces §5's
WINa: OLDa's median inside §5 WINa [min, max] widened by rel_gp·median
(`c2-100` [70.88, 75.12], `c3-25` [14.36, 15.74]); then `HIST-BETTER` /
`HIST-SAME` / `HIST-WORSE` by NEWa's median against 73.1·(1 ± 0.014) and
15.22·(1 ± 0.016); else `CONTROL-MOVED` (the in-session pair stands). No
direction is predicted beyond SAME-or-BETTER (the cadence moves the rate
law's input at Auto too).

**(C) EMB (`RWM_EMIT_BATCH=1`) vs NEW** at `c1s-400`, `c2-100`, `c3-25`
(X = EMB, REF = NEW). Per cell the clause set above, plus the **fed loss vs
truth** clause per leg: med(`plc`/truth) under EMB within [1/1.3, 1.3] ×
med(`plc`/truth) under NEW (the R6 research warns bursting may inflate the
per-path loss estimate; truth is `[TRUTH] loss=`, rule 19). Outcomes, in
precedence order: `UNSCOREABLE` (an abort cause) → `WORSE-AT-<cells>` (worse
at a cell without a hard blocker) → `FEED-MOVED-AT-<cell:leg>` (counts as
worse) → `UNSCOREABLE` (naming cells: a hard blocker or an unread feed ratio)
→ `FLIP-RECOMMENDED` (better at ≥ 1 cell, not worse anywhere, feed unchanged
everywhere, every cell scoreable) → `INERT-AS-DERIVED` (the routing witness
fires, nothing moves beyond MDE). A `FLIP-RECOMMENDED` here is qualified in
advance: the batching scope is a path-count step (`live_paths == 1`), the
same pattern the NO-MODE-SWITCH invariant forbids on δ/ρ, so a flip must
first express the scope continuously (or show it is not a step); the flip
is never made by this battery. **Execution witness (rule 1), stated limit**:
the gate echo and `[GATES]` prove the knob reached the binary; the engine has
no burst-size gauge, so the realised burst depth is not seen. The `[TRUTH]`
GSO factor (datagrams per skb) EMB vs NEW is reported as the indirect
witness (P1: GSO rises with batching), not scored. Expected (P1, n = 1):
c1s +12 % over cadence-on; c2/c3 goodput ceiling-bound, CPU the free axis.

**(D) Tunnel cell: inner kernel TCP** (`tun_bulk.sh`). Since ADR-0069 the TUN
MTU at Bulk/Auto is `symbol_size − 4 = 1196` (window clamp) where the
deleted block pipeline left 1500. Per bring-up: topo.sh `up <cell>`,
`raptorpath run` on both ends with `--protocol-hint <hint>` and **no**
`--window-reliable` (each binary picks its default: NEW the window, OLD the
block pipeline), a ping gate, then 4 cold TCP transfers (`transfer_bench.py
client --runs 1`, cubic, a fresh connection per transfer, completion includes
a 1-byte app ack) of 50 MB at c2 and 12 MB at c3. Plan: 2 rounds × seeds 42,
7 × cells c2, c3 × {NEW, OLD} × {bulk, auto}, arm order rotated; n = 16
transfers from 4 bring-ups per (cell, hint, binary). Witnesses per bring-up:
TUN MTU read from the kernel in each netns (NEW 1196 both, OLD 1500 both),
`[PIPE]` on both ends (NEW `window`, OLD `block`, hint matching), cadence echo
(NEW ACTIVE, OLD none), binary `sha256`; a failed bring-up is `NO_DATA`, a
witness failure excludes the bring-up. No MDE exists for inner TCP:
**outcome `MEASUREMENT-RECORDED`** with n, medians, ranges and per-seed
medians per (cell, hint, binary), and the NEW/OLD median ratio with whether
the ranges overlap (descriptive only); `UNSCOREABLE` at a (cell, hint) with
fewer than 8 live transfers on either binary; `NOT-RUN` if cut by the
budget. The NEW/OLD contrast confounds MTU with pipeline (no flag separates
them on either binary); it is reported as that.

**(E) Crown no-regression spot**: §5 (c) unchanged — `crownspot8.sh`
(`tail_matrix.sh` arm `ship`, env unset, which now means cadence ON), hint
realtime, c2 and c3, 400 B and 1200 B, ×8 reps (×6 if cut), seeds 42 and 7,
on the NEW binary, scored by `stage3_parse.py crown` against §5's bands:
`REPAIRS-INERT-ON-CROWN` / `CROWN-MOVED(...)` / `SPOT-UNSCOREABLE` /
`SPOT-NOT-RUN`.

**Known effects, declared so they are not misread.**
1. The cadence moves `plu` off the 0.0354 BOCD floor at every cell where the
   per-ack estimator sat on it (§5 (d): CTL 20/20 on the floor at c1d and
   c2 → 0); NEW vs OLD therefore moves the rate law's input, not only CPU.
2. Since the RX-slot fix (F4) the receiver packs more acks per QUIC packet
   at c2/c3/c8; both binaries carry it (it predates 2e264b7).
3. Honest SACK (wire v9) and live-path membership are in both binaries.
4. The window pipeline is the only pipeline on NEW; under the perf harness
   both binaries run the window (`--window-reliable`), so (A)–(C) compare
   like with like; only (D) runs each binary's default pipeline.
5. The Stage-3 binary prints no `cadence OFF` line; its control witness is
   the absence of both lines (and its `sha256`).
6. §5's A/A at `c1s-400` sat at 282–312 against V2's 285–294 (cross-era);
   drift across sessions is why OLD is re-measured here, not read from §5.
7. The allocation/clock cleanup changed no law; any CPU move at c2/c3/c7/c8
   beyond the cadence's is attributed to it only descriptively.

**Budget** (5 h cap from the first ssh; hard backstop = first ssh + 4 h 50
min; soft = hard − 10 min). Priors: build + tests ≈ 30–45 min; smoke ≈ 3
min; per block `R_PRIOR` = 6 min (19 invocations at §5's 13 s mean plus
margin); tunnel reserve 25 min; crown reserve 35 min (§5: 1958 s). The smoke
(`c2-100` NEW, `c8-100` OLD, `c3-25` NEWa, `c2-100` OLDa, `c1s-400` EMB; one
tunnel bring-up per binary at c2 bulk, 5 MB × 1; one `ship` 400 B rep) sets
`R_est` = `R_PRIOR`·max(1, c_meas/91 s). n per seed = min(8, ⌊(soft − now −
crown reserve − tunnel reserve)/(2·R_est)⌋). **Cut order**, each applied
only while n < 4: (1) crown reps 8 → 6; (2) drop the crown (E); (3) drop the
tunnel (D); (4) drop EMB (C; block 16/19); (5) drop auto (B; block 12/19).
n < 3 after all cuts is `ABORT-BUDGET`. The battery starts no (rep, seed)
block that would cross soft − reserves (`TRUNCATED-AT-REP-BOUNDARY`, scored
at the n reached).

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD`, `ABORT-TESTS` (a
real test failure; nothing is measured), `ABORT-OLD-BINARY`, `ABORT-SHA`
(either binary changed; checked at start and before every invocation),
`ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (every smoke row `LIVE` with its
CPU line, `[TRUTH]` per leg, `[DIAG]` `plc`/`plu`/`busy`/`cum`, CPU per
datagram; both tunnel bring-ups LIVE with ≥ 1 transfer; one `ship` rep),
`ABORT-BUDGET`, `ABORT-RC` (row `VOID-RC`, the battery goes on),
`ABORT-BRINGUP` (`NO_DATA` after 2 attempts). Void class `VOID-COTENANT`.

**Session rules.** §5's: both locks for the whole session; detached envelope;
earned sentinels (`DONE-ALL` only with a complete ledger, `verify4_parse.py
check` rc 0, no truncation, the tunnel and crown completed or cut by the
plan); the operator reads only sentinels and, before GO, `TESTS.txt` and the
smoke check, waiting in bounded loops on the VM side (≥ 5 min apart); `pkill
-x raptorpath` only; no `ens18`, firewall, `sshd` or non-`rp-*` namespace is
touched. Compact ledgers are copied to `docs/l1-raw/verify4/` (gzip above 2
MB).
