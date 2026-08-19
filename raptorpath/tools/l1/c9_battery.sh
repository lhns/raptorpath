#!/bin/bash
# THE c9 QUAD BATTERY (VM) — the capture half of goal-gate "Eppen's Condition
# at c8" §4's pre-registration (C9-1 … C9-4).
#
# THIS SCRIPT CAPTURES; IT SCORES NOTHING. The predictions are pre-registered
# in the ledger BEFORE this ran and are scored by `eppen_quad.py` against what
# lands here. Nothing in this file computes a rho, and that separation is the
# point: a driver that could see its own verdict could be tuned to it.
#
# THE TWO CELLS.
#   c9   SYMMETRIC quad   — 4 x the c2-class leg. C9-1 and C9-2's geometry,
#                           and the WIRE TWIN of `store_cap_sf_bench.rs`'s
#                           SIMULATED `c7x4` (`vec![C2, C2, C2, C2]`).
#   c9h  HETEROGENEOUS quad — 2 x c2 + 2 x c3. C9-3's geometry. It has NO
#                           bench twin; nothing in the tree simulates it.
#
# THE TWO ARMS, and why the second one is the whole reason this exists.
#   pooled   RWM_STORE_PATHS=1  the SHIPPED default, one shared pool.
#   percap   RWM_STORE_PERCAP=1 per-path accounts (refuted for goodput by
#                               ADR-0058 — that is NOT what is being re-run).
#
#   C9-4 measures rho_bar on BOTH arms at the SAME geometry. Eppen's rho is an
#   INPUT to his model: a property of the environment, which the inventory
#   policy cannot change. If that holds here, the two arms must agree
#   (|rho_pooled - rho_percap| <= 0.15). If the pooled arm's rho is much
#   HIGHER (>= +0.30), the correlation is the POOL'S OWN — an output of the
#   design under test, not a property of the cell — and Eppen's theorem is
#   being applied to a quantity his model treats as given. That outcome would
#   RETIRE CD-5's reading rather than confirm it, which is why the arm is run
#   even though its goodput verdict is already known.
#
# THE TWO PREREQUISITES, both inherited rather than re-argued (goal-gate
# "HARNESS ERA BOUNDARY"):
#   1. `RWM_ACKDIAG_WINDOW_US=250000`, set EXPLICITLY on every arm. The
#      shipped 2 s window yields four windows per rep, and six pairwise
#      correlations at a quad cannot be carried by four windows. This is a
#      BLOCKING dependency of C9-1..4, not a refinement. A 2 s ledger and a
#      250 ms ledger are never pooled, and the resolved value is echoed in
#      `[GATES]` so which one this is can be read off the capture.
#   2. PER-LEG netem seeds. `SEED=42` gives 42/1042/2042/3042 — four
#      INDEPENDENT loss realizations. The symmetric quad is exactly the shape
#      where the old shared-seed defect pinned rho_loss at +1 BY
#      CONSTRUCTION, so c9 would have been the worst possible cell to capture
#      under the previous harness era. It has no legacy era: every ledger it
#      produces is on the near side of the boundary.
#
#   usage: sudo bash c9_battery.sh <c9|c9h> <arm> <rep> [seed-spec]
#     e.g. sudo bash c9_battery.sh c9  pooled 1 42
#          sudo bash c9_battery.sh c9  percap 1 42
#          sudo bash c9_battery.sh c9h pooled 1 42
set -u
[ "$(id -u)" -eq 0 ] || { echo "must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
CELL="$1"; ARM="$2"; REP="$3"; SEED_ARG="${4:-42}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
DDIR=/home/vibe/c9
# The ledger name carries the SEED SPEC and the WINDOW, because both are part
# of the measurand and a file name is the last place a mismatch can be caught
# before two eras get pooled by a glob.
OUT=$DDIR/c9-w250-s${SEED_ARG//,/x}.log
mkdir -p "$DDIR"

# ── THE CELL TABLE ────────────────────────────────────────────────────────
# `perf_rwm_c.sh quad` takes the two LEG CLASSES and uses each for two legs,
# so `CA CB` below expands to `CA CA CB CB` on the wire.
#
# BYTES is sized for >= 30 windows per rep at the 250 ms cadence, which is the
# cadence's entire purpose. c9 carries ~2x c7's aggregate capacity (4 x
# 100 Mbit vs 2 x 100 Mbit) and so takes 2x c7's bytes to hold the same wall
# time; c9h carries ~2x c8's (2 x 100 + 2 x 20 Mbit vs 100 + 20) and takes 2x
# c8's. Both land near the ~10 s invocation the c7/c8 captures ran at, i.e.
# ~40 windows per rep against the four the 2 s window gave.
case "$CELL" in
  c9)   CA=c2; CB=c2; BYTES=400000000; RUNS=1 ;;
  c9h)  CA=c2; CB=c3; BYTES=50000000;  RUNS=3 ;;
  *) echo "unknown cell $CELL (want c9|c9h)" >&2; exit 2 ;;
esac

# ── THE ARM TABLE ─────────────────────────────────────────────────────────
# Exactly one knob differs between the arms. Both are named explicitly rather
# than one being "the default with nothing set": the `[GATES]` echo is
# two-sided, so an arm that sets nothing cannot be told from an arm whose knob
# failed to forward.
case "$ARM" in
  pooled) ARM_ENV="RWM_STORE_PATHS=1 RWM_STORE_PERCAP=0" ;;
  percap) ARM_ENV="RWM_STORE_PATHS=1 RWM_STORE_PERCAP=1" ;;
  *) echo "unknown arm $ARM (want pooled|percap)" >&2; exit 2 ;;
esac

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2; exit 3
fi

if [ ! -s "$OUT" ]; then
  {
    echo "# c9 quad battery $(date -u +%FT%TZ) seed_spec=$SEED_ARG"
    echo "# binary: $(sha256sum $BIN)"
    echo "# source: $(cat /home/vibe/raptorpath/COMMIT)"
    echo "# kernel: $(uname -r)"
    lscpu | grep "Model name"
    echo "# gauge:  RWM_ACKDIAG=1 RWM_ACKDIAG_WINDOW_US=250000 (NOT the 2 s default)"
    echo "# seeds:  PER-LEG, derived base+1000*i unless the spec pins them"
    echo "# base:   RWM_GEN=0, no RWM_DIAG — identical to the ackdiag capture"
  } >> "$OUT"
fi

t0=$(date +%s)
echo "=== rep=$REP cell=$CELL arm=$ARM seed=$SEED_ARG $CA/$CB/quad bytes=$BYTES runs=$RUNS $(date -u +%FT%TZ)" >> "$OUT"
# Stale-echo hygiene: an aborted invocation must never read the previous run's log.
rm -f /tmp/rwm-c.log /tmp/rwm-s.log
# shellcheck disable=SC2086
env SEED=$SEED_ARG RWM_GEN=0 RWM_ACKDIAG=1 RWM_ACKDIAG_WINDOW_US=250000 $ARM_ENV \
  bash perf_rwm_c.sh "$CA" "$CB" bulk "$BYTES" "$RUNS" quad 2>&1 \
  | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT" || true
echo "RUNTIME $CELL/$ARM rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

# ── LIVENESS, two-sided, on EVERY knob this arm rests on (discipline 15c) ──
# The gauge gate, the WINDOW, and the arm knob are each asserted from the
# `[GATES]` echo on BOTH endpoints, and the gauge is separately proven to have
# EMITTED (discipline 1: prove the mechanism under test executed). The window
# is the one that would otherwise fail silently: a mistyped override resolves
# back to 2 s, the run completes normally, and the ledger would be four
# windows per rep wearing a 250 ms file name.
gate() { grep "\[GATES\]" "$1" 2>/dev/null | tail -1 | grep -o "$2=[0-9]*"; }
for role in c s; do
  L=/tmp/rwm-$role.log
  g_ack=$(gate "$L" RWM_ACKDIAG)
  g_win=$(gate "$L" RWM_ACKDIAG_WINDOW_US)
  g_pc=$(gate "$L" RWM_STORE_PERCAP)
  g_sp=$(gate "$L" RWM_STORE_PATHS)
  echo "LIVENESS $CELL/$ARM rep=$REP role=$role [$g_ack] [$g_win] [$g_pc] [$g_sp]" >> "$OUT"
  [ "$g_ack" != "RWM_ACKDIAG=1" ] && echo "ARM-LIVENESS-FAIL-GAUGE $CELL/$ARM rep=$REP role=$role got='$g_ack'" >> "$OUT"
  [ "$g_win" != "RWM_ACKDIAG_WINDOW_US=250000" ] && echo "ARM-LIVENESS-FAIL-WINDOW $CELL/$ARM rep=$REP role=$role got='$g_win' (VOID: the ledger is NOT at the pre-registered cadence)" >> "$OUT"
  case "$ARM" in
    pooled) [ "$g_pc" != "RWM_STORE_PERCAP=0" ] && echo "ARM-LIVENESS-FAIL-ARM $CELL/$ARM rep=$REP role=$role got='$g_pc'" >> "$OUT" ;;
    percap) [ "$g_pc" != "RWM_STORE_PERCAP=1" ] && echo "ARM-LIVENESS-FAIL-ARM $CELL/$ARM rep=$REP role=$role got='$g_pc'" >> "$OUT" ;;
  esac
done
nc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -c '\[ACKDIAG\]' || true)
echo "LIVENESS-EMIT $CELL/$ARM rep=$REP ackdiag_lines_cli=$nc" >> "$OUT"
[ "$nc" -eq 0 ] && echo "ARM-LIVENESS-FAIL-NOEMIT $CELL/$ARM rep=$REP (VOID: gauge never reported)" >> "$OUT"

# ── THE FOUR-PATH LIVENESS CHECK — the `pid < 2` gate ─────────────────────
# A quad whose gauge only ever names p0 and p1 is the SF bench's truncation
# defect reproduced on the wire, and it would be INVISIBLE in every statistic
# downstream: six pairwise correlations would silently become one. So the
# DISTINCT path ids in this capture are counted and 4 is required. This is the
# check that the bench's `pid < 2` bug taught, written before the first quad
# ledger exists rather than after a verdict was read off a truncated one.
np=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null \
     | grep -oE '\[ACKDIAG\] p[0-9]+' | sort -u | wc -l)
echo "LIVENESS-PATHS $CELL/$ARM rep=$REP distinct_ackdiag_paths=$np" >> "$OUT"
[ "$np" -ne 4 ] && echo "ARM-LIVENESS-FAIL-PATHS $CELL/$ARM rep=$REP got=$np want=4 (VOID: this is not a quad measurement)" >> "$OUT"

# The [ACKDIAG] lines verbatim — THE DATUM. The cell tag carries the ARM, so
# a pooled and a percap window can never be paired by accident downstream.
sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[ACKDIAG\]' \
  | sed "s/^/ACKDIAG ${CELL}-${ARM} rep=$REP /" >> "$OUT"

cp /tmp/rwm-c.log "$DDIR/${CELL}-${ARM}-s${SEED_ARG//,/x}-r${REP}-c.log" 2>/dev/null || true
cp /tmp/rwm-s.log "$DDIR/${CELL}-${ARM}-s${SEED_ARG//,/x}-r${REP}-s.log" 2>/dev/null || true
cp /tmp/rwm-q.txt "$DDIR/${CELL}-${ARM}-s${SEED_ARG//,/x}-r${REP}-q.txt" 2>/dev/null \
  || echo "QCAP-MISSING $CELL/$ARM rep=$REP" >> "$OUT"
echo "RUN-DONE $CELL/$ARM rep=$REP"
