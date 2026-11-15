#!/bin/bash
# Bulk kernel TCP INSIDE the tunnel: one warm tunnel per invocation, REPS
# cold TCP object transfers through it (transfer_bench.py `server`/`client`,
# a fresh TCP connection per rep, completion including a 1-byte app ack).
#
#   sudo env RWM_BIN=<bin> SEED=<seed> bash tun_bulk.sh <cell> <hint> <bytes> <reps> [label]
#
# The tunnel is brought up exactly as `tail_matrix.sh` `run_arm` does it
# (topo.sh `up <cell>`, `raptorpath run` on both ends, TUN 10.99.0.1/.2, a
# ping gate, 3 bring-up attempts), with NO `--window-reliable`: the binary
# picks its own pipeline for the hint. That is the point of the cell: since
# ADR-0069 Bulk/Auto ride the window pipeline and the TUN MTU is clamped to
# symbol_size - 4 = 1196, where the deleted block pipeline left it at 1500.
#
# Witnesses (docs/measurement-discipline.md rules 1 and 15), one line each,
# for the parser (verify4_parse.py `tun`):
#   TUNMTU   the MTU of rpcli0 / rpsrv0 as the kernel reports it in each netns
#   TUNPIPE  the `[PIPE]` echo of each endpoint (pipeline/backend/hint)
#   TUNCAD   the estimator-cadence echo state of each endpoint
#   TUNSHA   the binary's sha256
# Per rep one `TUNREP {json}` line (the client's per-run JSON, or
# {"nodata": true}) and a final `TUNARM-DONE`. Every rep is bounded by
# `timeout`; nothing can wedge the caller. Env withheld from the binary
# (rule 15d): RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST,
# unless the caller exports them (rwm_forward_env re-adds those): env-unset
# is the binary's shipped default, emission batching ON since the flip
# (status §8 "Flipped in"); a per-symbol arm passes RWM_EMIT_BATCH=0, as
# verify4_run_all.sh does.
set -uo pipefail
cd "$(dirname "$0")" || { echo "ABORT-CD $(dirname "$0")"; exit 3; }
source ./lib.sh
set +e
BIN="${RWM_BIN:?RWM_BIN}"
CELL="${1:?cell}"; HINT="${2:?hint}"; BYTES="${3:?bytes}"; REPS="${4:?reps}"
LABEL="${5:-$HINT}"
SEED="${SEED:-42}"
TB_TMO="${RWM_TB_TIMEOUT_S:-90}"

TBS_PID=""
hard_cleanup() {
    stop_raptorpath
    # the bench server by its own pid (no pattern kill; measurement-discipline
    # VM protocol: process control is `pkill -x raptorpath` and nothing broader)
    [ -n "$TBS_PID" ] && kill "$TBS_PID" 2>/dev/null
    TBS_PID=""
    ip netns del "$NS_CLI" 2>/dev/null || true
    ip netns del "$NS_SRV" 2>/dev/null || true
}
on_exit() { local rc=$?; hard_cleanup; echo "EXIT rc=$rc ($(date +%T))"; }
trap on_exit EXIT

cad_state() { # log -> ACTIVE / OFF / NONE
    if grep -aq 'estimator heavy-math cadence ACTIVE' "$1" 2>/dev/null; then echo ACTIVE
    elif grep -aq 'estimator heavy-math cadence OFF' "$1" 2>/dev/null; then echo OFF
    else echo NONE; fi
}
pipe_of() { # log -> "pipeline/backend/hint" or "-"
    local l
    l=$(sed 's/\x1b\[[0-9;]*m//g' "$1" 2>/dev/null | grep -ao '\[PIPE\] pipeline=[a-z]* backend=[A-Za-z]* hint=[a-z]*' | tail -1)
    [ -z "$l" ] && { echo "-"; return; }
    echo "$l" | sed 's/\[PIPE\] pipeline=\([a-z]*\) backend=\([A-Za-z]*\) hint=\([a-z]*\)/\1\/\2\/\3/'
}

echo "TUNARM label=$LABEL cell=$CELL hint=$HINT bytes=$BYTES reps=$REPS seed=$SEED bin=$BIN start=$(date -u +%FT%TZ)"
echo "TUNSHA $(sha256sum "$BIN" | cut -d' ' -f1)"
up=0
for attempt in 1 2 3; do
    hard_cleanup; sleep 1
    bash ./topo.sh up "$CELL" --seed "$SEED" >/dev/null 2>&1 || true
    # shellcheck disable=SC2046
    ip netns exec "$NS_SRV" env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        $(rwm_forward_env) "$BIN" run --server --bind 10.77.0.2:7000 \
        --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$HINT" \
        >/tmp/tb-s.log 2>&1 &
    sleep 2
    # shellcheck disable=SC2046
    ip netns exec "$NS_CLI" env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        $(rwm_forward_env) "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
        --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$HINT" \
        >/tmp/tb-c.log 2>&1 &
    for i in $(seq 1 20); do
        ip netns exec "$NS_CLI" ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
        sleep 1
    done
    [[ $up -eq 1 ]] && break
    echo "TUN-BRINGUP-RETRY label=$LABEL attempt=$attempt"
done
if [[ $up -eq 0 ]]; then
    echo "TUN-BRINGUP-FAIL label=$LABEL"
    echo "TUNARM-DONE label=$LABEL reps=0 $(date -u +%FT%TZ)"
    exit 0
fi
mtu_c=$(ip netns exec "$NS_CLI" cat /sys/class/net/rpcli0/mtu 2>/dev/null || echo "-")
mtu_s=$(ip netns exec "$NS_SRV" cat /sys/class/net/rpsrv0/mtu 2>/dev/null || echo "-")
echo "TUNMTU cli=${mtu_c:--} srv=${mtu_s:--}"
echo "TUNPIPE cli=$(pipe_of /tmp/tb-c.log) srv=$(pipe_of /tmp/tb-s.log)"
echo "TUNCAD cli=$(cad_state /tmp/tb-c.log) srv=$(cad_state /tmp/tb-s.log)"
ip netns exec "$NS_SRV" python3 ./transfer_bench.py server --bind 10.99.0.2 --port 9900 \
    >/tmp/tb-srv.log 2>&1 &
TBS_PID=$!
sleep 0.5
done_n=0
for r in $(seq 1 "$REPS"); do
    out=$(timeout "$TB_TMO" ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.99.0.2 --port 9900 --bytes "$BYTES" --runs 1 2>/dev/null)
    rc=$?
    line=$(echo "$out" | grep '"run": 1' | tail -1)
    if [[ -n "$line" ]]; then
        echo "TUNREP label=$LABEL rep=$r rc=$rc $line"
        done_n=$((done_n + 1))
    else
        echo "TUNREP label=$LABEL rep=$r rc=$rc {\"nodata\": true}"
    fi
done
echo "TUNARM-DONE label=$LABEL reps=$done_n $(date -u +%FT%TZ)"
exit 0
