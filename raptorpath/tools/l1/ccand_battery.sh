#!/bin/bash
# THE CANDIDATES BATTERY — the VM battery for goal-gate "Candidates Battery —
# PRE-REGISTRATION" (own commit, written before this file existed and before any
# VM contact). That block is the CONTRACT: it is scored against, never modified,
# and no number in it may change now that the VM has been touched.
#
#   sudo bash ccand_battery.sh <seed> [reps]
#
# LITERATURE-BACKED SUCCESSORS item 4. It scores the two gates 16.67 and 16.68
# shipped DEFAULT OFF, on ONE binary from main@0055c5d.
#
# ── ARMS ────────────────────────────────────────────────────────────────
#   A     (shipped default)          THE CONTROL, re-measured same-session. NOTE
#                                    RWM_SUM_CAP IS DEFAULT ON since 2026-08-19,
#                                    so A here is the LADDER'S ARM N. It also
#                                    carries 16.68.1's fa= meter, which is the
#                                    point of running it.
#   D     RWM_DELTA_CAP=1            THE delta-CAP (16.67) — clamp((1+q)*Sigma,
#                                    floor, N*knee). Exactly ONE factor changes;
#                                    the Sigma, its path set, the estimator, the
#                                    COUNT multiplier, the ceiling and the floor
#                                    are IDENTICAL on both arms and cancel out.
#   R     RWM_RACK_CLOCKS=1          THE RACK CLOCK at its cited maximum.
#         RWM_RACK_REO_MULT=17       mult=17 is RFC 8985 6.2 Step 4's own upper
#                                    bound and the only value at which the SRTT
#                                    ceiling is reachable at any of our cells.
#   DR    D + R                      The composition. The two gates sit in
#                                    disjoint seats (pool_value_multiplier vs the
#                                    two round sites) and share no operand, so DR
#                                    is the FACTORISATION test, not a third law.
#
# ── THE TWO AUXILIARY ARMS, and what they may conclude ──────────────────
# Each is scored on its OWN echo line and on NOTHING else — not on goodput, not
# on latency, not in any guard denominator, not against A. The contract says so
# in THE ARMS, before the run.
#   R1    RWM_RACK_CLOCKS=1          THE CONFIRMATION ARM for 16.68's own defect
#         RWM_RACK_REO_MULT=1        finding: at RACK's own initial mult=1 the
#                                    SRTT ceiling provably CANNOT bind, and this
#                                    arm exists to READ ceil=0.0000 off the wire
#                                    instead of asserting it. A goodput
#                                    regression here is the PREDICTED
#                                    CONFIRMATION of the bench's 8-46 spurious
#                                    rounds, not a G-REG breach.
#   L     RWM_LOSS_SENT_TRUTH=1      THE ONLY ARM ON WHICH [LCW] CAN RECORD
#                                    ANYTHING — see THE SPECIFICATION FINDING in
#                                    the contract. The witness is fed inside
#                                    PathState::sender_truth_loss_delta, whose
#                                    only production callers sit behind
#                                    loss_sent_truth_active(), so [LCW] is
#                                    STRUCTURALLY SILENT on every other arm. This
#                                    arm takes NO verdict on the gate and
#                                    re-opens nothing the Ladder Battery closed.
#
# THE ENV IS DERIVED FROM THE ECHO EXPECTATIONS TABLE (`gate_expect` below), not
# written twice. An arm cannot be launched with an env its own liveness gate does
# not expect, which is the drift `ccap_battery.sh` avoided by hand and this one
# avoids by construction.
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c1   c1/c1 single  400 MB  1 Gbit    n=8   the N=1 IDENTITY check for D +
#                                              the one cell with real headroom +
#                                              the RACK receiver-ceiling cell.
#   c7   c2/c2 dual    200 MB  200 Mbit  n=8   THE CLEAN delta-cap rung.
#   c8   c2/c3 dual     25 MB  120 Mbit  n=12  THE LOAD-BEARING rung + the
#                                              dead-wall cell (B-WALL, PAIRED).
#   c8L  c2/c3 dual    200 MB  120 Mbit  n=12  The length axis and THE
#                                              ANCHOR-DEPENDENT cell: interior on
#                                              the primary era, PINNED on the
#                                              secondary. Both pre-declared.
#   sc2  c2/c2 single  100 MB  100 Mbit  n=8   The crown-class latency guard and
#                                              the 50% fa= cell. NOTE: D is
#                                              BIT-IDENTICAL here and the tree's
#                                              cleanest CoDel datum is therefore
#                                              UNREACHABLE by this law.
#
# The four SCORED arms run at EVERY cell. R1 runs at c8 + sc2 and L at sc2 + c7 +
# c8, both at n=2/seed, by the contract's own restriction — not a missing
# invocation.
#
# ── THE PRIMARY READOUTS ────────────────────────────────────────────────
#   [DCAP] on/eng/chg/chg_frac/pin/floor/cap/ask/q/b   rung D (16.67)
#          q= and b= ARE THE DIAL-ROUTING CHECK (discipline 1): q=0.100000
#          b=2.0000 at the bulk hint, or NOTHING in this battery is scored.
#   [RACK] ceil/gran/legacy_pin/round/legacy/mult      rung R (16.68)
#          fa=/fa_frac= vs fa_class=0.0625             16.68.1, ON EVERY ARM
#   [LCW]  over_n/over_mass/loss_mass/rect_frac        arm L only, NO BAR
#   [WALL] onset/dur_ms                                PAIRED WITHIN REP at c8
#   [SUMCAP] ...                                       present on EVERY arm now
#          (RWM_SUM_CAP is DEFAULT ON) — its absence is an INSTRUMENT-FAIL.
#   occcap_p50 -> CAPBIND                              every arm's realized cap
#
# ── FIVE INSTRUMENT FACTS, from the contract, encoded here ──────────────
#  1. [DCAP] is emitted ONLY on the ON arm (DeltaCapGauge::drop) but FED on both
#     arms including the counterfactual. Its absence on A/R/R1/L is CORRECT; its
#     presence there is CONTAMINATION.
#  2. `[DCAP] eng=0/0` at c1/sc2 is EXPECTED — the pooled seat returns None on
#     n_live < 2 BEFORE any multiplier is read, so D is BIT-IDENTICAL to A at
#     every single-path cell BY CONSTRUCTION — and is NOT a warm-up failure.
#     `eng=0/N` at a DUAL with RWM_DELTA_CAP=1 IS one, and voids the rep.
#  3. On arm A the [RACK] line reads `on=0 evals=0 ceil=0.0000 gran=0.0000
#     legacy_pin=0.0000 round=0.0 legacy=0.0` BY CONSTRUCTION and the ONLY field
#     carrying a datum is fa=. RackClockGauge::record is guarded by
#     pol.rack_clocks (net/mod.rs:7163) while record_fire is fed unconditionally
#     (net/mod.rs:7932) and Drop emits on `self.on || self.fired > 0`
#     (net/mod.rs:4225). A's ceil=0.0000 is A DENOMINATOR OF ZERO, NOT 16.68's
#     defect finding. R1's ceil=0.0000 at evals >> 0 IS the defect finding.
#  4. legacy= / legacy_pin= are fed on the ON arm ONLY — they are the
#     counterfactual against the shipped [25,100] ms clamp, computed inside the
#     armed law. The FIRST EVER measurement of the shipped clamp's own bind
#     fraction is therefore read off R / DR / R1, NEVER off A.
#  5. The RECEIVER's [RACK] gauge never calls record_fire (net/receiver.rs:209,
#     771-780 record evaluations only), so a server-side [RACK] line always reads
#     fa=0/0 and on arm A the receiver emits NO line at all. fa= is a SENDER-SITE
#     statistic here and the reporter takes it from the CLIENT log alone.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in ccand_parse.py (no [GATES] on
# EITHER endpoint = ABORT: no datum, no liveness verdict, not in any
# denominator). The seed-7 topo-ping abort class is handled by SYMMETRIC top-up
# sessions only, never asymmetric ones (ccand_topup.sh, guard G-TOPUP).
#
# ARMCOUNT BELOW IS NOT AN n. It counts PARSED ROWS and an aborted invocation
# still emits a row. The scored n is ccand_report.py's LIVE n, recomputed from
# the gates columns.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
CC_CELLS="${RWM_CCAND_CELLS:-c1 c7 c8 c8L sc2}"
CC_ARMS="${RWM_CCAND_ARMS:-A D R DR R1 L}"
TAG="${RWM_CCAND_TAG:-ccand}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/ccand/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/ccand/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# ── THE ECHO EXPECTATIONS TABLE — the contract's own, and the SINGLE source
#    of both the arm's env and the arm's liveness assertion. ────────────────
# Every gate is passed explicitly rather than left unset, so the control can be
# SHOWN to have been a control on both endpoints (`config::env_flag` treats
# "0"/"false" as OFF for every boolean gate since 2026-07-13).
#
# RWM_RACK_REO_MULT IS AN INTEGER, NOT A FLAG. It is carried in the same table
# because it is part of the arm's identity, and it is matched as `=[0-9]+` rather
# than `=[01]` everywhere below.
CC_ARM_GATES="RWM_DELTA_CAP RWM_RACK_CLOCKS RWM_RACK_REO_MULT RWM_LOSS_SENT_TRUTH RWM_SUM_CAP"
CC_CONTAM_GATES="RWM_QUANTILE_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1"
CC_GATES="$CC_ARM_GATES $CC_CONTAM_GATES"

gate_expect() { # arm gate -> value
  case "$2" in
    RWM_DELTA_CAP)
      case "$1" in D|DR) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_RACK_CLOCKS)
      case "$1" in R|DR|R1) echo 1 ;; *) echo 0 ;; esac ;;
    # RACK's own initial reo_wnd_mult is 1; the scored RACK arms drive it to
    # RACK's own maximum 17, which is the ONLY value at which the SRTT ceiling is
    # reachable at any of our cells. Arms that do not arm the clock still pass
    # the gate's DEFAULT so the echo assertion is explicit, not inherited.
    RWM_RACK_REO_MULT)
      case "$1" in R|DR) echo 17 ;; *) echo 1 ;; esac ;;
    RWM_LOSS_SENT_TRUTH)
      case "$1" in L) echo 1 ;; *) echo 0 ;; esac ;;
    # DEFAULT ON since 2026-08-19 (ladder rung N). Asserted rather than assumed:
    # a reader who takes the pre-ladder default mis-scales every cap prediction
    # in the contract by 2x.
    RWM_SUM_CAP)
      echo 1 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the table above.
arm_env() { # arm -> "RWM_X=v RWM_Y=v ..."
  local a="$1" g out=""
  for g in $CC_GATES; do out="$out $g=$(gate_expect "$a" "$g")"; done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes"  (identical geometry to ladder_battery.sh /
# ccap_battery.sh / deadwall_battery.sh -- the cells are TRANSCRIBED, never
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
# short-circuits before any multiplier is read and `[DCAP] eng=0/0` is the
# CORRECT reading, not a warm-up failure.
cell_paths() { case "$1" in c7|c8|c8L) echo 2 ;; *) echo 1 ;; esac; }

# THE PER-(ARM, CELL) n, applied INSIDE the interleaved loop and never as a
# separate pass, so every rep sits in the same round-robin on the same topologies
# as the reps it is compared against. n=12 at both c8 cells is the mode-rate
# lesson; n=8 elsewhere. The AUXILIARY arms are the contract's own restriction:
# R1 at c8 + sc2 and L at sc2 + c7 + c8, n=2/seed, scored on their own echo alone.
AUXREPS="${RWM_CCAND_AUXREPS:-2}"
arm_cell_reps() { # arm cell -> reps (0 = this arm does not run at this cell)
  case "$1" in
    R1) case "$2" in c8|sc2) echo "$AUXREPS" ;; *) echo 0 ;; esac ;;
    L)  case "$2" in sc2|c7|c8) echo "$AUXREPS" ;; *) echo 0 ;; esac ;;
    *)  case "$2" in c8|c8L) echo "$REPS" ;; *) echo "${RWM_CCAND_SMALLREPS:-8}" ;; esac ;;
  esac
}

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local npaths; npaths="$(cell_paths "$cell")"

  python3 ./ccand_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "CCAND-PARSE-FAIL $name rep=$REP" >> "$OUT"

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

  local g want got_c got_s pat echoline=""
  for g in $CC_GATES; do
    want="$(gate_expect "$arm" "$g")"
    # The mult is an INTEGER; every other gate is a flag. One table, two matchers.
    case "$g" in RWM_RACK_REO_MULT) pat="$g=[0-9][0-9]*" ;; *) pat="$g=[01]" ;; esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $CC_ARM_GATES " in
      # The ARMS' own gates: a mismatch is an ARM-LIVENESS-FAIL.
      *" $g "*)
        [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
        [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
        ;;
      # Expected 0 on EVERY arm. RWM_QUANTILE_CLOCKS OUTRANKS rack_clocks and
      # RWM_DERIVED_SWEEP is a RIVAL law for the same quantity that rack_clocks
      # REPLACES — either one present would silently substitute the law under
      # test, so both are CONTAMINATION and not merely unexpected.
      *)
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
  # law armed (the component bench's standing warning, and this battery CHANGES
  # those clocks on three of its six arms).
  got_c=$(printf '%s' "$gl_c" | grep -o "RWM_RECOV_MP=[01]")
  got_s=$(printf '%s' "$gl_s" | grep -o "RWM_RECOV_MP=[01]")
  { [ "$got_c" != "RWM_RECOV_MP=1" ] || [ "$got_s" != "RWM_RECOV_MP=1" ]; } \
    && echo "WITNESS-UNEXPECTED-RECOVMP $name rep=$REP cli='$got_c' srv='$got_s'" >> "$OUT"

  # ── THE PROSE ECHOES AND THE GAUGE LINES ─────────────────────────────────
  local dc_c dc_s rk_c rk_s lw_c sc_c sc_s uc us ttc tts akc wln
  dc_c=$(grep -c "\[DCAP\]" /tmp/rwm-c.log 2>/dev/null || true)
  dc_s=$(grep -c "\[DCAP\]" /tmp/rwm-s.log 2>/dev/null || true)
  rk_c=$(grep -c "\[RACK\]" /tmp/rwm-c.log 2>/dev/null || true)
  rk_s=$(grep -c "\[RACK\]" /tmp/rwm-s.log 2>/dev/null || true)
  # Counted across BOTH endpoints in one pass — `grep -hc` over two files prints
  # one count PER FILE, which would need summing; `grep -h | wc -l` does not.
  lw_c=$(grep -h "\[LCW\]" /tmp/rwm-c.log /tmp/rwm-s.log 2>/dev/null | wc -l | tr -d ' ')
  [ -z "$lw_c" ] && lw_c=0
  sc_c=$(grep -c "\[SUMCAP\]" /tmp/rwm-c.log 2>/dev/null || true)
  sc_s=$(grep -c "\[SUMCAP\]" /tmp/rwm-s.log 2>/dev/null || true)
  uc=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  us=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  ttc=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  tts=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  akc=$(grep -c "\[ACKDIAG\]" /tmp/rwm-c.log 2>/dev/null || true)
  wln=$(grep -c "\[WALL\]" /tmp/rwm-c.log 2>/dev/null || true)

  echo "LIVENESS $name rep=$REP npaths=$npaths dcap=$dc_c/$dc_s rack=$rk_c/$rk_s lcw=$lw_c sumcap=$sc_c/$sc_s actU=$uc/$us act3T=$ttc/$tts ackdiag=$akc wall=$wln --$echoline" >> "$OUT"

  # The gauges' OWN lines, verbatim, one per echo — so the ledger carries the
  # readout even if the parser ever changes its mind about a column. The SITE is
  # kept in the tag (instrument fact 5: fa= is a SENDER-site statistic and the
  # server's [RACK] always reads fa=0/0).
  local f
  for f in DCAP RACK SUMCAP WALL LCW; do
    (grep -h "\[$f\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" /tmp/rwm-s.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── [DCAP]: PRESENT on D/DR, ABSENT on A/R/R1/L (emitted only on the ON arm).
  if [ "$(gate_expect "$arm" RWM_DELTA_CAP)" = "1" ]; then
    { [ "$dc_c" -eq 0 ] && [ "$dc_s" -eq 0 ]; } \
      && echo "ARM-LIVENESS-FAIL-DCAP $name rep=$REP (RWM_DELTA_CAP=1 and no [DCAP] on either endpoint)" >> "$OUT"
    # INSTRUMENT FACT 2. eng=0/N with the gate ON at a DUAL is a WARM-UP FAILURE
    # and the rep carries no datum. eng=0/0 at a SINGLE-path cell is the CORRECT
    # reading (the pooled seat short-circuits at n_live < 2) and is recorded as
    # the expected identity, never as a failure.
    if [ "$npaths" -ge 2 ]; then
      (grep -h "\[DCAP\] on=1 eng=0/" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        && echo "DCAP-WARMUP-FAIL $name rep=$REP (eng=0/N at a DUAL with RWM_DELTA_CAP=1 — no datum)" >> "$OUT"
      # THE DIAL-ROUTING CHECK (MEASUREMENT DISCIPLINE 1), and it outranks every
      # number in the battery. The harness runs the `bulk` hint, so b(Bulk)=2 and
      # q=(b+1)/30=0.100000 EXACTLY. Anything else means the env var was read but
      # the dial did not reach the law.
      (grep -h "\[DCAP\] on=1 .* q=0.100000 b=2.0000" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        || echo "DIAL-ROUTE-FAIL $name rep=$REP (no [DCAP] with q=0.100000 b=2.0000 — the dial did not route; discipline 1)" >> "$OUT"
      # chg_frac=0 cannot happen while gain != 1+q: it is an INSTRUMENT FAILURE
      # here, not the null RESULT it would be on [SUMCAP] (net/mod.rs:3938-3944).
      (grep -h "\[DCAP\] on=1 .* chg_frac=0.0000" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        && echo "DCAP-INERT-IMPOSSIBLE $name rep=$REP (chg_frac=0.0000 with gain != 1+q — INSTRUMENT failure, not a result)" >> "$OUT"
    else
      (grep -h "\[DCAP\] on=1 eng=0/0" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        || echo "DCAP-N1-UNEXPECTED $name rep=$REP (single-path cell did NOT read eng=0/0 — the N=1 short-circuit did not hold)" >> "$OUT"
    fi
  else
    { [ "$dc_c" -gt 0 ] || [ "$dc_s" -gt 0 ]; } \
      && echo "ARM-CONTAMINATION-DCAP $name rep=$REP (cli=$dc_c srv=$dc_s with RWM_DELTA_CAP=0)" >> "$OUT"
  fi

  # ── [RACK]: RIDES EVERY ARM. On A/D/L it is 16.68.1's fa= meter and nothing
  # else (INSTRUMENT FACT 3: evals=0, so ceil=/gran=/legacy_pin= are DENOMINATORS
  # OF ZERO and must never be read as the defect finding). A run that fired no
  # recovery round stays silent by construction, and that is an fa INSTRUMENT-FAIL
  # for the rep, never `fa_frac = 0`.
  [ "$rk_c" -eq 0 ] && echo "INSTRUMENT-FAIL-RACK $name rep=$REP (no [RACK] on the client — no recovery round fired, so this rep carries NO fa datum; it is NOT fa_frac=0)" >> "$OUT"
  if [ "$(gate_expect "$arm" RWM_RACK_CLOCKS)" = "1" ]; then
    (grep -h "\[RACK\] on=1 evals=0 " /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
      && echo "RACK-WARMUP-FAIL $name rep=$REP (evals=0 with RWM_RACK_CLOCKS=1 — the clock law never evaluated, no bind-fraction datum)" >> "$OUT"
  else
    (grep -h "\[RACK\] on=1" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
      && echo "ARM-CONTAMINATION-RACK $name rep=$REP (on=1 with RWM_RACK_CLOCKS=0)" >> "$OUT"
  fi

  # ── [LCW]: THE SPECIFICATION FINDING, encoded. The witness is fed inside
  # PathState::sender_truth_loss_delta, whose only production callers sit behind
  # loss_sent_truth_active(), so it can only record on arm L. Its ABSENCE on
  # every other arm is CORRECT and is asserted here so a reader can never take
  # five columns of structural silence for a null RESULT. Its PRESENCE elsewhere
  # is an INSTRUMENT SURPRISE — recorded loudly, scored on nothing.
  if [ "$(gate_expect "$arm" RWM_LOSS_SENT_TRUTH)" = "1" ]; then
    [ "$lw_c" -eq 0 ] \
      && echo "ARM-LIVENESS-FAIL-LCW $name rep=$REP (RWM_LOSS_SENT_TRUTH=1 and no [LCW] — the witness recorded nothing)" >> "$OUT"
  else
    [ "$lw_c" -gt 0 ] \
      && echo "INSTRUMENT-SURPRISE-LCW $name rep=$REP (n=$lw_c [LCW] lines with RWM_LOSS_SENT_TRUTH=0 — the contract's specification finding is WRONG and the witness is reachable; RECORD, do not score)" >> "$OUT"
  fi

  # [SUMCAP] rides EVERY arm now: RWM_SUM_CAP is DEFAULT ON. Its absence is an
  # INSTRUMENT-FAIL, not an arm property — which is the one liveness rule that
  # INVERTS from the Ladder Battery, and it inverts because the default moved.
  { [ "$sc_c" -eq 0 ] && [ "$sc_s" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-SUMCAP $name rep=$REP (RWM_SUM_CAP is DEFAULT ON and no [SUMCAP] on either endpoint)" >> "$OUT"

  # Expected ABSENT on EVERY arm: no arm here reaches the three-term or unified
  # seats — 16.67's OTHER two axes are held fixed so the VALUE axis is alone.
  { [ "$ttc" -gt 0 ] || [ "$tts" -gt 0 ]; } \
    && echo "ARM-CONTAMINATION-3T $name rep=$REP (cli=$ttc srv=$tts)" >> "$OUT"
  { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } \
    && echo "ARM-CONTAMINATION-U $name rep=$REP (cli=$uc srv=$us)" >> "$OUT"

  # The dead-wall instrument, ON IN EVERY ARM: without it B-WALL's paired
  # contrast has no datum on this rep, on either side of the pair.
  [ "$wln" -eq 0 ] && echo "INSTRUMENT-FAIL-WALL $name rep=$REP (no [WALL] line on the client)" >> "$OUT"
  [ "$akc" -eq 0 ] && echo "INSTRUMENT-FAIL-ACKDIAG $name rep=$REP (no [ACKDIAG] line on the client)" >> "$OUT"
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  local pln; pln=$(grep -c "pl=" /tmp/rwm-c.log 2>/dev/null || true)
  [ "$pln" -eq 0 ] && echo "INSTRUMENT-FAIL-PL $name rep=$REP (no per-path pl=)" >> "$OUT"

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
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS arm's
  # log and pass its liveness gate.
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

  # G-SC2-LAT's probe is load-bearing at sc2 and is captured everywhere. D-LAT
  # reads it at the duals, so it is load-bearing there too.
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  # discipline 16b: the shaped device's own counters, on EVERY cell and EVERY
  # invocation. The headroom denominator is the TRANSFER wall (`seconds`), never
  # INVOCATION_S — see the contract's headroom protocol.
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $CC_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $CC_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== CANDIDATES BATTERY seed=$SEED_ARG reps=$REPS smallreps=${RWM_CCAND_SMALLREPS:-8} auxreps=$AUXREPS cells='$CC_CELLS' arms='$CC_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"Candidates Battery — PRE-REGISTRATION\" (commit 6bd5299), era main@0055c5d, ONE binary all arms"
  echo "=== ARMS A = shipped default (NOTE RWM_SUM_CAP IS DEFAULT ON: A is the LADDER'S ARM N) | D = RWM_DELTA_CAP | R = RACK mult=17 | DR = D+R"
  echo "=== AUX  R1 = RACK mult=1 (16.68's ceil=0.0000 defect finding, CONFIRMATION arm) | L = RWM_LOSS_SENT_TRUTH (the ONLY arm [LCW] can record on)"
  echo "=== AUX ARMS ARE SCORED ON THEIR OWN ECHO LINE AND ON NOTHING ELSE — excluded from G-REG and from every contrast, by the contract, before the run"
  echo "=== READOUTS [DCAP] (rung D; q=/b= ARE THE DIAL-ROUTE CHECK, discipline 1) | [RACK] ceil/gran/legacy_pin (rung R) + fa= (16.68.1, EVERY ARM) | [LCW] (arm L, NO BAR) | [WALL] PAIRED WITHIN REP at c8 | occcap_p50 -> CAPBIND"
  echo "=== c8L IS THE ANCHOR-DEPENDENT CELL: interior on the PRIMARY era (1.10*2815.4=3097 < 4096), PINNED on the SECONDARY (1.10*4976.1=5474 > 4096). Read it from [DCAP] pin= IN THE RUN. Neither outcome refutes 16.67."
  echo "=== D IS BIT-IDENTICAL AT c1/sc2 BY CONSTRUCTION. The tree's cleanest CoDel datum (sc2, Tier-1 2a) is UNREACHABLE by this law and no sc2 number here supports it."
  for A in $CC_ARMS; do echo "=== ARMENV $A: $(arm_env "$A")"; done
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
# ARMCOUNT IS NOT AN n — it counts PARSED ROWS and aborts emit rows. The scored n
# is ccand_report.py's LIVE n, recomputed from the gates columns.
echo "=== ARMCOUNTS (rows, NOT live n — see ccand_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $CC_CELLS; do
  for A in $CC_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "CCAND-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo CCAND-BATTERY-DONE-$SEED_ARG
