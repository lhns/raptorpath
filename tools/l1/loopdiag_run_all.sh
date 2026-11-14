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
# LD_PLAN=54b (status 54, the store-budget block, REPORTED ONLY): the suites on CAND's tree (levb54), then ONE stage in $RUN/rep54b: c8-100 at 600 MB, arms B2048 / B1024 / B640 on the BASE
# seat (LD_BASE_SHA = cand54) and L2048 / L1024 / L640 on the CAND seat, RWM_STORE_PATH_POOL = default / 1024 / 640, n = 6, rotated per group, 36 rows + a 6-row smoke (pool54_score.py).
# LD_PLAN=56 (status 56, the store-pool sweep, REPORTED ONLY, no build): ONE seat -- current main (LD_CAND_SHA = $LD_ROOT/COMMIT) runs every row; there is no BASE tree, no second build, no LD_BASE_SHA (ignored with a
# note). Pre-flight: >= 1 GB free on /dev/shm (the RWM_SUCC_DUMP row). Suites: the engine suites are skipped only if a per-file sha256 manifest of raptorpath/src + raptorpath/tests equals tools/l1/ref56-manifest.sha256
# (git at cand54 7fd92dd5; regenerate with ref_manifest.sh; LD_REF_MANIFEST overrides), else the full suites run as plan 54 did; the tools tests (python and bash) always run; LD_PRIOR_TESTS is refused. Then ONE stage
# in $RUN/rep56: smoke = 4 rows (c8 P80, c2 P128, c1s P384, the c8 P640D dump row), then 70 slots (c8 6 pools x 4, c2 6 x 3, c1s 2048 / 512 x 9 and 1024 / 768 / 384 x 3, the dump row), RWM_PERF_TIMEOUT_S=300;
# a VOID pool echo and a dump under 100 000 records are re-run once (pool56_score.py; LD_REHEARSE=1 and LD_HARD as plan 54b).
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
crlf_guard lib.sh lib_battery.sh loopdiag_run_all.sh loopdiag_battery.sh loopdiag_wait34.sh loopdiag_parse.py anchordiag_parse.py disp36.py test_disp36.py disp40.py test_disp40.py place38_parse.py test_place38_parse.py place41b_score.py bbr48_score.py test_bbr48_parse.py tail52_score.py test_tail52.py snap53_score.py test_snap53.py pool54_score.py test_pool54.py pool56_score.py test_pool56.py ref_manifest.sh ref56-manifest.sha256 place43_parse.py test_place43_parse.py place46_parse.py event_inject.py test_harness46.py test_harness47.py test_guard_v2.py test_topo_netem.sh \
    test_loopdiag_parse.py test_anchordiag_parse.py stage3_parse.py storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${LD_ROOT:?LD_ROOT}"
PLAN="${LD_PLAN:-24}"
# Plan 54 (status 54, the snapshot-order law's guard) IS plan 41d, unchanged, plus three row-level liveness witnesses (pool54_score.guard_witness): from here on PLAN is 41d and
# W54=1 rides in SENV, so no 41d branch can be missed. The scored rows, order, sizes, timeouts and verdict are plan 41d's; score54 prints score41d's output first.
W54=0; [ "$PLAN" = "54" ] && { W54=1; PLAN=41d; }
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
  *) echo "ABORT-PLAN LD_PLAN=$PLAN (24|27|29|34|36|38|40|41|41b|41c|41d|43|43b|46|47|48|52|53|54|54b|56)"; exit 3 ;;
esac
# Test seams (test_stubrun.py runs this script end to end against stubs); on the VM none is set and the defaults are the real paths.
DEVSHM="${LD_DEVSHM:-/dev/shm}"; PROCFS="${LD_PROCFS:-/proc}"; LOCK_VM="${LD_LOCK_VM:-/tmp/rwm-vm.lock}"; LOCK_RP="${LD_LOCK_RP:-/home/vibe/rp.lock}"
# LD_REHEARSE=1 (plans 41c, 41d, 46, 47, 48, 52, 53, 54, 54b, 56; status 48b amendment 1): the launch path up to and including each smoke and its check (gates, locks, builds,
# binary pins, smoke rows), then stop: REHEARSAL-PASS / LD-ALL-REHEARSAL-DONE. No suites (unless LD_PRIOR_TESTS names a run to verify) and no battery.
REHEARSE="${LD_REHEARSE:-0}"
if [ "$REHEARSE" = 1 ]; then
  case "$PLAN" in 41c|41d|46|47|48|52|53|54b|56) ;; *) echo "ABORT-REHEARSE LD_REHEARSE=1 is defined for plans 41c|41d|46|47|48|52|53|54|54b|56 only, not $PLAN; NOTHING WAS RUN"; exit 3 ;; esac
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
# Plan 46 shares the root-owned tmpfs tree, LD_HARD and the two-tree build with plans 38 / 41 (P38 stays 0: no place-wire arms or ladder).
if [ "$P46" = 1 ]; then
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
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  : "${LD_HARD:?LD_HARD (epoch) is required for plan $PLAN}"
  PREC=$(prec_samples)
  if [ "${PREC%% *}" != 1 ] || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
    era "ABORT-PRECONDITION ${PREC#* }; NOTHING WAS RUN"; exit 3
  fi
  era "LD-ALL precondition met ${PREC#* } io=$(grep '^full' "$PROCFS"/pressure/io)"
fi
if [ "$P56" = 1 ]; then
  # Plan 56 (70 slots) is launched with LD_HARD = launch + 4 h: a reference-manifest hit needs about 2 h 05 typical and 3 h 20 worst case (every non-control c1s row at its 300 s timeout); a miss adds the
  # 44-54 min engine suites (about 2 h 50 typical, 4 h 10 in the double worst case). Under 3 h left cannot hold even a typical miss session, so it is refused before anything runs (LD_HARD_MIN_S is a test seam).
  P56_LEFT=$(( LD_HARD - $(date +%s) ))
  era "LD-ALL plan 56 budget: ${P56_LEFT}s to LD_HARD (launch + 4 h expected; at least ${LD_HARD_MIN_S:-10800}s required)"
  [ "$P56_LEFT" -ge "${LD_HARD_MIN_S:-10800}" ] || { era "ABORT-BUDGET plan 56: ${P56_LEFT}s before LD_HARD, need at least ${LD_HARD_MIN_S:-10800}s; NOTHING WAS RUN"; exit 3; }
fi
take_lock "$LOCK_VM"
take_lock "$LOCK_RP"
era "LD-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 "$PROCFS"/loadavg); io=$(grep '^full' "$PROCFS"/pressure/io)"

if [ "$P56" = 1 ]; then
  # The raw successor dump (RWM_SUCC_DUMP) and the per-row logs live on tmpfs; the registered pre-flight is 1 GB free (LD_SHM_MIN_KB is a test seam).
  SHM_FREE_KB="$(df -Pk "$DEVSHM" 2>/dev/null | awk 'NR==2{print $4}')"
  era "LD-ALL plan 56 pre-flight: ${SHM_FREE_KB:-?} KB free on $DEVSHM (need ${LD_SHM_MIN_KB:-1048576})"
  { [[ "${SHM_FREE_KB:-}" =~ ^[0-9]+$ ]] && [ "$SHM_FREE_KB" -ge "${LD_SHM_MIN_KB:-1048576}" ]; } || fail_all "ABORT-SHM-SPACE $DEVSHM has ${SHM_FREE_KB:-?} KB free, plan 56 needs ${LD_SHM_MIN_KB:-1048576}; NOTHING WAS RUN"
  [ -z "${LD_BASE_SHA:-}" ] || era "LD-ALL plan 56 is single-seat: LD_BASE_SHA=${LD_BASE_SHA} is ignored (no BASE tree, no second build)"
fi
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
era "LD-ALL parser test ok"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
[ -d "$SRC" ] || fail_all "ABORT-BUILD no $SRC"
[ ! -e "$SRC/target" ] || fail_all "ABORT-BUILD $SRC/target is not fresh"
[ ! -e "$TARGET" ] || fail_all "ABORT-BUILD $TARGET is not fresh"
T0=$(date +%s)
( cd "$SRC" && CARGO_TARGET_DIR="$TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$BUILDLOG" 2>&1 \
  || fail_all "ABORT-BUILD ($BUILDLOG)"
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
SHA_BASE=""
if [ "$P41" = 1 ]; then
  # BASE: main's own binary, its own archive tree and its own fresh target dir (never shared with the shipped tree's).
  [ -d "$BASE_SRC" ] || fail_all "ABORT-BUILD no $BASE_SRC"
  [ ! -e "$BASE_SRC/target" ] || fail_all "ABORT-BUILD $BASE_SRC/target is not fresh"
  [ ! -e "$BASE_TARGET" ] || fail_all "ABORT-BUILD $BASE_TARGET is not fresh"
  # BASE's pin: LD_BASE_SHA (the full sha, recorded by amendment before launch) when set, else the historical main 4926822.
  BASE_PIN="${LD_BASE_SHA:-4926822}"
  case "$(cat "$ROOT/base/COMMIT" 2>/dev/null)" in "$BASE_PIN"*) ;; *) fail_all "ABORT-BUILD base COMMIT is not $BASE_PIN" ;; esac
  T1=$(date +%s)
  ( cd "$BASE_SRC" && CARGO_TARGET_DIR="$BASE_TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$ROOT/build-base.log" 2>&1 \
    || fail_all "ABORT-BUILD (base: $ROOT/build-base.log)"
  sudo -n install -m 0755 -o root -g root "$BASE_TARGET/release/raptorpath" "$BIN_BASE" || fail_all "ABORT-BUILD copy (base)"
  SHA_BASE="$(sha256sum "$BIN_BASE" | cut -d' ' -f1)"
  echo "$SHA_BASE  $BIN_BASE  commit=$(cat "$ROOT/base/COMMIT" 2>/dev/null) (BASE)" >> "$RUN/BINSHA.txt"
  era "LD-ALL built BASE wall=$(( $(date +%s) - T1 ))s binary $SHA_BASE"
  [ "$SHA_BASE" != "$SHA" ] || fail_all "ABORT-BUILD BASE and MAIN binaries are identical"
fi
if [ "$P46" = 1 ]; then
  grep -aq -- "RWM_PLACE_WIRE" "$BIN" && fail_all "ABORT-CAND the CAND binary carries RWM_PLACE_WIRE: it does not contain the promotion"
  [ "$P41" = 1 ] && grep -aq -- "RWM_PLACE_WIRE" "$BIN_BASE" && fail_all "ABORT-CAND the BASE binary carries RWM_PLACE_WIRE: it is not main's"
fi

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
    # The pre-registered flake rule: a failure that passes on an immediate solo
    # re-run of that test alone is FLAKE; any other is REAL.
    local real=0 t
    if ! grep -aq -E '^test .* FAILED$' "$SD/test-$name.log"; then
      echo "REAL-FAILURE $name rc=$rc with no failing test named (build error?)" >> "$SD/TESTS.txt"
      return 1
    fi
    for t in $(grep -a -E '^test .* FAILED$' "$SD/test-$name.log" | awk '{print $2}' | sort -u); do
      local base=("$@")
      local cmd=() a seen=0
      for a in "${base[@]}"; do [ "$a" = "--" ] && seen=1; [ "$seen" = "0" ] && cmd+=("$a"); done
      if ( cd "$REPO" && CARGO_TARGET_DIR="$TT" "${cmd[@]}" -- "$t" --exact --test-threads=1 ) > "$SD/test-$name-rerun.log" 2>&1 \
          && grep -aq "^test $t ... ok" "$SD/test-$name-rerun.log"; then
        echo "FLAKE $name $t (passed on the immediate solo re-run)" >> "$SD/TESTS.txt"
      else
        echo "REAL-FAILURE $name $t" >> "$SD/TESTS.txt"
        real=1
      fi
    done
    return $real
  }
  if [ "$ENGINE_SUITES" = 1 ]; then
    tst main timeout 7200 "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
    tst doc timeout 3600 "$CARGO" test --doc -p raptorpath --release || TFAIL=1
    tst wasm env -u GOLDEN_CAPTURE timeout 3600 "$CARGO" test -p raptorpath-wasm || TFAIL=1
    tst debug-witness timeout 7200 "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
  else
    echo "ENGINE-SUITES-REUSED $ENGINE_WHY" >> "$SD/TESTS.txt"
  fi
  if [ "$PLAN" = "56" ] || [ "$PLAN" = "41d" ] || [ "$PLAN" = "41" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || [ "$PLAN" = "52" ] || [ "$PLAN" = "53" ] || [ "$PLAN" = "54b" ] || { [ "$P46" = 1 ] && [ "${LD_P46_SUITES:-0}" = "1" ]; }; then
    # Status 41 / 47 / 48 / 49: "the tools/l1 python tests" are all of them, except test_stubrun.py: it re-runs this very script against stubs
    # (minutes, even in its fast mode) and belongs to harness changes only, never to a measurement session.
    tst parsers bash -c 'cd tools/l1 && for t in test_*.py; do [ "$t" = test_stubrun.py ] && continue; python3 "$t" || { echo "PYTHON-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1
    # Plan 56 (status 56): the two bash tools tests of section 54 (test_lib_battery.sh, test_abort_witness.sh) always run too (offline stubs: no root, no namespaces).
    [ "$PLAN" = "56" ] && { tst bashtests bash -c 'cd tools/l1 && for t in test_lib_battery.sh test_abort_witness.sh; do bash "$t" || { echo "BASH-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1; }
  else
    tst parsers bash -c "cd tools/l1 && python3 test_loopdiag_parse.py && python3 test_anchordiag_parse.py" || TFAIL=1
  fi
  [ "$ENGINE_SUITES" = 1 ] && echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$SD/test-debug-witness.log")" >> "$SD/TESTS.txt"
  # TESTS-OK and SRC-MANIFEST.sha256 are a SUITE RECORD that LD_PRIOR_TESTS (another plan's launch) accepts as "the engine suites passed on this tree": written only when they ran. A run that
  # skipped them (plan 56: a reference-manifest hit, or a rehearsal) leaves TOOLS-TESTS-OK instead.
  if [ "$ENGINE_SUITES" = 1 ]; then src_manifest > "$SD/SRC-MANIFEST.sha256"; fi       # what the run covered: LD_PRIOR_TESTS reuse compares against it
  if [ "$TFAIL" = "0" ]; then if [ "$ENGINE_SUITES" = 1 ]; then touch "$SD/TESTS-OK"; else touch "$SD/TOOLS-TESTS-OK"; fi; else touch "$SD/TESTS-FAIL"; fi
  era "LD-ALL suites done tfail=$TFAIL"
  return $TFAIL
}
# The identity of what the suites ran on: sha256 of every file under raptorpath/src, raptorpath/tests, raptorpath-math/src, raptorpath-wasm/src, gf256/src and vendor/
# (a patched dependency), plus the workspace Cargo.toml / Cargo.lock and every crate's Cargo.toml, in C-locale path order. A dependency patch invalidates a reuse.
src_manifest() {
  ( cd "$(cd "$HERE/../.." && pwd)" || exit 1
    { ls Cargo.toml Cargo.lock ./*/Cargo.toml 2>/dev/null | sed 's#^\./##'
      find raptorpath/src raptorpath/tests raptorpath-math/src raptorpath-wasm/src gf256/src vendor -type f 2>/dev/null; } | LC_ALL=C sort -u | xargs -r sha256sum )
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
# The suites step of the candidate plans: a verified reuse, else skipped by a rehearsal, else run.
suites_stage() {
  if [ "$PLAN" = "56" ]; then suites_stage56; return; fi      # plan 56 reuses by the reference manifest (below), never by LD_PRIOR_TESTS
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
  [ -z "${LD_PRIOR_TESTS:-}" ] || fail_all "ABORT-TESTS plan 56 reuses the engine suites by the reference manifest (LD_REF_MANIFEST), not by LD_PRIOR_TESTS"
  [ -s "$REF_MANIFEST" ] || fail_all "ABORT-TESTS no reference manifest at $REF_MANIFEST"
  cp "$REF_MANIFEST" "$SD/REF-MANIFEST.sha256"
  engine_manifest > "$SD/ENGINE-MANIFEST.now"
  if cmp -s "$SD/REF-MANIFEST.sha256" "$SD/ENGINE-MANIFEST.now"; then
    ENGINE_WHY="(raptorpath/src + raptorpath/tests byte-identical to the reference manifest $REF_MANIFEST, $(wc -l < "$SD/ENGINE-MANIFEST.now") files)"
    era "LD-ALL suite-reuse HIT: engine suites skipped $ENGINE_WHY"; ENGINE_SUITES=0
  else
    era "LD-ALL suite-reuse MISS: raptorpath/src + raptorpath/tests differ from the reference manifest ($(diff "$SD/REF-MANIFEST.sha256" "$SD/ENGINE-MANIFEST.now" | grep -c '^[<>]') manifest lines): $([ "$REHEARSE" = 1 ] && echo "a rehearsal runs no engine suites, the real run would" || echo "running") the full suites as plan 54 did"
    ENGINE_SUITES=1
  fi
  if [ "$REHEARSE" = 1 ]; then ENGINE_SUITES=0; ENGINE_WHY="(rehearsal: no engine suites; tools tests only)"; fi
  run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
}
# Plans 46 / 41c with LD_P46_SUITES=1 (status 49): after run_suites is defined (it was called above its definition before).
if [ "$P46" = 1 ] && [ "$PLAN" != "47" ] && [ "$PLAN" != "48" ] && [ "$PLAN" != "52" ] && [ "$PLAN" != "53" ] && [ "$PLAN" != "54b" ] && [ "$PLAN" != "56" ] && [ "$PLAN" != "41d" ] && { [ "${LD_P46_SUITES:-0}" = "1" ] || [ -n "${LD_PRIOR_TESTS:-}" ]; }; then
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
if [ "$PLAN" = "56" ] || [ "$PLAN" = "41d" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || [ "$PLAN" = "52" ] || [ "$PLAN" = "53" ] || [ "$PLAN" = "54b" ]; then
  SD="$RUN/suites"; mkdir -p "$SD"
  suites_stage
  era "LD-ALL suites ok; before the $({ [ "$PLAN" = "48" ] || [ "$PLAN" = "53" ]; } && echo smoke || echo guard) $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
  stage() { # name plan: smoke -> battery -> score -> check, all in $RUN/<name>; rc 0 iff the stage ran to the end
    local name="$1" splan="$2" dir="$RUN/$1" smk scr chk bto brc ok=0
    [ "$name" = "guard" ] && [ "$splan" = "41c" ] && splan="$GUARD_PLAN"      # LD_GUARD_SET=v2 (plans 47 / 52): the guard stage is plan 41d's
    case "$splan" in
      41c) smk=smoke41c; scr=score41c; chk=check41c ;;
      41d) smk=smoke41d; scr=score41d; chk=check41d
           [ "$W54" = 1 ] && { smk=smoke54; scr=score54; } ;;      # plan 54 = plan 41d + the witnesses: its own smoke criteria; score54 prints score41d's output first
      48) smk=smoke48; scr=score48; chk=check48 ;;
      52) smk=smoke52; scr=score52; chk=check52 ;;
      53) smk=smoke53; scr=score53; chk=check53 ;;
      54b) smk=smoke54b; scr=score54b; chk=check54b ;;
      56) smk=smoke56; scr=score56; chk=check56 ;;
      46) smk=smoke46; scr=score46; chk=check46 ;;
    esac
    mkdir -p "$dir"
    # LD_BASE_SHA rides explicitly: `sudo -n env` resets the environment, and the row parser reads it (status 49). LD_WITNESS54 rides the same way (plan 54's witnesses; the parser and the battery read it).
    local SENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$dir" LD_PLAN="$splan" LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE" LD_BASE_SHA="${LD_BASE_SHA:-}" LD_WITNESS54="$W54")
    era "LD-ALL stage $name (plan $splan) smoke; $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
    sudo -n env "${SENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./loopdiag_battery.sh
    if [ "$?" != "0" ] || ! python3 ./loopdiag_parse.py "$smk" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; then
      cat "$dir/smoke-check.txt" 2>/dev/null
      touch "$dir/ABORT-SMOKE"
      if grep -q '^ABORT-INJECT' "$dir/smoke-check.txt" 2>/dev/null; then era "ABORT-INJECT stage $name: a check row failed; no scored row ran"
      else era "ABORT-SMOKE stage $name (smoke-check.txt)"; fi
      return 1
    fi
    cat "$dir/smoke-check.txt"; touch "$dir/SMOKE-PASS"; era "LD-ALL stage $name SMOKE-PASS"
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
    else python3 ./loopdiag_parse.py "$scr" "$dir/ld.log" > "$dir/score.txt" 2>&1; fi
    [ "$brc" = "0" ] && python3 ./loopdiag_parse.py "$chk" "$dir/ld.log" && ok=1
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
    GOK=0
    stage guard 41d; grc=$?
    if [ "$grc" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi
    [ "$grc" = "0" ] && GOK=1
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
