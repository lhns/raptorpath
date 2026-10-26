#!/bin/bash
# THE ONE-STORE-LAW ENVELOPE (docs/status.md §18). Derived from
# threadd9_run_all.sh, simplified (one binary, ~15 min of measurement).
#
#   SL_ROOT=/home/vibe/n1/sl setsid nohup bash storelaw_run_all.sh > $SL_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree ($SL_ROOT/src/raptorpath/tools/l1), started as
# `vibe`; `sudo -n` only for the measurement stages. WAITS for both operator
# locks (never breaks them), then holds them for the whole session:
#   1. fresh release build (fresh target dir) -> bin, sha256;
#   2. smoke (c1s LAW + CTL, c2 LAW) -> SMOKE-PASS or ABORT-SMOKE;
#   3. battery (storelaw_battery.sh) -> score.txt;
#   4. the q2 source-pin test (its own commit), run in the same session.
# SENTINELS: DONE-ALL / FAILED-ALL (cause in all-era.txt).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh storelaw_run_all.sh storelaw_battery.sh storelaw_parse.py \
    stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${SL_ROOT:?SL_ROOT}"
RUN="$ROOT/run"
SRC="$ROOT/src/raptorpath"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN" "$ROOT/bin"
SENTINELS="DONE-ALL FAILED-ALL all-era.txt SMOKE-PASS ABORT-SMOKE BINSHA.txt build.log smoke.log smoke-check.txt sl.log score.txt q2test.log"
probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
LB_PROOF_EXTRA="launch=$(date -u +%FT%TZ)" prove_sentinels "$RUN" $SENTINELS
sudo -n true 2>/dev/null || { echo "ABORT-SUDO"; exit 3; }
for f in $SENTINELS; do rm -f "$RUN/$f"; done
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "SL-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }
LB_TAG="storelaw_run_all:$$"
LB_LOG="$RUN/all-era.txt"
era "SL-ALL start load=$(cat /proc/loadavg)"
install_lock_traps
# Wait for the locks (another agent may hold them); never probe-and-remove.
acq() { # path
  local p="$1" fd
  until ( set -o noclobber; : > "$p" ) 2>/dev/null; do sleep 20; done
  exec {fd}<>"$p"
  flock -n "$fd" || { echo "foreign flock on $p"; exit 4; }
  echo "$$ $LB_TAG $(date -u +%FT%TZ)" >&"$fd"
  LB_LOCK_FD["$p"]="$fd"
  LOCKS_TAKEN="$LOCKS_TAKEN $p"
  _lb_say "LOCK-TAKEN $p"
}
acq /tmp/rwm-vm.lock
acq /home/vibe/rp.lock
era "SL-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"

# 1. BUILD (fresh target)
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
[ ! -e "$ROOT/target" ] || fail_all "ABORT-BUILD $ROOT/target is not fresh"
T0=$(date +%s)
( cd "$SRC" && CARGO_TARGET_DIR="$ROOT/target" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD (build.log)"
cp "$ROOT/target/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$SHA  $BIN  commit=$(cat "$ROOT/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)" > "$RUN/BINSHA.txt"
era "SL-ALL built sha=$SHA wall=$(( $(date +%s) - T0 ))s"

# 2. SMOKE
sudo -n env SL_BIN="$BIN" SL_SHA="$SHA" SL_OUTDIR="$RUN" SL_TAG=smoke SL_SMOKE=1 bash ./storelaw_battery.sh
SRC_RC=$?
if [ "$SRC_RC" != "0" ] || ! python3 ./storelaw_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"
era "SL-ALL SMOKE-PASS"

# 3. BATTERY
T0=$(date +%s)
era "SL-ALL battery start load=$(cut -d' ' -f1-3 /proc/loadavg)"
timeout 7200 sudo -n env SL_BIN="$BIN" SL_SHA="$SHA" SL_OUTDIR="$RUN" SL_TAG=sl bash ./storelaw_battery.sh
BRC=$?
era "SL-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
python3 ./storelaw_parse.py score "$RUN/sl.log" > "$RUN/score.txt" 2>&1
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./storelaw_parse.py check "$RUN/sl.log" && BAT_OK=1

# 4. the q2 source-pin test (separate commit), same session
( cd "$SRC" && CARGO_TARGET_DIR="$ROOT/target" CARGO_BUILD_JOBS=4 "$CARGO" test --release --lib q2_one_task_per_loop_no_multiplexer ) > "$RUN/q2test.log" 2>&1
era "SL-ALL q2 test rc=$?"

era "SL-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; echo "SL-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
