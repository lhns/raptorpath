#!/bin/bash
# feat/c8-pool-law ATTRIBUTION top-up (post-battery diagnosis, no tuning):
# the battery's c7-capw regression (−22 vs pbs at s42) shows pool cap ~3.7-3.9k
# (not binding) with occupancy ~500 and per-path infl 33-48 — the throttle is
# NOT the CAPW sizing law. This arm separates the RS-composition cost from the
# law: rs = shipped default (PBS pool) + RWM_PLAIN_RS=1, NO RWM_STORE_CAPW.
#   c7-rs ≈ c7-capw  -> the regression is owned by the RS sampling composition;
#   c7-rs ≈ c7-pbs   -> the regression is owned by the CAPW sizing law.
# Same for c8 (consolidation measured stack-rs 79.11/87.98 cross-session;
# this is the same-session row). Sigma terms: the battery's sc2/sc3-capw
# singles are the rs-env singles (CAPW is N=1-inert; echo cosmetic).
#
#   usage: c8pool_topup.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-6}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8pool/topup-s${SEED_ARG}.log
DDIR=/home/vibe/c8pool/diag
mkdir -p "$DDIR" /home/vibe/c8pool
: > "$OUT"
echo "# c8pool topup $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"

run_one() { # name envs cellA cellB bytes
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/dual bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 dual 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  local sr pbs capw rs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  capw=$(grep -c "capacity-weighted outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  rs=$(grep -c "send-interval SAMPLER ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs/1 capw=$capw/0 rs=$rs/1" >> "$OUT"
  if [ "$sr" -eq 0 ] || [ "$pbs" -eq 0 ] || [ "$rs" -eq 0 ]; then echo "ARM-LIVENESS-FAIL $name rep=$REP" >> "$OUT"; fi
  if [ "$capw" -gt 0 ]; then echo "ARM-CONTAMINATION-capw $name rep=$REP" >> "$OUT"; fi
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap+=a[2];n++}}} END{if(n>0) printf "OCC mean_win=%.0f mean_cap=%.0f\n", w/n, cap/n; else print "OCC no-diag"}' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{ for(i=1;i<=NF;i++){ if($i~/^p[0-9]+:infl=/){ split($i,q,":"); pid=q[1]; infl[pid]+=substr(q[2],6); ni[pid]++ } if($i~/^sout=/ && pid!=""){ split(substr($i,6),s,"/"); so[pid]+=s[1]; ns[pid]++ } if($i~/^btlbw=/ && pid!=""){ bb[pid]=substr($i,7) } } } END{ for(p in ns){ printf "PERPATH %s mean_sout=%.0f mean_infl=%.0f btlbw=%s\n", p, so[p]/ns[p], (ni[p]>0)?infl[p]/ni[p]:0, bb[p] } }' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  run_one c7-rs "RWM_PLAIN_RS=1" c2 c2 200000000
  run_one c8-rs "RWM_PLAIN_RS=1" c2 c3 25000000
done
echo "TOPUP-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo TOPUP-DONE-$SEED_ARG
