#!/bin/bash
# Status §34: the on-VM waiter for the post-§32 c1 static grid (LD_PLAN=34; LDW_PLAN=36, 38, 40, 41 or 43 for the later sections).
# Polls every 10 min (`test -e` on both locks, no cargo/rustc) and launches
# loopdiag_run_all.sh with a hard deadline LD_HARD = launch + 4 h; a refusal
# before anything ran (rc 3 precondition, rc 4 lock) is retried until
# LDW_GIVE_UP (epoch).
#
#   LD_ROOT=/home/vibe/ld34 LD_SHM=/dev/shm/ld34 LDW_GIVE_UP=<epoch> \
#     setsid nohup bash loopdiag_wait34.sh > $LD_ROOT/waiter.out 2>&1 < /dev/null &
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
: "${LD_ROOT:?LD_ROOT}"
: "${LD_SHM:?LD_SHM}"
: "${LDW_GIVE_UP:?LDW_GIVE_UP}"
say() { echo "LDW $* $(date -u +%FT%TZ)" | tee -a "$LD_ROOT/waiter.txt"; }
free() {
  ! test -e /tmp/rwm-vm.lock && ! test -e /home/vibe/rp.lock \
    && [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -eq 0 ]
}
say "start"
while :; do
  if [ "$(date +%s)" -ge "$LDW_GIVE_UP" ]; then say "GAVE-UP"; exit 3; fi
  if free; then
    HARD=$(( $(date +%s) + 4 * 3600 ))
    say "launch hard=$HARD"
    ( cd "$HERE" && LD_ROOT="$LD_ROOT" LD_SHM="$LD_SHM" LD_PLAN="${LDW_PLAN:-34}" LD_HARD="$HARD" LD_PRIOR_TESTS="${LD_PRIOR_TESTS:-}" \
        timeout $(( 4 * 3600 + 600 )) bash ./loopdiag_run_all.sh ) >> "$LD_ROOT/launch.out" 2>&1 < /dev/null
    rc=$?
    say "run_all rc=$rc"
    if [ "$rc" != 3 ] && [ "$rc" != 4 ]; then
      touch "$LD_ROOT/LDW-DONE"; exit 0
    fi
  fi
  sleep 600
done
