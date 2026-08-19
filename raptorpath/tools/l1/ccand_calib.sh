#!/bin/bash
# THE CANDIDATES BATTERY — THE DISCIPLINE-16 CALIBRATION PASS.
#
#   sudo nohup bash ccand_calib.sh >/home/vibe/ccand/calib.out 2>&1 &
#
# WHAT THIS IS FOR, and it is the one step of this battery that may not be
# skipped. goal-gate "Candidates Battery — PRE-REGISTRATION" carries the METHOD
# and the protocol for its headroom table and leaves the NUMBERS empty, on
# purpose:
#
#   1. BEFORE the scored run, in the SAME session, on the SAME binary: ONE rep
#      per arm per cell, seed 42, with `tc -s qdisc show` captured on EVERY cell
#      and EVERY invocation — not a subset, which is the omission item 16 exists
#      to prevent.
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
# pre-registration that hides what it already saw is not a pre-registration.
#
# IT IS ALSO THE SMOKE, and this battery needs the smoke more than its
# predecessors did, because THREE of its gauges have never been run on a wire:
#
#   * [DCAP]'s `q=` / `b=` are THE DIAL-ROUTING CHECK (MEASUREMENT DISCIPLINE 1).
#     The harness runs the `bulk` hint, so b(Bulk)=2 and q=(b+1)/30=0.100000
#     EXACTLY. `ccand_report.py --calib` prints them per arm and per cell and
#     fails D-ROUTE loudly if they are anything else. A gate that is READ but
#     does not ROUTE is the defect ordinal tests do not catch.
#   * [RACK] rides EVERY arm, and on the CONTROL it carries `evals=0` BY
#     CONSTRUCTION with `fa=` the only field holding a datum. If A's [RACK] line
#     is absent, no recovery round fired and 16.68.1 has NO measurement — which
#     is the one thing this battery is uniquely able to deliver.
#   * [LCW] can only record on arm L (THE SPECIFICATION FINDING). Its absence
#     everywhere else is CORRECT and must be SEEN to be correct here, at n = 1
#     and 25 invocations, rather than inferred from the scored run.
#
# Read the LIVENESS / ARM-LIVENESS-FAIL / ARM-CONTAMINATION / DIAL-ROUTE-FAIL /
# INSTRUMENT-SURPRISE-LCW lines of the ledger before launching ccand_all.sh.
#
# WATCHER NOTE: `pgrep -f ccand_calib.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/ccand
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "CCAND-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "CCAND-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
} > "$OUTDIR/calib-era.txt"
RWM_CCAND_TAG=ccand-calib RWM_CCAND_SMALLREPS=1 RWM_CCAND_AUXREPS=1 \
  bash ccand_battery.sh 42 1
echo "CCAND-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"
# The headroom table itself, printed from the calibration ledger in the exact
# shape the contract's table wants. `--calib` prints the tc/headroom pass, the
# liveness audit and the NEW-GAUGE ECHO AUDIT and NOTHING ELSE: no bar is scored
# from n = 1.
python3 ./ccand_report.py --calib "$OUTDIR/ccand-calib-s42.log" \
  > "$OUTDIR/ccand-calib-headroom.txt" 2>&1 || true
touch "$OUTDIR/DONE-CALIB"
