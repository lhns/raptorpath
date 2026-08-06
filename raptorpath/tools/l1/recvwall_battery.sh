#!/bin/bash
# feat/recv-permsg L1 BATTERY (goal-gate "Receiver Per-Message Wall"
# amendment): the pre-registered A/B for RWM_EST_CADENCE.
#   c1 (PRIMARY, single 400 MB): def <-> est <-> eb <-> ebest, interleaved
#   sc2 (c2 single 100 MB) + sc3 (c3 single 25 MB): def <-> est (no-regression
#     + recovery-gauge class check)
#   c7 (dual 200 MB): def <-> est (>= 0.97x same-session Sigma via sc2 arms)
# Retry-hardened per the July flake class; aborts preserved; n quoted.
#   usage: recvwall_battery.sh <seed> [reps] [c1|singles|duals|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/recvwall/battery-s${SEED_ARG}.log
DDIR=/home/vibe/recvwall/diag
mkdir -p "$DDIR" /home/vibe/recvwall
: >> "$OUT"
echo "# recvwall BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

check_liveness() { # arm expect_est expect_eb
  local arm="$1" eest="$2" eeb="$3" estc ests eb
  estc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "estimator heavy-math cadence ACTIVE" || true)
  ests=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "estimator heavy-math cadence ACTIVE" || true)
  eb=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "emission batching ACTIVE" || true)
  echo "LIVENESS arm=$arm est_c=$estc est_s=$ests (want $eest) eb=$eb (want $eeb)" >> "$OUT"
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
  return 0
}

run_one() { # name envs cellA cellB bytes mode expect_est expect_eb
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" eest="$7" eeb="$8" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    check_liveness "$name" "$eest" "$eeb"
    # Mechanism gauges: echo rtt / fired / retx classes (falsification iii).
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|fired=[0-9]+|y=[0-9]+|retx=[0-9]+|srel=[0-9]+/[0-9]+" \
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

E_EST="RWM_EST_CADENCE=1"
E_EB="RWM_EMIT_BATCH=1"
E_EBEST="RWM_EMIT_BATCH=1 RWM_EST_CADENCE=1"

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-def   ""         c1 c1 400000000 single 0 0
    run_one c1-est   "$E_EST"   c1 c1 400000000 single 1 0
    run_one c1-eb    "$E_EB"    c1 c1 400000000 single 0 1
    run_one c1-ebest "$E_EBEST" c1 c1 400000000 single 1 1
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-def ""       c2 c2 100000000 single 0 0
    run_one sc2-est "$E_EST" c2 c2 100000000 single 1 0
    run_one sc3-def ""       c3 c3 25000000 single 0 0
    run_one sc3-est "$E_EST" c3 c3 25000000 single 1 0
  done
fi

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-def ""       c2 c2 200000000 dual 0 0
    run_one c7-est "$E_EST" c2 c2 200000000 dual 1 0
  done
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c1-def c1-est c1-eb c1-ebest sc2-def sc2-est sc3-def sc3-est c7-def c7-est; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
