#!/bin/bash
# THE SPAN RUN (VM) — the capture half of goal-gate "THE SPAN RUN —
# PRE-REGISTRATION". MEASUREMENT TRUTH item 4's VM half, plus item 5's first
# field sigma readout.
#
# THIS SCRIPT CAPTURES; IT SCORES NOTHING. C9-L1 and C9-L3's bands are
# pre-registered in the c9 CONTRACT and are cited, not re-derived, by the
# pre-registration this driver serves. Nothing here computes a ratio or a
# verdict — `span_parse.py` reads the ledger afterwards.
#
# THE ONE CELL.
#   c9h  HETEROGENEOUS quad — 2 x c2 + 2 x c3, i.e. `perf_rwm_c.sh c2 c3 ...
#        quad` which expands to `c2 c2 c3 c3` on the wire. RTprop ~10/10/37/37
#        ms, so there are TWO min-RTprop legs. That count of two is the whole
#        reason this geometry exists: the shipped span form
#        `rate_fast*(RTT_max - RTT_min)` and the crosscheck's un-adopted
#        `sum bw_i (RTT_max - RTT_i)` diverge by exactly the COUNT of
#        min-RTprop legs, which is invisible at every dual.
#
#   BYTES = 50 MB, the PRE-REGISTERED sizing, deliberately NOT the 3x that the
#   c9 re-smoke tried and reverted. c9h is sender-starved and cannot be rescued
#   by length (goal-gate c9 re-smoke FINDING 1). This clause needs windows to
#   EXIST, not correlation power: the span reads off `[CCAP]`.
#
# THE TWO ARMS, and the second one is a control rather than a comparison.
#   on   RWM_COMPOSED_CAP=1   the gate the c9 battery never set. `[CCAP]` is
#                             emitted only under `pol.composed_cap`, so this is
#                             the arm that HAS the instrument.
#   off  RWM_COMPOSED_CAP=0   `[CCAP]` must be ABSENT and the `[GATES]` echo
#                             must read `RWM_COMPOSED_CAP=0`. Two-sided, for
#                             the reason the c9 battery learned the hard way:
#                             an arm that sets nothing cannot be told from an
#                             arm whose knob failed to forward, and "the gauge
#                             was never armed" is not "the gauge read nothing".
#
#   RWM_COMPOSED_CAP SHIPS OFF AND STAYS OFF. It is set on one arm of one run
#   as a measurement instrument. This driver flips no default.
#
# WHY THIS IS NOT `c9_battery.sh` WITH ONE MORE ENV VAR. That driver's capture
# pipes `perf_rwm_c.sh` through `grep -E "summary|dnf|CPU:|GUARD|QDISC|QCAP"`,
# and `[CCAP]` / `[DIAG]` are on NEITHER that filter NOR the ledger — they land
# in /tmp/rwm-c.log, which the NEXT rep overwrites. A run scored on `[CCAP]`
# must PRESERVE the client log per rep, and that is the substantive difference
# below. Everything else — the witness columns, the two-sided liveness block,
# the ping-retry column — keeps `c9_battery.sh`'s definitions to the line.
#
#   usage: sudo bash span_battery.sh <on|off> <rep> [seed]
set -u
[ "$(id -u)" -eq 0 ] || { echo "must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
ARM="$1"; REP="$2"; SEED_ARG="${3:-42}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
DDIR=/home/vibe/span
OUT=$DDIR/span-c9h-s${SEED_ARG}.log
mkdir -p "$DDIR"

CA=c2; CB=c3; BYTES=50000000; RUNS=3

case "$ARM" in
  on)  ARM_ENV="RWM_COMPOSED_CAP=1" ;;
  off) ARM_ENV="RWM_COMPOSED_CAP=0" ;;
  *) echo "unknown arm $ARM (want on|off)" >&2; exit 2 ;;
esac

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2; exit 3
fi

if [ ! -s "$OUT" ]; then
  {
    echo "# span run $(date -u +%FT%TZ) seed=$SEED_ARG cell=c9h"
    echo "# binary: $(sha256sum $BIN)"
    echo "# source: $(cat /home/vibe/raptorpath/COMMIT)"
    echo "# kernel: $(uname -r)"
    lscpu | grep "Model name"
    echo "# arms:   RWM_COMPOSED_CAP=1 (on) vs =0 (off), two-sided"
    echo "# base:   RWM_GEN=0, RWM_DIAG=1 on BOTH arms (item 5 sigma readout)"
    echo "# NOTE:   RWM_DIAG=1 is NOT the c9 battery's base. This ledger is"
    echo "#         NOT poolable with c9's on any timing quantity."
  } >> "$OUT"
fi

t0=$(date +%s)
echo "=== rep=$REP cell=c9h arm=$ARM seed=$SEED_ARG $CA/$CB/quad bytes=$BYTES runs=$RUNS $(date -u +%FT%TZ)" >> "$OUT"
rm -f /tmp/rwm-c.log /tmp/rwm-s.log
# shellcheck disable=SC2086
env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 $ARM_ENV \
  bash perf_rwm_c.sh "$CA" "$CB" bulk "$BYTES" "$RUNS" quad 2>&1 \
  | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT" || true
echo "RUNTIME c9h/$ARM rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

# ── PRESERVE THE CLIENT LOG. This is what the run is scored off. ──────────
cp -f /tmp/rwm-c.log "$DDIR/cli-$ARM-r$REP.log" 2>/dev/null || true
cp -f /tmp/rwm-s.log "$DDIR/srv-$ARM-r$REP.log" 2>/dev/null || true

# ── ABORT WITNESS — `abort_witness.py`'s definition, not a re-implemented grep
aw_col() {
  python3 -c "
import sys; sys.path.insert(0, '.')
from abort_witness import read_witness
w = read_witness('/tmp/rwm-abort.txt')
print('' if w is None else (w.get('$1') if w.get('$1') is not None else ''))" 2>/dev/null
}
gates_c=$(grep -c '\[GATES\]' /tmp/rwm-c.log 2>/dev/null || true)
gates_s=$(grep -c '\[GATES\]' /tmp/rwm-s.log 2>/dev/null || true)
cause=$(python3 -c "
import sys; sys.path.insert(0, '.')
from abort_witness import cause_or
print(cause_or('/tmp/rwm-abort.txt'))" 2>/dev/null)
drain=$(aw_col drain_pids_t0)
if [ -f /tmp/rwm-abort.txt ]; then amiss=FALSE; else amiss=TRUE; fi
echo "WITNESS c9h/$ARM rep=$REP abort_cause=${cause:-no_record} abort_missing=$amiss drain_pids_t0=${drain:-NA} gates_cli=${gates_c:-0} gates_srv=${gates_s:-0}" >> "$OUT"
grep -E '^ping_.*_attempts=' /tmp/rwm-abort.txt 2>/dev/null \
  | sed "s/^/PING-RETRY c9h\/$ARM rep=$REP /" >> "$OUT" || true
if [ "$(( ${gates_c:-0} + ${gates_s:-0} ))" -eq 0 ]; then
  echo "ABORT c9h/$ARM rep=$REP (no [GATES] on either endpoint) abort_cause=${cause:-no_record}" >> "$OUT"
  [ "${cause:-no_record}" = "no_record" ] \
    && echo "INSTRUMENT-FAIL-WITNESS c9h/$ARM rep=$REP (an abort with no witness record)" >> "$OUT"
fi

# ── LIVENESS, TWO-SIDED, on the one knob this run rests on (discipline 15c) ─
# The `[GATES]` echo is read on BOTH endpoints and the gauge is separately
# proven to have EMITTED. `RWM_COMPOSED_CAP=1` with zero `[CCAP]` lines is the
# exact failure the c9 battery hit from the other side.
gate() { grep "\[GATES\]" "$1" 2>/dev/null | tail -1 | grep -o "$2=[0-9]*"; }
for role in c s; do
  L=/tmp/rwm-$role.log
  g_cc=$(gate "$L" RWM_COMPOSED_CAP)
  g_dg=$(gate "$L" RWM_DIAG)
  echo "LIVENESS c9h/$ARM rep=$REP role=$role [$g_cc] [$g_dg]" >> "$OUT"
  case "$ARM" in
    on)  [ "$g_cc" != "RWM_COMPOSED_CAP=1" ] && echo "ARM-LIVENESS-FAIL c9h/$ARM rep=$REP role=$role got='$g_cc'" >> "$OUT" ;;
    off) [ "$g_cc" != "RWM_COMPOSED_CAP=0" ] && echo "ARM-LIVENESS-FAIL c9h/$ARM rep=$REP role=$role got='$g_cc'" >> "$OUT" ;;
  esac
done
nccap=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -c '\[CCAP\]' || true)
ndiag=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -c '\[DIAG\]' || true)
echo "LIVENESS-EMIT c9h/$ARM rep=$REP ccap_lines_cli=${nccap:-0} diag_lines_cli=${ndiag:-0}" >> "$OUT"
# The `[CCAP]` line itself, verbatim, onto the ledger — so the ledger alone is
# scoreable and the preserved client logs are corroboration rather than the
# only copy.
sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep '\[CCAP\]' \
  | sed "s/^/CCAP c9h\/$ARM rep=$REP /" >> "$OUT" || true
echo "DONE c9h/$ARM rep=$REP $(date -u +%FT%TZ)" >> "$OUT"
