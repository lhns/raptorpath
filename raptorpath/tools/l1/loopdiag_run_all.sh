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
# LD_PLAN=41 (status section 41): plan 38's battery plus the arm BASE, main's own binary. TWO fresh builds, each from its own
# `git archive` tree with its own fresh target dir: $LD_ROOT/src (the shipped tree, COMMIT at $LD_ROOT/COMMIT) and
# $LD_ROOT/base/src (main at 4926822, COMMIT at $LD_ROOT/base/COMMIT, target $LD_ROOT/target-base). Order: parser gate,
# both builds, the suites on the shipped tree (a REAL failure is ABORT-TESTS: no row runs; the tools/l1 python tests are all
# test_*.py), the smoke, the battery from root-owned tmpfs. The BASE binary is installed as $LD_SHM/bin-base/raptorpath (same
# file name, so pgrep -x / pkill -x raptorpath see it).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh loopdiag_run_all.sh loopdiag_battery.sh loopdiag_wait34.sh loopdiag_parse.py anchordiag_parse.py disp36.py test_disp36.py disp40.py test_disp40.py place38_parse.py test_place38_parse.py \
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
  *) echo "ABORT-PLAN LD_PLAN=$PLAN (24|27|29|34|36|38|40|41)"; exit 3 ;;
esac
# Plans 38 and 41 share everything but the BASE arm (P38=1 is "the place-wire battery").
P38=0; { [ "$PLAN" = "38" ] || [ "$PLAN" = "41" ]; } && P38=1
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
if [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$PLAN" = "40" ]; then
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
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$PLAN" = "40" ]; then
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
if [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$PLAN" = "40" ]; then
  sudo -n install -m 0755 -o root -g root "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
else
  cp "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
fi
era "LD-ALL built wall=$(( $(date +%s) - T0 ))s"
SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$SHA  $BIN  commit=$(cat "$ROOT/COMMIT" 2>/dev/null)" > "$RUN/BINSHA.txt"
era "LD-ALL binary $SHA"
SHA_BASE=""
if [ "$PLAN" = "41" ]; then
  # BASE: main's own binary, its own archive tree and its own fresh target dir (never shared with the shipped tree's).
  [ -d "$BASE_SRC" ] || fail_all "ABORT-BUILD no $BASE_SRC"
  [ ! -e "$BASE_SRC/target" ] || fail_all "ABORT-BUILD $BASE_SRC/target is not fresh"
  [ ! -e "$BASE_TARGET" ] || fail_all "ABORT-BUILD $BASE_TARGET is not fresh"
  case "$(cat "$ROOT/base/COMMIT" 2>/dev/null)" in 4926822*) ;; *) fail_all "ABORT-BUILD base COMMIT is not 4926822" ;; esac
  T1=$(date +%s)
  ( cd "$BASE_SRC" && CARGO_TARGET_DIR="$BASE_TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$ROOT/build-base.log" 2>&1 \
    || fail_all "ABORT-BUILD (base: $ROOT/build-base.log)"
  sudo -n install -m 0755 -o root -g root "$BASE_TARGET/release/raptorpath" "$BIN_BASE" || fail_all "ABORT-BUILD copy (base)"
  SHA_BASE="$(sha256sum "$BIN_BASE" | cut -d' ' -f1)"
  echo "$SHA_BASE  $BIN_BASE  commit=$(cat "$ROOT/base/COMMIT" 2>/dev/null) (BASE)" >> "$RUN/BINSHA.txt"
  era "LD-ALL built BASE wall=$(( $(date +%s) - T1 ))s binary $SHA_BASE"
  [ "$SHA_BASE" != "$SHA" ] || fail_all "ABORT-BUILD BASE and MAIN binaries are identical"
fi

# ── plan 27: the test suites, after the battery ledger is on disk ────────────
# Same on-disk source tree (the workspace root, as threadr2_run_all.sh), its own
# fresh target dir; logs on the tmpfs run dir. A failure here is reported
# (TESTS-FAIL), never a battery void. The locks stay held.
run_suites() {
  local REPO TT TFAIL=0
  REPO="$(cd "$HERE/../../.." && pwd)"
  TT="$ROOT/target-tests"
  : > "$RUN/TESTS.txt"
  if [ -e "$TT" ]; then
    echo "REAL-FAILURE suites: $TT is not fresh -- suites not run" >> "$RUN/TESTS.txt"
    touch "$RUN/TESTS-FAIL"; era "LD-ALL suites NOT RUN: $TT is not fresh"; return 1
  fi
  tst() { # name cmd...   (cmd may carry a `--` before the test-binary args)
    local name="$1"; shift
    local t0; t0=$(date +%s)
    ( cd "$REPO" && CARGO_TARGET_DIR="$TT" "$@" ) > "$RUN/test-$name.log" 2>&1
    local rc=$?
    local sum
    sum=$(grep -a '^test result:' "$RUN/test-$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{printf "passed=%d failed=%d ignored=%d binaries=%d", p, f, i, NR}')
    echo "TEST $name rc=$rc wall=$(( $(date +%s) - t0 ))s $sum cmd=[$*]" >> "$RUN/TESTS.txt"
    grep -a -E '^test .* FAILED$|^---- ' "$RUN/test-$name.log" | head -40 >> "$RUN/TESTS.txt"
    era "LD-ALL test $name rc=$rc $sum"
    [ "$rc" = "0" ] && return 0
    # The pre-registered flake rule: a failure that passes on an immediate solo
    # re-run of that test alone is FLAKE; any other is REAL.
    local real=0 t
    if ! grep -aq -E '^test .* FAILED$' "$RUN/test-$name.log"; then
      echo "REAL-FAILURE $name rc=$rc with no failing test named (build error?)" >> "$RUN/TESTS.txt"
      return 1
    fi
    for t in $(grep -a -E '^test .* FAILED$' "$RUN/test-$name.log" | awk '{print $2}' | sort -u); do
      local base=("$@")
      local cmd=() a seen=0
      for a in "${base[@]}"; do [ "$a" = "--" ] && seen=1; [ "$seen" = "0" ] && cmd+=("$a"); done
      if ( cd "$REPO" && CARGO_TARGET_DIR="$TT" "${cmd[@]}" -- "$t" --exact --test-threads=1 ) > "$RUN/test-$name-rerun.log" 2>&1 \
          && grep -aq "^test $t ... ok" "$RUN/test-$name-rerun.log"; then
        echo "FLAKE $name $t (passed on the immediate solo re-run)" >> "$RUN/TESTS.txt"
      else
        echo "REAL-FAILURE $name $t" >> "$RUN/TESTS.txt"
        real=1
      fi
    done
    return $real
  }
  tst main timeout 7200 "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
  tst doc timeout 3600 "$CARGO" test --doc -p raptorpath --release || TFAIL=1
  tst wasm env -u GOLDEN_CAPTURE timeout 3600 "$CARGO" test -p raptorpath-wasm || TFAIL=1
  tst debug-witness timeout 7200 "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
  if [ "$PLAN" = "41" ]; then
    # Status 41: "the tools/l1 python tests" are all of them.
    tst parsers bash -c 'cd raptorpath/tools/l1 && for t in test_*.py; do python3 "$t" || { echo "PYTHON-TEST-FAIL $t"; exit 1; }; done' || TFAIL=1
  else
    tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_loopdiag_parse.py && python3 test_anchordiag_parse.py" || TFAIL=1
  fi
  echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
  if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
  era "LD-ALL suites done tfail=$TFAIL"
  return $TFAIL
}
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

BENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$RUN" LD_PLAN="$PLAN")
[ "$PLAN" = "41" ] && BENV+=(LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE")
sudo -n env "${BENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./loopdiag_battery.sh
SRC_RC=$?
if [ "$SRC_RC" != "0" ] || ! python3 ./loopdiag_parse.py "$P_SMOKE" "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"
era "LD-ALL SMOKE-PASS"

T0=$(date +%s)
era "LD-ALL battery start load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"
BTO=10800
if [ "$PLAN" = "34" ] || [ "$PLAN" = "36" ] || [ "$P38" = 1 ] || [ "$PLAN" = "40" ]; then
  BTO=$(( LD_HARD - $(date +%s) ))
  [ "$BTO" -ge 1800 ] || fail_all "ABORT-BUDGET ${BTO}s left before LD_HARD"
fi
timeout "$BTO" sudo -n env "${BENV[@]}" LD_TAG=ld bash ./loopdiag_battery.sh
BRC=$?
era "LD-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
python3 ./loopdiag_parse.py "$P_SCORE" "$RUN/ld.log" > "$RUN/score.txt" 2>&1
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./loopdiag_parse.py "$P_CHECK" "$RUN/ld.log" && BAT_OK=1
era "LD-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"

if [ "$PLAN" = "27" ]; then
  persist
  run_suites || true
fi

if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
