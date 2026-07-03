#!/bin/bash
# Phase-3 TCP diagnosis: capture on BOTH TUN interfaces during a tb
# connect attempt to see where the SYN dies (checksum? routing? never
# read from the TUN?). Also capture the client's TUN with -v for
# checksum validation.
set -uo pipefail
cd "$(dirname "$0")"

NS_C="rp-s3cli"; NS_S="rp-s3srv"
BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -f "raptorpath run" 2>/dev/null || true
    pkill -f tcpdump 2>/dev/null || true
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

# Captures: client TUN (SYN leaving the client stack), server TUN (SYN
# arriving after decode+inject), with checksum verification (-vv).
ip netns exec "$NS_C" tcpdump -i rps3cli -vv -n -c 6 tcp > /tmp/dump-cli.txt 2>&1 &
ip netns exec "$NS_S" tcpdump -i rps3srv -vv -n -c 6 tcp > /tmp/dump-srv.txt 2>&1 &
sleep 1

ip netns exec "$NS_S" nohup python3 /home/vibe/l1/transfer_bench.py server \
    --bind 10.99.3.2 --port 9902 >/tmp/rp3-tb.log 2>&1 &
sleep 0.5
timeout 12 ip netns exec "$NS_C" python3 /home/vibe/l1/transfer_bench.py client \
    --host 10.99.3.2 --port 9902 --bytes 100000 --runs 1 2>&1 | tail -1

sleep 2
pkill -f tcpdump 2>/dev/null; sleep 0.5
echo "=== CLIENT TUN (rps3cli):"
head -14 /tmp/dump-cli.txt
echo "=== SERVER TUN (rps3srv):"
head -14 /tmp/dump-srv.txt
