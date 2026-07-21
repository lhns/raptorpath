#!/bin/bash
# feat/consolidation (roadmap item 2) L1 battery: the COMPOSED DEFAULT STACK
# vs current-shipped vs LEAVE-ONE-OUT ablations — the strictly-better proof
# for each candidate default member (goal-gate "CONSOLIDATED VERDICT" roadmap;
# plan: consolidation composed-stack battery).
#
# Arms (BBR + RWM_STORE_SACK_RELEASE are the shipped defaults, present in
# EVERY arm; RWM_GEN=0 plain mode, RWM_DIAG=1 everywhere):
#   ship     = env unset (the current shipped defaults)
#   stack    = RWM_STORE_PATHS=1 RWM_RECOV_MP=1 RWM_MSTAR_ANCHOR=1 RWM_CLOCK_GAP=1
#   loo-pbs  = stack minus RWM_STORE_PATHS
#   loo-mp   = stack minus RWM_RECOV_MP
#   loo-ms   = stack minus RWM_MSTAR_ANCHOR
#   loo-gap  = stack minus RWM_CLOCK_GAP
#   stack-rs = stack + RWM_PLAIN_RS=1  (c8 ONLY: the witness-cost-in-composition probe)
#
# Cells (interleaved round-robin per rep, 1 run/invocation, fresh tunnel per
# invocation):
#   c7  (c2+c2 dual, 200 MB)  x 6 arms — the Sigma-gap cell
#   c8  (c2+c3 dual, 25 MB)   x 7 arms — the asymmetric cell (+ the RS probe)
#   dc1 (c1+c1 dual, 400 MB)  x 6 arms — the anti-scaling control
#   sc1 (c1 single, 400 MB)   x 2 arms (ship, stack) — dc1's single reference
#   sc2 (c2 single, 100 MB)   x 5 arms — Sigma terms (loo-pbs == stack at N=1:
#   sc3 (c3 single, 25 MB)    x 5 arms    STORE_PATHS is N>=2-gated, skipped)
#
# Liveness (MEASUREMENT DISCIPLINE items 1/6/7): per-arm expected-echo
# assertion for SR (default ON everywhere), PBS, MP, MS (plain-live subset
# echo), GAP; contamination asserted in both directions; ARMCOUNT per arm.
#
#   usage: consol_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/consol/battery-s${SEED_ARG}.log
DDIR=/home/vibe/consol/diag
mkdir -p "$DDIR" /home/vibe/consol
: > "$OUT"
echo "# consol battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

PBS="RWM_STORE_PATHS=1"
MP="RWM_RECOV_MP=1"
MS="RWM_MSTAR_ANCHOR=1"
GAP="RWM_CLOCK_GAP=1"
RS="RWM_PLAIN_RS=1"

STACK="$PBS $MP $MS $GAP"
LOO_PBS="$MP $MS $GAP"
LOO_MP="$PBS $MS $GAP"
LOO_MS="$PBS $MP $GAP"
LOO_GAP="$PBS $MP $MS"

run_one() { # name envs cellA cellB mode bytes exp_pbs exp_mp exp_ms exp_gap exp_rs
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6"
  local epbs="$7" emp="$8" ems="$9" egap="${10}" ers="${11}"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # Liveness (discipline 1/6): every stack member's echo must match the arm.
  local sr pbs mp ms gap rs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  mp=$(grep -c "multipath recovery suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  ms=$(grep -c "peer-report RTT-feed suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  gap=$(grep -c "clock-gap estimator hygiene ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  rs=$(grep -c "send-interval SAMPLER ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs/$epbs mp=$mp/$emp ms=$ms/$ems gap=$gap/$egap rs=$rs/$ers" >> "$OUT"
  if [ "$sr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-SR $name rep=$REP (SR is the shipped default)" >> "$OUT"; fi
  local v e tag
  for tag in "pbs:$pbs:$epbs" "mp:$mp:$emp" "ms:$ms:$ems" "gap:$gap:$egap" "rs:$rs:$ers"; do
    v=$(echo "$tag" | cut -d: -f2); e=$(echo "$tag" | cut -d: -f3)
    if [ "$e" -gt 0 ] && [ "$v" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-${tag%%:*} $name rep=$REP" >> "$OUT"; fi
    if [ "$e" -eq 0 ] && [ "$v" -gt 0 ]; then echo "ARM-CONTAMINATION-${tag%%:*} $name rep=$REP" >> "$OUT"; fi
  done
  # Occupancy / recovery gauges (the sackrel parsing, same fields).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|srel=[0-9]+/[0-9]+|paused=[0-9.]+%|retx=[0-9]+|pl=[0-9.]+" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^src=/){gsub(/src=|sym\/s/,"",$i);s+=$i;n++}; if($i~/^cod=/){gsub(/cod=|sym\/s/,"",$i);c+=$i}}} END{if(n>0) printf "RATES mean_src=%.0f mean_cod=%.0f cod_share=%.3f\n", s/n, c/n, (s>0)?c/s:0; else print "RATES no-diag"}' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap+=a[2];n++}; if($i~/^srel=/){split(substr($i,6),b,"/");r+=b[1];m++}}} END{if(n>0) printf "OCC mean_win=%.0f mean_cap=%.0f mean_srel=%.0f\n", w/n, cap/n, (m>0)?r/m:0; else print "OCC no-diag"}' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  #      name          envs        cA cB  mode   bytes      pbs mp ms gap rs
  # -- c7 (the Sigma-gap cell) --
  run_one c7-ship      ""          c2 c2 dual   200000000   0  0  0  0  0
  run_one c7-stack     "$STACK"    c2 c2 dual   200000000   1  1  1  1  0
  run_one c7-loo-pbs   "$LOO_PBS"  c2 c2 dual   200000000   0  1  1  1  0
  run_one c7-loo-mp    "$LOO_MP"   c2 c2 dual   200000000   1  0  1  1  0
  run_one c7-loo-ms    "$LOO_MS"   c2 c2 dual   200000000   1  1  0  1  0
  run_one c7-loo-gap   "$LOO_GAP"  c2 c2 dual   200000000   1  1  1  0  0
  # -- c8 (asymmetric; + the PLAIN_RS composition probe) --
  run_one c8-ship      ""          c2 c3 dual   25000000    0  0  0  0  0
  run_one c8-stack     "$STACK"    c2 c3 dual   25000000    1  1  1  1  0
  run_one c8-loo-pbs   "$LOO_PBS"  c2 c3 dual   25000000    0  1  1  1  0
  run_one c8-loo-mp    "$LOO_MP"   c2 c3 dual   25000000    1  0  1  1  0
  run_one c8-loo-ms    "$LOO_MS"   c2 c3 dual   25000000    1  1  0  1  0
  run_one c8-loo-gap   "$LOO_GAP"  c2 c3 dual   25000000    1  1  1  0  0
  run_one c8-stack-rs  "$STACK $RS" c2 c3 dual  25000000    1  1  1  1  1
  # -- dual-c1 (anti-scaling control) --
  run_one dc1-ship     ""          c1 c1 dual   400000000   0  0  0  0  0
  run_one dc1-stack    "$STACK"    c1 c1 dual   400000000   1  1  1  1  0
  run_one dc1-loo-pbs  "$LOO_PBS"  c1 c1 dual   400000000   0  1  1  1  0
  run_one dc1-loo-mp   "$LOO_MP"   c1 c1 dual   400000000   1  0  1  1  0
  run_one dc1-loo-ms   "$LOO_MS"   c1 c1 dual   400000000   1  1  0  1  0
  run_one dc1-loo-gap  "$LOO_GAP"  c1 c1 dual   400000000   1  1  1  0  0
  # -- singles (same-session Sigma terms; loo-pbs == stack at N=1, skipped) --
  run_one sc1-ship     ""          c1 c1 single 400000000   0  0  0  0  0
  run_one sc1-stack    "$STACK"    c1 c1 single 400000000   1  1  1  1  0
  run_one sc2-ship     ""          c2 c2 single 100000000   0  0  0  0  0
  run_one sc2-stack    "$STACK"    c2 c2 single 100000000   1  1  1  1  0
  run_one sc2-loo-mp   "$LOO_MP"   c2 c2 single 100000000   1  0  1  1  0
  run_one sc2-loo-ms   "$LOO_MS"   c2 c2 single 100000000   1  1  0  1  0
  run_one sc2-loo-gap  "$LOO_GAP"  c2 c2 single 100000000   1  1  1  0  0
  run_one sc3-ship     ""          c3 c3 single 25000000    0  0  0  0  0
  run_one sc3-stack    "$STACK"    c3 c3 single 25000000    1  1  1  1  0
  run_one sc3-loo-mp   "$LOO_MP"   c3 c3 single 25000000    1  0  1  1  0
  run_one sc3-loo-ms   "$LOO_MS"   c3 c3 single 25000000    1  1  0  1  0
  run_one sc3-loo-gap  "$LOO_GAP"  c3 c3 single 25000000    1  1  1  0  0
done

# Arm-liveness assertion (discipline 7): an arm with zero summaries fails
# LOUDLY, it does not vanish.
echo "--- ARMCOUNTS (expect $REPS headers per arm)" >> "$OUT"
for a in c7-ship c7-stack c7-loo-pbs c7-loo-mp c7-loo-ms c7-loo-gap \
         c8-ship c8-stack c8-loo-pbs c8-loo-mp c8-loo-ms c8-loo-gap c8-stack-rs \
         dc1-ship dc1-stack dc1-loo-pbs dc1-loo-mp dc1-loo-ms dc1-loo-gap \
         sc1-ship sc1-stack sc2-ship sc2-stack sc2-loo-mp sc2-loo-ms sc2-loo-gap \
         sc3-ship sc3-stack sc3-loo-mp sc3-loo-ms sc3-loo-gap; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
