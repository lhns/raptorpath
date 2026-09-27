#!/bin/bash
# THE PLACEMENT BATTERY (Track A of the law search; paper §16.81;
# pre-registration: goal-gate "Placement Battery — PRE-REGISTRATION").
#
#   sudo bash place_battery.sh <seed> [reps]
#
# WHAT IS BEING MEASURED. D0 established that 92–96 % of the holes at a dual
# cell are closed by the OTHER leg catching up — the scheduler manufactures
# them. §16.81 writes the placement law that manufactures them as ONE
# expression continuous in the dial and replaces three of its arbitrary
# constants with derived forms. This battery runs those forms as DEFAULT-ABSENT
# ARMS beside the shipped law.
#
# **THE READING ORDER IS PRE-REGISTERED AND IT IS NOT THE ARMS.** `[LAT]` on
# the CTL arm is read FIRST, before any challenger is looked at, and it may
# re-route the whole track (`INSTRUMENT-INDICTS-QUEUE`). The alternative —
# reading the decomposition after the arms and choosing which to believe — is
# the failure mode this discipline exists to prevent.
#
# ARMS — five, interleaved round-robin per rep (discipline 3):
#
#   CTL      (unset)                      the shipped law; the pinned cost
#                                         table asserts it is byte-identical
#   T0       RWM_PLACE_T=1e-6             the argmin limit — the T dial's own
#                                         floor, an EXISTING knob, so the
#                                         temperature axis has two ends
#   TSIG     RWM_PLACE_T_DERIVED=1        T = (√6/π)·σ̂_e/ref (§16.81.1)
#   HOL      RWM_PLACE_HOL=1              the frontier term X_i (§16.81.2)
#   HOLTSIG  both                         the composition
#
# **A TSIG TIE WITH CTL LICENSES σ̂_e/ref, NEVER 0.15.** That ruling is in the
# pre-registration and is restated here because it is the one result this
# battery is most likely to produce.
#
# CELLS: c7 (symmetric dual), c8L (het dual c2 || c3 at 100 MB — the aggregation
# seat; named c8L because every other battery's `c8` is the same geometry at
# 25 MB, and one name must mean one cell), c1 (SINGLE
# PATH, the must-not-move CONTROL: N = 1 collapses the softmax to an identity,
# so ANY movement at c1 VOIDS the run), c9h (n = 3 quad, WITNESS ONLY —
# `ABORT-QUAD` is pre-declared).
#
# GOODPUT IS A GUARD, PRE-DECLARED UNDERPOWERED AT n = 8 AND THEREFORE AT n = 4.
# It is reported so a regression is visible; it is not a score.
#
# ── THE 5 h CAP (goal-gate "OPERATOR AMENDMENT (2026-09-08 10:49Z)") ──────
# Every measurement is capped at 5 hours. The pre-registered n = 8 is cut to
# **n = 4** (the default `reps` below) BEFORE launch; no outcome set, guard or
# refuter changes, only n and the seed count. The envelope is enforced by
# `place_run_all.sh` (launch as `vibe`, NOT this script directly): seed 42,
# then seed 7 only if seed 42 finished under 2.5 h, hard backstop at 4 h 50 min.
#
# INVOCATION COUNT AT n = 4, PER SEED (arms x cells x reps + singles):
#
#     c7   5 arms x 4 reps = 20   200 MB dual        placeholder 2.0 min each
#     c8L  5 arms x 4 reps = 20   100 MB dual        placeholder 2.0 min each
#     c1   5 arms x 4 reps = 20   400 MB single      placeholder 2.0 min each
#     c9h  5 arms x 3 reps = 15   100 MB quad        placeholder 3.6 min each
#     sc2  4 reps          =  4   100 MB single      placeholder 2.0 min each
#     sc3  4 reps          =  4    25 MB single      placeholder 2.0 min each
#     ─────────────────────────
#     83 invocations/seed: 68 x 2.0 + 15 x 3.6 = 190 min ≈ 3 h 10 min/seed
#
# The placeholders are the r-battery's measured 3.59 min/quad-class invocation
# and a 2 min dual/single figure, NOT a measurement of THIS grid; the engine's
# own `RUNTIME` lines in docs/l1-raw put the transfer itself at 7–34 s and the
# rest is topology build/teardown, so the per-invocation cost does NOT scale
# with bytes. CONSEQUENCE, STATED BEFORE LAUNCH: at ~3.2 h for seed 42 the
# 2.5 h gate is NOT met and seed 7 is `SKIPPED-S7-5H-BUDGET` by construction
# unless the measured cost comes in under 1.8 min/invocation (150 min / 83).
# One seed fits the 4 h 50 min backstop with ~1 h 40 min of slack; a DNF costs
# `timeout 700` s ≈ 12 min, so ~8 DNFs consume that slack. The cell to drop
# first if the estimate is exceeded is `c9h` (witness only, ABORT-QUAD
# pre-declared: 54 min) — proposed in goal-gate "PLACEMENT BATTERY — n = 4
# AMENDMENT DETAIL", NOT applied here.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1 || { echo "ABORT-CD tools/l1"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard place_battery.sh lib_battery.sh

SEED_ARG="${1:?seed}"; REPS="${2:-4}"   # n = 4: the 5 h amendment
PLACE_CELLS="${RWM_PLACE_CELLS:-c7 c8L c1 c9h}"
PLACE_ARMS="${RWM_PLACE_ARMS:-CTL T0 TSIG HOL HOLTSIG}"
TAG="${RWM_PLACE_TAG:-place}"
# The run directory is `place_run_all.sh`'s (`RWM_PLACE_OUTDIR`, passed
# through its `sudo env`), so the ledger the envelope reads for DONE-S<seed>
# and the sentinels it writes are in ONE directory.
OUTDIR="${RWM_PLACE_OUTDIR:-/home/vibe/placement}"
OUT="$OUTDIR/${TAG}-s${SEED_ARG}.log"
DDIR="$OUTDIR/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

# ── THE EARNED + WRITABLE SENTINELS (discipline 7/15) ────────────────────
# A battery that cannot write its own log, or whose binary is not the one the
# pre-registration was written against, must fail at the TOP and not three
# hours in with a half-filled table.
: > "$OUT" 2>/dev/null || { echo "REFUSED: cannot write $OUT" >&2; exit 4; }
BIN=/home/vibe/raptorpath/target/release/raptorpath
LB_TAG=place_battery
LB_LOG="$OUT"
# EARNED: the arms must EXIST in this binary. A run whose gate names are not
# in the engine's own echo would silently score five copies of CTL — which is
# exactly what an absent-by-default arm looks like from the outside.
preflight_binary "$BIN" RWM_PLACE_T_DERIVED RWM_PLACE_HOL
# ── BOTH LOCKS (goal-gate "THE VM PROTOCOL"; lib_battery.sh's, with the
#    INT/TERM handlers that EXIT) ────────────────────────────────────────────
# `/tmp/rwm-vm.lock` is the box lock and `/home/vibe/rp.lock` the tree lock.
# They are OPERATOR locks — this script does not invent a third mechanism —
# but it REFUSES to run without them and it releases exactly what it took, so
# `ABORT-LOCK` is a reading of this ledger and not an assurance in a report.
# `noclobber` makes the create-or-fail atomic against a second launcher, which
# is also what keeps a second place_battery from starting.
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
# On INT/TERM the handler must EXIT after releasing: a `trap 'f' INT TERM`
# body that does not `exit` RESUMES the script (the r-battery of 2026-09-08 ran
# on for hours after TERM with both locks already cleared). Bash runs the
# handler only once the in-flight foreground invocation returns — which is why
# `place_run_all.sh`'s backstop follows its TERM with `pkill -x raptorpath`.
install_lock_traps
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT"
  exit 3
fi

arm_env() {
  case "$1" in
    CTL)     echo "" ;;
    T0)      echo "RWM_PLACE_T=1e-6" ;;
    TSIG)    echo "RWM_PLACE_T_DERIVED=1" ;;
    HOL)     echo "RWM_PLACE_HOL=1" ;;
    HOLTSIG) echo "RWM_PLACE_T_DERIVED=1 RWM_PLACE_HOL=1" ;;
  esac
}
# The EXPECTED resolved value of each arm gate, for the two-sided liveness
# check below. An arm that does not echo what it was configured for is a
# WIRING-FAILS row and contributes no datum.
arm_td() { case "$1" in TSIG|HOLTSIG) echo 1 ;; *) echo 0 ;; esac; }
arm_hl() { case "$1" in HOL|HOLTSIG)  echo 1 ;; *) echo 0 ;; esac; }

# cell -> "scenA scenB mode bytes"
cell_spec() {
  case "$1" in
    c7)  echo "c2 c2 dual 200000000" ;;
    c8L) echo "c2 c3 dual 100000000" ;;
    c1)  echo "c1 c1 single 400000000" ;;
    c9h) echo "c2 c3 quad 100000000" ;;
    *) echo "" ;;
  esac
}
# The n each cell gets. c9h is a WITNESS at n = 3, pre-declared ABORT-QUAD:
# the quad's instability is a known finding and the row is liveness, not a
# score.
cell_reps() { case "$1" in c9h) echo 3 ;; *) echo "$REPS" ;; esac; }

run_one() { # cell arm
  local cell="$1" arm="$2"
  case " $PLACE_CELLS " in *" $cell "*) ;; *) return 0 ;; esac
  case " $PLACE_ARMS " in *" $arm "*) ;; *) return 0 ;; esac
  [ "$REP" -le "$(cell_reps "$cell")" ] || return 0
  local name="$cell-$arm"
  local envs etd ehl ca cb mode bytes
  envs="$(arm_env "$arm")"; etd="$(arm_td "$arm")"; ehl="$(arm_hl "$arm")"
  read -r ca cb mode bytes <<< "$(cell_spec "$cell")"
  [ -n "$ca" ] || { echo "UNKNOWN-CELL $cell" >> "$OUT"; return 0; }

  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  # Stale-echo hygiene: an aborted invocation must never read the PREVIOUS
  # arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT"
  # The ENGINE's rc, not the grep's: `PIPESTATUS` is copied on the first line
  # after the pipeline (an `|| true` there would replace it with true's 0).
  local rc="${PIPESTATUS[0]}"
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s rc=$rc" >> "$OUT"
  [ "$rc" = "0" ] || echo "ENGINE-RC $name rep=$REP rc=$rc" >> "$OUT"

  python3 ./place_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log \
    >> "$OUT" 2>&1 || echo "PLACERESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── LIVENESS, TWO-SIDED ON BOTH ENDPOINTS (discipline 15c) ────────────
  # Scoped to the `[GATES]` line: the resolve-time liveness echoes carry the
  # gate NAMES in their prose, and an unscoped grep reads the documentation
  # instead of the resolved value.
  local gtc gts ghc ghs etan lat succ nfin
  gtc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_T_DERIVED=[01]")
  gts=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_T_DERIVED=[01]")
  ghc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_HOL=[01]")
  ghs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLACE_HOL=[01]")
  etan=$(grep -c "\[ETA\] site=sender" /tmp/rwm-c.log 2>/dev/null || true)
  lat=$(grep -c "\[LAT\] site=receiver" /tmp/rwm-s.log 2>/dev/null || true)
  succ=$(grep -c "\[SUCC\]" /tmp/rwm-s.log 2>/dev/null || true)
  # The counts above INCLUDE a `final=1` exit-flush line when the engine emits
  # one (a flushed-only short run is still a live instrument); the flush is
  # counted separately so "complete counts" is readable off the ledger.
  # place_parse.py applies the cadence rule (final skipped) on its own fields.
  nfin=$(grep -cE "^\[(LAT|SUCC|ETA)\] .*(^| )final=1( |$)" /tmp/rwm-s.log 2>/dev/null || true)
  echo "LIVENESS $name rep=$REP cli=[$gtc $ghc] srv=[$gts $ghs] eta_lines=$etan lat_lines=$lat succ_lines=$succ recv_final_lines=$nfin (expect td=$etd hol=$ehl)" >> "$OUT"
  [ "$gtc" != "RWM_PLACE_T_DERIVED=$etd" ] && echo "ARM-LIVENESS-FAIL-TD-CLI $name rep=$REP got='$gtc'" >> "$OUT"
  [ "$gts" != "RWM_PLACE_T_DERIVED=$etd" ] && echo "ARM-LIVENESS-FAIL-TD-SRV $name rep=$REP got='$gts'" >> "$OUT"
  [ "$ghc" != "RWM_PLACE_HOL=$ehl" ] && echo "ARM-LIVENESS-FAIL-HOL-CLI $name rep=$REP got='$ghc'" >> "$OUT"
  [ "$ghs" != "RWM_PLACE_HOL=$ehl" ] && echo "ARM-LIVENESS-FAIL-HOL-SRV $name rep=$REP got='$ghs'" >> "$OUT"
  # THE INSTRUMENTS' OWN LIVENESS. A battery whose gauges are absent has not
  # measured what it claims to measure.
  if [ -n "$gtc" ]; then
    [ "$etan" -eq 0 ] && echo "INSTRUMENT-FAIL-ETA $name rep=$REP" >> "$OUT"
    [ "$lat" -eq 0 ] && echo "INSTRUMENT-FAIL-LAT $name rep=$REP" >> "$OUT"
    [ "$succ" -eq 0 ] && echo "INSTRUMENT-FAIL-SUCC $name rep=$REP" >> "$OUT"
  fi

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

echo "=== PLACE BATTERY seed=$SEED_ARG reps=$REPS cells='$PLACE_CELLS' arms='$PLACE_ARMS' $(date -u +%FT%TZ)" >> "$OUT"
echo "=== binary sha256 $(sha256sum "$BIN" | cut -d' ' -f1)" >> "$OUT"
echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)" >> "$OUT"
lscpu | grep -E 'Model name|Flags' | head -2 >> "$OUT" || true

for REP in $(seq 1 "$REPS"); do
  for CELL in $PLACE_CELLS; do
    for ARM in $PLACE_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# ── THE AGGREGATION SINGLES (the guard's denominator) ────────────────────
# `c7 >= 0.97*sum` and `c8L >= 0.87*sum` are read against SAME-SESSION singles,
# never against a number from another run: the shaper, the host and the kernel
# all move between sessions and an aggregation ratio against a stale
# denominator is not a ratio.
for REP in $(seq 1 "$REPS"); do
  for S in sc2 sc3; do
    case " $PLACE_CELLS " in *" c7 "*|*" c8L "*) ;; *) continue ;; esac
    read -r sa sb smode sbytes <<< "$(case "$S" in
      sc2) echo "c2 c2 single 100000000" ;;
      sc3) echo "c3 c3 single 25000000" ;;
    esac)"
    echo "=== rep=$REP arm=$S-SINGLE seed=$SEED_ARG env=\"\" cell=$sa/$sb/$smode bytes=$sbytes $(date -u +%T)" >> "$OUT"
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    t0=$(date +%s)
    env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 \
      bash perf_rwm_c.sh "$sa" "$sb" bulk "$sbytes" 1 "$smode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP" >> "$OUT"
    src="${PIPESTATUS[0]}"
    echo "RUNTIME $S-SINGLE rep=$REP $(( $(date +%s) - t0 ))s rc=$src" >> "$OUT"
    [ "$src" = "0" ] || echo "ENGINE-RC $S-SINGLE rep=$REP rc=$src" >> "$OUT"
    python3 ./place_parse.py "$S" SINGLE "$SEED_ARG" "$REP" \
        /tmp/rwm-c.log /tmp/rwm-s.log >> "$OUT" 2>&1 \
      || echo "PLACERESULT-PARSE-FAIL $S-SINGLE rep=$REP" >> "$OUT"
  done
done

# Per-arm result-count tally: an arm that VANISHED must fail loudly rather
# than quietly reduce an n (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $PLACE_CELLS; do
  for A in $PLACE_ARMS; do
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$(cell_reps "$CELL")" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "PLACE-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
