#!/bin/bash
# THE HOLD-DOWN SWEEP — DOES THE SENDER'S WAITING TIME ON A REPORTED HOLE MOVE
# THE FALSE-REPAIR RATE THE TIMER NEVER COULD?
#
#   sudo bash hold_battery.sh <seed> [reps]
#
# THE QUESTION. The fire-cause pass classified 107,597 recovery fires: 0.59 %
# timer-driven, 98.99 % the sender answering a receiver gap report. The
# quantile-native sweep then moved the realized TIMER clock 2.1x across a 200x
# span of alpha, witnessed at 480/480, and the realized false-alarm rate did not
# follow at five of five cells -- `fa PERP W`. Every clock this tree has swept
# sets the timer. THIS BATTERY SWEEPS THE OTHER ONE.
#
# `RWM_HOLDDOWN_Q` makes the sender wait T(q) = W_q(1-q) -- section 16.76's
# order statistic evaluated on the HOLE-RESOLUTION stream -- before it answers a
# reported hole. Paper section 16.77 is the derivation; "THE HOLD-DOWN SWEEP --
# PRE-REGISTRATION" in goal-gate.md is the contract this script implements and
# is the ONLY place the scoring bars live. This script emits counts; it applies
# no bar and names no winner.
#
# THE ARMS ARE DERIVED, NOT CHOSEN (pre-registration section 3). The top is the
# derivation's own answer (q = 0.990); the floor is the window law's (N is flat
# at 2K = 20 for every q <= 0.5, so no level below 1 - 10/21 = 0.5238 is
# expressible); the spacing is section 16.76.8's adjacent-ratio rule (four arms
# across a = 1-q in [0.010, 0.500] at the uniform geometric ratio 50^(1/3) =
# 3.684, above the 3.4-3.6 separation threshold at K = 10).
#
#   CTL   absent       T = 0, the shipped machine, byte-identically
#   H500  q = 0.5      N =   20   realized level 0.5238  (the derived floor)
#   H136  q = 0.864    N =   74   realized level 0.8667
#   H037  q = 0.963    N =  271   realized level 0.9632
#   H010  q = 0.99     N = 1000   realized level 0.9900  (THE DERIVED LEVEL)
#
# RWM_GEN=0 ON EVERY ROW, AND IT IS LOAD-BEARING. Under generation coding the
# SACK->gap producer is suppressed (recv_nack_tx = None), so [FCAUSE]'s gap_
# classes and [SUCC]'s orig are STRUCTURALLY EMPTY -- this battery's entire
# measurand would read zero for a configuration reason. [HOLD] echoes gen= on
# every line and W3 asserts it rather than trusting the arm env.
#
# THE CLOCKS ARE ALL DISARMED. RWM_QUANTILE_CLOCKS, RWM_RACK_CLOCKS and
# RWM_DERIVED_SWEEP are contamination gates here and RWM_W_FORM must resolve to
# `cantelli`: the shipped [25,100] ms clamp is the timer on every row, on every
# arm, so the ONLY axis is the hold-down. A row whose timer moved is not a row
# of this battery.
#
# WITNESSES, per invocation (the pre-registration section 7 numbering):
#
#   W1  [GATES] RWM_HOLDDOWN_Q=<the arm's own value> at CLI *and* SRV.
#   W2  contamination: every clock gate 0, RWM_W_FORM=cantelli,
#       RWM_ALPHA_OVERRIDE=unset, and RWM_HOLDDOWN_Q=unset on CTL.
#   W3  [HOLD] site=sender present, with n_req= equal to the arm's own N
#       (n_req=- on CTL) and gen=0.
#   W4  THE ROUTING WITNESS. evals == sup + emit on every [HOLD] line, AND
#       sum(evals) == sum(sup) + [FCAUSE] n. `should_hold` is consulted exactly
#       once per fire that reaches record_fire_cause, and the fire is then
#       either HELD or CLASSIFIED. This is the one assertion a gauge agreeing
#       only with itself could not pass.
#   W5  THE SECOND ROUTING WITNESS. [SUCC] det > 0 and res > 0 at the RECEIVER
#       -- the measurand the derivation rests on is live on THIS binary.
#   W6  [RFA] fires > 0 at the RECEIVER -- the realized-false-repair read is
#       live.
#   W7  [FCAUSE] other=0 and unattr=0 -- the classification is exhaustive on
#       this binary, as it was on the fire-cause pass's.
#   W8  rc = 0, no hard abort, no DNF.
#   W9  goodput inside the cell's band. CTL ONLY -- see BANDSCOPE.
#
# LAW-DEAD IS A RESULT, NOT AN ABORT. `law_n = 0` on a treatment arm means the
# window never filled and the arm ran the shipped behaviour at every fire; the
# row is VOID for scoring and is REPORTED as such. c1-H010 is PREDICTED here
# before the run (749 resolutions per rep against N = 1000) and the prediction
# is in the pre-registration, not in a post-hoc explanation.
#
# BANDSCOPE: the goodput abort bands apply to CTL ONLY. On a treatment arm an
# out-of-band reading is a RESULT -- section 16.77.9's H4 -- and `band_applies`
# says so in the row rather than in a footnote.
#
# CELLS AND BANDS ARE THE SUCCESSOR-ARRIVAL PASS'S, TRANSCRIBED AND NEVER
# REDEFINED: a cell that differs from the ledger's cell is a different cell and
# its rows do not pool with the pass this one continues.
#
# THE PARSER IS alpha_parse.py, NOT A FORK OF IT. The [HOLD], [FCAUSE] and
# [SUCC] columns were added to that file ADDITIVELY -- no field removed, no
# field renamed, the `ALPHARESULT ` prefix unchanged -- so this battery's rows
# POOL with the alpha-sweep and quantile-native ledgers.
#
# NOTHING HERE FLIPS A DEFAULT. RWM_HOLDDOWN_Q is ABSENT by default and nothing
# shipped reads it.

set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
HD_CELLS="${RWM_HOLD_CELLS:-c1 c7 c8 c8L sc2}"
HD_ARMS="${RWM_HOLD_ARMS:-CTL H500 H136 H037 H010}"
TAG="${RWM_HOLD_TAG:-hold}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/hold
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var that is
# exported in this process reaches the binary whatever the forward list says,
# so the CONTROL arm's "absent" can only be made absent by unsetting it here.
# `RWM_W_FORM` joins the list because ABSENT is one of its three legal states
# and the CTL arm is the arm that has to be in it. `RWM_RTT_DUMP` joins it
# because the dump writes megabytes of sender stderr into the endpoint whose
# own clock this battery measures.
unset RWM_HOLDDOWN_Q RWM_ALPHA_OVERRIDE RWM_QUANTILE_CLOCKS RWM_W_FORM \
      RWM_RTT_DUMP RWM_SUCC_DUMP

# -- THE ARM TABLE -- the SINGLE source of the arm's env, the arm's liveness
#    assertion AND the arm's window-size expectation, so the three cannot
#    drift apart. --------------------------------------------------------
# The level is NOT a flag: it is matched as its own literal token, and the
# [GATES] echo prints the RESOLVED value (the RWM_ACKDIAG_WINDOW_US precedent),
# so a mistyped level resolves back to `unset` and is READ rather than inferred.
#
# THE STRINGS ARE THE ECHO'S STRINGS, LITERALLY. The [GATES] echo prints the
# resolved f64 through Rust's own `to_string()`, and the arm table is matched
# against that echo LITERALLY, on purpose. `0.99` not `0.990`; `0.5` not
# `0.500`. The quantile-native smoke caught exactly this mismatch at 2 of 12
# endpoint-checks (`got='...=0.4' want=0.40`) -- the arm was live and correct
# and the HARNESS would have called it dead at every rep.
arm_q() { # arm -> RWM_HOLDDOWN_Q, or "unset" for the control
  case "$1" in
    H500) echo 0.5 ;;
    H136) echo 0.864 ;;
    H037) echo 0.963 ;;
    H010) echo 0.99 ;;
    CTL)  echo unset ;;
    *)    echo "" ;;
  esac
}

# THE ARM'S EXPECTED `n_req`. TRANSCRIBED, NOT COMPUTED: N(1-q) is a property of
# the arm's level that the ENGINE derives, and a harness that re-derived it
# would agree with the engine by construction instead of checking it. The four
# numbers are pinned ABSOLUTELY in tests/recovery_bench.rs and are the
# pre-registration's own N(a) column.
arm_nreq() { # arm -> expected [HOLD] n_req token
  case "$1" in
    H500) echo 20 ;;
    H136) echo 74 ;;
    H037) echo 271 ;;
    H010) echo 1000 ;;
    CTL)  echo - ;;
    *)    echo "" ;;
  esac
}

# The parser's `alpha_cmd` positional. It carries `unset` on EVERY arm:
# `alpha_cmd` means the QUANTILE CLOCK's alpha in the pooled ledger and this
# battery does not touch that axis. The arm's own level is read back off the
# [HOLD] line's `q=` field, which is the seat that evaluated it.
arm_alpha() { echo unset; }

HD_ARM_GATES="RWM_HOLDDOWN_Q"
# RWM_DELTA_CAP is shipped-ON since 16.71 and is the SUBSTRATE this sweep runs
# on, not an axis of it: same value on every arm, asserted =1 rather than
# assumed, because a reader who takes the pre-flip default mis-scales every
# queue number in the result.
# RWM_W_FORM and RWM_ALPHA_OVERRIDE are SUBSTRATE here and not axes: the timer
# must be the shipped clamp on every row, so `cantelli`/`unset` are ASSERTED
# rather than assumed. They are word gates, not flags -- see the `pat` branch
# in check_and_parse.
HD_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP RWM_W_FORM RWM_ALPHA_OVERRIDE"
HD_CONTAM_GATES="RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP RWM_SUCC_DUMP"

gate_expect() { # arm gate -> expected [GATES] value (the RESOLVED echo)
  case "$2" in
    RWM_HOLDDOWN_Q)      arm_q "$1" ;;
    # ABSENT resolves to `cantelli` at the binary -- the echo prints what the
    # engine RESOLVED, which is the only reading that settles the axis.
    RWM_W_FORM)          echo cantelli ;;
    RWM_ALPHA_OVERRIDE)  echo unset ;;
    RWM_DELTA_CAP)       echo 1 ;;
    RWM_SUM_CAP)         echo 1 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the table above. The control gets NEITHER an
# RWM_ALPHA_OVERRIDE token NOR an RWM_W_FORM token — `unset` is an ABSENCE,
# not a value, and both of those axes have ABSENT as a legal state whose
# resolution the [GATES] echo is what reads back.
arm_env() { # arm -> "RWM_X=v ..."
  local a="$1" g out="" v
  for g in $HD_ARM_GATES $HD_SUBSTRATE_GATES $HD_CONTAM_GATES; do
    case "$g" in
      RWM_HOLDDOWN_Q) v="$(arm_q "$a")" ;;
      # `unset` is an ABSENCE, not a value: the CTL arm gets NO
      # RWM_HOLDDOWN_Q token at all, and NO arm gets an RWM_W_FORM or
      # RWM_ALPHA_OVERRIDE token, because absent is exactly the state whose
      # resolution the [GATES] echo is what reads back.
      RWM_W_FORM|RWM_ALPHA_OVERRIDE) v="unset" ;;
      *)              v="$(gate_expect "$a" "$g")" ;;
    esac
    [ "$v" = "unset" ] && continue
    out="$out $g=$v"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# ccand_battery.sh:202-215 via alpha_battery.sh:147-156, never redefined: a
# cell that differs from the ledger's cell is a different cell and its rows do
# not pool.
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
cell_paths() { case "$1" in c7|c8|c8L) echo 2 ;; *) echo 1 ;; esac; }

# The plain-window amendment's §3 bands, unchanged. CTL ONLY — see the header.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; c8) echo 50;; c8L) echo 45;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; c8) echo 100;; c8L) echo 95;; sc2) echo 92;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

arm_cell_reps() { echo "$REPS"; }

check_and_parse() { # name cell arm alpha cpus cpuc pingp qp
  local name="$1" cell="$2" arm="$3" alpha="$4" cpus="$5" cpuc="$6" pingp="$7" qp="$8"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log

  # THE PARSER IS `alpha_parse.py`, NOT A FORK OF IT. The new columns were
  # added to that file ADDITIVELY — no field removed, no field renamed, the
  # `ALPHARESULT ` prefix unchanged — so this battery's rows POOL with the
  # α-sweep's ledger instead of speaking a second dialect of it.
  python3 ./alpha_parse.py "$cell" "$arm" "$alpha" "$SEED_ARG" "$REP" \
      "$C" "$S" "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "HOLD-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the per-mechanism ACTIVE echoes' own prose
  # contains literal `RWM_*=0` strings (the flip battery's amendment-1 lesson).
  local gl_c gl_s
  gl_c=$(grep "\[GATES\]" "$C" 2>/dev/null | tail -1)
  gl_s=$(grep "\[GATES\]" "$S" 2>/dev/null | tail -1)

  # ABORT-CAUSE FIRST. No [GATES] on EITHER endpoint = ABORT: no datum, no
  # liveness verdict, and NOT in any denominator. Checked before any assertion
  # so an aborted invocation never produces a wall of liveness failures.
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  local g want got_c got_s pat echoline=""
  for g in $HD_ARM_GATES $HD_SUBSTRATE_GATES $HD_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    case "$g" in
      # A WORD, NOT A FLAG. `RWM_W_FORM` echoes `cantelli` or `quantile` and
      # `RWM_ALPHA_OVERRIDE` echoes `unset` or a number; matching either as
      # `[01]` would return the empty string for EVERY value and produce a
      # liveness gate that passes because it never matched.
      RWM_HOLDDOWN_Q|RWM_ALPHA_OVERRIDE|RWM_W_FORM) pat="$g=[^ ]*" ;;
      *)                             pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $HD_ARM_GATES $HD_SUBSTRATE_GATES " in
      *" $g "*)
        [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
        [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
        ;;
      *)
        { [ "$got_c" != "$g=0" ] || [ "$got_s" != "$g=0" ]; } \
          && echo "ARM-CONTAMINATION $name rep=$REP gate=$g cli='$got_c' srv='$got_s'" >> "$OUT"
        ;;
    esac
  done

  # The instruments must be armed on BOTH endpoints or their columns are void.
  local i
  for i in RWM_DIAG RWM_ACKDIAG RWM_WALLDIAG RWM_FDIAG; do
    got_c=$(printf '%s' "$gl_c" | grep -o "$i=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$i=[01]")
    echoline="$echoline $i=$got_c/$got_s(exp1)"
    { [ "$got_c" != "$i=1" ] || [ "$got_s" != "$i=1" ]; } \
      && echo "INSTRUMENT-FAIL-GATE $name rep=$REP gate=$i cli='$got_c' srv='$got_s'" >> "$OUT"
  done
  echo "LIVENESS $name rep=$REP$echoline" >> "$OUT"

  # -- W3: THE HOLD-DOWN GAUGE AT THE SEAT THAT EVALUATES IT ---------------
  # [GATES] can only say what was ASKED FOR. [HOLD] says what the LAW RESOLVED,
  # at the site that resolves it, and its `n_req` is the window law's OWN answer
  # to the arm's level. A row failing this is VOID: its independent variable did
  # not take. Read INDEPENDENTLY OF THE PARSER -- the parser's regex is the
  # thing a token change would break first, so the driver reads raw tokens and
  # the report reads parsed columns.
  local hq hn want_q want_n hl
  want_q="$(arm_q "$arm")"; [ "$want_q" = "unset" ] && want_q="unset"
  want_n="$(arm_nreq "$arm")"
  hl=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -v 'path=-' | tail -1)
  [ -z "$hl" ] && hl=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | tail -1)
  if [ -z "$hl" ]; then
    echo "INSTRUMENT-FAIL-HOLD $name rep=$REP (no [HOLD] at the sender -- the gap-report site was never reached)" >> "$OUT"
    hq=none; hn=none
  else
    # A LEADING SPACE IS LOAD-BEARING. `n_req=20` CONTAINS `q=20`, so the
    # bare pattern matched the window size and the calibration reported
    # `got=20 exp=0.5` at every armed arm - the harness calling a live and
    # correct arm dead, which is exactly what a calibration is for.
    hq=$(printf '%s' "$hl" | grep -o ' q=[^ ]*' | tail -1 | sed 's/^ q=//')
    hn=$(printf '%s' "$hl" | grep -o 'n_req=[^ ]*' | tail -1 | sed 's/^n_req=//')
    hq="${hq:-none}"; hn="${hn:-none}"
    # `q=` is printed to six places; the arm table carries the [GATES] echo's
    # own string. Compare NUMERICALLY so the two renderings cannot drift, and
    # compare `unset` literally because absence is not a number.
    if [ "$want_q" = "unset" ]; then
      [ "$hq" != "unset" ] \
        && echo "W3-QLEVEL-FAIL $name rep=$REP got=$hq exp=unset" >> "$OUT"
    else
      awk -v a="$hq" -v b="$want_q" 'BEGIN{exit !(a+0==b+0 && a!="")}' \
        || echo "W3-QLEVEL-FAIL $name rep=$REP got=$hq exp=$want_q" >> "$OUT"
    fi
    [ "$hn" != "$want_n" ] \
      && echo "W3-NREQ-FAIL $name rep=$REP got=$hn exp=$want_n" >> "$OUT"
    # gen= is load-bearing: under generation coding the gap-report path is
    # structurally empty and this battery's whole measurand reads zero for a
    # configuration reason.
    printf '%s' "$hl" | grep -q 'gen=0' \
      || echo "W3-GEN-FAIL $name rep=$REP (the [HOLD] line is not the plain window)" >> "$OUT"
  fi
  echo "W3HOLD $name rep=$REP q=$hq n_req=$hn (exp$want_q/$want_n)" >> "$OUT"

  # -- W4: THE ROUTING WITNESS ---------------------------------------------
  # `should_hold` is consulted exactly once per fire that reaches
  # `record_fire_cause`, and the fire is then either HELD or CLASSIFIED:
  #
  #     sum([HOLD] evals)  ==  sum([HOLD] sup)  +  [FCAUSE] n
  #
  # Its two sides come from two different gauges at two different sites, so it
  # is the one assertion a gauge agreeing only with itself could not pass. A
  # violation means the gate is NOT where this battery says it is, and the row
  # is VOID.
  local hev hsup hemit fcn fcother fcunattr fcgap
  hev=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'evals=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hsup=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'sup=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hemit=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'emit=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  fcn=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o ' n=[0-9]*' | tr -dc '0-9'); fcn="${fcn:-0}"
  fcgap=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'gap_data=[0-9]*' | tr -dc '0-9'); fcgap="${fcgap:-0}"
  fcother=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'other=[0-9]*' | tr -dc '0-9'); fcother="${fcother:-0}"
  fcunattr=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'unattr=[0-9]*' | tr -dc '0-9'); fcunattr="${fcunattr:-0}"
  echo "W4ROUTE $name rep=$REP evals=$hev sup=$hsup emit=$hemit fcause_n=$fcn gap_data=$fcgap" >> "$OUT"
  [ "$hev" -ne "$(( hsup + hemit ))" ] \
    && echo "ROUTING-FAIL-ACCOUNT $name rep=$REP evals=$hev != sup=$hsup + emit=$hemit" >> "$OUT"
  [ "$hev" -ne "$(( hsup + fcn ))" ] \
    && echo "ROUTING-FAIL-FCAUSE $name rep=$REP evals=$hev != sup=$hsup + [FCAUSE] n=$fcn (the gate is not where this battery says it is)" >> "$OUT"

  # -- W7: THE CLASSIFICATION IS EXHAUSTIVE ON THIS BINARY -------------------
  { [ "$fcother" -ne 0 ] || [ "$fcunattr" -ne 0 ]; } \
    && echo "INSTRUMENT-FAIL-CLASS $name rep=$REP other=$fcother unattr=$fcunattr" >> "$OUT"

  # -- W5: THE SECOND ROUTING WITNESS, AT THE RECEIVER ----------------------
  # The measurand the whole derivation rests on must be LIVE on this binary.
  # [SUCC] times HOLES where [FCAUSE] classifies FIRES: two counters over the
  # same underlying loss, bumped by different code at different events and at
  # different endpoints.
  local scdet scres scof
  scdet=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' det=[0-9]*' | tr -dc '0-9'); scdet="${scdet:-0}"
  scres=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' res=[0-9]*' | tr -dc '0-9'); scres="${scres:-0}"
  scof=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o 'orig_frac=[^ ]*' | tail -1 | sed 's/^orig_frac=//'); scof="${scof:-none}"
  echo "W5SUCC $name rep=$REP det=$scdet res=$scres orig_frac=$scof" >> "$OUT"
  { [ "$scdet" -eq 0 ] || [ "$scres" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-SUCC $name rep=$REP det=$scdet res=$scres" >> "$OUT"

  # THE WIRING TEST'S STATISTIC, printed on its own line so it is auditable
  # from the ledger alone. CROSS-ENDPOINT ON PURPOSE: numerator sender,
  # denominator receiver. Section 16.77.3 predicts rpd ~ 1 - orig_frac*q.
  echo "RPD $name rep=$REP fcause_n=$fcn succ_det=$scdet rpd=$(awk -v a="$fcn" -v b="$scdet" 'BEGIN{print (b>0)? a/b : "-"}') sup_frac=$(awk -v a="$hsup" -v b="$hev" 'BEGIN{print (b>0)? a/b : "-"}')" >> "$OUT"

  # The verbatim gauge dump -- every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone. [HOLD] is in the
  # loop AS WELL AS in the W3 block above: the block prints the ONE line each
  # witness was read from, the loop prints EVERY line, including the per-path
  # rows the report needs and the unattributed bucket it must not pool.
  local f
  for f in HOLD FCAUSE SUCC QALPHA QCLK RACK RFA DCAP WALL LCW; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # -- LAW-DEAD AND WINDOW-PARTIAL. RESULTS, NEVER ABORTS. -----------------
  # `law_n` counts the evaluations at which the arm's OWN window was full.
  # `law_n = 0` on a treatment arm means the window never filled and the arm ran
  # the SHIPPED behaviour at every fire -- the row is VOID for scoring and is
  # REPORTED as such. `law_n < evals` means the arm spent part of the run below
  # its own window, and THAT FRACTION IS THE THING TO SCORE -- the UNSCOREABLE
  # rule applied in the direction it actually points. Deleting the invocation
  # would delete the very evidence that the window did not fill. c1-H010 is
  # PREDICTED here before the run (749 resolutions/rep against N = 1000).
  local hlaw hfed hsamp
  hlaw=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'law_n=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hfed=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'fed=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hsamp=$(printf '%s' "$hl" | grep -o 'samp_n=[0-9]*' | tail -1 | tr -dc '0-9'); hsamp="${hsamp:-0}"
  if [ "$arm" != "CTL" ] && [ "$hlaw" -eq 0 ]; then
    echo "HOLD-LAW-DEAD $name rep=$REP (law_n=0 fed=$hfed samp_n=$hsamp n_req=$hn -- the window never filled; row VOID for scoring, REPORTED as the UNSCOREABLE rule)" >> "$OUT"
  elif [ "$arm" != "CTL" ] && [ "$hlaw" -lt "$hev" ]; then
    echo "WINDOW-PARTIAL $name rep=$REP law_n=$hlaw/$hev fed=$hfed n_req=$hn (RESULT, not an abort -- fraction to be scored)" >> "$OUT"
  fi

  # -- W6/W8/W9 + the band, into one JSONL witness row ---------------------
  local w1 rfa_n w2 w4 w5 mb lo hi lossy inband
  w1=$(grep -o '\[RFA\] gen=[01]' "$S" 2>/dev/null | tail -1 | sed 's/.*gen=//'); w1="${w1:-none}"
  rfa_n=$(grep -c '\[RFA\]' "$S" 2>/dev/null || true); rfa_n="${rfa_n:-0}"
  # W6: the realized-false-repair read must be LIVE, or clause (i)'s second
  # reading has no denominator.
  local rfa_fires
  rfa_fires=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'fires=[0-9]*' | tr -dc '0-9'); rfa_fires="${rfa_fires:-0}"
  [ "$rfa_fires" -eq 0 ] \
    && echo "INSTRUMENT-FAIL-RFA $name rep=$REP fires=0" >> "$OUT"
  w2=$(grep -c '\[PFRAC\]' "$C" 2>/dev/null || true); w2="${w2:-0}"
  # THE MAXIMUM, never the last line -- see the header.
  w4=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tr -dc '0-9\n' | sort -n | tail -1); w4="${w4:-0}"
  w5=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); w5="${w5:-none}"
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  # BAND SCOPE: CTL only. On a treatment arm an out-of-band reading is a
  # RESULT -- section 16.77.9's H4 -- and `band_applies` says so in the row
  # rather than in a footnote.
  local applies=0; [ "$arm" = "CTL" ] && applies=1

  echo "HOLDWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"q\":\"$want_q\",\"n_req\":\"$want_n\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"hold_q\":\"$hq\",\"hold_n_req\":\"$hn\",\"hold_evals\":$hev,\"hold_sup\":$hsup,\"hold_emit\":$hemit,\"hold_law_n\":$hlaw,\"hold_fed\":$hfed,\"hold_samp_n\":$hsamp,\"fcause_n\":$fcn,\"fcause_gap_data\":$fcgap,\"fcause_other\":$fcother,\"fcause_unattr\":$fcunattr,\"succ_det\":$scdet,\"succ_res\":$scres,\"succ_orig_frac\":\"$scof\",\"rfa_lines\":$rfa_n,\"rfa_fires\":$rfa_fires,\"W1_rfa_gen\":\"$w1\",\"W2_pfrac_lines\":$w2,\"W4_retx_max\":$w4,\"W5_rack_fa\":\"$w5\"}" \
    | tee -a "$OUTDIR/${TAG}-witness-s${SEED_ARG}.jsonl" >> "$OUT"
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs ca cb mode bytes alpha
  envs="$(arm_env "$arm")"
  alpha="$(arm_alpha "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name q=$(arm_q "$arm") n_req=$(arm_nreq "$arm") seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate on it.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$alpha" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true); pn="${pn:-0}"
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"

  # Per-rep captures. The driver's `trap cleanup EXIT` destroys the namespaces
  # the instant it returns, so these are copied under rep-unique names now.
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
  local li
  for li in 0 1 2 3; do
    [ -f "/tmp/rwm-ping-$li.txt" ] \
      && cp "/tmp/rwm-ping-$li.txt" "$DDIR/${name}-s${SEED_ARG}-r${REP}-p${li}.txt" 2>/dev/null
  done
  cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $HD_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $HD_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== HOLD BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate \"THE HOLD-DOWN SWEEP -- PRE-REGISTRATION\", and nothing else. Paper 16.77 is the derivation."
  echo "CELLS $HD_CELLS"
  echo "ARMS  $HD_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $HD_ARMS; do echo "ARMENV $A q=$(arm_q "$A") n_req=$(arm_nreq "$A") | $(arm_env "$A")"; done
  echo "AXIS  RWM_HOLDDOWN_Q, ABSENT by default; GARBAGE and any q outside (0,1) resolve back to ABSENT and print unset"
  echo "TIMER the SHIPPED [25,100] ms clamp on EVERY row: RWM_QUANTILE_CLOCKS/RWM_RACK_CLOCKS/RWM_DERIVED_SWEEP are CONTAMINATION gates and RWM_W_FORM must resolve cantelli"
  echo "W3    [HOLD] q=/n_req=/gen= at the SENDER, the seat that resolves them"
  echo "W4    THE ROUTING WITNESS: sum([HOLD] evals) == sum([HOLD] sup) + [FCAUSE] n, two gauges at two sites"
  echo "W5    [SUCC] det/res at the RECEIVER -- the measurand the derivation rests on, live on THIS binary"
  echo "RPD   the wiring test statistic, CROSS-ENDPOINT: [FCAUSE] n (sender) / [SUCC] det (receiver)"
  echo "HOLD-LAW-DEAD and WINDOW-PARTIAL are RESULTS and never aborts -- law_n/evals is the fraction to be scored"
  echo "PREDICTED before the run: c1-H010 UNSCOREABLE or heavily partial (749 resolutions/rep against N=1000)"
  echo "BANDSCOPE the goodput abort bands apply to CTL ONLY; out-of-band on a treatment arm is a RESULT (16.77.9 H4)"
  echo "BIN $BIN"
  echo "SHA256 $(sha256sum "$BIN" 2>/dev/null)"
  echo "COMMIT $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "KERNEL $(uname -r)"
  echo "UPTIME $(uptime)"
  echo "COTENANT kwin=$(pgrep -c kwin_x11 2>/dev/null || echo 0) sddm=$(pgrep -c sddm 2>/dev/null || echo 0)"
  echo "CPU $(lscpu | grep -E 'Model name' | head -1)"
  echo "CPUFLAGS $(lscpu | grep -oE 'aes|avx2|pclmulqdq' | sort -u | tr '\n' ' ')"
} >> "$OUT"

RC=0
for REP in $(seq 1 "$REPS"); do
  for CELL in $HD_CELLS; do
    for ARM in $HD_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — live n is law_n>0; the PRE-REGISTRATION applies the bars) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $HD_CELLS; do
  for A in $HD_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "HOLD-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo HOLD-BATTERY-DONE-$SEED_ARG
