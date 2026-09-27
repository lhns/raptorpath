#!/bin/bash
# THE BLOCK DEFAULT RE-TEST BATTERY (docs/status.md §4 "Block default re-test
# — pre-registration" and its amendments; discharges ADR-0069's re-test
# clause).
#
#   sudo env BR_SHA=<sha256> bash blockretest_battery.sh <seed> [reps]
#
# Launch through `blockretest_run_all.sh` (the 5 h envelope) for the scored
# battery; call directly (with BR_CELLS/BR_HINTS) for the smoke.
#
# ARMS, interleaved within each rep on ONE binary, fresh topology per
# invocation (perf_rwm_c.sh builds and tears down its namespaces):
#   BLK  RWM_C_PIPELINE=block  -- no --window-reliable, backend unset
#                                  (block pipeline, RaptorQ)
#   WIN  RWM_C_PIPELINE=window -- --window-reliable, backend unset (RLC),
#                                  RWM_GEN=0 (generation off)
# The arm order alternates by rep parity (BLK first on odd reps).
#
# CELLS c1 c2 c3 c7 c8 (c8 = the 25 MB dual c2 || c3), HINTS bulk auto,
# RWM_PERF_TIMEOUT_S=150 (a run past it is a DNF), RWM_DIAG=1 on both arms.
#
# PER INVOCATION the ledger gets: the `===` header, the driver's summary /
# dnf / CPU lines, `RUNTIME ... rc=<driver rc>`, and ONE `BRROW {json}` from
# blockretest_parse.py carrying the witnesses (the `pipeline=` header, the
# `[PIPE]` echo on BOTH endpoints, `[GATES]` on both, the window/RLC line) and
# the status LIVE / NO_DATA / VOID-RC / CONTAMINATED / WITNESS-FAIL.
#
# ABORT CAUSES (pre-registered, priority order) as ledger tokens:
#   ABORT-LOCK                 either lock held by someone else (exit 4)
#   ABORT-CRLF                 lib.sh / this tree carries CR bytes (exit 3)
#   ABORT-SHA                  binary sha256 != BR_SHA, checked at start AND
#                              before every invocation (the battery stops)
#   ABORT-SENTINEL-UNWRITABLE  ledger unwritable (probed at launch)
#   ABORT-SMOKE                decided by `blockretest_parse.py smoke`
#   ABORT-RC                   driver rc != 0: that row is VOID-RC, go on
#   ABORT-BRINGUP              no summary after BR_BRINGUP_TRIES attempts:
#                              NO_DATA, not an abort of the battery
#
# BALANCED TRUNCATION. With BR_SOFT_DEADLINE (epoch s) and BR_REP_EST_S set, a
# rep that would not finish before the deadline is not started:
# `TRUNCATED-AT-REP-BOUNDARY` is written and the battery ends normally (the
# envelope's hard backstop remains the last resort).
#
# LOCKS. Run alone, the battery takes both operator locks itself. Under the
# envelope (which holds them for the whole session: crown spot + both seeds)
# it is passed BR_LOCK_OWNER=<token> and VERIFIES that both lock files carry
# that token instead -- anything else is ABORT-LOCK.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh blockretest_battery.sh blockretest_parse.py \
    perf_rwm_c.sh topo_dual.sh l1common.py

SEED_ARG="${1:?seed}"; REPS="${2:-3}"
CELLS="${BR_CELLS:-c1 c2 c3 c7 c8}"
HINTS="${BR_HINTS:-bulk auto}"
OUTDIR="${BR_OUTDIR:-/home/vibe/blockretest/run}"
TAG="${BR_TAG:-br}"
TRIES="${BR_BRINGUP_TRIES:-2}"
BIN="${RWM_BIN:-$(cd "$HERE/../../.." && pwd)/target/release/raptorpath}"
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=blockretest_battery
LB_LOG="$OUT"

# ── LOCKS ────────────────────────────────────────────────────────────────
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
if [ -n "${BR_LOCK_OWNER:-}" ]; then
  for l in "$VM_LOCK" "$RP_LOCK"; do
    if ! grep -qF -- "$BR_LOCK_OWNER" "$l" 2>/dev/null; then
      _lb_say "ABORT-LOCK $l is not held by the envelope ($BR_LOCK_OWNER): $(cat "$l" 2>/dev/null)"
      exit 4
    fi
  done
  _lb_say "LOCKS-HELD-BY-ENVELOPE $BR_LOCK_OWNER"
else
  install_lock_traps
  take_lock "$VM_LOCK"
  take_lock "$RP_LOCK"
fi

# ── BINARY ───────────────────────────────────────────────────────────────
# preflight: the binary runs and carries the [PIPE] echo (else it is the old
# engine and the arms cannot be witnessed two-sided).
preflight_binary "$BIN" "[PIPE] pipeline="
SHA_NOW="$(sha256sum "$BIN" | cut -d' ' -f1)"
if [ -z "${BR_SHA:-}" ] || [ "$SHA_NOW" != "$BR_SHA" ]; then
  _lb_say "ABORT-SHA start: binary $SHA_NOW != BR_SHA '${BR_SHA:-unset}'"
  exit 5
fi

if pgrep -x raptorpath >/dev/null 2>&1; then
  _lb_say "BUSY: raptorpath already running -- aborting"
  exit 3
fi

cell_spec() { # cell -> "scenA scenB mode bytes"
  case "$1" in
    c1) echo "c1 c1 single 400000000" ;;
    c2) echo "c2 c2 single 100000000" ;;
    c3) echo "c3 c3 single 25000000" ;;
    c7) echo "c2 c2 dual 200000000" ;;
    c8) echo "c2 c3 dual 25000000" ;;
    *)  echo "" ;;
  esac
}
arm_pipe() { case "$1" in BLK) echo block ;; WIN) echo window ;; esac; }

run_one() { # cell hint arm
  local cell="$1" hint="$2" arm="$3" ca cb mode bytes pipe t0 rc attempt sha
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }
  pipe="$(arm_pipe "$arm")"
  local name="$cell-$hint-$arm"
  for attempt in $(seq 1 "$TRIES"); do
    sha="$(sha256sum "$BIN" | cut -d' ' -f1)"
    if [ "$sha" != "$BR_SHA" ]; then
      _lb_say "ABORT-SHA $name rep=$REP: binary $sha != $BR_SHA -- battery stopped"
      exit 5
    fi
    echo "=== rep=$REP arm=$arm cell=$cell hint=$hint seed=$SEED_ARG attempt=$attempt pipeline=$pipe spec=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/br-drv.out
    t0=$(date +%s)
    env SEED="$SEED_ARG" RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 \
        RWM_C_PIPELINE="$pipe" RWM_BIN="$BIN" \
      bash perf_rwm_c.sh "$ca" "$cb" "$hint" "$bytes" 1 "$mode" > /tmp/br-drv.out 2>&1
    rc=$?
    grep -aE "summary|\"dnf\"|CPU:|GUARD|--- RWM-C perf" /tmp/br-drv.out >> "$OUT" || true
    echo "RUNTIME $name rep=$REP attempt=$attempt $(( $(date +%s) - t0 ))s rc=$rc" >> "$OUT"
    [ "$rc" = "0" ] || echo "ABORT-RC $name rep=$REP rc=$rc (row void, battery goes on)" >> "$OUT"
    local base="$DDIR/${name}-s${SEED_ARG}-r${REP}-a${attempt}"
    cp /tmp/br-drv.out "$base-drv.out" 2>/dev/null || true
    cp /tmp/rwm-c.log "$base-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$base-s.log" 2>/dev/null || true
    # Bring-up retry: only an invocation with rc 0 and NO client summary
    # (the warm-up never acked) is retried, and every retry is counted.
    if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null \
        && [ "$attempt" -lt "$TRIES" ]; then
      echo "RUN-RETRY $name rep=$REP attempt=$attempt (no summary)" >> "$OUT"
      continue
    fi
    break
  done
  if [ "$rc" = "0" ] && ! grep -aq '"summary"' /tmp/rwm-c.log 2>/dev/null; then
    echo "ABORT-BRINGUP $name rep=$REP after $TRIES attempts: NO_DATA" >> "$OUT"
  fi
  python3 ./blockretest_parse.py row "$cell" "$arm" "$hint" "$SEED_ARG" "$REP" "$rc" \
      /tmp/br-drv.out /tmp/rwm-c.log /tmp/rwm-s.log >> "$OUT" 2>&1 \
    || echo "BRROW-PARSE-FAIL $name rep=$REP" >> "$OUT"
}

echo "=== BLOCKRETEST BATTERY seed=$SEED_ARG reps=$REPS cells='$CELLS' hints='$HINTS' arms='BLK WIN' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary $BIN sha256 $SHA_NOW" >> "$OUT"
echo "=== source $(cat "$HERE/../../../COMMIT" 2>/dev/null)" >> "$OUT"
echo "=== env RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=block|window runs=1 soft_deadline=${BR_SOFT_DEADLINE:-none} rep_est=${BR_REP_EST_S:-none}" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

for REP in $(seq 1 "$REPS"); do
  if [ -n "${BR_SOFT_DEADLINE:-}" ] && [ -n "${BR_REP_EST_S:-}" ] \
      && [ $(( $(date +%s) + BR_REP_EST_S )) -gt "$BR_SOFT_DEADLINE" ]; then
    echo "TRUNCATED-AT-REP-BOUNDARY seed=$SEED_ARG before rep=$REP (now+${BR_REP_EST_S}s > deadline $BR_SOFT_DEADLINE) $(date -u +%FT%TZ)" >> "$OUT"
    break
  fi
  if [ $(( REP % 2 )) -eq 1 ]; then ORDER="BLK WIN"; else ORDER="WIN BLK"; fi
  for CELL in $CELLS; do
    for HINT in $HINTS; do
      for ARM in $ORDER; do
        run_one "$CELL" "$HINT" "$ARM"
      done
    done
  done
  echo "REP-COMPLETE seed=$SEED_ARG rep=$REP $(date -u +%FT%TZ)" >> "$OUT"
done

# Per-arm result count (discipline 7): an arm that vanished fails loudly.
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $CELLS; do
  for HINT in $HINTS; do
    for A in BLK WIN; do
      N=$(grep -c "^BRROW .*\"arm\": \"$A\".*\"cell\": \"$CELL\".*\"hint\": \"$HINT\"" "$OUT" || true)
      echo "ARMCOUNT $CELL-$HINT-$A n=${N:-0}" >> "$OUT"
      [ "${N:-0}" -eq 0 ] && echo "ARM-VANISHED $CELL-$HINT-$A" >> "$OUT"
    done
  done
done
echo "BLOCKRETEST-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
exit 0
