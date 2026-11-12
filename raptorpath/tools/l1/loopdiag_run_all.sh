#!/bin/bash
# THE LOOP-TIME DIAGNOSTIC ENVELOPE (docs/status.md §23 follow-up). Derived from
# storelaw19_run_all.sh. ONE binary, built fresh from $LD_ROOT/src (a git
# archive with a COMMIT file at $LD_ROOT/COMMIT) into a fresh $LD_ROOT/target.
#
#   LD_ROOT=/home/vibe/n1/ld LD_SHM=/dev/shm/ld setsid nohup bash loopdiag_run_all.sh > $LD_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from $LD_ROOT/src/raptorpath/tools/l1 as `vibe`; `sudo -n` only for the
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
crlf_guard lib.sh lib_battery.sh loopdiag_run_all.sh loopdiag_battery.sh loopdiag_wait34.sh loopdiag_parse.py anchordiag_parse.py disp36.py test_disp36.py disp40.py test_disp40.py place38_parse.py test_place38_parse.py place41b_score.py bbr48_score.py test_bbr48_parse.py place43_parse.py test_place43_parse.py place46_parse.py event_inject.py test_harness46.py test_harness47.py test_topo_netem.sh \
    test_loopdiag_parse.py test_anchordiag_parse.py stage3_parse.py storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${LD_ROOT:?LD_ROOT}"
PLAN="${LD_PLAN:-24}"
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
  *) echo "ABORT-PLAN LD_PLAN=$PLAN (24|27|29|34|36|38|40|41|41b|41c|43|43b|46|47|48)"; exit 3 ;;
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
# Plan 46 shares the root-owned tmpfs tree, LD_HARD and the two-tree build with plans 38 / 41 (P38 stays 0: no place-wire arms or ladder).
if [ "$P46" = 1 ]; then
  CAND_SHA="${LD_CAND_SHA:-UNSET}"
  case "$CAND_SHA" in UNSET|"") echo "ABORT-CAND LD_CAND_SHA is unset (the CAND SHA is recorded by amendment before launch); NOTHING WAS RUN"; exit 3 ;; esac
  [ "$(cat "$LD_ROOT/COMMIT" 2>/dev/null)" = "$CAND_SHA" ] || { echo "ABORT-CAND $LD_ROOT/COMMIT is not LD_CAND_SHA=$CAND_SHA; NOTHING WAS RUN"; exit 3; }
fi
if [ "$P48" = 1 ]; then
  : "${LD_BASE_SHA:?LD_BASE_SHA (main at launch, recorded by amendment) is required for plan 48}"
fi
# Measurement runs from tmpfs (shared rules): the run dir and the measured
# binary live under LD_SHM; the build stays on disk; persist() copies the run
# dir to $ROOT/run after the last row (and on every exit path).
SHM="${LD_SHM:?LD_SHM (a /dev/shm dir)}"
case "$SHM" in /dev/shm/*) ;; *) echo "ABORT-SHM $SHM is not under /dev/shm"; exit 3 ;; esac
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
SENTINELS="DONE-ALL FAILED-ALL all-era.txt TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE BINSHA.txt smoke.log smoke-check.txt ld.log score.txt parser-test.log"
probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
LB_PROOF_EXTRA="launch=$(date -u +%FT%TZ)" prove_sentinels "$RUN" $SENTINELS
sudo -n true 2>/dev/null || { echo "ABORT-SUDO"; exit 3; }
for f in $SENTINELS; do rm -f "$RUN/$f"; done
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "LD-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; persist; exit 5; }
LB_TAG="loopdiag_run_all:$$"
LB_LOG="$RUN/all-era.txt"
era "LD-ALL start plan=$PLAN load=$(cat /proc/loadavg)"
install_lock_traps
# Wait read-only (test -e only) until both locks are free, then take them with
# lib_battery.sh take_lock (noclobber + flock on a <> fd). A lost race aborts
# with ABORT-LOCK and nothing run; relaunch.
until ! test -e /tmp/rwm-vm.lock && ! test -e /home/vibe/rp.lock; do sleep 30; done
prec_samples() { local i p s="" ok=1; for i in 1 2 3 4 5; do p=$(awk '/^procs_running/{print $2}' /proc/stat); s="$s${p:-?},"; [ -n "$p" ] && [ "$p" -le 2 ] || ok=0; [ "$i" -lt 5 ] && sleep 2; done; echo "$ok procs_running=[$s] cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc)"; }
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$P46" = 1 ] || [ "$PLAN" = "40" ] || [ "$P43" = 1 ]; then
  : "${LD_HARD:?LD_HARD (epoch) is required for plan $PLAN}"
  PREC=$(prec_samples)
  if [ "${PREC%% *}" != 1 ] || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
    era "ABORT-PRECONDITION ${PREC#* }; NOTHING WAS RUN"; exit 3
  fi
  era "LD-ALL precondition met ${PREC#* } io=$(grep '^full' /proc/pressure/io)"
fi
take_lock /tmp/rwm-vm.lock
take_lock /home/vibe/rp.lock
era "LD-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"

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
  grep -aq -- "RWM_PLACE_WIRE" "$BIN_BASE" && fail_all "ABORT-CAND the BASE binary carries RWM_PLACE_WIRE: it is not main's"
fi

# ── plan 27: the test suites, after the battery ledger is on disk ────────────
# Same on-disk source tree (the workspace root, as threadr2_run_all.sh), its own
# fresh target dir; logs on the tmpfs run dir. A failure here is reported
# (TESTS-FAIL), never a battery void. The locks stay held.
SD="$RUN"      # where the suite logs and TESTS.txt go (plan 47: $RUN/suites)
run_suites() {
  local REPO TT TFAIL=0
  REPO="$(cd "$HERE/../../.." && pwd)"
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
  tst main timeout 7200 "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
  tst doc timeout 3600 "$CARGO" test --doc -p raptorpath --release || TFAIL=1
  tst wasm env -u GOLDEN_CAPTURE timeout 3600 "$CARGO" test -p raptorpath-wasm || TFAIL=1
  tst debug-witness timeout 7200 "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
  if [ "$PLAN" = "41" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || { [ "$P46" = 1 ] && [ "${LD_P46_SUITES:-0}" = "1" ]; }; then
    # Status 41 / 47 / 48 / 49: "the tools/l1 python tests" are all of them.
    tst parsers bash -c 'cd raptorpath/tools/l1 && for t in test_*.py; do python3 "$t" || { echo "PYTHON-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1
  else
    tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_loopdiag_parse.py && python3 test_anchordiag_parse.py" || TFAIL=1
  fi
  echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$SD/test-debug-witness.log")" >> "$SD/TESTS.txt"
  if [ "$TFAIL" = "0" ]; then touch "$SD/TESTS-OK"; else touch "$SD/TESTS-FAIL"; fi
  era "LD-ALL suites done tfail=$TFAIL"
  return $TFAIL
}
# Plans 46 / 41c with LD_P46_SUITES=1 (status 49): after run_suites is defined (it was called above its definition before).
if [ "$P46" = 1 ] && [ "$PLAN" != "47" ] && [ "$PLAN" != "48" ] && [ "${LD_P46_SUITES:-0}" = "1" ]; then
  run_suites || fail_all "ABORT-TESTS (TESTS.txt)"
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' /proc/pressure/io)"
fi
if [ "$PLAN" = "41" ]; then
  run_suites || fail_all "ABORT-TESTS (TESTS.txt)"
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' /proc/pressure/io)"
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
  era "LD-ALL suites ok; before the smoke $(prec_samples | cut -d" " -f2-) io=$(grep '^full' /proc/pressure/io)"
fi

# ── plan 47: suites on CAND's tree, then the guard (41c), then the path-event test (46); one ledger subdir each ──────────────────
if [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ]; then
  SD="$RUN/suites"; mkdir -p "$SD"
  # LD_REHEARSE=1 (plan 48 only, status 48b amendment 1): the builds, the smoke rows and their check, then stop; no suites, no battery.
  if [ "$PLAN" = "48" ] && [ "${LD_REHEARSE:-0}" = "1" ]; then era "LD-ALL suites SKIPPED (rehearsal)"
  else run_suites || fail_all "ABORT-TESTS (suites/TESTS.txt)"; fi
  era "LD-ALL suites ok; before the $([ "$PLAN" = "48" ] && echo smoke || echo guard) $(prec_samples | cut -d" " -f2-) io=$(grep '^full' /proc/pressure/io)"
  stage() { # name plan: smoke -> battery -> score -> check, all in $RUN/<name>; rc 0 iff the stage ran to the end
    local name="$1" splan="$2" dir="$RUN/$1" smk scr chk bto brc ok=0
    case "$splan" in
      41c) smk=smoke41c; scr=score41c; chk=check41c ;;
      48) smk=smoke48; scr=score48; chk=check48 ;;
      46) smk=smoke46; scr=score46; chk=check46 ;;
    esac
    mkdir -p "$dir"
    # LD_BASE_SHA rides explicitly: `sudo -n env` resets the environment, and the row parser reads it (status 49).
    local SENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$dir" LD_PLAN="$splan" LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE" LD_BASE_SHA="${LD_BASE_SHA:-}")
    era "LD-ALL stage $name (plan $splan) smoke; $(prec_samples | cut -d" " -f2-) io=$(grep '^full' /proc/pressure/io)"
    sudo -n env "${SENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./loopdiag_battery.sh
    if [ "$?" != "0" ] || ! python3 ./loopdiag_parse.py "$smk" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; then
      cat "$dir/smoke-check.txt" 2>/dev/null
      touch "$dir/ABORT-SMOKE"
      if grep -q '^ABORT-INJECT' "$dir/smoke-check.txt" 2>/dev/null; then era "ABORT-INJECT stage $name: a check row failed; no scored row ran"
      else era "ABORT-SMOKE stage $name (smoke-check.txt)"; fi
      return 1
    fi
    cat "$dir/smoke-check.txt"; touch "$dir/SMOKE-PASS"; era "LD-ALL stage $name SMOKE-PASS"
    if [ "$splan" = "48" ] && [ "${LD_REHEARSE:-0}" = "1" ]; then touch "$dir/REHEARSAL-PASS"; era "LD-ALL REHEARSAL-PASS stage $name (smoke only, no battery)"; return 2; fi
    bto=$(( LD_HARD - $(date +%s) ))
    if [ "$bto" -lt 1800 ]; then era "ABORT-BUDGET stage $name: ${bto}s left before LD_HARD (need 1800)"; return 1; fi
    local T1; T1=$(date +%s)
    timeout "$bto" sudo -n env "${SENV[@]}" LD_TAG=ld bash ./loopdiag_battery.sh
    brc=$?
    era "LD-ALL stage $name battery rc=$brc wall=$(( $(date +%s) - T1 ))s"
    if { [ "$splan" = "41c" ] || [ "$splan" = "48" ]; } && [ "$brc" = "0" ]; then
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
    elif [ "$splan" = "41c" ] || [ "$splan" = "48" ]; then python3 ./loopdiag_parse.py "$scr" "$dir/ld.log" "$dir/repl.log" > "$dir/score.txt" 2>&1
    else python3 ./loopdiag_parse.py "$scr" "$dir/ld.log" > "$dir/score.txt" 2>&1; fi
    [ "$brc" = "0" ] && python3 ./loopdiag_parse.py "$chk" "$dir/ld.log" && ok=1
    if [ "$splan" = "41c" ] || [ "$splan" = "48" ]; then
      # The guard step is scored by score41c / score48 (rank-minp, the design mandatory), never by 41b's report, and its ledger must hold the 96 rows of the design.
      local nrows; nrows=$(grep -ac '^P38ROW ' "$dir/ld.log")
      if [ "$nrows" != "96" ]; then era "ABORT-ROWS stage $name: $nrows P38ROW lines, the guard's design is 96"; ok=0; fi
    fi
    era "LD-ALL stage $name end ok=$ok $(grep -a '^VERDICT' "$dir/score.txt" | head -1)"
    if [ "$splan" = "41c" ] || [ "$splan" = "48" ]; then
      # Only the verdict is computed on the VM. The resolution table (reported only) is recomputed on the HOST from the kept ledger.
      era "LD-ALL guard resolution NOT computed on the VM; on the host: python3 raptorpath/tools/l1/place38_parse.py resolution41c <pulled run dir>/guard/ld.log"
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
  GOK=0; EOK=0
  stage guard 41c && GOK=1
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

T0=$(date +%s)
era "LD-ALL battery start load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"
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
era "LD-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"

if [ "$PLAN" = "27" ]; then
  persist
  run_suites || true
fi

if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
