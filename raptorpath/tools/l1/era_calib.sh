#!/bin/bash
# THE ERA BATTERY — THE DISCIPLINE-16 CALIBRATION PASS.
#
#   sudo nohup bash era_calib.sh >/home/vibe/era/calib.out 2>&1 &
#
# WHAT THIS IS FOR, and it is the one step of this battery that may not be
# skipped. goal-gate "Era Battery — PRE-REGISTRATION" carries the METHOD and the
# protocol for its headroom table and leaves the NUMBERS empty, on purpose:
#
#   1. BEFORE the scored run, in the SAME session, on the SAME TWO binaries: ONE
#      rep per arm per cell, seed 42, with `tc -s qdisc show` captured on EVERY
#      cell and EVERY invocation — not a subset, which is the omission item 16
#      exists to prevent.
#   2. util = tc_bytes * 8 / (TRANSFER seconds * shaped capacity). THE
#      DENOMINATOR IS THE TRANSFER WALL (`seconds`), NEVER `INVOCATION_S`: the
#      latter is the whole script's wall (namespace bring-up, netem setup, the
#      verification pings, teardown), runs 1.12-2.11x the transfer, and read c7
#      at 77.6% when the cell was at 96.9% — which would have LICENSED exactly
#      the unsatisfiable target discipline 16 forbids.
#   3. The result is committed as THE CONTRACT'S COMPLETION, in its own commit,
#      BEFORE the scored battery runs: headroom >= 5% -> throughput targets
#      permitted; < 5% -> parity / latency only.
#   4. Where the calibration CONTRADICTS a permission the affected clause is VOID
#      for that cell and reported as void, never re-scoped after the fact.
#
# IT IS n = 1. It carries no sigma, no seed-7 evidence, and NOTHING IN IT IS A
# RESULT.
#
# IT IS ALSO THE SMOKE, AND THIS BATTERY NEEDS IT MORE THAN ANY PREDECESSOR DID,
# for a reason none of them had: **the OLD binary has never been run by this
# harness in its current form.** `4171b584` predates the `[GATES]` echo, the
# `RWM_LATPROBE` delivered-latency probe, the sectioned `tc` capture, the
# `[GATES]`-scoped liveness greps, the `RWM_BIN` override and the abort-cause
# witness — every one of which was built against a NEWER engine. What the
# calibration is looking for, specifically:
#
#   * THE ERA-INVARIANT ANCHORS FIRE ON BOTH ENDPOINTS OF THE OLD ARM. If they
#     do not, G-LIVE has no signal at OLD and the battery has ONE arm. This is
#     the single most important line in the calibration output.
#   * G-ERA IS CLEAN: 0 `[GATES]` lines two-sided on OLD, >=1 two-sided on NEW.
#     A violation here means the two trees were not built where the driver
#     thinks they were, and it is cheaper to find at 22 invocations than at 204.
#   * THE OLD BINARY ACCEPTS THE HARNESS'S CLI. `--window-reliable`,
#     `--protocol-hint`, `--bind`, `--peer`, `--bytes`, `--runs` all exist at
#     `4171b584:raptorpath/src/main.rs` — READ from the source before this file
#     was written, but reading is not running.
#   * `[DIAG]` POPULATES ON OLD with `win=`, `rtt=/wrtt=/rtp`, `khr=` and `pl=`,
#     which are the shared columns every cross-era contrast is computed from.
#   * `[CTLD]` POPULATES ON BOTH ERAS. It is THE ONLY cross-era mechanism gauge
#     this battery has; without it P1 and P3 are inferred from goodput alone.
#   * THE ABORT-CAUSE WITNESS RECORDS SOMETHING. An abort with `no_record` is an
#     INSTRUMENT-FAIL of the instrument.
#
# Read the LIVENESS / ABORT / G-ERA-VIOLATION / ERA-SURPRISE / ERA-LIVENESS-FAIL
# lines of the ledger before launching era_all.sh.
#
# WATCHER NOTE: `pgrep -f era_calib.sh` matches the WATCHER'S OWN shell. Watch
# the SENTINEL — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/era
mkdir -p "$OUTDIR"
rm -f "$OUTDIR/DONE-CALIB"
{
  echo "ERA-CALIB start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "ERA-CALIB one rep per arm per cell, seed 42 — the discipline-16 headroom pass AND the smoke"
  echo "ERA-CALIB the OLD binary has NEVER been run by this harness in its current form; that is what this pass is for"
} > "$OUTDIR/calib-era.txt"
RWM_ERA_TAG=era-calib RWM_ERA_SMALLREPS=1 RWM_ERA_AUXREPS=1 \
  bash era_battery.sh 42 1
echo "ERA-CALIB end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/calib-era.txt"
# `--calib` prints the invocation accounting, the abort-cause table, the
# anti-mix table, the per-era liveness audit and the headroom table, and SCORES
# NOTHING: no bar is scored from n = 1.
python3 ./era_report.py --calib "$OUTDIR/era-calib-s42.log" \
  > "$OUTDIR/era-calib-headroom.txt" 2>&1 || true
touch "$OUTDIR/DONE-CALIB"
