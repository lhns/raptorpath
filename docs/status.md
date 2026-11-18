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

## 58. Engine suites for F1 and Linux release-binary identity for the model-crate moves (no battery) — pre-registration (before any VM contact)

**Why.**
- **F1** (`f1-delta-nan` `39008a01`) changes `raptorpath/src`. It merges only after the engine suites pass on the VM, and before §51.
- **The test-only engine edits of shared-core-0** (`decision_trace_pin`, the non-vacuous walks) have never run on Linux. `decision_trace_pin`'s digests were captured on Windows.
- **Phase 6** (`phase6-forms` `71ad6b72`) and the end-of-run speed fix (`viz-endspeed`) edit `raptorpath-math` only. The engine calls none of the changed functions. ADR-0077 waives the §41c guard for such a step only on an identical **Linux** release-binary sha. The host build differed: base `0abc299a…` against head `70ca90c5…`.

**Part A — suites on the F1 tree.**
- **Tree A** = `f1-delta-nan` `b7e1f06f`: main `ba6637d7` with F1 merged. Its diff against main is F1's two files.
- **On the host** before archiving, all green:
  - `decision_trace_pin` 7/7, including the store digest that covers `codel_setpoint_q`. So F1 leaves the default-path values bit-identical at the pinned inputs.
  - `delta_override_store_cap` 1/1.
- **Run:** `tools/l1/suites_run.sh`, which holds both locks, checks the precondition, uses a fresh target and applies the §8/§11/§12 flake rule.
  - It runs release, doc, wasm (`env -u GOLDEN_CAPTURE`) and debug, the same four cargo suites as `run_suites` in `loopdiag_run_all.sh` that §54 ran. It also runs core, and a parsers line of four older parser tests (all present), not the full tools/l1 test set.
  - It now also runs `tst core cargo test -p raptorpath-core` (main `ba6637d7`).
- **Reading, per suite:**
  - **TESTS-OK:** the suites pass.
  - **A REAL failure in `decision_trace_pin`:** record which digest failed and under which profile (release or debug). Then run the same pin on the base tree on Linux (`cargo test -p raptorpath --lib -- decision_trace_pin`, in both profiles). F1 merges only if the store digest passes and every failing digest fails identically on base. That would be a platform difference, which then gets per-platform digests before any shared-core move.
  - **Any other REAL failure in the release, doc or debug engine suites:** blocks F1, and is diagnosed.
  - **A failure in the wasm, core or parsers suites:** cannot come from F1, whose diff is two engine files. It is a finding about today's merges on main. F1 still merges. `phase6-forms` (Part B) is held until it is diagnosed.
- **Merge rule:** F1 changes engine bytes, so the standing rule is suites, then a §41c guard. **Owner waiver, 2026-11-14:** F1 merges once Part A passes, with no guard. The evidence is the unchanged store digest on the host (default path bit-identical at the pinned inputs), plus `formula_agreement` and `law_shape` green. A store-digest failure on Linux withdraws the waiver.

**Part B — release-binary identity (Linux, same path, same toolchain).**
- **Runner:** `tools/l1/binid_run.sh` (main `ba6637d7`; both locks through `lib_battery.sh`'s `take_lock`), run from tree A's tools.
  - Its stub test `test_binid.py` skips on the Windows host (no `flock`), so it runs on the VM first. If it fails, Part B is not run.
- **Trees** (each a plain `git archive`, as in §56 and §57; the commit rides in the label, not inside the tree):
  1. **control** = `~/p57/main57b.tgz` (main `02ecbdd7`), the archive that built `8f313616…` on the VM today;
  2. **base** = main `ba6637d7`;
  3. **phase6** = `phase6-forms` `cca84f53` (main merged in; its diff against main touches only `raptorpath-math`, `raptorpath-wasm` and the visualizer page);
  4. **treeA** = `f1-delta-nan` `b7e1f06f`, reported only. It is the new reference sha once F1 merges.
  - `viz-endspeed` is not ready, so it waits for the next session.
- **For each tree,** at the same path `~/s58b/src` with a fresh target dir at the same path:
  - the harness's own build command, `CARGO_BUILD_JOBS=4 cargo build --release --bin raptorpath`, then the sha256 of `target/release/raptorpath`;
  - then `cargo test -p raptorpath-math -p raptorpath-wasm` (no capture).
- **Reading:**
  - **control ≠ `8f313616…`:** the method is wrong, and no tree is read.
  - **control = `8f313616…`, base ≠ control:** a finding about main itself (shared-core-0 was merged as test-only). base is then rebuilt once to rule out a one-off, and the cause is diagnosed before anything merges.
  - **base = control, phase6 = base, with its tests green:** the guard is waived (ADR-0077) and phase6 merges, unless Part A held it.
  - **phase6 ≠ base:** phase6 rides guard batch α.
  - **phase6's tests fail:** phase6 does not merge.
  - **treeA:** its sha is recorded as the post-F1 reference.

**Amendment 1 (before any VM contact).** `viz-endspeed` was pushed and green before the archive, so it joins Part B as tree **endspeed** = `e5d04df4`: `viz-endspeed` with `phase6-forms` (and main) merged in.
- **What it is:** the visualizer's end-of-run speed fix. `compute_r_star_exact` memoises the DP by repair count and visits only the live deficit band. The sim memoises the tail solve on exact input bits. The page gets a 12 ms frame budget.
- **Bit-identity:** the outputs are bit-identical against `#[cfg(test)]` copies of the old functions, and the goldens are unchanged with no capture.
- **Reading:** the same rules as phase6. It is compared with base. Since it contains phase6, endspeed = base implies phase6 = base for the engine; each is still recorded.

**Amendment 2 (after Part B's first run, before any re-run; the method is fixed, and no tree is read from run 1).**
- **Part A: PASS** (20:08–20:51Z). Every suite is green with no flakes: release 1136/0, debug 995/0, wasm 44, core 2, doc, parsers. `decision_trace_pin` is 7/7 in both release and debug on Linux.
  - **F1 merged** to main `9b8b42ca` under the owner's waiver.
  - **Deviation:** host builds of two unrelated projects (not raptorpath) ran during the session. Raptorpath's host cargo was paused. No reading here depends on timing.
- **Part B run 1:** the control (`main57b.tgz`) built `1ebe26e7…`, not the known `8f313616…`. By the registered rule the method is wrong, and no tree is read.
  - **Cause, found by reading:** `binid_run.sh` built from the workspace root, while the harness builds from the crate dir (`SRC=$ROOT/src/raptorpath`, `loopdiag_run_all.sh:195/326`). From the root, cargo unifies dependency features across every workspace member.
  - **Fix:** `binid_run.sh` now `cd`s into `src/raptorpath`. `test_binid.py` puts its fake tree under `raptorpath/`, so a root build fails the test.
  - **Reported from run 1, not read:** phase6 and endspeed built the same sha (`3c03b2b3…`). Every finished tree's math and wasm tests are green (182, 190 and 195 passed).
- **Part B run 2:** the same five trees with the fixed runner, after `test_binid.py` passes on the VM. The registered reading applies unchanged.

**Part B result (run 2, 21:34–22:13Z, the fixed runner).**

| tree | release sha256 | math + wasm tests |
|---|---|---|
| control (`main57b.tgz`, main `02ecbdd7`) | `8f313616…` = the known answer | 182 passed |
| base (main `ba6637d7`) | `b68d709d…` ≠ control | 190 passed |
| phase6 (`cca84f53`) | `62bc7dd8…` ≠ base | 195 passed |
| endspeed (`e5d04df4`) | `62bc7dd8…` = phase6 | 198 passed |
| treeA (`b7e1f06f`, main + F1) | `0ec95258…` | 190 passed |

- **The method is valid:** the control equals the known sha.
- **base ≠ control is diagnosed.** Tree X = base minus only the two-line `#[cfg(test)] mod decision_trace_pin;` declaration in `net/mod.rs` builds **`8f313616…`, exactly the control**, with 190 tests passing.
  - So main's engine is byte-identical to the control apart from the line numbers that shift below that declaration. The binary embeds location data. shared-core-0 changed no engine behaviour.
  - Tree Y (base with all of `raptorpath/src` reverted to the control's, keeping the workspace changes) is still building. It is recorded when done.
- **Read as registered:** phase6 ≠ base, so phase6 and endspeed ride guard batch α. The line-shift mechanism is shown for main only. For phase6 it is consistent but untested: phase6 inserts 77 lines at `raptorpath-math/src/lib.rs:202`, above endspeed's first edit at :1208.
- **The sha chain for later re-basing:** `0ec95258…` (main with F1) = `8f313616…` + the `net/mod.rs` line shift (X) + F1's two files.
- **Consequence for ADR-0077's waiver** (an identical release sha): only edits that move no existing line can meet it. Step 1.2, the span-seat extraction and every move into `raptorpath-core` rewrite lines, so they fail it by construction and ride guard α. The oracle spec's "identical sha waives the guard" branch is dead. A weaker identity (the same code section, with differences confined to location data) would change ADR-0077, so it is the owner's call. It would be calibrated on the base/X pair first.

**Amendment 3 (before the build): one re-laid candidate for the visualizer fix.**
- **Candidate** = current main (with F1) + endspeed's content, laid out so that no line of `raptorpath-math/src/lib.rs` above endspeed's first edit changes its number:
  - phase 6's five functions move to a new module file, declared and re-exported below that point;
  - new tests go inside test modules.
- **Control:** current main, known answer `0ec95258…`.
- **Reading:**
  - candidate = control, with its tests green, merges on ADR-0077's literal waiver;
  - otherwise guard α.
- **One attempt only.** A miss means the line-shift explanation is incomplete for the math crate.

**Amendment 3 result (22:39–22:54Z).**
- control = main `504c4ea6`: `0ec95258…`, equal to the predicted answer, with 190 tests passing.
- The re-laid candidate `endspeed-relaid` `e5cef8ce` (phase 6's forms in `raptorpath-math/src/span_forms.rs`, declared below the non-test code, plus endspeed's speed-ups): **`0ec95258…`, identical**, with 198 tests passing.
- It merges on ADR-0077's literal waiver. The line-shift explanation holds for the math crate too.
- **Tree Y** (base with `raptorpath/src` reverted, the workspace changes kept): `8f313616…`, 190 passed. That confirms the workspace member `raptorpath-core` does not touch the engine binary.
- `phase6-forms` and `viz-endspeed` are superseded by `endspeed-relaid`.
- **§58 is closed.**
  - Deviation: host builds of two unrelated projects ran during Part A.
  - Correction: Part B run 1 used the wrong build directory and was not read; this is recorded above.

**Not in this session.** The oracle's evicting-seat diagnostic row (m̄; `pfec-oracle-spec.md` §6) needs a harness path, a stub test and a rehearsal. It gets its own registration.

**Order and length.** Part A, about 50–60 min, then Part B, four builds, each followed by the math and wasm tests: about 4 × 15–20 min. No netns and no battery; nothing runs raptorpath. `LD_HARD` does not apply; the locks are held by the runner for each part.
- **Before launch:** `df` on the VM home. Stale run trees go first: the §56/§57 run dirs, whose ledgers are in the repo.
- **Host builds:** host cargo is paused, with the build lock held by the main session, throughout both parts.

**60-pre Result (2026-11-15, 13:39–14:25Z): PASS.** `s60-tree` `cb88df3a`.
- **Suites, all green, no flakes:**
  - release 1156/0 (48 ignored);
  - debug 1008/0;
  - wasm 50/0;
  - core 2/0;
  - doc and parsers rc 0.
- `decision_trace_pin` is 7/7 in release and debug.
- This was the first Linux run of the gauge's tests and the first full suites since the math-crate merges.
- **Registered `bin_sha`** (crate-directory build, `binid_run.sh`): **`e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d`**. math and wasm 203 passed. The rehearsal, 60a and 60b must each match it.
- **Reference manifest:** `tools/l1/ref60-manifest.sha256` (187 files of `raptorpath/src` + `raptorpath/tests` at `cb88df3a`) is committed and passed as `LD_REF_MANIFEST` to every §60 launch.
- **Deviation:** host builds of other projects may have run. Raptorpath host cargo was paused (lock held by the main session).

## 51. Block E (the evicting seat, paced) and the off-Bulk baseline and δ sweep, reported only: pre-registration (before any VM contact)

**What this registers.** It registers `docs/derivations/s51-evict-design.md` at main `2aaac8e8` **verbatim**, with `s51-design.md` (same commit) as its block-51 base: rows, readouts, predictions (§8.2), go/no-go and VOID (§10), and budget and launch order (§11). That design was reviewed twice by Fable. Nothing here changes a rule of the design. This section records the owner-level choices, the trees and the harness's readings.

**In plain words.**
- **Block E** measures the repair loop on a seat that may drop late data. This time the sender is paced below the link rate, so emission lasts through the measured window. It also measures, with a new gauge, how often the spare-capacity clamp binds.
- **§51** is the first engine measurement away from the Bulk preset. Its question is whether behaviour moves smoothly with the latency dial through every preset (NO-MODE-SWITCH, measured in the engine). It runs on main's unchanged binary.
- **§51's rows are not overdriven.** Reliable delivery ties the sender to the ack clock (design §0). Only block E needs pacing.

**Choices on the design's §12 owner questions.** These are the coordinator's choices, low-stakes and reversible. They were made after the owner's "take your recommendations" of 2026-11-16, which did not cover them, so they are reported to the owner and can be overruled:
1. §51 runs on **main's binary** (the shipped machine's baseline).
2. Pacing is the **app-side `RWM_PERF_RATE_MBIT`**. It costs a new binary and E-pre.
3. **PL with its calibration rule, plus PM.** Without PL, K4 has no scoreable arm.
4. The pacer and the spare-clamp gauge **stay on the measurement tree**. Main takes them only through a guard, if ever (`meas-s51b-rho` will want the pacer).
5. A **DEGENERATE-AS-WIRED** outcome (design §6.3) is a **defect finding** that goes to `rate-spare-clamp-verdict` for a ledger verdict (MEASUREMENT DISCIPLINE 18). The design read the code and found the clamp's cwnd is not enforced at either seat (§7).

**Trees.**

| block | tree | binary |
|---|---|---|
| E | **`e-tree` `b497fcfa`**: `s51e-tree` `77d53d9c` (main `2aaac8e8` + `slaw-gauge` `d416d59b` + the pacer `4ee1df9b` + the spare-clamp gauge `77d53d9c`) with `harness51` `a59efa60` merged in | registered by **E-pre**: `suites_run.sh` + `binid_run.sh` on one `git archive` of `e-tree`. Every later E stage must log the same `bin_sha`, and the reference manifest (`ref51e-manifest.sha256`) is generated from that archive and committed as a tools-only commit |
| 51 | **`b51-tree` `b19c4be9`**: main `14954df7` with `harness51` `a59efa60` merged; the engine diff against main is empty (the witness), and main itself takes the harness only after the Result | recorded at the 51 rehearsal; each 51 launch must equal it. No suites; the dial-pin pre-flight of `s51-design` §6.2 |

- **Engine identity witness for E:** the engine diff of the gauge merge (`9dcedd53`) against `cb88df3a` (§60's `s60-tree`) is empty. The paths are `raptorpath`, `raptorpath-math`, `raptorpath-core`, `raptorpath-wasm`, `gf256`, `Cargo.toml` and `Cargo.lock`. The `harness51` merge adds no engine change.
- **Forwarding:** `RWM_SLAW` and `RWM_PERF_RATE_MBIT` are forwarded by `e-tree`'s own `lib.sh`. Main's `lib.sh` does not forward `RWM_SLAW`, since main reads no such gate (`14954df7`).

**The launch shas** of both trees are pinned by an amendment after E-pre, as in §54. The E tree gains its `ref51e-manifest.sha256` commit then, and 51's tree may gain fixes from the E rehearsal.

**The engine as built, read against the design** (coordinator, from the source at `77d53d9c`):
- **§6.2's `rn + r0 = an` holds by construction on every block E row.**
  - `note_debt_accrued` is in the same per-symbol block as the recompute (`emit_source.rs` :497–:715).
  - The cache short-circuit (`taper_recompute_due`) can skip a recompute only when batching is on.
  - `emit_batch_on` requires `reliable` (`sender_policy.rs:739`), and the evicting seat is not reliable.
  - The engine worker's caveat ("only per symbol") applies to the batched Bulk path of its host test, which block E never runs. Go/no-go (g) is therefore unchanged.
- **The new `[GATES]` token** changes the default `[GATES]` line on the E tree (and its pin). Main is untouched.

**Host checks so far (not the suites).**
- **`s51e-tree`:** `--lib` slaw / perf / gates 33 pass; `decision_trace_pin` 7/7; `--test slaw_reachability` passes with the new `spc=` assertions on a driven loopback run; `render_window_two_legs` passes; `test_slaw_parse.py` and `test_s60.py` pass.
- **Red-first:** the new engine tests were written alongside the code and were never seen red. That is recorded, not hidden. E-pre's suites and the rehearsal are their first independent run.
- **`harness51`:** the unfiltered `test_stubrun.py` on the pushed head (LF export) reports 722 ok, and each of the plan and scorer tests passes. `test_harness46` failed once under host load, then passed on two standalone runs (flake, not investigated).
  - The shell tests were not run. `test_abort_witness.sh` and `test_lib_battery.sh` are Linux-only and run on the VM inside E-pre's window, before the E rehearsal.

**The harness's readings of the design** (each is a registered reading; a later change needs an amendment):
- **Chunk payloads:** 1 182 B at Bulk (1 196 − 14), checked against a committed s53 row (ratio 1.000), and 494 B at Realtime. The `[CHI] n=` form is unverified.
- **At c8 Realtime,** the `srcsize` upper bound is widened ×2.2, in case S08's duplicate source counts in `cum=` (unverified). A DNF row is checked against the upper bound only.
- **No witness has seen live data yet:** the `srcsize` band, the batching echo and the `[CHI]` check. The rehearsals are their first test. A witness that misfires on a rehearsal is amended, never silently relaxed.
- **Block 51's guard env:** it runs with `RWM_RTOBS=1 RWM_RDIAG=1`, beside the design's `RWM_GEN=0 RWM_DIAG=1`. Its scrub also removes `RWM_PERF_RATE_MBIT`.
- **Block E's binary pins:** block E has no `full` path; suite reuse covers the whole session. `ref51e-manifest.sha256` and `LD_BIN_SHA_EXPECT` have no defaults and must come from E-pre.
- **`LD_BASE_SHA`** is block 51's commit pin and doubles as its candidate pin.
- **Statistics:**
  - **d80** uses the exact null at 6 v 6, because a Monte Carlo null cannot reject there.
  - **`d80_51.json` (200 draws)** matches the design's table: c2 goodput / in-order / cpu_cli / cpu_srv 1 / 20 / 8 / 12 %, and c8 5 / 30 / 12 / 20 %. Only 6 rows qualify after `87c2135d`, so the in-session A/A governs resolution.
- **PL's τ_x and W1 upper bound** read n/a, because 700 sym/s is below the 1 000 sym/s thresholds.
- **The stubs:** a stub's goodput is arm-independent, so a FULL-mode T1/T2 passes deterministically in the stub run. The stub tests paths, not statistics.

**Order and budget** (design §11): E-pre (≈ 75–90 min, both locks, no `LD_HARD`) → E rehearsal (`LD_HARD` + 1 h) → E battery (+ 1.5 h) → 51 rehearsal → 51-c2 (+ 5 h) → 51-c8 (+ 5 h). No 51 rule reads an E result.

**E-pre failure rule** (as §60 §9 item 3):
- **In the gauge's or the new code's own tests:** the session blocks and the code is fixed.
- **Anywhere else:** re-run the failing suite on main `14954df7`. A failure identical on main is main's defect, and the session waits for its diagnosis or the owner's ruling. Flakes follow §8/§11/§12's rule.

**Fable's review** (2026-11-17), applied:
- the number (§51, since 61 is the c3 sweep's);
- the coordinator's choices are attributed to the coordinator;
- block 51 runs from its own branch;
- the `rn + r0` identity was checked against the source;
- the launch-sha pin;
- the shell tests.

**§51 Amendment 1 (2026-11-17, after E-pre, before the E rehearsal): E-pre PASS, and block E's pins.**
- **E-pre** ran 00:38–01:25Z on one `git archive` of `e-tree` `b497fcfa` under both locks (`suites_run.sh`, `binid_run.sh`, then the shell tests).
  - Suites: release 1 161 / 0 (48 ignored), debug 1 013 / 0 (47 ignored), wasm 50 / 0, core 2 / 0, doc and parsers rc 0. **TESTS-OK.**
  - The crate-directory build gives **`bin_sha` `8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf`**, the registered value for every later block E stage.
  - Shell tests on Linux: `test_abort_witness.sh` 12 / 0 and `test_lib_battery.sh` ALL CHECKS PASS.
- **The reference manifest:** `ref51e-manifest.sha256` is generated from `b497fcfa` with `ref_manifest.sh` (187 files). It is byte-equal to the VM's extracted tree, and it is committed as the tools-only `e-tree` **`a271e0bb`**. That commit is **block E's launch tree**: its engine diff against `b497fcfa` is empty.
- **Launch:** `LD_PLAN=51e`, `LD_CAND_SHA` = `a271e0bb` (full sha), and `LD_BIN_SHA_EXPECT` = the sha above. The rehearsal runs with `LD_REHEARSE=1` and `LD_HARD` + 1 h, from one `git archive` of `a271e0bb`.

**§51 Amendment 2 (2026-11-17, after the E rehearsal): NO-GO, caused by an engine defect (F2, below); the battery waits for the fix.**
- **The E rehearsal** (01:31–01:49Z, `e-tree` `a271e0bb`; ledger pulled to the host) logged the registered `bin_sha` and a manifest HIT.
- **U passed every gate and met its predictions:** an eviction share of 0.736, ζ = 0.95 and τ_x = 0.879. So `rate-spare-cwnd` reads NOT-ENFORCED on one row, reported only.
- **PL and PM both died in the warm-up** (`warm-up object never acked`). Every failure of gates (b)–(e) on those two rows follows from that, and (h) is uncomputable. The cause is engine defect F2: the warm-up runs unpaced, and the pacer is not involved.
- **Per §10, rehearsal rows are not re-run, and no workaround is used** (no retry, no seed change). Block E resumes after F2 is fixed: the E tree is rebuilt on the fix, then a new E-pre, then the rehearsal.
- **Block 51 is exposed too.** `use_packing` is keyed on the hint alone (`sender_policy.rs:678`), so §51's Realtime rows pack on main's binary. "§51 runs on main's unchanged binary" cannot hold until F2 is fixed. §51's block 51 therefore also waits for the fix, and its tree is re-based by amendment.

## 62. s62: the c3 knee need with the jitter removed (J = 0), reported with one scored label: pre-registration (before any VM contact)

**It registers `docs/derivations/s62-registration.md` at `s62-design` `0f40a59d` verbatim,** with `store-law/s62pred.py` and `.out` (the scorer, run end to end on real rows before registration). Fable reviewed it in three rounds, and the coordinator's registration edits are in that commit.
- **One cell:** `c3j0-25` (c3's netem line with jitter 0). 42 rows plus 2 dump rows, on binary `e410d640`, Bulk.
- **The scored label:** need₀ (the knee-native window minimum minus a G(250) floor) against Δ₀ = ½·q_R·F₀. The labels are BELOW / ABOVE / UNRESOLVED / UNSCOREABLE / FLOOR-SENSITIVE / CONTROL-MOVED, gated on the floor's count drop D₀ < 0.5 ms.
- **No prediction for need₀;** D₀ and E90_own are the rehearsal's first readings.
- **Every rehearsal NO-GO is registered with its consequence.** That includes ABORT-NETEM (the cell must run at J = 0) and one re-probe on NO-GO-PILOT, so there is no amendment-then-session loop.
- **CR62** (every clause needs the goodput cut) is s62's rule only. CR61 and the collapse-rule defect stay open.
- **Order:** after §63 (owner decision 13). The `harness62` worker starts after `harness63` is pushed, and the launch sha is pinned by an amendment.
- **Cost:** about 1.4 h typical, 2.8 h worst, with host cargo paused.

## 64. The F2 merge guard (§41c pattern, F2 alone), scored: pre-registration (before any VM contact)

**It registers `docs/derivations/f2-guard-registration.md` at `f2-guard-design` `39269746` verbatim.** Fable reviewed it in three rounds.
- **Seats:**
  - BASE64 = branch `g64-base` (main + `harness64`, tools only);
  - CAND64 = `g64-cand` (BASE64 + `git merge --no-ff github/f2-impl` at `ab9feec4`, 17 commits; commit 1 never alone).
  - The engine patch-id must equal `3e69ce68…`.
- **Stage G (scored, Bulk):** plan 41d's 96 rows (c1s, c1d, c2, c8; rank-minp, PASS iff p > 0.05), plus the F2 witnesses on every CAND row. An `F2-FAIL` row fails the guard whatever p reads.
- **Stage R (scored, Realtime):** c2-100 at ρ = 1 on both ends, 100 MB, 12 + 12 rows.
  - The F2 reading must pass on every CAND row. Completion is scored against BASE's surviving rows.
  - Non-F2 voids are replaced once; a repeat reads `RT-INCOMPLETE`.
- **`framing_ns`:**
  - the bound is frame ≤ +22.3 and read ≤ +36.5 ns per call;
  - the scored reading is the rehearsal's 10-run median of CAND − BASE (the frame point is an upper bound);
  - tonight's runs are a pre-read, committed raw.
  - Its label is a merge condition: PASS, or the owner accepts the printed cost.
- **Reuse:**
  - the engine suites come from `ab9feec4`'s VM run (release 1151/0, debug 1002 with one cited flake, wasm, core, seams), through a wider `src_manifest()` reference (`ref64`) with a host `cmp` proof;
  - the launch takes the rehearsal's sha-checked binaries and tools-test result.
- **Merge conditions:**
  - GATE-41d PASS, zero F2-FAIL rows, RT-PASS and the bench label;
  - the owner's go on wire v10 (every endpoint upgrades together; v9 is refused) and on an engine with no initial path refusing to start;
  - ADR-0078 to Accepted on that go.
  - These are asked once, with the Result.
- **Cost:** about 2 h typical and 4.5 h worst, with host cargo paused.
- **Order:** s62, then §64, then E-pre, block E, block 51. `harness64` is a second Sonnet worker, and the seat shas are pinned by an amendment.

**§62 Amendment 1 (2026-11-18, before any VM contact for its rows; Fable-reviewed): the launch tree and the harness's readings.**
- **Launch tree: `harness62` `95dafa59d5f3d2152bdf2d51aafcb39d95e41be7`,** tools only, on `harness61` `3cdc4470`. The engine diff against `1ecee59d` is empty.
  - No build: `LD_BIN_PREBUILT` = the staged `e410d640` binary (sha-checked), for the rehearsal and for 62a.
- **Pre-launch checks done:**
  - **ABORT-NETEM** on the real VM topology (topology only, no rows): `plan62.py netem` passes `c3j0`'s live `tc qdisc show` (`delay 20ms`, no jitter term), and it reads `jitter term 5ms` on `c3` on both legs. The topology was taken down after.
  - **The cell name `c3j0-25` through the real row path:** 61c renamed, run through `make_row`, `smoke(62r0)` and `score(62a)`, reproduces `s62pred.out`'s J = 5 numbers. One silent fallback was found and fixed: `c3j0-25` was missing from `plan60.LATE_T` / `plan61.LATE_T`, so it took the default. Tests now pin every cell-keyed table.
  - **The absolute control-arm test** is in `s62_score.py`, with a test.
- **Counts at `95dafa59`** (LF export):
  - 46 `tools/l1` test files, 2 284 ok. `test_s60` is the known host-only failure; `test_harness46` had a timing flake under load and passes alone.
  - All 22 plan-62 stub paths pass (117 ok).
  - The full unfiltered `test_stubrun.py` is owed before `harness62` reaches main.
- **The harness's readings are registered by reference** (`s62_score.DECISIONS`). Two are named here:
  - the budget guard sits below the `LD_HARD` offsets (2 400 s / 7 800 s against 1 h / 2.5 h);
  - a mixed pilot pair (u_lo ≥ 0.95 > u_hi) is NO-GO-PILOT, with no re-probe.
- **Run rules:**
  - host cargo and host tests are paused under the coordinator's host lock;
  - the rehearsal is `LD_PLAN=62r0 LD_REHEARSE=1 LD_CTL62=512`, with `LD_HARD` = launch + 1 h;
  - 62a is `LD_PLAN=62a LD_CTL62=512 LD_KNEE62=<PILOT62 W= verbatim> LD_REHEARSAL62=<the rehearsal's run dir>`, with `LD_HARD` = launch + 2.5 h.
  - Before 62a, the coordinator reads the first readings: D₀, F at three counts, E90_own and X_min's bind, u₈₀ / u₁₂₈ and the case, the K margins, the floor spread against J = 5's, and D1's dump coverage.
  - A rehearsal NO-GO is the Result.

**§62 Result (2026-11-18; the registered NO-GO; Fable-reviewed): `LABEL62 GO-c3j0 NO-GO-GATE`, so s62 ends as UNSCOREABLE. Its registered consequence is that R6.7 item 4's count rule comes first.** The cell (62a) does not run.

**In plain words.**
- With c3's jitter switched off, the floor's minimum, read from one native window up to 2 100 samples, still depends on the count by 3.36 ms, against the registered 0.5 ms gate.
- **Almost all of that is below 250 samples.** Beyond 250 the floor moves 0.39 ms, under the gate. Whether a minimum at 250 or more samples is count-free at J = 0 is the derivation's question, not this Result's.
- **The comparison with J = 5:** switching the jitter off removed the drop above 250 samples, and left the drop below it.
- The registered next step is the count rule, before any knee reading.

**Run.**
- Rehearsal `62r0`, launch 05:18:54Z, done at 05:38:48Z. Tree `harness62` `95dafa59`; prebuilt `e410d640`.
  - SMOKE-PASS / REHEARSAL-PASS are the harness criteria. The go/no-go read NO-GO-GATE.
- **Host:** cargo paused under the coordinator's host lock.
  - The `harness64` worker was told to pause host tests too, and did not confirm before the launch.
  - Its log and export directories show no file written between 05:18 and 05:39Z.
  - Either way, the same lower step shows in both quiet-host J = 5 sessions.
- The host re-score (`s62_score.py smoke62 62r0`) matches the VM's `smoke-check.txt` byte for byte.
- **Ledger:** `docs/l1-raw/s62/`.

**Every rehearsal check passed except the gate:**
- the live netem read is `delay 20ms` with no jitter term on both legs, before and after every row;
- floor validity: 65 G(250) groups, not COLLAPSED;
- control health: R1 / R6 17.81 / 17.82 Mbit/s, loss 0.0476 / 0.0495, within 1 % of each other;
- the pilot is proportional: `PILOT62 W=93.8 u_lo=0.810 u_hi=0.994`, giving K 88/100/112, P1 144;
- LATE lengths are 148–305 windows.

**First readings (the Result's content; reported, interpretation goes to a derivation):**

| reading | J = 0 (this rehearsal) | J = 5 (61c / 61a) |
|---|---|---|
| floor W 12 F: native / G(250) / G(2 100) | 44.656 / 41.688 / 41.297 ms | — |
| **D₀ = max(native, G250) − G2100** | **3.359 ms** (the gate needs < 0.5) | 8.43 / 7.07 |
| native → G(250) | 2.968 ms | 3.973 / 3.462 |
| G(250) → G(2 100) | 0.391 ms | 4.459 / 3.609 |
| floor window spread: native p50 − min / p90 − p10 | 1.986 / 0.641 ms | 5.801 / 8.169 (61a 5.941 / 8.285) |
| X_own on the floor pair | 3.34 ms (E50 1.99, E90 2.44); X_min 2.08 does not bind | — |
| E90_own / (p90 − tmin), ms | floor 2.47 / 2.48 and 2.42 / 2.34; W 80 7.86 / 8.00; W 128 13.82 / 13.05; control 20.69 / 18.01 and 24.54 / 19.21 | E90 9.34 (floor) |
| u₈₀ / u₁₂₈ | 0.810 / 0.994 | — |
| dump D12 coverage | 0.993 (21 248 of 21 403), no cap | — |

- **The jitter is off on both witnesses:** the netem line, and the floor's p90 − p10 falling from about 8.2 to 0.64 ms.
- **The count dependence that remains** sits in the step from the native window to 250 samples. That shows as the gap between native p50 − min (1.99 ms) and p90 − p10 (0.64 ms). **Corrected (count-rule `2d557191`):** this is two modes, not a thin tail. A tight bulk sits around 46.8 ms, and a queue-free low mode at 41.2–41.9 ms holds 2–3 % of samples.
  - A native floor window holds about 58 samples: the D12 dump's 21 403 over 371 windows.
  - D₀ is about 7 datagram times at c3 (0.49 ms each). That is the registration's "service-time" side, not its 0.1–0.2 ms timing side. That is the registration's framing; the cause goes to the derivation.
- **Limits:**
  - the J = 0 floor reading rests on two rows, the rehearsal's floor pair;
  - 61c's G(2 100) step had only 4 groups per W 12 row.
- **Reordering:** not measured (no counter exists).

**Registered consequence:** NEEDS-MORE-gate. The J = 0 floor still depends on its count, so R6.7 item 4's count rule comes first, as a derivation for review.
- J = 3 stays a separate decision (§62).
- Decision 8 is unchanged and unsettled.
- **The derivation starts from the D12 dump,** with no VM:
  - Are the low samples spread evenly in time, or do they follow gaps?
  - Which count does the law's lifetime-minimum floor (ℓ̂, m̂) correspond to?

**The count-rule derivation (2026-11-18; `count-rule` `2d557191`; post hoc, on existing rows; Fable-reviewed): no count rule exists on these rows. The open question is the sampler's design.**
- **Established, scoped to three W 12 rows at J = 0 and one loss rate** (s62's D12 dump and its floor pair):
  - The J = 0 floor has a queue-free low mode at 41.2–41.9 ms (40 ms propagation, plus about a datagram, plus processing).
  - 29–32 % of native windows reach it, in runs of 1–2 windows.
  - It appears mostly in windows that carry a retransmission: 84–87 % of low-mode windows, against 5–8 % of the rest. Every send pause of 60 ms or more sits next to a retransmit.
  - So how often the queue-free loop is seen depends on the loss and send rates.
- **s62's D₀ is the law's own brake term in steady state.** The native window minimum minus m̂ is 3.36 ms at J = 0 (44.66 − 41.30), and 8.4 / 7.1 at J = 5: D₀ exactly.
  - The brake bridges the window minimum the law steers and the long-run minimum it anchors on. Dropping it would put 3–8 ms into the need at zero load.
- **Defect finding (a ledger row owed, beside F07 and E13): b̂₀⁺ is a bad estimator of that brake.**
  - It latches the maximum of `tmin − m̂` over the first 8 windows, during the start transient, while m̂ is still falling. Boot windows hold 3–223 samples.
  - So one window sets F for the whole row: F reads 37.2–46.2 ms across 61c's rows and 42.9–138.5 across 61a's (one 103.4 ms brake, at k = 2).
  - Owed check: whether that 61a row is G12 s42 r5, the B1 outlier both §61a Results left open.
- **Not established (hypotheses):**
  - **m̂ as the floor anchor.** It reads the low mode at J = 0, but 34–35 ms at J = 5, the jitter's low extreme on a 40 ms path, and its count grows about 8× from floor to knee.
  - **Burst serialization as the cause of the bulk offset.** It explains about half (a 6.2-symbol burst × 0.47 ms ≈ 2.9 ms of J = 0's 5.4 ms), and it misses the W 16 dose point.
- **s62's consequence, restated:** no count rule exists on these rows. The 500-sample minimum still drifts 2.5–4.1 ms at J = 5 and 1.2 ms at c2, so it is dropped.
- **Next: a sampler-design derivation** (a fresh Opus agent, no VM, no s62 re-run):
  - how to estimate the brake, given it shrinks as windows fill (2.97 of 3.36 ms is gone by 250 samples at J = 0);
  - whether the knee's window minimum is also set by post-loss drains;
  - what anchors the floor under jitter.
  - J = 3 stays parked. Decision 8 is unchanged and unsettled.

**§64 Amendment 1 (2026-11-18, before any VM contact for its rows; Fable-reviewed): the seats and the run rules.**
- **Seats** (new branches; the first `g64-base` / `g64-cand` were not force-pushed over):
  - **BASE64 = `g64-base2` `3b8fca944d65b54f8d86c2ea5c7a003095d2e9d9`:** main `5fada689` + `harness64` `4feeb8fd` (`--no-ff`, tools only); its engine equals main's.
  - **CAND64 = `g64-cand2` `13bb364701aca15b1b7dbdaf161a0a128586cecf`:** BASE64 + `git merge --no-ff github/f2-impl` (`ab9feec4`). The engine patch-id is `3e69ce68…` as registered, and the tree equals `ab9feec4` outside `docs/` and `tools/`.
- **Suite reuse:** the `g64-cand2` archive's `src_manifest()` (212 lines) cmp-equals `tools/l1/ref64-manifest.sha256`, so it is a HIT on `ab9feec4`'s VM suite run (release 1151/0, debug 1002 with the cited flake, wasm 50/0, core 2/0, seams 3/0).
- **Harness tests on the launch tree:**
  - full fast unfiltered `test_stubrun.py` on an LF export of `g64-cand2`: ALL OK, 760 ok, all 20 plan-64 paths;
  - `STUBRUN_FULL=1` for `64/rehearse` and `64/full`: ALL OK;
  - every `tools/l1` test passes except `test_s60`'s known host-only step.
- **Checked before the launch:**
  - the rehearsal's smoke has a CAND row on a dual-leg cell (c8);
  - the stage-G F2 checks read real two-leg logs (§54's c8 / c1d rows, with a labelled shim adding the F2 lines);
  - the committed `store-law-n1` warm-up death classifies `BASE-F2-DEAD`, not a void;
  - stage R replaces a non-F2 void once, capped at 2 slots (stub-tested).
- **The harness's readings are registered by reference.** Those that reach other plans:
  - `tst core` in every plan's suites;
  - `--- F2` lines in every ledger;
  - the fast-mode guard design.
  - `GATE-64 FAIL` with reason NO VERDICT is a re-run, not a finding against F2.
- **Host load, found 2026-11-18:** a stuck python process from one of the coordinator's own commands (PID 11456, `python.exe -`, started 16.11 at 18:42 through a Claude shell) had used a full host core continuously for about 35 CPU-hours. It is gone before this launch.
  - It ran through §61's sessions, §63 and s62. Their Results stand on their own controls and checks, and none scored host CPU.
  - Host-load checks before a timing session now include the process list.
- **Run rules:**
  - host cargo and host tests are paused under the coordinator's host lock;
  - the rehearsal is `LD_PLAN=64 LD_REHEARSE=1` with `LD_HARD` = launch + 1.25 h; it builds both seats and runs `framing_ns`;
  - the launch reuses the rehearsal's binaries (`LD_BIN_PREBUILT_BASE` / `_CAND`, sha-checked) and its tools-test result (`LD_PRIOR_TOOLS`), with `LD_HARD` = launch + 2.5 h;
  - each uses fresh archives of `13bb3647` (CAND) and `3b8fca94` (BASE).
- **Before the launch, the coordinator reads:**
  - both seat shas, the seat check, the suite-reuse HIT and the tools manifest;
  - each seat's smoke row LIVE with rc 0;
  - the CAND F2 witnesses (`[SYMSZ]` 1200/1200 in stage G and 512/512 in stage R, the server's `[CTLD]` with `undeliv=0`);
  - stage R's `window_reliable: true` on both ends, finishing well inside 150 s;
  - the BENCH label and its ten-run table.

## 63. F2 first contact: the failing E rows on the old and fixed binaries, reported only — pre-registration (before any VM contact for its rows)

**Why.**
- The cause of F2 is proven in code. Its trigger, a lost `WindowStart`, is inferred and was never observed.
- **There is no pacer correlation.** The E rehearsal's U row ran on the same cell, hint, seed and reliability as PL and PM, and the warm-up runs unpaced (§51 Amendment 2). So the three rows ran one identical warm-up, and 2 of 3 failed. The failure is stochastic under one netem seed.
- So before the full E-pre, a few rows show two things: whether the control reproduces the failure, and whether the fix removes it.

**Trees and binaries.**

| seat | tree | binary |
|---|---|---|
| CONTROL | `e-tree` `a271e0bb`, unchanged | the registered E `bin_sha` `8013a312…` (§51 Amendment 1). Logged and checked at launch. |
| CAND | **`f2-cand-e` `1a2acc73`**: `e-tree` with `f2-prep`'s three commits, the design docs and `f2-impl`'s four engine commits cherry-picked (`-x`, never commit 1 alone), plus `harness63`'s tools commit merged at launch | recorded at the rehearsal; every later launch must equal it |

- **Engine identity witness:** the engine delta of `f2-cand-e` over `a271e0bb` has the same `git patch-id --stable` (`77887dc2…`) as `f2-impl` over main. So the CAND engine is exactly the change tested on `f2-impl`.
- `lib.sh` still forwards `RWM_SLAW` and `RWM_PERF_RATE_MBIT`.
- **The fix on Linux (VM, 2026-11-17):**
  - `f2-impl` commits 1–2 (`d3051fbf`) pass the full suites: release 1146/0, debug 998/0, wasm 50/0, core 2/0, parsers. The repro passes, its first Linux run.
  - Commit 3 (`bf44836e`) passes: release 1148/0, debug 1000/0.
  - Commit 4 (`ebdad071`, a test-race fix in the fail-closed test only) passes the three F2 loopback tests (fail-closed, mixed-size, repro) 10/10, and the release lib 604/0.
  - **The CAND tree's pre-flight** (`1a2acc73`): `cargo check --all-targets` clean; `--lib` slaw 14, perf 7 and gates 14 pass; `slaw_reachability` passes; the three F2 loopback tests pass.
  - The coordinator read commit 3's four extract sites: the read precedes delivery, `note_delivery` and `next_expected`, and an error ends the receive loop. No `tools/l1` parser reads a log line the fix changed.

- **The control gets no counter.** Adding `f2-prep`'s counter would change the binary away from the one that failed.
  - The control's failure witness is therefore the warm-up line itself: `warm-up object never acked`.
- **The candidate's witnesses:**
  - `undeliv=0` on `[CTLD]`;
  - no `[UNFRAMABLE]` line;
  - `[SYMSZ] local=512 decoder=512` on both ends.

**Rows** (plan 51e's own arm definitions, on cell c2-100 at the Realtime hint with `RWM_C_RELIABLE=0`; 10 rows):

| arm | pace | seeds | bytes |
|---|---|---|---|
| PL | 2.7664 Mbit/s | 42, 7 | 20 748 000 |
| PM | 35 Mbit/s | 42, 7 | 262 500 000 |
| U (unpaced) | none | 42 | 600 000 000 |

- Each row runs once per seat, CONTROL then CAND, in that order: 10 rows.
- Every row's warm-up is the same unpaced Realtime warm-up, so the five rows per seat are five draws of it: three at seed 42, two at seed 7.

**Predictions.**
- **CONTROL:** warm-up failures out of 5 are reported. There is no per-row prediction: the observed rate was 2 of 3 at seed 42, and the committed base rate is 1 of 6 Realtime rows.
  - If CONTROL fails 0 of 5, the CAND reading is weak (nothing was reproduced). That is reported, not voided.
  - A CONTROL row has no counter, so `undeliv` is n/a there.
- **CAND, all five rows: the F2 reading** is the warm-up ack plus the three witnesses:
  - `undeliv=0` from a `[CTLD]` line that is present (a missing line is a missing witness, not a pass);
  - no `[UNFRAMABLE]`;
  - `[SYMSZ]` local = decoder.
  - A warm-up failure or a failed witness on CAND is a finding against the fix, and it blocks E-pre.
- **CAND transfer completion** is reported separately. No paced transfer has yet completed on the VM on any binary, so this is the pacer's first live run. A failure after a clean warm-up is a pacer or E-tree finding, not one against F2.

**Not scored:** goodput, delay and E's gates (b)–(h). Those belong to block E's battery after E-pre.

**Run rules.**
- **`LD_REHEARSE` first:** the CAND build and the `bin_sha` checks, then CONTROL PL s42 and CAND PL s42, both reported.
  - A rehearsal row is not re-run. A failed CAND rehearsal row is the finding.
- **No engine suites in this session.** They ran on the `f2-impl` trees (above), and the CAND tree had the pre-flight only. CAND's full suites are E-pre's.
- **The CAND binary:** a plain release build from the launch archive, never a `test-seams` target dir. Its sha is recorded by `binid_run.sh` on that archive and pinned by an amendment.
- Host cargo paused.
- `LD_HARD` = launch + 1.5 h: two builds of about 8 min each, about 8 min of tools tests, and 10 rows of at most 150 s.

**Harness:** `harness63`, a Sonnet worker, tools only, off `e-tree`. It adds:
- a plan path `63` (10 rows, two seats; CONTROL pinned at `8013a312`, CAND by `LD_BIN_SHA_CAND`);
- the witnesses above in its check;
- `l1common.f2_void`, which voids the row in the E and 51 scorers (`VOID-F2`);
- stub-run path tests.

The launch sha is pinned by an amendment.

**Order:** §63 first, then s62 (owner decision 13), then block E's E-pre on the CAND tree, then block 51. The stacked §41c guard, with F2 at its base, comes before F2 merges to main, after this session. This session decides whether F2 is the whole cause.

**Before the merge, not before §63:**
- the early-`add_path` and no-initial-paths size gaps, closed with a test;
- `framing_ns` run on the VM;
- the `test-seams` tests added to `suites_run.sh`;
- ADR-0078's deviations recorded (including whether a fast close drops earlier deliveries);
- an end-to-end Realtime test that puts two or more small packets in one symbol. No row or loopback shown here does: perf chunks fill the 510-byte capacity, and the tests use 300 / 508 / 600 / 1 196 B packets.

**Pending:** Fable's review of this registration.

**§63 Amendment 1 (2026-11-17, before any VM contact for its rows): the launch tree and the seats.**
- **Launch tree: `s63-tree` `cda0c6290d2f59cd4f3a02204af0eeeaa17fd5ca`.** It is `f2-cand-e` `0590173a` (`e-tree` plus `f2-prep`, the design docs and all seven `f2-impl` engine commits) merged with `harness63` `4627b4b8` (tools only).
  - The engine delta over `a271e0bb` has the same patch-id as `f2-impl` over main (`490a0d1f…`).
  - `f2-impl`'s head `626fdf9a` passes the full suites on Linux: release 1150/0, debug 1002/0, wasm 50/0, core 2/0, seams 3/0, parsers.
- **CONTROL** is `LD_BIN_CONTROL` = the E rehearsal's own binary, copied on the VM. Its sha256 equals the registered `8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf`, and the harness checks it.
- **CAND** is a plain release build of the launch archive. The rehearsal records its sha (`LD_BIN_SHA_CAND`), and Amendment 2 pins it before the launch.
- **The harness, as built (`harness63`):**
  - plan 63 runs 10 rows once each, with no replacement;
  - no engine suites run, but the tools tests do;
  - the budget is 5 400 s for the launch and 1 800 s for the rehearsal;
  - a prebuilt CAND is refused.
- **The F2 reading:**
  - `undeliv` is read from the receiving end's `[CTLD]` lines. That is the server: `perf_rwm_c.sh`'s client sends the transfer.
  - The reported value is the sum of both ends.
  - A row with a `[CTLD]` line only on the sending end FAILs as a missing witness.
- **Stub runs:** the fast unfiltered stub run is ALL OK (758 ok at `57981699`), and every plan-63 stub path passes, including `cand-ctld-send-only` at `4627b4b8`.
- **Run rules:**
  - host cargo is paused, under the host lock held by the coordinator;
  - the rehearsal is `LD_PLAN=63 LD_REHEARSE=1` with `LD_HARD` = launch + 1 h;
  - the launch has `LD_HARD` = launch + 1.5 h;
  - each uses one fresh `git archive` of `cda0c629`.

**§63 Amendment 2 (2026-11-17, after the rehearsal, before the launch): the CAND pin and the rehearsal's readings.**
- **Rehearsal** (21:17–21:37Z, `s63-tree` `cda0c629`): REHEARSAL-PASS.
  - CAND built; its sha is recorded as **`LD_BIN_SHA_CAND` = `96c2dc1c9986ecdccbda24d36c15db4c3c913fb726ddd61f5e075318c9279cd7`**.
  - The CONTROL sha equals `8013a312…`.
- **Readings, reported:**
  - **CONTROL PL s42:** the warm-up acked in this draw. The failure is stochastic, as registered.
  - **CAND PL s42:** the F2 reading is PASS (warm-up acked, `undeliv=0` on the server's `[CTLD]`, no `[UNFRAMABLE]`, `[SYMSZ]` 512/512 on both ends).
  - **Both PL transfers are DNF** at the row's 75 s timeout, on both binaries. 20.7 MB at 2.77 Mbit/s needs about 60 s nominal, unreliable with eviction.
    - As registered, this is a pacer / E-tree reading, not an F2 one, and it is reported separately.
    - It goes to block E's design before E-pre: whether PL's timeout and completion rule fit an evicting seat.
- **Launch:** `LD_PLAN=63`, `LD_BIN_SHA_CAND` as above, `LD_BIN_CONTROL` as Amendment 1, `LD_HARD` = launch + 1.5 h, from a fresh archive of `cda0c629`.

**§63 Result (2026-11-17; REPORTED ONLY; Fable-reviewed): CAND F2 reading PASS on 5 of 5 rows; CONTROL failed the warm-up on 2 of 5.**

**In plain words.**
- On the rows that failed in the E rehearsal, the old binary still fails the Realtime warm-up some of the time: 2 of 5 here, 2 of 3 before.
- The fixed binary passed every time, with zero undecodable symbols on the receiving end and both ends decoding at 512.
- One of the old binary's failures was the unpaced U row, so the pacer is not part of the trigger.

**Runs.**
- **Rehearsal:** 21:17–21:37Z (Amendment 2).
- **Launch:**
  - The first launch, `~/s63` at 21:39Z, was refused by the budget guard before anything ran (5 391 s against 5 400 s: the coordinator's 30 s settle delay ate the margin).
  - The relaunch, `~/s63b` at 21:40:41Z with `LD_HARD` = launch + 1.6 h, is the session. Its rows ran until 22:10:49Z.
- Both `bin_sha` values equal their pins (CAND `96c2dc1c…`, CONTROL `8013a312…`), and the tools tests pass on the VM.
- `VERDICT63 REPORTED-ONLY rows=10/10 control_warmup_fail=2/5 cand_fail=0/5`.
- Host cargo was paused from 21:16:45Z to the end.
- The host re-score (`plan63.py report`) matches the VM's byte for byte.
- **Ledger:** `docs/l1-raw/s63/`.

| row | CONTROL (`8013a312`) warm-up | CAND (`96c2dc1c`) F2 reading |
|---|---|---|
| PL s42 | ACKED | PASS |
| PM s42 | **NEVER-ACKED** | PASS |
| U s42 (unpaced) | **NEVER-ACKED** | PASS |
| PL s7 | ACKED | PASS |
| PM s7 | ACKED | PASS |

- **CAND PASS on every row** means:
  - the warm-up acked;
  - `undeliv=0` on the server's (receiving end's) `[CTLD]` lines, with 0 on the client too;
  - no `[UNFRAMABLE]`;
  - `[SYMSZ]` 512/512 on both ends.
- **Pooled over the E rehearsal, this rehearsal and this session:**
  - CONTROL failed **4 of 9** warm-up draws on one binary: 4 of 7 at s42, 0 of 2 at s7.
  - CAND failed **0 of 6**.
  - If CAND failed at CONTROL's rate (about 4 in 9), 0 of 6 would come up about 3 % of the time.
- **The claim that the fix removes the failure rests on three things together:**
  - these rows;
  - the deterministic repro (a dropped `WindowStart`, 10 of 10 on Linux);
  - the code (no framing state is announced).
- **`undeliv=0`'s denominator** is given below: 42–531 k source symbols per CAND row.
- **The trigger stays inferred:** a lost `WindowStart`, consistent with the failure being stochastic under one netem seed. The CONTROL binary has no counter to show it directly.

**Correction to Amendment 2:** every transfer is DNF on both binaries, and that is by block E's design, not a finding.
- Plan 51e's paced timeout is the emission time plus 15 s ("the DNF lands 15 s after emission ends").
- U runs 600 MB against a 30 s timeout.
- So "goes to block E's design" is withdrawn.

**What follows:**
- **F2 is the cause of the warm-up deaths** that made the E rehearsal NO-GO. PL and PM's E gates (b)–(h) have still never been read; they come in E-pre's rehearsal.
- **Open before E-pre: every row reads `status=VOID-RC` (rc 1).** That includes rows whose warm-up acked and wrote a summary.
  - **No row in either session prints the client-failure marker** (`perf_rwm_c.sh:385`, printed only when the client exits non-zero). So the perf client exited 0 everywhere, and the rc 1 is added after it, in the script's tail or the battery's row wrapper.
  - The E rehearsal's rc-0 row (CONTROL unpaced U) is not a matched comparison for §63's acked rc-1 rows (CONTROL paced and every CAND row), so a binary or pacer cause is not yet excluded by comparison. The marker rules both out for the client itself.
  - The §63 readings come from the logs and do not depend on rc.
  - **Cause (found, 2026-11-18; fixed in `harness63` `16a6ed17`):** `harness63`'s own F2 lines in `perf_rwm_c.sh`.
    - The script sources `lib.sh`, which sets `-euo pipefail`. `grep -c` exits 1 on no match, so the F2 line ended the script with rc 1 after the client had exited 0, on every row with no `[UNFRAMABLE]` line.
    - The E tree's script (`a271e0bb`) has no such line. The fix ends each substitution with `|| true`.
    - The stub now runs the real script's F2 lines under `set -euo pipefail`, and plans 63 and 51e assert LIVE; both were red before the fix.
    - E-pre and the guard must take the fixed lines.
  - **The `undeliv=0` denominator:** the source symbols the server received per CAND row, recomputed from the raw logs by `plan63.py report`:
    - PL 42 802 / 42 980;
    - PM 530 746 / 530 283;
    - U 341 519.
    - Every row reads `undeliv=0` on both ends.
- **Next:**
  - s62, then E-pre on the final F2 tree;
  - the stacked §41c guard (F2 at its base) for the merge to main;
  - block E on the E tree plus F2; block 51 needs the merged binary.
- **Withdrawn:** §63's registration text "no paced transfer has yet completed on the VM ... this is the pacer's first live run". Plan 51e's rows DNF by design.

## Engine defect F2 (2026-11-17): a Realtime receiver can misread every packed symbol for the whole connection
- **Mechanism** (code-proven; diagnosis from the §51 E rehearsal logs, checked on main `756afa66`):
  - The receiver's `window_packed` starts false (`receiver.rs:138`), and only a `WindowStart` control message sets it (`:2153`).
  - `WindowStart` is sent once per path as an unreliable QUIC datagram, and its result is discarded (`net/mod.rs:1862–1871`).
  - Realtime packs (`sender_policy.rs:678`, hint-keyed), and the packed framing writes a big-endian length (`framing.rs:20`). The unpacked reader reads it little-endian (`framing.rs:81–89`), so a packed symbol read unpacked yields no packet.
  - **The receiver still records the delivery and acks the seq,** so the sender prunes it and ARQ never notices. If every path's `WindowStart` is lost, the tunnel is dead for the whole connection while its counters look healthy.
  - Bulk and Auto do not pack, so the default `false` matches their framing.
- **Trigger, inferred and not observed:** GE loss on the client→server path at connect (quinn `lost_packets` 4 in the first 2 s on both failing rows). Netem's seed 42 repeats the early loss bursts. (Corrected in §63: there was no pacer correlation. The unpaced U row ran the same warm-up and passed, so it was 2 failures in 3 draws of an identical warm-up.) It is recorded as open.
- **Base rate in committed ledgers:** 1 of 6 Realtime rows failed the warm-up the same way (`store-law-n1` final, c2 LAW Realtime s42 r3), against 0 of 48 rows at the other hints. Today: 2 of 3.
- **Age:** the code dates from 2026-03 (`e508adf4`). The §60 binary has it too.
- **Two defects:**
  - (a) the framing depends on a one-shot unreliable announce, and is chosen by a hint-keyed bit on the receive side (a NO-MODE-SWITCH debt);
  - (b) the receiver acks and records delivery for a symbol it could not unframe.
- **The fix's format is the owner's call** (a wire change, an ADR and a protocol bump). Before the fix:
  - a red-first loopback repro (drop the first control datagram at Realtime; assert app delivery);
  - a zero-cost undeliverable-symbol counter that every smoke asserts is zero;
  - an audit of other receiver state set only by one-shot unreliable messages.
- **A guard** against main is needed if Bulk's bytes on the wire change.

## 61. The c3 knee-margin sweep (owner decisions 8–9 of 2026-11-17), reported only: pre-registration (before any VM contact)

**What this registers.** It registers `docs/derivations/s61-registration.md` at `s61-c3-design` `ad31af2f` **verbatim**, together with its prediction script and output (`docs/derivations/store-law/s61pred.py` and `s61pred.out`). Fable reviewed it twice. Its question is whether the knee margin is the floor-window spread S_F, so that g's Realtime end can be measured.
- **Cells:** a single-leg c3 cell, `c3-25` (66 rows), and a c2 add-on of below-knee price arms (30 rows) in separate launches 61a and 61b.
- **Binary:** the §60 binary rebuilt from a tools-only branch `harness61` off `harness60-a2` `1ecee59d`. Every launch must log `bin_sha` `e410d640…5f25d`.
- **Fixes:** the four §60 defects as that document fixes them.

**The registration's questions, answered as the coordinator's choices (reported to the owner):**
1. **The falsifier margin Δ stays 0.085,** c2's q_R·F̂/S_F.
2. **If the rebuild does not reproduce `e410d640`** (most likely because the VM toolchain moved), the session does not run on a new binary silently. A new suites run and a new registered sha go in as an Amendment, with Fable's review, before any row. That costs ≈ 1 h.

**Before VM contact:**
- the harness (`harness61`, a Sonnet worker, per §9 of the registration), with its stub paths and a rehearsal;
- an amendment pinning the launch sha.

**Order:** this session queues after §51's E block.

**§61 Amendment 1 (2026-11-17, before any VM contact): the harness as built, its readings of the registration, and the run rules.**
- **This supersedes the §61 stub's "this session queues after §51's E block".** Block E and block 51 wait for the F2 fix (§51 Amendment 2), and §61 does not, so 61 runs first.
- **Launch tree.** The reviewed base is `harness61` `f86b4d16`: tools only, on `harness60-a2` `1ecee59d`.
  - **The launch sha is `harness61` `14e2bacd22f1572f8a9e79898d59610fbcf8f15c`.** It closes harness items 1–7 (below) in `da139d02`, plus `14e2bacd`, which corrects the PRICE header's seed label. Both are tools-only, and both are on `github`.
  - At `14e2bacd`, the engine diff against `1ecee59d` is still empty, including `ref60-manifest.sha256` and `lib.sh`. `test_harness61` passes in the worktree, 51 ok, witness included.
  - At `f86b4d16`, `git diff` against `1ecee59d` and `cb88df3a` over `raptorpath raptorpath-math raptorpath-wasm raptorpath-core gf256 Cargo.toml Cargo.lock` is empty. The diff against `1ecee59d` is also empty for `tools/l1/ref60-manifest.sha256` and `lib.sh`. `test_harness61` asserts both in a git worktree.
  - Every launch must log `bin_sha` = `e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d` before any row, or it aborts (`ABORT-BIN-SHA`). The abort is registration §7's rule. What follows a mismatch (a re-pin Amendment) is the coordinator's answer to owner question 2 (below).
- **F2 does not reach this session.** Every 61 arm runs the Bulk hint (`plan61.arm_hint` returns `bulk`). Bulk does not pack (F2 entry: "Bulk and Auto do not pack"), so the receiver's default `window_packed` = false matches its framing.
- **Also registered by reference: `s61_score.py`'s 16 DECISIONS at the launch sha `14e2bacd`.**
  - Each is one reading of a place the registration is silent. The table below was reviewed at `f86b4d16`, and each row's text describes that sha. Rows 5, 6 and 14 are superseded at `14e2bacd` by items 3, 1 and 7, and the DECISION texts there say so. At the launch sha the list is printed in the order 1–13, 15, 16, 14, so each is cited here by number and topic. DECISION 16 (the pilot's 0.1 resolution, ρ̄'s CI and the native readings) is items 4–6 below.
  - Two of them change a rule (5 and 6). They are registered as rewritten by items 3 and 1 below, not as they stand at `f86b4d16`.
  - Item 4 adds one reading: Ŵ is registered at 0.1-slot resolution.

  | # | topic | against the registration |
  |---|---|---|
  | 1 | inputs: window p50 = `[WSTAT]` p50; tmin and nk from `[SLAW]`; G pools windows with nk ≥ 1, and the native readings use nk ≥ 10 | fills a silence with `s61pred`'s own window sets. It reproduces `s61pred` exactly on §60's rows (pooling check below). |
  | 2 | pilot-row bytes 110 / 115 MB at W 88 / 128 (§1's formula), with no `LD_KNEE61` at the rehearsal | fills a silence: §1 prices "at and above the knee bracket", and the rehearsal has no bracket. The 120 s timeout covers the slow basis (≈ 71 s). |
  | 3 | the pilot's non-monotone pair u₈₈ ≥ 0.95 > u₁₂₈ falls to the last case, Ŵ = 88·0.95/u₈₈ (≤ 88, then clamped) | adds a fifth case to §2.2's four. The pilot control is R1 or R6, as §2.2 says. |
  | 4 | the knee: W₉₅,r by `cross_up` over every non-control arm; T̂min_r(W₉₅,r) interpolated over the K and P1 arms (G(2 100) at c3); σ_r pools the `sig` windows of the two bracketing arms | literal, except that σ_r pools windows where §5 says "median LATE sig over the bracketing arms". That fills a silence and decides nothing scored. |
  | 5 | with fewer than 2 floor pools, the floor pool's medians stand, W\* = the pool, and the floor reads as scoreable | **changes a rule.** It bypasses §5's UNSCOREABLE-FLOOR, since W\* = u₂/κ needs no line. It also differences an unshifted c3 against c2's shifted W → 0 draws. Item 3 replaces it. |
  | 6 | W → 0 = F − b·W₁, S_F − b·W₁; D₀ = D + b_F·W₁ | **changes a number** when W\* ≠ W₁. Replaced by the W → 0 reading below (item 1). |
  | 7 | c2 frozen (`C2_FROZEN`), ported draw for draw from `s61pred.py`, seed 60; paired draw b = c3's draw b − c2's draw b | literal (§5.2). |
  | 8 | σ₃ = the median over scoreable replicates of σ_r; σ₂ 1.785 and S_F,2 5.083 frozen; Δ = min(c2's 0.085, c3's q_R·F_c/S_F,c) | literal (§5.2). Δ is held at its point value in every draw, which fills a silence. |
  | 9 | CR61: e_own against the row's native median LATE tmin; X_own from the c3 W 12 rows (live, not DNF), and 10.66 at c2; a row with no ticks is not flagged; the arm's goodput median includes DNF rows at 0 | literal for e_own and X_own. The last two fill silences. |
  | 10 | CONTROL-MOVED with fewer than 4 unflagged live controls; κ = median over valid floor rows of u_row/W | literal (§4.1, §5). The "session median control gp" is the median over the unflagged controls, which fills a silence. |
  | 11 | the rehearsal's go/no-go mechanics; the control loss is the leg-0 `[TRUTH]` loss against 0.0476 + 0.01; `LD_NOGO=floor` makes the scorer start the floor pool at W 16 | fills silences. Under `LD_NOGO=floor`, 61a still runs its W 12 rows, and they stay in the ledger and in CR61's X_own reference (§4.1 says "from the c3 W 12 rows"). |
  | 12 | the label names (`LABEL61 …`, `VERDICT61`) | mechanism only. |
  | 13 | "AGREES (S or σ-scaled)" when \|SIGMA-DISCRIMINATION\| ≤ Δ | literal (§5.2). |
  | 14 | the price and landing tables' per-arm 90 % intervals (2 000 draws), and the monotonicity list | fills a silence. At `f86b4d16`, c2's table used seed 61 where §5 gives seed 60 at c2; item 7 fixed it. §60's merged arms carry frozen medians with no interval. That is reported, and the table decides nothing. |
  | 15 | τ = 0.05·S_F,c with S_F,c the u-matched line value; κ against the session median control; the c2 landing table merges §60's arms on DRIFT-OK | **confirmations, not changes.** §5 defines S_F,c at c3 as the line value at W\*, §5.1 sets τ from it, and §5 sets u_row against "the session median control gp". Only the order of computation (fit first, label after) is the harness's. W\* is held at its point value across draws (κ is not resampled), which fills a silence. The DRIFT-OK merge is §5.4's. |
- **The W → 0 reading (registered here; it replaces DECISION 6).**
  - **The gate is untouched.** §5.1's gate keeps E = b·W₁ and its labels. Only the bracket's offset becomes b·W\*, so §5.3's "D + E_F" is read as D + b_F·W\*.
  - **What is scored.** ρ, Δρ and D are read at the u-matched floor at both cells: c3 on the gate's lines at W\* = u₂/κ, and c2 natively at W 16, which is u₂ by definition. The harness does this, and so does the falsifier's AGREES / DISAGREES. W → 0 is a bracket. It can only replace a definite label with FLOOR-SENSITIVE, when it gives the opposite definite label (§5.1, §5.2, §5.3).
  - **W → 0 is the intercept at both cells:**

    ```text
       F₀ = F_c − b_F·W*  = a_F        S₀ = S_F,c − b_S·W*  = a_S        D₀(A) = D(A) + b_F·W*        (slopes refitted per draw)
    ```

    - At c2, W\* = W₁ = 16, so the frozen ρ₂,₀ = 0.877 [0.858, 0.908] stands unchanged.
    - At c3, the harness at `f86b4d16` subtracted b·W₁ from F_c read at W\*. That gives a + b·(W\* − W₁), which equals the intercept only when W\* = W₁.
  - **Reasons:**
    1. The sentence names "the extrapolated W → 0 floor".
    2. In the first commit (`aa05eacd`, line 408), the readings were "printed at the floor pool (scored) and at the extrapolated W → 0 floor". There F was read at W₁, so F − b·W₁ was the intercept. Advisor round 2 moved c3's F to W\* and left the bracket's formula as it was.
    3. c2's own shift (+0.080) spans the line from u₂ to zero. c3 must span the same distance, or Δρ₀ compares unequal offsets.
    4. Whenever W\* < W₁, the hybrid evaluates the line at a negative W.
  - **Size of the difference, b_F·(W\* − W₁)/S_F,3**, on the prediction's inputs: W\* 10–13 and S_F,3 8.5–12.5. The c2-scaled slope is b_F = 0.067–0.083 ms/slot (E_F ≈ 0.8–1.0 ms at W₁ = 12, §5.1). c8 leg 1's is +0.0133.

    | case | W\* − W₁ | c2-scaled slope | c8 leg 1 slope |
    |---|---|---|---|
    | nominal floor, W₁ = 12 | −2 to +1 | ≤ 0.020 in ρ (≤ 0.17 ms) | ≤ 0.003 |
    | after a floor move or `LD_NOGO=floor`, W₁ = 16 (W\* at most 6 slots below, §5) | −6 to −3 | ≤ 0.059 in ρ (≤ 0.50 ms) | ≤ 0.009 |

    - Against Δ = 0.085 that is up to a quarter of Δ nominally, and up to 0.7 Δ after a floor move. Since ρ₂'s own W → 0 shift is 0.94 Δ, it is enough to move a FLOOR-SENSITIVE call.
    - A synthetic check confirms it. On a floor sloped at 0.093 ms/slot with W\* 10.2 and W₁ 12, the harness at `f86b4d16` printed ρ₃,₀ 0.941, where the intercept gives 0.924 (derived from the printed line: F_c 32.845, b 0.0928, need 8.296, S_F,c 10). At `14e2bacd`, item 1's test asserts the intercept to 0.0006 on a floor sloped at 0.09 ms/slot.
    - With the c2-scaled slope, the S_F term (b_S −0.005 to −0.009) changes these numbers by under 10 %. With c8 leg 1's slope it is a larger share (30–60 %) of a negligible number.
  - **D₀ was not printed at `f86b4d16`; item 2 prints it.**
    - The harness computed D₀ only to set a `FLOOR-SENSITIVE` suffix on the D labels. §5.1 says every scored reading is printed at both floors, an UNRESOLVED W → 0 reading with its interval.
    - So D₀ is reported, and it gates only FLOOR-SENSITIVE (item 2).
    - The allowance's terms are not recomputed at W → 0, as §5.3's "D + E_F" states. For the decided form S, the omitted term is E_S + q·E_F ≈ −0.09…+0.04 ms (E at W\* ≈ 11; b_S −0.005 to −0.009, b_F 0.013–0.083), against c3's predicted D half-width of 0.2–0.3 ms. For wq it is 0.26·E_F ≈ 0.2 ms, on a predicted D of −3.9 to −7.1.
- **Harness items, closed at `da139d02`** (a Sonnet worker on `harness61`, tools only; each item with a test that fails first). Coordinator review of the diff: all seven match the text below, and the pilot sweep (3 481 pairs) confirms `arms(printed W)` equals the arms placed. DECISIONS 5, 6, 14 and the new 16 at the launch sha carry these readings:
  1. **W → 0 at the intercept** (done).
     - In `run_c3`'s bootstrap, `ef, es = fit_f[0] * w_star, fit_s[0] * w_star`, not `* fpool`.
     - On the `W->0:` point line, `f_c - gF['b'] * w_star` and `s_c - gS['b'] * w_star`.
     - D₀ = D + b_F·W\*.
     - Rewrite DECISION 6's text to match.
     - Test: a synthetic ledger with a linear floor F = a + b·W and W\* ≠ W₁ asserts ρ₃,₀ = median (T − a_F)/a_S absolutely. The existing synthetic floors are flat, so this path is untested end to end.
  2. **Print D₀.** For every candidate and hint, print D₀ (point, 90 % interval and label) beside the scored D, as §5.1 / §5.3 require. Test: the line is present and carries the interval.
  3. **The single-pool case (replaces DECISION 5).**
     - Compute W\* = u₂/κ in every case and apply §5's 6-slot rule against the lowest valid pool. With one pool, its medians stand as F_c and S_F,c only if W\* ≥ W_pool − 6; otherwise the reading is UNSCOREABLE-FLOOR.
     - With fewer than 2 pools, the W → 0 bracket and FLOOR-SENSITIVE read `NOT-COMPUTED` (no slope; nothing is extrapolated below the floor pool), and are never differenced against c2's shifted draws.
     - Tests:
       - a ledger where only W 24 holds valid rows reads `LABEL61 RHO UNSCOREABLE-FLOOR`. At `f86b4d16` it scores AGREES at W 24, and its W → 0 bracket prints [−0.110, −0.061] against c2's 0.877;
       - these ledgers need sample counts inside both parity bands. `test_s61`'s `synth_ledger(nk_per_slot=3.5)` gives floor n 252 and knee n 2 303; the default of 5 reads UNSCOREABLE-N first, so the test would pass for the wrong reason. The stop order is UNSCOREABLE, then UNSCOREABLE-N, then UNSCOREABLE-FLOOR, and the test asserts it reaches the floor branch;
       - a ledger where only W 12 holds valid rows scores, with W → 0 `NOT-COMPUTED`.
  4. **The pilot prints its case.**
     - `pilot_knee` returns the case it used: `straddle` / `secant` / `degenerate` / `both-above` / `non-monotone`, and `clamped=yes|no`.
     - `PILOT61` prints both, from `s61_score.rehearsal` and `plan61.py pilot`.
     - Ŵ is registered at 0.1-slot resolution. It is rounded to 0.1 before `arms()`, so the printed `W=`, which is what `LD_KNEE61` receives verbatim, reproduces the printed arms.
       - At `f86b4d16` the arms come from the unrounded Ŵ. Over a sweep of 12 003 (u₈₈, u₁₂₈) pairs, 404 printed W values derive different arms, for example Ŵ 123.85 gives K₂ 132 where the printed 123.8 gives 128.
       - The rounding moves Ŵ by ≤ 0.05 slot, against a pilot error of −5 % to +14 %.
     - Keep the line's existing prefix `PILOT61 W=… u88=… u128=… arms=…` and append `case=… clamped=…`, since `loopdiag_run_all.sh` greps the line. `pilot_knee` keeps its float return (existing tests call it), and the case comes from a separate function or an optional return.
     - u₁₂₈ ≤ 0 or u₈₈ ≤ 0 reads `PILOT61 none`, not a division error.
     - Tests: every case name, and `arms(float(printed W)) == printed arms`.
  5. **ρ̄ with its 90 % CI** (§5.2: "ρ̄ … with its CI"). Take it draw by draw from the same 2 000 draws, pooling c3's and c2's resampled per-replicate values in draw b. Do not add any RNG call to the ported c2 stream, so ρ₂ and ρ₂,₀ still reproduce.
  6. **The native ρ₃ and D-S** (§3 "the native reading is printed beside", §5.5 "every reading native at c3"). Print them, also under UNSCOREABLE-N. They decide nothing.
  7. **Seed 60 for c2's price-table intervals** (§5 symbols).
  - Items 1–4 change what the rehearsal or 61a prints or labels. Items 5–7 are reported outputs, done in the same commit.
  - After the change: `test_s61`, `test_harness61`, `test_stubrun.py` (fast, unfiltered) and every `tools/l1` python test pass in an LF export of the new head (recorded under the host tests below).
  - **Red-first** (coordinator, on an export of `f86b4d16` carrying the new `test_s61.py`): items 1, 2, 3, 5 and 6 fail there, and the run stops at the pilot block with `AttributeError: … pilot_detail`. So item 4 is red only as a missing function, and item 7's seed test was not reached.
  - **Pilot cross-check** (coordinator): `round(pilot_knee, 1)` equals `pilot_detail`'s Ŵ on 39 601 pairs. The 3 481-pair arms sweep is the worker's test.
- **The sample-count pooling, checked on real c3-shaped rows.**
  - **Method.** The scorer's own parse (`s61_score.row_extras`, with leg 1's `[SLAW]` / `[WSTAT]` lines renamed to p = 0, since the scorer reads leg 0) and its `reading(…, 250)` run on §60's c8 leg 1 G rows (`rep60a-diag-ld.tgz`).
  - **G(250) at W 32 / 64 / 96 / 256, 24 rows.** The result matches `s61pred.row_SF` row by row (max difference 0 over S_F, F, n and groups) and reproduces `s61pred.out` §1 / §3:
    - S_F 12.336 / 11.922 / 12.829 / 11.290;
    - F 33.493 / 33.826 / 33.611 / 36.337;
    - realised n 255 / 257 / 261 / 294, with 6 / 13 / 24 / 67 groups;
    - pooled S_F 12.067, and F at W 32–96 33.611;
    - the gate over the four pools gives S_F b −0.0046 ± 0.0011 (E −0.15) and F b +0.0133 ± 0.0011 (E +0.43).
  - **G(2 100) at W 256–1 280, 30 rows.** It is identical to `s61pred.row_SF` row by row. The realised n per group is 2 168–2 382, inside the knee parity band [1 874, 2 928]. The floor values (255–294) are inside [226, 353].
  - **Scope.** This tests the grouping and the parse on real rows. The u-matched line at W\*, κ and the W → 0 path are tested only on synthetic ledgers, with item 1's sloped-floor test among them.
- **Host tests at `da139d02`, LF `git archive` export** (worker run). `14e2bacd` changes one printed label, and after it `test_s61` and `test_harness61` were re-run in the worktree: ALL OK.
  - `test_s61` 75 ok, 0 skips, with `S61_S60_DIR` set; `test_harness61` 48 ok plus the witness skip (51 ok in the worktree); `test_harness_paths` 12 ok; `test_s60` 190 ok and `test_harness60` 41 ok (unchanged); `test_stubrun.py` fast and unfiltered, ALL OK, 672 ok, and filtered to the ten 61a / 61b paths, ALL OK.
  - **Coordinator, at `14e2bacd`** (LF export, 2026-11-17):
    - every `tools/l1/test_*.py` (44 files) exits 0 except `test_r_parse` and `test_s60`;
    - both of those fail only in their child-process steps, on a Windows host decode (the child writes `§` in cp1252). `test_r_parse` passes 73/73 with `PYTHONUTF8=1`, and it fails identically at `f86b4d16`. `test_s60`'s failing step runs `python -I`, which ignores that variable, and neither file nor its scorer changed on this branch;
    - the fast unfiltered `test_stubrun.py` gives ALL OK, 672 ok, with `PYTHONUTF8=1` (without it, 2 FAIL, from the same decode);
    - the `STUBRUN_FULL=1` 61a reuse-hit run gives ALL OK (66 rows, 415 s);
    - the launch's own python and bash tools tests run on the VM (Linux, UTF-8) before any row, and they bind.
  - `r68allow.py` is now on `harness61` (from `6794780d`), so `s61pred.py` runs from the branch alone.
  - **Earlier counts, at `f86b4d16`:**
  - `test_s61` gives 62 ok, 0 skipped, ALL OK. The export had `r68allow.py` placed in `docs/derivations/store-law/`, from `6794780d`, the same blob as on `github/main`. It is not on `harness61`, so the registered `s61pred.py` cannot run from that branch alone. The test's host block looks for it in the tree. and `S61_S60_DIR` pointed at `rep60a/b-diag-ld.tgz` from main. Without those inputs it gives 57 ok, and 61 ok with the tgz alone.
  - `test_harness61` gives 48 ok plus one skip in the export: the engine-diff witness, which needs git. It gives 51 ok in the `harness61` worktree, which includes the witness's three git checks.
  - Two checks run inside `test_s61` on §60's real rows:
    - CR61 flags 0 of 48 c2 G rows; `knee_of` gives W₉₅ 165.7–169.7; the native floor pairs and X_own 10.66 reproduce;
    - the c2 bootstrap reproduces ρ₂ = 0.797 [0.780, 0.825] and ρ₂,₀ = 0.877 [0.858, 0.908].
  - Reported by the harness worker and not re-run for this amendment: every `tools/l1` python test (44 files, 2 146 ok); the unfiltered fast `test_stubrun.py` (672 ok, with every 61a / 61b path); the `STUBRUN_FULL=1` 61a reuse-hit run (66 rows). `test_harness60` and `test_s60` are unchanged.
- **Before the rehearsal, on the VM, from the extracted archive** (Linux only; Git Bash cannot run them): `bash tools/l1/test_abort_witness.sh` and `bash tools/l1/test_lib_battery.sh`. Both must pass (registration §9's done criteria). A failure stops the session before any row.
- **Run rules for every 61 launch, the rehearsal included:**
  - **Host cargo is paused** (the host build lock is held by the main session) for the whole launch. §60's registration carries this rule (`s60-registration.md` §5, "Host: host cargo is paused") and the s61 registration omits it, so it is added here.
  - **`LD_HARD`:** launch + 4.0 h for the rehearsal and for 61a, and + 1.8 h for 61b. The budget guard applies under `LD_REHEARSE=1` too: `loopdiag_run_all.sh` sets `P57=1` for 61a / 61b and has no rehearsal exemption, so it requires 14 220 s for any 61a launch. The rehearsal therefore launches with 61a's `LD_HARD`, as §60 Amendment 1 did.
- **The registration's owner questions were answered as the coordinator's choices** (§61 stub; reported to the owner, not decided by them):
  - Δ stays 0.085;
  - a rebuild that does not reproduce `e410d640` stops the session for a re-pin Amendment, with a suites run and Fable's review.
- **Not yet run:** the real `perf_rwm_c.sh` c3 path (`topo.sh up c3`). The rehearsal is its first run.
- **Order:**
  - one `git archive` of `14e2bacd`, extracted fresh under `~/r61`;
  - the two shell tests;
  - the 61 rehearsal (`LD_PLAN=61a LD_REHEARSE=1`);
  - 61a, with `LD_KNEE61` = the rehearsal's printed `PILOT61 W=`, plus `LD_CTL61` / `LD_NOGO` as the rehearsal prints them;
  - 61b, with `LD_BIN_PREBUILT`, which may run before 61a if the VM slot suits (§8).

**§61a Result (2026-11-17; the c3 sweep ran clean and scored nothing: CONTROL-MOVED).**

**In plain words.**
- All 66 rows ran, on the registered binary, with no void, DNF or replaced row.
- **The registered outcome is CONTROL-MOVED,** so nothing at c3 is scored. CR61's ratio clause flagged all six W 2 048 controls (ratio 1.51–1.90). By goodput and qshare those controls look healthy: 17.70–17.83 Mbit/s, qshare 0.
- **The knee arms were also placed about 10 % too high.** So even without those flags, no replicate's knee falls inside the knee bracket.
- **Both are defects in the registration's rules.** Why the c3 control's rtt tail is so wide is not diagnosed.
- **A post-hoc bound,** read outside the registered rules: c3's knee margin is 0.31–0.56 of S_F, against c2's 0.80. The re-run (Amendment 2) is therefore not blind, and it is registered knowing that.

**Runs.**
- **Rehearsal:** launch 06:08:45Z, `REHEARSAL-PASS` 06:30:22Z.
  - floor GO (2/2); control GO (R1 17.77 vs 17.40; loss 0.0486 ≤ 0.0576);
  - pilot `PILOT61 W=119.5 u88=0.818 u128=0.985 case=straddle clamped=no`, giving K 112/124/136, B 68/92/104, P1 176.
- **61a:** launch 06:31:18Z, with `LD_KNEE61=119.5` and the defaults `LD_CTL61` unset (the 2 048 control) and `LD_NOGO` empty, both as the rehearsal printed. Rows ran 06:47–07:53Z.
  - `bin_sha` equals the registered `e410d640…`; suite-reuse HIT (`ref60`, 187 files);
  - the python and bash tools tests pass on the VM, `test_s60` and `test_r_parse` included (`suites/TESTS.txt`, rc 0). The host failures of Amendment 1 were Windows-only;
  - `VERDICT61 REPORTED-WITH-SCORED slots=66/66 live=66 void=0 replaced=0 dnf=0`;
  - host cargo paused throughout.
- **Reproduction:** the host re-score with `LD_KNEE61=119.5` matches the VM `score.txt` byte for byte.
- **Ledger:** `docs/l1-raw/s61/`, scrubbed. It holds:
  - the 61a ledger, era log, smoke check and both scores;
  - the rehearsal (`rehearsal.tgz`);
  - the post-hoc and path-exercise outputs named below.

**Registered labels.** `LABEL61 CONTROL-c3 CONTROL-MOVED`; `LABEL61 RHO NEEDS-MORE-control-pool`.

**Defect 1: CR61's ratio clause cannot separate health from collapse at a large control pool** (registration §4.1). This is a defect finding of the registration's collapse rule.

| arm | E90_own / (p90 − tmin)_own, 6 rows | goodput |
|---|---|---|
| K₁ 112 | 0.87–0.97 | 17.06–17.33 |
| K 124 / 136 | 0.89–0.97 | 17.62–17.85 |
| P1 176 | 0.95–1.02 | 17.71–17.97 |
| control 2 048 | **1.51–1.90** (E90_own 33.3–46.5 ms over 20.3–25.0 ms) | 17.70–17.83 |
| rehearsal R1 2 048 | **1.57** (38.75 / 24.71) | 17.77 |
| rehearsal R6 512 | 1.07 (23.89 / 22.24) | 17.75 |

- **The 1.5 was declared post hoc on §60,** between c8's healthy 1.40 and c1s G r1's 1.82.
  - Healthy c1s controls in §4c reach 1.32. The c3 controls span 1.51–1.90 at full goodput.
  - At this pool the ratio does not separate, and c1s G r1's 1.82 lies inside c3's healthy range.
- **c2's controls read 0.97–1.04** (§4c), so the defect does not reach 61b. c2's control also stands far above its BDP (a ≈ 195 ms standing queue), so "a large standing queue" alone does not explain c3.
  - c3's ≈ 1.1 s standing queue near netem's 1 000-packet limit is the case the registration's own §1 worried about. It is a candidate cause, not diagnosed.
- **The rehearsal's go/no-go missed it.** The floor go/no-go includes "not COLLAPSED (CR61)", but the control go/no-go does not. R1's ratio was 1.57.

**Defect 2: the pilot's straddle case overshoots on a concave u(W)** (registration §2.2).
- **The registration analysed only the secant case,** which reads about 4 % low. A straddle interpolates linearly between a point below 0.95 and a point on the plateau. On a concave curve that line reaches 0.95 later than the curve does.
- **On c3:**
  - the in-session u is W 92 0.852, W 104 0.934, W 112 0.966, W 124 0.995;
  - per-replicate W₉₅ is 106.8–109.7, against Ŵ 119.5 (−8 % to −11 %, outside the registered −5 % to +14 %);
  - 61a's own curve gives the same straddle (u₈₈ ≈ 0.817, u₁₂₈ ≈ 0.99, giving ≈ 119).
- **On c2 (§60's curve):** a straddle between W 160 (0.930) and W 256 (1.002) gives ≈ 187, against the knee of 168 (+11 %).
- **Consequence:** every replicate's W₉₅ lies below K₁ = 112, so all six are UNSCOREABLE (below-K1).

**Post-hoc readings (NOT scored).** The ratio clause is lifted and every other rule is as registered (`posthoc-no-ratio-clause.txt`).
- **Control and floor:**
  - control OK 6/6, median 17.78;
  - floor pool W 12, W\* 11.5, F_c 38.682, S_F,c 9.563;
  - GATE F NOT-CONVERGED (b +0.0895 ± 0.0240 ms/slot); GATE S_F UNRESOLVED (b −0.0221 ± 0.0267);
  - parity IN at both floor (296) and knee (2 396).
- **Knee: 0 of 6 replicates scoreable** (below-K1).
  - The printed Tmin(W₉₅) is K₁'s own G(2 100) row minimum. The knee interpolation runs over the K and P1 arms only (DECISION 4) and clamps at K₁, so the printed "chord" values are K₁'s.
  - Since tmin rises with W and every W₉₅ lies in [104, 112], W 104's row minimum bounds need from below and K₁'s from above.
  - Per replicate, ρ₃ lies in **[0.31–0.46, 0.50–0.56]**. These are point values with no interval, against ρ₂ 0.797 and Δ 0.085.
- **The bound is labelled post hoc, not a verdict.** The re-run is registered knowing that ρ₃ likely falls well below ρ₂.
- **Path exercise** (`path-exercise-k104.txt`; not a reading).
  - W 104 is counted as K₁, with K 104/112/124 and P1 136, and the ratio clause is lifted. This runs the scorer's whole scored branch on real c3 rows for the first time: bootstrap, Δρ, W → 0, σ-discrimination, D and D₀ per candidate and hint, native ρ₃, and ρ̄'s CI.
  - It runs to its labels with no error. The printed numbers fall inside the bounds above (ρ₃ 0.458), and they are not a result.
- **B1** (reported only): 0/18 fire. One floor row, G12 s42 r5, has margin +101.2 ms, against 0.4–9.0 for the rest. It is listed and not investigated here.

**Pending, added by this Result:**
- **CR61's ratio clause at large pools.** It is a defect finding of the collapse rule. Amendment 2 routes around it (a W 512 control) and does not fix it. A rule that separates collapse from health at a large pool must be derived before any reading uses a ≥ 2 048 control at c3.
- **The c3 control's wide rtt tail** (E90_own 33–46 ms over a 20–25 ms window spread): undiagnosed. Netem's 1 000-packet limit under a ≈ 1.1 s standing queue is the candidate.
- **The G12 s42 r5 B1 outlier** (−101 ms): not investigated.
- **For the owner, as a heads-up and not a finding:** the post-hoc bound points toward DISAGREES at c3. If the re-run confirms it, S_F is not the knee's term at a second leg shape, and decision 8's form (g·S_F) is reopened.

**§61 Amendment 2 (2026-11-17, before any VM contact for the re-run; Fable reviewed it twice, both rounds applied): 61a re-run after a new rehearsal.** The launch sha is pinned by a further amendment once the harness items land. Each fix is justified on control data, on the registration's own text, or on geometry. The pilot's direction comes from geometry, and its size is checked post hoc on 61a's and c2's curves. That is acceptable because arm placement touches no scored rule, and the scoreability rule is unchanged.
1. **The re-run's control is W 512, registered outright** (`LD_CTL61=512`).
   - **Why:** 7 of 7 W 2 048 rows this session read ratio 1.51–1.90 (the six 61a controls and the rehearsal's R1), so a 2 048 control would read CONTROL-MOVED again.
   - A rehearsal gate alone would not prevent that: one healthy R1 at 1.49 would pass, and 61a's six controls would then flag.
   - R1 (W 2 048) stays in the rehearsal as a reported row.
   - **Enforced by the harness:** a 61a launch, the rehearsal included, refuses (`ABORT-CONTROL61`, nothing run) unless `LD_CTL61=512`.
     - The global default is not flipped: `plan61.py plan 61b` prints the same rows before and after (61b's control stays W 2 048).
     - A test asserts that an R1 with ratio < 1.5 does not bring W 2 048 back.
   - **Prediction.** The six W 512 rows read ratio 0.95–1.3. The evidence is R6's 1.07 and P1's 0.95–1.02, with the rise toward 2 048 between.
     - u does not move: R6 / R1 goodput = 0.999 this session, so W₉₅ and κ stay where they are.
     - **If the re-run reads CONTROL-MOVED again,** there is no third session until CR61 is re-derived.
2. **R6 (W 512) carries the control go/no-go.** It is GO iff all of these hold:
   - R6's goodput ≥ 0.98 × max(R4, R5);
   - its loss truth ≤ 4.76 % + 1.0 pp;
   - it is not COLLAPSED under CR61.
   - On a single row, CR61's goodput cut compares the row with its own median and never fires, so only the ratio clause (≤ 1.5) is live. A failure is `NOGO-CONTROL`. This session's R6 read 1.07.
   - CR61 and §4c's table are unchanged, so c1s G r1 stays flagged. 61b is unaffected (c2 control ratios 0.97–1.04).
3. **The pilot's straddle case takes the proportional form** Ŵ = min(88·0.95/u₈₈, 128), as the both-above case already does. The cap holds because a straddle already bounds the knee at 128 or below, and the form exceeds 128 only when u₈₈ < 0.65.
   - **Why:** for a probe below the knee, u/W falls with W (u is concave), so proportionality from that probe reads low. In the both-above case, the same geometry makes it read high.
   - The registration's bracket, before r4 rounding, covers Ŵ/W₉₅ in [0.877, 1.053]. On c3's curve the rule's bias is 0.893 (W 12) to 0.948 (W 92). On c2's it is 0.973 at u 0.93, and 0.808–0.846 at u ≤ 0.32. c2's curve has no point between.
   - **Arms and margins over the plausible u₈₈** (two measurements this session: 0.817 and 0.818), against this session's W₉₅ 106.8–109.7:

     | u₈₈ | Ŵ | K | K₁ margin | K₃ margin |
     |---|---|---|---|---|
     | 0.79–0.80 | 105.8–104.5 | 100 / 108–112 / 120 | +6.8 % | +9.4 % |
     | 0.81 | 103.2 | 100 / 108 / 116 | +6.8 % | +5.7 % |
     | 0.82–0.83 | 102.0–100.7 | 96 / 104–108 / 116 | +11.3 % | +5.7 % |
     | 0.84–0.86 | 99.5–97.2 | 92–96 / 104 / 112 | +11.3–16.1 % | **+2.1 %** |

   - At u₈₈ ≥ 0.84, K₃ = r4(1.14·Ŵ) rounds down to 112. A replicate above it is UNSCOREABLE (above-K3) by the registered rule, which decides.
   - Implementation: once. `pilot_knee` becomes a wrapper of `pilot_detail`, and the case name `straddle` becomes `proportional`.
4. **Everything else stands as registered,** including Δ, ρ₂ (frozen), the floor rules and the D candidates.
5. **Harness and tests, for a new tools-only launch sha** (the engine and `bin_sha` are unchanged):
   - red-first tests for R6's go/no-go (each of the three clauses), the renamed pilot case, and `pilot_knee` as a wrapper of `pilot_detail`;
   - a `STUBRUN_FULL=1` 61a run with `LD_CTL61=512`, including the smoke's control row;
   - the fast unfiltered stub run, and every `tools/l1` test, on an LF export.
   - Multi-job stub runs wait until 61b prints `LD-ALL-DONE`.
6. **Texts the new sha rewrites:**
   - DECISIONS 3 (pilot cases), 11 (the rehearsal's go/no-go) and 16 (`case=` names);
   - Amendment 1's "`LD_KNEE61` = the rehearsal's printed `PILOT61 W=`, plus `LD_CTL61` / `LD_NOGO` as the rehearsal prints them". For the re-run this reads `LD_CTL61=512` always, with `LD_NOGO` as printed;
   - Amendment 1's DECISIONS table rows 3 and 11.
   - Each is named as superseded at the new sha.
7. **Order:** a fresh rehearsal (R6 gate, proportional pilot), then a fresh 66-row 61a, from one archive of the new sha.
8. **Not blind.** The re-run is registered after this session's post-hoc bound. Its label is still decided by the registered rules, and its Result reports both readings side by side.

**§61 Amendment 3 (2026-11-17, before any VM contact for the re-run; reviewed by Fable): the launch sha and the run rules for the re-run.**
- **Launch tree: `harness61` `3cdc447081ab2f83e58195b6358ea8ef206ddda1`,** tools only, two commits on `14e2bacd`.
  - `1b6da5e0` holds the code and tests. `3cdc4470` only writes `1b6da5e0` into DECISIONS 3, 11 and 16 as the superseding commit, since a text cannot name its own commit.
  - The engine diff against `1ecee59d`, including `ref60-manifest.sha256`, is empty, so `bin_sha` stays `e410d640…`.
- **Coordinator review of the diff (code only; the test diffs were read by their results, not line by line):**
  - `ABORT-CONTROL61` refuses a 61a launch without `LD_CTL61=512`.
  - The rehearsal plan has six distinct arms: R1 G2048, R2/R3 G12, R4 G88, R5 G128, R6 G512.
  - With `LD_CTL61=512`, 61a's 66 rows carry six G512 controls and no G2048. 61b's plan is identical with and without the variable.
  - R6's go/no-go has three clauses, matching Amendment 2. R1 prints as a reported row with its ratio.
  - `pilot_knee` is a wrapper of `pilot_detail`.
  - **Timing:** `ABORT-CONTROL61` fires after the build and the `bin_sha` check, before any row. Its "NOTHING WAS RUN" means no row ran; the locks were taken and the build ran.
- **The worker's three readings, registered:**
  - the "smoke's control row" coverage is the `STUBRUN_FULL=1 61a/reuse-hit` run at W 512 (61a's smoke is the single K₂ row);
  - an R6 with no `[DIAG]` ticks is NOT-GO, a strict reading;
  - `pilot_knee` returns the 0.1-rounded Ŵ.
- **Host tests** (LF `git archive` export of `3cdc4470`, `PYTHONUTF8=1`; worker run):
  - `test_s61` 83 ok, 0 skips; `test_harness61` 50 ok plus the witness skip (53 ok in the worktree, witness included); `test_harness_paths` 12 ok;
  - fast unfiltered `test_stubrun.py` ALL OK, 674 ok, including the new `61a/control-missing` path;
  - `STUBRUN_FULL=1 61a/reuse-hit` with `LD_CTL61=512` ALL OK, 66 rows;
  - every `tools/l1/test_*.py`: 43 of 44 exit 0. `test_s60`'s `python -I` step fails on the host only (Amendment 1); it passed on the VM in both 61a and 61b.
  - **Red-first:** on `14e2bacd`'s code, `test_s61` gives 14 FAIL (the proportional pilot, cap, wrapper and case name; R6's three clauses; the R1 cases; a DNF R6). `test_harness61` stops on the missing `control_w(ctl)`.
  - Not seen red: the 61b-unchanged guard (a regression guard, green before and after) and `61a/control-missing` against the old `run_all`.
- **The W 512 control's prediction and stop rule:**
  - **Predicted ratio for the six W 512 rows: 0.95–1.4.** The evidence is this session's R6 (1.07), P1 at W 176 (0.95–1.02), and the rise toward W 2 048 between them.
    - c8 leg 1's healthy arms in §4c reach 1.40 (W 1 280). That is context only: §60's per-slot goodput pools both legs, so leg 1's own knee cannot be read, and no pool-to-knee scaling is claimed.
  - **The rehearsal decides first:**
    - R6 ratio > 1.5 reads `NOGO-CONTROL`, as registered;
    - R6 ratio in (1.4, 1.5] is **stop-and-report**: 61a does not launch until a further amendment, since a control near the line can flag in 61a;
    - R6 ≤ 1.4 goes ahead.
  - **Before 61a,** the coordinator reads R6's ratio, u₈₈ and the printed arms from the rehearsal's output, not just its PASS.
- **Out-of-sample check of the proportional pilot (61b, c2).** At 61b's W 96 / 128 / 144 (u 0.611 / 0.783 / 0.861), Ŵ/W₉₅ against the knee of 168 is 0.888 / 0.924 / 0.946. All are inside the bracket's [0.877, 1.053]. These points fill the gap Amendment 2 named.
- **Run rules:**
  - both launches (rehearsal and 61a) and the host re-score carry `LD_CTL61=512`;
  - `LD_HARD` = launch + 4.0 h for each;
  - host cargo is paused for each launch: the coordinator holds the host build lock, and a worker's cargo waits at it;
  - one `git archive` of `3cdc4470`, extracted fresh for each launch.
- **Order:** the re-rehearsal (`LD_PLAN=61a LD_REHEARSE=1 LD_CTL61=512`), then 61a (`LD_KNEE61` = the printed `PILOT61 W=`, `LD_CTL61=512`, `LD_NOGO` as printed).

**§61a re-run Result (2026-11-17, Amendments 2–3; SCORED): ρ DISAGREES. At c3 the knee margin is about 0.46 of S_F, not c2's 0.80.**

**In plain words.**
- **The question.** Does the knee margin scale with the floor-window spread S_F, so that one g(δ)·S_F term serves every link shape (decision 8)?
- **The answer at a second link shape is no.** At c3 the knee needs 0.46 S_F, against c2's 0.80, and the difference is far outside the registered margin. By the coordinator's reading of ρ₃'s interval, the σ-scaled alternative does not explain it either.
- **Every candidate allowance covers c3's knee with room to spare,** so no candidate is short here. They over-allocate.
- **The registration's consequence:** S_F is not the knee's term, and decision 8's form is re-derived, with c3's need₃ against each rival sentence as the data.
- The re-run is not blind. It was registered after the first session's post-hoc bound (0.31–0.56), and the scored value lands inside that bound.

**Runs.**
- **Re-rehearsal:** launch 11:57:33Z, `REHEARSAL-PASS` 12:19:03Z, tree `3cdc4470` (the VM tree was checked for `ABORT-CONTROL61` and the proportional pilot).
  - floor GO (2/2);
  - control GO on R6 W 512: goodput 17.85 ≥ 17.40, loss 0.0503 ≤ 0.0576, CR61 ratio 1.11. That is under the 1.4 stop line, so 61a proceeded;
  - R1 W 2 048, reported: ratio 1.86, the eighth W 2 048 row above 1.5;
  - pilot `PILOT61 W=103.8 u88=0.806 u128=0.995 case=proportional clamped=no`, giving K 100/108/120, B 60/80/88, P1 152.
- **61a:** launch 12:20:50Z with `LD_KNEE61=103.8 LD_CTL61=512`, `LD_NOGO` empty. Rows ran 12:38–13:45Z.
  - `bin_sha` equals `e410d640…`; suite-reuse HIT; tools tests pass on the VM;
  - `VERDICT61 REPORTED-WITH-SCORED slots=66/66 live=66 void=0 replaced=0 dnf=0`.
- **Host:** the coordinator held the host lock from 11:56:57Z to the end, and the F2 worker's cargo waited at it.
- **VM:** io pressure at both launches (avg10 3.95–4.73) came from the coordinator's own archive extract just before each one. It was 0.15–0.34 at the smoke, before any row. The quiet-host first session gives the same ρ (bound and path exercise 0.458).
- **Reproduction:** the host re-score with `LD_KNEE61=103.8 LD_CTL61=512` matches the VM `score.txt` byte for byte.
- **Ledger:** `docs/l1-raw/s61/rep61c-*` and `rehearsal-c.tgz`.

**Registered labels.**

| label | value |
|---|---|
| CONTROL-c3 | **OK** (6/6 unflagged, median 17.82). W 512 ratios 1.05–1.16, inside the predicted 0.95–1.4; P1 152 reads 0.87–0.96 |
| FLOOR-F-c3 | NOT-CONVERGED (b +0.1021 ± 0.0278 ms/slot) |
| FLOOR-S-c3 | UNRESOLVED (b −0.0631 ± 0.0298) |
| **RHO** | **DISAGREES** (RHO-RAW DISAGREES) |
| SIGMA-DISCRIMINATION | SEPARABLE (+0.320, outside ±0.085) |
| D-{S, A0, A0f, Ac, wq, W80}-Bulk | all **COVERED** |

**The readings.**
- **Floor:** pool W 12, W\* = 11.6 (κ 0.00962), F_c 38.998, S_F,c 9.709. Parity is IN at both floor (286) and knee (2 336).
- **Knee:** 6/6 replicates scoreable. W₉₅ is 106.6–110.1, all inside [K₁, K₃] = [100, 120]. need per replicate: 4.26 / 4.50 / 4.40 / 4.46 / 3.61 / 4.48 ms.
- **ρ₃ = 0.457, 90 % [0.409, 0.483],** against ρ₂ 0.797 [0.780, 0.825] (frozen).
  - Δρ = −0.340 [−0.396, −0.311]; Δ = min(0.085, 0.201) = 0.085, so DISAGREES.
  - W → 0: ρ₃,₀ 0.538 and Δρ₀ [−0.420, −0.286] against ρ₂,₀ 0.877, so DISAGREES. FLOOR-SENSITIVE: no.
  - Native (reported only): ρ₃ 0.633, D-S −6.40 ms.
- **The prediction against the result:**
  - **Rival sentences** (`s61pred.out` §3; reported, since only the σ-scaled rival is scored by SIGMA-DISCRIMINATION):
    - S (need = ρ₂·S_F) predicts ρ₃ = ρ₂ = 0.797: rejected.
    - **Constant** (need₃ = need₂ = 0.797 × 5.083 = 4.05 ms) predicts ρ₃ = 4.05 / 9.709 = 0.417. It is the closest rival, and S-scaling predicts need₃ +91 %.
    - **σ-scaled,** on the measured inputs: ρ₃ = ρ₂ + the printed SIGMA-DISCRIMINATION 0.320 = 1.117. **Loop-scaled:** need₂/F₂ × F₃ = 4.05 / 8.679 × 38.998 = 18.2 ms, so ρ₃ ≈ 1.88. Both are far off.
      - SIGMA-DISCRIMINATION scores only whether S and σ-scaled can be told apart. Ruling σ-scaled out is the coordinator's reading from ρ₃'s interval.
    - **How constant depends on where the floor is read:**

      | floor | need₃ | need₂ | need₃ / need₂ |
      |---|---|---|---|
      | u-matched (scored) | 4.43 | 4.05 | 1.09 |
      | the intercept (c3 + b_F·W\*; c2 + its matched-n slope × 16) | 5.62 | 4.60 | 1.22 |
      | native windows (reported) | 3.71 (0.633 × 5.861) | 4.05 | 0.92 |

      Need₃ per replicate is 3.61–4.50 ms. No interval on need₃ − need₂ was computed.
    - **So across the two link shapes the knee margin looks roughly constant in milliseconds** (−8 % to +22 % by floor), not proportional to S_F (5.08 against 9.71). That is a lead from two points, not a law.
  - The first session's post-hoc bound [0.31–0.46, 0.50–0.56] held: 0.457.
  - The path exercise on the first session's rows read 0.458.
- **D per candidate** (need − A; median, 90 % bootstrap; Bulk scored, Auto / Realtime reported):

  | hint | S | A0 | A0f | Ac | wq | W80 |
  |---|---|---|---|---|---|---|
  | Bulk | −9.17 | −3.74 | −4.33 | −2.69 | −10.57 | −9.82 |
  | Auto | −7.87 | −2.64 | −3.03 | −1.93 | −7.19 | −8.52 |
  | Realtime | −7.22 | −2.08 | −2.38 | −1.56 | −5.50 | −7.87 |

  - Every interval is below 0, so every candidate is COVERED at c3.
  - D₀ at W → 0 is COVERED everywhere except Ac at Auto and Realtime (UNRESOLVED; reported).
- **Handed to the owner** (the AGREES branch's quantities, printed regardless):
  - ρ̄ pooled 0.615 [0.601, 0.641];
  - g_knee c2 0.711, c3 0.256;
  - the c3 landing table (Bulk W\* 105.1 at g = 0 to 150.3 at g = 1).
  - Under DISAGREES, these describe what one pooled g would cost. They are not a recommendation.
- **B1** (reported only): 2/18 fire (predicted 0–3).
- **Monotone:** NO, at W 16, from the floor's native tmin. This was already seen in the first session.

**What this means for the store law (FORMULA-FIRST).**
- **Decision 8's g(δ)·S_F + q(δ)·F̂ carries a term whose coefficient is not shape-free.** At Bulk, c2 needs 0.80 S_F and c3 needs 0.46 S_F, while every candidate allowance covers c3 by 2–10 ms.
- **The registered next step is a re-derivation.** Find what the knee margin does scale with, using c3's need₃ against each rival sentence (the D table and the σ-discrimination above) and c2's need₂.
  - The constant-in-ms reading (the A(c) family's c_B, W1.2) is the first candidate to derive. It is not adopted from two points.
- The ledger rows F07 (allowance) and E13 (ℓ̂) stay open. F07's ledger entry is owed an update citing this Result.
- **No engine change follows from this Result alone.** Decision 8's form is not in the engine. The shipped store cap is unchanged, and the engine form waits on this derivation.
- **Decision 10** (per-leg queue target 1 + q(δ)) uses only q(δ), the F̂ term's dial. That term is not under test here, so decision 10 stands and perleg Phase A is unaffected.

**Pending, added by this Result:**
- the decision-8 re-derivation (Opus; reviewed by Fable; the owner's call on the new form);
- CR61 at large pools (unchanged; R1's 1.86 is the eighth point);
- the c3 control tail (undiagnosed);
- the G12 s42 r5 B1 outlier from the first session.

**§61a re-run Result, Erratum 1 (2026-11-17, after the post-hoc re-derivation `knee-margin-rederive` `fc0d1514`; Fable-reviewed): the label stands; two readings of it are withdrawn.**
- **What stands:** `LABEL61 RHO DISAGREES`, computed exactly as registered. The re-derivation's script reproduces every scored number (κ, W\*, F_c, S_F,c, the six needs, ρ₃ 0.457, the NATIVE line) and c2's frozen inputs.
- **What the scored need is.** It subtracts a floor read at about 270 samples per group, G(250), from a knee minimum read at about 2 100, G(2 100). The registration did this at both cells, matching c2's counts on each side.
- **The minimum depends on the count, and it has not flattened.** Median over rows of the floor pool, against the sample count:

  | cell | native | G(250) | G(600) | G(1 200) | G(2 100) | G(4 200) |
  |---|---|---|---|---|---|---|
  | c2 W 16 | 8.679 (n 282) | 8.506 | 7.802 | 7.324 | 7.067 | 6.728 (n 4 344) |
  | c3 W 12 | 42.751 (n 60) | 38.778 | 37.008 | 35.590 | 34.319 | 33.227 (n 4 223) |

  - At c3 the count alone moves the floor by about 8.4 ms, twice the need (4.43).
  - The drop between the floor's count and the knee's count is 1.6 ms at c2 and 4.4 ms at c3. So matching each side's count across cells does not make ρ shape-invariant, which the registration's §3 assumed.
  - **Correction (same day, after round 2):** the last clause is not supported. Across both c3 sessions the c3/c2 ratio of that drop is [1.85, 3.18], which contains S_F's ratio (1.88–1.91). So these data do not refute §3's assumption that the minimum's bias scales with the spread.
- **Withdrawn:**
  1. "S_F is not the knee's term", as a statement about the path. The label shows that the registered ρ, under the registered counting, is not invariant across the two shapes. Whether any spread carries the load rise is not settled by these data.
  2. **"The knee margin looks roughly constant in milliseconds" (4.05 vs 4.43 ms).** It comes from the counting convention: at one count for both sides, need₃/need₂ is 1.57 [1.48, 1.64] (re-run) and 1.41 (first session). The lead is withdrawn.
- **What follows instead:**
  - The registration's own estimator item comes first: R6.7's sample-count rule, "before any engine form". The minimum needs a count rule, or a count-free statistic, before any knee reading is interpreted.
  - That is design work for review, not yet an owner question.
  - The re-derivation goes to a second round (both sessions' bands; knee arms at both counts; the floor's drop against the time span pooled).
  - Decision 8's form stays as decided, and is not in the engine. F07 stays UNDERIVED.
  - Decision 10 is unaffected.

**The knee-margin re-derivation, closed post hoc (2026-11-17; `knee-margin-rederive` `399407f4`, three rounds, Fable-reviewed).** These rows cannot settle decision 8, and no further round on them can add evidence.
- **The count drop follows jitter,** above 250 samples only. §62's J = 0 rehearsal removed the drop above 250 (0.39 ms) and left the drop from one native window to 250 (2.97 ms).
  - At a fixed count, interleaved and shuffled pooling read the same as contiguous pooling, so the drop comes from the count, not the pooled time span.
  - On the jitter-free c1s line the floor minimum falls only 0.12–0.18 ms over 32 windows, against 2.2 ms at c2 and 7.8 ms at c3.
- **Two cells can give every label.** Across 15 location and spread pairs (min, p10 and p50 against S_F, native S_F and p90 − p10), the same rows read AGREES, UNRESOLVED and DISAGREES of both signs. For example, the median's rise over the registered S_F is 0.711 at c2, against 0.713 and 0.750 at c3.
- **What the law fixes (coordinator, from the formula, R6.7 line):** T̂min(n\*) = F + σ + q(δ)·ℓ̂.
  - **The knee side is the per-window minimum T̂min,** so the law fixes the knee statistic.
  - **The floor F is built from m̂** (Copa `min_rtt`), ℓ̂ (a running lifetime minimum) and the boot term b̂₀⁺, not from a per-window floor arm. Its sample count is the law's open design item (R6.7's n₀).
  - **Corrected (2026-11-18, `count-rule` `2d557191`):** the earlier line "the native floor windows belong to the measurement, not to the law" is wrong. b̂₀⁺ is built from the 250 ms sampler's first 8 windows (`net/slaw.rs` `WinSampler::close`, :150–165, on `slaw-gauge` `d416d59b`), so the 250 ms window is the law's. Only the floor arm (W 12) is the measurement's.
- **On the knee-native minimum against native S_F:** 61c disagrees with c2, and 61a is marginal (it clears Δ by 0.002, on arms chosen post hoc; the two c3 sessions differ by 0.04 in ρ).
  - The need there is 4.05 / 3.71 / 4.10 ms, and the whole gap is native S_F (5.08 against 5.86–6.09).
  - So this reading and the withdrawn "constant in ms" are one fact read two ways.
- **Kept for the next measurement:**
  - an order-statistics criterion: a p-quantile is count-free to ε when m·p ≥ 1/ε;
  - c3 floor windows hold about 17–21 independent draws, so p10 and p90 fail the criterion there;
  - `RWM_RTT_DUMP` carries the same sample stream as `[SLAW]` (`record_rtt_sample`, `scheduler/path.rs`, on `slaw-gauge`).
- **Decision 8 stands as decided:** neither confirmed nor refuted. g's Realtime end is unmeasured, F07 stays UNDERIVED, and decision 10 is unaffected.
- **Next:** s62, a jitter sweep at c3 (sketch in the derivation doc §5). It goes to the owner as one call.

**§61b Result (2026-11-17; the c2 add-on ran clean; REPORTED with its two labels).**

**In plain words.** c2's price curve below its knee is now measured directly, at W 96 / 128 / 144. It matches §60's c2 rows at the overlap arm, so the two sessions merge into one c2 table. 61b scores only two labels: the control is OK, and there is no drift.

**Runs.**
- **First launch:** `~/s61b`, at 08:08:28Z. It stopped at `ABORT-PRECONDITION` (procs_running 11, from the coordinator's own copy and extract a moment before), and nothing ran.
- **The launch:** `~/s61b2`, 08:10:27Z, with `LD_BIN_PREBUILT` (61a's binary). Rows ran 08:21–08:56Z.
  - prebuilt `bin_sha` equals the registered `e410d640…`; suite-reuse HIT; the python and bash tools tests pass on the VM;
  - `VERDICT61 REPORTED-WITH-SCORED slots=30/30 live=30 void=0 replaced=0 dnf=0`.
- **Host:** cargo paused. The coordinator's single-threaded scorer and analysis scripts ran on the host around the launch (the tools tests and the smoke). During the scored rows the host carried only text edits, the roadmap build, git and a worker's edits. There were no stub or multi-job runs.
- **Reproduction:** the host re-score matches the VM `score.txt` byte for byte.
- **Ledger:** `docs/l1-raw/s61/rep61b-*`.

**Labels.** `LABEL61 CONTROL-c2 OK` (6/6 unflagged, median 90.27 Mbit/s); `LABEL61 DRIFT-c2 DRIFT-OK`.
- CR61 flags 0/30, with X_own frozen at 10.66 ms.
- The drift check at the overlap arm W 160: tmin 12.295 vs §60's 12.33 (|d| 0.035 ≤ 0.3), and u 0.930 vs 0.93.

**c2 price below the knee** (median over replicates, 90 % bootstrap, seed 60):

| W | u | tmin | Tp50 |
|---|---|---|---|
| 96 | 0.611 [0.605, 0.613] | 10.165 | 15.434 |
| 128 | 0.783 [0.780, 0.785] | 11.219 | 15.982 |
| 144 | 0.861 [0.858, 0.864] | 11.794 | 16.325 |
| 160 | 0.930 [0.927, 0.933] | 12.295 | 16.784 |

- **The chord over-read.** The registration's chords from §60 (48 → 160) predicted u 0.582 / 0.756 / 0.843 and tmin 10.45 / 11.39 / 11.86 at W 96 / 128 / 144.
  - Measured tmin lies 0.07–0.29 ms below the chord, as the convexity of tmin in W predicts.
  - Measured u lies above the chord, so u is concave in W. That is the shape that moved Amendment 2's pilot.
- **Monotone:** u and tmin are non-decreasing in W over every arm.
- **The landing table** (merged with §60's arms; F_c 8.679, S_F,c 5.083). It is reported only; the owner's call follows it.

  | g | Bulk W\* (u) | Realtime W\* (u) |
  |---|---|---|
  | 0 | 69.6 (0.452) | 51.1 (0.340) |
  | 0.25 | 115.8 (0.718) | 102.7 (0.647) |
  | 0.5 | 153.4 (0.901) | 140.1 (0.842) |
  | 1 | 198.5 (0.991) | 193.6 (0.990) |

- **The c3 side of the g(Realtime) question waits for the 61a re-run** (Amendment 2).

## 60. The store law's gauge session (B-1), reported only — pre-registration (before any VM contact)

**The registration is `docs/derivations/s60-registration.md` at `s60-design` `10272273`** (merged to main). It was reviewed with Fable in ten logged rounds and is registered verbatim from that commit; no rule is restated here.

**Owner summary (from the doc).**
- On a static pool the store fills within half a second, so the law's floors (b̂₀⁺, ℓ̂) are read from windows that already carry queue. So this session cannot test the law itself. It does three things:
  1. it measures where the queue-free floor lies, on pools below anything measured (c2 W 16/32/48, c8 W 32/64/96, c1s W 64/128/192);
  2. it tests the target FORM with those floors, decisively at c8 (B2, a gross-miss detector);
  3. at c2 it scores the allowance shortfall D. R6.7 predicts SHORT, with D of +0.9 to +2.2 ms.
- c1s scores the sign of R̃ − 1 at its control. Paired G/C rows (`RWM_SLAW=1/0`) check the gauge's own cost for gross errors.
- Five evicting c2 rows (plan 59) give m̄ (`adv`), W1 (`dbt_share`) and K4's inputs (`fec=`) for the P_fec oracle.

**Tree and binary.**
- `s60-tree` `cb88df3a` (main + `slaw-gauge` `d416d59b`), plus the harness (tools only).
- 60-pre (above) pins the suites, `bin_sha` and `ref60-manifest`.

**Launches.**
- 60a: 98 rows, typical 2.2 h, worst 3.2 h, `LD_HARD` +4 h.
- 60b: 47 rows (plus at most 5 evicting re-runs), typical 1.25 h, worst 2.9 h, `LD_HARD` +4 h.
- A rehearsal (10 rows) runs before 60a, with the c8 and c1s go/no-go and the witness checks.

**Next derivation:** R6.7 (`store-law-r67.md`), the candidate floor and the allowance item. A large sweep comes only after it.

**Amendment 1 (2026-11-16, before any VM contact for §60): the harness's readings of the registration.** `harness60` `8565e7d2` implements the registration.
- **Also registered by reference:** the scorer's ambiguity list, `s60_score.py` DECISIONS 1–21 at `8565e7d2`, one reading for each place the doc is silent. These include:
  - the pooled 80 % rule (5);
  - witness-VOID rows kept for the floor, D and P3 (6);
  - σ_R as the pooled within-arm sd (12);
  - COVERED without CONVERGED printing UNRESOLVED (13);
  - the starved-leg W 256 fallback and the all-COLLAPSED pool skip (19).
- **The bullets below are the readings that change a rule or a number.**
- **Launch tree:** `harness60` `8565e7d2`.
  - Its engine (`raptorpath/`, the math, wasm and core crates, `gf256`, `Cargo.*`) is byte-identical to `s60-tree` `cb88df3a`: the `git diff` is empty. Every later commit touches `tools/l1` only, and `ref60-manifest.sha256` is present.
  - Every launch must build `bin_sha` `e410d640…` and HIT `ref60-manifest`; otherwise it aborts before any row.
  - Main does not take the gauge, because it changes the release binary and has had no guard. The tools merge to main after the Result.
- **R-sign gate (§5, "read after replicate 2"):** this cannot be computed as written. After replicate 2 the calibration has 2 floor rows, and it needs ≥ 3.
  - So it is read at the **end of 60a**, on the full session calibration, over every live c2 and c8 control row.
  - **STOP:** any control R̃ ≥ 1.
  - **PASS:** no STOP, on ≥ 2 replayable control rows (the registered reading used two).
  - **UNSCOREABLE:** fewer than 2 replayable control rows, as when a cell has no valid calibration (an unattached c8 leg 1, fewer than 3 valid floor rows).
  - **Under `LD_NOGO=c8`, R-SIGN-c8 is always UNSCOREABLE.** Its floor failed go/no-go, so its control R̃ is reported but never gates. A c2 NO-GO only moves c2's pool to W 32, so c2 still gates.
  - **60b:** `loopdiag_run_all.sh` refuses to launch on a STOP or a missing label at either cell (`ABORT-RSIGN`, nothing run). UNSCOREABLE does not block, because none of 60b's readings (the c1s sign, G/C, the evicting rows) uses c2's or c8's R. It is recorded in the launch log.
  - **Stricter than registered:** the gate stops on any of about 6 control rows per cell, not 2. R̃ falls with W on anc-1 arms and crosses 1 near the knee (§3.2), and the controls (c2 W 2 048, c8 W 1 280) sit 940–1 880 slots above their knees (169, 340). So session noise (σ_R 0.008–0.015) can lift a control row to 1 only if R̃ there lies within a few σ_R of 1. No predicted control R̃ is registered. A STOP is amended as the doc says, and no reading of it is fixed here.
  - **Cost:** a failing law spends all of 60a's rows instead of stopping early. Those rows are reported.
- **Budget (§7 against §2):** §2 re-runs each floor-pool DNF once (R:357–358), but §7's worst cases leave that out.
  - Worst case at 190 s per re-run: 60a = 11 520 + 36 × 190 = 18 360 s (5.1 h); 60b = 10 440 + 18 × 190 = 13 860 s (3.85 h).
  - `LD_HARD` is launch + 5.2 h (60a) and + 3.9 h (60b), and the guard refuses below those values.
  - `LD_HARD` is sized per session from its worst case: status gives + 2.5 h to + 5 h, and §54 used + 5 h per launch. No discipline rule caps it at 4 h. §7's "together they exceed 4 h" is the reason the session is split, and the split stays. The typical lengths (2.2 h and 1.25 h) are unchanged.
  - The budget guard also applies to the rehearsals, which therefore launch with the same `LD_HARD`.
- **Re-runs and DNF rows:**
  - Once each: an echo or liveness VOID, which includes the evicting seat's `no-realtime-echo`, and a floor-pool DNF.
  - Never: a wrong binary (`bin-sha-*`, including uncheckable), which aborts.
  - **One row per slot calibrates:** the slot's last row (`pool56_score.counting`). A replaced DNF stays in the ledger as a datum, printed beside its replacement, and never enters the session median, the ≥ 3-row minimum or the drift order.
  - A DNF that is itself the counting row (its re-run also DNF'd) calibrates only with ≥ 120 LATE windows (30 s) on the leg.
- **Timeouts:** 150 s on every c2 and c8 row, not only on the floor-pool rows the doc names (§2). These rows typically take 75–90 s (§7). c1s floor rows also get 150 s, and the c1s 5 GB control 210 s (§7).
- **Instrument env (silent in the doc, whose §4 names `RWM_DIAG=1` only):**
  - every row runs with `RWM_RDIAG=1`;
  - every G and C row adds `RWM_RTOBS=1`, §56's env, so that §56's knee and collapse thresholds apply;
  - the evicting rows keep plan 59's env (no RTOBS).
  - G and C still differ in `RWM_SLAW` alone.
- **B2 (§3.2, "fires on ≥ 2 of the 5–6 scoreable replicates"):**
  - Scoreable means anc-bound eligible: the replay's anc bind fraction is ≥ 0.9 at both bracketing rows.
  - With fewer than 5 eligible replicates, B2 is UNSCOREABLE (UNDERPOWERED) whatever the fires.
  - With 5 or more, it is FIRES iff ≥ 2 fire, else NOT-FIRES (weakened if a floor is not CONVERGED).
- **c2 sign witness (§3.1):** R̃₆₇ comes from the same session F_c and l_c as D, and from the same nk ≥ 10 window medians at W₉₅. So sign(R̃₆₇ − 1) = −sign(D_r) holds exactly, and a mismatch is a scorer bug. |D_r| < 0.01 ms prints UNRESOLVED.
- **C2-SHORT needs ≥ 3 scoreable replicates.** The doc gives no minimum.
- **LATCHED (§2)** uses the row's own median LATE `l` in q·ℓ̂. Using the session l_c would be circular, because latched rows are left out of it.
- **FLOOR-CONV at c1s** reads NO-TAU-REGISTERED: the doc registers no τ for c1s. The slope label is still given, and the c1s control sign does not depend on it.
- **Drift fallback (§2):** under per-replicate pairing, a replicate without a same-(rep, seed) floor row cannot be replayed. At c1s that drops control replicates 7–12, so a triggered fallback there almost certainly leaves the sign test UNDERPOWERED. This is recorded before any data.
- **Go/no-go rows (§5):** every G floor-pool row of the cell in the rehearsal counts, the smoke rows included. That is 3 c1s W 64 rows, not 2, and c8's W 96 counts on leg 0. This lowers the chance NO-GO below §5's 6 % / 8–23 %. Rehearsal rows are never re-run (only scored slots are), so a rehearsal DNF fails criterion (a) and adds no further chance.
- **CAPLOG (§5):** each record replays by branch, and seats are reported rather than asserted to be `plain`. The replayable share is printed for G and evicting rows.
- **60b rehearsal (new):** `LD_PLAN=60b LD_REHEARSE=1` runs 60b's two smoke rows plus one evicting row. It checks:
  - plan 59's liveness;
  - `[SLAW] q` = 0.05;
  - median `[SPAN]` rr > 0;
  - the `[WSTORE] ackw` clock join: ackw(j) = wake_ack(tick j) − wake_ack(tick j − 1) on ≥ 10 windows, all equal.

  The join is the first engine check of decision 16's `[DIAG]`/`[WSTORE]` join. If it fails, 60b waits for an amendment.
- **Not computed in the scorer (reported as such):** "C is re-based against §56's c1s control" (§4) and the K4 oracle comparison (inputs only, off-VM per §6).
- **The witness sample (§8.3):**
  - **On file:** an engine-rendered sample (`testdata/slaw_sample.log`) and the hand-formatted `s60witness_ka.log`, which has an anchor-bound leg, an nk = 0 leg and a warm-up window. Both pass the witness with their negative controls (`test_s60.py`).
  - **Rehearsal:** every G row runs the per-row witness on engine output. The report says whether those rows show an anchor-bound leg and an empty leg. A case the rehearsal does not show stays verified only on the hand-formatted sample.
- **Tests:**
  - The full stub run (`STUBRUN_FULL=1`, every plan) was ALL OK at `b3547368`.
  - At `74370e3f`, the plan-60 stub paths (full tables, 12 paths) gave 78 ok, ALL OK.
  - At `8565e7d2`, every `tools/l1` Python test from `test_anchordiag_parse` to `test_storelaw_parse` passes, 33 files in all, including `test_s60`, `test_harness60`, `test_harness_paths` and `test_harness46`.
  - **Still owed:** the fast unfiltered `test_stubrun.py` and the shell tests at `8565e7d2`. Claude Code stopped that run for host memory, not for a failure. Since `b3547368` the shared scripts changed only in two comments and the 60b `ABORT-RSIGN` block.

**Amendment 2 (2026-11-16, after the 60b rehearsal, before 60b): the evicting rows leave §60.** 60a is unaffected: it has no evicting row, its own rehearsal passed, and it launched at `8565e7d2` at 10:07:42Z, as Amendment 1 registers. This amendment changes 60b only.
- **The 60b rehearsal** (`~/r60b`, `LD_REHEARSE=1`, `8565e7d2`) ended `ABORT-SMOKE` at 02:55:30Z.
  - `bin_sha` equalled the registered value and the suite manifest HIT.
  - Both G/C smoke rows (c1s G64, C2048) passed every check. G64's CAPLOG: 28 records, 28 replayed, 0 mismatches, bind(ceiling) 1.000.
  - The one evicting row (c2-100, Realtime, `RWM_C_RELIABLE=0`, s42) failed three smoke checks: the G-tag check (no `[CAPLOG]`), the CAPLOG replay (0 records), and the debt loop (median `[SPAN]` rr 0.000). It was also a DNF, which is expected on this seat (below).
- **Why the row gives almost nothing to read as registered (one rehearsal row, n = 1; an observation, not a Result):**
  1. **Overdriven.** The sender emits ≈ 90–108 k source symbols/s (`[DIAG] good=` ≈ 400 Mbit) into the 100 Mbit link, with `paused=0%`. All 600 MB (`cum` 1 214 576) is emitted by t ≈ 13 s; netem sent 164 MB of it.
  2. **Emission ends at ≈ 13 s.** The DNF itself is by design: perf.rs:34 records a Realtime DNF as an expected datum, and the E rows' 150 s timeout is what §60's 12 min for 5 rows assumed. What is new is that the sender is idle from ≈ 13 s to the timeout (`[SPAN]` stops at t = 12 s; `[WSTORE]` windows stretch to ≈ 2 s with ackw → 0). Each row would therefore spend 150 s for ≈ 8 s of LATE data (t 5–13 s).
  3. **rr = 0: the spare-capacity clamp is sufficient for it.** The rate seat hands `flat_rate = window_rate.min(spare)` (`emit_source.rs:543`), with `spare = (cwnd − in_flight)⁺ / in_flight` (`scheduler/path.rs:471`). The token's r is the rate after that clamp and the floor (`note_debt_accrued`, `emit_source.rs:706`). The unclamped `window_rate` is not printed, so the rate law's own value is unread.
     - Over the 53 sending windows (250 ms, t ≤ 13 s) the `[WSTORE] fec=` token reads r = 0 in 46 (87 %), at ε̂ up to 0.068.
     - The other 7 (t = 0.25 s, 10.25–10.75 s, 12.25–12.75 s) read r = 0.0001–0.50 with `dbt_share` 0.97–0.9995. So the debt loop runs whenever the clamp lets go, but the median rr is 0.
     - **Joined to all 52 `[DIAG]` ticks of the sending phase (t 0.3–13.0 s):**
       - 48 are saturated, with in_flight ≥ cwnd (5 647–33 392 against 702–10 000).
       - 4 are free (t = 10.3, 10.5, 12.3, 12.5 s: in_flight 662–6 755 against cwnd 10 000).
       - `np_act` (`diag.rs:558`, the saturation-filtered `active_paths()`) is 0 on every saturated tick and 1 on every free one.
       - All 4 free ticks fall inside the two r > 0 bursts, and none falls in an r = 0 stretch.
       - Windows are 250 ms means and ticks are point samples, so some r > 0 windows (t = 10.75 and 12.75 s) border a saturated tick.
       - The t = 0.25 s window precedes the first tick; at start in_flight = 0, where spare is ∞.
     - **Why this suffices:** `Scheduler::spare_capacity` folds over live paths (`p.active`), so on a saturated tick spare = 0 and flat_rate = 0. Nothing between the clamp and the accrual (`emit_source.rs:575–706`) caps `repair_rate` further; only the floor (`max`) applies.
     - **The source is not held to that cwnd.** cwnd jumps from 1 159 to 10 000 at t ≈ 10.3 s.
     - **Open:** whether `spare` reads a scheduler cwnd that nothing enforces while quinn's BBR is the controller. The clamp's bind fraction on this seat is owed as a gauge in the new registration (FORMULA-FIRST: every clamp gets one). Its ledger verdict waits for a row that is not overdriven. Both are added to the pending list below.
  4. **No `[CAPLOG]` by construction.** The store cap runs only under `plain_dyn_cap = reliable && !generation && !coded_only && !store_env_set` (`sender_policy.rs:523`), and the evicting seat is not reliable. Amendment 1's "the replayable share is printed for G and evicting rows" is withdrawn for evicting rows, and so is the smoke check that put the evicting row among the G rows that must carry CAPLOG.
- **The `[WSTORE] ackw` / `[DIAG] wake[ack=]` clock join (decision 16's join) passed on engine output.** All 121 compared window pairs were equal. Of the row's 122 windows, 117 had ackw > 0 and 53 had ackw > 10, so the ≥ 10 rule holds on active windows alone. Decision 16 is read only by evicting rows, so nothing left in 60b consumes it.
- **Changes to 60b:**
  - 60b and its rehearsal schedule no evicting row. **60b's launch tree is `harness60-a2` `1ecee59d`**, which changes `tools/l1` only: the engine `git diff` against `8565e7d2` is empty, so `bin_sha` and the manifest are unchanged. 60b is 40 slots and its rehearsal 2 rows.
  - The scorer prints the evicting sections as NOT-RUN and asserts no criterion over an empty row set. Evicting rows, if any are present, are left out of the CAPLOG tag and replay checks. The EVICT code stays, tested, for the new registration.
  - **Registered by reference for 60b:** `s60_score.py` DECISIONS 1–22 at `1ecee59d`. 22 is new and records this change.
  - **Scoring:** 60b and the Result are scored at `1ecee59d`. The host re-score must reproduce 60a's on-VM `LABEL60` lines.
  - **Tests at `1ecee59d`,** run on a `git archive` export (LF; a Windows checkout aborts the stub run with `ABORT-CRLF`): `test_s60`, `test_harness60`, `test_harness_paths` and `test_mbar59` pass. So do the 12 plan-60 stub paths of `test_stubrun.py` (`STUBRUN_ONLY` = the six 60a and six 60b paths, fast mode): 78 ok, ALL OK, 501 s.
  - **The 60b archive** is built from the commit with `git archive`, like `h60.tgz`, not from a worktree.
  - The budget pins (`BUDGET_S["60b"]`, `P57_NEED` 13 860 s, `LD_HARD` + 3.9 h) are unchanged. Without the evicting block they over-state the worst case by ≤ 32 min (§60's table: 5 rows + ≤ 5 re-runs).
  - Before 60b, the rehearsal re-runs at the new tools sha (the two smoke rows, `LD_REHEARSE=1`) and must log the same `bin_sha` and a manifest HIT.
- **What moves:** m̄ (`adv`), W1 (`dbt_share`) and K4's inputs for the P_fec oracle go to their own registration, as this section first said ("Not in this session", above). It needs:
  - an offered load paced below the link rate;
  - an object size and timeout chosen so the emission phase covers the LATE window (the DNF stays an expected datum);
  - the spare-clamp bind fraction as a reported gauge;
  - its own rehearsal, with rr > 0 as go/no-go.

  The pending item "K4 with the §60 data" now reads "K4 with that registration's data".
- **Pending, added by this amendment:**
  - the evicting-seat registration (above);
  - the spare-capacity clamp on a Realtime source: its bind-fraction gauge and its ledger verdict;
  - whether `spare_capacity`'s cwnd is enforced on that seat under quinn's BBR.
- **Still owed from Amendment 1:** the unfiltered fast `test_stubrun.py` and the shell tests. *(Paid, see the Result.)*

**§60 Result (2026-11-16; REPORTED with scored readings; 60a, the 60b rehearsal re-run and 60b all ran clean).**

**In plain words.**
- **c2:** the law's allowance σ + q·ℓ̂ falls short of the goodput knee by 1.7 ms. The floors are measured on near-empty pools. That is SHORT, as R6.7 predicted, and resolved.
  - The floor converged to within 0.18 ms, so the shortfall is a lower bound that is close to tight.
  - Fed these floors, the R6.7 form crosses 1 below W 160, under the knee at ≈ 168. The exact crossing is not located.
- **c8:** B2 could not be scored.
  - At the knee arms (W 320 / 384), leg 0's replay anchor binds on every window, but leg 1's binds on only 0–58 %. Leg 1 is calibrated at W 256, where its floor sits at 36.8 ms.
  - DECISION 12 pools both legs' terms, so the bind fraction is 0.50–0.79 against the 0.9 rule. Once leg 1's floor pool moved, B2 was unscoreable by construction.
  - The reported crossings sit far above every band. R̃'s crossing rests on a chord between W 512 and W 1 280.
- **c1s:** the control sign and the G/C check could not be read. The frozen collapse rule flags every W 2 048 row, G and C. Rows where quinn's window is not collapsed still show 13–17 ms of excess, far above the 6.1 ms threshold. So the rule flags the control pool's own queue. Real collapse hit 7 of 22 rows, within the predicted rate.
- **Held:**
  - P1 (b0p latches the start-up queue) at all three cells.
  - The R-sign gate.
  - Every row's replay witness and CAPLOG replay.
- **Missed:**
  - B1 fired on 3 rows, against "predicted not to fire".
  - P3 (ℓ̂ rises with the pool) rose 3–4× more than predicted at c2.
  - c8 leg 1's floor gate is uninformative. It was scored as registered, but the rule has a gap, below.

**Runs.** Ledger: `docs/l1-raw/s60/` (scrubbed).

| launch | tree | window (Z) | slots | verdict |
|---|---|---|---|---|
| 60a | `harness60` `8565e7d2` | 10:07:42 – 12:12:13 (smoke pass 10:24:45, battery 6 445 s) | 96/96 live, 0 void, 0 replaced, 0 DNF, 0 witness-void, 0 t60 missing | `VERDICT60 REPORTED-WITH-SCORED`, 17 labels, no NO-GO |
| 60b rehearsal (re-run) | `harness60-a2` `1ecee59d` | 12:13:13 – 12:31:02 | 2 smoke rows | `REHEARSAL-PASS` |
| 60b | `harness60-a2` `1ecee59d` | 12:31:25 – 13:26:17 (smoke pass 12:48:56, battery 2 240 s) | 40/40 live, same zeros | `VERDICT60 REPORTED-WITH-SCORED`, 6 labels, no NO-GO |

- **Identity:** each of the three launch logs carries `bin_sha equals the registered value e410d640…5f25d` and `suite-reuse HIT` on `ref60-manifest.sha256` (187 files):
  - `run60a-launch.out`, `run60b-launch.out`;
  - `rehearsal2.tgz`'s `launch.out`.
- **The R-sign gate before 60b** read 60a's score: `R-SIGN-c2 PASS` and `R-SIGN-c8 PASS`, each on 6 control rows.
- **Host re-score at `1ecee59d`** (`python -X utf8`): `rep60{a,b}-score.txt`, beside the VM's `-score-vm.txt`.
  - 60b's re-score is byte-equal to its VM score: one git blob, `670f2d97`.
  - 60a's equals its VM score (at `8565e7d2`) except for one added line, Amendment 2's "EVICTING ROWS: none", as registered.
  - All 17 of 60a's `LABEL60` lines reproduce.
- **Witness coverage** (counted from the printed `[SLAW]` lines of every G row, on windows with R defined):
  - The per-row replay witness passed on all 126 scored G rows (96 in 60a, 30 in 60b), reproducing printed R and Rb to 6·10⁻⁶.
  - Those windows include printed `anc=1` on some leg: c2 1 064, c8 42, c1s 1 416.
  - They also include c8 windows with an empty leg (one leg printed `nk=0`): 544.
  - The CAPLOG replay reproduced every replayable cap on every G row.
  - `b0k[0]` = 0 held on all rows: 48 at c2, 48 per c8 leg, 30 at c1s.
  - `m` = `l` held for t < 10 s on 1 872 of 1 872 c2 windows and 1 869 of 1 869 c8 windows.
- **Owed tests, now paid.**
  - The unfiltered fast `test_stubrun.py` at `1ecee59d` (LF export) printed `ALL OK`: 613 ok, 2 289 s.
  - Of the five `tools/l1` shell tests, three pass on the host.
  - `test_abort_witness.sh` and `test_lib_battery.sh` fail on the host only through Git Bash gaps: no `pgrep`, no `flock`, MSYS signal codes, Windows Python and `/tmp`. Both pass on the VM after 60b: 12 of 12, and ALL CHECKS PASS.

**Scored and registered readings.**

| reading | cell | label | value | registered prediction |
|---|---|---|---|---|
| **allowance shortfall D** (§3.1) | c2 | **SHORT, robust** | D = +1.692 ms, 90 % CI [+1.588, +1.836]; bracket arms 160: +1.309, 176: +2.069 (both SHORT) | +0.9 (flat) to +2.2 (secant), SHORT resolved |
| floor convergence | c2 | CONVERGED | E = 0.18 ± 0.03 ms (τ 0.3) | NOT-CONVERGED or UNRESOLVED under the slope; CONVERGED if flat |
| sub-floor slope | c2 | UNRESOLVED | b = 0.0113 ± 0.0016 ms/slot | resolves flat vs secant 0.020–0.027 |
| floor convergence / slope | c8 leg 0 | CONVERGED / SLOPED | E = 0.55 ± 0.02 (τ 1.0); b = 0.0171 ± 0.0005 | CONVERGED or UNRESOLVED |
| floor convergence / slope | c8 leg 1 | CONVERGED / FLAT, **uninformative (post hoc)** | as DECISION 7 registers, the fit used W 32 / 64 / 96, where leg 1 is starved (23 / 51 / 78 % of windows with `nk` ≥ 10), and extrapolated to W₁ = 256, leg 1's floor pool: b = −0.030 ± 0.004 (≈ 7 se below 0), E = −7.6 ms | a floor-pool move to 256 at the latest, as registered |
| floor slope | c1s | FLAT (no τ registered) | b = 0.0009 ± 0.0002 | — |
| **B2** (§3.2) | c8 | **UNSCOREABLE** | 0 of 6 replicates anc-bound; pooled bind (DECISION 12) at the two bracket arms 0.50–0.79 (< 0.9); per leg: leg 0 1.00 on all 12 rows, leg 1 0.00–0.58 | P(≥ 5 of 6 scoreable) ≈ 0.90 |
| **control sign** (§3.3) | c1s | **UNDERPOWERED** | 0 of 12 scoreable: all 12 COLLAPSED | UNDERPOWERED 0.06 |
| **G/C** (§4) | c1s | **UNRESOLVED** | 0 unflagged pairs of 10 | P(≥ 4 unflagged) 0.27–0.83 |
| R-sign | c2 / c8 | PASS / PASS | control R̃ 0.093–0.095 / 0.55–0.72 | R̃ < 1 |
| P1 (b0p ratio ≥ 3) | c2 / c8 / c1s | HOLDS ×3 | 5.00 / 6.14 / 42.96 | c2 ≈ 4.4 (possibly < 3 on the W 16 base), c8 ≈ 10, c1s ≈ 60 |
| P3 (Δℓ against q·ℓ̂_c) | c2 / c8 leg 0 / c8 leg 1 | RISES / RISES / FLAT | +5.17 ± 0.88 / +4.90 ± 0.53 / +2.06 ± 0.40 ms | c2 +1.3 to +1.5 (likely UNRESOLVED), c8 leg 0 +2.7 (RISES), leg 1 FLAT or UNRESOLVED |
| B1 (median LATE err > 0 on a valid calibration row) | c2 / c8 / c1s | FIRES / FIRES / does not fire | c2 1 of 5 (s42 r1 +0.075 ms); c8 2 of 12 leg-rows, both leg 0 (s42 r3 +0.051, s7 r4 +0.709); c1s 0 of 6 | does not fire |
| drift (fallback iff p < 0.05 and change > q·ℓ̂_c) | — | not triggered | c2 b0p +1.41 ms (Spearman +0.80, p 0.13) vs q·ℓ̂_c 0.56; c8 leg 1 b0p +4.09 (p 0.30) vs 3.31; c1s b0p +0.14 (p 0.058) vs 0.22; every `l` change at p ≥ 0.29 | unpowered, in-session |

**What each reading means.**
1. **c2: SHORT, with the floor converged (R6.7 open item 2).**
   - A floor read too high lowers D, so it can only hide a shortfall. By §2, SHORT is therefore a lower bound in any case.
   - The convergence gate says the bound is tight to about 0.18 ms. F_c = 8.68 ms is the W 16 median; the slope is fitted over W 16 / 32 / 48, and the extrapolation to W → 0 gives 8.50 ms.
   - The knee needs 1.69 ms more window-minimum rise than σ + q·ℓ̂_c supplies (σ ≈ 1.7–1.9 ms, q·ℓ̂_c = 0.56 ms at q = 0.10).
   - R̃₆₇'s sign witness agrees on 6 of 6 replicates. R̃₆₇ reads 0.925 already at W 160, and its crossing is `<160` on every replicate: below the knee W₉₅ ≈ 166–170, not located between 48 and 160.
   - R̃'s own crossings, 160.5–163.8, are a different recipe: calibrated ℓ̂ with the anchor.
   - **This is a defect finding for the allowance form and needs a law-ledger verdict** (FORMULA-FIRST). It is not a footnote. It sits under `law-ledger.md` F01, the store law R6.7 re-derives, as R6.7's open item 2.
   - D sits near the secant end of the prediction, although the sub-floor slope reads about half the secant (0.011 against 0.020–0.027).
2. **c8: B2 is unscoreable because leg 1's anchor does not bind after its floor pool moved.**
   - §3's premise (`s60abs.out`: c8 anc 1 from W 320) was read on §56 at the old calibration.
   - In session, per leg at W 320 / 384 (the replay's test m ≥ l + b0), leg 0 binds 1.00 on all 12 rows and leg 1 binds 0.00–0.58.
   - Leg 1 is starved at the floor pools, so it calibrates at W 256 (T_c 36.8 ms), as §5 registers.
   - DECISION 12 pools the (leg, window) terms, so the 0.9 rule cannot pass while leg 1 is unbound. Leg 1 carries ≈ 16 % of the delivered share (one W 512 row).
   - The combination of the pool move and the pooled rule makes B2 unscoreable by construction. The registration did not foresee it.
   - The crossings are reported only:
     - R̃ 600–690 is a chord between the W 512 arm (R̃ 1.08) and the W 1 280 arm (0.68), with no arm in between.
     - R̃₆₇ 509–571 crosses just above the 512 arm (arm medians 1.02 and 0.66).
   - Both A.4's reading of "above the band" and the ≈ 405 prediction assume anc 1, which did not hold. So nothing about the form is decided at c8, and no cause is attributed.
3. **c8 leg 1: the floor gate is uninformative, a gap in the registered rule.**
   - §2 fits each leg over the three floor pools with E = b·W₁, and DECISION 7 takes W₁ from the leg's floor pool. The moved pool is covered only by §2's bound for leg 1 (0.008 ms/slot × 160 slots).
   - So the scorer did what was registered. The fit runs over 32 / 64 / 96, where leg 1 is starved, and extrapolates to 256.
   - FLAT and CONVERGED pass only because their thresholds are one-sided: b is ≈ 7 se below zero and E = −7.6 ms.
   - Leg 1's T_c itself (36.8 ms) is the median at W 256, as registered. The gate does not feed R̃.
4. **c1s: the control sign and G/C are unread because the collapse rule flags the control pool.** Measured per W 2 048 row (`[DIAG]` excess, rule input).
   - **Not collapsed:** 15 of 22 rows have quinn qcwnd below the BDP on ≤ 3 % of windows. Their excess is E50 13.0–17.3 ms and E90 18.5–42.8 ms, with 94–99 % of ticks above X = 6.1. Excess here is the EWMA `rtt=` minus the row's minimum `rtp_us`.
   - **Collapsed:** 7 rows read qcwnd shares 0.42–0.95, with E50 23.8–101.5 ms and goodput 221–671 Mbit/s: G r5, r8, r9, r11, r12; C r7, r10.
   - So the EWMA RTT over a 2 048 pool's standing queue alone exceeds X, which was derived on §56's small pools and checked on §57a's 448–512.
   - The genuine collapse rate is 7 of 22 (0.32), inside the registered 0.29–0.48.
   - **This is a measurement defect of the frozen rule at the control pool.** It is recorded here and not re-scored.
   - **Post hoc, decides nothing:**
     - With the qcwnd share as the flag, 5 pairs have both rows unflagged (r1, r2, r3, r4, r6).
     - Their CPU s/GB log(G/C) (DECISION 18) is +0.016, +0.026, +0.011, +0.017, −0.027, a median of +1.6 %. That is against the registered prediction "within +1 %" and the PERTURBING threshold of +10 %. §4 needs at least 4 unflagged pairs.
     - Goodput log(G/C) on the same pairs: a median of −2.4 % (−0.145 … +0.016). A.6 sizes goodput at 13 pairs.
     - The qcwnd flag misses at least one degraded row: G r1 (752 Mbit/s, E90 42.8 ms, qcwnd share 0.01).
     - The arm-median R̃ at W 2 048 is 0.856, and printed R is 1.12.
5. **P3 RISES at c2 by 5.2 ms,** about 3–4× the prediction.
   - Post hoc, most of the gap is the base: the prediction was made against W 128 rows, and the floor pool's `l` is 5.58 ms at W 16.
   - ℓ̂, the lifetime echo minimum, is therefore load-dependent at the operating pools. That puts ℓ̂'s source into R6.7, as registered.
   - c8 leg 1 is FLAT, so the two ℓ̂ recipes coincide there.
6. **B1 fired on 3 leg-rows,** against "predicted not to fire".
   - The registered consequence is one sentence: where B1 does not fire, R − 1 ≥ σ/T̂ > 0 holds on that row by construction (§2). On these rows that guarantee is absent. The registration names no further consequence.
   - It does not touch the R-sign gate, which is read at the controls and passed.
   - The `b0bound.out` margins (+0.43 / +0.46 / +0.19 ms) and ō were derived at caps 64–256. The floor pools were untested, as §2 said.
7. **P1 held at every cell,** including c2 on the W 16 base (5.00 ≥ 3). The first-K brake latches the start-up queue, so the boot hold and n₀ stay terms of the law (R6.7).
8. **Drift:** the fallback did not trigger. But c2's b0p moved +1.41 ms across the session (p 0.13, unpowered) against q·ℓ̂_c 0.56, and c1s's b0p has p 0.058.
   - D reads no b0p.
   - R̃ does read it, so the c2 R̃ crossings carry that uncertainty.

**Defect findings needing a law-ledger verdict** (FORMULA-FIRST, MEASUREMENT DISCIPLINE 17–18):
- the allowance σ + q·ℓ̂ at c2 (SHORT, 90 % lower edge +1.588 ms, on a converged floor), under F01;
- ℓ̂'s load dependence (P3 RISES at c2 and c8 leg 0), under F01's inputs (E03, the lifetime minimum).

**Pending, added by this Result:**
- **The ledger verdicts** on the two findings above, in R6.7's derivation: the allowance form and ℓ̂'s source.
- **A collapse rule that separates collapse from a large pool's standing queue.** The qcwnd-below-BDP share alone misses G r1, so it might be combined with an excess over the row's own LATE `tmin`. It must be re-registered before any new c1s control-sign or G/C reading.
- **The floor-gate rule for a moved leg pool.** In the next registration, as an amendment: fit the leg over pools at and above its own floor pool, or read UNSCOREABLE. Add a scorer test with a moved leg-1 pool.
- **B2's anchor rule with an unbound leg:** per leg, or leg 0 only, decided before B2 is registered again. Leg 1 is unbound at its W 256 calibration.
- **B1's margins and ō at the floor pools.**
- ~~Merge the harness tools to main~~: done, `1cd13f5c` (tools/l1 by path; the gauge's engine code stays off main).
- ~~VM cleanup~~: done 2026-11-16 after the ledger commit (the §60 home and `/dev/shm` dirs removed).


## 60-pre. Engine suites and the reference binary for the store-law gauge session (no battery) — pre-registration (before any VM contact)

**Why.**
- The §60 session (B-1; registration on `s60-design`, entered into status before 60a) runs on main + the store-law gauge (`slaw-gauge` `d416d59b`). That tree misses `ref56-manifest.sha256`, so every launch would otherwise run the full engine suites, and a rehearsal runs none.
- This step runs the suites once, and pins the binary and a reference manifest that the rehearsal, 60a and 60b reuse.

**Tree.** `s60-tree` `cb88df3a`: main `aefe6d97` + `slaw-gauge` `d416d59b`. The merge is clean, and `raptorpath-math`, `raptorpath-wasm` and `raptorpath-core` are unchanged against main. Later tools-only commits keep the manifest HIT.

**Run.**
1. `tools/l1/suites_run.sh` on the tree, under both locks: release, doc, wasm, core, debug and parsers. This is the first full engine suite run since the math-crate merges and the first Linux run of the gauge's tests.
2. On TESTS-OK, `tools/l1/binid_run.sh` (crate-directory build, as the harness does) on the same archive. Its sha is the registered `bin_sha` that the rehearsal, 60a and 60b must each match.
3. Then `ref60-manifest.sha256` is generated with `ref_manifest.sh` from the same tree, committed (tools only), and passed as `LD_REF_MANIFEST` to every later launch. A suite-reuse MISS stops a launch.

**Failure rule.**
- A REAL failure inside the gauge's own tests (`slaw*`, `gates::` RWM_SLAW, `slaw_reachability`) blocks §60.
- A failure outside them is a finding about main. It is re-run on main alone (base tree). If it fails identically there, it is recorded as main's defect and waits for diagnosis or an owner ruling. If it fails only with the gauge, it blocks.
- `decision_trace_pin` must stay 7/7; any digest change blocks.

**Length.** About 50–60 min of suites plus about 10 min of build. Host raptorpath cargo is paused; the builds of other projects on the host are recorded, not controlled.

**Erratum and progress to "Owner decisions (2026-11-14)" (same day).**
- **Item 3, "ρ is inert at Auto and Realtime":** this holds under the shipped R (the cut form) only.
  - Under the block-exact candidate C6 (`docs/derivations/pfec-continuous.md`), R exceeds the slider's 1e-3 step at Realtime on all five audit cells (1.2e-3 to 4.1e-3) and at Auto on one cell. So ρ would act on 1–4 of 100 positions there.
  - The T_cut numbers (8 and 26 of 100 positions at harsh Bulk) are unchanged under either form, because R = ε at r* = 0.
  - (ii) lands first, as decided. Its R values are provisional until item 2 lands.
- **Item 2, the P_fec derivation (branch `pfec-derive`, open):**
  - **Root cause confirmed.** The paper defines P_fec per lost symbol (`fec-arq-model.md:205`), but §4.2 (`:693–699`) derives the window event, which includes zero losses. That is a paper-text erratum.
  - **C6 is the exact per-loss law for a block window.** It matches the repo's block oracle (ratio 0.99–1.01), is exactly 0 at r = 0, and is continuous.
  - **Neither the engine nor the sim decodes a block.** Both decode a sliding window. Against a sliding decode with no deadline, an upper bound on recovery, C6 sits 0.04–0.15 below at A* 4–7. At W 201, where `r_saturation` runs on L1 (`wsize=201` in §57b's client logs), its tail is ≈ 100× off.
  - **So no form ships.** The path is to derive the sliding per-loss law against an executed decoder, with Fable.
  - **Correction to my message to the owner:** the block law's r* under-provisioning does not carry to the engine. The sliding check has the shipped r* meeting the Auto target at c2 and c3, and Realtime is unresolved.
  - **On L1** (Bulk, `--window-reliable`), r* stays bit-identical 0 under any form.
- **Correction to the P_fec bullet above (2026-11-14, later the same day; `pfec-derive` `b252933b`, round 6).** Two readings in that bullet treated a one-sided bound as a value, and both are withdrawn: "its tail is ≈ 100× off" and "the sliding check has the shipped r* meeting the Auto target".
  - **The Python sliding decode has no deadline,** so it is only an *upper* bound on recovery. Its premises are uniform repair spacing (the engine's `TaperBudget` front-loads), one GE chain and Δ ≤ 8.
  - **What stands, in bracket language:**
    - C6 does not ship. It lies above the bound at W 201 (c3, at half the cut-off rate), needs a second resource bound, and would multiply the shed budget 6–11× unverified.
    - Under the bound's premises, the shipped form over-predicts recovery at A* 3–7 in 20 of 20 cells, so its R is too small there. At the engine pin (ε 0.048, r 0.34, A* 4, σ² 3.76), P ≤ 0.458 against the shipped 0.90.
    - The engine's r* adequacy at Auto and Realtime is **unknown either way**. The tail sample cannot resolve the target (c2: 0 / 4 101 holes; c3: 2 / 7 472).
    - The one executed datum (an L0 decode share of 0.71–0.84, one rep, inputs unrecorded) cannot be placed.
  - **Next:** an executed sliding oracle (`pfec-continuous.md` §8.2, design only). It must call engine code, not copy it. The trailing-span seat is inline in `emit_source.rs`, so it is either extracted as a pure move, which also gives `decision_trace_pin` its missing seat, or a copy is witnessed against `[SPAN]` lines.
    - Known answers come first: P = 0 at r = 0; the block oracle reproduced in a block configuration; P(∞) matching the Python bound.
    - Each caller's deadline is fixed with Fable.
    - The registration comes before the run.
- **Second correction (same day; `pfec-oracle-design` `2a622525`).** The caveat "the engine's `TaperBudget` front-loads repairs after each ack" is withdrawn.
  - `TaperBudget::accrue` has one production call site (`emit_source.rs:543–549`). It is passed `flat_rate = window_rate.min(spare)` and the same `spare`, so grant == rate on every call; the taper shape is inert as wired. Checked by reading and a 10⁵-call Python port; `taper_emission_l0.rs` asserts nothing.
  - The ledger verdict (a degenerate law as wired) waits for the oracle's K0 in Rust.
  - **The open premise is now the emitted share W1,** not the spacing. Acks cancel repair debt once per sender-loop iteration (`net/mod.rs:3481–3482`), so the share depends on m̄, the sources acked per advance, which no gauge has settled.
  - **Oracle spec (open):**
    - It calls the engine's RLC encoder/decoder, `TaperBudget`, D(δ) and the span seat. The span seat is extracted once, after phase 6, under the VM release-sha proof.
    - The debt loop is copied, and K4 witnesses it against engine W1 on a recorded row.
    - Known answers K0–K4 come before any reading.
    - It is decidable at A* 3–7 (shed budget), report-only in the W 201 body. The tail rule is in miss counts (T = 310 at c2, 171 at c3, per 10⁵ holes).
    - **Needs:** one VM evicting-seat row with `RWM_DIAG` (c2/c3 Realtime, not `--window-reliable`) to read m̄.
- **Visualizer fixes merged (`viz-fix`, main `0857a0cd`):**
  - the dead classic-hint machine is removed, and the named hints are now an alias table onto the one continuum constructor;
  - the W11 β = 0 guard is gone;
  - a W22 bounding test is added;
  - the store text is corrected per cell (2048 is a clamp measured pinned; 512·N is an underived constant);
  - the model-vs-engine table rows (C04, S05) and the stale references are fixed;
  - the goldens are unchanged, with no re-capture; 39 native wasm tests and both mjs gates pass.
- **The shared-core steps 0.1–0.3 are merged (main `e9188e01`).** The engine edits are test-only. `decision_trace_pin` was captured on Windows; the next VM suites check it on Linux.

**§57b Result (`LD_PLAN=57b`, the c1d pool sweep and the transfer test T, REPORTED ONLY): T = UNSCOREABLE.** Every scoreable (a) point passes, but c1d's smallest store-bound pool does not reach q_ℓ = 2.1 ms. (2026-11-14; rows 16:30–17:01Z on main `02ecbdd7`. Host cargo was paused throughout. Ledger: `docs/l1-raw/s57/rep57b-*`, `57b-*`.)

**Integrity.**
- **Binary:** `8f313616…`, the same binary as §54's CAND, §56 and §57a.
- **Suites and tests:** suite-reuse HIT; parsers and bash tests rc 0.
- **Smoke:** SMOKE-PASS on P192 and P2048.
- **Rows:** 30/30 counting, all LIVE; 0 VOID, 0 replaced, 0 DNF, 0 UNBOUND.
- **Flags:** both-leg flag on 3/30 rows (two of them control rows); one-leg-only on 14/30.
- **Host score:** equals the VM's (`rep57b-score-host.txt`).

**Per pool** (medians; ratio = goodput/control; the within-replicate ratio pairs each row with the same replicate's control, all rows):

| pool (W) | a/W | status | q_ℓ ms (unf) | goodput unf / all | ratio unf / all | within-replicate ratio | release batch |
|---|---|---|---|---|---|---|---|
| 2048 (4 096), control | 0.67 | READ (n_u 3) | 31.5 | 765.8 / 729.4 | 1 / 1 | 1 | 195 |
| 768 (1 536) | 0.72 | NOT-STORE-BOUND | 10.7 | 741.5 | 0.968 / 1.017 | 1.027 | 151 |
| 512 (1 024) | 0.76 | NOT-STORE-BOUND | 7.3 | 731.0 | 0.954 / 1.002 | 0.980 | 130 |
| 384 (768) | 0.77 | NOT-STORE-BOUND | 4.8 | 727.3 | 0.950 / 0.997 | 0.997 | 109 |
| 256 (512) | 0.81 | READ (n_u 4) | 3.0 | 680.4 / 677.8 | 0.888 / 0.929 | 0.899 | 80 |
| 192 (384) | 0.84 | READ (n_u 5) | 2.6 | 611.2 | 0.798 / 0.838 | 0.848 | 60 |

**T, as registered.**
- **(c) The control's own queue** is q_ℓ = 31.5 ms, against the predicted 13–21: OUT, above the band and not below 10 ms. Its unflagged median rests on 3 rows, because 2 of the 5 control rows are both-leg collapses.
- **(a) Offsets:**
  - δ₀(c1d) = 2.64 ms (P192) against δ₀(c1s) = 2.1 ms, a shift of +0.54 > 0.5. So (a) is read on both axes.
- **Raw axis:**
  - 2.1 ms lies below c1d's measured range (2.64–3.01 unflagged; 2.64–3.25 all rows): UNSCOREABLE.
  - 2.7 ms: 0.813 (unf) and 0.847 (all) against 0.89 ± 0.08: PASS. The unflagged value is d = −0.077, at the band's edge.
- **Corrected axis:**
  - 2.64 ms: 0.798 / 0.838 against 0.79: PASS.
  - 3.24 ms: out of range for unflagged rows (UNSCOREABLE); 0.928 for all rows (PASS).
- **Readings:** unflagged = UNSCOREABLE, all rows = UNSCOREABLE (raw) / PASS (corrected), so **T = UNSCOREABLE.**
- **(b)** is not readable: no store-bound pool lies at q_ℓ 4.7 or 6.8 ms.

**What it means (registered).**
- T is not tested. The registration's no-extrapolation rule applies: the c1d sweep's store-bound range (q_ℓ 2.6–3.3 ms) does not reach the first falsifying point.
- No scoreable point fires.
- **(c):** the control's queue (median over all 5 control rows) is 31.5 ms, above the predicted 13–21. Only the goodput denominator rests on the 3 unflagged control rows.

**Registration misses.**
- **W 1 536 was registered as the first store-bound candidate.** The first pool to clear the a/W ≥ 0.80 gate is W 512, and only by 0.005.
- **The design could not return PASS** unless the smallest read pool sat at q_ℓ ≤ 2.1 ms (`eval_a` / `read_a`). The registration asked only for one arm at ≤ 3.0 ms. Fix this in the next registration.
- **Resolution:** P192's q_ℓ rows span 1.83–3.88 ms (sample sd 0.78 ms), so its median is good to about ±0.35 ms. The falsifying region is 0.6 ms wide.

**Reported readouts (post hoc where marked).**
- **The store acts below the gate.**
  - Occupancy follows W: ≈ 1 100, 780 and 590 symbols at W 1 536, 1 024 and 768.
  - q_ℓ falls with it, from 31.5 to 10.7, 7.3 and 4.8 ms.
  - Those pools sit at a/W 0.72–0.77, under the registered 0.80 gate (rule 3), so T excludes them.
- **Goodput against the control depends on the denominator.**
  - The 5 control rows span 704–768 Mbit/s, and 2 are both-leg collapses. Their unflagged median (765.8) is 5 % above the all-rows median (729.4), which is the whole difference between ≈ 0.95 and ≈ 1.00 at W 1 536–768.
  - Within-replicate ratio ranges:

    | W | within-replicate ratio range |
    |---|---|
    | 1 536 | 0.885–1.066 |
    | 1 024 | 0.942–1.034 |
    | 768 | 0.915–1.039 |
    | 512 | 0.764–0.963 |
    | 384 | 0.793–0.859 |

    Replicates 3 and 4 divide by collapsed control rows.
- **Post hoc: c1d at the same total W reads like c1s.**
  - c1d's unflagged ratios at W 1 024 / 768 / 512 / 384 are 0.954 / 0.950 / 0.888 / 0.798. §56's c1s reads 0.96 / 0.94 / 0.89 / 0.79.
  - The q_ℓ values are close too: c1d 7.3 / 4.8 / 3.0 / 2.6 ms; c1s 6.8 / 4.7 / 2.7 / 2.1 ms.
  - Rule 3 excludes three of those four c1d pools from T, and the unflagged ratios carry the denominator's ±5 %. So this is a coincidence to register, not a transfer result.
- **Release batch / W** is 0.05 at the control and rises from 0.10 (W 1 536) to 0.16 (W 512 and 384). Ack wakes per loop fall from 14.1 (control) to 5–7. At c1s, §56 read ≈ 0.2.
- **Session position (the committed readout):** goodput over each pool's median is 1.013 in slots 1–15 and 1.000 in slots 16–30.
  - The battery lasted 31 minutes, and §57a's slowdown began about 34 minutes into its rows.
  - So this readout cannot separate host builds from elapsed time.
- **Collapse:** the both-leg flag fires on 3/30 rows (0.10, against §53's 5/30). One-leg-only fires on 14/30, at every pool.

**Next.** T needs a c1d pool with q_ℓ ≤ 2.1 ms. Three cases are open:
1. A smaller pool (W < 384) lowers q_ℓ further.
2. c1d's no-queue floor sits near 2.6 ms, so its offset, not its queue, fills the gap.
3. The one-leg flag (4 of 5 P192 rows) inflates q_ℓ. P192's one clean row reads 1.83 ms.

A next registration must:
- require the smallest pool at q_ℓ ≤ 2.1 ms;
- size n from the 0.78 ms row sd;
- fix the control denominator, for example as a within-replicate ratio.

This goes to the derivation's open list with §57a's burst hypothesis. Nothing is launched without a registration. B-1 (the gauge build) is next on the store-law track.

## Owner decisions (2026-11-16): after the §60 Result

The owner took the coordinator's recommendations ("take your recommendations"). Fable reviewed the evidence behind each pick; the picks are the coordinator's. Each item states what it unblocks.

1. **U1 (W1.2, `docs/derivations/w1-who-limits.md` §7): the store limit is in charge of latency, and the congestion controller is the safety net (candidate A).**
   - Paper §8.4 ("the substrate reads no δ") stands, and candidate C (the substrate as controller) is not pursued.
   - Unblocks: the δ-seat design on the store, together with U2.
2. **U3: a small Bulk goodput cost is accepted for about 10× lower delay.**
   - Evidence (§60's static arms, not the closed-loop law): at c2, W 256 matches the control's goodput (90.1–90.5 against 89.9–90.4 Mbit/s) at a window minimum of 19.8 ms against 200.5 ms. c8 W 512 costs about 1 %. The price sits at c1s, where §56 found that every pool cut pays.
   - The cost each δ point may carry is still set by the δ-seat law, which waits on U2.
3. **ADR-0068 Addendum 3 (branch `adr68-amend`): accepted, conditional on §48d.**
   - The hybrid controller grows out of `bbr_rs`; no second controller is built.
   - This is consistent with "continue `bbr_rs`" (2026-11-12) and the S3 vendored pacing seam.
   - Docs-only merge.
4. **Per-leg state (W3.2, `docs/derivations/w3-per-leg-state.md` §6 question 4): on revival, s is reseeded,** as today's R1 does.
   - This is the only open question Phase A (A0–A4, relocation with zero behaviour change) needs.
   - So `place-per-leg-impl` Phase A can start.
   - Questions 1 (`queue_target_mult` = 1 + q(δ)) and 2–3, 5–8 belong to Phase B and W3.3. Question 1 is held with U2.
5. **Binary identity: ADR-0077's identical-sha waiver is unchanged.** Zero-behaviour steps (e.g. shared-core moves, per-leg Phase A) are instead stacked into one guard run against main, bisected only on failure.
6. **Lever B stays paused** (§54's decision stands). Its case is weaker now: §54 showed that a smaller store pool alone clears the holes, and the store law it waits on has moved further out.
7. **Generation-coding stack (§3.2): stays parked, no disposition yet.** It unblocks nothing in the current chain, and archiving would cost a guard run.

**Held for the store-law verdicts.** The verdicts landed as `store-law-r68.md` (merged `12310902`), and two of the three held items were then decided (2026-11-17, below).

**Owner decisions (2026-11-17), on Fable's recommendation:**
8. **U2: the dial scales a measured knee margin, and Bulk keeps all of it.**
   - The allowance takes the form g(δ)·S_F + q(δ)·F̂.
   - S_F is the floor windows' median − minimum, and F̂ the measured floor (r68 §1.4 and §2.4).
   - g(Bulk) = 1. g's Realtime end is set by measurement, the c3 sweep's need/S_F reading against c2's 0.80, and is not declared now. q/q_B (r68's replay row) has no provenance in this role.
   - If c3's ratio disagrees, S_F is not the knee's term, and the form is re-derived.
   - Whether Realtime may land below the knee is decided later, from a price table that includes Tp50.
   - "Flexible" means one formula with a measured multiplier, not a runtime knob.
   - **Scope:** this rests on c2 only. At c8 every candidate lands at least 145 slots above the knee, and c1s is unread.
9. **The c3 sweep runs on the VM.**
   - Single leg, static pools, on a rebuild of the §60 gauge tree that must log `bin_sha` `e410d640`.
   - It carries r68 §3's rows plus 2–3 arms between W 48 and W 160, to price the side below the knee.
   - **Needs:** a single-leg c3 cell (today c3 exists only as c8's second leg), its own registration, and a rehearsal.
10. **Per-leg question 1: `queue_target_mult` = 1 + q(δ).** q's band is unchanged under decision 8.
11. **F2's fix format: (A), one framing for every symbol** (owner, 2026-11-17, taking Fable's recommendation).
    - Every symbol at every hint uses the same wire framing, so the receiver no longer depends on `WindowStart` announcing packing.
    - (B), a per-frame packed bit, was not taken: it keeps two framings and a per-symbol branch.
    - The fix comes with an ADR, a protocol bump and the red-first repro. If Bulk's bytes on the wire change, it also needs a guard against main.
    - The design is on `f2-design` (`7d841dfa`: `docs/derivations/f2-framing-design.md`, ADR-0078 Proposed; three review rounds with Fable).
12. **F2's safety net: fail closed** (owner, 2026-11-17).
    - After the fix, an unframable symbol can come only from an encode or decode bug. ARQ cannot repair it, because a resend repeats the same bytes.
    - The receiver counts it, logs `[UNFRAMABLE]`, and closes the tunnel. A stall or hole behind healthy counters was the alternative not taken.
    - **Size, by the recommended default** (not overruled): each receiver decodes with the size its sender announces in the reliable v10 handshake. It is reversible to refuse-on-mismatch without a wire change.
    - Mixed-hint pairs stay unsupported.
13. **s62, the c3 jitter sweep, runs after F2's §63** (owner, 2026-11-17).
    - It costs about 3.4 h typical (6.5 h worst) over five launches, with host cargo paused, on binary `e410d640` (Bulk; no F2 fix needed).
    - **Its one scored reading:** the knee need at J = 0 on the knee-native window minimum, gated on the floor's count drop being < 0.5 ms.
    - Sketch: `knee-margin-rederive` `c6c55958`, doc §5. It needs its own registration, a tools-only harness and a rehearsal.
    - **Shape changed by its review (2026-11-17; the coordinator's default, which the owner can overrule):**
      - About 1.4 h at J = 0 only, one rehearsal and one cell, with no prediction for the need.
      - It cannot settle decision 8 alone. It shows whether the counting problem comes from jitter, and which derivation comes next.
      - J = 3 is a later call (§62).
14. **Test and bench cost** (owner, 2026-11-18, "do the compiler cache and the reduced guard if Fable suggests it").
    - **Compiler cache: yes, as a trial, on host builds only.** `RUSTC_WRAPPER=sccache` (0.18.0, already installed) goes on every agent's host cargo command, in the worker rules.
      - It is a cache keyed on compiler inputs, not a shared target dir: one target per tree, the mutex and `CARGO_BUILD_JOBS=1` all stay.
      - Never on the VM, where binaries are identified by sha and the suites build from an empty target.
      - The first measured saving gets reported.
    - **Reduced guard: not taken.** Fable does not suggest it:
      - the reuse changes (suites only on the pinned sha, guard suite reuse on a byte-identical tree, a launch reusing its rehearsal's binary and tools tests) remove about half a guard's wall time without touching its rows;
      - F2's framing commit left both pins unchanged and still cost +45–70 ns per symbol on an unpinned path, which only the bench saw.
      - Revisit only with a table of past zero-behaviour guards and one specific reduced form with its detection power, as a derivation.

Still held: whether the slaw gauge's engine code reaches main (a stacked guard) or is dropped. The c3 sweep uses it on its own tree.

**Not yet askable:**
- the rate-law calls (need §51's r per cell);
- `bbr_rs` promotion (needs §48d).

## Owner decisions (2026-11-14): the visualizer, shared code and two model laws

All three were decided as recommended after Fable's review.

1. **Scope rule: amended (ADR-0077, CLAUDE.md).**
   - Formulas move to `raptorpath-math` and decisions to a new `raptorpath-core`.
   - Engine-side moves are zero-behaviour-change, proven by pins, an ns bench and a §41c guard. The guard is waived only for an identical release-binary sha.
   - Only class-A code moves from now on. The existing debt in `raptorpath-math` (W22, W14) is named and carries bounding tests.
   - A decision is shared only when the model holds its inputs, with an "executes" witness. Store, recovery and placement stay engine-only for now.
2. **FEC formula jump (`p_fec_normal`, W22 / ledger R03): a derivation is opened.**
   - The paper's Φ form fails the engine's r = 0 pin (`residual_loss_after_fec_is_the_design_residual`). With no repair symbols it still predicts ≈ 39 % recovery in that test's setting.
   - The shipped cut to 0 at r ≤ ε/(1−ε) is the step. Neither form is kept as is.
   - The derivation looks for a form that is continuous and exactly 0 at r = 0. Route: rate-law track (Opus derivation, paper line, a registered r per cell, then a guard).
   - It changes the engine's rate law, because `r_saturation` is on the default path.
3. **Give-up rule in the model: option (ii), landing before decision 2's fix.**
   - Rule: give up a hole iff age > D(δ) and given_up + 1 ≤ min(1 − ρ, R)·src_total. It replaces V5/V6 (the cartoon's budget choice) and W14 (the sim's T_cut horizon).
   - **The basis is corrected:** the engine's shed budget is R alone, and ρ enters it only as the seat (`shed_on`). So the 1 − ρ cap is a model law the engine lacks. It needs its paper §5.6 line and a bounding test.
   - **Two consequences are recorded:**
     - At Auto and Realtime, R is below the ρ slider's step, so ρ is inert over the slider there. This is a degenerate-law finding for the law ledger.
     - Dropping T_cut diverges at harsh Bulk (8–26 of 100 slider positions at ε 0.2–0.3), so it gets a bounding test.
   - Until decision 2 lands, the continuity gate excludes only the one W22 step its bounding test covers.

**§57a Result (`LD_PLAN=57a`, the emission-burst block at c1s, REPORTED ONLY): UNDERPOWERED.** (2026-11-14; rows 14:08–15:38Z on main `ef3d5e0e`. Ledger: `docs/l1-raw/s57/rep57a-*`, `57a-*`.)

**Integrity.**
- **Binary:** `8f313616…`, §54's CAND, which §56 also ran (`bin_sha` in every row).
- **Suites and tests:**
  - Engine suites reused (`LD-ALL suite-reuse HIT`, 182 files).
  - Tools tests: parsers rc 0. The bash tests rc 0, their first run on this tree (Amendment 1).
- **Smoke:** one B256-512 row, unflagged (share 0.019), with a realised burst of 75.3 > 64. SMOKE-PASS, with no relaunch.
- **Rows:**
  - 63/63 counting, all LIVE; 0 VOID, 0 replaced, 0 UNBOUND.
  - 2 DNF, both collapse rows. 23 rows are collapse-flagged.
  - Bind fraction 1.000 on every arm.
- **Host score:** equals the VM's byte for byte (`rep57a-score-host.txt`).

**Arm readouts** (R = lap rate over LATE, on unflagged bound rows; goodput is over all rows, DNF = 0):

| arm | n_u / 21 | collapse share | b̄ | R /s (SE) | goodput, all rows | release batch | client CPU s/GB |
|---|---|---|---|---|---|---|---|
| B64-512 (control) | 15 | 0.29 | 48.9 | 79 161 (819) | 733.4 | 111.5 | 17.20 |
| B64-448 | 14 | 0.33 | 46.7 | 75 221 (848) | 686.5 | 94.1 | 17.72 |
| B256-512 | 11 | 0.48 | 78.9 | 75 524 (1 249) | 643.2 | 105.6 | 16.96 |

**Reading: UNDERPOWERED** (S = 0.62 < 2). E1/E2 are not read, and the arms are reported.
- z_null = −2.43 and z_hyp = −1.92. ΔQ_b = 15.0 slots (k = 0.234). The chord is 3 940 /s, so R_pred = 78 239 /s.
- **Gates:**
  - CPU: not confounded (16.96–17.72 s/GB).
  - Collapse: 0.29 against 0.48, a gap of 0.19 ≤ 0.25, so not BIASED.
  - E5: does not fire (release/W 0.218 and 0.206).
- **RISING did not occur.**

**Scorecard against the registered expectation.**
- **b̄₂₅₆ = 78.9,** against the expected 90–105. So ΔQ_b = 15, against 21–29.
- **Row sd = 2.5–3.3 k/s,** against the expected ≈ 1.1 k. This, not ΔQ_b, is what put S at 0.62.
- **The chord (3.9 k) falls inside** the expected 2–4.8 k.
- **Re-basing:** the control median of 79.2 k lies outside §56's 82.7 k ± 3.0 k (RE-BASED, drift −3.5 k). This is a position effect, not a level shift (below). The control's unflagged rows in replicates 1–8 have a median of 80.7 k (n 6), inside §56's band. From replicate 9 on, the median is 77.5 k (n 9).

**Post hoc (labelled, not registered readings).**
- **Session position.** Every arm is slower in replicates 9–21 (from 14:42Z) than in 1–8:

  | arm | 1–8 | 9–21 |
  |---|---|---|
  | B64-512 | 80.7 k | 77.5 k |
  | B64-448 | 75.6 k | 73.5 k |
  | B256-512 | 77.9 k | 73.6 k |

  - The unflagged subsets sit at different positions: B256-512's three lowest rows are r19–r21, where the control has no unflagged row.
  - A likely cause is not ruled out. Host cargo builds ran almost continuously from ≈ 14:20Z. The host is itself a KVM guest, so it may share hardware with the VM.
- **Within-replicate pairs** (both rows unflagged), which remove position:
  - B256-512 − B64-512: n = 6, all negative, median −2.2 k/s (−2.7 %), range −4.9 to −0.1 k.
  - B64-448 − B64-512: n = 11, median −5.3 k.
- **Hypothesis for a new registration:** a larger emission burst lowers c1s's rate, by the order of the store-holding prediction (−0.9 k) or more. The paired n is 6 and not powered.

**Consequences for §57b**, committed before its battery:
- Host cargo is paused during the battery.
- Goodput against slot order, and the within-replicate ratios, are reported-only readouts.

## Engine defect F1 (2026-11-14): `RWM_DELTA` pins the store cap at its floor

**Found by** the shared-core design (`docs/derivations/shared-core-design.md` F1), from reading only. It was then confirmed red first.

- **The defect.** `codel_setpoint_q` read both band ends through `delta_budget_b(hint)` → `delta_price`, which returns the `RWM_DELTA` override when set. Under the override, b_lo = b_hi, so q(δ) = 0/0 = NaN. The shipped value multiplier 1 + q (`RWM_DELTA_CAP`, on) is then NaN, and `pooled_store_cap` clamps to `STORE_CAP_FLOOR` = 10 symbols.
- **Red test:** `raptorpath/tests/delta_override_store_cap.rs` fails on main with "q(delta) is NaN".
- **Fix:** branch `f1-delta-nan` `39008a01`. The band ends come from the hint table; the override moves only the operating point.
  - Without `RWM_DELTA`, the values are identical: `formula_agreement`, `law_shape` and the store-cap tests are green on the host.
  - It is one site; no other law reads its band ends through the override (grep).
- **Who is affected:** every row run with `RWM_DELTA` set since the δ-cap law landed (`70777c78`, `d60f3ab0`, 2026-08/09).
  - `tools/l1/r_battery.sh`'s MID arm sets it. Whether any ledger row of it postdates the defect is still to be checked.
  - The planned §51 δ sweep depends on `RWM_DELTA`, so **F1 merges before §51**.
- **Merge path:** the engine suites on the VM (it changes `raptorpath/src`), with the next engine session. It is not batched into the shared-core zero-change steps.

## 57. Amendment 1 to §57a and §57b: the harness as built (before any VM contact)

**Harness.** Branch `harness57` (`2550768c`, `19dc65d7`), fast-forwarded onto main as tools only (`tools/l1`; `raptorpath/src` and `raptorpath/tests` unchanged, so HEAD still equals `ref56-manifest.sha256` and the engine suites are expected to be reused).
- `LD_PLAN=57a` / `57b`: plan 56's single-seat shape; stages `rep57a` / `rep57b`; scorer `pool57_score.py`, tests `test_pool57.py`.
- No row sets `RWM_STORE` (the plan table's store column is `def`, so the law stays on). 57a sets both `RWM_STORE_PATH_POOL` and `RWM_EMIT_BURST` on every arm; 57b sets the pool per path and nothing at 2048. Both knobs are scrubbed (`env -u`).
- The realised burst is Δ`eb_syms`/Δ`eb_bursts` from the first to the last LATE client tick. The `[GATES]` echo of pool and burst is checked on both ends.
- Stub run (`STUBRUN_FULL=1 test_stubrun.py`): 465 ok, 0 FAIL, including the 57a/57b rehearse, full, reuse-hit and reuse-miss paths. Every other `tools/l1` python test passes.
- The bash tests (`test_lib_battery.sh`, `test_abort_witness.sh`) cannot run on the Windows host (no `flock`); they fail the same way on 29fb7c8f. The rehearsal is their first run on this tree.

**Rules the registration did not state, fixed now (before any row exists).**
1. **c1d flag.** "Both legs below BDP in > 0.10 of windows" means min over the two legs of the per-leg below-BDP share > 0.10. This reproduces the registered 5 of 30 (§53, goodput 729 against 827) and 0 of 24 (§54). The one-leg share (max over legs > 0.10) is the registered second readout (11 of 30 on §53).
   - **Erratum to `docs/derivations/store-law/c1dflag.py`:** its `FLAG` column is the any-leg rule (16 of 30), not the registered one. The registered numbers come from the both-leg reading.
2. **c1d T̄.** The top-level `rtt=` is the max over live paths, so at two legs T̄ uses the per-leg echo rtt (printed `{:.0}`, rounded to 1 ms), per-tick values rate-weighted over LATE. The top-level form is reported as Ttop.
   - **Resolution, measured where it matters only at one path.** On §56's 18 unflagged single-path c1s rows, P384 and P512 included (q_ℓ ≈ 2–3 ms), the integer per-leg form and the 0.1 ms top-level form differ by ≤ 0.036 ms. (§53's c1d rows show 36–327 distinct per-leg values per row, but they are control rows at ≈ 20 ms of queue, the wrong regime.)
   - **No two-leg small-pool row exists yet.** The 57b smoke's P192 row is read in the rehearsal: per leg, the number of distinct LATE rtt values and their tick sd. The rule is calibrated on §56's unflagged P384/P512 rows, where the 0.036 ms bound holds: tick sd 1.02–1.44 ms, 7–10 distinct values. If either leg reads a tick sd below 0.5 ms or fewer than 5 distinct values, de-quantisation is not shown, and the full 57b launch waits for a re-sourced T̄ by a further amendment. If the P192 smoke row is c1d-flagged, it is no reading, and the rehearsal is relaunched under the same cap as 57a's smoke.
   - ℓ̂ is the lowest `rtp_us` in a row's first 2 s, rate-weighted over the legs.
3. **LATE at c1d** is t ≥ 5 s, as at c1s.
4. **57a statistics.** sd is the sample sd (n − 1). SE_pred² = SE₂₅₆² + (1 − k)²·SE₅₁₂² + k²·SE₄₄₈², k = ΔQ_b/64, not clamped. b̄ is the arm median of the realised burst over unflagged rows.
5. **57a outcome precedence** (the first that applies): UNREAD (an arm with < 8 unflagged bound rows, or no SE, or no realised burst) → INERT (b̄₂₅₆ ≤ 64) → RISING → UNDERPOWERED → E1 → E2 → PARTIAL → INCONCLUSIVE. RISING has no S condition, so it is read before UNDERPOWERED. CPU-CONFOUNDED and BIASED annotate the reading; they never replace it.
6. **Bind (57a).** All three arms are reduced pools, so the bind rule applies to each. An UNBOUND row is reported and enters no median, as in §56. In 57b the shipped-pool control is exempt.
7. **E5** fires on the two W 512 arms. B64-448 is reported against the same band but does not fire it.
8. **T (57b).**
   - The control gives only the denominator and readout (c).
   - A pool is read with ≥ 3 unflagged bound rows, in both readings. Unbound rows are dropped in both readings. The a/W gate (≥ 0.80, inclusive) is the median of per-row a/W over each reading's own rows, so a pool can be read in one reading and NOT-STORE-BOUND in the other.
   - A point passes at |d| < 0.08 and fires at |d| ≥ 0.08. Any fire among the scoreable points is reported as LOW, HIGH or MIXED.
   - One point outside c1d's measured q_ℓ range with the other passing: UNSCOREABLE. No c1d arm at q_ℓ ≤ 3.0 ms: UNSCOREABLE.
   - **Offset estimator, corrected.** The registration takes δ₀(c1s) = 1.5 ms from echo floors (T₀ − ℓ̂), but δ₀(c1d) from the q_ℓ of the smallest store-bound pool. By c1d's rule c1s itself gives 2.1 ms, and c1s read against its own points returns OFFSET DIFFERS (checked with `read_a`). Both offsets now use the same estimator: **δ₀(c1s) = 2.1 ms**, the q_ℓ of c1s's smallest reference point. The scorer constant (`DELTA0_C1S`) and its test change before the 57b rehearsal, with a test that c1s read against itself passes on both axes. This is fixed before any c1d row of this block exists.
   - Offset axis (read when |δ₀(c1d) − δ₀(c1s)| > 0.5 ms): c1d is read at x_ref + (δ₀(c1d) − δ₀(c1s)), with δ₀(c1d) = the q_ℓ of the smallest read pool. Both axes must be scoreable. Equal states stand; PASS on one and a fire on the other is OFFSET DIFFERS; two different fires are MIXED.
   - T is the all-rows reading if it equals the unflagged reading, and UNSCOREABLE on any disagreement (for example LOW against MIXED), not only pass against fail.
   - Readout (c), the control's queue, is the median q_ℓ over all live control rows.
   - A control q_ℓ below 10 ms prints CONTROL-QUEUE-LOW before (a).
9. **Replacement.** A VOID pool or burst echo is replaced once in both launches (the registration said so for 57a only).
10. **57b arm tokens** are `P<per-path pool>`, W = 2 × pool.

**Smoke abort on a collapsed row (57a).** §56's heavily collapsed c1s rows (flag share ≥ 0.68) send short bursts (LATE mean 15.8–26.7 against ≈ 47 healthy); the three lightly flagged rows (0.19–0.32) read 44–48. So a collapse-flagged B256-512 smoke row can read ≤ 64 even when the knob works. On §56's 9 collapsed rows, the burst before onset is 45–59 where readable, but 2 of the 4 P512 rows collapse at t = 1 s, so a pre-onset reading is often absent.
- The registered abort rule stands, and no code changes.
- **The trigger is mechanical:** the smoke check's FAIL line contains "the row is collapse-flagged". That row is "no reading", not a knob failure, and the launch is relaunched.
- **The cap is two relaunches per launch kind** (the rehearsal and the full launch each). A third collapse-flagged smoke stops the block until a further amendment. Every relaunch is reported in the Result.
- **An unflagged smoke row that reads ≤ 64 is a real ABORT-SMOKE.** Nothing is relaunched.

**Budget.** 57a's worst case (3 h 40) leaves no room for the engine suites (44–54 min). The rehearsal evaluates the reuse manifest. If its `launch.out` does not print `LD-ALL suite-reuse HIT`, 57a is not launched until a further amendment. 57b has room.

**Constants checked against the registration text:** re-base 82.7 k ± max(3.0 k, 1 %), CPU gate 5 %, collapse-rate gate 0.25, row minimums 8 (57a) and 3 (57b), band 0.08, a/W ≥ 0.80, q_ℓ ≤ 3.0 ms, control range 13–21 ms and low mark 10 ms, E5 0.17–0.24, SE factor 1.253. §56's P384 and P512 c1s rows all bind (bind fraction 1.0), so the bind rule does not reduce the n = 21 calculation.

**Tree pin.** The commit and the binary sha are recorded by the launch (`BINSHA.txt`, `COMMIT`) and copied into the Result.

**§57a note (2026-11-14, after the rehearsal smoke row, before any battery row; no rule changes).**
- **Seen data: the rehearsal's one B256-512 row** (main `ef3d5e0e`, c1s-400, 5 GB, LIVE, unflagged at share 0.049):
  - realised mean burst 66.9 (registered expectation 90–105);
  - release batch 84.7 per ack wake (release/W 0.165, below E5's 0.17–0.24);
  - R 81.06 k/s; a/W 0.89; q_ℓ 2.94 ms; bind 1.0.
- **Bursts per ack wake ≈ 1.3** (57 281 bursts over ≈ 45 k wakes), against ≈ 2.2 on §56's P512 rows (48 per burst, ≈ 107 per wake). At burst 256 one burst is already about one wake's release, so a larger knob value would not raise it.
- **Expected outcome: UNDERPOWERED.** With b̄₂₅₆ ≈ 67, ΔQ_b ≈ 9.5 and k ≈ 0.15, so the predicted shift is ≈ 0.3–0.7 k/s. At the registered row sd (≈ 1.1 k) and n_u ≈ 12 per arm, S ≈ 0.5–1.3, and S ≥ 2 would need about 30 unflagged rows per arm. The block still returns RISING (no S condition), the in-session 448/512 chord at n = 21, b̄ and the release batch per arm, and 63 rows of collapse statistics. It runs as registered.
- **The smoke margin is thin** (2.9 above 64). If the full launch's own unflagged smoke row reads ≤ 64, that is the registered ABORT-SMOKE with no relaunch, read as "practically inert". It is not loosened.
- What sets the release batch is a different question, for a new registration after the Result.

**§57b note (2026-11-14, after the rehearsal, before any battery row; no rule changes).**
- **Rehearsal** (main `02ecbdd7`, 15:40–15:57Z): suite-reuse HIT, tools tests green, SMOKE-PASS on both rows.
- **Amendment 1's T̄ resolution check on the P192 smoke row** (unflagged): per-leg LATE tick sd 0.98 / 0.93 ms, 6 / 6 distinct values (3–8 ms). That is above the 0.5 ms and 5-value thresholds, so the full launch goes ahead.
- **Seen data (two rows):**
  - P192: a/W 0.86, q_ℓ 2.05 ms, R 66.8 k/s, 627 Mbit/s.
  - P2048 (control): bind 0.19, a/W 0.66, q_ℓ 25.7 ms, 764 Mbit/s. One row reads the control's queue above the registered 13–21 ms band, reported as is.
- **Run conditions:** host cargo is held paused (the host build mutex is taken) for the whole battery, as §57a's Result committed.

## 57b. The c1d pool sweep and the transfer test T (reported only, no build) — pre-registration (before any VM contact)

**Bind check, from §53's 30 c1d 5 GB rows at the shipped law** (`c1dcap.py` → `c1dcap.out`):
- np = 2 on every row.
- The late cap has a median of 3 568 (row medians 3 206–4 096); its p10 is 2 052–4 096, median 2 850.
- The mean a/W is 0.42–0.69 on 29 rows (0.97 and 0.86 on the two collapse rows), i.e. ≈ 1 700–2 300 symbols of
  occupancy.
- So **any pool W ≤ 2 048 binds the cap** (the law's ask exceeds 2 052 on ≥ 90 % of ticks on every row).
- But the store fills only where W drops below the occupancy the receiver-bound pair holds, ≈ 1 700–2 300 symbols.
- **W 2 048 would be cap-bound but likely store-open**, the same regime as the control. It is dropped.
- **W 1 536 is the first store-bound candidate.** Each pool's mean a/W is reported, and T reads only pools at
  a/W ≥ 0.80 (R6.4 amendment).

**Arms** (c1d-400, n = 5 each, 30 rows):

| arm | pool (W) |
|---|---|
| **control** (shipped) | 2 048 (4 096) |
| | 768 (1 536) |
| | 512 (1 024) |
| | 384 (768) |
| | 256 (512) |
| | 192 (384) |

- **Order:** Latin-square rotation, seeds 42 / 7 alternating.
- **Smoke:** pools 192 and 2 048.

**Flags (never voiding; flagged rows enter no median).**
- **c1d collapse: both legs below BDP in > 0.10 of windows** (`c1dflag.py`). Its validation:
  - **§53's 5 GB rows (pre-law engine):** it flags 5 of 30, goodput median 729 against unflagged 827 (−12 %).
  - **§54's 24 guard rows (today's engine, 400 MB):** it flags 0 (`c1dflag54.out`).
  - **So validation on today's engine is in-session:** the flagged-against-unflagged goodput gap is a readout, and T
    is read on all rows and on unflagged rows (R6.4 amendment).
- **Minimum:** ≥ 3 unflagged rows per pool for any reading. P(< 3 of 5) ≈ 0.04 per pool at §53's 5/30 rate. A pool
  below the minimum is `UNREAD`.
- CPU/GB is reported per pool.

**Readouts** (per pool): goodput/control; q_ℓ and c1d's floor offset δ₀ (R6.4); R; a/W; the release batch and ack
wakes per loop; both-leg and one-leg flag shares; CPU/GB.

**Predictions:** R6.4 (a), (b) and (c), with the amendment's offset, range and flag rules.

**Length.**

| | rows | estimate |
|---|---|---|
| all rows | 30 | × 75 s ≈ 38 min |
| collapses, typical | ≈ 5 | + ≈ 4 min |
| collapses, worst (5 rows at 300 s) | ≈ 5 | + ≈ 19 min |
| plus tools tests, rehearsal and smoke | — | ≈ 30 min |

- Typical ≈ **1 h 15**; worst ≈ 1 h 30.
- `LD_HARD` = launch + 2.5 h.

---

**Decision rule (both launches).** None; reported only.
- §57a decides only whether F03's burst holds, or costs, any of c1s's queue.
- §57b decides whether the steep part of the receiver-bound relation transfers across path counts.
- Neither changes the time law. B-1 follows.

**The transfer test T (derivation R6.4).**

**Data already seen** (disclosed, so excluded from the evaluation):
- p34's static table, contaminated (R4.1);
- the s53 pin readouts (`c1dpin.out`);
- §53's and §54's c1d per-row `qcwnd` and goodput (`c1dflag.out`, `c1dflag54.out`).

None is a W sweep.

**Registered: T, the receiver-queue transfer.** On a receiver-bound cell goodput depends on the standing queue at the
receiver's batching. So c1d's goodput relative to its own control is the same function of the standing queue as
c1s's.

**One q reference for both sweeps.** Each sweep's smallest arm must not set its own zero: c1s's smallest arm is 384
per leg, c1d's is 192 per leg. Both therefore use **q_ℓ = T̄ − ℓ̂**:
- T̄ is the rate-weighted mean echo loop over LATE ticks, from per-tick means.
- ℓ̂ is each leg's lifetime echo minimum (the lowest `rtp_us` in the row's first 2 s), rate-weighted.

On c1s (`s56curve.out`, `c1srows.out`; ℓ̂ = 2.4–2.6 ms, so q_ℓ ≈ q + 1.5):

| c1s q_ℓ ms | 2.1 | 2.7 | 4.7 | 6.8 | 15.7 (control) |
|---|---|---|---|---|---|
| goodput / control | 0.79 | 0.89 | 0.94 | 0.96 | 1 |
| unflagged rows behind the point | 2 | 5 | 3 | 2 | 6 |

**The band is sized from the spread.** The two sources of noise:
- The c1s reference points rest on 2–5 unflagged rows. Their goodput/control spread is ±0.01–0.02 at 512–1 024; the
  2-row points and the control (whose 6 rows include one at 727) are looser.
- The c1d side is a per-pool median of ≥ 3 unflagged rows over a control median. The §53 c1d 5 GB unflagged row sd is
  ≈ 3.5 %, which makes the ratio's sd ≈ 0.03.

Combined, the sd is ≈ 0.04, so the band is **±0.08** (≈ 2 sd). That matches §54's measured c1d goodput resolution of
8 %.

**What the band can resolve.** c1s's relation is steep only at q_ℓ ≤ 2.7 ms (0.79 → 0.89 over 0.6 ms, about 0.17 per
ms). Above that it is flat (≈ 0.01 per ms), where ±0.08 spans ±8 ms of q and cannot discriminate. So:
- **(a) falsifying:** c1d goodput/control at q_ℓ = 2.1 and 2.7 ms equals 0.79 and 0.89, **±0.08 each**. It is
  interpolated on c1d's arm medians in q_ℓ; at least one c1d arm must fall in q_ℓ ≤ 3.0 ms.
- **(b) reported only:** at q_ℓ = 4.7 and 6.8 ms, 0.94 and 0.96. c1d's q at the 0.95 point is reported, not
  falsified: near 0.95 a ±0.03 uncertainty in the goodput ratio is ±3 ms of q.
- **(c) the c1d control's own queue:** q_ℓ = **13–21 ms** at W 4 096. This is predicted from the seen s53 pin rows
  (echo loop 18.5–21.5 ms against ℓ̂ ≈ 2.5). The c1d control is interior (s53 a/W 0.47–0.63), so this tests the
  control's regime, not T. A control queue below 10 ms means the sweep's reference is a different operating point,
  and is reported before (a) is read.

**What each outcome means, stated now:**
- **(a) passes:** the steep part of the receiver-bound relation is a receiver property. It transfers across path
  counts, and is measured once per receiver class.
- **(a) fires low** (c1d at least 0.08 below at the same q_ℓ): two connections need more standing queue for the same
  utilisation. The relation depends on the connection count.
- **(a) fires high:** c1d needs less queue; the receiver batches across two connections more efficiently.
- **W is not a falsifier.** W = R·(T₀ + q) by Little, and R and T₀ are each cell's own. "W differs, q passes" means
  c1d's rate or floor differs, not its queue mechanism.
- **Disclosed:** p34's contaminated c1d, re-read in q_ℓ from its window medians, sits inside (a)'s band at 384 and
  512 (q_ℓ ≈ 2.2 and 3.0; 0.76 and 0.87 against c1s's 0.80 and 0.90). It is not counted.

**Amendment (after the second review).** Three rules are added; the band is unchanged.

**1. Floor offset.** T's x-axis carries each cell's no-queue offset, δ₀ = T₀ − ℓ̂: the no-queue echo loop above the
lifetime echo minimum. At c1s it is 4.0 − 2.5 ≈ 1.5 ms, while the falsifying region is only 0.6 ms wide.
- **Measurement:** c1d's offset is its smallest store-bound pool's q_ℓ, which is predicted to be at or near no queue.
  Both offsets are reported.
- **"Offset differs" reading:** if |δ₀(c1d) − δ₀(c1s)| > 0.5 ms, (a) is also read on the offset-corrected axis
  q_ℓ − δ₀. If (a) passes on one axis and fails on the other, the reading is **OFFSET DIFFERS**: the cells' no-queue
  floors differ, and the queue relation is not tested.
- **No extrapolation:** an (a) point outside c1d's measured q_ℓ range (its arm medians' min to max) is **UNSCOREABLE**.

**2. Flagged rows.** T is read twice: on all rows, and on unflagged rows only. The c1d flag flagged nothing on today's
engine at guard size (R6.5). If the two readings disagree (one passes (a), the other fails), T is **UNSCOREABLE**.

**3. Store-bound pools.** T is read only on c1d pools whose mean a/W is ≥ 0.80 (see R6.5's bind check). A pool below
0.80 is not store-bound, and reads a different regime.

## 57a. The emission-burst block at c1s (reported only, no build) — pre-registration (before any VM contact)

**Context.** The store-law derivation is frozen at rev 6 (`docs/derivations/store-law.md`; the §56 Addendum and its erratum). §56 left c1s without a knee: goodput keeps rising with standing queue. The registration is derivation R6.5 and R6.4, copied, and was reviewed by Fable three times. Both knobs exist on main, so nothing is built. It runs as two launches.

**Why.**
- At c1s, more standing queue keeps buying goodput (R6.1). Two sender-side mechanisms could size that queue: the
  emission burst (F03's `RWM_EMIT_BURST`, pinned at 64 on c1s/c1d) and the ack-driven release batch.
- **A null burst result means only "not the emission burst"**, not "the receiver".
- §57b is the held-out c1d map for R6.4's T.
- No build: both knobs exist on main. Both launches run before the gauge build (B-1).
- **The block is split into two launches**, because the burst block alone has a worst case of ≈ 3.7 h (below).

**What §56 already shows about the release batch** (`c1srows.py` → `c1srows.out`, unflagged c1s rows):

| c1s W | 384 | 512 | 768 | 1 024 | 2 048 |
|---|---|---|---|---|---|
| release batch (symbols per ack wake) | 72–73 | 102–112 | 141–168 | 134–181 | 177–270 |
| release batch / W | 0.19 | 0.20–0.22 | 0.18–0.22 | 0.13–0.18 | 0.09–0.13 |
| ack wakes per echo loop | 4.7 | 3.7–4.3 | 3.6–4.4 | 4.4–6.5 | 6.3–10.8 |
| mean a/W | 0.85 | 0.85–0.86 | 0.85–0.87 | 0.86–0.88 | 0.87–0.92 |
| sender busy % / ack-wait % | 41–43 / 90–91 | 45 / 81–84 | 48 / 61–68 | 48–50 / 57–66 | 45–57 / 45–61 |

At W ≤ 768 the store is released in ≈ 4–5 batches per loop, each ≈ 0.2·W. This is registered as a readout in both
launches.

**Common to both launches.**
- **Tree:** current main, pinned by amendment.
- **Pool knob:** `RWM_STORE_PATH_POOL` as §56. The ceiling is max(N·pool, floor), boot min(128, store_max), and every
  W ≥ 384 > 128.
- **Burst knob:** `RWM_EMIT_BURST` (`gates.rs:194/363`, default 64, clamped [2, 512], echoed at `gates.rs:443`).
- **Scrub:** `-u RWM_STORE_PATH_POOL -u RWM_EMIT_BURST`.
- **Suites:** engine suites are skipped only on a byte-identical reuse-manifest hit against §56's tree. The tools
  tests and `LD_REHEARSE=1` always run.
- **Run settings:** Bulk, `--window-reliable`, `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300`, 5 GB.
- **Liveness:** `[GATES]` echoes the requested pool and burst on both ends. Otherwise the row is VOID and replaced
  once.
- **Bind:** a non-control row binds if its bind fraction is ≥ 0.95. Otherwise it is reported at its realised cap and
  not replaced.

---

**Arms** (c1s-400, n = 21 each, 63 rows):

| arm | pool (W) | `RWM_EMIT_BURST` | role |
|---|---|---|---|
| **B64-512** | 512 (512) | 64 (shipped) | control, re-based against §56's P512 |
| **B64-448** | 448 (448) | 64 | the in-session map: the R₆₄ chord from 448 to 512 |
| **B256-512** | 512 (512) | 256 | the test arm |

- The burst-16 arm is dropped.
- **Order:** Latin-square rotation over the three arms per replicate, seeds 42 / 7 alternating.
- **Smoke:** one B256-512 row. It must read a realised mean burst > 64 (Δ`eb_syms`/Δ`eb_bursts` over LATE);
  otherwise **ABORT-SMOKE**, because the knob does not act.

**Why n = 21.**
- Every arm needs ≥ 8 unflagged rows: the interpolation uses all three arms.
- §56 collapsed 4/9 rows at P512; pooled over c1s it was 9/27 = 1/3.
- With X ~ Binomial(n, 1 − f), P(X ≥ 8):

  | f | n 12 | 14 | 16 | 18 | 20 | **21** | 24 |
  |---|---|---|---|---|---|---|---|
  | 1/3 | 0.632 | 0.851 | 0.950 | 0.986 | 0.996 | **0.998** | 1.000 |
  | 4/9 | 0.318 | 0.563 | 0.759 | 0.882 | 0.948 | **0.966** | 0.992 |

- At n = 21, all three arms clear 8 with p = 0.966³ ≈ **0.90** at f = 4/9, and 0.998³ ≈ 0.99 at f = 1/3.
- At n = 16, the three arms would clear together with only 0.76³ ≈ 0.44 at f = 4/9.

**The hypothesis, read directly from the session.** Let b̄₆₄ and b̄₂₅₆ be the realised mean bursts at W 512. If the
emission burst holds ΔQ_b = (b̄₂₅₆ − b̄₆₄)/2 slots of store that buy no transit, then:

```text
   R_pred  =  R₆₄(512)  −  (ΔQ_b / 64) · ( R₆₄(512) − R₆₄(448) )
```

Burst 256 at W 512 then sits ΔQ_b/64 of the way from the 512 control down to the 448 arm. All three terms are
in-session, so neither §56's map nor any drift enters. The chord is linear between two in-session points; §56's
384–512 chord (75 /s per slot, its 384 end on 2 rows, and the map concave there) is no longer used.

**Scoring.** Every R is the per-arm median lap rate over unflagged rows. Each arm's SE = 1.253·sd/√n_u, from that
arm's own realised unflagged row count and sd.
- z_null = (R₂₅₆ − R₆₄(512)) / √(SE₂₅₆² + SE₅₁₂²).
- z_hyp = (R₂₅₆ − R_pred) / SE_pred, where SE_pred combines all three arms' SEs.
- Separation S = (ΔQ_b/64)·(R₆₄(512) − R₆₄(448)) / √(SE₂₅₆² + SE₅₁₂²): the predicted shift in SE units.

**Pre-registered expectation.**
- b̄₂₅₆ ≈ 90–105, because the release batch (≈ 105 per wake at W 512) caps the burst, against b̄₆₄ ≈ 48. So
  ΔQ_b ≈ 21–29 slots.
- The chord R₆₄(512) − R₆₄(448) is ≈ 2–4.8 k/s; 4.8 k is §56's upper bound, which concavity lowers.
- So the predicted shift is ≈ −0.7 to −2.2 k/s. At n_u ≈ 12 per arm (row sd ≈ 1.1 k) the separation is S ≈ 1.2–3.9.

**Outcome table** (every reading uses the realised b̄ and the in-session chord):

| reading | condition | what it means (stated now) |
|---|---|---|
| **UNDERPOWERED** | S < 2 | the burst's predicted shift is below resolution at this chord. E1/E2 are not read; the arms are reported |
| **E1: BURST HOLDS STORE** | S ≥ 2, z_null ≤ −2 and \|z_hyp\| < 2 | the emission burst holds wasted store at c1s, sized as predicted: F03's cost |
| **E2: NOT THE BURST** | S ≥ 2, \|z_null\| < 1 and z_hyp ≥ +2 | the predicted shift is absent; the receiver and the release batch remain |
| **RISING** | z_null ≥ +2 | a larger burst *raises* c1s's rate, through fewer wakes (CPU) or larger receiver batches. F03's cap of 64 then costs goodput rather than holding store; read with the CPU gate |
| **PARTIAL** | S ≥ 2, z_null ≤ −2 and z_hyp ≤ −2 or ≥ +2 | a burst effect exists but is not (b − 1)/2-sized; reported with its ratio to the prediction |
| **INCONCLUSIVE** | any other combination | reported |

How a realised burst outside the expected band is treated:
- **b̄₂₅₆ between 64 and 90:** scored with that b̄. The shift is smaller, and UNDERPOWERED is likelier.
- **b̄₂₅₆ above 105:** scored with that b̄. The release batch is then not the burst's bound, and that is reported.
- **b̄₂₅₆ ≤ 64:** the arm is INERT; the smoke already aborts on this.

**Gates on the reading (reported, never voiding).**
- **CPU on both test arms.** The client CPU/GB of B256-512 and of B64-448 must each be within 5 % of B64-512's.
  Otherwise that arm's reading is labelled **CPU-CONFOUNDED**. Burst 256 is not assumed clean: larger bursts can help
  CPU or receiver batching (RISING).
- **Collapse rate per arm.** It is the share of rows flagged by §56 Amendment 2's c1s rule, registered per arm. If
  B256-512's rate differs from B64-512's by more than 0.25 (absolute), the unflagged subsets are differently selected,
  and the reading is labelled **BIASED**.
- **Re-basing of B64-512 against §56's P512,** with the same lap-rate definition: §56's unflagged median is 82.7 k/s,
  range 80.3–83.3 k. It is re-based if the in-session median lies outside 82.7 k ± 3.0 k (the §56 range, floored at
  1 %). The scoring uses only in-session arms, so re-basing reports drift; it changes no reading.

**Readouts.**
- **Primary:** R per arm.
- **Secondary:**
  - Q = a − R·T₀, from the mean counted store and per-tick-mean rates, with T₀ fixed at §56's c1s 4.0 ms;
  - q from 0.1 ms per-tick means;
  - the realised mean burst;
  - the release batch and its share of W, ack wakes per loop, and mean a/W;
  - sender busy and ack-wait %;
  - goodput, client and server CPU/GB, and the collapse share per arm.
- **E5:** release batch / W at W 512 outside 0.17–0.24 in any arm. That means the release is not per-loop as §56 read
  it.

**Length.**

| | rows | estimate |
|---|---|---|
| unflagged | ≈ 35–42 | × 75 s ≈ 44–53 min |
| collapsed, typical | ≈ 21–28 | × ≈ 120 s (§56's collapsed rows ran 237–800 Mbit/s, ≈ 50–170 s) ≈ 42–56 min |
| plus tools tests, rehearsal and smoke | — | ≈ 30 min |

- Typical ≈ **2 h 10**.
- Worst with every collapsed row at §56's slowest (170 s): ≈ 2 h 45.
- Worst with every collapsed row at the 300 s timeout: ≈ 3 h 40.
- `LD_HARD` = launch + 4 h.

---

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

**§56 Result (REPORTED ONLY): current-era static maps. c2 and c8 have a wide flat region; c1s pays at every cut.** (2026-11-14; full run 08:59–10:25Z on main `5fdfca30`; rehearsal 2 passed at 08:45Z. Ledger: `docs/l1-raw/s56/rep56-*`. The host score `rep56-score.txt`, with the §54b re-basing, is the ledger's score; the VM's copy is `rep56-score-vm.txt`.)

**Integrity.**
- 70/70 slots LIVE: 0 void, 0 replaced, 0 DNF, 0 UNBOUND.
- The per-arm "void" counts on c1s are the 9 quinn-collapse-flagged rows. They are set aside, not voided.
- Every bound row has bind 1.000 with n_live = N on every LATE tick. `bin_sha` = §54's CAND.
- **Re-basing (host, against §54b B640): NO.** Every c8 control readout is within tolerance.
- Suite reuse hit; the bash and python tests are green.

**Maps** (arm medians; goodput relative to the cell's control):

| c8 W | 1280 (ctl) | 768 | 512 | 320 | 224 | 160 |
|---|---|---|---|---|---|---|
| goodput Mbit/s | 106.5 | 106.7 | 106.0 | 100.7 | 92.8 | 79.6 |
| r₁ /s | 1 853 | 1 831 | 1 718 | 1 113 | 405 | 188 |
| app-echo rtt₀ / rtt₁ ms | 96 / 150 | 57 / 92 | 39 / 61 | 26 / 47 | 20 / 46 | 16 / 46 |
| in-order late mean s | 0.37 | 0.16 | 0.09 | 0.05 | 0.04 | 0.03 |

| c2 W | 2048 (ctl) | 384 | 256 | 192 | 160 | 128 |
|---|---|---|---|---|---|---|
| goodput Mbit/s | 90.2 | 90.4 | 90.4 | 89.6 | 84.3 | 70.9 |
| app-echo rtt ms | 208 | 38 | 25 | 19 | 16 | 16 |
| in-order late mean s | 0.41 | 0.05 | 0.03 | 0.02 | 0.02 | 0.02 |

| c1s W | 2048 (ctl) | 1024 | 768 | 512 | 384 |
|---|---|---|---|---|---|
| unflagged n | 6 | 2 | 3 | 5 | 2 |
| goodput Mbit/s, median [range] | 875 [727–887] | 843 [832–854] | 819 [813–830] | 782 [750–785] | 689 [688–691] |
| / control | 1 | 0.96 | 0.94 | 0.89 | 0.79 |
| quinn backlog / W | 0.43 | 0.36 | 0.26 | 0.13 | 0.14 |

**Reading.**
- **c2:** flat down to W 192–256, at −0.7 % goodput or less.
  - The app-echo RTT falls 208 → 19–25 ms, and the in-order delay 0.41 → 0.02–0.03 s.
  - Below the knee, 32 slots cost 6 % and 64 cost 21 %.
  - Every c2 prediction is in band.
- **c8:** flat to W 512 (−0.5 %), with the in-order delay falling 0.37 → 0.09 s.
  - **The knee (0.95 of control) is at W ≈ 340, not the predicted 500–700.** The in-order bands miss on the low side.
  - The shoulder is leg 1 starving (r₁ 1 853 → 188), so F1 does not fire. This supports the derivation's leg coupling.
  - The c8 control is pool 640. The comparison with the shipped pool (W 4 096) comes from §54b, on the same 600 MB rows.
- **c1s:** no plateau (0.96 / 0.94 / 0.89 / 0.79) on small unflagged n. Within the derivation's band.
  - **The slope is not yet a store cost.** Quinn's send-buffer backlog is 0.13–0.43 of W on unflagged c1s rows, against ≤ 0.06 at reduced c2 pools and ≤ 0.02 at c8. Who limits per tick (the W1.1 attribution classes) is not yet computed.
  - One unflagged control row (726.7) sits next to a flagged one (725.9).
- **Quinn collapses in 9/27 c1s rows,** with no dependence on W (P2048 3/9, P512 4/9).
  - Post hoc: onsets are earlier at pools ≤ 512 (1–25 s, against 29–49 s).

**Falsifiers.**
- F1–F4 and F6–F8 do not fire.
- **F5 FIRES, but that is not "Little fails".**
  - W/ΣR − T̄ is 1.2–3.0 ms at c1s, 0.6–1.4 at c2 and 1.0–5.1 at c8.
  - It passes 10 % of W only where the loop is 4–19 ms (c1s). There T̄ is a median of integer-millisecond echo samples, while Little needs the mean.
  - Open: recompute per tick. A fixed residual that survives is store time outside the echo loop, and belongs in the law's reference.
- **F9 is UNSCOREABLE** (Amendment 2): the dump row has 190 028 records.
- **F10 is UNSCOREABLE, not passed.** At the 0.95 point c2's q (0.8 ms) is inside T₀'s spread (1.0 ms).
  - Post hoc, at the 0.98 points all three cells clear the spread and read q/ℓ = 0.24 (c2), 1.26 (c8) and 3.85 (c1s). That is a factor 16, so the knee queue does not scale with ℓ.
  - The allowance's form is therefore unsettled, and U2 (δ's band) is not askable yet.

**The derivation's static points on these maps:**

| cell | law's point | reads |
|---|---|---|
| c2 | 174–192 | 0.96–0.99 |
| c2 ("expired") | 140–164 | 0.84–0.94, on the cliff |
| c8 (coupled) | ≈ 550 | within 0.5 % of control |
| c8 (per-leg) | 148–212 | ≈ 0.75–0.85 |
| c1s | 370–509 | 0.79–0.89, as predicted |

The derivation's "c8 −3 to −5 %, fails the guard" is to be re-checked against this.

**Decision rule.** None; reported only.

**Next:**
- **Store-law derivation rev 3:**
  - swap in these maps;
  - find which measured quantity the knee queue scales with, since ℓ fails;
  - explain the c8 knee miss;
  - put the margin above the knee, sized by the estimator bound;
  - redo the F5 residual per tick.
- **c1s:** per-tick attribution of who limits. A closed-loop c1s test waits for the controller question (owner).
- **Not offered:** an interim smaller fixed pool. It would be a pin, not a law.

**§56 Addendum (post hoc, 2026-11-14; from `docs/derivations/store-law.md` §R3–§R5 on the §56 ledger; nothing
registered is re-scored).**
- **F5 is resolved as a parser artefact.** Recomputed per tick with means (`store-law/s56.py`), |W − Σrᵢ·rttᵢ| is
  ≤ 0.06·W on every arm, c1s included. The scored medians used integer-millisecond echo values. A small time
  outside the echo loop remains: 0.3–0.9 ms at c2/c8 for W ≤ 512, growing with W at c1s. It changes the store size,
  not the ratio law's fixed point.
- **c1s is store-bound.** One class rule across cells (store full iff a ≥ 0.95·W; quinn backlogged iff B ≥ 98;
  `store-law/s56cls.py`):
  - c1s's mean occupancy is 0.85–0.90 of W, with a 10–15 % ack-batch sawtooth that a per-tick threshold misses;
  - ticks with quinn's backlog carry the higher send rate, and the rate falls with W in every class;
  - so quinn's backlog is an effect of W, not a limiter. Checked on three rows by the coordinator: occupancy
    0.85–0.88, backlogged rate ≥ the other ticks.
  - **Hypothesis only, under review:** c1s's goodput curve may be utilisation–delay-like rather than a knee. The reported flat product q·(1 − u)·C ≈ 12.5–21 symbols at W 384–1 024 appears to mix two capacity definitions, and with a single one it may rise with W. It is not a finding.
- **The held-out c1d prediction missed** (`store-law.md` R4.1, committed `bcafa8c3` before the evaluation).
  - At the s53 pin, c1d's σ reads 1.1–1.3 ms and its quinn backlog dwell 0.9 ms, so the decomposition's inputs do
    not transfer.
  - On p34 (contaminated: seen in rev 2), the knee queue is ×2.7 the decomposition's.
  - One variant (U, R5.1, committed `441f5def`) is registered for a clean c1d sweep.
- **c8's ½ comes from symmetric shaping.** `topo_dual.sh` shapes both directions (5 + 5 ms, 20 + 20 ms), so the
  legs' forward arrivals equalise at a leg-0 queue of ½ the loop gap. That is ≈ 15 ms, a leg-0 loop of ≈ 30 ms, at
  W ≈ 320–340, where the knee is. Asymmetric cells c8a/c8b are its falsifier (Part B-2).
- **Withdrawn derivation claims:**
  - rev 2's "c8 −3 to −5 %, fails the guard" (p27's 8 s rows; on §56 the coupled point reads within 0.5 %);
  - rev 3's "c1s 2 048 → 768 is a substrate cost" and its quinn-backlog knee term;
  - rev 3's boot-latched σ̂⁺ (one start-up transient becomes a lifetime queue: one c2 row at 133 ms);
  - rev 3's close matches of a three-term knee model (fitted post hoc; it holds only at c2 and c8);
  - rev 4's two-variant c1d registration (replaced by one variant, U).

**Erratum to the §56 Addendum (2026-11-14, derivation rev 6).**
- **The utilisation–delay hypothesis fails.** With one capacity definition and the control arm included, q·(C − R) rises with W at c1s: 14 → 55 symbols at C = 96.8 k, and the control sits at u = 1 with a finite 14 ms queue. Rev 5's flat product mixed two rate definitions and left out the control.
- **Withdrawn:**
  - variant U (R5.1, "registered" in the Addendum);
  - the power-criterion formula R5.4, an oracle: it needs capacity, which is E04, and a burst constant with no engine source.
- R6.4 replaces them.
- **Open design item, not settled:** δ's authority in the standing law is q(δ)·ℓ̂ ≈ 1 ms, its one seat. "No owner question" in rev 6 does not mean settled.
- **Frozen.** The derivation is frozen until new data comes in: two models were fitted to four c1s points and withdrawn.

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
