#!/bin/bash
# goal-gate "Store-Cap Triplication" L1 BATTERY (fix/store-cap-triplication).
#
# Single-knob, two arms:
#
#   def = <env unset>                  today's shipped default: the plain
#                                      dyn-store-cap phase sums its anchor
#                                      base over active_paths() while the
#                                      path-scaled law multiplies it by
#                                      n_live counted from live_paths()
#   uni = RWM_STORE_CAP_UNIFIED=1      THE candidate: both range over
#                                      live_paths()
#
# Pre-registered predictions (goal-gate, committed before the build):
#   P1 the effective store cap rises by 1/E - 1, E = active_sum/live_sum
#   P2 c7 in [0.97, 1.02] x Sigma and >= def - sigma      (cleanest dual)
#   P3 c8 within sigma or DOWN                            (THE RISK CELL)
#   P4 sc2 +0..+3%, sc3 +0..+5%, c1 highest headroom      (N = 1, not inert)
#   P5 dnf = 0, crown 1000/1000, retx/sweeps not worse beyond sigma
#
# THE MECHANISM CHECK is the `[SF]` gauge (net::store_cap_sf_gauge, RWM_DIAG-
# gated, behaviour-inert) plus the DIAG `win=<occupancy>/<cap>` pair: the
# pre-battery smoke measured cap 128 -> 1024 at c1 on this exact binary, and
# every run here re-scrapes both so the claim is checkable per rep.
#
# MEASUREMENT DISCIPLINE item 13 IS BINDING: launch detached, DO NOT POLL.
#   usage: storecap_battery.sh <seed> [reps] [c1|singles|duals|all]
set -uo pipefail
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/storecap/battery-s${SEED_ARG}.log
DDIR=/home/vibe/storecap/diag
mkdir -p "$DDIR" /home/vibe/storecap
: >> "$OUT"
echo "# storecap BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

# Two-sided liveness (MEASUREMENT DISCIPLINE 1 + 6). The gate is resolved in
# `sender_policy::resolve` and echoed from `run_impl`, so it prints on BOTH
# endpoints whenever configured; require it on both for `uni`, forbid it on
# both for `def`.
check_liveness() {
  local arm="$1" euni="$2"
  local uc us
  uc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "unified store-cap path set ACTIVE" || true)
  us=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "unified store-cap path set ACTIVE" || true)
  echo "LIVENESS arm=$arm uni_c=$uc uni_s=$us (want $euni)" >> "$OUT"
  if [ "$euni" -gt 0 ]; then
    { [ "$uc" -eq 0 ] || [ "$us" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (unified echo c=$uc s=$us)" >> "$OUT"
  else
    { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (unified echo in default arm)" >> "$OUT"
  fi
  return 0
}

run_one() { # name envs cellA cellB bytes mode e_uni
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" euni="$7"
  local attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC" >> "$OUT" || true
    check_liveness "$name" "$euni"
    # THE MECHANISM CHECK: the saturation-filter population, cumulative,
    # last sample of the run (client = the sender = where the phase lives).
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -a '\[SF\]' | tail -1 \
      | grep -oE "ticks=[0-9]+|live_sum=[0-9]+|active_sum=[0-9]+|short_ticks=[0-9]+|zero_ticks=[0-9]+" \
      | tr '\n' ' ' | sed 's/^/SF /' >> "$OUT"; echo >> "$OUT") || true
    # The cap ITSELF (`win=<un-SACKed>/<dyn cap>`) plus the standard
    # end-of-run gauges: P1 is a claim about the cap, so the cap is scraped.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sweeps=[0-9]+|retx=[0-9]+|y=[0-9]+|srel=[0-9]+/[0-9]+|gapdrop=[0-9]+|cwnd=[0-9]+|btlbw=[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | sed -n '5p' \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|sweeps=[0-9]+|cwnd=[0-9]+|btlbw=[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECHMID /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

E_UNI="RWM_STORE_CAP_UNIFIED=1"

# Candidate FIRST, arms interleaved round-robin per rep (discipline item 3).
if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-uni "$E_UNI" c2 c2 200000000 dual 1
    run_one c7-def ""       c2 c2 200000000 dual 0
    run_one c8-uni "$E_UNI" c2 c3  25000000 dual 1
    run_one c8-def ""       c2 c3  25000000 dual 0
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-uni "$E_UNI" c2 c2 100000000 single 1
    run_one sc2-def ""       c2 c2 100000000 single 0
    run_one sc3-uni "$E_UNI" c3 c3  25000000 single 1
    run_one sc3-def ""       c3 c3  25000000 single 0
  done
fi

if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-uni "$E_UNI" c1 c1 400000000 single 1
    run_one c1-def ""       c1 c1 400000000 single 0
  done
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c7-uni c7-def c8-uni c8-def sc2-uni sc2-def sc3-uni sc3-def c1-uni c1-def; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
