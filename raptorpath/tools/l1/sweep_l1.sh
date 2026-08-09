#!/bin/bash
# L1 phase-4 driver: one command reproduces the full evaluation.
#
#   sudo bash sweep_l1.sh [stacks] [cells] [runs]
#     stacks: comma list of cubic,bbr,quinn,mptcp,rp-bulk,rp-realtime
#             (default: cubic,bbr,quinn)
#     cells:  comma list (default: c1,c2,c3,c4,c5)
#     runs:   object repetitions (default: 10)
#
# Per-cell runtime stamps, death traps, DNF-as-result. netem seed pinned
# (42). Results: ~/l1/results/l1_<stack>_<cell>.jsonl + summary lines.
# Expected runtime: ~1-2 min per (stack,cell) except collapsed CCs on
# lossy cells (bounded by CELL_TIMEOUT).

set -uo pipefail
trap 'echo "SWEEP EXIT rc=$? ($(date +%T))"' EXIT
trap 'echo "SWEEP SIGTERM ($(date +%T))"' TERM
cd "$(dirname "$0")"
source ./lib.sh

STACKS="${1:-cubic,bbr,quinn}"
CELLS="${2:-c1,c2,c3,c4,c5}"
RUNS="${3:-10}"
BYTES=1800000
CELL_TIMEOUT=900
RES="/home/vibe/l1/results"; mkdir -p "$RES"
BIN="/home/vibe/raptorpath/target/release/raptorpath"
PERF="/home/vibe/quinn/target/release/quinn-perf"

cleanup_procs() {
    pkill -x quinn-perf 2>/dev/null || true
    pkill -f "python3 ./transfer_bench.py" 2>/dev/null || true
    pkill -f "raptorpath run" 2>/dev/null || true
}

run_tcp() { # cell cc
    ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --port 9900 \
        >/tmp/l1-tb.log 2>&1 &
    sleep 1
    timeout "$CELL_TIMEOUT" ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.77.0.2 --port 9900 --bytes $BYTES --runs "$RUNS" --cc "$2" \
        > "$RES/l1_$2_$1.jsonl" 2>&1 \
        || echo "{\"summary\":true,\"stack\":\"$2\",\"cell\":\"$1\",\"dnf\":true}" >> "$RES/l1_$2_$1.jsonl"
    grep '"summary"' "$RES/l1_$2_$1.jsonl" | tail -1
}

run_quinn() { # cell
    ip netns exec "$NS_SRV" nohup "$PERF" server --listen 10.77.0.2:4433 \
        >/tmp/l1-quinn-s.log 2>&1 &
    sleep 1
    timeout 150 ip netns exec "$NS_CLI" "$PERF" client raptorpath:4433 \
        --ip 10.77.0.2 --download-size 1800k --upload-size 0 \
        --duration 60 --interval 60 --json "$RES/l1_quinn_$1.json" \
        >/dev/null 2>/tmp/l1-quinn-c.log \
        || echo "quinn $1 FAILED: $(tail -1 /tmp/l1-quinn-c.log)"
    python3 - "$RES/l1_quinn_$1.json" "$1" <<'PYEOF' || true
import json, sys
d = json.load(open(sys.argv[1]))
n = sum(len(iv["streams"]) for iv in d["intervals"])
dur = max((st["end"] for iv in d["intervals"] for st in iv["streams"]), default=60)
tot = sum(st["bytes"] for iv in d["intervals"] for st in iv["streams"])
print(json.dumps({"summary": True, "stack": "quinn", "cell": sys.argv[2],
    "requests": n, "mean_s": round(dur/n, 4) if n else None,
    "goodput_mbps": round(tot*8/dur/1e6, 2)}))
PYEOF
}

run_rp() { # cell hint
    local hint="$2"
    ip netns exec "$NS_SRV" env $(rwm_forward_env) "$BIN" run --server --bind 10.77.0.2:7000 \
        --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint" \
        >/tmp/l1-rp-s.log 2>&1 &
    sleep 2
    ip netns exec "$NS_CLI" env $(rwm_forward_env) "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
        --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint" \
        >/tmp/l1-rp-c.log 2>&1 &
    local up=0
    for i in $(seq 1 25); do
        ip netns exec "$NS_CLI" ping -c 1 -W 1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
        sleep 1
    done
    if [[ $up -eq 0 ]]; then
        echo "{\"summary\":true,\"stack\":\"rp-$hint\",\"cell\":\"$1\",\"tunnel_failed\":true}"
        return
    fi
    ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server --port 9902 \
        --bind 10.99.0.2 >/tmp/l1-tb.log 2>&1 &
    sleep 1
    timeout "$CELL_TIMEOUT" ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
        --host 10.99.0.2 --port 9902 --bytes $BYTES --runs "$RUNS" \
        > "$RES/l1_rp-${hint}_$1.jsonl" 2>&1 \
        || echo "{\"summary\":true,\"stack\":\"rp-$hint\",\"cell\":\"$1\",\"dnf\":true}" >> "$RES/l1_rp-${hint}_$1.jsonl"
    grep '"summary"' "$RES/l1_rp-${hint}_$1.jsonl" | tail -1
    pkill -f "raptorpath run" 2>/dev/null || true
}

echo "=== L1 sweep start $(date -Is) stacks=$STACKS cells=$CELLS runs=$RUNS"
for cell in ${CELLS//,/ }; do
    for stack in ${STACKS//,/ }; do
        echo "--- $stack @ $cell start=$(date +%T)"
        cleanup_procs
        bash ./topo.sh down >/dev/null 2>&1
        bash ./topo.sh up "$cell" --seed 42 >/dev/null 2>&1
        case "$stack" in
            cubic|bbr|reno) run_tcp "$cell" "$stack" ;;
            quinn)          run_quinn "$cell" ;;
            rp-bulk)        run_rp "$cell" bulk ;;
            rp-realtime)    run_rp "$cell" realtime ;;
            *) echo "unknown stack $stack" ;;
        esac
        cleanup_procs
        echo "    done $(date +%T)"
    done
done
bash ./topo.sh down >/dev/null 2>&1
echo "=== L1 sweep done $(date -Is)"
