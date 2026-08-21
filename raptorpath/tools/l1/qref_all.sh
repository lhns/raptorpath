#!/bin/bash
# THE (q, refresh) SWEEP — two seeds, one detached session, one EARNED and
# PROVABLY-WRITABLE sentinel.
#
#   nohup bash qref_all.sh >/home/vibe/qrefresh/all.out 2>&1 &
#
# THIS SCRIPT IS STARTED AS `vibe`, NOT AS ROOT, AND THAT IS THE POINT. It uses
# `sudo` for the battery invocations alone (the battery needs root for the
# namespaces) and does every sentinel operation as the UNPRIVILEGED user, so
# the sentinel writability it proves at launch is the writability the exit path
# will actually have.
#
# TWO SEEDS, AND IT IS THE CONTRACT RATHER THAN A SHORTCUT. This pass SCORES an
# ordered prediction (P-A) and a band prediction (P-B) against a stated
# falsifier, unlike the successor-arrival pass which characterized a
# distribution on one seed. P-B's falsifier is written as "out of band ON BOTH
# SEEDS" and cannot be evaluated on one.
#
# THE REP GRID IS UNEVEN ON PURPOSE, and it is echoed rather than buried:
#
#     cell   per seed   TOTAL across both seeds   why
#     c1        2               4                 400 MB single-path, the
#                                                 longest invocation in the
#                                                 grid; its self-heal p50 is
#                                                 also the tightest measured
#                                                 (24.6 ms, [18.4–26.6])
#     c7        4               8                 dual-path, lossy: the rep-to-
#     sc2       4               8                 rep dispersion of `rpd` is
#                                                 the quantity P-A is scored
#                                                 against, so the lossy cells
#                                                 buy reps
#
# Each cell is a SEPARATE battery invocation with its own `RWM_QREF_CELLS` and
# its own rep count, appending to the same per-seed ledger. That is why the
# per-cell rep count is explicit here instead of being a map inside the battery:
# the battery's `reps` argument means "reps of everything it is told to run",
# and a map hidden inside it would be a second place where the grid lives.
#
# DISCIPLINE 13, RESTATED BECAUSE IT IS MEASURED AND NOT ADVISORY: polling a
# running battery is co-tenancy on the box under measurement, and it
# manufactures the abort signature it is looking for (measured 2026-08-07: 121
# RUN-RETRY over 171 polled invocations against 0 over 80 unpolled ones).
# Launch this, then WAIT. Collect once, at the end.
#
# WATCHER NOTE: `pgrep -f qref_battery.sh` matches the WATCHER'S OWN shell
# whenever its command line contains the string. Watch the SENTINEL, or the
# ledger's QREFRESH-BATTERY-DONE line — never the process table.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUTDIR=/home/vibe/qrefresh
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
  rm -f "$OUTDIR/qref-s$s.log" "$OUTDIR/qref-witness-s$s.jsonl"
done

# A SENTINEL IS EARNED, NOT UNCONDITIONAL. MEASURED, 2026-08-21: sigb_calib.sh's
# first invocation hit a CRLF trap in lib.sh, ran ZERO invocations, and still
# wrote its DONE sentinel — because the `touch` was the script's last line and
# nothing else. An unconditional `touch` converts a total failure into a
# clean-looking success. The ledger must EXIST, be NON-EMPTY, and carry the
# battery's own terminal line for the sentinel to be earned.
seed_done() {
  local s="$1" f="$OUTDIR/qref-s$1.log"
  if [ -s "$f" ] && grep -q "QREFRESH-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "QREF-ALL seed $s DID NOT COMPLETE — no QREFRESH-BATTERY-DONE in $f" \
    | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

# cell -> reps PER SEED. Echoed below, so the grid is readable from the run's
# own output and not only from this file.
cell_reps() { case "$1" in c1) echo 2 ;; c7) echo 4 ;; sc2) echo 4 ;; *) echo 0 ;; esac; }
CELLS="c1 c7 sc2"

echo "QREF-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" > "$OUTDIR/all-era.txt"
echo "QREF-ALL seeds: $SEEDS"
for c in $CELLS; do
  echo "QREF-ALL grid: cell=$c reps_per_seed=$(cell_reps "$c") reps_total=$(( $(cell_reps "$c") * 2 ))"
done

for s in $SEEDS; do
  for c in $CELLS; do
    r=$(cell_reps "$c")
    echo "QREF-ALL invoke seed=$s cell=$c reps=$r $(date -u +%FT%TZ)"
    # `sudo` HERE AND NOWHERE ELSE: the battery needs root for the rp-*
    # namespaces; every sentinel path above and below is touched as `vibe`.
    sudo env RWM_QREF_CELLS="$c" bash qref_battery.sh "$s" "$r"
  done
  seed_done "$s"
done
echo "QREF-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)" >> "$OUTDIR/all-era.txt"

ALL_OK=1
for s in $SEEDS; do
  [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0
done

if [ "$ALL_OK" -eq 1 ]; then
  touch "$OUTDIR/DONE-ALL"
  echo QREF-ALL-DONE
else
  touch "$OUTDIR/FAILED-ALL"
  echo QREF-ALL-FAILED
  exit 5
fi
