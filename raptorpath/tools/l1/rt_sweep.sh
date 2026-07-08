#!/bin/bash
# Receiver-tail + FEC-favorable-regime sweep (feat/receiver-tail).
# Per scenario, compares three arms on the SAME rp-native `perf` object:
#   arq       — pure ARQ (--window-reliable, no generation coding)
#   fecprior  — full sender-substrate stack (Fix1 CC-pace + Fix2 bounded
#               reactive + Fix3 OOO retention), RECEIVER-TAIL fix OFF
#               (RWM_REPORT_GENS unset ⇒ legacy 6, no BDP in-flight cap)
#   fectail   — fecprior + PART 1 receiver-tail flush (RWM_REPORT_GENS) +
#               PART 1.2 BDP-derived in-flight cap (RWM_INFL_BDP)
#
#   sudo bash rt_sweep.sh [reps] [bytes] [arms] [scens...]
#   arms: comma list subset of arq,fecprior,fectail   (default all three)
#   Mode-B env: BGEN BR BSTORE BINFLIGHT BPIPE CCHR REACTCAP OOORETAIN
#               REPORTGENS INFLBDP
cd "$(dirname "$0")"
REPS="${1:-3}"; BYTES="${2:-6000000}"; ARMS="${3:-arq,fecprior,fectail}"
shift 3 2>/dev/null || true
SCENS="${*:-c2r100 c2r200}"
BGEN="${BGEN:-768}"; BR="${BR:-0.20}"; BSTORE="${BSTORE:-4096}"; BINFLIGHT="${BINFLIGHT:-8192}"
BPIPE="${BPIPE:-2}"; CCHR="${CCHR:-1.1}"; REACTCAP="${REACTCAP:-1.0}"; OOORETAIN="${OOORETAIN:-16}"
REPORTGENS="${REPORTGENS:-256}"; INFLBDP="${INFLBDP:-2.0}"
# fectail runs a WIDE store (many generations in flight) so the receiver-tail
# parallelization is actually exercised; the BDP in-flight cap keeps the wire
# queue bounded despite the wide retention. fecprior keeps the tuned narrow store.
BSTORE_TAIL="${BSTORE_TAIL:-16384}"; OOORETAIN_TAIL="${OOORETAIN_TAIL:-24}"

run_fec() { # $1 = extra env assignments  $2 = store  $3 = ooo_retain
  local extra_env="$1" store="${2:-$BSTORE}" ooo="${3:-$OOORETAIN}" line mbps dnf pfrac pcod rcod sd
  line=$(env RWM_OOO=1 RWM_GEN="$BGEN" RWM_GEN_R="$BR" RWM_STORE="$store" \
         RWM_PIPELINE="$BPIPE" RWM_GEN_INFLIGHT="$BINFLIGHT" RWM_PFRAC=1 \
         RWM_CC_PACE=1 RWM_CC_PACE_HR="$CCHR" RWM_REACT_CAP="$REACTCAP" \
         RWM_OOO_RETAIN="$ooo" $extra_env \
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

echo "=== RT SWEEP reps=$REPS bytes=$BYTES arms=$ARMS ModeB[G=$BGEN r=$BR store=$BSTORE infl=$BINFLIGHT hr=$CCHR react=$REACTCAP ooo=$OOORETAIN reportgens=$REPORTGENS inflbdp=$INFLBDP] $(date +%T) ==="
for scen in $SCENS; do
  IFS=',' read -ra AL <<< "$ARMS"
  for arm in "${AL[@]}"; do
    while pgrep -x raptorpath >/dev/null; do sleep 2; done
    case "$arm" in
      arq)
        line=$(timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$BYTES" "$REPS" single 2>&1 | grep -E '"summary"')
        mbps=$(echo "$line" | grep -oE '"mean_mbps":[0-9.]+' | cut -d: -f2)
        sd=$(echo "$line"   | grep -oE '"stdev_s":[0-9.]+'   | cut -d: -f2)
        dnf=$(echo "$line"  | grep -oE '"dnf":[0-9]+'        | cut -d: -f2)
        printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s pfrac=- pcod=- rcod=- stdev_s=%s\n' "$scen" "$arm" "${mbps:-NA}" "${dnf:-NA}" "${sd:-NA}" ;;
      fecprior) read m d p pc rc sd <<< "$(run_fec "" "$BSTORE" "$OOORETAIN")" ;;
      fecwide)  read m d p pc rc sd <<< "$(run_fec "" "$BSTORE_TAIL" "$OOORETAIN_TAIL")" ;;
      fectail)  read m d p pc rc sd <<< "$(run_fec "RWM_REPORT_GENS=$REPORTGENS RWM_INFL_BDP=$INFLBDP" "$BSTORE_TAIL" "$OOORETAIN_TAIL")" ;;
    esac
    [[ "$arm" != arq ]] && printf 'RESULT scen=%s arm=%s mean_mbps=%s dnf=%s pfrac=%s pcod=%s rcod=%s stdev_s=%s\n' "$scen" "$arm" "${m:-NA}" "${d:-NA}" "${p:-NA}" "${pc:-NA}" "${rc:-NA}" "${sd:-NA}"
  done
done
echo "=== RT SWEEP DONE $(date +%T) ==="
