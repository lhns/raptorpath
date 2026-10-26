#!/bin/bash
# THE ONE-STORE-LAW BATTERY (docs/status.md, "18. One store law at every path
# count (ADR-0072) -- pre-registration", as amended). Derived from
# threadd9_battery.sh. ONE binary: LAW = the shipped defaults, CTL =
# RWM_STORE_PATHS=0 (main's single-path chain at N = 1; scored at c1s only).
#
#   sudo env SL_BIN=<bin> SL_SHA=<sha256> SL_OUTDIR=<dir> SL_TAG=<tag> \
#     [SL_SMOKE=1] bash storelaw_battery.sh
#
# SECTION 19 (same-session control): additionally SL_PLAN=19 SL_BIN_MAIN=<ce60079
# binary> SL_SHA_MAIN=<its sha256>. Arm MAIN runs that binary with no gate env;
# arm LAW runs SL_BIN. Plan: per (rep, seed) block, c1s-400 c1d-400 c2-100 c8-100,
# each {MAIN, LAW} with the arm order rotated by the block index (48), then the
# Realtime rerun: seed 42, reps 1..3, c1s-400 and c2-100, LAW (6) = 54.
#
# Launched by storelaw_run_all.sh (the envelope, which holds both VM locks).
#
# PLAN (the pre-registered 42 invocations; the smoke is 3):
#   per (rep 1..3, seed 42|7) block: c1s-400 {LAW,CTL} (order rotated by the
#     block index), then c1d-400 LAW, c2-100 LAW, c8-100 LAW   -> 5 x 6 = 30
#   delta leg: seed 42, reps 1..3, hint auto and realtime, c1s-400 LAW and
#     c2-100 LAW                                               -> 12
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh storelaw_battery.sh storelaw_parse.py stage3_parse.py \
    perf_rwm_c.sh topo.sh topo_dual.sh l1common.py

BIN="${SL_BIN:?SL_BIN}"
WANT="${SL_SHA:?SL_SHA}"
OUTDIR="${SL_OUTDIR:?SL_OUTDIR}"
TAG="${SL_TAG:-sl}"
TRIES="${SL_BRINGUP_TRIES:-2}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=storelaw_battery
LB_LOG="$OUT"

PLAN="${SL_PLAN:-18}"
BIN_MAIN="${SL_BIN_MAIN:-}"
WANT_MAIN="${SL_SHA_MAIN:-}"
preflight_binary "$BIN" "PIPE] pipeline=" "RWM_STORE_PATHS" "path-scaled outstanding pool ACTIVE"
GOT="$(sha256sum "$BIN" | cut -d' ' -f1)"
[ "$GOT" = "$WANT" ] || { _lb_say "ABORT-SHA start: $GOT != $WANT"; exit 5; }
if [ "$PLAN" = "19" ]; then
  [ -n "$BIN_MAIN" ] && [ -n "$WANT_MAIN" ] || { _lb_say "ABORT-SHA plan 19 needs SL_BIN_MAIN/SL_SHA_MAIN"; exit 5; }
  preflight_binary "$BIN_MAIN" "PIPE] pipeline=" "RWM_STORE_PATHS" "path-scaled outstanding pool ACTIVE"
  GOTM="$(sha256sum "$BIN_MAIN" | cut -d' ' -f1)"
  [ "$GOTM" = "$WANT_MAIN" ] || { _lb_say "ABORT-SHA start: main $GOTM != $WANT_MAIN"; exit 5; }
  [ "$GOTM" != "$GOT" ] || { _lb_say "ABORT-SHA main and law binaries are byte-identical"; exit 5; }
fi
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

run_one() { # cell arm hint seed rep     (arm: LAW|CTL; the parser arm name carries the hint)
  local cell="$1" arm="$2" hint="$3" seed="$4" rep="$5" ca cb mode bytes t0 rc wall attempt sha
  local cot_b=0 cot_a=0 cot=0 parm="$arm"
  [ "$hint" = "bulk" ] || parm="$arm-$hint"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  local name="$cell-$parm"
  local store_env=()
  [ "$arm" = "CTL" ] && store_env=(RWM_STORE_PATHS=0)
  local BIN="$BIN" WANT="$WANT"      # per-arm binary (MAIN is a different build in plan 19)
  if [ "$arm" = "MAIN" ]; then BIN="$BIN_MAIN"; WANT="$WANT_MAIN"; fi
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$WANT" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: $sha != $WANT -- battery stopped"; exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$parm attempt=$attempt pipeline=window hint=$hint spec=$ca/$cb/$mode bytes=$bytes store_env='${store_env[*]}' bin=$BIN sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/sl-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_IO_RT -u RWM_STORE_PATHS \
        "${store_env[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$BIN" \
      bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/sl-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/sl-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/sl-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./storelaw_parse.py row "$cell" "$parm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/sl-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "SLROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== STORE-LAW BATTERY tag=$TAG smoke=${SL_SMOKE:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary LAW $BIN sha256 $WANT" >> "$OUT"
[ "$PLAN" = "19" ] && echo "=== binary MAIN $BIN_MAIN sha256 $WANT_MAIN" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1 window; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT RWM_STORE_PATHS (shipped default); CTL sets RWM_STORE_PATHS=0" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

if [ "$PLAN" = "19" ] && [ "${SL_SMOKE:-0}" = "1" ]; then
  run_one c1s-400 MAIN bulk 42 1
  run_one c1s-400 LAW bulk 42 1
  run_one c2-100 MAIN bulk 42 1
  run_one c2-100 LAW bulk 42 1
  run_one c1s-400 LAW realtime 42 1
  echo "SL-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi
if [ "$PLAN" = "19" ]; then
  BLOCK=0
  for REP in 1 2 3; do
    for SEED in 42 7; do
      if [ $(( BLOCK % 2 )) -eq 0 ]; then FIRST=MAIN; SECOND=LAW; else FIRST=LAW; SECOND=MAIN; fi
      for CELL in c1s-400 c1d-400 c2-100 c8-100; do
        run_one "$CELL" "$FIRST" bulk "$SEED" "$REP"
        run_one "$CELL" "$SECOND" bulk "$SEED" "$REP"
      done
      echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
      BLOCK=$(( BLOCK + 1 ))
    done
  done
  for REP in 1 2 3; do
    run_one c1s-400 LAW realtime 42 "$REP"
    run_one c2-100 LAW realtime 42 "$REP"
  done
  echo "DELTA-BLOCK-COMPLETE $(date -u +%FT%TZ)" >> "$OUT"
  echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
  grep -ac '^SLROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
  echo "SL-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi
if [ "${SL_SMOKE:-0}" = "1" ]; then
  run_one c1s-400 LAW bulk 42 1
  run_one c1s-400 CTL bulk 42 1
  run_one c2-100 LAW bulk 42 1
  echo "SL-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
  exit 0
fi

BLOCK=0
for REP in 1 2 3; do
  for SEED in 42 7; do
    if [ $(( BLOCK % 2 )) -eq 0 ]; then FIRST=LAW; SECOND=CTL; else FIRST=CTL; SECOND=LAW; fi
    run_one c1s-400 "$FIRST" bulk "$SEED" "$REP"
    run_one c1s-400 "$SECOND" bulk "$SEED" "$REP"
    run_one c1d-400 LAW bulk "$SEED" "$REP"
    run_one c2-100 LAW bulk "$SEED" "$REP"
    run_one c8-100 LAW bulk "$SEED" "$REP"
    echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
    BLOCK=$(( BLOCK + 1 ))
  done
done
for REP in 1 2 3; do
  for HINT in auto realtime; do
    run_one c1s-400 LAW "$HINT" 42 "$REP"
    run_one c2-100 LAW "$HINT" 42 "$REP"
  done
  echo "DELTA-BLOCK-COMPLETE rep=$REP $(date -u +%FT%TZ)" >> "$OUT"
done
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
grep -ac '^SLROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
echo "SL-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
