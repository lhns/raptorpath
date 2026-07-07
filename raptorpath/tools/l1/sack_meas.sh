#!/bin/bash
# feat/sack-flow-control measurement battery. Runs the arm set for whatever
# binary is currently built at target/release/raptorpath. One tunnel per arm
# (perf_rwm_c.sh brings topo up/down each call), hard 700s per-arm timeout in
# the harness. Label = $1 (BEFORE/AFTER). Reps = $2 (default 6).
set -uo pipefail
cd "$(dirname "$0")"
LABEL="${1:?label}"
REPS="${2:-6}"
BYTES=1800000

run() {
    local tag="$1"; shift
    if pgrep -x raptorpath >/dev/null 2>&1; then
        echo "[$LABEL/$tag] BUSY — waiting";
        for _ in $(seq 1 60); do pgrep -x raptorpath >/dev/null 2>&1 || break; sleep 1; done
    fi
    echo "########## [$LABEL] $tag ##########"
    timeout 260 "$@" 2>&1 | grep -E "summary|dnf|RWM-C|done" || echo "{\"dnf\":true,\"arm\":\"$tag\"}"
    echo
}

# c2 single (THE headline): plain reliable, in-order
run "c2-single-plain"    sudo bash perf_rwm_c.sh c2 c2 bulk $BYTES $REPS single
# c2 single with OUT-OF-ORDER object completion (sender now decoupled by SACK)
run "c2-single-ooo"      sudo RWM_OOO=1 bash perf_rwm_c.sh c2 c2 bulk $BYTES $REPS single
# clean single control (no-regression check)
run "clean-single"       sudo bash perf_rwm_c.sh clean clean bulk $BYTES $REPS single
# C7 (c2+c2 symmetric) dual, plain
run "c7-dual-plain"      sudo bash perf_rwm_c.sh c2 c2 bulk $BYTES $REPS dual
# C8 (c2+c3 heterogeneous) dual, plain
run "c8-dual-plain"      sudo bash perf_rwm_c.sh c2 c3 bulk $BYTES $REPS dual
# C8 dual, out-of-order object completion
run "c8-dual-ooo"        sudo RWM_OOO=1 bash perf_rwm_c.sh c2 c3 bulk $BYTES $REPS dual
# C8 dual, systematic-repair (G=384 default)
run "c8-dual-systematic-G384" sudo RWM_EXTRA="--window-systematic-repair" RWM_GEN=384 bash perf_rwm_c.sh c2 c3 bulk $BYTES $REPS dual
echo "########## [$LABEL] BATTERY DONE ##########"
