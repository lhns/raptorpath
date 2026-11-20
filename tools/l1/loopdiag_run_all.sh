#!/bin/bash
# THE LOOP-TIME DIAGNOSTIC ENVELOPE (docs/status.md §23 follow-up). Derived from
# storelaw19_run_all.sh. ONE binary, built fresh from $LD_ROOT/src (a git
# archive with a COMMIT file at $LD_ROOT/COMMIT) into a fresh $LD_ROOT/target.
#
#   LD_ROOT=/home/vibe/n1/ld LD_SHM=/dev/shm/ld setsid nohup bash loopdiag_run_all.sh > $LD_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from $LD_ROOT/src/tools/l1 as `vibe`; `sudo -n` only for the
# measurement stages. WAITS for both operator locks (never breaks them), then
# holds them for the whole session: build -> smoke -> battery -> score.
#
# LD_PLAN=27 (default 24, unchanged): the DEF/BRS battery of status section 27
# (smoke27/score27/check27), then, AFTER the battery ledger is persisted, the
# test suites on the same on-disk source tree in a fresh $LD_ROOT/target-tests
# (TESTS.txt, TESTS-OK / TESTS-FAIL). A suite failure is reported, never voids
# the battery; DONE-ALL needs the battery ok; the final persist() runs last.
#
# LD_PLAN=29: the MAIN/PWR/KNEE battery of the power store law (smoke29/score29/
# check29). No suite stage (that is plan 27 only); the parser test still gates.
#
# LD_PLAN=34 (status section 34): the suites run BEFORE the smoke (a REAL failure is
# ABORT-TESTS: no row runs); the measurement launch precondition (procs_running <= 2 in
# 5 samples 2 s apart, no cargo/rustc) is checked before the locks are taken (exit 3,
# nothing run) and recorded again before the smoke; LD_HARD (epoch, required) bounds the
# battery: it is not started with less than 30 min left (ABORT-BUDGET), and its timeout
# ends at LD_HARD.
#
# LD_PLAN=36 (status section 36): the same precondition, LD_HARD and budget as plan 34, but NO suites (no engine
# change: main's binary, built fresh); the parser and scorer tests (test_loopdiag_parse.py, test_disp36.py) gate
# instead (ABORT-TESTS). The battery needs tcpdump and keeps the per-skb tables in $RUN/tables (persisted).
#
# LD_PLAN=46 (status section 46): the path-event test. Two trees like plan 41 (CAND at $LD_ROOT/src with its COMMIT file, BASE =
# main 4926822 at $LD_ROOT/base/src), built fresh. It REFUSES to launch (ABORT-CAND, nothing run) unless LD_CAND_SHA is set to the
# SHA recorded by amendment, is not the placeholder, and equals $LD_ROOT/COMMIT; and the CAND binary must not carry RWM_PLACE_WIRE.
# Suites only with LD_P46_SUITES=1 (the merge-candidate session runs them first). The smoke is the two CHECK ROWS (BASE at
# SLOW-HIT, CAND at FAST-HIT): a failed qdisc diff / backlog / event / exit witness or a non-zero inactive count is ABORT-INJECT
# and no scored row runs. The ledger score reads smoke.log and ld.log.
#
# LD_PLAN=41c (status 47): the status-41b guard on the merge candidate: BASE (main 4926822, n = 12) against CAND ($LD_ROOT/src at
# LD_CAND_SHA, n = 12) at c1s, c1d, c2 and c8 default, no WIRE arm (96 rows + a 2-row smoke); scored by place41b_score.py (rank-minp,
# the design mandatory: an incomplete cell reads "NO VERDICT, re-run").
# LD_PLAN=47 (status 47, "merge candidate"): ONE session on the candidate tree: the suites on CAND's tree first (release, doc, wasm,
# debug, every tools/l1 python test; the FLAKE rule; a REAL failure is ABORT-TESTS and nothing else runs), then the guard (plan 41c),
# then the path-event test (plan 46: its check rows are the smoke, ABORT-INJECT stops it). Each has its own ledger subdir under the
# run dir: suites/, guard/, p46/ (score.txt in each). A failed guard verdict does not stop plan 46; a stage that cannot run
# (smoke, budget, battery rc) stops the session.
#
# LD_PLAN=48 (status 48b, the bbr_rs battery): plan 41c's design (96 rows + a 2-row smoke) at section 48's sizes with TWO fresh trees: CAND at $LD_ROOT/src
# (COMMIT == LD_CAND_SHA else ABORT-CAND) run with RWM_QUIC_CC=bbr_rs, BASE = main at launch at $LD_ROOT/base/src (COMMIT == LD_BASE_SHA, set at launch). Order: parser
# gates, both builds, the suites on CAND's tree (a REAL failure is ABORT-TESTS: no row runs), the smoke (one CAND row then one BASE row at c8, unscored; ABORT-SMOKE),
# the battery, the plan-41c replacement rule (at most 4 witness-voided slots re-run once), score48 (VERDICT48 <label>). Ledgers under $RUN/bbr48/.
#
# LD_PLAN=52 (status 52, the c8 recovery levers): the suites on CAND's tree first (as plan 47/48: a REAL failure is ABORT-TESTS and nothing runs; LD_PRIOR_TESTS and LD_REHEARSE
# are supported), then the guard (plan 41c, unchanged and scored) in $RUN/guard, then a REPORTED-ONLY block in its own ledger $RUN/rep52 (LD_PLAN=52: c8-100 at 600 MB, arms BASE,
# CAND, W0 / W05 / WDER = BASE's binary with RWM_PLACE_WDIV=0 / 0.5 / RWM_PLACE_WDIV_DERIVED=1; 30 rows + a 3-row smoke; tail52_score.py, never a verdict). BASE is pinned by
# LD_BASE_SHA (required: main at launch must hold the RWM_PLACE_WDIV echo), CAND by LD_CAND_SHA. The reported block cannot change the guard's verdict (separate ledger and score);
# DONE-ALL needs both stages to run to the end. Rehearsal: both smokes, no battery.
#
# LD_PLAN=41d (status 41d, the merge guard v2): plan 48's shape (LD_CAND_SHA and LD_BASE_SHA required; suites on CAND's tree; LD_PRIOR_TESTS and LD_REHEARSE supported) running ONE
# scored stage, guard/ = plan 41c's 96 rows in plan 41c's order with the clause set v2: c2 / c8 rows at 600 MB with RWM_PERF_TIMEOUT_S=200 (c1s / c1d keep the guard size and 150),
# a smoke of plan 41c's two rows + one 600 MB c8 CAND row, a c2 / c8 row with under 30 s of [DIAG] ticks VOID-SHORT (re-run once by the replacement rule), score41d (GATE-41d).
# LD_GUARD_SET=v2 with LD_PLAN=47 or 52 swaps those sessions' guard stage (plan 41c, clause set v1, the default) for plan 41d's stage; any other plan refuses it.
#
# LD_PLAN=54 (status 54, the snapshot-order law's guard) is plan 41d, unchanged, with LD_WITNESS54=1: three row-level liveness witnesses (bin_sha per arm; at c8 the CAND p0 [HOLD] hn_n < 2000;
# [SNAPV] pt_st + tt_st = 0 on every CAND row), applied like the v2 VOID rule (never to a DNF row); smoke54 prints the rehearsal criteria, score54 = score41d's output + the witness table.
# LD_PLAN=64 (status 64, F2's merge guard: docs/derivations/f2-guard-registration.md; plan64.py, rt64_score.py, framing64.py) is plan 41d's guard (stage G, Bulk, LD_WITNESS64=1: the F2 row witnesses on every CAND row, the
# F2-FAIL status, score64 = score41d's output + the F2 witness table + GATE-64) followed by stage R (LD_PLAN=64 in the battery: Realtime c2-100 at rho = 1, arms RB / RC, 12 + 12 rows, $RUN/rt64). Reuse: the engine suites by a
# reference-manifest HIT over the WIDER src_manifest() set (LD_REF_MANIFEST, default tools/l1/ref64-manifest.sha256; a MISS is ABORT-SUITES before any row, a rehearsal included; LD_PRIOR_TESTS is refused); the binaries
# by LD_BIN_PREBUILT_BASE / LD_BIN_PREBUILT_CAND (each sha-checked against LD_BIN_SHA_BASE / LD_BIN_SHA_CAND: ABORT-BUILD); the tools tests by LD_PRIOR_TOOLS=<the rehearsal's run dir> when tools/l1 is byte-identical
# (TOOLS-MANIFEST.sha256 beside TOOLS-TESTS-OK; prints TOOLS-TESTS-REUSED). A rehearsal (LD_REHEARSE=1) adds the framing_ns bench (10 runs on CAND's tree, bench/framing_ns.txt, the BENCH label). LD_HARD = launch + 2.5 h, rehearsal + 1.25 h.
# LD_PLAN=65 (status 65, the exact replication of the Realtime in-order delay lean: docs/derivations/s65-registration.md; the registered scorer tools/l1/s65_score.py is wired in UNCHANGED) is a STAGE-R-ONLY launch: plan 64's stage R
# (c2-100, rho = 1, 100 MB, 150 s) at 24 pairs (48 rows) on the two prebuilt binaries of status 64 (LD_BIN_PREBUILT_BASE / _CAND, LD_BIN_SHA_BASE / _CAND both required, sha-checked before any row: ABORT-BUILD). No build, no cargo, no
# suites (SUITES-NOT-RUN); the tools tests are reused through LD_PRIOR_TOOLS (the rehearsal's run dir) on a byte-identical tools/l1, else run. The scorer's sha256 must equal LD_S65_SCORE_SHA (default: the registered pin; ABORT-TOOLS, nothing
# run). The process list is logged before the rehearsal / launch and again before the first row. Budget guard: plan64.py budget 65 (launch, 6 700 s) / 65r (rehearsal, 1 800 s); LD_HARD = launch + 2.5 h, rehearsal + 1 h.
# The smoke is RB s42 r1 and RC s42 r1 plus a conditional RB s7 r1 (only if RB s42 r1 is BASE-F2-DEAD); two dead RB rows are a NO-GO for the coordinator. $RUN/score.txt carries VERDICT65, TEST65 and F2-FINDING65.
# LD_PLAN=54b (status 54, the store-budget block, REPORTED ONLY): the suites on CAND's tree (levb54), then ONE stage in $RUN/rep54b: c8-100 at 600 MB, arms B2048 / B1024 / B640 on the BASE
# seat (LD_BASE_SHA = cand54) and L2048 / L1024 / L640 on the CAND seat, RWM_STORE_PATH_POOL = default / 1024 / 640, n = 6, rotated per group, 36 rows + a 6-row smoke (pool54_score.py).
# LD_PLAN=56 (status 56, the store-pool sweep, REPORTED ONLY, no build): ONE seat -- current main (LD_CAND_SHA = $LD_ROOT/COMMIT) runs every row; there is no BASE tree, no second build, no LD_BASE_SHA (ignored with a
# note). Pre-flight: >= 1 GB free on /dev/shm (the RWM_SUCC_DUMP row). Suites: the engine suites are skipped only if a per-file sha256 manifest of raptorpath/src + raptorpath/tests equals tools/l1/ref56-manifest.sha256
# (git at cand54 7fd92dd5; regenerate with ref_manifest.sh; LD_REF_MANIFEST overrides), else the full suites run as plan 54 did; the tools tests (python and bash) always run; LD_PRIOR_TESTS is refused. Then ONE stage
# in $RUN/rep56: smoke = 4 rows (c8 P80, c2 P128, c1s P384, the c8 P640D dump row), then 70 slots (c8 6 pools x 4, c2 6 x 3, c1s 2048 / 512 x 9 and 1024 / 768 / 384 x 3, the dump row), RWM_PERF_TIMEOUT_S=300;
# a VOID pool echo and a dump under 100 000 records are re-run once (pool56_score.py; LD_REHEARSE=1 and LD_HARD as plan 54b).
# LD_PLAN=57a / 57b (status 57a / 57b, the emission-burst block at c1s and the c1d pool sweep, REPORTED ONLY, no build): plan 56's single-seat session (LD_CAND_SHA = $LD_ROOT/COMMIT, no BASE
# tree, no LD_BASE_SHA, >= 1 GB free on /dev/shm, the same reference-manifest suite reuse and tools tests). ONE stage in $RUN/rep57a (smoke = the B256-512 row, which must read a realised mean
# burst > 64, then 63 slots: c1s-400 at 5 GB, B64-512 / B64-448 / B256-512 = RWM_STORE_PATH_POOL x RWM_EMIT_BURST, n = 21) or $RUN/rep57b (smoke = pools 192 and 2048, then 30 slots: c1d-400 at
# 5 GB, six pools, n = 5); RWM_PERF_TIMEOUT_S=300; a VOID pool / burst echo is re-run once (pool57_score.py). LD_HARD = launch + 4 h (57a) / launch + 2.5 h (57b); the budget guard refuses with
# less than the worst case left (13 200 s / 5 400 s; LD_HARD_MIN_S is a test seam).
# LD_PLAN=59 (status 59, the evicting-seat m-bar readout, REPORTED ONLY, no build): plan 57b's single-seat shape (LD_CAND_SHA = $LD_ROOT/COMMIT, no BASE tree, no LD_BASE_SHA, >= 1 GB free on /dev/shm, the
# reference-manifest suite reuse, the tools tests). ONE stage in $RUN/rep59: smoke = ONE evicting-seat row (c2-100 at 600 MB, --protocol-hint realtime, NO --window-reliable, the shipped-default unified machine (RWM_UNIFIED defaults on: the arm sets nothing for it); the
# sender must echo the shed law armed), then 5 slots (seeds 42 / 7 / 42 / 7 / 42); RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 (a DNF is expected, not a failure). A slot whose liveness witnesses
# fail is VOID and re-run once (mbar59_score.py / place38_parse.replace59). The budget guard refuses with under 7 200 s to LD_HARD (worst case: 5 + 5 replaced rows at the 150 s timeout, ~40 s bring-up
# each, plus the tools tests, a reference-manifest MISS -- the engine suites, 44-54 min -- and the build; LD_HARD_MIN_S is a test seam). Launch with LD_HARD = launch + 2.5 h.
# LD_PLAN=60a / 60b (status 60, the store law's gauge session: docs/derivations/s60-registration.md, tables in plan60.py, readouts in s60_score.py, REPORTED + SCORED by the registration's labels): plan 59's single-seat shape (LD_CAND_SHA = $LD_ROOT/COMMIT,
# no BASE tree, no LD_BASE_SHA, >= 1 GB free on /dev/shm, the tools tests). Differences: (1) LD_BIN_SHA_EXPECT (default: the registered bin_sha e410d640..., 60-pre's crate-directory build) must equal the built binary's sha256 (ABORT-BIN-SHA,
# before any row; LD_BIN_PREBUILT=<path> installs a prior launch's binary instead of building it, still checked); (2) the engine suites are reused ONLY by a reference-manifest HIT (LD_REF_MANIFEST, default tools/l1/ref60-manifest.sha256):
# a MISS is ABORT-SUITES (nothing run), a rehearsal included; (3) ONE stage rep60a / rep60b: smoke = 2 rows (60a: c2 W192 C then G; 60b: c1s W64 G 1 GB then W2048 C; no evicting row since status 60 Amendment 2), then 96 (60a) / 40 (60b) slots; LD_REHEARSE=1 on 60a runs the 10 rehearsal
# rows and the go/no-go (LD_NOGO=<c2,c8,c1s> carries the decisions into 60b's table and the scorer); a slot with a VOID RWM_SLAW / pool / liveness echo, or a floor-pool DNF, is re-run once (plan60.py replace; a wrong binary never is). The budget guard refuses with under
# 18 360 s (60a: 3.2 h + 36 floor re-runs) / 13 860 s (60b: 2.9 h + 18) to LD_HARD, so the launch sets LD_HARD = launch + 5.2 h (60a) / + 3.9 h (60b) (LD_HARD_MIN_S is a test seam).
# LD_PLAN=53 (status 53, stale gap reports under lever B): plan 52's envelope without the guard. The suites on CAND's tree first (a REAL failure is ABORT-TESTS and nothing runs; LD_PRIOR_TESTS and
# LD_REHEARSE are supported), then ONE REPORTED-ONLY stage in $RUN/rep53 (LD_PLAN=53: c8-100 at 600 MB with BASE / CAND / CANDS / BASES / CW0 n = 6, then c1d-400 at 5 GB with BASE 12 / CAND 12 / CANDS 6;
# 60 rows + an 8-row smoke; snap53_score.py, never a verdict). BASE is pinned by LD_BASE_SHA (required), CAND by LD_CAND_SHA; CANDS / BASES carry RWM_RECOV_SNAPVER=1, CW0 RWM_PLACE_WDIV=0. The smoke
# prints the registration's rehearsal criteria as PASS / FAIL lines (smoke-check.txt); any FAIL is ABORT-SMOKE. Rehearsal: the smoke only, no battery.
#
# LD_PLAN=41b (status section 41b): the same two-tree build as plan 41 and NO suites (raptorpath/src is unchanged from section 41's
# tested tree); the battery is 48 rows (BASE 12, MAIN 6, WIRE 6 per cell, c1s and c1d default) scored by place41b_score.py.
#
# LD_PLAN=41 (status section 41): plan 38's battery plus the arm BASE, main's own binary. TWO fresh builds, each from its own
# `git archive` tree with its own fresh target dir: $LD_ROOT/src (the shipped tree, COMMIT at $LD_ROOT/COMMIT) and
# $LD_ROOT/base/src (main at 4926822, COMMIT at $LD_ROOT/base/COMMIT, target $LD_ROOT/target-base). Order: parser gate,
# both builds, the suites on the shipped tree (a REAL failure is ABORT-TESTS: no row runs; the tools/l1 python tests are all
# test_*.py), the smoke, the battery from root-owned tmpfs. The BASE binary is installed as $LD_SHM/bin-base/raptorpath (same
# file name, so pgrep -x / pkill -x raptorpath see it).
# LD_PLAN=43b (status 43b): plan 43's code and binary (sha checked) with the SL-T / SH-T / MAIN arms at c2; refuses while G43B / TAU43B_US /
# BAND43B are unset. Otherwise the same envelope as plan 43.
#
# LD_PLAN=43 (status section 43 + amendments 1-2): the oracle store law in closed loop. run43's binary (diagnostic arm, never merged),
# built fresh. The plan-36 launch precondition, LD_HARD and root-owned tmpfs tree, but the budget guard is 45 min (ABORT-BUDGET) and
# no engine suites run here (the host gate (a)-(g) did); test_place43_parse.py gates (ABORT-TESTS). Launch through
# loopdiag_wait34.sh with LDW_PLAN=43 (the on-VM waiter), only after status 41b.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh loopdiag_run_all.sh loopdiag_battery.sh loopdiag_wait34.sh loopdiag_parse.py anchordiag_parse.py disp36.py test_disp36.py disp40.py test_disp40.py place38_parse.py test_place38_parse.py place41b_score.py bbr48_score.py test_bbr48_parse.py tail52_score.py test_tail52.py snap53_score.py test_snap53.py pool54_score.py test_pool54.py pool56_score.py test_pool56.py pool57_score.py test_pool57.py mbar59_score.py test_mbar59.py plan60.py s60_score.py test_s60.py test_harness60.py plan61.py s61_score.py test_s61.py test_harness61.py plan62.py s62_score.py test_s62.py test_harness62.py ref_manifest.sh ref56-manifest.sha256 place43_parse.py test_place43_parse.py place46_parse.py event_inject.py test_harness46.py test_harness47.py test_guard_v2.py test_topo_netem.sh \ plan51e.py s51e_score.py plan51.py s51_score.py test_s51e.py test_s51.py test_harness51.py plan63.py test_harness63.py \
    test_loopdiag_parse.py test_anchordiag_parse.py stage3_parse.py storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py plan64.py rt64_score.py framing64.py s65_score.py test_s65_score.py
crlf_guard plan67.py s67_score.py s67_fixtures.py test_s67.py test_harness67.py tun67_battery.sh tun_bulk.sh loopdiag_p67.sh
ROOT="${LD_ROOT:?LD_ROOT}"
PLAN="${LD_PLAN:-24}"
# Plan 54 (status 54, the snapshot-order law's guard) IS plan 41d, unchanged, plus three row-level liveness witnesses (pool54_score.guard_witness): from here on PLAN is 41d and
# W54=1 rides in SENV, so no 41d branch can be missed. The scored rows, order, sizes, timeouts and verdict are plan 41d's; score54 prints score41d's output first.
W54=0; [ "$PLAN" = "54" ] && { W54=1; PLAN=41d; }
# Plan 64 (status 64, F2's merge guard) is plan 41d too, with LD_WITNESS64=1 (the F2 row witnesses on every CAND row, score64) and a second stage rt64 (stage R) after it: W64=1 rides in SENV.
W64=0; [ "$PLAN" = "64" ] && { W64=1; PLAN=41d; }
P65=0; [ "$PLAN" = "65" ] && P65=1      # status 65: a stage-R-only launch (no alias: the battery sees LD_PLAN=65)
# Plan 67 (status 67, VM batch 1; loopdiag_p67.sh is the session envelope, sourced below): 67 = the rehearsal (LD_REHEARSE=1), 67g = guards 1 and 2 (Bulk), 67x = the stages off Bulk and the tunnel. P46 here only for the shared machinery
# (the root-owned tmpfs tree, LD_HARD, the launch precondition); the seats have their own trees and pins (no LD_CAND_SHA / COMMIT, no LD_BASE_SHA).
P67=0; case "$PLAN" in 67|67g|67x) P67=1 ;; esac
case "$PLAN" in
  24) P_SMOKE=smoke; P_SCORE=score; P_CHECK=check ;;
  27) P_SMOKE=smoke27; P_SCORE=score27; P_CHECK=check27 ;;
  29) P_SMOKE=smoke29; P_SCORE=score29; P_CHECK=check29 ;;
  34) P_SMOKE=smoke34; P_SCORE=score34; P_CHECK=check34 ;;
  36) P_SMOKE=smoke36; P_SCORE=score36; P_CHECK=check36 ;;
  40) P_SMOKE=smoke40; P_SCORE=score40; P_CHECK=check40 ;;
  38) P_SMOKE=smoke38; P_SCORE=score38; P_CHECK=check38 ;;
  41) P_SMOKE=smoke41; P_SCORE=score41; P_CHECK=check41 ;;
  41b) P_SMOKE=smoke41b; P_SCORE=score41b; P_CHECK=check41b ;;
  43) P_SMOKE=smoke43; P_SCORE=score43; P_CHECK=check43 ;;
  43b) P_SMOKE=smoke43b; P_SCORE=score43b; P_CHECK=check43b ;;
  46) P_SMOKE=smoke46; P_SCORE=score46; P_CHECK=check46 ;;
  41c) P_SMOKE=smoke41c; P_SCORE=score41c; P_CHECK=check41c ;;
  47) P_SMOKE=smoke46; P_SCORE=score46; P_CHECK=check46 ;;        # unused: the stages below pick their own parsers
  48) P_SMOKE=smoke48; P_SCORE=score48; P_CHECK=check48 ;;        # unused: the stage below picks them
  52) P_SMOKE=smoke52; P_SCORE=score52; P_CHECK=check52 ;;        # unused: the stages below pick their own parsers
  41d) P_SMOKE=smoke41d; P_SCORE=score41d; P_CHECK=check41d ;;    # unused: the stage below picks them
  53) P_SMOKE=smoke53; P_SCORE=score53; P_CHECK=check53 ;;        # unused: the stage below picks its own parsers
  54b) P_SMOKE=smoke54b; P_SCORE=score54b; P_CHECK=check54b ;;     # unused: the stage below picks its own parsers
  56) P_SMOKE=smoke56; P_SCORE=score56; P_CHECK=check56 ;;         # unused: the stage below picks its own parsers
  57a) P_SMOKE=smoke57a; P_SCORE=score57a; P_CHECK=check57a ;;     # unused: the stage below picks its own parsers
  57b) P_SMOKE=smoke57b; P_SCORE=score57b; P_CHECK=check57b ;;     # unused: the stage below picks its own parsers
  59) P_SMOKE=smoke59; P_SCORE=score59; P_CHECK=check59 ;;         # unused: the stage below picks its own parsers
  60a) P_SMOKE=smoke60; P_SCORE=score60; P_CHECK=check60 ;;        # unused: the stage below calls s60_score.py / plan60.py directly
  60b) P_SMOKE=smoke60; P_SCORE=score60; P_CHECK=check60 ;;        # unused: as 60a
  65) P_SMOKE=smoke65r; P_SCORE=score65r; P_CHECK=check65r ;;      # unused: the stage below picks them
  67|67g|67x) P_SMOKE=smoke67; P_SCORE=score67; P_CHECK=check67 ;;      # unused: loopdiag_p67.sh runs its own stages (s67_score.py / plan67.py)
  61a) P_SMOKE=smoke61; P_SCORE=score61; P_CHECK=check61 ;;        # unused: the stage below calls s61_score.py / plan61.py directly
  61b) P_SMOKE=smoke61; P_SCORE=score61; P_CHECK=check61 ;;        # unused: as 61a
  62r0|62a) P_SMOKE=smoke62; P_SCORE=score62; P_CHECK=check62 ;;    # unused: the stage below calls s62_score.py / plan62.py directly
  51) P_SMOKE=smoke51; P_SCORE=score51; P_CHECK=check51 ;;         # unused: the stage below calls s51_score.py / plan51.py directly
  63) P_SMOKE=smoke63; P_SCORE=score63; P_CHECK=check63 ;;        # unused: the stage below calls plan63.py directly
  51e) P_SMOKE=smoke51e; P_SCORE=score51e; P_CHECK=check51e ;;     # unused: the stage below calls s51e_score.py / plan51e.py directly
  *) echo "ABORT-PLAN LD_PLAN=$PLAN (67|67g|67x|24|27|29|34|36|38|40|41|41b|41c|41d|43|43b|46|47|48|51|51e|52|53|54|54b|56|57a|57b|59|60a|60b|61a|61b|62r0|62a|63|64|65)"; exit 3 ;;
esac
# Test seams (test_stubrun.py runs this script end to end against stubs); on the VM none is set and the defaults are the real paths.
DEVSHM="${LD_DEVSHM:-/dev/shm}"; PROCFS="${LD_PROCFS:-/proc}"; LOCK_VM="${LD_LOCK_VM:-/tmp/rwm-vm.lock}"; LOCK_RP="${LD_LOCK_RP:-/home/vibe/rp.lock}"
# LD_REHEARSE=1 (plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56, 57a, 57b, 59; status 48b amendment 1): the launch path up to and including each smoke and its check (gates, locks, builds,
# binary pins, smoke rows), then stop: REHEARSAL-PASS / LD-ALL-REHEARSAL-DONE. No suites (unless LD_PRIOR_TESTS names a run to verify) and no battery.
REHEARSE="${LD_REHEARSE:-0}"
[ "$PLAN" = "62r0" ] && REHEARSE=1      # status 62: the rehearsal IS plan 62r0 (its rows are its smoke; no battery follows it); plan 62a has no rehearsal mode
if [ "$REHEARSE" = 1 ] && [ "$PLAN" != "51" ] && [ "$PLAN" != "51e" ] && [ "$PLAN" != "63" ]; then      # plans 51 / 51e / 63 are rehearsable too (their own rules below)
  case "$PLAN" in 62r0|61a) ;; 41c|41d|46|47|48|52|53|54b|56|57a|57b|59|60a|60b|65|67) ;; *) echo "ABORT-REHEARSE LD_REHEARSE=1 is defined for plans 41c|41d|46|47|48|52|53|54|54b|56|57a|57b|59|60a|60b|61a|62r0|64|65|67 only, not $PLAN; NOTHING WAS RUN"; exit 3 ;; esac
fi
# The guard's clause-set version (status 41d). Plan 41d IS the v2 guard (stage `guard 41d`: c2 / c8 rows at 600 MB, timeout 200, score41d). The session plans 47 and 52 run
# their guard stage as plan 41c (clause set v1) unless LD_GUARD_SET=v2, which swaps it for plan 41d's stage. Every other plan keeps v1 and refuses the variable.
GUARD_PLAN=41c
case "${LD_GUARD_SET:-v1}" in
  v1) ;;
  v2) case "$PLAN" in 47|52) GUARD_PLAN=41d ;; *) echo "ABORT-GUARD-SET LD_GUARD_SET=v2 is defined for plans 47|52 only (plan 41d is the v2 guard itself), not $PLAN; NOTHING WAS RUN"; exit 3 ;; esac ;;
  *) echo "ABORT-GUARD-SET LD_GUARD_SET=${LD_GUARD_SET} (v1|v2); NOTHING WAS RUN"; exit 3 ;;
esac
# Plans 43 and 43b share the oracle code path (P43=1). Plan 43b refuses to launch while its plan variables are unset (nothing run).
P43=0; { [ "$PLAN" = "43" ] || [ "$PLAN" = "43b" ]; } && P43=1
if [ "$PLAN" = "43b" ]; then
  python3 ./place43_parse.py vars43b > /dev/null || { echo "ABORT-PLAN43B-UNSET G43B / TAU43B_US / BAND43B are unset: NOTHING WAS RUN"; exit 3; }
fi
# Plans 38, 41 and 41b share everything but the BASE arm (P38=1 is "the place-wire battery"; P41=1 builds and runs the BASE binary).
P38=0; { [ "$PLAN" = "38" ] || [ "$PLAN" = "41" ] || [ "$PLAN" = "41b" ] || [ "$PLAN" = "41c" ]; } && P38=1
P41=0; { [ "$PLAN" = "41" ] || [ "$PLAN" = "41b" ] || [ "$PLAN" = "41c" ] || [ "$PLAN" = "46" ] || [ "$PLAN" = "47" ]; } && P41=1
# P46=1: a candidate plan (46, 41c, 47): the shipped tree is the merge candidate, pinned by LD_CAND_SHA, and must hold the promotion.
P46=0; { [ "$PLAN" = "46" ] || [ "$PLAN" = "41c" ] || [ "$PLAN" = "47" ]; } && P46=1
# Plan 48 is a candidate plan too (CAND pinned by LD_CAND_SHA) and builds BASE; its BASE is pinned by LD_BASE_SHA (main at launch).
P48=0; [ "$PLAN" = "48" ] && { P48=1; P46=1; P41=1; }
# Plan 52 is a candidate plan too (CAND pinned by LD_CAND_SHA); its BASE is pinned by LD_BASE_SHA like plan 48's and must carry the RWM_PLACE_WDIV echo.
P52=0; [ "$PLAN" = "52" ] && { P52=1; P46=1; P41=1; }
# Plan 41d (the merge guard v2) is a candidate plan like 48: CAND pinned by LD_CAND_SHA, BASE (main at launch) by LD_BASE_SHA, suites, then the one guard stage.
P41D=0; [ "$PLAN" = "41d" ] && { P41D=1; P46=1; P41=1; }
# Plan 53 is a candidate plan too (CAND pinned by LD_CAND_SHA); its BASE is pinned by LD_BASE_SHA and must carry the RWM_PLACE_WDIV and RWM_RECOV_SNAPVER echoes (the battery's preflight checks both binaries).
P53=0; [ "$PLAN" = "53" ] && { P53=1; P46=1; P41=1; }
# Plan 54b (status 54, the store-budget block, REPORTED ONLY): plan 53's envelope without the guard and without the dup-diag arms; CAND = levb54 (LD_CAND_SHA), BASE = cand54 (LD_BASE_SHA).
P54B=0; [ "$PLAN" = "54b" ] && { P54B=1; P46=1; P41=1; }
# Plan 56 (status 56, the store-pool sweep, REPORTED ONLY, no build): ONE seat. A candidate plan (P46: current main is pinned by LD_CAND_SHA and $LD_ROOT/COMMIT, LD_HARD, the
# precondition, the root-owned tmpfs tree) but NOT P41: no BASE tree, no second build, no LD_BASE_SHA. Current main is both pin and candidate and there is no A/B, so a second seat would
# only duplicate the build (and the harness refuses two identical binaries); the sweep varies RWM_STORE_PATH_POOL on the one binary.
P56=0; [ "$PLAN" = "56" ] && { P56=1; P46=1; }
# Plans 57a / 57b (status 57a / 57b, REPORTED ONLY, no build): plan 56's single-seat shape (P46: pin, LD_HARD, precondition, the root-owned tmpfs tree; NOT P41: no BASE tree, no LD_BASE_SHA).
P57A=0; [ "$PLAN" = "57a" ] && { P57A=1; P46=1; }
P57B=0; [ "$PLAN" = "57b" ] && { P57B=1; P46=1; }
P57=0; { [ "$P57A" = 1 ] || [ "$P57B" = 1 ]; } && P57=1
# Plan 59 (status 59, REPORTED ONLY, no build) is plan 57b's single-seat shape, so it rides the P57 gates (budget guard, /dev/shm pre-flight, suites by the reference manifest, tools tests, one reported stage).
P59=0; [ "$PLAN" = "59" ] && { P59=1; P46=1; P57=1; }
# Plans 60a / 60b (status 60) ride plan 59's single-seat gates (budget guard, /dev/shm pre-flight, suites by the reference manifest, tools tests, one stage) with the differences listed in the header.
P60=0; { [ "$PLAN" = "60a" ] || [ "$PLAN" = "60b" ]; } && { P60=1; P46=1; P57=1; }
# Plans 61a / 61b (status 61, the c3 sweep) ride plan 60's gates (P60: the registered bin_sha, the strict reference-manifest rule, LD_BIN_PREBUILT; P57: budget guard, /dev/shm pre-flight, one stage) with their own tables, scorer and
# abort classes (ABORT-PILOT, NOGO-CONTROL); the R-sign gate is plan 60b's alone.
P61=0; { [ "$PLAN" = "61a" ] || [ "$PLAN" = "61b" ]; } && { P61=1; P60=1; P46=1; P57=1; }
# Plans 62r0 / 62a (status 62, c3 at jitter 0) ride the same gates (P60: the registered bin_sha e410d640, the strict reference-manifest rule; P57: budget guard, /dev/shm pre-flight, one stage) with their own tables, scorer and
# abort classes (ABORT-PILOT, ABORT-CONTROL62, ABORT-REHEARSAL62, ABORT-NETEM), NO build (LD_BIN_PREBUILT is required, its sha checked) and a tools-test reuse of the rehearsal's result (tools_reuse_ok below).
P62=0; { [ "$PLAN" = "62r0" ] || [ "$PLAN" = "62a" ]; } && { P62=1; P60=1; P46=1; P57=1; }
if [ "$P60" = 1 ]; then
  # The registered values of status 60 (60-pre PASSED): the crate-directory build of s60-tree cb88df3a, and tools/l1/ref60-manifest.sha256 (main 204cbc81). Both can be overridden (the stub suite does).
  LD_BIN_SHA_EXPECT="${LD_BIN_SHA_EXPECT:-e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d}"
  LD_REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref60-manifest.sha256}"
  [ -s "$LD_REF_MANIFEST" ] || { echo "ABORT-SUITES no reference manifest at $LD_REF_MANIFEST (plans 60a / 60b); NOTHING WAS RUN"; exit 3; }
  # The R-sign gate (s60 section 5, Amendment 1): R~ < 1 at the c2 / c8 controls is read at the END of 60a on the full calibration; 60b does not launch (nothing run) unless 60a's score (LD_SCORE60A = its
  # rep60a/score.txt) carries LABEL60 R-SIGN-c2 and R-SIGN-c8 as PASS or UNSCOREABLE (a STOP or a missing label aborts; 60b's readings do not use c2 / c8's R). A rehearsal has no 60a score and is exempt.
  if [ "$PLAN" = "60b" ] && [ "$REHEARSE" != "1" ]; then
    [ -s "${LD_SCORE60A:-}" ] || { echo "ABORT-RSIGN plan 60b needs 60a's score (LD_SCORE60A=<rep60a/score.txt>) to read the R-sign gate; NOTHING WAS RUN"; exit 3; }
    for _c in c2 c8; do
      grep -aqE "^LABEL60 R-SIGN-$_c (PASS|UNSCOREABLE)( |$)" "$LD_SCORE60A" || { echo "ABORT-RSIGN 60a's score has no 'LABEL60 R-SIGN-$_c PASS|UNSCOREABLE' ($(grep -a "^LABEL60 R-SIGN-$_c" "$LD_SCORE60A" | head -1)): the gate failed (STOP) or was not read; 60b does not launch; NOTHING WAS RUN"; exit 3; }
      echo "R-SIGN $_c: $(grep -a "^LABEL60 R-SIGN-$_c" "$LD_SCORE60A" | head -1) (60a's score; UNSCOREABLE does not block 60b)"
    done
  fi
fi
if [ "$W64" = 1 ]; then
  # The registered reference manifest of plan 64: src_manifest() of ab9feec4's archive (the tree whose suites ran on the VM); LD_REF_MANIFEST overrides it (the stub suite does).
  LD_REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref64-manifest.sha256}"
  [ -s "$LD_REF_MANIFEST" ] || { echo "ABORT-SUITES no reference manifest at $LD_REF_MANIFEST (plan 64); NOTHING WAS RUN"; exit 3; }
fi
if [ "$P67" = 1 ]; then P46=1; fi
if [ "$P65" = 1 ]; then
  # Plan 65 (status 65): P46 here only for the shared machinery (the root-owned tmpfs tree, LD_HARD, the launch precondition); it has no CAND source tree, no LD_CAND_SHA / COMMIT, no LD_BASE_SHA. Before anything runs:
  # the registered scorer by sha (ABORT-TOOLS) and the two prebuilt binaries with their pins (ABORT-BUILD); the shas of the binaries themselves are checked after the install, still before any row.
  P46=1
  S65_PIN="${LD_S65_SCORE_SHA:-41d9d58e27f42eb92862e243c25900d547ea7ffd6b64a6d00ddcb039fa844a21}"
  S65_GOT="$(sha256sum "$HERE/s65_score.py" 2>/dev/null | cut -d' ' -f1)"
  [ "$S65_GOT" = "$S65_PIN" ] || { echo "ABORT-TOOLS plan 65: tools/l1/s65_score.py sha256 ${S65_GOT:-<missing>} != the registered pin $S65_PIN (LD_S65_SCORE_SHA); NOTHING WAS RUN"; exit 3; }
  for _v in LD_BIN_PREBUILT_BASE LD_BIN_PREBUILT_CAND; do
    [ -n "${!_v:-}" ] && [ -f "${!_v}" ] && [ -x "${!_v}" ] || { echo "ABORT-BUILD plan 65 needs $_v (an executable file: the pinned binary from status 64's bins64); NOTHING WAS RUN"; exit 3; }
  done
  for _v in LD_BIN_SHA_BASE LD_BIN_SHA_CAND; do
    [ -n "${!_v:-}" ] || { echo "ABORT-BUILD plan 65 needs $_v (the registered sha256 of the binary); NOTHING WAS RUN"; exit 3; }
  done
fi
# Plan 51e (status 51 block E, docs/derivations/s51-evict-design.md; tables in plan51e.py, readouts in s51e_score.py): plan 60's single-seat shape with the STRICT suite rule (the engine suites ran once, in E-pre, on s51e-tree; a
# reference-manifest MISS is ABORT-SUITES, a rehearsal included) and a REGISTERED bin_sha that has NO default here (E-pre's result is not known until E-pre has run): LD_BIN_SHA_EXPECT and tools/l1/ref51e-manifest.sha256
# (E-pre's manifest, committed with the E-pre amendment; LD_REF_MANIFEST overrides) are required, and a missing one stops the launch before any lock is taken. LD_ONLY=PL (with LD_REHEARSE=1) is the PL-only recheck row.
# Plan 51 (block 51): main at launch, NO engine change and NO engine suites (nothing merges): LD_BASE_SHA (main at launch) is the commit pin (it is also the candidate pin of the shared single-seat gate; a different
# LD_CAND_SHA is refused); LD_CELL = c2-100 | c8-100, one cell per launch; LD_BIN_SHA_EXPECT is RECORDED by the rehearsal and REQUIRED to equal the built binary on a battery launch; LD_PRIOR_TESTS / LD_REF_MANIFEST are refused;
# the dial pins run on the built tree (ABORT-DIALPINS) before any row.
P51E=0; [ "$PLAN" = "51e" ] && { P51E=1; P46=1; P57=1; }
P51=0; [ "$PLAN" = "51" ] && { P51=1; P46=1; P57=1; }
# Plan 63 (status 63, F2's first VM contact; plan63.py, reported only): TWO seats on the single-seat gate shape (P46 + P57). CONTROL = the e-tree binary, pinned to the registered E bin_sha (LD_BIN_SHA_CONTROL, default
# 8013a312..., a test seam); CAND = $ROOT's tree, whose binary must equal LD_BIN_SHA_CAND (REQUIRED on a battery launch, no default; the rehearsal RECORDS it). CONTROL comes from LD_BIN_CONTROL (an executable, no build)
# or is built from $ROOT/base/src. A prebuilt CAND is refused (the CAND is a plain release build from the launch archive). No engine suites run and a manifest MISS is no abort (the CAND tree cannot hit ref51e); the tools tests do. LD_HARD = launch + 1.5 h.
P63=0; [ "$PLAN" = "63" ] && { P63=1; P46=1; P57=1; }
if [ "$P63" = 1 ]; then
  LD_BIN_SHA_CONTROL="${LD_BIN_SHA_CONTROL:-8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf}"; export LD_BIN_SHA_CONTROL LD_BIN_SHA_CAND      # the report (plan63.py report) reads both pins
  [ -z "${LD_BIN_PREBUILT:-}" ] || { echo "ABORT-BUILD plan 63 never accepts a prebuilt CAND (LD_BIN_PREBUILT is set): the CAND is a plain release build from the launch archive; NOTHING WAS RUN"; exit 3; }
  [ "$REHEARSE" = 1 ] || [ -n "${LD_BIN_SHA_CAND:-}" ] || { echo "ABORT-BIN-SHA plan 63 needs LD_BIN_SHA_CAND (the candidate's bin_sha, recorded by the rehearsal); NOTHING WAS RUN"; exit 3; }
fi
if [ "$P51E" = 1 ]; then
  LD_REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref51e-manifest.sha256}"
  [ -s "$LD_REF_MANIFEST" ] || { echo "ABORT-SUITES no reference manifest at $LD_REF_MANIFEST (plan 51e: E-pre's manifest); NOTHING WAS RUN"; exit 3; }
  [ -n "${LD_BIN_SHA_EXPECT:-}" ] || { echo "ABORT-BIN-SHA plan 51e needs LD_BIN_SHA_EXPECT (E-pre's registered bin_sha); NOTHING WAS RUN"; exit 3; }
fi
if [ "$P51" = 1 ]; then
  [ -n "${LD_BASE_SHA:-}" ] || { echo "ABORT-BASE plan 51 needs LD_BASE_SHA (main at launch, recorded by amendment); NOTHING WAS RUN"; exit 3; }
  [ -z "${LD_CAND_SHA:-}" ] || [ "$LD_CAND_SHA" = "$LD_BASE_SHA" ] || { echo "ABORT-CAND plan 51 is single-seat on main at launch: LD_CAND_SHA=$LD_CAND_SHA differs from LD_BASE_SHA=$LD_BASE_SHA; NOTHING WAS RUN"; exit 3; }
  LD_CAND_SHA="$LD_BASE_SHA"
  { [ -z "${LD_PRIOR_TESTS:-}" ] && [ -z "${LD_REF_MANIFEST:-}" ]; } || { echo "ABORT-TESTS plan 51 runs no engine suites (main at launch, nothing merges): LD_PRIOR_TESTS / LD_REF_MANIFEST are refused; NOTHING WAS RUN"; exit 3; }
  case "${LD_CELL:-c2-100}" in c2-100|c8-100) ;; *) echo "ABORT-CELL plan 51: LD_CELL=${LD_CELL} (want c2-100 or c8-100); NOTHING WAS RUN"; exit 3 ;; esac
  [ "$REHEARSE" = 1 ] || [ -n "${LD_CELL:-}" ] || { echo "ABORT-CELL plan 51 needs LD_CELL=c2-100|c8-100 (one cell per launch); NOTHING WAS RUN"; exit 3; }
  [ "$REHEARSE" = 1 ] || [ -n "${LD_BIN_SHA_EXPECT:-}" ] || { echo "ABORT-BIN-SHA plan 51 needs LD_BIN_SHA_EXPECT (the bin_sha the rehearsal recorded); NOTHING WAS RUN"; exit 3; }
fi
# Plan 46 shares the root-owned tmpfs tree, LD_HARD and the two-tree build with plans 38 / 41 (P38 stays 0: no place-wire arms or ladder).
if [ "$P46" = 1 ] && [ "$P65" != 1 ] && [ "$P67" != 1 ]; then
  CAND_SHA="${LD_CAND_SHA:-UNSET}"
  case "$CAND_SHA" in UNSET|"") echo "ABORT-CAND LD_CAND_SHA is unset (the CAND SHA is recorded by amendment before launch); NOTHING WAS RUN"; exit 3 ;; esac
  [ "$(cat "$LD_ROOT/COMMIT" 2>/dev/null)" = "$CAND_SHA" ] || { echo "ABORT-CAND $LD_ROOT/COMMIT is not LD_CAND_SHA=$CAND_SHA; NOTHING WAS RUN"; exit 3; }
fi
if [ "$P41D" = 1 ] || [ "$P48" = 1 ] || [ "$P52" = 1 ] || [ "$P53" = 1 ] || [ "$P54B" = 1 ]; then
  : "${LD_BASE_SHA:?LD_BASE_SHA (main at launch, recorded by amendment) is required for plans 48, 52, 53, 54, 54b and 41d}"
fi
# Measurement runs from tmpfs (shared rules): the run dir and the measured
# binary live under LD_SHM; the build stays on disk; persist() copies the run
# dir to $ROOT/run after the last row (and on every exit path).
SHM="${LD_SHM:?LD_SHM (a /dev/shm dir)}"
case "$SHM" in "$DEVSHM"/*) ;; *) echo "ABORT-SHM $SHM is not under $DEVSHM"; exit 3 ;; esac
RUN="$SHM/run"
SRC="$ROOT/src/raptorpath"
TARGET="$ROOT/target"
BIN="$SHM/bin/raptorpath"
BASE_SRC="$ROOT/base/src/raptorpath"
BASE_TARGET="$ROOT/target-base"
BIN_BASE="$SHM/bin-base/raptorpath"
BUILDLOG="$ROOT/build.log"
# Plan 36: systemd-logind RemoveIPC (default yes) deletes a non-system user's /dev/shm objects when the user's last
# session ends -- a binary copied there vanished between ssh sessions. So the tmpfs tree is ROOT-owned (sticky
# world-writable dirs, so this script can still write its sentinels) and the binary is installed as root.
if [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  sudo -n mkdir -p "$SHM" "$RUN" "$SHM/bin" "$SHM/bin-base" && sudo -n chmod 1777 "$SHM" "$RUN" "$SHM/bin" "$SHM/bin-base" || { echo "ABORT-SHM cannot create the root-owned tmpfs tree"; exit 3; }
else
  mkdir -p "$RUN" "$SHM/bin"
fi
persist() { mkdir -p "$ROOT/run" && cp -a "$RUN/." "$ROOT/run/" && echo "PERSISTED $RUN -> $ROOT/run $(date -u +%FT%TZ)"; }
SENTINELS="DONE-ALL FAILED-ALL all-era.txt TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS REHEARSAL-PASS ABORT-SMOKE BINSHA.txt smoke.log smoke-check.txt ld.log score.txt parser-test.log"
probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
LB_PROOF_EXTRA="launch=$(date -u +%FT%TZ)" prove_sentinels "$RUN" $SENTINELS
sudo -n true 2>/dev/null || { echo "ABORT-SUDO"; exit 3; }
for f in $SENTINELS; do rm -f "$RUN/$f"; done
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "LD-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; persist; exit 5; }
LB_TAG="loopdiag_run_all:$$"
LB_LOG="$RUN/all-era.txt"
era "LD-ALL start plan=$PLAN load=$(cat "$PROCFS"/loadavg)"
install_lock_traps
# Wait read-only (test -e only) until both locks are free, then take them with
# lib_battery.sh take_lock (noclobber + flock on a <> fd). A lost race aborts
# with ABORT-LOCK and nothing run; relaunch.
until ! test -e "$LOCK_VM" && ! test -e "$LOCK_RP"; do sleep 30; done
prec_samples() { local i p s="" ok=1; for i in 1 2 3 4 5; do p=$(awk '/^procs_running/{print $2}' "$PROCFS"/stat); s="$s${p:-?},"; [ -n "$p" ] && [ "$p" -le 2 ] || ok=0; [ "$i" -lt 5 ] && sleep 2; done; echo "$ok procs_running=[$s] cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc)"; }
# Plan 65: the process list, recorded in the era log (and kept in $RUN/PROCLIST-<phase>.txt) before the rehearsal / launch and again before the first row (status 64 recorded only the first, and only counts).
proclist65() { # phase
  local f="$RUN/PROCLIST-$1.txt"
  { date -u +%FT%TZ; ps -ef 2>/dev/null || ps 2>/dev/null || true; } > "$f" 2>/dev/null
  era "PROCLIST65 $1: $(wc -l < "$f") lines in $f; cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) python=$(pgrep -c python3 2>/dev/null || echo 0) raptorpath=$(pgrep -xc raptorpath) load=$(cut -d' ' -f1-3 "$PROCFS"/loadavg)"
}
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  : "${LD_HARD:?LD_HARD (epoch) is required for plan $PLAN}"
  PREC=$(prec_samples)
  if [ "${PREC%% *}" != 1 ] || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
    era "ABORT-PRECONDITION ${PREC#* }; NOTHING WAS RUN"; exit 3
  fi
  era "LD-ALL precondition met ${PREC#* } io=$(grep '^full' "$PROCFS"/pressure/io)"
  [ "$P65" != 1 ] || proclist65 "before-$([ "$REHEARSE" = 1 ] && echo rehearsal || echo launch)"
fi
if [ "$P56" = 1 ]; then
  # Plan 56 (70 slots) is launched with LD_HARD = launch + 4 h: a reference-manifest hit needs about 2 h 05 typical and 3 h 20 worst case (every non-control c1s row at its 300 s timeout); a miss adds the
  # 44-54 min engine suites (about 2 h 50 typical, 4 h 10 in the double worst case). Under 3 h left cannot hold even a typical miss session, so it is refused before anything runs (LD_HARD_MIN_S is a test seam).
  P56_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan 56 budget: ${P56_LEFT}s to LD_HARD (launch + 4 h expected; at least ${LD_HARD_MIN_S:-10800}s required)"
  [ "$P56_LEFT" -ge "${LD_HARD_MIN_S:-10800}" ] || { era "ABORT-BUDGET plan 56: ${P56_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-10800}s; NOTHING WAS RUN"; exit 3; }
fi
if [ "$P57" = 1 ]; then
  # Plans 57a / 57b are launched with LD_HARD = launch + 4 h (57a, worst case 3 h 40 with every collapsed row at the 300 s timeout) or launch + 2.5 h (57b, worst case 1 h 30). The guard refuses with
  # less than the WORST CASE of the status left (13 200 s / 5 400 s), before anything runs (LD_HARD_MIN_S is a test seam). A reference-manifest MISS adds the 44-54 min engine suites, which the
  # status's worst cases do not include; that is a gap of the pre-registration, not of this guard.
  P57_NEED=13200; [ "$P57B" = 1 ] && P57_NEED=5400
  P57_TAG="57a/57b ($PLAN)"; [ "$P59" = 1 ] && { P57_NEED=7200; P57_TAG="59"; }
  [ "$P61" = 1 ] && { P57_NEED=14220; [ "$PLAN" = "61b" ] && P57_NEED=6000; P57_TAG="$PLAN"; }      # status 61 section 8 (plan61.BUDGET_S): 3.95 h / 1.67 h worst case, floor DNF re-runs included
  [ "$P62" = 1 ] && { P57_NEED=7800; [ "$PLAN" = "62r0" ] && P57_NEED=2400; P57_TAG="$PLAN"; }      # status 62 section 8 (plan62.BUDGET_S): the worst case, strictly under the registered LD_HARD offset (launch + 2.5 h / + 1.0 h)
  [ "$P60" = 1 ] && [ "$P61" != 1 ] && [ "$P62" != 1 ] && { P57_NEED=18360; [ "$PLAN" = "60b" ] && P57_NEED=13860; P57_TAG="$PLAN"; }      # status 60 section 7: 3.2 h (60a) / 2.9 h (60b) PLUS the worst-case floor DNF re-runs (36 / 18 rows x 190 s): 5.1 h / 3.85 h (plan60.BUDGET_S)
  [ "$P51E" = 1 ] && { P57_NEED=2730; [ "$REHEARSE" = 1 ] && P57_NEED=1500; P57_TAG="51e"; }      # plan51e.BUDGET_S: the battery's worst case (every slot twice + build + smoke + tests); the rehearsal's 25 min
  [ "$P63" = 1 ] && { P57_NEED=5400; [ "$REHEARSE" = 1 ] && P57_NEED=1800; P57_TAG="63"; }      # plan63.BUDGET_S (section 63): two builds + the tools tests + 10 rows of at most 150 s = 1.5 h; the rehearsal ~30 min; LD_HARD = launch + 1.5 h
  [ "$P51" = 1 ] && { P57_NEED=17400; [ "$REHEARSE" = 1 ] && P57_NEED=1200; P57_TAG="51"; }          # plan51.BUDGET_S: 76 rows x 215 s + 16 min = 4 h 50 (s51-design section 7); the rehearsal ~20 min
  P57_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan $PLAN budget: ${P57_LEFT}s to LD_HARD (at least ${LD_HARD_MIN_S:-$P57_NEED}s required: the worst case of the status)"
  [ "$P57_LEFT" -ge "${LD_HARD_MIN_S:-$P57_NEED}" ] || { era "ABORT-BUDGET plan $P57_TAG: ${P57_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-$P57_NEED}s; NOTHING WAS RUN"; exit 3; }
fi
if [ "$P67" = 1 ]; then
  # Plan 67 (registration 6): refuse with less than the registered worst case before LD_HARD (plan67.BUDGET_S: 67 = 7 500 s, LD_HARD = launch + 2.25 h; 67g = 10 000 s, + 3 h; 67x = 15 600 s, + 4.5 h; LD_HARD_MIN_S is a test seam).
  P67_NEED="$(python3 ./plan67.py budget "$PLAN")"
  P67_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan $PLAN budget: ${P67_LEFT}s to LD_HARD (at least ${LD_HARD_MIN_S:-$P67_NEED}s required: the worst case of the registration)"
  [ "$P67_LEFT" -ge "${LD_HARD_MIN_S:-$P67_NEED}" ] || { era "ABORT-BUDGET plan $PLAN: ${P67_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-$P67_NEED}s; NOTHING WAS RUN"; exit 3; }
fi
if [ "$P65" = 1 ]; then
  # Plan 65 (registration 8 item 5): refuse with less than the registered worst case before LD_HARD (plan64.py budget 65 = 6 700 s the launch, 65r = 1 800 s the rehearsal; LD_HARD = launch + 2.5 h, rehearsal + 1 h; LD_HARD_MIN_S is a test seam).
  P65_NEED="$(python3 ./plan64.py budget "65$([ "$REHEARSE" = 1 ] && echo r)")"
  P65_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan 65 budget: ${P65_LEFT}s to LD_HARD (at least ${LD_HARD_MIN_S:-$P65_NEED}s required: the worst case of the status)"
  [ "$P65_LEFT" -ge "${LD_HARD_MIN_S:-$P65_NEED}" ] || { era "ABORT-BUDGET plan 65: ${P65_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-$P65_NEED}s; NOTHING WAS RUN"; exit 3; }
fi
if [ "$W64" = 1 ]; then
  # Plan 64 (status 64 section 4, item 8): the launch is run with LD_HARD = launch + 2.5 h, the rehearsal with + 1.25 h; the guard refuses with less than the registered worst case left (plan64.BUDGET_S: every RB row at its full
  # 150 s timeout, a dead warm-up waits it out; LD_HARD_MIN_S is a test seam).
  P64_NEED=7650; [ "$REHEARSE" = 1 ] && P64_NEED=3300
  P64_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan 64 budget: ${P64_LEFT}s to LD_HARD (at least ${LD_HARD_MIN_S:-$P64_NEED}s required: the worst case of the status)"
  [ "$P64_LEFT" -ge "${LD_HARD_MIN_S:-$P64_NEED}" ] || { era "ABORT-BUDGET plan 64: ${P64_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-$P64_NEED}s; NOTHING WAS RUN"; exit 3; }
fi
take_lock "$LOCK_VM"
take_lock "$LOCK_RP"
era "LD-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 "$PROCFS"/loadavg); io=$(grep '^full' "$PROCFS"/pressure/io)"

if [ "$P57" = 1 ]; then
  # Plans 57a / 57b keep plan 56's /dev/shm pre-flight (the 5 GB rows' per-row logs live on tmpfs): >= 1 GB free (LD_SHM_MIN_KB is a test seam).
  SHM_FREE_KB="$(df -Pk "$DEVSHM" 2>/dev/null | awk 'NR==2{print $4}')"
  era "LD-ALL plan $PLAN pre-flight: ${SHM_FREE_KB:-?} KB free on $DEVSHM (need ${LD_SHM_MIN_KB:-1048576})"
  { [[ "${SHM_FREE_KB:-}" =~ ^[0-9]+$ ]] && [ "$SHM_FREE_KB" -ge "${LD_SHM_MIN_KB:-1048576}" ]; } || fail_all "ABORT-SHM-SPACE $DEVSHM has ${SHM_FREE_KB:-?} KB free, plan $PLAN needs ${LD_SHM_MIN_KB:-1048576}; NOTHING WAS RUN"
  [ -z "${LD_BASE_SHA:-}" ] || [ "$P51" = 1 ] || era "LD-ALL plan $PLAN is single-seat: LD_BASE_SHA=${LD_BASE_SHA} is ignored (no BASE tree, no second build)"
fi
if [ "$P56" = 1 ]; then
  # The raw successor dump (RWM_SUCC_DUMP) and the per-row logs live on tmpfs; the registered pre-flight is 1 GB free (LD_SHM_MIN_KB is a test seam).
  SHM_FREE_KB="$(df -Pk "$DEVSHM" 2>/dev/null | awk 'NR==2{print $4}')"
  era "LD-ALL plan 56 pre-flight: ${SHM_FREE_KB:-?} KB free on $DEVSHM (need ${LD_SHM_MIN_KB:-1048576})"
  { [[ "${SHM_FREE_KB:-}" =~ ^[0-9]+$ ]] && [ "$SHM_FREE_KB" -ge "${LD_SHM_MIN_KB:-1048576}" ]; } || fail_all "ABORT-SHM-SPACE $DEVSHM has ${SHM_FREE_KB:-?} KB free, plan 56 needs ${LD_SHM_MIN_KB:-1048576}; NOTHING WAS RUN"
  [ -z "${LD_BASE_SHA:-}" ] || era "LD-ALL plan 56 is single-seat: LD_BASE_SHA=${LD_BASE_SHA} is ignored (no BASE tree, no second build)"
fi
# Plans 62r0 / 62a: the tools-test reuse. EVERY python and bash tools test of a launch (the pre-build gates below and the suites stage's full set) is skipped iff the rehearsal's passed on a byte-identical tools/l1: tools_manifest is the sha256 of every file under tools/l1 (C-locale path
# order, no __pycache__), recorded at the rehearsal (suites/TOOLS-MANIFEST.sha256, with TOOLS-TESTS-OK) and compared at the launch (LD_REHEARSAL62 = the rehearsal's run dir). Only 62a reuses; any difference RUNS the tests (a MISS is
# not an abort: the tests are the fallback, not the gate). The engine suites follow the reference-manifest rule above.
# The identity of what the tools tests ran on: sha256 of every file under tools/l1 (the tests, the scorers, the scripts, testdata), in C-locale path order; __pycache__ / *.pyc (the parser gate runs python in this tree
# before the suites stage, so the rehearsal's and the launch's trees differ in them without any source difference) are not part of it.
tools_manifest() {
  ( cd "$(cd "$HERE/../.." && pwd)" || exit 1
    find tools/l1 -type f ! -name '*.pyc' ! -path '*/__pycache__/*' | LC_ALL=C sort | xargs -r sha256sum | sed -E 's/^([0-9a-f]{64}) [ *]/\1  /' )
}
tools_reuse_ok() {      # reads $RUN/TOOLS-MANIFEST.now (tools_manifest, taken before any test ran)
  local d="${LD_REHEARSAL62:-}/suites"
  if [ "$PLAN" != "62a" ] || [ -z "${LD_REHEARSAL62:-}" ]; then return 1; fi
  if [ ! -e "$d/TOOLS-TESTS-OK" ] || [ ! -s "$d/TOOLS-MANIFEST.sha256" ]; then era "LD-ALL tools-tests reuse MISS: $d has no TOOLS-TESTS-OK / TOOLS-MANIFEST.sha256 (the rehearsal's tools tests did not pass or were not recorded): running the python and bash tools tests"; return 1; fi
  if cmp -s "$d/TOOLS-MANIFEST.sha256" "$RUN/TOOLS-MANIFEST.now"; then era "LD-ALL tools-tests reuse HIT: tools/l1 is byte-identical to the rehearsal's ($d): the python and bash tools tests are skipped"; return 0; fi
  era "LD-ALL tools-tests reuse MISS: tools/l1 differs from the rehearsal's ($(diff "$d/TOOLS-MANIFEST.sha256" "$RUN/TOOLS-MANIFEST.now" | grep -c '^[<>]') manifest lines): running the python and bash tools tests"; return 1
}
TOOLS_REUSE=0
if [ "$P62" = 1 ]; then
  tools_manifest > "$RUN/TOOLS-MANIFEST.now"      # taken BEFORE any test runs: nothing a test writes can change what the reuse compares
  if tools_reuse_ok; then TOOLS_REUSE=1; fi
fi
if [ "$TOOLS_REUSE" = 1 ]; then
  echo "TOOLS-TESTS-REUSED the rehearsal's python and bash tools tests ($LD_REHEARSAL62/suites): the pre-build gates are skipped" > "$RUN/parser-test.log"
else
python3 ./test_loopdiag_parse.py > "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log)"
if [ "$P43" = 1 ]; then
  python3 ./test_place43_parse.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_place43_parse.py)"
fi
if [ "$P46" = 1 ]; then
  python3 ./test_harness46.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_harness46.py)"
  python3 ./test_harness47.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_harness47.py)"
fi
if [ "$P48" = 1 ]; then
  python3 ./test_bbr48_parse.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_bbr48_parse.py)"
fi
if [ "$P52" = 1 ]; then
  for t in test_bbr48_parse.py test_place38_parse.py test_tail52.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P41D" = 1 ] || [ "$GUARD_PLAN" = "41d" ]; then      # the v2 guard's own parser, scorer and VOID-rule gate (status 41d)
  for t in test_guard_v2.py test_bbr48_parse.py test_place38_parse.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P67" = 1 ]; then      # status 67: the scorer, its fixtures, the tables and the wiring, and the primitives of plans 54 / 64 / 41d its stage G rides
  for t in test_l1common.py test_pool54.py test_harness64.py test_guard_v2.py test_bbr48_parse.py test_place38_parse.py test_s67.py test_harness67.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P65" = 1 ]; then      # status 65: plan 64's primitives and tables (plan 65 rides them) and the registered scorer's cases
  for t in test_l1common.py test_pool54.py test_harness64.py test_s65_score.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$W64" = 1 ]; then      # status 64: the F2 guard's primitives, tables, scorers and wiring (the stage-G witness path reuses plan 54's helpers)
  for t in test_l1common.py test_pool54.py test_harness64.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P53" = 1 ]; then
  for t in test_place38_parse.py test_tail52.py test_snap53.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P56" = 1 ]; then      # status 56: the sweep's row parser / scorer and the modules it builds on
  for t in test_place38_parse.py test_tail52.py test_snap53.py test_pool54.py test_pool56.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P57" = 1 ]; then      # status 57a / 57b: the block's row parser / scorer and the modules it builds on
  for t in test_place38_parse.py test_tail52.py test_snap53.py test_pool54.py test_pool56.py test_pool57.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P59" = 1 ] || [ "$P60" = 1 ]; then      # status 59: the evicting-seat scorer and its row witnesses (status 60 reuses plan 59's EVICT arm)
  python3 ./test_mbar59.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_mbar59.py)"
fi
if [ "$P60" = 1 ]; then      # status 60: the gauge session's scorer, tables and wiring
  for t in test_s60.py test_harness60.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P61" = 1 ]; then      # status 61: the c3 sweep's scorer, tables and wiring
  for t in test_s61.py test_harness61.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P62" = 1 ]; then      # status 62: c3 at jitter 0: the scorer, tables and wiring
  for t in test_s62.py test_harness62.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P63" = 1 ]; then      # status 63: its tables, witnesses and wiring, and the F2 counters in l1common
  for t in test_l1common.py test_harness63.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P51E" = 1 ]; then      # status 51 block E: its scorer, tables and wiring, and the evicting-seat modules it builds on
  for t in test_mbar59.py test_s60.py test_s51e.py test_harness51.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P51" = 1 ]; then      # status 51 block 51: its scorer, tables and wiring
  for t in test_place38_parse.py test_tail52.py test_s51.py test_harness51.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P54B" = 1 ] || [ "$W54" = 1 ]; then      # status 54: the block's parser / scorer and the guard's witnesses
  for t in test_place38_parse.py test_tail52.py test_snap53.py test_pool54.py; do
    python3 ./$t >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: $t)"
  done
fi
if [ "$P38" = 1 ]; then
  python3 ./test_place38_parse.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_place38_parse.py)"
fi
if [ "$PLAN" = "36" ] || [ "$PLAN" = "40" ]; then
  python3 ./test_disp36.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_disp36.py)"
  [ "$PLAN" = "40" ] && { python3 ./test_disp40.py >> "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log: test_disp40.py)"; }
  command -v tcpdump >/dev/null 2>&1 || fail_all "ABORT-BRINGUP no tcpdump on this host"
fi
fi
era "LD-ALL parser test ok$([ "$TOOLS_REUSE" = 1 ] && echo " (reused from the rehearsal: skipped)")"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
if [ "$P67" = 1 ]; then
  era "LD-ALL plan $PLAN: the seats are built / installed by loopdiag_p67.sh (no shipped tree, no BASE tree here)"
elif [ "$P65" = 1 ]; then
  # Plan 65: no build, no cargo, no SRC / BASE_SRC checks. The two pinned binaries of status 64 are installed (as root, onto the tmpfs tree) and sha-checked against their registered pins before any row.
  T0=$(date +%s)
  sudo -n install -m 0755 -o root -g root "$LD_BIN_PREBUILT_CAND" "$BIN" || fail_all "ABORT-BUILD copy (CAND)"
  sudo -n install -m 0755 -o root -g root "$LD_BIN_PREBUILT_BASE" "$BIN_BASE" || fail_all "ABORT-BUILD copy (BASE)"
  SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"; SHA_BASE="$(sha256sum "$BIN_BASE" | cut -d' ' -f1)"
  echo "$SHA  $BIN  commit=prebuilt $LD_BIN_PREBUILT_CAND" > "$RUN/BINSHA.txt"
  echo "$SHA_BASE  $BIN_BASE  commit=prebuilt $LD_BIN_PREBUILT_BASE (BASE)" >> "$RUN/BINSHA.txt"
  era "LD-ALL binaries installed (no build): CAND $SHA BASE $SHA_BASE"
  [ "$SHA" = "$LD_BIN_SHA_CAND" ] || fail_all "ABORT-BUILD plan 65 CAND binary: installed $SHA != pinned LD_BIN_SHA_CAND $LD_BIN_SHA_CAND; NOTHING WAS RUN"
  [ "$SHA_BASE" = "$LD_BIN_SHA_BASE" ] || fail_all "ABORT-BUILD plan 65 BASE binary: installed $SHA_BASE != pinned LD_BIN_SHA_BASE $LD_BIN_SHA_BASE; NOTHING WAS RUN"
  [ "$SHA" != "$SHA_BASE" ] || fail_all "ABORT-BUILD BASE and CAND binaries are identical"
  # the seat witness, as plan 64: CAND holds [SYMSZ] and [UNFRAMABLE], BASE neither
  for _s in '[SYMSZ]' '[UNFRAMABLE]'; do
    grep -aqF -- "$_s" "$BIN" || fail_all "ABORT-SEAT plan 65: the CAND binary has no $_s string: it is not the F2 build; NOTHING WAS RUN"
    ! grep -aqF -- "$_s" "$BIN_BASE" || fail_all "ABORT-SEAT plan 65: the BASE binary carries $_s: it is not main's; NOTHING WAS RUN"
  done
  era "LD-ALL seat witness: CAND binary holds [SYMSZ] and [UNFRAMABLE], BASE neither"
  era "SUITES-NOT-RUN plan 65: binaries sha-pinned to status 64's BINSHA.txt (no engine suites, no build)"
else
[ -d "$SRC" ] || fail_all "ABORT-BUILD no $SRC"
[ ! -e "$SRC/target" ] || fail_all "ABORT-BUILD $SRC/target is not fresh"
[ ! -e "$TARGET" ] || fail_all "ABORT-BUILD $TARGET is not fresh"
T0=$(date +%s)
[ "$P62" != 1 ] || [ -n "${LD_BIN_PREBUILT:-}" ] || fail_all "ABORT-BUILD plan $PLAN never builds: LD_BIN_PREBUILT (the staged binary, its sha checked against LD_BIN_SHA_EXPECT) is required; NOTHING WAS RUN"
if [ "$P60" = 1 ] && [ -n "${LD_BIN_PREBUILT:-}" ]; then
  # Plan 60b may reuse 60a's binary (its sha is still checked against LD_BIN_SHA_EXPECT below): no build, the TARGET dir stays absent.
  [ -x "$LD_BIN_PREBUILT" ] || fail_all "ABORT-BUILD LD_BIN_PREBUILT=$LD_BIN_PREBUILT is not an executable file"
  mkdir -p "$TARGET/release" && cp "$LD_BIN_PREBUILT" "$TARGET/release/raptorpath" || fail_all "ABORT-BUILD prebuilt copy"
  era "LD-ALL binary prebuilt from $LD_BIN_PREBUILT (no build)"
elif [ "$W64" = 1 ] && [ -n "${LD_BIN_PREBUILT_CAND:-}" ]; then
  # Plan 64: the launch installs the rehearsal's CAND build (its sha is checked against LD_BIN_SHA_CAND below, before any row): no build, the TARGET dir stays absent.
  [ -x "$LD_BIN_PREBUILT_CAND" ] || fail_all "ABORT-BUILD LD_BIN_PREBUILT_CAND=$LD_BIN_PREBUILT_CAND is not an executable file"
  mkdir -p "$TARGET/release" && cp "$LD_BIN_PREBUILT_CAND" "$TARGET/release/raptorpath" || fail_all "ABORT-BUILD prebuilt copy (CAND)"
  era "LD-ALL CAND binary prebuilt from $LD_BIN_PREBUILT_CAND (no build)"
else
( cd "$SRC" && CARGO_TARGET_DIR="$TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$BUILDLOG" 2>&1 \
  || fail_all "ABORT-BUILD ($BUILDLOG)"
fi
if [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  sudo -n install -m 0755 -o root -g root "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
else
  cp "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
fi
era "LD-ALL built wall=$(( $(date +%s) - T0 ))s"
SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
# Plan 43b runs the binary of status 43 (run43 b2b75b7): any other is ABORT-SHA, nothing measured.
SHA43="94959c05ec9898b4deaea53f815b60d8ba1d9636030f3959bea259d79c1dc008"
[ "$PLAN" != "43b" ] || [ "$SHA" = "$SHA43" ] || fail_all "ABORT-SHA plan 43b needs status 43's binary $SHA43, built $SHA"
echo "$SHA  $BIN  commit=$(cat "$ROOT/COMMIT" 2>/dev/null)" > "$RUN/BINSHA.txt"
era "LD-ALL binary $SHA"
# Plans 60a / 60b: the registered bin_sha (60-pre's crate-directory build) must equal the built binary, before any row (section 9 item 2: an unequal bin_sha stops the launch).
if [ "$P60" = 1 ] && [ "$SHA" != "$LD_BIN_SHA_EXPECT" ]; then
  fail_all "ABORT-BIN-SHA plan $PLAN: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"
fi
[ "$P60" != 1 ] || era "LD-ALL bin_sha equals the registered value $LD_BIN_SHA_EXPECT"
# Plan 64: a prebuilt binary REQUIRES its pin; a pin, when given, must equal the installed binary (built or prebuilt), before any row. The rehearsal sets no pin: it records both shas (BINSHA.txt) for the amendment.
if [ "$W64" = 1 ]; then
  { [ -z "${LD_BIN_PREBUILT_CAND:-}" ] || [ -n "${LD_BIN_SHA_CAND:-}" ]; } || fail_all "ABORT-BUILD plan 64: LD_BIN_PREBUILT_CAND needs LD_BIN_SHA_CAND (the amendment's pin); NOTHING WAS RUN"
  { [ -z "${LD_BIN_SHA_CAND:-}" ] || [ "$SHA" = "$LD_BIN_SHA_CAND" ]; } || fail_all "ABORT-BUILD plan 64 CAND binary: built / installed $SHA != pinned LD_BIN_SHA_CAND $LD_BIN_SHA_CAND; NOTHING WAS RUN"
fi
if [ "$P61" = 1 ]; then
  era "LD-ALL toolchain $(command -v rustc >/dev/null 2>&1 && rustc -V 2>&1 | head -1 || echo "rustc unavailable") | $(cargo -V 2>&1 | head -1)"      # section 7: a bin_sha mismatch most likely means the VM toolchain moved
  # Status 61 Amendment 2: the 61a control is W 512, registered outright. A 61a launch, the rehearsal included, refuses unless LD_CTL61=512 (nothing has run yet). The global default is not flipped (61b's control stays W 2 048).
  if [ "$PLAN" = "61a" ] && [ "${LD_CTL61:-}" != "512" ]; then
    fail_all "ABORT-CONTROL61 plan 61a: LD_CTL61='${LD_CTL61:-}' (the control is W 512 by status 61 Amendment 2: set LD_CTL61=512); NOTHING WAS RUN"
  fi
  # Section 6.1, priority 5 / 6: 61a needs the rehearsal's pilot knee (LD_KNEE61) and a control (LD_NOGO=control is the rehearsal's NOGO-CONTROL decision); a rehearsal needs neither. Nothing has run yet.
  if [ "$PLAN" = "61a" ] && [ "$REHEARSE" != "1" ]; then
    python3 ./plan61.py plan 61a smoke > /dev/null 2>&1 || fail_all "ABORT-PILOT plan 61a: LD_KNEE61='${LD_KNEE61:-}' is unset or invalid (the rehearsal's PILOT61 W-hat is required); NOTHING WAS RUN"
    case ",${LD_NOGO:-}," in *,control,*) fail_all "NOGO-CONTROL plan 61a: the rehearsal found no valid control (LD_NOGO=${LD_NOGO}); 61a does not launch; NOTHING WAS RUN" ;; esac
  fi
fi
if [ "$P62" = 1 ]; then
  # Status 62 (docs/derivations/s62-registration.md section 3.2 / 3.3 / 8): the control is W 512 (LD_CTL62=512, both launches); 62a needs the rehearsal's pilot knee (LD_KNEE62, the rehearsal's `PILOT62 W=` verbatim) and
  # the rehearsal's own GO (LD_REHEARSAL62 = the rehearsal's run dir, which holds rep62r0/smoke-check.txt: `LABEL62 GO-c3j0 GO` and a PILOT62 W= equal to LD_KNEE62). Nothing has run yet.
  [ "${LD_CTL62:-}" = "512" ] || fail_all "ABORT-CONTROL62 plan $PLAN: LD_CTL62='${LD_CTL62:-}' (the control is W 512, section 3.2: set LD_CTL62=512); NOTHING WAS RUN"
  if [ "$PLAN" = "62a" ]; then
    python3 ./plan62.py plan 62a smoke > /dev/null 2>&1 || fail_all "ABORT-PILOT plan 62a: LD_KNEE62='${LD_KNEE62:-}' is unset or invalid (the rehearsal's PILOT62 W-hat is required); NOTHING WAS RUN"
    R62F="${LD_REHEARSAL62:-/nonexistent}/rep62r0/smoke-check.txt"
    { [ -s "$R62F" ] && grep -aqx 'LABEL62 GO-c3j0 GO' "$R62F"; } || fail_all "ABORT-REHEARSAL62 plan 62a refuses unless the rehearsal printed 'LABEL62 GO-c3j0 GO' (LD_REHEARSAL62='${LD_REHEARSAL62:-}', $R62F); NOTHING WAS RUN"
    R62W="$(grep -aoE '^PILOT62 W=[0-9.]+' "$R62F" | tail -1 | cut -d= -f2)"
    { [ -n "$R62W" ] && [ "$R62W" = "${LD_KNEE62:-}" ]; } || fail_all "ABORT-REHEARSAL62 LD_KNEE62='${LD_KNEE62:-}' is not the rehearsal's PILOT62 W=${R62W:-none}; NOTHING WAS RUN"
    era "LD-ALL plan 62a: the rehearsal's GO and PILOT62 W=$R62W read from $R62F"
  fi
fi
# Plan 51e: E-pre's bin_sha, before any row. Plan 51: the rehearsal RECORDS the bin_sha (BINSHA.txt, this line); a battery launch must equal it.
if [ "$P51E" = 1 ] && [ "$SHA" != "$LD_BIN_SHA_EXPECT" ]; then
  fail_all "ABORT-BIN-SHA plan 51e: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"
fi
[ "$P51E" != 1 ] || era "LD-ALL bin_sha equals the registered value $LD_BIN_SHA_EXPECT"
if [ "$P51" = 1 ]; then
  if [ -n "${LD_BIN_SHA_EXPECT:-}" ] && [ "$SHA" != "$LD_BIN_SHA_EXPECT" ]; then
    fail_all "ABORT-BIN-SHA plan 51: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"
  fi
  if [ -n "${LD_BIN_SHA_EXPECT:-}" ]; then era "LD-ALL bin_sha equals the registered value $LD_BIN_SHA_EXPECT"
  else era "LD-ALL plan 51 bin_sha RECORDED (rehearsal): $SHA -- register it as LD_BIN_SHA_EXPECT for the battery launches"; fi
fi
# Plan 51: the VM pre-flight (s51-design section 6 item 2): the dial pins on the BUILT tree, before any row. A pin that fails, or that no test line proves executed, stops the launch (rule 1).
dialpins() {
  local REPO log="$RUN/dialpins.log" rc=0 pat
  REPO="$(cd "$HERE/../.." && pwd)"
  : > "$log"
  ( cd "$REPO" && CARGO_TARGET_DIR="$TARGET" timeout 3600 "$CARGO" test -p raptorpath --release --lib -- delta_budget_b_is_the_dial_not_a_mode scheduling_weights_are_the_dial_not_a_mode control::fec_rate --test-threads=2 ) >> "$log" 2>&1 || rc=1
  ( cd "$REPO" && CARGO_TARGET_DIR="$TARGET" timeout 3600 "$CARGO" test -p raptorpath-wasm --lib -- test_continuum_one_law_across_the_dial ) >> "$log" 2>&1 || rc=1
  for pat in '^test .*delta_budget_b_is_the_dial_not_a_mode \.\.\. ok$' '^test .*scheduling_weights_are_the_dial_not_a_mode \.\.\. ok$' '^test .*control::fec_rate.* \.\.\. ok$' '^test .*test_continuum_one_law_across_the_dial \.\.\. ok$'; do
    grep -aqE "$pat" "$log" || { era "LD-ALL dial pin NOT EXECUTED: $pat"; rc=1; }
  done
  ! grep -aqE '^test .* FAILED$' "$log" || rc=1
  [ "$rc" = 0 ] || fail_all "ABORT-DIALPINS plan 51: a dial pin failed or did not execute ($log); NOTHING WAS RUN"
  era "LD-ALL dial pins ok (delta_budget_b_is_the_dial_not_a_mode, scheduling_weights_are_the_dial_not_a_mode, control::fec_rate, test_continuum_one_law_across_the_dial)"
}
[ "$P51" != 1 ] || dialpins
SHA_BASE=""
if [ "$P63" = 1 ]; then
  # Plan 63: the CAND binary against LD_BIN_SHA_CAND (a rehearsal records it), then the CONTROL seat: LD_BIN_CONTROL (no build) or the BASE tree's build, installed as .../bin-base/raptorpath and equal to its pin.
  if [ -n "${LD_BIN_SHA_CAND:-}" ]; then
    [ "$SHA" = "$LD_BIN_SHA_CAND" ] || fail_all "ABORT-BIN-SHA plan 63 CAND: built $SHA != registered $LD_BIN_SHA_CAND; NOTHING WAS RUN"
    era "LD-ALL CAND bin_sha equals the registered value $LD_BIN_SHA_CAND"
  else
    era "LD-ALL plan 63 CAND bin_sha RECORDED (rehearsal): $SHA -- register it as LD_BIN_SHA_CAND for the launch"
  fi
  if [ -n "${LD_BIN_CONTROL:-}" ]; then
    [ -x "$LD_BIN_CONTROL" ] || fail_all "ABORT-BUILD LD_BIN_CONTROL=$LD_BIN_CONTROL is not an executable file"
    CTL_SRC_BIN="$LD_BIN_CONTROL"; era "LD-ALL CONTROL binary taken from $LD_BIN_CONTROL (no build)"
  else
    [ -d "$BASE_SRC" ] || fail_all "ABORT-BUILD no $BASE_SRC (plan 63's CONTROL tree) and no LD_BIN_CONTROL"
    [ ! -e "$BASE_SRC/target" ] || fail_all "ABORT-BUILD $BASE_SRC/target is not fresh"
    [ ! -e "$BASE_TARGET" ] || fail_all "ABORT-BUILD $BASE_TARGET is not fresh"
    ( cd "$BASE_SRC" && CARGO_TARGET_DIR="$BASE_TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$ROOT/build-base.log" 2>&1 || fail_all "ABORT-BUILD (CONTROL: $ROOT/build-base.log)"
    CTL_SRC_BIN="$BASE_TARGET/release/raptorpath"
  fi
  sudo -n install -m 0755 -o root -g root "$CTL_SRC_BIN" "$BIN_BASE" || fail_all "ABORT-BUILD copy (CONTROL)"
  SHA_BASE="$(sha256sum "$BIN_BASE" | cut -d' ' -f1)"
  echo "$SHA_BASE  $BIN_BASE  (CONTROL)" >> "$RUN/BINSHA.txt"
  [ "$SHA_BASE" = "$LD_BIN_SHA_CONTROL" ] || fail_all "ABORT-BIN-SHA plan 63 CONTROL: $SHA_BASE != pinned $LD_BIN_SHA_CONTROL; NOTHING WAS RUN"
  era "LD-ALL CONTROL bin_sha equals the pinned value $LD_BIN_SHA_CONTROL"
  [ "$SHA_BASE" != "$SHA" ] || fail_all "ABORT-BUILD CONTROL and CAND binaries are identical"
fi
if [ "$P41" = 1 ]; then
  # BASE: main's own binary, its own archive tree and its own fresh target dir (never shared with the shipped tree's).
  [ -d "$BASE_SRC" ] || fail_all "ABORT-BUILD no $BASE_SRC"
  [ ! -e "$BASE_SRC/target" ] || fail_all "ABORT-BUILD $BASE_SRC/target is not fresh"
  [ ! -e "$BASE_TARGET" ] || fail_all "ABORT-BUILD $BASE_TARGET is not fresh"
  # BASE's pin: LD_BASE_SHA (the full sha, recorded by amendment before launch) when set, else the historical main 4926822.
  BASE_PIN="${LD_BASE_SHA:-4926822}"
  case "$(cat "$ROOT/base/COMMIT" 2>/dev/null)" in "$BASE_PIN"*) ;; *) fail_all "ABORT-BUILD base COMMIT is not $BASE_PIN" ;; esac
  T1=$(date +%s)
  if [ "$W64" = 1 ] && [ -n "${LD_BIN_PREBUILT_BASE:-}" ]; then
    # Plan 64: the rehearsal's BASE build, installed instead of built (its sha is checked against LD_BIN_SHA_BASE below).
    [ -x "$LD_BIN_PREBUILT_BASE" ] || fail_all "ABORT-BUILD LD_BIN_PREBUILT_BASE=$LD_BIN_PREBUILT_BASE is not an executable file"
    mkdir -p "$BASE_TARGET/release" && cp "$LD_BIN_PREBUILT_BASE" "$BASE_TARGET/release/raptorpath" || fail_all "ABORT-BUILD prebuilt copy (BASE)"
    era "LD-ALL BASE binary prebuilt from $LD_BIN_PREBUILT_BASE (no build)"
  else
  ( cd "$BASE_SRC" && CARGO_TARGET_DIR="$BASE_TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$ROOT/build-base.log" 2>&1 \
    || fail_all "ABORT-BUILD (base: $ROOT/build-base.log)"
  fi
  sudo -n install -m 0755 -o root -g root "$BASE_TARGET/release/raptorpath" "$BIN_BASE" || fail_all "ABORT-BUILD copy (base)"
  SHA_BASE="$(sha256sum "$BIN_BASE" | cut -d' ' -f1)"
  echo "$SHA_BASE  $BIN_BASE  commit=$(cat "$ROOT/base/COMMIT" 2>/dev/null) (BASE)" >> "$RUN/BINSHA.txt"
  era "LD-ALL built BASE wall=$(( $(date +%s) - T1 ))s binary $SHA_BASE"
  [ "$SHA_BASE" != "$SHA" ] || fail_all "ABORT-BUILD BASE and MAIN binaries are identical"
  if [ "$W64" = 1 ]; then
    { [ -z "${LD_BIN_PREBUILT_BASE:-}" ] || [ -n "${LD_BIN_SHA_BASE:-}" ]; } || fail_all "ABORT-BUILD plan 64: LD_BIN_PREBUILT_BASE needs LD_BIN_SHA_BASE (the amendment's pin); NOTHING WAS RUN"
    { [ -z "${LD_BIN_SHA_BASE:-}" ] || [ "$SHA_BASE" = "$LD_BIN_SHA_BASE" ]; } || fail_all "ABORT-BUILD plan 64 BASE binary: built / installed $SHA_BASE != pinned LD_BIN_SHA_BASE $LD_BIN_SHA_BASE; NOTHING WAS RUN"
    # The seat witness (registration 2.1, before any row, a rehearsal included): the CAND binary holds the strings [SYMSZ] and [UNFRAMABLE] (the F2 framing), the BASE binary neither (main).
    for _s in '[SYMSZ]' '[UNFRAMABLE]'; do
      grep -aqF -- "$_s" "$BIN" || fail_all "ABORT-SEAT plan 64: the CAND binary has no $_s string: it is not the F2 build; NOTHING WAS RUN"
      ! grep -aqF -- "$_s" "$BIN_BASE" || fail_all "ABORT-SEAT plan 64: the BASE binary carries $_s: it is not main's; NOTHING WAS RUN"
    done
    era "LD-ALL seat witness: CAND binary holds [SYMSZ] and [UNFRAMABLE], BASE neither"
    # The rehearsal's two builds, kept in its run dir (persisted) for the launch's LD_BIN_PREBUILT_BASE / _CAND.
    mkdir -p "$RUN/bins64" && cp "$BIN" "$RUN/bins64/cand-raptorpath" && cp "$BIN_BASE" "$RUN/bins64/base-raptorpath" || fail_all "ABORT-BUILD cannot keep the binaries in $RUN/bins64"
  fi
fi
if [ "$P46" = 1 ]; then
  grep -aq -- "RWM_PLACE_WIRE" "$BIN" && fail_all "ABORT-CAND the CAND binary carries RWM_PLACE_WIRE: it does not contain the promotion"
  [ "$P41" = 1 ] && grep -aq -- "RWM_PLACE_WIRE" "$BIN_BASE" && fail_all "ABORT-CAND the BASE binary carries RWM_PLACE_WIRE: it is not main's"
fi

fi      # plan 65 skips the build block above
# ── plan 27: the test suites, after the battery ledger is on disk ────────────
# Same on-disk source tree (the workspace root, as threadr2_run_all.sh), its own
# fresh target dir; logs on the tmpfs run dir. A failure here is reported
# (TESTS-FAIL), never a battery void. The locks stay held.
SD="$RUN"      # where the suite logs and TESTS.txt go (plan 47: $RUN/suites)
ENGINE_SUITES=1; ENGINE_WHY=""      # plan 56 sets ENGINE_SUITES=0 on a reference-manifest hit (suites_stage56); every other plan always runs them
run_suites() {
  local REPO TT TFAIL=0
  REPO="$(cd "$HERE/../.." && pwd)"
  TT="$ROOT/target-tests"
  : > "$SD/TESTS.txt"
  if [ -e "$TT" ]; then
    echo "REAL-FAILURE suites: $TT is not fresh -- suites not run" >> "$SD/TESTS.txt"
    touch "$SD/TESTS-FAIL"; era "LD-ALL suites NOT RUN: $TT is not fresh"; return 1
  fi
  tst() { # name cmd...   (cmd may carry a `--` before the test-binary args)
    local name="$1"; shift
    local t0; t0=$(date +%s)
    ( cd "$REPO" && CARGO_TARGET_DIR="$TT" "$@" ) > "$SD/test-$name.log" 2>&1
    local rc=$?
    local sum
    sum=$(grep -a '^test result:' "$SD/test-$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{printf "passed=%d failed=%d ignored=%d binaries=%d", p, f, i, NR}')
    echo "TEST $name rc=$rc wall=$(( $(date +%s) - t0 ))s $sum cmd=[$*]" >> "$SD/TESTS.txt"
    grep -a -E '^test .* FAILED$|^---- ' "$SD/test-$name.log" | head -40 >> "$SD/TESTS.txt"
    era "LD-ALL test $name rc=$rc $sum"
    [ "$rc" = "0" ] && return 0
    # ADR-0080's flake rule (as tools/l1/suites_run.sh): a failure is a failure. The solo re-run only
    # classifies it, FLAKE (passed again) or REAL; both fail the suite unless the test is on
    # raptorpath/tests/quarantine.txt (tools/ci/flake_rule.py).
    local real=0 t kind
    if ! grep -aq -E '^test .* FAILED$' "$SD/test-$name.log"; then
      echo "REAL-FAILURE $name rc=$rc with no failing test named (build error?)" >> "$SD/TESTS.txt"
      return 1
    fi
    for t in $(grep -a -E '^test .* FAILED$' "$SD/test-$name.log" | awk '{print $2}' | sort -u); do
      local base=("$@")
      local cmd=() a seen=0
      for a in "${base[@]}"; do [ "$a" = "--" ] && seen=1; [ "$seen" = "0" ] && cmd+=("$a"); done
      if ( cd "$REPO" && CARGO_TARGET_DIR="$TT" "${cmd[@]}" -- "$t" --exact --test-threads=1 ) > "$SD/test-$name-rerun.log" 2>&1           && grep -aq "^test $t ... ok" "$SD/test-$name-rerun.log"; then
        echo "FLAKE $name $t (passed on the immediate solo re-run)" >> "$SD/TESTS.txt"
        kind=FLAKE-FAILURE
      else
        kind=REAL-FAILURE
      fi
      if python3 "$REPO/tools/ci/flake_rule.py" --quarantine "$REPO/raptorpath/tests/quarantine.txt" --check "$t"; then
        echo "QUARANTINED $name $t ($kind, on the quarantine list)" >> "$SD/TESTS.txt"
      else
        echo "$kind $name $t" >> "$SD/TESTS.txt"
        real=1
      fi
    done
    return $real
  }
  if [ "$ENGINE_SUITES" = 1 ]; then
    tst main timeout 7200 "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
    tst doc timeout 3600 "$CARGO" test --doc -p raptorpath --release || TFAIL=1
    tst wasm env -u GOLDEN_CAPTURE timeout 3600 "$CARGO" test -p raptorpath-wasm || TFAIL=1
    tst core timeout 3600 "$CARGO" test -p raptorpath-core || TFAIL=1
    tst debug-witness timeout 7200 "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
    # F2 (ADR-0078): the tests behind the off-by-default `test-seams` feature (the plain runs above skip them); exactly as f2-impl's suites_run.sh has it. A tree without the feature has no such tests.
    if grep -aq '^test-seams' "$REPO/raptorpath/Cargo.toml" 2>/dev/null; then
      tst seams timeout 3600 "$CARGO" test -p raptorpath --release --features test-seams --test f2_unframable_closes --test f2_mixed_symbol_size --test f2_realtime_packed -- --test-threads=1 || TFAIL=1
    else
      echo "SEAMS n/a (raptorpath/Cargo.toml declares no test-seams feature)" >> "$SD/TESTS.txt"
    fi
  else
    echo "ENGINE-SUITES-REUSED $ENGINE_WHY" >> "$SD/TESTS.txt"
  fi
  if [ "$PLAN" = "56" ] || [ "$P57" = 1 ] || [ "$PLAN" = "41d" ] || [ "$PLAN" = "41" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || [ "$PLAN" = "52" ] || [ "$PLAN" = "53" ] || [ "$PLAN" = "54b" ] || [ "$P65" = 1 ] || [ "$P67" = 1 ] || { [ "$P46" = 1 ] && [ "${LD_P46_SUITES:-0}" = "1" ]; }; then
    # Status 41 / 47 / 48 / 49: "the tools/l1 python tests" are all of them, except test_stubrun.py: it re-runs this very script against stubs
    # (minutes, even in its fast mode) and belongs to harness changes only, never to a measurement session.
    if [ "$P62" = 1 ] && [ "$TOOLS_REUSE" = 1 ]; then
      echo "TOOLS-TESTS-REUSED the rehearsal's python and bash tools tests ($LD_REHEARSAL62/suites: tools/l1 byte-identical, $(wc -l < "$RUN/TOOLS-MANIFEST.now") files)" >> "$SD/TESTS.txt"
    else
    tst parsers bash -c 'cd tools/l1 && for t in test_*.py; do [ "$t" = test_stubrun.py ] && continue; python3 "$t" || { echo "PYTHON-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1
    # Plan 56 (status 56): the two bash tools tests of section 54 (test_lib_battery.sh, test_abort_witness.sh) always run too (offline stubs: no root, no namespaces).
    { [ "$PLAN" = "56" ] || [ "$P57" = 1 ] || [ "$W64" = 1 ] || [ "$P65" = 1 ] || [ "$P67" = 1 ]; } && { tst bashtests bash -c 'cd tools/l1 && for t in test_lib_battery.sh test_abort_witness.sh; do bash "$t" || { echo "BASH-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1; }
    fi
  else
    tst parsers bash -c "cd tools/l1 && python3 test_loopdiag_parse.py && python3 test_anchordiag_parse.py" || TFAIL=1
  fi
  if [ "$P62" = 1 ] && [ "$TFAIL" = "0" ]; then cp "$RUN/TOOLS-MANIFEST.now" "$SD/TOOLS-MANIFEST.sha256"; fi      # the record a later launch's reuse compares against (written only when the tools tests passed, or were themselves reused)
  [ "$ENGINE_SUITES" = 1 ] && echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$SD/test-debug-witness.log")" >> "$SD/TESTS.txt"
  # TESTS-OK and SRC-MANIFEST.sha256 are a SUITE RECORD that LD_PRIOR_TESTS (another plan's launch) accepts as "the engine suites passed on this tree": written only when they ran. A run that
  # skipped them (plan 56: a reference-manifest hit, or a rehearsal) leaves TOOLS-TESTS-OK instead.
  if [ "$ENGINE_SUITES" = 1 ]; then src_manifest > "$SD/SRC-MANIFEST.sha256"; fi       # what the run covered: LD_PRIOR_TESTS reuse compares against it
  # TOOLS-TESTS-OK is written with TOOLS-MANIFEST.sha256 (what the tools tests covered): LD_PRIOR_TOOLS reuse (plan 64) compares against it.
  if [ "$TFAIL" = "0" ]; then if [ "$ENGINE_SUITES" = 1 ]; then touch "$SD/TESTS-OK"; else tools_manifest > "$SD/TOOLS-MANIFEST.sha256"; touch "$SD/TOOLS-TESTS-OK"; fi; else touch "$SD/TESTS-FAIL"; fi
  era "LD-ALL suites done tfail=$TFAIL"
  return $TFAIL
}
# The identity of what the suites ran on: sha256 of every file under raptorpath/src, raptorpath/tests, raptorpath-math/src, raptorpath-core/src, raptorpath-wasm/src, gf256/src and vendor/
# (a patched dependency), plus the workspace Cargo.toml / Cargo.lock and every crate's Cargo.toml, in C-locale path order. A dependency patch invalidates a reuse.
src_manifest() {
  ( cd "$(cd "$HERE/../.." && pwd)" || exit 1
    { ls Cargo.toml Cargo.lock ./*/Cargo.toml 2>/dev/null | sed 's#^\./##'
      find raptorpath/src raptorpath/tests raptorpath-math/src raptorpath-core/src raptorpath-wasm/src gf256/src vendor -type f 2>/dev/null; } | LC_ALL=C sort -u | xargs -r sha256sum )
}
# LD_PRIOR_TESTS (plan 34's amendment 2, now plans 41c / 46 / 47 / 48 too): a green suite run of a BYTE-IDENTICAL tree (see src_manifest) may stand in for
# this session's suites. "Same tree" is the src_manifest paths above. LD_PRIOR_TESTS is that run's dir (or its parent: the suites/ subdir of a plan 47 / 48 run is found too); it must hold TESTS-OK
# and the SRC-MANIFEST.sha256 recorded at suite time, and the manifest of this tree must equal it. Anything else is ABORT-TESTS.
reuse_prior_tests() {
  local d="" c
  for c in "$LD_PRIOR_TESTS" "$LD_PRIOR_TESTS/suites"; do [ -e "$c/TESTS-OK" ] && { d="$c"; break; }; done
  [ -n "$d" ] || { era "LD-ALL prior suites: no TESTS-OK in $LD_PRIOR_TESTS"; return 1; }
  [ -s "$d/SRC-MANIFEST.sha256" ] || { era "LD-ALL prior suites: $d has no SRC-MANIFEST.sha256 (recorded at suite time since this change); cannot prove the same tree"; return 1; }
  src_manifest > "$SD/SRC-MANIFEST.now"
  if ! cmp -s "$d/SRC-MANIFEST.sha256" "$SD/SRC-MANIFEST.now"; then
    era "LD-ALL prior suites: the manifest (src, tests, workspace and crate Cargo files, vendor/) differs from the tree $d ran on ($(diff "$d/SRC-MANIFEST.sha256" "$SD/SRC-MANIFEST.now" | grep -c '^[<>]') manifest lines)"; return 1
  fi
  cp "$d/TESTS.txt" "$SD/TESTS-prior.txt" && cp "$d/SRC-MANIFEST.sha256" "$SD/SRC-MANIFEST.sha256" && touch "$SD/TESTS-OK"
  era "LD-ALL suites: reused TESTS-OK from $d (manifest byte-identical, $(wc -l < "$SD/SRC-MANIFEST.sha256") files)"
}
# Plan 64 (status 64 section 1.3): the engine suites are reused ONLY by a reference-manifest HIT over the WIDER src_manifest() set (Cargo.toml, Cargo.lock, every crate's Cargo.toml, the engine crates' src, raptorpath/tests, vendor/;
# not tools/ or docs/) against LD_REF_MANIFEST (default ref64-manifest.sha256, generated from ab9feec4's archive). A MISS stops the launch before any row (ABORT-SUITES, a rehearsal included): the tree is not the one whose suites
# ran; running them in the session would hide that. LD_PRIOR_TESTS is refused (one reuse rule). The tools tests (python and bash) are reused from LD_PRIOR_TOOLS (the rehearsal's run dir) when tools/l1 is byte-identical to the
# one it ran on (TOOLS-TESTS-REUSED), else they run; a rehearsal always runs them and writes TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256. src_manifest() itself is unchanged; its sha256sum text/binary marker is normalised here.
suites_stage64() {
  [ -z "${LD_PRIOR_TESTS:-}" ] || fail_all "ABORT-TESTS plan 64 reuses the engine suites by the reference manifest (LD_REF_MANIFEST), not by LD_PRIOR_TESTS"
  [ -s "$REF_MANIFEST" ] || fail_all "ABORT-SUITES no reference manifest at $REF_MANIFEST"
  cp "$REF_MANIFEST" "$SD/REF-MANIFEST.sha256"
  src_manifest | sed -E 's/^([0-9a-f]{64}) [ *]/\1  /' > "$SD/SRC-MANIFEST.now"
  if cmp -s "$SD/REF-MANIFEST.sha256" "$SD/SRC-MANIFEST.now"; then
    ENGINE_WHY="(src_manifest() byte-identical to the reference manifest $REF_MANIFEST, $(wc -l < "$SD/SRC-MANIFEST.now") files)"
    era "LD-ALL suite-reuse HIT: engine suites skipped $ENGINE_WHY"; ENGINE_SUITES=0
  else
    era "LD-ALL suite-reuse MISS: src_manifest() differs from the reference manifest ($(diff "$SD/REF-MANIFEST.sha256" "$SD/SRC-MANIFEST.now" | grep -c '^[<>]') manifest lines)"
    fail_all "ABORT-SUITES plan 64 suite-reuse MISS: this tree is not the tested tree of the reference manifest $REF_MANIFEST; NOTHING WAS RUN"
  fi
  if [ "$REHEARSE" != 1 ] && [ -n "${LD_PRIOR_TOOLS:-}" ] && reuse_prior_tools; then
    echo "ENGINE-SUITES-REUSED $ENGINE_WHY" > "$SD/TESTS.txt"
    echo "TOOLS-TESTS-REUSED $PRIOR_TOOLS_DIR" >> "$SD/TESTS.txt"
  else
    run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
  fi
}
reuse_prior_tools() {
  local d="" c
  PRIOR_TOOLS_DIR=""
  for c in "$LD_PRIOR_TOOLS" "$LD_PRIOR_TOOLS/suites"; do [ -e "$c/TOOLS-TESTS-OK" ] && { d="$c"; break; }; done
  [ -n "$d" ] && [ -s "$d/TOOLS-MANIFEST.sha256" ] || { era "LD-ALL tools tests: no TOOLS-TESTS-OK with a TOOLS-MANIFEST.sha256 in $LD_PRIOR_TOOLS: the tools tests run"; return 1; }
  tools_manifest > "$SD/TOOLS-MANIFEST.now"
  if ! cmp -s "$d/TOOLS-MANIFEST.sha256" "$SD/TOOLS-MANIFEST.now"; then
    era "LD-ALL tools tests: tools/l1 differs from the tree the rehearsal ran on ($(diff "$d/TOOLS-MANIFEST.sha256" "$SD/TOOLS-MANIFEST.now" | grep -c '^[<>]') manifest lines): the tools tests run"; return 1
  fi
  cp "$d/TOOLS-MANIFEST.sha256" "$SD/TOOLS-MANIFEST.sha256" && cp "$d/TESTS.txt" "$SD/TESTS-tools-prior.txt" && touch "$SD/TOOLS-TESTS-OK"
  PRIOR_TOOLS_DIR="$d"
  era "TOOLS-TESTS-REUSED $d (tools/l1 byte-identical, $(wc -l < "$SD/TOOLS-MANIFEST.sha256") files)"
}
# The suites step of the candidate plans: a verified reuse, else skipped by a rehearsal, else run.
# Plan 65: no engine suites at all (the binaries are sha-pinned to status 64's record); the tools tests come from LD_PRIOR_TOOLS (the rehearsal's run dir) on a byte-identical tools/l1 (TOOLS-TESTS-REUSED), else they run;
# a rehearsal always runs them and writes TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256.
suites_stage65() {
  ENGINE_SUITES=0; ENGINE_WHY="(SUITES-NOT-RUN plan 65: binaries sha-pinned to status 64's BINSHA.txt)"
  if [ "$REHEARSE" != 1 ] && [ -n "${LD_PRIOR_TOOLS:-}" ] && reuse_prior_tools; then
    echo "ENGINE-SUITES-REUSED $ENGINE_WHY" > "$SD/TESTS.txt"
    echo "TOOLS-TESTS-REUSED $PRIOR_TOOLS_DIR" >> "$SD/TESTS.txt"
  else
    run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
  fi
}
suites_stage() {
  if [ "$P65" = 1 ]; then suites_stage65; return; fi
  if [ "$W64" = 1 ]; then suites_stage64; return; fi      # plan 64: the reference-manifest reuse over src_manifest() and the tools-test reuse (above)
  if [ "$PLAN" = "56" ] || [ "$P57" = 1 ]; then suites_stage56; return; fi      # plan 56 reuses by the reference manifest (below), never by LD_PRIOR_TESTS
  if [ -n "${LD_PRIOR_TESTS:-}" ]; then reuse_prior_tests || fail_all "ABORT-TESTS (prior suites not reusable: $LD_PRIOR_TESTS)"
  elif [ "$REHEARSE" = 1 ]; then era "LD-ALL suites SKIPPED (rehearsal)"
  else run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"; fi
}
# Plan 56 (status 56): the engine suites are skipped ONLY when a per-file sha256 manifest of raptorpath/src and raptorpath/tests equals the reference manifest
# (tools/l1/ref56-manifest.sha256, generated from git at cand54 7fd92dd5 -- the tree that passed section 54's suites -- by ref_manifest.sh; LD_REF_MANIFEST overrides the path).
# LD_PRIOR_TESTS (above) needs the prior run's directory on the VM and compares a wider manifest; it is refused here so there is one reuse rule. Any difference runs the full suites
# as plan 54 did. The tools tests (python AND bash) run either way. A rehearsal runs the tools tests only and records what the engine suites would have done.
REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref56-manifest.sha256}"
engine_manifest() {
  ( cd "$(cd "$HERE/../.." && pwd)" || exit 1
    find raptorpath/src raptorpath/tests -type f | LC_ALL=C sort | xargs -r sha256sum | sed -E 's/^([0-9a-f]{64}) [ *]/\1  /' )
}
suites_stage56() {
  if [ "$P63" = 1 ]; then      # plan 63: no engine suites (the CAND tree carries the F2 commits, which ran their own suites); the tools tests (python and bash) run
    ENGINE_SUITES=0; ENGINE_WHY="(plan 63: no engine suites; the tools tests run)"
    run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
    return
  fi
  if [ "$P51" = 1 ]; then      # plan 51: main at launch, no engine suites ever; the tools tests (python and bash) run, the dial pins run after the build
    ENGINE_SUITES=0; ENGINE_WHY="(plan 51: main at launch, no engine change; the dial pins run on the built tree)"
    run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
    return
  fi
  [ -z "${LD_PRIOR_TESTS:-}" ] || fail_all "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest (LD_REF_MANIFEST), not by LD_PRIOR_TESTS"
  [ -s "$REF_MANIFEST" ] || fail_all "ABORT-TESTS no reference manifest at $REF_MANIFEST"
  cp "$REF_MANIFEST" "$SD/REF-MANIFEST.sha256"
  engine_manifest > "$SD/ENGINE-MANIFEST.now"
  if cmp -s "$SD/REF-MANIFEST.sha256" "$SD/ENGINE-MANIFEST.now"; then
    ENGINE_WHY="(raptorpath/src + raptorpath/tests byte-identical to the reference manifest $REF_MANIFEST, $(wc -l < "$SD/ENGINE-MANIFEST.now") files)"
    era "LD-ALL suite-reuse HIT: engine suites skipped $ENGINE_WHY"; ENGINE_SUITES=0
  else
    era "LD-ALL suite-reuse MISS: raptorpath/src + raptorpath/tests differ from the reference manifest ($(diff "$SD/REF-MANIFEST.sha256" "$SD/ENGINE-MANIFEST.now" | grep -c '^[<>]') manifest lines): $([ "$REHEARSE" = 1 ] && echo "a rehearsal runs no engine suites, the real run would" || echo "running") the full suites as plan 54 did"
    # Plans 60a / 60b (section 7 and 9 item 1): the suites ran in 60-pre on the pinned tree; a MISS means this is not that tree, so the launch STOPS before any row (a rehearsal included).
    [ "$P60" != 1 ] || fail_all "ABORT-SUITES plan $PLAN suite-reuse MISS: this tree is not the pinned tree of the reference manifest $REF_MANIFEST; NOTHING WAS RUN"
    [ "$P51E" != 1 ] || fail_all "ABORT-SUITES plan $PLAN suite-reuse MISS: this tree is not E-pre's tree (reference manifest $REF_MANIFEST); NOTHING WAS RUN"
    ENGINE_SUITES=1
  fi
  if [ "$REHEARSE" = 1 ]; then ENGINE_SUITES=0; ENGINE_WHY="(rehearsal: no engine suites; tools tests only)"; fi
  run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
}
# Plan 64's rehearsal step (registration 3.1 item 1, section 5 item 7): framing_ns on CAND's tree, release, 10 runs into bench/framing_ns.txt. The test binary is BUILT first (its own fresh target dir, both locks held);
# only the runs are pinned to core 2 (`taskset -c 2`), so the build is not. framing64.py prints the per-point median of CAND - BASE with its range and the BENCH PASS / BENCH OVER label (a merge condition, not a launch gate).
# A failure of the step itself (build, a run, nothing parsed) is ABORT-BENCH: the rehearsal cannot record the label and is re-run.
framing_bench() {
  local BD="$RUN/bench" CS="$ROOT/src" BT="$ROOT/target-bench" i
  mkdir -p "$BD"
  [ ! -e "$BT" ] || fail_all "ABORT-BENCH $BT is not fresh"
  ( cd "$CS" && CARGO_TARGET_DIR="$BT" CARGO_BUILD_JOBS=4 timeout 3600 "$CARGO" test -p raptorpath --lib --release --no-run ) > "$BD/build.log" 2>&1 || fail_all "ABORT-BENCH build ($BD/build.log)"
  : > "$BD/framing_ns.txt"
  for i in 1 2 3 4 5 6 7 8 9 10; do
    echo "--- run $i" >> "$BD/framing_ns.txt"
    ( cd "$CS" && CARGO_TARGET_DIR="$BT" taskset -c 2 timeout 900 "$CARGO" test -p raptorpath --lib --release framing_ns -- --ignored --nocapture ) >> "$BD/framing_ns.txt" 2>> "$BD/framing_ns.err" \
      || fail_all "ABORT-BENCH run $i ($BD/framing_ns.err)"
  done
  python3 ./framing64.py "$BD/framing_ns.txt" > "$BD/framing-report.txt" 2>&1 || { cat "$BD/framing-report.txt"; fail_all "ABORT-BENCH nothing parsed from $BD/framing_ns.txt"; }
  cat "$BD/framing-report.txt"
  era "LD-ALL framing_ns (10 runs, CAND's tree, taskset -c 2): $(tail -1 "$BD/framing-report.txt")"
}
# Plan 67 (status 67): the session envelope (seats, pins, binaries, identity, suite reuse, tools tests, the stages, the verdicts); it exits, it never falls through to the plans below.
[ "$P67" != 1 ] || source "$HERE/loopdiag_p67.sh"
# Plans 46 / 41c with LD_P46_SUITES=1 (status 49): after run_suites is defined (it was called above its definition before).
if [ "$P46" = 1 ] && [ "$P65" != 1 ] && [ "$PLAN" != "47" ] && [ "$PLAN" != "48" ] && [ "$PLAN" != "52" ] && [ "$PLAN" != "53" ] && [ "$PLAN" != "54b" ] && [ "$PLAN" != "56" ] && [ "$P57" != 1 ] && [ "$PLAN" != "41d" ] && { [ "${LD_P46_SUITES:-0}" = "1" ] || [ -n "${LD_PRIOR_TESTS:-}" ]; }; then
  suites_stage
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
fi
if [ "$PLAN" = "41" ]; then
  run_suites || fail_all "ABORT-TESTS (TESTS.txt)"
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
fi
if [ "$PLAN" = "34" ]; then
  # Amendment 2: a relaunch on the same engine tree may reuse a green suite run of that
  # tree (LD_PRIOR_TESTS = its run dir, which must hold TESTS-OK); it is copied in and named.
  if [ -n "${LD_PRIOR_TESTS:-}" ]; then
    [ -e "$LD_PRIOR_TESTS/TESTS-OK" ] || fail_all "ABORT-TESTS (no TESTS-OK in $LD_PRIOR_TESTS)"
    cp "$LD_PRIOR_TESTS/TESTS.txt" "$RUN/TESTS-prior.txt" && touch "$RUN/TESTS-OK"
    era "LD-ALL suites: reused TESTS-OK from $LD_PRIOR_TESTS (same engine tree)"
  else
    run_suites || fail_all "ABORT-TESTS (TESTS.txt)"
  fi
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
fi

# ── plan 47: suites on CAND's tree, then the guard (41c), then the path-event test (46); one ledger subdir each ──────────────────
if [ "$PLAN" = "56" ] || [ "$P57" = 1 ] || [ "$PLAN" = "41d" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || [ "$PLAN" = "52" ] || [ "$PLAN" = "53" ] || [ "$PLAN" = "54b" ] || [ "$P65" = 1 ]; then
  SD="$RUN/suites"; mkdir -p "$SD"
  suites_stage
  era "LD-ALL suites ok; before the $({ [ "$PLAN" = "48" ] || [ "$PLAN" = "53" ]; } && echo smoke || echo guard) $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
  stage() { # name plan: smoke -> battery -> score -> check, all in $RUN/<name>; rc 0 iff the stage ran to the end
    local name="$1" splan="$2" dir="$RUN/$1" smk scr chk bto brc ok=0
    [ "$name" = "guard" ] && [ "$splan" = "41c" ] && splan="$GUARD_PLAN"      # LD_GUARD_SET=v2 (plans 47 / 52): the guard stage is plan 41d's
    case "$splan" in
      41c) smk=smoke41c; scr=score41c; chk=check41c ;;
      41d) smk=smoke41d; scr=score41d; chk=check41d
           [ "$W54" = 1 ] && { smk=smoke54; scr=score54; }      # plan 54 = plan 41d + the witnesses: its own smoke criteria; score54 prints score41d's output first
           [ "$W64" = 1 ] && { smk=smoke64; scr=score64; } ;;     # plan 64 = plan 41d + the F2 row witnesses: smoke64 adds the F2 criteria; score64 = score41d's output + the F2 witness table + GATE-64
      64) smk=smoke64r; scr=score64r; chk=check64r ;;             # plan 64's stage R (battery LD_PLAN=64): Realtime c2-100 at rho = 1, arms RB / RC; the replacement rule is the battery's (run_slot64)
      48) smk=smoke48; scr=score48; chk=check48 ;;
      52) smk=smoke52; scr=score52; chk=check52 ;;
      53) smk=smoke53; scr=score53; chk=check53 ;;
      54b) smk=smoke54b; scr=score54b; chk=check54b ;;
      56) smk=smoke56; scr=score56; chk=check56 ;;
      57a) smk=smoke57a; scr=score57a; chk=check57a ;;
      57b) smk=smoke57b; scr=score57b; chk=check57b ;;
      59) smk=smoke59; scr=score59; chk=check59 ;;
      60a|60b) smk=smoke60; scr=score60; chk=check60 ;;
      61a|61b) smk=smoke61; scr=score61; chk=check61 ;;
      62r0|62a) smk=smoke62; scr=score62; chk=check62 ;;
      63) smk=smoke63; scr=score63; chk=check63 ;;
      46) smk=smoke46; scr=score46; chk=check46 ;;
      65) smk=smoke65r; scr=score65r; chk=check65r ;;      # plan 65's stage R (battery LD_PLAN=65): plan 64's stage R at 24 pairs, scored by the registered s65_score.py
    esac
    mkdir -p "$dir"
    # LD_BASE_SHA rides explicitly: `sudo -n env` resets the environment, and the row parser reads it (status 49). LD_WITNESS54 rides the same way (plan 54's witnesses; the parser and the battery read it).
    local SENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$dir" LD_PLAN="$splan" LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE" LD_BASE_SHA="${LD_BASE_SHA:-}" LD_WITNESS54="$W54" LD_WITNESS64="$([ "$splan" = "41d" ] && echo "$W64" || echo 0)" LD_REHEARSE="$REHEARSE" LD_NOGO="${LD_NOGO:-}" LD_KNEE61="${LD_KNEE61:-}" LD_CTL61="${LD_CTL61:-}" LD_KNEE62="${LD_KNEE62:-}" LD_CTL62="${LD_CTL62:-}" LD_HARD="${LD_HARD:-}" LD_CELL="${LD_CELL:-}" LD_ONLY="${LD_ONLY:-}" LD_BLOCK_ROW_S="${LD_BLOCK_ROW_S:-}")
    era "LD-ALL stage $name (plan $splan) smoke; $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
    sudo -n env "${SENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./loopdiag_battery.sh
    local sbrc=$? P60SMOKE="$splan"
    [ "$P60" = 1 ] && [ "$REHEARSE" = 1 ] && [ "$splan" = "60a" ] && P60SMOKE=60r      # the rehearsal: the 10 rows and the go/no-go
    [ "$P60" = 1 ] && [ "$REHEARSE" = 1 ] && [ "$splan" = "60b" ] && P60SMOKE=60br      # 60b's rehearsal: its smoke (no evicting row since status 60 Amendment 2)
    local P61SMOKE="$splan"; [ "$P61" = 1 ] && [ "$REHEARSE" = 1 ] && P61SMOKE=61r      # the rehearsal: the 6 rows R1-R6, the go/no-go and PILOT61
    local P51SMOKE=smoke      # plans 51 / 51e: the rehearsal runs the registered rehearsal set; LD_ONLY (51e) is the PL-only recheck
    if [ "$REHEARSE" = 1 ]; then P51SMOKE=rehearsal; [ -n "${LD_ONLY:-}" ] && P51SMOKE=recheck; fi
    if [ "$P63" = 1 ]; then python3 ./plan63.py smoke "$P51SMOKE" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1
    elif [ "$P51E" = 1 ]; then python3 ./s51e_score.py smoke51e "$P51SMOKE" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1
    elif [ "$P51" = 1 ]; then python3 ./s51_score.py smoke51 "$P51SMOKE" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1
    elif [ "$P61" = 1 ]; then python3 ./s61_score.py smoke61 "$P61SMOKE" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; elif [ "$P62" = 1 ]; then python3 ./s62_score.py smoke62 "$splan" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; elif [ "$P60" = 1 ]; then python3 ./s60_score.py smoke60 "$P60SMOKE" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; else python3 ./loopdiag_parse.py "$smk" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; fi
    local smrc=$?
    if [ "$sbrc" != "0" ] || [ "$smrc" != "0" ]; then
      cat "$dir/smoke-check.txt" 2>/dev/null
      touch "$dir/ABORT-SMOKE"
      if grep -q '^ABORT-INJECT' "$dir/smoke-check.txt" 2>/dev/null; then era "ABORT-INJECT stage $name: a check row failed; no scored row ran"
      else era "ABORT-SMOKE stage $name (smoke-check.txt)"; fi
      grep -q '^NO-GO65' "$dir/smoke-check.txt" 2>/dev/null && era "NO-GO65 plan 65 rehearsal: two BASE-F2-DEAD rows (RB s42 r1 and RB s7 r1): NOT a harness fault, the two dead rows go to the coordinator; nothing is launched"
      return 1
    fi
    cat "$dir/smoke-check.txt"; touch "$dir/SMOKE-PASS"; era "LD-ALL stage $name SMOKE-PASS"
    [ "$P61" = 1 ] && [ "$REHEARSE" = 1 ] && era "REHEARSAL61 $(grep -aE '^(PILOT61|GO-NOGO |NOGO-CONTROL)' "$dir/smoke-check.txt" | tr '\n' ';')"
    [ "$P62" = 1 ] && [ "$REHEARSE" = 1 ] && era "REHEARSAL62 $(grep -aE '^(PILOT62|LABEL62 GO-c3j0|NO-GO-LIST)' "$dir/smoke-check.txt" | tr '\n' ';')"
    if [ "$REHEARSE" = 1 ]; then touch "$dir/REHEARSAL-PASS"; era "LD-ALL REHEARSAL-PASS stage $name (smoke only, no battery)"; return 2; fi
    bto=$(( LD_HARD - $(date +%s) ))
    if [ "$bto" -lt 1800 ]; then era "ABORT-BUDGET stage $name: ${bto}s left before LD_HARD (need 1800)"; return 1; fi
    local T1; T1=$(date +%s)
    timeout "$bto" sudo -n env "${SENV[@]}" LD_TAG=ld bash ./loopdiag_battery.sh
    brc=$?
    era "LD-ALL stage $name battery rc=$brc wall=$(( $(date +%s) - T1 ))s"
    if { [ "$splan" = "41c" ] || [ "$splan" = "48" ] || [ "$splan" = "41d" ]; } && [ "$brc" = "0" ]; then
      # The replacement rule (status 47): a slot voided by a witness (not DNF, not crash) is re-run once with the same (cell, arm, seed, rep); at most 4.
      python3 ./place38_parse.py replacements41c "$dir/ld.log" > "$dir/repl-slots.txt" 2> "$dir/repl-slots.err"
      local rrc=$?
      if [ "$rrc" = "3" ]; then
        era "LD-ALL guard: TOO-MANY voided slots, no replacement runs; the guard reads NO VERDICT ($(cat "$dir/repl-slots.err"))"
      elif [ "$rrc" = "0" ] && [ -s "$dir/repl-slots.txt" ]; then
        bto=$(( LD_HARD - $(date +%s) ))
        if [ "$bto" -ge 900 ]; then
          era "LD-ALL guard: re-running $(wc -l < "$dir/repl-slots.txt") voided slot(s) once: $(tr '\n' ';' < "$dir/repl-slots.txt")"
          timeout "$bto" sudo -n env "${SENV[@]}" LD_REPL_FILE="$dir/repl-slots.txt" LD_TAG=repl bash ./loopdiag_battery.sh
          era "LD-ALL guard: replacement run rc=$?"
        else
          era "ABORT-BUDGET guard replacement: ${bto}s left before LD_HARD (need 900)"
        fi
      fi
    fi
    if [ "$splan" = "46" ]; then python3 ./loopdiag_parse.py "$scr" "$dir/smoke.log" "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$splan" = "41c" ] || [ "$splan" = "48" ] || [ "$splan" = "41d" ]; then python3 ./loopdiag_parse.py "$scr" "$dir/ld.log" "$dir/repl.log" > "$dir/score.txt" 2>&1
    elif [ "$P63" = 1 ]; then python3 ./plan63.py report "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$P51E" = 1 ]; then python3 ./s51e_score.py score51e "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$P51" = 1 ]; then python3 ./s51_score.py score51 "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$P61" = 1 ]; then python3 ./s61_score.py score61 "$splan" "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$P62" = 1 ]; then python3 ./s62_score.py score62 "$splan" "$dir/ld.log" > "$dir/score.txt" 2>&1
    elif [ "$P60" = 1 ]; then python3 ./s60_score.py score60 "$splan" "$dir/ld.log" > "$dir/score.txt" 2>&1
    else python3 ./loopdiag_parse.py "$scr" "$dir/ld.log" > "$dir/score.txt" 2>&1; fi
    if [ "$P63" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan63.py check "$splan" "$dir/ld.log" && ok=1
    elif [ "$P51E" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan51e.py check "$splan" "$dir/ld.log" && ok=1
    elif [ "$P51" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan51.py check "$splan" "$dir/ld.log" && ok=1
    elif [ "$P61" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan61.py check "$splan" "$dir/ld.log" && ok=1
    elif [ "$P62" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan62.py check "$splan" "$dir/ld.log" && ok=1
    elif [ "$P60" = 1 ]; then [ "$brc" = "0" ] && python3 ./plan60.py check "$splan" "$dir/ld.log" && ok=1
    else [ "$brc" = "0" ] && python3 ./loopdiag_parse.py "$chk" "$dir/ld.log" && ok=1; fi
    if [ "$splan" = "41c" ] || [ "$splan" = "48" ] || [ "$splan" = "41d" ]; then
      # The guard step is scored by score41c / score48 (rank-minp, the design mandatory), never by 41b's report, and its ledger must hold the 96 rows of the design.
      local nrows; nrows=$(grep -ac '^P38ROW ' "$dir/ld.log")
      if [ "$nrows" != "96" ]; then era "ABORT-ROWS stage $name: $nrows P38ROW lines, the guard's design is 96"; ok=0; fi
    fi
    era "LD-ALL stage $name end ok=$ok $(grep -a '^VERDICT' "$dir/score.txt" | head -1)"
    if [ "$splan" = "41c" ] || [ "$splan" = "48" ] || [ "$splan" = "41d" ]; then
      # Only the verdict is computed on the VM. The resolution table (reported only) is recomputed on the HOST from the kept ledger.
      era "LD-ALL guard resolution NOT computed on the VM; on the host: python3 tools/l1/place38_parse.py resolution41$([ "$splan" = "41d" ] && echo d || echo c) <pulled run dir>/$name/ld.log"
    fi
    [ "$ok" = "1" ] && touch "$dir/DONE" && return 0
    return 1
  }
  if [ "$P65" = 1 ]; then
    # Status 65: stage R only (rt65), nothing before it but the checks above; a rehearsal stops after the smoke.
    ROK=0
    proclist65 "before-first-row-$([ "$REHEARSE" = 1 ] && echo rehearsal || echo launch)"
    stage rt65 65; rrc=$?
    if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$rrc" = "0" ] && ROK=1
    { echo "THE EXACT REPLICATION (status 65): suites/TESTS.txt, rt65/score.txt"; grep -aE '^(VERDICT65|TEST65|F2-FINDING65)' "$RUN/rt65/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 65 rt65_ok=$ROK $(grep -a '^VERDICT65' "$RUN/rt65/score.txt" | head -1 | cut -c1-80) raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rt65 did not complete (rt65/)"
  fi
  if [ "$PLAN" = "48" ]; then
    BOK=0
    stage bbr48 48; src=$?
    if [ "$src" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$src" = "0" ] && BOK=1
    { echo "BBR_RS BATTERY (status 48b): suites/TESTS.txt, bbr48/score.txt"; grep -a '^VERDICT' "$RUN/bbr48/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 48 ok=$BOK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$BOK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage bbr48 did not complete (bbr48/)"
  fi
  if [ "$PLAN" = "41d" ]; then
    # Status 41d: the merge guard v2 alone (suites on CAND's tree, then the one scored stage; c2 / c8 rows at 600 MB, score41d, GATE-41d).
    GOK=0; ROK=1
    [ "$W64" = 1 ] && [ "$REHEARSE" = 1 ] && framing_bench      # plan 64's rehearsal: framing_ns on CAND's tree (the label is a merge condition)
    stage guard 41d; grc=$?
    if [ "$grc" = "2" ] && [ "$W64" = 1 ]; then        # plan 64's rehearsal: stage G's smoke passed; stage R's smoke (RB s42, RC s42) is the second
      stage rt64 64; rrc=$?
      if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
      fail_all "rehearsal: stage rt64 smoke did not pass (rt64/)"
    fi
    if [ "$grc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$grc" = "0" ] && GOK=1
    if [ "$W64" = 1 ]; then
      # Plan 64: stage R after stage G, in its own ledger (plan 52's pattern); DONE-ALL needs both stages to run to the end. A stage G that did not run to the end stops the session.
      [ "$GOK" = "1" ] || fail_all "stage guard did not complete (guard/)"
      ROK=0; stage rt64 64 && ROK=1
      { echo "F2 MERGE GUARD (status 64): suites/TESTS.txt, guard/score.txt (stage G), rt64/score.txt (stage R)"; echo "== guard"; grep -aE '^(VERDICT|GATE-64)' "$RUN/guard/score.txt"; echo "== rt64"; grep -aE '^(VERDICT|RT-)' "$RUN/rt64/score.txt"; } > "$RUN/score.txt"
      era "LD-ALL end plan 64 guard_ok=$GOK rt64_ok=$ROK $(grep -a '^GATE-64' "$RUN/guard/score.txt" | head -1) $(grep -a '^VERDICT64R' "$RUN/rt64/score.txt" | head -1) raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
      if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
      fail_all "stage rt64 did not complete (rt64/)"
    fi
    { echo "MERGE GUARD v2 (status 41d): suites/TESTS.txt, guard/score.txt"; grep -a '^VERDICT' "$RUN/guard/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 41d$([ "$W54" = 1 ] && echo " (LD_PLAN=54: witnesses on)") guard_ok=$GOK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$GOK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage guard did not complete (guard/)"
  fi
  if [ "$PLAN" = "53" ]; then
    # No guard: one reported-only stage in its own ledger dir (smoke with the rehearsal criteria -> battery -> score -> count check).
    ROK=0
    stage rep53 53; rrc=$?
    if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$rrc" = "0" ] && ROK=1
    { echo "STALE GAP REPORTS SESSION (status 53): suites/TESTS.txt, rep53/score.txt (REPORTED ONLY)"; grep -a '^VERDICT' "$RUN/rep53/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 53 rep53_ok=$ROK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rep53 did not complete (rep53/)"
  fi
  if [ "$PLAN" = "54b" ]; then
    # Status 54, the store-budget block: no guard; one reported-only stage in its own ledger dir (smoke with the rehearsal criteria -> 36 rows -> score -> count check).
    ROK=0
    stage rep54b 54b; rrc=$?
    if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$rrc" = "0" ] && ROK=1
    { echo "STORE-BUDGET BLOCK (status 54): suites/TESTS.txt, rep54b/score.txt (REPORTED ONLY)"; grep -a '^VERDICT' "$RUN/rep54b/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 54b rep54b_ok=$ROK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rep54b did not complete (rep54b/)"
  fi
  if [ "$P57" = 1 ]; then
    # Status 57a / 57b: no guard, one seat; ONE reported-only stage in its own ledger dir (smoke with the rehearsal criteria -> the slots -> score -> slot check).
    ROK=0
    case "$PLAN" in 57a) stage rep57a 57a; rrc=$? ;; 59) stage rep59 59; rrc=$? ;; 60a) stage rep60a 60a; rrc=$? ;; 60b) stage rep60b 60b; rrc=$? ;; 61a) stage rep61a 61a; rrc=$? ;; 61b) stage rep61b 61b; rrc=$? ;; 62r0) stage rep62r0 62r0; rrc=$? ;; 62a) stage rep62a 62a; rrc=$? ;; 51) stage rep51 51; rrc=$? ;; 63) stage rep63 63; rrc=$? ;; 51e) stage rep51e 51e; rrc=$? ;; *) stage rep57b 57b; rrc=$? ;; esac
    if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$rrc" = "0" ] && ROK=1
    { echo "STATUS $PLAN (REPORTED ONLY): suites/TESTS.txt, rep$PLAN/score.txt"; grep -a '^VERDICT' "$RUN/rep$PLAN/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan $PLAN rep${PLAN}_ok=$ROK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rep$PLAN did not complete (rep$PLAN/)"
  fi
  if [ "$PLAN" = "56" ]; then
    # Status 56, the store-pool sweep: no guard, one seat; ONE reported-only stage in its own ledger dir (smoke with the rehearsal criteria -> 70 slots -> score -> slot check).
    ROK=0
    stage rep56 56; rrc=$?
    if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$rrc" = "0" ] && ROK=1
    { echo "STORE-POOL SWEEP (status 56): suites/TESTS.txt, rep56/score.txt (REPORTED ONLY)"; grep -a '^VERDICT' "$RUN/rep56/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 56 rep56_ok=$ROK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rep56 did not complete (rep56/)"
  fi
  if [ "$PLAN" = "52" ]; then
    # The guard (scored; plan 41c unchanged unless LD_GUARD_SET=v2) then the reported block in its own stage dir; the block never touches the guard's ledger or verdict.
    GOK=0; ROK=0
    stage guard 41c; grc=$?
    if [ "$grc" = "2" ]; then        # rehearsal: the guard's smoke passed; the reported block's smoke is the second
      stage rep52 52; rrc=$?
      if [ "$rrc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
      fail_all "rehearsal: stage rep52 smoke did not pass (rep52/)"
    fi
    [ "$grc" = "0" ] && GOK=1
    [ "$GOK" = "1" ] || fail_all "stage guard did not complete (guard/)"
    stage rep52 52 && ROK=1
    { echo "C8 RECOVERY LEVERS SESSION (status 52): suites/TESTS.txt, guard/score.txt (scored), rep52/score.txt (REPORTED ONLY)"; echo "== guard"; grep -a '^VERDICT' "$RUN/guard/score.txt"; echo "== rep52"; grep -a '^VERDICT' "$RUN/rep52/score.txt"; } > "$RUN/score.txt"
    era "LD-ALL end plan 52 guard_ok=$GOK rep52_ok=$ROK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
    if [ "$ROK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
    fail_all "stage rep52 did not complete (rep52/)"
  fi
  GOK=0; EOK=0
  stage guard 41c; grc=$?
  if [ "$grc" = "2" ]; then        # rehearsal: the guard's smoke passed; the path-event test's check rows are the second smoke
    stage p46 46; prc=$?
    if [ "$prc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    fail_all "rehearsal: stage p46 smoke did not pass (p46/)"
  fi
  [ "$grc" = "0" ] && GOK=1
  [ "$GOK" = "1" ] || fail_all "stage guard did not complete (guard/)"
  stage p46 46 && EOK=1
  { echo "MERGE-CANDIDATE SESSION (status 47): suites/TESTS.txt, guard/score.txt, p46/score.txt"; for f in guard p46; do echo "== $f"; grep -a '^VERDICT' "$RUN/$f/score.txt"; done; } > "$RUN/score.txt"
  era "LD-ALL end plan 47 guard_ok=$GOK p46_ok=$EOK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
  if [ "$EOK" = "1" ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
  fail_all "stage p46 did not complete (p46/)"
fi

BENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$RUN" LD_PLAN="$PLAN" LD_BASE_SHA="${LD_BASE_SHA:-}")
[ "$P41" = 1 ] && BENV+=(LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE")
sudo -n env "${BENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./loopdiag_battery.sh
SRC_RC=$?
if [ "$SRC_RC" != "0" ] || ! python3 ./loopdiag_parse.py "$P_SMOKE" "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  grep -q '^ABORT-INJECT' "$RUN/smoke-check.txt" 2>/dev/null && fail_all "ABORT-INJECT (smoke-check.txt: a check row failed; no scored row ran)"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"
era "LD-ALL SMOKE-PASS"
if [ "$REHEARSE" = 1 ]; then touch "$RUN/REHEARSAL-PASS"; era "LD-ALL REHEARSAL-PASS (smoke only, no battery)"; persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi

T0=$(date +%s)
era "LD-ALL battery start load=$(cut -d' ' -f1-3 "$PROCFS"/loadavg); io=$(grep '^full' "$PROCFS"/pressure/io)"
BTO=10800
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  BTO=$(( LD_HARD - $(date +%s) ))
  MINBTO=1800
  # status 43: not started with less than 45 min left. Amendment 2 (26 invocations): about 35 min expected, worst case
  # 26 x (200 s timeout + ~15 s bring-up) = 93 min, which LD_HARD (launch + 4 h) covers; the guard is unchanged.
  [ "$P43" = 1 ] && MINBTO=2700
  [ "$BTO" -ge "$MINBTO" ] || fail_all "ABORT-BUDGET ${BTO}s left before LD_HARD (need $MINBTO)"
fi
timeout "$BTO" sudo -n env "${BENV[@]}" LD_TAG=ld bash ./loopdiag_battery.sh
BRC=$?
era "LD-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
if [ "$PLAN" = "46" ]; then
  python3 ./loopdiag_parse.py "$P_SCORE" "$RUN/smoke.log" "$RUN/ld.log" > "$RUN/score.txt" 2>&1
else
  python3 ./loopdiag_parse.py "$P_SCORE" "$RUN/ld.log" > "$RUN/score.txt" 2>&1
fi
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./loopdiag_parse.py "$P_CHECK" "$RUN/ld.log" && BAT_OK=1
era "LD-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 "$PROCFS"/loadavg); io=$(grep '^full' "$PROCFS"/pressure/io)"

if [ "$PLAN" = "27" ]; then
  persist
  run_suites || true
fi

if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
