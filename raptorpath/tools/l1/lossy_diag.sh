#!/bin/bash
# diag/lossy-residual DIAGNOSIS (goal-gate "Lossy-Single Residual"): where do
# the missing 9-14% go on lossy SINGLE paths vs the BBR-class external bar
# (quinn-bbr 91.9-92.4 at c2, 18.6 at c3; tcp-bbr 17.5-17.8 at c3)?
# Instrumented sc2/sc3 runs, ONE run per invocation, fresh topology each,
# RWM_DIAG=1 RWM_RDIAG=1. Accounting terms read per run:
#   (a) FEC/retx overhead - [DIAG] cum=src/cod/ack totals (cod includes retx)
#                           + QDISC wire bytes/pkts/drops (perf_rwm_c.sh echo)
#   (b) wire idle         - [WIDLE] receiver inter-arrival gaps (wire truth)
#                           + sidle= sender emission gaps
#   (c) anchor over-read  - btlbw= gauge vs the cell's true rate, win=/cap,
#                           rtt/wrtt/rtp inflation
#   (d) store-cap probes  - RWM_STORE static arms + RWM_PLAIN_RS honest arm
#   (e) engine CPU        - CPUSRV/CPUCLI + [RDIAG] busy% (receiver)
#
#   usage: lossy_diag.sh <seed> [reps_per_arm]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-2}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/lossyres/diagnose-s${SEED_ARG}.log
DDIR=/home/vibe/lossyres/diag
mkdir -p "$DDIR" /home/vibe/lossyres
: > "$OUT"
echo "# lossy-residual DIAGNOSIS $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
uname -r >> "$OUT"

diag_one() { # name envs cell bytes
  local name="$1" envs="$2" cell="$3" bytes="$4"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_RDIAG=1 \
    bash perf_rwm_c.sh "$cell" "$cell" bulk "$bytes" 1 single 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:|QDISC" >> "$OUT" || true
  # Liveness echoes (discipline 1): which laws ran.
  local sr rs
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  rs=$(grep -c "send-interval SAMPLER ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr rs=$rs" >> "$OUT"
  # (a)+(b) sender: LAST [DIAG] line carries cumulative cum=src/cod/ack,
  # sidle, sweeps/retx; whole-run means for win/paused.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "t=[0-9.]+s|cum=[0-9]+/[0-9]+/[0-9]+|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|sweeps=[0-9]+|retx=[0-9]+|win=[0-9]+/[0-9]+" \
    | tr '\n' ' ' | sed 's/^/SENDER-LAST /' >> "$OUT"; echo >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^win=/){split(substr($i,5),a,"/");w+=a[1];cap=a[2];n++}; if($i~/^paused=/){gsub(/paused=|%/,"",$i);p+=$i}}} END{if(n>0) printf "SENDER-MEAN mean_win=%.0f last_cap=%s mean_paused=%.1f%% n_diag=%d\n", w/n, cap, p/n, n; else print "SENDER-MEAN no-diag"}' >> "$OUT") || true
  # (c) anchor gauge: per-path btlbw/rtt/wrtt/rtp from the last DIAG line +
  # the [SPAN] tail (rr = live repair rate, ar = A* send-rate anchor).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "p[0-9]+:infl=[0-9]+|btlbw=[0-9.]+|est=[Yn]|pl=[0-9.]+|rtt=[0-9.]+/wrtt=[0-9.]+/rtp[0-9.]+ms" \
    | tr '\n' ' ' | sed 's/^/ANCHOR /' >> "$OUT"; echo >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[SPAN\]' | tail -2 \
    | grep -oE "t=[0-9.]+s|a_star=[^ ]+|owed=[0-9.-]+|rr=[0-9.]+|debt=[0-9.]+|retx_buf=[0-9]+|ar=[0-9]+" \
    | tr '\n' ' ' | sed 's/^/SPAN-LAST /' >> "$OUT"; echo >> "$OUT") || true
  # (b) receiver wire idle: LAST [WIDLE] line (cumulative) from the server log.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep '\[WIDLE\]' | tail -1 \
    | sed 's/^/RECV /' >> "$OUT") || true
  # (e) receiver service: [RDIAG] busy%/msgs means.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep '\[RDIAG\]' \
    | awk '{for(i=1;i<=NF;i++){if($i~/^busy=/){gsub(/busy=|%/,"",$i);b+=$i;n++}; if($i~/^msgs=/){gsub(/msgs=|\/s/,"",$i);m+=$i}}} END{if(n>0) printf "RDIAG-MEAN busy=%.0f%% msgs=%.0f/s n=%d\n", b/n, m/n, n; else print "RDIAG-MEAN none"}' >> "$OUT") || true
  # goodput microstructure: every DIAG good= sample on one line (stall map).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' \
    | grep -oE "good=[0-9.]+" | cut -d= -f2 | tr '\n' ' ' \
    | sed 's/^/GOODSERIES /' >> "$OUT"; echo >> "$OUT") || true
  echo >> "$OUT"
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  # Defaults: the bar's 25 MB geometry + the 100 MB steady state (ramp split).
  diag_one sc2-def-25M   ""                 c2 25000000
  diag_one sc2-def-100M  ""                 c2 100000000
  diag_one sc3-def-25M   ""                 c3 25000000
  diag_one sc3-def-100M  ""                 c3 100000000
  # (d)/(c) probes: static store (dyn cap off) bracketing the honest cap vs
  # the over-read latch; the honest-anchor arm (RS witness cost known).
  diag_one sc2-s512-100M "RWM_STORE=512"    c2 100000000
  diag_one sc2-s256-100M "RWM_STORE=256"    c2 100000000
  diag_one sc3-s192-25M  "RWM_STORE=192"    c3 25000000
  diag_one sc3-s384-25M  "RWM_STORE=384"    c3 25000000
  diag_one sc2-rs-100M   "RWM_PLAIN_RS=1"   c2 100000000
  diag_one sc3-rs-25M    "RWM_PLAIN_RS=1"   c3 25000000
done
echo "DIAGNOSIS-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo DIAGNOSIS-DONE
