#!/bin/bash
# GOAL "THREE TERMS, NO CONSTANTS" phase 1.4 — the WHOLE battery, one launch.
#
# MEASUREMENT DISCIPLINE 13: the lock holder's own monitoring is co-tenancy.
# This script exists so the battery can be launched ONCE, detached, and left
# strictly alone until the sentinel file appears. Nothing polls it.
#
# Order (seeds INTERLEAVED at the battery level, arms interleaved per rep
# inside each battery — discipline 3/4):
#   1. tt_battery.sh 42   topo.sh cells, arms A/B/C x8 reps
#   2. tt_adv.sh     42   jit25 + shal8, arms A/B/C x8 reps
#   3. tt_battery.sh 7
#   4. tt_adv.sh     7
#   5. crown rows: tail_matrix `ship` vs `tt` at c2, both seeds (criterion 5)
#
#   usage: sudo bash tt_all.sh [reps]
set -u
[ "$(id -u)" -eq 0 ] || { echo "tt_all.sh must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
REPS="${1:-8}"
mkdir -p /home/vibe/threeterm
S=/home/vibe/threeterm/ALL.log
rm -f /home/vibe/threeterm/SENTINEL
: > "$S"
echo "TT-ALL START $(date -u +%FT%TZ) reps=$REPS" >> "$S"

for seed in 42 7; do
  echo "--- tt_battery seed=$seed $(date -u +%FT%TZ)" >> "$S"
  bash ./tt_battery.sh "$seed" "$REPS" >> "$S" 2>&1 || echo "TT-BATTERY-RC=$? seed=$seed" >> "$S"
  echo "--- tt_adv seed=$seed $(date -u +%FT%TZ)" >> "$S"
  bash ./tt_adv.sh "$seed" "$REPS" >> "$S" 2>&1 || echo "TT-ADV-RC=$? seed=$seed" >> "$S"
done

# ── Criterion 5: the realtime crown (<= ~41 ms at 1000/1000) ──────────────
# 50 msg/s x 20 s = 1000 messages per rep; `ship` (env unset) vs `tt` (the
# SCORED composed arm) at c2, 8 reps per arm per size, both seeds.
for seed in 42 7; do
  echo "=== CROWN seed=$seed $(date -u +%FT%TZ)" >> "$S"
  SEED=$seed RWM_TM_ARMS='ship tt' bash ./tail_matrix.sh c2 8 \
    > /home/vibe/threeterm/crown-s${seed}.log 2>&1 || echo "CROWN-FAIL seed=$seed" >> "$S"
  grep -E "^ARM |rep[0-9]+:|BRINGUP|NO_DATA" /home/vibe/threeterm/crown-s${seed}.log >> "$S" 2>/dev/null || true
done

echo "TT-ALL DONE $(date -u +%FT%TZ)" >> "$S"
date -u +%FT%TZ > /home/vibe/threeterm/SENTINEL
