#!/bin/bash
# feat/c8-pool-law DIAGNOSIS runs (goal-gate "C8-Aware Pool Law", DIAGNOSE
# FIRST): instrumented c8 runs per pool arm BEFORE the battery — the per-path
# store-attribution gauge (DIAG-only sout tracking, this branch) answers WHY
# legacy-1024 beats the N×2048 path-scaled pool at c8 under SACK-release:
#   (a) does the SLOW path hold an outsized share of the pooled outstanding
#       (sout_slow ≫ its honest cap ~400–500, dwell_slow ≫ RTprop+R)?
#   (b) is the un-SACKed frontier span the binder (win≈cap with paused>0 on
#       legacy vs win≪cap yet goodput-stalled on pbs)?
# Full DIAG series preserved per run; a compact per-path table per run here.
#
#   usage: c8pool_diag.sh <seed> [reps_per_arm]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-2}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8pool/diagnose-s${SEED_ARG}.log
DDIR=/home/vibe/c8pool/diag
mkdir -p "$DDIR" /home/vibe/c8pool
: > "$OUT"
echo "# c8pool DIAGNOSIS $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"

diag_one() { # name envs
  local name="$1" envs="$2"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=c2/c3/dual $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh c2 c3 bulk 25000000 1 dual 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  local sr pbs capw rs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  capw=$(grep -c "capacity-weighted outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  rs=$(grep -c "send-interval SAMPLER ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs capw=$capw rs=$rs" >> "$OUT"
  # Whole-run means: occupancy vs cap, paused fraction, per-path attribution.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap=a[2];n++}; if($i~/^paused=/){gsub(/paused=|%/,"",$i);p+=$i}; if($i~/^srel=/){split(substr($i,6),b,"/");sr2=b[2]}; if($i~/^retx=/){rt=substr($i,6)}}} END{if(n>0) printf "OCC mean_win=%.0f last_cap=%s mean_paused=%.1f%% total_srel=%s total_retx=%s n_diag=%d\n", w/n, cap, p/n, sr2, rt, n; else print "OCC no-diag"}' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{ for(i=1;i<=NF;i++){ if($i~/^p[0-9]+:infl=/){ split($i,q,":"); pid=q[1]; infl[pid]+=substr(q[2],6); ni[pid]++ } if($i~/^sout=/ && pid!=""){ split(substr($i,6),s,"/"); so[pid]+=s[1]; if(s[1]>mx[pid])mx[pid]=s[1]; ns[pid]++ } if($i~/^btlbw=/ && pid!=""){ bb[pid]=substr($i,7) } if($i~/^pl=/ && pid!=""){ pl[pid]=substr($i,4) } if($i~/^rtt=/ && $i~/ms$/ && pid!=""){ split(substr($i,5),r,"/"); rt[pid]=r[1]; match($i,/rtp[0-9.]+/); rp[pid]=substr($i,RSTART+3,RLENGTH-3) } } } END{ for(p in ns){ printf "PERPATH %s mean_sout=%.0f max_sout=%.0f mean_infl=%.0f btlbw=%s pl=%s last_rtt=%s rtp=%s dwell_ms=%.0f\n", p, so[p]/ns[p], mx[p], (ni[p]>0)?infl[p]/ni[p]:0, bb[p], pl[p], rt[p], rp[p], (bb[p]>0)?1000*so[p]/ns[p]/bb[p]:0 } }' >> "$OUT") || true
  # Time series (every 4th DIAG line, first 40): win + per-path sout/rtt — the
  # dwell/parking dynamics for the writeup.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | awk 'NR%4==1' | head -40 \
    | grep -oE "t=[0-9.]+s|win=[0-9]+/[0-9]+|good=[0-9.]+Mbit|p[0-9]+:infl=[0-9]+|sout=[0-9]+/[0-9]+/b[0-9]+|rtt=[0-9.]+/wrtt=[0-9.]+/rtp[0-9.]+ms" \
    | tr '\n' ' ' | sed 's/t=/\nt=/g' >> "$OUT") || true
  echo >> "$OUT"
  cp /tmp/rwm-c.log "$DDIR/diagnose-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/diagnose-${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  diag_one c8-legacy "RWM_STORE_PATHS=0"
  diag_one c8-pbs    ""
  diag_one c8-capw   "RWM_STORE_CAPW=1 RWM_PLAIN_RS=1"
done
echo "DIAGNOSIS-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo DIAGNOSIS-DONE
