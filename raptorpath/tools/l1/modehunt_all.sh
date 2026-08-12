#!/bin/bash
# THE MODE-HUNT BATTERY — both seeds, one detached session, one completion
# sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash modehunt_all.sh >/home/vibe/modehunt/all.out 2>&1 &
#
# Writes /home/vibe/modehunt/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/modehunt/modehunt-s{42,7}.log with the
# per-run client/server/qdisc/ping captures under diag/.
#
# WATCHER NOTE (carried from deadwall_all.sh / flip_all.sh / hi_all.sh, and it
# has bitten this project before): `pgrep -f modehunt_battery.sh` matches the
# WATCHER'S OWN shell whenever its command line contains the string. Watch the
# SENTINEL, or the ledger's MODEHUNT-BATTERY-DONE line — never the process
# table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/modehunt
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "MODEHUNT-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash modehunt_battery.sh 42 "${1:-12}"
bash modehunt_battery.sh 7 "${1:-12}"
echo "MODEHUNT-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
