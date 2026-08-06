#!/bin/bash
# feat/window-mtu PART 1 DIAGNOSIS (goal-gate "Window Decoupling + MTU
# Scaling"): name the 1024-latch's stall-insurance term. One instrumented
# run each way per cell:
#   sc3: def (1024 latch) vs s384 (RWM_STORE=384 honest-size static — the
#        known 12%-idle arm), 25 MB
#   sc2: def vs s256 (RWM_STORE=256 — the arm the July flake class lost),
#        100 MB
# x REPS (default 2), seed 42, RWM_DIAG=1. The full [DIAG] time series per
# run is preserved (diag/ copies) — the wnd2=head/hole + relgap + sidle
# gauges carry the D1/D2/D3 decision rule.
#
#   usage: winmtu_diag.sh [seed] [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-2}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/winmtu/diagnose-s${SEED_ARG}.log
DDIR=/home/vibe/winmtu/diag
mkdir -p "$DDIR" /home/vibe/winmtu
: > "$OUT"
echo "# winmtu part-1 DIAGNOSIS $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
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

run_one() { # name envs cell bytes
  local name="$1" envs="$2" cell="$3" bytes="$4" attempt ok=0
  for attempt in 1 2 3; do
    wait_clear
    sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$bytes $(date -u +%T)" >> "$OUT"
    sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$cell" "$cell" bulk "$bytes" 1 single 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli0" >> "$OUT" || true
    # Full DIAG series preserved per attempt (the diagnosis deliverable).
    cp /tmp/rwm-c.log "$DDIR/diag-${name}-s${SEED_ARG}-r${REP}-a${attempt}-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/diag-${name}-s${SEED_ARG}-r${REP}-a${attempt}-s.log" 2>/dev/null || true
    # Quick scrape: last 3 DIAG lines' wnd2/relgap/sidle/win/paused/mpr.
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -3 \
      | grep -oE "t=[0-9.]+s|win=[0-9]+/[0-9]+|paused=[0-9]+%|wnd2=[0-9]+/[0-9]+|relgap=[0-9]+ms/mx[0-9]+ms|sidle=[0-9]+ms/[0-9]+/mx[0-9]+ms|rtt=[0-9.]+ms|cum=[0-9]+/[0-9]+/[0-9]+|fired=[0-9]+|y=[0-9]+|retx=[0-9]+" \
      | tr '\n' ' ' | sed 's/^/GAUGE /' >> "$OUT"; echo >> "$OUT") || true
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1; break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
  done
  [[ $ok == 1 ]] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

for REP in $(seq 1 $REPS); do
  run_one sc3-def  ""              c3 25000000
  run_one sc3-s384 "RWM_STORE=384" c3 25000000
  run_one sc2-def  ""              c2 100000000
  run_one sc2-s256 "RWM_STORE=256" c2 100000000
done

echo "DIAG-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo DIAG-DONE
