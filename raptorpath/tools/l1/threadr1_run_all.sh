#!/bin/bash
# THE V-R1 ENVELOPE (docs/status.md, "17. Receiver per-datagram cost (R1) —
# pre-registration"; docs/measurement-discipline.md "The five-hour cap").
# Derived from the V-Q2 envelope (threadq2_run_all.sh).
#
#   TR1_ROOT=/home/vibe/r1run TR1_HARD_DEADLINE=<epoch> \
#     setsid nohup bash threadr1_run_all.sh > $TR1_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced R1 tree ($TR1_ROOT/src/raptorpath/tools/l1); the MAIN
# tree (archive of ce60079) is at $TR1_ROOT/main-src, with a COMMIT file.
# STARTED AS `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. R1 tree: `cargo build --release`; tests (release: `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`; `cargo test --doc -p raptorpath --release`;
#      `cargo test -p raptorpath-wasm`; DEBUG: `cargo test -p raptorpath
#      --no-fail-fast -- --test-threads=2`, the run with the owner identity
#      witness; the python parser tests); `cargo build --release --bin
#      raptorpath` -> bin/r1/raptorpath, sha256;
#   2. MAIN tree: `cargo build --release --bin raptorpath` (fresh target) ->
#      bin/main, sha256;
#   3. smoke (one invocation per arm at c1s-400 and c8-100, seed 42) ->
#      SMOKE-PASS or ABORT-SMOKE. A test failure is ABORT-TESTS.
#   4. budget -> PLAN.txt; 5. battery (threadr1_battery.sh, tag tr1); 6. score.
#
# SENTINELS: DONE-ALL only when the ledger carries TR1-BATTERY-DONE, `check`
# returns 0 and no truncation happened. FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadr1_run_all.sh threadr1_battery.sh threadr1_parse.py \
    threadq1_parse.py threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${TR1_ROOT:?TR1_ROOT}"
: "${TR1_HARD_DEADLINE:?TR1_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
MSRC="$ROOT/main-src"
BR1="$ROOT/bin/r1/raptorpath"
BMAIN="$ROOT/bin/main/raptorpath"
SOFT=$(( TR1_HARD_DEADLINE - 600 ))
R_PRIOR=240          # s per (rep, seed) block of 8 invocations (the 30 s/invocation prior)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall (4 invocations)
N_MAX=3              # reps per seed (pre-registered n = 6 per arm and cell)
SMOKE_PLAN="c1s-400:MAIN c1s-400:R1 c8-100:R1 c8-100:MAIN"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin/r1" "$ROOT/bin/main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE PLAN.txt BINSHA.txt build.log build-main.log \
smoke.log smoke-check.txt tr1.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$TR1_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.tr1-finished"
STAGE_PIDFILE="$RUN/.tr1-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "TR1-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="threadr1_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "TR1-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$TR1_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export TR1_LOCK_OWNER="threadr1_run_all:$$"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$TR1_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "TR1-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by threadr1_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $TR1_HARD_DEADLINE; the budget cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
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
  echo "TR1-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
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

# ── 1. BUILD + TESTS (R1 tree) ───────────────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "TR1-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "TR1-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
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
  era "TR1-ALL test $name rc=$rc $sum"
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
tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_l1common.py && python3 test_stage3_parse.py && python3 test_threadq1_parse.py && python3 test_threadq2_parse.py && python3 test_threadr1_parse.py" || TFAIL=1
# The P1 lock-order witness went with the scheduler mutex (threading R1);
# the owner identity witness is the one left (its panic message counted).
echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BR1" || fail_all "ABORT-BUILD copy"
TR1_SHA_R1="$(sha256sum "$BR1" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "TR1-ALL tests done tfail=$TFAIL r1 sha256=$TR1_SHA_R1"

# ── 2. MAIN binary (fresh target) ───────────────────────────────────────
for pair in "main:$MSRC:$BMAIN:build-main.log"; do
  IFS=: read -r who tree bin log <<< "$pair"
  [ -d "$tree" ] || fail_all "ABORT-BUILD no $tree"
  T0=$(date +%s)
  ( cd "$tree" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/$log" 2>&1 \
    || fail_all "ABORT-BUILD $who tree"
  cp "$tree/target/release/raptorpath" "$bin" || fail_all "ABORT-BUILD $who copy"
  era "TR1-ALL $who binary built wall=$(( $(date +%s) - T0 ))s"
done
TR1_SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
{ echo "$TR1_SHA_R1  $BR1  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$TR1_SHA_MAIN  $BMAIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"; } > "$RUN/BINSHA.txt"
era "TR1-ALL binaries r1=$TR1_SHA_R1 main=$TR1_SHA_MAIN"
export TR1_SHA_R1 TR1_SHA_MAIN
[ "$TFAIL" = "0" ] || fail_all "ABORT-TESTS (TESTS.txt)"

BENV=(TR1_SHA_R1="$TR1_SHA_R1" TR1_SHA_MAIN="$TR1_SHA_MAIN"
      TR1_LOCK_OWNER="$TR1_LOCK_OWNER" TR1_OUTDIR="$RUN"
      TR1_BIN_R1="$BR1" TR1_BIN_MAIN="$BMAIN")

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
T0=$(date +%s)
stage sudo -n env "${BENV[@]}" TR1_TAG=smoke TR1_SEEDS=42 TR1_SMOKE_PLAN="$SMOKE_PLAN" \
    bash ./threadr1_battery.sh 1
SRC_RC=$?
era "TR1-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./threadr1_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./threadr1_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "TR1-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
AVAIL=$(( SOFT - $(date +%s) ))
N=$(( AVAIL > 0 ? AVAIL / (2 * R_EST) : 0 ))
[ "$N" -gt "$N_MAX" ] && N=$N_MAX
[ "$N" -lt 2 ] && fail_all "ABORT-BUDGET n=$N (avail=${AVAIL}s R_est=${R_EST}s)"
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_EST}s cells=c1s-400,c1d-400,c2-100,c8-100 arms=MAIN,R1 env=RWM_RTOBS=1"
  echo "soft=$SOFT hard=$TR1_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "TR1-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
T0=$(date +%s)
era "TR1-ALL battery start cotenants: $(cotenants)"
stage sudo -n env "${BENV[@]}" TR1_TAG=tr1 TR1_SEEDS="42 7" \
    TR1_SOFT_DEADLINE="$SOFT" TR1_BLOCK_EST_S="$R_EST" bash ./threadr1_battery.sh "$N"
BRC=$?
era "TR1-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/tr1.log" ] && grep -aq "TR1-BATTERY-DONE" "$RUN/tr1.log" \
    && python3 ./threadr1_parse.py check "$RUN/tr1.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./threadr1_parse.py score "$RUN/tr1.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/tr1.log" 2>/dev/null && SOFT_TRUNC=1
era "TR1-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "TR1-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
