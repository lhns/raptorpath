#!/bin/bash
# L2 ws2: small-message latency percentiles through a cell.
#   sudo bash stream_bench.sh <cell> <stack> [rate] [duration]
#     stack: cubic|bbr|rp-bulk|rp-realtime
# One-way per-message latency (shared kernel clock), p50/p99/p999.
set -uo pipefail
trap 'echo "EXIT rc=$? ($(date +%T))"' EXIT
cd "$(dirname "$0")"
source ./lib.sh

CELL="${1:?cell}"; STACK="${2:?stack}"
RATE="${3:-50}"; DUR="${4:-30}"
BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f "python3 ./transfer_bench.py stream" 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT

bash ./topo.sh down >/dev/null 2>&1
bash ./topo.sh up "$CELL" --seed 42 >/dev/null 2>&1

case "$STACK" in
    cubic|bbr)
        ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py stream-server \
            --port 9910 >/tmp/stream-srv.log 2>&1 &
        sleep 1
        timeout $((DUR + 30)) ip netns exec "$NS_CLI" python3 ./transfer_bench.py \
            stream-client --host 10.77.0.2 --port 9910 --rate "$RATE" \
            --duration "$DUR" --cc "$STACK" >/dev/null 2>&1
        sleep 2
        ;;
    rp-bulk|rp-realtime)
        HINT="${STACK#rp-}"
        ip netns exec "$NS_SRV" env $(rwm_forward_env) "$BIN" run --server --bind 10.77.0.2:7000 \
            --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$HINT" \
            >/tmp/stream-rp-s.log 2>&1 &
        sleep 2
        ip netns exec "$NS_CLI" env $(rwm_forward_env) "$BIN" run --peer 10.77.0.2:7000 \
            --bind 10.77.0.1:0 --tun-name rpcli0 --tun-addr 10.99.0.1/24 \
            --protocol-hint "$HINT" >/tmp/stream-rp-c.log 2>&1 &
        for i in $(seq 1 25); do
            ip netns exec "$NS_CLI" ping -c 1 -W 1 10.99.0.2 >/dev/null 2>&1 && break
            sleep 1
        done
        ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py stream-server \
            --bind 10.99.0.2 --port 9910 >/tmp/stream-srv.log 2>&1 &
        sleep 1
        timeout $((DUR + 30)) ip netns exec "$NS_CLI" python3 ./transfer_bench.py \
            stream-client --host 10.99.0.2 --port 9910 --rate "$RATE" \
            --duration "$DUR" >/dev/null 2>&1
        sleep 2
        ;;
    *) echo "unknown stack $STACK"; exit 1 ;;
esac

echo "STACK=$STACK CELL=$CELL RATE=$RATE DUR=$DUR"
grep '"summary"' /tmp/stream-srv.log | tail -1
