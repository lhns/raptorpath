#!/bin/bash
# THE SAME-SESSION CONTROL ENVELOPE (docs/status.md §19). Derived from
# storelaw_run_all.sh. Two binaries built fresh in one session:
#   MAIN = ce60079 ($SL_ROOT/main-src, a git archive with a COMMIT file)
#   LAW  = the branch HEAD ($SL_ROOT/src, with a COMMIT file)
#
#   SL_ROOT=/home/vibe/n1/sl19 setsid nohup bash storelaw19_run_all.sh > $SL_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from $SL_ROOT/src/raptorpath/tools/l1 as `vibe`; `sudo -n` only for the
# measurement stages. WAITS for both operator locks (never breaks them), then
# holds them for the whole session: build both -> smoke -> battery -> score19.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh storelaw19_run_all.sh storelaw_battery.sh storelaw_parse.py \
    stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${SL_ROOT:?SL_ROOT}"
RUN="$ROOT/run"
LSRC="$ROOT/src/raptorpath"
MSRC="$ROOT/main-src/raptorpath"
BLAW="$ROOT/bin/law/raptorpath"
BMAIN="$ROOT/bin/main/raptorpath"
mkdir -p "$RUN" "$ROOT/bin/law" "$ROOT/bin/main"
SENTINELS="DONE-ALL FAILED-ALL all-era.txt SMOKE-PASS ABORT-SMOKE BINSHA.txt build-law.log build-main.log smoke.log smoke-check.txt sl.log score.txt parser-test.log"
probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
LB_PROOF_EXTRA="launch=$(date -u +%FT%TZ)" prove_sentinels "$RUN" $SENTINELS
sudo -n true 2>/dev/null || { echo "ABORT-SUDO"; exit 3; }
for f in $SENTINELS; do rm -f "$RUN/$f"; done
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "SL19-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }
LB_TAG="storelaw19_run_all:$$"
LB_LOG="$RUN/all-era.txt"
era "SL19-ALL start load=$(cat /proc/loadavg)"
install_lock_traps
acq() { # path: wait until free (never probe-and-remove), then hold it
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
era "SL19-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"

python3 ./test_storelaw_parse.py > "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log)"
era "SL19-ALL parser test ok"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
for pair in "law:$LSRC:$BLAW:build-law.log" "main:$MSRC:$BMAIN:build-main.log"; do
  IFS=: read -r who tree bin log <<< "$pair"
  [ -d "$tree" ] || fail_all "ABORT-BUILD no $tree"
  [ ! -e "$tree/target" ] || fail_all "ABORT-BUILD $tree/target is not fresh"
  T0=$(date +%s)
  ( cd "$tree" && CARGO_TARGET_DIR="$ROOT/target-$who" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$RUN/$log" 2>&1 \
    || fail_all "ABORT-BUILD $who ($log)"
  cp "$ROOT/target-$who/release/raptorpath" "$bin" || fail_all "ABORT-BUILD $who copy"
  era "SL19-ALL $who built wall=$(( $(date +%s) - T0 ))s"
done
SHA_LAW="$(sha256sum "$BLAW" | cut -d' ' -f1)"
SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
{ echo "$SHA_MAIN  MAIN $BMAIN  commit=$(cat "$ROOT/main-src/COMMIT" 2>/dev/null)"
  echo "$SHA_LAW  LAW $BLAW  commit=$(cat "$ROOT/COMMIT" 2>/dev/null)"; } > "$RUN/BINSHA.txt"
era "SL19-ALL binaries MAIN=$SHA_MAIN LAW=$SHA_LAW"
[ "$SHA_MAIN" != "$SHA_LAW" ] || fail_all "ABORT-BUILD the two binaries are byte-identical"

BENV=(SL_BIN="$BLAW" SL_SHA="$SHA_LAW" SL_BIN_MAIN="$BMAIN" SL_SHA_MAIN="$SHA_MAIN" SL_PLAN=19 SL_OUTDIR="$RUN")
sudo -n env "${BENV[@]}" SL_TAG=smoke SL_SMOKE=1 bash ./storelaw_battery.sh
SRC_RC=$?
if [ "$SRC_RC" != "0" ] || ! python3 ./storelaw_parse.py smoke19 "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"
era "SL19-ALL SMOKE-PASS"

T0=$(date +%s)
era "SL19-ALL battery start load=$(cut -d' ' -f1-3 /proc/loadavg)"
timeout 10800 sudo -n env "${BENV[@]}" SL_TAG=sl bash ./storelaw_battery.sh
BRC=$?
era "SL19-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
python3 ./storelaw_parse.py score19 "$RUN/sl.log" > "$RUN/score.txt" 2>&1
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./storelaw_parse.py check "$RUN/sl.log" && BAT_OK=1
era "SL19-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; echo "SL19-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
