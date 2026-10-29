#!/bin/bash
# Status §31 tests (not a measurement): the red/green of the per-drain prune
# tests, then the full suites on PD. Detached WAITER on the VM: it waits for
# both operator locks to be free and no cargo/rustc (polls every 10 min,
# `test -e` only), never breaks a lock.
#
#   TPT_ROOT=/home/vibe/pdtests TPT_WAIT_UNTIL=<epoch> \
#     setsid nohup bash threadpd_tests.sh > $TPT_ROOT/launch.out 2>&1 < /dev/null &
#
# Trees (git archive, LF): $TPT_ROOT/wit = the witness-only commit (prune
# still on the 2 s tick), $TPT_ROOT/pd = PD. Each has a COMMIT file.
# RED stage (wit) and GREEN stage (pd), release, fresh target dirs: the
# `fec::unified` lib tests, `receiver_prunes_per_drain`, `prune_per_drain_loopback`.
# Then tools/l1/suites_run.sh on pd (it takes the locks itself).
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
ROOT="${TPT_ROOT:?TPT_ROOT}"
: "${TPT_WAIT_UNTIL:?TPT_WAIT_UNTIL (epoch seconds) is required}"
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
    [ "$(date +%s)" -ge "$TPT_WAIT_UNTIL" ] && { era "TPT GAVE-UP waiting for the locks"; exit 3; }
    sleep 600
  done
}
crlf_guard "$ROOT/wit/raptorpath/tools/l1/lib_battery.sh" "$ROOT/pd/raptorpath/tools/l1/suites_run.sh"
era "TPT start wit=$(cat "$ROOT/wit/COMMIT") pd=$(cat "$ROOT/pd/COMMIT")"

stage() { # name tree
  local name="$1" tree="$2" t0 rc
  t0=$(date +%s)
  ( cd "$tree" || exit 3
    export CARGO_TARGET_DIR="$ROOT/tgt-$name"
    r=0
    timeout 5400 "$CARGO" test -p raptorpath --release --lib -- fec::unified --test-threads=1 || r=1
    timeout 1800 "$CARGO" test -p raptorpath --release --test receiver_prunes_per_drain || r=1
    timeout 1800 "$CARGO" test -p raptorpath --release --test prune_per_drain_loopback -- --test-threads=1 || r=1
    exit $r ) > "$ROOT/$name.log" 2>&1
  rc=$?
  era "TPT $name rc=$rc wall=$(( $(date +%s) - t0 ))s $(grep -a '^test result:' "$ROOT/$name.log" | awk '{p+=$4; f+=$6} END{printf "passed=%d failed=%d", p, f}')"
  grep -a -E '^test .* (ok|FAILED)$' "$ROOT/$name.log" > "$ROOT/$name-tests.txt"
  rm -rf "$ROOT/tgt-$name"
}

wait_free
(
  LB_TAG="threadpd_tests:$$"
  LB_LOG="$ROOT/era.txt"
  install_lock_traps
  take_lock "$VM_LOCK"
  take_lock "$RP_LOCK"
  stage red "$ROOT/wit"
  stage green "$ROOT/pd"
)
[ $? = 4 ] && era "TPT red/green lock race lost; retrying" && { wait_free; exec bash "$0"; }

while :; do
  wait_free
  SU_ROOT="$ROOT/suites" SU_SRC="$ROOT/pd" bash "$ROOT/pd/raptorpath/tools/l1/suites_run.sh"
  rc=$?
  [ "$rc" = 4 ] || [ "$rc" = 3 ] || break
  era "TPT suites refused rc=$rc (lock or cargo); waiting"
done
era "TPT done suites_rc=$rc raptorpath=$(pgrep -xc raptorpath)"
touch "$ROOT/TPT-DONE"
