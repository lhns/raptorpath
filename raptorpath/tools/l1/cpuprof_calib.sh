#!/bin/bash
# THE CPU-CEILING BATTERY'S CALIBRATION — MEASUREMENT DISCIPLINE 16, and it
# may not be skipped.
#
#   sudo bash cpuprof_calib.sh
#
# ONE rep per arm at the contract's cell, seed 42, the SAME session and the
# SAME binaries the scored battery will use. It is n = 1: no sigma, no seed-7
# evidence, and NOTHING IN IT IS A RESULT. Its output is committed as THE
# CONTRACT'S COMPLETION, in its own commit, BEFORE the scored battery launches.
#
# ── WHAT THIS CALIBRATION IS FOR, AND IT IS NOT THE USUAL THING ─────────
# Every calibration in this tree so far has answered ONE question — does the
# shaped link have headroom, so may throughput be claimed. **At c9 that
# question has already been answered and the answer is what created this
# battery**: the link reads 50.5 % headroom while the cell is saturated AT THE
# WRONG BOTTLENECK. The link permission is therefore recorded but is NOT the
# binding one here.
#
# THE BINDING PERMISSION AT A SENDER-BOUND CELL IS CPU HEADROOM, and this
# calibration is the first in the tree to measure it:
#
#   link_headroom = 1 - tc_bytes*8 / (TRANSFER seconds * shaped capacity)
#   cpu_headroom  = 1 - cores / NPROC          [cores = CPUCLI / seconds]
#
# The c9 ledger read `cores` = 1.51 of 6, i.e. 75 % CPU headroom on the BOX
# while the FLOW was pinned — which is why the contract's own clause says the
# ceiling is per-flow and not a saturated machine. Both numbers go in the
# table so neither can be quoted without the other.
#
# ── AND IT IS ALSO THE SMOKE, AND WHAT IT IS SMOKING IS THE INSTRUMENT PAIR
#   * `S`: does `RWM_CPUPROF=1` reach the binary through `perf_rwm_c.sh`'s
#     forwarding, does `[CPUPROF]` fire exactly once, and are ALL FIVE SEAMS
#     FED at a QUAD (the loopback reachability gate proves it at n=1 path;
#     four legs is a different placement regime and a seam could be dark).
#   * `P`: does `perf record -p` attach at all under this kernel's
#     `perf_event_paranoid`, does it produce SYMBOLIZED rows, and how big is
#     the attach gap.
#   * `B`: is the control CLEAN — no `RWM_CPUPROF=1`, no `[CPUPROF]` line.
#   * `.text` equality between the release and release-prof builds.
#   * the INSTRUMENT COST at n=1: S-vs-B and P-vs-B ms/MB. A cost that reads
#     large here is not a reason to re-scope; it is a number the contract
#     already pre-commits to reporting beside every decomposition.
#
# A calibration that CONTRADICTS a permission VOIDs the affected clause for
# that cell, and the clause is reported void — never re-scoped after the fact.
#
# WATCHER NOTE: `pgrep -f cpuprof_calib.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL file, never the process table.
set -u
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1

OUTDIR="${RWM_CP_OUTDIR:-/home/vibe/cpuprof}"
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"

{
  echo "=== CPU-CEILING CALIBRATION $(date -u +%FT%TZ)"
  echo "=== n = 1 PER ARM. NOTHING HERE IS A RESULT."
  echo "=== nproc $(nproc)"
} > "$OUTDIR/calib-cpuprof.txt"

RWM_CP_TAG=cpuprof-calib bash cpuprof_battery.sh 42 1 \
  >> "$OUTDIR/calib-cpuprof.txt" 2>&1

python3 ./cpuprof_parse.py --calib "$OUTDIR/cpuprof-calib-s42.log" \
  > "$OUTDIR/calib-cpuprof-table.txt" 2>&1

touch "$OUTDIR/DONE-CALIB"
echo "CPUPROF-CALIB-DONE"
