#!/bin/bash
# THE CANDIDATES BATTERY — both seeds, one detached session, one completion
# sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash ccand_all.sh >/home/vibe/ccand/all.out 2>&1 &
#
# Writes /home/vibe/ccand/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/ccand/ccand-s{42,7}.log with the per-run
# client/server/qdisc/ping captures under diag/.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL. goal-gate "Candidates Battery —
# PRE-REGISTRATION" leaves its headroom table EMPTY on purpose and fixes the
# protocol instead: one rep per arm per cell, seed 42, tc-measured, SAME session,
# SAME binary, BEFORE the scored run, committed as the contract's COMPLETION in
# its own commit before this script is launched. `ccand_calib.sh` is that pass.
# This script does NOT run it — running it here would put the calibration and the
# scored battery in one uninterruptible session and there would be no moment at
# which the completion could be committed. Launch order:
#
#   1. sudo bash ccand_calib.sh                        (one rep/arm/cell, s42)
#   2. commit the filled headroom table to goal-gate    (the contract's completion)
#   3. sudo nohup bash ccand_all.sh ... &               (this script)
#
# WATCHER NOTE (carried from ladder_all.sh / ccap_all.sh / modehunt_all.sh, and it
# has bitten this project before): `pgrep -f ccand_battery.sh` matches the
# WATCHER'S OWN shell whenever its command line contains the string. Watch the
# SENTINEL, or the ledger's CCAND-BATTERY-DONE line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it manufactures
# the seed-7 abort signature on seed 42 (measured 2026-08-07: 121 RUN-RETRY over
# 171 polled invocations against 0 over 80 unpolled ones). Launch this, then
# WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ccand
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
if [ ! -f "$OUTDIR/ccand-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/ccand-calib-s42.log." >&2
  echo "         The contract's headroom table is filled by ccand_calib.sh and" >&2
  echo "         committed BEFORE the scored run (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_CCAND_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_CCAND_NO_CALIB:-0}" = "1" ] || exit 4
fi
echo "CCAND-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash ccand_battery.sh 42 "${1:-12}"
bash ccand_battery.sh 7 "${1:-12}"
echo "CCAND-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
