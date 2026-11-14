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
`RWM_GEN_PIPE` (only reached when generation is on),
`RWM_RECOV_MP`, `RWM_RECOV_MP_LAW`, `RWM_SUM_CAP`, `RWM_DELTA_CAP`,
`RWM_ACK_MERGE`, `RWM_WIRE_COMPACT`, `RWM_RSTAR_TAIL` (r* provisions the
window loss-mass quantile, ADR-0063; `=0` is GE-only), `RWM_EST_CADENCE` (the loss
estimator's BOCD update batched: clean evidence accumulates and flushes every
10 ms, a loss-bearing ack flushes at once; flipped in `83462ae` on Stage 3
(d), §5; own echo, ACTIVE or OFF, not on `[GATES]`), `RWM_EMIT_BATCH`
(pacer-quantum emission batching, burst bound `RWM_EMIT_BURST` = 64 at every
path count, Law 0; flipped in `bf3a636` on the §8 re-run; on `[GATES]` as
`RWM_EMIT_BATCH=1` plus its own echo: `emission batching ACTIVE`, or `OFF`
at `=0`, or `out of scope` where the policy excludes it: ρ < 1, Realtime
packing, a coded wire). `RWM_PLAIN_RS`, `RWM_RS_ATTR`, `RWM_HONEST_K`,
`RWM_HONEST_CAP`, `RWM_POOL_ANCHOR`, `RWM_REPAIR_WAIT`, `RWM_PROACTIVE_PACER`,
`RWM_REASM_BDP` and `RWM_COMPOSED_CAP` are removed (§49), and `RWM_PLACE_HOL`,
`RWM_PLACE_T_DERIVED`, `RWM_HOLDDOWN_Q`, `RWM_REFRESH_FLOOR_US` and
`RWM_DERIVED_SWEEP` are removed (§50); tags `archive/RWM_*`; set in the
environment they are ignored.

Placement prices every leg on its wire RTT (ADR-0073, promoted from
`RWM_PLACE_WIRE` in `2d625fa`, merged to main in `85c2295`); there is no gate.

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
(paper §10). `RWM_RECOV_SP` is kept as an off arm. Store boot 128 and the
per-path pool 2048 are shipped store constants (§3.4). The store gain 2.0 is a
gate default, not a shipped cap constant: the default cap multiplies by
1 + q(δ) (ADR-0072), and the gain reaches only the `RWM_STORE_PATHS=0` control
and other arms (paper §11.2). *Corrected 2026-11-10.*

## 2. Most recent verdicts

| measurement | design | verdict |
|---|---|---|
| Simplify batches 1b–3 (§50) | `simplify-5` (Group C, `RWM_PLACE_HOL`, `RWM_PLACE_T_DERIVED` archived; −2 921 lines) vs main, suites + §41c guard, 12 + 12 | `GATE-41c PASS`, p = 0.172, 96/96 LIVE; **merged**. Closest clauses c1s `lag_E_cli` +15 % (p 0.009) and `rtp_floor_p0` +2.2 % (p 0.013), joint p 0.17; c1s client lag carried to §52 as a watch item |
| `bbr_rs` as the substrate CC (§48b) | CAND `bbr48cand` (`RWM_QUIC_CC=bbr_rs`) vs BASE main `0a1ddb88`, c1s/c1d at 5 GB, c2/c8 at 600 MB, n = 12 + 12, §41c guard + p_rest, R2b collapse witness; rehearsal first | running (rehearsal `REHEARSAL-PASS`, both c8 smoke rows LIVE). Registered prediction `CONTROLLED-WITH-COST`; any label goes to the user |
| Simplify batch 1 (§49) | `simplify-2` (nine default-off arms deleted, −2 665 lines of `raptorpath/src`) vs main `b3000701`, suites + §41c guard, 12 + 12 at c1s/c1d/c2/c8 | `GATE-41c PASS`, p = 0.957, 96/96 LIVE; **merged** (`0a1ddb88`). One CAND c1s row collapsed (148.5 Mbit/s): §41b's quinn window collapse, reported only. Five earlier launches died on harness paths, each fixed with a test |
| Merge candidate `diag36` (§41c + §46) | suites + §41c guard (rank-minp, 12 + 12) on CAND `fb67a6c2`; §46 path event on `dffd8517` (same engine) | guard `PASS`, p = 0.226; §46 `NO-DIFFERENCE` (R and post-event goodput WITHIN in both variants); **merged** (`85c2295`) |
| Fixed `bbr_rs` host gate (§48) | `cc_sim` red/green tests of quinn's BBR vs the fixed `bbr_rs`, 50 µs registered grid, plus a 10 µs grid | `STOP-NOT-REPRODUCED` as registered (50 µs misdrives quinn's sampler); on 10 µs quinn collapses (share 0.81) and `bbr_rs` holds the pipe at every phase, post hoc |
| Quinn BBR window collapse at c1s (§47) | diagnosis from §41b / §43 ledgers and quinn's source; no VM | the collapse is a latched-low `min_rtt` (W = 2·b̂·m̂); quinn's startup growth is unbounded, so the store is the only brake; led to §48 |
| Oracle store law, re-tuned (§43b) | the §43 oracle at G = 0.9, τ = 5 ms, c2, SL/SH starts, n = 3 each | c2 `CONVERGES-ONE-POINT`: both starts in band (191 / 187), Tp50 18–19.6 ms vs main ≈ 208 ms, goodput ≈ 2 % lower; diagnostic only, nothing ships |
| Oracle store law in closed loop (§43) | `RWM_STORE_LAW=oracle` (L from netem, Copa's 10 s RTprop, app-echo S), c2-100 at 600 MB and c1s-400 at 5 GB, SL/SH/MAIN, 24 rows | c2 `BISTABLE`, c1s `UNSTABLE` (the c1s runaway is quinn's window collapse); diagnostic only, nothing ships |
| `RWM_PLACE_WIRE` gate-off guard (§41b) | A/A-calibrated: BASE n = 12, MAIN n = 6, WIRE n = 6 reported, c1s/c1d | `GATE PASS`, p = 0.72; the user then promoted the wire price (ADR-0073), gate deleted |
| `RWM_PLACE_WIRE` iteration (§41) | allocation-free witness, host gate, then the §38 battery against main's binary | ladder `CURED` (WIRE vs MAIN); BASE gate `FAIL` on one c1d client-lag clause on the better side (the gate was not A/A-sized), so §41b |
| p90 dispersion estimator (§40) | the per-window p90 of k = 4 train rates fixed in advance, plus aggregation (netem `slot`) and policer (`tbf`) cells; capture diagnostic, no engine diff | `NOT-YET`, failed on one leg: c8's slow leg (E3 fails at cap 32, E4 unscoreable at 256); E1–E4 pass at c1s, c2 and c8's fast leg; `AGG-OVER-READ` ×2.05 (`c2slot`); `c2tbf` does not over-read, but the policer is not cleared (caps 32–128 not run); nothing ships |
| DoF ledger, the rate law's feedback leg (§39) | FlEC/AC-RLNC import derived on paper and bounded from the r > 0 battery's `[RFA]` counters; no VM | `CONTINUOUS-BUT-CANNOT-HELP-MEASURABLY`; no battery pre-registered. Addendum: one seed, n = 4, so the gain is "not shown", not refuted; funded r on one saturated path stays capped at parity with ARQ |
| Model review (§37) | joint fixed-point derivation, internal audit and 2021–2026 literature scan, reviewed with the advisor; no VM | record, no scored verdict: the architecture holds, the paper as written does not; one verified defect, the idle-leg stale-price lock-out (→ §38); no code changed |
| Idle-leg lock-out, `RWM_PLACE_WIRE` (§38) | the wire-clocked placement base (quinn's smoothed RTT in place of the app-echo srtt) vs MAIN, same binary, default cells plus static caps, n = 6 per arm, 96 rows | `REGRESSES-AT(c1s-400@def)`: client CPU/GB +2.5 % against a 2.4 % tolerance, ranges disjoint; every other clause passes (without it, `CURED`); no WIRE row locked, against 10 of 24 MAIN rows at the static cells; no benefit at the default law; start-up trigger shrink "not shown"; arm does not ship; iteration in §41 |
| Receiver-side dispersion as the link-rate source (§36) | capture diagnostic of the k-train estimators (k = 2, 4, 8, 16) at c1s, c2, c8; 58 rows, no engine diff | `NOT-YET`: at k = 4, D1 `CIRCULAR` and D2 `NOISY` on c1s, c2 and both c8 legs; no k passes either clause. Addendum (post hoc): the p90 reads the link from a cap crossover up; a pass needs a fresh registration (→ §40) |
| Receiver follow-ups of §31/§32 (§35) | five items: dead-peer fail-fast, `[LAT]` exactly-once with declared late fills, legacy-decoder prune by removal, one-prune size, `recv_alloc_bound` spread; tests plus one diagnostic | items 1–4 delivered as tests, ships nothing by itself (the client fails fast on a dead peer, `peer lost` within the idle timeout); item 5 `H-leak REFUTED`: the spread is the run's own speed, so the gate reads the min of 5 pairs |
| Store law's missing input after the receiver change (§34) | derivation from the committed ledgers, then a static-cap diagnostic (R1–R4) at c1s and c1d, post-§32 main | derivation `NONE-CLOSES`. Diagnostic: R1 knee 1 024 at both; R2 `CLOSED` (D-W not reopened); R3 at c1d `AMBIGUOUS`, fixed point `MISSES` (457, band [683, 1 536]); R4 `QUINN-SHARES`; no branch fires, the input stays open |
| c8 lag tail's timer hand-off on current main (§33) | offline reading of the committed §28 step-0 and §26 ledgers; no VM | `NOT MATERIAL`: no fix designed, nothing pre-registered, nothing ships |
| Decoder prune per drain (§32) | `NEW` (prune per drain) vs MAIN `b30044b`, ship battery | `SHIP`: PASS-EVERYWHERE; BETTER in goodput and server CPU/GB at c1s and c1d, WITHIN or better elsewhere; shipping is the operator's merge |
| Debug-suite 300 s stall (§31) | BASE `d6bf4e7` vs FIX `17824f7`, stall-rate check | `STALL-FIXED`: BASE stalled 15 of 20 iterations (all with the `[LAT]` floor panic), FIX 0 of 40; the fix is a correctness fix in release builds too |
| Noise-sized window for the power-seeking law (§30) | derivation from the existing ledgers (D-W); no VM | `DOES-NOT-CLOSE` |
| Power-seeking store law, candidate D (§29) | D against MAIN and the static knee, 4 cells, 36 rows | `NOT-VIABLE`: `STABILITY` `NOISE-DOMINATED` at all four cells, `GOODPUT` `WORSE` at c2 and c1s, `FIXED-POINT` `IN-BAND` only at c8; latency columns as promised (A_x p50 ×5–12 better) |
| Incremental SACK marking (§28) | step 0: SK vs the §26 sender; step 1: `NEW` vs MAIN `92934a7`, ship battery | step 0: per-wake bound `DROPPED` (NEW = SK, no poll ≥ 10 ms). Step 1: `SHIP`, PASS-EVERYWHERE; c8 client CPU/GB −30.1 % (disjoint ranges), c8 goodput TREND-WORSE (−5.7 %, overlapping); shipping is the operator's merge |
| Spread law's two premises (§27) | `[LOOPDIAG]` instrument on main, 4 cells, 128 rows | `PREMISES-FAIL`: bbr_rs reads the delivered rate, not the link (Q1 never `FLAT-AT-LINK`; `CIRCULAR` at c2); the spread inflates with the cap except at c2 (Q2); Q3 misses everywhere |
| c8 lag tail and the sender's long polls (§26) | MAIN `37dfd2f8…`, c8, 24 + 24 rows | (a) `MIXED` (an OS part and a larger runtime part); (b) `UNBOUNDED-PER-WAKE(sack)`: 81 % of the wall of polls ≥ 10 ms is the SACK drain; bound designed in §28 |
| c8 client `[LAG]` spread (§25) | diagnostic, 30 rows, placement step 1 | `UNSETTLED`: long task polls cover a third of late ticks (F = 0.33 < 0.5), no steal at the late ticks; placement ruled out; nothing ships |
| Which input can the store law read (§24) | the min-filtered loop time and quinn's BBR rate (options 1 and 3) at `c2-100` and `c1s-400`, 46 rows | no candidate passes: option 1 (min-filtered loop time) `INFLATES`; option 3 (quinn's rate) `OVER-READ` ×7.6 at c2 and `NEITHER` at c1s; its ask `MISSES` the knee band |
| c1 anchor over-read and the store law's input (§23) | diagnostic at the four cells | Q1 `MECHANISM-CONFIRMED` at c1s and c2; Q2 `HONEST` by its rule, but the send-interval anchor under-reads ×0.1–0.5; Q3 `HEADROOM-MISSING` at both single-path cells; no input change built |
| Threading R3 step 0, c1d receiver re-profile (§22) | R3 design gated on the H-alloc (a) profile witness; three attempts (the third, amendment 5, last) | `UNREAD` on the deciding witness (attach failed; then no call chain, no DWARF; then `ABORT-TEXT`); the rule selects no design, R3 stays shelved at `archive/thread-r3`, §22 closes |
| Receiver per-datagram cost with the `[LAT]` floor fix (R2, §21) | R2 (`73d868d`) vs MAIN 4e50504, c1s/c1d/c2/c8, n = 6, 48 rows | `DELIVERED`: PASS at every cell; the §17 c1s collapse is gone (687–746 Mbit/s); ships |
| R1 c1s collapse (§20) | five arms (`MAIN`, `R1`, `R1-d`, `MAIN+d`, `FIX`), 10 rows, 3 profiles | `H1-CONFIRMED` + `FIX-VALIDATED`: the collapse is R1's `[LAT]` pending store (`net/lat.rs`); swapping that one file decides it both ways |
| Same-session control for §18 (§19) | MAIN `ce60079` vs LAW (ADR-0072), c1s/c1d/c2/c8, one session, 54 invocations | `SHIPS`: no `REFUTED-NOOP` at the N ≥ 2 no-op cells (c1d, c8); c1s `PINNED`, c2 `INERT-AS-DERIVED`; §18's c1d `CONTROL-MOVED` is the cross-session reference (§18's verdict is not relabelled) |
| One store law at every path count (§18) | LAW (ADR-0072) vs the control, `c1s-400`, `c1d-400`, `c2-100`, 42 invocations | `REFUTED` through `CONTROL-MOVED` at c1d and c2 (not through the c1s contrast, which reads `PINNED`); attributed to the cross-session reference in §19; nothing ships or flips |
| Receiver per-datagram cost, R1 (§17) | R1 (`583b712`) vs MAIN, c1s/c1d/c2/c8, n = 6, 48 rows | `REFUTED-WITH-RECORD (R1 FAIL-AT-c1s-400)`: c1s goodput −69.6 %, CPUSRV/GB +149 % (WORSE); PASS at c1d, c2, c8; all four predictions MISSED; nothing ships |
| Threading Q3 investigation (§16) | step 1: per-task profile of the sender and receiver at `c1s-400`/`c1d-400`, n = 3, single seed, not scored | no lock-free send is built: neither pre-registered critical-path condition holds (the in-lock `sendmsg` wait is absorbed; §13's `own` arm gained nothing); the c1d ceiling is the server's receiver task |
| D9 attribution (§15) | `MAIN` 88476a4 vs `NOD9` (88476a4 minus the D9 hunk, `measure/nod9` e059e30), c1s/c1d/c2/c8, n = 6 (c8 n = 12), `RWM_RTOBS=1`; plus the c8 client `[LAG]` p99 re-check | `KEEP-D9 (MAIN better)`: NOD9 FAIL at c1s (goodput −11.4 %, CPUCLI/GB +4.9 %, both WORSE), WITHIN at c1d/c2/c8, BETTER nowhere; D9 stays; c8 client `[LAG]` p99 MAIN 2708 [1248–3636] n = 12, MAIN vs NOD9 WITHIN, cross-session numbers report-only |
| Threading Q2 (§14) | the scheduler split by direction (the sender owns the TX half and the FEC controller by `&mut`, acks handled in the sender via the owners' input channel; the receiver owns the RX half; `AckWake` and `SchedMutex` deleted; the `own` arm deleted) vs MAIN 0ef0e0d, c1s/c1d/c2/c8, n = 6, `RWM_RTOBS=1` | `DELIVERED`: PASS at every cell — goodput WITHIN everywhere, CPUCLI/GB −2 to −5 % (c1s TREND-BETTER), CPUSRV WITHIN, RTprop WITHIN; `[LAG]` p99 TREND-BETTER at c2 both sides and c1s server, TREND-WORSE at c8 client (+62 %, overlapping); amendment 1's c1d client-lag risk did not fire (−9.8 %); c2 CPU recovery of P1's +5.3 % MISSED (−2.2 %); client sender busy fell (c1d 59 → 56 %); merge is the operator's |
| Threading Q1 (§13) | the per-path I/O owner, placements IOS (`RWM_IO_RT=shared`) and IOO (`own`), and D9 alone, each vs MAIN 69fd846, c1s/c1d/c2/c8, n = 6, `RWM_RTOBS=1` everywhere | `DELIVERED (SHIP-SHARED)`: IOS passes everywhere — c1s goodput +48 %, c1d +67 % (729.5 Mbit/s, FDT bar 501 met), CPUSRV/GB −8 to −30 % at every cell, CPUCLI/GB −13 to −37 % at c1d/c2; IOO FAIL at c1d (client `[LAG]` p99 +22 %, RTprop floor +12/+15 %); routing predictions MET (`drv_off` 0 under own, 34–63 % under shared), own lock wait ≈ 0 MISSED at c1s (0.0115 > 0.01); D9 alone FAIL at c1s/c1d/c2 (c1s −32 %, CPUCLI +103 %: §12's c1s collapse re-scoped to D9); merge and `own` deletion are the operator's |
| Threading P2a (§12) | P2A (the logic actor: one task owns the scheduler and the FEC controller; acks as batches; perf on a worker; `RWM_RTOBS` opt-in) vs MAIN e655484, c1s/c1d/c2/c8, n = 6, both arms `RWM_RTOBS=1` | `REFUTED-WITH-RECORD (WORSE-AT-c1s-400)`: c1s goodput −33 %, CPUCLI/GB +87 %, CPUSRV/GB +55 % (disjoint); c1d/c2/c8 SAME (c1d goodput and c2 client CPU TREND-WORSE); RTprop floor and `[LAG]` p99 WITHIN everywhere; the sender loop iterates ≈ 3× (up to 11×) more at c1s (finding 1; the receiver's per-message budget unit, 06837b7, is a named confound); not shipped |
| Threading redesign P0 (§9) | P0 (named runtime + `[THR]`/`[LAG]`) vs MAIN 8d7d8c1, `c1s-400`/`c1d-400`, n = 3, `RWM_RDIAG=1`, 12 invocations | D3 `D3-REFUTED-WITH-RECORD` at c1s (server receiver task 79 % busy, hottest server thread 0.36 core; stop rule not fired; at c1d the receiver task reads 92 %); no-behaviour-change `REFUTED-WITH-RECORD` (c1s goodput −6.7 %, disjoint ranges at n = 3; c1d within); per-thread budget recorded; nothing flipped |
| Threading P1 (§11) | P1 (ack wake, no quinn call under the scheduler lock, DashMap guards dropped, persistent tail timer, lock-free counters) vs MAIN 8d7d8c1, c1s/c1d/c2/c8, n = 6 | `DELIVERED (SAME everywhere)`; ack wake holds (timer won ≤ 0.16 % of acked pauses); c1s sender-busy prediction MISSED (36.5 → 36.8 %); c2 client CPU +5.3 % within MDE; c8 fast-leg RTprop TREND-WORSE (ranges touch); shipped in c292268 |
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
| `reliable` boolean branches in the window sender/receiver | ρ = 1 vs ρ < 1 selecting code paths instead of composing with δ; since `bf3a636` this includes emission batching (`emit_batch_on` requires `reliable`); at the store-cap seat ρ < 1 has no outstanding gate at all (`tx_paused` and `plain_dyn_cap` require `reliable`, no `sent_store`; how ρ would enter the law: paper §6.1, status §23) |

| `symbol_size_for_hint` | 512-byte symbols at Realtime vs 1 200 bytes otherwise; every limit counted in symbols halves in bytes at Realtime. Found by the §51 design (`s51-design`, `docs/derivations/s51-design.md`); not in the law ledger yet |
| the CLI's ρ | ρ follows the hint: `--window-reliable` can only set ρ = 1, so ρ < 1 is reachable only at Realtime. ρ is not an independent dial on the engine CLI (§51 design) |

**The engine's δ knob moves only half the machine** (§51 design, 2026-11-13). `RWM_DELTA` sets any δ and is echoed in `[GATES]`, but the five hint-keyed rows above (the symbol size, the duplicate send, packing/batching, `queue_target_mult` and the default ρ) stay keyed on the hint string. §51's T2 test (each named hint against the same δ as a number) is designed to measure the resulting step. It predicts a FAIL at Realtime in both c2 and c8.

### 3.4 Open-constants register

All unprovenanced and uncorrected (correct value unknown). Source: paper §11.2.

| constant | value | where | why unprovenanced |
|---|---|---|---|
| tail-sweep / refresh clamp | `(2·srtt).clamp(25, 100) ms` | `net/recovery_laws.rs` `hole_nack_refresh` | undefeated, not derived; derivable from true-heal F only at duals |
| legacy age gate | `srtt/2` | `net/recovery_laws.rs` `legacy_age_ripe` | pre-RFC; compares flight age with a dwell-inclusive srtt; inert at c1 |
| RECOV_MP thresholds | `9/8·max(srtt, ewma)`, 3 packets | `net/recovery_laws.rs` `mp_time_threshold_split` | RFC 9002 values applied to an age that includes sender dwell |
| `GAP_ACK_MIN_INTERVAL` | 2 ms | `net/mod.rs` | a rate limit that is also the hole sampler; not derived |
| `NACK_RETX_COOLDOWN_FLOOR_US` | 10 ms | `net/mod.rs` | 10× kGranularity by choice; moves W by 6.8× across its range |
| `ELIGIBLE_SKEW` | — | removed with the block pipeline (`dacfd7c`, 2026-10-04); no longer in `src` | *(row stale; corrected 2026-11-10)* |
| taper copy `p_lost` | model-derived | `net/emit_source.rs` | measured waste is zero; why it never fires is undecided |
| store gain / recovery round | 2.0 / 100 ms | `gates.rs`, `net/store_cap.rs` `HONEST_RECOVERY_ROUND_S` | the gain no longer sets the default cap (1 + q(δ), ADR-0072; paper §11.2), only the `=0` control and off arms; κ is a fit *(corrected 2026-11-10)* |
| heal/closed classifier | `srtt/2` | D0 instrument (offline) | biases π0 upward; gates nothing; the engine's `[LATE]` π̂0 uses sender-stamp order instead |
| `PLACE_TEMPERATURE` | 0.15 | `scheduler/place.rs` | argmax of a four-point sweep whose verdict failed |
| `w_div` | 1.0 | `scheduler/mod.rs` `SchedulingWeights::from_hint` | derived GE form gives 0.475 (c2) / 0.552 (c3) |
| unmeasured-path price | 10.0 | `scheduler/place.rs` `place_costs` | a hard exclusion (e^−66 odds) in a continuous costume |
| near-tie band / floor | 0.8 / 0.25 | `place_repair_spare_path` | a relative band plus an absolute floor: a missing scale |
| stall fraction κ in placement | 1 | `place_costs` | declared upper bound; measured 0.005–0.067 |
| `BULK_TAIL_BUDGET` | 0.05 | `raptorpath-math` | an "e.g." in the derivation promoted to a const |
| `queue_target_mult` | 1.08 / 1.125 / 1.25 | `scheduler/copa.rs` | declared corner; no continuous form fits the three points |
| block shape by hint | — | removed with the block pipeline (`dacfd7c`); `BlockProfile::from_hint` is no longer in `src` | *(row stale; corrected 2026-11-10)* |
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

### 3.10 Ledger verdicts, 2026-11-13 (wasm mode-switch audit, §52 post-hoc)

Defect findings under FORMULA-FIRST and MEASUREMENT DISCIPLINE 17/18. Each one is a verdict, not a footnote. None is fixed yet, and every fix waits for an owner call or a derivation.

1. **The T_cut give-up law is PINNED (model only).**
   - `raptorpath-math::find_t_cut` is read through `max(t_cut, 2·RTT)` in `raptorpath-wasm` :1401.
   - At all ten measured ρ < 1 points it is either the 2·RTT floor or the 10·W search cap, so the law is two-valued over its operating range.
   - It steps at non-preset ρ* 0.983332 and 0.988175.
   - No engine caller.
   - Evidence: `docs/derivations/wasm-modeswitch-audit.md` (F4), branch `wasm-modeswitch-audit`.
2. **P_fec truncation: a code-vs-paper divergence, degenerate over the engine's measured range.**
   - `raptorpath-math::p_fec_normal` returns 0 when r(1−ε) − ε ≤ 0, and Φ(0) = ½ just above that point. That is a 0 → ½ step at r = ε/(1−ε).
   - Paper §4.2 (`fec-arq-model.md:699`) states the continuous Φ form. The cut has been in the code since `da837ee9`.
   - **Engine reach, on two default paths:**
     - the shed budget, `net/emit_source.rs:588` (`shed_budget_frac`, only when `shed_on`, i.e. ρ < 1);
     - the rate law's soft cap: `fec_rate.rs:161/:164` call `raptorpath_math::controller_rate`, which applies `soft_saturate(rate, r_saturation(...))` (math :825), and the engine sets `saturation_cap` by default.
   - **Whether the engine has run on the P_fec > 0 side is unsettled.** The plan records r ≈ 0 on the L1 cells, but the fixture comment at `fec_rate/tests.rs:277` describes a consumed r ≈ 0.34 at ε ≈ 4.8 %, above the crossing.
   - The crossing r is exactly ε/(1−ε): 0.0267 at c2 and 0.0515 at c3. The crossing δ is a model estimate between the Bulk and Auto presets, never at Bulk. §51 must read r per cell and settle this.
   - Conflict for the owner: the paper's form gives P_fec(0) > 0, against the engine pin `fec_rate/tests.rs:262–282`.
3. **The wasm model carries a dead second machine.**
   - The classic per-hint constructor arms (W2, W4, W5, W6, W9) and V1 are reachable only from tests and the golden capture. The UI passes one hint, `'continuum'`, at every dial position (gate: `test_visualizer_ui.mjs`).
   - Their outputs equal the continuum at the presets on all six golden cells. ρ is dropped for auto and realtime on that path.
   - Fix: removal or `cfg(test)` (owner calls are listed in the audit).
4. **`[QCLK] w_us` sits at the 100 ms clamp.** At c8 under CAND, p05 = p95 = 100 000 µs, while pooled srtt averages ≈ 443 ms (`docs/l1-raw/s52/`). This is a clamp that always binds. The emitting law is `tail_sweep_timeout_us` = clamp(2·pooled srtt, 25 ms, 100 ms) (`net/recovery_laws.rs`; identified by `dup-diag`): pooled srtt 258–468 ms in all 12 c8 rows puts 2·srtt far above the upper bound. With 3–8 timer fires per run, it is not the duplicate source.
5. **The c8 hole law was never measured in its operating regime before §52.**
   - Under the 20-gap horizon, p0 holes reached the law at ≈ 3.5 s age.
   - Under lever B the packet-threshold channel fires 3.7–6.6 k times per run (BASE 0–755).
   - ~~the original lands about 5 ms later on a FIFO leg~~ (withdrawn, `dup-diag`): `hy_p50` (5.1–5.6 ms p0, 2.8 ms p1) is the next fresh gap report at the ~2 ms ack cadence, not a flight time. The original had already been reported delivered when the channel fired.
   - Main already wastes about 12 % of its retransmits.
   - Diagnosis: `docs/derivations/dup-diag.md` (branch `dup-diag`), summarised under §52 Erratum; the measurement goes in §53.
6. **The receiver's `hole_nack_refresh` clock reads a degenerate input (`dup-diag`).** `[QCLK]` receiver shows srtt = exactly 50 000 µs with sigma n = 0 in all 12 c8 rows, so the refresh cadence is a constant 100 ms. The law's input never takes a measured value; the source of the constant is not yet traced.

7. **The guard clause `rtp_floor_p0` measures the end-of-run drain, not the path** (§53 Erratum 1). At 600 MB the minimum is the end-of-run drain on 20-gap rows and the start-up sample on lever-B rows, so a tail fix reads worse there. At guard size (§52 guard c8 rows, ≈ 9 s), both arms mix the two: the minimum is the start-up sample on 6/12 BASE and 5/12 CAND rows, and the end-of-run sample (0.0–1.1 s after source end) on the rest. So the clause is a minimum over two unrelated moments, not a path reading, at either size. Dry run done (`guard-clause-dry`): the source-phase floor is degenerate on guard rows (the same start-up sample); in-order delay resolves only on long rows. Proposal: §41d (guard v2, PROPOSED).
8. **The store cap is a constant at c8, on main too.** The pooled cap sits at its clamp N·pool = 4 096 for 0.74–0.98 of each run (`[SUMCAP] pin=`, §53, all five arms). The law under the clamp asks 12–43 k, because both inputs are over-read: `min_rtt` reads 7–33× the propagation delay (item 9), and `max_bw` 11–24× the link rate (not yet diagnosed). Over the c8 operating range the law's shape is hidden by its clamp.
9. **Copa `min_rtt` reads the sender's own standing queue.** It is a 10 s minimum of the app-echo RTT (`RWM_COPA_WIRE` off), which includes quinn send-buffer dwell. On a 10 ms path it reads 95–129 ms on main and 163–319 ms under lever B, from t ≈ 10.3 s (when the start-up sample leaves the window) on 30 of 30 c8 rows. Consumers: the store cap (`max_bw × min_rtt`, a positive loop cut by the clamp) and Copa's cwnd anchor floor (gates nothing here, since the in-flight brake is off). Evidence: `docs/derivations/p0-dwell-diag.md`.

### 3.9 Harness rule: report the lock-out classifier (2026-11-10)

Every multi-leg static-cap battery reports the §38 lock-out classifier per
row: LOCKED = a leg with egress share < 10 % and median post-warm-up
(t ≥ 1 s) `rtt` ≥ 3 × median `wrtt` (`tools/l1/place38_onset.py`). A LOCKED
row is flagged in the table, not averaged in silently. Reason: 27 of 110
static-arm rows were locked (c1d 10 of 59, c8 17 of 51; §38 amendment 2),
and an unflagged lock-out reads as a cap effect.

## Index of closed sections

Each closed section lives in its own file under `docs/status/`; every `§N` cite resolves here. Split from `docs/status.md` at 2d3b2069 (`git log --follow` does not track the split). New pre-registrations get their own file.

- [§4. Block default re-test — pre-registration](status/04-block-default-re-test.md)
- [§5. Stage-3 baseline — pre-registration](status/05-stage-3-baseline.md)
- [§6. V4 verification — pre-registration](status/06-v4-verification.md)
- [§7. Receive-buffer fix (F-A) — pre-registration](status/07-receive-buffer-fix-f-a.md)
- [§8. Emission-batching scope (Law 0) — pre-registration](status/08-emission-batching-scope-law-0.md)
- [§9. Threading redesign P0 — per-thread core budget — pre-registration](status/09-threading-redesign-p0.md)
- [§10. The per-path datagram feeder — evidence record (code not merged; superseded by the threading redesign)](status/10-the-per-path-datagram-feeder.md)
- [§11. Threading P1 — topology-independent defect fixes — pre-registration](status/11-threading-p1.md)
- [§12. Threading P2a — the logic actor — pre-registration](status/12-threading-p2a.md)
- [§13. Threading Q1 — the per-path I/O owner — pre-registration](status/13-threading-q1.md)
- [§14. Threading Q2 — the scheduler split by direction — pre-registration](status/14-threading-q2.md)
- [§15. D9 attribution and the c8 lag re-check — pre-registration](status/15-d9-attribution-and-the-c8-lag-re.md)
- [§16. Threading Q3 — the investigation (step 1) — pre-registration](status/16-threading-q3.md)
- [§17. Receiver per-datagram cost (R1) — pre-registration](status/17-receiver-per-datagram-cost-r1.md)
- [§18. One store law at every path count (ADR-0072) — pre-registration](status/18-one-store-law-at-every-path-count.md)
- [§19. Same-session control for §18 — pre-registration](status/19-same-session-control-for-18.md)
- [§20. R1 c1s collapse — diagnosis — pre-registration](status/20-r1-c1s-collapse.md)
- [§21. Receiver per-datagram cost, R1 with the [LAT] floor fix (R2) — pre-registration](status/21-receiver-per-datagram-cost-r1-with-the.md)
- [§22. Threading R3 step 0 — the c1d receiver re-profile — pre-registration](status/22-threading-r3-step-0.md)
- [§23. The c1 anchor over-read and the store law's input — diagnosis — pre-registration](status/23-the-c1-anchor-over-read-and-the.md)
- [§24. Which input can the store law read? — loop time, quinn's rate, jitter — diagnosis — pre-registration](status/24-which-input-can-the-store-law-read.md)
- [§25. The c8 client `[LAG]` spread — diagnosis — pre-registration](status/25-the-c8-client-lag-spread.md)
- [§26. The c8 lag tail and the sender's long polls — the two deciding measurements — pre-registration](status/26-the-c8-lag-tail-and-the-sender.md)
- [§27. The spread law's two premises — the L source and a cap-flat spread — diagnosis — pre-registration](status/27-the-spread-law-s-two-premises.md)
- [§28. Incremental SACK marking — ship battery (with a step-0 check of the per-wake bound) — pre-registration](status/28-incremental-sack-marking.md)
- [§29. The power-seeking store law (candidate D) against main and the static knee — diagnosis — pre-registration](status/29-the-power-seeking-store-law-candidate-d.md)
- [§30. A noise-sized measurement window for the power-seeking law (D) — derivation, no VM — verdict](status/30-a-noise-sized-measurement-window-for-the.md)
- [§31. The debug-suite 300 s transfer stall — decoder re-delivery below the prune floor — stall-rate check — pre-registration](status/31-the-debug-suite-300-s-transfer-stall.md)
- [§32. The decoder prune per drain (the measured R3 subset) — ship battery — pre-registration](status/32-the-decoder-prune-per-drain-the-measured.md)
- [§33. The c8 lag tail's timer hand-off component on current main — offline reading — result](status/33-the-c8-lag-tail-s-timer-hand.md)
- [§34. The store law's missing input after the receiver change — derivation, no VM — verdict, and a diagnostic pre-registration (not run)](status/34-the-store-law-s-missing-input-after.md)
- [§35. Receiver-side follow-ups of §31/§32 — the `recv_alloc_bound` spread (diagnostic) and the test sessions — pre-registration](status/35-receiver-side-follow-ups-of-31-32.md)
- [§36. Receiver-side dispersion as the store law's link-rate source — capture diagnostic — pre-registration](status/36-receiver-side-dispersion-as-the-store-law.md)
- [§37. Model review — joint fixed point, internal audit, literature — no VM, record](status/37-model-review.md)
- [§38. The idle-leg lock-out — the wire-clocked placement base (`RWM_PLACE_WIRE`) — pre-registration](status/38-the-idle-leg-lock-out.md)
- [§39. The DoF ledger, the rate law's feedback leg — derivation, no VM — verdict](status/39-the-dof-ledger-the-rate-law-s.md)
- [§40. The p90 dispersion estimator, fixed in advance, with aggregation and policer cells — capture diagnostic — pre-registration](status/40-the-p90-dispersion-estimator-fixed-in-advance.md)
- [§41. `RWM_PLACE_WIRE`, iteration once — an allocation-free witness — host gate, then battery — pre-registration](status/41-rwm-place-wire-iteration-once.md)
- [§41b. `RWM_PLACE_WIRE` gate-off against main — an A/A-calibrated guard — pre-registration](status/41b-rwm-place-wire-gate-off-against-main.md)
- [§42. The engine path for dispersion, and the store law's time term — derivation and feasibility, no VM — record](status/42-the-engine-path-for-dispersion-and-the.md)
- [§43. The oracle store law in closed loop: one operating point or two? — diagnostic arm `RWM_STORE_LAW=oracle` at c2 and c1s — pre-registration](status/43-the-oracle-store-law-in-closed-loop.md)
- [§44. Is the model overcomplicated? — law inventory and simplification review — no VM, record](status/44-is-the-model-overcomplicated.md)
- [§45. Simplification plan — reduce complexity, keep or improve performance — no VM, plan](status/45-simplification-plan.md)
- [§46. The path event — does the promoted placement price bring a leg back? — pre-registration](status/46-the-path-event.md)
- [§47. Quinn's BBR window collapse at c1s — diagnosis, no VM — record](status/47-quinn-s-bbr-window-collapse-at-c1s.md)
- [§48. A working substrate: the fixed `bbr_rs` against quinn's BBR — host gate, then battery — pre-registration](status/48-a-working-substrate-the-fixed-bbr-rs.md)
- [§41c. The merge guard: rank-minp, 12/12 at four cells — pre-registration](status/41c-the-merge-guard-rank-minp-12-12.md)
- [§48b. The `bbr_rs` battery on the VM — pre-registration (before any VM contact; advisor review)](status/48b-the-bbr-rs-battery-on-the-vm.md)
- [§49. Simplify batch 1 (`simplify-2`) against main — the guard session — pre-registration (before any VM contact)](status/49-simplify-batch-1-simplify-2-against-main.md)
- [§50. Simplify batches 1b–3 (`simplify-5`) against main — the guard session — pre-registration (before any VM contact)](status/50-simplify-batches-1b3-simplify-5-against-main.md)

## 52. The c8 recovery levers — gap horizon (scored) and retransmit leg choice (reported) — pre-registration (before any VM contact)

**Why.** The c8 completion tail (≈ 3.9 s on main, ≈ 8 % of a c8 run) is a hole backlog (H0 ≈ 1 890) built in the main phase and drained at ≈ 500/s ("c8 completion tail diagnosis"). Two levers, both post-hoc findings:
- **(B) the sender's gap horizon** is 20 of the 64 ranges a report carries;
- **leg choice:** SACK-gap retransmits are placed 786 : 1 against their original leg by the diversity weight, so c8's fast-leg holes are re-sent on the slow leg and hold their horizon slots for its clock ("Retransmit leg choice at c8").

**Trees.**
- **BASE = `base52`:** main + `wdiv-diag` (`RWM_PLACE_WDIV`, default unset = bit-identical by test; `[HOLD]` `cx_leg=`).
- **CAND = `cand52`:** BASE + `gaphorizon` (`sack_to_gaps` uses `MAX_SACK_RANGES`).
- `git diff BASE CAND -- raptorpath/src raptorpath/tests` = 3 files (`net/mod.rs`, `net/sack.rs`, `net/tests.rs`), lever B alone. Both pinned by amendment.

**Session (`LD_PLAN=52`).**
- **Suites** on CAND's tree.
- **The §41c guard, scored and unchanged:** rank-minp, 12 + 12 at c1s/c1d/c2/c8 default sizes, 96 rows, PASS iff p > 0.05.
- **Then a reported-only block** at c8-100 with 600 MB transfers. Arms, n = 6 each:
  - **BASE**;
  - **CAND**;
  - **W0** (BASE binary, `RWM_PLACE_WDIV=0`: retransmits place like source);
  - **W05** (`RWM_PLACE_WDIV=0.5`);
  - **WDER** (`RWM_PLACE_WDIV_DERIVED=1`).
- **Block layout:** 30 rows, rotated per group, never scored, no replacement. A rehearsal runs first.

**Readouts (reported, `tail52_score.py`, which reproduces §48b's c8 numbers exactly on the committed logs):**
- **Per arm:** goodput, completion tail, H0, frontier lag at source end, `[HOLD]` first-report age p50/p90 and closed-by-retransmit p50/p90 per original path, the `cx_leg` send-leg split, the retransmit send-leg-1 share (`mpr fp/on`), retransmits/GB, receiver duplicates, CPU/GB.
- **The lever-B premise:** K = μ·C per 4 s main-phase bin and N_h (seqs visited per report).
- **Witnesses:** each arm's env in its `[GATES]` echo on both ends; BASE/CAND carry none.
- **WDER's derived branch sits behind `ge.is_valid()`, which `[GATES]` cannot witness.** Its execution witness is therefore an outcome: its retransmit send-leg-1 share must differ from BASE's beyond BASE's own row range; otherwise WDER is read as not executed.
- **Watch item from §50:** c1s `lag_E_cli` (reported beside the guard).

**Predictions (numbers before results).**
- **Guard:** PASS. Lever B changes behaviour only where reports carry more than 20 ranges, i.e. c8 (≈ 63 ranges) and c1d (25–30); c1s and c2 are byte-identical in behaviour.
  - Risk: per-report CPU about × 3 at c8/c1d, below the guard's c8 CPU resolution (d80 8–12 %).
  - Not expected: c8 goodput WORSE; a shorter tail is a gain the one-sided guard does not score.
- **CAND at c8 600 MB, if K scales with the horizon (the premise; K ≈ 63–103 → ≈ 200–330 per bin):** H0 ≈ 100–200, tail ≈ 0.2–0.4 s, first-report age p50 < 1 s. If K does not scale, the tail moves little and the premise is refuted. The 64-range wire cap is the next limit.
- **W0:** send-leg-1 share falls from ≈ 0.58 toward the original-leg-1 share (≈ 0.24–0.27); tail ≈ 1.5–1.9 s (model; a knife edge, λ ≈ μ). **W05:** between W0 and BASE.
- **WDER:** the share falls (derived V ≈ 0.475 on the fast leg, against 1.0), by less than W0.

**Decision rule.**
- **Guard PASS:** `cand52` merges to main (`--no-ff`), carrying lever B and `wdiv-diag`'s inert plumbing.
  - **What vouches for `wdiv-diag`'s default-path cost** (the `cx_leg` count per closed retransmit), which the guard does not cover (it is in both arms): the suites' allocation bounds (`recv_alloc_bound`, `alloc_per_symbol`) and its unit tests. If either allocation bound fails, it does not merge.
- **Guard FAIL:** no merge; the failing clause, its size and the resolution table go to the user.
- **The reported block decides nothing by itself.** A leg-choice change (w_div's value or form) is its own registration after this result.

**§52 amendment 1 (before VM contact, advisor review).**
- **W0's prediction, corrected:** `RWM_PLACE_WDIV=0` places a retransmit like a source symbol; it does not keep it on its original leg. Predicted send-leg-1 share ≈ the source split on leg 1 (≈ 0.18, from the rows' `shares`), not the original-leg share. Predicted tail bracketing both model scenarios: **1.2–1.9 s** (a knife edge).
- **W05: a dose point, no numeric prediction.** At w = 0.5 the term is still ≈ 3 temperatures against the original leg; the idle-leg closed form cannot carry the loaded placement.
- **WDER:** a null (share within BASE's row range) reads **"not executed or no effect"**, never "not executed" alone.
- **Guard risk, corrected:** the live clause is **c1d CPU** (the horizon grows ×1.25–1.5 there; c1d CPU/GB d80 2.4–5 %), not c8.
- **Decision rule, added:**
  - If the reported stage does not complete, this session's guard verdict stands; a re-run's guard is not read.
  - If the 600 MB block reads **CAND worse than BASE beyond BASE's row range** on tail, goodput or receiver duplicates, the merge goes to the user even on a guard PASS (the guard's ≈ 9 s rows cannot see the tail).
  - NO VERDICT ⇒ re-run; ABORT-TESTS ⇒ fixed on the branch and re-run.
- **Before the full launch, the rehearsal must show:** every smoke row's `t52` complete (no `t52_error`, no None among the registered readouts, `cx_leg` summing to `cx_n` on the post-Group-C `[HOLD]` line); each W arm's env in `[GATES]` on both ends. The W rows' values are not read.
- **Archive fixture fix:** plan 52's test reads `bbr48b/diag-ld-full.tgz`, which the ledger export-ignore (measured before `harness52`) dropped from archives. Keep lines added on main and merged into both trees; the tools tests are run inside an extracted archive of CAND, with `LD_BASE_SHA` set, before pinning.
- **Debt at the merge:** `tests/common/recovery_model.rs` still models a 20-gap sender.

**§52 amendment 2 (before VM contact): the trees.**
- **BASE = `base52` `239897ef9948bb432dd3739d9dd7e6ebea982d05`** (`LD_BASE_SHA`). **CAND = `cand52` `ef26a2435212e07c7c920607d193462c7fa4ee8a`** (`LD_CAND_SHA`). `git diff BASE CAND -- raptorpath/src raptorpath/tests` = 3 files, +50 / −12 (lever B alone).
- **Pre-flight (VM, fresh target per tree):** both compile (`--all-targets`); lib BASE 589 / 0, CAND 591 / 0 (the two gap-horizon tests); `holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability` pass in both.
- **Archive check:** in an extracted `git archive` of CAND, all 31 `tools/l1` tests (`test_stubrun` excluded) pass with `LD_BASE_SHA` set, and `test_topo_netem` passes; the plan-52 fixtures are present.
- **Order:** the rehearsal (`LD_REHEARSE=1`, unscored; amendment 1's checks), then the full session on the same pins.

### 52. Result (2026-11-12; session 21:23–23:04Z, ledger `docs/l1-raw/s52/`)

**Labels.** Guard: **`VERDICT GATE-41c PASS`**, p = 0.2471 (min-p 0.0132 at c1s-400 `cpu_cli/GB`), 96/96 LIVE, 0 replaced slots. Block: `VERDICT52 REPORTED-ONLY rows=30/30 live=30`, every arm's witness echoed as registered. **Decision rule: the merge goes to the user.** The 600 MB block reads CAND worse than BASE beyond BASE's row range on **receiver duplicates** (CAND [2786–5169] vs BASE max 1974). Tail and goodput are better, outside BASE's range on the good side.
- **One merge voucher is unmet as registered:** the rule named `alloc_per_symbol`, which the suites never run because it is `#[ignore]`. It was run afterwards with `--ignored` on both trees and passes: BASE 23.14 allocs/symbol (13 404 B), CAND 23.03 (13 100 B).
  - **That does not vouch for what the rule registered.** The rule's purpose was `wdiv-diag`'s default-path cost, the `cx_leg` count per closed retransmit. Both trees carry `wdiv-diag`, so 23.14 against 23.03 cannot see that cost.
  - Neither allocation test reaches the path: both are lossless, so they close no hole by retransmit and print no `[HOLD]` line. `cx_leg` is therefore **vouched only by its unit tests**. That gap goes to the user.
- **Open for the user:** lever B acts at c1d, three c1d clauses lean worse (p 0.035 / 0.063 / 0.056), and no long-transfer c1d block exists.

**Suites.** main 1124 / 0 (47 ignored); doc 0 / 0; wasm 36 / 0; parsers ok; debug-witness 982 / **1** with 46 ignored. The failure was `holeclass_reachability::the_cross_path_class_is_populated_on_two_paths`, which the harness labelled `FLAKE` because it passed on the immediate solo re-run, so `tfail=0`.
- **What failed:** the assertion `age_n >= fed` ("a resolution cannot precede its own first report") on sender path 1, with `fed=6160 age_n=6154` (`flake-holeclass.txt`).
- **Cause:** a wrong test premise, not a race. `HoldObs::on_reported` (`net/holddown.rs`) adds to `age_n` only when the sender still holds a live flight for the seq (`flight: Some`). `fed` counts every resolution of a stamped hole. A hole first reported after its flight is gone is therefore fed without an age reading.
- **What `flight: None` means** (call site `net/sender_phases.rs:836`): the seq is in neither `nack_retx_at` nor `retransmit_buffer`. The sender has already dropped it, so this is a stale gap entry.
- **The test's premise is false on BOTH trees at c8.** In the 600 MB block, BASE's last `[HOLD]` lines already read `age_n < fed` (e.g. p0 `fed=18081 age_n=15353`). The test's own short scenario rarely reaches that state.
- **No CAND link (post-hoc loop, VM, both locks, debug, `--test-threads=2`, pinned trees):** BASE failed run 1 of its loop with the same assertion (path 1, `fed=2305 age_n=2296`), and CAND passed every run: **BASE 1 failure in 10, CAND 0 in 10** (`docs/l1-raw/s52/posthoc/`). The flake is the test's own, present on BASE (`base52` = main + `wdiv-diag`; main itself was not run).
- **Fix (its own commit, after the merge decision, so the pinned `cand52` stays what was guarded):** count first reports separately (`first_n`) and assert `first_n >= fed` and `age_n <= first_n`.
- **Allocation bounds** (the decision rule's vouchers for `wdiv-diag`): `recv_alloc_bound` passed. **`alloc_per_symbol` is `#[ignore]` (a microbench) and did not run.** The registration named a test that the suites never execute.

**Guard clauses** (x = CAND worse, relative median difference; p is the clause permutation p):

| cell | goodput | cpu_cli/GB | cpu_srv/GB | lag_E_cli | lag_E_srv | rtp_floor p0 / p1 |
|---|---|---|---|---|---|---|
| c1s-400 | +0.93 % (0.138) | +1.88 % (0.013) | +1.04 % (0.030) | +3.10 % (0.558) | −0.32 % (0.572) | −0.67 % (0.767) |
| c1d-400 | −2.97 % (0.651) | +0.00 % (0.451) | −0.70 % (0.722) | **+33.99 % (0.035)** | +17.34 % (0.063) | +3.99 % (0.056) / −2.40 % (0.935) |
| c2-100 | +0.07 % (0.653) | −2.10 % (0.797) | −2.17 % (0.705) | +0.18 % (0.609) | +1.81 % (0.211) | −0.41 % (0.614) |
| c8-100 | −4.07 % (0.986) | +0.84 % (0.134) | −3.03 % (0.615) | +0.72 % (0.434) | −10.83 % (0.994) | +12.37 % (0.081) / −3.91 % (0.556) |

- **Resolution:** three clauses are blind below their q95: c1d goodput (q95 0.060 > tol 0.056), c8 goodput (0.065 > 0.040), and c8 `rtp_floor_p1` (0.097 > 0.085).
- **§50 watch item, c1s `lag_E_cli`:** +3.10 %, p 0.558. §50's +15 % (p 0.009) did not recur.
- **c1s `cpu_cli` +1.9 % (p 0.013):** this sits at a cell where lever B was predicted behaviour-identical (reports there carry ≤ 20 ranges). It is the family min-p, not significant after the max-over-clauses correction. Read it as noise unless it recurs.
- **c8 `rtp_floor_p0` +12.4 % (p 0.081)** exceeds its BASE-only q95 of 0.109. That is the criterion that flags c1d below, and c8 is lever B's other cell. It is an **open item for the user** on the same footing.
- **c1d leans worse on three clauses:**
  - `lag_E_cli` +34 % (p 0.035);
  - `lag_E_srv` +17 % (p 0.063);
  - `rtp_floor_p0` +4 % (p 0.056).

  `lag_E_cli`'s +34 % exceeds that clause's BASE-only q95 of 0.32. c1d is one of the two cells where lever B acts (25–30 ranges). The registration named c1d CPU as the risk, not lag, so this lean is unregistered. None of the three is significant after the max-over-clauses correction. **Open item for the user:** no long-transfer c1d block exists to read lever B's effect there.

**600 MB c8 block, per arm** (mean / median [row range], n = 6; `rep52-score.txt` has every field):

| readout | BASE | CAND | W0 | W05 | WDER |
|---|---|---|---|---|---|
| goodput Mbit/s | 98.9 / 98.6 [96.8–101.1] | **107.2 / 107.1 [106.8–107.6]** | 98.2 / 98.5 [95.7–99.7] | 98.5 / 99.1 [95.8–99.6] | 98.7 / 99.0 [95.3–101.9] |
| completion tail s | 4.19 / 4.31 [3.18–5.21] | **0.40 / 0.44 [0.19–0.63]** | 4.35 / 4.29 [3.52–5.56] | 4.31 / 4.10 [3.60–5.70] | 3.82 / 3.59 [2.73–5.97] |
| H0 holes at source end | 1787 / 1909 [1334–2033] | 615 / 446 [67–1420] (n 5) | 2116 / 2159 | 1976 / 1894 | 1814 / 1812 |
| frontier lag at source end | 74149 / 73771 | 10740 / 10980 | 81863 / 77898 | 79857 / 82247 | 74226 / 75032 |
| retx send-leg-1 share | 0.641 / 0.640 [0.610–0.666] | 0.740 / 0.732 | **0.145** / 0.146 | 0.436 / 0.430 | **0.593** / 0.591 [0.524–0.638] |
| retransmits / GB | 25136 [24388–25672] | **29900** [27393–33045] | 26300 | 24899 | 24939 |
| receiver duplicates | 1764 [1583–1974] | **3729** [2786–5169] | 2243 | 1519 | 1614 |
| client CPU s/GB | 34.03 [33.17–36.08] | 36.15 [34.73–37.95] | 34.97 | 34.79 | 34.46 |
| server CPU s/GB | 45.33 [43.85–48.82] | 40.16 [38.53–45.15] | 45.11 | 45.92 | 45.64 |
| [HOLD] p0 first-report age p50/p90 ms | 3539 / 6291 | **344 / 360** | 311 / 6291 | 3539 / 6291 | 3408 / 6029 |
| [HOLD] p1 first-report age p50/p90 ms | 262 / 5243 | 360 / 590 | 147 / 5243 | 295 / 6029 | 360 / 5767 |
| [HOLD] p0 closed-by-retx p50/p90 ms | 172 / 360 | 295 / 590 | 156 / 246 | 164 / 279 | 197 / 360 |
| [HOLD] p1 closed-by-retx p50/p90 ms | 180 / 246 | 328 / 492 | 156 / 246 | 172 / 238 | 172 / 238 |
| cx_leg, orig p0 (pooled, leg 0 : leg 1) | 6955 : 32106 | 1534 : 45448 | 50334 : 6421 | 23776 : 20267 | 11944 : 27995 |
| cx_leg, orig p1 | 20903 : 13 | 20178 : 9 | 19963 : 2385 | 21392 : 183 | 20113 : 34 |
| K = μ·C per 4 s bin (range over bins) | 65–97 | 92–187 (137–187 after 4 s) | 52–82 | 61–81 | 66–119 |
| N_h seqs visited / report | 56–82 | 123–497 | 43–66 | 51–70 | 56–90 |

**Predictions against outcomes.**
- **Guard PASS:** hit. The c1d CPU risk was not realised (cpu_cli +0.00 %, cpu_srv −0.70 %). "c8 goodput worse": not seen (−4.07 %, i.e. better).
- **Lever-B premise (K scales with the horizon):** **partly held.** Main-phase K rose about 2.1× (BASE ≈ 78 mean, CAND ≈ 170 after the first bin), short of the predicted ≈ 200–330 (×2.6–4.2). N_h rose 2–6×.
- **CAND numbers:**
  - tail median 0.44 s against 0.2–0.4 s: at the band's upper edge, just outside it;
  - H0 median 446 against 100–200: **missed**. The row with no H0 is the shortest-tail row (0.194 s), so the n = 5 median is biased upward;
  - first-report age p50 < 1 s: **hit** (p0 3.5 s → 0.34 s).
  - The 64-range cap as the next limit was not instrumented directly; N_h up to ≈ 500 seqs per report is consistent with full 64-range reports.
- **W0 (corrected prediction):** share 0.145 against ≈ 0.18 predicted. Tail 4.35 s against 1.2–1.9 s predicted: **refuted**; W0's tail sits inside BASE's range.
- **W05:** a dose point (0.436); its tail is inside BASE's range.
- **WDER:** its mean share is 0.593, against BASE's row range [0.610–0.666].
  - Rows: 4 of 6 lie below BASE's minimum (0.524, 0.583, 0.586, 0.595), and 2 lie inside it (0.631, 0.638). By the registered outcome witness, the derived branch executed in at least those 4.
  - Tail: 3.82 s [2.73–5.97], inside BASE's range. **Executed, no effect on the tail detected.**


**Post-hoc readouts (not registered; from the committed logs, `docs/l1-raw/s52/rep52-diag-ld.tgz`, last sender `[HOLD]` line per path, summed over paths, mean of 6 rows):**

| per run | BASE | CAND | W0 | W05 | WDER |
|---|---|---|---|---|---|
| resolutions fed | 27 508 | 376 950 | 39 164 | 26 662 | 25 648 |
| healed without retransmit (`hn`) | 17 066 | 362 787 | 25 093 | 15 401 | 15 323 |
| healed with a young retransmit (`hy`, spurious) | 447 | 2 968 | 888 | 324 | 310 |
| closed by retransmit (`cx`) | 9 996 | 11 195 | 13 184 | 10 936 | 10 014 |
| `ripe_frac`, p0 (row range) | 0.62–0.74 | 0.010–0.042 | 0.58–0.66 | 0.66–0.71 | 0.67–0.75 |

- **Scale:** every row moved 507 616 source symbols (`cum=src`). Receiver duplicates are 0.35 % of source on BASE and 0.73 % on CAND.
- **Goodput arithmetic:** 600 MB at 98.9 Mbit/s takes 48.5 s. Removing the 3.8 s of tail gives 107.3 Mbit/s, the CAND figure. **At 600 MB, all of the +8.4 % is the tail; the main-phase rate is unchanged.**
  - The size dependence is unmeasured, and it is not a fixed saving.
  - A fixed 3.8 s on the guard's ≈ 9 s c8 rows would read about +40 %, but the guard read 4.07 % in CAND's favour, inside that clause's blind band.
  - So the tail grows with transfer length.

**Readings.**
1. **Under the 20-gap horizon, leg choice does not move the tail.** On the BASE binary, moving the retransmit send-leg-1 share across 0.145–0.64 (W0, W05, WDER) leaves the tail at 3.8–4.4 s, inside BASE's range. Lever B alone takes it to 0.4 s.
   - This does not show that leg choice is irrelevant once the horizon is 64. On CAND, 97 % of p0 closures go via leg 1, and closed-by-retransmit p50 rose 172 → 295 ms.
   - §48b's leg counterfactual (3.9 → 1.5–1.9 s) is refuted only as a description of the 20-gap binary.
   - The arms stay as levers until a W0-on-CAND row exists. That row is the natural test for lever B's duplicate cost.
2. **Lever B's cost mechanism, measured post hoc:**
   - The 64-gap horizon makes the sender see the slow leg's in-flight reorder gaps as holes: resolutions fed ×13.7, 96 % of them healing without a retransmit.
   - Nearly none of those holes is ripe when first reported (`ripe_frac` 0.65 → 0.02).
   - Retransmits fired on young holes that then healed rise ×6.6 (`hy` 447 → 2 968). These are likely spurious.
   - Per run, against BASE: `hy` +2 521, receiver duplicates +1 965, retransmits +2 858 (+4 764 per GB × 0.6 GB). The three are of the same size.
   - The stale-gap population (first reports without a live flight, `fed − age_n` on p0) does not grow with the horizon: 1.7–2.7 k per run on BASE, 1.0–2.1 k on CAND. It is not the source of the extra duplicates.
   - Client CPU/GB +6 % (overlapping ranges) is consistent with ×13.7 per-hole bookkeeping. Server CPU/GB −11 % likely reflects the shorter run (less tail-time polling); not measured.
   - Hypothesis (untested): the cost comes from the gap gate letting some young holes through, which the wider horizon now exposes, not from the horizon itself. A ripeness-gated retransmit on CAND would test it.
   - **Erratum (2026-11-13, `dup-diag`, Fable-checked).** The ×13.7 `fed` is not a count of the slow leg's in-flight gaps, and `fed`, `hn` and `ripe_frac` are inflated on CAND:
     - `[HOLD]` re-stamps a hole when a report re-lists it after resolution. Total sender `fed` exceeds the receiver's own hole count (`[SUCC] det`, server log) by 114–177 k in all six CAND rows, 10–15× the bound from retransmit re-listing (`cx` ≈ 10–12.5 k). On BASE, `fed` is 25–30 k against 194–238 k receiver holes (the 20-gap view).
     - **Candidate mechanism (log-consistent, red-tested, not yet measured per report):** gap reports are applied out of the order the receiver made them. Each report returns on the leg the triggering data arrived on (`net/receiver.rs` ~2008), p1's return is ~15 ms slower, and nothing orders them (`control_msg.rs:708–745`; the `serve_gaps` coalescing keeps the last-arrived report). The packet-threshold evidence accumulates across reports while the judged seq comes from the current one, so a stale report fires the channel on seqs fresher reports had shown delivered.
     - The original-path keying hypothesis is refuted (`source_path_map` is insert-once; in-leg FIFO measured, `[SUCC] sp_n` p50 0.43–5.2 s).
     - The hypothesis above (gap gate letting young holes through) is superseded by this one.
     - Not settled from the logs: the per-return-leg split of fires (`[FCAUSE]` is per run); BASE's 0.35 % duplicates (not explained, and the proposed filter does not predict touching them).
3. **Debt found:**
   - the holeclass test premise (false on both trees);
   - `alloc_per_symbol` is not a suite test, though the registration relied on it;
   - `tests/common/recovery_model.rs` still models 20 gaps;
   - c1d (open item, above).

## 53. Stale gap reports under lever B: the snapshot-order instrument, the filter arm, W0-on-CAND and a long c1d block — pre-registration (before any VM contact)

**Why.** §52 left the `cand52` merge with the user: lever B removes the c8 tail (4.19 → 0.40 s) but doubles receiver duplicates (0.35 → 0.73 % of source).
- `dup-diag` (`docs/derivations/dup-diag.md`, Fable-checked) names a candidate mechanism: gap reports are applied out of the order the receiver made them.
  - Each report returns on the leg its triggering data used; p1's return is ≈ 15 ms slower, and nothing orders the reports.
  - The packet threshold then fires on seqs a fresher report had shown delivered.
- It is red-tested (`net::tests`: 182 declarations for 1 real loss) and consistent with the §52 logs (§52 Erratum: sender `fed` exceeds receiver holes by 114–177 k on every CAND row), but not measured per report.
- This session measures it and tests the proposed filter. The user asked for W0-on-CAND and a long c1d block before the merge decision.

**Trees** (SHAs pinned by amendment after the VM pre-flight):
- **BASE53** = main `4a486dbe` + `wdiv-diag` (as in `base52`) + `dup-diag` at head `ae7f6c24` (instrument `5cd9790d`, arm `9617970d` default off, the verbatim fold/decision extraction `12eafc33`, tests and the derivation) + `test-debts` `6581d259` (removes the false holeclass `age_n >= fed` assertion, which failed 1/10 on BASE in §52 and would abort the suites).
- **CAND53** = BASE53 + `gaphorizon` (lever B).
- `git diff BASE53 CAND53 -- raptorpath/src raptorpath/tests` must be lever B alone (§52's 3 files).
- Engine bytes differ from `base52`/`cand52` (instrument plus inert arm), so §52's numbers are context, not a control. Every comparison is within this session.
- **Reviewed before pinning (main session):**
  - the instrument folds `V*` over every dequeued batch, coalesced ones included, and only the `RWM_DIAG` gauge reads it;
  - arm off, the coalescer keeps the last-arrived batch and every batch is judged (`snapver_coalesce_keeps`/`snapver_judges` return `true` when the arm is off);
  - the extraction is verbatim.

**Session (`LD_PLAN=53`, REPORTED ONLY, nothing scored).** Suites on CAND53. Then two blocks, rotated per group, no replacement, `RWM_DIAG` on as in §52's diag rows. A rehearsal (`LD_REHEARSE=1`) runs first.
- **c8-100, 600 MB, n = 6 per arm (30 rows):**
  - BASE;
  - CAND;
  - **CANDS** (CAND + `RWM_RECOV_SNAPVER=1`);
  - **BASES** (BASE + `RWM_RECOV_SNAPVER=1`);
  - **CW0** (CAND + `RWM_PLACE_WDIV=0`).
- **c1d, 5 GB (§48's size): BASE n = 12, CAND n = 12, CANDS n = 6 (30 rows).** n = 12 because §52's c1d lean needed it.

**Readouts (per arm; `tail52_score.py` extended, plus a `[SNAPV]` parser):**
- goodput, tail, H0;
- receiver duplicates, total and per arrival leg; retransmits/GB;
- CPU/GB on both ends; `lag_E_cli`, `lag_E_srv`; `rtp_floor_p0` (an open §52 item, both cells);
- `[HOLD]` per path: `fed`, `hn`, `hy`, `hy_p50`, `cx`, `ripe_frac`;
- `fed − det`: sender `[HOLD]` `fed` summed over paths, minus `det` from the receiver's final `[SUCC]` in the server log (the client log's `[SUCC]` lines are zeros);
- `[FCAUSE]`; packet-threshold fires (`y`) and time-threshold fires;
- `[SNAPV]` per return leg: `rep`, `stale`, `pt_st`, `tt_st`;
- the retransmit send-leg-1 share.

**Liveness (a row counts only if):**
- the arm's env appears in `[GATES]` on both ends (BASE/CAND rows show `RWM_RECOV_SNAPVER=0`);
- `[SNAPV]` is present on the sender, with `rep > 0` on each active leg and `pt_st + tt_st ≤` the row's fires;
- on CANDS/BASES rows, `pt_st + tt_st = 0` on every leg, `stale > 0` on p1 (c8), and `fed − det ≤ cx` (the arm skips `[HOLD]` stamps on stale batches, so the re-stamp excess must vanish by construction; `cx` bounds re-listing by retransmit).

No test executes the arm's wiring inside `serve_gaps` (the sim calls the filter function directly), so these outcome witnesses are the only proof it ran. `fed`, `hn`, `ripe_frac` and `fed − det` are witnesses on arm rows, never outcomes.

**Predictions (from `dup-diag.md` §3, amended, before any result).**
- **c8, arm off (BASE and CAND), the mechanism readouts:**
  - p0 `stale/rep` ≤ 0.01; p1 `stale/rep` ≥ 0.8; p1 share of `rep` 0.10–0.25.
  - **The enrichment factor E** = (stale share of packet-threshold fires) ÷ (stale share of judged reports). Under the null (fires independent of report age), E ≈ 1. Predicted: CAND E ≥ 2, because about half of CAND's packet-threshold fires are spurious and the mechanism puts them all on stale reports. BASE E is not predicted (few fires).
  - `tt_st` is not predicted; it measures the time channel's stale share.
- **c8, CANDS against CAND; the outcomes are the receiver duplicates and retransmits/GB:**
  - duplicates 1 800 ± 300 (CAND §52: 3 729), of which p1 700 ± 150 (2 329);
  - retransmits/GB down by the cut in spurious fires (about 2 500 fires per 0.6 GB ⇒ about −4 000/GB);
  - `hy` p0 ≤ 300 and p1 ≤ 350 (as `[HOLD]` reads them on judged batches);
  - goodput +0.2–0.5 Mbit/s (below spread); tail unchanged (0.19–0.63 s).
- **c8, BASES against BASE:** duplicates, retransmits/GB, goodput and tail within BASE's row range.
  - BASES duplicates below BASE's range would mean part of BASE's unexplained 0.35 % is stale-report driven. That is reported as a finding; it is the only test here of BASE's duplicates.
- **c8, CW0 (reported, not discriminating):** duplicates within CAND's range ± 20 % (both hypotheses predict an unchanged total); retransmit send-leg-1 share toward the source split (≈ 0.18); tail not predicted.
- **c1d (equal legs, symmetric return):**
  - direction only: `stale/rep` lower than c8's p1 on both legs, in every arm (no derived bound);
  - CANDS against CAND not predicted numerically.
  - **CAND vs BASE read rule:** per readout (`lag_E_cli`, `lag_E_srv`, `rtp_floor_p0`, CPU/GB both ends, duplicates), the rank-sum p at n = 12 + 12 and the median ratio are reported beside §41c's c1d resolution. The lag clauses are blind at 20–100 % (plan W5.4), so a lag null says nothing. A lean is reported; nothing is scored.
- **Falsifiers:**
  - arm off, c8: p1 `stale/rep` < 0.5, or CAND E < 1.5;
  - arm on, c8: CANDS p1 duplicates > 1 200, or p0 `hy_p50` stays ≈ 5 ms with p0 `hy` > 1 000.

**Decision rule.** Nothing merges from this session.
- **Confirmed and effective:** no falsifier fires, and CANDS duplicates are in 1 800 ± 300 or below BASE's range upper bound.
  - The filter becomes a law candidate (paper first, formula-first) and merges with lever B only after its own §41c guard.
  - The `cand52` merge question goes to the user with CANDS's numbers.
- **Confirmed, partial:** no arm-off falsifier fires, and CANDS duplicates fall between 2 100 and CAND's range lower bound.
  - The filter is a partial fix. The remainder is diagnosed (time channel via `tt_st`, BASE's own duplicates) before any law step.
- **Confirmed, ineffective:** no arm-off falsifier fires, and CANDS duplicates are inside CAND's range. Back to diagnosis; the user gets the counters.
- **Refuted:** an arm-off falsifier fires and CANDS duplicates stay inside CAND's range. The §52 Erratum's candidate is withdrawn in status; `cand52` goes to the user on §52's numbers plus CW0's.
- **Contradictory:** an arm-off falsifier fires, but CANDS duplicates still fall below CAND's range. The filter works through a mechanism other than the one registered. No law step; re-diagnose with the per-report counters before any claim.
- c1d is reported in every case, beside the user's open c1d item.
- A void row (liveness fails) ⇒ that arm is re-run. ABORT-TESTS ⇒ fixed on the branch and re-run.

**Before the full launch, the rehearsal must show:**
- `[SNAPV]` parsed on every smoke row, with `rep > 0` per active leg and `pt_st + tt_st ≤` fires;
- `fed − det` computed from the server log's final `[SUCC]`;
- each arm's env in `[GATES]` on both ends;
- one c1d 5 GB row finishing inside `RWM_PERF_TIMEOUT_S`;
- no None among the registered readouts.

The rehearsal's values are not read. The plan-53 harness gets a stub-run test before pinning (plan W5.2 pattern).

**VM pre-flight (same session, before the pin):**
- fresh target per tree; `cargo check --all-targets`; lib `net::` and `gates`;
- the integration tests `holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability`;
- `build_visualizer.sh` on a fresh wasm build, plus the native golden test without `GOLDEN_CAPTURE` (for `wasm-modeswitch-audit`);
- the `bound-tests` wasm pool test.

**§53 amendment 1 (before VM contact): trees, harness, three premise corrections.**
- **BASE = `base53` `5b605bb0f550f54a298b7f314b147424b0ad805b`** (`LD_BASE_SHA`). **CAND = `cand53` `def76c2350c5782d6e7303d078018d6c2d00eff7`** (`LD_CAND_SHA`).
  - `base53` is built on main `83fa8296`, not `4a486dbe`; the two differ in `docs/status.md` only.
  - `git diff base53 cand53 -- raptorpath/src raptorpath/tests` = 3 files (`net/mod.rs`, `net/sack.rs`, `net/tests.rs`), +50 / −12, lever B alone. CAND adds two harness commits (`tools/l1`).
  - Two merge conflicts, both from `wdiv-diag`, resolved as in `base52`: `net/tests.rs` (main's removals kept, the `cx_leg` test adapted to the one-argument `HoldDownGauge::new`) and `tools/l1/lib.sh` (echo list).
  - `RWM_RECOV_SNAPVER` and `RWM_PLACE_WDIV` are in `lib.sh` and the `[GATES]` byte pin.
- **Host checks:**
  - `cargo check --all-targets` exits 0 on both trees; `--lib gates` 15 / 0 on both; `--lib net::` BASE 295 / 0, CAND 297 / 0.
  - The `tools/l1` tests pass on the host and inside an extracted `git archive` of CAND with `LD_BASE_SHA` set (32 / 32).
  - The stub run executes plan 53's rehearse, full, prior-tests and mismatch paths end to end; three red launch cases (no `[SNAPV]`, a stale fire on an arm row, no final `[SUCC]`) fail as they must.
- **Harness:** plan 53 in `loopdiag_battery.sh` / `loopdiag_run_all.sh`, scorer `tools/l1/snap53_score.py` (`test_snap53.py`).
  - 60 rows: c8 5 arms × 6, then c1d 5 GB BASE 12 / CAND 12 / CANDS 6. `RWM_DIAG=1` and `RWM_PERF_TIMEOUT_S=200` on every row.
  - c1d CAND vs BASE: one-sided rank-sum p (worse side) via `place41b_score.statistic_rank`, with the min-p gate not printed (nothing scored).
  - On the §52 logs the parsers reproduce the Erratum (`fed − det` +114 052 to +176 958 on CAND, negative on BASE).
- **Premise corrections:**
  1. **The packet-threshold fire count is `fast=`, not `y=`.** In the sender's `mpr[…]` block, `y` counts fires whose live flight was younger than the time threshold. E uses `fast` (the counter whose flag also feeds `pt_st`). `y == fast` on all 30 §52 c8 rows and 24 c1d guard rows; any row where they differ is printed as `Y-NE-FAST`.
  2. **"No None among the readouts" exempts readouts that are undefined, not missing:**
     - H0 when the row's tail is shorter than one `[DIAG]` tick (1 of 6 §52 CAND rows; every c1d row) and `tail_s` is present;
     - `hy_p50` when `hy_n` = 0;
     - E when `fast` = 0 or Σ `stale` = 0.
  3. **The tail-sweep pseudo-leg (`-`) carries no snapshot.** Σ `rep`, Σ `stale` and Σ `pt_st` in E run over the numeric return legs only. `rep > 0` is checked on every active leg, so a missing leg line fails rather than passing vacuously.
- **Statistics (each band is read on these):**
  - count readouts and their bands, falsifiers and decision bands (duplicates, retx/GB, fires, `hy`, goodput, tail, CPU/GB) are read on the **arm mean**, as §52's reference figures are means; "within range" is the arm's row range;
  - E, `stale/rep` and the p1 share of `rep` are **pooled**: sums over the arm's LIVE rows, then the ratio;
  - "within BASE's range on every readout" (BASES) means duplicates, retx/GB, goodput and tail only.
- **The arm-row `[HOLD]` witness holds.** On a stale batch the arm passes an empty gap slice to `HoldDownGauge::on_report`, which returns before resolving anything (`max()` over no gaps is `None`). The stamped set is untouched, so `fed − det ≤ cx` is the right bound.
- **Stale-absent escape.** If every LIVE arm-off row of a block shows p1 `stale` = 0, the arm rows of that block are not voided for p1 `stale` = 0 (the scorer prints `STALE-ABSENT`). The arm-off falsifier then decides, and Refuted stays reachable. The smoke applies the same rule to its own BASE/CAND rows.
- **Binary witness.** `[GATES]` cannot tell the BASE binary from CAND's. The c8 tail is the witness: BASE and BASES about 4 s, CAND, CANDS and CW0 about 0.4 s (§52). An arm on the other side of that split is read as built on the wrong binary and voided by hand in the Result (the scorer does not apply this rule).
- **Rehearsal, one criterion added:** the filter arms' liveness (CANDS/BASES: `pt_st + tt_st = 0` on every leg, `stale > 0` on p1 at c8, `fed − det ≤ cx`) is checked on the smoke rows too, so an arm that does not act aborts the smoke instead of voiding 12 rows later. Under `LD_PLAN=53` a missing `RWM_RECOV_SNAPVER` token in `[GATES]` is a failure, never read as off.

**§53 amendment 2 (before the rehearsal): VM pre-flight, CAND re-pinned.**
- **VM pre-flight** (fresh target per tree, 06:16–06:31Z):
  - **BASE53 `5b605bb0`:** `check --all-targets` rc 0; lib `net::` + `gates` 308 / 0; `holeclass_reachability` 3 / 0, `recovery_bench` 7 / 0, `sim_nack_test` 4 / 0, `place_arms_reachability` 3 / 0.
  - **CAND53 `def76c23`:** `check` rc 0; lib 310 / 0; the same integration tests pass except `recovery_bench_fixtures_pin_the_plane`, which fails at FIXTURE 2's channel-mix pin.
- **Cause: a pin, not the engine.**
  - Since `test-debts`, `tests/common/recovery_model.rs` reads the engine's gap horizon (64 on CAND). FIXTURE 2's constants are bench outputs captured on the 20-gap plane, and `cand52` passed only because its model was still hard-coded to 20.
  - **Fix (tests only):** FIXTURE 2 selects its expected set by `gap_horizon()`, either 20 (unchanged) or 64 (captured on `cand53`); any other horizon panics.
- **CAND = `cand53` `b21421e6ca6062c7235394a8ec07a8c0f63062f3`** (`LD_CAND_SHA`; supersedes `def76c23`). BASE unchanged.
  - On the VM, `recovery_bench` 7 / 0 and `slack_bench` 8 / 0 on the new tree.
  - `git diff base53 cand53 -- raptorpath/src raptorpath/tests` is now lever B's 3 files plus `tests/recovery_bench.rs` (the horizon-keyed fixture).
- **Finding (model, reported; it predicts nothing for this session).** On the test-side recovery model at the c7 fixture point, the 64-gap horizon moves 70 of 154 app-echo-clock holes from the fast channel to the time channel:
  - app-echo mix (time/fast) [0, 154] → [70, 84]; p50 to first service 14 776 → 12 536 µs; sweeps 1 → 2; retransmits unchanged at 154;
  - the wire-clock mix stays [70, 84] at both horizons.

### 53. Result (2026-11-13; session 07:12–09:11Z, ledger `docs/l1-raw/s53/`, Fable-checked)

**Verdict at c8: CONFIRMED AND EFFECTIVE** (the decision rule's first branch).
- No falsifier fires.
- CANDS duplicates (mean 1 291) are below BASE's range upper bound (1 733) and below the registered band 1 800 ± 300.
- Session: 60 / 60 rows LIVE, 0 VOID; suites on CAND53 `tfail=0`; rehearsal and smoke PASS.
- Binary witness holds: c8 tail BASE/BASES 2.63–5.44 s, CAND/CANDS/CW0 0.14–0.68 s.

**c8-100, 600 MB (arm means unless marked; ratios pooled; n = 6 per arm):**

| readout | BASE | CAND | CANDS | BASES | CW0 |
|---|---|---|---|---|---|
| goodput Mbit/s | 99.3 | 107.1 | 107.8 | 98.6 | 106.4 |
| tail s | 3.95 | 0.36 | 0.41 | 4.42 | 0.30 |
| receiver duplicates | 1 546 | 4 004 | **1 291** | 1 427 | 6 691 |
| duplicates, range | 1 391–1 733 | 2 799–5 242 | 1 101–1 487 | 1 234–1 802 | 5 858–7 807 |
| duplicates p0 / p1 arrival | 908 / 638 | 1 496 / 2 509 | 746 / 545 | 816 / 611 | 5 332 / 1 359 |
| retransmits / GB | 25 298 | 30 661 | 25 157 | 24 938 | 36 940 |
| fires (y + r) | 15 179 | 18 396 | 15 094 | 14 963 | 22 164 |
| `[HOLD]` `hy` p0 / p1 | 212 / 200 | 2 704 / 682 | 93 / 34 | 163 / 17 | 6 309 / 352 |
| p0 `hy_p50` | 54 ms | 5.3 ms | 100 ms | 60 ms | 4.9 ms |
| `[SNAPV]` p1 `stale/rep` (pooled) | 0.986 | 0.993 | 0.993 | 0.984 | 0.997 |
| p1 share of `rep` (pooled) | 0.248 | 0.289 | 0.272 | 0.263 | 0.337 |
| p1 `pt_st` / `tt_st` | 158 / 999 | 3 469 / 2 481 | 0 / 0 | 0 / 0 | 6 376 / 2 416 |
| E (pooled) | 2.91 | 2.03 | — | — | 1.87 |
| `fed − det` | −165 k | +147 k | −58 k | −197 k | +40 k |
| retx send-leg-1 share | 0.59 | 0.74 | 0.73 | 0.60 | 0.17 |
| client / server CPU s/GB | 36.3 / 48.3 | 37.4 / 42.7 | 36.6 / 42.0 | 36.2 / 48.9 | 36.7 / 41.4 |
| `rtp_floor_p0` µs | 8 967 | 10 256 | 11 536 | 8 145 | 10 855 |
| `rtp_floor_p0`, range | 7 863–10 235 | 8 395–12 028 | 9 750–12 656 | 7 039–10 174 | 9 060–12 590 |

**Against the predictions.**
- **Arm off, the mechanism readouts:**
  - p0 `stale/rep` 0.000 (≤ 0.01 ✓); p1 0.986/0.993 (≥ 0.8 ✓); CAND `pt_st` 3 469 (2 500 ± 1 000 ✓, at the edge); BASE `pt_st` 158 (≤ 800 ✓).
  - CAND E 2.03 (≥ 2 ✓, at the edge; falsifier < 1.5 not met).
  - **Missed:** CAND's p1 share of reports, 0.289 against 0.10–0.25 (BASE's 0.248 is inside).
- **CANDS against CAND:**
  - duplicates 1 291 (registered 1 800 ± 300: better than the band, below BASE's mean); on p1 545 (registered 700 ± 150: just below the band);
  - retransmits/GB −5 504 (registered ≈ −4 000), back to BASE's level;
  - `hy` p0 93 (≤ 300 ✓), p1 34 (≤ 350 ✓); fires 15 094 (15 400 ± 700 ✓);
  - tail 0.41 s, inside CAND's range ✓;
  - **missed:** goodput +0.7 Mbit/s against the registered +0.2–0.5;
  - arm-on falsifiers: p1 duplicates 545 < 1 200; p0 `hy_p50` 100 ms, not ≈ 5 ms; `fed − det` < 0 on every row. None fires.
- **BASES against BASE:**
  - duplicates, retransmits/GB, goodput and tail are inside BASE's row range ✓.
  - Duplicates read −119 on the mean, and BASES's median (1 326) is below BASE's row minimum (1 391). BASE has ≈ 1 160 stale-judged fires per run, and BASES fires 216 fewer.
  - Supportable: no shift beyond BASE's range at n = 6. A stale-driven share of BASE's 0.35 % of order 10 % is not excluded; the rest is unexplained.
- **CW0 (reported, not discriminating): the prediction failed.**
  - Duplicates are 6 691, +67 % on CAND, outside the registered ± 20 %; 5 332 of them arrive on p0.
  - The send-leg-1 share fell to 0.17 as predicted (≈ 0.18).
  - The registered account predicted an unchanged total, but `pt_st` rose 3 469 → 6 376 and receiver holes `det` 258 k → 346 k. The duplicate account is incomplete for this arm.
  - *Hypothesis, untested:* with the defect present, placing retransmits like source moves spurious copies onto the fast leg and adds more.
  - Goodput −0.7 % against CAND (p = 0.003); tail 0.30 s.
  - CW0 ran with the defect, so **the leg question under the 64-gap horizon is still open**; W0 with the filter is the row that answers it.

**c1d-400, 5 GB (BASE n = 12, CAND n = 12, CANDS n = 6; median unless marked):**

| readout | BASE | CAND | CANDS |
|---|---|---|---|
| goodput Mbit/s | 824.5 | 805.7 | 807.8 |
| receiver duplicates | 3 011 | 3 413 | 3 253 |
| retransmits / GB | 4 850 | 5 081 | 5 145 |
| client / server CPU s/GB | 21.31 / 22.89 | 21.45 / 22.92 | 21.77 / 23.41 |
| `lag_E_cli` µs | 1 898 | 1 655 | 1 669 |
| `stale/rep` p0 / p1 (pooled) | 0.073 / 0.089 | 0.158 / 0.091 | 0.112 / 0.072 |
| E (pooled) | 5.61 | 3.29 | — |

- **Direction check ✓:** c1d `stale/rep` is 0.07–0.16, against c8 p1's 0.99.
- **CAND vs BASE at c1d (rank-sum, one-sided worse side, n = 12 + 12; median ratios):**
  - **duplicates +13 % (p = 0.033)**; retransmits/GB +5 % (p = 0.12); goodput −2.3 % (p = 0.24); CPU/GB both ends +0.2 to +0.6 % (p = 0.36, 0.49); `rtp_floor_p0` +1 % (p = 0.21).
  - `lag_E_cli` −13 % (p = 0.98), `lag_E_srv` −13 % (p = 0.94).
- **CANDS against CAND at c1d:** duplicates −5 % (p = 0.54); arm means inside CAND's range (its row ranges exceed CAND's on both lag readouts). **The filter does not remove lever B's c1d duplicate cost** at this resolution.
- **§52's c1d lean** (`lag_E_cli` +34 %, at the guard's 400 MB size) **is not seen at 5 GB**; the guard-size reading was not re-run. The lag clauses are blind at 20–100 %, so this is a null, not a clearance.
- **Stale reports occur at c1d too:** 7–16 %, about 1.5–1.6 k stale-judged fires per run (`pt_st + tt_st`), enriched (E 3.3–5.6), against ≈ 23–25 k fires per run.
- **One low row:** BASE s42 r6 reads 365 Mbit/s, with 705 duplicates and 7 300 fires (leg shares 0.24 / 0.76). The §38 lock-out classifier does not flag it (no leg below 10 % share); the cause is unexplained. It drags BASE's means, which is why this table reads medians.

**Open items (reported, not ruled):**
1. **c8 `rtp_floor_p0` rises on every lever-B arm** (§52 open item).
   - Means: BASE 8 967, BASES 8 145; CAND 10 256, CANDS 11 536, CW0 10 855.
   - CANDS against BASE is +29 % on the mean, +35 % on the median, with ranges overlapping only between 9 750 and 10 235.
   - CANDS against CAND (+15 %, median ratio, p = 0.091) is not established, since BASES reads lower than BASE.
   - Mechanism unknown. It belongs beside the duplicate numbers in the merge question.
2. **Lever B's c1d duplicate cost** (+13 %, p = 0.033) is not removed by the filter.
3. **BASE's duplicates** (≈ 1 500 per c8 run, ≈ 3 000 per c1d run) are mostly unexplained (above).
4. **The time channel also fires on stale reports:** CAND p1 `tt_st` 2 481, BASE 999. The filter removes both channels' stale fires.
5. **CW0's duplicate rise** (above), and the open leg question.
6. **`[HOLD]` is skewed on main too** (extends the §52 Erratum): BASES p0 `hn` 244 against BASE 10 044, `ripe_frac` 0.98 against 0.70. Main's `hn`/`ripe_frac` readings are mostly re-stamps from stale reports. §3.10 item 5's "12 % wasted" must be re-checked if it came from this gauge.

**What follows (no merge from this session).**
- **The filter becomes a law candidate:**
  - formula and derivation into the paper first (FORMULA-FIRST);
  - a test that executes the `serve_gaps` wiring (this session proved it by outcome only);
  - then the arm becomes the default law with no env;
  - then a §41c guard of CAND + filter against main.
- **The `cand52` merge question goes to the user, with:**
  - CANDS's c8 numbers: the tail gain (+8.5 % goodput at 600 MB c8), with duplicates and retransmits/GB at or below BASE's;
  - the costs that remain: c8 `rtp_floor_p0` +29 % and c1d duplicates +13 %.

**§53 Erratum 2 (pre-flight scope).** The registered VM pre-flight listed `build_visualizer.sh` on a fresh wasm build, the native golden test without `GOLDEN_CAPTURE` (for `wasm-modeswitch-audit`), and the `bound-tests` wasm pool test. The pre-flight script ran only the trees' check, lib and integration tests; those three were NOT run. They move to the next VM session's pre-flight.

**§53 Erratum 1 (2026-11-13, post hoc, `p0-dwell-diag` `a916359e`, Fable-checked).**
- **`rtp_floor_p0` is a run-minimum artefact.**
  - It is the lowest p0 Copa `min_rtt` (`rtp_us`) over the run's `[DIAG]` ticks.
  - On BASE/BASES that minimum falls 0.2–3.7 s after source end, in the drained tail. On every lever-B row (17 of 18; one CANDS row at source end) it is the t = 0.3 s start-up sample.
  - The clause therefore reads "did the run end with an idle drain" and scores any tail fix as worse (§3.10 item 7).
- **Delivered delay: the Result's "about the same (~0.9 s)" is withdrawn.**
  - `[LAT]` keeps at most 16 384 pending arrivals (`net/lat.rs` `PENDING_MAX`) and drops the rest (`over=`).
  - At c8 it recorded 11–24 % of deliveries on BASE/BASES (`over` 0.77–0.90 M), against 78–99.9 % on lever-B rows. Its BASE medians come from a biased subset.
  - Replacement, checked: mean in-order delay ≈ 5.3 s on BASE/BASES against ≈ 1.0 s on lever-B rows (`p0-dwell-diag/inorder.py`: the horizontal distance between the cumulative source-sent curve and `window_ack_seq`, the receiver's cumulative in-order ack, on the sender's `[DIAG]` ticks). Lever B's figure agrees with its near-complete `[LAT]` (≈ 0.9 s). BASE's mean exceeds its 3.95 s tail because main-phase holes reach the 20-gap sender only at ≈ 3.5 s age (§3.10 item 5), so the in-order frontier stalls for seconds during the main phase, not only at the end.
  - Earlier c8 delivered-delay readings from `[LAT]` must be checked for `over` before they are cited.
- **The real effect behind the floor: lever B moves store slots from old holes into quinn's p0 send buffer.**
  - **Store at its clamp:** the c8 sender is store-gated (`paused` ≈ 80 %). The store sits at its clamp N·pool = 4 096 for 0.74–0.98 of the run (`[SUMCAP] pin=`), on every arm, main included.
  - **Links saturated:** both legs send at link rate on every arm (p0 ≈ 9 855/s).
  - **Where the slots go:** lever B repairs holes before they age, so about 1 400 slots that held holes older than one RTT refill with source. p0's population rises from ≈ 2 020 to 3 350–3 660.
  - **Where it waits:** quinn's window caps p0 on the wire at ≈ 1 100–1 600, so the rest waits in quinn's send buffer (`dgq0` hand − tx ≈ 700–870 → 1 560–3 000).
  - **What it costs:** the p0 app-echo RTT and per-symbol first-arrival latency rise by ≈ 150–230 ms. The wire RTT is unchanged.
  - **Ruled out:** placement (p0 share 0.82 on every arm), retransmit volume (CANDS carries the population at fewer p0 retransmits than BASE), and quinn draining slower (p0 tx rate equal).
  - **Status:** the mechanism is consistent with Little's law on a fixed slot budget. The causal test (a pool arm) is pending; occupancy = rate × RTT + holes closing on every row is consistency, not proof.
- **Lever-B cost to report with the merge question:** quinn's send buffer evicts up to ≈ 1 000 datagrams per run on p0 under lever B (CW0 the most). These are counted per arm in `docs/derivations/p0-dwell-diag.md`. Their link to CW0's duplicate rise is untested.

## 41d. Merge guard v2 (ACCEPTED): the start-up RTT floor off at c2/c8, in-order delay scored on 600 MB c2/c8 rows — instrument change, no VM

**Status: ACCEPTED (owner, 2026-11-13).** Owner calls: (1) the start-up floor is REPORTED ONLY everywhere (option b); (2) the +34 min per guard is accepted. The first guard to use v2 cannot be registered until the implementation prerequisites are met.

**Why.**
- `rtp_floor_pN` is the run minimum of Copa `min_rtt`, a 10 s windowed minimum of the app-echo RTT. On guard-length rows (≈ 9 s) the window never rolls:
  - at c1s, c1d and c2 the clause reads the run's first non-zero sample on 89 of 96 (row, leg) pairs;
  - at c8 it reads the start-up sample on about half the rows and the end-of-run sample on the rest (§3.10 item 7).
  - At 600 MB it reads the end-of-run drain on 20-gap rows and the start-up sample on lever-B rows (§53 Erratum 1).
- It entered at threading P1 (§11, `c765e3f3`, `a557afcf`) as a probe for sender-induced queueing and was copied into every later guard. No design rationale for it as a merge clause is recorded.
- The guard has no delivered-delay clause. On §48b's ledger `bbr_rs` raised c8 in-order delay by 46 % (BASE 3 756–4 714 ms, CAND 5 456–6 587 ms, disjoint) while `rtp_floor_p0` read it as better (ratio 0.814).
- Evidence: `docs/derivations/guard-clause-dry.md` (branch `guard-clause-dry` `5f73fc6b`).

**Change (the statistic, rank-minp, 10 000 relabellings and PASS iff p > 0.05 are unchanged):**
1. **Row size:** c2 and c8 guard rows carry 600 MB (§48b's sizes, 43–56 s rows) instead of the guard size. c1s and c1d keep the guard size.
   - This changes EVERY c2 and c8 clause, not only the new one: longer rows can tighten goodput, CPU/GB and lag resolution, so v2 may fail a change v1 passed.
   - **Owed before adoption:** the 600 MB d80 per c2/c8 clause from §48b's ledger, beside §41c's guard-size d80.
2. **`rtp_floor_p0`/`p1`:** dropped from scoring everywhere (reported only; owner call 1).
3. **In-order delay mean (new, one-sided, higher is worse),** scored at c2 and c8:
   - **definition:** `delay(t) = t − t_src(ack(t))`, with `ack` the receiver's cumulative in-order ack (`window_ack_seq`, the third `cum=` field of the sender's `[DIAG]`) and `t_src` the inverse of the cumulative source-sent curve, linearly interpolated; the mean over ticks with `ack > 0`;
   - **edge cases:** ticks before the first ack are excluded, and an `ack` beyond the last source tick is excluded;
   - **void rule:** a c2/c8 row with under 30 s of ticks is VOID (re-run), never neutral-rank filled;
   - its p95 is never scored (quantised at 250 ms ticks), and it is never scored at c1 (A/A d80 20 % to over 100 %);
   - **domain:** reliable delivery only. Under shed or force-deliver the frontier passes undelivered seqs, so off-Bulk rows (ρ < 1) need a different definition before this clause applies there.
4. All other clauses are unchanged.

**Dry run: a SPLICE of existing ledgers.** No ledger matches v2's design (c1 at guard size, c2/c8 at 600 MB). Old p values were reproduced exactly first.

| ledger | what it can show | old p (min-p clause) | spliced p (min-p clause) | verdict |
|---|---|---|---|---|
| §52 guard | the dropped c2/c8 floor clauses; there are no long rows, so the new clause is absent | 0.2471 (c1s `cpu_cli/GB`) | 0.2206 (same) | PASS → PASS |
| §48b | the new clause on long c2/c8 rows; its c1 rows are 5 GB, not guard size | 0.0020 (c2 goodput) | 0.0018 (same) | FAIL → FAIL |
| place41b A/A | c1 only: unchanged by construction | 0.6920 | 0.6920 | PASS → PASS |

- **New clause resolution (A/A d80, §48b 12 + 12 long rows):** c2 8 %, c8 20 %.
- **Kept c1 floor clauses at guard size (§52 guard):** 5–8 %.
- No ledger carries the new clause as its only failing clause, so a v2-only flip has not been observed.

**Implementation prerequisites (before the first v2 guard is registered):**
- a selectable clause-set version with v1 kept (`guard47_checks.py` asserts the exact key set, and `test_bbr48_parse` pins a v1 result by clause name, so renaming or removing clauses breaks both);
- the `guard-clause-dry` functions merged; the guard plan's c2/c8 size and timeout; a stub-run path for v2; the void rule;
- the 600 MB d80 table (item 1).

**Owner calls (open):**
1. **The start-up floor at c1s/c1d.**
   - Option (a): keep it scored, under the honest name `startup_rtt_floor_pN`. It is live: c1d p0 read p = 0.056 in §52 and was the min-p clause on §47's ledger. No regression it would uniquely catch is recorded.
   - Option (b): report it only, everywhere.
2. **The cost:** the guard's row time rises from ≈ 13.6 to ≈ 48 min (+1 070 s at c2, +986 s at c8 per 24 rows), before per-block overheads.

**§41d implementation (2026-11-13, branch `guard-v2` `9e687938`).**
- **How to run it:** `LD_PLAN=41d` is §41c's 96-row design with clause set v2 (c2/c8 at 600 MB, `RWM_PERF_TIMEOUT_S=200`, scorer `score41d`).
- **v1 is unchanged:** it stays the default and reproduces §52 p = 0.2471 and §48b p = 0.0020 bit for bit.
- **VOID rule:** a live c2/c8 row whose last `[DIAG]` tick is earlier than 30 s is `VOID-SHORT` and re-run. A DNF keeps its worst rank.
- **The accepted v2 set on §48b's ledger:** p = 0.0015, FAIL (min-p at c2 goodput).
- **Owed d80 table** (a splice: §52 c1 rows plus §48b c2/c8 rows; 30 draws, seeds 0/1/2; `docs/derivations/guard-clause-dry.md` §11):
  - **v2 is stricter on goodput:** c8 12 % → 2 %, c2 2.4 % → 1 %; c2 `cpu_cli/GB` 8 → 5 % and c8 `cpu_srv/GB` 12 → 8 % are one grid step each;
  - **v2 is looser on `lag_E_srv`:** c2 50 → 100 %, c8 20 → 50 %;
  - **unchanged:** the other lag and CPU clauses;
  - **the new clause:** in-order delay mean, c2 5–8 %, c8 20 %.
- **Status:** a real v2 guard ledger replaces this splice at its first use.

**§48b addendum (reported; nothing changes retroactively).** `bbr_rs` raised c8 in-order delay by 46 %. It is frontier lag while the source is still sending, not the drained tail: B′, over the source phase only, reads the same. §48b's Result spoke of delivered delay at c2 only (the app-echo A_x ≈ 206 ms in both arms). §48d must register in-order delay as a readout and carry this as a known cost.

## 56. The store-pool sweep (reported only, no build): current-era static maps at c8, c2 and c1s — pre-registration (before any VM contact)

**Context.** Owner decision of 2026-11-14 (§54, after launch 2): the store law first, the lever-B guard paused. The derivation (`docs/derivations/store-law.md`, merged as docs only) gives a rate-free time law,

`n_{k+1} = clamp( n_k · Σᵢ wᵢ Fᵢ / Σᵢ wᵢ T̂ᵢ , floor, N·knee )`,  Fᵢ = max(m̃ᵢ + b̂₀ᵢ⁺, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺)) + q(δ)·ℓ̂ᵢ.

It needs no link-rate input: the rate cancels out of the ratio. The derivation closes Fable's three blockers on static maps:
- b₀ is a one-sided upper bound;
- the anchor is bounded by m̃ = min(m̂, ℓ̂ + b̂₀⁺);
- the leg coupling has one attractor.

**Not registrable as a law:**
- its predictions are static crossings on p27/p34-era ledgers;
- its Bulk costs (c1s −10 to −20 %, c1d ≈ −10 %, c8 ≈ −3 to −5 %) would fail guard v2;
- δ's term (q·ℓ) is smaller than the estimator slack, so δ has almost no authority (U2).

This block measures the maps the law's closed-loop registration (Part B) needs. It is derivation §13, copied here, and is Fable-reviewed twice.

**Why.**
- §54b measured three pools at 6.8–21.8 × the c8 BDP and does not bracket the knee.
- The store-law derivation (`docs/derivations/store-law.md`) predicts two shoulders at c8 and a single-leg knee at
  c2. Its inputs are static maps of an older era (p27: 100 MB, 2 rows per cap; p34 for c1).
- This block measures current-era static maps with the existing knob. It decides nothing by itself. It feeds the
  store law's closed-loop registration (Part B), the δ-authority question U2, and the lever-B guard.
- It is independent of the law's open blockers, because no law is built.

**Tree, knob and suites.**
- **Tree:** current main. The SHA is pinned by amendment after the VM pre-flight.
- **Knob:** `RWM_STORE_PATH_POOL=<pool>` (`gates.rs:323`).
  - The realised ceiling is `max(N·pool, floor)` (`store_cap.rs:90`). The boot cap is `min(128, store_max)`.
  - Every arm's W is ≥ 160 > 128, so neither bound inverts against the boot cap.
  - W binds only while the shipped ask exceeds it; see Liveness.
  - `RWM_STORE=<n>` is not used: it switches the law off and moves the seat.
  - The battery scrubs `-u RWM_STORE_PATH_POOL` (as plan 54b).
- **Engine suites** are skipped only if `raptorpath/src` and `raptorpath/tests` are byte-identical to the tree that
  passed §54's suites (`cand54` `7fd92dd5` / its merge on main). This is checked by a reuse manifest: a per-file
  sha256 list, diffed on the VM.
  - Any difference runs the full suites as in §54.
  - The tools tests (`tools/l1` python and bash tests) and a rehearsal (`LD_REHEARSE=1`) always run, because the
    sweep is a new harness path.
- **`RWM_SUCC_DUMP`** exists on main (default `RWM_SUCC_DUMP_MAX` = 200 000 records). The dump is written under
  the run tree on `/dev/shm`. The pre-flight checks that `/dev/shm` has ≥ 1 GB free, and the rehearsal records the
  dump's actual size.

**Arms** (Bulk, `--window-reliable`, `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300`):

| cell, size | pools (W) | n per pool | rows |
|---|---|---|---|
| c8-100, 600 MB | **640 (W 1 280, control = §54b B640)**, 384 (768), 256 (512), 160 (320), 112 (224), 80 (160) | 4 | 24 |
| c2-100, 600 MB | **2 048 (control, the shipped pool)**, 384, 256, 192, 160, 128 | 3 | 18 |
| c1s-400, 5 GB | **2 048 (control)** and 512: n = 6; 1 024, 768 and 384: n = 3 | 6 / 3 | 21 |
| c8-100, 600 MB, `RWM_SUCC_DUMP=1` | 640 | 1 | 1 (the hn_n anomaly, §6) |

- **Total:** 64 rows.
- **Order:** cells in blocks, c8 → c2 → c1s. Within a block, a Latin-square rotation of pools per replicate, seeds
  42 / 7 alternating. The c1s n = 6 arms take the extra replicates at the block's end, rotated.
- **Smoke:** one row per cell at its smallest pool.
- **Length:** about 24 + 18 + 22 + 1 min of rows. With the tools tests, the rehearsal and overheads that is about
  1.5 h. `LD_HARD` = launch + 3 h.

**Liveness and row treatment.**
- **Pool echo.** `[GATES]` and `pool_per_path` echo the requested pool on both ends. If they do not, the row is
  VOID and is replaced once.
- **Binding.**
  - **Control arms are exempt.** They are the shipped behaviour: c1s binds ≈ 0.7 at 2 048, and c2 is not pinned
    at 2 048.
  - A **non-control** row binds if its LATE-window bind fraction is ≥ 0.95. The LATE window is sending ticks at
    t ≥ 15 s (c1s: t ≥ 5 s).
  - An unbound non-control row is **not replaced**, since a rerun would repeat the miss. It is reported at its
    realised cap: the LATE-window mean of `win=…/b`. It is plotted at that W, and labelled `UNBOUND`.
- **DNF** (as 54b): goodput counts as 0, and every other readout comes from the row's `[DIAG]` lines.
- **Flags** (reported, never voiding):
  - **c1s quinn-window collapse:** the share of windows with `qcwnd` < ¼ of the row median exceeds 0.1 (§43b's
    duration measure).
    - Flagged rows are reported in every table.
    - They are **excluded from F8**, and the count is stated.
  - **c8:** the lock-out classifier (§3.9).
- **The SUCC_DUMP row** must write a dump with ≥ 100 000 records. Otherwise it is re-run once.

**Readouts per row** (LATE window; `store-law/little54b.py` and `pool54_score.py` definitions):
- per leg: tx rate rᵢ, app-echo rtt and wire rtt, `rtp_us` against its boot value, q₀ (`dgq hand − tx`), and the
  placement share;
- Σ send rate, goodput, tail, H0;
- in-order delay, late mean and p95;
- old holes (server `[RANK]`), `[HOLD]` `hn_n`, `fed` and `ripe_frac` per path, `[SUCC]` `det`;
- the Little residual (W − Σrᵢ·rttᵢ)/W;
- duplicates, retransmits/GB, client and server CPU/GB;
- the realised cap and the bind fraction.

**Readout per cell (new): the standing-queue time at the knee.**
- **Definitions:**
  - T̄(W) = the rate-weighted LATE echo loop Σrᵢ·rttᵢ/Σrᵢ.
  - T₀ = the same statistic over the first 1 s of ticks of the cell's smallest-W rows (no queue).
  - ℓ = the rate-weighted boot `rtp_us`.
- **Readout:** from the arm medians, interpolated in W, the W where goodput first reaches 0.95 and 0.98 of the
  control. At those two points, report T̄ − T₀ in ms and as a multiple of ℓ.
- **Why.** The derived law puts the standing queue at (b̂₀⁺ − b₀) + q(δ)·ℓ.
  - The estimator slack (0.4–3.7 ms) already exceeds δ's whole term (q·ℓ ≈ 0.12–4 ms).
  - The static-map points move only 174 → 192 (c2) and 370 → 509 (c1s) from Realtime to Bulk.
  - So δ has almost no authority. That is U2, which goes to the owner with this readout after Part A.

**Predictions** (bands; these are static-cap measurements, so registrable here).

c8, every control band from the §54b **B640** rows (`docs/l1-raw/s54/rep54b-*`, main's 20-run horizon;
`little54b.out`):

| c8 W | 1 280 (control) | 768 | 512 | 320 | 224 | 160 |
|---|---|---|---|---|---|---|
| Σ send rate /s | 11 629–11 732 (B640) | 11 400–11 650 | 11 050–11 600 | 10 300–10 900 | 9 700–10 100 | ≤ 9 300 |
| r₀ /s | 9 776–9 840 | ≥ 9 750 | ≥ 9 700 | ≥ 9 600 | 9 400–9 800 | ≤ 9 300 |
| r₁ /s | 1 839–1 913 | 1 650–1 900 | 1 400–1 800 | ≤ 1 000 | ≤ 400 | ≤ 300 |
| T̄ ms (≈ W/Σr) | 103–107 | 60–70 | 42–48 | 28–32 | ≈ 22 | ≈ 17 |
| rtt₀ / rtt₁ ms | 96 / 138–161 | — | — | — | — | — |
| goodput Mbit/s | 106.5–107.6 | — | — | — | — | — |
| in-order late mean s | 0.34–0.39 | 0.18–0.26 | 0.13–0.18 | 0.08–0.12 | 0.06–0.08 | — |
| old holes | 0 | 0 | 0 | 0 | 0 | 0 |

The c8 standing-queue readout is predicted to sit at W ≈ 500–700 (leg-1 shoulder), with T̄ − T₀ ≈ 25–35 ms
(the coupled floor, §5.2).

- **c2:** goodput ≥ 0.97 × control at W ≥ 192; 0.88–0.95 at 160; 0.72–0.82 at 128. T̄ ≈ W/9.6 k.
- **c1s:** goodput / control ≈ 0.94 (1 024), 0.92 (768), 0.84 (512), 0.80 (384), ± 0.05 (p34 era). Flagged rows
  are excluded.
- **Anchor:** `rtp_us` on c8 p0 falls toward boot as W shrinks, ≤ boot + 10 ms at W ≤ 320. On leg 1 it stays within
  boot + 8 ms at every W.

**Re-basing (control drift).**
- The c8 control is compared with B640 by **medians**, not by six-row min–max (that check nearly always fires,
  §54 launch 2).
- A readout is re-based iff |control median − B640 median| > tol, where tol = the B640 rows' full range for that
  readout, floored at 1 % of the B640 median. For example: Σ send rate tol = 103 /s; r₁ tol = 74 /s; in-order
  late tol = 0.05 s.
- If a readout re-bases, that readout's bands at the other W shift by the same median offset, and the shift is
  reported.
- The c2 and c1s controls have no prior bands. They set their cell's reference directly.

**Falsifiers (reported; each changes the derivation, not a ship decision):**

| # | what fires it | what it means |
|---|---|---|
| F1 | c8 r₁ at W = 320 ≥ 0.8 × r₁ at 1 280 | leg 1 is fed without a fast-leg queue; the coupled F is not needed at c8 |
| F2 | c8 Σ rate at 768 < 0.97 × control | the knee is above 768 |
| F3 | c8 r₀ < 0.97 × 9 800 at W = 224 | the leg-0 shoulder is above prediction |
| F4 | c8 `rtp0` at W ≤ 320 > boot + 10 ms | E03's queue persists even at small W; the bound m̃ is load-bearing at c8 |
| F5 | \|W − Σrᵢ·rttᵢ − old holes\| > 0.1·W at any bound W ≥ 320 | Little fails |
| F6 | old holes > 50 at any W ≤ 1 280 | P_h is lower than the derivation says |
| F7 | c2 goodput median at 192 < 0.95 × control | the c2 knee moved up in this era |
| F8 | c1s goodput median at 512 ≥ 0.95 × control, over unflagged rows of n = 6 (excluded count stated) | the c1s cost in the derivation is overstated |
| F9 | the SUCC_DUMP row shows p0 holes overtaken by later-placed p1 seqs | the hn_n anomaly is a real delay inversion, not a counter rule |
| F10 | the standing-queue multiple (T̄ − T₀)/ℓ at the 0.95 point differs across c1s, c2 and c8 by more than ×3 (max/min) | **the q·ℓ form is wrong**, not just its band: the knee's queue does not scale with the lifetime loop |

**Decision rule.** None; reported only. The maps replace p27/p34 as the inputs of Part B. F9's reading goes to the
guard-witness note (§6). F10 and the standing-queue readout go to the owner with U2.

**§56 Amendment 1: the harness as built, with ledger checks and deviations (before any VM contact).** The harness is `harness56` (`8157cbf2`, tools only), merged to main. `git diff` against the registration commit touches no file under `raptorpath/` and no `Cargo.*`.

*Ledger checks on committed rows (host):*
- **c2 is single-path.** All 211 `[DIAG]` ticks of the §54 guard c2 rows read `np=1 np_act=1`, so at c2 W = pool. (My plain-language summaries to the owner called c2 two-path. That was wrong.)
- **c2 is pinned at the shipped pool,** contrary to §56's "c2 not pinned at 2048".
  - The late `win=…/b` is 2 048 on 100 % of ticks in 24 of 26 rows (the other two: at least 92 %).
  - `[SUMCAP]` pin is 0.66–0.81, and the ask is 8 500–13 300.
  - This is a law-ledger note for F01, which is pinned at c2 as well as c8 and c1s.
  - The control exemption from the bind check stays: the controls are the shipped behaviour.
- **Every non-control pool binds** (proxy: the shipped pool's late cap as a lower bound on the ask):
  - c2 asks 8 500–13 300;
  - c1s 5 GB caps at 2 048 on every late tick, asking 3 200–8 000;
  - c8 B2048 sits at 4 096 on 99–100 % of ticks.
- **The live path count equals N on every late tick** of all committed rows read (the B640 and B2048 rows, 24 c2 rows, 24 c1s rows).

*Deviations from §56 (each decided here):*
1. **T₀ is redefined.** The first 1 s carries start-up queue: 62–103 ms at c8 against ℓ ≈ 15 ms, with a spread of 41 ms.
   - floorᵢ is the lowest arm-median LATE echo loop of leg i over the cell's arms.
   - An arm's T₀ is Σrᵢ·floorᵢ / Σrᵢ, with that arm's own rates, so q = T̄ − T₀ = Σrᵢ(rttᵢ − floorᵢ)/Σrᵢ ≥ 0 at every arm by construction.
   - A fixed knee-weighted T₀ was tested and rejected: on p27 it reads T₀ > T̄ at c2 cap 64 and c8 caps 128/192, because leg 1's share collapses.
   - On p27 the new q reads ≈ 3.8 ms at the c2 knee and 25 ms at c8 W 512, with a seed spread ≤ 0.5 ms.
   - F10 is UNSCOREABLE when q at the 0.95 point is not above the spread of T̄ over the smallest-W arm's rows.
   - T̄/ℓ is printed per arm, and it needs no T₀.
2. **Bind** is judged per tick against that tick's live path count × pool, not a fixed N·pool. The scorer prints `nlive_med` and `nlive_lt_share`.
3. **F8** is UNSCOREABLE unless each n = 6 arm has at least 4 unflagged rows. Collapse rows, whether they finish or DNF, are flagged and excluded, with the counts printed.
4. **F9 cannot be scored by this instrument.** The dump records (outcome, µs) with no path and no sequence number, so the dump row settles nothing.
   - The row is kept for its size and the `[HOLD]`/`[SUCC]` path-split counters (p0 `xp_frac` < 0.1 on the B640 row: a pointer, not a settlement).
   - The path and sequence fields go to Part B's gauge build. This block gets no engine change.
5. **Single seat:** a candidate plan with no BASE tree.
6. **`RWM_RTOBS=1`** is kept from 54b's instrument env, so the c8 control stays comparable to B640. The CPU readouts carry it.
7. **The smoke has 4 rows:** one per cell at its smallest pool, plus the dump row, which needs only ≥ 1 record. The 100 000 rule applies to the scored row.
8. **The Σ-rate re-basing tolerance is 117 /s** (B640 range 103, floored at 1 % of the median), not the quoted 103.
9. **c2 P128 has W = 128,** equal to the boot cap, not above it. Nothing inverts.
10. **F5 and F6 are read on arm medians.** UNBOUND rows stay in the tables at their realised cap and are excluded from the falsifiers.
11. **Length and budget.**
    - From real row durations, the c1s block is ≈ 41 min (54 min worst case), not 22.
    - The session takes ≈ 1 h 40 with suite reuse (2 h worst case) and ≈ 2 h 25 without (2 h 50 worst case).
    - **`LD_HARD` = launch + 4 h.** The run refuses with `ABORT-BUDGET` before any lock if less than 3 h remain.
12. **Suite reuse:** a per-file sha256 manifest of `raptorpath/src` and `raptorpath/tests` against `ref56-manifest.sha256`, generated at `cand54` `7fd92dd5`. It is byte-identical to main's.
    - On a hit, the engine suites are skipped, and the run writes `TOOLS-TESTS-OK`, never `TESTS-OK`.
    - The tools tests (all python plus §54's two bash tests) always run. `LD_PRIOR_TESTS` is refused.

*Test evidence (worker, `8157cbf2`):*
- `STUBRUN_FULL=1` unfiltered, all plans: ALL OK, 325 checks, including the 56 paths (rehearse, full, reuse-hit, reuse-miss, replacements, the budget refusal) and `HARNESS_PATHS` equal to the executed set.
- All 36 `tools/l1` python tests pass. `test_pool56` covers VOID, UNBOUND, DNF, flag exclusion, re-basing, F1–F10 at their boundaries, the T₀ floors and the q ≥ 0 invariant.
- Calibration: the scorer reproduces the §54b B640 ranges from `rep54b-diag-ld.tgz`.
- The bash tests run on the VM (pre-flight).

*Pre-flight additions:*
- the two bash tests;
- `[GATES]` printing `RWM_SUCC_DUMP`;
- `/dev/shm` ≥ 1 GB free;
- the release binary's sha against §54's CAND `8f313616…`. The engine is the same, so a match ties the c8 control to B640; a difference is explained before any control drift is read.

**§56 Amendment 2: rehearsal 1, ABORT-SMOKE on c1s, and the collapse flag (POST HOC, decided after seeing one smoke row; the predictions stay as registered).** Rehearsal 1 ran on 2026-11-14 at 06:21–06:42Z on main `558b7f87`. Ledger: `docs/l1-raw/s56/rehearsal1.tgz`.

*The harness held:*
- 4/4 smoke rows LIVE, and every witness passed.
- `bin_sha` `8f313616…` = §54's CAND (same engine), so the c8 control is tied to B640. P640D lands inside every B640 band (one row).
- Suite reuse HIT (182 files). The two bash tests ran: 43 `ok`, abort-witness 12 passed. The python tests are green.
- The dump row wrote 188 281 records (1.5 MB).

*What failed: c1s P384 DNF at 300 s.* This was a quinn window collapse (§47 shape), not a store-map point:
- `qcwnd` sat below the 250 kB BDP in 94 % of windows (18.6 kB late);
- 372 of the 384 slots sat in quinn's send buffer;
- the wire RTT was 2 ms against a 69 ms echo, and the Little residual −1.02.

The registered flag, `qcwnd` below ¼ of the row median, read 0.0 on it and 0.19–0.24 on the healthy c8/c2 rows. It is inverted for a whole-row collapse.

*Changes (harness56 rev 2, `12e362d9`, merged):*
1. **Collapse flag.** Now the share of `[DIAG]` windows (t ≥ 1 s) with leg-0 `qcwnd` below the leg's BDP, above 0.10, at c1s.
   - Validated on committed 5 GB rows: healthy §48b rows read 0.006–0.023; the four collapsed BASE rows read 0.58–0.94. §49's 148.5 Mbit/s row reads 0.89, and p27 DEF192 0.996.
   - On 400 MB rows, start-up windows dilute the share, so the 0.10 threshold holds for 5 GB rows only.
2. **Flagged rows enter no arm,** no median and no falsifier (F5/F6 included). They are reported in a collapse table per pool, with onset time (the first 4 s window that stays below BDP) and a pre-onset map point (send rate, T̄).
3. **Quinn's backlog as a share of W,** per leg, on every row: bind alone does not show who limits.
4. **Smoke:** a DNF on a flagged row is not a smoke failure. An unflagged DNF still is.
5. **c1s P2048 and P512 run at n = 9,** and F8 needs ≥ 5 unflagged rows per arm.
   - Stock quinn collapsed in 4 of 12 BASE c1s 5 GB rows at the shipped pool in §48b (`bbr_rs` 0/12), so P(F8 UNSCOREABLE) ≈ 0.15 per arm.
   - The block is now 70 slots (c1s 27).
6. **`LD_HARD` = launch + 4.5 h.** The worst case with every non-control c1s row at 300 s is ≈ 3 h 18 on a reuse hit; typical is ≈ 2 h 05.

*What the flag says about the derivation's c1s inputs:*
- **p34:** only DEF384 s42 r1 collapsed (0.81). The DEF256 rows read 0.00–0.095, so the p34 c1s cost is a store-cap cost, not a substrate artefact.
- **p27:** DEF192 collapsed. **§43:** SL s42 r2 collapsed.
- **Onset mechanism** (BASE §48b s42 r3, `[DIAG]` only; main prints no `[QCC]`): qcwnd ≈ 2 × the delivered rate × the 2 ms wire min-RTT. That is a falling bandwidth estimate holding the rate its own window allows, a low fixed point. A latched-low min-RTT is not excluded; quinn's own `rtt.min` is not visible.
- Whether a small W raises the collapse rate is a reported c1s readout, n = 1 today, carried to Part B and K09.

*Weakened:* Amendment 1's `xp_frac` pointer for F9. On the dump row `[HOLD]` p0 reads 0.015 and `[SUCC]` 0.97, over different populations. It settles nothing.

Rehearsal 2 runs on the merged tree before the full block.

**Erratum to §54 Amendment 1 (test evidence).** "`STUBRUN_FULL=1` … and the 54b DNF case" was wrong for full mode. On a clean `906eed2c` the full-mode `dnf54b` stub test fails (`dnf=3`): the registered design has three seed-7 B640 rows, and the test expected one. Only fast mode passed. The test now expects 3 in full mode and 1 in fast mode. §54's real rows had 0 DNF, so no result changes.

## 55. The law ledger — every shipped law and estimator audited — record, no VM

**What.** One row per law and per estimator that feeds a law: 70 rows (12 estimators, 6 δ maps, 6 flow-control, 9 rate, 10 span/shed/emission, 13 recovery, 10 scheduler/CC, 4 wasm). Each row records:
- the formula as the code computes it;
- whether the paper carries it in the same shape;
- the derivation and the provenance of each symbol;
- whether it was measured in its operating regime;
- the clamp bind fraction;
- whether its inputs are honest;
- any mode switch;
- a verdict and a ranked fix priority.

Evidence: `docs/derivations/law-ledger.md` / `.yaml` (branch `law-ledger` `4f84ce63`; criteria and the finished ledger Fable-reviewed).

**Verdicts (a row can carry several):** OK 13, DIVERGENT 17, UNDERIVED 30, PINNED 4, DEGENERATE 9, DISHONEST-INPUT 17, MODE-SWITCH 8.
- 9 of the 13 OK rows were never measured in their regime.
- Priorities: P0 5, P1 10, P2 19, P3 36.
- The PINNED threshold (bind fraction ≥ 0.5 at a measured cell) is the ledger's declared number; discipline 18 names none.

**Fix order (top, correctness first):**
1. **`max_bw` (E04), 11–24× the link at c8.** An open input design: on a store-bound sender every delivered-rate sampler reads the cap's own throughput, and every candidate so far failed (§23, §24, §27, §34, §36, §40, §43b).
2. **Copa `min_rtt` (E03), 7–33× propagation.** It reads the sender's own queue (§3.10 item 9). Quinn's wire `rtt.min()` and `bbr_rs`'s m̂ are untested as the store's RTprop.
3. **The store cap (F01).** PINNED at c8 (0.74–0.98) and c1s (≈ 0.7). It ships with items 1–2: with honest inputs the shipped law collapses to its floor at c2.
4. **The packet threshold plus the 20-gap horizon (C02).** At c8 it sees holes only at ≈ 3.5 s age. It ships with the snapshot-order law (C03).
5. **Then:** quinn BBR as the default substrate (K09, §47), the app-echo RTT feeding every recovery clock (E01), the pooled recovery srtt (E11), the receiver srtt seed (E10, which closes §3.10 item 6's trace: the 50 ms is the `LossEstimator` constructor seed), and the hole refresh (C07).

**Findings new to the record:**
- **Single-path loss detection is the pre-RFC `srtt/2` age gate** (C04): `legacy_age_ripe`, `net/sender_phases.rs:964`, whenever `RWM_RECOV_SP` is off and N ≤ 1. This is a law step at N = 1 → 2.
- **The default-path NACK repair throttle** (`NackCongestionState`, `net/mod.rs:256`; at most 10 retransmits per gap report, seven constants) has no paper entry and no gauge (C09).
- **The rate mix r(β) has run only on its r ≡ 0 corner:** every L1 cell runs the Bulk hint with χ ≡ 0 on the tunnel (R01). The default hint, Auto, was never measured. *Reconcile before citing:* §53's c8 rows carry a coded share of ≈ 0.5 %, so repair symbols come from a source other than the proactive rate. Which one is open.
- **Stale texts:**
  - the `pooled_store_cap` doc comment ("BDP + one recovery round of runway"; the code computes q·RTprop);
  - paper §11.2 ("knee inert at c7/c8"; it is pinned);
  - Appendix A.4's placement E_i line and A.2's taper density (superseded forms).

**Consequences.** The tech tree's forward order (`docs/roadmap.md`) takes the ledger's rank. Every row whose verdict is not OK maps to one fix node. Item 1 is an open research problem, not an implementation task, and its node carries a fallback.

## 54. The snapshot-order law against main (guard v2, scored) and the store-budget test (reported) — pre-registration (before any VM contact)

**Why.**
- **Guard:** §53 confirmed that stale gap reports drive spurious retransmits, and that judging only the newest snapshot removes them (CANDS duplicates 1 291 against CAND's 4 004). On main, the filter removes ≈ 1 160 stale-judged fires per c8 run (BASES).
  - `snapver-law` makes the filter the shipped law: paper §7.1 first, no env arm (`docs/derivations/dup-diag.md`).
  - Per the owner-approved order of 2026-11-13, it merges first and alone, behind the first guard v2 (§41d).
- **Reported block:** §53 Erratum 1 found that lever B moves ≈ 1 400 store slots from old holes into quinn's p0 send buffer (+150–230 ms p0 first arrival; in-order delay ≈ 5.3 → 1.0 s).
  - The store-budget block tests that mechanism by intervention, with the discriminating row on BASE (`docs/derivations/p0-dwell-diag.md` §8, Fable-reviewed).

**Trees** (SHAs pinned by amendment after the VM pre-flight):
- **BASE54** = main at launch (with guard v2 and the merged tools).
- **CAND54** = BASE54 + `snapver-law` (head 8191b3de; engine diff = the law, its seam, the `[SNAPV]` gauge and tests).
- **LEVB54** = CAND54 + `gaphorizon` (lever B) + the horizon-keyed `recovery_bench` fixture (cherry-pick of `b21421e6`; without it the suite fails as `cand53`'s did). Used only by the reported block. It is not a merge candidate here.
- **Two launches, because the harness has two tree seats:**
  - launch 1 (`LD_PLAN=54`): base BASE54, candidate CAND54 — the guard;
  - launch 2 (`LD_PLAN=54b`): base CAND54, candidate LEVB54 — the block's B and L arms.
- **Prerequisite:** plan 53's `[SNAPV]` parser and scorer (`snap53_score.py`, on unmerged `cand53`) are ported to main for both plans.

**Session (`LD_PLAN=54`).**
1. Suites on CAND54.
2. **The guard, SCORED:** plan 41d's design (§41d, v2: 96 rows; c1s/c1d at guard size; c2/c8 at 600 MB; in-order delay mean scored at c2/c8; the start-up floor reported only). BASE54 vs CAND54, rank-minp, PASS iff p > 0.05.
3. **The store-budget block, REPORTED ONLY:** c8-100 at 600 MB, `RWM_DIAG=1`, n = 6 per arm, rotated, no replacement, 36 rows.

| arm | binary | `RWM_STORE_PATH_POOL` | ceiling W |
|---|---|---|---|
| B2048 | CAND54 (main + law) | default 2048 | 4 096 (control) |
| B1024 | CAND54 | 1024 | 2 048 |
| **B640** | CAND54 | 640 | **1 280 (the discriminating row)** |
| L2048 | LEVB54 | default | 4 096 (control) |
| L1024 | LEVB54 | 1024 | 2 048 |
| L640 | LEVB54 | 640 | 1 280 |

- "BASE′" in the design is CAND54 here: main + the law, so both block binaries carry the filter.
- **The knob:** `RWM_STORE_PATH_POOL` exists (`gates.rs:323`, default 2048) and is in the `[GATES]` echo and the `lib.sh` forward list.
  - The battery must add `-u RWM_STORE_PATH_POOL` to its env scrub; it is missing on main.
  - Liveness compares the `[GATES]` echo and `pool_per_path` with the requested value (a garbled value silently falls back to 2048).

**Guard predictions.**
- **PASS.** The law is inert when reports arrive in order (c1s, c1d mostly, c2). At c8 it removes stale-judged fires (BASES vs BASE in §53: duplicates, retransmits/GB, goodput and tail inside BASE's range).
- **Registered risk (from §53's 600 MB rows, BASES vs BASE, n = 6 + 6):**
  - BASES leaned worse on what v2 scores at c8: goodput median −2.0 % (p = 0.33), tail 3.95 → 4.42 s, H0 1 743 → 1 835.
  - v2's c8 goodput resolution is about 2 % (§41d), so a c8 goodput FAIL is possible.
  - The v2 delay clause, computed post hoc on those logs (whole-run mean, `inorder_delay_stats`): BASE 4.41 → BASES 4.50 s median (+2 %, p = 0.29); CAND 0.96 → CANDS 1.02 s (+6 %, p = 0.22). Both are inside the clause's 20 % c8 resolution.
- **What a c8 FAIL would mean:** the filter alone costs main. Then the law and lever B go to the owner as ONE candidate, and the law does not merge alone.
- **Outcome witnesses** (there is no env arm, main has no `[SNAPV]` gauge, and no test executes `serve_gaps`):
  - each row's `bin_sha` matches its arm's build;
  - at c8, p0 `[HOLD] hn` collapses on CAND54 against BASE54 (§53: 11–409 with the filter, 9 557–10 659 without);
  - `[SNAPV]` `pt_st + tt_st = 0` on every CAND54 row (by construction; it shows the gauge is present).
  - A CAND54 c8 row without the `hn` collapse is VOID.

**Block predictions** (late window: sending ticks at t ≥ 15 s; two-value cells read H-hold / H-elastic; from `p0-dwell-diag.md` §8):

| readout | B2048 | B1024 | **B640** | L2048 | L1024 | L640 |
|---|---|---|---|---|---|---|
| goodput Mbit/s | 96–103 | 75–103 / 96–109 | **20–85 / ≥ 95** | 107–109 | 105–109 | 104–109 |
| p0 send backlog (`dgq0` hand − tx) | 300–1 600 | 0–600 / 100–1 000 | 0–200 / 0–400 | 900–2 700 | 100–1 000 | 0–400 |
| p0 app-echo RTT ms | 180–215 | 30–180 / 125–225 | 12–90 / 70–150 | 300–355 | 125–225 | 70–150 |
| old holes | 1.0–1.55 k | 0.5–1.6 k / ≤ 300 | **≥ 700 / ≤ 300** | 0–10 | 0–10 | 0–10 |
| tail s | 2.6–5.5 | 2–6 / ≤ 1 | 1.5–7 / ≤ 1 | 0.2–0.7 | 0.1–0.7 | 0.1–0.7 |
| quinn evictions (`dgq0 full`) | 0 | 0 | 0 | 0–350 | 0 | 0 |
| in-order delay, LATE-window mean s (`inorder_mean_late`) | 4.8–5.6 | ≥ 2.0 / ≤ 1.2 | **≥ 2.0 / ≤ 1.0** | 0.9–1.4 | 0.5–1.0 | 0.4–0.9 |

**Falsifiers (block):**
1. B640 goodput ≥ 0.95 × B2048 while its old holes stay ≥ 700: the mechanism is refuted.
2. B640 goodput ≥ 0.95 × B2048 with old holes ≤ 300: H-elastic, reported as a finding (lever B's gain would come from the oversized pool, which changes the merge argument).
3. L640 goodput < 0.97 × L2048: CAND's edge is above 1 280; read L1024 alone.
4. A budget residual > 0.1·W on any LEVB row: the identity fails.
- (The design's fifth falsifier is dropped: its direction differs between H-hold and H-elastic.)
- **Re-basing:** if a control row (B2048 or L2048) lands outside its §53 reference range (BASES or CANDS), the reduced-W bands are re-based on the new control before reading, and that is reported.

**Readouts (block):** goodput, tail, H0, receiver duplicates, retransmits/GB, CPU/GB, and per path:
- quinn evictions (`full`) and `hand − tx` from the last `[DIAG]`;
- in-order delay, two named statistics:
  - `inorder_mean_run`: the scored clause's whole-run mean (`threadp1_parse.inorder_delay_stats`);
  - `inorder_mean_late`: the block's late-window mean (sending ticks at t ≥ 15 s; `p0-dwell-diag/inorder.py`'s PIN-phase definition), plus p50, p95 and `lag_end`;
- old holes (server `[RANK]` holes − `tail_overcount`; readouts ≥ 15, excluding the last 6);
- the budget residual (each tick's own cap; per-path fields anchored on `pN:infl=`);
- the cap bind fraction (reported, never voiding);
- `[SNAPV]`.

**Decision rule.**
- **Guard PASS:** `snapver-law` merges to main (`--no-ff`). Its `[SNAPV]` gauge and paper §7.1 ship with it.
- **Guard FAIL:** no merge; the failing clause and the resolution table go to the owner.
- **The block decides nothing by itself.** Its outcome (mechanism confirmed / H-elastic / refuted) goes into the lever-B guard's registration and the store-law fix node (status §55 item F01).
- NO VERDICT ⇒ re-run. ABORT-TESTS ⇒ fixed on the branch and re-run.

**VM pre-flight (same session, before the pin; fresh target per tree):**
- `cargo check --all-targets`; lib `net::` and `gates`;
- the integration tests `holeclass_reachability`, `fcause_reachability`, `gauge_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability`, run on every tree (`snapver-law`'s were compiled but not run);
- **carried over from §53 Erratum 2:** `build_visualizer.sh` on a fresh wasm build, the native golden test without `GOLDEN_CAPTURE`, and the `bound-tests` wasm pool test;
- `df` on the VM. §52's directory took 20 GB, and this session builds three trees. s52, r52, post52, p53, r53 and s53 are removed after their ledgers are confirmed in `docs/l1-raw/`.
- **Expected length:** three release builds (≈ 6 min each), the suites (≈ 50 min), the 41d guard (≈ 48 min of rows plus overheads) and 36 block rows (≈ 1 h). Total ≈ 3.5 h, so `LD_HARD` = 5 h per launch.

**Rehearsal (`LD_REHEARSE=1`) must show:**
- the guard smoke rows (41d's two plus the 600 MB c8 row) complete, with no `VOID-SHORT`;
- one row per block arm with its pool in `[GATES]` and `pool_per_path` on both ends;
- `[SNAPV]` parsed, and the in-order delay, evictions, old holes and budget residual computed with no None;
- the c8 600 MB rows inside `RWM_PERF_TIMEOUT_S=200`.

**§55 Erratum 1 (2026-11-13, the recovery-clock derivation `wire-clock-derive` `17bfcefb`, `docs/derivations/wire-clock.md`; Fable-reviewed; rulings by the main session with Fable).**
- **The ledger's item 7 is withdrawn for the recovery clocks.** It proposed swapping the app-echo RTT (E01) for a wire RTT.
  - The time threshold (C01) and the cooldown (C05) compare a flight's age from the placement stamp. What they need is the per-path loop from placement to the gap report that would list the hole, and the app-echo estimates exactly that.
  - Checked in code: the echo key is the flight stamp (`emit_source.rs:384–391` passes `src_send_ts_us` to `send_symbol`, the same value stored in `retransmit_buffer` at `:457`), so age and clock share an origin.
  - Two cells reject a wire clock post hoc. At c1d the loop is ≈ 19–23 ms on a 2–3 ms wire RTT, so a wire threshold would sit at its floor and ripen nearly every self-healing hole. At c2 (§48b), `bbr_rs` drops the wire RTT 5× while the loop is unchanged.
- **E01's verdict, split by consumer:**
  - for the recovery clocks: consistent with its age origin, but inflated by the store defect, because the loop contains the sender's standing queue. It depends on the store fix (F01/E03/E04);
  - for the store law and the other RTprop consumers (E03, S01, E12, R07): still dishonest input.
- **The recovery-clock fix narrows to:**
  - **E11:** each seq's clock reads its own leg, not the pooled max over legs;
  - **C04:** one law at every path count, removing the N = 1 pre-RFC age gate, with "branch A" (hand the SACK ranges to the packet-threshold evidence ingest, `control_msg.rs:728`, so the fast channel executes at N = 1);
  - **C05:** the cooldown is KEPT for one measured session, re-keyed per leg, with a bind counter predicted 0 and a host test that a seq with no flight record must not re-fire on every report. It is deleted when the counter reads 0 on the VM.
- **Left out of the first change:**
  - replacing the second EWMA with the latest sample (the logs cannot evaluate it; it belongs with the per-leg state);
  - the receiver refresh (at c1 its 25 ms floor would bind and move the refresh from 100 to 25 ms where the CPU clause is tightest);
  - C09 (it needs its gauge first).
  - The 10 ms floor is kept; the derived 1 ms is its own arm later.
- **Order:** the snapshot-order law (§54) goes first. This change is based on `cand54`, because both change the gap-batch tuple.
- **Owed before registration:** the paper text first (§7.1, A.6, §11.2; drafted in the derivation's §8), a c2 goodput prediction (v2 resolves 1 % there) with branch B as the falsifier, and `fast/fired` at N = 1 as a readout.
- **Consequence for c8:** the recovery lever there is the dwell, i.e. the store fix and whether retransmits queue behind source in quinn's per-path send buffer (plan item 6d, unanswered).

**§54 Amendment 1 — harness as built, candidate heads, five deviations (before any row runs). The SHAs are pinned by Amendment 2 after a green VM pre-flight.**

*Candidate trees* (heads on `github`; the VM pre-flight runs on exactly these):
- **BASE54** = main `a1908f61`. It includes `harness54` (`0eb3892b`, tools and `.gitattributes` only), merged so that the registered prerequisite "the plan-53 parser and scorer ported to main" holds. `git diff 7968c870 BASE54 -- raptorpath Cargo.lock Cargo.toml` is empty. Main may later gain docs- or roadmap-only commits (`8a937b18`, the roadmap layout); the seat stays at `a1908f61`.
- **CAND54** = `cand54` `7fd92dd5`: the pin `7968c870`, plus `snapver-law` (`8191b3de`, `--no-ff`), plus `harness54`.
  - Engine diff against BASE54 (`raptorpath/src`, `raptorpath/tests`): 8 files, +637/−56. That is the law, its seam, the `[SNAPV]` gauge and tests.
- **LEVB54** = `levb54` `2ab104f4`: CAND54's law merge, plus `gaphorizon` (`--no-ff`), plus the cherry-pick of `b21421e6`, plus `harness54`.
  - Engine diff against CAND54: 4 files, +110/−21 (lever B and the `recovery_bench` fixture).
- `git diff cand54 levb54 -- tools .gitattributes` is empty: both launches run one harness.
- Host checks, run on the engine trees before the harness merges (the merges touch only `tools/` and `.gitattributes`, so the engine is unchanged since):
  - `cargo check --all-targets` is clean on both trees;
  - `--lib net:: gates`: 308 passed on CAND54, 310 on LEVB54;
  - `recovery_bench` on LEVB54: 7 passed, 1 ignored.

*Harness as built (`tools/l1`, stub-tested):*
- **Plan 54 is plan 41d unchanged.** `run_all` maps `LD_PLAN=54` to `41d` before any branch, so the scored rows, order, sizes, timeouts and scoring are 41d's (`score54` prints `score41d`'s output byte for byte, and a stub test asserts it).
  - The three outcome witnesses run behind `LD_WITNESS54=1`.
  - Re-run and NO VERDICT follow 41d's existing replacement rule (`place38_parse.py` `MAX_REPLACEMENTS = 4`, status §47); nothing new.
- **Plan 54b:** 36 rows (B arms on the BASE seat = CAND54, L arms on the CAND seat = LEVB54), a Latin-square rotation per group, a 6-row smoke.
  - `-u RWM_STORE_PATH_POOL` is in the env scrub, and the knob is forwarded to both ends.
  - Scorer: `pool54_score.py`, calibrated on the §53 logs. With the reference selector it reproduces the registered BASES old holes 1 089–1 510 and residual 1 257–1 733 exactly. Whole-run in-order medians: 4.54 s (BASES) and 1.03 s (CANDS), against the registered 4.50 / 1.02.

- **Binary pre-check (an abort path the registration does not name):** before any row, the battery checks the built binaries for the `[SNAPV]` gauge. For plan 54 it must be present in CAND and absent in BASE; for 54b it must be present in both seats.
- **`[SNAPV]` witness coverage** (the line prints only if a gap report was counted; a CAND row without it is VOID):
  - the rehearsal's smoke rows are c1s BASE, c1s CAND and c8 CAND (`place38_parse.py plan41d smoke`);
  - c1d: every one of §53's 30 c1d rows (`rep53-diag-ld.tgz`: 12 BASE, 12 CAND, 6 CANDS; all filter-off except CANDS, so `pt_st`/`tt_st` are non-zero there) prints the line, with thousands of reports per path;
  - c2: §52's 24 c2 guard rows (`s52/guard-diag-ld.tgz`, pre-gauge builds) count 1 820–2 767 gap reports each (`mpr[rep=`).

*Deviations from the registration (each decided here, before VM contact):*
1. **Plan 54b timeout `RWM_PERF_TIMEOUT_S=300`** (plan 54 stays at 200).
   - Why: under H-hold, B640's registered floor of 20 Mbit/s needs 240 s for 600 MB, so the discriminating row would be censored in exactly the outcome it tests.
   - Cost: about +10 min worst case.
   - A DNF still counts as goodput 0 in the falsifier medians. Its old holes, in-order delay, residual and q0 are still computed from its `[DIAG]` lines (stub-tested).
   - The rehearsal criterion is now "the c8 600 MB rows finish inside their plan's timeout".
2. **The hn witness threshold is `hn_n` < 2 000** on path 0. The registration names the ranges (11–409 with the filter, 9 557–10 659 without) but no threshold; 2 000 sits between them.
   - It applies to plan 54's CAND c8 rows only. It is never applied in 54b: lever-B rows read 47 000–196 000 with the filter on.
3. **Witnesses are never applied to a DNF row**, like v2's VOID-SHORT.
   - A witness that cannot run (the seat's sha did not reach the parser) is a harness fault, not a pass.
   - A row failing a witness is VOID and is replaced under 41d's rule.
4. **§8.5 liveness conditions not in §54** (p1 `stale>0`; the tail witness at W = 4 096) are printed per row and never void.
5. **The §53 re-basing check runs on the host.** `docs/l1-raw/s53/rep53-diag-ld.tgz` (9 MB) stays out of the VM archive. On the VM, `score54b` prints `REBASE-CHECK UNAVAILABLE`, and the check is run on the host on the copied ledger (`pool54_score.py score <ledger> --s53 …`) before the block is read.

*Also fixed:* `.gitattributes` now ships `docs/l1-raw/s52/guard-ld.log` and `bbr48b/run/bbr48/ld.log`, about 0.7 MB in all. Without them, `test_guard_v2.py` fails in any extracted archive, the pristine pin included, so 41d's and 54's parser gate would have hit ABORT-TESTS on the VM.

*Test evidence:*
- worker, at heads `0eb3892b`/`7fd92dd5`/`2ab104f4`: `STUBRUN_FULL=1` 54/full and 54b/full ALL OK, and the 54b DNF case. Fast mode (240 checks) ran at the previous heads.
- merged main `a1908f61`: `test_guard_v2` (v1 bit-identical: p 0.2471 / 0.0020), `test_guard_clause_dry`, `test_pool54`, `test_snap53`, `test_harness_paths`, `test_harness46`, `test_harness47`, `test_tail52` and `test_bbr48_parse` ALL OK.
- `git archive levb54` extract (what the VM receives; tools identical on all three trees): all 36 python test files pass (rc 0), including `test_stubrun` fast mode, `test_pool54`, `test_snap53` and `test_guard_v2`; the two bash tests (`test_lib_battery.sh`, `test_abort_witness.sh`) are the known Windows failures and run on the VM.

**§54 Amendment 2 — the pin, after a green VM pre-flight (2026-11-13, 16:51–17:16Z, both locks held).**

| seat | tree | SHA |
|---|---|---|
| launch 1 (`LD_PLAN=54`) BASE | main | `a1908f619f78cd270603719503743b01a2d06c67` |
| launch 1 CAND, launch 2 BASE | `cand54` | `7fd92dd5757b12a68d29400b6f99414e4191f35f` |
| launch 2 (`LD_PLAN=54b`) CAND | `levb54` | `2ab104f4c8f14954a8bfc69f0c9b52ce98a1d5b1` |

Pre-flight (fresh target per tree, `docs/l1-raw/s54/pf54.out`):

| check | BASE | CAND54 | LEVB54 |
|---|---|---|---|
| `cargo check --all-targets` | ok | ok | ok |
| `--lib net:: gates` | 299 | 308 | 310 |
| `holeclass_reachability` | 3 | 3 | 3 |
| `fcause_reachability` | 3 | 3 | 3 |
| `gauge_reachability` | 2 (+1 ignored) | 2 (+1) | 2 (+1) |
| `recovery_bench` | 7 (+1) | 7 (+1) | 7 (+1) |
| `sim_nack_test` | 4 | 4 | 4 |
| `place_arms_reachability` | 3 | 3 | 3 |

All tests passed, with 0 failures. The other checks:
- **Native wasm (BASE, `GOLDEN_CAPTURE` unset):** 36 passed, including `test_multipath_n1_identical_golden` (a sanity check of main) and the `bound-tests` wasm pool test `wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp` (on main; §53 Erratum 2's third item).
- **The bash tool tests on the VM** (CAND54 tree): `test_lib_battery.sh` all pass; `test_abort_witness.sh` 12 passed.
- **§53 Erratum 2's first two items ran on the host,** because the VM has no `wasm-pack`, Node or wasm32 target. Both ran on a fresh build of `wasm-modeswitch-audit` `f2e7b2f6`, the tree they were registered for:
  - `build_visualizer.sh`: rc 0; `test_visualizer.mjs` ("All visualizer engine checks passed") and `test_visualizer_ui.mjs` ("All visualizer UI checks passed"), 76 PASS, 0 FAIL;
  - `cargo test -p raptorpath-wasm` with `GOLDEN_CAPTURE` unset: 36 passed, including `test_multipath_n1_identical_golden` and `test_continuum_one_law_across_the_dial`.
  - §54's engine trees do not touch the visualizer or the wasm crate.
- **VM disk:** 44 GB free before the pre-flight. Only the `target*` build directories of s52, s53, r52, r53, p53 and post52 were removed; no logs, and the s52/s53 ledgers are in `docs/l1-raw/`.

Disk plan: the pre-flight targets are removed; each rehearsal's targets are removed before its full run; launch 1's targets are removed after its ledger copy, before launch 2.

Launch order: launch 1 (`LD_PLAN=54`), rehearsal then full; then launch 2 (`LD_PLAN=54b`), rehearsal then full. Each has its own `LD_ROOT` and fresh targets, with `LD_HARD` = launch + 5 h.

**§54 Result, launch 1 (`LD_PLAN=54`, the guard): GATE-41d PASS, p = 0.6897.** (2026-11-13; launch 17:40Z, guard rows 18:38–19:26Z. Ledger: `docs/l1-raw/s54/`.)

**Integrity.**
- Builds: CAND `8f313616…`, BASE `2053a52c…`.
- Suites passed on CAND54 (`tfail=0`).
- Smoke: SMOKE54, every criterion PASS.
- Rows: 96/96 LIVE. No VOID of any kind, no WITNESS-FAIL, no VOID-SHORT, 0 slots replaced.
- Split check (BASE alone, before CAND was read): every clause's q95 is inside its §21 tolerance except c8 `lag_E_cli` (q95 0.274 against tol 0.264). That clause stays in the scored min-p set (p 0.33), but the guard is blind on it below its q95.
- **Session resolution** (host, `place38_parse.py resolution41d`; d80 = the smallest shift caught in ≥ 80 % of draws):
  - goodput: c1s 5 %, c1d 8 %, c2 1 %, c8 2 %;
  - client CPU/GB: c1s 2.4 %, c1d 8 %, c2 5 %, c8 5 %;
  - in-order delay mean: c2 8 %, c8 20 %;
  - lag: 20–100 %.

**The guard (v2 clauses, CAND against BASE, positive x = CAND worse).**
- **Minimum.** The min-p is 0.066, at c1s `cpu_cli/GB` (x = +0.81 %).
- **Goodput.** No goodput clause moves by more than 1.9 %: c1s +0.17 %, c1d −1.88 %, c2 0.00 %, c8 +0.22 %.
- **In-order delay mean.** c2 +0.34 % (p 0.17); c8 +2.93 % (p 0.43).
- **The registered c8 risk did not materialise.** §53's BASES leaned −2.0 % on c8 goodput; here it is −0.2 % (99.45 against 99.68 Mbit/s), against this session's c8 goodput d80 of 2 %. The c8 in-order delay moves +2.9 %, against the registered post-hoc +2 % (BASES) and this session's d80 of 20 %.
- **Tails.** No row in either arm fell below 0.5 × its cell median. The lowest c8 rows are 96.2 (CAND) and 96.9 (BASE).

**Outcome witnesses (plan 54's, applied to the guard's own rows).**
- **The c8 `hn` witness holds:** every CAND c8 row is below the amended threshold `hn_n` < 2 000. p0 `[HOLD] hn_n` (holes that healed with no retransmit) median at c8: CAND 174 [22–417] against BASE 10 848 [9 147–12 854]. Both ranges overlap the §53 references (11–409 with the filter, 9 557–10 659 without).
- `bin_sha`: all 96 rows `sha_checked` true, each with its arm's build (`BINSHA.txt`).
- `[SNAPV]` `pt_st + tt_st = 0` on all 48 CAND rows; the gauge is absent on all 48 BASE rows.
- **Reported (not a witness):** at c1d, `hn_n` does not collapse (CAND 14 630 against BASE 13 251). The law is not inert at c1d, though: on CAND's 12 c1d rows `[SNAPV]` counts 56–223 stale reports per path per row, all set aside unjudged (`pt_st + tt_st = 0`). The registration's "inert … c1d mostly" is therefore wrong as worded. Why the hn count does not move at c1d is open.

**Decision (registered rule): `snapver-law` merges to main (`--no-ff`)**, with its `[SNAPV]` gauge and paper §7.1. CAND54 was main's tree `7968c870` plus the law plus the harness; main has since gained only `harness54` (identical tools), docs and the roadmap. Check: the merged tree differs from the guarded CAND `7fd92dd5` only in files main gained after `a1908f61` (`docs/status.md`, `docs/roadmap*`, `tools/roadmap/`, `docs/l1-raw/s54/`); the README row conflict was resolved by taking CAND54's resolved file. No new cargo run is needed.

**Launch 2 (54b, the store-budget block, reported only) follows** with its own ledger. It decides nothing by itself.

**§54 Result, launch 2 (`LD_PLAN=54b`, the store-budget block, REPORTED ONLY): H-elastic. Falsifier 2 fires; F1, F3 and F4 do not.** (2026-11-13; rows 20:45–21:14Z. Ledger: `docs/l1-raw/s54/rep54b-*`, `54b-*`.)

**Integrity.**
- Builds: B seat = CAND54 `8f313616…` (the same binary as launch 1's CAND); L seat = LEVB54 `159e4ca9…`.
- Suites passed on LEVB54. Smoke: SMOKE54B, every line PASS.
- 36/36 rows LIVE, 0 DNF. Every row's pool echo matches on both ends.

**Medians per arm** (n = 6 each, c8-100, 600 MB):

| arm | pool | goodput Mbit/s | tail s | in-order run / late s | p0 RTT ms | p0 backlog q0 | old holes | srv CPU s/GB | resid/W |
|---|---|---|---|---|---|---|---|---|---|
| B2048 | 2048 | 98.5 | 4.31 | 4.44 / 5.63 | 198 | 873 | 1 289 | 46.2 | 0.39 |
| B1024 | 1024 | 106.9 | 0.55 | 0.92 / 0.84 | 152 | 153 | 0 | 36.4 | 0.06 |
| **B640** | 640 | **106.9** | **0.12** | **0.38 / 0.35** | **96** | 18 | **0** | **34.8** | 0.04 |
| L2048 | 2048 | 108.0 | 0.33 | 0.90 / 0.93 | 354 | 2 374 | 0 | 36.8 | 0.01 |
| L1024 | 1024 | 107.6 | 0.17 | 0.52 / 0.49 | 162 | 343 | 0 | 36.1 | 0.01 |
| L640 | 640 | 107.3 | 0.12 | 0.32 / 0.30 | 98 | 22 | 0 | 35.3 | 0.02 |

- Every B640 row (106.5–107.6) and every B1024 row (105.3–107.3) is above every B2048 row (95.8–99.2). There is no overlap.
- Quinn evictions occur only on L2048 (median 60, max 460); every other arm has 0.
- Duplicates and retransmits/GB are flat across arms (median duplicates 1 210–1 406; median retransmits/GB 24 756–25 969).
- The knob reaches only the sender's store cap (`sender_phases.rs:118`, `:200`; `mod.rs` echoes it), so the server CPU change is an indirect effect.
- The cap binds on every row of every arm (bind fraction 0.997–1.000).

**Falsifiers.**
- **F2 fires:** B640 goodput 106.9 ≥ 0.95 × B2048 (93.6), with old holes 0 ≤ 300. The registration's reading is H-elastic: lever B's gain comes from the oversized pool.
- **F1 does not fire:** old holes are 0, not ≥ 700.
- **F3 does not fire:** L640 is 107.3 ≥ 0.97 × L2048.
- **F4 does not fire:** no LEVB row has a budget residual above 0.1·W.

**Registered predictions (`p0-dwell-diag.md` §8; for B1024/B640 the H-elastic value applies).**
- Hits: goodput, p0 backlog, p0 RTT, old holes and tail are inside the predicted band on all six arms.
- Misses:
  - in-order late: B2048 5.63 (band 4.8–5.6); L1024 0.49 (band 0.5–1.0); L640 0.30 (band 0.4–0.9), lower than predicted;
  - L2048 evictions: median 60, but rows reach 460 (band 0–350).

**Re-basing (registered; run on the host against §53's logs, `rep54b-rebase.txt`).** The check says YES: some control rows land just outside their §53 ranges.
- B2048:
  - old holes 1 076 against a lower bound of 1 077;
  - goodput 95.8 against 96.3–102.1;
  - tails 5.44 and 5.71 against an upper bound of 5.44.
- L2048: three goodputs and tails at the range edges, and one duplicate count.
- The check compares with the min–max of six rows, so it nearly always fires.
- **The reduced-W bands were not recomputed.** The reading cannot flip: B640's in-order delay (0.34–0.39 s) and old holes (0) sit far inside H-elastic's band (≤ 1.0 s, ≤ 300) and far from H-hold's (≥ 2.0 s, ≥ 700). The falsifiers read the in-run control.

**What it means.**
- **The goodput gain is the completion tail, not a faster sender.** The late-window send rate is 11 690–11 830 /s in all six arms. The B2048 → B640 gap of 98.5 → 106.9 Mbit/s is the 4.2 s tail on a 600 MB transfer. The lasting gains of a smaller W are latency and CPU:
  - in-order delay 4.44 → 0.38 s;
  - p0 RTT 198 → 96 ms;
  - p0 backlog 873 → 18;
  - server CPU −25 % per GB.
- **Two interventions each clear the old holes:** a smaller W (B640) and a longer gap horizon (L2048, lever B). They trade differently:
  - the smaller W also removes quinn's backlog and halves RTT;
  - the longer horizon keeps the highest goodput of all arms (108.0, 1 % above B640), at 354 ms RTT and with some evictions.

  §55 lists both as defects (F01 store cap, C02 horizon); the block shows them as a mismatch between the store's size and the horizon over which recovery can see holes.
- **On the L arms, the send rate falls as W shrinks** (11 826 → 11 745 /s, ranges not overlapping). That is a delay-against-goodput trade, δ's seat on the store law and the owner's open question U3. It is not a free win.
- **Lever B's increment shrinks as W falls, but is not zero:**
  - at 1 024: tail 0.17 against 0.55 s; in-order delay 0.52 against 0.92 s;
  - at 640: in-order delay 0.30–0.35 against 0.36–0.40 s, with no overlap.
- **Scale.** c8's base BDP is 225 000 B, about 190 symbols at 1 200 B (`place46_parse.py`: leg 0 125 000 B, leg 1 100 000 B). The arms' W is about 21.8×, 10.9× and 6.8× that.
- **The clamp binds on every row at every W:** the store law is pinned at its clamp (§55 F01, PINNED), and the clamp's value sets the outcome.
- **A fixed smaller constant would be a pin, not a law** (formula-first). It would also likely cost c1s, where §55 has the clamp binding about 70 % of the time at 2 048.
- **Next step:** the store-law derivation (F01, with C02's horizon in the same derivation), with this block as its measured anchor at c8. Per the registration, this outcome also goes into the lever-B guard's registration.

**Erratum to launch 1 (above).** `[HOLD] hn_n` counts holes that resolved with no retransmit ever emitted (`holddown.rs`, the self-heal class). It tracks the store regime: under the same law it reads 168 (B2048), 11 994 (B1024) and 55 038 (B640). Launch 1's c8 reading of what the law does holds only at the default pool.

**Owner decision (2026-11-14).** The lever-B guard (approved step 4) is paused. Its premise, that lever B is the c8 speed-up, does not hold after this block. The store law comes first: E04 + E03 + F01 + C02 in one derivation (law ledger §4 items 1–2), with this block as its measured c8 anchor. The anchor gives a slope, not the optimum, since B640 does not reach the goodput knee. Fable reviews the derivation before any code. Lever B is re-decided on that result.
