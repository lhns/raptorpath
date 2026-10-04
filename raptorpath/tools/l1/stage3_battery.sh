#!/bin/bash
# THE STAGE-3 BASELINE BATTERY (docs/status.md §5, "Stage-3 baseline —
# pre-registration", and its amendments).
#
#   sudo env S3_SHA=<sha256> S3_LOCK_OWNER=<token> RWM_BIN=<bin> \
#     bash stage3_battery.sh <reps-per-seed>
#
# Launched by `stage3_run_all.sh` (the 5 h envelope, which holds both locks),
# for the smoke (S3_TAG=smoke, S3_SMOKE=1) and for the battery.
#
# ARMS (one binary, fresh topology per invocation; perf_rwm_c.sh builds and
# tears down its namespaces). Every arm: RWM_GEN=0 RWM_DIAG=1
# RWM_PERF_TIMEOUT_S=150 SEED=<seed>, and RWM_EST_CADENCE / RWM_POOL_ANCHOR
# are `unset` first (measurement discipline 15d) and then set only by CAD:
#   A1, A2  window pipeline, bulk          (the CTL, run twice: the A/A)
#   CAD     window pipeline, bulk, RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0
#   WINa    window pipeline, auto
# Every arm is the window pipeline: the block pipeline (and its BLKb/BLKa
# arms) was deleted by ADR-0069; that plan is in git history.
#
# PLAN per (rep, seed) block -- 22 invocations; cells in this order, the arm
# order within every cell rotated by the block index (rule 3):
#   c1s-400  A1 A2 WINa                  (c1 single, 400 MB)
#   c1d-400  A1 A2 CAD                   (c1 || c1 dual, 400 MB)
#   c2-100   A1 A2 CAD WINa              (c2 single, 100 MB)
#   c3-25    A1 A2 CAD WINa              (c3 single, 25 MB)
#   c7-100   A1 A2 CAD WINa              (c2 || c2 dual, 100 MB)
#   c8-100   A1 A2 CAD WINa              (c2 || c3 dual, 100 MB)
# Blocks run rep 1 seed 42, rep 1 seed 7, rep 2 seed 42, ... With S3_NO_CAD=1
# (a pre-registered cut) CAD is left out of the plan.
#
# PER INVOCATION the ledger gets the `===` header, the driver's summary / dnf
# / CPU / [TRUTH] lines, `RUNTIME ... <s>s rc=<rc>`, a COTENANT line, and ONE
# `S3ROW {json}` from stage3_parse.py (status LIVE / NO_DATA / VOID-RC /
# VOID-COTENANT / CONTAMINATED / WITNESS-FAIL).
#
# ABORT CAUSES as ledger tokens (priority order): ABORT-LOCK (exit 4),
# ABORT-CRLF (exit 3), ABORT-SHA (start AND before every invocation; exit 5),
# ABORT-SENTINEL-UNWRITABLE (exit 3), ABORT-SMOKE (decided by the envelope),
# ABORT-RC (row void, battery goes on), ABORT-BRINGUP (no summary after
# S3_BRINGUP_TRIES attempts: NO_DATA).
#
# BALANCED TRUNCATION: with S3_SOFT_DEADLINE (epoch s) and S3_BLOCK_EST_S set,
# a (rep, seed) block that would not finish before the deadline is not
# started: `TRUNCATED-AT-REP-BOUNDARY` and a normal end.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh stage3_battery.sh stage3_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

REPS="${1:?reps per seed}"
SEEDS="${S3_SEEDS:-42 7}"
OUTDIR="${S3_OUTDIR:?S3_OUTDIR}"
TAG="${S3_TAG:-s3}"
TRIES="${S3_BRINGUP_TRIES:-2}"
BIN="${RWM_BIN:?RWM_BIN}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=stage3_battery
LB_LOG="$OUT"

# ── LOCKS: held by the envelope; verified here ───────────────────────────
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
for l in "$VM_LOCK" "$RP_LOCK"; do
  if [ -z "${S3_LOCK_OWNER:-}" ] || ! grep -qF -- "$S3_LOCK_OWNER" "$l" 2>/dev/null; then
    _lb_say "ABORT-LOCK $l is not held by the envelope (${S3_LOCK_OWNER:-unset}): $(cat "$l" 2>/dev/null)"
    exit 4
  fi
done
_lb_say "LOCKS-HELD-BY-ENVELOPE $S3_LOCK_OWNER"

# ── BINARY ───────────────────────────────────────────────────────────────
preflight_binary "$BIN" "PIPE] pipeline=" "estimator heavy-math cadence ACTIVE"
SHA_NOW="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ -z "${S3_SHA:-}" ] || [ "$SHA_NOW" != "$S3_SHA" ]; then
  _lb_say "ABORT-SHA start: binary $SHA_NOW != S3_SHA '${S3_SHA:-unset}'"
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
cell_arms() { # cell -> the arms run there
  local cad="CAD"
  [ "${S3_NO_CAD:-0}" = "1" ] && cad=""
  case "$1" in
    c1s-400) echo "A1 A2 WINa" ;;
    c1d-400) echo "A1 A2 $cad" ;;
    *)       echo "A1 A2 $cad WINa" ;;
  esac
}
arm_hint() { case "$1" in WINa) echo auto ;; *) echo bulk ;; esac; }
CELLS_ALL="c1s-400 c1d-400 c2-100 c3-25 c7-100 c8-100"
CELLS="${S3_CELLS:-$CELLS_ALL}"

run_one() { # cell arm seed rep
  local cell="$1" arm="$2" seed="$3" rep="$4" ca cb mode bytes pipe hint t0 rc wall attempt sha
  local cot_b=0 cot_a=0 cot=0
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  pipe=window; hint="$(arm_hint "$arm")"
  local name="$cell-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$S3_SHA" ]; then
      _lb_say "ABORT-SHA $name s$seed rep=$rep: binary $sha != $S3_SHA -- battery stopped"
      exit 5
    fi
    echo "=== rep=$rep seed=$seed cell=$cell arm=$arm attempt=$attempt pipeline=$pipe hint=$hint spec=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/s3-drv.out
    cot_b=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    t0=$(date +%s)
    # rule 15d: withhold the cadence pair from every arm, then set it on CAD only.
    if [ "$arm" = "CAD" ]; then
      env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0 \
          SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
          RWM_C_PIPELINE="$pipe" RWM_BIN="$BIN" \
        bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/s3-drv.out 2>&1
    else
      env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR \
          SEED="$seed" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
          RWM_C_PIPELINE="$pipe" RWM_BIN="$BIN" \
        bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/s3-drv.out 2>&1
    fi
    rc=$?
    wall=$(( $(date +%s) - t0 ))
    cot_a=$(( $(pgrep -xc cargo) + $(pgrep -xc rustc) ))
    echo "COTENANT $name s$seed rep=$rep before=$cot_b after=$cot_a load=$(cut -d" " -f1-3 /proc/loadavg)" >> "$OUT"
    grep -aE "summary|\"dnf\"|CPU:|GUARD|\[TRUTH\]|--- RWM-C perf" /tmp/s3-drv.out >> "$OUT" || true
    echo "RUNTIME $name s$seed rep=$rep attempt=$attempt ${wall}s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name s$seed rep=$rep rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${seed}-r${rep}-a${attempt}"
    cp /tmp/s3-drv.out "$base-drv.out" 2>/dev/null || true
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
  python3 ./stage3_parse.py row "$cell" "$arm" "$seed" "$rep" "$rc" "$wall" \
      /tmp/s3-drv.out /tmp/rwm-c.log /tmp/rwm-s.log "$cot" >> "$OUT" 2>&1 \
    || echo "S3ROW-PARSE-FAIL $name s$seed rep=$rep" >> "$OUT"
}

echo "=== STAGE3 BATTERY tag=$TAG reps_per_seed=$REPS seeds='$SEEDS' cells='$CELLS' no_cad=${S3_NO_CAD:-0} $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary $BIN sha256 $SHA_NOW" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 runs=1; CAD: RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0; soft_deadline=${S3_SOFT_DEADLINE:-none} block_est=${S3_BLOCK_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

BLOCK=0
STOP=0
for REP in $(seq 1 "$REPS"); do
  for SEED in $SEEDS; do
    if [ -n "${S3_SOFT_DEADLINE:-}" ] && [ -n "${S3_BLOCK_EST_S:-}" ] \
        && [ $(( $(date +%s) + S3_BLOCK_EST_S )) -gt "$S3_SOFT_DEADLINE" ]; then
      echo "TRUNCATED-AT-REP-BOUNDARY before rep=$REP seed=$SEED (now+${S3_BLOCK_EST_S}s > deadline $S3_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
      STOP=1
      break
    fi
    if [ -n "${S3_SMOKE_PLAN:-}" ]; then
      # smoke: an explicit "cell:arm cell:arm ..." list, nothing else
      for ca_ in $S3_SMOKE_PLAN; do
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
if [ -z "${S3_SMOKE_PLAN:-}" ]; then
  for CELL in $CELLS; do
    for A in $(cell_arms "$CELL"); do
      N=$(grep -c "^S3ROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
    done
  done
fi
echo "STAGE3-BATTERY-DONE tag=$TAG blocks=$BLOCK $(date -u +%FT%TZ)" >> "$OUT"
exit 0
