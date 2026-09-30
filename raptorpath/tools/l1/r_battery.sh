#!/bin/bash
# The r > 0 battery: does funded proactive r* beat reactive-only at bulk?
# Derivation: paper §4.9 (the corner r* = 0 and δ_exit); results: paper §9.8
# and docs/status.md §2 (Track B).
#
#   nohup bash r_battery.sh          >/home/vibe/rbattery/all.out 2>&1 &   # the battery
#   nohup bash r_battery.sh --calib  >/home/vibe/rbattery/all.out 2>&1 &   # the smoke
#
# Started as `vibe`, not root: `sudo` is used for the transfer invocations
# alone (they need root for the rp-* namespaces), and every sentinel operation
# runs as the unprivileged user, so the writability proven at launch is the
# writability the exit path has (docs/measurement-discipline.md,
# pre-registration protocol 5).
#
# ── The question ────────────────────────────────────────────────────────
# r can be funded through δ (`MID`) or through χ (`GLIDE`) and through
# nothing else, because those are the only two inputs the price form has
# (paper §4.9). The two rival hypotheses are `H_price` (the price form is
# right) and `H_object` (a per-object effect at the stream tail).
#
# ── Arms ────────────────────────────────────────────────────────────────
#   CTL     (unset)                                  the corner, measured.
#                                                    β = 1, χ = 0, r* = 0.
#   MID     RWM_DELTA=0.05 RWM_COPA_DELTA=0.005      β = ½ exactly, with the
#                                                    CC pinned at Bulk
#                                                    (`RWM_COPA_DELTA` ▸
#                                                    `RWM_DELTA` ▸ the hint's
#                                                    map, scheduler/mod.rs).
#   GLIDE   RWM_COMPLETION_EXPOSURE=1                χ fed from the perf
#                                                    client's own T_rem.
#   GLIDE-Z RWM_COMPLETION_EXPOSURE=1                pre-declared arm-absent:
#           RWM_TAIL_BUDGET=1e-3                     `RWM_TAIL_BUDGET` does not
#                                                    exist on this binary (see
#                                                    the guard below).
#
# ── Cells ───────────────────────────────────────────────────────────────
#   c3hg  single c3hg      the reachability cell, eps = 5.8000 % — the only
#                          cell in the grid above the 5 % line the glide's own
#                          ceiling `BULK_TAIL_BUDGET = 0.05` draws (see
#                          lib.sh for the name).
#   c8    dual   c2/c3     the predicted-corner control at a dual (mixed legs).
#   sc2   single c2        the predicted-corner control at a single.
#
# ── Sizes, and the scored unit ──────────────────────────────────────────
# 1.8 MB and 25 MB discriminate `H_object`, a per-object effect at the stream
# tail. The scored unit is the per-invocation completion p50 over that
# invocation's own objects (40 at 1.8 MB, 4 at 25 MB). n = 8 reps × 2 seeds =
# 16 p50s per arm-cell-size.
#
# ── Instruments on every invocation, in every arm ───────────────────────
# RWM_DIAG=1 (carries `[DIAG]` — whose `cum=src/cod/ack` triple is the r
# liveness witness — plus `[CHI]` and `[SHEDH]` on their own cadences),
# RWM_FDIAG=1 (the falsifier's instrument: decode-resolved vs source-resolved
# wall time per hole), RWM_ACKDIAG=1, RWM_WALLDIAG=1. `[RFA]`'s `preempt_src`
# rides along on the server log.
#
# ── Liveness, asserted per arm before any number is read ────────────────
# Witnesses W1..W10, one `RWITNESS {json}` row per invocation. The three that
# carry the battery:
#   W5  r reaches the wire — last `[DIAG] cum=` ⇒ cod > 0 on funded arms.
#       Its failure is `R-INERT`, attributed to budget-bound, estimator-bound
#       or wiring, never left unattributed.
#   W6  χ reaches the glide — `[CHI] max > 0.5` on GLIDE, `= 0` elsewhere.
#   W7  the CC pin held — a mechanical check, since no Copa δ echo exists.
#
# ABORT != DNF != INSTRUMENT-FAIL. No `[GATES]` on either endpoint = ABORT: no
# datum, no liveness verdict, and in no denominator.
#
# Nothing here flips a default: RWM_DELTA, RWM_COPA_DELTA and
# RWM_COMPLETION_EXPOSURE are absent/off by default and stay so.
set -uo pipefail

# ── CRLF safety ─────────────────────────────────────────────────────────
# A CRLF-tainted library can run zero invocations and still write a DONE
# sentinel. The VM protocol keeps the tree CR-free
# (docs/measurement-discipline.md rule 10), and this script also refuses to
# start if it sees a CR in itself or in its libraries, so it fails at line 1
# rather than at the first `case` arm. Every scrape below strips CR from the
# logs it reads, so a CRLF-tainted log is a parse problem, never a silent zero.
SELF="${BASH_SOURCE[0]}"
cd "$(dirname "$SELF")" || { echo "ABORT-CD $(dirname "$SELF")"; exit 3; }
# lib_battery.sh is sourced first so its guard can check it too; a CR-tainted
# library would fail to define crlf_guard, which the next line catches.
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard "$(basename "$SELF")" lib.sh lib_battery.sh
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

CALIB=0
[ "${1:-}" = "--calib" ] && CALIB=1

OUTDIR="${RWM_R_OUTDIR:-/home/vibe/rbattery}"
DDIR="$OUTDIR/diag"
BIN="${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"
REPS="${RWM_R_REPS:-8}"
SEEDS="${RWM_R_SEEDS:-42 7}"
R_CELLS="${RWM_R_CELLS:-c3hg c8 sc2}"
R_SIZES="${RWM_R_SIZES:-s18 s25}"
R_ARMS="${RWM_R_ARMS:-CTL MID GLIDE GLIDE-Z}"
if [ "$CALIB" -eq 1 ]; then
  REPS=1
  SEEDS="${RWM_R_SEEDS:-42}"
fi

# ── The inherited-environment purge, before anything else ───────────────
# An inherited value would make CTL something other than the shipped stack
# while it still wore CTL's name, and the whole battery reads against CTL.
# `RWM_THREE_TERM` is first on purpose: b(0.05) = sqrt(2) enters
# `contract_stall_s` — term 2 of the three-term store cap — iff that gate is
# armed, the one named confound. It ships off, is pinned off here, and is
# asserted two-sided on every invocation of every arm.
R_CONTAM_GATES="RWM_THREE_TERM RWM_MIN_R \
RWM_DERIVED_SWEEP RWM_COMPOSED_CAP \
RWM_HOLDDOWN_Q"
# shellcheck disable=SC2086
unset RWM_DELTA RWM_COPA_DELTA RWM_COMPLETION_EXPOSURE RWM_TAIL_BUDGET $R_CONTAM_GATES

# ── Sentinel writability is proven at launch, not discovered at exit ────
# A root-owned output directory makes the unprivileged `touch` fail silently
# (no `-e`), and a watcher then waits forever on a finished battery. So the
# write is proven, as the user who will perform it, on the exact absolute
# paths, before any measurement. The run directory is created unprivileged,
# here, before `sudo` is ever invoked.
mkdir -p "$OUTDIR" "$DDIR" 2>/dev/null

# `all.out` is proved first and the tee is opened only afterwards: opening the
# transcript before proving it would send the abort message that explains the
# failure into the file the failure is about.
probe_sentinel "$OUTDIR/all.out"
exec > >(tee -a "$OUTDIR/all.out") 2>&1

SENTINELS="DONE-ALL FAILED-ALL all-era.txt"
for s in $SEEDS; do SENTINELS="$SENTINELS DONE-S$s FAILED-S$s"; done
# shellcheck disable=SC2086
LB_PROOF_EXTRA="calib=$CALIB" prove_sentinels "$OUTDIR" $SENTINELS

# ── Both locks ──────────────────────────────────────────────────────────
# The VM protocol's two locks (docs/measurement-discipline.md, "The VM
# protocol"): `/tmp/rwm-vm.lock` is the box lock and `/home/vibe/rp.lock` the
# tree lock. This script refuses to run without them and releases exactly what
# it took. `noclobber` makes the create-or-fail atomic against a second
# launcher.
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
LB_TAG=r_battery
# On INT/TERM the handler must exit after releasing: a `trap 'f' INT TERM`
# body that does not `exit` resumes the script, which then runs on with both
# locks already cleared.
install_lock_traps
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting"
  exit 3
fi

# ── ARMS ────────────────────────────────────────────────────────────────
arm_env() { case "$1" in
  CTL)     echo "" ;;
  MID)     echo "RWM_DELTA=0.05 RWM_COPA_DELTA=0.005" ;;
  GLIDE)   echo "RWM_COMPLETION_EXPOSURE=1" ;;
  GLIDE-Z) echo "RWM_COMPLETION_EXPOSURE=1 RWM_TAIL_BUDGET=1e-3" ;;
esac; }
# The expected two-sided `[GATES]` echoes, per arm. `RWM_DELTA` echoes `unset`
# when absent and its resolved number when present (gates.rs);
# `RWM_COMPLETION_EXPOSURE` is a flag.
arm_delta_expect() { case "$1" in MID) echo "0.05" ;; *) echo "unset" ;; esac; }
arm_chi_expect()   { case "$1" in GLIDE|GLIDE-Z) echo "1" ;; *) echo "0" ;; esac; }
arm_funded()       { case "$1" in CTL) echo 0 ;; *) echo 1 ;; esac; }

# ── The GLIDE-Z guard ───────────────────────────────────────────────────
# `RWM_TAIL_BUDGET` does not exist on the binary: `BULK_TAIL_BUDGET` is a
# `const` in raptorpath-math consumed by the glide as
# `p + (BULK_TAIL_BUDGET - p)*chi`, with no env gate behind it and no
# `RWM_FORWARD` row. GLIDE-Z is declared arm-absent in advance; this guard
# makes that a measured fact of the run rather than an assumption.
#
# It probes the binary once, at launch, and caches the answer. A binary that
# does echo the gate flips GLIDE-Z on automatically.
GLIDE_Z_OK=0
probe_tail_budget_gate() {
  local echoed
  echoed=$("$BIN" perf --help 2>&1 | tr -d '\r' | grep -c "RWM_TAIL_BUDGET")
  if [ -f "$BIN" ] && [ "${echoed:-0}" -gt 0 ]; then
    GLIDE_Z_OK=1
  fi
  # The authoritative probe is the resolved `[GATES]` line, not the help text;
  # a first invocation settles it and `check_arm` re-reads it per rep.
  echo "GLIDE-Z-PROBE binary=$BIN help_mentions_gate=${echoed:-0} armed=$GLIDE_Z_OK"
}
probe_tail_budget_gate

# ── CELLS AND SIZES ─────────────────────────────────────────────────────
# cell -> "scenA scenB mode"
cell_spec() { case "$1" in
  c3hg) echo "c3hg c3hg single" ;;
  c8)   echo "c2   c3   dual"   ;;
  sc2)  echo "c2   c2   single" ;;
  *)    echo "" ;;
esac; }
# size -> "bytes runs". The object count is the scored unit's sample:
# `H_object` is a per-object effect at the stream tail, so an invocation
# transferring one object measures one draw of it. 40 objects at 1.8 MB and 4
# at 25 MB keep the bytes moved per invocation comparable (72 MB vs 100 MB).
size_spec() { case "$1" in
  s18) echo "1800000  40" ;;
  s25) echo "25000000 4"  ;;
  *)   echo "" ;;
esac; }

# Goodput bands, from earlier plain-window measurements. `c3hg`'s is derived
# from `sc3` and weaker, which is why it aborts nothing on its own — see the
# witness-first rule below.
band_lo() { case "$1" in sc2) echo 78 ;; c8) echo 50 ;; c3hg) echo 9  ;; *) echo 0     ;; esac; }
band_hi() { case "$1" in sc2) echo 92 ;; c8) echo 100;; c3hg) echo 18 ;; *) echo 99999 ;; esac; }
# The generation plateau: a reading inside it is the signature of generation
# coding leaking in despite RWM_GEN=0. It aborts. No band in this grid
# overlaps it.
PLATEAU_LO=26.8
PLATEAU_HI=34.1
# The shaped link per cell, for the calibration's headroom check
# (docs/measurement-discipline.md rule 16): a cell already at >= 97 % of its
# link on CTL can only be moved down.
cell_link() { case "$1" in c3hg) echo 20 ;; sc2) echo 100 ;; c8) echo 120 ;; *) echo 0 ;; esac; }

# ── SCRAPE HELPERS ──────────────────────────────────────────────────────
# Every read is last-line-wins, CR-stripped, and `|| true` guarded: the gauges
# below are cumulative (the `[RFA]` convention), so the last line is the run's
# accounting, and a missing gauge must produce an empty string that the
# witness reports — never a shell failure that kills the rep.
# lastline / field / countlines: lib_battery.sh. `field` is token-anchored
# and first-occurrence.

REP=0
FAILS=""
OUT=""
SEED_ARG=""

run_one() { # cell size arm
  local cell="$1" size="$2" arm="$3"
  local name="$cell-$size-$arm"
  local envs ca cb mode bytes runs
  envs="$(arm_env "$arm")"
  read -r ca cb mode <<< "$(cell_spec "$cell")"
  read -r bytes runs  <<< "$(size_spec "$size")"
  if [ -z "$ca" ] || [ -z "$bytes" ]; then
    echo "UNKNOWN-CELL-OR-SIZE $name" >> "$OUT"; return 0
  fi
  if [ "$arm" = "GLIDE-Z" ] && [ "$GLIDE_Z_OK" -eq 0 ]; then
    echo "ARM-ABSENT GLIDE-Z $cell-$size rep=$REP (RWM_TAIL_BUDGET does not exist on this binary; BULK_TAIL_BUDGET is a const at raptorpath-math/src/lib.rs:124 with no env gate and no RWM_FORWARD row. Pre-declared in the pre-registration section 2; contributes ARM-ABSENT and nothing else.)" >> "$OUT"
    return 0
  fi

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes runs=$runs $(date -u +%T)" >> "$OUT"
  # The forwarded environment, echoed by the harness itself: the engine prints
  # no `RWM_COPA_DELTA` echo, so this is its only direct witness. The
  # mechanical witness is W7.
  echo "RENV $name rep=$REP forwarded=\"SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1\"" >> "$OUT"

  # Stale-echo hygiene: an aborted invocation must never read the previous
  # arm's log and pass its liveness gate.
  sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log 2>/dev/null

  # `sudo` here and nowhere else: the transfer needs root for the rp-*
  # namespaces; every sentinel and lock path above and below is touched as the
  # unprivileged user.
  # shellcheck disable=SC2086
  sudo env SEED="$SEED_ARG" RWM_GEN=0 $envs \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" "$runs" "$mode" 2>&1 \
    | grep -aE "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|BUSY" >> "$OUT"
  # The transfer's rc, not the grep's. `${PIPESTATUS[0]}` is read on the very
  # next line because any command in between clobbers it, and a `|| true` here
  # would silently report rc = 0 for every invocation.
  local rc="${PIPESTATUS[0]}"
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$rc" >> "$OUT"

  check_arm "$cell" "$size" "$arm" "$rc"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
}

check_arm() { # cell size arm rc
  local cell="$1" size="$2" arm="$3" rc="$4"
  local name="$cell-$size-$arm"
  local C=/tmp/rwm-c.log S=/tmp/rwm-s.log
  FAILS=""

  # ── W1: abort cause first. No [GATES] on either endpoint = ABORT: no
  # datum, no liveness verdict, and in no denominator. Checked before any
  # assertion, so an aborted invocation never produces a wall of liveness
  # failures that look like findings.
  local gl_c gl_s
  gl_c=$(lastline "$C" "\[GATES\]")
  gl_s=$(lastline "$S" "\[GATES\]")
  if [ -z "$gl_c" ] && [ -z "$gl_s" ]; then
    echo "W1-NO-GATES $name rep=$REP rc=$rc (ABORT: no datum, in no denominator)" >> "$OUT"
    echo "RWITNESS {\"cell\":\"$cell\",\"size\":\"$size\",\"arm\":\"$arm\",\"seed\":$SEED_ARG,\"rep\":$REP,\"abort\":\"W1-NO-GATES\",\"rc\":$rc}" >> "$OUT"
    return 0
  fi

  # ── W2 / W3: the arm's own gates, matched literally, on both endpoints ──
  local d_exp c_exp d_c d_s x_c x_s
  d_exp="$(arm_delta_expect "$arm")"; c_exp="$(arm_chi_expect "$arm")"
  d_c=$(field "$gl_c" "RWM_DELTA");                d_c="${d_c:-none}"
  d_s=$(field "$gl_s" "RWM_DELTA");                d_s="${d_s:-none}"
  x_c=$(field "$gl_c" "RWM_COMPLETION_EXPOSURE");  x_c="${x_c:-none}"
  x_s=$(field "$gl_s" "RWM_COMPLETION_EXPOSURE");  x_s="${x_s:-none}"
  { [ "$d_c" != "$d_exp" ] || [ "$d_s" != "$d_exp" ]; } \
    && { echo "W2-DELTA-MISMATCH $name rep=$REP cli='$d_c' srv='$d_s' exp='$d_exp'" >> "$OUT"; FAILS="$FAILS W2-DELTA-MISMATCH"; }
  { [ "$x_c" != "$c_exp" ] || [ "$x_s" != "$c_exp" ]; } \
    && { echo "W3-CHI-MISMATCH $name rep=$REP cli='$x_c' srv='$x_s' exp='$c_exp'" >> "$OUT"; FAILS="$FAILS W3-CHI-MISMATCH"; }

  # GLIDE-Z's own re-probe: the authoritative reading of whether the gate
  # exists is the resolved [GATES] line, taken every rep so a binary swap
  # cannot go unnoticed.
  local tb_c
  tb_c=$(field "$gl_c" "RWM_TAIL_BUDGET"); tb_c="${tb_c:-none}"
  [ "$tb_c" != "none" ] && [ "$GLIDE_Z_OK" -eq 0 ] \
    && { GLIDE_Z_OK=1; echo "GLIDE-Z-NOW-ARMED $name rep=$REP (the binary echoes RWM_TAIL_BUDGET=$tb_c)" >> "$OUT"; }

  # ── W4: contamination. Every gate of the purge list off on both endpoints
  # (RWM_THREE_TERM is the named confound; see the purge above).
  local g v_c v_s
  for g in $R_CONTAM_GATES; do
    v_c=$(field "$gl_c" "$g"); v_s=$(field "$gl_s" "$g")
    case "$v_c" in ""|0|unset|none) ;; *) echo "W4-CONTAM $name rep=$REP cli $g=$v_c" >> "$OUT"; FAILS="$FAILS W4-CONTAM" ;; esac
    case "$v_s" in ""|0|unset|none) ;; *) echo "W4-CONTAM $name rep=$REP srv $g=$v_s" >> "$OUT"; FAILS="$FAILS W4-CONTAM" ;; esac
  done
  local rfa gen_c
  rfa=$(lastline "$S" "\[RFA\]")
  [ -z "$rfa" ] && rfa=$(lastline "$C" "\[RFA\]")
  gen_c=$(field "$rfa" "gen"); gen_c="${gen_c:-none}"
  [ "$gen_c" != "0" ] && [ "$gen_c" != "none" ] \
    && { echo "W4-CONTAM $name rep=$REP [RFA] gen=$gen_c (RWM_GEN=0 did not take)" >> "$OUT"; FAILS="$FAILS W4-CONTAM"; }

  # ── W5: does `r` reach the wire? The last [DIAG] cum=src/cod/ack triple
  # (net/diag.rs: the end-of-run accounting reads the last line).
  # `cod = 0` on a funded arm is R-INERT; the attribution between budget-bound,
  # estimator-bound and wiring is made in scoring from the arm's own echoed
  # loss estimate. The numbers it needs are all written into the witness row.
  local diag cum src_cum cod_cum ack_cum cod_frac funded
  diag=$(lastline "$C" "\[DIAG\] t=")
  cum=$(field "$diag" "cum"); cum="${cum:-//}"
  src_cum="${cum%%/*}"
  cod_cum="${cum#*/}"; cod_cum="${cod_cum%%/*}"
  ack_cum="${cum##*/}"
  src_cum="${src_cum:-0}"; cod_cum="${cod_cum:-0}"; ack_cum="${ack_cum:-0}"
  cod_frac=$(awk -v c="$cod_cum" -v s="$src_cum" 'BEGIN{t=c+s; if(t>0) printf "%.6f", c/t; else printf "0"}' 2>/dev/null)
  funded="$(arm_funded "$arm")"
  if [ "$funded" = "1" ]; then
    case "$cod_cum" in ''|0) echo "W5-R-INERT $name rep=$REP cum=$cum (r never reached the wire; ATTRIBUTION per the pre-registration's rule, from this row's own eps-hat)" >> "$OUT"; FAILS="$FAILS W5-R-INERT" ;; esac
  fi

  # ── W6: does χ reach the glide? [CHI] n/max/frac_gt_half, printed on both
  # arms on purpose: the control's `n=0 max=0.0000` is the two-sided half of
  # the reachability claim, so "the glide never ran" is a reading and never an
  # inference.
  local chi chi_n chi_max chi_gt chi_feed
  chi=$(lastline "$C" "\[CHI\]")
  chi_n=$(field "$chi" "n");            chi_n="${chi_n:-0}"
  chi_max=$(field "$chi" "max");        chi_max="${chi_max:-0}"
  chi_gt=$(field "$chi" "frac_gt_half"); chi_gt="${chi_gt:-0}"
  chi_feed=$(countlines "$C" "completion-exposure feed ACTIVE"); chi_feed="${chi_feed:-0}"
  if [ "$c_exp" = "1" ]; then
    awk -v m="$chi_max" 'BEGIN{exit !(m+0 > 0.5)}' \
      || { echo "W6-CHI-DEAD $name rep=$REP [CHI] n=$chi_n max=$chi_max frac_gt_half=$chi_gt feed_echo=$chi_feed" >> "$OUT"; FAILS="$FAILS W6-CHI-DEAD"; }
    [ "$chi_feed" -eq 0 ] \
      && { echo "W6-CHI-DEAD $name rep=$REP (no 'completion-exposure feed ACTIVE' echo -- the gate armed nothing)" >> "$OUT"; FAILS="$FAILS W6-CHI-DEAD"; }
  else
    awk -v m="$chi_max" 'BEGIN{exit !(m+0 > 0.0)}' \
      && { echo "W6-CHI-CONTAM $name rep=$REP [CHI] max=$chi_max on an UNARMED arm" >> "$OUT"; FAILS="$FAILS W6-CHI-CONTAM"; }
  fi

  # ── W7: did the CC pin hold? A mechanical check in place of a Copa δ echo.
  # `RWM_COPA_DELTA=0.005` keeps the congestion controller at Bulk while the
  # contract's delta moves to 0.05; a CC that had followed delta would target
  # a 20x tighter standing queue (q = 1/delta packets, paper §8.2) and cannot
  # hide inside CTL's own rep spread. Recorded here, adjudicated in
  # r_report.py, which has CTL's spread to compare against.
  local rtt_ms
  rtt_ms=$(field "$diag" "rtt"); rtt_ms="${rtt_ms:-}"
  rtt_ms="${rtt_ms%ms}"

  # ── W8: the falsifier's own instrument, [FDIAG] (net/receiver.rs) --
  # DECODE avg is decode-resolved wall time, SOURCE avg is ARQ-resolved wall
  # time, both per hole. `present_at_stall` rides beside them: a DECODE avg
  # quoted without it cannot tell how many holes were already buffered.
  local fd fd_dec_n fd_dec_avg fd_src_n fd_src_avg fd_pas fd_probe_h fd_probe_b
  fd=$(lastline "$S" "\[FDIAG\]")
  [ -z "$fd" ] && fd=$(lastline "$C" "\[FDIAG\]")
  fd_dec_n=$(printf '%s' "$fd"   | sed -n 's/.*DECODE n=\([0-9]*\).*/\1/p'); fd_dec_n="${fd_dec_n:-0}"
  fd_dec_avg=$(printf '%s' "$fd" | sed -n 's/.*DECODE n=[0-9]* avg=\([0-9.]*\)us.*/\1/p'); fd_dec_avg="${fd_dec_avg:-0}"
  fd_src_n=$(printf '%s' "$fd"   | sed -n 's/.*SOURCE n=\([0-9]*\).*/\1/p'); fd_src_n="${fd_src_n:-0}"
  fd_src_avg=$(printf '%s' "$fd" | sed -n 's/.*SOURCE n=[0-9]* avg=\([0-9.]*\)us.*/\1/p'); fd_src_avg="${fd_src_avg:-0}"
  fd_pas=$(printf '%s' "$fd"     | sed -n 's/.*present_at_stall=\([0-9]*\).*/\1/p'); fd_pas="${fd_pas:-0}"
  fd_probe_h=$(field "$fd" "probe_holes");    fd_probe_h="${fd_probe_h:-0}"
  fd_probe_b=$(field "$fd" "probe_buffered"); fd_probe_b="${fd_probe_b:-0}"
  [ -z "$fd" ] && { echo "W8-NO-FDIAG $name rep=$REP" >> "$OUT"; FAILS="$FAILS W8-NO-FDIAG"; }

  # ── W9: [RFA], and `preempt_src` by name -- the reactive plane's own view of
  # the same phenomenon (false = dup_src + preempt_src).
  local rfa_fires rfa_false rfa_ff rfa_dup rfa_pre rfa_fillc rfa_redund
  rfa_fires=$(field "$rfa" "fires");        rfa_fires="${rfa_fires:-0}"
  rfa_false=$(field "$rfa" "false");        rfa_false="${rfa_false:-0}"
  rfa_ff=$(field "$rfa" "false_frac");      rfa_ff="${rfa_ff:-0}"
  rfa_dup=$(field "$rfa" "dup_src");        rfa_dup="${rfa_dup:-0}"
  rfa_pre=$(field "$rfa" "preempt_src");    rfa_pre="${rfa_pre:-0}"
  rfa_fillc=$(field "$rfa" "fill_coded");   rfa_fillc="${rfa_fillc:-0}"
  rfa_redund=$(field "$rfa" "rep_redundant"); rfa_redund="${rfa_redund:-0}"
  [ -z "$rfa" ] && { echo "W9-NO-RFA $name rep=$REP" >> "$OUT"; FAILS="$FAILS W9-NO-RFA"; }

  # ── W10: rc = 0, and the run's own numbers scraped. The completion p50 and
  # mean_mbps come out of r_parse.py, which reads the per-run JSON the engine
  # prints; this block only records that the parse happened.
  [ "$rc" != "0" ] && { echo "W10-RC $name rep=$REP rc=$rc" >> "$OUT"; FAILS="$FAILS W10-RC"; }

  local parsed
  parsed=$(python3 ./r_parse.py "$cell" "$size" "$arm" "$SEED_ARG" "$REP" "$C" "$S" 2>/dev/null)
  if [ -z "$parsed" ]; then
    echo "RPARSE-FAIL $name rep=$REP" >> "$OUT"; FAILS="$FAILS RPARSE-FAIL"
    parsed='{}'
  fi
  echo "RRESULT $parsed" >> "$OUT"

  # ── The goodput guard, witness-first. The witnesses above are read first
  # and the band second, at every rep:
  #   * inside the generation plateau [26.8, 34.1] Mbit/s  => ABORT, a
  #     configuration fault (generation leaked in despite RWM_GEN=0);
  #   * outside the cell band but also outside the plateau, with W1/W2/W3
  #     clean => OUT-OF-BAND RESULT, retained with its cause named -- never an
  #     abort.
  local mb lo hi inband plateau
  mb=$(printf '%s' "$parsed" | sed -n 's/.*"mbps": *\([0-9.]*\).*/\1/p'); mb="${mb:-}"
  lo="$(band_lo "$cell")"; hi="$(band_hi "$cell")"
  if [ -n "$mb" ]; then
    plateau=$(awk -v m="$mb" -v a="$PLATEAU_LO" -v b="$PLATEAU_HI" 'BEGIN{print (m+0>=a && m+0<=b) ? 1 : 0}')
    inband=$(awk  -v m="$mb" -v a="$lo"         -v b="$hi"         'BEGIN{print (m+0>=a && m+0<=b) ? 1 : 0}')
    if [ "$plateau" = "1" ]; then
      echo "ABORT-PLATEAU $name rep=$REP mean_mbps=$mb inside [$PLATEAU_LO,$PLATEAU_HI] -- generation leaked in despite RWM_GEN=0. Configuration fault; no datum." >> "$OUT"
      FAILS="$FAILS ABORT-PLATEAU"
    elif [ "$inband" = "0" ]; then
      case "$FAILS" in
        *W2-*|*W3-*) echo "OUT-OF-BAND-VOID $name rep=$REP mean_mbps=$mb band=[$lo,$hi] (a witness already failed; the band is not read)" >> "$OUT" ;;
        *)           echo "OUT-OF-BAND-RESULT $name rep=$REP mean_mbps=$mb band=[$lo,$hi] plateau=no witnesses=clean -- RETAINED, cause to be named in scoring" >> "$OUT" ;;
      esac
    fi
  else
    inband=0; plateau=0
    echo "NO-GOODPUT $name rep=$REP (no per-run summary scraped)" >> "$OUT"
  fi

  echo "RWITNESS {\"cell\":\"$cell\",\"size\":\"$size\",\"arm\":\"$arm\",\"seed\":$SEED_ARG,\"rep\":$REP,\"rc\":$rc,\"mean_mbps\":\"${mb:-}\",\"band\":[$lo,$hi],\"in_band\":${inband:-0},\"plateau\":${plateau:-0},\"delta_cli\":\"$d_c\",\"delta_srv\":\"$d_s\",\"delta_exp\":\"$d_exp\",\"chi_cli\":\"$x_c\",\"chi_srv\":\"$x_s\",\"chi_exp\":\"$c_exp\",\"tail_budget_gate\":\"$tb_c\",\"rfa_gen\":\"$gen_c\",\"cum_src\":$src_cum,\"cum_cod\":$cod_cum,\"cum_ack\":$ack_cum,\"cod_frac\":$cod_frac,\"chi_n\":$chi_n,\"chi_max\":$chi_max,\"chi_frac_gt_half\":$chi_gt,\"chi_feed_echo\":$chi_feed,\"diag_rtt_ms\":\"${rtt_ms:-}\",\"fdiag_decode_n\":$fd_dec_n,\"fdiag_decode_avg_us\":$fd_dec_avg,\"fdiag_source_n\":$fd_src_n,\"fdiag_source_avg_us\":$fd_src_avg,\"fdiag_present_at_stall\":$fd_pas,\"fdiag_probe_holes\":$fd_probe_h,\"fdiag_probe_buffered\":$fd_probe_b,\"rfa_fires\":$rfa_fires,\"rfa_false\":$rfa_false,\"rfa_false_frac\":$rfa_ff,\"rfa_dup_src\":$rfa_dup,\"rfa_preempt_src\":$rfa_pre,\"rfa_fill_coded\":$rfa_fillc,\"rfa_rep_redundant\":$rfa_redund,\"fails\":\"${FAILS# }\"}" \
    >> "$OUTDIR/r-witness-s${SEED_ARG}.jsonl"
  echo "LIVENESS $name rep=$REP delta=[$d_c/$d_s exp=$d_exp] chi=[$x_c/$x_s exp=$c_exp] cum=$src_cum/$cod_cum/$ack_cum cod_frac=$cod_frac CHI(n=$chi_n max=$chi_max gt=$chi_gt feed=$chi_feed) FDIAG(dec=$fd_dec_n/${fd_dec_avg}us src=$fd_src_n/${fd_src_avg}us pas=$fd_pas) RFA(fires=$rfa_fires pre=$rfa_pre dup=$rfa_dup) rtt=${rtt_ms:-none}ms fails='${FAILS# }'" >> "$OUT"
}

# ── The era header, per seed ────────────────────────────────────────────
run_seed() {
  SEED_ARG="$1"
  OUT="$OUTDIR/r-s${SEED_ARG}.log"
  : > "$OUT"
  : > "$OUTDIR/r-witness-s${SEED_ARG}.jsonl"
  {
    echo "=== THE r > 0 BATTERY seed=$SEED_ARG reps=$REPS calib=$CALIB $(date -u +%FT%TZ)"
    echo "CONTRACT the pre-registration \"THE r > 0 BATTERY -- PRE-REGISTRATION\" (git history before 22b56d9), and nothing else. Paper §4.9 is the derivation; the pre-registration names the two rival hypotheses and the falsifier."
    echo "ARMS  $R_ARMS   (GLIDE-Z pre-declared ARM-ABSENT unless the binary echoes RWM_TAIL_BUDGET; armed=$GLIDE_Z_OK)"
    echo "AXIS  RWM_DELTA + RWM_COPA_DELTA + RWM_COMPLETION_EXPOSURE, all ABSENT/OFF by default. Nothing here flips a default."
    echo "PURGE $R_CONTAM_GATES  (unset at entry, asserted two-sided every rep; RWM_THREE_TERM is the named confound and ships DEFAULT OFF)"
    echo "WINDOW  --window-reliable, RWM_GEN=0 (plain reliable window), --protocol-hint bulk"
    echo "=== binary sha256 $(sha256sum "$BIN" 2>/dev/null | cut -d' ' -f1)"
    echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
    lscpu 2>/dev/null | grep -E 'Model name|Flags' | head -2
    local C S
    for C in $R_CELLS; do
      for S in $R_SIZES; do
        echo "GRID  $C-$S spec=\"$(cell_spec "$C")\" size=\"$(size_spec "$S")\" band=[$(band_lo "$C"),$(band_hi "$C")] link_mbit=$(cell_link "$C")"
      done
    done
    echo "PLATEAU [$PLATEAU_LO,$PLATEAU_HI] Mbit/s -- inside it is an ABORT (generation leaked in). No band in this grid overlaps it."
  } >> "$OUT"

  # Arms interleaved round-robin per rep (docs/measurement-discipline.md
  # rule 3): a drifting box must
  # drift across all arms equally, not across the tail of the last one.
  local CELL SIZE ARM
  for REP in $(seq 1 "$REPS"); do
    for CELL in $R_CELLS; do
      for SIZE in $R_SIZES; do
        for ARM in $R_ARMS; do
          run_one "$CELL" "$SIZE" "$ARM"
        done
      done
    done
  done

  # Per-arm result-count tally: an arm that vanished must fail loudly rather
  # than quietly reduce an n (docs/measurement-discipline.md rule 7).
  echo "=== ARMCOUNTS seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
  for CELL in $R_CELLS; do
    for SIZE in $R_SIZES; do
      for ARM in $R_ARMS; do
        local N
        N=$(grep -ac "\"cell\": \"$CELL\", \"size\": \"$SIZE\", \"arm\": \"$ARM\"" "$OUT" 2>/dev/null)
        N="${N:-0}"
        echo "ARMCOUNT $CELL-$SIZE-$ARM n=$N/$REPS" >> "$OUT"
        if [ "$N" -eq 0 ]; then
          if [ "$ARM" = "GLIDE-Z" ] && [ "$GLIDE_Z_OK" -eq 0 ]; then
            echo "ARM-ABSENT $CELL-$SIZE-GLIDE-Z (pre-declared; contributes nothing and no outcome may be reached from its absence)" >> "$OUT"
          else
            echo "ARM-VANISHED $CELL-$SIZE-$ARM" >> "$OUT"
          fi
        fi
      done
    done
  done
  echo "R-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
}

# A sentinel is earned, not unconditional. An unconditional `touch` converts a
# total failure into a clean-looking success; the log must exist, be
# non-empty, and carry the battery's own terminal line.
rm -f "$OUTDIR/DONE-ALL" "$OUTDIR/FAILED-ALL"
for s in $SEEDS; do rm -f "$OUTDIR/DONE-S$s" "$OUTDIR/FAILED-S$s"; done

echo "R-ALL start $(date -u +%FT%TZ) load=$(cat /proc/loadavg 2>/dev/null)" > "$OUTDIR/all-era.txt"
NARMS=0; for a in $R_ARMS; do [ "$a" = "GLIDE-Z" ] && [ "$GLIDE_Z_OK" -eq 0 ] && continue; NARMS=$((NARMS+1)); done
NCELLS=0; for c in $R_CELLS; do NCELLS=$((NCELLS+1)); done
NSIZES=0; for z in $R_SIZES; do NSIZES=$((NSIZES+1)); done
NSEEDS=0; for s in $SEEDS;   do NSEEDS=$((NSEEDS+1)); done
echo "R-ALL grid: arms=$NARMS cells=$NCELLS sizes=$NSIZES reps=$REPS seeds=$NSEEDS => $(( NARMS * NCELLS * NSIZES * REPS * NSEEDS )) invocations"

for s in $SEEDS; do
  run_seed "$s"
  seed_done "$s" "$OUTDIR/r-s$s.log" "R-BATTERY-DONE seed=$s" R-ALL
done
echo "R-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg 2>/dev/null)" >> "$OUTDIR/all-era.txt"

# ── The report. In --calib it discharges the four smoke clauses and nothing
# in it is a result (n = 1). In the battery it applies the pre-registered
# bar, attribution rule, falsifier and outcome set (see r_report.py).
python3 ./r_report.py --outdir "$OUTDIR" $( [ "$CALIB" -eq 1 ] && echo --calib ) \
  | tee -a "$OUTDIR/all-era.txt"
# The report's rc, not the tee's. `--calib` exits 6 on ABORT-SMOKE; a launcher
# that read the tee's 0 would report success for a failed smoke.
RC_REPORT="${PIPESTATUS[0]}"

ALL_OK=1
for s in $SEEDS; do [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0; done

# DONE-ALL needs every seed's earned DONE *and* a report that exited 0: a
# report that crashed, or a `--calib` that fired ABORT-SMOKE (rc 6), is a
# FAILED-ALL carrying that rc, never a DONE-ALL beside a failure line.
if [ "$ALL_OK" -eq 1 ] && [ "$RC_REPORT" -eq 0 ]; then
  touch "$OUTDIR/DONE-ALL"
  echo "R-ALL-DONE report_rc=$RC_REPORT"
else
  echo "report_rc=$RC_REPORT seeds_done=$ALL_OK $(date -u +%FT%TZ)" > "$OUTDIR/FAILED-ALL"
  echo "R-ALL-FAILED report_rc=$RC_REPORT seeds_done=$ALL_OK"
  [ "$ALL_OK" -eq 1 ] || exit 5
  exit "$RC_REPORT"
fi
