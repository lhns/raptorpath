#!/bin/bash
# THE THREADING-REDESIGN P0 BATTERY (docs/status.md §9, "Threading redesign
# P0 -- per-thread core budget -- pre-registration"): the P0 binary (named
# runtime + [THR]/[LAG]) against main's binary (8d7d8c1), interleaved in one
# session, both with RWM_RDIAG=1.
#
#   sudo env P0_SHA=<sha> MAIN_SHA=<sha> P0_LOCK_OWNER=<token> P0_BIN=<bin> \
#     MAIN_BIN=<bin> P0_OUTDIR=<dir> bash p0_battery.sh <reps>
#
# Launched by `p0_run_all.sh` (the envelope, which holds both locks), for the
# smoke (P0_TAG=smoke, P0_SMOKE_PLAN) and for the battery.
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1
# RWM_PERF_TIMEOUT_S=150, runs=1). Every arm first withholds RWM_EST_CADENCE
# RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RDIAG (rule 15d), then
# sets RWM_RDIAG=1 explicitly:
#   P0    RWM_BIN=$P0_BIN
#   MAIN  RWM_BIN=$MAIN_BIN
#
# PLAN: blocks (rep 1, s42), (rep 1, s7), (rep 2, s42) -> n = 3 per arm and
# cell; in every block the cells c1s-400 then c1d-400, each P0 and MAIN, the
# arm order rotated by the block index (rule 3).
#
# Per invocation the ledger gets the `===` header, the driver's summary /
# CPU / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a
# COTENANT line and ONE `P0ROW {json}` (p0_parse.py row). ABORT-SHA is checked
# before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh p0_battery.sh p0_parse.py stage3_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps}"
OUTDIR="${P0_OUTDIR:?P0_OUTDIR}"
TAG="${P0_TAG:-p0}"
TRIES="${P0_BRINGUP_TRIES:-2}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=p0_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${P0_LOCK_OWNER:-}" ] || ! grep -qF -- "$P0_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${P0_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $P0_LOCK_OWNER"

preflight_binary "${P0_BIN:?P0_BIN}" "PIPE] pipeline=" "estimator heavy-math cadence ACTIVE" \
    "RDIAG] busy=" "THR] rt phase=" "LAG] phase="
preflight_binary "${MAIN_BIN:?MAIN_BIN}" "PIPE] pipeline=" "estimator heavy-math cadence ACTIVE" "RDIAG] busy="
bin_of() { case "$1" in P0) echo "$P0_BIN" ;; MAIN) echo "$MAIN_BIN" ;; esac; }
sha_of() { case "$1" in P0) echo "${P0_SHA:-}" ;; MAIN) echo "${MAIN_SHA:-}" ;; esac; }
for A in P0 MAIN; do
  got="$(sha256sum "$(bin_of $A)" | cut -d' ' -f1)"
  if [ -z "$(sha_of $A)" ] || [ "$got" != "$(sha_of $A)" ]; then
    _lb_say "ABORT-SHA start: $A binary $got != '$(sha_of $A)'"
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
    *) echo "" ;;
  esac
}
CELLS="c1s-400 c1d-400"
ARMS=(P0 MAIN)
BLOCKS="${P0_BLOCKS:-1:42 1:7 2:42}"

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes t0 rc wall attempt sha bin want
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  bin="$(bin_of "$arm")"; want="$(sha_of "$arm")"
  local name="$cell-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$bin" | cut -d' ' -f1)"
    if [ "$sha" != "$want" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: binary $bin $sha != $want -- battery stopped"
      exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes bin=$bin sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/p0-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    # RWM_RTOBS=1: the [THR]/[LAG] instrument is opt-in since threading P2a
    # step 0 (the P0 binary ignores the variable: always on there).
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST -u RWM_RDIAG \
        RWM_RDIAG=1 RWM_RTOBS=1 \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/p0-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\]|--- RWM-C perf" /tmp/p0-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/p0-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./p0_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/p0-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "P0ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== THREAD-P0 BATTERY tag=$TAG blocks='$BLOCKS' cells='$CELLS' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary P0 $P0_BIN sha256 $P0_SHA" >> "$OUT"
echo "=== binary MAIN $MAIN_BIN sha256 $MAIN_SHA" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RDIAG, then RWM_RDIAG=1" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

BLOCK=0
for B in $BLOCKS; do
  REP="${B%%:*}"; SEED="${B##*:}"
  if [ -n "${P0_SMOKE_PLAN:-}" ]; then
    for ca_ in $P0_SMOKE_PLAN; do
      run_one "${ca_%%:*}" "${ca_##*:}" "$SEED" "$REP"
    done
  else
    for CELL in $CELLS; do
      k=$(( BLOCK % 2 ))
      for ((j = 0; j < 2; j++)); do
        run_one "$CELL" "${ARMS[$(( (j + k) % 2 ))]}" "$SEED" "$REP"
      done
    done
  fi
  echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  BLOCK=$(( BLOCK + 1 ))
done

echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
if [ -z "${P0_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS[@]}"; do
      N=$(grep -c "^P0ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "P0-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
