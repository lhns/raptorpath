#!/bin/bash
# Store-cap unification ATTRIBUTION + FLIP battery — both seeds, one
# detached session, one completion sentinel (discipline 13: launch
# detached, never poll).
#
#   sudo nohup bash uni_all.sh >/home/vibe/uniflip/all.out 2>&1 &
#
# Writes /home/vibe/uniflip/DONE-ALL when both seed batteries have
# finished; per-seed ledgers land at /home/vibe/uniflip/uni-s{42,7}.log.
# WATCHER NOTE (from flip_all.sh, kept): `pgrep -f uni_battery.sh`
# matches the WATCHER'S OWN shell when its command line contains the
# string — check the ledger's UNI-BATTERY-DONE line, not the process
# table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/uniflip
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "UNI-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash uni_battery.sh 42 "${1:-8}" "${2:-12}"
bash uni_battery.sh 7 "${1:-8}" "${2:-12}"
echo "UNI-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
