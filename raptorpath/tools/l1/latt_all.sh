#!/bin/bash
# THE LATENCY-TRUTH BATTERY — both seeds, one detached session, one completion
# sentinel (discipline 13: launch detached, never poll).
#
#   sudo nohup bash latt_all.sh >/home/vibe/latt/all.out 2>&1 &
#
# Writes /home/vibe/latt/DONE-ALL when BOTH seed batteries have finished;
# per-seed ledgers land at /home/vibe/latt/latt-s{42,7}.log with the per-run
# client/server/qdisc/PER-LEG-ping/abort captures under diag/.
#
# ── WHAT THIS BATTERY IS FOR ─────────────────────────────────────────────
# goal-gate "Era Battery — THE SCORED RESULT" §4 closed with a SIGN
# DISAGREEMENT it could not resolve and refused to average away: the engine's
# `q_p50` standing-queue estimate fell by 198-342 ms at the lossy duals while
# the independent ICMP probe read 13-45 ms SLOWER. It named the open instrument
# question rather than filing it as a caveat. This is the successor that
# answers it, and the answer is allowed to be "the delivered latency is worse".
#
# **THE PROBE THE ERA BATTERY USED WAS BROKEN IN THREE WAYS** (all read off the
# harness at `7f2b009`, all repaired in the commit before the pre-registration,
# all HARNESS-SIDE so both arms get the byte-identical instrument):
#   1. it pinged path A on EVERY topology, sampling ONE leg of the asymmetric
#      duals whose two legs the arms load DIFFERENTLY;
#   2. it was reaped with SIGTERM, which `ping` does not handle, so the
#      transmitted/received summary was never written and the loss columns were
#      None on all 204 invocations;
#   3. lost probes produce no sample at all, so the tail percentiles were
#      computed over the survivors of a deliberately lossy link — censoring the
#      worst states and biasing the tail LOW.
#
# ── THE ARMS — the era battery's, unchanged, because the CLAIM under
#    adjudication is the era battery's ─────────────────────────────────────
#   OLD  4171b584   the pre-arc default. PROTOCOL_VERSION 6, NO [GATES] echo.
#   NEW  main       today's shipped defaults.
#   (NO NR ARM. The auxiliary RACK instrument is scored on its own [RACK] line
#   in the era ledger and has nothing to say about delivered latency; running it
#   here would buy nothing and cost a sixth of the wall time.)
#
# ── THE CELLS — three, not five ──────────────────────────────────────────
#   c8   c2/c3 dual    25 MB  120 Mbit  n=12   THE DISAGREEMENT
#   c8L  c2/c3 dual   200 MB  120 Mbit  n=12   THE DISAGREEMENT, length axis
#   c7   c2/c2 dual   200 MB  200 Mbit  n=8    THE SYMMETRIC CONTROL — the cell
#                                              where the two instruments AGREED,
#                                              so a repair that breaks the
#                                              agreement here is a repair that
#                                              broke something.
#   `c1` and `sc2` are dropped: `c1` is single-path at 21 % utilisation where
#   the probe reads a flat 2.1 ms on both arms, and `sc2` is pre-registered
#   PARITY/LATENCY-ONLY at 98 % utilisation where the probe measures the wall.
#   Neither can move this question, and the wall time buys reps at the duals
#   instead.
#
# BOTH TREES, exactly as the era battery built them (same recipe, so the OLD
# binary is the SAME BINARY the era verdict was read off — verify the sha):
#
#   NEW  /home/vibe/raptorpath              at main
#   OLD  /home/vibe/era-old                 at 4171b5843d22140d54b2d05fc153451d0d03c545
#        sha256 fbd6b279d0d69a8f4d14f177fc5fead34c0ec9c04f3322a74b17528ca4cbaf4d
#
#     git -C /home/vibe/raptorpath worktree add --detach /home/vibe/era-old \
#         4171b5843d22140d54b2d05fc153451d0d03c545
#     (cd /home/vibe/era-old && cargo build --release -p raptorpath \
#        && git rev-parse HEAD > COMMIT)
#     (cd /home/vibe/raptorpath && git rev-parse HEAD > COMMIT)
#
# `era_battery.sh` refuses to start if either binary is missing OR if the two
# sha256 are IDENTICAL.
#
# LAUNCH ORDER, and step 3 is not optional:
#   1. build BOTH eras and write their COMMIT files
#   2. sudo bash latt_calib.sh                  (one rep/arm/cell, s42, THE SMOKE)
#   3. commit the filled headroom table to goal-gate   (contract's completion)
#   4. sudo nohup bash latt_all.sh ... &        (this script)
#
# WATCHER NOTE: `pgrep -f latt_battery` matches the WATCHER'S OWN shell. Watch
# the SENTINEL, or the ledger's ERA-BATTERY-DONE line — never the process table.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement and it manufactures
# the seed-7 abort signature on seed 42 (measured 2026-08-07: 121 RUN-RETRY over
# 171 polled invocations against 0 over 80 unpolled). Launch this, then WAIT.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/latt
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-ALL"
if [ ! -f "$OUTDIR/latt-calib-s42.log" ]; then
  echo "WARNING: no calibration ledger at $OUTDIR/latt-calib-s42.log." >&2
  echo "         The contract's headroom table is filled by latt_calib.sh and" >&2
  echo "         committed BEFORE the scored run (MEASUREMENT DISCIPLINE 16)." >&2
  echo "         Set RWM_LATT_NO_CALIB=1 to proceed anyway and RECORD why." >&2
  [ "${RWM_LATT_NO_CALIB:-0}" = "1" ] || exit 4
fi
{
  echo "LATT-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "LATT-ALL OLD $(cat "${RWM_ERA_OLD_ROOT:-/home/vibe/era-old}/COMMIT" 2>/dev/null)"
  echo "LATT-ALL NEW $(cat "${RWM_ERA_NEW_ROOT:-/home/vibe/raptorpath}/COMMIT" 2>/dev/null)"
  echo "LATT-ALL CONTRACT goal-gate \"Latency Truth — PRE-REGISTRATION\" (MEASUREMENT TRUTH item 1)"
} > "$OUTDIR/all-latt.txt"
export RWM_ERA_OUTDIR="$OUTDIR"
export RWM_ERA_TAG=latt
export RWM_ERA_CELLS="${RWM_LATT_CELLS:-c7 c8 c8L}"
export RWM_ERA_ARMS="${RWM_LATT_ARMS:-OLD NEW}"
export RWM_ERA_SMALLREPS=8     # c7
export RWM_ERA_AUXREPS=0       # no NR arm
bash era_battery.sh 42 "${1:-12}"
bash era_battery.sh 7  "${1:-12}"
echo "LATT-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-latt.txt"
touch "$OUTDIR/DONE-ALL"
