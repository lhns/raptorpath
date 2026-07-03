#!/bin/bash
# Phase-1 sweep: real TCP (cubic, bbr) across all single-path cells with
# precise object-completion timing (transfer_bench.py).
#
# Usage: sudo bash sweep_tcp.sh [outdir]
# Output: one JSONL file per (scenario, cc) + a summary table on stdout.

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

OUT="${1:-$HOME/l1/results}"
mkdir -p "$OUT"

SCENARIOS=(${SCENARIOS:-c1 c2 c3 c4 c5})
CCS=(cubic bbr)
SMALL_BYTES=1800000
SMALL_RUNS=10
BIG_RUNS=2

# Big-object size fitted to the scenario: collapsed CUBIC on high-loss/
# high-RTT cells runs at ~1 Mbit/s — 50 MB would blow any sane timeout.
big_bytes_for() {
    case "$1" in
        c1) echo 200000000 ;;
        c2) echo 50000000 ;;
        c3) echo 10000000 ;;
        c4|c5) echo 10000000 ;;
        *) echo 10000000 ;;
    esac
}

start_server() {
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null || true
    sleep 0.2
    ip netns exec "$NS_SRV" python3 ./transfer_bench.py server --port 9900 \
        >/tmp/rp-tb-server.log 2>&1 &
    sleep 0.4
}

echo "scenario,cc,small_mean_s,small_median_s,small_max_s,big_mean_mbps"
for scen in "${SCENARIOS[@]}"; do
    bash ./topo.sh up "$scen" --seed 42 >/dev/null
    start_server
    for cc in "${CCS[@]}"; do
        f="$OUT/tcp_${scen}_${cc}.jsonl"
        : > "$f"
        BIG_BYTES=$(big_bytes_for "$scen")
        # timeouts are recorded, not fatal (a collapsed CC that cannot
        # finish IS a result)
        timeout 1200 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
            --host 10.77.0.2 --port 9900 --bytes $SMALL_BYTES --runs $SMALL_RUNS \
            --cc "$cc" >> "$f" || echo "{\"summary\":true,\"timeout\":true,\"cc\":\"$cc\"}" >> "$f"
        timeout 1200 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
            --host 10.77.0.2 --port 9900 --bytes $BIG_BYTES --runs $BIG_RUNS \
            --cc "$cc" >> "$f" || echo "{\"summary\":true,\"timeout\":true,\"cc\":\"$cc\"}" >> "$f"
        small=$(grep '"summary"' "$f" | head -1)
        big=$(grep '"summary"' "$f" | tail -1)
        echo "$scen,$cc,$(echo "$small" | jq -r '.mean_s'),$(echo "$small" | jq -r '.median_s'),$(echo "$small" | jq -r '.max_s'),$(echo "$big" | jq -r '.mean_mbps')"
    done
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null || true
    bash ./topo.sh down >/dev/null
done
echo "results in $OUT"
