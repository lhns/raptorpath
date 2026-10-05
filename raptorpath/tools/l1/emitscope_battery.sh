#!/bin/bash
# THE EMISSION-BATCHING SCOPE BATTERY (docs/status.md §8, "Emission-batching
# scope (Law 0) -- pre-registration"): one binary, NEW (gate off) vs EB0
# (RWM_EMIT_BATCH=1, Law 0: the burst bound is emit_burst at every path
# count), interleaved in one session.
#
#   sudo env ES_SHA=<sha> ES_LOCK_OWNER=<token> ES_BIN=<bin> ES_OUTDIR=<dir> \
#     bash emitscope_battery.sh <reps-per-seed>
#
# Launched by `emitscope_run_all.sh` (the 5 h envelope, which holds both
# locks), for the smoke (ES_TAG=smoke, ES_SMOKE_PLAN) and for the battery.
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, `--window-reliable`,
# RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150, runs=1). Every arm first
# withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST
# (rule 15d); only EB0 sets RWM_EMIT_BATCH=1 back:
#   NEW   bulk, env unset (gate off)
#   EB0   bulk, RWM_EMIT_BATCH=1 (Law 0)
# (The Law-A fallback arm EBA is pre-registered in §8 but has no engine
# sub-gate yet; it is added here, with its own env, only if the stop rule
# fires.)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, arms ${ES_ARMS:-NEW EB0}
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT line and ONE
# `ESROW {json}` (emitscope_parse.py row).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh emitscope_battery.sh emitscope_parse.py stage3_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${ES_SEEDS:-42 7}"
OUTDIR="${ES_OUTDIR:?ES_OUTDIR}"
TAG="${ES_TAG:-es}"
TRIES="${ES_BRINGUP_TRIES:-2}"
BIN="${ES_BIN:?ES_BIN}"
ARMS_ALL="${ES_ARMS:-NEW EB0}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=emitscope_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${ES_LOCK_OWNER:-}" ] || ! grep -qF -- "$ES_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${ES_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $ES_LOCK_OWNER"

preflight_binary "$BIN" "PIPE] pipeline=" "estimator heavy-math cadence ACTIVE" \
    "emission batching ACTIVE" "eb_bursts="
got="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ -z "${ES_SHA:-}" ] || [ "$got" != "$ES_SHA" ]; then
  _lb_say "ABORT-SHA start: binary $got != '${ES_SHA:-unset}'"
  exit 5
fi
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
cell_arms() { echo "$ARMS_ALL"; }
CELLS_ALL="c1s-400 c1d-400 c2-100 c8-100"
CELLS="${ES_CELLS:-$CELLS_ALL}"

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes hint t0 rc wall attempt sha bin want
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  hint=bulk; bin="$BIN"; want="$ES_SHA"
  local name="$cell-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$bin" | cut -d' ' -f1)"
    if [ "$sha" != "$want" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: binary $bin $sha != $want -- battery stopped"
      exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=$hint spec=$ca/$cb/$mode bytes=$bytes bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/es-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    local aenv=""
    case "$arm" in
      EB0) aenv="RWM_EMIT_BATCH=1" ;;
    esac
    # shellcheck disable=SC2086
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        $aenv \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/es-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/es-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/es-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./emitscope_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/es-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "ESROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== ES BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' arms='$ARMS_ALL' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary ES $BIN sha256 $ES_SHA" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST; EB0: RWM_EMIT_BATCH=1; soft_deadline=${ES_SOFT_DEADLINE:-none} block_est=${ES_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${ES_SOFT_DEADLINE:-}" ] && [ -n "${ES_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + ES_BLOCK_EST_S )) -gt "$ES_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${ES_BLOCK_EST_S}s > deadline $ES_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${ES_SMOKE_PLAN:-}" ]; then
      for ca_ in $ES_SMOKE_PLAN; do
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
if [ -z "${ES_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in $(cell_arms "$CELL"); do
      N=$(grep -c "^ESROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "ES-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
