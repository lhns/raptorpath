#!/bin/bash
# THE ESTIMATOR BATTERY — THE DISCIPLINE-16 CALIBRATION, WHICH IS ALSO THE SMOKE.
#
#   sudo nohup bash sigb_calib.sh >/home/vibe/sigb/calib.out 2>&1 &
#
# ONE REP PER CELL, seed 42, on the SAME binary and in the SAME session as the
# scored run, BEFORE it. 5 invocations. **NOTHING IN IT IS A RESULT** — no
# clause of `S`, `B` or `C` is scored from `n = 1`, and the report's verdict
# section will say NO VERDICT on this input, which is correct.
#
# WHAT IT IS FOR — two things, and neither may be skipped.
#
# (1) HEADROOM (MEASUREMENT DISCIPLINE 16). `tc -s qdisc show` on EVERY cell
#     and EVERY invocation, not a subset. util = tc_bytes * 8 / (TRANSFER
#     seconds * shaped capacity). THE DENOMINATOR IS THE TRANSFER WALL, NEVER
#     `INVOCATION_S`. This battery writes NO GOODPUT CLAUSE, so the table
#     licenses nothing; it is here because a cell running at its ceiling
#     produces a different RTT sample process from one that is not, and that is
#     a property of the INPUT to the thing under test.
#
# (2) THE SMOKE, and this battery needs it as much as its predecessors did,
#     because THREE OF THE FOUR THINGS IT READS HAVE NEVER BEEN ON A WIRE:
#
#     * `rvar_us` / `qsp_us` / `msd_us` — brand new, and the whole measurement.
#       They must be present WITH `/n` counts on EVERY path entry of EVERY
#       [DIAG] block at BOTH endpoints (`W7`). A missing token is not a missing
#       column, it is the measurement failing.
#     * THE WINDOW-CLASS GAUGES MUST REACH A FULL WINDOW at every cell. `qsp`
#       needs `n >= 256` and `msd` needs `n >= 255`, and clause `C1` excludes
#       every reading below that. A cell where the window never fills yields
#       ZERO scoreable readings and the leg is UNSCOREABLE — which the
#       calibration must discover before 80 invocations are spent on it. `c8`
#       is the cell at risk: it is 25 MB, the smallest converged sample count
#       in the primitives table.
#     * THE PER-LEG PROBE, through the CANDIDATES' OWN FUNCTIONALS. Clause `B`
#       needs `P90-P50`, `median|dx|` and `sd` off the probe stream, per leg,
#       with censoring accounting. `latt_probe.py`'s repair is field-tested;
#       `sigb_probe.py`'s functionals are not.
#
# Read, in the calibration ledger, BEFORE launching sigb_all.sh:
#   ABORT / ABORT-GEN-PLATEAU / SUBSTRATE-FAIL / INSTRUMENT-FAIL-GATE /
#   INSTRUMENT-FAIL-PROBE / W7-FAIL-CLI / SIGB-PARSE-FAIL / OUT-OF-BAND
# and in the report: §1's witness block, §2's clause-C table (does the window
# fill?), and §5's probe block (is any leg over the contract bar?).
#
# WATCHER NOTE: `pgrep -f sigb_calib.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table (MEASUREMENT DISCIPLINE 13).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/sigb
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "SIGB-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "SIGB-CALIB one rep per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
  echo "SIGB-CALIB NOTHING HERE IS A RESULT (n = 1)"
} > "$OUTDIR/calib-era.txt"
RWM_SIGB_TAG=sigb-calib bash sigb_battery.sh 42 1
echo "SIGB-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"

# ── THE SENTINEL IS EARNED, NOT UNCONDITIONAL ────────────────────────────
#
# MEASURED, 2026-08-21, ON THIS SCRIPT'S FIRST INVOCATION. The shipped source
# tree reached the VM with CRLF line endings (`git archive` under
# `core.autocrlf=true`), so `lib.sh` died at line 6 with `$'\r': command not
# found`, `sigb_battery.sh` exited immediately, NOT ONE INVOCATION RAN — and
# this script cheerfully `touch`ed DONE-CALIB and printed SIGB-CALIB-DONE.
#
# **A WATCHER WATCHING THE SENTINEL — WHICH IS EXACTLY WHAT DISCIPLINE 13 TELLS
# IT TO WATCH — WOULD HAVE READ THAT AS A COMPLETED CALIBRATION.** The whole
# point of watching a sentinel instead of the process table is that the
# sentinel means something, and an unconditional `touch` at the end of a script
# means only "the script reached its last line". That is not a completion
# signal, it is a liveness signal for the shell.
#
# So the sentinel is now CONDITIONAL on the battery's own DONE line being in
# its own ledger. A run that produced no ledger writes FAILED-CALIB instead,
# which is a sentinel too — one that says the opposite thing.
LEDGER="$OUTDIR/sigb-calib-s42.log"
rm -f "$OUTDIR/FAILED-CALIB"
if [ -s "$LEDGER" ] && grep -q "SIGB-BATTERY-DONE" "$LEDGER"; then
  python3 ./sigb_report.py "$LEDGER" \
    > "$OUTDIR/sigb-calib-report.txt" 2>&1 || true
  touch "$OUTDIR/DONE-CALIB"
  echo SIGB-CALIB-DONE
else
  {
    echo "SIGB-CALIB FAILED $(date -u +%FT%TZ)"
    echo "  no ledger at $LEDGER, or no SIGB-BATTERY-DONE line in it."
    echo "  THE BATTERY DID NOT RUN. Check for a CRLF trap in tools/l1 first:"
    echo "    python3 -c 'print(open(\"lib.sh\",\"rb\").read().count(b\"\\r\\n\"))'"
  } | tee -a "$OUTDIR/calib-era.txt"
  touch "$OUTDIR/FAILED-CALIB"
  echo SIGB-CALIB-FAILED
  exit 5
fi
