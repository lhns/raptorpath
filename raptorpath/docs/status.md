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
(d), §5; own echo, ACTIVE or OFF, not on `[GATES]`), `RWM_EMIT_BATCH`
(pacer-quantum emission batching, burst bound `RWM_EMIT_BURST` = 64 at every
path count, Law 0; flipped in `bf3a636` on the §8 re-run; on `[GATES]` as
`RWM_EMIT_BATCH=1` plus its own echo: `emission batching ACTIVE`, or `OFF`
at `=0`, or `out of scope` where the policy excludes it: ρ < 1, Realtime
packing, a coded wire). `RWM_POOL_ANCHOR` is
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
| Threading redesign P0 (§9) | P0 (named runtime + `[THR]`/`[LAG]`) vs MAIN 8d7d8c1, `c1s-400`/`c1d-400`, n = 3, `RWM_RDIAG=1`, 12 invocations | D3 `D3-REFUTED-WITH-RECORD` at c1s (server receiver task 79 % busy, hottest server thread 0.36 core; stop rule not fired; at c1d the receiver task reads 92 %); no-behaviour-change `REFUTED-WITH-RECORD` (c1s goodput −6.7 %, disjoint ranges at n = 3; c1d within); per-thread budget recorded; nothing flipped |
| Emission-batching scope, re-run (§8) | `NEW` vs `EB0` (Law 0), 4 cells, n = 8 × 2 seeds, 128 invocations, on `927bb00` (§7 receive-buffer fix) | `FLIP-RECOMMENDED`: BETTER at every cell, WORSE nowhere; `c1d-400` +41 % goodput, sender CPU −12 to −35 %; fed loss unchanged; 0 rcvbuf drops; flipped in `bf3a636` |
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
| `net/sender_policy.rs` `use_packing` | symbol packing on Realtime only; since `bf3a636` also emission batching's exclusion (`emit_batch_on` requires `!use_packing`) |
| `scheduler/copa.rs` `queue_target_mult` | Copa queue target 1.08 / 1.125 / 1.25 by hint |
| `gates/scheduler_gates.rs` `copa_compete_active` (`RWM_COPA_COMPETE`) | Copa's TCP-competitive mode switching |
| `reliable` boolean branches in the window sender/receiver | ρ = 1 vs ρ < 1 selecting code paths instead of composing with δ; since `bf3a636` this includes emission batching (`emit_batch_on` requires `reliable`) |

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
| emission burst bound | `RWM_EMIT_BURST` = 64 | `net/emit_burst.rs` `emit_burst_bound` | "≈ 64 KB", not swept; shipped as measured (§8); binds 0.84 of bursts at c1 |
| taper-cache staleness | 50 ms | `net/emit_source.rs` `TAPER_CACHE_MAX_AGE_US` | no derivation and no gauge; shipped with emission batching inside the measured EB0 arm |
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
| the loss estimator was fed more loss than the wire drops (`plc=` 0.024 vs 0.005 at c2). **Premise void (§3.8):** 0.005 was netem's skb counter; the per-datagram truth at c2 is 0.026 | `fix/loss-feed`: the tracker credits reorder instead of charging it; the sender carries the late-arrival credit instead of clamping it; the `PathReport` loss feed is deleted; the receiver's incoming loss feeds the RX slot only (so `nack_effectiveness()`, which reads it, is no longer a constant 1.0 on an endpoint that also receives); `[DIAG] dgev` / `[CTLD] dgrx` count local datagram drops. Against per-datagram truth, fed loss already matched before the fix and still does: `plc`/truth 0.95–1.03 on every run at c2, c3 and both c8 legs (n = 3 per binary); the c1 dual reads 0.89–1.41, a few tens of `RcvbufErrors` per run — **skbs, each a GRO superpacket of ≈ 9 datagrams** (§7.1), so 0.05–0.46 % of datagrams; fixed by the 4 MB receive buffer (§7) |

### 3.7 Recorded, not fixed

| finding | evidence (VM) |
|---|---|
| BOCD `predictive_loss_upper` (`plu=`) reads ≈ 0.0354 on clean links: a floor from the prior and the run-length mix, unverified | `/home/vibe/s9/out-main/c1dual400-fix-r*-c.log` |
| balanced v9 striping costs ≈ 1.34× sender kernel CPU at the dual c1 cell | `/home/vibe/s9/out-perf/perf-c1dual-{base,fix}-r*.flat.txt` |
| Auto on block at c3 is congestion-window-bound at 7.2 Mbit/s (bulk block 17), not retention-bound. **Moot**: the block pipeline is removed (`dacfd7c`) | `/home/vibe/v1b/out/dbg-c3autoblk-r*-c.log` |
| the per-batch `Ack` arm (`RWM_ACK_MERGE=0`; formerly also the block pipeline) releases in-flight from the raw wire counts, `received + (expected − received)⁺`: under reorder it releases more than was sent (6,8,7,9 → 5 for 4; 6 before the tracker fix). The merged `WindowAck` arm releases through the credited pair and closes exactly | `s10_per_batch_ack_carries_the_late_arrival_credit` (loss feed only; the release is not asserted) |
| c8 Auto-on-block goodput is bimodal: 32.7 and 35.2 Mbit/s plain, 57.6 in the debug run. **Moot**: the block pipeline is removed (`dacfd7c`) | `/home/vibe/v1b/out/c8autoblk-r*-drv.out` |

### 3.8 Finding: every netem-counter loss truth was low by the GSO factor

> **Unit note (§7.1):** `rcvbuf_drops` / `RcvbufErrors` / `rxdrop` count **skbs** (GRO superpackets, ≈ 9.4 datagrams each at c1s), not datagrams: the per-socket `sk_drops` equalled the netns `RcvbufErrors` on every non-zero row. Every drop count in §6 and §8 is in skbs. Since the receive-buffer fix (§7) these drops are 0 at c1s/c1d/c2.

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

**Result** (scored 2026-10-04 against this pre-registration, literally; no
amendment was made): **(A) BETTER at all six cells; (B) BETTER at `c2-100`
and `c3-25`, with HIST-BETTER at both; (C) `FLIP-RECOMMENDED` (on sender CPU
at `c1s-400` and `c2-100`; qualified as pre-registered); (D)
`MEASUREMENT-RECORDED` at all four (cell, hint); (E)
`REPAIRS-INERT-ON-CROWN`; the harness fix (c) `DELIVERED`.** Nothing is
flipped by this battery.

*Binaries and session.* NEW = commit 7d8cec2 (this section's
pre-registration; engine tree = `main` 5fb230b), built fresh on the
benchmark VM (Xeon E5-2650 v3 era, 6 vCPU), `sha256
aa88d648def0b1253985b9d9ad6955d2190277bebc09026aaa2b79ce64ebd262`. OLD =
the V3 copy, reused because its `sha256` is exactly
`f3743664cb48ad81deef239d44710f4b12bf08f2db65c6d90fc5a947ed500035` (no
rebuild). First ssh 15:02:18Z (hard backstop 19:52:18Z); build 15:02–15:07Z;
tests 15:07–15:35Z; smoke 15:35–15:37Z; GO 15:38Z; battery 15:38–16:39Z
(3653 s, 304 invocations); tunnel 16:39–16:57Z (1088 s); crown 16:57–17:30Z;
locks released 17:30:31Z. **Session wall 2 h 28 min** of the 5 h cap. The
budget rule gave n = 8 per seed with no cut (`c_meas` 56 s < `C_PRED` 91 s,
so `R_est` = `R_PRIOR`). The three `sigma_diag_reachability` re-runs (below)
were run by the operator inside the session's locks between `SMOKE-PASS` and
GO; the envelope's cotenant check at battery start read `cargo=0 rustc=0`
(`all-era.txt`). One progress read at 16:08Z (`BLOCK-COMPLETE` count 7, 148
rows), per rule 13. VM left quiet: 0 `raptorpath`, 0 `cargo`, 0 `rp-*`
namespaces, both locks absent.

*Tests (NEW tree, VM, release).*

| command | rc | passed | failed | ignored |
|---|---|---|---|---|
| `cargo build --release` | 0 | — | — | — |
| `cargo test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2` (86 binaries) | 101 | 1035 | **1** | 50 |
| `cargo test --doc -p raptorpath --release` | 0 | 0 | 0 | 0 |
| `cargo test -p raptorpath-wasm` (`GOLDEN_CAPTURE` unset) | 0 | 35 | 0 | 0 |
| `gate_suite` ×2, fixed harness (`--nocapture --test-threads=1`) | 0, 0 | all | 0 | 1 |

The one failure is `sigma_diag_reachability::the_diag_line_reports_the_rtt_sigma_the_recovery_clock_needs`,
at its coverage precondition (clause 3b: "no `[DIAG]` tick saw the single
loopback path cwnd-full"): the two 8 MB loopback objects now finish in
0.3–0.8 s, so the run has 1–2 `[DIAG]` ticks and whether one of them lands on
a cwnd-full instant is timing. Re-run alone three times: fail, pass, fail —
a flake by the pre-registered rule, so not `ABORT-TESTS`; recorded below as
a finding (the test's precondition is not deterministic on the faster
sender). The doc-test command ran 0 doc tests.

*Harness fix (c): `DELIVERED`.* The fixed `gate_harness.rs` printed
byte-identical `gate_suite` output on both runs (330 lines, md5
2af16ecee0f0 both); the pre-fix harness, same binary, differed between its two
runs in two trial lines (trials 1 and 5 of the outage-recovery timings: "path0 back at
0.307 s / 0.308 s, goodput back 33 / 52 ms later"), i.e. the wall-clock
cadence heartbeat leaked into the sim, and the fix removes it. Every gate
passed in all four runs.

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (both taken 15:02:51Z, no `LOCK-TRUNCATED-BY-FOREIGN`) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script after sync) |
| `ABORT-BUILD` | no |
| `ABORT-TESTS` | no (one failure, a flake by the pre-registered re-run rule; above) |
| `ABORT-OLD-BINARY` | no (sha matched; reused) |
| `ABORT-SHA` | no (both binaries checked at start and before each invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 5 rows `LIVE` with every gauge and witness (NEW/NEWa/EMB cadence ACTIVE 1/1 and OFF 0/0; OLD/OLDa neither line; EMB gate 1/1 and echo; every other arm gate 0/0); tunnel NEW MTU 1196/1196 `window/Rlc/bulk`, OLD 1500/1500 `block/RaptorQ/bulk`; one `ship` rep |
| `ABORT-BUDGET` | no (n = 8, no cut) |
| `ABORT-RC` | 0 of 304 |
| `ABORT-BRINGUP` | 0 (0 `NO_DATA`) |
| `VOID-COTENANT` | 0 of 304 |

*What ran.* 16 blocks (8 reps × seeds 42, 7) × 19 invocations = 304 rows, all
`LIVE` (0 contaminated, 0 witness failures); 16 rows per (cell, arm), 8 per
seed; 0 DNF anywhere. Tunnel: 32 bring-ups, all LIVE, 128 transfers, 0
without data. Crown: 64 reps. Ledgers: `docs/l1-raw/verify4/` (`v4.log`
sha256 a40ac80d…, `tun.log` bac95773…, `crown/crown-s42.log` c14cbddb…,
`crown/crown-s7.log` 0ae92989…, `score.txt` with every per-rep value, the
smoke, `TESTS.txt`, the gate outputs, `PLAN.txt`, `BINSHA.txt`, `GO`,
`all-era.txt`); per-invocation endpoint logs (38 MB) and the full test logs
stay on the VM under `/home/vibe/v4/run`.

*(A) NEW vs OLD, window bulk* (n = 16 per arm; goodput median [min–max]
Mbit/s; CPUCLI median s; CPU per datagram = CPUCLI / egress datagrams; §5
relative MDE in brackets; 0 DNF):

| cell | goodput OLD → NEW | CPUCLI OLD → NEW | µs CPU / datagram | OLD vs §5 band | verdict |
|---|---|---|---|---|---|
| `c1s-400` | 299.2 [286.3–305.0] → **502.6** [466.7–536.5] (+68 %, MDE 4.9 %) | 14.67 → **7.42** (−49 %) | 42.7 → 21.8 | in band | **BETTER** |
| `c1d-400` | 187.6 [175.8–219.2] → **305.1** [237.5–390.4] (+63 %, 5.6 %) | 29.66 → **16.92** (−43 %) | 82.3 → 48.4 | in band | **BETTER** |
| `c2-100` | 88.56 → 88.86 (+0.3 %, within 1.4 %) | 4.92 → **3.51** (−29 %, MDE 6.3 %) | 53.9 → 37.7 | in band | **BETTER** (CPU) |
| `c3-25` | 17.34 → 17.37 (+0.2 %, within 1.6 %) | 2.23 → **1.81** (−19 %, 9.6 %) | 86.4 → 68.9 | in band | **BETTER** (CPU) |
| `c7-100` | 175.8 → 173.8 (−1.1 %, within 2.8 %) | 6.81 → **5.11** (−25 %, 3.6 %) | 74.6 → 54.7 | in band | **BETTER** (CPU) |
| `c8-100` | 101.9 → 104.3 (+2.4 %, within 4.0 %) | 8.06 → **5.56** (−31 %, 11.6 %) | 86.6 → 57.7 | in band | **BETTER** (CPU) |

Completion moved with goodput (c1s 10.69 → 6.37 s, c1d 17.06 → 10.49 s,
within MDE elsewhere). Both seeds agree in direction at every cell (per-seed
medians in `score.txt`). The OLD arm reproduced its §5 identity at every
cell (no `CONTROL-MOVED`). `PREDICTION-A MISSED-AT-c2-100, c3-25, c7-100,
c8-100`: predicted SAME there, measured BETTER — on sender CPU only, goodput
within MDE as predicted (the miss is the CPU clause the pre-registration said
was possible; §5 (d) measured −12 to −19 % CPUCLI for the cadence alone, here
−19 to −31 %).

*(B) Auto* (n = 16): `c2-100` NEWa 77.30 vs OLDa 72.47 Mbit/s (+6.7 %, MDE
1.4 %), CPUCLI 6.46 → 4.12 s (−36 %): **BETTER**; `c3-25` 15.57 vs 15.10
(+3.1 %, MDE 1.6 %), CPUCLI 2.57 → 1.92 s (−25 %): **BETTER**. OLDa
reproduced §5's WINa (c2 72.47 in [70.88, 75.12], c3 15.10 in [14.36,
15.74]), so the historical reading scores: **HIST-BETTER** at both (77.30 >
74.12; 15.57 > 15.46). The coded repair share at Auto fell (c2 0.184 →
0.116, c3 0.028 → 0.024), with `plu` off the floor (c2 OLDa 14/16 on the
floor → NEWa 0/16).

*(C) EMB vs NEW* (n = 16): `c1s-400` goodput 495.3 vs 502.6 (−1.4 %, within
4.9 %), CPUCLI 7.42 → **5.94 s** (−20 %, MDE 2.4 %), 21.8 → 17.3 µs per
datagram; `c2-100` goodput within (−0.1 %), CPUCLI 3.51 → **3.13 s**
(−10.8 %, MDE 6.3 %); `c3-25` within on every clause (CPUCLI −6.1 %, MDE
9.6 %). Fed loss vs truth unchanged at every leg (`plc`/truth c1s 2.87 →
3.37, inside [2.21, 3.73]; c2 1.002 → 1.001; c3 1.002 → 0.999). No DNF.
**`FLIP-RECOMMENDED`** (better at `c1s-400`:cpu and `c2-100`:cpu, worse
nowhere, feed unchanged, every cell scoreable), **qualified as
pre-registered**: the batching scope is `live_paths == 1`, a path-count step;
a flip must first express the scope continuously (or show it is not a step),
and dual cells were not measured. Execution: the CPU move proves the
mechanism ran; the indirect GSO witness did **not** rise (c1s 9.39 → 9.15,
c2 4.87 → 4.96, c3 3.29 → 3.33), so the saving is in the per-symbol engine
work, not in deeper kernel batching. P1's c1s goodput gain (+12 % over
cadence-on, n = 1) did not reproduce on this binary: NEW alone already runs at
≈ 500 Mbit/s here.

*(D) Tunnel, inner kernel TCP* (cubic, a cold connection per transfer, c2
50 MB, c3 12 MB; n = 16 transfers from 4 bring-ups per row; every bring-up's
MTU, pipeline and cadence witnesses held): **`MEASUREMENT-RECORDED`**.

| cell / hint | NEW (window, MTU 1196) median [min–max] | OLD (block, MTU 1500) | NEW/OLD | ranges |
|---|---|---|---|---|
| c2 bulk | 77.40 [64.82–80.71] Mbit/s | 73.10 [63.46–84.07] | 1.059 | overlap |
| c2 auto | 71.05 [68.06–73.11] | 44.63 [25.85–54.50] | 1.592 | disjoint |
| c3 bulk | 13.93 [12.32–14.76] | 12.55 [11.68–14.58] | 1.109 | overlap |
| c3 auto | 13.61 [12.56–14.09] | 10.08 [7.70–10.26] | 1.350 | disjoint |

Per-seed medians agree in direction (`score.txt`). The smaller inner MTU
did not cost inner-TCP goodput at these cells: at bulk the window pipeline
is level with or ahead of the block pipeline, and at Auto clearly ahead. As
pre-registered, this contrast confounds MTU with pipeline; no MDE exists for
it, so no verdict beyond the measurement.

*(E) Crown spot:* **`REPAIRS-INERT-ON-CROWN`**. p99 median / p50 median (ms),
all inside §5's bands: c2·400B 36.9/7.85 (s42), 36.4/7.89 (s7); c2·1200B
38.5/8.38, 42.3/8.20; c3·400B 113.1/24.06, 108.9/23.70; c3·1200B 90.9/25.19,
99.1/25.08. `count = 1000` in 64 of 64 reps. Single-rep outliers inside the
median rule: c2·1200B s7 one rep p99 620.8 ms; c2·400B s7 169.4.

*Outside the pre-registered set (findings, no verdict).*
1. **The c1 fed loss now reads well above truth.** `plc`/truth median at
   `c1s-400`: OLD 1.50, NEW 2.87, EMB 3.37 (truth ≈ 0.0011 in all three);
   at `c1d-400` OLD 1.01/1.10 → NEW 1.31/1.37 per leg. Kernel receive-buffer
   drops grew with the rate (`rcvbuf_drops` max per run: c1s OLD 50 → NEW
   138 → EMB 182; c1d 27 → 64); no engine token counts them, so they enter
   `plc` as loss while the egress truth (ahead of netem) does not see them —
   the §5 finding 6 term, larger now that the sender is 68 % faster (c1s
   `plc`/truth 1.50 → 2.87, ≈ 1.9×; `rcvbuf_drops` max 50 → 138, ≈ 2.8×). At c2/c3/c7/c8 the ratio is 0.996–1.010 for every arm. The (C)
   feed clause passed at c1s only because it is a ratio of ratios.
2. **The `sigma_diag_reachability` test is timing-dependent on this
   binary** (1 of 3 isolated re-runs passed): its saturated-tick
   precondition needs more `[DIAG]` ticks than a 0.3–0.8 s loopback run
   gives. A test-harness defect, not an engine failure (the σ clauses it
   reaches all held).
3. **`plu` left the 0.0354 floor everywhere the OLD arm sat on it** (OLD
   c1s 16/16, c1d 16/16 per leg, c2 16/16, c7 7/16, c8 fast leg 13/16 → NEW
   0/16 at each; known effect 1). Coded repair share at bulk stayed ≤ 0.8 %
   in every arm.
4. **The receiver got cheaper too**: CPUSRV median c1s 12.18 → 8.25 s, c1d
   21.29 → 15.88 s at higher goodput; level at c2/c3/c7/c8.
5. **The dual c1 sender is no longer pinned**: client busy 87 % → 82 % at
   +63 % goodput, and NEW's c1d spread is wide (237.5–390.4 Mbit/s).
6. The OLD block pipeline in the tunnel at c2 Auto is the noisiest row
   measured (25.85–54.50 Mbit/s).

*What it means.* The merged `main` is better than the Stage-3 binary on
every cell measured: dual and single 1 Gbit/s links move 63–68 % faster
with about half the sender CPU per datagram, and the loss-bound cells keep
their goodput (they sit near their link ceilings) while the sender spends
19–31 % less CPU. Auto is a few percent faster at c2/c3 on top. Inside the
tunnel, kernel TCP is not hurt by the smaller 1196-byte MTU: it is level or
better at Bulk and clearly better at Auto. The realtime tail did not move.
`RWM_EMIT_BATCH` saves a further 11–20 % sender CPU on single-path cells
without changing goodput or the loss feed; its flip waits on removing its
path-count scope. The one thing to watch is the c1 loss feed: at these
speeds the receiver's kernel drops up to 140–180 datagrams per run and the
engine counts them as path loss, so its fed loss is about three times the
wire's.

## 7. Receive-buffer fix (F-A) — pre-registration

The fix for V4 finding 1 (the c1 receiver's kernel UDP receive-buffer
overflow). Every endpoint socket now requests `SO_RCVBUF` = 4 000 000 B
(`B_req = R_line × T_pause × k_truesize / 2`, derived in fec-arq-model.md
§8.5, "The kernel receive buffer"). Each socket echoes `[RCVBUF] … granted=
clamped=` once at bind, and the receiver's `[CTLD]` line carries the
per-socket kernel drop counter `rxdrop<i>=`. Committed before the
verification run. No number below is a result.

**Probe already run (zero-build, recorded, not scored).** c1s-400, V4 NEW
binary (`aa88d648…`), seed 42, n = 8 per arm, interleaved, both VM locks
held.
- Arm B as specified (`ip netns exec rp-srv sysctl -w
  net.core.rmem_default=…`) **cannot run**. The write is refused with EPERM
  in a child netns: `net.core.rmem_*` is visible there but read-only, and
  only init_net can set it. So AN1's "per-netns" inference is refuted, and
  the harness-sysctl option is unavailable even as a probe. That session is
  therefore an A/A: 15 live rows, `rcvbuf_drops` 17–80 (medians 39 and 36),
  `plc`/truth medians 2.12 and 1.91, goodput medians 473.0 and 475.0 Mbit/s.
  One row is void: its driver script was edited while the run was in
  flight (rc 127).
- The replacement arm B is an `LD_PRELOAD` shim that requests
  `SO_RCVBUF` = 4 194 304 B on the server socket after `bind`. It is the
  engine fix's mechanism, applied from outside. Its witness on every row is
  the shim's echo `granted=8388608` and `ss -uamn` `rb8388608`. Results:
  A `rcvbuf_drops` 22–111 (median 64), `plc`/truth median 2.83, goodput
  median 449.3 Mbit/s; **B `rcvbuf_drops` 0 in 8/8**, `plc`/truth 0.97–1.07
  (median 1.01), goodput median 481.0 Mbit/s.
- Drop-count unit: the per-socket `ss` skmem `d` (= `sk_drops`, the field
  `SO_MEMINFO[SK_MEMINFO_DROPS]` returns) equalled the netns `RcvbufErrors`
  delta on every arm-A row (for example 69/69 and 57/57). It counts skbs,
  that is GRO superpackets (`[TRUTH] gso` median 9.45 at c1s).

**Binaries.** MAIN is the V4 NEW binary, `sha256 aa88d648def0b125…`. It is
reused because `git diff 7d8cec2b ddfc07a` over `raptorpath/src`,
`raptorpath-math`, `gf256`, `Cargo.lock` and `raptorpath/Cargo.toml` is
empty, so it is `main`'s engine. FIX is this branch's HEAD, archived with
`git archive`, built fresh on the VM (`cargo build --release --bin
raptorpath`), `sha256` recorded and re-checked before every invocation.
**Tests first** (FIX tree, on the VM, under the locks): `cargo test -p
raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2`,
`cargo test --doc -p raptorpath --release`, and `cargo test -p
raptorpath-wasm`. A real failure (one that does not pass on an isolated
re-run) is `ABORT-TESTS`. The red-first evidence is recorded beside them:
`every_endpoint_socket_reads_back_the_rcvbuf_floor` on the parent commit
(red) and on HEAD (green).

**Harness.** `perf_rwm_c.sh` from this commit's `tools/l1` (unmodified),
with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window
SEED=<seed>`, one run, `--window-reliable`, a fresh topology per invocation,
and every arm `env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST`. Rows are parsed by `verify4_parse.py row` under the arm
label `NEW`, because both binaries carry V4 NEW's witnesses. A 2 Hz `ss
-uamn` sampler in `rp-srv` records `rb`, `r` and `d` for every receiver
socket.

**Cells and plan.** `c1s-400` and `c1d-400`: MAIN against FIX, n = 8 per
seed × seeds 42 and 7 (16 per arm and cell). `c2-100` is the must-not-move
control, n = 4 per seed × seeds 42 and 7 (8 per arm). Blocks run rep 1 s42,
rep 1 s7, rep 2 s42, and so on. Within each block the cells run in the
order above, and the arm order alternates by rep (rule 3). 80 invocations,
about 25 min.

**Witnesses per row** (a failing row is `WITNESS-FAIL`, excluded and
counted):
- The verify4 NEW witness set: `[PIPE]` window/Rlc/bulk on both ends,
  `[GATES]`, cadence ACTIVE, and no emission batching.
- FIX: one `[RCVBUF]` echo per path on both endpoint logs, with `req=4000000`
  and `granted ≥ 2·min(req, rmem_max)` (8 000 000 on this VM); the server's
  `ss rb` reads 8000000; and `rxdrop<i>=` is present on the server's
  `[CTLD]`.
- MAIN: no `[RCVBUF]` line, and `ss rb` reads 212992.

**Outcomes per c1 cell** (X = FIX, REF = MAIN):
- **`FIXED`** iff all three hold:
  1. `rcvbuf_drops ≤ 2` in at least 15 of 16 FIX rows.
  2. The FIX median `plc`/truth is within [0.9, 1.3] on every leg.
  3. FIX goodput median ≥ MAIN median × (1 − rel_gp), with §5's rel_gp:
     `c1s-400` 4.9 %, `c1d-400` 5.6 %.
- **`NOT-FIXED-<clauses>`** names the clauses that failed. If (1) fails,
  the drain pause exceeds the 32 ms tolerance, and the ranked next lever is
  isolating the endpoint driver, not a larger buffer.
- **`UNSCOREABLE`** with fewer than 12 live rows in either arm, or ≥ 2
  witness-failed rows in either arm.

**Goodput** is also printed as `BETTER` / `SAME` / `WORSE` against the MAIN
median ± rel_gp. This is descriptive: the prediction is SAME-or-BETTER (the
probe gave +7 %, n = 8).

**Control `c2-100`:** `CONTROL-HELD` iff all of these hold:
- FIX goodput median is within MAIN median × (1 ± 0.014).
- `rcvbuf_drops` = 0 in every row of both arms.
- FIX median `plc`/truth is within [1/1.3, 1.3] × MAIN's.

Otherwise `CONTROL-MOVED` (named beside the c1 verdicts). Fewer than 6 live
rows per arm is `UNSCOREABLE`.

**Unit check (reported, not scored).** On FIX rows, the server's last
`[CTLD] rxdrop` sum is compared with the run's `rcvbuf_drops` and with the
final `ss d` value. If FIX drops are 0, this confirms the wiring only. The
unit evidence is the probe's non-zero A rows above.

**Abort causes, in priority order:**

| # | cause | token |
|---|---|---|
| 1 | either VM lock not free within the 25 min wait | `ABORT-LOCK` (nothing run) |
| 2 | CR bytes in the synced `tools/l1` scripts | `ABORT-CRLF` |
| 3 | build failure | `ABORT-BUILD` |
| 4 | a real test failure | `ABORT-TESTS` |
| 5 | binary `sha256` mismatch at any invocation | `ABORT-SHA` |
| 6 | smoke (one FIX c1s run) missing a FIX witness | `ABORT-SMOKE` |
| 7 | `raptorpath` already running at start | `BUSY` |

**Budget.** 5 h cap from the first ssh of the session. Build and tests are
about 45 min, the battery about 25 min. No truncation is expected; if the
cap binds, the battery stops at a rep boundary and is scored at the n it
reached.

**Amendment 1** (committed before any scored row exists). Session 1
(2026-10-04 23:44 UTC) is void and was not scored. It ended
`ABORT-SMOKE` with no battery row. The envelope wrote its driver output to
`/tmp/fa-drv.out`, which the probe had left behind owned by root, so the
smoke invocation never started (rc 1, 0 s). The smoke check then read the
probe's stale output and failed as it should. That session's test stage is
also not evidence for the green tree. It shared one cargo target between
the red tree (0597da4) and the FIX tree, and `git archive` gives every file
its commit time as mtime, so cargo reused the red tree's lib-test artifact
for the FIX tree. The FIX-tree run of
`every_endpoint_socket_reads_back_the_rcvbuf_floor` therefore failed with
the red value (212992). Its red-tree run does stand as the red evidence: it
was the first lib-test build in that target, from 0597da4's sources, and
failed with SO_RCVBUF 212992 < floor 8000000. Session 3 re-runs build,
tests, smoke and battery as pre-registered, with three changes: the FIX
tree's mtimes are refreshed, it uses its own target, and its temporary
files sit in the run directory. The verification FIX binary is session 3's
build.

### 7.1 Receive-buffer fix — result (scored against §7 and amendment 1)

**Session.** Session 3 ran 2026-10-05, 02:28–03:23 UTC, with both locks
held for the whole session. It was launched three times: the first two
attempts ended `ABORT-LOCK` while another battery held the locks, and
nothing ran in them. CPU: Xeon E5-2650 v3. FIX is commit 10b9d78, `sha256
85ae2568…`, the same hash as session 1's build. MAIN is `aa88d648…`. Raw
data is in `/home/vibe/fa/run3/` on the benchmark VM (`fa.log`, `diag/`,
`TESTS.txt`, `BINSHA.txt`).

**Abort table:** no abort cause fired. 81 rows: 1 smoke row and 80 battery
rows. Every one of the 80 battery rows is `LIVE` with all witnesses
passing, and none is excluded.

**Tests (FIX tree):**

| suite | passed | failed | ignored |
|---|---|---|---|
| `cargo test -p raptorpath -p raptorpath-math --release` | 1042 | 0 | 50 |
| `--doc` | rc 0 | | |
| `raptorpath-wasm` | 35 | 0 | |
| `rcvbuf` lib tests | 5 | 0 | |
| `rcvbuf_reachability` | 1 | 0 | |

Red-first: `every_endpoint_socket_reads_back_the_rcvbuf_floor` failed on
0597da4 (SO_RCVBUF 212992 < floor 8000000) and passes on 10b9d78.

**Witnesses.** Every FIX invocation echoed `[RCVBUF] … req=4000000
granted=8000000 via=SO_RCVBUF clamped=0` for every path, on both ends: 57
server and 57 client sockets. Its server `ss rb` read 8000000 on every row,
and its server `[CTLD]` carried `rxdrop<i>=` on every path. No MAIN row
echoed `[RCVBUF]`, and every MAIN `ss rb` read 212992.

| cell | arm | n | `rcvbuf_drops` (skbs) | `plc`/truth median per leg | goodput median (Mbit/s) [range]; per seed 42 / 7 |
|---|---|---|---|---|---|
| `c1s-400` | MAIN | 16 | 29–163, median 62 | 2.69 | 444.7 [411.7, 494.8]; 444.7 / 444.0 |
| `c1s-400` | FIX | 16 | **0 in 16/16** | **1.03** | 452.4 [377.2, 482.5]; 452.4 / 458.9 |
| `c1d-400` | MAIN | 16 | 4–104, median 27.5 | 1.34 / 1.63 | 284.2 [257.0, 359.5]; 284.0 / 284.6 |
| `c1d-400` | FIX | 16 | **0 in 16/16** | **1.02 / 1.04** | 283.7 [252.8, 327.5]; 295.1 / 278.7 |
| `c2-100` | MAIN | 8 | 0 | 0.99 | 88.9 [87.9, 89.2] |
| `c2-100` | FIX | 8 | 0 | 1.00 | 89.0 [88.5, 89.3] |

**Verdicts:**
- `c1s-400`: **`FIXED`**. Clause (1) holds in 16/16 rows, clause (2) at
  1.03, clause (3) at +1.7 %. Goodput reads `SAME` (descriptive).
- `c1d-400`: **`FIXED`**. Clause (1) holds in 16/16 rows, clause (2) at
  1.02 and 1.04, clause (3) at −0.2 %. Goodput reads `SAME`.
- `c2-100`: **`CONTROL-HELD`**. Goodput +0.03 %, 0 drops in both arms,
  feed 1.00 against 0.99.

**Unit check.** On all 40 FIX rows, the server's `rxdrop` sum, the netns
`RcvbufErrors` delta and the `ss d` value are all 0. This confirms the
wiring only. The unit evidence is the probe's: `ss d` (= `sk_drops`, the
field `SO_MEMINFO` reads) equalled `RcvbufErrors` on every non-zero row.
So the unit is skbs (GRO superpackets, about 9.4 datagrams each at c1s),
not datagrams. The token stays an instrument and is not fed to the
estimator.

**What it means.** The kernel receive buffer was the whole c1 loss-feed
excess. With the socket asking for 4 MB, the receiver drops nothing at
either c1 cell, and the engine's fed loss matches the wire's (`plc`/truth
1.02–1.04, down from 1.3–2.7). Goodput did not move beyond the MDE in
either direction: the probe's +7 % at c1s was not reproduced at n = 16. The
100 Mbit/s control is unchanged. V4 finding 1 is closed for hosts whose
`rmem_max` is at least 4 MB. On older hosts (`rmem_max` 212 992) the echo
reads `clamped=1` unless the engine runs as root, where `SO_RCVBUFFORCE`
applies.

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

## 9. Threading redesign P0 — per-thread core budget — pre-registration

Phase P0 of the threading redesign (owned paths, batched rings, no shared
hot locks): **measure, no behaviour change.** Committed before VM contact;
no number below is a result. Nothing is flipped by this battery.

**Question (D3).** At `c1s-400` the shipped stack is pinned near 510 Mbit/s
with the sender busy 35 % (§10 of `feat/quic-feeder-v2`, recorded in §10
below as evidence), which makes the ceiling a non-sender one. The topology
audit (D3) names the server's single receiver task (`run_receiver`:
decode, deliver, one WindowAck per data datagram) as the best-supported
candidate: P1 measured it at 0.61 core at 315 Mbit/s; scaled to 510 Mbit/s
with §10's 28 % lower server CPU per byte that is ≈ 0.71 core, and ≈ 0.99
if the receiver took no share of the saving. **Is any single server thread
or task at ≥ 0.9 core at `c1s-400`?** `c1d-400` is measured beside it for
the budget table and is not part of the question.

**Stop rule (from the plan).** If `D3-CONFIRMED` (below), P2's server half
— the receiver-role split — becomes the primary gate for `c1s`, and that
is stated in this file before P2 runs.

**What changed in the engine (measurement only).**

1. `main.rs`: `#[tokio::main]` is replaced by its own expansion,
   `Builder::new_multi_thread().enable_all().build()` + `block_on` (same
   default worker count, the available parallelism: 6 on the VM), plus
   `thread_name_fn` naming each pool thread `rp-w-<n>` in spawn order. No
   other runtime knob changes. Workers are launched first, in index order,
   so `rp-w-<i>` for `i < num_workers` is worker `i` — a hypothesis from
   tokio's launch order (the join, `worker_thread_id`, is unstable);
   `n ≥ num_workers` are blocking-pool threads (none expected on Linux).
2. `src/rtobs.rs`: the `[THR]` readout from the STABLE `RuntimeMetrics`
   only (`num_workers`, `worker_total_busy_duration`, `worker_park_count`,
   `worker_park_unpark_count`; no `tokio_unstable`), one `[THR] rt` line per
   worker; on Linux one `[THR] os` line per OS thread from
   `/proc/self/task/<tid>/stat` (utime + stime, `comm`), the `block_on`
   main thread (`comm=raptorpath`) included; one `[THR] sum` line;
   elsewhere `[THR] os unavailable` (Windows builds: the reader is
   `cfg(target_os = "linux")`).
3. The `[LAG]` probe: one task on the runtime ticks every 10 ms (tokio
   `interval`, `MissedTickBehavior::Delay`) and logs `now − scheduled`;
   p50/p99/max printed. Provenance: the Cats Effect starvation checker
   (th1 §1.5; paper §11.2 row `LAG_TICK`). **Instrument floor**: tokio
   rounds timer deadlines up to the next millisecond, so every tick reads
   0–1 ms late by construction — p50 ≈ 0.5–1 ms is the floor, not
   starvation; p99 and max are the signal. It runs on a worker and does not
   see the main thread.
4. Windows of measurement: `phase=xfer` lines bracket one perf object — the
   client's timed run (`run=1`; snapshot before `run_object`, lines after its
   ack) and the server's object (`obj=1`; snapshot at its first packet,
   lines after its completion ack is handed to the engine). Per-thread
   `cores` = thread CPU over that window ÷ the window's wall. `phase=run`
   lines are printed once at process end (cumulative since the runtime was
   built; the server's ride the SIGTERM graceful path; reported only).
5. Harness: `perf_rwm_c.sh` echoes each endpoint's `[THR] sum phase=xfer`
   and `[LAG] phase=xfer` lines into the driver output; the full tables stay
   in the endpoint logs the battery copies. `l1common.thr`/`thr_columns`
   parse them (offline tests in `test_l1common.py`, pinned to the Rust
   renderer's unit-test strings).

**Component characterization (rule 14), committed with the code.** Unit
tests in `rtobs.rs`: the `/proc` stat parser (comm counted from the last
`)`), the quantile rule (= `l1common.q`), the exact `[THR]` token set and
its deltas, the `os unavailable` path, `[LAG]` absent-as-`-`, and a live
runtime whose worker is named `rp-w-*` and whose probe yields ≥ 10 samples
in 250 ms with p99 ≥ p50. The routing test `tests/rtobs_reachability.rs`
runs the SHIPPED binary as perf client and server and asserts both ends'
`phase=xfer` and `phase=run` tables (one runtime: equal worker counts),
`busy_frac` in [0, 1.05], `[LAG]` n > 0, and on Linux an `rp-w-*` thread and
the main thread in `[THR] os`. What the battery should then see: 6 workers
per side; the server's hottest thread somewhere in 0.6–1.0 core; the
client's sender-hosting worker ≈ 0.35–0.6 core. What the benches cannot
see: which task a worker's CPU belongs to (tasks migrate between workers,
D7), hence the `[RDIAG]` reading below.

**Instruments for D3** (both, on the server, in the `c1s-400` P0 rows):

- **task-level**: `RWM_RDIAG=1` (exists, `net/mod.rs` ≈ 1250,
  `receiver.rs` ≈ 1115): `[RDIAG] busy=<%>` = 1 − (time the receiver task
  spent awaiting its `select!`) ÷ wall, every ≥ 500 ms, plus the inbound
  `msg_tx` depth. `busy` counts the task's CPU AND its blocked time inside
  its poll (quinn's mutex, the scheduler lock), so it is ≥ the task's
  cores; a ready-but-unscheduled task counts as idle. The per-row reading
  is the median over the IN-TRANSFER lines (`msgs ≥ 1000/s`; the transfer
  runs ≈ 53 k datagrams/s, the idle tail < 100/s). `busy ≥ 90 %` is read as
  "the receiver task is the service wall (≥ 0.9 core-equivalent of wall,
  lock wait included)";
- **thread-level**: the hottest server OS thread's `cores` in the `obj=1`
  window. A thread can host several tasks and a task can spread over
  threads, so ≥ 0.9 here is sufficient for a saturated thread, not
  necessary for a saturated task.

**Binaries and arms.** Two binaries, interleaved in one session (rule 3):
`P0` = this branch's HEAD (archived with `git -c core.autocrlf=false -c
core.eol=lf archive`, built fresh on the benchmark VM in a fresh run root
and target dir); `MAIN` = `main` 8d7d8c1 (the shipped stack the §10 NEW arm
measured, modulo §10's own known effect 1), archived the same way and built
in its own fresh target dir. `sha256` of both in `BINSHA.txt`, re-verified
before every invocation. MAIN is the control for the no-behaviour-change
check; it is added to the brief's design because §10's numbers are another
session (cross-session drift has measured 2.3×, rule 3) and §10's NEW was
not byte-identical to `main`.

**Invocation.** Every row is `perf_rwm_c.sh c1 c1 bulk 400000000 1
<single|dual>` (`c1s-400` = `c1 c1 single 400000000`, `c1d-400` = `c1 c1
dual 400000000`, §5's geometry), `--window-reliable`, fresh topology, with
`RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`; every
arm first `env -u`'s `RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST RWM_RDIAG` (rule 15d) and sets `RWM_RDIAG=1` explicitly;
`RWM_RDIAG` is in `RWM_FORWARD`, so it reaches BOTH ends (the client's
reading measures its ack task; reported). Neither §10 arm ran `RWM_RDIAG`.

**Plan.** Blocks (rep 1, seed 42), (rep 1, seed 7), (rep 2, seed 42): in
each block `c1s-400` then `c1d-400`, each P0 and MAIN, the arm order
rotated by the block index. **n = 3 per arm and cell** (2 × s42, 1 × s7;
both seeds reported, rule 4); 12 invocations.

**Capacity and headroom (rule 16).** `c1s-400`: 1000 Mbit shaped, §10 NEW
510.4 Mbit/s, ≈ 48 % headroom; `c1d-400`: 2000 Mbit, 424.4, ≈ 79 %. No
throughput target is set; goodput and CPU are no-change guards.

**Witnesses per row** (`p0_parse.py row`; a failing row is `WITNESS-FAIL`
or `CONTAMINATED`, excluded and counted; no client summary is `NO_DATA`):
§5's set via `stage3_parse.make_row` (`[PIPE] window/Rlc/bulk` and the
driver header; `[GATES]` on both ends; the RLC auto-select line on both;
cadence ACTIVE on both; `[GATES] RWM_POOL_ANCHOR=0` on both; no generation
guard), plus `[GATES] RWM_EMIT_BATCH=1` and `RWM_RDIAG=1` on both ends,
plus ≥ 1 in-transfer `[RDIAG]` line on the server; P0 rows: the `phase=xfer`
`[THR] rt` lines, the `[THR] sum` line, `[LAG]` with n > 0, and (Linux)
`[THR] os` lines naming an `rp-w-*` thread, on BOTH ends; MAIN rows: no
`[THR]`/`[LAG]` line on either end (two-sided, rule 15c: the wrong binary
fails).

**Outcomes.**

*D3* (P0 rows at `c1s-400`; medians over the live rows):
- **`D3-CONFIRMED`** iff median server `[RDIAG] busy` ≥ 90 % OR median
  hottest-server-thread `cores` ≥ 0.90. The stop rule fires and is recorded
  in the result.
- **`D3-REFUTED-WITH-RECORD`** iff both are readable at ≥ 3 live rows and
  both are below their threshold.
- **`UNSCOREABLE`** with fewer than 3 live P0 rows at `c1s-400`;
  **`NEEDS-MORE-<rdiag|thr>`** when one instrument is unread at ≥ 3 rows and
  the other is below its threshold.

*No-behaviour-change check* (P0 against MAIN, in session, per cell, for
goodput and `CPUCLI`, §5's committed relative MDE: `c1s-400` 4.9 % / 2.4 %,
`c1d-400` 5.6 % / 6.5 %): a clause is *within* iff |med(P0)/med(MAIN) − 1|
≤ MDE; outside the MDE it is *MOVED* when the two arms' [min, max] ranges are
disjoint and *UNDERPOWERED* when they overlap. **`NO-CHANGE-HELD`** iff every
clause is within; **`REFUTED-WITH-RECORD`** (the instrument changed
behaviour) iff any clause is MOVED; **`GUARD-UNDERPOWERED`** otherwise;
**`UNSCOREABLE`** with fewer than 3 live rows of either arm at a cell.
Reported beside it, not scored: P0's medians against the §10 NEW numbers
the brief names (`c1s-400` 510.4 Mbit/s and 5.73 s `CPUCLI`; `c1d-400` 424.4
and 11.19 s), and MAIN's identity against §10's control band (goodput
median within ±2 × rel_gp: `c1s-400` [467.1, 568.5], `c1d-400` [377.8,
473.0]; outside = `CONTROL-MOVED` beside that cell).

*The deliverable* (not an outcome; reported in every case): a per-thread
core budget for client and server at both cells — the OS threads ranked by
`cores` (`thr_r1..3`, with the hottest thread's `comm` per rep), the main
thread, the `rp-w-*` sum, the process sum, the six workers' `busy_frac`
ranked, park/unpark per second, `[LAG]` p50/p99/max, `[RDIAG]` busy and
`q_avg`, median [min–max] with n, and every per-rep value.

**Predictions** (checks, read as `MET`/`MISSED` in the result): D3 is borderline —
server `[RDIAG] busy` 75–95 % and the hottest server thread 0.6–0.9 core
(the derivation above); `NO-CHANGE-HELD` at both cells (the instrument adds
≈ 100 timer wakes/s and two `/proc` reads per object, ≪ the ≈ 50 k
datagrams/s and 16–39 k worker park/unparks per second).

**Known effects, declared.** (1) The lag probe is a new task on the runtime
in the P0 binary (≈ 100 wakes/s); (2) the perf ends read `/proc` (≈ 10
files) on the main thread at the object's first packet (server) or before
it (client), and again at its end; (3) `RWM_RDIAG=1` adds two clock reads
per inbound message on both ends in BOTH arms; (4) thread names change
`comm` from `tokio-runtime-w` to `rp-w-<n>`. These are what the
no-behaviour-change check measures.

**Abort causes, in priority order** (the scored section opens with this
table, filled): `ABORT-LOCK`, `ABORT-CRLF`, `ABORT-BUILD` (either binary),
`ABORT-TESTS` (`cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`, `cargo test --doc -p raptorpath
--release`, `cargo test -p raptorpath-wasm` with `GOLDEN_CAPTURE` unset, and
the python parser tests; a failure that does not pass on an immediate
re-run of that test alone is real — the automatic GO is withheld and the
operator writes GO only for a documented flake, e.g. the known
`sigma_diag_reachability`), `ABORT-SHA`, `ABORT-SENTINEL-UNWRITABLE`,
`ABORT-SMOKE` (smoke = `c1s-400` P0, `c1s-400` MAIN, `c1d-400` P0, seed 42:
every row `LIVE` with its CPU line and `[TRUTH]` per leg; both arms
present), `ABORT-RC` (row `VOID-RC`, the battery goes on), `ABORT-BRINGUP`
(`NO_DATA` after 2 attempts). Void class `VOID-COTENANT`. The Windows check
(`cargo check -p raptorpath --target x86_64-pc-windows-gnu`, if that target
is installed on the VM) is recorded and does not gate.

**Budget.** ≈ 35 min build + tests (§10's session: 31 min), ≈ 10 min for
main's binary, ≈ 1 min smoke, 12 invocations × ≈ 20 s ≈ 4–5 min; ≈ 55 min
of the 5 h cap; hard backstop at lock acquisition + 4 h 50 min. No cut is
planned; a truncated battery is scored at the n reached, and a null at
reduced n reads `GUARD-UNDERPOWERED` / `NEEDS-MORE-<n>`, never a
refutation.

**Session rules.** §10's: both locks for the whole session via
`lib_battery.sh` (`p0_run_all.sh`, started by the lock waiter
`p0_launch.sh`); the operator checks at most once per ≈ 20 min (rule 13,
recorded); `pkill -x raptorpath` only; no `ens18`, firewall, `sshd` or
non-`rp-*` namespace is touched. Harness: `tools/l1/p0_run_all.sh`,
`p0_battery.sh`, `p0_parse.py` (offline test `test_p0_parse.py`). Compact
ledgers are copied to `docs/l1-raw/thread-p0/`.

**Amendment A1** (committed before any scored result; no battery row
exists). Session 1 (locks 16:05:05Z–16:39:32Z, run root `thrp0`, binary
`743bc004…` built from `dc38801`) ended `ABORT-TESTS`: the suite read 1057
passed, **1 failed**, 50 ignored (doc rc 0; wasm 35 passed; the python
parser tests 88 / 31 / 45 checks, 0 failed; every `runtime_obs` unit test
and the reachability test passed; the Windows check not run: the VM has no
`x86_64-pc-windows-gnu` target). The failure was real, not a flake, and
caused by this tree: `store_cap_sf_bench::the_wires_offered_load_has_no_congestion_control`
bans the substrings of congestion-response names (`rto`, `rtt`, `cwnd`, …)
in `src/perf.rs`, and the new module's name, `rtobs`, contains `rto`. The
operator wrote NOGO (16:33Z); the envelope still built main's binary and
ran the smoke (3 rows `LIVE`, `SMOKE-PASS`; nothing in it is a result),
then stopped on the NOGO and released both locks; the VM was left with 0
`raptorpath` and 0 `rp-*` namespaces. Changes, in `277bd0b`: the module is
renamed `runtime_obs` (and its routing test
`tests/runtime_obs_reachability.rs`); the lexical ban is kept as is; no
other code change. What they amend here: every `rtobs` above reads
`runtime_obs`; the P0 binary is built from this amendment's commit (was
`dc38801`); the run is a fresh session (fresh run root `thrp0b` and fresh
target dirs for both binaries), the 5 h cap counted from its own lock
acquisition. Nothing else changes.

**Result** (scored 2026-10-05 against this pre-registration and amendment
A1, literally): **D3 `D3-REFUTED-WITH-RECORD`** at `c1s-400` (server
receiver task `[RDIAG] busy` median 79.0 %, hottest server thread 0.355
core; both below their thresholds). The stop rule did NOT fire: P2's server
half does not become the `c1s` gate on this evidence. **No-behaviour-change
check `REFUTED-WITH-RECORD`**: at `c1s-400` P0's goodput median is 6.7 %
below MAIN's with disjoint ranges (n = 3 each); every other clause is
within or underpowered. Nothing is flipped.

*Binary and session.* Session 2 (A1): P0 from `f718009` (engine
`277bd0b`), `sha256 047a2270f2726f144ddef46a296b26942fb25f8e0b11ec24a5ae60ca58eeb4ed`;
MAIN from `8d7d8c1`, `sha256 5dccf33bf647f3856a4041085822af0aab8125d7cd9e9302d18d2e61ce763d12`;
both archived LF and built fresh on the benchmark VM (Xeon E5-2650 v3 era,
6 vCPU) in run root `thrp0b` with fresh target dirs. The lock waiter
started 16:45Z and waited for another session's locks; locks held
17:44:48Z–18:20:46Z (**36 min** of the 5 h cap; session 1 used 34 min,
A1). Build 4.3 min, tests 25 min, main's build 4.4 min, smoke 28 s, AUTO-GO
18:18:52Z, battery 114 s (12 invocations). VM left quiet: 0 `raptorpath`,
0 `rp-*` namespaces, both locks released by the envelope.

*Tests (VM, release, session 2).* `cargo test -p raptorpath -p
raptorpath-math --release --no-fail-fast -- --test-threads=2` (90
binaries): **1 058 passed, 0 failed**, 50 ignored (all six `runtime_obs`
unit tests and `the_thr_and_lag_lines_fire_on_both_perf_ends` ok); `cargo
test --doc` rc 0; `cargo test -p raptorpath-wasm` 35 passed
(`GOLDEN_CAPTURE` unset); python parser tests 88 / 31 / 45 checks, 0 failed.
Windows: the VM has no `x86_64-pc-windows-gnu` target, so the envelope's
check did not run; the tree was checked on the Windows host instead (native
MSVC target, `cargo check -p raptorpath --bin raptorpath --test
rtobs_reachability` and `--lib --profile test`, before the rename; clean;
and after it, on `d465adc`, `--bin raptorpath --test
runtime_obs_reachability`: clean).

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` | no (session 2 took both at 17:44:48Z after waiting on a foreign holder) |
| `ABORT-CRLF` | no (0 CR bytes in every `tools/l1` script) |
| `ABORT-BUILD` | no (both binaries) |
| `ABORT-TESTS` | session 1 yes (A1); session 2 no |
| `ABORT-SHA` | no (checked at start and before every invocation) |
| `ABORT-SENTINEL-UNWRITABLE` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS`, 3 rows `LIVE`, both arms |
| `ABORT-RC` | 0 of 12 |
| `ABORT-BRINGUP` | 0 (0 `NO_DATA`) |
| `VOID-COTENANT` | 0 of 12 |

*What ran.* 3 blocks × 4 = 12 rows, all `LIVE` (every witness on every row:
`[GATES] RWM_EMIT_BATCH=1` and `RWM_RDIAG=1` on both ends; in-transfer
`[RDIAG]` on the server; P0 rows the `[THR] rt/os/sum` and `[LAG]` window
lines on both ends with an `rp-w-*` thread; MAIN rows no `[THR]`/`[LAG]`).
The server's `phase=run` lines were present in 6/6 P0 rows (the graceful
path works). n = 3 per arm and cell (seeds 42, 7, 42). MAIN is `IN-BAND`
against §10's control band at both cells. Ledgers:
`docs/l1-raw/thread-p0/` (`p0.log` sha256 eea18176…, `score.txt` with
every per-rep value, `TESTS.txt`, `BINSHA.txt`, smoke, `all-era.txt`,
session 1's `TESTS`/era/NOGO, and the full `[THR]`/`[LAG]`/`[RDIAG]` lines
of one c1s and one c1d P0 row); the per-invocation endpoint logs stay on
the VM under `thrp0b/run/diag-p0`.

*Goodput and CPU* (median [min–max], n = 3; §5 MDE):

| cell | goodput P0 / MAIN (Mbit/s) | Δ | CPUCLI s P0 / MAIN | Δ | CPUSRV s P0 / MAIN | µs CPU per datagram P0 / MAIN |
|---|---|---|---|---|---|---|
| `c1s-400` | 483.2 [461.7–491.2] / 518.2 [493.3–518.5] | **−6.7 % (MDE 4.9 %), ranges disjoint: MOVED** | 5.98 [5.88–6.04] / 5.79 [5.76–5.99] | +3.3 % (MDE 2.4 %), ranges overlap: UNDERPOWERED | 8.70 / 8.14 | 17.57 / 17.01 |
| `c1d-400` | 411.3 [392.7–424.0] / 413.2 [396.1–413.4] | −0.4 %: within | 11.56 / 11.51 | +0.4 %: within | 12.59 / 12.45 | 33.25 / 33.12 |

Against the §10 NEW numbers the brief names (reported, not scored): P0
`c1s-400` −5.3 % goodput, +4.4 % `CPUCLI`; `c1d-400` −3.1 %, +3.3 %.

*D3* (P0 rows at `c1s-400`; per rep s42 r1 / s7 r1 / s42 r2):

| reading | per rep | median | threshold | |
|---|---|---|---|---|
| server receiver task `[RDIAG] busy` (in-transfer lines) | 79.0 / 79.0 / 80.0 % | 79.0 % | ≥ 90 % | below |
| server `msg_tx` depth `q_avg` | 471 / 516 / 500 | 500 | — | — |
| hottest server OS thread (`cores`) | 0.273 / 0.355 / 0.381 | 0.355 | ≥ 0.90 | below |

**Per-thread core budget** (P0 rows, the object's own window ≈ 6.6 s at
c1s, ≈ 7.8 s at c1d; median [min–max], n = 3; `rp-w-*` are the six tokio
workers, `raptorpath` the `block_on` main thread that runs the perf
generator/sink):

| | c1s client | c1s server | c1d client | c1d server |
|---|---|---|---|---|
| hottest thread | 0.173 [0.162–0.175] | 0.355 [0.273–0.381] | 0.246 [0.232–0.264] | 0.301 [0.277–0.312] |
| 2nd / 3rd thread | 0.160 / 0.158 | 0.263 / 0.206 | 0.244 / 0.227 | 0.268 / 0.247 |
| main thread (`raptorpath`) | 0.109 | 0.175 | 0.123 | 0.137 |
| all workers (`rp-w-*`) | 0.771 | 1.112 | 1.355 | 1.472 |
| process total | 0.879 [0.856–0.918] | 1.287 [1.280–1.305] | 1.478 | 1.609 |
| worker `busy_frac`, ranked 1…6 | .20 .19 .18 .16 .13 .05 | .39 .29 .23 .16 .09 .05 | .35 .34 .32 .32 .28 .24 | .36 .31 .28 .27 .26 .22 |
| worker parks / park-unpark events per s | 8 448 / 16 896 | 8 544 / 17 089 | 22 520 / 45 039 | 19 375 / 38 750 |
| `[LAG]` p50 / p99 / max (µs) | 1 034 / 2 754 / 5 832 | 917 / 1 954 / 3 188 | 179 / 1 907 / 4 916 | 961 / 2 107 / 7 363 |
| `[RDIAG] busy` (its receiver task) | 10 % | 79 % | 19 % | **92 %** |

Per rep the hottest thread was a different worker each time (client c1s
`rp-w-5`, `rp-w-2`, `rp-w-5`; server c1s `rp-w-4`, `rp-w-2`, `rp-w-1`); the
full per-thread lists are in `score.txt`.

**Predictions** (checks): D3 borderline — server `[RDIAG] busy` 75–95 %
`MET` (79 %); hottest server thread 0.6–0.9 core `MISSED` (0.355);
`NO-CHANGE-HELD` at both cells `MISSED-AT-c1s-400`; 6 workers per side
`MET`; the client's sender-hosting worker 0.35–0.6 core `MISSED` (no client
thread above 0.175 at c1s).

*Outside the pre-registered set (findings, no verdict).*

1. **At `c1d-400` the server's receiver task is at the 90 % line**: busy
   92 % in all three reps (`q_avg` 227–390), while at `c1s-400`, the cell
   the question named, it is 79 %. On this reading the single receiver
   task is closer to being the wall on the dual cell than on the single
   one. The D3 stop rule names `c1s` only and does not fire; whether P2's
   server half should gate `c1d` is a question for P2's pre-registration.
2. **No thread is hot, on either side, at either cell: the stock runtime
   spreads the work evenly over all six workers.** At c1s the client's
   ≈ 0.88 core is spread at ≤ 0.175 per thread over five workers, and the
   server's 1.29 cores at ≤ 0.38 per thread; the hottest worker changes
   from rep to rep. Tasks migrate between workers (D7), so per-OS-thread
   CPU cannot localise a task under the stock runtime; the task-level
   reading (`[RDIAG]`) is the one that answers per-task questions until
   tasks are pinned to threads (P2).
3. **Wake churn, measured with stable metrics**: 8.4–8.5 k worker parks per
   second per side at c1s, 19–23 k at c1d (`worker_park_unpark_count` is
   ≈ 2 × `worker_park_count`, so it counts both transitions). This is the
   16–39 k sleeps/s order the off-CPU captures showed.
4. **tokio's busy time exceeds the OS CPU on the client** (c1s window:
   workers' `busy_s` sum 6.32 s against 5.98 s of process CPU): busy
   duration includes time a worker spends blocked inside a poll (quinn's
   connection mutex, `parking_lot` waits), which the OS does not charge.
5. **No starvation beyond a few ms**: `[LAG]` p99 1.9–2.8 ms and max
   3.2–8.7 ms everywhere. The stated instrument floor (p50 ≈ 0.5–1 ms) held
   except on the c1d client (p50 179 µs). A hypothesis, not measured:
   there workers are rarely parked and the timer is serviced by busy
   workers' maintenance ticks rather than by a parked worker's 1 ms-rounded
   sleep. The floor statement is therefore an upper description, not a
   bound.
6. **The no-behaviour-change refutation is not localised.** It rests on
   n = 3 per arm with the ranges 2.1 Mbit/s apart (P0 max 491.2 against
   MAIN min 493.3); the P0 c1s CPU per datagram is +3.3 % (overlapping
   ranges) and the server's `CPUSRV` +6.9 % (8.14 → 8.70 s, overlapping
   ranges; unscored) — the larger move is on the side that reads `/proc`
   at the object's first packet; at c1d nothing moved. Which declared known effect (the 100 Hz
   lag probe; the two `/proc` reads on the perf main thread per object,
   the server's at the object's first packet; the thread names) would
   cost ≈ 3–7 % at c1s only is not measured here. Consequence for P2: every
   arm of the P2 battery must carry the same instrument (the plan's arms
   all print `[THR]`/`[LAG]`), so this offset is common to the arms and is
   not attributed to the topology; a comparison against an
   instrument-free binary must name it.

*What it means.* The ≈ 500 Mbit/s single-path ceiling is not a single
saturated server receiver task and not any single saturated server
thread on this evidence: the receiver task is 79 % busy (lock waits
included) and no thread exceeds 0.4 core, while the
server process uses 1.29 cores spread over six workers that park ≈ 8.5 k
times per second. The other server tasks (quinn's ConnectionDriver and
EndpointDriver, the datagram reader) were not measured per task, and
because tasks migrate (finding 2) the flat per-thread profile cannot bound
them: one of them at ≥ 0.9 core spread over six workers would look the
same. The dual cell is different: there the receiver task is
at 92 %. The per-thread budget the redesign needs is above; its main
lesson is that under the stock scheduler every hot task wanders across all
six workers, so the owned-path layout's first observable is that the work
concentrates on the named owner threads. The instrument itself carried a
c1s goodput cost that the pre-registered check refutes as "no change"; P2
must compare instrumented arms only.

## 10. The per-path datagram feeder — evidence record (code not merged; superseded by the threading redesign)

Recorded here as evidence for the threading redesign (§9; the topology
audit cites it). The two batteries ran on branch `feat/quic-feeder-v2`,
whose `docs/status.md` §9 and §10 hold the full pre-registrations, results
and ledgers (`docs/l1-raw/feeder/`, `docs/l1-raw/feeder2/` on that branch).
**The feeder code is not merged**: neither battery recommended a flip, and
the threading redesign replaces the mechanism (a queue + drainer in front of
quinn's connection mutex) with exclusive per-path ownership, which removes
the contended mutex instead of moving the wait.

**Feeder v1** (that branch's §9; a per-path FIFO + tokio-task drainer for
every datagram, data and control; engine `7729992`, binary `1d2b7bdc…`,
n = 8 × 2 seeds, 128 invocations, VM 2026-10-05). Result
`WORSE-AT-c1s-400,c1d-400,c8-100` (`c2-100` SAME). `c1d-400` goodput 292.4 →
491.4 Mbit/s (+68 %) and CPUCLI −32 %; `c1s-400` CPUCLI +25.8 % (MDE 2.4 %),
4 of 16 FEED rows collapsed (down to 77.7 Mbit/s, RTprop floor up to
83 ms) because the server's ack feeder sat behind its receiver task (server
sojourn max 42–94 ms); `c8-100` goodput −4.5 %. The SOJOURN-BOUNDED clause
was mis-derived (iteration 19 µs, not 50–150 µs). Off-CPU capture at c1d:
sender wall asleep in quinn's `send_datagram`/mutex 0.202 → 0.016 (the wait
moved to the drainer, ≈ 0.10 of its wall).

**Feeder v2** (that branch's §10; data-only queue, control datagrams on the
caller's thread; drainer as a tokio task `FDT` or an OS thread `FDH`; the
direct seam `NEW` as control; engine `c9742a8`, binary `cd9fb661…`, n = 8 ×
2 seeds, 192 invocations, VM 2026-10-05, emission batching on). Result
**`FDT WORSE-AT-c1s-400,c1d-400,c2-100`; `FDH WORSE-AT-c1s-400,c2-100`**;
nothing flipped. Medians, n = 16:

| cell | goodput NEW / FDT / FDH (Mbit/s) | CPUCLI s NEW / FDT / FDH | µs CPU per datagram NEW / FDT / FDH | CPUSRV s NEW |
|---|---|---|---|---|
| `c1s-400` | 510.4 / 534.1 / 510.0 | 5.73 / 7.00 (+22 %) / 7.52 (+31 %) | 16.8 / 20.6 / 22.1 | 8.26 |
| `c1d-400` | 424.4 / **501.3 (+18 %)** / 476.8 (+12 %) | 11.19 / 9.30 (−17 %) / 10.76 | 32.2 / 26.5 / 30.8 | 12.02 |
| `c2-100` | 89.1 / 89.0 / 89.2 | 3.23 / 3.41 / 3.83 (+19 %) | 35.2 / 36.7 / 41.0 | 4.83 |
| `c8-100` | 101.6 / 102.3 / 103.0 | 5.35 / 4.96 / 5.68 | 56.6 / 50.4 / 60.4 | 6.04 |

What fired: FDT the RTprop-floor clause (c1d leg p1 +5.6 % against +5 %;
c2 +11 %: the task drainer runs only at the sender's yield) and CPU at c1s;
FDH CPU at c1s and c2, one TAIL excursion (c1s smoothed RTT 133 ms) and one
post-close control `drop`. Sender busy NEW / FDT / FDH: c1s 35 / 56 / 52 %,
c1d 96 / 50 / 59 %. Off-CPU capture at c1d: sender wall asleep in quinn
0.226 / 0.016 / 0.057; the drainer carries it (FDT 0.085, FDH 0.183 of its
wall); idle-worker sleeps 16.1 / 19.6 / 38.8 k/s.

**What the evidence says, for the redesign.** (1) At c1d the sender's wall
is the handoff into quinn's per-connection mutex (held across AES-GCM and
`sendmsg`); taking it off the sender is worth +18 % goodput at −17 % CPU.
(2) Every feeder arm moved that wait rather than removing it, and paid for
the move with +22–31 % CPU on single-path cells where goodput is pinned at
the ≈ 500 Mbit/s non-sender ceiling. (3) The task/thread A/B is an A/B of
tokio's wake placement: a wake from a worker lands in that worker's LIFO
slot (serialized, batched, deferred to the sender's yield), a wake from a
non-worker goes through the inject queue + an unpark (parallel, colliding
on the mutex, doubling worker park/unpark churn). Hence the plan: owned
paths (one I/O worker owns a path's endpoint and connection, so the mutex is
uncontended), batched rings with a parked-flag wake, and a logic actor that
owns the scheduler — measured first by P0 (§9).
