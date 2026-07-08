#!/bin/bash
# Repair-wait horizon sweep (feat/nack-timing). THE decisive test of the
# FEC-before-ARQ discipline: on a lossy high-RTT cell, does delaying the
# reactive deficit/NACK by a repair-coverage horizon let the in-flight
# PROACTIVE repair decode the hole first — climbing the proactive fraction
# from ~0.4 toward >0.9 and (the hoped-for headline) letting proactive FEC
# BEAT round-trip-bound ARQ on throughput?
#
#   sudo bash rw_sweep.sh [reps] [bytes] [waits_ms] [scens...]
#     waits_ms : comma list of RWM_REPAIR_WAIT values (ms). 0 = shipped path.
#   Narrow-store FEC arm (the tuned operating point) + an ARQ baseline per cell.
#   Mode-B env overrides: BGEN BR BSTORE BINFLIGHT BPIPE CCHR REACTCAP OOORETAIN
cd "$(dirname "$0")"
REPS="${1:-2}"; BYTES="${2:-15000000}"; WAITS="${3:-0,2,4,8,16}"
shift 3 2>/dev/null || true
SCENS="${*:-c2r100l10}"
BGEN="${BGEN:-384}"; BR="${BR:-0.35}"; BSTORE="${BSTORE:-4096}"; BINFLIGHT="${BINFLIGHT:-8192}"
BPIPE="${BPIPE:-2}"; CCHR="${CCHR:-1.1}"; REACTCAP="${REACTCAP:-1.0}"; OOORETAIN="${OOORETAIN:-16}"

run_fec() { # $1 = repair_wait_ms
  local rw="$1" line mbps dnf pfrac pcod rcod sd
  line=$(env RWM_OOO=1 RWM_GEN="$BGEN" RWM_GEN_R="$BR" RWM_STORE="$BSTORE" \
         RWM_PIPELINE="$BPIPE" RWM_GEN_INFLIGHT="$BINFLIGHT" RWM_PFRAC=1 \
         RWM_CC_PACE=1 RWM_CC_PACE_HR="$CCHR" RWM_REACT_CAP="$REACTCAP" \
         RWM_OOO_RETAIN="$OOORETAIN" RWM_REPAIR_WAIT="$rw" \
         RWM_EXTRA="--window-systematic-repair" \
         timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 \
         | grep -E '"summary"')
  mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
  sd=$(echo "$line"   | grep -oE '"stdev_s":[0-9.]+'   | cut -d: -f2)
  dnf=$(echo "$line"  | grep -oE '"dnf":[0-9]+'        | cut -d: -f2)
  pfrac=$(grep -oE 'proactive_fraction=[0-9.]+' /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  pcod=$(grep -oE 'proactive_coded=[0-9]+'   /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  rcod=$(grep -oE 'recovery_coded=[0-9]+'    /tmp/rwm-c.log 2>/dev/null | tail -1 | cut -d= -f2)
  echo "${mbps:-NA} ${dnf:-NA} ${pfrac:-NA} ${pcod:-NA} ${rcod:-NA} ${sd:-NA}"
}

echo "=== RW SWEEP reps=$REPS bytes=$BYTES waits=$WAITS ModeB[G=$BGEN r=$BR store=$BSTORE infl=$BINFLIGHT hr=$CCHR react=$REACTCAP ooo=$OOORETAIN] $(date +%T) ==="
for scen in $SCENS; do
  # ARQ baseline (pure --window-reliable, no coding).
  while pgrep -x raptorpath >/dev/null; do sleep 2; done
  line=$(timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
  ambps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
  asd=$(echo "$line"   | grep -oE '"stdev_s":[0-9.]+'   | cut -d: -f2)
  adnf=$(echo "$line"  | grep -oE '"dnf":[0-9]+'        | cut -d: -f2)
  printf 'RESULT scen=%s arm=arq wait=- mean_mbps=%s dnf=%s pfrac=- pcod=- rcod=- stdev_s=%s\n' "$scen" "${ambps:-NA}" "${adnf:-NA}" "${asd:-NA}"
  IFS=',' read -ra WL <<< "$WAITS"
  for rw in "${WL[@]}"; do
    while pgrep -x raptorpath >/dev/null; do sleep 2; done
    read m d p pc rc sd <<< "$(run_fec "$rw")"
    printf 'RESULT scen=%s arm=fec wait=%s mean_mbps=%s dnf=%s pfrac=%s pcod=%s rcod=%s stdev_s=%s\n' \
      "$scen" "$rw" "${m:-NA}" "${d:-NA}" "${p:-NA}" "${pc:-NA}" "${rc:-NA}" "${sd:-NA}"
  done
done
echo "=== RW SWEEP DONE $(date +%T) ==="
