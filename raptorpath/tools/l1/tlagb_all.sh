#!/bin/bash
# THE T-LAG BATTERY — both seeds, one detached session, one sentinel.
#
#   sudo nohup bash tlagb_all.sh [reps] >/home/vibe/tlagb/all.out 2>&1 &
#
# Writes /home/vibe/tlagb/DONE-ALL when BOTH seed batteries have finished.
# Per-seed ledgers land at /home/vibe/tlagb/tlagb-s{42,7}.log with the per-run
# client/server/qdisc/ping captures under diag/, and a per-seed band JSONL at
# /home/vibe/tlagb/tlagb-witness-s{42,7}.jsonl.
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
# THIS SCRIPT RUNS THE SCORED S/C PASS ONLY, WITH THE RTT DUMP OFF. Clause B is
# a SEPARATE pass with its own script, its own ledger and its own sentinel
# (`tlagb_bpass.sh`, `RWM_RTT_DUMP=1`), because the dump writes megabytes of
# sender stderr into the very endpoint whose dispersion is under measurement.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL, AND THIS SCRIPT DOES NOT RUN
# IT. Running it here would put the calibration and the scored battery in one
# uninterruptible session and there would be no moment at which the
# calibration's completion could be committed. Launch order:
#
#   1. sudo bash tlagb_calib.sh                        (one rep/cell, s42)
#   2. commit the filled headroom + smoke table        (the contract's completion)
#   3. sudo bash tlagb_battery.sh ... (smoke, c8 + sc2) (the post-commit smoke)
#   4. sudo nohup bash tlagb_all.sh ... &              (this script — the S/C pass)
#   5. sudo nohup bash tlagb_bpass.sh ... &            (the clause-B pass, dump ON)
#
# WATCHER NOTE (this has bitten this project before): `pgrep -f
# tlagb_battery.sh` matches the WATCHER'S OWN shell whenever its command line
# contains the string. Watch the SENTINEL, or the ledger's TLAGB-BATTERY-DONE
# line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/tlagb
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/DONE-S42" "$OUTDIR/DONE-S7"
if [ ! -f "$OUTDIR/tlagb-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/tlagb-calib-s42.log." >&2
  echo "         The headroom table and the W7/window/probe smoke are produced" >&2
  echo "         by tlagb_calib.sh and committed BEFORE the scored run" >&2
  echo "         (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_TLAGB_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_TLAGB_NO_CALIB:-0}" = "1" ] || exit 4
fi
REPS="${1:-8}"

# ── A SENTINEL IS EARNED, NOT UNCONDITIONAL ──────────────────────────────
#
# MEASURED, 2026-08-21: `sigb_calib.sh`'s first invocation hit a CRLF trap in
# `lib.sh`, ran ZERO invocations, and still wrote its DONE sentinel — because
# the `touch` was the script's last line and nothing else. Discipline 13 tells
# a watcher to watch the SENTINEL rather than the process table, so an
# unconditional `touch` converts a total failure into a clean-looking success
# at exactly the moment nobody is looking. Every sentinel this script writes is
# now conditional on the battery's own DONE line being in its own ledger.
seed_done() {  # seed sentinel — write it only if the ledger earned it
  local s="$1" f="$OUTDIR/tlagb-s$1.log"
  if [ -s "$f" ] && grep -q "TLAGB-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "TLAGB-ALL seed $s DID NOT COMPLETE — no TLAGB-BATTERY-DONE in $f" \
    | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

rm -f "$OUTDIR/FAILED-S42" "$OUTDIR/FAILED-S7"
echo "TLAGB-ALL start $(date -u +%FT%TZ) reps=$REPS load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash tlagb_battery.sh 42 "$REPS"
seed_done 42
echo "TLAGB-ALL s42 done $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
bash tlagb_battery.sh 7 "$REPS"
seed_done 7
echo "TLAGB-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
python3 ./tlagb_report.py "$OUTDIR/tlagb-s42.log" "$OUTDIR/tlagb-s7.log" \
  > "$OUTDIR/tlagb-report.txt" 2>&1 || true
# Per-seed reports beside the pooled one, so the seed-42-only reading the
# contract pre-commits to is available without re-running anything.
python3 ./tlagb_report.py "$OUTDIR/tlagb-s42.log" \
  > "$OUTDIR/tlagb-report-s42.txt" 2>&1 || true
python3 ./tlagb_report.py "$OUTDIR/tlagb-s7.log" \
  > "$OUTDIR/tlagb-report-s7.txt" 2>&1 || true

# THE TERMINAL SENTINEL, AND THERE ARE TWO OF THEM SO SILENCE IS NEVER THE
# ANSWER. DONE-ALL means at least seed 42 earned its DONE line — the state the
# contract pre-commits to scoring. FAILED-ALL means neither seed did. A watcher
# waits on `DONE-ALL || FAILED-ALL`, so a battery that dies still ends the wait
# instead of looking like one that is still running.
if [ -f "$OUTDIR/DONE-S42" ]; then
  touch "$OUTDIR/DONE-ALL"
  [ -f "$OUTDIR/DONE-S7" ] || echo "TLAGB-ALL: seed 7 NOT COMPLETE — score seed 42 alone, and report the seed-7 half as NOT RUN" | tee -a "$OUTDIR/all-era.txt"
  echo TLAGB-ALL-DONE
else
  touch "$OUTDIR/FAILED-ALL"
  echo TLAGB-ALL-FAILED
  exit 5
fi
