#!/bin/bash
# THE V-P1 BATTERY (docs/status.md, "Threading P1 — pre-registration"): the
# threading-P1 binary (P1) against `main` (MAIN), interleaved in one session.
#
#   sudo env TP1_SHA_P1=<sha> TP1_SHA_MAIN=<sha> TP1_LOCK_OWNER=<token> \
#     TP1_BIN_P1=<bin> TP1_BIN_MAIN=<bin> TP1_OUTDIR=<dir> bash threadp1_battery.sh <reps-per-seed>
#
# Launched by `threadp1_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TP1_TAG=smoke, TP1_SMOKE_PLAN) and for the battery.
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Both arms run the SHIPPED DEFAULT: every arm withholds
# RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST (measurement
# discipline 15d) and sets none back. The arms differ by binary only:
#   MAIN   main 8d7d8c1
#   P1     feat/thread-p1 (the pre-registration commit)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN P1.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT line and ONE
# `TP1ROW {json}` (threadp1_parse.py row). ABORT-SHA is checked for the arm's
# binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadp1_battery.sh threadp1_parse.py stage3_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${TP1_SEEDS:-42 7}"
OUTDIR="${TP1_OUTDIR:?TP1_OUTDIR}"
TAG="${TP1_TAG:-tp1}"
TRIES="${TP1_BRINGUP_TRIES:-2}"
BP1="${TP1_BIN_P1:?TP1_BIN_P1}"
BMAIN="${TP1_BIN_MAIN:?TP1_BIN_MAIN}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadp1_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TP1_LOCK_OWNER:-}" ] || ! grep -qF -- "$TP1_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TP1_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TP1_LOCK_OWNER"

preflight_binary "$BP1" "PIPE] pipeline=" "emission batching ACTIVE"
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE"
for pair in "P1:$BP1:${TP1_SHA_P1:-}" "MAIN:$BMAIN:${TP1_SHA_MAIN:-}"; do
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
arm_bin() { case "$1" in P1) echo "$BP1" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in P1) echo "$TP1_SHA_P1" ;; *) echo "$TP1_SHA_MAIN" ;; esac; }
CELLS="${TP1_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
ARMS_ALL=(MAIN P1)

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
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/tp1-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/tp1-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/tp1-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/tp1-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./threadp1_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/tp1-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TP1ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V-P1 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary P1 $BP1 sha256 $TP1_SHA_P1" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TP1_SHA_MAIN" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST (shipped default); soft_deadline=${TP1_SOFT_DEADLINE:-none} block_est=${TP1_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TP1_SOFT_DEADLINE:-}" ] && [ -n "${TP1_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TP1_BLOCK_EST_S )) -gt "$TP1_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TP1_BLOCK_EST_S}s > deadline $TP1_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TP1_SMOKE_PLAN:-}" ]; then
      for ca_ in $TP1_SMOKE_PLAN; do
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
if [ -z "${TP1_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TP1ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TP1-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
