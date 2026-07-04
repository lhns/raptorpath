#!/bin/bash
# L2 batch 2 (detached): warm-flow geometry + realtime-stream diagnostics.
# Expected ~20-30 min. Stamps per step; death traps.
set -uo pipefail
trap 'echo "BATCH EXIT rc=$? ($(date +%T))"' EXIT
trap 'echo "BATCH SIGTERM ($(date +%T))"' TERM
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f "python3 ./transfer_bench.py" 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}

step() { echo "--- $1 start=$(date +%T)"; }

# 1. Warm objects THROUGH the rp tunnel (bulk) at C2 — the fair-geometry rp number
step "rp-bulk-warm c2"
cleanup
bash ./topo.sh up c2 --seed 42 >/dev/null 2>&1
ip netns exec "$NS_SRV" "$BIN" run --server --bind 10.77.0.2:7000 \
    --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint bulk >/tmp/b2-s.log 2>&1 &
sleep 2
ip netns exec "$NS_CLI" "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
    --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint bulk >/tmp/b2-c.log 2>&1 &
for i in $(seq 1 25); do
    ip netns exec "$NS_CLI" ping -c 1 -W 1 10.99.0.2 >/dev/null 2>&1 && break; sleep 1
done
ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --bind 10.99.0.2 --port 9902 >/tmp/b2-tb.log 2>&1 &
sleep 1
timeout 600 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
    --host 10.99.0.2 --port 9902 --bytes 1800000 --runs 10 --warm 2>&1 | grep summary || echo DNF
echo "    done $(date +%T)"

# 2. Warm kernel baselines at C3/C5 (for the fair table)
for cell in c3 c5; do
    step "bbr-warm $cell"
    cleanup
    bash ./topo.sh up "$cell" --seed 42 >/dev/null 2>&1
    ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --port 9900 >/tmp/b2-tb.log 2>&1 &
    sleep 1
    timeout 600 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.77.0.2 --port 9900 --bytes 1800000 --runs 10 --cc bbr --warm 2>&1 | grep summary || echo DNF
    echo "    done $(date +%T)"
done

# 3. Diagnose the silent rp-realtime stream failures at c3/c5
for cell in c3 c5; do
    step "rp-realtime-stream-diag $cell"
    cleanup
    timeout 300 bash stream_bench.sh "$cell" rp-realtime 50 30 2>&1 | tail -3
    echo "  client log tail:"; tail -3 /tmp/stream-rp-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g'
    echo "    done $(date +%T)"
done

cleanup
echo "BATCH DONE $(date -Is)"
