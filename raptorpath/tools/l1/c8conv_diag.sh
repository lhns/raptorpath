#!/bin/bash
# feat/c8-conversion DIAGNOSIS runs (goal-gate "C8 Slow-Path Conversion",
# DIAGNOSE FIRST): instrumented c8 runs per pool arm BEFORE any fix build —
# the conversion gauges (this branch, RWM_DIAG-gated) name WHY slow-path
# symbols do not convert to delivered goodput at the heterogeneous dual cell:
#   (a) PLACEMENT STARVATION  — [C8CONV-S] splace share vs capacity share;
#   (b) BEHIND-FRONTIER       — [C8CONV-R] dup share of slow-path arrivals;
#   (c) HoL/REASSEMBLY        — [C8CONV-S] stallo owner split + [C8CONV-R]
#                                unb resolution split;
#   (d) ARRIVAL-ALIGNMENT     — [C8CONV-R] lead + [C8CONV-S] retxo/splace
#                                (slow-placed symbols re-served on fast).
# Sender gauges live in /tmp/rwm-c.log (client = bulk sender); receiver
# gauges in /tmp/rwm-s.log (server = bulk receiver). Full logs preserved.
#
#   usage: c8conv_diag.sh <seed> [reps_per_arm]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-2}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8conv/diagnose-s${SEED_ARG}.log
DDIR=/home/vibe/c8conv/diag
mkdir -p "$DDIR" /home/vibe/c8conv
: > "$OUT"
echo "# c8conv DIAGNOSIS $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"

diag_one() { # name envs
  local name="$1" envs="$2"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=c2/c3/dual $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh c2 c3 bulk 25000000 1 dual 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  local sr pbs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs" >> "$OUT"
  # The conversion gauges: LAST cumulative line each side.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[C8CONV-S\]' | tail -1 \
    | sed 's/^/SENDER  /' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep '\[C8CONV-R\]' | tail -1 \
    | sed 's/^/RECEIVER /' >> "$OUT") || true
  # Context: occupancy/paused/retx + per-path infl/btlbw/rtt means (as in
  # the c8pool diagnosis, for cross-session comparability).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap=a[2];n++}; if($i~/^paused=/){gsub(/paused=|%/,"",$i);p+=$i}; if($i~/^srel=/){split(substr($i,6),b,"/");sr2=b[2]}; if($i~/^retx=/){rt=substr($i,6)}}} END{if(n>0) printf "OCC mean_win=%.0f last_cap=%s mean_paused=%.1f%% total_srel=%s total_retx=%s n_diag=%d\n", w/n, cap, p/n, sr2, rt, n; else print "OCC no-diag"}' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{ for(i=1;i<=NF;i++){ if($i~/^p[0-9]+:infl=/){ split($i,q,":"); pid=q[1]; infl[pid]+=substr(q[2],6); ni[pid]++ } if($i~/^btlbw=/ && pid!=""){ bb[pid]=substr($i,7) } if($i~/^pl=/ && pid!=""){ pl[pid]=substr($i,4) } if($i~/^rtt=/ && $i~/ms$/ && pid!=""){ split(substr($i,5),r,"/"); rt[pid]=r[1]; match($i,/rtp[0-9.]+/); rp[pid]=substr($i,RSTART+3,RLENGTH-3) } } } END{ for(p in ni){ printf "PERPATH %s mean_infl=%.0f btlbw=%s pl=%s last_rtt=%s rtp=%s\n", p, infl[p]/ni[p], bb[p], pl[p], rt[p], rp[p] } }' >> "$OUT") || true
  # mpr tail (retx fp/on split) — the last cumulative recovery-plane line.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -oE 'mpr\[[^]]*\]' | tail -1 \
    | sed 's/^/MPR /' >> "$OUT") || true
  # Time series (every 4th line, first 30): conversion dynamics.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep '\[C8CONV-R\]' | awk 'NR%4==1' | head -30 \
    | sed 's/^/TS-R /' >> "$OUT") || true
  echo >> "$OUT"
  cp /tmp/rwm-c.log "$DDIR/diagnose-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/diagnose-${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  diag_one c8-legacy "RWM_STORE_PATHS=0"
  diag_one c8-pbs    ""
done
echo "DIAGNOSIS-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo DIAGNOSIS-DONE
