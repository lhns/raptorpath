#!/bin/bash
# Debug-level phase-3 smoke: like phase3_smoke.sh but RUST_LOG=debug and
# prints the interesting decode-path lines at the end.
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

ip netns exec "$NS_S" env RUST_LOG=raptorpath=debug "$BIN" run --server \
    --bind 10.87.0.2:7000 --tun-name rps3srv --tun-addr 10.99.3.2/24 \
    --protocol-hint bulk > /tmp/rp3-server-dbg.log 2>&1 &
sleep 2
ip netns exec "$NS_C" env RUST_LOG=raptorpath=debug "$BIN" run \
    --peer 10.87.0.2:7000 --bind 10.87.0.1:0 \
    --tun-name rps3cli --tun-addr 10.99.3.1/24 \
    --protocol-hint bulk > /tmp/rp3-client-dbg.log 2>&1 &
sleep 3

ip netns exec "$NS_C" ping -c 3 -i 0.5 -W 1 10.99.3.2 >/dev/null 2>&1 \
    && echo PING_OK || echo PING_FAIL
sleep 5
cleanup

strip() { sed 's/\x1b\[[0-9;]*m//g'; }
echo "=== SERVER decode-path lines:"
grep -aE "BlockStart|block decoded|decode|symbol_size|batch" /tmp/rp3-server-dbg.log | strip | tail -15
echo "=== CLIENT send-path lines:"
grep -aE "BlockStart|block|symbol_size|batch|send" /tmp/rp3-client-dbg.log | strip | tail -15
