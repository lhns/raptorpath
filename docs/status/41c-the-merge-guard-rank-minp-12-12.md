## 41c. The merge guard: rank-minp, 12/12 at four cells — pre-registration

Registered 2026-11-10, before any VM contact. It replaces §41b's guard
statistic for every guard run from now on: the merge-candidate session
(§46 item 9), the deletion batches and §48. §41b's statistic stays in the
scorer only to re-score history.

**Why.** Offline, on §41's and §41b's real rows (`guard47` 8721efb,
`docs/l1-raw/guard47/`), the max-z statistic is blind. With one clause fully
separated (CAND beyond every BASE row by ≥ 10 %), it read p up to 0.63, and
it reached 80 % power at no clause. Its saturating scale caps it near
p ≈ 0.036. So §41b's PASS was weaker than reported.

**Statistic (`place41b_score.py`, `report_guard`, `--method rank-minp`).**
- Clauses: 26 in all; c1s 6, c1d 7, c2 6, c8 7.
  - goodput;
  - client CPU/GB and server CPU/GB;
  - `lag_E_cli` and `lag_E_srv` (`[LAG] phase=xfer`, p99 − min);
  - the rtp floor per leg (minimum `rtp_us` in `[DIAG]`).
- Per clause, the worse-side rank sum of the CAND rows, using midranks
  within seed.
- Neutral and worst ranks:
  - an absent instrument value on a completed row takes the neutral mean
    rank. Fills are counted and printed per clause.
  - a **DNF row** (harness 43's COLLAPSE = DNF) takes the **worst rank on
    every clause** of its cell.
  - slow rows (goodput < 0.5 × the pooled, label-free cell median) are listed
    and reported only.
- Relabelling: one joint draw across all cells and clauses, stratified per
  seed with the true group sizes. N_PERM = 10 000, RNG seed 4141, and the
  true labelling counts as a member.
- Clause p = the share of labellings whose rank sum is ≥ the true one.
- Gate p = the share of labellings whose minimum clause p over all 26 is ≤
  the true minimum.
- **PASS iff p > 0.05.** One-sided: only the worse side fails.
- A cell short of its design (a voided row, or a row that never went LIVE)
  reads **NO VERDICT, re-run**, never PASS. The design is a required argument.

**Design.** BASE n = 12 and CAND n = 12 per cell, at c1s, c1d, c2 and c8
default.
- Seeds 42 and 7, 6 + 6 per seed (`BLOCKS41C`, `place38_parse.py`).
- One session, interleaved in 12 rep-major (seed, rep) groups, each holding
  a BASE/CAND pair per cell. The order alternates by group.
- 96 rows plus a 2-row smoke; the battery asserts 96.
- Duration: about 15 min, from §38's mean row times (c1s 6.3 s, c1d
  6.2 s, c2 11.2 s, c8 11.0 s) times 24 rows per cell, plus the smoke.
- No historical rows enter the null.

**Acceptance, met offline before registering**
- (a) One fully separated clause at any cell gives worst p = 0.0020 (130
  cases).
- (b) A/A false-alarm rate:
  - 4.0 % over 600 bootstrap datasets;
  - 5.6 % over 2 400 real splits, and 4.0 % over 4 000 fresh ones. The
    deciles are uniform.
- A stalled CAND build (all rows DNF) fails. It read p = 0.084 before the
  DNF rule.

**Resolution: the smallest shift caught in ≥ 80 % of draws**, with power at
+2.4 % CPU and −5 % goodput in brackets. c2 and c8 rest on stand-in rows
(§38 MAIN, 3 distinct per seed), so they are reported only. `score41c`
recomputes the table from this session's BASE rows (`session_resolution`:
resampled to the design and balanced per seed; 30 draws per point,
N_PERM 1000, 10 shift values) and prints it beside the verdict, for all four
cells, marked reported only.

The acceptance figures were computed at N_PERM 2000, the real-split A/A at
N_PERM 2000 and 500, and the real-split A/A on c1 cells only, before the DNF
rule.

| cell | goodput | client CPU/GB | server CPU/GB | rtp floor |
|---|---|---|---|---|
| c1s | 6 % (0.78) | 2.0 % (0.90) | 2.4 % (0.82) | 6 % |
| c1d | 10 % (0.30) | 10 % (0.35) | 5 % (0.25) | 5–6 % |
| c2 | 1 % (1.00) | 5 % (0.25) | 8 % (0.10) | 30 % |
| c8 | 10 % (0.40) | 6 % (0.35) | 6 % (0.13) | 10–15 % |

**Stated limits.**
- The two lag clauses are nearly blind: 30–100 % shifts go uncaught. A
  lag-only regression passes this guard.
- A PASS means "no regression above this table's resolution", nothing more.
- §41b's PASS re-scores under rank-minp at p = 0.692 (minimum clause p
  0.1235, c1s `lag_E_srv`, no DNF). §41's dry run re-scores at p = 0.140.

**Registered readout beside the guard: host ns per stamped symbol.** The
promotion adds a min-wire scan to every `[ETA]` stamp. A host bench reads
ns per stamped symbol, CAND against BASE, release profile, with the median
of 5 runs printed. It is reported only: it is the sensitive detector for
that CPU cost, and the VM guard is not.

**Amendment to §46 item 8 (CAND's identity).** CAND for the merge-candidate
session is `diag36`'s tip alone: the promotion, without simplification
batch 1. The batch gets its own guard run against the new main.

**Amendment to §46 item 9.** The guard in that session is §41c (this
section): rank-minp, 12/12 at c1s, c1d, c2 and c8, 96 rows, about 15 min.
It replaces "§41b's guard at c2 and c8". §46's own rows are unchanged.

**NO VERDICT.** A re-run is a fresh full guard session, and the voided
session's p is never used. If a second session again has CAND rows that DNF
or crash, and no BASE rows do, that is reported as a candidate defect,
`CAND-UNSTABLE`, and the candidate does not merge. A witness void alone is
not a candidate defect. BASE =
main `4926822`. Both SHAs are recorded by amendment before launch. With
batch 1 absent, the bisection clause of item 8 is void.


### 48. Result — the host gate: `STOP-NOT-REPRODUCED` as registered; on a finer simulator clock, quinn's collapse reproduces and the fix holds

Branch `bbrfix48-impl` (from main 4926822): `fbfea55` red tests, sim and
seams; `d455d7e` the fix; `15a16c5` the `[QCC]` gauge; `608c47b`, `1a7a29b`,
`7c65cb3` offline diagnostics. Evidence is in `docs/l1-raw/bbrfix48/` on that
branch. No VM contact.

**The registered outcome is `STOP-NOT-REPRODUCED`.** On the registered
simulator (50 µs poll grid), the mechanism is realised: quinn's `min_rtt`
latches at 150–950 µs on a 2.02 ms path (test 5 green). But test 4 is red:
quinn's window never falls below the BDP (share 0.0, stale and control
alike). So the battery does not run under §48.

**Deviations from the registration, all recorded.**
- **Stale-event phase.** At the registered phase (events on a 1 ms chunk
  edge), test 5 was red (quinn's minimum 1.05 ms). By the gate rule that is
  `VOID-SIM`, a harness fault, and the rule permits a harness fix with the
  L_j grid unchanged. The worker moved the events to 1.2005 s + 0.1·j, and
  the grid is unchanged. A phase sweep, run afterwards, checks that the
  choice does not matter post-fix (below).
- **Extra seams:** an `ack()` split of `on_ack`, `qcc()`/`qcc_of`,
  `pub(super)` QUIC config builders, a test-only `tracked_records()`, and an
  `Instant`-valued `AckEvent`. The `on_ack` split is neutral at `fbfea55`:
  `ack()` holds the identical body, `rtt.min()` is a pure getter read once,
  and the added `quinn_min_rtt` field is read only by the gauge. The fix
  then changes `ack()` on purpose.
- **Gauges** ride their own `[QCC]` line (amendment 2), not `[DIAG]`.
- **Jitter scenario.** The assertions were the worker's choice: b̂ ≥ 0.9·L
  before the first ProbeRtt and from exit + 1 s, over one ProbeRtt and 3 s
  after it. They held: b̂/L 1.03–1.07 outside ProbeRtt, a dip to 0.61 for
  0.21 s inside it. No jitter allowance is indicated.
- **Min filter.** The code uses `r_e ≤ m̂` (v1's `bbr_update_min_rtt`);
  §8.4's formula line says `<`. The two differ only on samples that tie m̂
  exactly. Under `<`, a tied recurring minimum would not restamp t_m. This
  is not a cure for the drain: the jitter run entered ProbeRtt at 10.0 s
  under `≤` too. A tie test (40 s of tied r_e = 10 ms samples) gives no
  ProbeRtt under the shipped `≤`, and entries at 12, 24 and 36 s for a model
  of `<`. It is reported only; the law is unchanged.

**Why test 4 was red: the simulator's clock, not quinn.**
- Quinn's bandwidth sampler takes min(send rate, ack rate) per `on_ack`
  call. Within one ACK frame, every call after the first sees a zero gap,
  so only the frame's first packet counts: 1 312 B over the gap between
  frame stamps. On a 50 µs grid that is 26.2 MB/s, against the 26.4 MB/s
  the filter read, which is 0.2 × the link.
- The delivered rate was normal (phase B 113.6 MB/s against a 112.5 MB/s
  supply). The ack-height term then grew to 253 MB, and the window with it.
- At a 10 µs grid, quinn's filter reads the link (130.9 MB/s), and the
  collapse reproduces (`7c65cb3`, run at HEAD 7c65cb3, dirty 0):
  - **stale:** share(W < BDP) = 0.806, min W 49 746 B, latched `min_rtt`
    180 µs on a 2.02 ms path;
  - **phase-B delivery:** 57.4 MB/s against 115.1 MB/s without stale stamps;
  - **no-stale control:** min W 307 633 B, share 0.0.

  The window equals 2 × filter × latched `min_rtt`. §47's inferred cause (a
  latched-low `min_rtt`) is therefore realised in simulation. It is **not
  refuted**.
- **Not yet reproduced:** the in-vivo row's pin at 13.8 KB with no further
  loss (§41b MAIN c1s s7 r3: qce 6→7 and lost +28 at 1.8 s, then qcwnd
  13.8–14.4 KB for > 5 s). Under W = 2 × filter × latch, 13.8 KB needs a
  55 µs latch at link rate, or about 1.28 ms at the delivered rate
  (≈ 5.4 MB/s). That is roughly half the ≈ 2.5 ms loop time, where the
  window-limited loop is neutral (2·m̂/R ≈ 1): a flat pin with no restoring
  force, as observed. This is inference from the law and the log.
  - In the sim the filter never decays: quinn only raises it, and phase-A
    bulk had set it at link rate.
  - A young-filter run (`6430eaa`) settles at W = 136 kB = 2 × 130.9 MB/s ×
    0.51 ms. It holds flat with no further loss, against 531 kB without stale
    stamps.
  - So the flat pin with no loss reproduces, but at a higher level.

**The fix at 10 µs** (`stale_phase_sweep_10us_grid`, post-fix, 5 phases
over 0–800 µs, at HEAD 7c65cb3): `bbr_rs` holds the pipe at every phase
(share 0.0, min W 307 518–307 577 B, minimum m̂ 2 020 µs). Quinn collapses
at every phase (share 0.75–0.85).

**Provenance.**
- Red at `fbfea55`: a fresh build, 7 passed, 7 failed, matching `red.txt`.
- Green at `15a16c5`: 14 passed, 0 failed, 1 ignored (the 50 µs quinn pin).
- Full debug lib suite at `15a16c5`: 611 passed, 0 failed, 10 ignored.
- Every run prints pwd, HEAD and dirty count.
- Five earlier logs ran a stale binary, because two worktrees shared a
  target dir. They are discarded.
- **Pre-fix phase sweep** (`ad41428` = `fbfea55` + `7c65cb3`'s sim, 626
  tests, dirty 0). Tests 1 and 2 are red at every phase:
  - 50 µs grid, 10 phases: share 0.69–0.85, min W 6.5–47 kB, m̂ 150–350 µs;
  - 10 µs grid, 5 phases: share 0.75–0.89, min W 21.6–57.9 kB.

  Post-fix they are green at every phase on both grids, so the result is
  attributed to the fix.
- Debug lib at `6430eaa`: 613 passed, 0 failed, 15 ignored.
- The release, `--doc` and wasm suites are still queued.
- Evidence is committed under `docs/l1-raw/bbrfix48/` on `bbrfix48-impl`
  (54e9a13, pushed). The invalid logs are not committed.
- **Not run:** the release, `--doc` and wasm suites.

**What this means.**
- The fix cures `bbr_rs`'s own defects. It holds the pipe in the scenario
  where quinn collapses, at every phase, and the unfixed controller
  collapses there too.
- Simulation does not reproduce the in-vivo pin. The VM is the only test of
  whether the fix cures that.
- Any VM test needs a new registration (§48b): the 10 µs grid fixed in
  advance; the pre-fix sweep run; the full suites; the §41c guard, settled
  against CONTROLLED-WITH-COST.

**§41c amendment 2 / §46 amendment: the trees for the merge-candidate session (status §47's plan, before VM contact).**
- **BASE = main `4926822`.**
- **CAND = `diag36` at `814b8ce`.** Its `raptorpath/src` equals `2d625fa`'s, which holds the promotion: the stamp is the priced E, and `RWM_PLACE_WIRE` is removed. The tree also carries the `guard47` scorer (`6dedf2b`). Later `diag36` commits are status-only, and the shipped tree is `814b8ce`.
- `LD_CAND_SHA=814b8ce…` (the full SHA is pinned in the launch log).
- **Session order:** suites on CAND, then the §41c guard (96 rows), then §46.
- **On the host after the session:** the resolution table (`resolution41c`), and the ns-per-stamped-symbol readout (`nsbench41c`, run on the host).

**§41c amendment 3 (before VM contact): prediction, decision rule, scorer wording.**
- **Prediction.** `GATE-41c PASS`. The likeliest clause to trip is c1s client CPU/GB. The promotion's scan read +0.8 % to +2.5 % there in §38, §41 and §41b, and the guard resolves about 2.0 % (§41c table). A FAIL on that clause is a live outcome.
- **Decision rule.**
  - **PASS** together with a §46 verdict other than `CAND-WORSE` or `UNSCOREABLE`: `diag36` merges to main (the user's go of 2026-11-11, given for this session).
  - **FAIL:** no merge. The user re-rules with the failing clause, its size, and the host ns bench beside it.
  - **§46 verdicts:**
    - `CAND-WORSE` blocks the merge (§46).
    - `UNSCOREABLE` defers the merge to a re-run.
    - `NOT-PROVOKED` and `NO-DIFFERENCE` go to the user labelled as such, and do not block a PASS merge.
    - `CAND-RECOVERS-FASTER` supports the merge.
  - **NO VERDICT:** re-run (amendment 1).
- **Scorer wording.** Since `6dedf2b`, `score41c` prints only the verdict block. The resolution table is the separate host command `place38_parse.py resolution41c <ledger> [--seed N]`, reported only. Its default seed is named in its header. The ns readout's BASE variant inlines main's stamp code (main has no `stamp_placement_eta`), so the ratio prices the wire-RTT lookup and the min-wire scan together.

**§41c amendment 4 (before VM contact): re-pinned CAND, and the replacement rule.**
- **CAND = `diag36` at `b6c6212d`.** It adds `guard47b`'s harness. Its `raptorpath/src` still equals `2d625fa`'s, and it supersedes the `814b8ce` pin.
- **All 28 `tools/l1` tests pass at `b6c6212d`.**
- **Replacement rule** (`guard47b`):
  - A slot voided by a witness is re-run once at the end of the battery, with the same cell, arm, seed and rep. At most 4 per session; beyond that, NO VERDICT.
  - Crash rows (`VOID-RC`, `NO_DATA`) and DNF rows are never replaced. They take the worst rank on every clause.
  - The void rate in §38, §41 and §41b was 2 of 252 rows. Both were WIRE rows on a WIRE-only witness, and none were BASE or MAIN rows.

**Session launched** 2026-11-11T13:28:24Z (VM UTC):
- `LD_PLAN=47`, `LD_CAND_SHA=b6c6212d0f0114c5faaf16caf816ea9372d8f667`, BASE `4926822a14af…`, `LD_HARD` = launch + 4 h.
- The precondition was met (procs_running [1,1,2,1,1], cargo 0, rustc 0), and both locks were taken.

**§41c readout: ns per stamped symbol** (`nsbench41c`, reported only).
- **Where.** Run on the VM (host memory was short): release build, `taskset -c 2`, 5 alternating BASE/CAND pairs, 2 000 000 calls per point.
- **Trees.** BASE = main `4926822` plus the test, which inlines main's stamp code. CAND = `2d625fa` plus the test, which calls `stamp_placement_eta` (branch `nsbench41c` 7a831e2).
- **Results** (ns per call, median with [min–max]):

| paths | BASE | CAND | CAND − BASE | ratio |
|---|---|---|---|---|
| 2 | 160.1 [154.6–187.1] | 210.3 [193.3–235.7] | +50 | 1.31 |
| 4 | 192.4 [184.1–243.8] | 231.3 [218.5–260.4] | +39 | 1.20 |

- The extra cost is the wire-RTT lookup plus the min-wire scan. At c1s's ≈ 90 k symbols/s, +50 ns is ≈ 4.5 ms of CPU per second, about 0.45 % of a core. That sits below the guard's ≈ 2 % c1s CPU resolution.
- The checksums differ by tree because the stamped values come from different laws.

**§41c session 1 result: `LD-ALL FAILED: ABORT-TESTS`** (14:28:18Z). The guard and §46 never ran, so there is no verdict.
- Suites: main rc 101 (1161 passed, 1 failed); debug-witness 1024 passed, 1 failed; doc ok; wasm 35 ok; parsers ok.
- The one failure, in both profiles: `allocations_per_source_symbol_stay_under_the_bound` (`tests/recv_alloc_bound.rs`), 25.03 and 24.99 against the bound 23.5 (main read 22.7–22.9 when the bound was set).
- **Cause.** `control_msg.rs` `wire_mirror_reads()` (from the `RWM_PLACE_WIRE` promotion) built two `Vec`s per ack batch, window ack and path report.
- **Fix** (`alloc47` 21836e9a): `Scheduler::mirror_wire_rtts(read)` walks the paths in place; both helpers are deleted. Red-first test `wire_mirror_step_allocates_nothing` (red at 2, green at 0); `stamp_placement_eta_allocates_nothing` reads 0 before and after.
- **VM check** (debug, 3 runs per tree): main passes 3/3; the old CAND fails 3/3 (25.13, 25.08, 25.11); the fix passes 3/3. `cargo check --all-targets` on the fix tree: rc 0.

**§41c amendment 5 (before VM contact): re-pinned CAND.**
- **CAND = `diag36` at `fb67a6c2f438b0bc4676301fe6daa581e683f940`** (`alloc47` merged). It supersedes the `b6c6212d` pin. Behaviour is unchanged apart from the allocations; the §41c ns readout (taken at `2d625fa`) stands as an upper bound.
- Everything else in §41c and amendments 1–4 is unchanged. Session 2 re-runs the whole plan 47, suites first.

**§41c session 2 result** (CAND `fb67a6c2`, launched 2026-11-11T16:30:22Z, ended 17:46:29Z): suites PASS, guard **PASS**, §46 `ABORT-INJECT` (no scored row ran).
- **Suites:** main rc 0 (1164 passed, 0 failed); doc ok; wasm 35 ok; debug-witness 1027 passed, 0 failed; parsers ok. `recv_alloc_bound` passes.
- **Guard (`GATE-41c[rank-minp]`): PASS, p = 0.2256.**
  - Design complete: 96/96 rows LIVE, no voids, 0 replaced slots, no DNF/crash/slow rows, no neutral fills.
  - Min-p (true) 0.0116 at c1d `rtp_floor_p0` (x = +4.8 %, inside §21's 5.7 % and below the BASE resolution q95 5.6 %). No other clause has p < 0.15.
  - Worse-side readings above 2 %: c1s `lag_E_srv` +6.3 % (p 0.21), c1d `rtp_floor_p1` +4.7 % (p 0.18); goodput is within ±2.1 % in every cell.
  - c1s client CPU/GB −0.2 % (p 0.58): the predicted live risk did not show. The resolution table marks c1s CPU blind below ≈ 2.8 % (q95 > §21 tol), as registered.
  - Tail: no row below 0.5 × cell median. The lowest CAND row is c1d s7 r2 at 0.797 × median (BASE's lowest 0.913).
  - QCWND-BELOW-BDP (reported): c1d CAND has rows with a leg's quinn cwnd under the BDP for up to the whole run (median 0), BASE at most 0.154. This is the §47 collapse signature; it is reported only.
- **§46: `ABORT-INJECT` at the check rows; a harness false fail, not an engine result.**
  - BASE/SLOW passed. CAND/FAST failed only on `qdisc: during[0] delay is not before + 1500 ms`.
  - Cause: `tc` prints netem delays with `%.3g` (iproute2 `sprint_time64`), so 5 + 1500 = 1505 ms reads back as `1.5s`; the checker's tolerance was 1 ms. The injection itself was right: injector `on.delay_ms` 1505 rc 0, and the engine saw it (`peak_wrtt` 1516 ms). BASE's 20 + 1500 prints exactly as `1.52s`, so the tests never hit this.
  - Fix (`place46_parse.py` `_tc_tol_ms`): tolerance = half a unit of the third significant digit, at least 1 ms (5 ms at 1505). Red-first test (5ms → 1.5s passes) plus a bound (5ms → 1.52s, 15 ms off, still fails). Dry-run on session 2's real captures: `SMOKE-PASS`.

**§46 amendment (before VM contact): the qdisc witness tolerance is tc's display precision** (above). Nothing else in §46 changes.

**§41c amendment 6 (before VM contact): session 3 runs §46 alone.**
- Session 2's suites and guard verdict stand; the guard is not re-run (its design completed and was scored; re-drawing it would raise which draw counts). No scored §46 row has run, so session 3 is not a second look at §46 data.
- **CAND = the commit that carries this amendment and the checker fix.** `raptorpath/src` is byte-identical to `fb67a6c2` (only `tools/l1/place46_parse.py`, `tools/l1/test_harness46.py` and this file differ), so session 2's suites and guard bind it.
- Session 3: `LD_PLAN=46`, a fresh build of both trees, no suites (`LD_P46_SUITES` unset), `LD_CAND_SHA` = the new pin. The decision rule is unchanged: guard PASS (session 2) plus a §46 verdict other than `CAND-WORSE`/`UNSCOREABLE` merges.

**§46 Result (session 3, CAND `dffd8517`, launched 2026-11-11T22:19:02Z, `LD-ALL-DONE` 22:47:21Z): `VERDICT NO-DIFFERENCE`.**
- Check rows: SLOW/BASE and FAST/CAND LIVE, qdisc ok (backlog peaks 362, 276); `SMOKE-PASS` under the amended tolerance. 22/22 battery rows ran; ABORT-TOKENS none.
- **SLOW-HIT** (4 vs 4 LIVE; the exit witness held on BASE 4/4, PROVOKED):
  - R: WITHIN, CAND median 3.85 s [1.97, 3.95, 4.74, 3.74] vs BASE 4.51 s [6.48, 4.27, 1.95, 4.75] (floor 1.00).
  - gbar_post: WITHIN, CAND 88.1 vs BASE 92.0 Mbit/s (floor 3.68).
  - Reported: G CAND 11.3 % vs BASE 9.4 %; R0 3.35 vs 4.21 s; T_w 3.50 vs 4.21 s.
- **FAST-HIT** (4 vs 4 LIVE; PROVOKED 4/4):
  - R: WITHIN, CAND 1.77 s vs BASE 1.75 s (floor 1.00).
  - gbar_post: WITHIN, CAND 82.9 vs BASE 86.0 Mbit/s (floor 3.44).
  - Reported: G CAND 5.1 % vs BASE −8.2 %; R0 0.48 vs 0.45 s; T_w 1.77 vs 1.75 s.
- **+500 ms at SLOW-HIT (reported only): no LIVE row.** All 4 rows are `WITNESS-FAIL event-did-not-bite`: the event witness asks for pre + 750 ms, but a +500 ms event can reach at most pre + 500 (peaks 545–590 against pre 204–290). The witness threshold is a fixed 750 ms that does not scale with the event's delta, a harness defect. It touches no scored row; to be fixed before these rows are read again.
- **c1d, leg 1 hit (reported only, n = 1 each):** R CAND 2.40 s vs BASE 6.22 s (BETTER, p(disjoint) 0.5, not scored); gbar_post 936.6 vs 876.4 Mbit/s.
- **QCWND-BELOW-BDP (reported):** c8 shares ≤ 0.024 in every arm; c1d BASE 0.215 vs CAND 0.024 (n = 1).
- **Decision rule (§41c):** guard PASS (session 2) with §46 `NO-DIFFERENCE` → `diag36` merges to main. NO-DIFFERENCE is reported to the user as such; it does not block.
- **Rule check.** SLOW ḡ_post's median gap (3.85 against the floor 3.68 Mbit/s) does not make WORSE: the registered rule (§46 item 5, `place46_parse.rule`) needs disjoint ranges *and* a gap beyond the floor, and the ranges overlap (CAND 84.3–91.9, BASE 78.4–106.2). The 4 void +500 ms rows are reported-only and do not enter UNSCOREABLE (which reads aborts, failed check rows, and < 3 LIVE rows in a *scored* variant). The witness constant is `EVENT_BITE_MS = 750` ("half of Δ = 1500").
- **Reading.** No measured gain: CAND's post-event goodput median is 3–4 Mbit/s lower in both variants, unresolved at n = 4. The c1d row (n = 1) is not evidence. The lock-out fix reaches main at no measured cost; the default path is not shown to be faster.

**§41c session 2 resolution readout** (`place38_parse.py resolution41c`, seed 0, reported only, a lower bound): smallest shift caught in ≥ 80 % of draws (d80): goodput c1s 5 %, c1d 8 %, c2 2 %, c8 8 %; CPU/GB 5 % at c1s and c1d, 8 % at c2 and c8 (power at +2.4 %: c1s client 0.40, server 0.27; elsewhere ≤ 0.20); `rtp_floor` 8–50 %; lag 20–100 %. The guard cannot see a CPU cost under ≈ 5 %; the host ns readout (+0.45 % of a core at c1s) is the evidence below that.

**Merge basis.** `diag36` merges on the guard of session 2 (CAND `fb67a6c2`) and §46 of session 3 (CAND `dffd8517`, engine bytes identical), under the unchanged §41c decision rule and the user's go of 2026-11-11. Ledgers: `docs/l1-raw/mergecand47/` (VM user and paths scrubbed).
- **Debt carried to main:** reconcile the `RWM_RSTAR_TAIL` default between the paper and status §1 (code: default ON, `fec_rate.rs:128`); the +500 ms witness to scale with Δ; `RWM_TEST_PLACE_HOLD` is already gone from `raptorpath/src`.

**Debt closed (2026-11-12, branch `fix49`).**
- `RWM_RSTAR_TAIL`: status §1's on-list now names it (code `gates.rs`, the paper §4.3 and ADR-0063 already said default on); §1 also records the ADR-0073 placement promotion. Closes §43's reconcile item.
- §46's event witness scales with the event: it bites at Δ/2 (`EVENT_BITE_FRAC = 0.5`), so 750 ms at the scored Δ = 1500 (unchanged) and 250 ms at +500. Session 3's four +500 ms rows (peaks +292 to +342 ms over pre) would read LIVE under it; they are not re-scored here. Tests: +500 at +300 bites, +500 at +200 does not, Δ = 1500 at +700 does not.
