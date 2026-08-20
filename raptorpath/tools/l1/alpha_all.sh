#!/bin/bash
# THE α-SWEEP — both seeds, one detached session, one completion sentinel.
#
#   sudo nohup bash alpha_all.sh >/home/vibe/alpha/all.out 2>&1 &
#
# Writes /home/vibe/alpha/DONE-ALL when BOTH seed batteries have finished.
# Per-seed ledgers land at /home/vibe/alpha/alpha-s{42,7}.log with the per-run
# client/server/qdisc/ping captures under diag/, and a per-seed witness JSONL
# at /home/vibe/alpha/alpha-witness-s{42,7}.jsonl.
#
# SEED 42 RUNS FIRST AND IS SCORED ON ITS OWN. The contract pre-commits it: if
# only seed 42 completes, the pass is scored at seed 42 and the seed-7 half is
# reported as NOT RUN. NO VERDICT IS UPGRADED BY A PARTIAL SEED. The battery's
# loop is rep-outer / cell / arm-innermost, so a truncated run carries BALANCED
# n across arms and cells rather than a complete prefix and an empty tail.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL, AND THIS SCRIPT DOES NOT RUN
# IT. Running it here would put the calibration and the scored battery in one
# uninterruptible session and there would be no moment at which the
# calibration's completion could be committed. Launch order:
#
#   1. sudo bash alpha_calib.sh                     (one rep/arm/cell, s42)
#   2. commit the filled headroom + smoke table      (the contract's completion)
#   3. sudo nohup bash alpha_all.sh ... &            (this script)
#
# WATCHER NOTE (this has bitten this project before): `pgrep -f
# alpha_battery.sh` matches the WATCHER'S OWN shell whenever its command line
# contains the string. Watch the SENTINEL, or the ledger's ALPHA-BATTERY-DONE
# line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/alpha
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/DONE-S42" "$OUTDIR/DONE-S7"
if [ ! -f "$OUTDIR/alpha-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/alpha-calib-s42.log." >&2
  echo "         The headroom table and the [QALPHA]/[QCLK] smoke are produced" >&2
  echo "         by alpha_calib.sh and committed BEFORE the scored run" >&2
  echo "         (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_ALPHA_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_ALPHA_NO_CALIB:-0}" = "1" ] || exit 4
fi
REPS="${1:-8}"
echo "ALPHA-ALL start $(date -u +%FT%TZ) reps=$REPS load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash alpha_battery.sh 42 "$REPS"
touch "$OUTDIR/DONE-S42"
echo "ALPHA-ALL s42 done $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
bash alpha_battery.sh 7 "$REPS"
touch "$OUTDIR/DONE-S7"
echo "ALPHA-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
python3 ./alpha_report.py "$OUTDIR/alpha-s42.log" "$OUTDIR/alpha-s7.log" \
  > "$OUTDIR/alpha-report.txt" 2>&1 || true
touch "$OUTDIR/DONE-ALL"
echo ALPHA-ALL-DONE
