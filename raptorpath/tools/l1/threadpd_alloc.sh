#!/bin/bash
# Status §32 Amendment 1 (tests, not a battery): recv_alloc_bound five times
# on each of BASE / PD / FIX, then the full suites on NEW. Detached WAITER:
# waits for both operator locks free and no cargo/rustc (`test -e` only,
# every 10 min), never breaks a lock.
#
#   TPA_ROOT=/home/vibe/pdalloc TPA_WAIT_UNTIL=<epoch> \
#     setsid nohup bash threadpd_alloc.sh > $TPA_ROOT/launch.out 2>&1 < /dev/null &
#
# Trees (git archive, LF, each with a COMMIT file): $TPA_ROOT/{base,pd,fix};
# the suites run on $TPA_ROOT/new (the branch head).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
ROOT="${TPA_ROOT:?TPA_ROOT}"
: "${TPA_WAIT_UNTIL:?TPA_WAIT_UNTIL (epoch seconds) is required}"
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$ROOT/era.txt"; }
free() {
  ! test -e "$VM_LOCK" && ! test -e "$RP_LOCK" \
    && [ "$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))" -eq 0 ]
}
wait_free() {
  until free; do
    [ "$(date +%s)" -ge "$TPA_WAIT_UNTIL" ] && { era "TPA GAVE-UP waiting for the locks"; exit 3; }
    sleep 600
  done
}
crlf_guard "$ROOT/new/raptorpath/tools/l1/suites_run.sh"
era "TPA start base=$(cat "$ROOT/base/COMMIT") pd=$(cat "$ROOT/pd/COMMIT") fix=$(cat "$ROOT/fix/COMMIT") new=$(cat "$ROOT/new/COMMIT")"

arm() { # name
  local name="$1" tree="$ROOT/$1" i
  export CARGO_TARGET_DIR="$ROOT/tgt-$name"
  ( cd "$tree" && timeout 3600 "$CARGO" test -p raptorpath --release --test recv_alloc_bound --no-run ) \
    > "$ROOT/build-$name.log" 2>&1 || { era "TPA $name build FAILED"; rm -rf "$CARGO_TARGET_DIR"; return; }
  for i in 1 2 3 4 5; do
    ( cd "$tree" && timeout 600 "$CARGO" test -p raptorpath --release --test recv_alloc_bound -- --nocapture ) \
      > "$ROOT/$name-$i.log" 2>&1
    era "TPA $name run=$i rc=$? $(grep -a -o 'RECV_ALLOC.*' "$ROOT/$name-$i.log" | tail -1)"
  done
  rm -rf "$CARGO_TARGET_DIR"
}

wait_free
(
  LB_TAG="threadpd_alloc:$$"
  LB_LOG="$ROOT/era.txt"
  install_lock_traps
  take_lock "$VM_LOCK"
  take_lock "$RP_LOCK"
  for a in base pd fix; do arm "$a"; done
)
[ $? = 4 ] && era "TPA lock race lost; retrying" && { wait_free; exec bash "$0"; }

while :; do
  wait_free
  SU_ROOT="$ROOT/suites" SU_SRC="$ROOT/new" bash "$ROOT/new/raptorpath/tools/l1/suites_run.sh"
  rc=$?
  [ "$rc" = 4 ] || [ "$rc" = 3 ] || break
  era "TPA suites refused rc=$rc (lock or cargo); waiting"
done
era "TPA done suites_rc=$rc raptorpath=$(pgrep -xc raptorpath)"
touch "$ROOT/TPA-DONE"
