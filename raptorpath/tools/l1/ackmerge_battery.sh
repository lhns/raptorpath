#!/bin/bash
# feat/ack-merge L1 BATTERY (goal-gate "Unlock The Default 1: ack-merge").
# The pre-registered four-arm battery. Everything unset == the shipped
# default (the est/eb/pool-anchor family was REVERTED at the end of
# "Ship The Wins 1b"), so every candidate arm is explicit.
#
# Arms:
#   prior = <env unset>                (today's shipped default)
#   est   = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1
#           (reproduces the 16.37 c7 blocker: 0.958-0.977 / 0.931-0.956)
#   merge = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_ACK_MERGE=1
#           (THE candidate -- the composed default flip under test)
#   am    = RWM_ACK_MERGE=1
#           (isolates the duplicate ack's own cost on today's default)
#
# Cells:
#   c7  dual   200 MB xREPS all four arms  (THE clause, >= 0.97x same-session
#              Sigma) + the sidle/sweeps/mpr/gapdrop/paused/relgap gauges
#   c1  single 400 MB xREPS all four arms  (PRIMARY >= 430)
#   sc2 single 100 MB xREPS all four arms  (within sigma; c7 Sigma source)
#   sc3 single  25 MB xREPS all four arms  (within sigma; c8 Sigma source)
#   c8  dual    25 MB xREPS all four arms  (>= 0.87 line)
#   sustained  1.2 GB x1 merge
#
# THE DENSITY GAUGE (prediction 1, the mechanism proof independent of
# throughput): the ACK-DIRECTION qdisc packet counters. perf_rwm_c.sh already
# prints `QDISC srv0/srv1` (receiver -> sender) beside the data-direction
# `QDISC cli0/cli1`; this driver captures BOTH on every run. Control datagrams
# per MB must fall ~2x in the merge arms. Captured on EVERY run, not sampled.
#
# Retry-hardened per the July flake class; aborts preserved; n quoted.
#   usage: ackmerge_battery.sh <seed> [reps] [c1|singles|duals|sustained|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/ackmerge/battery-s${SEED_ARG}.log
DDIR=/home/vibe/ackmerge/diag
mkdir -p "$DDIR" /home/vibe/ackmerge
: >> "$OUT"
echo "# ackmerge BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

# arm expect_est expect_eb expect_am
# Two-sided assertion per gate (MEASUREMENT DISCIPLINE 1): ARM-LIVENESS-FAIL
# when the mechanism the arm is named for did not echo, ARM-CONTAMINATION when
# a mechanism echoed in an arm that must not have it. The ack-merge echo is
# emitted in run_impl, so it MUST appear on BOTH logs (the receiver is what
# suppresses the legacy Ack, the sender is what re-homes its consumers).
check_liveness() {
  local arm="$1" eest="$2" eeb="$3" eam="$4"
  local estc ests eb amc ams
  cl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "$1" || true; }
  sl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "$1" || true; }
  estc=$(cl "estimator heavy-math cadence ACTIVE"); ests=$(sl "estimator heavy-math cadence ACTIVE")
  eb=$(cl "emission batching ACTIVE")
  amc=$(cl "ack-merge ACTIVE"); ams=$(sl "ack-merge ACTIVE")
  echo "LIVENESS arm=$arm est_c=$estc est_s=$ests (want $eest) eb=$eb (want $eeb) am_c=$amc am_s=$ams (want $eam)" >> "$OUT"
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
  if [ "$eam" -gt 0 ]; then
    { [ "$amc" -eq 0 ] || [ "$ams" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (ack-merge echo c=$amc s=$ams)" >> "$OUT"
  else
    { [ "$amc" -gt 0 ] || [ "$ams" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (ack-merge echo in non-merge arm)" >> "$OUT"
  fi
  return 0
}

run_one() { # name envs cellA cellB bytes mode e_est e_eb e_am
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6"
  local eest="$7" eeb="$8" eam="$9" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    # NOTE the grep: BOTH qdisc directions. `QDISC srv*` is the ACK direction
    # and is THE density gauge (prediction 1) -- prior batteries kept only
    # `QDISC cli*` (the data direction).
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC" >> "$OUT" || true
    check_liveness "$name" "$eest" "$eeb" "$eam"
    # THE mechanism-evidence gauges (goal clause 4 / prediction 3): the
    # stall-idle + sweep signature that 16.37 measured invariant across every
    # pool variant, plus the recovery/gap-channel counters the pre-registration
    # names. End-of-run totals.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|retx=[0-9]+|fired=[0-9]+|y=[0-9]+|srel=[0-9]+/[0-9]+|gapdrop=[0-9]+|mpr\[[^]]*\]|relgap[^ ]*|cwnd=[0-9]+|btlbw=[0-9]+|pa=on/[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    # Mid-run gauge (t~5s): the c7 operating point, not just the tail.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | sed -n '5p' \
      | grep -oE "win=[0-9]+/[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|paused=[0-9.]+%|cwnd=[0-9]+|btlbw=[0-9]+|gapdrop=[0-9]+" \
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

E_EST="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1"
E_MERGE="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_ACK_MERGE=1"
E_AM="RWM_ACK_MERGE=1"

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-merge "$E_MERGE" c2 c2 200000000 dual 1 1 1
    run_one c7-est   "$E_EST"   c2 c2 200000000 dual 1 1 0
    run_one c7-prior ""         c2 c2 200000000 dual 0 0 0
    run_one c7-am    "$E_AM"    c2 c2 200000000 dual 0 0 1
    run_one c8-merge "$E_MERGE" c2 c3 25000000 dual 1 1 1
    run_one c8-est   "$E_EST"   c2 c3 25000000 dual 1 1 0
    run_one c8-prior ""         c2 c3 25000000 dual 0 0 0
    run_one c8-am    "$E_AM"    c2 c3 25000000 dual 0 0 1
  done
fi

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-merge "$E_MERGE" c1 c1 400000000 single 1 1 1
    run_one c1-est   "$E_EST"   c1 c1 400000000 single 1 1 0
    run_one c1-prior ""         c1 c1 400000000 single 0 0 0
    run_one c1-am    "$E_AM"    c1 c1 400000000 single 0 0 1
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-merge "$E_MERGE" c2 c2 100000000 single 1 1 1
    run_one sc2-est   "$E_EST"   c2 c2 100000000 single 1 1 0
    run_one sc2-prior ""         c2 c2 100000000 single 0 0 0
    run_one sc2-am    "$E_AM"    c2 c2 100000000 single 0 0 1
    run_one sc3-merge "$E_MERGE" c3 c3 25000000 single 1 1 1
    run_one sc3-est   "$E_EST"   c3 c3 25000000 single 1 1 0
    run_one sc3-prior ""         c3 c3 25000000 single 0 0 0
    run_one sc3-am    "$E_AM"    c3 c3 25000000 single 0 0 1
  done
fi

if [[ "$SCOPE" == "sustained" || "$SCOPE" == "all" ]]; then
  REP=1
  run_one c1-merge-1200M "$E_MERGE" c1 c1 1200000000 single 1 1 1
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c7-merge c7-est c7-prior c7-am c8-merge c8-est c8-prior c8-am \
         c1-merge c1-est c1-prior c1-am \
         sc2-merge sc2-est sc2-prior sc2-am sc3-merge sc3-est sc3-prior sc3-am \
         c1-merge-1200M; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
