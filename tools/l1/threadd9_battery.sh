#!/bin/bash
# THE D9 ATTRIBUTION BATTERY (docs/status.md, "15. D9 attribution and the c8
# lag re-check -- pre-registration"): NOD9 (main 88476a4 with only the D9 hunk
# of 95c750a reverted: the perf body back on the block_on main thread) against
# MAIN (main 88476a4, D9 shipped), interleaved in one session. Derived from the
# V-Q2 driver (threadq2_battery.sh).
#
#   sudo env TD9_SHA_MAIN=<sha> TD9_SHA_NOD9=<sha> TD9_LOCK_OWNER=<token> \
#     TD9_BIN_MAIN=<bin> TD9_BIN_NOD9=<bin> TD9_OUTDIR=<dir> \
#     [TD9_FULL_REPS=3 TD9_EXTRA_CELLS=c8-100] bash threadd9_battery.sh <reps-per-seed>
#
# Launched by `threadd9_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TD9_TAG=smoke, TD9_SMOKE_PLAN) and the scored battery
# (TD9_TAG=td9).
#
# Reps beyond TD9_FULL_REPS (up to <reps-per-seed>) run only TD9_EXTRA_CELLS
# (the c8 n = 12 re-check): 2 invocations per block, same arm rotation.
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Every arm withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (measurement discipline 15d:
# the shipped default) and then sets RWM_RTOBS=1 on every arm (and, in the
# ackd block only, RWM_ACKDIAG=1):
#   MAIN   main 88476a4            (binary main; contains "perf task failed")
#   NOD9   88476a4 minus the D9 hunk (binary nod9; does NOT contain it)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN NOD9.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT
# line and ONE `TD9ROW {json}` (threadd9_parse.py row). ABORT-SHA is checked
# for the arm's binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadd9_battery.sh threadd9_parse.py threadq1_parse.py \
    threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${TD9_SEEDS:-42 7}"
OUTDIR="${TD9_OUTDIR:?TD9_OUTDIR}"
TAG="${TD9_TAG:-td9}"
TRIES="${TD9_BRINGUP_TRIES:-2}"
ACKD="${TD9_ACKDIAG:-0}"
BMAIN="${TD9_BIN_MAIN:?TD9_BIN_MAIN}"
BNOD9="${TD9_BIN_NOD9:?TD9_BIN_NOD9}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadd9_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TD9_LOCK_OWNER:-}" ] || ! grep -qF -- "$TD9_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TD9_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TD9_LOCK_OWNER"

preflight_binary "$BNOD9" "PIPE] pipeline=" "emission batching ACTIVE"
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE" "perf task failed"
# The D9 hunk is the only difference: its error string is in MAIN, not in NOD9.
if grep -aq -- "perf task failed" "$BNOD9" 2>/dev/null; then
  _lb_say "REFUSED: NOD9 binary still carries the D9 task (perf task failed) -- wrong binary"
  exit 5
fi
for pair in "NOD9:$BNOD9:${TD9_SHA_NOD9:-}" "MAIN:$BMAIN:${TD9_SHA_MAIN:-}"; do
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
arm_bin() { case "$1" in NOD9) echo "$BNOD9" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in NOD9) echo "$TD9_SHA_NOD9" ;; *) echo "$TD9_SHA_MAIN" ;; esac; }
CELLS="${TD9_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
FULL_REPS="${TD9_FULL_REPS:-$REPS}"      # reps beyond this run EXTRA_CELLS only
EXTRA_CELLS="${TD9_EXTRA_CELLS:-c8-100}"
ARMS_ALL=(MAIN NOD9)

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
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/td9-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    local extra=()
    [ "$ACKD" = "1" ] && extra+=(RWM_ACKDIAG=1)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_IO_RT \
        RWM_RTOBS=1 "${extra[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/td9-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\] phase|--- RWM-C perf" /tmp/td9-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/td9-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./threadd9_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/td9-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TD9ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== D9 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' ackdiag=$ACKD $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TD9_SHA_MAIN" >> "$OUT"
echo "=== binary NOD9 $BNOD9 sha256 $TD9_SHA_NOD9" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT (shipped default), then RWM_RTOBS=1$([ "$ACKD" = "1" ] && echo " RWM_ACKDIAG=1"); soft_deadline=${TD9_SOFT_DEADLINE:-none} block_est=${TD9_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TD9_SOFT_DEADLINE:-}" ] && [ -n "${TD9_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TD9_BLOCK_EST_S )) -gt "$TD9_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TD9_BLOCK_EST_S}s > deadline $TD9_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TD9_SMOKE_PLAN:-}" ]; then
      for ca_ in $TD9_SMOKE_PLAN; do
        run_one "${ca_%%:*}" "${ca_##*:}" "$SEED" "$REP"
      done
    else
      REP_CELLS="$CELLS"
      [ "$REP" -gt "$FULL_REPS" ] && REP_CELLS="$EXTRA_CELLS"
      for CELL in $REP_CELLS; do
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
if [ -z "${TD9_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TD9ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TD9-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
