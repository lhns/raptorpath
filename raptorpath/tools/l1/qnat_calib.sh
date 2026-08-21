#!/bin/bash
# THE QUANTILE-NATIVE α-SWEEP — THE DISCIPLINE-16 CALIBRATION, WHICH IS ALSO
# THE SMOKE.
#
#   sudo nohup bash qnat_calib.sh >/home/vibe/qnat/calib.out 2>&1 &
#
# ONE REP PER ARM PER CELL, seed 42, on the SAME binary and in the SAME session
# as the scored run, BEFORE it. 30 invocations. NOTHING IN IT IS A RESULT — no
# bar is scored from n = 1, and the report's verdict section will say NO VERDICT
# on this input, which is correct.
#
# WHAT IT IS FOR — two things, and neither may be skipped.
#
# (1) HEADROOM (MEASUREMENT DISCIPLINE 16). `tc -s qdisc show` on EVERY cell
#     and EVERY invocation, not a subset — the omission item 16 exists to
#     prevent. util = tc_bytes * 8 / (TRANSFER seconds * shaped capacity).
#     THE DENOMINATOR IS THE TRANSFER WALL (`seconds`), NEVER `INVOCATION_S`:
#     the latter is the whole script's wall (namespace bring-up, netem/tbf
#     setup, verification pings, teardown), runs 1.12-2.11x the transfer, and
#     read c7 at 77.6% when the cell was at 96.9% — which would have LICENSED
#     exactly the unsatisfiable target discipline 16 forbids.
#
#     THIS BATTERY WRITES NO GOODPUT-GAIN CLAUSE ANYWHERE, so the headroom
#     table cannot license one. It exists here to state, as a measured number,
#     that the goodput axis of the cost curve is ONE-SIDED DOWN.
#
# (2) THE SMOKE, and this battery needs it because THE AXIS IT SWEEPS HAS NEVER
#     BEEN ON A WIRE:
#
#     * `[GATES] RWM_W_FORM=` — brand new, and it is a WORD and not a flag.
#       ABSENT and GARBAGE both resolve to `cantelli`, so the CTL arm's expected
#       echo is `cantelli` while its env carries NO TOKEN AT ALL. A harness that
#       matched this as `[01]` would read the empty string at every arm and its
#       liveness gate would pass because it never matched.
#     * `[QALPHA] form= win_n=` — W7, the arm-liveness witness for the new axis,
#       at BOTH endpoints. On CTL `win_n` reads `unavail` at the sender and the
#       two sites DISAGREE about the contract α (the hint is not plumbed to the
#       receiver task). That disagreement is expected, is documented, and
#       DISAPPEARS on every Q arm because an override is a number and not a hint
#       mapping — which is why the receiver's `win_n` is asserted there and not
#       on CTL.
#     * `[QCLK] win_ok=` — the WINDOW-FILL counter. `win_ok < evals` on a Q arm
#       is a WINDOW-PARTIAL RESULT and never an abort, but a cell where the
#       window NEVER fills must be discovered here rather than after 80
#       invocations have been spent on it. `c8` is the cell at risk: it is the
#       25 MB cell, the smallest converged sample count in the primitives table,
#       and `Q002` wants a 5000-sample window.
#
# Read, in the calibration ledger, BEFORE launching qnat_all.sh:
#   ABORT / ARM-LIVENESS-FAIL / ARM-CONTAMINATION / INSTRUMENT-FAIL-GATE /
#   W6-FAIL-CLI / W6-FAIL-SRV / W7-QFORM-FAIL-CLI / W7-QFORM-FAIL-SRV /
#   W7-QWINN-FAIL-CLI / W7-QWINN-FAIL-SRV / QNAT-LAW-DEAD / WINDOW-PARTIAL /
#   QNAT-PARSE-FAIL
#
# WATCHER NOTE: `pgrep -f qnat_calib.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table (discipline 13).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/qnat
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB" "$OUTDIR/FAILED-CALIB"
{
  echo "QNAT-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "QNAT-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
  echo "QNAT-CALIB NOTHING HERE IS A RESULT (n = 1)"
} > "$OUTDIR/calib-era.txt"
RWM_QNAT_TAG=qnat-calib bash qnat_battery.sh 42 1
echo "QNAT-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"

# ── THE SENTINEL IS EARNED, NOT UNCONDITIONAL ────────────────────────────
#
# MEASURED, 2026-08-21, on `sigb_calib.sh`'s first invocation. The shipped
# source tree reached the VM with CRLF line endings (`git archive` under
# `core.autocrlf=true`), so `lib.sh` died at line 6 with `$'\r': command not
# found`, the battery exited immediately, NOT ONE INVOCATION RAN — and the
# script cheerfully `touch`ed DONE-CALIB and printed its DONE line.
#
# **A WATCHER WATCHING THE SENTINEL — WHICH IS EXACTLY WHAT DISCIPLINE 13 TELLS
# IT TO WATCH — WOULD HAVE READ THAT AS A COMPLETED CALIBRATION.** An
# unconditional `touch` at the end of a script means only "the script reached
# its last line". That is a liveness signal for the shell, not a completion
# signal for the measurement.
#
# So the sentinel is CONDITIONAL on the battery's own DONE line being in its own
# ledger, for its own seed. A run that produced no ledger writes FAILED-CALIB
# instead, which is a sentinel too — one that says the opposite thing.
LEDGER="$OUTDIR/qnat-calib-s42.log"
if [ -s "$LEDGER" ] && grep -q "QNAT-BATTERY-DONE seed=42" "$LEDGER"; then
  python3 ./qnat_report.py "$LEDGER" \
    > "$OUTDIR/qnat-calib-report.txt" 2>&1 || true
  touch "$OUTDIR/DONE-CALIB"
  echo QNAT-CALIB-DONE
else
  {
    echo "QNAT-CALIB FAILED $(date -u +%FT%TZ)"
    echo "  no ledger at $LEDGER, or no 'QNAT-BATTERY-DONE seed=42' line in it."
    echo "  THE BATTERY DID NOT RUN. Check for a CRLF trap in tools/l1 first:"
    echo "    python3 -c 'print(open(\"lib.sh\",\"rb\").read().count(b\"\\r\\n\"))'"
  } | tee -a "$OUTDIR/calib-era.txt"
  touch "$OUTDIR/FAILED-CALIB"
  echo QNAT-CALIB-FAILED
  exit 5
fi
