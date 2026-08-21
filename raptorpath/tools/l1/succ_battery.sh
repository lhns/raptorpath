#!/bin/bash
# THE SUCCESSOR-ARRIVAL PASS — WHAT IS THE DISTRIBUTION THE WAITING TIME MUST
# BE POSITIONED ON?
#
#   sudo bash succ_battery.sh <seed> [reps]
#
# THE QUESTION, AND WHY IT IS THE ONLY ONE LEFT BEFORE A FORMULA. The
# fire-cause pass (goal-gate, "THE FIRE-CAUSE PASS — THE SCORED RESULT")
# classified 107,597 recovery fires: 0.59 % timer-driven, 98.99 % gap_data —
# the receiver's SACK report, emitted when a higher seq arrives while a hole is
# outstanding. It named the successor measurand from that count and then named
# the reading it had NOT taken:
#
#   "the successor-arrival distribution has never been measured on this engine
#    ... A derivation written against an uncharacterized distribution would
#    repeat the exact defect just corrected."
#
# This pass takes that reading. It does not test a law, does not score an arm
# against a control, and does not derive a waiting time. It MEASURES A
# DISTRIBUTION on the machine AS SHIPPED, so the formula-first derivation that
# follows (ADR-0070: the formula and its derivation IN THE PAPER, before the
# code) is written against a characterized quantity.
#
# ONE ARM, AND THAT IS THE CONTRACT RATHER THAN A SHORTCUT. There is no
# treatment here. The shipped machine's own successor-arrival distribution is
# the thing nobody has ever looked at; a second arm would be a comparison
# against a distribution that does not yet have a shape. The clock is
# DISARMED (RWM_QUANTILE_CLOCKS absent) for the same reason the fire-cause
# pass measured OFF first: the shipped clamp is what 15 of 15 rows here are a
# measurement of.
#
# RWM_GEN=0 ON EVERY ROW, AND IT IS LOAD-BEARING. Under generation coding
# every arrival is coded, so the `orig` outcome is structurally empty and the
# pass would read orig_frac=0.0000 by construction — a clean, well-witnessed
# FALSE reading of the very quantity under study. The [SUCC] line echoes gen=
# so no row can be read out of its configuration scope, and W3 asserts it
# rather than trusting the arm env.
#
# RWM_SUCC_DUMP IS OFF ON THE SCORED PASS. The raw record dump writes megabytes
# of stderr AT THE RECEIVER, and receiver-side cost is directly goodput-visible
# — the goodput bands below are the constraint that says this pass did not
# perturb the machine it measured, and running the dump on the scored
# invocations would spend exactly that constraint. The quantile line is ungated
# and is what this pass reads. A dump pass, if one is ever needed for an
# offline functional, is SEPARATE, on the [RTTDUMP] precedent.
#
# ONE RUN PER INVOCATION (perf_rwm_c.sh ... 1). The gauge's high-water mark and
# open-hole map outlive an individual perf RUN, so a multi-run invocation reads
# across a seq-space reset. Declared in net/succ.rs, honoured here.
#
# CELLS AND BANDS ARE THE FIRE-CAUSE PASS'S, TRANSCRIBED AND NEVER REDEFINED: a
# cell that differs from the ledger's cell is a different cell and its rows do
# not pool with the pass this one continues.
#
# WITNESSES, per invocation:
#
#   W1  [SUCC] on the SERVER (receiver), det > 0     — else the row is VOID:
#       no hole was detected, so there is no distribution to read. Checked
#       before any quantile is scored.
#   W2  THE ACCOUNTING IDENTITY, on the wire:
#         det == orig_n + rep_n + aban_n + open + over
#       A violation is a FINDING ABOUT THE INSTRUMENT and VOIDS the row. The
#       three outcomes either partition the detected holes or they are not
#       outcomes.
#   W3  [SUCC] gen=                                  — must be 0 (plain
#       window). Otherwise `orig` is structurally empty and the row is VOID.
#   W4  [SUCC] over=                                 — must be 0. Nonzero
#       means a DECLARED RESOURCE BOUND bound, and a bounded gauge that
#       truncated its own denominator must say so rather than report a
#       quantile over a subset nobody can size.
#   W5  THE ROUTING GATE (MEASUREMENT DISCIPLINE rule 1 — prove the mechanism
#       under test executes). [RFA] fires > 0 at the SERVER *and* [FCAUSE]
#       gap_data > 0 at the CLIENT. These are three independent counters over
#       the same underlying loss, bumped by different code at different
#       events: [RFA] classifies ARRIVALS, [FCAUSE] classifies FIRES, [SUCC]
#       times HOLES. A det that moves while the other two read zero would mean
#       this gauge invented its own denominator, and a distribution measured
#       on a row where NO gap_data fire happened is not a measurement of the
#       thing those fires are governed by.
#   W6  [GATES] on BOTH endpoints: RWM_DIAG=1, RWM_FDIAG=1 (the readout),
#       RWM_QUANTILE_CLOCKS=0 (the clock is DISARMED), RWM_SUCC_DUMP=0 (the
#       pass is not paying for the dump), and every contamination gate 0.
#       A row failing this is VOID: its own configuration did not take.
#
# UNSCOREABLE-thin is a LEGAL OUTCOME, per outcome per cell, and the thresholds
# are in the PRE-REGISTRATION rather than here-and-also-there: this script
# emits the counts, succ_report.py applies the pre-stated bars.
#
# BANDSCOPE: there is ONE arm and it is the shipped machine, so the goodput
# abort bands apply to EVERY row (band_applies=1 throughout). That is the
# constraint that says this observation-only instrument did not cost the
# transfer it observed.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1

SEED_ARG="${1:?seed}"; REPS="${2:-3}"
SU_CELLS="${RWM_SU_CELLS:-c1 c7 c8 c8L sc2}"
TAG="${RWM_SU_TAG:-succ}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUTDIR=/home/vibe/succ
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var exported
# in this process reaches the binary whatever the forward list says, so "the
# clock is absent" and "the dump is absent" can only be made true HERE.
unset RWM_ALPHA_OVERRIDE RWM_QUANTILE_CLOCKS RWM_W_FORM RWM_RTT_DUMP \
      RWM_SUCC_DUMP RWM_SUCC_DUMP_MAX

SU_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP"
SU_CONTAM_GATES="RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS RWM_DERIVED_SWEEP \
RWM_COMPOSED_CAP RWM_THREE_TERM RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE \
RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 RWM_LOSS_SENT_TRUTH RWM_RTT_DUMP \
RWM_SUCC_DUMP"

gate_expect() { # gate -> expected [GATES] value (the RESOLVED echo)
  case "$1" in
    RWM_DELTA_CAP) echo 1 ;;
    RWM_SUM_CAP)   echo 1 ;;
    *)             echo 0 ;;
  esac
}

arm_env() { # the SHIPPED arm's env: the substrate gates, explicitly, and
            # every contamination gate explicitly 0 — "absent" is not a state
            # a [GATES] echo can distinguish from "inherited".
  local g out=""
  for g in $SU_SUBSTRATE_GATES $SU_CONTAM_GATES; do
    out="$out $g=$(gate_expect "$g")"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from fcause_battery.sh.
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

check_and_parse() { # name cell
  local name="$1" cell="$2"
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

  # ── W6: THE CONFIGURATION, BOTH ENDPOINTS ───────────────────────────────
  local g want got_c got_s echoline=""
  for g in $SU_SUBSTRATE_GATES $SU_CONTAM_GATES; do
    want="$(gate_expect "$g")"
    got_c=$(printf '%s' "$gl_c" | grep -o "$g=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$g=[01]")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $SU_SUBSTRATE_GATES " in
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

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone.
  local f
  for f in SUCC FCAUSE RACK RFA QALPHA QCLK; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── THE READING. Cumulative counters: the LAST [SUCC] is the reading, and
  #    it is read at the SERVER because the receiver is where holes live.
  local su
  su=$(grep "\[SUCC\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1)
  sfld() { printf '%s' "$su" | grep -o " $1=[^ ]*" | tail -1 | sed "s/^ $1=//"; }
  local det res open over ofrac cross gen
  det=$(sfld det);            det="${det:-0}"
  res=$(sfld res);            res="${res:-0}"
  open=$(sfld open);          open="${open:-0}"
  over=$(sfld over);          over="${over:-0}"
  ofrac=$(sfld orig_frac);    ofrac="${ofrac:--}"
  cross=$(sfld cross_us);     cross="${cross:--}"
  gen=$(sfld gen);            gen="${gen:-none}"

  # Per-outcome slots, all six, verbatim off the line.
  local o_n o_p50 o_p90 o_p99 o_mx o_mean
  local r_n r_p50 r_p90 r_p99 r_mx r_mean
  local a_n a_p50 a_p90 a_p99 a_mx a_mean
  o_n=$(sfld orig_n);   o_n="${o_n:-0}"
  o_p50=$(sfld orig_p50_us); o_p50="${o_p50:--}"
  o_p90=$(sfld orig_p90_us); o_p90="${o_p90:--}"
  o_p99=$(sfld orig_p99_us); o_p99="${o_p99:--}"
  o_mx=$(sfld orig_mx_us);   o_mx="${o_mx:--}"
  o_mean=$(sfld orig_mean_us); o_mean="${o_mean:--}"
  r_n=$(sfld rep_n);    r_n="${r_n:-0}"
  r_p50=$(sfld rep_p50_us);  r_p50="${r_p50:--}"
  r_p90=$(sfld rep_p90_us);  r_p90="${r_p90:--}"
  r_p99=$(sfld rep_p99_us);  r_p99="${r_p99:--}"
  r_mx=$(sfld rep_mx_us);    r_mx="${r_mx:--}"
  r_mean=$(sfld rep_mean_us); r_mean="${r_mean:--}"
  a_n=$(sfld aban_n);   a_n="${a_n:-0}"
  a_p50=$(sfld aban_p50_us); a_p50="${a_p50:--}"
  a_p90=$(sfld aban_p90_us); a_p90="${a_p90:--}"
  a_p99=$(sfld aban_p99_us); a_p99="${a_p99:--}"
  a_mx=$(sfld aban_mx_us);   a_mx="${a_mx:--}"
  a_mean=$(sfld aban_mean_us); a_mean="${a_mean:--}"

  # ── W5: THE ROUTING GATE, three independent counters ────────────────────
  local rfa_fires fc_gd fc_n retx_max rack_fa
  rfa_fires=$(grep "\[RFA\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1 \
    | grep -o ' fires=[0-9]*' | tail -1 | sed 's/^ fires=//'); rfa_fires="${rfa_fires:-0}"
  fc_gd=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1 \
    | grep -o ' gap_data=[0-9]*' | tail -1 | sed 's/^ gap_data=//'); fc_gd="${fc_gd:-0}"
  fc_n=$(grep "\[FCAUSE\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1 \
    | grep -o ' n=[0-9]*' | tail -1 | sed 's/^ n=//'); fc_n="${fc_n:-0}"
  retx_max=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tr -dc '0-9\n' | sort -n | tail -1)
  retx_max="${retx_max:-0}"
  rack_fa=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//')
  rack_fa="${rack_fa:-none}"

  local mb lo hi lossy inband
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')

  # W1: THE ROW-VALIDITY GATE, its own line, so a void row is read as void and
  # never as a measured distribution with zero holes in it.
  if [ -z "$su" ]; then
    echo "W1-SUCC-ABSENT $name rep=$REP (no [SUCC] at the RECEIVER — row VOID, no distribution)" >> "$OUT"
  elif [ "$det" -eq 0 ]; then
    echo "W1-SUCC-EMPTY $name rep=$REP (det=0 — no hole was ever detected; row VOID)" >> "$OUT"
  fi
  # W2: THE ACCOUNTING IDENTITY, checked on the wire.
  if [ -n "$su" ]; then
    local sum; sum=$(( o_n + r_n + a_n + open + over ))
    [ "$det" -ne "$sum" ] \
      && echo "W2-IDENTITY-FAIL $name rep=$REP det=$det != orig($o_n)+rep($r_n)+aban($a_n)+open($open)+over($over)=$sum (the outcomes do not partition the holes; row VOID)" >> "$OUT"
  fi
  # W3: the configuration contract.
  [ "$gen" != "0" ] \
    && echo "W3-GEN-CONTRACT $name rep=$REP gen=$gen (NOT plain window — the orig outcome is structurally empty; row VOID)" >> "$OUT"
  # W4: a declared bound that BOUND.
  [ "$over" != "0" ] \
    && echo "W4-BOUND-BOUND $name rep=$REP over=$over/$det (a declared resource bound truncated the measurement)" >> "$OUT"
  # W5: the routing gate — three counters, two of them not this gauge's.
  { [ "$lossy" = "1" ] && [ "$rfa_fires" -eq 0 ]; } \
    && echo "W5-NO-RFA $name rep=$REP (RFA fires=0 at a LOSSY cell — the independent arrival-class witness saw nothing)" >> "$OUT"
  { [ "$lossy" = "1" ] && [ "$fc_gd" -eq 0 ]; } \
    && echo "W5-NO-GAPDATA $name rep=$REP (FCAUSE gap_data=0 at a LOSSY cell — NOT ONE FIRE of the class this distribution governs happened on this row)" >> "$OUT"
  { [ "$lossy" = "1" ] && [ "$retx_max" -eq 0 ]; } \
    && echo "W5-NO-RETX $name rep=$REP (retx=0 at a LOSSY cell — the gap loop did not run)" >> "$OUT"

  echo "SUWITNESS {\"cell\":\"$cell\",\"arm\":\"SHIPPED\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":1,\"in_band\":$inband,\"lossy\":$lossy,\"det\":$det,\"res\":$res,\"open\":$open,\"over\":$over,\"orig_frac\":\"$ofrac\",\"cross_us\":\"$cross\",\"gen\":\"$gen\",\"orig_n\":$o_n,\"orig_p50\":\"$o_p50\",\"orig_p90\":\"$o_p90\",\"orig_p99\":\"$o_p99\",\"orig_mx\":\"$o_mx\",\"orig_mean\":\"$o_mean\",\"rep_n\":$r_n,\"rep_p50\":\"$r_p50\",\"rep_p90\":\"$r_p90\",\"rep_p99\":\"$r_p99\",\"rep_mx\":\"$r_mx\",\"rep_mean\":\"$r_mean\",\"aban_n\":$a_n,\"aban_p50\":\"$a_p50\",\"aban_p90\":\"$a_p90\",\"aban_p99\":\"$a_p99\",\"aban_mx\":\"$a_mx\",\"aban_mean\":\"$a_mean\",\"W5_rfa_fires\":$rfa_fires,\"W5_fc_gap_data\":$fc_gd,\"W5_fc_n\":$fc_n,\"W5_retx_max\":$retx_max,\"rack_fa\":\"$rack_fa\"}" \
    | tee -a "$OUTDIR/${TAG}-witness-s${SEED_ARG}.jsonl" >> "$OUT"
}

run_topo() { # cell
  local cell="$1" name="$1-SHIPPED"
  local envs ca cb mode bytes
  envs="$(arm_env)"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # row's log and pass its liveness gate on it.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs \
      RWM_DIAG=1 RWM_FDIAG=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC" >> "$OUT" || true
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  check_and_parse "$name" "$cell"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

run_one() { # cell
  case " $SU_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  run_topo "$1"
}

{
  echo "=== SUCCESSOR-ARRIVAL PASS seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT characterize P(successor arrives by t | hole outstanding) on the SHIPPED machine — the measurand the fire-cause pass named and did not measure"
  echo "CELLS $SU_CELLS"
  echo "ARMS  SHIPPED (one arm: there is no treatment; the shipped distribution is what has never been looked at)"
  echo "ARMENV SHIPPED | $(arm_env)"
  echo "CLOCK DISARMED — RWM_QUANTILE_CLOCKS absent/0, no RWM_ALPHA_OVERRIDE, no RWM_W_FORM"
  echo "GEN   RWM_GEN=0 on EVERY row — under generation the orig outcome is structurally empty and the pass would FALSELY read orig_frac=0"
  echo "DUMP  RWM_SUCC_DUMP=0 on EVERY row — the raw dump is a RECEIVER-side stderr cost and the goodput bands are the constraint it would spend"
  echo "RUNS  1 per invocation — the gauge outlives a perf RUN, so a multi-run invocation reads across a seq-space reset (net/succ.rs)"
  echo "SITE  [SUCC] is read at the SERVER (the receiver); [FCAUSE]/[RACK] at the CLIENT (the sender)"
  echo "BANDSCOPE ONE shipped arm, so the goodput bands apply to EVERY row (band_applies=1) — the constraint that says an observation-only gauge did not cost the transfer"
  echo "VOID  W1 (no/empty [SUCC]), W2 (identity fails) and W3 (gen!=0) VOID a row"
  echo "THIN  UNSCOREABLE-thin bars are in the PRE-REGISTRATION and applied by succ_report.py; this script emits counts only"
  echo "BIN $BIN"
  echo "SHA256 $(sha256sum "$BIN" 2>/dev/null)"
  echo "COMMIT $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "KERNEL $(uname -r)"
  echo "UPTIME $(uptime)"
  echo "CPU $(lscpu | grep -E 'Model name' | head -1)"
} >> "$OUT"

RC=0
for REP in $(seq 1 "$REPS"); do
  for CELL in $SU_CELLS; do
    run_one "$CELL"
  done
done

echo "=== ARMCOUNTS (rows, NOT live det — see succ_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $SU_CELLS; do
  N=$(grep -c "\"cell\":\"$CELL\"" "$OUT" || true); N="${N:-0}"
  echo "ARMCOUNT $CELL rows=$N/$REPS" >> "$OUT"
  [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL" >> "$OUT"
done
echo "SUCC-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo SUCC-BATTERY-DONE-$SEED_ARG
