#!/bin/bash
# GOAL "HONEST INPUTS" final open item — the RWM_STORE_CAP_UNIFIED
# ATTRIBUTION + FLIP battery (goal-gate "Store-Cap Unification —
# ATTRIBUTION + FLIP BATTERY — PRE-REGISTRATION" is the contract; it is
# committed BEFORE any run and scored against, never modified).
#
#   sudo bash uni_battery.sh <seed> [reps] [c1_reps]
#
# THE QUESTION (the flip battery's named missing measurement): the OLD
# store-cap battery measured A+U removing a needed brake at c8 (−19.6%,
# s7, on the legacy-fold default); the flip battery's F5 measured c8
# CLEAN under the honest anchor. Attribute it: does the harm live in the
# LEGACY FOLD's era (ALU) and vanish under the honest anchor (AU)?
#
# FIVE ARMS, same-session interleaved per cell per rep (discipline 3),
# ALL on the new default stack (RWM_HONEST_ANCHOR default ON since
# 9f6e56b):
#
#   A    (unset)                               the shipped default (HA=1)
#   AU   RWM_STORE_CAP_UNIFIED=1               the flip candidate
#   AL   RWM_HONEST_ANCHOR=0                   legacy-fold control (the OLD
#                                              battery's baseline, pinned)
#   ALU  RWM_HONEST_ANCHOR=0 + U=1             reproduces the OLD A+U arm —
#                                              must land in the −19.6%
#                                              class at c8 or the era moved
#   RU   RWM_PLAIN_RS=1 + U=1  (c1 ONLY)       the goal's criterion-1
#                                              reader: the honest-rate arm
#                                              on the would-be new default,
#                                              scored ONLY vs AU parity
#
# CELLS (c1 at n=12 — sized to the measured sigma=33.1 class; rest n=8):
#   c1   topo c1/c1 single 400 MB   the payoff cell (U's banked +15.8/+24.8%)
#   sc2  topo c2/c2 single 100 MB   parity + latency survival (F6 analog)
#   c7   topo c2/c2 dual 200 MB     U's other known effect surface; parity floor
#   c8   topo c2/c3 dual 25 MB      THE DECISION CELL (the −19.6% brake harm)
#
# INSTRUMENTS on every invocation: the `CPU: CPUSRV=/CPUCLI=` gauge
# (absent = INSTRUMENT-FAIL, loud), RWM_DIAG=1 (the `[SF]` filter gauge +
# the capboot consumed-cliff gauge via win=occ/cap, wait[], retx),
# tc -s qdisc capture (discipline 16 same-session headroom), RWM_LATPROBE=1.
#
# LIVENESS, asserted per arm and per direction BEFORE any number is read
# (discipline 1/15): all FIVE gates two-sided on the `[GATES]` line of
# BOTH endpoints; the honest-anchor ACTIVE echo PRESENT exactly on
# A/AU/RU and ABSENT on AL/ALU; the `unified store-cap path set ACTIVE`
# echo PRESENT exactly on AU/ALU/RU; no 3T/HK echo anywhere. An arm whose
# echo set is wrong is VOID and re-run symmetrically, not explained.
#
# ABORT != DNF != INSTRUMENT-FAIL, as encoded in flip_parse.py (reused
# byte-identical so numbers pool across sessions without a second
# dialect; the seed-7 topo-ping abort class is handled by SYMMETRIC
# top-up sessions only).
set -uo pipefail
[ "$(id -u)" -eq 0 ] || { echo "must be root"; exit 1; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
set +e            # per-arm abort tolerance (discipline 7)

SEED_ARG="${1:?seed}"; REPS="${2:-8}"; C1REPS="${3:-12}"
UNI_CELLS="${RWM_UNI_CELLS:-c1 sc2 c7 c8}"
UNI_ARMS="${RWM_UNI_ARMS:-A AU AL ALU RU}"
TAG="${RWM_UNI_TAG:-uni}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT="/home/vibe/uniflip/${TAG}-s${SEED_ARG}.log"
DDIR="/home/vibe/uniflip/diag"
mkdir -p "$(dirname "$OUT")" "$DDIR"

arm_env() { case "$1" in
  A)   echo "" ;;
  AU)  echo "RWM_STORE_CAP_UNIFIED=1" ;;
  AL)  echo "RWM_HONEST_ANCHOR=0" ;;
  ALU) echo "RWM_HONEST_ANCHOR=0 RWM_STORE_CAP_UNIFIED=1" ;;
  RU)  echo "RWM_PLAIN_RS=1 RWM_STORE_CAP_UNIFIED=1" ;;
esac; }
# Expected RESOLVED [GATES] values per arm (HA defaults ON in this era).
arm_3t() { echo 0; }
arm_rs() { case "$1" in RU) echo 1 ;; *) echo 0 ;; esac; }
arm_ha() { case "$1" in AL|ALU) echo 0 ;; *) echo 1 ;; esac; }
arm_hk() { echo 0; }
arm_u()  { case "$1" in AU|ALU|RU) echo 1 ;; *) echo 0 ;; esac; }
# RU is the criterion-1 reader: c1 only.
arm_cells() { case "$1" in RU) echo "c1" ;; *) echo "$UNI_CELLS" ;; esac; }

# topo cells -> "scenA scenB mode bytes"
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual 200000000" ;;
    c8)  echo "c2 c3 dual 25000000" ;;
    *) echo "" ;;
  esac
}
cell_reps() { case "$1" in c1) echo "$C1REPS" ;; *) echo "$REPS" ;; esac; }

# ── LIVENESS + PARSE (flip_battery.sh's gate, arms re-pointed) ───────────
check_and_parse() {
  local name="$1" cell="$2" arm="$3" cpus="$4" cpuc="$5" pingp="$6" qp="$7"
  local e3t ers eha ehk eu
  e3t="$(arm_3t "$arm")"; ers="$(arm_rs "$arm")"
  eha="$(arm_ha "$arm")"; ehk="$(arm_hk "$arm")"; eu="$(arm_u "$arm")"

  python3 ./flip_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" \
      /tmp/rwm-c.log /tmp/rwm-s.log "$cpus" "$cpuc" "$pingp" "$qp" \
    >> "$OUT" 2>&1 || echo "FLIPRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # Scoped to the [GATES] line (the amendment-1 lesson: the ACTIVE echoes'
  # own prose contains literal `RWM_*=0` strings).
  local g3c g3s grc grs ghc ghs gkc gks guc gus a3 hac has hkc hks uc us eng
  g3c=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  g3s=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  grc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  grs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  ghc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_ANCHOR=[01]")
  ghs=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_ANCHOR=[01]")
  gkc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_K=[01]")
  gks=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_HONEST_K=[01]")
  guc=$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  gus=$(grep "\[GATES\]" /tmp/rwm-s.log 2>/dev/null | tail -1 | grep -o "RWM_STORE_CAP_UNIFIED=[01]")
  a3=$(grep -c "three-term outstanding limit ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  hac=$(grep -c "windowed-max rate filter ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  has=$(grep -c "windowed-max rate filter ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  hkc=$(grep -c "echo-ratio floor ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  hks=$(grep -c "echo-ratio floor ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  uc=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  us=$(grep -c "unified store-cap path set ACTIVE" /tmp/rwm-s.log 2>/dev/null || true)
  eng=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep '\[3T\]' | grep -c "eng=1" || true)
  echo "LIVENESS $name rep=$REP cli=[$g3c $grc $ghc $gkc $guc] srv=[$g3s $grs $ghs $gks $gus] act3=$a3 actHA=$hac/$has actHK=$hkc/$hks actU=$uc/$us eng1=$eng (expect 3t=$e3t rs=$ers ha=$eha hk=$ehk u=$eu)" >> "$OUT"

  # An invocation with no [GATES] on either endpoint is an ABORT: no datum,
  # no liveness verdict (flip_parse.py records it; the report excludes it).
  if [ -z "$g3c" ] && [ -z "$g3s" ]; then
    echo "ABORT $name rep=$REP (no [GATES] on either endpoint)" >> "$OUT"
    return 0
  fi
  [ "$g3c" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-CLI $name rep=$REP got='$g3c'" >> "$OUT"
  [ "$g3s" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-SRV $name rep=$REP got='$g3s'" >> "$OUT"
  [ "$grc" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-CLI $name rep=$REP got='$grc'" >> "$OUT"
  [ "$grs" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-SRV $name rep=$REP got='$grs'" >> "$OUT"
  [ "$ghc" != "RWM_HONEST_ANCHOR=$eha" ] && echo "ARM-LIVENESS-FAIL-HA-CLI $name rep=$REP got='$ghc'" >> "$OUT"
  [ "$ghs" != "RWM_HONEST_ANCHOR=$eha" ] && echo "ARM-LIVENESS-FAIL-HA-SRV $name rep=$REP got='$ghs'" >> "$OUT"
  [ "$gkc" != "RWM_HONEST_K=$ehk" ] && echo "ARM-LIVENESS-FAIL-HK-CLI $name rep=$REP got='$gkc'" >> "$OUT"
  [ "$gks" != "RWM_HONEST_K=$ehk" ] && echo "ARM-LIVENESS-FAIL-HK-SRV $name rep=$REP got='$gks'" >> "$OUT"
  [ "$guc" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-CLI $name rep=$REP got='$guc'" >> "$OUT"
  [ "$gus" != "RWM_STORE_CAP_UNIFIED=$eu" ] && echo "ARM-LIVENESS-FAIL-U-SRV $name rep=$REP got='$gus'" >> "$OUT"
  # The ACTIVE echoes: PRESENT on their arms, ABSENT elsewhere. The HA echo
  # follows the RESOLVED gate (default ON in this era), so A/AU/RU carry it
  # and the AL/ALU legacy-fold arms must NOT.
  if [ "$eha" = "1" ]; then
    { [ "$hac" -eq 0 ] || [ "$has" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-HA-ECHO $name rep=$REP (VOID: cli=$hac srv=$has)" >> "$OUT"
  else
    { [ "$hac" -gt 0 ] || [ "$has" -gt 0 ]; } && echo "ARM-CONTAMINATION-HA $name rep=$REP" >> "$OUT"
  fi
  { [ "$hkc" -gt 0 ] || [ "$hks" -gt 0 ]; } && echo "ARM-CONTAMINATION-HK $name rep=$REP" >> "$OUT"
  if [ "$eu" = "1" ]; then
    { [ "$uc" -eq 0 ] || [ "$us" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL-U-ECHO $name rep=$REP (VOID: cli=$uc srv=$us)" >> "$OUT"
  else
    { [ "$uc" -gt 0 ] || [ "$us" -gt 0 ]; } && echo "ARM-CONTAMINATION-U $name rep=$REP" >> "$OUT"
  fi
  [ "$a3" -gt 0 ] && echo "ARM-CONTAMINATION-ACTIVE $name rep=$REP" >> "$OUT"
  [ "$eng" -gt 0 ] && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
  # The CPU gauge is the mechanism-hypothesis gauge: absent = INSTRUMENT-FAIL.
  [ -z "$cpuc" ] && echo "INSTRUMENT-FAIL-CPU $name rep=$REP" >> "$OUT"
  # The [SF] gauge must be present on every live invocation.
  local sfn
  sfn=$(grep -c "\[SF\]" /tmp/rwm-c.log 2>/dev/null || true)
  [ "$sfn" -eq 0 ] && echo "INSTRUMENT-FAIL-SF $name rep=$REP" >> "$OUT"

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
  # arm's log/gauges and pass its liveness gate.
  rm -f /tmp/rwm-c.log /tmp/rwm-s.log /tmp/rwm-perf-out.txt

  # shellcheck disable=SC2086
  env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 RWM_LATPROBE=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | tee /tmp/rwm-perf-out.txt \
    | grep -E "summary|\"dnf\"|CPU:|GUARD|QDISC|QCAP|LATPROBE" >> "$OUT" || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  local cpus cpuc
  cpus=$(grep -oP 'CPUSRV=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)
  cpuc=$(grep -oP 'CPUCLI=\K[0-9.]+' /tmp/rwm-perf-out.txt | tail -1)

  check_and_parse "$name" "$cell" "$arm" "$cpus" "$cpuc" /tmp/rwm-ping.txt /tmp/rwm-q.txt

  # Probe liveness (the cells carrying latency claims).
  local pn; pn=$(grep -c "time=" /tmp/rwm-ping.txt 2>/dev/null || true)
  { [ -n "$(grep "\[GATES\]" /tmp/rwm-c.log 2>/dev/null)" ] && [ "$pn" -eq 0 ]; } \
    && echo "INSTRUMENT-FAIL-PROBE $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null \
    || echo "QCAP-MISSING $name rep=$REP" >> "$OUT"
  cp /tmp/rwm-ping.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-p.txt" 2>/dev/null || true
}

run_one() { # cell arm
  case " $UNI_CELLS " in *" $1 "*) ;; *) return 0 ;; esac
  case " $UNI_ARMS " in *" $2 "*) ;; *) return 0 ;; esac
  case " $(arm_cells "$2") " in *" $1 "*) ;; *) return 0 ;; esac
  # c1 runs to C1REPS; the rest stop at REPS.
  [ "$REP" -gt "$(cell_reps "$1")" ] && return 0
  run_topo "$1" "$2"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

REPMAX="$REPS"; [ "$C1REPS" -gt "$REPMAX" ] && REPMAX="$C1REPS"

{
  echo "=== UNI ATTRIBUTION+FLIP BATTERY seed=$SEED_ARG reps=$REPS c1reps=$C1REPS cells='$UNI_CELLS' arms='$UNI_ARMS' $(date -u +%FT%TZ)"
  echo "=== binary sha256 $(sha256sum $BIN | cut -d' ' -f1)"
  echo "=== source $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
  echo "=== kernel $(uname -r)"
  echo "=== uptime/load $(uptime)"
  lscpu | grep -E 'Model name' | head -1
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' '; echo) || true
} >> "$OUT"

for REP in $(seq 1 "$REPMAX"); do
  for CELL in $UNI_CELLS; do
    for ARM in $UNI_ARMS; do
      run_one "$CELL" "$ARM"
    done
  done
done

# Per-arm tally: an arm that VANISHED must fail loudly (discipline 7).
echo "=== ARMCOUNTS $(date -u +%FT%TZ)" >> "$OUT"
for CELL in $UNI_CELLS; do
  for A in $UNI_ARMS; do
    case " $(arm_cells "$A") " in *" $CELL "*) ;; *) continue ;; esac
    N=$(grep -c "\"cell\": \"$CELL\", \"arm\": \"$A\"" "$OUT" || true)
    echo "ARMCOUNT $CELL-$A n=$N/$(cell_reps "$CELL")" >> "$OUT"
    [ "$N" -eq 0 ] && echo "ARM-VANISHED $CELL-$A" >> "$OUT"
  done
done
echo "UNI-BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo UNI-BATTERY-DONE-$SEED_ARG
