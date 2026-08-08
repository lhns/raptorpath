#!/bin/bash
# feat/ack-merge-flip L1 BATTERY (goal-gate "Ack-Merge Flip").
#
# The FULL-SCOPE, SINGLE-KNOB battery the 2026-08-07 ack-merge side result is
# owed. Two arms only, because the clause is single-knob:
#
#   prior = <env unset>          (today's shipped default)
#   am    = RWM_ACK_MERGE=1      (THE candidate -- the window-mode
#                                 control-datagram merge, alone)
#
# THE CLAUSE (pre-registered): c1 at or above the measured band
# (202.5 -> 223.0 s42 / 204.3 -> 223.4 s7, +10.1% / +9.3%) on BOTH seeds,
# AND receiver CPU/bit DOWN. Everything else is a NO-REGRESSION GATE, NOT a
# target -- c7 in particular is pre-registered as EXPECTED NOT TO MOVE (the
# 2026-08-07 session measured the suppressed "duplicate" ack at 1.04 per data
# message, 25x rarer than the frame it duplicates, and measured zero response
# in sidle/sweeps/gapdrop).
#
# Cells:
#   c1  single 400 MB xREPS both arms   (THE clause)
#   c7  dual   200 MB xREPS both arms   (no-regression, vs its OWN control)
#   c8  dual    25 MB xREPS both arms   (no-regression)
#   sc2 single 100 MB xREPS both arms   (no-regression; c7 Sigma source)
#   sc3 single  25 MB xREPS both arms   (no-regression; c8 Sigma source)
#   sustained  1.2 GB x1 BOTH arms      (the scope the reduced battery lacked)
#
# THE MECHANISM CHECK: the `[CTLD] p<id> tx=<n> rx=<n>` receiver-side quinn
# datagram-frame counters (net/mod.rs, RWM_DIAG-gated, behaviour-inert). At a
# window-mode receiver `tx` IS the control-frame count, so tx/rx is control
# datagrams per data message: 1.000 merged vs 1.038/1.053 default. The
# 2026-08-07 session read these BY HAND off two runs; this battery scrapes
# them on EVERY run, from the SERVER log. Plus CPUSRV/CPUCLI on every run
# (CPUSRV = the receiver = where the mechanism removes work).
#
# MEASUREMENT DISCIPLINE item 13 IS BINDING: launch detached, DO NOT POLL.
# The lock holder's own monitoring destroyed the 2026-08-07 full-scope
# attempt (171 invocations -> 26 summaries / 121 RUN-RETRY, vs 80/80/0
# unpolled). Collect ONCE, at the end.
#
# Retry-hardened per the July flake class; aborts preserved; n quoted.
#   usage: ackflip_battery.sh <seed> [reps] [c1|singles|duals|sustained|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/ackflip/battery-s${SEED_ARG}.log
DDIR=/home/vibe/ackflip/diag
mkdir -p "$DDIR" /home/vibe/ackflip
: >> "$OUT"
echo "# ackflip BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
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

# arm expect_am
# Two-sided assertion (MEASUREMENT DISCIPLINE 1 + 6): ARM-LIVENESS-FAIL when
# the mechanism the arm is named for did not echo, ARM-CONTAMINATION when it
# echoed in the arm that must not have it. The ack-merge echo is emitted in
# `run_impl`, so it MUST appear on BOTH logs -- the RECEIVER is what
# suppresses the legacy Ack (item 6: liveness at the receiver, not just the
# sender), the sender is what re-homes its consumers.
check_liveness() {
  local arm="$1" eam="$2"
  local amc ams
  cl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "$1" || true; }
  sl() { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "$1" || true; }
  amc=$(cl "ack-merge ACTIVE"); ams=$(sl "ack-merge ACTIVE")
  echo "LIVENESS arm=$arm am_c=$amc am_s=$ams (want $eam)" >> "$OUT"
  if [ "$eam" -gt 0 ]; then
    { [ "$amc" -eq 0 ] || [ "$ams" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (ack-merge echo c=$amc s=$ams)" >> "$OUT"
  else
    { [ "$amc" -gt 0 ] || [ "$ams" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP (ack-merge echo in prior arm)" >> "$OUT"
  fi
  return 0
}

run_one() { # name envs cellA cellB bytes mode e_am
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" eam="$7"
  local attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC" >> "$OUT" || true
    check_liveness "$name" "$eam"
    # THE MECHANISM CHECK (pre-registered): control-datagram density at the
    # RECEIVER, cumulative, last 1 Hz sample. `tx` = quinn DATAGRAM frames
    # sent by the receiver = control datagrams; `rx` = data datagrams in.
    # tx/rx is control datagrams per data message. Server log, every run.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -a '\[CTLD\]' | tail -2 \
      | tr '\n' ' ' | sed 's/^/CTLD /' >> "$OUT"; echo >> "$OUT") || true
    # The stall/recovery gauges, end-of-run totals (the 2026-08-07 evidence
    # set, kept so the c7 no-movement claim is checkable and not merely
    # asserted).
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|retx=[0-9]+|fired=[0-9]+|y=[0-9]+|srel=[0-9]+/[0-9]+|gapdrop=[0-9]+|mpr\[[^]]*\]|relgap[^ ]*|cwnd=[0-9]+|btlbw=[0-9]+|pa=on/[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | sed -n '5p' \
      | grep -oE "win=[0-9]+/[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|paused=[0-9.]+%|cwnd=[0-9]+|btlbw=[0-9]+|gapdrop=[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECHMID /' >> "$OUT"; echo >> "$OUT") || true
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

E_AM="RWM_ACK_MERGE=1"

# Arms interleaved round-robin per rep, candidate FIRST (discipline item 3 --
# the documented same-config drift is 2.3x, so alternation within one session
# is the only honest comparison).
if [[ "$SCOPE" == "c1" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c1-am    "$E_AM" c1 c1 400000000 single 1
    run_one c1-prior ""      c1 c1 400000000 single 0
  done
fi

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-am    "$E_AM" c2 c2 200000000 dual 1
    run_one c7-prior ""      c2 c2 200000000 dual 0
    run_one c8-am    "$E_AM" c2 c3 25000000 dual 1
    run_one c8-prior ""      c2 c3 25000000 dual 0
  done
fi

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-am    "$E_AM" c2 c2 100000000 single 1
    run_one sc2-prior ""      c2 c2 100000000 single 0
    run_one sc3-am    "$E_AM" c3 c3 25000000 single 1
    run_one sc3-prior ""      c3 c3 25000000 single 0
  done
fi

# The sustained run the reduced 2026-08-07 battery lacked -- and BOTH arms,
# so it is a comparison rather than a spot check.
if [[ "$SCOPE" == "sustained" || "$SCOPE" == "all" ]]; then
  for REP in 1 2; do
    run_one c1-am-1200M    "$E_AM" c1 c1 1200000000 single 1
    run_one c1-prior-1200M ""      c1 c1 1200000000 single 0
  done
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c1-am c1-prior c7-am c7-prior c8-am c8-prior \
         sc2-am sc2-prior sc3-am sc3-prior \
         c1-am-1200M c1-prior-1200M; do
  n=$(grep -c "=== rep=.* arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
