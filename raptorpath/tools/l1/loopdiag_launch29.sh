#!/bin/bash
# THE PLAN-29 WAITER (docs/status.md the power store law, plan 29). Runs on the VM as `vibe`, from
# $LD_ROOT/src/raptorpath/tools/l1:
#
#   LD_ROOT=/home/vibe/n1/ld LD_SHM=/dev/shm/ld [LD_WAIT_S=14400] \
#     setsid nohup bash loopdiag_launch29.sh > $LD_ROOT/launch.out 2>&1 < /dev/null &
#
# Waits (polling every 30 s) until the precondition holds, then `exec`s
# loopdiag_run_all.sh with LD_PLAN=29, which takes both locks itself.
# Precondition (shared rules):
#   * both operator locks are free, checked with `test -e` ONLY (this script never
#     opens, flocks or writes a lock path);
#   * no cargo / rustc process (pgrep -xc);
#   * `procs_running` in /proc/stat <= 2 in each of 5 samples taken 2 s apart
#     (the reader itself is one running process).
# Gives up with WAITER-EXPIRED after LD_WAIT_S seconds (default 14400).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
: "${LD_ROOT:?LD_ROOT}"
: "${LD_SHM:?LD_SHM (a /dev/shm dir)}"
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
WAIT_S="${LD_WAIT_S:-14400}"
POLL_S=30
T0=$(date +%s)

procs_running() { awk '/^procs_running/{print $2}' /proc/stat; }

# 0 = the precondition holds right now.
precondition() {
  test -e "$VM_LOCK" && return 1
  test -e "$RP_LOCK" && return 1
  [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -eq 0 ] || return 1
  local i pr
  for i in 1 2 3 4 5; do
    pr="$(procs_running)"
    [ -n "$pr" ] && [ "$pr" -le 2 ] || return 1
    [ "$i" -lt 5 ] && sleep 2
  done
  return 0
}

echo "WAITER start $(date -u +%FT%TZ) wait_s=$WAIT_S vm_lock=$VM_LOCK rp_lock=$RP_LOCK"
while :; do
  if precondition; then
    echo "WAITER precondition holds $(date -u +%FT%TZ) waited=$(( $(date +%s) - T0 ))s load=$(cut -d' ' -f1-3 /proc/loadavg); exec loopdiag_run_all.sh LD_PLAN=29"
    LD_PLAN=29 exec bash "$HERE/loopdiag_run_all.sh"
    echo "ABORT-EXEC could not exec loopdiag_run_all.sh"; exit 3
  fi
  if [ "$(( $(date +%s) - T0 ))" -ge "$WAIT_S" ]; then
    echo "WAITER-EXPIRED after ${WAIT_S}s $(date -u +%FT%TZ) vm_lock=$(test -e "$VM_LOCK" && echo held || echo free) rp_lock=$(test -e "$RP_LOCK" && echo held || echo free) cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) procs_running=$(procs_running)"
    exit 4
  fi
  sleep "$POLL_S"
done
