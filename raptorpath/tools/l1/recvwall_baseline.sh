#!/bin/bash
# feat/recv-permsg STEP 0 — re-baseline the engine walls on the v5 wire
# (goal-gate "Receiver Per-Message Wall" (c)): c1 single 400 MB def <-> eb
# (RWM_EMIT_BATCH=1) interleaved, plus engine-sink probes single-c1/dual-c1
# with RWM_RDIAG=1 (the 16.23 methodology). One seed per invocation.
#   usage: recvwall_baseline.sh <seed> [reps] [probes(0|1)]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-4}"; PROBES="${3:-1}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/recvwall/baseline-s${SEED_ARG}.log
DDIR=/home/vibe/recvwall/diag
mkdir -p "$DDIR" /home/vibe/recvwall
: >> "$OUT"
echo "# recvwall STEP0 re-baseline $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS probes=$PROBES" >> "$OUT"
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

check_liveness() { # arm expect_eb
  local arm="$1" eeb="$2" eb mtu
  eb=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "emission batching ACTIVE" || true)
  mtu=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "compact DATA framing ACTIVE" || true)
  echo "LIVENESS arm=$arm eb=$eb/$eeb v5=$mtu" >> "$OUT"
  [ "$eeb" -gt 0 ] && [ "$eb" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no eb echo)" >> "$OUT"
  [ "$eeb" -eq 0 ] && [ "$eb" -gt 0 ] && echo "ARM-CONTAMINATION arm=$arm rep=$REP (eb echo in def arm)" >> "$OUT"
  [ "$mtu" -eq 0 ] && echo "V5-DEFAULT-MISSING arm=$arm rep=$REP (compact echo absent)" >> "$OUT"
  return 0
}

run_one() { # name envs cellA cellB bytes mode expect_eb
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" eeb="$7" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    check_liveness "$name" "$eeb"
    # RDIAG lines live in the SERVER log (bulk receiver).
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep -a '\[RDIAG\]' \
      | tail -6 | sed 's/^/RDIAG-TAIL /' >> "$OUT") || true
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|fired=[0-9]+|retx=[0-9]+|srel=[0-9]+/[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/base-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      cp /tmp/rwm-s.log "$DDIR/base-${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/base-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

for REP in $(seq 1 $REPS); do
  run_one c1-def ""                 c1 c1 400000000 single 0
  run_one c1-eb  "RWM_EMIT_BATCH=1" c1 c1 400000000 single 1
done

if [[ "$PROBES" == "1" ]]; then
  for REP in 1 2; do
    run_one sink-sc1-def "RWM_RDIAG=1"                 c1 c1 400000000 single 0
    run_one sink-sc1-eb  "RWM_RDIAG=1 RWM_EMIT_BATCH=1" c1 c1 400000000 single 1
    run_one sink-dc1-def "RWM_RDIAG=1"                 c1 c1 400000000 dual   0
    run_one sink-dc1-eb  "RWM_RDIAG=1 RWM_EMIT_BATCH=1" c1 c1 400000000 dual   1
  done
fi

echo "# done $(date -u +%FT%TZ)" >> "$OUT"
for a in c1-def c1-eb sink-sc1-def sink-sc1-eb sink-dc1-def sink-dc1-eb; do
  n=$(grep -c "arm=$a " "$OUT" 2>/dev/null || true)
  echo "ARMCOUNT $a invocations=$n" >> "$OUT"
done
