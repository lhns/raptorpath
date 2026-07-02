#!/bin/bash
# Phase-1 baseline: real TCP through the L1 topology.
#
# Usage: sudo bash tcp_baseline.sh <scenario> <cc> [bytes] [runs]
#   cc: cubic | bbr | reno
#   bytes: transfer size per run (default 1800K = the L0 gate object)
#   runs: repetitions (default 5)
#
# Emits one JSON line per run: {scenario, cc, bytes, seconds, mbps, retrans}
# and a summary line. Topology must be up (topo.sh up <scenario>).

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

scenario="${1:?scenario}"
cc="${2:?cc (cubic|bbr|reno)}"
bytes="${3:-1800K}"
runs="${4:-5}"

# Server in rp-srv (idempotent restart)
ip netns exec "$NS_SRV" pkill -f 'iperf3 -s' 2>/dev/null || true
sleep 0.3
ip netns exec "$NS_SRV" iperf3 -s -D --pidfile /tmp/rp-iperf3.pid
sleep 0.3

total=0
for i in $(seq 1 "$runs"); do
    out=$(timeout 300 ip netns exec "$NS_CLI" \
        iperf3 -c 10.77.0.2 -n "$bytes" -C "$cc" --json)
    secs=$(echo "$out" | jq '.end.sum_sent.seconds')
    mbps=$(echo "$out" | jq '.end.sum_sent.bits_per_second / 1e6')
    retr=$(echo "$out" | jq '.end.sum_sent.retransmits')
    echo "{\"scenario\":\"$scenario\",\"cc\":\"$cc\",\"bytes\":\"$bytes\",\"run\":$i,\"seconds\":$secs,\"mbps\":$mbps,\"retransmits\":$retr}"
    total=$(echo "$total + $secs" | bc -l)
done
avg=$(echo "$total / $runs" | bc -l)
echo "SUMMARY scenario=$scenario cc=$cc bytes=$bytes runs=$runs avg_seconds=$(printf '%.3f' "$avg")"

ip netns exec "$NS_SRV" pkill -f 'iperf3 -s' 2>/dev/null || true
