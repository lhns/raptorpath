#!/bin/bash
# PROACTIVE-FEC-vs-ARQ crossover RTT sweep (branch feat/proactive-fec-highrtt).
#
# Mode A = pure-ARQ: default plain-reliable single-path (no FEC env).
# Mode B = proactive systematic FEC: --window-systematic-repair + out-of-order
#          object completion (decode-on-total, no in-order frontier), with a
#          BDP-scaled store/in-flight window and HIGH upfront proactive r so
#          holes decode from upfront repair with ~zero reactive round-trips.
#
# Cells c2r10..c2r200 (lib.sh): 100mbit, GE 1.3/50 ~2.6% mean loss, jitter=0 so
# RTT is the ONLY swept variable, one_way = RTT/2.
#
# For Mode B the proactive-recovery FRACTION is read from the server log's
# [PFRAC] trace (RWM_PFRAC=1): proactive_coded / (proactive+recovery) coded.
#
#   sudo bash pf_sweep.sh [reps] [bytes] [scens...]
# Mode B tuning via env (defaults chosen for the crossover proof):
#   BGEN=768 BR=0.30 BSTORE=4096 BINFLIGHT=8192   (BDP@RTT200 ~1700 sym; store≫BDP)
cd "$(dirname "$0")"
REPS="${1:-5}"
BYTES="${2:-1800000}"
shift 2 2>/dev/null || true
SCENS="${*:-c2r10 c2r50 c2r100 c2r200}"

BGEN="${BGEN:-768}"
BR="${BR:-0.30}"
BSTORE="${BSTORE:-4096}"
BINFLIGHT="${BINFLIGHT:-8192}"

echo "=== PF SWEEP reps=$REPS bytes=$BYTES  ModeB[G=$BGEN r=$BR store=$BSTORE infl=$BINFLIGHT]  $(date +%T) ==="
for scen in $SCENS; do
  for arm in ARQ FEC; do
    while pgrep -x raptorpath >/dev/null; do sleep 2; done
    if [[ "$arm" == FEC ]]; then
      # Transport-substrate fixes propagate via env when set by the caller:
      #   CCPACE=1     -> RWM_CC_PACE (Fix 1: CC-rate source pacing)
      #   CCHR=<f>     -> RWM_CC_PACE_HR (pacing headroom, default 1.1)
      #   REACTCAP=<n> -> RWM_REACT_CAP (Fix 2: bounded reactive symbols/round)
      #   OOORETAIN=1  -> RWM_OOO_RETAIN (Fix 3: OOO retention decouple)
      line=$(RWM_OOO=1 RWM_GEN="$BGEN" RWM_GEN_R="$BR" RWM_STORE="$BSTORE" \
             RWM_GEN_INFLIGHT="$BINFLIGHT" RWM_PFRAC=1 \
             RWM_CC_PACE="${CCPACE:-}" RWM_CC_PACE_HR="${CCHR:-}" \
             RWM_REACT_CAP="${REACTCAP:-}" RWM_OOO_RETAIN="${OOORETAIN:-}" \
             RWM_EXTRA="--window-systematic-repair" \
             timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
      pfrac=$(grep -oE 'proactive_fraction=[0-9.]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
      pcod=$(grep -oE 'proactive_coded=[0-9]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
      rcod=$(grep -oE 'recovery_coded=[0-9]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
    else
      line=$(timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
      pfrac="-"; pcod="-"; rcod="-"
    fi
    mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
    dnf=$(echo "$line" | grep -oE '"dnf":[0-9]+' | cut -d: -f2)
    printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s pfrac=%s pcod=%s rcod=%s\n' \
      "$scen" "$arm" "${mbps:-NA}" "${dnf:-NA}" "${pfrac:-NA}" "${pcod:-NA}" "${rcod:-NA}"
  done
done
echo "=== PF SWEEP DONE $(date +%T) ==="
