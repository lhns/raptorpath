#!/bin/bash
# THE QUANTILE-NATIVE α-SWEEP — both seeds, one detached session, one sentinel.
#
#   sudo nohup bash qnat_all.sh [reps] >/home/vibe/qnat/all.out 2>&1 &
#
# Writes /home/vibe/qnat/DONE-ALL when the pass has EARNED it. Per-seed ledgers
# land at /home/vibe/qnat/qnat-s{42,7}.log with the per-run client/server/qdisc/
# ping captures under diag/, and a per-seed witness JSONL at
# /home/vibe/qnat/qnat-witness-s{42,7}.jsonl.
#
# SEED 42 RUNS FIRST AND IS SCORED ON ITS OWN. The contract pre-commits it: if
# only seed 42 completes, the pass is scored at seed 42 and the seed-7 half is
# reported as NOT RUN. **NO VERDICT IS UPGRADED BY A PARTIAL SEED.** The
# battery's loop is rep-outer / cell / arm-innermost, so a truncated run carries
# BALANCED n across arms and cells rather than a complete prefix and an empty
# tail.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL, AND THIS SCRIPT DOES NOT RUN
# IT. Running it here would put the calibration and the scored battery in one
# uninterruptible session and there would be no moment at which the
# calibration's completion could be committed. Launch order:
#
#   1. sudo bash qnat_calib.sh                      (one rep/arm/cell, s42)
#   2. commit the filled headroom + smoke table      (the contract's completion)
#   3. sudo nohup bash qnat_all.sh ... &             (this script)
#
# WATCHER NOTE (this has bitten this project before): `pgrep -f qnat_battery.sh`
# matches the WATCHER'S OWN shell whenever its command line contains the string.
# Watch the SENTINEL, or the ledger's QNAT-BATTERY-DONE line — never the process
# table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/qnat
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/FAILED-ALL" \
      "$OUTDIR/DONE-S42" "$OUTDIR/FAILED-S42" \
      "$OUTDIR/DONE-S7" "$OUTDIR/FAILED-S7"
if [ ! -f "$OUTDIR/qnat-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/qnat-calib-s42.log." >&2
  echo "         The headroom table and the [GATES] RWM_W_FORM / [QALPHA]" >&2
  echo "         form=/win_n= / [QCLK] win_ok= smoke are produced by" >&2
  echo "         qnat_calib.sh and committed BEFORE the scored run" >&2
  echo "         (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_QNAT_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_QNAT_NO_CALIB:-0}" = "1" ] || exit 4
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
  local s="$1" f="$OUTDIR/qnat-s$1.log"
  if [ -s "$f" ] && grep -q "QNAT-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "QNAT-ALL seed $s DID NOT COMPLETE — no QNAT-BATTERY-DONE in $f" \
    | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

echo "QNAT-ALL start $(date -u +%FT%TZ) reps=$REPS load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash qnat_battery.sh 42 "$REPS"
seed_done 42
echo "QNAT-ALL s42 done $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
bash qnat_battery.sh 7 "$REPS"
seed_done 7
echo "QNAT-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
python3 ./qnat_report.py "$OUTDIR/qnat-s42.log" "$OUTDIR/qnat-s7.log" \
  > "$OUTDIR/qnat-report.txt" 2>&1 || true
# Per-seed reports beside the pooled one, so the seed-42-only reading the
# contract pre-commits to is available without re-running anything.
python3 ./qnat_report.py "$OUTDIR/qnat-s42.log" \
  > "$OUTDIR/qnat-report-s42.txt" 2>&1 || true
python3 ./qnat_report.py "$OUTDIR/qnat-s7.log" \
  > "$OUTDIR/qnat-report-s7.txt" 2>&1 || true

# THE TERMINAL SENTINEL, AND THERE ARE TWO OF THEM SO SILENCE IS NEVER THE
# ANSWER. DONE-ALL means at least seed 42 earned its DONE line — the state the
# contract pre-commits to scoring. FAILED-ALL means neither seed did. A watcher
# waits on `DONE-ALL || FAILED-ALL`, so a battery that dies still ends the wait
# instead of looking like one that is still running.
if [ -f "$OUTDIR/DONE-S42" ]; then
  touch "$OUTDIR/DONE-ALL"
  [ -f "$OUTDIR/DONE-S7" ] || echo "QNAT-ALL: seed 7 NOT COMPLETE — score seed 42 alone, and report the seed-7 half as NOT RUN" | tee -a "$OUTDIR/all-era.txt"
  echo QNAT-ALL-DONE
else
  touch "$OUTDIR/FAILED-ALL"
  echo QNAT-ALL-FAILED
  exit 5
fi
