#!/bin/bash
# §22 (docs/status.md): detached waiter for the R3 step-0 session. Polls the
# launch precondition (§22 Amendment 3, the shared rule: both locks absent
# by `test -e` only, no cargo / rustc, `procs_running` in /proc/stat ≤ 2 in
# each of 5 samples 2 s apart) about once a minute for up to R3D_WAIT_S
# (default 3 h), then runs $R3D_SCRIPT (default r3diag_run.sh), which re-checks it and takes the
# locks with take_lock. A run that refused before taking the locks
# (precondition rc 3, a lock taken in between rc 4, and no BUILD line) is
# retried; anything else ends the waiter. Never touches a lock.
#
#   R3D_ROOT=<root> setsid nohup bash r3diag_wait.sh > <root>/wait.out 2>&1 < /dev/null &
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
R="${R3D_ROOT:?R3D_ROOT}"
end=$(( $(date +%s) + ${R3D_WAIT_S:-10800} ))
log() { echo "$* $(date -u +%FT%TZ)" >> "$R/waiter.txt"; }
procs_running() { awk '$1 == "procs_running" { print $2 }' /proc/stat; }
ok() {
  [ ! -e /tmp/rwm-vm.lock ] && [ ! -e /home/vibe/rp.lock ] \
    && [ "$(pgrep -xc cargo)" = 0 ] && [ "$(pgrep -xc rustc)" = 0 ] || return 1
  local _
  for _ in 1 2 3 4 5; do
    [ "$(procs_running)" -le 2 ] || return 1
    sleep 2
  done
  return 0
}
log "WAIT start end=$end"
n=0
while [ "$(date +%s)" -lt "$end" ]; do
  if ok; then
    n=$(( n + 1 ))
    log "LAUNCH attempt=$n procs_running=$(procs_running) load=$(cut -d' ' -f1-3 /proc/loadavg)"
    H=$(( $(date +%s) + 10800 ))
    ( cd "$HERE" && R3D_ROOT="$R" R3D_HARD_DEADLINE=$H bash "${R3D_SCRIPT:-r3diag_run.sh}" ) > "$R/launch-$n.out" 2>&1 < /dev/null
    rc=$?
    if { [ "$rc" = 3 ] || [ "$rc" = 4 ]; } && ! grep -q "^BUILD " "$R/run/era.txt" 2>/dev/null; then
      log "REFUSED-BEFORE-LOCKS rc=$rc (retrying)"
      mv "$R/run" "$R/refused-$n" 2>/dev/null
      sleep 60
      continue
    fi
    log "RUN-ENDED rc=$rc"
    exit 0
  fi
  sleep 50
done
log "NOT-LAUNCHED precondition not met before the wait ended procs_running=$(procs_running)"
