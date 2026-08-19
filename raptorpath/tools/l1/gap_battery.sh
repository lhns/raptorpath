#!/bin/bash
# THE MISSING-HALF BATTERY — the VM driver for goal-gate "The Missing Half at
# the Fast Single Path — PRE-REGISTRATION". That block is the CONTRACT: it is
# scored against, never modified, and no number in it may change now that the
# VM has been touched.
#
#   sudo bash gap_battery.sh <seed> [reps]
#
# ── THE QUESTION ────────────────────────────────────────────────────────
# The era battery scored `P1` DISAGREES-LOW: `c1` goodput +6.84 % (s42) and
# +9.62 % (s7) against the pre-registered `[+12.7, +13.0] %` the ack-merge flip
# published — WITH the mechanism fully engaged (`[CTLD]` 1.96 reproduced to
# three digits). Its own §4 named two live explanations and separated neither:
#
#   (a) SUBSTRATE DRIFT — both era arms read LOWER in absolute terms than the
#       flip-era ledger (OLD 175/192 against 203/202 Mbit/s).
#   (b) A REAL INTERACTION among the flips shipped SINCE ack-merge, one of
#       which eats the gain.
#
# This battery separates them. **And it can, because of a fact about the two
# ledgers that is checkable without the VM and was checked before this file
# was written.**
#
# ── THE DISCOVERY THAT MAKES THE DRIFT CONTROL FREE ─────────────────────
# The ack-merge flip battery ran ONE binary with an ENV GATE (`prior` = unset,
# `am` = `RWM_ACK_MERGE=1`), built from commit `c2bfab7`, `sha256 fbd6b279…`.
# The era battery's OLD arm ran commit `4171b58`, `sha256 fbd6b279…`.
#
#   git diff --stat 4171b58 c2bfab7
#     raptorpath/docs/goal-gate.md | 34 ----------------------------------
#
# **THE TWO COMMITS ARE ENGINE-IDENTICAL — the only difference is 34 deleted
# lines of this document — and the two sha256s are the same string.** So the
# flip-era `prior` arm and the era battery's `OLD` arm are THE SAME BINARY IN
# THE SAME CONFIGURATION AT THE SAME CELL, measured eleven days apart, and
# they read **203.1 / 201.8** against **175.25 / 192.06** Mbit/s.
#
# That binary is ALREADY ON THE VM at `/home/vibe/era-old/target/release/
# raptorpath` and its sha was re-verified as `fbd6b279…` on 2026-08-19 by the
# Latency Truth contract's completion. **NO OLD-TREE REBUILD IS REQUIRED**,
# and the drift control is a re-run of a binary this session did not build.
#
# ── THE ARMS ────────────────────────────────────────────────────────────
# PHASE 1 — THE DRIFT CONTROL. All three on the OLD binary; none of them
# touches today's main.
#
#   Op  OLD + `RWM_DIAG=1`                     THE FLIP-ERA `prior` ARM,
#                                              BYTE-EXACT ENV. Its LEVEL against
#                                              the published 203.1/201.8 IS the
#                                              drift measurement.
#   Oa  OLD + `RWM_DIAG=1 RWM_ACK_MERGE=1`     THE FLIP-ERA `am` ARM, BYTE-EXACT.
#                                              `Oa/Op` re-measures the
#                                              +12.7/+13.0 % ratio on its own
#                                              binary, today.
#   Oe  OLD + the ERA battery's env            THE INSTRUMENT-LOAD CONTROL.
#       (`RWM_DIAG=1 RWM_ACKDIAG=1             `Oe` vs `Op` bounds what the era
#        RWM_WALLDIAG=1 RWM_LATPROBE=1`)       session's extra instruments cost;
#                                              `Oe` vs the era's own 175/192
#                                              closes the loop.
#
# **WHY `Oe` IS NOT REDUNDANT.** `RWM_ACKDIAG` and `RWM_WALLDIAG` name gauges
# that DO NOT EXIST at `4171b58` (the era battery's own header asserts
# `[ACKDIAG]` and `[WALL]` absent at OLD), so they are inert there. But
# `RWM_LATPROBE` is HARNESS-SIDE — it starts a 20 pkt/s `ping` per leg inside
# the client namespace — and it is present in the era session and ABSENT from
# the flip session. At a cell that may be sender-bound, a config difference
# between the two ledgers is a confound whether or not it turns out to matter,
# and the honest thing is to measure it rather than argue it away.
#
# PHASE 2 — THE LADDER. Today's main, ONE binary, env gates. Runs only on the
# branch the contract's reading rules select.
#
#   Nd  shipped defaults
#   Nm  `RWM_ACK_MERGE=0`      ← **THE DISCRIMINATING ARM.** `Nd − Nm` is
#                                ack-merge's MARGINAL effect in TODAY's
#                                composed stack, one binary, one session, paired
#                                within rep. The era battery could only ask this
#                                as a cross-era two-binary contrast, which
#                                confounds ack-merge with every later flip AND
#                                with drift. This asks it directly.
#   Nh  `RWM_HONEST_ANCHOR=0`
#   Ns  `RWM_SUM_CAP=0`        ← predicted INERT at c1 by the era contract's own
#   Nc  `RWM_DELTA_CAP=0`        claim (the pooled seat short-circuits at
#                                `n_live < 2`). A NON-parity reading FALSIFIES
#                                that claim, which is why they are here.
#
# ── THE ENV IS DERIVED FROM THE ECHO EXPECTATIONS, NEVER WRITTEN TWICE ──
# `ladder_battery.sh`'s convention: `gate_expect` is the single table, and both
# the launch env and the liveness assertion are computed from it. An arm cannot
# be launched carrying an env its own gate check does not expect.
#
# ── LIVENESS ON A BINARY WITH NO `[GATES]` ECHO ─────────────────────────
# `[GATES]` was added ONE DAY AFTER the pre-flip baseline, so the OLD arms have
# no gate echo at all and the usual assertion is unavailable. Two era-invariant
# signals carry it instead, and they are at DIFFERENT LEVELS on purpose:
#
#   the GATE took       `ack-merge ACTIVE`, two-sided (both roles emit it from
#                       `run_impl`) — present on `Oa`, ABSENT on `Op`/`Oe`.
#   the MECHANISM ran   `[CTLD]` — the receiver's control-datagram density,
#                       `RWM_DIAG`-only and present at BOTH eras. 1.96 is the
#                       pre-flip value the flip battery measured and the era
#                       battery reproduced to three digits.
#
# `[GATES]` then becomes the ANTI-MIX assertion exactly as in `era_battery.sh`:
# ABSENT two-sided IS the OLD binary, PRESENT two-sided IS today's.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
source ./abort_witness.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
GAP_CELLS="${RWM_GAP_CELLS:-c1}"
# PHASE 1 IS THE DEFAULT. Phase 2 is launched by the caller only on the branch
# the contract selects, and it is a DIFFERENT dispatch on purpose: a driver
# that ran both would make the branch a formality.
GAP_ARMS="${RWM_GAP_ARMS:-Op Oa Oe}"
TAG="${RWM_GAP_TAG:-gap}"

NEW_ROOT="${RWM_GAP_NEW_ROOT:-/home/vibe/raptorpath}"
OLD_ROOT="${RWM_GAP_OLD_ROOT:-/home/vibe/era-old}"
NEW_BIN="$NEW_ROOT/target/release/raptorpath"
OLD_BIN="$OLD_ROOT/target/release/raptorpath"

# G-SHA: the OLD binary must be the one BOTH prior ledgers were read off. A
# mismatch is not explained, it discards the session.
OLD_SHA_WANT="${RWM_GAP_OLD_SHA:-fbd6b279d0d69a8f4d14f177fc5fead34c0ec9c04f3322a74b17528ca4cbaf4d}"

OUTDIR="${RWM_GAP_OUTDIR:-/home/vibe/gap}"
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$OUTDIR" "$DDIR"

arm_bin() { case "$1" in O*) echo "$OLD_BIN" ;; *) echo "$NEW_BIN" ;; esac; }
arm_era() { case "$1" in O*) echo old ;; *) echo new ;; esac; }

# ── THE ONE TABLE. `gate_expect <arm> <gate>` -> the value that gate MUST
#    echo on that arm. Every NEW gate here ships DEFAULT ON, so an expectation
#    of 1 needs no env and an expectation of 0 is the arm's whole identity.
#    The OLD binary echoes nothing, so its column is handled separately.
NEW_GATES="RWM_ACK_MERGE RWM_HONEST_ANCHOR RWM_SUM_CAP RWM_DELTA_CAP"
gate_expect() { # arm gate -> 0|1
  local arm="$1" g="$2"
  case "$arm/$g" in
    Nm/RWM_ACK_MERGE)     echo 0 ;;
    Nh/RWM_HONEST_ANCHOR) echo 0 ;;
    Ns/RWM_SUM_CAP)       echo 0 ;;
    Nc/RWM_DELTA_CAP)     echo 0 ;;
    *)                    echo 1 ;;
  esac
}

# THE ENV, DERIVED. `RWM_DIAG=1` is on EVERY arm because it is on every arm of
# BOTH ledgers this battery is compared against — it is part of the byte-exact
# reproduction, not an addition.
arm_env() {
  local arm="$1" e="RWM_DIAG=1" g
  case "$arm" in
    Op) ;;
    Oa) e="$e RWM_ACK_MERGE=1" ;;
    # The ERA session's env, transcribed from `era_battery.sh`'s run_topo.
    Oe) e="$e RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1" ;;
    N*) for g in $NEW_GATES; do
          [ "$(gate_expect "$arm" "$g")" = "0" ] && e="$e $g=0"
        done ;;
  esac
  echo "$e"
}
# `ack-merge ACTIVE` expectation, per arm. At OLD the gate is DEFAULT OFF, so
# only `Oa` has it. At NEW it is DEFAULT ON, so only `Nm` lacks it.
arm_wants_am() {
  case "$1" in Oa) echo 1 ;; Op|Oe) echo 0 ;; Nm) echo 0 ;; *) echo 1 ;; esac
}
arm_wants_gates() { case "$1" in O*) echo 0 ;; *) echo 1 ;; esac; }

# cell -> "scenA scenB mode bytes". TRANSCRIBED from `ackflip_battery.sh`'s
# own c1 row (`run_one c1-prior "" c1 c1 400000000 single 0`) and from
# `era_battery.sh`'s `cell_spec`, which agree — that agreement is WHY the two
# ledgers are comparable at all, and it is why the row is transcribed rather
# than redefined here.
cell_spec() {
  case "$1" in
    c1) echo "c1 c1 single 400000000" ;;
    *) echo "" ;;
  esac
}

ANCHOR_CC="quinn congestion controller: BBR"
ANCHOR_MTU="MTU floor: min_mtu=initial_mtu"

check_and_parse() { # name cell arm
  local name="$1" cell="$2" arm="$3"
  local era; era="$(arm_era "$arm")"
  # Strip ANSI once; every count below reads the cleaned copies.
  sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log > /tmp/gap-c.txt 2>/dev/null
  sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log > /tmp/gap-s.txt 2>/dev/null

  local ac_c ac_s am_c am_s gl_c gl_s amk_c amk_s ctld_n
  ac_c=$(grep -cF "$ANCHOR_CC" /tmp/gap-c.txt 2>/dev/null || true); ac_c="${ac_c:-0}"
  ac_s=$(grep -cF "$ANCHOR_CC" /tmp/gap-s.txt 2>/dev/null || true); ac_s="${ac_s:-0}"
  am_c=$(grep -cF "$ANCHOR_MTU" /tmp/gap-c.txt 2>/dev/null || true); am_c="${am_c:-0}"
  am_s=$(grep -cF "$ANCHOR_MTU" /tmp/gap-s.txt 2>/dev/null || true); am_s="${am_s:-0}"
  gl_c=$(grep -c "\[GATES\]" /tmp/gap-c.txt 2>/dev/null || true); gl_c="${gl_c:-0}"
  gl_s=$(grep -c "\[GATES\]" /tmp/gap-s.txt 2>/dev/null || true); gl_s="${gl_s:-0}"
  amk_c=$(grep -c "ack-merge ACTIVE" /tmp/gap-c.txt 2>/dev/null || true); amk_c="${amk_c:-0}"
  amk_s=$(grep -c "ack-merge ACTIVE" /tmp/gap-s.txt 2>/dev/null || true); amk_s="${amk_s:-0}"
  ctld_n=$(grep -c "\[CTLD\]" /tmp/gap-s.txt 2>/dev/null || true); ctld_n="${ctld_n:-0}"

  # ── THE ABORT VERDICT FIRST, so an aborted invocation never produces a wall
  #    of liveness failures. Same definition as the era battery's.
  if [ "$((ac_c + ac_s + am_c + am_s))" -eq 0 ]; then
    local cause; cause=$(python3 -c "
import sys; sys.path.insert(0, '.')
from abort_witness import cause_or
print(cause_or('/tmp/rwm-abort.txt'))" 2>/dev/null)
    echo "ABORT $name rep=$REP era=$era (no era anchor on either endpoint) abort_cause=${cause:-no_record}" >> "$OUT"
    [ "${cause:-no_record}" = "no_record" ] \
      && echo "INSTRUMENT-FAIL-WITNESS $name rep=$REP (an abort with no witness record)" >> "$OUT"
    cp /tmp/rwm-abort.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-abort.txt" 2>/dev/null || true
    return 0
  fi

  { [ "$ac_c" -eq 0 ] || [ "$ac_s" -eq 0 ]; } \
    && echo "LIVENESS-FAIL-CC $name rep=$REP cli=$ac_c srv=$ac_s (the era-invariant CC anchor is not two-sided)" >> "$OUT"
  { [ "$am_c" -eq 0 ] || [ "$am_s" -eq 0 ]; } \
    && echo "LIVENESS-FAIL-MTU $name rep=$REP cli=$am_c srv=$am_s (the era-invariant MTU-floor anchor is not two-sided)" >> "$OUT"

  # ── G-ERA, THE ANTI-MIX ASSERTION. Mechanical proof of WHICH binary ran; it
  #    depends on no sha, no path and no trust.
  local want_g; want_g="$(arm_wants_gates "$arm")"
  if [ "$want_g" -eq 0 ]; then
    { [ "$gl_c" -gt 0 ] || [ "$gl_s" -gt 0 ]; } \
      && echo "G-ERA-VIOLATION $name rep=$REP ([GATES] present cli=$gl_c srv=$gl_s on an OLD arm — a NEW binary ran; REP IS VOID)" >> "$OUT"
  else
    { [ "$gl_c" -eq 0 ] || [ "$gl_s" -eq 0 ]; } \
      && echo "G-ERA-VIOLATION $name rep=$REP ([GATES] missing cli=$gl_c srv=$gl_s on a NEW arm — an OLD binary ran, or the engine died before the echo; REP IS VOID)" >> "$OUT"
    # THE LADDER'S OWN ASSERTION: every gate at its table value, both roles.
    local g want got_c got_s
    for g in $NEW_GATES; do
      want="$(gate_expect "$arm" "$g")"
      got_c=$(grep "\[GATES\]" /tmp/gap-c.txt 2>/dev/null | tail -1 | grep -o "$g=[01]")
      got_s=$(grep "\[GATES\]" /tmp/gap-s.txt 2>/dev/null | tail -1 | grep -o "$g=[01]")
      [ "$got_c" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-CLI $name rep=$REP gate=$g got='$got_c' want=$want" >> "$OUT"
      [ "$got_s" != "$g=$want" ] && echo "ARM-LIVENESS-FAIL-SRV $name rep=$REP gate=$g got='$got_s' want=$want" >> "$OUT"
    done
  fi

  # ── THE ACK-MERGE ARM ASSERTION, TWO-SIDED AND TWO-LEVEL. This is the ONLY
  #    mechanical arm check available on the OLD binary, and it is the same one
  #    `ackflip_battery.sh` used against this very binary.
  local want_am; want_am="$(arm_wants_am "$arm")"
  if [ "$want_am" -eq 1 ]; then
    { [ "$amk_c" -eq 0 ] || [ "$amk_s" -eq 0 ]; } \
      && echo "ARM-LIVENESS-FAIL-AM $name rep=$REP (ack-merge echo c=$amk_c s=$amk_s — the mechanism the arm is named for did not arm)" >> "$OUT"
  else
    { [ "$amk_c" -gt 0 ] || [ "$amk_s" -gt 0 ]; } \
      && echo "ARM-CONTAMINATION-AM $name rep=$REP (ack-merge echo c=$amk_c s=$amk_s in an arm that must not carry it)" >> "$OUT"
  fi
  # The MECHANISM gauge, required on BOTH eras. `[CTLD]` is RWM_DIAG-only and
  # every arm here sets RWM_DIAG=1, so its absence is an instrument failure and
  # never a structural silence.
  [ "$ctld_n" -eq 0 ] \
    && echo "INSTRUMENT-FAIL-CTLD $name rep=$REP (no [CTLD] on the receiver — the ack-merge density gauge is the mechanism evidence and it did not run)" >> "$OUT"

  # ── THE ERA-ABSENT GAUGES on OLD, asserted so a reader can never take
  #    structural silence for a null result — and so their PRESENCE would be
  #    seen immediately, since it would mean the binary is not its claimed era.
  local f n_c n_s absent=""
  if [ "$era" = "old" ]; then
    for f in ACKDIAG WALL SUMCAP DCAP RACK LCW CCAP SF; do
      n_c=$(grep -c "\[$f\]" /tmp/gap-c.txt 2>/dev/null || true); n_c="${n_c:-0}"
      n_s=$(grep -c "\[$f\]" /tmp/gap-s.txt 2>/dev/null || true); n_s="${n_s:-0}"
      { [ "$n_c" -gt 0 ] || [ "$n_s" -gt 0 ]; } \
        && echo "ERA-SURPRISE $name rep=$REP ([$f] present on the OLD era cli=$n_c srv=$n_s — this gauge does not exist at 4171b58)" >> "$OUT"
      absent="$absent $f=$n_c/$n_s"
    done
  fi

  # ── THE SCORED ROW. Goodput AND the CPU columns, because MEASUREMENT TRUTH
  #    item 2's A7 asks whether `c1` is sender-bound — and if it is, every
  #    number in this battery's headline column is a CPU number wearing a
  #    network name, and `ms_per_MB` is the mechanism behind any drift found.
  local cpus cpuc secs bytes_done mbit msmb cores pred
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  secs=$(grep -o '"seconds":[0-9.]*' /tmp/gap-c.txt | tail -1 | cut -d: -f2)
  bytes_done=$(grep -o '"bytes":[0-9]*' /tmp/gap-c.txt | tail -1 | cut -d: -f2)
  mbit=$(grep -o '"mean_mbps":[0-9.]*' /tmp/gap-c.txt | tail -1 | cut -d: -f2)
  msmb=NA; cores=NA; pred=NA
  if [ -n "$cpuc" ] && [ -n "$secs" ] && [ -n "$bytes_done" ]; then
    read -r msmb cores pred <<< "$(awk -v c="$cpuc" -v s="$secs" -v b="$bytes_done" \
      'BEGIN{ if (b>0 && s>0) { mb=b/1e6; m=c*1000/mb; co=c/s;
              printf "%.2f %.3f %.1f", m, co, (m>0? co/m*8000 : 0) } }')"
  fi
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP (no CPUCLI — the sender-bound question cannot be asked of this rep)" >> "$OUT"

  echo "GAPROW $name rep=$REP era=$era seed=$SEED_ARG cell=$cell mbit=${mbit:-NA} seconds=${secs:-NA} bytes=${bytes_done:-NA} cpucli=${cpuc:-NA} cpusrv=${cpus:-NA} ms_per_MB=${msmb:-NA} cores=${cores:-NA} pred_mbit=${pred:-NA}" >> "$OUT"
  echo "LIVENESS $name rep=$REP era=$era anchor_cc=$ac_c/$ac_s anchor_mtu=$am_c/$am_s gates=$gl_c/$gl_s ackmerge=$amk_c/$amk_s ctld=$ctld_n --$absent" >> "$OUT"

  # The mechanism gauges' OWN lines, verbatim, so the ledger carries the
  # readout even if a parser later changes its mind about a column. `[CTLD]`
  # is the one the `P1` adjudication turns on.
  for f in CTLD SUMCAP DCAP; do
    (grep -h "\[$f\]" /tmp/gap-c.txt 2>/dev/null \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=cli \1/" >> "$OUT") || true
    (grep -h "\[$f\]" /tmp/gap-s.txt 2>/dev/null \
      | sed "s/^.*\(\[$f\]\)/${f}LINE $name rep=$REP site=srv \1/" >> "$OUT") || true
  done

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
}

run_topo() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs bin era ca cb mode bytes
  envs="$(arm_env "$arm")"; bin="$(arm_bin "$arm")"; era="$(arm_era "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name era=$era seed=$SEED_ARG bin=$bin env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt /tmp/rwm-abort.txt \
        /tmp/gap-c.txt /tmp/gap-s.txt

  # `RWM_GEN=0` is on every arm — the plain-window control, and it is what BOTH
  # prior ledgers ran, so this battery's rows pool with both.
  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 RWM_BIN="$bin" $envs \
    AW_CELL="$cell" AW_ARM="$arm" AW_ERA="$era" AW_REP="$REP" \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|BUSY" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  check_and_parse "$name" "$cell" "$arm"
}

# ── PRE-FLIGHT ──────────────────────────────────────────────────────────
NEED_OLD=0; NEED_NEW=0
for A in $GAP_ARMS; do
  case "$A" in O*) NEED_OLD=1 ;; N*) NEED_NEW=1 ;; esac
done
[ "$NEED_OLD" = 1 ] && { [ -x "$OLD_BIN" ] || { echo "MISSING BINARY: $OLD_BIN — the drift control needs the pre-flip binary the BOTH prior ledgers were read off" | tee -a "$OUT" >&2; exit 5; }; }
[ "$NEED_NEW" = 1 ] && { [ -x "$NEW_BIN" ] || { echo "MISSING BINARY: $NEW_BIN" | tee -a "$OUT" >&2; exit 5; }; }

# G-SHA. THE HARD ONE: if the OLD binary is not `fbd6b279…` it is not the
# binary either prior ledger was read off, and every level comparison in this
# battery is against a number produced by a different executable. That is not
# a caveat, it is a different experiment — so it REFUSES rather than warns.
if [ "$NEED_OLD" = 1 ]; then
  OLD_SHA=$(sha256sum "$OLD_BIN" | cut -d' ' -f1)
  if [ "$OLD_SHA" != "$OLD_SHA_WANT" ]; then
    echo "G-SHA FAIL: OLD binary sha256 $OLD_SHA != the pre-registered $OLD_SHA_WANT." | tee -a "$OUT" >&2
    echo "  The ack-merge flip battery (203.1/201.8) and the era battery's OLD arm (175.25/192.06) were BOTH read off $OLD_SHA_WANT." | tee -a "$OUT" >&2
    echo "  A different binary makes every LEVEL comparison here a comparison against another executable's number. SESSION DISCARDED." | tee -a "$OUT" >&2
    exit 6
  fi
fi
if [ "$NEED_OLD" = 1 ] && [ "$NEED_NEW" = 1 ] \
   && [ "$(sha256sum "$OLD_BIN" | cut -d' ' -f1)" = "$(sha256sum "$NEW_BIN" | cut -d' ' -f1)" ]; then
  echo "IDENTICAL BINARIES: OLD and NEW are the same file" | tee -a "$OUT" >&2; exit 6
fi
if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2; exit 3
fi

{
  echo "=== MISSING-HALF BATTERY seed=$SEED_ARG reps=$REPS cells='$GAP_CELLS' arms='$GAP_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT ${RWM_GAP_CONTRACT:-goal-gate \"The Missing Half at the Fast Single Path — PRE-REGISTRATION\"}"
  echo "=== THE QUESTION: is the era battery's P1 shortfall (+6.84/+9.62 % against a pre-registered [+12.7, +13.0] %) SUBSTRATE DRIFT or an INTERACTION among the later flips?"
  echo "=== PHASE 1 arms are ALL on the PRE-FLIP binary. Op = the flip-era 'prior' arm byte-exact; Oa = the flip-era 'am' arm byte-exact; Oe = Op + the ERA session's env (instrument-load control)."
  echo "=== THE FLIP-ERA BINARY AND THE ERA-OLD BINARY ARE THE SAME FILE: c2bfab7 and 4171b58 differ only by 34 deleted lines of goal-gate.md, and both ledgers name sha256 fbd6b279...  So Op reproduces a measurement, it does not approximate one."
  echo "=== THE PUBLISHED LEVELS Op IS SCORED AGAINST: flip week (2026-08-08) c1-prior 203.1 +/- 8.5 (n=8, s42) and 201.8 +/- 5.6 (n=8, s7); era week (2026-08-19) c1 OLD 175.25 (s42) and 192.06 (s7)."
  echo "=== THE PUBLISHED RATIO Oa/Op IS SCORED AGAINST: +12.7 % (s42) and +13.0 % (s7)."
  echo "=== c1 IS SINGLE-PATH, so the 2026-08-19 per-leg netem seed boundary does NOT apply here: leg 0's seed is the base seed in both harness eras. Stated so the era boundary is not silently inherited."
  echo "=== LIVENESS ON OLD (no [GATES] echo exists at 4171b58): era-invariant anchors + 'ack-merge ACTIVE' two-sided (the gate) + [CTLD] on the receiver (the mechanism)."
  echo "=== G-ERA (anti-mix): [GATES] absent two-sided IS the OLD binary, present two-sided IS today's. A violation VOIDS the rep."
  echo "=== EVERY ROW CARRIES CPUCLI, ms_per_MB AND cores. MEASUREMENT TRUTH item 2's A7 asks whether c1 is sender-CPU-bound; if it is, this battery's headline column is a CPU column and ms_per_MB is the mechanism behind any drift found."
  echo "=== OLD binary $OLD_BIN sha256 $(sha256sum "$OLD_BIN" 2>/dev/null | cut -d' ' -f1)"
  echo "=== OLD source $(cat "$OLD_ROOT/COMMIT" 2>/dev/null)"
  [ "$NEED_NEW" = 1 ] && echo "=== NEW binary $NEW_BIN sha256 $(sha256sum "$NEW_BIN" 2>/dev/null | cut -d' ' -f1)"
  [ "$NEED_NEW" = 1 ] && echo "=== NEW source $(cat "$NEW_ROOT/COMMIT" 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm (desktop session — a DRIFT CANDIDATE, recorded on every session)"
  echo "=== steal $(awk '/^cpu /{print "user="$2" sys="$4" idle="$5" steal="$9}' /proc/stat 2>/dev/null) (host CPU steal — THE drift candidate if c1 is sender-bound)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

# Arms INTERLEAVED and adjacent within one rep of one cell, on the same freshly
# built topology — which is what makes the contrast PAIRED WITHIN REP, and it is
# the only defence against exactly the same-session drift this battery is about.
for REP in $(seq 1 "$REPS"); do
  for CELL in $GAP_CELLS; do
    for ARM in $GAP_ARMS; do
      run_topo "$CELL" "$ARM"
    done
  done
done

echo "=== ARMCOUNTS (rows, NOT live n — aborts emit no GAPROW) $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $GAP_CELLS; do
  for A in $GAP_ARMS; do
    N=$(grep -c "^GAPROW $CELL-$A " "$OUT" || true)
    echo "ARMCOUNT $CELL-$A rows=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "GAP-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo GAP-BATTERY-DONE-$SEED_ARG
