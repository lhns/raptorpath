#!/bin/bash
# THE (q, refresh) SWEEP — DOES LOWERING THE RECEIVER'S REPORT CADENCE MAKE THE
# HOLD-DOWN LEVEL EXPRESSIBLE, AND DOES THE REPAIR COUNT THEN MOVE?
#
#   sudo bash qref_battery.sh <seed> [reps]
#
# THE QUESTION. Section 16.77's battery swept `q` at a FIXED refresh and read
# flat at 5 of 5 cells. Section 16.77.8d then showed that outcome was
# GUARANTEED IN ADVANCE at 4 of them: two levels of `q` differing by less than
# ONE refresh interval command THE SAME CLOCK. The sweep had one dimension; the
# instrument had one step. This battery sweeps THE PAIR.
#
#     COST( q , refresh )   q       -- the hold-down LEVEL (16.77.3's order stat)
#                           refresh -- the RESOLUTION at which a level is
#                                      expressible at all
#
# `refresh` IS NOT A SECOND TREATMENT. It is the DOMAIN of the first. Lowering
# it is not hypothesised to make the machine better; it makes `q`'s commanded
# level distinguishable from its neighbours, which is the precondition every
# clause of 16.77.8 was written assuming and none of them had.
#
# THE GATE. `RWM_REFRESH_FLOOR_US` is `F` in the re-expressed cadence law
#
#     hole_nack_refresh(srtt) = (2*srtt).clamp( F , R*F ),  R = 4, F absent => 25 ms
#
# `R` is NOT a new constant: it is the shipped clamp's own aspect ratio, and
# 100 ms = 4 x 25 ms EXACTLY, so with `F` absent the engine is BYTE-IDENTICAL.
# There is no branch on `F`, no second law, no threshold selecting a code path.
# Paper section 16.78 is the derivation; goal-gate's "(q, refresh)" pre-
# registration is the contract this script implements and is the ONLY place the
# scoring bars live. This script emits counts; it applies no bar, names no
# winner.
#
# WHY THE WHOLE BAND SCALES AND NOT THE LOWER RAIL ALONE. The rail that censors
# the measurement is WHICHEVER RAIL BINDS AT THAT CELL, and it is not the same
# rail everywhere (16.78.0):
#
#     c1    2*srtt ~   4 ms  <= F     LOWER rail binds  =>  cadence = F
#     c7    2*srtt >= 100 ms >= R*F   UPPER rail binds  =>  cadence = 4*F
#     sc2   2*srtt >= 100 ms >= R*F   UPPER rail binds  =>  cadence = 4*F
#
# An override of the LOWER clamp alone would be INERT at two of the three cells
# this sweep runs -- a battery whose treatment cannot reach two of its cells is
# a battery whose criteria were unsatisfiable when they were written, which is
# the defect this tree has already recorded once against a three-term battery
# and does not intend to record twice.
#
# THE ARMS ARE DERIVED, NOT CHOSEN (16.78.3). The region is read off `[SUCC]`'s
# MEASURED `orig` p50 per cell -- the median time from hole detection to
# resolution by the hole's OWN original, which is exactly the quantity the
# hold-down is meant to sit INSIDE and the cadence currently sits ON TOP OF --
# and the target cadences are that median's halves and quarters:
#
#     cell   [SUCC] orig p50   shipped effective floor   p50/2      p50/4
#     c1        24.6 ms                25 ms  (1.02x)    12.3 ms    6.2 ms
#     c7        30.7 ms               100 ms  (3.26x)    15.4 ms    7.7 ms
#     sc2       98.3 ms               100 ms  (1.02x)    49.2 ms   24.6 ms
#
# `F` IS THEN DERIVED FROM THE TARGET AND THE BINDING RAIL, PER CELL -- see
# `arm_floor` and `arm_cadence` below, both TRANSCRIBED from 16.78.3's table.
#
#   CTL      F absent, q absent      the shipped machine, byte-identically
#   R0Q99    F absent, q = 0.99      16.77's own arm, re-run as the JOIN row
#   R0Q86    F absent, q = 0.864     ""
#   R2Q99    F = p50/2, q = 0.99     the first cadence below the floor
#   R2Q86    F = p50/2, q = 0.864
#   R4Q99    F = p50/4, q = 0.99     the deepest cadence in the region
#   R4Q86    F = p50/4, q = 0.864
#
# THE R0 ROWS ARE NOT FILLER. They are the SAME `q` at the SHIPPED cadence, so
# the (q, refresh) surface has a measured edge at `refresh = shipped` and any
# movement attributed to the cadence is read against a same-seed, same-cell,
# same-`q` row rather than against 16.77's ledger across an era boundary.
#
# RWM_GEN=0 ON EVERY ROW, AND IT IS LOAD-BEARING. Under generation coding the
# SACK->gap producer is suppressed (recv_nack_tx = None), so [FCAUSE]'s gap_
# classes and [SUCC]'s orig are STRUCTURALLY EMPTY -- this battery's entire
# measurand would read zero for a configuration reason. W4 asserts `gen=0` off
# the MEASURAND'S OWN GAUGES ([HOLD], [FCAUSE], [SUCC]) rather than off the
# [GATES] echo: `RWM_GEN` is the generation SIZE IN SYMBOLS, so the engine
# echoes its default `RWM_GEN=384` even when the invocation passed 0 and
# generation coding is off. MEASURED in this battery's calibration.
#
# THE TIMER IS DISARMED ON EVERY ROW. RWM_QUANTILE_CLOCKS, RWM_RACK_CLOCKS and
# RWM_DERIVED_SWEEP are contamination gates here and RWM_W_FORM must resolve to
# `cantelli`: the shipped [25,100] ms clamp is the SENDER's timer on every row,
# on every arm, so the only axes are `q` and the RECEIVER's report cadence. A
# row whose sender timer moved is not a row of this battery.
#
# WITNESSES, per invocation (the pre-registration's numbering):
#
#   W1  [GATES] on BOTH endpoints. Neither => ABORT (no datum, no denominator).
#   W2  RWM_REFRESH_FLOOR_US RESOLVED == the (cell, arm) expectation, on BOTH
#       endpoints.                                        W2-FLOOR-MISMATCH
#   W3  RWM_HOLDDOWN_Q RESOLVED == the arm's own level, LITERALLY, on both.
#                                                         W3-Q-MISMATCH
#   W4  contamination gates all 0 AND gen=0 at [HOLD]/[FCAUSE]/[SUCC].
#                                                         W4-CONTAM
#   W5  [HOLD] present at the sender, evals > 0, and evals == sup + emit.
#                                                         W5-NO-HOLD
#   W6  [QCLK] site=receiver present with evals > 0 and kept > 0 -- the seat
#       that REALIZES the refresh cadence has a distribution to report.
#                                                         W6-NO-QCLK
#   W7  [FCAUSE] present and n == timer + gap_data + gap_refresh + other.
#                                                         W7-NO-FCAUSE
#   W8  [SUCC] present at the receiver with det > 0.      W8-NO-SUCC
#   W9  [RFA] present at the receiver.                    W9-NO-RFA
#   W10 rc == 0 and mean_mbps scraped.                    W10-RC
#
# **THE WIRING WITNESS, AND IT IS THE POINT OF THE BATTERY.** 16.78.6's `F1`:
# a commanded `F` must actually PRODUCE a delivered cadence below the cell's
# SHIPPED effective floor. The DELIVERED CADENCE, NOT `F`, IS THE TREATMENT,
# and this battery WITNESSES it instead of assuming it -- off the RECEIVER's
# own `[QCLK] site=receiver` line, the seat that realizes the cadence:
#
#     CADENCE <cell>-<arm> want=.. p50=.. min=.. max=.. floor_shipped=.. below=..
#
# `below=1` iff `w_us_max < ` the cell's shipped effective floor: the MAXIMUM,
# because one refresh at or above the old floor is enough to say the region was
# not entered. THIS IS A RESULT LINE AND NEVER AN ABORT. If `F1` fires, every
# other clause is VOID -- but the row that shows it is void is the row that
# NAMES THE NEXT CADENCE IN THE CHAIN, and deleting the invocation would delete
# exactly that evidence.
#
# LAW-DEAD IS A RESULT, NOT AN ABORT. `law_n = 0` on an armed arm means the
# hold-down window never filled and the arm ran the SHIPPED behaviour at every
# fire; the row is VOID for scoring and is REPORTED as `ARM-VOID`.
#
# BANDSCOPE: the goodput abort bands apply to CTL ONLY. On a treatment arm an
# out-of-band reading is a RESULT -- it is 16.78.4's P-B, the prediction the
# band would otherwise abort before it could be read -- and `band_applies` says
# so IN THE ROW rather than in a footnote.
#
# CELLS AND BANDS ARE THE SUCCESSOR-ARRIVAL PASS'S, TRANSCRIBED AND NEVER
# REDEFINED: a cell that differs from the ledger's cell is a different cell and
# its rows do not pool with the pass this one continues. c8 and c8L are NOT run
# and that is DERIVED, not convenience: the successor-arrival pass recorded
# their quantiles NOT USABLE as derivation inputs (rep dispersion up to 52x),
# and a cell whose p50 cannot be trusted cannot supply a p50/2.
#
# THE PARSER IS alpha_parse.py, NOT A FORK OF IT -- so this battery's rows POOL
# with the alpha-sweep, quantile-native and hold-down ledgers.
#
# NOTHING HERE FLIPS A DEFAULT. RWM_REFRESH_FLOOR_US and RWM_HOLDDOWN_Q are
# both ABSENT by default and nothing shipped reads either.

set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-4}"
QR_CELLS="${RWM_QREF_CELLS:-c1 c7 sc2}"
QR_ARMS="${RWM_QREF_ARMS:-CTL R0Q99 R0Q86 R2Q99 R2Q86 R4Q99 R4Q86}"
TAG="${RWM_QREF_TAG:-qref}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/qrefresh
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
WIT="$OUTDIR/${TAG}-witness-s${SEED_ARG}.jsonl"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var that is
# exported in this process reaches the binary whatever the forward list says,
# so the CONTROL arm's "absent" can only be made absent by unsetting it here.
# `RWM_REFRESH_FLOOR_US` is on this list for exactly that reason and it is the
# NEW one: ABSENT is the state in which the cadence law is byte-identical to
# the shipped clamp, so an inherited value would make the CTL and R0 arms a
# lifted-cadence configuration silently sharing a ledger with the control they
# are supposed to BE. `RWM_W_FORM` joins the list because ABSENT is one of its
# three legal states. `RWM_RTT_DUMP`/`RWM_SUCC_DUMP` join it because the dumps
# write megabytes of stderr into the endpoints whose own clocks this battery
# measures.
unset RWM_HOLDDOWN_Q RWM_REFRESH_FLOOR_US RWM_ALPHA_OVERRIDE \
      RWM_QUANTILE_CLOCKS RWM_W_FORM RWM_RTT_DUMP RWM_SUCC_DUMP

# ── THE ARM TABLES ───────────────────────────────────────────────────────
# FOUR PARALLEL TABLES over the SAME arm names: the arm's env, the arm's
# window-size expectation, the arm's cadence-floor input, and the cadence that
# input must DELIVER. They are written adjacent, in one block, because the
# failure mode they exist to prevent is DRIFT between them -- an arm whose env
# says one thing and whose expectation says another produces a harness that
# calls a live arm dead, or a dead arm live, and both readings look like data.
#
# THE STRINGS ARE THE ECHO'S STRINGS, LITERALLY. The [GATES] echo prints the
# RESOLVED f64 through Rust's own `to_string()`, and the arm table is matched
# against that echo LITERALLY, on purpose. `0.99` not `0.990`; `0.864` not
# `0.8640`. The quantile-native smoke caught exactly this mismatch at 2 of 12
# endpoint-checks (`got='...=0.4' want=0.40`) -- the arm was live and correct
# and the HARNESS would have called it dead at every rep.
arm_q() { # arm -> RWM_HOLDDOWN_Q, or "unset" for the control
  case "$1" in
    R0Q99|R2Q99|R4Q99) echo 0.99 ;;
    R0Q86|R2Q86|R4Q86) echo 0.864 ;;
    CTL)               echo unset ;;
    *)                 echo "" ;;
  esac
}

# THE ARM'S EXPECTED `n_req`. TRANSCRIBED, NOT COMPUTED: N(1-q) is a property
# of the arm's level that the ENGINE derives, and a harness that re-derived it
# would agree with the engine BY CONSTRUCTION instead of checking it. Both
# numbers are pinned ABSOLUTELY in tests/recovery_bench.rs and are the hold-down
# pre-registration's own N(a) column.
arm_nreq() { # arm -> expected [HOLD] n_req token
  case "$(arm_q "$1")" in
    0.99)  echo 1000 ;;
    0.864) echo 74 ;;
    unset) echo - ;;
    *)     echo "" ;;
  esac
}

# THE CADENCE-FLOOR INPUT, PER (CELL, ARM). TRANSCRIBED from paper 16.78.3's
# derived table -- NOT computed from a p50 this script re-reads, because a
# script that recomputed `F` from the run's own [SUCC] would move its treatment
# with its measurement and no two reps would be the same arm.
#
# IT DEPENDS ON THE CELL AND THIS IS THE WHOLE POINT (16.78.0): the binding
# rail differs per cell, so the `F` that delivers a given cadence differs per
# cell. `unset` is an ABSENCE -- the gate is not passed at all on CTL and on
# every R0 arm, and the [GATES] echo reads back `unset`, which is the only
# reading that settles the axis.
arm_floor() { # cell arm -> RWM_REFRESH_FLOOR_US, or "unset"
  case "$2" in
    CTL|R0Q99|R0Q86) echo unset ;;
    R2Q99|R2Q86)
      case "$1" in
        c1)  echo 12300 ;;
        c7)  echo 3838 ;;
        sc2) echo 12288 ;;
        *)   echo "" ;;
      esac ;;
    R4Q99|R4Q86)
      case "$1" in
        c1)  echo 6150 ;;
        c7)  echo 1919 ;;
        sc2) echo 6144 ;;
        *)   echo "" ;;
      esac ;;
    *) echo "" ;;
  esac
}

# THE CELL'S SHIPPED EFFECTIVE FLOOR -- the cadence the UNLIFTED machine
# realizes at this cell, i.e. whichever rail of the shipped [25,100] ms clamp
# binds there (16.78.0's table). It is the CTL/R0 `want`, and it is the
# threshold `below=` is measured against.
cell_shipped_floor() { case "$1" in c1) echo 25000 ;; c7) echo 100000 ;; sc2) echo 100000 ;; *) echo 25000 ;; esac; }

# THE DELIVERED CADENCE THE ENGINE MUST PRODUCE, in µs, per (cell, arm).
# TRANSCRIBED from 16.78.3 as well, and it is NOT `F`: at c1 the LOWER rail
# binds so cadence = F, at c7 and sc2 the UPPER rail binds so cadence = 4*F.
# This is the number `F1` -- the wiring witness -- scores against, and writing
# it as a table rather than as `F` or `4*F` chosen by a branch is deliberate:
# a branch here would be the harness re-deriving the engine's own arithmetic
# and agreeing with itself.
arm_cadence() { # cell arm -> the DELIVERED cadence in µs
  case "$2" in
    CTL|R0Q99|R0Q86) cell_shipped_floor "$1" ;;
    R2Q99|R2Q86)
      case "$1" in
        c1)  echo 12300 ;;   # LOWER rail binds: cadence = F
        c7)  echo 15352 ;;   # UPPER rail binds: cadence = 4F
        sc2) echo 49152 ;;   # UPPER rail binds: cadence = 4F
        *)   echo 0 ;;
      esac ;;
    R4Q99|R4Q86)
      case "$1" in
        c1)  echo 6150 ;;
        c7)  echo 7676 ;;
        sc2) echo 24576 ;;
        *)   echo 0 ;;
      esac ;;
    *) echo 0 ;;
  esac
}

# THE MEASURED SELF-HEAL MEDIAN, per cell -- `[SUCC]`'s `orig` p50 from the
# successor-arrival pass, TRANSCRIBED. It is the quantity the whole arm grid is
# scaled to (the R2 arms are p50/2, the R4 arms p50/4) and it is carried in the
# ledger header so a reader can re-derive the grid from the ledger alone rather
# than from a paper section that may have been edited since.
cell_selfheal_p50_us() { case "$1" in c1) echo 24600 ;; c7) echo 30700 ;; sc2) echo 98300 ;; *) echo 0 ;; esac; }

# The parser's `alpha_cmd` positional. It carries `unset` on EVERY arm:
# `alpha_cmd` means the QUANTILE CLOCK's alpha in the pooled ledger and this
# battery does not touch that axis.
arm_alpha() { echo unset; }

QREF_ARM_GATES="RWM_HOLDDOWN_Q RWM_REFRESH_FLOOR_US"
# RWM_DELTA_CAP is shipped-ON since 16.71 and is the SUBSTRATE this sweep runs
# on, not an axis of it: same value on every arm, asserted =1 rather than
# assumed, because a reader who takes the pre-flip default mis-scales every
# queue number in the result.
# RWM_W_FORM and RWM_ALPHA_OVERRIDE are SUBSTRATE here and not axes: the
# SENDER's timer must be the shipped clamp on every row, so `cantelli`/`unset`
# are ASSERTED rather than assumed. They are word gates, not flags -- see the
# `pat` branch in check_and_parse.
QREF_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP RWM_W_FORM RWM_ALPHA_OVERRIDE"
QREF_CONTAM_GATES="RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP RWM_SUCC_DUMP"

# gate -> expected [GATES] value (the RESOLVED echo). IT TAKES THE CELL, unlike
# the hold-down battery's, and it has to: `RWM_REFRESH_FLOOR_US` is the one
# gate in this tree whose expected value is a function of the CELL as well as
# the arm, because the binding rail is a property of the cell (16.78.0). A
# two-argument version of this function could not express the treatment.
gate_expect() { # cell arm gate -> expected [GATES] value
  case "$3" in
    RWM_HOLDDOWN_Q)       arm_q "$2" ;;
    RWM_REFRESH_FLOOR_US) arm_floor "$1" "$2" ;;
    # ABSENT resolves to `cantelli` at the binary -- the echo prints what the
    # engine RESOLVED, which is the only reading that settles the axis.
    RWM_W_FORM)           echo cantelli ;;
    RWM_ALPHA_OVERRIDE)   echo unset ;;
    RWM_DELTA_CAP)        echo 1 ;;
    RWM_SUM_CAP)          echo 1 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the tables above. `unset` is an ABSENCE, not a
# value: an arm whose expectation is `unset` gets NO token at all, because
# absent is exactly the state whose RESOLUTION the [GATES] echo is what reads
# back. CTL therefore gets neither an RWM_HOLDDOWN_Q nor an
# RWM_REFRESH_FLOOR_US token, and NO arm gets an RWM_W_FORM or
# RWM_ALPHA_OVERRIDE token.
arm_env() { # cell arm -> "RWM_X=v ..."
  local c="$1" a="$2" g out="" v
  for g in $QREF_ARM_GATES $QREF_SUBSTRATE_GATES $QREF_CONTAM_GATES; do
    case "$g" in
      RWM_W_FORM|RWM_ALPHA_OVERRIDE) v="unset" ;;
      *)                             v="$(gate_expect "$c" "$a" "$g")" ;;
    esac
    [ "$v" = "unset" ] && continue
    out="$out $g=$v"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# hold_battery.sh:214-223 via ccand_battery.sh:202-215, never redefined: a cell
# that differs from the ledger's cell is a different cell and its rows do not
# pool.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    *) echo "" ;;
  esac
}
cell_paths() { case "$1" in c7) echo 2 ;; *) echo 1 ;; esac; }

# The plain-window amendment's §3 bands, unchanged. CTL ONLY — see BANDSCOPE.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; sc2) echo 92;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

arm_cell_reps() { echo "$REPS"; }

check_and_parse() { # name cell arm alpha cpus cpuc pingp qp
  local name="$1" cell="$2" arm="$3" alpha="$4" cpus="$5" cpuc="$6" pingp="$7" qp="$8"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log
  local FAILS=""     # the witness fail tokens, accumulated and carried in the row

  # THE PARSER IS `alpha_parse.py`, NOT A FORK OF IT. The [HOLD], [FCAUSE] and
  # [SUCC] columns were added to that file ADDITIVELY — no field removed, no
  # field renamed, the `ALPHARESULT ` prefix unchanged — so this battery's rows
  # POOL with the α-sweep and hold-down ledgers instead of speaking a second
  # dialect of them.
  python3 ./alpha_parse.py "$cell" "$arm" "$alpha" "$SEED_ARG" "$REP" \
      "$C" "$S" "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "QREF-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the per-mechanism ACTIVE echoes' own prose
  # contains literal `RWM_*=0` strings (the flip battery's amendment-1 lesson).
  local gl_c gl_s
  gl_c=$(grep "\[GATES\]" "$C" 2>/dev/null | tail -1)
  gl_s=$(grep "\[GATES\]" "$S" 2>/dev/null | tail -1)

  # -- W1: ABORT-CAUSE FIRST -----------------------------------------------
  # No [GATES] on EITHER endpoint = ABORT: no datum, no liveness verdict, and
  # NOT in any denominator. Checked before any assertion so an aborted
  # invocation never produces a wall of liveness failures that read like ten
  # separate defects.
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi
  { [ -z "$gl_c" ] || [ -z "$gl_s" ]; } && FAILS="$FAILS W1-NO-GATES"

  local g want got_c got_s pat echoline=""
  for g in $QREF_ARM_GATES $QREF_SUBSTRATE_GATES $QREF_CONTAM_GATES; do
    want="$(gate_expect "$cell" "$arm" "$g")"
    case "$g" in
      # A WORD, NOT A FLAG. `RWM_W_FORM` echoes `cantelli` or `quantile`,
      # `RWM_ALPHA_OVERRIDE` echoes `unset` or a number, and
      # `RWM_REFRESH_FLOOR_US` echoes `unset` or a µs integer; matching any of
      # them as `[01]` would return the empty string for EVERY value and
      # produce a liveness gate that passes because it never matched.
      RWM_HOLDDOWN_Q|RWM_REFRESH_FLOOR_US|RWM_ALPHA_OVERRIDE|RWM_W_FORM) pat="$g=[^ ]*" ;;
      *)                                                                 pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $QREF_ARM_GATES $QREF_SUBSTRATE_GATES " in
      *" $g "*)
        [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
        [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
        ;;
      *)
        { [ "$got_c" != "$g=0" ] || [ "$got_s" != "$g=0" ]; } \
          && { echo "ARM-CONTAMINATION $name rep=$REP gate=$g cli='$got_c' srv='$got_s'" >> "$OUT"; FAILS="$FAILS W4-CONTAM"; }
        ;;
    esac
  done

  # -- W2 and W3, NAMED SEPARATELY FROM THE LOOP ---------------------------
  # The loop above emits per-gate liveness lines; W2 and W3 are the two gates
  # that ARE the treatment, so they get their own tokens in the row. Both are
  # LITERAL comparisons against the RESOLVED echo -- the echo prints what the
  # engine decided, and a mistyped level resolves back to `unset` and is READ
  # rather than inferred.
  local floor_exp floor_got q_exp q_got
  floor_exp="$(arm_floor "$cell" "$arm")"
  q_exp="$(arm_q "$arm")"
  floor_got=$(printf '%s' "$gl_c" | grep -o 'RWM_REFRESH_FLOOR_US=[^ ]*' | sed 's/^RWM_REFRESH_FLOOR_US=//'); floor_got="${floor_got:-none}"
  q_got=$(printf '%s' "$gl_c" | grep -o 'RWM_HOLDDOWN_Q=[^ ]*' | sed 's/^RWM_HOLDDOWN_Q=//'); q_got="${q_got:-none}"
  local floor_got_s q_got_s
  floor_got_s=$(printf '%s' "$gl_s" | grep -o 'RWM_REFRESH_FLOOR_US=[^ ]*' | sed 's/^RWM_REFRESH_FLOOR_US=//'); floor_got_s="${floor_got_s:-none}"
  q_got_s=$(printf '%s' "$gl_s" | grep -o 'RWM_HOLDDOWN_Q=[^ ]*' | sed 's/^RWM_HOLDDOWN_Q=//'); q_got_s="${q_got_s:-none}"
  # BOTH ENDPOINTS. The refresh cadence is realized at the RECEIVER and the
  # hold is evaluated at the SENDER, so a gate live at one endpoint and dead at
  # the other is a HALF-ARMED ARM -- the configuration most likely to produce a
  # plausible, wrong number.
  { [ "$floor_got" != "$floor_exp" ] || [ "$floor_got_s" != "$floor_exp" ]; } \
    && { echo "W2-FLOOR-MISMATCH $name rep=$REP cli='$floor_got' srv='$floor_got_s' exp=$floor_exp" >> "$OUT"; FAILS="$FAILS W2-FLOOR-MISMATCH"; }
  { [ "$q_got" != "$q_exp" ] || [ "$q_got_s" != "$q_exp" ]; } \
    && { echo "W3-Q-MISMATCH $name rep=$REP cli='$q_got' srv='$q_got_s' exp=$q_exp" >> "$OUT"; FAILS="$FAILS W3-Q-MISMATCH"; }

  # -- W4: GENERATION CODING IS OFF, READ OFF THE GAUGES' OWN `gen=` --------
  # Under generation coding the SACK->gap producer is suppressed, so [FCAUSE]'s
  # gap classes and [SUCC]'s orig are STRUCTURALLY EMPTY and this battery's
  # whole measurand reads zero for a configuration reason.
  #
  # READ FROM THE GAUGE LINE AND *NOT* FROM `[GATES] RWM_GEN=`. `RWM_GEN` is
  # the generation SIZE IN SYMBOLS, not a flag: the invocation passes
  # `RWM_GEN=0` meaning "no generation coding", and the engine resolves and
  # echoes its own default `RWM_GEN=384` regardless. MEASURED in this battery's
  # calibration, where a check against the echo fired W4-CONTAM at 6 of 6
  # invocations while generation coding was in fact OFF at 6 of 6. The witness
  # that carries the meaning is the one the MEASURAND'S OWN GAUGES stamp:
  # `[HOLD]`, `[FCAUSE]` and `[SUCC]` each print `gen=<0|1>`, and `gen=0` there
  # is the statement that the rows this battery scores were produced with the
  # gap-report path live. That is a stronger check than the env echo, because
  # it is taken at the site rather than at the launcher.
  local gen_h gen_f gen_u
  gen_h=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | tail -1 | grep -o ' gen=[0-9]*' | sed 's/^ gen=//'); gen_h="${gen_h:-none}"
  gen_f=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o ' gen=[0-9]*' | sed 's/^ gen=//'); gen_f="${gen_f:-none}"
  gen_u=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' gen=[0-9]*' | sed 's/^ gen=//'); gen_u="${gen_u:-none}"
  { [ "$gen_h" != "0" ] || [ "$gen_f" != "0" ] || [ "$gen_u" != "0" ]; } \
    && { echo "W4-CONTAM $name rep=$REP gen HOLD='$gen_h' FCAUSE='$gen_f' SUCC='$gen_u' exp=0 (the gap-report path is structurally empty under generation coding)" >> "$OUT"; FAILS="$FAILS W4-CONTAM"; }

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

  # -- W5: THE HOLD GAUGE AT THE SEAT THAT EVALUATES IT ---------------------
  # [GATES] can only say what was ASKED FOR. [HOLD] says what the LAW RESOLVED,
  # at the site that resolves it. Read INDEPENDENTLY OF THE PARSER -- the
  # parser's regex is the thing a token change would break first, so the driver
  # reads raw tokens and the report reads parsed columns.
  local hl hq hn htus hd50
  hl=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -v 'path=-' | tail -1)
  [ -z "$hl" ] && hl=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | tail -1)
  # A LEADING SPACE IS LOAD-BEARING. `n_req=20` CONTAINS `q=20`, so the bare
  # pattern matched the window size and the hold-down calibration reported
  # `got=20 exp=0.5` at every armed arm -- the harness calling a live and
  # correct arm dead, which is exactly what a calibration is for.
  hq=$(printf '%s' "$hl" | grep -o ' q=[^ ]*' | tail -1 | sed 's/^ q=//'); hq="${hq:-none}"
  hn=$(printf '%s' "$hl" | grep -o 'n_req=[^ ]*' | tail -1 | sed 's/^n_req=//'); hn="${hn:-none}"
  htus=$(printf '%s' "$hl" | grep -o ' t_us=[^ ]*' | tail -1 | sed 's/^ t_us=//'); htus="${htus:-none}"
  hd50=$(printf '%s' "$hl" | grep -o 'hd_p50_us=[^ ]*' | tail -1 | sed 's/^hd_p50_us=//'); hd50="${hd50:-none}"

  local hev hsup hemit hlaw
  hev=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'evals=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hsup=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'sup=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hemit=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'emit=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  hlaw=$(grep "\[HOLD\] site=sender" "$C" 2>/dev/null | grep -o 'law_n=[0-9]*' | tr -dc '0-9\n' | awk '{t+=$1} END{print t+0}')
  if [ -z "$hl" ] || [ "$hev" -eq 0 ]; then
    echo "W5-NO-HOLD $name rep=$REP (no [HOLD] at the sender, or evals=0 -- the gap-report site was never reached)" >> "$OUT"
    FAILS="$FAILS W5-NO-HOLD"
  fi
  # `should_hold` is consulted exactly ONCE per fire that reaches
  # record_fire_cause, and the fire is then either HELD or CLASSIFIED, so
  # evals == sup + emit is an accounting identity of the gate itself. A
  # violation means the gate is NOT where this battery says it is.
  [ "$hev" -ne "$(( hsup + hemit ))" ] \
    && { echo "W5-NO-HOLD $name rep=$REP evals=$hev != sup=$hsup + emit=$hemit (the gate is not where this battery says it is)" >> "$OUT"; FAILS="$FAILS W5-NO-HOLD"; }
  echo "W5HOLD $name rep=$REP q=$hq n_req=$hn t_us=$htus hd_p50_us=$hd50 evals=$hev law_n=$hlaw sup=$hsup emit=$hemit (exp q=$q_exp n_req=$(arm_nreq "$arm"))" >> "$OUT"

  # ── W6 + THE WIRING WITNESS (16.78.6's F1) ───────────────────────────────
  # THE DELIVERED CADENCE, NOT `F`, IS THE TREATMENT. `[QCLK] site=receiver` is
  # the RECEIVER's own reading of the hole-refresh cadence it REALIZES -- the
  # seat that owns the clock this battery lifts -- and its w_us_* distribution
  # is the only place the lift can be seen to have LANDED rather than merely
  # been requested. `kept` is the sample count behind the quantiles: a p50 over
  # zero kept samples is a rendering, not a reading, so it is checked.
  local ql qev qkept qp50 qmin qmax want_cad shipped below
  ql=$(grep "\[QCLK\] site=receiver" "$S" 2>/dev/null | tail -1)
  qev=$(printf '%s' "$ql" | grep -o ' evals=[0-9]*' | tail -1 | tr -dc '0-9'); qev="${qev:-0}"
  qkept=$(printf '%s' "$ql" | grep -o ' kept=[0-9]*' | tail -1 | tr -dc '0-9'); qkept="${qkept:-0}"
  # `sed 's/^KEY=//'` and NOT `tr -dc '0-9'`: the key `w_us_p50` CONTAINS the
  # digits `50`, and `tr -dc` strips non-digits from the WHOLE match, so
  # `w_us_p50=25000` reads back as `5025000`. MEASURED in this battery's own
  # calibration, which is why the calibration exists. `w_us_min` / `w_us_max` /
  # `evals` / `kept` have no digits in their names and were never affected -
  # `below=` is computed from `max`, so the F1 verdict was never at risk - but
  # the p50 is the cadence figure clauses (i) and (iv) read, and a corrupted one
  # would have been a two-decade error inside a scored column.
  qp50=$(printf '%s' "$ql" | grep -o 'w_us_p50=[0-9]*' | tail -1 | sed 's/^w_us_p50=//'); qp50="${qp50:--1}"
  qmin=$(printf '%s' "$ql" | grep -o 'w_us_min=[0-9]*' | tail -1 | tr -dc '0-9'); qmin="${qmin:--1}"
  qmax=$(printf '%s' "$ql" | grep -o 'w_us_max=[0-9]*' | tail -1 | tr -dc '0-9'); qmax="${qmax:--1}"
  if [ -z "$ql" ] || [ "$qev" -eq 0 ] || [ "$qkept" -eq 0 ]; then
    echo "W6-NO-QCLK $name rep=$REP (no [QCLK] site=receiver, or evals=$qev kept=$qkept -- the seat that realizes the cadence has no distribution to report)" >> "$OUT"
    FAILS="$FAILS W6-NO-QCLK"
  fi
  want_cad="$(arm_cadence "$cell" "$arm")"
  shipped="$(cell_shipped_floor "$cell")"
  # `below=1` iff the MAXIMUM realized refresh is under the cell's SHIPPED
  # effective floor. THE MAXIMUM, not the median: one refresh at or above the
  # old floor is enough to say the sub-floor region was not entered, and a
  # median-based reading would call a half-lifted arm lifted.
  # `below=-` means NO READING, which is not the same as `not below` and must
  # never be pooled with one.
  if [ "$qmax" -lt 0 ]; then below="-"; else
    below=$(awk -v m="$qmax" -v f="$shipped" 'BEGIN{print (m<f)?1:0}')
  fi
  # THIS IS A RESULT LINE AND NEVER AN ABORT. If F1 fires -- the commanded F
  # did not produce a sub-floor cadence -- every other clause is VOID, but the
  # row that shows it is the row that NAMES THE NEXT CADENCE IN THE CHAIN, from
  # the same code rather than from a guess. Deleting the invocation would
  # delete exactly that evidence.
  echo "CADENCE $cell-$arm want=$want_cad p50=$qp50 min=$qmin max=$qmax floor_shipped=$shipped below=$below" >> "$OUT"

  # -- W7: THE FIRE CLASSIFICATION, AND ITS OWN ACCOUNTING IDENTITY --------
  # n == timer + gap_data + gap_refresh + other is the classification's own
  # closure: every counted fire lands in exactly one class. A residue means the
  # classes do not partition the fires on THIS binary and no `rpd` taken
  # through them is a count of anything nameable.
  local fcn fctimer fcgap fcgapr fcother fcunattr
  fcn=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o ' n=[0-9]*' | tr -dc '0-9'); fcn="${fcn:-0}"
  fctimer=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'timer=[0-9]*' | tr -dc '0-9'); fctimer="${fctimer:-0}"
  fcgap=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'gap_data=[0-9]*' | tr -dc '0-9'); fcgap="${fcgap:-0}"
  fcgapr=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'gap_refresh=[0-9]*' | tr -dc '0-9'); fcgapr="${fcgapr:-0}"
  fcother=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'other=[0-9]*' | tr -dc '0-9'); fcother="${fcother:-0}"
  fcunattr=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o 'unattr=[0-9]*' | tr -dc '0-9'); fcunattr="${fcunattr:-0}"
  if [ -z "$(grep '\[FCAUSE\]' "$C" 2>/dev/null)" ]; then
    echo "W7-NO-FCAUSE $name rep=$REP (no [FCAUSE] at the sender)" >> "$OUT"
    FAILS="$FAILS W7-NO-FCAUSE"
  elif [ "$fcn" -ne "$(( fctimer + fcgap + fcgapr + fcother ))" ]; then
    echo "W7-NO-FCAUSE $name rep=$REP n=$fcn != timer=$fctimer + gap_data=$fcgap + gap_refresh=$fcgapr + other=$fcother" >> "$OUT"
    FAILS="$FAILS W7-NO-FCAUSE"
  fi

  # -- W8: THE MEASURAND, AT THE RECEIVER ----------------------------------
  # [SUCC] times HOLES where [FCAUSE] classifies FIRES: two counters over the
  # same underlying loss, bumped by different code at different events and at
  # different ENDPOINTS. `det` is `rpd`'s denominator; without it the wiring
  # statistic has nothing to divide by.
  local scdet scres scof
  scdet=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' det=[0-9]*' | tr -dc '0-9'); scdet="${scdet:-0}"
  scres=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' res=[0-9]*' | tr -dc '0-9'); scres="${scres:-0}"
  scof=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o 'orig_frac=[^ ]*' | tail -1 | sed 's/^orig_frac=//'); scof="${scof:-none}"
  echo "W5SUCC $name rep=$REP det=$scdet res=$scres orig_frac=$scof selfheal_p50_us=$(cell_selfheal_p50_us "$cell")" >> "$OUT"
  [ "$scdet" -eq 0 ] \
    && { echo "W8-NO-SUCC $name rep=$REP det=$scdet res=$scres" >> "$OUT"; FAILS="$FAILS W8-NO-SUCC"; }

  # -- W9: the realized-false-repair read must be LIVE ---------------------
  # P-A is about repair COUNT and explicitly does NOT re-litigate `fa ⊥ T`, but
  # the FRACTION is reported alongside the count precisely so that a moved
  # count can never later be reported as a moved fraction. Its COUNT is carried
  # in the row as well as its fraction: a fraction over few fires is a
  # rendering of noise and the denominator is what says so.
  local rfa_fires rfa_false rfa_ff
  rfa_fires=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'fires=[0-9]*' | tr -dc '0-9'); rfa_fires="${rfa_fires:-0}"
  rfa_false=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o ' false=[0-9]*' | tr -dc '0-9'); rfa_false="${rfa_false:-0}"
  rfa_ff=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'false_frac=[^ ]*' | tail -1 | sed 's/^false_frac=//'); rfa_ff="${rfa_ff:-none}"
  [ -z "$(grep '\[RFA\]' "$S" 2>/dev/null)" ] \
    && { echo "W9-NO-RFA $name rep=$REP" >> "$OUT"; FAILS="$FAILS W9-NO-RFA"; }

  # -- W10: the invocation itself ------------------------------------------
  local mb
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  { [ "$RC" -ne 0 ] || [ "$mb" = "0" ]; } \
    && { echo "W10-RC $name rep=$REP rc=$RC mean_mbps=$mb" >> "$OUT"; FAILS="$FAILS W10-RC"; }

  # THE WIRING TEST'S STATISTIC, printed on its own line so it is auditable
  # from the ledger alone. CROSS-ENDPOINT ON PURPOSE: numerator SENDER,
  # denominator RECEIVER. P-A's shape is stated over THIS number.
  echo "RPD $cell-$arm rpd=$(awk -v a="$fcn" -v b="$scdet" 'BEGIN{print (b>0)? a/b : "-"}') sup_frac=$(awk -v a="$hsup" -v b="$hev" 'BEGIN{print (b>0)? a/b : "-"}') rep=$REP fcause_n=$fcn succ_det=$scdet" >> "$OUT"

  # The verbatim gauge dump -- every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone. [HOLD] and [QCLK]
  # are in this loop AS WELL AS in their witness blocks above: the blocks print
  # the ONE line each witness was read from, the loop prints EVERY line,
  # including the per-path rows the report needs and the unattributed bucket it
  # must not pool.
  local f
  for f in HOLD QCLK FCAUSE SUCC RFA LCW WALL; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # -- ARM-VOID. A RESULT, NEVER AN ABORT. ---------------------------------
  # `law_n` counts the evaluations at which the arm's OWN hold-down window was
  # full. `law_n = 0` on an armed arm means the window never filled and the arm
  # ran the SHIPPED behaviour at every fire: the row is VOID for scoring and is
  # REPORTED as such, because deleting the invocation would delete the very
  # evidence that the window did not fill.
  [ "$arm" != "CTL" ] && [ "$hlaw" -eq 0 ] \
    && echo "ARM-VOID $cell-$arm rep=$REP (HOLD-LAW-DEAD: law_n=0 evals=$hev n_req=$hn -- the window never filled; row VOID for scoring, REPORTED as the UNSCOREABLE rule)" >> "$OUT"

  # -- THE BAND, AND ITS SCOPE ---------------------------------------------
  local lo hi lossy inband applies
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  # BANDSCOPE: CTL ONLY. On a TREATMENT arm an out-of-band reading is a RESULT
  # -- it is P-B, the prediction the band would otherwise abort before it could
  # be read -- and `band_applies` says so IN THE ROW rather than in a footnote.
  applies=0; [ "$arm" = "CTL" ] && applies=1

  echo "QREFWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mean_mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"q_expect\":\"$q_exp\",\"q_got\":\"$q_got\",\"floor_expect\":\"$floor_exp\",\"floor_got\":\"$floor_got\",\"cadence_want\":$want_cad,\"cadence_p50\":$qp50,\"cadence_min\":$qmin,\"cadence_max\":$qmax,\"cadence_below\":\"$below\",\"cadence_floor_shipped\":$shipped,\"qclk_evals\":$qev,\"qclk_kept\":$qkept,\"hold_law_n\":$hlaw,\"hold_evals\":$hev,\"hold_sup\":$hsup,\"hold_emit\":$hemit,\"hold_t_us\":\"$htus\",\"hold_hd_p50_us\":\"$hd50\",\"hold_n_req\":\"$hn\",\"fcause_n\":$fcn,\"fcause_timer\":$fctimer,\"fcause_gap_data\":$fcgap,\"fcause_gap_refresh\":$fcgapr,\"fcause_other\":$fcother,\"fcause_unattr\":$fcunattr,\"succ_det\":$scdet,\"succ_res\":$scres,\"succ_orig_frac\":\"$scof\",\"selfheal_p50_us\":$(cell_selfheal_p50_us "$cell"),\"rfa_fires\":$rfa_fires,\"rfa_false\":$rfa_false,\"rfa_false_frac\":\"$rfa_ff\",\"fails\":\"${FAILS# }\"}" \
    | tee -a "$WIT" >> "$OUT"
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs ca cb mode bytes alpha
  envs="$(arm_env "$cell" "$arm")"
  alpha="$(arm_alpha "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name q=$(arm_q "$arm") n_req=$(arm_nreq "$arm") F=$(arm_floor "$cell" "$arm") cadence_want=$(arm_cadence "$cell" "$arm") seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
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
  case " $QR_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $QR_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== QREFRESH BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate \"THE (q, refresh) SWEEP -- PRE-REGISTRATION\", and nothing else. Paper 16.78 is the derivation."
  echo "CELLS $QR_CELLS   (c8/c8L NOT run: the successor-arrival pass recorded their quantiles NOT USABLE as derivation inputs -- rep dispersion up to 52x -- and a cell whose p50 cannot be trusted cannot supply a p50/2)"
  echo "ARMS  $QR_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $QR_ARMS; do echo "ARMENV $A q=$(arm_q "$A") n_req=$(arm_nreq "$A")"; done
  echo "AXIS1 RWM_HOLDDOWN_Q, ABSENT by default; GARBAGE and any q outside (0,1) resolve back to ABSENT and print unset"
  echo "AXIS2 RWM_REFRESH_FLOOR_US = F in (2*srtt).clamp(F, 4F); ABSENT => F = HOLE_NACK_REFRESH_MIN = 25 ms and the engine is BYTE-IDENTICAL. R=4 is the SHIPPED clamp's own aspect ratio, not a new constant."
  echo "GRID  the TRANSCRIBED (cell, arm) -> F / delivered-cadence table (paper 16.78.3); the DELIVERED CADENCE is the treatment, F is only its input:"
  for C in $QR_CELLS; do
    echo "GRID  $C selfheal_p50_us=$(cell_selfheal_p50_us "$C") shipped_floor_us=$(cell_shipped_floor "$C") band=[$(band_lo "$C"),$(band_hi "$C")] paths=$(cell_paths "$C")"
    for A in $QR_ARMS; do
      echo "GRID  $C-$A F=$(arm_floor "$C" "$A") cadence_want_us=$(arm_cadence "$C" "$A") | env=\"$(arm_env "$C" "$A")\""
    done
  done
  echo "F1    THE WIRING WITNESS, SCORED FIRST: the CADENCE lines. below=1 iff [QCLK] site=receiver w_us_max < the cell's shipped effective floor. It is a RESULT line and NEVER an abort -- if F1 fires every other clause is VOID and this row names the next cadence in the chain."
  echo "TIMER the SHIPPED [25,100] ms clamp is the SENDER's timer on EVERY row: RWM_QUANTILE_CLOCKS/RWM_RACK_CLOCKS/RWM_DERIVED_SWEEP are CONTAMINATION gates and RWM_W_FORM must resolve cantelli"
  echo "F3    a delivered cadence below TAIL_SWEEP_MIN_US = 25 ms puts the RECEIVER's report clock AHEAD of the SENDER's backstop for the first time in this engine's history -- a stated confound on the sub-floor arms, carried on every reading taken from them"
  echo "RPD   the wiring test statistic, CROSS-ENDPOINT: [FCAUSE] n (sender) / [SUCC] det (receiver)"
  echo "ARM-VOID (law_n=0) is a RESULT and never an abort"
  echo "BANDSCOPE the goodput abort bands apply to CTL ONLY (band_applies=1); an out-of-band TREATMENT reading is a RESULT -- it is 16.78.4's P-B"
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
  for CELL in $QR_CELLS; do
    for ARM in $QR_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — live n is law_n>0; the PRE-REGISTRATION applies the bars) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $QR_CELLS; do
  for A in $QR_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    # Counted against THIS driver's OWN row format, byte for byte, with -F so
    # the braces and quotes are literals. The hold-down battery counted the
    # PARSER's rendering of a similar row instead, which is a second dialect of
    # the same fact: a driver-side row-format change would have read zero rows
    # and reported ARM-VANISHED at every arm.
    N=$(grep -cF "QREFWITNESS {\"cell\":\"$CELL\",\"arm\":\"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "QREFRESH-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo QREFRESH-BATTERY-DONE-$SEED_ARG
