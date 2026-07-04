#!/bin/bash
# QUIC message-latency benchmark over the L1 harness.
#   sudo bash quic_stream_bench.sh [rate] [size] [duration] [cells...]
# msg_lat (quinn example) server in rp-srv bound to 10.77.0.2, client in
# rp-cli connecting to 10.77.0.2 — direct QUIC over the netem veth, the same
# geometry as the kernel-TCP (cubic/bbr) stream runs. One QUIC ordered stream.
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh

RATE="${1:-50}"; SIZE="${2:-1200}"; DUR="${3:-30}"
shift 3 2>/dev/null || true
CELLS=("$@")
[[ ${#CELLS[@]} -eq 0 ]] && CELLS=(c2 c3 c5)

BIN="/home/vibe/quinn/target/release/examples/msg_lat"
PORT=9920

cleanup() {
    pkill -x msg_lat 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT

# --- Guard: another agent may be measuring raptorpath on these namespaces. ---
# Wait until raptorpath is not running AND no rp-* namespaces exist.
while true; do
    if pgrep -x raptorpath >/dev/null 2>&1; then
        echo "WAIT: raptorpath running; polling in 30s ($(date +%T))"; sleep 30; continue
    fi
    if ip netns list | grep -q '^rp-'; then
        echo "WAIT: rp-* namespaces present; polling in 30s ($(date +%T))"; sleep 30; continue
    fi
    break
done
echo "namespaces free; starting QUIC measurement ($(date +%T))"

for CELL in "${CELLS[@]}"; do
    echo "======== CELL $CELL ========"
    pkill -x msg_lat 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
    bash ./topo.sh up "$CELL" --seed 42 >/dev/null 2>&1

    ip netns exec "$NS_SRV" "$BIN" server --listen 10.77.0.2:$PORT \
        >/tmp/quic-srv.log 2>/tmp/quic-srv.err &
    sleep 1
    timeout $((DUR + 90)) ip netns exec "$NS_CLI" "$BIN" client \
        --server-name raptorpath --ip 10.77.0.2:$PORT --rate "$RATE" \
        --size "$SIZE" --duration "$DUR" >/tmp/quic-cli.log 2>/tmp/quic-cli.err
    sleep 2

    echo "CELL=$CELL RATE=$RATE SIZE=$SIZE DUR=$DUR"
    echo -n "client: "; cat /tmp/quic-cli.log
    echo -n "server: "; grep '"summary"' /tmp/quic-srv.log | tail -1 \
        || { echo "NO SUMMARY"; echo "--- srv.err ---"; tail -5 /tmp/quic-srv.err; \
             echo "--- cli.err ---"; tail -5 /tmp/quic-cli.err; }
    pkill -x msg_lat 2>/dev/null || true
    bash ./topo.sh down >/dev/null 2>&1 || true
    sleep 1
done
echo "ALL DONE ($(date +%T))"
