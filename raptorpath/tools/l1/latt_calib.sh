#!/bin/bash
# THE LATENCY-TRUTH BATTERY — THE DISCIPLINE-16 CALIBRATION PASS AND THE SMOKE.
#
#   sudo nohup bash latt_calib.sh >/home/vibe/latt/calib.out 2>&1 &
#
# goal-gate "Latency Truth — PRE-REGISTRATION" (MEASUREMENT TRUTH item 1) leaves
# its headroom table EMPTY on purpose and fixes the protocol instead. This is
# that pass: ONE rep per arm per cell, seed 42, SAME session, SAME TWO binaries,
# BEFORE the scored run, committed as the contract's completion in its own
# commit before `latt_all.sh` is launched.
#
#   util = tc_bytes * 8 / (TRANSFER seconds * shaped capacity)
#   THE DENOMINATOR IS THE TRANSFER WALL (`seconds`), NEVER `INVOCATION_S`.
#   headroom >= 5 % -> throughput targets permitted; < 5 % -> parity / latency
#   only. Where the calibration CONTRADICTS a permission, that clause is VOID
#   for that cell and is reported as void, never re-scoped after the fact.
#
# IT IS n = 1. It carries no sigma, no seed-7 evidence, and NOTHING IN IT IS A
# RESULT.
#
# ── WHAT THIS SMOKE IS LOOKING FOR, AND IT IS NOT WHAT THE ERA CALIBRATION
#    LOOKED FOR. That pass was proving the OLD binary would run at all. This one
#    is proving THE REPAIRED PROBE MEASURES BOTH LEGS, because the whole
#    question this battery exists to settle was mis-instrumented last time:
#
#   * `/tmp/rwm-ping-0.txt` AND `/tmp/rwm-ping-1.txt` EXIST AND ARE NON-EMPTY on
#     every dual invocation of BOTH arms. The era battery probed leg A only, on
#     asymmetric cells, and leg A is the FAST leg at c8.
#   * EVERY leg carries a CENSORING FRACTION, which means `ping` wrote its own
#     `N packets transmitted, M received` summary — i.e. the SIGINT reap worked.
#     A leg reporting `sent_source=max_icmp_seq(LOWER BOUND)` is the SIGTERM
#     defect still present and must be fixed before the scored run, not
#     annotated after it.
#   * THE CENSORING IS BELOW THE CONTRACT BAR at the cells that matter. The GE
#     floors alone are 2.53 % (c2 leg) and 4.76 % (c3 leg); the loaded qdisc
#     adds tail drops. If a leg reads > 20 % here, the contract's clause (iii)
#     is about to fire on the scored run and the reader is told NOW.
#   * THE ENGINE GAUGES ARE TWO-SIDED (`[DIAG]` on both endpoints, `q_p50`
#     present), because the adjudication needs BOTH instruments or it is not an
#     adjudication.
#   * THE ABORT-CAUSE WITNESS IS ARMED and the REPAIRED topo-ping holds: the era
#     battery resolved all 38 of its aborts to a two-packet no-retry sanity ping
#     across a GE-lossy leg, and `aw_ping` now retries to 26 draws. The expected
#     abort count here is ~0. A repeat of the era's 38/204 means the repair did
#     not take and the battery must not launch.
#
# WATCHER NOTE: `pgrep -f latt_calib.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/latt
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "LATT-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "LATT-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
  echo "LATT-CALIB THE INSTRUMENT UNDER SMOKE IS THE PER-LEG DELIVERED-LATENCY PROBE. Read the LATPROBE-LEG lines: both legs non-empty, censoring printed, sent_source=summary."
} > "$OUTDIR/calib-latt.txt"
RWM_ERA_OUTDIR="$OUTDIR" RWM_ERA_TAG=latt-calib \
  RWM_ERA_CELLS="${RWM_LATT_CELLS:-c7 c8 c8L}" RWM_ERA_ARMS="${RWM_LATT_ARMS:-OLD NEW}" \
  RWM_ERA_SMALLREPS=1 RWM_ERA_AUXREPS=0 \
  bash era_battery.sh 42 1
echo "LATT-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-latt.txt"
# `--calib` prints the invocation accounting, the abort-cause table, the
# anti-mix table, the per-era liveness audit and the headroom table, and SCORES
# NOTHING: no bar is scored from n = 1.
python3 ./era_report.py --calib "$OUTDIR/latt-calib-s42.log" \
  > "$OUTDIR/latt-calib-headroom.txt" 2>&1 || true
# THE PROBE READOUT, SEPARATED OUT, because it is the reason this pass exists
# and it must not have to be hunted for in the ledger.
{
  echo "=== PER-LEG DELIVERED-LATENCY PROBE — the smoke's own subject ==="
  grep -h "LATPROBE" "$OUTDIR/latt-calib-s42.log" 2>/dev/null || echo "(none — THE PROBE DID NOT RUN)"
  echo
  echo "=== INSTRUMENT-FAIL-PROBE lines (per leg; any line here is a half-measured cell) ==="
  grep -h "INSTRUMENT-FAIL-PROBE" "$OUTDIR/latt-calib-s42.log" 2>/dev/null || echo "(none)"
  echo
  echo "=== ABORTS (the repaired topo-ping expectation is ~0) ==="
  grep -hc "^ABORT " "$OUTDIR/latt-calib-s42.log" 2>/dev/null || echo 0
} > "$OUTDIR/latt-calib-probe.txt" 2>&1
touch "$OUTDIR/DONE-CALIB"
