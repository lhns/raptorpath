#!/bin/bash
# GOAL "THREE TERMS, NO CONSTANTS" phase 1.4 — the HELD-OUT adversarial cells.
#
# jit25 and shal8 are the two criterion-3 cells the clean netem rig cannot
# express, and they live on `adv_cells.sh`, not `topo.sh` — hence a second
# driver with the same three arms, the same parser (tt_parse.py) and the same
# two-sided liveness assertion as `tt_battery.sh`.
#
#   A  baseline    env unset                        (the shipped default)
#   B  the law     RWM_THREE_TERM=1 RWM_PLAIN_RS=1  (the SCORED arm)
#   C  control     RWM_THREE_TERM=1                 (prices the anchor over-read)
#   D  attribution RWM_PLAIN_RS=1                   (the anchor WITHOUT the law)
#
# The pre-registration's own expectations for these two, recorded here so the
# run cannot be re-interpreted afterwards:
#   jit25  pred 1430/1300 sym (OFF 1024, x1.40), throughput UP +5..+15%. The
#          one cell where K is not a clean read: netem jitter +/-25ms at 25%
#          correlation moves srtt sample to sample and K is a windowed MIN, so
#          the limit should sit near the K=1 column (1300).
#   shal8  pred 455/325 sym (OFF 1024, x0.44), throughput FLAT +/-3% — the
#          law should be INERT because the 8-packet bottleneck binds first.
#          **A null here is the PREDICTED null, not a win.**
#
#   usage: sudo bash tt_adv.sh <seed> [reps]
set -u
[ "$(id -u)" -eq 0 ] || { echo "tt_adv.sh must run as root (sudo)" >&2; exit 2; }
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh   # gate forwarding: RWM_FORWARD / rwm_forward_env
set +e            # lib.sh forces `set -euo pipefail`; this driver runs
                  # WITHOUT -e on purpose (per-arm abort tolerance, item 7)
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/threeterm/adv-s${SEED_ARG}.log
DDIR=/home/vibe/threeterm/diag
mkdir -p "$DDIR" /home/vibe/threeterm
: > "$OUT"
{
  echo "# three-term ADV battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS"
  echo "# binary: $(sha256sum $BIN)"
  echo "# source: $(cat /home/vibe/raptorpath/COMMIT)"
  echo "# kernel: $(uname -r)"
  lscpu | grep "Model name"
  (lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ') || true
  echo
} >> "$OUT"

LAW="RWM_THREE_TERM=1 RWM_PLAIN_RS=1"
CTL="RWM_THREE_TERM=1"
ANC="RWM_PLAIN_RS=1"

# Arm D = the honest anchor WITHOUT the law: the attribution control that
# splits a movement at arm B between the LAW and the ANCHOR the law needs.
# Not scored, changes no verdict — it only decides which of the two owns a
# result. Attribution, not tuning.
arm_env() { case "$1" in A) echo "" ;; B) echo "$LAW" ;; C) echo "$CTL" ;; D) echo "$ANC" ;; esac; }
arm_3t()  { case "$1" in B|C) echo 1 ;; *) echo 0 ;; esac; }
arm_rs()  { case "$1" in B|D) echo 1 ;; *) echo 0 ;; esac; }

# Per-cell transfer size. jit25 delivers ~50 Mbit/s, so 50 MB is ~8 s of
# steady state; shal8's 8-packet bottleneck delivers ~6 Mbit/s, where 50 MB
# would cost ~68 s per invocation for no extra information — 25 MB there.
cell_bytes() { case "$1" in shal8) echo 25000000 ;; *) echo 50000000 ;; esac; }

run_one() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs e3t ers BYTES
  envs="$(arm_env "$arm")"; e3t="$(arm_3t "$arm")"; ers="$(arm_rs "$arm")"
  BYTES="$(cell_bytes "$cell")"
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$BYTES $(date -u +%T)" >> "$OUT"
  pkill -x raptorpath 2>/dev/null || true
  rm -f /tmp/tt-c.log /tmp/tt-s.log

  # Seed-7 topo-ping double-abort protocol (discipline 8): retry once, record
  # the aborted invocation LOUDLY, contribute NO datum.
  if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
    echo "TOPO-PING abort $name rep=$REP — retrying once" >> "$OUT"
    if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
      echo "TOPO-ABORT $name rep=$REP (double abort, invocation dropped)" >> "$OUT"
      bash ./adv_cells.sh down >/dev/null 2>&1 || true
      return 0
    fi
  fi

  # shellcheck disable=SC2086
  ip netns exec rp-srv env $(rwm_forward_env) RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=180 $envs \
      "$BIN" perf --server --bind 10.77.0.2:7000 \
      --window-reliable --protocol-hint bulk >/tmp/tt-s.log 2>&1 &
  local up=0
  for _ in $(seq 1 20); do
    ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000' && { up=1; break; }
    sleep 0.3
  done
  if [ "$up" -eq 0 ]; then
    echo "SRV-BIND-FAIL $name rep=$REP (invocation dropped)" >> "$OUT"
    pkill -x raptorpath 2>/dev/null || true
    bash ./adv_cells.sh down >/dev/null 2>&1 || true
    return 0
  fi
  sleep 1
  # shellcheck disable=SC2086
  timeout 400 ip netns exec rp-cli env $(rwm_forward_env) RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=180 $envs \
      "$BIN" perf --client --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
      --window-reliable --protocol-hint bulk \
      --bytes "$BYTES" --runs 1 >/tmp/tt-c.log 2>&1
  local rc=$?
  [ "$rc" -ne 0 ] && echo "CLIENT-RC $name rep=$REP rc=$rc" >> "$OUT"
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  bash ./adv_cells.sh counters > "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null || true

  python3 ./tt_parse.py "$cell" "$arm" "$SEED_ARG" "$REP" /tmp/tt-c.log /tmp/tt-s.log \
    >> "$OUT" 2>&1 || echo "TTRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"

  # ── LIVENESS, TWO-SIDED on BOTH logs (discipline 15c) ──────────────────
  local g3c g3s grc grs act eng
  # Scoped to `[GATES]`: the resolve-time ACTIVE echo's own PROSE contains the
  # literal `RWM_THREE_TERM=0 = the shipped-default control arm)`, so an
  # unscoped grep reads the documentation instead of the resolved value (the
  # pre-battery smoke caught exactly this — see tt_battery.sh).
  g3c=$(grep "\[GATES\]" /tmp/tt-c.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  g3s=$(grep "\[GATES\]" /tmp/tt-s.log 2>/dev/null | tail -1 | grep -o "RWM_THREE_TERM=[01]")
  grc=$(grep "\[GATES\]" /tmp/tt-c.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  grs=$(grep "\[GATES\]" /tmp/tt-s.log 2>/dev/null | tail -1 | grep -o "RWM_PLAIN_RS=[01]")
  act=$(grep -c "three-term outstanding limit ACTIVE" /tmp/tt-c.log 2>/dev/null || true)
  eng=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/tt-c.log 2>/dev/null | grep '\[3T\]' | grep -c "eng=1" || true)
  echo "LIVENESS $name rep=$REP cli=[$g3c $grc] srv=[$g3s $grs] active=$act eng1_lines=$eng (expect 3t=$e3t rs=$ers)" >> "$OUT"
  [ "$g3c" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-CLI $name rep=$REP got='$g3c'" >> "$OUT"
  [ "$g3s" != "RWM_THREE_TERM=$e3t" ] && echo "ARM-LIVENESS-FAIL-GATE-SRV $name rep=$REP got='$g3s'" >> "$OUT"
  [ "$grc" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-CLI $name rep=$REP got='$grc'" >> "$OUT"
  [ "$grs" != "RWM_PLAIN_RS=$ers" ] && echo "ARM-LIVENESS-FAIL-RS-SRV $name rep=$REP got='$grs'" >> "$OUT"
  if [ "$e3t" = "1" ]; then
    [ "$act" -eq 0 ] && echo "ARM-LIVENESS-FAIL-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -eq 0 ] && echo "ARM-LIVENESS-FAIL-3T $name rep=$REP (VOID: no eng=1)" >> "$OUT"
  else
    [ "$act" -gt 0 ] && echo "ARM-CONTAMINATION-ACTIVE $name rep=$REP" >> "$OUT"
    [ "$eng" -gt 0 ] && echo "ARM-CONTAMINATION-3T $name rep=$REP" >> "$OUT"
  fi
  (sed 's/\x1b\[[0-9;]*m//g' /tmp/tt-c.log | grep '\[3T\]' | grep 'eng=1' \
    | grep -oE "eng=[01] cap=[0-9]+ window=[0-9.]+ slack=[0-9.]+ span=[0-9.]+ rho=[0-9.]+ b=[0-9.]+" \
    | sed -n '1p;$p' | sed "s/^/3TLINE $name rep=$REP /" >> "$OUT") || true

  cp /tmp/tt-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/tt-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  pkill -x raptorpath 2>/dev/null || true
  bash ./adv_cells.sh down >/dev/null 2>&1 || true
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

for REP in $(seq 1 "$REPS"); do
  run_one jit25 A; run_one jit25 B; run_one jit25 C; run_one jit25 D
  run_one shal8 A; run_one shal8 B; run_one shal8 C; run_one shal8 D
done

echo "--- ARMCOUNTS (expect $REPS per arm)" >> "$OUT"
for c in jit25 shal8; do
  for a in A B C D; do
    hdr=$(grep -c "arm=$c-$a " "$OUT" || true)
    res=$(grep -c "\"cell\": \"$c\", \"arm\": \"$a\"" "$OUT" || true)
    echo "ARMCOUNT $c-$a headers=$hdr results=$res" >> "$OUT"
    [ "$res" -eq 0 ] && echo "ARM-VANISHED $c-$a" >> "$OUT"
  done
done
echo "ADV-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo ADV-DONE-$SEED_ARG
