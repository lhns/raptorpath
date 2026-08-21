#!/bin/bash
# THE FIRE-CAUSE DIAGNOSTIC PASS — one seed, one detached session, one EARNED
# sentinel.
#
#   sudo nohup bash fcause_all.sh [reps] >/home/vibe/fcause/all.out 2>&1 &
#
# Writes /home/vibe/fcause/DONE-ALL when the pass has EARNED it, or
# FAILED-ALL when it has not. A watcher waits on `DONE-ALL || FAILED-ALL`, so a
# battery that dies ends the wait instead of looking like one still running.
#
# ONE SEED (42), AND THAT IS THE CONTRACT RATHER THAN A SHORTCUT. This pass
# measures a COMPOSITION (which producer caused each fire), not an effect size
# against a null. A second seed would tighten a confidence interval on a
# quantity whose pre-registered decision rule is a MAJORITY THRESHOLD, and a
# cause mix that flips across seeds would be visible as a spread across the
# five cells this pass already runs. If the five cells disagree, THAT is the
# finding and a second seed is the next step rather than this one.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
#
# WATCHER NOTE: `pgrep -f fcause_battery.sh` matches the WATCHER'S OWN shell
# whenever its command line contains the string. Watch the SENTINEL, or the
# ledger's FCAUSE-BATTERY-DONE line — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/fcause
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/FAILED-ALL" "$OUTDIR/DONE-S42" "$OUTDIR/FAILED-S42"
REPS="${1:-3}"

# A SENTINEL IS EARNED, NOT UNCONDITIONAL. MEASURED, 2026-08-21:
# `sigb_calib.sh`'s first invocation hit a CRLF trap in `lib.sh`, ran ZERO
# invocations, and still wrote its DONE sentinel — because the `touch` was the
# script's last line and nothing else. Discipline 13 tells a watcher to watch
# the SENTINEL rather than the process table, so an unconditional `touch`
# converts a total failure into a clean-looking success at exactly the moment
# nobody is looking.
seed_done() {
  local s="$1" f="$OUTDIR/fcause-s$1.log"
  if [ -s "$f" ] && grep -q "FCAUSE-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "FCAUSE-ALL seed $s DID NOT COMPLETE — no FCAUSE-BATTERY-DONE in $f" \
    | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

echo "FCAUSE-ALL start $(date -u +%FT%TZ) reps=$REPS load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
bash fcause_battery.sh 42 "$REPS"
seed_done 42
echo "FCAUSE-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"

python3 ./fcause_report.py "$OUTDIR/fcause-s42.log" \
  > "$OUTDIR/fcause-report.txt" 2>&1 || true

if [ -f "$OUTDIR/DONE-S42" ]; then
  touch "$OUTDIR/DONE-ALL"
  echo FCAUSE-ALL-DONE
else
  touch "$OUTDIR/FAILED-ALL"
  echo FCAUSE-ALL-FAILED
  exit 5
fi
