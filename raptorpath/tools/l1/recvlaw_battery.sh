#!/bin/bash
# THE RECEIVER-LAW BATTERY — paper §16.83, Track C arms (A) and (B).
#
#   sudo bash recvlaw_battery.sh <seed> [reps]
#
# Contract: goal-gate "THE RECEIVER-LAW BATTERY — PRE-REGISTRATION"
# (feat/recvlaw-arms from main@9396ca0). Move the REPAIR DECISION to the
# receiver's seat and measure what it costs and what it buys.
#
# FOUR ARMS, PAIRED WITHIN A REP, ARMS INNERMOST — the alpha/ccand/era layout,
# so the four arms of one cell run adjacent on ONE freshly built topology and
# the contrast is paired:
#
#   CTL  both gates ABSENT.  The shipped machine: the receiver advertises
#        SACK ranges, the SENDER inverts them into gaps, and `[FCAUSE]
#        gap_data` is the dominant fire class (D0: retx 2 425-15 791/cell).
#   A    RWM_RECV_REQUEST_LAW=1.  The receiver REQUESTS at lateness
#        `l >= l*_recv`, `m = 1` (the COPY — today's bytes, so `[RFA] dup_src`
#        stays comparable to CTL).  The per-seq SACK->gap producer is
#        suppressed by the collision seam; `sack_tx` is UNTOUCHED (ADR-0060).
#        THE TIMING LEVER, ISOLATED.
#   B    RWM_RANK_FEEDBACK=1 alone.  The shipped 2 ms trigger spoken in the
#        deficit vocabulary: `m = clamp(ceil(k_half(pi0)), 1, A*)`, the seam
#        stays OPEN.  THE VOCABULARY LEVER, ISOLATED — and the wiring test.
#   AB   both.  The composition.
#
# CELLS.  c7 and c8 are SCORED; c1 and sc2 are MUST-NOT-MOVE CONTROLS with
# `lstar_us = 0` and `m = 1` pre-declared (16.83.2's `pi0 -> 0` limit IS the
# shipped machine there).  **A CONTROL THAT MOVES VOIDS THE RUN.**
#
# THE SCORED DIMENSIONS ARE TWO, AND `dU` IS NOT ONE OF THEM.
#   (1) the REALIZED FALSE FRACTION at the receiver (`[RFA] false_frac`), a
#       binomial on ~80 k holes per rep — which is powered at n = 3;
#   (2) WORST-LEG DELIVERED LATENCY (`[LAT] tot_p99` at the receiver, and the
#       ping probe beside it).
#   `dU` (goodput) IS A GUARD, NOT A SCORE.  The n for a scored `dU` at these
#   cells is 559 reps at c7 and 196 at c8 — INFEASIBLE — and saying so in
#   ADVANCE is what stops an underpowered throughput leg from being quoted as
#   a win.  An arm that leaves the CTL goodput spread is REFUTED on the guard;
#   an arm inside it has said nothing about goodput either way.
#
# PREDICTIONS, VERBATIM FROM §16.83.6, STATED BEFORE THE RUN:
#   * arm (A), at the DUALS: the realized false fraction falls by a factor
#     `1/(1 - F(l*))` — **c7 in [1.03, 2), c8 in [2, 10]** — with goodput
#     inside the CTL spread, and `knee_bind ~ 1` at c7.
#   * arm (B): `dup_src -> 0` BY CONSTRUCTION.  That is a witness of WIRING
#     and NOT a result; the result is `rep_redundant` and the `[RFA]` class
#     migration `dup_src -> preempt_src`.
#   * the CONTROLS: `lstar_us = 0`, `m_max = 1`, and no movement in any
#     scored column.
#   REFUTERS, also pre-stated: the false fraction does not move; or
#   `sampler_bind ~ 1` (the 2 ms `GAP_ACK_MIN_INTERVAL`, not the law, set the
#   time); or goodput leaves the CTL spread; or (B)'s `rep_redundant` rises
#   without the false fraction falling; or `wa1_none` shows the span answers
#   degrading to copies anyway.
#
# **THE KNEE-BOUND OUTCOME IS LIVE AND IS NOT A FOOTNOTE.** The `[LATE]`
# instrument's own loopback reading is `lstar_us = 0` with `knee_bind = 1.0`
# on BOTH L0 topologies, because `d` (the mean ARQ resolution) exceeds the
# observed knee `H` there so `(H - d)+ = 0`.  If the same holds at the L1
# duals, the request lateness is not set by the lateness distribution at all —
# it is set by the store's free headroom, i.e. by `RWM_STORE_GAIN = 2.0`,
# which is UNPROVENANCED.  **THE REPAIR LAW WOULD THEN BE THE STORE-CAP LAW
# WEARING A CLOCK**, and the verdict is a LEDGER verdict naming
# `RWM_STORE_GAIN` — not a performance finding.  `WK` carries the gauge on
# every row so this is READ and never inferred.
#
# WITNESSES, per invocation, both endpoints.
#   W1  [RFA] gen= on the receiver                     must read 0
#   W2  [PFRAC] lines on the sender                    the proactive plane
#   W3  [DIAG] retx=, MAX OVER ALL LINES               > 0 at lossy cells
#   W4  [RACK] fa=<spur>/<fired> on the sender         present
#   W5  [GATES] RWM_RECV_REQUEST_LAW / RWM_RANK_FEEDBACK at BOTH endpoints
#       THIS BATTERY'S OWN AXIS.  A row whose arm is not readable off its own
#       log is VOID — the 31 Mbit/s anomaly's own failure mode.
#   W6  [LATE] n= on the receiver                      > 0 at lossy cells
#   W7  [SUCC] det= on the receiver                    > 0 — the INDEPENDENT
#       hole witness, different code, same holes
#   W8  [FCAUSE] n= on the sender                      the fire plane exists
#   WA1 [REQS] wa1_some / wa1_none                     16.83.3's SOUNDNESS
#       PRECONDITION, COUNTED: `generate_repair_range` refuses unless the
#       whole span is retained, and a request law whose answers silently
#       degrade to copies is the shipped machine with extra latency.
#   WA2 [REQS] m_max / coded                           m > 1 spans SERVED
#   WA3 [REQS] stale / budget_bound                    the serving loop's own
#       refusals — an arm that is always budget-bound is not measuring its law
#   WL1 [LATE] lstar_us                                > 2000 at the duals,
#       = 0 at the singles.  **This is the L1 witness; L0 loopback CANNOT
#       produce it (see the KNEE-BOUND note above) and the reachability test
#       deliberately does not assert it.**
#   WL2 [REQ] sent= at the receiver AND [REQS] served= at the sender
#       > 0 on A/B/AB, = 0 on CTL.  MEASUREMENT DISCIPLINE rule 1 at BOTH
#       seats: a one-sided reading cannot tell "never built" from "never
#       served".
#   WK  [LATE] knee_bind= and sampler_bind=            echoed on every row
#
# W3 IS READ AS A MAXIMUM AND NEVER OFF THE LAST LINE — `retx=` in the [DIAG]
# tail is an INTERVAL counter (the alpha battery's own W4' lesson).
#
# THE GOODPUT BANDS APPLY TO CTL ONLY.  They were measured on the shipped
# machine.  On a treatment arm an out-of-band reading is a RESULT, printed as
# OUT-OF-BAND, never an abort.
#
# WATCHER NOTE: `pgrep -f recvlaw_battery.sh` matches the WATCHER'S OWN shell.
# Watch the ledger's RECVLAW-BATTERY-DONE line — never the process table
# (discipline 13).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1 || { echo "ABORT-CD tools/l1"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard recvlaw_battery.sh lib.sh lib_battery.sh
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-3}"
RL_CELLS="${RWM_RECVLAW_CELLS:-c1 sc2 c7 c8}"
RL_ARMS="${RWM_RECVLAW_ARMS:-CTL A B AB}"
TAG="${RWM_RECVLAW_TAG:-recvlaw}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/recvlaw/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/recvlaw/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (discipline 15's corollary): a var exported
# in this process reaches the binary whatever the forward list says, so the
# CONTROL arm's "absent" can only be made absent by unsetting it here.
unset RWM_RECV_REQUEST_LAW
unset RWM_RANK_FEEDBACK

# ── BOTH LOCKS (goal-gate "THE VM PROTOCOL"; lib_battery.sh's, with the
#    INT/TERM handlers that EXIT) ────────────────────────────────────────────
# `/tmp/rwm-vm.lock` is the box lock and `/home/vibe/rp.lock` the tree lock.
# They are OPERATOR locks — this script does not invent a third mechanism —
# but it REFUSES to run without them and it releases exactly what it took, so
# `ABORT-LOCK` is a reading of this ledger and not an assurance in a report.
# `noclobber` makes the create-or-fail atomic against a second launcher.
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
LB_TAG=recvlaw_battery
LB_LOG="$OUT"
# On INT/TERM the handler must EXIT after releasing: a `trap 'f' INT TERM`
# body that does not `exit` RESUMES the script (the r-battery of 2026-09-08 ran
# on for hours after TERM with both locks already cleared).
install_lock_traps
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT"
  exit 3
fi

# ── THE ARM TABLE — the SINGLE source of both the arm's env and the arm's
#    liveness assertion, so the two cannot drift apart. ────────────────────
RL_ARM_GATES="RWM_RECV_REQUEST_LAW RWM_RANK_FEEDBACK"
# The substrate: shipped-ON laws this battery RUNS ON rather than sweeps.
# Asserted =1 rather than assumed, because a reader who takes the pre-flip
# default mis-scales every queue number in the result.
RL_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP RWM_STORE_SACK_RELEASE"
RL_CONTAM_GATES="RWM_RACK_CLOCKS RWM_DERIVED_SWEEP RWM_QUANTILE_CLOCKS \
RWM_COMPOSED_CAP RWM_THREE_TERM RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE \
RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 RWM_LOSS_SENT_TRUTH RWM_NO_REACTIVE \
RWM_COMPLETION_EXPOSURE"

gate_expect() { # arm gate -> expected [GATES] value
  case "$2" in
    RWM_RECV_REQUEST_LAW) case "$1" in A|AB) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_RANK_FEEDBACK)    case "$1" in B|AB) echo 1 ;; *) echo 0 ;; esac ;;
    RWM_DELTA_CAP)           echo 1 ;;
    RWM_SUM_CAP)             echo 1 ;;
    RWM_STORE_SACK_RELEASE)  echo 1 ;;
    *) echo 0 ;;
  esac
}

# The arm's env, DERIVED from the table above. A gate whose expectation is 0
# is passed as `=0` rather than omitted: both gates are plain `env_flag`s, so
# `=0` and absent resolve identically, and passing the token makes the
# CONTROL's own arm-liveness check assert an ECHO rather than an absence.
arm_env() { # arm -> "RWM_X=v ..."
  local a="$1" g out="" v
  for g in $RL_ARM_GATES $RL_SUBSTRATE_GATES $RL_CONTAM_GATES; do
    v="$(gate_expect "$a" "$g")"
    out="$out $g=$v"
  done
  echo "${out# }"
}

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# alpha_battery.sh:150-160 (itself from ccand_battery.sh:202-215) and never
# redefined: a cell that differs from the ledger's cell is a different cell
# and its rows do not pool.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    c8)  echo "c2 c3 dual    25000000" ;;
    *) echo "" ;;
  esac
}
cell_paths() { case "$1" in c7|c8) echo 2 ;; *) echo 1 ;; esac; }
# The MUST-NOT-MOVE controls. `lstar_us = 0` and `m_max = 1` are PRE-DECLARED
# there; a violation VOIDS the run rather than becoming a finding.
is_control_cell() { case "$1" in c1|sc2) echo 1 ;; *) echo 0 ;; esac; }

# The plain-window amendment's §3 bands, transcribed unchanged. CTL ONLY.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; c8) echo 50;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; c8) echo 100;; sc2) echo 92;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

arm_cell_reps() { echo "$REPS"; }

check_and_parse() { # name cell arm cpus cpuc pingp qp
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log

  python3 ./recvlaw_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      "$C" "$S" "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "RECVLAW-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the per-mechanism ACTIVE echoes' own prose
  # contains literal `RWM_*=0` strings (the flip battery's amendment-1 lesson).
  local gl_c gl_s
  gl_c=$(grep "\[GATES\]" "$C" 2>/dev/null | tail -1)
  gl_s=$(grep "\[GATES\]" "$S" 2>/dev/null | tail -1)

  # ABORT-CAUSE FIRST. No [GATES] on EITHER endpoint = ABORT: no datum, no
  # liveness verdict, and NOT in any denominator.
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  # ── W5: THE ARM'S OWN AXIS, AT BOTH ENDPOINTS ───────────────────────────
  # The request law is consumed at the RECEIVER (which builds the message) AND
  # at the SENDER (which serves it, and whose gap producer the seam
  # suppresses), so the control's ABSENCE must be as mechanically assertable
  # as the arm's presence.
  local g want got_c got_s echoline=""
  for g in $RL_ARM_GATES $RL_SUBSTRATE_GATES $RL_CONTAM_GATES; do
    want="$(gate_expect "$arm" "$g")"
    got_c=$(printf '%s' "$gl_c" | grep -o "$g=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$g=[01]")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    case " $RL_ARM_GATES $RL_SUBSTRATE_GATES " in
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

  # The verbatim gauge dump — every line, both sites, so a later reader can
  # re-derive any column of the report from the ledger alone.
  local f
  for f in REQ REQS LATE RANK RFA FCAUSE RACK SUCC LAT; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── WL2: THE MECHANISM EXECUTED AT BOTH SEATS ───────────────────────────
  local rq_sent rq_served
  rq_sent=$(grep -o '\[REQ\] .*sent=[0-9]*' "$S" 2>/dev/null | tail -1 \
             | grep -o 'sent=[0-9]*' | tr -dc '0-9'); rq_sent="${rq_sent:-0}"
  rq_served=$(grep -o '\[REQS\] .*served=[0-9]*' "$C" 2>/dev/null | tail -1 \
             | grep -o 'served=[0-9]*' | tr -dc '0-9'); rq_served="${rq_served:-0}"
  case "$arm" in
    CTL)
      { [ "$rq_sent" -ne 0 ] || [ "$rq_served" -ne 0 ]; } \
        && echo "WL2-FAIL-CTL $name rep=$REP (the control built/served a request: sent=$rq_sent served=$rq_served)" >> "$OUT"
      ;;
    *)
      [ "$rq_sent" -eq 0 ] \
        && echo "WL2-FAIL-BUILD $name rep=$REP (the receiver never built a RepairRequest; row VOID)" >> "$OUT"
      [ "$rq_served" -eq 0 ] \
        && echo "WL2-FAIL-SERVE $name rep=$REP (requests were built and none served; row VOID)" >> "$OUT"
      ;;
  esac

  # ── THE SEAM, AS A READING ──────────────────────────────────────────────
  # §16.83.4's identifiability argument is CONDITIONAL on the receiver being
  # the single authority. On arms A and AB `gap_data` must be 0; on CTL and B
  # it must NOT be, or the contrast has no control.
  local gapd
  gapd=$(grep -o '\[FCAUSE\] .*gap_data=[0-9]*' "$C" 2>/dev/null | tail -1 \
          | grep -o 'gap_data=[0-9]*' | tr -dc '0-9'); gapd="${gapd:-0}"
  case "$arm" in
    A|AB) [ "$gapd" -ne 0 ] \
      && echo "SEAM-OPEN $name rep=$REP (gap_data=$gapd with the request law armed — a copy still flies inside [0,l*), so rho_heal is CENSORED and the row measures a different law)" >> "$OUT" ;;
    *)    [ "$gapd" -eq 0 ] && [ "$(is_lossy "$cell")" = "1" ] \
      && echo "SEAM-VACUOUS $name rep=$REP (gap_data=0 on a control arm at a lossy cell — the treatment's gap_data=0 proves nothing)" >> "$OUT" ;;
  esac

  # ── THE MUST-NOT-MOVE CONTROL CELLS ─────────────────────────────────────
  # 16.83.2: `pi0 -> 0` gives `l* = 0` and `k_half < 1` gives `m = 1`. Those
  # are the LAW's own limits at the single-path cells, not a configuration —
  # so a violation VOIDS the run rather than becoming a finding.
  if [ "$(is_control_cell "$cell")" = "1" ]; then
    local cl cm
    cl=$(grep -o '\[REQ\] .*lstar_us=[0-9]*' "$S" 2>/dev/null | tail -1 \
          | grep -o 'lstar_us=[0-9]*' | tr -dc '0-9'); cl="${cl:-0}"
    cm=$(grep -o '\[REQ\] .*m_max=[0-9]*' "$S" 2>/dev/null | tail -1 \
          | grep -o 'm_max=[0-9]*' | tr -dc '0-9'); cm="${cm:-0}"
    [ "$cl" -ne 0 ] && echo "CONTROL-MOVED $name rep=$REP (lstar_us=$cl at a single-path cell; 16.83.2 pre-declares 0 — RUN VOID)" >> "$OUT"
    [ "$cm" -gt 1 ] && echo "CONTROL-MOVED $name rep=$REP (m_max=$cm at a single-path cell; k_half(pi0)<1 pre-declares 1 — RUN VOID)" >> "$OUT"
  fi

  # ── W1..W8 + WA1..WA3 + WL1 + WK + the band, into one JSONL witness row ──
  local w1 w2 w3 w4 w6 w7 w8 mb lo hi lossy inband
  w1=$(grep -o '\[RFA\] gen=[01]' "$S" 2>/dev/null | tail -1 | sed 's/.*gen=//'); w1="${w1:-none}"
  w2=$(grep -c '\[PFRAC\]' "$C" 2>/dev/null || true); w2="${w2:-0}"
  # THE MAXIMUM, never the last line — `retx=` is an INTERVAL counter.
  w3=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tr -dc '0-9\n' | sort -n | tail -1); w3="${w3:-0}"
  w4=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); w4="${w4:-none}"
  w6=$(grep -o '\[LATE\] n=[0-9]*' "$S" 2>/dev/null | tail -1 | grep -o '[0-9]*$'); w6="${w6:-0}"
  w7=$(grep -o '\[SUCC\] .*det=[0-9]*' "$S" 2>/dev/null | tail -1 | grep -o 'det=[0-9]*' | tr -dc '0-9'); w7="${w7:-0}"
  w8=$(grep -o '\[FCAUSE\] .*n=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o ' n=[0-9]*' | tr -dc '0-9'); w8="${w8:-0}"

  local wa1s wa1n wa2m wa2c wa3s wa3b wl1 wk wsamp
  wa1s=$(grep -o '\[REQS\] .*wa1_some=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'wa1_some=[0-9]*' | tr -dc '0-9'); wa1s="${wa1s:-0}"
  wa1n=$(grep -o '\[REQS\] .*wa1_none=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'wa1_none=[0-9]*' | tr -dc '0-9'); wa1n="${wa1n:-0}"
  wa2m=$(grep -o '\[REQS\] .*m_max=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'm_max=[0-9]*' | tr -dc '0-9'); wa2m="${wa2m:-0}"
  wa2c=$(grep -o '\[REQS\] .*coded=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'coded=[0-9]*' | tr -dc '0-9'); wa2c="${wa2c:-0}"
  wa3s=$(grep -o '\[REQS\] .*stale=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'stale=[0-9]*' | tr -dc '0-9'); wa3s="${wa3s:-0}"
  wa3b=$(grep -o '\[REQS\] .*budget_bound=[0-9]*' "$C" 2>/dev/null | tail -1 | grep -o 'budget_bound=[0-9]*' | tr -dc '0-9'); wa3b="${wa3b:-0}"
  wl1=$(grep -o '\[LATE\] .*lstar_us=[0-9-]*' "$S" 2>/dev/null | tail -1 | sed 's/.*lstar_us=//'); wl1="${wl1:-none}"
  wk=$(grep -o '\[LATE\] .*knee_bind=[0-9.-]*' "$S" 2>/dev/null | tail -1 | sed 's/.*knee_bind=//'); wk="${wk:-none}"
  wsamp=$(grep -o '\[LATE\] .*sampler_bind=[0-9.-]*' "$S" 2>/dev/null | tail -1 | sed 's/.*sampler_bind=//'); wsamp="${wsamp:-none}"

  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  # BAND SCOPE: CTL only. On a treatment arm an out-of-band reading is a
  # RESULT, and `band_applies` says so in the row rather than in a footnote.
  local applies=0; [ "$arm" = "CTL" ] && applies=1

  echo "RECVLAWWITNESS {\"cell\":\"$cell\",\"arm\":\"$arm\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"band_applies\":$applies,\"in_band\":$inband,\"lossy\":$lossy,\"control_cell\":$(is_control_cell "$cell"),\"WL2_req_sent\":$rq_sent,\"WL2_req_served\":$rq_served,\"seam_gap_data\":$gapd,\"W1_rfa_gen\":\"$w1\",\"W2_pfrac_lines\":$w2,\"W3_retx_max\":$w3,\"W4_rack_fa\":\"$w4\",\"W6_late_n\":$w6,\"W7_succ_det\":$w7,\"W8_fcause_n\":$w8,\"WA1_some\":$wa1s,\"WA1_none\":$wa1n,\"WA2_m_max\":$wa2m,\"WA2_coded\":$wa2c,\"WA3_stale\":$wa3s,\"WA3_budget_bound\":$wa3b,\"WL1_lstar_us\":\"$wl1\",\"WK_knee_bind\":\"$wk\",\"WK_sampler_bind\":\"$wsamp\"}" \
    | tee -a "/home/vibe/recvlaw/${TAG}-witness-s${SEED_ARG}.jsonl" >> "$OUT"
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
  # arm's log and pass its liveness gate on it.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # RWM_GEN=0 ON EVERY ARM, and this one is STRUCTURAL rather than a
  # preference: under generation coding `recv_nack_tx` is already `None`,
  # the per-seq layer this battery's seam suppresses does not exist, and
  # `request_law_armed` returns false — so a generation run would give four
  # identical arms and a clean FALSE NULL. (The alpha battery's own §7.)
  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT"
  # No `|| true` here: it would run whenever the pipeline failed and replace
  # PIPESTATUS with true's 0, so a failed engine read rc=0.
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

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
  case " $RL_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $RL_ARMS "  in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

{
  echo "=== RECVLAW BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate 'THE RECEIVER-LAW BATTERY — PRE-REGISTRATION' (feat/recvlaw-arms from main@9396ca0)"
  echo "PAPER 16.83 arms (A) RWM_RECV_REQUEST_LAW and (B) RWM_RANK_FEEDBACK; (C) the S5 loop is NOT built"
  echo "CELLS $RL_CELLS   (c7,c8 SCORED; c1,sc2 MUST-NOT-MOVE controls)"
  echo "ARMS  $RL_ARMS   (paired within rep, ARMS INNERMOST)"
  for A in $RL_ARMS; do echo "ARMENV $A | $(arm_env "$A")"; done
  echo "SCORED realized false fraction ([RFA] false_frac, binomial on ~80k holes/rep) + worst-leg delivered latency ([LAT] tot_p99, ping probe)"
  echo "GUARD  dU is a GUARD and NOT a score: n for a scored dU is 559 reps at c7 and 196 at c8 — INFEASIBLE"
  echo "PREDICT A at duals: false fraction falls by 1/(1-F(l*)) — c7 in [1.03,2), c8 in [2,10]; knee_bind ~ 1 at c7"
  echo "PREDICT B: dup_src -> 0 BY CONSTRUCTION (a WIRING witness, not a result); rep_redundant + the [RFA] class migration is the result"
  echo "PREDICT controls: lstar_us=0, m_max=1, no scored column moves"
  echo "REFUTERS false fraction does not move; sampler_bind ~ 1; goodput leaves the CTL spread; rep_redundant rises without the false fraction falling; wa1_none shows the answers degrading to copies"
  echo "OUTCOMES the goal-gate lever set + KNEE-BOUND (the cap binds >= 95% => a LEDGER verdict naming RWM_STORE_GAIN) + UNSCOREABLE"
  echo "BANDSCOPE the goodput abort bands apply to CTL ONLY; out-of-band on a treatment arm is a RESULT"
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
  for CELL in $RL_CELLS; do
    for ARM in $RL_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $RL_CELLS; do
  for A in $RL_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true); N="${N:-0}"
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "RECVLAW-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo RECVLAW-BATTERY-DONE-$SEED_ARG
