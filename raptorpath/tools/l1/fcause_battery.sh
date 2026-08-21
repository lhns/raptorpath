#!/bin/bash
# THE FIRE-CAUSE DIAGNOSTIC PASS — WHY DO THE RECOVERY FIRES FIRE?
#
#   sudo bash fcause_battery.sh <seed> [reps]
#
# THE QUESTION, AND WHY IT IS THE ONLY ONE LEFT. The quantile-native sweep
# (goal-gate, "qnative sweep SCORED") moved the realized recovery clock W
# cleanly across six arms — a 200x span in the contract alpha, with
# [QALPHA] win_n tracking it arm for arm — and [RACK] fa_frac DID NOT MOVE at
# 4 of 5 cells. fa _|_ W. That independence refutes the shared premise of BOTH
# 16.69 routes: that fires are timer-driven, so repositioning the waiting time
# repositions the fires. The measurand derived from that premise — the
# ack-arrival distribution — is therefore the wrong quantity, and the only
# explanation the code leaves standing is that MOST FIRES ARE NOT TIMER-DRIVEN.
#
# This pass does not test a law. It MEASURES A COMPOSITION, with the new
# [FCAUSE] instrument, so the successor measurand is named from a count.
#
# TWO ARMS, PAIRED WITHIN A REP, ARMS INNERMOST — the qnat/ccand layout, so
# both arms of one cell run adjacent on ONE freshly built topology and the
# contrast is paired:
#
#   OFF   RWM_QUANTILE_CLOCKS=0, RWM_ALPHA_OVERRIDE ABSENT, RWM_W_FORM ABSENT
#         THE MACHINE AS SHIPPED. Measured FIRST and on its own merits: the
#         cause mix of the shipped engine is the reading the successor
#         measurand must be derived from, and it is a reading nobody has ever
#         taken. The shipped clamp (2*srtt).clamp(25,100) ms is the timer here.
#   Q009  alpha = 0.009, quantile form — the sweep's own Q009 probe point,
#         chosen because it is a MIDDLE arm of the measured span (win_n=1112)
#         rather than either extreme, so a cause mix that moves with arming
#         moves for a reason other than sitting on a clamp rail.
#
# THE SECOND ARM IS NOT A TREATMENT WHOSE GOODPUT IS SCORED. It exists to
# answer one question: DOES ARMING THE CLOCK CHANGE THE CAUSE MIX? If it does
# not, the timer's irrelevance is not an artifact of the shipped clamp.
#
# RWM_GEN=0 ON EVERY ARM, AND THAT IS LOAD-BEARING RATHER THAN A PREFERENCE.
# Under generation `recv_nack_tx = None`, so the SACK->gap producer is
# suppressed outright: BOTH gap_ classes are structurally empty and the cause
# mix would read timer_frac=1.0000 by construction — a clean, well-witnessed
# FALSE CONFIRMATION of the very premise under test. The [FCAUSE] line echoes
# gen= so no row can be read out of its configuration scope, and W3 below
# asserts it rather than trusting the arm env.
#
# CELLS AND BANDS ARE THE SWEEP'S, TRANSCRIBED AND NEVER REDEFINED: a cell
# that differs from the ledger's cell is a different cell and its rows do not
# pool with the sweep this pass is explaining.
#
# WITNESSES, per invocation:
#
#   W1  [FCAUSE] on the SENDER, n > 0            — else the row is VOID: there
#       is no cause mix to read. This is the row-validity gate and it is
#       checked before any cause is scored.
#   W2  [FCAUSE] other=                          — must be 0. A fire that
#       reached the site with no cause tag is UNCLASSIFIED, and an
#       unclassified fire is a FINDING about the instrument, never a guess.
#   W3  [FCAUSE] gen=                            — must be 0 (plain window).
#   W4  [DIAG] retx=, MAX OVER ALL LINES         — > 0 at lossy cells, and
#       n >= retx. The INDEPENDENT WITNESS: retx is bumped by different code
#       at the same emission, so a cause total below it means the counters
#       are missing fires the loop emitted.
#   W5  [RACK] fa=<spur>/<fired> on the sender   — present. The number the
#       sweep scored, carried beside its own cause breakdown.
#   W6  [GATES] RWM_QUANTILE_CLOCKS + [QALPHA] form=/win_n= at BOTH endpoints
#       — the arm-liveness witness. On Q009 the clock must be ARMED at the
#       law, not merely asked for; on OFF it must be ABSENT. A row failing
#       this is VOID: its own independent variable did not take.
#
# BANDSCOPE: the goodput abort bands apply to the OFF (shipped) arm ONLY. On
# Q009 an out-of-band reading is a RESULT — the sweep already measured that
# arming the clock moves goodput — and `band_applies` says so in the row
# rather than in a footnote.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1

SEED_ARG="${1:?seed}"; REPS="${2:-3}"
FC_CELLS="${RWM_FC_CELLS:-c1 c7 c8 c8L sc2}"
FC_ARMS="${RWM_FC_ARMS:-OFF Q009}"
TAG="${RWM_FC_TAG:-fcause}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/fcause
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var exported
# in this process reaches the binary whatever the forward list says, so the OFF
# arm's "absent" can only be made absent by unsetting it HERE.
unset RWM_ALPHA_OVERRIDE RWM_QUANTILE_CLOCKS RWM_W_FORM RWM_RTT_DUMP

# ── THE ARM TABLE — the SINGLE source of the arm's env, its liveness
#    assertion AND its window-size expectation, so the three cannot drift.
arm_alpha() { case "$1" in Q009) echo 0.009 ;; OFF) echo unset ;; *) echo "" ;; esac; }
arm_form()  { case "$1" in Q009) echo quantile ;; OFF) echo unset ;; *) echo "" ;; esac; }
# TRANSCRIBED from qnat_battery.sh, not recomputed: a harness that re-derived
# the window size would agree with the engine by construction.
arm_winn()  { case "$1" in Q009) echo 1112 ;; OFF) echo unavail ;; *) echo "" ;; esac; }

FC_ARM_GATES="RWM_QUANTILE_CLOCKS RWM_ALPHA_OVERRIDE RWM_W_FORM"
FC_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP"
FC_CONTAM_GATES="RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP"

gate_expect() { # arm gate -> expected [GATES] value (the RESOLVED echo)
  case "$2" in
    RWM_QUANTILE_CLOCKS) case "$1" in OFF) echo 0 ;; *) echo 1 ;; esac ;;
    RWM_ALPHA_OVERRIDE)  arm_alpha "$1" ;;
    RWM_W_FORM)          case "$1" in OFF) echo cantelli ;; *) echo quantile ;; esac ;;
    RWM_DELTA_CAP)       echo 1 ;;
    RWM_SUM_CAP)         echo 1 ;;
    *) echo 0 ;;
  esac
}

arm_env() { # arm -> "RWM_X=v ..."
  local a="$1" g out="" v
  for g in $FC_ARM_GATES $FC_SUBSTRATE_GATES $FC_CONTAM_GATES; do
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

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from qnat_battery.sh.
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
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; c8) echo 50;; c8L) echo 45;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; c8) echo 100;; c8L) echo 95;; sc2) echo 92;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

check_and_parse() { # name cell arm alpha
  local name="$1" cell="$2" arm="$3" alpha="$4"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log

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
  for g in $FC_ARM_GATES $FC_SUBSTRATE_GATES $FC_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    case "$g" in
      RWM_ALPHA_OVERRIDE|RWM_W_FORM) pat="$g=[^ ]*" ;;
      *)                             pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $FC_ARM_GATES $FC_SUBSTRATE_GATES " in
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

  local i
  for i in RWM_DIAG RWM_FDIAG; do
    got_c=$(printf '%s' "$gl_c" | grep -o "$i=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$i=[01]")
    echoline="$echoline $i=$got_c/$got_s(exp1)"
    { [ "$got_c" != "$i=1" ] || [ "$got_s" != "$i=1" ]; } \
      && echo "INSTRUMENT-FAIL-GATE $name rep=$REP gate=$i cli='$got_c' srv='$got_s'" >> "$OUT"
  done
  echo "LIVENESS $name rep=$REP$echoline" >> "$OUT"

  # ── W6: THE CLOCK AT THE LAW, BOTH ENDPOINTS ────────────────────────────
  local qa_c qa_s wf_c wf_s wn_c wn_s want_form want_winn
  qa_c=$(grep "\[QALPHA\] site=sender" "$C" 2>/dev/null | tail -1)
  qa_s=$(grep "\[QALPHA\] site=receiver" "$S" 2>/dev/null | tail -1)
  want_form="$(gate_expect "$arm" RWM_W_FORM)"
  want_winn="$(arm_winn "$arm")"
  wf_c=$(printf '%s' "$qa_c" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//'); wf_c="${wf_c:-none}"
  wf_s=$(printf '%s' "$qa_s" | grep -o 'form=[^ ]*' | tail -1 | sed 's/^form=//'); wf_s="${wf_s:-none}"
  wn_c=$(printf '%s' "$qa_c" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//'); wn_c="${wn_c:-none}"
  wn_s=$(printf '%s' "$qa_s" | grep -o 'win_n=[^ ]*' | tail -1 | sed 's/^win_n=//'); wn_s="${wn_s:-none}"
  echo "W6FORM $name rep=$REP cli=$wf_c/$wn_c srv=$wf_s/$wn_s (exp$want_form/$want_winn)" >> "$OUT"
  [ "$wf_c" != "$want_form" ] \
    && echo "W6-QFORM-FAIL-CLI $name rep=$REP got=$wf_c exp=$want_form" >> "$OUT"
  # The sender is always asserted. The RECEIVER's win_n is asserted only on the
  # Q arm: the protocol hint is not plumbed to the receiver task, so an
  # unoverridden OFF receiver resolves a DIFFERENT contract alpha and therefore
  # a different window size. qnat_battery.sh documents the same asymmetry.
  [ "$wn_c" != "$want_winn" ] \
    && echo "W6-QWINN-FAIL-CLI $name rep=$REP got=$wn_c exp=$want_winn" >> "$OUT"
  { [ "$arm" != "OFF" ] && [ "$wn_s" != "$want_winn" ]; } \
    && echo "W6-QWINN-FAIL-SRV $name rep=$REP got=$wn_s exp=$want_winn" >> "$OUT"

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone.
  local f
  for f in FCAUSE RACK RFA QALPHA QCLK; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── W1-W5 + the band, into one JSONL witness row ────────────────────────
  # THE READING. Cumulative counters: the LAST [FCAUSE] is the reading.
  local fc n timer gd gr other fired unattr tfrac
  fc=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1)
  fld() { printf '%s' "$fc" | grep -o " $1=[^ ]*" | tail -1 | sed "s/^ $1=//"; }
  n=$(fld n);            n="${n:-0}"
  timer=$(fld timer);    timer="${timer:-0}"
  gd=$(fld gap_data);    gd="${gd:-0}"
  gr=$(fld gap_refresh); gr="${gr:-0}"
  other=$(fld other);    other="${other:-0}"
  fired=$(fld fired);    fired="${fired:-0}"
  unattr=$(fld unattr);  unattr="${unattr:-0}"
  tfrac=$(fld timer_frac); tfrac="${tfrac:--}"
  local w3; w3=$(printf '%s' "$fc" | grep -o 'gen=[01]' | tail -1 | sed 's/gen=//'); w3="${w3:-none}"

  local w4 w5 mb lo hi lossy inband
  w4=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tr -dc '0-9\n' | sort -n | tail -1); w4="${w4:-0}"
  w5=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); w5="${w5:-none}"
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  local applies=0; [ "$arm" = "OFF" ] && applies=1

  # W1: the ROW-VALIDITY gate, stated as its own line so a void row is read as
  # void rather than as a measured timer_frac of zero.
  if [ -z "$fc" ]; then
    echo "W1-FCAUSE-ABSENT $name rep=$REP (no [FCAUSE] on the sender — row VOID, no cause mix)" >> "$OUT"
  elif [ "$n" -eq 0 ]; then
    echo "W1-FCAUSE-EMPTY $name rep=$REP (n=0 — the gap loop never fired; row VOID)" >> "$OUT"
  fi
  # W2: an unclassified fire is a FINDING about the instrument.
  [ "$other" != "0" ] \
    && echo "W2-UNCLASSIFIED $name rep=$REP other=$other/$n (a fire reached the site with no cause tag)" >> "$OUT"
  # W3: the configuration contract.
  [ "$w3" != "0" ] \
    && echo "W3-GEN-CONTRACT $name rep=$REP gen=$w3 (NOT plain window — both gap_ classes are structurally empty; row VOID)" >> "$OUT"
  # W4: the independent witness.
  { [ "$lossy" = "1" ] && [ "$w4" -eq 0 ]; } \
    && echo "W4-NO-RETX $name rep=$REP (retx=0 at a LOSSY cell — the gap loop did not run)" >> "$OUT"
  [ -n "$fc" ] && [ "$n" -lt "$w4" ] \
    && echo "W4-UNDERCOUNT $name rep=$REP n=$n < retx=$w4 (the cause counters are missing emitted fires)" >> "$OUT"
  # W5: the number the sweep scored.
  [ "$w5" = "none" ] \
    && echo "W5-NO-RACK $name rep=$REP (no [RACK] fa= on the sender)" >> "$OUT"

  echo "FCWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"alpha\":\"$alpha\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"n\":$n,\"timer\":$timer,\"gap_data\":$gd,\"gap_refresh\":$gr,\"other\":$other,\"fired\":$fired,\"unattr\":$unattr,\"timer_frac\":\"$tfrac\",\"W3_gen\":\"$w3\",\"W4_retx_max\":$w4,\"W5_rack_fa\":\"$w5\",\"w6_form_cli\":\"$wf_c\",\"w6_winn_cli\":\"$wn_c\",\"w6_form_srv\":\"$wf_s\",\"w6_winn_srv\":\"$wn_s\"}" \
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
      RWM_DIAG=1 RWM_FDIAG=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC" >> "$OUT" || true
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  check_and_parse "$name" "$cell" "$arm" "$alpha"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $FC_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $FC_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  run_topo "$1" "$2"
}

{
  echo "=== FCAUSE DIAGNOSTIC PASS seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT classify the recovery fires; the sweep proved fa _|_ W, so the 16.69 measurand is wrong and the cause mix is what names its successor"
  echo "CELLS $FC_CELLS"
  echo "ARMS  $FC_ARMS   (paired within rep, ARMS INNERMOST; OFF = the machine AS SHIPPED)"
  for A in $FC_ARMS; do echo "ARMENV $A alpha=$(arm_alpha "$A") form=$(arm_form "$A") gates_form=$(gate_expect "$A" RWM_W_FORM) win_n=$(arm_winn "$A") | $(arm_env "$A")"; done
  echo "GEN   RWM_GEN=0 on EVERY arm — under generation recv_nack_tx is None, both gap_ classes are structurally empty, and the pass would FALSELY CONFIRM its own premise"
  echo "BANDSCOPE the goodput abort bands apply to the OFF (shipped) arm ONLY; out-of-band on Q009 is a RESULT"
  echo "VOID  W1 (no/empty [FCAUSE]) and W3 (gen!=0) VOID a row — a void row is NOT a measured timer_frac of zero"
  echo "BIN $BIN"
  echo "SHA256 $(sha256sum "$BIN" 2>/dev/null)"
  echo "COMMIT $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "KERNEL $(uname -r)"
  echo "UPTIME $(uptime)"
  echo "CPU $(lscpu | grep -E 'Model name' | head -1)"
} >> "$OUT"

RC=0
for REP in $(seq 1 "$REPS"); do
  for CELL in $FC_CELLS; do
    for ARM in $FC_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — see fcause_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $FC_CELLS; do
  for A in $FC_ARMS; do
    N=$(grep -c "\"cell\":\"$CELL\",\"arm\":\"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "FCAUSE-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo FCAUSE-BATTERY-DONE-$SEED_ARG
