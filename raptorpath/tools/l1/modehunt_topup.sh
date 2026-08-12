#!/bin/bash
# THE MODE-HUNT BATTERY — SYMMETRIC TOP-UP, seed 7.
#
#   sudo nohup bash modehunt_topup.sh [reps] >/home/vibe/modehunt/topup.out 2>&1 &
#
# WHY THIS EXISTS. The pre-registration (goal-gate "Mode-Hunt Battery — VM
# PRE-REGISTRATION", commit b8fd6d9) fixes the convention:
#
#   "If aborts drive any SCORED arm below n = 8 at either seed, a top-up
#    session runs the SAME rep count for EVERY scored arm at that seed, under
#    the SAME binary, with its OWN ledger and OWN sentinel, pooled and
#    reported separately. Never asymmetric."
#
# The main pool tripped it: at seed 7 the documented topo-ping abort class took
# `c8-AUR` to **7 live** (12 headers, 5 aborts), one below the floor. Seed 42
# recorded ZERO aborts and is untouched by this session.
#
# WHAT IS SYMMETRIC HERE. Every SCORED arm at seed 7 gets the SAME rep count —
# `c8-AU`, `c8-AUR` and `c8L-AU`, which is exactly what `RWM_MH_ARMS="AU AUR"`
# selects once `cell_arms` restricts c8L to AU. The `A` pin is NOT topped up
# and that is not an asymmetry: the pre-registration disqualifies it as a
# contrast at any n, so it is not a scored arm and topping it up would buy
# nothing. Nothing is topped up on seed 42 because nothing there fell.
#
# THE BINARY IS NOT REBUILT. The top-up is only poolable with the main session
# if it is the same bytes; the caller asserts the sha256 before launching and
# the ledger header re-records it.
#
# WATCHER NOTE: `pgrep -f modehunt_topup.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/modehunt
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-TOPUP"
echo "MODEHUNT-TOPUP start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/topup-era.txt"
echo "MODEHUNT-TOPUP reason: c8-AUR seed 7 at 7 live (< 8 floor); symmetric over AU/AUR/c8L-AU" >> "$OUTDIR/topup-era.txt"
RWM_MH_TAG=topup RWM_MH_ARMS="AU AUR" bash modehunt_battery.sh 7 "${1:-8}"
echo "MODEHUNT-TOPUP end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/topup-era.txt"
touch "$OUTDIR/DONE-TOPUP"
