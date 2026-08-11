#!/bin/bash
# SYMMETRIC seed-7 TOP-UP for the store-cap unification attribution +
# flip battery. The pre-registration ("Store-Cap Unification —
# ATTRIBUTION + FLIP BATTERY — PRE-REGISTRATION") fixes the handling of
# the documented seed-7 topo-ping abort class:
#
#   "Seed-7 aborts (the documented topo-ping class: 9.7-38.9% at
#    sc2/c7/c8, 0% at c1, 0% on s42) are handled by SYMMETRIC top-up
#    sessions only — all five arms — pooled separately, never silently
#    merged."
#
# WHY THIS RUN EXISTS (measured, not chosen): the 2026-08-11 battery
# aborted 40/312 invocations (12.8% overall, ALL of them seed 7 at
# sc2/c7/c8; c1 and every s42 cell came in whole). That left LIVE
# per-arm n at seed 7 below the pre-registered n = 8:
#
#   sc2  A=6 AU=7 AL=5 ALU=3      c7  A=1 AU=7 AL=6 ALU=5
#   c8   A=4 AU=2 AL=4 ALU=6   <- THE DECISION CELL (U1 reads off AU)
#
# so U1's seed-7 verdict currently rests on n = 2. This session buys the
# n back. It is SYMMETRIC by construction: it runs the SAME rep count for
# EVERY arm at EVERY deficient cell (no arm is topped up to its own
# target), it passes the full arm list so the driver's own `arm_cells`
# rule decides RU's scope rather than a hand-picked selection, and it
# changes nothing else — same binary, same source commit, same driver.
#
# REPS: 20, sized to the MEASURED abort rate (40/96 = 41.7% at these
# three cells on this seed), not the hoped-for one: 20 attempts leave an
# expected ~11-12 live per arm, >= 8 with margin even if the class runs
# hotter than it did. Runtime estimate from this session's own RUNTIME
# lines (sc2 9.3s + c7 8.6s + c8 4.3s per invocation, x4 arms x20 reps)
# is ~30 min.
#
# POOLING: the top-up writes its OWN ledger (unitop-s7.log) and its own
# sentinel. Its reps are a SEPARATE same-session pool and are reported as
# such — never silently merged into the main session's rows.
#
#   sudo nohup bash uni_topup.sh >/home/vibe/uniflip/topup.out 2>&1 &
#
# Sentinel: /home/vibe/uniflip/DONE-TOPUP (discipline 13 — launch
# detached, watch the sentinel, never poll the process table).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/uniflip
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-TOPUP"

# The main battery's per-invocation diag captures are named
# <cell>-<arm>-s<seed>-r<rep>-{c,s}.log with NO tag, so a same-seed
# top-up at the same reps would overwrite them. Preserve them first;
# refuse to start if that fails (raw evidence is not expendable).
if [ -d "$OUTDIR/diag" ] && [ ! -d "$OUTDIR/diag-battery" ]; then
  mv "$OUTDIR/diag" "$OUTDIR/diag-battery" || exit 4
fi
mkdir -p "$OUTDIR/diag"

echo "UNI-TOPUP start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" \
  > "$OUTDIR/topup-era.txt"
sha256sum /home/vibe/raptorpath/target/release/raptorpath >> "$OUTDIR/topup-era.txt"
cat /home/vibe/raptorpath/COMMIT >> "$OUTDIR/topup-era.txt"

RWM_UNI_TAG=unitop \
RWM_UNI_CELLS="sc2 c7 c8" \
RWM_UNI_ARMS="A AU AL ALU RU" \
  bash uni_battery.sh 7 "${1:-20}" "${1:-20}"

echo "UNI-TOPUP end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" \
  >> "$OUTDIR/topup-era.txt"
touch "$OUTDIR/DONE-TOPUP"
