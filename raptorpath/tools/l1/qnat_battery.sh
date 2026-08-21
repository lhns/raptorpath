#!/bin/bash
# THE QUANTILE-NATIVE α-SWEEP — the α-sweep RE-RUN ON A NEW ENGINE ARM.
#
#   sudo bash qnat_battery.sh <seed> [reps]
#
# Contract: the α-sweep's pre-registration, carried forward onto the
# QUANTILE-NATIVE window form. THE CELLS, THE ARMS, THE BANDS, THE WITNESS SET
# AND THE LOOP ORDER ARE `alpha_battery.sh`'s, UNCHANGED AND ON PURPOSE — a cell
# that differs from the ledger's cell is a different cell and its rows do not
# pool. What is new is ONE axis, and it is named here rather than inferred:
#
#   `RWM_W_FORM` ∈ {`cantelli`, `quantile`}, read by the engine ONLY when
#   `RWM_QUANTILE_CLOCKS=1`; ABSENT or GARBAGE resolves to `cantelli`. The
#   `[GATES]` line echoes the RESOLVED TOKEN, so a mistyped form is READ rather
#   than assumed — the `RWM_ACKDIAG_WINDOW_US` precedent, and the same reason
#   `RWM_ALPHA_OVERRIDE` is matched as its own literal token below.
#
# SIX ARMS, PAIRED WITHIN A REP, ARMS INNERMOST — the ccand/era layout, so the
# six arms of one cell run adjacent on ONE freshly built topology and the
# contrast is paired:
#
#   CTL   RWM_QUANTILE_CLOCKS=0, RWM_ALPHA_OVERRIDE ABSENT, RWM_W_FORM ABSENT
#         the SHIPPED clamp (2*srtt).clamp(25,100) ms; `win_n` is `unavail`
#         because no quantile window exists to size
#   Q002  α = 0.002   quantile form, expected win_n = 5000
#   Q009  α = 0.009   quantile form, expected win_n = 1112
#   Q050  α = 0.05    quantile form, expected win_n =  200
#   Q184  α = 0.184   quantile form, expected win_n =   55
#   Q400  α = 0.4     quantile form, expected win_n =   25
#
# RWM_GEN=0 ON EVERY ARM. This is not a pooling preference — under generation
# `recv_nack_tx = None` (net/mod.rs:2434) and BOTH of α's consumers drive
# machinery with no producer, so an α-sweep with generation on returns a
# perfectly flat curve at every α and FLAT CURVE is a PRE-REGISTERED LEGAL
# OUTCOME. It would produce a clean, well-witnessed, zero-abort FALSE
# REFUTATION of a route. goal-gate "THE 31 Mbit/s ANOMALY — THE SCORED
# RESULT" §7.
#
# WITNESSES, per invocation, both endpoints. [GATES] RWM_GEN is NOT one of
# them (H9: it prints gen_size and is byte-identical either way), and W3
# (cod=0) IS RETIRED — the plain window still emits proactive FEC, so cod is
# 111-750 at every lossy plain-window cell and it never discriminated the
# generation axis at all.
#
#   W1  [RFA] gen= on the receiver                     must read 0
#   W2  [PFRAC] lines on the sender                    must be 0
#   W4' [DIAG] retx=, MAX OVER ALL LINES               > 0 at lossy cells
#   W5  [RACK] fa=<spur>/<fired> on the sender         present, fired > 0
#   W6  [GATES] RWM_ALPHA_OVERRIDE= and [QALPHA] alpha= at BOTH endpoints
#   W7  [QALPHA] form= and win_n= at BOTH endpoints — **THIS BATTERY'S OWN**,
#       the arm-liveness witness for the NEW axis, read straight off the gauge
#       lines and INDEPENDENTLY OF THE PARSER. The parser's own regex is the
#       thing a token change would break first, so the driver reads the raw
#       tokens and the report reads the parsed columns; two independent
#       readings of the same reachability fact.
#
# W7's `win_n` EXPECTATION IS ASSERTED ON THE SENDER AND PRINTED-NOT-ASSERTED
# ON THE RECEIVER, and that asymmetry is a documented property of the engine,
# not a softened gate. The protocol hint is NOT plumbed to the receiver task
# (the same fact that makes CTL's two `[QALPHA]` sites disagree about the
# contract α), so an UNOVERRIDDEN receiver resolves a DIFFERENT contract α and
# therefore a different window size. On a Q arm BOTH sites carry a NUMBER
# override — an override is a number and not a hint mapping — so both must read
# the expected `win_n`, and there the receiver IS asserted.
#
# WINDOW-PARTIAL IS A RESULT AND NEVER AN ABORT. `[QCLK] win_ok=` counts the
# evaluations at which the quantile window was FULL; `win_ok < evals` means the
# arm spent part of the run below its own window and the fraction is TO BE
# SCORED, not discarded. That is the UNSCOREABLE rule applied in the direction
# it actually points: the honest denominator is printed, and an invocation is
# not deleted for carrying one.
#
# W4' IS READ AS A MAXIMUM AND NEVER OFF THE LAST LINE. `retx=` in the [DIAG]
# tail is an INTERVAL counter; reading it off the last line made the
# plain-window primitives pass report this witness failing at 5 of 15 reps
# whose [RACK] fired on the same run was 11-5717. alpha_parse.py does the max.
#
# THE GOODPUT BANDS APPLY TO CTL ONLY. They were measured on the SHIPPED
# CLAMP. A treatment arm running a clock 4.5x slower at the sender may
# legitimately land outside them — and THAT IS THE EFFECT THIS BATTERY EXISTS
# TO MEASURE. Applying a control-derived band as an ABORT to a treatment arm
# would discard the result and call it a configuration error. On the treatment
# arms W1/W2/W4'/W5/W6/W7 are the sole configuration witnesses and an
# out-of-band goodput reading is a RESULT, printed as OUT-OF-BAND, never an
# abort.
#
# WATCHER NOTE: `pgrep -f qnat_battery.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL or the ledger's QNAT-BATTERY-DONE line — never the
# process table (discipline 13).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
QN_CELLS="${RWM_QNAT_CELLS:-c1 c7 c8 c8L sc2}"
QN_ARMS="${RWM_QNAT_ARMS:-CTL Q002 Q009 Q050 Q184 Q400}"
TAG="${RWM_QNAT_TAG:-qnat}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/qnat
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
unset RWM_ALPHA_OVERRIDE RWM_QUANTILE_CLOCKS RWM_W_FORM RWM_RTT_DUMP

# ── THE ARM TABLE — the SINGLE source of the arm's env, the arm's liveness
#    assertion AND the arm's window-size expectation, so the three cannot
#    drift apart. ───────────────────────────────────────────────────────────
# α is NOT a flag: it is matched as its own literal token, and the [GATES]
# echo prints the RESOLVED value (the RWM_ACKDIAG_WINDOW_US precedent), so a
# mistyped override resolves back to `unset` and is READ rather than inferred.
arm_alpha() { # arm -> α, or "unset" for the control
  case "$1" in
    Q002) echo 0.002 ;;
    Q009) echo 0.009 ;;
    Q050) echo 0.05 ;;
    Q184) echo 0.184 ;;
    # `0.4`, NOT `0.40`. The [GATES] echo prints the RESOLVED f64 through
    # Rust's own `to_string()`, which renders 0.40 as `0.4` — and the arm
    # table is matched against that echo LITERALLY, on purpose, so the arm's
    # env and its liveness assertion cannot drift apart. The smoke caught the
    # mismatch (`got='RWM_ALPHA_OVERRIDE=0.4' want=0.40`) at 2 of 12
    # endpoint-checks, which is exactly what a smoke is for: the arm was live
    # and correct, and the HARNESS would have called it dead at every rep.
    Q400) echo 0.4 ;;
    CTL)  echo unset ;;
    *)    echo "" ;;
  esac
}

# THE ARM'S `RWM_W_FORM` ENV VALUE — `unset` is an ABSENCE, not a value, and
# the CTL arm gets NO token at all. This is NOT the same string as the
# EXPECTED ECHO: absent resolves to `cantelli` at the binary, so `gate_expect`
# below expects `cantelli` at CTL while `arm_env` emits nothing.
arm_form() { # arm -> RWM_W_FORM env value, or "unset" for the control
  case "$1" in
    CTL) echo unset ;;
    Q002|Q009|Q050|Q184|Q400) echo quantile ;;
    *)   echo "" ;;
  esac
}

# THE ARM'S EXPECTED `win_n`. Transcribed, not computed: the window size is a
# property of the arm's α that the ENGINE derives, and a harness that
# re-derived it would agree with the engine by construction instead of
# checking it.
arm_winn() { # arm -> expected [QALPHA]/[QCLK] win_n token
  case "$1" in
    Q002) echo 5000 ;;
    Q009) echo 1112 ;;
    Q050) echo 200 ;;
    Q184) echo 55 ;;
    Q400) echo 25 ;;
    CTL)  echo unavail ;;
    *)    echo "" ;;
  esac
}

QN_ARM_GATES="RWM_QUANTILE_CLOCKS RWM_ALPHA_OVERRIDE RWM_W_FORM"
# RWM_DELTA_CAP is shipped-ON since 16.71 and is the SUBSTRATE this sweep runs
# on, not an axis of it: same value on every arm, asserted =1 rather than
# assumed, because a reader who takes the pre-flip default mis-scales every
# queue number in the result.
QN_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP"
QN_CONTAM_GATES="RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP"

gate_expect() { # arm gate -> expected [GATES] value (the RESOLVED echo)
  case "$2" in
    RWM_QUANTILE_CLOCKS) case "$1" in CTL) echo 0 ;; *) echo 1 ;; esac ;;
    RWM_ALPHA_OVERRIDE)  arm_alpha "$1" ;;
    # ABSENT resolves to `cantelli` at the binary — the echo prints what the
    # engine RESOLVED, which is the only reading that settles the axis.
    RWM_W_FORM)          case "$1" in CTL) echo cantelli ;; *) echo quantile ;; esac ;;
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
  for g in $QN_ARM_GATES $QN_SUBSTRATE_GATES $QN_CONTAM_GATES; do
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
    >> "$OUT" 2>&1 || echo "QNAT-PARSE-FAIL $name rep=$REP" >> "$OUT"

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
  for g in $QN_ARM_GATES $QN_SUBSTRATE_GATES $QN_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    case "$g" in
      # A WORD, NOT A FLAG. `RWM_W_FORM` echoes `cantelli` or `quantile` and
      # `RWM_ALPHA_OVERRIDE` echoes `unset` or a number; matching either as
      # `[01]` would return the empty string for EVERY value and produce a
      # liveness gate that passes because it never matched.
      RWM_ALPHA_OVERRIDE|RWM_W_FORM) pat="$g=[^ ]*" ;;
      *)                             pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $QN_ARM_GATES $QN_SUBSTRATE_GATES " in
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

  # ── W6: THE RESOLVED α AT THE LAW, BOTH ENDPOINTS ────────────────────────
  # [GATES] can only say what was ASKED FOR. [QALPHA] says what the LAW
  # EVALUATED, at the site that evaluates it. A row failing this is VOID: its
  # own independent variable did not take.
  local qa_c qa_s
  qa_c=$(grep "\[QALPHA\] site=sender" "$C" 2>/dev/null | tail -1)
  qa_s=$(grep "\[QALPHA\] site=receiver" "$S" 2>/dev/null | tail -1)
  [ -z "$qa_c" ] && echo "W6-FAIL-CLI $name rep=$REP (no [QALPHA] at the sender)" >> "$OUT"
  [ -z "$qa_s" ] && echo "W6-FAIL-SRV $name rep=$REP (no [QALPHA] at the receiver)" >> "$OUT"
  echo "QALPHALINE $name rep=$REP site=cli ${qa_c:-none}" >> "$OUT"
  echo "QALPHALINE $name rep=$REP site=srv ${qa_s:-none}" >> "$OUT"

  # ── W7: THE WINDOW FORM AT THE LAW, BOTH ENDPOINTS ───────────────────────
  # The NEW axis's own arm-liveness witness, read off the SAME [QALPHA] lines
  # W6 uses and INDEPENDENTLY OF THE PARSER — the parser's regex is the thing
  # a token change would break first, so the driver reads raw tokens and the
  # report reads parsed columns.
  local wf_c wf_s wn_c wn_s want_form want_winn
  want_form="$(gate_expect "$arm" RWM_W_FORM)"
  want_winn="$(arm_winn "$arm")"
  wf_c=$(printf '%s' "$qa_c" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//')
  wf_s=$(printf '%s' "$qa_s" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//')
  wn_c=$(printf '%s' "$qa_c" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//')
  wn_s=$(printf '%s' "$qa_s" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//')
  wf_c="${wf_c:-none}"; wf_s="${wf_s:-none}"
  wn_c="${wn_c:-none}"; wn_s="${wn_s:-none}"
  echo "W7FORM $name rep=$REP cli=$wf_c/$wn_c srv=$wf_s/$wn_s (exp$want_form/$want_winn)" >> "$OUT"
  [ "$wf_c" != "$want_form" ] \
    && echo "W7-QFORM-FAIL-CLI $name rep=$REP got=$wf_c exp=$want_form" >> "$OUT"
  [ "$wf_s" != "$want_form" ] \
    && echo "W7-QFORM-FAIL-SRV $name rep=$REP got=$wf_s exp=$want_form" >> "$OUT"
  # THE SENDER IS ALWAYS ASSERTED. The RECEIVER is asserted ONLY on a Q arm:
  # the protocol hint is not plumbed to the receiver task, so an UNOVERRIDDEN
  # CTL receiver resolves a DIFFERENT contract α and therefore a different
  # window size. On a Q arm both sites carry a NUMBER override, so both must
  # read the expected win_n and both are checked.
  [ "$wn_c" != "$want_winn" ] \
    && echo "W7-QWINN-FAIL-CLI $name rep=$REP got=$wn_c exp=$want_winn" >> "$OUT"
  { [ "$arm" != "CTL" ] && [ "$wn_s" != "$want_winn" ]; } \
    && echo "W7-QWINN-FAIL-SRV $name rep=$REP got=$wn_s exp=$want_winn" >> "$OUT"

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone. [QALPHA] is in
  # the loop AS WELL AS in the W6/W7 block above: the block prints the ONE
  # role-selected line each witness was read from, the loop prints EVERY line.
  local f
  for f in QALPHA QCLK RACK RFA DCAP WALL LCW; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── THE REALIZED-CLOCK REACHABILITY GATE ────────────────────────────────
  # A treatment arm whose quantile-native law never produced a single clock ran
  # the law BELOW it at every evaluation. `law_n` is the bind-fraction counter
  # that makes that visible; the first version of the gauge pooled the
  # fall-throughs and reported the sweep's own variable INVERTED.
  local qcl qn
  qcl=$(grep "\[QCLK\] site=sender" "$C" 2>/dev/null | tail -1)
  qn=$(printf '%s' "$qcl" | grep -o 'law_n=[0-9]*' | tail -1 | tr -dc '0-9'); qn="${qn:-0}"
  if [ "$arm" != "CTL" ] && [ "$qn" -eq 0 ]; then
    echo "QNAT-LAW-DEAD $name rep=$REP (law_n=0 — the quantile-native law never produced a clock; row VOID)" >> "$OUT"
  fi

  # ── THE WINDOW-FILL FRACTION. A RESULT, NEVER AN ABORT. ─────────────────
  # `win_ok` counts the evaluations at which the quantile window was FULL.
  # `win_ok < evals` means the arm spent part of the run below its own window,
  # and that FRACTION IS THE THING TO SCORE — the UNSCOREABLE rule applied in
  # the direction it actually points. Deleting the invocation would delete the
  # very evidence that the window did not fill.
  local wok wev qwn
  wok=$(printf '%s' "$qcl" | grep -o 'win_ok=[0-9]*' | tail -1 | tr -dc '0-9'); wok="${wok:-0}"
  wev=$(printf '%s' "$qcl" | grep -o 'evals=[0-9]*' | tail -1 | tr -dc '0-9'); wev="${wev:-0}"
  qwn=$(printf '%s' "$qcl" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//')
  qwn="${qwn:-none}"
  { [ "$arm" != "CTL" ] && [ "$wok" -lt "$wev" ]; } \
    && echo "WINDOW-PARTIAL $name rep=$REP win_ok=$wok/$wev win_n=$qwn (RESULT, not an abort — the UNSCOREABLE rule; fraction to be scored)" >> "$OUT"

  # ── W1/W2/W4'/W5/W7 + the band, into one JSONL witness row ──────────────
  local w1 rfa_n w2 w4 w5 mb lo hi lossy inband
  w1=$(grep -o '\[RFA\] gen=[01]' "$S" 2>/dev/null | tail -1 | sed 's/.*gen=//'); w1="${w1:-none}"
  rfa_n=$(grep -c '\[RFA\]' "$S" 2>/dev/null || true); rfa_n="${rfa_n:-0}"
  w2=$(grep -c '\[PFRAC\]' "$C" 2>/dev/null || true); w2="${w2:-0}"
  # THE MAXIMUM, never the last line — see the header.
  w4=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tr -dc '0-9\n' | sort -n | tail -1); w4="${w4:-0}"
  w5=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); w5="${w5:-none}"
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  # BAND SCOPE: CTL only. On a treatment arm an out-of-band reading is a
  # RESULT, and `band_applies` says so in the row rather than in a footnote.
  local applies=0; [ "$arm" = "CTL" ] && applies=1

  echo "QNATWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"alpha\":\"$alpha\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"qclk_law_n\":$qn,\"rfa_lines\":$rfa_n,\"W1_rfa_gen\":\"$w1\",\"W2_pfrac_lines\":$w2,\"W4_retx_max\":$w4,\"W5_rack_fa\":\"$w5\",\"w7_form_cli\":\"$wf_c\",\"w7_form_srv\":\"$wf_s\",\"w7_winn_cli\":\"$wn_c\",\"w7_winn_srv\":\"$wn_s\",\"qclk_win_ok\":$wok,\"qclk_evals\":$wev,\"qclk_win_n\":\"$qwn\"}" \
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
  echo "=== rep=$REP arm=$name alpha=$alpha form=$(arm_form "$arm") win_n=$(arm_winn "$arm") seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
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
  case " $QN_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $QN_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== QNAT BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT the α-sweep's pre-registration, carried onto the QUANTILE-NATIVE window form (RWM_W_FORM)"
  echo "CELLS $QN_CELLS"
  echo "ARMS  $QN_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $QN_ARMS; do echo "ARMENV $A alpha=$(arm_alpha "$A") form=$(arm_form "$A") gates_form=$(gate_expect "$A" RWM_W_FORM) win_n=$(arm_winn "$A") | $(arm_env "$A")"; done
  echo "AXIS  RWM_W_FORM ∈ {cantelli, quantile}, read ONLY when RWM_QUANTILE_CLOCKS=1; ABSENT or GARBAGE resolves to cantelli"
  echo "W7    [QALPHA] form=/win_n= at BOTH endpoints. win_n ASSERTED at the sender always, at the receiver on Q arms ONLY (the hint is not plumbed to the receiver task)"
  echo "WINDOW-PARTIAL is a RESULT and never an abort — win_ok/evals is the fraction to be scored"
  echo "BANDSCOPE the goodput abort bands apply to CTL ONLY; out-of-band on a treatment arm is a RESULT"
  echo "W3 RETIRED (cod= is 111-750 at every lossy plain-window cell; it never discriminated the generation axis)"
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
  for CELL in $QN_CELLS; do
    for ARM in $QN_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — see qnat_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $QN_CELLS; do
  for A in $QN_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "QNAT-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo QNAT-BATTERY-DONE-$SEED_ARG
