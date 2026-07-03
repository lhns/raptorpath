#!/bin/bash
# Phase-2 orchestrator: real QUIC (quinn) across all single-path cells +
# kernel MPTCP over the dual-path cells. Designed to run DETACHED on the
# VM (nohup); waits for any in-flight measurement to finish first.
# Progress and results -> ~/l1/results/phase2.log (JSONL-ish).
#
# Usage: nohup sudo bash sweep_phase2.sh > ~/l1/results/phase2.log 2>&1 &

set -uo pipefail   # NOT -e: a failed cell is recorded, not fatal
cd "$(dirname "$0")"
source ./lib.sh

RES="$HOME/l1/results"; mkdir -p "$RES"
PERF="/home/vibe/quinn/target/release/quinn-perf"

# Wait for any in-flight measurement using the rp-* namespaces
for i in $(seq 1 120); do
    pgrep -f "transfer_bench.py client" >/dev/null || break
    sleep 30
done

echo "=== phase2 start $(date -Is)"

# --- 2a: quinn across single-path cells ---
for scen in c1 c2 c3 c4 c5; do
    echo "--- quinn $scen"
    bash ./topo.sh down >/dev/null 2>&1
    bash ./topo.sh up "$scen" --seed 42 >/dev/null 2>&1
    pkill -f quinn-perf 2>/dev/null; sleep 0.5
    ip netns exec "$NS_SRV" nohup "$PERF" server --listen 10.77.0.2:4433 \
        >/tmp/rp-quinn-server.log 2>&1 &
    sleep 1
    timeout 150 ip netns exec "$NS_CLI" "$PERF" client \
        raptorpath:4433 --ip 10.77.0.2 \
        --download-size 1800k --upload-size 0 \
        --duration 60 --interval 60 --json "$RES/quinn_${scen}.json" \
        >/dev/null 2>/tmp/rp-quinn-client.log \
        || echo "quinn $scen FAILED: $(tail -2 /tmp/rp-quinn-client.log)"
    pkill -f quinn-perf 2>/dev/null
done
bash ./topo.sh down >/dev/null 2>&1

# --- 2b: kernel MPTCP over dual-path cells ---
run_mptcp() { # label scenA scenB
    local label="$1" a="$2" b="$3"
    echo "--- mptcp $label ($a + $b)"
    bash ./topo_dual.sh down >/dev/null 2>&1
    bash ./topo_dual.sh up "$a" "$b" --seed 42 >/dev/null 2>&1
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null; sleep 0.5
    ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --port 9901 \
        --proto mptcp >/tmp/rp-mptcp-server.log 2>&1 &
    sleep 1
    timeout 900 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.77.0.2 --port 9901 --bytes 1800000 --runs 10 --proto mptcp \
        > "$RES/mptcp_${label}_small.jsonl" 2>&1 \
        || echo "{\"dnf\":true}" >> "$RES/mptcp_${label}_small.jsonl"
    timeout 900 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.77.0.2 --port 9901 --bytes 50000000 --runs 2 --proto mptcp \
        > "$RES/mptcp_${label}_big.jsonl" 2>&1 \
        || echo "{\"dnf\":true}" >> "$RES/mptcp_${label}_big.jsonl"
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null
    grep '"summary"' "$RES/mptcp_${label}_small.jsonl" | tail -1
    grep '"summary"' "$RES/mptcp_${label}_big.jsonl" | tail -1
}
run_mptcp c7 c2 c2
run_mptcp c8 c2 c3
# Single-path MPTCP reference on c2 (aggregation delta needs this)
echo "--- mptcp single-path reference (c2)"
bash ./topo_dual.sh down >/dev/null 2>&1
bash ./topo.sh up c2 --seed 42 >/dev/null 2>&1
ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --port 9901 \
    --proto mptcp >/tmp/rp-mptcp-server.log 2>&1 &
sleep 1
timeout 900 ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
    --host 10.77.0.2 --port 9901 --bytes 50000000 --runs 2 --proto mptcp \
    > "$RES/mptcp_c2single_big.jsonl" 2>&1 || true
grep '"summary"' "$RES/mptcp_c2single_big.jsonl" | tail -1
ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null
bash ./topo.sh down >/dev/null 2>&1

echo "=== phase2 done $(date -Is)"
