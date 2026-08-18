#!/bin/bash
# THE LADDER BATTERY — SYMMETRIC TOP-UP.
#
#   sudo nohup bash ladder_topup.sh <seed> <cells> [reps] \
#        >/home/vibe/ladder/topup.out 2>&1 &
#
# WHY THIS EXISTS. The pre-registration (goal-gate "Ladder Battery —
# PRE-REGISTRATION", commit 91c00dd, guard G-TOPUP) fixes the convention:
#
#   "If aborts drive any scored arm below n = 8 at either seed, the top-up runs
#    the SAME rep count for EVERY ARM of that cell at that seed — same session,
#    same binary, own ledger, own sentinel. NEVER asymmetric: topping up one arm
#    and not the others makes that cell's contrast a cross-pool one, which is
#    the comparison two predecessors were lost to."
#
# WHAT IS SYMMETRIC HERE. All five arms, always. This battery has no
# pre-disqualified arm and no cell-restricted arm, so `RWM_LADDER_ARMS` is left
# at its default and the symmetry needs no special case. c8L's N-rung exclusion
# is a SCORING rule in the contract, not a missing invocation — the N arm still
# runs there, and its reps still top up with the rest.
#
# THE BINARY IS NOT REBUILT. The top-up is only poolable with the main session
# if it is the same bytes; the caller asserts the sha256 before launching and the
# ledger header re-records it.
#
# B-WALL READS THIS SESSION. The top-up is not only a repair — it is the SECOND
# POOL the paired dead-wall sign test is scored across ("the paired sign is
# consistent across BOTH seeds AND between the main pool and every top-up pool").
# `ladder_report.py` takes pool provenance from the FILENAME (`topup` in the
# basename), so this ledger must keep its tag.
#
# WATCHER NOTE: `pgrep -f ladder_topup.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table.
set -u
SEED="${1:?seed}"; CELLS="${2:?cells, space-separated, quoted}"; REPS="${3:-8}"
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ladder
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-TOPUP"
{
  echo "LADDER-TOPUP start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "LADDER-TOPUP seed=$SEED cells='$CELLS' reps=$REPS — SYMMETRIC over ALL FIVE ARMS"
} > "$OUTDIR/topup-era.txt"
RWM_LADDER_TAG=ladder-topup RWM_LADDER_CELLS="$CELLS" RWM_LADDER_SMALLREPS="$REPS" \
  bash ladder_battery.sh "$SEED" "$REPS"
echo "LADDER-TOPUP end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/topup-era.txt"
touch "$OUTDIR/DONE-TOPUP"
