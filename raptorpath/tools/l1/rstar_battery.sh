#!/bin/bash
# Task #46 (r* bursty-loss provisioning, paper 8.4.1) L1 spot check.
# ONE cell where the fix matters: REALTIME hint on single-path c3 (GE 2/40,
# eps ~4.8%, mean burst 2.5 — the standard bursty cell), plain window mode
# (RWM_GEN=0: the r*-consuming taper-repair path), same-binary interleaved A/B:
#   T = RWM_RSTAR_TAIL=1  (shipped: window-mass tail-provisioned r*)
#   L = RWM_RSTAR_TAIL=0  (legacy GE-only closed-form r*)
# DELIVERED-RELIABILITY observable: realtime's reorder horizon (20 ms) is far
# below the c3 ARQ round (~90 ms RTT), so any loss NOT recovered in-window
# (FEC) is force-delivered as a HOLE at the app and the perf object can never
# complete -> DNF. Objects are SMALL (100 KB ~ 203 chunks) so the per-object
# completion probability IS the app-level delivered reliability; DNFs are an
# EXPECTED datum and are cut short by RWM_PERF_TIMEOUT_S=5 (>> the ~1 s
# completing-object time at 20 Mbit). Overhead is read from the sender DIAG
# cod/src rates (mechanism liveness).
#   usage: rstar_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BYTES=100000
RUNS=20
OUT=/home/vibe/rstar/c3rt-s${SEED_ARG}.log
mkdir -p /home/vibe/rstar
: > "$OUT"
echo "# binary: $(sha256sum /home/vibe/raptorpath/target/release/raptorpath)" >> "$OUT"
echo "# source commit: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"

run_arm() {
  local arm="$1"; shift
  local envs="$*"
  echo "=== rep=$REP arm=$arm seed=$SEED_ARG env=\"$envs RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5\" cmd=\"perf_rwm_c.sh c3 c3 realtime $BYTES $RUNS single\" $(date +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5 $envs \
        bash perf_rwm_c.sh c3 c3 realtime $BYTES $RUNS single 2>&1 \
        | grep -E '"seconds"|"dnf"|"summary"|FATAL' >> "$OUT"
  # Preserve the SENDER DIAG log per run (src/cod rates = overhead liveness).
  local dlog=/home/vibe/rstar/diag-s${SEED_ARG}-r${REP}-${arm}.log
  grep -E '^\[DIAG\]' /tmp/rwm-c.log > "$dlog" 2>/dev/null
  awk '{ for(i=1;i<=NF;i++){ if($i ~ /^src=/){gsub(/src=|sym\/s/,"",$i); s+=$i}
         if($i ~ /^cod=/){gsub(/cod=|sym\/s/,"",$i); c+=$i} } n++ }
       END { if(n>0 && s>0) printf "LIVE: diag_lines=%d mean_src=%.0f mean_cod=%.0f cod_over_src=%.4f\n", n, s/n, c/n, c/s }' \
       "$dlog" >> "$OUT"
}

for REP in $(seq 1 "$REPS"); do
  run_arm T "RWM_RSTAR_TAIL=1"
  run_arm L "RWM_RSTAR_TAIL=0"
done
echo "battery c3rt seed $SEED_ARG done $(date +%T)" >> "$OUT"
