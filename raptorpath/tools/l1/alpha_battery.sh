#!/bin/bash
# THE α-SWEEP — goal #100 item 2, THE ISOLATION EXPERIMENT.
#
#   sudo bash alpha_battery.sh <seed> [reps]
#
# Contract: goal-gate "THE α-SWEEP — PRE-REGISTRATION" (feat/alpha-sweep from
# main@079a4c6). Hold everything fixed, move the recovery clock's false-alarm
# rate α, and measure what α COSTS in goodput and in delivered latency.
#
# SIX ARMS, PAIRED WITHIN A REP, ARMS INNERMOST — the ccand/era layout, so the
# six arms of one cell run adjacent on ONE freshly built topology and the
# contrast is paired:
#
#   CTL   RWM_QUANTILE_CLOCKS=0, RWM_ALPHA_OVERRIDE ABSENT
#         the SHIPPED clamp (2*srtt).clamp(25,100) ms, measured by 16.70.1
#         binding 92.4-99.7% of the time and violating RACK's own budget at
#         all five cells
#   Q002  α = 0.002   k = 22.34   route (b) at Bulk, MEASURED σ = 3.140 ms
#   Q009  α = 0.009   k = 10.49   route (d) at Bulk, ALL-MEASURED inputs
#   Q050  α = 0.05    k =  4.359  route (b) at Bulk, ESTIMATED σ = 18.1 ms;
#                                 and option (c), RACK's own 1/16 = 0.0625
#   Q184  α = 0.184   k =  2.106  route (b) at Auto, estimated σ; and route
#                                 (d) at Auto, all-measured inputs (0.1829)
#   Q400  α = 0.40    k =  1.225  no construction — the fast/wasteful right
#                                 anchor, so the optimum can be INTERIOR
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
#       THIS BATTERY'S OWN, and the one it owed itself: without it the sweep
#       has no proof that its own independent variable took, which is exactly
#       the failure the 31 Mbit/s anomaly recorded one level up.
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
# arms W1/W2/W4'/W5/W6 are the sole configuration witnesses and an
# out-of-band goodput reading is a RESULT, printed as OUT-OF-BAND, never an
# abort.
#
# WATCHER NOTE: `pgrep -f alpha_battery.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL or the ledger's ALPHA-BATTERY-DONE line — never the
# process table (discipline 13).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
AL_CELLS="${RWM_ALPHA_CELLS:-c1 c7 c8 c8L sc2}"
AL_ARMS="${RWM_ALPHA_ARMS:-CTL Q002 Q009 Q050 Q184 Q400}"
TAG="${RWM_ALPHA_TAG:-alpha}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/alpha/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/alpha/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var that is
# exported in this process reaches the binary whatever the forward list says,
# so the CONTROL arm's "absent" can only be made absent by unsetting it here.
unset RWM_ALPHA_OVERRIDE
unset RWM_QUANTILE_CLOCKS

# ── THE ARM TABLE — the SINGLE source of both the arm's env and the arm's
#    liveness assertion, so the two cannot drift apart. ────────────────────
# α is NOT a flag: it is matched as its own literal token, and the [GATES]
# echo prints the RESOLVED value (the RWM_ACKDIAG_WINDOW_US precedent), so a
# mistyped override resolves back to `unset` and is READ rather than inferred.
arm_alpha() { # arm -> α, or "unset" for the control
  case "$1" in
    Q002) echo 0.002 ;;
    Q009) echo 0.009 ;;
    Q050) echo 0.05 ;;
    Q184) echo 0.184 ;;
    Q400) echo 0.40 ;;
    CTL)  echo unset ;;
    *)    echo "" ;;
  esac
}

AL_ARM_GATES="RWM_QUANTILE_CLOCKS RWM_ALPHA_OVERRIDE"
# RWM_DELTA_CAP is shipped-ON since 16.71 and is the SUBSTRATE this sweep runs
# on, not an axis of it: same value on every arm, asserted =1 rather than
# assumed, because a reader who takes the pre-flip default mis-scales every
# queue number in the result.
AL_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP"
AL_CONTAM_GATES="RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_COMPOSED_CAP RWM_THREE_TERM \
RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 \
RWM_LOSS_SENT_TRUTH"

gate_expect() { # arm gate -> expected [GATES] value
  case "$2" in
    RWM_QUANTILE_CLOCKS) case "$1" in CTL) echo 0 ;; *) echo 1 ;; esac ;;
    RWM_ALPHA_OVERRIDE)  arm_alpha "$1" ;;
    RWM_DELTA_CAP)       echo 1 ;;
    RWM_SUM_CAP)         echo 1 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the table above. The control gets NO
# RWM_ALPHA_OVERRIDE token at all — `unset` is an ABSENCE, not a value.
arm_env() { # arm -> "RWM_X=v ..."
  local a="$1" g out="" v
  for g in $AL_ARM_GATES $AL_SUBSTRATE_GATES $AL_CONTAM_GATES; do
    v="$(gate_expect "$a" "$g")"
    [ "$g" = "RWM_ALPHA_OVERRIDE" ] && [ "$v" = "unset" ] && continue
    out="$out $g=$v"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# ccand_battery.sh:202-215, never redefined: a cell that differs from the
# ledger's cell is a different cell and its rows do not pool.
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

  python3 ./alpha_parse.py "$cell" "$arm" "$alpha" "$SEED_ARG" "$REP" \
      "$C" "$S" "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "ALPHA-PARSE-FAIL $name rep=$REP" >> "$OUT"

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
  for g in $AL_ARM_GATES $AL_SUBSTRATE_GATES $AL_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    case "$g" in
      RWM_ALPHA_OVERRIDE) pat="$g=[^ ]*" ;;
      *)                  pat="$g=[01]" ;;
    esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$pat")
    got_s=$(printf '%s' "$gl_s" | grep -o "$pat")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $AL_ARM_GATES $AL_SUBSTRATE_GATES " in
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

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone.
  local f
  for f in QCLK RACK RFA DCAP WALL LCW; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── THE REALIZED-CLOCK REACHABILITY GATE ────────────────────────────────
  # A treatment arm whose quantile law never produced a single clock ran the
  # law BELOW it at every evaluation. `law_n` is the bind-fraction counter
  # that makes that visible; the first version of the gauge pooled the
  # fall-throughs and reported the sweep's own variable INVERTED.
  local qn
  qn=$(grep -o '\[QCLK\] site=sender .*law_n=[0-9]*' "$C" 2>/dev/null | tail -1 \
        | grep -o 'law_n=[0-9]*' | tr -dc '0-9'); qn="${qn:-0}"
  if [ "$arm" != "CTL" ] && [ "$qn" -eq 0 ]; then
    echo "QCLK-LAW-DEAD $name rep=$REP (law_n=0 — every evaluation fell through; row VOID)" >> "$OUT"
  fi

  # ── W1/W2/W4'/W5 + the band, into one JSONL witness row ─────────────────
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

  echo "ALPHAWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"alpha\":\"$alpha\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"qclk_law_n\":$qn,\"rfa_lines\":$rfa_n,\"W1_rfa_gen\":\"$w1\",\"W2_pfrac_lines\":$w2,\"W4_retx_max\":$w4,\"W5_rack_fa\":\"$w5\"}" \
    | tee -a "/home/vibe/alpha/${TAG}-witness-s${SEED_ARG}.jsonl" >> "$OUT"
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs ca cb mode bytes alpha
  envs="$(arm_env "$arm")"
  alpha="$(arm_alpha "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name alpha=$alpha seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
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
  case " $AL_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $AL_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== ALPHA BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate 'THE α-SWEEP — PRE-REGISTRATION' (feat/alpha-sweep from main@079a4c6)"
  echo "CELLS $AL_CELLS"
  echo "ARMS  $AL_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $AL_ARMS; do echo "ARMENV $A alpha=$(arm_alpha "$A") | $(arm_env "$A")"; done
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
  for CELL in $AL_CELLS; do
    for ARM in $AL_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — see alpha_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $AL_CELLS; do
  for A in $AL_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "ALPHA-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo ALPHA-BATTERY-DONE-$SEED_ARG
