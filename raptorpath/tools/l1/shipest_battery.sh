#!/bin/bash
# feat/ship-est-cadence L1 BATTERY (goal-gate "Ship The Wins 1:
# est×honest-anchor"): the pre-registered composed-default flip battery.
# Arms:
#   new    = env unset            (est-cadence ON + emit-batch ON + pool-anchor ON
#                                  — the candidate default)
#   prior  = RWM_EST_CADENCE=0 RWM_EMIT_BATCH=0
#                                  (the full prior default: per-call BOCD,
#                                   per-symbol sender, legacy ack-interval pool)
#   estonly= RWM_POOL_ANCHOR=0 RWM_EMIT_BATCH=0
#                                  (the §16.35 c7-blocker reproduction control)
# Cells:
#   c1  single 400 MB  ×REPS new<->prior   (PRIMARY >= 430 new, both seeds)
#   sc2 single 100 MB  ×REPS new<->prior   (within sigma)
#   sc3 single  25 MB  ×REPS new<->prior   (within sigma)
#   c7  dual   200 MB  ×REPS new<->prior<->estonly  (>= 0.97x same-session
#                       Sigma via the sc2 arms; estonly = the blocker control)
#   c8  dual    25 MB  ×REPS new<->prior   (vs the 0.87x line / shipped class)
# Retry-hardened per the July flake class; aborts preserved; n quoted.
#   usage: shipest_battery.sh <seed> [reps] [c1|singles|duals|sustained|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/shipest/battery-s${SEED_ARG}.log
DDIR=/home/vibe/shipest/diag
mkdir -p "$DDIR" /home/vibe/shipest
: >> "$OUT"
echo "# shipest BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

check_liveness() { # arm expect_est expect_eb expect_pa
  local arm="$1" eest="$2" eeb="$3" epa="$4" estc ests eb pac pas
  estc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "estimator heavy-math cadence ACTIVE" || true)
  ests=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "estimator heavy-math cadence ACTIVE" || true)
  eb=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "emission batching ACTIVE" || true)
  pac=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "pool-anchor honest dual-store law ACTIVE" || true)
  pas=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "pool-anchor honest dual-store law ACTIVE" || true)
  echo "LIVENESS arm=$arm est_c=$estc est_s=$ests (want $eest) eb=$eb (want $eeb) pa_c=$pac pa_s=$pas (want $epa)" >> "$OUT"
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
  return 0
}

run_one() { # name envs cellA cellB bytes mode expect_est expect_eb expect_pa
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" eest="$7" eeb="$8" epa="$9" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    check_liveness "$name" "$eest" "$eeb" "$epa"
    # Mechanism gauges (predictions 1/e): store cap + pool-anchor engagement
    # + echo/fired/sweep classes + the per-path btlbw-vs-sr split.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|retx=[0-9]+|fired=[0-9]+|y=[0-9]+|srel=[0-9]+/[0-9]+|pa=on/[0-9]+|bdp[0-9]+\(cap[0-9]+\)|btlbw=[0-9]+|sr=[0-9]+/g[0-9]+d[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
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

E_PRIOR="RWM_EST_CADENCE=0 RWM_EMIT_BATCH=0"
E_ESTONLY="RWM_POOL_ANCHOR=0 RWM_EMIT_BATCH=0"

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-new   ""         c1 c1 400000000 single 1 1 1
    run_one c1-prior "$E_PRIOR" c1 c1 400000000 single 0 0 0
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-new   ""         c2 c2 100000000 single 1 1 1
    run_one sc2-prior "$E_PRIOR" c2 c2 100000000 single 0 0 0
    run_one sc3-new   ""         c3 c3 25000000 single 1 1 1
    run_one sc3-prior "$E_PRIOR" c3 c3 25000000 single 0 0 0
  done
fi

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-new     ""           c2 c2 200000000 dual 1 1 1
    run_one c7-prior   "$E_PRIOR"   c2 c2 200000000 dual 0 0 0
    run_one c7-estonly "$E_ESTONLY" c2 c2 200000000 dual 1 0 0
    run_one c8-new     ""           c2 c3 25000000 dual 1 1 1
    run_one c8-prior   "$E_PRIOR"   c2 c3 25000000 dual 0 0 0
  done
fi

if [[ "$SCOPE" == "sustained" || "$SCOPE" == "all" ]]; then
  REP=1
  run_one c1-new-1200M "" c1 c1 1200000000 single 1 1 1
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c1-new c1-prior sc2-new sc2-prior sc3-new sc3-prior c7-new c7-prior c7-estonly c8-new c8-prior c1-new-1200M; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
