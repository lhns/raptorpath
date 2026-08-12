#!/bin/bash
# THE DEAD-WALL BATTERY — the VM battery for goal-gate "The Derived Recovery
# Clamp — VM PRE-REGISTRATION" (own commit 16284c0, written before any VM
# contact). That block is the CONTRACT: it is scored against, never modified,
# and no number in it may change now that the VM has been touched.
#
#   sudo bash deadwall_battery.sh <seed> [reps]
#
# ── ARMS ────────────────────────────────────────────────────────────────
# The pre-registration names the arms {A, D, AU, AUD}. This driver spells
# the two derived-sweep arms R and AUR, which is the same partition under a
# different letter and nothing else:
#
#     ARM-ALIAS  R = D    AUR = AUD
#
# The alias is echoed into every log header so the scoring step maps to the
# contract without inference.
#
#   A    (unset)                                    the shipped stack — control
#   R    RWM_DERIVED_SWEEP=1                        the repair alone
#   AU   RWM_STORE_CAP_UNIFIED=1                    the deeper pool alone
#   AUR  RWM_STORE_CAP_UNIFIED=1 RWM_DERIVED_SWEEP=1  the interaction — the
#                                                   LOAD-BEARING cell (C4)
#
# ── CELLS ───────────────────────────────────────────────────────────────
#   c8   topo c2/c3 dual  25 MB   THE DECISION CELL — all four arms
#   c8L  topo c2/c3 dual 200 MB   THE TRANSFER-LENGTH ARM (C8) — A and R,
#                                 the same cell at 8x the bytes. The dead
#                                 wall is a roughly FIXED number of recovery
#                                 rounds, so its SHARE must fall as 1/duration
#                                 if the "c8 keying" of five sections of the
#                                 ledger is a byte-count artifact.
#   sc2  topo c2/c2 single 100 MB THE GUARD CELL — A and R. The store-cap-
#                                 bound cell: the derived sweep must not
#                                 regress where the clamp does not bind.
#   c1   topo c1/c1 single 400 MB C7's OWN CELLS — A and R. The
#   c7   topo c2/c2 dual   200 MB pre-registration scores C7 ("nothing moves
#                                 where the clamp does not bind") on c1 and
#                                 c7 BY NAME, and names c1 specifically as
#                                 the cell where a law defect would surface
#                                 FIRST: c1's 2*srtt = 18 ms sits BELOW the
#                                 legacy [25, 100] ms band, so it is outside
#                                 the coincidence property's cover. Running
#                                 sc2 alone would leave a pre-registered row
#                                 permanently unscorable for this session,
#                                 so both are carried. sc2 is kept as well —
#                                 it is a third witness at a cell whose bind
#                                 is the store cap, not the clock.
#
# ── THE PRIMARY STATISTIC ───────────────────────────────────────────────
# NOT goodput (c8's 2-sigma band is 42-46% of its own mean — no c8 goodput
# contrast resolves a 5-10% shift at any affordable n). The per-rep binary
#
#     wait_tun = 0% AND wait_paused = 0%
#
# measured 19/19 on the slowest c8 tail and 5/112 elsewhere. It resolves at
# n = 8 per arm and it is emitted as the `deadwall` COLUMN by
# deadwall_parse.py so no downstream step can acquire a second definition.
#
# ── INSTRUMENTS on every invocation ─────────────────────────────────────
# RWM_DIAG=1 (the wait histogram — THE statistic — plus win=occ/cap, khr/kraw,
# retx and the [SF] gauge), RWM_ACKDIAG=1 (the ack-cadence gauge, ON in EVERY
# arm: it is the wait-arm columns' companion and its absence is an
# INSTRUMENT-FAIL, never a datum), RWM_LATPROBE=1 (C5's delivered-latency
# probe — the spurious-vs-backstop arbitration is read off ping_p99), the CPU
# gauge, and the tc -s qdisc capture beside every target (discipline 16).
#
# ── LIVENESS, asserted per arm BEFORE any number is read (discipline 1/15) ─
#   * `[GATES] RWM_DERIVED_SWEEP=` and `RWM_STORE_CAP_UNIFIED=` TWO-SIDED on
#     BOTH endpoints — asserted =1 on the arms that carry them and =0 on the
#     arms that do not, so "gate absent in the control" is as checkable as
#     "gate present in the arm".
#   * `[GATES] RWM_RECOV_MP=` RECORDED on every arm (the component bench's
#     standing warning: any change to the recovery plane's clocks is only
#     safe with the RFC 9002 hole law armed). Not an arm — a witness.
#   * `derived recovery round ACTIVE` PRESENT on R/AUR, ABSENT on A/AU. The
#     gate had NO echo of its own until this battery's instrument commit;
#     `[GATES]` alone proves the env var was READ, not that either site ran.
#     The sender site echoes on the CLIENT log and the receiver site on the
#     SERVER log. Only the sender's tail-sweep arms on every bulk transfer
#     (unACKed symbols always exist); the receiver's hole refresh needs a
#     STALLED HOLE and a path clock, so a clean rep can legitimately carry
#     the client echo alone. VOID therefore means BOTH sides silent; a
#     one-sided echo is RECORDED, loudly, and left to the scoring step.
#   * `derived recovery round DIVERGED` RECORDED on R/AUR. This — not ACTIVE
#     — is what proves the derived law BOUND: the coincidence property makes
#     it identical to the clamped law wherever 2*srtt already lies inside
#     [25, 100] ms, so an arm with ACTIVE and no DIVERGED is bit-identical to
#     its control and its null is a null RESULT, not a null EFFECT.
#   * `unified store-cap path set ACTIVE` PRESENT on AU/AUR, ABSENT on A/R.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in deadwall_parse.py (no
# summary at all = ABORT). The seed-7 topo-ping abort class is handled by
# SYMMETRIC top-up sessions only, never asymmetric top-ups.
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"
DW_CELLS="${RWM_DW_CELLS:-c8 c8L sc2 c1 c7}"
DW_ARMS="${RWM_DW_ARMS:-A R AU AUR}"
TAG="${RWM_DW_TAG:-deadwall}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/deadwall/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/deadwall/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

arm_env() { case "$1" in
  A)   echo "" ;;
  R)   echo "RWM_DERIVED_SWEEP=1" ;;
  AU)  echo "RWM_STORE_CAP_UNIFIED=1" ;;
  AUR) echo "RWM_STORE_CAP_UNIFIED=1 RWM_DERIVED_SWEEP=1" ;;
esac; }
arm_ds() { case "$1" in R|AUR) echo 1 ;; *) echo 0 ;; esac; }
arm_u()  { case "$1" in AU|AUR) echo 1 ;; *) echo 0 ;; esac; }

# cell -> "scenA scenB mode bytes"
cell_spec() {
  case "$1" in
    c8)  echo "c2 c3 dual   25000000" ;;
    c8L) echo "c2 c3 dual  200000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c1)  echo "c1 c1 single 400000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    *) echo "" ;;
  esac
}
# The transfer-length and guard cells carry only the A/R contrast the
# pre-registration scores them on; the four-arm interaction lives at c8.
cell_arms() {
  case "$1" in
    c8)  echo "A R AU AUR" ;;
    *)   echo "A R" ;;
  esac
}

check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local eds eu
  eds="$(arm_ds "$arm")"; eu="$(arm_u "$arm")"

  python3 ./deadwall_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "DEADWALL-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line: the ACTIVE echoes' own prose contains
  # literal `RWM_*=0` strings (the amendment-1 lesson from the flip battery).
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
  # Each echo is one-shot per SITE per PROCESS, so a rep yields up to four
  # (sender/receiver x client/server) at whatever clock each saw first —
  # they are emitted separately rather than flattened, because the spread
  # between the warm-up clock and the steady-state one IS the reading.
  for _ep in c s; do
    (grep -h "derived recovery round" "/tmp/rwm-$_ep.log" 2>/dev/null \
      | sed 's/\x1b\[[0-9;]*m//g' \
      | sed -E "s/^.*derived recovery round (ACTIVE|DIVERGED).*(site=[^ ]+ srtt_us=[0-9]+ jitter_us=[0-9]+ derived_us=[0-9]+ legacy_us=[0-9]+).*$/DSLINE $name rep=$REP ep=$_ep \1 \2/" \
      | grep "^DSLINE" >> "$OUT") || true
  done

  # No [GATES] on EITHER endpoint = ABORT: no datum, no liveness verdict.
  if [ -z "$gdc" ] && [ -z "$gds" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi

  # The two ARM gates, two-sided, both endpoints.
  [ "$gdc" != "RWM_DERIVED_SWEEP=$eds" ] && echo "ARM-LIVENESS-FAIL-DS-CLI $name rep=$REP got='$gdc'" >> "$OUT"
  [ "$gds" != "RWM_DERIVED_SWEEP=$eds" ] && echo "ARM-LIVENESS-FAIL-DS-SRV $name rep=$REP got='$gds'" >> "$OUT"
  [ "$guc" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-CLI $name rep=$REP got='$guc'" >> "$OUT"
  [ "$gus" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-SRV $name rep=$REP got='$gus'" >> "$OUT"

  # RWM_RECOV_MP is a WITNESS, not an arm: recorded on every rep, and loud
  # if it is not what the whole battery assumes (ON by default).
  [ "$gmc" != "RWM_RECOV_MP=1" ] && echo "RECOVMP-UNEXPECTED-CLI $name rep=$REP got='$gmc'" >> "$OUT"
  [ "$gms" != "RWM_RECOV_MP=1" ] && echo "RECOVMP-UNEXPECTED-SRV $name rep=$REP got='$gms'" >> "$OUT"

  # The instrument must be armed on both endpoints or the columns are void.
  { [ "$gac" != "RWM_ACKDIAG=1" ] || [ "$gas" != "RWM_ACKDIAG=1" ]; } \
    && echo "INSTRUMENT-FAIL-ACKDIAG-GATE $name rep=$REP cli='$gac' srv='$gas'" >> "$OUT"

  # The derived round's ACTIVE echo: PRESENT on its arms, ABSENT elsewhere.
  if [ "$eds" = "1" ]; then
    if [ "$adc" -eq 0 ] && [ "$ads" -eq 0 ]; then
      echo "ARM-LIVENESS-FAIL-DS-ECHO $name rep=$REP (VOID: neither site ran)" >> "$OUT"
    elif [ "$adc" -eq 0 ] || [ "$ads" -eq 0 ]; then
      echo "DS-ECHO-ONE-SIDED $name rep=$REP (cli=$adc srv=$ads — recorded, not void)" >> "$OUT"
    fi
    # Not a failure: the coincidence property permits a bound-free arm. It
    # is the difference between a null RESULT and a null EFFECT, so it is
    # named on the rep rather than inferred later.
    { [ "$ddc" -eq 0 ] && [ "$dds" -eq 0 ]; } \
      && echo "DS-NO-DIVERGENCE $name rep=$REP (derived == clamped at every evaluation)" >> "$OUT"
  else
    { [ "$adc" -gt 0 ] || [ "$ads" -gt 0 ]; } && echo "ARM-CONTAMINATION-DS $name rep=$REP" >> "$OUT"
  fi

  # The unified store-cap echo: PRESENT on its arms, ABSENT elsewhere.
  if [ "$eu" = "1" ]; then
    { [ "$uc" -eq 0 ] || [ "$us" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-U-ECHO $name rep=$REP (VOID: cli=$uc srv=$us)" >> "$OUT"
  else
    { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } && echo "ARM-CONTAMINATION-U $name rep=$REP" >> "$OUT"
  fi

  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  # The ack-cadence gauge must actually have reported.
  [ "$akc" -eq 0 ] && echo "INSTRUMENT-FAIL-ACKDIAG $name rep=$REP (no [ACKDIAG] line on the client)" >> "$OUT"
  # THE STATISTIC's own instrument: without steady wait lines the rep has no
  # dead-wall verdict at all, which must be visible as such.
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

  # C5's probe is load-bearing here (the spurious-vs-backstop arbitration).
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $DW_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $DW_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  case " $(cell_arms "$1") " in *" $2 "*) ;; *) return 0 ;; esac
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

{
  echo "=== DEAD-WALL BATTERY seed=$SEED_ARG reps=$REPS cells='$DW_CELLS' arms='$DW_ARMS' $(date -u +%FT%TZ)"
  echo "=== CONTRACT goal-gate \"The Derived Recovery Clamp — VM PRE-REGISTRATION\" (commit 16284c0)"
  echo "=== ARM-ALIAS R=D AUR=AUD (the pre-registration's arm names)"
  echo "=== STATISTIC deadwall = (wait_tun == 0 AND wait_paused == 0), per rep"
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPS"); do
  for CELL in $DW_CELLS; do
    for ARM in $DW_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $DW_CELLS; do
  for A in $DW_ARMS; do
    case " $(cell_arms "$CELL") " in *" $A "*) ;; *) continue ;; esac
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$REPS" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "DEADWALL-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo DEADWALL-BATTERY-DONE-$SEED_ARG
