#!/bin/bash
# THE ANCHOR-DIAGNOSTIC ENVELOPE (docs/status.md §23). Derived from
# storelaw19_run_all.sh. ONE binary, built fresh from $AD_ROOT/src (a git
# archive with a COMMIT file at $AD_ROOT/COMMIT) into a fresh $AD_ROOT/target.
#
#   AD_ROOT=/home/vibe/n1/ad setsid nohup bash anchordiag_run_all.sh > $AD_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from $AD_ROOT/src/raptorpath/tools/l1 as `vibe`; `sudo -n` only for the
# measurement stages. WAITS for both operator locks (never breaks them), then
# holds them for the whole session: build -> smoke -> battery -> score.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh anchordiag_run_all.sh anchordiag_battery.sh anchordiag_parse.py \
    test_anchordiag_parse.py stage3_parse.py storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${AD_ROOT:?AD_ROOT}"
RUN="$ROOT/run"
SRC="$ROOT/src/raptorpath"
TARGET="$ROOT/target"
BIN="$ROOT/bin/raptorpath"
mkdir -p "$RUN" "$ROOT/bin"
SENTINELS="DONE-ALL FAILED-ALL all-era.txt SMOKE-PASS ABORT-SMOKE BINSHA.txt build.log smoke.log smoke-check.txt ad.log score.txt parser-test.log"
probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
LB_PROOF_EXTRA="launch=$(date -u +%FT%TZ)" prove_sentinels "$RUN" $SENTINELS
sudo -n true 2>/dev/null || { echo "ABORT-SUDO"; exit 3; }
for f in $SENTINELS; do rm -f "$RUN/$f"; done
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "AD-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }
LB_TAG="anchordiag_run_all:$$"
LB_LOG="$RUN/all-era.txt"
era "AD-ALL start load=$(cat /proc/loadavg)"
install_lock_traps
# Wait read-only (test -e only) until both locks are free, then take them with
# lib_battery.sh take_lock (noclobber + flock on a <> fd). A lost race aborts
# with ABORT-LOCK and nothing run; relaunch.
until ! test -e /tmp/rwm-vm.lock && ! test -e /home/vibe/rp.lock; do sleep 30; done
take_lock /tmp/rwm-vm.lock
take_lock /home/vibe/rp.lock
era "AD-ALL locks held; cotenants cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"

python3 ./test_anchordiag_parse.py > "$RUN/parser-test.log" 2>&1 || fail_all "ABORT-TESTS (parser-test.log)"
era "AD-ALL parser test ok"

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
[ -d "$SRC" ] || fail_all "ABORT-BUILD no $SRC"
[ ! -e "$SRC/target" ] || fail_all "ABORT-BUILD $SRC/target is not fresh"
[ ! -e "$TARGET" ] || fail_all "ABORT-BUILD $TARGET is not fresh"
T0=$(date +%s)
( cd "$SRC" && CARGO_TARGET_DIR="$TARGET" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ) > "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD (build.log)"
cp "$TARGET/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
era "AD-ALL built wall=$(( $(date +%s) - T0 ))s"
SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$SHA  $BIN  commit=$(cat "$ROOT/COMMIT" 2>/dev/null)" > "$RUN/BINSHA.txt"
era "AD-ALL binary $SHA"

BENV=(AD_BIN="$BIN" AD_SHA="$SHA" AD_OUTDIR="$RUN")
sudo -n env "${BENV[@]}" AD_TAG=smoke AD_SMOKE=1 bash ./anchordiag_battery.sh
SRC_RC=$?
if [ "$SRC_RC" != "0" ] || ! python3 ./anchordiag_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"
era "AD-ALL SMOKE-PASS"

T0=$(date +%s)
era "AD-ALL battery start load=$(cut -d' ' -f1-3 /proc/loadavg)"
timeout 10800 sudo -n env "${BENV[@]}" AD_TAG=ad bash ./anchordiag_battery.sh
BRC=$?
era "AD-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s"
python3 ./anchordiag_parse.py score "$RUN/ad.log" > "$RUN/score.txt" 2>&1
BAT_OK=0
[ "$BRC" = "0" ] && python3 ./anchordiag_parse.py check "$RUN/ad.log" && BAT_OK=1
era "AD-ALL end bat_ok=$BAT_OK raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ]; then touch "$RUN/DONE-ALL"; echo "AD-ALL-DONE"; exit 0; fi
fail_all "battery_ok=$BAT_OK"
