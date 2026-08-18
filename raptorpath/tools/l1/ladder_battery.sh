#!/bin/bash
# THE LADDER BATTERY — the VM battery for goal-gate "Ladder Battery —
# PRE-REGISTRATION" (own commit, written before this file existed and before any
# VM contact). That block is the CONTRACT: it is scored against, never modified,
# and no number in it may change now that the VM has been touched.
#
#   sudo bash ladder_battery.sh <seed> [reps]
#
# SHIPPED-LAW CLEANUP item 2. It scores the five gates items 1, 3 and 5 shipped
# DEFAULT OFF, as a LADDER of rungs on ONE binary from main@5ddf7f6.
#
# ── ARMS ────────────────────────────────────────────────────────────────
#   A     (unset)                    THE SHIPPED DEFAULT — clamp(gain*N*Sigma,
#                                    floor, N*knee), the contaminated eps-hat,
#                                    the leaky ledger, active_paths() at the
#                                    cap, no brake. The control every rung is
#                                    scored against, re-measured same-session.
#   N     RWM_SUM_CAP=1              THE xN DELETION (16.62) — clamp(gain*Sigma,
#                                    floor, N*knee). Exactly one factor changes;
#                                    gain, knee, floor, the Sigma-set and the
#                                    estimator are IDENTICAL on both arms and
#                                    cancel out of the comparison.
#   T     the ledger/loss trio       RWM_LOSS_SENT_TRUTH + RWM_CHARGE_RECOVERY +
#                                    RWM_RELEASE_1TO1. Read by the SCHEDULER
#                                    through its own cached process-globals,
#                                    never through SenderPolicy.
#   NT    N + T                      The composition test for the ONE real
#                                    interaction item 1 recorded: RELEASE_1TO1
#                                    moves in_flight (the brake's operand and
#                                    the admission gate's input) while the cap
#                                    law moves nothing the trio reads.
#   FULL  NT + UNIFIED + LATE_BRAKE  The goal's FULL arm, item 1 PART 3's exact
#                                    six-gate list — NOT expressible on main
#                                    until RWM_LATE_BRAKE was extracted.
#                                    RWM_COMPOSED_CAP and RWM_THREE_TERM stay 0:
#                                    FULL carries the xN deletion INSTEAD of the
#                                    composed pool law, not as well.
#
# THE ENV IS DERIVED FROM THE ECHO EXPECTATIONS TABLE (`gate_expect` below), not
# written twice. An arm cannot be launched with an env its own liveness gate
# does not expect, which is the drift `ccap_battery.sh` avoided by hand and this
# one avoids by construction.
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c1   c1/c1 single  400 MB  1 Gbit    n=8   the N=1 IDENTITY check (both cap
#                                              arms bit-identical by
#                                              construction) + the one cell with
#                                              headroom.
#   c7   c2/c2 dual    200 MB  200 Mbit  n=8   THE CLEAN N RUNG — symmetric,
#                                              interior by arithmetic, 2.37x
#                                              headroom over its own W+S=1379.
#   c8   c2/c3 dual     25 MB  120 Mbit  n=12  THE LOAD-BEARING N RUNG and the
#                                              dead-wall cell. The corrected cap
#                                              3020 is 0.71x the cell's own
#                                              W+S=4232: the deletion is
#                                              PREDICTED to under-fund the span
#                                              by 29% here.
#   c8L  c2/c3 dual    200 MB  120 Mbit  n=12  The length axis. PRE-DECLARED
#                                              UNSCOREABLE for the N rung:
#                                              Sigma=4976 against an interiority
#                                              threshold of 2048, so the ask is
#                                              2.43x the ceiling and no
#                                              multiplier verdict may be taken
#                                              (discipline 18(d)).
#   sc2  c2/c2 single  100 MB  100 Mbit  n=8   The N=1 identity + the crown-class
#                                              latency guard.
#
# Every arm runs at EVERY cell. There is no cell-restricted arm: a missing
# control at any cell would leave that cell unscoreable, and c8L's N-rung
# exclusion is a SCORING rule in the contract, not a missing invocation here.
#
# ── THE PRIMARY READOUTS ────────────────────────────────────────────────
#   [SUMCAP] on/eng/chg/chg_frac/pin/floor/cap/ask   the N rung (16.62)
#   [DIAG] per-path `pl=`                            the T rung's eps-hat axis
#   [ACKDIAG] recon[... ce/cr cr/sa]                 the T rung's own witness
#   [CCAP] brake=<closed>/<ticks>                    the brake rung
#   [WALL] onset/dur_ms                              the set/brake rung, PAIRED
#                                                    within rep index at c8
#   occcap_p50 -> CAPBIND                            every arm's realized cap
#
# ── THREE INSTRUMENT FACTS, from the contract, encoded here ─────────────
#  1. [SUMCAP] is emitted ONLY on the ON arm (SumCapGauge::drop) but FED on both
#     arms including the counterfactual. Its absence on A/T is CORRECT; its
#     presence on A/T is CONTAMINATION.
#  2. `[SUMCAP] eng=0/0` at c1/sc2 is EXPECTED — pooled_store_cap returns None
#     on n_live < 2 BEFORE the multiplier is read — and is NOT a warm-up
#     failure. `eng=0/N` at a DUAL with RWM_SUM_CAP=1 IS one, and voids the rep.
#  3. On FULL the [CCAP] line reads eng=0/0 cap=0.0 mem=0 floor=0 BY
#     CONSTRUCTION: it is emitted for either brake door (net/mod.rs:4744) while
#     its bind-fraction accumulator is guarded by composed_cap alone
#     (net/mod.rs:5524). The ONLY field carrying a datum on FULL is `brake=`.
#     The composed battery's `eng=0/N is a warm-up fail` rule is CORRECT there
#     and is NOT inherited here; inheriting it would flag every FULL rep.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in ladder_parse.py (no [GATES] on
# EITHER endpoint = ABORT: no datum, no liveness verdict, not in any
# denominator). The seed-7 topo-ping abort class is handled by SYMMETRIC top-up
# sessions only, never asymmetric ones (ladder_topup.sh, guard G-TOPUP).
#
# ARMCOUNT BELOW IS NOT AN n. It counts PARSED ROWS and an aborted invocation
# still emits a row. The scored n is ladder_report.py's LIVE n, recomputed from
# the gates columns. That recomputation is the only reason the mode-hunt
# battery's top-up trigger was ever visible.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
LD_CELLS="${RWM_LADDER_CELLS:-c1 c7 c8 c8L sc2}"
LD_ARMS="${RWM_LADDER_ARMS:-A N T NT FULL}"
TAG="${RWM_LADDER_TAG:-ladder}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/ladder/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/ladder/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# ── THE ECHO EXPECTATIONS TABLE — the contract's own, and the SINGLE source
#    of both the arm's env and the arm's liveness assertion. ────────────────
# The eight gates the contract names two-sided. RWM_COMPOSED_CAP and
# RWM_THREE_TERM are expected 0 on EVERY arm including FULL, and are set
# explicitly rather than left unset so the assertion is explicit rather than
# inherited (`config::env_flag` treats "0"/"false" as OFF for every boolean
# gate since 2026-07-13).
LD_GATES="RWM_SUM_CAP RWM_LOSS_SENT_TRUTH RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_COMPOSED_CAP RWM_THREE_TERM"

gate_expect() { # arm gate -> 0|1
  case "$2" in
    RWM_SUM_CAP)
      case "$1" in N|NT|FULL) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_LOSS_SENT_TRUTH|RWM_CHARGE_RECOVERY|RWM_RELEASE_1TO1)
      case "$1" in T|NT|FULL) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_STORE_CAP_UNIFIED|RWM_LATE_BRAKE)
      case "$1" in FULL) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_COMPOSED_CAP|RWM_THREE_TERM)
      echo 0 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the table above. Every one of the eight gates is
# passed with its EXPECTED value — the ON gates so the arm is armed, the OFF
# gates so the control can be shown to have been a control on both endpoints.
arm_env() { # arm -> "RWM_X=v RWM_Y=v ..."
  local a="$1" g out=""
  for g in $LD_GATES; do out="$out $g=$(gate_expect "$a" "$g")"; done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes"  (identical geometry to ccap_battery.sh /
# deadwall_battery.sh / flip_battery.sh -- the cells are TRANSCRIBED, never
# redefined here, which is the same rule capbind_check.py's CELL_PATHS follows)
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
# Live path count per cell, for instrument fact 2: at N=1 the pooled law
# short-circuits before the multiplier is read and `[SUMCAP] eng=0/0` is the
# CORRECT reading, not a warm-up failure.
cell_paths() { case "$1" in c7|c8|c8L) echo 2 ;; *) echo 1 ;; esac; }

# THE PER-CELL n. n=12 at both c8 cells is the mode-rate lesson (a low-base-rate
# per-rep statistic needs the reps where the decision is taken); n=8 elsewhere.
# Applied INSIDE the interleaved loop, never as a separate pass, so every rep
# sits in the same round-robin on the same topologies as the reps it is
# compared against.
cell_reps() { case "$1" in c8|c8L) echo "$REPS" ;; *) echo "${RWM_LADDER_SMALLREPS:-8}" ;; esac; }

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local npaths; npaths="$(cell_paths "$cell")"

  python3 ./ladder_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "LADDER-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── THE TWO-SIDED GATE ASSERTION, both endpoints, EVERY gate in the table.
  # Scoped to the [GATES] line: the ACTIVE echoes' own prose contains literal
  # `RWM_*=0` strings (the flip battery's amendment-1 lesson).
  local gl_c gl_s
  gl_c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1)
  gl_s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1)

  # No [GATES] on EITHER endpoint = ABORT: no datum, no liveness verdict, and
  # NOT in any denominator. Checked before any assertion so an aborted
  # invocation never produces a wall of liveness failures.
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  local g want got_c got_s echoline=""
  for g in $LD_GATES; do
    want="$(gate_expect "$arm" "$g")"
    got_c=$(printf '%s' "$gl_c" | grep -o "$g=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$g=[01]")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case "$g" in
      # The ARMS' own gates: a mismatch is an ARM-LIVENESS-FAIL.
      RWM_SUM_CAP|RWM_LOSS_SENT_TRUTH|RWM_CHARGE_RECOVERY|RWM_RELEASE_1TO1|RWM_STORE_CAP_UNIFIED|RWM_LATE_BRAKE)
        [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
        [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
        ;;
      # Expected 0 on EVERY arm including FULL: FULL carries the xN deletion
      # INSTEAD of the composed pool law. Presence is CONTAMINATION.
      RWM_COMPOSED_CAP|RWM_THREE_TERM)
        { [ "$got_c" != "$g=0" ] || [ "$got_s" != "$g=0" ]; } \
          && echo "ARM-CONTAMINATION $name rep=$REP gate=$g cli='$got_c' srv='$got_s'" >> "$OUT"
        ;;
    esac
  done

  # The instruments must be armed on BOTH endpoints or their columns are void.
  local i
  for i in RWM_DIAG RWM_ACKDIAG RWM_WALLDIAG; do
    got_c=$(printf '%s' "$gl_c" | grep -o "$i=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$i=[01]")
    echoline="$echoline $i=$got_c/$got_s(exp1)"
    { [ "$got_c" != "$i=1" ] || [ "$got_s" != "$i=1" ]; } \
      && echo "INSTRUMENT-FAIL-GATE $name rep=$REP gate=$i cli='$got_c' srv='$got_s'" >> "$OUT"
  done
  # WITNESS, recorded loudly when it is not what the whole battery assumes: a
  # change to the recovery plane's clocks is only safe with the RFC 9002 hole
  # law armed (the component bench's standing warning).
  got_c=$(printf '%s' "$gl_c" | grep -o "RWM_RECOV_MP=[01]")
  got_s=$(printf '%s' "$gl_s" | grep -o "RWM_RECOV_MP=[01]")
  { [ "$got_c" != "RWM_RECOV_MP=1" ] || [ "$got_s" != "RWM_RECOV_MP=1" ]; } \
    && echo "WITNESS-UNEXPECTED-RECOVMP $name rep=$REP cli='$got_c' srv='$got_s'" >> "$OUT"

  # ── THE PROSE ECHOES AND THE GAUGE LINES ─────────────────────────────────
  local sc_c sc_s uc us ttc tts ccn akc wln
  sc_c=$(grep -c "\[SUMCAP\]" /tmp/rwm-c.log 2>/dev/null || true)
  sc_s=$(grep -c "\[SUMCAP\]" /tmp/rwm-s.log 2>/dev/null || true)
  uc=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  us=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  ttc=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  tts=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  ccn=$(grep -c "\[CCAP\]" /tmp/rwm-c.log 2>/dev/null || true)
  akc=$(grep -c "\[ACKDIAG\]" /tmp/rwm-c.log 2>/dev/null || true)
  wln=$(grep -c "\[WALL\]" /tmp/rwm-c.log 2>/dev/null || true)

  echo "LIVENESS $name rep=$REP npaths=$npaths sumcap=$sc_c/$sc_s actU=$uc/$us act3T=$ttc/$tts ccap=$ccn ackdiag=$akc wall=$wln --$echoline" >> "$OUT"

  # The gauges' OWN lines, verbatim, one per echo — so the ledger carries the
  # readout even if the parser ever changes its mind about a column.
  (grep -h "\[SUMCAP\]" /tmp/rwm-c.log /tmp/rwm-s.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
    | sed "s/^.*\(\[SUMCAP\]\)/SUMCAPLINE $name rep=$REP \1/" >> "$OUT") || true
  (grep -h "\[CCAP\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
    | sed "s/^.*\(\[CCAP\]\)/CCAPLINE $name rep=$REP \1/" >> "$OUT") || true
  (grep -h "\[WALL\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
    | sed "s/^.*\(\[WALL\]\)/WALLLINE $name rep=$REP \1/" >> "$OUT") || true

  # [SUMCAP]: PRESENT on N/NT/FULL, ABSENT on A/T (emitted only on the ON arm).
  if [ "$(gate_expect "$arm" RWM_SUM_CAP)" = "1" ]; then
    { [ "$sc_c" -eq 0 ] && [ "$sc_s" -eq 0 ]; } \
      && echo "ARM-LIVENESS-FAIL-SUMCAP $name rep=$REP (RWM_SUM_CAP=1 and no [SUMCAP] on either endpoint)" >> "$OUT"
    # INSTRUMENT FACT 2. eng=0/N with the gate ON at a DUAL is a WARM-UP
    # FAILURE and the rep carries no datum. eng=0/0 at a SINGLE-path cell is
    # the CORRECT reading (pooled_store_cap short-circuits at n_live < 2) and
    # is recorded as the expected identity, never as a failure.
    if [ "$npaths" -ge 2 ]; then
      (grep -h "\[SUMCAP\] on=1 eng=0/" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        && echo "SUMCAP-WARMUP-FAIL $name rep=$REP (eng=0/N at a DUAL with RWM_SUM_CAP=1 — no datum)" >> "$OUT"
    else
      (grep -h "\[SUMCAP\] on=1 eng=0/0" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        || echo "SUMCAP-N1-UNEXPECTED $name rep=$REP (single-path cell did NOT read eng=0/0 — the N=1 short-circuit did not hold)" >> "$OUT"
    fi
  else
    { [ "$sc_c" -gt 0 ] || [ "$sc_s" -gt 0 ]; } \
      && echo "ARM-CONTAMINATION-SUMCAP $name rep=$REP (cli=$sc_c srv=$sc_s with RWM_SUM_CAP=0)" >> "$OUT"
  fi

  # The unified live set: PRESENT on FULL, ABSENT everywhere else.
  if [ "$(gate_expect "$arm" RWM_STORE_CAP_UNIFIED)" = "1" ]; then
    { [ "$uc" -eq 0 ] && [ "$us" -eq 0 ]; } \
      && echo "ARM-LIVENESS-FAIL-U $name rep=$REP (VOID: neither site echoed the unified path set)" >> "$OUT"
  else
    { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } \
      && echo "ARM-CONTAMINATION-U $name rep=$REP (cli=$uc srv=$us)" >> "$OUT"
  fi

  # [CCAP]: PRESENT on FULL (the LATE_BRAKE door), ABSENT elsewhere. INSTRUMENT
  # FACT 3 — on FULL the ONLY field carrying a datum is `brake=`; eng=0/0 here
  # is BY CONSTRUCTION and is deliberately NOT flagged.
  if [ "$(gate_expect "$arm" RWM_LATE_BRAKE)" = "1" ]; then
    [ "$ccn" -eq 0 ] && echo "ARM-LIVENESS-FAIL-CCAP $name rep=$REP (RWM_LATE_BRAKE=1 and no [CCAP] line)" >> "$OUT"
    # B-ARMED: brake=0/0 is a NULL EFFECT (never armed), brake=0/N a null
    # RESULT (armed, never closed). Only the former is a defect.
    (grep -h "\[CCAP\].* brake=0/0 " /tmp/rwm-c.log >/dev/null 2>&1) \
      && echo "BRAKE-NEVER-ARMED $name rep=$REP (brake=0/0 — a null EFFECT: no claim about the brake)" >> "$OUT"
  else
    [ "$ccn" -gt 0 ] && echo "ARM-CONTAMINATION-CCAP $name rep=$REP" >> "$OUT"
  fi

  # Expected ABSENT on EVERY arm: no arm here reaches the three-term pool seat.
  { [ "$ttc" -gt 0 ] || [ "$tts" -gt 0 ]; } \
    && echo "ARM-CONTAMINATION-3T $name rep=$REP (cli=$ttc srv=$tts — no ladder arm reaches the three-term pool seat)" >> "$OUT"

  # The dead-wall instrument, ON IN EVERY ARM: without it B-WALL's paired
  # contrast has no datum on this rep, on either side of the pair.
  [ "$wln" -eq 0 ] && echo "INSTRUMENT-FAIL-WALL $name rep=$REP (no [WALL] line on the client)" >> "$OUT"
  [ "$akc" -eq 0 ] && echo "INSTRUMENT-FAIL-ACKDIAG $name rep=$REP (no [ACKDIAG] line on the client)" >> "$OUT"
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  # The T rung's primary instrument. `pl=` rides the [DIAG] per-path block; its
  # absence voids the eps-hat axis on this rep and nothing else.
  local pln; pln=$(grep -c "pl=" /tmp/rwm-c.log 2>/dev/null || true)
  [ "$pln" -eq 0 ] && echo "INSTRUMENT-FAIL-PL $name rep=$REP (no per-path pl= — the eps-hat axis has no datum)" >> "$OUT"

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

  # G-SC2-LAT's probe is load-bearing at sc2 and is captured everywhere.
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  # discipline 16b: the shaped device's own counters, on EVERY cell and EVERY
  # invocation. The headroom denominator is the TRANSFER wall (`seconds`),
  # never INVOCATION_S — see the contract's headroom protocol.
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $LD_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $LD_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  [ "$REP" -le "$(cell_reps "$1")" ] || return 0
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== LADDER BATTERY seed=$SEED_ARG reps=$REPS smallreps=${RWM_LADDER_SMALLREPS:-8} cells='$LD_CELLS' arms='$LD_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"Ladder Battery — PRE-REGISTRATION\" (commit 91c00dd), era main@5ddf7f6, ONE binary all arms"
  echo "=== ARMS A = shipped | N = RWM_SUM_CAP | T = LOSS_SENT_TRUTH+CHARGE_RECOVERY+RELEASE_1TO1 | NT = N+T | FULL = NT+STORE_CAP_UNIFIED+LATE_BRAKE (COMPOSED_CAP=0 THREE_TERM=0)"
  echo "=== READOUTS [SUMCAP] (N rung) | per-path pl= + [ACKDIAG] recon (T rung) | [CCAP] brake= (brake rung) | [WALL] onset/dur_ms PAIRED WITHIN REP at c8 | occcap_p50 -> CAPBIND"
  echo "=== c8L IS PRE-DECLARED UNSCOREABLE FOR THE N RUNG (ceiling-governed: Sigma 4976 vs the 2048 interiority threshold)"
  for A in $LD_ARMS; do echo "=== ARMENV $A: $(arm_env "$A")"; done
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm (desktop session, recorded per era honesty)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPS"); do
  for CELL in $LD_CELLS; do
    for ARM in $LD_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
# ARMCOUNT IS NOT AN n — it counts PARSED ROWS and aborts emit rows. The scored
# n is ladder_report.py's LIVE n, recomputed from the gates columns.
echo "=== ARMCOUNTS (rows, NOT live n — see ladder_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $LD_CELLS; do
  for A in $LD_ARMS; do
    WANT=$(cell_reps "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "LADDER-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo LADDER-BATTERY-DONE-$SEED_ARG
