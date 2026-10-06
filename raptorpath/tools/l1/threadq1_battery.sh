#!/bin/bash
# THE V-Q1 BATTERY (docs/status.md, "13. Threading Q1 — pre-registration"):
# the per-path I/O owner in its two placements against `main` 69fd846, with
# the D9 commit alone as its own arm, interleaved in one session. Derived
# from the V-P2a driver (archive/thread-p2a threadp2a_battery.sh).
#
#   sudo env TQ1_SHA_MAIN=<sha> TQ1_SHA_D9=<sha> TQ1_SHA_Q1=<sha> TQ1_LOCK_OWNER=<token> \
#     TQ1_BIN_MAIN=<bin> TQ1_BIN_D9=<bin> TQ1_BIN_Q1=<bin> TQ1_OUTDIR=<dir> \
#     bash threadq1_battery.sh <reps-per-seed>
#
# Launched by `threadq1_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TQ1_TAG=smoke, TQ1_SMOKE_PLAN), the scored battery
# (TQ1_TAG=tq1) and the reported-only ack-cadence block (TQ1_TAG=ackd,
# TQ1_ACKDIAG=1).
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Every arm withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (measurement discipline 15d:
# the shipped default) and then sets RWM_RTOBS=1 on every arm (and, in the
# ackd block only, RWM_ACKDIAG=1):
#   MAIN   main 69fd846                       (binary main)
#   D9     main + the D9 commit 95c750a       (binary d9)
#   IOS    the Q1 binary, RWM_IO_RT=shared    (binary q1)
#   IOO    the Q1 binary, RWM_IO_RT=own       (binary q1)
#
# PLAN per (rep, seed) block -- 16 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN D9 IOS IOO.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT
# line and ONE `TQ1ROW {json}` (threadq1_parse.py row). ABORT-SHA is checked
# for the arm's binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadq1_battery.sh threadq1_parse.py threadp1_parse.py \
    stage3_parse.py perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${TQ1_SEEDS:-42 7}"
OUTDIR="${TQ1_OUTDIR:?TQ1_OUTDIR}"
TAG="${TQ1_TAG:-tq1}"
TRIES="${TQ1_BRINGUP_TRIES:-2}"
ACKD="${TQ1_ACKDIAG:-0}"
BMAIN="${TQ1_BIN_MAIN:?TQ1_BIN_MAIN}"
BD9="${TQ1_BIN_D9:?TQ1_BIN_D9}"
BQ1="${TQ1_BIN_Q1:?TQ1_BIN_Q1}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadq1_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TQ1_LOCK_OWNER:-}" ] || ! grep -qF -- "$TQ1_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TQ1_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TQ1_LOCK_OWNER"

preflight_binary "$BQ1" "PIPE] pipeline=" "emission batching ACTIVE" "TOPO] io_rt=" "perf task failed"
preflight_binary "$BD9" "PIPE] pipeline=" "emission batching ACTIVE" "perf task failed"
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE"
for pair in "Q1:$BQ1:${TQ1_SHA_Q1:-}" "D9:$BD9:${TQ1_SHA_D9:-}" "MAIN:$BMAIN:${TQ1_SHA_MAIN:-}"; do
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
arm_bin() { case "$1" in IOS|IOO) echo "$BQ1" ;; D9) echo "$BD9" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in IOS|IOO) echo "$TQ1_SHA_Q1" ;; D9) echo "$TQ1_SHA_D9" ;; *) echo "$TQ1_SHA_MAIN" ;; esac; }
arm_env() { case "$1" in IOS) echo "RWM_IO_RT=shared" ;; IOO) echo "RWM_IO_RT=own" ;; *) echo "" ;; esac; }
CELLS="${TQ1_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
ARMS_ALL=(MAIN D9 IOS IOO)

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
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes ackdiag=$ACKD iort=$(arm_env "$arm") bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/tq1-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    local extra=()
    [ "$ACKD" = "1" ] && extra+=(RWM_ACKDIAG=1)
    local ae; ae="$(arm_env "$arm")"
    [ -n "$ae" ] && extra+=("$ae")
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_IO_RT \
        RWM_RTOBS=1 "${extra[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/tq1-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\] phase|--- RWM-C perf" /tmp/tq1-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/tq1-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./threadq1_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/tq1-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TQ1ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V-Q1 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' ackdiag=$ACKD $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TQ1_SHA_MAIN" >> "$OUT"
echo "=== binary D9 $BD9 sha256 $TQ1_SHA_D9" >> "$OUT"
echo "=== binary Q1 $BQ1 sha256 $TQ1_SHA_Q1" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (shipped default), then RWM_RTOBS=1$([ "$ACKD" = "1" ] && echo " RWM_ACKDIAG=1"); IOS +RWM_IO_RT=shared, IOO +RWM_IO_RT=own; soft_deadline=${TQ1_SOFT_DEADLINE:-none} block_est=${TQ1_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TQ1_SOFT_DEADLINE:-}" ] && [ -n "${TQ1_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TQ1_BLOCK_EST_S )) -gt "$TQ1_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TQ1_BLOCK_EST_S}s > deadline $TQ1_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TQ1_SMOKE_PLAN:-}" ]; then
      for ca_ in $TQ1_SMOKE_PLAN; do
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
if [ -z "${TQ1_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TQ1ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TQ1-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
