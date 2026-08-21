#!/bin/bash
# THE VERDICT BATTERY — goal #101 item 4 — two arms, four cells, two seeds, one
# detached session, one EARNED and PROVABLY-WRITABLE sentinel.
#
#   nohup bash verdict_all.sh >/home/vibe/verdict/all.out 2>&1 &
#
# THIS SCRIPT IS STARTED AS `vibe`, NOT AS ROOT, AND THAT IS THE POINT. It uses
# `sudo` for the battery invocations alone (the battery needs root for the
# namespaces) and does every sentinel operation as the UNPRIVILEGED user, so
# the sentinel writability it proves at launch is the writability the exit path
# will actually have.
#
# TWO SEEDS, AND IT IS THE CONTRACT RATHER THAN A SHORTCUT. Every clause of the
# pre-registration is written "on both seeds"; none of them can be evaluated on
# one. A seed is a netem loss realisation, not a repetition.
#
# THE REP GRID, WITH ITS ARITHMETIC ECHOED RATHER THAN BURIED:
#
#     cell   per seed   TOTAL   why
#     c1        2         4     400 MB single-path, the longest invocation in
#                              the grid; the (q, refresh) sweep's own CTL rep
#                              range there was [197, 210] on n = 4 (CV 3.0%)
#     c7        4         8     the challenger's HOME cell — n = 8 is the n at
#                              which its measured effect was read, so this
#                              reproduces it rather than re-sizes it
#     sc2       4         8     CV 0.8%, but the (q, refresh) sweep read every
#                              treatment arm OUT of band here; a mixture is not
#                              sized by its control's dispersion
#     c8        4         8     the LOSS-HEAVY WITNESS cell. Reported, never a
#                              vote — see the pre-registration.
#
#     2 arms x (4 + 8 + 8 + 8) = 56 INVOCATIONS.
#
# Each cell is a SEPARATE battery invocation with its own `RWM_VRD_CELLS` and
# its own rep count, appending to the same per-seed ledger.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
#
# WATCHER NOTE: `pgrep -f verdict_battery.sh` matches the WATCHER'S OWN shell
# whenever its command line contains the string. Watch the SENTINEL, or the
# ledger's VERDICT-BATTERY-DONE line — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/verdict
SEEDS="42 7"

# ── SENTINEL WRITABILITY IS PROVEN AT LAUNCH, NOT DISCOVERED AT EXIT ──────
#
# THE RECORDED DEFECT THIS CLOSES. The hold-down sweep's launcher ran to
# completion and wrote NO sentinel at all: the output directory existed and was
# owned by ROOT, the launcher's `touch` ran as the UNPRIVILEGED user, and the
# script carried `set -uo pipefail` WITHOUT `-e`. So the `touch` failed, the
# failure was not fatal, nothing printed, and the watcher — which discipline 13
# tells to watch the SENTINEL and not the process table — waited on a file that
# could never appear, for a battery that had already finished. A root-owned
# directory plus an unprivileged write plus a non-fatal failure is a SILENTLY
# UNWRITTEN SENTINEL, and it converts a completed pass into an indefinite wait
# at exactly the moment nobody is looking.
#
# THE FIX IS NOT `set -e` AND IT IS NOT A LOUDER `touch`. It is to PROVE the
# write, as the user who will perform it, on the exact absolute paths, BEFORE
# any measurement is taken — because the only cheap moment to discover an
# unwritable sentinel is the moment before a multi-hour run, and the only
# expensive one is after it.
mkdir -p "$OUTDIR" 2>/dev/null

# Probe the PATH, not the directory: a directory can be writable while a stale
# root-owned file at the exact path is not, and it is the PATH the exit code
# will write. Write AND unlink, because a sentinel that cannot be REMOVED at
# the next launch is a sentinel that reports the PREVIOUS run's verdict.
probe() {
  local p="$1"
  if : > "$p.probe" 2>/dev/null && rm -f "$p.probe" 2>/dev/null; then
    echo "SENTINEL-WRITABLE $p (probed as $(id -un), write+unlink)"
    return 0
  fi
  echo "ABORT-SENTINEL-UNWRITABLE $p"
  echo "ABORT-SENTINEL-UNWRITABLE dir=$OUTDIR owner=$(stat -c '%U:%G %a' "$OUTDIR" 2>/dev/null) user=$(id -un)"
  echo "NOTHING WAS RUN. Fix the ownership of $OUTDIR and relaunch: a pass whose sentinel cannot be written is a pass whose completion cannot be observed."
  exit 3
}

# `all.out` IS PROVED FIRST AND THE TEE IS OPENED ONLY AFTERWARDS. Opening the
# transcript before proving it would send the abort message that explains the
# failure into the file the failure is about.
probe "$OUTDIR/all.out"
exec > >(tee -a "$OUTDIR/all.out") 2>&1

for s in $SEEDS; do probe "$OUTDIR/DONE-S$s"; probe "$OUTDIR/FAILED-S$s"; done
probe "$OUTDIR/DONE-ALL"
probe "$OUTDIR/FAILED-ALL"
probe "$OUTDIR/all-era.txt"
echo "SENTINEL-PROOF-COMPLETE $(date -u +%FT%TZ) user=$(id -un) dir=$OUTDIR"

rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/FAILED-ALL" \
      "$OUTDIR/DONE-S42" "$OUTDIR/FAILED-S42" \
      "$OUTDIR/DONE-S7"  "$OUTDIR/FAILED-S7"

# THE LEDGERS ARE CLEARED WITH THE SENTINELS, AND FOR THE SAME REASON. The
# battery opens its ledger with `>>` on purpose — three per-cell invocations
# per seed append to one file — but that also means a RELAUNCH appends to the
# previous era's rows. `ARMCOUNT` counts by grepping the ledger, so a stale era
# would be counted into this one and every arm would over-report its `rows=`.
# Worse, `seed_done()` greps for the terminal line: a ledger left over from a
# completed previous run would EARN a sentinel for a run that never happened,
# which is precisely the failure the earned-sentinel rule exists to prevent.
# Clearing here rather than inside the battery keeps the battery's own contract
# ("append, so the cells compose") intact.
for s in $SEEDS; do
  rm -f "$OUTDIR/vrd-s$s.log" "$OUTDIR/vrd-witness-s$s.jsonl"
done

# A SENTINEL IS EARNED, NOT UNCONDITIONAL. MEASURED, 2026-08-21: sigb_calib.sh's
# first invocation hit a CRLF trap in lib.sh, ran ZERO invocations, and still
# wrote its DONE sentinel — because the `touch` was the script's last line and
# nothing else. An unconditional `touch` converts a total failure into a
# clean-looking success. The ledger must EXIST, be NON-EMPTY, and carry the
# battery's own terminal line for the sentinel to be earned.
seed_done() {
  local s="$1" f="$OUTDIR/vrd-s$1.log"
  if [ -s "$f" ] && grep -q "VERDICT-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "VRD-ALL seed $s DID NOT COMPLETE — no VERDICT-BATTERY-DONE in $f" \
    | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

# cell -> reps PER SEED. Echoed below, so the grid is readable from the run's
# own output and not only from this file.
cell_reps() { case "$1" in c1) echo 2 ;; c7) echo 4 ;; sc2) echo 4 ;; c8) echo 4 ;; *) echo 0 ;; esac; }
CELLS="c1 c7 sc2 c8"

echo "VRD-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
echo "VRD-ALL seeds: $SEEDS"
for c in $CELLS; do
  echo "VRD-ALL grid: cell=$c reps_per_seed=$(cell_reps "$c") reps_total=$(( $(cell_reps "$c") * 2 ))"
done

for s in $SEEDS; do
  for c in $CELLS; do
    r=$(cell_reps "$c")
    echo "VRD-ALL invoke seed=$s cell=$c reps=$r $(date -u +%FT%TZ)"
    # `sudo` HERE AND NOWHERE ELSE: the battery needs root for the rp-*
    # namespaces; every sentinel path above and below is touched as `vibe`.
    sudo env RWM_VRD_CELLS="$c" bash verdict_battery.sh "$s" "$r"
  done
  seed_done "$s"
done
echo "VRD-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"

ALL_OK=1
for s in $SEEDS; do
  [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0
done

if [ "$ALL_OK" -eq 1 ]; then
  touch "$OUTDIR/DONE-ALL"
  echo VRD-ALL-DONE
else
  touch "$OUTDIR/FAILED-ALL"
  echo VRD-ALL-FAILED
  exit 5
fi
