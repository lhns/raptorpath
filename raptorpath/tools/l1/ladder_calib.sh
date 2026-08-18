#!/bin/bash
# THE LADDER BATTERY — THE DISCIPLINE-16 CALIBRATION PASS.
#
#   sudo nohup bash ladder_calib.sh >/home/vibe/ladder/calib.out 2>&1 &
#
# WHAT THIS IS FOR, and it is the one step of this battery that may not be
# skipped. goal-gate "Ladder Battery — PRE-REGISTRATION" carries the METHOD and
# the protocol for its headroom table and leaves the NUMBERS empty, on purpose:
#
#   1. BEFORE the scored run, in the SAME session, on the SAME binary: ONE rep
#      per arm per cell, seed 42, with `tc -s qdisc show` captured on EVERY cell
#      and EVERY invocation — not a subset, which is the omission item 16 exists
#      to prevent (the three-term battery took tc on 2 of its 9 cells, which is
#      why its unsatisfiable criteria were only visible afterwards).
#   2. util = tc_bytes * 8 / (TRANSFER seconds * shaped capacity). THE
#      DENOMINATOR IS THE TRANSFER WALL (`seconds`), NEVER `INVOCATION_S`: the
#      latter is the whole script's wall (namespace bring-up, netem/tbf setup,
#      the verification pings, teardown), runs 1.12-2.11x the transfer, and read
#      c7 at 77.6% when the cell is at 96.9% — which would have LICENSED exactly
#      the unsatisfiable target discipline 16 forbids.
#   3. The result is committed as THE CONTRACT'S COMPLETION, in its own commit,
#      BEFORE the scored battery runs, filling the headroom table's permission
#      column: headroom >= 5% -> throughput targets permitted; < 5% -> parity /
#      latency / cap-shape only.
#   4. Where the calibration CONTRADICTS a permission the affected clause is VOID
#      for that cell and reported as void, never re-scoped after the fact.
#
# IT IS n = 1. It carries no sigma, no seed-7 evidence, and NOTHING IN IT IS A
# RESULT. It is disclosed in full with the completion commit because a
# pre-registration that hides what it already saw is not a pre-registration
# (the latency-lever pattern, applied verbatim).
#
# It is also the SMOKE: it is the first execution of every arm's env on this
# binary, so a gate that does not reach the binary, a [SUMCAP] that never
# appears, a [CCAP] missing on FULL or a contaminated control shows up here — at
# n = 1 and 20 invocations — instead of eight cells into the scored run. Read
# the LIVENESS / ARM-LIVENESS-FAIL / ARM-CONTAMINATION lines of the ledger
# before launching ladder_all.sh.
#
# WATCHER NOTE: `pgrep -f ladder_calib.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ladder
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "LADDER-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "LADDER-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
} > "$OUTDIR/calib-era.txt"
RWM_LADDER_TAG=ladder-calib RWM_LADDER_SMALLREPS=1 bash ladder_battery.sh 42 1
echo "LADDER-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"
# The headroom table itself, printed from the calibration ledger in the exact
# shape the contract's table wants. `--calib` prints the tc/headroom pass and
# the liveness audit and NOTHING ELSE: no bar is scored from n = 1.
python3 ./ladder_report.py --calib "$OUTDIR/ladder-calib-s42.log" \
  > "$OUTDIR/ladder-calib-headroom.txt" 2>&1 || true
touch "$OUTDIR/DONE-CALIB"
