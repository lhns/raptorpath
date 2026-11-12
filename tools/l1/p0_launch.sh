#!/bin/bash
# Lock waiter for the threading-redesign P0 envelope (docs/status.md §9). The
# VM is shared; `take_lock` refuses (exit 4) when a lock is held. This waits
# -- bounded by P0_WAIT_MAX seconds (default 3 h), polling every 5 s -- until
# both lock files are absent, then runs p0_run_all.sh. If the envelope lost
# the race (exit 4 with an ABORT-LOCK line and no SMOKE-PASS), it waits
# again. Never touches a lock it does not own.
#
#   P0_ROOT=/home/vibe/p0 setsid nohup bash p0_launch.sh > $P0_ROOT/waiter.out 2>&1 < /dev/null &
#
# Sentinels in $P0_ROOT: WAITER-STARTED, WAITER-LAUNCHED (each launch),
# WAITER-GAVE-UP (bound reached), WAITER-EXIT rc=<envelope rc>.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
ROOT="${P0_ROOT:?P0_ROOT}"
MAX="${P0_WAIT_MAX:-10800}"
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
T0=$(date +%s)
echo "WAITER-STARTED $(date -u +%FT%TZ) max=${MAX}s" > "$ROOT/WAITER-STARTED"
while :; do
  if [ ! -e "$VM_LOCK" ] && [ ! -e "$RP_LOCK" ]; then
    echo "LAUNCH $(date -u +%FT%TZ)" >> "$ROOT/WAITER-LAUNCHED"
    bash ./p0_run_all.sh > "$ROOT/launch.out" 2>&1 < /dev/null
    rc=$?
    if [ "$rc" = "4" ] && grep -aq "ABORT-LOCK" "$ROOT/launch.out" && [ ! -f "$ROOT/run/SMOKE-PASS" ]; then
      echo "LOST-RACE $(date -u +%FT%TZ)" >> "$ROOT/WAITER-LAUNCHED"
    else
      echo "WAITER-EXIT rc=$rc $(date -u +%FT%TZ)" > "$ROOT/WAITER-EXIT"
      exit 0
    fi
  else
    echo "WAIT $(date -u +%T) vm=$(head -c 80 "$VM_LOCK" 2>/dev/null) rp=$(head -c 80 "$RP_LOCK" 2>/dev/null)" >> "$ROOT/waiter.log"
  fi
  if [ $(( $(date +%s) - T0 )) -ge "$MAX" ]; then
    echo "WAITER-GAVE-UP $(date -u +%FT%TZ) after ${MAX}s" > "$ROOT/WAITER-GAVE-UP"
    exit 0
  fi
  sleep "${P0_WAIT_POLL:-5}"
done
