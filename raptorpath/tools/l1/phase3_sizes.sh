#!/bin/bash
# Size bisection: where does the tunnel transfer break?
set -uo pipefail
cd "$(dirname "$0")"

NS_C="rp-s3cli"; NS_S="rp-s3srv"
BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -f "raptorpath run" 2>/dev/null || true
    pkill -f "transfer_bench.py server" 2>/dev/null || true
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

ip netns exec "$NS_S" "$BIN" run --server --bind 10.87.0.2:7000 \
    --tun-name rps3srv --tun-addr 10.99.3.2/24 --protocol-hint bulk \
    > /tmp/rp3-server.log 2>&1 &
sleep 2
ip netns exec "$NS_C" "$BIN" run --peer 10.87.0.2:7000 --bind 10.87.0.1:0 \
    --tun-name rps3cli --tun-addr 10.99.3.1/24 --protocol-hint bulk \
    > /tmp/rp3-client.log 2>&1 &

for i in $(seq 1 20); do
    ip netns exec "$NS_C" ping -c 1 -W 1 10.99.3.2 >/dev/null 2>&1 && break
    sleep 1
done
echo "tunnel up"

ip netns exec "$NS_S" nohup python3 /home/vibe/l1/transfer_bench.py server \
    --bind 10.99.3.2 --port 9902 >/tmp/rp3-tb.log 2>&1 &
sleep 0.5

for bytes in 100000 400000 1800000 1800000 1800000; do
    out=$(timeout 60 ip netns exec "$NS_C" python3 /home/vibe/l1/transfer_bench.py client \
        --host 10.99.3.2 --port 9902 --bytes "$bytes" --runs 1 2>&1 | tail -1)
    echo "SIZE $bytes -> $out"
    # Path still alive?
    ip netns exec "$NS_C" ping -c 1 -W 2 10.99.3.2 >/dev/null 2>&1 \
        && echo "  tunnel alive" || { echo "  TUNNEL DEAD"; break; }
done
grep -c "path timed out" /tmp/rp3-client.log /tmp/rp3-server.log 2>/dev/null || true
