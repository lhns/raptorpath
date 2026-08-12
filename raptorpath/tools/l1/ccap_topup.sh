#!/bin/bash
# THE COMPOSED-CAP BATTERY — SYMMETRIC TOP-UP.
#
#   sudo nohup bash ccap_topup.sh <seed> <cells> [reps] \
#        >/home/vibe/ccap/topup.out 2>&1 &
#
# WHY THIS EXISTS. The pre-registration (goal-gate "Composed-Cap Battery — VM
# PRE-REGISTRATION", commit 1e09c00, guard G-TOPUP) fixes the convention:
#
#   "If aborts drive any scored arm below n = 8 at either seed, the top-up runs
#    the SAME rep count for EVERY arm of that cell at that seed, same session,
#    same binary, own ledger, own sentinel. Never asymmetric."
#
# WHAT IS SYMMETRIC HERE. Both arms, always. This battery has no
# pre-disqualified pin arm and no cell-restricted arm, so `RWM_CC_ARMS` is left
# at its default "A C" and the symmetry needs no special case — topping up one
# arm of a cell and not the other would make the cell's contrast a
# cross-pool one, which is exactly the comparison the predecessor lost.
#
# THE BINARY IS NOT REBUILT. The top-up is only poolable with the main session
# if it is the same bytes; the caller asserts the sha256 before launching and
# the ledger header re-records it.
#
# S-WALL READS THIS SESSION. The top-up is not only a repair — it is the second
# pool the S-WALL stability claim is scored across. `ccap_report.py` takes pool
# provenance from the FILENAME (`topup` in the basename), so this ledger must
# keep its tag.
#
# WATCHER NOTE: `pgrep -f ccap_topup.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table.
set -u
SEED="${1:?seed}"; CELLS="${2:?cells, space-separated, quoted}"; REPS="${3:-8}"
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ccap
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-TOPUP"
{
  echo "CCAP-TOPUP start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "CCAP-TOPUP seed=$SEED cells='$CELLS' reps=$REPS — SYMMETRIC over BOTH arms"
} > "$OUTDIR/topup-era.txt"
RWM_CC_TAG=ccap-topup RWM_CC_CELLS="$CELLS" RWM_CC_SMALLREPS="$REPS" \
  bash ccap_battery.sh "$SEED" "$REPS"
echo "CCAP-TOPUP end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/topup-era.txt"
touch "$OUTDIR/DONE-TOPUP"
