#!/bin/bash
# THE V-SK ENVELOPE (docs/status.md, the incremental-SACK-marking
# pre-registration; docs/measurement-discipline.md "The five-hour cap").
# Derived from threadr2_run_all.sh (the V-R2 envelope).
#
#   TSK_ROOT=/home/vibe/skrun TSK_HARD_DEADLINE=<epoch> \
#     setsid nohup bash threadsk_run_all.sh > $TSK_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced NEW tree ($TSK_ROOT/src/raptorpath/tools/l1); the MAIN
# tree (archive of github/main 92934a7) is at $TSK_ROOT/main-src, with a COMMIT file.
# STARTED AS `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. NEW tree: `cargo build --release`; tests (release: `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`; `cargo test --doc -p raptorpath --release`;
#      `cargo test -p raptorpath-wasm`; DEBUG: `cargo test -p raptorpath
#      --no-fail-fast -- --test-threads=2`, the run with the owner identity
#      witness; the python parser tests); `cargo build --release --bin
#      raptorpath` -> bin/new/raptorpath, sha256;
#   2. MAIN tree: `cargo build --release --bin raptorpath` (fresh target) ->
#      bin/main, sha256;
#   3. smoke (one invocation per arm at c1s-400 and c8-100, seed 42) ->
#      SMOKE-PASS or ABORT-SMOKE. A test failure is ABORT-TESTS.
#   4. budget -> PLAN.txt; 5. battery (threadsk_battery.sh, tag tsk); 6. score.
#
# SENTINELS: DONE-ALL only when the ledger carries TSK-BATTERY-DONE, `check`
# returns 0 and no truncation happened. FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadsk_run_all.sh threadsk_battery.sh threadsk_parse.py \
    threadq1_parse.py threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${TSK_ROOT:?TSK_ROOT}"
: "${TSK_HARD_DEADLINE:?TSK_HARD_DEADLINE (epoch seconds) is required}"
# Agent rules (current): builds and test suites stay on disk; the MEASUREMENT
# runs from tmpfs -- both binaries are copied to $SHM/bin/{main,new} and the
# run dir lives under $SHM/run; it is copied to $ROOT/run at exit.
SHM="${TSK_SHM:-/dev/shm/tsk}"
RUN="$SHM/run"
SRC="$(cd "$HERE/../../.." && pwd)"
MSRC="$ROOT/main-src"
BNEW_D="$ROOT/bin/new/raptorpath"      # as built, on disk
BMAIN_D="$ROOT/bin/main/raptorpath"
BNEW="$SHM/bin/new/raptorpath"         # the measured copies, tmpfs
BMAIN="$SHM/bin/main/raptorpath"
persist() { mkdir -p "$ROOT/run" && cp -a "$RUN/." "$ROOT/run/" 2>/dev/null; }
# /proc/pressure/io `full` field value (avg10 | avg60 | avg300), empty if unread.
io_full() { awk -v k="$1" '/^full/{for(i=2;i<=NF;i++){split($i,a,"=");if(a[1]==k)print a[2]}}' /proc/pressure/io 2>/dev/null; }
SOFT=$(( TSK_HARD_DEADLINE - 600 ))
R_PRIOR=240          # s per (rep, seed) block of 8 invocations (the 30 s/invocation prior)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall (4 invocations)
N_MAX=3              # reps per seed (pre-registered n = 6 per arm and cell)
SMOKE_PLAN="c1s-400:MAIN c1s-400:NEW c8-100:NEW c8-100:MAIN"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin/new" "$ROOT/bin/main" "$SHM/bin/new" "$SHM/bin/main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE PLAN.txt BINSHA.txt build.log build-main.log \
smoke.log smoke-check.txt tsk.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$TSK_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.tsk-finished"
STAGE_PIDFILE="$RUN/.tsk-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "TSK-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
# Launch precondition (agent rules, current; supersedes the loadavg and io
# clauses): no cargo / rustc and /proc/stat procs_running <= 2 in each of 5
# samples 2 s apart, checked BEFORE the locks are taken (take_lock refuses a
# held lock; it never waits or breaks one; locks are tested only that way).
# loadavg counts disk-blocked tasks and is NOT used; io pressure is recorded
# as a covariate. The samples are in the ledger.
PR_SAMPLES=""
PR_OK=1
for _i in 1 2 3 4 5; do
  _p=$(awk '/^procs_running/{print $2}' /proc/stat)
  PR_SAMPLES="$PR_SAMPLES${_p:-?},"
  [ -n "$_p" ] && [ "$_p" -le 2 ] || PR_OK=0
  [ "$_i" -lt 5 ] && sleep 2
done
if [ "$PR_OK" != 1 ] || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  echo "ABORT-PRECONDITION procs_running=[$PR_SAMPLES] cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) io_full_avg10=$(io_full avg10); NOTHING WAS RUN."
  exit 3
fi
# ABORT-ONDISK: the measured binaries and the run dir must be on tmpfs.
for _d in "$RUN" "$SHM/bin/new" "$SHM/bin/main"; do
  [ "$(stat -f -c %T "$_d" 2>/dev/null)" = "tmpfs" ] || { echo "ABORT-ONDISK $_d is not tmpfs (rows would be VOID-ONDISK); NOTHING WAS RUN."; exit 5; }
done
LB_TAG="threadsk_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "TSK-ALL start $LAUNCH_ISO procs_running=[$PR_SAMPLES] io_full_avg10=$(io_full avg10) hard=$TSK_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export TSK_LOCK_OWNER="threadsk_run_all:$$"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) procs_running=$(awk '/^procs_running/{print $2}' /proc/stat) io_full_avg10=$(io_full avg10)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$TSK_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "TSK-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by threadsk_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $TSK_HARD_DEADLINE; the budget cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
  local spid; spid=$(cat "$STAGE_PIDFILE" 2>/dev/null)
  [ -n "$spid" ] && sudo -n kill -TERM "$spid" 2>/dev/null
  [ -n "$spid" ] && kill -TERM "$spid" 2>/dev/null
  sudo -n pkill -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 36); do
    [ "$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ] && break
    sleep 5
  done
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"
  echo "TSK-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; sudo -n chown -R "$(id -un)" "$RUN" 2>/dev/null; persist; release_locks; }
trap finish EXIT
trap 'finish; exit 130' INT
trap 'finish; exit 143' TERM
stage() { # run "$@" as the tracked stage (the backstop TERMs it)
  "$@" &
  echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; local rc=$?; rm -f "$STAGE_PIDFILE"
  return $rc
}

# ── 1. BUILD + TESTS (NEW tree) ───────────────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "TSK-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "TSK-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
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
  grep -a -E '^test .* FAILED$|^---- ' "$RUN/test-$name.log" | head -40 >> "$RUN/TESTS.txt"
  era "TSK-ALL test $name rc=$rc $sum"
  [ "$rc" = "0" ] && return 0
  # The pre-registered flake rule (as §8/§11/§12): a failure that passes on
  # an immediate re-run of that test alone is FLAKE; any other is REAL.
  local real=0 t
  if ! grep -aq -E '^test .* FAILED$' "$RUN/test-$name.log"; then
    echo "REAL-FAILURE $name rc=$rc with no failing test named (build error?)" >> "$RUN/TESTS.txt"
    return 1
  fi
  for t in $(grep -a -E '^test .* FAILED$' "$RUN/test-$name.log" | awk '{print $2}' | sort -u); do
    local base=("$@")
    local cmd=() a seen=0
    for a in "${base[@]}"; do [ "$a" = "--" ] && seen=1; [ "$seen" = "0" ] && cmd+=("$a"); done
    if ( cd "$SRC" && stage "${cmd[@]}" -- "$t" --exact --test-threads=1 ) > "$RUN/test-$name-rerun.log" 2>&1 \
        && grep -aq "^test $t ... ok" "$RUN/test-$name-rerun.log"; then
      echo "FLAKE $name $t (passed on the immediate solo re-run)" >> "$RUN/TESTS.txt"
    else
      echo "REAL-FAILURE $name $t" >> "$RUN/TESTS.txt"
      real=1
    fi
  done
  return $real
}
TFAIL=0
tst main "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
tst debug-witness "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_l1common.py && python3 test_stage3_parse.py && python3 test_threadq1_parse.py && python3 test_threadq2_parse.py && python3 test_threadr2_parse.py && python3 test_threadsk_parse.py" || TFAIL=1
# The P1 lock-order witness went with the scheduler mutex (threading NEW);
# the owner identity witness is the one left (its panic message counted).
echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BNEW_D" || fail_all "ABORT-BUILD copy"
cp "$BNEW_D" "$BNEW" || fail_all "ABORT-BUILD copy to tmpfs"
TSK_SHA_NEW="$(sha256sum "$BNEW" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "TSK-ALL tests done tfail=$TFAIL new sha256=$TSK_SHA_NEW"

# ── 2. MAIN binary (fresh target) ───────────────────────────────────────
for pair in "main:$MSRC:$BMAIN_D:build-main.log"; do
  IFS=: read -r who tree bin log <<< "$pair"
  [ -d "$tree" ] || fail_all "ABORT-BUILD no $tree"
  T0=$(date +%s)
  ( cd "$tree" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/$log" 2>&1 \
    || fail_all "ABORT-BUILD $who tree"
  cp "$tree/target/release/raptorpath" "$bin" || fail_all "ABORT-BUILD $who copy"
  cp "$bin" "$SHM/bin/$who/raptorpath" || fail_all "ABORT-BUILD $who copy to tmpfs"
  era "TSK-ALL $who binary built wall=$(( $(date +%s) - T0 ))s"
done
TSK_SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
{ echo "$TSK_SHA_NEW  $BNEW  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$TSK_SHA_MAIN  $BMAIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"; } > "$RUN/BINSHA.txt"
era "TSK-ALL binaries new=$TSK_SHA_NEW main=$TSK_SHA_MAIN"
export TSK_SHA_NEW TSK_SHA_MAIN
[ "$TFAIL" = "0" ] || fail_all "ABORT-TESTS (TESTS.txt)"

BENV=(TSK_SHA_NEW="$TSK_SHA_NEW" TSK_SHA_MAIN="$TSK_SHA_MAIN"
      TSK_LOCK_OWNER="$TSK_LOCK_OWNER" TSK_OUTDIR="$RUN"
      TSK_BIN_NEW="$BNEW" TSK_BIN_MAIN="$BMAIN")

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
T0=$(date +%s)
stage sudo -n env "${BENV[@]}" TSK_TAG=smoke TSK_SEEDS=42 TSK_SMOKE_PLAN="$SMOKE_PLAN" \
    bash ./threadsk_battery.sh 1
SRC_RC=$?
era "TSK-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./threadsk_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./threadsk_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "TSK-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
AVAIL=$(( SOFT - $(date +%s) ))
N=$(( AVAIL > 0 ? AVAIL / (2 * R_EST) : 0 ))
[ "$N" -gt "$N_MAX" ] && N=$N_MAX
[ "$N" -lt 2 ] && fail_all "ABORT-BUDGET n=$N (avail=${AVAIL}s R_est=${R_EST}s)"
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_EST}s cells=c1s-400,c1d-400,c2-100,c8-100 arms=MAIN,NEW env=RWM_RTOBS=1"
  echo "soft=$SOFT hard=$TSK_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "TSK-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
T0=$(date +%s)
era "TSK-ALL battery start cotenants: $(cotenants)"
stage sudo -n env "${BENV[@]}" TSK_TAG=tsk TSK_SEEDS="42 7" \
    TSK_SOFT_DEADLINE="$SOFT" TSK_BLOCK_EST_S="$R_EST" bash ./threadsk_battery.sh "$N"
BRC=$?
era "TSK-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/tsk.log" ] && grep -aq "TSK-BATTERY-DONE" "$RUN/tsk.log" \
    && python3 ./threadsk_parse.py check "$RUN/tsk.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./threadsk_parse.py score "$RUN/tsk.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/tsk.log" 2>/dev/null && SOFT_TRUNC=1
era "TSK-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') $(cotenants)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "TSK-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
