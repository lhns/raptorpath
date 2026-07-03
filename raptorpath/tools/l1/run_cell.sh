#!/bin/bash
# Robust single-cell runner: owns the full lifecycle (topology, server,
# client, cleanup) in ONE synchronous invocation — no fragile remote
# backgrounding. Prints client JSON lines; always cleans up via trap.
#
# Usage: sudo bash run_cell.sh <scenario> tcp <cc> <bytes> <runs> [timeout_s]
#        sudo bash run_cell.sh <scenario> quinn <bytes-si> <duration_s>

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

SCEN="${1:?scenario}"
PROTO="${2:?tcp|quinn}"

cleanup() {
    ip netns exec "$NS_SRV" pkill -f transfer_bench 2>/dev/null || true
    ip netns exec "$NS_SRV" pkill -f quinn-perf 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT

bash ./topo.sh up "$SCEN" --seed 42 >/dev/null

case "$PROTO" in
    tcp)
        CC="${3:?cc}"; BYTES="${4:?bytes}"; RUNS="${5:?runs}"; TMO="${6:-1740}"
        ip netns exec "$NS_SRV" nohup python3 ./transfer_bench.py server \
            --port 9900 >/tmp/rp-tb-server.log 2>&1 &
        sleep 1
        timeout "$TMO" ip netns exec "$NS_CLI" python3 ./transfer_bench.py client \
            --host 10.77.0.2 --port 9900 --bytes "$BYTES" --runs "$RUNS" --cc "$CC" \
            || echo "{\"summary\":true,\"scenario\":\"$SCEN\",\"cc\":\"$CC\",\"timeout_s\":$TMO,\"dnf\":true}"
        ;;
    quinn)
        BYTES="${3:-1800K}"; DURATION="${4:-30}"
        PERF="/home/vibe/quinn/target/release/quinn-perf"
        ip netns exec "$NS_SRV" nohup "$PERF" server --listen 10.77.0.2:4433 \
            >/tmp/rp-quinn-server.log 2>&1 &
        sleep 1
        timeout $((DURATION + 90)) ip netns exec "$NS_CLI" "$PERF" client \
            raptorpath:4433 --ip 10.77.0.2 \
            --download-size "$BYTES" --upload-size 0 \
            --duration "$DURATION" --interval "$DURATION" --json - \
            2>/tmp/rp-quinn-client.log || {
                echo "QUINN CLIENT FAILED:"; tail -5 /tmp/rp-quinn-client.log
            }
        ;;
    *) echo "unknown proto $PROTO" >&2; exit 1 ;;
esac
