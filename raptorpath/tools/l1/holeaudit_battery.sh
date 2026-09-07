#!/bin/bash
# D0 THE ATTRIBUTION AUDIT — the L1 battery.
# Scored ONLY against "THE ATTRIBUTION AUDIT (D0) — PRE-REGISTRATION".
# One arm: shipped defaults, RWM_GEN=0, RWM_DIAG=1 RWM_FDIAG=1, seed 42.
#   usage: holeaudit_battery.sh <reps>
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
REPS="${1:-3}"
SEED=42
BIN=/home/vibe/raptorpath/target/release/raptorpath
RUN=/home/vibe/holeaudit
OUT=$RUN/battery-s${SEED}.log
DDIR=$RUN/diag
mkdir -p "$DDIR"
: > "$OUT"
echo "# holeaudit battery $(date -u +%FT%TZ) seed=$SEED reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"

# Sentinel: writability PROVEN, by the unprivileged user, at LAUNCH.
SENT=$RUN/.sentinel-earned
if ! echo "earned $(date -u +%FT%TZ)" > "$SENT"; then
  echo "ABORT-SENTINEL-UNWRITABLE" >> "$OUT"; exit 3
fi
echo "SENTINEL ok $(cat $SENT)" >> "$OUT"

run_one() { # name scenA scenB mode bytes rep
  local name="$1" a="$2" b="$3" mode="$4" bytes="$5" rep="$6"
  echo "=== cell=$name rep=$rep seed=$SEED cells=$a/$b/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED RWM_GEN=0 RWM_DIAG=1 RWM_FDIAG=1 \
      bash perf_rwm_c.sh "$a" "$b" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|ABORT" >> "$OUT" || true
  echo "RC=$?" >> "$OUT"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log
  # CONTAMINATION witnesses: every rival clock must be absent on BOTH ends.
  for g in RWM_HOLDDOWN_Q RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS RWM_DERIVED_SWEEP \
           RWM_ALPHA_OVERRIDE RWM_W_FORM RWM_REFRESH_FLOOR_US; do
    echo "GATE $name r$rep $g cli=$(grep -c "$g=" $C 2>/dev/null || echo 0) srv=$(grep -c "$g=" $S 2>/dev/null || echo 0)" >> "$OUT"
  done
  # THE SENDER GAUGE — every [HOLD] line, both ends.
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[HOLD\]' | sed "s/^/HOLD $name r$rep cli /" >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' $S | grep '\[HOLD\]' | sed "s/^/HOLD $name r$rep srv /" >> "$OUT") || true
  # THE RECEIVER GAUGE — the LAST (cumulative) [SUCC], both ends.
  (sed 's/\x1b\[[0-9;]*m//g' $S | grep '\[SUCC\] ' | tail -1 | sed "s/^/SUCC $name r$rep srv /" >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[SUCC\] ' | tail -1 | sed "s/^/SUCC $name r$rep cli /" >> "$OUT") || true
  # THE FIRE-CAUSE and the WASTE SPLIT.
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[FCAUSE\]' | tail -1 | sed "s/^/FCAUSE $name r$rep cli /" >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[RFA\]' | tail -1 | sed "s/^/RFA $name r$rep cli /" >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' $S | grep '\[RFA\]' | tail -1 | sed "s/^/RFA $name r$rep srv /" >> "$OUT") || true
  # THE DIAG TAIL: taper=, retx=, mpr[..], and the cumulative source symbols
  # (the `det <= cum source symbols` witness).
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[DIAG\]' | tail -1 \
    | grep -oE "cum=[0-9/]+|retx=[0-9]+|sweeps=[0-9]+|gapdrop=[0-9]+|mpr\[[^]]*\]" \
    | tr '\n' ' ' | sed "s/^/DIAG $name r$rep cli /" >> "$OUT") || true
  echo >> "$OUT"
  # THE CONTROL-DATAGRAM DENSITY, both ends (the eviction arithmetic's inputs).
  (sed 's/\x1b\[[0-9;]*m//g' $C | grep '\[CTLD\]' | tail -1 | sed "s/^/CTLD $name r$rep cli /" >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' $S | grep '\[CTLD\]' | tail -1 | sed "s/^/CTLD $name r$rep srv /" >> "$OUT") || true
  # GEN WITNESS FIRST, then the band.
  echo "GENW $name r$rep succ_gen=$(sed 's/\x1b\[[0-9;]*m//g' $S | grep -o '\[SUCC\] gen=[01]' | tail -1) fc_gen=$(sed 's/\x1b\[[0-9;]*m//g' $C | grep -o '\[FCAUSE\] gen=[01]' | tail -1)" >> "$OUT"
  cp $C "$DDIR/${name}-r${rep}-c.log" 2>/dev/null || true
  cp $S "$DDIR/${name}-r${rep}-s.log" 2>/dev/null || true
  echo >> "$OUT"
}

for rep in $(seq 1 "$REPS"); do
  run_one c1  c1 c1 single 400000000 "$rep"
  run_one c7  c2 c2 dual   200000000 "$rep"
  run_one sc2 c2 c2 single 100000000 "$rep"
  run_one c8  c2 c3 dual    25000000 "$rep"
done

echo "# DONE $(date -u +%FT%TZ)" >> "$OUT"
echo "done $(date -u +%FT%TZ)" > "$SENT"
