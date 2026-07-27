#!/bin/bash
# diag/lossy-residual L1 BATTERY (goal-gate "Lossy-Single Residual"): the
# pre-registered A/B for RWM_RECOV_SP (single-path RFC9002 time-threshold
# hole suppression) at the two lossy single cells:
#   sc2 = c2 single 100 MB (steady state), sc3 = c3 single 25 MB (the bar
#   geometry). Arms def (env unset) <-> sp (RWM_RECOV_SP=1) interleaved
#   per rep within one session; fresh topology per invocation; 1 run per
#   invocation; RWM_DIAG=1 everywhere (the mechanism gauge: fired/y/supp).
# Retry-hardened per the diagnosis session's flake class: wait for port
# 7000 free + no live raptorpath before each invocation; retry an
# invocation up to 3x when it produced no summary (RUN-RETRY recorded, the
# abort preserved); a run lost 3x is RUN-LOST (recorded, n per arm quoted).
#
#   usage: lossy_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/lossyres/battery-s${SEED_ARG}.log
DDIR=/home/vibe/lossyres/diag
mkdir -p "$DDIR" /home/vibe/lossyres
: > "$OUT"
echo "# lossy-residual BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
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

run_one() { # name envs cell bytes  -> appends result lines
  local name="$1" envs="$2" cell="$3" bytes="$4" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$cell" "$cell" bulk "$bytes" 1 single 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli0" >> "$OUT" || true
    # Liveness (discipline 1): the sp echo must match the arm.
    local sp
    sp=$(grep -ac "single-path hole-law suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
    echo "LIVENESS sp=$sp (expect $([[ -n "$envs" ]] && echo '>=1' || echo 0))" >> "$OUT"
    # Mechanism gauge: last DIAG mpr + cum totals.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "cum=[0-9]+/[0-9]+/[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|retx=[0-9]+|sweeps=[0-9]+|mpr\[[^]]*\]" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1; break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-s.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

for REP in $(seq 1 $REPS); do
  run_one sc2-def "" c2 100000000
  run_one sc2-sp  "RWM_RECOV_SP=1" c2 100000000
  run_one sc3-def "" c3 25000000
  run_one sc3-sp  "RWM_RECOV_SP=1" c3 25000000
done

# ARMCOUNT assertion (discipline 7): summaries per arm, loudly.
for a in sc2-def sc2-sp sc3-def sc3-sp; do
  n=$(awk "/=== rep=.* arm=$a /{f=1} f&&/\"summary\":true/{c++;f=0} END{print c+0}" "$OUT")
  echo "ARMCOUNT $a n=$n (target $REPS)" >> "$OUT"
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE
