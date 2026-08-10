#!/bin/bash
# THE LATENCY-SCORED BATTERY (goal-gate "Latency Lever").
#
#   sudo bash lat_battery.sh <seed> [reps]
#
# The three-term battery scored the outstanding-data limit on THROUGHPUT and
# found nothing outside noise except a regression. "What Binds Throughput"
# then showed why: at five of eight cell-seeds the shipped default was
# already at 97-100 % of the shaped link, so the limit could only move DELAY.
# This battery scores it on the axis it actually moves.
#
# ARMS — three, not four. Arm C (RWM_THREE_TERM alone, the x4096 anchor
# over-read) was already scored and is not the law; dropping it buys back a
# quarter of the wall time, which is spent on reps instead.
#
#   A   (unset)                            the shipped default
#   B   RWM_THREE_TERM=1 RWM_PLAIN_RS=1    THE SCORED ARM (the composed law)
#   D   RWM_PLAIN_RS=1                     attribution control: the anchor
#                                          alone costs 35 % at c1 and ~12 %
#                                          at c7, so B-vs-A cannot be read
#                                          without it
#
# Interleaved round-robin per rep (discipline 3), fresh topology per
# invocation, RWM_DIAG=1 and RWM_LATPROBE=1 in every arm.
#
# THE PROBE. RWM_LATPROBE=1 makes perf_rwm_c.sh run an independent 20 pkt/s
# ICMP flow through the same shaped qdisc for the duration of the bulk
# transfer. That is the score: delivered round-trip time of a flow that is
# NOT the code under test, measured by the kernel, present in every arm.
#
# Cells: selected by the HEADROOM CHECK (discipline 16), not by habit. See
# the pre-registration.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
LAT_CELLS="${RWM_LAT_CELLS:-sc2 sc3 c2r100 c7 c2r200 c1}"
TAG="${RWM_LAT_TAG:-lat}"
OUT="/home/vibe/latlever/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/latlever/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

LAW="RWM_THREE_TERM=1 RWM_PLAIN_RS=1"
ANC="RWM_PLAIN_RS=1"

arm_env() { case "$1" in A) echo "" ;; B) echo "$LAW" ;; D) echo "$ANC" ;; esac; }
arm_3t()  { case "$1" in B) echo 1 ;; *) echo 0 ;; esac; }
arm_rs()  { case "$1" in B|D) echo 1 ;; *) echo 0 ;; esac; }

# cell -> "scenA scenB mode bytes"
cell_spec() {
  case "$1" in
    c1)     echo "c1 c1 single 400000000" ;;
    c7)     echo "c2 c2 dual 200000000" ;;
    sc2)    echo "c2 c2 single 100000000" ;;
    sc3)    echo "c3 c3 single 25000000" ;;
    c2r100) echo "c2r100 c2r100 single 100000000" ;;
    c2r200) echo "c2r200 c2r200 single 50000000" ;;
    *) echo "" ;;
  esac
}

run_one() { # cell arm
  local cell="$1" arm="$2"
  case " $LAT_CELLS " in *" $cell "*) ;; *) return 0 ;; esac
  local name="$cell-$arm"
  local envs e3t ers ca cb mode bytes
  envs="$(arm_env "$arm")"; e3t="$(arm_3t "$arm")"; ers="$(arm_rs "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate. The ping/qdisc captures are cleared
  # by perf_rwm_c.sh itself, at entry, for the same reason.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  python3 ./lat_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-ping.txt /tmp/rwm-q.txt \
    >> "$OUT" 2>&1 || echo "LATRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── LIVENESS, TWO-SIDED on BOTH endpoints (discipline 15c) ────────────
  # Scoped to the `[GATES]` line: the resolve-time ACTIVE echo's own PROSE
  # contains the literal `RWM_THREE_TERM=0`, and an unscoped grep reads the
  # documentation instead of the resolved value.
  local g3c g3s grc grs act eng wt dq pn
  g3c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  g3s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  grc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  grs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  act=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  eng=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep '\[3T\]' | grep -c "eng=1" || true)
  # THE NEW INSTRUMENTS' OWN LIVENESS ECHOES. A battery whose new gauges are
  # absent has not measured what it claims to measure, and this branch exists
  # BECAUSE the last one ran with `stall[` in 0 of 1 116 logs and nobody
  # noticed until afterwards.
  wt=$(grep -c "wait\[tun=" /tmp/rwm-c.log 2>/dev/null || true)
  dq=$(grep -c "dgq0\[hand=" /tmp/rwm-c.log 2>/dev/null || true)
  pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  echo "LIVENESS $name rep=$REP cli=[$g3c $grc] srv=[$g3s $grs] active=$act eng1_lines=$eng wait_lines=$wt dgq_lines=$dq ping_replies=$pn (expect 3t=$e3t rs=$ers)" >> "$OUT"
  [ "$g3c" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-CLI $name rep=$REP got='$g3c'" >> "$OUT"
  [ "$g3s" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-SRV $name rep=$REP got='$g3s'" >> "$OUT"
  [ "$grc" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-CLI $name rep=$REP got='$grc'" >> "$OUT"
  [ "$grs" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-SRV $name rep=$REP got='$grs'" >> "$OUT"
  if [ "$e3t" = "1" ]; then
    [ "$act" -eq 0 ] && echo "ARM-LIVENESS-FAIL-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -eq 0 ] && echo "ARM-LIVENESS-FAIL-3T $name rep=$REP (VOID: no eng=1)" >> "$OUT"
  else
    [ "$act" -gt 0 ] && echo "ARM-CONTAMINATION-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -gt 0 ] && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
  fi
  # An invocation that produced NO [GATES] on either endpoint is an ABORT and
  # contributes no datum; one that produced them but no probe/gauge is an
  # INSTRUMENT failure and must be loud, because it is silently scoreable.
  if [ "${gates_any:-1}" = "1" ] && [ -n "$g3c" ]; then
    [ "$wt" -eq 0 ] && echo "INSTRUMENT-FAIL-WAIT $name rep=$REP" >> "$OUT"
    [ "$dq" -eq 0 ] && echo "INSTRUMENT-FAIL-DGQ $name rep=$REP" >> "$OUT"
    [ "$pn" -eq 0 ] && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  fi

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null \
    || echo "PROBE-MISSING $name rep=$REP" >> "$OUT"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

echo "=== LAT BATTERY seed=$SEED_ARG reps=$REPS cells='$LAT_CELLS' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary sha256 $(sha256sum /home/vibe/raptorpath/target/release/raptorpath | cut -d' ' -f1)" >> "$OUT"
echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

for REP in $(seq 1 "$REPS"); do
  for CELL in $LAT_CELLS; do
    run_one "$CELL" A
    run_one "$CELL" B
    run_one "$CELL" D
  done
done

# Per-arm result-count tally: an arm that VANISHED must fail loudly rather
# than quietly reduce an n (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $LAT_CELLS; do
  for A in A B D; do
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "LAT-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
