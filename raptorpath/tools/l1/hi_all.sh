#!/bin/bash
# GOAL "HONEST INPUTS" phase 2 — both seeds, one detached session, one
# completion sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash hi_all.sh >/home/vibe/honestinputs/all.out 2>&1 &
#
# Writes /home/vibe/honestinputs/DONE-ALL when both seed batteries have
# finished; per-seed ledgers land at /home/vibe/honestinputs/hi-s{42,7}.log.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/honestinputs
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "HI-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash hi_battery.sh 42 "${1:-8}"
bash hi_battery.sh 7 "${1:-8}"
echo "HI-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
