#!/bin/bash
# THE EMISSION-BATCHING SCOPE BATTERY'S 5 h ENVELOPE (docs/status.md §8
# pre-registration; docs/measurement-discipline.md "The five-hour cap").
#
#   ES_ROOT=/home/vibe/es ES_HARD_DEADLINE=<epoch> \
#     setsid nohup bash emitscope_run_all.sh > $ES_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree ($ES_ROOT/src/raptorpath/tools/l1). STARTED AS
# `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. build + tests on the tree: `cargo build --release`; `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`; `cargo test --doc -p raptorpath --release`; `cargo
#      test -p raptorpath-wasm` (GOLDEN_CAPTURE unset); then `cargo build
#      --release --bin raptorpath` -> $ES_ROOT/bin/raptorpath, sha256;
#   2. the red/green record: the T1/T6 loopback and the scope pin with
#      --nocapture on this tree (green), then on $ES_ROOT/red-src (the
#      gauges-and-tests commit with the path-count step still in place),
#      outputs to RED.txt -- a record, not a gate;
#   3. smoke (ES_SMOKE_PLAN, one invocation each) -> SMOKE-PASS or
#      ABORT-SMOKE (`emitscope_parse.py smoke`: every row LIVE with every
#      gauge, and a DUAL EB0 row with mean burst depth > 1);
#   4. GO: automatic iff TESTS-OK and SMOKE-PASS; otherwise the operator
#      writes GO or NOGO after reading TESTS.txt (<= 40 min; timeout =
#      FAILED-ALL);
#   5. budget -> PLAN.txt; 6. battery (emitscope_battery.sh); 7. score ->
#      score.txt.
#
# SENTINELS: DONE-ALL only when the battery ledger carries ES-BATTERY-DONE,
# `emitscope_parse.py check` returns 0 and no truncation happened.
# FAILED-ALL (cause in all-era.txt); FAILED-ALL-TRUNCATED-5H-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh emitscope_run_all.sh emitscope_battery.sh emitscope_parse.py \
    stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${ES_ROOT:?ES_ROOT}"
: "${ES_HARD_DEADLINE:?ES_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
BIN="$ROOT/bin/raptorpath"
SOFT=$(( ES_HARD_DEADLINE - 600 ))
R_PRIOR=120          # s per (rep, seed) block of 8 invocations (§8 budget)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall
SMOKE_PLAN="${ES_SMOKE_PLAN:-c8-100:EB0 c1s-400:NEW c1d-400:EB0 c2-100:NEW}"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL RED.txt SMOKE-PASS ABORT-SMOKE GO NOGO PLAN.txt BINSHA.txt build.log \
smoke.log smoke-check.txt es.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$ES_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.es-finished"
STAGE_PIDFILE="$RUN/.es-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "ES-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="emitscope_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "ES-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$ES_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export ES_LOCK_OWNER="emitscope_run_all:$$"
refresh_locks() { # stage-name
  local l
  for l in "$VM_LOCK" "$RP_LOCK"; do
    if ! grep -qF -- "$ES_LOCK_OWNER" "$l" 2>/dev/null; then
      era "LOCK-TRUNCATED-BY-FOREIGN $l before $1: '$(cat "$l" 2>/dev/null)'"
    fi
    echo "$$ $LB_TAG $(date -u +%FT%TZ)" > "$l"
  done
}
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$ES_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "ES-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by emitscope_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $ES_HARD_DEADLINE; the 5 h cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
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
  echo "ES-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
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

# ── 1. BUILD + TESTS ─────────────────────────────────────────────────────
refresh_locks build
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "ES-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "ES-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
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
  era "ES-ALL test $name rc=$rc $sum"
  return $rc
}
TFAIL=0
tst main "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
ES_SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
echo "$ES_SHA  $BIN  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)" > "$RUN/BINSHA.txt"
era "ES-ALL tests done tfail=$TFAIL sha256=$ES_SHA"
export ES_SHA

# ── 2. RED/GREEN RECORD (not a gate) ─────────────────────────────────────
refresh_locks redgreen
rg() { # tag dir
  local tag="$1" dir="$2"
  # a separate tree sharing the target dir: its archive mtimes predate the
  # green build, so force a rebuild (else cargo re-runs the green artefacts)
  find "$dir" -name '*.rs' -exec touch {} +
  {
    echo "=== $tag tree=$dir commit=$(cat "$dir/COMMIT" 2>/dev/null) $(date -u +%FT%TZ)"
    ( cd "$dir" && CARGO_TARGET_DIR="$SRC/target" stage "$CARGO" test --release -p raptorpath \
        --test emit_batch_scope_loopback -- --nocapture --test-threads=1 ) 2>&1 \
      | grep -a -E '^N=|mean burst depth|panicked|assert|^test |test result|eb_bursts' | head -60
    echo "--- rc(loopback)=${PIPESTATUS[0]}"
    ( cd "$dir" && CARGO_TARGET_DIR="$SRC/target" stage "$CARGO" test --release -p raptorpath --lib \
        emit_burst -- --nocapture --test-threads=1 ) 2>&1 \
      | grep -a -E 'panicked|^test |test result|path-count|emit_batch_live' | head -40
    ( cd "$dir" && CARGO_TARGET_DIR="$SRC/target" stage "$CARGO" test --release -p raptorpath --lib \
        t2_ -- --nocapture --test-threads=1 ) 2>&1 \
      | grep -a -E 'panicked|^test |test result' | head -20
  } >> "$RUN/RED.txt" 2>&1
}
rg GREEN "$SRC"
if [ -d "$ROOT/red-src" ]; then rg RED "$ROOT/red-src"; fi
era "ES-ALL red/green record done (RED.txt)"
[ "$(sha256sum "$BIN" | cut -d' ' -f1)" = "$ES_SHA" ] || fail_all "ABORT-SHA after the red/green record"

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
refresh_locks smoke
T0=$(date +%s)
stage sudo -n env ES_SHA="$ES_SHA" ES_LOCK_OWNER="$ES_LOCK_OWNER" \
    ES_OUTDIR="$RUN" ES_TAG=smoke ES_SEEDS=42 ES_SMOKE_PLAN="$SMOKE_PLAN" \
    ES_BIN="$BIN" bash ./emitscope_battery.sh 1
SRC_RC=$?
era "ES-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./emitscope_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./emitscope_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "ES-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. GO ────────────────────────────────────────────────────────────────
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
era "ES-ALL GO ($(cat "$RUN/GO"))"

# ── 5. BUDGET ────────────────────────────────────────────────────────────
plan_n() { local avail=$(( SOFT - $(date +%s) )); echo $(( avail > 0 ? avail / (2 * R_EST) : 0 )); }
N=$(plan_n); [ "$N" -gt 8 ] && N=8
[ "$N" -lt 3 ] && fail_all "ABORT-BUDGET n=$N"
BSOFT=$SOFT
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_EST}s arms='${ES_ARMS:-NEW EB0}'"
  echo "battery_soft_deadline=$BSOFT soft=$SOFT hard=$ES_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "ES-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 6. BATTERY ───────────────────────────────────────────────────────────
refresh_locks battery
T0=$(date +%s)
era "ES-ALL battery start cotenants: $(cotenants)"
stage sudo -n env ES_SHA="$ES_SHA" ES_LOCK_OWNER="$ES_LOCK_OWNER" \
    ES_OUTDIR="$RUN" ES_TAG=es ES_SEEDS="42 7" ES_ARMS="${ES_ARMS:-NEW EB0}" \
    ES_SOFT_DEADLINE="$BSOFT" ES_BLOCK_EST_S="$R_EST" ES_BIN="$BIN" \
    bash ./emitscope_battery.sh "$N"
BRC=$?
era "ES-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/es.log" ] && grep -aq "ES-BATTERY-DONE" "$RUN/es.log" \
    && python3 ./emitscope_parse.py check "$RUN/es.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 7. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./emitscope_parse.py score "$RUN/es.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/es.log" 2>/dev/null && SOFT_TRUNC=1
era "ES-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "ES-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
