#!/bin/bash
# ACK-CADENCE MEASUREMENT (VM) — the wire reading of the instrument built in
# goal-gate "Ack-Cadence Gauge — THE INSTRUMENT".
#
# THIS IS AN INSTRUMENT READING, NOT A CRITERION SCORE. One arm, one seed.
#   arm: RWM_ACKDIAG=1 and nothing else (the SHIPPED DEFAULT's echo stream)
#   base env: SEED=42 RWM_GEN=0 — identical to every recent L1 battery
#             (tt/hi/copaclean), i.e. the plain-window pipeline the store-cap
#             and anchor ledgers are all scored on. NO RWM_DIAG: the gauge is
#             deliberately independent of it and the point is to read the
#             default without paying the 250 ms report.
#
# Cells: c7 (c2/c2 dual 200MB), c8 (c2/c3 dual 25MB), c2r100 (single 100MB).
#
#   usage: sudo bash ackdiag_battery.sh <cell> <rep> [seed]
set -u
[ "$(id -u)" -eq 0 ] || { echo "must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
CELL="$1"; REP="$2"; SEED_ARG="${3:-42}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
DDIR=/home/vibe/ackdiag
OUT=$DDIR/ackdiag-s${SEED_ARG}.log
mkdir -p "$DDIR"

case "$CELL" in
  c7)     CA=c2;     CB=c2;     MODE=dual;   BYTES=200000000; RUNS=1 ;;
  c8)     CA=c2;     CB=c3;     MODE=dual;   BYTES=25000000;  RUNS=3 ;;
  c2r100) CA=c2r100; CB=c2r100; MODE=single; BYTES=100000000; RUNS=1 ;;
  *) echo "unknown cell $CELL" >&2; exit 2 ;;
esac

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2; exit 3
fi

if [ ! -s "$OUT" ]; then
  {
    echo "# ackdiag measurement $(date -u +%FT%TZ) seed=$SEED_ARG"
    echo "# binary: $(sha256sum $BIN)"
    echo "# source: $(cat /home/vibe/raptorpath/COMMIT)"
    echo "# kernel: $(uname -r)"
    lscpu | grep "Model name"
    echo "# arm: RWM_ACKDIAG=1 (only); base env SEED=$SEED_ARG RWM_GEN=0; no RWM_DIAG"
  } >> "$OUT"
fi

t0=$(date +%s)
echo "=== rep=$REP cell=$CELL seed=$SEED_ARG $CA/$CB/$MODE bytes=$BYTES runs=$RUNS $(date -u +%FT%TZ)" >> "$OUT"
# Stale-echo hygiene: an aborted invocation must never read the previous run's log.
rm -f /tmp/rwm-c.log /tmp/rwm-s.log
env SEED=$SEED_ARG RWM_GEN=0 RWM_ACKDIAG=1 \
  bash perf_rwm_c.sh "$CA" "$CB" bulk "$BYTES" "$RUNS" "$MODE" 2>&1 \
  | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT" || true
echo "RUNTIME $CELL rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

# LIVENESS, two-sided (discipline 15c): the gate must read 1 on BOTH endpoints
# and the gauge must have actually EMITTED (matrix/discipline 1: prove the
# mechanism under test executed).
gc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")
gs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")
nc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -c '\[ACKDIAG\]' || true)
ns=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -c '\[ACKDIAG\]' || true)
echo "LIVENESS $CELL rep=$REP cli=[$gc] srv=[$gs] ackdiag_lines_cli=$nc srv=$ns" >> "$OUT"
[ "$gc" != "RWM_ACKDIAG=1" ] && echo "ARM-LIVENESS-FAIL-GATE-CLI $CELL rep=$REP got='$gc'" >> "$OUT"
[ "$gs" != "RWM_ACKDIAG=1" ] && echo "ARM-LIVENESS-FAIL-GATE-SRV $CELL rep=$REP got='$gs'" >> "$OUT"
[ "$nc" -eq 0 ] && echo "ARM-LIVENESS-FAIL-NOEMIT $CELL rep=$REP (VOID: gauge never reported)" >> "$OUT"

# The [ACKDIAG] lines verbatim — THE DATUM. Unwrapped to one line per report
# (the emitter wraps at the tracing layer only; grep -A keeps continuations).
sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[ACKDIAG\]' \
  | sed "s/^/ACKDIAG $CELL rep=$REP /" >> "$OUT"

cp /tmp/rwm-c.log "$DDIR/${CELL}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
cp /tmp/rwm-s.log "$DDIR/${CELL}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
cp /tmp/rwm-q.txt "$DDIR/${CELL}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
  || echo "QCAP-MISSING $CELL rep=$REP" >> "$OUT"
echo "RUN-DONE $CELL rep=$REP"
