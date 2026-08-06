#!/bin/bash
# feat/window-mtu L1 BATTERY (goal-gate "Window Decoupling + MTU Scaling"):
# the pre-registered A/B for RWM_WIN_DECOUPLE (part 1) and RWM_WIRE_COMPACT
# (part 2), parts evaluated INDEPENDENTLY plus one composed arm at singles.
#
#   singles (PRIMARY, vs quinn-bbr 91.9/18.6):
#     sc2 = c2 single 100 MB, sc3 = c3 single 25 MB
#     arms: def <-> fix (RWM_WIN_DECOUPLE=1) <-> mtu (RWM_WIRE_COMPACT=1)
#           <-> both, interleaved round-robin per rep
#   duals (no-regression; same-session Sigma from the singles arms):
#     c7 = c2+c2 dual 200 MB, c8 = c2+c3 dual 25 MB
#     arms: def <-> fix <-> mtu
#
# Retry-hardened per the July flake class (port wait + <=3 attempts, stale
# logs removed per attempt, aborts preserved, n per arm quoted).
#
#   usage: winmtu_battery.sh <seed> [reps] [singles|duals|all]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-8}"; SCOPE="${3:-all}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/winmtu/battery-s${SEED_ARG}.log
DDIR=/home/vibe/winmtu/diag
mkdir -p "$DDIR" /home/vibe/winmtu
: >> "$OUT"
echo "# winmtu BATTERY $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS scope=$SCOPE" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
uname -r >> "$OUT"

wait_clear() {
  for _ in $(seq 1 30); do
    if ! pgrep -x raptorpath >/dev/null 2>&1 \
       && ! sudo ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000'; then
      return 0
    fi
    sudo pkill -x raptorpath 2>/dev/null || true
    sleep 1
  done
  return 0
}

check_liveness() { # arm  -> LIVENESS + contamination lines
  local arm="$1" wd mtu wdg
  wd=$(grep -ac "window/inflight decoupling ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  mtu=$(grep -ac "compact DATA framing ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  wdg=$(grep -ac "wd=al" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS arm=$arm wd=$wd mtu=$mtu wd_gauge=$wdg" >> "$OUT"
  case "$arm" in
    *def*) { [ "$wd" -gt 0 ] || [ "$mtu" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP" >> "$OUT" ;;
    *fix*) [ "$wd" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no wd echo)" >> "$OUT"
           [ "$mtu" -gt 0 ] && echo "ARM-CONTAMINATION arm=$arm rep=$REP (mtu echo in fix arm)" >> "$OUT" ;;
    *mtu*) [ "$mtu" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no mtu echo)" >> "$OUT"
           [ "$wd" -gt 0 ] && echo "ARM-CONTAMINATION arm=$arm rep=$REP (wd echo in mtu arm)" >> "$OUT" ;;
    *both*) { [ "$wd" -eq 0 ] || [ "$mtu" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (wd=$wd mtu=$mtu)" >> "$OUT" ;;
  esac
  return 0
}

run_one() { # name envs cellA cellB bytes mode
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    check_liveness "$name"
    # Mechanism gauges: last DIAG line's wnd2/wd/rtt/fired + cum totals.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
      | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9]+%|wnd2=[0-9]+/[0-9]+|wd=al[0-9]+/r[0-9]+/ret[0-9]+|relgap=[0-9]+ms/mx[0-9]+ms|rtt=[0-9.]+ms|cum=[0-9]+/[0-9]+/[0-9]+|fired=[0-9]+|y=[0-9]+|retx=[0-9]+|srel=[0-9]+/[0-9]+" \
      | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-s.log" 2>/dev/null || true
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

E_FIX="RWM_WIN_DECOUPLE=1"
E_MTU="RWM_WIRE_COMPACT=1"
E_BOTH="RWM_WIN_DECOUPLE=1 RWM_WIRE_COMPACT=1"

if [[ "$SCOPE" == "singles" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one sc2-def  ""        c2 c2 100000000 single
    run_one sc2-fix  "$E_FIX"  c2 c2 100000000 single
    run_one sc2-mtu  "$E_MTU"  c2 c2 100000000 single
    run_one sc2-both "$E_BOTH" c2 c2 100000000 single
    run_one sc3-def  ""        c3 c3 25000000 single
    run_one sc3-fix  "$E_FIX"  c3 c3 25000000 single
    run_one sc3-mtu  "$E_MTU"  c3 c3 25000000 single
    run_one sc3-both "$E_BOTH" c3 c3 25000000 single
  done
fi

if [[ "$SCOPE" == "duals" || "$SCOPE" == "all" ]]; then
  for REP in $(seq 1 $REPS); do
    run_one c7-def ""       c2 c2 200000000 dual
    run_one c7-fix "$E_FIX" c2 c2 200000000 dual
    run_one c7-mtu "$E_MTU" c2 c2 200000000 dual
    run_one c8-def ""       c2 c3 25000000 dual
    run_one c8-fix "$E_FIX" c2 c3 25000000 dual
    run_one c8-mtu "$E_MTU" c2 c3 25000000 dual
  done
fi

# ARMCOUNT assertion (discipline 7): summaries per arm, loudly.
for a in sc2-def sc2-fix sc2-mtu sc2-both sc3-def sc3-fix sc3-mtu sc3-both \
         c7-def c7-fix c7-mtu c8-def c8-fix c8-mtu; do
  n=$(awk "/=== rep=.* arm=$a /{f=1} f&&/\"summary\":true/{c++;f=0} END{print c+0}" "$OUT")
  echo "ARMCOUNT $a n=$n" >> "$OUT"
done
echo "BATTERY-DONE seed=$SEED_ARG scope=$SCOPE $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE
