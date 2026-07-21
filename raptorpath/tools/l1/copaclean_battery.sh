#!/bin/bash
# feat/copa-sole-clean L1 battery: Copa-sole on the CLEAN substrate
# (goal-gate "Copa-Sole on Clean Substrate" pre-registration).
#
# Question: the #82 bulk gap (Copa-sole 0.86-0.89x BBR at sc2, 0.73-0.76x
# at c7) predates walls 8+9 (RWM_RECOV_MP, RWM_STORE_SACK_RELEASE) and the
# consolidated defaults; those walls throttled exactly the steady full-pipe
# regime where Copa trailed. Re-measure BOTH CC arms on the full current
# default stack (everything default ON; only RWM_QUIC_CC differs).
#
# Arms (interleaved round-robin per rep, 1 run/invocation, fresh tunnel per
# invocation, RWM_DIAG=1 everywhere, per-arm CC liveness asserted):
#   A = env unset            (BBR-under — the shipped default)
#   B = RWM_QUIC_CC=passthrough (Copa-sole: wire signal + delta(hint) + feed
#       defaults engage; RWM_COPA_COMPETE stays at its default OFF)
# Cells: sc2 (100MB), sc3 (25MB), c7 (200MB), c8 (25MB), dc1 (400MB).
# Queue/RTT distributions come from the copied per-run client DIAG logs
# (rtt=app-echo / wrtt=wire / rtp per path) — parsed offline.
# After the reps: delta(hint) liveness one-offs on arm B (realtime -> 50,
# auto -> 0.5; the bulk arms echo 0.005 on every rep).
#
#   usage: copaclean_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/copaclean/battery-s${SEED_ARG}.log
DDIR=/home/vibe/copaclean/diag
mkdir -p "$DDIR" /home/vibe/copaclean
: > "$OUT"
echo "# copaclean battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

PT="RWM_QUIC_CC=passthrough"

run_one() { # name envs cellA cellB mode bytes expect_cc(bbr|copa)
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6" expcc="$7"
  local t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"
  # CC liveness (discipline 1): the arm's controller must be the one running.
  local bbr pt feed sr pbs mp
  bbr=$(grep -c "congestion controller: BBR" /tmp/rwm-c.log 2>/dev/null || true)
  pt=$(grep -c "quinn congestion window is engine-owned" /tmp/rwm-c.log 2>/dev/null || true)
  feed=$(grep -c "Copa delivery feed ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  mp=$(grep -c "multipath recovery suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS cc_expect=$expcc bbr=$bbr pt=$pt feed=$feed sr=$sr pbs=$pbs mp=$mp" >> "$OUT"
  # delta(hint) + wire-clock + compete echoes (arm B only emits them).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log \
    | grep -oE "copa_wire=[a-z]+|delta=[0-9.]+|cc_pace=[a-z]+|compete=[a-z]+" \
    | sort -u | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  if [ "$expcc" = "copa" ]; then
    if [ "$pt" -eq 0 ] || [ "$feed" -eq 0 ]; then echo "ARM-LIVENESS-FAIL $name rep=$REP (pt=$pt feed=$feed)" >> "$OUT"; fi
    if [ "$bbr" -gt 0 ]; then echo "ARM-CONTAMINATION $name rep=$REP (bbr echo in copa arm)" >> "$OUT"; fi
  else
    if [ "$bbr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL $name rep=$REP (no BBR echo)" >> "$OUT"; fi
    if [ "$pt" -gt 0 ] || [ "$feed" -gt 0 ]; then echo "ARM-CONTAMINATION $name rep=$REP (copa echo in bbr arm)" >> "$OUT"; fi
  fi
  # est=Y + per-path clock sample (last DIAG block) for the quick view; the
  # full rtt/wrtt/rtp distributions come from the copied logs.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|srel=[0-9]+/[0-9]+|retx=[0-9]+|cwnd=[0-9]+|est=[YN]|rtt=[0-9]+/wrtt=[0-9]+/rtp[0-9]+ms" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^src=/){gsub(/src=|sym\/s/,"",$i);s+=$i;n++}; if($i~/^cod=/){gsub(/cod=|sym\/s/,"",$i);c+=$i}}} END{if(n>0) printf "RATES mean_src=%.0f mean_cod=%.0f cod_share=%.3f\n", s/n, c/n, (s>0)?c/s:0; else print "RATES no-diag"}' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  run_one sc2-a ""    c2 c2 single 100000000 bbr
  run_one sc2-b "$PT" c2 c2 single 100000000 copa
  run_one sc3-a ""    c3 c3 single 25000000  bbr
  run_one sc3-b "$PT" c3 c3 single 25000000  copa
  run_one c7-a  ""    c2 c2 dual   200000000 bbr
  run_one c7-b  "$PT" c2 c2 dual   200000000 copa
  run_one c8-a  ""    c2 c3 dual   25000000  bbr
  run_one c8-b  "$PT" c2 c3 dual   25000000  copa
  run_one dc1-a ""    c1 c1 dual   400000000 bbr
  run_one dc1-b "$PT" c1 c1 dual   400000000 copa
done

# delta(hint) liveness one-offs (arm B; the flip's continuous knob):
# realtime -> delta=50, auto -> delta=0.5 (bulk 0.005 echoed on every B rep).
echo "=== DELTACHECK realtime $(date -u +%T)" >> "$OUT"
sudo env SEED=$SEED_ARG RWM_GEN=0 $PT RWM_DIAG=1 bash perf_rwm_c.sh c2 c2 realtime 200000 1 single >/dev/null 2>&1 || true
(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -oE "copa_wire=[a-z]+|hint=[A-Za-z]+|delta=[0-9.]+|compete=[a-z]+" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"
cp /tmp/rwm-c.log "$DDIR/deltacheck-rt-s${SEED_ARG}-c.log" 2>/dev/null || true
echo "=== DELTACHECK auto $(date -u +%T)" >> "$OUT"
sudo env SEED=$SEED_ARG RWM_GEN=0 $PT RWM_DIAG=1 bash perf_rwm_c.sh c2 c2 auto 200000 1 single >/dev/null 2>&1 || true
(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -oE "copa_wire=[a-z]+|hint=[A-Za-z]+|delta=[0-9.]+|compete=[a-z]+" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"
cp /tmp/rwm-c.log "$DDIR/deltacheck-auto-s${SEED_ARG}-c.log" 2>/dev/null || true

# Arm-liveness assertion (discipline 7): an arm with zero summaries fails
# LOUDLY, it does not vanish.
echo "--- ARMCOUNTS (expect $REPS summaries per arm)" >> "$OUT"
for a in sc2-a sc2-b sc3-a sc3-b c7-a c7-b c8-a c8-b dc1-a dc1-b; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
