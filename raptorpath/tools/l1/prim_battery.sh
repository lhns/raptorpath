#!/bin/bash
# THE PASSIVE PRIMITIVES — THE FULL-CELL σ/ν/p/d PASS (goal #100 item 1).
#
#   sudo bash prim_battery.sh [outdir] [reps]
#
# ONE binary, ONE seed, FIVE cells, n reps each, SHIPPED DEFAULTS plus three
# READ-ONLY DIAGNOSTIC GATES and nothing else:
#
#   RWM_DIAG=1    the sender's per-path [DIAG] line — carries `sig_us=<µs>/n<n>`
#                 (σ) and `dgq<i>[hand=…]` (ν's denominator). Read by the c8
#                 σ pass under exactly this gate.
#   RWM_FDIAG=1   the receiver's [FDIAG] line — carries the per-hole delivery
#                 stall (d). Default OFF; this is its FIRST use as a scored
#                 instrument.
#   RWM_ACKDIAG=1 carried only so the rows pool with the committed ledgers,
#                 which all set it. Nothing here reads it.
#
# NOT SET, DELIBERATELY: `RWM_GEN=0`. Every committed ccand/ladder/ccap row
# ran the plain-window control, but this pass measures the SHIPPED machine —
# the ruling's whole point is that a value must be derived from what it
# actually depends on, and the shipped machine runs the generation pipeline.
# The consequence is stated in the pre-registration: these rows do NOT pool
# with the ccand ledger's, and ν here is not comparable to `nu_measure.py`'s
# 0.0438 without that caveat attached.
#
# THREE GATES, ALL READ-ONLY. None changes a law, a rate, a clock or a
# window. `RWM_DIAG` and `RWM_FDIAG` gate `eprintln!` sites; `RWM_ACKDIAG`
# gates a counter block with no feedback path. Two-sided verification is the
# `[GATES]` echo, asserted per invocation below.
set -uo pipefail
cd "$(dirname "$0")"

OUT="${1:-/home/vibe/prim}"
REPS="${2:-3}"
SEED="${RWM_PRIM_SEED:-42}"
CELLS="${RWM_PRIM_CELLS:-c1 c7 c8 c8L sc2}"

mkdir -p "$OUT"

# The five committed cells, transcribed from `ccand_battery.sh:202-215`
# VERBATIM. Not re-derived, not adjusted: a cell that differs from the
# ledger's cell is a different cell and its rows do not pool.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    c8)  echo "c2 c3 dual    25000000" ;;
    c8L) echo "c2 c3 dual   200000000" ;;
    *) echo "" ;;
  esac
}

echo "== PRIM PASS  seed=$SEED reps=$REPS cells=$CELLS  $(date -u +%FT%TZ)"
echo "== BIN ${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"
sha256sum "${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}" 2>/dev/null || true

for cell in $CELLS; do
  spec="$(cell_spec "$cell")"
  if [ -z "$spec" ]; then echo "!! unknown cell $cell" >&2; continue; fi
  set -- $spec
  ca="$1"; cb="$2"; mode="$3"; bytes="$4"
  for r in $(seq 1 "$REPS"); do
    tag="$cell-s$SEED-r$r"
    echo "== RUN $tag  ($ca/$cb $mode $bytes B)  $(date -u +%FT%TZ)"
    env SEED="$SEED" RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" \
      > "$OUT/$tag-run.log" 2>&1
    rc=$?
    # ABORT-CAUSE FIRST: the witness's own record, copied before anything is
    # read off the transfer. A rep with no [GATES] on both endpoints is an
    # abort and is reported as one, never retried away.
    gc=$(grep -c '\[GATES\]' /tmp/rwm-c.log 2>/dev/null || echo 0)
    gs=$(grep -c '\[GATES\]' /tmp/rwm-s.log 2>/dev/null || echo 0)
    fd=$(grep -c '\[FDIAG\]' /tmp/rwm-s.log 2>/dev/null || echo 0)
    dg=$(grep -c 'sig_us=' /tmp/rwm-c.log 2>/dev/null || echo 0)
    cp /tmp/rwm-c.log "$OUT/$tag-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$OUT/$tag-s.log" 2>/dev/null || true
    cp /tmp/rwm-q.txt "$OUT/$tag-q.txt" 2>/dev/null || true
    echo "PRIMWITNESS {\"cell\":\"$cell\",\"seed\":$SEED,\"rep\":$r,\"rc\":$rc,\"gates_cli\":$gc,\"gates_srv\":$gs,\"fdiag_lines\":$fd,\"sig_lines\":$dg}" \
      | tee -a "$OUT/prim-witness-s$SEED.jsonl"
  done
done

echo "== DONE $(date -u +%FT%TZ)"
touch "$OUT/DONE-PRIM-s$SEED"
