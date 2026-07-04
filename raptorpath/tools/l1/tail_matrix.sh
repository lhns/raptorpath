#!/bin/bash
# Authoritative realtime-vs-bulk tail comparison: N reps per arm, report
# the p99 DISTRIBUTION (single-run p99 is variance-dominated, so min/med/max
# of per-run p99s is the honest comparison). Matrix: {realtime,bulk} x
# {400,1200} B messages at a given cell.
#   sudo bash tail_matrix.sh <cell> <reps>
set -uo pipefail
trap 'echo "EXIT rc=$? ($(date +%T))"' EXIT
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
CELL="${1:-c2}"; REPS="${2:-5}"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f 'python3 ./transfer_bench.py' 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT

run_arm() { # hint size
    local hint="$1" size="$2"
    local p99s=()
    for r in $(seq 1 "$REPS"); do
        cleanup; sleep 0.5
        bash ./topo.sh up "$CELL" --seed $((42 + r)) >/dev/null 2>&1
        ip netns exec "$NS_SRV" "$BIN" run --server --bind 10.77.0.2:7000 \
            --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint" \
            >/tmp/tm-s.log 2>&1 &
        sleep 2
        ip netns exec "$NS_CLI" "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
            --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint" \
            >/tmp/tm-c.log 2>&1 &
        local up=0
        for i in $(seq 1 25); do
            ip netns exec "$NS_CLI" ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
            sleep 1
        done
        [[ $up -eq 0 ]] && { echo "  rep$r bringup-fail"; continue; }
        ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py stream-server \
            --bind 10.99.0.2 --port 9910 >/tmp/tm-srv.log 2>&1 &
        sleep 1
        timeout 60 ip netns exec "$NS_CLI" python3 ./transfer_bench.py stream-client \
            --host 10.99.0.2 --port 9910 --rate 50 --duration 25 --size "$size" \
            >/dev/null 2>&1
        sleep 1
        local p99
        p99=$(grep '"summary"' /tmp/tm-srv.log | tail -1 | grep -oE '"p99_ms": [0-9.]+' | grep -oE '[0-9.]+')
        [[ -n "$p99" ]] && p99s+=("$p99") && echo "  $hint ${size}B rep$r: p99=${p99}ms"
    done
    # distribution
    if [[ ${#p99s[@]} -gt 0 ]]; then
        printf '%s\n' "${p99s[@]}" | sort -n | awk -v h="$hint" -v s="$size" '
            {a[NR]=$1} END{
                printf "ARM %s %dB: n=%d  min=%.0f  median=%.0f  max=%.0f\n",
                    h, s, NR, a[1], a[int((NR+1)/2)], a[NR]}'
    fi
}

echo "=== tail matrix @ $CELL, $REPS reps/arm, 50msg/s x25s, seeds 43+ $(date +%T)"
for hint in realtime bulk; do
    for size in 400 1200; do
        echo "--- $hint ${size}B start=$(date +%T)"
        run_arm "$hint" "$size"
    done
done
echo "=== done $(date +%T)"
