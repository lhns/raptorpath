#!/bin/bash
# GOAL "THREE TERMS, NO CONSTANTS" phase 1.4 — the L1 battery, topo.sh cells.
# Scored against goal-gate "Three-Term Law — PRE-REGISTRATION" (commit
# 70833cd). That block is the CONTRACT; nothing here may modify it.
#
# THREE ARMS, and the third one is why this driver exists:
#
#   A  baseline   env unset                            (the shipped default)
#   B  the law    RWM_THREE_TERM=1 RWM_PLAIN_RS=1      (the SCORED arm)
#   C  control    RWM_THREE_TERM=1                     (prices the anchor)
#   D  attribution RWM_PLAIN_RS=1                      (the OTHER half of B)
#
# Arm D is not scored and changes no verdict. It exists because arm B — the
# arm the pre-registration mandates — composes TWO gates, so a movement at B
# is ambiguous between the LAW and the honest RATE ANCHOR it needs. D holds
# the anchor and drops the law; B − D is the law's own contribution and
# D − A is the anchor's. Measuring that is attribution, not tuning: it
# decides which of the two owns a result, it cannot rescue either.
#
# The pre-registration's ANCHOR CAVEAT: the law is LINEAR in `rate_i` and the
# shipped default anchor over-reads x4.6-7.4, so `RWM_THREE_TERM=1` ALONE
# clamps at WIN_STORE_MAX=4096 nearly everywhere and merely reproduces the
# x4096 arm. Arm C is therefore a DIAGNOSTIC, not the test; arm B is the arm
# the goal is scored on. Both are run so the anchor's price is measured, not
# argued.
#
# Cells (interleaved round-robin per rep — discipline 3 — 1 run per
# invocation, fresh tunnel per invocation):
#   c1     c1/c1 single 400MB   criterion 4 (the banked +16-25% floor)
#   c7     c2/c2 dual   200MB   criterion 5 (>=0.97xSigma; the SYMMETRIC dual,
#                               where the span term must read ZERO at N=2)
#   c8     c2/c3 dual    25MB   criterion 4 (>=0.87xSigma; the diagnosed cell)
#   sc2    c2/c2 single 100MB   criterion 5 (within sigma) + c7's Sigma term
#   sc3    c3/c3 single  25MB   criterion 5 (within sigma) + c8's Sigma term
#   c2r100 single         50MB  criterion 3 HELD-OUT (pred x3.30, +25..+60%)
#   c2r200 single         50MB  criterion 3 HELD-OUT but PRE-REGISTERED AS
#                               CLAMPED (B2): it can neither confirm nor
#                               refute the law. Run, reported as removed from
#                               the experiment by the memory ceiling.
#
# Liveness, per arm and per direction, BEFORE any number is read (discipline
# 1/15): the `[GATES]` resolved values on BOTH endpoints (the arm shows
# THREE_TERM=1 and the control shows THREE_TERM=0 on client AND server), the
# resolve-time `three-term outstanding limit ACTIVE` echo, and the per-2s
# `[3T] eng=1`. An arm whose `[3T]` never reaches eng=1 is VOID and must be
# re-run, not explained — flagged here as ARM-LIVENESS-FAIL-3T.
#
# MUST run as root (`sudo bash tt_battery.sh <seed> [reps]`, the adv_battery
# precedent): the per-arm stale-echo hygiene `rm -f /tmp/rwm-{c,s}.log` acts
# on files the sudo'd harness wrote as root, and /tmp is sticky.
#
#   usage: sudo bash tt_battery.sh <seed> [reps]
set -u
[ "$(id -u)" -eq 0 ] || { echo "tt_battery.sh must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
TT_CELLS="${RWM_TT_CELLS:-c1 c7 c8 sc2 sc3 c2r100 c2r200}"
TT_TAG="${RWM_TT_TAG:-battery}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/threeterm/${TT_TAG}-s${SEED_ARG}.log
DDIR=/home/vibe/threeterm/diag
mkdir -p "$DDIR" /home/vibe/threeterm
: > "$OUT"
{
  echo "# three-term battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS"
  echo "# binary: $(sha256sum $BIN)"
  echo "# source: $(cat /home/vibe/raptorpath/COMMIT)"
  echo "# kernel: $(uname -r)"
  lscpu | grep "Model name"
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ') || true
  echo
  echo "# arms: A='' B='RWM_THREE_TERM=1 RWM_PLAIN_RS=1' C='RWM_THREE_TERM=1' D='RWM_PLAIN_RS=1'"
  echo "# base env: SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 (every arm)"
} >> "$OUT"

LAW="RWM_THREE_TERM=1 RWM_PLAIN_RS=1"
CTL="RWM_THREE_TERM=1"
ANC="RWM_PLAIN_RS=1"

run_one() { # cell arm envs cellA cellB mode bytes exp_3t exp_rs
  local cell="$1" arm="$2" envs="$3" ca="$4" cb="$5" mode="$6" bytes="$7"
  local e3t="$8" ers="$9"
  local name="$cell-$arm"
  # TOP-UP support (RWM_TT_CELLS): restrict the schedule to a subset of cells
  # while keeping the arm interleaving inside each rep byte-identical. Used to
  # RE-RUN arms voided by the documented seed-7 topo-ping abort class
  # (discipline 8) rather than explain their reduced n away.
  case " $TT_CELLS " in *" $cell "*) ;; *) return 0 ;; esac
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene (the copaclean s7 lesson): an aborted invocation must
  # never be able to read the PREVIOUS arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:|GUARD" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  # ONE parser, shared with tt_adv.sh (tt_parse.py).
  python3 ./tt_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" /tmp/rwm-c.log /tmp/rwm-s.log \
    >> "$OUT" 2>&1 || echo "TTRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── LIVENESS, asserted TWO-SIDED on BOTH logs (discipline 15c) ─────────
  local g3c g3s grc grs act eng
  # MUST be scoped to the `[GATES]` line. The resolve-time ACTIVE echo's own
  # PROSE contains the literal string `RWM_THREE_TERM=0 = the shipped-default
  # control arm)`, so an unscoped `grep | tail -1` reads the documentation
  # instead of the resolved value and reports every ON arm as OFF (caught by
  # the pre-battery smoke, which showed active=1 eng1_lines=10 beside a
  # "GATE-CLI got=RWM_THREE_TERM=0" — the mechanism was live and the ASSERTION
  # was wrong).
  g3c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  g3s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  grc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  grs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  act=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  eng=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep '\[3T\]' | grep -c "eng=1" || true)
  echo "LIVENESS $name rep=$REP cli=[$g3c $grc] srv=[$g3s $grs] active=$act eng1_lines=$eng (expect 3t=$e3t rs=$ers)" >> "$OUT"
  [ "$g3c" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-CLI $name rep=$REP got='$g3c'" >> "$OUT"
  [ "$g3s" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-SRV $name rep=$REP got='$g3s'" >> "$OUT"
  [ "$grc" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-CLI $name rep=$REP got='$grc'" >> "$OUT"
  [ "$grs" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-SRV $name rep=$REP got='$grs'" >> "$OUT"
  if [ "$e3t" = "1" ]; then
    [ "$act" -eq 0 ] && echo "ARM-LIVENESS-FAIL-ACTIVE $name rep=$REP" >> "$OUT"
    # `eng=0` with the gate configured is a WARM-UP FAILURE, not a null.
    [ "$eng" -eq 0 ] && echo "ARM-LIVENESS-FAIL-3T $name rep=$REP (VOID: no eng=1)" >> "$OUT"
  else
    [ "$act" -gt 0 ] && echo "ARM-CONTAMINATION-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -gt 0 ] && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
  fi

  # The `[3T]` readouts verbatim (first + last engaged line) — the terms the
  # pre-registered table is compared against, kept raw in the ledger log.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[3T\]' | grep 'eng=1' \
    | grep -oE "eng=[01] cap=[0-9]+ window=[0-9.]+ slack=[0-9.]+ span=[0-9.]+ rho=[0-9.]+ b=[0-9.]+" \
    | sed -n '1p;$p' | sed "s/^/3TLINE $name rep=$REP /" >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

for REP in $(seq 1 "$REPS"); do
  #       cell   arm envs   cA     cB     mode   bytes      3t rs
  run_one c1     A  ""      c1     c1     single 400000000  0  0
  run_one c1     B  "$LAW"  c1     c1     single 400000000  1  1
  run_one c1     C  "$CTL"  c1     c1     single 400000000  1  0
  run_one c1     D  "$ANC"  c1     c1     single 400000000  0  1
  run_one c7     A  ""      c2     c2     dual   200000000  0  0
  run_one c7     B  "$LAW"  c2     c2     dual   200000000  1  1
  run_one c7     C  "$CTL"  c2     c2     dual   200000000  1  0
  run_one c7     D  "$ANC"  c2     c2     dual   200000000  0  1
  run_one c8     A  ""      c2     c3     dual    25000000  0  0
  run_one c8     B  "$LAW"  c2     c3     dual    25000000  1  1
  run_one c8     C  "$CTL"  c2     c3     dual    25000000  1  0
  run_one c8     D  "$ANC"  c2     c3     dual    25000000  0  1
  run_one sc2    A  ""      c2     c2     single 100000000  0  0
  run_one sc2    B  "$LAW"  c2     c2     single 100000000  1  1
  run_one sc2    C  "$CTL"  c2     c2     single 100000000  1  0
  run_one sc2    D  "$ANC"  c2     c2     single 100000000  0  1
  run_one sc3    A  ""      c3     c3     single  25000000  0  0
  run_one sc3    B  "$LAW"  c3     c3     single  25000000  1  1
  run_one sc3    C  "$CTL"  c3     c3     single  25000000  1  0
  run_one sc3    D  "$ANC"  c3     c3     single  25000000  0  1
  # c2r100 at 100 MB (not 50): this is a HELD-OUT criterion-3 cell where the
  # `[3T]` readout IS the datum, and the smoke got only 2 engaged lines at
  # 50 MB (the echo prints every 2 s). c2r200 stays at 50 MB — it is
  # pre-registered as CLAMPED (B2) and can neither confirm nor refute, so it
  # does not earn the extra minutes.
  run_one c2r100 A  ""      c2r100 c2r100 single 100000000  0  0
  run_one c2r100 B  "$LAW"  c2r100 c2r100 single 100000000  1  1
  run_one c2r100 C  "$CTL"  c2r100 c2r100 single 100000000  1  0
  run_one c2r100 D  "$ANC"  c2r100 c2r100 single 100000000  0  1
  run_one c2r200 A  ""      c2r200 c2r200 single  50000000  0  0
  run_one c2r200 B  "$LAW"  c2r200 c2r200 single  50000000  1  1
  run_one c2r200 C  "$CTL"  c2r200 c2r200 single  50000000  1  0
  run_one c2r200 D  "$ANC"  c2r200 c2r200 single  50000000  0  1
done

# ── F2, the delta falsifier, run ON PURPOSE (not read off the bulk arms) ──
# `(1 - rho)*D(delta)` is multiplied by zero at rho = 1, so delta must NOT
# enter the limit on a reliable transfer. Same cell, same arm, three named
# points on the delta dial (b = 2 / 1 / 0.5, echoed by `[3T] b=`).
#
# READ THIS THE RIGHT WAY. The pre-registration says "if the measured [3T]
# limit moves with the protocol hint on a reliable transfer, the law as
# shipped is wrong". Comparing `cap` between hints CANNOT do that job: the
# limit is LINEAR in the measured rate, three invocations achieve three
# different rates, so cap moves for a reason that has nothing to do with
# delta (the smoke measured exactly this - cap 492/458/703 at b=2/1/0.5 on
# achieved 84.0/29.5/62.5 Mbit/s). The delta-free invariant is the RATIO
#     slack/window = rho*(9/8*srtt + srtt) / (K*RTprop) = 17/8 = 2.125
# at rho = 1, whatever delta is - the rate cancels. `tt_parse.py` records it
# as `sw_ratio_*`, and THAT is what F2 is scored on.
for HINT in ${RWM_TT_DELTA-bulk auto realtime}; do
  echo "=== DELTACHECK hint=$HINT seed=$SEED_ARG $(date -u +%T)" >> "$OUT"
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $LAW RWM_DIAG=1 bash perf_rwm_c.sh c2 c2 "$HINT" 60000000 1 single >/dev/null 2>&1 || true
  python3 ./tt_parse.py "delta-$HINT" B "$SEED_ARG" 0 /tmp/rwm-c.log /tmp/rwm-s.log >> "$OUT" 2>&1 \
    || echo "DELTACHECK-PARSE-FAIL $HINT" >> "$OUT"
  cp /tmp/rwm-c.log "$DDIR/delta-$HINT-s${SEED_ARG}-c.log" 2>/dev/null || true
done

# ── Arm-liveness (discipline 7): an arm with zero results fails LOUDLY ────
echo "--- ARMCOUNTS (expect $REPS per arm)" >> "$OUT"
for c in $TT_CELLS; do
  for a in A B C D; do
    hdr=$(grep -c "arm=$c-$a " "$OUT" || true)
    res=$(grep -c "\"cell\": \"$c\", \"arm\": \"$a\"" "$OUT" || true)
    echo "ARMCOUNT $c-$a headers=$hdr results=$res" >> "$OUT"
    [ "$res" -eq 0 ] && echo "ARM-VANISHED $c-$a" >> "$OUT"
  done
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
