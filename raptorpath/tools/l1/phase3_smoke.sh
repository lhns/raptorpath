#!/bin/bash
# Phase-3 functional smoke: bring up the REAL raptorpath tunnel between two
# namespaces over an UNSHAPED veth (no netem — pure bring-up validation),
# ping through the TUN, and push one small object through it.
#
# Uses rp-s3cli/rp-s3srv so it cannot collide with the measurement
# namespaces (rp-cli/rp-srv) while phase 2 runs.
#
# Usage: sudo bash phase3_smoke.sh

set -uo pipefail
cd "$(dirname "$0")"

NS_C="rp-s3cli"; NS_S="rp-s3srv"
BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -f "raptorpath run" 2>/dev/null || true
    ip netns del "$NS_C" 2>/dev/null || true
    ip netns del "$NS_S" 2>/dev/null || true
}
trap cleanup EXIT
cleanup

ip netns add "$NS_C"; ip netns add "$NS_S"
ip link add s3c0 netns "$NS_C" type veth peer name s3s0 netns "$NS_S"
ip -n "$NS_C" addr add 10.87.0.1/24 dev s3c0
ip -n "$NS_S" addr add 10.87.0.2/24 dev s3s0
for l in s3c0 lo; do ip -n "$NS_C" link set "$l" up; done
for l in s3s0 lo; do ip -n "$NS_S" link set "$l" up; done

echo "=== starting raptorpath server"
ip netns exec "$NS_S" "$BIN" run --server \
    --bind 10.87.0.2:7000 \
    --tun-name rps3srv --tun-addr 10.99.3.2/24 \
    --protocol-hint bulk > /tmp/rp3-server.log 2>&1 &
sleep 2

echo "=== starting raptorpath client"
ip netns exec "$NS_C" "$BIN" run \
    --peer 10.87.0.2:7000 --bind 10.87.0.1:0 \
    --tun-name rps3cli --tun-addr 10.99.3.1/24 \
    --protocol-hint bulk > /tmp/rp3-client.log 2>&1 &

ok=0
for i in $(seq 1 20); do
    if ip netns exec "$NS_C" ping -c 1 -W 1 10.99.3.2 >/dev/null 2>&1; then
        echo "TUNNEL_UP after ~${i}s"; ok=1; break
    fi
    sleep 1
done
if [[ $ok -eq 0 ]]; then
    echo "TUNNEL_FAILED"
    echo "--- server log:"; tail -15 /tmp/rp3-server.log
    echo "--- client log:"; tail -15 /tmp/rp3-client.log
    exit 1
fi

echo "=== object transfer through the tunnel"
ip netns exec "$NS_S" nohup python3 /home/vibe/l1/transfer_bench.py server \
    --bind 10.99.3.2 --port 9902 >/tmp/rp3-tb.log 2>&1 &
sleep 1
timeout 60 ip netns exec "$NS_C" python3 /home/vibe/l1/transfer_bench.py client \
    --host 10.99.3.2 --port 9902 --bytes 1800000 --runs 3 | tail -2 \
    || { echo "TRANSFER_FAILED"; tail -5 /tmp/rp3-client.log; exit 1; }

echo "=== raptorpath logs (tail)"
tail -3 /tmp/rp3-server.log
echo "PHASE3_SMOKE_OK"
