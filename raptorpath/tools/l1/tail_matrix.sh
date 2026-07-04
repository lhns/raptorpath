#!/bin/bash
# Realtime-vs-bulk tail comparison, robust + fast: bring the tunnel up ONCE
# per arm, run N stream measurements through the SAME warm tunnel (no flaky
# per-rep bringup), report the p99 DISTRIBUTION (single-run p99 is
# variance-dominated). Matrix: {realtime,bulk} x {400,1200}B at <cell>.
# Every arm is hard-timeout-bounded so nothing can wedge the matrix.
#   sudo bash tail_matrix.sh <cell> <reps>
set -uo pipefail
trap 'echo "EXIT rc=$? ($(date +%T))"' EXIT
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
CELL="${1:-c2}"; REPS="${2:-5}"

hard_cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f 'python3 ./transfer_bench.py' 2>/dev/null || true
    ip netns del "$NS_CLI" 2>/dev/null || true
    ip netns del "$NS_SRV" 2>/dev/null || true
}
trap hard_cleanup EXIT

run_arm() { # hint size  -> one warm tunnel, REPS stream measurements
    local hint="$1" size="$2"
    hard_cleanup; sleep 0.5
    bash ./topo.sh up "$CELL" --seed 42 >/dev/null 2>&1
    ip netns exec "$NS_SRV" "$BIN" run --server --bind 10.77.0.2:7000 \
        --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint" \
        >/tmp/tm-s.log 2>&1 &
    sleep 2
    ip netns exec "$NS_CLI" "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
        --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint" \
        >/tmp/tm-c.log 2>&1 &
    local up=0
    for i in $(seq 1 20); do
        ip netns exec "$NS_CLI" ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
        sleep 1
    done
    [[ $up -eq 0 ]] && { echo "ARM $hint ${size}B: BRINGUP_FAIL"; hard_cleanup; return; }

    local p99s=()
    for r in $(seq 1 "$REPS"); do
        : > /tmp/tm-srv.log
        ip netns exec "$NS_SRV" timeout 30 python3 ./transfer_bench.py stream-server \
            --bind 10.99.0.2 --port 9910 >/tmp/tm-srv.log 2>&1 &
        local spid=$!
        sleep 0.5
        timeout 30 ip netns exec "$NS_CLI" python3 ./transfer_bench.py stream-client \
            --host 10.99.0.2 --port 9910 --rate 50 --duration 20 --size "$size" \
            >/dev/null 2>&1 || true
        wait $spid 2>/dev/null || true
        local p99
        p99=$(grep '"summary"' /tmp/tm-srv.log | tail -1 \
              | sed -n 's/.*"p99_ms": \([0-9.]*\).*/\1/p')
        if [[ -n "$p99" ]]; then p99s+=("$p99"); echo "  $hint ${size}B rep$r: p99=${p99}ms"; fi
    done
    hard_cleanup
    if [[ ${#p99s[@]} -gt 0 ]]; then
        printf '%s\n' "${p99s[@]}" | sort -n | awk -v h="$hint" -v s="$size" '
            {a[NR]=$1} END{ printf "ARM %s %dB: n=%d min=%.0f median=%.0f max=%.0f\n",
                h,s,NR,a[1],a[int((NR+1)/2)],a[NR] }'
    else
        echo "ARM $hint ${size}B: NO_DATA"
    fi
}

echo "=== tail matrix @ $CELL, $REPS reps/arm (warm tunnel), 50msg/s x20s $(date +%T)"
for hint in realtime bulk; do
    for size in 400 1200; do
        echo "--- $hint ${size}B start=$(date +%T)"
        run_arm "$hint" "$size"
    done
done
echo "=== done $(date +%T)"
