#!/bin/bash
# feat/window-mtu B1 jitter CROSS-CHECK (goal-gate "Window Decoupling + MTU
# Scaling" part 1, prediction 4 — attribution, NOT flip-gating): does the
# decoupled window law release the B1 jitter-cell Copa dwell ceiling
# (win=1024/1024 pin, Little's-law ~36 Mbit)?
#
# Cells: jit5 + jit15 (adv_cells.sh recipes verbatim, B1 battery cells).
# Arms (interleaved round-robin per rep, fresh cell + tunnel per invocation):
#   bbr     = env unset                        (same-session reference)
#   copa    = RWM_QUIC_CC=passthrough          (the B1 0.32-0.36x arm)
#   copafix = passthrough + RWM_WIN_DECOUPLE=1 (the ceiling-release arm)
#
#   usage: sudo bash winmtu_jit.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"; REPS="${2:-5}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/winmtu/jit-s${SEED_ARG}.log
DDIR=/home/vibe/winmtu/diag
mkdir -p "$DDIR" /home/vibe/winmtu
: > "$OUT"
echo "# winmtu jitter cross-check $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
uname -r >> "$OUT"

BYTES=25000000
BASEENV="RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=120"

arm_env() {
  case "$1" in
    bbr)     echo "" ;;
    copa)    echo "RWM_QUIC_CC=passthrough" ;;
    copafix) echo "RWM_QUIC_CC=passthrough RWM_WIN_DECOUPLE=1" ;;
  esac
}

run_one() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs; envs="$(arm_env "$arm")"
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$BYTES $(date -u +%T)" >> "$OUT"
  pkill -x raptorpath 2>/dev/null || true
  rm -f /tmp/adv-c.log /tmp/adv-s.log /tmp/adv-c-clean.log

  if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
    echo "TOPO-PING abort $name rep=$REP — retrying once" >> "$OUT"
    if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
      echo "TOPO-ABORT $name rep=$REP (double abort, invocation dropped)" >> "$OUT"
      bash ./adv_cells.sh down >/dev/null 2>&1 || true
      return 0
    fi
  fi

  # shellcheck disable=SC2086
  ip netns exec rp-srv env $BASEENV $envs "$BIN" perf --server --bind 10.77.0.2:7000 \
      --window-reliable --protocol-hint bulk >/tmp/adv-s.log 2>&1 &
  local up=0
  for _ in $(seq 1 20); do
    ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000' && { up=1; break; }
    sleep 0.3
  done
  if [ "$up" -eq 0 ]; then
    echo "SRV-BIND-FAIL $name rep=$REP (invocation dropped)" >> "$OUT"
    pkill -x raptorpath 2>/dev/null || true
    bash ./adv_cells.sh down >/dev/null 2>&1 || true
    return 0
  fi
  sleep 1
  # shellcheck disable=SC2086
  timeout 300 ip netns exec rp-cli env $BASEENV $envs "$BIN" perf --client \
      --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
      --window-reliable --protocol-hint bulk \
      --bytes "$BYTES" --runs 1 >/tmp/adv-c.log 2>&1
  local rc=$?
  [ "$rc" -ne 0 ] && echo "CLIENT-RC $name rep=$REP rc=$rc" >> "$OUT"
  sed 's/\x1b\[[0-9;]*m//g' /tmp/adv-c.log > /tmp/adv-c-clean.log 2>/dev/null || true
  (grep -E '"summary"|"dnf"|"run"' /tmp/adv-c-clean.log | tail -3 >> "$OUT") || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  # CC + law liveness (discipline 1).
  local bbrn ptn feedn wd wdg
  bbrn=$(grep -c "congestion controller: BBR" /tmp/adv-c-clean.log 2>/dev/null || true)
  ptn=$(grep -c "quinn congestion window is engine-owned" /tmp/adv-c-clean.log 2>/dev/null || true)
  feedn=$(grep -c "Copa delivery feed ACTIVE" /tmp/adv-c-clean.log 2>/dev/null || true)
  wd=$(grep -c "window/inflight decoupling ACTIVE" /tmp/adv-c-clean.log 2>/dev/null || true)
  wdg=$(grep -c "wd=al" /tmp/adv-c-clean.log 2>/dev/null || true)
  echo "LIVENESS arm=$arm bbr=$bbrn pt=$ptn feed=$feedn wd=$wd wd_gauge=$wdg" >> "$OUT"
  case "$arm" in
    bbr)
      [ "$bbrn" -eq 0 ] && echo "ARM-LIVENESS-FAIL $name rep=$REP (no BBR echo)" >> "$OUT" ;;
    copa)
      { [ "$ptn" -eq 0 ] || [ "$feedn" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL $name rep=$REP (pt=$ptn feed=$feedn)" >> "$OUT"
      [ "$wd" -gt 0 ] && echo "ARM-CONTAMINATION $name rep=$REP (wd echo in copa arm)" >> "$OUT" ;;
    copafix)
      { [ "$ptn" -eq 0 ] || [ "$feedn" -eq 0 ] || [ "$wd" -eq 0 ]; } \
        && echo "ARM-LIVENESS-FAIL $name rep=$REP (pt=$ptn feed=$feedn wd=$wd)" >> "$OUT" ;;
  esac

  # The ceiling gauge: last 3 DIAG lines' win/wnd2/wd/rtt (the 1024 pin vs
  # the released ceiling is THE cross-check datum).
  (grep -a '\[DIAG\]' /tmp/adv-c-clean.log | tail -3 \
    | grep -oE "win=[0-9]+/[0-9]+|paused=[0-9]+%|wnd2=[0-9]+/[0-9]+|wd=al[0-9]+/r[0-9]+/ret[0-9]+|rtt=[0-9.]+ms|cum=[0-9]+/[0-9]+/[0-9]+" \
    | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
  cp /tmp/adv-c.log "$DDIR/jit-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true

  pkill -x raptorpath 2>/dev/null || true
  bash ./adv_cells.sh down >/dev/null 2>&1 || true
}

for REP in $(seq 1 $REPS); do
  for cell in jit5 jit15; do
    for arm in bbr copa copafix; do
      run_one "$cell" "$arm"
    done
  done
done

for a in jit5-bbr jit5-copa jit5-copafix jit15-bbr jit15-copa jit15-copafix; do
  n=$(awk "/=== rep=.* arm=$a /{f=1} f&&/\"summary\":true/{c++;f=0} END{print c+0}" "$OUT")
  echo "ARMCOUNT $a n=$n (target $REPS)" >> "$OUT"
done
echo "JIT-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo JIT-DONE
