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
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh loopdiag_run_all.sh loopdiag_battery.sh loopdiag_launch27.sh loopdiag_parse.py anchordiag_parse.py \
    test_loopdiag_parse.py test_anchordiag_parse.py stage3_parse.py storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${LD_ROOT:?LD_ROOT}"
PLAN="${LD_PLAN:-24}"
case "$PLAN" in
  24) P_SMOKE=smoke; P_SCORE=score; P_CHECK=check ;;
  27) P_SMOKE=smoke27; P_SCORE=score27; P_CHECK=check27 ;;
  *) echo "ABORT-PLAN LD_PLAN=$PLAN (24|27)"; exit 3 ;;
esac
# Measurement runs from tmpfs (shared rules): the run dir and the measured
# binary live under LD_SHM; the build stays on disk; persist() copies the run
# dir to $ROOT/run after the last row (and on every exit path).
SHM="${LD_SHM:?LD_SHM (a /dev/shm dir)}"
case "$SHM" in /dev/shm/*) ;; *) echo "ABORT-SHM $SHM is not under /dev/shm"; exit 3 ;; esac
RUN="$SHM/run"
SRC="$ROOT/src/raptorpath"
TARGET="$ROOT/target"
BIN="$SHM/bin/raptorpath"
BUILDLOG="$ROOT/build.log"
mkdir -p "$RUN" "$SHM/bin"
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
take_lock /tmp/rwm-vm.lock
take_lock /home/vibe/rp.lock
era "LD-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"

python3 ./test_loopdiag_parse.py > "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log)"
era "LD-ALL parser test ok"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
[ -d "$SRC" ] || fail_all "ABORT-BUILD no $SRC"
[ ! -e "$SRC/target" ] || fail_all "ABORT-BUILD $SRC/target is not fresh"
[ ! -e "$TARGET" ] || fail_all "ABORT-BUILD $TARGET is not fresh"
T0=$(date +%s)
( cd "$SRC" && CARGO_TARGET_DIR="$TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$BUILDLOG" 2>&1 \
  || fail_all "ABORT-BUILD ($BUILDLOG)"
cp "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
era "LD-ALL built wall=$(( $(date +%s) - T0 ))s"
SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$SHA  $BIN  commit=$(cat "$ROOT/COMMIT" 2>/dev/null)" > "$RUN/BINSHA.txt"
era "LD-ALL binary $SHA"

BENV=(LD_BIN="$BIN" LD_SHA="$SHA" LD_OUTDIR="$RUN" LD_PLAN="$PLAN")
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
timeout 10800 sudo -n env "${BENV[@]}" LD_TAG=ld bash ./loopdiag_battery.sh
BRC=$?
era "LD-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
python3 ./loopdiag_parse.py "$P_SCORE" "$RUN/ld.log" > "$RUN/score.txt" 2>&1
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./loopdiag_parse.py "$P_CHECK" "$RUN/ld.log" && BAT_OK=1
era "LD-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg); io=$(grep '^full' /proc/pressure/io)"

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
  tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_loopdiag_parse.py && python3 test_anchordiag_parse.py" || TFAIL=1
  echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
  if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
  era "LD-ALL suites done tfail=$TFAIL"
  return $TFAIL
}
if [ "$PLAN" = "27" ]; then
  persist
  run_suites || true
fi

if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
