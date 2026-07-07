#!/bin/bash
# Clean RWM measurement: brings up topo (single|dual), runs server+client,
# captures the FULL client JSON stream to a file, prints every summary line.
#   sudo bash meas_rwm.sh <scenA> <scenB> <bytes> <runs> <single|dual> <tag>
# env: RWM_GEN, RWM_GEN_R, RWM_EXTRA, RWM_STORE
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
A="${1:?}"; B="${2:?}"; BYTES="${3:-50000000}"; RUNS="${4:-6}"; MODE="${5:-single}"; TAG="${6:-x}"
GEN="${RWM_GEN:-384}"; GR="${RWM_GEN_R:-0.15}"; EXTRA="${RWM_EXTRA:-}"
if [[ "$GEN" == "none" ]]; then TENV=""; else TENV="RWM_GEN=$GEN RWM_GEN_R=$GR"; fi
[[ -n "${RWM_STORE:-}" ]] && TENV="$TENV RWM_STORE=$RWM_STORE"
OUT="/tmp/meas_${TAG}.log"

cleanup() { pkill -x raptorpath 2>/dev/null || true; bash ./topo_dual.sh down >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup
pgrep -x raptorpath >/dev/null 2>&1 && { echo "BUSY"; exit 3; }

bash ./topo_dual.sh up "$A" "$B" --seed 42 >/dev/null 2>&1
if [[ "$MODE" == "dual" ]]; then
    SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"; PEERS="$SRV_BIND"; CLI_BIND="10.77.0.1:0,10.78.0.1:0"
else
    SRV_BIND="10.77.0.2:7000"; PEERS="10.77.0.2:7000"; CLI_BIND="10.77.0.1:0"
fi

ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable $EXTRA --protocol-hint bulk >/tmp/meas-s-${TAG}.log 2>&1 &
for _ in $(seq 1 20); do ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break; sleep 0.3; done
sleep 1

echo "--- meas tag=$TAG mode=$MODE A=$A B=$B GEN=$GEN r=$GR extra='$EXTRA' store=${RWM_STORE:-def} ($BYTES x $RUNS) start=$(date +%T)"
timeout 700 ip netns exec "$NS_CLI" env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" --window-reliable $EXTRA \
    --protocol-hint bulk --bytes "$BYTES" --runs "$RUNS" > "$OUT" 2>&1
echo "client_exit=$? done=$(date +%T)"
echo "=== per-run + summary ==="
grep -E '"summary"|"seconds".*warmup|"dnf"' "$OUT" || echo "NO SUMMARY - tail:"
grep -vE 'INFO|DEBUG|WARN' "$OUT" | tail -8
