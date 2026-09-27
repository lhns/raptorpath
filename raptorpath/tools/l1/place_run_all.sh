#!/bin/bash
# THE PLACEMENT BATTERY'S 5 h ENVELOPE — the detached driver for
# `place_battery.sh` under goal-gate "OPERATOR AMENDMENT (2026-09-08 10:49Z):
# EVERY MEASUREMENT IS CAPPED AT 5 HOURS". Same shape as the receiver-law
# battery's `/home/vibe/recvlaw/run_all.sh` (goal-gate "Staged and NOT run"),
# committed here so the envelope is a file in the tree and not a VM-side
# script that has to be re-typed.
#
#   nohup bash place_run_all.sh >/home/vibe/placement/launch.out 2>&1 &
#
# THIS SCRIPT IS STARTED AS `vibe`, NOT AS ROOT, AND THAT IS THE POINT. It uses
# `sudo` for the battery invocations alone (the battery needs root for the
# rp-* namespaces) and does every sentinel operation as the UNPRIVILEGED user,
# so the sentinel writability it proves at launch is the writability the exit
# path will actually have.
#
# THE ENVELOPE, STATED BEFORE ANYTHING RUNS:
#
#   1. launch time is recorded (`PLACE-ALL start`);
#   2. seed 42 runs at n = 4 (the amendment's n; `RWM_PLACE_REPS` overrides);
#   3. seed 7 runs ONLY if seed 42 EARNED its DONE in under 2.5 h from launch
#      — otherwise `SKIPPED-S7-5H-BUDGET` (or `SKIPPED-S7-S42-FAILED` when
#      seed 42 did not earn DONE at all: a second seed of a failed first is
#      not a second seed);
#   4. a detached BACKSTOP at 4 h 50 min TERMs the battery (its fixed trap
#      releases both operator locks and exits 143 once the in-flight
#      invocation returns), `sudo pkill -x raptorpath || true` ends that
#      invocation, the rp-* namespaces are waited on to clear, and
#      `FAILED-ALL-TRUNCATED-5H-BUDGET` is written. That sentinel name is the
#      monitor's match pattern; it is NOT a battery failure and the scored
#      section must say so;
#   5. `DONE-ALL` is written ONLY when every seed this script actually RAN
#      earned its own DONE and the backstop never fired. A skipped seed 7 is
#      not a failed seed 7: `DONE-ALL` beside `SKIPPED-S7-5H-BUDGET` is the
#      expected end state at the placeholder cost (see the battery header:
#      ~3 h 10 min per seed at n = 4, so the 2.5 h gate is NOT met).
#
# RUNTIME ESTIMATE (placeholders, NOT a measurement of this grid): 83
# invocations/seed = 68 dual/single x 2.0 min + 15 quad x 3.6 min = 190 min.
# One seed fits under the backstop with ~1 h 40 min of slack; each DNF costs
# ~12 min of it (`timeout 700` in perf_rwm_c.sh).
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (121 RUN-RETRY over 171
# polled invocations against 0 over 80 unpolled). Launch this, then WAIT.
# Watch the SENTINELS, never the process table: `pgrep -f place_battery.sh`
# matches the watcher's own shell whenever its command line carries the string.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1 || { echo "ABORT-CD tools/l1"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard place_run_all.sh place_battery.sh lib_battery.sh
OUTDIR="${RWM_PLACE_OUTDIR:-/home/vibe/placement}"
TAG="${RWM_PLACE_TAG:-place}"
REPS="${RWM_PLACE_REPS:-4}"
SEEDS="42 7"
SEED7_GATE_S=$(( 5 * 1800 ))        # 2 h 30 min from launch
BACKSTOP_S=$(( 4 * 3600 + 50 * 60 )) # 4 h 50 min from launch
LAUNCH_TS=$(date +%s)
LAUNCH_ISO=$(date -u +%FT%TZ)

# ── SENTINEL WRITABILITY IS PROVEN AT LAUNCH, NOT DISCOVERED AT EXIT ──────
# THE RECORDED DEFECT THIS CLOSES: the hold-down sweep's launcher ran to
# completion and wrote NO sentinel at all — the output directory was owned by
# ROOT, the `touch` ran as the unprivileged user, and the script carried
# `set -uo pipefail` WITHOUT `-e`. The fix is to PROVE the write, as the user
# who will perform it, on the exact ABSOLUTE paths, BEFORE anything privileged
# runs. THE RUN DIRECTORY IS CREATED UNPRIVILEGED, HERE, BEFORE `sudo` IS EVER
# INVOKED — that is the whole point of the ordering.
mkdir -p "$OUTDIR" 2>/dev/null

# Probe the PATH, not the directory (lib_battery.sh `probe_sentinel`).

# `all.out` (the run log) IS PROVED FIRST AND THE TEE IS OPENED ONLY
# AFTERWARDS: opening the transcript before proving it would send the abort
# message that explains the failure into the file the failure is about.
probe_sentinel "$OUTDIR/all.out"
exec > >(tee -a "$OUTDIR/all.out") 2>&1

SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
DONE-S42 FAILED-S42 DONE-S7 FAILED-S7 SKIPPED-S7-5H-BUDGET SKIPPED-S7-S42-FAILED"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO reps=$REPS seeds='$SEEDS'" prove_sentinels "$OUTDIR" $SENTINELS

# Only now anything that needs privilege — and `sudo` must not prompt under
# nohup, or the battery hangs at a password prompt nobody can see.
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi

# A RELAUNCH MUST NOT INHERIT A PREVIOUS ERA'S SENTINELS. The battery
# truncates its own per-seed ledger at its top (`: > "$OUT"`), so the ledgers
# need no clearing; the sentinels and the era file do.
for f in $SENTINELS; do rm -f "$OUTDIR/$f"; done
FIN="$OUTDIR/.place-run-all-finished"
rm -f "$FIN"

echo "PLACE-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
echo "PLACE-ALL grid: arms=5 cells='c7 c8L c1 c9h' reps=$REPS (c9h capped at 3) singles='sc2 sc3' => 83 invocations/seed at reps=4"
echo "PLACE-ALL envelope: seed7_gate=${SEED7_GATE_S}s backstop=${BACKSTOP_S}s"

# ── THE BACKSTOP ─────────────────────────────────────────────────────────
# Detached from the seed loop so it fires whatever the loop is blocked on.
# The order is the amendment's own: TERM the driver (its trap runs when the
# in-flight invocation returns), end the in-flight engine, let perf_rwm_c.sh
# tear its namespaces down, THEN write the sentinel — a sentinel written while
# rp-* namespaces still exist would tell the next launcher the box is clear
# when it is not.
backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    if [ $(( $(date +%s) - LAUNCH_TS )) -ge "$BACKSTOP_S" ]; then break; fi
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "PLACE-ALL BACKSTOP fired $now (+$(( $(date +%s) - LAUNCH_TS ))s >= ${BACKSTOP_S}s)"
  {
    echo "TRUNCATED by place_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; budget ${BACKSTOP_S}s; the 5 h operator cap, not a battery failure"
  } > "$OUTDIR/TRUNCATED.txt"
  # 1. TERM the battery: the fixed INT/TERM trap releases both operator locks
  #    and exits 143 once the foreground invocation returns.
  sudo pkill -TERM -f 'bash place_battery.sh' 2>/dev/null || true
  # 2. End the in-flight invocation so that return happens now.
  sudo pkill -x raptorpath 2>/dev/null || true
  # 3. Wait for the rp-* namespaces to clear (perf_rwm_c.sh's own teardown).
  local i
  for i in $(seq 1 36); do
    if [ "$(sudo ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ]; then
      echo "PLACE-ALL namespaces clear after $(( i * 5 ))s"
      break
    fi
    sleep 5
  done
  if [ "$(sudo ip netns list 2>/dev/null | grep -c '^rp-')" -gt 0 ]; then
    echo "PLACE-ALL NS-STILL-PRESENT after 180s: $(sudo ip netns list 2>/dev/null | grep '^rp-' | tr '\n' ' ')"
  fi
  # 4. The sentinel, last.
  touch "$OUTDIR/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "PLACE-ALL truncated $(date -u +%FT%TZ)" >> "$OUTDIR/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; }
trap finish EXIT

# A SENTINEL IS EARNED, NOT UNCONDITIONAL: the ledger must EXIST, be
# NON-EMPTY, and carry the battery's own terminal line.
# (lib_battery.sh `seed_done`)

run_seed() {
  local s="$1" t0 rc
  t0=$(date +%s)
  echo "PLACE-ALL invoke seed=$s reps=$REPS $(date -u +%FT%TZ)"
  # `sudo` HERE AND NOWHERE ELSE: the battery needs root for the rp-*
  # namespaces; every sentinel path above and below is touched as `vibe`.
  sudo env RWM_PLACE_TAG="$TAG" RWM_PLACE_OUTDIR="$OUTDIR" bash place_battery.sh "$s" "$REPS"
  rc=$?
  echo "PLACE-ALL seed=$s rc=$rc wall=$(( $(date +%s) - t0 ))s elapsed_since_launch=$(( $(date +%s) - LAUNCH_TS ))s $(date -u +%FT%TZ)" \
    | tee -a "$OUTDIR/all-era.txt"
  seed_done "$s" "$OUTDIR/$TAG-s$s.log" "PLACE-BATTERY-DONE seed=$s" PLACE-ALL
}

RAN=""
run_seed 42; RAN="42"

ELAPSED=$(( $(date +%s) - LAUNCH_TS ))
if [ -f "$OUTDIR/TRUNCATED.txt" ]; then
  echo "PLACE-ALL seed 7 SKIPPED: backstop fired during seed 42"
  touch "$OUTDIR/SKIPPED-S7-5H-BUDGET"
elif [ ! -f "$OUTDIR/DONE-S42" ]; then
  echo "PLACE-ALL seed 7 SKIPPED: seed 42 did not earn DONE (elapsed ${ELAPSED}s)"
  touch "$OUTDIR/SKIPPED-S7-S42-FAILED"
elif [ "$ELAPSED" -ge "$SEED7_GATE_S" ]; then
  echo "PLACE-ALL seed 7 SKIPPED: seed 42 took ${ELAPSED}s >= ${SEED7_GATE_S}s (the 2.5 h gate)"
  touch "$OUTDIR/SKIPPED-S7-5H-BUDGET"
else
  echo "PLACE-ALL seed 42 done in ${ELAPSED}s < ${SEED7_GATE_S}s: seed 7 runs"
  run_seed 7; RAN="$RAN 7"
fi

echo "PLACE-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg) ran='$RAN'" >> "$OUTDIR/all-era.txt"

# ── THE VERDICT SENTINEL: EARNED BY EVERY SEED THAT RAN ──────────────────
if [ -f "$OUTDIR/TRUNCATED.txt" ]; then
  # The backstop owns this outcome's sentinel; make sure it is there even if
  # the backstop was still waiting on namespaces when the loop returned.
  sleep 5
  touch "$OUTDIR/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "PLACE-ALL-TRUNCATED (5 h budget; ran='$RAN'; score what the ledgers hold and say so)"
  exit 6
fi
ALL_OK=1
for s in $RAN; do
  [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0
done
if [ "$ALL_OK" -eq 1 ]; then
  touch "$OUTDIR/DONE-ALL"
  echo "PLACE-ALL-DONE ran='$RAN'"
else
  touch "$OUTDIR/FAILED-ALL"
  echo "PLACE-ALL-FAILED ran='$RAN'"
  exit 5
fi
