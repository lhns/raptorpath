#!/bin/bash
# THE ERA BATTERY — both seeds, one detached session, one completion sentinel
# (discipline 13: launch detached, never poll).
#
#   sudo nohup bash era_all.sh >/home/vibe/era/all.out 2>&1 &
#
# Writes /home/vibe/era/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/era/era-s{42,7}.log with the per-run
# client/server/qdisc/ping/abort captures under diag/.
#
# THE CALIBRATION RUNS FIRST AND IS NOT OPTIONAL. goal-gate "Era Battery —
# PRE-REGISTRATION" leaves its headroom table EMPTY on purpose and fixes the
# protocol instead: one rep per arm per cell, seed 42, tc-measured, SAME session,
# SAME TWO binaries, BEFORE the scored run, committed as the contract's
# COMPLETION in its own commit before this script is launched. `era_calib.sh` is
# that pass. This script does NOT run it — running it here would put the
# calibration and the scored battery in one uninterruptible session and there
# would be no moment at which the completion could be committed. Launch order:
#
#   1. build BOTH eras and write their COMMIT files      (see BOTH TREES below)
#   2. sudo bash era_calib.sh                            (one rep/arm/cell, s42)
#   3. commit the filled headroom table to goal-gate      (contract's completion)
#   4. sudo nohup bash era_all.sh ... &                   (this script)
#
# BOTH TREES, and this is the step no previous battery had:
#
#   NEW  /home/vibe/raptorpath              at 6ad964d   (the working tree)
#   OLD  /home/vibe/era-old                 at 4171b5843d22140d54b2d05fc153451d0d03c545
#
#     git -C /home/vibe/raptorpath worktree add --detach /home/vibe/era-old \
#         4171b5843d22140d54b2d05fc153451d0d03c545
#     (cd /home/vibe/era-old && cargo build --release -p raptorpath \
#        && git rev-parse HEAD > COMMIT)
#     (cd /home/vibe/raptorpath && git rev-parse HEAD > COMMIT)
#
#   The OLD era builds clean with today's toolchain — VERIFIED LOCALLY before
#   the contract was written (cargo/rustc 1.95.0, `cargo check --release -p
#   raptorpath --all-targets`, 0 errors, warnings only). `era_battery.sh`
#   refuses to start if either binary is missing OR if the two sha256 are
#   IDENTICAL, because a session that silently ran one era twice would produce a
#   battery whose central contrast is zero by construction.
#
# WATCHER NOTE (carried from ladder_all.sh / ccand_all.sh, and it has bitten this
# project before): `pgrep -f era_battery.sh` matches the WATCHER'S OWN shell
# whenever its command line contains the string. Watch the SENTINEL, or the
# ledger's ERA-BATTERY-DONE line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it manufactures
# the seed-7 abort signature on seed 42 (measured 2026-08-07: 121 RUN-RETRY over
# 171 polled invocations against 0 over 80 unpolled ones). THAT MATTERS MORE HERE
# THAN ANYWHERE: this battery's whole abort protocol exists because the class is
# arm-correlated, and polling would manufacture exactly the correlation the
# abort-cause witness is deployed to explain. Launch this, then WAIT. Collect
# once, at the end, and read the ABORT-CAUSE TABLE BEFORE any contrast.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/era
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
if [ ! -f "$OUTDIR/era-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/era-calib-s42.log." >&2
  echo "         The contract's headroom table is filled by era_calib.sh and" >&2
  echo "         committed BEFORE the scored run (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_ERA_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_ERA_NO_CALIB:-0}" = "1" ] || exit 4
fi
{
  echo "ERA-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "ERA-ALL OLD $(cat "${RWM_ERA_OLD_ROOT:-/home/vibe/era-old}/COMMIT" 2>/dev/null)"
  echo "ERA-ALL NEW $(cat "${RWM_ERA_NEW_ROOT:-/home/vibe/raptorpath}/COMMIT" 2>/dev/null)"
} > "$OUTDIR/all-era.txt"
bash era_battery.sh 42 "${1:-12}"
bash era_battery.sh 7 "${1:-12}"
echo "ERA-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"
touch "$OUTDIR/DONE-ALL"
