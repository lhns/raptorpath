#!/bin/bash
# Stall probe: C2 topology, debug logs, start a transfer, sample the
# pipeline mid-stall, dump stage counters from both sides.
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh

BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -f "raptorpath run" 2>/dev/null || true
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null || true
}
trap cleanup EXIT
cleanup

bash ./topo.sh up c2 --seed 42 >/dev/null 2>&1

ip netns exec "$NS_SRV" env RUST_LOG=raptorpath=debug "$BIN" run --server \
    --bind 10.77.0.2:7000 --tun-name rpsrv0 --tun-addr 10.99.0.2/24 \
    --protocol-hint bulk > /tmp/rpS.log 2>&1 &
sleep 2
ip netns exec "$NS_CLI" env RUST_LOG=raptorpath=debug "$BIN" run \
    --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
    --tun-name rpcli0 --tun-addr 10.99.0.1/24 \
    --protocol-hint bulk > /tmp/rpC.log 2>&1 &

for i in $(seq 1 20); do
    ip netns exec "$NS_CLI" ping -c 1 -W 1 10.99.0.2 >/dev/null 2>&1 && break
    sleep 1
done
echo "tunnel up"

ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server \
    --bind 10.99.0.2 --port 9902 >/tmp/tb.log 2>&1 &
sleep 0.5
timeout 20 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
    --host 10.99.0.2 --port 9902 --bytes 200000 --runs 1 2>&1 | tail -1 &
TB_PID=$!
sleep 15

strip() { sed 's/\x1b\[[0-9;]*m//g'; }
echo "=== CLIENT counters:"
echo "encoded: $(grep -c 'encoded block' /tmp/rpC.log)  send_fail: $(grep -c 'failed to send' /tmp/rpC.log)  bp_changes: $(grep -c 'backpressure state change' /tmp/rpC.log)"
echo "last backpressure: $(grep 'backpressure state change' /tmp/rpC.log | tail -1 | strip)"
echo "last 2 encoded: "; grep 'encoded block' /tmp/rpC.log | tail -2 | strip
echo "=== SERVER counters:"
echo "blockstart: $(grep -c 'received BlockStart' /tmp/rpS.log)  decoded: $(grep -c 'block decoded' /tmp/rpS.log)  replayed: $(grep -c 'replaying' /tmp/rpS.log)  evicted: $(grep -c 'evicted' /tmp/rpS.log)"
echo "last 3 srv lines:"; tail -3 /tmp/rpS.log | strip
wait $TB_PID 2>/dev/null || true
echo "=== transfer result above (if any)"
