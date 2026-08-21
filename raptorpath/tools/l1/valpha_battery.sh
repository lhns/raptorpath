#!/bin/bash
# THE VERDICT-ALPHA BATTERY — goal #101 item 4, run against THE DERIVED-α CLOCK.
#
#   sudo bash valpha_battery.sh <seed> <reps>
#
# WHY THIS EXISTS AND WHAT IS DIFFERENT ABOUT IT. The verdict battery of
# 2026-08-21 held item 4's contest against the HOLD-DOWN challenger — `c7`-`R2Q86`,
# the strongest configuration the RECORD contained. That was correct, it was
# labelled as such, and it returned `SURVIVE ON MERIT` at 0 of 3 stable cells.
# **But item 4's condition says "the derived clock vs shipped clamp ... realized
# false-repair vs the derived alpha", and `R2Q86` is not a member of the
# derived-α family at all.** So the family stayed closed only BY ITS CURVE.
#
# THIS BATTERY ADJUDICATES THE DERIVED-α CLOCK ITSELF. Paper §16.79.7 names the
# member: `W_q(α*)` at the measured boundary optimum `α* = 0.40` — the MAXIMAL
# NON-NULL member of the family, the last non-degenerate point before the family
# collapses into the incumbent (the corner `T* = 0`, §16.79.0).
#
#   CTL   all three gates ABSENT -> byte-identical shipped machine, the
#         (2*srtt).clamp(25,100) ms timer, `RWM_W_FORM` resolving `cantelli`
#         and `RWM_ALPHA_OVERRIDE` resolving `unset`
#   Q400  RWM_QUANTILE_CLOCKS=1 RWM_W_FORM=quantile RWM_ALPHA_OVERRIDE=0.4
#         expected [QALPHA] win_n = 25
#
# THE AXES ARE SWAPPED RELATIVE TO verdict_battery.sh, AND THE SWAP IS THE WHOLE
# DIFF. There, `RWM_QUANTILE_CLOCKS`/`RWM_W_FORM`/`RWM_ALPHA_OVERRIDE` were
# CONTAMINATION gates that had to read OFF on every row, because the sender's
# timer had to be the shipped clamp. Here they are THE TREATMENT, and
# `RWM_HOLDDOWN_Q`/`RWM_REFRESH_FLOOR_US` — the previous battery's axes — are
# what must read ABSENT on every row. A row on which the OTHER battery's axis
# moved is not a row of this battery.
#
# `0.4`, NOT `0.40`. The [GATES] echo prints the RESOLVED f64 through Rust's own
# `to_string()`, which renders 0.40 as `0.4`, and the arm table is matched
# against that echo LITERALLY on purpose. The quantile-native smoke caught
# exactly this at 2 of 12 endpoint-checks (`got='...=0.4' want=0.40`) — the arm
# was live and correct and the HARNESS would have called it dead at every rep.
#
# `RWM_HOLDDOWN_Q` AND `RWM_REFRESH_FLOOR_US` ARE WORD GATES, NOT FLAGS, AND
# THEY ARE ASSERTED `unset` RATHER THAN `0`. They live in SUBSTRATE and not in
# CONTAM for exactly that reason: the contamination branch matches `<gate>=[01]`
# and would return the EMPTY STRING for every value of a gate that echoes
# `unset`, producing a liveness gate that passes because it never matched. That
# is the defect verdict_battery.sh's own comment records at its `pat` branch,
# and this is the same lesson applied to the mirrored axis.
#
# RWM_GEN=0 ON EVERY ROW, AND IT IS LOAD-BEARING. Under generation coding the
# SACK->gap producer is suppressed, so [FCAUSE]'s gap classes and [SUCC]'s orig
# are STRUCTURALLY EMPTY and this battery's whole measurand would read zero for
# a configuration reason. W4 asserts `gen=0` off the MEASURAND'S OWN GAUGES
# ([FCAUSE], [SUCC], [RFA]) rather than off the [GATES] echo: `RWM_GEN` is the
# generation SIZE IN SYMBOLS, so the engine echoes its default `RWM_GEN=384`
# even when the invocation passed 0. MEASURED in the verdict battery's
# calibration, where a check against the echo fired W4-CONTAM at 6 of 6
# invocations while generation coding was in fact OFF at 6 of 6.
#
# WITNESSES, per invocation (the pre-registration's numbering):
#
#   W1  [GATES] on BOTH endpoints. Neither => ABORT (no datum, no denominator).
#   W2  RWM_ALPHA_OVERRIDE RESOLVED == the arm's own α, LITERALLY, on BOTH
#       endpoints.                                        W2-ALPHA-MISMATCH
#   W3  RWM_W_FORM RESOLVED == the arm's form AND RWM_QUANTILE_CLOCKS ==
#       the arm's flag, on BOTH endpoints.                W3-FORM-MISMATCH
#   W4  the OTHER battery's axes ABSENT, every contamination gate 0, and gen=0
#       at [FCAUSE]/[SUCC]/[RFA].                         W4-CONTAM
#   W5  [QALPHA] present at BOTH endpoints with alpha=, form= and win_n=.
#       win_n ASSERTED at the sender ALWAYS; at the receiver on the Q arm ONLY
#       — the protocol hint is not plumbed to the receiver task, so an
#       UNOVERRIDDEN CTL receiver resolves a DIFFERENT contract α and therefore
#       a different window. On a Q arm BOTH sites carry a NUMBER override — an
#       override is a number and not a hint mapping — so both are asserted.
#       This asymmetry is a documented property of the engine, measured by the
#       quantile-native sweep, and NOT a softened gate.  W5-NO-QALPHA
#   W6  [QCLK] site=sender present with evals > 0; `law_n` REPORTED.
#                                                         W6-NO-QCLK
#   W7  [FCAUSE] present and n == timer + gap_data + gap_refresh + other.
#                                                         W7-NO-FCAUSE
#   W8  [SUCC] present at the receiver with det > 0.      W8-NO-SUCC
#   W9  [RFA] present at the receiver; false_frac AND its COUNT both scraped.
#                                                         W9-NO-RFA
#   W10 rc == 0 and mean_mbps scraped.                    W10-RC
#
# THE WIRING WITNESS, AND IT IS SCORED FIRST. `F1` here is the DERIVED-α
# analogue of the (q, refresh) sweep's cadence witness: the treatment is the
# REALIZED CLOCK, not the commanded α, and the battery WITNESSES it instead of
# assuming it — off the SENDER's own `[QCLK] site=sender` line, the seat that
# evaluates the recovery clock:
#
#     WCLOCK <cell>-<arm> law_n=.. evals=.. win_ok=.. win_n=.. p50=.. p05=.. p95=..
#
# LAW-DEAD IS A RESULT, NOT AN ABORT. `law_n = 0` on the armed arm means the
# quantile window never filled and the arm ran the SHIPPED behaviour at every
# fire; the row is VOID for scoring and is REPORTED as `ARM-VOID`. Deleting the
# invocation would delete exactly the evidence that the window did not fill.
#
# WINDOW-PARTIAL IS A RESULT AND NEVER AN ABORT either. `win_ok < evals` means
# the arm spent part of the run below its own window and THE FRACTION IS THE
# THING TO SCORE. The quantile-native sweep read 0.829-0.999 fill at the Q400
# arm across five cells and excluded nothing.
#
# BANDSCOPE: the goodput abort bands apply to CTL ONLY. On the treatment arm an
# out-of-band reading is a RESULT — it is the guard's `G1` reading, the whole
# point of the guard — and `band_applies` says so IN THE ROW rather than in a
# footnote. An out-of-band CHALLENGER may never be re-labelled an abort to keep
# the incumbent's table tidy, and an out-of-band CHALLENGER WIN may never be
# re-labelled one either.
#
# CELLS AND BANDS ARE THE VERDICT BATTERY'S, TRANSCRIBED AND NEVER REDEFINED, so
# these rows POOL with it and with the quantile-native sweep. c8 is the
# LOSS-HEAVY WITNESS cell and DOES NOT VOTE. c8L is not run: 52x dispersion.
#
# THE PARSER IS alpha_parse.py, NOT A FORK OF IT — so this battery's rows POOL
# with the alpha-sweep, quantile-native, hold-down and verdict ledgers.
#
# NOTHING HERE FLIPS A DEFAULT. RWM_QUANTILE_CLOCKS, RWM_W_FORM and
# RWM_ALPHA_OVERRIDE are all ABSENT/OFF by default and nothing shipped reads any
# of them.
#
# WATCHER NOTE: `pgrep -f valpha_battery.sh` matches the WATCHER'S OWN shell
# whenever its command line contains the string. Watch the SENTINEL, or the
# ledger's VALPHA-BATTERY-DONE line — never the process table (discipline 13).

set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-4}"
VA_CELLS="${RWM_VA_CELLS:-c1 c7 sc2 c8}"
VA_ARMS="${RWM_VA_ARMS:-CTL Q400}"
TAG="${RWM_VA_TAG:-va}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/valpha
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
WIT="$OUTDIR/${TAG}-witness-s${SEED_ARG}.jsonl"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var that is
# exported in this process reaches the binary whatever the forward list says, so
# the CONTROL arm's "absent" can only be made absent by unsetting it HERE.
# `RWM_ALPHA_OVERRIDE` and `RWM_W_FORM` are on this list because ABSENT is the
# state in which the sender's timer is the byte-identical shipped clamp, and an
# inherited value would make CTL a quantile-clock configuration silently sharing
# a ledger with the control it is supposed to BE. `RWM_HOLDDOWN_Q` and
# `RWM_REFRESH_FLOOR_US` are on it because they are the PREVIOUS battery's axes
# and an inherited one would make every row of this battery a two-axis
# configuration wearing a one-axis name.
unset RWM_ALPHA_OVERRIDE RWM_QUANTILE_CLOCKS RWM_W_FORM \
      RWM_HOLDDOWN_Q RWM_REFRESH_FLOOR_US RWM_RTT_DUMP RWM_SUCC_DUMP

# ── THE ARM TABLES ───────────────────────────────────────────────────────
# THREE PARALLEL TABLES over the SAME arm names: the arm's α, the arm's window
# form, and the arm's window-size expectation. They are written adjacent, in one
# block, because the failure mode they exist to prevent is DRIFT between them —
# an arm whose env says one thing and whose expectation says another produces a
# harness that calls a live arm dead, or a dead arm live, and both readings look
# like data.
#
# TRANSCRIBED FROM qnat_battery.sh, NOT RE-DERIVED. This is the EXACT machinery
# the quantile-native sweep ran, at the EXACT arm that sweep's reading (iii)
# selected. A battery that re-derived the arm would be adjudicating a different
# clock from the one §16.79.7 names.
arm_alpha() { # arm -> α, or "unset" for the control
  case "$1" in
    Q400) echo 0.4 ;;     # `0.4` NOT `0.40` — Rust's own to_string(). See header.
    CTL)  echo unset ;;
    *)    echo "" ;;
  esac
}

# The arm's `RWM_W_FORM` ENV value. This is NOT the same string as the EXPECTED
# ECHO: absent resolves to `cantelli` at the binary, so `gate_expect` expects
# `cantelli` at CTL while `arm_env` emits no token at all.
arm_form() { # arm -> RWM_W_FORM env value, or "unset" for the control
  case "$1" in
    CTL)  echo unset ;;
    Q400) echo quantile ;;
    *)    echo "" ;;
  esac
}

# THE ARM'S EXPECTED `win_n`. TRANSCRIBED, not computed: the window size is a
# property of the arm's α that the ENGINE derives, and a harness that re-derived
# it would agree with the engine BY CONSTRUCTION instead of checking it. `25` is
# the quantile-native pre-registration's own N(α) column at α = 0.40 and is
# pinned in that battery's §3 window-fill table.
arm_winn() { # arm -> expected [QALPHA]/[QCLK] win_n token
  case "$1" in
    Q400) echo 25 ;;
    CTL)  echo unavail ;;
    *)    echo "" ;;
  esac
}

VA_ARM_GATES="RWM_QUANTILE_CLOCKS RWM_ALPHA_OVERRIDE RWM_W_FORM"
# RWM_DELTA_CAP is shipped-ON since §16.71 and is the SUBSTRATE this battery
# runs on, not an axis of it: same value on every arm, asserted =1 rather than
# assumed, because a reader who takes the pre-flip default mis-scales every
# queue number in the result.
#
# RWM_HOLDDOWN_Q and RWM_REFRESH_FLOOR_US ARE IN SUBSTRATE AND NOT IN CONTAM,
# AND THE PLACEMENT IS THE POINT. They are the PREVIOUS battery's two axes and
# they must be ABSENT on every row here. But they are WORD gates — they echo
# `unset`, never `0` — so the CONTAM branch's `<gate>=[01]` pattern would match
# nothing and pass vacuously. In SUBSTRATE they are asserted LITERALLY against
# `unset`, two-sided, which is the only reading that settles the axis.
VA_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP RWM_HOLDDOWN_Q RWM_REFRESH_FLOOR_US"
VA_CONTAM_GATES="RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP RWM_SUCC_DUMP"

gate_expect() { # arm gate -> expected [GATES] value (the RESOLVED echo)
  case "$2" in
    RWM_QUANTILE_CLOCKS) case "$1" in CTL) echo 0 ;; *) echo 1 ;; esac ;;
    RWM_ALPHA_OVERRIDE)  arm_alpha "$1" ;;
    # ABSENT resolves to `cantelli` at the binary — the echo prints what the
    # engine RESOLVED, which is the only reading that settles the axis.
    RWM_W_FORM)          case "$1" in CTL) echo cantelli ;; *) echo quantile ;; esac ;;
    RWM_DELTA_CAP)       echo 1 ;;
    RWM_SUM_CAP)         echo 1 ;;
    # THE PREVIOUS BATTERY'S AXES. ABSENT on EVERY arm of this one.
    RWM_HOLDDOWN_Q)       echo unset ;;
    RWM_REFRESH_FLOOR_US) echo unset ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the tables above. `unset` is an ABSENCE, not a
# value: an arm whose expectation is `unset` gets NO token at all, because
# absent is exactly the state whose RESOLUTION the [GATES] echo reads back. CTL
# therefore gets neither an RWM_ALPHA_OVERRIDE nor an RWM_W_FORM token, and NO
# arm gets an RWM_HOLDDOWN_Q or RWM_REFRESH_FLOOR_US token.
arm_env() { # arm -> "RWM_X=v ..."
  local a="$1" g out="" v
  for g in $VA_ARM_GATES $VA_SUBSTRATE_GATES $VA_CONTAM_GATES; do
    case "$g" in
      RWM_ALPHA_OVERRIDE) v="$(arm_alpha "$a")" ;;
      RWM_W_FORM)         v="$(arm_form "$a")" ;;
      *)                  v="$(gate_expect "$a" "$g")" ;;
    esac
    [ "$v" = "unset" ] && continue
    out="$out $g=$v"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# verdict_battery.sh:391-406, itself from hold_battery.sh via ccand_battery.sh,
# never redefined: a cell that differs from the ledger's cell is a different
# cell and its rows do not pool.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    # c8 -- the LOSS-HEAVY WITNESS CELL. Its [SUCC] orig p50 carries 3.20x rep
    # dispersion and the successor-arrival pass recorded it NOT USABLE as a
    # derivation input. It is a WITNESS row, never a vote. Note that unlike the
    # verdict battery, THIS battery derives nothing from c8's p50 -- α = 0.40 is
    # a contract quantity and not a cell-scaled one -- so c8's caveat here is
    # about its DISPERSION alone and not about its treatment being ill-defined.
    c8)  echo "c2 c3 dual   25000000" ;;
    *) echo "" ;;
  esac
}
cell_paths() { case "$1" in c7|c8) echo 2 ;; *) echo 1 ;; esac; }

# The verdict battery's bands, unchanged. CTL ONLY — see BANDSCOPE.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; sc2) echo 78;; c8) echo 50;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; sc2) echo 92;; c8) echo 100;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

# THE CELL'S SHIPPED EFFECTIVE CLOCK — whichever rail of the shipped
# [25,100] ms clamp binds at this cell (§16.78.0's table). It is the CTL arm's
# expected realized `[QCLK] w_us_p50`, and the smoke asserts it: an inherited
# override would make CTL a quantile-clock configuration and this is the reading
# that settles it.
cell_shipped_clock() { case "$1" in c1) echo 25000 ;; *) echo 100000 ;; esac; }

arm_cell_reps() { echo "$REPS"; }

check_and_parse() { # name cell arm alpha cpus cpuc pingp qp
  local name="$1" cell="$2" arm="$3" alpha="$4" cpus="$5" cpuc="$6" pingp="$7" qp="$8"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log
  local FAILS=""     # the witness fail tokens, accumulated and carried in the row

  # THE PARSER IS `alpha_parse.py`, NOT A FORK OF IT, so this battery's rows
  # POOL with the α-sweep, quantile-native, hold-down and verdict ledgers
  # instead of speaking a second dialect of them.
  python3 ./alpha_parse.py "$cell" "$arm" "$alpha" "$SEED_ARG" "$REP" \
      "$C" "$S" "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "VA-PARSE-FAIL $name rep=$REP" >> "$OUT"

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
  for g in $VA_ARM_GATES $VA_SUBSTRATE_GATES $VA_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    case "$g" in
      # A WORD, NOT A FLAG. `RWM_W_FORM` echoes `cantelli` or `quantile`,
      # `RWM_ALPHA_OVERRIDE` echoes `unset` or a number, and
      # `RWM_HOLDDOWN_Q`/`RWM_REFRESH_FLOOR_US` echo `unset` or a value;
      # matching any of them as `[01]` would return the empty string for EVERY
      # value and produce a liveness gate that passes because it never matched.
      RWM_ALPHA_OVERRIDE|RWM_W_FORM|RWM_HOLDDOWN_Q|RWM_REFRESH_FLOOR_US) pat="$g=[^ ]*" ;;
      *)                                                                 pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $VA_ARM_GATES $VA_SUBSTRATE_GATES " in
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
  # The loop above emits per-gate liveness lines; W2 and W3 are the gates that
  # ARE the treatment, so they get their own tokens in the row. Both are LITERAL
  # comparisons against the RESOLVED echo — the echo prints what the engine
  # DECIDED, and a mistyped α resolves back to `unset` and is READ rather than
  # inferred.
  local a_exp a_got a_got_s f_exp f_got f_got_s qc_got qc_got_s qc_exp
  a_exp="$(arm_alpha "$arm")"
  f_exp="$(gate_expect "$arm" RWM_W_FORM)"
  qc_exp="$(gate_expect "$arm" RWM_QUANTILE_CLOCKS)"
  a_got=$(printf '%s' "$gl_c" | grep -o 'RWM_ALPHA_OVERRIDE=[^ ]*' | sed 's/^RWM_ALPHA_OVERRIDE=//'); a_got="${a_got:-none}"
  a_got_s=$(printf '%s' "$gl_s" | grep -o 'RWM_ALPHA_OVERRIDE=[^ ]*' | sed 's/^RWM_ALPHA_OVERRIDE=//'); a_got_s="${a_got_s:-none}"
  f_got=$(printf '%s' "$gl_c" | grep -o 'RWM_W_FORM=[^ ]*' | sed 's/^RWM_W_FORM=//'); f_got="${f_got:-none}"
  f_got_s=$(printf '%s' "$gl_s" | grep -o 'RWM_W_FORM=[^ ]*' | sed 's/^RWM_W_FORM=//'); f_got_s="${f_got_s:-none}"
  qc_got=$(printf '%s' "$gl_c" | grep -o 'RWM_QUANTILE_CLOCKS=[01]' | sed 's/^RWM_QUANTILE_CLOCKS=//'); qc_got="${qc_got:-none}"
  qc_got_s=$(printf '%s' "$gl_s" | grep -o 'RWM_QUANTILE_CLOCKS=[01]' | sed 's/^RWM_QUANTILE_CLOCKS=//'); qc_got_s="${qc_got_s:-none}"
  # BOTH ENDPOINTS. The clock is evaluated at the SENDER and the false-repair
  # ledger is stamped at the RECEIVER, so a gate live at one endpoint and dead
  # at the other is a HALF-ARMED ARM — the configuration most likely to produce
  # a plausible, wrong number.
  { [ "$a_got" != "$a_exp" ] || [ "$a_got_s" != "$a_exp" ]; } \
    && { echo "W2-ALPHA-MISMATCH $name rep=$REP cli='$a_got' srv='$a_got_s' exp=$a_exp" >> "$OUT"; FAILS="$FAILS W2-ALPHA-MISMATCH"; }
  { [ "$f_got" != "$f_exp" ] || [ "$f_got_s" != "$f_exp" ] \
    || [ "$qc_got" != "$qc_exp" ] || [ "$qc_got_s" != "$qc_exp" ]; } \
    && { echo "W3-FORM-MISMATCH $name rep=$REP form cli='$f_got' srv='$f_got_s' exp=$f_exp | qclocks cli='$qc_got' srv='$qc_got_s' exp=$qc_exp" >> "$OUT"; FAILS="$FAILS W3-FORM-MISMATCH"; }

  # -- W4: GENERATION CODING IS OFF, READ OFF THE GAUGES' OWN `gen=` --------
  # READ FROM THE GAUGE LINES AND *NOT* FROM `[GATES] RWM_GEN=`. `RWM_GEN` is
  # the generation SIZE IN SYMBOLS, not a flag: the invocation passes
  # `RWM_GEN=0` meaning "no generation coding", and the engine resolves and
  # echoes its own default `RWM_GEN=384` regardless. MEASURED in the verdict
  # battery's calibration, where a check against the echo fired W4-CONTAM at
  # 6 of 6 invocations while generation coding was in fact OFF at 6 of 6.
  local gen_f gen_u gen_r
  gen_f=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | tail -1 | grep -o ' gen=[0-9]*' | sed 's/^ gen=//'); gen_f="${gen_f:-none}"
  gen_u=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' gen=[0-9]*' | sed 's/^ gen=//'); gen_u="${gen_u:-none}"
  gen_r=$(grep "\[RFA\]" "$S" 2>/dev/null | tail -1 | grep -o 'gen=[0-9]*' | sed 's/^gen=//'); gen_r="${gen_r:-none}"
  { [ "$gen_f" != "0" ] || [ "$gen_u" != "0" ] || [ "$gen_r" != "0" ]; } \
    && { echo "W4-CONTAM $name rep=$REP gen FCAUSE='$gen_f' SUCC='$gen_u' RFA='$gen_r' exp=0 (the gap-report path is structurally empty under generation coding)" >> "$OUT"; FAILS="$FAILS W4-CONTAM"; }

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

  # -- W5: THE RESOLVED α AND FORM AT THE LAW, BOTH ENDPOINTS ---------------
  # [GATES] can only say what was ASKED FOR. [QALPHA] says what the LAW
  # RESOLVED, at the sites that resolve it. Read INDEPENDENTLY OF THE PARSER —
  # the parser's regex is the thing a token change would break first, so the
  # driver reads raw tokens and the report reads parsed columns; two independent
  # readings of the same reachability fact.
  local qa_c qa_s wf_c wf_s wn_c wn_s qa_a_c qa_a_s want_winn
  qa_c=$(grep "\[QALPHA\] site=sender" "$C" 2>/dev/null | tail -1)
  qa_s=$(grep "\[QALPHA\] site=receiver" "$S" 2>/dev/null | tail -1)
  want_winn="$(arm_winn "$arm")"
  wf_c=$(printf '%s' "$qa_c" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//'); wf_c="${wf_c:-none}"
  wf_s=$(printf '%s' "$qa_s" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//'); wf_s="${wf_s:-none}"
  wn_c=$(printf '%s' "$qa_c" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//'); wn_c="${wn_c:-none}"
  wn_s=$(printf '%s' "$qa_s" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//'); wn_s="${wn_s:-none}"
  qa_a_c=$(printf '%s' "$qa_c" | grep -o 'alpha=[^ ]*' | tail -1 | sed 's/^alpha=//'); qa_a_c="${qa_a_c:-none}"
  qa_a_s=$(printf '%s' "$qa_s" | grep -o 'alpha=[^ ]*' | tail -1 | sed 's/^alpha=//'); qa_a_s="${qa_a_s:-none}"
  { [ -z "$qa_c" ] || [ -z "$qa_s" ]; } \
    && { echo "W5-NO-QALPHA $name rep=$REP cli='${qa_c:-none}' srv='${qa_s:-none}'" >> "$OUT"; FAILS="$FAILS W5-NO-QALPHA"; }
  echo "W5QALPHA $name rep=$REP cli=form:$wf_c/win_n:$wn_c/alpha:$qa_a_c srv=form:$wf_s/win_n:$wn_s/alpha:$qa_a_s (exp form=$f_exp win_n=$want_winn)" >> "$OUT"
  [ "$wf_c" != "$f_exp" ] \
    && { echo "W5-QFORM-FAIL-CLI $name rep=$REP got=$wf_c exp=$f_exp" >> "$OUT"; FAILS="$FAILS W5-NO-QALPHA"; }
  [ "$wf_s" != "$f_exp" ] \
    && { echo "W5-QFORM-FAIL-SRV $name rep=$REP got=$wf_s exp=$f_exp" >> "$OUT"; FAILS="$FAILS W5-NO-QALPHA"; }
  # THE SENDER IS ALWAYS ASSERTED. The RECEIVER is asserted ONLY on the Q arm:
  # the protocol hint is not plumbed to the receiver task, so an UNOVERRIDDEN
  # CTL receiver resolves a DIFFERENT contract α and therefore a different
  # window size. On the Q arm both sites carry a NUMBER override — an override
  # is a number and not a hint mapping — so both must read the expected win_n
  # and both are checked. This asymmetry is a documented property of the engine
  # (quantile-native sweep, W7) and NOT a softened gate.
  [ "$wn_c" != "$want_winn" ] \
    && { echo "W5-QWINN-FAIL-CLI $name rep=$REP got=$wn_c exp=$want_winn" >> "$OUT"; FAILS="$FAILS W5-NO-QALPHA"; }
  { [ "$arm" != "CTL" ] && [ "$wn_s" != "$want_winn" ]; } \
    && { echo "W5-QWINN-FAIL-SRV $name rep=$REP got=$wn_s exp=$want_winn" >> "$OUT"; FAILS="$FAILS W5-NO-QALPHA"; }

  # ── W6 + THE WIRING WITNESS ──────────────────────────────────────────────
  # THE REALIZED CLOCK, NOT THE COMMANDED α, IS THE TREATMENT. `[QCLK]
  # site=sender` is the SENDER's own reading of the recovery clock it evaluates
  # — the seat that owns the law this battery arms — and its `w_us_*`
  # distribution is the only place the override can be seen to have LANDED
  # rather than merely been requested.
  local qcl qlaw qev qwok qwn qp50 qp05 qp95
  qcl=$(grep "\[QCLK\] site=sender" "$C" 2>/dev/null | tail -1)
  qlaw=$(printf '%s' "$qcl" | grep -o 'law_n=[0-9]*' | tail -1 | sed 's/^law_n=//'); qlaw="${qlaw:-0}"
  qev=$(printf '%s' "$qcl" | grep -o ' evals=[0-9]*' | tail -1 | tr -dc '0-9'); qev="${qev:-0}"
  qwok=$(printf '%s' "$qcl" | grep -o 'win_ok=[0-9]*' | tail -1 | sed 's/^win_ok=//'); qwok="${qwok:-0}"
  qwn=$(printf '%s' "$qcl" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//'); qwn="${qwn:-none}"
  # `sed 's/^KEY=//'` and NOT `tr -dc '0-9'`: the key `w_us_p50` CONTAINS the
  # digits `50`, and `tr -dc` strips non-digits from the WHOLE match, so
  # `w_us_p50=25000` reads back as `5025000`. MEASURED in the verdict battery's
  # own calibration, which is why that calibration exists. The same trap applies
  # to `w_us_p05` and `w_us_p95` and all three are read with `sed` here.
  qp50=$(printf '%s' "$qcl" | grep -o 'w_us_p50=[0-9]*' | tail -1 | sed 's/^w_us_p50=//'); qp50="${qp50:--1}"
  qp05=$(printf '%s' "$qcl" | grep -o 'w_us_p05=[0-9]*' | tail -1 | sed 's/^w_us_p05=//'); qp05="${qp05:--1}"
  qp95=$(printf '%s' "$qcl" | grep -o 'w_us_p95=[0-9]*' | tail -1 | sed 's/^w_us_p95=//'); qp95="${qp95:--1}"
  if [ -z "$qcl" ] || [ "$qev" -eq 0 ]; then
    echo "W6-NO-QCLK $name rep=$REP (no [QCLK] site=sender, or evals=$qev -- the seat that evaluates the clock has no distribution to report)" >> "$OUT"
    FAILS="$FAILS W6-NO-QCLK"
  fi
  # THIS IS A RESULT LINE AND NEVER AN ABORT.
  echo "WCLOCK $cell-$arm rep=$REP law_n=$qlaw evals=$qev win_ok=$qwok win_n=$qwn p50=$qp50 p05=$qp05 p95=$qp95 shipped_clock=$(cell_shipped_clock "$cell")" >> "$OUT"

  # -- W7: THE FIRE CLASSIFICATION, AND ITS OWN ACCOUNTING IDENTITY --------
  # n == timer + gap_data + gap_refresh + other is the classification's own
  # closure: every counted fire lands in exactly one class. A residue means the
  # classes do not partition the fires on THIS binary.
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

  # -- W8: THE HOLE LEDGER, AT THE RECEIVER --------------------------------
  local scdet scres scof
  scdet=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' det=[0-9]*' | tr -dc '0-9'); scdet="${scdet:-0}"
  scres=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o ' res=[0-9]*' | tr -dc '0-9'); scres="${scres:-0}"
  scof=$(grep "\[SUCC\]" "$S" 2>/dev/null | tail -1 | grep -o 'orig_frac=[^ ]*' | tail -1 | sed 's/^orig_frac=//'); scof="${scof:-none}"
  echo "W8SUCC $name rep=$REP det=$scdet res=$scres orig_frac=$scof" >> "$OUT"
  [ "$scdet" -eq 0 ] \
    && { echo "W8-NO-SUCC $name rep=$REP det=$scdet res=$scres" >> "$OUT"; FAILS="$FAILS W8-NO-SUCC"; }

  # -- W9: DIMENSION 2's OWN READ, AND IT MUST BE LIVE ----------------------
  # The FRACTION and the COUNT, both, always. A fraction over few fires is a
  # rendering of noise and the denominator is what says so; a moved count with
  # a level fraction is a READING and not a contradiction. Both are carried so
  # that neither can later be reported as the other.
  #
  # `fill_coded` is the SECOND-CENSOR term: the realized false-repair reading is
  # a BRACKET [false_frac, false/(false+fill_coded)] and not a point, and the
  # bracket is what §16.76-era batteries report at the shaped cells. Both ends
  # are scraped here so the scorer can widen or narrow honestly.
  local rfa_fires rfa_false rfa_ff rfa_fill
  rfa_fires=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'fires=[0-9]*' | tr -dc '0-9'); rfa_fires="${rfa_fires:-0}"
  rfa_false=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o ' false=[0-9]*' | tr -dc '0-9'); rfa_false="${rfa_false:-0}"
  rfa_ff=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'false_frac=[^ ]*' | tail -1 | sed 's/^false_frac=//'); rfa_ff="${rfa_ff:-none}"
  rfa_fill=$(grep '\[RFA\]' "$S" 2>/dev/null | tail -1 | grep -o 'fill_coded=[0-9]*' | tail -1 | sed 's/^fill_coded=//'); rfa_fill="${rfa_fill:-0}"
  [ -z "$(grep '\[RFA\]' "$S" 2>/dev/null)" ] \
    && { echo "W9-NO-RFA $name rep=$REP" >> "$OUT"; FAILS="$FAILS W9-NO-RFA"; }

  # THE COMMANDED HALF OF DIMENSION 2, from the SENDER's own [RACK] line. This
  # is the number compared against α = 0.40 directly — "realized false-repair vs
  # THE DERIVED ALPHA" needs the commanded rate as well as the realized one, and
  # the two live at different endpoints.
  local rack_fa rack_ff
  rack_fa=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); rack_fa="${rack_fa:-none}"
  rack_ff=$(grep '\[RACK\]' "$C" 2>/dev/null | tail -1 | grep -o 'fa_frac=[^ ]*' | tail -1 | sed 's/^fa_frac=//'); rack_ff="${rack_ff:-none}"

  # -- W10: the invocation itself ------------------------------------------
  local mb
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  { [ "$RC" -ne 0 ] || [ "$mb" = "0" ]; } \
    && { echo "W10-RC $name rep=$REP rc=$RC mean_mbps=$mb" >> "$OUT"; FAILS="$FAILS W10-RC"; }

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone. [QALPHA] and
  # [QCLK] are in this loop AS WELL AS in their witness blocks above: the blocks
  # print the ONE role-selected line each witness was read from, the loop prints
  # EVERY line.
  local f
  for f in QALPHA QCLK RACK RFA FCAUSE SUCC HOLD LCW WALL; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # -- ARM-VOID. A RESULT, NEVER AN ABORT. ---------------------------------
  # `law_n` counts the evaluations at which the arm's OWN quantile window was
  # full and the quantile-native law produced the clock. `law_n = 0` on the
  # armed arm means the arm ran the SHIPPED behaviour at every fire: the row is
  # VOID for scoring and is REPORTED as such, because deleting the invocation
  # would delete the very evidence that the window did not fill.
  [ "$arm" != "CTL" ] && [ "$qlaw" -eq 0 ] \
    && echo "ARM-VOID $cell-$arm rep=$REP (QNAT-LAW-DEAD: law_n=0 evals=$qev win_n=$qwn -- the quantile-native law never produced a clock; row VOID for scoring, REPORTED as the UNSCOREABLE rule)" >> "$OUT"

  # -- WINDOW-PARTIAL. ALSO A RESULT, NEVER AN ABORT. ----------------------
  { [ "$arm" != "CTL" ] && [ "$qwok" -lt "$qev" ]; } \
    && echo "WINDOW-PARTIAL $cell-$arm rep=$REP win_ok=$qwok/$qev win_n=$qwn (RESULT, not an abort -- the fraction is to be scored, and the quantile-native sweep read 0.829-0.999 here)" >> "$OUT"

  # -- THE BAND, AND ITS SCOPE ---------------------------------------------
  local lo hi lossy inband applies
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  # BANDSCOPE: CTL ONLY. On the TREATMENT arm an out-of-band reading is a RESULT
  # -- it is the guard's G1 reading -- and `band_applies` says so IN THE ROW.
  applies=0; [ "$arm" = "CTL" ] && applies=1

  echo "VAWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"alpha\":\"$alpha\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mean_mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"alpha_expect\":\"$a_exp\",\"alpha_got_cli\":\"$a_got\",\"alpha_got_srv\":\"$a_got_s\",\"form_expect\":\"$f_exp\",\"form_got_cli\":\"$wf_c\",\"form_got_srv\":\"$wf_s\",\"winn_expect\":\"$want_winn\",\"winn_got_cli\":\"$wn_c\",\"winn_got_srv\":\"$wn_s\",\"qclk_law_n\":$qlaw,\"qclk_evals\":$qev,\"qclk_win_ok\":$qwok,\"qclk_win_n\":\"$qwn\",\"w_us_p50\":$qp50,\"w_us_p05\":$qp05,\"w_us_p95\":$qp95,\"shipped_clock_us\":$(cell_shipped_clock "$cell"),\"fcause_n\":$fcn,\"fcause_timer\":$fctimer,\"fcause_gap_data\":$fcgap,\"fcause_gap_refresh\":$fcgapr,\"fcause_other\":$fcother,\"fcause_unattr\":$fcunattr,\"succ_det\":$scdet,\"succ_res\":$scres,\"succ_orig_frac\":\"$scof\",\"rfa_fires\":$rfa_fires,\"rfa_false\":$rfa_false,\"rfa_false_frac\":\"$rfa_ff\",\"rfa_fill_coded\":$rfa_fill,\"rack_fa\":\"$rack_fa\",\"rack_fa_frac\":\"$rack_ff\",\"fails\":\"${FAILS# }\"}" \
    | tee -a "$WIT" >> "$OUT"
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs ca cb mode bytes alpha
  envs="$(arm_env "$arm")"
  alpha="$(arm_alpha "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name alpha=$alpha form=$(arm_form "$arm") win_n=$(arm_winn "$arm") seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS arm's
  # log and pass its liveness gate on it.
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
  # the instant it returns, so these are copied under rep-unique names NOW. The
  # per-LEG ping files are DIMENSION 1's raw material and are copied
  # individually: the scored statistic is the WORST LEG, which cannot be
  # recovered from the pooled file.
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
  case " $VA_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $VA_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== VERDICT-ALPHA BATTERY (goal #101 item 4, THE DERIVED-alpha CLOCK) seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate \"THE VERDICT-ALPHA BATTERY -- PRE-REGISTRATION\", and nothing else. Paper 16.76 is the clock's derivation and 16.79.7 names the member."
  echo "CELLS $VA_CELLS   (c1/c7/sc2 are the THREE STABLE cells and carry the verdict; c8 is the LOSS-HEAVY WITNESS cell and carries none. c8L is not run at all: 52x rep dispersion.)"
  echo "ARMS-MEANING CTL = the shipped machine, all three gates ABSENT, byte-identical (2*srtt).clamp(25,100) ms; Q400 = W_q(alpha*) at the MEASURED BOUNDARY OPTIMUM alpha* = 0.40, the MAXIMAL NON-NULL member of the derived-alpha family (paper 16.79.7)"
  echo "ARMS  $VA_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $VA_ARMS; do echo "ARMENV $A alpha=$(arm_alpha "$A") form=$(arm_form "$A") gates_form=$(gate_expect "$A" RWM_W_FORM) win_n=$(arm_winn "$A") | env=\"$(arm_env "$A")\""; done
  echo "AXIS  RWM_QUANTILE_CLOCKS + RWM_W_FORM + RWM_ALPHA_OVERRIDE, all ABSENT/OFF by default. GARBAGE and any alpha outside (0,1) resolve back to ABSENT and print unset."
  echo "ALPHA-LITERAL the [GATES] echo prints the RESOLVED f64 through Rust's own to_string(): 0.40 renders as 0.4 and the arm table matches THAT, literally"
  echo "MIRRORED-AXIS RWM_HOLDDOWN_Q and RWM_REFRESH_FLOOR_US are the PREVIOUS battery's axes and are asserted ABSENT (unset), two-sided, on EVERY row of this one"
  for C in $VA_CELLS; do
    echo "GRID  $C shipped_clock_us=$(cell_shipped_clock "$C") band=[$(band_lo "$C"),$(band_hi "$C")] paths=$(cell_paths "$C") spec=\"$(cell_spec "$C")\""
  done
  echo "F1    THE WIRING WITNESS, SCORED FIRST: the WCLOCK lines. The armed arm must read law_n > 0 and win_n = 25 two-sided; CTL must read its cell's SHIPPED clock. It is a RESULT line and NEVER an abort."
  echo "ARM-VOID (law_n=0 on the armed arm) is a RESULT and never an abort"
  echo "WINDOW-PARTIAL (win_ok < evals) is a RESULT and never an abort -- the fraction is to be scored"
  echo "DIM1  DELIVERED LATENCY, per-leg, censoring-aware: the WORST LEG's p50 AND p95 from the RWM_LATPROBE per-leg ping, with legs_censor_max beside every reading"
  echo "DIM2  REALIZED FALSE REPAIR: [RFA] false_frac AND [RFA] false (the COUNT), both, plus the commanded [RACK] fa_frac -- and each compared against the DERIVED alpha 0.40 as well as against CTL"
  echo "GUARD goodput. G1 = the CTL band, ABSOLUTE. G2 = Copa utility at delta_auto = 0.5. See the pre-registration."
  echo "BANDSCOPE the goodput abort bands apply to CTL ONLY (band_applies=1); an out-of-band TREATMENT reading is a RESULT -- it is the guard's G1 reading"
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
  for CELL in $VA_CELLS; do
    for ARM in $VA_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n -- live n is law_n>0; the PRE-REGISTRATION applies the bars) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $VA_CELLS; do
  for A in $VA_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    # Counted against THIS driver's OWN row format, byte for byte, with -F so
    # the braces and quotes are literals. The hold-down battery counted the
    # PARSER's rendering of a similar row instead, which is a second dialect of
    # the same fact: a driver-side row-format change would have read zero rows
    # and reported ARM-VANISHED at every arm.
    N=$(grep -cF "VAWITNESS {\"cell\":\"$CELL\",\"arm\":\"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "VALPHA-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo VALPHA-BATTERY-DONE-$SEED_ARG
