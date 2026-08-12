#!/bin/bash
# THE COMPOSED-CAP BATTERY — the VM battery for goal-gate "Composed-Cap Battery
# — VM PRE-REGISTRATION" (own commit, written before any VM contact). That block
# is the CONTRACT: it is scored against, never modified, and no number in it may
# change now that the VM has been touched.
#
#   sudo bash ccap_battery.sh <seed> [reps]
#
# ADR-0070 Deliverable 3 step 4, run only after its steps 1-3 landed: the
# law-shape prevention kit, the [WALL] onset/duration instrument, and the
# composed law as ONE SF-bench arm.
#
# ── ARMS ────────────────────────────────────────────────────────────────
#   A   (unset)                THE SHIPPED DEFAULT — path_scaled_store_cap =
#                              clamp(gain*N*Sigma, 64, max(N*knee, 64)), the law
#                              ADR-0070 puts on trial. Pinned at its ceiling in
#                              121 of 126 dual reps across five sessions.
#   C   RWM_COMPOSED_CAP=1     THE COMPOSED LAW, AS ONE ARM — the three-term
#                              pool at the head of the plain dyn-cap chain, the
#                              unified live set at the BRAKE, and the late-stage
#                              per-path brake whose cap is the path's OWN cwnd.
#                              Composing is all it does: no law and no constant
#                              of its own.
#
# C IS NOT `BHU`. The wire's nearest prior arm also carried RWM_PLAIN_RS (whose
# 1.09-1.10x c7 CPU class is 16.50's F4 blocker), RWM_HONEST_K and
# RWM_STORE_CAP_UNIFIED. C carries NONE of them, and carries one thing BHU never
# had: the brake. Every BHU/DHU number in the pre-registration is labelled
# CONTEXT, never a prediction, for exactly this reason.
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c1   c1/c1 single  400 MB  1 Gbit    n=8   ~75% headroom — and NO throughput
#                                              target is written, because this
#                                              law has never run at 1 Gbit in
#                                              ANY layer (the SF bench states
#                                              three times that it refused to
#                                              invent a 1 Gbit geometry).
#   c7   c2/c2 dual    200 MB  200 Mbit  n=8   ~3% headroom — parity floor only.
#                                              The bench's 1.12x CANNOT be asked
#                                              here: +12% is 224 Mbit on a
#                                              200 Mbit link (discipline 16).
#   c8   c2/c3 dual     25 MB  120 Mbit  n=12  THE DEAD-WALL CELL. n=12 is the
#                                              mode-rate lesson.
#   c8L  c2/c3 dual    200 MB  120 Mbit  n=12  THE LENGTH AXIS — the dead-wall
#                                              regime boundary (16.54's 0/24).
#   sc2  c2/c2 single  100 MB  100 Mbit  n=8   ZERO headroom — parity + the
#                                              halved-latency survival only.
#
# Both arms run at EVERY cell. Unlike the mode-hunt battery there is no
# cell-restricted arm: the composed law is a whole-chain replacement and a
# missing control at any cell would leave that cell unscoreable.
#
# ── THE PRIMARY READOUTS ────────────────────────────────────────────────
# `[CCAP]` (engagement + the two surviving bind fractions + the brake) and
# `[WALL]` (the terminal window's onset and duration). NOT the tick-share
# dead-wall flag, which is parsed and emitted as a WITNESS and scored on
# nothing -- its arm orderings inverted between pools minutes apart, which is
# why RWM_WALLDIAG exists at all.
#
# ── INSTRUMENTS on every invocation, BOTH ARMS ──────────────────────────
# RWM_DIAG=1 (occupancy win=occ/cap, the wait histogram, khr/kraw, retx, [SF]),
# RWM_ACKDIAG=1 (the ack-cadence gauge -- its absence is an INSTRUMENT-FAIL,
# never a datum), RWM_WALLDIAG=1 (the dead-wall instrument, ON IN BOTH ARMS
# because the comparison is between arms), RWM_LATPROBE=1 (the delivered-latency
# probe P-LATENCY-SC2 is read off), the CPU gauge, and the tc -s qdisc capture
# beside EVERY target on EVERY cell -- not a subset (discipline 16b).
#
# ── LIVENESS, asserted per arm BEFORE any number is read (discipline 1/15) ─
#   * `[GATES] RWM_COMPOSED_CAP=` TWO-SIDED on BOTH endpoints — =1 on C, =0 on A.
#   * `[GATES] RWM_WALLDIAG=1` and `RWM_ACKDIAG=1` on BOTH endpoints, BOTH arms.
#   * `three-term outstanding limit ACTIVE` PRESENT on C, ABSENT on A. This is
#     the echo that proves the composed gate reached the POOL SEAT (through
#     sender_policy's `three_term_on = (three_term || composed_cap)`), and it is
#     a separate claim from [CCAP] reporting, which proves the gauge reached
#     TEARDOWN.
#   * `[CCAP]` PRESENT on C, ABSENT on A (emitted only under pol.composed_cap).
#     `eng=0/N` on a C rep is a WARM-UP FAILURE, not a null result: flagged as
#     INSTRUMENT-FAIL here and excluded from that cell's scoring by the reporter.
#   * `[WALL]` PRESENT on BOTH arms. Absent = the S-WALL claim has no datum.
#   * `unified store-cap path set ACTIVE` expected ABSENT on BOTH arms, and that
#     is CORRECT rather than a defect: RWM_COMPOSED_CAP does not set
#     RWM_STORE_CAP_UNIFIED. The pool law already reads live_paths()
#     unconditionally and the composed arm's unified set is at the BRAKE, whose
#     liveness is `[CCAP] brake=`. Its presence would be CONTAMINATION and is
#     flagged as such; its absence is recorded, loudly, so no later reader
#     mistakes the silence for a disarmed arm.
#   * `[GATES] RWM_THREE_TERM=` and `RWM_RECOV_MP=` RECORDED on every arm as
#     witnesses, not arms.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in ccap_parse.py (no summary at
# all = ABORT). The seed-7 topo-ping abort class is handled by SYMMETRIC top-up
# sessions only, never asymmetric ones.
#
# ARMCOUNT BELOW IS NOT AN n. It counts PARSED ROWS and an aborted invocation
# still emits a row. The scored n is ccap_report.py's LIVE n, recomputed from
# the gates columns. This is the trap that made the predecessor's top-up
# trigger visible only through the reporter.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
CC_CELLS="${RWM_CC_CELLS:-c1 c7 c8 c8L sc2}"
CC_ARMS="${RWM_CC_ARMS:-A C}"
TAG="${RWM_CC_TAG:-ccap}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/ccap/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/ccap/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

arm_env() { case "$1" in
  A) echo "" ;;
  C) echo "RWM_COMPOSED_CAP=1" ;;
esac; }
arm_cc() { case "$1" in C) echo 1 ;; *) echo 0 ;; esac; }

# cell -> "scenA scenB mode bytes"  (identical geometry to flip_battery.sh and
# deadwall_battery.sh -- the cells are transcribed, never redefined here)
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
# THE PER-CELL n. n=12 at both c8 cells is the mode-rate lesson (a low-base-rate
# per-rep statistic needs the reps where the decision is taken); n=8 elsewhere.
# Applied INSIDE the interleaved loop, never as a separate pass, so every rep
# sits in the same round-robin on the same topologies as the reps it is
# compared against.
cell_reps() { case "$1" in c8|c8L) echo "$REPS" ;; *) echo "${RWM_CC_SMALLREPS:-8}" ;; esac; }

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local ecc
  ecc="$(arm_cc "$arm")"

  python3 ./ccap_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "CCAP-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the ACTIVE echoes' own prose contains literal
  # `RWM_*=0` strings (the amendment-1 lesson from the flip battery).
  local gcc gcs gwc gws gac gas gtc gts guc gus gmc gms
  gcc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_COMPOSED_CAP=[01]")
  gcs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_COMPOSED_CAP=[01]")
  gwc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_WALLDIAG=[01]")
  gws=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_WALLDIAG=[01]")
  gac=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")
  gas=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")
  gtc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  gts=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  guc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  gus=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  gmc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_RECOV_MP=[01]")
  gms=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_RECOV_MP=[01]")

  local ttc tts uc us akc ccn wln
  ttc=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  tts=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  uc=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  us=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  akc=$(grep -c "\[ACKDIAG\]" /tmp/rwm-c.log 2>/dev/null || true)
  ccn=$(grep -c "\[CCAP\]" /tmp/rwm-c.log 2>/dev/null || true)
  wln=$(grep -c "\[WALL\]" /tmp/rwm-c.log 2>/dev/null || true)

  echo "LIVENESS $name rep=$REP cli=[$gcc $gwc $gac $gtc $guc $gmc] srv=[$gcs $gws $gas $gts $gus $gms] act3T=$ttc/$tts actU=$uc/$us ackdiag=$akc ccap=$ccn wall=$wln (expect cc=$ecc)" >> "$OUT"

  # The gauges' OWN lines, verbatim, one per echo — so the ledger carries the
  # readout even if the parser ever changes its mind about a column.
  (grep -h "\[CCAP\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
    | sed "s/^.*\(\[CCAP\]\)/CCAPLINE $name rep=$REP \1/" >> "$OUT") || true
  (grep -h "\[WALL\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
    | sed "s/^.*\(\[WALL\]\)/WALLLINE $name rep=$REP \1/" >> "$OUT") || true

  # No [GATES] on EITHER endpoint = ABORT: no datum, no liveness verdict, and
  # NOT in any denominator.
  if [ -z "$gcc" ] && [ -z "$gcs" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  # THE ARM GATE, two-sided, both endpoints. An arm that cannot show its
  # control was a control has measured one condition twice (discipline 15c).
  [ "$gcc" != "RWM_COMPOSED_CAP=$ecc" ] && echo "ARM-LIVENESS-FAIL-CC-CLI $name rep=$REP got='$gcc'" >> "$OUT"
  [ "$gcs" != "RWM_COMPOSED_CAP=$ecc" ] && echo "ARM-LIVENESS-FAIL-CC-SRV $name rep=$REP got='$gcs'" >> "$OUT"

  # The instruments must be armed on both endpoints or their columns are void.
  { [ "$gwc" != "RWM_WALLDIAG=1" ] || [ "$gws" != "RWM_WALLDIAG=1" ]; } \
    && echo "INSTRUMENT-FAIL-WALLDIAG-GATE $name rep=$REP cli='$gwc' srv='$gws'" >> "$OUT"
  { [ "$gac" != "RWM_ACKDIAG=1" ] || [ "$gas" != "RWM_ACKDIAG=1" ]; } \
    && echo "INSTRUMENT-FAIL-ACKDIAG-GATE $name rep=$REP cli='$gac' srv='$gas'" >> "$OUT"

  # WITNESSES, recorded loudly when they are not what the whole battery
  # assumes. RWM_THREE_TERM stays 0 on BOTH arms — the composed gate reaches
  # the pool seat WITHOUT it — and RWM_STORE_CAP_UNIFIED stays 0 on both.
  { [ "$gtc" != "RWM_THREE_TERM=0" ] || [ "$gts" != "RWM_THREE_TERM=0" ]; } \
    && echo "WITNESS-UNEXPECTED-3T $name rep=$REP cli='$gtc' srv='$gts'" >> "$OUT"
  { [ "$guc" != "RWM_STORE_CAP_UNIFIED=0" ] || [ "$gus" != "RWM_STORE_CAP_UNIFIED=0" ]; } \
    && echo "WITNESS-UNEXPECTED-U $name rep=$REP cli='$guc' srv='$gus'" >> "$OUT"
  { [ "$gmc" != "RWM_RECOV_MP=1" ] || [ "$gms" != "RWM_RECOV_MP=1" ]; } \
    && echo "WITNESS-UNEXPECTED-RECOVMP $name rep=$REP cli='$gmc' srv='$gms'" >> "$OUT"

  # THE POOL SEAT's echo: PRESENT on C, ABSENT on A. The sender site echoes on
  # the CLIENT log and the receiver site on the SERVER log; only the sender's
  # policy resolves the plain dyn-cap chain on every bulk transfer, so a clean
  # C rep can legitimately carry the client echo alone. VOID means BOTH sides
  # silent; a one-sided echo is RECORDED and left to scoring.
  if [ "$ecc" = "1" ]; then
    if [ "$ttc" -eq 0 ] && [ "$tts" -eq 0 ]; then
      echo "ARM-LIVENESS-FAIL-3T-ECHO $name rep=$REP (VOID: neither site ran the pool law)" >> "$OUT"
    elif [ "$ttc" -eq 0 ] || [ "$tts" -eq 0 ]; then
      echo "TT-ECHO-ONE-SIDED $name rep=$REP (cli=$ttc srv=$tts — recorded, not void)" >> "$OUT"
    fi
    # The composition's own gauge. Absent = the arm cannot be read as either a
    # null result or a null effect, which is the whole point of [CCAP].
    [ "$ccn" -eq 0 ] && echo "INSTRUMENT-FAIL-CCAP $name rep=$REP (no [CCAP] line on the client)" >> "$OUT"
    # eng=0/N with the gate ON is a WARM-UP FAILURE, NOT a null result. Named
    # on the rep so the two can never be confused at scoring time.
    (grep -h "\[CCAP\] eng=0/" /tmp/rwm-c.log >/dev/null 2>&1) \
      && echo "CCAP-WARMUP-FAIL $name rep=$REP (eng=0/N with RWM_COMPOSED_CAP=1 — no datum)" >> "$OUT"
  else
    { [ "$ttc" -gt 0 ] || [ "$tts" -gt 0 ]; } && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
    [ "$ccn" -gt 0 ] && echo "ARM-CONTAMINATION-CCAP $name rep=$REP" >> "$OUT"
  fi

  # Expected ABSENT on BOTH arms. Presence is CONTAMINATION, not a bonus.
  { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } \
    && echo "ARM-CONTAMINATION-U $name rep=$REP (cli=$uc srv=$us — RWM_COMPOSED_CAP does not set the U bit)" >> "$OUT"

  # The dead-wall instrument, ON IN BOTH ARMS: without it the S-WALL claim and
  # P-WALL-LENGTH have no datum on this rep.
  [ "$wln" -eq 0 ] && echo "INSTRUMENT-FAIL-WALL $name rep=$REP (no [WALL] line on the client)" >> "$OUT"

  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  [ "$akc" -eq 0 ] && echo "INSTRUMENT-FAIL-ACKDIAG $name rep=$REP (no [ACKDIAG] line on the client)" >> "$OUT"
  # The tick-share WITNESS's own instrument. Scored on nothing, but its absence
  # must be visible so the old-vs-new measurand comparison knows its own n.
  local wn
  wn=$(grep -c "wait\[tun=" /tmp/rwm-c.log 2>/dev/null || true)
  [ "$wn" -eq 0 ] && echo "WITNESS-NO-WAIT $name rep=$REP (no wait histogram — tick-share witness has no value)" >> "$OUT"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs ca cb mode bytes
  envs="$(arm_env "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

  # P-LATENCY-SC2's probe is load-bearing at sc2 and is captured everywhere.
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  # discipline 16b: the shaped device's own counters, on EVERY cell.
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $CC_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $CC_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  [ "$REP" -le "$(cell_reps "$1")" ] || return 0
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== COMPOSED-CAP BATTERY seed=$SEED_ARG reps=$REPS smallreps=${RWM_CC_SMALLREPS:-8} cells='$CC_CELLS' arms='$CC_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"Composed-Cap Battery — VM PRE-REGISTRATION\" (commit 1e09c00)"
  echo "=== ARMS A = shipped default | C = RWM_COMPOSED_CAP=1 (three-term pool + unified live set at the brake + late-stage per-path brake on the path's OWN cwnd)"
  echo "=== READOUTS [CCAP] eng/cap/mem/floor/brake and [WALL] onset/dur_ms — NOT the tick-share flag, which is a WITNESS scored on nothing"
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm (desktop session, recorded per era honesty)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPS"); do
  for CELL in $CC_CELLS; do
    for ARM in $CC_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
# ARMCOUNT IS NOT AN n — it counts PARSED ROWS and aborts emit rows. The scored
# n is ccap_report.py's LIVE n, recomputed from the gates columns.
echo "=== ARMCOUNTS (rows, NOT live n — see ccap_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $CC_CELLS; do
  for A in $CC_ARMS; do
    WANT=$(cell_reps "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "CCAP-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo CCAP-BATTERY-DONE-$SEED_ARG
