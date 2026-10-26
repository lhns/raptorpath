#!/bin/bash
# THE C1 ANCHOR OVER-READ DIAGNOSTIC (docs/status.md, "23. The c1 anchor
# over-read and the store law's input -- diagnosis -- pre-registration").
# Derived from storelaw_battery.sh. ONE binary (main, built fresh).
#
#   sudo env AD_BIN=<bin> AD_SHA=<sha256> AD_OUTDIR=<dir> AD_TAG=<tag> \
#     [AD_SMOKE=1] bash anchordiag_battery.sh
#
# Arms: MAIN (no arm env), PRS (RWM_PLAIN_RS=1 RWM_HONEST_CAP=0), S<n> (RWM_STORE=<n>).
# PLAN (64 invocations; the smoke is 3), blocks (seed,rep) = (42,1) (7,1) (42,2):
#   per block: c1s-400 c2-100 c1d-400 c8-100, each {MAIN,PRS} (order flips on odd
#   blocks); then the c1s-400 store sweep (ascending on even blocks, descending
#   on odd); then, blocks 0 and 1 only, the c2-100 sweep.
# Launched by anchordiag_run_all.sh (the envelope, which holds both VM locks).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh anchordiag_battery.sh anchordiag_parse.py stage3_parse.py \
    storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py

BIN="${AD_BIN:?AD_BIN}"
WANT="${AD_SHA:?AD_SHA}"
OUTDIR="${AD_OUTDIR:?AD_OUTDIR}"
TAG="${AD_TAG:-ad}"
TRIES="${AD_BRINGUP_TRIES:-2}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=anchordiag_battery
LB_LOG="$OUT"

# The instruments and gates the arms need must be in the binary.
preflight_binary "$BIN" "plain-mode send-interval SAMPLER ACTIVE" "RSTRACE-LEGACY" "ACKDIAG"
GOT="$(sha256sum "$BIN" | cut -d' ' -f1)"
[ "$GOT" = "$WANT" ] || { _lb_say "ABORT-SHA start: $GOT != $WANT"; exit 5; }
if pgrep -x raptorpath >/dev/null 2>&1; then
  _lb_say "BUSY: raptorpath already running -- aborting"; exit 3
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

run_one() { # cell arm seed rep     (arm: MAIN|PRS|S<n>)
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes t0 rc wall attempt sha thr
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  local name="$cell-$arm"
  local arm_env=()
  case "$arm" in
    MAIN) ;;
    PRS) arm_env=(RWM_PLAIN_RS=1 RWM_HONEST_CAP=0) ;;
    S[0-9]*) arm_env=(RWM_STORE="${arm#S}") ;;
    *) echo "UNKNOWN-ARM $arm" >> "$OUT"; return 0 ;;
  esac
  case "$cell" in c1s-400|c1d-400) thr=195000 ;; *) thr=19500 ;; esac   # 2x the link datagram rate
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$WANT" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: $sha != $WANT -- battery stopped"; exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes arm_env='${arm_env[*]}' rs_trace=$thr bin=$BIN sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/ad-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_STORE_PATHS \
        -u RWM_PLAIN_RS -u RWM_HONEST_CAP -u RWM_STORE \
        "${arm_env[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1 RWM_RS_TRACE="$thr" \
        RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/ad-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/ad-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/ad-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./anchordiag_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/ad-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "ADROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== ANCHOR-DIAG BATTERY tag=$TAG smoke=${AD_SMOKE:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary $BIN sha256 $WANT" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1 RWM_RS_TRACE=<thr> RWM_PERF_TIMEOUT_S=150 bulk window runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT RWM_STORE_PATHS RWM_PLAIN_RS RWM_HONEST_CAP RWM_STORE; PRS sets RWM_PLAIN_RS=1 RWM_HONEST_CAP=0; S<n> sets RWM_STORE=<n>" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

if [ "${AD_SMOKE:-0}" = "1" ]; then
  run_one c1s-400 MAIN 42 1
  run_one c1s-400 PRS 42 1
  run_one c2-100 S256 42 1
  echo "AD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

GRID_C1S=(192 256 384 512 768 1024 1536 2048)
GRID_C2=(64 96 128 192 256 384 512 1024)
sweep() { # cell block seed rep n...   (ascending on even blocks, descending on odd)
  local cell="$1" b="$2" seed="$3" rep="$4" n i
  shift 4
  local grid=("$@")
  if [ $(( b % 2 )) -eq 0 ]; then
    for n in "${grid[@]}"; do run_one "$cell" "S$n" "$seed" "$rep"; done
  else
    for (( i=${#grid[@]}-1; i>=0; i-- )); do run_one "$cell" "S${grid[$i]}" "$seed" "$rep"; done
  fi
}

BLOCK=0
for SPEC in "42 1" "7 1" "42 2"; do
  read -r SEED REP <<< "$SPEC"
  if [ $(( BLOCK % 2 )) -eq 0 ]; then FIRST=MAIN; SECOND=PRS; else FIRST=PRS; SECOND=MAIN; fi
  for CELL in c1s-400 c2-100 c1d-400 c8-100; do
    run_one "$CELL" "$FIRST" "$SEED" "$REP"
    run_one "$CELL" "$SECOND" "$SEED" "$REP"
  done
  sweep c1s-400 "$BLOCK" "$SEED" "$REP" "${GRID_C1S[@]}"
  [ "$BLOCK" -lt 2 ] && sweep c2-100 "$BLOCK" "$SEED" "$REP" "${GRID_C2[@]}"
  echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  BLOCK=$(( BLOCK + 1 ))
done
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
grep -ac '^ADROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
echo "AD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
