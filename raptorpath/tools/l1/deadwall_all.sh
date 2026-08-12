#!/bin/bash
# THE DEAD-WALL BATTERY — both seeds, one detached session, one completion
# sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash deadwall_all.sh >/home/vibe/deadwall/all.out 2>&1 &
#
# Writes /home/vibe/deadwall/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/deadwall/deadwall-s{42,7}.log with the
# per-run client/server/qdisc/ping captures under diag/.
#
# WATCHER NOTE (carried from flip_all.sh / hi_all.sh, and it has bitten this
# project before): `pgrep -f deadwall_battery.sh` matches the WATCHER'S OWN
# shell whenever its command line contains the string. Watch the SENTINEL, or
# the ledger's DEADWALL-BATTERY-DONE line — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/deadwall
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "DEADWALL-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash deadwall_battery.sh 42 "${1:-8}"
bash deadwall_battery.sh 7 "${1:-8}"
echo "DEADWALL-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
