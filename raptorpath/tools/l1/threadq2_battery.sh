#!/bin/bash
# THE V-Q2 BATTERY (docs/status.md, "14. Threading Q2 — pre-registration"):
# the scheduler split by direction (Q2) against `main` 0ef0e0d (MAIN, the
# Q1-shipped tree), interleaved in one session. Derived from the V-Q1 driver
# (threadq1_battery.sh).
#
#   sudo env TQ2_SHA_MAIN=<sha> TQ2_SHA_Q2=<sha> TQ2_LOCK_OWNER=<token> \
#     TQ2_BIN_MAIN=<bin> TQ2_BIN_Q2=<bin> TQ2_OUTDIR=<dir> \
#     bash threadq2_battery.sh <reps-per-seed>
#
# Launched by `threadq2_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TQ2_TAG=smoke, TQ2_SMOKE_PLAN), the scored battery
# (TQ2_TAG=tq2) and the reported-only ack-cadence block (TQ2_TAG=ackd,
# TQ2_ACKDIAG=1).
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Every arm withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (measurement discipline 15d:
# the shipped default) and then sets RWM_RTOBS=1 on every arm (and, in the
# ackd block only, RWM_ACKDIAG=1):
#   MAIN   main 0ef0e0d        (binary main)
#   Q2     the Q2 tree         (binary q2)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN Q2.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT
# line and ONE `TQ2ROW {json}` (threadq2_parse.py row). ABORT-SHA is checked
# for the arm's binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadq2_battery.sh threadq2_parse.py threadq1_parse.py \
    threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${TQ2_SEEDS:-42 7}"
OUTDIR="${TQ2_OUTDIR:?TQ2_OUTDIR}"
TAG="${TQ2_TAG:-tq2}"
TRIES="${TQ2_BRINGUP_TRIES:-2}"
ACKD="${TQ2_ACKDIAG:-0}"
BMAIN="${TQ2_BIN_MAIN:?TQ2_BIN_MAIN}"
BQ2="${TQ2_BIN_Q2:?TQ2_BIN_Q2}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadq2_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TQ2_LOCK_OWNER:-}" ] || ! grep -qF -- "$TQ2_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TQ2_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TQ2_LOCK_OWNER"

preflight_binary "$BQ2" "PIPE] pipeline=" "emission batching ACTIVE" "perf task failed"
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE" "TOPO] io_rt="
for pair in "Q2:$BQ2:${TQ2_SHA_Q2:-}" "MAIN:$BMAIN:${TQ2_SHA_MAIN:-}"; do
  IFS=: read -r who b want <<< "$pair"
  got="$(sha256sum "$b" | cut -d' ' -f1)"
  if [ -z "$want" ] || [ "$got" != "$want" ]; then
    _lb_say "ABORT-SHA start: $who binary $got != '${want:-unset}'"
    exit 5
  fi
done
if pgrep -x raptorpath >/dev/null 2>&1; then
  _lb_say "BUSY: raptorpath already running -- aborting"
  exit 3
fi

cell_spec() { # cell -> "scenA scenB mode bytes"
  case "$1" in
    c1s-400) echo "c1 c1 single 400000000" ;;
    c1d-400) echo "c1 c1 dual 400000000" ;;
    c2-100)  echo "c2 c2 single 100000000" ;;
    c8-100)  echo "c2 c3 dual 100000000" ;;
    *) echo "" ;;
  esac
}
arm_bin() { case "$1" in Q2) echo "$BQ2" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in Q2) echo "$TQ2_SHA_Q2" ;; *) echo "$TQ2_SHA_MAIN" ;; esac; }
CELLS="${TQ2_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
ARMS_ALL=(MAIN Q2)

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes t0 rc wall attempt sha bin want
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  bin="$(arm_bin "$arm")"; want="$(arm_sha "$arm")"
  local name="$cell-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$bin" | cut -d' ' -f1)"
    if [ "$sha" != "$want" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: binary $bin $sha != $want -- battery stopped"
      exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes ackdiag=$ACKD bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/tq2-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    local extra=()
    [ "$ACKD" = "1" ] && extra+=(RWM_ACKDIAG=1)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_IO_RT \
        RWM_RTOBS=1 "${extra[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/tq2-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\] phase|--- RWM-C perf" /tmp/tq2-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/tq2-drv.out "$base-drv.out" 2>/dev/null || true
    cp /tmp/rwm-c.log "$base-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$base-s.log" 2>/dev/null || true
    if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null \
        && [ "$attempt" -lt "$TRIES" ]; then
      echo "RUN-RETRY $name s$seed rep=$rep attempt=$attempt (no summary)" >> "$OUT"
      continue
    fi
    break
  done
  if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null; then
    echo "ABORT-BRINGUP $name s$seed rep=$rep after $TRIES attempts: NO_DATA" >> "$OUT"
  fi
  [ $(( cot_b + cot_a )) -gt 0 ] && cot=1
  python3 ./threadq2_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/tq2-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TQ2ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V-Q2 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' ackdiag=$ACKD $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TQ2_SHA_MAIN" >> "$OUT"
echo "=== binary Q2 $BQ2 sha256 $TQ2_SHA_Q2" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (shipped default), then RWM_RTOBS=1$([ "$ACKD" = "1" ] && echo " RWM_ACKDIAG=1"); soft_deadline=${TQ2_SOFT_DEADLINE:-none} block_est=${TQ2_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TQ2_SOFT_DEADLINE:-}" ] && [ -n "${TQ2_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TQ2_BLOCK_EST_S )) -gt "$TQ2_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TQ2_BLOCK_EST_S}s > deadline $TQ2_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TQ2_SMOKE_PLAN:-}" ]; then
      for ca_ in $TQ2_SMOKE_PLAN; do
        run_one "${ca_%%:*}" "${ca_##*:}" "$SEED" "$REP"
      done
    else
      for CELL in $CELLS; do
        n=${#ARMS_ALL[@]}
        k=$(( BLOCK % n ))
        for ((j = 0; j < n; j++)); do
          run_one "$CELL" "${ARMS_ALL[$(( (j + k) % n ))]}" "$SEED" "$REP"
        done
      done
    fi
    echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( BLOCK + 1 ))
  done
  [ "$STOP" -eq 1 ] && break
done

echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
if [ -z "${TQ2_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TQ2ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TQ2-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
