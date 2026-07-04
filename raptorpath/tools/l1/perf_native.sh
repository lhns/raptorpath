#!/bin/bash
# rp-native object benchmark at L1: `raptorpath perf` (no inner TCP, no
# kernel TUN) — objects straight over the transport, apples-to-apples vs
# quinn-perf. Server in rp-srv ns, client in rp-cli ns.
#   sudo bash perf_native.sh <cell> <hint> [bytes] [runs]
set -uo pipefail
trap 'echo "EXIT rc=$? ($(date +%T))"' EXIT
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
CELL="${1:?cell}"; HINT="${2:?hint}"; BYTES="${3:-1800000}"; RUNS="${4:-10}"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup
bash ./topo.sh up "$CELL" --seed 42 >/dev/null 2>&1

ip netns exec "$NS_SRV" "$BIN" perf --server --bind 10.77.0.2:7000 \
    --protocol-hint "$HINT" >/tmp/perf-s.log 2>&1 &
sleep 2
echo "--- perf native $HINT @ $CELL ($BYTES x $RUNS) start=$(date +%T)"
timeout 600 ip netns exec "$NS_CLI" "$BIN" perf --client \
    --peer 10.77.0.2:7000 --bytes "$BYTES" --runs "$RUNS" \
    --protocol-hint "$HINT" 2>&1 | grep -E "summary|seconds" | tail -3 \
    || echo "{\"dnf\":true,\"cell\":\"$CELL\",\"hint\":\"$HINT\"}"
echo "    done $(date +%T)"
echo "--- server log tail:"; sed 's/\x1b\[[0-9;]*m//g' /tmp/perf-s.log | tail -2
