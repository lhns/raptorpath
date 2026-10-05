#!/bin/bash
# THE THREADING-REDESIGN P0 BATTERY'S ENVELOPE (docs/status.md §9
# pre-registration; docs/measurement-discipline.md "The five-hour cap").
#
#   P0_ROOT=/home/vibe/p0 setsid nohup bash p0_launch.sh > $P0_ROOT/waiter.out 2>&1 < /dev/null &
#
# (p0_launch.sh waits for both locks to be free, then runs this). Run from
# the synced tree ($P0_ROOT/src/raptorpath/tools/l1); main's tree is
# $P0_ROOT/main-src (`git archive 8d7d8c1`). STARTED AS `vibe`, NOT ROOT;
# `sudo -n` only for the measurement stages. Hard deadline = lock
# acquisition + 4 h 50 min.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. build + tests (P0 tree, fresh target dir $P0_ROOT/target):
#      `cargo build --release`; `cargo test -p raptorpath -p raptorpath-math
#      --release --no-fail-fast -- --test-threads=2`; `cargo test --doc -p
#      raptorpath --release`; `cargo test -p raptorpath-wasm`; the Windows
#      check `cargo check -p raptorpath --target x86_64-pc-windows-gnu` when
#      that target is installed (recorded, not gating); the binary ->
#      $P0_ROOT/bin/raptorpath, sha256;
#   2. main's binary (fresh target dir $P0_ROOT/target-main) ->
#      $P0_ROOT/bin-main/raptorpath, sha256;
#   3. smoke (c1s-400 P0, c1s-400 MAIN, c1d-400 P0) -> SMOKE-PASS or
#      ABORT-SMOKE; GO automatic iff TESTS-OK and SMOKE-PASS (else the
#      operator writes GO / NOGO, <= 40 min);
#   4. battery (p0_battery.sh, 3 blocks x 4 invocations); 5. score.
#
# SENTINELS: DONE-ALL only when the ledger carries P0-BATTERY-DONE and
# `p0_parse.py check` returns 0; FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-5H-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh p0_run_all.sh p0_battery.sh p0_parse.py \
    stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${P0_ROOT:?P0_ROOT}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
MSRC="$ROOT/main-src"
BIN="$ROOT/bin/raptorpath"
MBIN="$ROOT/bin-main/raptorpath"
SMOKE_PLAN="c1s-400:P0 c1s-400:MAIN c1d-400:P0"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin" "$ROOT/bin-main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE GO NOGO BINSHA.txt build.log \
build-main.log smoke.log smoke-check.txt p0.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.p0-finished"
STAGE_PIDFILE="$RUN/.p0-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "P0-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="p0_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "P0-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg)" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export P0_LOCK_OWNER="p0_run_all:$$"
HARD=$(( $(date +%s) + 17400 ))   # lock acquisition + 4 h 50 min
era "P0-ALL locks held; hard=$HARD"
refresh_locks() { # stage-name
  local l
  for l in "$VM_LOCK" "$RP_LOCK"; do
    if ! grep -qF -- "$P0_LOCK_OWNER" "$l" 2>/dev/null; then
      era "LOCK-TRUNCATED-BY-FOREIGN $l before $1: '$(cat "$l" 2>/dev/null)'"
    fi
    echo "$$ $LB_TAG $(date -u +%FT%TZ)" > "$l"
  done
}
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$HARD" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "P0-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by p0_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $HARD; the 5 h cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
  local spid; spid=$(cat "$STAGE_PIDFILE" 2>/dev/null)
  [ -n "$spid" ] && sudo -n kill -TERM "$spid" 2>/dev/null
  [ -n "$spid" ] && kill -TERM "$spid" 2>/dev/null
  sudo -n pkill -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 36); do
    [ "$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ] && break
    sleep 5
  done
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "P0-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; release_locks; }
trap finish EXIT
trap 'finish; exit 130' INT
trap 'finish; exit 143' TERM
stage() { # run "$@" as the tracked stage (the backstop TERMs it)
  "$@" &
  echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; local rc=$?; rm -f "$STAGE_PIDFILE"
  return $rc
}

# ── 1. BUILD + TESTS (P0 tree) ───────────────────────────────────────────
refresh_locks build
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
export CARGO_TARGET_DIR="$ROOT/target"
era "P0-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO target=$CARGO_TARGET_DIR cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "P0-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
[ "$BRC" = "0" ] || fail_all "ABORT-BUILD rc=$BRC (see build.log)"
: > "$RUN/TESTS.txt"
tst() { # name -- cmd...
  local name="$1"; shift
  local t0; t0=$(date +%s)
  ( cd "$SRC" && stage "$@" ) > "$RUN/test-$name.log" 2>&1
  local rc=$?
  local sum
  sum=$(grep -a '^test result:' "$RUN/test-$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{printf "passed=%d failed=%d ignored=%d binaries=%d", p, f, i, NR}')
  echo "TEST $name rc=$rc wall=$(( $(date +%s) - t0 ))s $sum cmd=[$*]" >> "$RUN/TESTS.txt"
  grep -a -E '^test .* FAILED$|^failures:$|^    [a-z_:0-9]+$' "$RUN/test-$name.log" | head -40 >> "$RUN/TESTS.txt"
  era "P0-ALL test $name rc=$rc $sum"
  return $rc
}
TFAIL=0
tst main "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
# The P0 tests by name, from the main log (rule 1: they ran).
grep -aE "^test .*(runtime_obs|thr_lines|lag_|task_stat|quantile_rule|thr_and_lag|names_workers)" \
    "$RUN/test-main.log" | sed 's/^/P0-TEST /' >> "$RUN/TESTS.txt"
# The parsers' offline tests (python; rule 14 for the harness side).
for t in test_l1common.py test_p0_parse.py test_stage3_parse.py; do
  ( python3 "$HERE/$t" > "$RUN/test-$t.log" 2>&1 ); prc=$?
  echo "PYTEST $t rc=$prc $(tail -1 "$RUN/test-$t.log")" >> "$RUN/TESTS.txt"
  [ "$prc" = "0" ] || TFAIL=1
done
# Windows must still compile (recorded, not gating: the cross C toolchain the
# ring build script needs may be absent on the VM, which is not a defect of
# this tree).
if rustup target list --installed 2>/dev/null | grep -qx x86_64-pc-windows-gnu; then
  ( cd "$SRC" && stage "$CARGO" check -p raptorpath --target x86_64-pc-windows-gnu \
      --target-dir "$ROOT/target-win" ) > "$RUN/check-windows.log" 2>&1
  echo "WINDOWS-CHECK rc=$? (x86_64-pc-windows-gnu; check-windows.log; recorded, not gating)" >> "$RUN/TESTS.txt"
else
  echo "WINDOWS-CHECK not-run: target x86_64-pc-windows-gnu not installed (rustup target list --installed)" >> "$RUN/TESTS.txt"
fi
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$CARGO_TARGET_DIR/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
P0_SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "P0-ALL tests done tfail=$TFAIL sha256=$P0_SHA"

# ── 2. MAIN'S BINARY (the control) ───────────────────────────────────────
refresh_locks build-main
T0=$(date +%s)
( cd "$MSRC" && stage env CARGO_TARGET_DIR="$ROOT/target-main" "$CARGO" build --release --bin raptorpath ) \
    > "$RUN/build-main.log" 2>&1
MRC=$?
era "P0-ALL main build rc=$MRC wall=$(( $(date +%s) - T0 ))s commit=$(cat "$MSRC/COMMIT" 2>/dev/null)"
[ "$MRC" = "0" ] || fail_all "ABORT-BUILD main rc=$MRC (see build-main.log)"
cp "$ROOT/target-main/release/raptorpath" "$MBIN" || fail_all "ABORT-BUILD main copy"
MAIN_SHA="$(sha256sum "$MBIN" | cut -d' ' -f1)"
{ echo "$P0_SHA  $BIN  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$MAIN_SHA  $MBIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) (control)"; } > "$RUN/BINSHA.txt"
export P0_SHA MAIN_SHA

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
refresh_locks smoke
T0=$(date +%s)
stage sudo -n env P0_SHA="$P0_SHA" MAIN_SHA="$MAIN_SHA" P0_LOCK_OWNER="$P0_LOCK_OWNER" P0_OUTDIR="$RUN" \
    P0_TAG=smoke P0_BLOCKS="1:42" P0_SMOKE_PLAN="$SMOKE_PLAN" P0_BIN="$BIN" MAIN_BIN="$MBIN" \
    bash ./p0_battery.sh 1
SRC_RC=$?
era "P0-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./p0_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
touch "$RUN/SMOKE-PASS"

# ── GO: automatic iff TESTS-OK and SMOKE-PASS; else the operator decides ─
if [ -f "$RUN/TESTS-OK" ]; then
  echo "AUTO-GO tests-ok smoke-pass $(date -u +%FT%TZ)" > "$RUN/GO"
else
  GO_DEADLINE=$(( $(date +%s) + 2400 ))
  while :; do
    [ -f "$RUN/GO" ] && break
    [ -f "$RUN/NOGO" ] && fail_all "NOGO by the operator after the tests/smoke"
    [ "$(date +%s)" -ge "$GO_DEADLINE" ] && fail_all "NO-GO-TIMEOUT (40 min after SMOKE-PASS)"
    sleep 10
  done
fi
era "P0-ALL GO ($(cat "$RUN/GO"))"

# ── 4. BATTERY ───────────────────────────────────────────────────────────
refresh_locks battery
T0=$(date +%s)
era "P0-ALL battery start cotenants: $(cotenants)"
stage sudo -n env P0_SHA="$P0_SHA" MAIN_SHA="$MAIN_SHA" P0_LOCK_OWNER="$P0_LOCK_OWNER" P0_OUTDIR="$RUN" \
    P0_TAG=p0 P0_BIN="$BIN" MAIN_BIN="$MBIN" bash ./p0_battery.sh 1
BRC=$?
era "P0-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/p0.log" ] && grep -aq "P0-BATTERY-DONE" "$RUN/p0.log" \
    && python3 ./p0_parse.py check "$RUN/p0.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 5. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./p0_parse.py score "$RUN/p0.log" > "$RUN/score.txt" 2>&1
era "P0-ALL end bat_ok=$BAT_OK load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "P0-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
