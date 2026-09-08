#!/bin/bash
# THE PLACEMENT BATTERY (Track A of the law search; paper §16.81;
# pre-registration: goal-gate "Placement Battery — PRE-REGISTRATION").
#
#   sudo bash place_battery.sh <seed> [reps]
#
# WHAT IS BEING MEASURED. D0 established that 92–96 % of the holes at a dual
# cell are closed by the OTHER leg catching up — the scheduler manufactures
# them. §16.81 writes the placement law that manufactures them as ONE
# expression continuous in the dial and replaces three of its arbitrary
# constants with derived forms. This battery runs those forms as DEFAULT-ABSENT
# ARMS beside the shipped law.
#
# **THE READING ORDER IS PRE-REGISTERED AND IT IS NOT THE ARMS.** `[LAT]` on
# the CTL arm is read FIRST, before any challenger is looked at, and it may
# re-route the whole track (`INSTRUMENT-INDICTS-QUEUE`). The alternative —
# reading the decomposition after the arms and choosing which to believe — is
# the failure mode this discipline exists to prevent.
#
# ARMS — five, interleaved round-robin per rep (discipline 3):
#
#   CTL      (unset)                      the shipped law; the pinned cost
#                                         table asserts it is byte-identical
#   T0       RWM_PLACE_T=1e-6             the argmin limit — the T dial's own
#                                         floor, an EXISTING knob, so the
#                                         temperature axis has two ends
#   TSIG     RWM_PLACE_T_DERIVED=1        T = (√6/π)·σ̂_e/ref (§16.81.1)
#   HOL      RWM_PLACE_HOL=1              the frontier term X_i (§16.81.2)
#   HOLTSIG  both                         the composition
#
# **A TSIG TIE WITH CTL LICENSES σ̂_e/ref, NEVER 0.15.** That ruling is in the
# pre-registration and is restated here because it is the one result this
# battery is most likely to produce.
#
# CELLS: c7 (symmetric dual), c8 (het dual — the aggregation seat), c1 (SINGLE
# PATH, the must-not-move CONTROL: N = 1 collapses the softmax to an identity,
# so ANY movement at c1 VOIDS the run), c9h (n = 3 quad, WITNESS ONLY —
# `ABORT-QUAD` is pre-declared).
#
# GOODPUT IS A GUARD, PRE-DECLARED UNDERPOWERED AT n = 8. It is reported so a
# regression is visible; it is not a score.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
PLACE_CELLS="${RWM_PLACE_CELLS:-c7 c8 c1 c9h}"
PLACE_ARMS="${RWM_PLACE_ARMS:-CTL T0 TSIG HOL HOLTSIG}"
TAG="${RWM_PLACE_TAG:-place}"
OUT="/home/vibe/placement/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/placement/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# ── THE EARNED + WRITABLE SENTINELS (discipline 7/15) ────────────────────
# A battery that cannot write its own log, or whose binary is not the one the
# pre-registration was written against, must fail at the TOP and not three
# hours in with a half-filled table.
: > "$OUT" 2>/dev/null || { echo "REFUSED: cannot write $OUT" >&2; exit 4; }
BIN=/home/vibe/raptorpath/target/release/raptorpath
[ -x "$BIN" ] || { echo "REFUSED: no engine binary at $BIN" >&2; exit 4; }
# EARNED: the arms must EXIST in this binary. A run whose gate names are not
# in the engine's own echo would silently score five copies of CTL — which is
# exactly what an absent-by-default arm looks like from the outside.
if ! "$BIN" --help >/dev/null 2>&1; then
  echo "REFUSED: engine binary will not run" >&2; exit 4
fi
for G in RWM_PLACE_T_DERIVED RWM_PLACE_HOL; do
  if ! strings "$BIN" 2>/dev/null | grep -q "$G"; then
    echo "REFUSED: $G is not present in the binary — this is the OLD ENGINE" \
      | tee -a "$OUT" >&2
    exit 5
  fi
done
# BOTH LOCKS: no other engine may be running, and no second battery may start.
if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi
LOCK=/tmp/rwm-place-battery.lock
if ! mkdir "$LOCK" 2>/dev/null; then
  echo "BUSY: another place_battery holds $LOCK -- aborting" | tee -a "$OUT" >&2
  exit 3
fi
trap 'rmdir "$LOCK" 2>/dev/null || true' EXIT

arm_env() {
  case "$1" in
    CTL)     echo "" ;;
    T0)      echo "RWM_PLACE_T=1e-6" ;;
    TSIG)    echo "RWM_PLACE_T_DERIVED=1" ;;
    HOL)     echo "RWM_PLACE_HOL=1" ;;
    HOLTSIG) echo "RWM_PLACE_T_DERIVED=1 RWM_PLACE_HOL=1" ;;
  esac
}
# The EXPECTED resolved value of each arm gate, for the two-sided liveness
# check below. An arm that does not echo what it was configured for is a
# WIRING-FAILS row and contributes no datum.
arm_td() { case "$1" in TSIG|HOLTSIG) echo 1 ;; *) echo 0 ;; esac; }
arm_hl() { case "$1" in HOL|HOLTSIG)  echo 1 ;; *) echo 0 ;; esac; }

# cell -> "scenA scenB mode bytes"
cell_spec() {
  case "$1" in
    c7)  echo "c2 c2 dual 200000000" ;;
    c8)  echo "c2 c3 dual 100000000" ;;
    c1)  echo "c1 c1 single 400000000" ;;
    c9h) echo "c2 c3 quad 100000000" ;;
    *) echo "" ;;
  esac
}
# The n each cell gets. c9h is a WITNESS at n = 3, pre-declared ABORT-QUAD:
# the quad's instability is a known finding and the row is liveness, not a
# score.
cell_reps() { case "$1" in c9h) echo 3 ;; *) echo "$REPS" ;; esac; }

run_one() { # cell arm
  local cell="$1" arm="$2"
  case " $PLACE_CELLS " in *" $cell "*) ;; *) return 0 ;; esac
  case " $PLACE_ARMS " in *" $arm "*) ;; *) return 0 ;; esac
  [ "$REP" -le "$(cell_reps "$cell")" ] || return 0
  local name="$cell-$arm"
  local envs etd ehl ca cb mode bytes
  envs="$(arm_env "$arm")"; etd="$(arm_td "$arm")"; ehl="$(arm_hl "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  python3 ./place_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log \
    >> "$OUT" 2>&1 || echo "PLACERESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── LIVENESS, TWO-SIDED ON BOTH ENDPOINTS (discipline 15c) ────────────
  # Scoped to the `[GATES]` line: the resolve-time liveness echoes carry the
  # gate NAMES in their prose, and an unscoped grep reads the documentation
  # instead of the resolved value.
  local gtc gts ghc ghs etan lat succ
  gtc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_T_DERIVED=[01]")
  gts=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_T_DERIVED=[01]")
  ghc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_HOL=[01]")
  ghs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_HOL=[01]")
  etan=$(grep -c "\[ETA\] site=sender" /tmp/rwm-c.log 2>/dev/null || true)
  lat=$(grep -c "\[LAT\] site=receiver" /tmp/rwm-s.log 2>/dev/null || true)
  succ=$(grep -c "\[SUCC\]" /tmp/rwm-s.log 2>/dev/null || true)
  echo "LIVENESS $name rep=$REP cli=[$gtc $ghc] srv=[$gts $ghs] eta_lines=$etan lat_lines=$lat succ_lines=$succ (expect td=$etd hol=$ehl)" >> "$OUT"
  [ "$gtc" != "RWM_PLACE_T_DERIVED=$etd" ] && echo "ARM-LIVENESS-FAIL-TD-CLI $name rep=$REP got='$gtc'" >> "$OUT"
  [ "$gts" != "RWM_PLACE_T_DERIVED=$etd" ] && echo "ARM-LIVENESS-FAIL-TD-SRV $name rep=$REP got='$gts'" >> "$OUT"
  [ "$ghc" != "RWM_PLACE_HOL=$ehl" ] && echo "ARM-LIVENESS-FAIL-HOL-CLI $name rep=$REP got='$ghc'" >> "$OUT"
  [ "$ghs" != "RWM_PLACE_HOL=$ehl" ] && echo "ARM-LIVENESS-FAIL-HOL-SRV $name rep=$REP got='$ghs'" >> "$OUT"
  # THE INSTRUMENTS' OWN LIVENESS. A battery whose gauges are absent has not
  # measured what it claims to measure.
  if [ -n "$gtc" ]; then
    [ "$etan" -eq 0 ] && echo "INSTRUMENT-FAIL-ETA $name rep=$REP" >> "$OUT"
    [ "$lat" -eq 0 ] && echo "INSTRUMENT-FAIL-LAT $name rep=$REP" >> "$OUT"
    [ "$succ" -eq 0 ] && echo "INSTRUMENT-FAIL-SUCC $name rep=$REP" >> "$OUT"
  fi

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

echo "=== PLACE BATTERY seed=$SEED_ARG reps=$REPS cells='$PLACE_CELLS' arms='$PLACE_ARMS' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary sha256 $(sha256sum "$BIN" | cut -d' ' -f1)" >> "$OUT"
echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

for REP in $(seq 1 "$REPS"); do
  for CELL in $PLACE_CELLS; do
    for ARM in $PLACE_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# ── THE AGGREGATION SINGLES (the guard's denominator) ────────────────────
# `c7 >= 0.97*sum` and `c8 >= 0.87*sum` are read against SAME-SESSION singles,
# never against a number from another run: the shaper, the host and the kernel
# all move between sessions and an aggregation ratio against a stale
# denominator is not a ratio.
for REP in $(seq 1 "$REPS"); do
  for S in sc2 sc3; do
    case " $PLACE_CELLS " in *" c7 "*|*" c8 "*) ;; *) continue ;; esac
    read -r sa sb smode sbytes <<< "$(case "$S" in
      sc2) echo "c2 c2 single 100000000" ;;
      sc3) echo "c3 c3 single 25000000" ;;
    esac)"
    echo "=== rep=$REP arm=$S-SINGLE seed=$SEED_ARG env=\"\" cell=$sa/$sb/$smode bytes=$sbytes $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 \
      bash perf_rwm_c.sh "$sa" "$sb" bulk "$sbytes" 1 "$smode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT" || true
    python3 ./place_parse.py "$S" SINGLE "$SEED_ARG" "$REP" \
        /tmp/rwm-c.log /tmp/rwm-s.log >> "$OUT" 2>&1 \
      || echo "PLACERESULT-PARSE-FAIL $S-SINGLE rep=$REP" >> "$OUT"
  done
done

# Per-arm result-count tally: an arm that VANISHED must fail loudly rather
# than quietly reduce an n (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $PLACE_CELLS; do
  for A in $PLACE_ARMS; do
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$(cell_reps "$CELL")" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "PLACE-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
