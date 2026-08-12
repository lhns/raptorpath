#!/bin/bash
# THE MODE-HUNT BATTERY — the VM battery for goal-gate "Mode-Hunt Battery — VM
# PRE-REGISTRATION" (own commit b8fd6d9, written before any VM contact). That
# block is the CONTRACT: it is scored against, never modified, and no number in
# it may change now that the VM has been touched.
#
#   sudo bash modehunt_battery.sh <seed> [reps]
#
# ── WHAT IS DIFFERENT FROM ITS PREDECESSOR, IN ONE LINE ─────────────────
# `deadwall_battery.sh` scored its treatments against the SHIPPED DEFAULT and
# its stop rule fired because that control does not carry the mode (1/16). This
# driver scores against **AU**, the arm that does (8/11 = 0.727). Same cell,
# same statistic, same parser — a different BASELINE.
#
# ── ARMS ────────────────────────────────────────────────────────────────
#   AU   RWM_STORE_CAP_UNIFIED=1                          THE BASELINE. The
#                                                         arm that CARRIES the
#                                                         mode. n = 12/seed.
#   AUR  RWM_STORE_CAP_UNIFIED=1 RWM_DERIVED_SWEEP=1      THE TREATMENT — the
#                                                         predecessor's
#                                                         unscored halving
#                                                         lead. n = 12/seed.
#   A    (unset)                                          THE ERA PIN ONLY.
#                                                         n = 4/seed, and the
#                                                         pre-registration
#                                                         DISQUALIFIES it as a
#                                                         contrast: at a 0.062
#                                                         base rate its Wilson
#                                                         interval is ~[0,0.37]
#                                                         whatever it reads. It
#                                                         witnesses that the
#                                                         era did not move.
#
# The AUP arm (AU + the pooled ceiling) is NOT here and its absence is a
# DECISION, recorded in the pre-registration before contact: the pooled ceiling
# is bench-only (`Arm::PooledUnified` in `tests/store_cap_sf_bench.rs`; no
# engine gate, deliberately), and RWM_STORE_CAPW cannot stand in for it because
# `capw_terms` read `live_paths()` unconditionally and `capw_store_cap` sits
# ABOVE `path_scaled_store_cap` in the cap chain — so RWM_STORE_CAP_UNIFIED is
# a NO-OP wherever capw engages. That arm would measure P while wearing U's
# name. No engine code is invented in a launch step.
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c8   topo c2/c3 dual  25 MB   THE DECISION CELL — A, AU, AUR
#   c8L  topo c2/c3 dual 200 MB   THE BYTE-COUNT ARM — **AU ONLY**. The
#                                 predecessor put this contrast on A and R
#                                 (written before the arm separation was
#                                 known) and got 0/11 against 1/16, which is
#                                 vacuous: with the control near zero at both
#                                 lengths it cannot discriminate an artifact
#                                 from a mode that never fired. Asking it on
#                                 the arm that fires at 0.727 is the repair.
#
# ── THE PRIMARY STATISTIC ───────────────────────────────────────────────
# NOT goodput (c8's 2-sigma band is 42-46% of its own mean). The per-rep binary
#
#     wait_tun = 0% AND wait_paused = 0%
#
# emitted as the `deadwall` COLUMN by `deadwall_parse.py`, REUSED BYTE-
# IDENTICAL so no second dialect of the statistic can come into existence.
#
# ── INSTRUMENTS on every invocation ─────────────────────────────────────
# RWM_DIAG=1 (the wait histogram — THE statistic — plus win=occ/cap, khr/kraw,
# retx and the [SF] gauge, which is §16.52's own mechanism endpoint and this
# battery's G-SF guard), RWM_ACKDIAG=1 (the ack-cadence gauge, ON in EVERY arm:
# its absence is an INSTRUMENT-FAIL, never a datum), RWM_LATPROBE=1 (the
# delivered-latency probe the G-LATENCY guard is read off), the CPU gauge, and
# the tc -s qdisc capture beside every target (discipline 16).
#
# ── LIVENESS, asserted per arm BEFORE any number is read (discipline 1/15) ─
#   * `[GATES] RWM_STORE_CAP_UNIFIED=` TWO-SIDED on BOTH endpoints — =1 on
#     AU/AUR, =0 on A.
#   * `[GATES] RWM_DERIVED_SWEEP=` TWO-SIDED on BOTH endpoints — =1 on AUR,
#     =0 on A/AU.
#   * `unified store-cap path set ACTIVE` PRESENT on AU/AUR, ABSENT on A.
#   * `derived recovery round ACTIVE` PRESENT on AUR, ABSENT on A/AU. The
#     sender site echoes on the CLIENT log and the receiver site on the SERVER
#     log; only the sender's tail sweep arms on every bulk transfer, so a clean
#     rep can legitimately carry the client echo alone. VOID means BOTH sides
#     silent; a one-sided echo is RECORDED, loudly, and left to scoring.
#   * `derived recovery round DIVERGED` RECORDED on AUR. This — not ACTIVE —
#     is what proves the derived law BOUND: the coincidence property makes it
#     identical to the clamped law wherever 2*srtt already lies inside
#     [25, 100] ms, so an arm with ACTIVE and no DIVERGED is bit-identical to
#     AU and its null is a null RESULT, not a null EFFECT. The
#     pre-registration makes DS-NO-DIVERGENCE on a MAJORITY of AUR reps an
#     instrument falsifier that voids the whole battery.
#   * `[GATES] RWM_RECOV_MP=` RECORDED on every arm — a witness, not an arm.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in deadwall_parse.py (no summary
# at all = ABORT). The seed-7 topo-ping abort class is handled by SYMMETRIC
# top-up sessions only, never asymmetric top-ups.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-12}"
MH_CELLS="${RWM_MH_CELLS:-c8 c8L}"
MH_ARMS="${RWM_MH_ARMS:-A AU AUR}"
TAG="${RWM_MH_TAG:-modehunt}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/modehunt/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/modehunt/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

arm_env() { case "$1" in
  A)   echo "" ;;
  AU)  echo "RWM_STORE_CAP_UNIFIED=1" ;;
  AUR) echo "RWM_STORE_CAP_UNIFIED=1 RWM_DERIVED_SWEEP=1" ;;
esac; }
arm_ds() { case "$1" in AUR) echo 1 ;; *) echo 0 ;; esac; }
arm_u()  { case "$1" in AU|AUR) echo 1 ;; *) echo 0 ;; esac; }

# THE ERA PIN'S OWN n. The pre-registration fixes A at 4/seed and every other
# scored arm at REPS; the cap is applied INSIDE the interleaved loop (not as a
# separate pass) so the pin's reps sit in the same round-robin, on the same
# topologies, as the reps they are a witness for.
arm_reps() { case "$1" in
  A)   echo "${RWM_MH_PINREPS:-4}" ;;
  *)   echo "$REPS" ;;
esac; }

# cell -> "scenA scenB mode bytes"  (identical geometry to deadwall_battery.sh)
cell_spec() {
  case "$1" in
    c8)  echo "c2 c3 dual   25000000" ;;
    c8L) echo "c2 c3 dual  200000000" ;;
    *) echo "" ;;
  esac
}
# c8L carries AU and ONLY AU — the byte-count question asked on the arm that
# fires. Putting A or AUR there would spend reps on a contrast the
# pre-registration does not score.
cell_arms() {
  case "$1" in
    c8)  echo "A AU AUR" ;;
    c8L) echo "AU" ;;
    *)   echo "" ;;
  esac
}

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local eds eu
  eds="$(arm_ds "$arm")"; eu="$(arm_u "$arm")"

  python3 ./deadwall_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "MODEHUNT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the ACTIVE echoes' own prose contains literal
  # `RWM_*=0` strings (the amendment-1 lesson from the flip battery).
  local gdc gds guc gus gmc gms gac gas
  gdc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_DERIVED_SWEEP=[01]")
  gds=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_DERIVED_SWEEP=[01]")
  guc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  gus=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  gmc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_RECOV_MP=[01]")
  gms=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_RECOV_MP=[01]")
  gac=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")
  gas=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_ACKDIAG=[01]")

  local adc ads ddc dds uc us akc aks
  adc=$(grep -c "derived recovery round ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  ads=$(grep -c "derived recovery round ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  ddc=$(grep -c "derived recovery round DIVERGED" /tmp/rwm-c.log 2>/dev/null || true)
  dds=$(grep -c "derived recovery round DIVERGED" /tmp/rwm-s.log 2>/dev/null || true)
  uc=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  us=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  akc=$(grep -c "\[ACKDIAG\]" /tmp/rwm-c.log 2>/dev/null || true)
  aks=$(grep -c "\[ACKDIAG\]" /tmp/rwm-s.log 2>/dev/null || true)

  echo "LIVENESS $name rep=$REP cli=[$gdc $guc $gmc $gac] srv=[$gds $gus $gms $gas] actDS=$adc/$ads divDS=$ddc/$dds actU=$uc/$us ackdiag=$akc/$aks (expect ds=$eds u=$eu)" >> "$OUT"

  # The derived round's own numbers, ONE LINE PER ECHO, tagged by endpoint.
  for _ep in c s; do
    (grep -h "derived recovery round" "/tmp/rwm-$_ep.log" 2>/dev/null \
      | sed 's/\x1b\[[0-9;]*m//g' \
      | sed -E "s/^.*derived recovery round (ACTIVE|DIVERGED).*(site=[^ ]+ srtt_us=[0-9]+ jitter_us=[0-9]+ derived_us=[0-9]+ legacy_us=[0-9]+).*$/DSLINE $name rep=$REP ep=$_ep \1 \2/" \
      | grep "^DSLINE" >> "$OUT") || true
  done

  # No [GATES] on EITHER endpoint = ABORT: no datum, no liveness verdict, and
  # NOT in any denominator.
  if [ -z "$gdc" ] && [ -z "$gds" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  # The two ARM gates, two-sided, both endpoints.
  [ "$gdc" != "RWM_DERIVED_SWEEP=$eds" ] && echo "ARM-LIVENESS-FAIL-DS-CLI $name rep=$REP got='$gdc'" >> "$OUT"
  [ "$gds" != "RWM_DERIVED_SWEEP=$eds" ] && echo "ARM-LIVENESS-FAIL-DS-SRV $name rep=$REP got='$gds'" >> "$OUT"
  [ "$guc" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-CLI $name rep=$REP got='$guc'" >> "$OUT"
  [ "$gus" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-SRV $name rep=$REP got='$gus'" >> "$OUT"

  # RWM_RECOV_MP is a WITNESS, not an arm: recorded every rep, loud if it is
  # not what the whole battery assumes (ON by default).
  [ "$gmc" != "RWM_RECOV_MP=1" ] && echo "RECOVMP-UNEXPECTED-CLI $name rep=$REP got='$gmc'" >> "$OUT"
  [ "$gms" != "RWM_RECOV_MP=1" ] && echo "RECOVMP-UNEXPECTED-SRV $name rep=$REP got='$gms'" >> "$OUT"

  # The instrument must be armed on both endpoints or the columns are void.
  { [ "$gac" != "RWM_ACKDIAG=1" ] || [ "$gas" != "RWM_ACKDIAG=1" ]; } \
    && echo "INSTRUMENT-FAIL-ACKDIAG-GATE $name rep=$REP cli='$gac' srv='$gas'" >> "$OUT"

  # The derived round's ACTIVE echo: PRESENT on AUR, ABSENT elsewhere.
  if [ "$eds" = "1" ]; then
    if [ "$adc" -eq 0 ] && [ "$ads" -eq 0 ]; then
      echo "ARM-LIVENESS-FAIL-DS-ECHO $name rep=$REP (VOID: neither site ran)" >> "$OUT"
    elif [ "$adc" -eq 0 ] || [ "$ads" -eq 0 ]; then
      echo "DS-ECHO-ONE-SIDED $name rep=$REP (cli=$adc srv=$ads — recorded, not void)" >> "$OUT"
    fi
    # Not a failure: the coincidence property permits a bound-free arm. It is
    # the difference between a null RESULT and a null EFFECT, so it is named
    # on the rep rather than inferred later. The pre-registration escalates a
    # MAJORITY of these to an instrument falsifier at scoring time.
    { [ "$ddc" -eq 0 ] && [ "$dds" -eq 0 ]; } \
      && echo "DS-NO-DIVERGENCE $name rep=$REP (derived == clamped at every evaluation)" >> "$OUT"
  else
    { [ "$adc" -gt 0 ] || [ "$ads" -gt 0 ]; } && echo "ARM-CONTAMINATION-DS $name rep=$REP" >> "$OUT"
  fi

  # The unified store-cap echo: PRESENT on AU/AUR, ABSENT on the pin.
  if [ "$eu" = "1" ]; then
    { [ "$uc" -eq 0 ] || [ "$us" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-U-ECHO $name rep=$REP (VOID: cli=$uc srv=$us)" >> "$OUT"
  else
    { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } && echo "ARM-CONTAMINATION-U $name rep=$REP" >> "$OUT"
  fi

  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  # The ack-cadence gauge must actually have reported.
  [ "$akc" -eq 0 ] && echo "INSTRUMENT-FAIL-ACKDIAG $name rep=$REP (no [ACKDIAG] line on the client)" >> "$OUT"
  # THE STATISTIC's own instrument: without steady wait lines the rep has no
  # dead-wall verdict at all, which must be visible as such rather than
  # counting as a clean rep.
  local wn
  wn=$(grep -c "wait\[tun=" /tmp/rwm-c.log 2>/dev/null || true)
  [ "$wn" -eq 0 ] && echo "INSTRUMENT-FAIL-WAIT $name rep=$REP (no wait histogram — no dead-wall verdict)" >> "$OUT"

  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
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
  # arm's log and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

  # The G-LATENCY guard's probe is load-bearing here.
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $MH_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $MH_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  case " $(cell_arms "$1") " in *" $2 "*) ;; *) return 0 ;; esac
  # The era pin's own n, applied INSIDE the interleaved loop.
  [ "$REP" -le "$(arm_reps "$2")" ] || return 0
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== MODE-HUNT BATTERY seed=$SEED_ARG reps=$REPS pinreps=$(arm_reps A) cells='$MH_CELLS' arms='$MH_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"Mode-Hunt Battery — VM PRE-REGISTRATION\" (commit b8fd6d9)"
  echo "=== BASELINE AU (the arm that CARRIES the mode, 8/11 = 0.727 in the predecessor) — NOT the shipped default"
  echo "=== ARMS-DROPPED AUP (pooled ceiling + unified set): NOT gate-expressible, dropped before contact — see the pre-registration"
  echo "=== STATISTIC deadwall = (wait_tun == 0 AND wait_paused == 0), per rep"
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  echo "=== co-tenant $(pgrep -c -x kwin_x11 2>/dev/null || echo 0) kwin_x11 / $(pgrep -c -x sddm 2>/dev/null || echo 0) sddm (desktop session, recorded per era honesty)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPS"); do
  for CELL in $MH_CELLS; do
    for ARM in $MH_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7). The
# expected n is the arm's OWN cap, so the era pin's 4 is not read as 8 missing
# reps.
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $MH_CELLS; do
  for A in $MH_ARMS; do
    case " $(cell_arms "$CELL") " in *" $A "*) ;; *) continue ;; esac
    WANT=$(arm_reps "$A"); [ "$WANT" -gt "$REPS" ] && WANT=$REPS
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$WANT" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "MODEHUNT-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo MODEHUNT-BATTERY-DONE-$SEED_ARG
