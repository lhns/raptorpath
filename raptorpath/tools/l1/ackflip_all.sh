#!/bin/bash
# feat/ack-merge-flip: the WHOLE pre-registered battery, one detached
# invocation (goal-gate "Ack-Merge Flip").
#
# MEASUREMENT DISCIPLINE item 13: this exists precisely so the session
# launches ONCE, waits ONCE, and collects ONCE. Do not poll it. The
# 2026-08-07 full-scope attempt was destroyed by per-poll ssh+sudo+grep
# co-tenancy (171 invocations -> 26 summaries / 121 RUN-RETRY).
#
# Stages, in order:
#   1. battery seed 42, full scope, x8            (c1/c7/c8/sc2/sc3 + sustained)
#   2. battery seed  7, full scope, x8
#   3. crown  seed 42: tail_matrix c2 x4, arms "ship am"
#   4. crown  seed  7: tail_matrix c2 x4, arms "ship am"
# then touches /home/vibe/ackflip/DONE.
#
#   usage: nohup setsid bash ackflip_all.sh > /home/vibe/ackflip/driver.log 2>&1 &
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ackflip
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE"
BIN=/home/vibe/raptorpath/target/release/raptorpath

exec_stage() { # label command...
  local label="$1"; shift
  local t0 t1
  t0=$(date +%s)
  echo "### STAGE $label START $(date -u +%FT%TZ)"
  "$@"
  t1=$(date +%s)
  echo "### STAGE $label END $(date -u +%FT%TZ) runtime=$(( (t1-t0)/60 ))m$(( (t1-t0)%60 ))s"
}

{
  echo "# ackflip ALL $(date -u +%FT%TZ)"
  echo "# binary: $(sha256sum $BIN)"
  echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)"
  lscpu | grep "Model name"
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ') || true
  echo
  uname -r
} > "$OUTDIR/env.log" 2>&1

exec_stage battery-s42 bash ackflip_battery.sh 42 8 all
exec_stage battery-s7  bash ackflip_battery.sh 7  8 all

# CROWN (no-regression gate): tail_matrix c2 x4, realtime hint, both arms.
# `ship` = env unset = today's default; `am` = RWM_ACK_MERGE=1.
for S in 42 7; do
  exec_stage crown-s$S env SEED=$S RWM_TM_ARMS="ship am" \
    sudo -E bash tail_matrix.sh c2 4
done

# Teardown before the marker so the collector never races a live netns.
sudo bash cleanup.sh >/dev/null 2>&1 || true
echo "### ALL DONE $(date -u +%FT%TZ)"
touch "$OUTDIR/DONE"
