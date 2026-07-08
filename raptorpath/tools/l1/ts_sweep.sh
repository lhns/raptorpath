#!/bin/bash
# Transport-substrate A/B/C sweep (feat/transport-substrate).
# Per scen: ARQ (pure), FEC (fungible proactive, UNPACED baseline),
# FEC+PACE (Fix 1 CC-rate source pacing), and optionally more fix arms.
#   sudo bash ts_sweep.sh [reps] [bytes] [arms] [scens...]
# arms: space-free comma list subset of: arq,fec,fecpace,fecpace2,fecpace3
# env Mode-B tuning: BGEN BR BSTORE BINFLIGHT CCHR REACTCAP OOORETAIN
cd "$(dirname "$0")"
REPS="${1:-3}"; BYTES="${2:-1800000}"; ARMS="${3:-arq,fec,fecpace}"
shift 3 2>/dev/null || true
SCENS="${*:-c2r50 c2r100 c2r200}"
BGEN="${BGEN:-768}"; BR="${BR:-0.20}"; BSTORE="${BSTORE:-4096}"; BINFLIGHT="${BINFLIGHT:-8192}"
CCHR="${CCHR:-1.1}"; REACTCAP="${REACTCAP:-}"; OOORETAIN="${OOORETAIN:-}"

run_fec() { # $1=extra env assignments
  local extra_env="$1"
  local line
  line=$(env RWM_OOO=1 RWM_GEN="$BGEN" RWM_GEN_R="$BR" RWM_STORE="$BSTORE" \
         RWM_PIPELINE="${BPIPE:-2}" \
         RWM_GEN_INFLIGHT="$BINFLIGHT" RWM_PFRAC=1 $extra_env \
         RWM_EXTRA="--window-systematic-repair" \
         timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
  local mbps dnf pfrac pcod rcod sd
  mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
  sd=$(echo "$line" | grep -oE '"stdev_s":[0-9.]+' | cut -d: -f2)
  dnf=$(echo "$line" | grep -oE '"dnf":[0-9]+' | cut -d: -f2)
  pfrac=$(grep -oE 'proactive_fraction=[0-9.]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  pcod=$(grep -oE 'proactive_coded=[0-9]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  rcod=$(grep -oE 'recovery_coded=[0-9]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  echo "${mbps:-NA} ${dnf:-NA} ${pfrac:-NA} ${pcod:-NA} ${rcod:-NA} ${sd:-NA}"
}

echo "=== TS SWEEP reps=$REPS bytes=$BYTES arms=$ARMS  ModeB[G=$BGEN r=$BR store=$BSTORE infl=$BINFLIGHT hr=$CCHR reactcap=${REACTCAP:-def} ooo=${OOORETAIN:-off}]  $(date +%T) ==="
for scen in $SCENS; do
  IFS=',' read -ra AL <<< "$ARMS"
  for arm in "${AL[@]}"; do
    while pgrep -x raptorpath >/dev/null; do sleep 2; done
    case "$arm" in
      arq)
        line=$(timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
        mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
        sd=$(echo "$line" | grep -oE '"stdev_s":[0-9.]+' | cut -d: -f2)
        dnf=$(echo "$line" | grep -oE '"dnf":[0-9]+' | cut -d: -f2)
        printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s pfrac=- pcod=- rcod=- stdev_s=%s\n' "$scen" "$arm" "${mbps:-NA}" "${dnf:-NA}" "${sd:-NA}" ;;
      fec)      read m d p pc rc sd <<< "$(run_fec "")" ;;
      fecpace)  read m d p pc rc sd <<< "$(run_fec "RWM_CC_PACE=1 RWM_CC_PACE_HR=$CCHR")" ;;
      fecpace2) read m d p pc rc sd <<< "$(run_fec "RWM_CC_PACE=1 RWM_CC_PACE_HR=$CCHR RWM_REACT_CAP=$REACTCAP")" ;;
      fecpace3) read m d p pc rc sd <<< "$(run_fec "RWM_CC_PACE=1 RWM_CC_PACE_HR=$CCHR RWM_REACT_CAP=$REACTCAP RWM_OOO_RETAIN=$OOORETAIN")" ;;
    esac
    [[ "$arm" != arq ]] && printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s pfrac=%s pcod=%s rcod=%s stdev_s=%s\n' "$scen" "$arm" "${m:-NA}" "${d:-NA}" "${p:-NA}" "${pc:-NA}" "${rc:-NA}" "${sd:-NA}"
  done
done
echo "=== TS SWEEP DONE $(date +%T) ==="
