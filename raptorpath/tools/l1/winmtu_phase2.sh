#!/bin/bash
# feat/window-mtu phase 2 (post-scope-fix): fresh build (stale rm'd) then
# the falsification-5 dual re-run (redual, both seeds), the B1 jitter
# cross-check (both seeds), and the crown tail spot (seed 42).
# Run AFTER the s7 battery has finished (no compile during measurement).
set -u
cd /home/vibe/raptorpath
OUT=/home/vibe/winmtu/phase2.log
mkdir -p /home/vibe/winmtu
: > "$OUT"
echo "# winmtu PHASE2 $(date -u +%FT%TZ)" >> "$OUT"
rm -f target/release/raptorpath
cargo build --release -p raptorpath >> "$OUT" 2>&1
echo "# binary: $(sha256sum target/release/raptorpath)" >> "$OUT"
echo "# source: $(cat COMMIT)" >> "$OUT"

cd /home/vibe/raptorpath/raptorpath/tools/l1
echo "== redual s42 $(date -u +%T)" >> "$OUT"
sudo bash winmtu_battery.sh 42 8 redual >> "$OUT" 2>&1
echo "== redual s7 $(date -u +%T)" >> "$OUT"
sudo bash winmtu_battery.sh 7 8 redual >> "$OUT" 2>&1
echo "== jit s42 $(date -u +%T)" >> "$OUT"
sudo bash winmtu_jit.sh 42 5 >> "$OUT" 2>&1
echo "== jit s7 $(date -u +%T)" >> "$OUT"
sudo bash winmtu_jit.sh 7 5 >> "$OUT" 2>&1
echo "== tails s42 $(date -u +%T)" >> "$OUT"
sudo env SEED=42 RWM_TM_ARMS='default mtu wdfix wdmtu' \
  bash tail_matrix.sh c2 4 > /home/vibe/winmtu/tails-s42.log 2>&1
echo "PHASE2-DONE $(date -u +%FT%TZ)" >> "$OUT"
echo PHASE2-DONE
