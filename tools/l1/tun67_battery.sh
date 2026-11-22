#!/bin/bash
# Plan 67, stage T (docs/derivations/vm-batch-1-registration.md 3.3): the tunnel itself, B0 against S1.
#
#   sudo env LD_BIN_B0=<bin> LD_SHA_B0=<sha256> LD_BIN_S1=<bin> LD_SHA_S1=<sha256> LD_OUTDIR=<dir> LD_TAG=<tag> [LD_SMOKE=1] bash tun67_battery.sh
#
# Transfer rows: cell c2, hints auto and realtime, seats B0 and S1, ONE tunnel bring-up per (seat, hint) with REPS = 5 transfers of 100 MB at seed 42 (tun_bulk.sh: kernel TCP inside the tunnel, `raptorpath run` on both
# ends, no --window-reliable, a TOML file with no object-mode key loaded by both ends, info-level logs): 4 invocations, 20 transfers; the smoke is one invocation per seat at auto with REPS = 1.
# Refusal rows: for each of the four object-mode keys a TOML file that sets only that key to true; per seat `check` and `run` (16 commands). S1 must refuse both with the exact text; on B0 `run` must start and its
# `resolved configuration` line must read the key true (the defect on the shipped binary); B0's `check` commands are run and printed, never scored (12 commands are scored).
# The ledger (the one the scorer s67_score.py `tun` / `smokeT` reads):
#   T67SEAT seat=<B0|S1> hint=<h>              before each invocation's tun_bulk.sh output (TUNARM / TUNSHA / TUNMTU / TUNPIPE / TUNCAD / TUNCFG / TUNREP / TUNARM-DONE verbatim)
#   T67REF seat=.. cmd=<check|run> key=<key> rc=<n> cfg=<true|false|-> err=<first non-empty stderr line>
#   T67DONE, then LD-BATTERY-DONE
# A B0 `run` whose stderr carries the guard's refusal text means B0 is not main's binary: ABORT-SEAT (exit 5), nothing more runs. A bring-up failure is recorded by tun_bulk.sh (its TUN-BRINGUP-FAIL line goes to stdout, so it is
# in this ledger) and read by s67_score.py `tun`: ABORT-BRINGUP-T seat=<S> hint=<h> (registration 5.4 "bring-up exhausted (Stage T)"), no T verdict from the transfer clauses; loopdiag_p67.sh then does not count the stage ok.
# It is not an abort HERE: the other invocations and the refusal rows still run, so the stage reports everything it saw.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh tun67_battery.sh tun_bulk.sh plan67.py s67_score.py
OUTDIR="${LD_OUTDIR:?LD_OUTDIR}"
TAG="${LD_TAG:-ld}"
TMPD="${LD_TMPD:-/tmp}"
DEVSHM="${LD_DEVSHM:-/dev/shm}"
OUT="$OUTDIR/${TAG}.log"
DDIR="$OUTDIR/diag-${TAG}"
mkdir -p "$OUTDIR" "$DDIR"
: > "$OUT" 2>/dev/null || { echo "ABORT-SENTINEL-UNWRITABLE $OUT"; exit 3; }
LB_TAG=tun67_battery
LB_LOG="$OUT"
KEYS=(window_out_of_order window_coded_only window_generation_coding window_systematic_repair)
GUARD_MSG="is a perf/native-object mode"
SMOKE="${LD_SMOKE:-0}"
REPS=5; HINTS=(auto realtime); [ "$SMOKE" = "1" ] && { REPS=1; HINTS=(auto); }

# the seats: file, sha against the pin, identity strings (plan67.py strings: B0 has no guard, S1 has it)
for _seat in B0 S1; do
  _bv="LD_BIN_$_seat"; _sv="LD_SHA_$_seat"
  [ -n "${!_bv:-}" ] && [ -n "${!_sv:-}" ] || { _lb_say "ABORT-SHA plan 67 stage T seat $_seat: $_bv / $_sv unset"; exit 5; }
  preflight_binary "${!_bv}" "RWM_STORE"
  [ "$(sha256sum "${!_bv}" | cut -d' ' -f1)" = "${!_sv}" ] || { _lb_say "ABORT-SHA plan 67 stage T seat $_seat: ${!_bv} does not hash to its pin ${!_sv}"; exit 5; }
  while read -r _kind _str; do
    case "$_kind" in
      need) grep -aqF -- "$_str" "${!_bv}" 2>/dev/null || { _lb_say "ABORT-SEAT plan 67 stage T seat $_seat: the binary has no '$_str' string"; exit 5; } ;;
      forbid) ! grep -aqF -- "$_str" "${!_bv}" 2>/dev/null || { _lb_say "ABORT-SEAT plan 67 stage T seat $_seat: the binary carries '$_str'"; exit 5; } ;;
    esac
  done < <(python3 ./plan67.py strings "$_seat")
done
[ "${LD_BIN_B0}" != "${LD_BIN_S1}" ] || { _lb_say "ABORT-SHA plan 67 stage T: B0 and S1 are the same path"; exit 5; }
if pgrep -x raptorpath >/dev/null 2>&1; then _lb_say "BUSY: raptorpath already running -- aborting"; exit 3; fi

# The transfer rows' file: no object-mode key (comment only). Both ends load it, so the `resolved configuration` line is the config's own reading.
DEFAULT_TOML="$OUTDIR/tun67-default.toml"
printf '# plan 67 stage T: the transfer rows load this file; it sets no object-mode key\n' > "$DEFAULT_TOML"

echo "=== TUN67 BATTERY tag=$TAG smoke=$SMOKE reps=$REPS $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary B0 ${LD_BIN_B0} sha256 ${LD_SHA_B0}" >> "$OUT"
echo "=== binary S1 ${LD_BIN_S1} sha256 ${LD_SHA_S1}" >> "$OUT"
echo "LD-PLAN67 plan=t drop_s2=${LD_DROP_S2:-0}" >> "$OUT"      # the drop state in the ledger, as every plan-67 ledger (stage T runs B0 and S1 only: nothing reads it here yet)

# ── transfer rows: seats alternate inside each hint (B0 first, then S1), the hints in order ──
for hint in "${HINTS[@]}"; do
  for seat in B0 S1; do
    _bv="LD_BIN_$seat"
    echo "T67SEAT seat=$seat hint=$hint" >> "$OUT"
    rm -f "$TMPD"/tb-c.log "$TMPD"/tb-s.log "$TMPD"/tb-srv.log
    env RWM_BIN="${!_bv}" SEED=42 TB_CONFIG="$DEFAULT_TOML" TB_RUST_LOG=info TB_TMPD="$TMPD" bash ./tun_bulk.sh c2 "$hint" 100000000 "$REPS" "$seat-$hint" >> "$OUT" 2>&1
    echo "T67END seat=$seat hint=$hint rc=$? $(date -u +%T)" >> "$OUT"
    cp "$TMPD/tb-c.log" "$DDIR/$seat-$hint-tb-c.log" 2>/dev/null || true
    cp "$TMPD/tb-s.log" "$DDIR/$seat-$hint-tb-s.log" 2>/dev/null || true
  done
done

# ── refusal rows: 16 commands, S1 first (the scored ones), then B0 ──
strip_ansi() { sed 's/\x1b\[[0-9;]*m//g'; }
for seat in S1 B0; do
  _bv="LD_BIN_$seat"; bin="${!_bv}"
  for key in "${KEYS[@]}"; do
    f="$OUTDIR/tun67-$key.toml"
    printf '%s = true\n' "$key" > "$f"
    for cmd in check run; do
      lg="$DDIR/ref-$seat-$cmd-$key"
      if [ "$cmd" = "check" ]; then
        RUST_LOG=info timeout 20 "$bin" --config "$f" check > "$lg.out" 2> "$lg.err"
      else
        # a throwaway server on loopback with its own TUN: S1 refuses at start; B0 starts, is killed by the timeout, and has printed its resolved configuration by then
        RUST_LOG=info timeout 8 "$bin" --config "$f" run --server --bind 127.0.0.1:7999 --tun-name rptref0 --tun-addr 10.98.0.1/24 --protocol-hint auto > "$lg.out" 2> "$lg.err"
      fi
      rc=$?
      cfg="$(cat "$lg.out" "$lg.err" 2>/dev/null | strip_ansi | grep -a 'resolved configuration' | tail -n 1 | grep -ao "$key: \(true\|false\)" | head -n 1 | sed 's/.*: //')"
      err="$(strip_ansi < "$lg.err" | tr '\t' ' ' | grep -av '^[[:space:]]*$' | head -n 1)"
      echo "T67REF seat=$seat cmd=$cmd key=$key rc=$rc cfg=${cfg:--} err=$err" >> "$OUT"
      if [ "$seat" = "B0" ] && [ "$cmd" = "run" ] && grep -aqF -- "$GUARD_MSG" "$lg.err" "$lg.out" 2>/dev/null; then
        echo "ABORT-SEAT plan 67 stage T: B0's run refused $key with the guard's text: B0 is not main's binary" >> "$OUT"
        _lb_say "ABORT-SEAT plan 67 stage T: B0 refuses $key -- not main's binary"; exit 5
      fi
    done
  done
done
echo "T67DONE $(date -u +%FT%TZ)" >> "$OUT"
echo "LD-BATTERY-DONE tag=$TAG blocks=$([ "$SMOKE" = "1" ] && echo 0 || echo 1) $(date -u +%FT%TZ)" >> "$OUT"
exit 0
