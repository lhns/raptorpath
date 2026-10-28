#!/bin/bash
# THE V-PD BATTERY (paper 5.2 "Retained state: the decode-inert floor,
# applied per drain"): the receiver's decoder prune at the loop's wait point
# (once per drained batch) with the decoder floor, against `main` (MAIN),
# interleaved in one session. Derived from threadsk_battery.sh.
#
#   sudo env TPD_SHA_MAIN=<sha> TPD_SHA_NEW=<sha> TPD_LOCK_OWNER=<token> \
#     TPD_BIN_MAIN=<bin> TPD_BIN_NEW=<bin> TPD_OUTDIR=<dir> \
#     bash threadpd_battery.sh <reps-per-seed>
#
# Launched by `threadpd_run_all.sh` (the envelope, which holds both locks),
# for the smoke (TPD_TAG=smoke, TPD_SMOKE_PLAN) and the scored battery
# (TPD_TAG=tpd).
#
# ARMS (fresh topology per invocation; perf_rwm_c.sh, window pipeline =
# `--window-reliable`, bulk, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150,
# runs=1). Every arm withholds RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
# RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG RWM_IO_RT (measurement
# discipline 15d: the shipped default) and then sets RWM_RTOBS=1 and
# RWM_RDIAG=1 on every arm (never RWM_RTOBS=2: its per-section clock reads
# would perturb the receiver under test):
#   MAIN   github/main        (binary main)
#   NEW    the per-drain-prune tree (binary new)
#
# PLAN per (rep, seed) block -- 8 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400 c1d-400 c2-100 c8-100, each MAIN NEW.
#
# Per invocation the ledger gets the `===` header, the driver's summary / CPU
# / [TRUTH] / [THR] sum / [LAG] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT
# line and ONE `TPDROW {json}` (threadpd_parse.py row). ABORT-SHA is checked
# for the arm's binary before every invocation.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh threadpd_battery.sh threadpd_parse.py threadq1_parse.py \
    threadp1_parse.py stage3_parse.py perf_rwm_c.sh topo_dual.sh l1common.py

# /proc/pressure/io `full` field value (avg10 | avg60 | avg300), empty if unread.
io_full() { awk -v k="$1" '/^full/{for(i=2;i<=NF;i++){split($i,a,"=");if(a[1]==k)print a[2]}}' /proc/pressure/io 2>/dev/null; }

REPS="${1:?reps per seed}"
SEEDS="${TPD_SEEDS:-42 7}"
OUTDIR="${TPD_OUTDIR:?TPD_OUTDIR}"
TAG="${TPD_TAG:-tpd}"
TRIES="${TPD_BRINGUP_TRIES:-2}"
ACKD=0
BMAIN="${TPD_BIN_MAIN:?TPD_BIN_MAIN}"
BNEW="${TPD_BIN_NEW:?TPD_BIN_NEW}"
OUT="$OUTDIR/${TAG}.log"
DRV="$OUTDIR/tpd-drv.out"   # under the (tmpfs) run dir; perf_rwm_c.sh itself writes /tmp/rwm-{c,s}.log
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=threadpd_battery
LB_LOG="$OUT"

VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${TPD_LOCK_OWNER:-}" ] || ! grep -qF -- "$TPD_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${TPD_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $TPD_LOCK_OWNER"

# Two-sided binary witness: the fix's decoder floor gauge (`below_floor=` in
# the decoder's diag_stats) and the [RDIAG] prune witness must be in NEW and
# `below_floor=` absent from MAIN.
preflight_binary "$BNEW" "PIPE] pipeline=" "emission batching ACTIVE" "ack_batches=" "below_floor=" "prune_max_us="
preflight_binary "$BMAIN" "PIPE] pipeline=" "emission batching ACTIVE" "ack_batches="
if grep -aq -- "below_floor=" "$BMAIN" 2>/dev/null; then
  _lb_say "REFUSED: below_floor= is present in the MAIN binary -- the arms are not distinct"
  exit 5
fi
for pair in "NEW:$BNEW:${TPD_SHA_NEW:-}" "MAIN:$BMAIN:${TPD_SHA_MAIN:-}"; do
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
arm_bin() { case "$1" in NEW) echo "$BNEW" ;; *) echo "$BMAIN" ;; esac; }
arm_sha() { case "$1" in NEW) echo "$TPD_SHA_NEW" ;; *) echo "$TPD_SHA_MAIN" ;; esac; }
CELLS="${TPD_CELLS:-c1s-400 c1d-400 c2-100 c8-100}"
ARMS_ALL=(MAIN NEW)

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes t0 rc wall attempt sha bin want
  local cot_b=0 cot_a=0 cot=0 io_b=""
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
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log $DRV
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    io_b=$(io_full avg10)
    t0=$(date +%s)
    env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST \
        -u RWM_RTOBS -u RWM_RDIAG -u RWM_ACKDIAG -u RWM_IO_RT \
        RWM_RTOBS=1 RWM_RDIAG=1 \
        SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE=window RWM_BIN="$bin" \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" > $DRV 2>&1
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    # io pressure is a covariate (rows run from memory), never a void.
    echo "IOP $name s$seed rep=$rep before=${io_b:-} after=$(io_full avg10)" >> "$OUT"
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|\[THR\] sum|\[LAG\] phase|--- RWM-C perf" $DRV >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp $DRV "$base-drv.out" 2>/dev/null || true
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
  python3 ./threadpd_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      $DRV /tmp/rwm-c.log /tmp/rwm-s.log "$cot" "$sha" >> "$OUT" 2>&1 \
    || echo "TPDROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== V-PD BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' ackdiag=$ACKD $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary MAIN $BMAIN sha256 $TPD_SHA_MAIN" >> "$OUT"
echo "=== binary NEW $BNEW sha256 $TPD_SHA_NEW" >> "$OUT"
echo "=== tmpfs outdir=$(stat -f -c %T "$OUTDIR" 2>/dev/null) bins main=$(stat -f -c %T "$BMAIN" 2>/dev/null) new=$(stat -f -c %T "$BNEW" 2>/dev/null) tmp=$(stat -f -c %T /tmp 2>/dev/null)" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1 bulk --window-reliable; every arm env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG RWM_IO_RT (shipped default), then RWM_RTOBS=1 RWM_RDIAG=1; soft_deadline=${TPD_SOFT_DEADLINE:-none} block_est=${TPD_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags|^CPU\(s\)' | head -3 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${TPD_SOFT_DEADLINE:-}" ] && [ -n "${TPD_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + TPD_BLOCK_EST_S )) -gt "$TPD_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${TPD_BLOCK_EST_S}s > deadline $TPD_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${TPD_SMOKE_PLAN:-}" ]; then
      for ca_ in $TPD_SMOKE_PLAN; do
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
if [ -z "${TPD_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in "${ARMS_ALL[@]}"; do
      N=$(grep -c "^TPDROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "TPD-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
