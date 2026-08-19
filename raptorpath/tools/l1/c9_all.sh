#!/bin/bash
# THE c9 VALIDATION BATTERY — the full pre-registered matrix, launched detached.
#
#   sudo nohup bash c9_all.sh >/home/vibe/c9/all.out 2>&1 &
#
# THE MATRIX, verbatim from goal-gate's c9 contract §5: 2 cells x 2 arms x
# 2 seeds x 3 reps = 24 invocations.
#
#              | pooled (RWM_STORE_PERCAP=0) | percap (RWM_STORE_PERCAP=1)
#   c9         | C9-1, C9-2, C9-L1, C9-L2    | C9-2, C9-4
#   c9h        | C9-3, C9-L1, C9-L3          | C9-4
#
# THE LOOP ORDER IS THE PAIRED DESIGN, not a convenience. §5 requires that
# "the two arms of a pair run back to back at the same cell on the same
# binary", because C9-2 and C9-4 are scored as PAIRED differences within
# (seed, rep) and the c8-class arms in this tree are bistable — an unpaired
# mean over a bimodal population is a number with no cell behind it. So `arm`
# is the INNERMOST loop and nothing may be reordered above it.
#
# BOTH SEEDS, 42 and 7, each a BASE from which the legs derive (base+1000*i):
# 42 shapes 42/1042/2042/3042 and 7 shapes 7/1007/2007/3007 — eight distinct
# netem realizations across the two sessions, none shared between legs. c9 is
# a SYMMETRIC cell, which is exactly the shape the pre-2026-08-19 shared-seed
# defect pinned at rho_loss = +1 by construction; a c9 ledger whose -q.txt
# reads distinct_seeds = 1 is VOID, not a result.
#
# THE SENTINEL. `pgrep -f c9_all.sh` MATCHES THE WATCHER'S OWN SHELL and has
# cost this tree a battery before. Watch /home/vibe/c9/DONE — never the
# process table.
set -u
[ "$(id -u)" -eq 0 ] || { echo "must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
DDIR=/home/vibe/c9
mkdir -p "$DDIR"
rm -f "$DDIR/DONE"

{
  echo "C9-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "C9-ALL binary: $(sha256sum /home/vibe/raptorpath/target/release/raptorpath)"
  echo "C9-ALL source: $(cat /home/vibe/raptorpath/COMMIT)"
  echo "C9-ALL matrix: 2 cells x 2 arms x 2 seeds x 3 reps = 24 invocations"
  echo "C9-ALL arms run BACK TO BACK within (seed, rep) — the paired design of contract §5"
} | tee "$DDIR/all-progress.txt"

n=0
for SEED in 42 7; do
  for REP in 1 2 3; do
    for CELL in c9 c9h; do
      for ARM in pooled percap; do
        n=$((n + 1))
        echo "C9-ALL [$n/24] cell=$CELL arm=$ARM seed=$SEED rep=$REP $(date -u +%FT%TZ)" \
          | tee -a "$DDIR/all-progress.txt"
        bash c9_battery.sh "$CELL" "$ARM" "$REP" "$SEED"
        # A single invocation must never kill the battery: an abort is a
        # RECORDED OUTCOME with a witness cause, not a reason to stop. The
        # ledger carries the abort and the scorer counts it.
        rc=$?
        [ "$rc" -ne 0 ] && echo "C9-ALL [$n/24] driver rc=$rc (recorded, battery continues)" \
          | tee -a "$DDIR/all-progress.txt"
      done
    done
  done
done

{
  echo "C9-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg)"
  echo "C9-ALL invocations attempted: $n"
} | tee -a "$DDIR/all-progress.txt"

# The scorer, over BOTH seed ledgers. It SCORES; this file captures.
for S in 42 7; do
  [ -s "$DDIR/c9-w250-s${S}.log" ] || continue
  python3 ./eppen_quad.py --qdir "$DDIR" "$DDIR/c9-w250-s${S}.log" \
    > "$DDIR/c9-score-s${S}.txt" 2>&1 || true
done
python3 ./eppen_quad.py --qdir "$DDIR" "$DDIR"/c9-w250-s*.log \
  > "$DDIR/c9-score-bothseeds.txt" 2>&1 || true

# LAST LINE. Everything above must be on disk before the sentinel appears.
sync
touch "$DDIR/DONE"
echo "C9-ALL DONE $(date -u +%FT%TZ)" | tee -a "$DDIR/all-progress.txt"
