#!/bin/bash
# FEC-vs-ARQ crossover RTT sweep (branch feat/fec-arq-crossover).
# For each RTT: pure-ARQ (default) vs proactive frontier-FEC
# (RWM_FRONTIER=32 RWM_FRONTIER_R=0.10), single-path bulk, native perf.
# Cells c2r10..c2r200 (lib.sh): c2 loss/bw (100mbit, GE 1.3/50 ~2.5% loss),
# jitter=0 so RTT is the ONLY swept variable, one_way = RTT/2.
#
#   sudo bash rtt_sweep.sh [reps] [bytes]
cd "$(dirname "$0")"
REPS="${1:-5}"
BYTES="${2:-1800000}"
echo "=== RTT SWEEP  reps=$REPS bytes=$BYTES  $(date +%T) ==="
for scen in c2r10 c2r30 c2r50 c2r100 c2r200; do
  for arm in ARQ FEC; do
    if pgrep -x raptorpath >/dev/null; then sleep 3; fi
    if [[ "$arm" == FEC ]]; then
      ENV="RWM_FRONTIER=32 RWM_FRONTIER_R=0.10"
    else
      ENV=""
    fi
    line=$(env $ENV timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
    mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
    dnf=$(echo "$line" | grep -oE '"dnf":[0-9]+' | cut -d: -f2)
    printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s\n' "$scen" "$arm" "${mbps:-NA}" "${dnf:-NA}"
  done
done
echo "=== SWEEP DONE $(date +%T) ==="
