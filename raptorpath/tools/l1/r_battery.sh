#!/bin/bash
# THE r > 0 BATTERY — the VM driver for goal-gate "THE r > 0 BATTERY —
# PRE-REGISTRATION" (own commit, written before any VM contact). That block is
# the CONTRACT: it is scored against, never modified, and no number in it may
# change now that the VM has been touched. Paper §16.82 is the derivation.
#
#   nohup bash r_battery.sh          >/home/vibe/rbattery/all.out 2>&1 &   # the battery
#   nohup bash r_battery.sh --calib  >/home/vibe/rbattery/all.out 2>&1 &   # the smoke
#
# ── THIS SCRIPT IS STARTED AS `vibe`, NOT AS ROOT, AND THAT IS THE POINT ──
# It uses `sudo` for the transfer invocations alone (they need root for the
# rp-* namespaces) and does every sentinel operation as the UNPRIVILEGED user,
# so the sentinel writability it proves at launch is the writability the exit
# path will actually have. See "SENTINELS: EARNED, AND PROVEN WRITABLE BEFORE
# THE FIRST MEASUREMENT" in goal-gate, and the recorded defect it closes.
#
# ── THE QUESTION ────────────────────────────────────────────────────────
# Goal-gate's standing open item 3: "whether funded proactive r* beats
# reactive-only at bulk is an open item-11 question". §16.82.8 sharpened it to
# a stated domain — r can be funded through δ (`MID`) or through χ (`GLIDE`)
# and through nothing else, because those are the only two inputs the price
# form has. This battery asks it on exactly those two, against the two rival
# pre-stated hypotheses `H_price` and `H_object` (§16.82.4).
#
# ── ARMS ────────────────────────────────────────────────────────────────
#   CTL     (unset)                                  the corner, measured.
#                                                    β = 1, χ = 0, r* = 0.
#   MID     RWM_DELTA=0.05 RWM_COPA_DELTA=0.005      β = ½ EXACTLY (the
#                                                    arithmetic is in §3 of the
#                                                    pre-registration and is
#                                                    bit-exact, not "≈ ½"),
#                                                    with the CC PINNED at Bulk
#                                                    (`RWM_COPA_DELTA` ▸
#                                                    `RWM_DELTA` ▸ the hint's
#                                                    map, scheduler/mod.rs:135).
#   GLIDE   RWM_COMPLETION_EXPOSURE=1                χ fed from the perf
#                                                    client's own T_rem.
#   GLIDE-Z RWM_COMPLETION_EXPOSURE=1                PRE-DECLARED ARM-ABSENT:
#           RWM_TAIL_BUDGET=1e-3                     `RWM_TAIL_BUDGET` DOES NOT
#                                                    EXIST on this binary (see
#                                                    the guard in `arm_env`).
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c3hg  single c3hg      THE REACHABILITY CELL, eps = 5.8000 % — the ONLY
#                          cell in the grid ABOVE the 5 % line the glide's own
#                          ceiling `BULK_TAIL_BUDGET = 0.05` draws. NEW; see
#                          lib.sh's own note on why it is not called c3heavy.
#   c8    dual   c2/c3     the predicted-corner control at a DUAL (mixed legs).
#   sc2   single c2        the predicted-corner control at a SINGLE.
#
# ── SIZES, AND THE UNIT THAT IS SCORED ──────────────────────────────────
# 1.8 MB and 25 MB — §16.82.4's discriminator, and the ONLY reason 25 MB is in
# the grid. The SCORED UNIT is the per-invocation completion p50 over that
# invocation's own objects (40 at 1.8 MB, 4 at 25 MB), because `H_object` is a
# PER-OBJECT effect at the stream tail. n = 8 reps × 2 seeds = 16 p50s per
# arm-cell-size.
#
# ── INSTRUMENTS on every invocation, in every arm ───────────────────────
# RWM_DIAG=1 (carries `[DIAG]` — whose `cum=src/cod/ack` triple is the r
# liveness witness — plus `[CHI]` and `[SHEDH]` on their own cadences),
# RWM_FDIAG=1 (the pre-stated falsifier's own instrument: DECODE-resolved vs
# SOURCE-resolved wall time per hole), RWM_ACKDIAG=1, RWM_WALLDIAG=1.
# `[RFA]`'s `preempt_src` rides along on the server log.
#
# ── LIVENESS, asserted per arm BEFORE any number is read ────────────────
# W1..W10 of the pre-registration's §8, one `RWITNESS {json}` row per
# invocation. The three that carry the battery:
#   W5  `r` REACHES THE WIRE — last `[DIAG] cum=` ⇒ cod > 0 on funded arms.
#       Its failure is `R-INERT`, ATTRIBUTED by the pre-registration's rule to
#       BUDGET-BOUND / ESTIMATOR-BOUND / WIRING and never left unattributed.
#   W6  χ REACHES THE GLIDE — `[CHI] max > 0.5` on GLIDE, `= 0` elsewhere.
#   W7  THE CC PIN HELD — the MECHANICAL substitute for the Copa echo that
#       `gates.rs:1432` claims exists and that this tree does not have.
#
# ABORT != DNF != INSTRUMENT-FAIL. No `[GATES]` on EITHER endpoint = ABORT: no
# datum, no liveness verdict, and in NO denominator.
#
# NOTHING HERE FLIPS A DEFAULT. RWM_DELTA, RWM_COPA_DELTA and
# RWM_COMPLETION_EXPOSURE are ABSENT/OFF by default and stay so.
set -uo pipefail

# ── CRLF SAFETY ─────────────────────────────────────────────────────────
# MEASURED, 2026-08-21: `sigb_calib.sh`'s first invocation hit a CRLF trap in
# lib.sh, ran ZERO invocations, and still wrote its DONE sentinel. The shipped
# tree is CRLF-repaired after every sync and `lib.sh` is verified at 0 CR bytes
# as the canary — that is the VM protocol's job — but this script ALSO refuses
# to start if it can see a CR in itself or in lib.sh, because a launcher that
# cannot parse its own library must fail LOUDLY at line 1 rather than at the
# first `case` arm. Every scrape below additionally strips CR from the logs it
# reads, so a CRLF-tainted log is a parse problem and never a silent zero.
SELF="${BASH_SOURCE[0]}"
cd "$(dirname "$SELF")" || { echo "ABORT-CD $(dirname "$SELF")"; exit 3; }
for f in "$(basename "$SELF")" lib.sh; do
  if [ -f "$f" ] && LC_ALL=C grep -q $'\r' "$f" 2>/dev/null; then
    echo "ABORT-CRLF $f carries CR bytes -- the tree was synced without the CRLF repair."
    echo "NOTHING WAS RUN. Repair the tree (dos2unix) and relaunch."
    exit 3
  fi
done
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

# ── THE INHERITED-ENVIRONMENT PURGE, BEFORE ANYTHING ELSE ───────────────
# An inherited value would make CTL something other than the shipped stack
# while it still wore CTL's name, and the whole battery reads against CTL.
# `RWM_THREE_TERM` is FIRST in this list on purpose: b(0.05) = sqrt(2) enters
# `contract_stall_s` — TERM 2 of the three-term store cap — iff that gate is
# armed, which is the ONE confound the pre-registration is required to name.
# It ships DEFAULT OFF (gates.rs:1188) and is pinned off here and asserted
# two-sided on every invocation of every arm.
R_CONTAM_GATES="RWM_THREE_TERM RWM_MIN_R RWM_QUANTILE_CLOCKS RWM_RACK_CLOCKS \
RWM_DERIVED_SWEEP RWM_STORE_CAP_UNIFIED RWM_COMPOSED_CAP RWM_ALPHA_OVERRIDE \
RWM_HOLDDOWN_Q RWM_PLACE_SLACK"
# shellcheck disable=SC2086
unset RWM_DELTA RWM_COPA_DELTA RWM_COMPLETION_EXPOSURE RWM_TAIL_BUDGET $R_CONTAM_GATES

# ── SENTINEL WRITABILITY IS PROVEN AT LAUNCH, NOT DISCOVERED AT EXIT ────
# THE RECORDED DEFECT THIS CLOSES: the hold-down sweep's launcher ran to
# completion and wrote NO sentinel at all — the output directory was owned by
# ROOT, the `touch` ran as the unprivileged user, and the script carried
# `set -uo pipefail` WITHOUT `-e`. So the touch failed, silently, and the
# watcher waited on a file that could never appear for a battery that had
# already finished. The fix is to PROVE the write, as the user who will perform
# it, on the exact ABSOLUTE paths, BEFORE any measurement is taken.
#
# THE RUN DIRECTORY IS CREATED UNPRIVILEGED, HERE, BEFORE `sudo` IS EVER
# INVOKED — that is the whole point of the ordering.
mkdir -p "$OUTDIR" "$DDIR" 2>/dev/null

probe() {
  local p="$1"
  if : > "$p.probe" 2>/dev/null && rm -f "$p.probe" 2>/dev/null; then
    echo "SENTINEL-WRITABLE $p (probed as $(id -un), write+unlink)"
    return 0
  fi
  echo "ABORT-SENTINEL-UNWRITABLE $p"
  echo "ABORT-SENTINEL-UNWRITABLE dir=$OUTDIR owner=$(stat -c '%U:%G %a' "$OUTDIR" 2>/dev/null) user=$(id -un)"
  echo "NOTHING WAS RUN. Fix the ownership of $OUTDIR and relaunch: a pass whose sentinel cannot be written is a pass whose completion cannot be observed."
  exit 3
}

# `all.out` IS PROVED FIRST AND THE TEE IS OPENED ONLY AFTERWARDS: opening the
# transcript before proving it would send the abort message that explains the
# failure into the file the failure is about.
probe "$OUTDIR/all.out"
exec > >(tee -a "$OUTDIR/all.out") 2>&1

for s in $SEEDS; do probe "$OUTDIR/DONE-S$s"; probe "$OUTDIR/FAILED-S$s"; done
probe "$OUTDIR/DONE-ALL"
probe "$OUTDIR/FAILED-ALL"
probe "$OUTDIR/all-era.txt"
echo "SENTINEL-PROOF-COMPLETE $(date -u +%FT%TZ) user=$(id -un) dir=$OUTDIR calib=$CALIB"

# ── BOTH LOCKS ──────────────────────────────────────────────────────────
# The VM protocol's two locks (goal-gate "THE VM PROTOCOL"): `/tmp/rwm-vm.lock`
# is the box lock and `/home/vibe/rp.lock` the tree lock. They are OPERATOR
# locks — this script does not invent a third mechanism — but it REFUSES to run
# without them and it releases exactly what it took, so `ABORT-LOCK` is a
# reading of this ledger and not an assurance in a report. `noclobber` makes
# the create-or-fail atomic against a second launcher.
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
LOCKS_TAKEN=""
take_lock() {
  local p="$1"
  if (set -o noclobber; : > "$p") 2>/dev/null; then
    echo "$$ r_battery $(date -u +%FT%TZ)" > "$p" 2>/dev/null
    LOCKS_TAKEN="$LOCKS_TAKEN $p"
    echo "LOCK-TAKEN $p"
    return 0
  fi
  echo "ABORT-LOCK $p is held: $(cat "$p" 2>/dev/null)"
  echo "NOTHING WAS RUN. Co-tenancy on the box under measurement manufactures the abort signature it looks for (MEASURED: 121 RUN-RETRY over 171 polled invocations against 0 over 80 unpolled)."
  release_locks
  exit 4
}
release_locks() {
  local p
  for p in $LOCKS_TAKEN; do rm -f "$p" 2>/dev/null && echo "LOCK-RELEASED $p"; done
  LOCKS_TAKEN=""
}
trap 'release_locks' EXIT INT TERM
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
# The EXPECTED two-sided `[GATES]` echoes, per arm. `RWM_DELTA` echoes `unset`
# when absent and its resolved NUMBER when present (gates.rs:1718, :1738);
# `RWM_COMPLETION_EXPOSURE` is a flag (gates.rs:1245).
arm_delta_expect() { case "$1" in MID) echo "0.05" ;; *) echo "unset" ;; esac; }
arm_chi_expect()   { case "$1" in GLIDE|GLIDE-Z) echo "1" ;; *) echo "0" ;; esac; }
arm_funded()       { case "$1" in CTL) echo 0 ;; *) echo 1 ;; esac; }

# ── THE GLIDE-Z GUARD ───────────────────────────────────────────────────
# `RWM_TAIL_BUDGET` DOES NOT EXIST ON THIS BINARY. Verified on main@9396ca0:
# `BULK_TAIL_BUDGET` is a `const` (raptorpath-math/src/lib.rs:124) consumed by
# the glide at :712 as `p + (BULK_TAIL_BUDGET - p)*chi`; there is NO env gate
# behind it and no `RWM_FORWARD` row, so even a set variable would not reach
# the binary through this harness. The pre-registration declares GLIDE-Z
# ARM-ABSENT in advance; this guard is what makes that declaration a MEASURED
# fact of the run rather than an assumption carried from the desk.
#
# It probes the binary's OWN `[GATES]` echo once, at launch, and caches the
# answer. A binary that DOES echo the gate flips GLIDE-Z on automatically —
# nothing here has to be edited when the engine gains the line.
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
# size -> "bytes runs".  THE OBJECT COUNT IS THE SCORED UNIT'S SAMPLE:
# `H_object` is a per-object effect at the stream tail, so an invocation
# transferring ONE object measures ONE draw of it. 40 objects at 1.8 MB and 4
# at 25 MB keep the bytes moved per invocation comparable (72 MB vs 100 MB).
size_spec() { case "$1" in
  s18) echo "1800000  40" ;;
  s25) echo "25000000 4"  ;;
  *)   echo "" ;;
esac; }

# GOODPUT BANDS, from the committed plain-window ledgers
# (tools/l1/valpha_battery.sh:271-272). `c3hg`'s is DERIVED from `sc3` and said
# to be weaker, which is why it aborts nothing on its own — see the
# witness-first rule below.
band_lo() { case "$1" in sc2) echo 78 ;; c8) echo 50 ;; c3hg) echo 9  ;; *) echo 0     ;; esac; }
band_hi() { case "$1" in sc2) echo 92 ;; c8) echo 100;; c3hg) echo 18 ;; *) echo 99999 ;; esac; }
# THE GENERATION PLATEAU (goal-gate ~40913): a reading inside it is the 31
# Mbit/s anomaly's own signature and means generation leaked in despite
# RWM_GEN=0. It ABORTS. No band in this grid overlaps it.
PLATEAU_LO=26.8
PLATEAU_HI=34.1
# The shaped link per cell, for the calibration's headroom check (discipline
# 16): a cell already at >= 97 % of its link on CTL can only be moved DOWN.
cell_link() { case "$1" in c3hg) echo 20 ;; sc2) echo 100 ;; c8) echo 120 ;; *) echo 0 ;; esac; }

# ── SCRAPE HELPERS ──────────────────────────────────────────────────────
# EVERY read is last-line-wins, CR-stripped, and `|| true` guarded: the gauges
# below are CUMULATIVE (the `[RFA]` convention, net/mod.rs:2402), so the last
# line is the run's accounting, and a MISSING gauge must produce an empty
# string that the witness reports — never a shell failure that kills the rep.
lastline() { # file pattern
  grep -a "$2" "$1" 2>/dev/null | tr -d '\r' | sed 's/\x1b\[[0-9;]*m//g' | tail -1 || true
}
field() {    # text key   ->  the token after key= up to whitespace
  printf '%s' "$1" | grep -o "$2=[^ |]*" | tail -1 | sed "s/^$2=//" || true
}
countlines() { grep -ac "$2" "$1" 2>/dev/null || true; }

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
  # THE FORWARDED ENVIRONMENT, ECHOED BY THE HARNESS ITSELF. This is the ONLY
  # two-sided-ish witness `RWM_COPA_DELTA` has: gates.rs:1432 lists it in
  # EXTERNALLY_ECHOED as having its "own echo: scheduler Copa family resolve"
  # and NO SUCH ECHO EXISTS on this tree (scheduler/mod.rs carries three
  # eprintln! sites and none prints a delta). The MECHANICAL witness is W7.
  echo "RENV $name rep=$REP forwarded=\"SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1\"" >> "$OUT"

  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate.
  sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log 2>/dev/null

  # `sudo` HERE AND NOWHERE ELSE: the transfer needs root for the rp-*
  # namespaces; every sentinel and lock path above and below is touched as the
  # unprivileged user.
  # shellcheck disable=SC2086
  sudo env SEED="$SEED_ARG" RWM_GEN=0 $envs \
      RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 RWM_WALLDIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" "$runs" "$mode" 2>&1 \
    | grep -aE "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|BUSY" >> "$OUT"
  # THE TRANSFER'S rc, NOT THE GREP'S. `${PIPESTATUS[0]}` is read on the very
  # next line because any command in between clobbers it, and a `|| true` here
  # would silently report rc = 0 for every invocation — which is exactly how a
  # W10 gate stops being a gate.
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

  # ── W1: ABORT-CAUSE FIRST. No [GATES] on EITHER endpoint = ABORT: no
  # datum, no liveness verdict, and in NO denominator. Checked before any
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

  # ── W2 / W3: the arm's OWN gates, matched LITERALLY, on BOTH endpoints ──
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

  # GLIDE-Z's own re-probe: the AUTHORITATIVE reading of whether the gate
  # exists is the resolved [GATES] line, and it is taken every rep so a mid-era
  # binary swap cannot go unnoticed.
  local tb_c
  tb_c=$(field "$gl_c" "RWM_TAIL_BUDGET"); tb_c="${tb_c:-none}"
  [ "$tb_c" != "none" ] && [ "$GLIDE_Z_OK" -eq 0 ] \
    && { GLIDE_Z_OK=1; echo "GLIDE-Z-NOW-ARMED $name rep=$REP (the binary echoes RWM_TAIL_BUDGET=$tb_c)" >> "$OUT"; }

  # ── W4: CONTAMINATION. Every gate of the purge list OFF on both endpoints.
  # RWM_THREE_TERM is the one the pre-registration names: b(0.05) = sqrt(2)
  # enters contract_stall_s (TERM 2 of the three-term store cap) IFF it is
  # armed, and it ships DEFAULT OFF (gates.rs:1188).
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

  # ── W5: DOES `r` REACH THE WIRE? The last [DIAG] cum=src/cod/ack triple
  # (net/diag.rs:934,945 -- "the end-of-run accounting reads the LAST line").
  # `cod = 0` on a funded arm is R-INERT, and the pre-registration's
  # ATTRIBUTION RULE decides between BUDGET-BOUND, ESTIMATOR-BOUND and WIRING
  # from the arm's OWN echoed loss estimate. Nothing is attributed here; the
  # numbers the rule needs are all written into the witness row.
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

  # ── W6: DOES χ REACH THE GLIDE? [CHI] n/max/frac_gt_half (net/mod.rs:2340),
  # printed on BOTH arms on purpose: the control's `n=0 max=0.0000` is the
  # two-sided half of the reachability claim, so "the glide never ran" is a
  # READING and never an inference.
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

  # ── W7: DID THE CC PIN HOLD? The MECHANICAL substitute for the Copa echo
  # this tree does not have. `RWM_COPA_DELTA=0.005` keeps the congestion
  # controller at Bulk while the CONTRACT's delta moves to 0.05; a CC that had
  # followed delta would target a 20x TIGHTER standing queue (q = 1/delta
  # packets, scheduler/mod.rs:120-124) and cannot hide inside CTL's own rep
  # spread. The reading is recorded here and ADJUDICATED in r_report.py, which
  # is the only place that has CTL's spread to compare against.
  local rtt_ms
  rtt_ms=$(field "$diag" "rtt"); rtt_ms="${rtt_ms:-}"
  rtt_ms="${rtt_ms%ms}"

  # ── W8: THE PRE-STATED FALSIFIER'S OWN INSTRUMENT. [FDIAG]
  # (receiver.rs:1899) -- DECODE avg is decode-resolved wall time, SOURCE avg
  # is ARQ-resolved wall time, both per hole. `present_at_stall` rides beside
  # them BECAUSE of the 19-32 ms history: a DECODE avg quoted without it is not
  # a reading of this battery (16.82.6; goal-gate ~6983, 7078-7091).
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
  # the same phenomenon (net/mod.rs:5789-5795: false = dup_src + preempt_src).
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
  # prints; this block only records that the parse HAPPENED.
  [ "$rc" != "0" ] && { echo "W10-RC $name rep=$REP rc=$rc" >> "$OUT"; FAILS="$FAILS W10-RC"; }

  local parsed
  parsed=$(python3 ./r_parse.py "$cell" "$size" "$arm" "$SEED_ARG" "$REP" "$C" "$S" 2>/dev/null)
  if [ -z "$parsed" ]; then
    echo "RPARSE-FAIL $name rep=$REP" >> "$OUT"; FAILS="$FAILS RPARSE-FAIL"
    parsed='{}'
  fi
  echo "RRESULT $parsed" >> "$OUT"

  # ── THE GOODPUT GUARD, WITNESS-FIRST. The witnesses above are read FIRST and
  # the band SECOND, at every rep, without exception (goal-gate ~40913):
  #   * inside the GENERATION PLATEAU [26.8, 34.1] Mbit/s  => ABORT, a
  #     configuration fault (generation leaked in despite RWM_GEN=0);
  #   * outside the cell band but ALSO outside the plateau, with W1/W2/W3
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

# ── THE ERA HEADER, PER SEED ────────────────────────────────────────────
run_seed() {
  SEED_ARG="$1"
  OUT="$OUTDIR/r-s${SEED_ARG}.log"
  : > "$OUT"
  : > "$OUTDIR/r-witness-s${SEED_ARG}.jsonl"
  {
    echo "=== THE r > 0 BATTERY seed=$SEED_ARG reps=$REPS calib=$CALIB $(date -u +%FT%TZ)"
    echo "CONTRACT goal-gate \"THE r > 0 BATTERY -- PRE-REGISTRATION\", and nothing else. Paper 16.82 is the derivation; 16.82.4 the two rival hypotheses; 16.82.6 the falsifier."
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

  # ARMS INTERLEAVED ROUND-ROBIN PER REP (discipline 3): a drifting box must
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

  # Per-arm result-count tally: an arm that VANISHED must fail loudly rather
  # than quietly reduce an n (discipline 7).
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

# A SENTINEL IS EARNED, NOT UNCONDITIONAL. An unconditional `touch` converts a
# total failure into a clean-looking success; the ledger must EXIST, be
# NON-EMPTY, and carry the battery's own terminal line.
seed_done() {
  local s="$1" f="$OUTDIR/r-s$1.log"
  if [ -s "$f" ] && grep -q "R-BATTERY-DONE seed=$s" "$f"; then
    touch "$OUTDIR/DONE-S$s"
    return 0
  fi
  echo "R-ALL seed $s DID NOT COMPLETE -- no R-BATTERY-DONE in $f" | tee -a "$OUTDIR/all-era.txt"
  touch "$OUTDIR/FAILED-S$s"
  return 1
}

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
  seed_done "$s"
done
echo "R-ALL end $(date -u +%FT%TZ) load=$(cat /proc/loadavg 2>/dev/null)" >> "$OUTDIR/all-era.txt"

# ── THE REPORT. In --calib it discharges the four smoke clauses and NOTHING
# in it is a result (n = 1). In the battery it applies the pre-registration's
# §7 bar, §8 attribution rule, §8a falsifier and §11 outcome set.
python3 ./r_report.py --outdir "$OUTDIR" $( [ "$CALIB" -eq 1 ] && echo --calib ) \
  | tee -a "$OUTDIR/all-era.txt"
# THE REPORT'S rc, NOT THE TEE'S. `--calib` exits 6 on ABORT-SMOKE and that is
# the whole point of the smoke: a launcher that read the tee's 0 would print
# "nothing is launched" and then be believed to have launched nothing while
# reporting success.
RC_REPORT="${PIPESTATUS[0]}"

ALL_OK=1
for s in $SEEDS; do [ -f "$OUTDIR/DONE-S$s" ] || ALL_OK=0; done

if [ "$ALL_OK" -eq 1 ]; then
  touch "$OUTDIR/DONE-ALL"
  echo "R-ALL-DONE report_rc=$RC_REPORT"
else
  touch "$OUTDIR/FAILED-ALL"
  echo "R-ALL-FAILED report_rc=$RC_REPORT"
  exit 5
fi
