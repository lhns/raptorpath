#!/bin/bash
# THE α-SWEEP — THE DISCIPLINE-16 CALIBRATION PASS, WHICH IS ALSO THE SMOKE.
#
#   sudo nohup bash alpha_calib.sh >/home/vibe/alpha/calib.out 2>&1 &
#
# ONE REP PER ARM PER CELL, seed 42, on the SAME binary and in the SAME session
# as the scored run, BEFORE it. 30 invocations. NOTHING IN IT IS A RESULT.
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
#     that the goodput axis of the cost curve is ONE-SIDED DOWN — which is
#     what a cost curve wants and the opposite of the three-term battery's
#     error.
#
# (2) THE SMOKE, and this battery needs it more than its predecessors did,
#     because THREE of the things it reads have never been on a wire at all:
#
#     * [QALPHA] — brand new. It is W6, the arm-liveness witness, and it must
#       show the arm's OWN α at BOTH endpoints. On CTL it must show
#       `quantile=0` and `override=unset`, and the two sites will DISAGREE
#       about the contract's α there (sender bulk 1e-3, receiver Auto 1e-5:
#       the hint is not plumbed to the receiver task). That disagreement is
#       expected, is documented, and DISAPPEARS on every treatment arm because
#       an override is a number and not a hint mapping.
#     * [QCLK] — brand new, and `law_n` is the field to read first. A
#       treatment arm with `law_n = 0` never ran its own law: every evaluation
#       fell through to the law below it. That is the α-reachability gate
#       expressed at the site α actually enters, and a battery launched
#       without checking it would report a curve drawn from two different
#       laws.
#     * [RFA] at these five cells on SIX arms — the plain-window pass ran it
#       on one arm. Its `fill_src` fraction is what decides whether the
#       realized/commanded contrast resolves anywhere but c1.
#
# Read, in the calibration ledger, BEFORE launching alpha_all.sh:
#   ABORT / ARM-LIVENESS-FAIL / ARM-CONTAMINATION / INSTRUMENT-FAIL-GATE /
#   W6-FAIL-CLI / W6-FAIL-SRV / QCLK-LAW-DEAD / ALPHA-PARSE-FAIL
#
# WATCHER NOTE: `pgrep -f alpha_calib.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL — never the process table (discipline 13).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/alpha
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "ALPHA-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "ALPHA-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
} > "$OUTDIR/calib-era.txt"
RWM_ALPHA_TAG=alpha-calib bash alpha_battery.sh 42 1
echo "ALPHA-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"
# The headroom table and the liveness audit, printed from the calibration
# ledger. NO BAR IS SCORED FROM n = 1 — the report's verdict section will say
# NO VERDICT on this input, and that is correct.
python3 ./alpha_report.py "$OUTDIR/alpha-calib-s42.log" \
  > "$OUTDIR/alpha-calib-report.txt" 2>&1 || true
touch "$OUTDIR/DONE-CALIB"
echo ALPHA-CALIB-DONE
