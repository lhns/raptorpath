#!/bin/bash
# FEC frontier-repair config robustness (branch feat/fec-arq-crossover):
# can ANY W/offset/r combination beat pure-ARQ at high RTT (the most
# FEC-favorable regime)? Single-path bulk, native perf.
#
#   sudo bash fec_tune.sh [reps] [scenario]   # e.g. c2r200
cd "$(dirname "$0")"
REPS="${1:-5}"; SCEN="${2:-c2r200}"
echo "=== FEC TUNE  scen=$SCEN reps=$REPS  $(date +%T) ==="
run() { # label env...
  local label="$1"; shift
  if pgrep -x raptorpath >/dev/null; then sleep 3; fi
  local line
  line=$(env "$@" timeout 700 sudo -E bash perf_rwm_c.sh "$SCEN" "$SCEN" bulk 1800000 "$REPS" single 2>&1 | grep -E '"summary"')
  local mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
  local dnf=$(echo "$line" | grep -oE '"dnf":[0-9]+' | cut -d: -f2)
  printf 'TUNE %s mean_mbps=%s dnf=%s\n' "$label" "${mbps:-NA}" "${dnf:-NA}"
}
run ARQ
run FEC_r05_W16_off2  RWM_FRONTIER=16 RWM_FRONTIER_R=0.05 RWM_FRONTIER_OFFSET=2
run FEC_r10_W16_off2  RWM_FRONTIER=16 RWM_FRONTIER_R=0.10 RWM_FRONTIER_OFFSET=2
run FEC_r20_W32_off4  RWM_FRONTIER=32 RWM_FRONTIER_R=0.20 RWM_FRONTIER_OFFSET=4
run FEC_r05_W48_off8  RWM_FRONTIER=48 RWM_FRONTIER_R=0.05 RWM_FRONTIER_OFFSET=8
run FEC_r15_W64_off2  RWM_FRONTIER=64 RWM_FRONTIER_R=0.15 RWM_FRONTIER_OFFSET=2
echo "=== TUNE DONE $(date +%T) ==="
