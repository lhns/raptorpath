#!/bin/bash
# feat/c8-conversion L1 battery (goal-gate "C8 Slow-Path Conversion"
# pre-registration): the conversion fix vs the two incumbent pool laws on the
# shipped default stack.
#
# Arms (RWM_GEN=0 plain bulk, RWM_DIAG=1 everywhere; SR/MP defaults present
# in EVERY arm):
#   legacy = RWM_STORE_PATHS=0     (legacy-1024 pool — the c8 WATCH control)
#   pbs    = env unset             (shipped default: path-scaled N×2048 pool)
#   fix    = $FIX_ENV              (the pre-registered conversion fix, on the
#                                   pool base the diagnosis named)
#
# Cells: c7 (c2+c2 dual, 200 MB), c8 (c2+c3 dual, 25 MB) — verdict cells;
#        sc2 (c2 single, 100 MB), sc3 (c3 single, 25 MB) — same-session Σ
#        terms per arm env + N=1 inertness.
#
# Liveness (MEASUREMENT DISCIPLINE 1/6/7): per-arm expected-echo assertion
# both directions; ARMCOUNT per arm at the end.
#
#   usage: c8conv_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/c8conv/battery-s${SEED_ARG}.log
DDIR=/home/vibe/c8conv/diag
mkdir -p "$DDIR" /home/vibe/c8conv
: > "$OUT"
echo "# c8conv battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

LEGACY="RWM_STORE_PATHS=0"
# The pre-registered fix arm env (set by the pre-registration; see goal-gate).
FIX_ENV="${FIX_ENV:-RWM_PLACE_SLACK=1}"
FIX_ECHO="${FIX_ECHO:-frontier-slack placement ACTIVE}"

run_one() { # name envs cellA cellB mode bytes exp_pbs exp_fix
  local name="$1" envs="$2" ca="$3" cb="$4" mode="$5" bytes="$6"
  local epbs="$7" efix="$8"
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $bytes 1 $mode 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # Liveness (discipline 1/6): every gate's echo must match the arm.
  local sr pbs fix
  sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  pbs=$(grep -c "path-scaled outstanding pool ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
  fix=$(grep -c "$FIX_ECHO" /tmp/rwm-c.log 2>/dev/null || true)
  echo "LIVENESS sr=$sr pbs=$pbs/$epbs fix=$fix/$efix" >> "$OUT"
  if [ "$sr" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-SR $name rep=$REP (SR is the shipped default)" >> "$OUT"; fi
  if [ "$efix" -gt 0 ] && [ "$fix" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-fix $name rep=$REP" >> "$OUT"; fi
  if [ "$efix" -eq 0 ] && [ "$fix" -gt 0 ]; then echo "ARM-CONTAMINATION-fix $name rep=$REP" >> "$OUT"; fi
  if [ "$epbs" -gt 0 ] && [ "$pbs" -eq 0 ]; then echo "ARM-LIVENESS-FAIL-pbs $name rep=$REP" >> "$OUT"; fi
  if [ "$epbs" -eq 0 ] && [ "$pbs" -gt 0 ]; then echo "ARM-CONTAMINATION-pbs $name rep=$REP" >> "$OUT"; fi
  # Occupancy / recovery / conversion gauges.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|srel=[0-9]+/[0-9]+|paused=[0-9.]+%|retx=[0-9]+" \
    | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[C8CONV-S\]' | tail -1 \
    | sed 's/^/SENDER  /' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | grep '\[C8CONV-R\]' | tail -1 \
    | sed 's/^/RECEIVER /' >> "$OUT") || true
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -oE 'mpr\[[^]]*\]' | tail -1 \
    | sed 's/^/MPR /' >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
  #       name        envs                cA cB  mode   bytes      pbs fix
  # -- c8 (the target cell) --
  run_one c8-legacy  "$LEGACY"            c2 c3 dual   25000000    0   0
  run_one c8-pbs     ""                   c2 c3 dual   25000000    1   0
  run_one c8-fix     "$FIX_ENV"           c2 c3 dual   25000000    1   1
  run_one c8-lfix    "$LEGACY $FIX_ENV"   c2 c3 dual   25000000    0   1
  run_one c8-fixlive "$FIX_ENV RWM_RECOV_MP_LIVE=1" c2 c3 dual 25000000 1 1
  # -- c7 (symmetric preservation cell) --
  run_one c7-legacy  "$LEGACY"            c2 c2 dual   200000000   0   0
  run_one c7-pbs     ""                   c2 c2 dual   200000000   1   0
  run_one c7-fix     "$FIX_ENV"           c2 c2 dual   200000000   1   1
  # -- singles (same-session Σ terms per arm env; fix must be N=1-inert).
  # The fix INFO echo prints whenever the gate is CONFIGURED even though the
  # law is N≥2-gated (the c8pool battery's harness-note lesson) — expected
  # echo follows the ENV, inertness is asserted by the goodput itself. --
  run_one sc2-pbs    ""                   c2 c2 single 100000000   0   0
  run_one sc2-fix    "$FIX_ENV"           c2 c2 single 100000000   0   1
  run_one sc3-pbs    ""                   c3 c3 single 25000000    0   0
  run_one sc3-fix    "$FIX_ENV"           c3 c3 single 25000000    0   1
done

# Arm-liveness assertion (discipline 7): an arm with zero summaries fails
# LOUDLY, it does not vanish.
echo "--- ARMCOUNTS (expect $REPS headers per arm)" >> "$OUT"
for a in c8-legacy c8-pbs c8-fix c8-lfix c8-fixlive c7-legacy c7-pbs c7-fix \
         sc2-pbs sc2-fix sc3-pbs sc3-fix; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr" >> "$OUT"
  if [ "$hdr" -eq 0 ]; then echo "ARM-VANISHED $a" >> "$OUT"; fi
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
