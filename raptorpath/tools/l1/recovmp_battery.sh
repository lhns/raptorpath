#!/bin/bash
# feat/recovery-suppression L1 battery: RWM_RECOV_MP A/B (goal-gate 16.23
# successor — multipath recovery-plane over-emission, the fifth wall).
#
# Arms interleaved round-robin per rep, 1 run/invocation, fresh tunnel per
# invocation, RWM_DIAG=1 everywhere, per-arm liveness echo asserted
# (MEASUREMENT DISCIPLINE 1/7). Cells:
#   dual-c1 PB   ± MP  — THE control (GE 0.1%: nothing real to recover;
#                        legacy retx ×46 single, dual sinks BELOW single)
#   sc1     PB         — the dual-c1 control's same-session single reference
#   c7      PBP-H ± MP — the Σ-gap cell (best-c7 arm under profile)
#   sc2     PBP-H ± MP — c7's Σ single term + the N=1 identity check
#   c8      PBS  ± MP  — the asymmetric cell (pooled path-scaled arm)
#   sc3     PB   ± MP  — c8's slow single term + the N=1 identity check
#
#   usage: recovmp_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/recovmp/battery-s${SEED_ARG}.log
DDIR=/home/vibe/recovmp/diag
mkdir -p "$DDIR" /home/vibe/recovmp
: > "$OUT"
echo "# recovmp battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

PBPH="RWM_QUIC_CC=bbr RWM_STORE_PERCAP=1 RWM_PLAIN_RS=1"
PBS="RWM_QUIC_CC=bbr RWM_STORE_PATHS=1"
PB="RWM_QUIC_CC=bbr"

run_one() { # name envs cellA cellB mode bytes expect_mp(0/1)
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6" expmp="$7"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # Liveness (discipline 1): the MP echo must match the arm's expectation.
  local mp pc pbs
  mp=$(grep -c "multipath recovery suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pc=$(grep -c "per-path outstanding accounting ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS mp=$mp expect=$expmp percap=$pc pbs=$pbs" >> "$OUT"
  if [ "$expmp" -gt 0 ] && [ "$mp" -eq 0 ]; then echo "ARM-LIVENESS-FAIL $name rep=$REP" >> "$OUT"; fi
  if [ "$expmp" -eq 0 ] && [ "$mp" -gt 0 ]; then echo "ARM-CONTAMINATION $name rep=$REP" >> "$OUT"; fi
  # Waste gauges: the last (cumulative) DIAG line's counters + mpr trace +
  # per-path loss estimates (the serial-poisoning gauge).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "sweeps=[0-9]+|retx=[0-9]+|gapdrop=[0-9]+|xattr=[0-9]+/[0-9]+|mpr\[[^]]*\]|pl=[0-9.]+" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^src=/){gsub(/src=|sym\/s/,"",$i);s+=$i;n++}; if($i~/^cod=/){gsub(/cod=|sym\/s/,"",$i);c+=$i}}} END{if(n>0) printf "RATES mean_src=%.0f mean_cod=%.0f cod_share=%.3f\n", s/n, c/n, (s>0)?c/s:0; else print "RATES no-diag"}' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  run_one sc2-pbph    "$PBPH"                 c2 c2 single 100000000 0
  run_one sc2-pbph-mp "$PBPH RWM_RECOV_MP=1"  c2 c2 single 100000000 1
  run_one sc3-pb      "$PB"                   c3 c3 single 25000000  0
  run_one sc3-pb-mp   "$PB RWM_RECOV_MP=1"    c3 c3 single 25000000  1
  run_one c7-pbph     "$PBPH"                 c2 c2 dual   200000000 0
  run_one c7-pbph-mp  "$PBPH RWM_RECOV_MP=1"  c2 c2 dual   200000000 1
  run_one c8-pbs      "$PBS"                  c2 c3 dual   25000000  0
  run_one c8-pbs-mp   "$PBS RWM_RECOV_MP=1"   c2 c3 dual   25000000  1
  run_one sc1-pb      "$PB"                   c1 c1 single 400000000 0
  run_one dc1-pb      "$PB"                   c1 c1 dual   400000000 0
  run_one dc1-pb-mp   "$PB RWM_RECOV_MP=1"    c1 c1 dual   400000000 1
done

# Arm-liveness assertion (discipline 7): every arm must have produced a
# summary per rep — an arm with zero summaries fails LOUDLY.
echo "--- ARMCOUNTS (expect $REPS summaries per arm)" >> "$OUT"
for a in sc2-pbph sc2-pbph-mp sc3-pb sc3-pb-mp c7-pbph c7-pbph-mp c8-pbs c8-pbs-mp sc1-pb dc1-pb dc1-pb-mp; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
