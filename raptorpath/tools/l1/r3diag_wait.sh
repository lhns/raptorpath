#!/bin/bash
# §22 (docs/status.md): detached waiter for the R3 step-0 session. Polls the
# launch precondition (both locks absent by `test -e` only, loadavg 1-min
# < 2.0, no cargo / rustc, /proc/pressure/io `full avg60` < 5.0) once a
# minute for up to R3D_WAIT_S (default 3 h), then runs r3diag_run.sh, which
# re-checks it and takes the locks with take_lock. A run that refused before
# taking the locks (precondition rc 3, a lock taken in between rc 4, and no
# BUILD line) is retried; anything else ends the waiter. Never touches a lock.
#
#   R3D_ROOT=/home/vibe/r3diag setsid nohup bash r3diag_wait.sh \
#     > /home/vibe/r3diag/wait.out 2>&1 < /dev/null &
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
R="${R3D_ROOT:?R3D_ROOT}"
end=$(( $(date +%s) + ${R3D_WAIT_S:-10800} ))
log() { echo "$* $(date -u +%FT%TZ)" >> "$R/waiter.txt"; }
io60() { awk '$1 == "full" { split($3, b, "="); print b[2] }' /proc/pressure/io 2>/dev/null; }
ok() {
  [ ! -e /tmp/rwm-vm.lock ] && [ ! -e /home/vibe/rp.lock ] \
    && [ "$(pgrep -xc cargo)" = 0 ] && [ "$(pgrep -xc rustc)" = 0 ] \
    && awk -v l="$(cut -d' ' -f1 /proc/loadavg)" -v p="$(io60)" \
         'BEGIN { exit !(l < 2.0 && p != "" && p < 5.0) }'
}
log "WAIT start end=$end"
n=0
while [ "$(date +%s)" -lt "$end" ]; do
  if ok; then
    n=$(( n + 1 ))
    log "LAUNCH attempt=$n load=$(cut -d' ' -f1-3 /proc/loadavg) io_full_avg60=$(io60)"
    H=$(( $(date +%s) + 10800 ))
    ( cd "$HERE" && R3D_ROOT="$R" R3D_HARD_DEADLINE=$H bash r3diag_run.sh ) > "$R/launch-$n.out" 2>&1 < /dev/null
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
  sleep 60
done
log "NOT-LAUNCHED precondition not met before the wait ended load=$(cut -d' ' -f1-3 /proc/loadavg) io_full_avg60=$(io60)"
