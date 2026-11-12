#!/bin/bash
# THE V-R1 BATTERY (docs/status.md, "17. Receiver per-datagram cost (R1) —
# pre-registration"): the receiver's four cheap per-datagram fixes (R1)
# against `main` ce60079 (MAIN), interleaved in one session. Derived from the
# V-Q2 driver (threadq2_battery.sh).
#
#   sudo env TR1_SHA_MAIN=<sha> TR1_SHA_R1=<sha> TR1_LOCK_OWNER=<token> \
#     TR1_BIN_MAIN=<bin> TR1_BIN_R1=<bin> TR1_OUTDIR=<dir> \
#     bash threadr1_battery.sh <reps-per-seed>
#
# Launched by `threadr1_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TR1_TAG=smoke, TR1_SMOKE_PLAN) and the scored battery
# (TR1_TAG=tr1).
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Every arm withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG RWM_IO_RT (measurement
# discipline 15d: the shipped default) and then sets RWM_RTOBS=1 and
# RWM_RDIAG=1 on every arm (never RWM_RTOBS=2: its per-section clock reads
# would perturb the receiver under test):
#   MAIN   main ce60079        (binary main)
#   R1     the R1 tree         (binary r1)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN R1.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT
# line and ONE `TR1ROW {json}` (threadr1_parse.py row). ABORT-SHA is checked
# for the arm's binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadr1_battery.sh threadr1_parse.py threadq1_parse.py \
    threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${TR1_SEEDS:-42 7}"
OUTDIR="${TR1_OUTDIR:?TR1_OUTDIR}"
TAG="${TR1_TAG:-tr1}"
TRIES="${TR1_BRINGUP_TRIES:-2}"
ACKD=0
BMAIN="${TR1_BIN_MAIN:?TR1_BIN_MAIN}"
BR1="${TR1_BIN_R1:?TR1_BIN_R1}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadr1_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TR1_LOCK_OWNER:-}" ] || ! grep -qF -- "$TR1_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TR1_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TR1_LOCK_OWNER"

preflight_binary "$BR1" "PIPE] pipeline=" "emission batching ACTIVE" "ack_batches=" "ibp_hit=" "RXSEC"
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE" "ack_batches="
for pair in "R1:$BR1:${TR1_SHA_R1:-}" "MAIN:$BMAIN:${TR1_SHA_MAIN:-}"; do
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
arm_bin() { case "$1" in R1) echo "$BR1" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in R1) echo "$TR1_SHA_R1" ;; *) echo "$TR1_SHA_MAIN" ;; esac; }
CELLS="${TR1_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
ARMS_ALL=(MAIN R1)

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
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/tr1-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT \
        RWM_RTOBS=1 RWM_RDIAG=1 \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/tr1-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\] phase|--- RWM-C perf" /tmp/tr1-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/tr1-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./threadr1_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/tr1-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TR1ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V-R1 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' ackdiag=$ACKD $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TR1_SHA_MAIN" >> "$OUT"
echo "=== binary R1 $BR1 sha256 $TR1_SHA_R1" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG RWM_IO_RT (shipped default), then RWM_RTOBS=1 RWM_RDIAG=1; soft_deadline=${TR1_SOFT_DEADLINE:-none} block_est=${TR1_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TR1_SOFT_DEADLINE:-}" ] && [ -n "${TR1_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TR1_BLOCK_EST_S )) -gt "$TR1_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TR1_BLOCK_EST_S}s > deadline $TR1_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TR1_SMOKE_PLAN:-}" ]; then
      for ca_ in $TR1_SMOKE_PLAN; do
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
if [ -z "${TR1_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TR1ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TR1-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
