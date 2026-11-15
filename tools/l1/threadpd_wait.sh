#!/bin/bash
# Status §32: the on-VM waiter for the V-PD battery. Polls every 10 min
# (`test -e` on both locks, no cargo/rustc) and launches threadpd_run_all.sh
# with a hard deadline of launch + 4 h; a refusal before anything ran
# (rc 3 precondition, rc 4 lock) is retried until TPW_GIVE_UP (epoch).
#
#   TPD_ROOT=/home/vibe/pdrun TPW_GIVE_UP=<epoch> \
#     setsid nohup bash threadpd_wait.sh > $TPD_ROOT/waiter.out 2>&1 < /dev/null &
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
: "${TPD_ROOT:?TPD_ROOT}"
: "${TPW_GIVE_UP:?TPW_GIVE_UP}"
say() { echo "TPW $* $(date -u +%FT%TZ)" | tee -a "$TPD_ROOT/waiter.txt"; }
free() {
  ! test -e /tmp/rwm-vm.lock && ! test -e /home/vibe/rp.lock \
    && [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -eq 0 ]
}
say "start"
while :; do
  if [ "$(date +%s)" -ge "$TPW_GIVE_UP" ]; then say "GAVE-UP"; exit 3; fi
  if free; then
    HARD=$(( $(date +%s) + 4 * 3600 ))
    say "launch hard=$HARD"
    ( cd "$HERE" && TPD_ROOT="$TPD_ROOT" TPD_HARD_DEADLINE="$HARD" bash ./threadpd_run_all.sh ) \
      >> "$TPD_ROOT/launch.out" 2>&1 < /dev/null
    rc=$?
    say "run_all rc=$rc"
    if [ "$rc" != 3 ] && [ "$rc" != 4 ]; then
      touch "$TPD_ROOT/TPW-DONE"; exit 0
    fi
  fi
  sleep 600
done
