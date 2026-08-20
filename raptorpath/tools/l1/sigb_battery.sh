#!/bin/bash
# THE ESTIMATOR BATTERY — goal #101 item 2's VM HALF.
#
#   sudo bash sigb_battery.sh <seed> [reps]
#
# Contract: goal-gate "THE SIGMA ESTIMATOR — THE BATTERY, PRE-REGISTRATION"
# (feat/sigma-battery from main@2a1719a), scored against "THE SIGMA ESTIMATOR —
# THE ACCEPTANCE BAR" and against nothing else.
#
# THE UNIT UNDER TEST IS THE ESTIMATOR. NO CLOCK IS TOUCHED. There are no arms:
# `RWM_QUANTILE_CLOCKS` stays OFF, `RWM_ALPHA_OVERRIDE` stays ABSENT, and the
# shipped clamp runs exactly as it ships. The four gauges — `sig_us` (shipped,
# its own control) and the three candidates `rvar_us` / `qsp_us` / `msd_us` —
# are emitted SIDE BY SIDE by one `format!` in `net/diag.rs`, per path, per
# `[DIAG]` block, from the SAME RTT sample stream, in the SAME run. One
# invocation scores all four. That layout IS the design: every comparison is
# PAIRED and none is across sessions.
#
# ONE CONSEQUENCE, STATED SO IT IS NOT MISTAKEN FOR AN OMISSION: **there is no
# arm axis, so there is nothing here for an arm-liveness witness to check.**
# What replaces it is `W7` — the four fields present WITH their `n` counts on
# every path entry of every block. That is the reachability gate for the unit
# under test (MEASUREMENT DISCIPLINE rule 1: prove the mechanism under test
# executes), and it is checked at BOTH endpoints.
#
# CONFIGURATION: `RWM_GEN=0`, THE PLAIN WINDOW — the machine the clock lives
# on. Under generation `recv_nack_tx = None` (`net/mod.rs:2434`), so alpha's
# consumers have no producer and the clock this estimator feeds does not exist
# there. §16.74.5 requirement 3 names the generation seat as a SECOND seat and
# this battery does NOT run it; the pre-registration states that as a scope
# limit and pre-commits that no verdict here is transportable to that seat.
#
# THE INSTRUMENT SET IS `prim_battery_pw.sh`'s AND `alpha_battery.sh`'s,
# UNCHANGED. `RWM_DIAG RWM_FDIAG RWM_ACKDIAG RWM_WALLDIAG RWM_LATPROBE`, all 1.
# This is not a preference: the 287x sigma spread at `c8` that this battery
# exists to answer was measured with exactly this set, so the `sig_us` column
# here POOLS with that reading instead of merely resembling it. `RWM_FDIAG` in
# particular is load-bearing — the receiver's `[RFA] gen=` line is `W1`, and it
# is the ONLY direct engine echo of `window_generation` that exists.
#
# WITNESSES, per invocation, both endpoints. `W3` (`cod=0`) IS NOT CITED — it
# was RETIRED by goal-gate "THE PASSIVE PRIMITIVES — PLAIN WINDOW" §2 (the
# plain window still emits proactive FEC, so `cod` is 111-750 at every lossy
# plain-window cell and it never discriminated the generation axis).
#
#   W1  [RFA] gen= on the receiver               must read 0
#   W2  [PFRAC] lines on the sender              must be 0
#   W4' [DIAG] retx=, MAX OVER ALL LINES         > 0 at lossy cells
#   W5  [RACK] fa=<spur>/<fired> on the sender   present, fired > 0
#   W7  all four gauge tokens with /n counts on every path entry, BOTH sites
#       — THIS BATTERY'S OWN, and the reachability gate for the unit under test
#
# W4' IS A MAXIMUM AND NEVER OFF THE LAST LINE. `retx=` in the [DIAG] tail is
# an INTERVAL counter; read off the last line it reported this witness failing
# at 5 of 15 reps whose [RACK] `fired` on the same run was 11-5717.
#
# THE GOODPUT BAND IS A CONFIGURATION WITNESS HERE, AND IT HAS TEETH THE ALPHA
# SWEEP'S DID NOT. That battery ran treatment arms whose clocks legitimately
# moved goodput, so a band derived from the control could not abort them. This
# battery has NO ARMS — every invocation runs the shipped stack — so an
# out-of-band reading has no legitimate treatment explanation. The
# DISCRIMINATING test is the GENERATION PLATEAU, 26.8-34.1 Mbit/s: a reading
# inside it is the 31 Mbit/s anomaly's own signature and the invocation is
# ABORTED as a configuration fault. A reading outside the cell band but also
# outside the plateau, with W1/W2 clean, is an OUT-OF-BAND RESULT and is
# retained with its cause named — the precedence the plain-window pass fixed
# (`c7` r3's leg collapse) and this battery inherits unchanged.
#
# WATCHER NOTE: `pgrep -f sigb_battery.sh` matches the WATCHER'S OWN shell.
# Watch the SENTINEL or the ledger's SIGB-BATTERY-DONE line — never the process
# table (MEASUREMENT DISCIPLINE 13).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-invocation abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
SB_CELLS="${RWM_SIGB_CELLS:-c1 c7 c8 c8L sc2}"
TAG="${RWM_SIGB_TAG:-sigb}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/sigb/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/sigb/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# INHERITANCE DEFEATS AN ALLOWLIST (gate-forwarding audit, PROBE 0): a var
# exported in this process reaches the binary whatever the forward list says.
# The shipped stack's "absent" can only be made absent by unsetting it HERE.
unset RWM_ALPHA_OVERRIDE
unset RWM_QUANTILE_CLOCKS

# cell -> "scenA scenB mode bytes". TRANSCRIBED verbatim from
# `ccand_battery.sh:202-215` via `alpha_battery.sh:147-156`, never redefined: a
# cell that differs from the ledger's cell is a different cell and its rows do
# not pool. `c8` is the 25 MB cell — the one that failed (E) worst, at the
# smallest converged sample count in the primitives table.
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
cell_legs() { case "$1" in c7|c8|c8L) echo 2 ;; *) echo 1 ;; esac; }

# The committed plain-window bands (primitives-pw amendment §3, field-tested
# 14/15). SECONDARY here — see the header; the plateau is the discriminator.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; c8) echo 50;; c8L) echo 45;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; c8) echo 100;; c8L) echo 95;; sc2) echo 92;; *) echo 99999;; esac; }
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

# The shipped stack is the SUBSTRATE, asserted rather than assumed. A reader
# who takes the pre-flip defaults mis-scales every queue number in the result.
SB_SUBSTRATE_GATES="RWM_DELTA_CAP RWM_SUM_CAP"
SB_CONTAM_GATES="RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS RWM_DERIVED_SWEEP \
RWM_COMPOSED_CAP RWM_THREE_TERM RWM_STORE_CAP_UNIFIED RWM_LATE_BRAKE \
RWM_CHARGE_RECOVERY RWM_RELEASE_1TO1 RWM_LOSS_SENT_TRUTH"

check_and_parse() { # cell rep
  local cell="$1" name="$1"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log

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

  local g want got_c got_s echoline=""
  for g in $SB_SUBSTRATE_GATES $SB_CONTAM_GATES; do
    case " $SB_SUBSTRATE_GATES " in *" $g "*) want=1 ;; *) want=0 ;; esac
    got_c=$(printf '%s' "$gl_c" | grep -o "$g=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$g=[01]")
    echoline="$echoline $g=$got_c/$got_s(exp$want)"
    { [ "$got_c" != "$g=$want" ] || [ "$got_s" != "$g=$want" ]; } \
      && echo "SUBSTRATE-FAIL $name rep=$REP gate=$g cli='$got_c' srv='$got_s' want=$want" >> "$OUT"
  done
  # RWM_ALPHA_OVERRIDE must be ABSENT — `unset` is an ABSENCE, not a value, and
  # the [GATES] echo prints the RESOLVED value, so a leaked export is READ.
  local ao_c ao_s
  ao_c=$(printf '%s' "$gl_c" | grep -o "RWM_ALPHA_OVERRIDE=[^ ]*")
  ao_s=$(printf '%s' "$gl_s" | grep -o "RWM_ALPHA_OVERRIDE=[^ ]*")
  echoline="$echoline RWM_ALPHA_OVERRIDE=${ao_c:-absent}/${ao_s:-absent}(expunset)"
  { [ -n "$ao_c" ] && [ "$ao_c" != "RWM_ALPHA_OVERRIDE=unset" ]; } \
    && echo "SUBSTRATE-FAIL $name rep=$REP gate=RWM_ALPHA_OVERRIDE cli='$ao_c' want=absent" >> "$OUT"

  # The instruments must be armed on BOTH endpoints or their columns are void.
  local i
  for i in RWM_DIAG RWM_FDIAG RWM_ACKDIAG RWM_WALLDIAG; do
    got_c=$(printf '%s' "$gl_c" | grep -o "$i=[01]")
    got_s=$(printf '%s' "$gl_s" | grep -o "$i=[01]")
    echoline="$echoline $i=$got_c/$got_s(exp1)"
    { [ "$got_c" != "$i=1" ] || [ "$got_s" != "$i=1" ]; } \
      && echo "INSTRUMENT-FAIL-GATE $name rep=$REP gate=$i cli='$got_c' srv='$got_s'" >> "$OUT"
  done
  echo "LIVENESS $name rep=$REP$echoline" >> "$OUT"

  # ── THE LEDGER ROWS. RAW: every emission, warm-up included, `-` kept as `-`.
  #    The clause-C1 exclusions are applied by sigb_report.py against the `n`
  #    on each row, so they are re-derivable from the ledger alone (clause C3).
  local PFS=() li
  for li in $(seq 0 $(( $(cell_legs "$cell") - 1 ))); do
    [ -f "/tmp/rwm-ping-$li.txt" ] && PFS+=("/tmp/rwm-ping-$li.txt")
  done
  local QF=/tmp/rwm-q.txt; [ -f "$QF" ] || QF=-
  # `${PFS[@]+...}` and not `${PFS[@]}`: `set -u` is on (lib.sh) and an EMPTY
  # array is an unbound reference under bash < 4.4. A rep whose probe produced
  # no file at all must still land its gauge rows — the estimator is the unit
  # under test and clause S does not read the probe.
  python3 ./sigb_parse.py "$cell" "$SEED_ARG" "$REP" "$C" "$S" "$QF" \
      ${PFS[@]+"${PFS[@]}"} \
    >> "$OUT" 2>&1 || echo "SIGB-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── W7 AT THE DRIVER, INDEPENDENTLY OF THE PARSER ────────────────────────
  # The parser counts group misses; this counts raw token presence. Two
  # independent readings of the same reachability fact, because the parser's
  # own regex is the thing a token change would break first.
  local t f n_c n_s
  for t in sig_us rvar_us qsp_us msd_us; do
    n_c=$(grep -o "$t=[0-9-]*/n[0-9]*" "$C" 2>/dev/null | wc -l)
    n_s=$(grep -o "$t=[0-9-]*/n[0-9]*" "$S" 2>/dev/null | wc -l)
    echo "W7TOKEN $name rep=$REP $t cli=$n_c srv=$n_s" >> "$OUT"
    [ "$n_c" -eq 0 ] && echo "W7-FAIL-CLI $name rep=$REP token=$t (absent on the sender)" >> "$OUT"
  done

  # The verbatim gauge dump for the lines the report does NOT read, so a later
  # reader can re-derive any column of the result from the ledger alone.
  for f in RACK RFA FDIAG; do
    (grep -h "\[$f\]" "$C" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" "$S" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  # ── THE BAND, AND THE PLATEAU THAT ACTUALLY ABORTS ──────────────────────
  local mb lo hi lossy inband plateau
  mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
  lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
  inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')
  plateau=$(awk -v m="$mb" 'BEGIN{print (m>=26.8 && m<=34.1)?1:0}')
  [ "$plateau" -eq 1 ] \
    && echo "ABORT-GEN-PLATEAU $name rep=$REP mbps=$mb (26.8-34.1 = generation leaked in; row VOID)" >> "$OUT"
  [ "$inband" -eq 0 ] && [ "$plateau" -eq 0 ] \
    && echo "OUT-OF-BAND $name rep=$REP mbps=$mb band=[$lo,$hi] (RESULT, not an abort — cause to be named)" >> "$OUT"

  echo "SIGBBAND {\"cell\":\"$cell\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$RC,\"mbps\":$mb,\"band\":[$lo,$hi],\"in_band\":$inband,\"gen_plateau\":$plateau,\"lossy\":$lossy}" \
    | tee -a "/home/vibe/sigb/${TAG}-witness-s${SEED_ARG}.jsonl" >> "$OUT"
}

run_cell() { # cell
  local cell="$1" name="$1"
  local ca cb mode bytes
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP cell=$name seed=$SEED_ARG spec=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # cell's log and pass its liveness gate on it.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt \
        /tmp/rwm-ping.txt /tmp/rwm-ping-0.txt /tmp/rwm-ping-1.txt \
        /tmp/rwm-ping-2.txt /tmp/rwm-ping-3.txt

  env SEED=$SEED_ARG RWM_GEN=0 \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  RC=${PIPESTATUS[0]}
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$RC" >> "$OUT"

  check_and_parse "$cell" "$REP"

  local pn; pn=$(grep -c "time=" /tmp/rwm-ping-0.txt 2>/dev/null || true); pn="${pn:-0}"
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP (no probe replies on leg 0)" >> "$OUT"

  # Per-rep captures. The driver's `trap cleanup EXIT` destroys the namespaces
  # the instant it returns, so these are copied under rep-unique names now.
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  local li
  for li in 0 1 2 3; do
    [ -f "/tmp/rwm-ping-$li.txt" ] \
      && cp "/tmp/rwm-ping-$li.txt" "$DDIR/${name}-s${SEED_ARG}-r${REP}-p${li}.txt" 2>/dev/null
  done
  cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

{
  echo "=== SIGB BATTERY seed=$SEED_ARG reps=$REPS $(date -u +%FT%TZ)"
  echo "CONTRACT goal-gate 'THE SIGMA ESTIMATOR — THE BATTERY, PRE-REGISTRATION' (feat/sigma-battery from main@2a1719a)"
  echo "SCORED-AGAINST goal-gate 'THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR' and nothing else"
  echo "CELLS $SB_CELLS"
  echo "ARMS  NONE — the shipped stack, RWM_QUANTILE_CLOCKS OFF, RWM_ALPHA_OVERRIDE ABSENT. No clock is touched."
  echo "SEAT  plain window (RWM_GEN=0). The generation seat of §16.74.5 req 3 is NOT run — see the pre-registration's scope clause."
  echo "ENV   RWM_GEN=0 RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1"
  echo "W3    NOT CITED (RETIRED by primitives-pw §2 — cod= is 111-750 at every lossy plain-window cell)"
  echo "BIN $BIN"
  echo "SHA256 $(sha256sum "$BIN" 2>/dev/null)"
  echo "COMMIT $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "KERNEL $(uname -r)"
  echo "UPTIME $(uptime)"
  echo "COTENANT kwin=$(pgrep -c kwin_x11 2>/dev/null || echo 0) sddm=$(pgrep -c sddm 2>/dev/null || echo 0)"
  echo "CPU $(lscpu | grep -E 'Model name' | head -1)"
} >> "$OUT"

RC=0
# REP-OUTER / CELL-INNER, so a truncated run carries BALANCED n across cells
# rather than a complete prefix and an empty tail.
for REP in $(seq 1 "$REPS"); do
  for CELL in $SB_CELLS; do
    run_cell "$CELL"
  done
done

echo "=== CELLCOUNTS (rows, NOT live n — see sigb_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $SB_CELLS; do
  N=$(grep -c "\"cell\":\"$CELL\"" "$OUT" || true); N="${N:-0}"
  echo "CELLCOUNT $CELL rows=$N/$REPS" >> "$OUT"
  [ "$N" -eq 0 ] && echo "CELL-VANISHED $CELL" >> "$OUT"
done
echo "SIGB-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo SIGB-BATTERY-DONE-$SEED_ARG
