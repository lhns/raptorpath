#!/bin/bash
# goal-gate "Store-Cap Triplication" — P0, THE PRE-BATTERY SMOKE.
#
# MEASUREMENT DISCIPLINE 14(d) in executable form: the pre-registration says
# the battery is NOT RUN unless the saturation-filter POPULATION is live at
# L1. This measures it, at the real cells, before anything else runs.
#
# The instrument is the `[SF]` INFO line (net/mod.rs, RWM_DIAG-gated,
# behaviour-inert): at each dyn-cap refresh tick it reports the cumulative
#
#   ticks / live_sum / active_sum / short_ticks / zero_ticks
#
# where short = active_paths() returned FEWER paths than live_paths(), and
# zero = it returned NONE while paths were live (⇒ the shipped pooled cap
# falls out to store_boot_cap = 128). E = active_sum/live_sum is the anchor-
# mass retention the pre-registration's P1 is sized on.
#
# One rep per cell per arm — this is SHAKEOUT EVIDENCE, not battery evidence,
# and it is recorded as such. Both arms are run so that P1's "the sf= gauge
# itself must NOT move" clause is checkable from the smoke alone.
#
#   usage: bash storecap_smoke.sh [seed]
set -uo pipefail
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="${1:-42}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/storecap/smoke-s${SEED_ARG}.log
DDIR=/home/vibe/storecap/diag-smoke
mkdir -p "$DDIR" /home/vibe/storecap
: >> "$OUT"
{
  echo "# storecap SMOKE $(date -u +%FT%TZ) seed=$SEED_ARG"
  echo "# binary: $(sha256sum $BIN)"
  echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)"
  lscpu | grep "Model name"
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ') || true
  echo
  uname -r
} >> "$OUT"

wait_clear() {
  for _ in $(seq 1 30); do
    if ! pgrep -x raptorpath >/dev/null 2>&1 \
       && ! sudo ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000'; then
      return 0
    fi
    sudo pkill -x raptorpath 2>/dev/null || true
    sleep 1
  done
  return 0
}

smoke_one() { # label envs cellA cellB bytes mode e_uni
  local label="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" euni="$7"
  wait_clear
  sudo rm -f /tmp/rwm-c.log /tmp/rwm-s.log
  echo "=== SMOKE $label seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
  sudo env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
    bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
    | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
  # THE GAUGE: last cumulative [SF] sample of the run, client side (the
  # sender is where the dyn-cap phase lives).
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -a '\[SF\]' | tail -1 \
    | sed 's/^/SF /' >> "$OUT"; echo >> "$OUT") || true
  # Liveness echo, both directions (MEASUREMENT DISCIPLINE 1 + 6).
  local uc us
  uc=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null | grep -ac "unified store-cap path set ACTIVE" || true)
  us=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log 2>/dev/null | grep -ac "unified store-cap path set ACTIVE" || true)
  echo "LIVENESS $label uni_c=$uc uni_s=$us (want $euni)" >> "$OUT"
  # The store-cap gauges the DIAG line already carries.
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep -a '\[DIAG\]' | tail -1 \
    | grep -oE "win=[0-9]+/[0-9]+|store=[0-9]+/[0-9]+|cap=[0-9]+|paused=[0-9.]+%|rtt=[0-9.]+ms|cwnd=[0-9]+|btlbw=[0-9]+|retx=[0-9]+|sweeps=[0-9]+" \
    | tr '\n' ' ' | sed 's/^/MECH /' >> "$OUT"; echo >> "$OUT") || true
  cp /tmp/rwm-c.log "$DDIR/smoke-${label}-s${SEED_ARG}-c.log" 2>/dev/null || true
  cp /tmp/rwm-s.log "$DDIR/smoke-${label}-s${SEED_ARG}-s.log" 2>/dev/null || true
}

E_UNI="RWM_STORE_CAP_UNIFIED=1"

smoke_one c7-def  ""       c2 c2 200000000 dual   0
smoke_one c7-uni  "$E_UNI" c2 c2 200000000 dual   1
smoke_one c8-def  ""       c2 c3  25000000 dual   0
smoke_one c8-uni  "$E_UNI" c2 c3  25000000 dual   1
smoke_one sc2-def ""       c2 c2 100000000 single 0
smoke_one sc2-uni "$E_UNI" c2 c2 100000000 single 1
smoke_one sc3-def ""       c3 c3  25000000 single 0
smoke_one sc3-uni "$E_UNI" c3 c3  25000000 single 1
smoke_one c1-def  ""       c1 c1 400000000 single 0
smoke_one c1-uni  "$E_UNI" c1 c1 400000000 single 1

echo "# smoke done $(date -u +%FT%TZ)" >> "$OUT"
