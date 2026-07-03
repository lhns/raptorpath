#!/bin/bash
# Phase-2a: real QUIC (quinn-perf) through the L1 topology.
#
# quinn-perf repeats <download-size> requests for <duration> seconds on one
# connection; per-request completion is exactly our object metric. CC is
# quinn's default (Cubic-family) — "QUIC as commonly deployed"; kernel BBR
# (phase 1) bounds the loss-blind end.
#
# Topology must be up. Usage:
#   sudo bash quinn_bench.sh <scenario-label> [bytes] [duration_s]

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

LABEL="${1:?scenario label}"
BYTES="${2:-1800K}"
DURATION="${3:-30}"
PERF="$HOME/quinn/target/release/quinn-perf"
[[ -x "$PERF" ]] || PERF="/home/vibe/quinn/target/release/quinn-perf"

ip netns exec "$NS_SRV" pkill -f quinn-perf 2>/dev/null || true
sleep 0.3
ip netns exec "$NS_SRV" "$PERF" server --listen 10.77.0.2:4433 \
    > /tmp/rp-quinn-server.log 2>&1 &
sleep 0.7

out=$(timeout $((DURATION + 60)) ip netns exec "$NS_CLI" "$PERF" client \
    raptorpath:4433 --ip 10.77.0.2 \
    --download-size "$BYTES" --upload-size 0 \
    --duration "$DURATION" --interval "$DURATION" --json - 2>/dev/null | tail -1)

ip netns exec "$NS_SRV" pkill -f quinn-perf 2>/dev/null || true

echo "$out" | jq -c --arg label "$LABEL" --arg bytes "$BYTES" '{
    scenario: $label, proto: "quic-quinn", bytes: $bytes,
    stats: .
}' 2>/dev/null || { echo "RAW: $out"; }
