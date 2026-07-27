#!/bin/bash
# feat/c8-pool-law: full session driver — diagnosis (seed 42) then the
# battery on both seeds (42+7, ×8 interleaved reps, fmtcp ×4).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
LOG=/home/vibe/c8pool/all.log
mkdir -p /home/vibe/c8pool
{
  echo "# c8pool ALL start $(date -u +%FT%TZ)"
  bash c8pool_battery.sh 42 8 4
  bash c8pool_battery.sh 7 8 4
  echo "# c8pool ALL done $(date -u +%FT%TZ)"
} 2>&1 | tee -a "$LOG"
