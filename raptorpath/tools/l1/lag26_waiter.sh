#!/bin/bash
# The detached on-VM waiter for the §26 session (agent rules: host-side
# watchers get killed for memory). Stage 1: the full suites (suites_run.sh);
# stage 2: the §26 diagnostic (lagdiag2_run.sh), only after stage 1 is green.
# Each stage is retried every 5 min while it exits 3 (precondition) or 4
# (a lock is held); the scripts themselves check the precondition and take
# the locks (take_lock only -- this waiter never touches a lock path).
#
#   W_ROOT=/home/vibe/lagdiag2 setsid nohup bash lag26_waiter.sh > $W_ROOT/waiter.out 2>&1 < /dev/null &
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
ROOT="${W_ROOT:?W_ROOT}"
GIVEUP=$(( $(date +%s) + ${W_MAX_WAIT_S:-36000} ))
log() { echo "$* $(date -u +%FT%TZ)" >> "$ROOT/waiter.txt"; }
stage() { # name cmd...
  local name="$1" rc; shift
  while :; do
    "$@"; rc=$?
    case "$rc" in
      3|4) log "WAIT $name rc=$rc load=$(cut -d' ' -f1 /proc/loadavg) io=[$(grep '^full' /proc/pressure/io | cut -d' ' -f2-3)]" ;;
      *) log "DONE $name rc=$rc"; return "$rc" ;;
    esac
    [ "$(date +%s)" -lt "$GIVEUP" ] || { log "GIVEUP $name"; return 99; }
    sleep 300
  done
}
log "WAITER start"
stage suites env SU_ROOT="$ROOT/suites" bash ./suites_run.sh || { log "STOP suites not green"; exit 1; }
stage lagdiag2 env LG2_ROOT="$ROOT" LG2_HARD_DEADLINE=0 bash -c 'LG2_HARD_DEADLINE=$(( $(date +%s) + 14400 )) exec bash ./lagdiag2_run.sh'
log "WAITER end"
