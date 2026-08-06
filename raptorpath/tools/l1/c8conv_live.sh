#!/bin/bash
# feat/c8-conversion SUPPLEMENTAL: RWM_RECOV_MP_LIVE attribution arms — the
# recovery-liveness lever ALONE (no slack law) vs the pbs control, c8 + c7.
# Motivated by the main battery: the composed fixlive arm repaired half of
# pbs's c8 deficit (70.5 -> 82.7, sigma 14.6 -> 5.7 at s42) while the slack
# component was refuted — this isolates the live lever for its own
# inert-or-better-everywhere flip gate.
#   usage: c8conv_live.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-6}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8conv/live-s${SEED_ARG}.log
DDIR=/home/vibe/c8conv/diag
mkdir -p "$DDIR" /home/vibe/c8conv
: > "$OUT"
echo "# c8conv live-supplemental $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"

run_one() { # name envs cellA cellB mode bytes exp_live
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6" elive="$7"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  local sr live
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  live=$(grep -c "recovery clocks on LIVE paths ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr live=$live/$elive" >> "$OUT"
  if [ "$sr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-SR $name rep=$REP" >> "$OUT"; fi
  if [ "$elive" -gt 0 ] && [ "$live" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-live $name rep=$REP" >> "$OUT"; fi
  if [ "$elive" -eq 0 ] && [ "$live" -gt 0 ]; then echo "ARM-CONTAMINATION-live $name rep=$REP" >> "$OUT"; fi
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -oE 'mpr\[[^]]*\]' | tail -1 | sed 's/^/MPR /' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[C8CONV-S\]' | tail -1 | sed 's/^/SENDER  /' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  run_one c8-pbs2   ""                    c2 c3 dual 25000000  0
  run_one c8-live   "RWM_RECOV_MP_LIVE=1" c2 c3 dual 25000000  1
  run_one c7-pbs2   ""                    c2 c2 dual 200000000 0
  run_one c7-live   "RWM_RECOV_MP_LIVE=1" c2 c2 dual 200000000 1
done
echo "--- ARMCOUNTS" >> "$OUT"
for a in c8-pbs2 c8-live c7-pbs2 c7-live; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
done
echo "LIVE-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo LIVE-DONE-$SEED_ARG
