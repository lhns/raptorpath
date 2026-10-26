#!/bin/bash
# THE R3 SHIP ENVELOPE (docs/status.md, "22. R3 ship battery — pre-registration";
# docs/measurement-discipline.md "The five-hour cap").
# Derived from the R2 envelope (threadr2_run_all.sh).
#
#   TR3_ROOT=/home/vibe/r3run TR3_HARD_DEADLINE=<epoch> \
#     setsid nohup bash threadr3_run_all.sh > $TR3_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced R3 tree ($TR3_ROOT/src/raptorpath/tools/l1); the MAIN
# tree (archive of github/main at launch) is at $TR3_ROOT/main-src, with a
# COMMIT file.
# STARTED AS `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. R3 tree: `cargo build --release`; tests (release: `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`; `cargo test --doc -p raptorpath --release`;
#      `cargo test -p raptorpath-wasm`; DEBUG: `cargo test -p raptorpath
#      --no-fail-fast -- --test-threads=2`, the run with the owner identity
#      witness; the python parser tests); `cargo build --release --bin
#      raptorpath` -> bin/r3/raptorpath, sha256;
#   1b. RED PROOF (before the R3 suites): the MAIN tree plus R3's integration
#      tests `decoder_alloc_bound` and `receiver_prunes_per_drain` (copied in)
#      must FAIL: RED-OK per test, RED-MISSING for a test that does not fail
#      on MAIN (recorded, not an abort). The R3 unit tests in fec/unified.rs
#      (zero copy, floor) are NOT copied in: they live in the module under
#      test and need the new code to compile, so they cannot run on MAIN; they
#      run green in the R3 `main` suite (GREEN-OK / GREEN-MISSING recorded);
#   2. MAIN tree: `cargo build --release --bin raptorpath` (fresh target) ->
#      bin/main, sha256;
#   3. smoke (one invocation per arm at c1s-400 and c8-100, seed 42) ->
#      SMOKE-PASS or ABORT-SMOKE. A test failure is ABORT-TESTS.
#   4. budget -> PLAN.txt; 5. battery (threadr3_battery.sh, tag tr3); 6. score.
#
# SENTINELS: DONE-ALL only when the ledger carries TR3-BATTERY-DONE, `check`
# returns 0 and no truncation happened. FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadr3_run_all.sh threadr3_battery.sh threadr3_parse.py \
    threadq1_parse.py threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${TR3_ROOT:?TR3_ROOT}"
: "${TR3_HARD_DEADLINE:?TR3_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
MSRC="$ROOT/main-src"
BR3="$ROOT/bin/r3/raptorpath"
BMAIN="$ROOT/bin/main/raptorpath"
SOFT=$(( TR3_HARD_DEADLINE - 600 ))
R_PRIOR=240          # s per (rep, seed) block of 8 invocations (the 30 s/invocation prior)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall (4 invocations)
N_MAX=3              # reps per seed (pre-registered n = 6 per arm and cell)
SMOKE_PLAN="c1s-400:MAIN c1s-400:R3 c8-100:R3 c8-100:MAIN"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin/r3" "$ROOT/bin/main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE PLAN.txt BINSHA.txt build.log build-main.log \
smoke.log smoke-check.txt test-redproof.log tr3.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$TR3_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.tr3-finished"
STAGE_PIDFILE="$RUN/.tr3-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "TR3-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
# Launch precondition (status 21, as 17 Amendment 2): 1-minute loadavg < 2.0 and
# no cargo / rustc, checked BEFORE the locks are taken (take_lock refuses a held
# lock; it never waits or breaks one). The loadavg is in the ledger.
LOAD1=$(cut -d' ' -f1 /proc/loadavg)
if ! awk -v l="$LOAD1" 'BEGIN{exit !(l < 2.0)}' || [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -ne 0 ]; then
  echo "ABORT-PRECONDITION load1=$LOAD1 cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc); NOTHING WAS RUN."
  exit 3
fi
LB_TAG="threadr3_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "TR3-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$TR3_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export TR3_LOCK_OWNER="threadr3_run_all:$$"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$TR3_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "TR3-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by threadr3_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $TR3_HARD_DEADLINE; the budget cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
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
  echo "TR3-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
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

# ── 1. BUILD + TESTS (R3 tree) ───────────────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "TR3-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "TR3-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
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
  era "TR3-ALL test $name rc=$rc $sum"
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

# ── 1b. RED PROOF on MAIN + R3's integration tests ──────────────────────
# The copied test files are removed afterwards (the tree stays the archive of
# the named commit). Its own target dir, removed afterwards, so the MAIN
# release binary is still built in a fresh target.
RED_TESTS="in_order_sources_cost_at_most_one_and_a_half_allocations_each the_decoder_prune_runs_at_the_wait_point_not_in_the_report_block rdiag_carries_the_decoder_state_witness"
GREEN_TESTS="$RED_TESTS in_order_output_shares_the_input_buffer a_source_below_the_floor_is_refused_and_counted"
red_proof() {
  [ -d "$MSRC/raptorpath/tests" ] || fail_all "ABORT-BUILD no $MSRC/raptorpath/tests"
  local f
  for f in decoder_alloc_bound receiver_prunes_per_drain; do
    cp "$SRC/raptorpath/tests/$f.rs" "$MSRC/raptorpath/tests/$f.rs" || fail_all "ABORT-BUILD red-proof copy $f"
  done
  ( cd "$MSRC" && export CARGO_TARGET_DIR="$ROOT/tgt-redproof" && stage "$CARGO" test -p raptorpath --release --no-fail-fast \
      --test decoder_alloc_bound --test receiver_prunes_per_drain -- --test-threads=2 ) > "$RUN/test-redproof.log" 2>&1
  local rc=$?
  for f in decoder_alloc_bound receiver_prunes_per_drain; do rm -f "$MSRC/raptorpath/tests/$f.rs"; done
  rm -rf "$ROOT/tgt-redproof"
  local t
  for t in $RED_TESTS; do
    if grep -aq "^test $t ... FAILED" "$RUN/test-redproof.log"; then
      echo "RED-OK $t (fails on MAIN)" >> "$RUN/TESTS.txt"
    elif grep -aq "^test $t ... ok" "$RUN/test-redproof.log"; then
      echo "RED-MISSING $t (PASSES on MAIN: it does not prove the change)" >> "$RUN/TESTS.txt"
    else
      echo "RED-MISSING $t (did not run on MAIN: rc=$rc, see test-redproof.log)" >> "$RUN/TESTS.txt"
    fi
  done
  era "TR3-ALL red proof rc=$rc: $(grep -a -c '^RED-OK' "$RUN/TESTS.txt") RED-OK, $(grep -a -c '^RED-MISSING' "$RUN/TESTS.txt") RED-MISSING"
}
red_proof

TFAIL=0
tst main "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
for t in $GREEN_TESTS; do
  if grep -aq "^test .*$t ... ok" "$RUN/test-main.log"; then echo "GREEN-OK $t" >> "$RUN/TESTS.txt"
  else echo "GREEN-MISSING $t (not ok in the R3 main suite)" >> "$RUN/TESTS.txt"; TFAIL=1; fi
done
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
tst debug-witness "$CARGO" test -p raptorpath --no-fail-fast -- --test-threads=2 || TFAIL=1
tst parsers bash -c "cd raptorpath/tools/l1 && python3 test_l1common.py && python3 test_stage3_parse.py && python3 test_threadq1_parse.py && python3 test_threadq2_parse.py && python3 test_threadr3_parse.py" || TFAIL=1
# The P1 lock-order witness went with the scheduler mutex (threading R2);
# the owner identity witness is the one left (its panic message counted).
echo "IDENTITY-PANICS $(grep -ac 'io owner: quinn seam' "$RUN/test-debug-witness.log")" >> "$RUN/TESTS.txt"
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BR3" || fail_all "ABORT-BUILD copy"
TR3_SHA_R3="$(sha256sum "$BR3" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "TR3-ALL tests done tfail=$TFAIL r3 sha256=$TR3_SHA_R3"

# ── 2. MAIN binary (fresh target) ───────────────────────────────────────
for pair in "main:$MSRC:$BMAIN:build-main.log"; do
  IFS=: read -r who tree bin log <<< "$pair"
  [ -d "$tree" ] || fail_all "ABORT-BUILD no $tree"
  T0=$(date +%s)
  ( cd "$tree" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/$log" 2>&1 \
    || fail_all "ABORT-BUILD $who tree"
  cp "$tree/target/release/raptorpath" "$bin" || fail_all "ABORT-BUILD $who copy"
  era "TR3-ALL $who binary built wall=$(( $(date +%s) - T0 ))s"
done
TR3_SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
{ echo "$TR3_SHA_R3  $BR3  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$TR3_SHA_MAIN  $BMAIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"; } > "$RUN/BINSHA.txt"
era "TR3-ALL binaries r3=$TR3_SHA_R3 main=$TR3_SHA_MAIN"
export TR3_SHA_R3 TR3_SHA_MAIN
[ "$TFAIL" = "0" ] || fail_all "ABORT-TESTS (TESTS.txt)"

BENV=(TR3_SHA_R3="$TR3_SHA_R3" TR3_SHA_MAIN="$TR3_SHA_MAIN"
      TR3_LOCK_OWNER="$TR3_LOCK_OWNER" TR3_OUTDIR="$RUN"
      TR3_BIN_R3="$BR3" TR3_BIN_MAIN="$BMAIN")

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
T0=$(date +%s)
stage sudo -n env "${BENV[@]}" TR3_TAG=smoke TR3_SEEDS=42 TR3_SMOKE_PLAN="$SMOKE_PLAN" \
    bash ./threadr3_battery.sh 1
SRC_RC=$?
era "TR3-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./threadr3_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./threadr3_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "TR3-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
AVAIL=$(( SOFT - $(date +%s) ))
N=$(( AVAIL > 0 ? AVAIL / (2 * R_EST) : 0 ))
[ "$N" -gt "$N_MAX" ] && N=$N_MAX
[ "$N" -lt 2 ] && fail_all "ABORT-BUDGET n=$N (avail=${AVAIL}s R_est=${R_EST}s)"
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_EST}s cells=c1s-400,c1d-400,c2-100,c8-100 arms=MAIN,R3 env=RWM_RTOBS=1"
  echo "soft=$SOFT hard=$TR3_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "TR3-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
T0=$(date +%s)
era "TR3-ALL battery start cotenants: $(cotenants)"
stage sudo -n env "${BENV[@]}" TR3_TAG=tr3 TR3_SEEDS="42 7" \
    TR3_SOFT_DEADLINE="$SOFT" TR3_BLOCK_EST_S="$R_EST" bash ./threadr3_battery.sh "$N"
BRC=$?
era "TR3-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/tr3.log" ] && grep -aq "TR3-BATTERY-DONE" "$RUN/tr3.log" \
    && python3 ./threadr3_parse.py check "$RUN/tr3.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./threadr3_parse.py score "$RUN/tr3.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/tr3.log" 2>/dev/null && SOFT_TRUNC=1
era "TR3-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "TR3-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
