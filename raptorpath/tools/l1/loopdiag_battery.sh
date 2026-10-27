#!/bin/bash
# THE LOOP-TIME DIAGNOSTIC (docs/status.md §23 follow-up; paper §6.1 "The law
# is fed by its own output"). Derived from anchordiag_battery.sh. ONE binary
# (main + the RWM_LOOPDIAG instrument, built fresh).
#
#   sudo env LD_BIN=<bin> LD_SHA=<sha256> LD_OUTDIR=<dir> LD_TAG=<tag> \
#     [LD_SMOKE=1] bash loopdiag_battery.sh
#
# Per path and per 250 ms [DIAG] window, [LOOPDIAG] prints the echo-RTT loop
# time (min/p10/p50), Copa's RTprop/jitter/srtt, the delivered rate and quinn's
# own BBR pacing rate side by side.
# Arms: MAIN (no arm env), S<n> (RWM_STORE=<n>).
# PLAN (46 invocations; the smoke is 2), blocks (seed,rep) = (42,1) (7,1) (42,2):
#   per block: c1s-400 MAIN, c2-100 MAIN; then the c1s-400 store sweep
#   n in {192,256,384,512,768,1024,1536,2048} (ascending on even blocks,
#   descending on odd); then, blocks 0 and 1 only, the c2-100 sweep
#   n in {64,96,128,192,256,384,512,1024} (same direction rule).
#   3 x (2 + 8) + 2 x 8 = 46.
# Launched by loopdiag_run_all.sh (the envelope, which holds both VM locks).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh loopdiag_battery.sh loopdiag_parse.py anchordiag_parse.py stage3_parse.py \
    storelaw_parse.py perf_rwm_c.sh topo.sh topo_dual.sh l1common.py

BIN="${LD_BIN:?LD_BIN}"
WANT="${LD_SHA:?LD_SHA}"
OUTDIR="${LD_OUTDIR:?LD_OUTDIR}"
TAG="${LD_TAG:-ld}"
TRIES="${LD_BRINGUP_TRIES:-2}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=loopdiag_battery
LB_LOG="$OUT"

# The instruments and gates the arms need must be in the binary.
preflight_binary "$BIN" "ACKDIAG" "LOOPDIAG"
GOT="$(sha256sum "$BIN" | cut -d' ' -f1)"
[ "$GOT" = "$WANT" ] || { _lb_say "ABORT-SHA start: $GOT != $WANT"; exit 5; }
if pgrep -x raptorpath >/dev/null 2>&1; then
  _lb_say "BUSY: raptorpath already running -- aborting"; exit 3
fi

cell_spec() { # cell -> "scenA scenB mode bytes"
  case "$1" in
    c1s-400) echo "c1 c1 single 400000000" ;;
    c2-100)  echo "c2 c2 single 100000000" ;;
    *) echo "" ;;
  esac
}

# /proc/pressure/io `full avg10`: a per-row covariate (flagged in the parser, never a void).
io_full10() { awk '/^full/{for(i=1;i<=NF;i++) if($i ~ /^avg10=/){sub("avg10=","",$i); print $i}}' /proc/pressure/io 2>/dev/null || echo -1; }

run_one() { # cell arm seed rep     (arm: MAIN|S<n>)
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes t0 rc wall attempt sha
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  local name="$cell-$arm"
  local arm_env=()
  case "$arm" in
    MAIN) ;;
    S[0-9]*) arm_env=(RWM_STORE="${arm#S}") ;;
    *) echo "UNKNOWN-ARM $arm" >> "$OUT"; return 0 ;;
  esac
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$WANT" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: $sha != $WANT -- battery stopped"; exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=window hint=bulk spec=$ca/$cb/$mode bytes=$bytes arm_env='${arm_env[*]}' bin=$BIN sha=${sha:0:16} $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/ld-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    io_b=$(io_full10)
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_ACKDIAG -u RWM_LOOPDIAG -u RWM_IO_RT -u RWM_STORE_PATHS \
        -u RWM_PLAIN_RS -u RWM_HONEST_CAP -u RWM_STORE \
        "${arm_env[@]}" \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LOOPDIAG=1 \
        RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window RWM_BIN="$BIN" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > /tmp/ld-drv.out 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    io_a=$(io_full10)
    io_max=$(awk -v a="$io_b" -v b="$io_a" 'BEGIN{print (a>b)?a:b}')
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    echo "IOPRESSURE $name s$seed rep=$rep full_avg10_before=$io_b full_avg10_after=$io_a $(grep '^full' /proc/pressure/io)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/ld-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/ld-drv.out "$base-drv.out" 2>/dev/null || true
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
  ondisk=0
  case "$BIN" in /dev/shm/*) ;; *) ondisk=1 ;; esac
  case "$OUTDIR" in /dev/shm/*) ;; *) ondisk=1 ;; esac
  LD_ONDISK="$ondisk" LD_IO_FULL10="$io_max" python3 ./loopdiag_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/ld-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "LDROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== LOOP-DIAG BATTERY tag=$TAG smoke=${LD_SMOKE:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary $BIN sha256 $WANT" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LOOPDIAG=1 RWM_PERF_TIMEOUT_S=150 bulk window runs=1; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_LOOPDIAG RWM_IO_RT RWM_STORE_PATHS RWM_PLAIN_RS RWM_HONEST_CAP RWM_STORE; S<n> sets RWM_STORE=<n>" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

if [ "${LD_SMOKE:-0}" = "1" ]; then
  run_one c1s-400 MAIN 42 1
  run_one c2-100 S192 42 1
  echo "LD-BATTERY-DONE tag=$TAG blocks=0 $(date -u +%FT%TZ)" >> "$OUT"
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
  run_one c1s-400 MAIN "$SEED" "$REP"
  run_one c2-100 MAIN "$SEED" "$REP"
  sweep c1s-400 "$BLOCK" "$SEED" "$REP" "${GRID_C1S[@]}"
  [ "$BLOCK" -lt 2 ] && sweep c2-100 "$BLOCK" "$SEED" "$REP" "${GRID_C2[@]}"
  echo "BLOCK-COMPLETE rep=$REP seed=$SEED block=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
  BLOCK=$(( BLOCK + 1 ))
done
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
grep -ac '^LDROW ' "$OUT" | sed 's/^/ROWS /' >> "$OUT"
echo "LD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
