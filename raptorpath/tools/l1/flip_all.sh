#!/bin/bash
# GOAL "HONEST INPUTS" phase 4 — both seeds, one detached session, one
# completion sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash flip_all.sh >/home/vibe/flip/all.out 2>&1 &
#
# Writes /home/vibe/flip/DONE-ALL when both seed batteries have finished;
# per-seed ledgers land at /home/vibe/flip/flip-s{42,7}.log.
# WATCHER NOTE (from hi_all.sh, kept): `pgrep -f flip_battery.sh` matches the
# WATCHER'S OWN shell when its command line contains the string — check the
# ledger's FLIP-BATTERY-DONE line, not the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/flip
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "FLIP-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash flip_battery.sh 42 "${1:-8}" "${2:-12}"
bash flip_battery.sh 7 "${1:-8}" "${2:-12}"
echo "FLIP-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
