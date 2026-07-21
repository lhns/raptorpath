#!/bin/bash
# feat/consolidation orchestrator: the full composed-stack battery in the
# pre-registered priority order (LOO c7/c8/dc1 + singles first, then the
# tail-crown gate, then cross-traffic), both seeds interleaved at the
# battery level (s42 pass then s7 pass per stage; arms interleave per rep
# INSIDE each battery).
#   usage: consol_all.sh
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUT=/home/vibe/consol
mkdir -p "$OUT"

echo "=== consol_all start $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"

# Stage 1: the LOO battery (the strictly-better proof), seeds 42 + 7.
bash consol_battery.sh 42 8 2>&1 | tee -a "$OUT/all.log"
bash consol_battery.sh 7  8 2>&1 | tee -a "$OUT/all.log"

# Stage 2: the tail-crown regression gate — shipped streaming Realtime
# with and without the stack env, c2 (the 12-48x crown cell), 5 reps/arm.
for S in 42 7; do
  echo "--- tail crown seed=$S $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
  sudo env SEED=$S RWM_TM_ARMS="stream stack" bash tail_matrix.sh c2 5 \
    2>&1 | tee "$OUT/tail-s${S}.log" | tee -a "$OUT/all.log"
done

# Stage 3: cross-traffic c2, stack vs 1 Cubic flow (share documented, not
# gated) + the shipped-default reference, 5 invocations per arm per seed.
for S in 42 7; do
  for i in 1 2 3 4 5; do
    echo "--- xt stack seed=$S i=$i $(date -u +%FT%TZ)" >> "$OUT/xt-s${S}.log"
    sudo env SEED=$S RWM_STORE_PATHS=1 RWM_RECOV_MP=1 RWM_MSTAR_ANCHOR=1 RWM_CLOCK_GAP=1 \
      bash cross_traffic.sh c2 bbr 25000000 2>&1 | tee -a "$OUT/xt-s${S}.log" >/dev/null
    echo "--- xt ship seed=$S i=$i $(date -u +%FT%TZ)" >> "$OUT/xt-s${S}.log"
    sudo env SEED=$S bash cross_traffic.sh c2 bbr 25000000 2>&1 | tee -a "$OUT/xt-s${S}.log" >/dev/null
  done
done

echo "=== consol_all DONE $(date -u +%FT%TZ)" | tee -a "$OUT/all.log"
echo CONSOL-ALL-DONE
