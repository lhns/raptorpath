#!/bin/bash
# goal-gate "Unified Shedding + Flip Battery" (roadmap item 3) master driver.
# Runs the pre-registered L1 flip-gate battery on the CURRENT shipped defaults
# (BBR + SACK-release + STORE_PATHS + RECOV_MP + MSTAR/CLOCK_GAP all ON):
#   A: 3-arm realtime tail matrix (stream / unified+shed / legacy-rlc),
#      c2 + c3, 8 reps/arm, both seeds — THE GATE (p50/p99 + delivered n)
#   B: realtime delivered-reliability, perf c3 realtime single — arms
#      S / U / U0 (U0 = RWM_UNIFIED_SHED=0: the high-rho serializing arm;
#      the perf cell is --window-reliable (rho=1), so U == U0 is ALSO the
#      never-shed-the-reliable-contract check at L1)
#   C: bulk gen-sys parity, sc2 single + c7 dual, LS / US, 4 reps
#   D: depth knee c2r100 + c2r200, L1 (legacy + M* law) / U1 (unified),
#      4 reps — anchors (MSTAR default ON) live in both arms
# Usage: shed_all.sh   (runs everything, both seeds, priority order A,B,C,D)
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUT=/home/vibe/shed
BIN=/home/vibe/raptorpath/target/release/raptorpath
mkdir -p "$OUT"

hdr() { # logfile
  echo "# binary: $(sha256sum $BIN)" >> "$1"
  echo "# commit: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$1"
  echo "# lscpu: $(lscpu | grep 'Model name' | sed 's/ \+/ /g'); flags: $(lscpu | grep -o 'aes\|avx2\|pclmulqdq' | sort -u | tr '\n' ' ')" >> "$1"
}

echo "=== shed_all start $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"

# --- Battery A: 3-arm realtime tail matrix (the flip gate) ---
for SEEDV in 42 7; do
  for CELL in c2 c3; do
    LOG="$OUT/tail-$CELL-s$SEEDV.log"
    : > "$LOG"; hdr "$LOG"
    echo "# cmd: sudo RWM_TM_ARMS='stream unified rlc' SEED=$SEEDV bash tail_matrix.sh $CELL 8" >> "$LOG"
    sudo RWM_TM_ARMS='stream unified rlc' SEED=$SEEDV bash tail_matrix.sh "$CELL" 8 >> "$LOG" 2>&1
    echo "--- tail $CELL s$SEEDV done $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
  done
done
touch "$OUT/A.done"

# --- Battery B: realtime delivered reliability c3 (the rstar/r* cell) ---
rt_arm() { # arm envs
  local arm="$1"; shift
  local envs="$*"
  local LOG="$OUT/c3rt-s$SEEDV.log"
  echo "=== rep=$REP arm=$arm seed=$SEEDV env=\"$envs RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5\" cmd=\"perf_rwm_c.sh c3 c3 realtime 100000 20 single\" $(date +%T)" >> "$LOG"
  sudo env SEED=$SEEDV RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5 $envs \
        bash perf_rwm_c.sh c3 c3 realtime 100000 20 single 2>&1 \
        | grep -E '"seconds"|"dnf"|"summary"|FATAL' >> "$LOG"
  for lg in /tmp/rwm-s.log /tmp/rwm-c.log; do
    sudo sed 's/\x1b\[[0-9;]*m//g' "$lg" 2>/dev/null \
      | grep -oE '(RWM_UNIFIED[^"]*|Realtime mode: auto-selecting streaming backend|unified overload shedding ACTIVE[^"]*|A\* send-rate anchor ACTIVE[^"]*)' \
      | sort -u | sed "s|^|ECHO ${lg##*/}: |" >> "$LOG"
  done
  local dlog=$OUT/diag-c3rt-s$SEEDV-r$REP-$arm.log
  sudo sh -c "grep -E '^\[DIAG\]' /tmp/rwm-c.log > $dlog" 2>/dev/null
  awk '{ for(i=1;i<=NF;i++){ if($i ~ /^src=/){gsub(/src=|sym\/s/,"",$i); s+=$i}
         if($i ~ /^cod=/){gsub(/cod=|sym\/s/,"",$i); c+=$i} } n++ }
       END { if(n>0 && s>0) printf "LIVE: diag_lines=%d mean_src=%.0f mean_cod=%.0f cod_over_src=%.4f\n", n, s/n, c/n, c/s }' \
       "$dlog" >> "$LOG"
}
for SEEDV in 42 7; do
  LOG="$OUT/c3rt-s$SEEDV.log"; : > "$LOG"; hdr "$LOG"
  for REP in $(seq 1 8); do
    rt_arm S ""
    rt_arm U "RWM_UNIFIED=1"
    rt_arm U0 "RWM_UNIFIED=1 RWM_UNIFIED_SHED=0"
  done
  echo "battery c3rt seed $SEEDV done $(date +%T)" >> "$LOG"
  echo "--- c3rt s$SEEDV done $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
done
touch "$OUT/B.done"

# --- Battery C: bulk gen-sys parity sc2 + c7 ---
BASEC="RWM_GEN_R=0.03 RWM_GEN_PIPE=1 RWM_QUIC_CC=bbr RWM_DIAG=1"
bulk_arm() { # cell mode arm extraenv
  local cell="$1" mode="$2" arm="$3" extraenv="$4"
  local LOG="$OUT/bulk-$cell-s$SEEDV.log"
  echo "=== rep=$REP arm=$arm seed=$SEEDV env=\"$BASEC $extraenv RWM_EXTRA=--window-systematic-repair\" cmd=\"perf_rwm_c.sh c2 c2 bulk 25000000 1 $mode\" $(date +%T)" >> "$LOG"
  sudo env SEED=$SEEDV $BASEC $extraenv RWM_EXTRA="--window-systematic-repair" \
        bash perf_rwm_c.sh c2 c2 bulk 25000000 1 "$mode" 2>&1 \
        | grep -E '"seconds"|"dnf"|"summary"|GUARD|CPU:|FATAL' >> "$LOG"
  sudo sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null \
      | grep -oE 'RWM_UNIFIED: receive path on the unified global decoder[^"]*' \
      | sort -u | sed 's|^|ECHO rwm-s.log: |' >> "$LOG"
}
for SEEDV in 42 7; do
  for CELL in sc2 c7; do
    LOG="$OUT/bulk-$CELL-s$SEEDV.log"; : > "$LOG"; hdr "$LOG"
    MODE=single; [ "$CELL" = c7 ] && MODE=dual
    for REP in $(seq 1 4); do
      bulk_arm "$CELL" "$MODE" LS ""
      bulk_arm "$CELL" "$MODE" US "RWM_UNIFIED=1"
    done
    echo "battery bulk-$CELL seed $SEEDV done $(date +%T)" >> "$LOG"
  done
  echo "--- bulk s$SEEDV done $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
done
touch "$OUT/C.done"

# --- Battery D: depth knee c2r100 + c2r200 (anchors default ON both arms) ---
BASED="RWM_GEN_R=0.03 RWM_QUIC_CC=bbr RWM_DIAG=1"
knee_arm() { # cell arm extraenv
  local cell="$1" arm="$2" extraenv="$3"
  local LOG="$OUT/knee-$cell-s$SEEDV.log"
  echo "=== rep=$REP arm=$arm seed=$SEEDV env=\"$BASED $extraenv RWM_EXTRA=--window-systematic-repair\" cmd=\"perf_rwm_c.sh $cell $cell bulk 25000000 1 single\" $(date +%T)" >> "$LOG"
  sudo env SEED=$SEEDV $BASED $extraenv RWM_EXTRA="--window-systematic-repair" \
        bash perf_rwm_c.sh "$cell" "$cell" bulk 25000000 1 single 2>&1 \
        | grep -E '"seconds"|"dnf"|"summary"|GUARD|CPU:|FATAL' >> "$LOG"
  sudo sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null \
      | grep -oE 'RWM_UNIFIED: receive path on the unified global decoder[^"]*' \
      | sort -u | sed 's|^|ECHO rwm-s.log: |' >> "$LOG"
  sudo sh -c "grep -E '^\[GPIPE\]' /tmp/rwm-c.log 2>/dev/null | head -8" \
      | sed 's|^|GPIPE: |' >> "$LOG"
}
for SEEDV in 42 7; do
  for CELL in c2r100 c2r200; do
    LOG="$OUT/knee-$CELL-s$SEEDV.log"; : > "$LOG"; hdr "$LOG"
    for REP in $(seq 1 4); do
      knee_arm "$CELL" L1 "RWM_GEN_PIPE=1"
      knee_arm "$CELL" U1 "RWM_UNIFIED=1 RWM_GEN_PIPE=1"
    done
    echo "battery knee-$CELL seed $SEEDV done $(date +%T)" >> "$LOG"
  done
  echo "--- knee s$SEEDV done $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
done
touch "$OUT/D.done"
touch "$OUT/ALLSHED.done"
echo "=== shed_all DONE $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
