#!/bin/bash
# feat/bbr-default-and-store-release L1 battery: RWM_STORE_SACK_RELEASE
# (SACK-clocked store release, goal-gate "SACK-Clocked Store Release"
# pre-registration) x RWM_RECOV_MP, on the best-c7 config
# (PBS = plain + BBR-default (env unset, post Default-CC-Flip) +
# RWM_STORE_PATHS=1).
#
# Arms interleaved round-robin per rep, 1 run/invocation, fresh tunnel per
# invocation, RWM_DIAG=1 everywhere, per-arm liveness echo asserted
# (MEASUREMENT DISCIPLINE 1/7). Cells:
#   c7  PBS x {-,-SR,-MP,-SR-MP} — the Sigma-gap cell (4 arms)
#   c8  PBS x {-,-SR,-MP,-SR-MP} — the asymmetric regression watch (4 arms)
#   dc1 PB  x {-,-SR-MP}         — the control cell (+ same-session single)
#   sc1 PB                        — dual-c1's same-session single reference
#   sc2 PBS x {-,-SR}            — c7's Sigma term + N=1 inert-or-better check
#   sc3 PBS x {-,-SR}            — c8's slow Sigma term + N=1 check
#
#   usage: sackrel_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/sackrel/battery-s${SEED_ARG}.log
DDIR=/home/vibe/sackrel/diag
mkdir -p "$DDIR" /home/vibe/sackrel
: > "$OUT"
echo "# sackrel battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

# BBR is the shipped default post-flip: PBS arms set NO RWM_QUIC_CC.
PBS="RWM_STORE_PATHS=1"
PB=""
SR="RWM_STORE_SACK_RELEASE=1"
MP="RWM_RECOV_MP=1"

run_one() { # name envs cellA cellB mode bytes expect_sr(0/1) expect_mp(0/1)
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6" expsr="$7" expmp="$8"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # Liveness (discipline 1): SR + MP echoes must match the arm's expectation.
  local sr mp pbs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  mp=$(grep -c "multipath recovery suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr expect_sr=$expsr mp=$mp expect_mp=$expmp pbs=$pbs" >> "$OUT"
  if [ "$expsr" -gt 0 ] && [ "$sr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL $name rep=$REP" >> "$OUT"; fi
  if [ "$expsr" -eq 0 ] && [ "$sr" -gt 0 ]; then echo "ARM-CONTAMINATION $name rep=$REP" >> "$OUT"; fi
  if [ "$expmp" -gt 0 ] && [ "$mp" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-MP $name rep=$REP" >> "$OUT"; fi
  if [ "$expmp" -eq 0 ] && [ "$mp" -gt 0 ]; then echo "ARM-CONTAMINATION-MP $name rep=$REP" >> "$OUT"; fi
  # Dwell/occupancy gauges (the pre-registered mechanism evidence): last
  # cumulative DIAG line -> store occupancy (win=), SACK-released count
  # (srel=), retx/cod waste, pause duty.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|srel=[0-9]+/[0-9]+|paused=[0-9.]+%|sweeps=[0-9]+|retx=[0-9]+|gapdrop=[0-9]+|xattr=[0-9]+/[0-9]+|pl=[0-9.]+" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^src=/){gsub(/src=|sym\/s/,"",$i);s+=$i;n++}; if($i~/^cod=/){gsub(/cod=|sym\/s/,"",$i);c+=$i}}} END{if(n>0) printf "RATES mean_src=%.0f mean_cod=%.0f cod_share=%.3f\n", s/n, c/n, (s>0)?c/s:0; else print "RATES no-diag"}' >> "$OUT") || true
  # Mean store occupancy across DIAG samples (dwell gauge before/after).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap+=a[2];n++}; if($i~/^srel=/){split(substr($i,6),b,"/");r+=b[1];m++}}} END{if(n>0) printf "OCC mean_win=%.0f mean_cap=%.0f mean_srel=%.0f\n", w/n, cap/n, (m>0)?r/m:0; else print "OCC no-diag"}' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  run_one sc2-pbs        "$PBS"          c2 c2 single 100000000 0 0
  run_one sc2-pbs-sr     "$PBS $SR"      c2 c2 single 100000000 1 0
  run_one sc3-pbs        "$PBS"          c3 c3 single 25000000  0 0
  run_one sc3-pbs-sr     "$PBS $SR"      c3 c3 single 25000000  1 0
  run_one c7-pbs         "$PBS"          c2 c2 dual   200000000 0 0
  run_one c7-pbs-sr      "$PBS $SR"      c2 c2 dual   200000000 1 0
  run_one c7-pbs-mp      "$PBS $MP"      c2 c2 dual   200000000 0 1
  run_one c7-pbs-sr-mp   "$PBS $SR $MP"  c2 c2 dual   200000000 1 1
  run_one c8-pbs         "$PBS"          c2 c3 dual   25000000  0 0
  run_one c8-pbs-sr      "$PBS $SR"      c2 c3 dual   25000000  1 0
  run_one c8-pbs-mp      "$PBS $MP"      c2 c3 dual   25000000  0 1
  run_one c8-pbs-sr-mp   "$PBS $SR $MP"  c2 c3 dual   25000000  1 1
  run_one sc1-pb         "$PB"           c1 c1 single 400000000 0 0
  run_one dc1-pb         "$PB"           c1 c1 dual   400000000 0 0
  run_one dc1-pb-sr-mp   "$PB $SR $MP"   c1 c1 dual   400000000 1 1
done

# Arm-liveness assertion (discipline 7): every arm must have produced a
# summary per rep — an arm with zero summaries fails LOUDLY.
echo "--- ARMCOUNTS (expect $REPS summaries per arm)" >> "$OUT"
for a in sc2-pbs sc2-pbs-sr sc3-pbs sc3-pbs-sr c7-pbs c7-pbs-sr c7-pbs-mp c7-pbs-sr-mp c8-pbs c8-pbs-sr c8-pbs-mp c8-pbs-sr-mp sc1-pb dc1-pb dc1-pb-sr-mp; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
