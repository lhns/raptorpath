#!/bin/bash
# THE V-P1 ENVELOPE (docs/status.md, "Threading P1 — pre-registration";
# docs/measurement-discipline.md "The five-hour cap").
#
#   TP1_ROOT=/home/vibe/tp1run TP1_HARD_DEADLINE=<epoch> \
#     setsid nohup bash threadp1_run_all.sh > $TP1_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced P1 tree ($TP1_ROOT/src/raptorpath/tools/l1); the MAIN
# tree (archive of main 8d7d8c1) is at $TP1_ROOT/main-src. STARTED AS `vibe`,
# NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. P1 tree: `cargo build --release`; tests (release: `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`, `cargo test --doc -p raptorpath --release`, `cargo
#      test -p raptorpath-wasm`; DEBUG: `cargo test -p raptorpath
#      --no-fail-fast -- --test-threads=2`, the run in which the lock-order
#      witness is compiled in — every in-process and spawned-binary test then
#      doubles as a lock-order audit); `cargo build --release --bin
#      raptorpath` -> bin/p1/raptorpath, sha256;
#   2. MAIN tree: `cargo build --release --bin raptorpath` (fresh target)
#      -> bin/main/raptorpath, sha256;
#   3. smoke (one invocation per arm at c1s-400 and c8-100, seed 42) ->
#      SMOKE-PASS or ABORT-SMOKE. A test failure is ABORT-TESTS (the battery
#      does not run). No operator GO gate: the pre-registration fixes the
#      plan, so SMOKE-PASS proceeds.
#   4. budget -> PLAN.txt; 5. battery (threadp1_battery.sh); 6. score.
#
# SENTINELS: DONE-ALL only when the ledger carries TP1-BATTERY-DONE, `check`
# returns 0 and no truncation happened. FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadp1_run_all.sh threadp1_battery.sh threadp1_parse.py \
    stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${TP1_ROOT:?TP1_ROOT}"
: "${TP1_HARD_DEADLINE:?TP1_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
MSRC="$ROOT/main-src"
BP1="$ROOT/bin/p1/raptorpath"
BMAIN="$ROOT/bin/main/raptorpath"
SOFT=$(( TP1_HARD_DEADLINE - 600 ))
R_PRIOR=240          # s per (rep, seed) block of 8 invocations (pre-registered prior)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall
N_MAX=3              # reps per seed (pre-registered n)
SMOKE_PLAN="c1s-400:MAIN c1s-400:P1 c8-100:P1 c8-100:MAIN"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin/p1" "$ROOT/bin/main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE PLAN.txt BINSHA.txt build.log build-main.log \
smoke.log smoke-check.txt tp1.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$TP1_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.tp1-finished"
STAGE_PIDFILE="$RUN/.tp1-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "TP1-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="threadp1_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "TP1-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$TP1_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export TP1_LOCK_OWNER="threadp1_run_all:$$"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$TP1_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "TP1-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by threadp1_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $TP1_HARD_DEADLINE; the budget cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
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
  echo "TP1-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
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

# ── 1. BUILD + TESTS (P1 tree) ───────────────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "TP1-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "TP1-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
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
  era "TP1-ALL test $name rc=$rc $sum"
  [ "$rc" = "0" ] && return 0
  # The pre-registered flake rule (as §8): a failure that passes on an
  # immediate re-run of that test alone (same command, `-- <name> --exact`)
  # is recorded FLAKE; any other failure is REAL and the stage fails.
  local real=0 t
  if ! grep -aq -E '^test .* FAILED$' "$RUN/test-$name.log"; then
    echo "REAL-FAILURE $name rc=$rc with no failing test named (build error?)" >> "$RUN/TESTS.txt"
    return 1
  fi
  for t in $(grep -a -E '^test .* FAILED$' "$RUN/test-$name.log" | awk '{print $2}' | sort -u); do
    local base=("$@")
    # drop everything from the first `--` on, then re-add the filter
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
grep -a '\[lockorder\]' "$RUN/test-debug-witness.log" >> "$RUN/TESTS.txt" 2>/dev/null
echo "LOCKORDER-PANICS $(grep -ac 'lock order: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BP1" || fail_all "ABORT-BUILD copy"
TP1_SHA_P1="$(sha256sum "$BP1" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "TP1-ALL tests done tfail=$TFAIL p1 sha256=$TP1_SHA_P1"

# ── 2. MAIN binary (fresh target) ────────────────────────────────────────
[ -d "$MSRC" ] || fail_all "ABORT-BUILD no $MSRC"
T0=$(date +%s)
( cd "$MSRC" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/build-main.log" 2>&1 \
  || fail_all "ABORT-BUILD main tree"
cp "$MSRC/target/release/raptorpath" "$BMAIN" || fail_all "ABORT-BUILD main copy"
TP1_SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
{ echo "$TP1_SHA_P1  $BP1  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$TP1_SHA_MAIN  $BMAIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"; } > "$RUN/BINSHA.txt"
era "TP1-ALL binaries p1=$TP1_SHA_P1 main=$TP1_SHA_MAIN main-build-wall=$(( $(date +%s) - T0 ))s"
export TP1_SHA_P1 TP1_SHA_MAIN
[ "$TFAIL" = "0" ] || fail_all "ABORT-TESTS (TESTS.txt)"

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
T0=$(date +%s)
stage sudo -n env TP1_SHA_P1="$TP1_SHA_P1" TP1_SHA_MAIN="$TP1_SHA_MAIN" TP1_LOCK_OWNER="$TP1_LOCK_OWNER" \
    TP1_OUTDIR="$RUN" TP1_TAG=smoke TP1_SEEDS=42 TP1_SMOKE_PLAN="$SMOKE_PLAN" \
    TP1_BIN_P1="$BP1" TP1_BIN_MAIN="$BMAIN" bash ./threadp1_battery.sh 1
SRC_RC=$?
era "TP1-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./threadp1_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./threadp1_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "TP1-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
AVAIL=$(( SOFT - $(date +%s) ))
N=$(( AVAIL > 0 ? AVAIL / (2 * R_EST) : 0 ))
[ "$N" -gt "$N_MAX" ] && N=$N_MAX
[ "$N" -lt 2 ] && fail_all "ABORT-BUDGET n=$N (avail=${AVAIL}s R_est=${R_EST}s)"
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_EST}s cells=c1s-400,c1d-400,c2-100,c8-100 arms=MAIN,P1"
  echo "soft=$SOFT hard=$TP1_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "TP1-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
T0=$(date +%s)
era "TP1-ALL battery start cotenants: $(cotenants)"
stage sudo -n env TP1_SHA_P1="$TP1_SHA_P1" TP1_SHA_MAIN="$TP1_SHA_MAIN" TP1_LOCK_OWNER="$TP1_LOCK_OWNER" \
    TP1_OUTDIR="$RUN" TP1_TAG=tp1 TP1_SEEDS="42 7" \
    TP1_SOFT_DEADLINE="$SOFT" TP1_BLOCK_EST_S="$R_EST" TP1_BIN_P1="$BP1" TP1_BIN_MAIN="$BMAIN" \
    bash ./threadp1_battery.sh "$N"
BRC=$?
era "TP1-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/tp1.log" ] && grep -aq "TP1-BATTERY-DONE" "$RUN/tp1.log" \
    && python3 ./threadp1_parse.py check "$RUN/tp1.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./threadp1_parse.py score "$RUN/tp1.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/tp1.log" 2>/dev/null && SOFT_TRUNC=1
era "TP1-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "TP1-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
