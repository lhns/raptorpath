#!/bin/bash
# THE MISSING-HALF BATTERY'S CALIBRATION — MEASUREMENT DISCIPLINE 16, and it
# may not be skipped.
#
#   sudo bash gap_calib.sh
#
# ONE rep per PHASE-1 arm at `c1`, seed 42, the SAME session and the SAME
# binary the scored battery will use. It is n = 1: no sigma, no seed-7
# evidence, and NOTHING IN IT IS A RESULT. Its output is committed as THE
# CONTRACT'S COMPLETION, in its own commit, BEFORE the scored battery launches.
#
# ── AND IT IS THE SMOKE, AND WHAT IT IS SMOKING IS THE OLD BINARY ───────
# This battery's whole design rests on a claim that has never been exercised:
# that `/home/vibe/era-old/target/release/raptorpath` — the binary BOTH prior
# ledgers were read off — still runs, still handshakes against itself, and
# still arms its ack-merge gate. Nobody has launched it with
# `RWM_ACK_MERGE=1` since 2026-08-08. The smoke's checklist:
#
#   * G-SHA: `fbd6b279…`, refused rather than warned about (in the driver).
#   * `Op` and `Oa` both LIVE: era-invariant anchors two-sided on both.
#   * `ack-merge ACTIVE` two-sided on `Oa` and ABSENT on `Op` and `Oe` —
#     the two-sided arm assertion, on a binary with no `[GATES]` echo.
#   * `[CTLD]` present on the receiver on ALL THREE arms, and `Op`'s value
#     near the 1.96 both prior ledgers published. **This is the single most
#     informative line in the calibration**: `[CTLD]` is the mechanism gauge,
#     it is era-invariant, and if `Op` reproduces 1.96 at n = 1 then the
#     mechanism side of the comparison is intact before any goodput is read.
#   * `[GATES]` ABSENT two-sided on every OLD arm (anti-mix, and the proof
#     that `era-old` is what ran).
#   * NO `[ACKDIAG]`/`[WALL]` on `Oe` despite the env asking for them — the
#     gauges do not exist at `4171b58`, and confirming that at n = 1 is what
#     makes `Oe − Op` a measurement of the LATPROBE cost alone rather than of
#     three unknowns.
#   * `CPUCLI` present on every arm, so `ms_per_MB` is computable — the
#     column MEASUREMENT TRUTH item 2's A7 needs.
#   * abort rate ~0 (the topo-ping repair landed 2026-08-19).
#
# THE HEADROOM TABLE, AND THE SECOND COLUMN THIS CELL FORCES. `c1` is a
# 1 Gbit pipe at ~21 % utilisation, so LINK headroom permits throughput
# targets with room to spare — and that permission has never been the binding
# one at this cell. The c9 battery measured a sender saturating at 68.5 ms/MB
# while its link read 50 % headroom, and MEASUREMENT TRUTH item 2's A7 asks
# whether `c1` is the same. So this calibration reports BOTH, and the contract
# pre-commits to quoting neither without the other:
#
#   link_headroom = 1 - tc_bytes*8 / (TRANSFER seconds * shaped capacity)
#   cpu_headroom  = 1 - cores / NPROC          [cores = CPUCLI / seconds]
#
# A calibration that CONTRADICTS a permission VOIDs the affected clause for
# that cell, and the clause is reported void — never re-scoped after the fact.
#
# WATCHER NOTE: `pgrep -f gap_calib.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL file, never the process table.
set -u
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1

OUTDIR="${RWM_GAP_OUTDIR:-/home/vibe/gap}"
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"

{
  echo "=== MISSING-HALF CALIBRATION $(date -u +%FT%TZ)"
  echo "=== n = 1 PER ARM. NOTHING HERE IS A RESULT."
  echo "=== nproc $(nproc)"
} > "$OUTDIR/calib-gap.txt"

RWM_GAP_TAG=gap-calib bash gap_battery.sh 42 1 >> "$OUTDIR/calib-gap.txt" 2>&1

python3 ./gap_parse.py --calib "$OUTDIR/gap-calib-s42.log" \
  > "$OUTDIR/calib-gap-table.txt" 2>&1

touch "$OUTDIR/DONE-CALIB"
echo "GAP-CALIB-DONE"
