#!/bin/bash
# Phase-3: raptorpath itself (real binary, real TUN) through the L1 links.
#
# Topology must be up (topo.sh up <scenario> for single path, or
# topo_dual.sh up <scenA> <scenB> for two paths). This script:
#   1. starts raptorpath --server in rp-srv (TUN 10.99.0.2/24)
#   2. starts raptorpath client in rp-cli (TUN 10.99.0.1/24), one --peer
#      per path
#   3. waits for the tunnel to carry pings
#   4. runs transfer_bench.py THROUGH the tunnel (10.99.0.2)
#   5. tears the processes down (trap'd)
#
# Usage: sudo bash raptorpath_bench.sh <hint> <bytes> <runs> [--dual]
#   hint: bulk | auto | realtime

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

HINT="${1:-bulk}"
BYTES="${2:-1800000}"
RUNS="${3:-10}"
DUAL="${4:-}"

BIN="$HOME/raptorpath/target/release/raptorpath"
[[ -x "$BIN" ]] || { echo "raptorpath binary not found at $BIN" >&2; exit 1; }

cleanup() {
    pkill -f "raptorpath run" 2>/dev/null || true
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null || true
}
trap cleanup EXIT

cleanup
sleep 0.3

# Server: listens on the veth address(es)
SRV_BIND="10.77.0.2:7000"
[[ "$DUAL" == "--dual" ]] && SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"
ip netns exec "$NS_SRV" "$BIN" run --server \
    --bind "$SRV_BIND" \
    --tun-name rpsrv0 --tun-addr 10.99.0.2/24 \
    --protocol-hint "$HINT" \
    > /tmp/rp-server.log 2>&1 &
sleep 1.5

# Client: one peer per path
PEERS="10.77.0.2:7000"
[[ "$DUAL" == "--dual" ]] && PEERS="10.77.0.2:7000,10.78.0.2:7000"
ip netns exec "$NS_CLI" "$BIN" run \
    --peer "$PEERS" \
    --tun-name rpcli0 --tun-addr 10.99.0.1/24 \
    --protocol-hint "$HINT" \
    > /tmp/rp-client.log 2>&1 &

# Wait for tunnel liveness
for i in $(seq 1 30); do
    if ip netns exec "$NS_CLI" ping -c 1 -W 1 10.99.0.2 >/dev/null 2>&1; then
        echo "tunnel up after ~${i}s"
        break
    fi
    [[ $i -eq 30 ]] && { echo "tunnel failed to come up"; tail -5 /tmp/rp-server.log /tmp/rp-client.log; exit 1; }
    sleep 1
done

# Benchmark THROUGH the tunnel
ip netns exec "$NS_SRV" python3 ./transfer_bench.py server --port 9900 \
    --bind 10.99.0.2 > /tmp/rp-tb-server.log 2>&1 &
sleep 0.5
timeout 900 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
    --host 10.99.0.2 --port 9900 --bytes "$BYTES" --runs "$RUNS"

echo "--- raptorpath server log tail:"
tail -3 /tmp/rp-server.log || true
