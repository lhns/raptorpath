#!/bin/bash
# feat/c8-pool-law L1 battery: the c8-aware CAPACITY-WEIGHTED pool law
# (RWM_STORE_CAPW) vs the two incumbent pool laws, on the shipped default
# stack (goal-gate "C8-Aware Pool Law" pre-registration) — PLUS the
# DEPRECATION REGISTER's owed RWM_FMTCP re-test arm (piggybacked, ADR-0066).
#
# Arms (RWM_GEN=0 plain bulk, RWM_DIAG=1 everywhere; SR/MP/MS/GAP are the
# shipped defaults, present in EVERY arm):
#   legacy = RWM_STORE_PATHS=0             (the legacy-1024 pool — the c8 WATCH winner)
#   pbs    = env unset                     (shipped default: path-scaled N×2048 pool)
#   capw   = RWM_STORE_CAPW=1 RWM_PLAIN_RS=1 (the derived capacity-weighted pool;
#            RS = the honest anchor the law needs — the ADR-0058/LOO named composition)
#   fmtcp  = RWM_FMTCP=1                   (register re-test arm; self-selects the
#            systematic generation submode, G=384 r=0.10 shipped params; c7+c8
#            reps 1..FMTCP_REPS only)
#
# Cells: c7 (c2+c2 dual, 200 MB), c8 (c2+c3 dual, 25 MB) — the verdict cells;
#        sc2 (c2 single, 100 MB), sc3 (c3 single, 25 MB) — same-session Σ terms
#        + N=1 inertness (CAPW is N≥2-gated; its singles price the RS witness).
#
# Liveness (MEASUREMENT DISCIPLINE 1/6/7): per-arm expected-echo assertion for
# SR (default ON), PBS, CAPW, RS, FMTCP (deprecation warn = the activation
# echo) — both directions (contamination too); ARMCOUNT per arm at the end.
# Gauges: win=/srel=/retx= + per-path sout (the DIAG-only store-attribution
# gauge this branch adds: which path holds the pooled outstanding).
#
#   usage: c8pool_battery.sh <seed> [reps] [fmtcp_reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"; FMTCP_REPS="${3:-4}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8pool/battery-s${SEED_ARG}.log
DDIR=/home/vibe/c8pool/diag
mkdir -p "$DDIR" /home/vibe/c8pool
: > "$OUT"
echo "# c8pool battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS fmtcp_reps=$FMTCP_REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

LEGACY="RWM_STORE_PATHS=0"
CAPW="RWM_STORE_CAPW=1 RWM_PLAIN_RS=1"
FM="RWM_FMTCP=1"

run_one() { # name envs cellA cellB mode bytes exp_pbs exp_capw exp_rs exp_fm
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6"
  local epbs="$7" ecapw="$8" ers="$9" efm="${10}"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # Liveness (discipline 1/6): every gate's echo must match the arm.
  local sr pbs capw rs hc fm
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  capw=$(grep -c "capacity-weighted outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  rs=$(grep -c "send-interval SAMPLER ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  hc=$(grep -c "honest floor-clock store caps ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  fm=$(grep -c "RWM_FMTCP is deprecated" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs/$epbs capw=$capw/$ecapw rs=$rs/$ers hc=$hc fm=$fm/$efm" >> "$OUT"
  if [ "$efm" -eq 0 ] && [ "$sr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-SR $name rep=$REP (SR is the shipped default)" >> "$OUT"; fi
  local v e tag
  for tag in "pbs:$pbs:$epbs" "capw:$capw:$ecapw" "rs:$rs:$ers" "fm:$fm:$efm"; do
    v=$(echo "$tag" | cut -d: -f2); e=$(echo "$tag" | cut -d: -f3)
    if [ "$e" -gt 0 ] && [ "$v" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-${tag%%:*} $name rep=$REP" >> "$OUT"; fi
    if [ "$e" -eq 0 ] && [ "$v" -gt 0 ]; then echo "ARM-CONTAMINATION-${tag%%:*} $name rep=$REP" >> "$OUT"; fi
  done
  # Occupancy / recovery gauges (the consol parsing, same fields).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|srel=[0-9]+/[0-9]+|paused=[0-9.]+%|retx=[0-9]+|pl=[0-9.]+" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^src=/){gsub(/src=|sym\/s/,"",$i);s+=$i;n++}; if($i~/^cod=/){gsub(/cod=|sym\/s/,"",$i);c+=$i}}} END{if(n>0) printf "RATES mean_src=%.0f mean_cod=%.0f cod_share=%.3f\n", s/n, c/n, (s>0)?c/s:0; else print "RATES no-diag"}' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap+=a[2];n++}; if($i~/^srel=/){split(substr($i,6),b,"/");r+=b[1];m++}}} END{if(n>0) printf "OCC mean_win=%.0f mean_cap=%.0f mean_srel=%.0f\n", w/n, cap/n, (m>0)?r/m:0; else print "OCC no-diag"}' >> "$OUT") || true
  # Per-path store attribution (the diagnosis gauge, DIAG-only sout tracking):
  # mean sout / last cap / btlbw / rtt / rtp per path label — dwell_i =
  # sout_i/btlbw_i computable offline.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{ for(i=1;i<=NF;i++){ if($i~/^p[0-9]+:infl=/){ split($i,q,":"); pid=q[1]; infl[pid]+=substr(q[2],6); ni[pid]++ } if($i~/^sout=/ && pid!=""){ split(substr($i,6),s,"/"); so[pid]+=s[1]; ns[pid]++ } if($i~/^btlbw=/ && pid!=""){ bb[pid]=substr($i,7) } if($i~/^rtt=/ && pid!=""){ split(substr($i,5),r,"/"); rt[pid]=r[1] } if($i~/rtp[0-9.]+ms$/ && pid!=""){ match($i,/rtp[0-9.]+/); rp[pid]=substr($i,RSTART+3,RLENGTH-3) } } } END{ for(p in ns){ printf "PERPATH %s mean_sout=%.0f mean_infl=%.0f btlbw=%s rtt=%s rtp=%s dwell_ms=%.0f\n", p, so[p]/ns[p], (ni[p]>0)?infl[p]/ni[p]:0, bb[p], rt[p], rp[p], (bb[p]>0)?1000*so[p]/ns[p]/bb[p]:0 } }' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  #       name         envs      cA cB  mode   bytes     pbs capw rs fm
  # -- c7 (symmetric preservation cell) --
  run_one c7-legacy   "$LEGACY"  c2 c2 dual   200000000  0   0   0  0
  run_one c7-pbs      ""         c2 c2 dual   200000000  1   0   0  0
  run_one c7-capw     "$CAPW"    c2 c2 dual   200000000  1   1   1  0
  if [ "$REP" -le "$FMTCP_REPS" ]; then
    run_one c7-fmtcp  "$FM"      c2 c2 dual   200000000  0   0   0  1
  fi
  # -- c8 (the target cell) --
  run_one c8-legacy   "$LEGACY"  c2 c3 dual   25000000   0   0   0  0
  run_one c8-pbs      ""         c2 c3 dual   25000000   1   0   0  0
  run_one c8-capw     "$CAPW"    c2 c3 dual   25000000   1   1   1  0
  if [ "$REP" -le "$FMTCP_REPS" ]; then
    run_one c8-fmtcp  "$FM"      c2 c3 dual   25000000   0   0   0  1
  fi
  # -- singles (same-session Σ terms per arm env; CAPW N=1-inert + RS witness) --
  run_one sc2-legacy  "$LEGACY"  c2 c2 single 100000000  0   0   0  0
  run_one sc2-pbs     ""         c2 c2 single 100000000  0   0   0  0
  run_one sc2-capw    "$CAPW"    c2 c2 single 100000000  0   0   1  0
  run_one sc3-legacy  "$LEGACY"  c3 c3 single 25000000   0   0   0  0
  run_one sc3-pbs     ""         c3 c3 single 25000000   0   0   0  0
  run_one sc3-capw    "$CAPW"    c3 c3 single 25000000   0   0   1  0
done

# Arm-liveness assertion (discipline 7): an arm with zero summaries fails
# LOUDLY, it does not vanish.
echo "--- ARMCOUNTS (expect $REPS headers per arm; fmtcp arms $FMTCP_REPS)" >> "$OUT"
for a in c7-legacy c7-pbs c7-capw c7-fmtcp c8-legacy c8-pbs c8-capw c8-fmtcp \
         sc2-legacy sc2-pbs sc2-capw sc3-legacy sc3-pbs sc3-capw; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
