#!/bin/bash
# Plan 67, session 0 (loopdiag_p67pre.sh, step locks): the smallest child that goes the way repeat67_run.sh / binid67_run.sh go. It sources lib_battery.sh, installs the lock traps and takes BOTH operator locks
# (RWM_VM_LOCK, RWM_RP_LOCK) with the LB_LOCK_PARENT inheritance, then releases (a no-op for an inherited lock) and exits. The parent requires `LOCK-INHERITED` for both locks in $LC67_LOG within 30 s: a child that
# cannot inherit the session's locks would refuse (exit 4) at the first real child, hours into the session.
#   LC67_LOG=<log file> RWM_VM_LOCK=<path> RWM_RP_LOCK=<path> LB_LOCK_PARENT=<session pid> bash lockcheck67.sh
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 3
source ./lib_battery.sh
LB_TAG="lockcheck67:$$"
LB_LOG="${LC67_LOG:?LC67_LOG}"
install_lock_traps
take_lock "${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
take_lock "${RWM_RP_LOCK:-$HOME/rp.lock}"
release_locks
exit 0
