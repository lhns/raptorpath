#!/bin/bash
# THE COMPOSED-CAP BATTERY — both seeds, one detached session, one completion
# sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash ccap_all.sh >/home/vibe/ccap/all.out 2>&1 &
#
# Writes /home/vibe/ccap/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/ccap/ccap-s{42,7}.log with the per-run
# client/server/qdisc/ping captures under diag/.
#
# WATCHER NOTE (carried from modehunt_all.sh / deadwall_all.sh / flip_all.sh /
# hi_all.sh, and it has bitten this project before): `pgrep -f ccap_battery.sh`
# matches the WATCHER'S OWN shell whenever its command line contains the string.
# Watch the SENTINEL, or the ledger's CCAP-BATTERY-DONE line — never the process
# table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the seed-7 abort signature on seed 42 (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones). Launch
# this, then WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ccap
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
echo "CCAP-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash ccap_battery.sh 42 "${1:-12}"
bash ccap_battery.sh 7 "${1:-12}"
echo "CCAP-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
