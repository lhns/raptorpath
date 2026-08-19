#!/bin/bash
# THE ERA BATTERY — the VM battery for goal-gate "Era Battery —
# PRE-REGISTRATION" (own commit, written before this file existed and before any
# VM contact). That block is the CONTRACT: it is scored against, never modified,
# and no number in it may change now that the VM has been touched.
#
#   sudo bash era_battery.sh <seed> [reps]
#
# ERA LEDGER item 1. It measures the ARC'S CUMULATIVE SHIPPED EFFECT directly,
# on TWO binaries, instead of summing the rungs that produced it.
#
# ── THE TWO-BINARY PROTOCOL, AND EVERY WAY IT DIFFERS FROM ITS PREDECESSORS ──
# Every battery this tree has run scored arms of ONE binary. Its parsers, its
# liveness gates and its era-honesty record all assume that, and NONE of those
# assumptions hold here.
#
#   1. TWO BINARIES, both named in the header with their `sha256` and their
#      `COMMIT` file. A session header naming ONE binary is not an era battery.
#   2. WIRE-FORMAT COMPATIBILITY IS NOT REQUIRED AND IS NOT CLAIMED. Each arm is
#      a SELF-CONTAINED sender + receiver from ONE era; `RWM_BIN` is resolved
#      once per invocation and both roles get the same path. BINARIES ARE NEVER
#      MIXED WITHIN AN INVOCATION.
#   3. AND MIXING IS STRUCTURALLY IMPOSSIBLE ANYWAY: `PROTOCOL_VERSION` is 6 at
#      OLD and 7 at NEW, and both `Handshake::deserialize` and
#      `WireMessage::deserialize` hard-refuse a mismatch. A mixed pair fails at
#      the handshake, loudly, before a byte of data.
#   4. THE ENV CARRIES NO GATE. Each arm IS the shipped default of its era.
#      Passing `RWM_SUM_CAP=0` to OLD (where the gate does not exist) or
#      `RWM_DELTA_CAP=1` to NEW (where it is already the default) would either do
#      nothing or restate a default that could drift. This INVERTS the ladder and
#      candidates batteries' `gate_expect` derivation, deliberately, and the
#      liveness assertion moves onto the ECHO — the only one available at OLD.
#
# ── THE ARMS ────────────────────────────────────────────────────────────
#   OLD   4171b584   the pre-arc default: the PARENT of the RWM_ACK_MERGE flip.
#                    PROTOCOL_VERSION 6. NO [GATES] ECHO AT ALL.
#   NEW   6ad964d    today's main, shipped defaults: RWM_ACK_MERGE=1,
#                    RWM_HONEST_ANCHOR=1, RWM_SUM_CAP=1, RWM_DELTA_CAP=1.
#   NR    6ad964d + RWM_RACK_CLOCKS=1 RWM_RACK_REO_MULT=17 — THE AUXILIARY
#                    INSTRUMENT ARM, and the ONE place `RWM_*` appears in an env
#                    here. `[RACK] legacy_pin=` (the shipped [25,100] ms clamp's
#                    own bind fraction) is a counterfactual computed INSIDE the
#                    armed law and reads a DENOMINATOR OF ZERO on a RACK-off arm
#                    (candidates instrument fact 4), so the clamp cannot be read
#                    off the NEW arm at its own default. SCORED ON ITS OWN
#                    [RACK] LINE AND ON NOTHING ELSE — not on goodput, not on
#                    latency, in no denominator, against neither era, by the
#                    contract, before the run.
#
# ── THE PER-ERA ECHO EXPECTATIONS — the reason this file has two tables ──
# `[GATES]` was added by the 2026-08-09 gate-forwarding audit, ONE DAY AFTER the
# baseline commit. So the abort rule every battery since the flip battery
# encodes — "no [GATES] on EITHER endpoint = ABORT" — WOULD MARK EVERY SINGLE
# OLD INVOCATION AN ABORT. It is not a general liveness rule; it is a rule about
# an echo younger than the baseline.
#
# THE ERA-INVARIANT ANCHORS, read from `transport/quic.rs` at BOTH commits and
# emitted unconditionally by BOTH roles at endpoint construction:
#
#   ANCHOR_CC   "quinn congestion controller: BBR"      (OLD :289, NEW :289)
#   ANCHOR_MTU  "MTU floor: min_mtu=initial_mtu"        (OLD :1137, NEW :1269)
#
#   LIVE   both anchors on BOTH endpoints (plus [GATES] on NEW)
#   ABORT  neither anchor on either endpoint — no datum, no liveness verdict,
#          in NO denominator, AND NOW CARRYING AN `abort_cause=`.
#
# `[GATES]` then becomes the MECHANICAL ANTI-MIX ASSERTION (G-ERA): absent
# two-sided IS the OLD binary, present two-sided IS the NEW one. A violation
# means a binary was launched from the wrong era and the rep is VOID.
#
# ── WHAT IS ABSENT AT OLD, stated so no column is invented later ─────────
#   [GATES] [ACKDIAG] [WALL] [SUMCAP] [DCAP] [RACK] [LCW] [CCAP] [SF] and the
#   wait-reason histogram `wait[tun=…]`. Their absence on OLD is CORRECT and is
#   asserted here, so a reader can never take a column of structural silence for
#   a null RESULT. In particular THE c8 DEAD-WALL PAIRED CONTRAST IS NOT
#   AVAILABLE CROSS-ERA and no cross-era wall claim may be made.
#
# ── CELLS ───────────────────────────────────────────────────────────────
# TRANSCRIBED from `ccand_battery.sh`'s `cell_spec`, never redefined, so the
# rows pool with the ladder and candidates ledgers.
#   c1   c1/c1 single  400 MB  1 Gbit    n=8   ack-merge's own cell; both cap
#                                              flips are inert here by
#                                              construction (n_live < 2).
#   c7   c2/c2 dual    200 MB  200 Mbit  n=8
#   c8   c2/c3 dual     25 MB  120 Mbit  n=12  the load-bearing rung
#   c8L  c2/c3 dual    200 MB  120 Mbit  n=12  the length axis
#   sc2  c2/c2 single  100 MB  100 Mbit  n=8   single-path: NO delta-cap
#                                              contribution, by construction.
#
# ── ABORT != DNF != INSTRUMENT-FAIL, AND THE ABORT NOW SAYS WHY ─────────
# The exclusion of aborts from every denominator is sound ONLY while the aborts
# are INDEPENDENT OF THE ARM, and at c8/seed 7 the Candidates Battery measured
# 20 % on the control against 75 % on the RACK arm. Every abort here therefore
# carries `abort_cause=` from `abort_witness.sh`, and `era_report.py` prints the
# per-(cell, arm) abort table BEFORE the first contrast.
#
# ARMCOUNT BELOW IS NOT AN n. It counts PARSED ROWS and an aborted invocation
# still emits a row. The scored n is `era_report.py`'s LIVE n, recomputed from
# the per-era anchor columns.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
source ./abort_witness.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
ERA_CELLS="${RWM_ERA_CELLS:-c1 c7 c8 c8L sc2}"
ERA_ARMS="${RWM_ERA_ARMS:-OLD NEW NR}"
TAG="${RWM_ERA_TAG:-era}"

# ── THE TWO TREES. Overridable so the launch step can place them where it
#    likes, but BOTH must exist or the battery refuses to start: a session that
#    silently ran one era twice is the failure mode this check exists for.
NEW_ROOT="${RWM_ERA_NEW_ROOT:-/home/vibe/raptorpath}"
OLD_ROOT="${RWM_ERA_OLD_ROOT:-/home/vibe/era-old}"
NEW_BIN="$NEW_ROOT/target/release/raptorpath"
OLD_BIN="$OLD_ROOT/target/release/raptorpath"

# The output ROOT is overridable so a successor battery reusing this driver
# writes its own ledger tree instead of interleaving rows into the era ledger
# the era verdict was read off. The DEFAULT IS UNCHANGED, so every era artifact
# path in the record still resolves.
ERA_OUTDIR="${RWM_ERA_OUTDIR:-/home/vibe/era}"
OUT="$ERA_OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$ERA_OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# ── THE ERA TABLE: arm -> binary, and arm -> the ONE env it may carry ────
arm_bin() { case "$1" in OLD) echo "$OLD_BIN" ;; *) echo "$NEW_BIN" ;; esac; }
arm_era() { case "$1" in OLD) echo old ;; *) echo new ;; esac; }
# THE ONLY GATE ENV IN THIS BATTERY, and it belongs to the auxiliary instrument
# arm alone. OLD and NEW carry NOTHING: their arm IS their era's shipped default.
arm_env() {
  case "$1" in
    NR) echo "RWM_RACK_CLOCKS=1 RWM_RACK_REO_MULT=17" ;;
    *)  echo "" ;;
  esac
}
# `[GATES]` presence is the ANTI-MIX assertion (G-ERA): 0 lines on both
# endpoints IS the OLD binary, >=1 on both IS the NEW one.
arm_wants_gates() { case "$1" in OLD) echo 0 ;; *) echo 1 ;; esac; }

# cell -> "scenA scenB mode bytes" — TRANSCRIBED from ccand_battery.sh, never
# redefined here (the same rule capbind_check.py's CELL_PATHS follows).
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

# THE PER-(ARM, CELL) n, applied INSIDE the interleaved loop and never as a
# separate pass, so OLD and NEW sit adjacent in the same round-robin on the same
# freshly built topology — which is what makes G-PAIR's paired contrast
# available at all. n=12 at both c8 cells; n=8 elsewhere. NR is the contract's
# own restriction: c7 + c8 + sc2, n=2/seed, scored on its own echo alone.
AUXREPS="${RWM_ERA_AUXREPS:-2}"
arm_cell_reps() { # arm cell -> reps (0 = this arm does not run at this cell)
  case "$1" in
    NR) case "$2" in c7|c8|sc2) echo "$AUXREPS" ;; *) echo 0 ;; esac ;;
    *)  case "$2" in c8|c8L) echo "$REPS" ;; *) echo "${RWM_ERA_SMALLREPS:-8}" ;; esac ;;
  esac
}

# THE ERA-INVARIANT LIVENESS ANCHORS. Grepped as FIXED STRINGS (`grep -F`)
# because both contain regex metacharacters and one contains a `:` — and because
# the whole point is that they are byte-identical across the two eras.
ANCHOR_CC="quinn congestion controller: BBR"
ANCHOR_MTU="MTU floor: min_mtu=initial_mtu"

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local era; era="$(arm_era "$arm")"
  local npaths; npaths="$(cell_paths "$cell")"

  # THE PER-LEG PROBE FILE LIST (goal-gate "Latency Truth"). Derived from the
  # cell's OWN path count — the same `cell_paths` the liveness line prints — so
  # a cell that grows a leg cannot keep being scored on one. The list is passed
  # as the parser's 13th, OPTIONAL argument: era-battery rows parsed without it
  # are unchanged, which is what makes this instrument HARNESS-SIDE and
  # therefore identical in both arms.
  local legf="" li
  for ((li = 0; li < npaths; li++)); do
    legf="${legf}${legf:+,}/tmp/rwm-ping-${li}.txt"
  done

  python3 ./era_parse.py "$cell" "$arm" "$era" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" /tmp/rwm-abort.txt \
      "$legf" \
    >> "$OUT" 2>&1 || echo "ERA-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── G-LIVE, PER ERA. The anchors first, because they are the ONLY liveness
  # signal the OLD binary has and because the abort verdict is taken from them.
  local ac_c ac_s am_c am_s gl_c gl_s
  ac_c=$(grep -cF "$ANCHOR_CC" /tmp/rwm-c.log 2>/dev/null || true); ac_c="${ac_c:-0}"
  ac_s=$(grep -cF "$ANCHOR_CC" /tmp/rwm-s.log 2>/dev/null || true); ac_s="${ac_s:-0}"
  am_c=$(grep -cF "$ANCHOR_MTU" /tmp/rwm-c.log 2>/dev/null || true); am_c="${am_c:-0}"
  am_s=$(grep -cF "$ANCHOR_MTU" /tmp/rwm-s.log 2>/dev/null || true); am_s="${am_s:-0}"
  gl_c=$(grep -c "\[GATES\]" /tmp/rwm-c.log 2>/dev/null || true); gl_c="${gl_c:-0}"
  gl_s=$(grep -c "\[GATES\]" /tmp/rwm-s.log 2>/dev/null || true); gl_s="${gl_s:-0}"

  # THE ABORT VERDICT — neither anchor on either endpoint. Checked BEFORE any
  # assertion so an aborted invocation never produces a wall of liveness
  # failures, and now carrying the WITNESS's cause: an abort whose cause reads
  # `no_record` is an INSTRUMENT-FAIL OF THE WITNESS and is reported as one.
  if [ "$((ac_c + ac_s + am_c + am_s))" -eq 0 ]; then
    local cause; cause=$(python3 -c "
import sys; sys.path.insert(0, '.')
from abort_witness import cause_or
print(cause_or('/tmp/rwm-abort.txt'))" 2>/dev/null)
    echo "ABORT $name rep=$REP era=$era (no era anchor on either endpoint) abort_cause=${cause:-no_record}" >> "$OUT"
    [ "${cause:-no_record}" = "no_record" ] \
      && echo "INSTRUMENT-FAIL-WITNESS $name rep=$REP (an abort with no witness record — the abort-cause instrument did not run)" >> "$OUT"
    cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
    return 0
  fi

  # Two-sided on BOTH anchors: an anchor on ONE endpoint only is an
  # INSTRUMENT-FAIL for the rep, not an abort. That distinction is the whole
  # reason the abort test above sums all four counters instead of testing them
  # pairwise.
  { [ "$ac_c" -eq 0 ] || [ "$ac_s" -eq 0 ]; } \
    && echo "ERA-LIVENESS-FAIL-CC $name rep=$REP era=$era cli=$ac_c srv=$ac_s (the era-invariant CC anchor is not two-sided)" >> "$OUT"
  { [ "$am_c" -eq 0 ] || [ "$am_s" -eq 0 ]; } \
    && echo "ERA-LIVENESS-FAIL-MTU $name rep=$REP era=$era cli=$am_c srv=$am_s (the era-invariant MTU-floor anchor is not two-sided)" >> "$OUT"

  # ── G-ERA, THE ANTI-MIX ASSERTION. This is the mechanical proof of WHICH
  # binary ran, and it does not depend on the wire version, the sha, or trust.
  local want_g; want_g="$(arm_wants_gates "$arm")"
  if [ "$want_g" -eq 0 ]; then
    { [ "$gl_c" -gt 0 ] || [ "$gl_s" -gt 0 ]; } \
      && echo "G-ERA-VIOLATION $name rep=$REP era=$era ([GATES] present cli=$gl_c srv=$gl_s on the OLD arm — a NEW-era binary ran; REP IS VOID)" >> "$OUT"
  else
    { [ "$gl_c" -eq 0 ] || [ "$gl_s" -eq 0 ]; } \
      && echo "G-ERA-VIOLATION $name rep=$REP era=$era ([GATES] missing cli=$gl_c srv=$gl_s on a NEW arm — an OLD-era binary ran, or the engine died before the echo; REP IS VOID)" >> "$OUT"
    # The NEW era's shipped defaults, asserted rather than assumed: a reader who
    # takes the pre-arc defaults mis-attributes the whole cumulative delta.
    local g got_c got_s want
    for g in RWM_ACK_MERGE RWM_SUM_CAP RWM_DELTA_CAP RWM_RACK_CLOCKS RWM_QUANTILE_CLOCKS; do
      case "$g" in
        RWM_RACK_CLOCKS) [ "$arm" = "NR" ] && want=1 || want=0 ;;
        RWM_QUANTILE_CLOCKS) want=0 ;;
        *) want=1 ;;
      esac
      got_c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "$g=[01]")
      got_s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "$g=[01]")
      [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
      [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
    done
    # NR's own instrument: the clamp's bind fraction needs an ARMED law and at
    # least one evaluation. `evals=0` here is the same denominator-of-zero the
    # candidates battery's instrument fact 3 names, and it voids NR's readout.
    if [ "$arm" = "NR" ]; then
      (grep -hE "\[RACK\] on=1 evals=[1-9]" /tmp/rwm-c.log /tmp/rwm-s.log >/dev/null 2>&1) \
        || echo "RACK-WARMUP-FAIL $name rep=$REP (no [RACK] line with evals>0 — the clock law never evaluated, so this rep carries NO legacy_pin datum)" >> "$OUT"
    fi
  fi

  # ── THE ERA-ABSENT GAUGES, asserted so structural silence is never read as a
  # null result — and so their PRESENCE on OLD would be seen immediately (it
  # would mean the binary is not the era it claims).
  local f n_c n_s absent=""
  if [ "$era" = "old" ]; then
    for f in ACKDIAG WALL SUMCAP DCAP RACK LCW CCAP SF; do
      n_c=$(grep -c "\[$f\]" /tmp/rwm-c.log 2>/dev/null || true); n_c="${n_c:-0}"
      n_s=$(grep -c "\[$f\]" /tmp/rwm-s.log 2>/dev/null || true); n_s="${n_s:-0}"
      { [ "$n_c" -gt 0 ] || [ "$n_s" -gt 0 ]; } \
        && echo "ERA-SURPRISE $name rep=$REP ([$f] present on the OLD era cli=$n_c srv=$n_s — this gauge does not exist at 4171b584; the binary is NOT the era it claims)" >> "$OUT"
      absent="$absent $f=$n_c/$n_s"
    done
  fi

  # ── THE SHARED INSTRUMENTS, required on BOTH eras or the rep's columns void.
  local dg_c dg_s pln
  dg_c=$(grep -c "\[DIAG\]" /tmp/rwm-c.log 2>/dev/null || true); dg_c="${dg_c:-0}"
  dg_s=$(grep -c "\[DIAG\]" /tmp/rwm-s.log 2>/dev/null || true); dg_s="${dg_s:-0}"
  [ "$dg_c" -eq 0 ] && echo "INSTRUMENT-FAIL-DIAG $name rep=$REP (no [DIAG] on the client — no q_p50, no occupancy, no khr)" >> "$OUT"
  pln=$(grep -c "pl=" /tmp/rwm-c.log 2>/dev/null || true); pln="${pln:-0}"
  [ "$pln" -eq 0 ] && echo "INSTRUMENT-FAIL-PL $name rep=$REP (no per-path pl=)" >> "$OUT"
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP (no CPUCLI — E-CPU is one of the three scored claims here)" >> "$OUT"
  [ -z "$cpus" ] && echo "INSTRUMENT-FAIL-CPUSRV $name rep=$REP (no CPUSRV — the receiver is where P3's mechanism lives)" >> "$OUT"

  echo "LIVENESS $name rep=$REP era=$era npaths=$npaths anchor_cc=$ac_c/$ac_s anchor_mtu=$am_c/$am_s gates=$gl_c/$gl_s diag=$dg_c/$dg_s pl=$pln --$absent" >> "$OUT"

  # The gauges' OWN lines, verbatim, so the ledger carries the readout even if
  # the parser ever changes its mind about a column. NR's [RACK] is the only one
  # that carries a scored datum, and the SITE is kept in the tag.
  for f in RACK DCAP SUMCAP CTLD; do
    (grep -h "\[$f\]" /tmp/rwm-c.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" /tmp/rwm-s.log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs bin era ca cb mode bytes
  envs="$(arm_env "$arm")"; bin="$(arm_bin "$arm")"; era="$(arm_era "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name era=$era seed=$SEED_ARG bin=$bin env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS arm's
  # log and pass its liveness gate. The witness record is cleared by `aw_begin`
  # inside perf_rwm_c.sh, on the same principle.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt /tmp/rwm-abort.txt

  # THE ONE PLACE THE ERA IS SELECTED. `RWM_BIN` reaches both roles because
  # perf_rwm_c.sh resolves it once and passes the same `$BIN` to `--server` and
  # `--client`; the arm's own identity fields ride the witness record.
  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 RWM_BIN="$bin" $envs \
    AW_CELL="$cell" AW_ARM="$arm" AW_ERA="$era" AW_REP="$REP" \
    RWM_DIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE|BUSY" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

  # E-LAT's probe is load-bearing at every cell the calibration grants headroom,
  # so it is captured everywhere and its absence is reported everywhere — NOW
  # PER LEG. A dual whose leg-B probe produced nothing is a HALF-MEASURED cell,
  # and under the old single-leg probe it was indistinguishable from a healthy
  # one because leg B was never looked at.
  local npaths2 li pn
  npaths2="$(cell_paths "$cell")"
  for ((li = 0; li < npaths2; li++)); do
    pn=$(grep -c "time=" "/tmp/rwm-ping-${li}.txt" 2>/dev/null || true); pn="${pn:-0}"
    { [ "$pn" -eq 0 ] && [ -s /tmp/rwm-c.log ]; } \
      && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP leg=$li (no delivered-latency sample on this leg)" >> "$OUT"
    cp "/tmp/rwm-ping-${li}.txt" "$DDIR/${name}-s${SEED_ARG}-r${REP}-p${li}.txt" 2>/dev/null || true
  done
  # discipline 16b: the shaped device's own counters, on EVERY cell and EVERY
  # invocation. The headroom denominator is the TRANSFER wall (`seconds`), never
  # INVOCATION_S — see the contract's headroom protocol.
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $ERA_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $ERA_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  local want; want="$(arm_cell_reps "$2" "$1")"
  [ "$want" -gt 0 ] || return 0
  [ "$REP" -le "$want" ] || return 0
  run_topo "$1" "$2"
}

# ── BOTH BINARIES OR NOTHING. A session that silently ran ONE era twice would
# produce a battery whose central contrast is zero by construction, and G-ERA
# would only catch it after 204 invocations.
for B in "$OLD_BIN" "$NEW_BIN"; do
  [ -x "$B" ] || { echo "MISSING BINARY: $B — the era battery needs BOTH eras built" | tee -a "$OUT" >&2; exit 5; }
done
if [ "$(sha256sum "$OLD_BIN" | cut -d' ' -f1)" = "$(sha256sum "$NEW_BIN" | cut -d' ' -f1)" ]; then
  echo "IDENTICAL BINARIES: OLD and NEW are the same file — this is not an era battery" | tee -a "$OUT" >&2
  exit 6
fi
if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== ERA BATTERY seed=$SEED_ARG reps=$REPS smallreps=${RWM_ERA_SMALLREPS:-8} auxreps=$AUXREPS cells='$ERA_CELLS' arms='$ERA_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"Era Battery — PRE-REGISTRATION\", ERA LEDGER item 1 — TWO BINARIES, and that is new"
  echo "=== ARMS OLD = 4171b584 (pre-arc default, PROTOCOL_VERSION 6, NO [GATES] echo) | NEW = 6ad964d (shipped defaults) | NR = NEW + RACK mult=17, AUX"
  echo "=== NR IS SCORED ON ITS OWN [RACK] LINE AND ON NOTHING ELSE — excluded from every contrast and every denominator, by the contract, before the run"
  echo "=== WIRE COMPATIBILITY IS NOT REQUIRED AND NOT CLAIMED: each arm is a self-contained sender+receiver from ONE era. PROTOCOL_VERSION 6 vs 7 hard-refuse a mismatch at handshake, so mixing is impossible anyway."
  echo "=== THE ENV CARRIES NO GATE on OLD/NEW: each arm IS its era's shipped default. Liveness is asserted on the ECHO, the only signal OLD has."
  echo "=== G-ERA (anti-mix): [GATES] absent two-sided IS the OLD binary, present two-sided IS the NEW one. A violation VOIDS the rep."
  echo "=== G-LIVE anchors (era-invariant, both roles, both eras): '$ANCHOR_CC' + '$ANCHOR_MTU'"
  echo "=== ABSENT AT OLD by construction: [GATES] [ACKDIAG] [WALL] [SUMCAP] [DCAP] [RACK] [LCW] [CCAP] [SF] and wait[tun=...]. The c8 dead-wall paired contrast is NOT available cross-era."
  echo "=== ABORT = no era anchor on EITHER endpoint, and it now carries abort_cause= from the witness. Read the abort table BEFORE any contrast."
  echo "=== LATPROBE IS PER-LEG (goal-gate \"Latency Truth\"): one ICMP probe per leg, 20/s, reaped with SIGINT so ping writes its own transmitted/received summary."
  echo "=== EVERY delivered percentile carries a CENSORING FRACTION. A lost probe produces NO sample, GE loss censors exactly the worst states, and the survivors' tail is biased LOW."
  echo "=== q_p50 (engine's OWN standing-queue estimate, computed by the code under test) and ping_* (delivered RTT through the WHOLE shaped path, measured by the kernel) are DIFFERENT QUANTITIES and are never averaged."
  echo "=== THE PROBE IS HARNESS-SIDE: OLD and NEW get the byte-identical instrument, so the fix cannot favour an arm."
  echo "=== OLD binary $OLD_BIN sha256 $(sha256sum "$OLD_BIN" | cut -d' ' -f1)"
  echo "=== NEW binary $NEW_BIN sha256 $(sha256sum "$NEW_BIN" | cut -d' ' -f1)"
  echo "=== OLD source $(cat "$OLD_ROOT/COMMIT" 2>/dev/null)"
  echo "=== NEW source $(cat "$NEW_ROOT/COMMIT" 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm (desktop session, recorded per era honesty)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

# OLD and NEW are ADJACENT within one rep of one cell, on the same freshly built
# topology — the interleaving G-PAIR's paired contrast depends on.
for REP in $(seq 1 "$REPS"); do
  for CELL in $ERA_CELLS; do
    for ARM in $ERA_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
# ARMCOUNT IS NOT AN n — it counts PARSED ROWS and an aborted invocation still
# emits one (`era_parse.py` runs BEFORE the abort verdict, so the abort's own
# columns are in the ledger). The scored n is era_report.py's LIVE n, recomputed
# from the per-era anchor columns.
echo "=== ARMCOUNTS (rows, NOT live n — see era_report.py) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $ERA_CELLS; do
  for A in $ERA_ARMS; do
    WANT=$(arm_cell_reps "$A" "$CELL"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    [ "$WANT" -eq 0 ] && continue
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "ERA-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo ERA-BATTERY-DONE-$SEED_ARG
