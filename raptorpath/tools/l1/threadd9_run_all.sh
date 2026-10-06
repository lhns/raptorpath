#!/bin/bash
# THE D9 ENVELOPE (docs/status.md, "15. D9 attribution and the c8 lag
# re-check -- pre-registration"; docs/measurement-discipline.md "The five-hour
# cap"). Derived from the V-Q2 envelope (threadq2_run_all.sh).
#
#   TD9_ROOT=/home/vibe/d9run TD9_HARD_DEADLINE=<epoch> \
#     setsid nohup bash threadd9_run_all.sh > $TD9_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced harness tree ($TD9_ROOT/src/raptorpath/tools/l1). The
# two engine trees are $TD9_ROOT/main-src (git archive of 88476a4) and
# $TD9_ROOT/nod9-src (git archive of measure/nod9), each with a COMMIT file.
# STARTED AS `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. parser tests (python only);
#   2. MAIN and NOD9: `cargo build --release --bin raptorpath`, each in its own
#      fresh target dir -> bin/main, bin/nod9, sha256 (BINSHA.txt). No cargo
#      test suite is run: the engine arms are an unmodified main and a 7-line
#      revert (stated in the pre-registration);
#   3. smoke (one invocation per arm at c1s-400 and c8-100, seed 42) ->
#      SMOKE-PASS or ABORT-SMOKE;
#   4. budget -> PLAN.txt; 5. battery (threadd9_battery.sh, tag td9);
#   6. score.
#
# SENTINELS: DONE-ALL only when the ledger carries TD9-BATTERY-DONE, `check`
# returns 0 and no truncation happened. FAILED-ALL (cause in all-era.txt);
# FAILED-ALL-TRUNCATED-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadd9_run_all.sh threadd9_battery.sh threadd9_parse.py \
    threadq1_parse.py threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${TD9_ROOT:?TD9_ROOT}"
: "${TD9_HARD_DEADLINE:?TD9_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
MSRC="$ROOT/main-src"
NSRC="$ROOT/nod9-src"
BNOD9="$ROOT/bin/nod9/raptorpath"
BMAIN="$ROOT/bin/main/raptorpath"
SOFT=$(( TD9_HARD_DEADLINE - 600 ))
R_PRIOR=240          # s per (rep, seed) block of 8 invocations (the 30 s/invocation prior)
C_PRED=60            # s, the smoke plan's predicted summed invocation wall (4 invocations)
N_FULL=3             # full reps per seed (pre-registered n = 6 per arm and cell)
N_EXTRA=3            # extra c8-only reps per seed (pre-registered c8 n = 12)
SMOKE_PLAN="c1s-400:MAIN c1s-400:NOD9 c8-100:NOD9 c8-100:MAIN"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$ROOT/bin/nod9" "$ROOT/bin/main" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE PLAN.txt BINSHA.txt build-main.log build-nod9.log \
smoke.log smoke-check.txt td9.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$TD9_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.td9-finished"
STAGE_PIDFILE="$RUN/.td9-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "TD9-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="threadd9_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "TD9-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$TD9_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export TD9_LOCK_OWNER="threadd9_run_all:$$"
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$TD9_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "TD9-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by threadd9_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $TD9_HARD_DEADLINE; the budget cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
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
  echo "TD9-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
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

# ── 1. PARSER TESTS ──────────────────────────────────────────────────────
: > "$RUN/TESTS.txt"
( cd "$HERE" && stage bash -c "python3 test_l1common.py && python3 test_stage3_parse.py && python3 test_threadq1_parse.py && python3 test_threadd9_parse.py" ) > "$RUN/test-parsers.log" 2>&1
PRC=$?
echo "TEST parsers rc=$PRC" >> "$RUN/TESTS.txt"
tail -3 "$RUN/test-parsers.log" >> "$RUN/TESTS.txt"
era "TD9-ALL parser tests rc=$PRC"
if [ "$PRC" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fail_all "ABORT-TESTS (TESTS.txt)"; fi

# ── 2. THE TWO BINARIES (fresh targets) ──────────────────────────────────
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
for pair in "main:$MSRC:$BMAIN:build-main.log" "nod9:$NSRC:$BNOD9:build-nod9.log"; do
  IFS=: read -r who tree bin log <<< "$pair"
  [ -d "$tree" ] || fail_all "ABORT-BUILD no $tree"
  [ ! -e "$tree/target" ] || fail_all "ABORT-BUILD $tree/target is not fresh"
  T0=$(date +%s)
  era "TD9-ALL $who build start commit=$(cat "$tree/COMMIT" 2>/dev/null) cotenants: $(cotenants)"
  ( cd "$tree" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/$log" 2>&1 \
    || fail_all "ABORT-BUILD $who tree"
  cp "$tree/target/release/raptorpath" "$bin" || fail_all "ABORT-BUILD $who copy"
  era "TD9-ALL $who binary built wall=$(( $(date +%s) - T0 ))s"
done
TD9_SHA_MAIN="$(sha256sum "$BMAIN" | cut -d' ' -f1)"
TD9_SHA_NOD9="$(sha256sum "$BNOD9" | cut -d' ' -f1)"
{ echo "$TD9_SHA_MAIN  $BMAIN  commit=$(cat "$MSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$TD9_SHA_NOD9  $BNOD9  commit=$(cat "$NSRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"; } > "$RUN/BINSHA.txt"
era "TD9-ALL binaries main=$TD9_SHA_MAIN nod9=$TD9_SHA_NOD9"
[ "$TD9_SHA_MAIN" != "$TD9_SHA_NOD9" ] || fail_all "ABORT-BUILD the two binaries are byte-identical"
export TD9_SHA_NOD9 TD9_SHA_MAIN

BENV=(TD9_SHA_NOD9="$TD9_SHA_NOD9" TD9_SHA_MAIN="$TD9_SHA_MAIN"
      TD9_LOCK_OWNER="$TD9_LOCK_OWNER" TD9_OUTDIR="$RUN"
      TD9_BIN_NOD9="$BNOD9" TD9_BIN_MAIN="$BMAIN")

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
T0=$(date +%s)
stage sudo -n env "${BENV[@]}" TD9_TAG=smoke TD9_SEEDS=42 TD9_SMOKE_PLAN="$SMOKE_PLAN" \
    bash ./threadd9_battery.sh 1
SRC_RC=$?
era "TD9-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./threadd9_parse.py smoke "$RUN/smoke.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./threadd9_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "TD9-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
# Full blocks: 2 seeds x N_FULL x R_EST; extra c8-only blocks (2 of 8
# invocations): 2 seeds x N_EXTRA x R_EST/4. Prior-based, conservative.
AVAIL=$(( SOFT - $(date +%s) ))
NEED=$(( 2 * N_FULL * R_EST + 2 * N_EXTRA * R_EST / 4 ))
[ "$AVAIL" -lt "$NEED" ] && fail_all "ABORT-BUDGET avail=${AVAIL}s need=${NEED}s (R_est=${R_EST}s)"
{ echo "full_reps_per_seed=$N_FULL extra_c8_reps_per_seed=$N_EXTRA seeds=42,7 block_est=${R_EST}s cells=c1s-400,c1d-400,c2-100,c8-100 arms=MAIN,NOD9 env=RWM_RTOBS=1"
  echo "soft=$SOFT hard=$TD9_HARD_DEADLINE now=$(date +%s) avail=$AVAIL need=$NEED c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "TD9-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
T0=$(date +%s)
era "TD9-ALL battery start cotenants: $(cotenants)"
stage sudo -n env "${BENV[@]}" TD9_TAG=td9 TD9_SEEDS="42 7" TD9_FULL_REPS="$N_FULL" TD9_EXTRA_CELLS=c8-100 \
    TD9_SOFT_DEADLINE="$SOFT" TD9_BLOCK_EST_S="$R_EST" bash ./threadd9_battery.sh $(( N_FULL + N_EXTRA ))
BRC=$?
era "TD9-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/td9.log" ] && grep -aq "TD9-BATTERY-DONE" "$RUN/td9.log" \
    && python3 ./threadd9_parse.py check "$RUN/td9.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
python3 ./threadd9_parse.py score "$RUN/td9.log" > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/td9.log" 2>/dev/null && SOFT_TRUNC=1
era "TD9-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-') load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && [ "$SOFT_TRUNC" -eq 1 ]; then
  touch "$RUN/FAILED-ALL-TRUNCATED-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ]; then
  touch "$RUN/DONE-ALL"; echo "TD9-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK"
