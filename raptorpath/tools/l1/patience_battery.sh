#!/bin/bash
# feat/derived-patience L1 BATTERY
# (goal-gate "Unlock The Default 2: derived patience").
#
# The pre-registered FULL four-arm battery -- flip-eligible, unlike attempt
# 1's reduced one. Everything unset == the shipped default, so every
# candidate arm is explicit.
#
# Arms (ALL carry RWM_SIDLE_DERIVED=1, which is DIAG-only and behaviour-inert:
# it adds the sidle2=/idle2= fields beside the UNCHANGED legacy sidle=/idle=,
# and RWM_DIAG=1 is already set in every arm of every battery in this file.
# That is what makes prediction 1 -- the 3a artifact verdict -- measurable on
# the CONTROL arms, which is where it has to be measured):
#   prior   = <default>                     (today's shipped default)
#   est     = RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1
#             (reproduces the 16.37/16.39 c7 blocker: 0.955/0.924 in attempt 1)
#   pat     = est + RWM_PATIENCE_DERIVED=1  (THE candidate)
#   patonly = RWM_PATIENCE_DERIVED=1        (isolates the derived floor's own
#                                            effect off the est clock)
#
# Cells:
#   c7  dual   200 MB xREPS all four arms  (THE clause, >= 0.97x same-session
#              Sigma) + sidle/sidle2/sweeps/retx/mpr[..pf=..]/gapdrop/paused
#   c1  single 400 MB xREPS all four arms  (PRIMARY >= 430)
#   sc2 single 100 MB xREPS all four arms  (within sigma; c7 Sigma source)
#   sc3 single  25 MB xREPS all four arms  (within sigma; c8 Sigma source)
#   c8  dual    25 MB xREPS all four arms  (>= 0.87 line)
#   sustained  1.2 GB x1 pat
#
# THE MECHANISM GAUGES:
#   pf=<floor>/<clock>/<mean floor us> inside mpr[..] -- how many RFC 9002
#     6.1.2 threshold evaluations were pinned by the kGranularity FLOOR versus
#     governed by the 9/8*srtt CLOCK. "Patience demonstrably derived" (the
#     falsification clause's own words) means the floor term collapses.
#   sidle2=..ms/../mx..ms evt=..us sthr=..us -- the derived stall gauge and
#     the MEASURED inter-emission-event interval that drives it. Prediction 1.
#
# MEASUREMENT DISCIPLINE item 13 IS BINDING: launch detached, DO NOT POLL.
#   usage: patience_battery.sh <seed> [reps] [c1|singles|duals|sustained|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/patience/battery-s${SEED_ARG}.log
DDIR=/home/vibe/patience/diag
mkdir -p "$DDIR" /home/vibe/patience
: >> "$OUT"
echo "# patience BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

# arm expect_est expect_eb expect_pat
# Two-sided assertion per gate (MEASUREMENT DISCIPLINE 1): ARM-LIVENESS-FAIL
# when the mechanism the arm is named for did not echo, ARM-CONTAMINATION when
# a mechanism echoed in an arm that must not have it. The derived-patience echo
# is emitted in run_impl, so it appears on BOTH logs; the derived-stall-gauge
# echo must appear on EVERY arm (it is set unconditionally) -- an arm missing
# it invalidates prediction 1 for that run and is flagged.
check_liveness() {
  local arm="$1" eest="$2" eeb="$3" epat="$4"
  local estc ests eb patc pats sdc sds
  cl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "$1" || true; }
  sl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "$1" || true; }
  estc=$(cl "estimator heavy-math cadence ACTIVE"); ests=$(sl "estimator heavy-math cadence ACTIVE")
  eb=$(cl "emission batching ACTIVE")
  patc=$(cl "derived patience ACTIVE"); pats=$(sl "derived patience ACTIVE")
  sdc=$(cl "derived stall gauge ACTIVE"); sds=$(sl "derived stall gauge ACTIVE")
  echo "LIVENESS arm=$arm est_c=$estc est_s=$ests (want $eest) eb=$eb (want $eeb) pat_c=$patc pat_s=$pats (want $epat) sd_c=$sdc sd_s=$sds (want 1)" >> "$OUT"
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
  if [ "$epat" -gt 0 ]; then
    { [ "$patc" -eq 0 ] || [ "$pats" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (derived-patience echo c=$patc s=$pats)" >> "$OUT"
  else
    { [ "$patc" -gt 0 ] || [ "$pats" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (derived-patience echo in non-pat arm)" >> "$OUT"
  fi
  { [ "$sdc" -eq 0 ] || [ "$sds" -eq 0 ]; } && echo "ARM-GAUGE-FAIL arm=$arm rep=$REP (derived-stall-gauge echo c=$sdc s=$sds)" >> "$OUT"
  return 0
}

run_one() { # name envs cellA cellB bytes mode e_est e_eb e_pat
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6"
  local eest="$7" eeb="$8" epat="$9" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_SIDLE_DERIVED=1 RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC" >> "$OUT" || true
    check_liveness "$name" "$eest" "$eeb" "$epat"
    # THE mechanism-evidence gauges (goal clause 4 / predictions 1 + 3):
    # end-of-run totals off the last [DIAG] line, INCLUDING the new pf= split
    # and the derived stall gauge with its measured event interval.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sidle2=[0-9]+ms/[0-9]+/mx[0-9]+ms|evt=[0-9]+us|sthr=[0-9]+us|sweeps=[0-9]+|retx=[0-9]+|gapdrop=[0-9]+|mpr\[[^]]*\]|relgap[^ ]*|cwnd=[0-9]+|src=[0-9]+sym/s" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    # Mid-run gauge (t~5s): the operating point, not just the tail.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | sed -n '5p' \
      | grep -oE "win=[0-9]+/[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sidle2=[0-9]+ms/[0-9]+/mx[0-9]+ms|evt=[0-9]+us|sthr=[0-9]+us|sweeps=[0-9]+|paused=[0-9.]+%|cwnd=[0-9]+|gapdrop=[0-9]+|mpr\[[^]]*\]" \
      | tr '\n' ' ' | sed 's/^/MECHMID /' >> "$OUT"; echo >> "$OUT") || true
    # The receiver-side twin of the derived stall gauge (prediction 1's
    # wire-truth counterpart): last [WIDLE] line from the SERVER log.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep -a '\[WIDLE\]' | tail -1 \
      | sed 's/^/WIDLE /' >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-s.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

E_EST="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1"
E_PAT="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_PATIENCE_DERIVED=1"
E_PO="RWM_PATIENCE_DERIVED=1"

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-pat     "$E_PAT" c2 c2 200000000 dual 1 1 1
    run_one c7-est     "$E_EST" c2 c2 200000000 dual 1 1 0
    run_one c7-prior   ""       c2 c2 200000000 dual 0 0 0
    run_one c7-patonly "$E_PO"  c2 c2 200000000 dual 0 0 1
    run_one c8-pat     "$E_PAT" c2 c3 25000000 dual 1 1 1
    run_one c8-est     "$E_EST" c2 c3 25000000 dual 1 1 0
    run_one c8-prior   ""       c2 c3 25000000 dual 0 0 0
    run_one c8-patonly "$E_PO"  c2 c3 25000000 dual 0 0 1
  done
fi

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-pat     "$E_PAT" c1 c1 400000000 single 1 1 1
    run_one c1-est     "$E_EST" c1 c1 400000000 single 1 1 0
    run_one c1-prior   ""       c1 c1 400000000 single 0 0 0
    run_one c1-patonly "$E_PO"  c1 c1 400000000 single 0 0 1
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-pat     "$E_PAT" c2 c2 100000000 single 1 1 1
    run_one sc2-est     "$E_EST" c2 c2 100000000 single 1 1 0
    run_one sc2-prior   ""       c2 c2 100000000 single 0 0 0
    run_one sc2-patonly "$E_PO"  c2 c2 100000000 single 0 0 1
    run_one sc3-pat     "$E_PAT" c3 c3 25000000 single 1 1 1
    run_one sc3-est     "$E_EST" c3 c3 25000000 single 1 1 0
    run_one sc3-prior   ""       c3 c3 25000000 single 0 0 0
    run_one sc3-patonly "$E_PO"  c3 c3 25000000 single 0 0 1
  done
fi

if [[ "$SCOPE" == "sustained" || "$SCOPE" == "all" ]]; then
  REP=1
  run_one c1-pat-1200M "$E_PAT" c1 c1 1200000000 single 1 1 1
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c7-pat c7-est c7-prior c7-patonly c8-pat c8-est c8-prior c8-patonly \
         c1-pat c1-est c1-prior c1-patonly \
         sc2-pat sc2-est sc2-prior sc2-patonly sc3-pat sc3-est sc3-prior sc3-patonly \
         c1-pat-1200M; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
