#!/bin/bash
# THE CANDIDATES BATTERY — SYMMETRIC TOP-UP.
#
#   sudo nohup bash ccand_topup.sh <seed> <cells> [reps] \
#        >/home/vibe/ccand/topup.out 2>&1 &
#
# WHY THIS EXISTS. The pre-registration (goal-gate "Candidates Battery —
# PRE-REGISTRATION", commit 6bd5299, guard G-TOPUP) fixes the convention:
#
#   "If aborts drive any SCORED arm below n = 8 at either seed, the top-up runs
#    the SAME rep count for EVERY SCORED ARM of that cell at that seed — same
#    session, same binary, own ledger, own sentinel. NEVER asymmetric: topping up
#    one arm and not the others makes that cell's contrast a cross-pool one,
#    which is the comparison two predecessors were lost to."
#
# WHAT IS SYMMETRIC HERE, AND WHAT IS DELIBERATELY NOT. `RWM_CCAND_ARMS` is
# pinned to the FOUR SCORED ARMS — A D R DR — and the two AUXILIARY arms are
# deliberately excluded. That is not an asymmetry in the sense G-TOPUP forbids:
# R1 and L are scored on their OWN echo line and on nothing else, they enter no
# contrast and no guard denominator, so there is no cross-pool comparison for a
# missing top-up to corrupt. Topping them up would repair nothing and would spend
# reps on arms that carry no bar.
#
# THE BINARY IS NOT REBUILT. The top-up is only poolable with the main session if
# it is the same bytes; the caller asserts the sha256 before launching and the
# ledger header re-records it.
#
# B-WALL READS THIS SESSION. The top-up is not only a repair — it is the SECOND
# POOL the paired dead-wall sign test is scored across ("the paired sign is
# consistent across BOTH seeds AND between the main pool and every top-up pool").
# `ccand_report.py` takes pool provenance from the FILENAME (`topup` in the
# basename), so this ledger must keep its tag.
#
# WATCHER NOTE: `pgrep -f ccand_topup.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table.
set -u
SEED="${1:?seed}"; CELLS="${2:?cells, space-separated, quoted}"; REPS="${3:-8}"
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ccand
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-TOPUP"
{
  echo "CCAND-TOPUP start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "CCAND-TOPUP seed=$SEED cells='$CELLS' reps=$REPS — SYMMETRIC over ALL FOUR SCORED ARMS (A D R DR)"
  echo "CCAND-TOPUP the AUX arms R1 and L are excluded: they enter no contrast and no guard denominator"
} > "$OUTDIR/topup-era.txt"
RWM_CCAND_TAG=ccand-topup RWM_CCAND_CELLS="$CELLS" RWM_CCAND_ARMS="A D R DR" \
  RWM_CCAND_SMALLREPS="$REPS" bash ccand_battery.sh "$SEED" "$REPS"
echo "CCAND-TOPUP end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/topup-era.txt"
touch "$OUTDIR/DONE-TOPUP"
