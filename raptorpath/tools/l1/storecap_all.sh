#!/bin/bash
# goal-gate "Store-Cap Triplication": the WHOLE pre-registered battery, ONE
# detached invocation.
#
# MEASUREMENT DISCIPLINE item 13: this exists so the session launches ONCE,
# waits ONCE and collects ONCE. DO NOT POLL IT. (Measured 2026-08-07: polling
# turned 80/80 clean invocations into 26/171 with 121 RUN-RETRY, on seed 42,
# where the seed-7 abort class does not apply.)
#
# Stages:
#   1. battery seed 42, full scope, x8   (c7/c8 duals + sc2/sc3 + c1)
#   2. battery seed  7, full scope, x8
#   3. crown seed 42: tail_matrix c2 x4, arms "def uni"
#   4. crown seed  7: tail_matrix c2 x4, arms "def uni"
# then touches /home/vibe/storecap/DONE.
#
#   usage: nohup setsid bash storecap_all.sh > /home/vibe/storecap/driver.log 2>&1 &
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/storecap
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE"
BIN=/home/vibe/raptorpath/target/release/raptorpath

exec_stage() {
  local label="$1"; shift
  local t0 t1
  t0=$(date +%s)
  echo "### STAGE $label START $(date -u +%FT%TZ)"
  "$@"
  t1=$(date +%s)
  echo "### STAGE $label END $(date -u +%FT%TZ) runtime=$(( (t1-t0)/60 ))m$(( (t1-t0)%60 ))s"
}

{
  echo "# storecap ALL $(date -u +%FT%TZ)"
  echo "# binary: $(sha256sum $BIN)"
  echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)"
  lscpu | grep "Model name"
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ') || true
  echo
  uname -r
} > "$OUTDIR/env.log" 2>&1

exec_stage battery-s42 bash storecap_battery.sh 42 8 all
exec_stage battery-s7  bash storecap_battery.sh 7  8 all

# CROWN (no-regression gate): realtime tail matrix at c2, both arms.
for S in 42 7; do
  exec_stage crown-s$S env SEED=$S RWM_TM_ARMS="default uni" \
    sudo -E bash tail_matrix.sh c2 4
done

sudo bash cleanup.sh >/dev/null 2>&1 || true
echo "### ALL DONE $(date -u +%FT%TZ)"
touch "$OUTDIR/DONE"
