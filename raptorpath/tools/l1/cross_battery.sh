#!/bin/bash
# Cross-traffic battery driver (roadmap item 6, feat/copa-compete): the four
# arms of cross_traffic.sh interleaved round-robin per rep (MEASUREMENT
# DISCIPLINE #3), per-arm result-count asserted (#7 — an arm with zero
# XTRESULT lines fails the battery loudly).
#
#   sudo bash cross_battery.sh <scenario> <reps> <seed> [bytes] [outdir]
#
# Logs: $OUT/<scen>-<arm>-s<seed>.log (battery record incl. XTRESULT lines)
#       $OUT/diag-<scen>-<arm>-s<seed>-r<rep>.log (full per-run client log)
set -uo pipefail
cd "$(dirname "$0")"

SCEN="${1:?scenario}"; REPS="${2:-8}"; SEED="${3:-42}"
BYTES="${4:-25000000}"; OUT="${5:-/home/vibe/copacompete}"
ARMS=(solo copa compete bbr)

mkdir -p "$OUT"
echo "=== cross battery scen=$SCEN reps=$REPS seed=$SEED bytes=$BYTES $(date -u +%FT%TZ)"
lscpu | grep -E 'Model name|Flags' | head -2 || true

for rep in $(seq 1 "$REPS"); do
    for arm in "${ARMS[@]}"; do
        log="$OUT/$SCEN-$arm-s$SEED.log"
        echo "=== rep $rep arm $arm $(date +%T)" | tee -a "$log"
        SEED="$SEED" bash ./cross_traffic.sh "$SCEN" "$arm" "$BYTES" \
            >>"$log" 2>&1 || echo "WARN: rep $rep arm $arm rc=$? (recorded)" | tee -a "$log"
        cp /tmp/xt-c.log "$OUT/diag-$SCEN-$arm-s$SEED-r$rep.log" 2>/dev/null || true
        sleep 2
    done
done

# Per-arm liveness: every arm must have produced XTRESULT lines.
fail=0
for arm in "${ARMS[@]}"; do
    n=$(grep -c '^XTRESULT' "$OUT/$SCEN-$arm-s$SEED.log" 2>/dev/null || true)
    n="${n:-0}"
    echo "ARM $arm results: $n/$REPS"
    [[ "$n" -eq 0 ]] && fail=1
done
if [[ "$fail" == "1" ]]; then
    echo "BATTERY FAILED: an arm produced ZERO results (discipline #7)" >&2
    exit 7
fi
echo "=== battery done $(date -u +%FT%TZ)"
