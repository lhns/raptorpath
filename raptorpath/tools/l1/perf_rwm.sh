#!/bin/bash
# RWM Phase B measurement: `raptorpath perf` (rp-native objects) over the
# RELIABLE sliding-window pipeline (--window-reliable), single OR dual path,
# on the dual topology. Apples-to-apples: same binary, same topology, the
# path count is the only variable.
#
#   sudo bash perf_rwm.sh <scenA> <scenB> <hint> <bytes> <runs> <dual|single>
#
#   dual   = stripe over path A (scenA) + path B (scenB) via place_symbol law
#   single = fast-path-alone: only path A (scenA) — the §16.2 resequencing
#            ceiling reference
#
#   C7 = c2 c2   C8 = c2 c3
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
SCENA="${1:?scenA}"; SCENB="${2:?scenB}"; HINT="${3:-bulk}"
BYTES="${4:-1800000}"; RUNS="${5:-10}"; MODE="${6:-dual}"; PLACE_T="${7:-}"

# Optional RWM placement-temperature override (§16.3 dial) for the sweep.
TENV="$(rwm_forward_env)"   # gate forwarding: ONE shared list in lib.sh
[[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

# Refuse to start if a raptorpath measurement is already running.
if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running — aborting" >&2
    exit 3
fi

bash ./topo_dual.sh up "$SCENA" "$SCENB" --seed 42 >/dev/null 2>&1

if [[ "$MODE" == "dual" ]]; then
    SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"
    PEERS="10.77.0.2:7000,10.78.0.2:7000"
    CLI_BIND="10.77.0.1:0,10.78.0.1:0"
else
    SRV_BIND="10.77.0.2:7000"
    PEERS="10.77.0.2:7000"
    CLI_BIND="10.77.0.1:0"
fi

ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable --protocol-hint "$HINT" >/tmp/rwm-s.log 2>&1 &

# Wait for the server UDP socket (until-loop, hard-capped).
for _ in $(seq 1 20); do
    ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break
    sleep 0.3
done
sleep 1

echo "--- RWM perf mode=$MODE hint=$HINT A=$SCENA B=$SCENB T=${PLACE_T:-default} ($BYTES x $RUNS) start=$(date +%T)"
timeout 700 ip netns exec "$NS_CLI" env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" \
    --window-reliable --protocol-hint "$HINT" \
    --bytes "$BYTES" --runs "$RUNS" 2>&1 \
    | grep -E "summary|warmup|dnf" | tail -6 \
    || echo "{\"dnf\":true,\"mode\":\"$MODE\"}"
echo "    done $(date +%T)"
echo "--- server log tail:"; sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | tail -3
