#!/bin/bash
# THE BLOCK RE-TEST'S 5 h ENVELOPE (docs/status.md §4 pre-registration;
# docs/measurement-discipline.md "The five-hour cap"). Same shape as
# place_run_all.sh.
#
#   BR_SHA=<sha256> BR_REPS=3 BR_HARD_DEADLINE=<epoch> \
#     setsid nohup bash blockretest_run_all.sh > <outdir>/launch.out 2>&1 < /dev/null &
#
# STARTED AS `vibe`, NOT ROOT: every sentinel is proven writable and written
# by the unprivileged user; `sudo -n` is used for the measurement stages only.
#
# THE ENVELOPE, STATED BEFORE ANYTHING RUNS:
#   1. both operator locks are taken HERE, as vibe, for the whole session and
#      carry the token `blockretest_run_all:<pid>`; the battery verifies the
#      token (BR_LOCK_OWNER) instead of taking them, so no other session can
#      slip in between the crown spot and the seeds;
#   2. order (pre-registered): the crown spot (crownspot8.sh, its own
#      sentinels under $OUTDIR/crown), then seed 42 reps 1..BR_REPS, then
#      seed 7 reps 1..BR_REPS;
#   3. SOFT deadline = BR_HARD_DEADLINE - 10 min: the battery starts no rep
#      that its measured per-rep cost (BR_REP_EST_S) would carry past it
#      (TRUNCATED-AT-REP-BOUNDARY, a balanced stop); seed 7 is not started if
#      the soft deadline is already inside one rep;
#   4. the HARD backstop at BR_HARD_DEADLINE TERMs the in-flight stage by its
#      recorded pid, `sudo pkill -x raptorpath || true`, waits for the rp-*
#      namespaces to clear, then writes FAILED-ALL-TRUNCATED-5H-BUDGET (the
#      5 h cap, not a battery failure);
#   5. DONE-S<seed> is EARNED: the ledger carries BLOCKRETEST-BATTERY-DONE
#      AND `blockretest_parse.py check` returns 0. DONE-ALL only when both
#      seeds earned DONE, no truncation of any kind happened, and the crown
#      spot wrote its own DONE-ALL; otherwise FAILED-ALL (or the truncation
#      sentinel), with the cause in all-era.txt.
#
# DISCIPLINE 13: launch, then wait on the sentinels only. Never poll the
# ledgers or the process table of a running battery.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh blockretest_run_all.sh blockretest_battery.sh \
    blockretest_parse.py crownspot8.sh tail_matrix.sh perf_rwm_c.sh
OUTDIR="${BR_OUTDIR:-/home/vibe/blockretest/run}"
REPS="${BR_REPS:-3}"
: "${BR_SHA:?BR_SHA (expected binary sha256) is required}"
: "${BR_HARD_DEADLINE:?BR_HARD_DEADLINE (epoch seconds) is required}"
REP_EST_S="${BR_REP_EST_S:-2400}"
SOFT_DEADLINE=$(( BR_HARD_DEADLINE - 600 ))
BIN="${RWM_BIN:-$(cd "$HERE/../../.." && pwd)/target/release/raptorpath}"
LAUNCH_TS=$(date +%s)
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$OUTDIR" "$OUTDIR/crown" 2>/dev/null

probe_sentinel "$OUTDIR/all.out"
exec > >(tee -a "$OUTDIR/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
DONE-S42 FAILED-S42 DONE-S7 FAILED-S7 SKIPPED-S7-5H-BUDGET SKIPPED-S7-S42-FAILED \
br-s42.log br-s7.log"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO reps=$REPS sha=$BR_SHA hard=$BR_HARD_DEADLINE" \
  prove_sentinels "$OUTDIR" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do
  case "$f" in br-s*.log) ;; *) rm -f "$OUTDIR/$f" ;; esac
done
FIN="$OUTDIR/.br-run-all-finished"
STAGE_PIDFILE="$OUTDIR/.br-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="blockretest_run_all:$$"
LB_LOG="$OUTDIR/all-era.txt"
echo "BR-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-/home/vibe/rp.lock}"
export BR_LOCK_OWNER="blockretest_run_all:$$"

SHA_NOW="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ "$SHA_NOW" != "$BR_SHA" ]; then
  echo "ABORT-SHA envelope: $SHA_NOW != $BR_SHA" | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-ALL"
  exit 5
fi
echo "BR-ALL binary $BIN sha256 $SHA_NOW reps=$REPS rep_est=${REP_EST_S}s soft=$SOFT_DEADLINE hard=$BR_HARD_DEADLINE (now $LAUNCH_TS)"

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$BR_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "BR-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by blockretest_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $BR_HARD_DEADLINE; the 5 h cap, not a battery failure"; } > "$OUTDIR/TRUNCATED.txt"
  local spid; spid=$(cat "$STAGE_PIDFILE" 2>/dev/null)
  [ -n "$spid" ] && sudo -n kill -TERM "$spid" 2>/dev/null
  sudo -n pkill -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 36); do
    [ "$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ] && break
    sleep 5
  done
  touch "$OUTDIR/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "BR-ALL truncated $(date -u +%FT%TZ)" >> "$OUTDIR/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; release_locks; }
trap finish EXIT

# ── 1. THE CROWN SPOT ────────────────────────────────────────────────────
T0=$(date +%s)
echo "BR-ALL crown spot start $(date -u +%FT%TZ)"
CROWNSPOT_OUT="$OUTDIR/crown" RWM_BIN="$BIN" bash ./crownspot8.sh > "$OUTDIR/crown/run.out" 2>&1 &
echo $! > "$STAGE_PIDFILE"
wait "$(cat "$STAGE_PIDFILE")"
echo "BR-ALL crown spot rc=$? wall=$(( $(date +%s) - T0 ))s done=$([ -f "$OUTDIR/crown/DONE-ALL" ] && echo 1 || echo 0) $(date -u +%FT%TZ)" | tee -a "$OUTDIR/all-era.txt"
rm -f "$STAGE_PIDFILE"

# ── 2. THE SEEDS ─────────────────────────────────────────────────────────
run_seed() {
  local s="$1" t0 rc
  t0=$(date +%s)
  echo "BR-ALL invoke seed=$s reps=$REPS $(date -u +%FT%TZ)"
  sudo -n env BR_SHA="$BR_SHA" BR_LOCK_OWNER="$BR_LOCK_OWNER" BR_OUTDIR="$OUTDIR" \
      BR_SOFT_DEADLINE="$SOFT_DEADLINE" BR_REP_EST_S="$REP_EST_S" RWM_BIN="$BIN" \
      bash ./blockretest_battery.sh "$s" "$REPS" &
  echo $! > "$STAGE_PIDFILE"
  wait "$(cat "$STAGE_PIDFILE")"
  rc=$?
  rm -f "$STAGE_PIDFILE"
  echo "BR-ALL seed=$s rc=$rc wall=$(( $(date +%s) - t0 ))s since_launch=$(( $(date +%s) - LAUNCH_TS ))s $(date -u +%FT%TZ)" \
    | tee -a "$OUTDIR/all-era.txt"
  local led="$OUTDIR/br-s$s.log"
  if [ "$rc" = "0" ] && python3 ./blockretest_parse.py check "$led"; then
    seed_done "$s" "$led" "BLOCKRETEST-BATTERY-DONE seed=$s" BR-ALL
  else
    echo "BR-ALL seed $s NOT EARNED: rc=$rc or parser check failed" | tee -a "$OUTDIR/all-era.txt"
    touch "$OUTDIR/FAILED-S$s"
  fi
}

RAN=""
run_seed 42; RAN="42"
if [ -f "$OUTDIR/TRUNCATED.txt" ]; then
  touch "$OUTDIR/SKIPPED-S7-5H-BUDGET"
elif [ ! -f "$OUTDIR/DONE-S42" ]; then
  touch "$OUTDIR/SKIPPED-S7-S42-FAILED"
elif [ $(( $(date +%s) + REP_EST_S )) -gt "$SOFT_DEADLINE" ]; then
  echo "BR-ALL seed 7 SKIPPED: not one rep fits before the soft deadline"
  touch "$OUTDIR/SKIPPED-S7-5H-BUDGET"
else
  run_seed 7; RAN="$RAN 7"
fi

echo "BR-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg) ran='$RAN'" >> "$OUTDIR/all-era.txt"
SOFT_TRUNC=0
grep -aqh "TRUNCATED-AT-REP-BOUNDARY" "$OUTDIR"/br-s*.log 2>/dev/null && SOFT_TRUNC=1
if [ -f "$OUTDIR/TRUNCATED.txt" ] || [ "$SOFT_TRUNC" -eq 1 ] || [ -f "$OUTDIR/SKIPPED-S7-5H-BUDGET" ]; then
  sleep 5
  echo "BR-ALL-TRUNCATED ran='$RAN' soft=$SOFT_TRUNC (score what is balanced and say so)" | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-ALL-TRUNCATED-5H-BUDGET"
  exit 6
fi
ALL_OK=1
for s in 42 7; do [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0; done
[ -f "$OUTDIR/crown/DONE-ALL" ] || { ALL_OK=0; echo "BR-ALL crown spot did not earn DONE-ALL" | tee -a "$OUTDIR/all-era.txt"; }
if [ "$ALL_OK" -eq 1 ]; then
  touch "$OUTDIR/DONE-ALL"; echo "BR-ALL-DONE ran='$RAN'"
else
  touch "$OUTDIR/FAILED-ALL"; echo "BR-ALL-FAILED ran='$RAN'"; exit 5
fi
