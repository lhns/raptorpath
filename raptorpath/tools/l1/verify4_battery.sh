#!/bin/bash
# THE V4 VERIFICATION BATTERY (docs/status.md §6, "V4 verification —
# pre-registration"): the window-only, cadence-default binary (NEW) against
# the Stage-3 binary (OLD), interleaved in one session.
#
#   sudo env V4_SHA_NEW=<sha> V4_SHA_OLD=<sha> V4_LOCK_OWNER=<token> \
#     V4_BIN_NEW=<bin> V4_BIN_OLD=<bin> V4_OUTDIR=<dir> bash verify4_battery.sh <reps-per-seed>
#
# Launched by `verify4_run_all.sh` (the 5 h envelope, which holds both locks),
# for the smoke (V4_TAG=smoke, V4_SMOKE_PLAN) and for the battery.
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150, runs=1).
# Every arm first withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST (measurement discipline 15d); only EMB sets one back:
#   NEW    NEW binary, bulk, env unset (= the shipped default: cadence ON)
#   OLD    OLD binary (Stage 3, f3743664...), bulk, env unset (cadence OFF,
#          no OFF echo in that binary)
#   NEWa   NEW binary, auto
#   OLDa   OLD binary, auto
#   EMB    NEW binary, bulk, RWM_EMIT_BATCH=1
#
# PLAN per (rep, seed) block -- 19 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400  NEW OLD EMB
#   c1d-400  NEW OLD
#   c2-100   NEW OLD NEWa OLDa EMB
#   c3-25    NEW OLD NEWa OLDa EMB
#   c7-100   NEW OLD
#   c8-100   NEW OLD
# V4_NO_EMB=1 / V4_NO_AUTO=1 are the pre-registered cuts.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT line and ONE
# `V4ROW {json}` (verify4_parse.py row). ABORT-SHA is checked for the arm's
# binary before every invocation; the other abort tokens are stage3_battery's.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh verify4_battery.sh verify4_parse.py stage3_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${V4_SEEDS:-42 7}"
OUTDIR="${V4_OUTDIR:?V4_OUTDIR}"
TAG="${V4_TAG:-v4}"
TRIES="${V4_BRINGUP_TRIES:-2}"
BNEW="${V4_BIN_NEW:?V4_BIN_NEW}"
BOLD="${V4_BIN_OLD:?V4_BIN_OLD}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=verify4_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${V4_LOCK_OWNER:-}" ] || ! grep -qF -- "$V4_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${V4_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $V4_LOCK_OWNER"

preflight_binary "$BNEW" "PIPE] pipeline=" "estimator heavy-math cadence ACTIVE" \
    "estimator heavy-math cadence OFF" "emission batching ACTIVE"
preflight_binary "$BOLD" "PIPE] pipeline="
for pair in "NEW:$BNEW:${V4_SHA_NEW:-}" "OLD:$BOLD:${V4_SHA_OLD:-}"; do
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
    c3-25)   echo "c3 c3 single 25000000" ;;
    c7-100)  echo "c2 c2 dual 100000000" ;;
    c8-100)  echo "c2 c3 dual 100000000" ;;
    *) echo "" ;;
  esac
}
cell_arms() { # cell -> the arms run there
  local emb="EMB" au="NEWa OLDa"
  [ "${V4_NO_EMB:-0}" = "1" ] && emb=""
  [ "${V4_NO_AUTO:-0}" = "1" ] && au=""
  case "$1" in
    c1s-400) echo "NEW OLD $emb" ;;
    c2-100|c3-25) echo "NEW OLD $au $emb" ;;
    *) echo "NEW OLD" ;;
  esac
}
arm_hint() { case "$1" in NEWa|OLDa) echo auto ;; *) echo bulk ;; esac; }
arm_bin() { case "$1" in OLD|OLDa) echo "$BOLD" ;; *) echo "$BNEW" ;; esac; }
arm_sha() { case "$1" in OLD|OLDa) echo "$V4_SHA_OLD" ;; *) echo "$V4_SHA_NEW" ;; esac; }
CELLS_ALL="c1s-400 c1d-400 c2-100 c3-25 c7-100 c8-100"
CELLS="${V4_CELLS:-$CELLS_ALL}"

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes hint t0 rc wall attempt sha bin want
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  hint="$(arm_hint "$arm")"; bin="$(arm_bin "$arm")"; want="$(arm_sha "$arm")"
  local name="$cell-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$bin" | cut -d' ' -f1)"
    if [ "$sha" != "$want" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: binary $bin $sha != $want -- battery stopped"
      exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=$hint spec=$ca/$cb/$mode bytes=$bytes bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/v4-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    if [ "$arm" = "EMB" ]; then
      env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
          RWM_EMIT_BATCH=1 \
          SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
          RWM_C_PIPELINE=window RWM_BIN="$bin" \
        bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/v4-drv.out 2>&1
    else
      env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
          SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
          RWM_C_PIPELINE=window RWM_BIN="$bin" \
        bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/v4-drv.out 2>&1
    fi
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/v4-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/v4-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./verify4_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/v4-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "V4ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V4 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' no_emb=${V4_NO_EMB:-0} no_auto=${V4_NO_AUTO:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary NEW $BNEW sha256 $V4_SHA_NEW" >> "$OUT"
echo "=== binary OLD $BOLD sha256 $V4_SHA_OLD" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST; EMB: RWM_EMIT_BATCH=1; soft_deadline=${V4_SOFT_DEADLINE:-none} block_est=${V4_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${V4_SOFT_DEADLINE:-}" ] && [ -n "${V4_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + V4_BLOCK_EST_S )) -gt "$V4_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${V4_BLOCK_EST_S}s > deadline $V4_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${V4_SMOKE_PLAN:-}" ]; then
      for ca_ in $V4_SMOKE_PLAN; do
        run_one "${ca_%%:*}" "${ca_##*:}" "$SEED" "$REP"
      done
    else
      for CELL in $CELLS; do
        read -r -a ARMS_ <<< "$(cell_arms "$CELL")"
        n=${#ARMS_[@]}
        k=$(( BLOCK % n ))
        for ((j = 0; j < n; j++)); do
          run_one "$CELL" "${ARMS_[$(( (j + k) % n ))]}" "$SEED" "$REP"
        done
      done
    fi
    echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( BLOCK + 1 ))
  done
  [ "$STOP" -eq 1 ] && break
done

echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
if [ -z "${V4_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in $(cell_arms "$CELL"); do
      N=$(grep -c "^V4ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "V4-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
