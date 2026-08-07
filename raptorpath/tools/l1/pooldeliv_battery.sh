#!/bin/bash
# feat/pool-delivery-anchor L1 BATTERY (goal-gate "Ship The Wins 1b: the
# delivery-clocked pool anchor"): the pre-registered attempt-2 battery.
# The defaults were REVERTED at the end of attempt 1, so env-unset == the
# shipped prior default and every candidate arm is explicit.
# Arms:
#   deliv = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1
#           (arm A: est+eb+pool-anchor+POOL_DELIV, all riding the est opt-in)
#   pa    = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_POOL_DELIV=0
#           (attempt 1 EXACTLY -- the one-knob control for the delivery term)
#   prior = <env unset>  (the shipped default: per-call BOCD, per-symbol
#           sender, legacy ack-interval pool)
#   floor = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_POOL_DELIV=0
#           RWM_FLOOR_BOUND=1   (arm B: attempt 1's pool + the honest
#           anchor-floor bound, so Sigma-cwnd is the derived dual governor)
# Cells:
#   c1  single 400 MB  xREPS deliv/pa/prior/floor   (PRIMARY >= 430)
#   sc2 single 100 MB  xREPS deliv/prior/floor      (within sigma; Sigma source)
#   sc3 single  25 MB  xREPS deliv/prior/floor      (within sigma; Sigma source)
#   c7  dual   200 MB  xREPS deliv/pa/prior/floor   (THE clause, >= 0.97x
#                       same-session Sigma; pa = the attempt-1 comparison)
#   c8  dual    25 MB  xREPS deliv/pa/prior         (the WATCH cell)
# Retry-hardened per the July flake class; aborts preserved; n quoted.
#   usage: pooldeliv_battery.sh <seed> [reps] [c1|singles|duals|sustained|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/pooldeliv/battery-s${SEED_ARG}.log
DDIR=/home/vibe/pooldeliv/diag
mkdir -p "$DDIR" /home/vibe/pooldeliv
: >> "$OUT"
echo "# pooldeliv BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
uname -r >> "$OUT"

wait_clear() {
  for _ in $(seq 1 30); do
    if ! pgrep -x raptorpath >/dev/null 2>&1 \
       && ! sudo ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000'; then
      return 0
    fi
    sudo pkill -x raptorpath 2>/dev/null || true
    sleep 1
  done
  return 0
}

# arm expect_est expect_eb expect_pa expect_pd expect_fb
# Every gate gets a two-sided assertion: an ARM-LIVENESS-FAIL when the
# mechanism the arm is named for did not echo, an ARM-CONTAMINATION when a
# mechanism echoed in an arm that must not have it (MEASUREMENT DISCIPLINE 1).
check_liveness() {
  local arm="$1" eest="$2" eeb="$3" epa="$4" epd="$5" efb="$6"
  local estc ests eb pac pas pdc pds fbc fbs
  cl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "$1" || true; }
  sl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "$1" || true; }
  estc=$(cl "estimator heavy-math cadence ACTIVE"); ests=$(sl "estimator heavy-math cadence ACTIVE")
  eb=$(cl "emission batching ACTIVE")
  pac=$(cl "pool-anchor honest dual-store law ACTIVE"); pas=$(sl "pool-anchor honest dual-store law ACTIVE")
  pdc=$(cl "pool-anchor DELIVERY-CLOCKED rate ACTIVE"); pds=$(sl "pool-anchor DELIVERY-CLOCKED rate ACTIVE")
  fbc=$(cl "honest anchor-floor BOUND ACTIVE"); fbs=$(sl "honest anchor-floor BOUND ACTIVE")
  echo "LIVENESS arm=$arm est_c=$estc est_s=$ests (want $eest) eb=$eb (want $eeb) pa_c=$pac pa_s=$pas (want $epa) pd_c=$pdc pd_s=$pds (want $epd) fb_c=$fbc fb_s=$fbs (want $efb)" >> "$OUT"
  if [ "$eest" -gt 0 ]; then
    { [ "$estc" -eq 0 ] || [ "$ests" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (est echo c=$estc s=$ests)" >> "$OUT"
  else
    { [ "$estc" -gt 0 ] || [ "$ests" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (est echo in non-est arm)" >> "$OUT"
  fi
  if [ "$eeb" -gt 0 ]; then
    [ "$eb" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no eb echo)" >> "$OUT"
  else
    [ "$eb" -gt 0 ] && echo "ARM-CONTAMINATION arm=$arm rep=$REP (eb echo in non-eb arm)" >> "$OUT"
  fi
  if [ "$epa" -gt 0 ]; then
    [ "$pac" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no pool-anchor echo on client)" >> "$OUT"
  else
    { [ "$pac" -gt 0 ] || [ "$pas" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (pa echo in non-pa arm)" >> "$OUT"
  fi
  if [ "$epd" -gt 0 ]; then
    [ "$pdc" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no DELIVERY-CLOCKED echo on client)" >> "$OUT"
  else
    { [ "$pdc" -gt 0 ] || [ "$pds" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (pd echo in non-pd arm)" >> "$OUT"
  fi
  if [ "$efb" -gt 0 ]; then
    { [ "$fbc" -eq 0 ] || [ "$fbs" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (floor-bound echo c=$fbc s=$fbs)" >> "$OUT"
  else
    { [ "$fbc" -gt 0 ] || [ "$fbs" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (fb echo in non-fb arm)" >> "$OUT"
  fi
  return 0
}

run_one() { # name envs cellA cellB bytes mode e_est e_eb e_pa e_pd e_fb
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6"
  local eest="$7" eeb="$8" epa="$9" epd="${10}" efb="${11}" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    check_liveness "$name" "$eest" "$eeb" "$epa" "$epd" "$efb"
    # Mechanism gauges (prediction A.2/1): store cap + pool engagement + the
    # echo/sweep classes + THE per-path split btlbw (legacy over-read) vs sr
    # (attempt 1's send mean) vs dr (attempt 2's delivery clock, with its
    # accepted/short/gap/discard guard counters). dr-vs-sr IS the prediction.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|retx=[0-9]+|fired=[0-9]+|y=[0-9]+|srel=[0-9]+/[0-9]+|pa=on/[0-9]+|bdp[0-9]+\(cap[0-9]+\)|cwnd=[0-9]+|btlbw=[0-9]+|sr=[0-9]+/g[0-9]+d[0-9]+|dr=[0-9]+/a[0-9]+s[0-9]+g[0-9]+d[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    # Mid-run gauge too (t~5s): the c7 operating point, not just the tail.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | sed -n '5p' \
      | grep -oE "win=[0-9]+/[0-9]+|sweeps=[0-9]+|pa=on/[0-9]+|cap[0-9]+|cwnd=[0-9]+|btlbw=[0-9]+|sr=[0-9]+|dr=[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECHMID /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-s.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

E_DELIV="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1"
E_PA="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_POOL_DELIV=0"
E_FLOOR="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_POOL_DELIV=0 RWM_FLOOR_BOUND=1"

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-deliv "$E_DELIV" c1 c1 400000000 single 1 1 1 1 0
    run_one c1-pa    "$E_PA"    c1 c1 400000000 single 1 1 1 0 0
    run_one c1-prior ""         c1 c1 400000000 single 0 0 0 0 0
    run_one c1-floor "$E_FLOOR" c1 c1 400000000 single 1 1 1 0 1
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-deliv "$E_DELIV" c2 c2 100000000 single 1 1 1 1 0
    run_one sc2-prior ""         c2 c2 100000000 single 0 0 0 0 0
    run_one sc2-floor "$E_FLOOR" c2 c2 100000000 single 1 1 1 0 1
    run_one sc3-deliv "$E_DELIV" c3 c3 25000000 single 1 1 1 1 0
    run_one sc3-prior ""         c3 c3 25000000 single 0 0 0 0 0
    run_one sc3-floor "$E_FLOOR" c3 c3 25000000 single 1 1 1 0 1
  done
fi

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-deliv "$E_DELIV" c2 c2 200000000 dual 1 1 1 1 0
    run_one c7-pa    "$E_PA"    c2 c2 200000000 dual 1 1 1 0 0
    run_one c7-prior ""         c2 c2 200000000 dual 0 0 0 0 0
    run_one c7-floor "$E_FLOOR" c2 c2 200000000 dual 1 1 1 0 1
    run_one c8-deliv "$E_DELIV" c2 c3 25000000 dual 1 1 1 1 0
    run_one c8-pa    "$E_PA"    c2 c3 25000000 dual 1 1 1 0 0
    run_one c8-prior ""         c2 c3 25000000 dual 0 0 0 0 0
  done
fi

if [[ "$SCOPE" == "sustained" || "$SCOPE" == "all" ]]; then
  REP=1
  run_one c1-deliv-1200M "$E_DELIV" c1 c1 1200000000 single 1 1 1 1 0
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c1-deliv c1-pa c1-prior c1-floor sc2-deliv sc2-prior sc2-floor \
         sc3-deliv sc3-prior sc3-floor c7-deliv c7-pa c7-prior c7-floor \
         c8-deliv c8-pa c8-prior c1-deliv-1200M; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
