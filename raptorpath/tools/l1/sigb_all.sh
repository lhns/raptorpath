#!/bin/bash
# THE ESTIMATOR BATTERY — both seeds, one detached session, one sentinel.
#
#   sudo nohup bash sigb_all.sh [reps] >/home/vibe/sigb/all.out 2>&1 &
#
# Writes /home/vibe/sigb/DONE-ALL when BOTH seed batteries have finished.
# Per-seed ledgers land at /home/vibe/sigb/sigb-s{42,7}.log with the per-run
# client/server/qdisc/ping captures under diag/, and a per-seed band JSONL at
# /home/vibe/sigb/sigb-witness-s{42,7}.jsonl.
#
# SEED 42 RUNS FIRST AND IS SCORED ON ITS OWN. The contract pre-commits it: if
# only seed 42 completes, the pass is scored at seed 42 and the seed-7 half is
# reported as NOT RUN. **NO VERDICT IS UPGRADED BY A PARTIAL SEED.** The
# battery's loop is rep-outer / cell-inner, so a truncated run carries BALANCED
# n across cells rather than a complete prefix and an empty tail.
#
# THE TWO SEEDS POOL, AND THE PRE-REGISTRATION SAYS SO BEFORE THE DATA. The
# bar's `R_total` is "the POOLED sigma-hat readings of ALL reps at ONE cell,
# ONE seat, ONE alpha". A seed is a netem realisation, not a seat and not an
# alpha, so both seeds' reps pool into one `R_total` per leg. That is the
# HARDER reading — pooling two loss realisations can only widen a spread, never
# narrow one — and it is chosen for that reason and stated here rather than
# discovered later.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL, AND THIS SCRIPT DOES NOT RUN
# IT. Running it here would put the calibration and the scored battery in one
# uninterruptible session and there would be no moment at which the
# calibration's completion could be committed. Launch order:
#
#   1. sudo bash sigb_calib.sh                       (one rep/cell, s42)
#   2. commit the filled headroom + smoke table       (the contract's completion)
#   3. sudo bash sigb_battery.sh ... (smoke, c8 + sc2) (the post-commit smoke)
#   4. sudo nohup bash sigb_all.sh ... &              (this script)
#
# WATCHER NOTE (this has bitten this project before): `pgrep -f
# sigb_battery.sh` matches the WATCHER'S OWN shell whenever its command line
# contains the string. Watch the SENTINEL, or the ledger's SIGB-BATTERY-DONE
# line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/sigb
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/DONE-S42" "$OUTDIR/DONE-S7"
if [ ! -f "$OUTDIR/sigb-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/sigb-calib-s42.log." >&2
  echo "         The headroom table and the W7/window/probe smoke are produced" >&2
  echo "         by sigb_calib.sh and committed BEFORE the scored run" >&2
  echo "         (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_SIGB_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_SIGB_NO_CALIB:-0}" = "1" ] || exit 4
fi
REPS="${1:-8}"
echo "SIGB-ALL start $(date -u +%FT%TZ) reps=$REPS load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash sigb_battery.sh 42 "$REPS"
touch "$OUTDIR/DONE-S42"
echo "SIGB-ALL s42 done $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
bash sigb_battery.sh 7 "$REPS"
touch "$OUTDIR/DONE-S7"
echo "SIGB-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
python3 ./sigb_report.py "$OUTDIR/sigb-s42.log" "$OUTDIR/sigb-s7.log" \
  > "$OUTDIR/sigb-report.txt" 2>&1 || true
# Per-seed reports beside the pooled one, so the seed-42-only reading the
# contract pre-commits to is available without re-running anything.
python3 ./sigb_report.py "$OUTDIR/sigb-s42.log" \
  > "$OUTDIR/sigb-report-s42.txt" 2>&1 || true
python3 ./sigb_report.py "$OUTDIR/sigb-s7.log" \
  > "$OUTDIR/sigb-report-s7.txt" 2>&1 || true
touch "$OUTDIR/DONE-ALL"
echo SIGB-ALL-DONE
